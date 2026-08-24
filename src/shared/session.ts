/**
 * `sessions` sozlesmesi — Rust `SessionRecord`'un tip aynasi (ASU-030).
 *
 * Tek kaynak ve senkron testi icin bkz. `src/shared/memory.ts` bas yorumu;
 * ayni `.sql` dosyasi ve ayni `schema-mirror.spec.ts` bu tipi de baglar.
 *
 * Katman ayrimi (PROJECT.md Bolum 14): bu tablo **ham transcript degildir**.
 * Transcript en fazla bir dosya yolu olarak burada gorunur; oturum ozeti ve
 * durable memory ayri seylerdir.
 */

import { ContractError, assertNoUnexpectedKeys, isRecord, readers } from './contract';

export interface SessionRecord {
  readonly id: number;
  readonly startedAt: string;
  /** `null` = oturum hala acik (ya da temiz kapanamadi). */
  readonly endedAt: string | null;
  readonly projectId: string | null;
  /** `null` = ozet uretilmedi. Ozet basarisiz olsa da oturum kapanir. */
  readonly summary: string | null;
  /**
   * `null` = transcript diske yazilmadi (`ASUNA_TRANSCRIPT_STORAGE=false`).
   *
   * Yol kullaniciya gosterilir: kendi makinesindeki kendi dosyasini
   * bulabilmeli ve silebilmeli (PROJECT.md Bolum 20 incelenebilirlik).
   */
  readonly transcriptPath: string | null;
  readonly model: string;
  readonly inputTokens: number | null;
  readonly outputTokens: number | null;
  readonly totalTokens: number | null;
  readonly estimatedCostUsd: number | null;
  /**
   * Ham `Usage` kirilimi. Anahtarlar runtime'da dogrulanmadi (memory.md T5);
   * netlestiginde ASU-032 yeni bir migration ile kolon acabilir.
   */
  readonly usageJson: string | null;
  readonly createdAt: string;
}

/** Sozlesmedeki alanlarin tam listesi — sema kolon sirasiyla ayni. */
export const SESSION_RECORD_KEYS = [
  'id',
  'startedAt',
  'endedAt',
  'projectId',
  'summary',
  'transcriptPath',
  'model',
  'inputTokens',
  'outputTokens',
  'totalTokens',
  'estimatedCostUsd',
  'usageJson',
  'createdAt',
] as const;

export class SessionContractError extends ContractError {
  public override readonly name = 'SessionContractError';
}

function fail(field: string, expected: string): never {
  throw new SessionContractError(`\`${field}\` ${expected} olmali.`);
}

function failWith(message: string): never {
  throw new SessionContractError(message);
}

export function parseSessionRecord(value: unknown): SessionRecord {
  if (!isRecord(value)) {
    throw new SessionContractError('Oturum kaydi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, SESSION_RECORD_KEYS, failWith);

  const read = readers(value, fail);

  const startedAt = read.timestamp('startedAt');
  const endedAt = read.nullableTimestamp('endedAt');
  // Semadaki CHECK ile ayni kural; bozuk bir kayit UI'da negatif sure
  // gostermek yerine gurultulu bir hata uretsin.
  if (endedAt !== null && endedAt < startedAt) {
    fail('endedAt', 'baslangictan sonra bir zaman');
  }

  return {
    id: read.id('id'),
    startedAt,
    endedAt,
    projectId: read.nullableText('projectId'),
    summary: read.nullableText('summary'),
    transcriptPath: read.nullableText('transcriptPath'),
    model: read.text('model'),
    inputTokens: read.nullableCount('inputTokens'),
    outputTokens: read.nullableCount('outputTokens'),
    totalTokens: read.nullableCount('totalTokens'),
    estimatedCostUsd: read.nullableAmount('estimatedCostUsd'),
    usageJson: read.nullableJsonText('usageJson'),
    createdAt: read.timestamp('createdAt'),
  };
}

export function parseSessionRecords(value: unknown): SessionRecord[] {
  if (!Array.isArray(value)) {
    throw new SessionContractError('Oturum listesi bir dizi olmali.');
  }
  return value.map(parseSessionRecord);
}

/** Oturum hala acik mi? Yarim kalan oturumlar acilista kapatilir (ASU-032). */
export function isSessionOpen(session: SessionRecord): boolean {
  return session.endedAt === null;
}

// ---------------------------------------------------------------------------
// Istek sozlesmeleri (ASU-032)
// ---------------------------------------------------------------------------

/** Dokum satiri. Yalnizca `ASUNA_TRANSCRIPT_STORAGE=true` iken diske yazilir. */
export interface TranscriptLineInput {
  readonly role: 'user' | 'assistant';
  readonly text: string;
  /** Verilmezse dosyaya da yazilmaz — zaman uydurulmaz. */
  readonly at?: string;
}

/**
 * Oturum kapanisinda raporlanan kullanim.
 *
 * Skalerler `sessions` kolonlarina, tamami `usageJson`'a yazilir. Ayrintili
 * kirilimin anahtarlari runtime'da dogrulanmadi (memory.md T5) — bu yuzden
 * uydurulmus kolon acilmadi.
 */
export interface SessionUsageInput {
  readonly requests?: number;
  readonly inputTokens?: number;
  readonly outputTokens?: number;
  readonly totalTokens?: number;
  readonly inputTokenDetails?: readonly Readonly<Record<string, number>>[];
  readonly outputTokenDetails?: readonly Readonly<Record<string, number>>[];
}

export interface SessionFinalizeInput {
  readonly usage?: SessionUsageInput;
  readonly transcript?: readonly TranscriptLineInput[];
}

/**
 * Oturum yazma sonucu. `skipped` = hafiza kapali; oturum kaydi hic olusmadi ve
 * cagiran taraf elinde bir oturum kimligi oldugunu **sanmamali**.
 */
export type SessionWriteResult =
  | { readonly status: 'recorded'; readonly session: SessionRecord }
  | { readonly status: 'skipped'; readonly reason: 'memory-disabled' };

const SESSION_SKIP_REASONS = ['memory-disabled'] as const;

export function parseSessionWriteResult(value: unknown): SessionWriteResult {
  if (!isRecord(value)) {
    throw new SessionContractError('Oturum yazma sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);

  switch (value['status']) {
    case 'recorded':
      assertNoUnexpectedKeys(value, ['status', 'session'], failWith);
      return { status: 'recorded', session: parseSessionRecord(value['session']) };

    case 'skipped':
      assertNoUnexpectedKeys(value, ['status', 'reason'], failWith);
      return { status: 'skipped', reason: read.enumeration('reason', SESSION_SKIP_REASONS) };

    default:
      fail('status', 'su degerlerden biri: recorded, skipped');
  }
}

/**
 * Oturum suresi (ms) — kapanmamis oturumda `null`.
 *
 * Yarim kalan bir oturum acilista `endedAt = startedAt` ile kapatilir; bu
 * durumda sure `0` doner. Bu bilincli: gercek bitis zamani bilinmiyor ve
 * "simdi" yazmak saatler suren sahte bir oturum uretirdi (ASU-032).
 */
export function sessionDurationMs(session: SessionRecord): number | null {
  if (session.endedAt === null) {
    return null;
  }
  return Date.parse(session.endedAt) - Date.parse(session.startedAt);
}

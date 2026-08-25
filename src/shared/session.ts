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

/**
 * Oturumun **nasil** kapandigi (ASU-033, migration 002).
 *
 * Degerler semadaki CHECK kisitindan gelir (`002_session_end_reason.up.sql`) ve
 * `schema-mirror.spec.ts` ikisini birbirine baglar.
 */
export const SESSION_END_REASONS = ['completed', 'abandoned', 'error'] as const;

export type SessionEndReason = (typeof SESSION_END_REASONS)[number];

export interface SessionRecord {
  readonly id: number;
  readonly startedAt: string;
  /** `null` = oturum hala acik (ya da temiz kapanamadi). */
  readonly endedAt: string | null;
  readonly projectId: string | null;
  /**
   * `null` = ozet uretilmedi.
   *
   * Ozet uretimi kapanistan **sonra**, arka planda calisir (ASU-033); basarisiz
   * olursa ya da oturum cok kisaysa bu alan `null` kalir ve oturum yine kapanir.
   * Alan bir durum bayragi **degildir** — yarim kalan oturum `endReason` ile
   * isaretlenir.
   */
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
  /**
   * `null` = bilinmiyor (hala acik oturum ya da migration 002 oncesi kayit).
   * Uydurulmaz: "bilmiyoruz" ayri bir durumdur.
   */
  readonly endReason: SessionEndReason | null;
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
  // `ALTER TABLE ADD COLUMN` kolonu tablonun sonuna ekler; sira sema ile ayni.
  'endReason',
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
    endReason:
      value['endReason'] === null ? null : read.enumeration('endReason', SESSION_END_REASONS),
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

/**
 * Renderer'in bildirebilecegi kapanis nedeni (ASU-033).
 *
 * `abandoned` bilerek **yok**: yarim kalan oturumu tespit etmek host'un isidir
 * (acilistaki kurtarma). Gonderilirse Rust tarafi istegi reddeder.
 */
export type ReportedEndReason = Extract<SessionEndReason, 'completed' | 'error'>;

export interface SessionFinalizeInput {
  readonly usage?: SessionUsageInput;
  readonly transcript?: readonly TranscriptLineInput[];
  /** Verilmezse `completed` sayilir. */
  readonly endReason?: ReportedEndReason;
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

// ---------------------------------------------------------------------------
// Oturum gecmisi: listeleme + silme (ASU-065)
// ---------------------------------------------------------------------------

/**
 * Oturum listesinin tek satiri — Rust `SessionListItem` aynasi.
 *
 * [`SessionRecord`] degil: bu bir **denetim satiri**. Dokum dosyasinin yolu
 * bilerek yok; renderer'in bilmesi gereken tek sey dosyanin var olup olmadigi
 * ([`hasTranscriptFile`]). Silme host tarafinda yapilir ve yol orada,
 * `app_data_dir()` altinda oldugu dogrulanarak cozulur.
 */
export interface SessionListItem {
  readonly id: number;
  readonly startedAt: string;
  /** `null` = oturum hala acik. */
  readonly endedAt: string | null;
  readonly endReason: SessionEndReason | null;
  /** `null` = ozet uretilmedi (kisa oturum ya da basarisiz ozetleme). */
  readonly summaryPreview: string | null;
  /** `true` ise on izleme kirpildi; kaydin kendisi degismedi. */
  readonly summaryTruncated: boolean;
  readonly hasTranscriptFile: boolean;
}

export const SESSION_LIST_ITEM_KEYS = [
  'id',
  'startedAt',
  'endedAt',
  'endReason',
  'summaryPreview',
  'summaryTruncated',
  'hasTranscriptFile',
] as const;

/**
 * Oturum listesi + **olculen** sinirlar.
 *
 * `total` sunucudan gelir: "en yeni 50" demek yerine "50 / 214" demek
 * mumkun olsun. Tavani tahmin etmek (`records.length >= limit`) UI'in
 * uydurdugu bir bilgi olurdu.
 */
export interface SessionPage {
  readonly sessions: readonly SessionListItem[];
  readonly limit: number;
  readonly limitMax: number;
  readonly total: number;
}

/** Dokum dosyasina yapilan silme denemesinin sonucu — Rust aynasi. */
export const TRANSCRIPT_FILE_OUTCOMES = [
  /** Kayitta dokum yolu yoktu. */
  'not-recorded',
  'deleted',
  /** Kayitta yol vardi ama dosya diskte yok. */
  'already-gone',
  /** Yol sandbox disina cikiyordu: dokunulmadi. */
  'refused',
  'failed',
] as const;

export type TranscriptFileOutcome = (typeof TRANSCRIPT_FILE_OUTCOMES)[number];

/**
 * Tek oturum silmenin sonucu.
 *
 * `transcriptFile` ayri bir alan cunku satir ve dosya **ayri ayri** basarisiz
 * olabilir; "sildim" demek yalnizca ikisi de bilindiginde dogrudur.
 */
export type SessionDeleteResult =
  | {
      readonly status: 'deleted';
      readonly id: number;
      readonly transcriptFile: TranscriptFileOutcome;
    }
  | { readonly status: 'skipped'; readonly reason: 'memory-disabled' };

/** Toplu temizligin sonucu — hepsi sayi (kullanici "ne kadari gitti?" gormeli). */
export type SessionPurgeResult =
  | {
      readonly status: 'purged';
      readonly deletedSessions: number;
      readonly deletedFiles: number;
      /** Dokum dizininde birakilan girdi sayisi (yabanci ya da silinemeyen). */
      readonly remainingFiles: number;
    }
  | { readonly status: 'skipped'; readonly reason: 'memory-disabled' };

/**
 * "Konusma gecmisini sil" komutunun istedigi onay ifadesi — Rust
 * `session_repository::CLEAR_ALL_CONFIRMATION` ile **birebir** ayni.
 *
 * [`MEMORY_DELETE_ALL_CONFIRMATION`](./memory.ts) ile bilerek **farkli**: iki
 * aksiyonun kapsami farkli ve ayni cumleyi paylasmalari, birini yazip digerini
 * calistirma hatasini mumkun kilardi. Turkce karakter yok — kullanicinin klavye
 * duzeninden bagimsiz yazilabilmeli.
 */
export const SESSION_CLEAR_ALL_CONFIRMATION = 'KONUSMA GECMISINI SIL';

export function parseSessionListItem(value: unknown): SessionListItem {
  if (!isRecord(value)) {
    throw new SessionContractError('Oturum satiri bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, SESSION_LIST_ITEM_KEYS, failWith);

  const read = readers(value, fail);

  const startedAt = read.timestamp('startedAt');
  const endedAt = read.nullableTimestamp('endedAt');
  if (endedAt !== null && endedAt < startedAt) {
    fail('endedAt', 'baslangictan sonra bir zaman');
  }

  return {
    id: read.id('id'),
    startedAt,
    endedAt,
    endReason:
      value['endReason'] === null ? null : read.enumeration('endReason', SESSION_END_REASONS),
    summaryPreview: read.nullableText('summaryPreview'),
    summaryTruncated: read.boolean('summaryTruncated'),
    hasTranscriptFile: read.boolean('hasTranscriptFile'),
  };
}

export function parseSessionPage(value: unknown): SessionPage {
  if (!isRecord(value)) {
    throw new SessionContractError('Oturum sayfasi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, ['sessions', 'limit', 'limitMax', 'total'], failWith);

  const read = readers(value, fail);
  const raw = value['sessions'];
  if (!Array.isArray(raw)) {
    fail('sessions', 'bir dizi');
  }

  return {
    sessions: raw.map(parseSessionListItem),
    limit: read.id('limit'),
    limitMax: read.id('limitMax'),
    total: read.count('total'),
  };
}

export function parseSessionDeleteResult(value: unknown): SessionDeleteResult {
  if (!isRecord(value)) {
    throw new SessionContractError('Silme sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);

  switch (value['status']) {
    case 'deleted':
      assertNoUnexpectedKeys(value, ['status', 'id', 'transcriptFile'], failWith);
      return {
        status: 'deleted',
        id: read.id('id'),
        transcriptFile: read.enumeration('transcriptFile', TRANSCRIPT_FILE_OUTCOMES),
      };

    case 'skipped':
      assertNoUnexpectedKeys(value, ['status', 'reason'], failWith);
      return { status: 'skipped', reason: read.enumeration('reason', SESSION_SKIP_REASONS) };

    default:
      fail('status', 'su degerlerden biri: deleted, skipped');
  }
}

export function parseSessionPurgeResult(value: unknown): SessionPurgeResult {
  if (!isRecord(value)) {
    throw new SessionContractError('Temizlik sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);

  switch (value['status']) {
    case 'purged':
      assertNoUnexpectedKeys(
        value,
        ['status', 'deletedSessions', 'deletedFiles', 'remainingFiles'],
        failWith,
      );
      // `count`: sifir gecerli bir sonuc (zaten bos gecmis).
      return {
        status: 'purged',
        deletedSessions: read.count('deletedSessions'),
        deletedFiles: read.count('deletedFiles'),
        remainingFiles: read.count('remainingFiles'),
      };

    case 'skipped':
      assertNoUnexpectedKeys(value, ['status', 'reason'], failWith);
      return { status: 'skipped', reason: read.enumeration('reason', SESSION_SKIP_REASONS) };

    default:
      fail('status', 'su degerlerden biri: purged, skipped');
  }
}

/**
 * Oturum suresi (ms) — kapanmamis oturumda `null`.
 *
 * Yarim kalan bir oturum acilista `endedAt = startedAt` ile kapatilir; bu
 * durumda sure `0` doner. Bu bilincli: gercek bitis zamani bilinmiyor ve
 * "simdi" yazmak saatler suren sahte bir oturum uretirdi (ASU-032). Boyle bir
 * oturum `endReason === 'abandoned'` ile ayirt edilir — "0 saniye surdu" degil,
 * "ne kadar surdugunu bilmiyoruz" demektir.
 */
export function sessionDurationMs(session: SessionRecord): number | null {
  if (session.endedAt === null) {
    return null;
  }
  return Date.parse(session.endedAt) - Date.parse(session.startedAt);
}

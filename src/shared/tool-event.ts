/**
 * `tool_events` sozlesmesi — Rust `ToolEventRecord`'un tip aynasi (ASU-050).
 *
 * Tek kaynak `src-tauri/src/db/migrations/004_tool_events.up.sql`; kolon
 * adlarini, `approvalState` kumesini ve `riskLevel` kumesini `schema-mirror.spec.ts`
 * dogrudan o `.sql` dosyasindan okuyup buradaki sabitlerle karsilastirir. Ayni
 * zincirin ucuncu halkasi Rust `db/model.rs` — yani bir kolon eklemek ya da bir
 * onay durumu eklemek, dokunulmayan her katmanda kirmizi test uretir.
 *
 * # Bu tablonun urun anlami (PROJECT.md Bolum 19)
 *
 * Kullanicinin **denetim defteri**: Asuna'nin bilgisayarda ne yaptigini
 * gosteren kayit. Uc ozelligi tip duzeyinde de gorunur:
 *
 * - Her cagri yazilir — onaylanan, reddedilen, hata veren, zaman asimina
 *   ugrayan. `approvalState` bunlari ayirt eder.
 * - `argumentsRedacted` **ham arguman degildir**: anahtar adlari + kirpilmis
 *   skaler degerlerden olusan tek satirlik bir ozettir ve host tarafinda
 *   uretilir. Renderer bu alani gonderemez (bkz. [`ToolAuditInput`]).
 * - Defter salt yazilirdir: bu dosyada bir "silme sonucu" tipi **yok**, cunku
 *   silme komutu yok.
 */

import { ContractError, assertNoUnexpectedKeys, isRecord, readers } from './contract';

/**
 * Bir tool cagrisinin onay yolculugunun sonucu.
 *
 * Degerler semadaki CHECK kisitindan gelir (`004_tool_events.up.sql`);
 * `schema-mirror.spec.ts` ikisini birbirine baglar. Sira da bagli.
 */
export const TOOL_APPROVAL_STATES = [
  /** Bu risk seviyesi bu modda onay gerektirmiyordu (risk 0). */
  'not_required',
  /**
   * Onay gerekebilirdi ama `ASUNA_TOOL_APPROVAL_MODE` izin verdi.
   * `not_required`'dan bilerek ayri: "sorulabilirdi, ayarin izin verdi" demek,
   * ayari sonradan sorgulanabilir kilar.
   */
  'auto_approved',
  'approved',
  'denied',
  /** Onay istegi zaman asimina ugradi -> varsayilan **reddet** (ASU-048). */
  'timeout',
  /**
   * Onay asamasina hic gelinmedi: cagri daha once dustu (sema dogrulamasi,
   * bilinmeyen tool adi, sandbox on-kontrolu). `not_required` ile
   * karistirilmamali — orada onay GEREKMEDI, burada onay SORULAMADI.
   */
  'not_requested',
] as const;

export type ToolApprovalState = (typeof TOOL_APPROVAL_STATES)[number];

/**
 * Risk seviyeleri (PROJECT.md Bolum 5.4) — semadaki
 * `CHECK (risk_level IN (0, 1, 2, 3))` ile bagli.
 *
 * `src/asuna/tools/types.ts` icindeki `ToolRisk` ile ayni kume; o tip tool
 * *tanimi* icin, bu sabit sema aynasi icin. Ikisi `tool-event.spec.ts` ile
 * birbirine baglanir.
 */
export const TOOL_RISK_LEVELS = [0, 1, 2, 3] as const;

export type ToolRiskLevel = (typeof TOOL_RISK_LEVELS)[number];

/**
 * Bir tool cagrisinin **sonucu** (ASU-051, migration 005).
 *
 * [`ToolApprovalState`] ile karistirilmamali ve kumeleri bilerek kesismiyor:
 * onay durumu "calismasina izin verildi mi", bu ise "calisti mi ve isini
 * yapabildi mi" sorusunu cevaplar. `approved` + `failed` gecerli ve sik bir
 * kombinasyon — kullanici izin verdi, is patladi.
 *
 * Degerler semadaki CHECK kisitindan gelir (`005_tool_event_outcome.up.sql`);
 * `schema-mirror.spec.ts` ikisini birbirine baglar. Sira da bagli.
 */
export const TOOL_OUTCOMES = [
  /** Tool calisti ve isini yapti. */
  'succeeded',
  /**
   * Tool **calisti** ama isini yapamadi: implementasyon hatasi, sandbox reddi,
   * timeout. `not_run` degil — yan etkisi olabilecek bir cagri "hic olmadi"
   * diye kaydedilmemeli.
   */
  'failed',
  /**
   * Tool **hic** calismadi: sema reddi, onay reddi/zaman asimi, cagri
   * baslamadan iptal, kapatilmis tool. Yan etki ihtimali yok.
   */
  'not_run',
] as const;

export type ToolOutcome = (typeof TOOL_OUTCOMES)[number];

/**
 * Tool audit defterinin bir satiri.
 *
 * Salt okunur bir gecmis kaydi: UI bunu gosterir, degistirmez.
 */
export interface ToolEventRecord {
  readonly id: number;
  /**
   * `null` = cagriyi ureten oturum bilinmiyor ya da oturum kaydi silinmis
   * (FK `ON DELETE SET NULL`). Uydurulmus bir korelasyon kimligi yazilmaz —
   * "hangi konusmada oldugunu bilmiyoruz" ayri bir cevaptir.
   */
  readonly sessionId: number | null;
  readonly toolName: string;
  readonly riskLevel: ToolRiskLevel;
  /**
   * Redakte edilmis, tek satirlik arguman ozeti (`path=README.md, maxBytes=4096`).
   * Ic ice yapilar yalnizca sekil olarak gorunur (`{2 alan}` / `[3 oge]`), yani
   * dosya icerigi buraya yapisal olarak giremez.
   *
   * `null` = cagri argumansizdi.
   */
  readonly argumentsRedacted: string | null;
  readonly approvalState: ToolApprovalState;
  /**
   * Kisa, insan diliyle sonuc — **basari da hata da** burada
   * (`conventions.md`: "tool basarisi taklit edilmez").
   * `null` = soylenecek bir sonuc yok; tipik olarak cagri hic calismadi.
   */
  readonly resultSummary: string | null;
  readonly createdAt: string;
  /**
   * Cagri calisti mi, calistiysa basardi mi? (ASU-051).
   *
   * `null` = satir migration 005 oncesinde yazildi ve bu eksen o zaman
   * tutulmuyordu. Geriye donuk **uydurulmadi**: `approvalState: 'approved'` bir
   * cagrinin basarili bittigini soylemez.
   */
  readonly outcome: ToolOutcome | null;
}

/**
 * Sozlesmedeki alanlarin tam listesi — sema kolon sirasiyla ayni.
 *
 * `outcome` **sonda**: SQLite `ALTER TABLE ... ADD COLUMN` kolonu tablonun
 * sonuna koyar ve `PRAGMA table_info` sirasi budur.
 */
export const TOOL_EVENT_RECORD_KEYS = [
  'id',
  'sessionId',
  'toolName',
  'riskLevel',
  'argumentsRedacted',
  'approvalState',
  'resultSummary',
  'createdAt',
  'outcome',
] as const;

export class ToolEventContractError extends ContractError {
  public override readonly name = 'ToolEventContractError';
}

function fail(field: string, expected: string): never {
  throw new ToolEventContractError(`\`${field}\` ${expected} olmali.`);
}

function failWith(message: string): never {
  throw new ToolEventContractError(message);
}

function readRiskLevel(value: Record<string, unknown>): ToolRiskLevel {
  const raw = value['riskLevel'];
  if (!TOOL_RISK_LEVELS.includes(raw as ToolRiskLevel)) {
    fail('riskLevel', 'su degerlerden biri: 0, 1, 2, 3');
  }
  return raw as ToolRiskLevel;
}

export function parseToolEventRecord(value: unknown): ToolEventRecord {
  if (!isRecord(value)) {
    throw new ToolEventContractError('Audit kaydi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, TOOL_EVENT_RECORD_KEYS, failWith);

  const read = readers(value, fail);

  return {
    id: read.id('id'),
    sessionId: read.nullableId('sessionId'),
    toolName: read.text('toolName'),
    riskLevel: readRiskLevel(value),
    argumentsRedacted: read.nullableText('argumentsRedacted'),
    approvalState: read.enumeration('approvalState', TOOL_APPROVAL_STATES),
    resultSummary: read.nullableText('resultSummary'),
    createdAt: read.timestamp('createdAt'),
    // Nullable enum: `readers` icinde karsiligi yok ve tek kullanim icin
    // oraya bir yardimci eklemek sozlesme katmanini genisletirdi.
    outcome: value['outcome'] === null ? null : read.enumeration('outcome', TOOL_OUTCOMES),
  };
}

/**
 * Audit sayfasi + **olculen** sinirlar.
 *
 * `total` sunucudan gelir: "en yeni 50" demek yerine "50 / 214" demek mumkun
 * olsun. Tavani tahmin etmek (`events.length >= limit`) UI'in uydurdugu bir
 * bilgi olurdu (`SessionPage` ile ayni gerekce).
 */
export interface ToolEventPage {
  readonly events: readonly ToolEventRecord[];
  readonly limit: number;
  readonly limitMax: number;
  /** Filtreye uyan toplam kayit sayisi. */
  readonly total: number;
}

export function parseToolEventPage(value: unknown): ToolEventPage {
  if (!isRecord(value)) {
    throw new ToolEventContractError('Audit sayfasi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, ['events', 'limit', 'limitMax', 'total'], failWith);

  const read = readers(value, fail);
  const raw = value['events'];
  if (!Array.isArray(raw)) {
    fail('events', 'bir dizi');
  }

  return {
    events: raw.map(parseToolEventRecord),
    limit: read.id('limit'),
    limitMax: read.id('limitMax'),
    total: read.count('total'),
  };
}

/** Audit listesi istegi. Renderer yalnizca kac tane ve hangi oturum diyebilir. */
export interface ToolEventListQuery {
  readonly limit?: number;
  /** Verilmezse tum oturumlar (Tools sekmesinin varsayilan gorunumu). */
  readonly sessionId?: number;
}

/**
 * Audit yazma girdisi.
 *
 * `argumentsRedacted` diye bir alan **yok** ve olmayacak: renderer hazir bir
 * ozet gonderemez. Ham `arguments` gonderilir, ozetleme ve redaksiyon host
 * tarafinda yapilir (`db/tool_event_repository.rs`). Sebep: redaksiyonu
 * webview'e devretmek, onu modelin ciktisiyla ayni process'e teslim etmek
 * olurdu. Rust tarafi `deny_unknown_fields` ile bu alani gonderen istegi
 * dusurur.
 */
export interface ToolAuditInput {
  /**
   * Cagriyi ureten oturum kaydinin kimligi. Verilmezse "bilinmiyor" —
   * uydurulmaz (`ToolContext.sessionId` `null` olabilir).
   */
  readonly sessionId?: number;
  readonly toolName: string;
  readonly riskLevel: ToolRiskLevel;
  /** Ham argumanlar; saklanmaz, yalnizca ozetlenir. */
  readonly arguments?: unknown;
  readonly approvalState: ToolApprovalState;
  /**
   * Kisa sonuc ozeti — basari da hata da.
   *
   * DIKKAT: bu **modele giden metin degildir**. Icerik donduren bir tool
   * (`read_project_file`) modele dosyanin kendisini verir ama deftere tek
   * satirlik bir ozet yazar; ayrimi `ToolResult.auditSummary` tasir. Host
   * tarafi ayrica tek satira indirir, redaksiyondan gecirir ve 512 karakterde
   * kirpar.
   */
  readonly resultSummary?: string;
  /**
   * Cagri calisti mi, calistiysa basardi mi? (ASU-051).
   *
   * Verilmezse satira `NULL` yazilir: sessiz bir `succeeded` varsayimi
   * denetim defterine olculmemis bir iddia yazardi.
   */
  readonly outcome?: ToolOutcome;
}

/**
 * Audit yazma sonucu — Rust `ToolEventWriteResult` aynasi.
 *
 * `skipped` = kalici hafiza kapali; audit satiri **olusmadi** ve cagiran taraf
 * kaydedildigini sanmamali. Hata degil, kullanicinin karari.
 */
export type ToolEventWriteResult =
  | { readonly status: 'recorded'; readonly event: ToolEventRecord }
  | { readonly status: 'skipped'; readonly reason: 'memory-disabled' };

const TOOL_EVENT_SKIP_REASONS = ['memory-disabled'] as const;

export function parseToolEventWriteResult(value: unknown): ToolEventWriteResult {
  if (!isRecord(value)) {
    throw new ToolEventContractError('Audit yazma sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);

  switch (value['status']) {
    case 'recorded':
      assertNoUnexpectedKeys(value, ['status', 'event'], failWith);
      return { status: 'recorded', event: parseToolEventRecord(value['event']) };

    case 'skipped':
      assertNoUnexpectedKeys(value, ['status', 'reason'], failWith);
      return { status: 'skipped', reason: read.enumeration('reason', TOOL_EVENT_SKIP_REASONS) };

    default:
      fail('status', 'su degerlerden biri: recorded, skipped');
  }
}

/**
 * Bu onay durumunda tool **calistirildi mi**?
 *
 * Rust `ToolApprovalState::permitted_execution` ile ayni tanim; `tool-event.spec.ts`
 * ikisinin ayni kumeyi verdigini dogrular. UI'in "reddedildi" ile "calisti ama
 * hata verdi"yi ayirmasi icin gerekli.
 */
export function toolCallWasPermitted(state: ToolApprovalState): boolean {
  return state === 'not_required' || state === 'auto_approved' || state === 'approved';
}

/**
 * Risk 2 ve 3 **her zaman** acik onay ister; bu hicbir
 * `ASUNA_TOOL_APPROVAL_MODE` degeriyle gevsetilemez
 * (`asuna-config/security.md` Bolum 3, `conventions.md`).
 */
export function riskAlwaysRequiresApproval(risk: ToolRiskLevel): boolean {
  return risk >= 2;
}

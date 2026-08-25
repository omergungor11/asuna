/**
 * Oturum kaydinin renderer tarafindaki **tek** erisim noktasi (ASU-032).
 *
 * # Sozlesme
 *
 * - Renderer oturumun **modelini secmez** ve **transcript dosya yolu vermez**:
 *   ikisi de Rust tarafinda config'ten/`app_data_dir()`'den turetilir. Bu servis
 *   yalnizca "oturum basladi" / "oturum bitti + su kadar token + su dokum" der.
 * - Hafiza kapaliyken `session_start` `skipped` doner; o zaman elimizde oturum
 *   kimligi olmaz ve `session_finalize` **cagrilmaz**. Sessizce sahte bir
 *   kimlik uydurulmaz. "Kapali" acilis degeri de olabilir, kullanicinin oturum
 *   sirasinda cevirdigi bir anahtar da (ASU-037) — ikisi de hata degildir,
 *   `onSkipped` ile log'lanir.
 * - Kayit basarisiz olursa sesli oturum **devam eder**. Kapanis akisini
 *   dusurmemek icin [`SessionRecorder`] hatalari yakalar ve log'lar; yutmaz.
 */

import { invoke } from '@tauri-apps/api/core';

import {
  parseSessionDeleteResult,
  parseSessionPage,
  parseSessionPurgeResult,
  parseSessionWriteResult,
  type SessionDeleteResult,
  type SessionFinalizeInput,
  type SessionPage,
  type SessionPurgeResult,
  type SessionWriteResult,
} from '../../shared/session';
import { toStoreError } from '../../shared/store-error';

/**
 * Rust tarafindaki komut adlari. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-session{,-read}.json` ile birebir ayni olmali.
 *
 * Okuma ve degistirme bilerek ayri kumeler: oturum gecmisini gorunur kilmak ile
 * silebilmek ayri yetkiler (`asuna-session-read` / `asuna-session`).
 */
export const SESSION_READ_COMMANDS = {
  list: 'session_list',
} as const;

export const SESSION_COMMANDS = {
  start: 'session_start',
  finalize: 'session_finalize',
  delete: 'session_delete',
  clearAll: 'session_clear_all',
} as const;

async function call(command: string, args: Record<string, unknown>): Promise<unknown> {
  try {
    return await invoke<unknown>(command, args);
  } catch (error) {
    throw toStoreError(error);
  }
}

/**
 * Oturum kaydini acar.
 *
 * @param projectId Phase 4'te (ASU-039+) dolacak; simdilik her zaman bos.
 */
export async function startSessionRecord(projectId?: string): Promise<SessionWriteResult> {
  return parseSessionWriteResult(
    await call(SESSION_COMMANDS.start, { projectId: projectId ?? null }),
  );
}

/** Oturum kaydini kapatir: `endedAt`, token metadata ve (ayar aciksa) dokum. */
export async function finalizeSessionRecord(
  sessionId: number,
  input: SessionFinalizeInput = {},
): Promise<SessionWriteResult> {
  return parseSessionWriteResult(await call(SESSION_COMMANDS.finalize, { sessionId, input }));
}

/**
 * Oturum gecmisini listeler (ASU-065).
 *
 * Hafiza kapaliyken **bos sayfa** doner (hata degil); bozuk oldugunda
 * `unavailable` kodlu hata firlatir. Renderer siralamayi ya da alanlari
 * secemez — yalnizca kac satir istedigini soyleyebilir ve bu istek sunucu
 * tavanina kirpilir (`limit` / `limitMax` yanitta gorunur).
 */
export async function listSessions(limit?: number): Promise<SessionPage> {
  return parseSessionPage(
    await call(SESSION_READ_COMMANDS.list, { query: limit === undefined ? null : { limit } }),
  );
}

/**
 * Tek oturumu siler: kaydi (ozeti dahil) ve varsa diskteki dokum dosyasi.
 *
 * Silinen ozet bir sonraki oturumun baglamina **giremez**: Stage A baglami
 * onbelleklenmiyor, her `connect()` oncesi depodan yeniden okunuyor.
 *
 * Dosya yolu bu servise verilmez ve bu servisten donmez: yol Rust tarafinda
 * veritabanindan okunur ve `app_data_dir()` altinda oldugu dogrulanir. Donen
 * `transcriptFile` yalnizca **ne olduğunu** soyler.
 */
export async function deleteSession(sessionId: number): Promise<SessionDeleteResult> {
  return parseSessionDeleteResult(await call(SESSION_COMMANDS.delete, { sessionId }));
}

/**
 * **Tum** oturum kayitlarini ve dokum dosyalarini siler (ASU-065).
 *
 * Onay ifadesi cagiran taraftan gelir ve Rust'ta birebir karsilastirilir; bu
 * servis onu **uydurmaz** — `clearSessionHistory()` diye parametresiz bir cagri
 * mumkun degil. Ifade tutmazsa `invalid` kodlu hata firlar ve ne DB'ye ne diske
 * dokunulur.
 *
 * Kapsam `memories` tablosunu **icermez**; hafiza silme ayri bir aksiyondur
 * (`deleteAllMemories`).
 */
export async function clearSessionHistory(
  confirmationPhrase: string,
): Promise<SessionPurgeResult> {
  return parseSessionPurgeResult(await call(SESSION_COMMANDS.clearAll, { confirmationPhrase }));
}

/** Kapanmis oturumun UI'da gosterilen ozeti. */
export interface SessionOutcome {
  readonly id: number;
  readonly durationMs: number;
  readonly totalTokens: number | null;
  /**
   * Tahmini maliyet — `null` = **bilinmiyor**.
   *
   * ASU-033 ile dogrulanmis bir fiyat tablosu geldi (`src-tauri/src/pricing.rs`,
   * kaynak `docs/architecture/voice.md` Bolum 6). Sayi yalnizca iki kosul
   * birlikte saglaninca uretilir: modelin fiyati tabloda **var** ve token
   * kirilimi (ses/metin) toplami **aciklayabiliyor**. Aksi halde alan `null`
   * kalir ve UI "bilinmiyor" yazar — yaklasik bir deger uydurulmaz.
   *
   * Ozetleme maliyeti bu sayiya **dahil degildir**: ozet modelinin fiyati
   * dogrulanmadi, bu yuzden token cinsinden `usageJson.summary` altinda durur.
   */
  readonly estimatedCostUsd: number | null;
}

/**
 * Kapanan oturumun tek satirlik ozeti (R1 takibi — PROJECT.md Bolum 28).
 *
 * Maliyet bilinmiyorsa **"bilinmiyor"** yazilir; sifir ya da tahmini bir sayi
 * gosterilmez. Asuna bilmedigi seyi bildigini soylemez — bu kural UI metinleri
 * icin de gecerli.
 */
export function describeSessionOutcome(outcome: SessionOutcome | null): string {
  if (outcome === null) {
    return '—';
  }

  const parts = [formatDuration(outcome.durationMs)];
  if (outcome.totalTokens !== null) {
    parts.push(`${outcome.totalTokens.toLocaleString('tr-TR')} token`);
  }
  parts.push(
    outcome.estimatedCostUsd === null
      ? 'maliyet: bilinmiyor'
      : `maliyet: ~$${outcome.estimatedCostUsd.toFixed(4)}`,
  );

  return parts.join(' · ');
}

function formatDuration(durationMs: number): string {
  const totalSeconds = Math.max(0, Math.round(durationMs / 1_000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return minutes === 0
    ? `${seconds.toString()} sn`
    : `${minutes.toString()} dk ${seconds.toString()} sn`;
}

/** Yazmanin atlandigi asama — log mesajini ayirt edebilmek icin. */
export type SessionRecordStage = 'start' | 'finalize';

export interface SessionRecorderDeps {
  readonly start?: (projectId?: string) => Promise<SessionWriteResult>;
  readonly finalize?: (
    sessionId: number,
    input: SessionFinalizeInput,
  ) => Promise<SessionWriteResult>;
  /** Kayit hatasi sesli oturumu dusurmez ama gorunur olur. */
  readonly onError?: (error: unknown) => void;
  /**
   * Kayit **bilincli olarak** atlandi (`skipped`).
   *
   * Hata degil: kullanici kalici hafizayi kapatmis olabilir (ASU-037) ve bu
   * onun karari. Yine de sessiz kalmaz — "oturum neden kaydedilmedi?" sorusunun
   * cevabi log'da durur (`conventions.md`: sessiz yutma yok).
   */
  readonly onSkipped?: (stage: SessionRecordStage, reason: string) => void;
}

/**
 * Oturum yasam dongusunu kalici kayda baglar.
 *
 * Neden bir sinif: `session_start` asenkron, `disconnected` ise senkron gelir.
 * Kimligin "henuz gelmedi" hali bir yerde tutulmali; bunu hook'un ref'lerine
 * dagitmak yerine tek bir yerde toplamak, "oturum kapandi ama id daha
 * gelmemisti" durumunu **kaybetmeden** ele almayi mumkun kilar.
 */
export class SessionRecorder {
  private readonly deps: Required<SessionRecorderDeps>;

  private pending: Promise<number | null> | null = null;

  private sessionId: number | null = null;

  private startedAtMs: number | null = null;

  private closing: Promise<SessionOutcome | null> | null = null;

  public constructor(deps: SessionRecorderDeps = {}) {
    this.deps = {
      start: deps.start ?? startSessionRecord,
      finalize: deps.finalize ?? finalizeSessionRecord,
      onError: deps.onError ?? ((): void => undefined),
      onSkipped: deps.onSkipped ?? ((): void => undefined),
    };
  }

  /** Oturum acildi. Kayit arka planda olusur; ses akisini bekletmez. */
  public begin(startedAtMs: number, projectId?: string): void {
    if (this.pending !== null) {
      return;
    }
    this.startedAtMs = startedAtMs;
    this.closing = null;

    this.pending = this.deps
      .start(projectId)
      .then((result) => {
        // Hafiza kapali: kimlik yok, kapanista da yazma denenmez. Bu bir hata
        // degil (kullanicinin karari) ama gorunur kalir.
        if (result.status !== 'recorded') {
          this.deps.onSkipped('start', result.reason);
          this.sessionId = null;
          return null;
        }
        this.sessionId = result.session.id;
        return this.sessionId;
      })
      .catch((error: unknown) => {
        this.deps.onError(error);
        this.sessionId = null;
        return null;
      });
  }

  /**
   * Oturum kapandi.
   *
   * `session_start` hala ucusta olabilir; bu yuzden once o beklenir — aksi halde
   * kayit sonsuza kadar `ended_at = NULL` kalir ve bir sonraki acilista
   * "yarim kalmis oturum" olarak kurtarilirdi.
   */
  public end(endedAtMs: number, input: SessionFinalizeInput): Promise<SessionOutcome | null> {
    const pending = this.pending;
    if (pending === null) {
      return Promise.resolve(null);
    }
    this.pending = null;

    const startedAtMs = this.startedAtMs;
    this.startedAtMs = null;

    this.closing = pending
      .then(async (sessionId) => {
        if (sessionId === null) {
          return null;
        }
        const result = await this.deps.finalize(sessionId, input);
        if (result.status !== 'recorded') {
          // Oturum acildiktan **sonra** hafiza kapatilmis olabilir (ASU-037):
          // kapanis yazilmaz, UI'da oturum ozeti gorunmez. Hata degil.
          this.deps.onSkipped('finalize', result.reason);
          return null;
        }
        return {
          id: result.session.id,
          durationMs: Math.max(0, endedAtMs - (startedAtMs ?? endedAtMs)),
          totalTokens: result.session.totalTokens,
          estimatedCostUsd: result.session.estimatedCostUsd,
        } satisfies SessionOutcome;
      })
      .catch((error: unknown) => {
        this.deps.onError(error);
        return null;
      });

    return this.closing;
  }
}

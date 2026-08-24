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
 *   kimlik uydurulmaz.
 * - Kayit basarisiz olursa sesli oturum **devam eder**. Kapanis akisini
 *   dusurmemek icin [`SessionRecorder`] hatalari yakalar ve log'lar; yutmaz.
 */

import { invoke } from '@tauri-apps/api/core';

import {
  parseSessionWriteResult,
  type SessionFinalizeInput,
  type SessionWriteResult,
} from '../../shared/session';
import { toStoreError } from '../../shared/store-error';

/**
 * Rust tarafindaki komut adlari. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-session.json` ile birebir ayni olmali.
 */
export const SESSION_COMMANDS = {
  start: 'session_start',
  finalize: 'session_finalize',
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

export interface SessionRecorderDeps {
  readonly start?: (projectId?: string) => Promise<SessionWriteResult>;
  readonly finalize?: (
    sessionId: number,
    input: SessionFinalizeInput,
  ) => Promise<SessionWriteResult>;
  /** Kayit hatasi sesli oturumu dusurmez ama gorunur olur. */
  readonly onError?: (error: unknown) => void;
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
        // Hafiza kapali: kimlik yok, kapanista da yazma denenmez.
        this.sessionId = result.status === 'recorded' ? result.session.id : null;
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

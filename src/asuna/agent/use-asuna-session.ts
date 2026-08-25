/**
 * `useAsunaSession` — React ile Asuna ses servisi arasindaki **tek** kopru (ASU-015).
 *
 * # Sozlesme
 *
 * - Bilesenler bu hook'un dondugu duz veriyi gorur; `AsunaRealtimeService`, Tauri IPC
 *   ve SDK tipleri bilesen katmanina **sizmaz** (`conventions.md` — Servis Sinirlari).
 * - Durum uydurulmaz: gosterilen `state` tek dogru kaynaktan, `VoiceStateMachine`'den
 *   okunur (`useSyncExternalStore`). Hook paralel bir durum kopyasi tutmaz; yalnizca
 *   servis event'lerinden gelen **ek** olgulari (model, hata, aktif tool) biriktirir.
 * - Baglanti akisi: mikrofon izni -> config -> `connect()` (token mint SDK'nin lazy
 *   `apiKey` cagrisinda, servisin icinde olur) -> `LISTENING`.
 * - Tum bagimliliklar enjekte edilebilir; testler ne aga cikar ne mikrofona dokunur.
 *
 * # Ses yolu (ASU-016)
 *
 * Iki yonlu sesin kendisi burada **kurulmaz**: mikrofon track'ini ve `<audio autoplay>`
 * cikis elementini WebRTC transport'u (SDK) kendisi acar (voice.md Bolum 4). Hook'un isi
 * o akisi gorunur kilmak: konusma durumu, barge-in tepkisi ve "konusma sonu -> ilk ses"
 * gecikmesi. Mikrofon izni ve echo cancellation dogrulamasi baglanmadan once
 * `probeMicrophoneAccess()` ile yapilir.
 */

import {
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';

import {
  AsunaRealtimeError,
  type AsunaRealtimeErrorInfo,
  type AsunaRealtimeErrorKind,
} from './realtime-errors';
import type {
  AsunaRealtimeEvent,
  AsunaRealtimeEventListener,
  RealtimeUsageSnapshot,
  TranscriptEntry,
} from './realtime-events';
import { AsunaRealtimeService } from './realtime-service';
import { registerWindowCloseHandler } from './window-lifecycle';
import { probeMicrophoneAccess, type MicrophoneProbe } from '../audio/microphone-access';
import { loadFrontendConfig } from '../config/config.service';
import { describeTurnDetection, type FrontendConfig } from '../config/frontend-config';
import { buildSessionInstructions } from '../memory/bootstrap-context';
import { fetchPrivacySettings } from '../memory/privacy-service';
import { SessionRecorder, type SessionOutcome } from '../memory/session-service';
import {
  createLoggedVoiceStateMachine,
  isAsunaErrorKind,
  logger as defaultLogger,
  toUserFacingError,
  userFacingErrorFor,
  type AsunaLogger,
  type AsunaServiceErrorKind,
  type UserFacingError,
} from '../observability';
import type {
  VoiceState,
  VoiceStateMachine,
  VoiceTransitionReason,
} from '../state/voice-state-machine';
import { asunaToolRegistry } from '../tools';
import type { ToolRisk } from '../tools/types';
import type { SessionUsageInput, TranscriptLineInput } from '../../shared/session';

// ---------------------------------------------------------------------------
// Servis sinirlari
// ---------------------------------------------------------------------------

/**
 * Hook'un gordugu servis yuzeyi — `AsunaRealtimeService`'in genel API'si kadar.
 *
 * Dar tutulmasinin sebebi test degil sinir: hook oturumun ic islerine (token, SDP,
 * retry) erisemez, yalnizca hayat dongusunu tetikler.
 */
export interface AsunaSessionPort {
  connect(): Promise<void>;
  disconnect(): void;
  interrupt(): void;
  /** Bekleyen tool onayini onaylar (ASU-048); karti ASU-053 kurar. */
  approveToolCall(requestId: string): void;
  /** Bekleyen tool onayini reddeder. */
  rejectToolCall(requestId: string, reason?: string): void;
  subscribe(listener: AsunaRealtimeEventListener): () => void;
  getState(): VoiceState;
}

export interface AsunaSessionContext {
  readonly config: FrontendConfig;
  readonly stateMachine: VoiceStateMachine;
  /**
   * Acik oturum kaydi (ASU-032). Servis buradan **yalnizca** aktif oturumun
   * kimligini okur (`tool_events.session_id` korelasyonu, ASU-048/050); kaydi
   * acip kapatan taraf hook'un kendisidir.
   */
  readonly recorder: SessionRecorder;
}

export type AsunaSessionFactory = (context: AsunaSessionContext) => AsunaSessionPort;

/** Kurulum asamasinda (config okuma) olusan, etiketi bilinen hata. */
export class AsunaSessionSetupError extends Error {
  public override readonly name = 'AsunaSessionSetupError';

  public constructor(
    public readonly kind: AsunaServiceErrorKind,
    message: string,
  ) {
    super(message);
  }
}

// ---------------------------------------------------------------------------
// Hata cevirisi
// ---------------------------------------------------------------------------

/**
 * Realtime hata sinifi -> ASU-019 mesaj tablosundaki en yakin etiket.
 *
 * `Record` bilincli: yeni bir realtime hata turu eklenirse burasi derleme hatasi verir.
 */
const REALTIME_ERROR_FALLBACK: Readonly<Record<AsunaRealtimeErrorKind, AsunaServiceErrorKind>> =
  {
    token: 'realtime_connect_failed',
    unsupported: 'realtime_connect_failed',
    transport: 'realtime_connect_failed',
    session: 'realtime_disconnected',
    internal: 'unknown',
  };

/**
 * Servis hatasini kullaniciya gosterilecek mesaja cevirir.
 *
 * Oncelik `cause`: Rust tarafi hatayi zaten dokuz ayri etikete ayirmis
 * (`invalid_api_key`, `quota_exceeded`, ...) ve ASU-019 tablosunda her birinin
 * somut bir eylem cumlesi var. Etiket cozulemezse servisin kendi (redakte edilmis)
 * mesaji korunur — "bir seyler ters gitti" kovasina dusurulmez.
 */
export function describeRealtimeFailure(info: AsunaRealtimeErrorInfo): UserFacingError {
  if (info.cause !== null && isAsunaErrorKind(info.cause)) {
    return userFacingErrorFor(info.cause);
  }

  const fallback = userFacingErrorFor(REALTIME_ERROR_FALLBACK[info.kind]);
  return {
    kind: fallback.kind,
    message: info.message,
    action: fallback.action,
    // Oturum ici hatada servisin `retryable: false` demesi "otomatik yeniden baglanma
    // yapma" demektir (ASU-013 tasarim notu) — kullanicinin elle tekrar baglanmasi
    // anlamsiz degil. UI sozlesmesi burada ASU-019 tablosunu izler.
    retryable: info.kind === 'session' ? fallback.retryable : info.retryable,
  };
}

/** Aktivasyon akisindaki her hatayi tek bicime indirger. */
export function describeActivationError(error: unknown): UserFacingError {
  if (error instanceof AsunaRealtimeError) {
    return describeRealtimeFailure(error.info);
  }
  // `MicrophoneAccessError` / `AsunaSessionSetupError` `kind` alani tasir; geri kalan
  // her sey durustce "cozemedim" mesajina duser (ASU-019).
  return toUserFacingError(error);
}

// ---------------------------------------------------------------------------
// Gecikme olcumu (ASU-016 / ASU-020 girdisi)
// ---------------------------------------------------------------------------

/**
 * "Konusma sonu -> ilk ses" suresini olcer.
 *
 * DURUSTLUK NOTU: normalize event akisinda VAD'in "konusma bitti" sinyali **yok**
 * (`realtime-events.ts`). Konusma sonuna en yakin gozlemlenebilir iki isaret var:
 * kullanici transkriptinin kesinlesmesi ve modelin yanit uretmeye baslamasi
 * (`agent_thinking`). Hangisi once gelirse konusma sonu kabul edilir; olculen sure
 * bu yuzden gercek gecikmenin **alt siniridir** (VAD sessizlik penceresi disarida kalir).
 * ASU-020'de canli olcumle karsilastirilacak.
 */
export class TurnLatencyTracker {
  private speechEndAt: number | null = null;

  /** Ilk isaret kazanir; ayni turda gelen ikinci isaret olcumu bozmaz. */
  public markSpeechEnd(at: number): void {
    this.speechEndAt ??= at;
  }

  /** @returns olculen gecikme (ms) ya da olcum baslamadiysa `null`. */
  public takeLatency(at: number): number | null {
    const start = this.speechEndAt;
    if (start === null) {
      return null;
    }
    this.speechEndAt = null;
    return at - start;
  }

  public reset(): void {
    this.speechEndAt = null;
  }
}

// ---------------------------------------------------------------------------
// Transcript (ASU-017)
// ---------------------------------------------------------------------------

/**
 * Ekranda gorunen tek dokum satiri.
 *
 * `TranscriptEntry`'den tek farki `interrupted`: kesilen cevabin nerede kesildigini
 * kullanici gormeli (ASU-017). Bu bilgi item'in kendisinde degil, ayri bir event'te
 * (`agent_interrupted`) geliyor.
 */
export interface TranscriptLine {
  readonly itemId: string;
  readonly role: 'user' | 'assistant';
  readonly text: string;
  readonly status: 'in_progress' | 'completed' | 'incomplete';
  readonly interrupted: boolean;
}

/**
 * Bellekte tutulan azami satir sayisi.
 *
 * Phase 1'de dokum **yalnizca bellekte** (disk yazimi ASU-032). Sinirsiz buyuyen bir
 * dizi uzun oturumda hem bellegi hem render'i sisirir; en eski satirlar dusurulur.
 */
export const MAX_TRANSCRIPT_LINES = 200;

function upsertTranscript(
  lines: readonly TranscriptLine[],
  entry: TranscriptEntry,
): readonly TranscriptLine[] {
  const index = lines.findIndex((line) => line.itemId === entry.itemId);

  if (index >= 0) {
    const previous = lines[index];
    const next = lines.slice();
    next[index] = {
      itemId: entry.itemId,
      role: entry.role,
      text: entry.text,
      status: entry.status,
      // Kesilme isareti item guncellenince kaybolmamali.
      interrupted: previous?.interrupted === true || entry.status === 'incomplete',
    };
    return next;
  }

  const appended = [
    ...lines,
    {
      itemId: entry.itemId,
      role: entry.role,
      text: entry.text,
      status: entry.status,
      interrupted: entry.status === 'incomplete',
    },
  ];

  return appended.length > MAX_TRANSCRIPT_LINES
    ? appended.slice(appended.length - MAX_TRANSCRIPT_LINES)
    : appended;
}

/** Kesme aninda uretilmekte olan Asuna cevabini isaretler. */
function markLastAssistantInterrupted(
  lines: readonly TranscriptLine[],
): readonly TranscriptLine[] {
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const line = lines[index];
    if (line?.role !== 'assistant') {
      continue;
    }
    // En son Asuna satiri zaten bitmisse kesilecek bir sey yok: geriye donuk isaret koymuyoruz.
    if (line.status !== 'in_progress' || line.interrupted) {
      return lines;
    }
    const next = lines.slice();
    next[index] = { ...line, interrupted: true };
    return next;
  }
  return lines;
}

// ---------------------------------------------------------------------------
// Oturum kaydi (ASU-032)
// ---------------------------------------------------------------------------

/**
 * Kalici kayda gidecek dokumu biriktirir.
 *
 * Reducer state'i `dispatch` sonrasi bir render bekler; oturum kapanisi ise
 * senkron gelir. Kapanista "en son ne konusuldu"yu kaybetmemek icin dokum ayrica
 * burada, ref benzeri bir yapida tutulur.
 *
 * TEMPORARY (ASU-026): Phase 2'de oturum kapanis akisi wake word/idle timeout
 * ile birlestiginde bu toplayici oradan beslenecek; sozlesmesi degismeyecek.
 */
export class TranscriptCollector {
  private lines: readonly TranscriptLine[] = [];

  /** Item ilk gorulduğu an — dosyaya yazilan zaman. Uydurulmaz, olculur. */
  private readonly firstSeen = new Map<string, string>();

  public reset(): void {
    this.lines = [];
    this.firstSeen.clear();
  }

  public observe(entry: TranscriptEntry, atMs: number): void {
    if (!this.firstSeen.has(entry.itemId)) {
      this.firstSeen.set(entry.itemId, new Date(atMs).toISOString());
    }
    this.lines = upsertTranscript(this.lines, entry);
  }

  /** Kalici kayda uygun hale getirir: bos metinli (henuz gelmemis) satirlar atilir. */
  public toInput(): TranscriptLineInput[] {
    return this.lines
      .filter((line) => line.text.trim().length > 0)
      .map((line) => {
        const at = this.firstSeen.get(line.itemId);
        return at === undefined
          ? { role: line.role, text: line.text }
          : { role: line.role, text: line.text, at };
      });
  }
}

/** SDK'nin usage anlik goruntusunu kalici kayit sozlesmesine cevirir. */
export function toSessionUsageInput(usage: RealtimeUsageSnapshot): SessionUsageInput {
  return {
    requests: usage.requests,
    inputTokens: usage.inputTokens,
    outputTokens: usage.outputTokens,
    totalTokens: usage.totalTokens,
    inputTokenDetails: usage.inputTokenDetails,
    outputTokenDetails: usage.outputTokenDetails,
  };
}

// ---------------------------------------------------------------------------
// Event -> UI olgulari
// ---------------------------------------------------------------------------

/**
 * Ekranda gosterilen onay istegi (ASU-048; kart ASU-053).
 *
 * Servisin `tool_approval_requested` event'inin duz kopyasi: hook UI'a yeni bir
 * bilgi **eklemez**, ozellikle "ne kadar suresi kaldi"yi kendi saymaz —
 * `requestedAtMs` + `timeoutMs` ile karti geri sayimi kendisi cizer, ama
 * zaman asiminin kendisi servisin karari (otomatik reddetme orada).
 */
export interface PendingToolApproval {
  readonly requestId: string;
  readonly toolName: string;
  /** Tool'un insan diliyle ne yaptigi; bos = tanim bulunamadi. */
  readonly description: string;
  /** `null` = risk seviyesi bilinmiyor (kayitli olmayan tool). */
  readonly risk: ToolRisk | null;
  /** Redakte edilmis, tek satirlik arguman ozeti; `null` = argumansiz. */
  readonly argumentsPreview: string | null;
  /** Onay penceresi (ms) — kart geri sayimi. */
  readonly timeoutMs: number;
  /** Istegin gelis ani (`now()`), geri sayimin baslangici. */
  readonly requestedAtMs: number;
}

interface SessionFacts {
  /** Oturum acik mi — servisin `connected`/`disconnected` event'lerinden. */
  readonly connected: boolean;
  /** Hangi modelde konusuluyor (hard-code yok, event'ten gelir). */
  readonly model: string | null;
  /**
   * Su an calisan tool'un adi (ASU-044'ten beri gercekten doluyor).
   *
   * `tool_call_started` ile dolar, `tool_call_completed` ile bosalir; panelde
   * "Aktif araç" satiri bunu gosterir. Tool cagrisinin gorunmez olmasi, Asuna'nin
   * arka planda ne yaptigini kullanicidan saklamak demekti (PROJECT.md Bolum 21).
   */
  readonly activeTool: string | null;
  /**
   * Kullanici onayi bekleyen tool cagrisi (ASU-048).
   *
   * Tek eleman: onay karari **cagri basinadir** ve ayni anda birden fazla kart
   * gostermek "hepsini onayla" refleksine davetiye olurdu. Yeni bir istek
   * gelirse en yenisi gorunur; eskisi zaman asimiyla servis tarafinda
   * reddedilir.
   */
  readonly pendingApproval: PendingToolApproval | null;
  readonly error: UserFacingError | null;
  /**
   * Kullanici Asuna'nin sozunu kesti ve Asuna sustu (ASU-016 barge-in).
   * Bir sonraki ses parcasi baslayana ya da oturum kapanana kadar gorunur kalir.
   */
  readonly bargeIn: boolean;
  /** Son turun "konusma sonu -> ilk ses" suresi (ms). */
  readonly lastLatencyMs: number | null;
  /** Canli dokum — yalnizca bellekte (ASU-017). */
  readonly transcript: readonly TranscriptLine[];
  /** Kapanan oturumun kalici kaydi: sure + token (ASU-032, R1 takibi). */
  readonly sessionOutcome: SessionOutcome | null;
}

const INITIAL_FACTS: SessionFacts = {
  connected: false,
  model: null,
  activeTool: null,
  pendingApproval: null,
  error: null,
  bargeIn: false,
  lastLatencyMs: null,
  transcript: [],
  sessionOutcome: null,
};

type SessionAction =
  | { readonly type: 'activation_started' }
  | { readonly type: 'activation_failed'; readonly error: UserFacingError }
  | { readonly type: 'latency_measured'; readonly latencyMs: number }
  | { readonly type: 'session_recorded'; readonly outcome: SessionOutcome }
  | {
      readonly type: 'realtime_event';
      readonly event: AsunaRealtimeEvent;
      /** Event'in gelis ani — onay kartinin geri sayimi buradan baslar. */
      readonly atMs: number;
    };

function reduceRealtimeEvent(
  state: SessionFacts,
  event: AsunaRealtimeEvent,
  atMs: number,
): SessionFacts {
  switch (event.type) {
    case 'connecting':
      return { ...state, connected: false };

    case 'connected':
      return { ...state, connected: true, model: event.model, error: null, bargeIn: false };

    case 'reconnecting':
      // Sessiz retry yok: kullanici neden beklettigimizi gorsun.
      return { ...state, error: describeRealtimeFailure(event.error) };

    case 'disconnected':
      // Kapanan oturumun onay karti ekranda kalmaz: servis bekleyen istekleri
      // zaten reddetti (`abandonApprovals`).
      return {
        ...state,
        connected: false,
        activeTool: null,
        pendingApproval: null,
        bargeIn: false,
      };

    case 'error':
      return { ...state, error: describeRealtimeFailure(event.error) };

    case 'tool_call_started':
      return { ...state, activeTool: event.toolName };

    case 'tool_approval_requested':
      return {
        ...state,
        activeTool: event.toolName,
        pendingApproval: {
          requestId: event.requestId,
          toolName: event.toolName,
          description: event.description,
          risk: event.risk,
          argumentsPreview: event.argumentsPreview,
          timeoutMs: event.timeoutMs,
          requestedAtMs: atMs,
        },
      };

    case 'tool_approval_resolved':
      // Baska bir istegin karti ekrandaysa onu dusurmuyoruz.
      return state.pendingApproval?.requestId === event.requestId
        ? {
            ...state,
            pendingApproval: null,
            // Onaylandiysa tool simdi calisacak; reddedildiyse calisan bir sey yok.
            activeTool: event.outcome === 'approved' ? state.activeTool : null,
          }
        : state;

    case 'tool_call_completed':
      return { ...state, activeTool: null };

    // Barge-in: kullanici sozu kesti, sunucu uretilen sesi durdurdu. Gorsel tepki
    // olmazsa kullanici "duydu mu?" diye tekrar konusur (ASU-016).
    case 'agent_interrupted':
      return {
        ...state,
        bargeIn: true,
        transcript: markLastAssistantInterrupted(state.transcript),
      };

    // Yeni ses parcasi basladi: kesme isareti kalkar.
    case 'agent_audio_started':
      return { ...state, bargeIn: false };

    // Durum gecisleri FSM'den okunuyor; bu event'ler UI olgusu tasimiyor.
    case 'transcript':
      return { ...state, transcript: upsertTranscript(state.transcript, event.entry) };

    case 'agent_thinking':
    case 'agent_audio_stopped':
    case 'turn_ended':
    case 'usage':
    case 'unexpected_signal':
      return state;
  }
}

function reduceSession(state: SessionFacts, action: SessionAction): SessionFacts {
  switch (action.type) {
    case 'activation_started':
      // Yeni oturum yeni dokum: onceki oturumun satirlari modelin baglaminda da yok.
      return {
        ...state,
        error: null,
        activeTool: null,
        pendingApproval: null,
        bargeIn: false,
        transcript: [],
        sessionOutcome: null,
      };
    case 'activation_failed':
      return { ...state, connected: false, error: action.error };
    case 'latency_measured':
      return { ...state, lastLatencyMs: action.latencyMs };
    case 'session_recorded':
      return { ...state, sessionOutcome: action.outcome };
    case 'realtime_event':
      return reduceRealtimeEvent(state, action.event, action.atMs);
  }
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export interface UseAsunaSessionOptions {
  readonly loadConfig?: () => Promise<FrontendConfig>;
  readonly createService?: AsunaSessionFactory;
  readonly probeMicrophone?: () => Promise<MicrophoneProbe>;
  readonly stateMachine?: VoiceStateMachine;
  readonly logger?: AsunaLogger;
  /** Gecikme olcumunun zaman kaynagi — testte deterministik kilmak icin. */
  readonly now?: () => number;
  /**
   * Pencere kapanisini dinler; kanca cagrildiginda oturum kapatilir (ASU-018).
   * @returns kancayi soken fonksiyon.
   */
  readonly registerCloseHandler?: (handler: () => void) => () => void;
  /**
   * Oturumu kalici kayda baglar (ASU-032). Testlerde sahte kayitci verilir;
   * varsayilan `session_start` / `session_finalize` komutlarini cagirir.
   */
  readonly createSessionRecorder?: () => SessionRecorder;
}

export interface AsunaSession {
  /** Tek dogru durum kaynagi (`VoiceStateMachine`). */
  readonly state: VoiceState;
  /** Aktivasyon/kapanis islemi suruyor — buton bu sirada kilitli. */
  readonly busy: boolean;
  readonly connected: boolean;
  /**
   * Mikrofon Realtime oturumuna akiyor mu.
   *
   * Oturum acikken mikrofonun sahibi SDK'dir (voice.md Bolum 4 "Secenek A"), bu yuzden
   * "oturum acik" ile "mikrofon acik" Phase 1'de ayni sey.
   */
  readonly micActive: boolean;
  readonly model: string | null;
  readonly activeTool: string | null;
  /**
   * Kullanici onayi bekleyen tool cagrisi (ASU-048) — onay karti bunu gosterir.
   * `null` = bekleyen istek yok.
   */
  readonly pendingApproval: PendingToolApproval | null;
  readonly error: UserFacingError | null;
  /** Kullanici Asuna'nin sozunu kesti — gorsel tepki icin (ASU-016). */
  readonly bargeIn: boolean;
  /** Son turun olculen yanit gecikmesi (ms); yoksa `null`. */
  readonly lastLatencyMs: number | null;
  /** Canli dokum: kayit/log, sohbet gecmisi degil (ASU-017). */
  readonly transcript: readonly TranscriptLine[];
  /**
   * Kapanan oturumun kalici kaydi (ASU-032): sure, token ve tahmini maliyet.
   *
   * `null` = henuz kapanmis bir oturum yok ya da hafiza kapali oldugu icin
   * kayit tutulmadi. `estimatedCostUsd` su an her zaman `null` — dogrulanmamis
   * bir fiyat tablosundan sayi uydurulmuyor (ASU-033).
   */
  readonly sessionOutcome: SessionOutcome | null;
  /** "Talk to Asuna" — izin -> config -> connect. Hata icerde yakalanir. */
  readonly start: () => void;
  /** "Stop" — oturumu kapatir. */
  readonly stop: () => void;
  /**
   * Bekleyen tool onayini onaylar (ASU-048).
   *
   * `requestId` alir, "sonuncuyu onayla" demez: kartin gosterdigi istek ile
   * onaylanan istegin ayni oldugu **kanitli** olmali. Bekleyen istek yoksa ya
   * da kimlik eskimisse cagri sessizce gorunur bir "beklenmeyen sinyal"e duser,
   * yanlis bir cagriyi onaylamaz.
   */
  readonly approveTool: (requestId: string) => void;
  /** Bekleyen tool onayini reddeder; model reddi ogrenir. */
  readonly rejectTool: (requestId: string) => void;
}

interface ResolvedDeps {
  readonly loadConfig: () => Promise<FrontendConfig>;
  readonly createService: AsunaSessionFactory;
  readonly probeMicrophone: () => Promise<MicrophoneProbe>;
  readonly machine: VoiceStateMachine;
  readonly log: AsunaLogger;
  readonly now: () => number;
  readonly latency: TurnLatencyTracker;
  readonly registerCloseHandler: (handler: () => void) => () => void;
  readonly recorder: SessionRecorder;
  readonly transcriptCollector: TranscriptCollector;
}

function defaultCreateService(context: AsunaSessionContext): AsunaSessionPort {
  return new AsunaRealtimeService({
    config: context.config,
    stateMachine: context.stateMachine,
    // ASU-048/050: tool audit satirlari **gercek** oturum kaydina baglanir.
    // Kimlik her cagrida yeniden sorulur: `session_start` asenkron doner ve
    // oturumun ilk saniyelerinde henuz yoktur; o aralikta `null` yazilir —
    // uydurulmus bir korelasyon kimligi yanlis zincir uretirdi.
    resolveSessionId: (): number | null => context.recorder.currentSessionId,
    // ASU-047: liste registry'den gelir — "hangi yetenekler acik?" sorusunun
    // tek cevabi orasi. Hepsi risk 0 / onaysiz; MVP kurali "once salt okuma"
    // (PROJECT.md Bolum 17).
    tools: asunaToolRegistry.list(),
    // ASU-035: oturum acilmadan once Stage A baglami cekilir ve talimata
    // enjekte edilir. Hafiza kapali/bozuksa `buildSessionInstructions` durust
    // bir "hatirlamiyorum" satiriyla doner — konusma bloklanmaz.
    prepareInstructions: (): Promise<string> => buildSessionInstructions(),
    // ASU-037: transkript saklama anahtari **canli** okunur. Boot config'i
    // yalnizca tavan; kullanici Ayarlar'dan kapattiysa bu oturumda kullanici
    // sesi hic yaziya cevrilmez (yeniden baslatma gerekmez).
    resolveTranscription: async (): Promise<boolean> =>
      (await fetchPrivacySettings()).transcriptStorage,
  });
}

function resolveDeps(options: UseAsunaSessionOptions): ResolvedDeps {
  const log = options.logger ?? defaultLogger;
  const recorderLog = log.child('session-record');
  return {
    recorder:
      options.createSessionRecorder?.() ??
      new SessionRecorder({
        // Kayit hatasi konusmayi dusurmez ama gorunur olur (PROJECT.md Bolum 30).
        onError: (error: unknown): void => {
          recorderLog.warn('Oturum kaydi yazilamadi; konusma etkilenmedi.', {
            detail: error instanceof Error ? error.message : String(error),
          });
        },
        // ASU-037: kalici hafiza kapaliyken kayit **atlanir**. Hata degil —
        // kullanicinin karari; yine de sessiz kalmaz.
        onSkipped: (stage, reason): void => {
          recorderLog.info('Oturum kaydi atlandi.', { stage, reason });
        },
      }),
    transcriptCollector: new TranscriptCollector(),
    loadConfig: options.loadConfig ?? loadFrontendConfig,
    createService: options.createService ?? defaultCreateService,
    probeMicrophone:
      options.probeMicrophone ?? ((): Promise<MicrophoneProbe> => probeMicrophoneAccess()),
    // Log'lu makine: gecisler ASU-019 formatinda kendiliginden akar.
    machine: options.stateMachine ?? createLoggedVoiceStateMachine(),
    log: log.child('voice-session'),
    now: options.now ?? ((): number => Date.now()),
    latency: new TurnLatencyTracker(),
    registerCloseHandler: options.registerCloseHandler ?? registerWindowCloseHandler,
  };
}

export function useAsunaSession(options: UseAsunaSessionOptions = {}): AsunaSession {
  // Bagimliliklar mount aninda dondurulur: servis kimliginin render sirasinda
  // degismesi acik bir oturumu kaybetmek demek olurdu.
  const [deps] = useState<ResolvedDeps>(() => resolveDeps(options));

  const [facts, dispatch] = useReducer(reduceSession, INITIAL_FACTS);
  const [busy, setBusy] = useState(false);

  const serviceRef = useRef<AsunaSessionPort | null>(null);
  const unsubscribeRef = useRef<(() => void) | null>(null);
  const configRef = useRef<FrontendConfig | null>(null);
  const busyRef = useRef(false);
  const connectedRef = useRef(false);
  /** Servis tarafinda kapatilmasi gereken bir oturum var mi (baglanma asamasi dahil). */
  const sessionOpenRef = useRef(false);
  const mountedRef = useRef(true);
  /** Kapanistan hemen once gelen `usage` — kalici kayda yazilir (ASU-032). */
  const usageRef = useRef<SessionUsageInput | null>(null);

  const state = useSyncExternalStore(
    useCallback(
      (onStoreChange: () => void) =>
        deps.machine.subscribe(() => {
          onStoreChange();
        }),
      [deps.machine],
    ),
    useCallback(() => deps.machine.getState(), [deps.machine]),
    useCallback(() => deps.machine.getState(), [deps.machine]),
  );

  /** Gecisi yalnizca tablo izin veriyorsa uygular (dev'de `throw` politikasi var). */
  const transition = useCallback(
    (to: VoiceState, reason: VoiceTransitionReason): void => {
      if (deps.machine.getState() === to || !deps.machine.canTransition(to)) {
        return;
      }
      deps.machine.transition(to, reason);
    },
    [deps.machine],
  );

  /**
   * Gecikme olcumu (ASU-016 / ASU-020).
   *
   * Reducer saf kalsin diye zaman damgalari burada islenir; olculen sure hem log'a
   * hem UI'ya duser. Log satiri ASU-020 maliyet/gecikme notunun kaynagidir.
   */
  const trackLatency = useCallback(
    (event: AsunaRealtimeEvent): void => {
      const { latency, now, log } = deps;

      switch (event.type) {
        case 'agent_thinking':
          latency.markSpeechEnd(now());
          return;

        case 'transcript':
          if (event.entry.role === 'user' && event.entry.status === 'completed') {
            latency.markSpeechEnd(now());
          }
          return;

        case 'agent_audio_started': {
          const latencyMs = latency.takeLatency(now());
          if (latencyMs === null) {
            return;
          }
          // ASU-064: olcum tek basina ise yaramaz — hangi tur-tespiti ayariyla
          // alindigi ayni satirda durmali ki once/sonra karsilastirilabilsin.
          const config = configRef.current;
          const vad = config === null ? 'unknown' : describeTurnDetection(config);
          log.info(
            `Yanit gecikmesi: ${latencyMs.toString()} ms (konusma sonu -> ilk ses) vad=${vad}`,
            { latencyMs, vad },
          );
          dispatch({ type: 'latency_measured', latencyMs });
          return;
        }

        // Kesilen ya da biten turun olcumu tasinmaz — sonraki tur temiz baslar.
        case 'agent_interrupted':
        case 'agent_audio_stopped':
        case 'turn_ended':
        case 'disconnected':
          latency.reset();
          return;

        case 'connecting':
        case 'connected':
        case 'reconnecting':
        case 'usage':
        case 'error':
        case 'tool_call_started':
        case 'tool_call_completed':
        case 'tool_approval_requested':
        case 'tool_approval_resolved':
        case 'unexpected_signal':
          return;
      }
    },
    [deps],
  );

  /**
   * Oturumu kapatir. Idempotent; acik oturum yoksa hicbir sey yapmaz (ASU-018).
   *
   * Kapanis servisin isi: `usage` raporlanir, `RTCPeerConnection` kapanir ve mikrofon
   * track'leri durur (mikrofonun sahibi SDK — voice.md Bolum 4 "Secenek A").
   */
  const closeSession = useCallback(
    (reason: string): void => {
      const service = serviceRef.current;
      connectedRef.current = false;
      if (service === null || !sessionOpenRef.current) {
        return;
      }
      sessionOpenRef.current = false;
      deps.log.info(`Oturum kapatiliyor (${reason}).`);
      service.disconnect();
    },
    [deps],
  );

  /**
   * Oturum acikken gelen hata: kaynak sizmasin diye oturum kapatilir (R1 — acik
   * kalan oturum fatura yazar), ama olay gizlenmez.
   *
   * `disconnect()` durumu idle'a aldigi icin hemen ardindan tekrar `ERROR`'a geciliyor:
   * log'da "kapandi -> hata" zinciri gorunur kalir ve kullanici ekranda `ERROR` gorur
   * (phase-1.md ASU-018). `ERROR` terminal degil — buton yeniden baglanma yolunu acar.
   */
  const handleSessionFailure = useCallback((): void => {
    if (!connectedRef.current) {
      // Baglanti asamasindaki hata: servis kendini zaten kapatti.
      return;
    }
    deps.log.warn('Oturum hata sonrasi kapatiliyor (acik baglanti birakilmiyor).');
    closeSession('error');
    transition('ERROR', 'ERROR_OCCURRED');
  }, [closeSession, deps, transition]);

  /**
   * Kalici oturum kaydi (ASU-032).
   *
   * `usage` event'i kapanistan hemen once gelir (ASU-013); `disconnected`
   * senkron olarak onu izler. Bu yuzden kullanim burada bir ref'te tutulur ve
   * kapanista tuketilir.
   *
   * TEMPORARY: kapanis tetigi su an Phase 1'in `disconnected` event'i. ASU-026
   * (idle timeout / wake word ile oturum kapanisi) geldiginde tetik oraya
   * tasinacak; `SessionRecorder` sozlesmesi degismeyecek.
   */
  const recordSessionEvent = useCallback(
    (event: AsunaRealtimeEvent): void => {
      const { recorder, transcriptCollector, now } = deps;

      switch (event.type) {
        case 'connected':
          transcriptCollector.reset();
          usageRef.current = null;
          // `projectId` Phase 4'te (ASU-039+) dolacak.
          recorder.begin(now());
          return;

        case 'transcript':
          transcriptCollector.observe(event.entry, now());
          return;

        case 'usage':
          usageRef.current = toSessionUsageInput(event.usage);
          return;

        case 'disconnected': {
          const usage = usageRef.current;
          usageRef.current = null;
          void recorder
            .end(now(), {
              ...(usage === null ? {} : { usage }),
              transcript: transcriptCollector.toInput(),
            })
            .then((outcome) => {
              if (outcome !== null && mountedRef.current) {
                dispatch({ type: 'session_recorded', outcome });
              }
            });
          return;
        }

        // Kalan event'ler kalici kayda girmiyor. Liste acikca yaziliyor:
        // yeni bir event turu eklendiginde "kaydedilmeli mi?" sorusu derleme
        // zamaninda sorulsun.
        case 'connecting':
        case 'reconnecting':
        case 'error':
        case 'agent_thinking':
        case 'agent_audio_started':
        case 'agent_audio_stopped':
        case 'agent_interrupted':
        case 'turn_ended':
        case 'tool_call_started':
        case 'tool_call_completed':
        case 'tool_approval_requested':
        case 'tool_approval_resolved':
        case 'unexpected_signal':
          return;
      }
    },
    [deps],
  );

  const handleEvent = useCallback(
    (event: AsunaRealtimeEvent): void => {
      if (event.type === 'connected') {
        connectedRef.current = true;
      }
      trackLatency(event);
      recordSessionEvent(event);
      dispatch({ type: 'realtime_event', event, atMs: deps.now() });

      if (event.type === 'disconnected') {
        connectedRef.current = false;
        sessionOpenRef.current = false;
        return;
      }
      if (event.type === 'error') {
        handleSessionFailure();
      }
    },
    [deps, handleSessionFailure, recordSessionEvent, trackLatency],
  );

  const ensureService = useCallback(
    (config: FrontendConfig): AsunaSessionPort => {
      const existing = serviceRef.current;
      if (existing !== null) {
        return existing;
      }

      const service = deps.createService({
        config,
        stateMachine: deps.machine,
        recorder: deps.recorder,
      });
      // Tek abonelik: servis hook'un omru boyunca yasar, her baglantida yeniden
      // abone olunmaz (ASU-018 listener leak kontrolu).
      unsubscribeRef.current = service.subscribe(handleEvent);
      serviceRef.current = service;
      return service;
    },
    [deps, handleEvent],
  );

  const ensureConfig = useCallback(async (): Promise<FrontendConfig> => {
    const cached = configRef.current;
    if (cached !== null) {
      return cached;
    }

    try {
      const config = await deps.loadConfig();
      configRef.current = config;
      return config;
    } catch (error) {
      deps.log.error('Config okunamadi; ses oturumu baslatilmiyor.', {
        detail: error instanceof Error ? error.message : String(error),
      });
      throw new AsunaSessionSetupError(
        'config_unavailable',
        'Frontend config okunamadi (get_frontend_config).',
      );
    }
  }, [deps]);

  const failActivation = useCallback(
    (error: unknown): void => {
      const userFacing = describeActivationError(error);
      deps.log.warn(`Baglanti kurulamadi: ${userFacing.message}`, {
        kind: userFacing.kind,
        retryable: userFacing.retryable,
      });
      transition(
        'ERROR',
        userFacing.kind === 'mic_permission_denied'
          ? 'MIC_PERMISSION_DENIED'
          : 'ERROR_OCCURRED',
      );
      dispatch({ type: 'activation_failed', error: userFacing });
    },
    [deps, transition],
  );

  const activate = useCallback(async (): Promise<void> => {
    // Cift tiklama yaris korumasi: ref, `busy` state'inin render dongusunu beklemez.
    if (busyRef.current || connectedRef.current) {
      return;
    }
    busyRef.current = true;
    setBusy(true);
    dispatch({ type: 'activation_started' });

    try {
      // TEMPORARY: ASU-023 wake word ile degistirilecek — bu gecis Phase 2'de
      // `WAKE_WORD_DETECTED` nedeniyle motordan gelecek.
      transition('WAKING', 'ACTIVATION_REQUESTED');

      const probe = await deps.probeMicrophone();
      deps.log.info('Mikrofon izni verildi.', {
        echoCancellation: probe.echoCancellation,
        noiseSuppression: probe.noiseSuppression,
      });
      if (probe.echoCancellation !== true) {
        // Self-interrupt (Asuna kendi sesiyle kendini kesmesi) bu asamanin en yaygin
        // tuzagi (phase-1.md ASU-016). Sessizce gecistirmiyoruz.
        deps.log.warn(
          'Echo cancellation dogrulanamadi — Asuna kendi sesiyle kendini kesebilir.',
          { echoCancellation: probe.echoCancellation },
        );
      }

      const config = await ensureConfig();
      const service = ensureService(config);
      // Bu noktadan sonra servis tarafinda kapatilmasi gereken bir sey var: baglanma
      // yarida kalirsa bile `disconnect()` akisi terk edip temizler (ASU-013).
      sessionOpenRef.current = true;
      await service.connect();
    } catch (error) {
      // Baglanti kurulamadi: servis kendi kaynagini zaten birakti.
      sessionOpenRef.current = false;
      failActivation(error);
    } finally {
      busyRef.current = false;
      if (mountedRef.current) {
        setBusy(false);
      }
    }
  }, [deps, ensureConfig, ensureService, failActivation, transition]);

  const start = useCallback((): void => {
    void activate();
  }, [activate]);

  const stop = useCallback((): void => {
    closeSession('user_stop');
  }, [closeSession]);

  const approveTool = useCallback(
    (requestId: string): void => {
      const service = serviceRef.current;
      if (service === null) {
        return;
      }
      deps.log.info('Tool cagrisi onaylandi.', { requestId });
      service.approveToolCall(requestId);
    },
    [deps],
  );

  const rejectTool = useCallback(
    (requestId: string): void => {
      const service = serviceRef.current;
      if (service === null) {
        return;
      }
      deps.log.info('Tool cagrisi reddedildi.', { requestId });
      service.rejectToolCall(requestId);
    },
    [deps],
  );

  /**
   * Kapanis kancalari (ASU-018).
   *
   * - Pencere kapanirken oturum kapatilir (acik oturum fatura yazar).
   * - Bilesen unmount olurken oturum kapatilir **ve** servis aboneligi sokulur;
   *   dinleyici birikmesi olmaz.
   */
  useEffect(() => {
    mountedRef.current = true;
    const detachCloseHandler = deps.registerCloseHandler(() => {
      closeSession('window_closed');
    });

    return (): void => {
      mountedRef.current = false;
      detachCloseHandler();
      closeSession('unmounted');
      unsubscribeRef.current?.();
      unsubscribeRef.current = null;
      serviceRef.current = null;
    };
  }, [closeSession, deps]);

  return {
    state,
    busy,
    connected: facts.connected,
    micActive: facts.connected,
    model: facts.model,
    activeTool: facts.activeTool,
    pendingApproval: facts.pendingApproval,
    error: facts.error,
    bargeIn: facts.bargeIn,
    lastLatencyMs: facts.lastLatencyMs,
    transcript: facts.transcript,
    sessionOutcome: facts.sessionOutcome,
    start,
    stop,
    approveTool,
    rejectTool,
  };
}

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
import type { AsunaRealtimeEvent, AsunaRealtimeEventListener } from './realtime-events';
import { AsunaRealtimeService } from './realtime-service';
import { probeMicrophoneAccess, type MicrophoneProbe } from '../audio/microphone-access';
import { loadFrontendConfig } from '../config/config.service';
import type { FrontendConfig } from '../config/frontend-config';
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
  subscribe(listener: AsunaRealtimeEventListener): () => void;
  getState(): VoiceState;
}

export interface AsunaSessionContext {
  readonly config: FrontendConfig;
  readonly stateMachine: VoiceStateMachine;
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
    retryable: info.retryable,
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
// Event -> UI olgulari
// ---------------------------------------------------------------------------

interface SessionFacts {
  /** Oturum acik mi — servisin `connected`/`disconnected` event'lerinden. */
  readonly connected: boolean;
  /** Hangi modelde konusuluyor (hard-code yok, event'ten gelir). */
  readonly model: string | null;
  /** Phase 5'te dolacak; Phase 1'de tool yok ama gorunurluk yolu hazir. */
  readonly activeTool: string | null;
  readonly error: UserFacingError | null;
}

const INITIAL_FACTS: SessionFacts = {
  connected: false,
  model: null,
  activeTool: null,
  error: null,
};

type SessionAction =
  | { readonly type: 'activation_started' }
  | { readonly type: 'activation_failed'; readonly error: UserFacingError }
  | { readonly type: 'realtime_event'; readonly event: AsunaRealtimeEvent };

function reduceRealtimeEvent(state: SessionFacts, event: AsunaRealtimeEvent): SessionFacts {
  switch (event.type) {
    case 'connecting':
      return { ...state, connected: false };

    case 'connected':
      return { ...state, connected: true, model: event.model, error: null };

    case 'reconnecting':
      // Sessiz retry yok: kullanici neden beklettigimizi gorsun.
      return { ...state, error: describeRealtimeFailure(event.error) };

    case 'disconnected':
      return { ...state, connected: false, activeTool: null };

    case 'error':
      return { ...state, error: describeRealtimeFailure(event.error) };

    case 'tool_call_started':
    case 'tool_approval_requested':
      return { ...state, activeTool: event.toolName };

    case 'tool_call_completed':
      return { ...state, activeTool: null };

    // Durum gecisleri FSM'den okunuyor; bu event'ler UI olgusu tasimiyor.
    case 'agent_thinking':
    case 'agent_audio_started':
    case 'agent_audio_stopped':
    case 'agent_interrupted':
    case 'turn_ended':
    case 'transcript':
    case 'usage':
    case 'unexpected_signal':
      return state;
  }
}

function reduceSession(state: SessionFacts, action: SessionAction): SessionFacts {
  switch (action.type) {
    case 'activation_started':
      return { ...state, error: null, activeTool: null };
    case 'activation_failed':
      return { ...state, connected: false, error: action.error };
    case 'realtime_event':
      return reduceRealtimeEvent(state, action.event);
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
  readonly error: UserFacingError | null;
  /** "Talk to Asuna" — izin -> config -> connect. Hata icerde yakalanir. */
  readonly start: () => void;
  /** "Stop" — oturumu kapatir. */
  readonly stop: () => void;
}

interface ResolvedDeps {
  readonly loadConfig: () => Promise<FrontendConfig>;
  readonly createService: AsunaSessionFactory;
  readonly probeMicrophone: () => Promise<MicrophoneProbe>;
  readonly machine: VoiceStateMachine;
  readonly log: AsunaLogger;
}

function defaultCreateService(context: AsunaSessionContext): AsunaSessionPort {
  return new AsunaRealtimeService({
    config: context.config,
    stateMachine: context.stateMachine,
  });
}

function resolveDeps(options: UseAsunaSessionOptions): ResolvedDeps {
  const log = options.logger ?? defaultLogger;
  return {
    loadConfig: options.loadConfig ?? loadFrontendConfig,
    createService: options.createService ?? defaultCreateService,
    probeMicrophone:
      options.probeMicrophone ?? ((): Promise<MicrophoneProbe> => probeMicrophoneAccess()),
    // Log'lu makine: gecisler ASU-019 formatinda kendiliginden akar.
    machine: options.stateMachine ?? createLoggedVoiceStateMachine(),
    log: log.child('voice-session'),
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
  const mountedRef = useRef(true);

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

  const handleEvent = useCallback(
    (event: AsunaRealtimeEvent): void => {
      if (event.type === 'connected') {
        connectedRef.current = true;
      } else if (event.type === 'disconnected') {
        connectedRef.current = false;
      }
      dispatch({ type: 'realtime_event', event });
    },
    [dispatch],
  );

  const ensureService = useCallback(
    (config: FrontendConfig): AsunaSessionPort => {
      const existing = serviceRef.current;
      if (existing !== null) {
        return existing;
      }

      const service = deps.createService({ config, stateMachine: deps.machine });
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

      const config = await ensureConfig();
      const service = ensureService(config);
      await service.connect();
    } catch (error) {
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
    const service = serviceRef.current;
    if (service === null) {
      return;
    }
    connectedRef.current = false;
    service.disconnect();
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    return (): void => {
      mountedRef.current = false;
    };
  }, []);

  return {
    state,
    busy,
    connected: facts.connected,
    micActive: facts.connected,
    model: facts.model,
    activeTool: facts.activeTool,
    error: facts.error,
    start,
    stop,
  };
}

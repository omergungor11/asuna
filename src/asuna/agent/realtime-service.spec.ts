/**
 * `AsunaRealtimeService` testleri (ASU-013).
 *
 * Gercek SDK **hic** kullanilmaz: servise sahte bir [`RealtimeSessionFactory`] enjekte
 * edilir. Test aga cikmaz, mikrofona dokunmaz (`conventions.md` — Testing).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AsunaRealtimeError } from './realtime-errors';
import type { AsunaRealtimeEvent, RealtimeUsageSnapshot } from './realtime-events';
import type {
  EphemeralApiKeyProvider,
  RealtimeSessionPort,
  RealtimeSessionSignal,
  RealtimeSessionSignalListener,
  RealtimeSessionSpec,
} from './realtime-session-port';
import {
  AsunaRealtimeService,
  TOOL_APPROVAL_TIMEOUT_MODEL_MESSAGE,
  TOOL_DENIED_MODEL_MESSAGE,
  TOOL_FAILURE_PREFIX,
  toApprovalArgumentsPreview,
  toModelOutput,
  toSdkTool,
  toTurnDetectionSpec,
  type AsunaRealtimeServiceOptions,
} from './realtime-service';
import type { EphemeralRealtimeToken } from './realtime-token';
import { TOOL_APPROVAL_MODES, type FrontendConfig } from '../config/frontend-config';
import { buildAsunaInstructions } from '../prompts';
import { VoiceStateMachine, type VoiceState } from '../state/voice-state-machine';
import { resolveApproval } from '../tools/approval-policy';
import { NO_TOOL_ARGUMENTS, type AsunaToolDefinition, type ToolResult } from '../tools/types';
import type { ToolAuditInput } from '../../shared/tool-event';

const CONFIG: FrontendConfig = {
  realtimeModel: 'gpt-realtime-2.1-mini',
  realtimeVoice: 'marin',
  wakeWord: 'Hey Asuna',
  wakeWordProvider: 'sherpa-kws',
  idleTimeoutSeconds: 45,
  logLevel: 'info',
  memoryEnabled: true,
  transcriptStorage: true,
  toolApprovalMode: 'safe',
  turnDetection: 'semantic_vad',
  vadEagerness: 'high',
  vadSilenceMs: 400,
};

const TOKEN: EphemeralRealtimeToken = {
  value: 'ek_TEST_TOKENI',
  expiresAt: 1_690_000_600,
  model: CONFIG.realtimeModel,
};

const USAGE: RealtimeUsageSnapshot = {
  requests: 2,
  inputTokens: 120,
  outputTokens: 340,
  totalTokens: 460,
  inputTokenDetails: [{ audio_tokens: 100, cached_tokens: 20 }],
  outputTokenDetails: [{ audio_tokens: 340 }],
};

interface FakeSession {
  readonly spec: RealtimeSessionSpec;
  readonly emit: RealtimeSessionSignalListener;
  readonly port: RealtimeSessionPort;
  /** SDK'ya iletilen onaylar (ASU-048). */
  readonly approvals: string[];
  /** SDK'ya iletilen retler; `reason` modele giden metin. */
  readonly rejections: { readonly requestId: string; readonly reason?: string }[];
  apiKeyProvider: EphemeralApiKeyProvider | null;
  connectCalls: number;
  closeCalls: number;
  interruptCalls: number;
}

interface HarnessOptions {
  readonly config?: FrontendConfig;
  /** Deneme basina baglanti davranisi; bittiyse son eleman tekrarlanir. */
  readonly connectBehaviours?: readonly (() => Promise<void>)[];
  /** Fabrika kurucu asamasinda patlasin (WebRTC yok senaryosu). */
  readonly factoryError?: Error;
  readonly mintToken?: () => Promise<EphemeralRealtimeToken>;
  readonly usage?: RealtimeUsageSnapshot;
  readonly closeError?: Error;
  /** SDK onay iletimi patlasin (ASU-048 kanit geri alma yolu). */
  readonly approveError?: Error;
  readonly service?: Partial<AsunaRealtimeServiceOptions>;
}

/**
 * Tauri `invoke` bir `Error` ile degil, serilestirilmis `{ kind, message }` nesnesiyle
 * reddeder (`src-tauri/src/realtime_token.rs`). Test bunu **birebir** taklit ediyor;
 * Error'a sarmak gercek davranisi gizler ve `describeTokenError`'in asil yolunu atlar.
 */
function rejectAsIpcError(kind: string, message: string): Promise<EphemeralRealtimeToken> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- gercek IPC hatasi duz nesnedir
  return Promise.reject({ kind, message });
}

interface Harness {
  readonly service: AsunaRealtimeService;
  readonly machine: VoiceStateMachine;
  readonly events: AsunaRealtimeEvent[];
  readonly states: VoiceState[];
  readonly sessions: FakeSession[];
  readonly mint: () => number;
  /** Son acilan oturuma sinyal gonderir. */
  readonly emit: (signal: RealtimeSessionSignal) => void;
}

function createHarness(options: HarnessOptions = {}): Harness {
  const config = options.config ?? CONFIG;
  const sessions: FakeSession[] = [];
  const events: AsunaRealtimeEvent[] = [];
  const states: VoiceState[] = [];
  let mintCalls = 0;

  const machine = new VoiceStateMachine();
  machine.subscribe((transition) => {
    states.push(transition.to);
  });

  const service = new AsunaRealtimeService({
    config,
    stateMachine: machine,
    sleep: (): Promise<void> => Promise.resolve(),
    mintToken:
      options.mintToken ??
      ((): Promise<EphemeralRealtimeToken> => {
        mintCalls += 1;
        return Promise.resolve(TOKEN);
      }),
    createSession: (spec, onSignal): RealtimeSessionPort => {
      if (options.factoryError !== undefined) {
        throw options.factoryError;
      }

      const index = sessions.length;
      const session: FakeSession = {
        spec,
        emit: onSignal,
        approvals: [],
        rejections: [],
        apiKeyProvider: null,
        connectCalls: 0,
        closeCalls: 0,
        interruptCalls: 0,
        port: {
          connect: async ({ apiKey }): Promise<void> => {
            session.apiKeyProvider = apiKey;
            session.connectCalls += 1;
            const behaviours = options.connectBehaviours;
            if (behaviours === undefined || behaviours.length === 0) {
              await apiKey();
              return;
            }
            const behaviour = behaviours[Math.min(index, behaviours.length - 1)];
            await behaviour?.();
          },
          close: (): void => {
            session.closeCalls += 1;
            if (options.closeError !== undefined) {
              throw options.closeError;
            }
          },
          interrupt: (): void => {
            session.interruptCalls += 1;
          },
          approve: (requestId: string): Promise<void> => {
            session.approvals.push(requestId);
            return options.approveError === undefined
              ? Promise.resolve()
              : Promise.reject(options.approveError);
          },
          reject: (requestId: string, reason?: string): Promise<void> => {
            session.rejections.push({ requestId, ...(reason === undefined ? {} : { reason }) });
            return Promise.resolve();
          },
          usage: (): RealtimeUsageSnapshot => options.usage ?? USAGE,
        },
      };
      sessions.push(session);
      return session.port;
    },
    ...options.service,
  });

  service.subscribe((event) => {
    events.push(event);
  });

  return {
    service,
    machine,
    events,
    states,
    sessions,
    mint: (): number => mintCalls,
    emit: (signal): void => {
      const session = sessions.at(-1);
      if (session === undefined) {
        throw new Error('Test kurgusu: acik oturum yok.');
      }
      session.emit(signal);
    },
  };
}

function eventTypes(events: readonly AsunaRealtimeEvent[]): string[] {
  return events.map((event) => event.type);
}

// ---------------------------------------------------------------------------

describe('toTurnDetectionSpec (ASU-064)', () => {
  it('her acikgozluluk seviyesini oldugu gibi tasir', () => {
    for (const eagerness of ['auto', 'low', 'medium', 'high'] as const) {
      expect(toTurnDetectionSpec({ ...CONFIG, vadEagerness: eagerness })).toEqual({
        type: 'semantic_vad',
        eagerness,
      });
    }
  });

  it('server_vad`de sessizlik penceresi SDK alan adina cevrilir', () => {
    expect(
      toTurnDetectionSpec({ ...CONFIG, turnDetection: 'server_vad', vadSilenceMs: 1200 }),
    ).toEqual({ type: 'server_vad', silenceDurationMs: 1200 });
  });
});

// ---------------------------------------------------------------------------

describe('AsunaRealtimeService — yasam dongusu', () => {
  it('connect() durumu BOOTING -> WAKING -> CONNECTING -> LISTENING yapar', async () => {
    const harness = createHarness();

    expect(harness.service.getState()).toBe('BOOTING');
    await harness.service.connect();

    expect(harness.states).toEqual(['WAKING', 'CONNECTING', 'LISTENING']);
    expect(harness.service.getState()).toBe('LISTENING');
    expect(eventTypes(harness.events)).toEqual(['connecting', 'connected']);
  });

  it('cagiran taraf zaten WAKING durumundaysa adimi tekrarlamaz', async () => {
    const harness = createHarness();
    harness.machine.transition('WAKING', 'ACTIVATION_REQUESTED');
    harness.states.length = 0;

    await harness.service.connect();

    expect(harness.states).toEqual(['CONNECTING', 'LISTENING']);
  });

  it('oturum spec`i config`ten geliyor; model hard-code degil', async () => {
    const harness = createHarness();
    await harness.service.connect();

    const spec = harness.sessions[0]?.spec;
    expect(spec).toBeDefined();
    expect(spec?.model).toBe(CONFIG.realtimeModel);
    expect(spec?.voice).toBe(CONFIG.realtimeVoice);
    expect(spec?.transcription).toBe(true);
    expect(spec?.tools).toEqual([]);
    expect(spec?.instructions).toBe(buildAsunaInstructions());
  });

  /**
   * ASU-035: oturum baglami her `connect()` oncesi **taze** uretilir. Servis
   * omru boyunca sabit bir talimat, ikinci oturumda eski hafizayi enjekte
   * etmek demekti (ozet + cikarim kapanista calisiyor).
   */
  it('talimat her baglantida yeniden uretilir (baglam enjeksiyonu)', async () => {
    let call = 0;
    const harness = createHarness({
      service: {
        prepareInstructions: (): Promise<string> => {
          call += 1;
          return Promise.resolve(`TALIMAT-${call.toString()}`);
        },
      },
    });

    await harness.service.connect();
    harness.service.disconnect();
    await harness.service.connect();

    expect(harness.sessions[0]?.spec.instructions).toBe('TALIMAT-1');
    expect(harness.sessions[1]?.spec.instructions).toBe('TALIMAT-2');
  });

  /** Retry'lar ayni talimatla denenir: baglam oturum basina bir kez cekilir. */
  it('yeniden denemede baglam tekrar cekilmez', async () => {
    let call = 0;
    const harness = createHarness({
      connectBehaviours: [
        (): Promise<void> => Promise.reject(new Error('gecici hata')),
        (): Promise<void> => Promise.resolve(),
      ],
      service: {
        prepareInstructions: (): Promise<string> => {
          call += 1;
          return Promise.resolve(`TALIMAT-${call.toString()}`);
        },
      },
    });

    await harness.service.connect();

    expect(call).toBe(1);
    expect(harness.sessions[1]?.spec.instructions).toBe('TALIMAT-1');
  });

  /**
   * Baglam uretimi patlarsa konusma **bloklanmaz**: cekirdek talimatla devam
   * edilir ve olay gorunur kalir (sessiz yutma yok).
   */
  it('baglam uretimi patlarsa cekirdek talimatla devam eder', async () => {
    const harness = createHarness({
      service: {
        prepareInstructions: (): Promise<string> => Promise.reject(new Error('baglam yok')),
      },
    });

    await harness.service.connect();

    expect(harness.sessions[0]?.spec.instructions).toBe(buildAsunaInstructions());
    expect(eventTypes(harness.events)).toEqual(['error', 'connecting', 'connected']);
  });

  it('turn detection semantic_vad config`inden kuruluyor (ASU-064)', async () => {
    const harness = createHarness({
      config: { ...CONFIG, turnDetection: 'semantic_vad', vadEagerness: 'low' },
    });
    await harness.service.connect();

    expect(harness.sessions[0]?.spec.turnDetection).toEqual({
      type: 'semantic_vad',
      eagerness: 'low',
    });
  });

  it('turn detection server_vad secilince sessizlik penceresi tasiniyor (ASU-064)', async () => {
    const harness = createHarness({
      config: {
        ...CONFIG,
        turnDetection: 'server_vad',
        vadEagerness: 'high',
        vadSilenceMs: 250,
      },
    });
    await harness.service.connect();

    // `eagerness` server_vad'de anlamsiz: config'te dolu olsa bile spec'e sizmaz.
    expect(harness.sessions[0]?.spec.turnDetection).toEqual({
      type: 'server_vad',
      silenceDurationMs: 250,
    });
  });

  it('transcriptStorage kapaliyken transkripsiyon istenmiyor', async () => {
    const harness = createHarness({ config: { ...CONFIG, transcriptStorage: false } });
    await harness.service.connect();

    expect(harness.sessions[0]?.spec.transcription).toBe(false);
  });

  /**
   * ASU-037 / Gate 3 MEDIUM-3: transkript anahtari **calisma zamaninda**
   * kapatilabilir. Boot config'i acik olsa bile bir sonraki oturumda
   * transkripsiyon kurulmamali — yeniden baslatma beklenmiyor.
   */
  it('calisma zamani transkript anahtari kapaliysa transkripsiyon kurulmaz', async () => {
    let enabled = true;
    const harness = createHarness({
      service: { resolveTranscription: (): Promise<boolean> => Promise.resolve(enabled) },
    });

    await harness.service.connect();
    expect(harness.sessions[0]?.spec.transcription).toBe(true);

    harness.service.disconnect();
    enabled = false;
    await harness.service.connect();

    expect(harness.sessions[1]?.spec.transcription).toBe(false);
  });

  /** Anahtar okunamazsa transkripsiyon **kapali** kurulur; hata yutulmaz. */
  it('gizlilik durumu okunamazsa transkripsiyon kapali kalir ve hata gorunur', async () => {
    const harness = createHarness({
      service: {
        resolveTranscription: (): Promise<boolean> =>
          Promise.reject(new Error('gizlilik durumu okunamadi')),
      },
    });

    await harness.service.connect();

    expect(harness.sessions[0]?.spec.transcription).toBe(false);
    expect(eventTypes(harness.events)).toContain('error');
  });

  /** Acilista kapali olan anahtar calisma zamaninda **acilamaz** (tavan kurali). */
  it('acilista kapali transkript anahtari calisma zamaninda acilamaz', async () => {
    const harness = createHarness({
      config: { ...CONFIG, transcriptStorage: false },
      service: { resolveTranscription: (): Promise<boolean> => Promise.resolve(true) },
    });

    await harness.service.connect();

    expect(harness.sessions[0]?.spec.transcription).toBe(false);
  });

  it('token lazy uretiliyor: oturum kurulurken degil, SDK isteyince', async () => {
    const behaviour = vi.fn<() => Promise<void>>(() => Promise.resolve());
    const harness = createHarness({ connectBehaviours: [behaviour] });

    await harness.service.connect();

    // `connect()` cagrildi ama SDK apiKey'i istemedi -> token basilmadi.
    expect(harness.mint()).toBe(0);

    const provider = harness.sessions[0]?.apiKeyProvider;
    expect(provider).toBeDefined();
    await expect(provider?.()).resolves.toBe(TOKEN.value);
    expect(harness.mint()).toBe(1);
  });

  it('disconnect() usage raporlar, oturumu kapatir ve idle duruma doner', async () => {
    const harness = createHarness();
    await harness.service.connect();
    harness.events.length = 0;

    harness.service.disconnect();

    expect(eventTypes(harness.events)).toEqual(['usage', 'disconnected']);
    expect(harness.events[0]).toEqual({ type: 'usage', usage: USAGE });
    expect(harness.events[1]).toEqual({ type: 'disconnected', reason: 'requested' });
    expect(harness.sessions[0]?.closeCalls).toBe(1);
    expect(harness.service.getState()).toBe('BOOTING');
  });

  it('idleState IDLE_WAKE_WORD olarak verilebilir (Phase 2 hazirligi)', async () => {
    const harness = createHarness({ service: { idleState: 'IDLE_WAKE_WORD' } });
    await harness.service.connect();
    harness.service.disconnect();

    expect(harness.service.getState()).toBe('IDLE_WAKE_WORD');
  });

  it('disconnect() idempotent — ikinci cagri hicbir sey yaymaz', async () => {
    const harness = createHarness();
    await harness.service.connect();
    harness.service.disconnect();
    harness.events.length = 0;

    harness.service.disconnect();

    expect(harness.events).toEqual([]);
    expect(harness.sessions[0]?.closeCalls).toBe(1);
  });

  it('es zamanli connect() cagrilari tek oturum acar', async () => {
    const harness = createHarness();

    await Promise.all([harness.service.connect(), harness.service.connect()]);

    expect(harness.sessions).toHaveLength(1);
    expect(eventTypes(harness.events)).toEqual(['connecting', 'connected']);
  });

  it('baglanma sirasinda disconnect() cagrilirsa acilan oturum arkada birakilmaz', async () => {
    let release = (): void => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const harness = createHarness({ connectBehaviours: [(): Promise<void> => gate] });

    const connecting = harness.service.connect();
    harness.service.disconnect();
    release();
    await connecting;

    expect(harness.sessions[0]?.closeCalls).toBe(1);
    expect(harness.service.getState()).toBe('BOOTING');
    expect(eventTypes(harness.events)).toEqual(['connecting', 'disconnected']);
  });

  it('interrupt() oturuma iletilir', async () => {
    const harness = createHarness();
    await harness.service.connect();

    harness.service.interrupt();

    expect(harness.sessions[0]?.interruptCalls).toBe(1);
  });

  it('acik oturum yokken interrupt() sessizce yutulmaz', () => {
    const harness = createHarness();

    harness.service.interrupt();

    expect(harness.events).toEqual([
      { type: 'unexpected_signal', signal: 'interrupt', state: 'BOOTING' },
    ]);
  });

  it('close() hatasi kapanisi engellemez ama gorunur olur', async () => {
    const harness = createHarness({ closeError: new Error('transport zaten kapali') });
    await harness.service.connect();
    harness.events.length = 0;

    harness.service.disconnect();

    expect(eventTypes(harness.events)).toEqual(['usage', 'error', 'disconnected']);
    expect(harness.service.getState()).toBe('BOOTING');
  });
});

// ---------------------------------------------------------------------------

describe('AsunaRealtimeService — SDK sinyali -> durum eslemesi', () => {
  let harness: Harness;

  beforeEach(async () => {
    harness = createHarness();
    await harness.service.connect();
    harness.events.length = 0;
    harness.states.length = 0;
  });

  it('agent_start -> ASSISTANT_THINKING', () => {
    harness.emit({ type: 'agent_start' });

    expect(harness.service.getState()).toBe('ASSISTANT_THINKING');
    expect(harness.events).toEqual([{ type: 'agent_thinking' }]);
  });

  it('audio_start -> ASSISTANT_SPEAKING', () => {
    harness.emit({ type: 'agent_start' });
    harness.emit({ type: 'audio_start' });

    expect(harness.service.getState()).toBe('ASSISTANT_SPEAKING');
    expect(eventTypes(harness.events)).toEqual(['agent_thinking', 'agent_audio_started']);
  });

  it('audio_stopped -> LISTENING', () => {
    harness.emit({ type: 'agent_start' });
    harness.emit({ type: 'audio_start' });
    harness.emit({ type: 'audio_stopped' });

    expect(harness.service.getState()).toBe('LISTENING');
    expect(harness.states).toEqual(['ASSISTANT_THINKING', 'ASSISTANT_SPEAKING', 'LISTENING']);
  });

  it('audio_interrupted (barge-in) -> USER_SPEAKING', () => {
    harness.emit({ type: 'agent_start' });
    harness.emit({ type: 'audio_start' });
    harness.emit({ type: 'audio_interrupted' });

    expect(harness.service.getState()).toBe('USER_SPEAKING');
    expect(harness.events.at(-1)).toEqual({ type: 'agent_interrupted' });
  });

  it('agent_end durum degistirmez, yalnizca tur bitisini bildirir', () => {
    harness.emit({ type: 'agent_start' });
    harness.states.length = 0;

    harness.emit({ type: 'agent_end' });

    expect(harness.states).toEqual([]);
    expect(harness.events.at(-1)).toEqual({ type: 'turn_ended' });
  });

  it('error -> ERROR durumu ve gorunur hata', () => {
    harness.emit({
      type: 'error',
      error: {
        kind: 'session',
        cause: 'TransportError',
        message: 'kanal koptu',
        retryable: false,
      },
    });

    expect(harness.service.getState()).toBe('ERROR');
    expect(harness.events).toEqual([
      {
        type: 'error',
        error: {
          kind: 'session',
          cause: 'TransportError',
          message: 'kanal koptu',
          retryable: false,
        },
      },
    ]);
  });

  it('tekrarlanan sinyal (ayni hedef durum) gurultu uretmez', () => {
    harness.emit({ type: 'agent_start' });
    harness.states.length = 0;

    harness.emit({ type: 'agent_start' });

    expect(harness.states).toEqual([]);
    expect(harness.service.getState()).toBe('ASSISTANT_THINKING');
  });

  it('gecersiz sirali sinyal durumu bozmaz ama yutulmaz da', () => {
    // LISTENING -> ASSISTANT_SPEAKING kenari tabloda yok (once THINKING gelmeli).
    harness.emit({ type: 'audio_start' });

    expect(harness.service.getState()).toBe('LISTENING');
    // Durum degismedi ama iki sey de gorunur: hem beklenmeyen sinyal, hem sinyalin
    // kendisi. Hicbiri sessizce dusurulmuyor.
    expect(harness.events).toEqual([
      { type: 'unexpected_signal', signal: 'audio_start', state: 'LISTENING' },
      { type: 'agent_audio_started' },
    ]);
  });

  /** ASU-044'ten beri bu yol gercekten kullaniliyor (`get_current_project`). */
  it('tool sinyalleri TOOL_PENDING / AWAITING_APPROVAL yoluna baglanir', () => {
    harness.emit({ type: 'agent_start' });
    harness.emit({ type: 'tool_start', toolName: 'get_current_project' });
    expect(harness.service.getState()).toBe('TOOL_PENDING');

    harness.emit({
      type: 'tool_approval_requested',
      toolName: 'get_current_project',
      requestId: 'call_1',
      argumentsJson: null,
    });
    expect(harness.service.getState()).toBe('AWAITING_APPROVAL');

    harness.emit({ type: 'tool_end', toolName: 'get_current_project' });
    expect(harness.service.getState()).toBe('ASSISTANT_THINKING');

    expect(eventTypes(harness.events)).toEqual([
      'agent_thinking',
      'tool_call_started',
      'tool_approval_requested',
      'tool_call_completed',
    ]);
  });
});

// ---------------------------------------------------------------------------

describe('AsunaRealtimeService — transkript', () => {
  it('history sinyalini normalize transcript event`ine cevirir', async () => {
    const harness = createHarness();
    await harness.service.connect();
    harness.events.length = 0;

    harness.emit({
      type: 'history',
      entries: [{ itemId: 'i1', role: 'user', text: 'merhaba', status: 'completed' }],
    });

    expect(harness.events).toEqual([
      {
        type: 'transcript',
        entry: { itemId: 'i1', role: 'user', text: 'merhaba', status: 'completed' },
      },
    ]);
  });

  it('degismeyen satiri tekrar yaymaz, degiseni yayar', async () => {
    const harness = createHarness();
    await harness.service.connect();
    harness.events.length = 0;

    const partial = { itemId: 'i1', role: 'user', text: 'mer', status: 'in_progress' } as const;
    harness.emit({ type: 'history', entries: [partial] });
    harness.emit({ type: 'history', entries: [partial] });
    harness.emit({
      type: 'history',
      entries: [{ itemId: 'i1', role: 'user', text: 'merhaba', status: 'completed' }],
    });

    expect(harness.events).toHaveLength(2);
    expect(harness.events.at(-1)).toMatchObject({
      type: 'transcript',
      entry: { text: 'merhaba', status: 'completed' },
    });
  });

  it('yeni oturum dokum onbellegini sifirlar', async () => {
    const harness = createHarness();
    const entry = { itemId: 'i1', role: 'user', text: 'merhaba', status: 'completed' } as const;

    await harness.service.connect();
    harness.emit({ type: 'history', entries: [entry] });
    harness.service.disconnect();

    await harness.service.connect();
    harness.events.length = 0;
    harness.emit({ type: 'history', entries: [entry] });

    expect(eventTypes(harness.events)).toEqual(['transcript']);
  });
});

// ---------------------------------------------------------------------------

describe('AsunaRealtimeService — hata ve yeniden baglanma', () => {
  it('token hatasi ERROR durumuna ve Rust`un durust mesajina cevrilir', async () => {
    const harness = createHarness({
      mintToken: (): Promise<EphemeralRealtimeToken> =>
        rejectAsIpcError(
          'invalid_api_key',
          'OpenAI API anahtari gecersiz (yetkilendirme reddedildi).',
        ),
    });

    await expect(harness.service.connect()).rejects.toBeInstanceOf(AsunaRealtimeError);

    expect(harness.service.getState()).toBe('ERROR');
    const last = harness.events.at(-1);
    expect(last).toMatchObject({
      type: 'error',
      error: {
        kind: 'token',
        cause: 'invalid_api_key',
        message: 'OpenAI API anahtari gecersiz (yetkilendirme reddedildi).',
        retryable: false,
      },
    });
  });

  it('kalici token hatasinda yeniden denemez', async () => {
    const harness = createHarness({
      mintToken: (): Promise<EphemeralRealtimeToken> =>
        rejectAsIpcError('invalid_api_key', 'gecersiz anahtar'),
    });

    await expect(harness.service.connect()).rejects.toBeInstanceOf(AsunaRealtimeError);

    expect(harness.sessions).toHaveLength(1);
    expect(eventTypes(harness.events)).toEqual(['connecting', 'error']);
  });

  it('gecici hatada sinirli sayida yeniden dener ve pes eder', async () => {
    const harness = createHarness({
      mintToken: (): Promise<EphemeralRealtimeToken> =>
        rejectAsIpcError('network', 'OpenAI`ya ulasamadim.'),
    });

    await expect(harness.service.connect()).rejects.toBeInstanceOf(AsunaRealtimeError);

    expect(harness.sessions).toHaveLength(3);
    expect(eventTypes(harness.events)).toEqual([
      'connecting',
      'reconnecting',
      'connecting',
      'reconnecting',
      'connecting',
      'error',
    ]);
    expect(harness.events[1]).toMatchObject({
      type: 'reconnecting',
      attempt: 2,
      maxAttempts: 3,
    });
    expect(harness.service.getState()).toBe('ERROR');
  });

  it('ikinci denemede basarili olursa oturum acilir', async () => {
    const harness = createHarness({
      connectBehaviours: [
        (): Promise<void> => Promise.reject(new Error('SDP gonderilemedi')),
        (): Promise<void> => Promise.resolve(),
      ],
    });

    await harness.service.connect();

    expect(harness.service.getState()).toBe('LISTENING');
    expect(eventTypes(harness.events)).toEqual([
      'connecting',
      'reconnecting',
      'connecting',
      'connected',
    ]);
    // Basarisiz denemenin oturumu kapatildi.
    expect(harness.sessions[0]?.closeCalls).toBe(1);
  });

  it('WebRTC olmayan ortamda kurucu hatasi `unsupported` olarak siniflanir', async () => {
    const harness = createHarness({ factoryError: new Error('WebRTC is not supported') });

    await expect(harness.service.connect()).rejects.toBeInstanceOf(AsunaRealtimeError);

    expect(harness.events.at(-1)).toMatchObject({
      type: 'error',
      error: { kind: 'unsupported', retryable: false },
    });
    // Yeniden denenmedi.
    expect(eventTypes(harness.events)).toEqual(['connecting', 'error']);
  });

  it('token modeli config modeliyle uyusmazsa baglanmaz', async () => {
    const harness = createHarness({
      mintToken: (): Promise<EphemeralRealtimeToken> =>
        Promise.resolve({ ...TOKEN, model: 'gpt-realtime-2.1' }),
    });

    await expect(harness.service.connect()).rejects.toBeInstanceOf(AsunaRealtimeError);

    expect(harness.events.at(-1)).toMatchObject({
      type: 'error',
      error: { kind: 'internal', cause: 'model_mismatch', retryable: false },
    });
  });

  it('baglanti hatasi mesajindaki token gorunumlu parcalar redakte edilir', async () => {
    const harness = createHarness({
      connectBehaviours: [
        (): Promise<void> => Promise.reject(new Error('401 for key ek_SIZAN_DEGER')),
      ],
      service: { maxConnectAttempts: 1 },
    });

    await expect(harness.service.connect()).rejects.toBeInstanceOf(AsunaRealtimeError);

    const last = harness.events.at(-1);
    const message = last?.type === 'error' ? last.error.message : '';
    expect(message).not.toContain('ek_SIZAN_DEGER');
    expect(message).toContain('ek_<redacted>');
  });

  it('hatadan sonra yeniden baglanilabilir (ERROR terminal degil)', async () => {
    let failNext = true;
    const harness = createHarness({
      mintToken: (): Promise<EphemeralRealtimeToken> => {
        if (failNext) {
          failNext = false;
          return rejectAsIpcError('quota_exceeded', 'kota doldu');
        }
        return Promise.resolve(TOKEN);
      },
    });

    await expect(harness.service.connect()).rejects.toBeInstanceOf(AsunaRealtimeError);
    expect(harness.service.getState()).toBe('ERROR');

    await harness.service.connect();
    expect(harness.service.getState()).toBe('LISTENING');
  });
});

// ---------------------------------------------------------------------------

describe('AsunaRealtimeService — abonelik', () => {
  it('abonelik iptal edilebilir', async () => {
    const harness = createHarness();
    const seen: AsunaRealtimeEvent[] = [];
    const unsubscribe = harness.service.subscribe((event) => {
      seen.push(event);
    });

    await harness.service.connect();
    unsubscribe();
    harness.service.disconnect();

    expect(eventTypes(seen)).toEqual(['connecting', 'connected']);
  });

  it('bir abonenin hatasi digerlerini ve oturumu dusurmez', async () => {
    const listenerErrors: unknown[] = [];
    const harness = createHarness({
      service: {
        onListenerError: (error): void => {
          listenerErrors.push(error);
        },
      },
    });

    const seen: AsunaRealtimeEvent[] = [];
    harness.service.subscribe(() => {
      throw new Error('bozuk UI paneli');
    });
    harness.service.subscribe((event) => {
      seen.push(event);
    });

    await harness.service.connect();

    expect(harness.service.getState()).toBe('LISTENING');
    expect(eventTypes(seen)).toEqual(['connecting', 'connected']);
    expect(listenerErrors).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// Onay akisi (ASU-048)
// ---------------------------------------------------------------------------

/** Onay isteyen ornek tool: risk 2, yani mod ne olursa olsun onay ister. */
const RISKY_TOOL: AsunaToolDefinition = {
  name: 'edit_project_file',
  description: 'Kayitli proje kokundeki bir dosyayi duzenler.',
  risk: 2,
  requiresApproval: true,
  timeoutMs: 10_000,
  parameters: NO_TOOL_ARGUMENTS,
  execute: (): Promise<ToolResult> => Promise.resolve({ ok: true, summary: 'yazildi' }),
};

interface ApprovalHarness extends Harness {
  readonly audits: ToolAuditInput[];
}

function createApprovalHarness(
  overrides: Partial<AsunaRealtimeServiceOptions> = {},
  harnessOptions: HarnessOptions = {},
): ApprovalHarness {
  const audits: ToolAuditInput[] = [];
  const harness = createHarness({
    ...harnessOptions,
    service: {
      tools: [RISKY_TOOL],
      // Audit yazimi IPC'ye cikmaz: test aga/Tauri'ye dokunmaz.
      recordToolEvent: (input): void => void audits.push(input),
      approvalTimeoutMs: 1_000,
      ...overrides,
    },
  });
  return { ...harness, audits };
}

function requestApproval(
  harness: Harness,
  requestId = 'call_1',
  argumentsJson = '{"path":"README.md"}',
): void {
  harness.emit({ type: 'agent_start' });
  harness.emit({
    type: 'tool_approval_requested',
    toolName: RISKY_TOOL.name,
    requestId,
    argumentsJson,
  });
}

describe('AsunaRealtimeService — onay akisi (ASU-048)', () => {
  it('onay istegi AWAITING_APPROVAL durumunu ve karti besleyen event"i uretiyor', async () => {
    const harness = createApprovalHarness();
    await harness.service.connect();
    harness.events.length = 0;

    requestApproval(harness);

    expect(harness.service.getState()).toBe('AWAITING_APPROVAL');
    // Kart "izin ver?" demiyor: ne yapilacagini gosteriyor (security.md Bolum 3).
    expect(harness.events.at(-1)).toEqual({
      type: 'tool_approval_requested',
      requestId: 'call_1',
      toolName: 'edit_project_file',
      description: RISKY_TOOL.description,
      risk: 2,
      argumentsPreview: 'path=README.md',
      timeoutMs: 1_000,
    });
  });

  it('onaylandiginda SDK"ya iletiliyor ve sonuc duyuruluyor', async () => {
    const harness = createApprovalHarness();
    await harness.service.connect();
    requestApproval(harness);
    harness.events.length = 0;

    harness.service.approveToolCall('call_1');
    await Promise.resolve();

    expect(harness.sessions.at(-1)?.approvals).toEqual(['call_1']);
    expect(harness.events).toEqual([
      {
        type: 'tool_approval_resolved',
        requestId: 'call_1',
        toolName: 'edit_project_file',
        outcome: 'approved',
      },
    ]);
    // Onaylanan cagri calisacak ve kendi audit satirini `executeTool` yazacak;
    // burada ikinci bir satir yazilmiyor.
    expect(harness.audits).toEqual([]);
  });

  it('reddedildiginde model reddi ogreniyor ve defter `denied` yaziyor', async () => {
    const harness = createApprovalHarness();
    await harness.service.connect();
    requestApproval(harness);
    harness.events.length = 0;

    harness.service.rejectToolCall('call_1');

    expect(harness.sessions.at(-1)?.rejections).toEqual([
      { requestId: 'call_1', reason: TOOL_DENIED_MODEL_MESSAGE },
    ]);
    expect(harness.audits).toEqual([
      {
        toolName: 'edit_project_file',
        riskLevel: 2,
        arguments: { path: 'README.md' },
        approvalState: 'denied',
        resultSummary: TOOL_DENIED_MODEL_MESSAGE,
      },
    ]);
    // Reddedilen tool calismaz; model cevabina doner.
    expect(harness.service.getState()).toBe('ASSISTANT_THINKING');
    expect(harness.events.at(-1)).toEqual({
      type: 'tool_approval_resolved',
      requestId: 'call_1',
      toolName: 'edit_project_file',
      outcome: 'denied',
    });
  });

  /** phase-5.md ASU-048: "Onay zaman asimina ugrarsa tool calismiyor". */
  it('sure dolunca otomatik reddediliyor (varsayilan reddet)', async () => {
    vi.useFakeTimers();
    try {
      const harness = createApprovalHarness();
      await harness.service.connect();
      requestApproval(harness);
      harness.events.length = 0;

      await vi.advanceTimersByTimeAsync(1_000);

      expect(harness.sessions.at(-1)?.rejections).toEqual([
        { requestId: 'call_1', reason: TOOL_APPROVAL_TIMEOUT_MODEL_MESSAGE },
      ]);
      expect(harness.audits.at(0)?.approvalState).toBe('timeout');
      expect(harness.events.at(-1)).toMatchObject({
        type: 'tool_approval_resolved',
        outcome: 'timeout',
      });
      // Onay kaniti verilmedi: tool calistirilamaz (asagidaki kapi testi).
      const gate = harness.sessions.at(-1)?.spec.toolRuntime.approvalGate;
      expect(await gate?.(RISKY_TOOL, {})).toBe('denied');
    } finally {
      vi.useRealTimers();
    }
  });

  /**
   * Onay **kaniti** tek bir cagriyi gecirir: "hepsine izin ver" MVP'de yok.
   * Ikinci cagri ayni onaydan faydalanamaz.
   */
  it('onay kaniti tek cagri icin gecerli, ikincisi reddediliyor', async () => {
    const harness = createApprovalHarness();
    await harness.service.connect();
    requestApproval(harness);
    const gate = harness.sessions.at(-1)?.spec.toolRuntime.approvalGate;

    harness.service.approveToolCall('call_1');

    expect(await gate?.(RISKY_TOOL, {})).toBe('approved');
    expect(await gate?.(RISKY_TOOL, {})).toBe('denied');
  });

  /** Onay akisini atlayan bir cagri kapiyi gecemez — varsayilan reddet. */
  it('onaysiz gelen cagriyi kapi reddediyor', async () => {
    const harness = createApprovalHarness();
    await harness.service.connect();

    const gate = harness.sessions.at(-1)?.spec.toolRuntime.approvalGate;

    expect(await gate?.(RISKY_TOOL, {})).toBe('denied');
  });

  it('SDK onayi patlarsa kanit geri aliniyor ve hata gorunur oluyor', async () => {
    const harness = createApprovalHarness({}, { approveError: new Error('kanal koptu') });
    await harness.service.connect();
    requestApproval(harness);
    const gate = harness.sessions.at(-1)?.spec.toolRuntime.approvalGate;

    harness.service.approveToolCall('call_1');
    await Promise.resolve();
    await Promise.resolve();

    expect(await gate?.(RISKY_TOOL, {})).toBe('denied');
    expect(eventTypes(harness.events)).toContain('error');
  });

  it('bilinmeyen/cevaplanmis onay kimligi sessizce yutulmuyor', async () => {
    const harness = createApprovalHarness();
    await harness.service.connect();
    requestApproval(harness);

    harness.service.approveToolCall('call_1');
    harness.events.length = 0;
    // Ayni kimlik ikinci kez: istek zaten cevaplandi.
    harness.service.approveToolCall('call_1');
    harness.service.rejectToolCall('bilinmeyen');

    expect(eventTypes(harness.events)).toEqual(['unexpected_signal', 'unexpected_signal']);
    expect(harness.sessions.at(-1)?.approvals).toEqual(['call_1']);
  });

  it('oturum kapanirken bekleyen onay defterde `denied` olarak kaliyor', async () => {
    const harness = createApprovalHarness();
    await harness.service.connect();
    requestApproval(harness);
    harness.events.length = 0;

    harness.service.disconnect();

    expect(harness.audits.at(0)).toMatchObject({
      toolName: 'edit_project_file',
      approvalState: 'denied',
    });
    // Kart ekranda asili kalmiyor.
    expect(eventTypes(harness.events)).toContain('tool_approval_resolved');
  });

  /** ASU-050 korelasyonu: audit satiri gercek oturum kaydina bagli. */
  it('onay kararlari gercek oturum kimligiyle yaziliyor', async () => {
    const harness = createApprovalHarness({ resolveSessionId: (): number => 42 });
    await harness.service.connect();
    requestApproval(harness);

    harness.service.rejectToolCall('call_1');

    expect(harness.audits.at(0)?.sessionId).toBe(42);
  });

  it('oturum kimligi bilinmiyorsa alan gonderilmiyor (uydurulmuyor)', async () => {
    const harness = createApprovalHarness({ resolveSessionId: (): number | null => null });
    await harness.service.connect();
    requestApproval(harness);

    harness.service.rejectToolCall('call_1');

    expect(harness.audits.at(0)).not.toHaveProperty('sessionId');
  });

  it('tool"lara giden context gercek oturum kimligini tasiyor', async () => {
    const harness = createApprovalHarness({ resolveSessionId: (): number => 7 });
    await harness.service.connect();

    expect(harness.sessions.at(-1)?.spec.toolRuntime.resolveSessionId?.()).toBe(7);
  });

  it('oturuma giden mod config"ten geliyor', async () => {
    const harness = createApprovalHarness();
    await harness.service.connect();

    expect(harness.sessions.at(-1)?.spec.toolRuntime.approvalMode).toBe('safe');
  });
});

describe('toApprovalArgumentsPreview (ASU-048)', () => {
  it('argumansiz cagri icin `null` donuyor', () => {
    expect(toApprovalArgumentsPreview(null)).toBeNull();
    expect(toApprovalArgumentsPreview('{}')).toBeNull();
  });

  it('alfabetik tek satir uretiyor', () => {
    expect(toApprovalArgumentsPreview('{"path":"README.md","maxBytes":4096}')).toBe(
      'maxBytes=4096, path=README.md',
    );
  });

  /** Dosya icerigi karta dokulmez: ic ice yapilar yalnizca **sekil**. */
  it('ic ice yapilar yalnizca sekil olarak gorunuyor', () => {
    expect(toApprovalArgumentsPreview('{"lines":[1,2,3],"meta":{"a":1,"b":2}}')).toBe(
      'lines=[3 oge], meta={2 alan}',
    );
  });

  it('uzun metin kirpiliyor', () => {
    const preview = toApprovalArgumentsPreview(JSON.stringify({ text: 'x'.repeat(200) }));
    expect(preview).not.toBeNull();
    expect(preview?.length).toBeLessThan(80);
    expect(preview?.endsWith('…')).toBe(true);
  });

  /** Secret desenleri karta da gitmez (`redactText`). */
  it('secret gorunumlu degerler redakte ediliyor', () => {
    const preview = toApprovalArgumentsPreview(
      '{"token":"sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"}',
    );
    expect(preview).not.toContain('ABCDEFGHIJKLMNOPQRSTUVWXYZ');
  });

  it('bozuk JSON oldugu gibi degil, kirpilmis haliyle gosteriliyor', () => {
    expect(toApprovalArgumentsPreview('{bozuk')).toBe('{bozuk');
  });
});

// ---------------------------------------------------------------------------
// SDK `tool()` adaptoru (ASU-044)
// ---------------------------------------------------------------------------

describe('AsunaToolDefinition -> SDK tool adaptoru', () => {
  const readOnly: AsunaToolDefinition = {
    name: 'get_current_project',
    description: 'Guncel projeyi dondurur.',
    risk: 0,
    requiresApproval: false,
    timeoutMs: 25_000,
    // ASU-047: sema tanimin parcasi; SDK'ya giden JSON Schema da bundan uretilir.
    parameters: NO_TOOL_ARGUMENTS,
    execute: (): Promise<ToolResult> => Promise.resolve({ ok: true, summary: 'Proje: Asuna' }),
  };

  it('tanim SDK tool"una birebir tasiniyor', () => {
    const sdkTool = toSdkTool(readOnly);

    expect(sdkTool.type).toBe('function');
    expect(sdkTool.name).toBe('get_current_project');
    expect(sdkTool.description).toBe('Guncel projeyi dondurur.');
    // Asili kalan bir tool sesli oturumda cevapsiz bir sessizliktir.
    expect(sdkTool.timeoutMs).toBe(25_000);
    // Parametresiz: model hangi projenin okunacagini secemez.
    expect(sdkTool.parameters.properties).toEqual({});
    expect(sdkTool.strict).toBe(true);
  });

  /**
   * `conventions.md` pazarliksiz kurali: risk 2/3 her zaman onay ister. Registry
   * (ASU-047) gelene kadar zorlama burada; sessizce onaysiz calistirmak yerine
   * acilista patlar.
   */
  it('risk 2+ bir tool onaysiz kaydedilemiyor', () => {
    expect(() => toSdkTool({ ...readOnly, risk: 2 })).toThrow(AsunaRealtimeError);
    expect(() => toSdkTool({ ...readOnly, risk: 3, requiresApproval: true })).not.toThrow();
  });

  /**
   * `needsApproval` artik statik boolean degil, politika fonksiyonu (ASU-048).
   * SDK'ya giden karar `resolveApproval` matrisinin **ayni** karari olmali;
   * burada iki mod x her risk seviyesi ucdan uca olculuyor.
   */
  it('needsApproval politikasi matrisle ayni karari veriyor', async () => {
    for (const mode of TOOL_APPROVAL_MODES) {
      for (const risk of [0, 1, 2, 3] as const) {
        const definition: AsunaToolDefinition = {
          ...readOnly,
          risk,
          requiresApproval: risk >= 2,
        };
        const policy: (...args: never[]) => Promise<boolean> = toSdkTool(definition, {
          approvalMode: mode,
        }).needsApproval;

        expect(await policy()).toBe(
          resolveApproval(risk, definition.requiresApproval, mode) === 'needs_approval',
        );
      }
    }
  });

  /** Risk 2/3 iki modda da onay ister; konfigurasyon bunu gevsetemez. */
  it('risk 2 ve 3 her iki modda da onay istiyor', async () => {
    for (const mode of TOOL_APPROVAL_MODES) {
      for (const risk of [2, 3] as const) {
        const policy: (...args: never[]) => Promise<boolean> = toSdkTool(
          { ...readOnly, risk, requiresApproval: true },
          { approvalMode: mode },
        ).needsApproval;

        expect(await policy()).toBe(true);
      }
    }
  });

  /** Risk 0 salt-okuma tool iki modda da onaysiz — onay yorgunlugu uretmiyoruz. */
  it('risk 0 tool hicbir modda onay istemiyor', async () => {
    for (const mode of TOOL_APPROVAL_MODES) {
      const policy: (...args: never[]) => Promise<boolean> = toSdkTool(readOnly, {
        approvalMode: mode,
      }).needsApproval;

      expect(await policy()).toBe(false);
    }
  });

  /** Runtime verilmezse en siki varsayilan: risk 1 bile onay ister. */
  it('runtime verilmediginde en siki varsayilan uygulaniyor', async () => {
    const policy: (...args: never[]) => Promise<boolean> = toSdkTool({
      ...readOnly,
      risk: 1,
    }).needsApproval;

    expect(await policy()).toBe(true);
  });

  it('modele giden metin ozet; basarisizlik acikca isaretleniyor', () => {
    expect(toModelOutput({ ok: true, summary: 'Proje: Asuna', data: { gizli: 1 } })).toBe(
      'Proje: Asuna',
    );

    const failed = toModelOutput({
      ok: false,
      summary: 'Proje baglami okunamadi.',
      errorKind: 'project_context_unavailable',
    });
    expect(failed.startsWith(TOOL_FAILURE_PREFIX)).toBe(true);
    expect(failed).toContain('Proje baglami okunamadi.');
  });

  /** Yapisal veri ses oturumuna dokulmez (PROJECT.md Bolum 15). */
  it('`data` alani modele gonderilmiyor', () => {
    const output = toModelOutput({
      ok: true,
      summary: 'Proje: Asuna',
      data: { path: '/Users/omer/Work/asuna', sources: ['README.md'] },
    });

    expect(output).not.toContain('sources');
    expect(output).not.toContain('{');
  });
});

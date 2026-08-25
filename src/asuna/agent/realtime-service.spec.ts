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
  toTurnDetectionSpec,
  type AsunaRealtimeServiceOptions,
} from './realtime-service';
import type { EphemeralRealtimeToken } from './realtime-token';
import type { FrontendConfig } from '../config/frontend-config';
import { buildAsunaInstructions } from '../prompts';
import { VoiceStateMachine, type VoiceState } from '../state/voice-state-machine';

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

  it('Phase 5 tool sinyalleri TOOL_PENDING / AWAITING_APPROVAL yoluna baglanir', () => {
    harness.emit({ type: 'agent_start' });
    harness.emit({ type: 'tool_start', toolName: 'git_status' });
    expect(harness.service.getState()).toBe('TOOL_PENDING');

    harness.emit({ type: 'tool_approval_requested', toolName: 'git_status' });
    expect(harness.service.getState()).toBe('AWAITING_APPROVAL');

    harness.emit({ type: 'tool_end', toolName: 'git_status' });
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

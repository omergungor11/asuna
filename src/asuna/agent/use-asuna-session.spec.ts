/**
 * `useAsunaSession` testleri (ASU-015).
 *
 * Ne aga cikilir ne mikrofona dokunulur: servis, config ve mikrofon sondasi
 * enjekte edilir. Durum iddialari **gercek** `VoiceStateMachine` uzerinden yapilir —
 * hook'un durum uydurmadigi ancak boyle kanitlanir.
 */

import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { AsunaRealtimeError } from './realtime-errors';
import type { AsunaRealtimeEvent, AsunaRealtimeEventListener } from './realtime-events';
import {
  MAX_TRANSCRIPT_LINES,
  describeRealtimeFailure,
  useAsunaSession,
  type AsunaSession,
  type AsunaSessionPort,
  type UseAsunaSessionOptions,
} from './use-asuna-session';
import { MicrophoneAccessError, type MicrophoneProbe } from '../audio/microphone-access';
import type { FrontendConfig } from '../config/frontend-config';
import { AsunaLogger, type LogEntry } from '../observability';
import { VoiceStateMachine } from '../state/voice-state-machine';

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

const PROBE: MicrophoneProbe = { echoCancellation: true, noiseSuppression: true };

/** Gercek servisin durum makinesini surusunu taklit eden sahte oturum. */
interface FakeService extends AsunaSessionPort {
  readonly listenerCount: () => number;
  readonly connectCalls: () => number;
  readonly disconnectCalls: () => number;
  readonly emit: (event: AsunaRealtimeEvent) => void;
}

interface HarnessOptions {
  readonly connectError?: Error;
  readonly probe?: () => Promise<MicrophoneProbe>;
  readonly loadConfig?: () => Promise<FrontendConfig>;
  readonly logger?: AsunaLogger;
  /** Gecikme olcumu icin deterministik saat. */
  readonly now?: () => number;
}

interface Harness {
  readonly options: UseAsunaSessionOptions;
  readonly machine: VoiceStateMachine;
  readonly services: FakeService[];
  readonly service: () => FakeService;
  readonly probeCalls: () => number;
  readonly createdServices: () => number;
}

function createHarness(harnessOptions: HarnessOptions = {}): Harness {
  const machine = new VoiceStateMachine();
  const services: FakeService[] = [];
  let probeCalls = 0;

  const options: UseAsunaSessionOptions = {
    stateMachine: machine,
    logger: harnessOptions.logger ?? new AsunaLogger(),
    ...(harnessOptions.now === undefined ? {} : { now: harnessOptions.now }),
    loadConfig:
      harnessOptions.loadConfig ?? ((): Promise<FrontendConfig> => Promise.resolve(CONFIG)),
    probeMicrophone:
      harnessOptions.probe ??
      ((): Promise<MicrophoneProbe> => {
        probeCalls += 1;
        return Promise.resolve(PROBE);
      }),
    createService: (context): AsunaSessionPort => {
      const listeners = new Set<AsunaRealtimeEventListener>();
      let connects = 0;
      let disconnects = 0;

      const publish = (event: AsunaRealtimeEvent): void => {
        for (const listener of [...listeners]) {
          listener(event);
        }
      };

      const service: FakeService = {
        connect: async (): Promise<void> => {
          connects += 1;
          context.stateMachine.transition('CONNECTING', 'REALTIME_CONNECTING');
          publish({ type: 'connecting', attempt: 1, maxAttempts: 3 });
          await Promise.resolve();

          if (harnessOptions.connectError !== undefined) {
            // Gercek servis de hatada once `ERROR`'a gecer, sonra firlatir.
            context.stateMachine.transition('ERROR', 'ERROR_OCCURRED');
            throw harnessOptions.connectError;
          }

          context.stateMachine.transition('LISTENING', 'REALTIME_CONNECTED');
          publish({ type: 'connected', model: context.config.realtimeModel });
        },
        disconnect: (): void => {
          disconnects += 1;
          if (context.stateMachine.canTransition('BOOTING')) {
            context.stateMachine.transition('BOOTING', 'SESSION_CLOSED_BY_USER');
          }
          publish({ type: 'disconnected', reason: 'requested' });
        },
        interrupt: (): void => {
          publish({ type: 'agent_interrupted' });
        },
        subscribe: (listener): (() => void) => {
          listeners.add(listener);
          return (): void => {
            listeners.delete(listener);
          };
        },
        getState: () => context.stateMachine.getState(),
        listenerCount: (): number => listeners.size,
        connectCalls: (): number => connects,
        disconnectCalls: (): number => disconnects,
        emit: publish,
      };

      services.push(service);
      return service;
    },
  };

  return {
    options,
    machine,
    services,
    service: (): FakeService => {
      const service = services[services.length - 1];
      if (service === undefined) {
        throw new Error('Henuz servis olusturulmadi.');
      }
      return service;
    },
    probeCalls: (): number => probeCalls,
    createdServices: (): number => services.length,
  };
}

/** Hook'un asenkron aktivasyon zincirini act() icinde bosaltir. */
async function flush(action: () => void): Promise<void> {
  await act(async () => {
    action();
    await Promise.resolve();
  });
}

describe('useAsunaSession — baglanti akisi (ASU-015)', () => {
  it('start(): mikrofon izni -> config -> connect -> LISTENING', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    expect(result.current.state).toBe('BOOTING');
    expect(result.current.connected).toBe(false);
    expect(result.current.micActive).toBe(false);

    await flush(() => {
      result.current.start();
    });

    expect(harness.probeCalls()).toBe(1);
    expect(result.current.state).toBe('LISTENING');
    expect(result.current.connected).toBe(true);
    expect(result.current.micActive).toBe(true);
    expect(result.current.model).toBe(CONFIG.realtimeModel);
    expect(result.current.error).toBeNull();
    expect(result.current.busy).toBe(false);
  });

  it('mikrofon izni istenmeden once WAKING durumuna geciyor', async () => {
    const seen: string[] = [];
    const harness = createHarness({
      probe: (): Promise<MicrophoneProbe> => {
        seen.push('probe');
        return Promise.resolve(PROBE);
      },
    });
    harness.machine.subscribe((transition) => {
      seen.push(transition.to);
    });

    const { result } = renderHook(() => useAsunaSession(harness.options));
    await flush(() => {
      result.current.start();
    });

    expect(seen).toEqual(['WAKING', 'probe', 'CONNECTING', 'LISTENING']);
  });

  it('cift tiklama yaris kosulu uretmiyor — tek connect', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
      result.current.start();
      result.current.start();
    });

    expect(harness.createdServices()).toBe(1);
    expect(harness.service().connectCalls()).toBe(1);
    expect(harness.probeCalls()).toBe(1);
  });

  it('bagliyken start() yeniden baglanmaya calismiyor', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });
    await flush(() => {
      result.current.start();
    });

    expect(harness.service().connectCalls()).toBe(1);
  });

  it('aktivasyon sirasinda buton kilitli (busy)', async () => {
    let release: (() => void) | null = null;
    const harness = createHarness({
      probe: (): Promise<MicrophoneProbe> =>
        new Promise<MicrophoneProbe>((resolve) => {
          release = (): void => {
            resolve(PROBE);
          };
        }),
    });
    const { result } = renderHook(() => useAsunaSession(harness.options));

    act(() => {
      result.current.start();
    });
    expect(result.current.busy).toBe(true);

    await act(async () => {
      release?.();
      await Promise.resolve();
    });
    expect(result.current.busy).toBe(false);
  });

  it('stop(): oturumu kapatir ve idle duruma doner', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });
    act(() => {
      result.current.stop();
    });

    expect(harness.service().disconnectCalls()).toBe(1);
    expect(result.current.state).toBe('BOOTING');
    expect(result.current.connected).toBe(false);
    expect(result.current.micActive).toBe(false);
  });
});

describe('useAsunaSession — hata yollari (ASU-015 / ASU-019)', () => {
  it('mikrofon izni reddedilirse macOS kurulum yonlendirmesi gosteriliyor', async () => {
    const harness = createHarness({
      probe: (): Promise<MicrophoneProbe> =>
        Promise.reject(new MicrophoneAccessError('mic_permission_denied', 'reddedildi')),
    });
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });

    expect(result.current.state).toBe('ERROR');
    expect(result.current.error?.kind).toBe('mic_permission_denied');
    expect(result.current.error?.action).toContain('Gizlilik ve Güvenlik');
    expect(result.current.error?.retryable).toBe(true);
    expect(harness.createdServices()).toBe(0);
  });

  it('config okunamazsa oturum acilmiyor', async () => {
    const harness = createHarness({
      loadConfig: (): Promise<FrontendConfig> => Promise.reject(new Error('IPC yok')),
    });
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });

    expect(result.current.error?.kind).toBe('config_unavailable');
    expect(result.current.state).toBe('ERROR');
    expect(harness.createdServices()).toBe(0);
  });

  it('gecersiz API anahtarinda tekrar denemeye izin vermiyor', async () => {
    const harness = createHarness({
      connectError: new AsunaRealtimeError({
        kind: 'token',
        cause: 'invalid_api_key',
        message: 'anahtar reddedildi',
        retryable: false,
      }),
    });
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });

    expect(result.current.state).toBe('ERROR');
    expect(result.current.error?.kind).toBe('invalid_api_key');
    expect(result.current.error?.retryable).toBe(false);
    expect(result.current.connected).toBe(false);
    expect(result.current.busy).toBe(false);
  });

  it('ERROR durumundan yeniden baglanilabiliyor', async () => {
    const harness = createHarness({
      probe: vi
        .fn<() => Promise<MicrophoneProbe>>()
        .mockRejectedValueOnce(new MicrophoneAccessError('mic_permission_denied', 'reddedildi'))
        .mockResolvedValue(PROBE),
    });
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });
    expect(result.current.state).toBe('ERROR');

    await flush(() => {
      result.current.start();
    });

    expect(result.current.state).toBe('LISTENING');
    expect(result.current.error).toBeNull();
  });
});

describe('describeRealtimeFailure', () => {
  it('Rust etiketini ASU-019 mesaj tablosuna baglar', () => {
    const described = describeRealtimeFailure({
      kind: 'token',
      cause: 'quota_exceeded',
      message: 'kota',
      retryable: true,
    });

    expect(described.kind).toBe('quota_exceeded');
    expect(described.action).not.toBeNull();
  });

  it('etiket cozulemezse servisin kendi mesajini koruyor', () => {
    const described = describeRealtimeFailure({
      kind: 'transport',
      cause: null,
      message: 'Realtime oturumu acilamadi: SDP reddedildi',
      retryable: true,
    });

    expect(described.kind).toBe('realtime_connect_failed');
    expect(described.message).toBe('Realtime oturumu acilamadi: SDP reddedildi');
    expect(described.retryable).toBe(true);
  });
});

describe('useAsunaSession — iki yonlu ses ve barge-in (ASU-016)', () => {
  /** Baglantiyi kurup sahte servisi doner. */
  async function connected(harness: Harness): Promise<{
    readonly session: () => AsunaSession;
    readonly service: FakeService;
  }> {
    const { result } = renderHook(() => useAsunaSession(harness.options));
    await flush(() => {
      result.current.start();
    });
    return { session: (): AsunaSession => result.current, service: harness.service() };
  }

  it('konusma durumlarini yansitiyor: dusunuyor -> konusuyor -> dinliyor', async () => {
    const harness = createHarness();
    const { session, service } = await connected(harness);

    act(() => {
      harness.machine.transition('ASSISTANT_THINKING', 'ASSISTANT_RESPONSE_STARTED');
      service.emit({ type: 'agent_thinking' });
    });
    expect(session().state).toBe('ASSISTANT_THINKING');

    act(() => {
      harness.machine.transition('ASSISTANT_SPEAKING', 'ASSISTANT_AUDIO_STARTED');
      service.emit({ type: 'agent_audio_started' });
    });
    expect(session().state).toBe('ASSISTANT_SPEAKING');

    act(() => {
      harness.machine.transition('LISTENING', 'ASSISTANT_RESPONSE_COMPLETED');
      service.emit({ type: 'agent_audio_stopped' });
    });
    expect(session().state).toBe('LISTENING');
  });

  it('barge-in: soz kesilince gorsel tepki veriyor ve USER_SPEAKING oluyor', async () => {
    const harness = createHarness();
    const { session, service } = await connected(harness);

    act(() => {
      harness.machine.transition('ASSISTANT_THINKING', 'ASSISTANT_RESPONSE_STARTED');
      harness.machine.transition('ASSISTANT_SPEAKING', 'ASSISTANT_AUDIO_STARTED');
      service.emit({ type: 'agent_audio_started' });
    });
    expect(session().bargeIn).toBe(false);

    act(() => {
      harness.machine.transition('USER_SPEAKING', 'USER_INTERRUPTED');
      service.emit({ type: 'agent_interrupted' });
    });

    expect(session().state).toBe('USER_SPEAKING');
    expect(session().bargeIn).toBe(true);

    // Yeni cevap baslayinca isaret kalkar: kesme eski cevabin devami degil.
    act(() => {
      harness.machine.transition('ASSISTANT_THINKING', 'ASSISTANT_RESPONSE_STARTED');
      harness.machine.transition('ASSISTANT_SPEAKING', 'ASSISTANT_AUDIO_STARTED');
      service.emit({ type: 'agent_audio_started' });
    });
    expect(session().bargeIn).toBe(false);
  });

  it('konusma sonu -> ilk ses gecikmesini olcup logluyor', async () => {
    const entries: LogEntry[] = [];
    let clock = 0;
    const harness = createHarness({
      logger: new AsunaLogger({
        level: 'debug',
        sinks: [
          (entry): void => {
            entries.push(entry);
          },
        ],
      }),
      now: (): number => clock,
    });
    const { session, service } = await connected(harness);

    act(() => {
      clock = 1_000;
      service.emit({ type: 'agent_thinking' });
      clock = 1_480;
      service.emit({ type: 'agent_audio_started' });
    });

    expect(session().lastLatencyMs).toBe(480);
    expect(entries.some((entry) => entry.message.includes('Yanit gecikmesi: 480 ms'))).toBe(
      true,
    );
  });

  // ASU-064: olcumun yaninda ayar yoksa "onceki/sonraki" karsilastirmasi yapilamaz.
  it('gecikme log satiri aktif tur-tespiti ayarini da tasiyor', async () => {
    const entries: LogEntry[] = [];
    let clock = 0;
    const harness = createHarness({
      loadConfig: (): Promise<FrontendConfig> =>
        Promise.resolve({ ...CONFIG, turnDetection: 'server_vad', vadSilenceMs: 700 }),
      logger: new AsunaLogger({
        level: 'debug',
        sinks: [
          (entry): void => {
            entries.push(entry);
          },
        ],
      }),
      now: (): number => clock,
    });
    const { service } = await connected(harness);

    act(() => {
      clock = 1_000;
      service.emit({ type: 'agent_thinking' });
      clock = 2_240;
      service.emit({ type: 'agent_audio_started' });
    });

    const line = entries.find((entry) => entry.message.includes('Yanit gecikmesi'));
    expect(line?.message).toContain('1240 ms');
    expect(line?.message).toContain('vad=server/700ms');
    expect(line?.data).toMatchObject({ latencyMs: 1240, vad: 'server/700ms' });
  });

  it('kullanici transkripti kesinlestiginde olcum oradan basliyor', async () => {
    let clock = 0;
    const harness = createHarness({ now: (): number => clock });
    const { session, service } = await connected(harness);

    act(() => {
      clock = 2_000;
      service.emit({
        type: 'transcript',
        entry: { itemId: 'u1', role: 'user', text: 'merhaba', status: 'completed' },
      });
      clock = 2_200;
      service.emit({ type: 'agent_thinking' });
      clock = 2_600;
      service.emit({ type: 'agent_audio_started' });
    });

    expect(session().lastLatencyMs).toBe(600);
  });

  it('kesilen turun olcumu sonraki tura tasinmiyor', async () => {
    let clock = 0;
    const harness = createHarness({ now: (): number => clock });
    const { session, service } = await connected(harness);

    act(() => {
      clock = 100;
      service.emit({ type: 'agent_thinking' });
      clock = 200;
      service.emit({ type: 'agent_interrupted' });
      clock = 900;
      service.emit({ type: 'agent_audio_started' });
    });

    expect(session().lastLatencyMs).toBeNull();
  });

  it('echo cancellation dogrulanamazsa self-interrupt riskini uyariyor', async () => {
    const entries: LogEntry[] = [];
    const harness = createHarness({
      logger: new AsunaLogger({
        level: 'debug',
        sinks: [
          (entry): void => {
            entries.push(entry);
          },
        ],
      }),
      probe: (): Promise<MicrophoneProbe> =>
        Promise.resolve({ echoCancellation: false, noiseSuppression: true }),
    });
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });

    const warning = entries.find((entry) => entry.level === 'warn');
    expect(warning?.message).toContain('Echo cancellation dogrulanamadi');
  });
});

describe('useAsunaSession — canli transcript (ASU-017)', () => {
  async function connectedHook(harness: Harness): Promise<{
    readonly session: () => AsunaSession;
    readonly service: FakeService;
  }> {
    const { result } = renderHook(() => useAsunaSession(harness.options));
    await flush(() => {
      result.current.start();
    });
    return { session: (): AsunaSession => result.current, service: harness.service() };
  }

  it('kullanici ve Asuna satirlarini sirasiyla biriktirir', async () => {
    const harness = createHarness();
    const { session, service } = await connectedHook(harness);

    act(() => {
      service.emit({
        type: 'transcript',
        entry: { itemId: 'u1', role: 'user', text: 'merhaba', status: 'completed' },
      });
      service.emit({
        type: 'transcript',
        entry: { itemId: 'a1', role: 'assistant', text: 'buradayim', status: 'completed' },
      });
    });

    expect(session().transcript.map((line) => `${line.role}:${line.text}`)).toEqual([
      'user:merhaba',
      'assistant:buradayim',
    ]);
  });

  it('kismi satiri ayni itemId uzerinde kesinlestirir (kopya uretmez)', async () => {
    const harness = createHarness();
    const { session, service } = await connectedHook(harness);

    act(() => {
      service.emit({
        type: 'transcript',
        entry: { itemId: 'u1', role: 'user', text: 'bugun', status: 'in_progress' },
      });
    });
    expect(session().transcript[0]?.status).toBe('in_progress');

    act(() => {
      service.emit({
        type: 'transcript',
        entry: { itemId: 'u1', role: 'user', text: 'bugun ne yapsam', status: 'completed' },
      });
    });

    expect(session().transcript).toHaveLength(1);
    expect(session().transcript[0]?.text).toBe('bugun ne yapsam');
    expect(session().transcript[0]?.status).toBe('completed');
  });

  it('kesme aninda uretilen Asuna cevabini isaretler ve isaret kaybolmaz', async () => {
    const harness = createHarness();
    const { session, service } = await connectedHook(harness);

    act(() => {
      service.emit({
        type: 'transcript',
        entry: { itemId: 'a1', role: 'assistant', text: 'sana sunu', status: 'in_progress' },
      });
      service.emit({ type: 'agent_interrupted' });
    });
    expect(session().transcript[0]?.interrupted).toBe(true);

    // Ayni item guncellenince isaret korunur.
    act(() => {
      service.emit({
        type: 'transcript',
        entry: {
          itemId: 'a1',
          role: 'assistant',
          text: 'sana sunu anlat',
          status: 'completed',
        },
      });
    });
    expect(session().transcript[0]?.interrupted).toBe(true);
  });

  it('kesme, tamamlanmis eski cevabi geriye donuk isaretlemez', async () => {
    const harness = createHarness();
    const { session, service } = await connectedHook(harness);

    act(() => {
      service.emit({
        type: 'transcript',
        entry: { itemId: 'a1', role: 'assistant', text: 'tamamlandi', status: 'completed' },
      });
      service.emit({ type: 'agent_interrupted' });
    });

    expect(session().transcript[0]?.interrupted).toBe(false);
  });

  it('uzun oturumda satir sayisi sinirli kalir (bellek)', async () => {
    const harness = createHarness();
    const { session, service } = await connectedHook(harness);

    act(() => {
      for (let index = 0; index < MAX_TRANSCRIPT_LINES + 25; index += 1) {
        service.emit({
          type: 'transcript',
          entry: {
            itemId: `i${index.toString()}`,
            role: 'user',
            text: `satir ${index.toString()}`,
            status: 'completed',
          },
        });
      }
    });

    const transcript = session().transcript;
    expect(transcript).toHaveLength(MAX_TRANSCRIPT_LINES);
    expect(transcript[0]?.text).toBe('satir 25');
    expect(transcript[transcript.length - 1]?.text).toBe(
      `satir ${(MAX_TRANSCRIPT_LINES + 24).toString()}`,
    );
  });

  it('yeni oturum yeni dokumle baslar', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));
    await flush(() => {
      result.current.start();
    });

    act(() => {
      harness.service().emit({
        type: 'transcript',
        entry: { itemId: 'u1', role: 'user', text: 'eski oturum', status: 'completed' },
      });
    });
    expect(result.current.transcript).toHaveLength(1);

    act(() => {
      result.current.stop();
    });
    // Kapaninca dokum ekranda kalir: kullanici okumaya devam edebilir.
    expect(result.current.transcript).toHaveLength(1);

    await flush(() => {
      result.current.start();
    });
    expect(result.current.transcript).toHaveLength(0);
  });
});

describe('useAsunaSession — temiz disconnect ve kaynak temizligi (ASU-018)', () => {
  it('ard arda 5 baglan/kes: dinleyici birikmiyor, durum tutarli', async () => {
    const harness = createHarness();
    const { result, unmount } = renderHook(() => useAsunaSession(harness.options));

    for (let round = 0; round < 5; round += 1) {
      await flush(() => {
        result.current.start();
      });
      expect(result.current.state).toBe('LISTENING');
      expect(result.current.connected).toBe(true);

      act(() => {
        result.current.stop();
      });
      expect(result.current.state).toBe('BOOTING');
      expect(result.current.connected).toBe(false);

      // Her turda tek servis, tek dinleyici — abonelik birikmiyor.
      expect(harness.createdServices()).toBe(1);
      expect(harness.service().listenerCount()).toBe(1);
    }

    expect(harness.service().connectCalls()).toBe(5);
    expect(harness.service().disconnectCalls()).toBe(5);

    unmount();
    expect(harness.service().listenerCount()).toBe(0);
  });

  it('kapali oturumda stop() fazladan disconnect uretmiyor', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });
    act(() => {
      result.current.stop();
      result.current.stop();
      result.current.stop();
    });

    expect(harness.service().disconnectCalls()).toBe(1);
  });

  it('bilesen unmount olurken acik oturumu kapatiyor', async () => {
    const harness = createHarness();
    const { result, unmount } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });
    const service = harness.service();

    unmount();

    expect(service.disconnectCalls()).toBe(1);
    expect(service.listenerCount()).toBe(0);
  });

  it('pencere kapanirken oturumu kapatiyor ve kanca unmount’ta sokuluyor', async () => {
    const handlers: (() => void)[] = [];
    let detachCalls = 0;
    const harness = createHarness();
    const options: UseAsunaSessionOptions = {
      ...harness.options,
      registerCloseHandler: (handler): (() => void) => {
        handlers.push(handler);
        return (): void => {
          detachCalls += 1;
        };
      },
    };
    const { result, unmount } = renderHook(() => useAsunaSession(options));

    await flush(() => {
      result.current.start();
    });
    expect(handlers).toHaveLength(1);

    act(() => {
      handlers[0]?.();
    });

    expect(harness.service().disconnectCalls()).toBe(1);
    expect(result.current.connected).toBe(false);

    unmount();
    expect(detachCalls).toBe(1);
  });

  it('ag kopmasinda oturum otomatik temizleniyor ve UI ERROR gosteriyor', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });
    const service = harness.service();

    act(() => {
      harness.machine.transition('ERROR', 'ERROR_OCCURRED');
      service.emit({
        type: 'error',
        error: {
          kind: 'session',
          cause: null,
          message: 'Ses oturumunda hata olustu: baglanti koptu',
          retryable: false,
        },
      });
    });

    expect(service.disconnectCalls()).toBe(1);
    expect(result.current.state).toBe('ERROR');
    expect(result.current.connected).toBe(false);
    expect(result.current.error?.message).toContain('baglanti koptu');
    // ERROR terminal degil: yeniden baglanma yolu acik.
    expect(result.current.error?.retryable).toBe(true);

    await flush(() => {
      result.current.start();
    });
    expect(result.current.state).toBe('LISTENING');
    expect(service.connectCalls()).toBe(2);
  });
});

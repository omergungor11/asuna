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
import { ToolToggleStore } from '../tools';
import { NO_TOOL_ARGUMENTS, type AsunaToolDefinition, type ToolResult } from '../tools/types';
import { SessionRecorder } from '../memory/session-service';
import type { FrontendConfig } from '../config/frontend-config';
import { AsunaLogger, type LogEntry } from '../observability';
import { VoiceStateMachine } from '../state/voice-state-machine';
import type {
  SessionFinalizeInput,
  SessionRecord,
  SessionWriteResult,
} from '../../shared/session';

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
  /** Hook'tan gelen onay/ret cagrilari (ASU-048). */
  readonly approvals: string[];
  readonly rejections: string[];
  /** Oturum kaydini acan/kapatan kayitci — sessionId korelasyonu icin. */
  readonly recorder: SessionRecorder;
  /** Servise **baglanirken** verilen tool listesi (ASU-054 suzme kaniti). */
  readonly toolNames: readonly string[];
  /** Servisin her cagrida soracagi kapi (ASU-054). */
  readonly isToolEnabled: (toolName: string) => boolean;
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
      const approvals: string[] = [];
      const rejections: string[] = [];
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
        approveToolCall: (requestId): void => {
          approvals.push(requestId);
        },
        rejectToolCall: (requestId): void => {
          rejections.push(requestId);
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
        approvals,
        rejections,
        // ASU-048/050: servis oturum kimligini buradan okur; testler ayni
        // nesneyi gorup korelasyonu olcebilsin diye disari veriliyor.
        recorder: context.recorder,
        // ASU-054: hook'un servise verdigi liste ve kapi. Kapali bir tool'un
        // modele hic verilmedigi ancak buradan olculebilir.
        toolNames: context.tools
          .filter((tool) => context.isToolEnabled(tool.name))
          .map((tool) => tool.name),
        isToolEnabled: context.isToolEnabled,
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

describe('useAsunaSession — oturum kaydi (ASU-032)', () => {
  const OPEN_SESSION: SessionRecord = {
    id: 12,
    startedAt: '2026-08-25T10:00:00Z',
    endedAt: null,
    projectId: null,
    summary: null,
    transcriptPath: null,
    model: CONFIG.realtimeModel,
    inputTokens: null,
    outputTokens: null,
    totalTokens: null,
    estimatedCostUsd: null,
    usageJson: null,
    createdAt: '2026-08-25T10:00:00Z',
    endReason: null,
  };

  const CLOSED_SESSION: SessionRecord = {
    ...OPEN_SESSION,
    endedAt: '2026-08-25T10:04:00Z',
    inputTokens: 120,
    outputTokens: 80,
    totalTokens: 200,
    endReason: 'completed',
  };

  type StartMock = ReturnType<
    typeof vi.fn<(projectId?: string) => Promise<SessionWriteResult>>
  >;
  type FinalizeMock = ReturnType<
    typeof vi.fn<
      (sessionId: number, input: SessionFinalizeInput) => Promise<SessionWriteResult>
    >
  >;

  interface RecordingHarness {
    readonly harness: Harness;
    readonly options: UseAsunaSessionOptions;
    readonly start: StartMock;
    readonly finalize: FinalizeMock;
  }

  function createRecordingHarness(
    overrides: { readonly start?: StartMock } = {},
  ): RecordingHarness {
    const start: StartMock =
      overrides.start ??
      vi
        .fn<(projectId?: string) => Promise<SessionWriteResult>>()
        .mockResolvedValue({ status: 'recorded', session: OPEN_SESSION });
    const finalize: FinalizeMock = vi
      .fn<(sessionId: number, input: SessionFinalizeInput) => Promise<SessionWriteResult>>()
      .mockResolvedValue({ status: 'recorded', session: CLOSED_SESSION });

    // Deterministik saat: her okuma +1000 ms.
    let tick = 0;
    const harness = createHarness({
      now: (): number => {
        const value = tick;
        tick += 1_000;
        return value;
      },
    });

    return {
      harness,
      options: {
        ...harness.options,
        createSessionRecorder: (): SessionRecorder => new SessionRecorder({ start, finalize }),
      },
      start,
      finalize,
    };
  }

  it('oturum acilinca kayit acar, kapanista kullanim ve dokumu yazar', async () => {
    const { harness, options, start, finalize } = createRecordingHarness();
    const { result } = renderHook(() => useAsunaSession(options));

    await flush(() => {
      result.current.start();
    });
    expect(start).toHaveBeenCalledOnce();

    act(() => {
      harness.service().emit({
        type: 'transcript',
        entry: {
          itemId: 'item-1',
          role: 'user',
          text: 'Wake word yerel kalsin.',
          status: 'completed',
        },
      });
      harness.service().emit({
        type: 'usage',
        usage: {
          requests: 2,
          inputTokens: 120,
          outputTokens: 80,
          totalTokens: 200,
          inputTokenDetails: [{ audio_tokens: 90 }],
          outputTokenDetails: [],
        },
      });
    });

    await flush(() => {
      result.current.stop();
    });

    expect(finalize).toHaveBeenCalledOnce();
    const [sessionId, input] = finalize.mock.calls[0] ?? [0, {}];
    expect(sessionId).toBe(12);
    expect(input.usage).toEqual({
      requests: 2,
      inputTokens: 120,
      outputTokens: 80,
      totalTokens: 200,
      inputTokenDetails: [{ audio_tokens: 90 }],
      outputTokenDetails: [],
    });
    expect(input.transcript).toEqual([
      {
        role: 'user',
        text: 'Wake word yerel kalsin.',
        at: expect.stringMatching(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/) as unknown,
      },
    ]);

    // Sure ve token UI'ya dusuyor (R1 takibi); maliyet uydurulmuyor.
    expect(result.current.sessionOutcome).toEqual({
      id: 12,
      durationMs: expect.any(Number) as unknown,
      totalTokens: 200,
      estimatedCostUsd: null,
    });
  });

  /** Hafiza kapali: oturum kaydi yok, ama konusma sorunsuz calisti. */
  it('kayit atlandiginda konusma akisi etkilenmez', async () => {
    const { options, finalize } = createRecordingHarness({
      start: vi
        .fn<(projectId?: string) => Promise<SessionWriteResult>>()
        .mockResolvedValue({ status: 'skipped', reason: 'memory-disabled' }),
    });
    const { result } = renderHook(() => useAsunaSession(options));

    await flush(() => {
      result.current.start();
    });
    expect(result.current.state).toBe('LISTENING');

    await flush(() => {
      result.current.stop();
    });

    expect(finalize).not.toHaveBeenCalled();
    expect(result.current.sessionOutcome).toBeNull();
    expect(result.current.error).toBeNull();
  });

  /**
   * ASU-050 korelasyonu: servis oturum kimligini **kayitcidan** okur, hook'un
   * ayri bir kopyasindan degil. Kayit acilana kadar `null` (uydurulmuyor).
   */
  it('servise verilen kayitci gercek oturum kimligini veriyor', async () => {
    const { harness, options } = createRecordingHarness();
    const { result } = renderHook(() => useAsunaSession(options));

    await flush(() => {
      result.current.start();
    });

    expect(harness.service().recorder.currentSessionId).toBe(12);

    await flush(() => {
      result.current.stop();
    });
  });

  /** Kayit hatasi sesli oturumu dusurmez (PROJECT.md Bolum 30). */
  it('kayit hatasi oturumu dusurmez', async () => {
    const { options } = createRecordingHarness({
      start: vi
        .fn<(projectId?: string) => Promise<SessionWriteResult>>()
        .mockRejectedValue(new Error('disk dolu')),
    });
    const { result } = renderHook(() => useAsunaSession(options));

    await flush(() => {
      result.current.start();
    });
    expect(result.current.state).toBe('LISTENING');

    await flush(() => {
      result.current.stop();
    });
    expect(result.current.error).toBeNull();
    expect(result.current.sessionOutcome).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Onay akisi (ASU-048) — kart verisi ve karar yolu
// ---------------------------------------------------------------------------

describe('useAsunaSession — tool onayi', () => {
  const REQUEST: Extract<AsunaRealtimeEvent, { type: 'tool_approval_requested' }> = {
    type: 'tool_approval_requested',
    requestId: 'call_1',
    toolName: 'edit_project_file',
    description: 'Kayitli proje kokundeki bir dosyayi duzenler.',
    risk: 2,
    argumentsPreview: 'path=README.md',
    timeoutMs: 60_000,
  };

  async function connected(): Promise<{
    readonly harness: Harness;
    readonly result: { current: AsunaSession };
  }> {
    const harness = createHarness({ now: (): number => 5_000 });
    const { result } = renderHook(() => useAsunaSession(harness.options));
    await flush(() => {
      result.current.start();
    });
    return { harness, result };
  }

  it('bekleyen onay karti icin gereken her seyi tasiyor', async () => {
    const { harness, result } = await connected();

    act(() => {
      harness.service().emit(REQUEST);
    });

    // Kart "izin ver?" demiyor: ne yapilacagini gosteriyor (security.md Bolum 3).
    expect(result.current.pendingApproval).toEqual({
      requestId: 'call_1',
      toolName: 'edit_project_file',
      description: 'Kayitli proje kokundeki bir dosyayi duzenler.',
      risk: 2,
      argumentsPreview: 'path=README.md',
      timeoutMs: 60_000,
      requestedAtMs: 5_000,
    });
    expect(result.current.activeTool).toBe('edit_project_file');
  });

  it('onay/ret kararlari servise **kimlikle** iletiliyor', async () => {
    const { harness, result } = await connected();

    act(() => {
      harness.service().emit(REQUEST);
    });
    act(() => {
      result.current.approveTool('call_1');
    });
    act(() => {
      harness.service().emit({ ...REQUEST, requestId: 'call_2' });
    });
    act(() => {
      result.current.rejectTool('call_2');
    });

    expect(harness.service().approvals).toEqual(['call_1']);
    expect(harness.service().rejections).toEqual(['call_2']);
  });

  it('karar sonuclaninca kart kalkiyor', async () => {
    const { harness, result } = await connected();

    act(() => {
      harness.service().emit(REQUEST);
    });
    act(() => {
      harness.service().emit({
        type: 'tool_approval_resolved',
        requestId: 'call_1',
        toolName: 'edit_project_file',
        outcome: 'denied',
      });
    });

    expect(result.current.pendingApproval).toBeNull();
    // Reddedilen tool calismiyor: "aktif arac" satiri da temizleniyor.
    expect(result.current.activeTool).toBeNull();
  });

  /** Baska bir istegin sonucu ekrandaki karti dusurmuyor. */
  it('baska bir kimligin sonucu bekleyen karti etkilemiyor', async () => {
    const { harness, result } = await connected();

    act(() => {
      harness.service().emit(REQUEST);
    });
    act(() => {
      harness.service().emit({
        type: 'tool_approval_resolved',
        requestId: 'baska',
        toolName: 'edit_project_file',
        outcome: 'denied',
      });
    });

    expect(result.current.pendingApproval?.requestId).toBe('call_1');
  });

  it('oturum kapaninca bekleyen kart kalkiyor', async () => {
    const { harness, result } = await connected();

    act(() => {
      harness.service().emit(REQUEST);
    });
    await flush(() => {
      result.current.stop();
    });

    expect(result.current.pendingApproval).toBeNull();
  });
});

describe('useAsunaSession — tool gorunurlugu ve acma/kapama (ASU-054)', () => {
  const TOOLS: readonly AsunaToolDefinition[] = [
    {
      name: 'get_current_project',
      description: 'Kullanicinin su an uzerinde calistigi kayitli projeyi dondurur.',
      risk: 0,
      requiresApproval: false,
      timeoutMs: 5_000,
      parameters: NO_TOOL_ARGUMENTS,
      execute: (): Promise<ToolResult> => Promise.resolve({ ok: true, summary: 'oldu' }),
    },
    {
      name: 'open_project',
      description: 'Kayitli projeyi ayarlanmis kod editorunde acar; onay ister.',
      risk: 1,
      requiresApproval: true,
      timeoutMs: 5_000,
      parameters: NO_TOOL_ARGUMENTS,
      execute: (): Promise<ToolResult> => Promise.resolve({ ok: true, summary: 'acildi' }),
    },
  ];

  function toolHarness(): Harness & { readonly toggles: ToolToggleStore } {
    const toggles = new ToolToggleStore();
    const base = createHarness();
    return {
      ...base,
      toggles,
      options: { ...base.options, tools: TOOLS, toolToggles: toggles },
    };
  }

  it('registry"den turetilmis tool listesini donduruyor', async () => {
    const harness = toolHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    // Onay modu config'ten okunuyor; mount effect'inin cozulmesini bekle.
    await act(async () => {
      await Promise.resolve();
    });

    expect(result.current.tools.map((tool) => tool.name)).toEqual([
      'get_current_project',
      'open_project',
    ]);
    expect(result.current.tools[0]).toEqual({
      name: 'get_current_project',
      description: TOOLS[0]?.description,
      risk: 0,
      approval: 'not_required',
      enabled: true,
    });
    // Risk 1 `safe` modda onay ister (ASU-048 matrisi).
    expect(result.current.tools[1]?.approval).toBe('always');
  });

  /**
   * Config okunana kadar **en siki** politika gosterilir: "bu onaysiz calisir"
   * diye yanlis bir soz vermek, kullanicinin gozunde tool'u zararsiz gosterir.
   */
  it('config okunamazsa en siki politikayi gosteriyor', async () => {
    const base = createHarness({
      loadConfig: (): Promise<FrontendConfig> => Promise.reject(new Error('config yok')),
    });
    const options: UseAsunaSessionOptions = { ...base.options, tools: TOOLS };
    const { result } = renderHook(() => useAsunaSession(options));

    await act(async () => {
      await Promise.resolve();
    });

    expect(result.current.tools.map((tool) => tool.approval)).toEqual([
      'not_required',
      'always',
    ]);
  });

  it('setToolEnabled listeyi guncelliyor', async () => {
    const harness = toolHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await act(async () => {
      result.current.setToolEnabled('open_project', false);
      await Promise.resolve();
    });

    expect(result.current.tools.map((tool) => tool.enabled)).toEqual([true, false]);
    // Kapali tool ekrandan **kaybolmaz**; kullanici geri acabilmeli.
    expect(result.current.tools).toHaveLength(2);

    await act(async () => {
      result.current.setToolEnabled('open_project', true);
      await Promise.resolve();
    });
    expect(result.current.tools.map((tool) => tool.enabled)).toEqual([true, true]);
  });

  /** **Kabul kriteri**: kapali tool modele **verilmez**. */
  it('kapali tool servise verilen listeden dusuyor', async () => {
    const harness = toolHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await act(async () => {
      result.current.setToolEnabled('open_project', false);
      await Promise.resolve();
    });
    await flush(() => {
      result.current.start();
    });

    expect(harness.service().toolNames).toEqual(['get_current_project']);
  });

  /**
   * Acik bir oturumun ortasinda kapatma modelin listesini degistirmez (SDK'ya
   * verilen set sabit); bu yuzden servis her cagrida kapiyi yeniden sorar ve
   * cevap **aninda** degisir.
   */
  it('oturum aciktayken kapatma kapiyi hemen etkiliyor', async () => {
    const harness = toolHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));

    await flush(() => {
      result.current.start();
    });
    const service = harness.service();
    expect(service.isToolEnabled('open_project')).toBe(true);

    await act(async () => {
      result.current.setToolEnabled('open_project', false);
      await Promise.resolve();
    });

    expect(service.isToolEnabled('open_project')).toBe(false);
  });
});

describe('useAsunaSession — tool sonucu dokumde (ASU-054)', () => {
  it('tool sonucunu ozet satiri olarak ekliyor', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));
    await flush(() => {
      result.current.start();
    });

    await act(async () => {
      await Promise.resolve();
      harness.service().emit({
        type: 'tool_result',
        toolName: 'read_project_file',
        risk: 0,
        outcome: 'succeeded',
        approvalState: 'not_required',
        summary: 'README.md okundu (2.1 KB, kirpildi)',
      });
    });

    const line = result.current.transcript.at(-1);
    expect(line?.role).toBe('tool');
    expect(line?.text).toBe('README.md okundu (2.1 KB, kirpildi)');
    expect(line?.status).toBe('completed');
    expect(line?.interrupted).toBe(false);
    expect(line?.role === 'tool' && line.outcome).toBe('succeeded');
    expect(line?.role === 'tool' && line.toolName).toBe('read_project_file');
  });

  /** Reddedilen aksiyon da gorunur: "oldu mu, olmadi mi?" sorusu kalmamali. */
  it('calismayan cagriyi da yaziyor', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));
    await flush(() => {
      result.current.start();
    });

    await act(async () => {
      await Promise.resolve();
      harness.service().emit({
        type: 'tool_result',
        toolName: 'open_project',
        risk: 1,
        outcome: 'not_run',
        approvalState: 'denied',
        summary: 'Reddedildi: kullanici onaylamadi.',
      });
    });

    const line = result.current.transcript.at(-1);
    expect(line?.role === 'tool' && line.outcome).toBe('not_run');
    expect(line?.role === 'tool' && line.approvalState).toBe('denied');
  });

  /** Mevcut `user`/`assistant` tuketicileri kirilmiyor: satirlar bir arada. */
  it('konusma satirlariyla ayni akista, sirasi korunarak duruyor', async () => {
    const harness = createHarness();
    const { result } = renderHook(() => useAsunaSession(harness.options));
    await flush(() => {
      result.current.start();
    });

    await act(async () => {
      await Promise.resolve();
      harness.service().emit({
        type: 'transcript',
        entry: {
          itemId: 'item_1',
          role: 'user',
          text: 'README ne diyor?',
          status: 'completed',
        },
      });
      harness.service().emit({
        type: 'tool_result',
        toolName: 'read_project_file',
        risk: 0,
        outcome: 'succeeded',
        approvalState: 'not_required',
        summary: 'README.md okundu (2.1 KB)',
      });
      harness.service().emit({
        type: 'transcript',
        entry: {
          itemId: 'item_2',
          role: 'assistant',
          text: 'Projenin amaci...',
          status: 'completed',
        },
      });
    });

    expect(result.current.transcript.map((line) => line.role)).toEqual([
      'user',
      'tool',
      'assistant',
    ]);
    // Tool satirinin kimligi konusma item'lariyla carpismiyor.
    const ids = result.current.transcript.map((line) => line.itemId);
    expect(new Set(ids).size).toBe(3);
  });

  /**
   * Tool satirlari **kalici dokume girmez**: `transcript_lines` sozlesmesi
   * yalnizca `user`/`assistant` tanir ve tool cagrilari kendi defterinde
   * (`tool_events`) yasar. Ayni olayi iki yere yazmak, birinin silinip
   * otekinin kalmasi demekti.
   */
  it('kalici oturum kaydina yazilmiyor', async () => {
    const finalized: SessionFinalizeInput[] = [];
    const harness = createHarness();
    const options: UseAsunaSessionOptions = {
      ...harness.options,
      createSessionRecorder: (): SessionRecorder =>
        new SessionRecorder({
          start: (): Promise<SessionWriteResult> =>
            Promise.resolve({
              status: 'recorded',
              session: { id: 1 } as unknown as SessionRecord,
            }),
          finalize: (_sessionId, input): Promise<SessionWriteResult> => {
            finalized.push(input);
            return Promise.resolve({
              status: 'recorded',
              session: { id: 1 } as unknown as SessionRecord,
            });
          },
        }),
    };

    const { result } = renderHook(() => useAsunaSession(options));
    await flush(() => {
      result.current.start();
    });

    await act(async () => {
      await Promise.resolve();
      harness.service().emit({
        type: 'transcript',
        entry: { itemId: 'item_1', role: 'user', text: 'merhaba', status: 'completed' },
      });
      harness.service().emit({
        type: 'tool_result',
        toolName: 'read_project_file',
        risk: 0,
        outcome: 'succeeded',
        approvalState: 'not_required',
        summary: 'README.md okundu (2.1 KB)',
      });
    });

    await flush(() => {
      result.current.stop();
    });

    const roles = finalized.at(-1)?.transcript?.map((line) => line.role) ?? [];
    expect(roles).toEqual(['user']);
  });
});

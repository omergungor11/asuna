/**
 * `VoicePanel` testleri (ASU-015).
 *
 * Iki sey kanitlanir:
 * 1. Guven yuzeyi her an ekranda: durum rozeti, mikrofon gostergesi, tek buton.
 * 2. Panel bir sohbet penceresi **degil** — metin girisi / gonder butonu yok
 *    (CLAUDE.md prime directive).
 */

import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type {
  AsunaRealtimeEvent,
  AsunaRealtimeEventListener,
} from '../asuna/agent/realtime-events';
import type {
  AsunaSessionPort,
  UseAsunaSessionOptions,
} from '../asuna/agent/use-asuna-session';
import { MicrophoneAccessError, type MicrophoneProbe } from '../asuna/audio/microphone-access';
import type { FrontendConfig } from '../asuna/config/frontend-config';
import { AsunaLogger } from '../asuna/observability';
import type { CurrentProjectPort } from '../asuna/projects/use-current-project';
import {
  VoiceStateMachine,
  type VoiceState,
  type VoiceTransitionReason,
} from '../asuna/state/voice-state-machine';
import type { ProjectRecord } from '../shared/project';

import { VoicePanel } from './voice-panel';

const ASUNA_PROJECT: ProjectRecord = {
  id: 'asuna',
  name: 'Asuna',
  path: '/Users/arlec/Work/asuna',
  description: null,
  status: 'active',
  primaryLanguage: 'TypeScript',
  framework: 'React',
  gitRemote: null,
  lastOpenedAt: '2026-08-24T09:30:00Z',
  createdAt: '2026-08-01T09:30:00Z',
  updatedAt: '2026-08-24T09:30:00Z',
  metadataJson: '{}',
};

/** Proje kaydi ayri bir IPC yuzeyi; testte gercek `invoke`'a gidilmez. */
const NO_PROJECTS: CurrentProjectPort = {
  list: (): Promise<readonly ProjectRecord[]> => Promise.resolve([]),
};

const CONFIG: FrontendConfig = {
  realtimeModel: 'gpt-realtime-2.1-mini',
  realtimeVoice: null,
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

const GRANTED: MicrophoneProbe = { echoCancellation: true, noiseSuppression: true };

function createOptions(probe?: () => Promise<MicrophoneProbe>): UseAsunaSessionOptions {
  return {
    stateMachine: new VoiceStateMachine(),
    logger: new AsunaLogger(),
    loadConfig: (): Promise<FrontendConfig> => Promise.resolve(CONFIG),
    probeMicrophone: probe ?? ((): Promise<MicrophoneProbe> => Promise.resolve(GRANTED)),
    createService: (context): AsunaSessionPort => {
      const listeners = new Set<AsunaRealtimeEventListener>();
      const publish = (event: AsunaRealtimeEvent): void => {
        for (const listener of [...listeners]) {
          listener(event);
        }
      };

      return {
        connect: (): Promise<void> => {
          context.stateMachine.transition('CONNECTING', 'REALTIME_CONNECTING');
          context.stateMachine.transition('LISTENING', 'REALTIME_CONNECTED');
          publish({ type: 'connected', model: context.config.realtimeModel });
          return Promise.resolve();
        },
        disconnect: (): void => {
          context.stateMachine.transition('BOOTING', 'SESSION_CLOSED_BY_USER');
          publish({ type: 'disconnected', reason: 'requested' });
        },
        interrupt: (): void => undefined,
        // ASU-048: onay yolu bu panelde kullanilmiyor; port sozlesmesi geregi var.
        approveToolCall: (): void => undefined,
        rejectToolCall: (): void => undefined,
        subscribe: (listener): (() => void) => {
          listeners.add(listener);
          return (): void => {
            listeners.delete(listener);
          };
        },
        getState: () => context.stateMachine.getState(),
      };
    },
  };
}

/** ASU-053 onay istegi — kartin gostermesi gereken tum alanlarla. */
const APPROVAL_REQUEST: AsunaRealtimeEvent = {
  type: 'tool_approval_requested',
  requestId: 'req-7',
  toolName: 'open_project',
  description: 'Kayıtlı bir projeyi yapılandırılmış editörde açar.',
  risk: 1,
  argumentsPreview: 'projectId=asuna',
  timeoutMs: 60_000,
};

interface LiveHarness {
  readonly options: UseAsunaSessionOptions;
  /** Servis event'i yayinlar (hook reducer'ini besler). */
  readonly emit: (event: AsunaRealtimeEvent) => void;
  /** Durum makinesini surer — gercekte bunu servis yapar. */
  readonly transition: (to: VoiceState, reason: VoiceTransitionReason) => void;
  /** Servise ulasan onay/red kimlikleri: karar **kimlikle** verilmeli. */
  readonly approvals: readonly string[];
  readonly rejections: readonly string[];
}

/**
 * Canli bir oturumu taklit eden kosum takimi: event yayinlanabilir, durum
 * suruilebilir ve onay cagrilari kaydedilir. Gercek SDK'ya dokunulmaz.
 */
function createLiveHarness(): LiveHarness {
  const machine = new VoiceStateMachine();
  const approvals: string[] = [];
  const rejections: string[] = [];
  const listeners = new Set<AsunaRealtimeEventListener>();

  const publish = (event: AsunaRealtimeEvent): void => {
    for (const listener of [...listeners]) {
      listener(event);
    }
  };

  const options: UseAsunaSessionOptions = {
    ...createOptions(),
    stateMachine: machine,
    createService: (context): AsunaSessionPort => ({
      connect: (): Promise<void> => {
        context.stateMachine.transition('CONNECTING', 'REALTIME_CONNECTING');
        context.stateMachine.transition('LISTENING', 'REALTIME_CONNECTED');
        publish({ type: 'connected', model: context.config.realtimeModel });
        return Promise.resolve();
      },
      disconnect: (): void => undefined,
      interrupt: (): void => undefined,
      approveToolCall: (requestId: string): void => {
        approvals.push(requestId);
      },
      rejectToolCall: (requestId: string): void => {
        rejections.push(requestId);
      },
      subscribe: (listener): (() => void) => {
        listeners.add(listener);
        return (): void => {
          listeners.delete(listener);
        };
      },
      getState: () => context.stateMachine.getState(),
    }),
  };

  return {
    options,
    emit: (event): void => {
      act(() => {
        publish(event);
      });
    },
    transition: (to, reason): void => {
      act(() => {
        machine.transition(to, reason);
      });
    },
    approvals,
    rejections,
  };
}

async function click(name: string): Promise<void> {
  const button = screen.getByRole('button', { name });
  await act(async () => {
    fireEvent.click(button);
    await Promise.resolve();
  });
}

describe('VoicePanel', () => {
  it('durum rozetini ve mikrofon gostergesini her an gosterir', () => {
    render(<VoicePanel options={createOptions()} projectPort={NO_PROJECTS} />);

    expect(screen.getByRole('status')).toHaveTextContent('Bağlı değil');
    expect(screen.getByText('Mikrofon kapalı')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Talk to Asuna' })).toBeEnabled();
  });

  it('tek buton: baglaninca "Stop" olur, tekrar basinca kapanir', async () => {
    render(<VoicePanel options={createOptions()} projectPort={NO_PROJECTS} />);

    await click('Talk to Asuna');

    expect(screen.getByRole('button', { name: 'Stop' })).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Dinliyor');
    expect(screen.getByText('Mikrofon açık')).toBeInTheDocument();
    expect(screen.getByText(CONFIG.realtimeModel)).toBeInTheDocument();

    await click('Stop');

    expect(screen.getByRole('button', { name: 'Talk to Asuna' })).toBeInTheDocument();
    expect(screen.getByText('Mikrofon kapalı')).toBeInTheDocument();
  });

  it('mikrofon izni reddedilince kurulum yonlendirmesi gosterir', async () => {
    render(
      <VoicePanel
        options={createOptions((): Promise<MicrophoneProbe> =>
          Promise.reject(new MicrophoneAccessError('mic_permission_denied', 'reddedildi')),
        )}
        projectPort={NO_PROJECTS}
      />,
    );

    await click('Talk to Asuna');

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Mikrofona erişemiyorum');
    expect(alert).toHaveTextContent('Sistem Ayarları > Gizlilik ve Güvenlik > Mikrofon');
    expect(screen.getByRole('status')).toHaveTextContent('Hata');
    // Izin hatasi duzeltilebilir: buton tekrar denemeye acik kalir.
    expect(screen.getByRole('button', { name: 'Talk to Asuna' })).toBeEnabled();
  });

  /**
   * ASU-045: "mevcut proje" overlay'de de gorunur (PROJECT.md Bolum 19).
   * Asuna yanlis projede oldugunu saniyorsa kullanici bunu konusmadan once
   * gormeli.
   */
  it('guncel projeyi gosterir; secim yoksa uydurmaz', async () => {
    const { unmount } = render(
      <VoicePanel options={createOptions()} projectPort={NO_PROJECTS} />,
    );

    expect(await screen.findByText('seçilmedi')).toBeInTheDocument();
    unmount();

    render(
      <VoicePanel
        options={createOptions()}
        projectPort={{
          list: (): Promise<readonly ProjectRecord[]> => Promise.resolve([ASUNA_PROJECT]),
        }}
      />,
    );

    expect(await screen.findByText('Asuna')).toBeInTheDocument();
  });

  it('proje kaydi okunamazsa "proje yok" gibi gostermez', async () => {
    render(
      <VoicePanel
        options={createOptions()}
        projectPort={{
          list: (): Promise<readonly ProjectRecord[]> =>
            Promise.reject(new Error('proje kaydi kullanilamiyor')),
        }}
      />,
    );

    expect(
      await screen.findByText(/okunamadı: proje kaydi kullanilamiyor/),
    ).toBeInTheDocument();
  });

  /**
   * ASU-044 kabul kriteri: "tool cagrisi UI'da gorunuyor". Asuna arka planda bir
   * sey calistiriyorsa kullanici bunu **o an** gormeli (PROJECT.md Bolum 21).
   */
  it('calisan tool"un adini "Aktif araç" satirinda gosterir', async () => {
    let publish: ((event: AsunaRealtimeEvent) => void) | null = null;
    const options: UseAsunaSessionOptions = {
      ...createOptions(),
      createService: (context): AsunaSessionPort => {
        const listeners = new Set<AsunaRealtimeEventListener>();
        publish = (event: AsunaRealtimeEvent): void => {
          for (const listener of [...listeners]) {
            listener(event);
          }
        };
        return {
          connect: (): Promise<void> => {
            context.stateMachine.transition('CONNECTING', 'REALTIME_CONNECTING');
            context.stateMachine.transition('LISTENING', 'REALTIME_CONNECTED');
            publish?.({ type: 'connected', model: context.config.realtimeModel });
            return Promise.resolve();
          },
          disconnect: (): void => undefined,
          interrupt: (): void => undefined,
          approveToolCall: (): void => undefined,
          rejectToolCall: (): void => undefined,
          subscribe: (listener): (() => void) => {
            listeners.add(listener);
            return (): void => {
              listeners.delete(listener);
            };
          },
          getState: () => context.stateMachine.getState(),
        };
      },
    };

    render(<VoicePanel options={options} projectPort={NO_PROJECTS} />);
    await click('Talk to Asuna');

    const emit = (event: AsunaRealtimeEvent): void => {
      act(() => {
        publish?.(event);
      });
    };

    emit({ type: 'tool_call_started', toolName: 'get_current_project' });
    expect(screen.getByText('get_current_project')).toBeInTheDocument();

    // Tool bitince satir bosalir: biten bir is "hala calisiyor" gibi durmaz.
    emit({ type: 'tool_call_completed', toolName: 'get_current_project' });
    expect(screen.queryByText('get_current_project')).not.toBeInTheDocument();
  });

  /**
   * ASU-053: onay gerektiren bir tool cagrisi kart olmadan **calismaz**; kart
   * kullanicinin kapisi. Panelin disina portal edilmesi de kasitli: baska bir
   * sekme acikken Konusma paneli `hidden` olur, istek gorunmez kalmamali.
   */
  it('onay istegini panelin disinda, kart olarak gosterir', async () => {
    const harness = createLiveHarness();
    render(<VoicePanel options={harness.options} projectPort={NO_PROJECTS} />);
    await click('Talk to Asuna');

    harness.transition('ASSISTANT_THINKING', 'ASSISTANT_RESPONSE_STARTED');
    harness.transition('AWAITING_APPROVAL', 'TOOL_APPROVAL_REQUESTED');
    harness.emit(APPROVAL_REQUEST);

    const card = screen.getByRole('dialog', { name: 'Araç onayı' });
    expect(card).toHaveTextContent('open_project');
    expect(card).toHaveTextContent('Risk 1 · geri alınabilir');
    expect(card).toHaveTextContent('projectId=asuna');
    // Durum rozeti de ayni seyi soyler: iki gosterge birbirini dogrular.
    expect(screen.getByText('Onay bekliyor')).toBeInTheDocument();
    // Portal hedefi `document.body`: panel `hidden` olsa da kart kaybolmaz.
    expect(card.parentElement).toBe(document.body);
  });

  it('onay karari servise requestId ile gider', async () => {
    const harness = createLiveHarness();
    render(<VoicePanel options={harness.options} projectPort={NO_PROJECTS} />);
    await click('Talk to Asuna');

    harness.transition('ASSISTANT_THINKING', 'ASSISTANT_RESPONSE_STARTED');
    harness.transition('AWAITING_APPROVAL', 'TOOL_APPROVAL_REQUESTED');
    harness.emit(APPROVAL_REQUEST);

    fireEvent.click(screen.getByRole('button', { name: 'Onayla: open_project' }));
    expect(harness.approvals).toEqual(['req-7']);
    expect(harness.rejections).toEqual([]);

    harness.emit({
      type: 'tool_approval_resolved',
      requestId: 'req-7',
      toolName: 'open_project',
      outcome: 'approved',
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('reddetme de ayni kimlikle gider', async () => {
    const harness = createLiveHarness();
    render(<VoicePanel options={harness.options} projectPort={NO_PROJECTS} />);
    await click('Talk to Asuna');

    harness.transition('ASSISTANT_THINKING', 'ASSISTANT_RESPONSE_STARTED');
    harness.transition('AWAITING_APPROVAL', 'TOOL_APPROVAL_REQUESTED');
    harness.emit(APPROVAL_REQUEST);

    fireEvent.click(screen.getByRole('button', { name: 'Reddet: open_project' }));

    expect(harness.rejections).toEqual(['req-7']);
    expect(harness.approvals).toEqual([]);
  });

  /** ASU-054: "tool calisirken TOOL_PENDING durumu ve tool adi gorunuyor". */
  it('TOOL_PENDING durumunda hem rozet hem tool adi gorunur', async () => {
    const harness = createLiveHarness();
    render(<VoicePanel options={harness.options} projectPort={NO_PROJECTS} />);
    await click('Talk to Asuna');

    harness.transition('ASSISTANT_THINKING', 'ASSISTANT_RESPONSE_STARTED');
    harness.transition('TOOL_PENDING', 'TOOL_CALL_STARTED');
    harness.emit({ type: 'tool_call_started', toolName: 'read_project_file' });

    expect(screen.getByText('Araç çalışıyor')).toBeInTheDocument();
    expect(screen.getByText('read_project_file')).toBeInTheDocument();
  });

  it('sohbet arayuzu degil: metin girisi ve gonder butonu yok', () => {
    render(<VoicePanel options={createOptions()} projectPort={NO_PROJECTS} />);

    expect(screen.queryByRole('textbox')).toBeNull();
    expect(screen.queryAllByRole('button')).toHaveLength(1);
  });
});

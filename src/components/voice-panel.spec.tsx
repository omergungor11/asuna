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
import { VoiceStateMachine } from '../asuna/state/voice-state-machine';
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

  it('sohbet arayuzu degil: metin girisi ve gonder butonu yok', () => {
    render(<VoicePanel options={createOptions()} projectPort={NO_PROJECTS} />);

    expect(screen.queryByRole('textbox')).toBeNull();
    expect(screen.queryAllByRole('button')).toHaveLength(1);
  });
});

/**
 * Mikrofon izin sondasi testleri (ASU-015 / ASU-016).
 *
 * Gercek `getUserMedia` **hic** cagrilmaz: sahte bir opener enjekte edilir
 * (`conventions.md` — "test gercek API'ye/mikrofona vurmaz").
 */

import { describe, expect, it, vi } from 'vitest';

import {
  MICROPHONE_CONSTRAINTS,
  MicrophoneAccessError,
  probeMicrophoneAccess,
  type MicrophoneStreamLike,
  type MicrophoneTrackLike,
} from './microphone-access';

interface FakeTrack extends MicrophoneTrackLike {
  readonly stopCalls: () => number;
}

function createTrack(
  settings: MediaTrackSettings = { echoCancellation: true, noiseSuppression: true },
  onGetSettings?: () => void,
): FakeTrack {
  let stops = 0;
  return {
    stop: (): void => {
      stops += 1;
    },
    getSettings: (): MediaTrackSettings => {
      onGetSettings?.();
      return settings;
    },
    stopCalls: (): number => stops,
  };
}

function createStream(tracks: readonly MicrophoneTrackLike[]): MicrophoneStreamLike {
  return { getTracks: (): readonly MicrophoneTrackLike[] => tracks };
}

function namedError(name: string): Error {
  const error = new Error(`${name} olustu`);
  error.name = name;
  return error;
}

describe('probeMicrophoneAccess', () => {
  it('echo cancellation ve noise suppression kisitlarini acikca istiyor', async () => {
    const open = vi.fn(() => Promise.resolve(createStream([createTrack()])));

    await probeMicrophoneAccess(open);

    expect(open).toHaveBeenCalledWith({
      audio: { echoCancellation: true, noiseSuppression: true },
      video: false,
    });
    expect(MICROPHONE_CONSTRAINTS.echoCancellation).toBe(true);
    expect(MICROPHONE_CONSTRAINTS.noiseSuppression).toBe(true);
  });

  it('cihazda gercekten uygulanan ayarlari doner', async () => {
    const track = createTrack({ echoCancellation: true, noiseSuppression: false });

    const probe = await probeMicrophoneAccess(() => Promise.resolve(createStream([track])));

    expect(probe).toEqual({ echoCancellation: true, noiseSuppression: false });
  });

  it('okunamayan ayari `false` degil `null` raporlar', async () => {
    const track = createTrack({});

    const probe = await probeMicrophoneAccess(() => Promise.resolve(createStream([track])));

    expect(probe).toEqual({ echoCancellation: null, noiseSuppression: null });
  });

  it('sondadan sonra track’leri hemen durdurur (mikrofon acik kalmaz)', async () => {
    const first = createTrack();
    const second = createTrack();

    await probeMicrophoneAccess(() => Promise.resolve(createStream([first, second])));

    expect(first.stopCalls()).toBe(1);
    expect(second.stopCalls()).toBe(1);
  });

  it('ayar okunurken hata olsa bile track durduruluyor', async () => {
    const track = createTrack({}, () => {
      throw new Error('getSettings patladi');
    });

    await expect(
      probeMicrophoneAccess(() => Promise.resolve(createStream([track]))),
    ).rejects.toThrow('getSettings patladi');
    expect(track.stopCalls()).toBe(1);
  });

  it('izin reddini `mic_permission_denied` olarak etiketler', async () => {
    const attempt = probeMicrophoneAccess(() => Promise.reject(namedError('NotAllowedError')));

    await expect(attempt).rejects.toBeInstanceOf(MicrophoneAccessError);
    await expect(attempt).rejects.toMatchObject({ kind: 'mic_permission_denied' });
  });

  it('SecurityError de izin reddi sayilir', async () => {
    await expect(
      probeMicrophoneAccess(() => Promise.reject(namedError('SecurityError'))),
    ).rejects.toMatchObject({ kind: 'mic_permission_denied' });
  });

  it('cihaz bulunamadiginda `mic_unavailable` doner', async () => {
    await expect(
      probeMicrophoneAccess(() => Promise.reject(namedError('NotFoundError'))),
    ).rejects.toMatchObject({ kind: 'mic_unavailable' });
  });

  it('taninmayan hatayi uydurmadan `mic_unavailable` kovasina koyar', async () => {
    await expect(
      probeMicrophoneAccess(() => Promise.reject(new Error('kim bilir'))),
    ).rejects.toMatchObject({ kind: 'mic_unavailable' });
  });

  it('ses track’i gelmezse hata verir', async () => {
    await expect(
      probeMicrophoneAccess(() => Promise.resolve(createStream([]))),
    ).rejects.toMatchObject({ kind: 'mic_unavailable' });
  });
});

/**
 * Saglayici secimi config'ten geliyor mu (ASU-021 kabul kriteri).
 */

import { describe, expect, it, vi } from 'vitest';

import { FakeWakeWordProvider } from './fake-wake-word-provider';
import { SherpaKwsProvider } from './sherpa-kws-provider';
import {
  createWakeWordProvider,
  type WakeWordProviderConfig,
} from './wake-word-provider-factory';
import { WakeWordProviderError, type WakeWordProvider } from './wake-word-provider';

function config(overrides: Partial<WakeWordProviderConfig> = {}): WakeWordProviderConfig {
  return {
    wakeWord: overrides.wakeWord ?? 'Hey Asuna',
    wakeWordProvider: overrides.wakeWordProvider ?? 'fake',
  };
}

describe('createWakeWordProvider', () => {
  it('`fake` icin FakeWakeWordProvider kurar', () => {
    const provider = createWakeWordProvider(config({ wakeWordProvider: 'fake' }));
    expect(provider).toBeInstanceOf(FakeWakeWordProvider);
  });

  it('`sherpa-kws` icin SherpaKwsProvider kurar', () => {
    const provider = createWakeWordProvider(config({ wakeWordProvider: 'sherpa-kws' }));
    expect(provider).toBeInstanceOf(SherpaKwsProvider);
  });

  it('bilinmeyen saglayiciyi sessizce fake"e dusurmez, hata firlatir', () => {
    // Dogrulanmamis bir config (Rust ile renderer sozlesmesi ayrildi) taklidi.
    const broken = { wakeWord: 'Hey Asuna', wakeWordProvider: 'porcupine' };

    expect(() => createWakeWordProvider(broken as WakeWordProviderConfig)).toThrow(
      WakeWordProviderError,
    );
    try {
      createWakeWordProvider(broken as WakeWordProviderConfig);
    } catch (error) {
      expect(error).toMatchObject({ kind: 'unsupported_provider' });
      // Hata mesaji gelen degeri tekrarlamaz (FrontendConfigError ile ayni politika).
      expect(error instanceof Error ? error.message : '').not.toContain('porcupine');
    }
  });

  it('tetikleyici ifadeyi config"ten gecirir (koda gomulu degil)', async () => {
    const provider = createWakeWordProvider(
      config({ wakeWord: 'Selam Asuna', wakeWordProvider: 'fake' }),
    );

    expect(provider).toBeInstanceOf(FakeWakeWordProvider);
    const phrases: string[] = [];
    provider.onDetected((event) => phrases.push(event.phrase));

    await provider.initialize();
    await provider.start();
    (provider as FakeWakeWordProvider).trigger();
    await provider.stop();

    expect(phrases).toStrictEqual(['Selam Asuna']);
  });

  it('donen deger `WakeWordProvider` arayuzunun tamamini karsilar', async () => {
    // Tip **ve** calisma zamani: dort metodun dordu de arayuz uzerinden cagriliyor.
    const provider: WakeWordProvider = createWakeWordProvider(config());

    await expect(provider.initialize()).resolves.toBeUndefined();
    await expect(provider.start()).resolves.toBeUndefined();
    const unsubscribe = provider.onDetected(vi.fn());
    unsubscribe();
    await expect(provider.stop()).resolves.toBeUndefined();
  });
});

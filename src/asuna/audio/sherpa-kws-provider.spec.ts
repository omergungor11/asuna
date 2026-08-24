/**
 * `SherpaKwsProvider` iskeletinin **durust hata** davranisi (ASU-021).
 *
 * Bu testlerin tamami ASU-022'de degisecek — bilerek. Amaclari, motor gelene
 * kadar gecen surede kimsenin bu stub'i "calisiyor" sanmamasi.
 */

import { describe, expect, it, vi } from 'vitest';

import { SherpaKwsProvider, WAKE_WORD_DETECTED_EVENT } from './sherpa-kws-provider';
import { WakeWordProviderError } from './wake-word-provider';

function createProvider(): SherpaKwsProvider {
  return new SherpaKwsProvider({ phrase: 'Hey Asuna' });
}

describe('SherpaKwsProvider (ASU-022 oncesi iskelet)', () => {
  it('initialize() sessizce basari dondurmez, durustce hata verir', async () => {
    const provider = createProvider();

    await expect(provider.initialize()).rejects.toBeInstanceOf(WakeWordProviderError);
    await expect(provider.initialize()).rejects.toMatchObject({ kind: 'not_implemented' });
  });

  it('hata mesaji ne yapilacagini soyler (ASU-022 + fake fallback)', async () => {
    const error = await createProvider()
      .initialize()
      .catch((reason: unknown) => reason);

    expect(error).toBeInstanceOf(Error);
    const message = error instanceof Error ? error.message : '';
    expect(message).toContain('ASU-022');
    expect(message).toContain('ASUNA_WAKE_WORD_PROVIDER=fake');
  });

  it('start() kurulmamis motorda hata verir', async () => {
    await expect(createProvider().start()).rejects.toMatchObject({ kind: 'not_initialized' });
  });

  it('stop() her durumda guvenli — kapanis yolunu bozmaz', async () => {
    await expect(createProvider().stop()).resolves.toBeUndefined();
  });

  it('onDetected() abonelik kabul eder ve unsubscribe dondurur', () => {
    const provider = createProvider();
    const unsubscribe = provider.onDetected(vi.fn());

    expect(unsubscribe).toBeTypeOf('function');
    expect(() => {
      unsubscribe();
    }).not.toThrow();
  });

  it('Tauri event adi sabit — Rust tarafi (ASU-022) ayni adi kullanacak', () => {
    expect(WAKE_WORD_DETECTED_EVENT).toBe('asuna://wake-word-detected');
  });
});

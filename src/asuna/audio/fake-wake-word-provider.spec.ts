/**
 * `WakeWordProvider` sozlesme testleri (ASU-021).
 *
 * Sozlesme somut motorla degil, **fake** saglayiciyla dogrulanir: ASU-022 gercek
 * motoru getirdiginde ayni testler onun icin de anlamli kalir (conventions.md
 * "Testing": harici servisler mock'lanir, test gercek mikrofona vurmaz).
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  FAKE_WAKE_GLOBAL,
  FakeWakeWordProvider,
  type FakeWakeWordDebugTarget,
} from './fake-wake-word-provider';
import { WakeWordProviderError, type WakeWordEvent } from './wake-word-provider';

const PHRASE = 'Hey Asuna';
const FIXED_NOW = new Date('2026-08-24T09:30:00.000Z');

function createProvider(
  overrides: Partial<{
    debugTarget: FakeWakeWordDebugTarget;
    installDebugTrigger: boolean;
  }> = {},
): FakeWakeWordProvider {
  return new FakeWakeWordProvider({
    phrase: PHRASE,
    installDebugTrigger: overrides.installDebugTrigger ?? false,
    debugTarget: overrides.debugTarget ?? {},
    now: (): Date => FIXED_NOW,
  });
}

async function startedProvider(
  overrides: Partial<{
    debugTarget: FakeWakeWordDebugTarget;
    installDebugTrigger: boolean;
  }> = {},
): Promise<FakeWakeWordProvider> {
  const provider = createProvider(overrides);
  await provider.initialize();
  await provider.start();
  return provider;
}

describe('FakeWakeWordProvider — yasam dongusu', () => {
  it('initialize -> start -> trigger zinciri dinleyiciyi cagirir', async () => {
    const provider = await startedProvider();
    const events: WakeWordEvent[] = [];
    provider.onDetected((event) => events.push(event));

    expect(provider.trigger()).toBe(true);

    expect(events).toStrictEqual([
      { phrase: PHRASE, confidence: null, at: FIXED_NOW.toISOString() },
    ]);
  });

  it('initialize edilmeden start() cagrilirsa durustce hata verir', async () => {
    const provider = createProvider();

    await expect(provider.start()).rejects.toBeInstanceOf(WakeWordProviderError);
    await expect(provider.start()).rejects.toMatchObject({ kind: 'not_initialized' });
    expect(provider.isRunning()).toBe(false);
  });

  it('sahte motor akustik skor uydurmaz (confidence varsayilani null)', async () => {
    const provider = await startedProvider();
    const events: WakeWordEvent[] = [];
    provider.onDetected((event) => events.push(event));

    provider.trigger({ confidence: 0.42, phrase: 'Asuna' });

    expect(events).toStrictEqual([
      { phrase: 'Asuna', confidence: 0.42, at: FIXED_NOW.toISOString() },
    ]);
  });

  it('stop() sonrasi tetik gelmez ve dinleyici cagrilmaz', async () => {
    const provider = await startedProvider();
    const listener = vi.fn();
    provider.onDetected(listener);

    await provider.stop();

    expect(provider.isRunning()).toBe(false);
    expect(provider.trigger()).toBe(false);
    expect(listener).not.toHaveBeenCalled();
  });

  it('stop() baslamamis saglayicida da guvenli (kapanis yolu her durumda cagirir)', async () => {
    const provider = createProvider();
    await expect(provider.stop()).resolves.toBeUndefined();
  });

  it('stop() sonrasi tekrar start() ile uyanabilir (ASU-026 race yok)', async () => {
    const provider = await startedProvider();
    const listener = vi.fn();
    provider.onDetected(listener);

    await provider.stop();
    await provider.start();

    expect(provider.trigger()).toBe(true);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe('FakeWakeWordProvider — abonelik', () => {
  it('coklu dinleyicinin hepsi cagrilir', async () => {
    const provider = await startedProvider();
    const first = vi.fn();
    const second = vi.fn();
    provider.onDetected(first);
    provider.onDetected(second);

    provider.trigger();

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('unsubscribe yalnizca kendi dinleyicisini kaldirir', async () => {
    const provider = await startedProvider();
    const removed = vi.fn();
    const kept = vi.fn();
    const unsubscribe = provider.onDetected(removed);
    provider.onDetected(kept);

    unsubscribe();
    provider.trigger();

    expect(removed).not.toHaveBeenCalled();
    expect(kept).toHaveBeenCalledTimes(1);
  });

  it('unsubscribe iki kez cagrilabilir', async () => {
    const provider = await startedProvider();
    const listener = vi.fn();
    const unsubscribe = provider.onDetected(listener);

    unsubscribe();
    expect(() => {
      unsubscribe();
    }).not.toThrow();

    provider.trigger();
    expect(listener).not.toHaveBeenCalled();
  });

  it('bir dinleyicinin hatasi digerlerini engellemez (AggregateError ile bildirilir)', async () => {
    const provider = await startedProvider();
    const healthy = vi.fn();
    provider.onDetected(() => {
      throw new Error('bozuk debug paneli');
    });
    provider.onDetected(healthy);

    expect(() => provider.trigger()).toThrow(AggregateError);
    expect(healthy).toHaveBeenCalledTimes(1);
  });
});

describe('FakeWakeWordProvider — debug tetikleyici', () => {
  it('global yalnizca calisirken durur, stop() ile kaldirilir', async () => {
    const target: FakeWakeWordDebugTarget = {};
    const provider = await startedProvider({ debugTarget: target, installDebugTrigger: true });

    const trigger = target[FAKE_WAKE_GLOBAL];
    expect(trigger).toBeTypeOf('function');

    const listener = vi.fn();
    provider.onDetected(listener);
    expect(trigger?.()).toBe(true);
    expect(listener).toHaveBeenCalledTimes(1);

    await provider.stop();
    expect(target[FAKE_WAKE_GLOBAL]).toBeUndefined();
    // Elde kalan referans da artik tetiklemez.
    expect(trigger?.()).toBe(false);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('global ifadeyi override edebilir', async () => {
    const target: FakeWakeWordDebugTarget = {};
    const provider = await startedProvider({ debugTarget: target, installDebugTrigger: true });
    const events: WakeWordEvent[] = [];
    provider.onDetected((event) => events.push(event));

    target[FAKE_WAKE_GLOBAL]?.('Asuna beni toparla');

    expect(events.map((event) => event.phrase)).toStrictEqual(['Asuna beni toparla']);
  });

  it('kapaliyken global kurulmaz', async () => {
    const target: FakeWakeWordDebugTarget = {};
    await startedProvider({ debugTarget: target, installDebugTrigger: false });

    expect(target[FAKE_WAKE_GLOBAL]).toBeUndefined();
  });

  it('ayni global ikinci kez kurulmaya calisilirsa hata verir', async () => {
    const target: FakeWakeWordDebugTarget = {};
    await startedProvider({ debugTarget: target, installDebugTrigger: true });
    const second = createProvider({ debugTarget: target, installDebugTrigger: true });
    await second.initialize();

    await expect(second.start()).rejects.toMatchObject({ kind: 'engine_unavailable' });
  });
});

describe('FakeWakeWordProvider — gizlilik', () => {
  afterEach(() => {
    Reflect.deleteProperty(globalThis.navigator, 'mediaDevices');
    vi.restoreAllMocks();
  });

  /**
   * PROJECT.md Bolum 8 / ADR-004: idle'da renderer mikrofona **hic** dokunmaz.
   * Fake saglayici bu sozun test yolundaki karsiligidir — dev'de `fake` ile
   * calisan bir Asuna da idle'da hicbir sey dinlememeli.
   */
  it('yasam dongusunun hicbir adiminda getUserMedia cagirmaz', async () => {
    const getUserMedia = vi.fn();
    Object.defineProperty(globalThis.navigator, 'mediaDevices', {
      value: { getUserMedia },
      configurable: true,
    });

    const provider = createProvider();
    provider.onDetected(vi.fn());
    await provider.initialize();
    await provider.start();
    provider.trigger();
    await provider.stop();

    expect(getUserMedia).not.toHaveBeenCalled();
  });
});

/**
 * `privacy-service` testleri (ASU-037).
 *
 * Kanitlanan seyler:
 * 1. ACL'de kayitli komut adlari kullaniliyor (yazim hatasi sessiz bir red olurdu).
 * 2. Yanit **dogrulaniyor**; sozlesme disi payload kabul edilmiyor.
 * 3. `locked-by-env` reddi tipli hataya cevriliyor ve mesaji korunuyor.
 * 4. Ayar onbelleklenmiyor.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AsunaPrivacyError, PrivacyContractError } from '../../shared/privacy';

import {
  PRIVACY_COMMANDS,
  fetchPrivacySettings,
  updatePrivacySettings,
} from './privacy-service';

const invokeMock = vi.hoisted(() =>
  vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(),
);

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const SETTINGS = {
  memoryEnabled: true,
  transcriptStorage: false,
  memoryEnabledAtBoot: true,
  transcriptStorageAtBoot: false,
};

describe('privacy-service', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('ACL"de kayitli komut adlarini kullanir', async () => {
    invokeMock.mockResolvedValue(SETTINGS);

    await fetchPrivacySettings();
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('get_privacy_settings');

    invokeMock.mockClear();
    await updatePrivacySettings({ memoryEnabled: false });
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('set_privacy_settings', {
      patch: { memoryEnabled: false },
    });

    expect(PRIVACY_COMMANDS).toEqual({
      get: 'get_privacy_settings',
      set: 'set_privacy_settings',
    });
  });

  it('yaniti dogrular', async () => {
    invokeMock.mockResolvedValue(SETTINGS);
    await expect(fetchPrivacySettings()).resolves.toEqual(SETTINGS);
  });

  it('sozlesmeye uymayan yaniti reddeder', async () => {
    invokeMock.mockResolvedValue({ memoryEnabled: true });
    await expect(fetchPrivacySettings()).rejects.toBeInstanceOf(PrivacyContractError);

    invokeMock.mockResolvedValue({ ...SETTINGS, envPath: '/Users/x/.env' });
    await expect(fetchPrivacySettings()).rejects.toBeInstanceOf(PrivacyContractError);
  });

  /** Reddedilen bir gevsetme "bozuk" degil, kuraldir — kodu korunmali. */
  it('locked-by-env reddini tipli hataya cevirir', async () => {
    invokeMock.mockRejectedValue({
      code: 'locked-by-env',
      message: '`ASUNA_MEMORY_ENABLED` acilista kapatilmis',
    });

    const error = await updatePrivacySettings({ memoryEnabled: true }).catch(
      (value: unknown) => value,
    );

    expect(error).toBeInstanceOf(AsunaPrivacyError);
    expect((error as AsunaPrivacyError).isLockedByEnv).toBe(true);
    expect((error as AsunaPrivacyError).message).toContain('ASUNA_MEMORY_ENABLED');
  });

  it('ACL reddini (duz string) yutmaz', async () => {
    invokeMock.mockRejectedValue('set_privacy_settings not allowed on window "x"');

    const error = await fetchPrivacySettings().catch((value: unknown) => value);

    expect(error).toBeInstanceOf(AsunaPrivacyError);
    expect((error as AsunaPrivacyError).code).toBe('unknown');
    expect((error as AsunaPrivacyError).message).toContain('not allowed');
  });

  it('durumu onbelleklemez', async () => {
    invokeMock.mockResolvedValue(SETTINGS);

    await fetchPrivacySettings();
    await fetchPrivacySettings();

    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
});

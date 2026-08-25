/**
 * `shared/privacy` sozlesme testleri (ASU-037).
 *
 * Rust aynasi: `src-tauri/src/privacy.rs` (`settings_serialize_with_the_expected_contract`).
 */

import { describe, expect, it } from 'vitest';

import {
  AsunaPrivacyError,
  PRIVACY_SETTINGS_KEYS,
  PrivacyContractError,
  canEnableAtRuntime,
  parsePrivacySettings,
  toPrivacyError,
  type PrivacySettings,
} from './privacy';

const PAYLOAD = {
  memoryEnabled: false,
  transcriptStorage: true,
  memoryEnabledAtBoot: true,
  transcriptStorageAtBoot: true,
};

describe('parsePrivacySettings', () => {
  it('gecerli payload"u aynen dondurur', () => {
    expect(parsePrivacySettings(PAYLOAD)).toEqual(PAYLOAD);
  });

  it('sozlesme alanlari Rust ile ayni kumede', () => {
    expect([...PRIVACY_SETTINGS_KEYS].sort()).toEqual(
      [
        'memoryEnabled',
        'memoryEnabledAtBoot',
        'transcriptStorage',
        'transcriptStorageAtBoot',
      ].sort(),
    );
  });

  it('eksik alani reddeder', () => {
    expect(() =>
      parsePrivacySettings({
        memoryEnabled: false,
        transcriptStorage: true,
        memoryEnabledAtBoot: true,
      }),
    ).toThrow(PrivacyContractError);
  });

  /** Fazladan alan sessizce akmaz — bir gun `.env` yolu eklense gurultu cikar. */
  it('beklenmeyen alani reddeder', () => {
    expect(() => parsePrivacySettings({ ...PAYLOAD, envPath: '/Users/x/.env' })).toThrow(
      PrivacyContractError,
    );
  });

  it('boolean olmayan degeri reddeder', () => {
    expect(() => parsePrivacySettings({ ...PAYLOAD, memoryEnabled: 'true' })).toThrow(
      PrivacyContractError,
    );
    expect(() => parsePrivacySettings(null)).toThrow(PrivacyContractError);
  });

  /** Hata mesaji gelen degeri tekrarlamaz. */
  it('hata mesaji yalnizca alan adini soyler', () => {
    try {
      parsePrivacySettings({ ...PAYLOAD, transcriptStorage: 'gizli-deger' });
      expect.unreachable('hata bekleniyordu');
    } catch (error) {
      expect((error as Error).message).toContain('transcriptStorage');
      expect((error as Error).message).not.toContain('gizli-deger');
    }
  });
});

describe('canEnableAtRuntime', () => {
  /**
   * Kural: calisma zamani yalnizca **sikilastirir**. Acilista kapali olan bir
   * anahtar buradan acilamaz — DB dosyasi hic acilmadi.
   */
  it('acilista kapaliysa acmaya izin vermez', () => {
    const settings: PrivacySettings = {
      memoryEnabled: false,
      transcriptStorage: false,
      memoryEnabledAtBoot: false,
      transcriptStorageAtBoot: true,
    };

    expect(canEnableAtRuntime(settings, 'memoryEnabled')).toBe(false);
    expect(canEnableAtRuntime(settings, 'transcriptStorage')).toBe(true);
  });
});

describe('toPrivacyError', () => {
  it('tipli reddi kodu ve mesajiyla tasir', () => {
    const error = toPrivacyError({ code: 'locked-by-env', message: 'acilista kapali' });

    expect(error).toBeInstanceOf(AsunaPrivacyError);
    expect(error.code).toBe('locked-by-env');
    expect(error.isLockedByEnv).toBe(true);
    expect(error.message).toBe('acilista kapali');
  });

  it('taninmayan sekli uydurmaz ama mesaji korur', () => {
    expect(toPrivacyError('not allowed').code).toBe('unknown');
    expect(toPrivacyError(new Error('ipc down')).message).toBe('ipc down');
    expect(toPrivacyError({ code: 'uydurma', message: 'x' }).code).toBe('unknown');
    expect(toPrivacyError(undefined).message).toBe('Gizlilik ayari degistirilemedi.');
  });

  it('zaten tipli olan hatayi sarmalamaz', () => {
    const original = new AsunaPrivacyError('locked-by-env', 'x');
    expect(toPrivacyError(original)).toBe(original);
  });
});

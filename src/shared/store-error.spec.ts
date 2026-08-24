import { describe, expect, it } from 'vitest';

import {
  AsunaStoreError,
  STORE_ERROR_CODES,
  isStoreErrorCode,
  toStoreError,
} from './store-error';

describe('isStoreErrorCode', () => {
  it('Rust tarafindaki dort kodu tanir', () => {
    expect([...STORE_ERROR_CODES]).toEqual(['invalid', 'not-found', 'unavailable', 'storage']);
    for (const code of STORE_ERROR_CODES) {
      expect(isStoreErrorCode(code)).toBe(true);
    }
  });

  it('uydurulmus kodlari reddeder', () => {
    for (const value of ['notFound', 'disabled', '', 42, null]) {
      expect(isStoreErrorCode(value)).toBe(false);
    }
  });
});

describe('toStoreError', () => {
  it('Rust"un {code, message} bicimini tipli hataya cevirir', () => {
    const error = toStoreError({ code: 'not-found', message: 'kayit bulunamadi' });

    expect(error).toBeInstanceOf(AsunaStoreError);
    expect(error.code).toBe('not-found');
    expect(error.message).toBe('kayit bulunamadi');
    expect(error.isUnavailable).toBe(false);
  });

  /** "Kapali" degil "bozuk": UI bunu ariza olarak gostermeli (PROJECT.md 30). */
  it('unavailable"i ayirt eder', () => {
    expect(
      toStoreError({ code: 'unavailable', message: 'hafiza kullanilamiyor' }).isUnavailable,
    ).toBe(true);
  });

  /**
   * ACL reddi duz string olarak gelir. Uydurulmus bir koda eslenmez ama mesaji
   * da kaybolmaz — sessiz red en pahali hata turudur.
   */
  it('string reddi unknown koduyla korur', () => {
    const error = toStoreError('memory_create not allowed on window "main"');
    expect(error.code).toBe('unknown');
    expect(error.message).toContain('not allowed');
  });

  it('Error ornegini korur', () => {
    expect(toStoreError(new Error('ipc down')).message).toBe('ipc down');
  });

  it('taninmayan degeri durustce isaretler', () => {
    for (const value of [null, undefined, 42, {}, { code: 'nope', message: 'x' }]) {
      const error = toStoreError(value);
      expect(error.code).toBe('unknown');
      expect(error.message.length).toBeGreaterThan(0);
    }
  });

  it('zaten cevrilmis hatayi tekrar sarmalamaz', () => {
    const original = new AsunaStoreError('storage', 'veritabani islemi basarisiz');
    expect(toStoreError(original)).toBe(original);
  });
});

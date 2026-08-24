import { describe, expect, it } from 'vitest';

import {
  DB_AVAILABILITY_STATES,
  DbStatusError,
  isMemoryUsable,
  parseDbStatus,
} from './db-status';

const READY = {
  availability: 'ready',
  schemaVersion: 0,
  sqliteVersion: '3.53.2',
  reason: null,
};

describe('parseDbStatus', () => {
  it('gecerli bir payload"u dogrular', () => {
    expect(parseDbStatus(READY)).toEqual(READY);
  });

  it('uc durumu da kabul eder', () => {
    for (const availability of DB_AVAILABILITY_STATES) {
      const parsed = parseDbStatus({ ...READY, availability, schemaVersion: null });
      expect(parsed.availability).toBe(availability);
    }
  });

  it('bilinmeyen durumu reddeder', () => {
    expect(() => parseDbStatus({ ...READY, availability: 'degraded' })).toThrow(DbStatusError);
  });

  /**
   * Whitelist testi: backend yanlislikla fazladan bir alan dondurse (orn. DB
   * dosya yolu) bu sessizce renderer'a akmamali.
   */
  it('beklenmeyen alan iceren payload"u reddeder', () => {
    expect(() => parseDbStatus({ ...READY, databasePath: '/Users/x/asuna.db' })).toThrow(
      DbStatusError,
    );
  });

  it('hata mesaji gelen degeri tekrarlamaz', () => {
    const secretish = '/Users/x/Library/Application Support/com.omergungor.asuna/asuna.db';
    try {
      parseDbStatus({ ...READY, databasePath: secretish });
      expect.unreachable('hata bekleniyordu');
    } catch (error) {
      expect(error).toBeInstanceOf(DbStatusError);
      expect((error as Error).message).not.toContain(secretish);
    }
  });

  it('nesne olmayan payload"u reddeder', () => {
    for (const value of [null, undefined, 'ready', 42, []]) {
      expect(() => parseDbStatus(value)).toThrow(DbStatusError);
    }
  });

  it('gecersiz sema surumunu reddeder', () => {
    for (const schemaVersion of [-1, 1.5, '1']) {
      expect(() => parseDbStatus({ ...READY, schemaVersion })).toThrow(DbStatusError);
    }
  });

  it('bos sqlite surumunu reddeder', () => {
    expect(() => parseDbStatus({ ...READY, sqliteVersion: '' })).toThrow(DbStatusError);
  });

  it('gecersiz nedeni reddeder', () => {
    expect(() => parseDbStatus({ ...READY, reason: '' })).toThrow(DbStatusError);
    expect(() => parseDbStatus({ ...READY, reason: 12 })).toThrow(DbStatusError);
  });
});

describe('isMemoryUsable', () => {
  /**
   * `disabled` ve `unavailable` ayri durumlar ama ikisinde de hafizaya
   * yazilmaz — "hatirliyorum" iddiasi ikisinde de kurulamaz (PROJECT.md 39/10).
   */
  it('yalnizca ready durumunda true doner', () => {
    expect(isMemoryUsable(parseDbStatus(READY))).toBe(true);
    expect(
      isMemoryUsable(
        parseDbStatus({ ...READY, availability: 'disabled', schemaVersion: null }),
      ),
    ).toBe(false);
    expect(
      isMemoryUsable(
        parseDbStatus({
          ...READY,
          availability: 'unavailable',
          schemaVersion: null,
          reason: 'sema migration"lari uygulanamadi',
        }),
      ),
    ).toBe(false);
  });
});

import { describe, expect, it } from 'vitest';

import {
  MEMORY_KINDS,
  MemoryContractError,
  isMemoryKind,
  parseMemoryRecord,
  parseMemoryRecords,
  parseMemoryWriteResult,
  wasMemoryStored,
} from './memory';

const VALID = {
  id: 1,
  kind: 'decision',
  title: 'Wake word yerel kalir',
  content: 'Wake word tespiti bulutta degil, cihazda calisir.',
  summary: null,
  projectId: 'asuna',
  importance: 0.95,
  confidence: 1,
  sourceSessionId: 7,
  createdAt: '2026-08-25T10:00:00Z',
  updatedAt: '2026-08-25T10:00:00Z',
  lastAccessedAt: null,
  expiresAt: null,
  isArchived: false,
  metadataJson: '{}',
};

describe('isMemoryKind', () => {
  it('spec"teki on tipi tanir', () => {
    for (const kind of MEMORY_KINDS) {
      expect(isMemoryKind(kind)).toBe(true);
    }
  });

  /** ADR-005 B/3: uydurulmus bir kind DB'ye hic dokunmadan duser. */
  it('spec disi degerleri reddeder', () => {
    for (const value of ['project_decision', 'Preference', 'fact', '', 42, null]) {
      expect(isMemoryKind(value)).toBe(false);
    }
  });
});

describe('parseMemoryRecord', () => {
  it('gecerli bir kaydi dogrular', () => {
    expect(parseMemoryRecord(VALID)).toEqual(VALID);
  });

  it('on kind degerini de kabul eder', () => {
    for (const kind of MEMORY_KINDS) {
      expect(parseMemoryRecord({ ...VALID, kind }).kind).toBe(kind);
    }
  });

  it('bilinmeyen kind"i reddeder', () => {
    expect(() => parseMemoryRecord({ ...VALID, kind: 'project_decision' })).toThrow(
      MemoryContractError,
    );
  });

  /** `importance`/`confidence` domain kisitlari — semadaki CHECK ile ayni. */
  it('aralik disi importance/confidence reddeder', () => {
    for (const field of ['importance', 'confidence'] as const) {
      for (const value of [9, -0.1, 1.5, Number.NaN, '0.5']) {
        expect(() => parseMemoryRecord({ ...VALID, [field]: value })).toThrow(
          MemoryContractError,
        );
      }
    }
  });

  /**
   * Zaman damgasi bicimi Stage A siralamasinin dogrulugunu belirliyor;
   * epoch ya da yerel saat sessizce kabul edilmemeli.
   */
  it('UTC olmayan zaman damgalarini reddeder', () => {
    for (const value of [
      '1756108800',
      '2026-08-25 10:00:00',
      '2026-08-25T10:00:00+03:00',
      '2026-08-25',
    ]) {
      expect(() => parseMemoryRecord({ ...VALID, createdAt: value })).toThrow(
        MemoryContractError,
      );
    }
    expect(parseMemoryRecord({ ...VALID, createdAt: '2026-08-25T10:00:00.123Z' })).toBeTruthy();
  });

  it('bozuk metadataJson reddeder', () => {
    expect(() => parseMemoryRecord({ ...VALID, metadataJson: '{ bozuk' })).toThrow(
      MemoryContractError,
    );
  });

  /** Whitelist: `embedding` gibi sozlesme disi bir alan sessizce akmamali. */
  it('beklenmeyen alanlari reddeder', () => {
    expect(() => parseMemoryRecord({ ...VALID, embedding: [1, 2, 3] })).toThrow(
      MemoryContractError,
    );
  });

  it('hata mesaji kayit icerigini sizdirmaz', () => {
    const secretish = 'Kullanicinin banka sifresi 1234';
    try {
      parseMemoryRecord({ ...VALID, content: secretish, importance: 9 });
      expect.unreachable('hata bekleniyordu');
    } catch (error) {
      expect((error as Error).message).not.toContain(secretish);
      expect((error as Error).message).toContain('importance');
    }
  });

  it('nullable alanlarin null olmasina izin verir', () => {
    const parsed = parseMemoryRecord({
      ...VALID,
      summary: null,
      projectId: null,
      sourceSessionId: null,
      lastAccessedAt: null,
      expiresAt: null,
    });
    expect(parsed.projectId).toBeNull();
    expect(parsed.sourceSessionId).toBeNull();
  });

  it('nesne olmayan girdiyi reddeder', () => {
    for (const value of [null, undefined, 'memory', 5, []]) {
      expect(() => parseMemoryRecord(value)).toThrow(MemoryContractError);
    }
  });
});

describe('parseMemoryRecords', () => {
  it('listeyi dogrular', () => {
    expect(parseMemoryRecords([VALID, { ...VALID, id: 2 }])).toHaveLength(2);
  });

  it('dizi olmayan girdiyi reddeder', () => {
    expect(() => parseMemoryRecords(VALID)).toThrow(MemoryContractError);
  });

  it('tek bozuk kayitta tum listeyi reddeder', () => {
    expect(() => parseMemoryRecords([VALID, { ...VALID, kind: 'nope' }])).toThrow(
      MemoryContractError,
    );
  });
});

describe('parseMemoryWriteResult', () => {
  it('yazilan kaydi dogrular', () => {
    const result = parseMemoryWriteResult({ status: 'stored', record: VALID });
    expect(result).toEqual({ status: 'stored', record: VALID });
    expect(wasMemoryStored(result)).toBe(true);
  });

  it('silme sonucunu dogrular', () => {
    expect(parseMemoryWriteResult({ status: 'deleted', id: 3 })).toEqual({
      status: 'deleted',
      id: 3,
    });
  });

  /**
   * `ASUNA_MEMORY_ENABLED=false` iken yazma yapilmaz. Bu sonuc sessizce
   * "basarili" sayilamaz — cagiran taraf "kaydettim" diyemesin.
   */
  it('atlanan yazmayi ayirt eder', () => {
    const result = parseMemoryWriteResult({ status: 'skipped', reason: 'memory-disabled' });
    expect(result).toEqual({ status: 'skipped', reason: 'memory-disabled' });
    expect(wasMemoryStored(result)).toBe(false);
  });

  it('bilinmeyen durum ya da neden reddedilir', () => {
    for (const value of [
      { status: 'ok', record: VALID },
      { status: 'skipped', reason: 'because' },
      { status: 'stored' },
      { status: 'deleted', id: 0 },
      { status: 'stored', record: VALID, extra: 1 },
      'stored',
    ]) {
      expect(() => parseMemoryWriteResult(value)).toThrow(MemoryContractError);
    }
  });
});

import { describe, expect, it } from 'vitest';

import {
  SESSION_END_REASONS,
  SessionContractError,
  isSessionOpen,
  parseSessionRecord,
  parseSessionRecords,
  sessionDurationMs,
} from './session';

const VALID = {
  id: 7,
  startedAt: '2026-08-25T10:00:00Z',
  endedAt: '2026-08-25T10:04:00Z',
  projectId: 'asuna',
  summary: 'Wake word kararini konustuk.',
  transcriptPath: null,
  model: 'gpt-realtime-2.1',
  inputTokens: 120,
  outputTokens: 340,
  totalTokens: 460,
  estimatedCostUsd: 0.0123,
  usageJson: '{"inputTokenDetails":{"audioTokens":100}}',
  createdAt: '2026-08-25T10:00:00Z',
  endReason: 'completed',
};

describe('parseSessionRecord', () => {
  it('gecerli bir kaydi dogrular', () => {
    expect(parseSessionRecord(VALID)).toEqual(VALID);
  });

  /** Ozet uretimi basarisiz olsa da oturum kapanir (Phase 3 plani Adim 4). */
  it('ozetsiz kapanmis oturumu kabul eder', () => {
    const parsed = parseSessionRecord({ ...VALID, summary: null });
    expect(parsed.summary).toBeNull();
    expect(isSessionOpen(parsed)).toBe(false);
  });

  /** `ASUNA_TRANSCRIPT_STORAGE=false` → transcript diske yazilmadi. */
  it('transcript yolu olmayan oturumu kabul eder', () => {
    expect(parseSessionRecord({ ...VALID, transcriptPath: null }).transcriptPath).toBeNull();
  });

  it('acik oturumu tanir', () => {
    const parsed = parseSessionRecord({ ...VALID, endedAt: null, endReason: null });
    expect(isSessionOpen(parsed)).toBe(true);
    expect(parsed.endReason).toBeNull();
  });

  /** ASU-033: kapanis nedeni ayri bir alan; `summary` bayrak degil. */
  it('bilinen kapanis nedenlerini kabul, bilinmeyeni reddeder', () => {
    for (const endReason of SESSION_END_REASONS) {
      expect(parseSessionRecord({ ...VALID, endReason }).endReason).toBe(endReason);
    }
    for (const invalid of ['crashed', 'COMPLETED', '', 0]) {
      expect(() => parseSessionRecord({ ...VALID, endReason: invalid })).toThrow(
        SessionContractError,
      );
    }
  });

  /**
   * Yarim kalan oturum: sure sifir gorunur ama neden **ayri** alanda duruyor —
   * UI "0 saniye surdu" degil "beklenmedik sekilde kapandi" diyebilsin.
   */
  it('yarim kalan oturumu neden alaniyla tasir', () => {
    const parsed = parseSessionRecord({
      ...VALID,
      endedAt: VALID.startedAt,
      summary: null,
      endReason: 'abandoned',
    });
    expect(sessionDurationMs(parsed)).toBe(0);
    expect(parsed.endReason).toBe('abandoned');
    expect(parsed.summary).toBeNull();
  });

  /** Semadaki CHECK ile ayni kural — UI negatif sure gostermemeli. */
  it('baslangictan once biten oturumu reddeder', () => {
    expect(() => parseSessionRecord({ ...VALID, endedAt: '2026-08-25T09:00:00Z' })).toThrow(
      SessionContractError,
    );
  });

  it('negatif token/maliyet degerlerini reddeder', () => {
    for (const field of [
      'inputTokens',
      'outputTokens',
      'totalTokens',
      'estimatedCostUsd',
    ] as const) {
      expect(() => parseSessionRecord({ ...VALID, [field]: -1 })).toThrow(SessionContractError);
    }
  });

  /** Token/maliyet metadatasi API tarafindan gelmeyebilir (memory.md T5). */
  it('token/maliyet metadatasi olmadan da gecerli', () => {
    const parsed = parseSessionRecord({
      ...VALID,
      inputTokens: null,
      outputTokens: null,
      totalTokens: null,
      estimatedCostUsd: null,
      usageJson: null,
    });
    expect(parsed.totalTokens).toBeNull();
    expect(parsed.usageJson).toBeNull();
  });

  it('bozuk usageJson reddeder', () => {
    expect(() => parseSessionRecord({ ...VALID, usageJson: 'not json' })).toThrow(
      SessionContractError,
    );
  });

  it('bos model adini reddeder', () => {
    expect(() => parseSessionRecord({ ...VALID, model: '' })).toThrow(SessionContractError);
  });

  it('beklenmeyen alanlari reddeder', () => {
    expect(() => parseSessionRecord({ ...VALID, rawTranscript: 'merhaba' })).toThrow(
      SessionContractError,
    );
  });

  it('hata mesaji oturum icerigini sizdirmaz', () => {
    const secretish = 'Kullanicinin ozel konusmasi';
    try {
      parseSessionRecord({ ...VALID, summary: secretish, model: '' });
      expect.unreachable('hata bekleniyordu');
    } catch (error) {
      expect((error as Error).message).not.toContain(secretish);
    }
  });
});

describe('parseSessionRecords', () => {
  it('listeyi dogrular', () => {
    expect(parseSessionRecords([VALID, { ...VALID, id: 8 }])).toHaveLength(2);
  });

  it('dizi olmayan girdiyi reddeder', () => {
    expect(() => parseSessionRecords(VALID)).toThrow(SessionContractError);
  });
});

import { describe, expect, it } from 'vitest';

import { MEMORY_DELETE_ALL_CONFIRMATION } from './memory';
import {
  SESSION_CLEAR_ALL_CONFIRMATION,
  SESSION_END_REASONS,
  SessionContractError,
  TRANSCRIPT_FILE_OUTCOMES,
  isSessionOpen,
  parseSessionDeleteResult,
  parseSessionListItem,
  parseSessionPage,
  parseSessionPurgeResult,
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

// ---------------------------------------------------------------------------
// Oturum gecmisi: listeleme + silme (ASU-065)
// ---------------------------------------------------------------------------

const LIST_ITEM = {
  id: 7,
  startedAt: '2026-08-25T10:00:00Z',
  endedAt: '2026-08-25T10:04:00Z',
  endReason: 'completed',
  summaryPreview: 'Wake word kararini konustuk.',
  summaryTruncated: false,
  hasTranscriptFile: true,
};

describe('parseSessionListItem', () => {
  it('gecerli bir satiri dogrular', () => {
    expect(parseSessionListItem(LIST_ITEM)).toEqual(LIST_ITEM);
  });

  it('ozetsiz ve acik oturumu kabul eder', () => {
    const parsed = parseSessionListItem({
      ...LIST_ITEM,
      endedAt: null,
      endReason: null,
      summaryPreview: null,
    });
    expect(parsed.summaryPreview).toBeNull();
    expect(parsed.endedAt).toBeNull();
    expect(parsed.endReason).toBeNull();
  });

  /**
   * **GUVENLIK/GIZLILIK (ASU-065)**: dokum dosya yolu sozlesmede yok. Backend
   * bir gun yanlislikla dondurse bu sessizce renderer'a akmaz.
   */
  it('dosya yolu iceren bir satiri reddeder', () => {
    expect(() =>
      parseSessionListItem({ ...LIST_ITEM, transcriptPath: '/Users/x/transcripts/s.jsonl' }),
    ).toThrow(SessionContractError);
  });

  it('eksik bayraklari uydurmaz', () => {
    expect(() => parseSessionListItem({ ...LIST_ITEM, hasTranscriptFile: 'evet' })).toThrow(
      SessionContractError,
    );
    expect(() => parseSessionListItem({ ...LIST_ITEM, summaryTruncated: 1 })).toThrow(
      SessionContractError,
    );
  });
});

describe('parseSessionPage', () => {
  it('sayfayi sinirlariyla birlikte dogrular', () => {
    const page = parseSessionPage({
      sessions: [LIST_ITEM],
      limit: 50,
      limitMax: 200,
      total: 214,
    });
    expect(page.sessions).toHaveLength(1);
    expect(page.limit).toBe(50);
    expect(page.limitMax).toBe(200);
    expect(page.total).toBe(214);
  });

  it('bos sayfayi kabul eder', () => {
    expect(parseSessionPage({ sessions: [], limit: 50, limitMax: 200, total: 0 }).total).toBe(
      0,
    );
  });

  it('sinirlari eksik bir sayfayi reddeder', () => {
    expect(() => parseSessionPage({ sessions: [], limit: 50, total: 0 })).toThrow(
      SessionContractError,
    );
    expect(() =>
      parseSessionPage({ sessions: LIST_ITEM, limit: 1, limitMax: 2, total: 1 }),
    ).toThrow(SessionContractError);
  });
});

describe('parseSessionDeleteResult', () => {
  it('dosya sonucunu tasir', () => {
    for (const transcriptFile of TRANSCRIPT_FILE_OUTCOMES) {
      expect(parseSessionDeleteResult({ status: 'deleted', id: 7, transcriptFile })).toEqual({
        status: 'deleted',
        id: 7,
        transcriptFile,
      });
    }
  });

  it('bilinmeyen dosya sonucunu reddeder', () => {
    expect(() =>
      parseSessionDeleteResult({ status: 'deleted', id: 7, transcriptFile: 'belki' }),
    ).toThrow(SessionContractError);
  });

  it('atlanan islemi hata saymaz', () => {
    expect(parseSessionDeleteResult({ status: 'skipped', reason: 'memory-disabled' })).toEqual({
      status: 'skipped',
      reason: 'memory-disabled',
    });
  });

  it('bilinmeyen durumu reddeder', () => {
    expect(() => parseSessionDeleteResult({ status: 'purged', deleted: 1 })).toThrow(
      SessionContractError,
    );
  });
});

describe('parseSessionPurgeResult', () => {
  it('olculen sayilari tasir', () => {
    expect(
      parseSessionPurgeResult({
        status: 'purged',
        deletedSessions: 4,
        deletedFiles: 2,
        remainingFiles: 1,
      }),
    ).toEqual({ status: 'purged', deletedSessions: 4, deletedFiles: 2, remainingFiles: 1 });
  });

  it('sifir gecerli bir sonuctur', () => {
    const result = parseSessionPurgeResult({
      status: 'purged',
      deletedSessions: 0,
      deletedFiles: 0,
      remainingFiles: 0,
    });
    expect(result.status).toBe('purged');
  });

  it('eksik sayimi reddeder', () => {
    expect(() =>
      parseSessionPurgeResult({ status: 'purged', deletedSessions: 1, deletedFiles: 1 }),
    ).toThrow(SessionContractError);
  });
});

describe('onay ifadeleri', () => {
  /**
   * Iki toplu silme aksiyonunun ifadesi **ayni olmamali**: kapsamlari farkli ve
   * birini yazip digerini calistirmak mumkun olmamali. Rust aynasi:
   * `session_repository::CLEAR_ALL_CONFIRMATION`.
   */
  it('oturum temizligi hafiza silmeden farkli bir ifade ister', () => {
    expect(SESSION_CLEAR_ALL_CONFIRMATION).toBe('KONUSMA GECMISINI SIL');
    expect(SESSION_CLEAR_ALL_CONFIRMATION).not.toBe(MEMORY_DELETE_ALL_CONFIRMATION);
    // Turkce karakter yok: klavye duzeninden bagimsiz yazilabilmeli.
    expect(SESSION_CLEAR_ALL_CONFIRMATION).toMatch(/^[A-Z ]+$/);
  });
});

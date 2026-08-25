import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SessionContractError, type SessionRecord } from '../../shared/session';
import { AsunaStoreError } from '../../shared/store-error';
import {
  SESSION_COMMANDS,
  SESSION_READ_COMMANDS,
  SessionRecorder,
  clearSessionHistory,
  deleteSession,
  describeSessionOutcome,
  finalizeSessionRecord,
  listSessions,
  startSessionRecord,
} from './session-service';

const invokeMock = vi.hoisted(() =>
  vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(),
);

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const OPEN_SESSION: SessionRecord = {
  id: 12,
  startedAt: '2026-08-25T10:00:00Z',
  endedAt: null,
  projectId: null,
  summary: null,
  transcriptPath: null,
  model: 'gpt-realtime-2.1',
  inputTokens: null,
  outputTokens: null,
  totalTokens: null,
  estimatedCostUsd: null,
  usageJson: null,
  createdAt: '2026-08-25T10:00:00Z',
  endReason: null,
};

const CLOSED_SESSION: SessionRecord = {
  ...OPEN_SESSION,
  endedAt: '2026-08-25T10:04:00Z',
  inputTokens: 120,
  outputTokens: 80,
  totalTokens: 200,
  usageJson: '{"requests":2}',
  endReason: 'completed',
};

describe('session-service komutlari', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('ACL"de kayitli adlarla birebir ayni', () => {
    expect(SESSION_COMMANDS.start).toBe('session_start');
    expect(SESSION_COMMANDS.finalize).toBe('session_finalize');
  });

  /** Model ve transcript yolu renderer'dan gitmez: sozlesmede yoklar. */
  it('startSessionRecord yalnizca projeyi gonderir', async () => {
    invokeMock.mockResolvedValue({ status: 'recorded', session: OPEN_SESSION });

    await startSessionRecord();

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_start', { projectId: null });
    const args = JSON.stringify(invokeMock.mock.calls[0]?.[1]);
    expect(args).not.toContain('model');
    expect(args).not.toContain('transcriptPath');
  });

  it('finalizeSessionRecord kullanim ve dokumu gonderir', async () => {
    invokeMock.mockResolvedValue({ status: 'recorded', session: CLOSED_SESSION });

    await finalizeSessionRecord(12, {
      usage: { inputTokens: 120, outputTokens: 80, totalTokens: 200 },
      transcript: [{ role: 'user', text: 'merhaba' }],
    });

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_finalize', {
      sessionId: 12,
      input: {
        usage: { inputTokens: 120, outputTokens: 80, totalTokens: 200 },
        transcript: [{ role: 'user', text: 'merhaba' }],
      },
    });
  });

  it('sozlesmeye uymayan yaniti reddeder', async () => {
    invokeMock.mockResolvedValue({ status: 'recorded', session: { id: 1 } });
    await expect(startSessionRecord()).rejects.toBeInstanceOf(SessionContractError);
  });

  it('IPC hatasini tipli hataya cevirir', async () => {
    invokeMock.mockRejectedValue({ code: 'unavailable', message: 'hafiza kullanilamiyor' });

    const error = await startSessionRecord().catch((value: unknown) => value);
    expect(error).toBeInstanceOf(AsunaStoreError);
    expect((error as AsunaStoreError).code).toBe('unavailable');
  });
});

describe('SessionRecorder', () => {
  it('oturum acilisinda kayit acar, kapanista kimlikle kapatir', async () => {
    const start = vi.fn().mockResolvedValue({ status: 'recorded', session: OPEN_SESSION });
    const finalize = vi.fn().mockResolvedValue({ status: 'recorded', session: CLOSED_SESSION });
    const recorder = new SessionRecorder({ start, finalize });

    recorder.begin(1_000);
    const outcome = await recorder.end(241_000, {
      usage: { totalTokens: 200 },
      transcript: [{ role: 'user', text: 'merhaba' }],
    });

    expect(finalize).toHaveBeenCalledExactlyOnceWith(12, {
      usage: { totalTokens: 200 },
      transcript: [{ role: 'user', text: 'merhaba' }],
    });
    expect(outcome).toEqual({
      id: 12,
      durationMs: 240_000,
      totalTokens: 200,
      estimatedCostUsd: null,
    });
  });

  /**
   * `session_start` yanit vermeden oturum kapanabilir (kisa oturum). Kimlik
   * beklenmezse kayit sonsuza kadar acik kalir ve bir sonraki acilista
   * "yarim kalmis oturum" olarak kurtarilirdi.
   */
  it('kapanis, ucusta olan acilis cagrisini bekler', async () => {
    let resolveStart: (value: unknown) => void = () => undefined;
    const start = vi.fn().mockReturnValue(
      new Promise((resolve) => {
        resolveStart = resolve;
      }),
    );
    const finalize = vi.fn().mockResolvedValue({ status: 'recorded', session: CLOSED_SESSION });
    const recorder = new SessionRecorder({ start, finalize });

    recorder.begin(0);
    const closing = recorder.end(1_000, {});

    expect(finalize).not.toHaveBeenCalled();

    resolveStart({ status: 'recorded', session: OPEN_SESSION });
    await closing;

    expect(finalize).toHaveBeenCalledExactlyOnceWith(12, {});
  });

  /**
   * ASU-032 kabul kriteri: hafiza kapaliyken oturum kaydi olusmaz. Kapanista
   * uydurulmus bir kimlikle yazma denenmez.
   */
  it('hafiza kapaliyken kapanista yazma denemez', async () => {
    const start = vi.fn().mockResolvedValue({ status: 'skipped', reason: 'memory-disabled' });
    const finalize = vi.fn();
    const recorder = new SessionRecorder({ start, finalize });

    recorder.begin(0);
    await expect(recorder.end(1_000, {})).resolves.toBeNull();

    expect(finalize).not.toHaveBeenCalled();
  });

  /**
   * ASU-037 / Gate 3: kullanici hafizayi **oturum sirasinda** kapatirsa kapanis
   * da `skipped` doner. Bu bir hata degil (`onError` cagrilmaz) ama sessiz de
   * degil — nedeni log'a tasiyan `onSkipped` cagrilir.
   */
  it('atlanan kaydi hata saymaz, ama log icin raporlar', async () => {
    const onError = vi.fn();
    const onSkipped = vi.fn();
    const start = vi.fn().mockResolvedValue({ status: 'recorded', session: OPEN_SESSION });
    const finalize = vi
      .fn()
      .mockResolvedValue({ status: 'skipped', reason: 'memory-disabled' });
    const recorder = new SessionRecorder({ start, finalize, onError, onSkipped });

    recorder.begin(0);
    await expect(recorder.end(1_000, {})).resolves.toBeNull();

    expect(onError).not.toHaveBeenCalled();
    expect(onSkipped).toHaveBeenCalledExactlyOnceWith('finalize', 'memory-disabled');
  });

  it('acilis atlandiginda da raporlar', async () => {
    const onSkipped = vi.fn();
    const start = vi.fn().mockResolvedValue({ status: 'skipped', reason: 'memory-disabled' });
    const recorder = new SessionRecorder({ start, finalize: vi.fn(), onSkipped });

    recorder.begin(0);
    await recorder.end(1_000, {});

    expect(onSkipped).toHaveBeenCalledExactlyOnceWith('start', 'memory-disabled');
  });

  /** Kayit hatasi sesli oturumu dusurmez ama sessizce yutulmaz. */
  it('acilis hatasini raporlar ve kapanisi patlatmaz', async () => {
    const onError = vi.fn();
    const start = vi.fn().mockRejectedValue(new AsunaStoreError('storage', 'disk dolu'));
    const finalize = vi.fn();
    const recorder = new SessionRecorder({ start, finalize, onError });

    recorder.begin(0);
    await expect(recorder.end(1_000, {})).resolves.toBeNull();

    expect(onError).toHaveBeenCalledOnce();
    expect(finalize).not.toHaveBeenCalled();
  });

  it('kapanis hatasini raporlar', async () => {
    const onError = vi.fn();
    const start = vi.fn().mockResolvedValue({ status: 'recorded', session: OPEN_SESSION });
    const finalize = vi.fn().mockRejectedValue(new AsunaStoreError('storage', 'kilit'));
    const recorder = new SessionRecorder({ start, finalize, onError });

    recorder.begin(0);
    await expect(recorder.end(1_000, {})).resolves.toBeNull();

    expect(onError).toHaveBeenCalledOnce();
  });

  it('acilmamis oturum icin kapanis islemi yapmaz', async () => {
    const finalize = vi.fn();
    const recorder = new SessionRecorder({ start: vi.fn(), finalize });

    await expect(recorder.end(1_000, {})).resolves.toBeNull();
    expect(finalize).not.toHaveBeenCalled();
  });

  it('cift begin ikinci bir kayit acmaz', () => {
    const start = vi.fn().mockResolvedValue({ status: 'recorded', session: OPEN_SESSION });
    const recorder = new SessionRecorder({ start });

    recorder.begin(0);
    recorder.begin(0);

    expect(start).toHaveBeenCalledOnce();
  });
});

describe('describeSessionOutcome', () => {
  it('sure ve token kullanimini tek satirda ozetler', () => {
    expect(
      describeSessionOutcome({
        id: 1,
        durationMs: 192_000,
        totalTokens: 1_240,
        estimatedCostUsd: null,
      }),
    ).toBe('3 dk 12 sn · 1.240 token · maliyet: bilinmiyor');
  });

  it('bir dakikadan kisa oturumu saniye ile gosterir', () => {
    expect(
      describeSessionOutcome({
        id: 1,
        durationMs: 4_400,
        totalTokens: null,
        estimatedCostUsd: null,
      }),
    ).toBe('4 sn · maliyet: bilinmiyor');
  });

  /**
   * Maliyet bilinmiyorsa sifir ya da tahmin gosterilmez: dogrulanmis bir fiyat
   * tablosu olmadan sayi uretmek "uydurulmus maliyet" olur (ASU-033).
   */
  it('bilinmeyen maliyeti sifir gibi gostermez', () => {
    const text = describeSessionOutcome({
      id: 1,
      durationMs: 1_000,
      totalTokens: 10,
      estimatedCostUsd: null,
    });
    expect(text).toContain('bilinmiyor');
    expect(text).not.toContain('$0.0000');
  });

  it('maliyet bilindiginde gosterir', () => {
    expect(
      describeSessionOutcome({
        id: 1,
        durationMs: 60_000,
        totalTokens: 100,
        estimatedCostUsd: 0.0123,
      }),
    ).toContain('~$0.0123');
  });

  it('kapanmis oturum yoksa tire gosterir', () => {
    expect(describeSessionOutcome(null)).toBe('—');
  });
});

/**
 * ASU-065 — oturum gecmisi: listeleme + silme.
 *
 * Kanitlanan seyler: komut adlari ACL ile birebir, istek yuku dar (renderer yol
 * ya da siralama gonderemez), yanit sema dogrulamasindan geciyor ve onay
 * ifadesi servis tarafindan **uydurulmuyor**.
 */
describe('session-service — oturum gecmisi (ASU-065)', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  const LIST_ITEM = {
    id: 7,
    startedAt: '2026-08-25T10:00:00Z',
    endedAt: '2026-08-25T10:04:00Z',
    endReason: 'completed',
    summaryPreview: 'Wake word kararini konustuk.',
    summaryTruncated: false,
    hasTranscriptFile: false,
  };

  const PAGE = { sessions: [LIST_ITEM], limit: 50, limitMax: 200, total: 1 };

  it('ACL"de kayitli adlarla birebir ayni', () => {
    expect(SESSION_READ_COMMANDS.list).toBe('session_list');
    expect(SESSION_COMMANDS.delete).toBe('session_delete');
    expect(SESSION_COMMANDS.clearAll).toBe('session_clear_all');
  });

  it('listSessions limiti gonderir ve yaniti dogrular', async () => {
    invokeMock.mockResolvedValue(PAGE);

    const page = await listSessions(10);

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_list', {
      query: { limit: 10 },
    });
    expect(page.total).toBe(1);
    expect(page.sessions[0]?.summaryPreview).toBe('Wake word kararini konustuk.');
  });

  it('limit verilmezse sunucu varsayilanina birakir', async () => {
    invokeMock.mockResolvedValue(PAGE);

    await listSessions();

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_list', { query: null });
  });

  it('bozuk liste yanitini yutmaz', async () => {
    invokeMock.mockResolvedValue({ sessions: [LIST_ITEM], limit: 50, total: 1 });

    await expect(listSessions()).rejects.toBeInstanceOf(SessionContractError);
  });

  it('deleteSession yalnizca kimlik gonderir ve dosya sonucunu dondurur', async () => {
    invokeMock.mockResolvedValue({ status: 'deleted', id: 7, transcriptFile: 'deleted' });

    const result = await deleteSession(7);

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_delete', { sessionId: 7 });
    // Renderer yol vermez, yol geri de gelmez.
    expect(JSON.stringify(invokeMock.mock.calls[0]?.[1])).not.toContain('transcriptPath');
    expect(result).toEqual({ status: 'deleted', id: 7, transcriptFile: 'deleted' });
  });

  it('clearSessionHistory ifadeyi uydurmaz, cagirandan alir', async () => {
    invokeMock.mockResolvedValue({
      status: 'purged',
      deletedSessions: 2,
      deletedFiles: 1,
      remainingFiles: 0,
    });

    const result = await clearSessionHistory('KONUSMA GECMISINI SIL');

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_clear_all', {
      confirmationPhrase: 'KONUSMA GECMISINI SIL',
    });
    expect(result).toEqual({
      status: 'purged',
      deletedSessions: 2,
      deletedFiles: 1,
      remainingFiles: 0,
    });
  });

  it('yanlis ifadenin tipli hatasini cagirana tasir', async () => {
    invokeMock.mockRejectedValue({
      code: 'invalid',
      message: '`confirmationPhrase` birebir `KONUSMA GECMISINI SIL` olmali',
    });

    await expect(clearSessionHistory('yanlis')).rejects.toBeInstanceOf(AsunaStoreError);
  });
});

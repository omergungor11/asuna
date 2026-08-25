import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AsunaStoreError } from '../../shared/store-error';
import { ToolEventContractError, type ToolEventRecord } from '../../shared/tool-event';
import { logBuffer } from '../observability';
import {
  TOOL_AUDIT_COMMANDS,
  TOOL_AUDIT_LOG_SCOPE,
  listToolEvents,
  recordToolEvent,
} from './audit';

const invokeMock = vi.hoisted(() =>
  vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(),
);

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const EVENT: ToolEventRecord = {
  id: 1,
  sessionId: 3,
  toolName: 'open_project',
  riskLevel: 1,
  argumentsRedacted: 'projectId=asuna',
  approvalState: 'approved',
  resultSummary: 'Proje VS Code ile acildi.',
  createdAt: '2026-08-25T10:01:00Z',
};

describe('audit komut adlari', () => {
  it('ACL"de kayitli adlarla birebir ayni', () => {
    expect(TOOL_AUDIT_COMMANDS.record).toBe('record_tool_event');
    expect(TOOL_AUDIT_COMMANDS.list).toBe('tool_event_list');
  });

  /**
   * **ASU-050 kabul kriteri**: audit kayitlari uygulamadan silinemiyor.
   * Renderer'in elinde boyle bir komut adi bile yok.
   */
  it('silme ya da guncelleme komutu tanimlamaz', () => {
    expect(Object.keys(TOOL_AUDIT_COMMANDS).sort()).toEqual(['list', 'record']);
    for (const name of Object.values(TOOL_AUDIT_COMMANDS)) {
      expect(name).not.toMatch(/delete|update|clear|purge/);
    }
  });
});

describe('recordToolEvent', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('ham argumanlari host"a gonderir ve yazilan kaydi dondurur', async () => {
    invokeMock.mockResolvedValueOnce({ status: 'recorded', event: { ...EVENT } });

    const outcome = await recordToolEvent({
      sessionId: 3,
      toolName: 'open_project',
      riskLevel: 1,
      arguments: { projectId: 'asuna' },
      approvalState: 'approved',
      resultSummary: 'Proje VS Code ile acildi.',
    });

    expect(outcome).toEqual({ status: 'recorded', event: EVENT });
    expect(invokeMock).toHaveBeenCalledWith('record_tool_event', {
      input: {
        sessionId: 3,
        toolName: 'open_project',
        riskLevel: 1,
        arguments: { projectId: 'asuna' },
        approvalState: 'approved',
        resultSummary: 'Proje VS Code ile acildi.',
      },
    });
  });

  /**
   * Redaksiyon **host tarafinda**: renderer hazir bir ozet gondermez ve
   * gonderemez (Rust sozlesmesinde `argumentsRedacted` alani yok).
   */
  it('hazir bir arguman ozeti gondermez', async () => {
    invokeMock.mockResolvedValueOnce({ status: 'recorded', event: { ...EVENT } });

    await recordToolEvent({
      toolName: 'read_project_file',
      riskLevel: 0,
      arguments: { apiKey: 'sk-proj-SIZMAMALI' },
      approvalState: 'not_required',
    });

    const [, args] = invokeMock.mock.calls[0] ?? [];
    expect(args?.['input']).not.toHaveProperty('argumentsRedacted');
  });

  /** Reddedilen ve zaman asimina ugrayan cagrilar da bildirilir. */
  it('reddedilen ve zaman asimina ugrayan cagrilari da yazar', async () => {
    for (const approvalState of ['denied', 'timeout', 'not_requested'] as const) {
      invokeMock.mockResolvedValueOnce({
        status: 'recorded',
        event: { ...EVENT, approvalState, resultSummary: null },
      });

      const outcome = await recordToolEvent({
        toolName: 'open_project',
        riskLevel: 1,
        approvalState,
      });

      expect(outcome.status).toBe('recorded');
      expect(outcome.status === 'recorded' && outcome.event.approvalState).toBe(approvalState);
    }
  });

  it('kapali hafizada "kaydettim" demez', async () => {
    invokeMock.mockResolvedValueOnce({ status: 'skipped', reason: 'memory-disabled' });

    const outcome = await recordToolEvent({
      toolName: 'get_current_project',
      riskLevel: 0,
      approvalState: 'not_required',
    });

    expect(outcome).toEqual({ status: 'skipped', reason: 'memory-disabled' });
  });

  /**
   * **ASU-050 kabul kriteri**: "audit yazimi basarisiz olursa bu durum gorunur
   * oluyor (sessiz kayip yok)".
   *
   * Iki sey birden olculuyor: hata **firlatmaz** (tool sonucunu bozmaz) ve
   * **yutulmaz** (hem sonucta hem log'da gorunur).
   */
  it('yazma basarisiz olunca firlatmaz ama hatayi gorunur kilar', async () => {
    invokeMock.mockRejectedValueOnce({
      code: 'storage',
      message: 'veritabani islemi basarisiz',
    });
    logBuffer.clear();

    const outcome = await recordToolEvent({
      toolName: 'open_project',
      riskLevel: 1,
      approvalState: 'approved',
      resultSummary: 'Proje acildi.',
    });

    // 1) Firlatmadi: tool sonucu bu hatadan etkilenmez.
    expect(outcome.status).toBe('failed');
    expect(outcome.status === 'failed' && outcome.error).toBeInstanceOf(AsunaStoreError);
    expect(outcome.status === 'failed' && outcome.error.code).toBe('storage');

    // 2) Yutmadi: ayni hata `error` seviyesinde deftere de dustu. Log'a yazip
    // donmemek de bir tur sessiz kayip olurdu — cagiran taraf durumu
    // kullaniciya gosteremezdi.
    const entry = logBuffer
      .getSnapshot()
      .find((line) => line.scope === TOOL_AUDIT_LOG_SCOPE && line.level === 'error');
    expect(entry, 'audit hatasi log"a dusmedi').toBeDefined();
    expect(entry?.data).toMatchObject({ toolName: 'open_project', code: 'storage' });
  });

  it('ACL reddi de yutulmaz', async () => {
    invokeMock.mockRejectedValueOnce(
      'record_tool_event not allowed on window "main", ... permission: allow-record-tool-event',
    );

    const outcome = await recordToolEvent({
      toolName: 'open_project',
      riskLevel: 1,
      approvalState: 'approved',
    });

    expect(outcome.status).toBe('failed');
    expect(outcome.status === 'failed' && outcome.error.code).toBe('unknown');
    expect(outcome.status === 'failed' && outcome.error.message).toContain('not allowed');
  });

  /** Bozuk bir yanit sessizce kabul edilmez: sozlesme dogrulanir. */
  it('sozlesmeye uymayan yaniti hataya cevirir', async () => {
    invokeMock.mockResolvedValueOnce({
      status: 'recorded',
      event: { ...EVENT, approvalState: 'onaylandi' },
    });

    const outcome = await recordToolEvent({
      toolName: 'open_project',
      riskLevel: 1,
      approvalState: 'approved',
    });

    // Parse hatasi da bir hata: yazma "basarili" sayilmaz.
    expect(outcome.status).toBe('failed');
  });

  it('log kapsami tools.audit', () => {
    expect(TOOL_AUDIT_LOG_SCOPE).toBe('tools.audit');
  });
});

describe('listToolEvents', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('filtresiz istekte null gonderir ve sayfayi dogrular', async () => {
    invokeMock.mockResolvedValueOnce({
      events: [{ ...EVENT }],
      limit: 50,
      limitMax: 200,
      total: 1,
    });

    const page = await listToolEvents();

    expect(page.events).toEqual([EVENT]);
    expect(invokeMock).toHaveBeenCalledWith('tool_event_list', { query: null });
  });

  it('oturum filtresini oldugu gibi iletir', async () => {
    invokeMock.mockResolvedValueOnce({ events: [], limit: 10, limitMax: 200, total: 0 });

    await listToolEvents({ sessionId: 3, limit: 10 });

    expect(invokeMock).toHaveBeenCalledWith('tool_event_list', {
      query: { sessionId: 3, limit: 10 },
    });
  });

  /** "Audit'e bakamadim" ile "audit bos" ayni cevap degil. */
  it('ariza durumunda tipli hata firlatir', async () => {
    invokeMock.mockRejectedValueOnce({
      code: 'unavailable',
      message: "hafiza kullanilamiyor: sema migration'lari uygulanamadi",
    });

    await expect(listToolEvents()).rejects.toBeInstanceOf(AsunaStoreError);
  });

  it('bozuk sayfayi sessizce kabul etmez', async () => {
    invokeMock.mockResolvedValueOnce({
      events: [{ ...EVENT, riskLevel: 9 }],
      limit: 50,
      limitMax: 200,
      total: 1,
    });

    await expect(listToolEvents()).rejects.toBeInstanceOf(ToolEventContractError);
  });
});

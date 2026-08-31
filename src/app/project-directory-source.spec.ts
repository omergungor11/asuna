/**
 * `listCurrentProjectDirectory` testleri (plan-chat-shell.md WP4 — bosluk analizi).
 *
 * Bu modul, dosya secicinin sandbox'a bakan tek koprusu. Kanitlanan seyler:
 *
 * 1. Komut adi ve tek argumani sabit: `list_project_dir` + kok'e gore `path`.
 *    Komut **proje secmez** (registry'deki guncel proje) — servis de secmeye
 *    calismaz, ikinci bir arguman gondermez.
 * 2. Sandbox reddi **korunur**: host'un mesaji (ornegin "proje kokunun disi")
 *    kullaniciya aynen tasinir, "bir seyler ters gitti" ile degistirilmez.
 * 3. Cozulemeyen ret ya da bicimi bozuk basarili yanit sessizce bos listeye
 *    donusmez — durust bir hata firlatilir.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ProjectDirectoryView } from '../asuna/tools/list-project-files';

import { listCurrentProjectDirectory } from './project-directory-source';

const invokeMock = vi.hoisted(() =>
  vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(),
);

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const VIEW = {
  projectId: 'asuna',
  projectName: 'Asuna',
  path: 'src',
  entries: [{ name: 'app.tsx', kind: 'file', sizeBytes: 2048, blocked: false }],
  totalEntries: 1,
  returnedEntries: 1,
  truncated: false,
  scanCapped: false,
  maxEntries: 200,
};

beforeEach(() => {
  invokeMock.mockReset();
});

describe('listCurrentProjectDirectory', () => {
  it('yalnizca kok"e gore yolu gonderir; proje secmez', async () => {
    invokeMock.mockResolvedValue(VIEW);

    const view: ProjectDirectoryView = await listCurrentProjectDirectory('src');

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('list_project_dir', { path: 'src' });
    expect(view.entries).toHaveLength(1);
    expect(view.projectName).toBe('Asuna');
  });

  it('kok icin bos metin gonderir', async () => {
    invokeMock.mockResolvedValue({ ...VIEW, path: '' });

    await listCurrentProjectDirectory('');

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('list_project_dir', { path: '' });
  });

  /** Sandbox reddinin **nedeni** kullaniciya ulasmali; genel bir metne dusmemeli. */
  it('sandbox reddinin mesajini korur', async () => {
    invokeMock.mockRejectedValue({
      code: 'denied',
      message: 'yol proje kökünün dışına çıkıyor',
      auditSummary: 'escape attempt',
      escapeAttempt: true,
    });

    await expect(listCurrentProjectDirectory('../..')).rejects.toThrow(
      'Klasör listelenemedi: yol proje kökünün dışına çıkıyor',
    );
  });

  it('cozulemeyen reddi uydurmaz ama yutmaz da', async () => {
    invokeMock.mockRejectedValue({ beklenmeyen: true });

    await expect(listCurrentProjectDirectory('src')).rejects.toThrow(
      'Klasör listelenemedi ve nedeni çözülemedi.',
    );
  });

  it('bicimi bozuk basarili yaniti bos liste saymaz', async () => {
    invokeMock.mockResolvedValue({ entries: 'yok' });

    await expect(listCurrentProjectDirectory('src')).rejects.toThrow(
      'Klasör listelendi ama yanıt beklenen biçimde değil.',
    );
  });

  /** Orijinal ret nesnesi `cause` olarak korunur: log/teshis kaybolmaz. */
  it('orijinal hatayi cause olarak tasir', async () => {
    const refusal = {
      code: 'denied',
      message: 'blok listesi',
      auditSummary: 'blocked',
      escapeAttempt: false,
    };
    invokeMock.mockRejectedValue(refusal);

    const error = await listCurrentProjectDirectory('.env').catch((value: unknown) => value);

    expect(error).toBeInstanceOf(Error);
    expect((error as Error).cause).toBe(refusal);
  });
});

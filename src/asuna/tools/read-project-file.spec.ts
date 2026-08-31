/**
 * `read_project_file` tool testleri (ASU-051).
 *
 * Kanitlanan seyler:
 * 1. Tanim sozlesmesi: risk 0, onaysiz, tek alanli **strict** sema.
 * 2. Sema uydurma parametreyi reddediyor — model kok/proje secemez.
 * 3. Basarili okumada modele icerik, **deftere yalnizca ozet** gidiyor.
 * 4. Kirpma ve maskeleme sessiz degil: model ciktisinda yaziyor.
 * 5. Uc ret ayri: kacis denemesi / hassas dosya / dosya yok — ve hicbirinde
 *    "icerik uydur" yolu acilmiyor.
 */

import { describe, expect, it, vi } from 'vitest';

import { createReadProjectFileTool, READ_PROJECT_FILE_TOOL_NAME } from './read-project-file';
import { executeTool } from './registry';
import type { ToolContext } from './types';
import type { ToolAuditInput } from '../../shared/tool-event';

const CONTEXT: ToolContext = { sessionId: null, projectRoot: null };

const VIEW = {
  projectId: 'asuna',
  projectName: 'Asuna',
  path: 'README.md',
  content: '# Asuna\nSesli companion.\n',
  truncated: false,
  redacted: false,
  sizeBytes: 26,
  returnedChars: 26,
  maxChars: 6000,
};

function toolWith(
  readFile: (path: string) => Promise<unknown>,
): ReturnType<typeof createReadProjectFileTool> {
  return createReadProjectFileTool({ readFile });
}

/**
 * Tauri `invoke` hatayi bir `Error` degil, komutun serilestirdigi **duz nesne**
 * olarak reddeder (`{ code, message, escapeAttempt, auditSummary }`). Testin
 * olctugu davranis tam olarak bu, bu yuzden cast bilincli.
 */
function rejectWith(error: unknown): () => Promise<unknown> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- bkz. yukari
  return (): Promise<unknown> => Promise.reject(error);
}

describe('read_project_file — tanim', () => {
  it('risk 0, onaysiz ve konusulabilir bir aciklamasi var', () => {
    const tool = toolWith(() => Promise.resolve(VIEW));

    expect(tool.name).toBe(READ_PROJECT_FILE_TOOL_NAME);
    expect(tool.risk).toBe(0);
    expect(tool.requiresApproval).toBe(false);
    expect(tool.timeoutMs).toBeGreaterThan(0);
    // Model neyin YAPILMADIGINI da bilmeli.
    expect(tool.description).toContain('UYDURMA');
  });

  /**
   * Sema **tek alanli ve strict**: modelin `projectId` gibi bir parametre
   * uydurup kayitli kokler arasinda dolasmasi mumkun olmamali. Yanlis
   * parametre sessizce atilmaz, cagri hic calismaz.
   */
  it('uydurma parametreyi reddediyor ve komutu cagirmiyor', async () => {
    const readFile = vi.fn(() => Promise.resolve(VIEW));

    const result = await executeTool(
      toolWith(readFile),
      { path: 'README.md', projectId: 'baska-proje' },
      CONTEXT,
    );

    expect(readFile).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });

  it('bos yolu reddediyor', async () => {
    const readFile = vi.fn(() => Promise.resolve(VIEW));

    const result = await executeTool(toolWith(readFile), { path: '' }, CONTEXT);

    expect(readFile).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });
});

describe('read_project_file — basarili okuma', () => {
  it('icerigi modele veriyor, deftere yalnizca ozet yaziyor', async () => {
    const audits: ToolAuditInput[] = [];

    const result = await executeTool(
      toolWith(() => Promise.resolve(VIEW)),
      { path: 'README.md' },
      CONTEXT,
      { onAudit: (input): void => void audits.push(input) },
    );

    expect(result.ok).toBe(true);
    expect(result.summary).toContain('Sesli companion.');

    // **ASU-051 gizlilik kilidi**: dosya icerigi audit satirina girmez.
    expect(audits).toHaveLength(1);
    expect(audits[0]?.resultSummary).toBe('README.md okundu (26 B)');
    expect(audits[0]?.resultSummary).not.toContain('Sesli companion');
    expect(audits[0]?.outcome).toBe('succeeded');
    expect(audits[0]?.approvalState).toBe('not_required');
  });

  /** Kirpma **sessiz degil**: model "tamamini okudum" diyemez. */
  it('kirpildiginda bunu hem modele hem deftere yaziyor', async () => {
    const audits: ToolAuditInput[] = [];

    const result = await executeTool(
      toolWith(() =>
        Promise.resolve({
          ...VIEW,
          path: 'notes.md',
          content: 'x'.repeat(6000),
          truncated: true,
          sizeBytes: 40_000,
          returnedChars: 6000,
        }),
      ),
      { path: 'notes.md' },
      CONTEXT,
      { onAudit: (input): void => void audits.push(input) },
    );

    expect(result.summary).toContain('DIKKAT');
    expect(result.summary).toContain('Tamamini gormedin');
    expect(audits[0]?.resultSummary).toContain('kirpildi');
    expect(audits[0]?.resultSummary).toContain('39.1 KB');
  });

  it('maskeleme yapildiysa modele soyluyor', async () => {
    const result = await executeTool(
      toolWith(() => Promise.resolve({ ...VIEW, redacted: true })),
      { path: 'README.md' },
      CONTEXT,
    );

    expect(result.summary).toContain('maskelendi');
  });

  /** Mutlak yol hicbir katmanda gorunmemeli. */
  it('yapisal veride yalnizca gorece yol donuyor', async () => {
    const result = await executeTool(
      toolWith(() => Promise.resolve(VIEW)),
      { path: 'README.md' },
      CONTEXT,
    );

    expect(result.ok && result.data).toMatchObject({ path: 'README.md' });
    expect(JSON.stringify(result)).not.toContain('/Users/');
  });
});

describe('read_project_file — ret yollari', () => {
  async function refuse(
    error: unknown,
  ): Promise<{ summary: string; errorKind: string; auditSummary: string }> {
    const audits: ToolAuditInput[] = [];
    const result = await executeTool(toolWith(rejectWith(error)), { path: 'X' }, CONTEXT, {
      onAudit: (input): void => void audits.push(input),
    });
    return {
      summary: result.summary,
      errorKind: result.ok ? '' : result.errorKind,
      auditSummary: audits[0]?.resultSummary ?? '',
    };
  }

  /**
   * **ASU-051 kabul kriteri**: var olmayan dosyada icerik uydurulmuyor ve bu
   * bir guvenlik olayi gibi sunulmuyor.
   */
  it('dosya yok: "bulunamadi" der ve icerik uydurmayi yasaklar', async () => {
    const refusal = await refuse({
      code: 'not_found',
      message: 'boyle bir dosya yok',
      escapeAttempt: false,
      auditSummary: 'reddedildi (not_found): boyle bir dosya yok',
    });

    expect(refusal.errorKind).toBe('not_found');
    expect(refusal.summary).toContain('BULUNAMADI');
    expect(refusal.summary).toContain('UYDURMA');
    expect(refusal.summary).not.toContain('ERISIM REDDEDILDI');
  });

  /** Kacis denemesi "dosya yok"tan **ayri** sunuluyor. */
  it('kacis denemesi: "erisim reddedildi" der', async () => {
    const refusal = await refuse({
      code: 'traversal',
      message: 'yol proje kokunun disina cikiyor',
      escapeAttempt: true,
      auditSummary: 'reddedildi (traversal): yol proje kokunun disina cikiyor',
    });

    expect(refusal.errorKind).toBe('sandbox_denied');
    expect(refusal.summary).toContain('ERISIM REDDEDILDI');
    expect(refusal.auditSummary).toContain('traversal');
  });

  /**
   * Blok listesi bir kacis denemesi degil ama gevsetilebilir de degil: ozet
   * kuralin pazarliksiz oldugunu soyler.
   */
  it('hassas dosya: kuralin gevsetilemedigini soyler ve icerik sizmaz', async () => {
    const refusal = await refuse({
      code: 'blocklisted',
      message: 'ortam degiskeni dosyasi (.env) okunmaz',
      escapeAttempt: false,
      auditSummary: 'reddedildi (blocklisted): ortam degiskeni dosyasi (.env) okunmaz',
    });

    expect(refusal.errorKind).toBe('blocked_file');
    expect(refusal.summary).toContain('gevsetilemez');
    expect(refusal.summary).not.toContain('ERISIM REDDEDILDI');
  });

  it('proje secilmemis: kullaniciya sorulmasini soyler', async () => {
    const refusal = await refuse({
      code: 'no_current_project',
      message: 'guncel proje secilmemis',
      escapeAttempt: false,
      auditSummary: 'reddedildi (no_current_project): guncel proje secilmemis',
    });

    expect(refusal.errorKind).toBe('no_current_project');
    expect(refusal.summary).toContain('sor');
  });

  /** Taninmayan bir hata sekli: nedeni **uydurulmuyor**. */
  it('cozulemeyen hatada neden uydurmuyor', async () => {
    const refusal = await refuse(new Error('beklenmedik'));

    expect(refusal.errorKind).toBe('read_failed');
    expect(refusal.summary).toContain('nedeni cozulemedi');
    expect(refusal.auditSummary).toContain('unknown');
  });

  /** Sozlesmeye uymayan bir yanit "okundu" sayilmaz. */
  it('bozuk yanitta icerige guvenmiyor', async () => {
    const result = await executeTool(
      toolWith(() => Promise.resolve({ path: 'README.md' })),
      { path: 'README.md' },
      CONTEXT,
    );

    expect(result.ok).toBe(false);
    expect(!result.ok && result.errorKind).toBe('invalid_response');
  });

  /** Her ret yolu deftere gecer — sessiz dusme yok (ASU-050). */
  it('her ret audit"e `failed` olarak yaziliyor', async () => {
    const audits: ToolAuditInput[] = [];

    await executeTool(
      toolWith(
        rejectWith({
          code: 'blocklisted',
          message: 'okunmaz',
          escapeAttempt: false,
          auditSummary: 'reddedildi (blocklisted): okunmaz',
        }),
      ),
      { path: '.env' },
      CONTEXT,
      { onAudit: (input): void => void audits.push(input) },
    );

    expect(audits).toHaveLength(1);
    // Tool CALISTI ve okuyamadi: `not_run` degil.
    expect(audits[0]?.outcome).toBe('failed');
  });
});

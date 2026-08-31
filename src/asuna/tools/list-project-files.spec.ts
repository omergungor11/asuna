/**
 * `list_project_files` tool testleri (ASU-068).
 *
 * Kanitlanan seyler:
 * 1. Tanim: risk 0, onaysiz, tek alanli ve strict (renderer projeyi secemez).
 * 2. Bos `path` proje kokudur ve komuta oyle gider.
 * 3. Uc ret ayri sunuluyor: kacis / blok listesi / dizin degil / bulunamadi.
 * 4. Kirpma ve "okunamaz" isareti **sessiz degil** — model ozette goruyor.
 * 5. Deftere 200 satirlik liste degil, tek satir giriyor.
 */

import { describe, expect, it, vi } from 'vitest';

import {
  auditSummaryFor,
  createListProjectFilesTool,
  modelSummaryFor,
  parseProjectDirectoryView,
  LIST_PROJECT_FILES_TOOL_NAME,
  type ProjectDirectoryView,
} from './list-project-files';
import { resolveApproval } from './approval-policy';
import { executeTool } from './registry';
import type { ToolContext } from './types';
import type { ToolAuditInput } from '../../shared/tool-event';
import { TOOL_APPROVAL_MODES } from '../config/frontend-config';

const CONTEXT: ToolContext = { sessionId: null, projectRoot: null };

const VIEW = {
  projectId: 'asuna',
  projectName: 'Asuna',
  path: 'src',
  entries: [
    { name: 'components', kind: 'dir', sizeBytes: null, blocked: false },
    { name: 'main.ts', kind: 'file', sizeBytes: 2048, blocked: false },
  ],
  totalEntries: 2,
  returnedEntries: 2,
  truncated: false,
  scanCapped: false,
  maxEntries: 200,
};

function view(overrides: Partial<ProjectDirectoryView> = {}): ProjectDirectoryView {
  const parsed = parseProjectDirectoryView(VIEW);
  if (parsed === null) {
    throw new Error('sabit gorunum sozlesmeye uymali');
  }
  return { ...parsed, ...overrides };
}

function toolWith(
  listDirectory: (path: string) => Promise<unknown>,
): ReturnType<typeof createListProjectFilesTool> {
  return createListProjectFilesTool({ listDirectory });
}

function rejectWith(error: unknown): () => Promise<unknown> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- Tauri duz nesne reddeder
  return (): Promise<unknown> => Promise.reject(error);
}

describe('list_project_files — tanim', () => {
  it('risk 0, onaysiz', () => {
    const tool = toolWith(() => Promise.resolve(VIEW));

    expect(tool.name).toBe(LIST_PROJECT_FILES_TOOL_NAME);
    expect(tool.risk).toBe(0);
    expect(tool.requiresApproval).toBe(false);
    for (const mode of TOOL_APPROVAL_MODES) {
      expect(resolveApproval(tool.risk, tool.requiresApproval, mode)).toBe('not_required');
    }
  });

  /** Renderer/model projeyi ya da ozyinelemeyi secemez. */
  it('semada `path` disinda alan kabul etmiyor', async () => {
    const listDirectory = vi.fn(() => Promise.resolve(VIEW));

    for (const args of [
      { path: 'src', projectId: 'baska' },
      { path: 'src', recursive: true },
      { path: 'src', depth: 3 },
      { recursive: true },
    ]) {
      const result = await executeTool(toolWith(listDirectory), args, CONTEXT);
      expect(result.ok).toBe(false);
    }
    expect(listDirectory).not.toHaveBeenCalled();
  });

  it('aciklamasi tek seviye oldugunu ve uydurmayi yasakladigini soyluyor', () => {
    const { description } = toolWith(() => Promise.resolve(VIEW));
    expect(description).toContain('TEK SEVIYE');
    expect(description).toContain('UYDURMA');
  });

  /** Bos `path` gecerli ve **proje koku** demek. */
  it('bos yolu komuta oldugu gibi gonderiyor', async () => {
    const listDirectory = vi.fn(() => Promise.resolve({ ...VIEW, path: '' }));

    const result = await executeTool(toolWith(listDirectory), { path: '' }, CONTEXT);

    expect(result.ok).toBe(true);
    expect(listDirectory).toHaveBeenCalledWith('');
    expect(result.summary).toContain('kok dizini');
  });
});

describe('list_project_files — ozet', () => {
  it('kompakt liste uretiyor', () => {
    const summary = modelSummaryFor(view());

    expect(summary).toContain('Asuna / src');
    expect(summary).toContain('[dizin] components/');
    expect(summary).toContain('[dosya] main.ts (2 KB)');
  });

  it('bos dizini bos diye soyluyor', () => {
    const summary = modelSummaryFor(view({ entries: [], totalEntries: 0, returnedEntries: 0 }));

    expect(summary).toContain('BOS');
  });

  /**
   * **Gate 3 M2**: sayim tavana takildiginda toplam **bilinmiyor** — model
   * "yaklasik su kadar" bile diyememeli.
   */
  it('sayim kesildiyse toplami bilmedigini soyluyor', () => {
    const summary = modelSummaryFor(
      view({ truncated: true, scanCapped: true, totalEntries: 5000, returnedEntries: 2 }),
    );

    expect(summary).toContain('EN AZ 5000 girdi');
    expect(summary).toContain('BILMIYORSUN');
  });

  /** Kirpma sessiz degil (PROJECT.md Bolum 30). */
  it('kirpilmis listeyi "tamamini gormedin" diye isaretliyor', () => {
    const summary = modelSummaryFor(
      view({ truncated: true, totalEntries: 512, returnedEntries: 2 }),
    );

    expect(summary).toContain('512 girdi');
    expect(summary).toContain('Tamamini gormedin');
  });

  /**
   * Blok listesindeki dosya **gorunur ama isaretli**; ozet modele okumayi
   * denememesini soyluyor.
   */
  it('okunamaz girdileri isaretliyor ve okunmasini yasakliyor', () => {
    const summary = modelSummaryFor(
      view({
        entries: [{ name: '.env', kind: 'file', sizeBytes: 42, blocked: true }],
        totalEntries: 1,
        returnedEntries: 1,
      }),
    );

    expect(summary).toContain('.env');
    expect(summary).toContain('OKUNAMAZ');
    expect(summary).toContain('okumayi deneme');
  });

  it('alt klasor iceriginin listede olmadigini soyluyor', () => {
    expect(modelSummaryFor(view())).toContain('Alt klasorlerin icerigi burada YOK');
  });

  /** Deftere 200 satirlik liste degil, tek satir. */
  it('audit satiri yalnizca sayi ve dizin', () => {
    expect(auditSummaryFor(view())).toBe('2 girdi listelendi: src');
    expect(auditSummaryFor(view({ path: '' }))).toBe('2 girdi listelendi: (proje koku)');
  });
});

describe('list_project_files — durust ret', () => {
  async function refuse(error: unknown): Promise<{
    ok: boolean;
    summary: string;
    errorKind: string;
    audit: ToolAuditInput | undefined;
  }> {
    const audits: ToolAuditInput[] = [];
    const result = await executeTool(toolWith(rejectWith(error)), { path: 'x' }, CONTEXT, {
      onAudit: (input): void => void audits.push(input),
    });
    return {
      ok: result.ok,
      summary: result.summary,
      errorKind: result.ok ? '' : result.errorKind,
      audit: audits[0],
    };
  }

  it('kacis denemesini erisim reddi olarak sunuyor', async () => {
    const refusal = await refuse({
      code: 'traversal',
      message: 'yol proje kokunun disina cikiyor',
      escapeAttempt: true,
      auditSummary: 'reddedildi (traversal): yol proje kokunun disina cikiyor',
    });

    expect(refusal.errorKind).toBe('sandbox_denied');
    expect(refusal.summary).toContain('ERISIM REDDEDILDI');
    expect(refusal.audit?.outcome).toBe('failed');
    expect(refusal.audit?.resultSummary).toContain('traversal');
  });

  it('blok listesini kacis denemesinden ayri sunuyor', async () => {
    const refusal = await refuse({
      code: 'blocklisted',
      message: 'hassas dizin icerigi okunmaz',
      escapeAttempt: false,
      auditSummary: 'reddedildi (blocklisted): hassas dizin icerigi okunmaz',
    });

    expect(refusal.errorKind).toBe('blocked_directory');
    expect(refusal.summary).toContain('gevsetilemez');
    expect(refusal.summary).not.toContain('ERISIM REDDEDILDI');
  });

  /** "dizin degil" ile "yok" ayni sey degil — model farkli davranmali. */
  it('dosya verildiginde `read_project_file`a yonlendiriyor', async () => {
    const refusal = await refuse({
      code: 'not_a_directory',
      message: 'hedef bir dizin degil',
      escapeAttempt: false,
      auditSummary: 'reddedildi (not_a_directory): hedef bir dizin degil',
    });

    expect(refusal.errorKind).toBe('not_a_directory');
    expect(refusal.summary).toContain('read_project_file');
  });

  it('bulunamadiginda dosya adi uydurmayi yasakliyor', async () => {
    const refusal = await refuse({
      code: 'not_found',
      message: 'boyle bir dosya yok',
      escapeAttempt: false,
      auditSummary: 'reddedildi (not_found): boyle bir dosya yok',
    });

    expect(refusal.errorKind).toBe('not_found');
    expect(refusal.summary).toContain('UYDURMA');
  });

  it('proje secilmemisken `list_projects`a yonlendiriyor', async () => {
    const refusal = await refuse({
      code: 'no_current_project',
      message: 'guncel proje secilmemis',
      escapeAttempt: false,
      auditSummary: 'reddedildi (no_current_project): guncel proje secilmemis',
    });

    expect(refusal.errorKind).toBe('no_current_project');
    expect(refusal.summary).toContain('list_projects');
  });

  it('cozulemeyen hatada neden uydurmuyor', async () => {
    const refusal = await refuse(new Error('beklenmedik'));

    expect(refusal.errorKind).toBe('list_failed');
    expect(refusal.summary).toContain('nedeni cozulemedi');
  });

  /** Yeni alan sozlesmenin parcasi: eksikse gorunume guvenilmiyor. */
  it('`scanCapped` eksik yaniti reddediyor', async () => {
    const withoutFlag: Record<string, unknown> = { ...VIEW };
    delete withoutFlag['scanCapped'];

    const result = await executeTool(
      toolWith(() => Promise.resolve(withoutFlag)),
      { path: '' },
      CONTEXT,
    );

    expect(result.ok).toBe(false);
  });

  it('sozlesmeye uymayan yanitta dosya adi uydurmuyor', async () => {
    const result = await executeTool(
      toolWith(() => Promise.resolve({ projectId: 'asuna', entries: [] })),
      { path: '' },
      CONTEXT,
    );

    expect(result.ok).toBe(false);
    expect(result.summary).toContain('guvenmiyorum');
  });

  /** Girdi sekli bozuksa **tamami** reddedilir; yarim liste sunulmaz. */
  it('tek bozuk girdi tum listeyi reddettiriyor', async () => {
    const result = await executeTool(
      toolWith(() =>
        Promise.resolve({
          ...VIEW,
          entries: [VIEW.entries[0], { name: 'bozuk' }],
        }),
      ),
      { path: '' },
      CONTEXT,
    );

    expect(result.ok).toBe(false);
  });
});

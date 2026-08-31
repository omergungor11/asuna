/**
 * `list_projects` tool testleri (ASU-067).
 *
 * Kanitlanan seyler:
 * 1. Tanim: risk 0, onaysiz, parametresiz ve strict.
 * 2. "Guncel proje" secimi `most_recently_opened` SQL'inin aynasi.
 * 3. Bos liste **basarili** bir sonuc ve modele "uydurma" diyor.
 * 4. Komut hata verdiginde sayi/ad tahmin edilmiyor.
 * 5. Deftere yol degil **sayi** yaziliyor.
 */

import { describe, expect, it, vi } from 'vitest';

import {
  createListProjectsTool,
  pickCurrentProjectId,
  summariseProjects,
  LIST_PROJECTS_TOOL_NAME,
  MAX_LISTED_PROJECTS,
} from './list-projects';
import { resolveApproval } from './approval-policy';
import { executeTool } from './registry';
import type { ToolContext } from './types';
import type { ToolAuditInput } from '../../shared/tool-event';
import type { ProjectRecord, ProjectStatus } from '../../shared/project';
import { TOOL_APPROVAL_MODES } from '../config/frontend-config';

const CONTEXT: ToolContext = { sessionId: null, projectRoot: null };

function record(
  id: string,
  overrides: Partial<ProjectRecord> = {},
): Record<string, unknown> {
  const status: ProjectStatus = overrides.status ?? 'active';
  return {
    id,
    name: overrides.name ?? id,
    path: status === 'unlinked' ? null : (overrides.path ?? `/Users/deneme/${id}`),
    description: null,
    status,
    primaryLanguage: null,
    framework: null,
    gitRemote: null,
    lastOpenedAt: overrides.lastOpenedAt ?? null,
    createdAt: '2026-08-01T10:00:00Z',
    updatedAt: '2026-08-01T10:00:00Z',
    metadataJson: '{}',
  };
}

function parsed(id: string, overrides: Partial<ProjectRecord> = {}): ProjectRecord {
  return record(id, overrides) as unknown as ProjectRecord;
}

function toolWith(listProjects: () => Promise<unknown>): ReturnType<typeof createListProjectsTool> {
  return createListProjectsTool({ listProjects });
}

describe('list_projects — tanim', () => {
  it('risk 0, onaysiz ve parametresiz', () => {
    const tool = toolWith(() => Promise.resolve([]));

    expect(tool.name).toBe(LIST_PROJECTS_TOOL_NAME);
    expect(tool.risk).toBe(0);
    expect(tool.requiresApproval).toBe(false);
    for (const mode of TOOL_APPROVAL_MODES) {
      expect(resolveApproval(tool.risk, tool.requiresApproval, mode)).toBe('not_required');
    }
  });

  it('uydurulmus parametre reddediliyor ve komut hic cagrilmiyor', async () => {
    const listProjects = vi.fn(() => Promise.resolve([]));

    for (const args of [{ status: 'active' }, { limit: 5 }, { projectId: 'x' }]) {
      const result = await executeTool(toolWith(listProjects), args, CONTEXT);
      expect(result.ok).toBe(false);
    }
    expect(listProjects).not.toHaveBeenCalled();
  });

  it('aciklamasi uydurmayi yasakliyor', () => {
    expect(toolWith(() => Promise.resolve([])).description).toContain('UYDURMA');
  });
});

describe('pickCurrentProjectId — `most_recently_opened` aynasi', () => {
  it('hic acilmamis projeler arasinda guncel proje uydurmuyor', () => {
    expect(pickCurrentProjectId([parsed('a'), parsed('b')])).toBeNull();
  });

  it('en son acilani seciyor', () => {
    const projects = [
      parsed('eski', { lastOpenedAt: '2026-08-01T10:00:00Z' }),
      parsed('yeni', { lastOpenedAt: '2026-08-30T10:00:00Z' }),
      parsed('orta', { lastOpenedAt: '2026-08-15T10:00:00Z' }),
    ];
    expect(pickCurrentProjectId(projects)).toBe('yeni');
  });

  /** SQL: `ORDER BY last_opened_at DESC, id` — esitlikte kimlik artan. */
  it('esitlikte kimligi once gelen kazaniyor', () => {
    const projects = [
      parsed('zeta', { lastOpenedAt: '2026-08-30T10:00:00Z' }),
      parsed('alfa', { lastOpenedAt: '2026-08-30T10:00:00Z' }),
    ];
    expect(pickCurrentProjectId(projects)).toBe('alfa');
  });

  /** SQL: `status != 'unlinked'` — yolu olmayan etiket guncel proje olamaz. */
  it('yalnizca etiket olan kayit guncel proje sayilmiyor', () => {
    const projects = [
      parsed('etiket', { status: 'unlinked', lastOpenedAt: '2026-08-30T10:00:00Z' }),
      parsed('gercek', { lastOpenedAt: '2026-08-01T10:00:00Z' }),
    ];
    expect(pickCurrentProjectId(projects)).toBe('gercek');
  });
});

describe('list_projects — ozet', () => {
  it('bos listede proje uydurmayi yasakliyor', () => {
    const summary = summariseProjects([], null);

    expect(summary).toContain('Hic kayitli proje yok');
    expect(summary).toContain('UYDURMA');
  });

  it('guncel projeyi isaretliyor', () => {
    const projects = [parsed('asuna'), parsed('freelancer')];
    const summary = summariseProjects(projects, 'freelancer');

    expect(summary).toContain('2 kayitli proje');
    expect(summary).toContain('freelancer');
    expect(summary).toContain('[GUNCEL PROJE]');
    // Isaret yalnizca bir satirda.
    expect(summary.split('[GUNCEL PROJE]').length - 1).toBe(1);
  });

  it('guncel proje secilmemisse kullaniciya sorulmasini soyluyor', () => {
    const summary = summariseProjects([parsed('asuna')], null);

    expect(summary).toContain('SECILMEMIS');
    expect(summary).toContain('kendin secme');
  });

  it('durumlari oldugu gibi yansitiyor', () => {
    const summary = summariseProjects(
      [parsed('kayip', { status: 'missing' }), parsed('etiket', { status: 'unlinked' })],
      null,
    );

    expect(summary).toContain('kok dizini su an bulunamiyor');
    expect(summary).toContain('yalnizca hafiza etiketi');
  });

  /** Kirpma **sessiz degil**: gormedigin kismi gormedin diye yaziyor. */
  it('cok uzun listeyi kirpiyor ve kirpildigini soyluyor', () => {
    const projects = Array.from({ length: MAX_LISTED_PROJECTS + 5 }, (_, index) =>
      parsed(`proje-${index.toString()}`),
    );
    const summary = summariseProjects(projects, null);

    expect(summary).toContain(`${(MAX_LISTED_PROJECTS + 5).toString()} kayitli proje`);
    expect(summary).toContain('tamamini gormedin');
    expect(summary).not.toContain(`proje-${(MAX_LISTED_PROJECTS + 4).toString()} (id`);
  });
});

describe('list_projects — calisma', () => {
  it('listeyi donuyor ve deftere yol degil sayi yaziyor', async () => {
    const audits: ToolAuditInput[] = [];

    const result = await executeTool(
      toolWith(() =>
        Promise.resolve([
          record('asuna', { lastOpenedAt: '2026-08-30T10:00:00Z' }),
          record('freelancer'),
        ]),
      ),
      {},
      CONTEXT,
      { onAudit: (input): void => void audits.push(input) },
    );

    expect(result.ok).toBe(true);
    expect(result.summary).toContain('asuna');
    if (result.ok) {
      expect(result.data).toEqual({
        count: 2,
        currentProjectId: 'asuna',
        projects: [
          { id: 'asuna', name: 'asuna', status: 'active', isCurrent: true },
          { id: 'freelancer', name: 'freelancer', status: 'active', isCurrent: false },
        ],
      });
    }
    expect(audits[0]?.resultSummary).toBe('2 kayitli proje listelendi');
    expect(audits[0]?.resultSummary).not.toContain('/Users');
    expect(audits[0]?.outcome).toBe('succeeded');
    expect(audits[0]?.approvalState).toBe('not_required');
  });

  it('bos liste basarili sonuc, hata degil', async () => {
    const result = await executeTool(toolWith(() => Promise.resolve([])), {}, CONTEXT);

    expect(result.ok).toBe(true);
    expect(result.summary).toContain('Hic kayitli proje yok');
  });

  it('komut hata verdiginde sayi tahmin etmiyor', async () => {
    const result = await executeTool(
      toolWith(() =>
        // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- Tauri duz nesne reddeder
        Promise.reject({ code: 'unavailable', message: 'hafiza kullanilamiyor' }),
      ),
      {},
      CONTEXT,
    );

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe('project_list_unavailable');
    }
    expect(result.summary).toContain('tahmin etme');
  });

  /** Bozuk sekil sessizce "bos liste" gibi sunulmuyor. */
  it('sozlesmeye uymayan yanitta liste uydurmuyor', async () => {
    const result = await executeTool(
      toolWith(() => Promise.resolve([{ id: 'asuna' }])),
      {},
      CONTEXT,
    );

    expect(result.ok).toBe(false);
    expect(result.summary).toContain('okunamadi');
  });
});

/**
 * `set_current_project` tool testleri (ASU-070).
 *
 * Kanitlanan seyler:
 * 1. Tanim: risk 1 + tanimin **kendi** onay talebi (mod gevsetemez).
 * 2. Model adi soyler, tool kimligi cozer — ama **tam** eslesmeyle.
 * 3. Bilinmeyen ad ve belirsiz ad ayri hatalar; ikisinde de secim YAPILMAZ.
 * 4. Onaylanmadiginda secim degismiyor ve "gectim" denmiyor.
 * 5. Host reddi (etiket / kayip kok) oldugu gibi tasiniyor.
 */

import { describe, expect, it, vi } from 'vitest';

import {
  createSetCurrentProjectTool,
  matchProjects,
  SET_CURRENT_PROJECT_TOOL_NAME,
} from './set-current-project';
import { resolveApproval } from './approval-policy';
import { executeTool } from './registry';
import type { ToolContext } from './types';
import type { ToolAuditInput } from '../../shared/tool-event';
import type { ProjectRecord, ProjectStatus } from '../../shared/project';
import { TOOL_APPROVAL_MODES } from '../config/frontend-config';

const CONTEXT: ToolContext = { sessionId: null, projectRoot: null };

function record(
  id: string,
  overrides: { name?: string; status?: ProjectStatus } = {},
): Record<string, unknown> {
  const status: ProjectStatus = overrides.status ?? 'active';
  return {
    id,
    name: overrides.name ?? id,
    path: status === 'unlinked' ? null : `/Users/deneme/${id}`,
    description: null,
    status,
    primaryLanguage: null,
    framework: null,
    gitRemote: null,
    lastOpenedAt: null,
    createdAt: '2026-08-01T10:00:00Z',
    updatedAt: '2026-08-01T10:00:00Z',
    metadataJson: '{}',
  };
}

function parsed(id: string, overrides: { name?: string } = {}): ProjectRecord {
  return record(id, overrides) as unknown as ProjectRecord;
}

const PROJECTS = [record('asuna', { name: 'Asuna' }), record('freelancer')];

const APPROVED = { approvalGate: (): Promise<'approved'> => Promise.resolve('approved') };

function toolWith(options: {
  listProjects?: () => Promise<unknown>;
  setCurrentProject?: (projectId: string) => Promise<unknown>;
}): ReturnType<typeof createSetCurrentProjectTool> {
  return createSetCurrentProjectTool({
    listProjects: options.listProjects ?? ((): Promise<unknown> => Promise.resolve(PROJECTS)),
    setCurrentProject:
      options.setCurrentProject ??
      ((projectId: string): Promise<unknown> =>
        Promise.resolve(record(projectId, { name: projectId }))),
  });
}

describe('set_current_project — tanim', () => {
  it('risk 1 ve tanimin kendisi onay istiyor', () => {
    const tool = toolWith({});

    expect(tool.name).toBe(SET_CURRENT_PROJECT_TOOL_NAME);
    expect(tool.risk).toBe(1);
    expect(tool.requiresApproval).toBe(true);
    for (const mode of TOOL_APPROVAL_MODES) {
      expect(resolveApproval(tool.risk, tool.requiresApproval, mode)).toBe('needs_approval');
    }
  });

  it('semada `project` disinda alan kabul etmiyor', async () => {
    const setCurrentProject = vi.fn((projectId: string) =>
      Promise.resolve(record(projectId)),
    );

    for (const args of [
      { project: 'asuna', projectId: 'baska' },
      { projectId: 'asuna' },
      { project: '' },
      {},
    ]) {
      const result = await executeTool(
        toolWith({ setCurrentProject }),
        args,
        CONTEXT,
        APPROVED,
      );
      expect(result.ok).toBe(false);
    }
    expect(setCurrentProject).not.toHaveBeenCalled();
  });

  it('aciklamasi kismi eslesme olmadigini ve uydurmayi yasakladigini soyluyor', () => {
    const { description } = toolWith({});
    expect(description).toContain('kismi eslesme yoktur');
    expect(description).toContain('UYDURMA');
  });
});

describe('matchProjects — ad cozumu', () => {
  it('buyuk/kucuk harf farkini yok sayiyor', () => {
    expect(matchProjects([parsed('asuna', { name: 'Asuna' })], 'ASUNA').map((p) => p.id)).toEqual(
      ['asuna'],
    );
  });

  it('kimlikle de eslesiyor', () => {
    expect(matchProjects([parsed('asuna', { name: 'Asuna' })], 'asuna')).toHaveLength(1);
  });

  /** Kismi eslesme yok: `pro` ile `proje-a`ya gecmek yanlis projede dosya okumak. */
  it('kismi eslesme kabul etmiyor', () => {
    expect(matchProjects([parsed('freelancer')], 'free')).toHaveLength(0);
    expect(matchProjects([parsed('freelancer')], 'freelancer-2')).toHaveLength(0);
  });

  it('ayni adli iki kaydi da donuyor (secim yapmiyor)', () => {
    const projects = [
      parsed('proje-a', { name: 'Deneme' }),
      parsed('proje-b', { name: 'deneme' }),
    ];
    expect(matchProjects(projects, 'Deneme')).toHaveLength(2);
  });

  /**
   * **Gate 3 H1 regresyonu**: kimlik eslesmesi once denenip hemen donuluyordu
   * ve ad tarafindaki belirsizligi yutuyordu. Kimlikler adlarin slug'i
   * (`registry::add`), yani ayri bir isim uzayi degil: `a` / `a-2` ikilisinde
   * "a'ya gec" istegi tek aday gibi gorunup **yanlis kokte** calisirdi.
   */
  it('kimlik eslesmesi ad belirsizligini yutmuyor', () => {
    const projects = [parsed('a', { name: 'a' }), parsed('a-2', { name: 'A' })];

    const matches = matchProjects(projects, 'a');

    expect(matches).toHaveLength(2);
    // Kimlik eslesmesi ilk sirada: kullanici gercekten kimlik verdiyse gorunur.
    expect(matches[0]?.id).toBe('a');
    expect(matches.map((project) => project.id)).toEqual(['a', 'a-2']);
  });

  /** Gercek vaka: `freelancer` / `freelancer-2`. */
  it('slug catismasinda iki adayi da donuyor', () => {
    const projects = [
      parsed('freelancer', { name: 'freelancer' }),
      parsed('freelancer-2', { name: 'Freelancer' }),
    ];

    expect(matchProjects(projects, 'freelancer')).toHaveLength(2);
    // Kimlikle **tam** eslesen ikinci kayit tekil kalir.
    expect(matchProjects(projects, 'freelancer-2')).toHaveLength(1);
  });

  /** Ad catismasi yoksa kimlik eslesmesi yine tek aday. */
  it('ad catismasi olmadan kimlik eslesmesi tekil kaliyor', () => {
    const projects = [parsed('asuna', { name: 'Asuna' }), parsed('baska', { name: 'Baska' })];

    expect(matchProjects(projects, 'asuna')).toHaveLength(1);
  });

  it('bosluklari kirpiyor', () => {
    expect(matchProjects([parsed('asuna')], '  asuna  ')).toHaveLength(1);
  });
});

describe('set_current_project — onay akisi', () => {
  it('onaylanmadiginda secim degismiyor', async () => {
    const setCurrentProject = vi.fn((projectId: string) =>
      Promise.resolve(record(projectId)),
    );
    const audits: ToolAuditInput[] = [];

    const result = await executeTool(
      toolWith({ setCurrentProject }),
      { project: 'freelancer' },
      CONTEXT,
      {
        approvalMode: 'safe',
        approvalGate: () => Promise.resolve('denied'),
        onAudit: (input): void => void audits.push(input),
      },
    );

    expect(setCurrentProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    expect(audits[0]?.approvalState).toBe('denied');
    expect(audits[0]?.outcome).toBe('not_run');
  });

  it('onaylandiginda geciyor ve yeni projeyi soyluyor', async () => {
    const audits: ToolAuditInput[] = [];
    const setCurrentProject = vi.fn((projectId: string) =>
      Promise.resolve(record(projectId, { name: 'freelancer' })),
    );

    const result = await executeTool(
      toolWith({ setCurrentProject }),
      { project: 'freelancer' },
      CONTEXT,
      { approvalMode: 'safe', ...APPROVED, onAudit: (i): void => void audits.push(i) },
    );

    expect(setCurrentProject).toHaveBeenCalledWith('freelancer');
    expect(result.ok).toBe(true);
    expect(result.summary).toContain('Guncel proje artik "freelancer"');
    expect(audits[0]?.resultSummary).toBe('guncel proje degisti: freelancer');
    expect(audits[0]?.outcome).toBe('succeeded');
  });

  /** Model adi soyler, kimlik bilmez: cozum tool'un isi. */
  it('adla gelen istegi kimlige ceviriyor', async () => {
    const setCurrentProject = vi.fn((projectId: string) =>
      Promise.resolve(record(projectId, { name: 'Asuna' })),
    );

    await executeTool(
      toolWith({ setCurrentProject }),
      { project: 'Asuna' },
      CONTEXT,
      APPROVED,
    );

    expect(setCurrentProject).toHaveBeenCalledWith('asuna');
  });
});

describe('set_current_project — durust ret', () => {
  it('bilinmeyen adda proje uydurmuyor ve adaylari listeliyor', async () => {
    const setCurrentProject = vi.fn((projectId: string) =>
      Promise.resolve(record(projectId)),
    );

    const result = await executeTool(
      toolWith({ setCurrentProject }),
      { project: 'olmayan-proje' },
      CONTEXT,
      APPROVED,
    );

    expect(setCurrentProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe('project_not_found');
    }
    expect(result.summary).toContain('PROJE DEGISMEDI');
    expect(result.summary).toContain('Asuna');
    expect(result.summary).toContain('freelancer');
    expect(result.summary).toContain('UYDURMA');
  });

  /** **Belirsizlikte secim yok**: adaylar listelenir, kullaniciya sorulur. */
  it('ayni adli iki proje varsa secim yapmiyor', async () => {
    const setCurrentProject = vi.fn((projectId: string) =>
      Promise.resolve(record(projectId)),
    );

    const result = await executeTool(
      toolWith({
        listProjects: () =>
          Promise.resolve([
            record('proje-a', { name: 'Deneme' }),
            record('proje-b', { name: 'Deneme' }),
          ]),
        setCurrentProject,
      }),
      { project: 'Deneme' },
      CONTEXT,
      APPROVED,
    );

    expect(setCurrentProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe('ambiguous_project');
    }
    expect(result.summary).toContain('proje-a');
    expect(result.summary).toContain('proje-b');
    expect(result.summary).toContain('KULLANICIYA SOR');
  });

  /**
   * **Gate 3 H1 kabul kaniti**: uctan uca. Slug catismasinda tool secim
   * yapmiyor ve `project_set_current` **hic** cagrilmiyor.
   */
  it('slug catismasinda hicbir projeye gecmiyor', async () => {
    const setCurrentProject = vi.fn((projectId: string) =>
      Promise.resolve(record(projectId)),
    );

    const result = await executeTool(
      toolWith({
        listProjects: () =>
          Promise.resolve([
            record('freelancer', { name: 'freelancer' }),
            record('freelancer-2', { name: 'Freelancer' }),
          ]),
        setCurrentProject,
      }),
      { project: 'freelancer' },
      CONTEXT,
      APPROVED,
    );

    expect(setCurrentProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe('ambiguous_project');
    }
    expect(result.summary).toContain('freelancer-2');
    expect(result.summary).toContain('KULLANICIYA SOR');
  });

  it('kayitli proje yokken bunu soyluyor', async () => {
    const result = await executeTool(
      toolWith({ listProjects: () => Promise.resolve([]) }),
      { project: 'asuna' },
      CONTEXT,
      APPROVED,
    );

    expect(result.ok).toBe(false);
    expect(result.summary).toContain('Kayitli hicbir proje yok');
  });

  it('host reddini oldugu gibi tasiyor ve gectigini iddia etmiyor', async () => {
    const audits: ToolAuditInput[] = [];

    const result = await executeTool(
      toolWith({
        setCurrentProject: () =>
          // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- Tauri duz nesne reddeder
          Promise.reject({
            code: 'refused',
            message: 'projenin kok dizini bulunamiyor (missing)',
          }),
      }),
      { project: 'freelancer' },
      CONTEXT,
      { ...APPROVED, onAudit: (input): void => void audits.push(input) },
    );

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe('not_switchable');
    }
    expect(result.summary).toContain('IDDIA ETME');
    expect(audits[0]?.resultSummary).toBe('proje degismedi (refused)');
    expect(audits[0]?.outcome).toBe('failed');
  });

  it('liste okunamazsa secim denemiyor', async () => {
    const setCurrentProject = vi.fn((projectId: string) =>
      Promise.resolve(record(projectId)),
    );

    const result = await executeTool(
      toolWith({
        listProjects: () =>
          // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- Tauri duz nesne reddeder
          Promise.reject({ code: 'unavailable', message: 'hafiza kullanilamiyor' }),
        setCurrentProject,
      }),
      { project: 'asuna' },
      CONTEXT,
      APPROVED,
    );

    expect(setCurrentProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    expect(result.summary).toContain('PROJE DEGISMEDI');
  });

  it('bozuk yanitta gectigini dogrulayamiyorum diyor', async () => {
    const result = await executeTool(
      toolWith({ setCurrentProject: () => Promise.resolve({ id: 'freelancer' }) }),
      { project: 'freelancer' },
      CONTEXT,
      APPROVED,
    );

    expect(result.ok).toBe(false);
    expect(result.summary).toContain('dogrulayamiyorum');
  });
});

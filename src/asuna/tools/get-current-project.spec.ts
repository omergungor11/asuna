/**
 * `get_current_project` testleri (ASU-044).
 *
 * Kanitlanan seyler:
 * 1. Tool proje adi, yol, dal ve kisa bir ozet donuyor.
 * 2. Kayitli proje yoksa **acikca** soyluyor; uc belirsizlik nedeni ayri ayri
 *    tasiniyor ve her biri modele farkli bir soru sordurtuyor.
 * 3. `git.degraded` ve bozuk devir teslim dosyasi yutulmuyor.
 * 4. Uzun icerik kirpiliyor ve ham JSON modele dokulmuyor.
 * 5. Komut hata verirse tool `ok: false` doner — basari taklit edilmiyor.
 */

import { describe, expect, it } from 'vitest';

import {
  createGetCurrentProjectTool,
  firstSentenceOf,
  GET_CURRENT_PROJECT_TOOL_NAME,
  MAX_TOOL_SUMMARY_CHARS,
  summariseProjectContext,
  toToolResult,
} from './get-current-project';
import type { ToolContext } from './types';
import {
  AsunaRegistryError,
  CONTEXT_UNKNOWN_REASONS,
  type ContextUnknownReason,
  type GitMetadata,
  type HandoffRead,
  type ProjectContextView,
  type ProjectSummary,
} from '../../shared/project';

const CONTEXT: ToolContext = { sessionId: null, projectRoot: null };

const GIT: GitMetadata = {
  isRepository: true,
  branch: 'main',
  detached: false,
  isDirty: true,
  changedTrackedFiles: 3,
  recentCommits: ['feat(ASU-043): devir teslim artefakti'],
  remote: 'github.com/omergungor/asuna',
  degraded: false,
};

const SUMMARY: ProjectSummary = {
  projectId: 'asuna',
  name: 'Asuna',
  path: '/Users/arlec/Work/asuna',
  status: 'active',
  primaryLanguage: 'Rust',
  framework: 'Tauri',
  gitRemote: 'github.com/omergungor/asuna',
  sources: [
    {
      name: 'PROJECT.md',
      excerpt: '# Asuna\n\nLocal-first, macOS sesli kisisel AI companion. Chatbot degil.',
      truncated: false,
      sizeBytes: 4096,
    },
  ],
  totalChars: 1200,
  maxChars: 6000,
  budgetExhausted: false,
};

function known(
  overrides: {
    readonly summary?: Partial<ProjectSummary>;
    readonly git?: Partial<GitMetadata>;
    readonly handoff?: HandoffRead;
    readonly truncated?: boolean;
  } = {},
): ProjectContextView {
  return {
    status: 'known',
    project: {
      summary: { ...SUMMARY, ...overrides.summary },
      git: { ...GIT, ...overrides.git },
      handoff: overrides.handoff ?? { status: 'absent' },
      totalChars: 2000,
      maxChars: 9000,
      truncated: overrides.truncated ?? false,
    },
  };
}

function unknown(reason: ContextUnknownReason): ProjectContextView {
  return { status: 'unknown', reason, message: 'Kayitli proje yok.' };
}

describe('get_current_project — ozet uretimi', () => {
  it('proje adi, yol, dal ve kisa ozet donuyor', () => {
    const summary = summariseProjectContext(known());

    expect(summary).toContain('Asuna');
    expect(summary).toContain('/Users/arlec/Work/asuna');
    expect(summary).toContain('main');
    expect(summary).toContain('3 takip edilen dosyada kaydedilmemis degisiklik var');
    expect(summary).toContain('Local-first, macOS sesli kisisel AI companion.');
    expect(summary).toContain('Rust / Tauri');
  });

  it('temiz calisma agaci "kirli" gibi sunulmuyor', () => {
    const summary = summariseProjectContext(
      known({ git: { isDirty: false, changedTrackedFiles: 0 } }),
    );

    expect(summary).toContain('calisma agaci temiz');
    expect(summary).not.toContain('kaydedilmemis');
  });

  it('git deposu olmayan proje hata gibi degil, oldugu gibi anlatiliyor', () => {
    const summary = summariseProjectContext(
      known({ git: { isRepository: false, branch: null, isDirty: false } }),
    );

    expect(summary).toContain('git deposu degil');
  });

  it('detached HEAD uydurma bir dal adina cevrilmiyor', () => {
    const summary = summariseProjectContext(known({ git: { detached: true, branch: null } }));

    expect(summary).toContain('detached');
  });

  /** PROJECT.md Bolum 30: eksik bilgi "basarili" gibi sunulmaz. */
  it('`degraded` bayragi ozete giriyor', () => {
    const summary = summariseProjectContext(known({ git: { degraded: true } }));

    expect(summary).toContain('git durumu tam okunamadi');
  });

  it('bozuk `.asuna/context.json` sessizce "bos baglam" olmuyor', () => {
    const summary = summariseProjectContext(
      known({
        handoff: {
          status: 'ignored',
          reason: 'invalid-json',
          message: '.asuna/context.json gecerli JSON degil, yok sayildi',
        },
      }),
    );

    expect(summary).toContain('gecerli JSON degil');
  });

  it('devir teslim dosyasindaki hedef ve aktif is ozete giriyor', () => {
    const summary = summariseProjectContext(
      known({
        handoff: {
          status: 'loaded',
          context: {
            projectName: 'Asuna',
            objective: 'Sesli companion MVP',
            currentMilestone: 'M4',
            activeTask: 'ASU-044 ilk tool',
            blockers: [],
            recentDecisions: [],
          },
        },
      }),
    );

    expect(summary).toContain('Sesli companion MVP');
    expect(summary).toContain('ASU-044 ilk tool');
  });

  it('kayitli aciklama yoksa cumle uydurulmuyor', () => {
    const summary = summariseProjectContext(known({ summary: { sources: [] } }));

    expect(summary).toContain('kayitli bir aciklama bulunamadi');
  });

  /** Ses oturumuna repo dokulmez (PROJECT.md Bolum 15). */
  it('uzun icerik tavana kirpiliyor', () => {
    const summary = summariseProjectContext(
      known({
        summary: {
          name: 'A'.repeat(400),
          path: `/tmp/${'b'.repeat(400)}`,
          sources: [
            {
              name: 'PROJECT.md',
              excerpt: `${'c'.repeat(4000)}.`,
              truncated: true,
              sizeBytes: 40_000,
            },
          ],
        },
      }),
    );

    expect(summary.length).toBeLessThanOrEqual(MAX_TOOL_SUMMARY_CHARS);
    expect(summary.endsWith('…')).toBe(true);
  });

  it('kirpilmis baglam sessizce "tam" gibi gosterilmiyor', () => {
    const summary = summariseProjectContext(known({ truncated: true }));

    expect(summary).toContain('kirpildi');
  });

  it('markdown basligi proje aciklamasi sayilmiyor', () => {
    expect(firstSentenceOf('# Asuna\n\n> alinti\n\nGercek aciklama burada. Devami.')).toBe(
      'Gercek aciklama burada.',
    );
    expect(firstSentenceOf('# Sadece baslik\n')).toBeNull();
  });
});

describe('get_current_project — proje bilinmiyorsa', () => {
  it('uc neden de ayri ayri, uydurmayi yasaklayan bir yonergeyle donuyor', () => {
    const guidance: Readonly<Record<ContextUnknownReason, string>> = {
      'no-registered-project': 'hangi dizinde calistigini sor',
      'no-current-selection': 'hangisinde oldugunu sor',
      'root-missing': 'kok dizini su an bulunamiyor',
    };

    for (const reason of CONTEXT_UNKNOWN_REASONS) {
      const summary = summariseProjectContext(unknown(reason));
      expect(summary).toContain('Guncel proje bilinmiyor.');
      expect(summary).toContain(guidance[reason]);
      expect(summary.length).toBeLessThanOrEqual(MAX_TOOL_SUMMARY_CHARS);
    }
  });

  /** "Bilmiyorum" dogru bir cevaptir — tool hatasi degil. */
  it('belirsizlik `ok: true` doner ama nedeni veriyle birlikte tasir', () => {
    const result = toToolResult(unknown('no-current-selection'));

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.data).toEqual({ status: 'unknown', reason: 'no-current-selection' });
    }
  });
});

describe('get_current_project — tool sozlesmesi', () => {
  it('risk 0 ve onay istemiyor', () => {
    const tool = createGetCurrentProjectTool({ fetchContext: () => Promise.resolve(known()) });

    expect(tool.name).toBe(GET_CURRENT_PROJECT_TOOL_NAME);
    expect(tool.risk).toBe(0);
    expect(tool.requiresApproval).toBe(false);
    expect(tool.timeoutMs).toBeGreaterThan(0);
  });

  it('yapisal veri donuyor ama ozet ham JSON degil', async () => {
    const tool = createGetCurrentProjectTool({ fetchContext: () => Promise.resolve(known()) });

    const result = await tool.execute({}, CONTEXT);

    expect(result.ok).toBe(true);
    expect(result.summary).not.toContain('{');
    expect(result.summary).not.toContain('sizeBytes');
    if (result.ok) {
      expect(result.data).toEqual({
        status: 'known',
        projectId: 'asuna',
        name: 'Asuna',
        path: '/Users/arlec/Work/asuna',
        branch: 'main',
        isDirty: true,
        gitDegraded: false,
      });
    }
  });

  /** PROJECT.md Bolum 30: tool hata verirse Asuna basarili gibi konusmaz. */
  it('komut reddedilirse `ok: false` doner ve tahmin etmemesi soylenir', async () => {
    const tool = createGetCurrentProjectTool({
      // Rust tarafi tipli reddediyor; `toRegistryError` bunu koruyarak tasir.
      fetchContext: (): Promise<unknown> =>
        Promise.reject(new AsunaRegistryError('disabled', 'kalici depolama kapali')),
    });

    const result = await tool.execute({}, CONTEXT);

    expect(result.ok).toBe(false);
    expect(result.summary).toContain('kalici depolama kapali');
    expect(result.summary).toContain('tahmin etme');
    if (!result.ok) {
      expect(result.errorKind).toBe('project_context_unavailable');
    }
  });

  /** Sozlesme disi bir payload sessizce "proje yok"a donusmez. */
  it('bozuk payload basari gibi gosterilmiyor', async () => {
    const tool = createGetCurrentProjectTool({
      fetchContext: () => Promise.resolve({ status: 'known', project: { summary: {} } }),
    });

    const result = await tool.execute({}, CONTEXT);

    expect(result.ok).toBe(false);
  });
});

import { describe, expect, it } from 'vitest';

import {
  AsunaRegistryError,
  CONTEXT_UNKNOWN_REASONS,
  PROJECT_RECORD_KEYS,
  ProjectContractError,
  UNKNOWN_REGISTRY_ERROR_CODE,
  hasRegisteredRoot,
  parseGitMetadata,
  parseHandoffRead,
  parseProjectAddOutcome,
  parseProjectContextView,
  parseProjectRecord,
  parseProjectRecords,
  parseProjectRemoveOutcome,
  parseProjectSummary,
  toRegistryError,
} from './project';

function payload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: 'asuna',
    name: 'Asuna',
    path: '/Users/omer/Work/asuna',
    description: 'Local-first sesli companion',
    status: 'active',
    primaryLanguage: 'TypeScript',
    framework: 'Tauri',
    gitRemote: 'github.com/omergungor/asuna',
    lastOpenedAt: '2026-08-25T10:00:00Z',
    createdAt: '2026-08-24T09:00:00Z',
    updatedAt: '2026-08-25T10:00:00Z',
    metadataJson: '{}',
    ...overrides,
  };
}

describe('parseProjectRecord', () => {
  it('gecerli bir kaydi sozlesme tipine cevirir', () => {
    const project = parseProjectRecord(payload());

    expect(project.id).toBe('asuna');
    expect(project.status).toBe('active');
    expect(project.path).toBe('/Users/omer/Work/asuna');
    expect(Object.keys(project).sort()).toEqual([...PROJECT_RECORD_KEYS].sort());
  });

  it('etiket kayitlari (unlinked) yolsuz gecerlidir', () => {
    const project = parseProjectRecord(payload({ status: 'unlinked', path: null }));
    expect(project.path).toBeNull();
    expect(hasRegisteredRoot(project)).toBe(false);
  });

  /**
   * Semadaki iki yonlu CHECK'in aynasi. Backend bir gun yolsuz bir `active`
   * satiri dondurse, UI onu "kayitli proje" sanmamali.
   */
  it('yolsuz `active` ve yollu `unlinked` reddedilir', () => {
    expect(() => parseProjectRecord(payload({ path: null }))).toThrow(ProjectContractError);
    expect(() => parseProjectRecord(payload({ status: 'unlinked' }))).toThrow(
      ProjectContractError,
    );
  });

  it('bilinmeyen status reddedilir', () => {
    expect(() => parseProjectRecord(payload({ status: 'silinmis' }))).toThrow(
      ProjectContractError,
    );
  });

  it('sozlesme disi alan reddedilir', () => {
    expect(() => parseProjectRecord(payload({ secretPath: '/etc/passwd' }))).toThrow(
      ProjectContractError,
    );
  });

  it('bozuk zaman damgasi reddedilir', () => {
    expect(() => parseProjectRecord(payload({ createdAt: '25/08/2026' }))).toThrow(
      ProjectContractError,
    );
    expect(() => parseProjectRecord(payload({ lastOpenedAt: '2026-08-25 10:00:00' }))).toThrow(
      ProjectContractError,
    );
  });

  it('hic acilmamis proje `lastOpenedAt: null` tasir', () => {
    expect(parseProjectRecord(payload({ lastOpenedAt: null })).lastOpenedAt).toBeNull();
  });

  it('gecersiz metadataJson reddedilir', () => {
    expect(() => parseProjectRecord(payload({ metadataJson: '{ bozuk' }))).toThrow(
      ProjectContractError,
    );
  });

  /** Hata mesaji gelen degeri tekrarlamaz (yol da kullanicinin verisi). */
  it('hata mesaji gelen degeri tekrarlamaz', () => {
    try {
      parseProjectRecord(payload({ status: 'GIZLI-DEGER' }));
      expect.unreachable('hata bekleniyordu');
    } catch (error) {
      expect(error).toBeInstanceOf(ProjectContractError);
      expect((error as Error).message).not.toContain('GIZLI-DEGER');
    }
  });
});

describe('parseProjectRecords', () => {
  it('dizi olmayan girdi reddedilir', () => {
    expect(() => parseProjectRecords({})).toThrow(ProjectContractError);
  });

  it('bos liste gecerlidir — kayitli proje yoksa uydurma yapilmaz', () => {
    expect(parseProjectRecords([])).toEqual([]);
  });
});

describe('registry sonuclari (ASU-040)', () => {
  it('ekleme sonucu status ile ayirt edilir', () => {
    expect(parseProjectAddOutcome({ status: 'registered', project: payload() })).toMatchObject({
      status: 'registered',
    });
    expect(
      parseProjectAddOutcome({ status: 'already-registered', project: payload() }),
    ).toMatchObject({ status: 'already-registered' });
  });

  it('bilinmeyen ekleme status"u reddedilir', () => {
    expect(() => parseProjectAddOutcome({ status: 'ok', project: payload() })).toThrow(
      ProjectContractError,
    );
  });

  /** Kayit kaldirmak hafizayi silmez: `unlinked` sonucu bunu tasir. */
  it('kaldirma sonucu silinen ile etikete dusen ayrimini tasir', () => {
    expect(parseProjectRemoveOutcome({ status: 'deleted', id: 'asuna' })).toEqual({
      status: 'deleted',
      id: 'asuna',
    });

    const unlinked = parseProjectRemoveOutcome({
      status: 'unlinked',
      project: payload({ status: 'unlinked', path: null }),
      references: 3,
    });
    expect(unlinked).toMatchObject({ status: 'unlinked', references: 3 });
  });

  it('registry hatasi tipli koda cevrilir, mesaj yutulmaz', () => {
    const typed = toRegistryError({
      code: 'path-not-found',
      message: 'verilen yol bulunamadi',
    });
    expect(typed).toBeInstanceOf(AsunaRegistryError);
    expect(typed.code).toBe('path-not-found');

    // ACL reddi duz string olarak gelir; kod uydurulmaz ama mesaj korunur.
    const acl = toRegistryError('project_add not allowed. Command not found');
    expect(acl.code).toBe(UNKNOWN_REGISTRY_ERROR_CODE);
    expect(acl.message).toContain('not allowed');
  });
});

// ---------------------------------------------------------------------------
// `project_context` sozlesmesi (ASU-044)
// ---------------------------------------------------------------------------

function summaryPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    projectId: 'asuna',
    name: 'Asuna',
    path: '/Users/omer/Work/asuna',
    status: 'active',
    primaryLanguage: 'Rust',
    framework: 'Tauri',
    gitRemote: 'github.com/omergungor/asuna',
    sources: [{ name: 'README.md', excerpt: '# Asuna', truncated: false, sizeBytes: 128 }],
    totalChars: 1200,
    maxChars: 6000,
    budgetExhausted: false,
    ...overrides,
  };
}

function gitPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    isRepository: true,
    branch: 'main',
    detached: false,
    isDirty: true,
    changedTrackedFiles: 3,
    recentCommits: ['feat(ASU-044): ilk gercek tool'],
    remote: 'github.com/omergungor/asuna',
    degraded: false,
    ...overrides,
  };
}

function contextPayload(): Record<string, unknown> {
  return {
    status: 'known',
    project: {
      summary: summaryPayload(),
      git: gitPayload(),
      handoff: { status: 'absent' },
      totalChars: 2000,
      maxChars: 9000,
      truncated: false,
    },
  };
}

describe('parseProjectContextView', () => {
  it('bilinen projeyi ozet + git + devir teslim olarak cozer', () => {
    const view = parseProjectContextView(contextPayload());

    expect(view.status).toBe('known');
    if (view.status !== 'known') {
      throw new Error('known bekleniyordu');
    }
    expect(view.project.summary.projectId).toBe('asuna');
    expect(view.project.git.branch).toBe('main');
    expect(view.project.git.recentCommits).toHaveLength(1);
    expect(view.project.handoff.status).toBe('absent');
    expect(view.project.maxChars).toBe(9000);
  });

  /**
   * Uc neden ayri tasinir: Asuna'nin soracagi soru her birinde farkli. Tek bir
   * "bilmiyorum" kovasi modeli proje uydurmaya iterdi.
   */
  it('uc belirsizlik nedenini de oldugu gibi tasir', () => {
    for (const reason of CONTEXT_UNKNOWN_REASONS) {
      const view = parseProjectContextView({
        status: 'unknown',
        reason,
        message: 'Bilinmiyor.',
      });
      expect(view).toEqual({ status: 'unknown', reason, message: 'Bilinmiyor.' });
    }
  });

  it('taninmayan belirsizlik nedeni sessizce kabul edilmez', () => {
    expect(() =>
      parseProjectContextView({ status: 'unknown', reason: 'bilinmiyor', message: 'x' }),
    ).toThrow(ProjectContractError);
  });

  it('devir teslim dosyasinin uc durumu birbirine karismaz', () => {
    expect(parseHandoffRead({ status: 'absent' })).toEqual({ status: 'absent' });

    expect(
      parseHandoffRead({
        status: 'ignored',
        reason: 'invalid-json',
        message: 'gecerli JSON degil',
      }),
    ).toMatchObject({ status: 'ignored', reason: 'invalid-json' });

    const loaded = parseHandoffRead({
      status: 'loaded',
      context: {
        projectName: 'Asuna',
        objective: 'Sesli companion',
        currentMilestone: null,
        activeTask: null,
        blockers: [],
        recentDecisions: ['DB kazanir'],
      },
    });
    expect(loaded).toMatchObject({ status: 'loaded' });
    if (loaded.status === 'loaded') {
      expect(loaded.context.recentDecisions).toEqual(['DB kazanir']);
    }
  });

  /** `degraded` bir alan degil bir uyari: tip aynasinda kaybolmamali. */
  it('git degraded bayragi sozlesmede duruyor', () => {
    const git = parseGitMetadata(gitPayload({ degraded: true, branch: null, detached: true }));

    expect(git.degraded).toBe(true);
    expect(git.branch).toBeNull();
    expect(git.detached).toBe(true);
  });

  it('beklenmeyen alan sessizce gecmez', () => {
    expect(() => parseProjectSummary(summaryPayload({ embedding: [0.1] }))).toThrow(
      ProjectContractError,
    );
  });

  it('eksik alan "bos" diye yorumlanmaz', () => {
    const withoutBranch = gitPayload();
    delete withoutBranch['branch'];
    expect(() => parseGitMetadata(withoutBranch)).toThrow(ProjectContractError);
  });
});

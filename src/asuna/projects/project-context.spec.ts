/**
 * `project_context` okuma testleri (ASU-045).
 *
 * Vurgu: okuma **asla cokmez**. Komut heniz degisebilir (ASU-044 ile ayni anda
 * yazildi); eksik ya da beklenmedik bir cikti Projeler sekmesini bozmamali,
 * yalnizca "detay yuklenemedi" demeli.
 */

import { describe, expect, it } from 'vitest';

import { readProjectContext } from './project-context';

function knownPayload(): unknown {
  return {
    status: 'known',
    project: {
      summary: {
        projectId: 'asuna',
        name: 'Asuna',
        path: '/Users/arlec/Work/asuna',
        status: 'active',
        primaryLanguage: 'TypeScript',
        framework: 'React',
        gitRemote: 'github.com/omergungor/asuna',
        sources: [
          { name: 'README.md', excerpt: 'Sesli asistan.', truncated: false, sizeBytes: 120 },
          { name: 'package.json', excerpt: 'react, zod', truncated: true, sizeBytes: 900 },
        ],
        totalChars: 42,
        maxChars: 6000,
        budgetExhausted: false,
      },
      git: {
        isRepository: true,
        branch: 'feat/asu-045',
        detached: false,
        isDirty: true,
        changedTrackedFiles: 3,
        recentCommits: ['abc feat: bir sey'],
        remote: 'github.com/omergungor/asuna',
        degraded: false,
      },
      handoff: {
        status: 'loaded',
        context: {
          projectName: 'Asuna',
          objective: 'Sesli asistan',
          currentMilestone: 'M4',
          activeTask: 'ASU-045',
          blockers: ['ASU-044 bekleniyor'],
          recentDecisions: [],
        },
      },
      totalChars: 120,
      maxChars: 6000,
      truncated: false,
    },
  };
}

describe('readProjectContext', () => {
  it('bilinen baglami ozet + git + devir teslim olarak okur', () => {
    const result = readProjectContext(knownPayload());

    expect(result.status).toBe('known');
    if (result.status !== 'known') {
      return;
    }

    expect(result.detail.name).toBe('Asuna');
    expect(result.detail.path).toBe('/Users/arlec/Work/asuna');
    expect(result.detail.git).toEqual({
      isRepository: true,
      branch: 'feat/asu-045',
      detached: false,
      dirty: true,
      changedTrackedFiles: 3,
      degraded: false,
    });
    expect(result.detail.sources).toEqual([
      { name: 'README.md', excerpt: 'Sesli asistan.', truncated: false },
      { name: 'package.json', excerpt: 'react, zod', truncated: true },
    ]);
    expect(result.detail.handoff.activeTask).toBe('ASU-045');
    expect(result.detail.handoff.blockers).toEqual(['ASU-044 bekleniyor']);
    expect(result.detail.handoff.ignoredMessage).toBeNull();
  });

  it('guncel proje bilinmiyorsa bunu hata degil, urun durumu olarak doner', () => {
    const result = readProjectContext({
      status: 'unknown',
      reason: 'no-current-selection',
      message: 'Kayitli projeler var ama guncel proje secilmemis.',
    });

    expect(result).toEqual({
      status: 'unknown',
      message: 'Kayitli projeler var ama guncel proje secilmemis.',
    });
  });

  it('bozuk devir teslim dosyasinin nedeni gizlenmez', () => {
    const payload = knownPayload() as { project: Record<string, unknown> };
    payload.project['handoff'] = {
      status: 'ignored',
      reason: 'invalid-json',
      message: '.asuna/context.json gecerli JSON degil, yok sayildi',
    };

    const result = readProjectContext(payload);

    expect(result.status === 'known' && result.detail.handoff.ignoredMessage).toBe(
      '.asuna/context.json gecerli JSON degil, yok sayildi',
    );
  });

  it('git deposu olmayan proje icin cokmez, alanlari bos birakir', () => {
    const payload = knownPayload() as { project: Record<string, unknown> };
    payload.project['git'] = { isRepository: false };
    payload.project['handoff'] = { status: 'absent' };

    const result = readProjectContext(payload);

    expect(result.status === 'known' && result.detail.git.isRepository).toBe(false);
    expect(result.status === 'known' && result.detail.git.branch).toBeNull();
    expect(result.status === 'known' && result.detail.handoff.objective).toBeNull();
  });

  it('anlasilmayan cikti istisna atmaz, unavailable doner', () => {
    expect(readProjectContext(null).status).toBe('unavailable');
    expect(readProjectContext('bir metin').status).toBe('unavailable');
    expect(readProjectContext({ status: 'known', project: {} }).status).toBe('unavailable');
  });
});

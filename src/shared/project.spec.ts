import { describe, expect, it } from 'vitest';

import {
  PROJECT_RECORD_KEYS,
  ProjectContractError,
  hasRegisteredRoot,
  parseProjectRecord,
  parseProjectRecords,
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

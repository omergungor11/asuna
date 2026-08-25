import { describe, expect, it } from 'vitest';

import { parseProjectRecords } from '../../shared/project';
import {
  PROJECT_READ_COMMANDS,
  PROJECT_WRITE_COMMANDS,
  currentProjectOf,
} from './project-registry';

function project(
  id: string,
  lastOpenedAt: string | null,
  status = 'active',
): Record<string, unknown> {
  return {
    id,
    name: id,
    path: status === 'unlinked' ? null : `/tmp/${id}`,
    description: null,
    status,
    primaryLanguage: null,
    framework: null,
    gitRemote: null,
    lastOpenedAt,
    createdAt: '2026-08-24T09:00:00Z',
    updatedAt: '2026-08-25T10:00:00Z',
    metadataJson: '{}',
  };
}

describe('currentProjectOf', () => {
  it('hicbir proje acilmamissa guncel proje uydurulmaz', () => {
    const projects = parseProjectRecords([project('bir', null), project('iki', null)]);
    expect(currentProjectOf(projects)).toBeNull();
  });

  it('en son acilan kayitli projeyi secer', () => {
    const projects = parseProjectRecords([
      project('bir', '2026-08-20T10:00:00Z'),
      project('iki', '2026-08-24T10:00:00Z'),
      project('uc', null),
    ]);
    expect(currentProjectOf(projects)?.id).toBe('iki');
  });

  /** Etiketin kayitli koku yok — "su an buradayiz" diye sunulamaz. */
  it('etiket (unlinked) guncel proje olamaz', () => {
    const projects = parseProjectRecords([
      project('etiket', '2026-08-25T10:00:00Z', 'unlinked'),
    ]);
    expect(currentProjectOf(projects)).toBeNull();
  });

  it('bos listede guncel proje yoktur', () => {
    expect(currentProjectOf([])).toBeNull();
  });
});

/**
 * Komut adlari Rust tarafiyla birebir olmali; bir yazim hatasi ancak runtime'da,
 * sessiz bir ACL reddi olarak ortaya cikardi.
 */
describe('komut adlari', () => {
  it('okuma ve yazma kumeler halinde ve birbirine karismiyor', () => {
    expect(Object.values(PROJECT_READ_COMMANDS)).toEqual(['project_list']);
    expect(Object.values(PROJECT_WRITE_COMMANDS)).toEqual([
      'project_add',
      'project_remove',
      'project_set_current',
    ]);

    const read = new Set<string>(Object.values(PROJECT_READ_COMMANDS));
    for (const command of Object.values(PROJECT_WRITE_COMMANDS)) {
      expect(read.has(command)).toBe(false);
    }
  });
});

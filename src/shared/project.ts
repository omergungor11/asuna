/**
 * `projects` sozlesmesi — Rust `ProjectRecord`'un tip aynasi (ASU-039).
 *
 * Tek kaynak `src-tauri/src/db/migrations/003_projects.up.sql`; senkron testi
 * `schema-mirror.spec.ts`. Ayni zincirin diger halkasi `db/model.rs`
 * (`PRAGMA table_info` + CHECK kisiti karsilastirmasi).
 *
 * # Asuna proje **uydurmaz**
 *
 * Bu tablo yalnizca kullanicinin acikca kaydettigi proje koklerini ve Phase
 * 3'ten devralinan etiketleri tutar (PROJECT.md Bolum 4: "full filesystem
 * indexing" MVP disi). Otomatik disk taramasi yoktur; kayitli olmayan bir
 * projeyi Asuna bilmedigini soyler.
 */

import { ContractError, assertNoUnexpectedKeys, isRecord, readers } from './contract';

/**
 * Kayitli projenin durumu.
 *
 * Degerler semadaki CHECK kisitindan gelir (`003_projects.up.sql`).
 *
 * - `active`   — kayitli, yol erisilebilir
 * - `missing`  — kayitli ama yol artik yok. Kayit **silinmez**: kullanici
 *   harici diski takmayi unutmus olabilir (ASU-040)
 * - `archived` — kullanici gecmis icin tutuyor, aktif calisilmiyor
 * - `unlinked` — kayitli kok **yok**; yalnizca hafizada gecen bir proje
 *   etiketi. `path` bu durumda her zaman `null`'dur ve satir hicbir dosya
 *   sistemi yetkisi tasimaz
 */
export const PROJECT_STATUSES = ['active', 'missing', 'archived', 'unlinked'] as const;

export type ProjectStatus = (typeof PROJECT_STATUSES)[number];

export interface ProjectRecord {
  /** Slug (`asuna`). Sayisal degil: `memories.projectId` 001'den beri metin. */
  readonly id: string;
  readonly name: string;
  /**
   * Normalize edilmis, symlink'i cozulmus mutlak yol.
   * `null` yalnizca `status === 'unlinked'` iken mumkundur (sema CHECK'i bunu
   * iki yonlu zorlar).
   */
  readonly path: string | null;
  readonly description: string | null;
  readonly status: ProjectStatus;
  readonly primaryLanguage: string | null;
  readonly framework: string | null;
  /**
   * Remote **adi** (`github.com/omergungor/asuna`) — kimlik bilgisi ya da token
   * tasiyan bir URL buraya yazilmaz (ASU-042 redaksiyondan gecirir).
   */
  readonly gitRemote: string | null;
  /** `null` = hic acilmadi. Tahmin edilmez; kullanicinin acik secimiyle dolar. */
  readonly lastOpenedAt: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly metadataJson: string;
}

/** Sozlesmedeki alanlarin tam listesi — sema kolon sirasiyla ayni. */
export const PROJECT_RECORD_KEYS = [
  'id',
  'name',
  'path',
  'description',
  'status',
  'primaryLanguage',
  'framework',
  'gitRemote',
  'lastOpenedAt',
  'createdAt',
  'updatedAt',
  'metadataJson',
] as const;

export class ProjectContractError extends ContractError {
  public override readonly name = 'ProjectContractError';
}

function fail(field: string, expected: string): never {
  throw new ProjectContractError(`\`${field}\` ${expected} olmali.`);
}

function failWith(message: string): never {
  throw new ProjectContractError(message);
}

export function parseProjectRecord(value: unknown): ProjectRecord {
  if (!isRecord(value)) {
    throw new ProjectContractError('Proje kaydi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, PROJECT_RECORD_KEYS, failWith);

  const read = readers(value, fail);

  const status = read.enumeration('status', PROJECT_STATUSES);
  const path = read.nullableText('path');
  // Semadaki tablo duzeyi CHECK'in aynasi. Buraya dusen bir ihlal, backend ile
  // sozlesmenin kaydigini gosterir; sessizce "yolu yok" diye gecistirilmez.
  if ((status === 'unlinked') !== (path === null)) {
    failWith('`path` yalnizca `unlinked` projelerde bos olabilir.');
  }

  return {
    id: read.text('id'),
    name: read.text('name'),
    path,
    description: read.nullableText('description'),
    status,
    primaryLanguage: read.nullableText('primaryLanguage'),
    framework: read.nullableText('framework'),
    gitRemote: read.nullableText('gitRemote'),
    lastOpenedAt: read.nullableTimestamp('lastOpenedAt'),
    createdAt: read.timestamp('createdAt'),
    updatedAt: read.timestamp('updatedAt'),
    metadataJson: read.jsonText('metadataJson'),
  };
}

export function parseProjectRecords(value: unknown): ProjectRecord[] {
  if (!Array.isArray(value)) {
    throw new ProjectContractError('Proje listesi bir dizi olmali.');
  }
  return value.map(parseProjectRecord);
}

/**
 * Projenin kayitli bir kok dizini var mi?
 *
 * Sandbox (ASU-049) yalnizca bu kosulu saglayan projeleri gorecek; `unlinked`
 * bir etiket hicbir dosya sistemi yetkisi tasimaz.
 */
export function hasRegisteredRoot(project: ProjectRecord): boolean {
  return project.status !== 'unlinked';
}

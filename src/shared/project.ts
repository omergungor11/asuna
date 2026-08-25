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

// ---------------------------------------------------------------------------
// Registry sonuclari (ASU-040)
// ---------------------------------------------------------------------------

/**
 * Proje ekleme sonucu — Rust `ProjectAddOutcome` aynasi.
 *
 * Cift kayit bir **hata degil**: kullanici ayni dizini iki kez secmis olabilir.
 * Ama "eklendi" demek de yanlis olurdu; hangisinin oldugu acikca doner.
 */
export type ProjectAddOutcome =
  | { readonly status: 'registered'; readonly project: ProjectRecord }
  | { readonly status: 'already-registered'; readonly project: ProjectRecord };

/**
 * Proje kaydini kaldirma sonucu — Rust `ProjectRemoveOutcome` aynasi.
 *
 * `unlinked`: bu projeye bagli hafiza vardi, bu yuzden satir silinmedi;
 * yalnizca kayitli kok kaldirildi. Kaydi kaldirmak kullanicinin hafizasini
 * silmemeli — UI bunu acikca soylemeli.
 */
export type ProjectRemoveOutcome =
  | { readonly status: 'deleted'; readonly id: string }
  | {
      readonly status: 'unlinked';
      readonly project: ProjectRecord;
      /** Etiketi kullanan hafiza + oturum sayisi. */
      readonly references: number;
    };

export function parseProjectAddOutcome(value: unknown): ProjectAddOutcome {
  if (!isRecord(value)) {
    throw new ProjectContractError('Proje ekleme sonucu bir nesne olmali.');
  }

  switch (value['status']) {
    case 'registered':
      assertNoUnexpectedKeys(value, ['status', 'project'], failWith);
      return { status: 'registered', project: parseProjectRecord(value['project']) };

    case 'already-registered':
      assertNoUnexpectedKeys(value, ['status', 'project'], failWith);
      return { status: 'already-registered', project: parseProjectRecord(value['project']) };

    default:
      fail('status', 'su degerlerden biri: registered, already-registered');
  }
}

export function parseProjectRemoveOutcome(value: unknown): ProjectRemoveOutcome {
  if (!isRecord(value)) {
    throw new ProjectContractError('Proje kaldirma sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);

  switch (value['status']) {
    case 'deleted':
      assertNoUnexpectedKeys(value, ['status', 'id'], failWith);
      return { status: 'deleted', id: read.text('id') };

    case 'unlinked':
      assertNoUnexpectedKeys(value, ['status', 'project', 'references'], failWith);
      return {
        status: 'unlinked',
        project: parseProjectRecord(value['project']),
        references: read.count('references'),
      };

    default:
      fail('status', 'su degerlerden biri: deleted, unlinked');
  }
}

// ---------------------------------------------------------------------------
// Registry hatasi (ASU-040)
// ---------------------------------------------------------------------------

/** Rust `RegistryErrorCode` ile birebir. */
export const REGISTRY_ERROR_CODES = [
  'invalid',
  /** Yol mutlak degil, `~` iceriyor, filesystem koku ya da UTF-8 disi. */
  'path-refused',
  'path-not-found',
  'not-a-directory',
  'not-found',
  /** Islem bu proje durumunda anlamsiz (orn. etiketi guncel proje yapmak). */
  'refused',
  /** `ASUNA_MEMORY_ENABLED=false` — kayit tutulamiyor. */
  'disabled',
  'unavailable',
  'storage',
] as const;

export type RegistryErrorCode = (typeof REGISTRY_ERROR_CODES)[number];

/** Taninmayan sekil (cogunlukla ACL reddi ya da IPC katmani hatasi). */
export const UNKNOWN_REGISTRY_ERROR_CODE = 'unknown';

export type AsunaRegistryErrorCode = RegistryErrorCode | typeof UNKNOWN_REGISTRY_ERROR_CODE;

export class AsunaRegistryError extends Error {
  public override readonly name = 'AsunaRegistryError';

  public constructor(
    public readonly code: AsunaRegistryErrorCode,
    message: string,
  ) {
    super(message);
  }
}

function isRegistryErrorCode(value: unknown): value is RegistryErrorCode {
  return (
    typeof value === 'string' && (REGISTRY_ERROR_CODES as readonly string[]).includes(value)
  );
}

/**
 * `invoke` reddini tipli hataya cevirir. Hicbir zaman yutmaz; en kotu ihtimalle
 * `unknown` kodlu ama mesaji korunmus bir hata uretir (`toStoreError` ile ayni
 * sozlesme).
 */
export function toRegistryError(value: unknown): AsunaRegistryError {
  if (value instanceof AsunaRegistryError) {
    return value;
  }
  if (
    isRecord(value) &&
    isRegistryErrorCode(value['code']) &&
    typeof value['message'] === 'string'
  ) {
    return new AsunaRegistryError(value['code'], value['message']);
  }
  if (typeof value === 'string' && value.length > 0) {
    return new AsunaRegistryError(UNKNOWN_REGISTRY_ERROR_CODE, value);
  }
  if (value instanceof Error) {
    return new AsunaRegistryError(UNKNOWN_REGISTRY_ERROR_CODE, value.message);
  }
  return new AsunaRegistryError(UNKNOWN_REGISTRY_ERROR_CODE, 'Proje islemi basarisiz oldu.');
}

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

// ---------------------------------------------------------------------------
// Guncel proje baglami — `project_context` (ASU-044)
// ---------------------------------------------------------------------------

/**
 * Bu bolum `src-tauri/src/projects/view.rs` (`ProjectContextView`) ve onun
 * besledigi `context.rs` / `git_metadata.rs` / `handoff.rs` tiplerinin aynasidir.
 *
 * # Neden kati parser
 *
 * Bu sozlesmenin tuketicisi `get_current_project` tool'u — yani ciktisi
 * **modele** gidiyor. Sessizce yanlis okunan bir alan, Asuna'nin sesli olarak
 * yanlis bir branch soylemesi demek. Beklenmedik/eksik alan burada gurultulu
 * bir hataya donusur ve tool `ok: false` doner (PROJECT.md Bolum 30).
 *
 * Projeler sekmesinin (`asuna/projects/project-context.ts`) **hosgorulu** bir
 * okuyucusu var ve bu bilincli: orada eksik bir alan ekranin bir satirini
 * bosaltir, burada ise modele yanlis bilgi verirdi.
 */

/** Ozete giren tek kaynak dosya (`context.rs` `ContextSource` aynasi). */
export interface ContextSource {
  /** Kok'e gore dosya adi (`README.md`). Mutlak yol **donmez**. */
  readonly name: string;
  /** Kisaltilmis icerik; manifest'lerde ham dosya degil turetilmis ozet. */
  readonly excerpt: string;
  /** Icerik kirpildi mi? Sessiz kirpma yok. */
  readonly truncated: boolean;
  /** Diskteki ham boyut — "ne kadarini gormedim?" sorusunun cevabi. */
  readonly sizeBytes: number;
}

/** Kayitli bir projenin olculmus ozeti (`context.rs` `ProjectSummary` aynasi). */
export interface ProjectSummary {
  readonly projectId: string;
  readonly name: string;
  readonly path: string;
  readonly status: ProjectStatus;
  /** Manifest kanitiyla **tespit edilmis** dil; tahmin degil. */
  readonly primaryLanguage: string | null;
  readonly framework: string | null;
  readonly gitRemote: string | null;
  readonly sources: readonly ContextSource[];
  readonly totalChars: number;
  readonly maxChars: number;
  /** Toplam butce doldugu icin en az bir kaynak kisaldi/dusuruldu. */
  readonly budgetExhausted: boolean;
}

/** Salt okuma git durumu (`git_metadata.rs` `GitMetadata` aynasi). */
export interface GitMetadata {
  /** Kokun **kendisi** bir git calisma agaci mi? Ust dizindeki repo sayilmaz. */
  readonly isRepository: boolean;
  /** `null` = detached HEAD, depo degil ya da okunamadi. */
  readonly branch: string | null;
  readonly detached: boolean;
  readonly isDirty: boolean;
  /** Degisen **takip edilen** dosya sayisi (untracked sayilmaz). */
  readonly changedTrackedFiles: number;
  /** Son commit basliklari (en yeni once), kirpilmis ve redakte edilmis. */
  readonly recentCommits: readonly string[];
  /** Redakte edilmis remote **adi**; URL/token hicbir zaman burada olmaz. */
  readonly remote: string | null;
  /**
   * Bir alt komut basarisiz oldu ya da zaman asimina ugradi.
   *
   * Yutulmaz: Asuna bu bayrak aciksa "git durumunu tam okuyamadim" demeli
   * (PROJECT.md Bolum 30).
   */
  readonly degraded: boolean;
}

/** `.asuna/context.json` semasi (`handoff.rs` `HandoffContext` aynasi). */
export interface HandoffContext {
  readonly projectName: string | null;
  readonly objective: string | null;
  readonly currentMilestone: string | null;
  readonly activeTask: string | null;
  readonly blockers: readonly string[];
  readonly recentDecisions: readonly string[];
}

/** Devir teslim dosyasinin neden yok sayildigi (`handoff.rs` aynasi). */
export const HANDOFF_IGNORE_REASONS = [
  'invalid-json',
  'not-an-object',
  'too-large',
  'unreadable',
  'outside-root',
] as const;

export type HandoffIgnoreReason = (typeof HANDOFF_IGNORE_REASONS)[number];

/**
 * Devir teslim dosyasinin okunma sonucu.
 *
 * Uc durum bilerek ayri: "dosya yok" bir hata degil, "bozuk dosya" bir uyari,
 * "okundu" bir veri. `absent` ile `ignored`'i birlestirmek, bozuk bir dosyayi
 * sessizce "bos baglam" gibi gostermek olurdu.
 */
export type HandoffRead =
  | { readonly status: 'loaded'; readonly context: HandoffContext }
  | { readonly status: 'absent' }
  | {
      readonly status: 'ignored';
      readonly reason: HandoffIgnoreReason;
      readonly message: string;
    };

/** Guncel proje biliniyor: ozet + git durumu + devir teslim artefakti. */
export interface KnownProjectContext {
  readonly summary: ProjectSummary;
  readonly git: GitMetadata;
  readonly handoff: HandoffRead;
  /** Olculen toplam karakter (ozet + git + devir teslim). */
  readonly totalChars: number;
  readonly maxChars: number;
  /** Tavan asildigi icin en az bir liste kisaldi. */
  readonly truncated: boolean;
}

/**
 * "Guncel proje" neden bilinmiyor? (`context.rs` `ContextUnknownReason`)
 *
 * Uc neden **ayri** tutulur cunku Asuna'nin soracagi soru her birinde farkli:
 * "hangi dizinde calisiyorsun?" ile "disk takili mi?" ayni soru degil. Tek bir
 * "bilmiyorum" kovasi modeli proje uydurmaya iterdi.
 */
export const CONTEXT_UNKNOWN_REASONS = [
  'no-registered-project',
  'no-current-selection',
  'root-missing',
] as const;

export type ContextUnknownReason = (typeof CONTEXT_UNKNOWN_REASONS)[number];

/** `project_context` komutunun ciktisi. */
export type ProjectContextView =
  | { readonly status: 'known'; readonly project: KnownProjectContext }
  | {
      readonly status: 'unknown';
      readonly reason: ContextUnknownReason;
      readonly message: string;
    };

const CONTEXT_SOURCE_KEYS = ['name', 'excerpt', 'truncated', 'sizeBytes'] as const;

const PROJECT_SUMMARY_KEYS = [
  'projectId',
  'name',
  'path',
  'status',
  'primaryLanguage',
  'framework',
  'gitRemote',
  'sources',
  'totalChars',
  'maxChars',
  'budgetExhausted',
] as const;

const GIT_METADATA_KEYS = [
  'isRepository',
  'branch',
  'detached',
  'isDirty',
  'changedTrackedFiles',
  'recentCommits',
  'remote',
  'degraded',
] as const;

const HANDOFF_CONTEXT_KEYS = [
  'projectName',
  'objective',
  'currentMilestone',
  'activeTask',
  'blockers',
  'recentDecisions',
] as const;

const KNOWN_CONTEXT_KEYS = [
  'summary',
  'git',
  'handoff',
  'totalChars',
  'maxChars',
  'truncated',
] as const;

/** Bos olabilen metin — `readers.text` bos string'i reddeder, alinti bos olabilir. */
function readMaybeEmptyText(source: Record<string, unknown>, field: string): string {
  const value = source[field];
  if (typeof value !== 'string') {
    fail(field, 'string');
  }
  return value;
}

function readTextList(source: Record<string, unknown>, field: string): readonly string[] {
  const value = source[field];
  if (!Array.isArray(value)) {
    fail(field, 'string dizisi');
  }
  return value.map((item) => {
    if (typeof item !== 'string') {
      fail(field, 'yalnizca string iceren bir dizi');
    }
    return item;
  });
}

function parseContextSource(value: unknown): ContextSource {
  if (!isRecord(value)) {
    throw new ProjectContractError('Baglam kaynagi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, CONTEXT_SOURCE_KEYS, failWith);

  const read = readers(value, fail);
  return {
    name: read.text('name'),
    excerpt: readMaybeEmptyText(value, 'excerpt'),
    truncated: read.boolean('truncated'),
    sizeBytes: read.count('sizeBytes'),
  };
}

export function parseProjectSummary(value: unknown): ProjectSummary {
  if (!isRecord(value)) {
    throw new ProjectContractError('Proje ozeti bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, PROJECT_SUMMARY_KEYS, failWith);

  const read = readers(value, fail);
  const sources = value['sources'];
  if (!Array.isArray(sources)) {
    fail('sources', 'bir dizi');
  }

  return {
    projectId: read.text('projectId'),
    name: read.text('name'),
    path: read.text('path'),
    status: read.enumeration('status', PROJECT_STATUSES),
    primaryLanguage: read.nullableText('primaryLanguage'),
    framework: read.nullableText('framework'),
    gitRemote: read.nullableText('gitRemote'),
    sources: sources.map(parseContextSource),
    totalChars: read.count('totalChars'),
    maxChars: read.count('maxChars'),
    budgetExhausted: read.boolean('budgetExhausted'),
  };
}

export function parseGitMetadata(value: unknown): GitMetadata {
  if (!isRecord(value)) {
    throw new ProjectContractError('Git durumu bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, GIT_METADATA_KEYS, failWith);

  const read = readers(value, fail);
  return {
    isRepository: read.boolean('isRepository'),
    branch: read.nullableText('branch'),
    detached: read.boolean('detached'),
    isDirty: read.boolean('isDirty'),
    changedTrackedFiles: read.count('changedTrackedFiles'),
    recentCommits: readTextList(value, 'recentCommits'),
    remote: read.nullableText('remote'),
    degraded: read.boolean('degraded'),
  };
}

export function parseHandoffContext(value: unknown): HandoffContext {
  if (!isRecord(value)) {
    throw new ProjectContractError('Devir teslim baglami bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, HANDOFF_CONTEXT_KEYS, failWith);

  const read = readers(value, fail);
  return {
    projectName: read.nullableText('projectName'),
    objective: read.nullableText('objective'),
    currentMilestone: read.nullableText('currentMilestone'),
    activeTask: read.nullableText('activeTask'),
    blockers: readTextList(value, 'blockers'),
    recentDecisions: readTextList(value, 'recentDecisions'),
  };
}

export function parseHandoffRead(value: unknown): HandoffRead {
  if (!isRecord(value)) {
    throw new ProjectContractError('Devir teslim sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);
  switch (value['status']) {
    case 'loaded':
      assertNoUnexpectedKeys(value, ['status', 'context'], failWith);
      return { status: 'loaded', context: parseHandoffContext(value['context']) };

    case 'absent':
      assertNoUnexpectedKeys(value, ['status'], failWith);
      return { status: 'absent' };

    case 'ignored':
      assertNoUnexpectedKeys(value, ['status', 'reason', 'message'], failWith);
      return {
        status: 'ignored',
        reason: read.enumeration('reason', HANDOFF_IGNORE_REASONS),
        message: read.text('message'),
      };

    default:
      fail('status', 'su degerlerden biri: loaded, absent, ignored');
  }
}

function parseKnownProjectContext(value: unknown): KnownProjectContext {
  if (!isRecord(value)) {
    throw new ProjectContractError('Proje baglami bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, KNOWN_CONTEXT_KEYS, failWith);

  const read = readers(value, fail);
  return {
    summary: parseProjectSummary(value['summary']),
    git: parseGitMetadata(value['git']),
    handoff: parseHandoffRead(value['handoff']),
    totalChars: read.count('totalChars'),
    maxChars: read.count('maxChars'),
    truncated: read.boolean('truncated'),
  };
}

/**
 * `project_context` ciktisini dogrular.
 *
 * `unknown` bir **hata degil**: sozlesmenin gecerli bir dali. Nedeni oldugu gibi
 * tasinir; tool onu kendi cumlesine cevirir ama bilgiyi kaybetmez.
 */
export function parseProjectContextView(value: unknown): ProjectContextView {
  if (!isRecord(value)) {
    throw new ProjectContractError('Proje baglami sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);
  switch (value['status']) {
    case 'known':
      assertNoUnexpectedKeys(value, ['status', 'project'], failWith);
      return { status: 'known', project: parseKnownProjectContext(value['project']) };

    case 'unknown':
      assertNoUnexpectedKeys(value, ['status', 'reason', 'message'], failWith);
      return {
        status: 'unknown',
        reason: read.enumeration('reason', CONTEXT_UNKNOWN_REASONS),
        message: read.text('message'),
      };

    default:
      fail('status', 'su degerlerden biri: known, unknown');
  }
}

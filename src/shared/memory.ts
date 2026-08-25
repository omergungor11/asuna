/**
 * `memories` sozlesmesi — Rust `MemoryRecord`'un tip aynasi (ASU-030).
 *
 * # Tek kaynak
 *
 * Sema `src-tauri/src/db/migrations/001_memories_sessions.up.sql` icindedir.
 * Bu dosya onun aynasidir, ikinci bir tanim degil:
 * `src/shared/schema-mirror.spec.ts` o `.sql` dosyasini **okuyup** buradaki
 * [`MEMORY_KINDS`] ve [`MEMORY_RECORD_KEYS`] listeleriyle karsilastirir.
 * Kolon eklemek/silmek ya da bir `kind` degeri eklemek, bu dosyaya
 * dokunulmadigi surece kirmizi test uretir.
 *
 * Ayni zincirin Rust ucu: `src-tauri/src/db/model.rs` (`PRAGMA table_info`
 * karsilastirmasi). ADR-005 kurali: sema degisikligi ile ayna **ayni
 * commit'te** gider.
 */

import { ContractError, assertNoUnexpectedKeys, isRecord, readers } from './contract';

/**
 * Hafiza siniflandirmasi — PROJECT.md Bolum 5.3, semadaki `kind` CHECK kisiti
 * ile birebir (sira dahil).
 *
 * `working_context` listede ama durable tabloya **kural olarak terfi etmez**
 * (PROJECT.md Bolum 14): extraction bir adayi bu sinifta isaretleyip eleyebilsin
 * diye tanimli.
 */
export const MEMORY_KINDS = [
  'profile',
  'preference',
  'project',
  'decision',
  'task',
  'working_context',
  'relationship',
  'idea',
  'routine',
  'tool_state',
] as const;

export type MemoryKind = (typeof MEMORY_KINDS)[number];

export function isMemoryKind(value: unknown): value is MemoryKind {
  return typeof value === 'string' && (MEMORY_KINDS as readonly string[]).includes(value);
}

export interface MemoryRecord {
  readonly id: number;
  readonly kind: MemoryKind;
  readonly title: string;
  readonly content: string;
  readonly summary: string | null;
  /** Phase 4'e kadar serbest metin, FK'siz (ASU-039 baglayacak). */
  readonly projectId: string | null;
  /** `[0, 1]` — Stage A siralamasinin ana girdisi. */
  readonly importance: number;
  /** `[0, 1]` — cikarimin ne kadar emin oldugu. */
  readonly confidence: number;
  /** "Bu neden hatirlaniyor?" sorusunun cevabi (memory.md Bolum 2). */
  readonly sourceSessionId: number | null;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly lastAccessedAt: string | null;
  readonly expiresAt: string | null;
  readonly isArchived: boolean;
  readonly metadataJson: string;
}

/**
 * Sozlesmedeki alanlarin tam listesi — sema kolon sirasiyla ayni.
 *
 * `embedding` bilerek **yok**: MVP'de yazilmiyor (Stage B'ye ayrilmis) ve bir
 * BLOB'u her okumada renderer'a tasimak bos maliyet. Istisna
 * [`MEMORY_COLUMNS_NOT_MIRRORED`] ile acikca kayitli, sessizce unutulmus degil.
 */
export const MEMORY_RECORD_KEYS = [
  'id',
  'kind',
  'title',
  'content',
  'summary',
  'projectId',
  'importance',
  'confidence',
  'sourceSessionId',
  'createdAt',
  'updatedAt',
  'lastAccessedAt',
  'expiresAt',
  'isArchived',
  'metadataJson',
] as const;

/** Semada olup sozlesmeye **bilerek** alinmayan kolonlar. */
export const MEMORY_COLUMNS_NOT_MIRRORED = ['embedding'] as const;

export class MemoryContractError extends ContractError {
  public override readonly name = 'MemoryContractError';
}

function fail(field: string, expected: string): never {
  throw new MemoryContractError(`\`${field}\` ${expected} olmali.`);
}

function failWith(message: string): never {
  throw new MemoryContractError(message);
}

export function parseMemoryRecord(value: unknown): MemoryRecord {
  if (!isRecord(value)) {
    throw new MemoryContractError('Hafiza kaydi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, MEMORY_RECORD_KEYS, failWith);

  const read = readers(value, fail);

  return {
    id: read.id('id'),
    kind: read.enumeration('kind', MEMORY_KINDS),
    title: read.text('title'),
    content: read.text('content'),
    summary: read.nullableText('summary'),
    projectId: read.nullableText('projectId'),
    importance: read.unitInterval('importance'),
    confidence: read.unitInterval('confidence'),
    sourceSessionId: read.nullableId('sourceSessionId'),
    createdAt: read.timestamp('createdAt'),
    updatedAt: read.timestamp('updatedAt'),
    lastAccessedAt: read.nullableTimestamp('lastAccessedAt'),
    expiresAt: read.nullableTimestamp('expiresAt'),
    isArchived: read.boolean('isArchived'),
    metadataJson: read.jsonText('metadataJson'),
  };
}

export function parseMemoryRecords(value: unknown): MemoryRecord[] {
  if (!Array.isArray(value)) {
    throw new MemoryContractError('Hafiza listesi bir dizi olmali.');
  }
  return value.map(parseMemoryRecord);
}

// ---------------------------------------------------------------------------
// Istek sozlesmeleri (ASU-031) — Rust `memory_repository` girdi tiplerinin aynasi
// ---------------------------------------------------------------------------

/** Yeni hafiza kaydi. Sunucu tarafi ayrica dogrular; bu tip yalnizca sekli tarif eder. */
export interface MemoryDraft {
  readonly kind: MemoryKind;
  readonly title: string;
  readonly content: string;
  readonly summary?: string;
  readonly projectId?: string;
  /** `[0, 1]` */
  readonly importance: number;
  /** `[0, 1]` */
  readonly confidence: number;
  readonly sourceSessionId?: number;
  readonly expiresAt?: string;
  readonly metadataJson?: string;
}

/**
 * Kismi guncelleme.
 *
 * Nullable alanlarda uc durum vardir ve JSON'da da ayirt edilir:
 * **alan yok** = dokunma · **`null`** = temizle · **deger** = ata.
 * (Rust tarafinda `Option<Option<T>>`.)
 */
export interface MemoryPatch {
  readonly kind?: MemoryKind;
  readonly title?: string;
  readonly content?: string;
  readonly summary?: string | null;
  readonly projectId?: string | null;
  readonly importance?: number;
  readonly confidence?: number;
  readonly expiresAt?: string | null;
  readonly metadataJson?: string;
}

/** Arsiv gorunumu. Varsayilan `active` — arsivlenmis kayitlar retrieval'a girmez. */
export type MemoryArchiveFilter = 'active' | 'archived' | 'all';

/** Siralama secenekleri; her biri semadaki bir index'e karsilik gelir. */
export type MemorySort = 'recent' | 'oldest' | 'importance';

/**
 * Liste filtresi. Verilmeyen her alan icin sunucu tarafi **retrieval icin
 * guvenli** varsayilani kullanir (arsivli yok, suresi dolmus yok, erisim izi
 * birakilmaz).
 */
export interface MemoryFilter {
  /** Tek kayit getirmek icin — ayri bir IPC komutu acmadan `getById`. */
  readonly id?: number;
  readonly kinds?: readonly MemoryKind[];
  readonly projectId?: string;
  readonly archived?: MemoryArchiveFilter;
  /** `title` / `content` / `summary` icinde alt dize aramasi. */
  readonly search?: string;
  readonly includeExpired?: boolean;
  /**
   * `true` ise kullanici onayi bekleyen kayitlar elenir (ASU-034/ASU-035).
   *
   * Varsayilan `false`: Memory UI onay bekleyenleri **gormeye devam etmeli**
   * (kullanici onlari inceleyip onaylayabilsin). Stage A retrieval'i Rust
   * tarafinda `true` verir — onaylanmamis hafiza modelin baglamina girmez.
   */
  readonly excludePendingApproval?: boolean;
  readonly sort?: MemorySort;
  readonly limit?: number;
  /**
   * Donen kayitlarin `lastAccessedAt` degeri guncellensin mi?
   *
   * Liste goruntulemek erisim degildir; Stage A retrieval'i (ASU-035) erisimdir.
   */
  readonly markAccessed?: boolean;
}

// ---------------------------------------------------------------------------
// Yazma sonucu
// ---------------------------------------------------------------------------

/** `ASUNA_MEMORY_ENABLED=false` — hicbir sey yazilmadi. */
export type MemorySkipReason = 'memory-disabled';

/**
 * Yazma isleminin sonucu.
 *
 * `skipped` bir hata degil ama **sessiz de degil**: hafiza kapaliyken cagiran
 * taraf "kaydettim" diyemesin diye durum acikca tasinir (PROJECT.md Bolum 20).
 */
export type MemoryWriteResult =
  | { readonly status: 'stored'; readonly record: MemoryRecord }
  | { readonly status: 'deleted'; readonly id: number }
  | { readonly status: 'skipped'; readonly reason: MemorySkipReason };

const MEMORY_SKIP_REASONS = ['memory-disabled'] as const;

export function parseMemoryWriteResult(value: unknown): MemoryWriteResult {
  if (!isRecord(value)) {
    throw new MemoryContractError('Yazma sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);

  switch (value['status']) {
    case 'stored':
      assertNoUnexpectedKeys(value, ['status', 'record'], failWith);
      return { status: 'stored', record: parseMemoryRecord(value['record']) };

    case 'deleted':
      assertNoUnexpectedKeys(value, ['status', 'id'], failWith);
      return { status: 'deleted', id: read.id('id') };

    case 'skipped':
      assertNoUnexpectedKeys(value, ['status', 'reason'], failWith);
      return { status: 'skipped', reason: read.enumeration('reason', MEMORY_SKIP_REASONS) };

    default:
      fail('status', 'su degerlerden biri: stored, deleted, skipped');
  }
}

/** Yazma gercekten diske dustu mu? UI "kaydettim" demeden once buna bakar. */
export function wasMemoryStored(result: MemoryWriteResult): boolean {
  return result.status !== 'skipped';
}

// ---------------------------------------------------------------------------
// Onay bekleyen hafizalar (ASU-034 mekanizmasi, ASU-037 UI'i)
// ---------------------------------------------------------------------------

/**
 * `metadata_json` icindeki onay bayragi — Rust
 * `extraction::PENDING_APPROVAL_KEY` ile birebir ayni.
 *
 * Hassas turlerde (profil, iliski) cikarim kaydi **yazar ama isaretler**;
 * kullanici acikca onaylayana kadar retrieval o kaydi baglama koymaz
 * (PROJECT.md Bolum 26 sonu).
 */
export const MEMORY_PENDING_APPROVAL_KEY = 'pendingApproval';

/**
 * Kayit kullanici onayi bekliyor mu?
 *
 * Yalnizca `true` bayragi "bekliyor" demektir: bayrak yoksa ya da `false` ise
 * kayit normaldir. Bozuk/beklenmedik `metadata_json` "bekliyor" sayilmaz —
 * aksi halde tek bir bicim hatasi tum listeyi onay kuyruguna doldururdu.
 */
export function isPendingApproval(record: MemoryRecord): boolean {
  return readMetadata(record.metadataJson)?.[MEMORY_PENDING_APPROVAL_KEY] === true;
}

/**
 * Onay bayragini **kaldirmadan** `false` yapar; diger metadata alanlari
 * (orn. `extraction.promptVersion`) korunur.
 *
 * Bayrak silinmez, `false` yazilir: Rust tarafi "anahtar yoksa ne demek?"
 * sorusunu bilerek yaratmiyor.
 */
export function withApprovalGranted(metadataJson: string): string {
  const metadata = readMetadata(metadataJson) ?? {};
  return JSON.stringify({ ...metadata, [MEMORY_PENDING_APPROVAL_KEY]: false });
}

/** Gecerli bir JSON **nesnesi** ise dondurur; degilse `null`. */
function readMetadata(metadataJson: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(metadataJson);
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Toplu silme (ASU-037)
// ---------------------------------------------------------------------------

/**
 * "Tum hafizayi sil" komutunun istedigi onay ifadesi — Rust
 * `memory_repository::DELETE_ALL_CONFIRMATION` ile **birebir** ayni.
 *
 * Ifade komut imzasinin parcasi: cift onay yalnizca UI'da yasasaydi, tek bir
 * yanlis `invoke` tum hafizayi silebilirdi. Turkce karakter yok — kullanicinin
 * klavye duzeninden bagimsiz yazilabilmeli.
 */
export const MEMORY_DELETE_ALL_CONFIRMATION = 'TUM HAFIZAYI SIL';

/**
 * Toplu silmenin sonucu.
 *
 * `deleted` bir sayidir cunku kullanici "gercekten gitti mi, kac tane?"
 * sorusunun cevabini gormeli.
 */
export type MemoryPurgeResult =
  | { readonly status: 'purged'; readonly deleted: number }
  | { readonly status: 'skipped'; readonly reason: MemorySkipReason };

export function parseMemoryPurgeResult(value: unknown): MemoryPurgeResult {
  if (!isRecord(value)) {
    throw new MemoryContractError('Silme sonucu bir nesne olmali.');
  }

  const read = readers(value, fail);

  switch (value['status']) {
    case 'purged':
      assertNoUnexpectedKeys(value, ['status', 'deleted'], failWith);
      // `count`: sifir gecerli bir sonuc (zaten bos depo).
      return { status: 'purged', deleted: read.count('deleted') };

    case 'skipped':
      assertNoUnexpectedKeys(value, ['status', 'reason'], failWith);
      return { status: 'skipped', reason: read.enumeration('reason', MEMORY_SKIP_REASONS) };

    default:
      fail('status', 'su degerlerden biri: purged, skipped');
  }
}

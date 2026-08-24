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

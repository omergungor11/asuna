/**
 * Hafizanin renderer tarafindaki **tek** erisim noktasi (ASU-031).
 *
 * # Sozlesme
 *
 * - React bileseni `invoke` cagirmaz, SQL gormez, komut adi bilmez: bu servisi
 *   cagirir (ADR-005 / CLAUDE.md "servis katmani zorunlu").
 * - Komutlar **kaba tanelidir**; her SQL sorgusu icin ayri bir komut yoktur.
 *   `getMemoryById` bile ayri bir IPC yuzeyi acmaz — `memory_list`'in `id`
 *   filtresidir.
 * - Gelen her yanit sema dogrulamasindan gecer (`src/shared/memory.ts`).
 *   IPC'den gelen veri harici veridir; tip *iddia* edilmez, dogrulanir.
 * - Hata yutulmaz: Rust'in tipli hatasi [`AsunaStoreError`]'a cevrilir ve
 *   cagirana firlatilir. "Hafiza kapali" ile "hafiza bozuk" ayri seylerdir —
 *   birincisi `skipped` sonucu, ikincisi `unavailable` kodlu hatadir
 *   (PROJECT.md Bolum 30).
 */

import { invoke } from '@tauri-apps/api/core';

import {
  parseMemoryRecords,
  parseMemoryWriteResult,
  type MemoryDraft,
  type MemoryFilter,
  type MemoryPatch,
  type MemoryRecord,
  type MemoryWriteResult,
} from '../../shared/memory';
import { toStoreError } from '../../shared/store-error';

/**
 * Rust tarafindaki komut adlari. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-memory-{read,write}.json` ile birebir ayni olmali.
 *
 * Okuma ve yazma bilerek ayri kumeler: yazma yetkisi capability duzeyinde
 * kaldirilabilir olmali.
 */
export const MEMORY_READ_COMMANDS = {
  list: 'memory_list',
} as const;

export const MEMORY_WRITE_COMMANDS = {
  create: 'memory_create',
  update: 'memory_update',
  archive: 'memory_archive',
  delete: 'memory_delete',
} as const;

/** Tek `invoke` noktasi: hata cevirisi her cagri icin ayni sekilde yapilsin. */
async function call(command: string, args?: Record<string, unknown>): Promise<unknown> {
  try {
    return await (args === undefined
      ? invoke<unknown>(command)
      : invoke<unknown>(command, args));
  } catch (error) {
    throw toStoreError(error);
  }
}

/**
 * Filtreye uyan kayitlari getirir.
 *
 * Hafiza kapaliyken **bos dizi** doner (hata degil). Bozuk oldugunda
 * `unavailable` kodlu hata firlatir.
 */
export async function listMemories(filter?: MemoryFilter): Promise<MemoryRecord[]> {
  const raw = await call(MEMORY_READ_COMMANDS.list, { filter: filter ?? null });
  return parseMemoryRecords(raw);
}

/**
 * Tek kaydi kimligiyle getirir.
 *
 * Arsivli ve suresi dolmus kayitlar da doner: kullanici acikca **bu** kaydi
 * istedi; filtreleme retrieval'in isi.
 *
 * @param markAccessed kayit gercekten kullanildiysa (baglama girdiyse) `true`.
 */
export async function getMemoryById(
  id: number,
  markAccessed = false,
): Promise<MemoryRecord | null> {
  const records = await listMemories({
    id,
    archived: 'all',
    includeExpired: true,
    limit: 1,
    markAccessed,
  });
  return records[0] ?? null;
}

export async function createMemory(draft: MemoryDraft): Promise<MemoryWriteResult> {
  return parseMemoryWriteResult(await call(MEMORY_WRITE_COMMANDS.create, { draft }));
}

export async function updateMemory(id: number, patch: MemoryPatch): Promise<MemoryWriteResult> {
  return parseMemoryWriteResult(await call(MEMORY_WRITE_COMMANDS.update, { id, patch }));
}

/** Arsivler (`archived: true`) ya da arsivden cikarir. */
export async function archiveMemory(id: number, archived = true): Promise<MemoryWriteResult> {
  return parseMemoryWriteResult(await call(MEMORY_WRITE_COMMANDS.archive, { id, archived }));
}

/**
 * Kaydi **kalici olarak** siler.
 *
 * Arsivleme varsayilan yol ama gercek silme de olmali: kullanici hafizasini
 * gercekten silebilmeli (PROJECT.md Bolum 20).
 */
export async function deleteMemory(id: number): Promise<MemoryWriteResult> {
  return parseMemoryWriteResult(await call(MEMORY_WRITE_COMMANDS.delete, { id }));
}

/**
 * Hafiza (SQLite) alt sisteminin durumu — Rust `DbStatus`'un tip aynasi (ASU-029).
 *
 * Kaynak gercek: `src-tauri/src/db/state.rs`. ADR-005 kurali: sema/sozlesme
 * degisikligi ile bu ayna **ayni commit'te** gider. `src/db/` diye bir
 * TypeScript sema dizini yoktur — sema Rust tarafindadir, burada yalnizca
 * IPC sinirindan gecen sozlesme durur.
 *
 * Gelen payload tip *iddia* edilmez, [`parseDbStatus`] ile **dogrulanir**:
 * IPC sinirindan gelen her sey harici veridir.
 */

import { ContractError, assertNoUnexpectedKeys, isRecord, readers } from './contract';

/**
 * Hafizanin uc olasi durumu. `disabled` ile `unavailable` bilerek ayri:
 *
 * - `disabled` — kullanici `ASUNA_MEMORY_ENABLED=false` dedi. Ariza yok,
 *   DB dosyasi hic acilmadi (PROJECT.md Bolum 20 gizlilik garantisi).
 * - `unavailable` — acilis/migration basarisiz. Bu bir arizadir ve UI'da
 *   "kapali" ile ayni gorunmemeli; kullanici hafizasinin neden calismadigini
 *   bilmeli (PROJECT.md Bolum 30: "surface status").
 */
export const DB_AVAILABILITY_STATES = ['ready', 'disabled', 'unavailable'] as const;
export type DbAvailability = (typeof DB_AVAILABILITY_STATES)[number];

export interface DbStatus {
  readonly availability: DbAvailability;
  /** `PRAGMA user_version` — yalnizca `ready` iken dolu. */
  readonly schemaVersion: number | null;
  /** Gomulu SQLite surumu (`bundled`), makineden bagimsiz. */
  readonly sqliteVersion: string;
  /** Yalnizca `unavailable` iken dolu; kisa ve kullaniciya gosterilebilir. */
  readonly reason: string | null;
}

/** Sozlesmede izin verilen alanlarin tam listesi. */
export const DB_STATUS_KEYS = [
  'availability',
  'schemaVersion',
  'sqliteVersion',
  'reason',
] as const;

/**
 * Durum sozlesmesi ihlali.
 *
 * Mesaj yalnizca **alan adini** ve beklenen bicimi tasir, gelen degeri asla —
 * hata mesajlari log'a ve UI'a duser.
 */
export class DbStatusError extends ContractError {
  public override readonly name = 'DbStatusError';
}

function fail(field: string, expected: string): never {
  throw new DbStatusError(`\`${field}\` ${expected} olmali.`);
}

function failWith(message: string): never {
  throw new DbStatusError(message);
}

/**
 * Rust tarafindan gelen ham payload'u dogrular.
 *
 * Beklenmeyen alanlar **reddedilir** (whitelist): backend bir gun yanlislikla
 * fazladan bir alan dondurse (orn. DB dosya yolu), bu sessizce renderer'a
 * akmak yerine gurultulu bir hataya donusur.
 */
export function parseDbStatus(value: unknown): DbStatus {
  if (!isRecord(value)) {
    throw new DbStatusError('Durum payload bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, DB_STATUS_KEYS, failWith);

  const read = readers(value, fail);

  return {
    availability: read.enumeration('availability', DB_AVAILABILITY_STATES),
    // `0` gecerli bir sema surumu — `count`, `id`den farkli olarak sifira izin verir.
    schemaVersion: read.nullableCount('schemaVersion'),
    sqliteVersion: read.text('sqliteVersion'),
    reason: read.nullableText('reason'),
  };
}

/**
 * Hafiza yazilabilir mi? Cagiran taraf bunu kontrol etmek zorunda —
 * "hafiza var" varsayimi yapan kod, hafizasiz modda sessizce yanlis calisir.
 */
export function isMemoryUsable(status: DbStatus): boolean {
  return status.availability === 'ready';
}

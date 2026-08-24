/**
 * Hafiza durumunun renderer tarafindaki **tek** okuma noktasi (ASU-029).
 *
 * ADR-005 karari: React component'leri `invoke` cagirmaz, SQL yazmaz; servis
 * katmanini cagirir. Bu dosya o katmanin ilk uyesi — `memory-service.ts`
 * (CRUD, ASU-031) yanina gelecek.
 */

import { invoke } from '@tauri-apps/api/core';

import { parseDbStatus, type DbStatus } from '../../shared/db-status';

/**
 * Rust tarafindaki komut adi. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-db.json` ile birebir ayni olmali.
 */
export const DB_STATUS_COMMAND = 'db_status';

/**
 * Hafiza alt sisteminin guncel durumunu getirir.
 *
 * Onbelleklenmez: durum uygulama omru boyunca degisebilir (ornegin DB dosyasi
 * silinir ya da disk dolar) ve "hatirliyorum" iddiasinin dogrulugu buna bagli.
 * Hata yutulmaz — cagiran taraf hafizasiz modu gorunur kilmak zorunda.
 */
export async function fetchDbStatus(): Promise<DbStatus> {
  const raw = await invoke<unknown>(DB_STATUS_COMMAND);
  return parseDbStatus(raw);
}

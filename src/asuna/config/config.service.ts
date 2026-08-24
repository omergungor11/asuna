/**
 * Renderer tarafinin **tek** config okuma noktasi (ASU-009).
 *
 * Renderer'da `process.env` / `import.meta.env` uzerinden config okunmaz
 * (ESLint `no-restricted-globals` bunu ayrica zorlar). Degerler Tauri komutu
 * ile guvenilir process'ten gelir ve sema dogrulamasindan gecer.
 */

import { invoke } from '@tauri-apps/api/core';

import { parseFrontendConfig, type FrontendConfig } from './frontend-config';

/**
 * Rust tarafindaki komut adi. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-config.json` ile birebir ayni olmali.
 */
export const GET_FRONTEND_CONFIG_COMMAND = 'get_frontend_config';

let pending: Promise<FrontendConfig> | null = null;

async function fetchFrontendConfig(): Promise<FrontendConfig> {
  try {
    const raw = await invoke<unknown>(GET_FRONTEND_CONFIG_COMMAND);
    return parseFrontendConfig(raw);
  } catch (error) {
    // Hatayi yutma; sonraki cagri yeniden denesin diye cache'i temizle.
    pending = null;
    throw error;
  }
}

/**
 * Config'i yukler ve process omru boyunca onbellekler. Es zamanli cagrilar
 * ayni istegi paylasir — acilista birden fazla IPC turu olusmaz.
 */
export function loadFrontendConfig(): Promise<FrontendConfig> {
  pending ??= fetchFrontendConfig();
  return pending;
}

/**
 * Onbellegi temizler. Ayarlar degistiginde (ve testlerde) kullanilir.
 */
export function resetFrontendConfigCache(): void {
  pending = null;
}

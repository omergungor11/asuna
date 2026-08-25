/**
 * Gizlilik anahtarlarinin renderer tarafindaki **tek** erisim noktasi (ASU-037).
 *
 * # Sozlesme
 *
 * - React bileseni `invoke` cagirmaz, komut adi bilmez: bu servisi cagirir
 *   (ADR-005 / CLAUDE.md "servis katmani zorunlu").
 * - Gelen yanit sema dogrulamasindan gecer (`src/shared/privacy.ts`).
 * - Hata yutulmaz: Rust'in tipli hatasi [`AsunaPrivacyError`]'a cevrilir.
 *   `locked-by-env` "bozuk" degil, **kural**: acilista kapatilmis bir anahtar
 *   calisma zamaninda acilamaz.
 * - Onbellek yok. Ayar bir gizlilik iddiasidir; ekran her acildiginda gercek
 *   durum sorulur, bayat bir "acik" gosterilmez.
 */

import { invoke } from '@tauri-apps/api/core';

import {
  parsePrivacySettings,
  toPrivacyError,
  type PrivacyPatch,
  type PrivacySettings,
} from '../../shared/privacy';

/**
 * Rust tarafindaki komut adlari. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-privacy.json` ile birebir ayni olmali.
 */
export const PRIVACY_COMMANDS = {
  get: 'get_privacy_settings',
  set: 'set_privacy_settings',
} as const;

/**
 * Tek `invoke` noktasi: hata cevirisi her cagri icin ayni sekilde yapilsin.
 *
 * Cevrilen sey **IPC reddi**dir. Sema dogrulamasi bilerek disarida kalir:
 * bozuk bir payload bir "gizlilik hatasi" degil, sozlesme ihlalidir ve
 * [`PrivacyContractError`] olarak yukselir (memory-service ile ayni disiplin).
 */
async function call(command: string, args?: Record<string, unknown>): Promise<unknown> {
  try {
    return await (args === undefined
      ? invoke<unknown>(command)
      : invoke<unknown>(command, args));
  } catch (error) {
    throw toPrivacyError(error);
  }
}

/** Guncel (etkin + acilis) gizlilik durumunu getirir. */
export async function fetchPrivacySettings(): Promise<PrivacySettings> {
  return parsePrivacySettings(await call(PRIVACY_COMMANDS.get));
}

/**
 * Anahtarlari degistirir ve **yeni** durumu dondurur.
 *
 * Donen deger istegin kopyasi degil, sunucunun kabul ettigi gercek durumdur:
 * UI kendi tahminini gosterip yalan soylemesin.
 *
 * `.env` dosyasi degismez — degisiklik yalnizca calisan process icin gecerlidir
 * ve bir sonraki acilista env yine kaynaktir.
 */
export async function updatePrivacySettings(patch: PrivacyPatch): Promise<PrivacySettings> {
  return parsePrivacySettings(await call(PRIVACY_COMMANDS.set, { patch }));
}

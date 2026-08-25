/**
 * Proje kok dizini secici (ASU-045).
 *
 * # Neden ayri bir modul
 *
 * React bileseni plugin API'sini **dogrudan** cagirmaz (CLAUDE.md): dizin
 * secici de diger her sey gibi servis katmanindan gecer. Bilesen yalnizca
 * "bana bir yol ver" der; hangi plugin'in, hangi izinle acildigi burada kalir
 * ve testte tek noktadan sahtelenir.
 *
 * # Yetki siniri
 *
 * Acilan tek kapi `dialog:allow-open` ve yalnizca **dizin** secimi icin
 * (`directory: true`). `save` / `message` / `confirm` izinleri bilerek
 * kapalidir (`src-tauri/capabilities/asuna-dialog.json`): dosya secip okumak ya
 * da kullaniciya sistem penceresi cizdirmek bu task'in ihtiyaci degil.
 *
 * Secilen yol **dogrulanmaz**: mutlak olma, var olma, symlink cozumu ve dizin
 * olma kontrolu `project_add` komutunun (Rust) isi. Renderer'da yapilan bir
 * dogrulama guvenlik siniri olusturamaz.
 */

import { open } from '@tauri-apps/plugin-dialog';

/** Sistem penceresinin basligi. */
export const PROJECT_DIRECTORY_PICKER_TITLE = 'Proje kökünü seçin';

/**
 * Kullaniciya dizin secme penceresi acar.
 *
 * @returns Secilen mutlak yol; kullanici vazgectiyse `null`.
 * @throws Plugin cagirilamazsa (izin yok / plugin kayitli degil) hata yukselir;
 *   cagiran bunu kullaniciya **gostermeli**, sessizce yutmamali.
 */
export async function pickProjectDirectory(): Promise<string | null> {
  const selection: unknown = await open({
    directory: true,
    multiple: false,
    title: PROJECT_DIRECTORY_PICKER_TITLE,
  });

  // `directory: true` + `multiple: false` icin sozlesme `string | null`; yine de
  // gelen sey tip *iddia* edilmez, daraltilir.
  return typeof selection === 'string' && selection.length > 0 ? selection : null;
}

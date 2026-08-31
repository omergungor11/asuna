/**
 * Composer'in "Projeden dosya ekle" secicisi icin dizin kaynagi (WP3).
 *
 * # Neden burada
 *
 * Bilesenler IPC bilmez (CLAUDE.md); kompozisyon koku bilir. Renderer tarafinda
 * dizin listelemeye ayrilmis bir **servis** henuz yok: `list_project_dir`
 * komutunun tek TS sarmalayicisi `src/asuna/tools/list-project-files.ts` ve o
 * dosya modele metin ureten bir tool — UI'nin ihtiyaci olan sey ise
 * dogrulanmis veri. Bu modul aradaki en ince koprudur: **parser'lari yeniden
 * kullanir**, hicbir dogrulamayi kopyalamaz.
 *
 * > Kalici yeri `src/asuna/projects/` altinda bir servis olmali; frontend
 * > agent'in yazma izni orada yok, bu yuzden kok'te durup raporlaniyor.
 *
 * # Guvenlik siniri degismedi
 *
 * Komut `project_id` almaz (hedef her zaman registry'deki **guncel** projedir)
 * ve yalnizca kok'e gore gorece bir metin kabul eder. Traversal reddi, symlink
 * cozumu, blok listesi ve 200 girdi tavani Rust tarafinda
 * (`src-tauri/src/projects/listing.rs` + `security::sandbox`).
 */

import { invoke } from '@tauri-apps/api/core';

import {
  LIST_PROJECT_DIR_COMMAND,
  parseProjectDirectoryRefusal,
  parseProjectDirectoryView,
  type ProjectDirectoryView,
} from '../asuna/tools/list-project-files';

/**
 * Guncel proje kokune gore bir klasoru listeler.
 *
 * Ret durumunda host'un urettigi mesaj korunur; "bir seyler ters gitti" turu
 * bos hata uretilmez (PROJECT.md Bolum 30).
 */
export async function listCurrentProjectDirectory(path: string): Promise<ProjectDirectoryView> {
  let raw: unknown;
  try {
    raw = await invoke<unknown>(LIST_PROJECT_DIR_COMMAND, { path });
  } catch (error) {
    const refusal = parseProjectDirectoryRefusal(error);
    throw new Error(
      refusal === null
        ? 'Klasör listelenemedi ve nedeni çözülemedi.'
        : `Klasör listelenemedi: ${refusal.message}`,
      { cause: error },
    );
  }

  const view = parseProjectDirectoryView(raw);
  if (view === null) {
    throw new Error('Klasör listelendi ama yanıt beklenen biçimde değil.');
  }
  return view;
}

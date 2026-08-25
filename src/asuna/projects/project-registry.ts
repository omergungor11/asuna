/**
 * Kayitli proje koklerinin renderer tarafindaki **tek** erisim noktasi (ASU-040).
 *
 * # Sozlesme
 *
 * - React bileseni `invoke`'u degil bu servisi cagirir (CLAUDE.md: bilesenler
 *   dogrudan DB/shell'e dokunmaz).
 * - Yol dogrulamasi **burada yapilmaz**: mutlak olma, var olma, symlink cozumu
 *   ve `..` sadelestirmesi Rust tarafinin isidir. Renderer'in yaptigi bir
 *   dogrulama guvenlik sinirini olusturamaz; buradaki tek is dizin secicinin
 *   verdigi metni oldugu gibi iletmektir.
 * - `~` genisletme yok, otomatik disk taramasi yok. Asuna yalnizca kullanicinin
 *   acikca ekledigi kokleri gorur (PROJECT.md Bolum 4).
 * - "Guncel proje" ayri bir bayrak degil: kullanicinin en son **acik** secimi
 *   (`lastOpenedAt`). Bu yuzden `setCurrentProject` bir yazma islemidir.
 */

import { invoke } from '@tauri-apps/api/core';

import {
  parseProjectAddOutcome,
  parseProjectRecord,
  parseProjectRecords,
  parseProjectRemoveOutcome,
  toRegistryError,
  type ProjectAddOutcome,
  type ProjectRecord,
  type ProjectRemoveOutcome,
} from '../../shared/project';

/**
 * Rust tarafindaki komut adlari. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-projects{,-read}.json` ile birebir ayni olmali.
 *
 * Okuma ve degistirme bilerek ayri kumeler: projeleri gormek ile yeni bir kok
 * kaydedebilmek ayri yetkilerdir — kayitli kok listesi ASU-049 path
 * sandbox'inin tek kaynagi olacak.
 */
export const PROJECT_READ_COMMANDS = {
  list: 'project_list',
} as const;

export const PROJECT_WRITE_COMMANDS = {
  add: 'project_add',
  remove: 'project_remove',
  setCurrent: 'project_set_current',
} as const;

async function call(command: string, args: Record<string, unknown>): Promise<unknown> {
  try {
    return await invoke<unknown>(command, args);
  } catch (error) {
    throw toRegistryError(error);
  }
}

/**
 * Kayitli projeler; durumlari her cagride tazelenir (kaybolan kok `missing`).
 *
 * Bu bir disk taramasi **degildir**: yalnizca zaten kayitli koklerin var olup
 * olmadigi sorulur, hicbir dizinin icine girilmez.
 */
export async function listProjects(): Promise<ProjectRecord[]> {
  return parseProjectRecords(await call(PROJECT_READ_COMMANDS.list, {}));
}

/**
 * Kullanicinin sectigi dizini kaydeder.
 *
 * @param path Dizin secicinin verdigi **mutlak** yol. Host tarafi bunu
 *   `canonicalize` eder; var olmayan ya da dizin olmayan bir yol tipli hatayla
 *   reddedilir.
 * @param name Verilmezse dizin adi kullanilir.
 */
export async function addProject(path: string, name?: string): Promise<ProjectAddOutcome> {
  return parseProjectAddOutcome(
    await call(PROJECT_WRITE_COMMANDS.add, { path, name: name ?? null }),
  );
}

/**
 * Projeyi kayittan cikarir.
 *
 * Bagli hafiza varsa satir silinmez, etikete dusurulur (`status: 'unlinked'`) —
 * UI bunu kullaniciya soylemeli: "kayit kaldirildi, hafiza etiketi korundu".
 */
export async function removeProject(projectId: string): Promise<ProjectRemoveOutcome> {
  return parseProjectRemoveOutcome(await call(PROJECT_WRITE_COMMANDS.remove, { projectId }));
}

/** "Guncel proje" secimi — kullanicinin acik eylemi, tahmin degil. */
export async function setCurrentProject(projectId: string): Promise<ProjectRecord> {
  return parseProjectRecord(await call(PROJECT_WRITE_COMMANDS.setCurrent, { projectId }));
}

/**
 * Listedeki "guncel proje": en son acilan **kayitli** proje.
 *
 * Hicbiri acilmamissa `null` doner ve Asuna "hangi projedeyiz?" sorusuna
 * bilmedigini soyler — uydurmaz (ASU-041).
 */
export function currentProjectOf(projects: readonly ProjectRecord[]): ProjectRecord | null {
  let current: ProjectRecord | null = null;
  for (const project of projects) {
    if (project.lastOpenedAt === null || project.status === 'unlinked') {
      continue;
    }
    if (current === null || project.lastOpenedAt > (current.lastOpenedAt ?? '')) {
      current = project;
    }
  }
  return current;
}

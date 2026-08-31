/**
 * `list_projects` — kayitli proje koklerini listeler (ASU-067, risk 0).
 *
 * # Neden gerekti
 *
 * Canli testte ortaya cikan somut bosluk: kullanici Projeler sekmesinden bir
 * proje ekliyor, ama Asuna'nin registry'ye bakacak **hicbir** tool'u yok.
 * `get_current_project` yalnizca **tek** projeyi (guncel olani) gorur ve
 * secim yapilmamissa "bilmiyorum" der — "hangi projelerim var?" sorusunun
 * cevabi hicbir yerden gelmiyordu. Model de listeyi uyduramaz.
 *
 * # Ince backchannel
 *
 * `get_current_project` ile ayni desen: tool renderer'da calisir, tek isi
 * `project_list` komutunu cagirmak. Komut zaten Projeler sekmesini besliyor
 * (ASU-040) ve **salt okuma**: hicbir dizinin icine girmez, yalnizca kayitli
 * koklerin hala var olup olmadigini `stat` ile tazeler. Yeni bir Rust yuzeyi
 * acilmadi.
 *
 * # "Guncel proje" burada nasil belirleniyor
 *
 * `project_list` bir "current" bayragi dondurmez cunku oyle bir kolon yok:
 * guncel proje = **en son acilan** kayitli proje (`registry::current` →
 * `most_recently_opened`). [`pickCurrentProjectId`] o SQL'in aynasidir ve saf
 * bir fonksiyon olarak test edilir. Ikinci bir IPC cagrisi (`project_context`)
 * yapilmiyor: o komut dosya okur ve git calistirir — bir liste sorusu icin
 * fazlasiyla pahali.
 *
 * # Uydurma yok
 *
 * Liste bossa bu **basarili** bir sonuctur ve ozet modele acikca "hic kayitli
 * proje yok, uydurma" der. Kayitli olmayan bir dizini "projen" diye anlatmak
 * PROJECT.md Bolum 4'un (otomatik disk taramasi yok) yasakladigi seyin sesli
 * karsiligi olurdu.
 */

import { invoke } from '@tauri-apps/api/core';

import { NO_TOOL_ARGUMENTS, type AsunaToolDefinition, type ToolResult } from './types';
import {
  parseProjectRecords,
  toRegistryError,
  type ProjectRecord,
  type ProjectStatus,
} from '../../shared/project';

/**
 * Rust tarafindaki komut adi — `src-tauri/build.rs` (ACL manifest) ve
 * `capabilities/asuna-projects-read.json` ile birebir ayni olmali.
 */
const PROJECT_LIST_COMMAND = 'project_list';

export const LIST_PROJECTS_TOOL_NAME = 'list_projects';

/**
 * Tool cagrisi ust siniri.
 *
 * Komut kayitli kok sayisi kadar `stat` yapar (kayitli proje sayisi onlarla
 * olculur). 10 sn bunun kat kat ustunde; asilmasi bir aksaklik isaretidir —
 * ornegin bagli olmayan bir ag surucusu.
 */
export const LIST_PROJECTS_TIMEOUT_MS = 10_000;

/**
 * Ozete girecek en fazla proje.
 *
 * Cikti sesli bir cevabin girdisi; 40 satirlik bir liste zaten okunamaz.
 * Asilirsa **sessizce** kesilmez, kirpildigi ciktida yazar.
 */
export const MAX_LISTED_PROJECTS = 40;

/** Durum kodlarinin konusulabilir karsiligi (`shared/project.ts` aynasi). */
const STATUS_LABELS: Readonly<Record<ProjectStatus, string>> = {
  active: 'aktif',
  missing: 'kok dizini su an bulunamiyor',
  archived: 'arsivlenmis',
  unlinked: 'yalnizca hafiza etiketi, kayitli dizini yok',
};

/**
 * Guncel projenin kimligi — `registry::current` (`most_recently_opened`) aynasi.
 *
 * Kural birebir ayni: `last_opened_at` dolu olan ve `unlinked` **olmayan**
 * kayitlar arasinda en yeni acilan; esitlikte kimlik alfabetik olarak once
 * gelen. Hicbiri yoksa `null` — "guncel proje" tahmin edilmez (PROJECT.md
 * Bolum 11).
 *
 * Zaman damgalari sozlesme tarafindan ISO-8601 UTC olarak dogrulanmis
 * ([`parseProjectRecord`]), dolayisiyla metin karsilastirmasi kronolojik
 * siralamayla ayni sonucu verir.
 */
export function pickCurrentProjectId(projects: readonly ProjectRecord[]): string | null {
  let current: ProjectRecord | null = null;

  for (const project of projects) {
    if (project.lastOpenedAt === null || project.status === 'unlinked') {
      continue;
    }
    if (current === null) {
      current = project;
      continue;
    }
    // `current.lastOpenedAt` null olamaz: yukaridaki filtreden gecti.
    const openedAt = current.lastOpenedAt ?? '';
    if (project.lastOpenedAt > openedAt) {
      current = project;
    } else if (project.lastOpenedAt === openedAt && project.id < current.id) {
      current = project;
    }
  }

  return current?.id ?? null;
}

function describeProject(project: ProjectRecord, isCurrent: boolean): string {
  const marker = isCurrent ? ' [GUNCEL PROJE]' : '';
  const where = project.path ?? 'kayitli dizin yok';
  return `- ${project.name} (id: ${project.id})${marker} — ${where} [${STATUS_LABELS[project.status]}]`;
}

/**
 * Modele giden metin.
 *
 * Saf fonksiyon — IPC'siz test edilir. Yol **oldugu gibi** yaziliyor:
 * `get_current_project` de guncel projenin mutlak yolunu modele veriyor
 * (ASU-044), dolayisiyla burada farkli bir kural uygulamak ayni bilgiyi iki
 * ayri gizlilik seviyesinde tutmak olurdu. Kimlik de yaziliyor cunku
 * `set_current_project` ad yerine kimlikle de calisabilir.
 */
export function summariseProjects(
  projects: readonly ProjectRecord[],
  currentProjectId: string | null,
): string {
  if (projects.length === 0) {
    return (
      'Hic kayitli proje yok. Asuna yalnizca kullanicinin ACIKCA kaydettigi ' +
      'proje dizinlerini gorur; diski kendi basina taramaz. Kullaniciya hangi ' +
      'dizinde calistigini sor ve Projeler sekmesinden eklemesini iste (ya da ' +
      'izin verirse `register_project` ile ekle). Proje adi UYDURMA.'
    );
  }

  const shown = projects.slice(0, MAX_LISTED_PROJECTS);
  const lines = shown.map((project) =>
    describeProject(project, project.id === currentProjectId),
  );

  const header =
    currentProjectId === null
      ? `${projects.length.toString()} kayitli proje var; guncel proje SECILMEMIS.`
      : `${projects.length.toString()} kayitli proje var.`;

  const notices: string[] = [];
  if (projects.length > shown.length) {
    notices.push(
      `DIKKAT: yalnizca ilk ${shown.length.toString()} proje listelendi, ` +
        'tamamini gormedin.',
    );
  }
  if (currentProjectId === null) {
    notices.push(
      'Guncel proje secilmemis: hangisinde calisildigini kullaniciya SOR, ' +
        'kendin secme.',
    );
  }

  return [header, ...lines, ...notices].join('\n');
}

export interface ListProjectsOptions {
  /** IPC yerine sahte kaynak enjekte etmek icin (testler). */
  readonly listProjects?: () => Promise<unknown>;
}

function defaultListProjects(): Promise<unknown> {
  return invoke<unknown>(PROJECT_LIST_COMMAND, {});
}

/**
 * Tool'u kurar.
 *
 * `risk: 0` (salt okuma) ve `requiresApproval: false`: hicbir kayit degismez,
 * hicbir dizinin icine girilmez (PROJECT.md Bolum 5.4 / 17).
 */
export function createListProjectsTool(options: ListProjectsOptions = {}): AsunaToolDefinition {
  const listProjects = options.listProjects ?? defaultListProjects;

  return {
    name: LIST_PROJECTS_TOOL_NAME,
    description:
      'Kullanicinin Asuna\'ya KAYDETTIGI tum projeleri listeler: proje adi, kimlik, dizin ' +
      'yolu, durum ve hangisinin guncel proje oldugu. "Hangi projelerim var?", "kayitli ' +
      'projeleri say", "X projesi kayitli mi?", "baska hangi projelerde calisiyorum?" gibi ' +
      'sorularda kullan; ayrica bir projeye gecmeden once dogru adi ogrenmek icin. ' +
      'Parametre almaz. Yalnizca kayitli projeleri gorur — disk taramaz. Liste bossa ' +
      'kullaniciya bunu soyle; proje adi UYDURMA.',
    risk: 0,
    requiresApproval: false,
    timeoutMs: LIST_PROJECTS_TIMEOUT_MS,
    parameters: NO_TOOL_ARGUMENTS,
    async execute(): Promise<ToolResult> {
      let projects: readonly ProjectRecord[];
      try {
        projects = parseProjectRecords(await listProjects());
      } catch (error) {
        const message = toRegistryError(error).message;
        return {
          ok: false,
          summary:
            `Kayitli proje listesi okunamadi: ${message} ` +
            'Kullaniciya bunu oldugu gibi soyle; proje adi ya da sayisi tahmin etme.',
          auditSummary: 'proje listesi okunamadi',
          errorKind: 'project_list_unavailable',
        };
      }

      const currentProjectId = pickCurrentProjectId(projects);

      return {
        ok: true,
        summary: summariseProjects(projects, currentProjectId),
        // Deftere yol **girmez**: hangi projelerin kayitli oldugu bilgisi
        // sayidan ibaret kalir (`read_project_file` ile ayni ayrim).
        auditSummary: `${projects.length.toString()} kayitli proje listelendi`,
        data: {
          count: projects.length,
          currentProjectId,
          projects: projects.map((project) => ({
            id: project.id,
            name: project.name,
            status: project.status,
            isCurrent: project.id === currentProjectId,
          })),
        },
      };
    },
  };
}

/** Varsayilan ornek — `index.ts` bunu varsayilan registry'ye kaydeder. */
export const listProjectsTool: AsunaToolDefinition = createListProjectsTool();

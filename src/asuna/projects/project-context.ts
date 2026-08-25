/**
 * Guncel projenin baglami — Projeler sekmesinin detay bolumunun kaynagi
 * (ASU-045; komut ASU-044 `project_context`).
 *
 * # Ayni komut, iki tuketici
 *
 * Asuna'nin `get_current_project` tool'u ile bu ekran **ayni** komuttan
 * beslenir. Boylece ekranda gorunen ile sesli soylenen ayrisamaz: kullanici
 * "hangi projedeyiz?" diye sordugunda duydugu cevap, sekmeye bakinca gordugu
 * seyin aynisidir.
 *
 * # Neden hosgorulu okuma
 *
 * `shared/*` sozlesmelerinin aksine burada beklenmedik bir alan **istisna
 * atmaz**. Gerekce kapsam: proje listesi ve "guncel proje" secimi (ASU-040,
 * kati sozlesme) detay yuklenemedigi icin kaybolmamali. Detay bir **ek**;
 * eksikse ekran nedenini yazar, liste calismaya devam eder.
 *
 * # Kural: yok olan sey uydurulmaz
 *
 * Okunamayan alan `null` kalir ve arayuzde nedeni yazar. Ozet, branch ve devir
 * teslim alanlarinin hicbiri tahmin edilmez (PROJECT.md Bolum 30).
 */

import { invoke } from '@tauri-apps/api/core';

import { isRecord } from '../../shared/contract';
import { toRegistryError } from '../../shared/project';

/**
 * ASU-044 komutu. Argument **almaz**: renderer ne projeyi ne de okunacak
 * dosyayi secebilir (capability: `asuna-projects-read`).
 */
export const PROJECT_CONTEXT_COMMAND = 'project_context';

/** Ozete giren tek kaynak (`README.md`, `package.json`, ...). */
export interface ProjectContextSource {
  readonly name: string;
  readonly excerpt: string;
  /** Icerik kirpildi mi? Sessiz kirpma yok. */
  readonly truncated: boolean;
}

/** Salt okuma git durumu (`git_metadata.rs` aynasi, gosterilen alt kume). */
export interface ProjectGitView {
  readonly isRepository: boolean;
  /** `null` = depo degil, dal okunamadi ya da detached HEAD. */
  readonly branch: string | null;
  readonly detached: boolean;
  readonly dirty: boolean;
  readonly changedTrackedFiles: number;
  /** Bir alt komut basarisiz oldu — eksik bilgi "tam" gibi sunulmaz. */
  readonly degraded: boolean;
}

/** `.asuna/context.json` devir teslim artefakti (`handoff.rs` aynasi). */
export interface ProjectHandoffView {
  readonly objective: string | null;
  readonly currentMilestone: string | null;
  readonly activeTask: string | null;
  readonly blockers: readonly string[];
  /** Dosya var ama kullanilamadi — nedeni gizlenmez. */
  readonly ignoredMessage: string | null;
}

/** Guncel projenin gosterilebilir baglami. */
export interface ProjectContextDetail {
  readonly projectId: string | null;
  readonly name: string | null;
  readonly path: string | null;
  readonly sources: readonly ProjectContextSource[];
  readonly git: ProjectGitView;
  readonly handoff: ProjectHandoffView;
  /** Tavan asildigi icin en az bir liste kisaldi. */
  readonly truncated: boolean;
}

/**
 * Detay sonucu.
 *
 * - `known`       — baglam var
 * - `unknown`     — komut calisti ama guncel proje bilinmiyor (kayit yok, secim
 *   yok, kok kayip). Bu bir **hata degil**, urun durumu; Asuna da bu durumda
 *   proje uydurmaz, sorar.
 * - `unavailable` — komut cagirilamadi ya da anlasilmayan bir cevap dondu
 */
export type ProjectContextResult =
  | { readonly status: 'known'; readonly detail: ProjectContextDetail }
  | { readonly status: 'unknown'; readonly message: string }
  | { readonly status: 'unavailable'; readonly message: string };

function readRecord(value: unknown): Record<string, unknown> | null {
  return isRecord(value) ? value : null;
}

function readText(source: Record<string, unknown> | null, key: string): string | null {
  if (source === null) {
    return null;
  }
  const value = source[key];
  return typeof value === 'string' && value.trim().length > 0 ? value : null;
}

function readFlag(source: Record<string, unknown> | null, key: string): boolean {
  return source !== null && source[key] === true;
}

function readCount(source: Record<string, unknown> | null, key: string): number {
  if (source === null) {
    return 0;
  }
  const value = source[key];
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : 0;
}

function readTextList(source: Record<string, unknown> | null, key: string): readonly string[] {
  if (source === null) {
    return [];
  }
  const value = source[key];
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string' && item.length > 0)
    : [];
}

function readSources(value: unknown): readonly ProjectContextSource[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const sources: ProjectContextSource[] = [];
  for (const entry of value) {
    const source = readRecord(entry);
    const name = readText(source, 'name');
    const excerpt = readText(source, 'excerpt');
    if (name === null || excerpt === null) {
      continue;
    }
    sources.push({ name, excerpt, truncated: readFlag(source, 'truncated') });
  }
  return sources;
}

function readGit(value: unknown): ProjectGitView {
  const git = readRecord(value);
  return {
    isRepository: readFlag(git, 'isRepository'),
    branch: readText(git, 'branch'),
    detached: readFlag(git, 'detached'),
    dirty: readFlag(git, 'isDirty'),
    changedTrackedFiles: readCount(git, 'changedTrackedFiles'),
    degraded: readFlag(git, 'degraded'),
  };
}

function readHandoff(value: unknown): ProjectHandoffView {
  const handoff = readRecord(value);
  // `absent` (dosya yok) ile `ignored` (dosya bozuk) ayni sey degil: ilki
  // sessizdir, ikincisi ekranda uyari olur.
  const ignoredMessage =
    handoff?.['status'] === 'ignored'
      ? (readText(handoff, 'message') ?? '.asuna/context.json okunamadı, yok sayıldı')
      : null;
  const context = readRecord(handoff?.['context']);

  return {
    objective: readText(context, 'objective'),
    currentMilestone: readText(context, 'currentMilestone'),
    activeTask: readText(context, 'activeTask'),
    blockers: readTextList(context, 'blockers'),
    ignoredMessage,
  };
}

/**
 * Komut ciktisini gosterilebilir sonuca cevirir.
 *
 * Saf fonksiyon: IPC'siz test edilebilir ve **asla** istisna atmaz. Sekil
 * `ProjectContextView` (Rust) ile ayni, ama eksik/degisen alan ekrani cokertmez.
 */
export function readProjectContext(value: unknown): ProjectContextResult {
  const root = readRecord(value);
  if (root === null) {
    return { status: 'unavailable', message: 'Beklenmeyen bir yanıt geldi.' };
  }

  if (root['status'] === 'unknown') {
    return {
      status: 'unknown',
      message: readText(root, 'message') ?? 'Güncel proje bilinmiyor.',
    };
  }

  // `Known { project }` sarmali; sarmalsiz bir cevap gelirse kok nesne okunur.
  const project = readRecord(root['project']) ?? root;
  const summary = readRecord(project['summary']);
  if (summary === null) {
    return { status: 'unavailable', message: 'Yanıtta proje özeti yok.' };
  }

  return {
    status: 'known',
    detail: {
      projectId: readText(summary, 'projectId'),
      name: readText(summary, 'name'),
      path: readText(summary, 'path'),
      sources: readSources(summary['sources']),
      git: readGit(project['git']),
      handoff: readHandoff(project['handoff']),
      truncated: readFlag(project, 'truncated') || readFlag(summary, 'budgetExhausted'),
    },
  };
}

/**
 * Guncel projenin baglamini getirir.
 *
 * **Reddetmez**: komut yoksa, ACL kapaliysa ya da cevap anlasilmazsa
 * `unavailable` doner. Detayin yuklenememesi Projeler sekmesini bozmamali —
 * ama nedeni de gizlenmemeli, mesaj korunur.
 */
export async function fetchProjectContext(): Promise<ProjectContextResult> {
  try {
    return readProjectContext(await invoke<unknown>(PROJECT_CONTEXT_COMMAND, {}));
  } catch (error) {
    return { status: 'unavailable', message: toRegistryError(error).message };
  }
}

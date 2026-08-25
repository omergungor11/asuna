/**
 * `get_current_project` — Asuna'nin **ilk gercek tool'u** (ASU-044, risk 0).
 *
 * NOT: Phase 5'te registry'ye (ASU-047) tasinacak. Burada tool dogrudan
 * Realtime oturumuna veriliyor (`use-asuna-session` varsayilan listesi); registry,
 * permission gate ve `tool_events` audit yazimi ASU-047/048'in isi. Bu dosyanin
 * o tasinmada degismesi gereken tek yeri kayit noktasidir — sozlesme
 * (`AsunaToolDefinition`) ve ozet uretimi oldugu gibi kalir.
 *
 * # Ince backchannel
 *
 * Function tool'lar `RealtimeSession`'in kostugu yerde, yani **renderer'da**
 * calisir (voice.md Bolum 9). Bu yuzden burada dosya okunmaz, git cagrilmaz,
 * DB'ye dokunulmaz: tek is `project_context` komutunu cagirmak. Kayitli kok
 * secimi, dosya allowlist'i, `.env` blok listesi ve git zaman asimi guvenilir
 * tarafta (Rust) — PROJECT.md Bolum 19'un guvenlik modeli.
 *
 * # Modele ham JSON dokulmez
 *
 * Komut ~9 000 karaktere kadar yapisal veri dondurebilir. Sesli bir oturumda bunu
 * modele oldugu gibi vermek hem token israfi hem de "repoyu dokme" yasagina
 * (PROJECT.md Bolum 15) aykiri. Tool, konusulabilir **kisa** bir ozet uretir
 * ([`MAX_TOOL_SUMMARY_CHARS`]); yapisal veri `ToolResult.data` icinde kalir ve
 * UI/audit icin kullanilabilir.
 *
 * # Uydurma yok
 *
 * - Guncel proje bilinmiyorsa uc neden ayri ayri yansitilir; ozet modele
 *   **sormasini** soyler (PROJECT.md Bolum 11/30).
 * - Komut hata verirse `ok: false` doner ve ozet bunu acikca yazar; Asuna
 *   basarili gibi konusamaz.
 * - `git.degraded` yutulmaz: "git durumu tam okunamadi" ozete girer.
 */

import { invoke } from '@tauri-apps/api/core';

import type { AsunaToolDefinition, ToolResult } from './types';
import {
  parseProjectContextView,
  toRegistryError,
  type ContextUnknownReason,
  type ProjectContextView,
} from '../../shared/project';

/**
 * Rust tarafindaki komut adi — `src-tauri/build.rs` (ACL manifest) ve
 * `capabilities/asuna-projects-read.json` ile birebir ayni olmali.
 * Argument **almaz**: renderer ne projeyi ne de okunacak dosyayi secebilir.
 */
const PROJECT_CONTEXT_COMMAND = 'project_context';

export const GET_CURRENT_PROJECT_TOOL_NAME = 'get_current_project';

/**
 * Modele giden ozetin karakter tavani.
 *
 * Sesli bir cevabin girdisi bu; birkac cumlelik olmali. Tavan asilirsa ozet
 * **kirpilir ve bu gorunur olur** — sessiz kirpma yok.
 */
export const MAX_TOOL_SUMMARY_CHARS = 700;

/** Tek bir alintidan alinacak en fazla karakter (proje aciklamasi). */
const MAX_DESCRIPTION_CHARS = 180;

/**
 * Tool cagrisi ust siniri.
 *
 * Rust tarafi git alt komutlarina 5 sn/komut veriyor ve dort komut kosabiliyor;
 * bu tavan onun ustunde durur ki normal bir yavaslikta tool kesilmesin, ama
 * asili kalan bir cagri sesli oturumu sessizlige gomemesin.
 */
export const GET_CURRENT_PROJECT_TIMEOUT_MS = 25_000;

/** Ozette "kayitli aciklama yok" demek icin kullanilan kaynaklarin sirasi. */
const DESCRIPTION_SOURCES = ['PROJECT.md', 'README.md', 'CLAUDE.md', 'AGENTS.md'] as const;

/**
 * Belirsizlikte Asuna'ya **ne yapacagini** soyleyen satir.
 *
 * Uc neden ayri cumle: "hangi dizinde calisiyorsun?" ile "disk takili mi?" ayni
 * soru degil. Ortak nokta hepsinde ayni: proje adi uydurmak yasak.
 */
const UNKNOWN_GUIDANCE: Readonly<Record<ContextUnknownReason, string>> = {
  'no-registered-project':
    'Hic proje kayitli degil. Kullaniciya hangi dizinde calistigini sor ve Projeler sekmesinden eklemesini iste. Proje adi tahmin etme.',
  'no-current-selection':
    'Kayitli projeler var ama guncel proje secilmemis. Kullaniciya hangisinde oldugunu sor. Kendin secme, tahmin etme.',
  'root-missing':
    'Secili projenin kok dizini su an bulunamiyor (tasinmis ya da disk bagli degil). Kullaniciya bunu soyle; eski bilgiyi guncelmis gibi anlatma.',
};

function clip(text: string, limit: number): string {
  const trimmed = text.trim();
  if (trimmed.length <= limit) {
    return trimmed;
  }
  return `${trimmed.slice(0, Math.max(0, limit - 1)).trimEnd()}…`;
}

/**
 * Alintidan tek cumlelik bir proje aciklamasi cikarir.
 *
 * Markdown basliklari (`# Asuna`) atlanir: baslik projenin adidir, ne yaptigi
 * degil. Bulunamazsa `null` doner — bos bir cumle uydurulmaz.
 */
export function firstSentenceOf(excerpt: string): string | null {
  for (const rawLine of excerpt.split('\n')) {
    const line = rawLine.trim();
    if (line.length === 0 || line.startsWith('#') || line.startsWith('>')) {
      continue;
    }
    const stop = line.search(/[.!?](\s|$)/u);
    const sentence = stop === -1 ? line : line.slice(0, stop + 1);
    const cleaned = sentence.replace(/[*_`]/gu, '').trim();
    if (cleaned.length > 0) {
      return clip(cleaned, MAX_DESCRIPTION_CHARS);
    }
  }
  return null;
}

function describeGit(view: Extract<ProjectContextView, { status: 'known' }>): string {
  const { git } = view.project;

  if (!git.isRepository) {
    return 'Git: bu dizin bir git deposu degil.';
  }

  const branch = git.detached
    ? 'HEAD bir dala bagli degil (detached)'
    : (git.branch ?? 'dal adi okunamadi');
  const worktree = git.isDirty
    ? `${git.changedTrackedFiles.toString()} takip edilen dosyada kaydedilmemis degisiklik var`
    : 'calisma agaci temiz';

  return `Git: ${branch}, ${worktree}.`;
}

/**
 * Konusulabilir ozet. Saf fonksiyon — IPC'siz test edilir.
 *
 * Satir bazli ve etiketli: modelin bunu tek cumleye cevirmesi kolay, ama hangi
 * bilginin nereden geldigi kaybolmuyor.
 */
export function summariseProjectContext(view: ProjectContextView): string {
  if (view.status === 'unknown') {
    return clip(
      `Guncel proje bilinmiyor. ${view.message} ${UNKNOWN_GUIDANCE[view.reason]}`,
      MAX_TOOL_SUMMARY_CHARS,
    );
  }

  const { summary, git, handoff } = view.project;
  const lines: string[] = [
    `Proje: ${summary.name} (id: ${summary.projectId})`,
    `Yol: ${summary.path}`,
    describeGit(view),
  ];

  const stack = [summary.primaryLanguage, summary.framework].filter(
    (value): value is string => value !== null,
  );
  if (stack.length > 0) {
    lines.push(`Teknoloji: ${stack.join(' / ')}`);
  }

  const objective = handoff.status === 'loaded' ? handoff.context.objective : null;
  const description = objective ?? descriptionFromSources(summary.sources);
  lines.push(
    description === null
      ? 'Ozet: proje icin kayitli bir aciklama bulunamadi (PROJECT.md/README.md yok ya da bos).'
      : `Ozet: ${clip(description, MAX_DESCRIPTION_CHARS)}`,
  );

  if (handoff.status === 'loaded' && handoff.context.activeTask !== null) {
    lines.push(`Aktif is: ${clip(handoff.context.activeTask, MAX_DESCRIPTION_CHARS)}`);
  }

  // Eksik bilgi "tam" gibi sunulmaz (PROJECT.md Bolum 30).
  const notices: string[] = [];
  if (git.degraded) {
    notices.push('git durumu tam okunamadi');
  }
  if (handoff.status === 'ignored') {
    notices.push(handoff.message);
  }
  if (summary.budgetExhausted || view.project.truncated) {
    notices.push('proje ozeti tavana takildi ve kirpildi');
  }
  if (notices.length > 0) {
    lines.push(`Not: ${notices.join('; ')}.`);
  }

  return clip(lines.join('\n'), MAX_TOOL_SUMMARY_CHARS);
}

function descriptionFromSources(
  sources: Extract<ProjectContextView, { status: 'known' }>['project']['summary']['sources'],
): string | null {
  for (const name of DESCRIPTION_SOURCES) {
    const source = sources.find((candidate) => candidate.name === name);
    if (source === undefined) {
      continue;
    }
    const sentence = firstSentenceOf(source.excerpt);
    if (sentence !== null) {
      return sentence;
    }
  }
  return null;
}

/** Modele/UI'ya donen yapisal ozet. Secret degeri tasimaz. */
export interface CurrentProjectFacts {
  readonly projectId: string;
  readonly name: string;
  readonly path: string;
  readonly branch: string | null;
  readonly isDirty: boolean;
  /** Git durumu eksik okundu — cagiran taraf bunu gizleyemesin diye ayri alan. */
  readonly gitDegraded: boolean;
}

/**
 * Komut ciktisini tool sonucuna cevirir. Saf fonksiyon.
 *
 * `unknown` **basarili** bir sonuctur: "bilmiyorum" dogru bir cevaptir ve tool
 * hatasi degildir. `ok: false` yalnizca komut gercekten calismadiginda doner.
 */
export function toToolResult(view: ProjectContextView): ToolResult {
  const summary = summariseProjectContext(view);

  if (view.status === 'unknown') {
    return {
      ok: true,
      summary,
      data: { status: 'unknown', reason: view.reason },
    };
  }

  const facts: CurrentProjectFacts = {
    projectId: view.project.summary.projectId,
    name: view.project.summary.name,
    path: view.project.summary.path,
    branch: view.project.git.branch,
    isDirty: view.project.git.isDirty,
    gitDegraded: view.project.git.degraded,
  };

  return { ok: true, summary, data: { status: 'known', ...facts } };
}

export interface GetCurrentProjectOptions {
  /** IPC yerine sahte kaynak enjekte etmek icin (testler). */
  readonly fetchContext?: () => Promise<unknown>;
}

function defaultFetchContext(): Promise<unknown> {
  return invoke<unknown>(PROJECT_CONTEXT_COMMAND, {});
}

/**
 * Tool'u kurar.
 *
 * `risk: 0` (salt okuma) ve `requiresApproval: false`: hicbir seyi
 * degistirmiyor, hicbir sey silmiyor, kayitli kok disina cikmiyor
 * (PROJECT.md Bolum 5.4 / 17).
 */
export function createGetCurrentProjectTool(
  options: GetCurrentProjectOptions = {},
): AsunaToolDefinition {
  const fetchContext = options.fetchContext ?? defaultFetchContext;

  return {
    name: GET_CURRENT_PROJECT_TOOL_NAME,
    description:
      'Kullanicinin su an uzerinde calistigi kayitli projeyi dondurur: proje adi, yol, ' +
      'git dali ve kisa bir proje ozeti. Yalnizca kullanicinin ACIKCA kaydettigi projeleri ' +
      'gorur; guncel proje bilinmiyorsa bunu acikca soyler. "Hangi projedeyiz?", "bu proje ' +
      'ne yapiyor?", "hangi daldayim?" gibi sorularda kullan. Parametre almaz.',
    risk: 0,
    requiresApproval: false,
    timeoutMs: GET_CURRENT_PROJECT_TIMEOUT_MS,
    async execute(): Promise<ToolResult> {
      try {
        return toToolResult(parseProjectContextView(await fetchContext()));
      } catch (error) {
        // Hata yutulmaz ve basari taklit edilmez: model reddi oldugu gibi gorur.
        const message = toRegistryError(error).message;
        return {
          ok: false,
          summary:
            `Proje baglami okunamadi: ${message} ` +
            'Kullaniciya bunu oldugu gibi soyle; proje adi, dal ya da ozet tahmin etme.',
          errorKind: 'project_context_unavailable',
        };
      }
    },
  };
}

/** Varsayilan ornek — `use-asuna-session` bunu Realtime oturumuna verir. */
export const getCurrentProjectTool: AsunaToolDefinition = createGetCurrentProjectTool();

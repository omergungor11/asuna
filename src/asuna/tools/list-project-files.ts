/**
 * `list_project_files` — guncel proje icinde **dizin listeleme** (ASU-068, risk 0).
 *
 * # Neden gerekti
 *
 * `read_project_file` bir dosyayi okuyabiliyordu ama dosyanin **adini** bilmek
 * zorundaydi. Canli testte "freelancer klasorunde ne var?" sorusu cevapsiz
 * kaldi: model ya ad uyduracakti ya susacakti. Bu tool aradaki adimi kapatir.
 *
 * # Ince backchannel
 *
 * `read_project_file` ile ayni desen: burada hicbir dosya sistemi islemi yok,
 * tek is `list_project_dir` komutunu cagirmak. Kok secimi, path cozumu,
 * traversal reddi, blok listesi ve 200 girdi tavani guvenilir tarafta
 * (`src-tauri/src/projects/listing.rs` + `security::sandbox`).
 *
 * # Icerik yok, yalnizca isim
 *
 * Bu tool dosya **acmaz**. Donen sey ad, tur ve boyut. Bu yuzden `summary` ile
 * `auditSummary` arasindaki fark `read_project_file`taki kadar keskin degil;
 * yine de ayri veriliyor cunku 200 satirlik bir listeyi `tool_events`'e yazmak
 * defteri okunmaz hale getirirdi. Deftere giden tek satir: kac girdi, hangi
 * dizin.
 *
 * # Blok listesindeki dosyalar gizlenmez
 *
 * `.env` bir dizinde duruyorsa listede **gorunur** ve "okunamaz" isaretlenir.
 * Gizlemek kullaniciyi "neden gormuyor?" diye sasirtirdi; isim zaten bir
 * sizinti degil, **icerik** kapali kalmaya devam ediyor. Ozet modele bunu
 * acikca soyler ki `read_project_file` ile denemeye kalkmasin.
 */

import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

import type { AsunaToolDefinition, ToolResult } from './types';
import { isRecord } from '../../shared/contract';

/**
 * Rust tarafindaki komut adi — `src-tauri/build.rs` (ACL manifest) ve
 * `capabilities/asuna-project-dir-list.json` ile birebir ayni olmali.
 *
 * `export`: ayni komutu cagiran tool disi bir tuketici daha var (chat
 * kabugundaki dizin secici). Sabiti kopyalamasi, komut adi degistiginde iki
 * yerden birinin sessizce eskimesi demekti. **`index.ts`'ten re-export
 * EDILMEZ**: burasi bir IPC detayi, tool kayit noktasinin sozlesmesi degil.
 */
export const LIST_PROJECT_DIR_COMMAND = 'list_project_dir';

export const LIST_PROJECT_FILES_TOOL_NAME = 'list_project_files';

/**
 * Tool cagrisi ust siniri.
 *
 * Tek bir `read_dir` + girdi basina bir `metadata`. 10 sn bunun kat kat
 * ustunde; `read_project_file` ile ayni tavan, ayni gerekce.
 */
export const LIST_PROJECT_FILES_TIMEOUT_MS = 10_000;

/**
 * Yol metninin renderer tarafindaki tavani — host'un
 * `sandbox::MAX_RELATIVE_PATH_CHARS` degeriyle ayni.
 */
export const MAX_TOOL_PATH_CHARS = 4096;

/**
 * Modele giden liste metninin karakter tavani.
 *
 * `read_project_file`in kirpma butcesiyle ayni deger: "modele bir seferde ne
 * kadar metin gider" sorusunun bu repoda kabul edilmis cevabi. Host zaten 200
 * girdide kesiyor; bu tavan olagandisi uzun adlarin (yol gibi dosya adlari)
 * ciktiyi sisirmesine karsi ikinci bir kemer.
 */
export const MAX_LISTING_CHARS = 6_000;

/**
 * Arguman semasi — **tek** alan.
 *
 * `path` zorunlu ama **bos olabilir**: bos metin proje kokudur. Opsiyonel
 * yapmak yerine bos degere izin vermek bilincli — function calling semasinda
 * "alan yok" ile "bos alan" arasindaki fark modele gore belirsiz, tek bicim
 * daha ogretilebilir.
 *
 * `strictObject`: uydurulmus bir parametre (`projectId`, `recursive`, `depth`)
 * sessizce atilmaz, reddedilir.
 */
const LIST_PROJECT_FILES_PARAMETERS = z.strictObject({
  path: z.string().max(MAX_TOOL_PATH_CHARS, 'dizin yolu cok uzun'),
});

/** Girdi turu — Rust `EntryKind` aynasi. */
export type DirectoryEntryKind = 'dir' | 'file' | 'other';

const ENTRY_KINDS: readonly DirectoryEntryKind[] = ['dir', 'file', 'other'];

/** Komuttan donen tek girdi. */
export interface DirectoryEntry {
  readonly name: string;
  readonly kind: DirectoryEntryKind;
  /** Yalnizca duz dosyalarda dolu. */
  readonly sizeBytes: number | null;
  /** Asuna bu girdiyi okuyamaz (blok listesi, kacan symlink, bozuk ad). */
  readonly blocked: boolean;
}

/** Komuttan donen, dogrulanmis gorunum. */
export interface ProjectDirectoryView {
  readonly projectId: string;
  readonly projectName: string;
  /** Kok'e gore yol; kok'un kendisi icin bos metin. */
  readonly path: string;
  readonly entries: readonly DirectoryEntry[];
  /**
   * Sayilan girdi sayisi. [`scanCapped`] `true` ise bu bir **alt sinirdir**.
   */
  readonly totalEntries: number;
  readonly returnedEntries: number;
  readonly truncated: boolean;
  /**
   * Host tarama tavaninda durdu (ASU-068 / Gate 3 M2): dizinde `totalEntries`
   * kadarindan **daha fazlasi** olabilir. `truncated` ile ayni sey degil —
   * orada toplam biliniyor, burada bilinmiyor.
   */
  readonly scanCapped: boolean;
  readonly maxEntries: number;
}

/** Komuttan donen, dogrulanmis ret. */
export interface ProjectDirectoryRefusal {
  /** `traversal`, `blocklisted`, `not_a_directory`, `not_found`, ... */
  readonly code: string;
  readonly message: string;
  /** Host'un karari — renderer bunu **hesaplamaz**. */
  readonly escapeAttempt: boolean;
  readonly auditSummary: string;
}

function text(source: Record<string, unknown>, field: string): string | null {
  const value = source[field];
  return typeof value === 'string' ? value : null;
}

function parseEntry(value: unknown): DirectoryEntry | null {
  if (!isRecord(value)) {
    return null;
  }
  const name = text(value, 'name');
  const kind = value['kind'];
  const { sizeBytes, blocked } = value;

  if (
    name === null ||
    typeof kind !== 'string' ||
    !ENTRY_KINDS.includes(kind as DirectoryEntryKind) ||
    typeof blocked !== 'boolean' ||
    !(typeof sizeBytes === 'number' || sizeBytes === null || sizeBytes === undefined)
  ) {
    return null;
  }

  return {
    name,
    kind: kind as DirectoryEntryKind,
    sizeBytes: typeof sizeBytes === 'number' ? sizeBytes : null,
    blocked,
  };
}

/**
 * Komut ciktisini dogrular.
 *
 * Sozlesme ihlalinde `null` doner: eksik alanli bir gorunum modele "dizinde
 * bunlar var" diye sunulmamali — eksigi gorunmezdi.
 */
export function parseProjectDirectoryView(value: unknown): ProjectDirectoryView | null {
  if (!isRecord(value)) {
    return null;
  }

  const projectId = text(value, 'projectId');
  const projectName = text(value, 'projectName');
  const path = text(value, 'path');
  const { entries, totalEntries, returnedEntries, truncated, scanCapped, maxEntries } = value;

  if (
    projectId === null ||
    projectName === null ||
    path === null ||
    !Array.isArray(entries) ||
    typeof totalEntries !== 'number' ||
    typeof returnedEntries !== 'number' ||
    typeof truncated !== 'boolean' ||
    typeof scanCapped !== 'boolean' ||
    typeof maxEntries !== 'number'
  ) {
    return null;
  }

  const parsed: DirectoryEntry[] = [];
  for (const item of entries) {
    const entry = parseEntry(item);
    if (entry === null) {
      return null;
    }
    parsed.push(entry);
  }

  return {
    projectId,
    projectName,
    path,
    entries: parsed,
    totalEntries,
    returnedEntries,
    truncated,
    scanCapped,
    maxEntries,
  };
}

/** Hata payload'ini dogrular. Taninmayan sekilde `null` — neden uydurulmaz. */
export function parseProjectDirectoryRefusal(value: unknown): ProjectDirectoryRefusal | null {
  if (!isRecord(value)) {
    return null;
  }
  const code = text(value, 'code');
  const message = text(value, 'message');
  const auditSummary = text(value, 'auditSummary');
  const escapeAttempt = value['escapeAttempt'];

  if (code === null || message === null || typeof escapeAttempt !== 'boolean') {
    return null;
  }

  return {
    code,
    message,
    escapeAttempt,
    auditSummary: auditSummary ?? `reddedildi (${code})`,
  };
}

/** Insan diliyle boyut — `read-project-file.ts` ile ayni bicim. */
function describeSize(bytes: number): string {
  return bytes < 1024
    ? `${bytes.toString()} B`
    : `${(bytes / 1024).toFixed(1).replace('.0', '')} KB`;
}

/** Listelenen dizinin konusulabilir adi. */
function describeLocation(view: ProjectDirectoryView): string {
  return view.path.length === 0
    ? `${view.projectName} projesinin kok dizini`
    : `${view.projectName} / ${view.path}`;
}

function describeEntry(entry: DirectoryEntry): string {
  const notes: string[] = [];
  if (entry.kind === 'file' && entry.sizeBytes !== null) {
    notes.push(describeSize(entry.sizeBytes));
  }
  if (entry.blocked) {
    notes.push('OKUNAMAZ');
  }
  const suffix = notes.length > 0 ? ` (${notes.join(', ')})` : '';

  switch (entry.kind) {
    case 'dir':
      return `[dizin] ${entry.name}/${suffix}`;
    case 'file':
      return `[dosya] ${entry.name}${suffix}`;
    case 'other':
      // Symlink hedefi kaybolmus bag, soket, aygit dosyasi... Modelin bunu
      // "dosya" sanip okumaya calismasi yerine ne oldugunu bilmemesi daha durust.
      return `[diger] ${entry.name}${suffix}`;
  }
}

/**
 * Deftere ve transcript'e giden satir.
 *
 * Yalnizca kac girdi ve hangi dizin — 200 satirlik liste `tool_events`'e
 * girmez.
 */
export function auditSummaryFor(view: ProjectDirectoryView): string {
  const location = view.path.length === 0 ? '(proje koku)' : view.path;
  return `${view.returnedEntries.toString()} girdi listelendi: ${location}`;
}

/**
 * Modele giden metin: baslik + kompakt liste.
 *
 * Baslik **once** geliyor ki model listeyi okumadan once neyin eksik oldugunu
 * bilsin — kirpilmis bir listeyi "dizinde bunlar var" diye ozetlemek
 * PROJECT.md Bolum 30'un yasakladigi sey.
 */
export function modelSummaryFor(view: ProjectDirectoryView): string {
  const header: string[] = [];

  if (view.totalEntries === 0) {
    return `${describeLocation(view)} BOS — icinde hicbir dosya ya da klasor yok.`;
  }

  const counted = view.scanCapped
    ? `EN AZ ${view.totalEntries.toString()} girdi (tam sayi bilinmiyor)`
    : `${view.totalEntries.toString()} girdi`;
  header.push(`${describeLocation(view)} — ${counted}.`);
  if (view.truncated) {
    header.push(
      `DIKKAT: yalnizca ilk ${view.returnedEntries.toString()} girdi asagida. ` +
        'Tamamini gormedin; "dizinde su kadar sey var" diye kesin konusma.',
    );
  }
  if (view.scanCapped) {
    header.push(
      'NOT: dizin cok kalabalik oldugu icin sayim da yarida kesildi — toplam ' +
        'girdi sayisini BILMIYORSUN, "yaklasik su kadar" bile deme.',
    );
  }
  if (view.entries.some((entry) => entry.blocked)) {
    header.push(
      'NOT: OKUNAMAZ isaretli girdiler hassas dosya kuralina takiliyor ' +
        '(.env, anahtarlar, kimlik bilgileri). Icerikleri okunamaz; okumayi deneme.',
    );
  }
  header.push('Alt klasorlerin icerigi burada YOK; gerekiyorsa ayrica sor.');

  const body = view.entries.map(describeEntry).join('\n');
  const rendered = `${header.join('\n')}\n---\n${body}`;

  if (rendered.length <= MAX_LISTING_CHARS) {
    return rendered;
  }
  // Ikinci kemer: kirpma yine sessiz degil.
  return `${rendered.slice(0, MAX_LISTING_CHARS - 1)}\n[liste uzun oldugu icin kesildi]`;
}

/** Ret kodlarindan modele giden yonlendirme. */
function guidanceFor(refusal: ProjectDirectoryRefusal): { summary: string; errorKind: string } {
  if (refusal.escapeAttempt) {
    return {
      errorKind: 'sandbox_denied',
      summary:
        `ERISIM REDDEDILDI: ${refusal.message}. Yalnizca guncel proje kok dizinin ` +
        'ICINDEKI klasorler listelenebilir; yol proje kokune gore verilir ve proje ' +
        'kokunun kendisi icin bos metin kullanilir. Kullaniciya bunu durustce soyle, ' +
        'dizin icerigi hakkinda tahmin yurutme.',
    };
  }

  switch (refusal.code) {
    case 'blocklisted':
      return {
        errorKind: 'blocked_directory',
        summary:
          `LISTELENMEDI: ${refusal.message}. Bu dizin turu (SSH anahtarlari, kimlik ` +
          'bilgileri, secrets) Asuna icin kapalidir ve bu kural kullanici istese de ' +
          'gevsetilemez. Icerigi hakkinda tahmin yurutme.',
      };
    case 'not_a_directory':
      return {
        errorKind: 'not_a_directory',
        summary:
          `LISTELENMEDI: ${refusal.message}. Verdigin yol bir klasor degil, bir dosya. ` +
          'Icerigini gormek istiyorsan `read_project_file` kullan.',
      };
    case 'not_found':
      return {
        errorKind: 'not_found',
        summary:
          `BULUNAMADI: ${refusal.message}. Boyle bir klasor proje kokunun icinde yok. ` +
          'Dosya ya da klasor adi UYDURMA; kullaniciya bulunamadigini soyle. Proje ' +
          'kokunde ne oldugunu gormek icin `path` alanini BOS birakip tekrar cagirabilirsin.',
      };
    case 'no_current_project':
      return {
        errorKind: 'no_current_project',
        summary:
          'Guncel proje bilinmiyor, bu yuzden hangi kok icinde bakacagimi bilmiyorum. ' +
          'Kullaniciya hangi projede calistigini sor; `list_projects` ile kayitli ' +
          'projeleri gorebilirsin. Dizin icerigi uydurma.',
      };
    default:
      return {
        errorKind: refusal.code,
        summary: `Dizin listelenemedi: ${refusal.message}. Icerik uydurma.`,
      };
  }
}

export interface ListProjectFilesOptions {
  /** IPC yerine sahte kaynak enjekte etmek icin (testler). */
  readonly listDirectory?: (path: string) => Promise<unknown>;
}

function defaultListDirectory(path: string): Promise<unknown> {
  return invoke<unknown>(LIST_PROJECT_DIR_COMMAND, { path });
}

/**
 * Tool'u kurar.
 *
 * `risk: 0` (salt okuma) ve `requiresApproval: false`: hicbir sey degismiyor,
 * hicbir dosya acilmiyor, kayitli kok disina cikilmiyor (PROJECT.md Bolum
 * 5.4 / 17).
 */
export function createListProjectFilesTool(
  options: ListProjectFilesOptions = {},
): AsunaToolDefinition {
  const listDirectory = options.listDirectory ?? defaultListDirectory;

  return {
    name: LIST_PROJECT_FILES_TOOL_NAME,
    description:
      'Kullanicinin su an uzerinde calistigi projedeki BIR KLASORUN icerigini listeler: ' +
      'dosya ve klasor adlari, turleri ve boyutlari. Dosya ICERIGI donmez (onun icin ' +
      '`read_project_file`). `path` proje kok dizinine GORE verilir (ornek: "src", ' +
      '"docs/plans"); proje kokunun kendisi icin `path` alanini BOS BIRAK (""). Mutlak ' +
      'yol, "~" ve ".." ile disari cikan yollar reddedilir. SADECE TEK SEVIYE listeler — ' +
      'alt klasorlerin icerigi icin o klasoru ayrica sor. "Bu klasorde ne var?", "hangi ' +
      'dosyalar var?", "src altinda neler var?", "bu projede test dosyasi var mi?" gibi ' +
      'sorularda kullan. Dosya ya da klasor adi UYDURMA — listede olmayan bir seyi varmis ' +
      'gibi anlatma.',
    risk: 0,
    requiresApproval: false,
    timeoutMs: LIST_PROJECT_FILES_TIMEOUT_MS,
    parameters: LIST_PROJECT_FILES_PARAMETERS,
    async execute(args: unknown): Promise<ToolResult> {
      const parsed = LIST_PROJECT_FILES_PARAMETERS.safeParse(args);
      if (!parsed.success) {
        return {
          ok: false,
          summary:
            'Dizin yolu okunamadi; `path` alani gerekli (proje koku icin bos metin ver).',
          errorKind: 'invalid_arguments',
        };
      }

      let raw: unknown;
      try {
        raw = await listDirectory(parsed.data.path);
      } catch (error) {
        const refusal = parseProjectDirectoryRefusal(error);
        if (refusal === null) {
          return {
            ok: false,
            summary:
              'Dizin listelenemedi ve nedeni cozulemedi. Kullaniciya bunu oldugu gibi ' +
              'soyle; dizin icerigi hakkinda tahmin yurutme.',
            auditSummary: 'listelenemedi (unknown): komut tanimlanamayan bir hata dondu',
            errorKind: 'list_failed',
          };
        }
        const guidance = guidanceFor(refusal);
        return {
          ok: false,
          summary: guidance.summary,
          // Host'un urettigi, redaksiyondan gecmis satir.
          auditSummary: refusal.auditSummary,
          errorKind: guidance.errorKind,
        };
      }

      const view = parseProjectDirectoryView(raw);
      if (view === null) {
        return {
          ok: false,
          summary:
            'Dizin listelendi ama yanit beklenen bicimde degil; listeye guvenmiyorum. ' +
            'Kullaniciya bunu soyle, dosya adi uydurma.',
          auditSummary: 'listelenemedi (contract): yanit sozlesmeye uymuyor',
          errorKind: 'invalid_response',
        };
      }

      return {
        ok: true,
        summary: modelSummaryFor(view),
        auditSummary: auditSummaryFor(view),
        data: {
          path: view.path,
          totalEntries: view.totalEntries,
          returnedEntries: view.returnedEntries,
          truncated: view.truncated,
        },
      };
    },
  };
}

/** Varsayilan ornek — `index.ts` bunu varsayilan registry'ye kaydeder. */
export const listProjectFilesTool: AsunaToolDefinition = createListProjectFilesTool();

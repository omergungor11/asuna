/**
 * `read_project_file` — guncel proje koku icinde tek dosya okuma (ASU-051, risk 0).
 *
 * # Ince backchannel
 *
 * `get_current_project` ile ayni desen: tool renderer'da calisir, dolayisiyla
 * burada **hicbir dosya sistemi islemi yoktur**. Tek is `read_project_file`
 * komutunu cagirmak. Kok secimi, path cozumu, traversal reddi, blok listesi,
 * boyut/ikili kontrolu, kirpma ve redaksiyon guvenilir tarafta
 * (`src-tauri/src/projects/files.rs` + `security::sandbox`).
 *
 * # Renderer projeyi secemez
 *
 * Semada `path` disinda alan yok ve komut da `projectId` almiyor: hedef her
 * zaman kullanicinin sectigi guncel projedir. Modelin "hangi projeden okuyayim?"
 * diye secim yapabilmesi, kayitli kokler arasinda dolasma yuzeyi acardi.
 *
 * # Modele giden metin ile deftere giden metin ayri
 *
 * Bu tool **icerik** donduruyor: `summary` dosyanin (kirpilmis) metnini tasir,
 * cunku model sesli cevabi ondan uretecek. Ayni metnin `tool_events`'e dusmesi
 * migration 004'un acikca yasakladigi sey ("dosya icerigi audit'e girmez"), bu
 * yuzden [`ToolResult.auditSummary`] ayri veriliyor: deftere ve transcript
 * satirina yalnizca "hangi dosya, ne kadar, kirpildi mi" gider.
 *
 * # Uydurma yok
 *
 * Uc ret **ayri ayri** yansitilir ve ozet modele ne diyecegini soyler:
 *
 * - **Kacis denemesi** (`../../.ssh/...`, mutlak yol, `~`): "erisim reddedildi".
 * - **Hassas dosya** (`.env`, anahtar, credential): "bu dosya okunmaz" — bu bir
 *   kacis denemesi degil, kullanicinin kendi dosyasi icin verilmis bir kural.
 * - **Dosya yok**: "bulunamadi". Model icerik uydurmamali; ozet bunu acikca yazar.
 *
 * Ucunu tek bir "okuyamadim" kovasina indirgemek, modelin en olasi kacamagini
 * (icerik uydurmak) davet ederdi.
 */

import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

import type { AsunaToolDefinition, ToolResult } from './types';
import { isRecord } from '../../shared/contract';

/**
 * Rust tarafindaki komut adi — `src-tauri/build.rs` (ACL manifest) ve
 * `capabilities/asuna-project-file-read.json` ile birebir ayni olmali.
 */
const READ_PROJECT_FILE_COMMAND = 'read_project_file';

export const READ_PROJECT_FILE_TOOL_NAME = 'read_project_file';

/**
 * Tool cagrisi ust siniri.
 *
 * Yerel bir dosya okumasi: sandbox cozumu + en fazla 256 KiB disk okumasi.
 * 10 sn bunun kat kat ustunde; asilmasi bir aksaklik isaretidir ve sesli
 * oturumu sessizlige gommeden kesilmeli.
 */
export const READ_PROJECT_FILE_TIMEOUT_MS = 10_000;

/**
 * Yol metninin renderer tarafindaki tavani.
 *
 * Host zaten 4096 karakterde kesiyor (`sandbox::MAX_RELATIVE_PATH_CHARS`); bu
 * tavan aynisini sema seviyesinde uygular ki sacma bir girdi IPC'ye hic
 * cikmasin. Ikisi ayni buyukluk sinifinda olmali — kucuk tutmak gercek bir
 * yolu reddederdi.
 */
export const MAX_TOOL_PATH_CHARS = 4096;

/**
 * Arguman semasi — **tek** alan.
 *
 * `strictObject`: uydurulmus bir parametre (`projectId`, `maxBytes`) sessizce
 * atilmaz, reddedilir. Modelin yanlis bir zihinsel modelle devam etmesi
 * yerine hatayi gormek istiyoruz (`NO_TOOL_ARGUMENTS` ile ayni gerekce).
 */
const READ_PROJECT_FILE_PARAMETERS = z.strictObject({
  path: z
    .string()
    .min(1, 'dosya yolu bos olamaz')
    .max(MAX_TOOL_PATH_CHARS, 'dosya yolu cok uzun'),
});

/** Komuttan donen, dogrulanmis gorunum. */
export interface ProjectFileView {
  readonly projectId: string;
  readonly projectName: string;
  /** Kok'e gore yol. Mutlak yol **hicbir zaman** donmez. */
  readonly path: string;
  readonly content: string;
  readonly truncated: boolean;
  readonly redacted: boolean;
  readonly sizeBytes: number;
  readonly returnedChars: number;
  readonly maxChars: number;
}

/** Komuttan donen, dogrulanmis ret. */
export interface ProjectFileRefusal {
  /** Sabit kod: `traversal`, `blocklisted`, `not_found`, `no_current_project`... */
  readonly code: string;
  readonly message: string;
  /** Host'un karari — renderer bunu **hesaplamaz**. */
  readonly escapeAttempt: boolean;
  /** Deftere yazilacak, redaksiyondan gecmis tek satirlik ozet. */
  readonly auditSummary: string;
}

function text(source: Record<string, unknown>, field: string): string | null {
  const value = source[field];
  return typeof value === 'string' ? value : null;
}

/**
 * Komut ciktisini dogrular.
 *
 * Sozlesme ihlalinde `null` doner: "bozuk yanit" ile "dosya okundu" ayni
 * sey degil ve eksik alanli bir gorunum modele yarim icerik olarak
 * sunulmamali.
 */
export function parseProjectFileView(value: unknown): ProjectFileView | null {
  if (!isRecord(value)) {
    return null;
  }

  const projectId = text(value, 'projectId');
  const projectName = text(value, 'projectName');
  const path = text(value, 'path');
  const content = text(value, 'content');
  const { truncated, redacted, sizeBytes, returnedChars, maxChars } = value;

  if (
    projectId === null ||
    projectName === null ||
    path === null ||
    content === null ||
    typeof truncated !== 'boolean' ||
    typeof redacted !== 'boolean' ||
    typeof sizeBytes !== 'number' ||
    typeof returnedChars !== 'number' ||
    typeof maxChars !== 'number'
  ) {
    return null;
  }

  return {
    projectId,
    projectName,
    path,
    content,
    truncated,
    redacted,
    sizeBytes,
    returnedChars,
    maxChars,
  };
}

/**
 * Hata payload'ini dogrular.
 *
 * Taninmayan bir sekil geldiginde uydurma yapmiyoruz: `null` doner ve cagiran
 * "okuyamadim, nedenini bilmiyorum" der.
 */
export function parseProjectFileRefusal(value: unknown): ProjectFileRefusal | null {
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

/** Insan diliyle boyut — audit satiri ve transcript icin. */
function describeSize(bytes: number): string {
  return bytes < 1024
    ? `${bytes.toString()} B`
    : `${(bytes / 1024).toFixed(1).replace('.0', '')} KB`;
}

/**
 * Deftere ve transcript'e giden satir. **Icerik tasimaz** — yalnizca hangi
 * dosyanin okundugu, ne kadari ve neyin degistigi.
 */
export function auditSummaryFor(view: ProjectFileView): string {
  const notes: string[] = [describeSize(view.sizeBytes)];
  if (view.truncated) {
    notes.push(`ilk ${view.returnedChars.toString()} karakter, kirpildi`);
  }
  if (view.redacted) {
    notes.push('hassas degerler maskelendi');
  }
  return `${view.path} okundu (${notes.join(', ')})`;
}

/**
 * Modele giden metin: kisa bir baslik + dosyanin (kirpilmis) icerigi.
 *
 * Baslik **once** geliyor ki model icerigi okumaya baslamadan once neyin eksik
 * oldugunu bilsin: kirpilmis bir dosyayi "tamamini okudum" diye ozetlemek
 * PROJECT.md Bolum 30'un yasakladigi seydir.
 */
export function modelSummaryFor(view: ProjectFileView): string {
  const header: string[] = [`${view.path} (${describeSize(view.sizeBytes)})`];
  if (view.truncated) {
    header.push(
      `DIKKAT: dosya uzun oldugu icin yalnizca ilk ${view.returnedChars.toString()} ` +
        'karakteri asagida. Tamamini gormedin; "dosyanin tamami" gibi konusma.',
    );
  }
  if (view.redacted) {
    header.push(
      'NOT: icerikte gizli goren degerler maskelendi (<redacted>). Maskelenen ' +
        'degeri tahmin etme ve kullaniciya sorma.',
    );
  }
  return `${header.join('\n')}\n---\n${view.content}`;
}

/** Ret kodlarindan modele giden yonlendirme. */
function guidanceFor(refusal: ProjectFileRefusal): { summary: string; errorKind: string } {
  if (refusal.escapeAttempt) {
    return {
      errorKind: 'sandbox_denied',
      summary:
        `ERISIM REDDEDILDI: ${refusal.message}. Yalnizca guncel proje kok dizinin ` +
        'ICINDEKI dosyalar okunabilir; yol proje kokune gore verilir. Kullaniciya ' +
        'bunu durustce soyle, dosyanin icerigi hakkinda tahmin yurutme.',
    };
  }

  switch (refusal.code) {
    case 'blocklisted':
      return {
        errorKind: 'blocked_file',
        summary:
          `OKUNMADI: ${refusal.message}. Bu dosya turu (ortam degiskenleri, anahtarlar, ` +
          'kimlik bilgileri) Asuna icin kapalidir ve bu kural kullanici istese de ' +
          'gevsetilemez. Icerigi hakkinda tahmin yurutme.',
      };
    case 'not_found':
    case 'not_a_file':
      return {
        errorKind: 'not_found',
        summary:
          `BULUNAMADI: ${refusal.message}. Dosya proje kokunun icinde yok. Icerik ` +
          'UYDURMA; kullaniciya dosyanin bulunamadigini soyle ve dogru yolu sor.',
      };
    case 'no_current_project':
      return {
        errorKind: 'no_current_project',
        summary:
          'Guncel proje bilinmiyor, bu yuzden hangi kok icinde arayacagimi bilmiyorum. ' +
          'Kullaniciya hangi projede calistigini sor; Projeler sekmesinden secebilir. ' +
          'Dosya icerigi uydurma.',
      };
    case 'too_large':
    case 'binary':
      return {
        errorKind: refusal.code,
        summary:
          `OKUNMADI: ${refusal.message}. Bu dosya metin olarak okunamiyor. Icerigi ` +
          'hakkinda tahmin yurutme.',
      };
    default:
      return {
        errorKind: refusal.code,
        summary: `Dosya okunamadi: ${refusal.message}. Icerik uydurma.`,
      };
  }
}

export interface ReadProjectFileOptions {
  /** IPC yerine sahte kaynak enjekte etmek icin (testler). */
  readonly readFile?: (path: string) => Promise<unknown>;
}

function defaultReadFile(path: string): Promise<unknown> {
  return invoke<unknown>(READ_PROJECT_FILE_COMMAND, { path });
}

/**
 * Tool'u kurar.
 *
 * `risk: 0` (salt okuma) ve `requiresApproval: false`: hicbir seyi
 * degistirmiyor, hicbir sey silmiyor, kayitli kok disina cikmiyor
 * (PROJECT.md Bolum 5.4 / 17).
 */
export function createReadProjectFileTool(
  options: ReadProjectFileOptions = {},
): AsunaToolDefinition {
  const readFile = options.readFile ?? defaultReadFile;

  return {
    name: READ_PROJECT_FILE_TOOL_NAME,
    description:
      'Kullanicinin su an uzerinde calistigi projedeki BIR metin dosyasini okur ve icerigini ' +
      'dondurur. `path` proje kok dizinine GORE verilir (ornek: "README.md", ' +
      '"src/main.ts", "docs/plan.md"); mutlak yol, "~" ve ".." ile disari cikan yollar ' +
      'reddedilir. `.env`, SSH anahtarlari ve kimlik bilgisi dosyalari okunamaz. Uzun ' +
      'dosyalar kirpilir ve kirpildigi ciktida yazar. "README ne diyor?", "bu dosyada ne ' +
      'var?", "config nasil ayarlanmis?" gibi sorularda kullan. Dosya bulunamazsa icerik ' +
      'UYDURMA — bulunamadigini soyle.',
    risk: 0,
    requiresApproval: false,
    timeoutMs: READ_PROJECT_FILE_TIMEOUT_MS,
    parameters: READ_PROJECT_FILE_PARAMETERS,
    async execute(args: unknown): Promise<ToolResult> {
      // `executeTool` semadan gecmis degeri verir; yine de tip iddia edilmiyor.
      const parsed = READ_PROJECT_FILE_PARAMETERS.safeParse(args);
      if (!parsed.success) {
        return {
          ok: false,
          summary: 'Dosya yolu okunamadi; `path` alani gerekli.',
          errorKind: 'invalid_arguments',
        };
      }

      let raw: unknown;
      try {
        raw = await readFile(parsed.data.path);
      } catch (error) {
        const refusal = parseProjectFileRefusal(error);
        if (refusal === null) {
          // Tanimadigimiz bir hata: nedenini uydurmuyoruz.
          return {
            ok: false,
            summary:
              'Dosya okunamadi ve nedeni cozulemedi. Kullaniciya bunu oldugu gibi soyle; ' +
              'dosya icerigi hakkinda tahmin yurutme.',
            auditSummary: 'okunamadi (unknown): komut tanimlanamayan bir hata dondu',
            errorKind: 'read_failed',
          };
        }
        const guidance = guidanceFor(refusal);
        return {
          ok: false,
          summary: guidance.summary,
          // Host'un urettigi, redaksiyondan gecmis satir: yol ve icerik tasimaz.
          auditSummary: refusal.auditSummary,
          errorKind: guidance.errorKind,
        };
      }

      const view = parseProjectFileView(raw);
      if (view === null) {
        return {
          ok: false,
          summary:
            'Dosya okundu ama yanit beklenen bicimde degil; icerige guvenmiyorum. ' +
            'Kullaniciya bunu soyle, icerik uydurma.',
          auditSummary: 'okunamadi (contract): yanit sozlesmeye uymuyor',
          errorKind: 'invalid_response',
        };
      }

      return {
        ok: true,
        summary: modelSummaryFor(view),
        // Defter ve transcript **icerik gormez** (modul dokumantasyonu).
        auditSummary: auditSummaryFor(view),
        data: {
          path: view.path,
          truncated: view.truncated,
          redacted: view.redacted,
          sizeBytes: view.sizeBytes,
          returnedChars: view.returnedChars,
        },
      };
    },
  };
}

/** Varsayilan ornek — `index.ts` bunu varsayilan registry'ye kaydeder. */
export const readProjectFileTool: AsunaToolDefinition = createReadProjectFileTool();

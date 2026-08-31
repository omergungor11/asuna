/**
 * `open_project` — guncel projeyi konfigure edilmis editorde acar
 * (ASU-052, **risk 1**).
 *
 * Asuna'nin ilk yan etkili tool'u. Risk 1 = "geri alinabilir dusuk risk"
 * (PROJECT.md Bolum 5.4): bir pencere acilir, hicbir dosya degismez, kapatmak
 * kullanicinin elinde. ASU-048 matrisi geregi mevcut iki modda da **acik onay**
 * ister; onay akisi (`pendingApproval` / `approveTool`) zaten kurulu, bu tool
 * yalnizca registry'ye kaydediliyor.
 *
 * # Renderer hicbir sey secemez
 *
 * Sema **parametresiz**. Ne acilacak yol, ne editor komutu, ne proje: hepsi
 * guven sinirinin icinde cozulur (`src-tauri/src/projects/editor.rs`). Modelin
 * "hangi programi calistirayim?" diye bir parametresi olsaydi bu, adi
 * `open_project` olan bir genel komut calistiricisi olurdu — PROJECT.md
 * Bolum 18'in yasakladigi sey.
 *
 * # Reddedilince "actim" denmez
 *
 * Onay reddedildiginde `executeTool` bu tool'u **hic** cagirmaz ve modele
 * `denied` doner (ASU-048). Tool kendi hata yollarinda da ayni disiplini
 * uygular: editor bulunamadiginda ozet acikca "acilmadi" der ve neden
 * acilmadigini yazar (PROJECT.md Bolum 30'un ornek cumlesi).
 */

import { invoke } from '@tauri-apps/api/core';

import { NO_TOOL_ARGUMENTS, type AsunaToolDefinition, type ToolResult } from './types';
import { isRecord } from '../../shared/contract';

/**
 * Rust tarafindaki komut adi — `src-tauri/build.rs` (ACL manifest) ve
 * `capabilities/asuna-project-open.json` ile birebir ayni olmali.
 */
const OPEN_PROJECT_COMMAND = 'open_project';

export const OPEN_PROJECT_TOOL_NAME = 'open_project';

/**
 * Tool cagrisi ust siniri.
 *
 * Alt process **beklenmiyor**: `spawn` doner, editorun acilmasi beklenmez
 * (bir GUI editoru saatlerce acik kalir). 15 sn yalnizca kayit guncellemesi ve
 * process baslatma icin; asilmasi bir aksaklik isaretidir.
 */
export const OPEN_PROJECT_TIMEOUT_MS = 15_000;

/** Komuttan donen, dogrulanmis sonuc. */
export interface ProjectOpenOutcome {
  readonly projectId: string;
  readonly projectName: string;
  /** Calistirilan editor komutu — hangi programin acildigini gormek denetimin parcasi. */
  readonly editor: string;
  readonly openedAt: string;
}

/** Komuttan donen, dogrulanmis ret. */
export interface ProjectOpenRefusal {
  /** `editor_not_found`, `no_current_project`, `not_openable`, ... */
  readonly code: string;
  readonly message: string;
  readonly auditSummary: string;
}

function text(source: Record<string, unknown>, field: string): string | null {
  const value = source[field];
  return typeof value === 'string' ? value : null;
}

export function parseProjectOpenOutcome(value: unknown): ProjectOpenOutcome | null {
  if (!isRecord(value)) {
    return null;
  }
  const projectId = text(value, 'projectId');
  const projectName = text(value, 'projectName');
  const editor = text(value, 'editor');
  const openedAt = text(value, 'openedAt');

  if (projectId === null || projectName === null || editor === null || openedAt === null) {
    return null;
  }
  return { projectId, projectName, editor, openedAt };
}

export function parseProjectOpenRefusal(value: unknown): ProjectOpenRefusal | null {
  if (!isRecord(value)) {
    return null;
  }
  const code = text(value, 'code');
  const message = text(value, 'message');
  if (code === null || message === null) {
    return null;
  }
  return {
    code,
    message,
    auditSummary: text(value, 'auditSummary') ?? `acilmadi (${code})`,
  };
}

/** Ret kodlarindan modele giden yonlendirme. */
function guidanceFor(refusal: ProjectOpenRefusal): { summary: string; errorKind: string } {
  switch (refusal.code) {
    case 'editor_not_found':
      return {
        errorKind: 'editor_not_found',
        summary:
          `PROJE ACILMADI. Acmayi denedim ama editor komutu bulunamadi: ${refusal.message}. ` +
          'Kullaniciya bunu oldugu gibi soyle ve editor komutunu (ASUNA_EDITOR_COMMAND) ' +
          'kontrol etmesini oner. "Actim" DEME.',
      };
    case 'no_current_project':
      return {
        errorKind: 'no_current_project',
        summary:
          'PROJE ACILMADI: guncel proje secilmemis, hangi projeyi acacagimi bilmiyorum. ' +
          'Kullaniciya hangi projede calistigini sor; Projeler sekmesinden secebilir.',
      };
    case 'not_openable':
      return {
        errorKind: 'not_openable',
        summary:
          `PROJE ACILMADI: ${refusal.message}. Kullaniciya durumu oldugu gibi soyle; ` +
          'acildigini iddia etme.',
      };
    default:
      return {
        errorKind: refusal.code,
        summary: `PROJE ACILMADI: ${refusal.message}. Acildigini iddia etme.`,
      };
  }
}

export interface OpenProjectOptions {
  /** IPC yerine sahte kaynak enjekte etmek icin (testler). */
  readonly openProject?: () => Promise<unknown>;
}

function defaultOpenProject(): Promise<unknown> {
  return invoke<unknown>(OPEN_PROJECT_COMMAND, {});
}

/**
 * Tool'u kurar.
 *
 * `risk: 1` + `requiresApproval: true`. Tanim onay istegini **kendi** de
 * isaretliyor, yalnizca moda guvenmiyor: `resolveApproval` bir tanimin onay
 * talebini gevsetemez (`approval-policy.ts`), yani ileride risk 1'i otomatik
 * geciren bir mod eklense bile bu tool sorulmaya devam eder. Kullanicinin
 * ekraninda bir program acmak, sessizce olmamasi gereken bir seydir.
 */
export function createOpenProjectTool(options: OpenProjectOptions = {}): AsunaToolDefinition {
  const openProject = options.openProject ?? defaultOpenProject;

  return {
    name: OPEN_PROJECT_TOOL_NAME,
    description:
      'Kullanicinin su an uzerinde calistigi kayitli projeyi, ayarlanmis kod editorunde ' +
      '(varsayilan VS Code) acar. "Bu projeyi ac", "VS Code\'da ac", "editorde acar misin" ' +
      'gibi isteklerde kullan. Yalnizca GUNCEL projeyi acar: acilacak dizini, editoru ya da ' +
      'baska bir programi SEN SECEMEZSIN, parametre almaz. Kullanici onayi gerektirir; ' +
      'onaylanmazsa hicbir sey acilmaz ve bunu durustce soylemelisin.',
    risk: 1,
    requiresApproval: true,
    timeoutMs: OPEN_PROJECT_TIMEOUT_MS,
    parameters: NO_TOOL_ARGUMENTS,
    async execute(): Promise<ToolResult> {
      let raw: unknown;
      try {
        raw = await openProject();
      } catch (error) {
        const refusal = parseProjectOpenRefusal(error);
        if (refusal === null) {
          return {
            ok: false,
            summary:
              'PROJE ACILMADI ve nedeni cozulemedi. Kullaniciya bunu oldugu gibi soyle; ' +
              'acildigini iddia etme.',
            auditSummary: 'acilmadi (unknown): komut tanimlanamayan bir hata dondu',
            errorKind: 'open_failed',
          };
        }
        const guidance = guidanceFor(refusal);
        return {
          ok: false,
          summary: guidance.summary,
          auditSummary: refusal.auditSummary,
          errorKind: guidance.errorKind,
        };
      }

      const outcome = parseProjectOpenOutcome(raw);
      if (outcome === null) {
        // Komut hata firlatmadi ama yanit taninmiyor. "Acildi" demek bir
        // tahmin olurdu; acilmis olabilecegini soyluyoruz ve iddia etmiyoruz.
        return {
          ok: false,
          summary:
            'Editoru baslatma istegi gonderildi ama sonuc beklenen bicimde donmedi; ' +
            'acildigini dogrulayamiyorum. Kullaniciya ekranini kontrol etmesini soyle.',
          auditSummary: 'acilmadi (contract): yanit sozlesmeye uymuyor',
          errorKind: 'invalid_response',
        };
      }

      return {
        ok: true,
        summary:
          `${outcome.projectName} projesi \`${outcome.editor}\` ile acildi. ` +
          'Editorun on plana gelmesi birkac saniye surebilir.',
        auditSummary: `${outcome.projectName} projesi ${outcome.editor} ile acildi`,
        data: {
          projectId: outcome.projectId,
          editor: outcome.editor,
          openedAt: outcome.openedAt,
        },
      };
    },
  };
}

/** Varsayilan ornek — `index.ts` bunu varsayilan registry'ye kaydeder. */
export const openProjectTool: AsunaToolDefinition = createOpenProjectTool();

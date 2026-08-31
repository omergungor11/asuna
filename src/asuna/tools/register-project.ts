/**
 * `register_project` — yeni bir proje kokunu kaydeder (ASU-069, **risk 2**).
 *
 * # Bu tool sandbox'in yuzeyini genisletir
 *
 * Asuna'nin okuyabildigi alan = kayitli proje kokleri (`security.md` Bolum 2).
 * `read_project_file` ve `list_project_files` bu listeyle sinirli; dolayisiyla
 * **listeye bir satir eklemek, okunabilir alani buyutmek** demektir. Diger
 * risk 1 tool'u (`open_project`) bir pencere aciyor ve kapatilabiliyor; bu ise
 * kalici bir yetki degisikligi.
 *
 * # Neden risk 2 (Gate 3 M3)
 *
 * Ilk turda risk 1 yazilmisti: hicbir dosya degismiyor ve kayit geri
 * alinabiliyor. Ama risk seviyesi yalnizca bir etiket degil, iki mekanizmanin
 * girdisi:
 *
 * - **`ToolRegistry.register` zorlamasi.** Risk 2+ bir tanim `requiresApproval`
 *   olmadan **kayit edilemez** (`registry.ts`). Risk 1'de o koruma yok: bir gun
 *   biri asagidaki `requiresApproval: true` satirini silse, tool sessizce
 *   onaysiz calisir hale gelirdi. Risk 2 bunu derleme/acilis hatasina cevirir.
 * - **Kullaniciya gosterilen etiket.** Onay karti ve `tool_events` risk
 *   seviyesini yaziyor; "risk 1 — geri alinabilir dusuk risk", okunabilir alani
 *   **kalici** genisleten bir islem icin dogru cumle degil.
 *
 * Bugun davranis farki yok (ikisi de her modda onay ister); degisen sey
 * korumanin ayara degil **tanima** baglanmasi.
 *
 * Uc sonuc, ucu de kodda:
 *
 * 1. **Her modda onay.** Risk 2 `resolveApproval` icinde mod tablosuna
 *    **bakilmadan** onay dondurur; ayrica tanim `requiresApproval: true` diyor.
 *    Iki bagimsiz kilit.
 * 2. **Onay karti yolu gosterir.** Sema tek alanli ve alanin adi `path`;
 *    `toApprovalArgumentsPreview` bunu `path=/Users/.../proje` olarak karta
 *    yazar. Kullanici neyi onayladigini gorur, "bir seyler ekleyeyim mi?"
 *    demez (PROJECT.md Bolum 19 — visible action state).
 * 3. **Dogrulama renderer'da degil host'ta.** Asagidaki aciklama modelin ne
 *    gonderecegini sekillendirir ama hicbir sey **garanti etmez**: `~`
 *    genisletme reddi, mutlak yol sarti, var olma kontrolu, ev dizini /
 *    `~/Library` / sistem dizinleri reddi ve blok listesi
 *    `src-tauri/src/projects/registry.rs` icinde (ASU-069'da eklendi).
 *    Renderer'in gonderdigi metin ne olursa olsun karar orada veriliyor.
 *
 * # "Kaydettim" ile "gectim" ayni sey degil
 *
 * Kayit **guncel projeyi degistirmez** (`registry::add` sozlesmesi). Ozet bunu
 * modele acikca soyler ki "artik oradayiz" gibi konusmasin; gecis ayri bir
 * onaya tabi (`set_current_project`).
 */

import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

import type { AsunaToolDefinition, ToolResult } from './types';
import {
  parseProjectAddOutcome,
  toRegistryError,
  type AsunaRegistryErrorCode,
  type ProjectAddOutcome,
} from '../../shared/project';

/**
 * Rust tarafindaki komut adi — `src-tauri/build.rs` (ACL manifest) ve
 * `capabilities/asuna-projects-write.json` ile birebir ayni olmali.
 */
const PROJECT_ADD_COMMAND = 'project_add';

export const REGISTER_PROJECT_TOOL_NAME = 'register_project';

/**
 * Tool cagrisi ust siniri.
 *
 * `canonicalize` + tek bir DB transaction'i. 15 sn bunun kat kat ustunde; bir
 * ag surucusunde asili kalan `canonicalize` cagrisi sesli oturumu sessizlige
 * gommeden kesilmeli. Tool'un kendi sayaci **onaydan sonra** baslar
 * (`registry.ts`).
 */
export const REGISTER_PROJECT_TIMEOUT_MS = 15_000;

/** Host'un `registry::MAX_PATH_CHARS` degeriyle ayni tavan. */
export const MAX_TOOL_PATH_CHARS = 4096;

/**
 * Arguman semasi — **tek** alan.
 *
 * Proje **adi** bilerek yok: ad verilmezse host dizin adini kullanir
 * (`registry::add`). Modelin ad uydurabilmesi, kullanicinin onay kartinda
 * gordugu yol ile listede sonra gorecegi ad arasinda fark acardi.
 *
 * `strictObject`: uydurulmus bir parametre (`name`, `setCurrent`, `recursive`)
 * sessizce atilmaz, reddedilir.
 */
const REGISTER_PROJECT_PARAMETERS = z.strictObject({
  path: z
    .string()
    .min(1, 'proje dizini bos olamaz')
    .max(MAX_TOOL_PATH_CHARS, 'proje dizini cok uzun'),
});

/** Ret kodlarindan modele giden yonlendirme. */
function guidanceFor(
  code: AsunaRegistryErrorCode,
  message: string,
): { summary: string; errorKind: string } {
  switch (code) {
    case 'path-refused':
      return {
        errorKind: 'path_refused',
        summary:
          `KAYDEDILMEDI: yol kabul edilmedi — ${message}. Proje dizini MUTLAK bir yol ` +
          'olmali ("~" genisletilmez), sistem dizinleri, ev dizininin kendisi, ' +
          '`~/Library` ve hassas dizinler (.ssh, .aws, secrets ...) kaydedilemez. ' +
          'Kullaniciya bunu oldugu gibi soyle ve dogru yolu sor; kaydettigini IDDIA ETME.',
      };
    case 'path-not-found':
      return {
        errorKind: 'path_not_found',
        summary:
          `KAYDEDILMEDI: verilen dizin bulunamadi — ${message}. Var olmayan bir dizin ` +
          'proje olarak kaydedilemez. Kullaniciya tam yolu sor; yol UYDURMA.',
      };
    case 'not-a-directory':
      return {
        errorKind: 'not_a_directory',
        summary:
          `KAYDEDILMEDI: verilen yol bir dizin degil — ${message}. Proje koku bir ` +
          'KLASOR olmali, dosya degil.',
      };
    case 'invalid':
      return {
        errorKind: 'invalid_project',
        summary: `KAYDEDILMEDI: ${message}. Kullaniciya durumu oldugu gibi soyle.`,
      };
    case 'disabled':
      return {
        errorKind: 'disabled',
        summary:
          'KAYDEDILMEDI: kalici depolama kapali, proje kaydi tutulamiyor. Kullaniciya ' +
          'bunu soyle; kaydettigini iddia etme.',
      };
    // Kalan kodlar bu komut yolunda uretilmiyor (`not-found` / `refused` kayit
    // degil **secim** yollarina ait) ya da altyapi arizasi. Yine de tek tek
    // yaziliyorlar: `AsunaRegistryErrorCode` genisledigi gun burasi derlenmez
    // ve yeni kodun ne diyecegi unutulamaz.
    case 'not-found':
    case 'refused':
    case 'unavailable':
    case 'storage':
    case 'unknown':
      return {
        errorKind: code,
        summary: `KAYDEDILMEDI: ${message}. Kaydettigini IDDIA ETME.`,
      };
  }
}

/** Basarili sonucun modele giden metni. Saf fonksiyon. */
export function summariseOutcome(outcome: ProjectAddOutcome): string {
  const { project } = outcome;
  const where = project.path ?? 'kayitli dizin yok';

  if (outcome.status === 'already-registered') {
    return (
      `Bu dizin ZATEN KAYITLIYDI: "${project.name}" (id: ${project.id}, ${where}). ` +
      'Yeni bir kayit olusturulmadi. Kullaniciya bunu soyle; "ekledim" deme.'
    );
  }

  return (
    `"${project.name}" projesi kaydedildi (id: ${project.id}, ${where}). ` +
    'Guncel proje DEGISMEDI — hala oncekindesin. Kullanici bu projeye gecmek ' +
    'isterse `set_current_project` kullan (o da ayrica onay ister).'
  );
}

export interface RegisterProjectOptions {
  /** IPC yerine sahte kaynak enjekte etmek icin (testler). */
  readonly registerProject?: (path: string) => Promise<unknown>;
}

function defaultRegisterProject(path: string): Promise<unknown> {
  return invoke<unknown>(PROJECT_ADD_COMMAND, { path });
}

/**
 * Tool'u kurar.
 *
 * `risk: 2` + `requiresApproval: true` (modul dokumantasyonu). Islem geri
 * alinabilir (kullanici Projeler sekmesinden kaydi kaldirir) ama okunabilir
 * alani **kalici** genisletiyor; risk 2 hem `ToolRegistry.register`
 * zorlamasini devreye sokar hem karttaki etiketi dogrular.
 */
export function createRegisterProjectTool(
  options: RegisterProjectOptions = {},
): AsunaToolDefinition {
  const registerProject = options.registerProject ?? defaultRegisterProject;

  return {
    name: REGISTER_PROJECT_TOOL_NAME,
    description:
      'Yeni bir proje dizinini Asuna\'nin kayitli projelerine EKLER. Yalnizca kullanici ' +
      'ACIKCA "su klasoru projelerime ekle / kaydet" dediginde kullan — kendi basina ' +
      'proje ekleme. `path` MUTLAK bir dizin yolu olmali (ornek: ' +
      '"/Users/ad/Work/projem"); "~" genisletilmez, gorece yol kabul edilmez, dizin ' +
      'DISKTE VAR OLMALI. Ev dizininin kendisi, "~/Library", sistem dizinleri ve hassas ' +
      'dizinler (.ssh, .aws, secrets) kaydedilemez. Yolu SEN UYDURMA — kullanicidan tam ' +
      'yolu al. Bu islem Asuna\'nin okuyabildigi alani genisletir, bu yuzden kullanici ' +
      'onayi gerektirir; onaylanmazsa hicbir sey kaydedilmez ve bunu durustce ' +
      'soylemelisin. Kayit guncel projeyi DEGISTIRMEZ.',
    risk: 2,
    requiresApproval: true,
    timeoutMs: REGISTER_PROJECT_TIMEOUT_MS,
    parameters: REGISTER_PROJECT_PARAMETERS,
    async execute(args: unknown): Promise<ToolResult> {
      const parsed = REGISTER_PROJECT_PARAMETERS.safeParse(args);
      if (!parsed.success) {
        return {
          ok: false,
          summary: 'Proje dizini okunamadi; `path` alani gerekli (mutlak dizin yolu).',
          errorKind: 'invalid_arguments',
        };
      }

      let raw: unknown;
      try {
        raw = await registerProject(parsed.data.path);
      } catch (error) {
        const registryError = toRegistryError(error);
        const guidance = guidanceFor(registryError.code, registryError.message);
        return {
          ok: false,
          summary: guidance.summary,
          // **Sonuc** ozeti yol tasimaz; yalnizca ret kodu. Argumandaki yol
          // ayri bir alan (`tool_events.arguments_redacted`) ve oraya host
          // tarafinda redakte edilerek YAZILIR — denetim icin gerekli, cunku
          // "hangi dizin kaydedilmek istendi?" sorusunun cevabi odur.
          auditSummary: `kaydedilmedi (${registryError.code})`,
          errorKind: guidance.errorKind,
        };
      }

      let outcome: ProjectAddOutcome;
      try {
        outcome = parseProjectAddOutcome(raw);
      } catch {
        // Komut hata firlatmadi ama yanit taninmiyor: "kaydettim" demek bir
        // tahmin olurdu (`open-project.ts` ile ayni disiplin).
        return {
          ok: false,
          summary:
            'Kayit istegi gonderildi ama sonuc beklenen bicimde donmedi; kaydedildigini ' +
            'dogrulayamiyorum. Kullaniciya Projeler sekmesinden kontrol etmesini soyle.',
          auditSummary: 'kaydedilmedi (contract): yanit sozlesmeye uymuyor',
          errorKind: 'invalid_response',
        };
      }

      const registered = outcome.status === 'registered';
      return {
        ok: true,
        summary: summariseOutcome(outcome),
        auditSummary: registered
          ? `proje kaydedildi: ${outcome.project.name}`
          : `proje zaten kayitliydi: ${outcome.project.name}`,
        data: {
          status: outcome.status,
          projectId: outcome.project.id,
          name: outcome.project.name,
        },
      };
    },
  };
}

/** Varsayilan ornek — `index.ts` bunu varsayilan registry'ye kaydeder. */
export const registerProjectTool: AsunaToolDefinition = createRegisterProjectTool();

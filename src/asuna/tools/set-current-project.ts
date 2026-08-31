/**
 * `set_current_project` — guncel proje secimini degistirir (ASU-070, **risk 1**).
 *
 * # Neden onayli
 *
 * "Guncel proje" tek bir etiket degil; `read_project_file`, `list_project_files`
 * ve `open_project`in **hedefi**. Secimi degistirmek, sonraki her dosya
 * cagrisinin baska bir kok'e gitmesi demek. Kullanicinin ekraninda gorunen
 * proje ile Asuna'nin okudugu proje sessizce ayrilmamali — bu yuzden tanim
 * `requiresApproval: true` (mod gevsetemez, `approval-policy.ts`).
 *
 * # Model kimlik bilmez, ad bilir
 *
 * Kullanici "freelancer'a gec" der; modelin elinde `freelancer` kimligi yoktur.
 * Bu yuzden tool once `project_list`i cagirip **adi cozer**, sonra
 * `project_set_current`i kimlikle cagirir. Iki bilincli karar:
 *
 * - **Tam eslesme.** Buyuk/kucuk harf yok sayilir ama "kismi eslesme" yok:
 *   `pro` yazip `proje-a`ya gecmek, modelin yanlis projeye gecip dosya okumasi
 *   demek olurdu.
 * - **Birden cok aday = hata.** Iki proje ayni adi tasiyorsa tool **secmez**;
 *   adaylari listeler ve model kullaniciya sorar. "Herhalde bunu kastetti"
 *   diye secmek, PROJECT.md Bolum 30'un yasakladigi tahmin.
 *
 * # Ince backchannel
 *
 * Iki IPC cagrisi, sifir yerel is. Cozum ve dogrulama host'ta: yolu olmayan
 * bir etiket (`unlinked`) ya da kok'u kaybolmus bir proje guncel yapilamaz
 * (`registry::set_current`) ve o ret oldugu gibi modele tasinir.
 */

import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

import type { AsunaToolDefinition, ToolResult } from './types';
import {
  parseProjectRecord,
  parseProjectRecords,
  toRegistryError,
  type AsunaRegistryErrorCode,
  type ProjectRecord,
} from '../../shared/project';

/** Rust tarafindaki komut adlari — `build.rs` ACL manifest'i ile birebir ayni. */
const PROJECT_LIST_COMMAND = 'project_list';
const PROJECT_SET_CURRENT_COMMAND = 'project_set_current';

export const SET_CURRENT_PROJECT_TOOL_NAME = 'set_current_project';

/**
 * Tool cagrisi ust siniri.
 *
 * Iki komut: kayitli kokleri tazeleyen bir liste + tek satirlik bir guncelleme.
 * 15 sn ikisinin toplamının kat kat ustunde. Sayac **onaydan sonra** baslar
 * (`registry.ts`).
 */
export const SET_CURRENT_PROJECT_TIMEOUT_MS = 15_000;

/** Proje adinin ust siniri — host'un `registry::MAX_NAME_CHARS` degeriyle ayni. */
export const MAX_PROJECT_REFERENCE_CHARS = 120;

/** Belirsizlikte modele gosterilecek en fazla aday/secenek. */
export const MAX_CANDIDATES_SHOWN = 15;

/**
 * Arguman semasi — **tek** alan.
 *
 * `project` hem ad hem kimlik kabul eder: model konusmada adi duyar, ama
 * `list_projects` ciktisinda kimligi de gormustur. Iki ayri alan acmak
 * ("name" / "projectId") modelin ikisini birden doldurup celiskiye dusmesine
 * acik olurdu.
 */
const SET_CURRENT_PROJECT_PARAMETERS = z.strictObject({
  project: z
    .string()
    .min(1, 'proje adi bos olamaz')
    .max(MAX_PROJECT_REFERENCE_CHARS, 'proje adi cok uzun'),
});

/**
 * Karsilastirma icin normalize eder.
 *
 * Turkce yerel kucultme bilincli: urunun konusma dili Turkce ve "İstanbul" gibi
 * bir ad ile modelin yazdigi "istanbul" ancak ayni donusum iki tarafa da
 * uygulanirsa eslesir.
 */
function normalise(value: string): string {
  return value.trim().toLocaleLowerCase('tr');
}

/**
 * Ada ya da kimlige gore eslesen kayitlar. Saf fonksiyon.
 *
 * # Kimlik eslesmesi belirsizligi **yutmaz** (Gate 3 H1)
 *
 * Ilk surumde kimlik eslesmesi bulununca hemen tek kayit donuluyordu ve gerekce
 * "kimlikler benzersizdir" idi. Benzersizlik dogru ama **yetersiz**: kimlikler
 * adlarin slug'i olarak uretiliyor (`registry::add` → `slugify`), yani ad ve
 * kimlik ayri isim uzaylari degil. Somut vaka:
 *
 * ```text
 * { id: 'freelancer',   name: 'freelancer' }
 * { id: 'freelancer-2', name: 'Freelancer' }
 * ```
 *
 * Kullanici "freelancer'a gec" dediginde kimlik eslesmesi **tek** aday
 * donuyordu; oysa ada gore **iki** aday var ve dogru cevap "hangisini
 * kastettin?". Sessizce birini secmek, sonraki her dosya cagrisinin yanlis
 * kokte calismasi demekti.
 *
 * Yeni kural: iki kume de hesaplanir. Kimlik eslesmesi disinda **baska** bir ad
 * eslesmesi varsa sonuc birlestirilir ve cagiran taraf bunu belirsizlik olarak
 * gorur. Kimlik eslesmesi hep **basta** durur: kullanici gercekten kimlik
 * verdiyse aday listesinde ilk sirada gorunmeli.
 */
export function matchProjects(
  projects: readonly ProjectRecord[],
  reference: string,
): readonly ProjectRecord[] {
  const needle = normalise(reference);

  const byName = projects.filter((project) => normalise(project.name) === needle);
  const byId = projects.find((project) => normalise(project.id) === needle);

  if (byId === undefined) {
    return byName;
  }

  const otherNameMatches = byName.filter((project) => project.id !== byId.id);
  return otherNameMatches.length === 0 ? [byId] : [byId, ...otherNameMatches];
}

/** Modele gosterilecek aday listesi — kirpma sessiz degil. */
function describeCandidates(projects: readonly ProjectRecord[]): string {
  if (projects.length === 0) {
    return 'Kayitli hicbir proje yok.';
  }

  const shown = projects.slice(0, MAX_CANDIDATES_SHOWN);
  const names = shown.map((project) => `"${project.name}" (id: ${project.id})`).join(', ');
  return projects.length > shown.length
    ? `${names} (ve ${(projects.length - shown.length).toString()} tane daha)`
    : names;
}

/** Ret kodlarindan modele giden yonlendirme. */
function guidanceFor(
  code: AsunaRegistryErrorCode,
  message: string,
): { summary: string; errorKind: string } {
  switch (code) {
    case 'refused':
      return {
        errorKind: 'not_switchable',
        summary:
          `PROJE DEGISMEDI: ${message}. Bu kayit ya yalnizca bir hafiza etiketi (kayitli ` +
          'dizini yok) ya da kok dizini su an bulunamiyor. Kullaniciya bunu oldugu gibi ' +
          'soyle; gectigini IDDIA ETME.',
      };
    case 'not-found':
      return {
        errorKind: 'project_not_found',
        summary:
          `PROJE DEGISMEDI: ${message}. Kullaniciya hangi projeyi kastettigini sor; ` +
          '`list_projects` ile kayitli projeleri gorebilirsin.',
      };
    case 'disabled':
      return {
        errorKind: 'disabled',
        summary:
          'PROJE DEGISMEDI: kalici depolama kapali, secim kaydedilemiyor. Kullaniciya ' +
          'bunu soyle; gectigini iddia etme.',
      };
    // Kalan kodlar bu komut yolunda uretilmiyor (yol dogrulamasi `project_add`
    // isi) ya da altyapi arizasi. Tek tek yaziliyorlar ki
    // `AsunaRegistryErrorCode` genisledigi gun burasi derlenmesin.
    case 'invalid':
    case 'path-refused':
    case 'path-not-found':
    case 'not-a-directory':
    case 'unavailable':
    case 'storage':
    case 'unknown':
      return {
        errorKind: code,
        summary: `PROJE DEGISMEDI: ${message}. Gectigini IDDIA ETME.`,
      };
  }
}

export interface SetCurrentProjectOptions {
  /** IPC yerine sahte kaynak enjekte etmek icin (testler). */
  readonly listProjects?: () => Promise<unknown>;
  readonly setCurrentProject?: (projectId: string) => Promise<unknown>;
}

function defaultListProjects(): Promise<unknown> {
  return invoke<unknown>(PROJECT_LIST_COMMAND, {});
}

function defaultSetCurrentProject(projectId: string): Promise<unknown> {
  return invoke<unknown>(PROJECT_SET_CURRENT_COMMAND, { projectId });
}

/**
 * Tool'u kurar.
 *
 * `risk: 1` + `requiresApproval: true`. Hicbir dosya degismiyor ve secim geri
 * alinabilir (kullanici Projeler sekmesinden geri secer), ama sonraki her
 * dosya cagrisinin hedefi degisiyor — sessizce olmamali.
 */
export function createSetCurrentProjectTool(
  options: SetCurrentProjectOptions = {},
): AsunaToolDefinition {
  const listProjects = options.listProjects ?? defaultListProjects;
  const setCurrentProject = options.setCurrentProject ?? defaultSetCurrentProject;

  return {
    name: SET_CURRENT_PROJECT_TOOL_NAME,
    description:
      'Asuna\'nin uzerinde calistigi GUNCEL PROJEYI degistirir. `project` alanina ' +
      'kullanicinin soyledigi proje ADINI (ya da `list_projects` ciktisindaki kimligi) ' +
      'yaz — tam ad gerekir, kismi eslesme yoktur. "Freelancer projesine gec", "artik X ' +
      'projesinde calisiyorum", "projeyi degistir" gibi isteklerde kullan. Proje ONCE ' +
      'KAYITLI OLMALI: bulunamazsa `list_projects` ile mevcut projeleri gor ve kullaniciya ' +
      'sor, proje UYDURMA. Ayni adda birden fazla proje varsa secim yapmam — kullaniciya ' +
      'hangisini kastettigini sor. Bu secim sonraki tum dosya islemlerinin hedefini ' +
      'degistirir, bu yuzden kullanici onayi gerektirir; onaylanmazsa proje DEGISMEZ ve ' +
      'bunu durustce soylemelisin.',
    risk: 1,
    requiresApproval: true,
    timeoutMs: SET_CURRENT_PROJECT_TIMEOUT_MS,
    parameters: SET_CURRENT_PROJECT_PARAMETERS,
    async execute(args: unknown): Promise<ToolResult> {
      const parsed = SET_CURRENT_PROJECT_PARAMETERS.safeParse(args);
      if (!parsed.success) {
        return {
          ok: false,
          summary: 'Proje adi okunamadi; `project` alani gerekli.',
          errorKind: 'invalid_arguments',
        };
      }
      const reference = parsed.data.project;

      let projects: readonly ProjectRecord[];
      try {
        projects = parseProjectRecords(await listProjects());
      } catch (error) {
        const message = toRegistryError(error).message;
        return {
          ok: false,
          summary:
            `PROJE DEGISMEDI: kayitli proje listesi okunamadi (${message}), bu yuzden ` +
            'hangi projeye gececegimi cozemedim. Kullaniciya bunu oldugu gibi soyle.',
          auditSummary: 'proje degismedi (list_failed)',
          errorKind: 'project_list_unavailable',
        };
      }

      const matches = matchProjects(projects, reference);

      if (matches.length === 0) {
        return {
          ok: false,
          summary:
            `PROJE DEGISMEDI: "${reference}" adinda kayitli bir proje yok. ` +
            `Kayitli projeler: ${describeCandidates(projects)}. Kullaniciya hangisini ` +
            'kastettigini sor; proje UYDURMA ve kendi kafana gore secme.',
          auditSummary: 'proje degismedi (not_found): eslesen kayit yok',
          errorKind: 'project_not_found',
        };
      }

      if (matches.length > 1) {
        return {
          ok: false,
          summary:
            `PROJE DEGISMEDI: "${reference}" adiyla ${matches.length.toString()} kayit ` +
            `eslesiyor: ${describeCandidates(matches)}. Hangisini kastettigini ` +
            'KULLANICIYA SOR; sonra kimligi (id) vererek tekrar dene.',
          auditSummary: `proje degismedi (ambiguous): ${matches.length.toString()} aday`,
          errorKind: 'ambiguous_project',
        };
      }

      // `matches.length === 1` — dizinin ilk elemani var.
      const target = matches[0];
      if (target === undefined) {
        return {
          ok: false,
          summary: 'PROJE DEGISMEDI: eslesme cozulemedi. Gectigini iddia etme.',
          auditSummary: 'proje degismedi (internal)',
          errorKind: 'match_failed',
        };
      }

      let raw: unknown;
      try {
        raw = await setCurrentProject(target.id);
      } catch (error) {
        const registryError = toRegistryError(error);
        const guidance = guidanceFor(registryError.code, registryError.message);
        return {
          ok: false,
          summary: guidance.summary,
          auditSummary: `proje degismedi (${registryError.code})`,
          errorKind: guidance.errorKind,
        };
      }

      let project: ProjectRecord;
      try {
        project = parseProjectRecord(raw);
      } catch {
        return {
          ok: false,
          summary:
            'Proje degistirme istegi gonderildi ama sonuc beklenen bicimde donmedi; ' +
            'gectigimi dogrulayamiyorum. Kullaniciya Projeler sekmesinden kontrol ' +
            'etmesini soyle.',
          auditSummary: 'proje degismedi (contract): yanit sozlesmeye uymuyor',
          errorKind: 'invalid_response',
        };
      }

      return {
        ok: true,
        summary:
          `Guncel proje artik "${project.name}" (id: ${project.id}). Bundan sonraki ` +
          'dosya ve dizin islemleri bu projenin kok dizininde yapilacak.',
        auditSummary: `guncel proje degisti: ${project.name}`,
        data: { projectId: project.id, name: project.name, status: project.status },
      };
    },
  };
}

/** Varsayilan ornek — `index.ts` bunu varsayilan registry'ye kaydeder. */
export const setCurrentProjectTool: AsunaToolDefinition = createSetCurrentProjectTool();

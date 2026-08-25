/**
 * Onay politikasi (ASU-048, PROJECT.md Bolum 5.4 / `security.md` Bolum 3).
 *
 * Bu modul **saf**tir: IPC yok, SDK yok, zaman yok. Tek isi "bu cagri onay
 * ister mi?" sorusuna deterministik cevap vermek ve verilen cevabi audit
 * defterinin diline (`approval_state`) cevirmek. Uygulama iki katmanda:
 *
 * 1. **SDK katmani** — `realtime-service.ts` `toSdkTool` icinde `needsApproval`
 *    bu fonksiyonu cagirir. `needs_approval` donen bir tool'da SDK `execute`'u
 *    **hic** cagirmaz; once `tool_approval_requested` cikar, karar
 *    `session.approve/reject` ile verilir.
 * 2. **Calistirma kapisi** — `executeTool` ayni kararı bagimsiz olarak yeniden
 *    hesaplar ve onay kanitini `approvalGate` ile sorar. Ayni kurali iki kez
 *    uygulamak israf degil savunma: politika fonksiyonu SDK'ya yanlis
 *    baglanirsa (ya da bir cagiran registry'yi atlarsa) tool yine calismaz.
 *
 * # Varsayilan: CALISTIRMA
 *
 * Belirsizlik onay lehine cozulur (phase-5.md ASU-048). Onay kanali yoksa,
 * onay cevabi gelmezse ya da bir hata olusursa sonuc **reddetmektir** —
 * "herhalde sorun yoktur" diye calistirmak yok.
 *
 * # Karar matrisi
 *
 * | Risk | `requiresApproval` | `safe` | `always` |
 * |------|--------------------|--------|----------|
 * | 0    | `false`            | onaysiz | onaysiz |
 * | 0    | `true`             | ONAY    | ONAY    |
 * | 1    | herhangi           | ONAY    | ONAY    |
 * | 2    | herhangi           | ONAY    | ONAY    |
 * | 3    | herhangi           | ONAY    | ONAY    |
 *
 * Uc karar bilinçli ve gerekcesi burada durur:
 *
 * - **Risk 2/3 mod'a bakmaz.** `ASUNA_TOOL_APPROVAL_MODE` bu satirlari
 *   gevsetemez (`conventions.md`, `security.md` Bolum 3). Bu yuzden mod
 *   tablosuna bakilmadan **once** donulur — ileride bir mod eklendiginde
 *   yanlislikla bu satira dokunmasi mumkun olmasin.
 * - **`always` modunda risk 0 da onay ISTEMEZ.** "Her cagriya sor" demek,
 *   sesli bir oturumda `get_current_project` gibi salt-okuma bir cagri icin de
 *   kart cikarmak demekti; onay yorgunlugu asil onemli olan risk 2/3 kartinin
 *   da refleksle onaylanmasina yol acar. Kabul kriteri de kosulsuz yazili:
 *   "Risk 0: onaysiz calisiyor". `always`'in anlami bu yuzden "risk 0'i da sor"
 *   degil, **kilit**: ileride gevsetici bir mod eklense bile `always` hicbir
 *   riski otomatik gecirmez.
 * - **Mevcut iki mod risk 1'de ayni davranir.** `safe` icin phase-5.md acikca
 *   "safe modda onay ister" diyor; `always` ondan daha gevsek olamaz. Yani
 *   bugun `safe` ve `always` kayitli risk seviyelerinde ayni sonucu uretir ve
 *   bu **dokumante edilmis** bir durumdur, kesfedilecek bir surpriz degil.
 *   Fark, gevsetici bir mod (`auto`/`trusted`, risk 1'i otomatik geciren)
 *   eklendiginde ortaya cikar: o zaman degisecek tek yer
 *   [`RISK_1_NEEDS_APPROVAL`] tablosudur ve tablo `Record<ToolApprovalMode, …>`
 *   oldugu icin yeni bir mod eklemek burada derleme hatasi verir.
 */

import type { ToolRisk } from './types';
import type { ToolApprovalMode } from '../config/frontend-config';
import type { ToolApprovalState } from '../../shared/tool-event';

/**
 * Politikanin cevabi. Bilerek iki degerli: "onay gerekmiyor" ile "onay lazim".
 * Onayin **sonucu** ayri bir sey ([`ApprovalOutcome`]) — kararla sonucu ayni
 * tipte tasimak, "sorulmadi" ile "soruldu ve onaylandi"yi karistirmaya davetiye
 * olurdu (audit'te bu ikisi farkli satir).
 */
export type ApprovalDecision = 'not_required' | 'needs_approval';

/**
 * Onay istegi nasil sonuclandi.
 *
 * `timeout` bir hata degil bir **karardir**: sure dolduysa cevap "hayir"dir
 * (phase-5.md ASU-048 — "Onay zaman asimina ugrarsa tool calismiyor").
 */
export type ApprovalOutcome = 'approved' | 'denied' | 'timeout';

/**
 * Onay istegi icin ust sinir.
 *
 * Sabit ve konfigurabilir degil: bu bir gorsel tercih degil, guvenlik
 * varsayilani. 60 sn, sesli bir oturumda kullanicinin karti gorup karar
 * vermesine yeter; daha uzunu "bekleyen onay" ile "unutulmus onay" arasindaki
 * farki silerdi. Sure dolunca sonuc **reddetmektir**.
 */
export const APPROVAL_TIMEOUT_MS = 60_000;

/**
 * Risk 1'in mod'a gore karari.
 *
 * `Record<ToolApprovalMode, boolean>`: `TOOL_APPROVAL_MODES` kumesine yeni bir
 * mod eklendiginde burasi derlenmez — yeni modun risk 1'de ne yapacagi
 * **unutulamaz**. Risk 2/3 bu tabloya hic ugramaz (mod'la gevsetilemez).
 */
const RISK_1_NEEDS_APPROVAL: Readonly<Record<ToolApprovalMode, boolean>> = {
  /** phase-5.md ASU-048: "Risk 1 ... safe modda onay ister". */
  safe: true,
  /** `always` hicbir kosulda `safe`'ten gevsek olamaz. */
  always: true,
};

/**
 * Bu cagri icin onay gerekiyor mu?
 *
 * Modulun basindaki matrisin tek uygulamasi. Saf ve senkron: karar zamana,
 * aga ya da kullaniciya bagli degil — onay **istegi** oyle, karar degil.
 *
 * @param risk Tool tanimindaki risk seviyesi (PROJECT.md Bolum 5.4).
 * @param requiresApproval Tanimin kendi talebi. `true` ise mod'a bakilmadan
 *   onay istenir: bir tool yazari "beni her zaman sor" diyebilir, tersini
 *   diyemez.
 * @param mode `ASUNA_TOOL_APPROVAL_MODE`.
 */
export function resolveApproval(
  risk: ToolRisk,
  requiresApproval: boolean,
  mode: ToolApprovalMode,
): ApprovalDecision {
  // Risk 2/3: mod tablosuna **bakilmadan** onay. Bu satirin ustunde bir
  // konfigurasyon kontrolu olmamasi bilincli.
  if (risk >= 2) {
    return 'needs_approval';
  }

  // Tanimin kendi talebi mod'u gecer: gevsetme degil sikilastirma yonunde.
  if (requiresApproval) {
    return 'needs_approval';
  }

  if (risk === 1) {
    return RISK_1_NEEDS_APPROVAL[mode] ? 'needs_approval' : 'not_required';
  }

  // Risk 0 + tanim onay istemiyor: iki modda da onaysiz calisir.
  return 'not_required';
}

/**
 * Karar + sonuc -> audit defterinin `approval_state` degeri (ASU-050).
 *
 * Ayrimlar sema yorumlariyla ayni (`shared/tool-event.ts`):
 *
 * - `not_required`: onay **gerekmedi** (risk 0).
 * - `auto_approved`: onay gerekebilirdi ama ayar izin verdi. Mevcut mod
 *   kumesiyle bu yola **girilmez** (risk 1 iki modda da onay ister); yine de
 *   tanimli, cunku gevsetici bir mod eklendiginde audit satirinin etiketi
 *   `not_required` ("gerekmiyordu") diye yalan soylememeli.
 * - `not_requested`: onay gerekiyordu ama **sorulamadi** (onay kanali yok).
 *   Cagri calismadi.
 * - `approved` / `denied` / `timeout`: soruldu ve cevaplandi.
 *
 * @param outcome Onay istegi sonucu; `null` = istek hic yapilamadi.
 */
export function approvalStateFor(
  risk: ToolRisk,
  decision: ApprovalDecision,
  outcome: ApprovalOutcome | null,
): ToolApprovalState {
  if (decision === 'not_required') {
    return risk === 0 ? 'not_required' : 'auto_approved';
  }
  return outcome ?? 'not_requested';
}

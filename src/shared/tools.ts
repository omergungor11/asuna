/**
 * Tool katmaninin UI'a acilan **ozet** sozlesmesi (ASU-054).
 *
 * `src/asuna/tools/` icindeki tanimlar sema + calistirici tasir; UI'nin buna
 * ihtiyaci yok ve sinir kurali geregi (`components/` → servis → registry)
 * oraya dokunmaz. Bu dosya renderer'in "hangi tool'lar var, hangisi acik?"
 * sorusuna aldigi cevabin sekli. `useAsunaSession()` bunu `tools` alaninda
 * doner, `setToolEnabled()` ile degistirir.
 *
 * Kapatma **oturum-yereldir** (bellekte): kalici ayar degil, PROJECT.md
 * Bolum 19 "kullanici tool'u tek tek kapatabilmeli" maddesinin MVP karsiligi.
 */

import type { ToolRiskLevel } from './tool-event';

/** Tool'un onay politikasi — ASU-048 matrisinin insan-dili karsiligi. */
export type ToolApprovalPolicy =
  /** Risk 0/1 safe mod disinda: onaysiz calisir. */
  | 'not_required'
  /** Her cagrida kullanici onayi istenir. */
  | 'always';

export interface ToolSummary {
  /** Registry'deki benzersiz ad (`get_current_project`, `read_project_file`...). */
  readonly name: string;
  /** Modele verilen aciklama — UI'da da ayni metin gosterilir, ikinci bir metin tutulmaz. */
  readonly description: string;
  readonly risk: ToolRiskLevel;
  readonly approval: ToolApprovalPolicy;
  /** `false` = kullanici bu oturum icin kapatti; model listeden gormez, cagri reddedilir. */
  readonly enabled: boolean;
}

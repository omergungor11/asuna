/**
 * Modele acilan tool'lar (PROJECT.md Bolum 17).
 *
 * Bu dosya **kayit noktasidir**: "hangi yetenekler modele acik?" sorusunun tek
 * cevabi burada kurulan [`ToolRegistry`] ornegidir. Calistirma kurallari
 * (sema, onay kapisi, timeout, yapisal sonuc, audit) `registry.ts`'te; onay
 * matrisi `approval-policy.ts`'te; `tool_events` yazimi `audit.ts`'te.
 *
 * MVP kurali (PROJECT.md Bolum 17): **once salt okuma**. Risk 2+ bir tool
 * eklemek orchestrator karari; bu listeye sessizce eklenmez — registry zaten
 * onaysiz risk 2/3 tanimini kayit aninda reddeder.
 *
 * Acma/kapama (ASU-054) burada degil: hangi tool'larin **kayitli** oldugu
 * uygulama karari, hangilerinin **acik** oldugu kullanicinin oturum-yerel
 * karari (`tool-toggles.ts`).
 */

import { getCurrentProjectTool } from './get-current-project';
import { listProjectFilesTool } from './list-project-files';
import { listProjectsTool } from './list-projects';
import { openProjectTool } from './open-project';
import { readProjectFileTool } from './read-project-file';
import { registerProjectTool } from './register-project';
import { ToolRegistry } from './registry';
import { setCurrentProjectTool } from './set-current-project';

export {
  APPROVAL_TIMEOUT_MS,
  approvalStateFor,
  resolveApproval,
  type ApprovalDecision,
  type ApprovalOutcome,
} from './approval-policy';
export { createGetCurrentProjectTool, getCurrentProjectTool } from './get-current-project';
export {
  createListProjectFilesTool,
  listProjectFilesTool,
  LIST_PROJECT_FILES_TOOL_NAME,
} from './list-project-files';
export {
  createListProjectsTool,
  listProjectsTool,
  pickCurrentProjectId,
  LIST_PROJECTS_TOOL_NAME,
} from './list-projects';
export { createOpenProjectTool, openProjectTool, OPEN_PROJECT_TOOL_NAME } from './open-project';
export {
  createRegisterProjectTool,
  registerProjectTool,
  REGISTER_PROJECT_TOOL_NAME,
} from './register-project';
export {
  createSetCurrentProjectTool,
  setCurrentProjectTool,
  SET_CURRENT_PROJECT_TOOL_NAME,
} from './set-current-project';
export {
  createReadProjectFileTool,
  readProjectFileTool,
  READ_PROJECT_FILE_TOOL_NAME,
} from './read-project-file';
export {
  executeTool,
  MAX_TOOL_TIMEOUT_MS,
  TOOL_ERROR_KINDS,
  ToolRegistry,
  ToolRegistryError,
  type ToolApprovalGate,
  type ToolExecutionOptions,
  type ToolResultReport,
  type ToolRegistryErrorCode,
} from './registry';
export { approvalPolicyFor, buildToolSummaries, ToolToggleStore } from './tool-toggles';
export { NO_TOOL_ARGUMENTS } from './types';
export type {
  AsunaToolDefinition,
  ToolContext,
  ToolInputSchema,
  ToolResult,
  ToolRisk,
} from './types';

/**
 * Varsayilan tool setini kaydeder.
 *
 * Fabrika olarak duruyor cunku testler (ve ileride farkli oturum profilleri)
 * paylasilan bir ornegi kirletmeden kendi registry'sini kurabilmeli.
 */
export function createAsunaToolRegistry(): ToolRegistry {
  const registry = new ToolRegistry();
  registry.register(getCurrentProjectTool);
  // ASU-067: risk 0, salt okuma. `project_list`i sarar — yeni bir Rust yuzeyi
  // acmaz. "Hangi projelerim var?" sorusunun tek cevabi.
  registry.register(listProjectsTool);
  // ASU-051: risk 0, salt okuma. Sandbox + blok listesi Rust tarafinda.
  registry.register(readProjectFileTool);
  // ASU-068: risk 0, salt okuma. Dosya **acmaz**, yalnizca ad/tur/boyut doner;
  // ozyineleme yok, 200 girdi tavani (Rust tarafinda).
  registry.register(listProjectFilesTool);
  // ASU-052: risk 1, **onay ister**. Asuna'nin ilk yan etkili yetenegi;
  // registry onaysiz bir risk 1 tanimini zaten kabul etmezdi ama tanim da
  // kendisi `requiresApproval: true` diyor (gevsetici bir mod eklenirse bile
  // sorulmaya devam etsin).
  registry.register(openProjectTool);
  // ASU-069: **risk 2**, onay ister (Gate 3 M3). Diger yan etkili tool'lardan
  // farki: bu tool sandbox'in YUZEYINI kalici olarak genisletir (kayitli kok =
  // okunabilir alan). Risk 2 olmasi `register`in zorlamasini devreye sokuyor —
  // onay talebi tanimdan silinse tool acilista reddedilir. Yol dogrulamasi
  // renderer'da degil `projects::registry` icinde.
  registry.register(registerProjectTool);
  // ASU-070: risk 1, **onay ister**. Guncel proje sonraki her dosya cagrisinin
  // hedefi; sessizce kaymamali.
  registry.register(setCurrentProjectTool);
  return registry;
}

/**
 * Realtime oturumunun kullandigi varsayilan registry.
 *
 * Yedi eleman: dort risk 0 (salt okuma), iki risk 1 ve **bir risk 2**
 * (`register_project`, Gate 3 M3 — kalici yetki genislemesi). Risk 3
 * (destructive / harici etki) hala **yok**. Modul yuklenirken
 * kurulur: gecersiz bir tanim (onaysiz risk 2/3, bozuk ad, tavani asan timeout)
 * uygulama acilirken patlar, konusma ortasinda degil.
 *
 * Wave D (ASU-067..070) neyi kapatti: Asuna registry'yi **goremiyordu**
 * (kullanici UI'dan proje ekliyor, model bunu bilmiyor) ve dizin icerigini
 * listeleyemiyordu (dosya adini bilmeden dosya okunamaz). Ikisi de canli testte
 * cikan gercek bosluklar.
 */
export const asunaToolRegistry: ToolRegistry = createAsunaToolRegistry();

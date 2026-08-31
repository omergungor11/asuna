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
import { openProjectTool } from './open-project';
import { readProjectFileTool } from './read-project-file';
import { ToolRegistry } from './registry';

export {
  APPROVAL_TIMEOUT_MS,
  approvalStateFor,
  resolveApproval,
  type ApprovalDecision,
  type ApprovalOutcome,
} from './approval-policy';
export { createGetCurrentProjectTool, getCurrentProjectTool } from './get-current-project';
export { createOpenProjectTool, openProjectTool, OPEN_PROJECT_TOOL_NAME } from './open-project';
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
  // ASU-051: risk 0, salt okuma. Sandbox + blok listesi Rust tarafinda.
  registry.register(readProjectFileTool);
  // ASU-052: risk 1, **onay ister**. Asuna'nin ilk yan etkili yetenegi;
  // registry onaysiz bir risk 1 tanimini zaten kabul etmezdi ama tanim da
  // kendisi `requiresApproval: true` diyor (gevsetici bir mod eklenirse bile
  // sorulmaya devam etsin).
  registry.register(openProjectTool);
  return registry;
}

/**
 * Realtime oturumunun kullandigi varsayilan registry.
 *
 * Uc eleman: iki risk 0 (salt okuma) ve bir risk 1 (onayli). Modul yuklenirken
 * kurulur: gecersiz bir tanim (onaysiz risk 2/3, bozuk ad, tavani asan timeout)
 * uygulama acilirken patlar, konusma ortasinda degil.
 */
export const asunaToolRegistry: ToolRegistry = createAsunaToolRegistry();

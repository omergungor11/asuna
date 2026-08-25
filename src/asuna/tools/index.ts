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
 */

import { getCurrentProjectTool } from './get-current-project';
import { ToolRegistry } from './registry';

export {
  APPROVAL_TIMEOUT_MS,
  approvalStateFor,
  resolveApproval,
  type ApprovalDecision,
  type ApprovalOutcome,
} from './approval-policy';
export { createGetCurrentProjectTool, getCurrentProjectTool } from './get-current-project';
export {
  executeTool,
  MAX_TOOL_TIMEOUT_MS,
  TOOL_ERROR_KINDS,
  ToolRegistry,
  ToolRegistryError,
  type ToolApprovalGate,
  type ToolExecutionOptions,
  type ToolRegistryErrorCode,
} from './registry';
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
  return registry;
}

/**
 * Realtime oturumunun kullandigi varsayilan registry.
 *
 * Su an tek eleman ve risk 0. Modul yuklenirken kurulur: gecersiz bir tanim
 * (onaysiz risk 2/3, bozuk ad, tavani asan timeout) uygulama acilirken patlar,
 * konusma ortasinda degil.
 */
export const asunaToolRegistry: ToolRegistry = createAsunaToolRegistry();

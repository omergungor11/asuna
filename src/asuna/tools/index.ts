/**
 * Modele acilan tool'lar (PROJECT.md Bolum 17).
 *
 * Bu dosya **kayit noktasidir**: "hangi yetenekler modele acik?" sorusunun tek
 * cevabi burada kurulan [`ToolRegistry`] ornegidir. Calistirma kurallari
 * (sema, timeout, yapisal sonuc) `registry.ts`'te; onay politikasi ASU-048,
 * `tool_events` audit yazimi ASU-050.
 *
 * MVP kurali (PROJECT.md Bolum 17): **once salt okuma**. Risk 2+ bir tool
 * eklemek orchestrator karari; bu listeye sessizce eklenmez — registry zaten
 * onaysiz risk 2/3 tanimini kayit aninda reddeder.
 */

import { getCurrentProjectTool } from './get-current-project';
import { ToolRegistry } from './registry';

export { createGetCurrentProjectTool, getCurrentProjectTool } from './get-current-project';
export {
  executeTool,
  MAX_TOOL_TIMEOUT_MS,
  TOOL_ERROR_KINDS,
  ToolRegistry,
  ToolRegistryError,
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

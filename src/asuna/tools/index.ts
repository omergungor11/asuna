/**
 * Modele acilan tool'lar (PROJECT.md Bolum 17).
 *
 * NOT: Phase 5'te registry'ye (ASU-047) tasinacak. Bu dosya o registry'nin
 * yerini tutmuyor — sadece "hangi tool'lar acik?" sorusunun **tek** cevabini
 * veriyor ki liste iki farkli cagiran tarafindan iki farkli sekilde
 * kurulamasin. Permission gate, onay kuyrugu ve `tool_events` audit yazimi
 * ASU-047/048 ile buraya gelecek.
 *
 * MVP kurali (PROJECT.md Bolum 17): **once salt okuma**. Risk 2+ bir tool
 * eklemek orchestrator karari; bu listeye sessizce eklenmez.
 */

import { getCurrentProjectTool } from './get-current-project';
import type { AsunaToolDefinition } from './types';

export { createGetCurrentProjectTool, getCurrentProjectTool } from './get-current-project';
export type { AsunaToolDefinition, ToolContext, ToolResult, ToolRisk } from './types';

/**
 * Realtime oturumuna verilen varsayilan tool listesi.
 *
 * Su an tek eleman ve hepsi risk 0. Liste buyudukce burada tutulmasinin sebebi
 * gorunurluk: modele hangi yeteneklerin acildigini tek bir yerde okunur olmali.
 */
export const DEFAULT_ASUNA_TOOLS: readonly AsunaToolDefinition[] = [getCurrentProjectTool];

/**
 * Aktif prompt surumunun tek secim noktasi (ASU-012).
 *
 * `conventions.md` — "Prompt Dosyalari": versiyonlu dosyalar yan yana yasar
 * (`core.v1.ts`, ileride `core.v2.ts`), **aktif versiyon tek yerden secilir**.
 * Uygulama kodu `core.v1` dosyasini dogrudan import etmez; buradan alir.
 */

export {
  ASUNA_CORE_PROMPT,
  ASUNA_CORE_PROMPT_VERSION,
  buildAsunaInstructions,
} from './core.v2';
export type { AsunaInstructionsContext } from './core.v2';

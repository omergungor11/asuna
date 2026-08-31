/**
 * Tool acma/kapama — **oturum-yerel** anahtar seti (ASU-054).
 *
 * PROJECT.md Bolum 19: "Tool tek tek devre disi birakilabiliyor". Bu modul o
 * maddenin MVP karsiligi ve bilerek en kucuk olani: bellekte bir isim kumesi
 * ve dinleyiciler. Kalici ayar, DB tablosu ya da IPC yuzeyi **yok**.
 *
 * # Neden kalici degil
 *
 * Kalici bir "kapali tool" listesi, kullanicinin aylar once kapattigi bir
 * yetenegi sessizce eksik birakabilir ve "Asuna neden bunu yapmiyor?" sorusunun
 * cevabi gorunmez bir ayara gomulurdu. Oturum-yerel olmasi kapatmayi **acik ve
 * gecici** bir eylem yapar. Kalicilik istenirse ayri bir karar (ve ayri bir
 * ekran) gerekir.
 *
 * # Kapatmanin iki katmani
 *
 * 1. **Liste**: kapali tool modele verilen tool listesinden dusurulur
 *    ([`ToolToggleStore.enabledDefinitions`]). Model onu hic gormez.
 * 2. **Kapi**: `executeTool` her cagrida `isEnabled`i tekrar sorar
 *    (`registry.ts` `TOOL_ERROR_KINDS.disabled`). Bu ikinci katman, acik bir
 *    oturumun ortasinda kapatilan bir tool icin gerekli: SDK'ya verilen liste o
 *    oturum boyunca sabittir, dolayisiyla model kapali bir tool'u yine
 *    cagirabilir. Cagri calismaz ve **deftere gecer** — gizli/gorunmez bir
 *    calistirma yolu yok (ASU-054 kabul kriteri).
 *
 * # `useSyncExternalStore` sozlesmesi
 *
 * Snapshot [`ToolToggleStore.disabledNames`]: **degismedigi surece ayni
 * referansi** donen, dondurulmus bir dizi. React'in `getSnapshot`'tan bekledigi
 * sozlesme bu — her cagride yeni bir dizi uretmek sonsuz render dongusu
 * demektir. Turetilmis liste ([`buildToolSummaries`]) cagiran tarafta bu
 * referansa `useMemo` ile baglanir.
 */

import { resolveApproval } from './approval-policy';
import type { AsunaToolDefinition } from './types';
import type { ToolApprovalMode } from '../config/frontend-config';
import type { ToolApprovalPolicy, ToolSummary } from '../../shared/tools';
import type { ToolRiskLevel } from '../../shared/tool-event';

type ToggleListener = () => void;

/**
 * Kapatilmis tool adlarinin oturum-yerel kaydi.
 *
 * Varsayilan: **her sey acik**. Kume yalnizca kullanicinin kapattiklarini
 * tutar; boylece registry'ye yeni bir tool eklendiginde kapali baslamaz.
 */
export class ToolToggleStore {
  private readonly disabled = new Set<string>();

  private readonly listeners = new Set<ToggleListener>();

  /**
   * `useSyncExternalStore` snapshot'i: kapatilmis tool adlari.
   *
   * Referans yalnizca gercek bir degisiklikte yenilenir. Her cagride yeni bir
   * dizi uretmek React'in snapshot sozlesmesini bozar (sonsuz render).
   */
  private snapshot: readonly string[] = Object.freeze([]);

  /** Kapatilmis tool adlari — degismedigi surece **ayni** referans. */
  public get disabledNames(): readonly string[] {
    return this.snapshot;
  }

  public isEnabled(toolName: string): boolean {
    return !this.disabled.has(toolName);
  }

  /**
   * Tool'u acar/kapatir.
   *
   * Durum degismediyse dinleyiciler **cagrilmaz**: ayni degeri iki kez yazmak
   * gereksiz bir render turu uretmemeli.
   */
  public setEnabled(toolName: string, enabled: boolean): void {
    const changed = enabled ? this.disabled.delete(toolName) : !this.disabled.has(toolName);
    if (!changed) {
      return;
    }
    if (!enabled) {
      this.disabled.add(toolName);
    }
    this.snapshot = Object.freeze([...this.disabled].sort());
    for (const listener of [...this.listeners]) {
      listener();
    }
  }

  /** Modele verilecek liste: kapali olanlar **dusurulur**. */
  public enabledDefinitions(
    definitions: readonly AsunaToolDefinition[],
  ): readonly AsunaToolDefinition[] {
    return definitions.filter((definition) => this.isEnabled(definition.name));
  }

  /** @returns aboneligi kaldiran fonksiyon. */
  public subscribe(listener: ToggleListener): () => void {
    this.listeners.add(listener);
    return (): void => {
      this.listeners.delete(listener);
    };
  }
}

/**
 * Onay matrisinin (ASU-048) UI diline cevrilmis hali.
 *
 * Tek kaynak yine [`resolveApproval`]: Tools sekmesinde gorunen politika ile
 * cagri aninda uygulanan politika ayni fonksiyondan gelir, ikinci bir tablo
 * yok. Aksi halde ekran "onaysiz" derken kart cikabilirdi.
 */
export function approvalPolicyFor(
  definition: AsunaToolDefinition,
  mode: ToolApprovalMode,
): ToolApprovalPolicy {
  return resolveApproval(definition.risk, definition.requiresApproval, mode) ===
    'needs_approval'
    ? 'always'
    : 'not_required';
}

/**
 * Registry tanimlarini UI ozetine cevirir (ASU-054 sozlesmesi,
 * `src/shared/tools.ts`).
 *
 * Saf fonksiyon: React'siz ve IPC'siz test edilir. Aciklama **modele verilen
 * metnin aynisi** — ekran icin ikinci bir metin tutmak, ikisinin zamanla
 * ayrismasi demekti.
 */
export function buildToolSummaries(
  definitions: readonly AsunaToolDefinition[],
  mode: ToolApprovalMode,
  isEnabled: (toolName: string) => boolean,
): readonly ToolSummary[] {
  return definitions.map((definition) => ({
    name: definition.name,
    description: definition.description,
    risk: definition.risk satisfies ToolRiskLevel,
    approval: approvalPolicyFor(definition, mode),
    enabled: isEnabled(definition.name),
  }));
}

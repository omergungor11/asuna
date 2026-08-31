/**
 * Tool acma/kapama kaydi + UI ozeti testleri (ASU-054).
 *
 * Kanitlanan seyler:
 * 1. Varsayilan **acik**; kapatma yalnizca kapatilani etkiliyor.
 * 2. Snapshot referansi degismedigi surece sabit (`useSyncExternalStore`
 *    sozlesmesi — aksi halde sonsuz render).
 * 3. Ayni degeri yeniden yazmak dinleyicileri uyandirmiyor.
 * 4. UI'da gorunen onay politikasi ASU-048 matrisinin **ayni** fonksiyonundan
 *    geliyor; ikinci bir tablo yok.
 */

import { describe, expect, it, vi } from 'vitest';

import { resolveApproval } from './approval-policy';
import { createAsunaToolRegistry } from './index';
import { approvalPolicyFor, buildToolSummaries, ToolToggleStore } from './tool-toggles';
import { NO_TOOL_ARGUMENTS, type AsunaToolDefinition, type ToolResult } from './types';
import { TOOL_APPROVAL_MODES } from '../config/frontend-config';

function defineTool(overrides: Partial<AsunaToolDefinition> = {}): AsunaToolDefinition {
  return {
    name: 'get_current_project',
    description: 'Kullanicinin su an uzerinde calistigi kayitli projeyi dondurur.',
    risk: 0,
    requiresApproval: false,
    timeoutMs: 5_000,
    parameters: NO_TOOL_ARGUMENTS,
    execute: (): Promise<ToolResult> => Promise.resolve({ ok: true, summary: 'oldu' }),
    ...overrides,
  };
}

describe('ToolToggleStore', () => {
  it('varsayilan olarak her sey acik', () => {
    const store = new ToolToggleStore();

    expect(store.isEnabled('get_current_project')).toBe(true);
    // Hic gorulmemis bir ad da acik: registry'ye eklenen yeni bir tool kapali
    // baslamamali.
    expect(store.isEnabled('henuz_yok')).toBe(true);
    expect(store.disabledNames).toEqual([]);
  });

  it('yalnizca kapatilan tool"u etkiliyor', () => {
    const store = new ToolToggleStore();

    store.setEnabled('open_project', false);

    expect(store.isEnabled('open_project')).toBe(false);
    expect(store.isEnabled('read_project_file')).toBe(true);
    expect(store.disabledNames).toEqual(['open_project']);
  });

  it('yeniden aciyor', () => {
    const store = new ToolToggleStore();
    store.setEnabled('open_project', false);

    store.setEnabled('open_project', true);

    expect(store.isEnabled('open_project')).toBe(true);
    expect(store.disabledNames).toEqual([]);
  });

  /**
   * `useSyncExternalStore` sozlesmesi: `getSnapshot` degismedigi surece **ayni
   * referansi** donmeli. Her cagride yeni bir dizi uretmek sonsuz render
   * dongusu demektir.
   */
  it('snapshot referansi degisiklik olmadan sabit kaliyor', () => {
    const store = new ToolToggleStore();
    const first = store.disabledNames;

    expect(store.disabledNames).toBe(first);

    store.setEnabled('open_project', false);
    const second = store.disabledNames;
    expect(second).not.toBe(first);
    expect(store.disabledNames).toBe(second);
  });

  it('degismeyen bir yazma dinleyicileri uyandirmiyor', () => {
    const store = new ToolToggleStore();
    const listener = vi.fn();
    store.subscribe(listener);

    store.setEnabled('open_project', true); // zaten acikti
    expect(listener).not.toHaveBeenCalled();

    store.setEnabled('open_project', false);
    expect(listener).toHaveBeenCalledTimes(1);

    store.setEnabled('open_project', false); // zaten kapaliydi
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('abonelik sokulebiliyor', () => {
    const store = new ToolToggleStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    unsubscribe();
    store.setEnabled('open_project', false);

    expect(listener).not.toHaveBeenCalled();
  });

  /** Modele giden liste: kapali olan **yok**. */
  it('kapali tool"u modele verilecek listeden dusuruyor', () => {
    const store = new ToolToggleStore();
    const definitions = [defineTool(), defineTool({ name: 'open_project', risk: 1 })];

    store.setEnabled('open_project', false);

    expect(store.enabledDefinitions(definitions).map((tool) => tool.name)).toEqual([
      'get_current_project',
    ]);
  });
});

describe('buildToolSummaries', () => {
  it('registry tanimlarini UI sozlesmesine ceviriyor', () => {
    const store = new ToolToggleStore();
    const definitions = [defineTool()];

    const [summary] = buildToolSummaries(definitions, 'safe', (name) => store.isEnabled(name));

    expect(summary).toEqual({
      name: 'get_current_project',
      // Aciklama modele verilenin **aynisi**: ikinci bir metin tutulmuyor.
      description: definitions[0]?.description,
      risk: 0,
      approval: 'not_required',
      enabled: true,
    });
  });

  it('kapali tool"u `enabled: false` ile gosteriyor (listeden silmiyor)', () => {
    const store = new ToolToggleStore();
    store.setEnabled('get_current_project', false);

    const [summary] = buildToolSummaries([defineTool()], 'safe', (name) =>
      store.isEnabled(name),
    );

    // Kapali tool ekrandan **kaybolmaz**; kullanici geri acabilmeli.
    expect(summary?.enabled).toBe(false);
  });

  /**
   * Ekranda gorunen politika ile cagri aninda uygulanan politika **ayni
   * fonksiyondan** gelmeli; aksi halde ekran "onaysiz" derken kart cikabilirdi.
   */
  it('onay politikasi ASU-048 matrisiyle birebir', () => {
    const definitions = createAsunaToolRegistry().list();

    for (const mode of TOOL_APPROVAL_MODES) {
      for (const summary of buildToolSummaries(definitions, mode, () => true)) {
        const definition = definitions.find((tool) => tool.name === summary.name);
        expect(definition).toBeDefined();
        const expected =
          resolveApproval(
            definition?.risk ?? 0,
            definition?.requiresApproval ?? false,
            mode,
          ) === 'needs_approval'
            ? 'always'
            : 'not_required';
        expect(summary.approval).toBe(expected);
      }
    }
  });

  /** Varsayilan set: iki salt okuma onaysiz, `open_project` her modda onayli. */
  it('varsayilan tool setinin politikalari beklendigi gibi', () => {
    const summaries = buildToolSummaries(createAsunaToolRegistry().list(), 'safe', () => true);
    const byName = new Map(summaries.map((summary) => [summary.name, summary]));

    expect(byName.get('get_current_project')?.approval).toBe('not_required');
    expect(byName.get('read_project_file')?.approval).toBe('not_required');
    expect(byName.get('open_project')?.approval).toBe('always');
    expect(byName.get('open_project')?.risk).toBe(1);
  });

  it('approvalPolicyFor risk 2/3"u modden bagimsiz onayli sayiyor', () => {
    const mutation = defineTool({ name: 'edit_file', risk: 2, requiresApproval: true });

    for (const mode of TOOL_APPROVAL_MODES) {
      expect(approvalPolicyFor(mutation, mode)).toBe('always');
    }
  });
});

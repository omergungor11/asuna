/**
 * Tool registry + `executeTool` testleri (ASU-047).
 *
 * Kanitlanan seyler:
 * 1. Ayni isim iki kez kaydedilemiyor; sozlesme ihlali kayit aninda patliyor.
 * 2. Gecersiz arguman **tool'u calistirmiyor** (spy cagrilmiyor) ve yapisal hata donuyor.
 * 3. Asili kalan tool timeout'ta yapisal sonuca donusuyor; `context.signal` abort oluyor.
 * 4. Disaridan gelen iptal sonucu bekletmiyor.
 * 5. `get_current_project` registry uzerinden ayni davranisi gosteriyor.
 */

import { describe, expect, it, vi } from 'vitest';
import { z } from 'zod';

import {
  createGetCurrentProjectTool,
  GET_CURRENT_PROJECT_TOOL_NAME,
} from './get-current-project';
import { createAsunaToolRegistry } from './index';
import {
  executeTool,
  MAX_TOOL_TIMEOUT_MS,
  TOOL_ERROR_KINDS,
  ToolRegistry,
  ToolRegistryError,
} from './registry';
import {
  NO_TOOL_ARGUMENTS,
  type AsunaToolDefinition,
  type ToolContext,
  type ToolResult,
} from './types';

const CONTEXT: ToolContext = { sessionId: null, projectRoot: null };

const OK: ToolResult = { ok: true, summary: 'oldu' };

type Execute = (args: unknown, context: ToolContext) => Promise<ToolResult>;

function defineTool(overrides: Partial<AsunaToolDefinition> = {}): AsunaToolDefinition {
  return {
    name: 'read_project_file',
    description: 'Kayitli proje kokundeki bir dosyayi okur ve icerigini ozetler.',
    risk: 0,
    requiresApproval: false,
    timeoutMs: 5_000,
    parameters: NO_TOOL_ARGUMENTS,
    execute: (): Promise<ToolResult> => Promise.resolve(OK),
    ...overrides,
  };
}

describe('ToolRegistry — kayit', () => {
  it('kaydediyor, listeliyor ve isimle cozuyor', () => {
    const registry = new ToolRegistry();
    const tool = defineTool();

    registry.register(tool);

    expect(registry.list()).toEqual([tool]);
    expect(registry.resolve('read_project_file')).toBe(tool);
    expect(registry.has('read_project_file')).toBe(true);
  });

  /** Uydurma bir tool adi sessizce baska bir seye cozulmez. */
  it('bilinmeyen ad `null` donuyor', () => {
    expect(new ToolRegistry().resolve('rm_rf_slash')).toBeNull();
  });

  it('liste kayit sirasini koruyor', () => {
    const registry = new ToolRegistry();
    registry.register(defineTool({ name: 'read_project_file' }));
    registry.register(defineTool({ name: 'get_git_status' }));

    expect(registry.list().map((tool) => tool.name)).toEqual([
      'read_project_file',
      'get_git_status',
    ]);
  });

  /**
   * Ustune yazmak, hangi implementasyonun calistigini belirsiz birakirdi —
   * guvenlik acisindan en kotu belirsizlik turu.
   */
  it('ayni isim ikinci kez kaydedilemiyor', () => {
    const registry = new ToolRegistry();
    const first = defineTool();
    registry.register(first);

    expect(() => {
      registry.register(defineTool({ description: 'Ayni isim, baska bir implementasyon.' }));
    }).toThrow(ToolRegistryError);

    // Ilk kayit korunuyor: cakisma sessizce "son kazanir"a donusmuyor.
    expect(registry.resolve('read_project_file')).toBe(first);
    expect(registry.size).toBe(1);
  });

  it('cift kayit hatasi kodlu ve tool adini soyluyor', () => {
    const registry = new ToolRegistry();
    registry.register(defineTool());

    try {
      registry.register(defineTool());
      expect.unreachable('cift kayit kabul edildi');
    } catch (error) {
      expect(error).toBeInstanceOf(ToolRegistryError);
      if (error instanceof ToolRegistryError) {
        expect(error.code).toBe('duplicate_tool');
        expect(error.message).toContain('read_project_file');
      }
    }
  });
});

describe('ToolRegistry — sozlesme zorlamasi', () => {
  /** `conventions.md` pazarliksiz kurali: risk 2/3 her zaman onay ister. */
  it('risk 2/3 onaysiz kaydedilemiyor', () => {
    const registry = new ToolRegistry();

    expect(() => {
      registry.register(
        defineTool({ name: 'delete_branch', risk: 2, requiresApproval: false }),
      );
    }).toThrow(ToolRegistryError);
    expect(() => {
      registry.register(defineTool({ name: 'push_branch', risk: 3, requiresApproval: false }));
    }).toThrow(ToolRegistryError);

    expect(registry.size).toBe(0);

    expect(() => {
      registry.register(defineTool({ name: 'push_branch', risk: 3, requiresApproval: true }));
    }).not.toThrow();
  });

  it('snake_case olmayan ad reddediliyor', () => {
    const registry = new ToolRegistry();

    for (const name of [
      'getCurrentProject',
      'Read_File',
      'read file',
      '',
      '_read',
      'read__file',
    ]) {
      expect(() => {
        registry.register(defineTool({ name }));
      }).toThrow(ToolRegistryError);
    }
  });

  it('timeout zorunlu ve tavani var', () => {
    const registry = new ToolRegistry();

    for (const timeoutMs of [
      0,
      -1,
      Number.NaN,
      Number.POSITIVE_INFINITY,
      MAX_TOOL_TIMEOUT_MS + 1,
    ]) {
      expect(() => {
        registry.register(defineTool({ timeoutMs }));
      }).toThrow(ToolRegistryError);
    }

    expect(() => {
      registry.register(defineTool({ timeoutMs: MAX_TOOL_TIMEOUT_MS }));
    }).not.toThrow();
  });

  it('bos/yetersiz aciklama reddediliyor', () => {
    expect(() => {
      new ToolRegistry().register(defineTool({ description: 'dosya' }));
    }).toThrow(ToolRegistryError);
  });
});

describe('executeTool — sema dogrulamasi', () => {
  const parameters = z.strictObject({ path: z.string().min(1) });

  /** Kabul kriteri: gecersizse **calistirilmiyor**. */
  it('gecersiz arguman tool"u calistirmiyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));
    const tool = defineTool({ parameters, execute });

    const result = await executeTool(tool, { path: 42 }, CONTEXT);

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe(TOOL_ERROR_KINDS.invalidArguments);
      expect(result.summary).toContain('path');
      expect(result.summary).toContain('calistirilmadi');
    }
  });

  it('eksik alan da calistirmiyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));

    const result = await executeTool(defineTool({ parameters, execute }), {}, CONTEXT);

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });

  /** Reddedilen deger modele/audit'e geri dokulmez (PROJECT.md Bolum 19). */
  it('hata ozeti reddedilen degeri yazmiyor', async () => {
    const result = await executeTool(
      defineTool({ parameters: z.strictObject({ path: z.string() }) }),
      { path: { file: '/Users/omer/.ssh/id_ed25519' }, token: 'sk-live-cok-gizli' },
      CONTEXT,
    );

    expect(result.ok).toBe(false);
    expect(result.summary).not.toContain('id_ed25519');
    expect(result.summary).not.toContain('sk-live-cok-gizli');
  });

  /** Parametresiz tool'a uydurma parametre sessizce yutulmaz. */
  it('parametresiz tool fazladan alani reddediyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));

    const result = await executeTool(defineTool({ execute }), { project: 'asuna' }, CONTEXT);

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });

  it('gecerli arguman parse edilmis haliyle tool"a geciyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));

    const result = await executeTool(
      defineTool({ parameters, execute }),
      { path: 'README.md' },
      CONTEXT,
    );

    expect(result).toEqual(OK);
    expect(execute).toHaveBeenCalledTimes(1);
    expect(execute.mock.calls[0]?.[0]).toEqual({ path: 'README.md' });
  });
});

describe('executeTool — timeout ve iptal', () => {
  it('asili kalan tool oturumu kilitlemiyor', async () => {
    vi.useFakeTimers();
    try {
      const tool = defineTool({
        timeoutMs: 5_000,
        execute: (): Promise<ToolResult> => new Promise<ToolResult>(() => undefined),
      });

      const pending = executeTool(tool, {}, CONTEXT);
      await vi.advanceTimersByTimeAsync(5_000);
      const result = await pending;

      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.errorKind).toBe(TOOL_ERROR_KINDS.timeout);
        expect(result.summary).toContain('5000 ms');
        // "Basarisiz" degil "bilinmiyor": is arkada bitmis olabilir.
        expect(result.summary).toContain('yapildi deme');
      }
    } finally {
      vi.useRealTimers();
    }
  });

  it('timeout tool"un iptal sinyalini tetikliyor', async () => {
    vi.useFakeTimers();
    try {
      let seen: AbortSignal | undefined;
      const tool = defineTool({
        timeoutMs: 1_000,
        execute: (_args: unknown, context: ToolContext): Promise<ToolResult> => {
          seen = context.signal;
          return new Promise<ToolResult>(() => undefined);
        },
      });

      const pending = executeTool(tool, {}, CONTEXT);
      expect(seen?.aborted).toBe(false);

      await vi.advanceTimersByTimeAsync(1_000);
      await pending;

      expect(seen?.aborted).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('timeout dolmadan biten tool normal sonucunu donuyor', async () => {
    vi.useFakeTimers();
    try {
      const tool = defineTool({ timeoutMs: 5_000 });

      const pending = executeTool(tool, {}, CONTEXT);
      await vi.advanceTimersByTimeAsync(10_000);

      expect(await pending).toEqual(OK);
    } finally {
      vi.useRealTimers();
    }
  });

  it('disaridan iptal sonucu bekletmiyor', async () => {
    const controller = new AbortController();
    const tool = defineTool({
      timeoutMs: 60_000,
      execute: (): Promise<ToolResult> => new Promise<ToolResult>(() => undefined),
    });

    const pending = executeTool(tool, {}, CONTEXT, { signal: controller.signal });
    controller.abort();
    const result = await pending;

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe(TOOL_ERROR_KINDS.aborted);
    }
  });

  it('zaten iptal edilmis cagri hic calistirilmiyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));
    const controller = new AbortController();
    controller.abort();

    const result = await executeTool(defineTool({ execute }), {}, CONTEXT, {
      signal: controller.signal,
    });

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });
});

describe('executeTool — yapisal sonuc', () => {
  /** `conventions.md`: sessiz yutma yok, basari taklit edilmez. */
  it('tool firlatirsa yapisal hataya cevriliyor, `throw` disari cikmiyor', async () => {
    const tool = defineTool({
      execute: (): Promise<ToolResult> => Promise.reject(new Error('IPC reddetti')),
    });

    const result = await executeTool(tool, {}, CONTEXT);

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe(TOOL_ERROR_KINDS.failed);
      expect(result.summary).toContain('IPC reddetti');
    }
  });

  it('tool"un kendi `ok: false` sonucu oldugu gibi tasiniyor', async () => {
    const own: ToolResult = { ok: false, summary: 'Proje yok.', errorKind: 'no_project' };

    expect(
      await executeTool(defineTool({ execute: () => Promise.resolve(own) }), {}, CONTEXT),
    ).toEqual(own);
  });
});

describe('varsayilan registry — `get_current_project`', () => {
  it('registry"ye kayitli ve risk 0', () => {
    const registry = createAsunaToolRegistry();
    const tool = registry.resolve(GET_CURRENT_PROJECT_TOOL_NAME);

    expect(tool).not.toBeNull();
    expect(tool?.risk).toBe(0);
    expect(tool?.requiresApproval).toBe(false);
    expect(registry.list()).toHaveLength(1);
  });

  /** ASU-044 davranisi tasinmada bozulmadi: ayni ozet, ayni yapisal veri. */
  it('registry uzerinden calistirildiginda ayni sonucu donuyor', async () => {
    const registry = new ToolRegistry();
    registry.register(
      createGetCurrentProjectTool({
        fetchContext: () =>
          Promise.resolve({
            status: 'unknown',
            reason: 'no-current-selection',
            message: 'Yok.',
          }),
      }),
    );

    const tool = registry.resolve(GET_CURRENT_PROJECT_TOOL_NAME);
    expect(tool).not.toBeNull();

    const result = await executeTool(tool!, {}, CONTEXT);

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.data).toEqual({ status: 'unknown', reason: 'no-current-selection' });
      expect(result.summary).toContain('Guncel proje bilinmiyor.');
    }
  });
});

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
  type ToolApprovalGate,
} from './registry';
import {
  NO_TOOL_ARGUMENTS,
  type AsunaToolDefinition,
  type ToolContext,
  type ToolResult,
} from './types';
import type { ToolAuditInput } from '../../shared/tool-event';

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

describe('executeTool — onay kapisi (ASU-048)', () => {
  /** Risk 0 tool onay kapisina hic ugramiyor: gate cagrilmiyor. */
  it('onay gerekmeyen cagri gate"e sorulmuyor', async () => {
    const gate = vi.fn<ToolApprovalGate>();
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));

    const result = await executeTool(defineTool({ execute }), {}, CONTEXT, {
      approvalMode: 'safe',
      approvalGate: gate,
    });

    expect(gate).not.toHaveBeenCalled();
    expect(execute).toHaveBeenCalledTimes(1);
    expect(result.ok).toBe(true);
  });

  /**
   * Reddedilen cagri **calismiyor**. Kanit spy: `execute` hic cagrilmadi,
   * yani "calisti ama sonucu atildi" degil, gercekten calismadi.
   */
  it('reddedilen onayda execute HIC cagrilmiyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));
    const tool = defineTool({ risk: 2, requiresApproval: true, execute });

    const result = await executeTool(tool, {}, CONTEXT, {
      approvalMode: 'safe',
      approvalGate: () => Promise.resolve('denied'),
    });

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe(TOOL_ERROR_KINDS.denied);
      // Model reddi acikca gormeli: "yaptim" diyemesin.
      expect(result.summary).toContain('onaylanmadi');
    }
  });

  it('onay zaman asiminda execute HIC cagrilmiyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));
    const tool = defineTool({ risk: 2, requiresApproval: true, execute });

    const result = await executeTool(tool, {}, CONTEXT, {
      approvalMode: 'safe',
      approvalGate: () => Promise.resolve('timeout'),
    });

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errorKind).toBe(TOOL_ERROR_KINDS.denied);
      expect(result.summary).toContain('sure doldu');
    }
  });

  /**
   * Kapiyi baglamayi unutmak "onaysiz calistir"a donusmuyor. ASU-048'in
   * varsayilani: belirsizlik onay lehine, yani CALISTIRMA.
   */
  it('onay kanali yoksa onay gerektiren tool calismiyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));
    const tool = defineTool({ risk: 3, requiresApproval: true, execute });

    const audits: ToolAuditInput[] = [];
    const result = await executeTool(tool, {}, CONTEXT, {
      approvalMode: 'always',
      onAudit: (input) => audits.push(input),
    });

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    // Onay GEREKTI ama SORULAMADI: `not_required` ile karistirilmiyor.
    expect(audits[0]?.approvalState).toBe('not_requested');
  });

  /** Gate patlarsa da varsayilan reddetmek — hata "herhalde tamamdir" demek degil. */
  it('gate firlatirsa cagri reddediliyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));
    const tool = defineTool({ risk: 2, requiresApproval: true, execute });

    const result = await executeTool(tool, {}, CONTEXT, {
      approvalGate: () => Promise.reject(new Error('kanal koptu')),
    });

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });

  it('onaylanan cagri calisiyor ve gate"e tanim + parse edilmis arguman gidiyor', async () => {
    const gate = vi.fn<ToolApprovalGate>(() => Promise.resolve('approved'));
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));
    const tool = defineTool({
      risk: 1,
      execute,
      parameters: z.strictObject({ path: z.string() }),
    });

    const result = await executeTool(tool, { path: 'README.md' }, CONTEXT, {
      approvalMode: 'safe',
      approvalGate: gate,
    });

    expect(gate).toHaveBeenCalledWith(tool, { path: 'README.md' });
    expect(execute).toHaveBeenCalledTimes(1);
    expect(result.ok).toBe(true);
  });

  /**
   * Onay bekleme suresi tool'un calisma butcesini yemiyor: sayac onaydan sonra
   * basliyor. Aksi halde 60 sn dusunen bir kullanici, 5 sn timeout'lu bir
   * tool'u kendiliginden zaman asimina ugratirdi.
   */
  it('tool timeout sayaci onaydan sonra basliyor', async () => {
    const tool = defineTool({ risk: 2, requiresApproval: true, timeoutMs: 30 });

    const result = await executeTool(tool, {}, CONTEXT, {
      approvalGate: async (): Promise<'approved'> => {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return 'approved';
      },
    });

    expect(result.ok).toBe(true);
  });

  /** Mod verilmezse en siki davranis: risk 1 onay ister. */
  it('mod verilmediginde en siki varsayilan uygulaniyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));

    const result = await executeTool(defineTool({ risk: 1, execute }), {}, CONTEXT);

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });
});

describe('executeTool — audit kaydi (ASU-048 x ASU-050)', () => {
  function collect(): {
    readonly audits: ToolAuditInput[];
    readonly onAudit: (i: ToolAuditInput) => void;
  } {
    const audits: ToolAuditInput[] = [];
    return { audits, onAudit: (input): void => void audits.push(input) };
  }

  it('calisan risk 0 cagri `not_required` ile yaziliyor', async () => {
    const { audits, onAudit } = collect();

    await executeTool(defineTool(), {}, { sessionId: 7, projectRoot: null }, { onAudit });

    expect(audits).toHaveLength(1);
    expect(audits[0]).toMatchObject({
      toolName: 'read_project_file',
      riskLevel: 0,
      approvalState: 'not_required',
      sessionId: 7,
      resultSummary: 'oldu',
    });
  });

  it('onaylanan cagri `approved`, reddedilen `denied`, zaman asimi `timeout` yaziyor', async () => {
    const tool = defineTool({ risk: 2, requiresApproval: true });
    const outcomes = ['approved', 'denied', 'timeout'] as const;
    const states: string[] = [];

    for (const outcome of outcomes) {
      const { audits, onAudit } = collect();
      await executeTool(tool, {}, CONTEXT, {
        approvalGate: () => Promise.resolve(outcome),
        onAudit,
      });
      expect(audits).toHaveLength(1);
      states.push(audits[0]?.approvalState ?? 'yok');
    }

    expect(states).toEqual(['approved', 'denied', 'timeout']);
  });

  /** Sema reddi onay asamasina hic gelmiyor; defter bunu ayirt ediyor. */
  it('sema reddi `not_requested` ile yaziliyor', async () => {
    const { audits, onAudit } = collect();
    const tool = defineTool({ parameters: z.strictObject({ path: z.string() }) });

    await executeTool(tool, { path: 42 }, CONTEXT, { onAudit });

    expect(audits[0]?.approvalState).toBe('not_requested');
  });

  /**
   * Oturum kimligi bilinmiyorsa alan **yok** — sifir ya da uydurma bir deger
   * yazilmiyor (audit satiri "hangi konusma" sorusuna yanlis cevap vermez).
   */
  it('oturum kimligi bilinmiyorsa alan gonderilmiyor', async () => {
    const { audits, onAudit } = collect();

    await executeTool(defineTool(), {}, CONTEXT, { onAudit });

    expect(audits[0]).not.toHaveProperty('sessionId');
  });

  it('ham argumanlar oldugu gibi gonderiliyor (redaksiyon host tarafinda)', async () => {
    const { audits, onAudit } = collect();
    const tool = defineTool({ parameters: z.strictObject({ path: z.string() }) });

    await executeTool(tool, { path: 'README.md' }, CONTEXT, { onAudit });

    expect(audits[0]?.arguments).toEqual({ path: 'README.md' });
  });

  it('audit kancasi cagri basina tam bir kez cagriliyor', async () => {
    const { audits, onAudit } = collect();
    const tool = defineTool({
      execute: (): Promise<ToolResult> => Promise.reject(new Error('patladi')),
    });

    await executeTool(tool, {}, CONTEXT, { onAudit });

    expect(audits).toHaveLength(1);
    expect(audits[0]?.resultSummary).toContain('patladi');
  });
});

describe('varsayilan registry — `get_current_project`', () => {
  it('registry"ye kayitli ve risk 0', () => {
    const registry = createAsunaToolRegistry();
    const tool = registry.resolve(GET_CURRENT_PROJECT_TOOL_NAME);

    expect(tool).not.toBeNull();
    expect(tool?.risk).toBe(0);
    expect(tool?.requiresApproval).toBe(false);
  });

  /**
   * Varsayilan set **acikca** yazili: yeni bir yetenegin registry'ye sessizce
   * eklenmesi (ve modele acilmasi) bir liste karsilastirmasiyla goruluyor.
   * PROJECT.md Bolum 17 "once salt okuma": risk 2+ bir tool bu listeye
   * orchestrator karari olmadan giremez.
   */
  it('varsayilan set yedi tool: dort salt okuma, iki risk 1, bir risk 2', () => {
    const registry = createAsunaToolRegistry();

    expect(registry.list().map((tool) => tool.name)).toEqual([
      'get_current_project',
      'list_projects',
      'read_project_file',
      'list_project_files',
      'open_project',
      'register_project',
      'set_current_project',
    ]);

    // ASU-051/067/068: salt okuma yuzeyleri onaysiz.
    for (const name of ['list_projects', 'read_project_file', 'list_project_files']) {
      expect(registry.resolve(name)?.risk, name).toBe(0);
      expect(registry.resolve(name)?.requiresApproval, name).toBe(false);
    }

    // ASU-052/070: risk 1 ve tanimlarin kendisi onay istiyor — ileride risk 1'i
    // otomatik geciren bir mod eklense bile sorulmaya devam ederler
    // (`resolveApproval` bir tanimi gevsetemez).
    for (const name of ['open_project', 'set_current_project']) {
      expect(registry.resolve(name)?.risk, name).toBe(1);
      expect(registry.resolve(name)?.requiresApproval, name).toBe(true);
    }

    // ASU-069 (Gate 3 M3): tek risk 2 tool'u. Kayitli kok = okunabilir alan
    // demek, dolayisiyla kayit **kalici** bir yetki genislemesi. Risk 2 olmasi
    // `register` zorlamasini devreye sokuyor: onay talebi silinirse tool
    // acilista reddedilir.
    expect(registry.resolve('register_project')?.risk).toBe(2);
    expect(registry.resolve('register_project')?.requiresApproval).toBe(true);

    // Risk 3 (destructive/harici etki) hicbir tool acik degil ve risk 2 yalnizca
    // bu bir tanede — yeni bir tanenin sessizce eklenmesi burada gorunur.
    expect(registry.list().filter((tool) => tool.risk >= 2).map((tool) => tool.name)).toEqual([
      'register_project',
    ]);
    expect(registry.list().filter((tool) => tool.risk === 3)).toEqual([]);
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

describe('executeTool — sonuc ekseni (ASU-051 `outcome`)', () => {
  function collect(): {
    readonly audits: ToolAuditInput[];
    readonly onAudit: (i: ToolAuditInput) => void;
  } {
    const audits: ToolAuditInput[] = [];
    return { audits, onAudit: (input): void => void audits.push(input) };
  }

  it('basarili cagri `succeeded` yaziyor', async () => {
    const { audits, onAudit } = collect();
    await executeTool(defineTool(), {}, CONTEXT, { onAudit });
    expect(audits[0]?.outcome).toBe('succeeded');
  });

  /**
   * `execute` CALISTI ve isini yapamadi: `not_run` degil `failed`. Yan etkisi
   * olabilecek bir cagriyi "hic olmadi" diye kaydetmek denetim defterine yalan
   * yazmak olurdu (migration 005 gerekcesi).
   */
  it('calisip basarisiz olan cagri `failed` yaziyor', async () => {
    const { audits, onAudit } = collect();
    const tool = defineTool({
      execute: (): Promise<ToolResult> =>
        Promise.resolve({ ok: false, summary: 'olmadi', errorKind: 'x' }),
    });

    await executeTool(tool, {}, CONTEXT, { onAudit });

    expect(audits[0]?.outcome).toBe('failed');
  });

  it('firlatan tool da `failed` — hata yutulmuyor', async () => {
    const { audits, onAudit } = collect();
    const tool = defineTool({
      execute: (): Promise<ToolResult> => Promise.reject(new Error('patladi')),
    });

    await executeTool(tool, {}, CONTEXT, { onAudit });

    expect(audits[0]?.outcome).toBe('failed');
  });

  /** Sema reddi: `execute` hic cagrilmadi. */
  it('gecersiz argumanda `not_run` yaziyor', async () => {
    const { audits, onAudit } = collect();

    await executeTool(defineTool(), { uydurma: 1 }, CONTEXT, { onAudit });

    expect(audits[0]?.outcome).toBe('not_run');
    expect(audits[0]?.approvalState).toBe('not_requested');
  });

  /** Onay reddi: yine `execute` cagrilmadi. */
  it('onaylanmayan cagri `not_run` yaziyor', async () => {
    const { audits, onAudit } = collect();
    const tool = defineTool({ risk: 2, requiresApproval: true });

    await executeTool(tool, {}, CONTEXT, {
      approvalGate: () => Promise.resolve('denied'),
      onAudit,
    });

    expect(audits[0]?.approvalState).toBe('denied');
    expect(audits[0]?.outcome).toBe('not_run');
  });

  /**
   * Iki eksen bagimsiz ve **birlikte** anlamli: kullanici izin verdi, is
   * calisti ve patladi.
   */
  it('onaylanmis bir cagri `failed` olabilir', async () => {
    const { audits, onAudit } = collect();
    const tool = defineTool({
      risk: 2,
      requiresApproval: true,
      execute: (): Promise<ToolResult> =>
        Promise.resolve({ ok: false, summary: 'olmadi', errorKind: 'x' }),
    });

    await executeTool(tool, {}, CONTEXT, {
      approvalGate: () => Promise.resolve('approved'),
      onAudit,
    });

    expect(audits[0]?.approvalState).toBe('approved');
    expect(audits[0]?.outcome).toBe('failed');
  });

  /**
   * **ASU-051 gizlilik kilidi**: icerik donduren bir tool'un modele verdigi
   * metin deftere girmez. `auditSummary` varsa **o** yazilir.
   */
  it('auditSummary varsa deftere o yaziliyor, modele giden metin degil', async () => {
    const { audits, onAudit } = collect();
    const tool = defineTool({
      execute: (): Promise<ToolResult> =>
        Promise.resolve({
          ok: true,
          summary: 'OPENAI_API_KEY=cok-gizli-dosya-icerigi\nsatir 2\nsatir 3',
          auditSummary: 'README.md okundu (2.1 KB)',
        }),
    });

    await executeTool(tool, {}, CONTEXT, { onAudit });

    expect(audits[0]?.resultSummary).toBe('README.md okundu (2.1 KB)');
    expect(audits[0]?.resultSummary).not.toContain('cok-gizli-dosya-icerigi');
  });

  it('auditSummary yoksa summary yaziliyor (mevcut davranis)', async () => {
    const { audits, onAudit } = collect();
    await executeTool(defineTool(), {}, CONTEXT, { onAudit });
    expect(audits[0]?.resultSummary).toBe('oldu');
  });
});

describe('executeTool — kapatilmis tool (ASU-054)', () => {
  /**
   * **Kabul kriteri**: "Gizli/gorunmez tool calistirma yolu yok". Kapali bir
   * tool modele verilen listede zaten yok; bu kapi acik bir oturumun ortasinda
   * kapatilan tool icin.
   */
  it('kapali tool calismiyor ve `execute` hic cagrilmiyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));
    const tool = defineTool({ execute });

    const result = await executeTool(tool, {}, CONTEXT, {
      isEnabled: () => false,
    });

    expect(execute).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    expect(!result.ok && result.errorKind).toBe(TOOL_ERROR_KINDS.disabled);
  });

  /** Reddedilen cagri **deftere gecer**: sessizce dusmez. */
  it('kapali tool cagrisi audit"e `not_run` olarak yaziliyor', async () => {
    const audits: ToolAuditInput[] = [];

    await executeTool(defineTool(), {}, CONTEXT, {
      isEnabled: () => false,
      onAudit: (input): void => void audits.push(input),
    });

    expect(audits).toHaveLength(1);
    expect(audits[0]?.outcome).toBe('not_run');
    expect(audits[0]?.approvalState).toBe('not_requested');
    expect(audits[0]?.toolName).toBe('read_project_file');
  });

  /**
   * Kapatma onay kapisinin **onunde**: onay gerektiren bir tool icin kullaniciya
   * kart bile cikmaz. Aksi halde kapali bir tool icin onay istenir, kullanici
   * onaylar ve sonra reddedilirdi — anlamsiz bir dongu.
   */
  it('kapali tool icin onay bile istenmiyor', async () => {
    const approvalGate = vi.fn<ToolApprovalGate>(() => Promise.resolve('approved' as const));
    const tool = defineTool({ risk: 2, requiresApproval: true });

    await executeTool(tool, {}, CONTEXT, { isEnabled: () => false, approvalGate });

    expect(approvalGate).not.toHaveBeenCalled();
  });

  it('acik tool etkilenmiyor', async () => {
    const execute = vi.fn<Execute>(() => Promise.resolve(OK));

    const result = await executeTool(defineTool({ execute }), {}, CONTEXT, {
      isEnabled: () => true,
    });

    expect(execute).toHaveBeenCalledTimes(1);
    expect(result.ok).toBe(true);
  });
});

describe('executeTool — sonuc bildirimi (ASU-054 transcript)', () => {
  it('her cikis yolunda tam bir kez cagriliyor', async () => {
    const reports: string[] = [];
    const onResult = (report: { outcome: string }): void => void reports.push(report.outcome);

    await executeTool(defineTool(), {}, CONTEXT, { onResult });
    await executeTool(defineTool(), { uydurma: 1 }, CONTEXT, { onResult });
    await executeTool(defineTool(), {}, CONTEXT, { isEnabled: () => false, onResult });

    expect(reports).toEqual(['succeeded', 'not_run', 'not_run']);
  });

  /** Transcript satiri da icerik gormez. */
  it('ozet olarak auditSummary kullaniliyor', async () => {
    const summaries: string[] = [];
    const tool = defineTool({
      execute: (): Promise<ToolResult> =>
        Promise.resolve({
          ok: true,
          summary: 'dosyanin tamami burada, cok uzun bir metin',
          auditSummary: 'README.md okundu (2.1 KB)',
        }),
    });

    await executeTool(tool, {}, CONTEXT, {
      onResult: (report): void => void summaries.push(report.summary),
    });

    expect(summaries).toEqual(['README.md okundu (2.1 KB)']);
  });

  it('risk ve onay durumu raporda tasiniyor', async () => {
    const reports: { risk: number; approvalState: string }[] = [];
    const tool = defineTool({ risk: 1, requiresApproval: true });

    await executeTool(tool, {}, CONTEXT, {
      approvalGate: () => Promise.resolve('approved'),
      onResult: (report): void =>
        void reports.push({ risk: report.risk, approvalState: report.approvalState }),
    });

    expect(reports).toEqual([{ risk: 1, approvalState: 'approved' }]);
  });
});

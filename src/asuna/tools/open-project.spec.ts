/**
 * `open_project` tool testleri (ASU-052).
 *
 * Kanitlanan seyler:
 * 1. Tanim: risk 1 + tanimin **kendi** onay talebi (mod gevsetemez).
 * 2. Parametresiz ve strict: model ne yolu ne editoru secebilir.
 * 3. Onaylanmadiginda `execute` **hic** cagrilmiyor ve model "actim" diyemiyor.
 * 4. Editor bulunamadiginda durust hata (PROJECT.md Bolum 30 cumlesi).
 * 5. Her yol deftere gecer; calisan `succeeded`, calismayan `not_run`.
 */

import { describe, expect, it, vi } from 'vitest';

import { createOpenProjectTool, OPEN_PROJECT_TOOL_NAME } from './open-project';
import { resolveApproval } from './approval-policy';
import { executeTool } from './registry';
import type { ToolContext } from './types';
import type { ToolAuditInput } from '../../shared/tool-event';
import { TOOL_APPROVAL_MODES } from '../config/frontend-config';

const CONTEXT: ToolContext = { sessionId: null, projectRoot: null };

const OUTCOME = {
  projectId: 'asuna',
  projectName: 'Asuna',
  editor: 'code',
  openedAt: '2026-08-25T10:00:00Z',
};

function toolWith(
  openProject: () => Promise<unknown>,
): ReturnType<typeof createOpenProjectTool> {
  return createOpenProjectTool({ openProject });
}

/**
 * Tauri `invoke` hatayi bir `Error` degil, komutun serilestirdigi **duz nesne**
 * olarak reddeder (`{ code, message, auditSummary }`). Testin olctugu davranis
 * tam olarak bu, bu yuzden cast bilincli.
 */
function rejectWith(error: unknown): () => Promise<unknown> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- bkz. yukari
  return (): Promise<unknown> => Promise.reject(error);
}

describe('open_project — tanim', () => {
  it('risk 1 ve tanimin kendisi onay istiyor', () => {
    const tool = toolWith(() => Promise.resolve(OUTCOME));

    expect(tool.name).toBe(OPEN_PROJECT_TOOL_NAME);
    expect(tool.risk).toBe(1);
    expect(tool.requiresApproval).toBe(true);
  });

  /**
   * Onay talebi **tanimda** oldugu icin hicbir mod onu gevsetemez
   * (`approval-policy.ts`: bir tanim sikilastirabilir, gevsetemez). Ileride
   * risk 1'i otomatik geciren bir mod eklenirse bu tool sorulmaya devam eder.
   */
  it('her onay modunda onay istiyor', () => {
    const tool = toolWith(() => Promise.resolve(OUTCOME));

    for (const mode of TOOL_APPROVAL_MODES) {
      expect(resolveApproval(tool.risk, tool.requiresApproval, mode)).toBe('needs_approval');
    }
  });

  /**
   * Parametresiz: modelin "hangi programi calistirayim?" diye bir alani
   * olsaydi bu, adi `open_project` olan bir genel komut calistiricisi olurdu
   * (PROJECT.md Bolum 18 yasagi).
   */
  it('parametre kabul etmiyor', async () => {
    const openProject = vi.fn(() => Promise.resolve(OUTCOME));

    for (const args of [{ path: '/etc' }, { editor: '/bin/sh' }, { projectId: 'x' }]) {
      const result = await executeTool(toolWith(openProject), args, CONTEXT, {
        approvalGate: () => Promise.resolve('approved'),
      });
      expect(result.ok).toBe(false);
    }
    expect(openProject).not.toHaveBeenCalled();
  });

  it('aciklamasi modelin secim yapamayacagini soyluyor', () => {
    const tool = toolWith(() => Promise.resolve(OUTCOME));
    expect(tool.description).toContain('SEN SECEMEZSIN');
  });
});

describe('open_project — onay akisi', () => {
  /** **ASU-055 kabul kriteri**: reddedilince proje acilmiyor. */
  it('onaylanmadiginda komut hic cagrilmiyor', async () => {
    const openProject = vi.fn(() => Promise.resolve(OUTCOME));
    const audits: ToolAuditInput[] = [];

    const result = await executeTool(toolWith(openProject), {}, CONTEXT, {
      approvalMode: 'safe',
      approvalGate: () => Promise.resolve('denied'),
      onAudit: (input): void => void audits.push(input),
    });

    expect(openProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    expect(audits[0]?.approvalState).toBe('denied');
    expect(audits[0]?.outcome).toBe('not_run');
    expect(audits[0]?.riskLevel).toBe(1);
  });

  /** Onay kanali baglanmamissa da calismaz — varsayilan calistirmamak. */
  it('onay kanali yoksa calismiyor', async () => {
    const openProject = vi.fn(() => Promise.resolve(OUTCOME));

    const result = await executeTool(toolWith(openProject), {}, CONTEXT, {
      approvalMode: 'safe',
    });

    expect(openProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });

  it('onaylandiginda aciliyor ve deftere `succeeded` yaziliyor', async () => {
    const audits: ToolAuditInput[] = [];

    const result = await executeTool(
      toolWith(() => Promise.resolve(OUTCOME)),
      {},
      CONTEXT,
      {
        approvalMode: 'safe',
        approvalGate: () => Promise.resolve('approved'),
        onAudit: (input): void => void audits.push(input),
      },
    );

    expect(result.ok).toBe(true);
    expect(result.summary).toContain('Asuna projesi');
    expect(result.summary).toContain('code');
    expect(audits[0]?.approvalState).toBe('approved');
    expect(audits[0]?.outcome).toBe('succeeded');
    expect(audits[0]?.resultSummary).toBe('Asuna projesi code ile acildi');
  });
});

describe('open_project — durust hata', () => {
  async function refuse(error: unknown): Promise<{
    ok: boolean;
    summary: string;
    errorKind: string;
    audit: ToolAuditInput | undefined;
  }> {
    const audits: ToolAuditInput[] = [];
    const result = await executeTool(toolWith(rejectWith(error)), {}, CONTEXT, {
      approvalGate: () => Promise.resolve('approved'),
      onAudit: (input): void => void audits.push(input),
    });
    return {
      ok: result.ok,
      summary: result.summary,
      errorKind: result.ok ? '' : result.errorKind,
      audit: audits[0],
    };
  }

  /** **ASU-052 kabul kriteri** + PROJECT.md Bolum 30'un ornek cumlesi. */
  it('editor bulunamadiginda "actim" demiyor', async () => {
    const refusal = await refuse({
      code: 'editor_not_found',
      message: '`code` komutu bulunamadi; editor acilamadi',
      auditSummary: 'acilmadi (editor_not_found): `code` komutu bulunamadi',
    });

    expect(refusal.ok).toBe(false);
    expect(refusal.errorKind).toBe('editor_not_found');
    expect(refusal.summary).toContain('PROJE ACILMADI');
    expect(refusal.summary).toContain('"Actim" DEME');
    // Calisti ve yapamadi: `not_run` degil `failed`.
    expect(refusal.audit?.outcome).toBe('failed');
  });

  it('proje secilmemisken kullaniciya sorulmasini soyler', async () => {
    const refusal = await refuse({
      code: 'no_current_project',
      message: 'guncel proje secilmemis',
      auditSummary: 'acilmadi (no_current_project): guncel proje secilmemis',
    });

    expect(refusal.errorKind).toBe('no_current_project');
    expect(refusal.summary).toContain('sor');
  });

  it('cozulemeyen hatada neden uydurmuyor', async () => {
    const refusal = await refuse(new Error('beklenmedik'));

    expect(refusal.errorKind).toBe('open_failed');
    expect(refusal.summary).toContain('nedeni cozulemedi');
    expect(refusal.audit?.resultSummary).toContain('unknown');
  });

  /**
   * Komut hata firlatmadi ama yanit taninmiyor: "acildi" demek bir **tahmin**
   * olurdu. Acilmis olabilecegini soyluyoruz, iddia etmiyoruz.
   */
  it('bozuk yanitta acildigini iddia etmiyor', async () => {
    const result = await executeTool(
      toolWith(() => Promise.resolve({ projectId: 'asuna' })),
      {},
      CONTEXT,
      { approvalGate: () => Promise.resolve('approved') },
    );

    expect(result.ok).toBe(false);
    expect(result.summary).toContain('dogrulayamiyorum');
  });
});

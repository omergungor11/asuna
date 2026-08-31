/**
 * `register_project` tool testleri (ASU-069).
 *
 * Kanitlanan seyler:
 * 1. Tanim: risk 1 + tanimin **kendi** onay talebi (mod gevsetemez).
 * 2. Onaylanmadiginda komut **hic** cagrilmiyor — sandbox yuzeyi buyumuyor.
 * 3. Onay karti yolu gosterebilsin diye sema alani `path`.
 * 4. Host reddi (sistem dizini, `~`, yok) modele oldugu gibi tasiniyor ve
 *    "kaydettim" denmiyor.
 * 5. Kayit guncel projeyi degistirmiyor ve ozet bunu soyluyor.
 */

import { describe, expect, it, vi } from 'vitest';

import {
  createRegisterProjectTool,
  summariseOutcome,
  REGISTER_PROJECT_TOOL_NAME,
} from './register-project';
import { resolveApproval } from './approval-policy';
import { executeTool, ToolRegistry, ToolRegistryError } from './registry';
import { toApprovalArgumentsPreview } from '../agent/realtime-service';
import type { ToolContext } from './types';
import type { ToolAuditInput } from '../../shared/tool-event';
import type { ProjectAddOutcome } from '../../shared/project';
import { TOOL_APPROVAL_MODES } from '../config/frontend-config';

const CONTEXT: ToolContext = { sessionId: null, projectRoot: null };

const PROJECT = {
  id: 'freelancer',
  name: 'freelancer',
  path: '/Users/deneme/Work/freelancer',
  description: null,
  status: 'active',
  primaryLanguage: null,
  framework: null,
  gitRemote: null,
  lastOpenedAt: null,
  createdAt: '2026-08-31T10:00:00Z',
  updatedAt: '2026-08-31T10:00:00Z',
  metadataJson: '{}',
};

const REGISTERED = { status: 'registered', project: PROJECT };

function toolWith(
  registerProject: (path: string) => Promise<unknown>,
): ReturnType<typeof createRegisterProjectTool> {
  return createRegisterProjectTool({ registerProject });
}

function rejectWith(error: unknown): (path: string) => Promise<unknown> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- Tauri duz nesne reddeder
  return (): Promise<unknown> => Promise.reject(error);
}

const APPROVED = { approvalGate: (): Promise<'approved'> => Promise.resolve('approved') };

describe('register_project — tanim', () => {
  /**
   * **Gate 3 M3**: risk 1 degil **risk 2**. Islem geri alinabilir ama
   * okunabilir alani kalici genisletiyor.
   */
  it('risk 2 ve tanimin kendisi onay istiyor', () => {
    const tool = toolWith(() => Promise.resolve(REGISTERED));

    expect(tool.name).toBe(REGISTER_PROJECT_TOOL_NAME);
    expect(tool.risk).toBe(2);
    expect(tool.requiresApproval).toBe(true);
  });

  /** Sandbox yuzeyini genisleten bir tool hicbir modda otomatik gecmemeli. */
  it('her onay modunda onay istiyor', () => {
    const tool = toolWith(() => Promise.resolve(REGISTERED));

    for (const mode of TOOL_APPROVAL_MODES) {
      expect(resolveApproval(tool.risk, tool.requiresApproval, mode)).toBe('needs_approval');
    }
  });

  /**
   * Risk 2'nin somut kazanci (Gate 3 M3): onay talebi tanimdan silinirse
   * registry tool'u **kayit etmez**. Risk 1'de bu koruma yoktu ve sessizce
   * onaysiz bir yetki genislemesi mumkun olurdu.
   */
  it('onay talebi silinirse registry tool\'u kabul etmiyor', () => {
    const registry = new ToolRegistry();
    const tool = { ...toolWith(() => Promise.resolve(REGISTERED)), requiresApproval: false };

    expect(() => {
      registry.register(tool);
    }).toThrow(ToolRegistryError);
  });

  /** Risk 2 mod tablosuna **bakilmadan** onay dondurur (ikinci kilit). */
  it('onay talebi olmasa bile risk 2 tek basina onay istiyor', () => {
    for (const mode of TOOL_APPROVAL_MODES) {
      expect(resolveApproval(2, false, mode)).toBe('needs_approval');
    }
  });

  it('semada `path` disinda alan kabul etmiyor', async () => {
    const registerProject = vi.fn(() => Promise.resolve(REGISTERED));

    for (const args of [
      { path: '/tmp/x', name: 'uydurma-ad' },
      { path: '/tmp/x', setCurrent: true },
      { name: 'ad' },
      { path: '' },
    ]) {
      const result = await executeTool(toolWith(registerProject), args, CONTEXT, APPROVED);
      expect(result.ok).toBe(false);
    }
    expect(registerProject).not.toHaveBeenCalled();
  });

  /**
   * **ASU-069 kabul kriteri**: onay kartinda yol net gorunmeli. Kartin metnini
   * `toApprovalArgumentsPreview` uretiyor; alan adinin `path` olmasi bunu
   * `path=/Users/...` haline getiriyor.
   */
  it('onay karti yolu acikca gosteriyor', () => {
    const preview = toApprovalArgumentsPreview(
      JSON.stringify({ path: '/Users/deneme/Work/freelancer' }),
    );

    expect(preview).toBe('path=/Users/deneme/Work/freelancer');
  });

  /**
   * **Gate 3 M1**: 64 karakterlik eski deger tavani derin bir proje yolunu tam
   * da **sonundan** kesiyordu; kullanici ne onayladigini goremezdi.
   */
  it('uzun bir yolu onay kartinda anlamli birakiyor', () => {
    const path =
      '/Users/deneme/Work-Restored/monorepo-cok-uzun-bir-ad/paketler/servisler/worker-uygulamasi';
    expect(path.length).toBeGreaterThan(64);

    const preview = toApprovalArgumentsPreview(JSON.stringify({ path }));

    expect(preview).toBe(`path=${path}`);
    expect(preview).toContain('worker-uygulamasi');
  });

  /** Cok daha uzun bir yolda bile **son** bilesenler korunuyor (ortadan kirpma). */
  it('asiri uzun yolu ortadan kirpiyor, sonunu koruyor', () => {
    const path = `/Users/deneme/${'a'.repeat(300)}/paketler/worker`;

    const preview = toApprovalArgumentsPreview(JSON.stringify({ path }));

    expect(preview).toContain('/Users/deneme/');
    expect(preview).toContain('…');
    expect(preview).toContain('paketler/worker');
  });

  it('aciklamasi yol uydurmayi ve kendi basina eklemeyi yasakliyor', () => {
    const { description } = toolWith(() => Promise.resolve(REGISTERED));
    expect(description).toContain('UYDURMA');
    expect(description).toContain('kendi basina');
  });
});

describe('register_project — onay akisi', () => {
  it('onaylanmadiginda komut hic cagrilmiyor', async () => {
    const registerProject = vi.fn(() => Promise.resolve(REGISTERED));
    const audits: ToolAuditInput[] = [];

    const result = await executeTool(
      toolWith(registerProject),
      { path: '/Users/deneme/Work/freelancer' },
      CONTEXT,
      {
        approvalMode: 'safe',
        approvalGate: () => Promise.resolve('denied'),
        onAudit: (input): void => void audits.push(input),
      },
    );

    expect(registerProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
    expect(audits[0]?.approvalState).toBe('denied');
    expect(audits[0]?.outcome).toBe('not_run');
    expect(audits[0]?.riskLevel).toBe(2);
  });

  it('onay kanali yoksa calismiyor', async () => {
    const registerProject = vi.fn(() => Promise.resolve(REGISTERED));

    const result = await executeTool(toolWith(registerProject), { path: '/tmp/x' }, CONTEXT, {
      approvalMode: 'safe',
    });

    expect(registerProject).not.toHaveBeenCalled();
    expect(result.ok).toBe(false);
  });

  it('onaylandiginda kaydediyor ve deftere `succeeded` yaziyor', async () => {
    const audits: ToolAuditInput[] = [];
    const registerProject = vi.fn(() => Promise.resolve(REGISTERED));

    const result = await executeTool(
      toolWith(registerProject),
      { path: '/Users/deneme/Work/freelancer' },
      CONTEXT,
      { approvalMode: 'safe', ...APPROVED, onAudit: (i): void => void audits.push(i) },
    );

    expect(registerProject).toHaveBeenCalledWith('/Users/deneme/Work/freelancer');
    expect(result.ok).toBe(true);
    expect(audits[0]?.approvalState).toBe('approved');
    expect(audits[0]?.outcome).toBe('succeeded');
    expect(audits[0]?.resultSummary).toBe('proje kaydedildi: freelancer');
  });
});

describe('register_project — sonuc', () => {
  it('kayit guncel projeyi degistirmedigini soyluyor', () => {
    const outcome = { status: 'registered', project: PROJECT } as unknown as ProjectAddOutcome;
    const summary = summariseOutcome(outcome);

    expect(summary).toContain('kaydedildi');
    expect(summary).toContain('Guncel proje DEGISMEDI');
    expect(summary).toContain('set_current_project');
  });

  /** Cift kayit bir hata degil ama "ekledim" de degil. */
  it('zaten kayitliysa "ekledim" demiyor', () => {
    const outcome = {
      status: 'already-registered',
      project: PROJECT,
    } as unknown as ProjectAddOutcome;
    const summary = summariseOutcome(outcome);

    expect(summary).toContain('ZATEN KAYITLIYDI');
    expect(summary).toContain('"ekledim" deme');
  });
});

describe('register_project — durust ret', () => {
  async function refuse(error: unknown): Promise<{
    ok: boolean;
    summary: string;
    errorKind: string;
    audit: ToolAuditInput | undefined;
  }> {
    const audits: ToolAuditInput[] = [];
    const result = await executeTool(toolWith(rejectWith(error)), { path: '/x' }, CONTEXT, {
      ...APPROVED,
      onAudit: (input): void => void audits.push(input),
    });
    return {
      ok: result.ok,
      summary: result.summary,
      errorKind: result.ok ? '' : result.errorKind,
      audit: audits[0],
    };
  }

  /**
   * Host reddi (sistem dizini, `~`, ev dizini, blok listesi) hep
   * `path-refused` ile gelir — hepsinde "kaydettim" yasak.
   */
  it('yol reddedildiginde kaydettigini iddia etmiyor', async () => {
    const refusal = await refuse({
      code: 'path-refused',
      message: 'yol kabul edilmedi: hassas dizin icerigi okunmaz',
    });

    expect(refusal.errorKind).toBe('path_refused');
    expect(refusal.summary).toContain('KAYDEDILMEDI');
    expect(refusal.summary).toContain('IDDIA ETME');
    expect(refusal.audit?.resultSummary).toBe('kaydedilmedi (path-refused)');
    // Defter yol tasimaz.
    expect(refusal.audit?.resultSummary).not.toContain('/');
    expect(refusal.audit?.outcome).toBe('failed');
  });

  it('var olmayan dizini ayri kodla sunuyor', async () => {
    const refusal = await refuse({
      code: 'path-not-found',
      message: 'verilen yol bulunamadi ya da erisilemiyor',
    });

    expect(refusal.errorKind).toBe('path_not_found');
    expect(refusal.summary).toContain('UYDURMA');
  });

  it('dosya verildiginde dizin gerektigini soyluyor', async () => {
    const refusal = await refuse({
      code: 'not-a-directory',
      message: 'verilen yol bir dizin degil',
    });

    expect(refusal.errorKind).toBe('not_a_directory');
    expect(refusal.summary).toContain('KLASOR');
  });

  it('depolama kapaliyken kaydettigini iddia etmiyor', async () => {
    const refusal = await refuse({
      code: 'disabled',
      message: 'kalici depolama kapali; proje kaydi tutulamiyor',
    });

    expect(refusal.errorKind).toBe('disabled');
    expect(refusal.summary).toContain('iddia etme');
  });

  it('bozuk yanitta kaydedildigini dogrulayamiyorum diyor', async () => {
    const result = await executeTool(
      toolWith(() => Promise.resolve({ status: 'registered' })),
      { path: '/x' },
      CONTEXT,
      APPROVED,
    );

    expect(result.ok).toBe(false);
    expect(result.summary).toContain('dogrulayamiyorum');
  });
});

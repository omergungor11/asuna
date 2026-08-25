import { describe, expect, it } from 'vitest';

import type { ToolRisk } from '../asuna/tools/types';
import {
  TOOL_APPROVAL_STATES,
  TOOL_RISK_LEVELS,
  ToolEventContractError,
  parseToolEventPage,
  parseToolEventRecord,
  parseToolEventWriteResult,
  riskAlwaysRequiresApproval,
  toolCallWasPermitted,
  type ToolApprovalState,
  type ToolEventRecord,
  type ToolRiskLevel,
} from './tool-event';

const RECORD: ToolEventRecord = {
  id: 7,
  sessionId: 3,
  toolName: 'open_project',
  riskLevel: 1,
  argumentsRedacted: 'projectId=asuna',
  approvalState: 'approved',
  resultSummary: 'Proje VS Code ile acildi.',
  createdAt: '2026-08-25T10:01:00Z',
};

/** `ToolRisk` (tool tanimi) ile `ToolRiskLevel` (sema aynasi) ayni kume olmali. */
const RISK_TYPES_ARE_COMPATIBLE: ToolRisk = 3 satisfies ToolRiskLevel;

describe('parseToolEventRecord', () => {
  it('gecerli bir audit satirini okur', () => {
    expect(parseToolEventRecord({ ...RECORD })).toEqual(RECORD);
  });

  it('argumansiz ve sonucsuz bir cagriyi null olarak tasir', () => {
    const parsed = parseToolEventRecord({
      ...RECORD,
      sessionId: null,
      argumentsRedacted: null,
      resultSummary: null,
    });
    expect(parsed.argumentsRedacted).toBeNull();
    expect(parsed.resultSummary).toBeNull();
    // Oturum bilinmiyorsa uydurulmaz.
    expect(parsed.sessionId).toBeNull();
  });

  it('bilinmeyen onay durumunu reddeder', () => {
    expect(() => parseToolEventRecord({ ...RECORD, approvalState: 'onaylandi' })).toThrow(
      ToolEventContractError,
    );
  });

  it('aralik disi risk seviyesini reddeder', () => {
    for (const riskLevel of [-1, 4, 1.5, '1', null]) {
      expect(() => parseToolEventRecord({ ...RECORD, riskLevel })).toThrow(
        ToolEventContractError,
      );
    }
  });

  /**
   * Backend bir gun yanlislikla fazladan bir alan dondurse (orn. ham arguman),
   * bu sessizce renderer'a akmak yerine gurultulu bir hataya donusur.
   */
  it('sozlesmede olmayan alani reddeder', () => {
    expect(() =>
      parseToolEventRecord({ ...RECORD, argumentsRaw: '{"path":"/tmp/.env"}' }),
    ).toThrow(ToolEventContractError);
  });

  it('zaman damgasini UTC ISO-8601 olarak zorunlu tutar', () => {
    expect(() => parseToolEventRecord({ ...RECORD, createdAt: '25/08/2026' })).toThrow(
      ToolEventContractError,
    );
  });

  /** Hata mesaji gelen degeri tekrarlamaz (`contract.ts` guvenlik kurali). */
  it('hata mesajinda alan degerini tekrarlamaz', () => {
    const secret = 'sk-proj-SIZMAMALI';
    try {
      parseToolEventRecord({ ...RECORD, createdAt: secret });
      expect.unreachable('hata bekleniyordu');
    } catch (error) {
      expect((error as Error).message).not.toContain(secret);
    }
  });
});

describe('parseToolEventPage', () => {
  it('sayfa sinirlarini oldugu gibi tasir', () => {
    const page = parseToolEventPage({
      events: [{ ...RECORD }],
      limit: 50,
      limitMax: 200,
      total: 214,
    });
    expect(page.events).toHaveLength(1);
    // "50 / 214" demek, "en yeni 50" demekten durust.
    expect(page.total).toBe(214);
    expect(page.limitMax).toBe(200);
  });

  it('bos defteri kabul eder', () => {
    const page = parseToolEventPage({ events: [], limit: 50, limitMax: 200, total: 0 });
    expect(page.events).toEqual([]);
    expect(page.total).toBe(0);
  });

  it('dizi olmayan govdeyi reddeder', () => {
    expect(() =>
      parseToolEventPage({ events: {}, limit: 50, limitMax: 200, total: 0 }),
    ).toThrow(ToolEventContractError);
  });
});

describe('parseToolEventWriteResult', () => {
  it('yazilan kaydi dondurur', () => {
    const result = parseToolEventWriteResult({ status: 'recorded', event: { ...RECORD } });
    expect(result).toEqual({ status: 'recorded', event: RECORD });
  });

  /** Kapali hafiza bir hata degil ama "kaydettim" de degil. */
  it('atlanmis yazmayi acikca isaretler', () => {
    expect(parseToolEventWriteResult({ status: 'skipped', reason: 'memory-disabled' })).toEqual(
      {
        status: 'skipped',
        reason: 'memory-disabled',
      },
    );
  });

  it('bilinmeyen durumu reddeder', () => {
    expect(() => parseToolEventWriteResult({ status: 'deleted', id: 1 })).toThrow(
      ToolEventContractError,
    );
  });
});

describe('onay durumu yardimcilari', () => {
  /**
   * Rust `ToolApprovalState::permitted_execution` ile ayni kume. Ikisi ayrisirsa
   * UI "reddedildi" bir cagriyi "calisti" gibi gosterirdi.
   */
  it('yalnizca uc durum tool"un calistigi anlamina gelir', () => {
    const permitted = TOOL_APPROVAL_STATES.filter(toolCallWasPermitted);
    expect(permitted).toEqual(['not_required', 'auto_approved', 'approved']);
  });

  it('reddedilen, zaman asimina ugrayan ve sorulmayan cagrilar calismamis sayilir', () => {
    for (const state of ['denied', 'timeout', 'not_requested'] satisfies ToolApprovalState[]) {
      expect(toolCallWasPermitted(state)).toBe(false);
    }
  });

  /**
   * `security.md` Bolum 3: risk 2/3 hicbir `ASUNA_TOOL_APPROVAL_MODE` degeriyle
   * gevsetilemez. Ayni tanim Rust `ToolRiskLevel::always_requires_approval`.
   */
  it('risk 2 ve 3 her zaman acik onay ister', () => {
    expect(TOOL_RISK_LEVELS.filter(riskAlwaysRequiresApproval)).toEqual([2, 3]);
  });

  it('tool tanimindaki ToolRisk ile sema aynasi ayni kumeyi tasir', () => {
    expect(RISK_TYPES_ARE_COMPATIBLE).toBe(3);
    expect([...TOOL_RISK_LEVELS]).toEqual([0, 1, 2, 3]);
  });
});

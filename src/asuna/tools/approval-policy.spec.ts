/**
 * Onay politikasi testleri (ASU-048).
 *
 * Kabul kriteri "her risk seviyesi x her mod matrisi" diyor; bu dosya matrisi
 * **tablo olarak** yaziyor ve tabloyu tek tek dogruluyor. Neden tablo: politika
 * bir if zinciri degil bir sozlesme; birinin `resolveApproval` icinde bir satiri
 * gevsetmesi burada tek bir kirmizi test olarak gorunmeli, uc ayri testin
 * yorumundan cikarilmamali.
 */

import { describe, expect, it } from 'vitest';

import {
  approvalStateFor,
  resolveApproval,
  type ApprovalDecision,
  type ApprovalOutcome,
} from './approval-policy';
import type { ToolRisk } from './types';
import { TOOL_APPROVAL_MODES } from '../config/frontend-config';
import { TOOL_APPROVAL_STATES, toolCallWasPermitted } from '../../shared/tool-event';

const RISK_LEVELS: readonly ToolRisk[] = [0, 1, 2, 3];

interface MatrixRow {
  readonly risk: ToolRisk;
  readonly requiresApproval: boolean;
  readonly safe: ApprovalDecision;
  readonly always: ApprovalDecision;
}

/**
 * `approval-policy.ts` modul dokumantasyonundaki matrisin birebir kopyasi.
 *
 * Iki yerde durmasinin sebebi: dokuman insanlar icin, tablo derleyici icin.
 * Ikisi ayrildiginda testin kirmizi olmasi lazim, sessizce eskimesi degil.
 */
const MATRIX: readonly MatrixRow[] = [
  // Risk 0, tanim onay istemiyor: iki modda da onaysiz. `always` modunda bile
  // salt-okuma bir cagri icin kart cikarmiyoruz (onay yorgunlugu).
  { risk: 0, requiresApproval: false, safe: 'not_required', always: 'not_required' },
  // Risk 0 ama tanim "beni sor" diyor: tanimin talebi mod'u gecer.
  { risk: 0, requiresApproval: true, safe: 'needs_approval', always: 'needs_approval' },
  // Risk 1: phase-5.md "safe modda onay ister"; `always` daha gevsek olamaz.
  { risk: 1, requiresApproval: false, safe: 'needs_approval', always: 'needs_approval' },
  { risk: 1, requiresApproval: true, safe: 'needs_approval', always: 'needs_approval' },
  // Risk 2/3: **her zaman** onay. `requiresApproval: false` bir tanim hatasidir
  // (registry kayit aninda reddeder) ve burada da gevsetme uretmez.
  { risk: 2, requiresApproval: true, safe: 'needs_approval', always: 'needs_approval' },
  { risk: 2, requiresApproval: false, safe: 'needs_approval', always: 'needs_approval' },
  { risk: 3, requiresApproval: true, safe: 'needs_approval', always: 'needs_approval' },
  { risk: 3, requiresApproval: false, safe: 'needs_approval', always: 'needs_approval' },
];

describe('resolveApproval — risk x mod matrisi', () => {
  for (const row of MATRIX) {
    for (const mode of TOOL_APPROVAL_MODES) {
      const expected = row[mode];
      it(`risk ${row.risk.toString()} · requiresApproval=${String(row.requiresApproval)} · ${mode} -> ${expected}`, () => {
        expect(resolveApproval(row.risk, row.requiresApproval, mode)).toBe(expected);
      });
    }
  }

  /** Matris testinin gercekten her kombinasyonu kapsadigi. */
  it('matris her risk seviyesini ve her modu iceriyor', () => {
    expect(new Set(MATRIX.map((row) => row.risk))).toEqual(new Set(RISK_LEVELS));
    expect(TOOL_APPROVAL_MODES).toEqual(['safe', 'always']);
    // Her risk icin `requiresApproval`in iki hali de var.
    for (const risk of RISK_LEVELS) {
      const flags = MATRIX.filter((row) => row.risk === risk).map(
        (row) => row.requiresApproval,
      );
      expect(new Set(flags)).toEqual(new Set([true, false]));
    }
  });
});

describe('resolveApproval — pazarliksiz kurallar', () => {
  /**
   * `security.md` Bolum 3 / `conventions.md`: `ASUNA_TOOL_APPROVAL_MODE` risk
   * 2/3'u bypass edemez. Mod kumesi buyurse bile bu testin gecmesi gerekir.
   */
  it('risk 2 ve 3 hicbir modda onaysiz calismiyor', () => {
    for (const mode of TOOL_APPROVAL_MODES) {
      for (const risk of [2, 3] as const) {
        expect(resolveApproval(risk, true, mode)).toBe('needs_approval');
        expect(resolveApproval(risk, false, mode)).toBe('needs_approval');
      }
    }
  });

  it('risk 0 salt-okuma cagri iki modda da onaysiz calisiyor', () => {
    for (const mode of TOOL_APPROVAL_MODES) {
      expect(resolveApproval(0, false, mode)).toBe('not_required');
    }
  });

  /**
   * Bugun `safe` ve `always` ayni sonucu uretiyor ve bu **dokumante edilmis**
   * bir durum (bkz. `approval-policy.ts` modul yorumu). Test bunu bir surpriz
   * degil bir olcum olarak sabitliyor: fark, gevsetici bir mod eklendiginde
   * ortaya cikacak.
   */
  it('mevcut iki mod kayitli risk seviyelerinde ayni karari veriyor', () => {
    for (const risk of RISK_LEVELS) {
      for (const requiresApproval of [true, false]) {
        expect(resolveApproval(risk, requiresApproval, 'safe')).toBe(
          resolveApproval(risk, requiresApproval, 'always'),
        );
      }
    }
  });
});

describe('approvalStateFor — audit etiketleri', () => {
  it('onay gerekmeyen risk 0 cagri `not_required` yaziyor', () => {
    expect(approvalStateFor(0, 'not_required', null)).toBe('not_required');
  });

  /**
   * Risk >= 1 onaysiz gecerse bu "gerekmiyordu" degil "ayar izin verdi"dir.
   * Mevcut mod kumesiyle bu yola girilmiyor; etiket yine de dogru olmali ki
   * gevsetici bir mod eklendiginde audit yalan soylemesin.
   */
  it('risk >= 1 onaysiz gecerse `auto_approved` yaziyor', () => {
    expect(approvalStateFor(1, 'not_required', null)).toBe('auto_approved');
    expect(approvalStateFor(2, 'not_required', null)).toBe('auto_approved');
  });

  it('onay soruldu ve cevaplandiysa cevap oldugu gibi yaziliyor', () => {
    const outcomes: readonly ApprovalOutcome[] = ['approved', 'denied', 'timeout'];
    for (const outcome of outcomes) {
      expect(approvalStateFor(2, 'needs_approval', outcome)).toBe(outcome);
    }
  });

  /** Onay gerekiyordu ama sorulamadi — `not_required` ile karistirilmamali. */
  it('onay sorulamadiysa `not_requested` yaziliyor', () => {
    expect(approvalStateFor(2, 'needs_approval', null)).toBe('not_requested');
  });

  it('urettigi her deger sema kumesinde ve calisma izni dogru', () => {
    const produced = [
      approvalStateFor(0, 'not_required', null),
      approvalStateFor(1, 'not_required', null),
      approvalStateFor(2, 'needs_approval', 'approved'),
      approvalStateFor(2, 'needs_approval', 'denied'),
      approvalStateFor(2, 'needs_approval', 'timeout'),
      approvalStateFor(2, 'needs_approval', null),
    ];

    for (const state of produced) {
      expect(TOOL_APPROVAL_STATES).toContain(state);
    }
    // Yalnizca ilk uc etiket "tool calisti" demek.
    expect(produced.map(toolCallWasPermitted)).toEqual([true, true, true, false, false, false]);
  });
});

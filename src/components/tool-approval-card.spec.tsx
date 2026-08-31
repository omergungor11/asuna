/**
 * `ToolApprovalCard` testleri (ASU-053).
 *
 * Kanitlanan seyler:
 * 1. Kart onay icin gereken her seyi gosterir: ad, ne yapacagi, risk, redakte
 *    edilmis argumanlar, kalan sure.
 * 2. Karar **`requestId` ile** verilir — "sonuncuyu onayla" yok.
 * 3. Klavye guvenli yonde: odak "Reddet"e gelir, onaylayan kisayol yoktur.
 * 4. Geri sayim biter ama UI **hicbir sey cagirmaz**: otomatik reddetme serviste.
 * 5. Istek cozulunce kart kapanir ve kisa bir bildirim kalir.
 */

import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';

import type { PendingToolApproval } from '../asuna/agent/use-asuna-session';

import { DECISION_NOTICE_MS, ToolApprovalCard } from './tool-approval-card';

const START_MS = 1_000_000;

const REQUEST: PendingToolApproval = {
  requestId: 'req-42',
  toolName: 'open_project',
  description: 'Kayıtlı bir projeyi yapılandırılmış editörde açar.',
  risk: 1,
  argumentsPreview: 'projectId=asuna',
  timeoutMs: 60_000,
  requestedAtMs: START_MS,
};

let clock = START_MS;
const now = (): number => clock;

/** Sahte saati ve zamanlayicilari birlikte ilerletir. */
function advance(ms: number): void {
  act(() => {
    clock += ms;
    vi.advanceTimersByTime(ms);
  });
}

type DecisionMock = Mock<(requestId: string) => void>;

interface Handlers {
  readonly onApprove: DecisionMock;
  readonly onReject: DecisionMock;
}

function handlers(): Handlers {
  return {
    onApprove: vi.fn<(requestId: string) => void>(),
    onReject: vi.fn<(requestId: string) => void>(),
  };
}

function renderCard(
  approval: PendingToolApproval | null,
  hooks: Handlers,
): ReturnType<typeof render> {
  return render(
    <ToolApprovalCard
      approval={approval}
      onApprove={hooks.onApprove}
      onReject={hooks.onReject}
      now={now}
    />,
  );
}

beforeEach(() => {
  clock = START_MS;
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('ToolApprovalCard', () => {
  it('bekleyen istek yokken hicbir sey cizmez', () => {
    const { container } = renderCard(null, handlers());

    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('onay icin gereken her seyi gosterir', () => {
    renderCard(REQUEST, handlers());

    const card = screen.getByRole('dialog', { name: 'Araç onayı' });
    expect(card).toHaveTextContent('open_project');
    expect(card).toHaveTextContent('Kayıtlı bir projeyi yapılandırılmış editörde açar.');
    expect(card).toHaveTextContent('Risk 1 · geri alınabilir');
    expect(card).toHaveTextContent('projectId=asuna');
    expect(card).toHaveTextContent('Kalan süre: 60 sn');
    expect(card).toHaveAttribute('data-risk', '1');
  });

  it('risk bilinmiyorsa bunu gizlemez', () => {
    renderCard({ ...REQUEST, risk: null, argumentsPreview: null }, handlers());

    expect(screen.getByRole('dialog')).toHaveTextContent('Risk bilinmiyor');
    expect(screen.getByRole('dialog')).toHaveTextContent('Argümansız çağrı');
  });

  it('karari requestId ile verir', () => {
    const hooks = handlers();
    renderCard(REQUEST, hooks);

    fireEvent.click(screen.getByRole('button', { name: 'Onayla: open_project' }));

    expect(hooks.onApprove).toHaveBeenCalledExactlyOnceWith('req-42');
    expect(hooks.onReject).not.toHaveBeenCalled();
  });

  it('reddetme de requestId ile gider', () => {
    const hooks = handlers();
    renderCard(REQUEST, hooks);

    fireEvent.click(screen.getByRole('button', { name: 'Reddet: open_project' }));

    expect(hooks.onReject).toHaveBeenCalledExactlyOnceWith('req-42');
    expect(hooks.onApprove).not.toHaveBeenCalled();
  });

  it('ayni istek icin ikinci karari gondermez', () => {
    const hooks = handlers();
    renderCard(REQUEST, hooks);

    const approve = screen.getByRole('button', { name: 'Onayla: open_project' });
    fireEvent.click(approve);
    fireEvent.click(approve);

    expect(hooks.onApprove).toHaveBeenCalledTimes(1);
    expect(approve).toBeDisabled();
  });

  /**
   * Odak guvenli yone gelir: kart tam kullanici Enter'a basarken acilirsa
   * sonuc onay degil olsa olsa reddir.
   */
  it('kart acilinca odak "Reddet" butonuna gelir', () => {
    renderCard(REQUEST, handlers());

    expect(screen.getByRole('button', { name: 'Reddet: open_project' })).toHaveFocus();
  });

  it('Esc reddeder', () => {
    const hooks = handlers();
    renderCard(REQUEST, hooks);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(hooks.onReject).toHaveBeenCalledExactlyOnceWith('req-42');
    expect(hooks.onApprove).not.toHaveBeenCalled();
  });

  /**
   * Kart tam kullanici Enter'a basarken acilabilir. Onaylayan bir kisayol
   * olsaydi risk 1+ bir aksiyon refleksle onaylanirdi — o yuzden **yok**.
   */
  it('kart acilisindaki Enter hicbir seyi onaylamaz', () => {
    const hooks = handlers();
    renderCard(REQUEST, hooks);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Enter' });
    // Odak "Reddet" butonunda: oradaki Enter de onaylamaz.
    fireEvent.keyDown(screen.getByRole('button', { name: 'Reddet: open_project' }), {
      key: 'Enter',
    });

    expect(hooks.onApprove).not.toHaveBeenCalled();
    expect(hooks.onReject).not.toHaveBeenCalled();
    // Kart hala acik: karar verilmedi, kapi kapanmadi.
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('Esc karari acik olan istegin kimligine yazar', () => {
    const hooks = handlers();
    const { rerender } = renderCard(REQUEST, hooks);

    const next: PendingToolApproval = { ...REQUEST, requestId: 'req-43' };
    rerender(
      <ToolApprovalCard
        approval={next}
        onApprove={hooks.onApprove}
        onReject={hooks.onReject}
        now={now}
      />,
    );

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(hooks.onReject).toHaveBeenCalledExactlyOnceWith('req-43');
  });

  it('geri sayimi saniyede bir tazeler', () => {
    renderCard(REQUEST, handlers());

    advance(3000);
    expect(screen.getByRole('dialog')).toHaveTextContent('Kalan süre: 57 sn');

    advance(7000);
    expect(screen.getByRole('dialog')).toHaveTextContent('Kalan süre: 50 sn');
  });

  /** Zaman asimini servis tetikler; UI yalnizca gosterir (ASU-048). */
  it('sure dolunca kendisi reddetmez', () => {
    const hooks = handlers();
    renderCard(REQUEST, hooks);

    advance(90_000);

    expect(screen.getByRole('dialog')).toHaveTextContent('Kalan süre: 0 sn');
    expect(hooks.onReject).not.toHaveBeenCalled();
    expect(hooks.onApprove).not.toHaveBeenCalled();
  });

  it('reddedilen istekten sonra kart kapanir, kisa bildirim kalir', () => {
    const hooks = handlers();
    const { rerender } = renderCard(REQUEST, hooks);

    fireEvent.click(screen.getByRole('button', { name: 'Reddet: open_project' }));
    rerender(
      <ToolApprovalCard
        approval={null}
        onApprove={hooks.onApprove}
        onReject={hooks.onReject}
        now={now}
      />,
    );

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('status')).toHaveTextContent('Reddettin — araç çalıştırılmadı.');

    // Bildirim gecici: transcript'e girmez, ekranda da asili kalmaz.
    advance(DECISION_NOTICE_MS + 100);
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('karar verilmeden sure dolduysa zaman asimi der', () => {
    const hooks = handlers();
    const { rerender } = renderCard(REQUEST, hooks);

    advance(REQUEST.timeoutMs + 500);
    rerender(
      <ToolApprovalCard
        approval={null}
        onApprove={hooks.onApprove}
        onReject={hooks.onReject}
        now={now}
      />,
    );

    expect(screen.getByRole('status')).toHaveTextContent(
      'Onay süresi doldu — istek reddedildi, araç çalıştırılmadı.',
    );
  });

  it('sure dolmadan kapanan istek icin basari taklidi yapmaz', () => {
    const hooks = handlers();
    const { rerender } = renderCard(REQUEST, hooks);

    advance(2000);
    rerender(
      <ToolApprovalCard
        approval={null}
        onApprove={hooks.onApprove}
        onReject={hooks.onReject}
        now={now}
      />,
    );

    expect(screen.getByRole('status')).toHaveTextContent(
      'Onay isteği kapandı — araç çalıştırılmadı.',
    );
  });

  it('arguman ozeti duz metin olarak basilir', () => {
    renderCard(
      { ...REQUEST, argumentsPreview: 'path=<img src=x onerror=alert(1)>' },
      handlers(),
    );

    expect(screen.getByText('path=<img src=x onerror=alert(1)>')).toBeInTheDocument();
    expect(screen.getByRole('dialog').querySelector('img')).toBeNull();
  });
});

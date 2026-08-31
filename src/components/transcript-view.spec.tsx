/**
 * `TranscriptView` testleri (ASU-017).
 */

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type {
  SpokenTranscriptLine,
  ToolTranscriptLine,
} from '../asuna/agent/use-asuna-session';

import { TranscriptView, VISIBLE_LINE_COUNT } from './transcript-view';

/** Tool satiri (ASU-054) — dokumun `role: 'tool'` varyanti. */
function toolLine(
  overrides: Partial<ToolTranscriptLine> & { readonly itemId: string },
): ToolTranscriptLine {
  return {
    role: 'tool',
    toolName: 'read_project_file',
    text: 'README.md okundu (2 KB).',
    status: 'completed',
    interrupted: false,
    outcome: 'succeeded',
    risk: 0,
    approvalState: 'not_required',
    ...overrides,
  };
}

function line(
  overrides: Partial<SpokenTranscriptLine> & { readonly itemId: string },
): SpokenTranscriptLine {
  return {
    role: 'user',
    text: 'merhaba',
    status: 'completed',
    interrupted: false,
    ...overrides,
  };
}

/** jsdom layout yapmaz; kaydirma olculerini elle tanimliyoruz. */
function stubScrollBox(node: HTMLElement, scrollHeight: number, clientHeight: number): void {
  let scrollTop = 0;
  Object.defineProperty(node, 'scrollHeight', { value: scrollHeight, configurable: true });
  Object.defineProperty(node, 'clientHeight', { value: clientHeight, configurable: true });
  Object.defineProperty(node, 'scrollTop', {
    configurable: true,
    get: (): number => scrollTop,
    set: (value: number): void => {
      scrollTop = value;
    },
  });
}

describe('TranscriptView', () => {
  it('bos dokumde durustce "henuz konusma yok" der', () => {
    render(<TranscriptView lines={[]} />);

    expect(screen.getByText('Henüz konuşma yok.')).toBeInTheDocument();
  });

  it('kullanici ve Asuna repliklerini ayirt eder', () => {
    render(
      <TranscriptView
        lines={[
          line({ itemId: 'u1', role: 'user', text: 'bugün ne yapsam' }),
          line({ itemId: 'a1', role: 'assistant', text: 'şu taskla başla' }),
        ]}
      />,
    );

    const userLine = screen.getByText('bugün ne yapsam').closest('p');
    const assistantLine = screen.getByText('şu taskla başla').closest('p');

    expect(userLine).toHaveAttribute('data-role', 'user');
    expect(userLine).toHaveTextContent('Sen:');
    expect(assistantLine).toHaveAttribute('data-role', 'assistant');
    expect(assistantLine).toHaveTextContent('Asuna:');
  });

  it('kismi transkript gorunur, kesinlesince ayni satir guncellenir', () => {
    const { rerender } = render(
      <TranscriptView lines={[line({ itemId: 'u1', text: 'bugün', status: 'in_progress' })]} />,
    );

    expect(screen.getByText('bugün').closest('p')).toHaveAttribute(
      'data-status',
      'in_progress',
    );

    rerender(
      <TranscriptView
        lines={[line({ itemId: 'u1', text: 'bugün ne yapsam', status: 'completed' })]}
      />,
    );

    expect(screen.queryByText('bugün')).toBeNull();
    expect(screen.getByText('bugün ne yapsam').closest('p')).toHaveAttribute(
      'data-status',
      'completed',
    );
    expect(screen.getAllByRole('paragraph')).toHaveLength(1);
  });

  it('kesilen Asuna cevabini isaretler', () => {
    render(
      <TranscriptView
        lines={[
          line({
            itemId: 'a1',
            role: 'assistant',
            text: 'sana şunu anlatayım',
            status: 'in_progress',
            interrupted: true,
          }),
        ]}
      />,
    );

    expect(screen.getByRole('log')).toHaveTextContent('— kesildi');
  });

  it('uzun oturumda yalnizca son satirlari basar ve gizleneni soyler', () => {
    const lines = Array.from({ length: VISIBLE_LINE_COUNT + 40 }, (_unused, index) =>
      line({ itemId: `i${index.toString()}`, text: `satir ${index.toString()}` }),
    );

    render(<TranscriptView lines={lines} />);

    expect(screen.getAllByRole('paragraph')).toHaveLength(VISIBLE_LINE_COUNT + 1);
    expect(screen.getByText('Önceki 40 satır gizlendi.')).toBeInTheDocument();
    expect(screen.queryByText('satir 0')).toBeNull();
    expect(screen.getByText(`satir ${(lines.length - 1).toString()}`)).toBeInTheDocument();
  });

  it('kullanici yukari kaydirdiysa zorla asagi atmaz', () => {
    const { rerender } = render(
      <TranscriptView lines={[line({ itemId: 'u1', text: 'ilk' })]} />,
    );
    const log = screen.getByRole('log');
    stubScrollBox(log, 1_000, 200);

    // Kullanici yukari kaydirdi.
    log.scrollTop = 100;
    fireEvent.scroll(log);

    rerender(
      <TranscriptView
        lines={[line({ itemId: 'u1', text: 'ilk' }), line({ itemId: 'u2', text: 'ikinci' })]}
      />,
    );

    expect(log.scrollTop).toBe(100);

    // Kullanici tekrar dibe indi: otomatik kaydirma geri gelir.
    log.scrollTop = 800;
    fireEvent.scroll(log);

    rerender(
      <TranscriptView
        lines={[
          line({ itemId: 'u1', text: 'ilk' }),
          line({ itemId: 'u2', text: 'ikinci' }),
          line({ itemId: 'u3', text: 'ucuncu' }),
        ]}
      />,
    );

    expect(log.scrollTop).toBe(1_000);
  });

  /**
   * ASU-054: "tool sonucu (basari/hata + kisa ozet) transcript akisinda
   * gorunuyor". Satir konusma satirlarindan ayirt edilebilir olmali.
   */
  it('tool satirini ayri bir satir olarak gosterir', () => {
    render(
      <TranscriptView
        lines={[
          line({ itemId: 'u1', text: 'readme"de ne yaziyor' }),
          toolLine({ itemId: 't1' }),
        ]}
      />,
    );

    const row = screen.getByText('README.md okundu (2 KB).').closest('p');

    expect(row).toHaveAttribute('data-role', 'tool');
    expect(row).toHaveAttribute('data-outcome', 'succeeded');
    expect(row).toHaveTextContent('Araç · read_project_file:');
    expect(row).toHaveTextContent('başarılı');
  });

  it('basarisiz tool cagrisini basarili gibi gostermez', () => {
    render(
      <TranscriptView
        lines={[
          toolLine({
            itemId: 't2',
            text: 'Dosya bulunamadı.',
            outcome: 'failed',
          }),
        ]}
      />,
    );

    const row = screen.getByText('Dosya bulunamadı.').closest('p');

    expect(row).toHaveAttribute('data-outcome', 'failed');
    expect(row).toHaveTextContent('hata');
  });

  /** Calismayan cagri "basarili" gibi gorunmez (`not_run` != `succeeded`). */
  it('hic calismayan cagriyi ayri etiketler', () => {
    render(
      <TranscriptView
        lines={[
          toolLine({
            itemId: 't3',
            text: 'Onay verilmediği için çalıştırılmadı.',
            outcome: 'not_run',
            approvalState: 'denied',
            risk: 1,
          }),
        ]}
      />,
    );

    const row = screen.getByText('Onay verilmediği için çalıştırılmadı.').closest('p');

    expect(row).toHaveAttribute('data-outcome', 'not_run');
    expect(row).toHaveAttribute('data-risk', '1');
    expect(row).toHaveTextContent('çalışmadı');
  });

  it('tool ciktisi duz metin olarak basilir', () => {
    const { container } = render(
      <TranscriptView lines={[toolLine({ itemId: 't4', text: '<b>enjekte</b>' })]} />,
    );

    expect(screen.getByText('<b>enjekte</b>')).toBeInTheDocument();
    expect(container.querySelector('b')).toBeNull();
  });

  it('sohbet arayuzu degil: metin girisi yok', () => {
    render(<TranscriptView lines={[line({ itemId: 'u1' })]} />);

    expect(screen.queryByRole('textbox')).toBeNull();
    expect(screen.queryAllByRole('button')).toHaveLength(0);
  });
});

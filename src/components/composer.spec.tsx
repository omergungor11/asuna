/**
 * `Composer` testleri (plan-chat-shell.md WP3).
 *
 * Kanitlanan seyler:
 * 1. Enter gonderir, Shift+Enter gondermez (yeni satir acar).
 * 2. Bos / yalnizca bosluktan olusan mesaj gonderilmez ve yazilan kaybolmaz.
 * 3. Gonderilen metin **trim**'lenmis gider; alan gonderimden sonra temizlenir.
 * 4. Bekleyen dosyalar cip olarak gorunur.
 * 5. "Projeden dosya ekle" yalnizca konusma bir projedeyken ve dizin kaynagi
 *    verilmisken cikar.
 * 6. Mikrofon butonu ses moduna gecis sinyali verir — burada kayit baslamaz.
 * 7. Ayni gonderim iki kez gitmez: pes pese Enter / cift tiklama tek istek uretir
 *    ve IME bileseni sirasindaki Enter gondermez (WP4 bosluk analizi).
 */

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { ProjectDirectoryView } from '../asuna/tools/list-project-files';
import type { ChatAttachment } from '../shared/chat';

import { Composer, type ComposerProps } from './composer';

function attachment(overrides: Partial<ChatAttachment> = {}): ChatAttachment {
  return {
    id: 5,
    sessionId: 1,
    messageId: null,
    fileName: 'notlar.md',
    mimeType: 'text/markdown',
    sizeBytes: 2048,
    origin: 'upload',
    createdAt: '2026-08-31T10:00:00Z',
    ...overrides,
  };
}

const EMPTY_DIR: ProjectDirectoryView = {
  projectId: 'asuna',
  projectName: 'Asuna',
  path: '',
  entries: [],
  totalEntries: 0,
  returnedEntries: 0,
  truncated: false,
  scanCapped: false,
  maxEntries: 200,
};

function renderComposer(overrides: Partial<ComposerProps> = {}): ComposerProps {
  const props: ComposerProps = {
    sending: false,
    pendingAttachments: [],
    attaching: false,
    attachError: null,
    projectId: null,
    onSend: vi.fn(),
    onAttachFiles: vi.fn(),
    onAttachProjectFile: vi.fn(),
    ...overrides,
  };

  render(<Composer {...props} />);
  return props;
}

function textarea(): HTMLTextAreaElement {
  return screen.getByRole('textbox', { name: 'Mesaj' });
}

describe('Composer', () => {
  it('Enter gonderir ve alani temizler', () => {
    const props = renderComposer();

    fireEvent.change(textarea(), { target: { value: 'merhaba' } });
    fireEvent.keyDown(textarea(), { key: 'Enter' });

    expect(props.onSend).toHaveBeenCalledWith('merhaba');
    expect(textarea()).toHaveValue('');
  });

  it('Shift+Enter gondermez — yeni satir icin', () => {
    const props = renderComposer();

    fireEvent.change(textarea(), { target: { value: 'ilk satır' } });
    fireEvent.keyDown(textarea(), { key: 'Enter', shiftKey: true });

    expect(props.onSend).not.toHaveBeenCalled();
    expect(textarea()).toHaveValue('ilk satır');
  });

  it('bos mesaj gonderilmez, buton kapalidir', () => {
    const props = renderComposer();

    expect(screen.getByRole('button', { name: 'Gönder' })).toBeDisabled();

    fireEvent.change(textarea(), { target: { value: '   ' } });
    fireEvent.keyDown(textarea(), { key: 'Enter' });

    expect(props.onSend).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Gönder' })).toBeDisabled();
    // Yazilan kaybolmadi.
    expect(textarea()).toHaveValue('   ');
  });

  it('bosluklari kirpip gonderir', () => {
    const props = renderComposer();

    fireEvent.change(textarea(), { target: { value: '  selam  ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));

    expect(props.onSend).toHaveBeenCalledWith('selam');
  });

  it('yanit beklenirken gonderim kilitlidir', () => {
    const props = renderComposer({ sending: true });

    fireEvent.change(textarea(), { target: { value: 'ikinci mesaj' } });
    fireEvent.keyDown(textarea(), { key: 'Enter' });

    expect(props.onSend).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Gönder' })).toBeDisabled();
  });

  it('bekleyen dosyalari cip olarak gosterir', () => {
    renderComposer({ pendingAttachments: [attachment()] });

    const chips = screen.getByRole('list', { name: 'Eklenecek dosyalar' });
    expect(chips).toHaveTextContent('notlar.md · 2 KB');
  });

  it('dosya secimini yukari verir (okuma bilesenin isi degil)', () => {
    const onAttachFiles = vi.fn<(files: readonly File[]) => void>();
    renderComposer({ onAttachFiles });

    const file = new File(['icerik'], 'notlar.md', { type: 'text/markdown' });
    fireEvent.change(screen.getByLabelText('Dosya seç'), { target: { files: [file] } });

    expect(onAttachFiles).toHaveBeenCalledTimes(1);
    const [files] = onAttachFiles.mock.calls[0] ?? [];
    expect(files?.[0]?.name).toBe('notlar.md');
  });

  it('"Projeden dosya ekle" yalnizca projeli konusmada cikar', () => {
    renderComposer();
    expect(screen.queryByRole('button', { name: 'Projeden dosya ekle' })).toBeNull();
  });

  it('projeli konusmada secici acilir ve secim yukari bildirilir', async () => {
    const source = vi.fn<(path: string) => Promise<ProjectDirectoryView>>(() =>
      Promise.resolve({
        ...EMPTY_DIR,
        entries: [
          { name: 'src', kind: 'dir', sizeBytes: null, blocked: false },
          { name: 'README.md', kind: 'file', sizeBytes: 1024, blocked: false },
          { name: '.env', kind: 'file', sizeBytes: 120, blocked: true },
        ],
        totalEntries: 3,
        returnedEntries: 3,
      }),
    );

    const props = renderComposer({ projectId: 'asuna', listProjectDirectory: source });

    fireEvent.click(screen.getByRole('button', { name: 'Projeden dosya ekle' }));

    const readme = await screen.findByRole('button', { name: 'README.md · 1 KB' });
    // Blok listesindeki dosya gorunur ama secilemez.
    expect(screen.getByRole('button', { name: '.env · 120 B' })).toBeDisabled();

    fireEvent.click(readme);
    expect(props.onAttachProjectFile).toHaveBeenCalledWith('README.md');
    // Secimden sonra secici kapanir.
    expect(screen.queryByRole('region', { name: 'Projeden dosya ekle' })).toBeNull();
  });

  /**
   * Kullanici Enter'a iki kez basarsa (ya da tuslama tekrarlarsa) ayni metin
   * ikinci kez gitmemeli: `sending` bayragi henuz yukaridan geri gelmemis olsa
   * bile alan gonderim aninda temizlendigi icin ikinci basis bos mesaja duser.
   * Aksi halde kullanici tek soru sordugunu sanirken model iki kez cagrilir
   * (iki DB yazimi + iki kez ucret).
   */
  it('pes pese iki Enter tek istek uretir', () => {
    const props = renderComposer();

    fireEvent.change(textarea(), { target: { value: 'merhaba' } });
    fireEvent.keyDown(textarea(), { key: 'Enter' });
    fireEvent.keyDown(textarea(), { key: 'Enter' });
    fireEvent.keyDown(textarea(), { key: 'Enter' });

    expect(props.onSend).toHaveBeenCalledExactlyOnceWith('merhaba');
  });

  it('gonder butonuna cift tiklamak tek istek uretir', () => {
    const props = renderComposer();

    fireEvent.change(textarea(), { target: { value: 'merhaba' } });
    const send = screen.getByRole('button', { name: 'Gönder' });
    fireEvent.click(send);
    fireEvent.click(send);

    expect(props.onSend).toHaveBeenCalledTimes(1);
  });

  /** IME (Turkce/Japonca girdi) bileseni sirasindaki Enter kelimeyi tamamlar. */
  it('IME bileseni sirasindaki Enter gondermez', () => {
    const props = renderComposer();

    fireEvent.change(textarea(), { target: { value: 'merhab' } });
    fireEvent.keyDown(textarea(), { key: 'Enter', isComposing: true });

    expect(props.onSend).not.toHaveBeenCalled();
    expect(textarea()).toHaveValue('merhab');
  });

  it('mikrofon butonu ses moduna gecis ister', () => {
    const onOpenVoice = vi.fn();
    renderComposer({ onOpenVoice });

    fireEvent.click(screen.getByRole('button', { name: 'Ses moduna geç' }));
    expect(onOpenVoice).toHaveBeenCalledTimes(1);
  });
});

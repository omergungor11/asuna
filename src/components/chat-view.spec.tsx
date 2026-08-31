/**
 * `ChatView` testleri (plan-chat-shell.md WP3).
 *
 * Kanitlanan seyler:
 * 1. Mesajlar servisten yuklenir; kullanici ve asistan **ayri** rollerle
 *    render edilir (metin duz, markdown yok).
 * 2. Gonderim sirasinda "yazıyor…" gostergesi cikar, yanit gelince kaybolur.
 * 3. Baslik otomasyonu: baslik **ilk** kullanici mesajindan sonra konur, ikinci
 *    mesajda tekrar konmaz.
 * 4. Baslik yazilamazsa hata yutulmaz — mesaj durur, uyari gorunur.
 * 5. Gonderim basarisiz olursa mesaj listeye eklenmez ve hata gorunur.
 * 6. Eklenen dosya once composer'da bekleyen cip, gonderimden sonra mesajin
 *    cipi olur; `sendMessage`e attachment kimlikleri gider.
 * 7. Konusma degisince bekleyen ekler tasinmaz ve gec gelen yanit yeni
 *    konusmaya yazilmaz (WP4 bosluk analizi).
 *
 * IPC yok: butun servis yuzeyi sahte port.
 */

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi, type Mock } from 'vitest';

import { AsunaStoreError } from '../shared/store-error';
import type { ChatAttachment, ChatMessage, ChatReply } from '../shared/chat';

import { ChatView, type ChatViewPort } from './chat-view';

const SESSION_ID = 3;

function message(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id: 1,
    sessionId: SESSION_ID,
    role: 'user',
    content: 'merhaba',
    createdAt: '2026-08-31T10:00:00Z',
    ...overrides,
  };
}

function attachment(overrides: Partial<ChatAttachment> = {}): ChatAttachment {
  return {
    id: 11,
    sessionId: SESSION_ID,
    messageId: null,
    fileName: 'notlar.md',
    mimeType: 'text/markdown',
    sizeBytes: 1024,
    origin: 'upload',
    createdAt: '2026-08-31T10:00:00Z',
    ...overrides,
  };
}

function reply(text: string, userId = 20, assistantId = 21): ChatReply {
  return {
    userMessage: message({ id: userId, role: 'user', content: text }),
    assistantMessage: message({ id: assistantId, role: 'assistant', content: 'anladım' }),
  };
}

interface TestPort extends ChatViewPort {
  readonly listMessages: Mock<(sessionId: number) => Promise<readonly ChatMessage[]>>;
  readonly listAttachments: Mock<(sessionId: number) => Promise<readonly ChatAttachment[]>>;
  readonly sendMessage: Mock<
    (sessionId: number, text: string, attachmentIds: readonly number[]) => Promise<ChatReply>
  >;
  readonly setTitle: Mock<(sessionId: number, title: string) => Promise<void>>;
  readonly ingestAttachment: Mock<(sessionId: number, file: File) => Promise<ChatAttachment>>;
  readonly attachProjectFile: Mock<
    (sessionId: number, relativePath: string) => Promise<ChatAttachment>
  >;
}

function createPort(overrides: Partial<TestPort> = {}): TestPort {
  return {
    listMessages: vi.fn(() => Promise.resolve<readonly ChatMessage[]>([])),
    listAttachments: vi.fn(() => Promise.resolve<readonly ChatAttachment[]>([])),
    sendMessage: vi.fn((_sessionId: number, text: string) => Promise.resolve(reply(text))),
    setTitle: vi.fn(() => Promise.resolve()),
    ingestAttachment: vi.fn(() => Promise.resolve(attachment())),
    attachProjectFile: vi.fn(() =>
      Promise.resolve(attachment({ id: 12, fileName: 'README.md', origin: 'project' })),
    ),
    ...overrides,
  };
}

function type(text: string): void {
  fireEvent.change(screen.getByRole('textbox', { name: 'Mesaj' }), { target: { value: text } });
}

describe('ChatView', () => {
  it('mesajlari yukler ve rollerine gore ayirir', async () => {
    const port = createPort({
      listMessages: vi.fn(() =>
        Promise.resolve<readonly ChatMessage[]>([
          message({ id: 1, role: 'user', content: 'soru' }),
          message({ id: 2, role: 'assistant', content: 'cevap\nikinci satır' }),
        ]),
      ),
    });

    render(<ChatView sessionId={SESSION_ID} projectId={null} port={port} />);

    const user = await screen.findByRole('article', { name: 'Sen mesajı' });
    expect(user).toHaveTextContent('soru');
    const assistant = screen.getByRole('article', { name: 'Asuna mesajı' });
    expect(assistant).toHaveTextContent('cevap');
    expect(port.listMessages).toHaveBeenCalledWith(SESSION_ID);
  });

  it('bos konusmada ne yapilacagini soyler', async () => {
    render(<ChatView sessionId={SESSION_ID} projectId={null} port={createPort()} />);

    expect(await screen.findByText(/Bu konuşma boş/)).toBeInTheDocument();
  });

  it('gonderim sirasinda "yazıyor…" gosterir, yanit gelince mesajlari basar', async () => {
    let resolveSend: (value: ChatReply) => void = () => undefined;
    const port = createPort({
      sendMessage: vi.fn(
        () =>
          new Promise<ChatReply>((resolve) => {
            resolveSend = resolve;
          }),
      ),
    });

    render(<ChatView sessionId={SESSION_ID} projectId={null} port={port} />);
    await screen.findByText(/Bu konuşma boş/);

    type('merhaba');
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));

    expect(await screen.findByText('Asuna yazıyor…')).toBeInTheDocument();

    resolveSend(reply('merhaba'));

    expect(await screen.findByText('anladım')).toBeInTheDocument();
    expect(screen.queryByText('Asuna yazıyor…')).toBeNull();
  });

  it('ilk kullanici mesajindan sonra basligi otomatik koyar (ilk 60 karakter)', async () => {
    const port = createPort();
    const onChanged = vi.fn();

    render(
      <ChatView
        sessionId={SESSION_ID}
        projectId={null}
        port={port}
        onConversationChanged={onChanged}
      />,
    );
    await screen.findByText(/Bu konuşma boş/);

    const uzun = 'a'.repeat(80);
    type(uzun);
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));

    await waitFor(() => {
      expect(port.setTitle).toHaveBeenCalledWith(SESSION_ID, 'a'.repeat(60));
    });
    await waitFor(() => {
      expect(onChanged).toHaveBeenCalled();
    });
  });

  it('ikinci mesajdan sonra basligi tekrar yazmaz', async () => {
    const port = createPort({
      listMessages: vi.fn(() =>
        Promise.resolve<readonly ChatMessage[]>([message({ id: 1, role: 'user' })]),
      ),
    });

    render(<ChatView sessionId={SESSION_ID} projectId={null} port={port} />);
    await screen.findByRole('article', { name: 'Sen mesajı' });

    type('ikinci soru');
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));

    await waitFor(() => {
      expect(port.sendMessage).toHaveBeenCalled();
    });
    expect(port.setTitle).not.toHaveBeenCalled();
  });

  it('baslik yazilamazsa hatayi gizlemez', async () => {
    const port = createPort({
      setTitle: vi.fn(() => Promise.reject(new AsunaStoreError('storage', 'disk dolu'))),
    });

    render(<ChatView sessionId={SESSION_ID} projectId={null} port={port} />);
    await screen.findByText(/Bu konuşma boş/);

    type('merhaba');
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));

    expect(await screen.findByText(/Başlık kaydedilemedi/)).toBeInTheDocument();
    // Mesajin kendisi gitti: baslik hatasi konusmayi silmez.
    expect(screen.getByText('anladım')).toBeInTheDocument();
  });

  it('gonderim basarisizsa mesaj eklenmez, hata gorunur', async () => {
    const port = createPort({
      sendMessage: vi.fn(() =>
        Promise.reject(new AsunaStoreError('unavailable', 'hafıza kapalı')),
      ),
    });

    render(<ChatView sessionId={SESSION_ID} projectId={null} port={port} />);
    await screen.findByText(/Bu konuşma boş/);

    type('merhaba');
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/hafıza kapalı/);
    expect(screen.queryByRole('article', { name: 'Sen mesajı' })).toBeNull();
    expect(screen.queryByText('Asuna yazıyor…')).toBeNull();
  });

  it('eklenen dosya once bekleyen cip, gonderimden sonra mesajin cipi olur', async () => {
    const port = createPort();

    render(<ChatView sessionId={SESSION_ID} projectId={null} port={port} />);
    await screen.findByText(/Bu konuşma boş/);

    const file = new File(['icerik'], 'notlar.md', { type: 'text/markdown' });
    fireEvent.change(screen.getByLabelText('Dosya seç'), { target: { files: [file] } });

    const pendingChips = await screen.findByRole('list', { name: 'Eklenecek dosyalar' });
    expect(pendingChips).toHaveTextContent('notlar.md');
    expect(port.ingestAttachment).toHaveBeenCalledWith(SESSION_ID, file);

    type('dosyaya bak');
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));

    await waitFor(() => {
      expect(port.sendMessage).toHaveBeenCalledWith(SESSION_ID, 'dosyaya bak', [11]);
    });

    const userMessage = await screen.findByRole('article', { name: 'Sen mesajı' });
    expect(
      within(userMessage).getByRole('list', { name: 'Mesajın dosyaları' }),
    ).toHaveTextContent('notlar.md');
    expect(screen.queryByRole('list', { name: 'Eklenecek dosyalar' })).toBeNull();
  });

  it('mesajlar okunamazsa nedeni yazar', async () => {
    const port = createPort({
      listMessages: vi.fn(() => Promise.reject(new AsunaStoreError('storage', 'bozuk kayıt'))),
    });

    render(<ChatView sessionId={SESSION_ID} projectId={null} port={port} />);

    expect(await screen.findByRole('alert')).toHaveTextContent('bozuk kayıt');
  });

  /**
   * **Sahiplik siniri (WP4)**: bir konusmada bekleyen ek, baska bir konusma
   * acilinca composer'da kalmamali. Kalsaydi renderer o kimligi yeni konusmanin
   * `chat_send`ine verirdi; Rust bunu reddeder (sahiplik dogrulamasi) ama o
   * noktaya gelinmemeli — kullanici "dosyam gitti mi?" sorusunu yasamamali.
   */
  it('konusma degisince bekleyen ekler tasinmaz', async () => {
    const port = createPort();
    const { rerender } = render(
      <ChatView sessionId={SESSION_ID} projectId={null} port={port} />,
    );
    await screen.findByText(/Bu konuşma boş/);

    const file = new File(['icerik'], 'notlar.md', { type: 'text/markdown' });
    fireEvent.change(screen.getByLabelText('Dosya seç'), { target: { files: [file] } });
    await screen.findByRole('list', { name: 'Eklenecek dosyalar' });

    rerender(<ChatView sessionId={99} projectId={null} port={port} />);

    expect(screen.queryByRole('list', { name: 'Eklenecek dosyalar' })).toBeNull();

    type('yeni konusma');
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));

    await waitFor(() => {
      expect(port.sendMessage).toHaveBeenCalledWith(99, 'yeni konusma', []);
    });
  });

  /** Gec gelen yanit **acik** konusmaya yazilmaz: bayat cevap baska bir
   * konusmanin icinde belirmez. */
  it('baska konusma icin gelen gec yanit ekrana yazilmaz', async () => {
    let resolveSend: (value: ChatReply) => void = () => undefined;
    const port = createPort({
      sendMessage: vi.fn(
        () =>
          new Promise<ChatReply>((resolve) => {
            resolveSend = resolve;
          }),
      ),
    });

    const { rerender } = render(
      <ChatView sessionId={SESSION_ID} projectId={null} port={port} />,
    );
    await screen.findByText(/Bu konuşma boş/);

    type('ilk konusmanin sorusu');
    fireEvent.click(screen.getByRole('button', { name: 'Gönder' }));
    await screen.findByText('Asuna yazıyor…');

    rerender(<ChatView sessionId={99} projectId={null} port={port} />);
    resolveSend(reply('ilk konusmanin sorusu'));

    await screen.findByText(/Bu konuşma boş/);
    expect(screen.queryByText('anladım')).toBeNull();
    expect(screen.queryByText('ilk konusmanin sorusu')).toBeNull();
    expect(port.setTitle).not.toHaveBeenCalledWith(99, expect.anything());
  });
  /**
   * Review M4: ek listesi mesajlarla AYNI zincirde degil. `attachment_list`
   * duserse konusma yine okunur; kullanici mesajlarini bir dosya listesi
   * hatasi yuzunden kaybetmez.
   */
  it('ek listesi okunamazsa mesajlar yine gorunur, uyari ayri durur', async () => {
    const port = createPort({
      listMessages: vi.fn(() =>
        Promise.resolve<readonly ChatMessage[]>([
          message({ id: 1, role: 'assistant', content: 'okunabilir cevap' }),
        ]),
      ),
      listAttachments: vi.fn(() =>
        Promise.reject(new AsunaStoreError('storage', 'ek tablosu bozuk')),
      ),
    });

    render(<ChatView sessionId={SESSION_ID} projectId={null} port={port} />);

    expect(await screen.findByText('okunabilir cevap')).toBeInTheDocument();
    expect(await screen.findByText(/Dosya listesi okunamadı/)).toHaveTextContent(
      'ek tablosu bozuk',
    );
    // Konusma "acilamadi" gibi gosterilmez: bu ikincil bir uyari.
    expect(screen.queryByRole('alert')).toBeNull();
    // Yazma yolu acik kalir.
    expect(screen.getByRole('textbox', { name: 'Mesaj' })).toBeInTheDocument();
  });

  /**
   * Review H1/M2: `chat_send` ses oturumlarini reddediyor. Ekran bunu hataya
   * duserek degil TASARIMLA karsilar — yazma yolu hic acilmaz.
   */
  it('ses oturumunda composer render edilmez, nedeni yazili', async () => {
    const port = createPort({
      listMessages: vi.fn(() =>
        Promise.resolve<readonly ChatMessage[]>([
          message({ id: 1, role: 'assistant', content: 'sesli cevabın dökümü' }),
        ]),
      ),
    });

    render(<ChatView sessionId={SESSION_ID} projectId={null} modality="voice" port={port} />);

    expect(await screen.findByText('sesli cevabın dökümü')).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'Mesaj' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Gönder' })).toBeNull();
    expect(screen.getByText(/buraya metin yazılamaz/)).toBeInTheDocument();
  });

  it('bos ses oturumunda "ilk mesajı yaz" demez', async () => {
    render(
      <ChatView sessionId={SESSION_ID} projectId={null} modality="voice" port={createPort()} />,
    );

    expect(await screen.findByText(/Bu bir ses oturumu/)).toBeInTheDocument();
    expect(screen.queryByText(/İlk mesajı yaz/)).toBeNull();
  });

  it('ses oturumunda mikrofon butonu ses moduna gecirir', async () => {
    const onOpenVoice = vi.fn();
    render(
      <ChatView
        sessionId={SESSION_ID}
        projectId={null}
        modality="voice"
        port={createPort()}
        onOpenVoice={onOpenVoice}
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Ses moduna geç' }));
    expect(onOpenVoice).toHaveBeenCalledTimes(1);
  });
});

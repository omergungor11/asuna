/**
 * Kabuk testleri — chat shell pivotu (plan-chat-shell.md WP3 / ADR-006).
 *
 * Kanitlanan seyler:
 * 1. Kabuk iki kolon: kenar cubugunda konusma listesi, ana alanda secilen ekran.
 * 2. "+ Yeni konuşma" akisi: `recorded` ise konusma acilir, `skipped` ise
 *    kullaniciya hafizanin kapali oldugu **soylenir** (sahte konusma acilmaz).
 * 3. Konusma silinince servis cagrilir ve liste **yeniden okunur**.
 * 4. Ses paneli hicbir ekran degisiminde unmount edilmez — canli Realtime
 *    oturumu kopmaz (degismeyen kural).
 * 5. Projeye tiklamak proje ekranini acar ve o projede konusma baslatilabilir.
 *
 * IPC yok: `chat-service`, proje kaydi ve hafiza servisleri sahtelenir.
 */

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type * as ToolAuditModule from '../asuna/tools/audit';
import type * as ProjectContextModule from '../asuna/projects/project-context';
import type * as ProjectRegistryModule from '../asuna/projects/project-registry';
import type {
  ChatAttachment,
  ChatMessage,
  ChatReply,
  ConversationStartResult,
  ConversationSummary,
} from '../shared/chat';
import type { DbStatus } from '../shared/db-status';
import type { ToolEventPage } from '../shared/tool-event';
import type { PrivacySettings } from '../shared/privacy';
import type { ProjectRecord } from '../shared/project';

import { App } from './app';

// Metin sohbetinin tum IPC yuzeyi tek modulde; kabuk testinde tamami sahte.
const chat = vi.hoisted(() => ({
  listConversations: vi.fn<() => Promise<readonly ConversationSummary[]>>(),
  startConversation: vi.fn<(projectId?: string) => Promise<ConversationStartResult>>(),
  setConversationTitle: vi.fn<(sessionId: number, title: string) => Promise<void>>(),
  deleteConversation: vi.fn<(sessionId: number) => Promise<void>>(),
  listMessages: vi.fn<(sessionId: number) => Promise<readonly ChatMessage[]>>(),
  listAttachments: vi.fn<(sessionId: number) => Promise<readonly ChatAttachment[]>>(),
  sendMessage: vi.fn<(sessionId: number, text: string) => Promise<ChatReply>>(),
  ingestAttachment: vi.fn<(sessionId: number, file: File) => Promise<ChatAttachment>>(),
  attachProjectFile: vi.fn<(sessionId: number, path: string) => Promise<ChatAttachment>>(),
}));

vi.mock('../asuna/agent/chat-service', () => chat);

// Kabuk testi IPC'ye dokunmaz: hafiza ekrani acildiginda gercek `db_status`
// komutu cagrilmasin diye servis katmani sahtelenir (ASU-036).
const DISABLED: DbStatus = {
  availability: 'disabled',
  schemaVersion: null,
  sqliteVersion: '3.46.0',
  reason: null,
};

vi.mock('../asuna/memory/db-status-service', () => ({
  DB_STATUS_COMMAND: 'db_status',
  fetchDbStatus: (): Promise<DbStatus> => Promise.resolve(DISABLED),
}));

// Ayni gerekce (ASU-037): ayarlar ekrani acildiginda gercek gizlilik komutu
// cagrilmasin.
const PRIVACY: PrivacySettings = {
  memoryEnabled: false,
  transcriptStorage: true,
  memoryEnabledAtBoot: false,
  transcriptStorageAtBoot: true,
};

vi.mock('../asuna/memory/privacy-service', () => ({
  PRIVACY_COMMANDS: { get: 'get_privacy_settings', set: 'set_privacy_settings' },
  fetchPrivacySettings: (): Promise<PrivacySettings> => Promise.resolve(PRIVACY),
  updatePrivacySettings: (): Promise<PrivacySettings> => Promise.resolve(PRIVACY),
}));

// ASU-054: Araclar ekrani acildiginda gercek `tool_event_list` komutu
// cagrilmasin. Defter bos donuyor — ekranin kendisi test ediliyor, veri degil.
const EMPTY_EVENTS: ToolEventPage = { events: [], limit: 25, limitMax: 200, total: 0 };

vi.mock('../asuna/tools/audit', async (importOriginal) => ({
  ...(await importOriginal<typeof ToolAuditModule>()),
  listToolEvents: (): Promise<ToolEventPage> => Promise.resolve(EMPTY_EVENTS),
}));

// ASU-045: proje kaydi da ayri bir IPC yuzeyi. Hem kenar cubugu, hem proje
// ekrani, hem de ses panelindeki "mevcut proje" gostergesi bu servisten okur.
const ASUNA_PROJECT: ProjectRecord = {
  id: 'asuna',
  name: 'Asuna',
  path: '/Users/arlec/Work/asuna',
  description: null,
  status: 'active',
  primaryLanguage: 'TypeScript',
  framework: 'React',
  gitRemote: null,
  lastOpenedAt: '2026-08-24T09:30:00Z',
  createdAt: '2026-08-01T09:30:00Z',
  updatedAt: '2026-08-24T09:30:00Z',
  metadataJson: '{}',
};

vi.mock('../asuna/projects/project-registry', async (importOriginal) => ({
  ...(await importOriginal<typeof ProjectRegistryModule>()),
  listProjects: (): Promise<readonly ProjectRecord[]> => Promise.resolve([ASUNA_PROJECT]),
}));

vi.mock('../asuna/projects/project-context', async (importOriginal) => ({
  ...(await importOriginal<typeof ProjectContextModule>()),
  fetchProjectContext: (): Promise<{ status: 'unavailable'; message: string }> =>
    Promise.resolve({ status: 'unavailable', message: 'test ortamında komut yok' }),
}));

function conversation(overrides: Partial<ConversationSummary> = {}): ConversationSummary {
  const now = new Date().toISOString();
  return {
    id: 1,
    title: 'Dün ne konuştuk',
    modality: 'text',
    projectId: null,
    startedAt: now,
    lastActivityAt: now,
    messageCount: 2,
    ...overrides,
  };
}

const ADSIZ = conversation({ id: 2, title: null, projectId: 'asuna' });

/** Kenar cubugu: ayni etiket bos ekranda da var, secim daraltilir. */
function sidebar(): HTMLElement {
  return screen.getByRole('navigation', { name: 'Asuna kenar çubuğu' });
}

beforeEach(() => {
  vi.clearAllMocks();
  chat.listConversations.mockResolvedValue([conversation(), ADSIZ]);
  chat.startConversation.mockResolvedValue({ status: 'recorded', id: 42 });
  chat.setConversationTitle.mockResolvedValue(undefined);
  chat.deleteConversation.mockResolvedValue(undefined);
  chat.listMessages.mockResolvedValue([]);
  chat.listAttachments.mockResolvedValue([]);
});

describe('App — chat shell', () => {
  it('kenar cubugunu ve konusma listesini acar; basliksiz konusma "Adsız" gorunur', async () => {
    render(<App />);

    expect(screen.getByRole('heading', { name: 'Asuna', level: 1 })).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Dün ne konuştuk' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Adsız konuşma' })).toBeInTheDocument();
  });

  it('konusma secilmeden mesaj alani acilmaz; bos durum yol gosterir', async () => {
    render(<App />);

    expect(await screen.findByText(/Soldan bir konuşma seç/)).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Konuşma' })).toBeNull();
  });

  it('"+ Yeni konuşma" konusma acar ve mesaj alanina gecer', async () => {
    render(<App />);

    fireEvent.click(within(sidebar()).getByRole('button', { name: '+ Yeni konuşma' }));

    expect(await screen.findByRole('region', { name: 'Konuşma' })).toBeInTheDocument();
    expect(chat.startConversation).toHaveBeenCalledTimes(1);
    // Yeni konusmanin mesajlari servisten okunur.
    await waitFor(() => {
      expect(chat.listMessages).toHaveBeenCalledWith(42);
    });
  });

  it('hafiza kapaliyken konusma acilmaz, kullaniciya soylenir', async () => {
    chat.startConversation.mockResolvedValue({ status: 'skipped', reason: 'memory disabled' });

    render(<App />);
    fireEvent.click(within(sidebar()).getByRole('button', { name: '+ Yeni konuşma' }));

    expect(await screen.findByText(/Konuşma geçmişi \(hafıza\) kapalı/)).toBeInTheDocument();
    // Sahte bir konusma acilmadi: mesaj alani hala yok.
    expect(screen.queryByRole('region', { name: 'Konuşma' })).toBeNull();
  });

  it('konusma silme onay ister, sonra servisi cagirip listeyi tazeler', async () => {
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Sil: Dün ne konuştuk' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    await waitFor(() => {
      expect(chat.deleteConversation).toHaveBeenCalledWith(1);
    });
    // Liste UI tahminiyle degil, servisten yeniden okunur.
    await waitFor(() => {
      expect(chat.listConversations).toHaveBeenCalledTimes(2);
    });
  });

  it('hafiza ekranina gecince ses paneli monte kalir (oturum kopmaz)', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: 'Hafıza' }));

    expect(await screen.findByText(/Hafıza kapalı/)).toBeInTheDocument();

    const voice = document.getElementById('asuna-panel-voice');
    expect(voice).toHaveAttribute('hidden');
    // Gizli ama YIKILMAMIS: canli Realtime oturumu ekran degisiminde kopmaz.
    expect(voice?.querySelector('.asuna-panel')).not.toBeNull();
  });

  it('ses moduna gecince panel gorunur olur, digerleri monte edilmez', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: 'Ses modu' }));

    await waitFor(() => {
      expect(document.getElementById('asuna-panel-voice')).not.toHaveAttribute('hidden');
    });
    expect(document.getElementById('asuna-panel-memory')).toBeNull();
  });

  it('araclar ekrani denetim defterini acar, ses panelini yikmaz', async () => {
    render(<App />);

    expect(document.getElementById('asuna-panel-tools')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Araçlar' }));

    expect(await screen.findByText('Bu filtreye uyan araç çağrısı yok.')).toBeInTheDocument();
    expect(
      document.getElementById('asuna-panel-voice')?.querySelector('.asuna-panel'),
    ).not.toBeNull();
  });

  it('ayarlar ekrani gizlilik anahtarlarini acar, ses panelini yikmaz', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: 'Ayarlar' }));

    expect(await screen.findByRole('switch', { name: 'Konuşma dökümü saklama' })).toBeChecked();
    // `.env` ile kapatilmis anahtar buradan acilamaz.
    expect(screen.getByRole('switch', { name: 'Kalıcı hafıza' })).toBeDisabled();
    expect(
      document.getElementById('asuna-panel-voice')?.querySelector('.asuna-panel'),
    ).not.toBeNull();
  });

  it('projeye tiklayinca proje ekrani acilir ve o projede konusma baslatilir', async () => {
    render(<App />);

    // Kenar cubugundaki proje satiri (ana alandaki proje karti degil).
    fireEvent.click(await within(sidebar()).findByRole('button', { name: 'Asuna' }));

    const start = await screen.findByRole('button', { name: 'Bu projede yeni konuşma' });
    fireEvent.click(start);

    await waitFor(() => {
      expect(chat.startConversation).toHaveBeenCalledWith('asuna');
    });
    expect(await screen.findByRole('region', { name: 'Konuşma' })).toBeInTheDocument();
  });
  /**
   * Review H1/M2: ses oturumu listede tur bilgisiyle durur ve acildiginda
   * **salt okunur** gelir — `chat_send` voice oturumlari reddettigi icin
   * kullaniciya once yazdirip sonra hata gostermek durust olmazdi.
   */
  it('listeden secilen ses oturumu salt okunur acilir', async () => {
    chat.listConversations.mockResolvedValue([
      conversation({ id: 5, title: null, modality: 'voice' }),
    ]);

    render(<App />);

    fireEvent.click(await within(sidebar()).findByRole('button', { name: 'Sesli oturum' }));

    expect(await screen.findByRole('region', { name: 'Konuşma' })).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'Mesaj' })).toBeNull();
    expect(screen.getByText(/buraya metin yazılamaz/)).toBeInTheDocument();
  });
});

/**
 * `Sidebar` testleri (plan-chat-shell.md WP3).
 *
 * Kanitlanan seyler:
 * 1. Konusmalar **tarihe gore** gruplanir (Bugün / Dün / Son 7 gün / Daha eski)
 *    ve grup icindeki sira backend'in verdigi sira olarak kalir.
 * 2. Basligi olmayan konusma "Adsız konuşma" yazar — bos satir birakilmaz.
 * 3. Acik konusma isaretlidir (`aria-current`).
 * 4. Silme **onay ister**; onaydan once servis cagrisi tetiklenmez.
 * 5. Liste okunamadiysa neden ekranda yazar (bos liste ile karistirilmaz).
 *
 * Bilesen saf sunum: burada servis de IPC de yok, yalnizca props/olay.
 */

import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { ConversationSummary } from '../shared/chat';
import type { ProjectRecord } from '../shared/project';

import { Sidebar, type SidebarProps } from './sidebar';

const NOW = new Date('2026-08-31T12:00:00');

function conversation(overrides: Partial<ConversationSummary> = {}): ConversationSummary {
  return {
    id: 1,
    title: 'Bugünkü konuşma',
    modality: 'text',
    projectId: null,
    startedAt: '2026-08-31T09:00:00',
    lastActivityAt: '2026-08-31T09:00:00',
    messageCount: 2,
    ...overrides,
  };
}

const PROJECT: ProjectRecord = {
  id: 'asuna',
  name: 'Asuna',
  path: '/Users/arlec/Work/asuna',
  description: null,
  status: 'active',
  primaryLanguage: 'TypeScript',
  framework: null,
  gitRemote: null,
  lastOpenedAt: null,
  createdAt: '2026-08-01T09:30:00Z',
  updatedAt: '2026-08-01T09:30:00Z',
  metadataJson: '{}',
};

function renderSidebar(overrides: Partial<SidebarProps> = {}): SidebarProps {
  const props: SidebarProps = {
    view: 'chat',
    conversations: [conversation()],
    conversationsLoading: false,
    conversationsError: null,
    activeSessionId: null,
    projects: [PROJECT],
    projectsError: null,
    activeProjectId: null,
    starting: false,
    busySessionId: null,
    onNewConversation: vi.fn(),
    onSelectConversation: vi.fn(),
    onDeleteConversation: vi.fn(),
    onSelectProject: vi.fn(),
    onSelectView: vi.fn(),
    now: NOW,
    ...overrides,
  };

  render(<Sidebar {...props} />);
  return props;
}

describe('Sidebar', () => {
  it('konusmalari tarih gruplarina ayirir ve grup icindeki sirayi korur', () => {
    renderSidebar({
      conversations: [
        conversation({ id: 1, title: 'Bugün A', lastActivityAt: '2026-08-31T11:00:00' }),
        conversation({ id: 2, title: 'Bugün B', lastActivityAt: '2026-08-31T08:00:00' }),
        conversation({ id: 3, title: 'Dünkü', lastActivityAt: '2026-08-30T22:00:00' }),
        conversation({ id: 4, title: 'Salı', lastActivityAt: '2026-08-27T10:00:00' }),
        conversation({ id: 5, title: 'Geçen ay', lastActivityAt: '2026-07-15T10:00:00' }),
      ],
    });

    expect(screen.getByRole('heading', { name: 'Bugün' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Dün' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Son 7 gün' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Daha eski' })).toBeInTheDocument();

    const today = screen.getByRole('region', { name: 'Bugün' });
    const titles = within(today)
      .getAllByRole('button')
      .map((button) => button.textContent)
      .filter((text) => text !== 'Sil');
    expect(titles).toEqual(['Bugün A', 'Bugün B']);

    expect(
      within(screen.getByRole('region', { name: 'Son 7 gün' })).getByRole('button', {
        name: 'Salı',
      }),
    ).toBeInTheDocument();
  });

  it('basliksiz konusmayi "Adsız konuşma" olarak yazar', () => {
    renderSidebar({ conversations: [conversation({ title: null })] });

    expect(screen.getByRole('button', { name: 'Adsız konuşma' })).toBeInTheDocument();
  });

  it('acik konusmayi isaretler ve tiklaninca secimi bildirir', () => {
    const props = renderSidebar({
      conversations: [conversation({ id: 7, title: 'Açık olan' })],
      activeSessionId: 7,
    });

    const row = screen.getByRole('button', { name: 'Açık olan' });
    expect(row).toHaveAttribute('aria-current', 'true');

    fireEvent.click(row);
    expect(props.onSelectConversation).toHaveBeenCalledWith(7);
  });

  it('silme onay ister; onaydan once servis cagrilmaz', () => {
    const props = renderSidebar({
      conversations: [conversation({ id: 9, title: 'Silinecek' })],
    });

    fireEvent.click(screen.getByRole('button', { name: 'Sil: Silinecek' }));
    expect(props.onDeleteConversation).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));
    expect(props.onDeleteConversation).toHaveBeenCalledWith(9);
  });

  /**
   * Review H1: `conversation_list` ses oturumlarini da donduruyor. Kullanici
   * bir ses oturumunu bos metin konusmasi sanip silerse `session_delete`
   * oturumun OZETINI ve varsa DISKTEKI DOKUMUNU de kalici siler — bu yuzden
   * tur listede gorunur, uyari da silme onayinda yazili.
   */
  it('ses oturumunu rozet ve "Sesli oturum" basligiyla ayirir', () => {
    renderSidebar({
      conversations: [conversation({ id: 4, title: null, modality: 'voice' })],
    });

    expect(screen.getByRole('button', { name: 'Sesli oturum' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Adsız konuşma' })).toBeNull();
    expect(screen.getByText('Ses')).toBeInTheDocument();
  });

  it('ses oturumunun silme onayi ozet ve dokum kaybini yazar', () => {
    renderSidebar({
      conversations: [conversation({ id: 4, title: null, modality: 'voice' })],
    });

    fireEvent.click(screen.getByRole('button', { name: 'Sil: Sesli oturum' }));

    const confirm = screen.getByRole('group', { name: 'Sesli oturum silme onayı' });
    expect(confirm).toHaveTextContent('ses oturumu');
    expect(confirm).toHaveTextContent('döküm');
  });

  it('metin konusmasinin onayi ses uyarisini kullanmaz', () => {
    renderSidebar({ conversations: [conversation({ id: 9, title: 'Silinecek' })] });

    fireEvent.click(screen.getByRole('button', { name: 'Sil: Silinecek' }));

    const confirm = screen.getByRole('group', { name: 'Silinecek silme onayı' });
    expect(confirm).toHaveTextContent('kalıcı olarak silinsin mi');
    expect(confirm).not.toHaveTextContent('döküm');
  });

  it('vazgecince silme cagrisi yapilmaz', () => {
    const props = renderSidebar({
      conversations: [conversation({ id: 9, title: 'Silinecek' })],
    });

    fireEvent.click(screen.getByRole('button', { name: 'Sil: Silinecek' }));
    fireEvent.click(screen.getByRole('button', { name: 'Vazgeç' }));

    expect(props.onDeleteConversation).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Sil: Silinecek' })).toBeInTheDocument();
  });

  it('liste okunamadiysa nedeni yazar, "konuşma yok" demez', () => {
    renderSidebar({ conversations: [], conversationsError: 'Depolama hatası: disk dolu' });

    expect(screen.getByRole('alert')).toHaveTextContent('Depolama hatası: disk dolu');
    expect(screen.queryByText('Henüz konuşma yok.')).toBeNull();
  });

  it('projeye ve bolume tiklamayi yukari bildirir', () => {
    const props = renderSidebar();

    fireEvent.click(screen.getByRole('button', { name: 'Asuna' }));
    expect(props.onSelectProject).toHaveBeenCalledWith('asuna');

    fireEvent.click(screen.getByRole('button', { name: 'Ses modu' }));
    expect(props.onSelectView).toHaveBeenCalledWith('voice');
  });

  it('yeni konusma acilirken buton kilitlenir', () => {
    renderSidebar({ starting: true });

    expect(screen.getByRole('button', { name: '+ Yeni konuşma' })).toBeDisabled();
  });
});

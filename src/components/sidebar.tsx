/**
 * Sol kenar cubugu (plan-chat-shell.md WP3) — konusmalar, projeler, bolumler.
 *
 * Saf sunum: servis cagirmaz, IPC bilmez, kendi verisini yuklemez. Listeleri ve
 * geri cagrilari kabuk (`src/app/app.tsx`) verir; buradaki tek yerel durum
 * silme onayidir (`window.confirm` yok — WKWebView'de tum pencereyi kilitler ve
 * arka planda canli bir ses oturumu olabilir; `session-list.tsx` ile ayni karar).
 *
 * Tum metinler duz render edilir; baslik model ciktisindan turemis olabilir.
 */

import { useState } from 'react';

import type { ProjectRecord } from '../shared/project';
import type { ConversationSummary } from '../shared/chat';

import {
  conversationTitleOf,
  describeDeleteConfirmation,
  groupConversations,
} from './chat-text';

/** Ana alanda hangi ekranin oldugu. */
export type ShellView = 'chat' | 'projects' | 'memory' | 'tools' | 'settings' | 'voice';

/** Alt navigasyon — bolum listesi tek yerde durur. */
const SHELL_SECTIONS = [
  { id: 'voice', label: 'Ses modu' },
  { id: 'memory', label: 'Hafıza' },
  { id: 'tools', label: 'Araçlar' },
  { id: 'settings', label: 'Ayarlar' },
] as const satisfies readonly { id: ShellView; label: string }[];

export interface SidebarProps {
  readonly view: ShellView;
  readonly conversations: readonly ConversationSummary[];
  readonly conversationsLoading: boolean;
  /** Liste okunamadiysa nedeni; gizlenmez. */
  readonly conversationsError: string | null;
  readonly activeSessionId: number | null;
  readonly projects: readonly ProjectRecord[];
  readonly projectsError: string | null;
  readonly activeProjectId: string | null;
  /** Yeni konusma acilirken buton kilitlenir. */
  readonly starting: boolean;
  readonly busySessionId: number | null;
  readonly onNewConversation: () => void;
  readonly onSelectConversation: (sessionId: number) => void;
  readonly onDeleteConversation: (sessionId: number) => void;
  readonly onSelectProject: (projectId: string) => void;
  readonly onSelectView: (view: ShellView) => void;
  /** Testlerin gruplamayi sabitlemesi icin; uretimde gercek saat. */
  readonly now?: Date;
}

export function Sidebar({
  view,
  conversations,
  conversationsLoading,
  conversationsError,
  activeSessionId,
  projects,
  projectsError,
  activeProjectId,
  starting,
  busySessionId,
  onNewConversation,
  onSelectConversation,
  onDeleteConversation,
  onSelectProject,
  onSelectView,
  now,
}: SidebarProps): React.JSX.Element {
  const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);

  const groups = groupConversations(conversations, now ?? new Date());

  return (
    <nav className="asuna-sidebar" aria-label="Asuna kenar çubuğu">
      <h1 className="asuna-sidebar__brand">Asuna</h1>

      <button
        type="button"
        className="asuna-sidebar__new"
        disabled={starting}
        onClick={onNewConversation}
      >
        + Yeni konuşma
      </button>

      <div className="asuna-sidebar__scroll">
        {conversationsError !== null && (
          <p className="asuna-sidebar__notice" role="alert">
            {conversationsError}
          </p>
        )}

        {conversationsLoading && conversations.length === 0 && (
          <p className="asuna-sidebar__notice">Konuşmalar yükleniyor…</p>
        )}

        {!conversationsLoading && conversationsError === null && conversations.length === 0 && (
          <p className="asuna-sidebar__notice">Henüz konuşma yok.</p>
        )}

        {groups.map((group) => (
          <section key={group.id} className="asuna-sidebar__group" aria-label={group.label}>
            <h2 className="asuna-sidebar__group-title">{group.label}</h2>
            <ul className="asuna-sidebar__list">
              {group.conversations.map((conversation) => {
                const active = view === 'chat' && conversation.id === activeSessionId;
                const title = conversationTitleOf(conversation);

                return (
                  <li
                    key={conversation.id}
                    className="asuna-sidebar__row"
                    data-active={active}
                    data-modality={conversation.modality}
                  >
                    <button
                      type="button"
                      className="asuna-sidebar__row-title"
                      aria-current={active ? 'true' : undefined}
                      onClick={(): void => {
                        setPendingDeleteId(null);
                        onSelectConversation(conversation.id);
                      }}
                    >
                      {title}
                    </button>

                    {/* Rozet butonun DISINDA: satirin erisilebilir adi baslik
                        olarak kalir, tur bilgisi ayrica gorunur (review H1). */}
                    {conversation.modality === 'voice' && (
                      <span className="asuna-sidebar__badge">Ses</span>
                    )}

                    {pendingDeleteId === conversation.id ? (
                      <span
                        className="asuna-sidebar__confirm"
                        role="group"
                        aria-label={`${title} silme onayı`}
                      >
                        {/* Ne kaybedilecegi onaydan ONCE yazili: ses oturumunda
                            silinen sey mesajlar degil, ozet ve disk dokumu. */}
                        <span className="asuna-sidebar__confirm-text">
                          {describeDeleteConfirmation(conversation)}
                        </span>
                        <span className="asuna-sidebar__confirm-actions">
                          <button
                            type="button"
                            disabled={busySessionId === conversation.id}
                            onClick={(): void => {
                              setPendingDeleteId(null);
                              onDeleteConversation(conversation.id);
                            }}
                          >
                            Evet, sil
                          </button>
                          <button
                            type="button"
                            disabled={busySessionId === conversation.id}
                            onClick={(): void => {
                              setPendingDeleteId(null);
                            }}
                          >
                            Vazgeç
                          </button>
                        </span>
                      </span>
                    ) : (
                      <button
                        type="button"
                        className="asuna-sidebar__row-delete"
                        aria-label={`Sil: ${title}`}
                        disabled={busySessionId === conversation.id}
                        onClick={(): void => {
                          setPendingDeleteId(conversation.id);
                        }}
                      >
                        Sil
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>
          </section>
        ))}

        <section className="asuna-sidebar__group" aria-label="Projeler">
          <h2 className="asuna-sidebar__group-title">Projeler</h2>

          {projectsError !== null && (
            <p className="asuna-sidebar__notice" role="alert">
              {projectsError}
            </p>
          )}

          {projectsError === null && projects.length === 0 && (
            <p className="asuna-sidebar__notice">Kayıtlı proje yok.</p>
          )}

          <ul className="asuna-sidebar__list">
            {projects.map((project) => {
              const active = view === 'projects' && project.id === activeProjectId;
              return (
                <li key={project.id} className="asuna-sidebar__row" data-active={active}>
                  <button
                    type="button"
                    className="asuna-sidebar__row-title"
                    aria-current={active ? 'true' : undefined}
                    onClick={(): void => {
                      onSelectProject(project.id);
                    }}
                  >
                    {project.name}
                  </button>
                </li>
              );
            })}
          </ul>

          <button
            type="button"
            className="asuna-sidebar__link"
            aria-current={view === 'projects' && activeProjectId === null ? 'true' : undefined}
            onClick={(): void => {
              onSelectView('projects');
            }}
          >
            Projeleri yönet
          </button>
        </section>
      </div>

      <div className="asuna-sidebar__sections">
        {SHELL_SECTIONS.map((section) => (
          <button
            key={section.id}
            type="button"
            className="asuna-sidebar__link"
            aria-current={view === section.id ? 'true' : undefined}
            onClick={(): void => {
              onSelectView(section.id);
            }}
          >
            {section.label}
          </button>
        ))}
      </div>
    </nav>
  );
}

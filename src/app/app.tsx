import { Suspense, lazy, useCallback, useEffect, useMemo, useState } from 'react';

import {
  deleteConversation,
  listConversations,
  startConversation,
} from '../asuna/agent/chat-service';
import type { UseAsunaSessionOptions } from '../asuna/agent/use-asuna-session';
import { subscribeProjectsChanged } from '../asuna/projects/project-events';
import { listProjects } from '../asuna/projects/project-registry';
import { asunaToolRegistry, ToolToggleStore } from '../asuna/tools';
import { ChatView } from '../components/chat-view';
import { MEMORY_DISABLED_NOTICE, describeChatError } from '../components/chat-text';
import { MemoryView } from '../components/memory-view';
import type { ProjectChatSection } from '../components/project-detail';
import { ProjectsView } from '../components/projects-view';
import { SettingsView } from '../components/settings-view';
import { Sidebar, type ShellView } from '../components/sidebar';
import { ToolsView } from '../components/tools-view';
import { VoicePanel } from '../components/voice-panel';
import type { ConversationSummary } from '../shared/chat';
import type { ProjectRecord } from '../shared/project';
import { describeRegistryError } from '../components/project-text';

import { listCurrentProjectDirectory } from './project-directory-source';

/**
 * Debug log paneli (ASU-019) yalnizca gelistirmede yuklenir.
 *
 * `import.meta.env.DEV` uretim build'inde `false` sabitine indirgenir; ternary
 * olu koda duser ve dinamik `import()` bundle'a hic girmez.
 */
const DebugPanel = import.meta.env.DEV
  ? lazy(async () => {
      const module = await import('../components/debug-panel');
      return { default: module.DebugPanel };
    })
  : null;

/** Ekranda acik olan konusma — kimlik ve projesi birlikte tasinir. */
interface ActiveConversation {
  readonly id: number;
  /** Konusmanin projesi; yoksa `null` (proje dosyasi eklenemez). */
  readonly projectId: string | null;
  /**
   * Oturumun turu. `voice` oturumlar salt okunur acilir: `chat_send` onlari
   * reddediyor ve ekran bunu hataya dusmeden, tasarimla karsilar (review M2).
   */
  readonly modality: 'voice' | 'text';
}

/**
 * Uygulama kabugu — iki kolon: kenar cubugu + ana alan (ADR-006 / chat shell).
 *
 * Kabuk hala ince: burada model cagrisi, SQL ya da dosya sistemi yok. Tuttugu
 * sey **secim** (hangi konusma/hangi ekran), paylasilan tool anahtar seti ve
 * konusma listesinin tek kopyasi.
 *
 * # Pivot notu
 *
 * Eski "sohbet penceresi kurma" direktifi kullanici karariyla degisti: metin
 * sohbeti artik urunun parcasi. Ses **silinmedi** — ChatGPT'deki gibi ayri bir
 * mod (`VoicePanel`).
 *
 * # Degismeyen kural: ses paneli ASLA unmount edilmez
 *
 * `VoicePanel` canli bir Realtime oturumu tasir. Baska bir ekrana gecince
 * unmount edilseydi oturum kopar, kullanici konusurken Asuna susardi. Bu yuzden
 * panel her zaman monte kalir, yalnizca `hidden` ile gizlenir. Diger ekranlar
 * tersine yalnizca acikken monte olur — kapali ekran IPC sorgusu atmasin.
 *
 * # Konusma listesi neden burada
 *
 * Liste iki yerde gorunuyor (kenar cubugu ve proje sayfasi). Iki ayri yukleme
 * iki farkli gercek uretirdi: silinen bir konusma bir tarafta durmaya devam
 * ederdi. Tek kaynak burada, tazeleme tek sinyalle olur.
 */
export function App(): React.JSX.Element {
  const [view, setView] = useState<ShellView>('chat');
  const [active, setActive] = useState<ActiveConversation | null>(null);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);

  const [conversations, setConversations] = useState<readonly ConversationSummary[]>([]);
  const [conversationsError, setConversationsError] = useState<string | null>(null);
  const [loadedToken, setLoadedToken] = useState<number | null>(null);
  const [reloadToken, setReloadToken] = useState(0);

  const [projects, setProjects] = useState<readonly ProjectRecord[]>([]);
  const [projectsError, setProjectsError] = useState<string | null>(null);
  const [projectsToken, setProjectsToken] = useState(0);

  const [starting, setStarting] = useState(false);
  const [busySessionId, setBusySessionId] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  /**
   * Tool tanimlari + anahtarlari (ASU-054) — kabuk **kompozisyon kokudur**.
   *
   * Ikisi de burada bir kez kurulur ve **ayni ornek** iki yere verilir: ses
   * oturumuna (modele giden liste ve `executeTool` kapisi bunlari okur) ve
   * Araclar ekranina.
   *
   * - Ayri **store** olsaydi ekranda "Kapalı" gorunen bir tool calismaya devam
   *   ederdi.
   * - Ayri **liste** olsaydi (ekran registry'yi kendi okusaydi) oturuma
   *   daraltilmis bir liste verildigi anda ekran, modele acik olmayan
   *   tool'lari "Açık" gosterirdi.
   *
   * Ikisi de `useState` ile dondurulur: her render'da yeni bir store,
   * kullanicinin kapattigi tool'u sessizce geri acardi.
   */
  const [toolDefinitions] = useState(() => asunaToolRegistry.list());
  const [toolToggles] = useState(() => new ToolToggleStore());
  const sessionOptions = useMemo<UseAsunaSessionOptions>(
    () => ({ toolToggles, tools: toolDefinitions }),
    [toolToggles, toolDefinitions],
  );

  const conversationsLoading = loadedToken !== reloadToken;

  const refreshConversations = useCallback((): void => {
    setReloadToken((token) => token + 1);
  }, []);

  useEffect(() => {
    let cancelled = false;

    listConversations().then(
      (list) => {
        if (cancelled) {
          return;
        }
        setConversations(list);
        setConversationsError(null);
        setLoadedToken(reloadToken);
      },
      (error: unknown) => {
        if (cancelled) {
          return;
        }
        // Bayat liste gosterilmez: ya dogru veri olur ya da neden olmadigi yazar.
        setConversations([]);
        setConversationsError(describeChatError(error));
        setLoadedToken(reloadToken);
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [reloadToken]);

  // Projeler sekmesinde yapilan bir degisiklik kenar cubugunu da tazeler
  // (ASU-045 sinyali — veri tasimaz, taraflar gercegi servisten okur).
  useEffect(
    () =>
      subscribeProjectsChanged(() => {
        setProjectsToken((token) => token + 1);
      }),
    [],
  );

  useEffect(() => {
    let cancelled = false;

    listProjects().then(
      (records) => {
        if (!cancelled) {
          setProjects(records);
          setProjectsError(null);
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          setProjects([]);
          setProjectsError(describeRegistryError(error));
        }
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [projectsToken]);

  const openConversation = useCallback((conversation: ActiveConversation): void => {
    setActive(conversation);
    setView('chat');
    setNotice(null);
  }, []);

  /**
   * Yeni konusma.
   *
   * Hafiza kapaliysa komut `skipped` doner ve bu **yutulmaz**: sahte bir gecici
   * konusma acmak, kullaniciya kaydedilmeyen bir gecmis vaat etmek olurdu.
   */
  const handleNewConversation = useCallback(
    (projectId: string | null): void => {
      setStarting(true);
      setNotice(null);

      const start = projectId === null ? startConversation() : startConversation(projectId);

      start.then(
        (result) => {
          setStarting(false);
          if (result.status === 'skipped') {
            setNotice(MEMORY_DISABLED_NOTICE);
            return;
          }
          // Yeni acilan konusma her zaman metin: `startConversation`
          // modality='text' ile cagriliyor (chat-service sozlesmesi).
          setActive({ id: result.id, projectId, modality: 'text' });
          setView('chat');
          refreshConversations();
        },
        (error: unknown) => {
          setStarting(false);
          setNotice(describeChatError(error));
        },
      );
    },
    [refreshConversations],
  );

  const handleDeleteConversation = useCallback(
    (sessionId: number): void => {
      setBusySessionId(sessionId);
      setNotice(null);

      deleteConversation(sessionId).then(
        () => {
          setBusySessionId(null);
          setActive((current) =>
            current !== null && current.id === sessionId ? null : current,
          );
          refreshConversations();
        },
        (error: unknown) => {
          setBusySessionId(null);
          setNotice(describeChatError(error));
        },
      );
    },
    [refreshConversations],
  );

  const handleSelectView = useCallback((next: ShellView): void => {
    setView(next);
    setNotice(null);
    if (next === 'projects') {
      // "Projeleri yönet": belirli bir proje secimi yok.
      setActiveProjectId(null);
    }
  }, []);

  const selectedProject = projects.find((project) => project.id === activeProjectId) ?? null;

  const projectChat = useMemo<ProjectChatSection>(
    () => ({
      projectId: activeProjectId,
      projectName: selectedProject?.name ?? null,
      conversations: conversations.filter(
        (conversation) => conversation.projectId === activeProjectId,
      ),
      notice,
      starting,
      onStartConversation: (projectId: string): void => {
        handleNewConversation(projectId);
      },
      onSelectConversation: (sessionId: number): void => {
        const summary = conversations.find((item) => item.id === sessionId) ?? null;
        openConversation({
          id: sessionId,
          projectId: summary?.projectId ?? null,
          modality: summary?.modality ?? 'text',
        });
      },
    }),
    [
      activeProjectId,
      conversations,
      handleNewConversation,
      notice,
      openConversation,
      selectedProject,
      starting,
    ],
  );

  return (
    <div className="asuna-shell">
      <Sidebar
        view={view}
        conversations={conversations}
        conversationsLoading={conversationsLoading}
        conversationsError={conversationsError}
        activeSessionId={active?.id ?? null}
        projects={projects}
        projectsError={projectsError}
        activeProjectId={activeProjectId}
        starting={starting}
        busySessionId={busySessionId}
        onNewConversation={(): void => {
          handleNewConversation(null);
        }}
        onSelectConversation={(sessionId): void => {
          const summary = conversations.find((item) => item.id === sessionId) ?? null;
          openConversation({
            id: sessionId,
            projectId: summary?.projectId ?? null,
            modality: summary?.modality ?? 'text',
          });
        }}
        onDeleteConversation={handleDeleteConversation}
        onSelectProject={(projectId): void => {
          setActiveProjectId(projectId);
          setView('projects');
          setNotice(null);
        }}
        onSelectView={handleSelectView}
      />

      <main className="asuna-main">
        {/* Ses paneli her zaman monte: canli oturum ekran degisiminde kopmaz. */}
        <div
          id="asuna-panel-voice"
          className="asuna-main__panel"
          aria-label="Ses modu"
          hidden={view !== 'voice'}
        >
          <VoicePanel options={sessionOptions} />
        </div>

        {view === 'chat' && (
          <div id="asuna-panel-chat" className="asuna-main__panel">
            {notice !== null && (
              <p className="asuna-main__notice" role="status">
                {notice}
              </p>
            )}

            {active === null ? (
              <div className="asuna-empty">
                <p>
                  Soldan bir konuşma seç ya da yeni bir konuşma başlat. Konuşmalar ve eklenen
                  dosyalar kalıcı olarak saklanır; istediğin an silebilirsin.
                </p>
                <button
                  type="button"
                  disabled={starting}
                  onClick={(): void => {
                    handleNewConversation(null);
                  }}
                >
                  + Yeni konuşma
                </button>
              </div>
            ) : (
              <ChatView
                key={active.id}
                sessionId={active.id}
                projectId={active.projectId}
                modality={active.modality}
                listProjectDirectory={listCurrentProjectDirectory}
                onConversationChanged={refreshConversations}
                onOpenVoice={(): void => {
                  setView('voice');
                }}
              />
            )}
          </div>
        )}

        {/* Projeler yalnizca acikken monte olur: kapali ekran proje listesi
            sormaz. Guncel proje secimi ses panelinde ayrica gorunur (ASU-045). */}
        {view === 'projects' && (
          <div id="asuna-panel-projects" className="asuna-main__panel">
            <ProjectsView chat={projectChat} />
          </div>
        )}

        {view === 'memory' && (
          <div id="asuna-panel-memory" className="asuna-main__panel">
            <MemoryView />
          </div>
        )}

        {/* Araclar ekrani da yalnizca acikken monte olur: kapali ekran denetim
            defterini sorgulamaz (ASU-054). Onay karti bundan bagimsizdir —
            ses panelinden `document.body`'ye portal edilir (ASU-053). */}
        {view === 'tools' && (
          <div id="asuna-panel-tools" className="asuna-main__panel">
            <ToolsView definitions={toolDefinitions} toggles={toolToggles} />
          </div>
        )}

        {view === 'settings' && (
          <div id="asuna-panel-settings" className="asuna-main__panel">
            <SettingsView />
          </div>
        )}
      </main>

      {DebugPanel !== null && (
        <Suspense fallback={null}>
          <DebugPanel />
        </Suspense>
      )}
    </div>
  );
}

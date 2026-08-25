/**
 * Projeler sekmesi (ASU-045) — kayitli kokleri gor, ekle, kaldir, guncel projeyi sec.
 *
 * # Neden var
 *
 * PROJECT.md Bolum 4 / Bolum 19: Asuna yalnizca kullanicinin **acikca**
 * kaydettigi proje koklerini bilir ve "su an hangi projedeyiz?" sorusunun
 * cevabi her an gorunur olmali. Bu ekran o kaydin tek gorunur yuzu: otomatik
 * disk taramasi yok, tahmin yok — listede ne varsa kullanici oraya koymustur.
 *
 * # Sinirlar
 *
 * - Bilesen `invoke` cagirmaz, dosya sistemine dokunmaz, dizin secicinin
 *   plugin'ini bile dogrudan tanimaz: her sey [`ProjectsViewPort`] uzerinden
 *   `src/asuna/projects/*` servislerine gider (ADR-005 / CLAUDE.md).
 * - Yol dogrulamasi burada **yapilmaz**. Kullanicinin sectigi ya da yapistirdigi
 *   metin oldugu gibi `project_add`'e gider; mutlak olma, var olma, symlink
 *   cozumu ve dizin olma kontrolu Rust tarafinin isi (renderer'da yapilan
 *   dogrulama guvenlik siniri olusturamaz).
 * - Sekme minimal (R7): arama, siralama, surukle-birak, proje duzenleme yok.
 *   Ekranin isi guven uretmek, dashboard olmak degil.
 */

import { useCallback, useEffect, useState } from 'react';

import {
  fetchProjectContext,
  type ProjectContextResult,
} from '../asuna/projects/project-context';
import { pickProjectDirectory } from '../asuna/projects/directory-picker';
import { notifyProjectsChanged } from '../asuna/projects/project-events';
import {
  addProject,
  currentProjectOf,
  listProjects,
  removeProject,
  setCurrentProject,
} from '../asuna/projects/project-registry';
import { listSessions } from '../asuna/memory/session-service';
import type { ProjectAddOutcome, ProjectRecord, ProjectRemoveOutcome } from '../shared/project';
import type { SessionListItem, SessionPage } from '../shared/session';

import { describeMemoryError } from './memory-text';
import { ProjectDetail } from './project-detail';
import { ProjectItem } from './project-item';
import {
  describeAddOutcome,
  describeRegistryError,
  describeRemoveOutcome,
} from './project-text';

/**
 * Bilesenin servis yuzeyi. Testler gercek IPC'ye ve gercek dizin secicisine
 * dokunmadan sahte port verir; uretimde [`DEFAULT_PROJECTS_PORT`] kullanilir.
 */
export interface ProjectsViewPort {
  readonly list: () => Promise<readonly ProjectRecord[]>;
  readonly add: (path: string) => Promise<ProjectAddOutcome>;
  readonly remove: (projectId: string) => Promise<ProjectRemoveOutcome>;
  readonly setCurrent: (projectId: string) => Promise<ProjectRecord>;
  /** Sistem dizin secici; kullanici vazgecerse `null`. */
  readonly pickDirectory: () => Promise<string | null>;
  /** Guncel projenin baglami (ASU-044). Reddetmez; `unavailable` doner. */
  readonly loadContext: () => Promise<ProjectContextResult>;
  /**
   * Son oturum ozeti icin **ayri** bir IPC yuzeyi (`asuna-session-read`).
   * Ayni ekranda gorunuyor olmasi onu proje yetkisi yapmaz.
   */
  readonly listSessions: (limit?: number) => Promise<SessionPage>;
}

/** Uretim portu: dogrudan servis katmani. */
const DEFAULT_PROJECTS_PORT: ProjectsViewPort = {
  list: listProjects,
  add: (path) => addProject(path),
  remove: removeProject,
  setCurrent: setCurrentProject,
  pickDirectory: pickProjectDirectory,
  loadContext: fetchProjectContext,
  listSessions,
};

interface Notice {
  readonly tone: 'info' | 'error';
  readonly text: string;
}

interface DetailState {
  /** Hangi proje + hangi tazeleme icin yuklendi. */
  readonly key: string;
  readonly result: ProjectContextResult;
  readonly lastSession: SessionListItem | null;
  readonly lastSessionError: string | null;
}

export interface ProjectsViewProps {
  readonly port?: ProjectsViewPort;
}

export function ProjectsView({
  port = DEFAULT_PROJECTS_PORT,
}: ProjectsViewProps): React.JSX.Element {
  const [projects, setProjects] = useState<readonly ProjectRecord[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadedToken, setLoadedToken] = useState<number | null>(null);
  const [reloadToken, setReloadToken] = useState(0);

  const [pathDraft, setPathDraft] = useState('');
  const [adding, setAdding] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [pendingRemoveId, setPendingRemoveId] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  const [detail, setDetail] = useState<DetailState | null>(null);

  /**
   * "Yukleniyor" bir state degil, **turetilmis** bir gercek: en son tamamlanan
   * yukleme ile istenen ayni degilse liste bayattir (bkz. `memory-view.tsx`).
   */
  const loading = loadedToken !== reloadToken;

  const current = currentProjectOf(projects);
  const currentId = current === null ? null : current.id;
  const detailKey = currentId === null ? null : `${currentId} ${reloadToken.toString()}`;
  const detailLoading = detailKey !== null && detail?.key !== detailKey;

  useEffect(() => {
    let cancelled = false;

    port.list().then(
      (records) => {
        if (cancelled) {
          return;
        }
        setProjects(records);
        setLoadError(null);
        setLoadedToken(reloadToken);
      },
      (error: unknown) => {
        if (cancelled) {
          return;
        }
        // Hata varken bayat liste gosterilmez: ekranda ya dogru veri olur ya da
        // neden olmadigi yazar.
        setProjects([]);
        setLoadError(describeRegistryError(error));
        setLoadedToken(reloadToken);
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [port, reloadToken]);

  // Detay yalnizca guncel proje varken sorulur: secim yoksa komutun cevabi da
  // "bilinmiyor" olurdu, bunu bos yere IPC ile ogrenmeye gerek yok.
  useEffect(() => {
    if (detailKey === null) {
      return undefined;
    }

    let cancelled = false;

    Promise.all([
      port.loadContext(),
      port.listSessions(1).then(
        (page): { session: SessionListItem | null; error: string | null } => ({
          session: page.sessions[0] ?? null,
          error: null,
        }),
        (error: unknown): { session: SessionListItem | null; error: string | null } => ({
          session: null,
          error: describeMemoryError(error),
        }),
      ),
    ]).then(
      ([result, session]) => {
        if (!cancelled) {
          setDetail({
            key: detailKey,
            result,
            lastSession: session.session,
            lastSessionError: session.error,
          });
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          setDetail({
            key: detailKey,
            result: { status: 'unavailable', message: describeRegistryError(error) },
            lastSession: null,
            lastSessionError: null,
          });
        }
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [port, detailKey]);

  /**
   * Yazma islemlerinin ortak yolu.
   *
   * Her yazmadan sonra liste **yeniden okunur** ve ses paneline sinyal gider:
   * ekranda gorunen sey backend'in kabul ettigi durum olur, UI'nin tahmini
   * degil (guncel proje overlay'de de gorunuyor — PROJECT.md Bolum 19).
   */
  const finishWrite = useCallback((message: Notice): void => {
    setNotice(message);
    setPendingRemoveId(null);
    setBusyId(null);
    setAdding(false);
    setReloadToken((token) => token + 1);
    notifyProjectsChanged();
  }, []);

  const submitAdd = useCallback(
    (path: string): void => {
      setAdding(true);
      setNotice(null);

      port.add(path).then(
        (outcome) => {
          setPathDraft('');
          finishWrite({ tone: 'info', text: describeAddOutcome(outcome) });
        },
        (error: unknown) => {
          setAdding(false);
          setNotice({ tone: 'error', text: describeRegistryError(error) });
        },
      );
    },
    [port, finishWrite],
  );

  const handlePick = useCallback((): void => {
    setNotice(null);

    port.pickDirectory().then(
      (path) => {
        if (path === null) {
          // Vazgecmek bir hata degil; sessizce geri donulur.
          return;
        }
        // Secilen yol gorunur kalsin: kullanici neyi ekledigini gorsun.
        setPathDraft(path);
        submitAdd(path);
      },
      (error: unknown) => {
        // Dizin secici acilamadi (izin/plugin) — yutulmaz.
        setNotice({
          tone: 'error',
          text: `Dizin seçici açılamadı: ${describeRegistryError(error)}`,
        });
      },
    );
  }, [port, submitAdd]);

  const handleAddTyped = useCallback((): void => {
    const path = pathDraft.trim();
    if (path === '') {
      setNotice({ tone: 'error', text: 'Bir dizin seçin ya da proje kökünün yolunu yazın.' });
      return;
    }
    submitAdd(path);
  }, [pathDraft, submitAdd]);

  const handleSetCurrent = useCallback(
    (project: ProjectRecord): void => {
      setBusyId(project.id);
      setNotice(null);

      port.setCurrent(project.id).then(
        (record) => {
          finishWrite({ tone: 'info', text: `Güncel proje: ${record.name}` });
        },
        (error: unknown) => {
          setBusyId(null);
          setNotice({ tone: 'error', text: describeRegistryError(error) });
        },
      );
    },
    [port, finishWrite],
  );

  const handleConfirmRemove = useCallback(
    (projectId: string): void => {
      setBusyId(projectId);
      setNotice(null);

      port.remove(projectId).then(
        (outcome) => {
          // `unlinked` sonucu "sildim" degildir: kayit kaldirildi, hafiza
          // etiketi korundu (ASU-040).
          finishWrite({ tone: 'info', text: describeRemoveOutcome(outcome) });
        },
        (error: unknown) => {
          setBusyId(null);
          setNotice({ tone: 'error', text: describeRegistryError(error) });
        },
      );
    },
    [port, finishWrite],
  );

  return (
    <section className="asuna-projects" aria-label="Projeler">
      <p className="asuna-projects__note">
        Asuna yalnızca buraya <strong>açıkça</strong> eklediğiniz proje köklerini bilir. Disk
        taraması yapılmaz; listede olmayan bir proje sorulduğunda Asuna bilmediğini söyler.
      </p>

      <div className="asuna-projects__add">
        <button type="button" disabled={adding} onClick={handlePick}>
          Dizin seç
        </button>

        <label className="asuna-projects__field">
          <span>veya yolu yazın</span>
          <input
            type="text"
            value={pathDraft}
            disabled={adding}
            autoComplete="off"
            spellCheck={false}
            placeholder="/Users/…/proje"
            onChange={(event): void => {
              setPathDraft(event.target.value);
            }}
          />
        </label>

        <button type="button" disabled={adding} onClick={handleAddTyped}>
          Ekle
        </button>
      </div>

      {notice !== null && (
        <p
          className="asuna-projects__notice"
          role={notice.tone === 'error' ? 'alert' : 'status'}
        >
          {notice.text}
        </p>
      )}

      {loadError !== null && (
        <p className="asuna-projects__notice" role="alert">
          {loadError}
        </p>
      )}

      {loading && projects.length === 0 && (
        <p className="asuna-projects__notice">Projeler yükleniyor…</p>
      )}

      {!loading && loadError === null && projects.length === 0 && (
        <p className="asuna-projects__notice">Henüz kayıtlı proje yok.</p>
      )}

      {projects.length > 0 && (
        <ul className="asuna-projects__list" aria-label="Kayıtlı projeler">
          {projects.map((project) => (
            <ProjectItem
              key={project.id}
              project={project}
              current={current !== null && current.id === project.id}
              confirmingRemove={pendingRemoveId === project.id}
              busy={busyId === project.id}
              onSetCurrent={handleSetCurrent}
              onRequestRemove={(projectId): void => {
                setPendingRemoveId(projectId);
                setNotice(null);
              }}
              onCancelRemove={(): void => {
                setPendingRemoveId(null);
              }}
              onConfirmRemove={handleConfirmRemove}
            />
          ))}
        </ul>
      )}

      <ProjectDetail
        result={detail !== null && detail.key === detailKey ? detail.result : null}
        loading={detailLoading}
        lastSession={detail !== null && detail.key === detailKey ? detail.lastSession : null}
        lastSessionError={
          detail !== null && detail.key === detailKey ? detail.lastSessionError : null
        }
      />
    </section>
  );
}

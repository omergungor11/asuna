/**
 * `ProjectsView` testleri (ASU-045).
 *
 * Kanitlanan seyler:
 * 1. Liste **denetlenebilir**: ad, yol, dil/cati ve son acilma gorunur; yolu
 *    kaybolmus proje acikca isaretli (`missing`).
 * 2. Ekleme iki yoldan da calisir: sistem dizin secici (`{ directory: true }`)
 *    ve elle yazilan yol. Secilen metin **oldugu gibi** servise gider — UI yol
 *    dogrulamasi yapmaz (o Rust tarafinin isi).
 * 3. Kaldirma onay ister ve `unlinked` sonucunda "sildim" demez: kayit
 *    kaldirildi, hafiza etiketi korundu.
 * 4. "Guncel proje" kullanicinin acik eylemi; secim sonrasi liste **servisten**
 *    yeniden okunur.
 * 5. Detay yuklenemezse sekme calismaya devam eder, ama neden yuklenemedigi
 *    ekranda yazar.
 *
 * Servis katmani sahte port ile degistirilir: gercek `invoke` ve gercek dizin
 * secici yok.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, type Mock } from 'vitest';

import type { ProjectContextResult } from '../asuna/projects/project-context';
import {
  AsunaRegistryError,
  type ProjectAddOutcome,
  type ProjectRecord,
  type ProjectRemoveOutcome,
} from '../shared/project';
import type { SessionPage } from '../shared/session';

import { ProjectsView, type ProjectsViewPort } from './projects-view';

function project(overrides: Partial<ProjectRecord> = {}): ProjectRecord {
  return {
    id: 'asuna',
    name: 'Asuna',
    path: '/Users/arlec/Work/asuna',
    description: null,
    status: 'active',
    primaryLanguage: 'TypeScript',
    framework: 'React',
    gitRemote: 'github.com/omergungor/asuna',
    lastOpenedAt: '2026-08-24T09:30:00Z',
    createdAt: '2026-08-01T09:30:00Z',
    updatedAt: '2026-08-24T09:30:00Z',
    metadataJson: '{}',
    ...overrides,
  };
}

const ASUNA = project();
const ESKI = project({
  id: 'eski-proje',
  name: 'Eski Proje',
  path: '/Volumes/Harici/eski',
  status: 'missing',
  primaryLanguage: null,
  framework: null,
  lastOpenedAt: null,
});

const KNOWN_CONTEXT: ProjectContextResult = {
  status: 'known',
  detail: {
    projectId: 'asuna',
    name: 'Asuna',
    path: '/Users/arlec/Work/asuna',
    sources: [{ name: 'README.md', excerpt: 'Sesli asistan.', truncated: false }],
    git: {
      isRepository: true,
      branch: 'feat/asu-045',
      detached: false,
      dirty: false,
      changedTrackedFiles: 0,
      degraded: false,
    },
    handoff: {
      objective: null,
      currentMilestone: null,
      activeTask: null,
      blockers: [],
      ignoredMessage: null,
    },
    truncated: false,
  },
};

const SESSION_PAGE: SessionPage = {
  sessions: [
    {
      id: 12,
      startedAt: '2026-08-24T08:00:00Z',
      endedAt: '2026-08-24T08:20:00Z',
      endReason: 'completed',
      summaryPreview: 'Projeler sekmesi konusuldu.',
      summaryTruncated: false,
      hasTranscriptFile: true,
    },
  ],
  limit: 1,
  limitMax: 200,
  total: 1,
};

interface TestPort extends ProjectsViewPort {
  readonly rows: ProjectRecord[];
  readonly list: Mock<() => Promise<readonly ProjectRecord[]>>;
  readonly add: Mock<(path: string) => Promise<ProjectAddOutcome>>;
  readonly remove: Mock<(projectId: string) => Promise<ProjectRemoveOutcome>>;
  readonly setCurrent: Mock<(projectId: string) => Promise<ProjectRecord>>;
  readonly pickDirectory: Mock<() => Promise<string | null>>;
  readonly loadContext: Mock<() => Promise<ProjectContextResult>>;
  readonly listSessions: Mock<(limit?: number) => Promise<SessionPage>>;
}

function createPort(initial: readonly ProjectRecord[]): TestPort {
  const rows = [...initial];

  const port: TestPort = {
    rows,
    list: vi.fn(() => Promise.resolve([...rows])),
    add: vi.fn((path: string) => {
      const added = project({ id: 'yeni', name: 'yeni', path, lastOpenedAt: null });
      rows.push(added);
      return Promise.resolve<ProjectAddOutcome>({ status: 'registered', project: added });
    }),
    remove: vi.fn((projectId: string) => {
      const index = rows.findIndex((row) => row.id === projectId);
      const [removed] = rows.splice(index, 1);
      return Promise.resolve<ProjectRemoveOutcome>({
        status: 'deleted',
        id: removed?.id ?? projectId,
      });
    }),
    setCurrent: vi.fn((projectId: string) => {
      const index = rows.findIndex((row) => row.id === projectId);
      const row = rows[index];
      if (row === undefined) {
        return Promise.reject(new AsunaRegistryError('not-found', 'Proje kaydi yok.'));
      }
      // "Guncel proje" ayri bir bayrak degil: en son acilan kayit.
      const opened = { ...row, lastOpenedAt: '2026-08-25T10:00:00Z' };
      rows.splice(index, 1, opened);
      return Promise.resolve(opened);
    }),
    pickDirectory: vi.fn(() => Promise.resolve<string | null>(null)),
    loadContext: vi.fn(() => Promise.resolve(KNOWN_CONTEXT)),
    listSessions: vi.fn(() => Promise.resolve(SESSION_PAGE)),
  };

  return port;
}

async function findRow(name: string): Promise<HTMLElement> {
  const heading = await screen.findByRole('heading', { name });
  const row = heading.closest('li');
  if (row === null) {
    throw new Error(`"${name}" satiri bulunamadi`);
  }
  return row;
}

describe('ProjectsView', () => {
  it('kayitli projeleri ad, yol, dil/cati ve son acilma ile listeler', async () => {
    render(<ProjectsView port={createPort([ASUNA])} />);

    const row = await findRow('Asuna');
    expect(row).toHaveTextContent('/Users/arlec/Work/asuna');
    expect(row).toHaveTextContent('TypeScript · React');
    expect(row).toHaveTextContent('son açılma: 2026-08-24');
  });

  it('yolu kaybolmus projeyi acikca isaretler ve kaydin silinmedigini soyler', async () => {
    render(<ProjectsView port={createPort([ASUNA, ESKI])} />);

    const row = await findRow('Eski Proje');
    expect(row).toHaveTextContent('yolu bulunamıyor');
    expect(row).toHaveTextContent('Kayıt silinmedi');
    // Tespit edilememis dil gizlenmez.
    expect(row).toHaveTextContent('dil/çatı bilinmiyor');
    expect(row).toHaveTextContent('hiç açılmadı');
  });

  it('dizin secicinin verdigi yolu oldugu gibi servise gonderir', async () => {
    const port = createPort([]);
    port.pickDirectory.mockResolvedValueOnce('/Users/arlec/Work/yeni-proje');

    render(<ProjectsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Dizin seç' }));

    await waitFor(() => {
      expect(port.add).toHaveBeenCalledWith('/Users/arlec/Work/yeni-proje');
    });
    expect(await screen.findByRole('status')).toHaveTextContent('Proje eklendi');
  });

  it('kullanici dizin secmekten vazgecerse hicbir sey eklenmez', async () => {
    const port = createPort([]);
    port.pickDirectory.mockResolvedValueOnce(null);

    render(<ProjectsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Dizin seç' }));

    await waitFor(() => {
      expect(port.pickDirectory).toHaveBeenCalled();
    });
    expect(port.add).not.toHaveBeenCalled();
  });

  it('elle yazilan yol da eklenebilir; bos giriste servise gidilmez', async () => {
    const port = createPort([]);
    render(<ProjectsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ekle' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Bir dizin seçin');
    expect(port.add).not.toHaveBeenCalled();

    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: '  /Users/arlec/Work/elle  ' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Ekle' }));

    await waitFor(() => {
      expect(port.add).toHaveBeenCalledWith('/Users/arlec/Work/elle');
    });
  });

  it('reddedilen yolun nedeni ekranda kalir', async () => {
    const port = createPort([]);
    port.add.mockRejectedValueOnce(
      new AsunaRegistryError('path-not-found', '/yok/boyle/bir/yer bulunamadi'),
    );

    render(<ProjectsView port={port} />);

    fireEvent.change(await screen.findByRole('textbox'), {
      target: { value: '/yok/boyle/bir/yer' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Ekle' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Yol bulunamadı: /yok/boyle/bir/yer bulunamadi',
    );
  });

  it('kaldirma once onay ister, sonra listeyi servisten tazeler', async () => {
    const port = createPort([ASUNA]);
    render(<ProjectsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Kaldır: Asuna' }));
    expect(port.remove).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Evet, kaldır' }));

    await waitFor(() => {
      expect(port.remove).toHaveBeenCalledWith('asuna');
    });
    expect(await screen.findByText('Henüz kayıtlı proje yok.')).toBeInTheDocument();
  });

  it('kaldirma onayindan vazgecilebilir', async () => {
    const port = createPort([ASUNA]);
    render(<ProjectsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Kaldır: Asuna' }));
    fireEvent.click(screen.getByRole('button', { name: 'Vazgeç' }));

    expect(screen.getByRole('button', { name: 'Kaldır: Asuna' })).toBeInTheDocument();
    expect(port.remove).not.toHaveBeenCalled();
  });

  it('hafizaya bagli proje kaldirilinca "sildim" demez', async () => {
    const port = createPort([ASUNA]);
    port.remove.mockResolvedValueOnce({
      status: 'unlinked',
      project: project({ path: null, status: 'unlinked', lastOpenedAt: null }),
      references: 4,
    });

    render(<ProjectsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Kaldır: Asuna' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, kaldır' }));

    const notice = await screen.findByRole('status');
    expect(notice).toHaveTextContent('Kayıt kaldırıldı, hafıza etiketi korundu');
    expect(notice).toHaveTextContent('4 kayıt');
    expect(notice).toHaveTextContent('Hafıza silinmedi.');
  });

  it('guncel proje kullanicinin acik eylemiyle secilir ve gorunur olur', async () => {
    const port = createPort([
      project({ id: 'bir', name: 'Bir', lastOpenedAt: null }),
      project({ id: 'iki', name: 'İki', lastOpenedAt: null }),
    ]);

    render(<ProjectsView port={port} />);

    // Hicbiri acilmamisken guncel proje yok: detay sorulmaz, kullaniciya sorulur.
    expect(await screen.findByText(/Güncel proje seçilmedi/)).toBeInTheDocument();
    expect(port.loadContext).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Güncel proje yap: İki' }));

    await waitFor(() => {
      expect(port.setCurrent).toHaveBeenCalledWith('iki');
    });
    const row = await findRow('İki');
    await waitFor(() => {
      expect(row).toHaveTextContent('güncel');
    });
    // Secim sonrasi detay artik sorulabilir.
    await waitFor(() => {
      expect(port.loadContext).toHaveBeenCalled();
    });
  });

  it('kayitli kokü olmayan etiket guncel proje yapilamaz', async () => {
    const port = createPort([
      project({
        id: 'etiket',
        name: 'Etiket',
        path: null,
        status: 'unlinked',
        lastOpenedAt: null,
      }),
    ]);

    render(<ProjectsView port={port} />);

    expect(
      await screen.findByRole('button', { name: 'Güncel proje yap: Etiket' }),
    ).toBeDisabled();
    expect(await findRow('Etiket')).toHaveTextContent('yalnızca hafıza etiketi');
  });

  it('guncel projenin git dali ve son oturum ozeti gorunur', async () => {
    render(<ProjectsView port={createPort([ASUNA])} />);

    const detail = await screen.findByRole('region', { name: 'Güncel proje detayı' });
    await waitFor(() => {
      expect(detail).toHaveTextContent('feat/asu-045');
    });
    expect(detail).toHaveTextContent('çalışma ağacı temiz');
    expect(detail).toHaveTextContent('Projeler sekmesi konusuldu.');
    expect(detail).toHaveTextContent('README.md');
    expect(detail).toHaveTextContent('Sesli asistan.');
  });

  it('detay yuklenemezse liste calisir ama neden yazilir', async () => {
    const port = createPort([ASUNA]);
    port.loadContext.mockResolvedValueOnce({
      status: 'unavailable',
      message: 'project_context not allowed',
    });

    render(<ProjectsView port={port} />);

    expect(await screen.findByRole('heading', { name: 'Asuna' })).toBeInTheDocument();
    expect(
      await screen.findByText(/Detay yüklenemedi: project_context not allowed/),
    ).toBeInTheDocument();
  });

  it('liste okunamazsa bayat veri gosterilmez, hata yazilir', async () => {
    const port = createPort([]);
    port.list.mockRejectedValueOnce(
      new AsunaRegistryError('disabled', 'ASUNA_MEMORY_ENABLED=false'),
    );

    render(<ProjectsView port={port} />);

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Hafıza kapalı olduğu için proje kaydı tutulamıyor.',
    );
  });
});

/**
 * `ProjectFilePicker` testleri (plan-chat-shell.md WP4 — bosluk analizi).
 *
 * Kanitlanan seyler:
 *
 * 1. **Renderer mutlak yol ya da `..` kuramaz**: gezinme yolu girdi adlarindan
 *    yeniden kurulur, ust klasore cikis kok'te durur. Kaynaga giden HER yol
 *    dogrulanir — sandbox'in onundeki ilk kapi bu.
 * 2. Tarama tavanina takilan dizinde sayi **kesinmis gibi** gosterilmez
 *    (`scanCapped` → "en az N girdi var").
 * 3. Blok listesindeki girdi gizlenmez, "okunamaz" olarak isaretlenir ve
 *    secilemez.
 * 4. Klasor okunamazsa bayat liste degil, nedeni gorunur.
 *
 * IPC yok: dizin kaynagi bir sahte fonksiyon (`ProjectDirectorySource` portu).
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ProjectDirectoryView } from '../asuna/tools/list-project-files';
import { AsunaStoreError } from '../shared/store-error';

import { ProjectFilePicker, type ProjectDirectorySource } from './project-file-picker';

const ROOT: ProjectDirectoryView = {
  projectId: 'asuna',
  projectName: 'Asuna',
  path: '',
  entries: [
    { name: 'src', kind: 'dir', sizeBytes: null, blocked: false },
    { name: 'README.md', kind: 'file', sizeBytes: 1024, blocked: false },
    { name: '.env', kind: 'file', sizeBytes: 120, blocked: true },
  ],
  totalEntries: 3,
  returnedEntries: 3,
  truncated: false,
  scanCapped: false,
  maxEntries: 200,
};

const SRC: ProjectDirectoryView = {
  ...ROOT,
  path: 'src',
  entries: [{ name: 'app.tsx', kind: 'file', sizeBytes: 2048, blocked: false }],
  totalEntries: 1,
  returnedEntries: 1,
};

interface FakeSource {
  readonly source: ProjectDirectorySource;
  /** Kaynaga gonderilen yollar, sirasiyla. */
  readonly paths: () => readonly string[];
}

function sourceFor(views: Readonly<Record<string, ProjectDirectoryView>>): FakeSource {
  const seen: string[] = [];
  const source = vi.fn<ProjectDirectorySource>((path) => {
    seen.push(path);
    const view = views[path];
    return view === undefined
      ? Promise.reject(new Error(`beklenmeyen yol: ${path}`))
      : Promise.resolve(view);
  });

  return { source, paths: (): readonly string[] => seen };
}

const noop = (): void => {
  /* test bunu kullanmiyor */
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe('ProjectFilePicker', () => {
  it('kok klasoru acar ve dosyayi kok"e gore yoluyla secer', async () => {
    const { source } = sourceFor({ '': ROOT });
    const onPick = vi.fn();

    render(<ProjectFilePicker source={source} onPick={onPick} onClose={noop} />);

    fireEvent.click(await screen.findByRole('button', { name: 'README.md · 1 KB' }));

    expect(onPick).toHaveBeenCalledExactlyOnceWith('README.md');
    expect(source).toHaveBeenCalledWith('');
  });

  it('alt klasore girer ve yolu adlardan kurar', async () => {
    const { source, paths } = sourceFor({ '': ROOT, src: SRC });
    const onPick = vi.fn();

    render(<ProjectFilePicker source={source} onPick={onPick} onClose={noop} />);

    fireEvent.click(await screen.findByRole('button', { name: 'src/' }));
    fireEvent.click(await screen.findByRole('button', { name: 'app.tsx · 2 KB' }));

    expect(onPick).toHaveBeenCalledExactlyOnceWith('src/app.tsx');
    expect(paths()).toEqual(['', 'src']);
  });

  /**
   * **Guvenlik siniri**: yukari cikis `..` metni gondermez, yolu kirpar; kok'te
   * "Üst klasör" hic gorunmez. Boylece traversal denemesi komuta hic ulasmaz.
   */
  it('ust klasore cikarken `..` gondermez ve kok"un ustune cikamaz', async () => {
    const { source, paths } = sourceFor({ '': ROOT, src: SRC });

    render(<ProjectFilePicker source={source} onPick={noop} onClose={noop} />);

    // Kok'te ust klasor yok.
    expect(await screen.findByRole('button', { name: 'src/' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '↑ Üst klasör' })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'src/' }));
    fireEvent.click(await screen.findByRole('button', { name: '↑ Üst klasör' }));

    await waitFor(() => {
      expect(paths()).toEqual(['', 'src', '']);
    });

    for (const path of paths()) {
      expect(path, `kaynaga giden yol: ${path}`).not.toContain('..');
      expect(path.startsWith('/'), `mutlak yol gonderildi: ${path}`).toBe(false);
      expect(path).not.toContain('~');
    }
  });

  it('blok listesindeki dosyayi gizlemez, secilemez isaretler', async () => {
    const { source } = sourceFor({ '': ROOT });
    const onPick = vi.fn();

    render(<ProjectFilePicker source={source} onPick={onPick} onClose={noop} />);

    const blocked = await screen.findByRole('button', { name: '.env · 120 B' });
    expect(blocked).toBeDisabled();
    expect(screen.getByText('okunamaz')).toBeInTheDocument();

    fireEvent.click(blocked);
    expect(onPick).not.toHaveBeenCalled();
  });

  /**
   * `scanCapped`: `totalEntries` bir **alt sinirdir**. "toplam 200" yazmak
   * kullaniciya olmayan bir kesinlik satardi.
   */
  it('tarama tavanina takilan klasorde sayiyi kesin gibi gostermez', async () => {
    const { source } = sourceFor({
      '': {
        ...ROOT,
        truncated: true,
        scanCapped: true,
        returnedEntries: 200,
        totalEntries: 5000,
      },
    });

    render(<ProjectFilePicker source={source} onPick={noop} onClose={noop} />);

    const notice = await screen.findByText(/Yalnızca ilk 200 girdi/);
    expect(notice).toHaveTextContent('en az 5000 girdi var');
    expect(notice).not.toHaveTextContent('toplam');
  });

  it('tavana takilmadan kirpildiysa toplami yazar', async () => {
    const { source } = sourceFor({
      '': { ...ROOT, truncated: true, scanCapped: false, returnedEntries: 200, totalEntries: 240 },
    });

    render(<ProjectFilePicker source={source} onPick={noop} onClose={noop} />);

    expect(await screen.findByText(/Yalnızca ilk 200 girdi/)).toHaveTextContent('toplam 240');
  });

  it('bos klasoru bos oldugunu soyleyerek gosterir', async () => {
    const { source } = sourceFor({
      '': { ...ROOT, entries: [], totalEntries: 0, returnedEntries: 0 },
    });

    render(<ProjectFilePicker source={source} onPick={noop} onClose={noop} />);

    expect(await screen.findByText('Bu klasör boş.')).toBeInTheDocument();
  });

  it('klasor okunamazsa nedeni gorunur, liste gosterilmez', async () => {
    const source = vi.fn<ProjectDirectorySource>(() =>
      Promise.reject(new AsunaStoreError('invalid', 'yol proje kokunun disinda')),
    );

    render(<ProjectFilePicker source={source} onPick={noop} onClose={noop} />);

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('yol proje kokunun disinda');
    expect(screen.queryByRole('list', { name: 'Klasör içeriği' })).toBeNull();
  });

  it('ekleme surerken hicbir girdi secilemez', async () => {
    const { source } = sourceFor({ '': ROOT });

    render(<ProjectFilePicker source={source} onPick={noop} onClose={noop} busy />);

    expect(await screen.findByRole('button', { name: 'README.md · 1 KB' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'src/' })).toBeDisabled();
  });
});

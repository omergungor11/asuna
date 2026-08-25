/**
 * `MemoryView` testleri (ASU-036).
 *
 * Kanitlanan seyler:
 * 1. Hafiza **denetlenebilir**: kayit, turu, tarihi ve kaynak oturumu gorunur.
 * 2. Arama/filtre servise dogru `MemoryFilter` ile gider — UI kendi kendine
 *    filtrelemez, `markAccessed` sizdirmaz.
 * 3. Silme onay ister, onaydan sonra liste **gercekten** tutarlidir.
 * 4. Hafiza kapali / bozuk / hatali durumlarda dogru cumle cikar; hicbiri
 *    digerinin yerine gecmez.
 *
 * Servis katmani sahte port ile degistirilir: gercek `invoke` yok.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, type Mock } from 'vitest';

import type { DbStatus } from '../shared/db-status';
import type {
  MemoryFilter,
  MemoryPatch,
  MemoryRecord,
  MemoryWriteResult,
} from '../shared/memory';
import { AsunaStoreError } from '../shared/store-error';

import { MemoryView, type MemoryViewPort } from './memory-view';

const READY: DbStatus = {
  availability: 'ready',
  schemaVersion: 1,
  sqliteVersion: '3.46.0',
  reason: null,
};

function record(overrides: Partial<MemoryRecord> = {}): MemoryRecord {
  return {
    id: 1,
    kind: 'decision',
    title: 'Wake word yerel kalir',
    content: 'Wake word tespiti bulutta degil, cihazda calisir.',
    summary: null,
    projectId: 'asuna',
    importance: 0.9,
    confidence: 1,
    sourceSessionId: 7,
    createdAt: '2026-08-20T09:30:00Z',
    updatedAt: '2026-08-20T09:30:00Z',
    lastAccessedAt: null,
    expiresAt: null,
    isArchived: false,
    metadataJson: '{}',
    ...overrides,
  };
}

const DECISION = record();
const IDEA = record({
  id: 2,
  kind: 'idea',
  title: 'Overlay her zaman ustte',
  content: 'Overlay diger pencerelerin ustunde kalsin.',
  sourceSessionId: null,
});

interface TestPort extends MemoryViewPort {
  /** Depodaki kayitlar; silme/arsivleme burada gercekten uygulanir. */
  readonly rows: MemoryRecord[];
  /** Servise giden son filtre — hangi sorgunun atildigi test edilebilsin. */
  readonly lastFilter: () => MemoryFilter | undefined;
  readonly fetchStatus: Mock<() => Promise<DbStatus>>;
  readonly list: Mock<(filter: MemoryFilter) => Promise<readonly MemoryRecord[]>>;
  readonly archive: Mock<(id: number, archived: boolean) => Promise<MemoryWriteResult>>;
  readonly remove: Mock<(id: number) => Promise<MemoryWriteResult>>;
  readonly update: Mock<(id: number, patch: MemoryPatch) => Promise<MemoryWriteResult>>;
}

function createPort(initial: readonly MemoryRecord[], status: DbStatus = READY): TestPort {
  const rows = [...initial];

  const fetchStatus = vi.fn(() => Promise.resolve(status));

  const list = vi.fn((filter: MemoryFilter) =>
    Promise.resolve(
      rows
        .filter((row) => (filter.archived === 'archived' ? row.isArchived : true))
        .slice(0, filter.limit ?? rows.length),
    ),
  );

  const archive = vi.fn((id: number, archived: boolean): Promise<MemoryWriteResult> => {
    const index = rows.findIndex((row) => row.id === id);
    const updated = { ...rows[index]!, isArchived: archived };
    rows[index] = updated;
    return Promise.resolve({ status: 'stored', record: updated });
  });

  const remove = vi.fn((id: number): Promise<MemoryWriteResult> => {
    const index = rows.findIndex((row) => row.id === id);
    rows.splice(index, 1);
    return Promise.resolve({ status: 'deleted', id });
  });

  const update = vi.fn((id: number, patch: MemoryPatch): Promise<MemoryWriteResult> => {
    const index = rows.findIndex((row) => row.id === id);
    const updated = {
      ...rows[index]!,
      ...(patch.metadataJson === undefined ? {} : { metadataJson: patch.metadataJson }),
    };
    rows[index] = updated;
    return Promise.resolve({ status: 'stored', record: updated });
  });

  return {
    rows,
    fetchStatus,
    list,
    archive,
    remove,
    update,
    lastFilter: (): MemoryFilter | undefined => list.mock.calls.at(-1)?.[0],
  };
}

/** Debounce beklemesi testleri yavaslatmasin; gecikme davranissal degil. */
const FAST = { searchDebounceMs: 0 } as const;

describe('MemoryView — listeleme', () => {
  it('kaydi turu, tarihi ve kaynak oturumuyla gosterir', async () => {
    const port = createPort([DECISION, IDEA]);
    render(<MemoryView port={port} {...FAST} />);

    expect(await screen.findByText('Wake word yerel kalir')).toBeInTheDocument();
    // Rozet, `Tür` filtresindeki ayni adli secenekle karismasin.
    expect(
      screen.getByText('Karar', { selector: '.asuna-memory-item__badge' }),
    ).toBeInTheDocument();
    expect(screen.getByText('Oturum #7')).toBeInTheDocument();
    expect(
      screen.getByText('Wake word tespiti bulutta degil, cihazda calisir.'),
    ).toBeInTheDocument();

    expect(document.querySelector('time')).toHaveAttribute('datetime', '2026-08-20T09:30:00Z');
  });

  it('kaynak oturumu bilinmeyen kaydi gizlemez, bilinmedigini yazar', async () => {
    const port = createPort([IDEA]);
    render(<MemoryView port={port} {...FAST} />);

    expect(await screen.findByText('Kaynak oturum bilinmiyor')).toBeInTheDocument();
  });

  it('listeyi goruntulemek erisim sayilmaz: markAccessed gonderilmez', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);

    await screen.findByText('Wake word yerel kalir');
    expect(port.lastFilter()).toEqual({ archived: 'active', sort: 'recent', limit: 25 });
  });

  it('model ciktisi duz metin olarak basilir', async () => {
    const port = createPort([record({ content: '<b>enjekte</b>' })]);
    render(<MemoryView port={port} {...FAST} />);

    expect(await screen.findByText('<b>enjekte</b>')).toBeInTheDocument();
    expect(document.querySelector('b')).toBeNull();
  });

  /**
   * ASU-037: onay bekleyen kayitlar listenin **ustunde** ayri bir bolumde
   * cikar; normal listede iki kez gorunmezler (ayrinti:
   * `pending-approvals.spec.tsx`).
   */
  it('onay bekleyen hafizalari ayri bolumde gosterir', async () => {
    const port = createPort([
      DECISION,
      record({
        id: 3,
        kind: 'profile',
        title: 'Hassas bir profil notu',
        metadataJson: '{"pendingApproval":true}',
      }),
    ]);
    render(<MemoryView port={port} {...FAST} />);

    const section = await screen.findByRole('region', { name: 'Onay bekleyen hafızalar' });
    expect(section).toHaveTextContent('Hassas bir profil notu');
    expect(
      screen.getByRole('button', { name: 'Onayla: Hassas bir profil notu' }),
    ).toBeInTheDocument();
  });

  it('onay bekleyen kayit yoksa bolum hic cizilmez', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);

    await screen.findByText('Wake word yerel kalir');
    expect(
      screen.queryByRole('region', { name: 'Onay bekleyen hafızalar' }),
    ).not.toBeInTheDocument();
  });

  it('kayit yokken bos durum yazar', async () => {
    const port = createPort([]);
    render(<MemoryView port={port} {...FAST} />);

    expect(await screen.findByText('Henüz kayıtlı hafıza yok.')).toBeInTheDocument();
  });
});

describe('MemoryView — arama ve filtre', () => {
  it('arama metnini servise `search` olarak gecirir', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.change(screen.getByLabelText('Ara'), { target: { value: 'wake' } });

    await waitFor(() => {
      expect(port.lastFilter()?.search).toBe('wake');
    });
  });

  it('kind filtresini `kinds` dizisi olarak gecirir', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.change(screen.getByLabelText('Tür'), { target: { value: 'idea' } });

    await waitFor(() => {
      expect(port.lastFilter()?.kinds).toEqual(['idea']);
    });
  });

  it('arsiv gorunumu degisince filtre de degisir', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.change(screen.getByLabelText('Arşiv'), { target: { value: 'archived' } });

    await waitFor(() => {
      expect(port.lastFilter()?.archived).toBe('archived');
    });
  });

  it('filtreye kayit uymadiginda bos liste mesaji filtreyi soyler', async () => {
    const port = createPort([]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Henüz kayıtlı hafıza yok.');

    fireEvent.change(screen.getByLabelText('Ara'), { target: { value: 'yok böyle bir şey' } });

    expect(await screen.findByText('Bu filtreye uyan hafıza yok.')).toBeInTheDocument();
  });
});

describe('MemoryView — silme', () => {
  it('once onay ister, onaysiz silmez', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.click(screen.getByRole('button', { name: 'Sil: Wake word yerel kalir' }));

    expect(
      screen.getByText('Bu hafıza kalıcı olarak silinsin mi? Geri alınamaz.'),
    ).toBeInTheDocument();
    expect(port.remove).not.toHaveBeenCalled();
  });

  it('vazgecince kayit yerinde kalir', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.click(screen.getByRole('button', { name: 'Sil: Wake word yerel kalir' }));
    fireEvent.click(screen.getByRole('button', { name: 'Vazgeç' }));

    expect(port.remove).not.toHaveBeenCalled();
    expect(
      screen.getByRole('button', { name: 'Sil: Wake word yerel kalir' }),
    ).toBeInTheDocument();
  });

  it('onaydan sonra siler ve liste tutarli kalir', async () => {
    const port = createPort([DECISION, IDEA]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.click(screen.getByRole('button', { name: 'Sil: Wake word yerel kalir' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    await waitFor(() => {
      expect(screen.queryByText('Wake word yerel kalir')).not.toBeInTheDocument();
    });
    expect(port.remove).toHaveBeenCalledWith(1);
    // Silme sonrasi liste yeniden okundu: ekrandaki sey bellekteki degil, depodaki.
    expect(port.list.mock.calls.length).toBeGreaterThan(1);
    expect(screen.getByText('Overlay her zaman ustte')).toBeInTheDocument();
  });

  it('silme hata verirse durustce soyler, kaydi ekrandan silmez', async () => {
    const port = createPort([DECISION]);
    port.remove.mockRejectedValueOnce(new AsunaStoreError('storage', 'disk dolu'));
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.click(screen.getByRole('button', { name: 'Sil: Wake word yerel kalir' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    expect(await screen.findByText('Depolama hatası: disk dolu')).toBeInTheDocument();
    expect(screen.getByText('Wake word yerel kalir')).toBeInTheDocument();
  });
});

describe('MemoryView — arsivleme', () => {
  it('aktif kaydi arsivler', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.click(screen.getByRole('button', { name: 'Arşivle: Wake word yerel kalir' }));

    await waitFor(() => {
      expect(port.archive).toHaveBeenCalledWith(1, true);
    });
  });

  it('arsivli kaydi arsivden cikarir', async () => {
    const port = createPort([record({ isArchived: true })]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.click(
      screen.getByRole('button', { name: 'Arşivden çıkar: Wake word yerel kalir' }),
    );

    await waitFor(() => {
      expect(port.archive).toHaveBeenCalledWith(1, false);
    });
  });

  it('hafiza kapaliyken yazma atlanmissa "yaptim" demez', async () => {
    const port = createPort([DECISION]);
    port.archive.mockResolvedValueOnce({ status: 'skipped', reason: 'memory-disabled' });
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    fireEvent.click(screen.getByRole('button', { name: 'Arşivle: Wake word yerel kalir' }));

    expect(
      await screen.findByText('Hafıza kapalı olduğu için işlem uygulanmadı.'),
    ).toBeInTheDocument();
  });
});

describe('MemoryView — sayfalama', () => {
  it('tavan dolduysa daha fazlasini yukler', async () => {
    const port = createPort([DECISION, IDEA]);
    render(<MemoryView port={port} pageSize={2} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    expect(port.lastFilter()?.limit).toBe(2);

    fireEvent.click(screen.getByRole('button', { name: 'Daha fazla yükle' }));

    await waitFor(() => {
      expect(port.lastFilter()?.limit).toBe(4);
    });
  });

  it('tavan dolmadiysa "daha fazla" cikmaz', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} pageSize={5} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    expect(screen.queryByRole('button', { name: 'Daha fazla yükle' })).not.toBeInTheDocument();
  });
});

describe('MemoryView — hafiza yokken', () => {
  it('kapali hafizayi ariza gibi gostermez ve sorgu atmaz', async () => {
    const port = createPort([DECISION], {
      availability: 'disabled',
      schemaVersion: null,
      sqliteVersion: '3.46.0',
      reason: null,
    });
    render(<MemoryView port={port} {...FAST} />);

    expect(await screen.findByText(/Hafıza kapalı/)).toBeInTheDocument();
    expect(port.list).not.toHaveBeenCalled();
    expect(screen.queryByLabelText('Ara')).not.toBeInTheDocument();
  });

  it('bozuk hafizayi ariza olarak gosterir ve nedenini yazar', async () => {
    const port = createPort([DECISION], {
      availability: 'unavailable',
      schemaVersion: null,
      sqliteVersion: '3.46.0',
      reason: 'migration 001 basarisiz',
    });
    render(<MemoryView port={port} {...FAST} />);

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Hafıza kullanılamıyor: migration 001 basarisiz');
    expect(port.list).not.toHaveBeenCalled();
  });

  it('durum sorgusu patlarsa sessiz kalmaz', async () => {
    const port = createPort([DECISION]);
    port.fetchStatus.mockRejectedValueOnce(new AsunaStoreError('unavailable', 'db acilamadi'));
    render(<MemoryView port={port} {...FAST} />);

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Hafıza durumu okunamadı: Hafıza kullanılamıyor: db acilamadi',
    );
  });

  it('liste sorgusu patlarsa hata kodunu anlamli cumleye cevirir', async () => {
    const port = createPort([DECISION]);
    // `mockRejectedValue` (Once degil): ASU-037 ile ekran iki sorgu atiyor
    // (onay kuyrugu + ana liste). Olculen sey "liste sorgusu bozuk" —
    // sorgulardan yalnizca birincisinin bozulmasi degil.
    port.list.mockRejectedValue(new AsunaStoreError('unavailable', 'veritabani kilitli'));
    render(<MemoryView port={port} {...FAST} />);

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Hafıza kullanılamıyor: veritabani kilitli',
    );
  });
});

describe('MemoryView — sohbet penceresi degil', () => {
  it('mesaj yazma alani ya da gonder butonu yok', async () => {
    const port = createPort([DECISION]);
    render(<MemoryView port={port} {...FAST} />);
    await screen.findByText('Wake word yerel kalir');

    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /gönder/i })).not.toBeInTheDocument();
  });
});

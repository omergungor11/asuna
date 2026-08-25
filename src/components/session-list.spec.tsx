/**
 * `SessionList` testleri (ASU-065).
 *
 * Kanitlanan seyler:
 * 1. Oturum ozeti **gorunur**: tarih, kapanis durumu, ozet on izlemesi ve
 *    diskte dokum olup olmadigi ekranda.
 * 2. Silme onay ister; onaydan sonra liste depodan **yeniden** okunur ve silinen
 *    oturum ekranda kalmaz.
 * 3. Dokum dosyasina ne oldugu gizlenmez — `refused` / `failed` durumlari
 *    "sildim" diye gecistirilmez.
 * 4. Hafiza kapaliyken "sildim" denmez; hata yutulmaz.
 *
 * Servis katmani sahte port ile degistirilir: gercek `invoke` yok.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, type Mock } from 'vitest';

import type { SessionDeleteResult, SessionListItem, SessionPage } from '../shared/session';
import { AsunaStoreError } from '../shared/store-error';

import { SessionList, type SessionListPort } from './session-list';

function item(overrides: Partial<SessionListItem> = {}): SessionListItem {
  return {
    id: 7,
    startedAt: '2026-08-20T09:30:00Z',
    endedAt: '2026-08-20T09:42:00Z',
    endReason: 'completed',
    summaryPreview: 'Konusulanlar: wake word yerel kalir.',
    summaryTruncated: false,
    hasTranscriptFile: false,
    ...overrides,
  };
}

interface TestPort extends SessionListPort {
  readonly rows: SessionListItem[];
  readonly list: Mock<(limit?: number) => Promise<SessionPage>>;
  readonly remove: Mock<(sessionId: number) => Promise<SessionDeleteResult>>;
}

function createPort(
  initial: readonly SessionListItem[],
  outcome: SessionDeleteResult = { status: 'deleted', id: 7, transcriptFile: 'not-recorded' },
): TestPort {
  const rows = [...initial];

  const list = vi.fn((limit?: number) =>
    Promise.resolve<SessionPage>({
      sessions: rows.slice(0, limit ?? rows.length),
      limit: limit ?? rows.length,
      limitMax: 200,
      total: rows.length,
    }),
  );

  const remove = vi.fn((sessionId: number): Promise<SessionDeleteResult> => {
    if (outcome.status === 'deleted') {
      const index = rows.findIndex((row) => row.id === sessionId);
      if (index >= 0) {
        rows.splice(index, 1);
      }
    }
    return Promise.resolve(outcome);
  });

  return { rows, list, remove };
}

describe('SessionList — listeleme', () => {
  it('oturumu tarihi, durumu ve ozet on izlemesiyle gosterir', async () => {
    render(<SessionList port={createPort([item()])} />);

    expect(await screen.findByText('Oturum #7')).toBeInTheDocument();
    expect(screen.getByText(/temiz kapandı/)).toBeInTheDocument();
    expect(screen.getByText('Konusulanlar: wake word yerel kalir.')).toBeInTheDocument();
    // Sayim sunucudan gelir; UI tahmin yurutmez.
    expect(screen.getByRole('heading', { name: 'Oturumlar (1 / 1)' })).toBeInTheDocument();
  });

  it('ozeti olmayan ve yarim kalan oturumu durustce yazar', async () => {
    render(
      <SessionList
        port={createPort([
          item({ id: 3, summaryPreview: null, endReason: 'abandoned' }),
          item({ id: 4, summaryPreview: null, endReason: null, endedAt: null }),
        ])}
      />,
    );

    expect(await screen.findByText('Oturum #3')).toBeInTheDocument();
    expect(screen.getAllByText('Bu oturum için özet üretilmedi.')).toHaveLength(2);
    expect(screen.getByText(/yarım kaldı/)).toBeInTheDocument();
    // Acik oturum gizlenmez; "0 saniye surdu" gibi bir sayi da uydurulmaz.
    expect(screen.getByText(/sürüyor/)).toBeInTheDocument();
  });

  it('diskte dokum dosyasi olan oturumu isaretler', async () => {
    render(<SessionList port={createPort([item({ hasTranscriptFile: true })])} />);

    expect(await screen.findByText('Diskte konuşma dökümü dosyası var.')).toBeInTheDocument();
  });

  it('kirpilmis ozeti kirpilmis olarak gosterir', async () => {
    render(<SessionList port={createPort([item({ summaryTruncated: true })])} />);

    expect(await screen.findByText(/özet kısaltıldı/)).toBeInTheDocument();
  });

  it('gecmis bossa bunu yazar', async () => {
    render(<SessionList port={createPort([])} />);

    expect(await screen.findByText('Kayıtlı oturum yok.')).toBeInTheDocument();
  });

  it('liste okunamazsa sessiz kalmaz', async () => {
    const port = createPort([item()]);
    port.list.mockRejectedValueOnce(
      new AsunaStoreError('unavailable', 'sema migrationlari uygulanamadi'),
    );
    render(<SessionList port={port} />);

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Hafıza kullanılamıyor: sema migrationlari uygulanamadi',
    );
    expect(screen.queryByText('Oturum #7')).not.toBeInTheDocument();
  });

  /** Sunucu tavanina carpildiginda "hepsi bu kadar" denmez. */
  it('sunucu tavanina carpildigini soyler', async () => {
    const port = createPort([item()]);
    port.list.mockResolvedValue({
      sessions: [item()],
      limit: 200,
      limitMax: 200,
      total: 640,
    });
    render(<SessionList port={port} />);

    expect(await screen.findByText(/En yeni 200 oturum gösteriliyor/)).toHaveTextContent(
      'toplam 640 oturum var',
    );
    expect(
      screen.queryByRole('button', { name: 'Daha fazla oturum yükle' }),
    ).not.toBeInTheDocument();
  });

  it('daha fazlasi varken sayfayi buyutur', async () => {
    const rows = Array.from({ length: 5 }, (_, index) => item({ id: index + 1 }));
    const port = createPort(rows);
    render(<SessionList port={port} pageSize={2} />);

    await screen.findByText('Oturum #1');
    expect(port.list).toHaveBeenLastCalledWith(2);

    fireEvent.click(screen.getByRole('button', { name: 'Daha fazla oturum yükle' }));

    await waitFor(() => {
      expect(port.list).toHaveBeenLastCalledWith(4);
    });
  });
});

describe('SessionList — silme', () => {
  it('tek tikla silmez: satir ici onay ister', async () => {
    const port = createPort([item()]);
    render(<SessionList port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Sil: Oturum #7' }));
    expect(port.remove).not.toHaveBeenCalled();
    expect(
      screen.getByText(/Bu oturumun kaydı, özeti ve varsa döküm dosyası/),
    ).toHaveTextContent('Geri alınamaz');

    // Vazgecmek gercekten vazgecer.
    fireEvent.click(screen.getByRole('button', { name: 'Vazgeç' }));
    expect(port.remove).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Sil: Oturum #7' })).toBeInTheDocument();
  });

  /**
   * **ASU-065 kabul kriteri**: onaydan sonra oturum gider ve liste depodan
   * yeniden okunur — silinen satir ekranda kalmaz.
   */
  it('onaydan sonra siler ve liste tutarli kalir', async () => {
    const port = createPort([item(), item({ id: 8, summaryPreview: 'Ikinci oturum.' })]);
    const onChanged = vi.fn();
    render(<SessionList port={port} onChanged={onChanged} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Sil: Oturum #7' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    expect(await screen.findByRole('status')).toHaveTextContent(
      'Oturum kaydı ve özeti silindi (diskte döküm dosyası yoktu).',
    );
    await waitFor(() => {
      expect(screen.queryByText('Oturum #7')).not.toBeInTheDocument();
    });
    expect(screen.getByText('Oturum #8')).toBeInTheDocument();
    expect(port.remove).toHaveBeenCalledExactlyOnceWith(7);
    // Hafiza listesi de tazelenmeli: kaynak oturum artik yok.
    expect(onChanged).toHaveBeenCalledOnce();
  });

  it('dokum dosyasi silindiginde bunu ayrica soyler', async () => {
    const port = createPort([item({ hasTranscriptFile: true })], {
      status: 'deleted',
      id: 7,
      transcriptFile: 'deleted',
    });
    render(<SessionList port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Sil: Oturum #7' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    expect(await screen.findByRole('status')).toHaveTextContent(
      'Oturum kaydı, özeti ve konuşma dökümü dosyası silindi.',
    );
  });

  /**
   * Dosyaya dokunulmadiysa bu **gizlenmez**: kayitli yol Asuna'nin klasorunun
   * disina cikiyorsa kullanici bilmeli (traversal guard'in gorunur yuzu).
   */
  it('dosyaya dokunulmadigini gizlemez', async () => {
    const port = createPort([item({ hasTranscriptFile: true })], {
      status: 'deleted',
      id: 7,
      transcriptFile: 'refused',
    });
    render(<SessionList port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Sil: Oturum #7' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    const notice = await screen.findByRole('status');
    expect(notice).toHaveTextContent('DOKUNULMADI');
    expect(notice).toHaveTextContent('döküm klasörünün dışına çıkıyor');
  });

  it('dosya silinemediginde basari taklidi yapmaz', async () => {
    const port = createPort([item({ hasTranscriptFile: true })], {
      status: 'deleted',
      id: 7,
      transcriptFile: 'failed',
    });
    render(<SessionList port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Sil: Oturum #7' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    expect(await screen.findByRole('status')).toHaveTextContent('döküm dosyası silinemedi');
  });

  it('hafiza kapaliyken "sildim" demez', async () => {
    const port = createPort([item()], { status: 'skipped', reason: 'memory-disabled' });
    render(<SessionList port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Sil: Oturum #7' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    expect(await screen.findByRole('status')).toHaveTextContent(
      'Hafıza kapalı olduğu için oturum kaydı silinmedi.',
    );
    // Satir ekranda kalir: silinmedi.
    expect(screen.getByText('Oturum #7')).toBeInTheDocument();
  });

  it('silme hatasini yutmaz', async () => {
    const port = createPort([item()]);
    port.remove.mockRejectedValueOnce(new AsunaStoreError('not-found', 'kayit bulunamadi'));
    render(<SessionList port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Sil: Oturum #7' }));
    fireEvent.click(screen.getByRole('button', { name: 'Evet, sil' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('Kayıt bulunamadı');
  });
});

describe('SessionList — metin', () => {
  /**
   * Ozet modelden gelir: HTML olarak yorumlanmaz.
   */
  it('ozeti duz metin olarak basar', async () => {
    render(
      <SessionList port={createPort([item({ summaryPreview: '<b>kalin</b> olmamali' })])} />,
    );

    expect(await screen.findByText('<b>kalin</b> olmamali')).toBeInTheDocument();
    expect(document.querySelector('.asuna-session-item__summary b')).toBeNull();
  });

  /** Ozetin silinmesinin **neden** onemli oldugu ekranda yazili. */
  it('ozetin bir sonraki konusmaya verildigini aciklar', async () => {
    render(<SessionList port={createPort([])} />);

    const hint = await screen.findByText(/Her oturumun özeti/);
    expect(hint).toHaveTextContent('bir daha hatırlanmaz');
    expect(hint).toHaveTextContent('hafıza kayıtlarını silmez');
  });
});

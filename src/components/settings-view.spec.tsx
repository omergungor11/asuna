/**
 * `SettingsView` testleri (ASU-037).
 *
 * Kanitlanan seyler:
 * 1. Iki anahtar da gorunur ve gercek duruma gore cizilir.
 * 2. Anahtar degistirmek servise **kismi** bir patch gonderir ve ekran
 *    sunucunun kabul ettigi durumu gosterir (kendi tahminini degil).
 * 3. Kapatmanin geriye donuk veriyi silmedigi kullaniciya **yazili** olarak
 *    soyleniyor — ASU-037 kabul kriteri.
 * 4. Acilista kapatilmis anahtar kilitli ve nedeni yazili.
 * 5. "Tum hafizayi sil" cift onay istiyor: once niyet, sonra birebir ifade.
 *
 * Servis katmani sahte port ile degistirilir: gercek `invoke` yok.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, type Mock } from 'vitest';

import { MEMORY_DELETE_ALL_CONFIRMATION, type MemoryPurgeResult } from '../shared/memory';
import { AsunaPrivacyError, type PrivacyPatch, type PrivacySettings } from '../shared/privacy';

import { SettingsView, type SettingsViewPort } from './settings-view';

interface TestPort extends SettingsViewPort {
  readonly fetchPrivacy: Mock<() => Promise<PrivacySettings>>;
  readonly updatePrivacy: Mock<(patch: PrivacyPatch) => Promise<PrivacySettings>>;
  readonly purgeMemories: Mock<(phrase: string) => Promise<MemoryPurgeResult>>;
}

function createPort(initial: Partial<PrivacySettings> = {}): TestPort {
  let settings: PrivacySettings = {
    memoryEnabled: true,
    transcriptStorage: true,
    memoryEnabledAtBoot: true,
    transcriptStorageAtBoot: true,
    ...initial,
  };

  const fetchPrivacy = vi.fn(() => Promise.resolve(settings));

  const updatePrivacy = vi.fn((patch: PrivacyPatch) => {
    settings = { ...settings, ...patch };
    return Promise.resolve(settings);
  });

  const purgeMemories = vi.fn((): Promise<MemoryPurgeResult> =>
    Promise.resolve({ status: 'purged', deleted: 3 }),
  );

  return { fetchPrivacy, updatePrivacy, purgeMemories };
}

const memorySwitch = (): HTMLElement => screen.getByRole('switch', { name: 'Kalıcı hafıza' });
const transcriptSwitch = (): HTMLElement =>
  screen.getByRole('switch', { name: 'Konuşma dökümü saklama' });

describe('SettingsView — gizlilik anahtarlari', () => {
  it('iki anahtari da guncel durumla gosterir', async () => {
    render(<SettingsView port={createPort({ transcriptStorage: false })} />);

    expect(await screen.findByRole('switch', { name: 'Kalıcı hafıza' })).toBeChecked();
    expect(transcriptSwitch()).not.toBeChecked();
  });

  /** ASU-037: degisiklik yeniden baslatmadan etkili — ekran da hemen doner. */
  it('anahtari kapatinca kismi patch gonderir ve sunucunun durumunu gosterir', async () => {
    const port = createPort();
    render(<SettingsView port={port} />);
    await screen.findByRole('switch', { name: 'Kalıcı hafıza' });

    fireEvent.click(memorySwitch());

    await waitFor(() => {
      expect(memorySwitch()).not.toBeChecked();
    });
    // Yalnizca dokunulan alan gonderilir; digeri "dokunma" anlaminda yok.
    expect(port.updatePrivacy).toHaveBeenCalledExactlyOnceWith({ memoryEnabled: false });
    // Diger anahtar etkilenmedi.
    expect(transcriptSwitch()).toBeChecked();
  });

  /**
   * **ASU-037 kabul kriteri**: kapatildiginda geriye donuk veriye ne oldugu
   * net anlatiliyor — silinmiyor, sadece yazilmiyor.
   */
  it('kapaliyken gecmis verinin silinmedigini yazar', async () => {
    render(<SettingsView port={createPort({ memoryEnabled: false })} />);

    const explanation = await screen.findByText(/Yeni hiçbir hafıza yazılmaz/);
    expect(explanation).toHaveTextContent('SİLİNMEZ');
    expect(explanation).toHaveTextContent('tek tek silinebilir');
  });

  it('transcript kapaliyken var olan dosyalarin durdugunu yazar', async () => {
    render(<SettingsView port={createPort({ transcriptStorage: false })} />);

    expect(await screen.findByText(/Döküm diske yazılmaz/)).toHaveTextContent('SİLİNMEZ');
  });

  /** Anahtarlarin kaynagi acilistaki `.env`; bu ekran dosyaya yazmaz. */
  it('acilis kaynaginin .env oldugunu ve dosyaya yazilmadigini soyler', async () => {
    render(<SettingsView port={createPort()} />);

    const note = await screen.findByText(/açılışta/);
    expect(note).toHaveTextContent('.env');
    expect(note).toHaveTextContent('yazılmaz');
  });

  /**
   * Acilista kapatilmis anahtar buradan **acilamaz**: DB dosyasi hic acilmadi.
   * Kullanici tiklayip hata gormek yerine nedenini onceden okur.
   */
  it('acilista kapatilmis anahtari kilitler ve nedenini yazar', async () => {
    const port = createPort({ memoryEnabled: false, memoryEnabledAtBoot: false });
    render(<SettingsView port={port} />);

    await waitFor(() => {
      expect(memorySwitch()).toBeDisabled();
    });
    expect(screen.getByText(/yeniden başlatın/)).toHaveTextContent('ASUNA_MEMORY_ENABLED');
    // Kilitli anahtar hicbir istek uretmez.
    fireEvent.click(memorySwitch());
    expect(port.updatePrivacy).not.toHaveBeenCalled();
  });

  it('servis reddini yutmaz', async () => {
    const port = createPort();
    port.updatePrivacy.mockRejectedValueOnce(
      new AsunaPrivacyError('locked-by-env', '`ASUNA_MEMORY_ENABLED` acilista kapatilmis'),
    );
    render(<SettingsView port={port} />);
    await screen.findByRole('switch', { name: 'Kalıcı hafıza' });

    fireEvent.click(memorySwitch());

    expect(await screen.findByRole('alert')).toHaveTextContent('ASUNA_MEMORY_ENABLED');
    // Reddedilen istek ekranda "kapandi" gibi gorunmez.
    expect(memorySwitch()).toBeChecked();
  });

  it('ayarlar okunamazsa sessiz kalmaz', async () => {
    const port = createPort();
    port.fetchPrivacy.mockRejectedValueOnce(new Error('ipc down'));
    render(<SettingsView port={port} />);

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Gizlilik ayarları okunamadı: ipc down',
    );
  });
});

describe('SettingsView — tum hafizayi sil', () => {
  it('tek tikla silmez: once niyet, sonra birebir ifade ister', async () => {
    const port = createPort();
    render(<SettingsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Tüm hafızayı sil' }));
    expect(port.purgeMemories).not.toHaveBeenCalled();

    const confirmButton = screen.getByRole('button', { name: 'Kalıcı olarak sil' });
    expect(confirmButton).toBeDisabled();

    const input = screen.getByRole('textbox');
    fireEvent.change(input, { target: { value: 'tum hafizayi sil' } });
    expect(confirmButton).toBeDisabled();

    fireEvent.change(input, { target: { value: MEMORY_DELETE_ALL_CONFIRMATION } });
    expect(confirmButton).toBeEnabled();

    fireEvent.click(confirmButton);

    expect(await screen.findByRole('status')).toHaveTextContent(
      '3 hafıza kalıcı olarak silindi.',
    );
    expect(port.purgeMemories).toHaveBeenCalledExactlyOnceWith(MEMORY_DELETE_ALL_CONFIRMATION);
  });

  it('vazgecince hicbir sey silinmez ve yazilan ifade temizlenir', async () => {
    const port = createPort();
    render(<SettingsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Tüm hafızayı sil' }));
    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: MEMORY_DELETE_ALL_CONFIRMATION },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Vazgeç' }));

    expect(port.purgeMemories).not.toHaveBeenCalled();
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();

    // Yeniden acilinca kutu bos: onceki yazi "hazir onay" olarak kalmaz.
    fireEvent.click(screen.getByRole('button', { name: 'Tüm hafızayı sil' }));
    expect(screen.getByRole('textbox')).toHaveValue('');
  });

  it('neyin silinmedigini de yazar', async () => {
    render(<SettingsView port={createPort()} />);

    expect(await screen.findByText(/Geri alınamaz/)).toHaveTextContent(
      'Oturum kayıtları/özetleri ve diskteki konuşma dökümü dosyaları bu işlemle silinmez',
    );
  });

  it('hafiza kapaliyken "sildim" demez', async () => {
    const port = createPort();
    port.purgeMemories.mockResolvedValueOnce({
      status: 'skipped',
      reason: 'memory-disabled',
    });
    render(<SettingsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Tüm hafızayı sil' }));
    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: MEMORY_DELETE_ALL_CONFIRMATION },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Kalıcı olarak sil' }));

    expect(await screen.findByRole('status')).toHaveTextContent(
      'Hafıza kapalı olduğu için silinecek bir kayıt yok.',
    );
  });

  it('silme hata verirse onay ekraninda kalir ve nedeni gosterir', async () => {
    const port = createPort();
    port.purgeMemories.mockRejectedValueOnce(new Error('disk dolu'));
    render(<SettingsView port={port} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Tüm hafızayı sil' }));
    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: MEMORY_DELETE_ALL_CONFIRMATION },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Kalıcı olarak sil' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('disk dolu');
    expect(screen.getByRole('button', { name: 'Kalıcı olarak sil' })).toBeInTheDocument();
  });
});

describe('SettingsView — guvenlik', () => {
  /** Bu ekran secret gostermez: API anahtari alani ya da metni yok. */
  it('API anahtari alani icermez', async () => {
    render(<SettingsView port={createPort()} />);
    await screen.findByRole('switch', { name: 'Kalıcı hafıza' });

    expect(screen.queryByLabelText(/API/i)).not.toBeInTheDocument();
    expect(document.querySelector('input[type="password"]')).toBeNull();
    expect(screen.queryByText(/sk-/)).not.toBeInTheDocument();
  });
});

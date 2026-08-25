/**
 * Ayarlar sekmesi — gizlilik kontrolleri (ASU-037).
 *
 * # Neden var
 *
 * PROJECT.md Bolum 20 / Bolum 21: kullanici neyin saklandigini bilmeli ve
 * bunu **kapatabilmeli**. Bu ekran bir tercih paneli degil, bir gizlilik
 * kontrol yuzeyi: her anahtarin yanina "kapatinca ne oluyor" cumlesi yazilir,
 * cunku bir anahtarin ne yaptigini bilmeden kapatmak guven uretmez.
 *
 * # Uc kural
 *
 * 1. **Kapatmak silmez.** Anahtar yalnizca *bundan sonrasini* durdurur;
 *    gecmis veri yerinde kalir ve gorulebilir/silinebilir olmaya devam eder.
 *    Bu ekranda acikca yazili — kullanici "kapattim, gitti" sanmasin.
 * 2. **Acilis kaynagi `.env`.** Buradaki degisiklik yalnizca calisan uygulama
 *    icin gecerlidir; dosyaya yazilmaz. Acilista kapatilmis bir anahtar
 *    buradan **acilamaz** (bkz. `src-tauri/src/privacy.rs`) — bu durumda
 *    anahtar kilitli cizilir ve nedeni yazilir.
 * 3. **Silme cift onayli.** Once niyet, sonra birebir yazilan onay ifadesi.
 *
 * # Sinirlar
 *
 * - Bilesen `invoke` cagirmaz: her sey [`SettingsViewPort`] uzerinden servis
 *   katmanina gider (ADR-005).
 * - Secret gostermez ve tutmaz; bu ekranda API anahtari alani **yoktur**.
 * - Model kimligi hard-code edilmez — bu ekran model secmez.
 */

import { useCallback, useEffect, useState } from 'react';

import { deleteAllMemories } from '../asuna/memory/memory-service';
import { fetchPrivacySettings, updatePrivacySettings } from '../asuna/memory/privacy-service';
import { MEMORY_DELETE_ALL_CONFIRMATION, type MemoryPurgeResult } from '../shared/memory';
import {
  canEnableAtRuntime,
  type PrivacyPatch,
  type PrivacySettings,
  type PrivacyToggleKey,
} from '../shared/privacy';

import { describeMemoryError } from './memory-text';

/** Bilesenin servis yuzeyi; testler gercek IPC'ye dokunmadan sahte port verir. */
export interface SettingsViewPort {
  readonly fetchPrivacy: () => Promise<PrivacySettings>;
  readonly updatePrivacy: (patch: PrivacyPatch) => Promise<PrivacySettings>;
  readonly purgeMemories: (confirmationPhrase: string) => Promise<MemoryPurgeResult>;
}

const DEFAULT_SETTINGS_PORT: SettingsViewPort = {
  fetchPrivacy: fetchPrivacySettings,
  updatePrivacy: updatePrivacySettings,
  purgeMemories: deleteAllMemories,
};

interface ToggleCopy {
  readonly key: PrivacyToggleKey;
  readonly label: string;
  readonly envKey: string;
  /** Anahtar acikken ne oluyor. */
  readonly whenOn: string;
  /** Kapatilinca ne oluyor — ve **ne olmuyor**. */
  readonly whenOff: string;
}

const TOGGLES: readonly ToggleCopy[] = [
  {
    key: 'memoryEnabled',
    label: 'Kalıcı hafıza',
    envKey: 'ASUNA_MEMORY_ENABLED',
    whenOn: 'Asuna konuşmalardan çıkardığı kalıcı hafızaları kaydedebilir.',
    whenOff:
      'Yeni hiçbir hafıza yazılmaz. Daha önce kaydedilenler SİLİNMEZ: Hafıza sekmesinde ' +
      'görünmeye ve tek tek silinebilir olmaya devam eder.',
  },
  {
    key: 'transcriptStorage',
    label: 'Konuşma dökümü saklama',
    envKey: 'ASUNA_TRANSCRIPT_STORAGE',
    whenOn: 'Oturum kapanınca konuşma dökümü diske yazılır (oturum başına bir dosya).',
    whenOff:
      'Döküm diske yazılmaz. Daha önce yazılmış döküm dosyaları SİLİNMEZ; uygulama veri ' +
      'klasöründe durur.',
  },
];

type LoadState =
  | { readonly phase: 'loading' }
  | { readonly phase: 'ready'; readonly settings: PrivacySettings }
  | { readonly phase: 'error'; readonly message: string };

/** Toplu silmenin iki asamasi: once niyet, sonra birebir yazilan ifade. */
type PurgePhase = 'idle' | 'confirming' | 'working';

interface Notice {
  readonly tone: 'info' | 'error';
  readonly text: string;
}

export interface SettingsViewProps {
  readonly port?: SettingsViewPort;
}

/**
 * Gizlilik komutlarinin hatasi. `AsunaPrivacyError` dahil her `Error`'un mesaji
 * **korunur**; "bir seyler ters gitti" turu bos cumle yok.
 */
function describeError(error: unknown): string {
  return error instanceof Error && error.message.length > 0
    ? error.message
    : 'Beklenmeyen bir hata oluştu.';
}

export function SettingsView({
  port = DEFAULT_SETTINGS_PORT,
}: SettingsViewProps): React.JSX.Element {
  const [state, setState] = useState<LoadState>({ phase: 'loading' });
  const [busyToggle, setBusyToggle] = useState<PrivacyToggleKey | null>(null);
  const [toggleNotice, setToggleNotice] = useState<Notice | null>(null);

  const [purgePhase, setPurgePhase] = useState<PurgePhase>('idle');
  const [purgeDraft, setPurgeDraft] = useState('');
  const [purgeNotice, setPurgeNotice] = useState<Notice | null>(null);

  // Onbelleklenmez: gizlilik iddiasinin dogrulugu tazeligine bagli.
  useEffect(() => {
    let cancelled = false;
    port.fetchPrivacy().then(
      (settings) => {
        if (!cancelled) {
          setState({ phase: 'ready', settings });
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          setState({ phase: 'error', message: describeError(error) });
        }
      },
    );
    return (): void => {
      cancelled = true;
    };
  }, [port]);

  const handleToggle = useCallback(
    (key: PrivacyToggleKey, next: boolean): void => {
      setBusyToggle(key);
      setToggleNotice(null);

      // `exactOptionalPropertyTypes`: dokunulmayan alan hic gonderilmez.
      const patch: PrivacyPatch =
        key === 'memoryEnabled' ? { memoryEnabled: next } : { transcriptStorage: next };

      port.updatePrivacy(patch).then(
        (settings) => {
          setBusyToggle(null);
          // Ekranda gosterilen sey sunucunun kabul ettigi durum; istegin
          // kopyasi degil. Aksi halde reddedilen bir istek "acik" gorunurdu.
          setState({ phase: 'ready', settings });
        },
        (error: unknown) => {
          setBusyToggle(null);
          setToggleNotice({ tone: 'error', text: describeError(error) });
        },
      );
    },
    [port],
  );

  const handlePurge = useCallback((): void => {
    setPurgePhase('working');
    setPurgeNotice(null);

    port.purgeMemories(MEMORY_DELETE_ALL_CONFIRMATION).then(
      (result) => {
        setPurgePhase('idle');
        setPurgeDraft('');
        if (result.status === 'skipped') {
          setPurgeNotice({
            tone: 'info',
            text: 'Hafıza kapalı olduğu için silinecek bir kayıt yok.',
          });
          return;
        }
        setPurgeNotice({
          tone: 'info',
          text:
            result.deleted === 0
              ? 'Silinecek hafıza yoktu.'
              : `${result.deleted.toString()} hafıza kalıcı olarak silindi.`,
        });
      },
      (error: unknown) => {
        // Onay ekraninda kalinir: kullanici yeniden deneyebilsin ve neyin
        // olmadigini gorsun. Metin hafiza katmaninin sozluguyle yazilir.
        setPurgePhase('confirming');
        setPurgeNotice({ tone: 'error', text: describeMemoryError(error) });
      },
    );
  }, [port]);

  if (state.phase === 'loading') {
    return (
      <section className="asuna-settings" aria-label="Ayarlar">
        <p className="asuna-settings__notice">Gizlilik ayarları okunuyor…</p>
      </section>
    );
  }

  if (state.phase === 'error') {
    return (
      <section className="asuna-settings" aria-label="Ayarlar">
        <p className="asuna-settings__notice" role="alert">
          Gizlilik ayarları okunamadı: {state.message}
        </p>
      </section>
    );
  }

  const settings = state.settings;
  const phraseMatches = purgeDraft === MEMORY_DELETE_ALL_CONFIRMATION;

  return (
    <section className="asuna-settings" aria-label="Ayarlar">
      <h2 className="asuna-settings__heading">Gizlilik</h2>

      <p className="asuna-settings__note">
        Bu anahtarlar açılışta <code>.env</code> dosyasından okunur. Buradaki değişiklik{' '}
        <strong>hemen</strong> geçerli olur (yeniden başlatma gerekmez) ama <code>.env</code>{' '}
        dosyasına yazılmaz: bir sonraki açılışta yine dosyadaki değer geçerli olur.
      </p>

      <ul className="asuna-settings__list">
        {TOGGLES.map((toggle) => {
          const checked = settings[toggle.key];
          const canEnable = canEnableAtRuntime(settings, toggle.key);
          // Kapatmak her zaman serbest; acmak yalnizca acilista aciksa.
          const locked = !checked && !canEnable;

          return (
            <li key={toggle.key} className="asuna-settings__item">
              <label className="asuna-settings__switch">
                <input
                  type="checkbox"
                  role="switch"
                  checked={checked}
                  disabled={locked || busyToggle !== null}
                  onChange={(event): void => {
                    // Ikinci kat: kilitli anahtar reddedilecegi bilinen bir
                    // istek uretmez (birinci kat `disabled`, ucuncusu Rust).
                    if (locked) {
                      return;
                    }
                    handleToggle(toggle.key, event.target.checked);
                  }}
                />
                <span>{toggle.label}</span>
              </label>

              <p className="asuna-settings__explain">
                {checked ? toggle.whenOn : toggle.whenOff}
              </p>

              {locked && (
                <p className="asuna-settings__explain">
                  <code>{toggle.envKey}</code> açılışta <code>false</code> olduğu için buradan
                  açılamaz: hafıza dosyası hiç açılmadı. Açmak için <code>.env</code> dosyasını
                  düzenleyip Asuna&apos;yı yeniden başlatın.
                </p>
              )}
            </li>
          );
        })}
      </ul>

      {toggleNotice !== null && (
        <p className="asuna-settings__notice" role="alert">
          {toggleNotice.text}
        </p>
      )}

      <h2 className="asuna-settings__heading">Tüm hafızayı sil</h2>

      <p className="asuna-settings__explain">
        Kayıtlı <strong>bütün</strong> hafızalar kalıcı olarak silinir. Geri alınamaz. Oturum
        kayıtları/özetleri ve diskteki konuşma dökümü dosyaları bu işlemle silinmez.
      </p>

      {purgePhase === 'idle' ? (
        <button
          type="button"
          className="asuna-settings__danger"
          onClick={(): void => {
            setPurgeNotice(null);
            setPurgeDraft('');
            setPurgePhase('confirming');
          }}
        >
          Tüm hafızayı sil
        </button>
      ) : (
        <div
          className="asuna-settings__confirm"
          role="group"
          aria-label="Tüm hafızayı silme onayı"
        >
          <label className="asuna-settings__field">
            <span>
              Onaylamak için <code>{MEMORY_DELETE_ALL_CONFIRMATION}</code> yazın:
            </span>
            <input
              type="text"
              value={purgeDraft}
              disabled={purgePhase === 'working'}
              autoComplete="off"
              onChange={(event): void => {
                setPurgeDraft(event.target.value);
              }}
            />
          </label>

          <button
            type="button"
            className="asuna-settings__danger"
            disabled={!phraseMatches || purgePhase === 'working'}
            onClick={handlePurge}
          >
            Kalıcı olarak sil
          </button>
          <button
            type="button"
            disabled={purgePhase === 'working'}
            onClick={(): void => {
              setPurgePhase('idle');
              setPurgeDraft('');
            }}
          >
            Vazgeç
          </button>
        </div>
      )}

      {purgeNotice !== null && (
        <p
          className="asuna-settings__notice"
          role={purgeNotice.tone === 'error' ? 'alert' : 'status'}
        >
          {purgeNotice.text}
        </p>
      )}
    </section>
  );
}

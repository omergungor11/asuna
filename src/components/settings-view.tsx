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
import { clearSessionHistory } from '../asuna/memory/session-service';
import { MEMORY_DELETE_ALL_CONFIRMATION, type MemoryPurgeResult } from '../shared/memory';
import {
  canEnableAtRuntime,
  type PrivacyPatch,
  type PrivacySettings,
  type PrivacyToggleKey,
} from '../shared/privacy';
import { SESSION_CLEAR_ALL_CONFIRMATION, type SessionPurgeResult } from '../shared/session';

import { describeMemoryError } from './memory-text';

/** Bilesenin servis yuzeyi; testler gercek IPC'ye dokunmadan sahte port verir. */
export interface SettingsViewPort {
  readonly fetchPrivacy: () => Promise<PrivacySettings>;
  readonly updatePrivacy: (patch: PrivacyPatch) => Promise<PrivacySettings>;
  readonly purgeMemories: (confirmationPhrase: string) => Promise<MemoryPurgeResult>;
  /**
   * Oturum kayitlari + dokum dosyalari (ASU-065).
   *
   * `purgeMemories` ile **ayri** bir komut ve ayri bir onay ifadesi: kapsamlari
   * farkli, dolayisiyla tek bir "hepsini sil" dugmesi arkasina saklanamazlar.
   */
  readonly clearSessions: (confirmationPhrase: string) => Promise<SessionPurgeResult>;
}

const DEFAULT_SETTINGS_PORT: SettingsViewPort = {
  fetchPrivacy: fetchPrivacySettings,
  updatePrivacy: updatePrivacySettings,
  purgeMemories: deleteAllMemories,
  clearSessions: clearSessionHistory,
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
      'klasöründe durur. Silmek için aşağıdaki “Konuşma geçmişini sil” bölümünü kullanın.',
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

/**
 * Geri alinamaz bir silme aksiyonunun **cift onayli** kabugu (ASU-037/ASU-065).
 *
 * Ikinci kapi (birebir ifade) komut imzasinin parcasi; bu bilesen yalnizca
 * birinci kapiyi ve metni tasir. Iki aksiyon ayni kabugu paylasir ama **ayni
 * dugme degildir**: farkli baslik, farkli ifade, farkli komut. Tek bir "hepsini
 * sil" dugmesi kapsamlari gizlerdi.
 */
function DangerAction({
  heading,
  description,
  openLabel,
  confirmLabel,
  phrase,
  run,
}: {
  readonly heading: string;
  readonly description: React.ReactNode;
  readonly openLabel: string;
  readonly confirmLabel: string;
  readonly phrase: string;
  /** Basari halinde kullaniciya gosterilecek cumleyi dondurur. */
  readonly run: (phrase: string) => Promise<string>;
}): React.JSX.Element {
  const [phase, setPhase] = useState<PurgePhase>('idle');
  const [draft, setDraft] = useState('');
  const [notice, setNotice] = useState<Notice | null>(null);

  const handleConfirm = useCallback((): void => {
    setPhase('working');
    setNotice(null);

    run(phrase).then(
      (text) => {
        setPhase('idle');
        setDraft('');
        setNotice({ tone: 'info', text });
      },
      (error: unknown) => {
        // Onay ekraninda kalinir: kullanici yeniden deneyebilsin ve neyin
        // olmadigini gorsun. Metin hafiza katmaninin sozluguyle yazilir.
        setPhase('confirming');
        setNotice({ tone: 'error', text: describeMemoryError(error) });
      },
    );
  }, [phrase, run]);

  return (
    <>
      <h2 className="asuna-settings__heading">{heading}</h2>
      <p className="asuna-settings__explain">{description}</p>

      {phase === 'idle' ? (
        <button
          type="button"
          className="asuna-settings__danger"
          onClick={(): void => {
            setNotice(null);
            setDraft('');
            setPhase('confirming');
          }}
        >
          {openLabel}
        </button>
      ) : (
        <div className="asuna-settings__confirm" role="group" aria-label={`${heading} onayı`}>
          <label className="asuna-settings__field">
            <span>
              Onaylamak için <code>{phrase}</code> yazın:
            </span>
            <input
              type="text"
              value={draft}
              disabled={phase === 'working'}
              autoComplete="off"
              onChange={(event): void => {
                setDraft(event.target.value);
              }}
            />
          </label>

          <button
            type="button"
            className="asuna-settings__danger"
            disabled={draft !== phrase || phase === 'working'}
            onClick={handleConfirm}
          >
            {confirmLabel}
          </button>
          <button
            type="button"
            disabled={phase === 'working'}
            onClick={(): void => {
              setPhase('idle');
              setDraft('');
            }}
          >
            Vazgeç
          </button>
        </div>
      )}

      {notice !== null && (
        <p
          className="asuna-settings__notice"
          role={notice.tone === 'error' ? 'alert' : 'status'}
        >
          {notice.text}
        </p>
      )}
    </>
  );
}

export function SettingsView({
  port = DEFAULT_SETTINGS_PORT,
}: SettingsViewProps): React.JSX.Element {
  const [state, setState] = useState<LoadState>({ phase: 'loading' });
  const [busyToggle, setBusyToggle] = useState<PrivacyToggleKey | null>(null);
  const [toggleNotice, setToggleNotice] = useState<Notice | null>(null);

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

  const runPurgeMemories = useCallback(
    async (phrase: string): Promise<string> => {
      const result = await port.purgeMemories(phrase);
      if (result.status === 'skipped') {
        return 'Hafıza kapalı olduğu için silinecek bir kayıt yok.';
      }
      return result.deleted === 0
        ? 'Silinecek hafıza yoktu.'
        : `${result.deleted.toString()} hafıza kalıcı olarak silindi.`;
    },
    [port],
  );

  /**
   * Sonuc cumlesi **olculen** sayilardan kurulur: kac oturum, kac dosya.
   * "Temizlendi" demek yetmez — kullanici neyin gittigini gormeli. Dokum
   * dizininde birakilan girdi varsa (Asuna'nin uretmedigi dosyalar ya da
   * silinemeyenler) bu da gizlenmez.
   */
  const runClearSessions = useCallback(
    async (phrase: string): Promise<string> => {
      const result = await port.clearSessions(phrase);
      if (result.status === 'skipped') {
        return 'Hafıza kapalı olduğu için silinecek bir oturum kaydı yok.';
      }
      if (result.deletedSessions === 0 && result.deletedFiles === 0) {
        return 'Silinecek oturum kaydı ya da döküm dosyası yoktu.';
      }

      const parts = [
        `${result.deletedSessions.toString()} oturum kaydı (özetleriyle birlikte)`,
        `${result.deletedFiles.toString()} döküm dosyası`,
      ];
      const left =
        result.remainingFiles === 0
          ? ''
          : ` Döküm klasöründe ${result.remainingFiles.toString()} dosya bırakıldı: ` +
            'Asuna’nın yazmadığı dosyalar silinmez.';
      return `${parts.join(' ve ')} kalıcı olarak silindi.${left}`;
    },
    [port],
  );

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

      {/* Iki silme aksiyonu, iki ayri kapsam. Her ikisinin metni digerinin
          **kapsam disi** oldugunu acikca yazar: "hepsini sildim" deyip bir seyi
          birakmak en kotu sonuc (Gate 3 / MEDIUM-6). */}
      <DangerAction
        heading="Tüm hafızayı sil"
        description={
          <>
            Kayıtlı <strong>bütün</strong> hafızalar kalıcı olarak silinir. Geri alınamaz.
            Oturum kayıtları/özetleri ve diskteki konuşma dökümü dosyaları bu işlemle silinmez —
            onlar için aşağıdaki “Konuşma geçmişini sil” bölümünü kullanın.
          </>
        }
        openLabel="Tüm hafızayı sil"
        confirmLabel="Kalıcı olarak sil"
        phrase={MEMORY_DELETE_ALL_CONFIRMATION}
        run={runPurgeMemories}
      />

      <DangerAction
        heading="Konuşma geçmişini sil"
        description={
          <>
            <strong>Bütün</strong> oturum kayıtları (oturum özetleri dahil) ve diske yazılmış
            konuşma dökümü dosyaları kalıcı olarak silinir. Geri alınamaz. Oturum özeti bir
            sonraki konuşmanın başında Asuna’ya verilen bağlamın parçasıdır; silindikten sonra o
            özet bir daha hatırlanmaz. Çıkarılmış hafıza kayıtları bu işlemle silinmez — onlar
            için yukarıdaki “Tüm hafızayı sil” bölümünü ya da Hafıza sekmesini kullanın.
          </>
        }
        openLabel="Konuşma geçmişini sil"
        confirmLabel="Konuşma geçmişini kalıcı olarak sil"
        phrase={SESSION_CLEAR_ALL_CONFIRMATION}
        run={runClearSessions}
      />
    </section>
  );
}

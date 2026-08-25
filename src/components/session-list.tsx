/**
 * "Oturumlar" bolumu — oturum ozetlerini gorunur ve **silinebilir** kilar
 * (ASU-065).
 *
 * # Neden var (M3 blokaji)
 *
 * M3 kabul testinde kullanici hafiza kayitlarini sildi ama Asuna hatirlamaya
 * devam etti: Stage A her oturum acilisinda **son oturum ozetini** enjekte
 * ediyor ve `sessions.summary` hicbir yerden silinemiyordu. Hafiza sekmesi
 * "depo incelenebilir, kullanici silebilir" (PROJECT.md Bolum 20) sozunu
 * veriyorsa, hatirlamaya sebep olan her kayit burada gorunmeli.
 *
 * # Sinirlar
 *
 * - Bilesen `invoke` cagirmaz, SQL gormez: her sey [`SessionListPort`]
 *   uzerinden servis katmanina gider (ADR-005).
 * - **Dokum dosya yolu gosterilmez** cunku sozlesmede yok: renderer'a
 *   kullanicinin dizin yapisi tasinmaz. Gorunen sey "diskte dokum var mi".
 * - Onay **satir ici**; `window.confirm` yok (WKWebView'de tum pencereyi
 *   kilitler ve arka planda canli ses oturumu olabilir).
 * - Metin duz render edilir; `dangerouslySetInnerHTML` yok — ozet modelden gelir.
 */

import { useCallback, useEffect, useState } from 'react';

import type { SessionDeleteResult, SessionPage } from '../shared/session';

import { describeMemoryError } from './memory-text';
import { TRANSCRIPT_OUTCOME_TEXT, describeSessionTiming } from './session-text';

/** Ilk acilista gosterilen oturum sayisi. */
export const SESSION_PAGE_SIZE = 10;

/** Bilesenin servis yuzeyi; testler sahte port verir, uretimde servis katmani. */
export interface SessionListPort {
  readonly list: (limit?: number) => Promise<SessionPage>;
  readonly remove: (sessionId: number) => Promise<SessionDeleteResult>;
}

export interface SessionListProps {
  readonly port: SessionListPort;
  readonly pageSize?: number;
  /** Silme sonrasi hafiza listesi de tazelensin: kaynak oturum artik yok. */
  readonly onChanged?: () => void;
}

interface Notice {
  readonly tone: 'info' | 'error';
  readonly text: string;
}

export function SessionList({
  port,
  pageSize = SESSION_PAGE_SIZE,
  onChanged,
}: SessionListProps): React.JSX.Element {
  const [limit, setLimit] = useState(pageSize);
  const [page, setPage] = useState<SessionPage | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadToken, setReloadToken] = useState(0);

  const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  useEffect(() => {
    let cancelled = false;

    port.list(limit).then(
      (result) => {
        if (!cancelled) {
          setPage(result);
          setLoadError(null);
        }
      },
      (error: unknown) => {
        if (cancelled) {
          return;
        }
        // Hata varken bayat liste gosterilmez: ekranda ya dogru veri olur ya da
        // neden olmadigi yazar.
        setPage(null);
        setLoadError(describeMemoryError(error));
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [port, limit, reloadToken]);

  const handleConfirmDelete = useCallback(
    (id: number): void => {
      setBusyId(id);
      setNotice(null);

      port.remove(id).then(
        (result) => {
          setBusyId(null);
          setPendingDeleteId(null);

          if (result.status === 'skipped') {
            setNotice({
              tone: 'info',
              text: 'Hafıza kapalı olduğu için oturum kaydı silinmedi.',
            });
            return;
          }
          setNotice({ tone: 'info', text: TRANSCRIPT_OUTCOME_TEXT[result.transcriptFile] });
          setReloadToken((token) => token + 1);
          onChanged?.();
        },
        (error: unknown) => {
          setBusyId(null);
          setPendingDeleteId(null);
          setNotice({ tone: 'error', text: describeMemoryError(error) });
        },
      );
    },
    [onChanged, port],
  );

  const sessions = page?.sessions ?? [];
  // "Daha fazla" yalnizca gercekten daha fazlasi varsa cikar: sayi sunucudan
  // geliyor, UI tahmin yurutmuyor.
  const shown = sessions.length;
  const total = page?.total ?? 0;
  const atServerCap = page !== null && page.limit >= page.limitMax;
  const showLoadMore = page !== null && shown < total && !atServerCap;

  return (
    <section className="asuna-sessions" aria-label="Oturumlar">
      <h3 className="asuna-sessions__title">
        Oturumlar{page === null ? '' : ` (${shown.toString()} / ${total.toString()})`}
      </h3>
      <p className="asuna-sessions__hint">
        Her oturumun özeti bir sonraki konuşmanın başında Asuna’ya verilir. Bir oturumu silmek
        özetini de siler; o özet bir daha hatırlanmaz. Bu işlem oturumdan çıkarılmış hafıza
        kayıtlarını silmez — onlar yukarıda tek tek silinebilir.
      </p>

      {notice !== null && (
        <p className="asuna-memory__notice" role={notice.tone === 'error' ? 'alert' : 'status'}>
          {notice.text}
        </p>
      )}

      {loadError !== null && (
        <p className="asuna-memory__notice" role="alert">
          {loadError}
        </p>
      )}

      {page === null && loadError === null && (
        <p className="asuna-memory__notice">Oturumlar yükleniyor…</p>
      )}

      {page !== null && sessions.length === 0 && (
        <p className="asuna-memory__notice">Kayıtlı oturum yok.</p>
      )}

      {sessions.length > 0 && (
        <ul className="asuna-sessions__list" aria-label="Oturum kayıtları">
          {sessions.map((item) => (
            <li key={item.id} className="asuna-session-item" data-end-reason={item.endReason}>
              <div className="asuna-session-item__head">
                <h4 className="asuna-session-item__title">Oturum #{item.id.toString()}</h4>
                <time className="asuna-session-item__meta" dateTime={item.startedAt}>
                  {describeSessionTiming(item)}
                </time>
              </div>

              <p className="asuna-session-item__summary">
                {item.summaryPreview ?? 'Bu oturum için özet üretilmedi.'}
                {item.summaryTruncated && (
                  <span className="asuna-session-item__truncated"> (özet kısaltıldı)</span>
                )}
              </p>

              {item.hasTranscriptFile && (
                <p className="asuna-session-item__meta">Diskte konuşma dökümü dosyası var.</p>
              )}

              {pendingDeleteId === item.id ? (
                <div
                  className="asuna-memory-item__confirm"
                  role="group"
                  aria-label={`Oturum #${item.id.toString()} silme onayı`}
                >
                  <p className="asuna-memory-item__confirm-text">
                    Bu oturumun kaydı, özeti ve varsa döküm dosyası kalıcı olarak silinsin mi?
                    Geri alınamaz.
                  </p>
                  <button
                    type="button"
                    disabled={busyId === item.id}
                    onClick={(): void => {
                      handleConfirmDelete(item.id);
                    }}
                  >
                    Evet, sil
                  </button>
                  <button
                    type="button"
                    disabled={busyId === item.id}
                    onClick={(): void => {
                      setPendingDeleteId(null);
                    }}
                  >
                    Vazgeç
                  </button>
                </div>
              ) : (
                <div className="asuna-memory-item__actions">
                  <button
                    type="button"
                    disabled={busyId === item.id}
                    aria-label={`Sil: Oturum #${item.id.toString()}`}
                    onClick={(): void => {
                      setPendingDeleteId(item.id);
                      setNotice(null);
                    }}
                  >
                    Sil
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      {showLoadMore && (
        <button
          type="button"
          className="asuna-memory__more"
          onClick={(): void => {
            setLimit(limit + pageSize);
          }}
        >
          Daha fazla oturum yükle
        </button>
      )}

      {atServerCap && shown < total && (
        <p className="asuna-memory__notice">
          En yeni {page.limitMax.toString()} oturum gösteriliyor; toplam {total.toString()}{' '}
          oturum var. Tamamını temizlemek için Ayarlar &gt; Konuşma geçmişini sil.
        </p>
      )}
    </section>
  );
}

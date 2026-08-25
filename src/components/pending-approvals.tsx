/**
 * "Onay bekleyen hafızalar" bolumu (ASU-037; mekanizma ASU-034).
 *
 * # Neden var
 *
 * Cikarim, hassas turlerdeki (profil, iliski) adaylari kaydeder ama
 * `metadata_json.pendingApproval = true` ile isaretler; retrieval onlari
 * baglama koymaz. Isaret orada durur ama kullanici goremezse **hicbir zaman**
 * onaylanmaz — kayit sessizce olu veri olur. Bu bolum o kuyrugun gorunur
 * yuzudur (PROJECT.md Bolum 26 sonu).
 *
 * # Sinirlar
 *
 * - Filtre **UI tarafinda**: `MemoryFilter` bir `pendingApproval` boyutu
 *   sunmuyor ve bu task backend sozlesmesini genisletmiyor. Bu yuzden son
 *   [`PENDING_SCAN_LIMIT`] kayit taranir; daha buyuk depolarda kuyrugun tamami
 *   gorunmeyebilir. Gercek cozum sunucu tarafi filtre — backend task'i.
 * - Onaylamak bir **yazma**dir: kalici hafiza calisma zamaninda kapaliysa
 *   servis `skipped` doner ve bu ekran "onayladim" demez. Reddetmek (silme)
 *   her durumda calisir.
 * - Metin duz render edilir; `dangerouslySetInnerHTML` yok.
 */

import { useCallback, useEffect, useState } from 'react';

import {
  isPendingApproval,
  wasMemoryStored,
  withApprovalGranted,
  type MemoryFilter,
  type MemoryPatch,
  type MemoryRecord,
  type MemoryWriteResult,
} from '../shared/memory';

import { MEMORY_KIND_LABELS, describeMemoryError, describeMemorySource } from './memory-text';

/**
 * Onay kuyrugu icin taranan kayit sayisi. Rust tarafindaki `MAX_LIST_LIMIT`
 * ile ayni: daha buyugunu istemek sunucuda sessizce kirpilirdi.
 */
export const PENDING_SCAN_LIMIT = 200;

export interface PendingApprovalsProps {
  readonly list: (filter: MemoryFilter) => Promise<readonly MemoryRecord[]>;
  readonly update: (id: number, patch: MemoryPatch) => Promise<MemoryWriteResult>;
  readonly remove: (id: number) => Promise<MemoryWriteResult>;
  /** Kuyruk degistiginde ana listenin de tazelenmesi icin. */
  readonly onChanged: () => void;
}

interface Notice {
  readonly tone: 'info' | 'error';
  readonly text: string;
}

export function PendingApprovals({
  list,
  update,
  remove,
  onChanged,
}: PendingApprovalsProps): React.JSX.Element | null {
  const [records, setRecords] = useState<readonly MemoryRecord[]>([]);
  // Tarama tavana carptiysa kuyrugun tamamini gormedigimizi biliyoruz (sunucu
  // en fazla PENDING_SCAN_LIMIT kayit doner) — bunu kullaniciya acikca soyleriz.
  const [scanHitCap, setScanHitCap] = useState(false);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [reloadToken, setReloadToken] = useState(0);

  useEffect(() => {
    let cancelled = false;

    // Arsivli ve suresi dolmus kayitlar da taranir: onay bekleyen bir kayit
    // filtreye takilip gorunmez kalmasin.
    list({
      archived: 'all',
      includeExpired: true,
      sort: 'recent',
      limit: PENDING_SCAN_LIMIT,
    }).then(
      (result) => {
        if (!cancelled) {
          setRecords(result.filter(isPendingApproval));
          setScanHitCap(result.length >= PENDING_SCAN_LIMIT);
        }
      },
      () => {
        // Kuyrugu okuyamamak ana listeyi bozmamali; hata orada zaten
        // gosteriliyor. Burada sessizce bos kalir.
        if (!cancelled) {
          setRecords([]);
          setScanHitCap(false);
        }
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [list, reloadToken]);

  const runWrite = useCallback(
    (id: number, operation: () => Promise<MemoryWriteResult>, doneText: string): void => {
      setBusyId(id);
      setNotice(null);

      operation().then(
        (result) => {
          setBusyId(null);
          if (!wasMemoryStored(result)) {
            setNotice({
              tone: 'info',
              text: 'Hafıza kapalı olduğu için işlem uygulanmadı.',
            });
            return;
          }
          setNotice({ tone: 'info', text: doneText });
          setReloadToken((token) => token + 1);
          onChanged();
        },
        (error: unknown) => {
          setBusyId(null);
          setNotice({ tone: 'error', text: describeMemoryError(error) });
        },
      );
    },
    [onChanged],
  );

  const handleApprove = useCallback(
    (record: MemoryRecord): void => {
      runWrite(
        record.id,
        () => update(record.id, { metadataJson: withApprovalGranted(record.metadataJson) }),
        'Hafıza onaylandı.',
      );
    },
    [runWrite, update],
  );

  const handleReject = useCallback(
    (record: MemoryRecord): void => {
      runWrite(record.id, () => remove(record.id), 'Hafıza reddedildi ve silindi.');
    },
    [runWrite, remove],
  );

  // Kuyruk bossa bolum hic cizilmez: bos bir "onay bekleyen yok" kutusu her
  // gun goruldugunde anlamsizlasir.
  if (records.length === 0) {
    return notice === null ? null : (
      <p className="asuna-memory__notice" role={notice.tone === 'error' ? 'alert' : 'status'}>
        {notice.text}
      </p>
    );
  }

  return (
    <section className="asuna-approvals" aria-label="Onay bekleyen hafızalar">
      <h3 className="asuna-approvals__title">Onay bekleyen hafızalar ({records.length})</h3>
      <p className="asuna-approvals__hint">
        Bu kayıtlar hassas kabul edildi. Onaylanana kadar Asuna bunları konuşmaya getirmez;
        reddedilirse kalıcı olarak silinir.
      </p>

      {notice !== null && (
        <p className="asuna-memory__notice" role={notice.tone === 'error' ? 'alert' : 'status'}>
          {notice.text}
        </p>
      )}

      <ul className="asuna-approvals__list">
        {records.map((record) => (
          <li key={record.id} className="asuna-approvals__item" data-kind={record.kind}>
            <div className="asuna-memory-item__head">
              <span className="asuna-memory-item__badge">
                {MEMORY_KIND_LABELS[record.kind]}
              </span>
              <h4 className="asuna-memory-item__title">{record.title}</h4>
            </div>

            <p className="asuna-memory-item__body">{record.summary ?? record.content}</p>
            <p className="asuna-memory-item__meta">
              {describeMemorySource(record.sourceSessionId)}
            </p>

            <div className="asuna-memory-item__actions">
              <button
                type="button"
                disabled={busyId === record.id}
                aria-label={`Onayla: ${record.title}`}
                onClick={(): void => {
                  handleApprove(record);
                }}
              >
                Onayla
              </button>
              <button
                type="button"
                disabled={busyId === record.id}
                aria-label={`Reddet: ${record.title}`}
                onClick={(): void => {
                  handleReject(record);
                }}
              >
                Reddet
              </button>
            </div>
          </li>
        ))}
      </ul>
      {scanHitCap && (
        <p className="asuna-approvals__notice">
          Yalnızca en yeni {PENDING_SCAN_LIMIT} kayıt tarandı — kuyruğun tamamı bundan uzun
          olabilir.
        </p>
      )}
    </section>
  );
}

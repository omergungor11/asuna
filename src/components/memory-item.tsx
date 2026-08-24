/**
 * Tek hafiza kaydinin satiri (ASU-036).
 *
 * Saf sunum: kendi durumu yok, servis cagirmaz — props in, event out. Silme
 * onayinin acik olup olmadigini bile ust bilesen soyler, boylece ayni anda tek
 * satir onay bekleyebilir.
 *
 * `window.confirm` **kullanilmaz**: WKWebView'de tum pencereyi kilitler ve ses
 * oturumu arka planda calisiyor olabilir. Onay satir icinde, iptal edilebilir.
 *
 * Metin duz render edilir; `dangerouslySetInnerHTML` yok — icerik modelden gelir.
 */

import type { MemoryRecord } from '../shared/memory';

import { MEMORY_KIND_LABELS, describeMemorySource, formatMemoryTimestamp } from './memory-text';

/** Listede gosterilen govde metninin azami uzunlugu. */
export const BODY_PREVIEW_LIMIT = 240;

export interface MemoryItemProps {
  readonly record: MemoryRecord;
  /** Bu satir icin silme onayi bekleniyor mu? */
  readonly confirmingDelete: boolean;
  /** Bu satir uzerinde bir yazma islemi surerken butonlar kilitlenir. */
  readonly busy: boolean;
  readonly onRequestDelete: (id: number) => void;
  readonly onCancelDelete: () => void;
  readonly onConfirmDelete: (id: number) => void;
  readonly onToggleArchive: (record: MemoryRecord) => void;
}

function previewOf(record: MemoryRecord): string {
  const body = record.summary ?? record.content;
  return body.length > BODY_PREVIEW_LIMIT ? `${body.slice(0, BODY_PREVIEW_LIMIT)}…` : body;
}

export function MemoryItem({
  record,
  confirmingDelete,
  busy,
  onRequestDelete,
  onCancelDelete,
  onConfirmDelete,
  onToggleArchive,
}: MemoryItemProps): React.JSX.Element {
  return (
    <li className="asuna-memory-item" data-kind={record.kind} data-archived={record.isArchived}>
      <div className="asuna-memory-item__head">
        <span className="asuna-memory-item__badge">{MEMORY_KIND_LABELS[record.kind]}</span>
        <h3 className="asuna-memory-item__title">{record.title}</h3>
        {record.isArchived && <span className="asuna-memory-item__archived">arşivde</span>}
      </div>

      <p className="asuna-memory-item__body">{previewOf(record)}</p>

      <p className="asuna-memory-item__meta">
        <time dateTime={record.createdAt}>{formatMemoryTimestamp(record.createdAt)}</time>
        {' · '}
        <span className="asuna-memory-item__source">
          {describeMemorySource(record.sourceSessionId)}
        </span>
      </p>

      {confirmingDelete ? (
        <div
          className="asuna-memory-item__confirm"
          role="group"
          aria-label={`${record.title} silme onayı`}
        >
          <p className="asuna-memory-item__confirm-text">
            Bu hafıza kalıcı olarak silinsin mi? Geri alınamaz.
          </p>
          <button
            type="button"
            disabled={busy}
            onClick={(): void => {
              onConfirmDelete(record.id);
            }}
          >
            Evet, sil
          </button>
          <button type="button" disabled={busy} onClick={onCancelDelete}>
            Vazgeç
          </button>
        </div>
      ) : (
        <div className="asuna-memory-item__actions">
          {/* Erisilebilir ad basligi tasir: listede on tane "Sil" butonu varken
              hangisinin hangi kayda ait oldugu ekran okuyucuda da belli olsun. */}
          <button
            type="button"
            disabled={busy}
            aria-label={`${record.isArchived ? 'Arşivden çıkar' : 'Arşivle'}: ${record.title}`}
            onClick={(): void => {
              onToggleArchive(record);
            }}
          >
            {record.isArchived ? 'Arşivden çıkar' : 'Arşivle'}
          </button>
          <button
            type="button"
            disabled={busy}
            aria-label={`Sil: ${record.title}`}
            onClick={(): void => {
              onRequestDelete(record.id);
            }}
          >
            Sil
          </button>
        </div>
      )}
    </li>
  );
}

/**
 * "Projeden dosya ekle" — guncel proje kokunde gezinilebilir kucuk secici
 * (plan-chat-shell.md WP3).
 *
 * # Sinirlar
 *
 * - Bilesen `invoke` cagirmaz: dizin icerigi [`ProjectDirectorySource`] portu
 *   uzerinden gelir, portu kompozisyon koku (`src/app/`) baglar. Yol cozumu,
 *   traversal reddi, blok listesi ve 200 girdi tavani guvenilir tarafta
 *   (`src-tauri/src/projects/listing.rs` + `security::sandbox`).
 * - Renderer **mutlak yol kuramaz**: yukari cikma yalnizca kok'e kadar gider,
 *   `..` metni hicbir zaman komuta gonderilmez (yol, girdi adlarindan yeniden
 *   kurulur).
 * - Blok listesindeki dosyalar **gizlenmez**, "okunamaz" olarak isaretlenir ve
 *   tiklanamaz: kullaniciyi "neden gormuyorum?" diye sasirtmaktansa kuralin
 *   gorunur olmasi yeglenir (`list-project-files.ts` ile ayni gerekce).
 * - Tum metinler duz render edilir; `dangerouslySetInnerHTML` yok.
 */

import { useEffect, useState } from 'react';

import type { ProjectDirectoryView } from '../asuna/tools/list-project-files';

import { describeAttachmentSize, describeChatError } from './chat-text';

/**
 * Dizin listeleme kaynagi.
 *
 * @param path Proje kokune **gore** yol; kok icin bos metin.
 */
export type ProjectDirectorySource = (path: string) => Promise<ProjectDirectoryView>;

export interface ProjectFilePickerProps {
  readonly source: ProjectDirectorySource;
  /** Secilen dosyanin kok'e gore yolu. */
  readonly onPick: (relativePath: string) => void;
  readonly onClose: () => void;
  /** Ekleme surerken liste kilitlenir: iki kez eklenmesin. */
  readonly busy?: boolean;
}

/** Kok'e gore yolu birlestirir — `..` uretmez, mutlak yol kuramaz. */
function joinPath(base: string, name: string): string {
  return base === '' ? name : `${base}/${name}`;
}

/** Bir ust dizin; kok'te `null` (yukarisi yok). */
function parentOf(path: string): string | null {
  if (path === '') {
    return null;
  }
  const index = path.lastIndexOf('/');
  return index === -1 ? '' : path.slice(0, index);
}

export function ProjectFilePicker({
  source,
  onPick,
  onClose,
  busy = false,
}: ProjectFilePickerProps): React.JSX.Element {
  const [path, setPath] = useState('');
  const [view, setView] = useState<ProjectDirectoryView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    source(path).then(
      (result) => {
        if (!cancelled) {
          setView(result);
          setError(null);
        }
      },
      (failure: unknown) => {
        if (cancelled) {
          return;
        }
        // Bayat liste gosterilmez: ya dogru icerik ya da neden olmadigi yazar.
        setView(null);
        setError(describeChatError(failure));
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [source, path]);

  const parent = parentOf(path);
  const projectName = view === null ? 'Proje' : view.projectName;
  const location = path === '' ? projectName : `${projectName} / ${path}`;

  return (
    <section className="asuna-file-picker" aria-label="Projeden dosya ekle">
      <div className="asuna-file-picker__head">
        <p className="asuna-file-picker__path">{location}</p>
        <button type="button" onClick={onClose}>
          Kapat
        </button>
      </div>

      {error !== null && (
        <p className="asuna-file-picker__notice" role="alert">
          {error}
        </p>
      )}

      {view === null && error === null && (
        <p className="asuna-file-picker__notice">Klasör okunuyor…</p>
      )}

      {view !== null && (
        <>
          {view.truncated && (
            <p className="asuna-file-picker__notice">
              {/* `scanCapped`: tarama tavanina takildi — `totalEntries` bir ALT
                  sinirdir; kesin sayi gibi gosterilirse yalan olur. */}
              Yalnızca ilk {view.returnedEntries.toString()} girdi gösteriliyor (
              {view.scanCapped
                ? `en az ${view.totalEntries.toString()} girdi var`
                : `toplam ${view.totalEntries.toString()}`}
              ).
            </p>
          )}

          {view.entries.length === 0 && (
            <p className="asuna-file-picker__notice">Bu klasör boş.</p>
          )}

          <ul className="asuna-file-picker__list" aria-label="Klasör içeriği">
            {parent !== null && (
              <li className="asuna-file-picker__entry">
                <button
                  type="button"
                  disabled={busy}
                  onClick={(): void => {
                    setPath(parent);
                  }}
                >
                  ↑ Üst klasör
                </button>
              </li>
            )}

            {view.entries.map((entry) => {
              const size =
                entry.kind === 'file' ? describeAttachmentSize(entry.sizeBytes) : null;
              const label =
                entry.kind === 'dir'
                  ? `${entry.name}/`
                  : `${entry.name}${size === null ? '' : ` · ${size}`}`;

              return (
                <li
                  key={entry.name}
                  className="asuna-file-picker__entry"
                  data-kind={entry.kind}
                  data-blocked={entry.blocked}
                >
                  <button
                    type="button"
                    // Okunamaz girdi listede kalir ama eklenemez.
                    disabled={busy || entry.blocked || entry.kind === 'other'}
                    onClick={(): void => {
                      if (entry.kind === 'dir') {
                        setPath(joinPath(path, entry.name));
                        return;
                      }
                      onPick(joinPath(path, entry.name));
                    }}
                  >
                    {label}
                  </button>
                  {entry.blocked && <span className="asuna-file-picker__badge">okunamaz</span>}
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}

/**
 * Kayitli tek projenin satiri (ASU-045).
 *
 * Saf sunum: kendi durumu yok, servis cagirmaz — props in, event out. Kaldirma
 * onayinin acik olup olmadigini ust bilesen soyler; boylece ayni anda tek satir
 * onay bekler.
 *
 * `window.confirm` **kullanilmaz**: WKWebView'de tum pencereyi kilitler ve ses
 * oturumu arka planda calisiyor olabilir (bkz. `memory-item.tsx`).
 *
 * Yol duz metin olarak render edilir; `dangerouslySetInnerHTML` yok.
 */

import type { ProjectRecord } from '../shared/project';

import {
  PROJECT_STATUS_HINTS,
  PROJECT_STATUS_LABELS,
  describeLastOpened,
  describeProjectPath,
  describeProjectStack,
} from './project-text';

export interface ProjectItemProps {
  readonly project: ProjectRecord;
  /** Guncel proje bu mu? (`currentProjectOf` ile turetilir, ayri bayrak degil.) */
  readonly current: boolean;
  readonly confirmingRemove: boolean;
  /** Bu satir uzerinde bir yazma islemi surerken butonlar kilitlenir. */
  readonly busy: boolean;
  readonly onSetCurrent: (project: ProjectRecord) => void;
  readonly onRequestRemove: (projectId: string) => void;
  readonly onCancelRemove: () => void;
  readonly onConfirmRemove: (projectId: string) => void;
}

export function ProjectItem({
  project,
  current,
  confirmingRemove,
  busy,
  onSetCurrent,
  onRequestRemove,
  onCancelRemove,
  onConfirmRemove,
}: ProjectItemProps): React.JSX.Element {
  const hint = PROJECT_STATUS_HINTS[project.status];
  // Kayitli kokü olmayan bir etiket "guncel proje" olamaz (Rust tarafi da
  // reddeder): reddedilecegi bilinen istek hic uretilmez.
  const selectable = !current && project.status !== 'unlinked';

  return (
    <li className="asuna-project" data-status={project.status} data-current={current}>
      <div className="asuna-project__head">
        <h3 className="asuna-project__name">{project.name}</h3>
        {current && (
          <span className="asuna-project__badge asuna-project__badge--current">güncel</span>
        )}
        {/* Durum rozeti her zaman yazili: `missing` bir kayit "kayıtlı" gibi
            gorunmemeli (ASU-040). */}
        <span className="asuna-project__badge" data-status={project.status}>
          {PROJECT_STATUS_LABELS[project.status]}
        </span>
      </div>

      <p className="asuna-project__path">{describeProjectPath(project)}</p>

      <p className="asuna-project__meta">
        {describeProjectStack(project)}
        {' · son açılma: '}
        {describeLastOpened(project.lastOpenedAt)}
      </p>

      {hint !== null && <p className="asuna-project__hint">{hint}</p>}

      {confirmingRemove ? (
        <div
          className="asuna-project__confirm"
          role="group"
          aria-label={`${project.name} kaldırma onayı`}
        >
          <p className="asuna-project__confirm-text">
            Bu proje kayıttan çıkarılsın mı? Diskteki dosyalara dokunulmaz. Projeye bağlı hafıza
            varsa kayıt silinmez, hafıza etiketine dönüşür.
          </p>
          <button
            type="button"
            disabled={busy}
            onClick={(): void => {
              onConfirmRemove(project.id);
            }}
          >
            Evet, kaldır
          </button>
          <button type="button" disabled={busy} onClick={onCancelRemove}>
            Vazgeç
          </button>
        </div>
      ) : (
        <div className="asuna-project__actions">
          {/* Erisilebilir ad basligi tasir: listede on tane "Kaldır" varken
              hangisinin hangi projeye ait oldugu ekran okuyucuda da belli olsun. */}
          <button
            type="button"
            disabled={busy || !selectable}
            aria-label={`Güncel proje yap: ${project.name}`}
            onClick={(): void => {
              onSetCurrent(project);
            }}
          >
            Güncel proje yap
          </button>
          <button
            type="button"
            disabled={busy}
            aria-label={`Kaldır: ${project.name}`}
            onClick={(): void => {
              onRequestRemove(project.id);
            }}
          >
            Kaldır
          </button>
        </div>
      )}
    </li>
  );
}

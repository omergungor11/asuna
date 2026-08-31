/**
 * Mesaj yazma alani (plan-chat-shell.md WP3).
 *
 * # Saf sunum
 *
 * Bilesen servis cagirmaz, IPC bilmez, dosya **okumaz**: secilen `File`
 * nesnelerini oldugu gibi yukari verir; metne cevirme ve gonderme
 * `chat-service.ingestAttachment` isidir (redaksiyon, boyut siniri ve dosya-adi
 * blocklist'i Rust tarafinda).
 *
 * # Klavye sozlesmesi
 *
 * Enter gonderir, Shift+Enter yeni satir acar. Bos (yalnizca bosluk) mesaj
 * gonderilmez ve alan temizlenmez — kullanicinin yazdigi kaybolmaz.
 *
 * # Mikrofon
 *
 * Mikrofon butonu burada bir kayit baslatmaz: ses **ayri bir mod** (VoicePanel).
 * Buton yalnizca o moda gecis sinyali verir; canli oturum kabukta yasar.
 */

import { useRef, useState } from 'react';

import type { ChatAttachment } from '../shared/chat';

import { describeAttachment } from './chat-text';
import { ProjectFilePicker, type ProjectDirectorySource } from './project-file-picker';

export interface ComposerProps {
  /** Yanit beklenirken gonderim kilitlenir. */
  readonly sending: boolean;
  /** Henuz bir mesaja baglanmamis, bu gonderimle gidecek dosyalar. */
  readonly pendingAttachments: readonly ChatAttachment[];
  readonly attaching: boolean;
  /** Dosya eklenemediyse nedeni; gizlenmez. */
  readonly attachError: string | null;
  /**
   * Konusma bir projeye bagliysa proje kimligi — "Projeden dosya ekle"
   * yalnizca o zaman anlamli (aksi halde hangi kok icinde bakilacagi belirsiz).
   */
  readonly projectId: string | null;
  /** Proje dosyasi secici icin dizin kaynagi; yoksa secici gosterilmez. */
  readonly listProjectDirectory?: ProjectDirectorySource;
  readonly onSend: (text: string) => void;
  readonly onAttachFiles: (files: readonly File[]) => void;
  readonly onAttachProjectFile: (relativePath: string) => void;
  readonly onOpenVoice?: () => void;
}

export function Composer({
  sending,
  pendingAttachments,
  attaching,
  attachError,
  projectId,
  listProjectDirectory,
  onSend,
  onAttachFiles,
  onAttachProjectFile,
  onOpenVoice,
}: ComposerProps): React.JSX.Element {
  const [draft, setDraft] = useState('');
  const [pickerOpen, setPickerOpen] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const submit = (): void => {
    const text = draft.trim();
    if (text === '' || sending) {
      return;
    }
    onSend(text);
    setDraft('');
  };

  const showPicker = projectId !== null && listProjectDirectory !== undefined;

  return (
    <div className="asuna-composer">
      {pendingAttachments.length > 0 && (
        <ul className="asuna-composer__chips" aria-label="Eklenecek dosyalar">
          {pendingAttachments.map((attachment) => (
            <li
              key={attachment.id}
              className="asuna-chip"
              data-origin={attachment.origin}
              title={describeAttachment(attachment)}
            >
              {describeAttachment(attachment)}
            </li>
          ))}
        </ul>
      )}

      {attachError !== null && (
        <p className="asuna-composer__notice" role="alert">
          {attachError}
        </p>
      )}

      {showPicker && pickerOpen && (
        <ProjectFilePicker
          source={listProjectDirectory}
          busy={attaching}
          onPick={(relativePath): void => {
            onAttachProjectFile(relativePath);
            setPickerOpen(false);
          }}
          onClose={(): void => {
            setPickerOpen(false);
          }}
        />
      )}

      <div className="asuna-composer__row">
        <label className="asuna-composer__field">
          <span className="asuna-visually-hidden">Mesaj</span>
          <textarea
            className="asuna-composer__input"
            value={draft}
            rows={2}
            placeholder="Asuna’ya yaz…"
            spellCheck={false}
            onChange={(event): void => {
              setDraft(event.target.value);
            }}
            onKeyDown={(event): void => {
              // Shift+Enter yeni satir; IME bileseni sirasinda Enter gonderme.
              if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                event.preventDefault();
                submit();
              }
            }}
          />
        </label>

        <div className="asuna-composer__actions">
          <input
            ref={fileInput}
            className="asuna-visually-hidden"
            type="file"
            multiple
            aria-label="Dosya seç"
            onChange={(event): void => {
              const files = Array.from(event.target.files ?? []);
              // Ayni dosya arka arkaya secilebilsin: input degeri sifirlanir.
              event.target.value = '';
              if (files.length > 0) {
                onAttachFiles(files);
              }
            }}
          />

          <button
            type="button"
            aria-label="Dosya ekle"
            disabled={attaching}
            onClick={(): void => {
              fileInput.current?.click();
            }}
          >
            📎
          </button>

          {showPicker && (
            <button
              type="button"
              disabled={attaching}
              onClick={(): void => {
                setPickerOpen((open) => !open);
              }}
            >
              Projeden dosya ekle
            </button>
          )}

          {onOpenVoice !== undefined && (
            <button type="button" aria-label="Ses moduna geç" onClick={onOpenVoice}>
              🎙
            </button>
          )}

          <button
            type="button"
            className="asuna-composer__send"
            disabled={sending || draft.trim() === ''}
            onClick={submit}
          >
            Gönder
          </button>
        </div>
      </div>

      <p className="asuna-composer__hint">
        Enter gönderir, Shift+Enter yeni satır. Eklenen dosyalar Asuna’ya metin olarak gider;
        gizli anahtarlar kaydedilmeden önce maskelenir.
      </p>
    </div>
  );
}

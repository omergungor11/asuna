/**
 * Tek bir konusmanin ekrani (plan-chat-shell.md WP3).
 *
 * # Pivot notu
 *
 * Asuna artik ChatGPT/Claude benzeri bir metin sohbetini de tasiyor (ADR-006).
 * Eski "sohbet penceresi kurma" yasagi bu kararla degisti; ses **silinmedi**,
 * ayri bir mod olarak duruyor (`VoicePanel` kabukta hep monte).
 *
 * # Sinirlar
 *
 * - Bilesen `invoke` cagirmaz, SQL gormez, OpenAI'ye istek atmaz: her sey
 *   [`ChatViewPort`] uzerinden `src/asuna/agent/chat-service` fonksiyonlarina
 *   gider (ADR-005 / CLAUDE.md).
 * - Mesaj metni **duz** render edilir (`white-space: pre-wrap`); v1'de markdown
 *   yok ve `dangerouslySetInnerHTML` hicbir kosulda yok — icerik model
 *   ciktisindan ve proje dosyalarindan geliyor olabilir.
 * - Basari taklidi yok: gonderim basarisiz olursa mesaj listeye **eklenmez**,
 *   hata gorunur (PROJECT.md Bolum 30).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  attachProjectFile,
  ingestAttachment,
  listAttachments,
  listMessages,
  sendMessage,
  setConversationTitle,
} from '../asuna/agent/chat-service';
import { logger, type UserFacingError } from '../asuna/observability';
import type { ChatAttachment, ChatMessage, ChatReply } from '../shared/chat';

import {
  VOICE_COMPOSER_NOTE,
  VOICE_EMPTY_STATE,
  chatErrorNotice,
  describeAttachment,
  describeChatError,
  deriveConversationTitle,
} from './chat-text';
import { Composer } from './composer';
import { ErrorNotice } from './error-notice';
import { formatMemoryTimestamp } from './memory-text';
import type { ProjectDirectorySource } from './project-file-picker';

/** Bilesenin servis yuzeyi; testler sahte port verir, uretimde servis katmani. */
export interface ChatViewPort {
  readonly listMessages: (sessionId: number) => Promise<readonly ChatMessage[]>;
  readonly listAttachments: (sessionId: number) => Promise<readonly ChatAttachment[]>;
  readonly sendMessage: (
    sessionId: number,
    text: string,
    attachmentIds: readonly number[],
  ) => Promise<ChatReply>;
  readonly setTitle: (sessionId: number, title: string) => Promise<void>;
  readonly ingestAttachment: (sessionId: number, file: File) => Promise<ChatAttachment>;
  readonly attachProjectFile: (
    sessionId: number,
    relativePath: string,
  ) => Promise<ChatAttachment>;
}

/** Uretim portu: dogrudan servis katmani (`chat-service`). */
const DEFAULT_CHAT_PORT: ChatViewPort = {
  listMessages,
  listAttachments,
  sendMessage,
  setTitle: setConversationTitle,
  ingestAttachment,
  attachProjectFile,
};

export interface ChatViewProps {
  readonly sessionId: number;
  /** Konusmanin projesi; yoksa `null` (proje dosyasi eklenemez). */
  readonly projectId: string | null;
  /**
   * Oturumun turu (varsayilan `text`).
   *
   * `voice` ise ekran **salt okunur**: composer render edilmez, baslik
   * otomasyonu calismaz. Gerekce tasarimsal, hata degil — `chat_send` ses
   * oturumlarini reddediyor (review M2) ve kullaniciya once "yaz, sonra
   * reddedildi" dedirtmek durust bir arayuz olmazdi.
   */
  readonly modality?: 'voice' | 'text';
  readonly port?: ChatViewPort;
  /** Proje dosyasi secicinin dizin kaynagi — kompozisyon koku baglar. */
  readonly listProjectDirectory?: ProjectDirectorySource;
  /** Baslik/mesaj degisti: kenar cubugundaki liste tazelensin. */
  readonly onConversationChanged?: () => void;
  readonly onOpenVoice?: () => void;
}

const ROLE_LABELS: Readonly<Record<ChatMessage['role'], string>> = {
  user: 'Sen',
  assistant: 'Asuna',
  system: 'Sistem',
  tool: 'Araç',
};

/**
 * Ekranin tum durumu **tek** nesnede ve konusma kimligiyle etiketli.
 *
 * Gerekce: konusma degisince durumu bir `useEffect` icinde `setState` ile
 * sifirlamak zincirleme render uretir (React'in "you might not need an effect"
 * uyarisi). Kimligi durumun icine koyunca sifirlama **turetilmis** olur:
 * kimlik uyusmuyorsa bos durum render edilir, hicbir effect gerekmez. Ayni
 * etiket bayat yanitlara karsi da kalkan: baska bir konusma icin gelen gec
 * cevap yazilmaz.
 */
interface ConversationState {
  readonly sessionId: number;
  readonly messages: readonly ChatMessage[] | null;
  readonly attachments: readonly ChatAttachment[];
  readonly loadError: string | null;
  /**
   * Ek listesi okunamadi (review M4).
   *
   * Mesajlardan **ayri** tutulur: dosya listesinin dusmesi konusmayi gizlemez,
   * yalnizca "cipler eksik olabilir" uyarisi cikarir.
   */
  readonly attachmentsError: string | null;
  readonly sending: boolean;
  readonly sendError: UserFacingError | null;
  readonly attaching: boolean;
  readonly attachError: string | null;
  readonly titleError: string | null;
}

function emptyConversationState(sessionId: number): ConversationState {
  return {
    sessionId,
    messages: null,
    attachments: [],
    loadError: null,
    attachmentsError: null,
    sending: false,
    sendError: null,
    attaching: false,
    attachError: null,
    titleError: null,
  };
}

export function ChatView({
  sessionId,
  projectId,
  modality = 'text',
  port = DEFAULT_CHAT_PORT,
  listProjectDirectory,
  onConversationChanged,
  onOpenVoice,
}: ChatViewProps): React.JSX.Element {
  const [stored, setStored] = useState<ConversationState>(() =>
    emptyConversationState(sessionId),
  );
  const state = stored.sessionId === sessionId ? stored : emptyConversationState(sessionId);
  const isVoice = modality === 'voice';

  const endRef = useRef<HTMLDivElement>(null);

  /** Yalnizca **acik** konusmanin durumunu gunceller; bayat yanit yazilmaz. */
  const patch = useCallback(
    (updater: (previous: ConversationState) => ConversationState): void => {
      setStored((previous) =>
        previous.sessionId === sessionId ? updater(previous) : previous,
      );
    },
    [sessionId],
  );

  /**
   * Mesajlar ve ekler **ayri** yuklenir (review M4).
   *
   * Onceden ikisi `Promise.all` ile baglanmisti: `attachment_list` duserse
   * konusmanin kendisi de kayboluyordu — kullanici okunabilir mesajlarini
   * bir dosya listesi hatasi yuzunden goremezdi. Simdi mesajlar **birincil**,
   * ek listesi ikincil; ikincisi duserse yalnizca satir ici bir uyari cikar.
   *
   * Iki zincir de fonksiyonel guncelleme kullanir ve ayni `sessionId` etiketini
   * korur: hangisi once biterse bitsin digerinin sonucunu ezmez.
   */
  useEffect(() => {
    let cancelled = false;

    const baseFor = (previous: ConversationState): ConversationState =>
      previous.sessionId === sessionId ? previous : emptyConversationState(sessionId);

    port.listMessages(sessionId).then(
      (loaded) => {
        if (!cancelled) {
          setStored((previous) => ({
            ...baseFor(previous),
            messages: loaded,
            loadError: null,
          }));
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          // Bayat icerik gosterilmez: ya konusmanin kendisi ya da neden
          // acilamadigi ekranda olur.
          setStored((previous) => ({
            ...baseFor(previous),
            messages: [],
            loadError: describeChatError(error),
          }));
        }
      },
    );

    port.listAttachments(sessionId).then(
      (files) => {
        if (!cancelled) {
          setStored((previous) => ({
            ...baseFor(previous),
            attachments: files,
            attachmentsError: null,
          }));
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          setStored((previous) => ({
            ...baseFor(previous),
            attachmentsError: `Dosya listesi okunamadı: ${describeChatError(error)}`,
          }));
        }
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [port, sessionId]);

  // Yeni mesaj gelince en alta kay. jsdom `scrollIntoView` implemente etmez,
  // bu yuzden varligi kontrol edilir — test ortami bu yuzden patlamaz.
  useEffect(() => {
    const node = endRef.current;
    if (node !== null && typeof node.scrollIntoView === 'function') {
      node.scrollIntoView({ block: 'end' });
    }
  }, [state.messages, state.sending]);

  const attachments = state.attachments;
  const pending = useMemo(
    () => attachments.filter((attachment) => attachment.messageId === null),
    [attachments],
  );

  const messages = state.messages;

  const handleSend = useCallback(
    (text: string): void => {
      // Savunma katmani: ses oturumunda composer zaten render edilmiyor, ama
      // bu kapi kapali kalmali — `chat_send` voice oturumlari reddediyor ve
      // baslik otomasyonu da tetiklenmemeli (review M2).
      if (isVoice) {
        return;
      }

      const attachmentIds = pending.map((attachment) => attachment.id);
      // Baslik yalnizca ILK kullanici mesajindan turetilir; sonrakiler
      // kullanicinin/otomatik konan basligi ezmez.
      const isFirstUserMessage = (messages ?? []).every((message) => message.role !== 'user');

      patch((previous) => ({
        ...previous,
        sending: true,
        sendError: null,
        titleError: null,
      }));

      port.sendMessage(sessionId, text, attachmentIds).then(
        (reply) => {
          patch((previous) => ({
            ...previous,
            sending: false,
            messages: [...(previous.messages ?? []), reply.userMessage, reply.assistantMessage],
            // Bekleyen dosyalar artik kullanici mesajina bagli: cipler yukari
            // tasinir, composer temizlenir.
            attachments: previous.attachments.map((attachment) =>
              attachmentIds.includes(attachment.id)
                ? { ...attachment, messageId: reply.userMessage.id }
                : attachment,
            ),
          }));

          if (!isFirstUserMessage) {
            onConversationChanged?.();
            return;
          }

          port.setTitle(sessionId, deriveConversationTitle(text)).then(
            () => {
              onConversationChanged?.();
            },
            (error: unknown) => {
              // Yutulmaz: mesaj gitti ama baslik konmadi — kullanici bunu
              // gorur, redaksiyonlu logger'a da duser (ASU-019).
              logger.error('konusma basligi kaydedilemedi', {
                sessionId,
                reason: describeChatError(error),
              });
              patch((previous) => ({
                ...previous,
                titleError: `Başlık kaydedilemedi: ${describeChatError(error)}`,
              }));
              onConversationChanged?.();
            },
          );
        },
        (error: unknown) => {
          // Basari taklidi yok: mesaj listeye eklenmez, hata gorunur.
          patch((previous) => ({
            ...previous,
            sending: false,
            sendError: chatErrorNotice(error),
          }));
        },
      );
    },
    [isVoice, messages, onConversationChanged, patch, pending, port, sessionId],
  );

  const handleAttachFiles = useCallback(
    (files: readonly File[]): void => {
      patch((previous) => ({ ...previous, attaching: true, attachError: null }));

      Promise.all(files.map((file) => port.ingestAttachment(sessionId, file))).then(
        (records) => {
          patch((previous) => ({
            ...previous,
            attaching: false,
            attachments: [...previous.attachments, ...records],
          }));
        },
        (error: unknown) => {
          patch((previous) => ({
            ...previous,
            attaching: false,
            attachError: describeChatError(error),
          }));
        },
      );
    },
    [patch, port, sessionId],
  );

  const handleAttachProjectFile = useCallback(
    (relativePath: string): void => {
      patch((previous) => ({ ...previous, attaching: true, attachError: null }));

      port.attachProjectFile(sessionId, relativePath).then(
        (record) => {
          patch((previous) => ({
            ...previous,
            attaching: false,
            attachments: [...previous.attachments, record],
          }));
        },
        (error: unknown) => {
          patch((previous) => ({
            ...previous,
            attaching: false,
            attachError: describeChatError(error),
          }));
        },
      );
    },
    [patch, port, sessionId],
  );

  return (
    <section className="asuna-chat" aria-label="Konuşma">
      <div className="asuna-chat__messages" aria-live="polite">
        {messages === null && <p className="asuna-chat__notice">Mesajlar yükleniyor…</p>}

        {state.loadError !== null && (
          <p className="asuna-chat__notice" role="alert">
            {state.loadError}
          </p>
        )}

        {/* Ek listesi ikincil: dusmesi konusmayi gizlemez, hafif bir uyari
            birakir — cipler eksik gorunuyor olabilir (review M4). */}
        {state.attachmentsError !== null && (
          <p className="asuna-chat__hint" role="status">
            {state.attachmentsError}
          </p>
        )}

        {messages !== null && messages.length === 0 && state.loadError === null && (
          <p className="asuna-chat__notice">
            {isVoice
              ? VOICE_EMPTY_STATE
              : 'Bu konuşma boş. İlk mesajı yaz — konuşma ve dosyalar kalıcı olarak saklanır, istediğin an silebilirsin.'}
          </p>
        )}

        {(messages ?? []).map((message) => {
          const files = attachments.filter((attachment) => attachment.messageId === message.id);

          return (
            <article
              key={message.id}
              className="asuna-message"
              data-role={message.role}
              aria-label={`${ROLE_LABELS[message.role]} mesajı`}
            >
              <header className="asuna-message__head">
                <span className="asuna-message__role">{ROLE_LABELS[message.role]}</span>
                <time className="asuna-message__time" dateTime={message.createdAt}>
                  {formatMemoryTimestamp(message.createdAt)}
                </time>
              </header>

              {files.length > 0 && (
                <ul className="asuna-message__chips" aria-label="Mesajın dosyaları">
                  {files.map((attachment) => (
                    <li
                      key={attachment.id}
                      className="asuna-chip"
                      data-origin={attachment.origin}
                    >
                      {describeAttachment(attachment)}
                    </li>
                  ))}
                </ul>
              )}

              <p className="asuna-message__body">{message.content}</p>
            </article>
          );
        })}

        {state.sending && (
          <p className="asuna-chat__typing" role="status">
            Asuna yazıyor…
          </p>
        )}

        <div ref={endRef} />
      </div>

      {state.sendError !== null && <ErrorNotice error={state.sendError} />}

      {state.titleError !== null && (
        <p className="asuna-chat__notice" role="alert">
          {state.titleError}
        </p>
      )}

      {/* Ses oturumu salt okunur: composer hic render edilmez. Kapali bir
          metin alani gostermek "yaz ama calismaz" demek olurdu (review M2). */}
      {isVoice ? (
        <p className="asuna-chat__readonly">
          {VOICE_COMPOSER_NOTE}
          {onOpenVoice !== undefined && (
            <>
              {' '}
              <button type="button" onClick={onOpenVoice}>
                Ses moduna geç
              </button>
            </>
          )}
        </p>
      ) : (
        <Composer
          sending={state.sending}
          pendingAttachments={pending}
          attaching={state.attaching}
          attachError={state.attachError}
          projectId={projectId}
          {...(listProjectDirectory === undefined ? {} : { listProjectDirectory })}
          {...(onOpenVoice === undefined ? {} : { onOpenVoice })}
          onSend={handleSend}
          onAttachFiles={handleAttachFiles}
          onAttachProjectFile={handleAttachProjectFile}
        />
      )}
    </section>
  );
}

/**
 * Metin sohbetinin renderer tarafindaki **tek** erisim noktasi (plan-chat-shell.md).
 *
 * # Sozlesme
 *
 * - Renderer modeli secmez, API anahtari gormez, prompt insa etmez: `chat_send`
 *   Rust tarafinda konusma gecmisini DB'den okur, OpenAI'yi cagirir ve iki
 *   mesaji (kullanici + asistan) kalici hale getirip doner.
 * - Dosya ekleme icin YENI Tauri plugin'i yok: `<input type="file">` ile alinan
 *   `File` burada **metin olarak** okunur ve icerik Rust'a gider; redaksiyon,
 *   boyut siniri ve dosya-adi blocklist'i Rust tarafindadir. Bu servis o
 *   kontrollerin hicbirini yerine getirmis gibi davranmaz.
 * - Proje dosyasi ekleme yolu (`attachment_from_project`) sandbox'tan gecer;
 *   renderer yalnizca proje-goreli yol soyler, mutlak yol kuramaz.
 * - Hafiza kapaliyken konusma ACILMAZ (`skipped`) — metin sohbeti kalici
 *   olmak zorundadir; sahte/gecici bir konusma uydurulmaz.
 */

import { invoke } from '@tauri-apps/api/core';

import {
  parseChatAttachment,
  parseChatAttachmentList,
  parseChatMessageList,
  parseChatReply,
  parseConversationList,
  parseConversationStartResult,
  type ChatAttachment,
  type ChatMessage,
  type ChatReply,
  type ConversationStartResult,
  type ConversationSummary,
} from '../../shared/chat';
import { toStoreError } from '../../shared/store-error';

/**
 * Rust tarafindaki komut adlari. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-chat.json` ile birebir ayni olmali.
 */
export const CHAT_COMMANDS = {
  conversationList: 'conversation_list',
  start: 'session_start',
  setTitle: 'session_set_title',
  delete: 'session_delete',
  messageList: 'message_list',
  send: 'chat_send',
  attachmentIngest: 'attachment_ingest',
  attachmentFromProject: 'attachment_from_project',
  attachmentList: 'attachment_list',
} as const;

async function call(command: string, args: Record<string, unknown>): Promise<unknown> {
  try {
    return await invoke<unknown>(command, args);
  } catch (error) {
    throw toStoreError(error);
  }
}

/** Konusma listesi — son aktiviteye gore azalan sirali gelir. */
export async function listConversations(): Promise<readonly ConversationSummary[]> {
  return parseConversationList(await call(CHAT_COMMANDS.conversationList, {}));
}

/**
 * Yeni metin konusmasi acar. Hafiza kapaliysa `skipped` doner; cagiran bunu
 * kullaniciya soyler ("konusma gecmisi kapali"), sessizce yutmaz.
 */
export async function startConversation(projectId?: string): Promise<ConversationStartResult> {
  return parseConversationStartResult(
    await call(CHAT_COMMANDS.start, { projectId: projectId ?? null, modality: 'text' }),
  );
}

/** Konusma basligini gunceller (ilk mesajdan otomatik ya da kullanici eliyle). */
export async function setConversationTitle(sessionId: number, title: string): Promise<void> {
  await call(CHAT_COMMANDS.setTitle, { sessionId, title });
}

/**
 * Konusmayi siler. Mevcut `session_delete` komutunu kullanir; migration 006
 * sonrasi mesajlar ve attachment'lar CASCADE ile birlikte gider.
 */
export async function deleteConversation(sessionId: number): Promise<void> {
  await call(CHAT_COMMANDS.delete, { sessionId });
}

/** Konusmanin tum mesajlari, eskiden yeniye. */
export async function listMessages(sessionId: number): Promise<readonly ChatMessage[]> {
  return parseChatMessageList(await call(CHAT_COMMANDS.messageList, { sessionId }));
}

/** Konusmanin attachment kayitlari (icerik degil, metadata). */
export async function listAttachments(sessionId: number): Promise<readonly ChatAttachment[]> {
  return parseChatAttachmentList(await call(CHAT_COMMANDS.attachmentList, { sessionId }));
}

/**
 * Kullanici mesajini gonderir, asistan yanitini bekler.
 *
 * `attachmentIds` bu konusmaya onceden `ingestAttachment`/`attachProjectFile`
 * ile eklenmis kayitlarin kimlikleridir; baska konusmanin attachment'i Rust
 * tarafinda reddedilir.
 */
export async function sendMessage(
  sessionId: number,
  text: string,
  attachmentIds: readonly number[] = [],
): Promise<ChatReply> {
  return parseChatReply(
    await call(CHAT_COMMANDS.send, { sessionId, text, attachmentIds: [...attachmentIds] }),
  );
}

/**
 * Kullanicinin sectigi dosyayi konusmaya ekler.
 *
 * Yalnizca metin dosyalari desteklenir (v1). Okuma hatasi ya da Rust'in binary
 * tespiti durust bir hataya donusur; "eklendi ama bos" durumu uretilmez.
 */
export async function ingestAttachment(sessionId: number, file: File): Promise<ChatAttachment> {
  const content = await file.text();
  return parseChatAttachment(
    await call(CHAT_COMMANDS.attachmentIngest, {
      sessionId,
      fileName: file.name,
      content,
      mimeType: file.type === '' ? null : file.type,
    }),
  );
}

/** Guncel projeden (sandbox icinden) bir dosyayi konusmaya ekler. */
export async function attachProjectFile(
  sessionId: number,
  relativePath: string,
): Promise<ChatAttachment> {
  return parseChatAttachment(
    await call(CHAT_COMMANDS.attachmentFromProject, { sessionId, relativePath }),
  );
}

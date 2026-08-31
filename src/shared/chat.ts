/**
 * Chat Shell veri tipleri + parser'lari (plan-chat-shell.md).
 *
 * # Sozlesme
 *
 * Bu dosya renderer ile Rust chat komutlari arasindaki **tek** tip sozlesmesidir.
 * Rust tarafi camelCase JSON uretir (mevcut komut deseni); buradaki parser'lar
 * IPC'den geleni dogrular. Parser'lar **bilinmeyen alanlara tolerantdir**:
 * Rust'in ileride alan eklemesi eski renderer'i kirmaz. Bilinen bir alanin
 * tipi bozuksa hata firlatilir — sessizce `undefined` tasinmaz.
 */

/** Konusma listesi satiri (`conversation_list` komutu). */
export interface ConversationSummary {
  readonly id: number;
  /** `null` = kullanici/otomatik baslik henuz yok; UI "Adsız konuşma" yazar. */
  readonly title: string | null;
  readonly modality: 'voice' | 'text';
  readonly projectId: string | null;
  readonly startedAt: string;
  /** Son mesajin zamani; mesaj yoksa `startedAt`. Siralama bu alana gore. */
  readonly lastActivityAt: string;
  readonly messageCount: number;
}

/** Tek mesaj (`message_list` / `chat_send` sonucu). */
export interface ChatMessage {
  readonly id: number;
  readonly sessionId: number;
  readonly role: 'user' | 'assistant' | 'system' | 'tool';
  readonly content: string;
  readonly createdAt: string;
}

/** Konusmaya eklenmis dosya kaydi. Icerik DB'de redakte edilmis saklanir; bu tip iceriği TASIMAZ. */
export interface ChatAttachment {
  readonly id: number;
  readonly sessionId: number;
  /** `null` = henuz bir mesaja baglanmadi (composer'da bekliyor). */
  readonly messageId: number | null;
  readonly fileName: string;
  readonly mimeType: string | null;
  readonly sizeBytes: number | null;
  readonly origin: 'upload' | 'project';
  readonly createdAt: string;
}

/** `chat_send` sonucu: kalici hale gelmis kullanici mesaji + asistan yaniti. */
export interface ChatReply {
  readonly userMessage: ChatMessage;
  readonly assistantMessage: ChatMessage;
}

/** Konusma acma sonucu — hafiza kapaliysa kayit `skipped` doner (hata degil). */
export type ConversationStartResult =
  | { readonly status: 'recorded'; readonly id: number }
  | { readonly status: 'skipped'; readonly reason: string };

// ---------------------------------------------------------------------------
// Parser'lar
// ---------------------------------------------------------------------------

function asRecord(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new TypeError(`${label}: nesne bekleniyordu`);
  }
  return value as Record<string, unknown>;
}

function readNumber(record: Record<string, unknown>, key: string, label: string): number {
  const value = record[key];
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new TypeError(`${label}.${key}: sayi bekleniyordu`);
  }
  return value;
}

function readString(record: Record<string, unknown>, key: string, label: string): string {
  const value = record[key];
  if (typeof value !== 'string') {
    throw new TypeError(`${label}.${key}: metin bekleniyordu`);
  }
  return value;
}

function readNullableString(
  record: Record<string, unknown>,
  key: string,
  label: string,
): string | null {
  const value = record[key];
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value !== 'string') {
    throw new TypeError(`${label}.${key}: metin ya da null bekleniyordu`);
  }
  return value;
}

function readNullableNumber(
  record: Record<string, unknown>,
  key: string,
  label: string,
): number | null {
  const value = record[key];
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new TypeError(`${label}.${key}: sayi ya da null bekleniyordu`);
  }
  return value;
}

function readEnum<T extends string>(
  record: Record<string, unknown>,
  key: string,
  allowed: readonly T[],
  label: string,
): T {
  const value = record[key];
  if (typeof value !== 'string' || !(allowed as readonly string[]).includes(value)) {
    throw new TypeError(`${label}.${key}: ${allowed.join('|')} bekleniyordu`);
  }
  return value as T;
}

const MODALITIES = ['voice', 'text'] as const;
const MESSAGE_ROLES = ['user', 'assistant', 'system', 'tool'] as const;
const ATTACHMENT_ORIGINS = ['upload', 'project'] as const;

export function parseConversationSummary(value: unknown): ConversationSummary {
  const record = asRecord(value, 'conversation');
  return {
    id: readNumber(record, 'id', 'conversation'),
    title: readNullableString(record, 'title', 'conversation'),
    modality: readEnum(record, 'modality', MODALITIES, 'conversation'),
    projectId: readNullableString(record, 'projectId', 'conversation'),
    startedAt: readString(record, 'startedAt', 'conversation'),
    lastActivityAt: readString(record, 'lastActivityAt', 'conversation'),
    messageCount: readNumber(record, 'messageCount', 'conversation'),
  };
}

export function parseConversationList(value: unknown): readonly ConversationSummary[] {
  if (!Array.isArray(value)) {
    throw new TypeError('conversation_list: dizi bekleniyordu');
  }
  return value.map(parseConversationSummary);
}

export function parseChatMessage(value: unknown): ChatMessage {
  const record = asRecord(value, 'message');
  return {
    id: readNumber(record, 'id', 'message'),
    sessionId: readNumber(record, 'sessionId', 'message'),
    role: readEnum(record, 'role', MESSAGE_ROLES, 'message'),
    content: readString(record, 'content', 'message'),
    createdAt: readString(record, 'createdAt', 'message'),
  };
}

export function parseChatMessageList(value: unknown): readonly ChatMessage[] {
  if (!Array.isArray(value)) {
    throw new TypeError('message_list: dizi bekleniyordu');
  }
  return value.map(parseChatMessage);
}

export function parseChatAttachment(value: unknown): ChatAttachment {
  const record = asRecord(value, 'attachment');
  return {
    id: readNumber(record, 'id', 'attachment'),
    sessionId: readNumber(record, 'sessionId', 'attachment'),
    messageId: readNullableNumber(record, 'messageId', 'attachment'),
    fileName: readString(record, 'fileName', 'attachment'),
    mimeType: readNullableString(record, 'mimeType', 'attachment'),
    sizeBytes: readNullableNumber(record, 'sizeBytes', 'attachment'),
    origin: readEnum(record, 'origin', ATTACHMENT_ORIGINS, 'attachment'),
    createdAt: readString(record, 'createdAt', 'attachment'),
  };
}

export function parseChatAttachmentList(value: unknown): readonly ChatAttachment[] {
  if (!Array.isArray(value)) {
    throw new TypeError('attachment_list: dizi bekleniyordu');
  }
  return value.map(parseChatAttachment);
}

export function parseChatReply(value: unknown): ChatReply {
  const record = asRecord(value, 'chat_send');
  return {
    userMessage: parseChatMessage(record['userMessage']),
    assistantMessage: parseChatMessage(record['assistantMessage']),
  };
}

/**
 * `session_start` yanitini konusma acilisina cevirir. Mevcut komutun yaniti
 * (`SessionWriteResult` bicimi) burada **yerel olarak** ve tolerant parse
 * edilir — `shared/session.ts` parser'ina bagimlilik bilerek yok: o dosyanin
 * strict sozlesmesi ses oturumu kaydina aittir ve buradan sikilastirilamaz.
 */
export function parseConversationStartResult(value: unknown): ConversationStartResult {
  const record = asRecord(value, 'session_start');
  const status = record['status'];
  if (status === 'recorded') {
    const session = asRecord(record['session'], 'session_start.session');
    return { status: 'recorded', id: readNumber(session, 'id', 'session_start.session') };
  }
  if (status === 'skipped') {
    return { status: 'skipped', reason: readString(record, 'reason', 'session_start') };
  }
  throw new TypeError('session_start: status recorded|skipped bekleniyordu');
}

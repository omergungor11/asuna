/**
 * Chat kabugunun metin katmani (plan-chat-shell.md — WP3).
 *
 * Bilesenlerden ayri bir modul: etiketler, gruplama kurali ve hata sozleri tek
 * yerde durur ve saf fonksiyon olduklari icin render etmeden test edilebilirler
 * (`memory-text.ts` / `project-text.ts` ile ayni desen).
 *
 * Kural degismedi: hicbir sey guzellestirilmez. Bir islem basarisiz olduysa
 * kullanici **ne oldugunu** gorur (PROJECT.md Bolum 30 — basari taklidi yok).
 */

import type { UserFacingError } from '../asuna/observability';
import type { ChatAttachment, ConversationSummary } from '../shared/chat';
import { AsunaStoreError } from '../shared/store-error';

/** Basligi olmayan **metin** konusmasinin listede gorunen adi. */
export const UNTITLED_CONVERSATION = 'Adsız konuşma';

/**
 * Basligi olmayan **ses** oturumunun listede gorunen adi.
 *
 * Ayri bir sozcuk kullanmak guvenlik meselesi: `conversation_list` ses
 * oturumlarini da dondurur ve "Adsız konuşma" etiketi kullaniciya bunun bos bir
 * metin sohbeti oldugunu dusundururdu. Silmek ise `session_delete` cagirir —
 * oturumun **ozetini** ve varsa **diskteki dokumunu** kalici siler. Etiket bu
 * yuzden turu acikca soyler (review H1).
 */
export const VOICE_CONVERSATION_FALLBACK = 'Sesli oturum';

/** Ses oturumu silinirken gosterilen uyari — kaybedilecek sey acikca yazilir. */
export const VOICE_DELETE_WARNING =
  'Bu bir ses oturumu — özeti ve varsa konuşma dökümü de kalıcı silinir. Geri alınamaz.';

/** Metin konusmasi silinirken gosterilen onay metni. */
export const TEXT_DELETE_WARNING =
  'Bu konuşma, mesajları ve dosyaları kalıcı olarak silinsin mi? Geri alınamaz.';

/** Ses oturumu acikken composer yerine gorunen aciklama. */
export const VOICE_COMPOSER_NOTE =
  'Bu bir ses oturumu; buraya metin yazılamaz. Konuşmak için Ses modunu aç.';

/** Ses oturumunun bos mesaj listesi — "ilk mesajı yaz" demek yaniltici olurdu. */
export const VOICE_EMPTY_STATE =
  'Bu bir ses oturumu. Metin mesajı içermez; özeti Hafıza ekranında görünür.';

/** Silme onayinin metni — ses oturumunda kaybedilecek sey farkli. */
export function describeDeleteConfirmation(conversation: ConversationSummary): string {
  return conversation.modality === 'voice' ? VOICE_DELETE_WARNING : TEXT_DELETE_WARNING;
}

/**
 * Otomatik baslik ust siniri (plan: "ilk 60 karakter").
 *
 * Sinir **karakter** cinsinden ve kirpma sessiz: baslik zaten ozet niyetine
 * yazilmiyor, kullanici tam metni konusmanin icinde goruyor.
 */
export const CONVERSATION_TITLE_MAX_CHARS = 60;

/** Hafiza kapaliyken konusma acilamaz — sahte/gecici konusma uydurulmaz. */
export const MEMORY_DISABLED_NOTICE =
  'Konuşma geçmişi (hafıza) kapalı — Ayarlar’dan açın. Metin sohbeti kalıcı ' +
  'kayıt olmadan başlatılmaz.';

/**
 * Listede gorunen ad.
 *
 * Baslik yoksa **turu** soyleyen bir yedege duser: ses oturumu ile bos metin
 * konusmasi ayni gorunmemeli (review H1).
 */
export function conversationTitleOf(conversation: ConversationSummary): string {
  const title = conversation.title;
  if (title !== null && title.trim() !== '') {
    return title;
  }
  return conversation.modality === 'voice'
    ? VOICE_CONVERSATION_FALLBACK
    : UNTITLED_CONVERSATION;
}

/**
 * Ilk kullanici mesajindan baslik turetir.
 *
 * Satir sonlari ve tekrarli bosluklar tek bosluga indirilir: baslik tek satirlik
 * bir liste etiketi, mesajin kopyasi degil.
 */
export function deriveConversationTitle(text: string): string {
  const flattened = text.replace(/\s+/gu, ' ').trim();
  return flattened.slice(0, CONVERSATION_TITLE_MAX_CHARS).trim();
}

// ---------------------------------------------------------------------------
// Tarih gruplari
// ---------------------------------------------------------------------------

export const CONVERSATION_GROUP_IDS = ['today', 'yesterday', 'week', 'older'] as const;

export type ConversationGroupId = (typeof CONVERSATION_GROUP_IDS)[number];

/**
 * `Record<ConversationGroupId, string>`: yeni bir grup eklenirse etiketi yazmayi
 * unutmak derleme hatasi olur, calisma aninda bos baslik degil.
 */
export const CONVERSATION_GROUP_LABELS: Readonly<Record<ConversationGroupId, string>> = {
  today: 'Bugün',
  yesterday: 'Dün',
  week: 'Son 7 gün',
  older: 'Daha eski',
};

export interface ConversationGroup {
  readonly id: ConversationGroupId;
  readonly label: string;
  readonly conversations: readonly ConversationSummary[];
}

function startOfLocalDay(value: Date): number {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
}

const DAY_MS = 86_400_000;

/**
 * Bir zaman damgasinin hangi gruba dustugu.
 *
 * Karsilastirma **yerel gun** sinirinda yapilir: gece 23:50'de yazilan mesaj
 * ertesi sabah "Bugün" degil "Dün" olmali. Cozumlenemeyen deger `older`'a
 * duser — uydurma bir tarih hesaplamaktansa listenin sonunda durmasi durust.
 */
export function conversationGroupOf(timestamp: string, now: Date): ConversationGroupId {
  const parsed = new Date(timestamp);
  if (Number.isNaN(parsed.getTime())) {
    return 'older';
  }

  const days = Math.round((startOfLocalDay(now) - startOfLocalDay(parsed)) / DAY_MS);
  if (days <= 0) {
    return 'today';
  }
  if (days === 1) {
    return 'yesterday';
  }
  return days < 7 ? 'week' : 'older';
}

/**
 * Konusmalari tarih gruplarina ayirir.
 *
 * Grup **icindeki** sira degistirilmez: liste zaten `lastActivityAt` azalan
 * sirada geliyor (`conversation_list` sozlesmesi), UI onu yeniden siralayip
 * backend ile celismez. Bos gruplar dusurulur.
 */
export function groupConversations(
  conversations: readonly ConversationSummary[],
  now: Date = new Date(),
): readonly ConversationGroup[] {
  const buckets = new Map<ConversationGroupId, ConversationSummary[]>();

  for (const conversation of conversations) {
    const id = conversationGroupOf(conversation.lastActivityAt, now);
    const bucket = buckets.get(id);
    if (bucket === undefined) {
      buckets.set(id, [conversation]);
    } else {
      bucket.push(conversation);
    }
  }

  const groups: ConversationGroup[] = [];
  for (const id of CONVERSATION_GROUP_IDS) {
    const bucket = buckets.get(id);
    if (bucket !== undefined && bucket.length > 0) {
      groups.push({ id, label: CONVERSATION_GROUP_LABELS[id], conversations: bucket });
    }
  }
  return groups;
}

// ---------------------------------------------------------------------------
// Attachment
// ---------------------------------------------------------------------------

/** Insan diliyle boyut; bilinmiyorsa `null` — "0 B" uydurulmaz. */
export function describeAttachmentSize(bytes: number | null): string | null {
  if (bytes === null || !Number.isFinite(bytes) || bytes < 0) {
    return null;
  }
  return bytes < 1024
    ? `${bytes.toString()} B`
    : `${(bytes / 1024).toFixed(1).replace('.0', '')} KB`;
}

/** Cip etiketi: dosya adi + varsa boyut. */
export function describeAttachment(attachment: ChatAttachment): string {
  const size = describeAttachmentSize(attachment.sizeBytes);
  return size === null ? attachment.fileName : `${attachment.fileName} · ${size}`;
}

// ---------------------------------------------------------------------------
// Hatalar
// ---------------------------------------------------------------------------

/**
 * Servis hatasini kullaniciya gosterilecek cumleye cevirir.
 *
 * Orijinal mesaj her zaman korunur — kod yalnizca baglam ekler.
 */
export function describeChatError(error: unknown): string {
  if (error instanceof AsunaStoreError) {
    switch (error.code) {
      case 'unavailable':
        return `Konuşma kaydı kullanılamıyor: ${error.message}`;
      case 'not-found':
        return `Kayıt bulunamadı — liste güncel olmayabilir. (${error.message})`;
      case 'invalid':
        return `İstek reddedildi: ${error.message}`;
      case 'storage':
        return `Depolama hatası: ${error.message}`;
      case 'unknown':
        return error.message;
    }
  }

  if (error instanceof Error) {
    return error.message;
  }

  return 'Konuşma işlemi bilinmeyen bir nedenle başarısız oldu.';
}

/**
 * Hatayi `ErrorNotice`'in bekledigi bicime cevirir.
 *
 * Deponun kapali/bozuk olmasi ayri bir etiket aliyor: "tekrar dene" demek
 * yaniltici olurdu, kullanicinin gidecegi yer Ayarlar.
 */
export function chatErrorNotice(error: unknown): UserFacingError {
  const message = describeChatError(error);

  if (error instanceof AsunaStoreError && error.code === 'unavailable') {
    return {
      kind: 'memory_unavailable',
      message,
      action: 'Ayarlar > Kalıcı hafıza anahtarını kontrol et.',
      retryable: false,
    };
  }

  return { kind: 'unknown', message, action: null, retryable: true };
}

/**
 * Oturum gecmisi bolumunun metin katmani (ASU-065).
 *
 * `memory-text.ts` ile ayni gerekce: etiketler ve durum cumleleri tek yerde
 * durur, saf fonksiyon olduklari icin bilesen render etmeden test edilebilirler.
 * Ayri bir dosya olmasinin nedeni kapsam: bunlar `sessions` sozlesmesine bagli,
 * `memories`e degil.
 *
 * Kural: hicbir sey guzellestirilmez. Bir dosya silinemediyse ya da dosyaya
 * **dokunulmadiysa** kullanici bunu oldugu gibi gorur (PROJECT.md Bolum 30 —
 * basari taklidi yok).
 */

import type {
  SessionEndReason,
  SessionListItem,
  TranscriptFileOutcome,
} from '../shared/session';

import { formatMemoryTimestamp } from './memory-text';

/**
 * Kapanis nedeni etiketleri.
 *
 * `Record<SessionEndReason, string>`: semaya yeni bir deger eklenirse
 * (`src/shared/session.ts` aynasi) burasi derleme hatasi verir — etiket
 * sessizce eksik kalmaz.
 */
export const SESSION_END_REASON_LABELS: Readonly<Record<SessionEndReason, string>> = {
  completed: 'temiz kapandı',
  abandoned: 'yarım kaldı',
  error: 'hata ile bitti',
};

/**
 * Silme sonrasi dokum dosyasina ne oldugu.
 *
 * Bes durumun bes ayri cumlesi var: "sildim" demek yalnizca dosya gercekten
 * gittiginde dogru. Ozellikle `refused` gizlenmez — kayitli yol Asuna'nin
 * klasorunun disina cikiyorsa (bozuk/elle duzenlenmis bir kayit) kullanici
 * bunu bilmeli.
 */
export const TRANSCRIPT_OUTCOME_TEXT: Readonly<Record<TranscriptFileOutcome, string>> = {
  'not-recorded': 'Oturum kaydı ve özeti silindi (diskte döküm dosyası yoktu).',
  deleted: 'Oturum kaydı, özeti ve konuşma dökümü dosyası silindi.',
  'already-gone': 'Oturum kaydı ve özeti silindi; döküm dosyası diskte zaten yoktu.',
  refused:
    'Oturum kaydı ve özeti silindi. Döküm dosyasına DOKUNULMADI: kayıtlı yol Asuna’nın ' +
    'döküm klasörünün dışına çıkıyor.',
  failed: 'Oturum kaydı ve özeti silindi, ama döküm dosyası silinemedi (dosya sistemi hatası).',
};

/**
 * Bir oturumun tek satirlik zaman/durum ozeti.
 *
 * Sure **hesaplanmaz**: yarim kalan oturumlarda `endedAt = startedAt` yazilir
 * (gercek bitis bilinmiyor, ASU-032) ve oradan sure uretmek "0 saniye surdu"
 * yalanini dogururdu. Bilinmeyen durum da gizlenmez.
 */
export function describeSessionTiming(item: SessionListItem): string {
  const started = formatMemoryTimestamp(item.startedAt);
  if (item.endedAt === null) {
    return `${started} · sürüyor`;
  }
  return item.endReason === null
    ? `${started} · durum bilinmiyor`
    : `${started} · ${SESSION_END_REASON_LABELS[item.endReason]}`;
}

/**
 * Memory UI'in metin katmani (ASU-036).
 *
 * Bilesenlerden ayri bir modul: etiketler ve hata sozleri tek yerde durur,
 * saf fonksiyon olduklari icin bilesen render etmeden test edilebilirler.
 *
 * Kural: hicbir sey guzellestirilmez. Bir hata olduysa kullanici **ne oldugunu**
 * ve **kimin sucu** oldugunu gorur (PROJECT.md Bolum 30 — basari taklidi yok).
 */

import type { MemoryKind } from '../shared/memory';
import { AsunaStoreError } from '../shared/store-error';

/**
 * `MemoryKind` rozet etiketleri.
 *
 * `Record<MemoryKind, string>` oldugu icin semaya yeni bir `kind` eklendiginde
 * (ayna: `src/shared/memory.ts`) burasi derleme hatasi verir — rozet sessizce
 * eksik kalmaz.
 */
export const MEMORY_KIND_LABELS: Readonly<Record<MemoryKind, string>> = {
  profile: 'Profil',
  preference: 'Tercih',
  project: 'Proje',
  decision: 'Karar',
  task: 'Görev',
  working_context: 'Çalışma bağlamı',
  relationship: 'İlişki',
  idea: 'Fikir',
  routine: 'Rutin',
  tool_state: 'Araç durumu',
};

/**
 * Zaman damgasini yerel saatte `YYYY-AA-GG SS:DD` olarak yazar.
 *
 * `toLocaleString` bilerek kullanilmiyor: cikti makinenin diline gore degisir,
 * ayni ekran iki makinede iki turlu okunur. Cozumlenemeyen deger **oldugu gibi**
 * gosterilir; uydurma tarih basmaktansa ham metin durustur.
 */
export function formatMemoryTimestamp(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  const pad = (part: number): string => part.toString().padStart(2, '0');

  return (
    `${parsed.getFullYear().toString()}-${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())}` +
    ` ${pad(parsed.getHours())}:${pad(parsed.getMinutes())}`
  );
}

/**
 * "Bu neden hatirlaniyor?" sorusunun UI cevabi (memory.md Bolum 2).
 *
 * Kaynak oturum bilinmiyorsa bu **gizlenmez**: kullanici kaydin nereden geldigini
 * bilmediginin de farkinda olmali.
 */
export function describeMemorySource(sourceSessionId: number | null): string {
  return sourceSessionId === null
    ? 'Kaynak oturum bilinmiyor'
    : `Oturum #${sourceSessionId.toString()}`;
}

/**
 * Servis hatasini kullaniciya gosterilecek cumleye cevirir.
 *
 * Orijinal mesaj her zaman korunur — kod yalnizca baglam ekler. "Bir seyler ters
 * gitti" turu bos mesajlar yok.
 */
export function describeMemoryError(error: unknown): string {
  if (error instanceof AsunaStoreError) {
    switch (error.code) {
      case 'unavailable':
        return `Hafıza kullanılamıyor: ${error.message}`;
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

  return 'Hafıza işlemi bilinmeyen bir nedenle başarısız oldu.';
}

/**
 * Tool katmaninin metin yuzeyi (ASU-053 / ASU-054).
 *
 * `memory-text.ts` ve `session-text.ts` ile ayni gerekce: etiketler ve durum
 * cumleleri tek yerde durur, saf fonksiyon olduklari icin bilesen render
 * etmeden test edilebilirler. Ayri bir dosya olmasinin nedeni kapsam: bunlar
 * `tools` / `tool_events` sozlesmelerine bagli.
 *
 * Kural: hicbir sey guzellestirilmez. Reddedilen bir cagri "yapilamadi" degil
 * **reddedildi** der; zaman asimina ugrayan cagri "reddedildi" der, cunku
 * varsayilan karar reddir (ASU-048). Basari taklidi yok (PROJECT.md Bolum 30).
 */

import type { ToolApprovalState, ToolOutcome, ToolRiskLevel } from '../shared/tool-event';
import type { ToolApprovalPolicy } from '../shared/tools';
import { AsunaStoreError } from '../shared/store-error';

/**
 * Risk seviyelerinin insan dili karsiligi (PROJECT.md Bolum 5.4).
 *
 * `Record<ToolRiskLevel, string>`: sema bir seviye eklerse burasi derleme
 * hatasi verir — ekranda etiketsiz bir sayi kalmaz.
 */
export const TOOL_RISK_LABELS: Readonly<Record<ToolRiskLevel, string>> = {
  0: 'Risk 0 · salt okuma',
  1: 'Risk 1 · geri alınabilir',
  2: 'Risk 2 · değişiklik yapar',
  3: 'Risk 3 · geri alınamaz / dış etki',
};

/**
 * Risk seviyesi bilinmiyorsa bu **gizlenmez**.
 *
 * Kayitli olmayan bir tool icin onay istegi geldiyse (`risk: null`) kullanici
 * bunu gormeli: "bilmiyorum" da bir cevaptir ve sessizce "Risk 0" yazmak
 * yalan olurdu.
 */
export function describeToolRisk(risk: ToolRiskLevel | null): string {
  return risk === null ? 'Risk bilinmiyor' : TOOL_RISK_LABELS[risk];
}

/** Kart/liste renk kodu icin `data-risk` degeri. */
export function riskAttribute(risk: ToolRiskLevel | null): string {
  return risk === null ? 'unknown' : risk.toString();
}

/** Onay politikasinin insan dili karsiligi (ASU-048 matrisi). */
export const TOOL_APPROVAL_POLICY_LABELS: Readonly<Record<ToolApprovalPolicy, string>> = {
  not_required: 'Onaysız çalışır',
  always: 'Her seferinde onay',
};

/**
 * Audit defterindeki onay durumlari.
 *
 * `not_required` ile `not_requested` bilerek farkli cumleler: birinde onay
 * GEREKMEDI, otekinde onay SORULAMADI (ASU-050).
 */
export const TOOL_APPROVAL_STATE_LABELS: Readonly<Record<ToolApprovalState, string>> = {
  not_required: 'Onay gerekmedi',
  auto_approved: 'Ayar otomatik onayladı',
  approved: 'Onaylandı',
  denied: 'Reddedildi',
  timeout: 'Süre doldu — reddedildi',
  not_requested: 'Onay sorulmadı',
};

/**
 * Sonuc etiketleri — `TOOL_OUTCOMES` (sema aynasi) ile ayni kume.
 *
 * `Record<ToolOutcome, string>`: semaya yeni bir sonuc eklenirse burasi
 * derleme hatasi verir. `failed` ile `not_run` bilerek farkli kelimeler:
 * biri "calisti ama olmadi", oteki "hic calismadi" — yan etki ihtimali
 * bakimindan ayni sey degiller.
 */
export const TOOL_OUTCOME_LABELS: Readonly<Record<ToolOutcome, string>> = {
  succeeded: 'başarılı',
  failed: 'hata',
  not_run: 'çalışmadı',
};

/**
 * Onay penceresinin kalan suresi.
 *
 * `ceil`: 0.4 sn kalmisken "0 sn" yazmak, henuz reddedilmemis bir istegi
 * bitmis gostermek olurdu. Negatif deger 0'a kirpilir — UI zaman asimini
 * **tetiklemez**, yalnizca gosterir (otomatik reddetme serviste, ASU-048).
 */
export function formatApprovalCountdown(remainingMs: number): string {
  const seconds = Math.max(0, Math.ceil(remainingMs / 1000));
  return `${seconds.toString()} sn`;
}

/**
 * Audit/tool servis hatasini kullaniciya gosterilecek cumleye cevirir.
 *
 * `describeMemoryError` ile ayni desen: orijinal mesaj korunur, kod yalnizca
 * baglam ekler. "Bir seyler ters gitti" turu bos mesaj yok — kullanici denetim
 * defterine **neden** bakamadigini bilmeli.
 */
export function describeToolError(error: unknown): string {
  if (error instanceof AsunaStoreError) {
    switch (error.code) {
      case 'unavailable':
        return `Araç geçmişi kullanılamıyor: ${error.message}`;
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

  return 'Araç geçmişi bilinmeyen bir nedenle okunamadı.';
}

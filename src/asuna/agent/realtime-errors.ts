/**
 * Realtime katmaninin hata siniflandirmasi ve redaksiyonu (ASU-013).
 *
 * Iki kaynak var:
 * 1. Rust IPC (`mint_realtime_token`) — `{ kind, message }` seklinde **zaten durust ve
 *    redakte edilmis** bir hata doner (`src-tauri/src/realtime_token.rs`).
 * 2. OpenAI Agents SDK / tarayici — tipsiz `unknown`.
 *
 * Ikisi de disariya tek bir [`AsunaRealtimeErrorInfo`] olarak cikar. "Bir seyler ters
 * gitti" tek kovasi yok: her varyantin ayirt edilebilir bir `kind`'i ve kullaniciya
 * soylenebilecek somut bir mesaji var (PROJECT.md Bolum 30).
 */

/** Hatanin hangi asamada olustugu — UI ve ASU-019 log'u bunun uzerine switch yazar. */
export const ASUNA_REALTIME_ERROR_KINDS = [
  /** Ephemeral token uretilemedi (Rust komutu hata dondu ya da yanit bozuk). */
  'token',
  /** Ortam Realtime oturumunu tasiyamiyor (WebRTC yok, mikrofon yok). */
  'unsupported',
  /** `connect()` basarisiz — ag, SDP, yetkilendirme. */
  'transport',
  /** Oturum acildi ama SDK bir `error` event'i yayinladi. */
  'session',
  /** Asuna tarafindaki sozlesme ihlali (beklenmeyen durum, yanlis kullanim). */
  'internal',
] as const;

export type AsunaRealtimeErrorKind = (typeof ASUNA_REALTIME_ERROR_KINDS)[number];

export interface AsunaRealtimeErrorInfo {
  readonly kind: AsunaRealtimeErrorKind;
  /**
   * Alt katmanin makine-okunur etiketi — Rust `RealtimeTokenError::kind()`
   * (`invalid_api_key`, `network`, ...) ya da SDK hata sinifinin adi. Yoksa `null`.
   */
  readonly cause: string | null;
  /** Kullaniciya gosterilebilecek, redakte edilmis, somut mesaj. */
  readonly message: string;
  /** Yeniden baglanma denemesi anlamli mi (ag/gecici sorun) yoksa bosuna mi (yanlis key). */
  readonly retryable: boolean;
}

/**
 * Cagiran tarafa firlatilan hata. Bilgi `info` alaninda yapisal olarak durur;
 * `message` sadece okunabilirlik icin ayni metni tasir.
 */
export class AsunaRealtimeError extends Error {
  public override readonly name = 'AsunaRealtimeError';

  public constructor(public readonly info: AsunaRealtimeErrorInfo) {
    super(info.message);
  }
}

// ---------------------------------------------------------------------------
// Redaksiyon
// ---------------------------------------------------------------------------

/**
 * `sk-...` / `ek_...` gorunumlu her parcayi maskeler.
 *
 * Rust tarafi kendi mesajlarini zaten redakte ediyor; bu, renderer'da **son savunma
 * hatti**: SDK ya da tarayici kaynakli bir hata metni ephemeral token'i icerebilir ve
 * bu metin UI'a/log'a dusecek.
 *
 * Rust `redact_secrets` ile ayni kural: token karakteri (harf, rakam, `-`, `_`) disi
 * her sey sinir sayilir.
 */
export function redactSecrets(input: string): string {
  return input.replace(/[A-Za-z0-9_-]+/g, (word) => {
    if (word.startsWith('sk-')) {
      return 'sk-<redacted>';
    }
    if (word.startsWith('ek_')) {
      return 'ek_<redacted>';
    }
    return word;
  });
}

// ---------------------------------------------------------------------------
// Rust IPC hatasi
// ---------------------------------------------------------------------------

/** `mint_realtime_token` hatasinin IPC bicimi: `{ kind, message }`. */
export interface RealtimeTokenIpcError {
  readonly kind: string;
  readonly message: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** IPC hatasini tanir. Tip *iddia* edilmez, dogrulanir. */
export function parseRealtimeTokenIpcError(value: unknown): RealtimeTokenIpcError | null {
  if (!isRecord(value)) {
    return null;
  }
  const { kind, message } = value;
  if (typeof kind !== 'string' || kind.length === 0) {
    return null;
  }
  if (typeof message !== 'string' || message.length === 0) {
    return null;
  }
  return { kind, message };
}

/**
 * Token hatasinin yeniden denemeye deger olup olmadigi.
 *
 * Yanlis API anahtari ya da erisilemeyen model 3 kez denemekle duzelmez — kullaniciyi
 * bekletmek yerine hemen durust cevabi vermek daha iyi (sonsuz retry yasagi).
 */
const RETRYABLE_TOKEN_ERROR_KINDS: readonly string[] = ['network', 'upstream_unavailable'];

/**
 * Ephemeral token asamasindaki hatayi normalize eder.
 *
 * Rust mesaji **oldugu gibi** korunur: zaten Turkce, somut ve redakte. Taninmayan bir
 * hata icin genel ama durust bir mesaj uretilir; "bir seyler ters gitti" denmez.
 */
export function describeTokenError(error: unknown): AsunaRealtimeErrorInfo {
  const ipc = parseRealtimeTokenIpcError(error);
  if (ipc !== null) {
    return {
      kind: 'token',
      cause: ipc.kind,
      message: redactSecrets(ipc.message),
      retryable: RETRYABLE_TOKEN_ERROR_KINDS.includes(ipc.kind),
    };
  }

  return {
    kind: 'token',
    cause: causeLabel(error),
    message:
      'Ses oturumu icin gecici anahtar alinamadi. Asuna arka planiyla konusulamiyor; ' +
      `ayrinti: ${describeUnknown(error)}`,
    retryable: false,
  };
}

// ---------------------------------------------------------------------------
// SDK / tarayici hatasi
// ---------------------------------------------------------------------------

/**
 * SDK'nin `ek_` guard'i (voice.md Bolum 4). Bu mesaj gorunuyorsa renderer'a kalici bir
 * API anahtari verilmis demektir — yeniden denemek degil, kodu duzeltmek gerekir.
 */
const EPHEMERAL_GUARD_MARKER = 'ephemeral client key';

/** WebRTC olmayan bir ortamda `RealtimeSession` kurucusu bu mesajla patlar. */
const WEBRTC_UNSUPPORTED_MARKER = 'webrtc is not supported';

function causeLabel(error: unknown): string | null {
  if (error instanceof Error && error.name.length > 0) {
    return error.name;
  }
  return null;
}

function describeUnknown(error: unknown): string {
  if (error instanceof Error) {
    // Stack degil, yalnizca mesaj: ic dosya yollari kullaniciya/log'a sizmasin
    // (`conventions.md` — "Hata mesajlari ic detay sizdirmaz").
    return redactSecrets(error.message);
  }
  if (typeof error === 'string') {
    return redactSecrets(error);
  }
  return 'aciklama uretilemedi (hata nesnesi taninmiyor)';
}

/** `connect()` sirasinda olusan hatayi normalize eder. */
export function describeConnectError(error: unknown): AsunaRealtimeErrorInfo {
  const detail = describeUnknown(error);
  const lowered = detail.toLowerCase();

  if (lowered.includes(WEBRTC_UNSUPPORTED_MARKER)) {
    return {
      kind: 'unsupported',
      cause: causeLabel(error),
      message:
        'Bu ortamda WebRTC yok, sesli oturum acilamiyor. Asuna masaustu uygulamasi ' +
        'uzerinden calistirilmali.',
      retryable: false,
    };
  }

  if (lowered.includes(EPHEMERAL_GUARD_MARKER)) {
    return {
      kind: 'internal',
      cause: causeLabel(error),
      message:
        'Realtime oturumu kisa omurlu bir anahtar bekliyordu ama farkli bir anahtar ' +
        'verildi. Bu bir yapilandirma hatasi; yeniden denemek duzeltmez.',
      retryable: false,
    };
  }

  return {
    kind: 'transport',
    cause: causeLabel(error),
    message: `Realtime oturumu acilamadi: ${detail}`,
    retryable: true,
  };
}

/** Oturum acikken gelen SDK `error` event'ini normalize eder. */
export function describeSessionError(error: unknown): AsunaRealtimeErrorInfo {
  return {
    kind: 'session',
    cause: causeLabel(error),
    message: `Ses oturumunda hata olustu: ${describeUnknown(error)}`,
    // Oturum ici hatalarda otomatik yeniden baglanma yapilmiyor (asagida ASU-013
    // tasarim notu); bu bayrak yalnizca cagiran taraf icin bilgi.
    retryable: false,
  };
}

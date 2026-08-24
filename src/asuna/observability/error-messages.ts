/**
 * Makine-okunur hata etiketlerinden **durust** Turkce kullanici mesajlarina harita (ASU-019).
 *
 * Kaynak: `PROJECT.md` Bolum 30 ("Asuna must fail gracefully", "Do not pretend
 * success", ornek: "Su an ses baglantisini kuramadim. Yerel moddayim."),
 * `asuna-config/conventions.md` — "Hata Yonetimi".
 *
 * Tasarim kararlari:
 * - **Tek "bir seyler ters gitti" kovasi yok.** Rust tarafi (`realtime_token.rs`)
 *   hatayi zaten dokuz ayri `kind` etiketine ayiriyor; burada her etiketin ayri
 *   bir cumlesi var. Bilinmeyen etiket icin bile mesaj durust: "ne oldugunu
 *   cozemedim" der, "tamamdir" demez.
 * - **Ic detay sizmaz.** Upstream mesaj/stack/URL kullaniciya tasinmaz; bu modul
 *   yalnizca `kind` etiketini okur, mesajin govdesini kendisi uretir. Ham hata
 *   `logger` uzerinden (redakte edilerek) ayrica loglanir — kullaniciya gosterilen
 *   ile log'a giden ayridir.
 * - **`retryable` bir UI sozlesmesidir.** `ERROR` durumundan tekrar baglanma yolu
 *   (ASU-019 kabul kriteri) bu bayraga bakar; anahtar hatalarinda tekrar denemek
 *   ayni duvara carpar, o yuzden `false`.
 */

/** `src-tauri/src/realtime_token.rs` -> `RealtimeTokenError::kind()` ile birebir ayni kume. */
export const REALTIME_TOKEN_ERROR_KINDS = [
  'missing_api_key',
  'invalid_api_key',
  'model_access_denied',
  'quota_exceeded',
  'network',
  'upstream_unavailable',
  'unexpected_status',
  'malformed_response',
  'http_client_unavailable',
] as const;

export type RealtimeTokenErrorKind = (typeof REALTIME_TOKEN_ERROR_KINDS)[number];

/** Renderer tarafi servislerinin uretebilecegi hatalar (mikrofon, oturum, config, memory, tool). */
export const ASUNA_SERVICE_ERROR_KINDS = [
  'mic_permission_denied',
  'mic_unavailable',
  'realtime_connect_failed',
  'realtime_disconnected',
  'config_unavailable',
  'memory_unavailable',
  'tool_failed',
  'unknown',
] as const;

export type AsunaServiceErrorKind = (typeof ASUNA_SERVICE_ERROR_KINDS)[number];

export const ASUNA_ERROR_KINDS = [
  ...REALTIME_TOKEN_ERROR_KINDS,
  ...ASUNA_SERVICE_ERROR_KINDS,
] as const;

export type AsunaErrorKind = RealtimeTokenErrorKind | AsunaServiceErrorKind;

/** Etiketi cozulemeyen her sey buraya duser. */
export const UNKNOWN_ERROR_KIND: AsunaErrorKind = 'unknown';

/** Kullaniciya gosterilecek/soylenecek hata. */
export interface UserFacingError {
  readonly kind: AsunaErrorKind;
  /** Ne oldugunu soyleyen ana cumle — Asuna'nin agzindan, basari taklidi yok. */
  readonly message: string;
  /** Kullanicinin atabilecegi somut adim; yoksa `null`. */
  readonly action: string | null;
  /** Ayni islemi tekrar denemek anlamli mi (`ERROR` durumundan cikis yolu). */
  readonly retryable: boolean;
}

type UserFacingErrorTemplate = Omit<UserFacingError, 'kind'>;

/**
 * `Record<AsunaErrorKind, ...>` bilincli bir secim: yeni bir `kind` eklendiginde
 * mesaji yazmayi unutmak **derleme hatasi** olur, calisma aninda bos mesaj degil.
 */
const MESSAGES: Readonly<Record<AsunaErrorKind, UserFacingErrorTemplate>> = {
  // --- Ephemeral token uretimi (Rust) ---
  missing_api_key: {
    message: 'Şu an ses bağlantısını kuramadım: OpenAI API anahtarı tanımlı değil.',
    action: '`.env` dosyasındaki `OPENAI_API_KEY` değerini doldurup Asuna’yı yeniden başlat.',
    retryable: false,
  },
  invalid_api_key: {
    message: 'Şu an ses bağlantısını kuramadım: OpenAI API anahtarı reddedildi.',
    action: 'Anahtarı yenileyip `.env` dosyasını güncelle, sonra Asuna’yı yeniden başlat.',
    retryable: false,
  },
  model_access_denied: {
    message: 'Şu an ses bağlantısını kuramadım: bu hesabın seçili ses modeline erişimi yok.',
    action:
      'OpenAI panelinden model erişimini kontrol et ya da `ASUNA_REALTIME_MODEL` değerini değiştir.',
    retryable: false,
  },
  quota_exceeded: {
    message: 'Şu an ses bağlantısını kuramadım: OpenAI kota sınırına takıldım.',
    action: 'Faturalandırma ve kullanım limitlerini kontrol et, sonra tekrar dene.',
    retryable: true,
  },
  network: {
    message: 'Şu an ses bağlantısını kuramadım: OpenAI’ya ulaşamıyorum.',
    action: 'İnternet bağlantını kontrol edip tekrar dene.',
    retryable: true,
  },
  upstream_unavailable: {
    message: 'Şu an ses bağlantısını kuramadım: OpenAI ses servisi yanıt vermiyor.',
    action: 'Birazdan tekrar dene.',
    retryable: true,
  },
  unexpected_status: {
    message: 'Şu an ses bağlantısını kuramadım: OpenAI beklenmedik bir yanıt döndü.',
    action: 'Tekrar dene; sürerse log panelindeki son satırlara bak.',
    retryable: true,
  },
  malformed_response: {
    message: 'Şu an ses bağlantısını kuramadım: oturum yanıtını okuyamadım.',
    action: 'Tekrar dene; sürerse log panelindeki son satırlara bak.',
    retryable: true,
  },
  http_client_unavailable: {
    message: 'Şu an ses bağlantısını kuramadım: güvenli HTTPS istemcisini kuramadım.',
    action: 'Sistem TLS/sertifika yapılandırmanı kontrol et.',
    retryable: false,
  },

  // --- Renderer servisleri ---
  mic_permission_denied: {
    message: 'Mikrofona erişemiyorum: izin verilmemiş. Seni duyamıyorum.',
    action:
      'Sistem Ayarları > Gizlilik ve Güvenlik > Mikrofon altında Asuna’ya izin ver, sonra tekrar dene.',
    retryable: true,
  },
  mic_unavailable: {
    message: 'Mikrofonu açamadım: kullanılabilir bir giriş aygıtı bulamadım.',
    action:
      'Mikrofonun bağlı olduğunu ve başka bir uygulama tarafından kullanılmadığını kontrol et.',
    retryable: true,
  },
  realtime_connect_failed: {
    message: 'Şu an ses bağlantısını kuramadım.',
    action: 'Tekrar bağlanmayı dene.',
    retryable: true,
  },
  realtime_disconnected: {
    message: 'Ses bağlantım koptu, şu an seni duymuyorum.',
    action: 'Tekrar bağlanmayı dene.',
    retryable: true,
  },
  config_unavailable: {
    message: 'Ayarlarımı okuyamadım, bu yüzden ses oturumunu başlatmıyorum.',
    action: '`.env` dosyasını kontrol edip Asuna’yı yeniden başlat.',
    retryable: true,
  },
  memory_unavailable: {
    message:
      'Hafızama ulaşamıyorum; konuşmaya devam edebilirim ama bu oturumu hatırlamayacağım.',
    action: null,
    retryable: true,
  },
  tool_failed: {
    message: 'Denedim ama işlem tamamlanmadı; yapmış gibi davranmayacağım.',
    action: 'Log panelindeki son satırlarda hatanın ayrıntısı var.',
    retryable: true,
  },
  unknown: {
    message: 'Beklenmedik bir hata oldu ve ne olduğunu tam olarak çözemedim.',
    action: 'Tekrar dene; sürerse log panelindeki son satırlara bak.',
    retryable: true,
  },
};

const KNOWN_KINDS: ReadonlySet<string> = new Set<string>(ASUNA_ERROR_KINDS);

export function isAsunaErrorKind(value: string): value is AsunaErrorKind {
  return KNOWN_KINDS.has(value);
}

/** Etiketten kullanici mesajini uretir. Bilinmeyen etiket jenerik (ama durust) mesaja duser. */
export function userFacingErrorFor(kind: string): UserFacingError {
  const resolved: AsunaErrorKind = isAsunaErrorKind(kind) ? kind : UNKNOWN_ERROR_KIND;
  return { kind: resolved, ...MESSAGES[resolved] };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Ham hatadan etiketi cikarir.
 *
 * Taninan bicimler: Rust IPC payload'i `{ kind, message }` ve dogrudan etiket
 * string'i. Baska hicbir sey **tahmin edilmez** — mesaj metnine bakarak etiket
 * uydurmak kirilgan ve yaniltici olur.
 */
export function errorKindOf(error: unknown): AsunaErrorKind {
  if (typeof error === 'string' && isAsunaErrorKind(error)) {
    return error;
  }
  if (isRecord(error)) {
    const kind = error['kind'];
    if (typeof kind === 'string' && isAsunaErrorKind(kind)) {
      return kind;
    }
  }
  return UNKNOWN_ERROR_KIND;
}

/** Ham hatayi (IPC payload'i, string, bilinmeyen) kullanici mesajina cevirir. */
export function toUserFacingError(error: unknown): UserFacingError {
  return userFacingErrorFor(errorKindOf(error));
}

/** Tek satirlik gosterim: mesaj + varsa somut adim. */
export function describeUserFacingError(error: UserFacingError): string {
  return error.action === null ? error.message : `${error.message} ${error.action}`;
}

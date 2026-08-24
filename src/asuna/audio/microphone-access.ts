/**
 * Mikrofon izin kapisi (ASU-015 / ASU-016).
 *
 * # Neden ayri bir izin cagrisi var?
 *
 * Realtime oturumunda mikrofonu **SDK kendisi acar** (voice.md Bolum 4, "Secenek A"):
 * `mediaStream` verilmez, boylece `session.close()` track'leri de durdurur ve macOS
 * mikrofon gostergesi soner (ASU-018). Ama o zaman izin promptu tam WebRTC el sikismasinin
 * ortasinda cikar ve kullanici reddederse hata `connect()` icinden, tanimasi zor bir
 * bicimde doner.
 *
 * Bu yuzden baglanmadan **once** kisa omurlu bir sonda (probe) acilir: izin tetiklenir,
 * cihazin gercekten uygulanan ayarlari okunur ve track'ler **hemen** durdurulur. Sonda
 * stream'i hicbir yere verilmez; SDK kendi stream'ini acar.
 *
 * # Echo cancellation
 *
 * `echoCancellation` / `noiseSuppression` acikca isteniyor (phase-1.md ASU-016 notu:
 * self-interrupt bu asamanin en yaygin tuzagi). Sondanin okudugu gercek ayarlar
 * cagirana donuyor ki "istedik" ile "uygulandi" karistirilmasin — ASU-020 sesli
 * dogrulamasinda log'dan bakilacak tek yer burasi.
 */

/** Sondanin istedigi kisitlar. Ayni kisitlari WKWebView'in varsayilani da saglar (voice.md Bolum 11). */
export const MICROPHONE_CONSTRAINTS = {
  echoCancellation: true,
  noiseSuppression: true,
} as const satisfies MediaTrackConstraints;

/** Mikrofon hatasinin makine-okunur etiketi — `toUserFacingError` bu alani okur (ASU-019). */
export type MicrophoneErrorKind = 'mic_permission_denied' | 'mic_unavailable';

export class MicrophoneAccessError extends Error {
  public override readonly name = 'MicrophoneAccessError';

  public constructor(
    public readonly kind: MicrophoneErrorKind,
    message: string,
  ) {
    super(message);
  }
}

/**
 * Gercek `MediaStreamTrack`'in kullandigimiz dar yuzeyi.
 *
 * Tam DOM tipini istemiyoruz: testte sahte bir track uretmek icin 20 alan
 * doldurmak gerekirdi (`conventions.md` — "Harici servisler mock'lanir").
 */
export interface MicrophoneTrackLike {
  stop(): void;
  getSettings(): MediaTrackSettings;
}

export interface MicrophoneStreamLike {
  getTracks(): readonly MicrophoneTrackLike[];
}

/** `navigator.mediaDevices.getUserMedia` yerine gecebilecek dar imza. */
export type MicrophoneOpener = (
  constraints: MediaStreamConstraints,
) => Promise<MicrophoneStreamLike>;

/** Sondadan donen kanit: cihazda **gercekten** ne uygulandi. */
export interface MicrophoneProbe {
  /** Ayar okunamadiysa `null` — "bilmiyorum" ile "kapali" ayni sey degil. */
  readonly echoCancellation: boolean | null;
  readonly noiseSuppression: boolean | null;
}

/** `getUserMedia` reddi -> izin reddi. */
const PERMISSION_DENIED_ERRORS: ReadonlySet<string> = new Set([
  'NotAllowedError',
  'PermissionDeniedError',
  'SecurityError',
]);

interface MediaDevicesLike {
  getUserMedia(constraints: MediaStreamConstraints): Promise<MicrophoneStreamLike>;
}

interface NavigatorLike {
  readonly mediaDevices?: MediaDevicesLike;
}

function defaultMicrophoneOpener(
  constraints: MediaStreamConstraints,
): Promise<MicrophoneStreamLike> {
  const navigatorLike: NavigatorLike = globalThis.navigator;
  const devices = navigatorLike.mediaDevices;

  if (devices === undefined) {
    return Promise.reject(
      new MicrophoneAccessError(
        'mic_unavailable',
        'Bu ortamda mikrofon API’si (navigator.mediaDevices) yok.',
      ),
    );
  }

  return devices.getUserMedia(constraints);
}

function errorName(error: unknown): string {
  return error instanceof Error ? error.name : '';
}

function toMicrophoneAccessError(error: unknown): MicrophoneAccessError {
  if (error instanceof MicrophoneAccessError) {
    return error;
  }

  const name = errorName(error);

  if (PERMISSION_DENIED_ERRORS.has(name)) {
    return new MicrophoneAccessError(
      'mic_permission_denied',
      'Mikrofon izni verilmedi (getUserMedia reddedildi).',
    );
  }

  // Cihaz yok / baska uygulama tutuyor / kisit karsilanamadi / iptal edildi:
  // hepsi "mikrofonu acamadim" kovasi. Tahmin uretmiyoruz, etiketi durustce genel tutuyoruz.
  return new MicrophoneAccessError(
    'mic_unavailable',
    name.length > 0
      ? `Mikrofon acilamadi (${name}).`
      : 'Mikrofon acilamadi (neden bildirilmedi).',
  );
}

function readFlag(
  settings: MediaTrackSettings,
  key: 'echoCancellation' | 'noiseSuppression',
): boolean | null {
  const value = settings[key];
  return typeof value === 'boolean' ? value : null;
}

/**
 * Mikrofon iznini tetikler, uygulanan ayarlari okur ve track'leri **hemen** durdurur.
 *
 * @throws {MicrophoneAccessError} izin reddedildi ya da cihaz acilamadi.
 */
export async function probeMicrophoneAccess(
  open: MicrophoneOpener = defaultMicrophoneOpener,
): Promise<MicrophoneProbe> {
  let stream: MicrophoneStreamLike;

  try {
    stream = await open({ audio: MICROPHONE_CONSTRAINTS, video: false });
  } catch (error) {
    throw toMicrophoneAccessError(error);
  }

  const tracks = stream.getTracks();

  try {
    const track = tracks[0];
    if (track === undefined) {
      throw new MicrophoneAccessError(
        'mic_unavailable',
        'Mikrofon acildi ama ses track’i gelmedi.',
      );
    }

    const settings = track.getSettings();
    return {
      echoCancellation: readFlag(settings, 'echoCancellation'),
      noiseSuppression: readFlag(settings, 'noiseSuppression'),
    };
  } finally {
    // Sonda burada biter: mikrofon acik birakilmaz. Oturum mikrofonunu SDK acar.
    for (const track of tracks) {
      track.stop();
    }
  }
}

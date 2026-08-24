/**
 * Wake word saglayici adapter'i (ASU-021).
 *
 * Kaynak: `PROJECT.md` Bolum 8 ("Recommended wake-word provider" — arayuz birebir
 * oradan alindi), `docs/decisions/ADR-004-wake-word-provider.md`,
 * `asuna-config/conventions.md` — "Mimari — Servis Sinirlari".
 *
 * # Neden once arayuz?
 *
 * ADR-004 motoru (sherpa-onnx `KeywordSpotter`) ve yerlesimini (Tauri Rust process)
 * secti, ama **model + tetikleyici ifade karari halen acik**. Bu dosya vendor'dan
 * bagimsiz sozlesmeyi sabitler: Phase 2'nin geri kalani ve testler gercek motor
 * hazir olmadan ilerleyebilir, motor degisirse (oww-rs / rustpotter — ADR-004 exit
 * plani) uygulamanin geri kalani degismez.
 *
 * # Sinirlar (pazarliksiz)
 *
 * - Uygulamanin geri kalani **somut** saglayici tipini import etmez; yalnizca
 *   [`WakeWordProvider`] arayuzunu ve [`createWakeWordProvider`] fabrikasini gorur
 *   (`wake-word-provider-factory.ts`).
 * - Idle'da bulut yok: tespit tamamen local'dir. Bu arayuzun hicbir implementasyonu
 *   idle mikrofon karesini OpenAI'ya gondermez ya da diske yazmaz
 *   (PROJECT.md Bolum 8 "Privacy behavior").
 * - Bu dosya React'e, DOM'a ve Tauri'ye bagimli **degildir**. Durumun gorsel
 *   sunumu frontend'in isi; burada yalnizca motor/servis katmani var.
 */

/**
 * Saglayici secimi — `ASUNA_WAKE_WORD_PROVIDER`.
 *
 * Tek kaynak config sozlesmesidir: deger Rust tarafinda dogrulanir ve
 * `get_frontend_config` whitelist'i ile renderer'a gelir. Burada yeniden
 * tanimlanmaz (yalnizca yeniden export edilir) ki iki taraf birbirinden
 * sessizce ayrilmasin.
 */
export type { WakeWordProviderKind } from '../config/frontend-config';

/**
 * Tespit olayi.
 *
 * `confidence` **nullable**, opsiyonel degil: her motor skor bildirmez ve
 * "skor bilinmiyor" ile "skor 0" ayni sey degildir (ayni ayrim
 * `microphone-access.ts` `MicrophoneProbe` icinde de var).
 */
export interface WakeWordEvent {
  /** Tetikleyen ifade — `ASUNA_WAKE_WORD` (orn. "Hey Asuna"). */
  readonly phrase: string;
  /** Motorun bildirdigi guven skoru; motor skor vermiyorsa `null`. */
  readonly confidence: number | null;
  /** UTC ISO-8601 (conventions.md zaman kurali; `VoiceStateTransition.at` ile ayni bicim). */
  readonly at: string;
}

export type WakeWordListener = (event: WakeWordEvent) => void;

/**
 * PROJECT.md Bolum 8'deki adapter arayuzu — **birebir**, genisletilmeden.
 *
 * Yasam dongusu sozlesmesi (tum implementasyonlar uymak zorunda):
 * 1. `initialize()` bir kez cagrilir; motor kaynaklarini hazirlar. Tekrar cagrilmasi
 *    zararsizdir (idempotent).
 * 2. `start()` yalnizca basarili bir `initialize()` sonrasi cagrilabilir; aksi halde
 *    `WakeWordProviderError('not_initialized')` firlatir.
 * 3. `stop()` her zaman guvenlidir (baslamamis saglayicida no-op) ve `stop()` sonrasi
 *    **hicbir** dinleyici tetiklenmez.
 * 4. `onDetected()` abonelik kaldirma fonksiyonu doner; iki kez cagrilmasi zararsizdir.
 *    Abonelik `stop()`/`start()` dongusunden bagimsiz yasar.
 */
export interface WakeWordProvider {
  initialize(): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
  onDetected(callback: WakeWordListener): () => void;
}

/** Makine-okunur hata etiketi — UI/log tarafi mesaji degil bu alani okur. */
export type WakeWordProviderErrorKind =
  /** `start()` cagrildi ama `initialize()` basarili degil. */
  | 'not_initialized'
  /** Saglayici henuz uygulanmadi (ASU-022 stub'i). Sahte basari dondurulmez. */
  | 'not_implemented'
  /** Config'ten bilinmeyen bir saglayici adi geldi. */
  | 'unsupported_provider'
  /** Motor kurulamadi: model dosyasi yok, mikrofon baska uygulamada, vb. */
  | 'engine_unavailable';

export class WakeWordProviderError extends Error {
  public override readonly name = 'WakeWordProviderError';

  public constructor(
    public readonly kind: WakeWordProviderErrorKind,
    message: string,
  ) {
    super(message);
  }
}

/**
 * Dinleyici kumesi — saglayici implementasyonlarinin paylastigi kucuk yardimci.
 *
 * Uygulamanin geri kalani bunu kullanmaz; `audio/` icindeki iki saglayicinin
 * ayni abonelik/hata semantigini tekrar etmemesi icin var.
 *
 * Bir dinleyicinin hatasi digerlerini engellemez (bozuk bir debug paneli wake
 * akisini dusurmemeli); hatalar toplanip birlikte firlatilir — `VoiceStateMachine`
 * ile ayni desen, conventions.md "Sessiz yutma yok".
 */
export class WakeWordDetectionEmitter {
  private readonly listeners = new Set<WakeWordListener>();

  /** @returns aboneligi kaldiran fonksiyon (birden fazla cagrilmasi zararsiz). */
  public add(listener: WakeWordListener): () => void {
    this.listeners.add(listener);
    return (): void => {
      this.listeners.delete(listener);
    };
  }

  public get size(): number {
    return this.listeners.size;
  }

  public clear(): void {
    this.listeners.clear();
  }

  /** @throws {AggregateError} bir veya daha fazla dinleyici hata firlattiysa. */
  public emit(event: WakeWordEvent): void {
    const failures: unknown[] = [];

    for (const listener of [...this.listeners]) {
      try {
        listener(event);
      } catch (error) {
        failures.push(error);
      }
    }

    if (failures.length > 0) {
      throw new AggregateError(failures, `Wake word dinleyicisi hata verdi: ${event.phrase}.`);
    }
  }
}

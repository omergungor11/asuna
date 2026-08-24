/**
 * `FakeWakeWordProvider` (ASU-021) — test/gelistirme saglayicisi.
 *
 * ADR-004'un model + ifade karari acikken Phase 2'nin geri kalani (ASU-023 durum
 * gecisi, ASU-025 timeout, ASU-026 kapanis akisi) bu saglayici ile vendor'suz
 * ilerler; testler gercek mikrofona ve KWS modeline vurmaz
 * (conventions.md "Testing": harici servisler mock'lanir).
 *
 * # Pazarliksiz: bu saglayici mikrofon ACMAZ
 *
 * Ne `getUserMedia` cagirir, ne Rust tarafinda `cpal` stream'i actirir, ne de bir
 * ses karesine dokunur. `ASUNA_WAKE_WORD_PROVIDER=fake` ile calisan bir Asuna,
 * idle'da hicbir sey dinlemez — yalnizca elle tetiklenir. Bu, gizlilik sozunun
 * (PROJECT.md Bolum 8) test yolunda da bozulmadigi anlamina gelir.
 *
 * # Iki tetikleme yolu
 *
 * - **Programatik**: [`FakeWakeWordProvider.trigger`] — testlerin kullandigi yol.
 * - **Debug**: `start()` sirasinda global bir fonksiyon kurulur
 *   ([`FAKE_WAKE_GLOBAL`] = `__asunaFakeWake`). DevTools konsolundan
 *   `__asunaFakeWake()` cagrilarak wake akisi elle denenebilir. Global yalnizca
 *   saglayici **calisirken** durur: `stop()` onu kaldirir, boylece idle'da
 *   uygulamada asili duran bir tetikleyici kalmaz.
 */

import {
  WakeWordDetectionEmitter,
  WakeWordProviderError,
  type WakeWordEvent,
  type WakeWordListener,
  type WakeWordProvider,
} from './wake-word-provider';

/** Debug tetikleyicisinin global adi. Uretimde de ayni ad kullanilir; kurulumu `dev` ile sinirli. */
export const FAKE_WAKE_GLOBAL = '__asunaFakeWake';

/**
 * DevTools'tan cagrilan tetikleyici.
 *
 * @returns olay yayinlandi mi. Saglayici `start()` edilmemisse `false` doner —
 *   "tetikledim" yalani soylenmez (conventions.md "Hata Yonetimi").
 */
export type FakeWakeTrigger = (phrase?: string) => boolean;

/**
 * Global'in kurulacagi hedef. Testte gercek `globalThis` yerine sade bir nesne
 * verilir; boylece testler birbirinin global'ini kirletmez.
 */
export interface FakeWakeWordDebugTarget {
  [FAKE_WAKE_GLOBAL]?: FakeWakeTrigger;
}

export interface FakeWakeTriggerOptions {
  /** Varsayilan: saglayicinin config'ten aldigi ifade. */
  readonly phrase?: string;
  /** Varsayilan: `null` — sahte motorun akustik skoru yoktur, uydurulmaz. */
  readonly confidence?: number | null;
}

export interface FakeWakeWordProviderOptions {
  /**
   * Tetikleyici ifade. **Zorunlu**: `ASUNA_WAKE_WORD` config'ten gelir, koda
   * gomulmez (PROJECT.md Bolum 23 — "hicbir yerde hard-code edilmez").
   */
  readonly phrase: string;
  /** Debug global'i kurulsun mu. Varsayilan: yalnizca dev derlemesinde. */
  readonly installDebugTrigger?: boolean;
  /** Varsayilan: `globalThis`. */
  readonly debugTarget?: FakeWakeWordDebugTarget;
  /** Zaman kaynagi — testte deterministik kilmak icin enjekte edilir. */
  readonly now?: () => Date;
}

function defaultDebugTarget(): FakeWakeWordDebugTarget {
  return globalThis as FakeWakeWordDebugTarget;
}

export class FakeWakeWordProvider implements WakeWordProvider {
  private readonly emitter = new WakeWordDetectionEmitter();

  private readonly phrase: string;

  private readonly debugTarget: FakeWakeWordDebugTarget;

  private readonly debugTriggerEnabled: boolean;

  private readonly now: () => Date;

  private initialized = false;

  private running = false;

  /** Kurdugumuz global — baskasinin ayni ada koydugu fonksiyonu silmemek icin tutulur. */
  private installedTrigger: FakeWakeTrigger | null = null;

  public constructor(options: FakeWakeWordProviderOptions) {
    this.phrase = options.phrase;
    this.debugTarget = options.debugTarget ?? defaultDebugTarget();
    this.debugTriggerEnabled = options.installDebugTrigger ?? import.meta.env.DEV;
    this.now = options.now ?? ((): Date => new Date());
  }

  public initialize(): Promise<void> {
    this.initialized = true;
    return Promise.resolve();
  }

  public start(): Promise<void> {
    if (!this.initialized) {
      return Promise.reject(
        new WakeWordProviderError(
          'not_initialized',
          'FakeWakeWordProvider.start() cagrildi ama initialize() calismadi.',
        ),
      );
    }

    try {
      this.installDebugTrigger();
    } catch (error) {
      // `start()` sozlesmesi: hata **reject** olarak doner, senkron firlatilmaz —
      // `provider.start().catch(...)` yazan cagiran da hatayi gorebilmeli.
      return Promise.reject(
        error instanceof Error
          ? error
          : new WakeWordProviderError('engine_unavailable', 'Debug tetikleyicisi kurulamadi.'),
      );
    }

    this.running = true;
    return Promise.resolve();
  }

  public stop(): Promise<void> {
    // Baslamamis saglayicida no-op: kapanis yolu (ASU-026) her durumda cagirabilmeli.
    this.running = false;
    this.removeDebugTrigger();
    return Promise.resolve();
  }

  public onDetected(callback: WakeWordListener): () => void {
    return this.emitter.add(callback);
  }

  /** Tanilama/test icin: motor su an dinliyor mu. */
  public isRunning(): boolean {
    return this.running;
  }

  /**
   * Tespiti elle uretir.
   *
   * @returns olay yayinlandi mi. `start()` edilmemis ya da `stop()` edilmis bir
   *   saglayici `false` doner ve **hicbir** dinleyiciyi cagirmaz — gercek motorun
   *   durdurulduktan sonra susmasi burada da taklit edilir.
   * @throws {AggregateError} bir dinleyici hata firlattiysa.
   */
  public trigger(options: FakeWakeTriggerOptions = {}): boolean {
    if (!this.running) {
      return false;
    }

    const event: WakeWordEvent = {
      phrase: options.phrase ?? this.phrase,
      confidence: options.confidence ?? null,
      at: this.now().toISOString(),
    };

    this.emitter.emit(event);
    return true;
  }

  private installDebugTrigger(): void {
    if (!this.debugTriggerEnabled || this.installedTrigger !== null) {
      return;
    }

    const existing = this.debugTarget[FAKE_WAKE_GLOBAL];
    if (existing !== undefined) {
      throw new WakeWordProviderError(
        'engine_unavailable',
        `\`${FAKE_WAKE_GLOBAL}\` zaten tanimli — ayni anda iki wake word saglayicisi calisiyor olabilir.`,
      );
    }

    const trigger: FakeWakeTrigger = (phrase?: string): boolean =>
      this.trigger(phrase === undefined ? {} : { phrase });

    this.debugTarget[FAKE_WAKE_GLOBAL] = trigger;
    this.installedTrigger = trigger;
  }

  private removeDebugTrigger(): void {
    if (this.installedTrigger === null) {
      return;
    }

    // Yalnizca kendi kurdugumuzu kaldiriyoruz.
    if (this.debugTarget[FAKE_WAKE_GLOBAL] === this.installedTrigger) {
      // `delete obj[computedKey]` ESLint `no-dynamic-delete` ile yasak; `Reflect`
      // ayni isi yapar ve anahtari gercekten kaldirir (`= undefined` birakmaz).
      Reflect.deleteProperty(this.debugTarget, FAKE_WAKE_GLOBAL);
    }
    this.installedTrigger = null;
  }
}

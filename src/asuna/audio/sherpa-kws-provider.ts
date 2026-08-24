/**
 * `SherpaKwsProvider` — ISKELET (ASU-021). Gercek uygulama **ASU-022**.
 *
 * Kaynak: `docs/decisions/ADR-004-wake-word-provider.md` ("Mimari yerlesim" ve
 * "Secilen Cozumun Detaylari"), `asuna-tasks/phases/phase-2.md` ASU-022.
 *
 * # Bu dosya neden simdiden var?
 *
 * Fabrikanin (`wake-word-provider-factory.ts`) `sherpa-kws` dali icin bir hedefi
 * olmali ve o dal **durustce** "henuz uygulanmadi" demeli. Sessizce `FakeWakeWordProvider`'a
 * dusmek ya da `initialize()`'da hicbir sey yapmayip basari dondurmek, "Asuna dinliyor"
 * sanilan ama aslinda sagir bir uygulama uretirdi (conventions.md "Hata Yonetimi":
 * basari taklit edilmez).
 *
 * # Mimari (ADR-004) — mikrofon renderer'da DEGIL
 *
 * ```text
 * IDLE_WAKE_WORD : Rust  -> cpal input stream ACIK -> sherpa-onnx KeywordSpotter
 *                  Renderer -> getUserMedia CAGRILMAMIS, Realtime oturumu YOK, ag trafigi SIFIR
 * WAKING         : Rust  -> cpal stream'i DURDURUR + Tauri event yayinlar
 *                  Renderer -> bu saglayici event'i WakeWordProvider.onDetected'a cevirir
 * Oturum kapanisi: Renderer -> track'leri durdurur; Rust -> cpal stream'i YENIDEN ACAR
 * ```
 *
 * Yani bu sinif bir **motor degil, kopru**: renderer tarafinda hicbir ses karesi
 * islenmez, `sherpa-onnx` paketi renderer bundle'ina girmez.
 */

import {
  WakeWordDetectionEmitter,
  WakeWordProviderError,
  type WakeWordListener,
  type WakeWordProvider,
} from './wake-word-provider';

/**
 * Rust tarafinin tespit olayini yayinladigi Tauri event adi.
 *
 * SOZLESME: ASU-022'de `src-tauri` tarafi bu adi birebir kullanir ve
 * `src-tauri/capabilities/*.json` icinde renderer'a `event:default` (listen) izni
 * bu ad icin verilir. Ad iki tarafta da string literal olarak tekrarlanmaz —
 * Rust tarafi da kendi sabitine baglanir.
 */
export const WAKE_WORD_DETECTED_EVENT = 'asuna://wake-word-detected';

/**
 * Rust'tan gelen tespit payload'u (ASU-022 sozlesmesi).
 *
 * DIKKAT: bu tip yalnizca *beklentiyi* anlatir. Gelen payload ASU-022'de tip
 * **iddia** edilmeden dogrulanacak (conventions.md: harici veri schema ile
 * dogrulanir) — guvenilir process'ten gelse bile surum uyusmazligi sessizce
 * `undefined` bir ifadeye donusmemeli.
 */
export interface WakeWordDetectedPayload {
  /** Tetikleyen keyword — Rust tarafinda `ASUNA_WAKE_WORD` ile eslesir. */
  readonly phrase: string;
  /** `KeywordResult` skoru; motor bildirmezse `null`. */
  readonly confidence: number | null;
  /** UTC ISO-8601. */
  readonly at: string;
}

export interface SherpaKwsProviderOptions {
  /** `ASUNA_WAKE_WORD` — yalnizca olayi dogrulamak/loglamak icin; keyword listesi Rust'ta. */
  readonly phrase: string;
}

export class SherpaKwsProvider implements WakeWordProvider {
  /** Abonelikler simdiden kabul edilir; ASU-022'de Tauri event'i buraya baglanacak. */
  private readonly emitter = new WakeWordDetectionEmitter();

  private readonly phrase: string;

  public constructor(options: SherpaKwsProviderOptions) {
    this.phrase = options.phrase;
  }

  /**
   * @throws {WakeWordProviderError} her zaman — motor tarafi (ASU-022) henuz yok.
   *
   * ASU-022'de burasi soyle olacak (TODO, bilerek yorumda — `listen` importu ve
   * `event:default` capability'si motor gelmeden bundle'a girmesin):
   *
   * ```ts
   * // import { listen, type UnlistenFn } from '@tauri-apps/api/event';
   * //
   * // await invoke(START_WAKE_WORD_ENGINE_COMMAND);   // Rust: cpal + KeywordSpotter kurulur
   * // this.unlisten = await listen<unknown>(WAKE_WORD_DETECTED_EVENT, (event) => {
   * //   const payload = parseWakeWordDetectedPayload(event.payload); // schema dogrulamasi
   * //   this.emitter.emit(payload);
   * // });
   * ```
   */
  public initialize(): Promise<void> {
    return Promise.reject(
      new WakeWordProviderError(
        'not_implemented',
        `sherpa-onnx KWS motoru (\`${this.phrase}\`) henuz uygulanmadi — ASU-022. ` +
          'Gelistirme icin `ASUNA_WAKE_WORD_PROVIDER=fake` kullanin.',
      ),
    );
  }

  /**
   * @throws {WakeWordProviderError} her zaman: basarili bir `initialize()` mumkun
   *   olmadigi icin bu noktaya ancak hatayi yutan bir cagiran ulasir.
   */
  public start(): Promise<void> {
    return Promise.reject(
      new WakeWordProviderError(
        'not_initialized',
        'SherpaKwsProvider.start() cagrildi ama motor kurulmadi (ASU-022).',
      ),
    );
  }

  /**
   * No-op — ama hata firlatmaz.
   *
   * Kapanis/temizlik yollari (ASU-026, `window-lifecycle`) saglayicinin durumundan
   * bagimsiz olarak `stop()` cagirabilmeli; burada patlamak, motor hic kurulmamisken
   * uygulama kapanisini bozardi.
   */
  public stop(): Promise<void> {
    return Promise.resolve();
  }

  public onDetected(callback: WakeWordListener): () => void {
    return this.emitter.add(callback);
  }
}

/**
 * `AsunaRealtimeService` ile OpenAI Agents SDK arasindaki **tek** sinir (ASU-013).
 *
 * Servis yalnizca bu port'u tanir; gercek implementasyon `realtime-service.ts` icindeki
 * `createOpenAiRealtimeSession` fabrikasidir. Testler ayni port'u sahte bir nesneyle
 * karsilar — bu sayede yasam dongusu ve event -> durum eslemeleri **ag olmadan** test
 * edilir (`conventions.md` — "Harici servisler mock'lanir").
 *
 * Bu dosyada SDK importu **yoktur**.
 */

import type { AsunaRealtimeErrorInfo } from './realtime-errors';
import type { RealtimeUsageSnapshot, TranscriptEntry } from './realtime-events';
import type { VadEagerness } from '../config/frontend-config';
import type { AsunaToolDefinition } from '../tools/types';

/**
 * Tur tespiti ayari (ASU-064) — SDK tipi degil, duz veri.
 *
 * Ayrik birlesim bilincli: `eagerness` yalnizca `semantic_vad`'de,
 * `silenceDurationMs` yalnizca `server_vad`'de anlamli. Ikisini ayni nesnede
 * opsiyonel alan olarak tutmak, ilgisiz bir alani sessizce SDK'ya gondermeye
 * davetiye cikarirdi.
 */
export type TurnDetectionSpec =
  | { readonly type: 'semantic_vad'; readonly eagerness: VadEagerness }
  | { readonly type: 'server_vad'; readonly silenceDurationMs: number };

/**
 * Oturumun acilis parametreleri. Model ID burada bir *degerdir*, sabit degil —
 * `ASUNA_REALTIME_MODEL` config'inden gelir (hard-code yasagi).
 */
export interface RealtimeSessionSpec {
  /** `buildAsunaInstructions(context)` ciktisi. */
  readonly instructions: string;
  readonly model: string;
  /** `ASUNA_REALTIME_VOICE`; `null` = SDK varsayilani. */
  readonly voice: string | null;
  /**
   * Kullanici sesinin transkript edilip edilmeyecegi (`ASUNA_TRANSCRIPT_STORAGE`).
   * `false` ise transkripsiyon **kapatilir** — hem maliyet hem gizlilik (voice.md Bolum 2).
   */
  readonly transcription: boolean;
  /**
   * Tur tespiti ayari (`ASUNA_TURN_DETECTION` + `ASUNA_VAD_*`). Sabit degil:
   * gecikme/erken-kesme takasi rebuild olmadan env ile ayarlanabilir (ASU-064).
   */
  readonly turnDetection: TurnDetectionSpec;
  /**
   * Modele acilan tool'lar. ASU-044'ten beri dolu (`get_current_project`, risk 0);
   * SDK `tool()` cevrimi `realtime-service.ts` icinde yapilir — SDK tipi bu
   * dosyaya girmez.
   */
  readonly tools: readonly AsunaToolDefinition[];
}

/**
 * SDK event'lerinin ham veri karsiligi. Isimlendirme bilerek SDK event adlarini izler
 * (voice.md Bolum 3 tablosu) ki eslemenin dogrulugu tek bakista gorulebilsin; tasidiklari
 * ise duz veridir.
 */
export type RealtimeSessionSignal =
  /** SDK `agent_start`. */
  | { readonly type: 'agent_start' }
  /** SDK `agent_end`. */
  | { readonly type: 'agent_end' }
  /** SDK `audio_start`. */
  | { readonly type: 'audio_start' }
  /** SDK `audio_stopped`. */
  | { readonly type: 'audio_stopped' }
  /** SDK `audio_interrupted` (barge-in). */
  | { readonly type: 'audio_interrupted' }
  /** SDK `history_updated` / `history_added` — normalize edilmis dokum satirlari. */
  | { readonly type: 'history'; readonly entries: readonly TranscriptEntry[] }
  /** SDK `agent_tool_start` — Phase 5. */
  | { readonly type: 'tool_start'; readonly toolName: string }
  /** SDK `agent_tool_end` — Phase 5. */
  | { readonly type: 'tool_end'; readonly toolName: string }
  /** SDK `tool_approval_requested` — Phase 5. */
  | { readonly type: 'tool_approval_requested'; readonly toolName: string }
  /** SDK `error`. */
  | { readonly type: 'error'; readonly error: AsunaRealtimeErrorInfo };

export type RealtimeSessionSignalListener = (signal: RealtimeSessionSignal) => void;

/** Kisa omurlu `ek_` token'i uretecek lazy fonksiyon (voice.md Bolum 5). */
export type EphemeralApiKeyProvider = () => Promise<string>;

/** Servisin gordugu oturum yuzeyi. */
export interface RealtimeSessionPort {
  /**
   * Baglanti kurar. `apiKey` **lazy**: SDK token'i tam ihtiyac aninda ister, boylece
   * yeniden denemelerde taze token uretilir ve token gereksiz yere bellekte durmaz.
   */
  connect(options: { readonly apiKey: EphemeralApiKeyProvider }): Promise<void>;
  /** SDK'da `void` — `await` edilmez (voice.md Bolum 9 madde 4). */
  close(): void;
  /** Manuel "sus": uretilen yaniti keser. */
  interrupt(): void;
  /** Anlik token kullanimi. Kapanistan hemen once okunur. */
  usage(): RealtimeUsageSnapshot;
}

/**
 * Oturum fabrikasi. Ortam desteklemiyorsa (WebRTC yok) **kurucu asamasinda** hata
 * firlatabilir; servis bunu yakalayip `ERROR` durumuna cevirir.
 */
export type RealtimeSessionFactory = (
  spec: RealtimeSessionSpec,
  onSignal: RealtimeSessionSignalListener,
) => RealtimeSessionPort;

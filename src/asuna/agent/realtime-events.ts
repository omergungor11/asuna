/**
 * `AsunaRealtimeService`'in disariya yaydigi **normalize** event'ler (ASU-013).
 *
 * Sozlesme: bu dosyada OpenAI Agents SDK'sindan gelen hicbir tip yoktur ve olmayacaktir.
 * UI, log ve ileride memory cikarimi bu union'i tuketir; SDK surumu degistiginde
 * degismesi gereken tek yer `realtime-service.ts` icindeki cevirici olmalidir
 * (`conventions.md` — "Ucuncu parti SDK detaylari wrapper arkasinda kalir").
 *
 * Durum degisiklikleri bu union'da **yok**: tek dogru durum kaynagi
 * `VoiceStateMachine` ve gecisler oradan (`subscribe`) yayinlanir (ASU-014).
 */

import type { AsunaRealtimeErrorInfo } from './realtime-errors';
import type { VoiceState } from '../state/voice-state-machine';

/** Konusma dokumunun tek bir satiri. */
export interface TranscriptEntry {
  /** Realtime item kimligi — ayni item guncellendikce ayni kalir. */
  readonly itemId: string;
  readonly role: 'user' | 'assistant';
  /** Bilinen en son metin. Transkripsiyon henuz gelmediyse bos olabilir. */
  readonly text: string;
  readonly status: 'in_progress' | 'completed' | 'incomplete';
}

/**
 * Oturum kapanisinda raporlanan token kullanimi (ASU-020 maliyet olcumu).
 *
 * `*TokenDetails` icindeki anahtarlar (`audio_tokens`, `cached_tokens`, ...) SDK
 * tarafinda `Record<string, number>` — voice.md Bolum 6 V9: gercek anahtar isimleri
 * ilk canli oturumda dogrulanacak. Bu yuzden burada da serbest kayit olarak tasiniyor.
 */
export interface RealtimeUsageSnapshot {
  readonly requests: number;
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly totalTokens: number;
  readonly inputTokenDetails: readonly Readonly<Record<string, number>>[];
  readonly outputTokenDetails: readonly Readonly<Record<string, number>>[];
}

/** Oturumun neden kapandigi. */
export type RealtimeDisconnectReason =
  /** `disconnect()` cagrildi (kullanici "Stop" dedi ya da idle timeout). */
  | 'requested'
  /** Kurtarilamaz bir hata oturumu kapatti. */
  | 'error';

export type AsunaRealtimeEvent =
  /** Token isteniyor / SDP kuruluyor. `attempt` 1 tabanli. */
  | { readonly type: 'connecting'; readonly attempt: number; readonly maxAttempts: number }
  /** Oturum acildi; model ID gorunur olsun ki hangi modelde konusuldugu belirsiz kalmasin. */
  | { readonly type: 'connected'; readonly model: string }
  /** Sinirli yeniden deneme — sessiz degil, gorunur (ASU-013 kabul kriteri). */
  | {
      readonly type: 'reconnecting';
      readonly attempt: number;
      readonly maxAttempts: number;
      readonly delayMs: number;
      readonly error: AsunaRealtimeErrorInfo;
    }
  /** Kapanistan hemen once yayinlanir (maliyet olcumu). */
  | { readonly type: 'usage'; readonly usage: RealtimeUsageSnapshot }
  | { readonly type: 'disconnected'; readonly reason: RealtimeDisconnectReason }
  /** Model yanit uretmeye basladi. */
  | { readonly type: 'agent_thinking' }
  | { readonly type: 'agent_audio_started' }
  | { readonly type: 'agent_audio_stopped' }
  /** Barge-in: kullanici Asuna'nin sozunu kesti. */
  | { readonly type: 'agent_interrupted' }
  /** Tur bitti (SDK `agent_end`). Metin ayrica `transcript` ile gelir. */
  | { readonly type: 'turn_ended' }
  | { readonly type: 'transcript'; readonly entry: TranscriptEntry }
  /** Phase 5 (ASU-05x) placeholder: Phase 1'de tool yok, bu event yayinlanmaz. */
  | { readonly type: 'tool_call_started'; readonly toolName: string }
  /** Phase 5 (ASU-05x) placeholder. */
  | { readonly type: 'tool_call_completed'; readonly toolName: string }
  /** Phase 5 (ASU-05x) placeholder. */
  | { readonly type: 'tool_approval_requested'; readonly toolName: string }
  /**
   * SDK, mevcut duruma uymayan bir sinyal yolladi. Sessizce yutulmuyor ama sesli
   * oturumu da dusurmuyor (`conventions.md` — "Bozulan alt sistem tum urunu dusurmez").
   */
  | {
      readonly type: 'unexpected_signal';
      readonly signal: string;
      readonly state: VoiceState;
    }
  | { readonly type: 'error'; readonly error: AsunaRealtimeErrorInfo };

export type AsunaRealtimeEventListener = (event: AsunaRealtimeEvent) => void;

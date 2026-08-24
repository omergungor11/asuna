/**
 * Agent katmaninin genel yuzeyi (ASU-013).
 *
 * Disaridaki kod (UI hook'lari, ileride memory/tool katmanlari) agent modullerini
 * tek tek degil buradan alir. Bu yuzeyde OpenAI Agents SDK'sina ait **hicbir tip yok**.
 */

export { AsunaRealtimeService, createOpenAiRealtimeSession } from './realtime-service';
export type { AsunaRealtimeServiceOptions } from './realtime-service';

export type {
  AsunaRealtimeEvent,
  AsunaRealtimeEventListener,
  RealtimeDisconnectReason,
  RealtimeUsageSnapshot,
  TranscriptEntry,
} from './realtime-events';

export {
  ASUNA_REALTIME_ERROR_KINDS,
  AsunaRealtimeError,
  redactSecrets,
} from './realtime-errors';
export type { AsunaRealtimeErrorInfo, AsunaRealtimeErrorKind } from './realtime-errors';

export type {
  EphemeralApiKeyProvider,
  RealtimeSessionFactory,
  RealtimeSessionPort,
  RealtimeSessionSignal,
  RealtimeSessionSignalListener,
  RealtimeSessionSpec,
} from './realtime-session-port';

export { MINT_REALTIME_TOKEN_COMMAND, mintRealtimeToken } from './realtime-token';
export type { EphemeralRealtimeToken } from './realtime-token';

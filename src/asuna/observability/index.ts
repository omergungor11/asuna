/**
 * Observability genel API'si (ASU-019).
 *
 * Diger alt sistemler (agent, audio, memory, tools) buradan import eder;
 * ic dosya duzeni disari sozlesme degildir.
 */

export {
  AsunaLogger,
  DEFAULT_LOG_BUFFER_CAPACITY,
  LOG_LEVELS,
  LogRingBuffer,
  REDACTED,
  applyConfigLogLevel,
  createConsoleSink,
  formatClockTime,
  formatLogEntry,
  isLevelEnabledFor,
  isSensitiveKey,
  logBuffer,
  logger,
  redactData,
  redactText,
  redactValue,
  type LogEntry,
  type LogLevel,
  type LogSink,
  type LoggerOptions,
} from './logger';

export {
  VOICE_STATE_LOG_SCOPE,
  attachVoiceStateLogger,
  createLoggedVoiceStateMachine,
  createVoiceStateLogger,
  formatInvalidTransitionLine,
  formatStateTransitionLine,
  type LoggedVoiceStateMachineOptions,
  type VoiceStateLoggerHooks,
} from './state-logger';

export {
  ASUNA_ERROR_KINDS,
  ASUNA_SERVICE_ERROR_KINDS,
  REALTIME_TOKEN_ERROR_KINDS,
  UNKNOWN_ERROR_KIND,
  describeUserFacingError,
  errorKindOf,
  isAsunaErrorKind,
  toUserFacingError,
  userFacingErrorFor,
  type AsunaErrorKind,
  type AsunaServiceErrorKind,
  type RealtimeTokenErrorKind,
  type UserFacingError,
} from './error-messages';

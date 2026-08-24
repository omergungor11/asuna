/**
 * Renderer'in gorebilecegi config alt kumesi (ASU-009).
 *
 * Bu tip, Rust tarafindaki `FrontendConfig` whitelist'inin (`src-tauri/src/config.rs`)
 * TypeScript karsiligidir. Kalici `OPENAI_API_KEY` ve wake-word motoru detaylari
 * bu sozlesmede **yoktur** ve hicbir kosulda renderer'a gelmez
 * (PROJECT.md Bolum 19, security.md Bolum 1).
 *
 * Gelen payload tip *iddia* edilmez, [`parseFrontendConfig`] ile **dogrulanir**.
 * Zod henuz bagimlilik degil; dogrulama elle ve dar tutuldu.
 */

export const LOG_LEVELS = ['error', 'warn', 'info', 'debug'] as const;
export type LogLevel = (typeof LOG_LEVELS)[number];

/**
 * Tool onay politikasi. Risk 2/3 tool'lar iki modda da her zaman onay ister;
 * bu deger onlari bypass edemez (conventions.md "Tool Tanimi").
 */
export const TOOL_APPROVAL_MODES = ['safe', 'always'] as const;
export type ToolApprovalMode = (typeof TOOL_APPROVAL_MODES)[number];

export interface FrontendConfig {
  /** `ASUNA_REALTIME_MODEL` — model ID hicbir yerde hard-code edilmez. */
  readonly realtimeModel: string;
  /** `ASUNA_REALTIME_VOICE`; `null` = SDK varsayilani. */
  readonly realtimeVoice: string | null;
  /** `ASUNA_WAKE_WORD` — sadece gosterim/metin; motor Rust tarafinda. */
  readonly wakeWord: string;
  readonly idleTimeoutSeconds: number;
  readonly logLevel: LogLevel;
  readonly memoryEnabled: boolean;
  readonly transcriptStorage: boolean;
  readonly toolApprovalMode: ToolApprovalMode;
}

/** Sozlesmede izin verilen alanlarin tam listesi. */
export const FRONTEND_CONFIG_KEYS = [
  'realtimeModel',
  'realtimeVoice',
  'wakeWord',
  'idleTimeoutSeconds',
  'logLevel',
  'memoryEnabled',
  'transcriptStorage',
  'toolApprovalMode',
] as const;

/**
 * Config sozlesmesi ihlali.
 *
 * GUVENLIK: mesaj yalnizca **alan adini** ve beklenen bicimi tasir, gelen
 * **degeri asla** tasimaz — beklenmeyen bir alanin icinde secret olabilir ve
 * hata mesajlari log'a/UI'a dusebilir.
 */
export class FrontendConfigError extends Error {
  public override readonly name = 'FrontendConfigError';

  public constructor(message: string) {
    super(message);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readString(source: Record<string, unknown>, key: string): string {
  const value = source[key];
  if (typeof value !== 'string' || value.length === 0) {
    throw new FrontendConfigError(`\`${key}\` bos olmayan bir string olmali.`);
  }
  return value;
}

function readNullableString(source: Record<string, unknown>, key: string): string | null {
  const value = source[key];
  if (value === null) {
    return null;
  }
  if (typeof value !== 'string' || value.length === 0) {
    throw new FrontendConfigError(`\`${key}\` bos olmayan bir string ya da null olmali.`);
  }
  return value;
}

function readBoolean(source: Record<string, unknown>, key: string): boolean {
  const value = source[key];
  if (typeof value !== 'boolean') {
    throw new FrontendConfigError(`\`${key}\` boolean olmali.`);
  }
  return value;
}

function readPositiveInteger(source: Record<string, unknown>, key: string): number {
  const value = source[key];
  if (typeof value !== 'number' || !Number.isInteger(value) || value <= 0) {
    throw new FrontendConfigError(`\`${key}\` pozitif tam sayi olmali.`);
  }
  return value;
}

function readEnum<T extends string>(
  source: Record<string, unknown>,
  key: string,
  allowed: readonly T[],
): T {
  const value = source[key];
  if (typeof value !== 'string' || !(allowed as readonly string[]).includes(value)) {
    throw new FrontendConfigError(
      `\`${key}\` su degerlerden biri olmali: ${allowed.join(', ')}.`,
    );
  }
  return value as T;
}

/**
 * Rust tarafindan gelen ham payload'u dogrular.
 *
 * Beklenmeyen alanlar **reddedilir** (whitelist). Bunun sebebi sadece tip
 * hijyeni degil: backend bir gun yanlislikla fazladan bir alan dondurse
 * (orn. bir credential), bu sessizce renderer'a akmak yerine gurultulu bir
 * hataya donusur.
 */
export function parseFrontendConfig(value: unknown): FrontendConfig {
  if (!isRecord(value)) {
    throw new FrontendConfigError('Config payload bir nesne olmali.');
  }

  const allowed: readonly string[] = FRONTEND_CONFIG_KEYS;
  const unexpected = Object.keys(value).filter((key) => !allowed.includes(key));
  if (unexpected.length > 0) {
    throw new FrontendConfigError(
      `Config payload beklenmeyen alan(lar) iceriyor: ${unexpected.join(', ')}.`,
    );
  }

  return {
    realtimeModel: readString(value, 'realtimeModel'),
    realtimeVoice: readNullableString(value, 'realtimeVoice'),
    wakeWord: readString(value, 'wakeWord'),
    idleTimeoutSeconds: readPositiveInteger(value, 'idleTimeoutSeconds'),
    logLevel: readEnum(value, 'logLevel', LOG_LEVELS),
    memoryEnabled: readBoolean(value, 'memoryEnabled'),
    transcriptStorage: readBoolean(value, 'transcriptStorage'),
    toolApprovalMode: readEnum(value, 'toolApprovalMode', TOOL_APPROVAL_MODES),
  };
}

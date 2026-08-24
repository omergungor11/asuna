/**
 * Ephemeral Realtime token'inin renderer tarafi (ASU-013, ASU-011 komutunun tuketicisi).
 *
 * Kalici `OPENAI_API_KEY` bu tarafa **hic gelmez**; renderer yalnizca kisa omurlu
 * `ek_` token'ini gorur (PROJECT.md Bolum 19, voice.md Bolum 5).
 *
 * Kurallar:
 * - Token cache'lenmez; her `connect()` oncesi taze istenir.
 * - Token degeri log'lanmaz, hata mesajina konmaz, `toString`'e sizmaz.
 * - Gelen payload tip *iddia* edilmez, dogrulanir (`conventions.md`).
 */

import { invoke } from '@tauri-apps/api/core';

/**
 * Rust komut adi — `src-tauri/src/realtime_token.rs` ve capability manifest'i ile
 * birebir ayni olmali.
 */
export const MINT_REALTIME_TOKEN_COMMAND = 'mint_realtime_token';

/** `EphemeralToken` (Rust) camelCase karsiligi: `{ value, expiresAt, model }`. */
export interface EphemeralRealtimeToken {
  /** `ek_` ile baslayan kisa omurlu client secret. */
  readonly value: string;
  /** Unix epoch (saniye). */
  readonly expiresAt: number;
  /** Token'in basildigi model ID — renderer'in kullandigi model ile ayni olmali. */
  readonly model: string;
}

/** Token uretimi/dogrulamasi basarisiz oldu. Mesaj **token degeri tasimaz**. */
export class RealtimeTokenContractError extends Error {
  public override readonly name = 'RealtimeTokenContractError';

  public constructor(message: string) {
    super(message);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * IPC yanitini dogrular.
 *
 * `sk-` gorunumlu bir deger reddedilir. Rust tarafi bunu zaten engelliyor; buradaki
 * ikinci kontrol savunma amacli: kalici bir anahtarin WebRTC transport'una gecmesi
 * (SDK guard'i olsa bile) sessizce olmamali.
 */
export function parseEphemeralRealtimeToken(value: unknown): EphemeralRealtimeToken {
  if (!isRecord(value)) {
    throw new RealtimeTokenContractError('Token yaniti bir nesne olmali.');
  }

  const { value: token, expiresAt, model } = value;

  if (typeof token !== 'string' || token.trim().length === 0) {
    throw new RealtimeTokenContractError('Token yaniti bos olmayan bir `value` icermeli.');
  }
  if (token.startsWith('sk-')) {
    // GUVENLIK: deger hata mesajina konmaz.
    throw new RealtimeTokenContractError(
      'Token yaniti kalici bir API anahtari gorunumunde; kisa omurlu anahtar bekleniyordu.',
    );
  }
  if (typeof expiresAt !== 'number' || !Number.isFinite(expiresAt) || expiresAt <= 0) {
    throw new RealtimeTokenContractError(
      'Token yanitindaki `expiresAt` pozitif bir sayi olmali.',
    );
  }
  if (typeof model !== 'string' || model.length === 0) {
    throw new RealtimeTokenContractError('Token yaniti bos olmayan bir `model` icermeli.');
  }

  return { value: token, expiresAt, model };
}

/** Rust'tan taze bir ephemeral token ister. */
export async function mintRealtimeToken(): Promise<EphemeralRealtimeToken> {
  const raw = await invoke<unknown>(MINT_REALTIME_TOKEN_COMMAND);
  return parseEphemeralRealtimeToken(raw);
}

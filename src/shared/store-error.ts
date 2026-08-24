/**
 * Hafiza/oturum deposunun IPC hatasi — Rust `StoreError`'un tip aynasi (ASU-031).
 *
 * Rust tarafi `Result<T, StoreError>` donunce Tauri, hatayi **serilestirilmis
 * haliyle** (`{ code, message }`) reddedilen promise'e koyar. Renderer'in bunu
 * `Error` sanip `error.message`'a bakmasi sessiz bir hataya yol acardi: nesnenin
 * `message` alani var ama `instanceof Error` degil.
 *
 * Ayrica ACL reddi bambaska bir sekilde gelir (duz string). Ikisi burada tek bir
 * tipe indirgenir ve **hicbiri uydurulmus bir koda** eslenmez: taninmayan sekil
 * `unknown` kodunu alir, mesaji korunur.
 */

import { isRecord } from './contract';

/** Rust `StoreErrorCode` ile birebir. */
export const STORE_ERROR_CODES = ['invalid', 'not-found', 'unavailable', 'storage'] as const;

export type StoreErrorCode = (typeof STORE_ERROR_CODES)[number];

/**
 * Taninmayan sekil. Uydurulmus bir sinif yerine acik bir "bilmiyorum":
 * cogunlukla ACL reddi ya da IPC katmani hatasi olur.
 */
export const UNKNOWN_STORE_ERROR_CODE = 'unknown';

export type AsunaStoreErrorCode = StoreErrorCode | typeof UNKNOWN_STORE_ERROR_CODE;

export function isStoreErrorCode(value: unknown): value is StoreErrorCode {
  return typeof value === 'string' && (STORE_ERROR_CODES as readonly string[]).includes(value);
}

/** Servis katmaninin firlattigi hata. */
export class AsunaStoreError extends Error {
  public override readonly name = 'AsunaStoreError';

  public constructor(
    public readonly code: AsunaStoreErrorCode,
    message: string,
  ) {
    super(message);
  }

  /** Hafiza kapali degil, **bozuk** — UI bunu ariza olarak gostermeli. */
  public get isUnavailable(): boolean {
    return this.code === 'unavailable';
  }
}

/**
 * `invoke` reddini tipli hataya cevirir. Hicbir zaman yutmaz; en kotu ihtimalle
 * `unknown` kodlu ama mesaji korunmus bir hata uretir.
 */
export function toStoreError(value: unknown): AsunaStoreError {
  if (value instanceof AsunaStoreError) {
    return value;
  }

  if (
    isRecord(value) &&
    isStoreErrorCode(value['code']) &&
    typeof value['message'] === 'string'
  ) {
    return new AsunaStoreError(value['code'], value['message']);
  }

  if (typeof value === 'string' && value.length > 0) {
    return new AsunaStoreError(UNKNOWN_STORE_ERROR_CODE, value);
  }

  if (value instanceof Error) {
    return new AsunaStoreError(UNKNOWN_STORE_ERROR_CODE, value.message);
  }

  return new AsunaStoreError(UNKNOWN_STORE_ERROR_CODE, 'Hafiza islemi basarisiz oldu.');
}

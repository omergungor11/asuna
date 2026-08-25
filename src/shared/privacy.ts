/**
 * Calisma zamani gizlilik ayarlari — Rust `PrivacySettings`'in tip aynasi (ASU-037).
 *
 * Kaynak gercek: `src-tauri/src/privacy.rs`. ADR-005 kurali: sozlesme
 * degisikligi ile bu ayna **ayni commit'te** gider.
 *
 * # Iki deger, iki anlam
 *
 * Her anahtar icin **etkin** deger ve **acilis** degeri ayri tasinir:
 *
 * - etkin (`memoryEnabled`) — su an ne oluyor.
 * - acilis (`memoryEnabledAtBoot`) — `.env` ne diyordu. Bu bir tavandir:
 *   acilista kapali olan bir sey calisma zamaninda **acilamaz** (DB dosyasi hic
 *   acilmadi). UI anahtari kilitli cizip nedenini yazabilsin diye gonderiliyor.
 *
 * Bu alanlar secret degil, kullanicinin kendi ayarlaridir.
 */

import { ContractError, assertNoUnexpectedKeys, isRecord, readers } from './contract';

export interface PrivacySettings {
  /** Kalici hafizaya **yeni** kayit yazilabilir mi? */
  readonly memoryEnabled: boolean;
  /** Konusma dokumu diske yazilabilir mi? */
  readonly transcriptStorage: boolean;
  /** `ASUNA_MEMORY_ENABLED` — acilis degeri (tavan). */
  readonly memoryEnabledAtBoot: boolean;
  /** `ASUNA_TRANSCRIPT_STORAGE` — acilis degeri (tavan). */
  readonly transcriptStorageAtBoot: boolean;
}

export const PRIVACY_SETTINGS_KEYS = [
  'memoryEnabled',
  'transcriptStorage',
  'memoryEnabledAtBoot',
  'transcriptStorageAtBoot',
] as const;

/**
 * Kismi guncelleme: verilmeyen alana **dokunulmaz**.
 *
 * `exactOptionalPropertyTypes` acik oldugu icin "alan var ama `undefined`"
 * belirsizligi yok; Rust tarafi da `deny_unknown_fields` ile bekliyor.
 */
export interface PrivacyPatch {
  readonly memoryEnabled?: boolean;
  readonly transcriptStorage?: boolean;
}

export class PrivacyContractError extends ContractError {
  public override readonly name = 'PrivacyContractError';
}

function fail(field: string, expected: string): never {
  throw new PrivacyContractError(`\`${field}\` ${expected} olmali.`);
}

function failWith(message: string): never {
  throw new PrivacyContractError(message);
}

export function parsePrivacySettings(value: unknown): PrivacySettings {
  if (!isRecord(value)) {
    throw new PrivacyContractError('Gizlilik ayarlari bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, PRIVACY_SETTINGS_KEYS, failWith);

  const read = readers(value, fail);

  return {
    memoryEnabled: read.boolean('memoryEnabled'),
    transcriptStorage: read.boolean('transcriptStorage'),
    memoryEnabledAtBoot: read.boolean('memoryEnabledAtBoot'),
    transcriptStorageAtBoot: read.boolean('transcriptStorageAtBoot'),
  };
}

/**
 * Bir anahtar calisma zamaninda **acilabilir** mi?
 *
 * Kapatmak her zaman mumkundur; acmak yalnizca acilista aciksa mumkundur.
 * UI bunu tiklamadan once bilmek zorunda — kilitli bir anahtari tiklatip
 * hata gostermek, kullaniciya ayarin ne oldugunu ogretmez.
 */
export function canEnableAtRuntime(settings: PrivacySettings, key: PrivacyToggleKey): boolean {
  return key === 'memoryEnabled'
    ? settings.memoryEnabledAtBoot
    : settings.transcriptStorageAtBoot;
}

export type PrivacyToggleKey = 'memoryEnabled' | 'transcriptStorage';

// ---------------------------------------------------------------------------
// Hata
// ---------------------------------------------------------------------------

/** Rust `PrivacyError::code()` ile birebir. */
export const PRIVACY_ERROR_CODES = ['locked-by-env'] as const;

export type PrivacyErrorCode = (typeof PRIVACY_ERROR_CODES)[number];

/** Taninmayan sekil (cogunlukla ACL reddi ya da IPC katmani hatasi). */
export const UNKNOWN_PRIVACY_ERROR_CODE = 'unknown';

export type AsunaPrivacyErrorCode = PrivacyErrorCode | typeof UNKNOWN_PRIVACY_ERROR_CODE;

export class AsunaPrivacyError extends Error {
  public override readonly name = 'AsunaPrivacyError';

  public constructor(
    public readonly code: AsunaPrivacyErrorCode,
    message: string,
  ) {
    super(message);
  }

  /** Anahtar `.env` ile kapatilmis; yeniden baslatmadan acilamaz. */
  public get isLockedByEnv(): boolean {
    return this.code === 'locked-by-env';
  }
}

/**
 * `invoke` reddini tipli hataya cevirir — `toStoreError` ile ayni disiplin:
 * hicbir zaman yutmaz, en kotu ihtimalle `unknown` kodlu ama mesaji korunmus
 * bir hata uretir.
 */
export function toPrivacyError(value: unknown): AsunaPrivacyError {
  if (value instanceof AsunaPrivacyError) {
    return value;
  }

  if (isRecord(value) && typeof value['message'] === 'string') {
    const code = value['code'];
    return new AsunaPrivacyError(
      typeof code === 'string' && (PRIVACY_ERROR_CODES as readonly string[]).includes(code)
        ? (code as PrivacyErrorCode)
        : UNKNOWN_PRIVACY_ERROR_CODE,
      value['message'],
    );
  }

  if (typeof value === 'string' && value.length > 0) {
    return new AsunaPrivacyError(UNKNOWN_PRIVACY_ERROR_CODE, value);
  }

  if (value instanceof Error) {
    return new AsunaPrivacyError(UNKNOWN_PRIVACY_ERROR_CODE, value.message);
  }

  return new AsunaPrivacyError(UNKNOWN_PRIVACY_ERROR_CODE, 'Gizlilik ayari degistirilemedi.');
}

/**
 * Seviyeli, redaksiyonlu logger (ASU-019).
 *
 * Kaynak: `PROJECT.md` Bolum 29 ("Log state transitions", "Never log secrets",
 * "In development, provide a debug console"), Bolum 19 (secret sinirlari),
 * `asuna-config/conventions.md` — "Hata Yonetimi".
 *
 * Tasarim kararlari:
 * - **Redaksiyon opsiyonel degil.** Her mesaj ve her veri alani sink'e ulasmadan
 *   once [`redactText`] / [`redactValue`] suzgecinden gecer. Cagiran tarafin
 *   "bunu maskele" demeyi unutmasi bir sizinti uretmez; varsayilan guvenli taraftir.
 * - **Ring buffer bellekte, diskte degil.** Son [`DEFAULT_LOG_BUFFER_CAPACITY`]
 *   satir tutulur; debug paneli (ASU-019) bu tampondan beslenir. Kalici log
 *   dosyasi bu task'in kapsaminda degil — diske yazmak ayri bir gizlilik karari
 *   (PROJECT.md Bolum 20) ve `TRANSCRIPT.md` ile birlikte ele alinir.
 * - **React bagimliligi yok.** Tampon `subscribe`/`getSnapshot` sunar; React
 *   tarafi bunu `useSyncExternalStore` ile okur. Logger UI'i tanimaz.
 */

import { LOG_LEVELS, type LogLevel } from '../config/frontend-config';

export { LOG_LEVELS, type LogLevel };

/**
 * Siddet sirasi: kucuk sayi daha kritik. Bir seviye, kendisi ve daha kritik
 * olanlari gecirir (`info` secildiyse `debug` elenir).
 */
const LEVEL_RANK: Readonly<Record<LogLevel, number>> = {
  error: 0,
  warn: 1,
  info: 2,
  debug: 3,
};

/**
 * `candidate` seviyesi, `threshold` esiginde gorunur mu.
 * Hem logger filtresi hem debug paneli filtresi ayni fonksiyonu kullanir.
 */
export function isLevelEnabledFor(candidate: LogLevel, threshold: LogLevel): boolean {
  return LEVEL_RANK[candidate] <= LEVEL_RANK[threshold];
}

/** Log satirinda secret yerine basilan sabit. */
export const REDACTED = '<redacted>';

/** Ring buffer varsayilan kapasitesi (satir). */
export const DEFAULT_LOG_BUFFER_CAPACITY = 500;

/** Redaksiyonun ic ice inecegi azami derinlik; daha derini kesilir. */
const MAX_REDACTION_DEPTH = 6;

/** Redakte edilmis dizide tutulacak azami eleman sayisi. */
const MAX_REDACTION_ARRAY_LENGTH = 100;

// ---------------------------------------------------------------------------
// Redaksiyon
// ---------------------------------------------------------------------------

/**
 * Kalici API key prefix'i (`sk-`) ve ephemeral Realtime token prefix'i (`ek_`).
 * Rust tarafindaki `redact_secrets` ile ayni kume — iki sinirda ayni davranis
 * (`src-tauri/src/realtime_token.rs`).
 */
const SECRET_VALUE_PREFIXES = ['sk-', 'ek_'] as const;

/** Token karakterleri: harf, rakam, `-`, `_`. Digerleri sinir sayilir. */
const TOKEN_WORD_PATTERN = /[A-Za-z0-9_-]+/g;

/**
 * Adi tam olarak eslesirse degeri her kosulda maskelenen alanlar.
 *
 * `value` bilerek listede: Rust tarafindaki `EphemeralToken` alan adi budur
 * (`{ value, expiresAt, model }`) ve token'in log'a en olasi giris kapisidir.
 * Karsiligi, adi `value` olan zararsiz alanlarin da maskelenmesidir; bu takas
 * bilincli — okunabilirlik ugruna sizinti riski alinmaz.
 */
const EXACT_SENSITIVE_KEYS: ReadonlySet<string> = new Set([
  'apikey',
  'auth',
  'authorization',
  'cookie',
  'credential',
  'credentials',
  'key',
  'password',
  'secret',
  'token',
  'value',
]);

/**
 * Alan adinin *icinde* gecmesi yeterli olan guclu isaretciler.
 * `tokenCount` gibi zararsiz alanlar bu listeye takilmaz (bkz. test).
 */
const SENSITIVE_KEY_FRAGMENTS: readonly string[] = [
  'accesstoken',
  'apikey',
  'authorization',
  'bearer',
  'clientsecret',
  'credential',
  'ephemeraltoken',
  'password',
  'privatekey',
  'refreshtoken',
  'secret',
  'sessiontoken',
];

function normalizeKey(key: string): string {
  return key.toLowerCase().replace(/[^a-z0-9]/g, '');
}

/** Bir alan adinin degeri log'a girmeden maskelenmesi gerekiyor mu. */
export function isSensitiveKey(key: string): boolean {
  const normalized = normalizeKey(key);
  if (EXACT_SENSITIVE_KEYS.has(normalized)) {
    return true;
  }
  return SENSITIVE_KEY_FRAGMENTS.some((fragment) => normalized.includes(fragment));
}

/**
 * Metindeki `sk-...` / `ek_...` gorunumlu her parcayi maskeler.
 *
 * Son savunma hatti: cagiran taraf bir hata mesajini oldugu gibi loglasa bile
 * icindeki anahtar log'a dusmez.
 */
export function redactText(input: string): string {
  return input.replace(TOKEN_WORD_PATTERN, (word) => {
    const prefix = SECRET_VALUE_PREFIXES.find((candidate) => word.startsWith(candidate));
    return prefix === undefined ? word : `${prefix}${REDACTED}`;
  });
}

function redactUnknown(value: unknown, depth: number, seen: WeakSet<object>): unknown {
  if (typeof value === 'string') {
    return redactText(value);
  }
  if (value === null || typeof value === 'number' || typeof value === 'boolean') {
    return value;
  }
  if (value === undefined) {
    return undefined;
  }
  if (typeof value === 'bigint') {
    return `${value.toString()}n`;
  }
  if (typeof value === 'function' || typeof value === 'symbol') {
    return `<${typeof value}>`;
  }

  // Buradan sonrasi nesne: derinlik ve dongu korumasi.
  if (depth >= MAX_REDACTION_DEPTH) {
    return '<depth-limit>';
  }
  if (seen.has(value)) {
    return '<circular>';
  }
  seen.add(value);

  if (value instanceof Date) {
    return value.toISOString();
  }
  if (value instanceof Error) {
    // GUVENLIK: stack bilerek disarida — dosya yolu/ic detay sizdirmaz
    // (conventions.md "Hata Yonetimi").
    return { name: value.name, message: redactText(value.message) };
  }
  if (Array.isArray(value)) {
    const items = value
      .slice(0, MAX_REDACTION_ARRAY_LENGTH)
      .map((item) => redactUnknown(item, depth + 1, seen));
    if (value.length > MAX_REDACTION_ARRAY_LENGTH) {
      items.push(`<${(value.length - MAX_REDACTION_ARRAY_LENGTH).toString()} more>`);
    }
    return items;
  }

  const output: Record<string, unknown> = {};
  for (const [key, entryValue] of Object.entries(value)) {
    output[key] = isSensitiveKey(key) ? REDACTED : redactUnknown(entryValue, depth + 1, seen);
  }
  return output;
}

/** Herhangi bir degeri log'a yazilabilir, redakte edilmis bir kopyaya cevirir. */
export function redactValue(value: unknown): unknown {
  return redactUnknown(value, 0, new WeakSet());
}

/** Log verisi objesini redakte eder (hassas alan adlari + secret gorunumlu degerler). */
export function redactData(data: Readonly<Record<string, unknown>>): Record<string, unknown> {
  const output: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    output[key] = isSensitiveKey(key) ? REDACTED : redactUnknown(value, 1, new WeakSet());
  }
  return output;
}

// ---------------------------------------------------------------------------
// Log kaydi
// ---------------------------------------------------------------------------

/** Tamponda tutulan tek satir. Icerigi zaten redakte edilmistir. */
export interface LogEntry {
  readonly level: LogLevel;
  /** UTC ISO-8601 (conventions.md zaman kurali). */
  readonly at: string;
  /** Kaynak alt sistem: `voice-state`, `realtime`, `config`... */
  readonly scope: string;
  readonly message: string;
  /** Yapisal ek veri; yoksa `null` (`exactOptionalPropertyTypes` ile bos alan belirsizligi olmaz). */
  readonly data: Readonly<Record<string, unknown>> | null;
}

/** Log satirini tuketen hedef (konsol, tampon, ileride dosya). */
export type LogSink = (entry: LogEntry) => void;

/** `2026-08-24T12:10:01.000Z` -> `12:10:01` (UTC). */
export function formatClockTime(isoTimestamp: string): string {
  const time = isoTimestamp.slice(11, 19);
  return time.length === 8 ? time : isoTimestamp;
}

/** PROJECT.md Bolum 29'a yakin tek satirlik metin bicimi. */
export function formatLogEntry(entry: LogEntry): string {
  const head = `${formatClockTime(entry.at)} ${entry.level.toUpperCase().padEnd(5)} [${entry.scope}]`;
  if (entry.data === null) {
    return `${head} ${entry.message}`;
  }
  return `${head} ${entry.message} ${safeStringify(entry.data)}`;
}

function safeStringify(data: Readonly<Record<string, unknown>>): string {
  try {
    // Nesne kokunde `JSON.stringify` her zaman string doner; `undefined` yalnizca
    // fonksiyon/undefined kokler icin olurdu.
    return JSON.stringify(data);
  } catch {
    // Serilestirilemeyen veri (BigInt vb.) log'u dusurmemeli.
    return '<unserializable>';
  }
}

// ---------------------------------------------------------------------------
// Ring buffer
// ---------------------------------------------------------------------------

const EMPTY_SNAPSHOT: readonly LogEntry[] = Object.freeze([]);

/**
 * Son N log satirini tutan halka tampon.
 *
 * `getSnapshot()` ayni icerik icin **ayni referansi** dondurur; React'in
 * `useSyncExternalStore` sozlesmesi bunu gerektirir (aksi halde sonsuz render).
 */
export class LogRingBuffer {
  private readonly entries: LogEntry[] = [];

  private readonly listeners = new Set<() => void>();

  private snapshotCache: readonly LogEntry[] | null = EMPTY_SNAPSHOT;

  public constructor(public readonly capacity: number = DEFAULT_LOG_BUFFER_CAPACITY) {
    if (!Number.isInteger(capacity) || capacity <= 0) {
      throw new RangeError('LogRingBuffer kapasitesi pozitif tam sayi olmali.');
    }
  }

  public get size(): number {
    return this.entries.length;
  }

  public push(entry: LogEntry): void {
    this.entries.push(entry);
    if (this.entries.length > this.capacity) {
      this.entries.splice(0, this.entries.length - this.capacity);
    }
    this.snapshotCache = null;
    this.notify();
  }

  /** Degismez goruntu — cagiran taraf tamponu mutasyona ugratamaz. */
  public getSnapshot(): readonly LogEntry[] {
    this.snapshotCache ??= Object.freeze([...this.entries]);
    return this.snapshotCache;
  }

  public clear(): void {
    if (this.entries.length === 0) {
      return;
    }
    this.entries.length = 0;
    this.snapshotCache = EMPTY_SNAPSHOT;
    this.notify();
  }

  /** @returns aboneligi kaldiran fonksiyon. */
  public subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return (): void => {
      this.listeners.delete(listener);
    };
  }

  private notify(): void {
    for (const listener of [...this.listeners]) {
      try {
        listener();
      } catch (error) {
        // Bozuk bir debug paneli log zincirini dusurmemeli; ama hata da
        // sessizce yutulmaz (conventions.md). Logger'a geri donmek ozyineleme
        // uretecegi icin dogrudan konsola yazilir.
        console.error('[asuna] log abonesi hata verdi:', redactValue(error));
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Logger
// ---------------------------------------------------------------------------

/** Konsola yazan sink. Uretimde de acik kalir; icerigi zaten redaktedir. */
export function createConsoleSink(): LogSink {
  return (entry: LogEntry): void => {
    const line = formatLogEntry(entry);
    switch (entry.level) {
      case 'error':
        console.error(line);
        return;
      case 'warn':
        console.warn(line);
        return;
      case 'info':
        console.info(line);
        return;
      case 'debug':
        console.debug(line);
        return;
    }
  };
}

/** Ust logger ile cocuklari arasinda paylasilan, calisma aninda degisebilen seviye. */
interface LevelRef {
  level: LogLevel;
}

export interface LoggerOptions {
  readonly level?: LogLevel;
  readonly scope?: string;
  readonly sinks?: readonly LogSink[];
  readonly buffer?: LogRingBuffer | null;
  /** Zaman kaynagi — testte deterministik kilmak icin enjekte edilir. */
  readonly now?: () => Date;
}

/**
 * Seviyeli logger.
 *
 * Seviye `ASUNA_LOG_LEVEL` config'inden gelir ([`applyConfigLogLevel`]); config
 * asenkron yuklendigi icin acilista guvenli bir varsayilanla calisir.
 */
export class AsunaLogger {
  /** `child()` icinde ust logger'in referansiyla degistirilir; disari sizmaz. */
  private levelRef: LevelRef;

  private readonly scope: string;

  private readonly sinks: readonly LogSink[];

  private readonly buffer: LogRingBuffer | null;

  private readonly now: () => Date;

  public constructor(options: LoggerOptions = {}) {
    this.levelRef = { level: options.level ?? defaultLogLevel() };
    this.scope = options.scope ?? 'asuna';
    this.sinks = options.sinks ?? [];
    this.buffer = options.buffer ?? null;
    this.now = options.now ?? ((): Date => new Date());
  }

  public getLevel(): LogLevel {
    return this.levelRef.level;
  }

  /** Seviyeyi degistirir; ayni koke bagli tum `child` logger'lari da etkilenir. */
  public setLevel(level: LogLevel): void {
    this.levelRef.level = level;
  }

  public isEnabled(level: LogLevel): boolean {
    return isLevelEnabledFor(level, this.levelRef.level);
  }

  /** Ayni tampon/sink/seviyeyi paylasan, farkli `scope` etiketli logger. */
  public child(scope: string): AsunaLogger {
    const forked = new AsunaLogger({
      scope,
      sinks: this.sinks,
      buffer: this.buffer,
      now: this.now,
    });
    // Seviye referansini paylas: `setLevel` tek noktadan tum agaci gunceller.
    forked.levelRef = this.levelRef;
    return forked;
  }

  public error(message: string, data?: Readonly<Record<string, unknown>>): void {
    this.log('error', message, data);
  }

  public warn(message: string, data?: Readonly<Record<string, unknown>>): void {
    this.log('warn', message, data);
  }

  public info(message: string, data?: Readonly<Record<string, unknown>>): void {
    this.log('info', message, data);
  }

  public debug(message: string, data?: Readonly<Record<string, unknown>>): void {
    this.log('debug', message, data);
  }

  public log(level: LogLevel, message: string, data?: Readonly<Record<string, unknown>>): void {
    if (!this.isEnabled(level)) {
      return;
    }

    const entry: LogEntry = {
      level,
      at: this.now().toISOString(),
      scope: this.scope,
      message: redactText(message),
      data: data === undefined ? null : redactData(data),
    };

    this.buffer?.push(entry);
    for (const sink of this.sinks) {
      sink(entry);
    }
  }
}

function defaultLogLevel(): LogLevel {
  // Gelistirmede ayrinti isteriz; uretimde varsayilan `ASUNA_LOG_LEVEL=info`
  // (PROJECT.md Bolum 23) ile ayni.
  return import.meta.env.DEV ? 'debug' : 'info';
}

// ---------------------------------------------------------------------------
// Uygulama capinda tek ornek
// ---------------------------------------------------------------------------

/** Debug panelinin (ASU-019) besledigi tampon. */
export const logBuffer = new LogRingBuffer(DEFAULT_LOG_BUFFER_CAPACITY);

/** Uygulama logger'i. Alt sistemler `logger.child('realtime')` ile dallanir. */
export const logger = new AsunaLogger({
  scope: 'asuna',
  buffer: logBuffer,
  sinks: [createConsoleSink()],
});

/** `ASUNA_LOG_LEVEL` config'ini logger'a uygular (ASU-009 config servisi ile). */
export function applyConfigLogLevel(config: { readonly logLevel: LogLevel }): void {
  logger.setLevel(config.logLevel);
}

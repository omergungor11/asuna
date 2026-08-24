/**
 * IPC sinirindan gelen payload'lari dogrulamak icin ortak yardimcilar (ASU-030).
 *
 * IPC'den gelen her sey **harici veridir**: tip *iddia* edilmez, dogrulanir
 * (`tsconfig` `strict` + `noUncheckedIndexedAccess` bunu zaten zorluyor).
 *
 * GUVENLIK: hicbir hata mesaji **gelen degeri** tekrarlamaz — yalnizca alan
 * adini ve beklenen bicimi soyler. Bir hafiza kaydinin icerigi kullanicinin
 * en mahrem verisi olabilir ve hata mesajlari log'a/UI'a duser.
 */

/** Sozlesme ihlali. Alt tipler `name` alanini ezerek kendini tanitir. */
export class ContractError extends Error {
  public override readonly name: string = 'ContractError';
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * `snake_case` → `camelCase`. Sema kolon adlarini TS alan adlarina cevirir;
 * donusum repository/sozlesme sinirinda kalir (conventions.md "Database").
 */
export function toCamelCase(value: string): string {
  return value.replace(/_([a-z0-9])/g, (_match, char: string) => char.toUpperCase());
}

/**
 * UTC ISO-8601. Ayni kural DB'de de `GLOB` ile zorlanir; buradaki bicim daha
 * dardir (yerel offset ya da epoch kabul edilmez) — Stage A siralamasi metin
 * siralamasi oldugu icin karisik bicim sessizce yanlis sonuc uretir.
 */
const UTC_ISO_8601 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$/;

export function isUtcIso8601(value: string): boolean {
  return UTC_ISO_8601.test(value);
}

type Fail = (field: string, expected: string) => never;

/**
 * Alan okuyuculari. Hepsi ayni `fail` fonksiyonunu paylasir; boylece hata
 * mesajinin bicimi ve hangi hata sinifinin firlatildigi cagiran modulun
 * elinde kalir (`MemoryContractError`, `SessionContractError`, ...).
 */
export interface ContractReaders {
  /** Bos olmayan string. */
  text(field: string): string;
  nullableText(field: string): string | null;
  /** UTC ISO-8601 zaman damgasi. */
  timestamp(field: string): string;
  nullableTimestamp(field: string): string | null;
  /** Negatif olmayan tam sayi (`0` gecerli). */
  count(field: string): number;
  nullableCount(field: string): number | null;
  /** SQLite rowid — pozitif tam sayi. */
  id(field: string): number;
  nullableId(field: string): number | null;
  /** `[0, 1]` araligi — `importance` / `confidence` (PROJECT.md Bolum 26). */
  unitInterval(field: string): number;
  /** Negatif olmayan ondalik (maliyet). */
  nullableAmount(field: string): number | null;
  boolean(field: string): boolean;
  enumeration<T extends string>(field: string, allowed: readonly T[]): T;
  /** Gecerli JSON metni (`metadata_json`, `usage_json`). */
  jsonText(field: string): string;
  nullableJsonText(field: string): string | null;
}

export function readers(source: Record<string, unknown>, fail: Fail): ContractReaders {
  const nullable = <T>(field: string, read: () => T): T | null =>
    source[field] === null ? null : read();

  return {
    /** Bos olmayan string. */
    text(field: string): string {
      const value = source[field];
      if (typeof value !== 'string' || value.length === 0) {
        fail(field, 'bos olmayan bir string');
      }
      return value;
    },

    nullableText(field: string): string | null {
      return nullable(field, () => this.text(field));
    },

    /** UTC ISO-8601 zaman damgasi. */
    timestamp(field: string): string {
      const value = this.text(field);
      if (!isUtcIso8601(value)) {
        fail(field, 'UTC ISO-8601 zaman damgasi (orn. 2026-08-25T10:00:00Z)');
      }
      return value;
    },

    nullableTimestamp(field: string): string | null {
      return nullable(field, () => this.timestamp(field));
    },

    /** Negatif olmayan tam sayi. */
    count(field: string): number {
      const value = source[field];
      if (typeof value !== 'number' || !Number.isInteger(value) || value < 0) {
        fail(field, 'negatif olmayan tam sayi');
      }
      return value;
    },

    nullableCount(field: string): number | null {
      return nullable(field, () => this.count(field));
    },

    /** SQLite rowid — pozitif tam sayi. */
    id(field: string): number {
      const value = source[field];
      if (typeof value !== 'number' || !Number.isInteger(value) || value <= 0) {
        fail(field, 'pozitif tam sayi');
      }
      return value;
    },

    nullableId(field: string): number | null {
      return nullable(field, () => this.id(field));
    },

    /** `[0, 1]` araligi — `importance` / `confidence` (PROJECT.md Bolum 26). */
    unitInterval(field: string): number {
      const value = source[field];
      if (typeof value !== 'number' || !Number.isFinite(value) || value < 0 || value > 1) {
        fail(field, '0 ile 1 arasi (dahil) ondalik sayi');
      }
      return value;
    },

    nullableAmount(field: string): number | null {
      return nullable(field, () => {
        const value = source[field];
        if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
          fail(field, 'negatif olmayan sayi');
        }
        return value;
      });
    },

    boolean(field: string): boolean {
      const value = source[field];
      if (typeof value !== 'boolean') {
        fail(field, 'boolean');
      }
      return value;
    },

    enumeration<T extends string>(field: string, allowed: readonly T[]): T {
      const value = source[field];
      if (typeof value !== 'string' || !(allowed as readonly string[]).includes(value)) {
        fail(field, `su degerlerden biri: ${allowed.join(', ')}`);
      }
      return value as T;
    },

    /** Gecerli JSON metni (`metadata_json`, `usage_json`). */
    jsonText(field: string): string {
      const value = this.text(field);
      try {
        JSON.parse(value);
      } catch {
        fail(field, 'gecerli JSON metni');
      }
      return value;
    },

    nullableJsonText(field: string): string | null {
      return nullable(field, () => this.jsonText(field));
    },
  };
}

/**
 * Whitelist: sozlesmede olmayan bir alan **reddedilir**.
 *
 * Sadece tip hijyeni degil — backend bir gun yanlislikla fazladan bir alan
 * dondurse (orn. `embedding`), bu sessizce renderer'a akmak yerine gurultulu
 * bir hataya donusur.
 */
export function assertNoUnexpectedKeys(
  source: Record<string, unknown>,
  allowed: readonly string[],
  fail: (message: string) => never,
): void {
  const unexpected = Object.keys(source).filter((key) => !allowed.includes(key));
  if (unexpected.length > 0) {
    fail(`Payload beklenmeyen alan(lar) iceriyor: ${unexpected.join(', ')}.`);
  }
}

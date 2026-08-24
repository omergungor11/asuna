/**
 * Sema ↔ TypeScript tip aynasi senkron testi (ASU-030 kabul kriteri:
 * "TypeScript tipleri schema ile tek kaynaktan turetiliyor — elle senkronize
 * edilen ikinci tanim yok").
 *
 * # Neden kod uretimi degil
 *
 * Uretilmis bir `.ts` dosyasi commit edilseydi, uretici calistirilmadan
 * yapilan bir sema degisikligi yine sessizce kayardi (uretimin kendisi de bir
 * gate ister). Bunun yerine **tek kaynak dogrudan `.sql` dosyasidir** ve uc
 * tuketici de ona testle baglanir:
 *
 * | Tuketici | Bag |
 * |---|---|
 * | SQLite | DDL'in kendisi |
 * | Rust (`db/model.rs`) | `PRAGMA table_info` + `kinds_declared_in_schema()` karsilastirmasi |
 * | TypeScript (`shared/*.ts`) | **bu dosya** |
 *
 * Sonuc: bir kolon eklemek, silmek, yeniden adlandirmak ya da bir `kind`
 * degeri eklemek — dokunulmayan her katmanda kirmizi test uretir.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { cwd } from 'node:process';

import { describe, expect, it } from 'vitest';

import { toCamelCase } from './contract';
import { MEMORY_COLUMNS_NOT_MIRRORED, MEMORY_KINDS, MEMORY_RECORD_KEYS } from './memory';
import { SESSION_END_REASONS, SESSION_RECORD_KEYS } from './session';

const MIGRATIONS_DIR = 'src-tauri/src/db/migrations';

/**
 * Tek kaynak: migration'lar **sirasiyla**.
 *
 * Bu listeye yeni bir migration eklendiginde kolon aynasi otomatik olarak
 * guncel kalir; eklenmezse (yeni bir `.sql` yazilip buraya konmazsa) sema ile
 * TypeScript arasindaki fark bir sonraki kolon degisikliginde kirmizi test
 * uretir. Yol degisirse dosya okunamaz ve test duser — sessiz kayma yok.
 */
const MIGRATION_FILES = [
  '001_memories_sessions.up.sql',
  '002_session_end_reason.up.sql',
] as const;

function readMigration(name: string): string {
  return readFileSync(resolve(cwd(), MIGRATIONS_DIR, name), 'utf8');
}

/** Tum migration'larin metni, sirayla birlestirilmis. */
const schema = MIGRATION_FILES.map(readMigration).join('\n');

/** `CREATE TABLE <name> ( ... ) STRICT;` blogundaki kolon adlari, sirasiyla. */
function createdColumnsOf(table: string): string[] {
  const start = schema.indexOf(`CREATE TABLE ${table} (`);
  expect(start, `\`${table}\` tablosu semada bulunmali`).toBeGreaterThanOrEqual(0);

  const body = schema.slice(start);
  const end = body.indexOf(') STRICT;');
  expect(end, `\`${table}\` tablosu \`) STRICT;\` ile kapanmali`).toBeGreaterThan(0);

  return body
    .slice(body.indexOf('(') + 1, end)
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith('--'))
    .map((line) => line.split(/\s+/)[0] ?? '')
    .filter((name) => /^[a-z][a-z0-9_]*$/.test(name));
}

/**
 * Bir tablonun **guncel** kolonlari: `CREATE TABLE` + sonraki migration'larin
 * `ALTER TABLE ... ADD COLUMN`'lari, uygulanma sirasiyla.
 *
 * Sira onemli: SQLite `ADD COLUMN` ile gelen kolonu tablonun **sonuna** koyar
 * (`PRAGMA table_info` sirasi budur) ve Rust `SESSION_COLUMNS` ile TypeScript
 * `SESSION_RECORD_KEYS` bu siraya gore yazilmistir.
 */
function columnsOf(table: string): string[] {
  const added = [
    ...schema.matchAll(new RegExp(`ALTER TABLE ${table} ADD COLUMN\\s+([a-z][a-z0-9_]*)`, 'g')),
  ].map((match) => match[1] ?? '');

  return [...createdColumnsOf(table), ...added];
}

/** Bir CHECK kisitindaki (`... IN ('a', 'b')`) degerler. */
function valuesInCheck(marker: string): string[] {
  const start = schema.indexOf(marker);
  expect(start, `\`${marker}\` kisiti semada bulunmali`).toBeGreaterThanOrEqual(0);

  const rest = schema.slice(start + marker.length);
  const end = rest.indexOf(')');
  return [...rest.slice(0, end).matchAll(/'([^']+)'/g)].map((match) => match[1] ?? '');
}

/** `memories.kind` CHECK kisitindaki degerler. */
function kindsDeclaredInSchema(): string[] {
  return valuesInCheck('CHECK (kind IN (');
}

describe('memories tablosu <-> src/shared/memory.ts', () => {
  it('kolon adlari sozlesme alanlariyla birebir esleisiyor (sira dahil)', () => {
    const mirrored = columnsOf('memories').filter(
      (column) => !(MEMORY_COLUMNS_NOT_MIRRORED as readonly string[]).includes(column),
    );

    expect(mirrored.map(toCamelCase)).toEqual([...MEMORY_RECORD_KEYS]);
  });

  /**
   * Istisna listesi gercekten var olan bir kolonu tarif etmeli; yoksa
   * "unuttugumuz kolon" ile "bilerek disarida biraktigimiz kolon" ayrimi
   * anlamini kaybeder.
   */
  it('sozlesme disi birakilan kolonlar semada gercekten var', () => {
    const columns = columnsOf('memories');
    for (const column of MEMORY_COLUMNS_NOT_MIRRORED) {
      expect(columns).toContain(column);
    }
    expect([...MEMORY_COLUMNS_NOT_MIRRORED]).toEqual(['embedding']);
  });

  it('kind degerleri semadaki CHECK kisitiyla birebir', () => {
    expect(kindsDeclaredInSchema()).toEqual([...MEMORY_KINDS]);
  });

  /** PROJECT.md Bolum 5.3'teki on tip — kaynak spec ile de bagli kalsin. */
  it('PROJECT.md Bolum 5.3 listesindeki on tipi tasiyor', () => {
    expect(MEMORY_KINDS).toHaveLength(10);
    expect([...MEMORY_KINDS]).toEqual([
      'profile',
      'preference',
      'project',
      'decision',
      'task',
      'working_context',
      'relationship',
      'idea',
      'routine',
      'tool_state',
    ]);
  });
});

describe('sessions tablosu <-> src/shared/session.ts', () => {
  /** `ALTER TABLE ... ADD COLUMN` ile eklenen kolonlar dahil (ASU-033). */
  it('kolon adlari sozlesme alanlariyla birebir esleisiyor (sira dahil)', () => {
    expect(columnsOf('sessions').map(toCamelCase)).toEqual([...SESSION_RECORD_KEYS]);
  });

  it('sonraki migration"larla eklenen kolonlar da aynada', () => {
    expect(columnsOf('sessions')).toContain('end_reason');
    expect(createdColumnsOf('sessions')).not.toContain('end_reason');
  });

  it('endReason degerleri semadaki CHECK kisitiyla birebir', () => {
    expect(valuesInCheck('end_reason IN (')).toEqual([...SESSION_END_REASONS]);
  });
});

describe('sema disiplini', () => {
  /**
   * ASU-030 kabul kriteri: `project_id` alanlari simdilik nullable ve FK'siz;
   * Phase 4 migration plani semada **not edilmis** olmali.
   */
  it('project_id icin Phase 4 FK plani semada not edilmis', () => {
    expect(schema).toContain('ASU-039');
    expect(schema).toMatch(/project_id\s+TEXT/);
    expect(schema).not.toMatch(/project_id\s+TEXT[^\n]*REFERENCES/);
  });

  /** Sorgu icin gerekli index'ler (kabul kriteri). */
  it('gerekli index"ler tanimli', () => {
    for (const index of [
      'idx_memories_kind',
      'idx_memories_project_id',
      'idx_memories_importance',
      'idx_memories_is_archived',
      'idx_memories_created_at',
    ]) {
      expect(schema).toContain(index);
    }
  });

  /** Her `up` icin bir `down` (ADR-005). */
  it('her migration"in geri almasi mevcut', () => {
    for (const file of MIGRATION_FILES) {
      const down = readMigration(file.replace('.up.sql', '.down.sql'));
      expect(down.trim().length, `\`${file}\` icin down bos`).toBeGreaterThan(0);
    }

    const first = readMigration('001_memories_sessions.down.sql');
    expect(first).toContain('DROP TABLE IF EXISTS memories;');
    expect(first).toContain('DROP TABLE IF EXISTS sessions;');
    // FK yonunun tersi: once referans veren.
    expect(first.indexOf('DROP TABLE IF EXISTS memories;')).toBeLessThan(
      first.indexOf('DROP TABLE IF EXISTS sessions;'),
    );

    expect(readMigration('002_session_end_reason.down.sql')).toContain(
      'ALTER TABLE sessions DROP COLUMN end_reason;',
    );
  });

  /**
   * ASU-030 kurali: yayinlanmis bir migration bir daha **degistirilmez**;
   * duzeltme yeni bir dosya ekler. Bu test yalnizca kuralin dosyada yazili
   * kalmasini garanti eder — insan hatirlatmasi da bir gate.
   */
  it('degismezlik kurali migration dosyalarinda yazili', () => {
    for (const file of MIGRATION_FILES) {
      expect(readMigration(file)).toContain('BIR DAHA DEGISTIRILMEZ');
    }
  });
});

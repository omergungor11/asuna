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
import { SESSION_RECORD_KEYS } from './session';

/** Tek kaynak. Yol degisirse test dosyayi bulamaz ve duser — sessiz kayma yok. */
const SCHEMA_PATH = resolve(cwd(), 'src-tauri/src/db/migrations/001_memories_sessions.up.sql');

const schema = readFileSync(SCHEMA_PATH, 'utf8');

/** `CREATE TABLE <name> ( ... ) STRICT;` blogundaki kolon adlari, sirasiyla. */
function columnsOf(table: string): string[] {
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

/** `memories.kind` CHECK kisitindaki degerler. */
function kindsDeclaredInSchema(): string[] {
  const marker = 'CHECK (kind IN (';
  const start = schema.indexOf(marker);
  expect(start, '`kind` CHECK kisiti semada bulunmali').toBeGreaterThanOrEqual(0);

  const rest = schema.slice(start + marker.length);
  const end = rest.indexOf(')');
  return [...rest.slice(0, end).matchAll(/'([^']+)'/g)].map((match) => match[1] ?? '');
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
  it('kolon adlari sozlesme alanlariyla birebir esleisiyor (sira dahil)', () => {
    expect(columnsOf('sessions').map(toCamelCase)).toEqual([...SESSION_RECORD_KEYS]);
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
  it('geri alma migration"i mevcut', () => {
    const down = readFileSync(
      resolve(cwd(), 'src-tauri/src/db/migrations/001_memories_sessions.down.sql'),
      'utf8',
    );
    expect(down).toContain('DROP TABLE IF EXISTS memories;');
    expect(down).toContain('DROP TABLE IF EXISTS sessions;');
    // FK yonunun tersi: once referans veren.
    expect(down.indexOf('DROP TABLE IF EXISTS memories;')).toBeLessThan(
      down.indexOf('DROP TABLE IF EXISTS sessions;'),
    );
  });
});

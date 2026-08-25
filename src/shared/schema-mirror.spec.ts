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
import { PROJECT_RECORD_KEYS, PROJECT_STATUSES } from './project';
import { SESSION_END_REASONS, SESSION_RECORD_KEYS } from './session';
import { TOOL_APPROVAL_STATES, TOOL_EVENT_RECORD_KEYS, TOOL_RISK_LEVELS } from './tool-event';

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
  '003_projects.up.sql',
  '004_tool_events.up.sql',
] as const;

function readMigration(name: string): string {
  return readFileSync(resolve(cwd(), MIGRATIONS_DIR, name), 'utf8');
}

/** Tum migration'larin metni, sirayla birlestirilmis. */
const schema = MIGRATION_FILES.map(readMigration).join('\n');

/**
 * Bir tablonun **son** `CREATE TABLE` blogunun basladigi konum.
 *
 * `lastIndexOf`: SQLite'ta bir tabloya FK eklemenin tek yolu onu yeniden
 * yaratmaktir (003, `project_id` -> `projects.id`). Yani `memories` ve
 * `sessions` icin semada birden fazla `CREATE TABLE` blogu var ve gecerli olan
 * **sonuncusu**. `indexOf` kullanmak, aynayi 001'deki (artik gecmis) tanima
 * baglar ve bir yeniden yaratmada dusen kolonu sessizce kacirir.
 */
function lastCreateOf(table: string): number {
  const start = schema.lastIndexOf(`CREATE TABLE ${table} (`);
  expect(start, `\`${table}\` tablosu semada bulunmali`).toBeGreaterThanOrEqual(0);
  return start;
}

/** `CREATE TABLE <name> ( ... ) STRICT;` blogundaki kolon adlari, sirasiyla. */
function createdColumnsOf(table: string): string[] {
  const body = schema.slice(lastCreateOf(table));
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
 * Bir tablonun **guncel** kolonlari: son `CREATE TABLE` + ondan **sonra** gelen
 * `ALTER TABLE ... ADD COLUMN`'lar, uygulanma sirasiyla.
 *
 * Sira onemli: SQLite `ADD COLUMN` ile gelen kolonu tablonun **sonuna** koyar
 * (`PRAGMA table_info` sirasi budur) ve Rust `SESSION_COLUMNS` ile TypeScript
 * `SESSION_RECORD_KEYS` bu siraya gore yazilmistir.
 *
 * `ADD COLUMN`'lar son `CREATE TABLE`'dan once kaldiysa (002'nin `end_reason`'i
 * gibi) tekrar sayilmaz: yeniden yaratma o kolonu zaten govdeye almistir.
 */
function columnsOf(table: string): string[] {
  const added = [
    ...schema
      .slice(lastCreateOf(table))
      .matchAll(new RegExp(`ALTER TABLE ${table} ADD COLUMN\\s+([a-z][a-z0-9_]*)`, 'g')),
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

  /**
   * `end_reason` 002'de `ADD COLUMN` ile geldi, 003'te tablo yeniden
   * yaratilirken govdeye **sonda** yazildi. Ikisi de tablonun sonunu gosterir;
   * ayna sirasi degismedi.
   */
  it('ADD COLUMN ile gelen kolon yeniden yaratmadan sonra da sonda', () => {
    const columns = columnsOf('sessions');
    expect(columns).toContain('end_reason');
    expect(columns.at(-1)).toBe('end_reason');
  });

  it('endReason degerleri semadaki CHECK kisitiyla birebir', () => {
    expect(valuesInCheck('end_reason IN (')).toEqual([...SESSION_END_REASONS]);
  });
});

describe('projects tablosu <-> src/shared/project.ts', () => {
  it('kolon adlari sozlesme alanlariyla birebir esleisiyor (sira dahil)', () => {
    expect(columnsOf('projects').map(toCamelCase)).toEqual([...PROJECT_RECORD_KEYS]);
  });

  it('status degerleri semadaki CHECK kisitiyla birebir', () => {
    expect(valuesInCheck('status IN (')).toEqual([...PROJECT_STATUSES]);
  });

  /** PROJECT.md Bolum 12.2'deki alan listesi — kaynak spec ile de bagli kalsin. */
  it('PROJECT.md Bolum 12.2 alanlarinin tamamini tasiyor', () => {
    for (const field of [
      'id',
      'name',
      'path',
      'description',
      'status',
      'primary_language',
      'framework',
      'git_remote',
      'last_opened_at',
      'created_at',
      'updated_at',
      'metadata_json',
    ]) {
      expect(columnsOf('projects')).toContain(field);
    }
  });

  /**
   * `path` hem benzersiz hem sorgulanabilir olmali (ASU-039). Tek UNIQUE index
   * ikisini birden karsilar; ayri bir `UNIQUE` kisiti + ayri bir index ikinci
   * bir index uretirdi.
   */
  it('path benzersiz ve index"li', () => {
    expect(schema).toContain('CREATE UNIQUE INDEX idx_projects_path ON projects (path);');
  });

  /** Yolsuz kayit yalnizca `unlinked` olabilir — iki yonlu CHECK. */
  it('unlinked <=> path IS NULL kisiti semada', () => {
    expect(schema).toContain("CHECK ((status = 'unlinked') = (path IS NULL))");
  });
});

describe('tool_events tablosu <-> src/shared/tool-event.ts', () => {
  it('kolon adlari sozlesme alanlariyla birebir esleisiyor (sira dahil)', () => {
    expect(columnsOf('tool_events').map(toCamelCase)).toEqual([...TOOL_EVENT_RECORD_KEYS]);
  });

  it('approvalState degerleri semadaki CHECK kisitiyla birebir', () => {
    expect(valuesInCheck('approval_state IN (')).toEqual([...TOOL_APPROVAL_STATES]);
  });

  /**
   * Risk kumesi semada `BETWEEN` degil `IN (0, 1, 2, 3)` olarak yazili — tam da
   * bu testin okuyabilmesi icin. `BETWEEN` yazilsaydi Rust enum'u ve TypeScript
   * sabiti sema ile yalnizca yorum uzerinden bagli olurdu.
   */
  it('riskLevel kumesi semadan okunabiliyor ve sabitle ayni', () => {
    expect(schema).toContain('CHECK (risk_level IN (0, 1, 2, 3))');
    expect([...TOOL_RISK_LEVELS]).toEqual([0, 1, 2, 3]);
  });

  /** PROJECT.md Bolum 12.2 alan listesi — kaynak spec ile de bagli kalsin. */
  it('PROJECT.md Bolum 12.2 alanlarinin tamamini tasiyor', () => {
    expect(columnsOf('tool_events')).toEqual([
      'id',
      'session_id',
      'tool_name',
      'risk_level',
      'arguments_redacted',
      'approval_state',
      'result_summary',
      'created_at',
    ]);
  });

  /**
   * **ASU-050 kabul kriteri**: oturum silinince audit **kalir**.
   *
   * `ON DELETE CASCADE` yazilsaydi "konusma gecmisini sil" dugmesi ayni zamanda
   * audit defterini silen bir primitif olurdu — yani "audit kayitlari
   * uygulamadan silinemiyor" kriteri dolayli olarak delinirdi.
   */
  it('session_id bagi silinince kopar, audit satiri silinmez', () => {
    const declaration = /session_id\s+INTEGER REFERENCES sessions \(id\)[^\n]*/.exec(
      schema,
    )?.[0];
    expect(declaration).toBeDefined();
    expect(declaration).toContain('ON DELETE SET NULL');

    // Hicbir DDL satiri silmeyi yayan bir eylem tanimlamasin. Yorum satirlari
    // atiliyor: 004'un bas yorumu tam da bu karari anlatirken ifadeyi geciyor.
    const ddl = schema
      .split('\n')
      .filter((line) => !line.trim().startsWith('--'))
      .join('\n');
    expect(ddl).not.toContain('ON DELETE CASCADE');
  });

  /**
   * Uzunluk tavanlari yorum degil calisan kisit: Rust kirpmayi bir gun
   * atlarsa, bir dosya icerigi audit defterine sessizce sizmak yerine INSERT
   * aninda duser.
   */
  it('arguman ve sonuc ozetlerinin uzunluk tavani semada zorlaniyor', () => {
    expect(schema).toContain('length(arguments_redacted) <= 512');
    expect(schema).toContain('length(result_summary) <= 512');
    expect(schema).toContain('length(tool_name) <= 64');
  });

  it('audit sorgu eksenleri index"li', () => {
    for (const index of [
      'idx_tool_events_session_id',
      'idx_tool_events_created_at',
      'idx_tool_events_tool_name',
    ]) {
      expect(schema).toContain(index);
    }
  });
});

describe('sema disiplini', () => {
  /**
   * ASU-030'da birakilan plan ASU-039'da uygulandi: `project_id` artik
   * `projects(id)`'ye FK ile bagli. `ON DELETE SET NULL` sart — proje silinince
   * hafiza **silinmez**, yalnizca izi kopar (kabul kriteri).
   */
  it('project_id artik projects(id) yabanci anahtari', () => {
    const declarations = [...schema.matchAll(/project_id\s+TEXT[^\n]*/g)]
      .map((match) => match[0])
      // Son tanim gecerli: 001'deki FK'siz hali gecmis, 003'teki hali guncel.
      .slice(-2);

    expect(declarations).toHaveLength(2);
    for (const declaration of declarations) {
      expect(declaration).toContain('REFERENCES projects (id)');
      expect(declaration).toContain('ON DELETE SET NULL');
    }
  });

  /** Sorgu icin gerekli index'ler (kabul kriteri). */
  it('gerekli index"ler tanimli', () => {
    for (const index of [
      'idx_memories_kind',
      'idx_memories_project_id',
      'idx_memories_importance',
      'idx_memories_is_archived',
      'idx_memories_created_at',
      'idx_projects_path',
      'idx_projects_last_opened_at',
      'idx_projects_status',
    ]) {
      expect(schema).toContain(index);
    }
  });

  /**
   * 003 `memories` ve `sessions`i FK eklemek icin yeniden yaratti. Eski
   * kabuklar (`*_old`) migration icinde dusurulmus olmali; kalan bir kabuk
   * kullanicinin DB'sinde sessizce iki kat yer kaplardi.
   */
  it('yeniden yaratmada kullanilan gecici tablolar dusurulmus', () => {
    for (const shell of ['memories_old', 'sessions_old']) {
      expect(schema).toContain(`DROP TABLE ${shell};`);
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

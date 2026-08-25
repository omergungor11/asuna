-- ASU-039 — `projects` tablosu + `project_id` yabanci anahtarlari
-- (PROJECT.md Bolum 12.2, Phase 4)
--
-- BU DOSYA YAYINLANMISTIR VE BIR DAHA DEGISTIRILMEZ. Duzeltme yeni bir
-- migration ekler (ADR-005 "Migration Karari").
--
-- ===========================================================================
-- Neden tablo yeniden yaratiliyor
-- ===========================================================================
--
-- 001'de birakilan not: "`projects` tablosu Phase 4 (ASU-039). O migration once
-- `projects`i yaratir, sonra mevcut `project_id` degerlerini eslestirir ve
-- tabloyu yeniden yaratarak FK ekler (SQLite'ta ALTER TABLE ile FK eklenemez).
-- Eslesmeyen degerler NULL'a cekilmez, olduklari gibi birakilir — kullanici
-- verisi sessizce silinmez."
--
-- Bu dosya o plani harfiyen uyguluyor. `ALTER TABLE ... ADD CONSTRAINT` SQLite'ta
-- yok; tek yol tabloyu yeni tanimla yeniden yaratip veriyi tasimaktir.
--
-- ===========================================================================
-- Sira neden boyle (FK zorlamasi ACIKKEN de calismali)
-- ===========================================================================
--
-- Uygulama baglantisinda `PRAGMA foreign_keys = ON` (db/mod.rs). Bu, klasik
-- "tabloyu yeniden yarat" tarifini iki noktada tehlikeli yapar:
--
--   * `DROP TABLE sessions` FK acikken **ortuk bir DELETE** calistirir ve bu
--     DELETE `memories.source_session_id` uzerindeki `ON DELETE SET NULL`
--     eylemini tetikler. Yani naif bir siralama, "bu hafiza neden hatirlaniyor?"
--     baglarinin **tamamini** sessizce silerdi.
--   * Migration'lar tek bir transaction icinde kosuyor; `PRAGMA foreign_keys`
--     transaction icinde degistirilemez (sessizce yok sayilir).
--
-- Cozum: hicbir **ebeveyn** tablo, cocugu hala ona bakarken dusurulmuyor.
-- Eski tablolar once yeniden adlandiriliyor (rename veri silmez, eylem
-- tetiklemez), yeni tablolar dolduruluyor, en son eski kabuklar dusuruluyor.
-- Bu sira `foreign_keys` ACIK da KAPALI da olsa ayni son semayi uretir
-- (`Migrations::validate()` FK'siz bir baglantida kosar — ikisi de dogrulanir).
--
-- ===========================================================================
-- `unlinked` durumu — kayip veri yerine durust bir kayit
-- ===========================================================================
--
-- 001'den beri `project_id` **serbest metindi**: renderer "asuna" gibi bir
-- etiket yaziyordu. FK eklenince bu etiketlerin bir karsiligi olmali. Iki kotu
-- secenek vardi: (a) eslesmeyeni NULL'a cekmek — kullanicinin hafizasindaki
-- proje bagini silmek, (b) FK'yi hic eklememek.
--
-- Ucuncu yol secildi: her etiket icin `status = 'unlinked'` bir proje satiri
-- acilir. Anlami net — "bu proje adi hafizada geciyor ama kayitli bir kok
-- dizini yok". `path` bu satirlarda NULL'dur ve tablo duzeyindeki CHECK bunu
-- iki yonlu zorlar (`unlinked` <=> `path IS NULL`): kayitli bir projenin yolu
-- uydurulamaz, yolsuz bir satir da "aktif" gorunemez.
--
-- ProjectRegistry (ASU-040) ayni id'ye sahip bir dizin kaydedildiginde bu
-- satiri **sahiplenir** (path + status guncellenir, id degismez). Boylece Phase
-- 3'te yazilmis hafizalar, proje ilk kez kaydedildigi anda dogru projeye
-- baglanir — yeni bir satir acilip eski hafizalar oksuz kalmaz.

-- ---------------------------------------------------------------------------
-- 1) projects
-- ---------------------------------------------------------------------------

CREATE TABLE projects (
    -- Slug (`asuna`, `realestate-pipeline-cyprus`). INTEGER degil, cunku
    -- `memories.project_id` 001'den beri TEXT ve kullanicinin verisi orada
    -- duruyor; sayisal bir id'ye gecis o veriyi tasiyamazdi.
    id               TEXT    NOT NULL PRIMARY KEY CHECK (length(id) > 0),
    name             TEXT    NOT NULL CHECK (length(name) > 0),
    -- Normalize edilmis, symlink'i cozulmus MUTLAK yol (ASU-040).
    -- `GLOB '/*'` mutlak olmayi, `NOT GLOB '*/'` sondaki egik cizginin
    -- olmamasini zorlar: ayni dizin iki farkli metinle iki kez kaydedilemesin.
    -- `length > 1` filesystem kokunun (`/`) proje olarak kaydedilmesini de
    -- keser — bir sandbox koku olarak anlamsiz ve tehlikeli olurdu.
    path             TEXT    CHECK (path IS NULL OR (length(path) > 1 AND path GLOB '/*' AND path NOT GLOB '*/')),
    description      TEXT    CHECK (description IS NULL OR length(description) > 0),
    -- active   : kayitli, yol erisilebilir
    -- missing  : kayitli ama yol artik yok (ASU-040 isaretler; kayit SILINMEZ)
    -- archived : kullanici gecmis icin tutuyor, aktif calisilmiyor
    -- unlinked : kayitli kok yok — Phase 3'ten devralinan etiket (yukariya bak)
    status           TEXT    NOT NULL CHECK (status IN ('active', 'missing', 'archived', 'unlinked')),
    primary_language TEXT    CHECK (primary_language IS NULL OR length(primary_language) > 0),
    framework        TEXT    CHECK (framework IS NULL OR length(framework) > 0),
    -- Remote **adi/host'u** (`github.com/omergungor/asuna`). Kimlik bilgisi ya
    -- da token tasiyan bir URL buraya yazilmaz (ASU-042 redaksiyondan gecirir).
    git_remote       TEXT    CHECK (git_remote IS NULL OR length(git_remote) > 0),
    -- NULL = hic acilmadi. "Guncel proje" secimi bunu gunceller (ASU-040);
    -- deger tahmine dayali degil, kullanicinin acik seciminden gelir.
    last_opened_at   TEXT    CHECK (last_opened_at IS NULL OR last_opened_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    created_at       TEXT    NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    updated_at       TEXT    NOT NULL CHECK (updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    metadata_json    TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
    -- Yolsuz kayit yalnizca `unlinked` olabilir; `unlinked` bir kayitta da yol
    -- bulunamaz. Tek yonlu bir CHECK yazsaydik "path'i olan unlinked" ya da
    -- "path'i olmayan active" satirlari sessizce olusabilirdi.
    CHECK ((status = 'unlinked') = (path IS NULL))
) STRICT;

-- `path` hem benzersiz hem sorgulanabilir olmali (ASU-039 kabul kriteri).
-- Ayri bir `UNIQUE` kisiti + ayri bir index iki index uretirdi; tek UNIQUE
-- index ikisini de karsilar. NULL'lar SQLite'ta birbirinden farkli sayilir,
-- yani birden fazla `unlinked` satir sorun degil.
CREATE UNIQUE INDEX idx_projects_path ON projects (path);
CREATE INDEX idx_projects_last_opened_at ON projects (last_opened_at DESC);
CREATE INDEX idx_projects_status ON projects (status);

-- Phase 3'ten devralinan etiketler. `UNION` zaten tekillestiriyor.
-- Zaman damgasi SQLite'ta uretiliyor cunku migration'in kendi "simdi"si var;
-- bicim semadaki GLOB ile birebir ayni (saniye hassasiyeti, `Z` sonlu).
INSERT INTO projects (id, name, path, status, created_at, updated_at)
SELECT label,
       label,
       NULL,
       'unlinked',
       strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
       strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  FROM (SELECT project_id AS label FROM memories  WHERE project_id IS NOT NULL
        UNION
        SELECT project_id AS label FROM sessions  WHERE project_id IS NOT NULL);

-- ---------------------------------------------------------------------------
-- 2) sessions — yeniden yaratma (project_id -> projects.id)
-- ---------------------------------------------------------------------------
--
-- Once rename: `memories` hala eski tabloya bakiyor ve rename hicbir satiri
-- silmez. `DROP` en sona birakiliyor (bkz. bas yorum).

ALTER TABLE sessions RENAME TO sessions_old;

CREATE TABLE sessions (
    id                 INTEGER PRIMARY KEY,
    started_at         TEXT    NOT NULL CHECK (started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    ended_at           TEXT    CHECK (ended_at IS NULL OR (ended_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z' AND ended_at >= started_at)),
    project_id         TEXT    REFERENCES projects (id) ON DELETE SET NULL ON UPDATE CASCADE CHECK (project_id IS NULL OR length(project_id) > 0),
    summary            TEXT,
    transcript_path    TEXT    CHECK (transcript_path IS NULL OR length(transcript_path) > 0),
    model              TEXT    NOT NULL CHECK (length(model) > 0),
    input_tokens       INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0),
    output_tokens      INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
    total_tokens       INTEGER CHECK (total_tokens IS NULL OR total_tokens >= 0),
    estimated_cost_usd REAL    CHECK (estimated_cost_usd IS NULL OR estimated_cost_usd >= 0.0),
    usage_json         TEXT    CHECK (usage_json IS NULL OR json_valid(usage_json)),
    created_at         TEXT    NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    -- 002'de `ALTER TABLE ADD COLUMN` ile gelmisti; yeniden yaratmada da
    -- **sonda** kaliyor ki `PRAGMA table_info` sirasi degismesin
    -- (`SESSION_COLUMNS` ve `SESSION_RECORD_KEYS` bu siraya yazili).
    end_reason         TEXT    CHECK (end_reason IS NULL OR end_reason IN ('completed', 'abandoned', 'error'))
) STRICT;

-- `ON UPDATE CASCADE`: proje slug'i degisirse (kullanici projeyi yeniden
-- adlandirirsa) bag kopmaz. `ON DELETE SET NULL`: proje kaydi silinince oturum
-- ve hafiza **silinmez**, yalnizca projeye olan izi kopar — hafizayi silme
-- yetkisi kullanicinindir (PROJECT.md Bolum 20, `memories.source_session_id`
-- ile ayni gerekce).

INSERT INTO sessions (id, started_at, ended_at, project_id, summary, transcript_path,
                      model, input_tokens, output_tokens, total_tokens,
                      estimated_cost_usd, usage_json, created_at, end_reason)
SELECT id, started_at, ended_at, project_id, summary, transcript_path,
       model, input_tokens, output_tokens, total_tokens,
       estimated_cost_usd, usage_json, created_at, end_reason
  FROM sessions_old;

-- ---------------------------------------------------------------------------
-- 3) memories — yeniden yaratma (project_id -> projects.id)
-- ---------------------------------------------------------------------------

ALTER TABLE memories RENAME TO memories_old;

CREATE TABLE memories (
    id                INTEGER PRIMARY KEY,
    kind              TEXT    NOT NULL CHECK (kind IN ('profile', 'preference', 'project', 'decision', 'task', 'working_context', 'relationship', 'idea', 'routine', 'tool_state')),
    title             TEXT    NOT NULL CHECK (length(title) > 0),
    content           TEXT    NOT NULL CHECK (length(content) > 0),
    summary           TEXT,
    project_id        TEXT    REFERENCES projects (id) ON DELETE SET NULL ON UPDATE CASCADE CHECK (project_id IS NULL OR length(project_id) > 0),
    importance        REAL    NOT NULL CHECK (importance >= 0.0 AND importance <= 1.0),
    confidence        REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    source_session_id INTEGER REFERENCES sessions (id) ON DELETE SET NULL,
    created_at        TEXT    NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    updated_at        TEXT    NOT NULL CHECK (updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    last_accessed_at  TEXT    CHECK (last_accessed_at IS NULL OR last_accessed_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    expires_at        TEXT    CHECK (expires_at IS NULL OR expires_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    is_archived       INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    embedding         BLOB,
    metadata_json     TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json))
) STRICT;

INSERT INTO memories (id, kind, title, content, summary, project_id, importance,
                      confidence, source_session_id, created_at, updated_at,
                      last_accessed_at, expires_at, is_archived, embedding, metadata_json)
SELECT id, kind, title, content, summary, project_id, importance,
       confidence, source_session_id, created_at, updated_at,
       last_accessed_at, expires_at, is_archived, embedding, metadata_json
  FROM memories_old;

-- ---------------------------------------------------------------------------
-- 4) Eski kabuklari dusur — once cocuk, sonra ebeveyn
-- ---------------------------------------------------------------------------
--
-- `memories_old`e referans veren yok; dusurmesi hicbir eylem tetiklemez.
-- Ardindan `sessions_old` de oksuz kalir. Ters sirada dusurulseydi,
-- `sessions_old` uzerindeki ortuk DELETE `memories_old.source_session_id`
-- degerlerini NULL'a cekerdi — ve o an veri hala oradan kopyalanmamis olsaydi
-- bag kalici olarak kaybolurdu.

DROP TABLE memories_old;
DROP TABLE sessions_old;

-- ---------------------------------------------------------------------------
-- 5) Index'ler
-- ---------------------------------------------------------------------------
--
-- Eski tablolarin index'leri onlarla birlikte dustu. Adlar ve tanimlar 001 ile
-- **birebir ayni**: sorgu planlari degismemeli, yalnizca FK eklendi.

CREATE INDEX idx_sessions_started_at ON sessions (started_at DESC);
CREATE INDEX idx_sessions_project_id ON sessions (project_id);
CREATE INDEX idx_sessions_open ON sessions (started_at DESC) WHERE ended_at IS NULL;

CREATE INDEX idx_memories_kind ON memories (kind);
CREATE INDEX idx_memories_project_id ON memories (project_id);
CREATE INDEX idx_memories_importance ON memories (importance DESC);
CREATE INDEX idx_memories_is_archived ON memories (is_archived);
CREATE INDEX idx_memories_created_at ON memories (created_at DESC);
CREATE INDEX idx_memories_source_session_id ON memories (source_session_id);
CREATE INDEX idx_memories_stage_a ON memories (is_archived, project_id, importance DESC, created_at DESC);
CREATE INDEX idx_memories_expires_at ON memories (expires_at) WHERE expires_at IS NOT NULL;

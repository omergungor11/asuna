-- ASU-030 — `sessions` + `memories` (PROJECT.md Bolum 12.2)
--
-- BU DOSYA YAYINLANMISTIR VE BIR DAHA DEGISTIRILMEZ. Duzeltme yeni bir
-- migration ekler (ADR-005 "Migration Karari"). Bir kullanicinin DB'sinde bu
-- migration zaten uygulandiysa buradaki degisiklik ona hic ulasmaz — iki
-- makine sessizce farkli semaya duser.
--
-- Tasarim notlari:
--   * `STRICT`: SQLite'in tip zorlamasi acik. `importance = "cok"` gibi bir
--     yazim INSERT aninda reddedilir, yillar sonra bir okuma hatasi olarak
--     degil.
--   * Zaman alanlari UTC ISO-8601 metin (conventions.md "Database"). Bicim
--     `GLOB` ile zorlanir: epoch saniyesi ya da yerel saat yazilirsa Stage A
--     siralamasi (metin siralamasi) sessizce bozulurdu.
--   * `sessions` once yaratilir: `memories.source_session_id` ona referans verir.

CREATE TABLE sessions (
    id                 INTEGER PRIMARY KEY,
    started_at         TEXT    NOT NULL CHECK (started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    ended_at           TEXT    CHECK (ended_at IS NULL OR (ended_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z' AND ended_at >= started_at)),
    project_id         TEXT    CHECK (project_id IS NULL OR length(project_id) > 0),
    summary            TEXT,
    transcript_path    TEXT    CHECK (transcript_path IS NULL OR length(transcript_path) > 0),
    model              TEXT    NOT NULL CHECK (length(model) > 0),
    input_tokens       INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0),
    output_tokens      INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
    total_tokens       INTEGER CHECK (total_tokens IS NULL OR total_tokens >= 0),
    estimated_cost_usd REAL    CHECK (estimated_cost_usd IS NULL OR estimated_cost_usd >= 0.0),
    usage_json         TEXT    CHECK (usage_json IS NULL OR json_valid(usage_json)),
    created_at         TEXT    NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- `summary` NULL kalabilir: ozet uretimi basarisiz olsa da oturum kapanmali
-- (Phase 3 plani Adim 4). `transcript_path` NULL = transcript diske
-- yazilmadi (`ASUNA_TRANSCRIPT_STORAGE=false`).
--
-- Token/maliyet: `input_tokens` / `output_tokens` / `total_tokens` /
-- `estimated_cost_usd` UI'in gosterdigi skalerlerdir. Ayrintili kirilim
-- (`Usage.inputTokensDetails` anahtarlari) runtime'da dogrulanmadigi icin
-- (memory.md T5) simdilik `usage_json` icinde ham JSON olarak durur;
-- anahtarlar netlestiginde ASU-032 yeni bir migration ile kolon acabilir.

CREATE INDEX idx_sessions_started_at ON sessions (started_at DESC);
CREATE INDEX idx_sessions_project_id ON sessions (project_id);
-- Acilista "yarim kalmis oturum var mi?" sorgusu (Phase 3 plani Adim 4).
CREATE INDEX idx_sessions_open ON sessions (started_at DESC) WHERE ended_at IS NULL;

CREATE TABLE memories (
    id                INTEGER PRIMARY KEY,
    kind              TEXT    NOT NULL CHECK (kind IN ('profile', 'preference', 'project', 'decision', 'task', 'working_context', 'relationship', 'idea', 'routine', 'tool_state')),
    title             TEXT    NOT NULL CHECK (length(title) > 0),
    content           TEXT    NOT NULL CHECK (length(content) > 0),
    summary           TEXT,
    project_id        TEXT    CHECK (project_id IS NULL OR length(project_id) > 0),
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

-- `kind` CHECK'i **tek kaynaktir**: Rust `MemoryKind` ve TypeScript
-- `MEMORY_KINDS` bu satira testlerle baglidir (memory.md T2 burada kapandi).
-- Deger listesi PROJECT.md Bolum 5.3.
--
-- `source_session_id`: "bu neden hatirlaniyor?" sorusunun cevabi (memory.md
-- Bolum 2). Oturum silinirse hafiza silinmez, yalnizca izi kopar
-- (`ON DELETE SET NULL`) — hafizayi silme yetkisi kullanicinindir.
--
-- `project_id` simdilik **serbest metin ve FK'siz**: `projects` tablosu Phase 4
-- (ASU-039). O migration once `projects`i yaratir, sonra mevcut `project_id`
-- degerlerini eslestirir ve tabloyu yeniden yaratarak FK ekler (SQLite'ta
-- ALTER TABLE ile FK eklenemez). Eslesmeyen degerler NULL'a cekilmez, olduklari
-- gibi birakilir — kullanici verisi sessizce silinmez.
--
-- `embedding` MVP'de **yazilmaz**. Stage B'ye kadar NULL kalir; semada durmasi
-- sonradan migration acmamak icin (memory.md Bolum 2). Renderer'a da gitmez.
--
-- `is_archived`: silme yerine arsivleme varsayilan yol, ama gercek DELETE de
-- desteklenir — memory "kullanici tarafindan gercekten silinebilir" olmali
-- (PROJECT.md Bolum 20).

CREATE INDEX idx_memories_kind ON memories (kind);
CREATE INDEX idx_memories_project_id ON memories (project_id);
CREATE INDEX idx_memories_importance ON memories (importance DESC);
CREATE INDEX idx_memories_is_archived ON memories (is_archived);
CREATE INDEX idx_memories_created_at ON memories (created_at DESC);
CREATE INDEX idx_memories_source_session_id ON memories (source_session_id);
-- Stage A'nin ana sorgusu: arsivlenmemis + projeye ait, onem ve tazelik sirali
-- (memory.md Bolum 4). Tek tek kolon index'leri bu bilesik sorgu icin yetmez.
CREATE INDEX idx_memories_stage_a ON memories (is_archived, project_id, importance DESC, created_at DESC);
-- Suresi dolmus kayitlarin taranmasi (memory.md T7). Kismi index: kayitlarin
-- cogunda `expires_at` NULL olacak.
CREATE INDEX idx_memories_expires_at ON memories (expires_at) WHERE expires_at IS NOT NULL;

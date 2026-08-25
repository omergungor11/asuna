-- ASU-039 geri alma.
--
-- KAPSAM: bu dosya **gelistirme araci**dir. Uygulama acilisinda yalnizca
-- `to_latest()` cagrilir; `down` otomatik olarak hicbir zaman kosmaz.
--
-- Ne kaybolur: `projects` tablosunun kendisi (ad, yol, dil, framework...).
-- Ne KAYBOLMAZ: `memories.project_id` / `sessions.project_id` etiketleri —
-- FK dusuruluyor ama metin degerler oldugu gibi kaliyor. Yani 003'un
-- devraldigi `unlinked` satirlar geri alinsa bile hafizanin proje etiketi
-- yerinde durur ve migration yeniden ileri sarildiginda ayni satirlar yeniden
-- uretilir. Geri alma **kullanici verisi silmez**.
--
-- Sira 003'un tersi degil, ayni ilkenin tekrari: hicbir ebeveyn tablo cocugu
-- hala ona bakarken dusurulmez (bkz. 003_projects.up.sql bas yorumu). Ortuk
-- DELETE'in `ON DELETE SET NULL` eylemini tetiklemesi bu sirada mumkun degil.

-- ---------------------------------------------------------------------------
-- 1) sessions — FK'siz tanima geri don (002 sonrasi hali: end_reason kolonu var)
-- ---------------------------------------------------------------------------

ALTER TABLE sessions RENAME TO sessions_old;

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
    created_at         TEXT    NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    -- Sonda kalmali: 002'nin `down`'i (`DROP COLUMN end_reason`) ve
    -- `SESSION_COLUMNS` sirasi buna dayaniyor.
    end_reason         TEXT    CHECK (end_reason IS NULL OR end_reason IN ('completed', 'abandoned', 'error'))
) STRICT;

INSERT INTO sessions (id, started_at, ended_at, project_id, summary, transcript_path,
                      model, input_tokens, output_tokens, total_tokens,
                      estimated_cost_usd, usage_json, created_at, end_reason)
SELECT id, started_at, ended_at, project_id, summary, transcript_path,
       model, input_tokens, output_tokens, total_tokens,
       estimated_cost_usd, usage_json, created_at, end_reason
  FROM sessions_old;

-- ---------------------------------------------------------------------------
-- 2) memories — 001 tanimina geri don
-- ---------------------------------------------------------------------------

ALTER TABLE memories RENAME TO memories_old;

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

INSERT INTO memories (id, kind, title, content, summary, project_id, importance,
                      confidence, source_session_id, created_at, updated_at,
                      last_accessed_at, expires_at, is_archived, embedding, metadata_json)
SELECT id, kind, title, content, summary, project_id, importance,
       confidence, source_session_id, created_at, updated_at,
       last_accessed_at, expires_at, is_archived, embedding, metadata_json
  FROM memories_old;

-- ---------------------------------------------------------------------------
-- 3) Eski kabuklar + projects
-- ---------------------------------------------------------------------------

DROP TABLE memories_old;
DROP TABLE sessions_old;

-- Artik hicbir tablo `projects`e referans vermiyor; dusurmek guvenli.
DROP INDEX IF EXISTS idx_projects_status;
DROP INDEX IF EXISTS idx_projects_last_opened_at;
DROP INDEX IF EXISTS idx_projects_path;
DROP TABLE projects;

-- ---------------------------------------------------------------------------
-- 4) 001'in index'leri
-- ---------------------------------------------------------------------------

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

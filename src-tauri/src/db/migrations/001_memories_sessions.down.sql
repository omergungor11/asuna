-- ASU-030 migration 1'in geri alinmasi.
--
-- KAPSAM: bu dosya **gelistirme araci**dir. Uygulama acilisinda yalnizca
-- `to_latest()` cagrilir; `down` otomatik olarak hicbir zaman kosmaz.
-- Kullanicinin DB'sinde calistirilmasi veri kaybi demektir ve orchestrator
-- onayi olmadan yapilmaz.
--
-- Sira FK yonunun tersi: once referans veren (`memories`), sonra referans
-- verilen (`sessions`). Index'ler tablo ile birlikte duser, yine de acikca
-- yazildi — okuyan kisi neyin geri alindigini tam gorsun.

DROP INDEX IF EXISTS idx_memories_expires_at;
DROP INDEX IF EXISTS idx_memories_stage_a;
DROP INDEX IF EXISTS idx_memories_source_session_id;
DROP INDEX IF EXISTS idx_memories_created_at;
DROP INDEX IF EXISTS idx_memories_is_archived;
DROP INDEX IF EXISTS idx_memories_importance;
DROP INDEX IF EXISTS idx_memories_project_id;
DROP INDEX IF EXISTS idx_memories_kind;
DROP TABLE IF EXISTS memories;

DROP INDEX IF EXISTS idx_sessions_open;
DROP INDEX IF EXISTS idx_sessions_project_id;
DROP INDEX IF EXISTS idx_sessions_started_at;
DROP TABLE IF EXISTS sessions;

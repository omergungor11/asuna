//! Sema migration'lari (ASU-029, ASU-030 / ADR-005 OQ-2).
//!
//! # Kurallar (ADR-005 "Migration Karari")
//!
//! 1. Migration'lar **sirali** ve **degismez**. Yayinlanmis bir `M` bir daha
//!    duzenlenmez; duzeltme yeni bir `M` ekler. Sebep: `PRAGMA user_version`
//!    zaten uygulanmis migration'lari tekrar calistirmaz — eski dosyayi
//!    degistirmek yalnizca yeni kurulumlari etkiler ve iki makineyi sessizce
//!    farkli semaya dusurur.
//! 2. Her `M::up` icin `M::down` yazilir. `down` gelistirme sirasinda geri
//!    alabilmek icindir; kullanicinin DB'sinde otomatik olarak **kosmaz**.
//! 3. Sema degisikligi `src/shared/*.ts` tip aynasiyla **ayni commit'te** gider.
//! 4. [`migrations`]`().validate()` bir birim testte kosar — bozuk SQL CI'da
//!    yakalanir, kullanicinin acilisinda degil.
//!
//! # Neden `.sql` dosyalari
//!
//! DDL'ler `include_str!` ile gomulur. Boylece ayni metin hem Rust hem
//! TypeScript testleri tarafindan okunabilir: `memories.kind` CHECK'i sema ile
//! `MemoryKind` enum'unu ve `src/shared/memory.ts` union'ini birbirine baglayan
//! **tek kaynak** haline gelir (ASU-030 kabul kriteri: "elle senkronize edilen
//! ikinci tanim yok").

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

use super::DbError;

/// Migration 1 — `sessions` + `memories` (ASU-030).
pub const V1_UP: &str = include_str!("001_memories_sessions.up.sql");
pub const V1_DOWN: &str = include_str!("001_memories_sessions.down.sql");

/// Migration 2 — `sessions.end_reason` (ASU-033).
pub const V2_UP: &str = include_str!("002_session_end_reason.up.sql");
pub const V2_DOWN: &str = include_str!("002_session_end_reason.down.sql");

/// Migration 3 — `projects` + `project_id` yabanci anahtarlari (ASU-039).
pub const V3_UP: &str = include_str!("003_projects.up.sql");
pub const V3_DOWN: &str = include_str!("003_projects.down.sql");

/// Migration 4 — `tool_events` audit tablosu (ASU-050).
pub const V4_UP: &str = include_str!("004_tool_events.up.sql");
pub const V4_DOWN: &str = include_str!("004_tool_events.down.sql");

/// Migration 5 — `tool_events.outcome` (ASU-051).
pub const V5_UP: &str = include_str!("005_tool_event_outcome.up.sql");
pub const V5_DOWN: &str = include_str!("005_tool_event_outcome.down.sql");

/// Migration 6 — `sessions.title` / `sessions.modality` + `messages` +
/// `attachments` (Chat Shell pivotu, plan-chat-shell.md WP1).
pub const V6_UP: &str = include_str!("006_conversations.up.sql");
pub const V6_DOWN: &str = include_str!("006_conversations.down.sql");

/// Sirali migration tanimlari.
///
/// **Bu vektore yalnizca sona ekleme yapilir.** Araya ekleme ya da silme, daha
/// once uygulanmis surumlerin anlamini degistirir.
fn definitions() -> Vec<M<'static>> {
    vec![
        M::up(V1_UP).down(V1_DOWN),
        M::up(V2_UP).down(V2_DOWN),
        M::up(V3_UP).down(V3_DOWN),
        M::up(V4_UP).down(V4_DOWN),
        M::up(V5_UP).down(V5_DOWN),
        M::up(V6_UP).down(V6_DOWN),
    ]
}

/// Bu kod surumunun bekledigi sema surumu (`PRAGMA user_version`).
pub const EXPECTED_SCHEMA_VERSION: u32 = 6;

/// Migration kumesi. Testler `validate()` icin bunu kullanir.
pub fn migrations() -> Migrations<'static> {
    Migrations::new(definitions())
}

/// Migration'lari ileri yonde, idempotent uygular.
pub(super) fn apply(connection: &mut Connection) -> Result<(), DbError> {
    migrations()
        .to_latest(connection)
        .map_err(DbError::Migration)
}

/// `memories.kind` CHECK kisitindaki degerleri **sema metninden** okur.
///
/// Rust enum'u ve TypeScript union'i bu listeye testlerle baglanir; kimse
/// listeyi tek tarafli genisletemez.
///
/// Kaynak **en son** tanim (`V3_UP`): 003 tabloyu FK eklemek icin yeniden
/// yaratti, yani gecerli CHECK kisiti oradadir. 001'deki liste ile ayni oldugu
/// ayrica test ediliyor — yeniden yaratma sirasinda bir deger dusmus olamaz.
pub fn kinds_declared_in_schema() -> Vec<String> {
    values_in_check(V3_UP, "CHECK (kind IN (")
}

/// `sessions.end_reason` CHECK kisitindaki degerleri **sema metninden** okur
/// (ASU-033). Rust `SessionEndReason` ve TypeScript `SESSION_END_REASONS` bu
/// listeye testlerle baglidir.
pub fn end_reasons_declared_in_schema() -> Vec<String> {
    values_in_check(V3_UP, "end_reason IN (")
}

/// `projects.status` CHECK kisitindaki degerleri **sema metninden** okur
/// (ASU-039). Rust `ProjectStatus` ve TypeScript `PROJECT_STATUSES` bu listeye
/// testlerle baglidir.
pub fn project_statuses_declared_in_schema() -> Vec<String> {
    values_in_check(V3_UP, "status IN (")
}

/// `tool_events.approval_state` CHECK kisitindaki degerleri **sema metninden**
/// okur (ASU-050). Rust `ToolApprovalState` ve TypeScript
/// `TOOL_APPROVAL_STATES` bu listeye testlerle baglidir.
pub fn approval_states_declared_in_schema() -> Vec<String> {
    values_in_check(V4_UP, "approval_state IN (")
}

/// `tool_events.risk_level` CHECK kisitindaki degerleri **sema metninden**
/// okur (ASU-050).
///
/// Degerler tirnaksiz sayilardir (`0, 1, 2, 3`); [`values_in_check`] tirnak
/// kirpmayi zaten kosulsuz yapar, dolayisiyla ayni ayristirici calisir.
/// `BETWEEN 0 AND 3` yazilsaydi kume sema metninden okunamaz, Rust enum'u ile
/// sema arasindaki bag yalnizca yoruma dayanirdi.
pub fn risk_levels_declared_in_schema() -> Vec<String> {
    values_in_check(V4_UP, "risk_level IN (")
}

/// `tool_events.outcome` CHECK kisitindaki degerleri **sema metninden** okur
/// (ASU-051). Rust `ToolOutcome` ve TypeScript `TOOL_OUTCOMES` bu listeye
/// testlerle baglidir.
///
/// Kaynak `V5_UP`: kolon `ALTER TABLE ... ADD COLUMN` ile geldi, yani gecerli
/// CHECK kisiti 004'te degil 005'te.
pub fn outcomes_declared_in_schema() -> Vec<String> {
    values_in_check(V5_UP, "outcome IN (")
}

/// `sessions.modality` CHECK kisitindaki degerleri **sema metninden** okur
/// (Chat Shell / migration 006). Rust `SessionModality` ve TypeScript
/// `ConversationSummary['modality']` bu listeye testlerle baglidir.
pub fn modalities_declared_in_schema() -> Vec<String> {
    values_in_check(V6_UP, "modality IN (")
}

/// `messages.role` CHECK kisitindaki degerleri **sema metninden** okur
/// (migration 006). Rust `MessageRole` ve TypeScript `ChatMessage['role']`
/// bu listeye testlerle baglidir.
pub fn message_roles_declared_in_schema() -> Vec<String> {
    values_in_check(V6_UP, "role IN (")
}

/// `attachments.origin` CHECK kisitindaki degerleri **sema metninden** okur
/// (migration 006). Rust `AttachmentOrigin` ve TypeScript
/// `ChatAttachment['origin']` bu listeye testlerle baglidir.
pub fn attachment_origins_declared_in_schema() -> Vec<String> {
    values_in_check(V6_UP, "origin IN (")
}

/// `... IN ('a', 'b')` listesini ayristirir.
fn values_in_check(schema: &str, marker: &str) -> Vec<String> {
    let start = schema
        .find(marker)
        .unwrap_or_else(|| panic!("`{marker}` kisiti semada bulunmali"))
        + marker.len();
    let rest = &schema[start..];
    let end = rest.find(')').expect("CHECK kisiti kapanmali");

    rest[..end]
        .split(',')
        .map(|item| item.trim().trim_matches('\'').to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;

    /// ADR-005 gate: bozuk SQL, eksik virgul, gecersiz CHECK — hepsi burada
    /// yakalanir. `validate()` migration'lari temiz bir bellek ici DB'ye
    /// uygular, yani gercek bir kuru kosum.
    #[test]
    fn migration_set_is_valid() {
        migrations()
            .validate()
            .expect("migration'lar temiz bir DB'ye uygulanabilmeli");
    }

    /// `to_latest` sema surumunu bekledigimiz yere getirir ve tekrar
    /// calistirildiginda hicbir sey yapmaz.
    #[test]
    fn applying_migrations_is_idempotent() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");

        for _ in 0..3 {
            apply(&mut connection).expect("migration uygulanmali");
            let version: u32 = connection
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .expect("user_version okunmali");
            assert_eq!(version, EXPECTED_SCHEMA_VERSION);
        }
    }

    /// `down` gercekten calisiyor mu? (ADR-005'te `tauri-plugin-sql`'in
    /// down'lari sessizce attigi olculmustu — burada tersini kanitliyoruz.)
    #[test]
    fn migrations_can_be_rolled_back_and_reapplied() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        apply(&mut connection).expect("migration uygulanmali");

        migrations()
            .to_version(&mut connection, 0)
            .expect("geri alinabilmeli");

        let remaining: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('memories', 'sessions')",
                [],
                |row| row.get(0),
            )
            .expect("sqlite_master okunmali");
        assert_eq!(remaining, 0, "down migration tablolari birakmis");

        // Ve yeniden ileri sarilabilmeli.
        apply(&mut connection).expect("yeniden uygulanmali");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version okunmali");
        assert_eq!(version, EXPECTED_SCHEMA_VERSION);
    }

    // --- Sema butunlugu ----------------------------------------------------
    //
    // CHECK kisitlari yorum degil, calisan kod. Bir sonraki task bunlara
    // guvenerek yazacak; burada gercekten zorlandiklari dogrulaniyor.

    fn fresh_db() -> crate::db::AsunaDb {
        crate::db::AsunaDb::open_in_memory().expect("DB acilmali")
    }

    fn insert_memory(db: &crate::db::AsunaDb, sql: &str) -> Result<usize, crate::db::DbError> {
        db.with_connection(|conn| conn.execute(sql, []))
    }

    #[test]
    fn memory_kind_check_rejects_values_outside_the_spec() {
        let db = fresh_db();
        let error = insert_memory(
            &db,
            "INSERT INTO memories (kind, title, content, importance, confidence, created_at, updated_at)
             VALUES ('project_decision', 't', 'c', 0.5, 0.5, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z')",
        )
        .expect_err("bilinmeyen kind reddedilmeli");
        assert!(matches!(error, DbError::Query(_)));
    }

    #[test]
    fn importance_and_confidence_are_bounded_to_zero_one() {
        let db = fresh_db();
        for (importance, confidence) in [(9.0, 1.0), (-0.1, 1.0), (0.5, 1.5)] {
            insert_memory(
                &db,
                &format!(
                    "INSERT INTO memories (kind, title, content, importance, confidence, created_at, updated_at)
                     VALUES ('decision', 't', 'c', {importance}, {confidence}, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z')"
                ),
            )
            .expect_err("aralik disi deger reddedilmeli");
        }
    }

    /// Epoch saniyesi ya da yerel saat yazilirsa Stage A siralamasi (metin
    /// siralamasi) sessizce bozulurdu — DB bunu kabul etmiyor.
    #[test]
    fn timestamps_must_be_utc_iso_8601() {
        let db = fresh_db();
        for stamp in [
            "1756108800",
            "2026-08-25 10:00:00",
            "25/08/2026",
            "2026-08-25T10:00:00+03:00",
        ] {
            insert_memory(
                &db,
                &format!(
                    "INSERT INTO memories (kind, title, content, importance, confidence, created_at, updated_at)
                     VALUES ('decision', 't', 'c', 0.5, 0.5, '{stamp}', '{stamp}')"
                ),
            )
            .expect_err("UTC ISO-8601 disi zaman damgasi reddedilmeli");
        }

        // Salise'li bicim kabul edilmeli.
        insert_memory(
            &db,
            "INSERT INTO memories (kind, title, content, importance, confidence, created_at, updated_at)
             VALUES ('decision', 't', 'c', 0.5, 0.5, '2026-08-25T10:00:00.123Z', '2026-08-25T10:00:00.123Z')",
        )
        .expect("salise'li ISO-8601 kabul edilmeli");
    }

    #[test]
    fn metadata_json_must_be_valid_json() {
        let db = fresh_db();
        insert_memory(
            &db,
            "INSERT INTO memories (kind, title, content, importance, confidence, created_at, updated_at, metadata_json)
             VALUES ('idea', 't', 'c', 0.5, 0.5, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z', '{ bozuk')",
        )
        .expect_err("gecersiz JSON reddedilmeli");
    }

    /// `STRICT` tablo: tip zorlamasi acik. `importance = 'cok'` INSERT aninda
    /// duser, aylar sonra bir okuma hatasi olarak degil.
    #[test]
    fn strict_tables_reject_wrong_column_types() {
        let db = fresh_db();
        insert_memory(
            &db,
            "INSERT INTO memories (kind, title, content, importance, confidence, created_at, updated_at)
             VALUES ('idea', 't', 'c', 'cok', 0.5, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z')",
        )
        .expect_err("STRICT tablo yanlis tipi reddetmeli");
    }

    /// Var olmayan bir oturuma referans veren hafiza yazilamaz
    /// (`PRAGMA foreign_keys = ON` acilista uygulaniyor).
    #[test]
    fn source_session_id_is_a_real_foreign_key() {
        let db = fresh_db();
        insert_memory(
            &db,
            "INSERT INTO memories (kind, title, content, importance, confidence, created_at, updated_at, source_session_id)
             VALUES ('decision', 't', 'c', 0.5, 0.5, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z', 4242)",
        )
        .expect_err("var olmayan oturum referansi reddedilmeli");
    }

    /// Oturum silinince hafiza **silinmez**, yalnizca izi kopar. Kullanicinin
    /// hafizasi transcript temizligine kurban gitmemeli.
    #[test]
    fn deleting_a_session_keeps_the_memory_but_clears_the_link() {
        let db = fresh_db();
        let (remaining, link): (i64, Option<i64>) = db
            .with_connection(|conn| {
                conn.execute(
                    "INSERT INTO sessions (started_at, model, created_at)
                     VALUES ('2026-08-25T10:00:00Z', 'gpt-realtime-2.1', '2026-08-25T10:00:00Z')",
                    [],
                )?;
                let session_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO memories (kind, title, content, importance, confidence, created_at, updated_at, source_session_id)
                     VALUES ('decision', 't', 'c', 0.9, 1.0, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z', ?1)",
                    [session_id],
                )?;
                conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;
                conn.query_row(
                    "SELECT count(*), max(source_session_id) FROM memories",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .expect("sorgu calismali");

        assert_eq!(remaining, 1, "hafiza oturumla birlikte silinmemeli");
        assert_eq!(link, None, "kopan referans NULL'a cekilmeli");
    }

    #[test]
    fn a_session_cannot_end_before_it_started() {
        let db = fresh_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO sessions (started_at, ended_at, model, created_at)
                 VALUES ('2026-08-25T10:00:00Z', '2026-08-25T09:00:00Z', 'm', '2026-08-25T10:00:00Z')",
                [],
            )
        })
        .expect_err("bitis baslangictan once olamaz");
    }

    /// ASU-030 kabul kriteri: sorgu icin gerekli index'ler.
    #[test]
    fn the_required_indexes_exist() {
        let db = fresh_db();
        let names: Vec<String> = db
            .with_connection(|conn| {
                let mut statement = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type = 'index' ORDER BY name")?;
                let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<String>>>()
            })
            .expect("index listesi okunmali");

        for expected in [
            "idx_projects_path",
            "idx_projects_last_opened_at",
            "idx_projects_status",
            "idx_memories_kind",
            "idx_memories_project_id",
            "idx_memories_importance",
            "idx_memories_is_archived",
            "idx_memories_created_at",
            "idx_memories_source_session_id",
            "idx_memories_stage_a",
            "idx_memories_expires_at",
            "idx_sessions_started_at",
            "idx_sessions_project_id",
            "idx_sessions_open",
            // ASU-050 — audit defterinin sorgulanabilir eksenleri.
            "idx_tool_events_session_id",
            "idx_tool_events_created_at",
            "idx_tool_events_tool_name",
            // Chat Shell (006) — konusma uzerinden erisim ekseni.
            "idx_messages_session_id",
            "idx_attachments_session_id",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "`{expected}` index'i yok. Mevcut: {names:?}"
            );
        }
    }

    // --- Migration 2: `end_reason` (ASU-033) --------------------------------

    /// Eski bir DB (sema 1) uzerinde: yarim kalan oturumun bayrak cumlesi
    /// `end_reason`'a tasinir ve `summary` **temizlenir** — ozet alani bundan
    /// sonra yalnizca gercek ozeti tasir (ASU-034'un girdisi kirlenmez).
    #[test]
    fn migration_two_moves_the_abandoned_flag_out_of_the_summary_column() {
        const FLAG: &str = crate::db::session_repository::ABANDONED_SESSION_SUMMARY;

        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        migrations()
            .to_version(&mut connection, 1)
            .expect("sema 1 uygulanmali");

        connection
            .execute(
                "INSERT INTO sessions (id, started_at, ended_at, summary, model, created_at)
                 VALUES (1, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z', ?1, 'm', '2026-08-25T10:00:00Z'),
                        (2, '2026-08-25T11:00:00Z', '2026-08-25T11:30:00Z', 'Gercek ozet.', 'm', '2026-08-25T11:00:00Z'),
                        (3, '2026-08-25T12:00:00Z', NULL, NULL, 'm', '2026-08-25T12:00:00Z')",
                [FLAG],
            )
            .expect("eski kayitlar yazilmali");

        apply(&mut connection).expect("sema 2'ye yukseltilmeli");

        let rows: Vec<(i64, Option<String>, Option<String>)> = connection
            .prepare("SELECT id, end_reason, summary FROM sessions ORDER BY id")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .collect()
            })
            .expect("kayitlar okunmali");

        assert_eq!(
            rows,
            vec![
                (1, Some("abandoned".to_owned()), None),
                (
                    2,
                    Some("completed".to_owned()),
                    Some("Gercek ozet.".to_owned())
                ),
                // Hala acik oturum: durum bilinmiyor, uydurulmuyor.
                (3, None, None),
            ]
        );
    }

    /// Geri alma calisiyor ve kaybolan bilgi insan diliyle geri yaziliyor.
    #[test]
    fn migration_two_can_be_rolled_back_without_losing_the_reason() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        apply(&mut connection).expect("migration uygulanmali");
        connection
            .execute(
                "INSERT INTO sessions (started_at, ended_at, model, created_at, end_reason)
                 VALUES ('2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z', 'm', '2026-08-25T10:00:00Z', 'abandoned')",
                [],
            )
            .expect("kayit yazilmali");

        migrations()
            .to_version(&mut connection, 1)
            .expect("sema 1'e donulebilmeli");

        let summary: Option<String> = connection
            .query_row("SELECT summary FROM sessions", [], |row| row.get(0))
            .expect("okunmali");
        assert_eq!(
            summary.as_deref(),
            Some(crate::db::session_repository::ABANDONED_SESSION_SUMMARY)
        );

        apply(&mut connection).expect("yeniden ileri sarilmali");
    }

    #[test]
    fn end_reason_check_rejects_values_outside_the_spec() {
        let db = fresh_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO sessions (started_at, model, created_at, end_reason)
                 VALUES ('2026-08-25T10:00:00Z', 'm', '2026-08-25T10:00:00Z', 'kapandi')",
                [],
            )
        })
        .expect_err("bilinmeyen end_reason reddedilmeli");
    }

    /// Sema metnindeki bayrak cumlesi ile Rust sabiti ayni olmali; aksi halde
    /// geriye donuk doldurma hicbir satiri eslestirmez ve **sessizce** hicbir
    /// sey yapmaz.
    #[test]
    fn the_backfill_matches_the_flag_sentence_used_by_the_recovery_path() {
        assert!(V2_UP.contains(crate::db::session_repository::ABANDONED_SESSION_SUMMARY));
        assert!(V2_DOWN.contains(crate::db::session_repository::ABANDONED_SESSION_SUMMARY));
    }

    #[test]
    fn schema_declares_the_three_end_reasons() {
        assert_eq!(
            end_reasons_declared_in_schema(),
            ["completed", "abandoned", "error"]
        );
    }

    // --- Migration 3: `projects` (ASU-039) ----------------------------------

    /// Sema 2'de veri olan bir DB acar; FK zorlamasi **acik** (uretimdeki gibi).
    fn database_at_version_two_with_foreign_keys_on() -> Connection {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("FK zorlamasi acilmali");
        migrations()
            .to_version(&mut connection, 2)
            .expect("sema 2 uygulanmali");

        connection
            .execute_batch(
                "INSERT INTO sessions (id, started_at, ended_at, project_id, summary, model, created_at, end_reason)
                 VALUES (1, '2026-08-25T10:00:00Z', '2026-08-25T10:30:00Z', 'asuna', 'Ozet.', 'gpt-realtime-2.1', '2026-08-25T10:00:00Z', 'completed'),
                        (2, '2026-08-25T11:00:00Z', NULL, NULL, NULL, 'gpt-realtime-2.1', '2026-08-25T11:00:00Z', NULL);

                 INSERT INTO memories (id, kind, title, content, project_id, importance, confidence, source_session_id, created_at, updated_at)
                 VALUES (1, 'decision', 'Wake word yerel', 'Cihazda kalir.', 'asuna', 0.9, 1.0, 1, '2026-08-25T10:05:00Z', '2026-08-25T10:05:00Z'),
                        (2, 'preference', 'Kisa cevap', 'Kod yazarken kisa.', NULL, 0.5, 0.8, NULL, '2026-08-25T10:06:00Z', '2026-08-25T10:06:00Z'),
                        (3, 'task', 'Eski proje', 'Baska bir etiket.', 'gel-gez-gor', 0.4, 0.7, 1, '2026-08-25T10:07:00Z', '2026-08-25T10:07:00Z');",
            )
            .expect("sema 2 verisi yazilmali");

        connection
    }

    /// **ASU-039 kabul kaniti.** FK eklemek icin tablolar yeniden yaratiliyor;
    /// bu islemin hicbir satiri, hicbir etiketi ve hicbir hafiza→oturum bagini
    /// kaybetmemesi gerekiyor.
    ///
    /// Ozellikle `source_session_id`: naif bir siralama (`DROP TABLE sessions`
    /// once) FK acikken ortuk bir DELETE calistirip `ON DELETE SET NULL`
    /// eylemini tetikler ve "bu hafiza neden hatirlaniyor?" baglarinin
    /// **tamamini** sessizce silerdi.
    #[test]
    fn migration_three_preserves_every_row_label_and_link() {
        let mut connection = database_at_version_two_with_foreign_keys_on();

        apply(&mut connection).expect("sema 3'e yukseltilmeli");

        let memories: Vec<(i64, Option<String>, Option<i64>)> = connection
            .prepare("SELECT id, project_id, source_session_id FROM memories ORDER BY id")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .collect()
            })
            .expect("hafizalar okunmali");
        assert_eq!(
            memories,
            vec![
                (1, Some("asuna".to_owned()), Some(1)),
                (2, None, None),
                (3, Some("gel-gez-gor".to_owned()), Some(1)),
            ],
            "yeniden yaratma bir etiketi ya da oturum bagini dusurmus"
        );

        let sessions: Vec<(i64, Option<String>, Option<String>)> = connection
            .prepare("SELECT id, project_id, end_reason FROM sessions ORDER BY id")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .collect()
            })
            .expect("oturumlar okunmali");
        assert_eq!(
            sessions,
            vec![
                (1, Some("asuna".to_owned()), Some("completed".to_owned())),
                (2, None, None),
            ]
        );
    }

    /// Devralinan her etiket icin `unlinked` bir satir acilir — ne fazlasi ne
    /// eksigi. Yol **uydurulmaz**.
    #[test]
    fn migration_three_backfills_one_unlinked_project_per_carried_over_label() {
        let mut connection = database_at_version_two_with_foreign_keys_on();
        apply(&mut connection).expect("sema 3'e yukseltilmeli");

        let projects: Vec<(String, String, Option<String>, String)> = connection
            .prepare("SELECT id, name, path, status FROM projects ORDER BY id")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })?
                    .collect()
            })
            .expect("projeler okunmali");

        assert_eq!(
            projects,
            vec![
                (
                    "asuna".to_owned(),
                    "asuna".to_owned(),
                    None,
                    "unlinked".to_owned()
                ),
                (
                    "gel-gez-gor".to_owned(),
                    "gel-gez-gor".to_owned(),
                    None,
                    "unlinked".to_owned()
                ),
            ]
        );
    }

    /// **Kabul kriteri**: proje silinince hafiza silinmez, yalnizca izi kopar.
    #[test]
    fn deleting_a_project_keeps_the_memory_and_the_session() {
        let mut connection = database_at_version_two_with_foreign_keys_on();
        apply(&mut connection).expect("sema 3'e yukseltilmeli");

        connection
            .execute("DELETE FROM projects WHERE id = 'asuna'", [])
            .expect("proje silinebilmeli");

        let (memory_count, linked): (i64, i64) = connection
            .query_row(
                "SELECT count(*), count(project_id) FROM memories",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("hafizalar okunmali");
        assert_eq!(memory_count, 3, "hafiza proje ile birlikte silinmemeli");
        assert_eq!(
            linked, 1,
            "yalnizca silinen projenin etiketi NULL'a dusmeli"
        );

        let session_project: Option<String> = connection
            .query_row("SELECT project_id FROM sessions WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("oturum okunmali");
        assert_eq!(session_project, None);

        // Baglantinin kendisi (hafiza -> oturum) etkilenmemeli.
        let source: Option<i64> = connection
            .query_row(
                "SELECT source_session_id FROM memories WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("okunmali");
        assert_eq!(source, Some(1));
    }

    /// Kayitli olmayan bir projeye hafiza yazilamaz: FK gercekten zorluyor.
    #[test]
    fn an_unknown_project_label_is_rejected_by_the_foreign_key() {
        let db = fresh_db();
        insert_memory(
            &db,
            "INSERT INTO memories (kind, title, content, project_id, importance, confidence, created_at, updated_at)
             VALUES ('decision', 't', 'c', 'hic-kayitli-olmayan', 0.5, 0.5, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z')",
        )
        .expect_err("kayitsiz proje referansi reddedilmeli");
    }

    /// Yol normalize edilmis olmali: mutlak, sondaki egik cizgi olmadan ve
    /// filesystem koku olmadan. Ayni dizin iki kez kaydedilemez.
    #[test]
    fn project_paths_must_be_absolute_normalised_and_unique() {
        let db = fresh_db();

        let insert = |path: &str| {
            db.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO projects (id, name, path, status, created_at, updated_at)
                     VALUES (?1, ?1, ?2, 'active', '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z')",
                    rusqlite::params![format!("p{}", path.len()), path],
                )
            })
        };

        for rejected in ["gorece/yol", "/tmp/asuna/", "/", "~/Work/asuna"] {
            insert(rejected).expect_err("normalize edilmemis yol reddedilmeli");
        }

        insert("/tmp/asuna").expect("normalize edilmis yol kabul edilmeli");
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO projects (id, name, path, status, created_at, updated_at)
                 VALUES ('ikinci', 'Ikinci', '/tmp/asuna', 'active', '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z')",
                [],
            )
        })
        .expect_err("ayni yol iki kez kaydedilemez");
    }

    /// `unlinked` <=> `path IS NULL`. Tek yonlu bir CHECK, "yolu olan unlinked"
    /// ya da "yolsuz active" satirlarini sessizce mumkun kilardi.
    #[test]
    fn only_unlinked_projects_may_have_a_null_path() {
        let db = fresh_db();

        let insert = |id: &str, status: &str, path: Option<&str>| {
            db.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO projects (id, name, path, status, created_at, updated_at)
                     VALUES (?1, ?1, ?2, ?3, '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z')",
                    rusqlite::params![id, path, status],
                )
            })
        };

        insert("a", "active", None).expect_err("yolsuz `active` kabul edilmemeli");
        insert("b", "missing", None).expect_err("yolsuz `missing` kabul edilmemeli");
        insert("c", "unlinked", Some("/tmp/x")).expect_err("yollu `unlinked` kabul edilmemeli");

        insert("d", "unlinked", None).expect("yolsuz `unlinked` gecerli");
        insert("e", "missing", Some("/tmp/kayip")).expect("kayip proje yolunu korur");
        // Birden fazla `unlinked` satir: UNIQUE index NULL'lari farkli sayar.
        insert("f", "unlinked", None).expect("ikinci etiket de yazilabilmeli");
    }

    #[test]
    fn project_status_check_rejects_values_outside_the_spec() {
        let db = fresh_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO projects (id, name, path, status, created_at, updated_at)
                 VALUES ('x', 'X', '/tmp/x', 'silinmis', '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z')",
                [],
            )
        })
        .expect_err("bilinmeyen status reddedilmeli");
    }

    /// Geri alma etiketleri kaybetmez; ileri sarma ayni `unlinked` satirlari
    /// yeniden uretir.
    #[test]
    fn migration_three_can_be_rolled_back_without_losing_project_labels() {
        let mut connection = database_at_version_two_with_foreign_keys_on();
        apply(&mut connection).expect("sema 3'e yukseltilmeli");

        migrations()
            .to_version(&mut connection, 2)
            .expect("sema 2'ye donulebilmeli");

        let tables: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'projects'",
                [],
                |row| row.get(0),
            )
            .expect("sqlite_master okunmali");
        assert_eq!(tables, 0, "`projects` dusurulmeliydi");

        let labels: Vec<Option<String>> = connection
            .prepare("SELECT project_id FROM memories ORDER BY id")
            .and_then(|mut statement| statement.query_map([], |row| row.get(0))?.collect())
            .expect("etiketler okunmali");
        assert_eq!(
            labels,
            vec![
                Some("asuna".to_owned()),
                None,
                Some("gel-gez-gor".to_owned())
            ],
            "geri alma kullanici verisini silmemeli"
        );

        apply(&mut connection).expect("yeniden ileri sarilmali");
        let projects: i64 = connection
            .query_row("SELECT count(*) FROM projects", [], |row| row.get(0))
            .expect("okunmali");
        assert_eq!(projects, 2);
    }

    #[test]
    fn schema_declares_the_four_project_statuses() {
        assert_eq!(
            project_statuses_declared_in_schema(),
            ["active", "missing", "archived", "unlinked"]
        );
    }

    /// 003 `memories` ve `sessions` tablolarini yeniden yaratti. Yeniden
    /// yazilan CHECK kisitlari orijinalleriyle **birebir** ayni deger kumesini
    /// tasimali; aksi halde bir `kind` ya da `end_reason` sessizce dusmus olur.
    #[test]
    fn the_rebuilt_tables_keep_the_original_check_constraint_values() {
        assert_eq!(
            values_in_check(V1_UP, "CHECK (kind IN ("),
            values_in_check(V3_UP, "CHECK (kind IN ("),
        );
        assert_eq!(
            values_in_check(V2_UP, "end_reason IN ("),
            values_in_check(V3_UP, "end_reason IN ("),
        );
        // Geri alma da ayni kumeyi geri yazmali.
        assert_eq!(
            values_in_check(V3_DOWN, "CHECK (kind IN ("),
            values_in_check(V3_UP, "CHECK (kind IN ("),
        );
    }

    #[test]
    fn schema_declares_the_ten_memory_kinds_from_the_spec() {
        // PROJECT.md Bolum 5.3 — sira dahil birebir.
        assert_eq!(
            kinds_declared_in_schema(),
            [
                "profile",
                "preference",
                "project",
                "decision",
                "task",
                "working_context",
                "relationship",
                "idea",
                "routine",
                "tool_state",
            ]
        );
    }

    // --- Migration 4: `tool_events` (ASU-050) -------------------------------

    fn insert_tool_event(db: &crate::db::AsunaDb, sql: &str) -> Result<usize, crate::db::DbError> {
        db.with_connection(|conn| conn.execute(sql, []))
    }

    #[test]
    fn schema_declares_the_six_approval_states() {
        assert_eq!(
            approval_states_declared_in_schema(),
            [
                "not_required",
                "auto_approved",
                "approved",
                "denied",
                "timeout",
                "not_requested",
            ]
        );
    }

    /// Risk kumesi sema metninden okunabilir olmali (`BETWEEN` degil `IN`);
    /// aksi halde Rust enum'u ile sema arasindaki bag yalnizca yoruma dayanirdi.
    #[test]
    fn schema_declares_the_four_risk_levels() {
        assert_eq!(risk_levels_declared_in_schema(), ["0", "1", "2", "3"]);
    }

    #[test]
    fn approval_state_check_rejects_values_outside_the_spec() {
        let db = fresh_db();
        for state in ["", "APPROVED", "onaylandi", "auto", "skipped"] {
            insert_tool_event(
                &db,
                &format!(
                    "INSERT INTO tool_events (tool_name, risk_level, approval_state, created_at)
                     VALUES ('get_current_project', 0, '{state}', '2026-08-25T10:00:00Z')"
                ),
            )
            .expect_err("bilinmeyen approval_state reddedilmeli");
        }
    }

    #[test]
    fn risk_level_check_rejects_values_outside_zero_to_three() {
        let db = fresh_db();
        for risk in ["-1", "4", "9"] {
            insert_tool_event(
                &db,
                &format!(
                    "INSERT INTO tool_events (tool_name, risk_level, approval_state, created_at)
                     VALUES ('open_project', {risk}, 'approved', '2026-08-25T10:00:00Z')"
                ),
            )
            .expect_err("aralik disi risk seviyesi reddedilmeli");
        }
    }

    /// Uzunluk tavanlari yorum degil, calisan kisit: bir dosya icerigi ya da
    /// uzun bir stack trace audit defterine sessizce sizamaz.
    #[test]
    fn oversized_audit_fields_are_rejected_by_the_schema() {
        let db = fresh_db();
        let long = "x".repeat(513);

        insert_tool_event(
            &db,
            &format!(
                "INSERT INTO tool_events (tool_name, risk_level, arguments_redacted, approval_state, created_at)
                 VALUES ('read_project_file', 0, '{long}', 'not_required', '2026-08-25T10:00:00Z')"
            ),
        )
        .expect_err("tavani asan arguman ozeti reddedilmeli");

        insert_tool_event(
            &db,
            &format!(
                "INSERT INTO tool_events (tool_name, risk_level, result_summary, approval_state, created_at)
                 VALUES ('read_project_file', 0, '{long}', 'not_required', '2026-08-25T10:00:00Z')"
            ),
        )
        .expect_err("tavani asan sonuc ozeti reddedilmeli");

        insert_tool_event(
            &db,
            &format!(
                "INSERT INTO tool_events (tool_name, risk_level, approval_state, created_at)
                 VALUES ('{}', 0, 'not_required', '2026-08-25T10:00:00Z')",
                "t".repeat(65)
            ),
        )
        .expect_err("tavani asan tool adi reddedilmeli");

        // Bos metin de gecmez: "yazildi ama bos" ile "yazilmadi" ayni gorunmemeli.
        insert_tool_event(
            &db,
            "INSERT INTO tool_events (tool_name, risk_level, arguments_redacted, approval_state, created_at)
             VALUES ('read_project_file', 0, '', 'not_required', '2026-08-25T10:00:00Z')",
        )
        .expect_err("bos arguman ozeti reddedilmeli (NULL kullanilmali)");
    }

    #[test]
    fn audit_timestamps_must_be_utc_iso_8601() {
        let db = fresh_db();
        for stamp in [
            "1756108800",
            "2026-08-25 10:00:00",
            "2026-08-25T10:00:00+03:00",
        ] {
            insert_tool_event(
                &db,
                &format!(
                    "INSERT INTO tool_events (tool_name, risk_level, approval_state, created_at)
                     VALUES ('open_project', 1, 'approved', '{stamp}')"
                ),
            )
            .expect_err("UTC ISO-8601 disi zaman damgasi reddedilmeli");
        }
    }

    /// Var olmayan bir oturuma referans veren audit satiri yazilamaz.
    #[test]
    fn the_audit_session_link_is_a_real_foreign_key() {
        let db = fresh_db();
        insert_tool_event(
            &db,
            "INSERT INTO tool_events (session_id, tool_name, risk_level, approval_state, created_at)
             VALUES (4242, 'open_project', 1, 'approved', '2026-08-25T10:00:00Z')",
        )
        .expect_err("var olmayan oturum referansi reddedilmeli");
    }

    /// **ASU-050 kabul kaniti.** Oturum silinince audit satiri **kalir**;
    /// yalnizca oturuma olan izi kopar.
    ///
    /// `ON DELETE CASCADE` yazilsaydi "konusma gecmisini sil" dugmesi ayni
    /// zamanda audit defterini silen bir primitif olurdu — yani "audit
    /// kayitlari uygulamadan silinemiyor" kriteri dolayli olarak delinirdi.
    #[test]
    fn deleting_a_session_keeps_the_audit_row_but_clears_the_link() {
        let db = fresh_db();
        let (remaining, link, tool): (i64, Option<i64>, String) = db
            .with_connection(|conn| {
                conn.execute(
                    "INSERT INTO sessions (started_at, model, created_at)
                     VALUES ('2026-08-25T10:00:00Z', 'gpt-realtime-2.1', '2026-08-25T10:00:00Z')",
                    [],
                )?;
                let session_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO tool_events (session_id, tool_name, risk_level, approval_state, result_summary, created_at)
                     VALUES (?1, 'open_project', 1, 'approved', 'Proje VS Code ile acildi.', '2026-08-25T10:01:00Z')",
                    [session_id],
                )?;
                conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;
                conn.query_row(
                    "SELECT count(*), max(session_id), max(tool_name) FROM tool_events",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })
            .expect("sorgu calismali");

        assert_eq!(remaining, 1, "audit satiri oturumla birlikte silinmis");
        assert_eq!(link, None, "kopan referans NULL'a cekilmeli");
        assert_eq!(tool, "open_project", "audit icerigi degismemeli");
    }

    /// Migration 4 var olan veriye dokunmaz: sema 3'te yazilmis hafiza, oturum
    /// ve proje satirlari yerinde kalir (yeni tablo ekleniyor, hicbiri yeniden
    /// yaratilmiyor).
    #[test]
    fn migration_four_only_adds_a_table() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("FK zorlamasi acilmali");
        migrations()
            .to_version(&mut connection, 3)
            .expect("sema 3 uygulanmali");

        connection
            .execute_batch(
                "INSERT INTO projects (id, name, path, status, created_at, updated_at)
                 VALUES ('asuna', 'Asuna', '/tmp/asuna-004', 'active', '2026-08-25T10:00:00Z', '2026-08-25T10:00:00Z');

                 INSERT INTO sessions (id, started_at, project_id, model, created_at)
                 VALUES (1, '2026-08-25T10:00:00Z', 'asuna', 'gpt-realtime-2.1', '2026-08-25T10:00:00Z');

                 INSERT INTO memories (id, kind, title, content, project_id, importance, confidence, source_session_id, created_at, updated_at)
                 VALUES (1, 'decision', 'Wake word yerel', 'Cihazda kalir.', 'asuna', 0.9, 1.0, 1, '2026-08-25T10:05:00Z', '2026-08-25T10:05:00Z');",
            )
            .expect("sema 3 verisi yazilmali");

        apply(&mut connection).expect("sema 4'e yukseltilmeli");

        let counts: (i64, i64, i64, i64) = connection
            .query_row(
                "SELECT (SELECT count(*) FROM projects),
                        (SELECT count(*) FROM sessions),
                        (SELECT count(*) FROM memories),
                        (SELECT count(*) FROM tool_events)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("sayilar okunmali");
        assert_eq!(
            counts,
            (1, 1, 1, 0),
            "migration 4 var olan veriyi degistirdi"
        );

        // Hafiza→oturum bagi da yerinde: yeniden yaratma olmadigi icin hicbir
        // ortuk DELETE tetiklenmedi.
        let link: Option<i64> = connection
            .query_row(
                "SELECT source_session_id FROM memories WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("bag okunmali");
        assert_eq!(link, Some(1));
    }

    /// Geri alma tabloyu dusurur ve ileri sarim yeniden kurar; kullanicinin
    /// hafizasi/oturumlari bu yolculuktan etkilenmez.
    #[test]
    fn migration_four_can_be_rolled_back_and_reapplied() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        apply(&mut connection).expect("migration uygulanmali");
        connection
            .execute(
                "INSERT INTO tool_events (tool_name, risk_level, approval_state, created_at)
                 VALUES ('get_current_project', 0, 'not_required', '2026-08-25T10:00:00Z')",
                [],
            )
            .expect("audit satiri yazilmali");

        migrations()
            .to_version(&mut connection, 3)
            .expect("sema 3'e donulebilmeli");

        let remaining: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'tool_events'",
                [],
                |row| row.get(0),
            )
            .expect("sqlite_master okunmali");
        assert_eq!(remaining, 0, "down migration tabloyu birakmis");

        apply(&mut connection).expect("yeniden ileri sarilmali");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version okunmali");
        assert_eq!(version, EXPECTED_SCHEMA_VERSION);
    }

    // -----------------------------------------------------------------------
    // Migration 5 — `tool_events.outcome` (ASU-051)
    // -----------------------------------------------------------------------

    /// Sema 4 doneminde yazilmis satirlar kolonu NULL ile karsilar ve
    /// **oldugu gibi** kalir. Geriye donuk doldurma yok: `approved` bir satirin
    /// basarili bittigini soylemez, uydurmuyoruz.
    #[test]
    fn migration_five_leaves_older_audit_rows_untouched_with_a_null_outcome() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        migrations()
            .to_version(&mut connection, 4)
            .expect("sema 4 uygulanmali");

        connection
            .execute(
                "INSERT INTO tool_events (tool_name, risk_level, approval_state, result_summary, created_at)
                 VALUES ('get_current_project', 0, 'not_required', 'Proje: Asuna', '2026-08-25T10:00:00Z')",
                [],
            )
            .expect("sema 4 audit satiri yazilmali");

        apply(&mut connection).expect("sema 5'e yukseltilmeli");

        let (count, outcome, summary): (i64, Option<String>, Option<String>) = connection
            .query_row(
                "SELECT (SELECT count(*) FROM tool_events), outcome, result_summary
                   FROM tool_events WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("satir okunmali");

        assert_eq!(count, 1, "migration 5 audit satirini dusurdu");
        assert_eq!(outcome, None, "eski satira olculmemis bir sonuc yazilmis");
        assert_eq!(summary.as_deref(), Some("Proje: Asuna"));
    }

    /// Kume semada zorlaniyor: uydurma bir sonuc etiketi INSERT aninda duser.
    #[test]
    fn the_outcome_check_constraint_rejects_unknown_values() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        apply(&mut connection).expect("migration uygulanmali");

        for value in ["basarili", "SUCCEEDED", "skipped", ""] {
            let error = connection.execute(
                "INSERT INTO tool_events (tool_name, risk_level, approval_state, outcome, created_at)
                 VALUES ('read_project_file', 0, 'not_required', ?1, '2026-08-25T10:00:00Z')",
                rusqlite::params![value],
            );
            assert!(
                error.is_err(),
                "`{value}` semadan gecti — CHECK kisiti kume disi degeri kabul ediyor"
            );
        }

        for value in outcomes_declared_in_schema() {
            connection
                .execute(
                    "INSERT INTO tool_events (tool_name, risk_level, approval_state, outcome, created_at)
                     VALUES ('read_project_file', 0, 'not_required', ?1, '2026-08-25T10:00:00Z')",
                    rusqlite::params![value],
                )
                .unwrap_or_else(|error| panic!("`{value}` semadan gecmeliydi: {error}"));
        }
    }

    /// Kume sema metninden okunabiliyor — Rust enum'u ve TypeScript sabiti bu
    /// listeye baglanacak.
    #[test]
    fn the_outcome_set_is_readable_from_the_schema_text() {
        assert_eq!(
            outcomes_declared_in_schema(),
            vec![
                "succeeded".to_owned(),
                "failed".to_owned(),
                "not_run".to_owned()
            ]
        );
    }

    /// Geri alma yalnizca kolonu dusurur: audit satirlari, onay durumlari ve
    /// sonuc ozetleri yerinde kalir. Ileri sarim kolonu yeniden kurar.
    #[test]
    fn migration_five_can_be_rolled_back_without_losing_audit_rows() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        apply(&mut connection).expect("migration uygulanmali");
        connection
            .execute(
                "INSERT INTO tool_events (tool_name, risk_level, approval_state, result_summary, outcome, created_at)
                 VALUES ('open_project', 1, 'approved', 'Proje editorde acildi.', 'succeeded', '2026-08-25T10:00:00Z')",
                [],
            )
            .expect("audit satiri yazilmali");

        migrations()
            .to_version(&mut connection, 4)
            .expect("sema 4'e donulebilmeli");

        let (count, summary, approval): (i64, Option<String>, String) = connection
            .query_row(
                "SELECT (SELECT count(*) FROM tool_events), result_summary, approval_state
                   FROM tool_events WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("satir okunmali");
        assert_eq!(count, 1, "geri alma audit satirini sildi");
        assert_eq!(summary.as_deref(), Some("Proje editorde acildi."));
        assert_eq!(approval, "approved");

        let has_outcome: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('tool_events') WHERE name = 'outcome'",
                [],
                |row| row.get(0),
            )
            .expect("table_info okunmali");
        assert_eq!(has_outcome, 0, "down migration kolonu birakmis");

        apply(&mut connection).expect("yeniden ileri sarilmali");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version okunmali");
        assert_eq!(version, EXPECTED_SCHEMA_VERSION);
    }

    // -----------------------------------------------------------------------
    // Migration 6 — Chat Shell: `sessions.title` / `.modality` + `messages` +
    // `attachments` (plan-chat-shell.md WP1)
    // -----------------------------------------------------------------------

    /// Kume metinleri sema metninden okunabiliyor — Rust enum'lari ve
    /// TypeScript sabitleri bu satirlara baglanacak.
    #[test]
    fn the_chat_value_sets_are_readable_from_the_schema_text() {
        assert_eq!(modalities_declared_in_schema(), ["voice", "text"]);
        assert_eq!(
            message_roles_declared_in_schema(),
            ["user", "assistant", "system", "tool"]
        );
        assert_eq!(
            attachment_origins_declared_in_schema(),
            ["upload", "project"]
        );
    }

    /// 006 oncesindeki oturumlar **oldugu gibi** kalir: baslik NULL, modalite
    /// `voice`. Bu bir tahmin degil — metin sohbeti bu migration'dan once
    /// yoktu, yani her eski satir gercekten bir ses oturumu.
    #[test]
    fn migration_six_marks_existing_sessions_as_voice_without_a_title() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("FK zorlamasi acilmali");
        migrations()
            .to_version(&mut connection, 5)
            .expect("sema 5 uygulanmali");

        connection
            .execute(
                "INSERT INTO sessions (id, started_at, summary, model, created_at, end_reason)
                 VALUES (1, '2026-08-25T10:00:00Z', 'Ozet.', 'gpt-realtime-2.1', '2026-08-25T10:00:00Z', 'completed')",
                [],
            )
            .expect("sema 5 oturumu yazilmali");

        apply(&mut connection).expect("sema 6'ya yukseltilmeli");

        let (title, modality, summary): (Option<String>, String, Option<String>) = connection
            .query_row(
                "SELECT title, modality, summary FROM sessions WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("oturum okunmali");
        assert_eq!(title, None, "eski oturuma baslik uydurulmus");
        assert_eq!(modality, "voice");
        assert_eq!(summary.as_deref(), Some("Ozet."), "ozet degismemeli");
    }

    #[test]
    fn the_modality_and_role_and_origin_checks_reject_unknown_values() {
        let db = fresh_db();

        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO sessions (started_at, model, created_at, modality)
                 VALUES ('2026-08-25T10:00:00Z', 'm', '2026-08-25T10:00:00Z', 'video')",
                [],
            )
        })
        .expect_err("bilinmeyen modality reddedilmeli");

        let session_id = insert_session(&db);

        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO messages (session_id, role, content, created_at)
                 VALUES (?1, 'developer', 'merhaba', '2026-08-25T10:00:00Z')",
                params![session_id],
            )
        })
        .expect_err("bilinmeyen role reddedilmeli");

        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO attachments (session_id, file_name, origin, content, created_at)
                 VALUES (?1, 'a.txt', 'indirme', 'x', '2026-08-25T10:00:00Z')",
                params![session_id],
            )
        })
        .expect_err("bilinmeyen origin reddedilmeli");
    }

    /// Bos mesaj yazilamaz ("gonderdim ama bos" ile "gondermedim" ayni
    /// gorunmemeli) ve bos baslik da yazilamaz (NULL kullanilir).
    #[test]
    fn empty_content_and_empty_title_are_rejected_by_the_schema() {
        let db = fresh_db();
        let session_id = insert_session(&db);

        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO messages (session_id, role, content, created_at)
                 VALUES (?1, 'user', '', '2026-08-25T10:00:00Z')",
                params![session_id],
            )
        })
        .expect_err("bos mesaj reddedilmeli");

        db.with_connection(|conn| {
            conn.execute(
                "UPDATE sessions SET title = '' WHERE id = ?1",
                params![session_id],
            )
        })
        .expect_err("bos baslik reddedilmeli (NULL kullanilmali)");

        db.with_connection(|conn| {
            conn.execute(
                "UPDATE sessions SET title = ?2 WHERE id = ?1",
                params![session_id, "b".repeat(201)],
            )
        })
        .expect_err("tavani asan baslik reddedilmeli");
    }

    /// Attachment icerigi icin tavan: komut katmanindaki kirpma bir gun
    /// atlanirsa dosya DB'ye sessizce sizmaz.
    #[test]
    fn oversized_attachment_content_is_rejected_by_the_schema() {
        let db = fresh_db();
        let session_id = insert_session(&db);

        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO attachments (session_id, file_name, origin, content, created_at)
                 VALUES (?1, 'buyuk.txt', 'upload', ?2, '2026-08-25T10:00:00Z')",
                params![session_id, "x".repeat(32_001)],
            )
        })
        .expect_err("tavani asan icerik reddedilmeli");

        // Bos icerik gecerli: gercekten bos bir dosya eklenmis olabilir.
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO attachments (session_id, file_name, origin, content, created_at)
                 VALUES (?1, 'bos.txt', 'upload', '', '2026-08-25T10:00:00Z')",
                params![session_id],
            )
        })
        .expect("bos dosya icerigi kabul edilmeli");
    }

    /// **Kabul kriteri 2**: konusmayi silmek mesajlari ve eklenen dosyalarin
    /// icerigini gercekten goturur. `tool_events`in tam tersi davranis ve
    /// gerekcesi 006'nin bas yorumunda.
    #[test]
    fn deleting_a_session_cascades_to_messages_and_attachments() {
        let db = fresh_db();
        let session_id = insert_session(&db);

        let (messages, attachments, events) = db
            .with_connection(|conn| {
                conn.execute(
                    "INSERT INTO messages (session_id, role, content, created_at)
                     VALUES (?1, 'user', 'merhaba', '2026-08-25T10:00:00Z')",
                    params![session_id],
                )?;
                let message_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO attachments (session_id, message_id, file_name, origin, content, created_at)
                     VALUES (?1, ?2, 'notlar.md', 'upload', 'gizli olmayan metin', '2026-08-25T10:00:00Z')",
                    params![session_id, message_id],
                )?;
                // Audit satiri ayni oturuma bagli: CASCADE **ona** bulasmamali.
                conn.execute(
                    "INSERT INTO tool_events (session_id, tool_name, risk_level, approval_state, created_at)
                     VALUES (?1, 'read_project_file', 0, 'not_required', '2026-08-25T10:00:00Z')",
                    params![session_id],
                )?;

                conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;

                conn.query_row(
                    "SELECT (SELECT count(*) FROM messages),
                            (SELECT count(*) FROM attachments),
                            (SELECT count(*) FROM tool_events)",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
                )
            })
            .expect("sorgu calismali");

        assert_eq!(messages, 0, "mesajlar konusmayla birlikte gitmeliydi");
        assert_eq!(attachments, 0, "eklenen dosya icerigi DB'de kalmis");
        assert_eq!(events, 1, "audit defteri konusma silinince silinmemeli");
    }

    /// Mesaj silinirse ekin **kaydi** kalir, yalnizca bagi kopar: bekleyen bir
    /// ek ile silinmis bir ek ayni gorunmemeli.
    #[test]
    fn deleting_a_message_only_clears_the_attachment_link() {
        let db = fresh_db();
        let session_id = insert_session(&db);

        let (remaining, link): (i64, Option<i64>) = db
            .with_connection(|conn| {
                conn.execute(
                    "INSERT INTO messages (session_id, role, content, created_at)
                     VALUES (?1, 'user', 'merhaba', '2026-08-25T10:00:00Z')",
                    params![session_id],
                )?;
                let message_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO attachments (session_id, message_id, file_name, origin, content, created_at)
                     VALUES (?1, ?2, 'notlar.md', 'project', 'metin', '2026-08-25T10:00:00Z')",
                    params![session_id, message_id],
                )?;
                conn.execute("DELETE FROM messages WHERE id = ?1", params![message_id])?;
                conn.query_row(
                    "SELECT count(*), max(message_id) FROM attachments",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .expect("sorgu calismali");

        assert_eq!(remaining, 1);
        assert_eq!(link, None, "kopan referans NULL'a cekilmeli");
    }

    #[test]
    fn a_message_cannot_belong_to_an_unknown_session() {
        let db = fresh_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO messages (session_id, role, content, created_at)
                 VALUES (4242, 'user', 'merhaba', '2026-08-25T10:00:00Z')",
                [],
            )
        })
        .expect_err("var olmayan oturum referansi reddedilmeli");
    }

    #[test]
    fn chat_timestamps_must_be_utc_iso_8601() {
        let db = fresh_db();
        let session_id = insert_session(&db);

        for stamp in [
            "1756108800",
            "2026-08-25 10:00:00",
            "2026-08-25T10:00:00+03:00",
        ] {
            db.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO messages (session_id, role, content, created_at)
                     VALUES (?1, 'user', 'merhaba', ?2)",
                    params![session_id, stamp],
                )
            })
            .expect_err("UTC ISO-8601 disi zaman damgasi reddedilmeli");
        }
    }

    /// Geri alma metin sohbetini kaldirir ama `sessions` satirlarina
    /// dokunmaz; ileri sarim tablolari ve kolonlari yeniden kurar.
    #[test]
    fn migration_six_can_be_rolled_back_without_losing_the_sessions() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("FK zorlamasi acilmali");
        apply(&mut connection).expect("migration uygulanmali");

        connection
            .execute_batch(
                "INSERT INTO sessions (id, started_at, model, created_at, title, modality)
                 VALUES (1, '2026-08-31T10:00:00Z', 'gpt-4o-mini', '2026-08-31T10:00:00Z', 'Ilk konusma', 'text');

                 INSERT INTO messages (session_id, role, content, created_at)
                 VALUES (1, 'user', 'merhaba', '2026-08-31T10:00:01Z');

                 INSERT INTO attachments (session_id, file_name, origin, content, created_at)
                 VALUES (1, 'notlar.md', 'upload', 'metin', '2026-08-31T10:00:02Z');",
            )
            .expect("konusma yazilmali");

        migrations()
            .to_version(&mut connection, 5)
            .expect("sema 5'e donulebilmeli");

        let leftovers: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master
                  WHERE type = 'table' AND name IN ('messages', 'attachments')",
                [],
                |row| row.get(0),
            )
            .expect("sqlite_master okunmali");
        assert_eq!(leftovers, 0, "down migration tablolari birakmis");

        let chat_columns: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('sessions')
                  WHERE name IN ('title', 'modality')",
                [],
                |row| row.get(0),
            )
            .expect("table_info okunmali");
        assert_eq!(chat_columns, 0, "down migration kolonlari birakmis");

        let (sessions, model): (i64, String) = connection
            .query_row(
                "SELECT (SELECT count(*) FROM sessions), model FROM sessions WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("oturum okunmali");
        assert_eq!(sessions, 1, "geri alma oturum kaydini silmis");
        assert_eq!(model, "gpt-4o-mini");

        apply(&mut connection).expect("yeniden ileri sarilmali");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version okunmali");
        assert_eq!(version, EXPECTED_SCHEMA_VERSION);
    }

    /// Testlerde kullanilan minimal oturum satiri.
    fn insert_session(db: &crate::db::AsunaDb) -> i64 {
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO sessions (started_at, model, created_at)
                 VALUES ('2026-08-25T10:00:00Z', 'gpt-realtime-2.1', '2026-08-25T10:00:00Z')",
                [],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .expect("oturum yazilmali")
    }
}

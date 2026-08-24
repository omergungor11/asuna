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

/// Sirali migration tanimlari.
///
/// **Bu vektore yalnizca sona ekleme yapilir.** Araya ekleme ya da silme, daha
/// once uygulanmis surumlerin anlamini degistirir.
fn definitions() -> Vec<M<'static>> {
    vec![M::up(V1_UP).down(V1_DOWN)]
}

/// Bu kod surumunun bekledigi sema surumu (`PRAGMA user_version`).
pub const EXPECTED_SCHEMA_VERSION: u32 = 1;

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
pub fn kinds_declared_in_schema() -> Vec<String> {
    const MARKER: &str = "CHECK (kind IN (";

    let start = V1_UP
        .find(MARKER)
        .expect("`memories.kind` CHECK kisiti semada bulunmali")
        + MARKER.len();
    let rest = &V1_UP[start..];
    let end = rest.find(')').expect("`kind` CHECK kisiti kapanmali");

    rest[..end]
        .split(',')
        .map(|item| item.trim().trim_matches('\'').to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
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
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "`{expected}` index'i yok. Mevcut: {names:?}"
            );
        }
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
}

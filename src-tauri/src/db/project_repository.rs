//! `projects` tablosunun satir katmani (ASU-039).
//!
//! # Bu modul ne yapar, ne yapmaz
//!
//! Burasi **yalnizca SQL**. Yol normalizasyonu, `canonicalize`, symlink cozumu,
//! slug uretimi ve "bu dizin gercekten var mi?" sorusu bir kat yukarida,
//! [`crate::projects::registry`] icindedir (ASU-040). Sebep: dosya sistemine
//! dokunan mantik transaction icinde calismamali ve birim testi diske ihtiyac
//! duymadan kosabilmeli.
//!
//! # Etiket ≠ kayit
//!
//! [`ensure_label`] bir `projects` satiri **acabilir**, ama actigi satir her
//! zaman `unlinked`'tir: yolu yoktur, dolayisiyla hicbir dosya sistemi yetkisi
//! tasimaz (ASU-049 sandbox'i yalnizca `path`i dolu kayitlari gorecek).
//!
//! Neden gerekli: `memories.project_id` 001'den beri **serbest metindi** ve
//! bugun de hafiza cikarimi (`extraction.rs`) modelden gelen bir `projectId`
//! yazabiliyor. 003 ile bu kolon `projects(id)`'ye FK oldu; etiketi olan ama
//! karsiligi olmayan bir yazim artik FK ihlali verirdi. Iki kotu secenek vardi:
//! etiketi sessizce NULL'a cekmek (kullanicinin verisini bozmak) ya da yazimi
//! reddetmek (hafizayi kaybetmek). Ucuncu yol: etiketi **oldugu gibi sakla** ve
//! ona yolsuz bir ev ac. Migration 003 gecmis veriye ayni kurali uyguluyor;
//! burasi ayni kuralin ileriye donuk hali.

use rusqlite::{params, Connection, OptionalExtension};

use super::model::{ProjectRecord, ProjectStatus};
use super::{AsunaDb, DbError};

/// Serbest metin etiketin `projects` tablosunda bir karsiligi oldugundan emin
/// olur; yoksa `unlinked` bir satir acar.
///
/// Var olan satir **hicbir sekilde degistirilmez**: kayitli (`active`) bir
/// projenin adi, yolu ya da durumu bir hafiza yaziminin yan etkisi olarak
/// degisemez.
pub fn ensure_label(connection: &Connection, label: &str, now: &str) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO projects (id, name, path, status, created_at, updated_at)
         VALUES (?1, ?1, NULL, 'unlinked', ?2, ?2)",
        params![label, now],
    )?;
    Ok(())
}

/// [`ensure_label`]'in `Option` alan hali — cagiran tarafta `if let` tekrarini
/// onler.
pub fn ensure_optional_label(
    connection: &Connection,
    label: Option<&str>,
    now: &str,
) -> rusqlite::Result<()> {
    match label {
        Some(label) => ensure_label(connection, label, now),
        None => Ok(()),
    }
}

/// Tek projeyi kimligiyle okur.
pub fn find_by_id(db: &AsunaDb, id: &str) -> Result<Option<ProjectRecord>, DbError> {
    db.with_connection(|connection| {
        connection
            .query_row(
                &format!(
                    "SELECT {} FROM projects WHERE id = ?1",
                    ProjectRecord::select_columns()
                ),
                params![id],
                ProjectRecord::from_row,
            )
            .optional()
    })
}

/// Tek projeyi **normalize edilmis** yoluyla okur.
///
/// Yol karsilastirmasi metin esitligidir; bu yalnizca yolun tabloya
/// `canonicalize` edilmis halde yazildigi icin dogru (bkz. registry).
pub fn find_by_path(db: &AsunaDb, path: &str) -> Result<Option<ProjectRecord>, DbError> {
    db.with_connection(|connection| {
        connection
            .query_row(
                &format!(
                    "SELECT {} FROM projects WHERE path = ?1",
                    ProjectRecord::select_columns()
                ),
                params![path],
                ProjectRecord::from_row,
            )
            .optional()
    })
}

/// Tum projeler. Sira: once son acilan, sonra ad.
///
/// `last_opened_at` NULL olanlar sona duser — hic acilmamis bir proje "en eski"
/// degil, **bilinmiyor**dur ve listenin basini kapatmamalidir.
pub fn list_all(db: &AsunaDb) -> Result<Vec<ProjectRecord>, DbError> {
    db.with_connection(|connection| {
        let mut statement = connection.prepare(&format!(
            "SELECT {} FROM projects
              ORDER BY last_opened_at IS NULL, last_opened_at DESC, name COLLATE NOCASE",
            ProjectRecord::select_columns()
        ))?;
        let rows = statement.query_map([], ProjectRecord::from_row)?;
        rows.collect::<rusqlite::Result<Vec<ProjectRecord>>>()
    })
}

/// En son acilan **kayitli** proje (`unlinked` haric).
///
/// "Guncel proje" tahmini burada uretilmez; bu yalnizca kullanicinin en son
/// acik olarak sectigi projeyi hatirlar (ASU-041 belirsizlikte `unknown` doner).
pub fn most_recently_opened(db: &AsunaDb) -> Result<Option<ProjectRecord>, DbError> {
    db.with_connection(|connection| {
        connection
            .query_row(
                &format!(
                    "SELECT {} FROM projects
                      WHERE last_opened_at IS NOT NULL AND status != '{}'
                      ORDER BY last_opened_at DESC, id
                      LIMIT 1",
                    ProjectRecord::select_columns(),
                    ProjectStatus::Unlinked.as_str()
                ),
                [],
                ProjectRecord::from_row,
            )
            .optional()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-08-25T10:00:00Z";

    fn db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB")
    }

    #[test]
    fn ensure_label_creates_an_unlinked_row_once() {
        let db = db();
        db.with_connection(|connection| {
            ensure_label(connection, "asuna", NOW)?;
            ensure_label(connection, "asuna", NOW)?;
            Ok(())
        })
        .expect("etiket acilmali");

        let record = find_by_id(&db, "asuna")
            .expect("okunmali")
            .expect("satir olmali");
        assert_eq!(record.status, ProjectStatus::Unlinked);
        assert_eq!(record.path, None, "etiketin yolu uydurulmaz");
        assert_eq!(record.name, "asuna");
        assert_eq!(record.last_opened_at, None);
    }

    /// Kayitli bir projenin uzerine hafiza yazimi **yazamaz**: `ensure_label`
    /// var olan satiri gormezden gelir.
    #[test]
    fn ensure_label_never_overwrites_a_registered_project() {
        let db = db();
        db.with_connection(|connection| {
            connection.execute(
                "INSERT INTO projects (id, name, path, status, created_at, updated_at)
                 VALUES ('asuna', 'Asuna', '/tmp/asuna', 'active', ?1, ?1)",
                params![NOW],
            )?;
            ensure_label(connection, "asuna", "2027-01-01T00:00:00Z")?;
            Ok(())
        })
        .expect("calismali");

        let record = find_by_id(&db, "asuna")
            .expect("okunmali")
            .expect("satir olmali");
        assert_eq!(record.status, ProjectStatus::Active);
        assert_eq!(record.path.as_deref(), Some("/tmp/asuna"));
        assert_eq!(record.name, "Asuna");
        assert_eq!(record.updated_at, NOW);
    }

    #[test]
    fn lists_recently_opened_first_and_never_guesses_the_current_project() {
        let db = db();
        db.with_connection(|connection| {
            connection.execute(
                "INSERT INTO projects (id, name, path, status, created_at, updated_at, last_opened_at)
                 VALUES ('bir', 'Bir', '/tmp/bir', 'active', ?1, ?1, '2026-08-20T10:00:00Z'),
                        ('iki', 'Iki', '/tmp/iki', 'active', ?1, ?1, '2026-08-24T10:00:00Z'),
                        ('uc',  'Uc',  '/tmp/uc',  'active', ?1, ?1, NULL)",
                params![NOW],
            )?;
            ensure_label(connection, "eski-etiket", NOW)?;
            Ok(())
        })
        .expect("kayitlar yazilmali");

        let ids: Vec<String> = list_all(&db)
            .expect("listelenmeli")
            .into_iter()
            .map(|project| project.id)
            .collect();
        assert_eq!(ids, ["iki", "bir", "eski-etiket", "uc"]);

        let current = most_recently_opened(&db)
            .expect("okunmali")
            .expect("bir proje olmali");
        assert_eq!(current.id, "iki");
    }

    /// Hic acilmamis proje varken bile "guncel proje" uydurulmaz.
    #[test]
    fn most_recently_opened_is_empty_when_nothing_was_ever_opened() {
        let db = db();
        db.with_connection(|connection| ensure_label(connection, "etiket", NOW))
            .expect("etiket");
        assert!(most_recently_opened(&db).expect("okunmali").is_none());
    }

    #[test]
    fn finds_a_project_by_its_normalised_path() {
        let db = db();
        db.with_connection(|connection| {
            connection.execute(
                "INSERT INTO projects (id, name, path, status, created_at, updated_at)
                 VALUES ('asuna', 'Asuna', '/tmp/asuna', 'active', ?1, ?1)",
                params![NOW],
            )
        })
        .expect("kayit");

        assert!(find_by_path(&db, "/tmp/asuna").expect("okunmali").is_some());
        // Sondaki egik cizgi normalize edilmis bir yol degil — eslesmemeli.
        assert!(find_by_path(&db, "/tmp/asuna/")
            .expect("okunmali")
            .is_none());
    }

    #[test]
    fn optional_label_is_a_no_op_for_none() {
        let db = db();
        db.with_connection(|connection| ensure_optional_label(connection, None, NOW))
            .expect("calismali");
        assert!(list_all(&db).expect("listelenmeli").is_empty());
    }
}

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

// ---------------------------------------------------------------------------
// Kayit islemleri (ASU-040 registry'nin SQL yuzu)
// ---------------------------------------------------------------------------
//
// Bu fonksiyonlarin hicbiri yol dogrulamasi yapmaz. Yolun mutlak, var olan,
// symlink'i cozulmus bir dizin oldugu `projects::registry` tarafinda garanti
// edilir; burada yalnizca semanin kisitlari (UNIQUE path, `unlinked <=> path
// IS NULL`) devrededir.

/// Kayitli bir projenin yaratilmasi icin gereken alanlar.
pub struct NewProject<'a> {
    pub id: &'a str,
    pub name: &'a str,
    /// Normalize edilmis, symlink'i cozulmus mutlak dizin yolu.
    pub path: &'a str,
}

/// Yeni bir **kayitli** (`active`) proje ekler.
pub fn insert_registered(
    connection: &Connection,
    new: &NewProject<'_>,
    now: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO projects (id, name, path, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'active', ?4, ?4)",
        params![new.id, new.name, new.path, now],
    )?;
    Ok(())
}

/// Devralinan bir etiketi (`unlinked`) gercek bir kayda **yukseltir**.
///
/// `WHERE ... status = 'unlinked'`: kayitli bir projenin yolu bu yoldan
/// degistirilemez. Donen deger 0 ise satir zaten kayitliydi ve cagiran taraf
/// baska bir id secmek zorunda.
///
/// Neden onemli: Phase 3'te `project_id = 'asuna'` yazilmis hafizalar, kullanici
/// o dizini ilk kez kaydettiginde **kendiliginden** dogru projeye baglanir.
/// Yeni bir satir acilsaydi eski hafizalar oksuz kalirdi (ASU-039 karari).
pub fn adopt_unlinked(
    connection: &Connection,
    id: &str,
    name: &str,
    path: &str,
    now: &str,
) -> rusqlite::Result<usize> {
    connection.execute(
        "UPDATE projects
            SET name = ?2, path = ?3, status = 'active', updated_at = ?4
          WHERE id = ?1 AND status = 'unlinked'",
        params![id, name, path, now],
    )
}

/// Transaction icinden tek satir okur.
pub fn load(connection: &Connection, id: &str) -> rusqlite::Result<Option<ProjectRecord>> {
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
}

/// Yolu **olan** bir kaydin durumunu degistirir (`active` / `missing` /
/// `archived`).
///
/// `unlinked` bu yoldan yazilamaz: sema `unlinked <=> path IS NULL` kisitini
/// zorlar ve yolu bosaltmak ayri bir islemdir ([`demote_to_unlinked`]).
pub fn set_status(
    connection: &Connection,
    id: &str,
    status: ProjectStatus,
    now: &str,
) -> rusqlite::Result<usize> {
    debug_assert!(
        status.has_registered_root(),
        "`unlinked` icin `demote_to_unlinked` kullanilmali"
    );
    connection.execute(
        "UPDATE projects SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status, now],
    )
}

/// "Guncel proje" secimi: `last_opened_at` tazelenir.
///
/// Tahmin yok — bu yalnizca kullanicinin acik seciminde cagrilir (ASU-040).
pub fn touch_last_opened(connection: &Connection, id: &str, now: &str) -> rusqlite::Result<usize> {
    connection.execute(
        "UPDATE projects SET last_opened_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )
}

/// Bu projeye bagli hafiza + oturum sayisi.
///
/// Kaydi kaldirirken satiri gercekten silmek ile etikete dusurmek arasindaki
/// karari bu sayi belirler (bkz. `registry::remove`).
pub fn reference_count(connection: &Connection, id: &str) -> rusqlite::Result<i64> {
    connection.query_row(
        "SELECT (SELECT count(*) FROM memories WHERE project_id = ?1)
              + (SELECT count(*) FROM sessions WHERE project_id = ?1)",
        params![id],
        |row| row.get(0),
    )
}

/// Kaydi etikete dusurur: yol silinir, satir ve etiket korunur.
///
/// Kullanici projeyi kayittan cikardiginda **hafizasini kaybetmemeli**. Satir
/// silinseydi FK `ON DELETE SET NULL` tum `project_id` degerlerini bosaltir ve
/// "proje X'te alinan karar" baglami kalici olarak kaybolurdu.
pub fn demote_to_unlinked(connection: &Connection, id: &str, now: &str) -> rusqlite::Result<usize> {
    connection.execute(
        "UPDATE projects SET path = NULL, status = 'unlinked', updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )
}

/// Satiri gercekten siler. Yalnizca hicbir hafiza/oturum baglamiyorken.
pub fn delete(connection: &Connection, id: &str) -> rusqlite::Result<usize> {
    connection.execute("DELETE FROM projects WHERE id = ?1", params![id])
}

/// Verilen kolonlari gunceller; `updated_at` her zaman tazelenir.
///
/// Kolon adlari **cagiran tarafta sabit metindir**, kullanici girdisi degil
/// (`registry::ProjectPatch`); deger tarafi her zaman parametredir.
pub fn apply_patch(
    connection: &Connection,
    id: &str,
    assignments: &[(&'static str, rusqlite::types::Value)],
    now: &str,
) -> rusqlite::Result<usize> {
    if assignments.is_empty() {
        return connection.execute(
            "UPDATE projects SET updated_at = ?2 WHERE id = ?1",
            params![id, now],
        );
    }

    let clause = assignments
        .iter()
        .map(|(column, _)| format!("{column} = ?"))
        .collect::<Vec<String>>()
        .join(", ");

    let mut values: Vec<rusqlite::types::Value> =
        assignments.iter().map(|(_, value)| value.clone()).collect();
    values.push(rusqlite::types::Value::Text(now.to_owned()));
    values.push(rusqlite::types::Value::Text(id.to_owned()));

    connection.execute(
        &format!("UPDATE projects SET {clause}, updated_at = ? WHERE id = ?"),
        rusqlite::params_from_iter(values.iter()),
    )
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

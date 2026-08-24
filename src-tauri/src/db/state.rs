//! Veritabaninin uygulama omru boyunca tutulan durumu + `db_status` komutu.
//!
//! # Neden bir enum
//!
//! PROJECT.md Bolum 30: **"Memory database error → continue conversation
//! without memory and surface status."** Yani DB hatasi bir panic degil, bir
//! *urun durumu*. [`DbState`] bu durumu tip duzeyinde temsil eder; boylece
//! ilerideki bir repository cagrisi "DB var" varsayimini yapamaz — `Option`
//! ya da tipli hata almak zorunda kalir.
//!
//! Uc durum bilerek ayri tutuluyor:
//!
//! - [`DbState::Ready`] — hafiza calisiyor.
//! - [`DbState::Disabled`] — kullanici `ASUNA_MEMORY_ENABLED=false` dedi. DB
//!   dosyasi **hic acilmaz**; bu bir gizlilik garantisidir (PROJECT.md Bolum 20).
//! - [`DbState::Unavailable`] — acilis/migration basarisiz. Bu bir arizadir ve
//!   UI'da "kapali" ile ayni gorunmemelidir; kullanici hafizasinin neden
//!   calismadigini bilmeli.

use serde::Serialize;
use tauri::State;

use super::{describe_error_chain, AsunaDb};

/// Renderer'a giden hafiza durumu etiketi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DbAvailability {
    /// Hafiza calisiyor.
    Ready,
    /// `ASUNA_MEMORY_ENABLED=false` — kullanici kapatti, ariza yok.
    Disabled,
    /// Acilis ya da migration basarisiz — hafizasiz degrade mod.
    Unavailable,
}

/// Tauri `manage()` ile tutulan DB durumu.
pub enum DbState {
    Ready(AsunaDb),
    Disabled,
    Unavailable {
        /// Kisa, kullaniciya gosterilebilir neden. Dosya yolu / SQL / kullanici
        /// icerigi **tasimaz** (bkz. [`super::DbError`] dokumantasyonu).
        reason: String,
    },
}

impl DbState {
    /// DB'yi acar ve durumu belirler. **Hicbir kosulda panic etmez ve hata
    /// firlatmaz** — cagrilan yerde `?` yoktur, cunku acilis basarisizligi
    /// uygulamayi durdurmamalidir.
    pub fn initialize<R: tauri::Runtime>(app: &tauri::AppHandle<R>, memory_enabled: bool) -> Self {
        if !memory_enabled {
            eprintln!(
                "[asuna] ASUNA_MEMORY_ENABLED=false — kalici hafiza kapali, \
                 veritabani dosyasi acilmiyor."
            );
            return Self::Disabled;
        }

        match AsunaDb::open(app) {
            Ok(db) => {
                let version = db
                    .schema_version()
                    .map_or_else(|_| "bilinmiyor".to_owned(), |value| value.to_string());
                eprintln!(
                    "[asuna] Veritabani hazir (SQLite {}, sema surumu {version}).",
                    rusqlite::version()
                );
                Self::Ready(db)
            }
            Err(error) => {
                // Sessiz yutma yok: tam hata zinciri **yerel log'a** yazilir.
                // IPC'ye yalnizca kisa neden gider.
                eprintln!(
                    "[asuna] Veritabani acilamadi: {}",
                    describe_error_chain(&error)
                );
                eprintln!(
                    "[asuna] Asuna hafizasiz modda devam ediyor; oturum calisir, \
                     kalici hafiza yazilmaz (PROJECT.md Bolum 30)."
                );
                Self::Unavailable {
                    reason: error.to_string(),
                }
            }
        }
    }

    /// Acik DB — yoksa `None`. Cagiran taraf hafizasiz modu ele almak zorunda.
    pub fn database(&self) -> Option<&AsunaDb> {
        match self {
            Self::Ready(db) => Some(db),
            Self::Disabled | Self::Unavailable { .. } => None,
        }
    }

    pub fn availability(&self) -> DbAvailability {
        match self {
            Self::Ready(_) => DbAvailability::Ready,
            Self::Disabled => DbAvailability::Disabled,
            Self::Unavailable { .. } => DbAvailability::Unavailable,
        }
    }

    /// Durum ozeti. **Bu fonksiyon hata dondurmez**: durum sorgusunun kendisi
    /// patlarsa kullanici hicbir sey ogrenemez. Sorgu hatasi durumu
    /// `Unavailable`'a dusurur.
    pub fn status(&self) -> DbStatus {
        let sqlite_version = rusqlite::version().to_owned();

        match self {
            Self::Ready(db) => match db.schema_version() {
                Ok(schema_version) => DbStatus {
                    availability: DbAvailability::Ready,
                    schema_version: Some(schema_version),
                    sqlite_version,
                    reason: None,
                },
                Err(error) => DbStatus {
                    availability: DbAvailability::Unavailable,
                    schema_version: None,
                    sqlite_version,
                    reason: Some(error.to_string()),
                },
            },
            Self::Disabled => DbStatus {
                availability: DbAvailability::Disabled,
                schema_version: None,
                sqlite_version,
                reason: None,
            },
            Self::Unavailable { reason } => DbStatus {
                availability: DbAvailability::Unavailable,
                schema_version: None,
                sqlite_version,
                reason: Some(reason.clone()),
            },
        }
    }
}

impl std::fmt::Debug for DbState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready(_) => f.write_str("DbState::Ready"),
            Self::Disabled => f.write_str("DbState::Disabled"),
            Self::Unavailable { reason } => write!(f, "DbState::Unavailable({reason})"),
        }
    }
}

/// Renderer'a giden hafiza durumu (`serde` camelCase).
///
/// Bu tipin TypeScript aynasi: `src/shared/db-status.ts`. Alan eklenirse iki
/// taraf **ayni commit'te** guncellenir (ADR-005).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStatus {
    pub availability: DbAvailability,
    /// Uygulanmis sema surumu (`PRAGMA user_version`); yalnizca `ready` iken dolu.
    pub schema_version: Option<u32>,
    /// Gomulu SQLite surumu — makineden bagimsiz, tekrarlanabilir olmali.
    pub sqlite_version: String,
    /// Yalnizca `unavailable` iken dolu; kisa ve kullaniciya gosterilebilir.
    pub reason: Option<String>,
}

/// Hafiza alt sisteminin durumu.
///
/// Salt okunur ve icerik dondurmez: hicbir hafiza kaydi, dosya yolu ya da SQL
/// bu komuttan gecmez. Bu yuzden ayri (ve tek) bir "okuma" izniyle acilir.
#[tauri::command]
pub fn db_status(state: State<'_, DbState>) -> DbStatus {
    state.status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::EXPECTED_SCHEMA_VERSION;

    #[test]
    fn ready_state_reports_the_schema_version() {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB acilmali");
        let status = DbState::Ready(db).status();

        assert_eq!(status.availability, DbAvailability::Ready);
        assert_eq!(status.schema_version, Some(EXPECTED_SCHEMA_VERSION));
        assert_eq!(status.reason, None);
        assert!(!status.sqlite_version.is_empty());
    }

    /// Kapali hafiza bir ariza degil: `reason` bos kalir ki UI "bozuk" demesin.
    #[test]
    fn disabled_state_is_not_reported_as_a_failure() {
        let status = DbState::Disabled.status();

        assert_eq!(status.availability, DbAvailability::Disabled);
        assert_eq!(status.schema_version, None);
        assert_eq!(status.reason, None);
        assert!(DbState::Disabled.database().is_none());
    }

    #[test]
    fn unavailable_state_carries_a_reason() {
        let state = DbState::Unavailable {
            reason: "sema migration'lari uygulanamadi".to_owned(),
        };
        let status = state.status();

        assert_eq!(status.availability, DbAvailability::Unavailable);
        assert_eq!(
            status.reason.as_deref(),
            Some("sema migration'lari uygulanamadi")
        );
        assert!(state.database().is_none());
    }

    /// Renderer'in gordugu JSON sozlesmesi — `src/shared/db-status.ts` ile birebir.
    #[test]
    fn status_serializes_with_the_expected_contract() {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB acilmali");
        let json = serde_json::to_value(DbState::Ready(db).status()).expect("serialize");

        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["availability", "reason", "schemaVersion", "sqliteVersion"]
        );
        assert_eq!(json["availability"], "ready");

        assert_eq!(
            serde_json::to_value(DbState::Disabled.status()).expect("serialize")["availability"],
            "disabled"
        );
        assert_eq!(
            serde_json::to_value(
                DbState::Unavailable {
                    reason: "x".to_owned()
                }
                .status()
            )
            .expect("serialize")["availability"],
            "unavailable"
        );
    }

    /// `Debug` DB yolu ya da baglanti detayi sizdirmamali (log/panic yuzeyi).
    #[test]
    fn debug_output_stays_coarse() {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB acilmali");
        assert_eq!(format!("{:?}", DbState::Ready(db)), "DbState::Ready");
        assert_eq!(format!("{:?}", DbState::Disabled), "DbState::Disabled");
    }
}

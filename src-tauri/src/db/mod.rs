//! SQLite kalici depolama — **tek erisim noktasi** (ASU-029 / ADR-005).
//!
//! # Sozlesme
//!
//! - SQLite'a yalnizca bu process'ten erisilir. Renderer'a ham SQL yuzeyi
//!   acilmaz; webview yalnizca kaba taneli `#[tauri::command]`'lari cagirir
//!   (`state::db_status`, ilerleyen task'larda `memory_*`).
//! - **DB yolu renderer'dan parametre olarak alinmaz.** Yol
//!   [`resolve_db_path`] icinde Tauri path API'sinden turetilir; boylece
//!   `../..` gibi bir kacis yolu IPC sinirinde hic olusmaz (ADR-005 A/3'te
//!   olculen tuzak).
//! - Acilis sirasi ADR-005'te sabitlendi ve degistirilmez:
//!   `create_dir_all(parent)` → `journal_mode=WAL` → `foreign_keys=ON` →
//!   `synchronous=NORMAL` → `busy_timeout=5s` → `migrations().to_latest()`.
//! - Acilis basarisiz olursa **uygulama dusmez**: cagiran taraf
//!   ([`state::DbState`]) hafizasiz moda gecer ve durumu gorunur kilar
//!   (PROJECT.md Bolum 30).
//!
//! # Test edilebilirlik
//!
//! [`AsunaDb::open_in_memory`] ve [`AsunaDb::open_at`] Tauri app'i olmadan
//! calisir; birim testleri gercek uygulama veri dizinine **hicbir zaman**
//! yazmaz.

pub mod clock;
pub mod memory_repository;
pub mod migrations;
pub mod model;
pub mod retrieval;
pub mod session_repository;
pub mod state;
pub mod store_error;
pub mod transcript;

pub use migrations::EXPECTED_SCHEMA_VERSION;
pub use model::{MemoryKind, MemoryRecord, SessionEndReason, SessionRecord};
pub use retrieval::{build_bootstrap_context, SessionBootstrapContext};
pub use state::{db_status, DbAvailability, DbState, DbStatus};
pub use store_error::{StoreError, StoreErrorCode, StoreSkipReason};

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::Connection;
use tauri::Manager;
use thiserror::Error;

/// Uygulama veri dizini altindaki DB dosyasinin adi (ADR-005 "DB Dosya Konumu").
///
/// WAL modu yaninda `asuna.db-wal` ve `asuna.db-shm` kardes dosyalari uretir;
/// yedekleme/export ucunu birden kopyalamali ya da `VACUUM INTO` kullanmalidir.
pub const DB_FILE_NAME: &str = "asuna.db";

/// Baska bir yazicinin kilidi birakmasi icin beklenecek sure.
///
/// Tek kullanicili yerel bir DB'de cakisma nadirdir; yine de WAL modunda
/// checkpoint sirasinda kisa kilitler olusur. 5 sn "kilit varsa bekle, yoksa
/// durust bir hata ver" dengesi (ADR-005).
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Gelistirme build'lerinde DB yolunu ezen ortam degiskeni.
///
/// GUVENLIK: yalnizca `#[cfg(debug_assertions)]` derlemelerde okunur. Release
/// binary'de bu kacis yolu **derlenmis kodda yoktur** — kullanicinin gercek
/// hafizasi env degiskeniyle baska bir dosyaya yonlendirilemez.
#[cfg(debug_assertions)]
pub const ENV_DB_PATH_OVERRIDE: &str = "ASUNA_DB_PATH";

// ---------------------------------------------------------------------------
// Hata tipi
// ---------------------------------------------------------------------------

/// Veritabani katmani hatasi.
///
/// GUVENLIK/GIZLILIK: hicbir varyantin `Display` metni dosya yolu, SQL sorgusu
/// ya da kullanici icerigi tasimaz — bu metin IPC ile renderer'a ve log'a
/// gidebilir (`conventions.md` "Hata Yonetimi"). Ayrinti `#[source]`
/// zincirinde durur ve yalnizca yerel log'a yazilir.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("uygulama veri dizini cozulemedi")]
    DataDir(#[source] tauri::Error),

    #[error("veritabani dizini olusturulamadi")]
    CreateDirectory(#[source] std::io::Error),

    #[error("veritabani dosyasi acilamadi")]
    Open(#[source] rusqlite::Error),

    #[error("acilis PRAGMA'lari uygulanamadi")]
    Pragma(#[source] rusqlite::Error),

    /// WAL istendi ama SQLite baska bir journal modunda kaldi. Sessizce kabul
    /// edilmez: WAL olmadan es zamanli okuma/yazma davranisi degisir.
    #[error("journal modu WAL'a alinamadi")]
    JournalMode { actual: String },

    #[error("sema migration'lari uygulanamadi")]
    Migration(#[source] rusqlite_migration::Error),

    #[error("veritabani sorgusu basarisiz")]
    Query(#[source] rusqlite::Error),

    /// Baglanti kilidini tutan bir thread panic'ledi. Baglanti artik guvenilir
    /// degil; hafizasiz moda dusmek dogru davranis.
    #[error("veritabani baglantisi kullanilamaz durumda")]
    Poisoned,
}

/// Hata zincirinin tamami — **yalnizca yerel log icin**, IPC'ye gitmez.
pub fn describe_error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        message.push_str(" <- ");
        message.push_str(&inner.to_string());
        source = inner.source();
    }
    message
}

// ---------------------------------------------------------------------------
// Baglanti
// ---------------------------------------------------------------------------

/// Acik DB'nin fiziksel konumu. Testte gecici dosya / bellek, uretimde
/// uygulama veri dizini.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbLocation {
    File(PathBuf),
    /// `:memory:` — yalnizca test. WAL desteklenmez, bu beklenen bir farktir.
    Memory,
}

/// Asuna'nin SQLite baglantisi.
///
/// Tek baglanti bir `Mutex` arkasinda tutulur: yerel, tek kullanicili bir
/// uygulamada connection pool'un getirdigi karmasiklik (hangi invoke hangi
/// baglantiya duser → transaction'in bolunmesi, ADR-005 A/5) kazancindan
/// buyuk. `busy_timeout` ile birlikte WAL checkpoint'leri de guvenli.
pub struct AsunaDb {
    connection: Mutex<Connection>,
    location: DbLocation,
}

impl std::fmt::Debug for AsunaDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `Connection` `Debug` uretmiyor; ayrica dosya yolunu log'a basmiyoruz.
        f.debug_struct("AsunaDb")
            .field(
                "location",
                &match &self.location {
                    DbLocation::File(_) => "file",
                    DbLocation::Memory => "memory",
                },
            )
            .finish()
    }
}

impl AsunaDb {
    /// Uretim yolu: DB'yi uygulama veri dizininde acar.
    ///
    /// Yol [`resolve_db_path`] ile turetilir — cagiran taraf yol veremez.
    pub fn open<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<Self, DbError> {
        let path = resolve_db_path(app)?;
        Self::open_at(&path)
    }

    /// Verilen dosya yolunda acar. Uretimde yalnizca [`AsunaDb::open`]
    /// tarafindan cagrilir; ayrica testlerde gecici dizin icin kullanilir.
    pub fn open_at(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(DbError::CreateDirectory)?;
        }
        let connection = Connection::open(path).map_err(DbError::Open)?;
        let db = Self::bootstrap(connection, DbLocation::File(path.to_path_buf()))?;
        // Migration'lardan **sonra**: WAL kardes dosyalari (`-wal`, `-shm`) ilk
        // yazmayla olusur, once cagirmak onlari kacirirdi.
        restrict_db_permissions(path);
        Ok(db)
    }

    /// Bellek ici DB — birim testleri icin. Diske hicbir sey yazmaz.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let connection = Connection::open_in_memory().map_err(DbError::Open)?;
        Self::bootstrap(connection, DbLocation::Memory)
    }

    fn bootstrap(mut connection: Connection, location: DbLocation) -> Result<Self, DbError> {
        apply_pragmas(&connection, &location)?;
        migrations::apply(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
            location,
        })
    }

    pub fn location(&self) -> &DbLocation {
        &self.location
    }

    /// Baglantiya kilitli erisim.
    ///
    /// `&mut Connection` veriliyor cunku `rusqlite::Transaction` bunu sart
    /// kosar: `memories` + `tool_events` gibi cok tabloli yazimlar tek
    /// transaction'da yapilabilsin (ADR-005 B/5). Ham SQL bu modulun disina
    /// cikmaz — repository katmani (ASU-031) da bu kapiyi kullanir.
    pub fn with_connection<T>(
        &self,
        action: impl FnOnce(&mut Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, DbError> {
        let mut guard = self.connection.lock().map_err(|_| DbError::Poisoned)?;
        action(&mut guard).map_err(DbError::Query)
    }

    /// Uygulanmis sema surumu (`PRAGMA user_version`).
    pub fn schema_version(&self) -> Result<u32, DbError> {
        let raw: i64 = self
            .with_connection(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))?;
        Ok(u32::try_from(raw).unwrap_or(0))
    }
}

/// DB dosyasini ve WAL kardeslerini `0600`'e ceker (Gate 3 / LOW-8).
///
/// # Neden gerekli
///
/// SQLite dosyayi `0666 & ~umask` ile acar; tipik `umask 022` ile sonuc `0644`
/// — yani ayni makinedeki **baska bir kullanici** hafizayi okuyabilir. Uygulama
/// veri dizini macOS'ta genelde daralticidir ama buna guvenmek bir varsayimdir;
/// hafiza kullanicinin en mahrem verisi (`asuna-config/security.md` Bolum 5).
///
/// # Neden hata dondurmuyor
///
/// Izin sikilastirilamamasi hafizayi **acilmaz** kilmaz. Acilisi dusurmek
/// (PROJECT.md Bolum 30: "bozulan alt sistem tum urunu dusurmez") yanlis
/// takas olurdu; sessizce de gecistirilmez — durum yerel log'a yazilir.
fn restrict_db_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for suffix in ["", "-wal", "-shm"] {
            let mut sibling = path.as_os_str().to_owned();
            sibling.push(suffix);
            let sibling = PathBuf::from(sibling);

            let Ok(metadata) = std::fs::metadata(&sibling) else {
                continue; // Dosya yok (WAL kardesleri her zaman olusmaz).
            };
            if metadata.permissions().mode() & 0o777 == 0o600 {
                continue;
            }
            if let Err(error) =
                std::fs::set_permissions(&sibling, std::fs::Permissions::from_mode(0o600))
            {
                // Yol log'a girmiyor: kullanicinin dizin yapisi sizmasin.
                eprintln!("[asuna] Veritabani dosya izinleri sikilastirilamadi: {error}");
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Acilis PRAGMA'lari — sira ADR-005'te sabit.
fn apply_pragmas(connection: &Connection, location: &DbLocation) -> Result<(), DbError> {
    // `journal_mode` deger donduren bir PRAGMA; `pragma_update` ile ayarlanamaz.
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(DbError::Pragma)?;

    // Bellek ici DB WAL'i desteklemez ve `memory` doner — test yolunda beklenen.
    // Dosya tabanli DB'de WAL'a gecilemediyse bu sessizce gecistirilmez.
    if matches!(location, DbLocation::File(_)) && !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(DbError::JournalMode {
            actual: journal_mode,
        });
    }

    // FK'ler SQLite'ta varsayilan olarak KAPALI. `memories.source_session_id`
    // gibi referanslarin gercekten korunmasi icin her baglantida acilir.
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(DbError::Pragma)?;

    // WAL + NORMAL: her commit'te fsync yok, ama checkpoint'te dayaniklilik var.
    // Yerel bir companion icin dogru denge (guc kesintisinde son islem
    // kaybedilebilir, DB bozulmaz).
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(DbError::Pragma)?;

    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(DbError::Pragma)?;

    Ok(())
}

/// DB dosyasinin yolunu **Rust tarafinda** cozer.
///
/// GUVENLIK: bu fonksiyonun imzasinda renderer'dan gelebilecek hicbir girdi
/// yok. Yol her zaman `app_data_dir()` altindadir; `app_cache_dir()` bilerek
/// kullanilmaz (OS tarafindan silinebilir — hafiza oraya konmaz).
pub fn resolve_db_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, DbError> {
    #[cfg(debug_assertions)]
    if let Some(path) = dev_path_override() {
        return Ok(path);
    }

    let dir = app.path().app_data_dir().map_err(DbError::DataDir)?;
    Ok(dir.join(DB_FILE_NAME))
}

/// `ASUNA_DB_PATH` — yalnizca gelistirme build'lerinde.
#[cfg(debug_assertions)]
fn dev_path_override() -> Option<PathBuf> {
    parse_dev_override(std::env::var(ENV_DB_PATH_OVERRIDE).ok().as_deref())
}

/// Override degerinin saf ayristirmasi (process environment'a dokunmaz).
/// Bos/whitespace deger "ayarlanmamis" sayilir — aksi halde `ASUNA_DB_PATH=`
/// satiri DB'yi calisma dizinindeki adsiz bir dosyaya yonlendirirdi.
#[cfg(debug_assertions)]
fn parse_dev_override(raw: Option<&str>) -> Option<PathBuf> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test icin izole bir gecici dizin. `std::env::temp_dir()` altinda,
    /// **gercek uygulama veri dizinine asla dokunmaz**.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "asuna-db-test-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let path = std::env::temp_dir().join(unique);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("gecici dizin olusturulabilmeli");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn opens_an_in_memory_database_without_touching_the_filesystem() {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB acilmali");
        assert_eq!(db.location(), &DbLocation::Memory);
        assert_eq!(
            db.schema_version().expect("sema surumu okunmali"),
            EXPECTED_SCHEMA_VERSION
        );
    }

    /// Acilis, dosya yoksa dizini ve dosyayi olusturur (ADR-005 acilis sirasi).
    #[test]
    fn creates_the_database_file_and_missing_parent_directories() {
        let temp = TempDir::new("create");
        let path = temp.join("nested").join(DB_FILE_NAME);
        assert!(!path.exists());

        let db = AsunaDb::open_at(&path).expect("DB acilmali");

        assert!(path.exists(), "DB dosyasi olusturulmali");
        assert_eq!(db.location(), &DbLocation::File(path));
    }

    /// **Gate 3 / LOW-8**: DB dosyasi ve WAL kardesleri yalnizca sahibi
    /// tarafindan okunabilir. SQLite varsayilani `0644`'tur (umask'a bagli) —
    /// ayni makinedeki baska bir kullanici hafizayi okuyamamali.
    #[cfg(unix)]
    #[test]
    fn the_database_file_is_only_readable_by_the_owner() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new("perms");
        let path = temp.join(DB_FILE_NAME);
        // Migration'lar zaten yazma yapti: `-wal`/`-shm` kardesleri olusmus olmali.
        let _db = AsunaDb::open_at(&path).expect("DB acilmali");

        for suffix in ["", "-wal", "-shm"] {
            let mut sibling = path.clone().into_os_string();
            sibling.push(suffix);
            let sibling = PathBuf::from(sibling);
            let Ok(metadata) = std::fs::metadata(&sibling) else {
                continue;
            };
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{}: {mode:o}", sibling.display());
        }
    }

    /// Dosya tabanli DB WAL modunda acilir — kardes `-wal` dosyasi bunun kaniti.
    #[test]
    fn file_database_uses_wal_journal_mode() {
        let temp = TempDir::new("wal");
        let path = temp.join(DB_FILE_NAME);
        let db = AsunaDb::open_at(&path).expect("DB acilmali");

        let mode: String = db
            .with_connection(|conn| conn.query_row("PRAGMA journal_mode", [], |row| row.get(0)))
            .expect("journal_mode okunmali");
        assert_eq!(mode.to_ascii_lowercase(), "wal");
    }

    /// FK zorlamasi acik olmadan `source_session_id` gibi referanslar sessizce
    /// bozulur; SQLite varsayilani KAPALI oldugu icin bu test bir regresyon kapisi.
    #[test]
    fn foreign_key_enforcement_is_enabled() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");
        let enabled: i64 = db
            .with_connection(|conn| conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0)))
            .expect("foreign_keys okunmali");
        assert_eq!(enabled, 1);
    }

    #[test]
    fn synchronous_is_normal() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");
        // 1 == NORMAL
        let synchronous: i64 = db
            .with_connection(|conn| conn.query_row("PRAGMA synchronous", [], |row| row.get(0)))
            .expect("synchronous okunmali");
        assert_eq!(synchronous, 1);
    }

    /// Migration'lar idempotent: ayni dosya art arda acildiginda sema surumu
    /// degismez ve veri korunur (ADR-005 B/6).
    #[test]
    fn reopening_the_same_file_is_idempotent() {
        let temp = TempDir::new("idempotent");
        let path = temp.join(DB_FILE_NAME);

        let first = AsunaDb::open_at(&path).expect("ilk acilis");
        let version = first.schema_version().expect("sema surumu");
        drop(first);

        for _ in 0..3 {
            let db = AsunaDb::open_at(&path).expect("tekrar acilis");
            assert_eq!(db.schema_version().expect("sema surumu"), version);
        }
    }

    /// Bozuk/erisilemez bir yol `Result` doner — panic yok (ADR-005 A/8'de
    /// olculen `tauri-plugin-sql` panic yuzeyinin tam tersi davranis).
    #[test]
    fn returns_an_error_instead_of_panicking_on_an_unusable_path() {
        let temp = TempDir::new("unusable");
        // Dosyayi dizin gibi kullanmaya calis: parent bir dosya.
        let blocker = temp.join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("dosya yazilabilmeli");

        let error = AsunaDb::open_at(&blocker.join(DB_FILE_NAME)).expect_err("hata bekleniyordu");
        assert!(matches!(
            error,
            DbError::CreateDirectory(_) | DbError::Open(_)
        ));
    }

    /// Hata mesajlari IPC'ye ve log'a gidebiliyor — dosya yolu tasimamali.
    #[test]
    fn error_messages_do_not_leak_the_database_path() {
        let temp = TempDir::new("no-leak");
        let blocker = temp.join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("dosya yazilabilmeli");

        let error = AsunaDb::open_at(&blocker.join(DB_FILE_NAME)).expect_err("hata bekleniyordu");
        let message = error.to_string();
        assert!(!message.contains("blocker"), "mesaj: {message}");
        assert!(!message.contains(DB_FILE_NAME), "mesaj: {message}");
    }

    #[test]
    fn error_chain_description_includes_the_source() {
        let error = DbError::CreateDirectory(std::io::Error::other("disk dolu"));
        let described = describe_error_chain(&error);
        assert!(described.contains("veritabani dizini olusturulamadi"));
        assert!(described.contains("disk dolu"));
    }

    /// `AsunaDb` log'a/panic mesajina basilabilir — `Debug` yol sizdirmamali.
    #[test]
    fn debug_output_does_not_leak_the_database_path() {
        let temp = TempDir::new("debug");
        let path = temp.join(DB_FILE_NAME);
        let db = AsunaDb::open_at(&path).expect("DB acilmali");

        let debug = format!("{db:?}");
        assert!(
            !debug.contains(&path.display().to_string()),
            "debug: {debug}"
        );
    }

    /// Dev override yalnizca anlamli bir deger icin devreye girer; bos deger
    /// sessizce calisma dizininde bir DB acmaz.
    #[cfg(debug_assertions)]
    #[test]
    fn dev_override_is_only_used_for_a_non_blank_value() {
        assert_eq!(parse_dev_override(None), None);
        assert_eq!(parse_dev_override(Some("")), None);
        assert_eq!(parse_dev_override(Some("   ")), None);
        assert_eq!(
            parse_dev_override(Some("  /tmp/asuna-dev.db  ")),
            Some(PathBuf::from("/tmp/asuna-dev.db"))
        );
    }
}

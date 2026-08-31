//! `ProjectRegistry` — kayitli proje koklerinin **tek kaynagi** (ASU-040).
//!
//! # Sozlesme
//!
//! - Asuna yalnizca kullanicinin **acikca kaydettigi** kokleri gorur. Otomatik
//!   disk taramasi yoktur ve eklenmeyecektir (PROJECT.md Bolum 4).
//! - Kaydedilen yol her zaman [`std::fs::canonicalize`] ile normalize edilir:
//!   `..` cozulur, symlink'ler izlenir, sonuc mutlak bir dizindir. Var olmayan
//!   bir yol **kaydedilemez**.
//! - `~` genisletme **yoktur**. Yolu secen taraf dizin secicidir (ASU-045) ve
//!   secici zaten mutlak yol verir; kabuk sozdizimini burada yorumlamak, hangi
//!   home dizininin kastedildigi sorusunu Asuna'ya tahmin ettirirdi.
//! - Kayitli bir kok sonradan kaybolursa satir **silinmez**, `missing` olarak
//!   isaretlenir: harici disk takili olmayabilir, kullanicinin hafizasi buna
//!   kurban gitmemeli.
//!
//! # ASU-049 icin: bu modul sandbox'in tek kaynagidir
//!
//! Phase 5'te gelen path sandbox'i (`asuna-config/security.md` Bolum 2: "Her
//! dosya tool'u **kayitli proje root'u** alir") kok listesini **buradan** alir,
//! baska hicbir yerden. Somut kural:
//!
//! - Sandbox yalnizca `path`i dolu kayitlari gorur. `unlinked` bir satir bir
//!   **etiket**tir (ASU-039), bir yetki degil — yolu yoktur, dolayisiyla hicbir
//!   dizini acamaz.
//! - Kok karsilastirmasi `canonicalize` edilmis yollar uzerinde yapilir; hem
//!   kayit sirasinda (burada) hem erisim sirasinda (ASU-049). `startsWith` ile
//!   ham metin karsilastirmasi yeterli degildir — bir kok icindeki symlink kok
//!   disini gosterebilir.
//! - Yeni bir kok eklemenin **tek** yolu [`add`]'dir ve o da kullanicinin acik
//!   eylemine baglidir. Bir tool kendi kokunu ekleyemez.

use std::path::{Path, PathBuf};

use rusqlite::types::Value;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::model::{ProjectRecord, ProjectStatus};
use crate::db::project_repository as repository;
use crate::db::{clock, AsunaDb, DbError, DbState};
use crate::security::blocklist;

/// Proje adinin ust siniri. Ad UI'da ve ses oturumunda gecer; bir dizin adinin
/// makul uzunlugunu asani kabul etmek baglami sisirir.
const MAX_NAME_CHARS: usize = 120;

/// Aciklamanin ust siniri (PROJECT.md Bolum 15: "ozetle, dumpleme").
const MAX_DESCRIPTION_CHARS: usize = 500;

/// Yol uzunlugu tavani. macOS `PATH_MAX` 1024; tavan ondan yuksek tutulup
/// yalnizca acikca sacma degerleri keser.
const MAX_PATH_CHARS: usize = 4096;

/// Slug catismasinda denenecek en fazla son ek (`-2` ... `-50`).
const MAX_SLUG_ATTEMPTS: u32 = 50;

/// Slug uretilemeyen bir dizin adi icin son care.
const FALLBACK_SLUG: &str = "proje";

/// Proje koku olarak **kabul edilmeyen** sistem agaclari — **on ek** eslesmesi
/// (ASU-069 / Gate 3 C1).
///
/// # Neden bir liste var
///
/// ASU-069'dan once kok kaydinin tek musterisi UI'daki dizin secicisiydi:
/// kullanici bir pencerede tikliyordu. Artik `register_project` tool'u ile
/// **model** de bir yol onerebiliyor ve kayitli kok = Asuna'nin okuyabildigi
/// alan demek. Yani her kayit sandbox'in yuzeyini genisletiyor; "var olan bir
/// dizin" olmak yeterli bir olcut degil.
///
/// # Neden bunlar **on ek**
///
/// Bu agaclarin **hicbir** alt dizini kullanicinin projesi degil. Ilk turda
/// tam eslesme yazilmisti ve Gate 3 bunun bir bypass oldugunu gosterdi:
/// `/System/Volumes/Data/...` (macOS firmlink) tam eslesmeye takilmadigi icin
/// tum kullanici agaci ikinci bir kanonik yoldan aciliyordu. `/System` on ek
/// olunca o kapi kapanıyor.
///
/// `/Volumes` bilerek **burada degil**: harici disktki bir proje
/// (`/Volumes/Yedek/isler/proje`) mesru bir kok. Yalnizca dizinin **kendisi**
/// reddediliyor ([`REFUSED_SYSTEM_DIRECTORIES`]).
const REFUSED_SYSTEM_SUBTREES: [&str; 4] = ["/System", "/Library", "/Applications", "/Network"];

/// Proje koku olamayan tekil dizinler — **tam** eslesme.
///
/// Bunlarin alt agaci acik kalmali: `/usr/local/src/deneme` mesru bir proje
/// olabilir, `/Volumes/Disk/proje` de. En onemlisi `/private/var/folders/...`:
/// macOS'ta gecici dizinler orada yasiyor ve bu repo'nun testlerinin tamami
/// oraya gercek proje kokleri kaydediyor. `/private`i on ek yapmak testleri
/// degil, **gercek kullanimi** de kirardi.
///
/// Listede hem kisayol hem `canonicalize` sonrasi hali var (`/etc` ve
/// `/private/etc`): kullanicidan hangisinin gelecegi bilinmez.
const REFUSED_SYSTEM_DIRECTORIES: [&str; 14] = [
    "/Volumes",
    "/private",
    "/etc",
    "/private/etc",
    "/var",
    "/private/var",
    "/tmp",
    "/private/tmp",
    "/usr",
    "/bin",
    "/sbin",
    "/opt",
    "/Users",
    "/home",
];

/// macOS firmlink oneki: **ayni** dizin iki kanonik yoldan gorunebilir
/// (`/Users/ad` ve `/System/Volumes/Data/Users/ad`).
///
/// Gate 3 C1'in ikinci yarisi buydu: `~/Library` korumasi `home.join("Library")`
/// on ekine bakiyordu ve firmlink yolundan gelen bir istek o oneki tasimadigi
/// icin geciyordu. Karsilastirmadan **once** soyuluyor.
const DATA_VOLUME_PREFIX: &str = "/System/Volumes/Data";

// ---------------------------------------------------------------------------
// Hata
// ---------------------------------------------------------------------------

/// Renderer'in ayirt etmesi gereken hata sinifi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegistryErrorCode {
    /// Girdi dogrulamadan gecmedi (bos ad, cok uzun aciklama...).
    Invalid,
    /// Yol kabul edilmedi: mutlak degil, `~` iceriyor, filesystem koku ya da
    /// UTF-8 disi.
    PathRefused,
    /// Yol diskte yok ya da erisilemiyor.
    PathNotFound,
    /// Yol var ama dizin degil.
    NotADirectory,
    /// Verilen id ile kayitli proje yok.
    NotFound,
    /// Islem bu proje durumunda anlamsiz (orn. yolu olmayan bir etiketi
    /// "guncel proje" yapmak).
    Refused,
    /// Kalici depolama kapali (`ASUNA_MEMORY_ENABLED=false`) — kayit tutulamaz.
    Disabled,
    /// Hafiza alt sistemi arizali.
    Unavailable,
    /// DB islemi basarisiz.
    Storage,
}

/// Registry hatasi.
///
/// GUVENLIK/GIZLILIK: hicbir varyantin mesaji **yol** tasimaz. Yolu zaten
/// cagiran taraf biliyor; mesajin log'a ve UI'a dusen kopyasinda kullanicinin
/// dizin yapisini tekrarlamanin bir kazanci yok (`conventions.md` "Hata
/// Yonetimi", `db::DbError` ile ayni kural).
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{detail}")]
    Invalid { detail: String },

    #[error("yol kabul edilmedi: {detail}")]
    PathRefused { detail: String },

    #[error("verilen yol bulunamadi ya da erisilemiyor")]
    PathNotFound,

    #[error("verilen yol bir dizin degil")]
    NotADirectory,

    #[error("proje kaydi bulunamadi")]
    NotFound,

    #[error("{detail}")]
    Refused { detail: String },

    #[error("kalici depolama kapali; proje kaydi tutulamiyor")]
    Disabled,

    #[error("hafiza kullanilamiyor: {reason}")]
    Unavailable { reason: String },

    #[error("veritabani islemi basarisiz")]
    Storage(#[source] DbError),
}

impl RegistryError {
    fn invalid(detail: impl Into<String>) -> Self {
        Self::Invalid {
            detail: detail.into(),
        }
    }

    fn path_refused(detail: impl Into<String>) -> Self {
        Self::PathRefused {
            detail: detail.into(),
        }
    }

    fn refused(detail: impl Into<String>) -> Self {
        Self::Refused {
            detail: detail.into(),
        }
    }

    fn storage(error: DbError, operation: &'static str) -> Self {
        eprintln!(
            "[asuna] `{operation}` basarisiz: {}",
            crate::db::describe_error_chain(&error)
        );
        Self::Storage(error)
    }

    pub fn code(&self) -> RegistryErrorCode {
        match self {
            Self::Invalid { .. } => RegistryErrorCode::Invalid,
            Self::PathRefused { .. } => RegistryErrorCode::PathRefused,
            Self::PathNotFound => RegistryErrorCode::PathNotFound,
            Self::NotADirectory => RegistryErrorCode::NotADirectory,
            Self::NotFound => RegistryErrorCode::NotFound,
            Self::Refused { .. } => RegistryErrorCode::Refused,
            Self::Disabled => RegistryErrorCode::Disabled,
            Self::Unavailable { .. } => RegistryErrorCode::Unavailable,
            Self::Storage(_) => RegistryErrorCode::Storage,
        }
    }
}

impl Serialize for RegistryError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            code: RegistryErrorCode,
            message: &'a str,
        }

        let message = self.to_string();
        Wire {
            code: self.code(),
            message: &message,
        }
        .serialize(serializer)
    }
}

// ---------------------------------------------------------------------------
// Yol normalizasyonu
// ---------------------------------------------------------------------------

/// Kayda uygun, normalize edilmis bir kok dizin yolu.
///
/// Bu tipi uretmenin tek yolu [`RegisteredRoot::resolve`]'dir; yani "dogrulanmis
/// yol" ile "renderer'dan gelen metin" tip duzeyinde ayrilir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredRoot {
    path: PathBuf,
    text: String,
}

/// Kullanicinin ev dizini — `canonicalize` edilmis.
///
/// `None` = `HOME` tanimli degil ya da cozulemedi. O durumda ev dizini tabanli
/// kurallar **atlanir**; uydurulmus bir home yolu ile karsilastirma yapmak,
/// olmayan bir korumayi var gibi gostermek olurdu. Kalan kurallar (sistem
/// dizinleri, blok listesi) yine kosar.
fn home_directory() -> Option<PathBuf> {
    let raw = std::env::var_os("HOME")?;
    if raw.is_empty() {
        return None;
    }
    std::fs::canonicalize(raw).ok()
}

/// Bu dizin bir proje koku olabilir mi? (ASU-069)
///
/// Kayitli kok = Asuna'nin okuyabildigi alan. `register_project` tool'undan
/// beri bu alani genisletmeyi **model** de onerebiliyor, dolayisiyla "var olan
/// bir dizin" olmak yeterli degil. Dort ret:
///
/// 1. **Filesystem koku** (`/`) — bir sandbox koku olarak "her sey" demektir.
/// 2. **Sistem agaclari ve dizinleri** ([`REFUSED_SYSTEM_SUBTREES`] on ek,
///    [`REFUSED_SYSTEM_DIRECTORIES`] tam eslesme) — kullanicinin projesi degil.
/// 3. **Ev dizini, onu iceren her ust dizin ve `~/Library`** — ilk ikisi tum
///    kullanici verisini tek kayitla acardi (`/Users`, `/`,
///    `/System/Volumes/Data`); ucuncusu uygulama destek dosyalari, tarayici
///    profilleri ve token'lar demek (`~/Library/Application Support/gh`).
///    Hicbiri "proje" degil.
/// 4. **Blok listesindeki yollar** (`~/.ssh`, `~/.aws`, `.../secrets` ...) —
///    [`crate::security::sandbox`] boyle bir kok altindaki dosyalari zaten
///    okumuyordu; kaydi bastan reddetmek ayni karari **gorunur** kiliyor
///    (kullanici "ekledim ama calismiyor" ile bas basa kalmasin).
///
/// Butun karsilastirmalar **firmlink normalizasyonundan sonra** yapilir
/// ([`strip_data_volume`]): macOS'ta ayni dizin iki kanonik yoldan gorunur ve
/// kurallardan yalnizca birinin o yolu gormesi bir bypass demektir.
///
/// `home` disaridan veriliyor ki kural ortam degiskenine dokunmadan test
/// edilebilsin.
fn refuse_unsuitable_root(canonical: &Path, home: Option<&Path>) -> Result<(), RegistryError> {
    // Firmlink normalizasyonu **once**: butun karsilastirmalar tek bir kanonik
    // bicim uzerinde yapilsin. Aksi halde her kural iki kez yazilmak zorunda
    // kalir ve biri unutuldugunda sessizce delinir (Gate 3 C1).
    let candidate = strip_data_volume(canonical);
    let home = home.map(strip_data_volume);

    if candidate.parent().is_none() {
        return Err(RegistryError::path_refused(
            "filesystem koku proje olarak kaydedilemez",
        ));
    }

    if REFUSED_SYSTEM_SUBTREES
        .iter()
        .any(|refused| candidate.starts_with(refused))
    {
        return Err(RegistryError::path_refused(
            "sistem dizinleri proje olarak kaydedilemez",
        ));
    }

    if REFUSED_SYSTEM_DIRECTORIES
        .iter()
        .any(|refused| Path::new(refused) == candidate)
    {
        return Err(RegistryError::path_refused(
            "sistem dizinleri proje olarak kaydedilemez",
        ));
    }

    if let Some(home) = home.as_deref() {
        if candidate == home {
            return Err(RegistryError::path_refused(
                "ev dizininin kendisi proje olarak kaydedilemez; \
                 bir alt dizin secin",
            ));
        }
        // **Ata reddi** (Gate 3 C1): kok, ev dizinini *iceriyorsa* reddedilir.
        // `/Users` ya da `/System/Volumes/Data` gibi bir yol "ev dizininin
        // kendisi" degildir ama ondan daha genistir — tek kayitla tum
        // kullanici verisini acar. Tam eslesme bu ailenin yalnizca bir uyesini
        // yakaliyordu; `starts_with` tersi yonde bakarak hepsini kapatir.
        if home.starts_with(&candidate) {
            return Err(RegistryError::path_refused(
                "ev dizinini iceren bir ust dizin proje olarak kaydedilemez; \
                 projenin kendi dizinini secin",
            ));
        }
        // On ek: `~/Library` altindaki her sey (Application Support, Keychains,
        // tarayici profilleri, `~/Library/Application Support/gh`) kapali.
        if candidate.starts_with(home.join("Library")) {
            return Err(RegistryError::path_refused(
                "`~/Library` ve altindaki dizinler proje olarak kaydedilemez",
            ));
        }
    }

    if let Some(reason) = blocklist::is_blocked_resolved(&candidate) {
        return Err(RegistryError::path_refused(reason.describe()));
    }

    Ok(())
}

/// macOS firmlink onekini soyar: `/System/Volumes/Data/Users/ad` → `/Users/ad`.
///
/// Onek yoksa yol **oldugu gibi** doner. `/System/Volumes/Data`in kendisi `/`
/// olur ve filesystem koku kuralina takilir — dogru sonuc, cunku o yol gercekten
/// tum veri biriminin kokudur.
fn strip_data_volume(path: &Path) -> PathBuf {
    match path.strip_prefix(DATA_VOLUME_PREFIX) {
        Ok(rest) => Path::new("/").join(rest),
        Err(_) => path.to_path_buf(),
    }
}

impl RegisteredRoot {
    /// Ham metni normalize eder.
    ///
    /// Sirasiyla: bos/uzunluk kontrolu → `~` reddi → mutlak olma kontrolu →
    /// `canonicalize` (symlink + `..` cozumu, var olma kontrolu) → dizin olma
    /// kontrolu → **uygunsuz kok reddi** ([`refuse_unsuitable_root`]: filesystem
    /// koku, sistem dizinleri, ev dizini/`~/Library`, blok listesi) → UTF-8
    /// kontrolu.
    ///
    /// `canonicalize` **once** cagrilmaz: var olmayan bir yolda hata verir ve
    /// "mutlak degil" ile "yok" ayrimini kaybederdik.
    pub fn resolve(raw: &str) -> Result<Self, RegistryError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(RegistryError::path_refused("bos"));
        }
        if trimmed.chars().count() > MAX_PATH_CHARS {
            return Err(RegistryError::path_refused("cok uzun"));
        }
        // `~` kabuk sozdizimidir, yol degil. Genisletmek hangi home dizininin
        // kastedildigini tahmin etmek olurdu.
        if trimmed.starts_with('~') {
            return Err(RegistryError::path_refused(
                "`~` genisletilmez, tam yol verilmeli",
            ));
        }
        if trimmed.contains('\0') {
            return Err(RegistryError::path_refused("gecersiz karakter iceriyor"));
        }

        let candidate = Path::new(trimmed);
        if !candidate.is_absolute() {
            return Err(RegistryError::path_refused("mutlak yol olmali"));
        }

        // Var olma + symlink + `..` cozumu tek adimda.
        let canonical =
            std::fs::canonicalize(candidate).map_err(|_| RegistryError::PathNotFound)?;
        if !canonical.is_dir() {
            return Err(RegistryError::NotADirectory);
        }
        refuse_unsuitable_root(&canonical, home_directory().as_deref())?;

        let text = canonical
            .to_str()
            .ok_or_else(|| RegistryError::path_refused("UTF-8 disi"))?
            .to_owned();

        Ok(Self {
            path: canonical,
            text,
        })
    }

    pub fn as_path(&self) -> &Path {
        &self.path
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Dizin adi — varsayilan proje adi.
    fn directory_name(&self) -> String {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(FALLBACK_SLUG)
            .to_owned()
    }
}

/// Dizin adindan slug uretir: kucuk harf, ASCII harf/rakam ve `-`.
///
/// Turkce karakterler ASCII karsiliklarina indirgenir (`ç` → `c`); slug hem
/// `memories.project_id` degeri hem de kullanicinin sesli olarak duyabilecegi
/// bir kimlik oluyor, okunabilir kalmali.
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut pending_separator = false;

    for character in name.chars() {
        let mapped = match character {
            'ç' | 'Ç' => Some('c'),
            'ğ' | 'Ğ' => Some('g'),
            'ı' | 'I' => Some('i'),
            'İ' | 'i' => Some('i'),
            'ö' | 'Ö' => Some('o'),
            'ş' | 'Ş' => Some('s'),
            'ü' | 'Ü' => Some('u'),
            other if other.is_ascii_alphanumeric() => Some(other.to_ascii_lowercase()),
            _ => None,
        };

        match mapped {
            Some(value) => {
                if pending_separator && !slug.is_empty() {
                    slug.push('-');
                }
                pending_separator = false;
                slug.push(value);
            }
            None => pending_separator = true,
        }
    }

    if slug.is_empty() {
        FALLBACK_SLUG.to_owned()
    } else {
        slug
    }
}

// ---------------------------------------------------------------------------
// Sonuc tipleri
// ---------------------------------------------------------------------------

/// Proje ekleme sonucu.
///
/// Cift kayit bir **hata degil**: kullanici ayni dizini iki kez secmis olabilir.
/// Ama "eklendi" demek de yanlis olurdu — hangisinin oldugu acikca donuyor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ProjectAddOutcome {
    Registered {
        project: ProjectRecord,
    },
    /// Bu yol zaten kayitli. Donen kayit mevcut olandir; yeni satir acilmadi.
    AlreadyRegistered {
        project: ProjectRecord,
    },
}

/// Proje kaydini kaldirma sonucu.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ProjectRemoveOutcome {
    /// Satir gercekten silindi (bagli hafiza/oturum yoktu).
    Deleted { id: String },
    /// Kayit kaldirildi ama satir **etiket olarak** korundu: bu projeye bagli
    /// hafiza var ve etiketi silmek "proje X'te alinan karar" baglamini
    /// kaybettirirdi.
    ///
    /// `Box`: varyantlar arasi boyut farki (`SessionWriteResult` ile ayni
    /// gerekce). Serde acisindan seffaf — telde fark yok.
    Unlinked {
        project: Box<ProjectRecord>,
        references: i64,
    },
}

/// [`update`] icin alan yamasi. Kolon adlari burada **sabit metindir**.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectPatch {
    pub name: Option<String>,
    /// `Some(None)` = aciklamayi temizle.
    pub description: Option<Option<String>>,
    /// Kullanicinin degistirebilecegi tek durum ekseni.
    ///
    /// `missing` ve `unlinked` **makine tarafindan** yonetilir; renderer'in
    /// bunlari yazabilmesi, olmayan bir dizini "aktif" gostermeyi mumkun kilardi.
    pub archived: Option<bool>,
}

/// ASU-041/042'nin yazdigi tespit sonuclari.
///
/// Ayri bir tip: bu alanlar kullanicinin girdigi degil, Asuna'nin **olctugu**
/// degerler. Renderer'a acilan bir komutu yok.
#[derive(Debug, Clone, Default)]
pub struct DetectedMetadata {
    pub primary_language: Option<String>,
    pub framework: Option<String>,
    /// Redakte edilmis remote **adi** — token tasiyan URL asla (ASU-042).
    pub git_remote: Option<String>,
}

// ---------------------------------------------------------------------------
// Registry islemleri
// ---------------------------------------------------------------------------

/// Yeni bir proje koku kaydeder.
///
/// `name` verilmezse dizin adi kullanilir. Kimlik (slug) dizin adindan uretilir;
/// catisirsa once **devralinan etiket sahiplenilir** (ASU-039), o da yoksa
/// `-2`, `-3` ... son eki denenir.
///
/// Kayit "guncel proje" secimini **degistirmez**: guncel proje kullanicinin
/// ayri ve acik bir secimidir ([`set_current`]).
pub fn add(
    db: &AsunaDb,
    raw_path: &str,
    name: Option<&str>,
    now: &str,
) -> Result<ProjectAddOutcome, RegistryError> {
    let root = RegisteredRoot::resolve(raw_path)?;
    let name = match name {
        Some(value) => validated_name(value)?,
        None => validated_name(&root.directory_name())?,
    };

    db.with_connection(|connection| {
        let transaction = connection.transaction()?;

        // Cift kayit: ayni **normalize edilmis** yol iki satir acamaz.
        let existing = transaction
            .query_row(
                &format!(
                    "SELECT {} FROM projects WHERE path = ?1",
                    ProjectRecord::select_columns()
                ),
                rusqlite::params![root.as_str()],
                ProjectRecord::from_row,
            )
            .optional()?;

        if let Some(mut project) = existing {
            // Kayit geri gelmis olabilir (`missing` -> `active`).
            if project.status == ProjectStatus::Missing {
                repository::set_status(&transaction, &project.id, ProjectStatus::Active, now)?;
                project = repository::load(&transaction, &project.id)?.unwrap_or(project);
            }
            transaction.commit()?;
            return Ok(AddAttempt::Done(Box::new(
                ProjectAddOutcome::AlreadyRegistered { project },
            )));
        }

        let base = slugify(&root.directory_name());
        let Some(id) = allocate_id(&transaction, &base, &name, root.as_str(), now)? else {
            // Hicbir sey yazilmadi; transaction dusuruluyor.
            return Ok(AddAttempt::IdExhausted);
        };
        // `INSERT`/`UPDATE` basarili olduysa satir vardir; yoksa sema ile kod
        // kaymis demektir ve bu sessizce gecistirilmez.
        let project =
            repository::load(&transaction, &id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        transaction.commit()?;
        Ok(AddAttempt::Done(Box::new(ProjectAddOutcome::Registered {
            project,
        })))
    })
    .map_err(|error| RegistryError::storage(error, "project_add"))?
    .into_outcome()
}

/// [`add`]'in ic sonucu: kimlik uretilememesi bir DB hatasi degil, bir domain
/// durumu — ama `rusqlite::Error`'a sikistirilamaz.
enum AddAttempt {
    Done(Box<ProjectAddOutcome>),
    /// Ayni adli cok fazla proje var; kimlik **uydurulmaz**.
    IdExhausted,
}

impl AddAttempt {
    fn into_outcome(self) -> Result<ProjectAddOutcome, RegistryError> {
        match self {
            Self::Done(outcome) => Ok(*outcome),
            Self::IdExhausted => Err(RegistryError::invalid(format!(
                "ayni adli {MAX_SLUG_ATTEMPTS} projeden fazlasi kaydedilemez; \
                 projeye farkli bir ad verin"
            ))),
        }
    }
}

/// Slug catismasini cozer ve satiri yazar; kullanilan kimligi doner.
///
/// `None` = tavana ulasildi ve **hicbir sey yazilmadi**.
fn allocate_id(
    connection: &rusqlite::Connection,
    base: &str,
    name: &str,
    path: &str,
    now: &str,
) -> rusqlite::Result<Option<String>> {
    for attempt in 0..=MAX_SLUG_ATTEMPTS {
        let candidate = if attempt == 0 {
            base.to_owned()
        } else {
            format!("{base}-{}", attempt + 1)
        };

        match repository::load(connection, &candidate)? {
            None => {
                repository::insert_registered(
                    connection,
                    &repository::NewProject {
                        id: &candidate,
                        name,
                        path,
                    },
                    now,
                )?;
                return Ok(Some(candidate));
            }
            // Devralinan etiket: satiri **sahiplen**, yenisini acma. Boylece
            // Phase 3'te bu etiketle yazilmis hafizalar dogru projeye baglanir.
            Some(existing) if existing.status == ProjectStatus::Unlinked => {
                let updated = repository::adopt_unlinked(connection, &candidate, name, path, now)?;
                if updated == 1 {
                    return Ok(Some(candidate));
                }
            }
            Some(_) => {}
        }
    }

    Ok(None)
}

/// Kayitli projeler — her cagrida durumlari tazelenmis olarak.
///
/// **Bu bir disk taramasi degildir.** Yalnizca zaten kayitli olan koklerin var
/// olup olmadigi sorulur (`stat`); dizinlerin **icine** girilmez, bilinmeyen
/// hicbir yol ziyaret edilmez.
pub fn list(db: &AsunaDb, now: &str) -> Result<Vec<ProjectRecord>, RegistryError> {
    refresh_statuses(db, now)?;
    repository::list_all(db).map_err(|error| RegistryError::storage(error, "project_list"))
}

/// Kayitli koklerin diskte hala var olup olmadigini isaretler.
///
/// Kaybolan kok `missing`, geri gelen kok `active` olur. `archived` ve
/// `unlinked` satirlara **dokunulmaz**: ilki kullanicinin karari, ikincisinin
/// zaten yolu yok.
pub fn refresh_statuses(db: &AsunaDb, now: &str) -> Result<usize, RegistryError> {
    let projects =
        repository::list_all(db).map_err(|error| RegistryError::storage(error, "project_list"))?;

    let mut changed = 0usize;
    for project in projects {
        let Some(path) = project.path.as_deref() else {
            continue; // `unlinked` — yolu yok.
        };
        let present = Path::new(path).is_dir();
        let next = match (project.status, present) {
            (ProjectStatus::Active, false) => Some(ProjectStatus::Missing),
            (ProjectStatus::Missing, true) => Some(ProjectStatus::Active),
            // Arsivlenmis proje kullanicinin karari; diskteki durumu onu
            // arsivden cikarmaz.
            _ => None,
        };
        let Some(next) = next else { continue };

        db.with_connection(|connection| repository::set_status(connection, &project.id, next, now))
            .map_err(|error| RegistryError::storage(error, "project_refresh"))?;
        changed += 1;
    }

    Ok(changed)
}

/// Alanlari gunceller (ad, aciklama, arsiv bayragi).
pub fn update(
    db: &AsunaDb,
    id: &str,
    patch: &ProjectPatch,
    now: &str,
) -> Result<ProjectRecord, RegistryError> {
    let existing = require_project(db, id)?;

    let mut assignments: Vec<(&'static str, Value)> = Vec::new();
    if let Some(name) = patch.name.as_deref() {
        assignments.push(("name", Value::Text(validated_name(name)?)));
    }
    if let Some(description) = patch.description.as_ref() {
        assignments.push((
            "description",
            match description.as_deref().map(str::trim) {
                None | Some("") => Value::Null,
                Some(text) => {
                    if text.chars().count() > MAX_DESCRIPTION_CHARS {
                        return Err(RegistryError::invalid(format!(
                            "`description` en fazla {MAX_DESCRIPTION_CHARS} karakter olabilir"
                        )));
                    }
                    Value::Text(text.to_owned())
                }
            },
        ));
    }
    if let Some(archived) = patch.archived {
        if !existing.status.has_registered_root() {
            return Err(RegistryError::refused(
                "yolu olmayan bir etiket arsivlenemez",
            ));
        }
        let status = if archived {
            ProjectStatus::Archived
        } else if Path::new(existing.path.as_deref().unwrap_or_default()).is_dir() {
            ProjectStatus::Active
        } else {
            ProjectStatus::Missing
        };
        assignments.push(("status", Value::Text(status.as_str().to_owned())));
    }

    db.with_connection(|connection| {
        repository::apply_patch(connection, id, &assignments, now)?;
        repository::load(connection, id)
    })
    .map_err(|error| RegistryError::storage(error, "project_update"))?
    .ok_or(RegistryError::NotFound)
}

/// Projeyi kayittan cikarir.
///
/// Bagli hafiza/oturum varsa satir **silinmez**, etikete dusurulur: kullanici
/// kaydi kaldirdiginda hafizasini kaybetmemeli (bkz.
/// `project_repository::demote_to_unlinked`).
pub fn remove(db: &AsunaDb, id: &str, now: &str) -> Result<ProjectRemoveOutcome, RegistryError> {
    let existing = require_project(db, id)?;

    db.with_connection(|connection| {
        let transaction = connection.transaction()?;
        let references = repository::reference_count(&transaction, id)?;

        let outcome = if references == 0 {
            repository::delete(&transaction, id)?;
            ProjectRemoveOutcome::Deleted { id: id.to_owned() }
        } else {
            repository::demote_to_unlinked(&transaction, id, now)?;
            let project = repository::load(&transaction, id)?.unwrap_or(existing.clone());
            ProjectRemoveOutcome::Unlinked {
                project: Box::new(project),
                references,
            }
        };

        transaction.commit()?;
        Ok(outcome)
    })
    .map_err(|error| RegistryError::storage(error, "project_remove"))
}

/// "Guncel proje" secimi — **kullanicinin acik eylemi**.
///
/// Yalnizca `last_opened_at` tazelenir; ayri bir "current" bayragi yok. Sebep:
/// iki kaynak (bayrak + zaman damgasi) birbirinden kayabilir ve o an Asuna
/// hangisine inanacagini bilemezdi. Tek eksen var, o da kullanicinin en son
/// acik secimi.
///
/// Kaydi olmayan (`unlinked`) ya da yolu kaybolmus (`missing`) bir proje guncel
/// yapilamaz: Asuna okuyamayacagi bir projeyi "su an buradayiz" diye
/// sunmamali.
pub fn set_current(db: &AsunaDb, id: &str, now: &str) -> Result<ProjectRecord, RegistryError> {
    let existing = require_project(db, id)?;

    if !existing.status.has_registered_root() {
        return Err(RegistryError::refused(
            "bu proje yalnizca bir etiket; once kok dizini kaydedilmeli",
        ));
    }
    let path = existing.path.as_deref().unwrap_or_default();
    if !Path::new(path).is_dir() {
        // Kaydi guncel isaretlemeden once durumu duzelt: kullanici "neden
        // calismiyor?" sorusunun cevabini listede gormeli.
        db.with_connection(|connection| {
            repository::set_status(connection, id, ProjectStatus::Missing, now)
        })
        .map_err(|error| RegistryError::storage(error, "project_set_current"))?;
        return Err(RegistryError::refused(
            "projenin kok dizini bulunamiyor (missing)",
        ));
    }

    db.with_connection(|connection| {
        repository::touch_last_opened(connection, id, now)?;
        repository::load(connection, id)
    })
    .map_err(|error| RegistryError::storage(error, "project_set_current"))?
    .ok_or(RegistryError::NotFound)
}

/// Kullanicinin en son actigi kayitli proje — yoksa `None`.
///
/// **Tahmin yok.** Hicbir proje acilmamissa Asuna "hangi projedeyiz?" sorusuna
/// bilmedigini soyler (ASU-041 `unknown` doner).
pub fn current(db: &AsunaDb) -> Result<Option<ProjectRecord>, RegistryError> {
    repository::most_recently_opened(db)
        .map_err(|error| RegistryError::storage(error, "project_current"))
}

/// ASU-041/042'nin olctugu metadata'yi yazar. Renderer'a acik degil.
pub fn record_detected_metadata(
    db: &AsunaDb,
    id: &str,
    detected: &DetectedMetadata,
    now: &str,
) -> Result<(), RegistryError> {
    let mut assignments: Vec<(&'static str, Value)> = Vec::new();
    for (column, value) in [
        ("primary_language", detected.primary_language.as_deref()),
        ("framework", detected.framework.as_deref()),
        ("git_remote", detected.git_remote.as_deref()),
    ] {
        let Some(value) = value else { continue };
        let trimmed = value.trim();
        assignments.push((
            column,
            if trimmed.is_empty() {
                Value::Null
            } else {
                Value::Text(trimmed.to_owned())
            },
        ));
    }

    if assignments.is_empty() {
        return Ok(());
    }

    db.with_connection(|connection| repository::apply_patch(connection, id, &assignments, now))
        .map_err(|error| RegistryError::storage(error, "project_detect"))?;
    Ok(())
}

fn require_project(db: &AsunaDb, id: &str) -> Result<ProjectRecord, RegistryError> {
    repository::find_by_id(db, id)
        .map_err(|error| RegistryError::storage(error, "project_load"))?
        .ok_or(RegistryError::NotFound)
}

fn validated_name(raw: &str) -> Result<String, RegistryError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(RegistryError::invalid("`name` bos birakilamaz"));
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(RegistryError::invalid(format!(
            "`name` en fazla {MAX_NAME_CHARS} karakter olabilir"
        )));
    }
    Ok(trimmed.to_owned())
}

// ---------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------

/// Komut katmani icin DB erisimi.
///
/// `Disabled` burada **hata**: `memory_create` gibi sessizce atlanamaz. "Proje
/// ekle" dedikten sonra hicbir sey olmamasi ve basari donmesi, kullanicinin
/// kaydettigini sandigi bir projeyle calismasi demek olurdu.
///
/// `pub(crate)`: [`super::view::project_context`] ayni kapiyi kullanir — ikinci
/// bir "DB kapali mi?" yorumu iki farkli davranis uretirdi.
pub(crate) fn database(state: &DbState) -> Result<&AsunaDb, RegistryError> {
    match state.access() {
        Ok(Some(db)) => Ok(db),
        Ok(None) => Err(RegistryError::Disabled),
        Err(reason) => Err(RegistryError::Unavailable {
            reason: reason.to_owned(),
        }),
    }
}

/// Kullanicinin sectigi dizini kaydeder.
///
/// Dizin secici **UI'in isidir** (ASU-045). Komut yalnizca bir metin alir ve o
/// metnin var olan, mutlak, symlink'i cozulmus bir dizin olmasini sart kosar;
/// `~` genisletilmez.
#[tauri::command]
pub fn project_add(
    state: State<'_, DbState>,
    path: String,
    name: Option<String>,
) -> Result<ProjectAddOutcome, RegistryError> {
    let db = database(&state)?;
    add(db, &path, name.as_deref(), &clock::now_utc())
}

/// Kayitli projeleri listeler (durumlari tazelenmis).
#[tauri::command]
pub fn project_list(state: State<'_, DbState>) -> Result<Vec<ProjectRecord>, RegistryError> {
    let db = database(&state)?;
    list(db, &clock::now_utc())
}

/// Projeyi kayittan cikarir. Bagli hafiza varsa etiket korunur.
#[tauri::command]
pub fn project_remove(
    state: State<'_, DbState>,
    project_id: String,
) -> Result<ProjectRemoveOutcome, RegistryError> {
    let db = database(&state)?;
    remove(db, &project_id, &clock::now_utc())
}

/// "Guncel proje" secimi — kullanicinin acik eylemi.
#[tauri::command]
pub fn project_set_current(
    state: State<'_, DbState>,
    project_id: String,
) -> Result<ProjectRecord, RegistryError> {
    let db = database(&state)?;
    set_current(db, &project_id, &clock::now_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-08-25T10:00:00Z";
    const LATER: &str = "2026-08-25T12:00:00Z";

    /// Izole gecici dizin. Gercek uygulama veri dizinine **asla** dokunmaz.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "asuna-registry-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let path = std::env::temp_dir().join(unique);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("gecici dizin");
            Self(path)
        }

        fn child(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("alt dizin");
            path
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB")
    }

    fn text(path: &Path) -> String {
        path.to_str().expect("UTF-8 yol").to_owned()
    }

    fn registered(outcome: ProjectAddOutcome) -> ProjectRecord {
        match outcome {
            ProjectAddOutcome::Registered { project } => project,
            ProjectAddOutcome::AlreadyRegistered { project } => {
                panic!("yeni kayit bekleniyordu, mevcut donduruldu: {}", project.id)
            }
        }
    }

    // --- Normalizasyon ------------------------------------------------------

    #[test]
    fn a_relative_path_is_refused() {
        let error = RegisteredRoot::resolve("gorece/yol").expect_err("reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::PathRefused);
    }

    /// `~` bir kabuk sozdizimi; genisletmek home dizinini **tahmin** etmek olur.
    #[test]
    fn a_tilde_path_is_refused_without_expansion() {
        let error = RegisteredRoot::resolve("~/Work/asuna").expect_err("reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::PathRefused);
        assert!(error.to_string().contains('~'));
    }

    #[test]
    fn the_filesystem_root_cannot_be_registered() {
        let error = RegisteredRoot::resolve("/").expect_err("reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::PathRefused);
    }

    /// **ASU-069 kabul kaniti**: `register_project` tool'u kayitli kok
    /// listesini genisletebiliyor; sistem dizinleri bu yolla kaydedilemez.
    #[test]
    fn system_directories_cannot_be_registered() {
        for path in ["/usr", "/etc", "/private", "/var", "/tmp", "/Applications"] {
            // Yol makinede yoksa test bir sey kanitlamaz; varsa reddedilmeli.
            let Ok(canonical) = std::fs::canonicalize(path) else {
                continue;
            };
            let Err(error) = RegisteredRoot::resolve(&text(&canonical)) else {
                panic!("`{path}` proje olarak kaydedilebildi");
            };
            assert_eq!(error.code(), RegistryErrorCode::PathRefused, "yol `{path}`");
        }
    }

    /// Ev dizininin **kendisi** proje degildir: tek kayitla tum kullanici
    /// verisi Asuna'ya acilirdi.
    #[test]
    fn the_home_directory_itself_cannot_be_registered() {
        let temp = TempDir::new("home");
        let home = std::fs::canonicalize(temp.path()).expect("canonicalize");

        let error = refuse_unsuitable_root(&home, Some(&home)).expect_err("ev dizini reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::PathRefused);
        assert!(error.to_string().contains("ev dizini"), "mesaj: {error}");

        // Ama altindaki bir dizin kaydedilebilir.
        let project = home.join("Work").join("asuna");
        refuse_unsuitable_root(&project, Some(&home)).expect("alt dizin kabul edilmeli");
    }

    /// **Gate 3 C1 (a) regresyonu**: ev dizininin bir seviye **ustu**
    /// (`/Users`) tam eslesmeye takilmiyordu ve tum kullanici agacini tek
    /// kayitla acardi. Ata reddi bu ailenin tamamini kapatir.
    #[test]
    fn an_ancestor_of_the_home_directory_cannot_be_registered() {
        let home = Path::new("/tmp/x/home");

        for ancestor in ["/tmp/x", "/tmp", "/"] {
            let Err(error) = refuse_unsuitable_root(Path::new(ancestor), Some(home)) else {
                panic!("`{ancestor}` ev dizinini iceriyor, reddedilmeliydi");
            };
            assert_eq!(error.code(), RegistryErrorCode::PathRefused, "`{ancestor}`");
        }

        // Kardes ve alt dizinler etkilenmiyor.
        refuse_unsuitable_root(Path::new("/tmp/x/home/Work"), Some(home))
            .expect("alt dizin kabul edilmeli");
        refuse_unsuitable_root(Path::new("/tmp/y/baska"), Some(home))
            .expect("kardes dizin kabul edilmeli");
    }

    /// **Gate 3 C1 (b) regresyonu**: macOS firmlink'i
    /// (`/System/Volumes/Data/...`) ayni dizini ikinci bir kanonik yoldan
    /// gosterir. `~/Library` korumasi o yolda on eki tutturamiyordu.
    #[test]
    fn the_data_volume_firmlink_cannot_be_used_to_bypass_the_home_rules() {
        let home = Path::new("/tmp/x/home");

        for path in [
            "/System/Volumes/Data/tmp/x/home/Library",
            "/System/Volumes/Data/tmp/x/home/Library/Application Support/gh",
            "/System/Volumes/Data/tmp/x/home",
            "/System/Volumes/Data/tmp/x",
            "/System/Volumes/Data",
            "/System/Volumes/Data/tmp/x/home/.ssh",
        ] {
            let Err(error) = refuse_unsuitable_root(Path::new(path), Some(home)) else {
                panic!("`{path}` firmlink uzerinden kabul edildi");
            };
            assert_eq!(error.code(), RegistryErrorCode::PathRefused, "`{path}`");
        }

        // Firmlink uzerinden gelen **mesru** bir proje yolu yine kabul edilir.
        refuse_unsuitable_root(
            Path::new("/System/Volumes/Data/tmp/x/home/Work/asuna"),
            Some(home),
        )
        .expect("firmlink uzerinden gelen gercek proje kabul edilmeli");
    }

    /// `/System` ve kardesleri **on ek** olarak reddedilir: alt agaclarinin
    /// hicbiri kullanicinin projesi degil.
    #[test]
    fn system_subtrees_are_refused_by_prefix() {
        let home = Path::new("/tmp/x/home");

        for path in [
            "/System",
            "/System/Volumes",
            "/System/Library/CoreServices",
            "/Library/Keychains",
            "/Applications/Xcode.app",
            "/Network/Servers",
        ] {
            let Err(error) = refuse_unsuitable_root(Path::new(path), Some(home)) else {
                panic!("`{path}` kabul edildi");
            };
            assert_eq!(error.code(), RegistryErrorCode::PathRefused, "`{path}`");
        }
    }

    /// Tam eslesme listesi **alt agaci** kapatmaz: harici disk ve gecici dizin
    /// altindaki gercek projeler kaydedilebilmeli.
    #[test]
    fn refused_directories_do_not_close_their_subtrees() {
        let home = Path::new("/tmp/x/home");

        for path in ["/Volumes", "/usr", "/private", "/private/var", "/Users"] {
            assert!(
                refuse_unsuitable_root(Path::new(path), Some(home)).is_err(),
                "`{path}` kabul edildi"
            );
        }

        for path in [
            "/Volumes/Yedek/isler/proje",
            "/usr/local/src/deneme",
            "/private/var/folders/ab/cd/T/asuna-test",
        ] {
            refuse_unsuitable_root(Path::new(path), Some(home))
                .unwrap_or_else(|error| panic!("`{path}` reddedildi: {error}"));
        }
    }

    /// **Gercek makine kaniti**: `$HOME`un ustu ve firmlink'li `~/Library`
    /// gercek `HOME` degeriyle de reddediliyor.
    #[test]
    fn the_real_home_ancestors_are_refused_on_this_machine() {
        // `HOME` yoksa test bir sey kanitlamaz.
        let Some(home) = home_directory() else {
            return;
        };
        let Some(parent) = home.parent() else {
            return;
        };

        assert!(
            refuse_unsuitable_root(parent, Some(&home)).is_err(),
            "ev dizininin ust dizini kabul edildi: {}",
            parent.display()
        );

        let firmlinked = Path::new(DATA_VOLUME_PREFIX)
            .join(home.join("Library").strip_prefix("/").expect("mutlak yol"));
        assert!(
            refuse_unsuitable_root(&firmlinked, Some(&home)).is_err(),
            "firmlink uzerinden ~/Library kabul edildi: {}",
            firmlinked.display()
        );
    }

    /// `~/Library` ve altindaki her sey: uygulama destek dosyalari, tarayici
    /// profilleri, keychain. Proje degil.
    #[test]
    fn the_library_directory_cannot_be_registered() {
        let temp = TempDir::new("library");
        let home = std::fs::canonicalize(temp.path()).expect("canonicalize");

        for suffix in [
            "Library",
            "Library/Application Support",
            "Library/Keychains",
        ] {
            let Err(error) = refuse_unsuitable_root(&home.join(suffix), Some(&home)) else {
                panic!("`{suffix}` kabul edildi");
            };
            assert_eq!(error.code(), RegistryErrorCode::PathRefused, "`{suffix}`");
        }
    }

    /// Blok listesindeki bir dizin kok olamaz. Sandbox altindaki dosyalari
    /// zaten okumuyordu; kaydi bastan reddetmek ayni karari gorunur kiliyor.
    #[test]
    fn blocklisted_directories_cannot_be_registered() {
        let temp = TempDir::new("blocked");
        let home = std::fs::canonicalize(temp.path()).expect("canonicalize");

        for suffix in [".ssh", ".aws", ".gnupg", "secrets", "app-credentials"] {
            let Err(error) = refuse_unsuitable_root(&home.join(suffix), Some(&home)) else {
                panic!("`{suffix}` kabul edildi");
            };
            assert_eq!(error.code(), RegistryErrorCode::PathRefused, "`{suffix}`");
        }
    }

    /// Ev dizini cozulemediginde kalan kurallar yine kosar; olmayan bir koruma
    /// var gibi gosterilmez.
    #[test]
    fn root_validation_still_works_without_a_home_directory() {
        assert!(refuse_unsuitable_root(Path::new("/"), None).is_err());
        assert!(refuse_unsuitable_root(Path::new("/usr"), None).is_err());
        assert!(refuse_unsuitable_root(Path::new("/Users/kimse/.ssh"), None).is_err());
        assert!(refuse_unsuitable_root(Path::new("/Users/kimse/Work/asuna"), None).is_ok());
    }

    /// Gecici dizinler (`/private/var/folders/...`) **reddedilmez**: sistem
    /// dizini listesi tam eslesme, on ek degil. Bu testlerin kendisi de o
    /// dizinlerde kosuyor.
    #[test]
    fn a_directory_under_a_system_path_is_still_registrable() {
        let temp = TempDir::new("under-system");
        RegisteredRoot::resolve(&text(temp.path())).expect("gecici dizin kaydedilebilmeli");
    }

    #[test]
    fn a_missing_path_cannot_be_registered() {
        let temp = TempDir::new("missing");
        let error = RegisteredRoot::resolve(&text(&temp.path().join("yok")))
            .expect_err("var olmayan yol reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::PathNotFound);
    }

    #[test]
    fn a_file_is_not_a_project_root() {
        let temp = TempDir::new("file");
        let file = temp.path().join("README.md");
        std::fs::write(&file, b"merhaba").expect("dosya");

        let error = RegisteredRoot::resolve(&text(&file)).expect_err("dosya reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::NotADirectory);
    }

    /// **Kabul kriteri**: `..` cozulur ve symlink izlenir.
    #[test]
    fn traversal_and_symlinks_are_resolved() {
        let temp = TempDir::new("normalise");
        let real = temp.child("gercek");
        temp.child("baska");

        let noisy = format!("{}/baska/../gercek", text(temp.path()));
        let resolved = RegisteredRoot::resolve(&noisy).expect("normalize edilmeli");
        assert_eq!(
            resolved.as_path(),
            std::fs::canonicalize(&real).expect("canonical")
        );

        #[cfg(unix)]
        {
            let link = temp.path().join("kisayol");
            std::os::unix::fs::symlink(&real, &link).expect("symlink");
            let through_link = RegisteredRoot::resolve(&text(&link)).expect("symlink cozulmeli");
            assert_eq!(through_link.as_path(), resolved.as_path());
        }
    }

    // --- Slug ---------------------------------------------------------------

    #[test]
    fn slugs_are_readable_ascii() {
        assert_eq!(slugify("asuna"), "asuna");
        assert_eq!(slugify("Nexos Investment"), "nexos-investment");
        assert_eq!(slugify("Şeytan İbo"), "seytan-ibo");
        assert_eq!(slugify("my_project.v2"), "my-project-v2");
        assert_eq!(slugify("...  "), FALLBACK_SLUG);
        assert_eq!(slugify("--kenar--"), "kenar");
    }

    // --- Ekleme -------------------------------------------------------------

    #[test]
    fn adding_a_directory_registers_it_with_a_canonical_path() {
        let temp = TempDir::new("add");
        let root = temp.child("asuna");
        let db = db();

        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));

        assert_eq!(project.id, "asuna");
        assert_eq!(project.name, "asuna");
        assert_eq!(project.status, ProjectStatus::Active);
        assert_eq!(
            project.path.as_deref(),
            std::fs::canonicalize(&root).expect("canonical").to_str()
        );
        // Ekleme "guncel proje" secimi degildir.
        assert_eq!(project.last_opened_at, None);
        assert!(current(&db).expect("okunmali").is_none());
    }

    /// **Kabul kriteri**: cift kayit engeli. Ayni dizin — hatta gurultulu bir
    /// yol metniyle — ikinci bir satir acmaz.
    #[test]
    fn the_same_directory_cannot_be_registered_twice() {
        let temp = TempDir::new("dup");
        let root = temp.child("asuna");
        let db = db();

        let first = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));

        let noisy = format!("{}/../asuna", text(&root));
        let second = add(&db, &noisy, None, LATER).expect("ikinci cagri hata olmamali");

        match second {
            ProjectAddOutcome::AlreadyRegistered { project } => assert_eq!(project.id, first.id),
            ProjectAddOutcome::Registered { project } => {
                panic!("cift kayit acildi: {}", project.id)
            }
        }
        assert_eq!(list(&db, LATER).expect("listelenmeli").len(), 1);
    }

    /// Ayni **adli** iki farkli dizin: kimlik catismasi son ekle cozulur.
    #[test]
    fn two_directories_with_the_same_name_get_distinct_ids() {
        let temp = TempDir::new("collide");
        let first = temp.child("bir/asuna");
        let second = temp.child("iki/asuna");
        let db = db();

        assert_eq!(
            registered(add(&db, &text(&first), None, NOW).expect("ilk")).id,
            "asuna"
        );
        assert_eq!(
            registered(add(&db, &text(&second), None, NOW).expect("ikinci")).id,
            "asuna-2"
        );
    }

    /// **ASU-039 karari burada odeniyor**: Phase 3'ten devralinan etiket, ayni
    /// adli dizin ilk kez kaydedildiginde **sahiplenilir** — eski hafizalar
    /// oksuz kalmaz.
    #[test]
    fn registering_a_directory_adopts_the_carried_over_label() {
        let temp = TempDir::new("adopt");
        let root = temp.child("asuna");
        let db = db();

        // Phase 3 gibi: etiketli bir hafiza.
        db.with_connection(|connection| {
            crate::db::project_repository::ensure_label(connection, "asuna", NOW)?;
            connection.execute(
                "INSERT INTO memories (kind, title, content, project_id, importance, confidence, created_at, updated_at)
                 VALUES ('decision', 'Karar', 'Icerik', 'asuna', 0.9, 1.0, ?1, ?1)",
                rusqlite::params![NOW],
            )
        })
        .expect("etiketli hafiza");

        let project = registered(add(&db, &text(&root), Some("Asuna"), LATER).expect("eklenmeli"));

        assert_eq!(project.id, "asuna", "yeni bir kimlik uretilmemeli");
        assert_eq!(project.status, ProjectStatus::Active);
        assert_eq!(project.name, "Asuna");

        let linked: i64 = db
            .with_connection(|connection| {
                connection.query_row(
                    "SELECT count(*) FROM memories WHERE project_id = 'asuna'",
                    [],
                    |row| row.get(0),
                )
            })
            .expect("sayilmali");
        assert_eq!(linked, 1, "eski hafiza yeni kayda baglanmali");
    }

    // --- Missing ------------------------------------------------------------

    /// **Kabul kriteri**: sonradan kaybolan kok `missing` isaretlenir, kayit
    /// **silinmez**.
    #[test]
    fn a_root_that_disappears_is_marked_missing_not_deleted() {
        let temp = TempDir::new("vanish");
        let root = temp.child("asuna");
        let db = db();
        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));

        std::fs::remove_dir_all(&root).expect("dizin silinmeli");

        let projects = list(&db, LATER).expect("listelenmeli");
        assert_eq!(projects.len(), 1, "kayit silinmemeli");
        assert_eq!(projects[0].status, ProjectStatus::Missing);
        assert_eq!(projects[0].path.as_deref(), project.path.as_deref());

        // Geri gelirse yeniden `active`.
        std::fs::create_dir_all(&root).expect("dizin geri");
        assert_eq!(
            list(&db, LATER).expect("listelenmeli")[0].status,
            ProjectStatus::Active
        );
    }

    /// Kaybolmus proje "guncel proje" yapilamaz — Asuna okuyamayacagi bir
    /// projeyi "su an buradayiz" diye sunmamali.
    #[test]
    fn a_missing_project_cannot_become_the_current_one() {
        let temp = TempDir::new("current-missing");
        let root = temp.child("asuna");
        let db = db();
        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));
        std::fs::remove_dir_all(&root).expect("silinmeli");

        let error = set_current(&db, &project.id, LATER).expect_err("reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::Refused);
        // Durum da duzeltilmis olmali: kullanici nedenini listede gorsun.
        assert_eq!(
            require_project(&db, &project.id).expect("okunmali").status,
            ProjectStatus::Missing
        );
        assert!(current(&db).expect("okunmali").is_none());
    }

    // --- Guncel proje -------------------------------------------------------

    #[test]
    fn the_current_project_is_an_explicit_choice_never_a_guess() {
        let temp = TempDir::new("current");
        let first = temp.child("bir");
        let second = temp.child("iki");
        let db = db();

        let bir = registered(add(&db, &text(&first), None, NOW).expect("ilk"));
        registered(add(&db, &text(&second), None, NOW).expect("ikinci"));

        // Iki proje kayitli ama hicbiri secilmedi: tahmin yok.
        assert!(current(&db).expect("okunmali").is_none());

        let chosen = set_current(&db, &bir.id, LATER).expect("secilmeli");
        assert_eq!(chosen.last_opened_at.as_deref(), Some(LATER));
        assert_eq!(current(&db).expect("okunmali").map(|p| p.id), Some(bir.id));
    }

    #[test]
    fn a_label_without_a_root_cannot_become_the_current_project() {
        let db = db();
        db.with_connection(|connection| {
            crate::db::project_repository::ensure_label(connection, "etiket", NOW)
        })
        .expect("etiket");

        let error = set_current(&db, "etiket", LATER).expect_err("reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::Refused);
    }

    // --- Kaldirma -----------------------------------------------------------

    #[test]
    fn removing_a_project_without_memories_deletes_the_row() {
        let temp = TempDir::new("remove");
        let root = temp.child("asuna");
        let db = db();
        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));

        let outcome = remove(&db, &project.id, LATER).expect("kaldirilmali");
        assert_eq!(
            outcome,
            ProjectRemoveOutcome::Deleted {
                id: project.id.clone()
            }
        );
        assert!(list(&db, LATER).expect("listelenmeli").is_empty());
    }

    /// Kayit kaldirmak **hafizayi silmez**: etiket korunur.
    #[test]
    fn removing_a_project_with_memories_keeps_the_label() {
        let temp = TempDir::new("remove-linked");
        let root = temp.child("asuna");
        let db = db();
        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));

        db.with_connection(|connection| {
            connection.execute(
                "INSERT INTO memories (kind, title, content, project_id, importance, confidence, created_at, updated_at)
                 VALUES ('decision', 'Karar', 'Icerik', ?1, 0.9, 1.0, ?2, ?2)",
                rusqlite::params![project.id, NOW],
            )
        })
        .expect("hafiza");

        let outcome = remove(&db, &project.id, LATER).expect("kaldirilmali");
        match outcome {
            ProjectRemoveOutcome::Unlinked {
                project: unlinked,
                references,
            } => {
                assert_eq!(references, 1);
                assert_eq!(unlinked.status, ProjectStatus::Unlinked);
                assert_eq!(unlinked.path, None, "kayitli kok kalkmali");
            }
            other => panic!("etikete dusurulmeliydi: {other:?}"),
        }

        let label: Option<String> = db
            .with_connection(|connection| {
                connection.query_row("SELECT project_id FROM memories", [], |row| row.get(0))
            })
            .expect("okunmali");
        assert_eq!(label.as_deref(), Some(project.id.as_str()));
    }

    #[test]
    fn removing_an_unknown_project_is_a_typed_error() {
        let db = db();
        assert_eq!(
            remove(&db, "yok", NOW).expect_err("hata").code(),
            RegistryErrorCode::NotFound
        );
    }

    // --- Guncelleme ---------------------------------------------------------

    #[test]
    fn updating_renames_and_archives() {
        let temp = TempDir::new("update");
        let root = temp.child("asuna");
        let db = db();
        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));

        let renamed = update(
            &db,
            &project.id,
            &ProjectPatch {
                name: Some("  Asuna Companion  ".to_owned()),
                description: Some(Some("Sesli companion".to_owned())),
                archived: Some(true),
            },
            LATER,
        )
        .expect("guncellenmeli");

        assert_eq!(renamed.name, "Asuna Companion");
        assert_eq!(renamed.description.as_deref(), Some("Sesli companion"));
        assert_eq!(renamed.status, ProjectStatus::Archived);
        // Kimlik degismez: hafiza etiketleri ona bagli.
        assert_eq!(renamed.id, project.id);

        // Arsivden cikarma diskteki gercek duruma doner.
        let restored = update(
            &db,
            &project.id,
            &ProjectPatch {
                archived: Some(false),
                ..ProjectPatch::default()
            },
            LATER,
        )
        .expect("guncellenmeli");
        assert_eq!(restored.status, ProjectStatus::Active);
    }

    /// Arsivlenmis proje disk tazelemesinde **kendiliginden** aktiflesmez.
    #[test]
    fn refreshing_does_not_unarchive_a_project() {
        let temp = TempDir::new("archived");
        let root = temp.child("asuna");
        let db = db();
        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));
        update(
            &db,
            &project.id,
            &ProjectPatch {
                archived: Some(true),
                ..ProjectPatch::default()
            },
            LATER,
        )
        .expect("arsivlenmeli");

        assert_eq!(refresh_statuses(&db, LATER).expect("tazelenmeli"), 0);
        assert_eq!(
            require_project(&db, &project.id).expect("okunmali").status,
            ProjectStatus::Archived
        );
    }

    #[test]
    fn an_empty_name_is_rejected() {
        let temp = TempDir::new("blank");
        let root = temp.child("asuna");
        let db = db();

        assert_eq!(
            add(&db, &text(&root), Some("   "), NOW)
                .expect_err("hata")
                .code(),
            RegistryErrorCode::Invalid
        );
        assert!(list(&db, NOW).expect("listelenmeli").is_empty());
    }

    #[test]
    fn an_over_long_description_is_rejected() {
        let temp = TempDir::new("long");
        let root = temp.child("asuna");
        let db = db();
        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));

        let error = update(
            &db,
            &project.id,
            &ProjectPatch {
                description: Some(Some("a".repeat(MAX_DESCRIPTION_CHARS + 1))),
                ..ProjectPatch::default()
            },
            LATER,
        )
        .expect_err("reddedilmeli");
        assert_eq!(error.code(), RegistryErrorCode::Invalid);
    }

    // --- Tespit edilen metadata ---------------------------------------------

    #[test]
    fn detected_metadata_is_recorded_without_touching_the_name() {
        let temp = TempDir::new("detect");
        let root = temp.child("asuna");
        let db = db();
        let project = registered(add(&db, &text(&root), None, NOW).expect("eklenmeli"));

        record_detected_metadata(
            &db,
            &project.id,
            &DetectedMetadata {
                primary_language: Some("TypeScript".to_owned()),
                framework: Some("Tauri".to_owned()),
                git_remote: Some("github.com/omergungor/asuna".to_owned()),
            },
            LATER,
        )
        .expect("yazilmali");

        let updated = require_project(&db, &project.id).expect("okunmali");
        assert_eq!(updated.primary_language.as_deref(), Some("TypeScript"));
        assert_eq!(updated.framework.as_deref(), Some("Tauri"));
        assert_eq!(
            updated.git_remote.as_deref(),
            Some("github.com/omergungor/asuna")
        );
        assert_eq!(updated.name, project.name);
    }

    // --- Hata sozlesmesi ----------------------------------------------------

    /// Hata mesajlari kullanicinin dizin yapisini tekrarlamaz.
    #[test]
    fn errors_never_echo_the_path() {
        let temp = TempDir::new("no-echo");
        let secret = temp.child("cok-gizli-dizin");
        let inner = secret.join("yok");

        let error = RegisteredRoot::resolve(&text(&inner)).expect_err("hata");
        assert!(!error.to_string().contains("cok-gizli-dizin"), "{error}");
    }

    #[test]
    fn errors_serialize_as_code_and_message() {
        let json = serde_json::to_value(RegistryError::PathNotFound).expect("serialize");
        assert_eq!(json["code"], "path-not-found");

        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["code", "message"]);
    }

    /// Renderer'in gordugu sonuc sozlesmesi — `src/shared/project.ts` aynasi.
    #[test]
    fn outcomes_serialize_with_a_tagged_status() {
        let temp = TempDir::new("wire");
        let root = temp.child("asuna");
        let db = db();
        let outcome = add(&db, &text(&root), None, NOW).expect("eklenmeli");

        let json = serde_json::to_value(&outcome).expect("serialize");
        assert_eq!(json["status"], "registered");
        assert_eq!(json["project"]["status"], "active");

        let removed = serde_json::to_value(remove(&db, "asuna", LATER).expect("kaldirilmali"))
            .expect("serialize");
        assert_eq!(removed["status"], "deleted");
        assert_eq!(removed["id"], "asuna");
    }

    /// Kalici depolama kapaliyken "eklendi" demek yalan olurdu.
    #[test]
    fn adding_a_project_while_storage_is_disabled_is_an_error() {
        let error = database(&DbState::Disabled).expect_err("hata bekleniyordu");
        assert_eq!(error.code(), RegistryErrorCode::Disabled);

        let error = database(&DbState::Unavailable {
            reason: "veritabani dosyasi acilamadi".to_owned(),
        })
        .expect_err("hata bekleniyordu");
        assert_eq!(error.code(), RegistryErrorCode::Unavailable);
    }

    /// Otomatik tarama yok: bos bir registry, dolu bir diskte bile bostur.
    #[test]
    fn the_registry_never_discovers_projects_on_its_own() {
        let temp = TempDir::new("no-scan");
        temp.child("bir");
        temp.child("iki/uc");
        let db = db();

        assert!(
            list(&db, NOW).expect("listelenmeli").is_empty(),
            "kayitli olmayan dizinler asla kendiliginden gorunmemeli"
        );
    }
}

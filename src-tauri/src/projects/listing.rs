//! `list_project_dir` — kayitli proje koku icinde **dizin listeleme** yuzeyi
//! (ASU-068, PROJECT.md Bolum 17/19).
//!
//! # Neden var
//!
//! ASU-051 tek bir dosyayi okuyabiliyordu ama Asuna'nin "freelancer klasorunde
//! ne var?" sorusuna verecek hicbir cevabi yoktu: dosya adini bilmeden dosya
//! okunamaz ve model ad **uydurmamali**. Bu modul o bosluğu kapatir — icerik
//! degil, yalnizca **isimler**.
//!
//! # Neden Rust tarafinda
//!
//! [`super::files`] ile birebir ayni gerekce: tool runner renderer'da yasiyor
//! ve renderer guvenilmez. Renderer yalnizca kok'e gore gorece bir metin verir;
//! kok secimi, yol cozumu, blok listesi ve tavan burada.
//!
//! # Uc bilincli karar
//!
//! 1. **Ozyineleme yok.** Tek seviye listelenir. Model alt dizini merak
//!    ediyorsa ayrica sorar. Ozyineleme, `node_modules/` ya da `.git/` gibi bir
//!    dizine denk geldiginde sesli bir oturuma on binlerce satir bosaltirdi;
//!    ayrica "ne kadarini gordum?" sorusunun cevabini bulaniklastirirdi.
//!    Yan etkisi tam olarak istenen sey: `.git`, `node_modules`, `target`
//!    listede **tek satir** olarak gorunur, icleri acilmaz.
//! 2. **Girdi tavani** [`MAX_DIRECTORY_ENTRIES`]. Asilirsa
//!    [`ProjectDirectoryView::truncated`] doner ve gercek toplam
//!    ([`ProjectDirectoryView::total_entries`]) yaninda durur — kirpma sessiz
//!    degil (`files.rs` ile ayni kural).
//! 3. **Blok listesindeki girdiler gizlenmez, isaretlenir.** `.env` bir
//!    dizinde duruyorsa listede `blocked: true` ile gorunur. Gizlemek
//!    kullaniciyi "neden gormuyor?" diye sasirtirdi ve modelin dizin icerigi
//!    hakkinda yanlis bir zihinsel model kurmasina yol acardi. **Isim bir
//!    sizinti degil**: icerigi okuma yolu ([`super::files`]) blok listesi
//!    tarafindan kapali kalmaya devam ediyor ve bu modul hicbir dosyayi acmaz.
//!
//! # Ret sessiz degil
//!
//! Sandbox reddi oldugu gibi tasinir ([`SandboxViolation`]); ek olarak tek yeni
//! kod var: hedef bir **dizin degilse** [`ProjectDirectoryError::NotADirectory`]
//! doner. `not_found` ile karistirilmaz — "dosya var ama dizin degil" ile
//! "boyle bir sey yok" modelin farkli davranmasi gereken iki durum.

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::db::{AsunaDb, DbState};
use crate::security::blocklist;
use crate::security::sandbox::{self, SandboxViolation};

use super::registry::{self, RegistryError};

/// Tek cagrida donebilecek en fazla girdi.
///
/// # Neden 200
///
/// Cikti **modele** gidiyor ve model sesli cevap uretecek. Elle yazilmis bir
/// proje dizininde 200 girdi zaten fazlasiyla ustte; asan yerler pratikte
/// uretilmis dizinlerdir (`node_modules/`, `target/`) ve orada tam listenin
/// konusma degeri yok. Tavan asildiginda cagri **duşmez**, kirpilir ve
/// kirpildigi soylenir.
pub const MAX_DIRECTORY_ENTRIES: usize = 200;

/// Tek cagrida **taranacak** en fazla girdi (Gate 3 M2).
///
/// # Neden ikinci bir tavan gerekiyor
///
/// [`MAX_DIRECTORY_ENTRIES`] yalnizca **ciktiyi** koruyordu, **isi** degil:
/// `node_modules/.pnpm` gibi bir dizinde on binlerce girdi okunuyor ve
/// symlink'ler icin binlerce `canonicalize` cagriliyordu. TS tarafi 10 sn'de
/// timeout dondurse bile Rust durmaz — `invoke` iptal edilmiyor — yani sesli
/// oturumun ortasinda diski mesgul eden bir is arkada calismaya devam ederdi.
///
/// # Neden 5 000
///
/// Ciktinin 25 kati: gercek bir proje dizininde asilmasi imkansiz, uretilmis
/// bir dizinde ise hemen asiliyor. Asildiginda cagri **duşmez**: tarama durur,
/// [`ProjectDirectoryView::scan_capped`] doner ve
/// [`ProjectDirectoryView::total_entries`] "**en az** bu kadar" anlamina gelir.
pub const MAX_SCANNED_ENTRIES: usize = 5_000;

// ---------------------------------------------------------------------------
// Cikti
// ---------------------------------------------------------------------------

/// Bir dizin girdisinin turu.
///
/// `Other` bilincli bir kova: symlink hedefi kaybolmus bir bag, soket, aygit
/// dosyasi... Modelin bunlari "dosya" sanip okumaya calismasi yerine ne
/// oldugunu bilmemesi daha durust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Dir,
    File,
    Other,
}

/// Listelenen tek bir girdi.
///
/// GIZLILIK: yalnizca **ad** doner; mutlak yol hicbir alanda yok
/// ([`super::files::ProjectFileView`] ile ayni kural).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntryView {
    /// Dizin icindeki ad (yol degil).
    pub name: String,
    pub kind: EntryKind,
    /// Yalnizca duz dosyalarda dolu.
    pub size_bytes: Option<u64>,
    /// Asuna bu girdiyi **okuyamaz**: blok listesine takiliyor, kok disina
    /// cikan bir symlink ya da adi metne cevrilemiyor.
    pub blocked: bool,
}

/// Listelenmis dizin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectoryView {
    pub project_id: String,
    pub project_name: String,
    /// Kok'e gore yol; **kok'un kendisi icin bos metin**.
    pub path: String,
    pub entries: Vec<DirectoryEntryView>,
    /// Sayilan girdi sayisi (kirpmadan once).
    ///
    /// [`Self::scan_capped`] `true` ise bu deger **alt sinirdir**: dizinde en az
    /// bu kadar girdi var, daha fazlasi olabilir.
    pub total_entries: usize,
    /// Tarama [`MAX_SCANNED_ENTRIES`] tavaninda durduruldu.
    ///
    /// Ayri bir alan cunku "200'de kirpildi" ile "5 000'de saymayi biraktik"
    /// farkli seyler: ilkinde toplam biliniyor, ikincisinde bilinmiyor. Tek
    /// bayrakla anlatmak, modele bilmedigi bir sayiyi biliyormus gibi
    /// verdirirdi.
    pub scan_capped: bool,
    /// Donen girdi sayisi (olculen, tahmin degil).
    pub returned_entries: usize,
    /// Tavan asildi: [`Self::entries`] listenin **basi**.
    pub truncated: bool,
    /// Uygulanan tavanin kendisi de gorunur — kirpmanin nedeni sorulabilsin.
    pub max_entries: usize,
}

// ---------------------------------------------------------------------------
// Ret
// ---------------------------------------------------------------------------

/// `list_project_dir` reddi.
///
/// GIZLILIK: hicbir varyantin mesaji yol tasimaz.
#[derive(Debug, thiserror::Error)]
pub enum ProjectDirectoryError {
    /// Sandbox reddetti. Gerekce **oldugu gibi** tasinir.
    #[error("{0}")]
    Denied(#[from] SandboxViolation),

    /// Hedef var ama dizin degil.
    ///
    /// [`SandboxViolation::NotAFile`]'in aynasi ve ayri bir kod: model "dizin
    /// sandim, mesela dosyaymis" ile "boyle bir sey yok"u ayirt edebilmeli.
    #[error("hedef bir dizin degil")]
    NotADirectory,

    /// Guncel proje secilmemis ya da hic proje kayitli degil.
    #[error("guncel proje secilmemis; once hangi projede calisildigi belirlenmeli")]
    NoCurrentProject,

    /// Kalici depolama kapali — kayitli kok listesi okunamiyor.
    #[error("kalici depolama kapali; kayitli proje kokleri okunamiyor")]
    Disabled,

    #[error("hafiza kullanilamiyor: {reason}")]
    Unavailable { reason: String },

    #[error("veritabani islemi basarisiz")]
    Storage,
}

impl From<RegistryError> for ProjectDirectoryError {
    fn from(value: RegistryError) -> Self {
        match value {
            RegistryError::Disabled => Self::Disabled,
            RegistryError::Unavailable { reason } => Self::Unavailable { reason },
            _ => Self::Storage,
        }
    }
}

impl ProjectDirectoryError {
    /// Makine tarafinin ayirt etmesi icin sabit kod.
    ///
    /// Sandbox reddi kendi kodunu tasir; kumeler kesismiyor
    /// (`not_a_directory` sandbox'ta yok, `not_a_file` burada yok).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Denied(violation) => violation.code(),
            Self::NotADirectory => "not_a_directory",
            Self::NoCurrentProject => "no_current_project",
            Self::Disabled => "disabled",
            Self::Unavailable { .. } => "unavailable",
            Self::Storage => "storage",
        }
    }

    /// Bu ret bir **kacis denemesi** miydi? Renderer bunu hesaplamaz.
    pub fn escape_attempt(&self) -> bool {
        match self {
            Self::Denied(violation) => violation.is_escape_attempt(),
            _ => false,
        }
    }

    /// `tool_events.result_summary` alanina yazilacak tek satirlik ozet.
    pub fn audit_summary(&self) -> String {
        match self {
            Self::Denied(violation) => violation.audit_outcome().result_summary,
            other => format!("reddedildi ({}): {other}", other.code()),
        }
    }
}

impl Serialize for ProjectDirectoryError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            code: &'a str,
            message: &'a str,
            escape_attempt: bool,
            audit_summary: &'a str,
        }

        let message = self.to_string();
        let audit_summary = self.audit_summary();
        Wire {
            code: self.code(),
            message: &message,
            escape_attempt: self.escape_attempt(),
            audit_summary: &audit_summary,
        }
        .serialize(serializer)
    }
}

// ---------------------------------------------------------------------------
// Listeleme
// ---------------------------------------------------------------------------

/// Guncel proje icindeki bir dizini listeler.
///
/// Saf(ish) is fonksiyonu: komut yalnizca `State` cozer ve buraya devreder.
///
/// `relative` bos ise (ya da `.` / `a/..` gibi kok'e cozulen bir yolsa) hedef
/// **proje kokudur**. [`sandbox::resolve_in_root`] o durumda
/// [`SandboxViolation::Empty`] doner — bu bir dosya hedefi icin dogru karar,
/// bir dizin hedefi icin degil; burada kok'e cevriliyor.
pub fn list(db: &AsunaDb, relative: &str) -> Result<ProjectDirectoryView, ProjectDirectoryError> {
    let project = registry::current(db)?.ok_or(ProjectDirectoryError::NoCurrentProject)?;
    let root = sandbox::resolve_project_root(db, &project.id)?;

    let (target, path) = match sandbox::resolve_in_root(&root, relative) {
        Ok(resolved) => (
            resolved.as_path().to_path_buf(),
            resolved.relative().to_owned(),
        ),
        Err(SandboxViolation::Empty) => (root.clone(), String::new()),
        Err(violation) => return Err(violation.into()),
    };

    let metadata = std::fs::metadata(&target).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SandboxViolation::NotFound,
        _ => SandboxViolation::Unreadable,
    })?;
    if !metadata.is_dir() {
        return Err(ProjectDirectoryError::NotADirectory);
    }

    let Scan {
        mut entries,
        capped: scan_capped,
    } = read_entries(&target, &root)?;
    entries.sort_by(|left, right| {
        order_of(left.kind)
            .cmp(&order_of(right.kind))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });

    let total_entries = entries.len();
    let truncated = scan_capped || total_entries > MAX_DIRECTORY_ENTRIES;
    entries.truncate(MAX_DIRECTORY_ENTRIES);

    Ok(ProjectDirectoryView {
        project_id: project.id,
        project_name: project.name,
        path,
        returned_entries: entries.len(),
        entries,
        total_entries,
        truncated,
        scan_capped,
        max_entries: MAX_DIRECTORY_ENTRIES,
    })
}

/// Siralama: once dizinler, sonra dosyalar, en son "diger".
///
/// Sesli bir cevapta "su klasorler var, su dosyalar var" demek, karisik bir
/// listeyi okumaktan anlasilir. Sirali olmasi ayrica **deterministik**: `readdir`
/// sirasi dosya sistemine gore degisir ve ayni dizin iki cagrida farkli
/// gorunurdu.
const fn order_of(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Dir => 0,
        EntryKind::File => 1,
        EntryKind::Other => 2,
    }
}

/// Tarama sonucu: girdiler + tavana takilip takilmadigi.
struct Scan {
    entries: Vec<DirectoryEntryView>,
    /// [`MAX_SCANNED_ENTRIES`]'e ulasildi; dizinde daha fazlasi olabilir.
    capped: bool,
}

/// Dizin girdilerini okur.
///
/// Tek bir girdinin okunamamasi **yutulmaz**: liste yarim donmez, cagri tipli
/// olarak duser. Yarim bir liste "bu dizinde bunlar var" diye sunulurdu ve
/// eksigi gorunmezdi.
///
/// Tarama [`MAX_SCANNED_ENTRIES`] girdide **durur** (Gate 3 M2): tavana
/// takilmak bir hata degil, olculen ve bildirilen bir durum. Iterator burada
/// birakildigi icin kalan girdiler hic okunmaz ve onlar icin `metadata` /
/// `canonicalize` cagrilmaz — korunan sey cikti degil, **is**.
fn read_entries(target: &Path, root: &Path) -> Result<Scan, ProjectDirectoryError> {
    let read_dir = std::fs::read_dir(target).map_err(|_| SandboxViolation::Unreadable)?;

    let mut entries = Vec::new();
    for entry in read_dir {
        if entries.len() >= MAX_SCANNED_ENTRIES {
            return Ok(Scan {
                entries,
                capped: true,
            });
        }
        let entry = entry.map_err(|_| SandboxViolation::Unreadable)?;
        entries.push(describe_entry(&entry, root));
    }
    Ok(Scan {
        entries,
        capped: false,
    })
}

/// Tek bir girdiyi gorunume cevirir. Dosya **acilmaz**.
fn describe_entry(entry: &std::fs::DirEntry, root: &Path) -> DirectoryEntryView {
    let raw_name = entry.file_name();
    let (name, name_readable) = match raw_name.to_str() {
        Some(value) => (value.to_owned(), true),
        // UTF-8 disi ad: gosterebiliriz ama modelin bu adi tekrar yazip dosyaya
        // ulasmasi garanti degil. Belirsizlikte "okunamaz" isaretlemek dogru
        // yondeki hata (`blocklist::is_blocked` ayni karari veriyor).
        None => (raw_name.to_string_lossy().into_owned(), false),
    };

    let full_path = entry.path();
    let metadata = entry.metadata().ok();
    let kind = match metadata.as_ref() {
        Some(value) if value.is_dir() => EntryKind::Dir,
        Some(value) if value.is_file() => EntryKind::File,
        _ => EntryKind::Other,
    };
    let blocked = !name_readable || is_unreadable(&full_path, entry, root);

    // Bloklu girdide **boyut da donmez**: `.env`in kac bayt oldugu kucuk ama
    // gereksiz bir sizinti (kac anahtar var?). Okunamayan bir dosyanin olcusu
    // modelin isine yaramaz.
    let size_bytes = match (kind, metadata.as_ref()) {
        (EntryKind::File, Some(value)) if !blocked => Some(value.len()),
        _ => None,
    };

    DirectoryEntryView {
        blocked,
        name,
        kind,
        size_bytes,
    }
}

/// Bu girdi Asuna icin kapali mi?
///
/// Iki kaynak:
///
/// 1. **Blok listesi** — cozulmus tam yol uzerinde (`.env`, `*.pem`, `.ssh/`...).
/// 2. **Kok disina cikan symlink** — bagin kendisi kok icinde ama hedefi
///    disarida. `resolve_in_root` boyle bir yolu okumada zaten
///    [`SandboxViolation::SymlinkEscape`] ile reddediyor; liste de ayni cevabi
///    **onceden** vermeli, yoksa model okunabilir sanip cagri israf eder.
fn is_unreadable(full_path: &Path, entry: &std::fs::DirEntry, root: &Path) -> bool {
    if blocklist::is_blocked(full_path).is_some() {
        return true;
    }

    let is_symlink = entry
        .file_type()
        .map(|file_type| file_type.is_symlink())
        .unwrap_or(true);
    if !is_symlink {
        return false;
    }

    match std::fs::canonicalize(full_path) {
        Ok(resolved) => {
            !resolved.starts_with(root) || blocklist::is_blocked_resolved(&resolved).is_some()
        }
        // Kirik bag: hedefi cozulemiyor, dolayisiyla okunamaz.
        Err(_) => true,
    }
}

/// Guncel proje koku icindeki bir dizini listeler (ASU-068).
///
/// Renderer yalnizca **kok'e gore gorece bir yol** verebilir (bos metin =
/// proje koku). Ne projeyi ne mutlak bir yolu secebilir; `~`, mutlak yol ve
/// `..` ile disari cikma girisimi tipli olarak reddedilir. Dosya **icerigi
/// donmez** — yalnizca adlar, turler ve boyutlar.
#[tauri::command]
pub fn list_project_dir(
    state: State<'_, DbState>,
    path: String,
) -> Result<ProjectDirectoryView, ProjectDirectoryError> {
    let db = registry::database(&state)?;
    list(db, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use crate::projects::registry::ProjectAddOutcome;

    const NOW: &str = "2026-08-31T10:00:00Z";

    /// Izole gecici dizin — `files.rs` testleriyle ayni desen (sahte filesystem
    /// yok, gercek `canonicalize` davranisi olculuyor).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asuna-listing-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("gecici dizin");
            Self(path)
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

    struct Fixture {
        db: AsunaDb,
        root: TempDir,
    }

    fn fixture(label: &str) -> Fixture {
        let root = TempDir::new(label);
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let path = root.path().to_string_lossy().into_owned();

        let outcome = registry::add(&db, &path, Some("Deneme"), NOW).expect("proje kaydedilmeli");
        let project = match outcome {
            ProjectAddOutcome::Registered { project }
            | ProjectAddOutcome::AlreadyRegistered { project } => project,
        };
        registry::set_current(&db, &project.id, NOW).expect("guncel proje secilmeli");

        Fixture { db, root }
    }

    fn write(fixture: &Fixture, relative: &str, contents: &str) {
        let path = fixture.root.path().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("ust dizin");
        }
        std::fs::write(path, contents).expect("dosya yazilmali");
    }

    fn names(view: &ProjectDirectoryView) -> Vec<&str> {
        view.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    fn entry<'a>(view: &'a ProjectDirectoryView, name: &str) -> &'a DirectoryEntryView {
        view.entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("`{name}` girdisi listede yok: {:?}", names(view)))
    }

    /// **ASU-068 kabul kaniti**: bos yol proje kokunu listeler.
    #[test]
    fn an_empty_path_lists_the_project_root() {
        let fixture = fixture("root");
        write(&fixture, "README.md", "# Asuna\n");
        std::fs::create_dir_all(fixture.root.path().join("src")).expect("src");

        let view = list(&fixture.db, "").expect("listelenmeli");

        assert_eq!(view.path, "");
        assert_eq!(view.total_entries, 2);
        assert_eq!(view.returned_entries, 2);
        assert!(!view.truncated);
        // Once dizin, sonra dosya.
        assert_eq!(names(&view), vec!["src", "README.md"]);
        assert_eq!(entry(&view, "src").kind, EntryKind::Dir);
        assert_eq!(entry(&view, "src").size_bytes, None);
        assert_eq!(entry(&view, "README.md").kind, EntryKind::File);
        assert_eq!(entry(&view, "README.md").size_bytes, Some(8));
    }

    /// `.` ve kok'e cozulen yollar da kok demektir.
    #[test]
    fn paths_that_resolve_to_the_root_are_accepted_as_the_root() {
        let fixture = fixture("rootalias");
        write(&fixture, "a.txt", "x");

        for relative in [".", "./", "src/..", "  "] {
            let view = list(&fixture.db, relative)
                .unwrap_or_else(|error| panic!("`{relative}` listelenmeli: {error}"));
            assert_eq!(view.path, "", "`{relative}` kok'e cozulmeli");
        }
    }

    #[test]
    fn a_subdirectory_is_listed_relative_to_the_root() {
        let fixture = fixture("subdir");
        write(&fixture, "src/main.rs", "fn main() {}");
        write(&fixture, "src/lib.rs", "//");

        let view = list(&fixture.db, "src").expect("listelenmeli");

        assert_eq!(view.path, "src");
        assert_eq!(names(&view), vec!["lib.rs", "main.rs"]);
    }

    /// **Ozyineleme yok**: alt dizin tek satir olarak gorunur, icerigi acilmaz.
    #[test]
    fn nested_directories_are_a_single_line_and_never_expanded() {
        let fixture = fixture("norecurse");
        write(
            &fixture,
            "node_modules/left-pad/index.js",
            "module.exports=0",
        );
        write(&fixture, "target/debug/build.log", "log");
        write(&fixture, ".git/HEAD", "ref: refs/heads/main");

        let view = list(&fixture.db, "").expect("listelenmeli");

        assert_eq!(names(&view), vec![".git", "node_modules", "target"]);
        for name in [".git", "node_modules", "target"] {
            assert_eq!(entry(&view, name).kind, EntryKind::Dir);
        }
        // Alt girdilerin hicbiri listede yok.
        assert!(!names(&view).contains(&"left-pad"));
        assert!(!names(&view).contains(&"HEAD"));
    }

    /// Blok listesindeki dosya **gorunur ama isaretli**; icerigi hicbir zaman
    /// donmez (bu modul hicbir dosya acmaz).
    #[test]
    fn blocked_files_are_visible_but_marked() {
        let fixture = fixture("blocked");
        write(&fixture, ".env", "OPENAI_API_KEY=sk-SIZMAMALI");
        write(&fixture, "server.pem", "-----BEGIN PRIVATE KEY-----");
        write(&fixture, "README.md", "# ok");

        let view = list(&fixture.db, "").expect("listelenmeli");

        assert!(entry(&view, ".env").blocked);
        assert!(entry(&view, "server.pem").blocked);
        assert!(!entry(&view, "README.md").blocked);

        let serialized = serde_json::to_string(&view).expect("serilesmeli");
        assert!(
            !serialized.contains("SIZMAMALI"),
            "dosya icerigi listeye sizdi: {serialized}"
        );
    }

    /// Blok listesindeki bir **dizin** listelenemez.
    #[test]
    fn a_blocklisted_directory_cannot_be_listed() {
        let fixture = fixture("blockeddir");
        write(&fixture, ".ssh/id_ed25519", "PRIVATE");

        let error = list(&fixture.db, ".ssh").expect_err("reddedilmeli");

        assert_eq!(error.code(), "blocklisted");
        assert!(!error.escape_attempt());
    }

    /// **Kotu yol seti**: kacis denemeleri tipli olarak reddediliyor.
    #[test]
    fn escape_attempts_are_refused_with_their_own_codes() {
        let fixture = fixture("escape");

        for (relative, code) in [
            ("../..", "traversal"),
            ("../../.ssh", "traversal"),
            ("~/.ssh", "tilde"),
            ("~", "tilde"),
            ("/etc", "absolute"),
            ("/", "absolute"),
            ("src/../../..", "traversal"),
        ] {
            let Err(error) = list(&fixture.db, relative) else {
                panic!("`{relative}` reddedilmeliydi");
            };
            assert_eq!(error.code(), code, "yol `{relative}`");
            assert!(error.escape_attempt(), "yol `{relative}` kacis sayilmali");
        }
    }

    #[test]
    fn a_file_target_is_refused_with_its_own_code() {
        let fixture = fixture("notadir");
        write(&fixture, "README.md", "# Asuna");

        let error = list(&fixture.db, "README.md").expect_err("reddedilmeli");

        assert_eq!(error.code(), "not_a_directory");
        assert!(!error.escape_attempt());
    }

    #[test]
    fn a_missing_directory_is_not_a_refusal() {
        let fixture = fixture("missingdir");

        let error = list(&fixture.db, "yok").expect_err("reddedilmeli");

        assert_eq!(error.code(), "not_found");
        assert!(!error.escape_attempt());
    }

    /// **Tavan**: 200 girdiden fazlasi kirpilir ve kirpma sessiz degil.
    #[test]
    fn the_entry_cap_is_applied_and_reported() {
        let fixture = fixture("cap");
        for index in 0..(MAX_DIRECTORY_ENTRIES + 25) {
            write(&fixture, &format!("dosya-{index:04}.txt"), "x");
        }

        let view = list(&fixture.db, "").expect("listelenmeli");

        assert_eq!(view.total_entries, MAX_DIRECTORY_ENTRIES + 25);
        assert_eq!(view.returned_entries, MAX_DIRECTORY_ENTRIES);
        assert_eq!(view.entries.len(), MAX_DIRECTORY_ENTRIES);
        assert!(view.truncated);
        assert_eq!(view.max_entries, MAX_DIRECTORY_ENTRIES);
    }

    /// **Gate 3 M2 regresyonu**: 200 tavani yalnizca ciktiyi koruyordu, isi
    /// degil. Artik tarama da tavanli ve tavana takildigi **bildiriliyor**.
    ///
    /// Olculen sey: tavanin uzerindeki girdiler icin `metadata` cagrilmiyor.
    /// Dogrudan olcmek yerine tarama tavaninin uygulandigi kanitlaniyor —
    /// `total_entries` tavanda duruyor ve `scan_capped` bunu soyluyor.
    #[test]
    fn the_scan_stops_at_the_work_cap_and_says_so() {
        let fixture = fixture("scancap");
        for index in 0..(MAX_SCANNED_ENTRIES + 40) {
            write(&fixture, &format!("d-{index:05}.txt"), "x");
        }

        let view = list(&fixture.db, "").expect("listelenmeli");

        assert_eq!(view.total_entries, MAX_SCANNED_ENTRIES);
        assert!(view.scan_capped, "tarama tavani bildirilmedi");
        assert!(view.truncated);
        assert_eq!(view.returned_entries, MAX_DIRECTORY_ENTRIES);
    }

    /// Tavanin altinda kalan bir dizin "eksik" diye isaretlenmez.
    #[test]
    fn a_small_directory_is_not_marked_as_capped() {
        let fixture = fixture("nocap");
        for index in 0..(MAX_DIRECTORY_ENTRIES + 5) {
            write(&fixture, &format!("d-{index:04}.txt"), "x");
        }

        let view = list(&fixture.db, "").expect("listelenmeli");

        assert_eq!(view.total_entries, MAX_DIRECTORY_ENTRIES + 5);
        assert!(!view.scan_capped, "tarama tavani yanlislikla bildirildi");
        // Cikti yine kirpik: iki eksen bagimsiz.
        assert!(view.truncated);
    }

    /// **Gate 3 L1**: bloklu bir girdinin boyutu donmez — kac bayt oldugu
    /// kucuk ama gereksiz bir sizinti.
    #[test]
    fn blocked_entries_do_not_report_their_size() {
        let fixture = fixture("blockedsize");
        write(
            &fixture,
            ".env",
            "OPENAI_API_KEY=sk-uzun-bir-deger-buraya\n",
        );
        write(&fixture, "README.md", "# ok");

        let view = list(&fixture.db, "").expect("listelenmeli");

        let blocked = entry(&view, ".env");
        assert!(blocked.blocked);
        assert_eq!(blocked.size_bytes, None, "bloklu girdi boyut sizdirdi");
        assert!(entry(&view, "README.md").size_bytes.is_some());
    }

    #[test]
    fn an_empty_directory_is_reported_as_empty_not_as_an_error() {
        let fixture = fixture("empty");
        std::fs::create_dir_all(fixture.root.path().join("bos")).expect("bos dizin");

        let view = list(&fixture.db, "bos").expect("listelenmeli");

        assert_eq!(view.total_entries, 0);
        assert!(view.entries.is_empty());
        assert!(!view.truncated);
    }

    /// Kok disina cikan bir symlink listede **okunamaz** isaretlenir: model
    /// okunabilir sanip cagri israf etmesin.
    #[cfg(unix)]
    #[test]
    fn a_symlink_that_escapes_the_root_is_marked_unreadable() {
        let outside = TempDir::new("outside");
        std::fs::write(outside.path().join("gizli.txt"), "DISARIDA").expect("dosya");

        let fixture = fixture("symlink");
        write(&fixture, "icerde.txt", "burada");
        std::os::unix::fs::symlink(
            outside.path().join("gizli.txt"),
            fixture.root.path().join("kacak.txt"),
        )
        .expect("symlink");

        let view = list(&fixture.db, "").expect("listelenmeli");

        assert!(entry(&view, "kacak.txt").blocked);
        assert!(!entry(&view, "icerde.txt").blocked);
    }

    #[test]
    fn listing_without_a_current_project_is_refused() {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");

        let error = list(&db, "").expect_err("reddedilmeli");

        assert_eq!(error.code(), "no_current_project");
    }

    /// Hicbir ret **kok yolunu** tasimaz (`SandboxViolation` ile ayni kural).
    ///
    /// Verilen gorece yol da tekrarlanmiyor: mesajlar sabit metinler
    /// (`"yol proje kokunun disina cikiyor"`) ve girdiyi geri yansitmiyorlar.
    #[test]
    fn refusals_never_carry_the_root_path() {
        let fixture = fixture("nopath");
        let root = fixture.root.path().to_string_lossy().into_owned();
        let canonical = std::fs::canonicalize(fixture.root.path())
            .expect("canonicalize")
            .to_string_lossy()
            .into_owned();

        for relative in ["../..", "/etc", "yok", "~/.ssh", "gizli-dosya-adi"] {
            let error = list(&fixture.db, relative).expect_err("reddedilmeli");
            let serialized = serde_json::to_string(&error).expect("serilesmeli");
            assert!(
                !serialized.contains(&root) && !serialized.contains(&canonical),
                "kok yolu sizdi (`{relative}`): {serialized}"
            );
            assert!(
                !serialized.contains("gizli-dosya-adi"),
                "girdi geri yansidi (`{relative}`): {serialized}"
            );
        }
    }
}

//! Path sandbox — dosya erisiminin **tek** kapisi (ASU-049).
//!
//! `asuna-config/security.md` Bolum 2 ve PROJECT.md Bolum 19'un kod karsiligi.
//! Bir tool bir dosyaya dokunacaksa yolu buradan gecer; baska bir yol yoktur ve
//! olmayacaktir.
//!
//! # Neden Rust tarafinda
//!
//! Renderer guvenilmez (docs/architecture/security.md Bolum 1). Renderer "hangi
//! kok" diyemez, "hangi dosya" derken de yalnizca **kok'e gore gorece** bir metin
//! verebilir. Kok secimi ve cozum burada, guven sinirinin icinde yapilir.
//!
//! # Akis
//!
//! ```text
//! (project_id, relative)
//!   → registry'den kayitli kok            (yalnizca `active`)
//!     → kok canonicalize                  (kok'un KENDISI symlink olabilir)
//!       → gorece yolun leksik cozumu      (`.`/`..` dosya sistemine dokunmadan)
//!         → aday = kok + leksik yol
//!           → adayin KENDISI canonicalize (symlink cozumu; yol yoksa en yakin
//!             var olan ata + kalan bilesenler)
//!             → sonuc hala kok altinda mi?   (`starts_with`, metin degil bilesen)
//!               → blok listesi (cozulmus yol uzerinde)
//!                 → izin
//! ```
//!
//! # Karar: leksik cozum **once**, `canonicalize` **sonra**
//!
//! `..` bilesenleri dosya sistemine sorulmadan, saymayla cozulur. Iki kazanci
//! var:
//!
//! 1. Var olmayan bir yol icin de karar verilebilir — "dosya yok" ile "kacis
//!    denendi" ayni gorunmez ([`crate::db::transcript`] ayni deseni kullaniyor).
//! 2. `link/../x` gibi bir yol, `link`in **hedefinin** ustune degil, kok icindeki
//!    `x`e cozulur. Kabuk semantigi burada bilerek terk ediliyor: daha kisitlayici
//!    olan yorum seciliyor.
//!
//! `canonicalize` yine de cagriliyor cunku leksik cozum symlink gormez; kok
//! icindeki bir bagin disariyi gostermesi ancak gercek cozumle yakalanir.
//!
//! # Karar: percent-encoding **cozulmez**
//!
//! `..%2F..%2F.ssh%2Fid_ed25519` gibi bir girdi decode **edilmez**. Decode etmek,
//! "hangi katman kac kez cozer?" sorusunu acar ve klasik cift-decode aciklarini
//! davet eder. Ham metin tek bir dosya adi bileseni olarak degerlendirilir:
//! sonuc kok'un **icinde** kalir ve o adda bir dosya olmadigi icin okuma
//! [`SandboxViolation::NotFound`] ile duser. Yani girdi kacamaz; yalnizca
//! anlamsizlasir.
//!
//! # Karar: blok listesi **cozulmus tam yol** uzerinde calisir
//!
//! Kok'un kendi bilesenleri de kontrole girer. Sonuc: `~/.ssh` ya da
//! `~/secrets/x` gibi bir dizin proje olarak kaydedilmis olsa bile altindaki
//! dosyalar okunmaz — [`SandboxViolation::Blocklisted`] doner. Bu bilincli bir
//! yanlis pozitif: boyle bir kokun "normal proje" olma ihtimali, sessizce
//! credential okumanin riskinden kucuk.
//!
//! # Reddin sessiz olmamasi
//!
//! Her ret **tipli** doner ([`SandboxViolation`]) ve
//! [`SandboxViolation::audit_outcome`] ile dogrudan `tool_events` satirina
//! cevrilebilir. Bos icerik donup "dosya bostu" izlenimi vermek yasak.

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use crate::db::model::ProjectStatus;
use crate::db::tool_event_repository::MAX_RESULT_SUMMARY_CHARS;
use crate::db::{project_repository, AsunaDb, ToolApprovalState};

use super::blocklist::{self, BlockReason};

// ---------------------------------------------------------------------------
// Tavanlar
// ---------------------------------------------------------------------------

/// Gorece yol metninin karakter tavani.
///
/// `registry::MAX_PATH_CHARS` ile ayni buyukluk sinifi (macOS `PATH_MAX` 1024).
/// Amac dar bir limit koymak degil, acikca sacma girdiyi (megabaytlik "yol")
/// cozum makinesine sokmadan kesmek.
pub const MAX_RELATIVE_PATH_CHARS: usize = 4096;

/// Bu katmanda okunabilecek en buyuk dosya.
///
/// # Neden 256 KiB, neden kirpma degil **red**
///
/// - **Neden bir tavan var:** okuma bir ses oturumunun ortasinda oluyor. Cok
///   megabaytlik bir dosyayi belege alip %99'unu atmak, gecikmeye duyarli bir
///   dongude bosa I/O ve bosa bellek.
/// - **Neden bu kadar yuksek:** ozet butcesi cok daha kucuk
///   (`context::MAX_TOTAL_CONTEXT_CHARS` 6000 karakter). Tavani oraya yakin
///   secmek, "README'mi oku" diyen kullaniciya normal ama uzun bir dosyayi
///   **reddettirirdi**. 256 KiB, elle yazilmis bir kaynak/dokuman dosyasinin
///   makul ustunun birkac kati.
/// - **Neden bu kadar dusuk:** 256 KiB ustu bir "metin" dosyasi pratikte
///   uretilmis ya da vendor'lanmis veridir (lock, bundle, dump) — sesli olarak
///   sorulan sey degildir.
/// - **Neden kirpma degil red:** kirpma bir **sunum** karari ve ASU-051'in isi.
///   Guvenlik katmaninin kirpmasi, "ne kadarini gordum?" sorusunun cevabini iki
///   yere dagitirdi. Burada tavan asilirsa cagri tipli olarak duser; ASU-051
///   kendi (daha kucuk) butcesini bu tavanin **altinda** uygular ve kirptigini
///   ciktida soyler.
pub const MAX_READABLE_FILE_BYTES: u64 = 256 * 1024;

/// Ikili dosya tespiti icin bakilan on ek.
pub const BINARY_SNIFF_BYTES: usize = 8 * 1024;

/// On ekte kabul edilen en yuksek kontrol baytı orani (yuzde).
///
/// `\t`, `\n`, `\r` metindir ve sayilmaz. 0x80+ baytlar UTF-8 devam baytlari
/// olabilir, onlar da sayilmaz. Geriye kalan C0 kontrol baytlari ve `DEL` bir
/// metin dosyasinda seyrek gorunur; %10 esigi hem gercek metni (form feed,
/// vertical tab iceren eski dokumanlar) gecirir hem ikiliyi yakalar.
pub const MAX_CONTROL_BYTE_PERCENT: u32 = 10;

// ---------------------------------------------------------------------------
// Ret
// ---------------------------------------------------------------------------

/// Sandbox'in bir erisimi **neden** reddettigi.
///
/// GIZLILIK: hicbir varyantin mesaji yol tasimaz. Yolu zaten cagiran taraf
/// biliyor; mesajin log'a, UI'a ve `tool_events` satirina dusen kopyasinda
/// kullanicinin dizin yapisini tekrarlamanin kazanci yok
/// (`projects::registry::RegistryError` ile ayni kural).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SandboxViolation {
    /// Verilen kimlikte kayitli, kullanilabilir bir proje koku yok:
    /// hic kaydedilmemis, yalnizca bir etiket (`unlinked`) ya da arsivlenmis.
    #[error("bu kimlikte kullanilabilir bir kayitli proje koku yok")]
    NotRegistered,

    /// Kok kayitli ama su an diskte yok/erisilemiyor (`missing`).
    #[error("projenin kok dizini su an bulunamiyor")]
    RootMissing,

    /// Gorece yol bos ya da kok'un kendisini gosteriyor (`.`).
    #[error("dosya yolu bos; kok dizinin kendisi bir dosya hedefi degil")]
    Empty,

    /// Yol metni tavanı asti.
    #[error("dosya yolu cok uzun")]
    TooLong,

    /// Yol NUL baytı iceriyor — kesme (truncation) saldirisinin klasik girdisi.
    #[error("dosya yolu gecersiz karakter iceriyor")]
    NullByte,

    /// Mutlak yol verildi. Sandbox'ta mutlak yol diye bir sey yok: hedef her
    /// zaman kayitli kok'e goredir.
    #[error("mutlak yol kabul edilmiyor; yol kayitli proje kokune gore verilmeli")]
    AbsolutePath,

    /// `~` ile baslayan yol. Kabuk sozdizimi genisletilmez (registry ile ayni
    /// kural): hangi home dizininin kastedildigi tahmin edilmez.
    #[error("`~` genisletilmez; yol kayitli proje kokune gore verilmeli")]
    Tilde,

    /// `..` bilesenleri kok'un disina cikiyor.
    #[error("yol proje kokunun disina cikiyor")]
    Traversal,

    /// Yol kok icinde kaliyordu ama bir symlink cozuldugunde disari cikti.
    #[error("yol bir sembolik bag uzerinden proje kokunun disina cikiyor")]
    SymlinkEscape,

    /// Cozulmus yol hassas dosya blok listesine takildi. Gerekce blok
    /// listesinden **oldugu gibi** tasinir: "gizli olabilir" degil, somut kural.
    #[error("{}", .0.describe())]
    Blocklisted(BlockReason),

    /// Yol var ama duz dosya degil (dizin, soket, aygit...).
    #[error("hedef bir duz dosya degil")]
    NotAFile,

    /// Yol sandbox icinde ama diskte boyle bir dosya yok. **Uydurulmaz.**
    #[error("boyle bir dosya yok")]
    NotFound,

    /// Dosya [`MAX_READABLE_FILE_BYTES`] tavanini asiyor.
    #[error("dosya cok buyuk: {size_bytes} bayt, tavan {limit_bytes} bayt")]
    TooLarge { size_bytes: u64, limit_bytes: u64 },

    /// Ikili icerik. Modele ham gonderilmez (security.md Bolum 2).
    #[error("dosya ikili gorunuyor; icerigi metin olarak okunmadi")]
    Binary,

    /// Dosya var ama okunamadi (izin, I/O).
    #[error("dosya okunamadi")]
    Unreadable,
}

impl SandboxViolation {
    /// Makine tarafinin ayirt etmesi icin sabit kod. Log ve audit'te bu geciyor;
    /// mesaj metni degisse bile kod sabit kalir.
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotRegistered => "not_registered",
            Self::RootMissing => "root_missing",
            Self::Empty => "empty",
            Self::TooLong => "too_long",
            Self::NullByte => "null_byte",
            Self::AbsolutePath => "absolute",
            Self::Tilde => "tilde",
            Self::Traversal => "traversal",
            Self::SymlinkEscape => "symlink_escape",
            Self::Blocklisted(_) => "blocklisted",
            Self::NotAFile => "not_a_file",
            Self::NotFound => "not_found",
            Self::TooLarge { .. } => "too_large",
            Self::Binary => "binary",
            Self::Unreadable => "unreadable",
        }
    }

    /// Bu ret bir **guvenlik** ihlali mi, yoksa siradan bir "yok/uygun degil"
    /// durumu mu?
    ///
    /// Ikisi ayri sunulur: kacis denemesi kullaniciya gorunur bir uyari,
    /// "dosya yok" ise sadece durust bir cevaptir.
    pub const fn is_escape_attempt(self) -> bool {
        matches!(
            self,
            Self::AbsolutePath | Self::Tilde | Self::Traversal | Self::SymlinkEscape
        )
    }
}

/// Bir ihlalin `tool_events` satirina donusmus hali (ASU-050 sozlesmesi).
///
/// Audit yazimini **bu modul yapmaz** — yazan taraf tool sarmalayicisidir
/// (ASU-051). Burada yalnizca donusum duruyor ki her cagri yerinde
/// `approval_state`in ne olacagi yeniden yorumlanmasin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxAuditOutcome {
    /// **Her zaman** [`ToolApprovalState::NotRequested`]: sandbox on-kontroldur,
    /// onay asamasina hic gelinmemistir. `Denied` degil — `Denied` kullanicinin
    /// verdigi karardir, bu ise politikanin kararidir (ASU-050 notu).
    pub approval_state: ToolApprovalState,
    /// `tool_events.result_summary` alanina yazilacak tek satirlik ozet.
    /// Yol **icermez**, tavan uygulanmistir.
    pub result_summary: String,
}

impl SandboxViolation {
    /// Ihlali audit satirina cevirir.
    ///
    /// Reddedilen erisim sessizce bos donmez: cagiran taraf bu ciktiyi
    /// `recordToolEvent`e gecirir ve satir defterde gorunur.
    pub fn audit_outcome(self) -> SandboxAuditOutcome {
        let mut summary = format!("reddedildi ({}): {self}", self.code());
        if summary.chars().count() > MAX_RESULT_SUMMARY_CHARS {
            summary = summary.chars().take(MAX_RESULT_SUMMARY_CHARS).collect();
        }
        SandboxAuditOutcome {
            approval_state: ToolApprovalState::NotRequested,
            result_summary: summary,
        }
    }
}

// ---------------------------------------------------------------------------
// Cozulmus yol
// ---------------------------------------------------------------------------

/// Sandbox'tan **gecmis** mutlak yol.
///
/// Bu tipi uretmenin tek yolu [`resolve_in_project`] / [`resolve_in_root`];
/// yani "dogrulanmis yol" ile "modelden gelen metin" tip duzeyinde ayrilir
/// (`RegisteredRoot` ile ayni desen). Bir fonksiyon `SandboxedPath` aliyorsa
/// kontrolun yapildigi derleme zamaninda okunur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxedPath {
    root: PathBuf,
    path: PathBuf,
    relative: String,
}

impl SandboxedPath {
    /// Cozulmus mutlak yol. Diske bu yolla gidilir.
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    /// Erisimi veren kayitli kok (canonicalize edilmis).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Kok'e gore gosterilebilir yol — UI, model ciktisi ve audit icin.
    ///
    /// Mutlak yol **donmez**: kullanicinin dizin yapisi ne modele ne de
    /// `tool_events` satirina girer.
    pub fn relative(&self) -> &str {
        &self.relative
    }
}

/// Sandbox'tan gecmis, okunmus metin dosyasi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxedFile {
    /// Dosyanin tamami. **Kirpilmamis** — kirpma ASU-051'in isi.
    pub text: String,
    /// Diskteki olculen boyut.
    pub size_bytes: u64,
}

// ---------------------------------------------------------------------------
// Cozum
// ---------------------------------------------------------------------------

/// Kayitli bir projenin icindeki bir yolu cozer.
///
/// Kok listesi **yalnizca** [`crate::projects::registry`]'den gelir
/// (registry.rs bas yorumundaki sozlesme). Cagiran taraf kok secemez.
///
/// Yalnizca [`ProjectStatus::Active`] bir kayit erisim verir:
/// - `unlinked` bir satir yolu olmayan bir **etiket**tir, yetki degil;
/// - `archived` kullanicinin "burada calismiyorum" karari;
/// - `missing` kok su an diskte yok.
///
/// Dosyanin **var olmasi gerekmez**: cozum ile okuma ayri adimlar
/// ([`read_text`]).
pub fn resolve_in_project(
    db: &AsunaDb,
    project_id: &str,
    relative: &str,
) -> Result<SandboxedPath, SandboxViolation> {
    let root = resolve_project_root(db, project_id)?;
    resolve_in_root(&root, relative)
}

/// Kayitli bir projenin **kok dizinini** cozer.
///
/// [`resolve_in_project`]'in ilk yarisi, ayri bir fonksiyon olarak. Sebep:
/// kokun **kendisi** de mesru bir hedef olabilir — `list_project_dir` (ASU-068)
/// proje kokunu listeler ve [`resolve_in_root`] o durumda
/// [`SandboxViolation::Empty`] doner (bir **dosya** hedefi olarak dogru karar,
/// bir **dizin** hedefi olarak degil).
///
/// Ayni durum ve blok listesi kurallari gecerli:
///
/// - Yalnizca [`ProjectStatus::Active`] bir kayit erisim verir.
/// - Kok `canonicalize` edilir (kokun kendisi symlink olabilir) ve gercekten
///   dizin olmasi sart kosulur.
/// - **Kok da blok listesinden gecer**: `~/.ssh` ya da `~/secrets/x` gibi bir
///   dizin proje olarak kaydedilmis olsa bile acilmaz. Bu, modul basindaki
///   "blok listesi cozulmus tam yol uzerinde calisir" kuralinin kok'e dusen
///   yarisi — [`resolve_in_root`] dosya yolunda ayni karari zaten veriyordu,
///   burasi dizin yolunda ayni sonucu uretir.
pub fn resolve_project_root(db: &AsunaDb, project_id: &str) -> Result<PathBuf, SandboxViolation> {
    let project = project_repository::find_by_id(db, project_id)
        .map_err(|_| SandboxViolation::NotRegistered)?
        .ok_or(SandboxViolation::NotRegistered)?;

    match project.status {
        ProjectStatus::Active => {}
        ProjectStatus::Missing => return Err(SandboxViolation::RootMissing),
        ProjectStatus::Archived | ProjectStatus::Unlinked => {
            return Err(SandboxViolation::NotRegistered)
        }
    }

    let root = project
        .path
        .as_deref()
        .ok_or(SandboxViolation::RootMissing)?;

    let canonical = std::fs::canonicalize(root).map_err(|_| SandboxViolation::RootMissing)?;
    if !canonical.is_dir() {
        return Err(SandboxViolation::RootMissing);
    }
    if let Some(reason) = blocklist::is_blocked_resolved(&canonical) {
        return Err(SandboxViolation::Blocklisted(reason));
    }
    Ok(canonical)
}

/// [`resolve_in_project`]'in kok'u dogrudan alan hali.
///
/// Ayri fonksiyon oldugu icin yol mantiginin tamami veritabani olmadan test
/// edilebiliyor — kotu yol vakalari gercek dizin ve gercek symlink uzerinde
/// kosuyor, taklit (mock) filesystem uzerinde degil.
pub fn resolve_in_root(root: &Path, relative: &str) -> Result<SandboxedPath, SandboxViolation> {
    let trimmed = relative.trim();
    if trimmed.is_empty() {
        return Err(SandboxViolation::Empty);
    }
    if trimmed.chars().count() > MAX_RELATIVE_PATH_CHARS {
        return Err(SandboxViolation::TooLong);
    }
    if trimmed.contains('\0') {
        return Err(SandboxViolation::NullByte);
    }
    // `~` yalnizca **basta** anlamlidir; ortadaki bir `~` siradan bir dizin
    // adidir ve oyle islenir.
    if trimmed.starts_with('~') {
        return Err(SandboxViolation::Tilde);
    }

    let lexical = lexical_relative(Path::new(trimmed))?;
    if lexical.as_os_str().is_empty() {
        // `.`, `./`, `a/..` — hedef kok'un kendisi.
        return Err(SandboxViolation::Empty);
    }

    // Kok'un KENDISI symlink olabilir; canonicalize sonrasi karsilastirma
    // yapmak bunu kendiliginden dogru kilar.
    let canonical_root = std::fs::canonicalize(root).map_err(|_| SandboxViolation::RootMissing)?;
    if !canonical_root.is_dir() {
        return Err(SandboxViolation::RootMissing);
    }

    let candidate = canonical_root.join(&lexical);
    let resolved = resolve_existing_prefix(&candidate).ok_or(SandboxViolation::RootMissing)?;

    // `starts_with` bilesen bazinda calisir: `/tmp/kok-2` yolu `/tmp/kok`
    // altinda sayilmaz. Ham metin karsilastirmasi bu tuzaga duserdi.
    if !resolved.starts_with(&canonical_root) {
        // Leksik cozumden sonra `..` kalmadigi icin disari cikmanin tek yolu
        // bir sembolik bagdir.
        return Err(SandboxViolation::SymlinkEscape);
    }

    // Sozlesme: blok listesi symlink cozuldukten **sonra** uygulanir.
    if let Some(reason) = blocklist::is_blocked_resolved(&resolved) {
        return Err(SandboxViolation::Blocklisted(reason));
    }

    let relative = resolved
        .strip_prefix(&canonical_root)
        .unwrap_or(&lexical)
        .to_string_lossy()
        .into_owned();

    Ok(SandboxedPath {
        root: canonical_root,
        path: resolved,
        relative,
    })
}

/// `.` ve `..` bilesenlerini **dosya sistemine dokunmadan** cozer.
///
/// `..` bir bileseni siler; silecek bilesen kalmadiysa yol kok'un disina
/// cikiyor demektir. `canonicalize` burada kullanilamaz: var olmayan bir dosya
/// icin hata dondururdu ve "dosya yok" ile "kacis denendi" ayirt edilemezdi
/// (bkz. [`crate::db::transcript`]).
fn lexical_relative(raw: &Path) -> Result<PathBuf, SandboxViolation> {
    let mut parts: Vec<&OsStr> = Vec::new();

    for component in raw.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Err(SandboxViolation::AbsolutePath)
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(SandboxViolation::Traversal);
                }
            }
            Component::Normal(part) => parts.push(part),
        }
    }

    Ok(parts.iter().collect())
}

/// Adayi canonicalize eder; yol yoksa **var olan en yakin ataya** kadar geri
/// sarar ve kalan bilesenleri uzerine ekler.
///
/// Neden gerekli: var olmayan bir dosya icin de "sandbox icinde mi?" sorusunun
/// cevabi verilebilmeli. Aksi halde bir tool once dosyayi olusturup sonra
/// sorabilirdi.
///
/// Kalan bilesenler leksik cozumden geciyor (`..` ve `.` yok), bu yuzden
/// eklemek yeni bir kacis yolu acmaz.
fn resolve_existing_prefix(candidate: &Path) -> Option<PathBuf> {
    let mut tail: Vec<OsString> = Vec::new();
    let mut current = candidate.to_path_buf();

    loop {
        if let Ok(base) = std::fs::canonicalize(&current) {
            let mut resolved = base;
            for part in tail.iter().rev() {
                resolved.push(part);
            }
            return Some(resolved);
        }
        let name = current.file_name()?.to_os_string();
        tail.push(name);
        if !current.pop() {
            return None;
        }
    }
}

// ---------------------------------------------------------------------------
// Okuma kapisi: boyut + ikili
// ---------------------------------------------------------------------------

/// Sandbox'tan gecmis bir yolu **metin olarak** okur.
///
/// Sirasiyla: duz dosya mi → boyut tavani → oku → ikili tespiti → UTF-8.
/// Her ret tipli; hicbir yol sessizce bos metin dondurmez.
pub fn read_text(path: &SandboxedPath) -> Result<SandboxedFile, SandboxViolation> {
    // `symlink_metadata` degil `metadata`: yol zaten canonicalize edilmis, bagi
    // burada bir kez daha cozmenin anlami yok ve `metadata` hedefin gercek
    // boyutunu verir.
    let metadata = std::fs::metadata(path.as_path()).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SandboxViolation::NotFound,
        _ => SandboxViolation::Unreadable,
    })?;

    if !metadata.is_file() {
        return Err(SandboxViolation::NotAFile);
    }

    let size_bytes = metadata.len();
    if size_bytes > MAX_READABLE_FILE_BYTES {
        return Err(SandboxViolation::TooLarge {
            size_bytes,
            limit_bytes: MAX_READABLE_FILE_BYTES,
        });
    }

    let bytes = std::fs::read(path.as_path()).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SandboxViolation::NotFound,
        _ => SandboxViolation::Unreadable,
    })?;

    if looks_binary(&bytes) {
        return Err(SandboxViolation::Binary);
    }

    // Ikili tespiti sezgisel; UTF-8 dogrulamasi kesin. Ikisi birlikte: modele
    // ham bayt gitmez.
    let text = String::from_utf8(bytes).map_err(|_| SandboxViolation::Binary)?;

    Ok(SandboxedFile { text, size_bytes })
}

/// Ilk [`BINARY_SNIFF_BYTES`] bayta bakarak icerigin ikili olup olmadigini
/// tahmin eder.
///
/// Iki olcut:
/// 1. **NUL baytı** — metin dosyalarinda pratikte bulunmaz, ikili dosyalarda
///    neredeyse her zaman bulunur. Tek basina yeterli kanit.
/// 2. **Kontrol baytı orani** — `\t`/`\n`/`\r` disi C0 baytlari ve `DEL`.
///    [`MAX_CONTROL_BYTE_PERCENT`] esigini asarsa ikili sayilir.
///
/// 0x80+ baytlar sayilmaz: UTF-8 metninde (Turkce dahil) normaldir. On ekle
/// yetinmek bilincli — bir dosyanin ikiliginin ilk 8 KiB'de belli olmamasi
/// nadirdir ve tam tarama her okumaya ikinci bir gecis eklerdi.
pub fn looks_binary(bytes: &[u8]) -> bool {
    let prefix = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    if prefix.is_empty() {
        return false;
    }
    if prefix.contains(&0) {
        return true;
    }

    let control = prefix
        .iter()
        .filter(|byte| {
            let value = **byte;
            (value < 0x09 || (0x0D < value && value < 0x20) || value == 0x7F)
                && value != 0x0A
                && value != 0x0D
        })
        .count();

    (control as u64) * 100 > (prefix.len() as u64) * u64::from(MAX_CONTROL_BYTE_PERCENT)
}

// ---------------------------------------------------------------------------
// Testler — **asil is** (ASU-049 kabul kriteri: min. 15 kotu yol vakasi)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbState;
    use crate::projects::registry::{self, ProjectAddOutcome, ProjectPatch};

    const NOW: &str = "2026-08-25T10:00:00Z";

    /// Izole gecici dizin — gercek uygulama veri dizinine **asla** dokunmaz.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "asuna-sandbox-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let path = std::env::temp_dir().join(unique);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("gecici dizin");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn dir(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("alt dizin");
            path
        }

        fn file(&self, name: &str, contents: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("ust dizin");
            }
            std::fs::write(&path, contents).expect("dosya");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Icinde normal bir dosya bulunan bir proje koku.
    fn project(label: &str) -> (TempDir, PathBuf) {
        let temp = TempDir::new(label);
        let root = temp.dir("proje");
        std::fs::write(root.join("README.md"), b"# Asuna\n").expect("README");
        (temp, root)
    }

    fn refuse(root: &Path, relative: &str) -> SandboxViolation {
        match resolve_in_root(root, relative) {
            Ok(allowed) => panic!(
                "reddedilmeliydi ama gecti: `{relative}` -> `{}`",
                allowed.relative()
            ),
            Err(violation) => violation,
        }
    }

    // -----------------------------------------------------------------------
    // Pozitif kontroller — sandbox calisan bir seyi kirmamali
    // -----------------------------------------------------------------------

    #[test]
    fn an_ordinary_file_inside_the_root_resolves() {
        let (_temp, root) = project("ok");

        let resolved = resolve_in_root(&root, "README.md").expect("okunabilmeli");
        assert_eq!(resolved.relative(), "README.md");
        assert!(resolved.as_path().starts_with(resolved.root()));
        assert_eq!(
            read_text(&resolved).expect("okunmali").text,
            "# Asuna\n".to_owned()
        );
    }

    #[test]
    fn noisy_but_contained_paths_are_normalised_not_refused() {
        let (_temp, root) = project("noisy");
        std::fs::create_dir_all(root.join("docs")).expect("docs");
        std::fs::write(root.join("docs/mimari.md"), b"metin").expect("dosya");

        for raw in [
            "./docs/mimari.md",
            "docs/./mimari.md",
            "docs/../docs/mimari.md",
            "  docs/mimari.md  ",
        ] {
            let resolved = resolve_in_root(&root, raw).expect(raw);
            assert_eq!(resolved.relative(), "docs/mimari.md", "girdi: {raw}");
        }
    }

    /// **Pozitif kontrol (kabul kriteri).** Kok ICINDE kalan bir symlink
    /// izlenir — sandbox "symlink gordum, reddettim" demez.
    #[cfg(unix)]
    #[test]
    fn a_symlink_that_stays_inside_the_root_is_allowed() {
        let (_temp, root) = project("link-inside");
        let real = root.join("docs");
        std::fs::create_dir_all(&real).expect("docs");
        std::fs::write(real.join("notlar.md"), b"icerik").expect("dosya");
        std::os::unix::fs::symlink(&real, root.join("kisayol")).expect("symlink");

        let resolved = resolve_in_root(&root, "kisayol/notlar.md").expect("izin verilmeli");
        assert_eq!(resolved.relative(), "docs/notlar.md");
        assert_eq!(read_text(&resolved).expect("okunmali").text, "icerik");
    }

    /// **Pozitif kontrol.** Kok'un KENDISI bir symlink olabilir (macOS'ta
    /// `/tmp` zaten oyle). Cozum gercek dizine iner ve icerideki dosya okunur.
    #[cfg(unix)]
    #[test]
    fn a_root_that_is_itself_a_symlink_resolves_to_its_target() {
        let temp = TempDir::new("root-link");
        let real = temp.dir("gercek-proje");
        std::fs::write(real.join("README.md"), b"icerik").expect("dosya");
        let link = temp.path().join("kok-bagi");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");

        let resolved = resolve_in_root(&link, "README.md").expect("izin verilmeli");
        let canonical_real = std::fs::canonicalize(&real).expect("canonical");

        assert_eq!(resolved.root(), canonical_real);
        assert!(resolved.as_path().starts_with(&canonical_real));
        assert_eq!(resolved.relative(), "README.md");
    }

    // -----------------------------------------------------------------------
    // Kotu yol 1-4: traversal
    // -----------------------------------------------------------------------

    /// **Kabul kriteri, birebir**: `../../.ssh/id_ed25519`.
    ///
    /// Varyant `Traversal`, `Blocklisted` degil: kacis leksik olarak, adin ne
    /// oldugu sorulmadan **once** karara baglaniyor.
    #[test]
    fn case_01_the_canonical_ssh_traversal_is_refused_as_traversal() {
        let (_temp, root) = project("t1");
        assert_eq!(
            refuse(&root, "../../.ssh/id_ed25519"),
            SandboxViolation::Traversal
        );
    }

    #[test]
    fn case_02_a_sibling_directory_escape_is_refused() {
        let temp = TempDir::new("t2");
        let root = temp.dir("proje");
        temp.file("komsu/gizli.txt", b"sir");

        assert_eq!(
            refuse(&root, "../komsu/gizli.txt"),
            SandboxViolation::Traversal
        );
    }

    #[test]
    fn case_03_a_bare_parent_component_is_refused() {
        let (_temp, root) = project("t3");
        assert_eq!(refuse(&root, ".."), SandboxViolation::Traversal);
        assert_eq!(refuse(&root, "../"), SandboxViolation::Traversal);
    }

    /// Normalize sonrasi **disari** cikan yol: `node_modules` bileseni bir `..`
    /// ile silinir, ikinci `..` kok'u asar.
    #[test]
    fn case_04_traversal_hidden_behind_a_real_directory_is_refused() {
        let (_temp, root) = project("t4");
        std::fs::create_dir_all(root.join("node_modules")).expect("dizin");

        assert_eq!(
            refuse(&root, "node_modules/../../.env"),
            SandboxViolation::Traversal
        );
    }

    // -----------------------------------------------------------------------
    // Kotu yol 5-8: mutlak yol ve `~`
    // -----------------------------------------------------------------------

    #[test]
    fn case_05_an_absolute_path_is_refused() {
        let (_temp, root) = project("t5");
        assert_eq!(refuse(&root, "/etc/passwd"), SandboxViolation::AbsolutePath);
    }

    #[test]
    fn case_06_the_filesystem_root_is_refused() {
        let (_temp, root) = project("t6");
        assert_eq!(refuse(&root, "/"), SandboxViolation::AbsolutePath);
    }

    /// `~` genisletilmez (registry ile ayni kural): hangi home dizini oldugu
    /// tahmin edilmez.
    #[test]
    fn case_07_a_tilde_home_path_is_refused_without_expansion() {
        let (_temp, root) = project("t7");
        assert_eq!(refuse(&root, "~/.ssh/id_ed25519"), SandboxViolation::Tilde);
    }

    #[test]
    fn case_08_a_tilde_user_path_is_refused_without_expansion() {
        let (_temp, root) = project("t8");
        assert_eq!(refuse(&root, "~root/.ssh/id_rsa"), SandboxViolation::Tilde);
    }

    // -----------------------------------------------------------------------
    // Kotu yol 9-12: bicimsiz girdi
    // -----------------------------------------------------------------------

    #[test]
    fn case_09_an_empty_path_is_refused() {
        let (_temp, root) = project("t9");
        assert_eq!(refuse(&root, ""), SandboxViolation::Empty);
        assert_eq!(refuse(&root, "    "), SandboxViolation::Empty);
    }

    /// Kok'un kendisi bir dosya hedefi degil.
    #[test]
    fn case_10_a_bare_current_directory_is_refused() {
        let (_temp, root) = project("t10");
        assert_eq!(refuse(&root, "."), SandboxViolation::Empty);
        assert_eq!(refuse(&root, "./"), SandboxViolation::Empty);
        assert_eq!(refuse(&root, "docs/.."), SandboxViolation::Empty);
    }

    #[test]
    fn case_11_a_path_with_a_nul_byte_is_refused() {
        let (_temp, root) = project("t11");
        assert_eq!(
            refuse(&root, "README.md\0/../../.ssh/id_ed25519"),
            SandboxViolation::NullByte
        );
    }

    #[test]
    fn case_12_an_absurdly_long_path_is_refused() {
        let (_temp, root) = project("t12");
        let long = format!("{}.md", "a".repeat(MAX_RELATIVE_PATH_CHARS));
        assert_eq!(refuse(&root, &long), SandboxViolation::TooLong);
    }

    // -----------------------------------------------------------------------
    // Kotu yol 13-19: blok listesi (kok'un ICINDE olsa bile)
    // -----------------------------------------------------------------------

    /// **Kabul kriteri**: `.env` proje kokunun icinde olsa da okunmaz.
    #[test]
    fn case_13_env_files_inside_the_root_are_blocklisted() {
        let (_temp, root) = project("t13");
        std::fs::write(root.join(".env"), b"OPENAI_API_KEY=sk-gercek").expect("dosya");
        std::fs::write(root.join(".env.local"), b"X=1").expect("dosya");

        for name in [".env", ".env.local", ".env.production"] {
            assert_eq!(
                refuse(&root, name),
                SandboxViolation::Blocklisted(BlockReason::EnvironmentFile),
                "okunmamali: {name}"
            );
        }
    }

    /// **Normalize sonrasi ICERIDE ama yine de bloklu.** Traversal kontrolu
    /// gecti; kararı blok listesi verdi.
    #[test]
    fn case_14_a_normalised_path_that_lands_on_a_blocked_file_is_refused() {
        let (_temp, root) = project("t14");
        std::fs::create_dir_all(root.join("node_modules")).expect("dizin");
        std::fs::write(root.join(".env"), b"GIZLI=1").expect("dosya");

        assert_eq!(
            refuse(&root, "node_modules/../.env"),
            SandboxViolation::Blocklisted(BlockReason::EnvironmentFile)
        );
    }

    #[test]
    fn case_15_private_key_material_inside_the_root_is_blocklisted() {
        let (_temp, root) = project("t15");
        for name in [
            "id_rsa",
            "id_rsa.pub",
            "id_ed25519",
            "keys/id_ecdsa",
            "certs/app.p12",
            "android/release.keystore",
            "certs/server.pem",
        ] {
            assert_eq!(
                refuse(&root, name),
                SandboxViolation::Blocklisted(BlockReason::PrivateKeyMaterial),
                "okunmamali: {name}"
            );
        }
    }

    #[test]
    fn case_16_a_dot_ssh_directory_inside_the_root_is_blocklisted() {
        let (_temp, root) = project("t16");
        assert_eq!(
            refuse(&root, ".ssh/config"),
            SandboxViolation::Blocklisted(BlockReason::SensitiveDirectory)
        );
        assert_eq!(
            refuse(&root, ".aws/credentials"),
            SandboxViolation::Blocklisted(BlockReason::SensitiveDirectory)
        );
    }

    #[test]
    fn case_17_credential_stores_inside_the_root_are_blocklisted() {
        let (_temp, root) = project("t17");
        for name in [".npmrc", ".netrc", ".git-credentials", ".pypirc"] {
            assert_eq!(
                refuse(&root, name),
                SandboxViolation::Blocklisted(BlockReason::CredentialStore),
                "okunmamali: {name}"
            );
        }
    }

    /// `.git/config` repo-yerel remote URL'inde token tasiyabilir
    /// (`https://x:ghp_...@github.com/...`) ve `[credential]` helper satirlarini
    /// barindirir. ASU-042 remote **adini** `git remote get-url` yolundan alir,
    /// bu dosyaya ihtiyaci yok.
    #[test]
    fn case_18_the_repo_local_git_config_is_blocklisted() {
        let (_temp, root) = project("t18");
        std::fs::create_dir_all(root.join(".git")).expect("dizin");
        std::fs::write(
            root.join(".git/config"),
            b"[remote \"origin\"]\n\turl = https://x:ghp_TOKEN@github.com/o/a.git\n",
        )
        .expect("dosya");

        assert_eq!(
            refuse(&root, ".git/config"),
            SandboxViolation::Blocklisted(BlockReason::CredentialStore)
        );
    }

    /// Kok'un KENDISI hassas bir dizinin altindaysa altindaki her sey reddedilir.
    /// Bilincli yanlis pozitif (modul dokumantasyonu).
    #[test]
    fn case_19_a_root_registered_under_a_sensitive_directory_grants_nothing() {
        let temp = TempDir::new("t19");
        let root = temp.dir(".ssh/sahte-proje");
        std::fs::write(root.join("README.md"), b"zararsiz gorunuyor").expect("dosya");

        assert_eq!(
            refuse(&root, "README.md"),
            SandboxViolation::Blocklisted(BlockReason::SensitiveDirectory)
        );
    }

    // -----------------------------------------------------------------------
    // Kotu yol 20-21: symlink kacisi
    // -----------------------------------------------------------------------

    /// **Kabul kriteri**: kok icindeki bir bag disariyi gosteriyorsa reddedilir.
    #[cfg(unix)]
    #[test]
    fn case_20_a_symlink_to_an_outside_directory_is_refused() {
        let temp = TempDir::new("t20");
        let root = temp.dir("proje");
        let outside = temp.dir("disarisi");
        std::fs::write(outside.join("gizli.txt"), b"sir").expect("dosya");
        std::os::unix::fs::symlink(&outside, root.join("kacis")).expect("symlink");

        assert_eq!(
            refuse(&root, "kacis/gizli.txt"),
            SandboxViolation::SymlinkEscape
        );
    }

    #[cfg(unix)]
    #[test]
    fn case_21_a_symlink_to_an_outside_file_is_refused() {
        let temp = TempDir::new("t21");
        let root = temp.dir("proje");
        let secret = temp.file("disarisi/anahtar.txt", b"sir");
        std::os::unix::fs::symlink(&secret, root.join("notlar.md")).expect("symlink");

        assert_eq!(refuse(&root, "notlar.md"), SandboxViolation::SymlinkEscape);
    }

    // -----------------------------------------------------------------------
    // Kotu yol 22-24: boyut, ikili, var olmayan dosya
    // -----------------------------------------------------------------------

    #[test]
    fn case_22_a_file_over_the_ceiling_is_refused_not_truncated() {
        let (_temp, root) = project("t22");
        let size = (MAX_READABLE_FILE_BYTES + 1) as usize;
        std::fs::write(root.join("dump.log"), vec![b'a'; size]).expect("dosya");

        let resolved = resolve_in_root(&root, "dump.log").expect("yol gecerli");
        assert_eq!(
            read_text(&resolved).expect_err("reddedilmeli"),
            SandboxViolation::TooLarge {
                size_bytes: MAX_READABLE_FILE_BYTES + 1,
                limit_bytes: MAX_READABLE_FILE_BYTES,
            }
        );

        // Tavanin tam ustundeki dosya gecmeli — sinir kapali araliktir.
        std::fs::write(
            root.join("tam.log"),
            vec![b'a'; MAX_READABLE_FILE_BYTES as usize],
        )
        .expect("dosya");
        let edge = resolve_in_root(&root, "tam.log").expect("yol gecerli");
        assert_eq!(
            read_text(&edge).expect("okunmali").size_bytes,
            MAX_READABLE_FILE_BYTES
        );
    }

    #[test]
    fn case_23_a_binary_file_is_refused() {
        let (_temp, root) = project("t23");
        // Mach-O benzeri bir on ek + NUL dolgusu.
        let mut bytes = vec![0xCF, 0xFA, 0xED, 0xFE];
        bytes.extend(std::iter::repeat_n(0u8, 4096));
        std::fs::write(root.join("asuna.bin"), &bytes).expect("dosya");

        let resolved = resolve_in_root(&root, "asuna.bin").expect("yol gecerli");
        assert_eq!(
            read_text(&resolved).expect_err("reddedilmeli"),
            SandboxViolation::Binary
        );
    }

    /// Var olmayan dosyada **icerik uydurulmaz** ve bos metin donmez.
    #[test]
    fn case_24_a_missing_file_reports_not_found_rather_than_empty_content() {
        let (_temp, root) = project("t24");
        let resolved = resolve_in_root(&root, "docs/yok.md").expect("yol gecerli");
        assert_eq!(
            read_text(&resolved).expect_err("reddedilmeli"),
            SandboxViolation::NotFound
        );
    }

    #[test]
    fn case_25_a_directory_target_is_not_a_file() {
        let (_temp, root) = project("t25");
        std::fs::create_dir_all(root.join("docs")).expect("dizin");

        let resolved = resolve_in_root(&root, "docs").expect("yol gecerli");
        assert_eq!(
            read_text(&resolved).expect_err("reddedilmeli"),
            SandboxViolation::NotAFile
        );
    }

    // -----------------------------------------------------------------------
    // Kotu yol 26: percent-encoding — decode EDILMEZ
    // -----------------------------------------------------------------------

    /// Karar (modul dokumantasyonu): `%2F` cozulmez. Ham metin **tek bir dosya
    /// adi** olur; kacis gerceklesmez, yalnizca anlamsizlasir.
    #[test]
    fn case_26_percent_encoded_separators_are_not_decoded() {
        let (_temp, root) = project("t26");

        for raw in [
            "..%2F..%2F.ssh%2Fid_ed25519",
            "%2Fetc%2Fpasswd",
            "%2e%2e%2f%2e%2e%2fetc%2fpasswd",
        ] {
            let resolved = resolve_in_root(&root, raw).expect("kok icinde bir ad olarak kalmali");
            assert!(
                resolved.as_path().starts_with(resolved.root()),
                "kok disina cikti: {raw}"
            );
            assert!(
                !resolved.as_path().to_string_lossy().contains("/etc/passwd"),
                "decode edilmis: {raw}"
            );
            // Boyle bir dosya yok — okuma durust bir sekilde duser.
            assert_eq!(
                read_text(&resolved).expect_err("okunmamali"),
                SandboxViolation::NotFound
            );
        }
    }

    // -----------------------------------------------------------------------
    // Kotu yol 27-30: kayit durumu (registry sandbox'in tek kaynagi)
    // -----------------------------------------------------------------------

    fn db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB")
    }

    fn register(db: &AsunaDb, root: &Path) -> String {
        let text = root.to_str().expect("UTF-8 yol");
        match registry::add(db, text, None, NOW).expect("eklenmeli") {
            ProjectAddOutcome::Registered { project } => project.id,
            ProjectAddOutcome::AlreadyRegistered { project } => project.id,
        }
    }

    #[test]
    fn case_27_an_unregistered_project_id_grants_nothing() {
        let db = db();
        assert_eq!(
            resolve_in_project(&db, "hic-kaydedilmemis", "README.md").expect_err("reddedilmeli"),
            SandboxViolation::NotRegistered
        );
    }

    /// Kok kayitli ama diskte yok: `missing` ile "hic kaydedilmemis" ayri
    /// cevaplar — kullaniciya sorulacak soru farkli ("disk takili mi?").
    #[test]
    fn case_28_a_missing_root_is_reported_as_missing_not_as_unregistered() {
        let (temp, root) = project("t28");
        let db = db();
        let id = register(&db, &root);

        std::fs::remove_dir_all(&root).expect("silinmeli");
        // `list` durumu tazeler (`active` -> `missing`).
        registry::list(&db, NOW).expect("listelenmeli");

        assert_eq!(
            resolve_in_project(&db, &id, "README.md").expect_err("reddedilmeli"),
            SandboxViolation::RootMissing
        );
        drop(temp);
    }

    /// Yolu olmayan bir **etiket** hicbir dizini acamaz (registry.rs sozlesmesi).
    #[test]
    fn case_29_an_unlinked_label_grants_no_filesystem_access() {
        let db = db();
        db.with_connection(|connection| {
            crate::db::project_repository::ensure_label(connection, "etiket", NOW)
        })
        .expect("etiket");

        assert_eq!(
            resolve_in_project(&db, "etiket", "README.md").expect_err("reddedilmeli"),
            SandboxViolation::NotRegistered
        );
    }

    /// Arsivlenmis proje: kullanicinin "burada calismiyorum" karari sandbox'ta
    /// da gecerli.
    #[test]
    fn case_30_an_archived_project_grants_no_access() {
        let (_temp, root) = project("t30");
        let db = db();
        let id = register(&db, &root);

        // Once erisim var.
        resolve_in_project(&db, &id, "README.md").expect("aktifken okunabilmeli");

        registry::update(
            &db,
            &id,
            &ProjectPatch {
                archived: Some(true),
                ..ProjectPatch::default()
            },
            NOW,
        )
        .expect("arsivlenmeli");

        assert_eq!(
            resolve_in_project(&db, &id, "README.md").expect_err("reddedilmeli"),
            SandboxViolation::NotRegistered
        );
    }

    /// Kayitli kok uzerinden traversal da ayni sekilde duser — DB yolu ile
    /// dogrudan yol ayni makineyi kullaniyor.
    #[test]
    fn case_31_traversal_through_a_registered_project_is_refused() {
        let (_temp, root) = project("t31");
        let db = db();
        let id = register(&db, &root);

        assert_eq!(
            resolve_in_project(&db, &id, "../../.ssh/id_ed25519").expect_err("reddedilmeli"),
            SandboxViolation::Traversal
        );
    }

    // -----------------------------------------------------------------------
    // Sozlesme testleri: ret sessiz degil, mesaj yol sizdirmiyor
    // -----------------------------------------------------------------------

    /// **Kabul kriteri**: reddedilen erisim sessizce bos donmez — audit satirina
    /// cevrilebilir tipli bir sonuc doner.
    #[test]
    fn every_violation_maps_to_a_not_requested_audit_row() {
        for violation in [
            SandboxViolation::NotRegistered,
            SandboxViolation::RootMissing,
            SandboxViolation::Empty,
            SandboxViolation::TooLong,
            SandboxViolation::NullByte,
            SandboxViolation::AbsolutePath,
            SandboxViolation::Tilde,
            SandboxViolation::Traversal,
            SandboxViolation::SymlinkEscape,
            SandboxViolation::Blocklisted(BlockReason::EnvironmentFile),
            SandboxViolation::NotAFile,
            SandboxViolation::NotFound,
            SandboxViolation::TooLarge {
                size_bytes: 999_999,
                limit_bytes: MAX_READABLE_FILE_BYTES,
            },
            SandboxViolation::Binary,
            SandboxViolation::Unreadable,
        ] {
            let outcome = violation.audit_outcome();

            assert_eq!(
                outcome.approval_state,
                ToolApprovalState::NotRequested,
                "{violation:?}"
            );
            assert!(
                !outcome.approval_state.permitted_execution(),
                "{violation:?} calistirilmis gorunmemeli"
            );
            assert!(
                outcome.result_summary.contains(violation.code()),
                "ozet kodu tasimali: {}",
                outcome.result_summary
            );
            assert!(
                outcome.result_summary.chars().count() <= MAX_RESULT_SUMMARY_CHARS,
                "ozet tavani asti: {}",
                outcome.result_summary
            );
        }
    }

    /// Ret mesajlari kullanicinin dizin yapisini tekrarlamaz.
    #[test]
    fn violation_messages_never_echo_a_path() {
        let temp = TempDir::new("no-echo");
        let root = temp.dir("cok-gizli-dizin");

        for raw in ["../../.ssh/id_ed25519", "/etc/passwd", "~/gizli", ".env"] {
            let violation = refuse(&root, raw);
            let message = violation.to_string();
            assert!(
                !message.contains("cok-gizli-dizin") && !message.contains('/'),
                "mesaj yol sizdirdi: {message}"
            );
            assert!(!violation.code().is_empty());
        }
    }

    /// Kacis denemesi ile "dosya yok" ayri siniflar: biri uyari, oteki durust
    /// bir cevap.
    #[test]
    fn escape_attempts_are_distinguishable_from_ordinary_refusals() {
        for violation in [
            SandboxViolation::AbsolutePath,
            SandboxViolation::Tilde,
            SandboxViolation::Traversal,
            SandboxViolation::SymlinkEscape,
        ] {
            assert!(violation.is_escape_attempt(), "{violation:?}");
        }
        for violation in [
            SandboxViolation::NotFound,
            SandboxViolation::NotAFile,
            SandboxViolation::Binary,
            SandboxViolation::Empty,
        ] {
            assert!(!violation.is_escape_attempt(), "{violation:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Ikili tespiti
    // -----------------------------------------------------------------------

    #[test]
    fn text_including_turkish_and_emoji_is_not_mistaken_for_binary() {
        for sample in [
            "".as_bytes(),
            "merhaba".as_bytes(),
            "Şeytan İbo — ğüçöş\n\tsatir\r\n".as_bytes(),
            "kod: fn main() { println!(\"👋\"); }\n".as_bytes(),
        ] {
            assert!(!looks_binary(sample), "metin ikili sayildi: {sample:?}");
        }
    }

    #[test]
    fn a_single_nul_byte_is_enough_evidence() {
        assert!(looks_binary(b"metin\0metin"));
    }

    #[test]
    fn a_high_control_byte_ratio_counts_as_binary() {
        let mut bytes = vec![b'a'; 100];
        // NUL kullanmadan, yalnizca oranla karar verildigini gosterir.
        for slot in bytes.iter_mut().take(20) {
            *slot = 0x01;
        }
        assert!(looks_binary(&bytes));

        let mut mild = vec![b'a'; 100];
        for slot in mild.iter_mut().take(5) {
            *slot = 0x01;
        }
        assert!(!looks_binary(&mild));
    }

    /// Depolama kapali/arizaliyken sandbox erisim vermez.
    #[test]
    fn a_disabled_database_grants_no_access() {
        assert!(DbState::Disabled.access().expect("durum").is_none());
    }
}

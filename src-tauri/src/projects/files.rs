//! `read_project_file` — kayitli proje koku icindeki **tek** dosya okuma yuzeyi
//! (ASU-051, PROJECT.md Bolum 17/19).
//!
//! # Neden Rust tarafinda
//!
//! Tool runner renderer'da yasiyor (voice.md Bolum 9) ve renderer guvenilmez.
//! Renderer burada yalnizca **kok'e gore gorece bir metin** verebilir; hangi
//! kokun kullanilacagini, yolun nasil cozulecegini, neyin blok listesinde
//! oldugunu ve ne kadarinin donecegini guven sinirinin **icindeki** bu modul
//! belirler. Cozum ve blok listesi zaten [`crate::security::sandbox`]'in isi;
//! bu modul onun uzerine yalnizca iki sey ekler:
//!
//! 1. **Kok secimi renderer'da degil.** Komut bir `project_id` **almaz**:
//!    okunacak kok her zaman kullanicinin sectigi guncel projedir
//!    (`registry::current`). Renderer kayitli projeler arasinda dolasarak
//!    dosya okuyamaz — `project_context` ile ayni kural.
//! 2. **Kirpma.** Sandbox 256 KiB'a kadar okur ve **kirpmaz**
//!    ([`sandbox::MAX_READABLE_FILE_BYTES`] dokumantasyonu: kirpma bir sunum
//!    karari, guvenlik katmaninin isi degil). Sunum karari burada veriliyor.
//!
//! # Cikti butcesi neden karakter, neden 6 000
//!
//! Donen metin **modele** gidiyor ve model sesli cevap uretiyor. Bayt degil
//! karakter sayiyoruz cunku butcenin gerceklestigi yer token/konusma tarafi.
//! 6 000, `context::MAX_TOTAL_CONTEXT_CHARS` ile **ayni** deger: proje ozeti
//! icin zaten kabul edilmis "modele bir seferde ne kadar metin gider" tavani.
//! Ikinci bir sayi uydurmak, ayni sorunun iki farkli cevabini uretirdi.
//! Sandbox tavaninin (256 KiB) cok altinda kalmasi bilincli — okuma reddi ile
//! kirpma birbirinin yerine gecmez.
//!
//! Kirpma **sessiz degil**: [`ProjectFileView::truncated`] doner ve gercek
//! dosya boyutu ([`ProjectFileView::size_bytes`]) yaninda durur, boylece Asuna
//! "dosyanin tamamini okudum" diyemez.
//!
//! # Icerik redaksiyonu
//!
//! Blok listesi `.env`, anahtar ve credential **dosyalarini** kapatir; ama
//! siradan bir kaynak dosyasinin icine gomulmus bir token'i kapatmaz. Donen
//! metin bu yuzden [`redaction::redact_sensitive_text`] suzgecinden gecer
//! (`asuna-config/security.md` Bolum 5 ile ayni suzgec). Bir sey maskelendiyse
//! [`ProjectFileView::redacted`] bunu **soyler**: sessizce degistirilmis bir
//! dosya icerigi, "gordugum sey bu muydu?" sorusunu cevapsiz birakirdi.
//!
//! # Ret sessiz degil ve tek tip degil
//!
//! Her ret tipli doner ve iki bilgiyi ayri ayri tasir:
//!
//! - [`ProjectFileError::code`] — makine tarafinin ayirt edecegi sabit kod
//!   (sandbox reddi icin dogrudan [`SandboxViolation::code`]).
//! - [`ProjectFileError::escape_attempt`] — "kacis denendi" mi yoksa siradan
//!   bir "yok/uygun degil" mi? Ikisi kullaniciya **ayni sekilde sunulmamali**:
//!   biri gorunur bir guvenlik uyarisi, oteki yalnizca durust bir cevap.
//!   Ozellikle `not_found` ile `traversal` ayni kovaya konsaydi model "dosya
//!   yok" diye baslayip icerik uydurmaya acik hale gelirdi.
//!
//! Hicbir hata mesaji yol, dizin yapisi ya da dosya icerigi tasimaz
//! (`sandbox::SandboxViolation` ile ayni kural).

use serde::Serialize;
use tauri::State;

use crate::db::{AsunaDb, DbState};
use crate::redaction;
use crate::security::sandbox::{self, SandboxViolation};

use super::registry::{self, RegistryError};

/// Modele donebilecek en fazla karakter (bkz. modul dokumantasyonu).
pub const MAX_PROJECT_FILE_CHARS: usize = 6_000;

// ---------------------------------------------------------------------------
// Cikti
// ---------------------------------------------------------------------------

/// Okunmus proje dosyasi.
///
/// GIZLILIK: [`Self::path`] **kok'e gore** yoldur; mutlak yol donmez, yani
/// kullanicinin dizin yapisi ne modele ne `tool_events` satirina girer
/// ([`sandbox::SandboxedPath::relative`] sozlesmesi).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileView {
    /// Okumayi saglayan kayitli proje.
    pub project_id: String,
    pub project_name: String,
    /// Kok'e gore, cozulmus yol (`docs/architecture/tools.md`).
    pub path: String,
    /// Dosya metni — kirpilmis ve redakte edilmis **olabilir**; hangisinin
    /// oldugu asagidaki bayraklarda yazili.
    pub content: String,
    /// Butce asildi: [`Self::content`] dosyanin **basi**.
    pub truncated: bool,
    /// Icerikte en az bir deger maskelendi.
    pub redacted: bool,
    /// Diskteki gercek dosya boyutu.
    pub size_bytes: u64,
    /// Donen metnin karakter sayisi (olculen, tahmin degil).
    pub returned_chars: usize,
    /// Uygulanan tavanin kendisi de gorunur — kirpmanin nedeni sorulabilsin.
    pub max_chars: usize,
}

// ---------------------------------------------------------------------------
// Ret
// ---------------------------------------------------------------------------

/// `read_project_file` reddi.
///
/// GIZLILIK: hicbir varyantin mesaji yol ya da dosya icerigi tasimaz.
#[derive(Debug, thiserror::Error)]
pub enum ProjectFileError {
    /// Sandbox reddetti. Gerekce **oldugu gibi** tasinir; burada yeniden
    /// yorumlanmaz.
    #[error("{0}")]
    Denied(#[from] SandboxViolation),

    /// Guncel proje secilmemis ya da hic proje kayitli degil. Bu bir hata
    /// degil bir **bilgi eksigi**: model dosya adi ya da icerik uydurmak yerine
    /// kullaniciya sormali.
    #[error("guncel proje secilmemis; once hangi projede calisildigi belirlenmeli")]
    NoCurrentProject,

    /// Kalici depolama kapali — kayitli kok listesi okunamiyor, dolayisiyla
    /// hicbir dosya sandbox'tan gecemez.
    #[error("kalici depolama kapali; kayitli proje kokleri okunamiyor")]
    Disabled,

    #[error("hafiza kullanilamiyor: {reason}")]
    Unavailable { reason: String },

    #[error("veritabani islemi basarisiz")]
    Storage,
}

impl From<RegistryError> for ProjectFileError {
    fn from(value: RegistryError) -> Self {
        match value {
            RegistryError::Disabled => Self::Disabled,
            RegistryError::Unavailable { reason } => Self::Unavailable { reason },
            // Kalan varyantlar bu yolda uretilmiyor (yalnizca `current` cagriliyor);
            // yine de sessizce "dosya yok"a dusurulmuyor.
            _ => Self::Storage,
        }
    }
}

impl ProjectFileError {
    /// Makine tarafinin ayirt etmesi icin sabit kod.
    ///
    /// Sandbox reddi kendi kodunu tasir (`traversal`, `blocklisted`,
    /// `not_found`, ...): kodu burada yeniden etiketlemek, iki katmanin ayni
    /// seye iki isim vermesi olurdu. Kumeler kesismiyor.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Denied(violation) => violation.code(),
            Self::NoCurrentProject => "no_current_project",
            Self::Disabled => "disabled",
            Self::Unavailable { .. } => "unavailable",
            Self::Storage => "storage",
        }
    }

    /// Bu ret bir **kacis denemesi** miydi?
    ///
    /// `false` demek "sorun yok" demek degil; "bu bir guvenlik olayi degil,
    /// siradan bir yok/uygun degil durumu" demek. Cagiran taraf ikisini ayri
    /// sunar (`read-project-file.ts`).
    pub fn escape_attempt(&self) -> bool {
        match self {
            Self::Denied(violation) => violation.is_escape_attempt(),
            _ => false,
        }
    }

    /// `tool_events.result_summary` alanina yazilacak tek satirlik ozet.
    ///
    /// Sandbox reddinde [`SandboxViolation::audit_outcome`] sozlesmesinden
    /// gelir (ASU-049'un ASU-051 icin biraktigi entegrasyon noktasi); diger
    /// yollarda ayni bicimde uretilir. Her zaman dolu: reddedilen bir erisim
    /// deftere **bos ozetle** dusmez.
    pub fn audit_summary(&self) -> String {
        match self {
            Self::Denied(violation) => violation.audit_outcome().result_summary,
            other => format!("reddedildi ({}): {other}", other.code()),
        }
    }
}

impl Serialize for ProjectFileError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            code: &'a str,
            message: &'a str,
            /// Kacis denemesi mi? Renderer bunu **hesaplamaz**, host soyler.
            escape_attempt: bool,
            /// Audit satirina yazilacak, redaksiyondan gecmis tek satirlik ozet.
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
// Okuma
// ---------------------------------------------------------------------------

/// Guncel projenin icindeki bir dosyayi okur.
///
/// Saf(ish) is fonksiyonu: komut yalnizca `State` cozer ve buraya devreder,
/// boylece tum yol gercek dizinler uzerinde, Tauri kurmadan test edilebilir.
pub fn read(db: &AsunaDb, relative: &str) -> Result<ProjectFileView, ProjectFileError> {
    let project = registry::current(db)?.ok_or(ProjectFileError::NoCurrentProject)?;

    // Cozum ve blok listesi sandbox'ta; burada yeniden yorumlanmiyor.
    let path = sandbox::resolve_in_project(db, &project.id, relative)?;
    let file = sandbox::read_text(&path)?;

    // Once redaksiyon, sonra kirpma: tersi olsaydi tavana denk gelen bir secret
    // yarim gorunebilirdi.
    let redacted_text = redaction::redact_sensitive_text(&file.text);
    let redacted = redacted_text != file.text;

    let returned_chars = redacted_text.chars().count();
    let truncated = returned_chars > MAX_PROJECT_FILE_CHARS;
    let content: String = if truncated {
        redacted_text.chars().take(MAX_PROJECT_FILE_CHARS).collect()
    } else {
        redacted_text
    };

    Ok(ProjectFileView {
        project_id: project.id,
        project_name: project.name,
        path: path.relative().to_owned(),
        returned_chars: content.chars().count(),
        content,
        truncated,
        redacted,
        size_bytes: file.size_bytes,
        max_chars: MAX_PROJECT_FILE_CHARS,
    })
}

/// Guncel proje koku icindeki bir dosyayi metin olarak okur (ASU-051).
///
/// Renderer yalnizca **kok'e gore gorece bir yol** verebilir: ne projeyi ne de
/// mutlak bir yolu secebilir. Mutlak yol, `~` ve `..` ile disari cikma girisimi
/// tipli olarak reddedilir; `.env`, SSH anahtari ve credential dosyalari blok
/// listesindedir.
#[tauri::command]
pub fn read_project_file(
    state: State<'_, DbState>,
    path: String,
) -> Result<ProjectFileView, ProjectFileError> {
    let db = registry::database(&state)?;
    read(db, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::{Path, PathBuf};

    use crate::db::clock;
    use crate::projects::registry::ProjectAddOutcome;

    const NOW: &str = "2026-08-25T10:00:00Z";

    /// Izole gecici dizin — gercek uygulama veri dizinine **asla** dokunmaz.
    /// `security::sandbox` testleriyle ayni desen: sahte filesystem yok, gercek
    /// dizin ve gercek `canonicalize` davranisi olculuyor.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asuna-files-{label}-{}-{:?}",
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

    /// Gercek bir dizin + gercek bir kayit + "guncel proje" secimi.
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
        let target = fixture.root.path().join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("dizin olusturulmali");
        }
        std::fs::write(target, contents).expect("dosya yazilmali");
    }

    fn canonical_root(fixture: &Fixture) -> String {
        std::fs::canonicalize(fixture.root.path())
            .expect("canonicalize")
            .to_string_lossy()
            .into_owned()
    }

    // --- Mutlu yol ----------------------------------------------------------

    #[test]
    fn reads_a_text_file_from_the_current_project_root() {
        let fixture = fixture("happy");
        write(&fixture, "README.md", "# Asuna\n\nSesli companion.\n");

        let view = read(&fixture.db, "README.md").expect("okunmali");

        assert_eq!(view.path, "README.md");
        assert_eq!(view.content, "# Asuna\n\nSesli companion.\n");
        assert!(!view.truncated);
        assert!(!view.redacted);
        assert_eq!(view.size_bytes, 26);
        assert_eq!(view.max_chars, MAX_PROJECT_FILE_CHARS);
        assert_eq!(view.project_name, "Deneme");
    }

    /// Donen yol **gorece**: mutlak yol modele de audit'e de girmez.
    #[test]
    fn the_returned_path_is_relative_to_the_root() {
        let fixture = fixture("relative");
        write(&fixture, "docs/architecture/voice.md", "ses");

        let view = read(&fixture.db, "./docs/architecture/voice.md").expect("okunmali");

        assert_eq!(view.path, "docs/architecture/voice.md");
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(
            !json.contains(&canonical_root(&fixture)),
            "mutlak yol ciktiya sizdi: {json}"
        );
    }

    // --- Kirpma -------------------------------------------------------------

    #[test]
    fn an_oversized_file_is_clipped_and_says_so() {
        let fixture = fixture("clip");
        let body = "a".repeat(MAX_PROJECT_FILE_CHARS + 500);
        write(&fixture, "notes.md", &body);

        let view = read(&fixture.db, "notes.md").expect("okunmali");

        assert!(view.truncated, "kirpma bildirilmedi");
        assert_eq!(view.returned_chars, MAX_PROJECT_FILE_CHARS);
        assert_eq!(view.content.chars().count(), MAX_PROJECT_FILE_CHARS);
        // Gercek boyut kaybolmuyor: "ne kadarini gordum?" cevaplanabilir.
        assert_eq!(view.size_bytes, (MAX_PROJECT_FILE_CHARS + 500) as u64);
    }

    /// Tavana **tam** oturan dosya kirpilmis sayilmaz — sinirda uydurma yok.
    #[test]
    fn a_file_exactly_at_the_budget_is_not_marked_truncated() {
        let fixture = fixture("exact");
        write(&fixture, "exact.md", &"b".repeat(MAX_PROJECT_FILE_CHARS));

        let view = read(&fixture.db, "exact.md").expect("okunmali");

        assert!(!view.truncated);
        assert_eq!(view.returned_chars, MAX_PROJECT_FILE_CHARS);
    }

    /// Kirpma **karakter** sinirinda: cok baytli metin ortasindan bolunup
    /// bozuk cikmaz.
    #[test]
    fn clipping_happens_on_a_character_boundary() {
        let fixture = fixture("utf8");
        write(&fixture, "tr.md", &"gusioc".repeat(MAX_PROJECT_FILE_CHARS));

        let view = read(&fixture.db, "tr.md").expect("okunmali");

        assert!(view.truncated);
        assert_eq!(view.content.chars().count(), MAX_PROJECT_FILE_CHARS);
    }

    /// Butce sandbox tavaninin **altinda** kalmali (ASU-049 sozlesmesi):
    /// kirpma ile red birbirinin yerine gecmez.
    #[test]
    fn the_clipping_budget_stays_below_the_sandbox_read_limit() {
        assert!(
            (MAX_PROJECT_FILE_CHARS as u64) < sandbox::MAX_READABLE_FILE_BYTES,
            "kirpma butcesi sandbox tavanina ulasti"
        );
    }

    // --- Redaksiyon ---------------------------------------------------------

    #[test]
    fn secrets_inside_an_allowed_file_are_masked_and_reported() {
        let fixture = fixture("redact");
        write(
            &fixture,
            "config.ts",
            "const key = \"sk-proj-BU-DEGER-SIZMAMALI\";\n",
        );

        let view = read(&fixture.db, "config.ts").expect("okunmali");

        assert!(
            !view.content.contains("BU-DEGER-SIZMAMALI"),
            "secret modele sizdi: {}",
            view.content
        );
        assert!(view.redacted, "maskeleme sessizce yapildi");
    }

    // --- Ret yollari --------------------------------------------------------

    /// **ASU-051 kabul kriteri**: var olmayan dosyada icerik uydurulmaz ve bu
    /// bir kacis denemesi olarak etiketlenmez.
    #[test]
    fn a_missing_file_is_not_found_and_not_an_escape_attempt() {
        let fixture = fixture("missing");

        let error = read(&fixture.db, "YOK.md").expect_err("hata bekleniyordu");

        assert_eq!(error.code(), "not_found");
        assert!(!error.escape_attempt());
        assert!(error.audit_summary().starts_with("reddedildi (not_found)"));
    }

    /// Kacis denemesi "dosya yok"tan **ayri** kodla doner.
    #[test]
    fn an_escape_attempt_is_reported_separately_from_a_missing_file() {
        let fixture = fixture("escape");

        for (relative, expected) in [
            ("../../.ssh/id_ed25519", "traversal"),
            ("/etc/passwd", "absolute"),
            ("~/.ssh/id_ed25519", "tilde"),
        ] {
            let error = read(&fixture.db, relative).expect_err("hata bekleniyordu");
            assert_eq!(error.code(), expected, "yol: {relative}");
            assert!(error.escape_attempt(), "yol: {relative}");
        }
    }

    /// **ASU-055 kabul kriteri**: `.env` okunmaz ve icerigi sizmaz.
    #[test]
    fn blocklisted_files_are_refused_without_leaking_their_contents() {
        let fixture = fixture("blocklist");
        write(&fixture, ".env", "OPENAI_API_KEY=sk-proj-SIZMAMALI\n");

        let error = read(&fixture.db, ".env").expect_err("hata bekleniyordu");

        assert_eq!(error.code(), "blocklisted");
        // Blok listesi bir kacis denemesi degil: kullanici kendi dosyasini
        // istedi, biz vermedik. Ikisi ayni uyariyla sunulmamali.
        assert!(!error.escape_attempt());
        let json = serde_json::to_string(&error).expect("serialize");
        assert!(!json.contains("SIZMAMALI"), "icerik sizdi: {json}");
        assert!(
            !json.contains("OPENAI_API_KEY"),
            "anahtar adi sizdi: {json}"
        );
    }

    /// Hicbir ret mesaji kullanicinin dizin yapisini tekrarlamaz.
    #[test]
    fn no_refusal_message_carries_a_path() {
        let fixture = fixture("nopath");
        let absolute = canonical_root(&fixture);

        for relative in ["YOK.md", "../disari", ".env", "/etc/passwd"] {
            let error = read(&fixture.db, relative).expect_err("hata bekleniyordu");
            let json = serde_json::to_string(&error).expect("serialize");
            assert!(!json.contains(&absolute), "mutlak yol sizdi: {json}");
            // Blok listesi mesaji girdiyi degil **kategoriyi** adlandirir
            // ("ortam degiskeni dosyasi (.env) okunmaz"); kural sabit metin,
            // kullanicinin yazdigi yolun kopyasi degil. Digerlerinde girdi hic
            // gorunmemeli.
            if relative != ".env" {
                assert!(
                    !json.contains(relative),
                    "verilen yol mesaja yazildi: {json}"
                );
            }
        }
    }

    /// **Uydurma yok**: proje secilmemisken cevap "dosya yok" degil, "hangi
    /// projede oldugumu bilmiyorum".
    #[test]
    fn reading_without_a_current_project_is_its_own_error() {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");

        let error = read(&db, "README.md").expect_err("hata bekleniyordu");

        assert_eq!(error.code(), "no_current_project");
        assert!(!error.escape_attempt());
    }

    /// Dizin bir dosya hedefi degildir; bos icerik donmez.
    #[test]
    fn a_directory_target_is_refused_instead_of_returning_empty_content() {
        let fixture = fixture("dir");
        std::fs::create_dir_all(fixture.root.path().join("docs")).expect("dizin");

        let error = read(&fixture.db, "docs").expect_err("hata bekleniyordu");

        assert_eq!(error.code(), "not_a_file");
    }

    /// Ikili dosya modele ham bayt olarak gitmez.
    #[test]
    fn a_binary_file_is_refused() {
        let fixture = fixture("binary");
        std::fs::write(
            fixture.root.path().join("logo.png"),
            [0x89, b'P', b'N', b'G', 0x00, 0x01, 0x02, 0x03],
        )
        .expect("dosya yazilmali");

        let error = read(&fixture.db, "logo.png").expect_err("hata bekleniyordu");

        assert_eq!(error.code(), "binary");
    }

    /// Wire bicimi sabit: renderer sozlesmesi bu dort alandan olusur.
    #[test]
    fn the_error_wire_format_is_stable() {
        let fixture = fixture("wire");
        let error = read(&fixture.db, "YOK.md").expect_err("hata bekleniyordu");

        let json = serde_json::to_value(&error).expect("serialize");
        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["auditSummary", "code", "escapeAttempt", "message"]);
        assert_eq!(json["escapeAttempt"], false);
    }

    /// Kok diskten kaybolursa erisim **acik kalmaz**: sandbox yalnizca `active`
    /// kayitlara izin verir ve durum tazelemesi kaydi `missing` yapar.
    #[test]
    fn a_vanished_project_root_grants_no_access() {
        let fixture = fixture("vanished");
        write(&fixture, "README.md", "icerik");

        std::fs::remove_dir_all(fixture.root.path()).expect("dizin silinmeli");
        registry::list(&fixture.db, &clock::now_utc()).expect("liste tazelenmeli");

        let error = read(&fixture.db, "README.md").expect_err("hata bekleniyordu");
        assert_eq!(error.code(), "root_missing");
    }
}

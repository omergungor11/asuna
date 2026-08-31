//! `open_project` — kayitli bir projeyi konfigure edilmis editorde acar
//! (ASU-052, PROJECT.md Bolum 32 Phase 5).
//!
//! Asuna'nin **ilk yan etkili** aksiyonu: risk 1 (geri alinabilir dusuk risk),
//! `safe` modda acik onay ister (ASU-048 matrisi). Onay akisi renderer'da;
//! bu modul yalnizca onaylanmis bir cagriyi yurutur.
//!
//! # Shell yok, string birlestirme yok
//!
//! Alt process [`std::process::Command`] ile, **arguman vektoru** olarak
//! kurulur: `Command::new(editor).arg(path)`. Hicbir yerde `sh -c` yoktur ve
//! komut metni kurulmaz. Sonuc: proje dizininin adi `; rm -rf ~` olsa bile
//! tek bir arguman olarak gecer, yorumlanmaz (test:
//! `the_project_path_is_passed_as_a_single_argument_not_a_shell_string`).
//!
//! Editor komutunun **kendisi** de yorumlanmaz: `ASUNA_EDITOR_COMMAND` bosluk
//! ya da kabuk metakarakteri iceremez ([`crate::config`] dogrulamasi), yani
//! `code --wait` gibi bir deger acilista reddedilir — sessizce "code --wait"
//! adinda bir dosya aranmaz.
//!
//! # Cocuga ne miras kaliyor
//!
//! `OPENAI_API_KEY` process environment'inda **yoksa** zaten miras kalmaz:
//! Asuna kendi `.env` okuyucusunu kullanir ve `std::env::set_var` cagirmaz
//! (`docs/architecture/security.md` — "dotenvy yok" karari, tam olarak bu
//! senaryo icin alinmisti). Kullanici anahtari kendi kabugunda export etmisse
//! degeri process'e girmis olur; bu durumda da cocuga gecmemesi icin
//! [`std::process::Command::env_remove`] ile acikca siliniyor. Editor'un
//! Asuna'nin faturasina erisebilecek bir anahtarla acilmasi icin hicbir sebep
//! yok.
//!
//! Cocugun stdio'su `null`: editor Asuna'nin log akisina yazamaz.
//!
//! # Yalnizca kayitli, `active` bir kok acilir
//!
//! Komut bir yol **almaz**; hedef kullanicinin sectigi guncel projedir
//! (`registry::current`). Keyfi bir dizin acmanin yolu yok — kok listesinin
//! tek kaynagi registry (`registry.rs` sozlesmesi).
//!
//! # Durust hata (PROJECT.md Bolum 30)
//!
//! Editor komutu bulunamazsa cikti "actim" degil: tipli
//! [`EditorError::EditorNotFound`] doner ve mesaj hangi komutun aranip
//! bulunamadigini soyler. Model bunu oldugu gibi aktarmali.

use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;
use tauri::State;

use crate::config::{AsunaConfig, KEY_OPENAI_API_KEY};
use crate::db::clock;
use crate::db::model::ProjectStatus;
use crate::db::{project_repository, AsunaDb, DbState};

use super::registry::{self, RegistryError};

// ---------------------------------------------------------------------------
// Cikti
// ---------------------------------------------------------------------------

/// Proje editorde acildi.
///
/// GIZLILIK: yol **donmez**. Kullanicinin dizin yapisi modele ve
/// `tool_events` satirina girmez; proje adi ve kimligi yeterli.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOpenOutcome {
    pub project_id: String,
    pub project_name: String,
    /// Calistirilan editor komutu (`code`). Secret degil, kullanicinin kendi
    /// ayari — hangi programin acildigini gormek denetimin parcasi.
    pub editor: String,
    /// Guncellenen `last_opened_at` degeri (UTC ISO-8601).
    pub opened_at: String,
}

// ---------------------------------------------------------------------------
// Ret
// ---------------------------------------------------------------------------

/// `open_project` reddi.
///
/// GIZLILIK: hicbir varyantin mesaji yol tasimaz (`RegistryError` ile ayni
/// kural). Editor komutunun **adi** tasinir — kullanicinin kendi ayari ve
/// "hangi komut bulunamadi?" sorusunun cevabi o.
#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    /// Guncel proje secilmemis ya da hic proje kayitli degil.
    #[error("guncel proje secilmemis; once hangi projede calisildigi belirlenmeli")]
    NoCurrentProject,

    /// Kayit var ama acilabilir degil: yalnizca bir etiket (`unlinked`),
    /// arsivlenmis ya da kok su an diskte yok.
    #[error("{detail}")]
    NotOpenable { detail: String },

    /// Konfigure edilen editor komutu `PATH` uzerinde bulunamadi.
    ///
    /// PROJECT.md Bolum 30'un ornek cumlesi bu varyanttan uretilir: "Projeyi
    /// acmayi denedim ama VS Code komutu bulunamadi."
    #[error("`{command}` komutu bulunamadi; editor acilamadi")]
    EditorNotFound { command: String },

    /// Komut bulundu ama baslatilamadi (izin, calistirilabilir degil, ...).
    #[error("`{command}` calistirilamadi: {detail}")]
    LaunchFailed { command: String, detail: String },

    #[error("kalici depolama kapali; kayitli proje kokleri okunamiyor")]
    Disabled,

    #[error("hafiza kullanilamiyor: {reason}")]
    Unavailable { reason: String },

    #[error("veritabani islemi basarisiz")]
    Storage,
}

impl From<RegistryError> for EditorError {
    fn from(value: RegistryError) -> Self {
        match value {
            RegistryError::Disabled => Self::Disabled,
            RegistryError::Unavailable { reason } => Self::Unavailable { reason },
            _ => Self::Storage,
        }
    }
}

impl EditorError {
    /// Makine tarafinin ayirt etmesi icin sabit kod.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoCurrentProject => "no_current_project",
            Self::NotOpenable { .. } => "not_openable",
            Self::EditorNotFound { .. } => "editor_not_found",
            Self::LaunchFailed { .. } => "launch_failed",
            Self::Disabled => "disabled",
            Self::Unavailable { .. } => "unavailable",
            Self::Storage => "storage",
        }
    }

    /// `tool_events.result_summary` alanina yazilacak tek satirlik ozet.
    ///
    /// Her ret yolunda dolu: acilmayan bir proje deftere **bos ozetle**
    /// dusmez (ASU-052 kabul kriteri "basari/hatasi `tool_events`'e yaziliyor").
    pub fn audit_summary(&self) -> String {
        format!("acilmadi ({}): {self}", self.code())
    }
}

impl Serialize for EditorError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            code: &'a str,
            message: &'a str,
            audit_summary: &'a str,
        }

        let message = self.to_string();
        let audit_summary = self.audit_summary();
        Wire {
            code: self.code(),
            message: &message,
            audit_summary: &audit_summary,
        }
        .serialize(serializer)
    }
}

// ---------------------------------------------------------------------------
// Baslatma
// ---------------------------------------------------------------------------

/// Editoru **tek arguman** ile baslatir.
///
/// Ayri ve dar bir fonksiyon: enjeksiyon davranisi (yolun tek arguman olarak
/// gecmesi) veritabani ya da Tauri kurmadan, gercek bir alt process uzerinde
/// test edilebilsin diye.
///
/// Cikis kodu **beklenmez**: bir GUI editoru saatlerce acik kalir; `wait()`
/// cagirmak sesli oturumu kilitlerdi. "Baslatildi" ile "kullanici gordu" ayni
/// sey degil ve cikti bunu iddia etmiyor — [`ProjectOpenOutcome`] yalnizca
/// komutun basladigini soyler.
pub fn launch(command: &str, path: &Path) -> Result<(), EditorError> {
    // String birlestirme YOK: yol ayri bir arguman olarak gecer.
    let spawned = Command::new(command)
        .arg(path)
        // Kalici anahtar hicbir kosulda cocuga gecmez (modul dokumantasyonu).
        .env_remove(KEY_OPENAI_API_KEY)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match spawned {
        Ok(_child) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(EditorError::EditorNotFound {
                command: command.to_owned(),
            })
        }
        Err(error) => Err(EditorError::LaunchFailed {
            command: command.to_owned(),
            detail: error.kind().to_string(),
        }),
    }
}

/// Guncel projeyi verilen editor komutuyla acar ve `last_opened_at`i tazeler.
///
/// Sira onemli: **once** baslatma denenir, **sonra** kayit guncellenir. Tersi
/// olsaydi bulunamayan bir editorde "en son acilan proje" degismis olurdu —
/// olmayan bir olayin izi.
pub fn open(db: &AsunaDb, editor: &str, now: &str) -> Result<ProjectOpenOutcome, EditorError> {
    let project = registry::current(db)?.ok_or(EditorError::NoCurrentProject)?;

    if project.status != ProjectStatus::Active {
        return Err(EditorError::NotOpenable {
            detail: match project.status {
                ProjectStatus::Missing => "projenin kok dizini su an bulunamiyor".to_owned(),
                ProjectStatus::Archived => "proje arsivlenmis".to_owned(),
                ProjectStatus::Unlinked => {
                    "bu proje yalnizca bir etiket; kayitli bir kok dizini yok".to_owned()
                }
                ProjectStatus::Active => unreachable!("yukaridaki kosul zaten aktifi eliyor"),
            },
        });
    }

    let path = project.path.clone().ok_or(EditorError::NotOpenable {
        detail: "bu proje yalnizca bir etiket; kayitli bir kok dizini yok".to_owned(),
    })?;

    launch(editor, Path::new(&path))?;

    db.with_connection(|connection| {
        project_repository::touch_last_opened(connection, &project.id, now)
    })
    .map_err(|_| EditorError::Storage)?;

    Ok(ProjectOpenOutcome {
        project_id: project.id,
        project_name: project.name,
        editor: editor.to_owned(),
        opened_at: now.to_owned(),
    })
}

/// Guncel projeyi konfigure edilmis editorde acar (ASU-052, risk 1).
///
/// Renderer **hicbir sey secemez**: ne yol, ne proje, ne editor komutu. Hedef
/// registry'deki guncel projedir, komut `ASUNA_EDITOR_COMMAND`'dir ve ikisi de
/// guven sinirinin icinde cozulur.
#[tauri::command]
pub fn open_project(
    state: State<'_, DbState>,
    config: State<'_, AsunaConfig>,
) -> Result<ProjectOpenOutcome, EditorError> {
    let db = registry::database(&state)?;
    open(db, &config.editor_command, &clock::now_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use crate::projects::registry::ProjectAddOutcome;

    const NOW: &str = "2026-08-25T10:00:00Z";

    /// Izole gecici dizin (`security::sandbox` testleriyle ayni desen).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asuna-editor-{label}-{}-{:?}",
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

    /// Argumanlarini bir dosyaya yazan sahte "editor". Gercek bir alt process:
    /// enjeksiyon davranisini taklit uzerinde degil, isletim sisteminin
    /// gercekten yaptigi sey uzerinde olcuyoruz.
    fn fake_editor(temp: &TempDir, log: &Path) -> PathBuf {
        let script = temp.path().join("fake-editor.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\" >> '{}'; done\n",
                log.display()
            ),
        )
        .expect("script yazilmali");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("calistirilabilir yapilmali");
        }

        script
    }

    /// Alt process'in argumanlari log'a yazmasini bekler (spawn asenkron).
    fn wait_for_log(log: &Path) -> String {
        for _ in 0..200 {
            if let Ok(text) = std::fs::read_to_string(log) {
                if !text.trim().is_empty() {
                    return text;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("sahte editor hicbir arguman yazmadi");
    }

    fn register(db: &AsunaDb, path: &Path) -> String {
        let outcome = registry::add(db, &path.to_string_lossy(), Some("Deneme"), NOW)
            .expect("proje kaydedilmeli");
        let project = match outcome {
            ProjectAddOutcome::Registered { project }
            | ProjectAddOutcome::AlreadyRegistered { project } => project,
        };
        registry::set_current(db, &project.id, NOW).expect("guncel proje secilmeli");
        project.id
    }

    // --- Enjeksiyon ---------------------------------------------------------

    /// **ASU-052 kabul kriteri**: shell enjeksiyonuna kapali.
    ///
    /// Dizin adi kabuk metakarakterleri iceriyor. Bir `sh -c` yolu olsaydi
    /// `rm` calisir ve kanit dosyasi silinirdi; arguman vektoru ile yol tek
    /// bir metin olarak gecer.
    #[test]
    fn the_project_path_is_passed_as_a_single_argument_not_a_shell_string() {
        let temp = TempDir::new("inject");
        let hostile = temp.path().join("proje; rm -rf $HOME && echo pwned");
        std::fs::create_dir_all(&hostile).expect("dizin");
        let canary = hostile.join("kanit.txt");
        std::fs::write(&canary, "duruyor").expect("kanit");

        let log = temp.path().join("argv.log");
        let editor = fake_editor(&temp, &log);

        launch(&editor.to_string_lossy(), &hostile).expect("baslatilmali");
        let written = wait_for_log(&log);

        // Tek satir = tek arguman; yol bosluk/`;` uzerinden bolunmedi.
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "yol birden fazla argumana bolundu: {lines:?}"
        );
        assert_eq!(lines[0], hostile.to_string_lossy());
        assert!(canary.exists(), "kanit dosyasi silindi — komut yorumlanmis");
    }

    // --- Durust hata --------------------------------------------------------

    /// **ASU-052 kabul kriteri**: bulunamayan editor icin durust hata.
    #[test]
    fn a_missing_editor_command_is_reported_honestly() {
        let temp = TempDir::new("missing-editor");

        let error =
            launch("asuna-boyle-bir-komut-yok", temp.path()).expect_err("hata bekleniyordu");

        assert_eq!(error.code(), "editor_not_found");
        assert!(
            error.to_string().contains("asuna-boyle-bir-komut-yok"),
            "mesaj hangi komutun bulunamadigini soylemiyor: {error}"
        );
        assert!(error
            .audit_summary()
            .starts_with("acilmadi (editor_not_found)"));
    }

    /// Calistirilabilir olmayan bir dosya "bulunamadi" ile karistirilmaz.
    #[cfg(unix)]
    #[test]
    fn a_non_executable_command_is_not_reported_as_missing() {
        let temp = TempDir::new("noexec");
        let script = temp.path().join("editor.sh");
        std::fs::write(&script, "#!/bin/sh\n").expect("dosya");

        let error = launch(&script.to_string_lossy(), temp.path()).expect_err("hata bekleniyordu");

        assert_eq!(error.code(), "launch_failed");
    }

    // --- Kayit yolu ---------------------------------------------------------

    #[test]
    fn opening_the_current_project_touches_last_opened_at() {
        let temp = TempDir::new("open");
        let root = temp.path().join("proje");
        std::fs::create_dir_all(&root).expect("dizin");
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let project_id = register(&db, &root);

        let log = temp.path().join("argv.log");
        let editor = fake_editor(&temp, &log);
        let later = "2026-08-26T09:30:00Z";

        let outcome = open(&db, &editor.to_string_lossy(), later).expect("acilmali");

        assert_eq!(outcome.project_id, project_id);
        assert_eq!(outcome.project_name, "Deneme");
        assert_eq!(outcome.opened_at, later);

        let record = project_repository::find_by_id(&db, &project_id)
            .expect("okunmali")
            .expect("kayit olmali");
        assert_eq!(record.last_opened_at.as_deref(), Some(later));

        // Gercekten baslatildi ve kok dizini aldi.
        let written = wait_for_log(&log);
        assert_eq!(
            written.trim(),
            std::fs::canonicalize(&root)
                .expect("canonicalize")
                .to_string_lossy()
        );
    }

    /// Editor bulunamazsa kayit **degismez**: olmayan bir olayin izi kalmaz.
    #[test]
    fn a_failed_launch_does_not_touch_last_opened_at() {
        let temp = TempDir::new("nolaunch");
        let root = temp.path().join("proje");
        std::fs::create_dir_all(&root).expect("dizin");
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let project_id = register(&db, &root);

        let before = project_repository::find_by_id(&db, &project_id)
            .expect("okunmali")
            .expect("kayit")
            .last_opened_at;

        let error = open(&db, "asuna-boyle-bir-komut-yok", "2026-08-26T09:30:00Z")
            .expect_err("hata bekleniyordu");
        assert_eq!(error.code(), "editor_not_found");

        let after = project_repository::find_by_id(&db, &project_id)
            .expect("okunmali")
            .expect("kayit")
            .last_opened_at;
        assert_eq!(before, after, "basarisiz acilis kaydi tazeledi");
    }

    /// **Uydurma yok**: proje secilmemisken "actim" degil, "hangi proje?".
    #[test]
    fn opening_without_a_current_project_is_its_own_error() {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");

        let error = open(&db, "code", NOW).expect_err("hata bekleniyordu");

        assert_eq!(error.code(), "no_current_project");
    }

    /// Kok diskten kaybolmussa acilmaz ve durum durustce soylenir.
    #[test]
    fn a_vanished_root_is_refused_before_any_process_is_started() {
        let temp = TempDir::new("vanished");
        let root = temp.path().join("proje");
        std::fs::create_dir_all(&root).expect("dizin");
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        register(&db, &root);
        std::fs::remove_dir_all(&root).expect("dizin silinmeli");
        registry::list(&db, &clock::now_utc()).expect("liste tazelenmeli");

        let error = open(&db, "asuna-boyle-bir-komut-yok", NOW).expect_err("hata bekleniyordu");

        // Editor komutu hic denenmedi: hata kok'ten, komuttan degil.
        assert_eq!(error.code(), "not_openable");
    }

    /// Wire bicimi sabit: renderer sozlesmesi bu uc alandan olusur ve yol
    /// tasimaz.
    #[test]
    fn the_error_wire_format_is_stable_and_carries_no_path() {
        let temp = TempDir::new("wire");
        let error =
            launch("asuna-boyle-bir-komut-yok", temp.path()).expect_err("hata bekleniyordu");

        let json = serde_json::to_value(&error).expect("serialize");
        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["auditSummary", "code", "message"]);
        assert!(
            !json
                .to_string()
                .contains(&temp.path().to_string_lossy().into_owned()),
            "yol hataya sizdi: {json}"
        );
    }
}

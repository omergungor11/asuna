//! Opsiyonel transcript persist (ASU-032).
//!
//! # Gizlilik sozlesmesi
//!
//! `ASUNA_TRANSCRIPT_STORAGE=false` iken **diske hicbir sey yazilmaz**: ne
//! dosya, ne dizin, ne bos bir yer tutucu. Bu bayrak bir "gorunurluk ayari"
//! degil, bir gizlilik garantisi (PROJECT.md Bolum 20, memory.md Bolum 5). Bu
//! yuzden karar [`persist_if_enabled`] icinde, **yazma yolunun onunde** durur ve
//! davranissal olarak test edilir (dizin sonrasinda gercekten bos mu).
//!
//! Renderer tarafinda **destekleyici** bir onlem daha var: Realtime oturumu
//! acilirken `audio.input.transcription` yalnizca transcript saklama aciksa
//! kurulur (voice.md Bolum 2, `src/asuna/agent/realtime-service.ts`). Bu bir
//! "ikinci kat garanti" degil: renderer'a guvenilmez ve asil garanti burada,
//! yazma yolunun onundeki kapidir. Renderer tarafi yalnizca gereksiz yere
//! transkripsiyon uretilmesini (ve maliyetini) onler.
//!
//! ASU-037 ile karar **iki** kaynaktan gelir ve ikisi de `&&` ile baglanir:
//! acilis degeri (cagiranin gecirdigi `enabled`) ve calisma zamani anahtari
//! ([`crate::privacy`]). Kullanici ayari oturum ortasinda kapatirsa yazma o
//! andan itibaren durur; yeniden baslatma gerekmez.
//!
//! # Bicim
//!
//! Oturum basina bir JSONL dosyasi: her satir bir replik
//! (`{"at":...,"role":"user","text":...}`). Neden JSONL: kullanici kendi
//! dosyasini `grep`leyebilmeli ve satir satir okuyabilmeli; kismi yazilmis bir
//! dosya bile ayristirilabilir kalir.
//!
//! Dosya izinleri `0600`, dizin `0700`: transcript kullanicinin en mahrem
//! verisidir, ayni makinedeki baska bir kullanici okuyamamali.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::DbError;

/// Uygulama veri dizini altindaki transcript dizini.
pub const TRANSCRIPT_DIR_NAME: &str = "transcripts";

/// Tek bir replik.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptLine {
    pub role: TranscriptRole,
    pub text: String,
    /// Repligin zamani; renderer vermezse dosyada da bulunmaz (uydurulmaz).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptRole {
    User,
    Assistant,
}

/// Bir oturumun transcript dosya adi.
pub fn transcript_file_name(session_id: i64) -> String {
    format!("session-{session_id}.jsonl")
}

/// Transcript dizinini **Rust tarafinda** cozer; renderer yol veremez.
pub fn transcript_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, DbError> {
    let dir = app.path().app_data_dir().map_err(DbError::DataDir)?;
    Ok(dir.join(TRANSCRIPT_DIR_NAME))
}

/// Transcript'i **yalnizca** ayar aciksa diske yazar.
///
/// - `enabled == false` → `Ok(None)`; dosya sistemi hic **acilmaz**.
/// - calisma zamani anahtari kapali → `Ok(None)` (ASU-037).
/// - `lines` bos → `Ok(None)`; bos bir dosya yaratmak yalnizca gurultudur.
/// - aksi halde `base_dir/session-<id>.jsonl` yazilir ve yolu donulur.
///
/// Yazma hatasi cagirana doner: oturum kaydinin kapanmasini **engellememeli**
/// ama sessizce yutulmamali da (cagiran taraf `transcript_path`'i bos birakip
/// hatayi log'lar).
pub fn persist_if_enabled(
    enabled: bool,
    base_dir: &Path,
    session_id: i64,
    lines: &[TranscriptLine],
) -> io::Result<Option<PathBuf>> {
    persist_with_runtime_switch(
        crate::privacy::process_transcript_storage(),
        enabled,
        base_dir,
        session_id,
        lines,
    )
}

/// [`persist_if_enabled`]'in test edilebilir govdesi.
///
/// Calisma zamani anahtari parametre olarak aliniyor cunku process genelindeki
/// durum ([`crate::privacy::install_process_state`]) geri alinamaz; onu bir
/// testte kapatmak ayni process'teki diger testleri etkilerdi. Kapali anahtarin
/// **davranisi** (diske hicbir sey yazilmamasi) boylece dogrudan olculebiliyor.
fn persist_with_runtime_switch(
    runtime_enabled: bool,
    enabled: bool,
    base_dir: &Path,
    session_id: i64,
    lines: &[TranscriptLine],
) -> io::Result<Option<PathBuf>> {
    if !enabled || !runtime_enabled || lines.is_empty() {
        return Ok(None);
    }

    create_private_dir(base_dir)?;

    let path = base_dir.join(transcript_file_name(session_id));
    let mut file = create_private_file(&path)?;

    for line in lines {
        let encoded = serde_json::to_string(line).map_err(io::Error::other)?;
        writeln!(file, "{encoded}")?;
    }
    file.sync_all()?;

    Ok(Some(path))
}

/// Dizini **yaratilis aninda** `0700` ile acar (Gate 3 / LOW-7).
///
/// Once yaratip sonra `chmod` etmek, iki islem arasinda dunyaya okunabilir bir
/// pencere birakiyordu. `DirBuilder::mode` bu pencereyi kapatir. Dizin zaten
/// varsa mode uygulanmaz — o durumda [`tighten_permissions`] devreye girer.
#[cfg(unix)]
fn create_private_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    tighten_permissions(path, 0o700)
}

#[cfg(not(unix))]
fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

/// Dosyayi **yaratilis aninda** `0600` ile acar (Gate 3 / LOW-8).
///
/// `File::create` + `chmod` yerine `OpenOptions::mode`: transcript kullanicinin
/// en mahrem verisi ve ilk `write`'tan onceki bir saniyelik gevsek izin bile
/// gereksiz bir risk. `truncate(true)`: ayni oturum yeniden yazilirsa dosya
/// buyumez, degisir.
#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<File> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // `mode` yalnizca **yeni** dosyaya uygulanir; onceki bir calismadan kalmis
    // gevsek izinli bir dosya yine sikilastirilir.
    tighten_permissions(path, 0o600)?;
    Ok(file)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> io::Result<File> {
    File::create(path)
}

// ---------------------------------------------------------------------------
// Silme (ASU-065)
// ---------------------------------------------------------------------------

/// Bir oturumun dokum dosyasina yapilan silme denemesinin sonucu (ASU-065).
///
/// Neden bir enum ve neden bu kadar cok varyant: "sildim" demek yalnizca dosya
/// gercekten gittiginde dogrudur. `bool` donmek uc ayri gercegi tek kelimeye
/// sikistirirdi — kayitta dosya yoktu, dosya zaten yoktu, dosyaya dokunmayi
/// **reddettik**. Ucu de kullaniciya farkli sey soyler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranscriptFileOutcome {
    /// Oturum kaydinda dokum yolu yoktu (`ASUNA_TRANSCRIPT_STORAGE=false`).
    NotRecorded,
    /// Dosya bulundu ve silindi.
    Deleted,
    /// Kayitta yol vardi ama dosya diskte yok (kullanici elle silmis olabilir).
    AlreadyGone,
    /// Yol sandbox disina cikiyor ya da dosya degil: **dokunulmadi**.
    Refused,
    /// Silme denendi, dosya sistemi izin vermedi. Hata log'lanir.
    Failed,
}

/// Kayitli dokum dosyasini siler — **yalnizca** sandbox icindeyse.
///
/// # Guvenlik (traversal guard)
///
/// Silinecek yol renderer'dan **gelmez**, `sessions.transcript_path`'ten okunur.
/// Yine de veritabani bozulmus/elle duzenlenmis olabilir: bir satir
/// `.../transcripts/../../.ssh/id_ed25519` gosteriyorsa o dosya silinmemeli.
/// Bu yuzden iki kosul birlikte aranir ve ikisi de saglanmazsa
/// [`TranscriptFileOutcome::Refused`] donulur:
///
/// 1. Yol, `base_dir` (`app_data_dir()/transcripts`) **altinda** olmali —
///    karsilastirma once `..`/`.` bilesenleri lexical olarak cozulerek yapilir
///    (dosya var olmayabilecegi icin `canonicalize` kullanilamaz).
/// 2. Dosya adi tam olarak bu oturumun adi olmali
///    ([`transcript_file_name`]) — bozuk bir satir baska bir oturumun dokumunu
///    silmeye yol acmasin.
///
/// Symlink'ler de reddedilir: dizinin icine yerlestirilmis bir link, kosul (1)
/// saglaniyor gibi gorunurken hedefi sandbox disinda olabilir.
pub fn delete_recorded_file(
    base_dir: &Path,
    session_id: i64,
    recorded_path: Option<&str>,
) -> TranscriptFileOutcome {
    let Some(recorded) = recorded_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return TranscriptFileOutcome::NotRecorded;
    };

    let Some(path) = resolve_inside(base_dir, session_id, Path::new(recorded)) else {
        // GIZLILIK: yol log'a girmiyor; oturum kimligi yeter.
        eprintln!("[asuna] Oturum {session_id} dokum yolu sandbox disinda, dosyaya dokunulmadi.");
        return TranscriptFileOutcome::Refused;
    };

    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => TranscriptFileOutcome::AlreadyGone,
        Err(error) => {
            eprintln!("[asuna] Dokum dosyasi okunamadi: {error}");
            TranscriptFileOutcome::Failed
        }
        Ok(metadata) if !metadata.file_type().is_file() => {
            eprintln!("[asuna] Oturum {session_id} dokum yolu duz dosya degil, dokunulmadi.");
            TranscriptFileOutcome::Refused
        }
        Ok(_) => match fs::remove_file(&path) {
            Ok(()) => TranscriptFileOutcome::Deleted,
            Err(error) => {
                eprintln!("[asuna] Dokum dosyasi silinemedi: {error}");
                TranscriptFileOutcome::Failed
            }
        },
    }
}

/// Kayitli yolu sandbox icinde dogrular. `None` = reddedildi.
fn resolve_inside(base_dir: &Path, session_id: i64, recorded: &Path) -> Option<PathBuf> {
    let base = normalize_lexically(base_dir);
    let candidate = normalize_lexically(recorded);

    if !candidate.starts_with(&base) {
        return None;
    }
    if candidate.file_name()? != std::ffi::OsStr::new(&transcript_file_name(session_id)) {
        return None;
    }
    Some(candidate)
}

/// `.` ve `..` bilesenlerini **dosya sistemine dokunmadan** cozer.
///
/// `canonicalize` kullanilamaz: silinecek dosya zaten silinmis olabilir ve o
/// durumda hata donerdi — yani "dosya yok" ile "yol kacis deniyor" ayni gorunurdu.
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Dizin temizliginin sonucu (ASU-065).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TranscriptPurge {
    /// Silinen dokum dosyasi sayisi.
    pub deleted: u32,
    /// Dizinde **birakilan** girdi sayisi: silinemeyen dosyalar ve Asuna'nin
    /// uretmedigi yabanci girdiler. Sifir degilse kullaniciya soylenir.
    pub remaining: u32,
}

/// `transcripts/` dizinindeki tum dokum dosyalarini siler.
///
/// Yalnizca **Asuna'nin urettigi** adlar silinir (`session-<id>.jsonl`):
/// kullanicinin bu dizine koydugu baska bir dosyayi silmek, istemedigi bir seyi
/// silmek olurdu. Bu tur girdiler `remaining` icinde sayilir ve dizin de
/// birakilir; dizin ancak tamamen bosaldiginda kaldirilir.
pub fn purge_directory(base_dir: &Path) -> TranscriptPurge {
    let mut purge = TranscriptPurge::default();

    let Ok(entries) = fs::read_dir(base_dir) else {
        // Dizin yok = temizlenecek bir sey yok (hata degil).
        return purge;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let is_transcript = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_transcript_file_name)
            && path.is_file();

        if !is_transcript {
            purge.remaining = purge.remaining.saturating_add(1);
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => purge.deleted = purge.deleted.saturating_add(1),
            Err(error) => {
                eprintln!("[asuna] Dokum dosyasi silinemedi: {error}");
                purge.remaining = purge.remaining.saturating_add(1);
            }
        }
    }

    if purge.remaining == 0 {
        // Bos dizin de kalmasin; basarisiz olursa onemli degil (sessiz degil,
        // yalnizca gurultusuz: dosyalar zaten gitti).
        let _ = fs::remove_dir(base_dir);
    }
    purge
}

/// Ad, Asuna'nin urettigi bir dokum dosyasi adi mi?
fn is_transcript_file_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("session-") else {
        return false;
    };
    let Some(digits) = rest.strip_suffix(".jsonl") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Izinler beklenenden gevsekse sikilastirir. Zaten dogruysa dosya sistemine
/// dokunmaz (gereksiz `chmod` yok).
#[cfg(unix)]
fn tighten_permissions(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let current = fs::metadata(path)?.permissions().mode() & 0o777;
    if current == mode {
        return Ok(());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gecici dizin — gercek uygulama veri dizinine **asla** dokunulmaz.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "asuna-transcript-test-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let path = std::env::temp_dir().join(unique);
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("gecici dizin olusturulabilmeli");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        /// Dizin agacindaki tum dosyalar (rekursif).
        fn files(&self) -> Vec<PathBuf> {
            fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
                let Ok(entries) = fs::read_dir(dir) else {
                    return;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path, found);
                    } else {
                        found.push(path);
                    }
                }
            }
            let mut found = Vec::new();
            walk(&self.0, &mut found);
            found
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn lines() -> Vec<TranscriptLine> {
        vec![
            TranscriptLine {
                role: TranscriptRole::User,
                text: "Wake word'u yerel tutuyoruz.".to_owned(),
                at: Some("2026-08-25T10:00:00Z".to_owned()),
            },
            TranscriptLine {
                role: TranscriptRole::Assistant,
                text: "Anladim, not ettim.".to_owned(),
                at: None,
            },
        ]
    }

    #[test]
    fn writes_one_jsonl_line_per_turn_named_after_the_session() {
        let temp = TempDir::new("write");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);

        let path = persist_if_enabled(true, &dir, 42, &lines())
            .expect("yazma basarili olmali")
            .expect("yol donmeli");

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("session-42.jsonl")
        );

        let content = fs::read_to_string(&path).expect("dosya okunabilmeli");
        let rows: Vec<&str> = content.lines().collect();
        assert_eq!(rows.len(), 2);

        let first: serde_json::Value = serde_json::from_str(rows[0]).expect("gecerli JSON");
        assert_eq!(first["role"], "user");
        assert_eq!(first["at"], "2026-08-25T10:00:00Z");

        // Zaman verilmediyse uydurulmaz: alan dosyada hic yok.
        let second: serde_json::Value = serde_json::from_str(rows[1]).expect("gecerli JSON");
        assert_eq!(second["role"], "assistant");
        assert!(second.get("at").is_none(), "zaman uydurulmus: {second}");
    }

    /// **ASU-032 kabul kriteri (gizlilik).** Ayar kapaliyken diske hicbir sey
    /// yazilmaz — bayrak testi degil, dosya sistemi testi.
    #[test]
    fn writes_absolutely_nothing_to_disk_when_storage_is_disabled() {
        let temp = TempDir::new("disabled");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);

        let result = persist_if_enabled(false, &dir, 42, &lines()).expect("hata olmamali");

        assert_eq!(result, None);
        assert!(!dir.exists(), "transcript dizini olusturulmus");
        assert!(
            temp.files().is_empty(),
            "diske dosya yazilmis: {:?}",
            temp.files()
        );
    }

    /// **ASU-037 kabul kriteri (yeniden baslatmadan etkili).** Acilista ayar
    /// acik olsa bile, kullanici calisma zamaninda kapattiysa yazma no-op olur.
    /// Yine bayrak degil dosya sistemi testi: dizin bile olusmuyor.
    #[test]
    fn the_runtime_switch_stops_writing_even_when_boot_allowed_it() {
        let temp = TempDir::new("runtime-off");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);

        let result = persist_with_runtime_switch(false, true, &dir, 42, &lines())
            .expect("kapali anahtar hata degil");

        assert_eq!(result, None);
        assert!(!dir.exists(), "transcript dizini olusturulmus");
        assert!(
            temp.files().is_empty(),
            "diske dosya yazilmis: {:?}",
            temp.files()
        );

        // Anahtar geri acilinca ayni cagri yazar — durum kalici olarak bozulmaz.
        assert!(persist_with_runtime_switch(true, true, &dir, 42, &lines())
            .expect("acik anahtar")
            .is_some());
    }

    /// Bos oturum icin bos dosya birakilmaz.
    #[test]
    fn does_not_create_a_file_for_an_empty_transcript() {
        let temp = TempDir::new("empty");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);

        assert_eq!(
            persist_if_enabled(true, &dir, 1, &[]).expect("hata olmamali"),
            None
        );
        assert!(!dir.exists());
        assert!(temp.files().is_empty());
    }

    /// Transcript kullanicinin en mahrem verisi: ayni makinedeki baska bir
    /// kullanici okuyamamali.
    #[cfg(unix)]
    #[test]
    fn transcript_files_are_only_readable_by_the_owner() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new("perms");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);
        let path = persist_if_enabled(true, &dir, 7, &lines())
            .expect("yazma")
            .expect("yol");

        let file_mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        let dir_mode = fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777;

        assert_eq!(file_mode, 0o600, "dosya izinleri: {file_mode:o}");
        assert_eq!(dir_mode, 0o700, "dizin izinleri: {dir_mode:o}");

        // Gate 3 / LOW-7,8: onceki bir calismadan kalmis gevsek izinler bir
        // sonraki yazimda sikilastirilir (izin **yaratilis aninda** verilir,
        // ama var olan dosya da duzeltilir).
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod");
        persist_if_enabled(true, &dir, 7, &lines()).expect("ikinci yazim");

        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777,
            0o700
        );
    }

    /// Ayni oturum tekrar yazilirsa dosya buyumez, degisir (yeniden kapanma).
    #[test]
    fn rewriting_the_same_session_replaces_the_file() {
        let temp = TempDir::new("rewrite");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);

        persist_if_enabled(true, &dir, 5, &lines()).expect("ilk yazim");
        let path = persist_if_enabled(true, &dir, 5, &lines()[..1])
            .expect("ikinci yazim")
            .expect("yol");

        let content = fs::read_to_string(&path).expect("okuma");
        assert_eq!(content.lines().count(), 1);
    }

    #[test]
    fn unknown_transcript_fields_are_rejected_at_the_ipc_boundary() {
        assert!(serde_json::from_str::<TranscriptLine>(
            r#"{"role":"user","text":"merhaba","path":"/etc/passwd"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<TranscriptLine>(r#"{"role":"system","text":"x"}"#).is_err());
    }

    // --- silme (ASU-065) --------------------------------------------------

    /// **ASU-065 kabul kriteri**: dokum dosyasi gercekten diskten gidiyor.
    #[test]
    fn deleting_a_session_removes_its_transcript_file_from_disk() {
        let temp = TempDir::new("delete");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);
        let path = persist_if_enabled(true, &dir, 42, &lines())
            .expect("yazma")
            .expect("yol");
        assert!(path.exists());

        let outcome = delete_recorded_file(&dir, 42, Some(&path.to_string_lossy()));

        assert_eq!(outcome, TranscriptFileOutcome::Deleted);
        assert!(!path.exists(), "dosya diskte kalmis");
        assert!(
            temp.files().is_empty(),
            "diskte dosya kalmis: {:?}",
            temp.files()
        );

        // Ikinci cagri "sildim" demez: dosya zaten yok.
        assert_eq!(
            delete_recorded_file(&dir, 42, Some(&path.to_string_lossy())),
            TranscriptFileOutcome::AlreadyGone
        );
    }

    #[test]
    fn a_session_without_a_recorded_path_reports_nothing_to_delete() {
        let temp = TempDir::new("no-path");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);

        assert_eq!(
            delete_recorded_file(&dir, 1, None),
            TranscriptFileOutcome::NotRecorded
        );
        assert_eq!(
            delete_recorded_file(&dir, 1, Some("   ")),
            TranscriptFileOutcome::NotRecorded
        );
        assert!(!dir.exists(), "silme yolu dizin olusturmamali");
    }

    /// **GUVENLIK (ASU-065)**: DB bozulmus/elle duzenlenmis olsa bile silme
    /// sandbox'in disina cikamaz. Traversal, mutlak kacis ve baska bir oturumun
    /// dosyasi — ucu de reddedilir ve **hicbiri diske dokunmaz**.
    #[test]
    fn the_delete_path_never_escapes_the_transcript_directory() {
        let temp = TempDir::new("traversal");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);
        fs::create_dir_all(&dir).expect("dizin");

        // Sandbox disinda, silinmemesi gereken bir "kurban" dosya.
        let victim = temp.path().join("id_ed25519");
        fs::write(&victim, b"PRIVATE KEY").expect("kurban dosyasi yazilabilmeli");

        // Sandbox icinde baska bir oturumun dosyasi.
        let other = persist_if_enabled(true, &dir, 9, &lines())
            .expect("yazma")
            .expect("yol");

        let escapes = [
            // Traversal: dizinin icinden cikip disariya uzaniyor.
            dir.join("..")
                .join("id_ed25519")
                .to_string_lossy()
                .into_owned(),
            // `..` ile geri gelip kurbani hedefliyor, ama dosya adi dogru.
            dir.join("..")
                .join("session-42.jsonl")
                .to_string_lossy()
                .into_owned(),
            // Tamamen baska bir mutlak yol.
            "/etc/passwd".to_owned(),
            // Sandbox icinde ama **baska** oturumun dosyasi.
            other.to_string_lossy().into_owned(),
            // Goreli yol: base_dir altinda oldugu dogrulanamaz.
            "session-42.jsonl".to_owned(),
        ];

        for escape in escapes {
            assert_eq!(
                delete_recorded_file(&dir, 42, Some(&escape)),
                TranscriptFileOutcome::Refused,
                "yol reddedilmedi: {escape}"
            );
        }

        assert!(victim.exists(), "sandbox disindaki dosya silinmis");
        assert!(other.exists(), "baska oturumun dokumu silinmis");
    }

    /// Dizine yerlestirilmis bir symlink, adi dogru olsa bile takip edilmez.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_transcript_is_refused_instead_of_followed() {
        let temp = TempDir::new("symlink");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);
        fs::create_dir_all(&dir).expect("dizin");

        let victim = temp.path().join("secret.txt");
        fs::write(&victim, b"gizli").expect("kurban dosyasi");

        let link = dir.join(transcript_file_name(7));
        std::os::unix::fs::symlink(&victim, &link).expect("symlink kurulabilmeli");

        assert_eq!(
            delete_recorded_file(&dir, 7, Some(&link.to_string_lossy())),
            TranscriptFileOutcome::Refused
        );
        assert!(victim.exists(), "symlink hedefi silinmis");
        assert!(
            fs::symlink_metadata(&link).is_ok(),
            "link'e dokunulmamis olmali"
        );
    }

    /// **ASU-065 kabul kriteri**: toplu temizlik sonrasi dizin bos.
    #[test]
    fn purging_the_directory_removes_every_transcript_file() {
        let temp = TempDir::new("purge");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);
        for id in 1..=3 {
            persist_if_enabled(true, &dir, id, &lines()).expect("yazma");
        }

        let purge = purge_directory(&dir);

        assert_eq!(purge.deleted, 3);
        assert_eq!(purge.remaining, 0);
        assert!(!dir.exists(), "bosalan dizin de kaldirilmali");
        assert!(temp.files().is_empty(), "kalan: {:?}", temp.files());

        // Dizin yokken tekrar cagirmak hata degil.
        assert_eq!(purge_directory(&dir), TranscriptPurge::default());
    }

    /// Asuna'nin uretmedigi dosyalar silinmez: kullanicinin o dizine koydugu bir
    /// seyi silmek, istemedigi bir seyi silmektir. Sayilir ve **raporlanir**.
    #[test]
    fn purging_leaves_files_asuna_did_not_write() {
        let temp = TempDir::new("purge-foreign");
        let dir = temp.path().join(TRANSCRIPT_DIR_NAME);
        persist_if_enabled(true, &dir, 5, &lines()).expect("yazma");

        let foreign = dir.join("notlarim.txt");
        fs::write(&foreign, b"benim notum").expect("yabanci dosya");
        let nested = dir.join("session-abc.jsonl");
        fs::write(&nested, b"{}").expect("desene uymayan dosya");

        let purge = purge_directory(&dir);

        assert_eq!(purge.deleted, 1);
        assert_eq!(purge.remaining, 2);
        assert!(foreign.exists(), "yabanci dosya silinmis");
        assert!(nested.exists(), "desene uymayan dosya silinmis");
        assert!(dir.exists(), "icinde dosya varken dizin kaldirilmamali");
    }

    #[test]
    fn only_generated_transcript_names_are_recognized() {
        assert!(is_transcript_file_name("session-1.jsonl"));
        assert!(is_transcript_file_name("session-987654321.jsonl"));

        for name in [
            "session-.jsonl",
            "session-1.json",
            "session-1.jsonl.bak",
            "sessions-1.jsonl",
            "session-abc.jsonl",
            "session--1.jsonl",
            ".env",
        ] {
            assert!(!is_transcript_file_name(name), "kabul edilmemeli: {name}");
        }
    }

    #[test]
    fn the_outcome_is_explicit_on_the_wire() {
        for (outcome, expected) in [
            (TranscriptFileOutcome::NotRecorded, "not-recorded"),
            (TranscriptFileOutcome::Deleted, "deleted"),
            (TranscriptFileOutcome::AlreadyGone, "already-gone"),
            (TranscriptFileOutcome::Refused, "refused"),
            (TranscriptFileOutcome::Failed, "failed"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("serialize"), expected);
        }
    }
}

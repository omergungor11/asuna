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
//! Ikinci kat koruma renderer tarafinda: `transcriptStorage` kapaliyken Realtime
//! oturumunda `audio.input.transcription` hic acilmaz (voice.md Bolum 2) — yani
//! yazilacak metin uretilmez bile.
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

    fs::create_dir_all(base_dir)?;
    restrict_permissions(base_dir, 0o700)?;

    let path = base_dir.join(transcript_file_name(session_id));
    let mut file = File::create(&path)?;
    restrict_permissions(&path, 0o600)?;

    for line in lines {
        let encoded = serde_json::to_string(line).map_err(io::Error::other)?;
        writeln!(file, "{encoded}")?;
    }
    file.sync_all()?;

    Ok(Some(path))
}

/// Sahibinden baskasi okuyamasin. Unix disinda sessizce atlanir (Asuna macOS
/// hedefli; yine de derleme kirilmasin).
#[cfg(unix)]
fn restrict_permissions(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
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
}

//! `.asuna/context.json` — proje basina **devir teslim artefakti** (ASU-043,
//! PROJECT.md Bolum 16).
//!
//! # Bu dosya tek gercek kaynak DEGILDIR
//!
//! PROJECT.md Bolum 16, harfiyen: *"This file is not the only source of truth;
//! it is a compact handoff artifact."*
//!
//! Cakisma kurali bu yuzden tek yonlu ve **pazarliksiz**:
//!
//! > **DB kazanir.** `memories` / `sessions` / `projects` tablolari ile
//! > `.asuna/context.json` celisirse Asuna DB'ye inanir. Dosya, DB'yi
//! > *guncelleyemez*; DB dosyayi guncelleyebilir.
//!
//! Gerekce: dosya kullanicinin (ya da baska bir aracin, ya da bir git merge
//! cakismasinin) her an elle degistirebilecegi bir metindir. Onu otoriter
//! saymak, "hafizami sildim" diyen bir kullanicinin silinmis kararlarinin bir
//! dosyadan geri dogmasi demekti (M3'te tam olarak bu sinifta bir hata
//! yakalandi — ASU-065). Tersi yonde ise dosyanin degeri yuksek: repo ile
//! birlikte tasinir, git'e girer, baska bir makinede ya da baska bir ajanda
//! okunabilir.
//!
//! Pratik sonuclar:
//!
//! - Okuma **tamamlayicidir**: dosyadaki bir karar DB'de yoksa Asuna onu
//!   "hatirladigi bir karar" gibi sunmaz; "projenin devir teslim notunda soyle
//!   yaziyor" der.
//! - Yazma **tek yonludur**: oturum sonunda DB'deki durum dosyaya islenir.
//! - Dosya silinirse hicbir hafiza kaybolmaz.
//!
//! # Guvenlik
//!
//! - Okuma ve yazma **yalnizca kayitli proje koku altinda** (ASU-040). Hedef yol
//!   `canonicalize` edildikten sonra kok prefix'i kontrol edilir; `.asuna` bir
//!   symlink olup disari gosteriyorsa islem reddedilir.
//! - Yazilan metin [`redact_sensitive_text`]'ten gecer: icerik oturumdan gelir
//!   (model ciktisi) ve kullanicinin sesli okudugu bir anahtar buraya **kalici**
//!   olarak girebilirdi.
//! - Yazma **atomiktir**: gecici dosya + `rename`. Yarim bir JSON birakmak,
//!   bir sonraki acilista "bozuk dosya" demek olurdu.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::redaction::redact_sensitive_text;

/// Artefaktin bulundugu dizin (proje koku altinda).
pub const HANDOFF_DIR: &str = ".asuna";

/// Artefakt dosyasinin adi.
pub const HANDOFF_FILE: &str = "context.json";

/// Okunacak en fazla bayt. Devir teslim notu kisa bir belgedir; bunu asan bir
/// dosya ya bozuk ya da baska bir sey.
pub const MAX_HANDOFF_BYTES: usize = 64 * 1024;

/// Tek metin alaninin karakter tavani.
pub const MAX_FIELD_CHARS: usize = 300;

/// Liste alanlarindaki en fazla girdi.
pub const MAX_LIST_ITEMS: usize = 10;

/// Liste girdisi basina karakter tavani.
pub const MAX_ITEM_CHARS: usize = 200;

// ---------------------------------------------------------------------------
// Sema
// ---------------------------------------------------------------------------

/// `.asuna/context.json` semasi (PROJECT.md Bolum 16).
///
/// Tum alanlar opsiyonel: yarim doldurulmus bir dosya gecerlidir ve eksik alan
/// **uydurulmaz**.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffContext {
    pub project_name: Option<String>,
    pub objective: Option<String>,
    pub current_milestone: Option<String>,
    pub active_task: Option<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub recent_decisions: Vec<String>,
}

impl HandoffContext {
    /// Hicbir alani dolu olmayan baglam.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Alanlari tavanlara indirger ve **saklanacak metni redakte eder**.
    ///
    /// Hem okumada hem yazmada calisir: elle duzenlenmis bir dosyadaki 5 MB'lik
    /// bir `objective` de, modelden gelen bir karar metni de ayni suzgecten
    /// gecer.
    fn normalise(self) -> Self {
        Self {
            project_name: normalise_field(self.project_name),
            objective: normalise_field(self.objective),
            current_milestone: normalise_field(self.current_milestone),
            active_task: normalise_field(self.active_task),
            blockers: normalise_list(self.blockers),
            recent_decisions: normalise_list(self.recent_decisions),
        }
    }
}

fn normalise_field(value: Option<String>) -> Option<String> {
    let value = value?;
    let redacted = redact_sensitive_text(value.trim());
    if redacted.is_empty() {
        return None;
    }
    Some(clip(&redacted, MAX_FIELD_CHARS))
}

fn normalise_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter_map(|item| {
            let redacted = redact_sensitive_text(item.trim());
            (!redacted.is_empty()).then(|| clip(&redacted, MAX_ITEM_CHARS))
        })
        .take(MAX_LIST_ITEMS)
        .collect()
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(limit.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

// ---------------------------------------------------------------------------
// Okuma
// ---------------------------------------------------------------------------

/// Dosyanin neden yok sayildigi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HandoffIgnoreReason {
    /// JSON ayristirilamadi (elle duzenlenmis, yarim yazilmis, merge cakismasi).
    InvalidJson,
    /// Kok bir JSON nesnesi degil (dizi, sayi, metin...).
    NotAnObject,
    /// Dosya tavandan buyuk.
    TooLarge,
    /// Okunamadi (izin, IO).
    Unreadable,
    /// Yol kayitli kok disina cikiyor (symlink).
    OutsideRoot,
}

impl HandoffIgnoreReason {
    /// Kullaniciya gosterilebilecek aciklama. Yol **icermez**.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::InvalidJson => ".asuna/context.json gecerli JSON degil, yok sayildi",
            Self::NotAnObject => ".asuna/context.json bir JSON nesnesi degil, yok sayildi",
            Self::TooLarge => ".asuna/context.json beklenenden buyuk, yok sayildi",
            Self::Unreadable => ".asuna/context.json okunamadi, yok sayildi",
            Self::OutsideRoot => ".asuna/context.json proje kokunun disina cikiyor, okunmadi",
        }
    }
}

/// Okuma sonucu.
///
/// Uc durum bilerek ayri: "dosya yok" bir hata degil, "bozuk dosya" bir uyari,
/// "okundu" bir veri. Ikisini birlestirmek, bozuk bir dosyayi sessizce "bos
/// baglam" gibi gostermek olurdu.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum HandoffRead {
    Loaded {
        context: Box<HandoffContext>,
    },
    /// Dosya yok — bos baglam, **hata degil**.
    Absent,
    /// Dosya var ama kullanilamadi. Uygulama cokmez, uyari verilir.
    Ignored {
        reason: HandoffIgnoreReason,
        message: &'static str,
    },
}

impl HandoffRead {
    /// Okunan baglam; yoksa/bozuksa bos baglam.
    ///
    /// Cagiran taraf "bos mu, bozuk mu?" ayrimini onemsemiyorsa bunu kullanir —
    /// ama ayrim [`HandoffRead`] uzerinde durmaya devam eder.
    pub fn context_or_empty(&self) -> HandoffContext {
        match self {
            Self::Loaded { context } => (**context).clone(),
            Self::Absent | Self::Ignored { .. } => HandoffContext::default(),
        }
    }
}

/// Kayitli kok altindaki artefaktin yolu.
pub fn handoff_path(root: &Path) -> PathBuf {
    root.join(HANDOFF_DIR).join(HANDOFF_FILE)
}

/// Artefakti okur.
///
/// Bozuk dosya uygulamayi **cokertmez**: uyari log'lanir ve
/// [`HandoffRead::Ignored`] doner.
pub fn read(root: &Path) -> HandoffRead {
    let path = handoff_path(root);

    let Ok(resolved) = std::fs::canonicalize(&path) else {
        // Dosya yok ya da yol cozulemiyor — ikisi de "bos baglam".
        return HandoffRead::Absent;
    };
    if !is_inside_root(root, &resolved) {
        return ignored(HandoffIgnoreReason::OutsideRoot);
    }

    let Ok(metadata) = std::fs::metadata(&resolved) else {
        return ignored(HandoffIgnoreReason::Unreadable);
    };
    if !metadata.is_file() {
        return HandoffRead::Absent;
    }
    if metadata.len() > MAX_HANDOFF_BYTES as u64 {
        return ignored(HandoffIgnoreReason::TooLarge);
    }

    let Ok(bytes) = std::fs::read(&resolved) else {
        return ignored(HandoffIgnoreReason::Unreadable);
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return ignored(HandoffIgnoreReason::InvalidJson);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return ignored(HandoffIgnoreReason::InvalidJson);
    };
    let Some(object) = value.as_object() else {
        return ignored(HandoffIgnoreReason::NotAnObject);
    };

    // Alan alan, **hosgorulu** okuma. `serde` ile tek seferde ayristirmak,
    // tek bir yanlis tipli alan yuzunden butun dosyayi cope atardi; bu dosya
    // elle duzenlenmeye acik ve "blockers: null" yazan bir kullanici geri
    // kalanini da kaybetmemeli. Yok sayilan alanlar sessiz degil, log'lanir.
    let mut ignored_fields: Vec<&str> = Vec::new();
    let mut text_field = |key: &'static str| -> Option<String> {
        match object.get(key) {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(value)) => Some(value.clone()),
            Some(_) => {
                ignored_fields.push(key);
                None
            }
        }
    };

    let context = HandoffContext {
        project_name: text_field("projectName"),
        objective: text_field("objective"),
        current_milestone: text_field("currentMilestone"),
        active_task: text_field("activeTask"),
        blockers: string_list(object, "blockers", &mut ignored_fields),
        recent_decisions: string_list(object, "recentDecisions", &mut ignored_fields),
    }
    .normalise();

    if !ignored_fields.is_empty() {
        eprintln!(
            "[asuna] .asuna/context.json: beklenmeyen tipteki alanlar yok sayildi: {}",
            ignored_fields.join(", ")
        );
    }

    HandoffRead::Loaded {
        context: Box::new(context),
    }
}

fn string_list(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
    ignored_fields: &mut Vec<&'static str>,
) -> Vec<String> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        Some(_) => {
            ignored_fields.push(key);
            Vec::new()
        }
    }
}

fn ignored(reason: HandoffIgnoreReason) -> HandoffRead {
    // Sessiz yutma yok: kullanici dosyasinin neden kullanilmadigini bilmeli.
    eprintln!("[asuna] {}", reason.describe());
    HandoffRead::Ignored {
        reason,
        message: reason.describe(),
    }
}

/// Cozulmus yol kayitli kokun altinda mi?
///
/// Iki taraf da `canonicalize` edilir: metin `starts_with` karsilastirmasi
/// macOS'ta `/tmp` ↔ `/private/tmp` yuzunden yaniltici olurdu ve symlink
/// kacisini yakalayamazdi.
fn is_inside_root(root: &Path, resolved: &Path) -> bool {
    match std::fs::canonicalize(root) {
        Ok(canonical_root) => resolved.starts_with(&canonical_root),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Yazma
// ---------------------------------------------------------------------------

/// Yazma hatasi.
///
/// GUVENLIK: mesajlar yol tasimaz (`db::DbError` ile ayni kural).
#[derive(Debug, thiserror::Error)]
pub enum HandoffWriteError {
    #[error("hedef yol proje kokunun disina cikiyor")]
    OutsideRoot,

    #[error(".asuna dizini olusturulamadi")]
    CreateDirectory(#[source] std::io::Error),

    #[error("devir teslim dosyasi yazilamadi")]
    Write(#[source] std::io::Error),

    #[error("devir teslim dosyasi yerine tasinamadi")]
    Rename(#[source] std::io::Error),

    #[error("devir teslim dosyasi seri hale getirilemedi")]
    Serialize(#[source] serde_json::Error),
}

/// Artefakti **atomik** olarak yazar.
///
/// # Atomiklik
///
/// Icerik once ayni dizindeki gecici bir dosyaya yazilir, `sync_all` ile diske
/// indirilir, sonra `rename` ile hedefin uzerine tasinir. POSIX'te ayni dosya
/// sistemi icindeki `rename` atomiktir: okuyan taraf ya eski ya yeni dosyayi
/// gorur, **yarim** dosyayi hicbir zaman gormez.
///
/// Gecici dosya bilerek hedefle ayni dizinde: `/tmp`'ye yazip tasimak dosya
/// sistemi sinirini gecebilir ve `rename` `EXDEV` ile duserdi.
///
/// # Sandbox
///
/// Hedef her zaman `<kayitli kok>/.asuna/context.json`. Yol cagirandan
/// alinmaz; `.asuna` bir symlink olup disari gosteriyorsa yazma **reddedilir**.
pub fn write(root: &Path, context: &HandoffContext) -> Result<PathBuf, HandoffWriteError> {
    let directory = root.join(HANDOFF_DIR);
    std::fs::create_dir_all(&directory).map_err(HandoffWriteError::CreateDirectory)?;

    // Dizin **olusturulduktan sonra** cozulur: var olmayan bir yol
    // `canonicalize` edilemez.
    let resolved_directory =
        std::fs::canonicalize(&directory).map_err(HandoffWriteError::CreateDirectory)?;
    if !is_inside_root(root, &resolved_directory) {
        return Err(HandoffWriteError::OutsideRoot);
    }

    let normalised = context.clone().normalise();
    let mut json =
        serde_json::to_string_pretty(&normalised).map_err(HandoffWriteError::Serialize)?;
    json.push('\n');

    // Gecici ad benzersiz: iki oturum ayni anda yazarsa birbirinin yarim
    // dosyasini gormesin.
    let unique = format!(
        ".{HANDOFF_FILE}.tmp-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    );
    let temporary = resolved_directory.join(unique);
    let target = resolved_directory.join(HANDOFF_FILE);

    let outcome = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(json.as_bytes())?;
        // Guc kesintisinde bos bir dosya birakmamak icin: once icerik diske,
        // sonra rename.
        file.sync_all()?;
        Ok(())
    })();

    if let Err(error) = outcome {
        let _ = std::fs::remove_file(&temporary);
        return Err(HandoffWriteError::Write(error));
    }

    if let Err(error) = std::fs::rename(&temporary, &target) {
        let _ = std::fs::remove_file(&temporary);
        return Err(HandoffWriteError::Rename(error));
    }

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "asuna-handoff-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let path = std::env::temp_dir().join(unique);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("gecici dizin");
            Self(path)
        }

        fn root(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("proje dizini");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_raw(root: &Path, content: &str) {
        let directory = root.join(HANDOFF_DIR);
        std::fs::create_dir_all(&directory).expect("dizin");
        std::fs::write(directory.join(HANDOFF_FILE), content).expect("dosya");
    }

    fn sample() -> HandoffContext {
        HandoffContext {
            project_name: Some("Asuna".to_owned()),
            objective: Some("Local-first voice AI companion".to_owned()),
            current_milestone: Some("Realtime voice MVP".to_owned()),
            active_task: Some("Connect wake word to realtime session".to_owned()),
            blockers: Vec::new(),
            recent_decisions: vec![
                "Use OpenAI Realtime Agents SDK".to_owned(),
                "Use gpt-realtime-2.1".to_owned(),
                "Keep wake-word detection local".to_owned(),
            ],
        }
    }

    // --- Sema ---------------------------------------------------------------

    /// **Kabul kriteri**: sema PROJECT.md Bolum 16'daki ornekle birebir.
    #[test]
    fn the_schema_matches_the_documented_example() {
        let temp = TempDir::new("schema");
        let root = temp.root("asuna");
        write_raw(
            &root,
            r#"{
              "projectName": "Asuna",
              "objective": "Local-first voice AI companion",
              "currentMilestone": "Realtime voice MVP",
              "activeTask": "Connect wake word to realtime session",
              "blockers": [],
              "recentDecisions": [
                "Use OpenAI Realtime Agents SDK",
                "Use gpt-realtime-2.1",
                "Keep wake-word detection local"
              ]
            }"#,
        );

        match read(&root) {
            HandoffRead::Loaded { context } => assert_eq!(*context, sample()),
            other => panic!("okunmaliydi: {other:?}"),
        }
    }

    /// Yazilan JSON'in anahtarlari sema ile birebir (`camelCase`).
    #[test]
    fn the_written_json_uses_the_documented_keys() {
        let temp = TempDir::new("keys");
        let root = temp.root("asuna");
        let path = write(&root, &sample()).expect("yazilmali");

        let text = std::fs::read_to_string(&path).expect("okunmali");
        let value: serde_json::Value = serde_json::from_str(&text).expect("gecerli JSON");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("nesne")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "activeTask",
                "blockers",
                "currentMilestone",
                "objective",
                "projectName",
                "recentDecisions",
            ]
        );
        assert!(text.ends_with('\n'), "dosya satir sonuyla bitmeli");
    }

    // --- Yok / bozuk --------------------------------------------------------

    /// **Kabul kriteri**: dosya yoksa hata degil, bos baglam.
    #[test]
    fn a_missing_file_is_an_empty_context_not_an_error() {
        let temp = TempDir::new("absent");
        let root = temp.root("asuna");

        assert_eq!(read(&root), HandoffRead::Absent);
        assert!(read(&root).context_or_empty().is_empty());
    }

    /// **Kabul kriteri**: bozuk JSON uygulamayi cokertmez, uyari ile yok sayilir.
    #[test]
    fn broken_json_is_ignored_with_a_warning_not_a_crash() {
        let temp = TempDir::new("broken");
        let root = temp.root("asuna");

        for (content, expected) in [
            ("{ bu gecerli JSON degil", HandoffIgnoreReason::InvalidJson),
            ("", HandoffIgnoreReason::InvalidJson),
            // Git merge cakismasi birakilmis dosya.
            (
                "<<<<<<< HEAD\n{}\n=======\n{}\n>>>>>>> other\n",
                HandoffIgnoreReason::InvalidJson,
            ),
            ("[1, 2, 3]", HandoffIgnoreReason::NotAnObject),
            ("\"sadece bir metin\"", HandoffIgnoreReason::NotAnObject),
        ] {
            write_raw(&root, content);
            match read(&root) {
                HandoffRead::Ignored { reason, message } => {
                    assert_eq!(reason, expected, "girdi: {content}");
                    assert!(!message.is_empty());
                }
                other => panic!("yok sayilmaliydi ({content}): {other:?}"),
            }
            assert!(read(&root).context_or_empty().is_empty());
        }
    }

    /// Yanlis tipteki tek bir alan butun dosyayi cope atmaz; yalnizca o alan
    /// yok sayilir. Dosya elle duzenlenmeye acik.
    #[test]
    fn a_single_mistyped_field_does_not_discard_the_rest() {
        let temp = TempDir::new("mistyped");
        let root = temp.root("asuna");
        write_raw(
            &root,
            r#"{"projectName": "Asuna", "objective": 42, "blockers": "bos degil",
                "recentDecisions": ["Karar bir"]}"#,
        );

        match read(&root) {
            HandoffRead::Loaded { context } => {
                assert_eq!(context.project_name.as_deref(), Some("Asuna"));
                assert_eq!(context.objective, None, "yanlis tipli alan uydurulmaz");
                assert!(context.blockers.is_empty());
                assert_eq!(context.recent_decisions, ["Karar bir"]);
            }
            other => panic!("okunmaliydi: {other:?}"),
        }
    }

    #[test]
    fn an_oversized_file_is_ignored() {
        let temp = TempDir::new("huge");
        let root = temp.root("asuna");
        let huge = format!("{{\"objective\": \"{}\"}}", "a".repeat(MAX_HANDOFF_BYTES));
        write_raw(&root, &huge);

        assert!(matches!(
            read(&root),
            HandoffRead::Ignored {
                reason: HandoffIgnoreReason::TooLarge,
                ..
            }
        ));
    }

    // --- Sinirlar + redaksiyon ---------------------------------------------

    #[test]
    fn oversized_fields_and_lists_are_clipped() {
        let temp = TempDir::new("clip");
        let root = temp.root("asuna");

        let context = HandoffContext {
            objective: Some("kelime ".repeat(500)),
            blockers: (0..40).map(|index| format!("engel {index}")).collect(),
            recent_decisions: vec!["karar ".repeat(200)],
            ..HandoffContext::default()
        };
        write(&root, &context).expect("yazilmali");

        let stored = match read(&root) {
            HandoffRead::Loaded { context } => *context,
            other => panic!("okunmaliydi: {other:?}"),
        };
        assert!(stored.objective.expect("dolu").chars().count() <= MAX_FIELD_CHARS);
        assert_eq!(stored.blockers.len(), MAX_LIST_ITEMS);
        assert!(stored.recent_decisions[0].chars().count() <= MAX_ITEM_CHARS);
    }

    /// Icerik oturumdan (model ciktisindan) gelir; sizan bir anahtar dosyaya
    /// **kalici** olarak girmemeli.
    #[test]
    fn secrets_are_redacted_before_being_written_to_disk() {
        let temp = TempDir::new("redact");
        let root = temp.root("asuna");

        let context = HandoffContext {
            active_task: Some("anahtari sk-proj-COK-GIZLI-DEGER ile degistir".to_owned()),
            recent_decisions: vec!["token: ghp_BASKA_BIR_GIZLI_DEGER kullanilacak".to_owned()],
            ..HandoffContext::default()
        };
        let path = write(&root, &context).expect("yazilmali");

        let text = std::fs::read_to_string(&path).expect("okunmali");
        assert!(!text.contains("COK-GIZLI-DEGER"), "{text}");
        assert!(!text.contains("ghp_BASKA_BIR_GIZLI_DEGER"), "{text}");
        assert!(text.contains("sk-<redacted>"), "{text}");
    }

    /// Elle yazilmis bir dosyadaki anahtar da okuma sirasinda maskelenir —
    /// dosyadan modele ham gecmez.
    #[test]
    fn secrets_in_a_hand_written_file_are_masked_on_read() {
        let temp = TempDir::new("redact-read");
        let root = temp.root("asuna");
        write_raw(
            &root,
            r#"{"objective": "sk-proj-ELLE-YAZILMIS-ANAHTAR ile deneme"}"#,
        );

        let context = read(&root).context_or_empty();
        let objective = context.objective.expect("dolu");
        assert!(!objective.contains("ELLE-YAZILMIS-ANAHTAR"), "{objective}");
    }

    // --- Atomik yazma -------------------------------------------------------

    /// **Kabul kriteri**: yazma atomik, yarim dosya birakmiyor.
    #[test]
    fn writing_is_atomic_and_leaves_no_temporary_file() {
        let temp = TempDir::new("atomic");
        let root = temp.root("asuna");

        write(&root, &sample()).expect("ilk yazma");
        write(
            &root,
            &HandoffContext {
                objective: Some("Ikinci hali".to_owned()),
                ..HandoffContext::default()
            },
        )
        .expect("ikinci yazma");

        let entries: Vec<String> = std::fs::read_dir(root.join(HANDOFF_DIR))
            .expect("dizin okunmali")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .collect();
        assert_eq!(
            entries,
            [HANDOFF_FILE],
            "gecici dosya birakilmis: {entries:?}"
        );

        // Uzerine yazma tamamlanmis olmali; yarim/eski icerik yok.
        let context = read(&root).context_or_empty();
        assert_eq!(context.objective.as_deref(), Some("Ikinci hali"));
        assert_eq!(context.project_name, None);
    }

    #[test]
    fn writing_creates_the_asuna_directory_when_missing() {
        let temp = TempDir::new("mkdir");
        let root = temp.root("asuna");
        assert!(!root.join(HANDOFF_DIR).exists());

        let path = write(&root, &sample()).expect("yazilmali");
        assert!(path.is_file());
        assert!(path.starts_with(std::fs::canonicalize(&root).expect("canonical")));
    }

    #[test]
    fn a_round_trip_preserves_the_context() {
        let temp = TempDir::new("round-trip");
        let root = temp.root("asuna");

        write(&root, &sample()).expect("yazilmali");
        assert_eq!(read(&root).context_or_empty(), sample());
    }

    // --- Sandbox ------------------------------------------------------------

    /// **Kabul kriteri**: yazma kayitli kok disina cikamaz. `.asuna` bir
    /// symlink olup disari gosteriyorsa islem reddedilir.
    #[cfg(unix)]
    #[test]
    fn writing_through_a_symlinked_directory_outside_the_root_is_refused() {
        let temp = TempDir::new("escape");
        let root = temp.root("asuna");
        let outside = temp.0.join("disarisi");
        std::fs::create_dir_all(&outside).expect("dis dizin");
        std::os::unix::fs::symlink(&outside, root.join(HANDOFF_DIR)).expect("symlink");

        let error = write(&root, &sample()).expect_err("reddedilmeli");
        assert!(matches!(error, HandoffWriteError::OutsideRoot), "{error}");
        assert!(
            !outside.join(HANDOFF_FILE).exists(),
            "kok disina dosya yazilmis"
        );
    }

    /// Ayni symlink okuma tarafinda da reddedilir.
    #[cfg(unix)]
    #[test]
    fn reading_through_a_symlink_outside_the_root_is_refused() {
        let temp = TempDir::new("escape-read");
        let root = temp.root("asuna");
        let outside = temp.0.join("disarisi");
        std::fs::create_dir_all(&outside).expect("dis dizin");
        std::fs::write(outside.join(HANDOFF_FILE), r#"{"objective":"disaridan"}"#)
            .expect("dis dosya");
        std::os::unix::fs::symlink(&outside, root.join(HANDOFF_DIR)).expect("symlink");

        match read(&root) {
            HandoffRead::Ignored { reason, .. } => {
                assert_eq!(reason, HandoffIgnoreReason::OutsideRoot);
            }
            other => panic!("reddedilmeliydi: {other:?}"),
        }
    }

    /// Hata mesajlari yol sizdirmaz (log'a ve UI'a dusebilir).
    #[test]
    fn write_errors_never_leak_a_path() {
        let message = HandoffWriteError::OutsideRoot.to_string();
        assert!(!message.contains('/'), "{message}");
    }

    /// Yok sayma gerekceleri de yol icermez.
    #[test]
    fn ignore_reasons_never_leak_a_path() {
        for reason in [
            HandoffIgnoreReason::InvalidJson,
            HandoffIgnoreReason::NotAnObject,
            HandoffIgnoreReason::TooLarge,
            HandoffIgnoreReason::Unreadable,
            HandoffIgnoreReason::OutsideRoot,
        ] {
            let description = reason.describe();
            // Yalnizca sabit artefakt yolu gecebilir, kullanicinin dizini degil.
            assert!(description.starts_with(".asuna/") || !description.contains('/'));
        }
    }

    #[test]
    fn read_results_serialize_with_a_tagged_status() {
        let absent = serde_json::to_value(HandoffRead::Absent).expect("serialize");
        assert_eq!(absent["status"], "absent");

        let ignored = serde_json::to_value(HandoffRead::Ignored {
            reason: HandoffIgnoreReason::InvalidJson,
            message: HandoffIgnoreReason::InvalidJson.describe(),
        })
        .expect("serialize");
        assert_eq!(ignored["status"], "ignored");
        assert_eq!(ignored["reason"], "invalid-json");
    }
}

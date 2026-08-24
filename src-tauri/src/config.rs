//! Asuna tipli konfigurasyon katmani (ASU-009 / PROJECT.md Bolum 23).
//!
//! # Kurallar
//!
//! - **Tek okuma noktasi.** Uygulamanin baska hicbir yeri `std::env` okumaz;
//!   model ID'si, wake word, timeout gibi degerler hicbir yerde hard-code edilmez.
//! - **Sessiz default yok.** Beklenen her degisken `.env` (veya process environment)
//!   icinde *tanimli* olmak zorunda. Eksik ya da gecersiz deger acilista net bir
//!   hata uretir; kod "yoksa 45 varsayalim" demez.
//! - **`OPENAI_API_KEY` bu process'i terk etmez.** [`AsunaConfig`] bilerek
//!   `Serialize` **turetmez** — dolayisiyla bir `#[tauri::command]`'in donus tipi
//!   olamaz. Renderer'a yalnizca [`FrontendConfig`] whitelist'i gider.
//! - Hata mesajlari degeri degil, yalnizca **anahtar adini** ve beklenen bicimi
//!   icerir (PROJECT.md Bolum 19: secret log'a/hata mesajina yazilmaz).

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use serde::Serialize;

use crate::env_file::{self, EnvFileError};

/// Ayristirilmis `KEY=value` kumesi.
pub type EnvMap = BTreeMap<String, String>;

pub const KEY_OPENAI_API_KEY: &str = "OPENAI_API_KEY";
pub const KEY_REALTIME_MODEL: &str = "ASUNA_REALTIME_MODEL";
pub const KEY_REALTIME_VOICE: &str = "ASUNA_REALTIME_VOICE";
pub const KEY_WAKE_WORD: &str = "ASUNA_WAKE_WORD";
pub const KEY_MEMORY_ENABLED: &str = "ASUNA_MEMORY_ENABLED";
pub const KEY_TRANSCRIPT_STORAGE: &str = "ASUNA_TRANSCRIPT_STORAGE";
pub const KEY_TOOL_APPROVAL_MODE: &str = "ASUNA_TOOL_APPROVAL_MODE";
pub const KEY_IDLE_TIMEOUT_SECONDS: &str = "ASUNA_IDLE_TIMEOUT_SECONDS";
pub const KEY_LOG_LEVEL: &str = "ASUNA_LOG_LEVEL";
pub const KEY_WAKE_WORD_PROVIDER: &str = "ASUNA_WAKE_WORD_PROVIDER";
pub const KEY_WAKE_WORD_MODEL_DIR: &str = "ASUNA_WAKE_WORD_MODEL_DIR";
pub const KEY_WAKE_WORD_THRESHOLD: &str = "ASUNA_WAKE_WORD_THRESHOLD";

/// Konfigurasyonu olusturan tum anahtarlar. Process environment sadece bu
/// listedeki anahtarlar icin okunur — alakasiz degiskenler config'e sizmaz.
pub const ALL_KEYS: [&str; 12] = [
    KEY_OPENAI_API_KEY,
    KEY_REALTIME_MODEL,
    KEY_REALTIME_VOICE,
    KEY_WAKE_WORD,
    KEY_MEMORY_ENABLED,
    KEY_TRANSCRIPT_STORAGE,
    KEY_TOOL_APPROVAL_MODE,
    KEY_IDLE_TIMEOUT_SECONDS,
    KEY_LOG_LEVEL,
    KEY_WAKE_WORD_PROVIDER,
    KEY_WAKE_WORD_MODEL_DIR,
    KEY_WAKE_WORD_THRESHOLD,
];

/// `ASUNA_IDLE_TIMEOUT_SECONDS` icin kabul edilen aralik.
/// Alt sinir: 5 sn altinda konusma arasi sessizlik oturumu keser.
/// Ust sinir: Realtime oturumu API tarafinda 60 dk ile sinirli (voice.md Bolum 9).
const IDLE_TIMEOUT_RANGE: std::ops::RangeInclusive<u32> = 5..=1800;

/// SDK'nin kabul ettigi Realtime ses kimlikleri
/// (`docs/architecture/voice.md` Bolum 9, dogrulama 2026-08-24).
/// Yanlis yazilmis bir ses adi aksi halde ancak oturum acilirken, opak bir API
/// hatasi olarak ortaya cikardi — burada acilista yakalanir.
const KNOWN_VOICES: [&str; 10] = [
    "alloy", "ash", "ballad", "coral", "echo", "sage", "shimmer", "verse", "marin", "cedar",
];

// ---------------------------------------------------------------------------
// Secret sarmalayici
// ---------------------------------------------------------------------------

/// Log'a/hata mesajina yanlislikla basilmasin diye sarmalanmis secret.
///
/// `Display` ve `Serialize` **bilerek implemente edilmedi**; deger yalnizca
/// [`SecretString::expose`] ile, acikca istenerek alinir.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    /// Ham degeri dondurur. Yalnizca guvenilir process icinde, dogrudan
    /// OpenAI'ya giden `Authorization` header'i icin cagrilir (ASU-011).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

// ---------------------------------------------------------------------------
// Enum'lar
// ---------------------------------------------------------------------------

/// Log seviyesi. Renderer'a gider (whitelist).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "error" => Some(Self::Error),
            "warn" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }
}

/// Tool onay politikasi.
///
/// **Risk 2 ve 3 tool'lar her iki modda da her zaman onay ister** — bu deger
/// onlari bypass edemez (conventions.md "Tool Tanimi", security.md Bolum 3).
/// Mod yalnizca risk 1 (geri alinabilir) tool'larin davranisini belirler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolApprovalMode {
    /// Varsayilan ve onerilen: risk 0 serbest, risk 1 sessiz, risk 2/3 onay ister.
    Safe,
    /// Risk 0 disindaki her cagri onay ister.
    Always,
}

impl ToolApprovalMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "safe" => Some(Self::Safe),
            "always" => Some(Self::Always),
            _ => None,
        }
    }
}

/// Wake word saglayicisi secimi (`WakeWordProvider` adapter'inin hangi somut
/// implementasyonunun kurulacagi — ADR-004). Renderer'a **gitmez**: motor Rust
/// tarafinda calisir, renderer vendor adini gormez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeWordProviderKind {
    /// sherpa-onnx `KeywordSpotter` (ADR-004, ASU-022).
    SherpaKws,
    /// Test/gelistirme icin sahte saglayici (ASU-021) — mikrofon acmaz.
    Fake,
}

impl WakeWordProviderKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "sherpa-kws" => Some(Self::SherpaKws),
            "fake" => Some(Self::Fake),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SherpaKws => "sherpa-kws",
            Self::Fake => "fake",
        }
    }
}

// ---------------------------------------------------------------------------
// Hata tipi
// ---------------------------------------------------------------------------

/// Konfigurasyon yukleme hatasi. Hicbir varyant degeri tasimaz.
#[derive(Debug)]
pub enum ConfigError {
    /// `.env` dosyasi okunamadi / ayristirilamadi.
    EnvFile(EnvFileError),
    /// Anahtar hic tanimlanmamis.
    Missing { key: &'static str },
    /// Anahtar tanimli ama bos — bu anahtar icin bos deger anlamsiz.
    Empty { key: &'static str },
    /// Deger tanimli ama gecersiz. `expected` **beklenen bicimi** anlatir,
    /// gelen degeri asla tekrarlamaz.
    Invalid { key: &'static str, expected: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvFile(inner) => write!(f, "{inner}"),
            Self::Missing { key } => write!(
                f,
                "`{key}` tanimli degil. `.env.example` dosyasini `.env` olarak kopyalayip doldurun."
            ),
            Self::Empty { key } => write!(f, "`{key}` bos birakilamaz."),
            Self::Invalid { key, expected } => {
                write!(f, "`{key}` gecersiz — beklenen: {expected}.")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::EnvFile(inner) => Some(inner),
            _ => None,
        }
    }
}

impl From<EnvFileError> for ConfigError {
    fn from(value: EnvFileError) -> Self {
        Self::EnvFile(value)
    }
}

// ---------------------------------------------------------------------------
// Config tipleri
// ---------------------------------------------------------------------------

/// Uygulamanin tam konfigurasyonu — **yalnizca guvenilir process (Rust) icinde**.
///
/// GUVENLIK: bu tip `Serialize` **turetmez**. Bu, `OPENAI_API_KEY`'in bir Tauri
/// command'inin donus degerinde yer almasini derleme zamaninda imkansiz kilar.
/// Renderer'a gonderilecek alt kume icin [`AsunaConfig::to_frontend`] kullanilir.
#[derive(Debug, Clone)]
pub struct AsunaConfig {
    openai_api_key: SecretString,
    pub realtime_model: String,
    pub realtime_voice: Option<String>,
    pub wake_word: String,
    pub memory_enabled: bool,
    pub transcript_storage: bool,
    pub tool_approval_mode: ToolApprovalMode,
    pub idle_timeout_seconds: u32,
    pub log_level: LogLevel,
    pub wake_word_provider: WakeWordProviderKind,
    pub wake_word_model_dir: Option<PathBuf>,
    pub wake_word_threshold: f32,
}

impl AsunaConfig {
    /// Kalici OpenAI API key'i. Yalnizca ephemeral Realtime token uretimi icin
    /// (ASU-011). Donen deger log'lanmaz, IPC'ye konmaz, diske yazilmaz.
    pub fn openai_api_key(&self) -> &SecretString {
        &self.openai_api_key
    }

    /// Renderer'a acilan **whitelist**. Yeni alan eklemek bilincli bir guvenlik
    /// karari — buraya alan eklerken "bu deger webview'e sizabilir mi" sorusu
    /// cevaplanmis olmali.
    pub fn to_frontend(&self) -> FrontendConfig {
        FrontendConfig {
            realtime_model: self.realtime_model.clone(),
            realtime_voice: self.realtime_voice.clone(),
            wake_word: self.wake_word.clone(),
            idle_timeout_seconds: self.idle_timeout_seconds,
            log_level: self.log_level,
            memory_enabled: self.memory_enabled,
            transcript_storage: self.transcript_storage,
            tool_approval_mode: self.tool_approval_mode,
        }
    }
}

/// Renderer'a gecen config alt kumesi (whitelist — blacklist degil).
///
/// Burada **olmayan** her sey renderer'a gitmez: `OPENAI_API_KEY`,
/// wake word saglayicisi/model dizini/esigi (motor Rust tarafinda calisir,
/// renderer vendor adini gormez — ADR-004).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendConfig {
    pub realtime_model: String,
    pub realtime_voice: Option<String>,
    pub wake_word: String,
    pub idle_timeout_seconds: u32,
    pub log_level: LogLevel,
    pub memory_enabled: bool,
    pub transcript_storage: bool,
    pub tool_approval_mode: ToolApprovalMode,
}

// ---------------------------------------------------------------------------
// Yukleme
// ---------------------------------------------------------------------------

/// `.env` + process environment'tan konfigurasyonu yukler.
///
/// Oncelik: **process environment kazanir**. Gerekce: CI, `launchd` ve ileride
/// macOS Keychain enjeksiyonu dosyayi ezebilmeli; ayrica
/// `OPENAI_API_KEY=... pnpm tauri dev` tek seferlik override'i calismali.
pub fn load() -> Result<AsunaConfig, ConfigError> {
    let mut map = match env_file::find_env_file() {
        Some(path) => env_file::load(&path)?,
        None => EnvMap::new(),
    };

    for key in ALL_KEYS {
        if let Ok(value) = std::env::var(key) {
            map.insert(key.to_owned(), value);
        }
    }

    load_from_map(&map)
}

/// Saf ayristirma/dogrulama — dosya sistemine ve process environment'a dokunmaz.
pub fn load_from_map(map: &EnvMap) -> Result<AsunaConfig, ConfigError> {
    let openai_api_key = SecretString(required_non_empty(map, KEY_OPENAI_API_KEY)?);

    let realtime_model = required_non_empty(map, KEY_REALTIME_MODEL)?;
    if realtime_model.contains(char::is_whitespace) {
        return Err(ConfigError::Invalid {
            key: KEY_REALTIME_MODEL,
            expected: "bosluk icermeyen bir model ID (orn. `gpt-realtime-2.1`)".to_owned(),
        });
    }

    let realtime_voice = match optional(map, KEY_REALTIME_VOICE)? {
        None => None,
        Some(voice) => {
            if !KNOWN_VOICES.contains(&voice.as_str()) {
                return Err(ConfigError::Invalid {
                    key: KEY_REALTIME_VOICE,
                    expected: format!(
                        "sunlardan biri veya bos: {} (bos = SDK varsayilani)",
                        KNOWN_VOICES.join(", ")
                    ),
                });
            }
            Some(voice)
        }
    };

    let wake_word = required_non_empty(map, KEY_WAKE_WORD)?;

    let memory_enabled = required_bool(map, KEY_MEMORY_ENABLED)?;
    let transcript_storage = required_bool(map, KEY_TRANSCRIPT_STORAGE)?;

    let tool_approval_mode_raw = required_non_empty(map, KEY_TOOL_APPROVAL_MODE)?;
    let tool_approval_mode =
        ToolApprovalMode::parse(&tool_approval_mode_raw).ok_or(ConfigError::Invalid {
            key: KEY_TOOL_APPROVAL_MODE,
            expected: "`safe` veya `always`".to_owned(),
        })?;

    let idle_timeout_raw = required_non_empty(map, KEY_IDLE_TIMEOUT_SECONDS)?;
    let idle_timeout_seconds: u32 =
        idle_timeout_raw
            .parse::<u32>()
            .map_err(|_| ConfigError::Invalid {
                key: KEY_IDLE_TIMEOUT_SECONDS,
                expected: format!(
                    "{}-{} arasi tam sayi (saniye)",
                    IDLE_TIMEOUT_RANGE.start(),
                    IDLE_TIMEOUT_RANGE.end()
                ),
            })?;
    if !IDLE_TIMEOUT_RANGE.contains(&idle_timeout_seconds) {
        return Err(ConfigError::Invalid {
            key: KEY_IDLE_TIMEOUT_SECONDS,
            expected: format!(
                "{}-{} arasi tam sayi (saniye)",
                IDLE_TIMEOUT_RANGE.start(),
                IDLE_TIMEOUT_RANGE.end()
            ),
        });
    }

    let log_level_raw = required_non_empty(map, KEY_LOG_LEVEL)?;
    let log_level = LogLevel::parse(&log_level_raw).ok_or(ConfigError::Invalid {
        key: KEY_LOG_LEVEL,
        expected: "`error`, `warn`, `info` veya `debug`".to_owned(),
    })?;

    let provider_raw = required_non_empty(map, KEY_WAKE_WORD_PROVIDER)?;
    let wake_word_provider =
        WakeWordProviderKind::parse(&provider_raw).ok_or(ConfigError::Invalid {
            key: KEY_WAKE_WORD_PROVIDER,
            expected: "`sherpa-kws` veya `fake`".to_owned(),
        })?;

    // Dizinin *varligi* burada kontrol edilmez: model dosyalari uygulamadan
    // bagimsiz indirilebilir ve saglayici init'inde (ASU-022) dogrulanir.
    // Config katmani dosya sistemine bagimli olmadan test edilebilir kalir.
    let wake_word_model_dir = optional(map, KEY_WAKE_WORD_MODEL_DIR)?.map(PathBuf::from);

    let threshold_raw = required_non_empty(map, KEY_WAKE_WORD_THRESHOLD)?;
    let wake_word_threshold: f32 =
        threshold_raw
            .parse::<f32>()
            .map_err(|_| ConfigError::Invalid {
                key: KEY_WAKE_WORD_THRESHOLD,
                expected: "0 (haric) ile 1 (dahil) arasi ondalik sayi".to_owned(),
            })?;
    if !wake_word_threshold.is_finite() || wake_word_threshold <= 0.0 || wake_word_threshold > 1.0 {
        return Err(ConfigError::Invalid {
            key: KEY_WAKE_WORD_THRESHOLD,
            expected: "0 (haric) ile 1 (dahil) arasi ondalik sayi".to_owned(),
        });
    }

    Ok(AsunaConfig {
        openai_api_key,
        realtime_model,
        realtime_voice,
        wake_word,
        memory_enabled,
        transcript_storage,
        tool_approval_mode,
        idle_timeout_seconds,
        log_level,
        wake_word_provider,
        wake_word_model_dir,
        wake_word_threshold,
    })
}

/// Anahtar tanimli olmali ve bos olmamali.
fn required_non_empty(map: &EnvMap, key: &'static str) -> Result<String, ConfigError> {
    let raw = map.get(key).ok_or(ConfigError::Missing { key })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::Empty { key });
    }
    Ok(trimmed.to_owned())
}

/// Anahtar **tanimli olmali** (sessiz default yok) ama bos birakilabilir;
/// bos deger "belirtilmedi" anlamina gelir.
fn optional(map: &EnvMap, key: &'static str) -> Result<Option<String>, ConfigError> {
    let raw = map.get(key).ok_or(ConfigError::Missing { key })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_owned()))
    }
}

fn required_bool(map: &EnvMap, key: &'static str) -> Result<bool, ConfigError> {
    let raw = required_non_empty(map, key)?;
    match raw.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ConfigError::Invalid {
            key,
            expected: "`true` veya `false`".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_API_KEY: &str = "sk-proj-COK-GIZLI-TEST-DEGERI";

    fn valid_map() -> EnvMap {
        let pairs = [
            (KEY_OPENAI_API_KEY, TEST_API_KEY),
            (KEY_REALTIME_MODEL, "gpt-realtime-2.1"),
            (KEY_REALTIME_VOICE, "marin"),
            (KEY_WAKE_WORD, "Hey Asuna"),
            (KEY_MEMORY_ENABLED, "true"),
            (KEY_TRANSCRIPT_STORAGE, "false"),
            (KEY_TOOL_APPROVAL_MODE, "safe"),
            (KEY_IDLE_TIMEOUT_SECONDS, "45"),
            (KEY_LOG_LEVEL, "info"),
            (KEY_WAKE_WORD_PROVIDER, "sherpa-kws"),
            (KEY_WAKE_WORD_MODEL_DIR, ""),
            (KEY_WAKE_WORD_THRESHOLD, "0.25"),
        ];
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }

    fn map_with(key: &str, value: &str) -> EnvMap {
        let mut map = valid_map();
        map.insert(key.to_owned(), value.to_owned());
        map
    }

    fn map_without(key: &str) -> EnvMap {
        let mut map = valid_map();
        map.remove(key);
        map
    }

    #[test]
    fn parses_a_complete_valid_environment() {
        let config = load_from_map(&valid_map()).expect("gecerli config yuklenmeli");

        assert_eq!(config.realtime_model, "gpt-realtime-2.1");
        assert_eq!(config.realtime_voice.as_deref(), Some("marin"));
        assert_eq!(config.wake_word, "Hey Asuna");
        assert!(config.memory_enabled);
        assert!(!config.transcript_storage);
        assert_eq!(config.tool_approval_mode, ToolApprovalMode::Safe);
        assert_eq!(config.idle_timeout_seconds, 45);
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(config.wake_word_provider, WakeWordProviderKind::SherpaKws);
        assert_eq!(config.wake_word_model_dir, None);
        assert!((config.wake_word_threshold - 0.25).abs() < f32::EPSILON);
        assert_eq!(config.openai_api_key().expose(), TEST_API_KEY);
    }

    /// Sessiz default yok: 12 anahtarin **her biri** eksikse acilista hata.
    #[test]
    fn every_key_is_required() {
        for key in ALL_KEYS {
            let Err(error) = load_from_map(&map_without(key)) else {
                panic!("`{key}` eksikken hata bekleniyordu");
            };
            match error {
                ConfigError::Missing { key: missing } => assert_eq!(missing, key),
                ref other => panic!("`{key}` icin Missing bekleniyordu, gelen: {other:?}"),
            }
            let message = error.to_string();
            assert!(message.contains(key), "mesaj anahtari anmiyor: {message}");
        }
    }

    #[test]
    fn api_key_cannot_be_empty() {
        let error =
            load_from_map(&map_with(KEY_OPENAI_API_KEY, "   ")).expect_err("hata bekleniyordu");
        assert!(matches!(
            error,
            ConfigError::Empty {
                key: KEY_OPENAI_API_KEY
            }
        ));
    }

    #[test]
    fn optional_keys_accept_empty_value_as_unset() {
        let mut map = valid_map();
        map.insert(KEY_REALTIME_VOICE.to_owned(), String::new());
        map.insert(KEY_WAKE_WORD_MODEL_DIR.to_owned(), "  ".to_owned());

        let config = load_from_map(&map).expect("bos opsiyonel degerler kabul edilmeli");
        assert_eq!(config.realtime_voice, None);
        assert_eq!(config.wake_word_model_dir, None);
    }

    #[test]
    fn model_dir_is_kept_as_path() {
        let config = load_from_map(&map_with(KEY_WAKE_WORD_MODEL_DIR, "/opt/asuna/kws"))
            .expect("gecerli dizin kabul edilmeli");
        assert_eq!(
            config.wake_word_model_dir,
            Some(PathBuf::from("/opt/asuna/kws"))
        );
    }

    #[test]
    fn rejects_invalid_booleans() {
        for value in ["1", "yes", "evet", "TRUE!", "on"] {
            let Err(error) = load_from_map(&map_with(KEY_MEMORY_ENABLED, value)) else {
                panic!("`{value}` reddedilmeliydi");
            };
            assert!(matches!(
                error,
                ConfigError::Invalid {
                    key: KEY_MEMORY_ENABLED,
                    ..
                }
            ));
        }
    }

    #[test]
    fn accepts_case_insensitive_booleans() {
        let config =
            load_from_map(&map_with(KEY_TRANSCRIPT_STORAGE, "TRUE")).expect("TRUE kabul edilmeli");
        assert!(config.transcript_storage);
    }

    #[test]
    fn rejects_unknown_log_level() {
        let error =
            load_from_map(&map_with(KEY_LOG_LEVEL, "verbose")).expect_err("hata bekleniyordu");
        assert!(matches!(
            error,
            ConfigError::Invalid {
                key: KEY_LOG_LEVEL,
                ..
            }
        ));
    }

    #[test]
    fn rejects_unknown_tool_approval_mode() {
        // "never" gibi bir mod bilerek yok: onay kapatilamaz.
        let error = load_from_map(&map_with(KEY_TOOL_APPROVAL_MODE, "never"))
            .expect_err("hata bekleniyordu");
        assert!(matches!(
            error,
            ConfigError::Invalid {
                key: KEY_TOOL_APPROVAL_MODE,
                ..
            }
        ));
    }

    #[test]
    fn rejects_unknown_wake_word_provider() {
        let error = load_from_map(&map_with(KEY_WAKE_WORD_PROVIDER, "porcupine"))
            .expect_err("hata bekleniyordu");
        assert!(matches!(
            error,
            ConfigError::Invalid {
                key: KEY_WAKE_WORD_PROVIDER,
                ..
            }
        ));
    }

    #[test]
    fn rejects_unknown_voice() {
        let error =
            load_from_map(&map_with(KEY_REALTIME_VOICE, "asuna")).expect_err("hata bekleniyordu");
        assert!(matches!(
            error,
            ConfigError::Invalid {
                key: KEY_REALTIME_VOICE,
                ..
            }
        ));
    }

    #[test]
    fn rejects_model_with_whitespace() {
        let error = load_from_map(&map_with(KEY_REALTIME_MODEL, "gpt realtime"))
            .expect_err("hata bekleniyordu");
        assert!(matches!(
            error,
            ConfigError::Invalid {
                key: KEY_REALTIME_MODEL,
                ..
            }
        ));
    }

    #[test]
    fn rejects_out_of_range_or_non_numeric_idle_timeout() {
        for value in ["0", "4", "1801", "-5", "abc", "45.5"] {
            let Err(error) = load_from_map(&map_with(KEY_IDLE_TIMEOUT_SECONDS, value)) else {
                panic!("`{value}` reddedilmeliydi");
            };
            assert!(matches!(
                error,
                ConfigError::Invalid {
                    key: KEY_IDLE_TIMEOUT_SECONDS,
                    ..
                }
            ));
        }
    }

    #[test]
    fn rejects_out_of_range_or_non_finite_threshold() {
        for value in ["0", "0.0", "-0.5", "1.01", "abc", "NaN", "inf"] {
            let Err(error) = load_from_map(&map_with(KEY_WAKE_WORD_THRESHOLD, value)) else {
                panic!("`{value}` reddedilmeliydi");
            };
            assert!(matches!(
                error,
                ConfigError::Invalid {
                    key: KEY_WAKE_WORD_THRESHOLD,
                    ..
                }
            ));
        }
    }

    #[test]
    fn accepts_threshold_upper_bound() {
        let config =
            load_from_map(&map_with(KEY_WAKE_WORD_THRESHOLD, "1.0")).expect("1.0 gecerli olmali");
        assert!((config.wake_word_threshold - 1.0).abs() < f32::EPSILON);
    }

    // --- Guvenlik testleri (CLAUDE.md: guvenlik mantigi test edilmeden merge edilmez) ---

    /// `Debug` ciktisi API key'i sizdirmaz — log/panic mesajlari bu tipi basabilir.
    #[test]
    fn debug_output_redacts_the_api_key() {
        let config = load_from_map(&valid_map()).expect("gecerli config yuklenmeli");
        let debug = format!("{config:?}");
        assert!(!debug.contains(TEST_API_KEY), "debug ciktisi: {debug}");
        assert!(debug.contains("<redacted>"));
    }

    /// Hicbir hata mesaji deger sizdirmaz (gecersiz deger secret olabilir).
    #[test]
    fn error_messages_never_contain_values() {
        // `ASUNA_REALTIME_MODEL` icin bosluk eklendi: aksi halde deger
        // sozdizimsel olarak gecerli bir model ID sayilir ve hata uretmez.
        let model_value = format!("{TEST_API_KEY} bosluklu");
        let cases = [
            (KEY_OPENAI_API_KEY, "   "),
            (KEY_REALTIME_MODEL, model_value.as_str()),
            (KEY_REALTIME_VOICE, TEST_API_KEY),
            (KEY_MEMORY_ENABLED, TEST_API_KEY),
            (KEY_TOOL_APPROVAL_MODE, TEST_API_KEY),
            (KEY_IDLE_TIMEOUT_SECONDS, TEST_API_KEY),
            (KEY_LOG_LEVEL, TEST_API_KEY),
            (KEY_WAKE_WORD_PROVIDER, TEST_API_KEY),
            (KEY_WAKE_WORD_THRESHOLD, TEST_API_KEY),
        ];
        for (key, value) in cases {
            let Err(error) = load_from_map(&map_with(key, value)) else {
                panic!("`{key}` icin hata bekleniyordu");
            };
            let message = error.to_string();
            assert!(
                !message.contains(TEST_API_KEY),
                "`{key}` hatasi degeri sizdirdi: {message}"
            );
            assert!(
                message.contains(key),
                "`{key}` hatasi anahtari anmiyor: {message}"
            );
        }
    }

    /// Whitelist testi: frontend'e giden JSON **tam olarak** 8 alan icerir ve
    /// API key hicbir bicimde icinde degildir.
    #[test]
    fn frontend_config_exposes_only_the_whitelisted_fields() {
        let config = load_from_map(&valid_map()).expect("gecerli config yuklenmeli");
        let json = serde_json::to_value(config.to_frontend()).expect("serialize edilebilmeli");

        let object = json.as_object().expect("JSON nesnesi bekleniyordu");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "idleTimeoutSeconds",
                "logLevel",
                "memoryEnabled",
                "realtimeModel",
                "realtimeVoice",
                "toolApprovalMode",
                "transcriptStorage",
                "wakeWord",
            ]
        );

        let serialized = json.to_string();
        assert!(!serialized.contains(TEST_API_KEY), "JSON: {serialized}");
        assert!(
            !serialized.to_lowercase().contains("apikey"),
            "JSON: {serialized}"
        );
        // Wake word motoru detaylari da renderer'a gitmez (ADR-004).
        assert!(!serialized.contains("sherpa"), "JSON: {serialized}");
    }

    #[test]
    fn frontend_config_serializes_enums_as_lowercase_strings() {
        let mut map = valid_map();
        map.insert(KEY_LOG_LEVEL.to_owned(), "debug".to_owned());
        map.insert(KEY_TOOL_APPROVAL_MODE.to_owned(), "always".to_owned());
        map.insert(KEY_REALTIME_VOICE.to_owned(), String::new());

        let config = load_from_map(&map).expect("gecerli config yuklenmeli");
        let json = serde_json::to_value(config.to_frontend()).expect("serialize edilebilmeli");

        assert_eq!(json["logLevel"], "debug");
        assert_eq!(json["toolApprovalMode"], "always");
        assert!(json["realtimeVoice"].is_null());
    }
}

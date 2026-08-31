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
pub const KEY_SUMMARY_MODEL: &str = "ASUNA_SUMMARY_MODEL";
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
pub const KEY_TURN_DETECTION: &str = "ASUNA_TURN_DETECTION";
pub const KEY_VAD_EAGERNESS: &str = "ASUNA_VAD_EAGERNESS";
pub const KEY_VAD_SILENCE_MS: &str = "ASUNA_VAD_SILENCE_MS";
/// ASU-052: `open_project` tool'unun calistiracagi editor komutu.
pub const KEY_EDITOR_COMMAND: &str = "ASUNA_EDITOR_COMMAND";

/// Konfigurasyonu olusturan tum anahtarlar. Process environment sadece bu
/// listedeki anahtarlar icin okunur — alakasiz degiskenler config'e sizmaz.
pub const ALL_KEYS: [&str; 17] = [
    KEY_OPENAI_API_KEY,
    KEY_REALTIME_MODEL,
    KEY_SUMMARY_MODEL,
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
    KEY_TURN_DETECTION,
    KEY_VAD_EAGERNESS,
    KEY_VAD_SILENCE_MS,
    KEY_EDITOR_COMMAND,
];

/// `ASUNA_EDITOR_COMMAND` bos birakildiginda kullanilan komut.
///
/// Model ID'lerinin aksine bir varsayilan **kabul edilebilir**: burada
/// "sessiz default" bir maliyet ya da gizlilik karari uretmiyor (ASU-033
/// LOW-11 gerekcesi bu anahtar icin gecerli degil), ve degerin ne oldugu
/// `open_project` ciktisinda + audit satirinda gorunur. Anahtarin **tanimli**
/// olmasi yine de sart: `.env.example` icinde durur ve kullanici hangi
/// programin acilacagini gorur.
pub const DEFAULT_EDITOR_COMMAND: &str = "code";

/// `ASUNA_EDITOR_COMMAND` icin karakter tavani. Bir program adi; yol da olsa
/// `PATH_MAX` sinifinin altinda kalir.
const MAX_EDITOR_COMMAND_CHARS: usize = 256;

/// Editor komutunda **yasak** karakterler.
///
/// Komut hicbir zaman bir kabuga verilmiyor (`projects::editor` arguman
/// vektoru kullanir), yani bunlar calistirilamaz. Yasak olmalarinin sebebi
/// baska: `code --wait` gibi bir deger yazan kullanici, sessizce "code --wait"
/// **adinda** bir dosya aranmasini degil, net bir hata gormeli. Bosluk ve
/// metakarakterleri acilista reddetmek bu belirsizligi kapatir.
const FORBIDDEN_EDITOR_CHARS: [char; 15] = [
    ' ', '\t', '\n', '\r', ';', '&', '|', '`', '$', '(', ')', '<', '>', '"', '\'',
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

/// `ASUNA_VAD_SILENCE_MS` icin kabul edilen aralik (ASU-064).
/// Alt sinir: 100 ms altinda sunucu her nefes arasini "konusma bitti" sayar.
/// Ust sinir: 2000 ms zaten fark edilir bir gecikme — daha yukarisi ayar degil ariza.
/// Yalnizca `server_vad` modunda kullanilir; `semantic_vad` sessizligi model ile karar verir.
const VAD_SILENCE_RANGE: std::ops::RangeInclusive<u32> = 100..=2000;

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
    /// Sarmalayiciyi kurar. Kaynak yalnizca guvenilir process olabilir
    /// (`.env` / process environment / ileride Keychain) — renderer'dan gelen
    /// bir deger buraya girmez.
    pub fn new(value: String) -> Self {
        Self(value)
    }

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

/// Realtime tur tespiti (turn detection) yontemi — ASU-064.
///
/// Renderer'a gider: oturum config'ini `RealtimeSession`'a veren taraf renderer
/// (voice.md Bolum 7). Vendor detayi degil, davranis ayari.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnDetectionKind {
    /// Model "konusma bitti mi" kararini anlamdan verir (varsayilan).
    SemanticVad,
    /// Klasik sessizlik esigi; `ASUNA_VAD_SILENCE_MS` ile ayarlanir.
    ServerVad,
}

impl TurnDetectionKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "semantic_vad" => Some(Self::SemanticVad),
            "server_vad" => Some(Self::ServerVad),
            _ => None,
        }
    }
}

/// `semantic_vad` icin "konusma bitti" kararinin acikgozlulugu (ASU-064).
///
/// Yuksek deger = karar daha erken = daha dusuk gecikme, ama Turkce'de tumce
/// ortasindaki duraklamalarda erken kesme riski. Dusuk deger tersi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VadEagerness {
    Auto,
    Low,
    Medium,
    High,
}

impl VadEagerness {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto" => Some(Self::Auto),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// Wake word saglayicisi secimi — `WakeWordProvider` adapter'inin hangi somut
/// implementasyonunun kurulacagi (ADR-004, ASU-021).
///
/// Renderer'a **gider**: adapter'i renderer kurar
/// (`src/asuna/audio/wake-word-provider-factory.ts`), dolayisiyla hangisinin
/// kurulacagini bilmek zorunda. Bu bir secret degil, bir davranis ayaridir.
/// Motorun **detaylari** (model dizini, esik, keyword dosyasi) renderer'a
/// gitmez ve gitmemeli: ses isleme tamamen bu process'te kalir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
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
    /// Oturum ozeti uretiminde kullanilan metin modeli (ASU-033).
    ///
    /// Renderer'a **gitmez**: ozet cagrisini bu process yapar (kalici API key
    /// burada), dolayisiyla webview'in bu model ID'sini bilmesi icin bir neden
    /// yok. `to_frontend` whitelist'ine eklenmemesi bilincli bir karardir.
    pub summary_model: String,
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
    pub turn_detection: TurnDetectionKind,
    pub vad_eagerness: VadEagerness,
    /// Yalnizca `turn_detection == ServerVad` iken anlamli; deger her durumda
    /// dogrulanir ki mod degistirmek yeniden yapilandirma gerektirmesin.
    pub vad_silence_ms: u32,
    /// `open_project` tool'unun calistiracagi editor komutu (ASU-052).
    ///
    /// Renderer'a **gitmez**: alt process'i bu taraf baslatir, webview'in hangi
    /// programin calistigini bilmesine gerek yok ve bilmesi ona bir secim hakki
    /// oneririr gibi gorunurdu. Komut adi kullaniciya `open_project` ciktisinda
    /// ve audit satirinda gorunur.
    pub editor_command: String,
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
            wake_word_provider: self.wake_word_provider,
            idle_timeout_seconds: self.idle_timeout_seconds,
            log_level: self.log_level,
            memory_enabled: self.memory_enabled,
            transcript_storage: self.transcript_storage,
            tool_approval_mode: self.tool_approval_mode,
            turn_detection: self.turn_detection,
            vad_eagerness: self.vad_eagerness,
            vad_silence_ms: self.vad_silence_ms,
        }
    }
}

/// Renderer'a gecen config alt kumesi (whitelist — blacklist degil).
///
/// Burada **olmayan** her sey renderer'a gitmez: `OPENAI_API_KEY`, wake word
/// model dizini ve esigi (motor Rust tarafinda calisir; renderer ses karesi
/// gormez, yalnizca tespit olayini alir — ADR-004).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendConfig {
    pub realtime_model: String,
    pub realtime_voice: Option<String>,
    pub wake_word: String,
    /// ASU-021: hangi `WakeWordProvider` adapter'inin kurulacagi. Yalnizca secim;
    /// motor detayi degil.
    pub wake_word_provider: WakeWordProviderKind,
    pub idle_timeout_seconds: u32,
    pub log_level: LogLevel,
    pub memory_enabled: bool,
    pub transcript_storage: bool,
    pub tool_approval_mode: ToolApprovalMode,
    /// ASU-064: tur tespiti ayarlari renderer'a acilir cunku `RealtimeSession`
    /// config'ini renderer kurar (voice.md Bolum 7). Secret icermez.
    pub turn_detection: TurnDetectionKind,
    pub vad_eagerness: VadEagerness,
    pub vad_silence_ms: u32,
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
    let openai_api_key = SecretString::new(required_non_empty(map, KEY_OPENAI_API_KEY)?);

    let realtime_model = model_id(map, KEY_REALTIME_MODEL, "gpt-realtime-2.1")?;
    let summary_model = model_id(map, KEY_SUMMARY_MODEL, "gpt-4o-mini")?;

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

    // Bos = varsayilan (`code`). Anahtar yine de tanimli olmali: hangi
    // programin acilacagi `.env`'de gorunur kalsin.
    let editor_command = match optional(map, KEY_EDITOR_COMMAND)? {
        None => DEFAULT_EDITOR_COMMAND.to_owned(),
        Some(command) => {
            if command.chars().count() > MAX_EDITOR_COMMAND_CHARS {
                return Err(ConfigError::Invalid {
                    key: KEY_EDITOR_COMMAND,
                    expected: format!("en fazla {MAX_EDITOR_COMMAND_CHARS} karakter"),
                });
            }
            if command.contains(FORBIDDEN_EDITOR_CHARS) || command.contains('\0') {
                return Err(ConfigError::Invalid {
                    key: KEY_EDITOR_COMMAND,
                    expected: "argumansiz tek bir komut adi ya da tam yol (orn. `code`); \
                               bosluk ve kabuk karakterleri kabul edilmiyor"
                        .to_owned(),
                });
            }
            command
        }
    };

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

    // --- Tur tespiti (ASU-064) ---------------------------------------------
    let turn_detection_raw = required_non_empty(map, KEY_TURN_DETECTION)?;
    let turn_detection =
        TurnDetectionKind::parse(&turn_detection_raw).ok_or(ConfigError::Invalid {
            key: KEY_TURN_DETECTION,
            expected: "`semantic_vad` veya `server_vad`".to_owned(),
        })?;

    let eagerness_raw = required_non_empty(map, KEY_VAD_EAGERNESS)?;
    let vad_eagerness = VadEagerness::parse(&eagerness_raw).ok_or(ConfigError::Invalid {
        key: KEY_VAD_EAGERNESS,
        expected: "`auto`, `low`, `medium` veya `high`".to_owned(),
    })?;

    let silence_expected = || {
        format!(
            "{}-{} arasi tam sayi (milisaniye)",
            VAD_SILENCE_RANGE.start(),
            VAD_SILENCE_RANGE.end()
        )
    };
    let silence_raw = required_non_empty(map, KEY_VAD_SILENCE_MS)?;
    let vad_silence_ms: u32 = silence_raw
        .parse::<u32>()
        .map_err(|_| ConfigError::Invalid {
            key: KEY_VAD_SILENCE_MS,
            expected: silence_expected(),
        })?;
    if !VAD_SILENCE_RANGE.contains(&vad_silence_ms) {
        return Err(ConfigError::Invalid {
            key: KEY_VAD_SILENCE_MS,
            expected: silence_expected(),
        });
    }

    Ok(AsunaConfig {
        openai_api_key,
        realtime_model,
        summary_model,
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
        turn_detection,
        vad_eagerness,
        vad_silence_ms,
        editor_command,
    })
}

/// Bosluk icermeyen bir model kimligi.
///
/// Ayni kural iki anahtar icin de gecerli: model ID hicbir yerde hard-code
/// edilmez, ama yanlis yazilmis bir deger de sessizce API'ye gonderilmez.
fn model_id(map: &EnvMap, key: &'static str, example: &str) -> Result<String, ConfigError> {
    let value = required_non_empty(map, key)?;
    if value.contains(char::is_whitespace) {
        return Err(ConfigError::Invalid {
            key,
            expected: format!("bosluk icermeyen bir model ID (orn. `{example}`)"),
        });
    }
    Ok(value)
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
            (KEY_SUMMARY_MODEL, "gpt-4o-mini"),
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
            (KEY_TURN_DETECTION, "semantic_vad"),
            (KEY_VAD_EAGERNESS, "high"),
            (KEY_VAD_SILENCE_MS, "400"),
            (KEY_EDITOR_COMMAND, "code"),
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
        assert_eq!(config.summary_model, "gpt-4o-mini");
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
        assert_eq!(config.turn_detection, TurnDetectionKind::SemanticVad);
        assert_eq!(config.vad_eagerness, VadEagerness::High);
        assert_eq!(config.vad_silence_ms, 400);
        assert_eq!(config.openai_api_key().expose(), TEST_API_KEY);
    }

    /// Sessiz default yok: anahtarlarin **her biri** eksikse acilista hata.
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

    /// ASU-052: bos deger "belirtilmedi" demek ve varsayilana duser. Anahtarin
    /// **tanimli** olmasi yine sart (`every_key_is_required` ayrica olcuyor).
    #[test]
    fn an_empty_editor_command_falls_back_to_the_default() {
        let config =
            load_from_map(&map_with(KEY_EDITOR_COMMAND, "   ")).expect("bos deger kabul edilmeli");
        assert_eq!(config.editor_command, DEFAULT_EDITOR_COMMAND);
        assert_eq!(config.editor_command, "code");
    }

    #[test]
    fn an_absolute_editor_path_is_accepted() {
        let config = load_from_map(&map_with(KEY_EDITOR_COMMAND, "/usr/local/bin/code"))
            .expect("tam yol kabul edilmeli");
        assert_eq!(config.editor_command, "/usr/local/bin/code");
    }

    /// **ASU-052 guvenlik kilidi**: komut hicbir zaman bir kabuga verilmiyor,
    /// bu yuzden metakarakterler calisamaz — ama sessizce "boyle bir dosya yok"
    /// hatasina donusmemeleri icin acilista reddediliyor. Argumanli bir editor
    /// komutu (`code --wait`) da bu kurala takilir ve kullanici net bir mesaj
    /// gorur.
    #[test]
    fn an_editor_command_with_shell_characters_is_refused_at_startup() {
        for value in [
            "code --wait",
            "code; rm -rf ~",
            "code && open .",
            "code | tee /tmp/x",
            "`whoami`",
            "$(id)",
            "code > /tmp/out",
            "\"code\"",
        ] {
            let Err(error) = load_from_map(&map_with(KEY_EDITOR_COMMAND, value)) else {
                panic!("`{value}` reddedilmeliydi");
            };
            assert!(
                matches!(
                    error,
                    ConfigError::Invalid {
                        key: KEY_EDITOR_COMMAND,
                        ..
                    }
                ),
                "`{value}` icin Invalid bekleniyordu, gelen: {error:?}"
            );
        }
    }

    #[test]
    fn an_over_long_editor_command_is_refused() {
        let error = load_from_map(&map_with(KEY_EDITOR_COMMAND, &"c".repeat(300)))
            .expect_err("hata bekleniyordu");
        assert!(matches!(
            error,
            ConfigError::Invalid {
                key: KEY_EDITOR_COMMAND,
                ..
            }
        ));
    }

    /// Editor komutu renderer'a **gitmez**: `FrontendConfig` whitelist'i onu
    /// tasimaz. Webview'in hangi programin acildigini bilmesine gerek yok.
    #[test]
    fn the_editor_command_is_not_exposed_to_the_renderer() {
        let config = load_from_map(&map_with(KEY_EDITOR_COMMAND, "cursor"))
            .expect("gecerli config yuklenmeli");
        let json = serde_json::to_string(&config.to_frontend()).expect("serialize");
        assert!(
            !json.contains("cursor"),
            "editor komutu renderer'a sizdi: {json}"
        );
        assert!(
            !json.contains("editorCommand"),
            "alan renderer'a sizdi: {json}"
        );
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
        // Iki model anahtari da ayni kurala tabi (ASU-033).
        for key in [KEY_REALTIME_MODEL, KEY_SUMMARY_MODEL] {
            let Err(error) = load_from_map(&map_with(key, "gpt realtime")) else {
                panic!("`{key}` icin hata bekleniyordu");
            };
            match error {
                ConfigError::Invalid { key: invalid, .. } => assert_eq!(invalid, key),
                other => panic!("`{key}` icin Invalid bekleniyordu, gelen: {other:?}"),
            }
        }
    }

    /// GUVENLIK: ozet modeli renderer'a **gitmez**. Ozet cagrisini kalici API
    /// key'in yasadigi process yapar; webview'in bu ID'yi bilmesi gerekmez.
    #[test]
    fn summary_model_stays_in_the_trusted_process() {
        let config = load_from_map(&map_with(KEY_SUMMARY_MODEL, "gpt-ozet-gizli"))
            .expect("gecerli config yuklenmeli");
        let json = serde_json::to_value(config.to_frontend()).expect("serialize edilebilmeli");

        let serialized = json.to_string();
        assert!(!serialized.contains("gpt-ozet-gizli"), "JSON: {serialized}");
        assert!(
            !serialized.to_lowercase().contains("summary"),
            "JSON: {serialized}"
        );
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

    // --- Tur tespiti (ASU-064) ---------------------------------------------

    #[test]
    fn accepts_every_valid_eagerness_level() {
        for (raw, expected) in [
            ("auto", VadEagerness::Auto),
            ("low", VadEagerness::Low),
            ("medium", VadEagerness::Medium),
            ("high", VadEagerness::High),
        ] {
            let config = load_from_map(&map_with(KEY_VAD_EAGERNESS, raw))
                .unwrap_or_else(|error| panic!("`{raw}` kabul edilmeliydi: {error}"));
            assert_eq!(config.vad_eagerness, expected);
        }
    }

    #[test]
    fn rejects_unknown_eagerness() {
        // `HIGH`/`aggressive` sessizce `high`a dusmez: yanlis yazim acilista patlar.
        for value in ["HIGH", "aggressive", "fast", "1", ""] {
            let Err(error) = load_from_map(&map_with(KEY_VAD_EAGERNESS, value)) else {
                panic!("`{value}` reddedilmeliydi");
            };
            assert!(matches!(
                error,
                ConfigError::Invalid {
                    key: KEY_VAD_EAGERNESS,
                    ..
                } | ConfigError::Empty {
                    key: KEY_VAD_EAGERNESS
                }
            ));
        }
    }

    #[test]
    fn accepts_both_turn_detection_kinds() {
        for (raw, expected) in [
            ("semantic_vad", TurnDetectionKind::SemanticVad),
            ("server_vad", TurnDetectionKind::ServerVad),
        ] {
            let config = load_from_map(&map_with(KEY_TURN_DETECTION, raw))
                .unwrap_or_else(|error| panic!("`{raw}` kabul edilmeliydi: {error}"));
            assert_eq!(config.turn_detection, expected);
        }
    }

    #[test]
    fn rejects_unknown_turn_detection() {
        // `null` bilerek yok: tur yonetimini uygulamaya devretmek istenmiyor (voice.md 7).
        for value in ["null", "none", "semantic", "vad"] {
            let Err(error) = load_from_map(&map_with(KEY_TURN_DETECTION, value)) else {
                panic!("`{value}` reddedilmeliydi");
            };
            assert!(matches!(
                error,
                ConfigError::Invalid {
                    key: KEY_TURN_DETECTION,
                    ..
                }
            ));
        }
    }

    #[test]
    fn rejects_out_of_range_or_non_numeric_vad_silence() {
        for value in ["0", "99", "2001", "-100", "abc", "400.5"] {
            let Err(error) = load_from_map(&map_with(KEY_VAD_SILENCE_MS, value)) else {
                panic!("`{value}` reddedilmeliydi");
            };
            assert!(matches!(
                error,
                ConfigError::Invalid {
                    key: KEY_VAD_SILENCE_MS,
                    ..
                }
            ));
        }
    }

    #[test]
    fn accepts_vad_silence_bounds() {
        for value in ["100", "2000"] {
            let config = load_from_map(&map_with(KEY_VAD_SILENCE_MS, value))
                .unwrap_or_else(|error| panic!("`{value}` kabul edilmeliydi: {error}"));
            assert_eq!(config.vad_silence_ms.to_string(), value);
        }
    }

    /// Sessizlik suresi `semantic_vad` modunda kullanilmasa da dogrulanir:
    /// mod degistirmek (server_vad'e gecmek) ikinci bir duzeltme gerektirmemeli.
    #[test]
    fn vad_silence_is_validated_even_in_semantic_mode() {
        let mut map = valid_map();
        map.insert(KEY_TURN_DETECTION.to_owned(), "semantic_vad".to_owned());
        map.insert(KEY_VAD_SILENCE_MS.to_owned(), "5000".to_owned());

        let Err(error) = load_from_map(&map) else {
            panic!("aralik disi sessizlik suresi reddedilmeliydi");
        };
        assert!(matches!(
            error,
            ConfigError::Invalid {
                key: KEY_VAD_SILENCE_MS,
                ..
            }
        ));
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
            (KEY_SUMMARY_MODEL, model_value.as_str()),
            (KEY_REALTIME_VOICE, TEST_API_KEY),
            (KEY_MEMORY_ENABLED, TEST_API_KEY),
            (KEY_TOOL_APPROVAL_MODE, TEST_API_KEY),
            (KEY_IDLE_TIMEOUT_SECONDS, TEST_API_KEY),
            (KEY_LOG_LEVEL, TEST_API_KEY),
            (KEY_WAKE_WORD_PROVIDER, TEST_API_KEY),
            (KEY_WAKE_WORD_THRESHOLD, TEST_API_KEY),
            (KEY_TURN_DETECTION, TEST_API_KEY),
            (KEY_VAD_EAGERNESS, TEST_API_KEY),
            (KEY_VAD_SILENCE_MS, TEST_API_KEY),
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

    /// Whitelist testi: frontend'e giden JSON **tam olarak** 11 alan icerir ve
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
                "turnDetection",
                "vadEagerness",
                "vadSilenceMs",
                "wakeWord",
                "wakeWordProvider",
            ]
        );

        let serialized = json.to_string();
        assert!(!serialized.contains(TEST_API_KEY), "JSON: {serialized}");
        assert!(
            !serialized.to_lowercase().contains("apikey"),
            "JSON: {serialized}"
        );
    }

    /// ASU-021 / ADR-004: saglayici **secimi** renderer'a gider (adapter'i o kurar),
    /// motor **detaylari** gitmez — model dizini ve esik bu process'te kalir.
    #[test]
    fn frontend_config_exposes_the_provider_choice_but_not_the_engine_details() {
        const MODEL_DIR: &str = "/opt/asuna/kws-model-dizini";

        let mut map = valid_map();
        map.insert(KEY_WAKE_WORD_MODEL_DIR.to_owned(), MODEL_DIR.to_owned());
        map.insert(KEY_WAKE_WORD_THRESHOLD.to_owned(), "0.77".to_owned());

        let config = load_from_map(&map).expect("gecerli config yuklenmeli");
        let json = serde_json::to_value(config.to_frontend()).expect("serialize edilebilmeli");

        assert_eq!(json["wakeWordProvider"], "sherpa-kws");

        let serialized = json.to_string();
        assert!(!serialized.contains(MODEL_DIR), "JSON: {serialized}");
        assert!(!serialized.contains("0.77"), "JSON: {serialized}");
        assert!(
            !serialized.to_lowercase().contains("threshold"),
            "JSON: {serialized}"
        );
    }

    #[test]
    fn frontend_config_serializes_the_fake_provider_as_kebab_case() {
        let config = load_from_map(&map_with(KEY_WAKE_WORD_PROVIDER, "fake"))
            .expect("gecerli config yuklenmeli");
        let json = serde_json::to_value(config.to_frontend()).expect("serialize edilebilmeli");

        assert_eq!(json["wakeWordProvider"], "fake");
    }

    #[test]
    fn frontend_config_serializes_enums_as_lowercase_strings() {
        let mut map = valid_map();
        map.insert(KEY_LOG_LEVEL.to_owned(), "debug".to_owned());
        map.insert(KEY_TOOL_APPROVAL_MODE.to_owned(), "always".to_owned());
        map.insert(KEY_REALTIME_VOICE.to_owned(), String::new());
        map.insert(KEY_TURN_DETECTION.to_owned(), "server_vad".to_owned());
        map.insert(KEY_VAD_EAGERNESS.to_owned(), "low".to_owned());

        let config = load_from_map(&map).expect("gecerli config yuklenmeli");
        let json = serde_json::to_value(config.to_frontend()).expect("serialize edilebilmeli");

        assert_eq!(json["logLevel"], "debug");
        assert_eq!(json["toolApprovalMode"], "always");
        assert!(json["realtimeVoice"].is_null());
        // Renderer'in gordugu bicim TS whitelist'iyle birebir ayni olmali
        // (`src/asuna/config/frontend-config.ts`).
        assert_eq!(json["turnDetection"], "server_vad");
        assert_eq!(json["vadEagerness"], "low");
        assert_eq!(json["vadSilenceMs"], 400);
    }
}

//! Metin sohbeti — komut katmani (Chat Shell, `asuna-plans/plan-chat-shell.md` WP2).
//!
//! # Neden model cagrisi burada
//!
//! Kalici `OPENAI_API_KEY` yalnizca bu process'te yasar (`config.rs`), yani
//! sohbet cagrisini renderer yapamaz. `summary.rs` ile ayni disiplin:
//!
//! - Anahtar `Authorization` header'i disinda hicbir yere yazilmaz; hicbir hata
//!   varyanti API govdesi, header ya da anahtar tasimaz ve IPC'ye giden mesaj
//!   yine de [`redact_secrets`] suzgecinden gecer.
//! - Yonlendirme kapali (`redirect::none`): `Authorization` tanimadigimiz bir
//!   host'a tasinamaz.
//! - Model ID hard-code **edilmez**: `ASUNA_CHAT_MODEL` tek noktadan okunur.
//! - Renderer prompt insa etmez, gecmisi secmez, model secmez. Yalnizca
//!   "su konusmaya su metni gonder" der.
//!
//! # Yazma neden tek transaction
//!
//! Kullanici mesaji ve asistan yaniti **birlikte** yazilir
//! ([`message_repository::append_in_tx`]). Iki ayri yazma arasinda uygulama
//! kapanirsa konusmada cevapsiz bir kullanici mesaji kalirdi; ekler de ayni
//! transaction icinde kullanici mesajina baglanir.
//!
//! # Redaksiyon nerede
//!
//! Dosya icerigi DB'ye girmeden **once** redakte edilir
//! ([`attachment_ingest`], ve proje yolunda `projects::files::read`); yani
//! modele giden metin de, `attachments.content` icinde saklanan metin de zaten
//! maskelenmis olandir.
//!
//! Kullanicinin **yazdigi** mesaj ve modelin **dondurdugu** yanit bilerek
//! redakte edilmez: bunlar konusmanin kendisidir ve ekranda gorunen metnin
//! saklanan metinden farkli olmasi, "Asuna boyle demedi" durumunu uretirdi.
//! Ozet/hafiza yolundaki (`summary.rs`) kural burada gecerli degil cunku orada
//! metin **turetilmis** bir kayittir, konusmanin kendisi degil.

use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::config::{AsunaConfig, SecretString};
use crate::db::attachment_repository::{self, AttachmentDraft, AttachmentPayload};
use crate::db::model::{AttachmentOrigin, AttachmentRecord, MessageRecord, MessageRole};
use crate::db::model::{ProjectRecord, SessionModality, SessionRecord};
use crate::db::store_error::database;
use crate::db::{clock, message_repository, project_repository, session_repository};
use crate::db::{AsunaDb, DbState, StoreError, StoreErrorCode};
use crate::privacy::PrivacyState;
use crate::projects::files::{self, ProjectFileError};
use crate::projects::registry;
use crate::realtime_token::NetworkCause;
use crate::redaction::{redact_secrets, redact_sensitive_text};
use crate::security::blocklist;

/// OpenAI Chat Completions endpoint'i (`summary.rs` ile ayni yuzey).
pub const CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Sohbet cagrisinin ust siniri. Kullanici **burada bekliyor** (ozetin aksine),
/// ama sonsuza kadar da beklememeli.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// TCP + TLS el sikismasi icin ayri, daha kisa sinir.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Modele tasinan gecmis mesaj sayisi (plan-chat-shell.md WP2).
///
/// Konusmanin tamami degil: uzun bir konusma her turda butun gecmisi yeniden
/// gonderirse maliyet konusma uzunlugunun karesi gibi buyur. Kirpma **sondan**
/// tutulur (en yeni mesajlar) ve modele soylenmez — model gordugu kadarina
/// gore cevap verir, "konusmanin basi buydu" gibi bir iddiada bulunmaz.
pub const MAX_HISTORY_MESSAGES: u32 = 40;

/// Gecmisin karakter butcesi. Mesaj sayisi tavani tek basina yetmez: 40 uzun
/// mesaj baglam penceresini tek basina doldurabilir.
const MAX_HISTORY_CHARS: usize = 24_000;

/// Renderer'in [`attachment_ingest`]'e gonderebilecegi azami karakter.
///
/// Bu **girdi** tavani: asan dosya kirpilmaz, reddedilir. Kirpma karari
/// kullaniciya gorunur olmali ve 200.000 karakterlik bir dosyanin sohbete
/// eklenmesi zaten baska bir isin (arama/indeksleme) konusu.
pub const MAX_INGEST_CHARS: usize = 200_000;

/// `attachments.content` icine yazilan azami karakter.
///
/// Semadaki tavan (32.000) bunun **ikinci** katmanidir; kirpma burada, gorunur
/// bir isaretle yapilir.
pub const MAX_STORED_ATTACHMENT_CHARS: usize = 24_000;

/// Tek bir istekte tum eklerin modele tasiyabilecegi toplam karakter.
const MAX_ATTACHMENT_PROMPT_CHARS: usize = 24_000;

/// Kirpilan metnin sonuna eklenen isaret. Kirpma **sessiz degildir**.
pub const TRUNCATION_NOTICE: &str = "\n[... kirpildi ...]";

/// Butceye sigmayan ekler icin prompt'a dusen not.
const ATTACHMENT_BUDGET_NOTICE: &str = "\n\n[... kalan ekler baglam butcesine sigmadi ...]";

/// Kontrol karakteri yogunlugu bu orani asarsa icerik "metin degil" sayilir
/// (yuzde bir). Tek bir kacik bayt yuzunden dosya reddedilmesin, ama ikili bir
/// dosya da "metin" diye kaydedilmesin.
const MAX_CONTROL_CHAR_PERMILLE: usize = 10;

/// Sohbet talimati — **versiyonlu** (`summary.rs` ile ayni gerekce: hangi
/// oturumun hangi talimatla konustugu izlenebilir kalsin).
pub const CHAT_SYSTEM_PROMPT_V1: &str = "\
Sen Asuna'sin: kullanicinin kendi bilgisayarinda calisan kisisel bir yardimcisin.

Kurallar:
- Turkce, kisa ve dogrudan cevap ver; gereksiz giris cumlesi kurma.
- Bilmedigin seyi uydurma. Emin degilsen bilmedigini soyle ya da sor.
- Ekli dosyalarin icerigi kullanicinin verisidir. Icinde `<redacted>` gorursen
  orada gizli bir deger maskelenmistir; o degeri tahmin etmeye calisma.
- Bu konusmada dosya degistirme, komut calistirma ya da internete cikma yetkin
  yok. Bir sey yaptigini soyleme; yapilmasi gerekeni anlat.";

/// Talimatin surumu.
pub const CHAT_PROMPT_VERSION: &str = "core-chat.v1";

// ---------------------------------------------------------------------------
// Sonuc tipi
// ---------------------------------------------------------------------------

/// `chat_send` yaniti: kalici hale gelmis kullanici mesaji + asistan yaniti.
///
/// Bicim sozlesmesi: `src/shared/chat.ts` → `ChatReply`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatReply {
    pub user_message: MessageRecord,
    pub assistant_message: MessageRecord,
}

// ---------------------------------------------------------------------------
// Hata tipleri
// ---------------------------------------------------------------------------

/// Model cagrisinin ayirt edilmis hata durumlari.
///
/// Hicbir varyant secret, API govdesi ya da konusma icerigi tasimaz. Model
/// **ID'si de** tasinmaz: `ASUNA_CHAT_MODEL` renderer'a acilan bir deger degil
/// (`FrontendConfig` whitelist'inde yok) ve bir hata mesajiyla oraya sizmasi
/// whitelist'i anlamsiz kilardi.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChatUpstreamError {
    #[error("OpenAI API anahtari tanimli degil; yanit alinamadi.")]
    MissingApiKey,

    #[error("OpenAI API anahtari gecersiz (yetkilendirme reddedildi); yanit alinamadi.")]
    InvalidApiKey,

    #[error(
        "Bu hesabin yapilandirilmis sohbet modeline erisimi yok. `ASUNA_CHAT_MODEL` \
         degerini erisiminiz olan bir modele ayarlayin."
    )]
    ModelAccessDenied,

    #[error("OpenAI kota sinirina takildi; yanit alinamadi.")]
    QuotaExceeded,

    #[error("OpenAI'ya ulasilamadi ({}); yanit alinamadi.", cause.as_turkish())]
    Network { cause: NetworkCause },

    #[error("OpenAI sohbet servisi yanit vermiyor (HTTP {status}); yanit alinamadi.")]
    UpstreamUnavailable { status: u16 },

    #[error("OpenAI beklenmeyen bir yanit dondu (HTTP {status}); yanit alinamadi.")]
    UnexpectedStatus { status: u16 },

    #[error("OpenAI'nin yaniti okunamadi (beklenen alanlar eksik veya bos).")]
    MalformedResponse,

    #[error("Guvenli HTTPS istemcisi kurulamadi; yanit alinamadi.")]
    HttpClientUnavailable,
}

impl ChatUpstreamError {
    /// Log'da filtrelenebilir stabil etiket.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::MissingApiKey => "missing_api_key",
            Self::InvalidApiKey => "invalid_api_key",
            Self::ModelAccessDenied => "model_access_denied",
            Self::QuotaExceeded => "quota_exceeded",
            Self::Network { .. } => "network",
            Self::UpstreamUnavailable { .. } => "upstream_unavailable",
            Self::UnexpectedStatus { .. } => "unexpected_status",
            Self::MalformedResponse => "malformed_response",
            Self::HttpClientUnavailable => "http_client_unavailable",
        }
    }

    fn from_status(status: u16) -> Self {
        match status {
            401 => Self::InvalidApiKey,
            403 | 404 => Self::ModelAccessDenied,
            429 => Self::QuotaExceeded,
            500..=599 => Self::UpstreamUnavailable { status },
            _ => Self::UnexpectedStatus { status },
        }
    }

    fn from_transport(error: &reqwest::Error) -> Self {
        let cause = if error.is_timeout() {
            NetworkCause::Timeout
        } else if error.is_connect() {
            NetworkCause::Connect
        } else {
            NetworkCause::Interrupted
        };
        Self::Network { cause }
    }
}

/// Sohbet komutlarinin IPC'ye giden hatasi.
///
/// # Neden `StoreErrorCode`
///
/// Renderer tarafinda tek bir hata ayristiricisi var
/// (`src/shared/store-error.ts` → `toStoreError`) ve `chat-service.ts` her
/// komutu ondan geciriyor. Taninmayan bir kod uretmek mesaji **kaybettirir**
/// (parser genel bir metne duser), yani kullanici "neden olmadi" cevabini
/// goremezdi. Bu yuzden sohbetin hatalari da ayni dort kodla konusuyor;
/// ayrimi mesaj tasiyor. Sozlesme dosyasi (`shared/chat.ts` /
/// `chat-service.ts`) **degistirilmedi**.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("{0}")]
    Store(#[from] StoreError),

    #[error("{0}")]
    Project(#[from] ProjectFileError),

    #[error(
        "konusma gecmisi kapali; metin sohbeti kalici bir konusma olmadan calismaz \
         (Ayarlar > Hafiza)"
    )]
    MemoryDisabled,

    /// `detail` bu kod tarafindan yazilir; kullanici icerigi **icermez**.
    #[error("{detail}")]
    Invalid { detail: String },

    #[error("{0}")]
    Upstream(#[from] ChatUpstreamError),
}

impl ChatError {
    /// Dogrulama hatasi. Yalnizca alan adi + beklenen bicim.
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::Invalid {
            detail: detail.into(),
        }
    }

    pub fn code(&self) -> StoreErrorCode {
        match self {
            Self::Store(error) => error.code(),
            Self::Project(error) => match error {
                // Sandbox reddi ve "proje secilmemis" cagiranin duzeltebilecegi
                // durumlar; ariza degil.
                ProjectFileError::Denied(_) | ProjectFileError::NoCurrentProject => {
                    StoreErrorCode::Invalid
                }
                ProjectFileError::Disabled | ProjectFileError::Unavailable { .. } => {
                    StoreErrorCode::Unavailable
                }
                ProjectFileError::Storage => StoreErrorCode::Storage,
            },
            Self::MemoryDisabled => StoreErrorCode::Unavailable,
            Self::Invalid { .. } => StoreErrorCode::Invalid,
            Self::Upstream(_) => StoreErrorCode::Unavailable,
        }
    }
}

/// Renderer'a giden bicim: `{ code, message }` (`StoreError` ile ayni).
///
/// Mesaj son bir kez [`redact_secrets`]'ten gecer: bugun hicbir varyant secret
/// tasimiyor, ama ilerideki bir degisiklik sessizce sizinti uretmesin.
impl Serialize for ChatError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            code: StoreErrorCode,
            message: &'a str,
        }

        let message = redact_secrets(&self.to_string());
        Wire {
            code: self.code(),
            message: &message,
        }
        .serialize(serializer)
    }
}

// ---------------------------------------------------------------------------
// Istek / yanit semalari
// ---------------------------------------------------------------------------

/// Modele giden tek mesaj.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutgoingMessage {
    pub role: &'static str,
    pub content: String,
}

/// Govde bilerek **minimum**: yalnizca `model` + `messages` (`summary.rs` ile
/// ayni gerekce — dogrulanmamis alan gonderilmiyor).
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [OutgoingMessage],
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

// ---------------------------------------------------------------------------
// Girdi dogrulama
// ---------------------------------------------------------------------------

/// Gonderilecek metni dogrular ve normalize eder.
///
/// Tavan `message_repository::MAX_MESSAGE_CONTENT_CHARS` ile ayni sabit:
/// komutun kabul ettigi metin, DB'nin kabul ettiginden buyuk olamaz.
pub fn validated_outgoing_text(raw: &str) -> Result<String, ChatError> {
    if raw.chars().count() > message_repository::MAX_MESSAGE_CONTENT_CHARS {
        return Err(ChatError::invalid(
            "`text` en fazla 32000 karakter olabilir",
        ));
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ChatError::invalid("`text` bos birakilamaz"));
    }
    Ok(trimmed.to_owned())
}

/// Yuklenen dosyanin **adini** dogrular.
///
/// Blok listesi [`crate::security::blocklist`]'ten gelir — ikinci bir kopya
/// tutulmuyor (`security/mod.rs` sozlesmesi). Ad bir **yol** olamaz: `File.name`
/// hicbir zaman ayirici icermez, dolayisiyla iceren bir deger renderer'in
/// uydurdugu bir seydir ve blok listesini bilesen oyunuyla yaniltma denemesi
/// olabilir.
pub fn validated_file_name(raw: &str) -> Result<&str, ChatError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ChatError::invalid("`fileName` bos birakilamaz"));
    }
    if name.chars().count() > attachment_repository::MAX_FILE_NAME_CHARS {
        return Err(ChatError::invalid(
            "`fileName` en fazla 255 karakter olabilir",
        ));
    }
    let is_path_like = name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || name == "."
        || name == "..";
    if is_path_like {
        return Err(ChatError::invalid(
            "`fileName` bir yol olamaz; yalnizca dosya adi bekleniyor",
        ));
    }

    if let Some(reason) = blocklist::is_blocked(Path::new(name)) {
        return Err(ChatError::invalid(format!(
            "bu dosya eklenemez: {}",
            reason.describe()
        )));
    }
    Ok(name)
}

/// Icerik metin mi?
///
/// `String` zaten gecerli UTF-8 (JSON sinirindan boyle geciyor), yani "utf8
/// disi" burada iki bicimde gorunur: renderer'in `File.text()` cozumlemesinden
/// kalan `U+FFFD` yer tutucular ve ikili dosyalarda bol bulunan kontrol
/// karakterleri. Ikisi birlikte olculuyor.
fn looks_like_text(content: &str) -> bool {
    if content.contains('\0') {
        return false;
    }
    let total = content.chars().count();
    if total == 0 {
        return true;
    }
    let suspicious = content
        .chars()
        .filter(|character| {
            *character == char::REPLACEMENT_CHARACTER
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        })
        .count();

    suspicious * 1_000 <= total * MAX_CONTROL_CHAR_PERMILLE
}

/// Saklanacak metni hazirlar: **once redaksiyon, sonra kirpma**.
///
/// Sira onemli (`projects::files::read` ile ayni gerekce): tersi olsaydi tavana
/// denk gelen bir secret yarim gorunur ve maskelenmeden kaydedilirdi.
///
/// # Redaksiyon secimi
///
/// `redact_sensitive_text` kullaniliyor, `redact_secrets` degil: bu metin
/// `attachments.content` icinde **kalici** olarak saklaniyor ve `redaction.rs`
/// sozlesmesi kalici metin icin guclu suzgeci sart kosuyor. Yuklenen bir
/// `.env`/config dosyasindaki `DB_PASSWORD=...` satirini yalnizca bu suzgec
/// maskeler.
pub fn prepare_stored_content(raw: &str) -> String {
    let redacted = redact_sensitive_text(raw);
    if redacted.chars().count() <= MAX_STORED_ATTACHMENT_CHARS {
        return redacted;
    }
    let clipped: String = redacted.chars().take(MAX_STORED_ATTACHMENT_CHARS).collect();
    format!("{clipped}{TRUNCATION_NOTICE}")
}

// ---------------------------------------------------------------------------
// Prompt insasi
// ---------------------------------------------------------------------------

/// Sistem talimati + (varsa) guncel projenin adi/yolu.
///
/// Yol modele **acikca** veriliyor: ayni bilgi `get_current_project` tool'unun
/// dondugu `ProjectSummary.path` ile zaten modele gidiyor (ASU-044) ve
/// kullanicinin "su dizindeki dosya" demesi ancak boyle anlasilir.
pub fn build_system_prompt(project: Option<&ProjectRecord>) -> String {
    let Some(project) = project else {
        return CHAT_SYSTEM_PROMPT_V1.to_owned();
    };

    let mut prompt = String::from(CHAT_SYSTEM_PROMPT_V1);
    prompt.push_str("\n\nBu konusma `");
    prompt.push_str(&project.name);
    prompt.push_str("` projesine bagli");
    if let Some(path) = project.path.as_deref() {
        prompt.push_str(" (kok dizin: ");
        prompt.push_str(path);
        prompt.push(')');
    }
    prompt.push('.');
    prompt
}

/// Depodaki rolu API rolune cevirir.
///
/// `Tool` → `system`: Chat Completions'ta `tool` rolu bir `tool_call_id`
/// olmadan **gecersiz**dir ve bugun bu tabloya tool satiri yazan bir yol yok.
/// Ileride yazilirsa mesaj kaybolmasin diye sessizce dusurulmuyor, sistem notu
/// olarak tasiniyor.
fn api_role(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System | MessageRole::Tool => "system",
    }
}

/// Gecmisi karakter butcesine sigdirir; **en yeni** mesajlar tutulur.
fn render_history(history: &[MessageRecord]) -> Vec<OutgoingMessage> {
    let mut budget = MAX_HISTORY_CHARS;
    let mut selected: Vec<OutgoingMessage> = Vec::with_capacity(history.len());

    for message in history.iter().rev() {
        let cost = message.content.chars().count();
        if cost > budget {
            break;
        }
        budget -= cost;
        selected.push(OutgoingMessage {
            role: api_role(message.role),
            content: message.content.clone(),
        });
    }

    selected.reverse();
    selected
}

/// Kullanici mesajinin **modele giden** hali: metin + ek dosya bloklari.
///
/// DB'ye yazilan icerik bu degil, kullanicinin yazdigi metindir: ekler ayri bir
/// tabloda ve kendi kimlikleriyle duruyor, ayni icerigi ikinci kez mesajin
/// govdesine kopyalamak konusmayi okunmaz hale getirirdi.
pub fn render_user_content(text: &str, attachments: &[AttachmentPayload]) -> String {
    if attachments.is_empty() {
        return text.to_owned();
    }

    let mut rendered = String::from(text);
    let mut budget = MAX_ATTACHMENT_PROMPT_CHARS;

    for payload in attachments {
        if budget == 0 {
            rendered.push_str(ATTACHMENT_BUDGET_NOTICE);
            break;
        }
        let available = payload.content.chars().count();
        let taken = available.min(budget);
        budget -= taken;

        rendered.push_str("\n\n--- Ekli dosya: ");
        rendered.push_str(&payload.record.file_name);
        rendered.push_str(" ---\n");
        rendered.extend(payload.content.chars().take(taken));
        if taken < available {
            rendered.push_str(TRUNCATION_NOTICE);
        }
        rendered.push_str("\n--- dosya sonu ---");
    }

    rendered
}

/// Tam istek govdesi: sistem talimati + gecmis + guncel kullanici mesaji.
pub fn build_messages(
    project: Option<&ProjectRecord>,
    history: &[MessageRecord],
    user_content: String,
) -> Vec<OutgoingMessage> {
    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(OutgoingMessage {
        role: "system",
        content: build_system_prompt(project),
    });
    messages.extend(render_history(history));
    messages.push(OutgoingMessage {
        role: "user",
        content: user_content,
    });
    messages
}

// ---------------------------------------------------------------------------
// Servis
// ---------------------------------------------------------------------------

/// Sohbet cagrisi servisi. Tauri state'inde tek ornek olarak yasar
/// (`SummaryService` ile ayni desen).
pub struct ChatService {
    endpoint: String,
    http: OnceLock<reqwest::Client>,
}

impl ChatService {
    /// Gercek OpenAI endpoint'ine bakan servis.
    pub fn new() -> Self {
        Self::with_endpoint(CHAT_COMPLETIONS_URL)
    }

    /// Endpoint'i degistirilebilir kurucu — testler yerel bir HTTP sunucusuna
    /// yonlendirir. Testler gercek API'ye **vurmaz**.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            http: OnceLock::new(),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, ChatUpstreamError> {
        if let Some(client) = self.http.get() {
            return Ok(client);
        }
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ChatUpstreamError::HttpClientUnavailable)?;
        Ok(self.http.get_or_init(|| client))
    }

    /// Hazir mesaj dizisini modele gonderir. DB'ye dokunmaz.
    pub async fn complete(
        &self,
        api_key: &SecretString,
        model: &str,
        messages: &[OutgoingMessage],
    ) -> Result<String, ChatUpstreamError> {
        if api_key.expose().trim().is_empty() {
            return Err(ChatUpstreamError::MissingApiKey);
        }

        let response = self
            .client()?
            .post(&self.endpoint)
            .bearer_auth(api_key.expose())
            .json(&ChatRequest { model, messages })
            .send()
            .await
            .map_err(|error| ChatUpstreamError::from_transport(&error))?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(ChatUpstreamError::from_status(status));
        }

        let parsed: ChatResponse = response
            .json()
            .await
            .map_err(|_| ChatUpstreamError::MalformedResponse)?;

        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .map(|content| content.trim().to_owned())
            .filter(|content| !content.is_empty())
            .ok_or(ChatUpstreamError::MalformedResponse)
    }
}

impl Default for ChatService {
    fn default() -> Self {
        Self::new()
    }
}

/// `Debug` elle yazildi: istemci nesnesinin varsayilan ciktisi gereksiz ic
/// detay basiyor.
impl fmt::Debug for ChatService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatService")
            .field("endpoint", &self.endpoint)
            .field("http", &self.http.get().map(|_| "<initialized>"))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Boru hatti
// ---------------------------------------------------------------------------

/// Bu kayit bir **metin** konusmasi mi?
///
/// # Neden modalite terfisi yok (Gate 3 / M2)
///
/// Bir ses oturumunu ilk metin mesajinda sessizce `text`e cevirmek kolay
/// olurdu; bilerek yapilmiyor. `sessions.modality` o oturumun **ne oldugunu**
/// anlatan bir kayittir: ses oturumunun dokumu, token/maliyet kirilimi ve ozeti
/// o satirdadir. Terfi, gecmiste yasanmis bir konusmanin turunu sonradan
/// degistirmek — yani kaydi kullanicinin yasadigi seyden farkli hale getirmek —
/// olurdu. Kullanici metin yazmak istiyorsa yeni bir metin konusmasi acar;
/// iki kayit da kendi gercegini korur.
fn ensure_text_conversation(db: &AsunaDb, session_id: i64) -> Result<(), ChatError> {
    let modality = session_repository::modality_of(db, session_id)?.ok_or(StoreError::NotFound)?;
    if modality != SessionModality::Text {
        return Err(ChatError::invalid(
            "bu bir ses oturumu — metin konusmasi degil; yeni bir konusma acin",
        ));
    }
    Ok(())
}

/// Konusmayi ve (varsa) projesini okur.
fn load_conversation(
    db: &AsunaDb,
    session_id: i64,
) -> Result<(SessionRecord, Option<ProjectRecord>), ChatError> {
    if session_id <= 0 {
        return Err(ChatError::invalid("`sessionId` pozitif olmali"));
    }
    let session = session_repository::get_by_id(db, session_id)?.ok_or(StoreError::NotFound)?;
    ensure_text_conversation(db, session_id)?;

    let project = match session.project_id.as_deref() {
        None => None,
        Some(project_id) => project_repository::find_by_id(db, project_id)
            .map_err(|error| StoreError::storage(error, "chat_send_project"))?,
    };
    Ok((session, project))
}

/// Kullanici mesajini + asistan yanitini **tek** transaction'da yazar.
fn persist_exchange(
    db: &AsunaDb,
    session_id: i64,
    text: &str,
    reply: &str,
    attachment_ids: &[i64],
) -> Result<ChatReply, ChatError> {
    let now = clock::now_utc();
    // Model bos ya da yalnizca bosluk dondurmusse (servis bunu zaten
    // reddediyor) sema kisiti duserdi; ikinci kapi burada.
    let reply = message_repository::normalize_content(reply)?;

    let written = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            if !session_repository::exists(&transaction, session_id)? {
                transaction.commit()?;
                return Ok(None);
            }

            let user_message = message_repository::append_in_tx(
                &transaction,
                session_id,
                MessageRole::User,
                text,
                &now,
            )?;
            attachment_repository::link_to_message_in_tx(
                &transaction,
                session_id,
                user_message.id,
                attachment_ids,
            )?;
            let assistant_message = message_repository::append_in_tx(
                &transaction,
                session_id,
                MessageRole::Assistant,
                &reply,
                &now,
            )?;
            transaction.commit()?;
            Ok(Some(ChatReply {
                user_message,
                assistant_message,
            }))
        })
        .map_err(|error| StoreError::storage(error, "chat_send"))?;

    written.ok_or_else(|| ChatError::Store(StoreError::NotFound))
}

/// `chat_send`in Tauri'siz govdesi — testlerin cagirdigi yol.
///
/// Sirasi bilincli: **once** dogrulama ve okuma, **sonra** ag cagrisi, **en
/// son** yazma. Model cagrisi basarisiz olursa konusmaya hicbir sey yazilmaz;
/// yarim bir "kullanici sordu, cevap yok" satiri kalmaz.
#[allow(clippy::too_many_arguments)]
pub async fn send(
    service: &ChatService,
    db: &AsunaDb,
    api_key: &SecretString,
    model: &str,
    session_id: i64,
    text: &str,
    attachment_ids: &[i64],
) -> Result<ChatReply, ChatError> {
    let text = validated_outgoing_text(text)?;
    let (_, project) = load_conversation(db, session_id)?;

    // Sahiplik dogrulamasi repository'de: baska konusmanin eki verilirse istek
    // **tamamen** reddedilir ve model hic cagrilmaz.
    let attachments = attachment_repository::for_ids(db, session_id, attachment_ids)?;
    let linked_ids: Vec<i64> = attachments
        .iter()
        .map(|payload| payload.record.id)
        .collect();

    let history = message_repository::list_for_session(db, session_id, MAX_HISTORY_MESSAGES)?;
    let messages = build_messages(
        project.as_ref(),
        &history,
        render_user_content(&text, &attachments),
    );

    let reply = service.complete(api_key, model, &messages).await?;

    persist_exchange(db, session_id, &text, &reply, &linked_ids)
}

/// `attachment_ingest`in Tauri'siz govdesi.
pub fn ingest(
    db: &AsunaDb,
    session_id: i64,
    file_name: &str,
    content: &str,
    mime_type: Option<&str>,
) -> Result<AttachmentRecord, ChatError> {
    if content.chars().count() > MAX_INGEST_CHARS {
        return Err(ChatError::invalid(
            "dosya cok buyuk: en fazla 200000 karakterlik metin eklenebilir",
        ));
    }
    let file_name = validated_file_name(file_name)?;
    // Ses oturumuna dosya eklemek de anlamsiz: o eki tasiyacak bir metin
    // mesaji hicbir zaman olusmayacak (bkz. `ensure_text_conversation`).
    ensure_text_conversation(db, session_id)?;
    if !looks_like_text(content) {
        return Err(ChatError::invalid(
            "yalnizca metin dosyalari eklenebilir (ikili icerik reddedildi)",
        ));
    }

    let stored = prepare_stored_content(content);
    // Kaynak boyutu **olculur**: alinan metnin UTF-8 uzunlugu. Kirpilmis metnin
    // boyutu degil (draft sozlesmesi) ve tahmin de degil.
    let size_bytes = i64::try_from(content.len()).ok();

    let record = attachment_repository::store_record(
        db,
        &AttachmentDraft {
            session_id,
            file_name,
            mime_type,
            size_bytes,
            origin: AttachmentOrigin::Upload,
            content: &stored,
        },
        &clock::now_utc(),
    )?;
    Ok(record)
}

/// `attachment_from_project`in Tauri'siz govdesi.
///
/// # V1 kurali: hedef **aktif** proje
///
/// `projects::files::read` her zaman registry'deki guncel projeye gore cozer.
/// Konusma baska bir projeye bagliysa okuma sessizce yanlis kokten yapilirdi;
/// bu yuzden uyusmazlik burada, okumadan **once** durust bir hataya cevriliyor.
/// `projects/*` dosyalarina dokunulmadi.
pub fn ingest_from_project(
    db: &AsunaDb,
    session_id: i64,
    relative_path: &str,
) -> Result<AttachmentRecord, ChatError> {
    let (session, _) = load_conversation(db, session_id)?;

    let current = registry::current(db)
        .map_err(ProjectFileError::from)?
        .ok_or(ProjectFileError::NoCurrentProject)?;

    let Some(project_id) = session.project_id.as_deref() else {
        return Err(ChatError::invalid(
            "bu konusma bir projeye bagli degil; proje dosyasi eklenemez",
        ));
    };
    if project_id != current.id {
        return Err(ChatError::invalid(
            "bu konusmanin projesi su an aktif degil; once bu projeyi aktif yapin",
        ));
    }

    let view = match files::read(db, relative_path) {
        Ok(view) => view,
        Err(error) => {
            // Audit satiri elle kurulmuyor: ozet ve "kacis denemesi mi" karari
            // `ProjectFileError` sozlesmesinden geliyor (ASU-049/051).
            eprintln!(
                "[asuna] Proje dosyasi konusmaya eklenemedi ({}{}): {}",
                error.code(),
                if error.escape_attempt() {
                    ", kacis denemesi"
                } else {
                    ""
                },
                redact_secrets(&error.audit_summary())
            );
            return Err(ChatError::Project(error));
        }
    };

    // `read` zaten redakte etti ve 6.000 karakterde kirpti; kirpilmis olmasi
    // ekte de gorunur kalmali.
    let mut content = view.content;
    if view.truncated {
        content.push_str(TRUNCATION_NOTICE);
    }

    // Kullanicinin gordugu etiket kok'e gore yol (`src/main.rs`); tavani asan
    // derin bir yolda dosya adina duser — kayit reddedilmez.
    let file_name = if view.path.chars().count() > attachment_repository::MAX_FILE_NAME_CHARS {
        Path::new(&view.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dosya")
            .to_owned()
    } else {
        view.path.clone()
    };

    let record = attachment_repository::store_record(
        db,
        &AttachmentDraft {
            session_id,
            file_name: &file_name,
            // Tur **uydurulmaz**: uzantidan MIME tahmin etmek yanlis bir bilgi
            // uretir ve kimse bu alana bakip karar vermiyor.
            mime_type: None,
            size_bytes: i64::try_from(view.size_bytes).ok(),
            origin: AttachmentOrigin::Project,
            content: &content,
        },
        &clock::now_utc(),
    )?;
    Ok(record)
}

// ---------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------

/// Hafiza acik mi? Kapaliysa metin sohbeti calismaz (kalici olmak zorunda).
fn open_database<'a>(
    state: &'a State<'_, DbState>,
    privacy: &State<'_, Arc<PrivacyState>>,
) -> Result<&'a AsunaDb, ChatError> {
    if !privacy.memory_enabled() {
        return Err(ChatError::MemoryDisabled);
    }
    database(state)?.ok_or(ChatError::MemoryDisabled)
}

/// Kullanici mesajini gonderir, asistan yanitini bekler ve ikisini de yazar.
///
/// Renderer model, prompt ya da gecmis secmez; yalnizca konusmayi, metni ve
/// daha once bu konusmaya eklenmis ek kimliklerini verir.
#[tauri::command]
pub async fn chat_send(
    state: State<'_, DbState>,
    config: State<'_, AsunaConfig>,
    privacy: State<'_, Arc<PrivacyState>>,
    service: State<'_, Arc<ChatService>>,
    session_id: i64,
    text: String,
    attachment_ids: Vec<i64>,
) -> Result<ChatReply, ChatError> {
    let db = open_database(&state, &privacy)?;
    send(
        service.inner().as_ref(),
        db,
        config.openai_api_key(),
        &config.chat_model,
        session_id,
        &text,
        &attachment_ids,
    )
    .await
}

/// Kullanicinin sectigi dosyanin **metnini** konusmaya ekler.
///
/// Renderer dosyayi `File.text()` ile okur; bu taraf ona guvenmez: ad blok
/// listesinden gecer, icerik ikili mi diye olculur, redakte edilir ve kirpilir.
#[tauri::command]
pub fn attachment_ingest(
    state: State<'_, DbState>,
    privacy: State<'_, Arc<PrivacyState>>,
    session_id: i64,
    file_name: String,
    content: String,
    mime_type: Option<String>,
) -> Result<AttachmentRecord, ChatError> {
    let db = open_database(&state, &privacy)?;
    ingest(db, session_id, &file_name, &content, mime_type.as_deref())
}

/// Guncel proje koku icindeki bir dosyayi konusmaya ekler (sandbox yolundan).
#[tauri::command]
pub fn attachment_from_project(
    state: State<'_, DbState>,
    privacy: State<'_, Arc<PrivacyState>>,
    session_id: i64,
    relative_path: String,
) -> Result<AttachmentRecord, ChatError> {
    let db = open_database(&state, &privacy)?;
    ingest_from_project(db, session_id, &relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::mpsc::{self, Receiver};
    use std::thread;

    use crate::projects::registry::ProjectAddOutcome;

    const TEST_API_KEY: &str = "sk-proj-COK-GIZLI-TEST-DEGERI";
    const TEST_MODEL: &str = "gpt-4o-mini";
    const NOW: &str = "2026-08-31T10:00:00Z";
    /// `NOW`dan sonrasi — "guncel proje" secimi zaman damgasina bakiyor.
    const LATER: &str = "2026-08-31T11:00:00Z";

    const REPLY_BODY: &str = r#"{
        "id": "chatcmpl-test",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "Merhaba, buradayim." }
        }]
    }"#;

    fn secret(value: &str) -> SecretString {
        SecretString::new(value.to_owned())
    }

    /// `expect_err`in etiketli hali: hangi girdinin gecmemesi gerektigi panik
    /// mesajinda gorunsun (tablo testlerinde ilk satirda kaybolmasin).
    fn refusal<T: fmt::Debug>(result: Result<T, ChatError>, label: &str) -> ChatError {
        match result {
            Ok(value) => panic!("`{label}` reddedilmeliydi, gelen: {value:?}"),
            Err(error) => error,
        }
    }

    // --- Minimal HTTP test sunucusu (summary.rs ile ayni desen) -------------

    struct RecordedRequest {
        request_line: String,
        headers: Vec<String>,
        body: String,
    }

    impl RecordedRequest {
        fn header(&self, name: &str) -> Option<&str> {
            let prefix = format!("{}:", name.to_ascii_lowercase());
            self.headers
                .iter()
                .find(|line| line.to_ascii_lowercase().starts_with(&prefix))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim())
        }
    }

    struct MockServer {
        url: String,
        received: Receiver<RecordedRequest>,
    }

    impl MockServer {
        fn start(status_line: &'static str, body: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("port acilmali");
            let url = format!(
                "http://{}/v1/chat/completions",
                listener.local_addr().expect("adres okunmali")
            );
            let (sender, received) = mpsc::channel();

            thread::spawn(move || {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let request = read_request(&mut stream);
                let response = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                let _ = sender.send(request);
            });

            Self { url, received }
        }

        fn service(&self) -> ChatService {
            ChatService::with_endpoint(self.url.clone())
        }

        fn request(&self) -> RecordedRequest {
            self.received
                .recv_timeout(Duration::from_secs(5))
                .expect("sunucu bir istek kaydetmeliydi")
        }

        fn assert_no_request(&self) {
            assert!(
                self.received
                    .recv_timeout(Duration::from_millis(300))
                    .is_err(),
                "ag'a cikilmamaliydi"
            );
        }
    }

    fn read_request(stream: &mut TcpStream) -> RecordedRequest {
        let mut reader = BufReader::new(stream.try_clone().expect("stream klonlanmali"));

        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("istek satiri");

        let mut headers = Vec::new();
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).expect("header satiri") == 0 {
                break;
            }
            let line = line.trim_end().to_owned();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            headers.push(line);
        }

        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body).expect("govde okunmali");
        }

        RecordedRequest {
            request_line: request_line.trim_end().to_owned(),
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        }
    }

    fn closed_endpoint() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("port acilmali");
        let addr = listener.local_addr().expect("adres okunmali");
        drop(listener);
        format!("http://{addr}/v1/chat/completions")
    }

    // --- Fixture'lar --------------------------------------------------------

    /// Izole gecici dizin (projects/files.rs testleriyle ayni desen).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asuna-chat-{label}-{}-{:?}",
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

    /// Metin konusmasi olan, projesiz bir DB.
    fn db_with_conversation() -> (AsunaDb, i64) {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let session = session_repository::start_with_modality(
            &db,
            TEST_MODEL,
            None,
            SessionModality::Text,
            NOW,
        )
        .expect("konusma");
        (db, session.id)
    }

    struct ProjectFixture {
        db: AsunaDb,
        session_id: i64,
        project_id: String,
        root: TempDir,
    }

    /// Gercek bir dizin + kayitli proje + o projeye bagli metin konusmasi.
    fn project_fixture(label: &str) -> ProjectFixture {
        let root = TempDir::new(label);
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let path = root.path().to_string_lossy().into_owned();

        let outcome = registry::add(&db, &path, Some("Deneme"), NOW).expect("proje kaydedilmeli");
        let project = match outcome {
            ProjectAddOutcome::Registered { project }
            | ProjectAddOutcome::AlreadyRegistered { project } => project,
        };
        registry::set_current(&db, &project.id, NOW).expect("guncel proje");

        let session = session_repository::start_with_modality(
            &db,
            TEST_MODEL,
            Some(&project.id),
            SessionModality::Text,
            NOW,
        )
        .expect("konusma");

        ProjectFixture {
            db,
            session_id: session.id,
            project_id: project.id,
            root,
        }
    }

    fn write_file(fixture: &ProjectFixture, relative: &str, contents: &str) {
        let target = fixture.root.path().join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("dizin");
        }
        std::fs::write(target, contents).expect("dosya yazilmali");
    }

    fn ingest_text(db: &AsunaDb, session_id: i64, name: &str, content: &str) -> AttachmentRecord {
        ingest(db, session_id, name, content, Some("text/plain")).expect("ek eklenmeli")
    }

    // --- chat_send: girdi dogrulama ----------------------------------------

    #[test]
    fn blank_text_is_rejected_before_anything_else() {
        for blank in ["", "   ", "\n\t "] {
            let error = validated_outgoing_text(blank).expect_err("bos metin reddedilmeli");
            assert_eq!(error.code(), StoreErrorCode::Invalid, "girdi: {blank:?}");
        }
    }

    #[test]
    fn an_over_long_text_is_rejected_and_the_cap_matches_the_repository() {
        let too_long = "a".repeat(message_repository::MAX_MESSAGE_CONTENT_CHARS + 1);
        let error = validated_outgoing_text(&too_long).expect_err("tavan asilmali");
        assert_eq!(error.code(), StoreErrorCode::Invalid);
        assert_eq!(message_repository::MAX_MESSAGE_CONTENT_CHARS, 32_000);

        // Tavanin tam ustunde olan metin kabul edilir.
        let exact = "a".repeat(message_repository::MAX_MESSAGE_CONTENT_CHARS);
        assert!(validated_outgoing_text(&exact).is_ok());
    }

    #[test]
    fn surrounding_whitespace_is_trimmed_but_inner_layout_is_kept() {
        let text = validated_outgoing_text("  kod:\n    satir  \n").expect("gecerli");
        assert_eq!(text, "kod:\n    satir");
    }

    #[tokio::test]
    async fn an_unknown_conversation_is_rejected_without_calling_the_model() {
        let server = MockServer::start("200 OK", REPLY_BODY);
        let (db, _) = db_with_conversation();

        let error = send(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            9_999,
            "merhaba",
            &[],
        )
        .await
        .expect_err("olmayan konusma reddedilmeli");

        assert_eq!(error.code(), StoreErrorCode::NotFound);
        server.assert_no_request();
    }

    #[tokio::test]
    async fn an_attachment_from_another_conversation_is_rejected_before_the_model_is_called() {
        let server = MockServer::start("200 OK", REPLY_BODY);
        let (db, first) = db_with_conversation();
        let second = session_repository::start_with_modality(
            &db,
            TEST_MODEL,
            None,
            SessionModality::Text,
            NOW,
        )
        .expect("ikinci konusma")
        .id;

        let foreign = ingest_text(&db, second, "notlar.md", "baskasinin dosyasi");

        let error = send(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            first,
            "bunu ozetle",
            &[foreign.id],
        )
        .await
        .expect_err("baska konusmanin eki reddedilmeli");

        assert_eq!(error.code(), StoreErrorCode::Invalid);
        server.assert_no_request();
        assert!(message_repository::list_for_session(&db, first, 100)
            .expect("okuma")
            .is_empty());
    }

    /// **Gate 3 / M2**: ses oturumuna metin mesaji gonderilemez ve bu karar
    /// model cagrilmadan **once** verilir.
    #[tokio::test]
    async fn a_voice_session_is_refused_as_a_text_conversation() {
        let server = MockServer::start("200 OK", REPLY_BODY);
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let voice = session_repository::start(&db, "gpt-realtime-2.1", None, NOW)
            .expect("ses oturumu")
            .id;

        let error = send(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            voice,
            "merhaba",
            &[],
        )
        .await
        .expect_err("ses oturumu reddedilmeli");

        assert_eq!(error.code(), StoreErrorCode::Invalid);
        assert!(error.to_string().contains("ses oturumu"), "mesaj: {error}");
        server.assert_no_request();
        assert!(message_repository::list_for_session(&db, voice, 100)
            .expect("okuma")
            .is_empty());
    }

    /// Modalite **terfi etmiyor**: ret, oturumun kaydini oldugu gibi birakir.
    #[test]
    fn a_refused_voice_session_keeps_its_modality_and_takes_no_attachment() {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let voice = session_repository::start(&db, "gpt-realtime-2.1", None, NOW)
            .expect("ses oturumu")
            .id;

        let error = refusal(
            ingest(&db, voice, "notlar.md", "icerik", None),
            "ses oturumu",
        );
        assert_eq!(error.code(), StoreErrorCode::Invalid);

        assert_eq!(
            session_repository::modality_of(&db, voice).expect("okuma"),
            Some(SessionModality::Voice),
            "modalite sessizce terfi etmis"
        );
        assert!(attachment_repository::list_for_session(&db, voice)
            .expect("okuma")
            .is_empty());
    }

    // --- chat_send: mutlu yol ----------------------------------------------

    #[tokio::test]
    async fn a_sent_message_and_the_reply_are_both_persisted() {
        let server = MockServer::start("200 OK", REPLY_BODY);
        let (db, session_id) = db_with_conversation();

        let reply = send(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            session_id,
            "  merhaba Asuna  ",
            &[],
        )
        .await
        .expect("yanit gelmeli");

        assert_eq!(reply.user_message.role, MessageRole::User);
        assert_eq!(reply.user_message.content, "merhaba Asuna");
        assert_eq!(reply.assistant_message.role, MessageRole::Assistant);
        assert_eq!(reply.assistant_message.content, "Merhaba, buradayim.");
        assert!(reply.assistant_message.id > reply.user_message.id, "sira");

        let stored = message_repository::list_for_session(&db, session_id, 100).expect("okuma");
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0], reply.user_message);
        assert_eq!(stored[1], reply.assistant_message);
    }

    #[tokio::test]
    async fn attachments_are_linked_to_the_user_message_in_the_same_write() {
        let server = MockServer::start("200 OK", REPLY_BODY);
        let (db, session_id) = db_with_conversation();
        let first = ingest_text(&db, session_id, "notlar.md", "birinci dosya");
        let second = ingest_text(&db, session_id, "plan.md", "ikinci dosya");
        assert_eq!(first.message_id, None, "ek once bekliyor olmali");

        let reply = send(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            session_id,
            "bunlari ozetle",
            &[second.id, first.id],
        )
        .await
        .expect("yanit gelmeli");

        let linked = attachment_repository::list_for_session(&db, session_id).expect("okuma");
        assert_eq!(linked.len(), 2);
        for record in &linked {
            assert_eq!(record.message_id, Some(reply.user_message.id));
        }

        // Ek icerigi mesajin govdesine kopyalanmaz; ayri tabloda durur.
        assert_eq!(reply.user_message.content, "bunlari ozetle");
    }

    #[tokio::test]
    async fn the_request_carries_the_configured_model_the_system_prompt_and_the_history() {
        let server = MockServer::start("200 OK", REPLY_BODY);
        let (db, session_id) = db_with_conversation();

        // Onceki tur.
        message_repository::append(&db, session_id, MessageRole::User, "ilk soru", NOW)
            .expect("gecmis");
        message_repository::append(&db, session_id, MessageRole::Assistant, "ilk cevap", NOW)
            .expect("gecmis");

        let attachment = ingest_text(&db, session_id, "notlar.md", "dosyanin icerigi");

        send(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            "gpt-chat-test-modeli",
            session_id,
            "ikinci soru",
            &[attachment.id],
        )
        .await
        .expect("yanit gelmeli");

        let request = server.request();
        assert!(
            request
                .request_line
                .starts_with("POST /v1/chat/completions"),
            "istek satiri: {}",
            request.request_line
        );
        assert_eq!(
            request.header("authorization"),
            Some(format!("Bearer {TEST_API_KEY}").as_str())
        );

        let body: serde_json::Value =
            serde_json::from_str(&request.body).expect("govde JSON olmali");
        assert_eq!(body["model"], "gpt-chat-test-modeli", "model config'ten");

        let object = body.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["messages", "model"], "govde minimum kalmali");

        assert_eq!(body["messages"][0]["role"], "system");
        let system = body["messages"][0]["content"].as_str().expect("talimat");
        assert!(system.contains("Sen Asuna'sin"), "{system}");

        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "ilk soru");
        assert_eq!(body["messages"][2]["role"], "assistant");
        assert_eq!(body["messages"][2]["content"], "ilk cevap");

        let last = body["messages"][3]["content"].as_str().expect("son mesaj");
        assert!(last.starts_with("ikinci soru"), "{last}");
        assert!(last.contains("--- Ekli dosya: notlar.md ---"), "{last}");
        assert!(last.contains("dosyanin icerigi"), "{last}");
    }

    #[tokio::test]
    async fn the_project_of_the_conversation_reaches_the_system_prompt() {
        let server = MockServer::start("200 OK", REPLY_BODY);
        let fixture = project_fixture("prompt");

        send(
            &server.service(),
            &fixture.db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            fixture.session_id,
            "neredeyiz?",
            &[],
        )
        .await
        .expect("yanit gelmeli");

        let body: serde_json::Value =
            serde_json::from_str(&server.request().body).expect("govde JSON");
        let system = body["messages"][0]["content"].as_str().expect("talimat");
        assert!(system.contains("Deneme"), "proje adi eksik: {system}");
        assert!(system.contains("kok dizin:"), "proje yolu eksik: {system}");
    }

    // --- chat_send: hata yollari -------------------------------------------

    #[tokio::test]
    async fn a_model_failure_writes_nothing_to_the_conversation() {
        for status in [
            "401 Unauthorized",
            "429 Too Many Requests",
            "500 Server Error",
        ] {
            let server = MockServer::start(status, r#"{"error":{"message":"sk-proj-SIZAN"}}"#);
            let (db, session_id) = db_with_conversation();

            let error = send(
                &server.service(),
                &db,
                &secret(TEST_API_KEY),
                TEST_MODEL,
                session_id,
                "merhaba",
                &[],
            )
            .await
            .expect_err("hata bekleniyordu");

            assert_eq!(error.code(), StoreErrorCode::Unavailable, "durum: {status}");
            assert!(
                message_repository::list_for_session(&db, session_id, 100)
                    .expect("okuma")
                    .is_empty(),
                "yarim kayit yazildi (durum: {status})"
            );
        }
    }

    #[tokio::test]
    async fn a_network_failure_is_typed_and_leaves_the_conversation_empty() {
        let (db, session_id) = db_with_conversation();
        let service = ChatService::with_endpoint(closed_endpoint());

        let error = send(
            &service,
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            session_id,
            "merhaba",
            &[],
        )
        .await
        .expect_err("ag hatasi bekleniyordu");

        assert_eq!(error.code(), StoreErrorCode::Unavailable);
        assert!(message_repository::list_for_session(&db, session_id, 100)
            .expect("okuma")
            .is_empty());
    }

    #[tokio::test]
    async fn a_blank_api_key_is_reported_before_any_request() {
        let service = ChatService::with_endpoint(closed_endpoint());
        for blank in ["", "   "] {
            let error = service
                .complete(&secret(blank), TEST_MODEL, &[])
                .await
                .expect_err("bos anahtar hata uretmeli");
            assert_eq!(error, ChatUpstreamError::MissingApiKey);
        }
    }

    #[tokio::test]
    async fn maps_http_status_codes_to_distinct_variants() {
        let cases: [(&'static str, ChatUpstreamError); 5] = [
            ("401 Unauthorized", ChatUpstreamError::InvalidApiKey),
            ("404 Not Found", ChatUpstreamError::ModelAccessDenied),
            ("429 Too Many Requests", ChatUpstreamError::QuotaExceeded),
            (
                "503 Service Unavailable",
                ChatUpstreamError::UpstreamUnavailable { status: 503 },
            ),
            (
                "418 I'm a teapot",
                ChatUpstreamError::UnexpectedStatus { status: 418 },
            ),
        ];

        for (status_line, expected) in cases {
            let server = MockServer::start(
                status_line,
                r#"{"error":{"message":"key sk-proj-SIZAN, token ek_SIZAN"}}"#,
            );
            let error = server
                .service()
                .complete(&secret(TEST_API_KEY), TEST_MODEL, &[])
                .await
                .expect_err("hata bekleniyordu");
            assert_eq!(error, expected, "durum: {status_line}");
        }
    }

    #[tokio::test]
    async fn rejects_malformed_or_empty_completions() {
        for body in [
            r#"{"choices":[]}"#,
            r#"{"choices":[{"message":{"role":"assistant"}}]}"#,
            r#"{"choices":[{"message":{"role":"assistant","content":"   "}}]}"#,
            "<html>gateway</html>",
        ] {
            let server = MockServer::start("200 OK", body);
            let error = server
                .service()
                .complete(&secret(TEST_API_KEY), TEST_MODEL, &[])
                .await
                .expect_err("bozuk govde hata uretmeli");
            assert_eq!(error, ChatUpstreamError::MalformedResponse, "govde: {body}");
        }
    }

    // --- Prompt insasi ------------------------------------------------------

    #[test]
    fn the_history_keeps_the_newest_turns_within_the_character_budget() {
        let long = "x".repeat(MAX_HISTORY_CHARS / 2);
        let history = vec![
            MessageRecord {
                id: 1,
                session_id: 1,
                role: MessageRole::User,
                content: long.clone(),
                created_at: NOW.to_owned(),
            },
            MessageRecord {
                id: 2,
                session_id: 1,
                role: MessageRole::Assistant,
                content: long.clone(),
                created_at: NOW.to_owned(),
            },
            MessageRecord {
                id: 3,
                session_id: 1,
                role: MessageRole::User,
                content: "en yeni".to_owned(),
                created_at: NOW.to_owned(),
            },
        ];

        let rendered = render_history(&history);
        assert_eq!(rendered.len(), 2, "butceyi asan en eski mesaj dusmeli");
        assert_eq!(rendered[0].role, "assistant");
        assert_eq!(rendered[1].content, "en yeni");
    }

    #[test]
    fn tool_and_system_rows_are_carried_as_system_messages() {
        assert_eq!(api_role(MessageRole::User), "user");
        assert_eq!(api_role(MessageRole::Assistant), "assistant");
        assert_eq!(api_role(MessageRole::System), "system");
        assert_eq!(api_role(MessageRole::Tool), "system");
    }

    #[test]
    fn the_prompt_states_that_asuna_cannot_act_in_this_conversation() {
        assert!(CHAT_SYSTEM_PROMPT_V1.contains("uydurma"));
        assert!(CHAT_SYSTEM_PROMPT_V1.contains("komut calistirma"));
        assert!(CHAT_SYSTEM_PROMPT_V1.contains("<redacted>"));
        assert_eq!(CHAT_PROMPT_VERSION, "core-chat.v1");
    }

    #[test]
    fn attachment_blocks_stay_inside_the_prompt_budget() {
        let payload = |id: i64, size: usize| AttachmentPayload {
            record: AttachmentRecord {
                id,
                session_id: 1,
                message_id: None,
                file_name: format!("dosya-{id}.txt"),
                mime_type: None,
                size_bytes: None,
                origin: AttachmentOrigin::Upload,
                created_at: NOW.to_owned(),
            },
            content: "y".repeat(size),
        };

        let attachments = [
            payload(1, MAX_ATTACHMENT_PROMPT_CHARS),
            payload(2, MAX_ATTACHMENT_PROMPT_CHARS),
        ];
        let rendered = render_user_content("soru", &attachments);

        assert!(rendered.contains("--- Ekli dosya: dosya-1.txt ---"));
        assert!(
            rendered.contains(ATTACHMENT_BUDGET_NOTICE.trim()),
            "sigmayan ek sessizce dusmemeli"
        );
        assert!(!rendered.contains("dosya-2.txt"), "butce asildi");
    }

    // --- attachment_ingest --------------------------------------------------

    #[test]
    fn an_uploaded_env_file_is_stored_with_its_secrets_masked() {
        let (db, session_id) = db_with_conversation();
        let content = "OPENAI_API_KEY=sk-proj-COK-GIZLI-DEGER\nDB_PASSWORD=hunter2\nPORT=3000\n";

        let record =
            ingest(&db, session_id, "ornek.txt", content, None).expect("metin dosyasi eklenmeli");

        let stored = attachment_repository::for_ids(&db, session_id, &[record.id])
            .expect("okuma")
            .pop()
            .expect("kayit");

        assert!(
            !stored.content.contains("COK-GIZLI-DEGER"),
            "{}",
            stored.content
        );
        assert!(!stored.content.contains("hunter2"), "{}", stored.content);
        assert!(
            stored.content.contains(crate::redaction::REDACTION_MARKER),
            "{}",
            stored.content
        );
        // Gizli olmayan satir bozulmaz: suzgec metni silmiyor, maskeliyor.
        assert!(stored.content.contains("PORT=3000"), "{}", stored.content);
        assert!(
            stored.content.starts_with("OPENAI_API_KEY="),
            "{}",
            stored.content
        );
    }

    #[test]
    fn an_over_long_upload_is_clipped_with_a_visible_marker() {
        let (db, session_id) = db_with_conversation();
        let content = "a".repeat(MAX_STORED_ATTACHMENT_CHARS + 500);

        let record = ingest_text(&db, session_id, "uzun.txt", &content);
        let stored = attachment_repository::for_ids(&db, session_id, &[record.id])
            .expect("okuma")
            .pop()
            .expect("kayit");

        assert!(stored.content.ends_with(TRUNCATION_NOTICE));
        assert_eq!(
            stored.content.chars().count(),
            MAX_STORED_ATTACHMENT_CHARS + TRUNCATION_NOTICE.chars().count()
        );
        // Kaynak boyutu kirpilmis metnin degil, gelen metnin boyutu.
        assert_eq!(stored.record.size_bytes, Some(content.len() as i64));
    }

    #[test]
    fn a_utf8_boundary_is_respected_when_clipping() {
        let (db, session_id) = db_with_conversation();
        let content = "ç".repeat(MAX_STORED_ATTACHMENT_CHARS + 10);

        let record = ingest_text(&db, session_id, "turkce.txt", &content);
        let stored = attachment_repository::for_ids(&db, session_id, &[record.id])
            .expect("okuma")
            .pop()
            .expect("kayit");
        assert!(stored.content.starts_with('ç'));
    }

    #[test]
    fn binary_content_is_refused() {
        let (db, session_id) = db_with_conversation();
        for content in [
            "PK\u{3}\u{4}\u{0}\u{0}ikili",
            &format!("metin{}", "\u{FFFD}".repeat(50)),
            &format!("{}gizli", "\u{1}\u{2}\u{3}".repeat(20)),
        ] {
            let error = ingest(&db, session_id, "dosya.bin", content, None)
                .expect_err("ikili icerik reddedilmeli");
            assert_eq!(error.code(), StoreErrorCode::Invalid);
            assert!(
                error.to_string().contains("yalnizca metin dosyalari"),
                "mesaj: {error}"
            );
        }
    }

    #[test]
    fn ordinary_text_with_a_few_control_characters_is_still_accepted() {
        let (db, session_id) = db_with_conversation();
        let content = format!("satir\tbir\r\nsatir iki\n{}", "u".repeat(500));
        assert!(ingest(&db, session_id, "duz.txt", &content, None).is_ok());
    }

    /// **Kabul kriteri (plan 3)**: `.env` **ADI** reddedilir; liste
    /// `security::blocklist`ten gelir, kopyalanmaz.
    #[test]
    fn blocklisted_file_names_are_refused() {
        let (db, session_id) = db_with_conversation();
        for name in [
            ".env",
            ".env.local",
            "production.env",
            "id_rsa",
            "id_ed25519.pub",
            "sunucu.pem",
            "ozel.key",
            "kasa.p12",
            "giris.keychain",
            ".npmrc",
            ".git-credentials",
            "aws-credentials.json",
        ] {
            let error = refusal(ingest(&db, session_id, name, "icerik", None), name);
            assert_eq!(error.code(), StoreErrorCode::Invalid, "ad: {name}");
            assert!(
                error.to_string().contains("eklenemez"),
                "ad: {name}, mesaj: {error}"
            );
        }
    }

    #[test]
    fn a_file_name_that_is_a_path_is_refused() {
        let (db, session_id) = db_with_conversation();
        for name in ["../../.ssh/id_ed25519", "alt/dizin/notlar.md", "..", "."] {
            let error = refusal(ingest(&db, session_id, name, "icerik", None), name);
            assert_eq!(error.code(), StoreErrorCode::Invalid, "ad: {name}");
        }
    }

    #[test]
    fn an_upload_over_the_input_cap_is_refused_not_clipped() {
        let (db, session_id) = db_with_conversation();
        let content = "a".repeat(MAX_INGEST_CHARS + 1);

        let error =
            ingest(&db, session_id, "dev.txt", &content, None).expect_err("girdi tavani asilmali");
        assert_eq!(error.code(), StoreErrorCode::Invalid);
        assert!(attachment_repository::list_for_session(&db, session_id)
            .expect("okuma")
            .is_empty());
    }

    #[test]
    fn an_attachment_for_an_unknown_conversation_is_refused() {
        let (db, _) = db_with_conversation();
        let error = ingest(&db, 9_999, "notlar.md", "icerik", None).expect_err("konusma yok");
        assert_eq!(error.code(), StoreErrorCode::NotFound);
    }

    // --- attachment_from_project -------------------------------------------

    #[test]
    fn a_project_file_is_attached_through_the_sandbox() {
        let fixture = project_fixture("attach");
        write_file(&fixture, "README.md", "# Asuna\nSesli companion.\n");

        let record = ingest_from_project(&fixture.db, fixture.session_id, "README.md")
            .expect("proje dosyasi eklenmeli");

        assert_eq!(record.origin, AttachmentOrigin::Project);
        assert_eq!(record.file_name, "README.md");
        assert_eq!(record.mime_type, None, "tur uydurulmamali");

        let stored = attachment_repository::for_ids(&fixture.db, fixture.session_id, &[record.id])
            .expect("okuma")
            .pop()
            .expect("kayit");
        assert!(stored.content.contains("Sesli companion."));
    }

    /// **Kabul kriteri (plan 3)**: proje icindeki bir secret redakte edilerek
    /// saklanir — redaksiyon `projects::files::read` yolundan geliyor.
    #[test]
    fn a_secret_inside_a_project_file_is_masked_before_it_is_stored() {
        let fixture = project_fixture("redaction");
        write_file(&fixture, "notlar.md", "anahtar: sk-proj-COK-GIZLI-DEGER\n");

        let record =
            ingest_from_project(&fixture.db, fixture.session_id, "notlar.md").expect("eklenmeli");
        let stored = attachment_repository::for_ids(&fixture.db, fixture.session_id, &[record.id])
            .expect("okuma")
            .pop()
            .expect("kayit");

        assert!(
            !stored.content.contains("COK-GIZLI-DEGER"),
            "{}",
            stored.content
        );
        assert!(
            stored.content.contains("sk-<redacted>"),
            "{}",
            stored.content
        );
    }

    /// **Kabul kriteri (plan 4)**: kok disina cikan yol reddedilir; sandbox
    /// kararinin kendisi tasinir, burada yeniden yorumlanmaz.
    #[test]
    fn paths_outside_the_project_root_are_refused() {
        let fixture = project_fixture("escape");
        for path in [
            "../../.ssh/id_ed25519",
            "/etc/passwd",
            "~/.aws/credentials",
            "../gizli.txt",
        ] {
            let error = refusal(
                ingest_from_project(&fixture.db, fixture.session_id, path),
                path,
            );
            assert!(
                matches!(error, ChatError::Project(_)),
                "yol: {path}, hata: {error}"
            );
        }
        assert!(
            attachment_repository::list_for_session(&fixture.db, fixture.session_id)
                .expect("okuma")
                .is_empty()
        );
    }

    /// Kok **icindeki** `.env` de okunmaz: "projenin kendi dosyasi" istisnasi yok.
    #[test]
    fn a_blocklisted_file_inside_the_root_is_refused() {
        let fixture = project_fixture("blocked");
        write_file(&fixture, ".env", "OPENAI_API_KEY=sk-proj-GIZLI\n");

        let error =
            ingest_from_project(&fixture.db, fixture.session_id, ".env").expect_err("bloklu");
        assert!(matches!(error, ChatError::Project(_)), "hata: {error}");
    }

    /// V1 kurali: `read` her zaman **aktif** projeye gore cozer. Konusmanin
    /// projesi aktif degilse sessizce yanlis kokten okumak yerine durust hata.
    #[test]
    fn a_conversation_whose_project_is_not_active_is_refused() {
        let fixture = project_fixture("mismatch");
        write_file(&fixture, "README.md", "# Asuna\n");

        let other = TempDir::new("mismatch-other");
        let outcome = registry::add(
            &fixture.db,
            &other.path().to_string_lossy(),
            Some("Diger"),
            NOW,
        )
        .expect("ikinci proje");
        let other_project = match outcome {
            ProjectAddOutcome::Registered { project }
            | ProjectAddOutcome::AlreadyRegistered { project } => project,
        };
        // "Guncel proje" = en son acilan (registry::set_current sozlesmesi), yani
        // ikinci secim NOW'dan **sonra** olmali; ayni damgada esitlik olurdu.
        registry::set_current(&fixture.db, &other_project.id, LATER).expect("aktif proje degisti");
        assert_ne!(other_project.id, fixture.project_id);

        let error = ingest_from_project(&fixture.db, fixture.session_id, "README.md")
            .expect_err("uyusmazlik reddedilmeli");

        assert_eq!(error.code(), StoreErrorCode::Invalid);
        assert!(error.to_string().contains("aktif yapin"), "mesaj: {error}");
    }

    #[test]
    fn a_conversation_without_a_project_cannot_attach_project_files() {
        let fixture = project_fixture("projectless");
        let session = session_repository::start_with_modality(
            &fixture.db,
            TEST_MODEL,
            None,
            SessionModality::Text,
            NOW,
        )
        .expect("projesiz konusma")
        .id;

        let error = ingest_from_project(&fixture.db, session, "README.md")
            .expect_err("projesiz konusma reddedilmeli");
        assert_eq!(error.code(), StoreErrorCode::Invalid);
    }

    // --- Dosya adi: buyuk/kucuk harf, bosluk, ayirici, NUL ------------------

    /// Blok listesi ASCII'de **buyuk/kucuk harf duyarsiz** ve ad once
    /// kirpiliyor: `.ENV` ya da `"  .env  "` yazarak liste atlanamaz.
    ///
    /// Mevcut `blocklisted_file_names_are_refused` testi yalnizca kucuk harfli
    /// adlari deniyordu; kacis denemesi tam olarak bu varyantlarda olur.
    #[test]
    fn the_file_name_block_list_ignores_letter_case_and_padding() {
        let (db, session_id) = db_with_conversation();
        for name in [
            ".ENV",
            ".Env.Local",
            "PRODUCTION.ENV",
            "ID_RSA",
            "Id_Ed25519.pub",
            "Sunucu.PEM",
            "gizli.Key",
            "Kasa.P12",
            "  .env  ",
            "\t.npmrc ",
            " AWS-Credentials.json",
        ] {
            let error = refusal(ingest(&db, session_id, name, "icerik", None), name);
            assert_eq!(error.code(), StoreErrorCode::Invalid, "ad: {name}");
        }

        assert!(
            attachment_repository::list_for_session(&db, session_id)
                .expect("okuma")
                .is_empty(),
            "reddedilen ad icin kayit yazilmis"
        );
    }

    /// `File.name` hicbir zaman ayirici ya da NUL icermez; iceren bir deger
    /// renderer'in **uydurdugu** bir seydir. Ters bolu ve NUL de reddedilir:
    /// aksi halde `is_blocked` yalnizca son bileseni gorur ve
    /// `notlar.md\0.env` gibi bir ad depoda kirpilmis olarak gorunurdu.
    #[test]
    fn a_file_name_carrying_a_separator_or_a_null_byte_is_refused() {
        let (db, session_id) = db_with_conversation();
        for name in [
            "..\\..\\.ssh\\id_rsa",
            "C:\\Users\\omer\\.env",
            "notlar.md\u{0}.env",
            "\u{0}",
            ".. /x",
            "/etc/passwd",
        ] {
            let error = refusal(ingest(&db, session_id, name, "icerik", None), name);
            assert_eq!(error.code(), StoreErrorCode::Invalid, "ad: {name}");
        }
    }

    /// **Katmanli savunma**: blok listesi bir **ad** kuralidir ve her supheli
    /// icerigi yakalayamaz. `yedek.txt` tamamen masum bir addir; icinde bir
    /// `.env` dokumu tasiyor olmasi adindan anlasilmaz. Bu durumda bile icerik
    /// redaksiyondan gecer — ad listesi tek savunma degildir.
    ///
    /// (Testin ilk hali `yedek.key.txt` kullaniyordu; tester B2 duzeltmesinden
    /// sonra o ad **reddediliyor** — bkz. `blocklist::appending_a_harmless_
    /// extension_does_not_bypass_the_list`. Katman-2 iddiasi icin adin gercekten
    /// gecmesi gerektiginden ornek masum bir ada tasindi.)
    #[test]
    fn content_is_still_masked_when_a_name_slips_past_the_block_list() {
        let (db, session_id) = db_with_conversation();
        let content = "AWS_SECRET_ACCESS_KEY=cok-gizli-deger\n\
                       OPENAI_API_KEY=sk-proj-COK-GIZLI\n\
                       Authorization: Bearer ek_test_tokeni\n\
                       PORT=3000\n";

        let record = ingest(&db, session_id, "yedek.txt", content, None)
            .expect("masum bir ad listeye takilmaz");

        let stored = attachment_repository::for_ids(&db, session_id, &[record.id])
            .expect("okuma")
            .pop()
            .expect("kayit");

        for secret in ["cok-gizli-deger", "sk-proj-COK-GIZLI", "ek_test_tokeni"] {
            assert!(
                !stored.content.contains(secret),
                "maskelenmemis deger: {secret} / {}",
                stored.content
            );
        }
        assert!(stored.content.contains("PORT=3000"), "{}", stored.content);
    }

    /// Ekin **modele giden** hali de maskelenmis olmali: `attachments.content`
    /// zaten redakte edilmis metni tutuyor ve prompt onu okuyor — ham dosya
    /// ikinci bir yoldan istege sizmamali.
    #[tokio::test]
    async fn an_attachment_secret_never_reaches_the_model() {
        let server = MockServer::start("200 OK", REPLY_BODY);
        let (db, session_id) = db_with_conversation();
        let attachment = ingest_text(
            &db,
            session_id,
            "ayarlar.txt",
            "DB_PASSWORD=hunter2\nOPENAI_API_KEY=sk-proj-COK-GIZLI\n",
        );

        send(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            session_id,
            "bu ayarlari acikla",
            &[attachment.id],
        )
        .await
        .expect("yanit gelmeli");

        let body = server.request().body;
        assert!(!body.contains("hunter2"), "govde: {body}");
        assert!(!body.contains("sk-proj-COK-GIZLI"), "govde: {body}");
        assert!(
            body.contains(crate::redaction::REDACTION_MARKER),
            "govde: {body}"
        );
        // Dosyanin **adi** gider (kullanici neyi ekledigini bilsin), icerigi degil.
        assert!(body.contains("ayarlar.txt"), "govde: {body}");
    }

    // --- Yazmanin butunlugu -------------------------------------------------

    /// Asistan yaniti yazilamiyorsa kullanici mesaji da yazilmaz.
    ///
    /// `send` bu durumu servis katmaninda zaten eliyor; buradaki kapi ikincisi
    /// ve **yazma** adiminin kendisine ait: yarim bir "kullanici sordu, cevap
    /// yok" satiri kalmamali.
    #[test]
    fn an_unusable_reply_leaves_no_half_exchange_behind() {
        let (db, session_id) = db_with_conversation();

        for reply in ["", "   ", "\n\t"] {
            let error = persist_exchange(&db, session_id, "soru", reply, &[])
                .expect_err("bos yanit yazilmamali");
            assert_eq!(error.code(), StoreErrorCode::Invalid, "yanit: {reply:?}");
        }

        assert!(message_repository::list_for_session(&db, session_id, 100)
            .expect("okuma")
            .is_empty());
    }

    /// Model cagrisi surerken konusma silinirse yazma **tumden** duser: silinmis
    /// bir konusmaya mesaj geri dogmaz.
    #[test]
    fn a_conversation_deleted_during_the_call_gets_no_write_at_all() {
        let (db, session_id) = db_with_conversation();
        ingest_text(&db, session_id, "notlar.md", "icerik");

        session_repository::delete(&db, session_id).expect("konusma silinmeli");

        let error = persist_exchange(&db, session_id, "soru", "cevap", &[])
            .expect_err("silinmis konusmaya yazilmamali");
        assert_eq!(error.code(), StoreErrorCode::NotFound);

        let messages: i64 = db
            .with_connection(|connection| {
                connection.query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
            })
            .expect("sayim");
        let attachments: i64 = db
            .with_connection(|connection| {
                connection.query_row("SELECT count(*) FROM attachments", [], |row| row.get(0))
            })
            .expect("sayim");
        assert_eq!(messages, 0, "sahipsiz mesaj yazildi");
        assert_eq!(attachments, 0, "ek CASCADE ile gitmeliydi");
    }

    // --- Gizlilik / hata yuzeyi --------------------------------------------

    #[test]
    fn no_upstream_error_variant_leaks_secret_material() {
        let variants = [
            ChatUpstreamError::MissingApiKey,
            ChatUpstreamError::InvalidApiKey,
            ChatUpstreamError::ModelAccessDenied,
            ChatUpstreamError::QuotaExceeded,
            ChatUpstreamError::Network {
                cause: NetworkCause::Timeout,
            },
            ChatUpstreamError::UpstreamUnavailable { status: 503 },
            ChatUpstreamError::UnexpectedStatus { status: 418 },
            ChatUpstreamError::MalformedResponse,
            ChatUpstreamError::HttpClientUnavailable,
        ];

        let mut kinds = Vec::new();
        for variant in &variants {
            let rendered = format!("{variant} | {variant:?}");
            assert!(!rendered.contains("sk-"), "sizinti: {rendered}");
            assert!(!rendered.contains(TEST_API_KEY), "sizinti: {rendered}");
            assert!(variant.to_string().len() > 20, "mesaj cok kisa: {variant}");
            kinds.push(variant.kind());
        }
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), variants.len(), "her varyantin kendi etiketi");
    }

    /// Model **ID'si** IPC'ye gitmez: `FrontendConfig` whitelist'i onu tasimiyor,
    /// bir hata mesajiyla arka kapidan gitmesi de anlamsiz olurdu.
    #[test]
    fn the_chat_model_id_never_reaches_the_renderer_through_an_error() {
        let json = serde_json::to_value(ChatError::Upstream(ChatUpstreamError::ModelAccessDenied))
            .expect("serialize");
        let message = json["message"].as_str().expect("mesaj");
        assert!(message.contains("ASUNA_CHAT_MODEL"), "{message}");
        assert!(!message.contains("gpt-"), "{message}");
    }

    /// IPC bicimi `StoreError` ile ayni: `{ code, message }` ve kod, renderer'in
    /// tanidigi dort degerden biri (`shared/store-error.ts`).
    #[test]
    fn errors_serialize_as_a_known_code_and_message_pair() {
        let cases = [
            (ChatError::invalid("`text` bos birakilamaz"), "invalid"),
            (ChatError::Store(StoreError::NotFound), "not-found"),
            (ChatError::MemoryDisabled, "unavailable"),
            (
                ChatError::Upstream(ChatUpstreamError::QuotaExceeded),
                "unavailable",
            ),
        ];

        for (error, expected_code) in cases {
            let json = serde_json::to_value(&error).expect("serialize");
            assert_eq!(json["code"], expected_code, "hata: {error}");
            assert!(
                json["message"].as_str().is_some_and(|m| !m.is_empty()),
                "hata: {error}"
            );

            let object = json.as_object().expect("JSON nesnesi");
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["code", "message"]);
        }
    }

    /// Servisin `Debug` ciktisi secret basmaz.
    #[test]
    fn service_debug_output_is_safe() {
        let debug = format!("{:?}", ChatService::new());
        assert!(debug.contains(CHAT_COMPLETIONS_URL));
        assert!(!debug.contains("sk-"), "debug: {debug}");
    }

    #[test]
    fn default_endpoint_is_the_documented_openai_url() {
        assert_eq!(
            CHAT_COMPLETIONS_URL,
            "https://api.openai.com/v1/chat/completions"
        );
        assert!(CHAT_COMPLETIONS_URL.starts_with("https://"), "TLS zorunlu");
    }
}

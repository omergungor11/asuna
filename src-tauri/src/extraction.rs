//! Hafiza cikarim boru hatti (ASU-034).
//!
//! PROJECT.md Bolum 26:
//! `konusma -> oturum ozeti -> aday hafizalar -> dogrulama/dedup -> kalici depolama`
//!
//! # Neden model DB'ye yazmiyor
//!
//! Realtime modeline "sunu veritabanina kaydet" dedirtmek, hafizanin icerigini
//! denetlenemez bir yan etkiye cevirir. Burada model yalnizca **aday** uretir;
//! neyin kaydedilecegine bu modul karar verir. Modelden gelen her aday
//! [`validate_candidate`] suzgecinden gecer ve tek bir adayin reddi digerlerini
//! dusurmez — "never invent memories" ilkesinin muhendislik karsiligi budur.
//!
//! # Kalici hale gelmeyen seyler
//!
//! - `working_context` ve `tool_state` (PROJECT.md Bolum 14): acik dosya,
//!   terminal hatasi, son tool ciktisi durable memory degildir. Talimatta
//!   listelenmezler; yine de gelirlerse kod **reddeder** (iki kat savunma).
//! - [`MIN_IMPORTANCE`] altindaki adaylar.
//! - Ayni kind + ayni proje altinda benzer bir kayit zaten varsa yeni satir
//!   acilmaz; mevcut kayit guncellenir ([`is_duplicate`]).
//!
//! # Hassas kategoriler onay bekler
//!
//! `profile` ve `relationship` adaylari **kaydedilir ama** `metadata_json`
//! icinde `"pendingApproval": true` bayragi ile isaretlenir. Stage A retrieval
//! (ASU-035) bu kayitlari **disarida birakir**; kullanici Memory ekranindan
//! onaylayip bayragi kaldirinca (`memory_update`) kalicilasir. Kayit kullanici
//! goremeden yok edilmez, ama onaylanmadan da modele gitmez (PROJECT.md
//! Bolum 26 sonu + Bolum 20 "storage is inspectable").
//!
//! # Ozeti asla geri almaz
//!
//! Cikarim, ozet yazildiktan **sonra** ayni arka plan gorevinde devam eder.
//! Hata halinde log'lanir ve durur: oturum kaydi da ozeti de oldugu gibi kalir.
//!
//! # Guvenlik sozlesmesi
//!
//! `summary.rs` ile ayni: kalici `OPENAI_API_KEY` yalnizca bu process'te,
//! `#[tauri::command]` yok (renderer bu modulu cagiramaz), yonlendirme kapali,
//! hicbir hata varyanti secret / API govdesi / hafiza icerigi tasimaz.

use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::Manager;
use thiserror::Error;

use crate::config::{AsunaConfig, SecretString};
use crate::db::clock;
use crate::db::memory_repository::{
    self, ArchiveFilter, MemoryDraft, MemoryFilter, MemoryPatch, MemorySort,
};
use crate::db::transcript::{TranscriptLine, TranscriptRole};
use crate::db::{AsunaDb, DbState, MemoryKind, MemoryRecord, StoreError};
use crate::realtime_token::NetworkCause;
use crate::redaction::{redact_secrets, redact_sensitive_text};
use crate::summary::CHAT_COMPLETIONS_URL;

// ---------------------------------------------------------------------------
// Sinirlar ve esikler
// ---------------------------------------------------------------------------

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Onem esigi: bunun **altindaki** aday kaydedilmez.
///
/// Bilerek `const`, yeni bir env anahtari degil: her esik icin zorunlu bir
/// `.env` anahtari acmak mevcut kurulumlari kirar ve kullaniciya anlamini
/// bilmedigi bir sayi sorar. Deger uretimde olculdukten sonra config'e
/// tasinabilir (ayni gerekce `MIN_TRANSCRIPT_LINES` icin de gecerliydi).
pub const MIN_IMPORTANCE: f64 = 0.5;

/// Tek oturumdan kabul edilen en fazla aday. Model talimata uymayip 50 madde
/// yazarsa hafiza tek oturumda sismez.
const MAX_CANDIDATES: usize = 8;

/// Aday `content` siniri. Uzun metin "ozet degil dokum" demektir; kirpmak
/// yarim cumle saklardi, bu yuzden **reddedilir**.
const MAX_CANDIDATE_CONTENT_CHARS: usize = 600;

/// Aday `title` siniri — asilirsa kirpilir (baslik kozmetik, icerik degil).
const MAX_CANDIDATE_TITLE_CHARS: usize = 120;

/// `projectId` siniri — `memory_repository` ile ayni.
const MAX_PROJECT_ID_CHARS: usize = 120;

/// Modele gonderilen dokumun karakter siniri (ozet zaten ayrica gonderiliyor).
const MAX_TRANSCRIPT_CHARS: usize = 12_000;

/// Dedup taramasinin ustune baktigi en fazla kayit
/// (`memory_repository::MAX_LIST_LIMIT` ile ayni).
///
/// **Bilincli sinir**: ayni kind + ayni proje altinda en yeni 200 kayda bakilir.
/// Tam tarama, hafiza buyudukce her oturum kapanisinda tum tabloyu okumak
/// demekti; semantik dedup zaten backlog'da (PROJECT.md Bolum 13 Stage C).
const DEDUP_SCAN_LIMIT: u32 = 200;

/// Alt dize eslesmesinin gecerli sayilmasi icin kisa metnin en az uzunlugu.
///
/// Bu esik olmasa "yok" gibi bir icerik her uzun kaydin icinde gecer ve
/// birbiriyle ilgisiz hafizalar "ayni" sayilirdi.
///
/// **Gate 3 / MEDIUM-4**: esik 12'den 40'a cikarildi. 12 karakter bir Turkce
/// cumlenin yarisi bile degil; "kahve sevmiyor" gibi bir kayit "esi kahve
/// sevmiyor" adayini yutuyordu — iki farkli kisi hakkindaki iki farkli hafiza
/// tek satira iniyor ve **geri alinamiyordu** (yeni icerik yazilmaz, yalnizca
/// onem guncellenir).
const MIN_SUBSET_CHARS: usize = 40;

/// Alt dize eslesmesinde kisa/uzun uzunluk oraninin alt siniri.
///
/// Uzunluk esigi tek basina yetmiyor: 40 karakterlik bir kayit 300 karakterlik
/// bir adayin icinde gecebilir ve ikisi ayni hafiza olmayabilir. Iki metin
/// "ayni sey" sayilacaksa boylari da birbirine yakin olmali.
const MIN_SUBSET_LENGTH_RATIO: f64 = 0.8;

/// Otomatik olarak kalici hafizaya **terfi etmeyen** turler (PROJECT.md Bolum 14).
pub const NON_DURABLE_KINDS: [MemoryKind; 2] = [MemoryKind::WorkingContext, MemoryKind::ToolState];

/// Kalici kayit oncesi **acik onay** isteyen turler (PROJECT.md Bolum 26 sonu).
pub const SENSITIVE_KINDS: [MemoryKind; 2] = [MemoryKind::Profile, MemoryKind::Relationship];

/// `metadata_json` icindeki onay bayragi. Stage A retrieval (ASU-035) bu
/// anahtari `true` olan kayitlari **gecirmez**.
pub const PENDING_APPROVAL_KEY: &str = "pendingApproval";

/// `usage_json` icinde cikarim maliyetinin yazildigi anahtar
/// (`$.summary` **ezilmez**).
pub const USAGE_KEY: &str = "extraction";

/// Cikarim talimati — **versiyonlu**. Degistirmek yeni bir sabit acmak demek;
/// hangi oturumdan hangi talimatla hafiza uretildigi izlenebilir kalsin.
///
/// `working_context` ve `tool_state` gecerli deger listesinde **yok**: model
/// gecici baglami aday olarak bile uretmemeli.
pub const MEMORY_EXTRACTION_PROMPT_V1: &str = "\
Sen bir hafiza cikarim adimisin. Sana bir sesli oturumun ozeti ve (varsa) dokumu verilir.
Gorevin: yalnizca **kalici olarak hatirlanmaya deger** bilgileri cikarmak.

Cikti **yalnizca** bir JSON dizisi olmali; oncesinde/sonrasinda hicbir metin, aciklama
ya da kod bloku isareti olmamali. Her eleman su alanlari tasir:
{\"kind\": \"...\", \"title\": \"...\", \"content\": \"...\", \"importance\": 0.0, \"confidence\": 0.0, \"projectId\": \"...\"}

Gecerli kind degerleri (baskasini kullanma):
profile, preference, project, decision, task, relationship, idea, routine

Kurallar:
- Yalnizca metinde acikca gecenleri yaz. Cikarim yapma, tamamlama, tahmin etme, uydurma.
- Gecici baglami **cikarma**: acik dosya, terminal hatasi, aktif branch, son tool ciktisi,
  o anki konusmanin akisi kalici hafiza degildir.
- content tek cumle ve kendi basina anlasilir olmali; \"bunu\", \"orasi\" gibi baglama
  bagli ifadeler kullanma.
- title en fazla bes kelimelik kisa bir etiket.
- importance ve confidence 0 ile 1 arasinda ondalik sayi olmali; emin degilsen dusuk yaz.
- projectId yalnizca metinde acikca bir proje adi geciyorsa yazilir; yoksa alani hic yazma.
- Hatirlanmaya deger bir sey yoksa bos dizi dondur: []
- En fazla 8 eleman.";

/// Cikarim talimatinin surumu — `usage_json` ve `metadata_json` icine yazilir.
pub const EXTRACTION_PROMPT_VERSION: &str = "memory-extraction.v1";

// ---------------------------------------------------------------------------
// Sonuc tipleri
// ---------------------------------------------------------------------------

/// Bir adayin **neden** reddedildigi. Log'da filtrelenebilir, testte
/// dogrulanabilir; kullanici icerigi tasimaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateRejection {
    /// Dizi elemani JSON nesnesi degil.
    NotAnObject,
    /// `kind` eksik ya da bilinen bir deger degil.
    UnknownKind,
    /// `working_context` / `tool_state` — durable memory'ye terfi etmez.
    NonDurableKind,
    /// `content` yok ya da bos.
    EmptyContent,
    /// `content` cok uzun (ozet degil dokum).
    ContentTooLong,
    /// `importance` / `confidence` yok, sayi degil ya da 0-1 disinda.
    InvalidScore,
    /// `importance` esigin altinda.
    BelowThreshold,
    /// `title` / `projectId` beklenen tipte degil ya da cok uzun.
    InvalidField,
}

impl CandidateRejection {
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotAnObject => "not_an_object",
            Self::UnknownKind => "unknown_kind",
            Self::NonDurableKind => "non_durable_kind",
            Self::EmptyContent => "empty_content",
            Self::ContentTooLong => "content_too_long",
            Self::InvalidScore => "invalid_score",
            Self::BelowThreshold => "below_threshold",
            Self::InvalidField => "invalid_field",
        }
    }
}

/// Dogrulamadan gecmis aday.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCandidate {
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub importance: f64,
    pub confidence: f64,
    pub project_id: Option<String>,
}

/// Cikarim cagrisinin token maliyeti.
///
/// **USD yok**: `summary.rs` ile ayni gerekce — cikarim modelinin fiyati
/// dogrulanmadi, olculen sey (token) kaydedilir, cevrimi uydurulmaz.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionUsage {
    pub model: String,
    pub prompt_version: &'static str,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    /// Her zaman `null` (bkz. tip dokumantasyonu).
    pub estimated_cost_usd: Option<f64>,
}

/// Modelden gelen ham cikti + dogrulama sonucu.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionResult {
    pub candidates: Vec<MemoryCandidate>,
    pub rejected: Vec<CandidateRejection>,
    pub usage: ExtractionUsage,
}

/// Depolama adiminin sayilari.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExtractionStats {
    /// Yeni acilan kayit.
    pub created: usize,
    /// Dedup ile mevcut kayda yazilan aday.
    pub updated: usize,
    /// Dogrulamadan gecemeyen aday.
    pub rejected: usize,
    /// Dogrulamayi gecti ama DB yazmasi basarisiz oldu.
    pub failed: usize,
}

/// Cikarim bilincli olarak **calismadi**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionSkipReason {
    /// `ASUNA_MEMORY_ENABLED=false` — model **cagrilmaz**, DB'ye dokunulmaz.
    MemoryDisabled,
    /// Ozet uretilemedigi icin cikarimin girdisi yok.
    NoSummary,
}

impl ExtractionSkipReason {
    pub const fn label(self) -> &'static str {
        match self {
            Self::MemoryDisabled => "memory_disabled",
            Self::NoSummary => "no_summary",
        }
    }
}

/// Boru hattinin sonucu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionOutcome {
    /// Cikarim calisti (sifir aday uretmis olabilir).
    Completed(ExtractionStats),
    Skipped(ExtractionSkipReason),
    /// Model ya da ag hatasi; ozet ve oturum kaydi **dokunulmadan** kaldi.
    Failed,
}

// ---------------------------------------------------------------------------
// Hata tipi
// ---------------------------------------------------------------------------

/// Cikarim cagrisinin ayirt edilmis hata durumlari.
///
/// IPC'ye gitmez; log'a duser. Hicbir varyant secret, dokum, hafiza icerigi ya
/// da API govdesi tasimaz.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExtractionError {
    #[error("OpenAI API anahtari tanimli degil; hafiza cikarimi yapilamadi.")]
    MissingApiKey,

    #[error(
        "OpenAI API anahtari gecersiz (yetkilendirme reddedildi); hafiza cikarimi yapilamadi."
    )]
    InvalidApiKey,

    #[error(
        "Bu hesabin `{model}` modeline erisimi yok. `ASUNA_SUMMARY_MODEL` degerini \
         erisiminiz olan bir modele ayarlayin."
    )]
    ModelAccessDenied { model: String },

    #[error("OpenAI kota sinirina takildi; hafiza cikarimi yapilamadi.")]
    QuotaExceeded,

    #[error("OpenAI'ya ulasilamadi ({}); hafiza cikarimi yapilamadi.", cause.as_turkish())]
    Network { cause: NetworkCause },

    #[error("OpenAI cikarim servisi yanit vermiyor (HTTP {status}); hafiza cikarimi yapilamadi.")]
    UpstreamUnavailable { status: u16 },

    #[error("OpenAI beklenmeyen bir yanit dondu (HTTP {status}); hafiza cikarimi yapilamadi.")]
    UnexpectedStatus { status: u16 },

    #[error("OpenAI'nin cikarim yaniti okunamadi (beklenen alanlar eksik veya bos).")]
    MalformedResponse,

    #[error("Modelin cikarim yaniti JSON dizisi degil; hicbir aday kabul edilmedi.")]
    NotAJsonArray,

    #[error("Guvenli HTTPS istemcisi kurulamadi; hafiza cikarimi yapilamadi.")]
    HttpClientUnavailable,
}

impl ExtractionError {
    /// Log'da filtrelenebilir stabil etiket.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::MissingApiKey => "missing_api_key",
            Self::InvalidApiKey => "invalid_api_key",
            Self::ModelAccessDenied { .. } => "model_access_denied",
            Self::QuotaExceeded => "quota_exceeded",
            Self::Network { .. } => "network",
            Self::UpstreamUnavailable { .. } => "upstream_unavailable",
            Self::UnexpectedStatus { .. } => "unexpected_status",
            Self::MalformedResponse => "malformed_response",
            Self::NotAJsonArray => "not_a_json_array",
            Self::HttpClientUnavailable => "http_client_unavailable",
        }
    }

    fn from_status(status: u16, model: &str) -> Self {
        match status {
            401 => Self::InvalidApiKey,
            403 | 404 => Self::ModelAccessDenied {
                model: model.to_owned(),
            },
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

// ---------------------------------------------------------------------------
// Istek / yanit semalari
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'static str,
    content: &'a str,
}

/// Govde bilerek **minimum**: yalnizca `model` + `messages` (`summary.rs` ile
/// ayni gerekce). `response_format` gibi alanlarin bu hesapta/modelde davranisi
/// dogrulanmadi; JSON talimatla isteniyor ve gelen metin savunmaci sekilde
/// ayristiriliyor ([`parse_candidate_array`]).
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: [ChatMessage<'a>; 2],
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
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

#[derive(Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: Option<i64>,
    #[serde(default)]
    completion_tokens: Option<i64>,
    #[serde(default)]
    total_tokens: Option<i64>,
}

// ---------------------------------------------------------------------------
// Girdi hazirligi
// ---------------------------------------------------------------------------

fn speaker(role: TranscriptRole) -> &'static str {
    match role {
        TranscriptRole::User => "Kullanici",
        TranscriptRole::Assistant => "Asuna",
    }
}

/// Modele gidecek kullanici mesaji: ozet **zorunlu**, dokum destekleyici.
///
/// Ozet once yazilir: cikarimin birincil girdisi odur (PROJECT.md Bolum 26
/// boru hatti). Dokum yalnizca ozetin kaybettigi ayrinti icin ve sondan
/// kirpilarak eklenir.
fn render_input(summary_text: &str, lines: &[TranscriptLine]) -> String {
    let mut rendered = format!("Oturum ozeti:\n{}", summary_text.trim());

    let mut entries: Vec<String> = Vec::new();
    let mut budget = MAX_TRANSCRIPT_CHARS;
    for line in lines.iter().rev() {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        let entry = format!("{}: {text}", speaker(line.role));
        if entry.chars().count() + 1 > budget {
            break;
        }
        budget -= entry.chars().count() + 1;
        entries.push(entry);
    }

    if !entries.is_empty() {
        entries.reverse();
        rendered.push_str("\n\nDokum:\n");
        rendered.push_str(&entries.join("\n"));
    }
    rendered
}

// ---------------------------------------------------------------------------
// Cikti ayristirma + dogrulama
// ---------------------------------------------------------------------------

/// Modelin metnini JSON dizisine cevirir.
///
/// Talimat "yalnizca dizi" diyor ama modeller sik sik ` ```json ` blogu ya da
/// `{"memories": [...]}` sarmalayicisi uretiyor. Bu iki bicim **tolere edilir**;
/// baska her sey hata olur (tahmin ederek icerik uydurmayiz).
fn parse_candidate_array(raw: &str) -> Result<Vec<Value>, ExtractionError> {
    let mut text = raw.trim();
    if text.starts_with("```") {
        text = text.trim_start_matches('`');
        text = text.strip_prefix("json").unwrap_or(text);
        text = text.trim_start_matches('`').trim();
        if let Some(end) = text.rfind("```") {
            text = text[..end].trim();
        }
    }
    if text.is_empty() {
        return Err(ExtractionError::MalformedResponse);
    }

    let parsed: Value = serde_json::from_str(text).map_err(|_| ExtractionError::NotAJsonArray)?;
    match parsed {
        Value::Array(items) => Ok(items),
        Value::Object(object) => object
            .get("memories")
            .and_then(Value::as_array)
            .cloned()
            .ok_or(ExtractionError::NotAJsonArray),
        _ => Err(ExtractionError::NotAJsonArray),
    }
}

fn string_field(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, CandidateRejection> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_owned()))
            }
        }
        Some(_) => Err(CandidateRejection::InvalidField),
    }
}

/// Skor **uydurulmaz**: eksik ya da sayi olmayan deger varsayilana cekilmez,
/// aday reddedilir.
fn score_field(object: &Map<String, Value>, key: &str) -> Result<f64, CandidateRejection> {
    match object.get(key).and_then(Value::as_f64) {
        Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => Ok(value),
        _ => Err(CandidateRejection::InvalidScore),
    }
}

/// `content`'ten kisa bir baslik turetir (model `title` vermediyse).
fn derive_title(content: &str) -> String {
    let first_sentence = content
        .split_terminator(['.', '!', '?', '\n'])
        .next()
        .unwrap_or(content)
        .trim();
    let source = if first_sentence.is_empty() {
        content.trim()
    } else {
        first_sentence
    };
    clip(source, 60)
}

/// Karakter sinirinda kirpar (byte ile kesmek UTF-8'i bozar).
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let clipped: String = text.chars().take(max).collect();
    format!("{}…", clipped.trim_end())
}

pub fn is_durable(kind: MemoryKind) -> bool {
    !NON_DURABLE_KINDS.contains(&kind)
}

pub fn requires_approval(kind: MemoryKind) -> bool {
    SENSITIVE_KINDS.contains(&kind)
}

/// Tek bir adayi dogrular. Reddedilen aday digerlerini **dusurmez**; cagiran
/// tarafta sayilir ve log'lanir.
pub fn validate_candidate(raw: &Value) -> Result<MemoryCandidate, CandidateRejection> {
    let object = raw.as_object().ok_or(CandidateRejection::NotAnObject)?;

    let kind = string_field(object, "kind")?
        .as_deref()
        .and_then(MemoryKind::parse)
        .ok_or(CandidateRejection::UnknownKind)?;
    if !is_durable(kind) {
        return Err(CandidateRejection::NonDurableKind);
    }

    let content = string_field(object, "content")?.ok_or(CandidateRejection::EmptyContent)?;
    if content.chars().count() > MAX_CANDIDATE_CONTENT_CHARS {
        return Err(CandidateRejection::ContentTooLong);
    }

    let importance = score_field(object, "importance")?;
    let confidence = score_field(object, "confidence")?;
    if importance < MIN_IMPORTANCE {
        return Err(CandidateRejection::BelowThreshold);
    }

    let project_id = string_field(object, "projectId")?;
    if project_id
        .as_ref()
        .is_some_and(|value| value.chars().count() > MAX_PROJECT_ID_CHARS)
    {
        return Err(CandidateRejection::InvalidField);
    }

    let title = match string_field(object, "title")? {
        Some(value) => clip(&value, MAX_CANDIDATE_TITLE_CHARS),
        None => derive_title(&content),
    };

    Ok(MemoryCandidate {
        kind,
        title,
        content,
        importance,
        confidence,
        project_id,
    })
}

// ---------------------------------------------------------------------------
// Deduplication (deterministik — semantik dedup backlog'da)
// ---------------------------------------------------------------------------

/// Karsilastirma icin metni sadelestirir: kucuk harf, noktalama -> bosluk,
/// bosluk yigilmasi tek boslugа iner.
///
/// **Sinir**: `to_lowercase` Unicode katlamasi yapar ama Turkce'ye ozgu
/// `I/ı` ayrimi her zaman beklendigi gibi degildir; bu MVP icin kabul edilen
/// bir kayip (`memory_repository`'deki `LIKE` notu ile ayni gerekce).
pub fn normalize_for_dedup(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            pending_space = false;
            normalized.extend(character.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    normalized
}

/// Iki normalize metin "ayni hafiza" mi?
///
/// Kural bilerek dar: birebir esitlik ya da **hem yeterince uzun hem de
/// boyca yakin** bir tam alt dize iliskisi. Bulanik benzerlik (Levenshtein,
/// embedding) **yok** — yanlis pozitif, kullanicinin farkli iki hafizasini
/// birlestirir ve geri alinamaz.
///
/// Alt dize kolu iki kosulu birden ister ([`MIN_SUBSET_CHARS`],
/// [`MIN_SUBSET_LENGTH_RATIO`]): kisa metin anlamli uzunlukta olmali **ve**
/// uzun metnin cogunu kaplamali. "esi kahve sevmiyor" adayi "kahve sevmiyor"
/// kaydinin kopyasi degildir; ikisi de saklanir.
pub fn is_duplicate(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left == right {
        return true;
    }
    let (short, long) = if left.chars().count() <= right.chars().count() {
        (left, right)
    } else {
        (right, left)
    };

    let short_len = short.chars().count();
    let long_len = long.chars().count();
    if short_len < MIN_SUBSET_CHARS {
        return false;
    }
    // `long_len` burada en az `short_len` (>0), yani bolme guvenli.
    if (short_len as f64) / (long_len as f64) < MIN_SUBSET_LENGTH_RATIO {
        return false;
    }
    long.contains(short)
}

/// Adayin karsiligi olan mevcut kaydi arar.
///
/// Arsivlenmis kayitlar da taranir: kullanici bir hafizayi arsivlediyse ayni
/// bilgiyi yeni bir satir olarak geri getirmek onun kararini gecersiz kilardi.
/// Guncelleme arsiv durumuna dokunmaz — kayit arsivde kalir.
fn find_duplicate(
    db: &AsunaDb,
    candidate: &MemoryCandidate,
    now: &str,
) -> Result<Option<MemoryRecord>, StoreError> {
    let filter = MemoryFilter {
        kinds: vec![candidate.kind],
        project_id: candidate.project_id.clone(),
        archived: ArchiveFilter::All,
        include_expired: true,
        sort: MemorySort::Recent,
        limit: Some(DEDUP_SCAN_LIMIT),
        ..MemoryFilter::default()
    };

    let target = normalize_for_dedup(&candidate.content);
    Ok(memory_repository::list(db, &filter, now)?
        .into_iter()
        .find(|record| {
            record.project_id.as_deref() == candidate.project_id.as_deref()
                && is_duplicate(&normalize_for_dedup(&record.content), &target)
        }))
}

// ---------------------------------------------------------------------------
// Depolama
// ---------------------------------------------------------------------------

/// Adayin saklanacak metinlerini redakte eder (Gate 3 / HIGH-2).
///
/// `project_id` **dokunulmadan** gecer: proje kimligi bir tanimlayicidir,
/// serbest metin degil; zaten `validate_candidate` tarafindan sinirlaniyor.
fn redact_candidate(candidate: &MemoryCandidate) -> MemoryCandidate {
    MemoryCandidate {
        title: redact_sensitive_text(&candidate.title),
        content: redact_sensitive_text(&candidate.content),
        ..candidate.clone()
    }
}

/// Yeni kaydin `metadata_json`'i.
///
/// `pendingApproval` her zaman **acikca** yazilir (`false` da): retrieval
/// tarafinda "anahtar yoksa ne demek?" sorusu kalmasin.
fn metadata_for(kind: MemoryKind) -> String {
    serde_json::json!({
        PENDING_APPROVAL_KEY: requires_approval(kind),
        "extraction": { "promptVersion": EXTRACTION_PROMPT_VERSION },
    })
    .to_string()
}

/// Adaylari kalici hale getirir: benzerini bulursa gunceller, bulamazsa acar.
///
/// Tek bir adayin DB hatasi digerlerini dusurmez; hata log'lanir ve
/// [`ExtractionStats::failed`] artar.
///
/// # Redaksiyon (Gate 3 / HIGH-2)
///
/// Her adayin `title` + `content` alani yazmadan **once**
/// [`redact_sensitive_text`] suzgecinden gecer. Suzgec dedup taramasindan da
/// once uygulanir: karsilastirilan metin ile saklanan metin ayni olmali, aksi
/// halde ayni bilgi bir kez maskeli bir kez maskesiz saklanabilirdi
/// (`asuna-config/security.md` Bolum 5).
pub fn persist_candidates(
    db: &AsunaDb,
    session_id: i64,
    candidates: &[MemoryCandidate],
    now: &str,
) -> ExtractionStats {
    let mut stats = ExtractionStats::default();

    for candidate in candidates {
        let candidate = &redact_candidate(candidate);
        let existing = match find_duplicate(db, candidate, now) {
            Ok(existing) => existing,
            Err(error) => {
                eprintln!("[asuna] Hafiza adayi icin dedup taramasi basarisiz: {error}");
                stats.failed += 1;
                continue;
            }
        };

        let outcome = match existing {
            // Var olan kayit guncellenir: yeni satir acilmaz, onem **maksimum**
            // alinir (bir kez onemli oldugu anlasilan bilgi degersizlesmez) ve
            // `updated_at` tazelenir. Icerik/metadata **ezilmez**: kullanicinin
            // duzenledigi metin ya da verdigi onay kaybolmamali.
            Some(record) => memory_repository::update(
                db,
                record.id,
                &MemoryPatch {
                    importance: Some(record.importance.max(candidate.importance)),
                    ..MemoryPatch::default()
                },
                now,
            )
            .map(|_| false),
            None => memory_repository::create(
                db,
                &MemoryDraft {
                    kind: candidate.kind,
                    title: candidate.title.clone(),
                    content: candidate.content.clone(),
                    summary: None,
                    project_id: candidate.project_id.clone(),
                    importance: candidate.importance,
                    confidence: candidate.confidence,
                    // Kaynak izlenebilir: "bu neden hatirlaniyor?" sorusunun
                    // cevabi oturum kaydidir (PROJECT.md Bolum 20).
                    source_session_id: Some(session_id),
                    expires_at: None,
                    metadata_json: Some(metadata_for(candidate.kind)),
                },
                now,
            )
            .map(|_| true),
        };

        match outcome {
            Ok(true) => stats.created += 1,
            Ok(false) => stats.updated += 1,
            Err(error) => {
                // Icerik log'lanmaz; yalnizca tur ve hata.
                eprintln!(
                    "[asuna] `{}` turunde hafiza adayi kaydedilemedi: {error}",
                    candidate.kind.as_str()
                );
                stats.failed += 1;
            }
        }
    }

    stats
}

/// Cikarim maliyetini ve sayilarini oturum metadata'sina yamalar.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsagePatch<'a> {
    #[serde(flatten)]
    usage: &'a ExtractionUsage,
    created: usize,
    updated: usize,
    rejected: usize,
    failed: usize,
}

fn attach_usage(db: &AsunaDb, session_id: i64, usage: &ExtractionUsage, stats: ExtractionStats) {
    let patch = UsagePatch {
        usage,
        created: stats.created,
        updated: stats.updated,
        rejected: stats.rejected,
        failed: stats.failed,
    };
    let Ok(encoded) = serde_json::to_string(&patch) else {
        eprintln!("[asuna] Cikarim maliyeti JSON'a cevrilemedi; maliyet yazilmadi.");
        return;
    };
    if let Err(error) =
        crate::db::session_repository::attach_usage(db, session_id, USAGE_KEY, &encoded)
    {
        // Hafiza kayitlari zaten yazildi; maliyet notu ikincil.
        eprintln!("[asuna] Cikarim maliyeti oturuma yazilamadi: {error}");
    }
}

// ---------------------------------------------------------------------------
// Servis
// ---------------------------------------------------------------------------

/// Aday uretim servisi. Tauri state'inde tek ornek olarak yasar; renderer'a
/// **acilmaz** (komutu yok).
pub struct ExtractionService {
    endpoint: String,
    http: OnceLock<reqwest::Client>,
}

impl ExtractionService {
    pub fn new() -> Self {
        Self::with_endpoint(CHAT_COMPLETIONS_URL)
    }

    /// Endpoint'i degistirilebilir kurucu — testler yerel bir HTTP sunucusuna
    /// yonlendirir; gercek API'ye **vurulmaz**.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            http: OnceLock::new(),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, ExtractionError> {
        if let Some(client) = self.http.get() {
            return Ok(client);
        }
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ExtractionError::HttpClientUnavailable)?;
        Ok(self.http.get_or_init(|| client))
    }

    /// Ozet (+ dokum) uzerinden aday uretir ve dogrular. DB'ye dokunmaz.
    pub async fn extract(
        &self,
        api_key: &SecretString,
        model: &str,
        summary_text: &str,
        lines: &[TranscriptLine],
    ) -> Result<ExtractionResult, ExtractionError> {
        if api_key.expose().trim().is_empty() {
            return Err(ExtractionError::MissingApiKey);
        }

        let input = render_input(summary_text, lines);
        let response = self
            .client()?
            .post(&self.endpoint)
            .bearer_auth(api_key.expose())
            .json(&ChatRequest {
                model,
                messages: [
                    ChatMessage {
                        role: "system",
                        content: MEMORY_EXTRACTION_PROMPT_V1,
                    },
                    ChatMessage {
                        role: "user",
                        content: &input,
                    },
                ],
            })
            .send()
            .await
            .map_err(|error| ExtractionError::from_transport(&error))?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(ExtractionError::from_status(status, model));
        }

        let parsed: ChatResponse = response
            .json()
            .await
            .map_err(|_| ExtractionError::MalformedResponse)?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .ok_or(ExtractionError::MalformedResponse)?;

        let items = parse_candidate_array(&content)?;

        let mut candidates = Vec::new();
        let mut rejected = Vec::new();
        for item in items.iter().take(MAX_CANDIDATES) {
            match validate_candidate(item) {
                Ok(candidate) => candidates.push(candidate),
                Err(reason) => rejected.push(reason),
            }
        }
        // Talimattaki siniri asan fazlalik da sessizce yutulmaz.
        for _ in MAX_CANDIDATES..items.len() {
            rejected.push(CandidateRejection::InvalidField);
        }

        let usage = parsed.usage;
        Ok(ExtractionResult {
            candidates,
            rejected,
            usage: ExtractionUsage {
                model: model.to_owned(),
                prompt_version: EXTRACTION_PROMPT_VERSION,
                prompt_tokens: usage.as_ref().and_then(|usage| usage.prompt_tokens),
                completion_tokens: usage.as_ref().and_then(|usage| usage.completion_tokens),
                total_tokens: usage.as_ref().and_then(|usage| usage.total_tokens),
                estimated_cost_usd: None,
            },
        })
    }
}

impl Default for ExtractionService {
    fn default() -> Self {
        Self::new()
    }
}

/// `Debug` elle yazildi: istemci nesnesinin varsayilan ciktisi gereksiz ic
/// detay basiyor.
impl fmt::Debug for ExtractionService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtractionService")
            .field("endpoint", &self.endpoint)
            .field("http", &self.http.get().map(|_| "<initialized>"))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Boru hatti
// ---------------------------------------------------------------------------

/// Cagri ayarlari — secret ve model, state kilidi tutulmadan kopyalanir.
#[derive(Debug, Clone)]
pub struct ExtractionSettings {
    pub api_key: SecretString,
    /// `ASUNA_SUMMARY_MODEL` yeniden kullanilir: cikarim da kisa, ucuz bir
    /// metin cagrisidir ve her adim icin ayri zorunlu bir env anahtari acmak
    /// mevcut kurulumlari kirardi.
    pub model: String,
    /// `ASUNA_MEMORY_ENABLED`. `false` iken model **hic cagrilmaz**.
    pub memory_enabled: bool,
}

/// Cikarimin girdisi.
#[derive(Debug, Clone, Copy)]
pub struct ExtractionInput<'a> {
    pub session_id: i64,
    pub summary_text: &'a str,
    pub lines: &'a [TranscriptLine],
}

/// Cikarim bilincli olarak atlanmali mi?
pub fn skip_reason(
    settings: &ExtractionSettings,
    summary_text: &str,
) -> Option<ExtractionSkipReason> {
    if !settings.memory_enabled {
        return Some(ExtractionSkipReason::MemoryDisabled);
    }
    if summary_text.trim().is_empty() {
        return Some(ExtractionSkipReason::NoSummary);
    }
    None
}

/// Adaylari uretir, dogrular, dedup'lar ve kaydeder.
///
/// **Hicbir kosulda `Err` donmez**: bu adim ozeti ve oturum kaydini etkilemez.
pub async fn extract_session_memories(
    service: &ExtractionService,
    db: &AsunaDb,
    settings: &ExtractionSettings,
    input: &ExtractionInput<'_>,
    now: &str,
) -> ExtractionOutcome {
    if let Some(reason) = skip_reason(settings, input.summary_text) {
        return ExtractionOutcome::Skipped(reason);
    }

    let result = match service
        .extract(
            &settings.api_key,
            &settings.model,
            input.summary_text,
            input.lines,
        )
        .await
    {
        Ok(result) => result,
        Err(error) => {
            // Sessiz yutma yok; ozet ve oturum kaydi dokunulmadan kalir.
            eprintln!(
                "[asuna] Hafiza cikarimi yapilamadi ({}): {}",
                error.kind(),
                redact_secrets(&error.to_string())
            );
            return ExtractionOutcome::Failed;
        }
    };

    if !result.rejected.is_empty() {
        let mut labels: Vec<&str> = result
            .rejected
            .iter()
            .map(|reason| reason.label())
            .collect();
        labels.sort_unstable();
        eprintln!(
            "[asuna] {} hafiza adayi dogrulamadan gecemedi ({}).",
            labels.len(),
            labels.join(", ")
        );
    }

    let mut stats = persist_candidates(db, input.session_id, &result.candidates, now);
    stats.rejected = result.rejected.len();
    attach_usage(db, input.session_id, &result.usage, stats);

    ExtractionOutcome::Completed(stats)
}

/// Ozet yazildiktan **sonra** ayni arka plan gorevinden cagrilir (`summary.rs`).
///
/// Buradaki hicbir sonuc ozeti geri almaz: en kotu durumda hafiza uretilmez ve
/// sebep log'lanir.
pub async fn extract_after_summary<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session_id: i64,
    summary_text: &str,
    lines: &[TranscriptLine],
) {
    let Some(settings) = app
        .try_state::<AsunaConfig>()
        .map(|config| ExtractionSettings {
            api_key: config.openai_api_key().clone(),
            model: config.summary_model.clone(),
            // Acilis degeri **ve** calisma zamani anahtari (ASU-037): kullanici
            // Ayarlar'dan hafizayi kapattiysa oturum kapanisinda aday uretilmez.
            // Kurulmamis process state "kisitsiz" doner, o zaman config karar verir.
            memory_enabled: config.memory_enabled && crate::privacy::process_memory_enabled(),
        })
    else {
        eprintln!("[asuna] Yapilandirma okunamadi; hafiza cikarimi yapilmayacak.");
        return;
    };

    // Hafiza kapaliyken **ag'a cikilmaz**: ne cagri, ne maliyet, ne DB.
    if let Some(reason) = skip_reason(&settings, summary_text) {
        eprintln!(
            "[asuna] Oturum {session_id} icin hafiza cikarimi yapilmadi ({}).",
            reason.label()
        );
        return;
    }

    let Some(service) = app.try_state::<Arc<ExtractionService>>() else {
        eprintln!("[asuna] Cikarim servisi kayitli degil; hafiza cikarimi yapilmayacak.");
        return;
    };
    let service = Arc::clone(service.inner());

    let Some(state) = app.try_state::<DbState>() else {
        return;
    };
    let Some(db) = state.database() else {
        eprintln!("[asuna] Hafiza deposu kullanilamiyor; hafiza cikarimi yapilmayacak.");
        return;
    };

    let outcome = extract_session_memories(
        &service,
        db,
        &settings,
        &ExtractionInput {
            session_id,
            summary_text,
            lines,
        },
        &clock::now_utc(),
    )
    .await;

    if let ExtractionOutcome::Completed(stats) = outcome {
        if stats.created > 0 || stats.updated > 0 {
            eprintln!(
                "[asuna] Oturum {session_id}: {} yeni hafiza, {} guncelleme.",
                stats.created, stats.updated
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::mpsc::{self, Receiver};
    use std::thread;

    use crate::db::session_repository::{self, SessionFinalizeInput};

    const TEST_API_KEY: &str = "sk-proj-COK-GIZLI-TEST-DEGERI";
    const TEST_MODEL: &str = "gpt-4o-mini";
    const START: &str = "2026-08-25T10:00:00Z";
    const END: &str = "2026-08-25T10:12:00Z";
    const NOW: &str = "2026-08-25T10:15:00Z";
    const LATER: &str = "2026-08-25T12:00:00Z";
    const SUMMARY: &str = "Konusulanlar: Wake word mimarisi.\nKararlar: Tespit yerelde kalacak.\n\
                           Yarim kalanlar: Esik secilmedi.";

    fn secret(value: &str) -> SecretString {
        SecretString::new(value.to_owned())
    }

    fn settings() -> ExtractionSettings {
        ExtractionSettings {
            api_key: secret(TEST_API_KEY),
            model: TEST_MODEL.to_owned(),
            memory_enabled: true,
        }
    }

    fn line(role: TranscriptRole, text: &str) -> TranscriptLine {
        TranscriptLine {
            role,
            text: text.to_owned(),
            at: None,
        }
    }

    fn conversation() -> Vec<TranscriptLine> {
        vec![
            line(TranscriptRole::User, "Wake word'u nasil kuralim?"),
            line(TranscriptRole::Assistant, "Tespiti yerelde tutalim."),
        ]
    }

    /// Kapanmis + ozetlenmis bir oturum iceren bellek ici DB.
    fn db_with_summarized_session() -> (AsunaDb, i64) {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let session =
            session_repository::start(&db, "gpt-realtime-2.1", None, START).expect("oturum");
        session_repository::finalize(&db, session.id, &SessionFinalizeInput::default(), None, END)
            .expect("kapanis");
        session_repository::attach_summary(
            &db,
            session.id,
            SUMMARY,
            Some(r#"{"model":"gpt-4o-mini","totalTokens":358}"#),
        )
        .expect("ozet");
        (db, session.id)
    }

    fn memories(db: &AsunaDb) -> Vec<MemoryRecord> {
        memory_repository::list(
            db,
            &MemoryFilter {
                archived: ArchiveFilter::All,
                include_expired: true,
                sort: MemorySort::Oldest,
                limit: Some(50),
                ..MemoryFilter::default()
            },
            NOW,
        )
        .expect("liste")
    }

    fn body_with(candidates: &str) -> String {
        let content = serde_json::to_string(candidates).expect("string");
        format!(
            r#"{{"choices":[{{"message":{{"role":"assistant","content":{content}}}}}],
                "usage":{{"prompt_tokens":420,"completion_tokens":80,"total_tokens":500}}}}"#
        )
    }

    async fn run(db: &AsunaDb, session_id: i64, server: &MockServer) -> ExtractionOutcome {
        run_with(db, session_id, server, &settings()).await
    }

    async fn run_with(
        db: &AsunaDb,
        session_id: i64,
        server: &MockServer,
        settings: &ExtractionSettings,
    ) -> ExtractionOutcome {
        let lines = conversation();
        extract_session_memories(
            &server.service(),
            db,
            settings,
            &ExtractionInput {
                session_id,
                summary_text: SUMMARY,
                lines: &lines,
            },
            NOW,
        )
        .await
    }

    // --- Minimal HTTP test sunucusu (summary.rs ile ayni desen) ------------

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
        fn start(status_line: &'static str, body: impl Into<String>) -> Self {
            let body: String = body.into();
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

        fn ok(body: impl Into<String>) -> Self {
            Self::start("200 OK", body)
        }

        fn service(&self) -> ExtractionService {
            ExtractionService::with_endpoint(self.url.clone())
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

    // --- Kabul kriteri: gecerli aday -> kalici kayit ----------------------

    #[tokio::test]
    async fn a_valid_candidate_becomes_a_memory_linked_to_its_session() {
        let server = MockServer::ok(body_with(
            r#"[{"kind":"decision","title":"Wake word yerel",
                 "content":"Wake word tespiti bulutta degil cihazda calisir.",
                 "importance":0.9,"confidence":1.0,"projectId":"asuna"}]"#,
        ));
        let (db, session_id) = db_with_summarized_session();

        let outcome = run(&db, session_id, &server).await;
        assert_eq!(
            outcome,
            ExtractionOutcome::Completed(ExtractionStats {
                created: 1,
                ..ExtractionStats::default()
            })
        );

        let stored = memories(&db);
        assert_eq!(stored.len(), 1);
        let record = &stored[0];
        assert_eq!(record.kind, MemoryKind::Decision);
        assert_eq!(
            record.content,
            "Wake word tespiti bulutta degil cihazda calisir."
        );
        assert_eq!(record.title, "Wake word yerel");
        assert_eq!(record.project_id.as_deref(), Some("asuna"));
        assert!((record.importance - 0.9).abs() < f64::EPSILON);
        // Kabul kriteri: kaynak izlenebilir.
        assert_eq!(record.source_session_id, Some(session_id));

        let metadata: Value = serde_json::from_str(&record.metadata_json).expect("JSON");
        assert_eq!(metadata[PENDING_APPROVAL_KEY], false, "hassas tur degil");
        assert_eq!(
            metadata["extraction"]["promptVersion"],
            EXTRACTION_PROMPT_VERSION
        );

        // Maliyet `usage_json.$.extraction` altina yamandi; ozet **ezilmedi**.
        let session = session_repository::get_by_id(&db, session_id)
            .expect("okuma")
            .expect("kayit");
        assert_eq!(session.summary.as_deref(), Some(SUMMARY));
        let usage: Value = serde_json::from_str(&session.usage_json.expect("usage")).expect("JSON");
        assert_eq!(
            usage["summary"]["totalTokens"], 358,
            "ozet maliyeti korunmali"
        );
        assert_eq!(usage["extraction"]["model"], TEST_MODEL);
        assert_eq!(usage["extraction"]["promptTokens"], 420);
        assert_eq!(usage["extraction"]["totalTokens"], 500);
        assert_eq!(
            usage["extraction"]["promptVersion"],
            EXTRACTION_PROMPT_VERSION
        );
        assert_eq!(usage["extraction"]["created"], 1);
        assert!(usage["extraction"]["estimatedCostUsd"].is_null());
    }

    /// **Kabul kriteri**: gecersiz kind / aralik disi skor / bos content
    /// reddedilir — ve tek bir red digerlerini dusurmez.
    #[tokio::test]
    async fn invalid_candidates_are_rejected_without_dropping_the_valid_one() {
        let server = MockServer::ok(body_with(
            r#"[{"kind":"project_decision","content":"Bilinmeyen tur.","importance":0.9,"confidence":1.0},
                {"kind":"decision","content":"Aralik disi skor.","importance":1.7,"confidence":1.0},
                {"kind":"decision","content":"   ","importance":0.9,"confidence":1.0},
                {"kind":"decision","content":"Skor yok.","confidence":1.0},
                {"kind":"decision","content":"Skor metin.","importance":"cok","confidence":1.0},
                "duz metin",
                {"kind":"idea","content":"Gecerli tek aday budur.","importance":0.8,"confidence":0.9}]"#,
        ));
        let (db, session_id) = db_with_summarized_session();

        let outcome = run(&db, session_id, &server).await;
        assert_eq!(
            outcome,
            ExtractionOutcome::Completed(ExtractionStats {
                created: 1,
                rejected: 6,
                ..ExtractionStats::default()
            })
        );

        let stored = memories(&db);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].content, "Gecerli tek aday budur.");
    }

    /// **Kabul kriteri** (PROJECT.md Bolum 14): gecici baglam durable memory'ye
    /// terfi etmez. Talimatta listelenmiyor; yine de gelirse kod reddeder.
    #[tokio::test]
    async fn working_context_and_tool_state_never_become_durable_memories() {
        let server = MockServer::ok(body_with(
            r#"[{"kind":"working_context","content":"Su an app.tsx acik.","importance":0.9,"confidence":1.0},
                {"kind":"tool_state","content":"Secili editor VS Code.","importance":0.95,"confidence":1.0}]"#,
        ));
        let (db, session_id) = db_with_summarized_session();

        let outcome = run(&db, session_id, &server).await;
        assert_eq!(
            outcome,
            ExtractionOutcome::Completed(ExtractionStats {
                rejected: 2,
                ..ExtractionStats::default()
            })
        );
        assert!(memories(&db).is_empty(), "gecici baglam kaydedilmemeli");

        // Ret sebebi "bilinmeyen tur" degil, acikca "terfi etmez".
        for kind in NON_DURABLE_KINDS {
            let raw = serde_json::json!({
                "kind": kind.as_str(), "content": "gecici", "importance": 1.0, "confidence": 1.0
            });
            assert_eq!(
                validate_candidate(&raw),
                Err(CandidateRejection::NonDurableKind)
            );
        }
    }

    /// **Kabul kriteri**: hassas kategorilerde kalici kayit oncesi acik onay.
    /// Aday atilmaz (kullanici goremeden karar verilmis olurdu) ama
    /// `pendingApproval` ile isaretlenir; retrieval onu gecirmez.
    #[tokio::test]
    async fn sensitive_kinds_are_stored_but_wait_for_explicit_approval() {
        let server = MockServer::ok(body_with(
            r#"[{"kind":"profile","content":"Kullanici Lefkosa'da yasiyor.","importance":0.8,"confidence":0.9},
                {"kind":"relationship","content":"Ekip lideri Mehmet ile calisiyor.","importance":0.7,"confidence":0.8},
                {"kind":"preference","content":"Kod yazarken kisa cevap ister.","importance":0.8,"confidence":0.9}]"#,
        ));
        let (db, session_id) = db_with_summarized_session();

        run(&db, session_id, &server).await;

        for record in memories(&db) {
            let metadata: Value = serde_json::from_str(&record.metadata_json).expect("JSON");
            let pending = metadata[PENDING_APPROVAL_KEY].as_bool().expect("bayrak");
            assert_eq!(
                pending,
                requires_approval(record.kind),
                "tur: {}",
                record.kind.as_str()
            );
        }
    }

    /// **Kabul kriteri**: benzer hafiza varsa yeni kayit acilmaz, mevcut
    /// guncellenir (onem **maksimum**, `updated_at` tazelenir).
    #[tokio::test]
    async fn a_similar_candidate_updates_the_existing_memory_instead_of_creating_one() {
        let (db, session_id) = db_with_summarized_session();
        let existing = memory_repository::create(
            &db,
            &MemoryDraft {
                kind: MemoryKind::Preference,
                title: "Kisa cevap".to_owned(),
                content: "Kod yazarken kisa cevap ister.".to_owned(),
                summary: None,
                project_id: None,
                importance: 0.6,
                confidence: 0.9,
                source_session_id: None,
                expires_at: None,
                metadata_json: None,
            },
            NOW,
        )
        .expect("mevcut kayit");

        // Ayni bilgi, farkli noktalama/buyuk harf + ek cumle.
        let server = MockServer::ok(body_with(
            r#"[{"kind":"preference",
                 "content":"Kod yazarken KISA cevap ister!","importance":0.9,"confidence":1.0}]"#,
        ));
        let outcome = extract_session_memories(
            &server.service(),
            &db,
            &settings(),
            &ExtractionInput {
                session_id,
                summary_text: SUMMARY,
                lines: &[],
            },
            LATER,
        )
        .await;

        assert_eq!(
            outcome,
            ExtractionOutcome::Completed(ExtractionStats {
                updated: 1,
                ..ExtractionStats::default()
            })
        );

        let stored = memories(&db);
        assert_eq!(stored.len(), 1, "yeni satir acilmamali");
        let record = &stored[0];
        assert_eq!(record.id, existing.id);
        assert!(
            (record.importance - 0.9).abs() < f64::EPSILON,
            "onem maksimum alinmali"
        );
        assert_eq!(record.updated_at, LATER, "updated_at tazelenmeli");
        assert_eq!(record.created_at, NOW, "created_at korunmali");
        assert_eq!(
            record.content, "Kod yazarken kisa cevap ister.",
            "kullanicinin metni ezilmemeli"
        );
    }

    /// **Gate 3 / MEDIUM-4 regresyonu**: kisa bir kayit, onu iceren **farkli**
    /// bir adayi yutmamali.
    ///
    /// Eski esik (12 karakter) ile "kahve sevmiyor" kaydi "esi kahve sevmiyor"
    /// adayini "ayni hafiza" sayiyordu: iki farkli kisi hakkindaki iki bilgi
    /// tek satira iniyor, yeni icerik **hic yazilmadigi** icin de geri
    /// getirilemiyordu.
    #[tokio::test]
    async fn a_candidate_that_merely_contains_an_existing_memory_is_not_a_duplicate() {
        let (db, session_id) = db_with_summarized_session();
        memory_repository::create(
            &db,
            &MemoryDraft {
                kind: MemoryKind::Preference,
                title: "Kahve".to_owned(),
                content: "kahve sevmiyor".to_owned(),
                summary: None,
                project_id: None,
                importance: 0.7,
                confidence: 1.0,
                source_session_id: None,
                expires_at: None,
                metadata_json: None,
            },
            NOW,
        )
        .expect("mevcut kayit");

        let server = MockServer::ok(body_with(
            r#"[{"kind":"preference","content":"esi kahve sevmiyor",
                 "importance":0.8,"confidence":1.0}]"#,
        ));
        let outcome = run(&db, session_id, &server).await;

        assert_eq!(
            outcome,
            ExtractionOutcome::Completed(ExtractionStats {
                created: 1,
                ..ExtractionStats::default()
            }),
            "farkli bir bilgi yeni kayit olmali"
        );

        let stored = memories(&db);
        assert_eq!(stored.len(), 2, "iki ayri hafiza saklanmali");
        let contents: Vec<&str> = stored
            .iter()
            .map(|record| record.content.as_str())
            .collect();
        assert!(contents.contains(&"kahve sevmiyor"), "{contents:?}");
        assert!(contents.contains(&"esi kahve sevmiyor"), "{contents:?}");
    }

    /// **Gate 3 / HIGH-2**: aday icerigine gomulmus bir API anahtari kalici
    /// kayda **maskeli** girer (`asuna-config/security.md` Bolum 5).
    #[tokio::test]
    async fn a_secret_inside_a_candidate_is_masked_before_it_is_stored() {
        let (db, session_id) = db_with_summarized_session();

        let server = MockServer::ok(body_with(
            r#"[{"kind":"decision",
                 "title":"Anahtar sk-proj-BASLIKTA-SIZAN",
                 "content":"Kullanici anahtarini okudu: sk-proj-COK-GIZLI-DEGER, parola: hunter2.",
                 "importance":0.9,"confidence":1.0}]"#,
        ));
        run(&db, session_id, &server).await;

        let stored = memories(&db);
        assert_eq!(stored.len(), 1);
        let record = &stored[0];

        assert!(
            !record.content.contains("COK-GIZLI-DEGER") && !record.content.contains("hunter2"),
            "secret kalici kayda girdi: {}",
            record.content
        );
        assert!(
            record.content.contains("sk-<redacted>"),
            "{}",
            record.content
        );
        assert!(
            record.content.contains("parola: <redacted>"),
            "{}",
            record.content
        );
        // Metnin geri kalani korunur: suzgec hafizayi bozmaz, yalnizca maskeler.
        assert!(
            record.content.starts_with("Kullanici anahtarini okudu:"),
            "{}",
            record.content
        );
        assert!(
            !record.title.contains("BASLIKTA-SIZAN"),
            "baslik maskelenmedi: {}",
            record.title
        );
    }

    /// Daha dusuk onemli bir tekrar mevcut degeri **dusurmez**.
    #[tokio::test]
    async fn deduplication_never_lowers_the_stored_importance() {
        let (db, session_id) = db_with_summarized_session();
        memory_repository::create(
            &db,
            &MemoryDraft {
                kind: MemoryKind::Decision,
                title: "Wake word yerel".to_owned(),
                content: "Wake word tespiti cihazda calisir.".to_owned(),
                summary: None,
                project_id: None,
                importance: 0.95,
                confidence: 1.0,
                source_session_id: None,
                expires_at: None,
                metadata_json: None,
            },
            NOW,
        )
        .expect("kayit");

        let server = MockServer::ok(body_with(
            r#"[{"kind":"decision","content":"Wake word tespiti cihazda calisir.",
                 "importance":0.6,"confidence":0.7}]"#,
        ));
        run(&db, session_id, &server).await;

        let stored = memories(&db);
        assert_eq!(stored.len(), 1);
        assert!((stored[0].importance - 0.95).abs() < f64::EPSILON);
    }

    /// Farkli projelerin ayni cumlesi ayni hafiza **degildir**.
    #[tokio::test]
    async fn deduplication_is_scoped_to_the_project() {
        let (db, session_id) = db_with_summarized_session();
        memory_repository::create(
            &db,
            &MemoryDraft {
                kind: MemoryKind::Decision,
                title: "Testler once".to_owned(),
                content: "Testler CI'da her push'ta calisir.".to_owned(),
                summary: None,
                project_id: Some("baska-proje".to_owned()),
                importance: 0.8,
                confidence: 1.0,
                source_session_id: None,
                expires_at: None,
                metadata_json: None,
            },
            NOW,
        )
        .expect("kayit");

        let server = MockServer::ok(body_with(
            r#"[{"kind":"decision","content":"Testler CI'da her push'ta calisir.",
                 "importance":0.8,"confidence":1.0,"projectId":"asuna"}]"#,
        ));
        run(&db, session_id, &server).await;

        assert_eq!(memories(&db).len(), 2, "farkli proje ayri hafiza");
    }

    /// **Kabul kriteri**: onem esigi altindaki aday kaydedilmez.
    #[tokio::test]
    async fn candidates_below_the_importance_threshold_are_not_stored() {
        let server = MockServer::ok(body_with(
            r#"[{"kind":"idea","content":"Onemsiz bir yan not.","importance":0.4,"confidence":0.9},
                {"kind":"idea","content":"Tam esikte olan not.","importance":0.5,"confidence":0.9}]"#,
        ));
        let (db, session_id) = db_with_summarized_session();

        let outcome = run(&db, session_id, &server).await;
        assert_eq!(
            outcome,
            ExtractionOutcome::Completed(ExtractionStats {
                created: 1,
                rejected: 1,
                ..ExtractionStats::default()
            })
        );
        assert_eq!(memories(&db).len(), 1, "esik dahil, alti degil");
        assert_eq!(memories(&db)[0].content, "Tam esikte olan not.");
    }

    /// **Kabul kriteri**: cikarim basarisiz olsa da ozet ve oturum kaydi
    /// bozulmaz.
    #[tokio::test]
    async fn an_extraction_failure_leaves_the_summary_and_session_intact() {
        for status in [
            "401 Unauthorized",
            "429 Too Many Requests",
            "500 Server Error",
        ] {
            let server = MockServer::start(status, r#"{"error":{"message":"sk-proj-SIZAN"}}"#);
            let (db, session_id) = db_with_summarized_session();

            assert_eq!(
                run(&db, session_id, &server).await,
                ExtractionOutcome::Failed,
                "durum: {status}"
            );

            let session = session_repository::get_by_id(&db, session_id)
                .expect("okuma")
                .expect("kayit");
            assert_eq!(
                session.summary.as_deref(),
                Some(SUMMARY),
                "ozet geri alindi"
            );
            assert_eq!(session.ended_at.as_deref(), Some(END));
            assert!(memories(&db).is_empty());
        }
    }

    #[tokio::test]
    async fn a_network_failure_is_typed_and_does_not_touch_the_session() {
        let (db, session_id) = db_with_summarized_session();
        let service = ExtractionService::with_endpoint(closed_endpoint());

        let outcome = extract_session_memories(
            &service,
            &db,
            &settings(),
            &ExtractionInput {
                session_id,
                summary_text: SUMMARY,
                lines: &[],
            },
            NOW,
        )
        .await;

        assert_eq!(outcome, ExtractionOutcome::Failed);
        assert_eq!(
            session_repository::get_by_id(&db, session_id)
                .expect("okuma")
                .expect("kayit")
                .summary
                .as_deref(),
            Some(SUMMARY)
        );
    }

    /// **Kabul kriteri**: `ASUNA_MEMORY_ENABLED=false` iken cikarim hic
    /// calismaz — ag'a cikilmaz, DB'ye dokunulmaz.
    #[tokio::test]
    async fn extraction_does_not_run_when_memory_is_disabled() {
        let server = MockServer::ok(body_with(
            r#"[{"kind":"decision","content":"Kaydedilmemeli.","importance":0.9,"confidence":1.0}]"#,
        ));
        let (db, session_id) = db_with_summarized_session();

        let outcome = run_with(
            &db,
            session_id,
            &server,
            &ExtractionSettings {
                memory_enabled: false,
                ..settings()
            },
        )
        .await;

        assert_eq!(
            outcome,
            ExtractionOutcome::Skipped(ExtractionSkipReason::MemoryDisabled)
        );
        server.assert_no_request();
        assert!(memories(&db).is_empty());
    }

    #[tokio::test]
    async fn an_empty_summary_skips_the_call() {
        let server = MockServer::ok(body_with("[]"));
        let (db, session_id) = db_with_summarized_session();

        let outcome = extract_session_memories(
            &server.service(),
            &db,
            &settings(),
            &ExtractionInput {
                session_id,
                summary_text: "   ",
                lines: &[],
            },
            NOW,
        )
        .await;

        assert_eq!(
            outcome,
            ExtractionOutcome::Skipped(ExtractionSkipReason::NoSummary)
        );
        server.assert_no_request();
    }

    /// Bos dizi bir hata degil: "hatirlanmaya deger bir sey yok" gecerli bir
    /// cevaptir ve uydurma hafiza uretmemenin dogru sonucudur.
    #[tokio::test]
    async fn an_empty_candidate_list_is_a_valid_answer() {
        let server = MockServer::ok(body_with("[]"));
        let (db, session_id) = db_with_summarized_session();

        assert_eq!(
            run(&db, session_id, &server).await,
            ExtractionOutcome::Completed(ExtractionStats::default())
        );
        assert!(memories(&db).is_empty());
    }

    // --- Istek sekli ------------------------------------------------------

    #[tokio::test]
    async fn sends_a_minimal_request_with_the_versioned_prompt_and_configured_model() {
        let server = MockServer::ok(body_with("[]"));
        let lines = conversation();
        server
            .service()
            .extract(&secret(TEST_API_KEY), TEST_MODEL, SUMMARY, &lines)
            .await
            .expect("cikarim");

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

        let body: Value = serde_json::from_str(&request.body).expect("govde JSON");
        assert_eq!(body["model"], TEST_MODEL, "model config'ten gelmeli");

        let object = body.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["messages", "model"], "govde minimum kalmali");

        assert_eq!(body["messages"][0]["content"], MEMORY_EXTRACTION_PROMPT_V1);
        let user = body["messages"][1]["content"].as_str().expect("girdi");
        assert!(user.contains("Oturum ozeti:"), "{user}");
        assert!(user.contains("Kararlar: Tespit yerelde kalacak."), "{user}");
        assert!(
            user.contains("Kullanici: Wake word'u nasil kuralim?"),
            "{user}"
        );
    }

    #[tokio::test]
    async fn missing_api_key_is_reported_before_any_request() {
        let service = ExtractionService::with_endpoint(closed_endpoint());
        for blank in ["", "   "] {
            assert_eq!(
                service
                    .extract(&secret(blank), TEST_MODEL, SUMMARY, &[])
                    .await
                    .expect_err("bos key hata uretmeli"),
                ExtractionError::MissingApiKey
            );
        }
    }

    #[tokio::test]
    async fn maps_http_status_codes_to_distinct_variants() {
        let cases: [(&'static str, ExtractionError); 5] = [
            ("401 Unauthorized", ExtractionError::InvalidApiKey),
            (
                "404 Not Found",
                ExtractionError::ModelAccessDenied {
                    model: TEST_MODEL.to_owned(),
                },
            ),
            ("429 Too Many Requests", ExtractionError::QuotaExceeded),
            (
                "503 Service Unavailable",
                ExtractionError::UpstreamUnavailable { status: 503 },
            ),
            (
                "418 I'm a teapot",
                ExtractionError::UnexpectedStatus { status: 418 },
            ),
        ];

        for (status_line, expected) in cases {
            let server = MockServer::start(status_line, r#"{"error":{"message":"sk-proj-SIZAN"}}"#);
            let error = server
                .service()
                .extract(&secret(TEST_API_KEY), TEST_MODEL, SUMMARY, &[])
                .await
                .expect_err("hata bekleniyordu");
            assert_eq!(error, expected, "durum: {status_line}");
        }
    }

    #[tokio::test]
    async fn rejects_responses_that_are_not_candidate_arrays() {
        let cases = [
            (r#"{"choices":[]}"#, ExtractionError::MalformedResponse),
            (
                r#"{"choices":[{"message":{"role":"assistant"}}]}"#,
                ExtractionError::MalformedResponse,
            ),
            ("<html>gateway</html>", ExtractionError::MalformedResponse),
        ];
        for (body, expected) in cases {
            let server = MockServer::start("200 OK", body);
            assert_eq!(
                server
                    .service()
                    .extract(&secret(TEST_API_KEY), TEST_MODEL, SUMMARY, &[])
                    .await
                    .expect_err("hata bekleniyordu"),
                expected,
                "govde: {body}"
            );
        }

        // Modelin **metni** dizi degilse de icerik uydurulmaz.
        for content in ["Hatirlanacak bir sey yok.", "{\"kind\":\"idea\"}", "   "] {
            let server = MockServer::ok(body_with(content));
            assert!(server
                .service()
                .extract(&secret(TEST_API_KEY), TEST_MODEL, SUMMARY, &[])
                .await
                .is_err());
        }
    }

    /// Model kod blogu ya da `{"memories": [...]}` sarmalayicisi uretirse
    /// icerik kaybedilmez.
    #[tokio::test]
    async fn tolerates_fenced_and_wrapped_json() {
        let candidate =
            r#"{"kind":"idea","content":"Sarmalanmis aday.","importance":0.8,"confidence":0.9}"#;
        for content in [
            format!("```json\n[{candidate}]\n```"),
            format!("{{\"memories\": [{candidate}]}}"),
        ] {
            let server = MockServer::ok(body_with(&content));
            let result = server
                .service()
                .extract(&secret(TEST_API_KEY), TEST_MODEL, SUMMARY, &[])
                .await
                .expect("aday okunmali");
            assert_eq!(result.candidates.len(), 1, "icerik: {content}");
        }
    }

    #[tokio::test]
    async fn only_the_first_candidates_are_accepted() {
        let many: Vec<String> = (0..12)
            .map(|index| {
                format!(
                    r#"{{"kind":"idea","content":"Aday numarasi {index} icin uzun bir icerik.",
                        "importance":0.8,"confidence":0.9}}"#
                )
            })
            .collect();
        let server = MockServer::ok(body_with(&format!("[{}]", many.join(","))));
        let (db, session_id) = db_with_summarized_session();

        run(&db, session_id, &server).await;
        assert_eq!(memories(&db).len(), MAX_CANDIDATES);
    }

    // --- Dogrulama birimleri ----------------------------------------------

    #[test]
    fn validation_rules_are_explicit() {
        let base = serde_json::json!({
            "kind": "idea", "content": "Yeterince uzun bir hafiza icerigi.",
            "importance": 0.8, "confidence": 0.9
        });
        assert!(validate_candidate(&base).is_ok());

        let cases: [(Value, CandidateRejection); 7] = [
            (
                Value::String("metin".to_owned()),
                CandidateRejection::NotAnObject,
            ),
            (
                serde_json::json!({"content":"x","importance":0.9,"confidence":0.9}),
                CandidateRejection::UnknownKind,
            ),
            (
                serde_json::json!({"kind":"idea","content":"  ","importance":0.9,"confidence":0.9}),
                CandidateRejection::EmptyContent,
            ),
            (
                serde_json::json!({"kind":"idea","content":"x","importance":-0.1,"confidence":0.9}),
                CandidateRejection::InvalidScore,
            ),
            (
                serde_json::json!({"kind":"idea","content":"x","importance":0.9}),
                CandidateRejection::InvalidScore,
            ),
            (
                serde_json::json!({"kind":"idea","content":"x","importance":0.2,"confidence":0.9}),
                CandidateRejection::BelowThreshold,
            ),
            (
                serde_json::json!({"kind":"idea","content":"x","importance":0.9,"confidence":0.9,
                                   "projectId":42}),
                CandidateRejection::InvalidField,
            ),
        ];
        for (raw, expected) in cases {
            assert_eq!(validate_candidate(&raw), Err(expected), "girdi: {raw}");
        }

        // Cok uzun icerik kirpilmaz, reddedilir.
        let long = serde_json::json!({
            "kind": "idea", "content": "ç".repeat(MAX_CANDIDATE_CONTENT_CHARS + 1),
            "importance": 0.9, "confidence": 0.9
        });
        assert_eq!(
            validate_candidate(&long),
            Err(CandidateRejection::ContentTooLong)
        );
    }

    #[test]
    fn a_missing_title_is_derived_from_the_content() {
        let raw = serde_json::json!({
            "kind": "decision", "content": "Wake word yerelde kalir. Bulut kullanilmaz.",
            "importance": 0.9, "confidence": 1.0
        });
        let candidate = validate_candidate(&raw).expect("aday");
        assert_eq!(candidate.title, "Wake word yerelde kalir");
        assert!(!candidate.title.is_empty());
    }

    #[test]
    fn every_rejection_reason_has_a_distinct_label() {
        let reasons = [
            CandidateRejection::NotAnObject,
            CandidateRejection::UnknownKind,
            CandidateRejection::NonDurableKind,
            CandidateRejection::EmptyContent,
            CandidateRejection::ContentTooLong,
            CandidateRejection::InvalidScore,
            CandidateRejection::BelowThreshold,
            CandidateRejection::InvalidField,
        ];
        let mut labels: Vec<&str> = reasons.iter().map(|reason| reason.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), reasons.len());
    }

    // --- Dedup birimleri ---------------------------------------------------

    #[test]
    fn normalization_ignores_case_punctuation_and_spacing() {
        assert_eq!(
            normalize_for_dedup("  Kod yazarken   KISA cevap ister!  "),
            normalize_for_dedup("kod yazarken kisa cevap ister.")
        );
        assert_eq!(normalize_for_dedup("A, B; C."), "a b c");
    }

    #[test]
    fn duplicate_detection_requires_a_meaningful_overlap() {
        let long = normalize_for_dedup("Kod yazarken kisa cevap ister.");

        // Yalnizca noktalama/buyuk harf farki: ayni hafiza.
        assert!(is_duplicate(
            &long,
            &normalize_for_dedup("KOD YAZARKEN KISA CEVAP ISTER")
        ));

        // Yeterince uzun **ve** boyca yakin alt dize: ayni hafiza.
        let decision = normalize_for_dedup("Wake word tespiti tamamen cihazda kalacak");
        assert!(decision.chars().count() >= MIN_SUBSET_CHARS);
        assert!(is_duplicate(
            &decision,
            &normalize_for_dedup("Wake word tespiti tamamen cihazda kalacak. Evet.")
        ));

        // Kisa ortak parca "ayni hafiza" demek degil.
        assert!(!is_duplicate(
            &normalize_for_dedup("yok"),
            &normalize_for_dedup("Toplantida karar yok denildi.")
        ));
        assert!(!is_duplicate(
            &long,
            &normalize_for_dedup("Testler her push'ta calisir.")
        ));
        assert!(!is_duplicate("", "bos"));

        // **Gate 3 / MEDIUM-4**: esik altindaki alt dize artik yutmuyor. Iki
        // cumle de saklanir; hangisinin kalacagina kullanici karar verir.
        assert!(!is_duplicate(
            &long,
            &normalize_for_dedup("Kullanici kod yazarken kisa cevap ister, uzun anlatim istemez.")
        ));

        // Esigi gecen ama boyca uzak alt dize de yutmuyor: uzun metnin icinde
        // gecmek "ayni hafiza" demek degil.
        assert!(!is_duplicate(
            &decision,
            &normalize_for_dedup(
                "Wake word tespiti tamamen cihazda kalacak; ayrica idle mikrofon verisi \
                 diske yazilmayacak ve oturum acilmadan buluta ses gitmeyecek."
            )
        ));
    }

    // --- Prompt sozlesmesi -------------------------------------------------

    /// Talimat "uydurma" yasagini ve gecici baglam sinirini **acikca** soyler;
    /// bunlar urun kurallari (PROJECT.md Bolum 14 + 26) ve prompt'un icinde
    /// durmali.
    #[test]
    fn the_prompt_forbids_invention_and_transient_context() {
        assert!(MEMORY_EXTRACTION_PROMPT_V1.contains("uydurma"));
        assert!(MEMORY_EXTRACTION_PROMPT_V1.contains("terminal hatasi"));
        assert!(MEMORY_EXTRACTION_PROMPT_V1.contains("[]"));
        for kind in NON_DURABLE_KINDS {
            assert!(
                !MEMORY_EXTRACTION_PROMPT_V1.contains(kind.as_str()),
                "gecici tur talimatta listelenmemeli: {}",
                kind.as_str()
            );
        }
        for kind in MemoryKind::ALL {
            if is_durable(kind) {
                assert!(
                    MEMORY_EXTRACTION_PROMPT_V1.contains(kind.as_str()),
                    "kalici tur talimatta olmali: {}",
                    kind.as_str()
                );
            }
        }
    }

    #[test]
    fn the_threshold_and_sensitive_sets_are_documented_constants() {
        assert!((0.0..=1.0).contains(&MIN_IMPORTANCE));
        assert_eq!(
            SENSITIVE_KINDS,
            [MemoryKind::Profile, MemoryKind::Relationship]
        );
        assert!(SENSITIVE_KINDS.iter().copied().all(requires_approval));
        assert!(!requires_approval(MemoryKind::Decision));
    }

    // --- Redaksiyon / gizlilik --------------------------------------------

    #[test]
    fn no_error_variant_leaks_secret_material() {
        let variants = [
            ExtractionError::MissingApiKey,
            ExtractionError::InvalidApiKey,
            ExtractionError::ModelAccessDenied {
                model: TEST_MODEL.to_owned(),
            },
            ExtractionError::QuotaExceeded,
            ExtractionError::Network {
                cause: NetworkCause::Timeout,
            },
            ExtractionError::UpstreamUnavailable { status: 503 },
            ExtractionError::UnexpectedStatus { status: 418 },
            ExtractionError::MalformedResponse,
            ExtractionError::NotAJsonArray,
            ExtractionError::HttpClientUnavailable,
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

    #[test]
    fn service_debug_output_is_safe() {
        let debug = format!("{:?}", ExtractionService::new());
        assert!(debug.contains(CHAT_COMPLETIONS_URL));
        assert!(!debug.contains("sk-"), "debug: {debug}");
    }
}

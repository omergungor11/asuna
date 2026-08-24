//! Oturum ozeti boru hatti (ASU-033).
//!
//! # Neden ayri bir cagri
//!
//! Ozet, realtime modelinden **istenmez**. PROJECT.md Bolum 26'nin kurali:
//! konusan modele "bunu kaydet / sunu ozetle" dedirtmek, hafizanin icerigini
//! denetlenemez bir yan etkiye cevirir. Boru hatti bilerek ayri, sonradan ve
//! denetlenebilir: `konusma -> transcript -> (ayri model cagrisi) -> ozet`.
//! ASU-034 bu ozetin ustune aday hafizalari kuracak.
//!
//! # Guvenlik sozlesmesi (realtime_token.rs ile ayni disiplin)
//!
//! - Kalici `OPENAI_API_KEY` yalnizca bu process'te; `Authorization` header'i
//!   disinda hicbir yere yazilmaz. Renderer bu modulun hicbir seyini cagirmaz —
//!   `#[tauri::command]` **yok**, IPC yuzeyi **yok**.
//! - Yonlendirme kapali (`redirect::none`): `Authorization` tanimadigimiz bir
//!   host'a tasinamaz.
//! - Yanittan yalnizca ihtiyacimiz olan alanlar okunur; okunmayan veri sizamaz.
//! - Hicbir hata varyanti API govdesi, header ya da transcript tasimaz; IPC'ye
//!   zaten gitmez ama log'a giden mesaj yine de [`redact_secrets`] suzgecinden
//!   gecer.
//!
//! # Oturumu asla bloklamaz
//!
//! Kapanis (`session_finalize`) once DB'ye yazar ve doner; ozet **sonra**,
//! arka planda uretilip ayri bir `UPDATE` ile eklenir. Uygulama o sirada
//! kapanirsa kaybedilen sey yalnizca ozettir: oturum kaydi kapali, `end_reason`
//! dogru, `summary` NULL. Kuyruk/retry tablosu bilerek yok — en basit guvenli
//! tasarim bu ve yarim yazilmis bir "ozet bekliyor" durumu uretmiyor.

use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use thiserror::Error;

use crate::config::{AsunaConfig, SecretString};
use crate::db::session_repository;
use crate::db::transcript::{TranscriptLine, TranscriptRole};
use crate::db::{AsunaDb, DbState};
use crate::realtime_token::{redact_secrets, NetworkCause};

/// OpenAI Chat Completions endpoint'i.
pub const CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Ozet cagrisinin ust siniri. Kullanici burada beklemiyor (cagri arka planda),
/// ama sonsuza kadar da asili kalmamali.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// TCP + TLS el sikismasi icin ayri, daha kisa sinir.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Bu sayidan **az** replikte ozet uretilmez.
///
/// "Hey Asuna" deyip vazgecilen bir oturum icin model cagirmak hem para hem
/// gurultu: uretilen metin bos bir ozet olur ve ASU-034'e degersiz aday tasir.
pub const MIN_TRANSCRIPT_LINES: usize = 2;

/// Modele gonderilen dokumun karakter siniri.
///
/// Uzun bir oturum baglam penceresini ve maliyeti sisirir. Sinir asilirsa
/// **son** replikler tutulur (yeni olan daha degerli) ve kirpildigi modele
/// acikca soylenir — model "konusmanin basi buydu" sanmasin.
const MAX_PROMPT_CHARS: usize = 24_000;

/// Saklanan ozetin karakter siniri. Model talimata uymayip roman yazarsa
/// `sessions.summary` sisirilmez.
const MAX_SUMMARY_CHARS: usize = 1_500;

/// Kirpildiginda dokumun basina eklenen isaret.
const TRUNCATION_MARKER: &str = "[Konusmanin basi kirpildi.]";

/// Ozet talimati — **versiyonlu**. Degistirmek yeni bir sabit acmak demektir;
/// eski oturumlarin hangi talimatla ozetlendigi izlenebilir kalsin
/// (`prompts/*.v1.ts` ile ayni disiplin, ama bu cagri Rust tarafinda yapiliyor
/// cunku kalici API key burada).
pub const SESSION_SUMMARY_PROMPT_V1: &str = "\
Sen bir konusma ozetleyicisisin. Sana bir sesli oturumun dokumu verilecek.
Turkce, kisa ve yapili bir ozet yaz. Tam olarak su uc basligi kullan:

Konusulanlar: <en fazla iki cumle>
Kararlar: <alinan kararlar; yoksa `yok`>
Yarim kalanlar: <tamamlanmamis isler/sorular; yoksa `yok`>

Kurallar:
- Yalnizca dokumda gecenleri yaz. Cikarim yapma, tamamlama, tahmin etme.
- Dokumda olmayan hicbir isim, tarih, sayi ya da karar uydurma.
- Markdown, madde isareti, emoji ya da giris cumlesi kullanma.
- Toplam 120 kelimeyi gecme.";

/// Ozet talimatinin surumu — `usage_json` icine yazilir.
pub const SUMMARY_PROMPT_VERSION: &str = "core-summary.v1";

// ---------------------------------------------------------------------------
// Sonuc tipleri
// ---------------------------------------------------------------------------

/// Ozetin token maliyeti.
///
/// **USD yok.** Ozet modelinin fiyati dogrulanmadi (`docs/architecture/voice.md`
/// Bolum 6 yalnizca Realtime fiyatlarini iceriyor); dogrulanmamis bir fiyatla
/// carpmak "uydurulmus maliyet" olurdu. Bu yuzden maliyet **token cinsinden**
/// kaydediliyor: olculen sey gercek, cevrimi yapilmiyor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryUsage {
    /// Ozeti ureten model (config'ten; hard-code degil).
    pub model: String,
    pub prompt_version: &'static str,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    /// Her zaman `null`: fiyat dogrulanmadi (bkz. tip dokumantasyonu).
    pub estimated_cost_usd: Option<f64>,
}

/// Uretilmis ozet.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub text: String,
    pub usage: SummaryUsage,
}

/// Boru hattinin sonucu. Cagiran taraf icin (ve testler icin) **acik**:
/// "ozet yok" ile "ozet uretilmedi cunku oturum cok kisaydi" ayni sey degil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryOutcome {
    /// Ozet uretildi ve `sessions.summary` guncellendi.
    Stored,
    /// Bilincli olarak uretilmedi.
    Skipped(SkipReason),
    /// Uretilemedi; hata log'landi, oturum kaydi olduğu gibi kaldi.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// [`MIN_TRANSCRIPT_LINES`] altinda replik.
    TooShort,
    /// Dokum bos ya da yalnizca bosluk — ozetlenecek metin yok.
    NoContent,
}

// ---------------------------------------------------------------------------
// Hata tipi
// ---------------------------------------------------------------------------

/// Ozet uretiminin ayirt edilmis hata durumlari.
///
/// IPC'ye gitmez (renderer bu modulu cagirmaz) ama yerel log'a duser; hicbir
/// varyant secret, transcript ya da API govdesi tasimaz.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SummaryError {
    #[error("OpenAI API anahtari tanimli degil; ozet uretilemedi.")]
    MissingApiKey,

    #[error("OpenAI API anahtari gecersiz (yetkilendirme reddedildi); ozet uretilemedi.")]
    InvalidApiKey,

    #[error(
        "Bu hesabin `{model}` modeline erisimi yok. `ASUNA_SUMMARY_MODEL` degerini \
         erisiminiz olan bir modele ayarlayin."
    )]
    ModelAccessDenied { model: String },

    #[error("OpenAI kota sinirina takildi; ozet uretilemedi.")]
    QuotaExceeded,

    #[error("OpenAI'ya ulasilamadi ({}); ozet uretilemedi.", cause.as_turkish())]
    Network { cause: NetworkCause },

    #[error("OpenAI ozet servisi yanit vermiyor (HTTP {status}); ozet uretilemedi.")]
    UpstreamUnavailable { status: u16 },

    #[error("OpenAI beklenmeyen bir yanit dondu (HTTP {status}); ozet uretilemedi.")]
    UnexpectedStatus { status: u16 },

    #[error("OpenAI'nin ozet yaniti okunamadi (beklenen alanlar eksik veya bos).")]
    MalformedResponse,

    #[error("Guvenli HTTPS istemcisi kurulamadi; ozet uretilemedi.")]
    HttpClientUnavailable,
}

impl SummaryError {
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

/// Govde bilerek **minimum**: yalnizca `model` + `messages`. `temperature`,
/// `max_completion_tokens` gibi alanlarin bu hesapta/modelde davranisi
/// dogrulanmadi; kisa cikti talimatla isteniyor, gelen metin ayrica
/// [`MAX_SUMMARY_CHARS`] ile kirpiliyor.
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
// Dokum -> prompt
// ---------------------------------------------------------------------------

fn speaker(role: TranscriptRole) -> &'static str {
    match role {
        TranscriptRole::User => "Kullanici",
        TranscriptRole::Assistant => "Asuna",
    }
}

/// Dokumu modele verilecek tek metne cevirir.
///
/// Zaman damgasi **gonderilmez**: ozet icin degeri yok, gizlilik acisindan
/// gereksiz ayrinti.
fn render_transcript(lines: &[TranscriptLine]) -> String {
    let mut rendered: Vec<String> = Vec::with_capacity(lines.len());
    let mut budget = MAX_PROMPT_CHARS;
    let mut truncated = false;

    for line in lines.iter().rev() {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        let entry = format!("{}: {text}", speaker(line.role));
        if entry.chars().count() + 1 > budget {
            truncated = true;
            break;
        }
        budget -= entry.chars().count() + 1;
        rendered.push(entry);
    }

    rendered.reverse();
    if truncated {
        rendered.insert(0, TRUNCATION_MARKER.to_owned());
    }
    rendered.join("\n")
}

/// Modelin dondurdugu metni saklanabilir hale getirir.
fn clean_summary(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= MAX_SUMMARY_CHARS {
        return Some(trimmed.to_owned());
    }
    // Karakter siniri: `String` byte ile kesilirse UTF-8 bozulur.
    let clipped: String = trimmed.chars().take(MAX_SUMMARY_CHARS).collect();
    Some(format!("{}…", clipped.trim_end()))
}

// ---------------------------------------------------------------------------
// Servis
// ---------------------------------------------------------------------------

/// Ozet uretim servisi. Tauri state'inde tek ornek olarak yasar.
pub struct SummaryService {
    endpoint: String,
    http: OnceLock<reqwest::Client>,
}

impl SummaryService {
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

    fn client(&self) -> Result<&reqwest::Client, SummaryError> {
        if let Some(client) = self.http.get() {
            return Ok(client);
        }
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| SummaryError::HttpClientUnavailable)?;
        Ok(self.http.get_or_init(|| client))
    }

    /// Dokumden ozet uretir. DB'ye dokunmaz.
    pub async fn summarize(
        &self,
        api_key: &SecretString,
        model: &str,
        lines: &[TranscriptLine],
    ) -> Result<SessionSummary, SummaryError> {
        if api_key.expose().trim().is_empty() {
            return Err(SummaryError::MissingApiKey);
        }

        let transcript = render_transcript(lines);
        let response = self
            .client()?
            .post(&self.endpoint)
            .bearer_auth(api_key.expose())
            .json(&ChatRequest {
                model,
                messages: [
                    ChatMessage {
                        role: "system",
                        content: SESSION_SUMMARY_PROMPT_V1,
                    },
                    ChatMessage {
                        role: "user",
                        content: &transcript,
                    },
                ],
            })
            .send()
            .await
            .map_err(|error| SummaryError::from_transport(&error))?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(SummaryError::from_status(status, model));
        }

        let parsed: ChatResponse = response
            .json()
            .await
            .map_err(|_| SummaryError::MalformedResponse)?;

        let text = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .as_deref()
            .and_then(clean_summary)
            .ok_or(SummaryError::MalformedResponse)?;

        let usage = parsed.usage;
        Ok(SessionSummary {
            text,
            usage: SummaryUsage {
                model: model.to_owned(),
                prompt_version: SUMMARY_PROMPT_VERSION,
                prompt_tokens: usage.as_ref().and_then(|usage| usage.prompt_tokens),
                completion_tokens: usage.as_ref().and_then(|usage| usage.completion_tokens),
                total_tokens: usage.as_ref().and_then(|usage| usage.total_tokens),
                estimated_cost_usd: None,
            },
        })
    }
}

impl Default for SummaryService {
    fn default() -> Self {
        Self::new()
    }
}

/// `Debug` elle yazildi: istemci nesnesinin varsayilan ciktisi gereksiz ic
/// detay basiyor.
impl fmt::Debug for SummaryService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SummaryService")
            .field("endpoint", &self.endpoint)
            .field("http", &self.http.get().map(|_| "<initialized>"))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Boru hatti
// ---------------------------------------------------------------------------

/// Dokum ozetlenmeye deger mi?
pub fn skip_reason(lines: &[TranscriptLine]) -> Option<SkipReason> {
    if lines.len() < MIN_TRANSCRIPT_LINES {
        return Some(SkipReason::TooShort);
    }
    if lines.iter().all(|line| line.text.trim().is_empty()) {
        return Some(SkipReason::NoContent);
    }
    None
}

/// Ozeti uretir ve kapanmis oturuma yazar.
///
/// **Hicbir kosulda `Err` donmez**: bu boru hatti oturumu bloklamaz, hatalar
/// burada log'lanip [`SummaryOutcome::Failed`] olarak bildirilir.
pub async fn summarize_session(
    service: &SummaryService,
    db: &AsunaDb,
    api_key: &SecretString,
    model: &str,
    session_id: i64,
    lines: &[TranscriptLine],
) -> SummaryOutcome {
    if let Some(reason) = skip_reason(lines) {
        return SummaryOutcome::Skipped(reason);
    }

    match service.summarize(api_key, model, lines).await {
        Ok(summary) => store(db, session_id, &summary),
        Err(error) => {
            // Sessiz yutma yok; ama oturum kaydi zaten kapali ve dogru.
            eprintln!(
                "[asuna] Oturum ozeti uretilemedi ({}): {}",
                error.kind(),
                redact_secrets(&error.to_string())
            );
            SummaryOutcome::Failed
        }
    }
}

/// Uretilmis ozeti kaydeder.
fn store(db: &AsunaDb, session_id: i64, summary: &SessionSummary) -> SummaryOutcome {
    let usage_patch = match serde_json::to_string(&summary.usage) {
        Ok(patch) => Some(patch),
        Err(_) => {
            // Ozet metni maliyet kaydi yuzunden kaybedilmez.
            eprintln!("[asuna] Ozet maliyeti JSON'a cevrilemedi; ozet maliyetsiz kaydediliyor.");
            None
        }
    };

    match session_repository::attach_summary(db, session_id, &summary.text, usage_patch.as_deref())
    {
        Ok(_) => SummaryOutcome::Stored,
        Err(error) => {
            eprintln!("[asuna] Oturum ozeti kaydedilemedi: {error}");
            SummaryOutcome::Failed
        }
    }
}

/// Kapanis akisindan cagrilan tetik: ozeti **arka planda** uretir.
///
/// Cagiran (`session_finalize`) bu noktada DB yazmasini bitirmistir; buradan
/// sonrasi oturumun kapanmasini etkilemez. Ozet servisi ya da DB kayitli
/// degilse (testler, hafizasiz mod) hicbir sey yapilmaz — sessizce degil,
/// log'layarak.
pub fn spawn_for_session<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session_id: i64,
    lines: Vec<TranscriptLine>,
) {
    if let Some(reason) = skip_reason(&lines) {
        if reason == SkipReason::TooShort {
            eprintln!(
                "[asuna] Oturum {session_id} icin ozet uretilmedi: {} replikten az.",
                MIN_TRANSCRIPT_LINES
            );
        }
        return;
    }

    // Servis kayitli degilse (ACL/birim testleri) **ag'a cikilmaz**.
    let Some(service) = app.try_state::<Arc<SummaryService>>() else {
        eprintln!("[asuna] Ozet servisi kayitli degil; oturum ozeti uretilmeyecek.");
        return;
    };
    let service = Arc::clone(service.inner());

    // Secret ve model **kopyalanarak** arka plan gorevine tasinir; state
    // kilidi bir ag cagrisi boyunca tutulmaz.
    let Some((api_key, model)) = app.try_state::<AsunaConfig>().map(|config| {
        (
            config.openai_api_key().clone(),
            config.summary_model.clone(),
        )
    }) else {
        eprintln!("[asuna] Yapilandirma okunamadi; oturum ozeti uretilmeyecek.");
        return;
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Ag cagrisi once: DB kilidi bir HTTP istegi boyunca tutulmaz.
        let outcome = match service.summarize(&api_key, &model, &lines).await {
            Ok(summary) => summary,
            Err(error) => {
                eprintln!(
                    "[asuna] Oturum ozeti uretilemedi ({}): {}",
                    error.kind(),
                    redact_secrets(&error.to_string())
                );
                return;
            }
        };

        let stored = {
            let Some(state) = app.try_state::<DbState>() else {
                return;
            };
            let Some(db) = state.database() else {
                return;
            };
            store(db, session_id, &outcome)
        };

        // ASU-034: hafiza cikarimi ozetin **ustune** kurulur ve ancak ozet
        // gercekten yazildiysa calisir (yazilmamis bir ozetten hafiza uretmek
        // kaynagi izlenemez bir kayit birakirdi). Buradan sonrasi ozeti
        // etkilemez: cikarim hata verse de `sessions.summary` yerinde kalir.
        if stored == SummaryOutcome::Stored {
            crate::extraction::extract_after_summary(&app, session_id, &outcome.text, &lines).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::mpsc::{self, Receiver};
    use std::thread;

    use crate::db::session_repository::{self, SessionFinalizeInput};
    use crate::db::AsunaDb;

    const TEST_API_KEY: &str = "sk-proj-COK-GIZLI-TEST-DEGERI";
    const TEST_MODEL: &str = "gpt-4o-mini";
    const START: &str = "2026-08-25T10:00:00Z";
    const END: &str = "2026-08-25T10:12:00Z";

    const SUMMARY_BODY: &str = r#"{
        "id": "chatcmpl-test",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Konusulanlar: Wake word mimarisi konusuldu.\nKararlar: Tespit yerelde kalacak.\nYarim kalanlar: Esik degeri secilmedi."
            }
        }],
        "usage": { "prompt_tokens": 310, "completion_tokens": 48, "total_tokens": 358 }
    }"#;

    fn secret(value: &str) -> SecretString {
        SecretString::new(value.to_owned())
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
            line(TranscriptRole::User, "Tamam, esigi sonra konusuruz."),
        ]
    }

    /// Kapanmis bir oturum iceren bellek ici DB.
    fn db_with_closed_session() -> (AsunaDb, i64) {
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let session =
            session_repository::start(&db, "gpt-realtime-2.1", None, START).expect("oturum");
        session_repository::finalize(&db, session.id, &SessionFinalizeInput::default(), None, END)
            .expect("kapanis");
        (db, session.id)
    }

    // --- Minimal HTTP test sunucusu (realtime_token.rs ile ayni desen) -----

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

        fn service(&self) -> SummaryService {
            SummaryService::with_endpoint(self.url.clone())
        }

        fn request(&self) -> RecordedRequest {
            self.received
                .recv_timeout(Duration::from_secs(5))
                .expect("sunucu bir istek kaydetmeliydi")
        }

        /// Sunucuya hic istek gelmedigini dogrular.
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

    // --- Kabul kriteri: sabit transcript -> ozet yazildi ------------------

    #[tokio::test]
    async fn a_fixed_transcript_produces_a_summary_that_is_written_to_the_session() {
        let server = MockServer::start("200 OK", SUMMARY_BODY);
        let (db, session_id) = db_with_closed_session();

        let outcome = summarize_session(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            session_id,
            &conversation(),
        )
        .await;

        assert_eq!(outcome, SummaryOutcome::Stored);

        let record = session_repository::get_by_id(&db, session_id)
            .expect("okuma")
            .expect("kayit");
        let summary = record.summary.expect("ozet yazilmali");
        assert!(summary.starts_with("Konusulanlar:"), "ozet: {summary}");
        assert!(summary.contains("Kararlar:"), "ozet: {summary}");
        assert!(summary.contains("Yarim kalanlar:"), "ozet: {summary}");

        // Oturum kaydinin kendisi bozulmadi.
        assert_eq!(record.ended_at.as_deref(), Some(END));
        assert_eq!(
            record.end_reason,
            Some(crate::db::SessionEndReason::Completed)
        );

        // Ozetleme maliyeti oturum metadata'sinda — token cinsinden.
        let usage: serde_json::Value =
            serde_json::from_str(&record.usage_json.expect("usage_json")).expect("gecerli JSON");
        assert_eq!(usage["summary"]["model"], TEST_MODEL);
        assert_eq!(usage["summary"]["promptTokens"], 310);
        assert_eq!(usage["summary"]["completionTokens"], 48);
        assert_eq!(usage["summary"]["totalTokens"], 358);
        assert_eq!(usage["summary"]["promptVersion"], SUMMARY_PROMPT_VERSION);
        // Fiyat dogrulanmadi: USD **uydurulmuyor**.
        assert!(usage["summary"]["estimatedCostUsd"].is_null());
    }

    /// Realtime oturumunun kendi token kirilimi ozet yazilirken **ezilmez**.
    #[tokio::test]
    async fn storing_a_summary_keeps_the_realtime_usage_breakdown() {
        let server = MockServer::start("200 OK", SUMMARY_BODY);
        let db = AsunaDb::open_in_memory().expect("bellek ici DB");
        let session =
            session_repository::start(&db, "gpt-realtime-2.1", None, START).expect("oturum");
        session_repository::finalize(
            &db,
            session.id,
            &SessionFinalizeInput {
                usage: Some(session_repository::SessionUsage {
                    requests: Some(4),
                    input_tokens: Some(1_200),
                    output_tokens: Some(800),
                    total_tokens: Some(2_000),
                    input_token_details: vec![serde_json::json!({ "audio_tokens": 1_200 })],
                    output_token_details: vec![serde_json::json!({ "audio_tokens": 800 })],
                }),
                ..SessionFinalizeInput::default()
            },
            None,
            END,
        )
        .expect("kapanis");

        summarize_session(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            session.id,
            &conversation(),
        )
        .await;

        let record = session_repository::get_by_id(&db, session.id)
            .expect("okuma")
            .expect("kayit");
        let usage: serde_json::Value =
            serde_json::from_str(&record.usage_json.expect("usage_json")).expect("gecerli JSON");

        assert_eq!(usage["requests"], 4, "realtime kullanimi kayboldu");
        assert_eq!(usage["inputTokenDetails"][0]["audio_tokens"], 1_200);
        assert_eq!(usage["summary"]["totalTokens"], 358);
        // Realtime maliyeti dogrulanmis fiyattan hesaplandi (voice.md Bolum 6).
        let expected = 1_200.0 * 32.0 / 1e6 + 800.0 * 64.0 / 1e6;
        let cost = record.estimated_cost_usd.expect("maliyet hesaplanmali");
        assert!((cost - expected).abs() < 1e-12, "maliyet: {cost}");
    }

    /// **Kabul kriteri**: ozet uretimi basarisiz olsa da oturum kaydi kapali
    /// kalir; `summary` NULL, hata log'lanir.
    #[tokio::test]
    async fn a_model_failure_leaves_the_session_closed_without_a_summary() {
        for status in [
            "401 Unauthorized",
            "429 Too Many Requests",
            "500 Server Error",
        ] {
            let server = MockServer::start(status, r#"{"error":{"message":"sk-proj-SIZAN"}}"#);
            let (db, session_id) = db_with_closed_session();

            let outcome = summarize_session(
                &server.service(),
                &db,
                &secret(TEST_API_KEY),
                TEST_MODEL,
                session_id,
                &conversation(),
            )
            .await;

            assert_eq!(outcome, SummaryOutcome::Failed, "durum: {status}");

            let record = session_repository::get_by_id(&db, session_id)
                .expect("okuma")
                .expect("kayit");
            assert_eq!(record.summary, None, "durum: {status}");
            assert_eq!(record.ended_at.as_deref(), Some(END), "oturum acildi mi?");
            assert_eq!(
                record.end_reason,
                Some(crate::db::SessionEndReason::Completed)
            );
        }
    }

    /// Ag hatasi da ayni: panik yok, oturum bozulmuyor.
    #[tokio::test]
    async fn a_network_failure_is_typed_and_does_not_touch_the_session() {
        let (db, session_id) = db_with_closed_session();
        let service = SummaryService::with_endpoint(closed_endpoint());

        let outcome = summarize_session(
            &service,
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            session_id,
            &conversation(),
        )
        .await;

        assert_eq!(outcome, SummaryOutcome::Failed);
        assert_eq!(
            session_repository::get_by_id(&db, session_id)
                .expect("okuma")
                .expect("kayit")
                .summary,
            None
        );
    }

    /// **Kabul kriteri**: cok kisa oturumda ozet uretilmiyor — ve ag'a hic
    /// cikilmiyor (bos gurultu de yok, bos maliyet de).
    #[tokio::test]
    async fn very_short_sessions_are_skipped_without_calling_the_model() {
        let server = MockServer::start("200 OK", SUMMARY_BODY);
        let (db, session_id) = db_with_closed_session();

        let outcome = summarize_session(
            &server.service(),
            &db,
            &secret(TEST_API_KEY),
            TEST_MODEL,
            session_id,
            &[line(TranscriptRole::User, "Hey Asuna")],
        )
        .await;

        assert_eq!(outcome, SummaryOutcome::Skipped(SkipReason::TooShort));
        server.assert_no_request();
        assert_eq!(
            session_repository::get_by_id(&db, session_id)
                .expect("okuma")
                .expect("kayit")
                .summary,
            None
        );
    }

    #[test]
    fn skip_rules_are_explicit() {
        assert_eq!(skip_reason(&[]), Some(SkipReason::TooShort));
        assert_eq!(
            skip_reason(&[line(TranscriptRole::User, "tek replik")]),
            Some(SkipReason::TooShort)
        );
        assert_eq!(
            skip_reason(&[
                line(TranscriptRole::User, "   "),
                line(TranscriptRole::Assistant, "")
            ]),
            Some(SkipReason::NoContent)
        );
        assert_eq!(skip_reason(&conversation()), None);
    }

    // --- Istek sekli ------------------------------------------------------

    #[tokio::test]
    async fn sends_a_minimal_chat_completions_request_with_the_configured_model() {
        let server = MockServer::start("200 OK", SUMMARY_BODY);
        server
            .service()
            .summarize(&secret(TEST_API_KEY), TEST_MODEL, &conversation())
            .await
            .expect("ozet uretilmeli");

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
        assert_eq!(body["model"], TEST_MODEL, "model config'ten gelmeli");

        // Govde minimum: yalnizca `model` + `messages`.
        let object = body.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["messages", "model"]);

        // Talimat + dokum; realtime modeline "kaydet" dedirtilmiyor, ayri cagri.
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], SESSION_SUMMARY_PROMPT_V1);
        let user = body["messages"][1]["content"]
            .as_str()
            .expect("dokum metni");
        assert!(
            user.contains("Kullanici: Wake word'u nasil kuralim?"),
            "{user}"
        );
        assert!(user.contains("Asuna: Tespiti yerelde tutalim."), "{user}");
    }

    /// Talimat "uydurma" yasagini acikca soylemeli — bu bir urun kurali
    /// (PROJECT.md "never invent memories") ve prompt'un icinde durmali.
    #[test]
    fn the_prompt_forbids_invention_and_asks_for_the_three_sections() {
        assert!(SESSION_SUMMARY_PROMPT_V1.contains("uydurma"));
        assert!(SESSION_SUMMARY_PROMPT_V1.contains("Konusulanlar:"));
        assert!(SESSION_SUMMARY_PROMPT_V1.contains("Kararlar:"));
        assert!(SESSION_SUMMARY_PROMPT_V1.contains("Yarim kalanlar:"));
    }

    #[tokio::test]
    async fn missing_api_key_is_reported_before_any_request() {
        let service = SummaryService::with_endpoint(closed_endpoint());
        for blank in ["", "   "] {
            let error = service
                .summarize(&secret(blank), TEST_MODEL, &conversation())
                .await
                .expect_err("bos key hata uretmeli");
            assert_eq!(error, SummaryError::MissingApiKey);
        }
    }

    #[tokio::test]
    async fn maps_http_status_codes_to_distinct_variants() {
        let cases: [(&'static str, SummaryError); 5] = [
            ("401 Unauthorized", SummaryError::InvalidApiKey),
            (
                "404 Not Found",
                SummaryError::ModelAccessDenied {
                    model: TEST_MODEL.to_owned(),
                },
            ),
            ("429 Too Many Requests", SummaryError::QuotaExceeded),
            (
                "503 Service Unavailable",
                SummaryError::UpstreamUnavailable { status: 503 },
            ),
            (
                "418 I'm a teapot",
                SummaryError::UnexpectedStatus { status: 418 },
            ),
        ];

        for (status_line, expected) in cases {
            let server = MockServer::start(
                status_line,
                r#"{"error":{"message":"key sk-proj-SIZAN, token ek_SIZAN"}}"#,
            );
            let error = server
                .service()
                .summarize(&secret(TEST_API_KEY), TEST_MODEL, &conversation())
                .await
                .expect_err("hata bekleniyordu");
            assert_eq!(error, expected, "durum: {status_line}");
        }
    }

    #[tokio::test]
    async fn rejects_malformed_or_empty_completions() {
        let bodies = [
            r#"{"choices":[]}"#,
            r#"{"choices":[{"message":{"role":"assistant"}}]}"#,
            r#"{"choices":[{"message":{"role":"assistant","content":"   "}}]}"#,
            r#"{"choices":[{"message":{"role":"assistant","content":""}}]}"#,
            "<html>gateway</html>",
        ];

        for body in bodies {
            let server = MockServer::start("200 OK", body);
            let error = server
                .service()
                .summarize(&secret(TEST_API_KEY), TEST_MODEL, &conversation())
                .await
                .expect_err("bozuk govde hata uretmeli");
            assert_eq!(error, SummaryError::MalformedResponse, "govde: {body}");
        }
    }

    /// `usage` gelmezse ozet yine saklanir; token sayilari **uydurulmaz**.
    #[tokio::test]
    async fn a_response_without_usage_still_stores_the_summary_with_unknown_cost() {
        let server = MockServer::start(
            "200 OK",
            r#"{"choices":[{"message":{"content":"Konusulanlar: Kisa bir sohbet."}}]}"#,
        );
        let (db, session_id) = db_with_closed_session();

        assert_eq!(
            summarize_session(
                &server.service(),
                &db,
                &secret(TEST_API_KEY),
                TEST_MODEL,
                session_id,
                &conversation(),
            )
            .await,
            SummaryOutcome::Stored
        );

        let record = session_repository::get_by_id(&db, session_id)
            .expect("okuma")
            .expect("kayit");
        let usage: serde_json::Value =
            serde_json::from_str(&record.usage_json.expect("usage_json")).expect("gecerli JSON");
        assert!(usage["summary"]["totalTokens"].is_null());
        assert_eq!(usage["summary"]["model"], TEST_MODEL);
    }

    // --- Redaksiyon / gizlilik -------------------------------------------

    /// Hicbir hata varyanti secret ya da API govdesi tasimaz.
    #[test]
    fn no_error_variant_leaks_secret_material() {
        let variants = [
            SummaryError::MissingApiKey,
            SummaryError::InvalidApiKey,
            SummaryError::ModelAccessDenied {
                model: TEST_MODEL.to_owned(),
            },
            SummaryError::QuotaExceeded,
            SummaryError::Network {
                cause: NetworkCause::Timeout,
            },
            SummaryError::UpstreamUnavailable { status: 503 },
            SummaryError::UnexpectedStatus { status: 418 },
            SummaryError::MalformedResponse,
            SummaryError::HttpClientUnavailable,
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

    /// Servisin `Debug` ciktisi secret basmaz.
    #[test]
    fn service_debug_output_is_safe() {
        let debug = format!("{:?}", SummaryService::new());
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

    // --- Dokum isleme -----------------------------------------------------

    /// Zaman damgasi modele gitmez; roller Turkce etiketlenir.
    #[test]
    fn rendered_transcript_omits_timestamps() {
        let rendered = render_transcript(&[TranscriptLine {
            role: TranscriptRole::User,
            text: "merhaba".to_owned(),
            at: Some("2026-08-25T10:00:00Z".to_owned()),
        }]);
        assert_eq!(rendered, "Kullanici: merhaba");
    }

    /// Uzun oturum kirpilir: **son** replikler tutulur ve kirpma modele
    /// soylenir (model "konusmanin basi buydu" sanmamali).
    #[test]
    fn a_long_transcript_is_clipped_from_the_start_and_marked() {
        let lines: Vec<TranscriptLine> = (0..2_000)
            .map(|index| line(TranscriptRole::User, &format!("replik {index} ")))
            .collect();

        let rendered = render_transcript(&lines);
        assert!(rendered.chars().count() <= MAX_PROMPT_CHARS + TRUNCATION_MARKER.len() + 1);
        assert!(rendered.starts_with(TRUNCATION_MARKER), "{rendered:.80}");
        assert!(rendered.ends_with("replik 1999"), "son replik korunmali");
    }

    /// Model talimata uymazsa saklanan ozet yine de sinirli kalir ve UTF-8
    /// bozulmaz.
    #[test]
    fn an_over_long_summary_is_clipped_on_a_character_boundary() {
        let raw = "ç".repeat(MAX_SUMMARY_CHARS + 500);
        let cleaned = clean_summary(&raw).expect("metin var");
        assert_eq!(cleaned.chars().count(), MAX_SUMMARY_CHARS + 1);
        assert!(cleaned.ends_with('…'));
    }

    #[test]
    fn blank_summaries_are_rejected() {
        assert_eq!(clean_summary("   \n "), None);
        assert_eq!(clean_summary(" ozet "), Some("ozet".to_owned()));
    }
}

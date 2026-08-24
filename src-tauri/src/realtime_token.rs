//! Ephemeral Realtime client secret uretimi (ASU-011).
//!
//! # Neden Rust tarafinda
//!
//! Kalici `OPENAI_API_KEY` bu process'ten cikmaz (PROJECT.md Bolum 7 & 19).
//! Renderer, Realtime oturumu acmadan once bu modulun komutunu cagirir ve
//! yalnizca **kisa omurlu** bir client secret (`ek_...`) ile `expires_at`
//! degerini alir. `Authorization: Bearer <kalici key>` header'i sadece burada,
//! `https://api.openai.com/v1/realtime/client_secrets` istegine eklenir.
//!
//! # Guvenlik sozlesmesi
//!
//! - Donen tipte (`EphemeralToken`) kalici key, organizasyon/proje bilgisi ya da
//!   ham API yaniti **yok**; yalnizca `value`, `expiresAt`, `model`.
//! - `EphemeralToken`'in `Debug` implementasyonu elle yazildi: token degeri
//!   log'a/panic mesajina basilamaz.
//! - Hicbir hata varyanti API yanit govdesini ya da istek header'ini tasimaz.
//!   `reqwest::Error` bilerek saklanmaz (Display'i URL sizdirabilir); yerine
//!   kaba bir [`NetworkCause`] siniflandirmasi tutulur.
//! - IPC'ye giden hata mesaji son bir kez [`redact_secrets`] suzgecinden gecer.
//!
//! Referans: `docs/architecture/voice.md` Bolum 5 (endpoint, payload, yanit
//! semasi ve hata beklentileri 2026-08-24'te dogrulandi).

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize, Serializer};
use tauri::State;
use thiserror::Error;

use crate::config::{AsunaConfig, SecretString};

/// OpenAI ephemeral client secret endpoint'i (voice.md Bolum 5).
pub const CLIENT_SECRETS_URL: &str = "https://api.openai.com/v1/realtime/client_secrets";

/// `expires_after.anchor` icin API'nin kabul ettigi tek deger.
const EXPIRES_ANCHOR: &str = "created_at";

/// Token'in gecerlilik suresi (saniye). API'nin varsayilani da 600; deger
/// **acikca** gonderiliyor ki TTL sunucu tarafinda sessizce degisemesin.
/// Kabul edilen aralik 10-7200 (voice.md Bolum 5).
///
/// Bu sure token ile *oturum acma* penceresidir; acilan oturum token'in
/// omrunden bagimsiz olarak devam eder.
const TOKEN_TTL_SECONDS: u32 = 600;

/// Tum istegin (baglanti + yanit) ust siniri. Token uretimi kullanicinin
/// "Asuna ile konus" tiklamasi ile ses arasindaki gecikmede duruyor; burada
/// dakikalarca beklemek yerine durust bir ag hatasi vermek daha iyi.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// TCP + TLS el sikismasi icin ayri, daha kisa sinir.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Ephemeral token prefix'i (voice.md Bolum 5: basari kriteri).
const EPHEMERAL_PREFIX: &str = "ek_";

/// Kalici API key prefix'i. Bir yanit bu prefix'le gelirse renderer'a
/// gecirmek yerine hata uretilir.
const PERMANENT_KEY_PREFIX: &str = "sk-";

// ---------------------------------------------------------------------------
// Redaksiyon
// ---------------------------------------------------------------------------

/// Metindeki `sk-...` / `ek_...` gorunumlu her parcayi maskeler.
///
/// Bu bir *son savunma hatti*: modulun hicbir hata varyanti zaten secret
/// tasimiyor, ama IPC sinirindan gecen mesaj bu suzgecten gecirilir ki
/// ilerideki bir degisiklik sessizce sizinti uretmesin.
pub fn redact_secrets(input: &str) -> String {
    // Ayirici olarak whitespace ve JSON/tirnak gurultusu kullaniliyor; token
    // karakter kumesi (harf, rakam, `-`, `_`) disindaki her sey sinir sayilir.
    let is_token_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';

    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while !rest.is_empty() {
        // Sonraki aday baslangici: token karakteri olan bir konum.
        let Some(start) = rest.find(is_token_char) else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail.find(|c: char| !is_token_char(c)).unwrap_or(tail.len());
        let word = &tail[..end];

        if word.starts_with(PERMANENT_KEY_PREFIX) {
            output.push_str("sk-<redacted>");
        } else if word.starts_with(EPHEMERAL_PREFIX) {
            output.push_str("ek_<redacted>");
        } else {
            output.push_str(word);
        }

        rest = &tail[end..];
    }

    output
}

// ---------------------------------------------------------------------------
// Donus tipi
// ---------------------------------------------------------------------------

/// Renderer'a giden kisa omurlu Realtime kimlik bilgisi.
///
/// `Serialize` camelCase uretir: `{ value, expiresAt, model }`. Bilerek
/// `Deserialize` **turetilmedi** — bu tip yalnizca disari akar.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EphemeralToken {
    value: String,
    expires_at: i64,
    model: String,
}

impl EphemeralToken {
    /// Token degeri. Yalnizca IPC serilestirmesi ve testler icin; log'lanmaz.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Unix epoch (saniye) — token'in son kullanma zamani.
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }

    /// Token'in basildigi model ID'si (config'ten gelir, hard-code degil).
    pub fn model(&self) -> &str {
        &self.model
    }
}

/// GUVENLIK: `Debug` elle yazildi. `derive(Debug)` olsaydi bir `unwrap`/panic
/// mesaji ya da `tracing` cagrisi token'i log'a dusurebilirdi.
impl fmt::Debug for EphemeralToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EphemeralToken")
            .field("value", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("model", &self.model)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Hata tipi
// ---------------------------------------------------------------------------

/// Ag katmani hatasinin kaba siniflandirmasi.
///
/// `reqwest::Error`'in kendisi saklanmaz: Display'i istek URL'sini ve zincirdeki
/// alt hatalari basar. Kullaniciya bu ayrintinin faydasi yok, sizma riski var.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkCause {
    /// TCP/TLS baglantisi kurulamadi (internet yok, DNS, proxy, sertifika).
    Connect,
    /// Sinir asildi ([`REQUEST_TIMEOUT`] / [`CONNECT_TIMEOUT`]).
    Timeout,
    /// Istek gonderildi ama tamamlanamadi (baglanti koptu, govde yarim kaldi).
    Interrupted,
}

impl NetworkCause {
    /// Kullaniciya gosterilen kisa neden. `pub`: ozet boru hatti (ASU-033) da
    /// ayni siniflandirmayi kullaniyor — iki yerde iki farkli "baglanti hatasi"
    /// sozlugu tutmak drift uretir.
    pub fn as_turkish(self) -> &'static str {
        match self {
            Self::Connect => "baglanti kurulamadi",
            Self::Timeout => "zaman asimi",
            Self::Interrupted => "baglanti kesildi",
        }
    }
}

/// Token uretiminin ayirt edilmis hata durumlari (PROJECT.md Bolum 30).
///
/// Her varyant kullaniciya **durust** ve farkli bir mesaj uretir; "bir seyler
/// ters gitti" tek kovasi yok. Hicbir varyant secret, header ya da yanit
/// govdesi tasimaz.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RealtimeTokenError {
    /// Kalici API key hic yok / bos. Normalde acilista yakalanir (ASU-009),
    /// burada da savunma amacli kontrol edilir.
    #[error(
        "OpenAI API anahtari tanimli degil. `.env` dosyasindaki `OPENAI_API_KEY` \
         degerini doldurup Asuna'yi yeniden baslatin."
    )]
    MissingApiKey,

    /// HTTP 401 — key OpenAI tarafindan reddedildi.
    #[error(
        "OpenAI API anahtari gecersiz (yetkilendirme reddedildi). Anahtari \
         yenileyip `.env` dosyasini guncelleyin."
    )]
    InvalidApiKey,

    /// HTTP 403 / 404 — hesabin bu Realtime modeline erisimi yok.
    #[error(
        "Bu hesabin `{model}` modeline erisimi yok. OpenAI panelinden model \
         erisimini kontrol edin ya da `ASUNA_REALTIME_MODEL` degerini erisiminiz \
         olan bir modele ayarlayin."
    )]
    ModelAccessDenied { model: String },

    /// HTTP 429 — kota/oran siniri veya faturalandirma.
    #[error(
        "OpenAI kota sinirina takildi. Faturalandirma/kullanim limitlerini \
         kontrol edin, sonra tekrar deneyin."
    )]
    QuotaExceeded,

    /// Istek OpenAI'ya hic ulasamadi ya da yarida kaldi.
    #[error("OpenAI'ya ulasamadim ({}). Internet baglantinizi kontrol edin.", cause.as_turkish())]
    Network { cause: NetworkCause },

    /// HTTP 5xx — servis tarafinda gecici sorun (PROJECT.md Bolum 30
    /// "API unavailable"). Cagiran taraf yeniden denemeye karar verebilir.
    #[error(
        "OpenAI ses servisi su an yanit vermiyor (HTTP {status}). Birazdan \
         tekrar deneyin."
    )]
    UpstreamUnavailable { status: u16 },

    /// Siniflandirilamayan HTTP durumu. Yutulmaz, oldugu gibi bildirilir.
    #[error("OpenAI beklenmeyen bir yanit dondu (HTTP {status}).")]
    UnexpectedStatus { status: u16 },

    /// 200 geldi ama govde beklenen semada degil (ya da token bos/yanlis
    /// bicimde). "Basarili" gibi davranmak yok.
    #[error("OpenAI'nin ses oturumu yaniti okunamadi (beklenen alanlar eksik veya gecersiz).")]
    MalformedResponse,

    /// HTTPS istemcisi kurulamadi (TLS yapilandirmasi/kok sertifikalar).
    #[error("Guvenli HTTPS istemcisi kurulamadi. Sistem TLS yapilandirmasi okunamiyor.")]
    HttpClientUnavailable,
}

impl RealtimeTokenError {
    /// Renderer'in `switch` yazabilecegi stabil makine-okunur etiket.
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

    /// HTTP durum kodunu tipli varyanta cevirir.
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

    /// `reqwest` hatasini kaba bir ag nedenine indirger — hata nesnesi
    /// saklanmaz (bkz. [`NetworkCause`]).
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

/// IPC'ye giden bicim: `{ "kind": "...", "message": "..." }`.
///
/// Mesaj [`redact_secrets`] suzgecinden gecer.
impl Serialize for RealtimeTokenError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            kind: &'a str,
            message: String,
        }

        Wire {
            kind: self.kind(),
            message: redact_secrets(&self.to_string()),
        }
        .serialize(serializer)
    }
}

// ---------------------------------------------------------------------------
// Istek / yanit semalari
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ExpiresAfter {
    anchor: &'static str,
    seconds: u32,
}

/// `session` payload'i bilerek **minimum**: `instructions`, `tools` ve ses
/// ayarlari SDK tarafindan data channel uzerinden `session.update` ile
/// gonderiliyor; iki yerde tutmak drift uretir (voice.md Bolum 5).
#[derive(Serialize)]
struct SessionSpec<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    model: &'a str,
}

#[derive(Serialize)]
struct ClientSecretRequest<'a> {
    expires_after: ExpiresAfter,
    session: SessionSpec<'a>,
}

impl<'a> ClientSecretRequest<'a> {
    fn new(model: &'a str) -> Self {
        Self {
            expires_after: ExpiresAfter {
                anchor: EXPIRES_ANCHOR,
                seconds: TOKEN_TTL_SECONDS,
            },
            session: SessionSpec {
                kind: "realtime",
                model,
            },
        }
    }
}

/// Yanittan **yalnizca** iki alan aliniyor. `session` blogu ve olasi
/// organizasyon/proje alanlari bilerek okunmuyor: okunmayan veri sizamaz.
#[derive(Deserialize)]
struct ClientSecretResponse {
    value: String,
    expires_at: i64,
}

// ---------------------------------------------------------------------------
// Servis
// ---------------------------------------------------------------------------

/// Token uretme servisi. Tauri state'inde tek ornek olarak yasar.
///
/// HTTPS istemcisi ilk basarili kullanimda kurulur ve saklanir: acilista
/// kurulamayan bir TLS yapilandirmasi tum uygulamayi dusurmemeli
/// (PROJECT.md Bolum 30 — bozulan alt sistem urunu dusurmez), hata token
/// istendigi anda tipli olarak yuzeye cikar.
pub struct RealtimeTokenService {
    endpoint: String,
    http: OnceLock<reqwest::Client>,
}

impl RealtimeTokenService {
    /// Gercek OpenAI endpoint'ine bakan servis.
    pub fn new() -> Self {
        Self::with_endpoint(CLIENT_SECRETS_URL)
    }

    /// Endpoint'i degistirilebilir kurucu. Testler yerel bir HTTP sunucusuna
    /// yonlendirir — testler gercek API'ye **vurmaz** (conventions.md Testing).
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            http: OnceLock::new(),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, RealtimeTokenError> {
        if let Some(client) = self.http.get() {
            return Ok(client);
        }
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            // Yonlendirme yok: `Authorization` header'i tanimadigimiz bir
            // host'a tasinmasin.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| RealtimeTokenError::HttpClientUnavailable)?;
        Ok(self.http.get_or_init(|| client))
    }

    /// Kisa omurlu client secret uretir.
    ///
    /// `api_key` yalnizca `Authorization` header'ina yazilir; donen degerde,
    /// hata varyantlarinda ve log'da yer almaz.
    pub async fn mint(
        &self,
        api_key: &SecretString,
        model: &str,
    ) -> Result<EphemeralToken, RealtimeTokenError> {
        if api_key.expose().trim().is_empty() {
            return Err(RealtimeTokenError::MissingApiKey);
        }

        let response = self
            .client()?
            .post(&self.endpoint)
            .bearer_auth(api_key.expose())
            .json(&ClientSecretRequest::new(model))
            .send()
            .await
            .map_err(|error| RealtimeTokenError::from_transport(&error))?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(RealtimeTokenError::from_status(status, model));
        }

        let parsed: ClientSecretResponse = response
            .json()
            .await
            .map_err(|_| RealtimeTokenError::MalformedResponse)?;

        let value = parsed.value.trim();
        // Kalici key gorunumlu bir deger renderer'a **asla** gecmez. Prefix
        // `ek_` beklenir; OpenAI prefix'i degistirirse burada patlamamasi icin
        // sert kural yalnizca `sk-` yasagi + bos olmama uzerinde.
        if value.is_empty() || value.starts_with(PERMANENT_KEY_PREFIX) || parsed.expires_at <= 0 {
            return Err(RealtimeTokenError::MalformedResponse);
        }

        Ok(EphemeralToken {
            value: value.to_owned(),
            expires_at: parsed.expires_at,
            model: model.to_owned(),
        })
    }
}

impl Default for RealtimeTokenService {
    fn default() -> Self {
        Self::new()
    }
}

/// `Debug` elle yazildi: endpoint dogrudan secret degil ama istemci nesnesinin
/// varsayilan `Debug` ciktisi gereksiz ic detay basiyor.
impl fmt::Debug for RealtimeTokenService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RealtimeTokenService")
            .field("endpoint", &self.endpoint)
            .field("http", &self.http.get().map(|_| "<initialized>"))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tauri komutu
// ---------------------------------------------------------------------------

/// Renderer'in Realtime oturumu acmak icin cagirdigi komut.
///
/// Model ID `ASUNA_REALTIME_MODEL` config'inden gelir (hard-code yok).
/// Token cache'lenmez: her `connect()` oncesi taze basilir (voice.md Bolum 5).
#[tauri::command]
pub async fn mint_realtime_token(
    config: State<'_, AsunaConfig>,
    tokens: State<'_, RealtimeTokenService>,
) -> Result<EphemeralToken, RealtimeTokenError> {
    tokens
        .mint(config.openai_api_key(), &config.realtime_model)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::mpsc::{self, Receiver};
    use std::thread;

    const TEST_API_KEY: &str = "sk-proj-COK-GIZLI-TEST-DEGERI";
    const TEST_MODEL: &str = "gpt-realtime-2.1";
    const TEST_TOKEN: &str = "ek_TEST_EPHEMERAL_DEGERI";

    fn secret(value: &str) -> SecretString {
        SecretString::new(value.to_owned())
    }

    // --- Minimal HTTP test sunucusu -------------------------------------
    // `wiremock` yerine ~60 satir std kodu: yeni bir bagimlilik (ve onun
    // getirdigi ~30 crate) tek bir POST'u taklit etmek icin gereksiz.

    /// Sunucunun gordugu istek — assertion'lar icin kaydedilir.
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
        /// Tek istek karsilar, verilen durum + govdeyi doner, sonra kapanir.
        fn start(status_line: &'static str, body: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("port acilmali");
            let url = format!(
                "http://{}/v1/realtime/client_secrets",
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

        fn service(&self) -> RealtimeTokenService {
            RealtimeTokenService::with_endpoint(self.url.clone())
        }

        fn request(&self) -> RecordedRequest {
            self.received
                .recv_timeout(Duration::from_secs(5))
                .expect("sunucu bir istek kaydetmeliydi")
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

    /// Hicbir seyin dinlemedigi bir adres: baglanti reddi uretir.
    fn closed_endpoint() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("port acilmali");
        let addr = listener.local_addr().expect("adres okunmali");
        drop(listener);
        format!("http://{addr}/v1/realtime/client_secrets")
    }

    // --- Basarili mint --------------------------------------------------

    #[tokio::test]
    async fn mints_a_token_and_parses_value_and_expiry() {
        let server = MockServer::start(
            "200 OK",
            r#"{"value":"ek_TEST_EPHEMERAL_DEGERI","expires_at":1690000600,
                "session":{"type":"realtime","model":"gpt-realtime-2.1",
                "organization":"org-GIZLI"}}"#,
        );

        let token = server
            .service()
            .mint(&secret(TEST_API_KEY), TEST_MODEL)
            .await
            .expect("basarili yanit token uretmeli");

        assert_eq!(token.value(), TEST_TOKEN);
        assert_eq!(token.expires_at(), 1_690_000_600);
        assert_eq!(token.model(), TEST_MODEL);
    }

    /// Istek voice.md Bolum 5'teki sozlesmeye uygun mu: dogru metod, kalici
    /// key `Authorization` header'inda, payload minimum, TTL acikca 600.
    #[tokio::test]
    async fn sends_the_documented_request_shape() {
        let server = MockServer::start("200 OK", r#"{"value":"ek_x","expires_at":1}"#);
        server
            .service()
            .mint(&secret(TEST_API_KEY), TEST_MODEL)
            .await
            .expect("mint basarili olmali");

        let request = server.request();
        assert!(
            request
                .request_line
                .starts_with("POST /v1/realtime/client_secrets"),
            "istek satiri: {}",
            request.request_line
        );
        assert_eq!(
            request.header("authorization"),
            Some(format!("Bearer {TEST_API_KEY}").as_str())
        );
        assert_eq!(request.header("content-type"), Some("application/json"));

        let body: serde_json::Value =
            serde_json::from_str(&request.body).expect("govde JSON olmali");
        assert_eq!(body["session"]["type"], "realtime");
        assert_eq!(body["session"]["model"], TEST_MODEL);
        assert_eq!(body["expires_after"]["anchor"], "created_at");
        assert_eq!(body["expires_after"]["seconds"], 600);
        // `session` payload'i minimum kalmali (drift onlemi).
        let session = body["session"].as_object().expect("session nesnesi");
        let mut keys: Vec<&str> = session.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["model", "type"]);
    }

    /// Donen tipte kalici key ve organizasyon bilgisi yok; JSON camelCase.
    #[tokio::test]
    async fn serialized_token_exposes_only_three_camel_case_fields() {
        let server = MockServer::start(
            "200 OK",
            r#"{"value":"ek_TEST_EPHEMERAL_DEGERI","expires_at":1690000600,
                "session":{"organization":"org-GIZLI","project":"proj-GIZLI"}}"#,
        );
        let token = server
            .service()
            .mint(&secret(TEST_API_KEY), TEST_MODEL)
            .await
            .expect("mint basarili olmali");

        let json = serde_json::to_value(&token).expect("serialize edilebilmeli");
        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["expiresAt", "model", "value"]);

        let serialized = json.to_string();
        assert!(!serialized.contains(TEST_API_KEY), "JSON: {serialized}");
        assert!(!serialized.contains("org-GIZLI"), "JSON: {serialized}");
        assert!(!serialized.contains("proj-GIZLI"), "JSON: {serialized}");
    }

    // --- Hata varyantlari -----------------------------------------------

    #[tokio::test]
    async fn missing_api_key_is_reported_before_any_request() {
        // Endpoint kasten kapali: istek atilirsa `Network` gelirdi.
        let service = RealtimeTokenService::with_endpoint(closed_endpoint());
        for blank in ["", "   ", "\t\n"] {
            let error = service
                .mint(&secret(blank), TEST_MODEL)
                .await
                .expect_err("bos key hata uretmeli");
            assert_eq!(error, RealtimeTokenError::MissingApiKey);
        }
    }

    #[tokio::test]
    async fn maps_http_status_codes_to_distinct_variants() {
        let cases: [(&'static str, RealtimeTokenError); 7] = [
            ("401 Unauthorized", RealtimeTokenError::InvalidApiKey),
            (
                "403 Forbidden",
                RealtimeTokenError::ModelAccessDenied {
                    model: TEST_MODEL.to_owned(),
                },
            ),
            (
                "404 Not Found",
                RealtimeTokenError::ModelAccessDenied {
                    model: TEST_MODEL.to_owned(),
                },
            ),
            ("429 Too Many Requests", RealtimeTokenError::QuotaExceeded),
            (
                "500 Internal Server Error",
                RealtimeTokenError::UpstreamUnavailable { status: 500 },
            ),
            (
                "503 Service Unavailable",
                RealtimeTokenError::UpstreamUnavailable { status: 503 },
            ),
            (
                "418 I'm a teapot",
                RealtimeTokenError::UnexpectedStatus { status: 418 },
            ),
        ];

        for (status_line, expected) in cases {
            // Govde bilerek secret gorunumlu: hicbir varyanta sizmamali.
            let server = MockServer::start(
                status_line,
                r#"{"error":{"message":"invalid api key sk-proj-SIZAN, token ek_SIZAN"}}"#,
            );
            let error = server
                .service()
                .mint(&secret(TEST_API_KEY), TEST_MODEL)
                .await
                .expect_err("hata bekleniyordu");
            assert_eq!(error, expected, "durum: {status_line}");
            assert!(!error.to_string().contains("sk-proj-SIZAN"));
            assert!(!error.to_string().contains("ek_SIZAN"));
        }
    }

    #[tokio::test]
    async fn network_failure_is_typed_not_panicking() {
        let service = RealtimeTokenService::with_endpoint(closed_endpoint());
        let error = service
            .mint(&secret(TEST_API_KEY), TEST_MODEL)
            .await
            .expect_err("baglanti reddi hata uretmeli");

        assert_eq!(error.kind(), "network");
        assert!(matches!(error, RealtimeTokenError::Network { .. }));
    }

    /// Gecersiz endpoint (bos URL) da panic degil, tipli hata uretir.
    #[tokio::test]
    async fn invalid_endpoint_does_not_panic() {
        let service = RealtimeTokenService::with_endpoint("bu-bir-url-degil");
        let error = service
            .mint(&secret(TEST_API_KEY), TEST_MODEL)
            .await
            .expect_err("gecersiz URL hata uretmeli");
        assert!(matches!(error, RealtimeTokenError::Network { .. }));
    }

    #[tokio::test]
    async fn rejects_malformed_success_responses() {
        let bodies = [
            // Alan eksik
            r#"{"expires_at":1690000600}"#,
            r#"{"value":"ek_x"}"#,
            // Tip yanlis
            r#"{"value":"ek_x","expires_at":"yarin"}"#,
            // JSON degil
            "<html>gateway</html>",
            // Bos token
            r#"{"value":"","expires_at":1690000600}"#,
            r#"{"value":"   ","expires_at":1690000600}"#,
            // Gecersiz expiry
            r#"{"value":"ek_x","expires_at":0}"#,
            r#"{"value":"ek_x","expires_at":-1}"#,
        ];

        for body in bodies {
            let server = MockServer::start("200 OK", body);
            let error = server
                .service()
                .mint(&secret(TEST_API_KEY), TEST_MODEL)
                .await
                .expect_err("bozuk govde hata uretmeli");
            assert_eq!(
                error,
                RealtimeTokenError::MalformedResponse,
                "govde: {body}"
            );
        }
    }

    /// Kalici key gorunumlu bir deger renderer'a **gecmez**.
    #[tokio::test]
    async fn refuses_to_return_a_permanent_looking_key() {
        let server = MockServer::start(
            "200 OK",
            r#"{"value":"sk-proj-YANLISLIKLA-KALICI","expires_at":1690000600}"#,
        );
        let error = server
            .service()
            .mint(&secret(TEST_API_KEY), TEST_MODEL)
            .await
            .expect_err("kalici key gorunumlu deger reddedilmeli");
        assert_eq!(error, RealtimeTokenError::MalformedResponse);
    }

    // --- Redaksiyon ------------------------------------------------------

    #[test]
    fn redact_secrets_masks_permanent_and_ephemeral_tokens() {
        let cases = [
            ("Bearer sk-proj-ABC123", "Bearer sk-<redacted>"),
            ("token=ek_abc_DEF-99 bitti", "token=ek_<redacted> bitti"),
            (
                r#"{"value":"ek_XYZ","key":"sk-live-1"}"#,
                r#"{"value":"ek_<redacted>","key":"sk-<redacted>"}"#,
            ),
            (
                "sk-a ek_b sk-c",
                "sk-<redacted> ek_<redacted> sk-<redacted>",
            ),
            // Yanlis pozitif olmamali
            ("gpt-realtime-2.1 modeli", "gpt-realtime-2.1 modeli"),
            ("skor ekip degeri", "skor ekip degeri"),
            ("", ""),
        ];
        for (input, expected) in cases {
            assert_eq!(redact_secrets(input), expected, "girdi: {input}");
        }
    }

    /// Her hata varyantinin Display / Debug / IPC ciktisi secret'siz.
    #[test]
    fn no_error_variant_leaks_secret_material() {
        let variants = [
            RealtimeTokenError::MissingApiKey,
            RealtimeTokenError::InvalidApiKey,
            RealtimeTokenError::ModelAccessDenied {
                model: TEST_MODEL.to_owned(),
            },
            RealtimeTokenError::QuotaExceeded,
            RealtimeTokenError::Network {
                cause: NetworkCause::Connect,
            },
            RealtimeTokenError::Network {
                cause: NetworkCause::Timeout,
            },
            RealtimeTokenError::Network {
                cause: NetworkCause::Interrupted,
            },
            RealtimeTokenError::UpstreamUnavailable { status: 503 },
            RealtimeTokenError::UnexpectedStatus { status: 418 },
            RealtimeTokenError::MalformedResponse,
            RealtimeTokenError::HttpClientUnavailable,
        ];

        let mut seen_kinds = Vec::new();
        for variant in &variants {
            let rendered = format!("{variant} | {variant:?}");
            let wire = serde_json::to_string(variant).expect("serialize edilebilmeli");
            for haystack in [&rendered, &wire] {
                assert!(!haystack.contains("sk-"), "sizinti: {haystack}");
                assert!(!haystack.contains("ek_"), "sizinti: {haystack}");
                assert!(!haystack.contains(TEST_API_KEY), "sizinti: {haystack}");
            }
            // Mesaj bos/genel olmasin: kullaniciya durust bir sey soylemeli.
            assert!(variant.to_string().len() > 20, "mesaj cok kisa: {variant}");
            seen_kinds.push(variant.kind());
        }

        // Her varyantin ayri bir `kind` etiketi var (renderer ayirt edebilsin).
        let mut unique = seen_kinds.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 9, "kind etiketleri: {seen_kinds:?}");
    }

    /// IPC bicimi `{ kind, message }` — renderer bunun uzerine switch yazacak.
    #[test]
    fn error_serializes_as_kind_and_message() {
        let json = serde_json::to_value(RealtimeTokenError::QuotaExceeded)
            .expect("serialize edilebilmeli");
        assert_eq!(json["kind"], "quota_exceeded");
        assert!(
            json["message"]
                .as_str()
                .expect("mesaj string")
                .contains("kota"),
            "mesaj: {json}"
        );
        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["kind", "message"]);
    }

    /// Token'in `Debug` ciktisi degeri maskeler — log/panic yolu guvenli.
    #[tokio::test]
    async fn token_debug_output_redacts_the_value() {
        let server = MockServer::start(
            "200 OK",
            r#"{"value":"ek_TEST_EPHEMERAL_DEGERI","expires_at":1690000600}"#,
        );
        let token = server
            .service()
            .mint(&secret(TEST_API_KEY), TEST_MODEL)
            .await
            .expect("mint basarili olmali");

        let debug = format!("{token:?}");
        assert!(!debug.contains(TEST_TOKEN), "debug: {debug}");
        assert!(debug.contains("<redacted>"), "debug: {debug}");
        // Expiry ve model teshis icin gorunur kalir.
        assert!(debug.contains("1690000600"), "debug: {debug}");
        assert!(debug.contains(TEST_MODEL), "debug: {debug}");
    }

    /// Servisin `Debug` ciktisi da secret basmaz.
    #[test]
    fn service_debug_output_is_safe() {
        let service = RealtimeTokenService::new();
        let debug = format!("{service:?}");
        assert!(debug.contains(CLIENT_SECRETS_URL));
        assert!(!debug.contains("sk-"), "debug: {debug}");
    }

    #[test]
    fn default_endpoint_is_the_documented_openai_url() {
        assert_eq!(
            CLIENT_SECRETS_URL,
            "https://api.openai.com/v1/realtime/client_secrets"
        );
        assert!(CLIENT_SECRETS_URL.starts_with("https://"), "TLS zorunlu");
    }
}

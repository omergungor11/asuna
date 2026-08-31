//! Tool audit defteri — `tool_events` yazma ve okuma (ASU-050).
//!
//! # Sozlesme
//!
//! - **Append-only.** Bu modulde `UPDATE` ya da `DELETE` yolu **yoktur** ve
//!   renderer'a acilan komut kumesinde de yoktur ([`record_tool_event`] +
//!   [`tool_event_list`], baska hicbir sey). ASU-050 kabul kriteri: "audit
//!   kayitlari uygulamadan silinemiyor (MVP'de salt yazilir)". Kilit semada
//!   degil IPC yuzeyindedir ve bir ACL testiyle sabitlenmistir.
//! - **Her cagri yazilir.** Onaylanan, reddedilen, hata veren, zaman asimina
//!   ugrayan — hepsi. Yalnizca calisanlari kaydeden bir defter denetim defteri
//!   degil, bir basari vitrinidir (PROJECT.md Bolum 19).
//! - **Redaksiyon host tarafinda.** Renderer bir arguman **ozeti** gondermez;
//!   ham `arguments` JSON'unu gonderir ve ozeti [`summarize_arguments`] uretir.
//!   Neden bu yon: renderer'in urettigi bir metne guvenmek, redaksiyonu
//!   webview'e devretmek olurdu — oysa webview modelin ciktisiyla ayni
//!   process'te yasiyor. Ozet **hicbir zaman** ic ice bir degeri serilestirmez,
//!   yani bir dosya icerigi audit defterine yapisal olarak giremez.
//! - **Sessiz kayip yok.** Yazma basarisiz olursa komut tipli bir hata doner
//!   (`StoreError`) ve tam zincir yerel log'a yazilir. Cagiran taraf (ASU-047
//!   tool runner) bu hatayi tool sonucuna **karistirmaz** ama yutmaz da:
//!   TypeScript sarmalayicisi `src/asuna/tools/audit.ts` hatayi yapisal bir
//!   `failed` sonucuna cevirir ve `error` seviyesinde log'lar.
//!
//! # Arguman ozeti bicimi
//!
//! Tek satir, `anahtar=deger` ciftleri, `, ` ile ayrilmis; anahtarlar
//! **alfabetik** (deterministik — cagri sirasi bir bilgi sizdirmasin):
//!
//! ```text
//! path=README.md, maxBytes=4096
//! ```
//!
//! Deger kurallari:
//!
//! | JSON tipi | Ozette gorunum |
//! |---|---|
//! | string | bosluklari tek boslukta toplanmis, [`MAX_ARGUMENT_VALUE_CHARS`] karakterde kirpilmis metin (`…`) |
//! | number / bool | oldugu gibi |
//! | null | `null` |
//! | array | `[N oge]` — **icerik yok** |
//! | object | `{N alan}` — **icerik yok** |
//!
//! Kritik kural ic ice yapilar: bir dizinin ya da nesnenin **icerigi** hicbir
//! zaman yazilmaz, yalnizca sekli. Bu, "dosya icerigi audit'e girmiyor"
//! kriterini bir uzunluk tahminine degil, bicimin kendisine bagliyor.
//!
//! Uretilen metin sonra [`redaction::redact_sensitive_text`]'ten gecer
//! (`parola=...` / `token=...` / `sk-...` maskelenir) ve
//! [`MAX_ARGUMENT_SUMMARY_CHARS`] karakterde kirpilir. Kirpma **gorunur**: son
//! karakter `…` olur. Tavanlar semada CHECK olarak da yazili — bir gun bu kod
//! kirpmayi atlarsa satir INSERT aninda duser, defter sessizce sismez.

use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::clock;
use super::model::{ToolApprovalState, ToolEventRecord, ToolOutcome, ToolRiskLevel};
use super::store_error::{database, StoreError, StoreSkipReason};
use super::{AsunaDb, DbState};
use crate::privacy::PrivacyState;
use crate::redaction;

/// Tool adinin azami uzunlugu — semadaki CHECK ile ayni.
///
/// Tool adi bir **etikettir**; icerik tasiyacak kadar uzun olamaz. Tavan
/// asilirsa istek reddedilir (kirpilmaz): kirpilmis bir tool adi, var olmayan
/// bir tool'u varmis gibi gosterirdi.
pub const MAX_TOOL_NAME_CHARS: usize = 64;

/// Arguman ozetinin azami uzunlugu — semadaki CHECK ile ayni.
pub const MAX_ARGUMENT_SUMMARY_CHARS: usize = 512;

/// Sonuc ozetinin azami uzunlugu — semadaki CHECK ile ayni.
pub const MAX_RESULT_SUMMARY_CHARS: usize = 512;

/// Tek bir arguman degerinin ozette kaplayabilecegi azami karakter.
///
/// Kucuk bilincli: `path=src/main.rs` gibi degerler tam sigar, bir dosya
/// icerigi ya da uzun bir prompt sigmaz.
pub const MAX_ARGUMENT_VALUE_CHARS: usize = 64;

/// Ozette gosterilen azami anahtar sayisi; fazlasi `+N alan` olarak sayilir.
pub const MAX_ARGUMENT_KEYS: usize = 12;

/// Audit listesinin varsayilan uzunlugu.
pub const DEFAULT_TOOL_EVENT_LIST_LIMIT: u32 = 50;

/// Audit listesi icin tavan. Asan istek **reddedilmez, kirpilir** — ama
/// kirpildigi [`ToolEventPage`] icinde gorunur olur (`limit` + `total`),
/// `session_list` ile ayni sozlesme.
pub const MAX_TOOL_EVENT_LIST_LIMIT: u32 = 200;

/// Kirpma isareti. Metnin **kirpildigi** gorunur olmali: kirpilmis bir degeri
/// tam sanmak, argumanin ne oldugu konusunda yanlis fikir verir.
const TRUNCATION_MARKER: char = '…';

// ---------------------------------------------------------------------------
// Girdi / cikti tipleri
// ---------------------------------------------------------------------------

/// Bir tool cagrisinin audit girdisi.
///
/// `arguments` **ham** JSON'dur ve saklanmaz: ozet ve redaksiyon host tarafinda
/// uretilir (modul dokumantasyonu). `deny_unknown_fields`: renderer'in
/// `argumentsRedacted` gibi hazir bir metin gondermeye calismasi istegi
/// dusurur — o alan bu sozlesmede yok.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolEventInput {
    /// Cagriyi ureten oturum kaydinin kimligi. `None` = bilinmiyor (hafiza
    /// kapali ya da oturum kaydi henuz acilmadi). Uydurulmaz.
    #[serde(default)]
    pub session_id: Option<i64>,
    pub tool_name: String,
    pub risk_level: ToolRiskLevel,
    /// Ham argumanlar. `None` = cagri argumansizdi.
    #[serde(default)]
    pub arguments: Option<serde_json::Value>,
    pub approval_state: ToolApprovalState,
    /// Kisa sonuc ozeti — basari da hata da. `None` = soylenecek sonuc yok.
    ///
    /// DIKKAT (ASU-051): burasi **modele giden metin degildir**. Icerik donduren
    /// bir tool (`read_project_file`) modele dosyanin kendisini verir ama deftere
    /// tek satirlik bir ozet yazar; renderer tarafinda ayrimi `ToolResult.auditSummary`
    /// tasir. Yine de tek savunma bu degil: ozet host tarafinda tek satira
    /// indirilir, redaksiyondan gecer ve 512 karakterde kirpilir.
    #[serde(default)]
    pub result_summary: Option<String>,
    /// Cagri calisti mi, calistiysa basardi mi? (ASU-051).
    ///
    /// `None` = cagiran bu ekseni bildirmedi. Sessiz bir `succeeded` varsayimi
    /// yok: olculmemis bir basari iddiasi denetim defterine yazilmaz.
    #[serde(default)]
    pub outcome: Option<ToolOutcome>,
}

/// Audit yazma sonucu. `Skipped` = kalici hafiza kapali (hata degil).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ToolEventWriteResult {
    Recorded { event: Box<ToolEventRecord> },
    Skipped { reason: StoreSkipReason },
}

/// Audit listesi + **olculen** sinirlar (`SessionPage` ile ayni desen).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolEventPage {
    pub events: Vec<ToolEventRecord>,
    /// Uygulanan limit (kirpilmis olabilir).
    pub limit: u32,
    /// [`MAX_TOOL_EVENT_LIST_LIMIT`] — tavanin kendisi de gorunur.
    pub limit_max: u32,
    /// Filtreye uyan **toplam** kayit sayisi. "En yeni 50" demek yerine
    /// "50 / 214" demek mumkun olsun; tavana carpip carpmadigini UI tahmin
    /// etmesin.
    pub total: u32,
}

/// Liste istegi. Renderer yalnizca **kac tane** ve **hangi oturum** diyebilir;
/// siralama ve alan secimi host tarafinda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolEventListQuery {
    #[serde(default)]
    pub limit: Option<u32>,
    /// `None` = tum oturumlar (Tools sekmesinin varsayilan gorunumu).
    #[serde(default)]
    pub session_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// Arguman ozeti + redaksiyon
// ---------------------------------------------------------------------------

/// Metni tek satira indirir: tum bosluk dizileri tek boslukta toplanir.
///
/// Neden: audit satiri tek satirlik bir denetim kaydidir ve dosya icerigi
/// cok satirlidir. Tek satir sozlesmesi, uzunluk tavaniyla birlikte, icerik
/// dokmeyi yapisal olarak zorlastirir.
fn collapse_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut pending_space = false;

    for character in input.chars() {
        if character.is_whitespace() {
            pending_space = !output.is_empty();
            continue;
        }
        if pending_space {
            output.push(' ');
            pending_space = false;
        }
        output.push(character);
    }

    output
}

/// Metni `limit` karaktere kirpar; kirpildiysa sona [`TRUNCATION_MARKER`] koyar.
///
/// Karakter (kod noktasi) bazli: SQLite `length()` de TEXT icin karakter sayar,
/// yani semadaki CHECK ile ayni birimi kullaniyoruz. Byte bazli kirpma hem UTF-8
/// siniri kirabilir hem tavani yanlis olcerdi.
fn truncate_chars(input: &str, limit: usize) -> String {
    if input.chars().count() <= limit {
        return input.to_owned();
    }
    // Isaret de tavana dahil: sonuc asla `limit`i asmaz.
    let keep = limit.saturating_sub(1);
    let mut output: String = input.chars().take(keep).collect();
    output.push(TRUNCATION_MARKER);
    output
}

/// Tek bir JSON degerinin ozetteki gorunumu.
///
/// Dizi ve nesne yalnizca **sekil** olarak gorunur; icerikleri hicbir zaman
/// serilestirilmez (modul dokumantasyonu — "kritik kural").
fn render_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Number(number) => {
            truncate_chars(&number.to_string(), MAX_ARGUMENT_VALUE_CHARS)
        }
        serde_json::Value::String(text) => {
            truncate_chars(&collapse_whitespace(text), MAX_ARGUMENT_VALUE_CHARS)
        }
        serde_json::Value::Array(items) => format!("[{} oge]", items.len()),
        serde_json::Value::Object(fields) => format!("{{{} alan}}", fields.len()),
    }
}

/// Ham argumanlardan audit ozeti uretir.
///
/// @returns `None` — cagri argumansiz sayilir (`null`, bos nesne ya da hic
/// gonderilmemis). Bos bir metin yazmak yerine NULL: "arguman yoktu" ile
/// "arguman vardi ama ozetlenemedi" ayni gorunmemeli.
pub fn summarize_arguments(arguments: Option<&serde_json::Value>) -> Option<String> {
    let arguments = arguments?;

    let rendered = match arguments {
        serde_json::Value::Null => return None,
        serde_json::Value::Object(fields) if fields.is_empty() => return None,
        serde_json::Value::Object(fields) => {
            // Alfabetik sira: cagirmanin anahtar sirasi bir bilgi tasimasin ve
            // ayni cagri her zaman ayni ozeti uretsin (test edilebilirlik).
            let mut keys: Vec<&String> = fields.keys().collect();
            keys.sort_unstable();

            let shown = keys.len().min(MAX_ARGUMENT_KEYS);
            let mut parts: Vec<String> = keys
                .iter()
                .take(shown)
                .map(|key| {
                    // Deger, anahtarin **kendisiyle** okunur; gosterim icin
                    // kirpilmis ad yalnizca metne girer.
                    let value = fields
                        .get(key.as_str())
                        .map_or_else(|| "null".to_owned(), render_value);
                    let label = truncate_chars(&collapse_whitespace(key), MAX_ARGUMENT_VALUE_CHARS);
                    format!("{label}={value}")
                })
                .collect();
            if keys.len() > shown {
                parts.push(format!("+{} alan", keys.len() - shown));
            }
            parts.join(", ")
        }
        // Nesne olmayan bir govde (dizi, metin, sayi): tek deger olarak ozetlenir.
        other => render_value(other),
    };

    let redacted = redaction::redact_sensitive_text(&rendered);
    let clamped = truncate_chars(&redacted, MAX_ARGUMENT_SUMMARY_CHARS);
    (!clamped.is_empty()).then_some(clamped)
}

/// Sonuc ozetini saklanabilir hale getirir: tek satir, redakte, tavana kirpilmis.
///
/// Sonuc ozeti de redaksiyondan gecer: bir tool hata mesaji bir token ya da
/// parola tasiyabilir ve bu metin **kalici** olarak saklanacak
/// (`redaction::redact_sensitive_text` bas yorumu).
fn normalize_result_summary(summary: Option<&str>) -> Option<String> {
    let text = collapse_whitespace(summary?);
    if text.is_empty() {
        return None;
    }
    let redacted = redaction::redact_sensitive_text(&text);
    let clamped = truncate_chars(&redacted, MAX_RESULT_SUMMARY_CHARS);
    (!clamped.is_empty()).then_some(clamped)
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

/// Bir tool cagrisini audit defterine yazar.
///
/// # Oturum bagi kopmus olabilir
///
/// `session_id` verildiyse ama o oturum kaydi artik yoksa (kullanici konusma
/// gecmisini bu arada sildi), satir `session_id = NULL` ile yazilir ve durum
/// yerel log'a duser. Alternatif — FK ihlalinin tum yazmayi dusurmesi — audit
/// kaydinin **tamamen** kaybolmasi demek olurdu; kopan bir bag, kayip bir
/// denetim satirindan iyidir.
pub fn record(
    db: &AsunaDb,
    input: &ToolEventInput,
    now: &str,
) -> Result<ToolEventRecord, StoreError> {
    let tool_name = input.tool_name.trim();
    if tool_name.is_empty() {
        return Err(StoreError::invalid("`toolName` bos birakilamaz"));
    }
    if tool_name.chars().count() > MAX_TOOL_NAME_CHARS {
        return Err(StoreError::invalid(
            "`toolName` en fazla 64 karakter olabilir",
        ));
    }
    if matches!(input.session_id, Some(id) if id <= 0) {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }
    if !clock::is_utc_iso8601(now) {
        return Err(StoreError::invalid(
            "`now` UTC ISO-8601 olmali (orn. 2026-08-25T10:00:00Z)",
        ));
    }

    let arguments_redacted = summarize_arguments(input.arguments.as_ref());
    let result_summary = normalize_result_summary(input.result_summary.as_deref());
    let tool_name = tool_name.to_owned();

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;

            // Oturum hala var mi? Yoksa bagi NULL'a cek (bkz. fn dokumantasyonu).
            let session_id = match input.session_id {
                None => None,
                Some(id) => transaction
                    .query_row(
                        "SELECT id FROM sessions WHERE id = ?1",
                        params![id],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?,
            };
            if input.session_id.is_some() && session_id.is_none() {
                eprintln!(
                    "[asuna] Tool audit kaydi oturum bagi olmadan yazildi: oturum kaydi \
                     bulunamadi (silinmis olabilir)."
                );
            }

            transaction.execute(
                "INSERT INTO tool_events
                   (session_id, tool_name, risk_level, arguments_redacted,
                    approval_state, result_summary, created_at, outcome)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    session_id,
                    tool_name,
                    input.risk_level,
                    arguments_redacted,
                    input.approval_state,
                    result_summary,
                    now,
                    input.outcome,
                ],
            )?;
            let id = transaction.last_insert_rowid();
            let record = load(&transaction, id)?;
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "record_tool_event"))?;

    record.ok_or(StoreError::NotFound)
}

/// En yeni audit kayitlarini dondurur; `session_id` verilirse o oturuma filtreler.
///
/// Siralama `created_at DESC, id DESC`: zaman damgasi saniye hassasiyetinde
/// (bkz. [`clock`]), ayni saniyede yazilan iki cagrinin sirasi `id` ile cozulur.
/// Bir tool cagrisi genellikle saniyeler surer, yani bu ayrim gercekten gerekli.
pub fn list_recent(
    db: &AsunaDb,
    session_id: Option<i64>,
    limit: u32,
) -> Result<ToolEventPage, StoreError> {
    let limit = limit.clamp(1, MAX_TOOL_EVENT_LIST_LIMIT);

    // Iki ayri **sabit** sorgu; filtre metinle birlestirilmiyor. `?N IS NULL OR`
    // hilesi tek sorguya indirirdi ama `idx_tool_events_session_id`'yi
    // kullanilamaz hale getirirdi.
    const SELECT_ALL: &str = "SELECT id, session_id, tool_name, risk_level, arguments_redacted,
                                     approval_state, result_summary, created_at, outcome
                                FROM tool_events
                               ORDER BY created_at DESC, id DESC
                               LIMIT ?1";
    const SELECT_FOR_SESSION: &str =
        "SELECT id, session_id, tool_name, risk_level, arguments_redacted,
                                             approval_state, result_summary, created_at, outcome
                                        FROM tool_events
                                       WHERE session_id = ?2
                                       ORDER BY created_at DESC, id DESC
                                       LIMIT ?1";

    db.with_connection(|connection| {
        let total: i64 = match session_id {
            None => {
                connection.query_row("SELECT COUNT(*) FROM tool_events", [], |row| row.get(0))?
            }
            Some(id) => connection.query_row(
                "SELECT COUNT(*) FROM tool_events WHERE session_id = ?1",
                params![id],
                |row| row.get(0),
            )?,
        };

        let mut statement = connection.prepare(match session_id {
            None => SELECT_ALL,
            Some(_) => SELECT_FOR_SESSION,
        })?;
        let rows = match session_id {
            None => statement.query_map(params![limit], ToolEventRecord::from_row)?,
            Some(id) => statement.query_map(params![limit, id], ToolEventRecord::from_row)?,
        };
        let events = rows.collect::<rusqlite::Result<Vec<ToolEventRecord>>>()?;

        Ok(ToolEventPage {
            events,
            limit,
            limit_max: MAX_TOOL_EVENT_LIST_LIMIT,
            total: u32::try_from(total).unwrap_or(u32::MAX),
        })
    })
    .map_err(|error| StoreError::storage(error, "tool_event_list"))
}

fn load(connection: &rusqlite::Connection, id: i64) -> rusqlite::Result<Option<ToolEventRecord>> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM tool_events WHERE id = ?1",
                ToolEventRecord::select_columns()
            ),
            params![id],
            ToolEventRecord::from_row,
        )
        .optional()
}

// ---------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------

/// Bir tool cagrisini audit defterine yazar (ASU-050).
///
/// Cagiran taraf ASU-047'nin tool runner'idir ve **her** cagri icin bunu
/// cagirir: onaylanan, reddedilen, hata veren, zaman asimina ugrayan.
///
/// # Sozlesme (ASU-047 bunu kullanacak)
///
/// - Basari: `{ status: "recorded", event: {...} }`.
/// - Kalici hafiza kapali: `{ status: "skipped", reason: "memory-disabled" }`.
///   Hata **degil** — kullanicinin karari. Sonuc: hafiza kapaliyken kalici bir
///   audit izi de tutulmaz; tool cagrilarinin gorunurlugu o durumda canli UI'a
///   (ASU-054) kalir.
/// - Yazma basarisiz: tipli `StoreError`. Bu hata **tool sonucunu degistirmez**
///   ama yutulmaz: tam zincir yerel log'a duser ve hata cagirana doner
///   (`src/asuna/tools/audit.ts` bunu `{ status: "failed" }`e cevirir).
///
/// # Neden yazma yuzeyi renderer'a acildi
///
/// ADR-005 "Etkiler" bolumu "renderer'in `tool_events`'e yazma yolu yoktur"
/// diyordu; o cumle tool yurutmesinin de Rust tarafinda olacagi varsayimiyla
/// yazilmis. Gerceklesen mimari farkli: tool runner renderer'da yasiyor
/// (Realtime SDK oradan konusuyor), dolayisiyla "her cagri kaydedilir"
/// kriterinin tek yolu dar bir yazma komutu. Yuzeyin darligi telafi ediyor:
/// tek yon (append), ham SQL yok, ozetleme ve redaksiyon host tarafinda, silme
/// ve guncelleme komutu yok. Not `asuna-docs/DECISIONS.md`'ye tasinacak.
#[tauri::command]
pub fn record_tool_event(
    state: State<'_, DbState>,
    privacy: State<'_, Arc<PrivacyState>>,
    input: ToolEventInput,
) -> Result<ToolEventWriteResult, StoreError> {
    if !privacy.memory_enabled() {
        return Ok(ToolEventWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    }
    let Some(db) = database(&state)? else {
        return Ok(ToolEventWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    let event = record(db, &input, &clock::now_utc())?;
    Ok(ToolEventWriteResult::Recorded {
        event: Box::new(event),
    })
}

/// Audit defterini listeler (salt okuma, ASU-050).
///
/// Hafiza kapaliyken **bos sayfa** doner (hata degil) — `session_list` ile ayni
/// sozlesme. Bozuk oldugunda tipli hata doner: "audit yok" ile "audit'e
/// bakamadim" ayni cevaplar degil (PROJECT.md Bolum 30).
#[tauri::command]
pub fn tool_event_list(
    state: State<'_, DbState>,
    query: Option<ToolEventListQuery>,
) -> Result<ToolEventPage, StoreError> {
    let query = query.unwrap_or_default();
    let limit = query
        .limit
        .unwrap_or(DEFAULT_TOOL_EVENT_LIST_LIMIT)
        .clamp(1, MAX_TOOL_EVENT_LIST_LIMIT);

    if matches!(query.session_id, Some(id) if id <= 0) {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }

    let Some(db) = database(&state)? else {
        return Ok(ToolEventPage {
            events: Vec::new(),
            limit,
            limit_max: MAX_TOOL_EVENT_LIST_LIMIT,
            total: 0,
        });
    };
    list_recent(db, query.session_id, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-08-25T10:00:00Z";

    fn fresh_db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB acilmali")
    }

    fn open_session(db: &AsunaDb) -> i64 {
        db.with_connection(|connection| {
            connection.execute(
                "INSERT INTO sessions (started_at, model, created_at)
                 VALUES (?1, 'gpt-realtime-2.1', ?1)",
                params![NOW],
            )?;
            Ok(connection.last_insert_rowid())
        })
        .expect("oturum acilmali")
    }

    fn input(
        tool_name: &str,
        risk_level: ToolRiskLevel,
        approval_state: ToolApprovalState,
    ) -> ToolEventInput {
        ToolEventInput {
            session_id: None,
            tool_name: tool_name.to_owned(),
            risk_level,
            arguments: None,
            approval_state,
            result_summary: None,
            outcome: None,
        }
    }

    // --- Arguman ozeti + redaksiyon -----------------------------------------

    #[test]
    fn arguments_are_summarized_as_sorted_key_value_pairs() {
        let summary = summarize_arguments(Some(&serde_json::json!({
            "path": "README.md",
            "maxBytes": 4096,
            "follow": false,
        })))
        .expect("ozet uretilmeli");

        // Alfabetik: ayni cagri her zaman ayni ozeti uretir.
        assert_eq!(summary, "follow=false, maxBytes=4096, path=README.md");
    }

    #[test]
    fn a_call_without_arguments_has_no_summary() {
        assert_eq!(summarize_arguments(None), None);
        assert_eq!(summarize_arguments(Some(&serde_json::json!(null))), None);
        assert_eq!(summarize_arguments(Some(&serde_json::json!({}))), None);
    }

    /// **ASU-050 kabul kriteri**: secret'lar audit'e girmiyor.
    #[test]
    fn secret_looking_argument_values_are_masked() {
        let summary = summarize_arguments(Some(&serde_json::json!({
            "apiKey": "sk-proj-BU-DEGER-SIZMAMALI",
            "password": "hunter2",
            "note": "token=abc123def",
        })))
        .expect("ozet uretilmeli");

        assert!(
            !summary.contains("BU-DEGER-SIZMAMALI"),
            "kalici anahtar sizdi: {summary}"
        );
        assert!(!summary.contains("hunter2"), "parola sizdi: {summary}");
        assert!(!summary.contains("abc123def"), "token sizdi: {summary}");
        // Anahtarin **adi** kalir: "password=<redacted>" okunabilir bir denetim
        // satiridir, tek basina "<redacted>" neyin maskelendigini soylemez.
        assert!(summary.starts_with("apiKey=<redacted>"), "ozet: {summary}");
        assert!(summary.contains("password=<redacted>"), "ozet: {summary}");
        assert!(summary.contains("note=token=<redacted>"), "ozet: {summary}");
    }

    /// **ASU-050 kabul kriteri**: dosya icerigi audit'e girmiyor.
    ///
    /// Iki ayri mekanizma olculuyor: ic ice yapilar yalnizca **sekil** olarak
    /// gorunur (icerik hic serilestirilmez) ve ust duzey bir metin deger
    /// [`MAX_ARGUMENT_VALUE_CHARS`] karakterde kirpilir.
    #[test]
    fn file_contents_cannot_enter_the_audit_log() {
        let secret_content = "OPENAI_API_KEY=cok-gizli-deger\nsatir 2\nsatir 3\n".repeat(50);

        // 1) Ic ice: yalnizca sekil. Uzunluktan bagimsiz — icerik hicbir zaman
        // serilestirilmedigi icin buraya bir dosya "sigamaz", giremez.
        let nested = summarize_arguments(Some(&serde_json::json!({
            "file": { "path": "/tmp/.env", "content": secret_content.clone() },
            "lines": [secret_content.clone(), secret_content.clone()],
        })))
        .expect("ozet uretilmeli");
        assert_eq!(nested, "file={2 alan}, lines=[2 oge]");
        assert!(!nested.contains("cok-gizli-deger"));

        // 2) Ust duzey metin: [`MAX_ARGUMENT_VALUE_CHARS`] karakterde kirpilir
        // ve kirpildigi gorunur. Icerik bilerek "masum": olculen sey kirpma
        // mekanizmasi, redaksiyon degil.
        let plain = "satir bir, satir iki, satir uc; ".repeat(50);
        let flat = summarize_arguments(Some(&serde_json::json!({ "content": plain })))
            .expect("ozet uretilmeli");
        assert!(flat.ends_with(TRUNCATION_MARKER), "ozet: {flat}");
        assert_eq!(
            flat.chars().count(),
            "content=".len() + MAX_ARGUMENT_VALUE_CHARS,
            "ozet: {flat}"
        );

        // 3) Ikisi birlikte: uzun bir dosya icerigi hem kirpilir hem redakte
        // edilir; hicbir satiri audit'e ulasmaz.
        let both = summarize_arguments(Some(&serde_json::json!({ "content": secret_content })))
            .expect("ozet uretilmeli");
        assert!(!both.contains("cok-gizli-deger"), "ozet: {both}");
        assert!(
            both.chars().count() <= MAX_ARGUMENT_SUMMARY_CHARS,
            "ozet: {both}"
        );
    }

    /// Cok satirli bir deger tek satira indirilir: audit satiri tek satirdir ve
    /// bu, icerik dokmeyi yapisal olarak zorlastiran ikinci kuraldir.
    #[test]
    fn argument_values_are_collapsed_to_a_single_line() {
        let summary = summarize_arguments(Some(&serde_json::json!({
            "message": "ilk satir\n\tikinci   satir",
        })))
        .expect("ozet uretilmeli");
        assert_eq!(summary, "message=ilk satir ikinci satir");
        assert!(!summary.contains('\n'));
    }

    #[test]
    fn too_many_argument_keys_are_counted_not_dumped() {
        let mut fields = serde_json::Map::new();
        for index in 0..30 {
            fields.insert(format!("k{index:02}"), serde_json::json!(index));
        }
        let summary =
            summarize_arguments(Some(&serde_json::Value::Object(fields))).expect("ozet uretilmeli");

        assert!(summary.contains("k00=0"), "ozet: {summary}");
        assert!(summary.ends_with("+18 alan"), "ozet: {summary}");
        assert!(!summary.contains("k29"), "ozet: {summary}");
    }

    /// Nesne olmayan bir govde de ozetlenir; sessizce dusurulmez.
    #[test]
    fn non_object_argument_bodies_are_still_summarized() {
        assert_eq!(
            summarize_arguments(Some(&serde_json::json!(["a", "b", "c"]))).as_deref(),
            Some("[3 oge]")
        );
        assert_eq!(
            summarize_arguments(Some(&serde_json::json!("README.md"))).as_deref(),
            Some("README.md")
        );
        assert_eq!(
            summarize_arguments(Some(&serde_json::json!(42))).as_deref(),
            Some("42")
        );
    }

    /// Ozet tavani asamaz — semadaki CHECK bunu ayrica zorluyor.
    #[test]
    fn the_summary_never_exceeds_the_schema_ceiling() {
        let mut fields = serde_json::Map::new();
        for index in 0..MAX_ARGUMENT_KEYS {
            fields.insert(
                format!("uzun_anahtar_{index}"),
                serde_json::json!("x".repeat(200)),
            );
        }
        let summary =
            summarize_arguments(Some(&serde_json::Value::Object(fields))).expect("ozet uretilmeli");
        assert!(
            summary.chars().count() <= MAX_ARGUMENT_SUMMARY_CHARS,
            "uzunluk: {}",
            summary.chars().count()
        );
    }

    #[test]
    fn result_summaries_are_redacted_and_clamped() {
        let normalized = normalize_result_summary(Some(
            "Dosya okundu.\nAnahtar: sk-proj-SIZMAMALI kullanildi.",
        ))
        .expect("ozet uretilmeli");
        assert!(!normalized.contains("SIZMAMALI"), "ozet: {normalized}");
        assert!(!normalized.contains('\n'));

        let long = normalize_result_summary(Some(&"y".repeat(2_000))).expect("ozet");
        assert!(long.chars().count() <= MAX_RESULT_SUMMARY_CHARS);
        assert!(long.ends_with(TRUNCATION_MARKER));

        assert_eq!(normalize_result_summary(None), None);
        assert_eq!(normalize_result_summary(Some("   ")), None);
    }

    // --- Yazma ---------------------------------------------------------------

    /// **ASU-050 kabul kriteri**: reddedilen, zaman asimina ugrayan ve onaya hic
    /// gitmeyen cagrilar da yaziliyor — audit bir basari vitrini degil.
    #[test]
    fn denied_timed_out_and_never_asked_calls_are_all_recorded() {
        let db = fresh_db();

        for state in ToolApprovalState::ALL {
            record(
                &db,
                &input("open_project", ToolRiskLevel::LowRisk, state),
                NOW,
            )
            .unwrap_or_else(|error| panic!("`{state:?}` yazilmali: {error}"));
        }

        let page = list_recent(&db, None, 100).expect("liste okunmali");
        assert_eq!(page.total, ToolApprovalState::ALL.len() as u32);

        let mut recorded: Vec<ToolApprovalState> = page
            .events
            .iter()
            .map(|event| event.approval_state)
            .collect();
        recorded.sort_by_key(|state| state.as_str());
        let mut expected = ToolApprovalState::ALL.to_vec();
        expected.sort_by_key(|state| state.as_str());
        assert_eq!(recorded, expected);
    }

    #[test]
    fn a_recorded_event_carries_the_session_link() {
        let db = fresh_db();
        let session_id = open_session(&db);

        let event = record(
            &db,
            &ToolEventInput {
                session_id: Some(session_id),
                arguments: Some(serde_json::json!({ "projectId": "asuna" })),
                result_summary: Some("Proje VS Code ile acildi.".to_owned()),
                ..input(
                    "open_project",
                    ToolRiskLevel::LowRisk,
                    ToolApprovalState::Approved,
                )
            },
            NOW,
        )
        .expect("yazilmali");

        assert_eq!(event.session_id, Some(session_id));
        assert_eq!(event.arguments_redacted.as_deref(), Some("projectId=asuna"));
        assert_eq!(
            event.result_summary.as_deref(),
            Some("Proje VS Code ile acildi.")
        );
        assert_eq!(event.created_at, NOW);
    }

    // --- `outcome` ekseni (ASU-051) -----------------------------------------

    /// Onay durumu ile sonuc **birlikte** yazilir ve birbirinden bagimsizdir.
    /// Kritik kombinasyon: kullanici izin verdi, is calisti ve **patladi**.
    #[test]
    fn an_approved_call_can_still_be_recorded_as_failed() {
        let db = fresh_db();

        let event = record(
            &db,
            &ToolEventInput {
                result_summary: Some("Editor komutu bulunamadi.".to_owned()),
                outcome: Some(ToolOutcome::Failed),
                ..input(
                    "open_project",
                    ToolRiskLevel::LowRisk,
                    ToolApprovalState::Approved,
                )
            },
            NOW,
        )
        .expect("yazilmali");

        assert_eq!(event.approval_state, ToolApprovalState::Approved);
        assert_eq!(event.outcome, Some(ToolOutcome::Failed));
    }

    /// Reddedilen bir cagri `not_run` ile deftere gecer: yan etki ihtimali yok.
    #[test]
    fn a_denied_call_is_recorded_as_not_run() {
        let db = fresh_db();

        let event = record(
            &db,
            &ToolEventInput {
                outcome: Some(ToolOutcome::NotRun),
                ..input(
                    "open_project",
                    ToolRiskLevel::LowRisk,
                    ToolApprovalState::Denied,
                )
            },
            NOW,
        )
        .expect("reddedilen cagri da yazilmali");

        assert_eq!(event.outcome, Some(ToolOutcome::NotRun));
    }

    /// Cagiran sonucu bildirmezse `NULL` yazilir — sessiz bir `succeeded`
    /// varsayimi denetim defterine olculmemis bir iddia yazardi.
    #[test]
    fn an_unreported_outcome_stays_null_instead_of_defaulting_to_success() {
        let db = fresh_db();

        let event = record(
            &db,
            &input(
                "get_current_project",
                ToolRiskLevel::ReadOnly,
                ToolApprovalState::NotRequired,
            ),
            NOW,
        )
        .expect("yazilmali");

        assert_eq!(event.outcome, None);
    }

    /// Renderer sozlesmesi: `outcome` opsiyonel ama kume disi bir deger serde
    /// sinirinde duser — DB'ye hic dokunulmaz.
    #[test]
    fn an_unknown_outcome_is_rejected_at_the_serde_boundary() {
        let accepted: ToolEventInput = serde_json::from_value(serde_json::json!({
            "toolName": "read_project_file",
            "riskLevel": 0,
            "approvalState": "not_required",
            "outcome": "succeeded",
        }))
        .expect("gecerli girdi kabul edilmeli");
        assert_eq!(accepted.outcome, Some(ToolOutcome::Succeeded));

        for bad in ["basarili", "SUCCEEDED", "skipped", "denied"] {
            assert!(
                serde_json::from_value::<ToolEventInput>(serde_json::json!({
                    "toolName": "read_project_file",
                    "riskLevel": 0,
                    "approvalState": "not_required",
                    "outcome": bad,
                }))
                .is_err(),
                "`{bad}` serde sinirindan gecti"
            );
        }
    }

    /// Sonuc listeleme yolundan da geri geliyor; kolon SELECT listesinden
    /// dusmus olsaydi bu test yakalardi.
    #[test]
    fn the_outcome_survives_the_listing_path() {
        let db = fresh_db();
        record(
            &db,
            &ToolEventInput {
                outcome: Some(ToolOutcome::Succeeded),
                ..input(
                    "read_project_file",
                    ToolRiskLevel::ReadOnly,
                    ToolApprovalState::NotRequired,
                )
            },
            NOW,
        )
        .expect("yazilmali");

        let page = list_recent(&db, None, 10).expect("listelenmeli");
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].outcome, Some(ToolOutcome::Succeeded));

        let json = serde_json::to_value(&page.events[0]).expect("serialize");
        assert_eq!(json["outcome"], "succeeded");
    }

    /// Oturum kaydi bu arada silinmisse audit satiri **yine yazilir**; yalnizca
    /// bagi bos kalir. Alternatif (FK ihlali → yazma dusuyor) denetim satirinin
    /// tamamen kaybolmasi olurdu.
    #[test]
    fn a_missing_session_clears_the_link_instead_of_losing_the_event() {
        let db = fresh_db();

        let event = record(
            &db,
            &ToolEventInput {
                session_id: Some(4_242),
                ..input(
                    "get_current_project",
                    ToolRiskLevel::ReadOnly,
                    ToolApprovalState::NotRequired,
                )
            },
            NOW,
        )
        .expect("audit satiri kaybedilmemeli");

        assert_eq!(event.session_id, None);
        assert_eq!(list_recent(&db, None, 10).expect("liste").total, 1);
    }

    /// **Kabul kriteri**: oturum silinince audit kalir (FK `ON DELETE SET NULL`).
    #[test]
    fn deleting_the_session_keeps_the_audit_row() {
        let db = fresh_db();
        let session_id = open_session(&db);
        record(
            &db,
            &ToolEventInput {
                session_id: Some(session_id),
                ..input(
                    "open_project",
                    ToolRiskLevel::LowRisk,
                    ToolApprovalState::Approved,
                )
            },
            NOW,
        )
        .expect("yazilmali");

        db.with_connection(|connection| {
            connection.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
        })
        .expect("oturum silinmeli");

        let page = list_recent(&db, None, 10).expect("liste okunmali");
        assert_eq!(page.total, 1, "audit satiri oturumla birlikte silinmis");
        assert_eq!(page.events[0].session_id, None);
    }

    #[test]
    fn invalid_input_is_refused_before_touching_the_database() {
        let db = fresh_db();

        for (tool_name, expectation) in [("", "bos ad"), ("   ", "yalnizca bosluk")] {
            let error = record(
                &db,
                &input(
                    tool_name,
                    ToolRiskLevel::ReadOnly,
                    ToolApprovalState::NotRequired,
                ),
                NOW,
            )
            .expect_err(expectation);
            assert!(matches!(error, StoreError::Invalid { .. }), "{error}");
        }

        let error = record(
            &db,
            &input(
                &"t".repeat(65),
                ToolRiskLevel::ReadOnly,
                ToolApprovalState::NotRequired,
            ),
            NOW,
        )
        .expect_err("tavani asan tool adi reddedilmeli");
        assert!(matches!(error, StoreError::Invalid { .. }), "{error}");

        let error = record(
            &db,
            &ToolEventInput {
                session_id: Some(0),
                ..input(
                    "open_project",
                    ToolRiskLevel::LowRisk,
                    ToolApprovalState::Approved,
                )
            },
            NOW,
        )
        .expect_err("sifir oturum kimligi reddedilmeli");
        assert!(matches!(error, StoreError::Invalid { .. }), "{error}");

        let error = record(
            &db,
            &input(
                "open_project",
                ToolRiskLevel::LowRisk,
                ToolApprovalState::Approved,
            ),
            "25/08/2026",
        )
        .expect_err("gecersiz zaman damgasi reddedilmeli");
        assert!(matches!(error, StoreError::Invalid { .. }), "{error}");

        // Hicbiri yazilmadi.
        assert_eq!(list_recent(&db, None, 10).expect("liste").total, 0);
    }

    // --- Okuma ---------------------------------------------------------------

    #[test]
    fn events_are_listed_newest_first_and_can_be_filtered_by_session() {
        let db = fresh_db();
        let first = open_session(&db);
        let second = open_session(&db);

        for (session_id, tool) in [
            (Some(first), "get_current_project"),
            (Some(first), "open_project"),
            (Some(second), "read_project_file"),
            (None, "get_current_project"),
        ] {
            record(
                &db,
                &ToolEventInput {
                    session_id,
                    ..input(
                        tool,
                        ToolRiskLevel::ReadOnly,
                        ToolApprovalState::NotRequired,
                    )
                },
                NOW,
            )
            .expect("yazilmali");
        }

        let all = list_recent(&db, None, 50).expect("liste okunmali");
        assert_eq!(all.total, 4);
        // Ayni saniye: siralama `id DESC` ile cozulur, en yeni once.
        assert_eq!(all.events[0].id, 4);
        assert_eq!(all.events[3].id, 1);

        let filtered = list_recent(&db, Some(first), 50).expect("liste okunmali");
        assert_eq!(filtered.total, 2);
        assert!(filtered
            .events
            .iter()
            .all(|event| event.session_id == Some(first)));

        // Kaydi olmayan bir oturum: bos ama durust.
        let empty = list_recent(&db, Some(9_999), 50).expect("liste okunmali");
        assert_eq!(empty.total, 0);
        assert!(empty.events.is_empty());
    }

    /// Kirpma **gorunur**: `limit` uygulanan degeri, `limitMax` tavani,
    /// `total` depodaki gercek sayiyi soyler.
    #[test]
    fn the_limit_is_clamped_and_the_clamping_is_visible() {
        let db = fresh_db();
        for _ in 0..3 {
            record(
                &db,
                &input(
                    "get_current_project",
                    ToolRiskLevel::ReadOnly,
                    ToolApprovalState::NotRequired,
                ),
                NOW,
            )
            .expect("yazilmali");
        }

        let page = list_recent(&db, None, 1).expect("liste okunmali");
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.limit, 1);
        assert_eq!(page.limit_max, MAX_TOOL_EVENT_LIST_LIMIT);
        assert_eq!(page.total, 3, "tavana carpildigi gorunmeli");

        let page = list_recent(&db, None, 10_000).expect("liste okunmali");
        assert_eq!(page.limit, MAX_TOOL_EVENT_LIST_LIMIT);

        let page = list_recent(&db, None, 0).expect("liste okunmali");
        assert_eq!(page.limit, 1, "sifir limit bos sayfa uretmemeli");
    }

    // --- Append-only ---------------------------------------------------------

    /// **ASU-050 kabul kriteri**: audit kayitlari uygulamadan silinemiyor.
    ///
    /// Bu test kaynak metnini okur: `delete` / `update` / `purge` gibi bir
    /// fonksiyon eklendigi anda duser. Sema silmeyi engellemiyor (engelleyemez);
    /// kilit **kod yuzeyinde** ve burada kontrol ediliyor. IPC tarafi
    /// `commands.rs` ve `acl_regression.rs` testleriyle ayrica kilitli.
    #[test]
    fn this_module_exposes_no_delete_or_update_path() {
        let source = include_str!("tool_event_repository.rs");

        for forbidden in [
            "DELETE FROM tool_events",
            "UPDATE tool_events",
            "DROP TABLE tool_events",
        ] {
            // Test modulu bu metinleri yalnizca **yasak liste** olarak iceriyor;
            // gercek bir SQL olarak gecmemeli.
            let occurrences = source.matches(forbidden).count();
            assert_eq!(
                occurrences, 1,
                "`{forbidden}` bu modulde gecmemeli (append-only)"
            );
        }
    }
}

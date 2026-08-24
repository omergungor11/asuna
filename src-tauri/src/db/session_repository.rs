//! `sessions` kaydi: acilis, kapanis, yarim kalan oturumlarin kurtarilmasi
//! (ASU-032).
//!
//! # Sozlesme
//!
//! - Oturum **modelini renderer secmez**: `model` her zaman `AsunaConfig`'ten
//!   (`ASUNA_REALTIME_MODEL`) gelir. Aksi halde webview'den gelen bir metin
//!   dogrudan fatura kaydina yazilirdi.
//! - Kapanis yolu **hicbir zaman** oturumu ayakta birakmaz: transcript yazimi
//!   ya da usage okumasi basarisiz olsa bile `ended_at` yazilir. Ozet uretimi de
//!   ayni disiplinle ASU-033'te eklenecek.
//! - Hafiza kapaliyken (`ASUNA_MEMORY_ENABLED=false`) hicbir oturum kaydi
//!   olusmaz; komut `skipped` doner ve renderer oturum kimligi almaz — sonraki
//!   `session_finalize` cagrisi da yapilmaz.
//!
//! # Yarim kalan oturum
//!
//! Cokme/kill sonrasi `ended_at` NULL kalir. Acilista bu kayitlar kapatilir
//! (`idx_sessions_open` kismi index'i tam bu sorgu icin var). `ended_at`
//! **`started_at`'e** esitlenir: gercek bitis zamani bilinmiyor ve "simdi"
//! yazmak 20 saatlik sahte bir oturum (ve sahte bir maliyet penceresi)
//! uretirdi. Sifir sure "bilmiyoruz" demenin en az yaniltici yolu; nedeni de
//! `summary` alanina insan diliyle yaziliyor.
//!
//! Alternatif bir `end_reason` kolonu daha temiz olurdu; yeni bir migration
//! (+ uc katmanli tip aynasi) bu task'in kapsamini asiyor — ASU-033 token/
//! maliyet kolonlarina dokunurken birlikte degerlendirilecek.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::clock;
use super::model::SessionRecord;
use super::store_error::{database, StoreError, StoreSkipReason};
use super::transcript::{self, TranscriptLine};
use super::{AsunaDb, DbState};
use crate::config::AsunaConfig;

/// Yarim kalan oturuma yazilan aciklama.
///
/// `summary` burada bir **durum bayragi gibi** kullaniliyor; bilincli bir odun
/// (bkz. modul dokumantasyonu). Kullaniciya gosterilebilir bir cumle olmasi da
/// bu yuzden onemli: oturum listesinde "0 saniye" goren kisi nedenini gorur.
pub const ABANDONED_SESSION_SUMMARY: &str =
    "Oturum beklenmedik sekilde kapandi (uygulama yeniden acilirken kapatildi).";

/// Bir oturumda beklenebilecek azami replik sayisi.
///
/// Renderer'in gonderdigi dokum diske yazilacak; sinirsiz bir dizi hem IPC
/// mesajini hem dosyayi sisirir. Ust sinir asilirsa **son** replikler tutulur
/// (yeni olan daha degerli).
pub const MAX_TRANSCRIPT_LINES: usize = 2_000;

// ---------------------------------------------------------------------------
// Girdi / cikti tipleri
// ---------------------------------------------------------------------------

/// Realtime oturumunun token kullanimi (ASU-013 `RealtimeUsageSnapshot` aynasi).
///
/// Skaler alanlar kolonlara, tamami `usage_json`'a yazilir. Ayrintili kirilimin
/// anahtarlari runtime'da dogrulanmadigi icin (memory.md T5) kirilim ham JSON
/// olarak saklanir — uydurulmus kolon acilmaz.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionUsage {
    #[serde(default)]
    pub requests: Option<i64>,
    #[serde(default)]
    pub input_tokens: Option<i64>,
    #[serde(default)]
    pub output_tokens: Option<i64>,
    #[serde(default)]
    pub total_tokens: Option<i64>,
    #[serde(default)]
    pub input_token_details: Vec<serde_json::Value>,
    #[serde(default)]
    pub output_token_details: Vec<serde_json::Value>,
}

/// Oturum kapanis girdisi.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionFinalizeInput {
    #[serde(default)]
    pub usage: Option<SessionUsage>,
    /// Yalnizca `ASUNA_TRANSCRIPT_STORAGE=true` iken diske yazilir; aksi halde
    /// bellekte kalir ve atilir.
    #[serde(default)]
    pub transcript: Vec<TranscriptLine>,
}

/// Oturum yazma sonucu. `Skipped` = hafiza kapali (hata degil).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum SessionWriteResult {
    Recorded { session: Box<SessionRecord> },
    Skipped { reason: StoreSkipReason },
}

/// Kapanis sirasinda dogrulanmis/olculmus degerler.
struct FinalizeValues {
    ended_at: String,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    usage_json: Option<String>,
    transcript_path: Option<String>,
}

fn token_count(field: &'static str, value: Option<i64>) -> Result<Option<i64>, StoreError> {
    match value {
        Some(count) if count < 0 => Err(StoreError::invalid(format!("`{field}` negatif olamaz"))),
        other => Ok(other),
    }
}

impl SessionUsage {
    /// Ham kirilimi JSON'a cevirir. Serilestirme basarisiz olamaz (tipler
    /// zaten JSON'dan geldi) ama yine de `Result` ile tasiniyor.
    fn to_json(&self) -> Result<String, StoreError> {
        serde_json::to_string(self).map_err(|_| StoreError::invalid("`usage` JSON'a cevrilemedi"))
    }
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

/// Yeni oturum kaydi acar (`started_at` + `model`).
pub fn start(
    db: &AsunaDb,
    model: &str,
    project_id: Option<&str>,
    now: &str,
) -> Result<SessionRecord, StoreError> {
    let model = model.trim();
    if model.is_empty() {
        return Err(StoreError::invalid("`model` bos birakilamaz"));
    }
    let project_id = project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let now = validated_now(now)?;

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT INTO sessions (started_at, project_id, model, created_at)
                 VALUES (?1, ?2, ?3, ?1)",
                params![now, project_id, model],
            )?;
            let id = transaction.last_insert_rowid();
            let record = load(&transaction, id)?;
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "session_start"))?;

    record.ok_or(StoreError::NotFound)
}

/// Oturumu kapatir: `ended_at`, token/maliyet metadata ve varsa transcript yolu.
///
/// `transcript_path` cagiran tarafindan verilir — dosyayi yazma karari
/// (`ASUNA_TRANSCRIPT_STORAGE`) komut katmanindadir, repository dosya sistemine
/// dokunmaz.
pub fn finalize(
    db: &AsunaDb,
    id: i64,
    input: &SessionFinalizeInput,
    transcript_path: Option<&str>,
    now: &str,
) -> Result<SessionRecord, StoreError> {
    if id <= 0 {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }
    let now = validated_now(now)?;

    let usage = input.usage.as_ref();
    let values = FinalizeValues {
        ended_at: now,
        input_tokens: token_count("inputTokens", usage.and_then(|usage| usage.input_tokens))?,
        output_tokens: token_count("outputTokens", usage.and_then(|usage| usage.output_tokens))?,
        total_tokens: token_count("totalTokens", usage.and_then(|usage| usage.total_tokens))?,
        usage_json: usage.map(SessionUsage::to_json).transpose()?,
        transcript_path: transcript_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    };

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;

            // Saat geriye kaymissa `ended_at < started_at` semadaki CHECK'e
            // takilirdi. Oturumu kaybetmektense sifir sure yazmak dogru:
            // kapanis **her kosulda** tamamlanmali.
            let started_at: Option<String> = transaction
                .query_row(
                    "SELECT started_at FROM sessions WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;

            let Some(started_at) = started_at else {
                transaction.commit()?;
                return Ok(None);
            };
            let ended_at = if values.ended_at < started_at {
                started_at
            } else {
                values.ended_at.clone()
            };

            transaction.execute(
                "UPDATE sessions
                    SET ended_at = ?1,
                        input_tokens = ?2,
                        output_tokens = ?3,
                        total_tokens = ?4,
                        usage_json = ?5,
                        transcript_path = ?6
                  WHERE id = ?7",
                params![
                    ended_at,
                    values.input_tokens,
                    values.output_tokens,
                    values.total_tokens,
                    values.usage_json,
                    values.transcript_path,
                    id,
                ],
            )?;

            let record = load(&transaction, id)?;
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "session_finalize"))?;

    record.ok_or(StoreError::NotFound)
}

/// Yarim kalan oturumlari kapatir. Acilista bir kez cagrilir.
///
/// @returns kapatilan oturum sayisi.
pub fn close_abandoned(db: &AsunaDb) -> Result<usize, StoreError> {
    db.with_connection(|connection| {
        connection.execute(
            "UPDATE sessions
                SET ended_at = started_at,
                    summary = COALESCE(summary, ?1)
              WHERE ended_at IS NULL",
            params![ABANDONED_SESSION_SUMMARY],
        )
    })
    .map_err(|error| StoreError::storage(error, "session_recovery"))
}

/// Tek oturumu kimligiyle okur.
pub fn get_by_id(db: &AsunaDb, id: i64) -> Result<Option<SessionRecord>, StoreError> {
    db.with_connection(|connection| load(connection, id))
        .map_err(|error| StoreError::storage(error, "session_get"))
}

fn load(connection: &rusqlite::Connection, id: i64) -> rusqlite::Result<Option<SessionRecord>> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM sessions WHERE id = ?1",
                SessionRecord::select_columns()
            ),
            params![id],
            SessionRecord::from_row,
        )
        .optional()
}

fn validated_now(now: &str) -> Result<String, StoreError> {
    if !clock::is_utc_iso8601(now) {
        return Err(StoreError::invalid(
            "`now` UTC ISO-8601 olmali (orn. 2026-08-25T10:00:00Z)",
        ));
    }
    Ok(now.to_owned())
}

/// Dokumu ust sinira kirpar (son replikler kalir).
fn clamp_transcript(lines: &[TranscriptLine]) -> &[TranscriptLine] {
    if lines.len() <= MAX_TRANSCRIPT_LINES {
        lines
    } else {
        &lines[lines.len() - MAX_TRANSCRIPT_LINES..]
    }
}

// ---------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------

/// Oturum kaydi acar. `model` config'ten gelir; renderer secemez.
#[tauri::command]
pub fn session_start(
    state: State<'_, DbState>,
    config: State<'_, AsunaConfig>,
    project_id: Option<String>,
) -> Result<SessionWriteResult, StoreError> {
    let Some(db) = database(&state)? else {
        return Ok(SessionWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    let session = start(
        db,
        &config.realtime_model,
        project_id.as_deref(),
        &clock::now_utc(),
    )?;
    Ok(SessionWriteResult::Recorded {
        session: Box::new(session),
    })
}

/// Oturumu kapatir; transcript yalnizca ayar aciksa diske yazilir.
///
/// Transcript yazimi basarisiz olursa oturum yine de kapanir: `transcript_path`
/// bos kalir ve hata **yerel log'a** yazilir (sessiz yutma yok, ama kapanis
/// engellenmez — acik kalan bir oturum kaydi fatura ve hafiza acisindan daha
/// pahalidir).
#[tauri::command]
pub fn session_finalize<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, DbState>,
    config: State<'_, AsunaConfig>,
    session_id: i64,
    input: Option<SessionFinalizeInput>,
) -> Result<SessionWriteResult, StoreError> {
    let Some(db) = database(&state)? else {
        return Ok(SessionWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    let input = input.unwrap_or_default();
    let transcript_path = persist_transcript(&app, &config, session_id, &input);

    let session = finalize(
        db,
        session_id,
        &input,
        transcript_path.as_deref(),
        &clock::now_utc(),
    )?;
    Ok(SessionWriteResult::Recorded {
        session: Box::new(session),
    })
}

/// Transcript'i (ayar aciksa) diske yazar; hata halinde `None` doner ve log'lar.
fn persist_transcript<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    config: &AsunaConfig,
    session_id: i64,
    input: &SessionFinalizeInput,
) -> Option<String> {
    if !config.transcript_storage {
        // GIZLILIK: kapaliyken dizin yolu bile cozulmez.
        return None;
    }

    let directory = match transcript::transcript_dir(app) {
        Ok(directory) => directory,
        Err(error) => {
            eprintln!(
                "[asuna] Transcript dizini cozulemedi: {}",
                super::describe_error_chain(&error)
            );
            return None;
        }
    };

    match transcript::persist_if_enabled(
        true,
        &directory,
        session_id,
        clamp_transcript(&input.transcript),
    ) {
        Ok(path) => path.map(|path| path.to_string_lossy().into_owned()),
        Err(error) => {
            // Yol log'a girmiyor: kullanicinin dizin yapisi hata metnine dusmesin.
            eprintln!("[asuna] Transcript yazilamadi: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::store_error::StoreErrorCode;
    use crate::db::transcript::TranscriptRole;

    const MODEL: &str = "gpt-realtime-2.1";
    const START: &str = "2026-08-25T10:00:00Z";
    const END: &str = "2026-08-25T10:04:00Z";

    fn fresh_db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB acilmali")
    }

    fn usage() -> SessionUsage {
        SessionUsage {
            requests: Some(3),
            input_tokens: Some(1_200),
            output_tokens: Some(800),
            total_tokens: Some(2_000),
            input_token_details: vec![serde_json::json!({ "audio_tokens": 900 })],
            output_token_details: vec![serde_json::json!({ "audio_tokens": 700 })],
        }
    }

    #[test]
    fn starts_a_session_with_the_model_and_no_end_time() {
        let db = fresh_db();
        let session = start(&db, MODEL, Some("asuna"), START).expect("oturum acilmali");

        assert!(session.id > 0);
        assert_eq!(session.started_at, START);
        assert_eq!(session.created_at, START);
        assert_eq!(session.ended_at, None, "yeni oturum acik olmali");
        assert_eq!(session.model, MODEL);
        assert_eq!(session.project_id.as_deref(), Some("asuna"));
        assert_eq!(session.summary, None);
        assert_eq!(session.transcript_path, None);
        assert_eq!(session.total_tokens, None);
    }

    #[test]
    fn start_rejects_an_empty_model_and_a_malformed_clock() {
        let db = fresh_db();
        assert_eq!(
            start(&db, "  ", None, START).expect_err("bos model").code(),
            StoreErrorCode::Invalid
        );
        assert_eq!(
            start(&db, MODEL, None, "simdi")
                .expect_err("bozuk zaman")
                .code(),
            StoreErrorCode::Invalid
        );
    }

    #[test]
    fn finalize_writes_end_time_and_usage_metadata() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");

        let closed = finalize(
            &db,
            session.id,
            &SessionFinalizeInput {
                usage: Some(usage()),
                transcript: Vec::new(),
            },
            None,
            END,
        )
        .expect("kapanmali");

        assert_eq!(closed.ended_at.as_deref(), Some(END));
        assert_eq!(closed.input_tokens, Some(1_200));
        assert_eq!(closed.output_tokens, Some(800));
        assert_eq!(closed.total_tokens, Some(2_000));
        assert_eq!(closed.model, MODEL, "kullanilan model kaydin icinde");

        // Ham kirilim kaybolmadi (memory.md T5): anahtarlar dogrulanana kadar
        // uydurulmus kolon yerine JSON'da duruyor.
        let raw: serde_json::Value =
            serde_json::from_str(&closed.usage_json.expect("usage_json")).expect("gecerli JSON");
        assert_eq!(raw["inputTokenDetails"][0]["audio_tokens"], 900);
        assert_eq!(raw["requests"], 3);

        // Fiyat tablosu dogrulanmadigi icin maliyet **uydurulmuyor** (bkz. ASU-033).
        assert_eq!(closed.estimated_cost_usd, None);
    }

    #[test]
    fn finalize_without_usage_still_closes_the_session() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");

        let closed = finalize(&db, session.id, &SessionFinalizeInput::default(), None, END)
            .expect("kapanmali");

        assert_eq!(closed.ended_at.as_deref(), Some(END));
        assert_eq!(closed.usage_json, None);
        assert_eq!(closed.total_tokens, None);
    }

    #[test]
    fn finalize_records_the_transcript_path_when_one_was_written() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");

        let closed = finalize(
            &db,
            session.id,
            &SessionFinalizeInput::default(),
            Some("/tmp/asuna-test/transcripts/session-1.jsonl"),
            END,
        )
        .expect("kapanmali");

        assert_eq!(
            closed.transcript_path.as_deref(),
            Some("/tmp/asuna-test/transcripts/session-1.jsonl")
        );
    }

    /// Saat geriye kaymissa oturum kaybedilmez: sifir sure yazilir.
    #[test]
    fn finalize_never_writes_an_end_before_the_start() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, END).expect("oturum");

        let closed = finalize(
            &db,
            session.id,
            &SessionFinalizeInput::default(),
            None,
            START,
        )
        .expect("kapanmali");

        assert_eq!(closed.ended_at.as_deref(), Some(END));
    }

    #[test]
    fn finalize_reports_not_found_for_an_unknown_session() {
        let db = fresh_db();
        assert_eq!(
            finalize(&db, 999, &SessionFinalizeInput::default(), None, END)
                .expect_err("bilinmeyen oturum")
                .code(),
            StoreErrorCode::NotFound
        );
    }

    #[test]
    fn finalize_rejects_negative_token_counts() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");

        let error = finalize(
            &db,
            session.id,
            &SessionFinalizeInput {
                usage: Some(SessionUsage {
                    input_tokens: Some(-5),
                    ..usage()
                }),
                transcript: Vec::new(),
            },
            None,
            END,
        )
        .expect_err("negatif token reddedilmeli");
        assert_eq!(error.code(), StoreErrorCode::Invalid);
    }

    /// **ASU-032 kabul kriteri**: cokme sonrasi yarim kalan oturum bir sonraki
    /// acilista kapatilir — `ended_at` NULL kalmaz.
    #[test]
    fn abandoned_sessions_are_closed_on_the_next_startup() {
        let db = fresh_db();
        let crashed = start(&db, MODEL, None, START).expect("oturum");
        let clean = start(&db, MODEL, None, START).expect("oturum");
        finalize(&db, clean.id, &SessionFinalizeInput::default(), None, END).expect("kapanis");

        let closed = close_abandoned(&db).expect("kurtarma");
        assert_eq!(closed, 1, "yalnizca yarim kalan oturum kapatilmali");

        let recovered = get_by_id(&db, crashed.id).expect("okuma").expect("kayit");
        assert_eq!(
            recovered.ended_at.as_deref(),
            Some(START),
            "bitis zamani bilinmiyor; sahte bir sure yazilmamali"
        );
        assert_eq!(
            recovered.summary.as_deref(),
            Some(ABANDONED_SESSION_SUMMARY)
        );

        // Temiz kapanan oturum degismedi.
        let untouched = get_by_id(&db, clean.id).expect("okuma").expect("kayit");
        assert_eq!(untouched.ended_at.as_deref(), Some(END));
        assert_eq!(untouched.summary, None);

        // Ikinci calistirma bir sey yapmaz (idempotent).
        assert_eq!(close_abandoned(&db).expect("kurtarma"), 0);
    }

    #[test]
    fn transcript_is_clamped_to_the_upper_bound_keeping_the_latest_turns() {
        let lines: Vec<TranscriptLine> = (0..MAX_TRANSCRIPT_LINES + 10)
            .map(|index| TranscriptLine {
                role: TranscriptRole::User,
                text: index.to_string(),
                at: None,
            })
            .collect();

        let clamped = clamp_transcript(&lines);
        assert_eq!(clamped.len(), MAX_TRANSCRIPT_LINES);
        assert_eq!(clamped[0].text, "10", "en eski replikler dusurulmeli");
        assert_eq!(
            clamped[clamped.len() - 1].text,
            (MAX_TRANSCRIPT_LINES + 9).to_string()
        );
    }

    #[test]
    fn write_result_serializes_with_an_explicit_status() {
        let json = serde_json::to_value(SessionWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        })
        .expect("serialize");
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["reason"], "memory-disabled");
    }

    #[test]
    fn unknown_finalize_fields_are_rejected_at_the_ipc_boundary() {
        assert!(serde_json::from_str::<SessionFinalizeInput>(
            r#"{"transcriptPath":"/etc/passwd"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<SessionFinalizeInput>(
            r#"{"usage":{"inputTokens":1,"secret":"x"}}"#
        )
        .is_err());

        let parsed: SessionFinalizeInput = serde_json::from_str(
            r#"{"usage":{"inputTokens":10,"outputTokens":5,"totalTokens":15},
                "transcript":[{"role":"user","text":"merhaba"}]}"#,
        )
        .expect("gecerli girdi");
        assert_eq!(parsed.transcript.len(), 1);
        assert_eq!(
            parsed.usage.expect("usage").total_tokens,
            Some(15),
            "camelCase alanlar okunmali"
        );
    }
}

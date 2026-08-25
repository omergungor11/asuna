//! `sessions` kaydi: acilis, kapanis, yarim kalan oturumlarin kurtarilmasi
//! (ASU-032).
//!
//! # Sozlesme
//!
//! - Oturum **modelini renderer secmez**: `model` her zaman `AsunaConfig`'ten
//!   (`ASUNA_REALTIME_MODEL`) gelir. Aksi halde webview'den gelen bir metin
//!   dogrudan fatura kaydina yazilirdi.
//! - Kapanis yolu **hicbir zaman** oturumu ayakta birakmaz: transcript yazimi,
//!   usage okumasi ya da ozet uretimi basarisiz olsa bile `ended_at` yazilir.
//!   Ozet (ASU-033) kapanistan **sonra**, ayri bir `UPDATE` ile eklenir.
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
//! uretirdi. Sifir sure "bilmiyoruz" demenin en az yaniltici yolu.
//!
//! **ASU-033**: neden artik `summary` alanina yazilmiyor. Ozet alani ASU-034'un
//! memory extraction girdisidir; oraya konan bir durum bayragi ya gercek ozeti
//! ezer ya da bir sistem cumlesinden hafiza uretilmesine yol acardi. Durum
//! migration 002 ile acilan `end_reason` kolonunda tutuluyor
//! ([`SessionEndReason`]), eski kayitlar da o migration'da tasindi.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::clock;
use super::model::{SessionEndReason, SessionRecord};
use super::store_error::{database, StoreError, StoreSkipReason};
use super::transcript::{self, TranscriptLine};
use super::{AsunaDb, DbState};
use crate::config::AsunaConfig;
use crate::summary;

/// Yarim kalan oturumun **eski** isaretlenme bicimi (ASU-032).
///
/// Artik yazilmiyor: durum `sessions.end_reason` kolonunda tutuluyor. Sabit
/// duruyor cunku migration 002 eski kayitlari **bu cumleyi eslestirerek**
/// tasiyor; metin degisirse geriye donuk doldurma sessizce hicbir sey yapmaz
/// (bir test ikisini birbirine bagliyor).
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

/// Renderer'in bildirebilecegi kapanis nedeni (ASU-033).
///
/// `abandoned` bilerek **yok**: yarim kalan oturumu tespit etmek host'un isidir
/// (acilistaki kurtarma). Renderer bir oturumu "kurtarilmis" ilan edemez, aksi
/// halde `end_reason` kolonu neyi olctugunu kaybederdi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportedEndReason {
    /// Oturum normal sekilde kapandi.
    #[default]
    Completed,
    /// Oturum bir hata ile sonlandi (baglanti koptu, SDK hatasi).
    Error,
}

impl From<ReportedEndReason> for SessionEndReason {
    fn from(value: ReportedEndReason) -> Self {
        match value {
            ReportedEndReason::Completed => Self::Completed,
            ReportedEndReason::Error => Self::Error,
        }
    }
}

/// Oturum kapanis girdisi.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionFinalizeInput {
    #[serde(default)]
    pub usage: Option<SessionUsage>,
    /// Yalnizca `ASUNA_TRANSCRIPT_STORAGE=true` iken diske yazilir; aksi halde
    /// bellekte kalir ve atilir. Ozet uretimi (ASU-033) bu bellekteki metni
    /// kullanir — diske yazma ayarindan bagimsizdir.
    #[serde(default)]
    pub transcript: Vec<TranscriptLine>,
    /// Verilmezse `completed` sayilir.
    #[serde(default)]
    pub end_reason: ReportedEndReason,
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
    end_reason: SessionEndReason,
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
        end_reason: input.end_reason.into(),
    };

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;

            // Saat geriye kaymissa `ended_at < started_at` semadaki CHECK'e
            // takilirdi. Oturumu kaybetmektense sifir sure yazmak dogru:
            // kapanis **her kosulda** tamamlanmali.
            //
            // `model` de buradan okunuyor: fiyatlandirma **kaydin kendi
            // modeline** gore yapilir, config'in o anki degerine gore degil
            // (kullanici modeli oturum ortasinda degistirmis olabilir).
            let row: Option<(String, String)> = transaction
                .query_row(
                    "SELECT started_at, model FROM sessions WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            let Some((started_at, model)) = row else {
                transaction.commit()?;
                return Ok(None);
            };
            let ended_at = if values.ended_at < started_at {
                started_at
            } else {
                values.ended_at.clone()
            };

            // Maliyet: yalnizca dogrulanmis fiyat + kirilimi aciklanabilen
            // kullanim icin. Aksi halde `NULL` kalir — UI "bilinmiyor" yazar.
            let estimated_cost_usd =
                usage.and_then(|usage| crate::pricing::estimate_realtime_cost_usd(&model, usage));

            transaction.execute(
                "UPDATE sessions
                    SET ended_at = ?1,
                        input_tokens = ?2,
                        output_tokens = ?3,
                        total_tokens = ?4,
                        usage_json = ?5,
                        transcript_path = ?6,
                        end_reason = ?7,
                        estimated_cost_usd = ?8
                  WHERE id = ?9",
                params![
                    ended_at,
                    values.input_tokens,
                    values.output_tokens,
                    values.total_tokens,
                    values.usage_json,
                    values.transcript_path,
                    values.end_reason,
                    estimated_cost_usd,
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
/// `summary` alanina **dokunmaz** (ASU-033): durum `end_reason` kolonunda
/// tutulur, ozet alani ozet icindir.
///
/// @returns kapatilan oturum sayisi.
pub fn close_abandoned(db: &AsunaDb) -> Result<usize, StoreError> {
    db.with_connection(|connection| {
        connection.execute(
            "UPDATE sessions
                SET ended_at = started_at,
                    end_reason = ?1
              WHERE ended_at IS NULL",
            params![SessionEndReason::Abandoned],
        )
    })
    .map_err(|error| StoreError::storage(error, "session_recovery"))
}

/// Kapanmis bir oturuma ozeti (ve ozet maliyetini) **sonradan** ekler (ASU-033).
///
/// Kapanis ile ozet bilincli olarak ayri iki yazma: ozet uretimi ag uzerinden
/// saniyeler surer ve basarisiz olabilir; oturum kaydinin kapanmasi buna
/// bagli olamaz (kabul kriteri: "ozet basarisiz olursa oturum yine kapaniyor").
///
/// `usage_patch` verilirse `usage_json` icine `$.summary` altina yazilir —
/// realtime oturumunun token kirilimi **ezilmez**, yaninda durur. Kolon NULL
/// ise once bos bir nesne olusturulur.
///
/// `WHERE ended_at IS NOT NULL`: hala acik (ya da silinmis) bir oturuma ozet
/// yazilmaz. Boyle bir durumda [`StoreError::NotFound`] doner.
pub fn attach_summary(
    db: &AsunaDb,
    id: i64,
    summary_text: &str,
    usage_patch: Option<&str>,
) -> Result<SessionRecord, StoreError> {
    if id <= 0 {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }
    let summary_text = summary_text.trim();
    if summary_text.is_empty() {
        return Err(StoreError::invalid("`summary` bos birakilamaz"));
    }
    if let Some(patch) = usage_patch {
        if serde_json::from_str::<serde_json::Value>(patch).is_err() {
            return Err(StoreError::invalid("`usagePatch` gecerli JSON olmali"));
        }
    }

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            let updated = transaction.execute(
                "UPDATE sessions
                    SET summary = ?1,
                        usage_json = CASE
                            WHEN ?2 IS NULL THEN usage_json
                            ELSE json_set(COALESCE(usage_json, '{}'), '$.summary', json(?2))
                        END
                  WHERE id = ?3 AND ended_at IS NOT NULL",
                params![summary_text, usage_patch, id],
            )?;

            let record = if updated == 0 {
                None
            } else {
                load(&transaction, id)?
            };
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "session_summary"))?;

    record.ok_or(StoreError::NotFound)
}

/// Kapanmis bir oturumun `usage_json` alanina **tek bir alt agac** yamalar
/// (ASU-034).
///
/// [`attach_summary`] ozet metnini yazarken `$.summary` altini dolduruyor;
/// cikarim adiminin kendi maliyeti ise ozetten **sonra** olusuyor ve ozeti
/// yeniden yazmasi gerekmiyor. Ayri bir fonksiyon olmasinin nedeni bu: ayni
/// isi `attach_summary` ile yapmak, ozet metnini gereksiz yere ikinci kez
/// yazmak (ve yanlislikla ezmek) anlamina gelirdi.
///
/// `key` **sabit metindir** (`"extraction"` gibi); kullanici girdisi buraya
/// gelmez. Yine de JSON yolu SQL'e gomulmez, parametre olarak baglanir.
/// Var olan diger anahtarlar (`$.summary`, realtime kirilimi) korunur.
pub fn attach_usage(
    db: &AsunaDb,
    id: i64,
    key: &'static str,
    usage_patch: &str,
) -> Result<SessionRecord, StoreError> {
    if id <= 0 {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }
    if serde_json::from_str::<serde_json::Value>(usage_patch).is_err() {
        return Err(StoreError::invalid("`usagePatch` gecerli JSON olmali"));
    }

    let path = format!("$.{key}");
    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            let updated = transaction.execute(
                "UPDATE sessions
                    SET usage_json = json_set(COALESCE(usage_json, '{}'), ?1, json(?2))
                  WHERE id = ?3 AND ended_at IS NOT NULL",
                params![path, usage_patch, id],
            )?;

            let record = if updated == 0 {
                None
            } else {
                load(&transaction, id)?
            };
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "session_usage"))?;

    record.ok_or(StoreError::NotFound)
}

/// Ozeti yazilmis, **temiz kapanmis** en son oturum (ASU-035 Stage A girdisi).
///
/// Uc kosul birlikte aranir ve ucu de bilincli:
///
/// - `ended_at IS NOT NULL` — hala acik bir oturumun ozeti yoktur.
/// - `summary IS NOT NULL` — ozet uretimi basarisiz olduysa (ag hatasi) o
///   oturumdan tasinacak bir bilgi yok; bir onceki ozete duselim.
/// - `end_reason = 'completed'` — yarim kalan (`abandoned`) oturumun `summary`
///   alani zaten bostur; hata ile biten (`error`) oturumun ozeti ise eksik bir
///   konusmayi anlatir. Sonraki oturuma "gecen sefer sunu konustuk" diye
///   eksik/yanlis bir ozet tasimak, hic tasimamaktan kotudur.
///
/// `ORDER BY ended_at DESC, id DESC`: zaman damgasi saniye hassasiyetinde
/// (bkz. [`clock`]); ayni saniyede kapanan iki kaydin sirasi `id` ile cozulur.
pub fn latest_completed_summary(db: &AsunaDb) -> Result<Option<SessionRecord>, StoreError> {
    db.with_connection(|connection| {
        connection
            .query_row(
                &format!(
                    "SELECT {} FROM sessions
                      WHERE ended_at IS NOT NULL
                        AND summary IS NOT NULL
                        AND end_reason = ?1
                      ORDER BY ended_at DESC, id DESC
                      LIMIT 1",
                    SessionRecord::select_columns()
                ),
                params![SessionEndReason::Completed],
                SessionRecord::from_row,
            )
            .optional()
    })
    .map_err(|error| StoreError::storage(error, "session_recent"))
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
///
/// # Ozet (ASU-033)
///
/// Komut ozeti **beklemez**. Once DB yazmasi tamamlanir ve sonuc doner; ozet
/// uretimi arka planda tetiklenir ve tamamlaninca ayri bir `UPDATE` ile
/// eklenir. Neden bu sira:
///
/// 1. Kapanis bir ag cagrisina bagimli olamaz — kullanici "Asuna kapandi mi?"
///    diye 30 saniye beklemez.
/// 2. Uygulama ozet donmeden kapanirsa kaybedilen tek sey ozettir; oturum
///    kaydi zaten kapali ve tutarli (`summary` NULL, `end_reason` dogru).
/// 3. Kuyruk/retry tablosu gerekmiyor: yarim kalmis bir "ozet bekliyor" durumu
///    hic olusmuyor.
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

    // Kapanis yazildi. Ozet bundan **sonra**, arka planda; buradan itibaren
    // hicbir hata oturum kaydini etkilemez.
    summary::spawn_for_session(&app, session.id, input.transcript);

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
                ..SessionFinalizeInput::default()
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

        // Kirilim toplami aciklamiyor (900 ses tokeni, 1.200 toplam): maliyet
        // **uydurulmuyor** (ASU-033, `pricing`).
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
                ..SessionFinalizeInput::default()
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
        // ASU-033: durum `end_reason` kolonunda; `summary` ozet icin ayrildi.
        assert_eq!(recovered.end_reason, Some(SessionEndReason::Abandoned));
        assert_eq!(
            recovered.summary, None,
            "ozet alani durum bayragi olarak kullanilmamali"
        );

        // Temiz kapanan oturum degismedi.
        let untouched = get_by_id(&db, clean.id).expect("okuma").expect("kayit");
        assert_eq!(untouched.ended_at.as_deref(), Some(END));
        assert_eq!(untouched.summary, None);
        assert_eq!(untouched.end_reason, Some(SessionEndReason::Completed));

        // Ikinci calistirma bir sey yapmaz (idempotent).
        assert_eq!(close_abandoned(&db).expect("kurtarma"), 0);
    }

    // --- Kapanis nedeni + ozet (ASU-033) ----------------------------------

    #[test]
    fn finalize_records_the_reported_end_reason() {
        for (reported, expected) in [
            (ReportedEndReason::Completed, SessionEndReason::Completed),
            (ReportedEndReason::Error, SessionEndReason::Error),
        ] {
            let db = fresh_db();
            let session = start(&db, MODEL, None, START).expect("oturum");
            let closed = finalize(
                &db,
                session.id,
                &SessionFinalizeInput {
                    end_reason: reported,
                    ..SessionFinalizeInput::default()
                },
                None,
                END,
            )
            .expect("kapanmali");

            assert_eq!(closed.end_reason, Some(expected));
        }
    }

    /// Renderer bir oturumu "kurtarilmis" ilan edemez: `abandoned` sozlesmede
    /// yok, IPC sinirinde duser.
    #[test]
    fn the_renderer_cannot_claim_a_session_was_abandoned() {
        assert!(
            serde_json::from_str::<SessionFinalizeInput>(r#"{"endReason":"abandoned"}"#).is_err()
        );
        assert!(serde_json::from_str::<SessionFinalizeInput>(r#"{"endReason":"cokme"}"#).is_err());

        let parsed: SessionFinalizeInput =
            serde_json::from_str(r#"{"endReason":"error"}"#).expect("gecerli girdi");
        assert_eq!(parsed.end_reason, ReportedEndReason::Error);

        // Verilmezse `completed`.
        let default: SessionFinalizeInput = serde_json::from_str("{}").expect("gecerli girdi");
        assert_eq!(default.end_reason, ReportedEndReason::Completed);
    }

    #[test]
    fn attach_summary_writes_the_text_and_the_usage_patch() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");
        finalize(&db, session.id, &SessionFinalizeInput::default(), None, END).expect("kapanis");

        let updated = attach_summary(
            &db,
            session.id,
            "  Konusulanlar: Sema kararlari.  ",
            Some(r#"{"model":"gpt-4o-mini","totalTokens":120}"#),
        )
        .expect("ozet yazilmali");

        assert_eq!(
            updated.summary.as_deref(),
            Some("Konusulanlar: Sema kararlari."),
            "bosluklar kirpilmali"
        );
        let usage: serde_json::Value =
            serde_json::from_str(&updated.usage_json.expect("usage_json")).expect("gecerli JSON");
        assert_eq!(usage["summary"]["totalTokens"], 120);
    }

    /// Cikarim maliyeti kendi anahtarina yazilir; ozet ve realtime kirilimi
    /// **ezilmez** (ASU-034).
    #[test]
    fn attach_usage_patches_one_key_without_touching_the_others() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");
        finalize(
            &db,
            session.id,
            &SessionFinalizeInput {
                usage: Some(SessionUsage {
                    requests: Some(3),
                    input_tokens: None,
                    output_tokens: None,
                    total_tokens: None,
                    input_token_details: Vec::new(),
                    output_token_details: Vec::new(),
                }),
                ..SessionFinalizeInput::default()
            },
            None,
            END,
        )
        .expect("kapanis");
        attach_summary(
            &db,
            session.id,
            "Konusulanlar: Sema kararlari.",
            Some(r#"{"totalTokens":120}"#),
        )
        .expect("ozet");

        let updated = attach_usage(
            &db,
            session.id,
            "extraction",
            r#"{"model":"gpt-4o-mini","totalTokens":500,"created":2}"#,
        )
        .expect("maliyet yazilmali");

        assert_eq!(
            updated.summary.as_deref(),
            Some("Konusulanlar: Sema kararlari."),
            "ozet metni korunmali"
        );
        let usage: serde_json::Value =
            serde_json::from_str(&updated.usage_json.expect("usage_json")).expect("gecerli JSON");
        assert_eq!(usage["requests"], 3, "realtime kirilimi korunmali");
        assert_eq!(
            usage["summary"]["totalTokens"], 120,
            "ozet maliyeti korunmali"
        );
        assert_eq!(usage["extraction"]["totalTokens"], 500);
        assert_eq!(usage["extraction"]["created"], 2);
    }

    #[test]
    fn attach_usage_refuses_open_sessions_and_broken_json() {
        let db = fresh_db();
        let open = start(&db, MODEL, None, START).expect("oturum");
        assert_eq!(
            attach_usage(&db, open.id, "extraction", "{}")
                .expect_err("acik oturum")
                .code(),
            StoreErrorCode::NotFound
        );

        finalize(&db, open.id, &SessionFinalizeInput::default(), None, END).expect("kapanis");
        assert_eq!(
            attach_usage(&db, open.id, "extraction", "{ bozuk")
                .expect_err("bozuk JSON")
                .code(),
            StoreErrorCode::Invalid
        );
        assert_eq!(
            attach_usage(&db, 0, "extraction", "{}")
                .expect_err("gecersiz id")
                .code(),
            StoreErrorCode::Invalid
        );
    }

    /// Hala acik bir oturuma ozet yazilmaz — ozet kapanmis konusmanin ozetidir.
    #[test]
    fn attach_summary_refuses_open_or_unknown_sessions() {
        let db = fresh_db();
        let open = start(&db, MODEL, None, START).expect("oturum");

        assert_eq!(
            attach_summary(&db, open.id, "ozet", None)
                .expect_err("acik oturum")
                .code(),
            StoreErrorCode::NotFound
        );
        assert_eq!(
            attach_summary(&db, 999, "ozet", None)
                .expect_err("bilinmeyen oturum")
                .code(),
            StoreErrorCode::NotFound
        );
    }

    #[test]
    fn attach_summary_rejects_empty_text_and_broken_usage_json() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");
        finalize(&db, session.id, &SessionFinalizeInput::default(), None, END).expect("kapanis");

        assert_eq!(
            attach_summary(&db, session.id, "   ", None)
                .expect_err("bos ozet")
                .code(),
            StoreErrorCode::Invalid
        );
        assert_eq!(
            attach_summary(&db, session.id, "ozet", Some("{ bozuk"))
                .expect_err("bozuk JSON")
                .code(),
            StoreErrorCode::Invalid
        );
        // Hicbiri kaydi degistirmemis olmali.
        assert_eq!(
            get_by_id(&db, session.id)
                .expect("okuma")
                .expect("kayit")
                .summary,
            None
        );
    }

    /// **Kabul kriteri**: maliyet yalnizca dogrulanmis fiyat + aciklanabilir
    /// kirilim varsa hesaplanir.
    #[test]
    fn cost_is_estimated_only_from_a_verified_price_and_an_explained_breakdown() {
        let db = fresh_db();
        let priced = start(&db, "gpt-realtime-2.1", None, START).expect("oturum");
        let closed = finalize(
            &db,
            priced.id,
            &SessionFinalizeInput {
                usage: Some(SessionUsage {
                    requests: Some(1),
                    input_tokens: Some(1_000),
                    output_tokens: Some(500),
                    total_tokens: Some(1_500),
                    input_token_details: vec![serde_json::json!({ "audio_tokens": 1_000 })],
                    output_token_details: vec![serde_json::json!({ "audio_tokens": 500 })],
                }),
                ..SessionFinalizeInput::default()
            },
            None,
            END,
        )
        .expect("kapanis");

        let expected = 1_000.0 * 32.0 / 1e6 + 500.0 * 64.0 / 1e6;
        let cost = closed.estimated_cost_usd.expect("fiyat tabloda var");
        assert!((cost - expected).abs() < 1e-12, "maliyet: {cost}");

        // Fiyati dogrulanmamis bir model: sayi **uydurulmaz**.
        let unknown = start(&db, "gpt-hayali-realtime", None, START).expect("oturum");
        let closed = finalize(
            &db,
            unknown.id,
            &SessionFinalizeInput {
                usage: Some(usage()),
                ..SessionFinalizeInput::default()
            },
            None,
            END,
        )
        .expect("kapanis");
        assert_eq!(closed.estimated_cost_usd, None);
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

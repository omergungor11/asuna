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
//! - Hafiza kapaliyken hicbir oturum kaydi olusmaz; komut `skipped` doner ve
//!   renderer oturum kimligi almaz — sonraki `session_finalize` cagrisi da
//!   yapilmaz. "Kapali" iki kaynaktan gelebilir ve **ikisi de** kontrol edilir:
//!   acilis degeri (`ASUNA_MEMORY_ENABLED=false` → DB hic acilmaz) ve calisma
//!   zamani anahtari ([`crate::privacy::PrivacyState`], ASU-037). Ikincisi
//!   olmadan kullanici Ayarlar'dan hafizayi kapatsa bile oturum satiri, dokum
//!   dosyasi ve ozet yazilmaya devam ederdi.
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

use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::clock;
use super::model::{SessionEndReason, SessionModality, SessionRecord};
use super::project_repository;
use super::store_error::{database, StoreError, StoreSkipReason};
use super::transcript::{self, TranscriptFileOutcome, TranscriptLine};
use super::{AsunaDb, DbState};
use crate::config::AsunaConfig;
use crate::privacy::PrivacyState;
use crate::summary;

/// Yarim kalan oturumun **eski** isaretlenme bicimi (ASU-032).
///
/// Artik yazilmiyor: durum `sessions.end_reason` kolonunda tutuluyor. Sabit
/// duruyor cunku migration 002 eski kayitlari **bu cumleyi eslestirerek**
/// tasiyor; metin degisirse geriye donuk doldurma sessizce hicbir sey yapmaz
/// (bir test ikisini birbirine bagliyor).
pub const ABANDONED_SESSION_SUMMARY: &str =
    "Oturum beklenmedik sekilde kapandi (uygulama yeniden acilirken kapatildi).";

/// Oturum listesinin varsayilan uzunlugu (ASU-065).
pub const DEFAULT_SESSION_LIST_LIMIT: u32 = 50;

/// Oturum listesi icin tavan. Asan istek **reddedilmez, kirpilir** — ama
/// kirpildigi [`SessionPage`] icinde gorunur olur (`limit` + `total`), cunku
/// "hepsi bu kadar" yalanini uretmek denetlenebilirligi bozar.
pub const MAX_SESSION_LIST_LIMIT: u32 = 200;

/// Listede gosterilen ozet on izlemesinin azami karakteri.
///
/// Tam ozet listeye konmaz: ekran denetim yuzeyi, okuma ekrani degil. Kirpma
/// [`SessionListItem::summary_truncated`] ile **gorunur** (kirpilmis metni tam
/// sanmak, hafizanin ne tasidigi konusunda yanlis fikir verir).
pub const SUMMARY_PREVIEW_CHARS: usize = 280;

/// Bir oturumda beklenebilecek azami replik sayisi.
///
/// Renderer'in gonderdigi dokum diske yazilacak; sinirsiz bir dizi hem IPC
/// mesajini hem dosyayi sisirir. Ust sinir asilirsa **son** replikler tutulur
/// (yeni olan daha degerli).
pub const MAX_TRANSCRIPT_LINES: usize = 2_000;

/// Konusma basliginin azami karakteri — semadaki CHECK ile ayni (migration
/// 006). Asan baslik **kirpilmaz, reddedilir**: bkz. [`set_title`].
pub const MAX_SESSION_TITLE_CHARS: usize = 200;

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

// ---------------------------------------------------------------------------
// Listeleme + silme sozlesmesi (ASU-065)
// ---------------------------------------------------------------------------

/// Oturum listesinin tek satiri.
///
/// [`SessionRecord`] **degil**: bu bir denetim satiri, DB kaydinin kopyasi
/// degil. Iki alan bilerek disarida:
///
/// - `transcript_path`: renderer'a dosya yolu gitmez. Kullanicinin dizin yapisi
///   webview'e tasinacak bir bilgi degil ve silme zaten host tarafinda yapiliyor
///   — UI'in bilmesi gereken tek sey **dosya var mi**
///   ([`SessionListItem::has_transcript_file`]).
/// - `usage_json` / token kirilimi: oturum ozeti UI'i (ASU-032) bunlari kapanis
///   aninda zaten gosteriyor; listeye tasimak her satirda ham JSON demek olurdu.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListItem {
    pub id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub end_reason: Option<SessionEndReason>,
    /// `None` = ozet uretilmedi (kisa oturum ya da basarisiz ozetleme).
    pub summary_preview: Option<String>,
    /// `true` ise on izleme kirpildi; kaydin kendisi degismedi.
    pub summary_truncated: bool,
    /// Diskte bir dokum dosyasi kayitli mi (`transcript_path IS NOT NULL`)?
    pub has_transcript_file: bool,
}

/// Oturum listesi + **olculen** sinirlar.
///
/// `total` bilerek var: `memory_list` yalnizca kirpilmis bir dizi donuyor ve UI
/// tavana carptigini tahmin etmek zorunda kaliyor (backlog: sunucu tarafi
/// sayfalama). Oturum sayisi tek bir `COUNT(*)` ile bilinebiliyor; "50 / 214
/// oturum" demek, "en yeni 50" demekten durust.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    pub sessions: Vec<SessionListItem>,
    /// Uygulanan limit (kirpilmis olabilir).
    pub limit: u32,
    /// [`MAX_SESSION_LIST_LIMIT`] — tavanin kendisi de gorunur.
    pub limit_max: u32,
    /// Depodaki toplam oturum sayisi.
    pub total: u32,
}

/// Liste istegi. Renderer yalnizca **kac tane** diyebilir; siralama ve alan
/// secimi host tarafinda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionListQuery {
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Tek oturum silmenin sonucu.
///
/// `transcript_file` ayri bir alan cunku iki is birlikte yapiliyor ve **ayri
/// ayri** basarisiz olabilir: satir gidip dosya kalabilir. "Sildim" demek
/// yalnizca ikisi de bilindiginde dogrudur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum SessionDeleteResult {
    // Alan adlari da `camelCase`: enum uzerindeki `rename_all` yalnizca **varyant**
    // adlarini etkiler, alanlari etkilemez (bu ayrim bir testle bagli).
    #[serde(rename_all = "camelCase")]
    Deleted {
        id: i64,
        transcript_file: TranscriptFileOutcome,
    },
    Skipped {
        reason: StoreSkipReason,
    },
}

/// Toplu temizligin sonucu — hepsi **sayi**, cunku kullanici "gercekten gitti
/// mi, ne kadari?" sorusunun cevabini gormeli.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum SessionPurgeResult {
    #[serde(rename_all = "camelCase")]
    Purged {
        /// Silinen `sessions` satiri sayisi (ozetler dahil).
        deleted_sessions: u32,
        /// Diskten silinen dokum dosyasi sayisi.
        deleted_files: u32,
        /// Dokum dizininde **birakilan** girdi sayisi (silinemeyenler +
        /// Asuna'nin uretmedigi dosyalar). Sifir degilse UI bunu yazar.
        remaining_files: u32,
    },
    Skipped {
        reason: StoreSkipReason,
    },
}

/// Baslik yazmanin sonucu (Chat Shell).
///
/// `SessionWriteResult` **kullanilmiyor**: o tip tam bir [`SessionRecord`]
/// tasiyor ve `src/shared/session.ts` onu beklenmeyen alan varsa hata vererek
/// ayristiriyor. Baslik yazmak icin tum kaydi geri gondermek hem gereksiz hem
/// de o sozlesmeyi bu yola bagimli kilardi.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum SessionTitleResult {
    #[serde(rename_all = "camelCase")]
    Recorded {
        id: i64,
        title: String,
    },
    Skipped {
        reason: StoreSkipReason,
    },
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

/// Yeni **ses** oturumu acar (`started_at` + `model`).
///
/// Modalite [`SessionModality::Voice`]: bu, 006 oncesindeki tek yoldu ve
/// mevcut cagiranlarin (ses akisi, testler) davranisi degismedi. Metin
/// konusmasi acan yol [`start_with_modality`]'dir.
pub fn start(
    db: &AsunaDb,
    model: &str,
    project_id: Option<&str>,
    now: &str,
) -> Result<SessionRecord, StoreError> {
    start_with_modality(db, model, project_id, SessionModality::Voice, now)
}

/// Yeni oturum kaydi acar; modaliteyi cagiran secer (Chat Shell).
///
/// Ayri bir fonksiyon olmasinin sebebi bir uslup tercihi degil: [`start`]'in
/// imzasini degistirmek ses yolundaki ve `extraction` testlerindeki tum
/// cagiranlari dokunmaya zorlardi. Varsayilan davranis tek bir yerde yazili
/// kalsin diye [`start`] buraya deleger.
pub fn start_with_modality(
    db: &AsunaDb,
    model: &str,
    project_id: Option<&str>,
    modality: SessionModality,
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
            // 003'ten beri `project_id` bir yabanci anahtar; etiketin karsiligi
            // yoksa `unlinked` bir satir acilir (`memory_create` ile ayni kural).
            project_repository::ensure_optional_label(&transaction, project_id.as_deref(), &now)?;
            // `modality` acikca yaziliyor: semadaki DEFAULT bir guvenlik agi,
            // yazma yolunun kaynagi degil.
            transaction.execute(
                "INSERT INTO sessions (started_at, project_id, model, created_at, modality)
                 VALUES (?1, ?2, ?3, ?1, ?4)",
                params![now, project_id, model, modality],
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

// ---------------------------------------------------------------------------
// Listeleme + silme (ASU-065)
// ---------------------------------------------------------------------------

/// En yeni oturumlari denetim satiri olarak dondurur.
///
/// Siralama `started_at DESC, id DESC`: zaman damgasi saniye hassasiyetinde
/// (bkz. [`clock`]), ayni saniyede acilan iki oturumun sirasi `id` ile cozulur.
/// **Acik oturumlar da listelenir** — su an konusulan oturum kullanicidan
/// gizlenmez; `ended_at = null` olarak gorunur.
pub fn list_recent(db: &AsunaDb, limit: u32) -> Result<SessionPage, StoreError> {
    let limit = limit.clamp(1, MAX_SESSION_LIST_LIMIT);

    db.with_connection(|connection| {
        let total: i64 =
            connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;

        let mut statement = connection.prepare(
            "SELECT id, started_at, ended_at, end_reason, summary,
                    transcript_path IS NOT NULL AS has_transcript_file
               FROM sessions
              ORDER BY started_at DESC, id DESC
              LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            let summary: Option<String> = row.get("summary")?;
            let (summary_preview, summary_truncated) = clamp_summary(summary.as_deref());
            Ok(SessionListItem {
                id: row.get("id")?,
                started_at: row.get("started_at")?,
                ended_at: row.get("ended_at")?,
                end_reason: row.get("end_reason")?,
                summary_preview,
                summary_truncated,
                has_transcript_file: row.get::<_, i64>("has_transcript_file")? != 0,
            })
        })?;
        let sessions = rows.collect::<rusqlite::Result<Vec<SessionListItem>>>()?;

        Ok(SessionPage {
            sessions,
            limit,
            limit_max: MAX_SESSION_LIST_LIMIT,
            total: u32::try_from(total).unwrap_or(u32::MAX),
        })
    })
    .map_err(|error| StoreError::storage(error, "session_list"))
}

/// Ozeti on izleme uzunluguna kirpar. @returns `(on izleme, kirpildi mi)`.
fn clamp_summary(summary: Option<&str>) -> (Option<String>, bool) {
    let Some(text) = summary.map(str::trim).filter(|text| !text.is_empty()) else {
        return (None, false);
    };
    if text.chars().count() <= SUMMARY_PREVIEW_CHARS {
        return (Some(text.to_owned()), false);
    }
    let head: String = text.chars().take(SUMMARY_PREVIEW_CHARS).collect();
    (Some(format!("{head}…")), true)
}

/// Oturum satirini siler ve varsa **kayitli dokum yolunu** dondurur.
///
/// Dosya bu katmanda silinmez: repository dosya sistemine dokunmaz (yazma
/// yolundaki ayrimin aynisi). Yol cagirana doner, silmeyi komut yapar.
///
/// Sira bilincli — **once satir**: kullanicinin sikayeti "sildim ama hatirladi"
/// idi ve hatirlamayi ureten sey `sessions.summary`. Dosya silinemese bile ozet
/// gitmis olmali; aksi halde bir `EACCES` hatasi hafizanin silinmesini
/// engellerdi.
///
/// `memories.source_session_id` bu satira bagliysa **hafiza silinmez**: FK
/// `ON DELETE SET NULL` (migration 001). Kayit durur, kaynagi "bilinmiyor"a
/// doner — hafizayi silme yetkisi kullanicinindir ve o ayri bir aksiyondur.
pub fn delete(db: &AsunaDb, id: i64) -> Result<Option<String>, StoreError> {
    if id <= 0 {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }

    let outcome = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            let recorded: Option<Option<String>> = transaction
                .query_row(
                    "SELECT transcript_path FROM sessions WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;

            let Some(recorded) = recorded else {
                transaction.commit()?;
                return Ok(None);
            };
            transaction.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
            transaction.commit()?;
            Ok(Some(recorded))
        })
        .map_err(|error| StoreError::storage(error, "session_delete"))?;

    outcome.ok_or(StoreError::NotFound)
}

/// **Tum** oturum kayitlarini siler ve silinen sayiyi dondurur.
///
/// [`memory_repository::delete_all`](super::memory_repository::delete_all) ile
/// ayni desen: silme sonrasi `VACUUM` denenir (serbest sayfalar dosyada
/// kalmasin — bu bir gizlilik aksiyonu), basarisiz olursa islem yine basarili
/// sayilir ve hata yerel log'a duser.
pub fn delete_all(db: &AsunaDb) -> Result<u32, StoreError> {
    let deleted = db
        .with_connection(|connection| connection.execute("DELETE FROM sessions", []))
        .map_err(|error| StoreError::storage(error, "session_clear_all"))?;

    if let Err(error) = db.with_connection(|connection| connection.execute_batch("VACUUM")) {
        eprintln!("[asuna] Oturum temizligi sonrasi VACUUM basarisiz: {error}");
    }

    Ok(u32::try_from(deleted).unwrap_or(u32::MAX))
}

/// Konusma basligini yazar (Chat Shell).
///
/// # Neden kirpma degil reddetme
///
/// Baslik kullanicinin (ya da otomatik baslik kuralinin) verdigi bir etikettir.
/// Tavani asan bir basligi sessizce kirpmak, listede gordugu metnin kaydin
/// tamami oldugunu sanmasina yol acardi; kirpma karari cagiranin
/// (`setTitle(ilk 60 karakter)`) ve gorunur olmali.
///
/// Bos baslik da reddedilir: "baslik yok" durumu NULL ile ifade edilir ve UI
/// onu "Adsiz konusma" olarak yazar. Bos metin ikisinin arasinda anlamsiz bir
/// ucuncu durum uretirdi (semadaki CHECK de bunu zorluyor).
pub fn set_title(db: &AsunaDb, id: i64, title: &str) -> Result<String, StoreError> {
    if id <= 0 {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }
    let title = title.trim();
    if title.is_empty() {
        return Err(StoreError::invalid("`title` bos birakilamaz"));
    }
    if title.chars().count() > MAX_SESSION_TITLE_CHARS {
        return Err(StoreError::invalid(
            "`title` en fazla 200 karakter olabilir",
        ));
    }
    let title = title.to_owned();

    let updated = db
        .with_connection(|connection| {
            connection.execute(
                "UPDATE sessions SET title = ?2 WHERE id = ?1",
                params![id, title],
            )
        })
        .map_err(|error| StoreError::storage(error, "session_set_title"))?;

    if updated == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(title)
}

/// Kaydin modalitesi. `None` = boyle bir oturum yok.
///
/// # Neden ayri bir okuma
///
/// [`SessionRecord`] `modality` **tasimiyor** (`model::SESSION_COLUMNS_NOT_LOADED`):
/// o tip `session_start` yanitinin govdesi ve `src/shared/session.ts` onu
/// beklenmeyen alan varsa hata vererek ayristiriyor. `chat_send` ise metin
/// konusmasi ile ses oturumunu ayirt etmek zorunda (Gate 3 / M2), bu yuzden tek
/// kolonluk bu projeksiyon acildi — satir tipini genisletmek IPC sozlesmesini
/// kirardi.
pub(crate) fn modality_of(db: &AsunaDb, id: i64) -> Result<Option<SessionModality>, StoreError> {
    if id <= 0 {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }

    db.with_connection(|connection| {
        connection
            .query_row(
                "SELECT modality FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get::<_, SessionModality>("modality"),
            )
            .optional()
    })
    .map_err(|error| StoreError::storage(error, "session_modality"))
}

/// Boyle bir oturum kaydi var mi?
///
/// `messages`/`attachments` yazma yollari bunu **once** sorar: FK ihlalini
/// bir `Storage` hatasina cevirmek yerine "boyle bir konusma yok" (`NotFound`)
/// demek, cagiranin duzeltebilecegi bir cevaptir.
pub(crate) fn exists(connection: &rusqlite::Connection, id: i64) -> rusqlite::Result<bool> {
    let found: Option<i64> = connection
        .query_row(
            "SELECT id FROM sessions WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
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
///
/// Calisma zamani hafiza anahtari kapaliysa (ASU-037) DB'ye **hic dokunulmaz**
/// ve `skipped` doner: renderer oturum kimligi almaz, dolayisiyla kapanista
/// yazilacak bir kayit da olusmaz.
///
/// # `modality` (Chat Shell)
///
/// Opsiyonel ve varsayilani `voice`. Ses yolu (`session-manager.ts`) bu
/// parametreyi **gondermiyor** ve gondermesi de gerekmiyor — yani mevcut
/// cagiranlar ve `src/shared/session.ts` sozlesmesi degismedi. Metin sohbeti
/// `modality: "text"` gonderir (`chat-service.ts`).
///
/// Yanit bicimi de degismedi: [`SessionRecord`] `title`/`modality` alanlarini
/// **tasimaz** (bkz. `model::SESSION_COLUMNS_NOT_LOADED`); konusma listesinin
/// ihtiyaci olan alanlari `conversation_list` doner.
#[tauri::command]
pub fn session_start(
    state: State<'_, DbState>,
    config: State<'_, AsunaConfig>,
    privacy: State<'_, Arc<PrivacyState>>,
    project_id: Option<String>,
    modality: Option<SessionModality>,
) -> Result<SessionWriteResult, StoreError> {
    if !privacy.memory_enabled() {
        return Ok(SessionWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    }
    let Some(db) = database(&state)? else {
        return Ok(SessionWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    // Model **modaliteye gore** secilir (Gate 3 / L3): metin konusmasini
    // yuruten model `ASUNA_CHAT_MODEL`dir, `sessions.model` de onu yazmali.
    // Aksi halde kayit "bu konusmayi gpt-realtime yapti" derdi — hicbir
    // realtime cagrisi olmamis bir konusma icin yanlis bir kayit ve maliyet
    // analizini de yaniltirdi. Renderer ikisini de secemez; yalnizca
    // modaliteyi soyler.
    let modality = modality.unwrap_or_default();
    let model = match modality {
        SessionModality::Text => &config.chat_model,
        SessionModality::Voice => &config.realtime_model,
    };

    let session = start_with_modality(
        db,
        model,
        project_id.as_deref(),
        modality,
        &clock::now_utc(),
    )?;
    Ok(SessionWriteResult::Recorded {
        session: Box::new(session),
    })
}

/// Konusmanin basligini yazar (Chat Shell).
///
/// Renderer ilk kullanici mesajindan sonra otomatik bir baslik gonderir; ayni
/// komut kullanicinin elle yeniden adlandirmasi icin de kullanilir.
///
/// Hafiza kapaliyken `skipped` doner (hata degil): baslik, kalici bir kaydin
/// alani; kayit yoksa yazilacak bir sey de yok. `memory_delete` ile ayni ayrim
/// gecerli degil cunku bu bir **yazma** islemi.
#[tauri::command]
pub fn session_set_title(
    state: State<'_, DbState>,
    privacy: State<'_, Arc<PrivacyState>>,
    session_id: i64,
    title: String,
) -> Result<SessionTitleResult, StoreError> {
    if !privacy.memory_enabled() {
        return Ok(SessionTitleResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    }
    let Some(db) = database(&state)? else {
        return Ok(SessionTitleResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    let title = set_title(db, session_id, &title)?;
    Ok(SessionTitleResult::Recorded {
        id: session_id,
        title,
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
/// # Gizlilik (ASU-037)
///
/// Calisma zamani hafiza anahtari kapaliysa hicbir sey yazilmaz: ne oturum
/// satiri, ne dokum dosyasi, ne de ozet gorevi. Komut `skipped` doner — bu bir
/// hata degil, kullanicinin karari (`memory_create` ile ayni sozlesme).
#[tauri::command]
pub fn session_finalize<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, DbState>,
    config: State<'_, AsunaConfig>,
    privacy: State<'_, Arc<PrivacyState>>,
    session_id: i64,
    input: Option<SessionFinalizeInput>,
) -> Result<SessionWriteResult, StoreError> {
    if !privacy.memory_enabled() {
        return Ok(SessionWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    }
    let Some(db) = database(&state)? else {
        return Ok(SessionWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    let input = input.unwrap_or_default();
    // Dokum **bir kez** kirpilir: diske yazilan dilim ile ozete giden dilim
    // ayni olmali (aksi halde dosya son 2.000 repligi, ozet ise sinirsiz bir
    // diziyi gorurdu).
    let transcript = clamp_transcript(&input.transcript).to_vec();
    let transcript_path = persist_transcript(&app, &config, session_id, &transcript);

    let session = finalize(
        db,
        session_id,
        &input,
        transcript_path.as_deref(),
        &clock::now_utc(),
    )?;

    // Kapanis yazildi. Ozet bundan **sonra**, arka planda; buradan itibaren
    // hicbir hata oturum kaydini etkilemez.
    summary::spawn_for_session(&app, session.id, transcript);

    Ok(SessionWriteResult::Recorded {
        session: Box::new(session),
    })
}

/// Toplu oturum temizligini onaylayan ifade — kullanicinin **birebir** yazmasi
/// gerekir (ASU-065).
///
/// `memory_repository::DELETE_ALL_CONFIRMATION` ile **bilerek farkli**: iki
/// aksiyonun kapsami farkli ve ayni cumleyi paylasmalari, birini yazip
/// digerini calistirma hatasini mumkun kilardi. Turkce karakter yok —
/// kullanicinin klavye duzeninden bagimsiz yazilabilmeli.
///
/// TypeScript aynasi: `src/shared/session.ts` → `SESSION_CLEAR_ALL_CONFIRMATION`.
pub const CLEAR_ALL_CONFIRMATION: &str = "KONUSMA GECMISINI SIL";

/// Oturum kayitlarini listeler (salt okuma, ASU-065).
///
/// Hafiza kapaliyken **bos sayfa** doner (hata degil) — `memory_list` ile ayni
/// sozlesme. Renderer siralamayi ya da alanlari secemez; yalnizca kac satir
/// istedigini soyleyebilir ve bu istek tavana kirpilir.
#[tauri::command]
pub fn session_list(
    state: State<'_, DbState>,
    query: Option<SessionListQuery>,
) -> Result<SessionPage, StoreError> {
    let limit = query
        .and_then(|query| query.limit)
        .unwrap_or(DEFAULT_SESSION_LIST_LIMIT)
        .clamp(1, MAX_SESSION_LIST_LIMIT);

    let Some(db) = database(&state)? else {
        return Ok(SessionPage {
            sessions: Vec::new(),
            limit,
            limit_max: MAX_SESSION_LIST_LIMIT,
            total: 0,
        });
    };
    list_recent(db, limit)
}

/// Tek oturumu siler: `sessions` satiri + varsa diskteki dokum dosyasi.
///
/// # Neden bu komut var (M3 blokaji)
///
/// M3 kabul testinde kullanici hafiza kayitlarini sildi ama Asuna hatirlamaya
/// devam etti: Stage A her oturum acilisinda **son oturum ozetini** enjekte
/// ediyor ([`latest_completed_summary`]) ve `sessions.summary` silinemiyordu.
/// Bu komut o boslugu kapatir — silinen ozet bir sonraki baglama giremez, cunku
/// baglam onbelleklenmiyor ve her `connect()` oncesi depodan yeniden okunuyor.
///
/// # Gizlilik
///
/// Calisma zamani hafiza anahtarina **bakmaz** (`memory_delete` ile ayni
/// gerekce): kullanici hafizayi kapattiktan sonra da kendi verisini
/// temizleyebilmeli. Anahtar "daha az hatirla" yonunu kapatmaz.
///
/// Dosya yolu renderer'dan gelmez ve renderer'a donmez: `transcript_path`
/// DB'den okunur ve `app_data_dir()/transcripts` altinda oldugu dogrulanir
/// (bkz. [`transcript::delete_recorded_file`]).
#[tauri::command]
pub fn session_delete<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, DbState>,
    session_id: i64,
) -> Result<SessionDeleteResult, StoreError> {
    let Some(db) = database(&state)? else {
        return Ok(SessionDeleteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    let recorded = delete(db, session_id)?;
    Ok(SessionDeleteResult::Deleted {
        id: session_id,
        transcript_file: remove_transcript_file(&app, session_id, recorded.as_deref()),
    })
}

/// **Tum** oturum kayitlarini ve dokum dosyalarini siler (ASU-065).
///
/// Iki kapi vardir ve ikisi de gecilmelidir: UI'daki iki asamali onay ve
/// buradaki [`CLEAR_ALL_CONFIRMATION`] ifadesi. Ifade tutmazsa ne DB'ye ne
/// diske dokunulur.
///
/// # Kapsam — ve neden `memory_delete_all`'dan ayri
///
/// Bu komut `sessions` tablosunu (ozetler dahil) ve `transcripts/` dizinini
/// temizler; `memories` tablosuna **dokunmaz**. Iki aksiyon ayri, cunku
/// kapsamlari ayri: kullanici konusma gecmisini silip cikarilmis hafizalari
/// tutmak isteyebilir (ya da tersi). "Hepsini sildim" deyip bir seyi birakmak
/// en kotu sonuc — bu yuzden her iki ekran da neyin **kapsam disi** oldugunu
/// yaziyor (Gate 3 / MEDIUM-6 karari, M3 testiyle revize edildi).
///
/// Hafiza acilista kapaliysa DB dosyasi hic acilmamistir; o durumda `0` oturum
/// silinir ama **dokum dosyalari yine temizlenir** — onceki bir calismadan
/// kalan dosyalarin silinemez olmasi, anahtari bir tuzaga cevirirdi.
#[tauri::command]
pub fn session_clear_all<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, DbState>,
    confirmation_phrase: String,
) -> Result<SessionPurgeResult, StoreError> {
    if confirmation_phrase != CLEAR_ALL_CONFIRMATION {
        // Mesaj kullanicinin yazdigi metni **tekrarlamaz**.
        return Err(StoreError::invalid(format!(
            "`confirmationPhrase` birebir `{CLEAR_ALL_CONFIRMATION}` olmali"
        )));
    }

    // Ariza (`Unavailable`) burada hata olarak cikar: bozuk bir DB uzerinde
    // "temizledim" demek yanlis olurdu.
    let deleted_sessions = match database(&state)? {
        Some(db) => delete_all(db)?,
        None => 0,
    };

    let purge = match transcript::transcript_dir(&app) {
        Ok(directory) => transcript::purge_directory(&directory),
        Err(error) => {
            eprintln!(
                "[asuna] Dokum dizini cozulemedi: {}",
                super::describe_error_chain(&error)
            );
            transcript::TranscriptPurge::default()
        }
    };

    Ok(SessionPurgeResult::Purged {
        deleted_sessions,
        deleted_files: purge.deleted,
        remaining_files: purge.remaining,
    })
}

/// Dokum dosyasini siler; dizin cozulemezse `Failed` doner ve log'lar.
fn remove_transcript_file<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session_id: i64,
    recorded_path: Option<&str>,
) -> TranscriptFileOutcome {
    if recorded_path.is_none() {
        // GIZLILIK: kayitta dosya yoksa dizin yolu bile cozulmez.
        return TranscriptFileOutcome::NotRecorded;
    }

    match transcript::transcript_dir(app) {
        Ok(directory) => transcript::delete_recorded_file(&directory, session_id, recorded_path),
        Err(error) => {
            eprintln!(
                "[asuna] Dokum dizini cozulemedi: {}",
                super::describe_error_chain(&error)
            );
            TranscriptFileOutcome::Failed
        }
    }
}

/// Transcript'i (ayar aciksa) diske yazar; hata halinde `None` doner ve log'lar.
fn persist_transcript<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    config: &AsunaConfig,
    session_id: i64,
    lines: &[TranscriptLine],
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

    match transcript::persist_if_enabled(true, &directory, session_id, lines) {
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

    // --- listeleme + silme (ASU-065) --------------------------------------

    /// Ozeti yazilmis, temiz kapanmis bir oturum kurar.
    fn completed_session(db: &AsunaDb, summary: &str, transcript_path: Option<&str>) -> i64 {
        let session = start(db, MODEL, None, START).expect("oturum");
        finalize(
            db,
            session.id,
            &SessionFinalizeInput::default(),
            transcript_path,
            END,
        )
        .expect("kapanis");
        attach_summary(db, session.id, summary, None).expect("ozet");
        session.id
    }

    #[test]
    fn lists_the_newest_sessions_with_an_audit_row() {
        let db = fresh_db();
        let first = completed_session(&db, "Ilk oturum: wake word.", None);
        let second = completed_session(
            &db,
            "Ikinci oturum: retrieval.",
            Some("/tmp/asuna/transcripts/session-2.jsonl"),
        );
        let open = start(&db, MODEL, None, START).expect("acik oturum");

        let page = list_recent(&db, DEFAULT_SESSION_LIST_LIMIT).expect("liste");

        assert_eq!(page.total, 3);
        assert_eq!(page.limit, DEFAULT_SESSION_LIST_LIMIT);
        assert_eq!(page.limit_max, MAX_SESSION_LIST_LIMIT);
        // En yeni once; ayni saniyede acilanlarin sirasi `id` ile cozulur.
        assert_eq!(
            page.sessions
                .iter()
                .map(|item| item.id)
                .collect::<Vec<i64>>(),
            [open.id, second, first]
        );

        let listed = &page.sessions[2];
        assert_eq!(
            listed.summary_preview.as_deref(),
            Some("Ilk oturum: wake word.")
        );
        assert!(!listed.summary_truncated);
        assert_eq!(listed.ended_at.as_deref(), Some(END));
        assert_eq!(listed.end_reason, Some(SessionEndReason::Completed));
        assert!(!listed.has_transcript_file);

        assert!(
            page.sessions[1].has_transcript_file,
            "dokum dosyasi olan oturum isaretlenmeli"
        );
        // Acik oturum da gorunur: konusulan oturum kullanicidan gizlenmez.
        assert_eq!(page.sessions[0].ended_at, None);
        assert_eq!(page.sessions[0].summary_preview, None);
    }

    /// Liste **yol tasimaz**: kullanicinin dizin yapisi webview'e gitmez.
    #[test]
    fn the_list_never_carries_the_transcript_path() {
        let db = fresh_db();
        completed_session(
            &db,
            "ozet",
            Some("/Users/kurban/Library/Application Support/asuna/transcripts/session-1.jsonl"),
        );

        let page = list_recent(&db, 10).expect("liste");
        let json = serde_json::to_string(&page).expect("serialize");

        assert!(json.contains("\"hasTranscriptFile\":true"), "yanit: {json}");
        assert!(!json.contains("/Users/kurban"), "yol sizdi: {json}");
        assert!(!json.contains("transcriptPath"), "yanit: {json}");
    }

    #[test]
    fn the_list_limit_is_clamped_and_the_ceiling_is_visible() {
        let db = fresh_db();
        for _ in 0..3 {
            start(&db, MODEL, None, START).expect("oturum");
        }

        let page = list_recent(&db, 2).expect("liste");
        assert_eq!(page.sessions.len(), 2);
        assert_eq!(page.limit, 2);
        assert_eq!(page.total, 3, "tavan asilsa da toplam gorunur");

        // Tavani asan istek reddedilmez, kirpilir — ve kirpildigi gorunur.
        let page = list_recent(&db, 10_000).expect("liste");
        assert_eq!(page.limit, MAX_SESSION_LIST_LIMIT);
        assert_eq!(page.limit_max, MAX_SESSION_LIST_LIMIT);
    }

    #[test]
    fn long_summaries_are_previewed_and_marked_as_truncated() {
        let (preview, truncated) = clamp_summary(Some("kisa ozet"));
        assert_eq!(preview.as_deref(), Some("kisa ozet"));
        assert!(!truncated);

        let long = "a".repeat(SUMMARY_PREVIEW_CHARS + 10);
        let (preview, truncated) = clamp_summary(Some(&long));
        let preview = preview.expect("on izleme");
        assert!(truncated);
        assert_eq!(
            preview.chars().count(),
            SUMMARY_PREVIEW_CHARS + 1,
            "kirpma isareti dahil"
        );

        assert_eq!(clamp_summary(None), (None, false));
        assert_eq!(clamp_summary(Some("   ")), (None, false));
    }

    /// **ASU-065 kabul kriteri**: silinen oturumun ozeti gercekten gider ve
    /// kayitli dokum yolu cagirana doner (dosyayi komut katmani siler).
    #[test]
    fn deleting_a_session_removes_the_row_and_returns_the_recorded_path() {
        let db = fresh_db();
        let path = "/tmp/asuna/transcripts/session-1.jsonl";
        let id = completed_session(&db, "Ozet: wake word yerel kalir.", Some(path));

        let recorded = delete(&db, id).expect("silinmeli");

        assert_eq!(recorded.as_deref(), Some(path));
        assert!(get_by_id(&db, id).expect("okuma").is_none());
        assert_eq!(
            delete(&db, id).expect_err("ikinci silme").code(),
            StoreErrorCode::NotFound
        );
    }

    #[test]
    fn deleting_a_session_without_a_transcript_reports_no_path() {
        let db = fresh_db();
        let id = completed_session(&db, "ozet", None);
        assert_eq!(delete(&db, id).expect("silinmeli"), None);
    }

    #[test]
    fn delete_rejects_non_positive_ids_before_touching_the_database() {
        let db = fresh_db();
        for id in [0_i64, -1] {
            assert_eq!(
                delete(&db, id).expect_err("gecersiz id").code(),
                StoreErrorCode::Invalid
            );
        }
    }

    /// Oturum silmek **hafizayi silmez**: FK `ON DELETE SET NULL`. Kayit durur,
    /// kaynagi "bilinmiyor"a doner — hafizayi silmek ayri bir aksiyondur.
    #[test]
    fn deleting_a_session_keeps_the_memories_it_produced() {
        use crate::db::memory_repository::{self, MemoryDraft};

        let db = fresh_db();
        let id = completed_session(&db, "ozet", None);
        let memory = memory_repository::create(
            &db,
            &MemoryDraft {
                kind: crate::db::MemoryKind::Decision,
                title: "Wake word yerel".to_owned(),
                content: "Cihazda calisir.".to_owned(),
                summary: None,
                project_id: None,
                importance: 0.9,
                confidence: 1.0,
                source_session_id: Some(id),
                expires_at: None,
                metadata_json: None,
            },
            START,
        )
        .expect("hafiza");

        delete(&db, id).expect("oturum silinmeli");

        let kept = memory_repository::get_by_id(&db, memory.id, START, false)
            .expect("okuma")
            .expect("hafiza durmali");
        assert_eq!(kept.source_session_id, None, "kaynak bilinmiyora donmeli");
    }

    #[test]
    fn clearing_all_sessions_empties_the_table_and_reports_the_count() {
        let db = fresh_db();
        completed_session(&db, "bir", None);
        completed_session(&db, "iki", Some("/tmp/asuna/transcripts/session-2.jsonl"));
        start(&db, MODEL, None, START).expect("acik oturum");

        assert_eq!(delete_all(&db).expect("temizlik"), 3);
        assert_eq!(
            list_recent(&db, DEFAULT_SESSION_LIST_LIMIT)
                .expect("liste")
                .total,
            0
        );

        // Bos depoda tekrar cagirmak hata degil, yalnizca 0.
        assert_eq!(delete_all(&db).expect("bos depo"), 0);
    }

    /// **ASU-065 / M3 blokaji**: silinen oturumun ozeti Stage A'ya bir daha
    /// girmez — `latest_completed_summary` bir **onceki** kalan ozete duser.
    #[test]
    fn the_deleted_session_summary_never_comes_back_to_stage_a() {
        let db = fresh_db();
        let older = completed_session(&db, "Eski oturum: sema kararlari.", None);
        let newest = completed_session(&db, "Son oturum: wake word yerel kalir.", None);

        let latest = latest_completed_summary(&db)
            .expect("okuma")
            .expect("ozet olmali");
        assert_eq!(latest.id, newest);

        delete(&db, newest).expect("silinmeli");

        let fallback = latest_completed_summary(&db)
            .expect("okuma")
            .expect("bir onceki ozet kalmali");
        assert_eq!(fallback.id, older);
        assert_eq!(
            fallback.summary.as_deref(),
            Some("Eski oturum: sema kararlari.")
        );

        delete(&db, older).expect("silinmeli");
        assert_eq!(
            latest_completed_summary(&db).expect("okuma"),
            None,
            "hepsi silindiginde tasinacak ozet kalmamali"
        );
    }

    #[test]
    fn the_clear_all_confirmation_phrase_is_exact_and_distinct() {
        assert_eq!(CLEAR_ALL_CONFIRMATION, "KONUSMA GECMISINI SIL");
        // Iki toplu silme aksiyonunun ifadesi ayni olmamali: biri yazilip
        // digeri calistirilamasin.
        assert_ne!(
            CLEAR_ALL_CONFIRMATION,
            crate::db::memory_repository::DELETE_ALL_CONFIRMATION
        );
    }

    #[test]
    fn delete_and_purge_results_serialize_with_an_explicit_status() {
        let json = serde_json::to_value(SessionDeleteResult::Deleted {
            id: 7,
            transcript_file: TranscriptFileOutcome::Deleted,
        })
        .expect("serialize");
        assert_eq!(json["status"], "deleted");
        assert_eq!(json["id"], 7);
        assert_eq!(json["transcriptFile"], "deleted");

        let json = serde_json::to_value(SessionPurgeResult::Purged {
            deleted_sessions: 4,
            deleted_files: 2,
            remaining_files: 1,
        })
        .expect("serialize");
        assert_eq!(json["status"], "purged");
        assert_eq!(json["deletedSessions"], 4);
        assert_eq!(json["deletedFiles"], 2);
        assert_eq!(json["remainingFiles"], 1);

        let json = serde_json::to_value(SessionDeleteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        })
        .expect("serialize");
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["reason"], "memory-disabled");
    }

    /// Renderer liste sorgusuna kendi alanlarini ekleyemez (siralama, proje,
    /// yol...): sozlesme kapali.
    #[test]
    fn unknown_list_query_fields_are_rejected_at_the_ipc_boundary() {
        assert!(serde_json::from_str::<SessionListQuery>(r#"{"orderBy":"summary"}"#).is_err());
        assert!(
            serde_json::from_str::<SessionListQuery>(r#"{"transcriptPath":"/etc/passwd"}"#)
                .is_err()
        );

        let parsed: SessionListQuery = serde_json::from_str(r#"{"limit":10}"#).expect("gecerli");
        assert_eq!(parsed.limit, Some(10));
        assert_eq!(
            serde_json::from_str::<SessionListQuery>("{}")
                .expect("gecerli")
                .limit,
            None
        );
    }

    // --- Chat Shell: modalite + baslik (migration 006) -----------------------

    /// Modalite verilmeden acilan oturum bir **ses** oturumudur — mevcut
    /// cagiranlarin davranisi degismedi.
    #[test]
    fn a_session_started_without_a_modality_is_a_voice_session() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum acilmali");

        let modality: String = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT modality FROM sessions WHERE id = ?1",
                    params![session.id],
                    |row| row.get(0),
                )
            })
            .expect("modalite okunmali");
        assert_eq!(modality, "voice");
        assert_eq!(SessionModality::default(), SessionModality::Voice);
    }

    #[test]
    fn a_text_conversation_records_its_modality() {
        let db = fresh_db();
        let session = start_with_modality(&db, "gpt-4o-mini", None, SessionModality::Text, START)
            .expect("konusma acilmali");

        let modality: SessionModality = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT modality FROM sessions WHERE id = ?1",
                    params![session.id],
                    |row| row.get(0),
                )
            })
            .expect("modalite okunmali");
        assert_eq!(modality, SessionModality::Text);
    }

    /// **Sozlesme kapisi**: `SessionRecord` yaniti Chat Shell kolonlarini
    /// tasimaz. `src/shared/session.ts` beklenmeyen alanda hata firlatiyor —
    /// buraya bir alan eklemek calisan ses yolunu IPC sinirinde kirardi.
    #[test]
    fn the_session_record_payload_did_not_grow_with_the_chat_columns() {
        let db = fresh_db();
        let session = start_with_modality(&db, MODEL, None, SessionModality::Text, START)
            .expect("oturum acilmali");

        let json = serde_json::to_value(&session).expect("serialize");
        let object = json.as_object().expect("JSON nesnesi");
        assert!(!object.contains_key("title"), "yanit: {json}");
        assert!(!object.contains_key("modality"), "yanit: {json}");
        assert_eq!(object.len(), crate::db::model::SESSION_COLUMNS.len());
    }

    #[test]
    fn sets_and_overwrites_the_conversation_title() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");

        assert_eq!(
            set_title(&db, session.id, "  Ilk konusma  ").expect("baslik yazilmali"),
            "Ilk konusma",
            "bastaki/sondaki bosluk kirpilmali"
        );

        set_title(&db, session.id, "Yeniden adlandirildi").expect("baslik degismeli");

        let title: Option<String> = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT title FROM sessions WHERE id = ?1",
                    params![session.id],
                    |row| row.get(0),
                )
            })
            .expect("baslik okunmali");
        assert_eq!(title.as_deref(), Some("Yeniden adlandirildi"));
    }

    #[test]
    fn a_new_conversation_has_no_title() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");

        let title: Option<String> = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT title FROM sessions WHERE id = ?1",
                    params![session.id],
                    |row| row.get(0),
                )
            })
            .expect("baslik okunmali");
        assert_eq!(
            title, None,
            "baslik uydurulmamali (UI 'Adsiz konusma' yazar)"
        );
    }

    /// Bos ve asiri uzun baslik **reddedilir** (kirpilmaz) — gerekce
    /// `set_title` dokumantasyonunda.
    #[test]
    fn an_empty_or_oversized_title_is_rejected() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");

        for title in ["", "   ", "\n\t"] {
            assert_eq!(
                set_title(&db, session.id, title)
                    .expect_err("bos baslik reddedilmeli")
                    .code(),
                StoreErrorCode::Invalid
            );
        }

        let too_long = "b".repeat(MAX_SESSION_TITLE_CHARS + 1);
        assert_eq!(
            set_title(&db, session.id, &too_long)
                .expect_err("uzun baslik reddedilmeli")
                .code(),
            StoreErrorCode::Invalid
        );

        // Tam tavan gecerli olmali (sinir kapali araliktir).
        let exact = "b".repeat(MAX_SESSION_TITLE_CHARS);
        set_title(&db, session.id, &exact).expect("tam tavandaki baslik kabul edilmeli");

        // Reddedilen hicbir istek kaydi degistirmedi.
        let title: Option<String> = db
            .with_connection(|conn| {
                conn.query_row(
                    "SELECT title FROM sessions WHERE id = ?1",
                    params![session.id],
                    |row| row.get(0),
                )
            })
            .expect("baslik okunmali");
        assert_eq!(title, Some(exact));
    }

    #[test]
    fn setting_the_title_of_an_unknown_conversation_reports_not_found() {
        let db = fresh_db();
        assert_eq!(
            set_title(&db, 4242, "yok")
                .expect_err("bilinmeyen konusma")
                .code(),
            StoreErrorCode::NotFound
        );
        assert_eq!(
            set_title(&db, 0, "gecersiz")
                .expect_err("gecersiz kimlik")
                .code(),
            StoreErrorCode::Invalid
        );
    }

    #[test]
    fn the_title_result_is_tagged_on_the_wire() {
        let json = serde_json::to_value(SessionTitleResult::Recorded {
            id: 3,
            title: "Ilk konusma".to_owned(),
        })
        .expect("serialize");
        assert_eq!(json["status"], "recorded");
        assert_eq!(json["id"], 3);
        assert_eq!(json["title"], "Ilk konusma");

        let json = serde_json::to_value(SessionTitleResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        })
        .expect("serialize");
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["reason"], "memory-disabled");
    }

    #[test]
    fn exists_separates_a_live_conversation_from_a_deleted_one() {
        let db = fresh_db();
        let session = start(&db, MODEL, None, START).expect("oturum");

        let (before, after) = db
            .with_connection(|conn| {
                let before = exists(conn, session.id)?;
                conn.execute("DELETE FROM sessions WHERE id = ?1", params![session.id])?;
                let after = exists(conn, session.id)?;
                Ok((before, after))
            })
            .expect("sorgu calismali");

        assert!(before);
        assert!(!after);
    }
}

//! `memories` CRUD + kaba taneli komutlar (ASU-031).
//!
//! # Sozlesme
//!
//! - **Ham SQL bu modulun disina cikmaz.** Renderer `memory_create`,
//!   `memory_list`, `memory_update`, `memory_archive`, `memory_delete`
//!   komutlarini cagirir; her SQL sorgusu icin ayri komut acilmaz (ADR-005
//!   "komutlar kaba taneli").
//! - **Dogrulama IPC sinirinda.** Gecersiz `kind` serde'de, gecersiz aralik ve
//!   bos metin burada, geri kalan her sey semadaki CHECK kisitlarinda duser.
//!   Uc kat da bilerek duruyor: biri unutulursa digerleri tutar.
//! - **Hafiza kapaliyken yazma no-op, okuma bos.** `ASUNA_MEMORY_ENABLED=false`
//!   iken DB dosyasi hic acilmaz ([`DbState::Disabled`]); komutlar `Ok` doner ve
//!   yazmanin atlandigini [`MemoryWriteResult::Skipped`] ile **acikca** soyler —
//!   renderer "kaydettim" sanmaz.
//! - **Calisma zamani anahtari yalnizca "daha fazla hatirla" yonunu kapatir**
//!   (ASU-037). [`crate::privacy::PrivacyState`] kapaliyken [`memory_create`] ve
//!   [`memory_update`] `Skipped` doner; [`memory_archive`], [`memory_delete`] ve
//!   [`memory_delete_all`] **calismaya devam eder**. Gerekce: kullanici hafizayi
//!   kapattiktan sonra da var olan kayitlarini gorup silebilmeli — aksi halde
//!   anahtar, kendi verisini temizlemesini engelleyen bir tuzaga donusurdu
//!   (PROJECT.md Bolum 20).
//! - **Ariza sessizce yutulmaz.** `DbState::Unavailable` iken okuma da yazma da
//!   `unavailable` kodlu hata doner (PROJECT.md Bolum 30).
//!
//! # Erisim izi ve yaslandirma
//!
//! - `last_accessed_at` yalnizca **erisim** oldugunda guncellenir. "Erisim" =
//!   bir kaydin gercekten okunup kullanilmasi (Stage A retrieval, tek kayit
//!   acma). Memory UI'inda listeyi karistirmak erisim sayilmaz: her render'da
//!   tum satirlarin erisim zamanini ezmek hem alani anlamsizlastirir hem
//!   goruntuleme sirasinda yazma uretir. Karar cagirana birakildi:
//!   [`MemoryFilter::mark_accessed`].
//! - `expires_at` gecmis kayitlar retrieval'da **donmez**. Silinmezler:
//!   temizlik politikasi ayri bir is (memory.md T7). Kullanici onlari yine de
//!   gorup silebilsin diye [`MemoryFilter::include_expired`] var.

use std::sync::Arc;

use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize};
use tauri::State;

use crate::privacy::PrivacyState;

use super::clock;
use super::model::{MemoryKind, MemoryRecord};
use super::store_error::{database, StoreError, StoreSkipReason};
use super::{AsunaDb, DbState};

// ---------------------------------------------------------------------------
// Sinirlar
// ---------------------------------------------------------------------------

/// `limit` verilmediginde donen kayit sayisi.
pub const DEFAULT_LIST_LIMIT: u32 = 50;

/// `limit` icin tavan. Renderer daha buyugunu isterse **kirpilir**: sinirsiz bir
/// liste hem UI'i dondurur hem tum hafizayi tek IPC mesajina koyar.
pub const MAX_LIST_LIMIT: u32 = 200;

const MAX_TITLE_CHARS: usize = 200;
const MAX_CONTENT_CHARS: usize = 8_000;
const MAX_SUMMARY_CHARS: usize = 1_000;
const MAX_PROJECT_ID_CHARS: usize = 120;
const MAX_METADATA_CHARS: usize = 4_000;
const MAX_SEARCH_CHARS: usize = 120;

/// `metadata_json` icin varsayilan — semadaki DEFAULT ile ayni.
const EMPTY_METADATA: &str = "{}";

// ---------------------------------------------------------------------------
// Girdi tipleri
// ---------------------------------------------------------------------------

/// Yeni hafiza kaydi. `deny_unknown_fields`: sozlesmede olmayan bir alan
/// sessizce yutulmaz (TS tarafindaki `assertNoUnexpectedKeys` ile ayni disiplin).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryDraft {
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    pub importance: f64,
    pub confidence: f64,
    /// "Bu neden hatirlaniyor?" izinin kaynagi (memory.md Bolum 2).
    #[serde(default)]
    pub source_session_id: Option<i64>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
}

/// Kismi guncelleme.
///
/// Nullable alanlarda **uc durum** var ve ucu de ayirt edilir:
/// alan yok = dokunma · `null` = temizle · deger = ata. Bu yuzden
/// `Option<Option<T>>` (bkz. [`explicit_option`]); tek `Option` ile "temizle"
/// istegi sessizce "dokunma"ya donusurdu.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryPatch {
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, deserialize_with = "explicit_option")]
    pub summary: Option<Option<String>>,
    #[serde(default, deserialize_with = "explicit_option")]
    pub project_id: Option<Option<String>>,
    #[serde(default)]
    pub importance: Option<f64>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default, deserialize_with = "explicit_option")]
    pub expires_at: Option<Option<String>>,
    #[serde(default)]
    pub metadata_json: Option<String>,
}

/// `null` ile "alan yok"u ayirt eden deserializer.
fn explicit_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::deserialize(deserializer).map(Some)
}

/// Arsiv filtresi. `Option<bool>` yerine enum: JSON'da `null` ile "alan yok"
/// karismasin ve **varsayilan guvenli** olsun (arsivlenmis kayitlar gelmez).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveFilter {
    /// Yalnizca arsivlenmemis kayitlar (varsayilan).
    #[default]
    Active,
    /// Yalnizca arsivlenmis kayitlar.
    Archived,
    /// Hepsi — Memory UI'inin "arsivi de goster" gorunumu.
    All,
}

/// Siralama. Her secenek semadaki bir index'e karsilik gelir (ASU-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemorySort {
    /// En yeni once (`idx_memories_created_at`).
    #[default]
    Recent,
    /// En eski once.
    Oldest,
    /// Onem, esitlikte tazelik (`idx_memories_stage_a`) — Stage A'nin sirasi.
    Importance,
}

/// Liste filtresi. Tum alanlar opsiyonel; varsayilanlar **retrieval icin
/// guvenli** tarafta (arsivli yok, suresi dolmus yok, erisim izi birakilmaz).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryFilter {
    /// Tek kayit getirmek icin (`getById`). Ayri bir komut acmamak adina
    /// filtrenin bir boyutu — bkz. modul dokumantasyonu.
    #[serde(default)]
    pub id: Option<i64>,
    /// Bos = tum tipler.
    #[serde(default)]
    pub kinds: Vec<MemoryKind>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub archived: ArchiveFilter,
    /// `title` / `content` / `summary` icinde alt dize aramasi.
    #[serde(default)]
    pub search: Option<String>,
    /// `true` ise suresi dolmus kayitlar da doner (inceleme/silme icin).
    #[serde(default)]
    pub include_expired: bool,
    /// `true` ise kullanici onayi bekleyen kayitlar **elenir** (ASU-034/ASU-035).
    ///
    /// Varsayilan `false` bilincli: Memory UI onay bekleyenleri **gormeye devam
    /// etmeli** (kullanici onlari inceleyip onaylayabilsin/silebilsin,
    /// PROJECT.md Bolum 20). Stage A retrieval ise `true` verir — onaylanmamis
    /// bir hafiza modelin baglamina girmez.
    #[serde(default)]
    pub exclude_pending_approval: bool,
    #[serde(default)]
    pub sort: MemorySort,
    #[serde(default)]
    pub limit: Option<u32>,
    /// `true` ise donen kayitlarin `last_accessed_at` degeri guncellenir.
    #[serde(default)]
    pub mark_accessed: bool,
}

// ---------------------------------------------------------------------------
// Cikti tipleri
// ---------------------------------------------------------------------------

/// Yazma isleminin sonucu.
///
/// `Skipped` bir hata degil: hafiza kapaliyken islem yapilmadi ve bu **gorunur**
/// olmali. Sessizce `Stored` donmek "kaydettim" yalanini uretirdi.
///
/// `Box<MemoryRecord>`: varyantlar arasindaki boyut farki (bir kayit ~264 bayt,
/// digerleri 8 bayt) enum'un tamamini sisirirdi. `serde` icin fark yok — JSON
/// bicimi ayni.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum MemoryWriteResult {
    Stored { record: Box<MemoryRecord> },
    Deleted { id: i64 },
    Skipped { reason: StoreSkipReason },
}

/// Toplu silmenin sonucu (ASU-037).
///
/// Ayri bir tip cunku sonuc **sayi**dir: kullanici "gercekten gitti mi?"
/// sorusunun cevabini gormeli. [`MemoryWriteResult::Deleted`] tek bir `id`
/// tasir ve buraya uymaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum MemoryPurgeResult {
    Purged { deleted: u32 },
    Skipped { reason: StoreSkipReason },
}

// ---------------------------------------------------------------------------
// Dogrulama
// ---------------------------------------------------------------------------

/// Bos olmayan, kirpilmis metin.
fn required_text(field: &'static str, raw: &str, max: usize) -> Result<String, StoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(StoreError::invalid(format!("`{field}` bos birakilamaz")));
    }
    if trimmed.chars().count() > max {
        return Err(StoreError::invalid(format!(
            "`{field}` en fazla {max} karakter olabilir"
        )));
    }
    Ok(trimmed.to_owned())
}

/// Opsiyonel metin: bos/bosluk "verilmedi" sayilir (semadaki
/// `length(...) > 0` CHECK'i ile uyumlu).
fn optional_text(
    field: &'static str,
    raw: Option<&str>,
    max: usize,
) -> Result<Option<String>, StoreError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some(value) => Ok(Some(required_text(field, value, max)?)),
    }
}

fn unit_interval(field: &'static str, value: f64) -> Result<f64, StoreError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(StoreError::invalid(format!(
            "`{field}` 0 ile 1 arasinda (dahil) bir sayi olmali"
        )));
    }
    Ok(value)
}

fn timestamp(field: &'static str, raw: &str) -> Result<String, StoreError> {
    if !clock::is_utc_iso8601(raw) {
        return Err(StoreError::invalid(format!(
            "`{field}` UTC ISO-8601 olmali (orn. 2026-08-25T10:00:00Z)"
        )));
    }
    Ok(raw.to_owned())
}

fn metadata_json(raw: Option<&str>) -> Result<String, StoreError> {
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(EMPTY_METADATA.to_owned());
    };
    if value.chars().count() > MAX_METADATA_CHARS {
        return Err(StoreError::invalid(format!(
            "`metadataJson` en fazla {MAX_METADATA_CHARS} karakter olabilir"
        )));
    }
    // Semada `json_valid(metadata_json)` CHECK'i var; burada da dogrulaniyor ki
    // hata mesaji "veritabani islemi basarisiz" degil, alan adini soyleyen bir
    // mesaj olsun.
    if serde_json::from_str::<serde_json::Value>(value).is_err() {
        return Err(StoreError::invalid("`metadataJson` gecerli JSON olmali"));
    }
    Ok(value.to_owned())
}

fn row_id(field: &'static str, value: i64) -> Result<i64, StoreError> {
    if value <= 0 {
        return Err(StoreError::invalid(format!(
            "`{field}` pozitif bir kayit kimligi olmali"
        )));
    }
    Ok(value)
}

/// Dogrulanmis, normalize edilmis draft.
struct NormalizedDraft {
    kind: MemoryKind,
    title: String,
    content: String,
    summary: Option<String>,
    project_id: Option<String>,
    importance: f64,
    confidence: f64,
    source_session_id: Option<i64>,
    expires_at: Option<String>,
    metadata_json: String,
}

impl MemoryDraft {
    fn normalize(&self) -> Result<NormalizedDraft, StoreError> {
        Ok(NormalizedDraft {
            kind: self.kind,
            title: required_text("title", &self.title, MAX_TITLE_CHARS)?,
            content: required_text("content", &self.content, MAX_CONTENT_CHARS)?,
            summary: optional_text("summary", self.summary.as_deref(), MAX_SUMMARY_CHARS)?,
            project_id: optional_text(
                "projectId",
                self.project_id.as_deref(),
                MAX_PROJECT_ID_CHARS,
            )?,
            importance: unit_interval("importance", self.importance)?,
            confidence: unit_interval("confidence", self.confidence)?,
            source_session_id: self
                .source_session_id
                .map(|id| row_id("sourceSessionId", id))
                .transpose()?,
            expires_at: self
                .expires_at
                .as_deref()
                .map(|value| timestamp("expiresAt", value))
                .transpose()?,
            metadata_json: metadata_json(self.metadata_json.as_deref())?,
        })
    }
}

/// `SET` atamalari: (kolon, deger). Kolon adlari **sabit metin**, kullanici
/// girdisi SQL'e hicbir zaman metin olarak girmez.
type Assignments = Vec<(&'static str, Value)>;

impl MemoryPatch {
    fn normalize(&self) -> Result<Assignments, StoreError> {
        let mut set: Assignments = Vec::new();

        if let Some(kind) = self.kind {
            set.push(("kind", Value::Text(kind.as_str().to_owned())));
        }
        if let Some(title) = self.title.as_deref() {
            set.push((
                "title",
                Value::Text(required_text("title", title, MAX_TITLE_CHARS)?),
            ));
        }
        if let Some(content) = self.content.as_deref() {
            set.push((
                "content",
                Value::Text(required_text("content", content, MAX_CONTENT_CHARS)?),
            ));
        }
        if let Some(summary) = self.summary.as_ref() {
            set.push((
                "summary",
                nullable_text("summary", summary.as_deref(), MAX_SUMMARY_CHARS)?,
            ));
        }
        if let Some(project_id) = self.project_id.as_ref() {
            set.push((
                "project_id",
                nullable_text("projectId", project_id.as_deref(), MAX_PROJECT_ID_CHARS)?,
            ));
        }
        if let Some(importance) = self.importance {
            set.push((
                "importance",
                Value::Real(unit_interval("importance", importance)?),
            ));
        }
        if let Some(confidence) = self.confidence {
            set.push((
                "confidence",
                Value::Real(unit_interval("confidence", confidence)?),
            ));
        }
        if let Some(expires_at) = self.expires_at.as_ref() {
            set.push((
                "expires_at",
                match expires_at.as_deref() {
                    None => Value::Null,
                    Some(value) => Value::Text(timestamp("expiresAt", value)?),
                },
            ));
        }
        if let Some(metadata) = self.metadata_json.as_deref() {
            set.push(("metadata_json", Value::Text(metadata_json(Some(metadata))?)));
        }

        if set.is_empty() {
            return Err(StoreError::invalid(
                "guncellenecek en az bir alan verilmeli",
            ));
        }
        Ok(set)
    }
}

fn nullable_text(field: &'static str, raw: Option<&str>, max: usize) -> Result<Value, StoreError> {
    Ok(match optional_text(field, raw, max)? {
        None => Value::Null,
        Some(value) => Value::Text(value),
    })
}

// ---------------------------------------------------------------------------
// Sorgu kurulumu
// ---------------------------------------------------------------------------

/// Dinamik `SELECT` — metin **yalnizca sabitlerden** kurulur, degerler her zaman
/// baglanan parametredir (`?`).
struct ListQuery {
    sql: String,
    params: Vec<Value>,
}

/// LIKE deseninde `%`, `_` ve kacis karakteri kullanicinin aradigi **harf**
/// olmali; joker olarak yorumlanmamali.
fn like_pattern(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len() + 2);
    for character in term.chars() {
        if matches!(character, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    format!("%{escaped}%")
}

fn build_list_query(filter: &MemoryFilter, now: &str) -> Result<ListQuery, StoreError> {
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();

    if let Some(id) = filter.id {
        conditions.push("id = ?".to_owned());
        params.push(Value::Integer(row_id("id", id)?));
    }

    match filter.archived {
        ArchiveFilter::Active => conditions.push("is_archived = 0".to_owned()),
        ArchiveFilter::Archived => conditions.push("is_archived = 1".to_owned()),
        ArchiveFilter::All => {}
    }

    if !filter.kinds.is_empty() {
        let placeholders = vec!["?"; filter.kinds.len()].join(", ");
        conditions.push(format!("kind IN ({placeholders})"));
        for kind in &filter.kinds {
            params.push(Value::Text(kind.as_str().to_owned()));
        }
    }

    if let Some(project_id) = optional_text(
        "projectId",
        filter.project_id.as_deref(),
        MAX_PROJECT_ID_CHARS,
    )? {
        conditions.push("project_id = ?".to_owned());
        params.push(Value::Text(project_id));
    }

    if let Some(term) = optional_text("search", filter.search.as_deref(), MAX_SEARCH_CHARS)? {
        // NOT: SQLite'in `LIKE` buyuk/kucuk harf esitligi yalnizca ASCII icin
        // gecerlidir; "Ismet"/"ismet" gibi Turkce'ye ozgu katlama yapilmaz.
        // Bu bilincli bir MVP siniri — dogru cozum FTS5/ICU, backlog'da.
        conditions.push(
            "(title LIKE ? ESCAPE '\\' OR content LIKE ? ESCAPE '\\' \
             OR summary LIKE ? ESCAPE '\\')"
                .to_owned(),
        );
        let pattern = like_pattern(&term);
        for _ in 0..3 {
            params.push(Value::Text(pattern.clone()));
        }
    }

    if !filter.include_expired {
        conditions.push("(expires_at IS NULL OR expires_at > ?)".to_owned());
        params.push(Value::Text(now.to_owned()));
    }

    if filter.exclude_pending_approval {
        // `IS NOT 1`, `= 0` DEGIL. Elle (Memory UI) olusturulan kayitlarda
        // anahtar hic yoktur ve `json_extract` `NULL` doner; `= 0` bu kayitlarin
        // hepsini sessizce elerdi — oysa kullanicinin kendi yazdigi hafiza onay
        // beklemez. SQLite'ta `NULL IS NOT 1` -> 1 (uc degerli mantik degil).
        conditions.push(format!(
            "json_extract(metadata_json, '$.{}') IS NOT 1",
            crate::extraction::PENDING_APPROVAL_KEY
        ));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };

    // `id` tie-breaker: zaman damgalari saniye hassasiyetinde (bkz. `clock`),
    // ayni saniyede yazilan kayitlarin sirasi yoksa liste her sorguda kayabilir.
    let order = match filter.sort {
        MemorySort::Recent => "created_at DESC, id DESC",
        MemorySort::Oldest => "created_at ASC, id ASC",
        MemorySort::Importance => "importance DESC, created_at DESC, id DESC",
    };

    let limit = filter
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .min(MAX_LIST_LIMIT);
    params.push(Value::Integer(i64::from(limit)));

    Ok(ListQuery {
        sql: format!(
            "SELECT {} FROM memories{where_clause} ORDER BY {order} LIMIT ?",
            MemoryRecord::select_columns()
        ),
        params,
    })
}

/// Tek kaydi id ile okur (filtre uygulanmaz — yazma sonrasi geri okuma icin).
fn load(connection: &Connection, id: i64) -> rusqlite::Result<Option<MemoryRecord>> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM memories WHERE id = ?1",
                MemoryRecord::select_columns()
            ),
            params![id],
            MemoryRecord::from_row,
        )
        .optional()
}

/// Donen kayitlarin erisim zamanini gunceller (tek `UPDATE`).
fn mark_accessed(
    connection: &Connection,
    records: &mut [MemoryRecord],
    now: &str,
) -> rusqlite::Result<()> {
    if records.is_empty() {
        return Ok(());
    }

    let placeholders = vec!["?"; records.len()].join(", ");
    let mut params: Vec<Value> = Vec::with_capacity(records.len() + 1);
    params.push(Value::Text(now.to_owned()));
    params.extend(records.iter().map(|record| Value::Integer(record.id)));

    connection.execute(
        &format!("UPDATE memories SET last_accessed_at = ? WHERE id IN ({placeholders})"),
        rusqlite::params_from_iter(params.iter()),
    )?;

    for record in records.iter_mut() {
        record.last_accessed_at = Some(now.to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

/// Yeni hafiza kaydi olusturur ve **yazilan** hali geri okur.
///
/// `now` cagiran tarafindan verilir ([`clock::now_utc`]); testler sabit bir
/// zaman gecirerek deterministik kalir.
pub fn create(db: &AsunaDb, draft: &MemoryDraft, now: &str) -> Result<MemoryRecord, StoreError> {
    let draft = draft.normalize()?;
    let now = timestamp("now", now)?;

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT INTO memories
                   (kind, title, content, summary, project_id, importance, confidence,
                    source_session_id, created_at, updated_at, expires_at, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?11)",
                params![
                    draft.kind,
                    draft.title,
                    draft.content,
                    draft.summary,
                    draft.project_id,
                    draft.importance,
                    draft.confidence,
                    draft.source_session_id,
                    now,
                    draft.expires_at,
                    draft.metadata_json,
                ],
            )?;
            let id = transaction.last_insert_rowid();
            let record = load(&transaction, id)?;
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "memory_create"))?;

    // INSERT basarili olduysa satir vardir; yoksa sema ile kod kaymis demektir.
    record.ok_or(StoreError::NotFound)
}

/// Filtreye uyan kayitlari dondurur.
pub fn list(
    db: &AsunaDb,
    filter: &MemoryFilter,
    now: &str,
) -> Result<Vec<MemoryRecord>, StoreError> {
    let now = timestamp("now", now)?;
    let query = build_list_query(filter, &now)?;
    let touch = filter.mark_accessed;

    db.with_connection(|connection| {
        let transaction = connection.transaction()?;

        let mut records = {
            let mut statement = transaction.prepare(&query.sql)?;
            let rows = statement.query_map(
                rusqlite::params_from_iter(query.params.iter()),
                MemoryRecord::from_row,
            )?;
            rows.collect::<rusqlite::Result<Vec<MemoryRecord>>>()?
        };

        if touch {
            mark_accessed(&transaction, &mut records, &now)?;
        }

        transaction.commit()?;
        Ok(records)
    })
    .map_err(|error| StoreError::storage(error, "memory_list"))
}

/// Tek kaydi kimligiyle getirir.
///
/// Arsiv/expiry filtresi **uygulanmaz**: kullanici acikca bu kaydi istedi.
/// Filtreleme retrieval'in isi ([`list`]).
pub fn get_by_id(
    db: &AsunaDb,
    id: i64,
    now: &str,
    touch: bool,
) -> Result<Option<MemoryRecord>, StoreError> {
    let filter = MemoryFilter {
        id: Some(id),
        archived: ArchiveFilter::All,
        include_expired: true,
        limit: Some(1),
        mark_accessed: touch,
        ..MemoryFilter::default()
    };
    Ok(list(db, &filter, now)?.into_iter().next())
}

/// Verilen alanlari gunceller; `updated_at` her zaman tazelenir.
pub fn update(
    db: &AsunaDb,
    id: i64,
    patch: &MemoryPatch,
    now: &str,
) -> Result<MemoryRecord, StoreError> {
    let id = row_id("id", id)?;
    let assignments = patch.normalize()?;
    let now = timestamp("now", now)?;

    let clause = assignments
        .iter()
        .map(|(column, _)| format!("{column} = ?"))
        .collect::<Vec<String>>()
        .join(", ");

    let mut params: Vec<Value> = assignments.into_iter().map(|(_, value)| value).collect();
    params.push(Value::Text(now));
    params.push(Value::Integer(id));

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            let changed = transaction.execute(
                &format!("UPDATE memories SET {clause}, updated_at = ? WHERE id = ?"),
                rusqlite::params_from_iter(params.iter()),
            )?;
            let record = if changed == 0 {
                None
            } else {
                load(&transaction, id)?
            };
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "memory_update"))?;

    record.ok_or(StoreError::NotFound)
}

/// Arsivler / arsivden cikarir.
///
/// Arsivleme varsayilan "kaldirma" yolu; gercek silme de destekleniyor
/// ([`delete`]) cunku hafiza kullanici tarafindan **gercekten** silinebilmeli
/// (PROJECT.md Bolum 20).
pub fn set_archived(
    db: &AsunaDb,
    id: i64,
    archived: bool,
    now: &str,
) -> Result<MemoryRecord, StoreError> {
    let id = row_id("id", id)?;
    let now = timestamp("now", now)?;

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            let changed = transaction.execute(
                "UPDATE memories SET is_archived = ?1, updated_at = ?2 WHERE id = ?3",
                params![i64::from(archived), now, id],
            )?;
            let record = if changed == 0 {
                None
            } else {
                load(&transaction, id)?
            };
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "memory_archive"))?;

    record.ok_or(StoreError::NotFound)
}

/// Kaydi kalici olarak siler.
pub fn delete(db: &AsunaDb, id: i64) -> Result<(), StoreError> {
    let id = row_id("id", id)?;

    let changed = db
        .with_connection(|connection| {
            connection.execute("DELETE FROM memories WHERE id = ?1", params![id])
        })
        .map_err(|error| StoreError::storage(error, "memory_delete"))?;

    if changed == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// **Tum** hafiza kayitlarini siler ve silinen sayiyi dondurur.
///
/// Silinen satirlarin serbest kalan SQLite sayfalari dosyada kalmasin diye
/// ardindan `VACUUM` denenir: bu bir gizlilik aksiyonu, "listeden kalksin"
/// degil. `VACUUM` basarisiz olursa (kilit, disk) islem **basarili sayilir** —
/// satirlar zaten gitti; artik sayfalar bir sonraki yazmada ustune yazilir.
/// Sessiz yutma yok: hata yerel log'a duser.
pub fn delete_all(db: &AsunaDb) -> Result<u32, StoreError> {
    let deleted = db
        .with_connection(|connection| connection.execute("DELETE FROM memories", []))
        .map_err(|error| StoreError::storage(error, "memory_delete_all"))?;

    if let Err(error) = db.with_connection(|connection| connection.execute_batch("VACUUM")) {
        eprintln!("[asuna] Toplu silme sonrasi VACUUM basarisiz: {error}");
    }

    Ok(u32::try_from(deleted).unwrap_or(u32::MAX))
}

// ---------------------------------------------------------------------------
// Komutlar (kaba taneli — her SQL icin ayri komut yok)
// ---------------------------------------------------------------------------

/// Toplu silmeyi onaylayan ifade — kullanicinin **birebir** yazmasi gerekir.
///
/// Neden bir parametre: cift onay yalnizca UI'da yasarsa, komut hala tek bir
/// yanlis `invoke` ile tum hafizayi silebilir. Ifade komut imzasinin parcasi
/// olunca "yanlislikla cagirma" yolu kapanir (ASU-037).
///
/// TypeScript aynasi: `src/shared/memory.ts` → `MEMORY_DELETE_ALL_CONFIRMATION`.
pub const DELETE_ALL_CONFIRMATION: &str = "TUM HAFIZAYI SIL";

/// Yeni hafiza kaydi yazar.
#[tauri::command]
pub fn memory_create(
    state: State<'_, DbState>,
    privacy: State<'_, Arc<PrivacyState>>,
    draft: MemoryDraft,
) -> Result<MemoryWriteResult, StoreError> {
    if !privacy.memory_enabled() {
        return Ok(MemoryWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    }
    let Some(db) = database(&state)? else {
        return Ok(MemoryWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };
    Ok(MemoryWriteResult::Stored {
        record: Box::new(create(db, &draft, &clock::now_utc())?),
    })
}

/// Hafiza kayitlarini filtreleyerek listeler. Hafiza kapaliyken **bos** doner.
#[tauri::command]
pub fn memory_list(
    state: State<'_, DbState>,
    filter: Option<MemoryFilter>,
) -> Result<Vec<MemoryRecord>, StoreError> {
    let Some(db) = database(&state)? else {
        return Ok(Vec::new());
    };
    list(db, &filter.unwrap_or_default(), &clock::now_utc())
}

/// Var olan bir kaydin alanlarini gunceller.
///
/// Kalici hafiza calisma zamaninda kapaliysa `Skipped` doner: guncelleme de bir
/// "hatirlama" yazimidir (onay bekleyen bir kaydi onaylamak dahil).
#[tauri::command]
pub fn memory_update(
    state: State<'_, DbState>,
    privacy: State<'_, Arc<PrivacyState>>,
    id: i64,
    patch: MemoryPatch,
) -> Result<MemoryWriteResult, StoreError> {
    if !privacy.memory_enabled() {
        return Ok(MemoryWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    }
    let Some(db) = database(&state)? else {
        return Ok(MemoryWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };
    Ok(MemoryWriteResult::Stored {
        record: Box::new(update(db, id, &patch, &clock::now_utc())?),
    })
}

/// Kaydi arsivler ya da arsivden cikarir.
///
/// Calisma zamani gizlilik anahtarina **bakmaz**: arsivleme kullanicinin kendi
/// temizligidir, Asuna'nin yeni bir sey hatirlamasi degil.
#[tauri::command]
pub fn memory_archive(
    state: State<'_, DbState>,
    id: i64,
    archived: bool,
) -> Result<MemoryWriteResult, StoreError> {
    let Some(db) = database(&state)? else {
        return Ok(MemoryWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };
    Ok(MemoryWriteResult::Stored {
        record: Box::new(set_archived(db, id, archived, &clock::now_utc())?),
    })
}

/// Kaydi kalici olarak siler.
///
/// Gizlilik anahtarina bakmaz — silme her zaman kullanilabilir olmali.
#[tauri::command]
pub fn memory_delete(state: State<'_, DbState>, id: i64) -> Result<MemoryWriteResult, StoreError> {
    let Some(db) = database(&state)? else {
        return Ok(MemoryWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };
    delete(db, id)?;
    Ok(MemoryWriteResult::Deleted { id })
}

/// **Tum** hafizayi siler (ASU-037).
///
/// Iki kapi vardir ve ikisi de gecilmelidir: UI'daki iki asamali onay ve
/// buradaki [`DELETE_ALL_CONFIRMATION`] ifadesi. Ifade eslesmezse DB'ye
/// **hic dokunulmaz** ve `invalid` kodlu tipli hata doner.
///
/// Kapsam bilerek dar: yalnizca `memories` tablosu. Oturum kayitlari/ozetleri ve
/// diskteki transcript dosyalari bu komutla silinmez — UI bunu acikca yazar,
/// cunku "hepsini sildim" diyip bir seyi birakmak en kotu sonuctur.
#[tauri::command]
pub fn memory_delete_all(
    state: State<'_, DbState>,
    confirmation_phrase: String,
) -> Result<MemoryPurgeResult, StoreError> {
    if confirmation_phrase != DELETE_ALL_CONFIRMATION {
        // Mesaj kullanicinin yazdigi metni **tekrarlamaz**.
        return Err(StoreError::invalid(format!(
            "`confirmationPhrase` birebir `{DELETE_ALL_CONFIRMATION}` olmali"
        )));
    }

    let Some(db) = database(&state)? else {
        return Ok(MemoryPurgeResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    Ok(MemoryPurgeResult::Purged {
        deleted: delete_all(db)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::store_error::StoreErrorCode;

    const NOW: &str = "2026-08-25T10:00:00Z";
    const LATER: &str = "2026-08-25T11:00:00Z";

    fn fresh_db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB acilmali")
    }

    fn draft(kind: MemoryKind, title: &str, content: &str) -> MemoryDraft {
        MemoryDraft {
            kind,
            title: title.to_owned(),
            content: content.to_owned(),
            summary: None,
            project_id: None,
            importance: 0.5,
            confidence: 0.5,
            source_session_id: None,
            expires_at: None,
            metadata_json: None,
        }
    }

    fn titles(records: &[MemoryRecord]) -> Vec<&str> {
        records.iter().map(|record| record.title.as_str()).collect()
    }

    fn list_all(db: &AsunaDb, filter: MemoryFilter) -> Vec<MemoryRecord> {
        list(db, &filter, NOW).expect("liste okunmali")
    }

    // --- create -----------------------------------------------------------

    #[test]
    fn creates_a_record_and_returns_what_was_written() {
        let db = fresh_db();
        let record = create(
            &db,
            &MemoryDraft {
                summary: Some("Kisa ozet".to_owned()),
                project_id: Some("asuna".to_owned()),
                importance: 0.9,
                confidence: 1.0,
                metadata_json: Some(r#"{"source":"test"}"#.to_owned()),
                ..draft(MemoryKind::Decision, "Wake word yerel", "Cihazda calisir.")
            },
            NOW,
        )
        .expect("kayit olusmali");

        assert!(record.id > 0);
        assert_eq!(record.kind, MemoryKind::Decision);
        assert_eq!(record.summary.as_deref(), Some("Kisa ozet"));
        assert_eq!(record.project_id.as_deref(), Some("asuna"));
        assert_eq!(record.created_at, NOW);
        assert_eq!(record.updated_at, NOW);
        assert_eq!(record.last_accessed_at, None);
        assert!(!record.is_archived);
        assert_eq!(record.metadata_json, r#"{"source":"test"}"#);
    }

    #[test]
    fn create_defaults_metadata_to_an_empty_object() {
        let db = fresh_db();
        let record = create(&db, &draft(MemoryKind::Idea, "t", "c"), NOW).expect("kayit");
        assert_eq!(record.metadata_json, EMPTY_METADATA);
    }

    #[test]
    fn create_trims_whitespace_and_rejects_blank_text() {
        let db = fresh_db();
        let record = create(
            &db,
            &draft(MemoryKind::Idea, "  bosluklu  ", " icerik "),
            NOW,
        )
        .expect("kayit");
        assert_eq!(record.title, "bosluklu");
        assert_eq!(record.content, "icerik");

        for (title, content) in [("   ", "c"), ("t", "")] {
            let error = create(&db, &draft(MemoryKind::Idea, title, content), NOW)
                .expect_err("bos metin reddedilmeli");
            assert_eq!(error.code(), StoreErrorCode::Invalid);
        }
    }

    #[test]
    fn create_rejects_out_of_range_scores() {
        let db = fresh_db();
        for (importance, confidence) in [(1.5, 0.5), (-0.1, 0.5), (0.5, f64::NAN)] {
            let error = create(
                &db,
                &MemoryDraft {
                    importance,
                    confidence,
                    ..draft(MemoryKind::Idea, "t", "c")
                },
                NOW,
            )
            .expect_err("aralik disi skor reddedilmeli");
            assert_eq!(error.code(), StoreErrorCode::Invalid);
        }
    }

    #[test]
    fn create_rejects_invalid_metadata_and_timestamps() {
        let db = fresh_db();

        let error = create(
            &db,
            &MemoryDraft {
                metadata_json: Some("{ bozuk".to_owned()),
                ..draft(MemoryKind::Idea, "t", "c")
            },
            NOW,
        )
        .expect_err("bozuk JSON reddedilmeli");
        assert_eq!(error.code(), StoreErrorCode::Invalid);
        assert!(error.to_string().contains("metadataJson"));

        let error = create(
            &db,
            &MemoryDraft {
                expires_at: Some("2026-08-25 10:00:00".to_owned()),
                ..draft(MemoryKind::Idea, "t", "c")
            },
            NOW,
        )
        .expect_err("UTC olmayan zaman reddedilmeli");
        assert!(error.to_string().contains("expiresAt"));
    }

    /// Dogrulama hatalari alan adini soyler, kullanici icerigini **tekrarlamaz**.
    #[test]
    fn validation_errors_never_echo_user_content() {
        let db = fresh_db();
        let secret = "Kullanicinin banka sifresi 1234";

        let error = create(
            &db,
            &MemoryDraft {
                importance: 9.0,
                ..draft(MemoryKind::Idea, "t", secret)
            },
            NOW,
        )
        .expect_err("hata bekleniyordu");

        let message = error.to_string();
        assert!(!message.contains(secret), "mesaj: {message}");
        assert!(message.contains("importance"), "mesaj: {message}");
    }

    /// Var olmayan bir oturuma bagli hafiza yazilamaz (FK) — hata yutulmaz.
    #[test]
    fn create_reports_a_storage_error_for_an_unknown_session() {
        let db = fresh_db();
        let error = create(
            &db,
            &MemoryDraft {
                source_session_id: Some(4_242),
                ..draft(MemoryKind::Decision, "t", "c")
            },
            NOW,
        )
        .expect_err("FK ihlali hata vermeli");
        assert_eq!(error.code(), StoreErrorCode::Storage);
    }

    // --- list / filtreler --------------------------------------------------

    fn seed(db: &AsunaDb) {
        create(
            db,
            &MemoryDraft {
                project_id: Some("asuna".to_owned()),
                importance: 0.9,
                ..draft(MemoryKind::Decision, "Wake word yerel", "Cihazda calisir.")
            },
            NOW,
        )
        .expect("kayit");
        create(
            db,
            &MemoryDraft {
                project_id: Some("asuna".to_owned()),
                importance: 0.2,
                ..draft(
                    MemoryKind::Preference,
                    "Kisa cevap",
                    "Kod yazarken kisa konus.",
                )
            },
            LATER,
        )
        .expect("kayit");
        create(
            db,
            &MemoryDraft {
                project_id: Some("baska-proje".to_owned()),
                importance: 0.5,
                ..draft(MemoryKind::Idea, "Baska fikir", "Ilgisiz icerik.")
            },
            LATER,
        )
        .expect("kayit");
    }

    #[test]
    fn lists_everything_by_default_newest_first() {
        let db = fresh_db();
        seed(&db);

        let records = list_all(&db, MemoryFilter::default());
        assert_eq!(records.len(), 3);
        // Ayni saniyedeki iki kaydin sirasi id ile cozulur (en yeni once).
        assert_eq!(
            titles(&records),
            ["Baska fikir", "Kisa cevap", "Wake word yerel"]
        );
    }

    /// **ASU-034 sozlesmesi / ASU-035 filtresi**: `excludePendingApproval` yalnizca
    /// bayragi **`true`** olan kayitlari eler.
    ///
    /// Kritik ayrim: elle (Memory UI) olusturulan kayitlarda anahtar hic yoktur ve
    /// bu kayitlar onay beklemez. Kosul `IS NOT 1` bu yuzden; `= 0` yazilsaydi
    /// kullanicinin kendi yazdigi her hafiza sessizce retrieval disinda kalirdi.
    #[test]
    fn pending_approval_filter_only_drops_the_flagged_records() {
        let db = fresh_db();
        create(
            &db,
            &MemoryDraft {
                metadata_json: Some(r#"{"pendingApproval":true}"#.to_owned()),
                ..draft(MemoryKind::Profile, "Onay bekliyor", "hassas")
            },
            NOW,
        )
        .expect("kayit");
        create(
            &db,
            &MemoryDraft {
                metadata_json: Some(r#"{"pendingApproval":false}"#.to_owned()),
                ..draft(MemoryKind::Decision, "Onaylanmis", "a")
            },
            NOW,
        )
        .expect("kayit");
        // Anahtar yok (elle olusturulmus kayit).
        create(&db, &draft(MemoryKind::Idea, "Elle yazilmis", "b"), NOW).expect("kayit");

        // Varsayilan: UI davranisi degismez — onay bekleyen de gorunur.
        assert_eq!(list_all(&db, MemoryFilter::default()).len(), 3);

        let filtered = list_all(
            &db,
            MemoryFilter {
                exclude_pending_approval: true,
                ..MemoryFilter::default()
            },
        );
        let mut visible = titles(&filtered);
        visible.sort_unstable();
        assert_eq!(visible, ["Elle yazilmis", "Onaylanmis"]);
    }

    #[test]
    fn filters_by_kind_project_and_sort() {
        let db = fresh_db();
        seed(&db);

        let by_kind = list_all(
            &db,
            MemoryFilter {
                kinds: vec![MemoryKind::Decision, MemoryKind::Preference],
                ..MemoryFilter::default()
            },
        );
        assert_eq!(titles(&by_kind), ["Kisa cevap", "Wake word yerel"]);

        let by_project = list_all(
            &db,
            MemoryFilter {
                project_id: Some("asuna".to_owned()),
                sort: MemorySort::Importance,
                ..MemoryFilter::default()
            },
        );
        assert_eq!(titles(&by_project), ["Wake word yerel", "Kisa cevap"]);

        let oldest = list_all(
            &db,
            MemoryFilter {
                sort: MemorySort::Oldest,
                ..MemoryFilter::default()
            },
        );
        assert_eq!(titles(&oldest)[0], "Wake word yerel");
    }

    #[test]
    fn searches_title_content_and_summary() {
        let db = fresh_db();
        seed(&db);
        create(
            &db,
            &MemoryDraft {
                summary: Some("ozette gecen kelime: elma".to_owned()),
                ..draft(MemoryKind::Routine, "Baslik", "Icerik")
            },
            NOW,
        )
        .expect("kayit");

        let by_content = list_all(
            &db,
            MemoryFilter {
                search: Some("cihazda".to_owned()),
                ..MemoryFilter::default()
            },
        );
        assert_eq!(titles(&by_content), ["Wake word yerel"]);

        let by_summary = list_all(
            &db,
            MemoryFilter {
                search: Some("elma".to_owned()),
                ..MemoryFilter::default()
            },
        );
        assert_eq!(titles(&by_summary), ["Baslik"]);
    }

    /// `%` kullanicinin aradigi harf; joker olarak yorumlanirsa arama her seyi
    /// dondurur ve "aramam calisti" yanilsamasi olusur.
    #[test]
    fn search_treats_wildcards_as_literal_characters() {
        let db = fresh_db();
        create(
            &db,
            &draft(MemoryKind::Idea, "Yuzde", "Indirim %50 oldu"),
            NOW,
        )
        .expect("kayit");
        create(&db, &draft(MemoryKind::Idea, "Duz", "Indirim yok"), NOW).expect("kayit");

        let hits = list_all(
            &db,
            MemoryFilter {
                search: Some("%50".to_owned()),
                ..MemoryFilter::default()
            },
        );
        assert_eq!(titles(&hits), ["Yuzde"]);

        // Tek basina `%` joker olsaydi her kaydi getirirdi; harf olarak
        // arandiginda yalnizca icinde gercekten `%` gecen kayit doner.
        let literal_percent = list_all(
            &db,
            MemoryFilter {
                search: Some("%".to_owned()),
                ..MemoryFilter::default()
            },
        );
        assert_eq!(titles(&literal_percent), ["Yuzde"]);
    }

    #[test]
    fn hides_archived_records_unless_asked() {
        let db = fresh_db();
        seed(&db);
        let target = list_all(&db, MemoryFilter::default())[0].id;
        set_archived(&db, target, true, LATER).expect("arsivlenmeli");

        assert_eq!(list_all(&db, MemoryFilter::default()).len(), 2);
        assert_eq!(
            list_all(
                &db,
                MemoryFilter {
                    archived: ArchiveFilter::Archived,
                    ..MemoryFilter::default()
                }
            )
            .len(),
            1
        );
        assert_eq!(
            list_all(
                &db,
                MemoryFilter {
                    archived: ArchiveFilter::All,
                    ..MemoryFilter::default()
                }
            )
            .len(),
            3
        );
    }

    /// Kabul kriteri: suresi dolmus kayit retrieval'da donmez ama silinmez de.
    #[test]
    fn expired_records_are_not_returned_by_retrieval() {
        let db = fresh_db();
        create(
            &db,
            &MemoryDraft {
                expires_at: Some("2026-08-25T09:00:00Z".to_owned()),
                ..draft(MemoryKind::WorkingContext, "Suresi dolmus", "Gecici baglam")
            },
            "2026-08-25T08:00:00Z",
        )
        .expect("kayit");
        create(
            &db,
            &MemoryDraft {
                expires_at: Some("2026-08-25T23:00:00Z".to_owned()),
                ..draft(MemoryKind::WorkingContext, "Hala gecerli", "Gecici baglam")
            },
            "2026-08-25T08:00:00Z",
        )
        .expect("kayit");

        assert_eq!(
            titles(&list_all(&db, MemoryFilter::default())),
            ["Hala gecerli"]
        );

        let with_expired = list_all(
            &db,
            MemoryFilter {
                include_expired: true,
                sort: MemorySort::Oldest,
                ..MemoryFilter::default()
            },
        );
        assert_eq!(
            with_expired.len(),
            2,
            "kayit silinmemeli, sadece gizlenmeli"
        );
    }

    #[test]
    fn limit_defaults_and_is_capped() {
        let db = fresh_db();
        for index in 0..5 {
            create(
                &db,
                &draft(MemoryKind::Idea, &format!("kayit-{index}"), "c"),
                NOW,
            )
            .expect("kayit");
        }

        assert_eq!(
            list_all(
                &db,
                MemoryFilter {
                    limit: Some(2),
                    ..MemoryFilter::default()
                }
            )
            .len(),
            2
        );

        // Tavani asan istek reddedilmez, kirpilir — ve kirpma gercekten SQL'e gider.
        let query = build_list_query(
            &MemoryFilter {
                limit: Some(10_000),
                ..MemoryFilter::default()
            },
            NOW,
        )
        .expect("sorgu kurulmali");
        assert_eq!(
            query.params.last(),
            Some(&Value::Integer(i64::from(MAX_LIST_LIMIT)))
        );

        let default_query =
            build_list_query(&MemoryFilter::default(), NOW).expect("sorgu kurulmali");
        assert_eq!(
            default_query.params.last(),
            Some(&Value::Integer(i64::from(DEFAULT_LIST_LIMIT)))
        );
    }

    // --- erisim izi --------------------------------------------------------

    #[test]
    fn access_time_is_only_written_when_requested() {
        let db = fresh_db();
        seed(&db);

        let browsed = list_all(&db, MemoryFilter::default());
        assert!(browsed
            .iter()
            .all(|record| record.last_accessed_at.is_none()));

        let retrieved = list(
            &db,
            &MemoryFilter {
                mark_accessed: true,
                ..MemoryFilter::default()
            },
            LATER,
        )
        .expect("liste");
        assert!(retrieved
            .iter()
            .all(|record| record.last_accessed_at.as_deref() == Some(LATER)));

        // Ve gercekten DB'ye yazilmis olmali.
        let reread = list_all(&db, MemoryFilter::default());
        assert!(reread
            .iter()
            .all(|record| record.last_accessed_at.as_deref() == Some(LATER)));
    }

    #[test]
    fn get_by_id_returns_archived_and_expired_records() {
        let db = fresh_db();
        let record = create(
            &db,
            &MemoryDraft {
                expires_at: Some("2026-08-25T09:00:00Z".to_owned()),
                ..draft(MemoryKind::Task, "Suresi dolmus", "c")
            },
            "2026-08-25T08:00:00Z",
        )
        .expect("kayit");
        set_archived(&db, record.id, true, NOW).expect("arsivlenmeli");

        let found = get_by_id(&db, record.id, LATER, true)
            .expect("okuma")
            .expect("kayit bulunmali");
        assert_eq!(found.id, record.id);
        assert_eq!(found.last_accessed_at.as_deref(), Some(LATER));

        assert!(get_by_id(&db, record.id + 999, NOW, false)
            .expect("okuma")
            .is_none());
    }

    // --- update / archive / delete ----------------------------------------

    #[test]
    fn updates_only_the_given_fields_and_refreshes_updated_at() {
        let db = fresh_db();
        let original = create(
            &db,
            &MemoryDraft {
                summary: Some("eski ozet".to_owned()),
                ..draft(MemoryKind::Idea, "eski baslik", "eski icerik")
            },
            NOW,
        )
        .expect("kayit");

        let updated = update(
            &db,
            original.id,
            &MemoryPatch {
                title: Some("yeni baslik".to_owned()),
                importance: Some(1.0),
                ..MemoryPatch::default()
            },
            LATER,
        )
        .expect("guncellenmeli");

        assert_eq!(updated.title, "yeni baslik");
        assert_eq!(
            updated.content, "eski icerik",
            "dokunulmayan alan korunmali"
        );
        assert_eq!(updated.summary.as_deref(), Some("eski ozet"));
        assert!((updated.importance - 1.0).abs() < f64::EPSILON);
        assert_eq!(updated.created_at, NOW);
        assert_eq!(updated.updated_at, LATER);
    }

    /// `null` "temizle" demek; alanin hic verilmemesi "dokunma" demek.
    #[test]
    fn nullable_fields_distinguish_clear_from_untouched() {
        let db = fresh_db();
        let original = create(
            &db,
            &MemoryDraft {
                summary: Some("ozet".to_owned()),
                project_id: Some("asuna".to_owned()),
                expires_at: Some("2026-09-01T00:00:00Z".to_owned()),
                ..draft(MemoryKind::Task, "t", "c")
            },
            NOW,
        )
        .expect("kayit");

        let untouched = update(
            &db,
            original.id,
            &MemoryPatch {
                title: Some("yeni".to_owned()),
                ..MemoryPatch::default()
            },
            LATER,
        )
        .expect("guncelleme");
        assert_eq!(untouched.summary.as_deref(), Some("ozet"));
        assert_eq!(
            untouched.expires_at.as_deref(),
            Some("2026-09-01T00:00:00Z")
        );

        let cleared = update(
            &db,
            original.id,
            &MemoryPatch {
                summary: Some(None),
                project_id: Some(None),
                expires_at: Some(None),
                ..MemoryPatch::default()
            },
            LATER,
        )
        .expect("guncelleme");
        assert_eq!(cleared.summary, None);
        assert_eq!(cleared.project_id, None);
        assert_eq!(cleared.expires_at, None);
    }

    #[test]
    fn empty_patch_is_rejected_instead_of_touching_the_row() {
        let db = fresh_db();
        let record = create(&db, &draft(MemoryKind::Idea, "t", "c"), NOW).expect("kayit");

        let error = update(&db, record.id, &MemoryPatch::default(), LATER)
            .expect_err("bos patch reddedilmeli");
        assert_eq!(error.code(), StoreErrorCode::Invalid);

        let unchanged = get_by_id(&db, record.id, NOW, false)
            .expect("okuma")
            .expect("kayit");
        assert_eq!(unchanged.updated_at, NOW);
    }

    #[test]
    fn archive_and_unarchive_round_trip() {
        let db = fresh_db();
        let record = create(&db, &draft(MemoryKind::Idea, "t", "c"), NOW).expect("kayit");

        let archived = set_archived(&db, record.id, true, LATER).expect("arsiv");
        assert!(archived.is_archived);
        assert_eq!(archived.updated_at, LATER);

        let restored = set_archived(&db, record.id, false, LATER).expect("geri al");
        assert!(!restored.is_archived);
    }

    #[test]
    fn deletes_a_record_for_good() {
        let db = fresh_db();
        let record = create(&db, &draft(MemoryKind::Idea, "t", "c"), NOW).expect("kayit");

        delete(&db, record.id).expect("silinmeli");
        assert!(get_by_id(&db, record.id, NOW, false)
            .expect("okuma")
            .is_none());
    }

    #[test]
    fn missing_records_report_not_found_instead_of_pretending_to_succeed() {
        let db = fresh_db();

        for error in [
            update(
                &db,
                999,
                &MemoryPatch {
                    title: Some("x".to_owned()),
                    ..MemoryPatch::default()
                },
                NOW,
            )
            .expect_err("guncelleme"),
            set_archived(&db, 999, true, NOW).expect_err("arsiv"),
            delete(&db, 999).expect_err("silme"),
        ] {
            assert_eq!(error.code(), StoreErrorCode::NotFound);
        }
    }

    #[test]
    fn non_positive_ids_are_rejected_before_touching_the_database() {
        let db = fresh_db();
        for id in [0_i64, -1] {
            assert_eq!(
                delete(&db, id).expect_err("gecersiz id").code(),
                StoreErrorCode::Invalid
            );
        }
    }

    // --- sozlesme ----------------------------------------------------------

    #[test]
    fn write_result_serializes_with_an_explicit_status() {
        let json = serde_json::to_value(MemoryWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        })
        .expect("serialize");
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["reason"], "memory-disabled");

        let json = serde_json::to_value(MemoryWriteResult::Deleted { id: 7 }).expect("serialize");
        assert_eq!(json["status"], "deleted");
        assert_eq!(json["id"], 7);
    }

    /// Sozlesme disi bir alan sessizce yutulmaz — TS tarafindaki whitelist ile
    /// ayni disiplin.
    #[test]
    fn unknown_fields_are_rejected_at_the_ipc_boundary() {
        let error = serde_json::from_str::<MemoryDraft>(
            r#"{"kind":"idea","title":"t","content":"c","importance":0.5,
                "confidence":0.5,"embedding":[1,2,3]}"#,
        )
        .expect_err("bilinmeyen alan reddedilmeli");
        assert!(error.to_string().contains("embedding"));

        assert!(serde_json::from_str::<MemoryDraft>(
            r#"{"kind":"sql_injection","title":"t","content":"c","importance":0.5,"confidence":0.5}"#
        )
        .is_err());
    }

    #[test]
    fn patch_json_distinguishes_absent_from_null() {
        let absent: MemoryPatch = serde_json::from_str(r#"{"title":"x"}"#).expect("patch");
        assert_eq!(absent.summary, None);

        let cleared: MemoryPatch = serde_json::from_str(r#"{"summary":null}"#).expect("patch");
        assert_eq!(cleared.summary, Some(None));

        let set: MemoryPatch = serde_json::from_str(r#"{"summary":"yeni"}"#).expect("patch");
        assert_eq!(set.summary, Some(Some("yeni".to_owned())));
    }

    #[test]
    fn filter_defaults_are_safe_for_retrieval() {
        let filter = MemoryFilter::default();
        assert_eq!(filter.archived, ArchiveFilter::Active);
        assert!(!filter.include_expired);
        assert!(!filter.mark_accessed);
        assert_eq!(filter.sort, MemorySort::Recent);
    }

    /// **ASU-037**: toplu silme gercekten siler ve kac kayit gittigini soyler.
    #[test]
    fn delete_all_removes_every_record_and_reports_the_count() {
        let db = fresh_db();
        create(&db, &draft(MemoryKind::Decision, "a", "icerik"), NOW).expect("kayit");
        create(&db, &draft(MemoryKind::Idea, "b", "icerik"), NOW).expect("kayit");
        let archived = create(&db, &draft(MemoryKind::Task, "c", "icerik"), NOW).expect("kayit");
        set_archived(&db, archived.id, true, NOW).expect("arsivle");

        // Arsivli kayit da gider: "tum hafiza" gercekten tumu demek.
        assert_eq!(delete_all(&db).expect("toplu silme"), 3);

        let remaining = list(
            &db,
            &MemoryFilter {
                archived: ArchiveFilter::All,
                include_expired: true,
                ..MemoryFilter::default()
            },
            NOW,
        )
        .expect("listeleme");
        assert!(remaining.is_empty(), "kalan kayit: {remaining:?}");

        // Bos depoda tekrar cagirmak hata degil, yalnizca 0.
        assert_eq!(delete_all(&db).expect("bos depo"), 0);
    }

    #[test]
    fn purge_result_serializes_with_the_expected_contract() {
        let json =
            serde_json::to_value(MemoryPurgeResult::Purged { deleted: 12 }).expect("serialize");
        assert_eq!(json["status"], "purged");
        assert_eq!(json["deleted"], 12);

        let json = serde_json::to_value(MemoryPurgeResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        })
        .expect("serialize");
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["reason"], "memory-disabled");
    }

    /// Onay ifadesi kod tarafinda sabit; UI ile ayni metin olmali.
    #[test]
    fn the_delete_all_confirmation_phrase_is_exact() {
        assert_eq!(DELETE_ALL_CONFIRMATION, "TUM HAFIZAYI SIL");
    }

    /// Cagirandan gelen zaman damgasi da dogrulanir: bozuk bir `now`
    /// sessizce DB'ye yazilamaz.
    #[test]
    fn a_malformed_now_is_rejected() {
        let db = fresh_db();
        let error = create(&db, &draft(MemoryKind::Idea, "t", "c"), "simdi")
            .expect_err("bozuk zaman reddedilmeli");
        assert_eq!(error.code(), StoreErrorCode::Invalid);
    }
}

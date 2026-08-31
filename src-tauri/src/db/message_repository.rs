//! Konusma mesajlari — `messages` yazma ve okuma (Chat Shell / migration 006).
//!
//! # Sozlesme
//!
//! - Bir mesaj **her zaman** bir konusmaya (oturuma) aittir: `session_id` NOT
//!   NULL ve konusma silinince mesajlar CASCADE ile gider. Sahipsiz mesaj
//!   kavrami yok.
//! - Bos mesaj yazilmaz. Icerik once [`normalize_content`] ile kirpilir
//!   (bastaki/sondaki bosluklar) ve bos kalirsa istek **reddedilir**:
//!   "gonderdim ama bos" ile "gondermedim" ayni gorunmemeli.
//! - Renderer'a giden bicim `src/shared/chat.ts` → `ChatMessage`; alan adlari
//!   ve tipleri oraya baglidir ([`MessageRecord`]).
//! - Bu modul **model cagirmaz**. `chat_send` (WP2) once modeli konusturur,
//!   sonra kullanici mesajini ve yaniti tek transaction'da buraya yazar —
//!   yarim bir konusma (kullanici mesaji yazildi, yanit yazilmadi) diske
//!   dusmesin diye. Bu yuzden [`append_in_tx`] `pub(crate)` olarak disari
//!   veriliyor.
//!
//! # Neden mesaj icerigi icin sema tavani yok
//!
//! `tool_events`teki uzunluk tavanlari bir **denetim** satirinin sessizce
//! sismesini engellemek icindi. Burada icerik zaten kullanicinin (ve modelin)
//! verisinin kendisi: bir tavan, uzun bir asistan yanitini INSERT aninda
//! dusurur ve konusmayi yarim birakirdi. Tavan yalnizca **renderer'in
//! gonderebilecegi** metne uygulanir ([`MAX_MESSAGE_CONTENT_CHARS`]); host
//! tarafindan yazilan model yaniti bu tavana takilmaz.

use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;

use super::model::{MessageRecord, MessageRole};
use super::session_repository;
use super::store_error::{database, StoreError, StoreSkipReason};
use super::{clock, AsunaDb, DbState};
use crate::privacy::PrivacyState;

/// Mesaj listesinin varsayilan uzunlugu.
///
/// `session_list` / `tool_event_list` tavanlarindan (50/200) buyuk, bilincli:
/// orasi bir **denetim** ekrani, burasi konusmanin kendisi. Bir konusmayi
/// yarim gostermek, denetim listesini kirpmakla ayni sey degil.
pub const DEFAULT_MESSAGE_LIST_LIMIT: u32 = 500;

/// Mesaj listesi icin tavan. Asan istek reddedilmez, **kirpilir** — ve kirpma
/// her zaman **sondan** (en yeni mesajlar) tutulur: bir konusmaya donen
/// kullanici son konusulani gormek ister.
pub const MAX_MESSAGE_LIST_LIMIT: u32 = 2_000;

/// Renderer'in [`message_append`] ile gonderebilecegi azami karakter.
///
/// `chat_send`in girdi tavani ile ayni (plan-chat-shell.md). Host tarafinda
/// yazilan model yanitina uygulanmaz — bkz. modul dokumantasyonu.
pub const MAX_MESSAGE_CONTENT_CHARS: usize = 32_000;

// ---------------------------------------------------------------------------
// Girdi / cikti tipleri
// ---------------------------------------------------------------------------

/// Mesaj yazma sonucu. `Skipped` = hafiza kapali (hata degil) —
/// `SessionWriteResult` ile ayni sozlesme.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum MessageWriteResult {
    Recorded { message: Box<MessageRecord> },
    Skipped { reason: StoreSkipReason },
}

// ---------------------------------------------------------------------------
// Dogrulama
// ---------------------------------------------------------------------------

/// Mesaj icerigini yazilabilir hale getirir.
///
/// Bastaki/sondaki bosluklar kirpilir (composer'dan gelen `"\n"` bir mesaj
/// degildir), ic bosluklara **dokunulmaz**: kod blogunun girintisi kullanicinin
/// verisidir. Bos kalirsa istek reddedilir.
pub fn normalize_content(raw: &str) -> Result<String, StoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(StoreError::invalid("`content` bos birakilamaz"));
    }
    Ok(trimmed.to_owned())
}

fn validated_session_id(session_id: i64) -> Result<i64, StoreError> {
    if session_id <= 0 {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }
    Ok(session_id)
}

fn validated_now(now: &str) -> Result<(), StoreError> {
    if !clock::is_utc_iso8601(now) {
        return Err(StoreError::invalid(
            "`now` UTC ISO-8601 olmali (orn. 2026-08-25T10:00:00Z)",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

/// Konusmaya bir mesaj ekler.
///
/// Konusma yoksa `NotFound` doner — FK ihlalini bir `Storage` hatasina
/// cevirmek yerine: "boyle bir konusma yok" cagiranin duzeltebilecegi bir
/// durum, "veritabani islemi basarisiz" degil.
pub fn append(
    db: &AsunaDb,
    session_id: i64,
    role: MessageRole,
    content: &str,
    now: &str,
) -> Result<MessageRecord, StoreError> {
    let session_id = validated_session_id(session_id)?;
    let content = normalize_content(content)?;
    validated_now(now)?;

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            if !session_repository::exists(&transaction, session_id)? {
                transaction.commit()?;
                return Ok(None);
            }
            let record = append_in_tx(&transaction, session_id, role, &content, now)?;
            transaction.commit()?;
            Ok(Some(record))
        })
        .map_err(|error| StoreError::storage(error, "message_append"))?;

    record.ok_or(StoreError::NotFound)
}

/// Ayni transaction icinde mesaj yazar ve yazilan satiri dondurur.
///
/// # Cagiran ne garanti etmeli
///
/// - `content` **onceden** [`normalize_content`]'ten gecmis olmali (bos icerik
///   burada semaya carpar ve tum transaction'i dusurur).
/// - `session_id` var olan bir konusmayi gostermeli
///   ([`session_repository::exists`]). Aksi halde FK ihlali doner — yani
///   sahipsiz bir satir **yazilamaz**, yalnizca hata mesaji daha az anlasilir
///   olur.
///
/// # Neden `pub(crate)`
///
/// `chat_send` kullanici mesajini ve asistan yanitini **tek** transaction'da
/// yazmak zorunda: iki ayri yazma arasinda uygulama kapanirsa konusmada
/// cevapsiz bir kullanici mesaji kalirdi. Ayni sebeple ek dosyalarin mesaja
/// baglanmasi da ayni transaction'dadir
/// ([`super::attachment_repository::link_to_message_in_tx`]).
pub(crate) fn append_in_tx(
    transaction: &rusqlite::Transaction<'_>,
    session_id: i64,
    role: MessageRole,
    content: &str,
    now: &str,
) -> rusqlite::Result<MessageRecord> {
    transaction.execute(
        "INSERT INTO messages (session_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![session_id, role, content, now],
    )?;
    let id = transaction.last_insert_rowid();
    load(transaction, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

/// Konusmanin mesajlarini **eskiden yeniye** dondurur.
///
/// Tavan asilirsa **son** `limit` mesaj tutulur (bir konusmaya donen kullanici
/// son konusulani gormek ister), ama donen dizinin sirasi yine eskiden yeniye
/// olur — ekranda yukaridan asagi okunacak.
///
/// Siralama `id`: `INTEGER PRIMARY KEY` ekleme sirasiyla artiyor ve
/// `created_at` saniye hassasiyetinde (bkz. [`clock`]) — ayni saniyede yazilan
/// kullanici mesaji ile asistan yaniti yalnizca `id` ile dogru siralanabilir.
pub fn list_for_session(
    db: &AsunaDb,
    session_id: i64,
    limit: u32,
) -> Result<Vec<MessageRecord>, StoreError> {
    let session_id = validated_session_id(session_id)?;
    let limit = limit.clamp(1, MAX_MESSAGE_LIST_LIMIT);

    db.with_connection(|connection| {
        let mut statement = connection.prepare(&format!(
            "SELECT {columns} FROM messages
              WHERE session_id = ?1
              ORDER BY id DESC
              LIMIT ?2",
            columns = MessageRecord::select_columns()
        ))?;
        let rows = statement.query_map(params![session_id, limit], MessageRecord::from_row)?;
        let mut messages = rows.collect::<rusqlite::Result<Vec<MessageRecord>>>()?;
        // Sorgu en yeniden eskiye okudu (tavan sondan kirpsin diye); ekrandaki
        // sira bunun tersi.
        messages.reverse();
        Ok(messages)
    })
    .map_err(|error| StoreError::storage(error, "message_list"))
}

fn load(connection: &rusqlite::Connection, id: i64) -> rusqlite::Result<Option<MessageRecord>> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM messages WHERE id = ?1",
                MessageRecord::select_columns()
            ),
            params![id],
            MessageRecord::from_row,
        )
        .optional()
}

// ---------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------

/// Konusmaya tek bir mesaj ekler.
///
/// # Bu komut renderer'in normal yolu DEGIL
///
/// Metin sohbetinin yolu `chat_send`dir: o komut modeli cagirir ve iki mesaji
/// birden, tek transaction'da yazar. Bu komut yardimci bir yuzeydir (sistem
/// notu yazmak, testler, ileride "sesli konusmayi metne dokme").
///
/// GUVENLIK NOTU (ACL karari backend agent'a ait): komut `role` parametresi
/// aldigi icin renderer teknik olarak `assistant` rolunde bir mesaj
/// yazabilir — yani model soylememisken "Asuna boyle dedi" satiri
/// uretilebilir. Bu yuzden komutun `build.rs` ACL manifest'ine ve bir
/// capability dosyasina eklenmesi **gerekli olmadikca yapilmamalidir**;
/// `chat_send` bu modulun `append_in_tx` yolunu zaten kullaniyor.
#[tauri::command]
pub fn message_append(
    state: State<'_, DbState>,
    privacy: State<'_, Arc<PrivacyState>>,
    session_id: i64,
    role: MessageRole,
    content: String,
) -> Result<MessageWriteResult, StoreError> {
    if content.chars().count() > MAX_MESSAGE_CONTENT_CHARS {
        return Err(StoreError::invalid(
            "`content` en fazla 32000 karakter olabilir",
        ));
    }
    if !privacy.memory_enabled() {
        return Ok(MessageWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    }
    let Some(db) = database(&state)? else {
        return Ok(MessageWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        });
    };

    let message = append(db, session_id, role, &content, &clock::now_utc())?;
    Ok(MessageWriteResult::Recorded {
        message: Box::new(message),
    })
}

/// Konusmanin mesajlarini listeler (salt okuma).
///
/// Hafiza kapaliyken **bos dizi** doner (hata degil) — `session_list` ile ayni
/// sozlesme. Bicim: `src/shared/chat.ts` → `parseChatMessageList`.
#[tauri::command]
pub fn message_list(
    state: State<'_, DbState>,
    session_id: i64,
    limit: Option<u32>,
) -> Result<Vec<MessageRecord>, StoreError> {
    let limit = limit
        .unwrap_or(DEFAULT_MESSAGE_LIST_LIMIT)
        .clamp(1, MAX_MESSAGE_LIST_LIMIT);

    let Some(db) = database(&state)? else {
        return Ok(Vec::new());
    };
    list_for_session(db, session_id, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::store_error::StoreErrorCode;

    const NOW: &str = "2026-08-31T10:00:00Z";

    fn fresh_db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB acilmali")
    }

    fn session(db: &AsunaDb) -> i64 {
        session_repository::start(db, "gpt-realtime-2.1", None, NOW)
            .expect("oturum acilmali")
            .id
    }

    #[test]
    fn appends_a_message_and_reads_it_back() {
        let db = fresh_db();
        let session_id = session(&db);

        let message =
            append(&db, session_id, MessageRole::User, "Merhaba", NOW).expect("mesaj yazilmali");

        assert!(message.id > 0);
        assert_eq!(message.session_id, session_id);
        assert_eq!(message.role, MessageRole::User);
        assert_eq!(message.content, "Merhaba");
        assert_eq!(message.created_at, NOW);
    }

    /// Liste eskiden yeniye gelir — ekranda yukaridan asagi okunacak.
    #[test]
    fn lists_messages_oldest_first() {
        let db = fresh_db();
        let session_id = session(&db);

        for (role, content, stamp) in [
            (MessageRole::User, "birinci", "2026-08-31T10:00:00Z"),
            (MessageRole::Assistant, "ikinci", "2026-08-31T10:00:00Z"),
            (MessageRole::User, "ucuncu", "2026-08-31T10:00:05Z"),
        ] {
            append(&db, session_id, role, content, stamp).expect("mesaj yazilmali");
        }

        let messages =
            list_for_session(&db, session_id, DEFAULT_MESSAGE_LIST_LIMIT).expect("liste okunmali");
        let contents: Vec<&str> = messages
            .iter()
            .map(|message| message.content.as_str())
            .collect();
        // Ilk ikisi ayni saniyede yazildi: siralamayi `id` cozuyor.
        assert_eq!(contents, ["birinci", "ikinci", "ucuncu"]);
    }

    /// Tavan asildiginda **son** mesajlar tutulur; sira yine eskiden yeniye.
    #[test]
    fn the_limit_keeps_the_newest_messages_in_reading_order() {
        let db = fresh_db();
        let session_id = session(&db);

        for index in 0..5 {
            append(
                &db,
                session_id,
                MessageRole::User,
                &format!("mesaj-{index}"),
                NOW,
            )
            .expect("mesaj yazilmali");
        }

        let messages = list_for_session(&db, session_id, 2).expect("liste okunmali");
        let contents: Vec<&str> = messages
            .iter()
            .map(|message| message.content.as_str())
            .collect();
        assert_eq!(contents, ["mesaj-3", "mesaj-4"]);
    }

    /// Baska bir konusmanin mesajlari karismaz.
    #[test]
    fn messages_are_scoped_to_their_conversation() {
        let db = fresh_db();
        let first = session(&db);
        let second = session(&db);

        append(&db, first, MessageRole::User, "birincide", NOW).expect("mesaj");
        append(&db, second, MessageRole::User, "ikincide", NOW).expect("mesaj");

        let messages =
            list_for_session(&db, second, DEFAULT_MESSAGE_LIST_LIMIT).expect("liste okunmali");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "ikincide");
    }

    #[test]
    fn empty_content_is_rejected_before_the_database_is_touched() {
        let db = fresh_db();
        let session_id = session(&db);

        for content in ["", "   ", "\n\t "] {
            assert_eq!(
                append(&db, session_id, MessageRole::User, content, NOW)
                    .expect_err("bos icerik reddedilmeli")
                    .code(),
                StoreErrorCode::Invalid
            );
        }

        let messages =
            list_for_session(&db, session_id, DEFAULT_MESSAGE_LIST_LIMIT).expect("liste okunmali");
        assert!(messages.is_empty(), "reddedilen mesaj yazilmis");
    }

    /// Ic bosluklara dokunulmaz: kod blogunun girintisi kullanicinin verisi.
    #[test]
    fn only_the_surrounding_whitespace_is_trimmed() {
        let db = fresh_db();
        let session_id = session(&db);

        let message = append(
            &db,
            session_id,
            MessageRole::Assistant,
            "\n  satir-1\n    satir-2  \n",
            NOW,
        )
        .expect("mesaj yazilmali");
        assert_eq!(message.content, "satir-1\n    satir-2");
    }

    #[test]
    fn appending_to_an_unknown_conversation_reports_not_found() {
        let db = fresh_db();
        assert_eq!(
            append(&db, 4242, MessageRole::User, "merhaba", NOW)
                .expect_err("bilinmeyen konusma")
                .code(),
            StoreErrorCode::NotFound
        );
    }

    #[test]
    fn a_non_positive_conversation_id_is_rejected() {
        let db = fresh_db();
        for id in [0, -1] {
            assert_eq!(
                append(&db, id, MessageRole::User, "merhaba", NOW)
                    .expect_err("gecersiz kimlik")
                    .code(),
                StoreErrorCode::Invalid
            );
            assert_eq!(
                list_for_session(&db, id, 10)
                    .expect_err("gecersiz kimlik")
                    .code(),
                StoreErrorCode::Invalid
            );
        }
    }

    #[test]
    fn a_malformed_clock_is_rejected() {
        let db = fresh_db();
        let session_id = session(&db);
        assert_eq!(
            append(&db, session_id, MessageRole::User, "merhaba", "simdi")
                .expect_err("bozuk zaman")
                .code(),
            StoreErrorCode::Invalid
        );
    }

    /// **Kabul kriteri 2**: konusma silinince mesajlar gercekten gider.
    #[test]
    fn deleting_the_conversation_removes_its_messages() {
        let db = fresh_db();
        let session_id = session(&db);
        append(&db, session_id, MessageRole::User, "merhaba", NOW).expect("mesaj");

        session_repository::delete(&db, session_id).expect("konusma silinmeli");

        let remaining: i64 = db
            .with_connection(|conn| {
                conn.query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
            })
            .expect("sayim okunmali");
        assert_eq!(remaining, 0);
    }

    /// `chat_send`in kullandigi yol: iki mesaj tek transaction'da.
    #[test]
    fn two_messages_can_be_written_in_a_single_transaction() {
        let db = fresh_db();
        let session_id = session(&db);

        let (user, assistant) = db
            .with_connection(|connection| {
                let transaction = connection.transaction()?;
                let user = append_in_tx(&transaction, session_id, MessageRole::User, "soru", NOW)?;
                let assistant = append_in_tx(
                    &transaction,
                    session_id,
                    MessageRole::Assistant,
                    "cevap",
                    NOW,
                )?;
                transaction.commit()?;
                Ok((user, assistant))
            })
            .expect("iki mesaj yazilmali");

        assert!(assistant.id > user.id);
        let messages =
            list_for_session(&db, session_id, DEFAULT_MESSAGE_LIST_LIMIT).expect("liste");
        assert_eq!(messages.len(), 2);
    }

    /// Transaction geri alinirsa **hicbir** mesaj kalmaz: yarim bir konusma
    /// (soru yazildi, cevap yazilmadi) diske dusmez.
    #[test]
    fn a_rolled_back_transaction_leaves_no_half_conversation() {
        let db = fresh_db();
        let session_id = session(&db);

        let outcome = db.with_connection(|connection| {
            let transaction = connection.transaction()?;
            append_in_tx(&transaction, session_id, MessageRole::User, "soru", NOW)?;
            // Ikinci yazma basarisiz (bos icerik semaya carpar).
            let failed = append_in_tx(&transaction, session_id, MessageRole::Assistant, "", NOW);
            assert!(failed.is_err(), "bos icerik semadan gecmis");
            transaction.rollback()?;
            Ok(())
        });
        assert!(outcome.is_ok());

        let messages =
            list_for_session(&db, session_id, DEFAULT_MESSAGE_LIST_LIMIT).expect("liste");
        assert!(messages.is_empty(), "geri alinan mesaj kalmis");
    }

    /// Renderer'a giden bicim `src/shared/chat.ts` ile ayni.
    #[test]
    fn the_write_result_is_tagged_on_the_wire() {
        let json = serde_json::to_value(MessageWriteResult::Skipped {
            reason: StoreSkipReason::MemoryDisabled,
        })
        .expect("serialize");
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["reason"], "memory-disabled");
    }
}

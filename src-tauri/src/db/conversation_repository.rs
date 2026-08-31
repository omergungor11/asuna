//! Konusma listesi — Chat Shell'in kenar cubugunu besleyen tek sorgu
//! (migration 006, plan-chat-shell.md WP1).
//!
//! # Neden `session_list` kullanilmiyor
//!
//! `session_list` bir **denetim** yuzeyidir (ASU-065): kapanis nedeni, ozet on
//! izlemesi, dokum dosyasi var mi. Kenar cubugu ise baska bir soruyu cevaplar:
//! "hangi konusmalar var, hangisine en son ne zaman dokundum?". Ayni sorguya
//! iki isi birden yaptirmak, `SessionListItem`e alan eklemek demekti — oysa o
//! tipin TypeScript aynasi (`src/shared/session.ts`) beklenmeyen alanda hata
//! firlatiyor ve calisan bir ekrani kirardi.
//!
//! Iki sorgu ayni tabloyu okuyor ama farkli projeksiyonlar uretiyor; bu bir
//! tekrar degil, iki ayri sozlesme.
//!
//! # Ses konusmalari da listede
//!
//! Filtre **yok**: `modality = 'text'` kosulu koymak, kullanicinin sesle
//! konustugu bir oturumu kenar cubugundan gizlerdi ve "konusmam nereye gitti?"
//! sorusunu uretirdi. Modalite her satirda gorunur; ayrimi UI yapar.
//!
//! Bicim sozlesmesi: `src/shared/chat.ts` → `ConversationSummary`.

use rusqlite::params;
use serde::Serialize;
use tauri::State;

use super::model::SessionModality;
use super::store_error::{database, StoreError};
use super::{AsunaDb, DbState};

/// Konusma listesinin varsayilan uzunlugu.
pub const DEFAULT_CONVERSATION_LIST_LIMIT: u32 = 200;

/// Konusma listesi icin tavan. Asan istek reddedilmez, **kirpilir**.
pub const MAX_CONVERSATION_LIST_LIMIT: u32 = 1_000;

/// Kenar cubugundaki tek konusma satiri.
///
/// [`super::model::SessionRecord`] **degil**: bu bir liste projeksiyonu, DB
/// kaydinin kopyasi degil (`SessionListItem` ile ayni disiplin). Ozet, token,
/// maliyet ve dokum yolu burada bilerek yok — kenar cubugu bunlari gostermiyor
/// ve her satirda tasimak bos maliyet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: i64,
    /// `None` = baslik henuz yok; UI "Adsiz konusma" yazar. Uydurulmaz.
    pub title: Option<String>,
    pub modality: SessionModality,
    pub project_id: Option<String>,
    pub started_at: String,
    /// Son mesajin zamani; hic mesaj yoksa `started_at`. Siralama bu alana gore.
    pub last_activity_at: String,
    pub message_count: i64,
}

/// Konusmalari **son aktiviteye gore** azalan sirada dondurur.
///
/// # Siralama
///
/// `last_activity_at DESC, id DESC`. Ikinci anahtar gerekli: zaman damgasi
/// saniye hassasiyetinde (bkz. [`super::clock`]) ve arka arkaya acilan iki
/// konusma ayni saniyeye dusebilir — o durumda "yeni acilan uste" beklentisini
/// yalnizca `id` koruyabilir.
///
/// Metin siralamasi UTC ISO-8601 icin dogru siralamadir (`memories` Stage A
/// ile ayni varsayim); semadaki GLOB kisiti bunu zorluyor.
pub fn list_recent(db: &AsunaDb, limit: u32) -> Result<Vec<ConversationSummary>, StoreError> {
    let limit = limit.clamp(1, MAX_CONVERSATION_LIST_LIMIT);

    db.with_connection(|connection| {
        // `LEFT JOIN`: mesaji olmayan konusma da listede kalir (yeni acilmis
        // bos konusma ekranda gorunmeli). `COALESCE` ile son aktivite o durumda
        // baslangic zamanina duser — uydurulmus bir "hic" degeri yok.
        let mut statement = connection.prepare(
            "SELECT s.id                                        AS id,
                    s.title                                     AS title,
                    s.modality                                  AS modality,
                    s.project_id                                AS project_id,
                    s.started_at                                AS started_at,
                    COALESCE(MAX(m.created_at), s.started_at)   AS last_activity_at,
                    COUNT(m.id)                                 AS message_count
               FROM sessions s
               LEFT JOIN messages m ON m.session_id = s.id
              GROUP BY s.id
              ORDER BY last_activity_at DESC, s.id DESC
              LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            Ok(ConversationSummary {
                id: row.get("id")?,
                title: row.get("title")?,
                modality: row.get("modality")?,
                project_id: row.get("project_id")?,
                started_at: row.get("started_at")?,
                last_activity_at: row.get("last_activity_at")?,
                message_count: row.get("message_count")?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<ConversationSummary>>>()
    })
    .map_err(|error| StoreError::storage(error, "conversation_list"))
}

// ---------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------

/// Konusma listesini dondurur (salt okuma).
///
/// Hafiza kapaliyken **bos dizi** doner (hata degil) — `session_list` ile ayni
/// sozlesme; bozuk oldugunda tipli hata doner ("konusma yok" ile "listeye
/// bakamadim" ayni cevap degil, PROJECT.md Bolum 30).
///
/// Renderer siralamayi ya da alanlari secemez; yalnizca kac satir istedigini
/// soyleyebilir ve bu istek tavana kirpilir.
#[tauri::command]
pub fn conversation_list(
    state: State<'_, DbState>,
    limit: Option<u32>,
) -> Result<Vec<ConversationSummary>, StoreError> {
    let limit = limit
        .unwrap_or(DEFAULT_CONVERSATION_LIST_LIMIT)
        .clamp(1, MAX_CONVERSATION_LIST_LIMIT);

    let Some(db) = database(&state)? else {
        return Ok(Vec::new());
    };
    list_recent(db, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::model::MessageRole;
    use crate::db::{message_repository, project_repository, session_repository};

    fn fresh_db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB acilmali")
    }

    fn conversation(db: &AsunaDb, started_at: &str, modality: SessionModality) -> i64 {
        session_repository::start_with_modality(db, "gpt-4o-mini", None, modality, started_at)
            .expect("konusma acilmali")
            .id
    }

    #[test]
    fn a_new_conversation_reports_no_messages_and_falls_back_to_its_start_time() {
        let db = fresh_db();
        let id = conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);

        let conversations =
            list_recent(&db, DEFAULT_CONVERSATION_LIST_LIMIT).expect("liste okunmali");

        assert_eq!(conversations.len(), 1);
        let row = &conversations[0];
        assert_eq!(row.id, id);
        assert_eq!(row.title, None, "baslik uydurulmamali");
        assert_eq!(row.modality, SessionModality::Text);
        assert_eq!(row.project_id, None);
        assert_eq!(row.started_at, "2026-08-31T10:00:00Z");
        assert_eq!(
            row.last_activity_at, "2026-08-31T10:00:00Z",
            "mesaj yokken son aktivite baslangic zamanidir"
        );
        assert_eq!(row.message_count, 0);
    }

    #[test]
    fn the_last_message_drives_the_activity_time_and_the_count() {
        let db = fresh_db();
        let id = conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);

        message_repository::append(
            &db,
            id,
            MessageRole::User,
            "merhaba",
            "2026-08-31T10:00:05Z",
        )
        .expect("mesaj");
        message_repository::append(
            &db,
            id,
            MessageRole::Assistant,
            "merhaba!",
            "2026-08-31T10:00:09Z",
        )
        .expect("mesaj");

        let row = list_recent(&db, DEFAULT_CONVERSATION_LIST_LIMIT).expect("liste")[0].clone();
        assert_eq!(row.last_activity_at, "2026-08-31T10:00:09Z");
        assert_eq!(row.message_count, 2);
    }

    /// Siralama son aktiviteye gore: **eski** ama az once konusulan bir
    /// konusma, yeni acilmis bos bir konusmanin ustunde durur.
    #[test]
    fn conversations_are_ordered_by_the_last_activity_not_by_the_start_time() {
        let db = fresh_db();
        let old = conversation(&db, "2026-08-30T09:00:00Z", SessionModality::Text);
        let fresh = conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);

        message_repository::append(
            &db,
            old,
            MessageRole::User,
            "hala buradayim",
            "2026-08-31T12:00:00Z",
        )
        .expect("mesaj");

        let ids: Vec<i64> = list_recent(&db, DEFAULT_CONVERSATION_LIST_LIMIT)
            .expect("liste")
            .iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, [old, fresh]);
    }

    /// Ayni saniyede acilan iki konusmanin sirasini `id` cozer: yeni olan uste.
    #[test]
    fn ties_are_broken_by_the_newest_conversation() {
        let db = fresh_db();
        let first = conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);
        let second = conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);

        let ids: Vec<i64> = list_recent(&db, DEFAULT_CONVERSATION_LIST_LIMIT)
            .expect("liste")
            .iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, [second, first]);
    }

    /// Ses oturumlari da listede — modalite gorunur, filtre yok.
    #[test]
    fn voice_sessions_appear_in_the_list_too() {
        let db = fresh_db();
        conversation(&db, "2026-08-31T09:00:00Z", SessionModality::Voice);
        conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);

        let modalities: Vec<SessionModality> = list_recent(&db, DEFAULT_CONVERSATION_LIST_LIMIT)
            .expect("liste")
            .iter()
            .map(|row| row.modality)
            .collect();
        assert_eq!(modalities, [SessionModality::Text, SessionModality::Voice]);
    }

    #[test]
    fn the_title_and_the_project_label_are_carried() {
        let db = fresh_db();
        db.with_connection(|connection| {
            project_repository::ensure_optional_label(
                connection,
                Some("asuna"),
                "2026-08-31T10:00:00Z",
            )
        })
        .expect("proje etiketi acilmali");

        let id = session_repository::start_with_modality(
            &db,
            "gpt-4o-mini",
            Some("asuna"),
            SessionModality::Text,
            "2026-08-31T10:00:00Z",
        )
        .expect("konusma")
        .id;
        session_repository::set_title(&db, id, "Chat Shell plani").expect("baslik");

        let row = list_recent(&db, DEFAULT_CONVERSATION_LIST_LIMIT).expect("liste")[0].clone();
        assert_eq!(row.title.as_deref(), Some("Chat Shell plani"));
        assert_eq!(row.project_id.as_deref(), Some("asuna"));
    }

    /// Silinen konusma listeden gercekten cikar (mesajlari CASCADE ile gitti).
    #[test]
    fn a_deleted_conversation_leaves_the_list() {
        let db = fresh_db();
        let id = conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);
        message_repository::append(
            &db,
            id,
            MessageRole::User,
            "merhaba",
            "2026-08-31T10:00:01Z",
        )
        .expect("mesaj");

        session_repository::delete(&db, id).expect("konusma silinmeli");

        assert!(list_recent(&db, DEFAULT_CONVERSATION_LIST_LIMIT)
            .expect("liste")
            .is_empty());
    }

    #[test]
    fn the_limit_is_clamped_and_keeps_the_most_recent_conversations() {
        let db = fresh_db();
        let older = conversation(&db, "2026-08-30T10:00:00Z", SessionModality::Text);
        let newer = conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);

        let ids: Vec<i64> = list_recent(&db, 1)
            .expect("liste")
            .iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, [newer], "tavan en yeniyi tutmali");

        // `0` gecerli bir istek degil ama reddedilmez: 1'e kirpilir.
        assert_eq!(list_recent(&db, 0).expect("liste").len(), 1);
        assert!(list_recent(&db, u32::MAX)
            .expect("liste")
            .iter()
            .any(|row| row.id == older));
    }

    /// Bicim sozlesmesi `src/shared/chat.ts` → `parseConversationSummary`.
    #[test]
    fn the_row_serializes_with_the_contract_field_names() {
        let db = fresh_db();
        let id = conversation(&db, "2026-08-31T10:00:00Z", SessionModality::Text);
        session_repository::set_title(&db, id, "Baslik").expect("baslik");

        let row = list_recent(&db, DEFAULT_CONVERSATION_LIST_LIMIT).expect("liste")[0].clone();
        let json = serde_json::to_value(&row).expect("serialize");

        let mut keys: Vec<&str> = json
            .as_object()
            .expect("JSON nesnesi")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "id",
                "lastActivityAt",
                "messageCount",
                "modality",
                "projectId",
                "startedAt",
                "title",
            ]
        );
        assert_eq!(json["modality"], "text");
        assert_eq!(json["messageCount"], 0);
        assert_eq!(json["title"], "Baslik");
    }
}

//! Konusmaya eklenen dosyalar — `attachments` yazma ve okuma
//! (Chat Shell / migration 006).
//!
//! # Sozlesme
//!
//! - **Bu modul REDAKSIYON YAPMAZ.** [`store_record`] verilen metni oldugu gibi
//!   yazar. Redaksiyon (`redaction::redact_secrets`), boyut siniri, kirpma,
//!   dosya-adi blocklist'i ve sandbox kontrolu **komut katmanindadir**
//!   (`chat::attachment_ingest` / `chat::attachment_from_project`). Bu ayrim
//!   `tool_event_repository` ile bilerek farkli: orada ozetleme+redaksiyon
//!   repository'nin isiydi cunku girdi ham bir JSON'du ve tek bir dogru ozet
//!   bicimi vardi; burada girdi bir dosya icerigidir ve onu **nereden geldigine
//!   gore** farkli kapilardan gecirmek gerekir (yuklenen dosya vs. proje
//!   kokunden okunan dosya). Repository'nin ikisini de bilmesi, sandbox
//!   mantiginin ikinci bir kopyasini uretirdi.
//!
//!   Sonuc: bu modulu cagiran her yol, cagirmadan **once** redakte etmis
//!   olmalidir. Kolon adi (`content`) ve bu yorum sozlesmenin tamami; kod
//!   bunu zorlayamaz, ama iceriğin DB'ye giden tek yolu o iki komuttur.
//!
//! - **Icerik renderer'a donmez.** Komut yanitlarinda gecen tip
//!   [`AttachmentRecord`] ve o tipte `content` alani **yok**. Iceriğe ihtiyaci
//!   olan tek yer host tarafindaki `chat_send`; o da [`AttachmentPayload`]
//!   uzerinden okur — `Serialize` turetmeyen, yani bir `#[tauri::command]`
//!   yanitinda **donemeyen** bir tip.
//!
//! - **Sahiplik dogrulanir.** [`for_ids`] yalnizca verilen konusmaya ait
//!   kayitlari okur; listede baska bir konusmanin kimligi varsa istek
//!   **tamamen** reddedilir (sessizce filtrelenmez — model, kullanicinin
//!   gonderdigini sandigi bir dosyayi gormeden cevap vermemeli).

use rusqlite::{params_from_iter, OptionalExtension};
use tauri::State;

use super::model::{AttachmentOrigin, AttachmentRecord};
use super::session_repository;
use super::store_error::{database, StoreError};
use super::{clock, AsunaDb, DbState};

/// Dosya adinin azami uzunlugu — semadaki CHECK ile ayni.
pub const MAX_FILE_NAME_CHARS: usize = 255;

/// MIME turunun azami uzunlugu — semadaki CHECK ile ayni.
pub const MAX_MIME_TYPE_CHARS: usize = 128;

/// Saklanan iceriğin azami karakteri — semadaki CHECK ile ayni.
///
/// Komut katmani zaten 24.000 karaktere kirpiyor; bu tavan o kirpmanin ikinci
/// katmani. Asan icerik **kirpilmaz, reddedilir**: repository'nin kullanicinin
/// dosyasini sessizce kisaltmasi, kirpmayi gorunur kilan komut katmani
/// isaretini (`[... kirpildi ...]`) atlatmak olurdu.
pub const MAX_ATTACHMENT_CONTENT_CHARS: usize = 32_000;

/// Tek bir mesaja baglanabilecek azami ek sayisi.
///
/// [`for_ids`] sorgusundaki placeholder sayisini sinirlar (sinirsiz bir dizi,
/// sinirsiz uzunlukta bir SQL metni demek olurdu) ve ayni zamanda urun karari:
/// bir mesaja yirmiden fazla dosya eklemek prompt butcesini tek basina yer.
pub const MAX_ATTACHMENT_IDS: usize = 20;

// ---------------------------------------------------------------------------
// Girdi / cikti tipleri
// ---------------------------------------------------------------------------

/// Yazilacak attachment. `content` **onceden redakte edilmis** metindir
/// (modul dokumantasyonu).
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentDraft<'a> {
    pub session_id: i64,
    pub file_name: &'a str,
    /// `None` = tur bilinmiyor. Uydurulmaz.
    pub mime_type: Option<&'a str>,
    /// **Kaynak** dosyanin boyutu; saklanan (kirpilmis) metnin degil. `None` =
    /// bilinmiyor.
    pub size_bytes: Option<i64>,
    pub origin: AttachmentOrigin,
    pub content: &'a str,
}

/// Attachment kaydi + **icerigi** — yalnizca host tarafi icin.
///
/// GIZLILIK: bu tipte `Serialize` **bilerek yok**. Bir gun biri iceriği
/// renderer'a dondurmek isterse once bu satiri degistirmek zorunda kalir; yani
/// karar gorunur olur, kazara olmaz.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentPayload {
    pub record: AttachmentRecord,
    /// Redakte edilmis metin (DB'de ne varsa o).
    pub content: String,
}

// ---------------------------------------------------------------------------
// Dogrulama
// ---------------------------------------------------------------------------

fn validated_session_id(session_id: i64) -> Result<i64, StoreError> {
    if session_id <= 0 {
        return Err(StoreError::invalid("`sessionId` pozitif olmali"));
    }
    Ok(session_id)
}

/// Kimlik listesini tekillestirir ve siralar; gecersiz/asiri listeyi reddeder.
fn validated_ids(ids: &[i64]) -> Result<Vec<i64>, StoreError> {
    if ids.iter().any(|id| *id <= 0) {
        return Err(StoreError::invalid("`attachmentIds` pozitif olmali"));
    }
    let mut unique: Vec<i64> = ids.to_vec();
    unique.sort_unstable();
    unique.dedup();

    if unique.len() > MAX_ATTACHMENT_IDS {
        return Err(StoreError::invalid(
            "`attachmentIds` en fazla 20 kayit icerebilir",
        ));
    }
    Ok(unique)
}

/// `?1, ?2, ...` — **yalnizca** placeholder uretir; hicbir kullanici degeri
/// SQL metnine girmez (`conventions.md`: string birlestirmeyle sorgu kurulmaz).
fn placeholders(count: usize, offset: usize) -> String {
    (0..count)
        .map(|index| format!("?{}", index + offset + 1))
        .collect::<Vec<String>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

/// Attachment kaydini yazar.
///
/// **Redaksiyon yapmaz** — bkz. modul dokumantasyonu. Cagiran taraf iceriği
/// `redaction::redact_secrets`ten gecirmis ve kirpmis olmalidir.
///
/// Konusma yoksa `NotFound` doner.
pub(crate) fn store_record(
    db: &AsunaDb,
    draft: &AttachmentDraft<'_>,
    now: &str,
) -> Result<AttachmentRecord, StoreError> {
    let session_id = validated_session_id(draft.session_id)?;

    let file_name = draft.file_name.trim();
    if file_name.is_empty() {
        return Err(StoreError::invalid("`fileName` bos birakilamaz"));
    }
    if file_name.chars().count() > MAX_FILE_NAME_CHARS {
        return Err(StoreError::invalid(
            "`fileName` en fazla 255 karakter olabilir",
        ));
    }

    let mime_type = draft
        .mime_type
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if matches!(mime_type, Some(value) if value.chars().count() > MAX_MIME_TYPE_CHARS) {
        return Err(StoreError::invalid(
            "`mimeType` en fazla 128 karakter olabilir",
        ));
    }

    if matches!(draft.size_bytes, Some(size) if size < 0) {
        return Err(StoreError::invalid("`sizeBytes` negatif olamaz"));
    }

    // Kirpma degil **reddetme**: bkz. `MAX_ATTACHMENT_CONTENT_CHARS`.
    if draft.content.chars().count() > MAX_ATTACHMENT_CONTENT_CHARS {
        return Err(StoreError::invalid(
            "`content` en fazla 32000 karakter olabilir",
        ));
    }

    if !clock::is_utc_iso8601(now) {
        return Err(StoreError::invalid(
            "`now` UTC ISO-8601 olmali (orn. 2026-08-25T10:00:00Z)",
        ));
    }

    let record = db
        .with_connection(|connection| {
            let transaction = connection.transaction()?;
            if !session_repository::exists(&transaction, session_id)? {
                transaction.commit()?;
                return Ok(None);
            }

            transaction.execute(
                "INSERT INTO attachments
                   (session_id, message_id, file_name, mime_type, size_bytes, origin, content, created_at)
                 VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    session_id,
                    file_name,
                    mime_type,
                    draft.size_bytes,
                    draft.origin,
                    draft.content,
                    now,
                ],
            )?;
            let id = transaction.last_insert_rowid();
            let record = load(&transaction, id)?;
            transaction.commit()?;
            Ok(record)
        })
        .map_err(|error| StoreError::storage(error, "attachment_store_record"))?;

    record.ok_or(StoreError::NotFound)
}

/// Konusmanin attachment kayitlarini eskiden yeniye dondurur — **icerik yok**.
pub fn list_for_session(
    db: &AsunaDb,
    session_id: i64,
) -> Result<Vec<AttachmentRecord>, StoreError> {
    let session_id = validated_session_id(session_id)?;

    db.with_connection(|connection| {
        let mut statement = connection.prepare(&format!(
            "SELECT {columns} FROM attachments
              WHERE session_id = ?1
              ORDER BY id",
            columns = AttachmentRecord::select_columns()
        ))?;
        let rows = statement.query_map([session_id], AttachmentRecord::from_row)?;
        rows.collect::<rusqlite::Result<Vec<AttachmentRecord>>>()
    })
    .map_err(|error| StoreError::storage(error, "attachment_list"))
}

/// Verilen kimliklerin **bu konusmaya ait** kayitlarini icerigiyle okur.
///
/// # Sahiplik
///
/// Sorgu `session_id` ile kisitli. Istenen kimliklerden biri baska bir
/// konusmaya aitse ya da hic yoksa istek **tamamen** reddedilir
/// (`Invalid`) — ve iki durum bilerek **ayirt edilmez**: "bu id baska
/// konusmada var" demek, kullanicinin baska konusmalarindaki kayitlarin
/// varligini sizdiran bir sorgulama araci olurdu.
///
/// Sessiz filtreleme de yapilmaz: eksik bir ekle cevap uretmek, kullanicinin
/// gonderdigini sandigi dosyayi modelin hic gormemesi demektir.
///
/// Donen dizi `id` sirasindadir — cagiranin verdigi sira degil (deterministik
/// prompt uretimi icin).
pub(crate) fn for_ids(
    db: &AsunaDb,
    session_id: i64,
    ids: &[i64],
) -> Result<Vec<AttachmentPayload>, StoreError> {
    let session_id = validated_session_id(session_id)?;
    let ids = validated_ids(ids)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // `?1` = session_id, `?2..` = kimlikler. Metne yalnizca placeholder giriyor.
    let sql = format!(
        "SELECT {columns}, content FROM attachments
          WHERE session_id = ?1 AND id IN ({slots})
          ORDER BY id",
        columns = AttachmentRecord::select_columns(),
        slots = placeholders(ids.len(), 1)
    );

    let payloads = db
        .with_connection(|connection| {
            let mut statement = connection.prepare(&sql)?;
            let mut bindings: Vec<i64> = Vec::with_capacity(ids.len() + 1);
            bindings.push(session_id);
            bindings.extend_from_slice(&ids);

            let rows = statement.query_map(params_from_iter(bindings), |row| {
                Ok(AttachmentPayload {
                    record: AttachmentRecord::from_row(row)?,
                    content: row.get("content")?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<AttachmentPayload>>>()
        })
        .map_err(|error| StoreError::storage(error, "attachment_for_ids"))?;

    if payloads.len() != ids.len() {
        return Err(StoreError::invalid(
            "`attachmentIds` bu konusmaya ait olmayan bir kayit iceriyor",
        ));
    }
    Ok(payloads)
}

/// Bekleyen ekleri bir mesaja baglar (ayni transaction icinde).
///
/// `session_id` kosulu burada da var: [`for_ids`] zaten dogruladi, ama sahiplik
/// kontrolunun **yazma** sorgusunda da bulunmasi, ileride cagiranin sirayi
/// degistirmesi halinde sessiz bir bosluk birakmaz.
///
/// # Bir ek yalnizca **bir kez** baglanir (Gate 3 / M1)
///
/// `AND message_id IS NULL` kosulu ve satir sayisi kontrolu birlikte calisir:
/// zaten baglanmis bir ek ikinci bir mesaja **tasinamaz**. Onceki hali sessizce
/// `UPDATE` ediyordu, yani renderer eski bir ek kimligini tekrar gonderdiginde
/// dosya **eski mesajdan kopar** ve yeni mesaja gecerdi — kullanicinin gecmiste
/// gonderdigi mesaj, gozu onunde ekini kaybederdi.
///
/// Eslesme eksikse [`rusqlite::Error::StatementChangedRows`] doner ve cagiranin
/// transaction'i duser (kullanici mesaji da asistan yaniti da yazilmaz):
/// yarim baglanmis bir alisveris birakmaktansa istegin tamami reddedilir.
///
/// @returns baglanan satir sayisi (`ids.len()` ile ayni olmak zorunda).
pub(crate) fn link_to_message_in_tx(
    transaction: &rusqlite::Transaction<'_>,
    session_id: i64,
    message_id: i64,
    ids: &[i64],
) -> rusqlite::Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    // `?1` = message_id, `?2` = session_id, `?3..` = kimlikler.
    let sql = format!(
        "UPDATE attachments SET message_id = ?1
          WHERE session_id = ?2 AND message_id IS NULL AND id IN ({slots})",
        slots = placeholders(ids.len(), 2)
    );
    let mut bindings: Vec<i64> = Vec::with_capacity(ids.len() + 2);
    bindings.push(message_id);
    bindings.push(session_id);
    bindings.extend_from_slice(ids);

    let linked = transaction.execute(&sql, params_from_iter(bindings))?;
    if linked != ids.len() {
        return Err(rusqlite::Error::StatementChangedRows(linked));
    }
    Ok(linked)
}

fn load(connection: &rusqlite::Connection, id: i64) -> rusqlite::Result<Option<AttachmentRecord>> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM attachments WHERE id = ?1",
                AttachmentRecord::select_columns()
            ),
            [id],
            AttachmentRecord::from_row,
        )
        .optional()
}

// ---------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------

/// Konusmanin eklenen dosyalarini listeler (salt okuma, **icerik yok**).
///
/// Hafiza kapaliyken bos dizi doner (hata degil) — `session_list` ile ayni
/// sozlesme. Bicim: `src/shared/chat.ts` → `parseChatAttachmentList`.
#[tauri::command]
pub fn attachment_list(
    state: State<'_, DbState>,
    session_id: i64,
) -> Result<Vec<AttachmentRecord>, StoreError> {
    let Some(db) = database(&state)? else {
        return Ok(Vec::new());
    };
    list_for_session(db, session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::message_repository;
    use crate::db::model::MessageRole;
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

    fn draft(session_id: i64, file_name: &str) -> AttachmentDraft<'_> {
        AttachmentDraft {
            session_id,
            file_name,
            mime_type: Some("text/markdown"),
            size_bytes: Some(2_048),
            origin: AttachmentOrigin::Upload,
            content: "redakte edilmis metin",
        }
    }

    #[test]
    fn stores_a_record_without_a_message_link() {
        let db = fresh_db();
        let session_id = session(&db);

        let record = store_record(&db, &draft(session_id, "notlar.md"), NOW).expect("yazilmali");

        assert!(record.id > 0);
        assert_eq!(record.session_id, session_id);
        assert_eq!(record.message_id, None, "yeni ek bekleyen durumda olmali");
        assert_eq!(record.file_name, "notlar.md");
        assert_eq!(record.mime_type.as_deref(), Some("text/markdown"));
        assert_eq!(record.size_bytes, Some(2_048));
        assert_eq!(record.origin, AttachmentOrigin::Upload);
        assert_eq!(record.created_at, NOW);
    }

    /// Bos MIME turu `None`a duser: "tur soylenmedi" ile "tur bos metin" ayni
    /// sey degil.
    #[test]
    fn a_blank_mime_type_becomes_null() {
        let db = fresh_db();
        let session_id = session(&db);

        let record = store_record(
            &db,
            &AttachmentDraft {
                mime_type: Some("   "),
                ..draft(session_id, "notlar.txt")
            },
            NOW,
        )
        .expect("yazilmali");
        assert_eq!(record.mime_type, None);
    }

    #[test]
    fn invalid_drafts_are_rejected_before_the_database_is_touched() {
        let db = fresh_db();
        let session_id = session(&db);

        let cases: Vec<AttachmentDraft<'_>> = vec![
            AttachmentDraft {
                file_name: "   ",
                ..draft(session_id, "x")
            },
            AttachmentDraft {
                size_bytes: Some(-1),
                ..draft(session_id, "notlar.md")
            },
            AttachmentDraft {
                session_id: 0,
                ..draft(session_id, "notlar.md")
            },
        ];
        for case in cases {
            assert_eq!(
                store_record(&db, &case, NOW)
                    .expect_err("gecersiz taslak reddedilmeli")
                    .code(),
                StoreErrorCode::Invalid
            );
        }

        let long_name = "a".repeat(256);
        assert_eq!(
            store_record(&db, &draft(session_id, &long_name), NOW)
                .expect_err("uzun dosya adi")
                .code(),
            StoreErrorCode::Invalid
        );

        let long_content = "x".repeat(MAX_ATTACHMENT_CONTENT_CHARS + 1);
        assert_eq!(
            store_record(
                &db,
                &AttachmentDraft {
                    content: &long_content,
                    ..draft(session_id, "buyuk.txt")
                },
                NOW,
            )
            .expect_err("tavani asan icerik")
            .code(),
            StoreErrorCode::Invalid,
            "icerik kirpilmamali, reddedilmeli"
        );

        assert!(
            list_for_session(&db, session_id).expect("liste").is_empty(),
            "reddedilen taslak yazilmis"
        );
    }

    #[test]
    fn storing_into_an_unknown_conversation_reports_not_found() {
        let db = fresh_db();
        assert_eq!(
            store_record(&db, &draft(4242, "notlar.md"), NOW)
                .expect_err("bilinmeyen konusma")
                .code(),
            StoreErrorCode::NotFound
        );
    }

    /// Liste iceriği **tasimaz** — gizlilik kapisi tipin kendisinde.
    #[test]
    fn the_listing_never_carries_the_file_content() {
        let db = fresh_db();
        let session_id = session(&db);
        store_record(&db, &draft(session_id, "notlar.md"), NOW).expect("yazilmali");

        let records = list_for_session(&db, session_id).expect("liste okunmali");
        assert_eq!(records.len(), 1);

        let json = serde_json::to_value(&records[0]).expect("serialize");
        assert!(
            !json
                .as_object()
                .expect("JSON nesnesi")
                .contains_key("content"),
            "attachment listesi dosya icerigi tasiyor: {json}"
        );
    }

    #[test]
    fn attachments_are_scoped_to_their_conversation() {
        let db = fresh_db();
        let first = session(&db);
        let second = session(&db);
        store_record(&db, &draft(first, "birinci.md"), NOW).expect("yazilmali");
        store_record(&db, &draft(second, "ikinci.md"), NOW).expect("yazilmali");

        let records = list_for_session(&db, second).expect("liste okunmali");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].file_name, "ikinci.md");
    }

    #[test]
    fn for_ids_reads_the_content_of_the_owned_records() {
        let db = fresh_db();
        let session_id = session(&db);
        let first = store_record(&db, &draft(session_id, "bir.md"), NOW).expect("yazilmali");
        let second = store_record(
            &db,
            &AttachmentDraft {
                content: "ikinci icerik",
                ..draft(session_id, "iki.md")
            },
            NOW,
        )
        .expect("yazilmali");

        // Sira bilerek ters verildi: donen dizi `id` sirasinda olmali.
        let payloads = for_ids(&db, session_id, &[second.id, first.id]).expect("okunmali");
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0].record.id, first.id);
        assert_eq!(payloads[0].content, "redakte edilmis metin");
        assert_eq!(payloads[1].record.id, second.id);
        assert_eq!(payloads[1].content, "ikinci icerik");
    }

    /// **Sahiplik dogrulamasi**: baska bir konusmanin eki istenirse istek
    /// tamamen duser — sessizce filtrelenmez.
    #[test]
    fn for_ids_rejects_an_attachment_from_another_conversation() {
        let db = fresh_db();
        let mine = session(&db);
        let other = session(&db);
        let ours = store_record(&db, &draft(mine, "benim.md"), NOW).expect("yazilmali");
        let theirs = store_record(&db, &draft(other, "baskasi.md"), NOW).expect("yazilmali");

        let error = for_ids(&db, mine, &[ours.id, theirs.id]).expect_err("reddedilmeli");
        assert_eq!(error.code(), StoreErrorCode::Invalid);

        // Var olmayan bir kimlik **ayni** hatayi verir: iki durum ayirt
        // edilemez olmali (varlik sizdirmasi yok).
        let missing = for_ids(&db, mine, &[9_999]).expect_err("reddedilmeli");
        assert_eq!(missing.code(), StoreErrorCode::Invalid);
        assert_eq!(missing.to_string(), error.to_string());
    }

    #[test]
    fn for_ids_validates_the_id_list() {
        let db = fresh_db();
        let session_id = session(&db);

        assert!(
            for_ids(&db, session_id, &[])
                .expect("bos liste hata degil")
                .is_empty(),
            "eksiz mesaj gecerli bir durum"
        );

        for ids in [vec![0], vec![-3]] {
            assert_eq!(
                for_ids(&db, session_id, &ids)
                    .expect_err("gecersiz kimlik")
                    .code(),
                StoreErrorCode::Invalid
            );
        }

        let too_many: Vec<i64> = (1..=(MAX_ATTACHMENT_IDS as i64 + 1)).collect();
        assert_eq!(
            for_ids(&db, session_id, &too_many)
                .expect_err("tavani asan liste")
                .code(),
            StoreErrorCode::Invalid
        );
    }

    /// Ayni kimlik iki kez verilirse kayit bir kez doner (ve istek reddedilmez).
    #[test]
    fn for_ids_deduplicates_repeated_ids() {
        let db = fresh_db();
        let session_id = session(&db);
        let record = store_record(&db, &draft(session_id, "bir.md"), NOW).expect("yazilmali");

        let payloads = for_ids(&db, session_id, &[record.id, record.id]).expect("okunmali");
        assert_eq!(payloads.len(), 1);
    }

    #[test]
    fn links_pending_attachments_to_a_message() {
        let db = fresh_db();
        let session_id = session(&db);
        let other = session(&db);
        let mine = store_record(&db, &draft(session_id, "benim.md"), NOW).expect("yazilmali");
        let theirs = store_record(&db, &draft(other, "baskasi.md"), NOW).expect("yazilmali");

        let message_id = db
            .with_connection(|connection| {
                let transaction = connection.transaction()?;
                let message = message_repository::append_in_tx(
                    &transaction,
                    session_id,
                    MessageRole::User,
                    "dosyaya bak",
                    NOW,
                )?;
                let linked =
                    link_to_message_in_tx(&transaction, session_id, message.id, &[mine.id])?;
                assert_eq!(linked, 1);
                transaction.commit()?;
                Ok(message.id)
            })
            .expect("baglama calismali");

        let records = list_for_session(&db, session_id).expect("liste okunmali");
        assert_eq!(records[0].message_id, Some(message_id));

        let untouched = list_for_session(&db, other).expect("liste okunmali");
        assert_eq!(untouched[0].message_id, None, "{}", theirs.file_name);
    }

    /// Baska konusmanin eki `session_id` kosuluyla eslesmez; eksik eslesme artik
    /// **sessizce atlanmaz**, istegin tamami duser.
    #[test]
    fn linking_an_attachment_from_another_conversation_fails_the_whole_write() {
        let db = fresh_db();
        let session_id = session(&db);
        let other = session(&db);
        let mine = store_record(&db, &draft(session_id, "benim.md"), NOW).expect("yazilmali");
        let theirs = store_record(&db, &draft(other, "baskasi.md"), NOW).expect("yazilmali");

        let outcome = db.with_connection(|connection| {
            let transaction = connection.transaction()?;
            let message = message_repository::append_in_tx(
                &transaction,
                session_id,
                MessageRole::User,
                "dosyaya bak",
                NOW,
            )?;
            link_to_message_in_tx(&transaction, session_id, message.id, &[mine.id, theirs.id])?;
            transaction.commit()?;
            Ok(())
        });

        assert!(outcome.is_err(), "yabanci ek sessizce atlanmis");

        // Transaction dustu: ne mesaj ne de baglama kaldi.
        assert!(message_repository::list_for_session(&db, session_id, 10)
            .expect("okuma")
            .is_empty());
        for record in list_for_session(&db, session_id).expect("liste okunmali") {
            assert_eq!(record.message_id, None);
        }
    }

    /// **Gate 3 / M1**: baglanmis bir ek ikinci bir mesaja **tasinmaz**.
    ///
    /// Onceki hali sessizce `UPDATE` ediyordu: renderer eski bir ek kimligini
    /// tekrar gonderdiginde dosya gecmisteki mesajdan kopar ve yeni mesaja
    /// gecerdi — kullanici, gozu onunde duran eski mesajin ekini kaybederdi.
    #[test]
    fn an_already_linked_attachment_cannot_be_moved_to_another_message() {
        let db = fresh_db();
        let session_id = session(&db);
        let record = store_record(&db, &draft(session_id, "benim.md"), NOW).expect("yazilmali");

        let first_message = db
            .with_connection(|connection| {
                let transaction = connection.transaction()?;
                let message = message_repository::append_in_tx(
                    &transaction,
                    session_id,
                    MessageRole::User,
                    "ilk mesaj",
                    NOW,
                )?;
                link_to_message_in_tx(&transaction, session_id, message.id, &[record.id])?;
                transaction.commit()?;
                Ok(message.id)
            })
            .expect("ilk baglama calismali");

        let outcome = db.with_connection(|connection| {
            let transaction = connection.transaction()?;
            let message = message_repository::append_in_tx(
                &transaction,
                session_id,
                MessageRole::User,
                "ikinci mesaj",
                NOW,
            )?;
            link_to_message_in_tx(&transaction, session_id, message.id, &[record.id])?;
            transaction.commit()?;
            Ok(())
        });

        assert!(outcome.is_err(), "bagli ek ikinci kez baglanmis");

        // Eski mesajin eki **yerinde**; ikinci mesaj hic yazilmadi.
        let records = list_for_session(&db, session_id).expect("liste okunmali");
        assert_eq!(records[0].message_id, Some(first_message));

        let messages = message_repository::list_for_session(&db, session_id, 10).expect("okuma");
        assert_eq!(messages.len(), 1, "ikinci mesaj yazilmis");
        assert_eq!(messages[0].content, "ilk mesaj");
    }

    /// **Kabul kriteri 2**: konusma silinince eklenen dosyanin redakte edilmis
    /// icerigi de DB'den gider.
    #[test]
    fn deleting_the_conversation_removes_its_attachments() {
        let db = fresh_db();
        let session_id = session(&db);
        store_record(&db, &draft(session_id, "notlar.md"), NOW).expect("yazilmali");

        session_repository::delete(&db, session_id).expect("konusma silinmeli");

        let remaining: i64 = db
            .with_connection(|conn| {
                conn.query_row("SELECT count(*) FROM attachments", [], |row| row.get(0))
            })
            .expect("sayim okunmali");
        assert_eq!(remaining, 0);
    }

    #[test]
    fn placeholders_are_the_only_thing_interpolated_into_sql() {
        assert_eq!(placeholders(3, 1), "?2, ?3, ?4");
        assert_eq!(placeholders(1, 0), "?1");
    }
}

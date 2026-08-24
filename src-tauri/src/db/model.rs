//! `memories` ve `sessions` satir tipleri (ASU-030).
//!
//! # Tek kaynak disiplini
//!
//! Sema `migrations/001_memories_sessions.up.sql` icindedir. Buradaki tipler
//! onun **aynasidir**, ikinci bir tanim degil: testler kolon adlarini
//! `PRAGMA table_info` ile, `kind` degerlerini de semadaki CHECK kisiti ile
//! karsilastirir. Ayni zincirin ucuncu halkasi `src/shared/memory.ts` ve
//! `src/shared/session.ts`; onlar da ayni `.sql` dosyasini okuyan bir Vitest
//! testiyle bagli.
//!
//! Yani bir kolon eklemek/silmek, dokunulmayan her katmanda **kirmizi test**
//! uretir — sessiz kayma mumkun degil.
//!
//! # Serde
//!
//! `camelCase` uretir; `conventions.md` "Database": donusum repository
//! sinirinda yapilir, `snake_case` uygulama icine sizmaz.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::{Row, ToSql};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// MemoryKind
// ---------------------------------------------------------------------------

/// Hafiza siniflandirmasi — PROJECT.md Bolum 5.3.
///
/// Serbest metin **degil**: gecersiz bir deger hem IPC sinirinde (serde) hem
/// DB'de (CHECK kisiti) reddedilir. ADR-005 B/3'te olculen davranis budur —
/// `kind: "sql_injection"` DB'ye hic dokunmadan duser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// Kullanicinin kendisi hakkinda kalici bilgi.
    Profile,
    /// Kalici tercih ("kod yazarken kisa cevap").
    Preference,
    /// Bir projenin amaci/kimligi.
    Project,
    /// Mimari ya da urun karari.
    Decision,
    /// Acik/kapali is.
    Task,
    /// Oturum omurlu baglam. Durable tabloya **kural olarak terfi etmez**
    /// (PROJECT.md Bolum 14) — deger listede, cunku extraction'in bir adayi
    /// bu sinifta isaretleyip elemesi gerekebilir.
    WorkingContext,
    /// Kisi/ekip/entegrasyon iliskisi.
    Relationship,
    /// Fikir, not.
    Idea,
    /// Tekrarlayan aliskanlik/workflow.
    Routine,
    /// Bir tool'un kalici durumu (orn. secili editor).
    ToolState,
}

impl MemoryKind {
    /// Tum degerler — sira semadaki CHECK kisiti ile ayni (test ile baglidir).
    pub const ALL: [Self; 10] = [
        Self::Profile,
        Self::Preference,
        Self::Project,
        Self::Decision,
        Self::Task,
        Self::WorkingContext,
        Self::Relationship,
        Self::Idea,
        Self::Routine,
        Self::ToolState,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Preference => "preference",
            Self::Project => "project",
            Self::Decision => "decision",
            Self::Task => "task",
            Self::WorkingContext => "working_context",
            Self::Relationship => "relationship",
            Self::Idea => "idea",
            Self::Routine => "routine",
            Self::ToolState => "tool_state",
        }
    }

    /// Sessiz default **yok**: bilinmeyen deger `None` doner.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }
}

impl ToSql for MemoryKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for MemoryKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw = value.as_str()?;
        // DB'de CHECK kisiti var; buraya dusmek sema ile kodun kaydigi anlamina
        // gelir ve sessizce yutulmaz.
        Self::parse(raw).ok_or_else(|| FromSqlError::Other("bilinmeyen memories.kind".into()))
    }
}

// ---------------------------------------------------------------------------
// memories
// ---------------------------------------------------------------------------

/// `memories` satiri — `embedding` **haric**.
///
/// `embedding` bilerek disarida: MVP'de yazilmiyor (Stage B'ye ayrilmis,
/// memory.md Bolum 2) ve bir `BLOB`'u her okumada tasimak bos maliyet.
/// Testler bu istisnayi acikca dogrular; kolon sessizce unutulmus olamaz.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: i64,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub summary: Option<String>,
    /// Phase 4'e kadar serbest metin, FK'siz (ASU-039 migration'i baglayacak).
    pub project_id: Option<String>,
    pub importance: f64,
    pub confidence: f64,
    /// "Bu neden hatirlaniyor?" sorusunun cevabi (memory.md Bolum 2).
    pub source_session_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub last_accessed_at: Option<String>,
    pub expires_at: Option<String>,
    pub is_archived: bool,
    pub metadata_json: String,
}

/// `MemoryRecord`'un okudugu kolonlar — `SELECT` listesi ile ayni sira.
pub const MEMORY_COLUMNS: [&str; 15] = [
    "id",
    "kind",
    "title",
    "content",
    "summary",
    "project_id",
    "importance",
    "confidence",
    "source_session_id",
    "created_at",
    "updated_at",
    "last_accessed_at",
    "expires_at",
    "is_archived",
    "metadata_json",
];

/// Semada olup satir tipine **bilerek** alinmayan kolonlar.
pub const MEMORY_COLUMNS_NOT_LOADED: [&str; 1] = ["embedding"];

impl MemoryRecord {
    /// [`MEMORY_COLUMNS`] sirasiyla secilmis bir satiri okur.
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            kind: row.get("kind")?,
            title: row.get("title")?,
            content: row.get("content")?,
            summary: row.get("summary")?,
            project_id: row.get("project_id")?,
            importance: row.get("importance")?,
            confidence: row.get("confidence")?,
            source_session_id: row.get("source_session_id")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            last_accessed_at: row.get("last_accessed_at")?,
            expires_at: row.get("expires_at")?,
            is_archived: row.get("is_archived")?,
            metadata_json: row.get("metadata_json")?,
        })
    }

    /// Virgulle ayrilmis kolon listesi — repository'nin `SELECT`'i icin.
    /// Sabit metinden uretilir, kullanici girdisi hicbir zaman karismaz.
    pub fn select_columns() -> String {
        MEMORY_COLUMNS.join(", ")
    }
}

// ---------------------------------------------------------------------------
// sessions
// ---------------------------------------------------------------------------

/// Oturumun **nasil** kapandigi (ASU-033, migration 002).
///
/// `summary` bir durum bayragi degildir: oturum ozeti ASU-034'un memory
/// extraction girdisidir ve "Oturum beklenmedik sekilde kapandi" gibi bir
/// cumle oraya karisirsa hem gercek ozeti ezer hem uydurma hafiza uretir.
/// Durum bu yuzden ayri ve makine-okunur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndReason {
    /// `session_finalize` ile temiz kapanis.
    Completed,
    /// Cokme/kill sonrasi acilista kurtarildi; gercek bitis zamani bilinmiyor.
    Abandoned,
    /// Oturum bir hata ile sonlandi (renderer bildirir).
    Error,
}

impl SessionEndReason {
    /// Tum degerler — sira semadaki CHECK kisiti ile ayni (test ile baglidir).
    pub const ALL: [Self; 3] = [Self::Completed, Self::Abandoned, Self::Error];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
            Self::Error => "error",
        }
    }

    /// Sessiz default **yok**: bilinmeyen deger `None` doner.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.as_str() == raw)
    }
}

impl ToSql for SessionEndReason {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for SessionEndReason {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw = value.as_str()?;
        Self::parse(raw).ok_or_else(|| FromSqlError::Other("bilinmeyen sessions.end_reason".into()))
    }
}

/// `sessions` satiri.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub id: i64,
    pub started_at: String,
    /// `None` = oturum hala acik (ya da temiz kapanamadi).
    pub ended_at: Option<String>,
    pub project_id: Option<String>,
    /// `None` = ozet uretilmedi. Ozet basarisiz olsa da oturum kapanir.
    pub summary: Option<String>,
    /// `None` = transcript diske yazilmadi (`ASUNA_TRANSCRIPT_STORAGE=false`).
    /// Yol kullaniciya gosterilir: kendi makinesindeki kendi dosyasini
    /// bulabilmeli ve silebilmeli (PROJECT.md Bolum 20 incelenebilirlik).
    pub transcript_path: Option<String>,
    pub model: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub estimated_cost_usd: Option<f64>,
    /// Ham `Usage` kirilimi. Anahtarlar runtime'da dogrulanmadi (memory.md T5);
    /// netlestiginde ASU-032 yeni migration ile kolon acabilir.
    pub usage_json: Option<String>,
    pub created_at: String,
    /// `None` = bilinmiyor. Hala acik oturumlarda beklenen deger budur;
    /// migration 002 oncesinden kalan kayitlarda da bos kalabilir.
    pub end_reason: Option<SessionEndReason>,
}

/// Sema kolon sirasiyla ayni. `end_reason` **sonda**: `ALTER TABLE ADD COLUMN`
/// kolonu tablonun sonuna ekler (`PRAGMA table_info` sirasi budur).
pub const SESSION_COLUMNS: [&str; 14] = [
    "id",
    "started_at",
    "ended_at",
    "project_id",
    "summary",
    "transcript_path",
    "model",
    "input_tokens",
    "output_tokens",
    "total_tokens",
    "estimated_cost_usd",
    "usage_json",
    "created_at",
    "end_reason",
];

impl SessionRecord {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            started_at: row.get("started_at")?,
            ended_at: row.get("ended_at")?,
            project_id: row.get("project_id")?,
            summary: row.get("summary")?,
            transcript_path: row.get("transcript_path")?,
            model: row.get("model")?,
            input_tokens: row.get("input_tokens")?,
            output_tokens: row.get("output_tokens")?,
            total_tokens: row.get("total_tokens")?,
            estimated_cost_usd: row.get("estimated_cost_usd")?,
            usage_json: row.get("usage_json")?,
            created_at: row.get("created_at")?,
            end_reason: row.get("end_reason")?,
        })
    }

    pub fn select_columns() -> String {
        SESSION_COLUMNS.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use crate::db::AsunaDb;

    fn table_columns(db: &AsunaDb, table: &str) -> Vec<String> {
        db.with_connection(|conn| {
            // `table` sabit bir test girdisi; PRAGMA parametre kabul etmiyor.
            let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
            let rows = statement.query_map([], |row| row.get::<_, String>("name"))?;
            rows.collect::<rusqlite::Result<Vec<String>>>()
        })
        .expect("table_info okunmali")
    }

    /// Sema ile Rust enum'u ayni `kind` kumesini tanimali. Semaya bir deger
    /// eklenip enum unutulursa (ya da tersi) bu test duser.
    #[test]
    fn memory_kind_matches_the_schema_check_constraint() {
        let from_schema = migrations::kinds_declared_in_schema();
        let from_enum: Vec<String> = MemoryKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_owned())
            .collect();
        assert_eq!(from_enum, from_schema);
    }

    #[test]
    fn memory_kind_round_trips_through_serde() {
        for kind in MemoryKind::ALL {
            let json = serde_json::to_string(&kind).expect("serialize");
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
            let parsed: MemoryKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, kind);
        }
    }

    /// Bilinmeyen `kind` IPC sinirinde duser — DB'ye hic dokunulmaz.
    #[test]
    fn unknown_memory_kind_is_rejected() {
        assert_eq!(MemoryKind::parse("sql_injection"), None);
        assert_eq!(MemoryKind::parse("Preference"), None);
        assert!(serde_json::from_str::<MemoryKind>("\"project_decision\"").is_err());
    }

    /// Satir tipi ile gercek tablo kolonlari ortusmeli. Yeni bir kolon
    /// eklenip `MemoryRecord`'a alinmazsa, istisna listesine de yazilmadigi
    /// surece bu test duser.
    #[test]
    fn memory_columns_cover_the_table() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");
        let actual = table_columns(&db, "memories");

        let mut expected: Vec<String> = MEMORY_COLUMNS
            .iter()
            .chain(MEMORY_COLUMNS_NOT_LOADED.iter())
            .map(|name| (*name).to_owned())
            .collect();
        expected.sort();

        let mut actual_sorted = actual.clone();
        actual_sorted.sort();
        assert_eq!(actual_sorted, expected);

        // `embedding` MVP'de okunmuyor ama semada duruyor.
        assert!(actual.contains(&"embedding".to_owned()));
        assert!(!MEMORY_COLUMNS.contains(&"embedding"));
    }

    #[test]
    fn session_columns_cover_the_table() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");
        let mut actual = table_columns(&db, "sessions");
        actual.sort();

        let mut expected: Vec<String> = SESSION_COLUMNS
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        expected.sort();

        assert_eq!(actual, expected);
    }

    /// Sema ile Rust enum'u ayni `end_reason` kumesini tanimali (ASU-033).
    #[test]
    fn session_end_reason_matches_the_schema_check_constraint() {
        let from_schema = migrations::end_reasons_declared_in_schema();
        let from_enum: Vec<String> = SessionEndReason::ALL
            .iter()
            .map(|reason| reason.as_str().to_owned())
            .collect();
        assert_eq!(from_enum, from_schema);
    }

    #[test]
    fn unknown_session_end_reason_is_rejected() {
        assert_eq!(SessionEndReason::parse("kapandi"), None);
        assert_eq!(SessionEndReason::parse("Completed"), None);
        assert!(serde_json::from_str::<SessionEndReason>("\"crashed\"").is_err());
    }

    /// Satirdan tipe okuma gercekten calisiyor mu (kolon adi yazim hatasi
    /// derleme zamani yakalanmaz).
    #[test]
    fn records_round_trip_through_the_database() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");

        let (session, memory) = db
            .with_connection(|conn| {
                conn.execute(
                    "INSERT INTO sessions (started_at, ended_at, model, input_tokens, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?1)",
                    rusqlite::params![
                        "2026-08-25T10:00:00Z",
                        "2026-08-25T10:04:00Z",
                        "gpt-realtime-2.1",
                        120_i64
                    ],
                )?;
                let session_id = conn.last_insert_rowid();

                conn.execute(
                    "INSERT INTO memories
                       (kind, title, content, project_id, importance, confidence,
                        source_session_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    rusqlite::params![
                        MemoryKind::Decision,
                        "Wake word yerel kalir",
                        "Wake word tespiti bulutta degil, cihazda calisir.",
                        "asuna",
                        0.95_f64,
                        1.0_f64,
                        session_id,
                        "2026-08-25T10:05:00Z",
                    ],
                )?;

                let session = conn.query_row(
                    &format!("SELECT {} FROM sessions", SessionRecord::select_columns()),
                    [],
                    SessionRecord::from_row,
                )?;
                let memory = conn.query_row(
                    &format!("SELECT {} FROM memories", MemoryRecord::select_columns()),
                    [],
                    MemoryRecord::from_row,
                )?;
                Ok((session, memory))
            })
            .expect("kayitlar okunmali");

        assert_eq!(session.model, "gpt-realtime-2.1");
        assert_eq!(session.input_tokens, Some(120));
        assert_eq!(session.summary, None);
        assert_eq!(session.transcript_path, None);
        assert_eq!(session.end_reason, None, "yazilmayan durum uydurulmaz");

        assert_eq!(memory.kind, MemoryKind::Decision);
        assert_eq!(memory.project_id.as_deref(), Some("asuna"));
        assert_eq!(memory.source_session_id, Some(session.id));
        assert!(!memory.is_archived, "varsayilan arsivlenmemis olmali");
        assert_eq!(memory.metadata_json, "{}");
        assert_eq!(memory.last_accessed_at, None);
        assert_eq!(memory.expires_at, None);
    }

    /// Renderer'a giden JSON `camelCase`; `snake_case` uygulama icine sizmaz.
    #[test]
    fn records_serialize_as_camel_case() {
        let record = MemoryRecord {
            id: 1,
            kind: MemoryKind::ToolState,
            title: "t".to_owned(),
            content: "c".to_owned(),
            summary: None,
            project_id: None,
            importance: 0.5,
            confidence: 0.5,
            source_session_id: None,
            created_at: "2026-08-25T10:00:00Z".to_owned(),
            updated_at: "2026-08-25T10:00:00Z".to_owned(),
            last_accessed_at: None,
            expires_at: None,
            is_archived: false,
            metadata_json: "{}".to_owned(),
        };

        let json = serde_json::to_value(&record).expect("serialize");
        let object = json.as_object().expect("JSON nesnesi");
        assert!(object.contains_key("projectId"));
        assert!(object.contains_key("sourceSessionId"));
        assert!(object.contains_key("isArchived"));
        assert!(!object.contains_key("project_id"));
        assert!(!object.contains_key("embedding"));
        assert_eq!(json["kind"], "tool_state");
    }
}

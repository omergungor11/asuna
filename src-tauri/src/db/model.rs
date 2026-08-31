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

// ---------------------------------------------------------------------------
// projects
// ---------------------------------------------------------------------------

/// Kayitli projenin durumu (ASU-039, migration 003).
///
/// Degerler semadaki CHECK kisitindan gelir ve `src/shared/project.ts`
/// `PROJECT_STATUSES` ile testlerle baglidir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    /// Kayitli ve yolu erisilebilir.
    Active,
    /// Kayitli ama yol artik yok. Kayit **silinmez** — kullanici diski
    /// baglamayi unutmus olabilir (ASU-040).
    Missing,
    /// Kullanici gecmis icin tutuyor; aktif calisilmiyor.
    Archived,
    /// Kayitli kok **yok**: yalnizca hafizada gecen bir proje etiketi.
    /// `path` bu durumda her zaman NULL'dur ve satir hicbir dosya sistemi
    /// yetkisi tasimaz (bkz. `db::project_repository`).
    Unlinked,
}

impl ProjectStatus {
    /// Tum degerler — sira semadaki CHECK kisiti ile ayni (test ile baglidir).
    pub const ALL: [Self; 4] = [Self::Active, Self::Missing, Self::Archived, Self::Unlinked];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Missing => "missing",
            Self::Archived => "archived",
            Self::Unlinked => "unlinked",
        }
    }

    /// Sessiz default **yok**: bilinmeyen deger `None` doner.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == raw)
    }

    /// Bu durumdaki bir projenin kayitli bir kok dizini var mi?
    ///
    /// `Unlinked` disindaki her durum bir yol tasir; sema bunu CHECK ile de
    /// zorlar. Sandbox (ASU-049) yalnizca yolu olan kayitlari gorecek.
    pub const fn has_registered_root(self) -> bool {
        !matches!(self, Self::Unlinked)
    }
}

impl ToSql for ProjectStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for ProjectStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw = value.as_str()?;
        Self::parse(raw).ok_or_else(|| FromSqlError::Other("bilinmeyen projects.status".into()))
    }
}

/// `projects` satiri (PROJECT.md Bolum 12.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    /// Slug (`asuna`). INTEGER degil: `memories.project_id` 001'den beri TEXT.
    pub id: String,
    pub name: String,
    /// Normalize edilmis, symlink'i cozulmus mutlak yol.
    /// `None` yalnizca [`ProjectStatus::Unlinked`] icin mumkundur.
    pub path: Option<String>,
    pub description: Option<String>,
    pub status: ProjectStatus,
    pub primary_language: Option<String>,
    pub framework: Option<String>,
    /// Remote **adi** — kimlik bilgisi/token tasiyan URL buraya yazilmaz
    /// (ASU-042 redaksiyondan gecirir).
    pub git_remote: Option<String>,
    /// `None` = hic acilmadi. Tahmin edilmez; kullanicinin acik secimiyle dolar.
    pub last_opened_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata_json: String,
}

/// `ProjectRecord`'un okudugu kolonlar — sema kolon sirasiyla ayni.
pub const PROJECT_COLUMNS: [&str; 12] = [
    "id",
    "name",
    "path",
    "description",
    "status",
    "primary_language",
    "framework",
    "git_remote",
    "last_opened_at",
    "created_at",
    "updated_at",
    "metadata_json",
];

impl ProjectRecord {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            path: row.get("path")?,
            description: row.get("description")?,
            status: row.get("status")?,
            primary_language: row.get("primary_language")?,
            framework: row.get("framework")?,
            git_remote: row.get("git_remote")?,
            last_opened_at: row.get("last_opened_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            metadata_json: row.get("metadata_json")?,
        })
    }

    pub fn select_columns() -> String {
        PROJECT_COLUMNS.join(", ")
    }
}

// ---------------------------------------------------------------------------
// tool_events (ASU-050)
// ---------------------------------------------------------------------------

/// Tool cagrisinin risk seviyesi — PROJECT.md Bolum 5.4.
///
/// DB'de `INTEGER`, IPC'de sayi (TypeScript `ToolRisk = 0 | 1 | 2 | 3`).
/// Serbest bir `u8` **degil**: `risk: 7` gonderen bir istek serde sinirinde
/// duser, DB'ye hic dokunmadan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub enum ToolRiskLevel {
    /// Salt okuma; yan etkisi yok.
    ReadOnly = 0,
    /// Geri alinabilir dusuk risk (orn. editorde proje acmak).
    LowRisk = 1,
    /// Mutation — dosya duzenleme, paket kurma, build.
    Mutation = 2,
    /// Destructive / harici etki — silme, push, mail, deploy, harcama.
    Destructive = 3,
}

impl ToolRiskLevel {
    /// Tum degerler — sira semadaki CHECK kisiti ile ayni (test ile baglidir).
    pub const ALL: [Self; 4] = [
        Self::ReadOnly,
        Self::LowRisk,
        Self::Mutation,
        Self::Destructive,
    ];

    pub const fn as_i64(self) -> i64 {
        self as i64
    }

    /// Sessiz default **yok**: bilinmeyen deger `None` doner.
    pub fn parse(raw: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.as_i64() == raw)
    }

    /// Bu seviye ASU-048'de **her zaman** acik onay gerektiriyor mu?
    ///
    /// `asuna-config/security.md` Bolum 3 ve `conventions.md`: risk 2 ve 3 icin
    /// `requiresApproval` hicbir `ASUNA_TOOL_APPROVAL_MODE` degeriyle
    /// gevsetilemez. Audit tarafi bunu zorlamaz (mod bilgisi burada yok) ama
    /// politika katmani ile ayni tanimi paylasmasi, iki yerde iki farkli
    /// "yuksek risk" tanimi olusmasini engeller.
    pub const fn always_requires_approval(self) -> bool {
        matches!(self, Self::Mutation | Self::Destructive)
    }
}

impl From<ToolRiskLevel> for i64 {
    fn from(value: ToolRiskLevel) -> Self {
        value.as_i64()
    }
}

impl TryFrom<i64> for ToolRiskLevel {
    type Error = String;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::parse(value).ok_or_else(|| {
            // Mesaj yalnizca beklenen bicimi soyler; gelen degeri tekrarlamak
            // bir sizinti riski degil ama gerekli de degil.
            "`riskLevel` 0, 1, 2 ya da 3 olmali".to_owned()
        })
    }
}

impl ToSql for ToolRiskLevel {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_i64()))
    }
}

impl FromSql for ToolRiskLevel {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw = value.as_i64()?;
        Self::parse(raw)
            .ok_or_else(|| FromSqlError::Other("bilinmeyen tool_events.risk_level".into()))
    }
}

/// Bir tool cagrisinin onay yolculugunun sonucu (ASU-050, migration 004).
///
/// Alti degerin **hepsi** ayri bir gercek durumu anlatir; "onaylanmadi" tek bir
/// kovaya konsaydi kullanici, kendisinin reddettigi bir cagri ile onay
/// penceresi hic acilmadan dusen bir cagriyi ayirt edemezdi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalState {
    /// Bu risk seviyesi bu modda onay gerektirmiyordu (risk 0).
    NotRequired,
    /// Onay gerekebilirdi ama `ASUNA_TOOL_APPROVAL_MODE` izin verdi.
    /// [`Self::NotRequired`]'dan bilerek ayri: "sorulabilirdi, ayarin izin
    /// verdi" demek, ayari sonradan sorgulanabilir kilar.
    AutoApproved,
    /// Kullanici acikca onayladi.
    Approved,
    /// Kullanici acikca reddetti.
    Denied,
    /// Onay istegi zaman asimina ugradi → varsayilan **reddet** (ASU-048).
    Timeout,
    /// Onay asamasina hic gelinmedi: cagri daha once dustu (sema dogrulamasi,
    /// bilinmeyen tool adi, sandbox on-kontrolu). [`Self::NotRequired`] ile
    /// karistirilmamali — orada onay GEREKMEDI, burada onay SORULAMADI.
    NotRequested,
}

impl ToolApprovalState {
    /// Tum degerler — sira semadaki CHECK kisiti ile ayni (test ile baglidir).
    pub const ALL: [Self; 6] = [
        Self::NotRequired,
        Self::AutoApproved,
        Self::Approved,
        Self::Denied,
        Self::Timeout,
        Self::NotRequested,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::AutoApproved => "auto_approved",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Timeout => "timeout",
            Self::NotRequested => "not_requested",
        }
    }

    /// Sessiz default **yok**: bilinmeyen deger `None` doner.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == raw)
    }

    /// Bu durumda tool **calistirildi mi**?
    ///
    /// UI'in "reddedildi" ile "calisti ama hata verdi"yi ayirmasi icin gerekli:
    /// calismamis bir cagrida `result_summary` NULL'dur ve bos bir ozet
    /// uydurulmaz.
    pub const fn permitted_execution(self) -> bool {
        matches!(
            self,
            Self::NotRequired | Self::AutoApproved | Self::Approved
        )
    }
}

impl ToSql for ToolApprovalState {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for ToolApprovalState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw = value.as_str()?;
        Self::parse(raw)
            .ok_or_else(|| FromSqlError::Other("bilinmeyen tool_events.approval_state".into()))
    }
}

/// Bir tool cagrisinin **sonucu** (ASU-051, migration 005).
///
/// [`ToolApprovalState`] ile ayni sey degil ve karistirilmamali: onay durumu
/// "calismasina izin verildi mi", bu ise "calisti mi ve isini yapabildi mi"
/// sorusunu cevaplar. Ikisi bagimsiz eksenler —
/// `approved` + `Failed` gecerli ve sik bir kombinasyon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    /// Tool calisti ve isini yapti.
    Succeeded,
    /// Tool **calisti** ama isini yapamadi: implementasyon hatasi, sandbox
    /// reddi, timeout. [`Self::NotRun`] degil — yan etkisi olabilecek bir cagri
    /// "hic olmadi" diye kaydedilmemeli.
    Failed,
    /// Tool **hic** calismadi: sema reddi, onay reddi/zaman asimi, cagri
    /// baslamadan iptal. Yan etki ihtimali yok.
    NotRun,
}

impl ToolOutcome {
    /// Tum degerler — sira semadaki CHECK kisiti ile ayni (test ile baglidir).
    pub const ALL: [Self; 3] = [Self::Succeeded, Self::Failed, Self::NotRun];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::NotRun => "not_run",
        }
    }

    /// Sessiz default **yok**: bilinmeyen deger `None` doner.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|outcome| outcome.as_str() == raw)
    }
}

impl ToSql for ToolOutcome {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for ToolOutcome {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw = value.as_str()?;
        Self::parse(raw).ok_or_else(|| FromSqlError::Other("bilinmeyen tool_events.outcome".into()))
    }
}

/// `tool_events` satiri (PROJECT.md Bolum 12.2 + ASU-051 `outcome`).
///
/// Salt yazilir bir denetim satiridir: repository katmaninda `UPDATE` ya da
/// `DELETE` yolu yoktur ve renderer'a acilan komut kumesinde de yoktur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolEventRecord {
    pub id: i64,
    /// `None` = cagriyi ureten oturum bilinmiyor ya da oturum kaydi silinmis
    /// (FK `ON DELETE SET NULL`). Uydurulmus bir korelasyon kimligi yazilmaz.
    pub session_id: Option<i64>,
    pub tool_name: String,
    pub risk_level: ToolRiskLevel,
    /// Anahtar adlari + kirpilmis skaler degerlerden olusan **tek satirlik**
    /// ozet; ic ice yapilar yalnizca sekil olarak gorunur. Ham arguman degil
    /// (bkz. `db::tool_event_repository::summarize_arguments`).
    /// `None` = cagri argumansizdi.
    pub arguments_redacted: Option<String>,
    pub approval_state: ToolApprovalState,
    /// Kisa, insan diliyle sonuc — **basari da hata da** buraya yazilir
    /// (`conventions.md`: "tool basarisi taklit edilmez"). `None` = soylenecek
    /// bir sonuc yok; tipik olarak cagri hic calismadi.
    pub result_summary: Option<String>,
    pub created_at: String,
    /// Cagri calisti mi, calistiysa basardi mi? (ASU-051, migration 005).
    ///
    /// `None` = satir 005 oncesinde yazildi ve bu eksen o zaman tutulmuyordu.
    /// Geriye donuk **uydurulmaz**: `approval_state = approved` bir cagrinin
    /// basarili bittigini soylemez.
    pub outcome: Option<ToolOutcome>,
}

/// `ToolEventRecord`'un okudugu kolonlar — sema kolon sirasiyla ayni.
///
/// `outcome` **sonda**: `ALTER TABLE ... ADD COLUMN` kolonu tablonun sonuna
/// koyar ve `PRAGMA table_info` sirasi budur (`sessions.end_reason` ile ayni).
pub const TOOL_EVENT_COLUMNS: [&str; 9] = [
    "id",
    "session_id",
    "tool_name",
    "risk_level",
    "arguments_redacted",
    "approval_state",
    "result_summary",
    "created_at",
    "outcome",
];

impl ToolEventRecord {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            session_id: row.get("session_id")?,
            tool_name: row.get("tool_name")?,
            risk_level: row.get("risk_level")?,
            arguments_redacted: row.get("arguments_redacted")?,
            approval_state: row.get("approval_state")?,
            result_summary: row.get("result_summary")?,
            created_at: row.get("created_at")?,
            outcome: row.get("outcome")?,
        })
    }

    pub fn select_columns() -> String {
        TOOL_EVENT_COLUMNS.join(", ")
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
                // 003'ten beri `project_id` bir yabanci anahtar: etiketin bir
                // karsiligi olmali (bkz. `db::project_repository::ensure_label`).
                conn.execute(
                    "INSERT INTO projects (id, name, path, status, created_at, updated_at)
                     VALUES ('asuna', 'asuna', NULL, 'unlinked', ?1, ?1)",
                    rusqlite::params!["2026-08-25T10:00:00Z"],
                )?;
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

    // --- projects (ASU-039) -------------------------------------------------

    #[test]
    fn project_status_matches_the_schema_check_constraint() {
        let from_schema = migrations::project_statuses_declared_in_schema();
        let from_enum: Vec<String> = ProjectStatus::ALL
            .iter()
            .map(|status| status.as_str().to_owned())
            .collect();
        assert_eq!(from_enum, from_schema);
    }

    #[test]
    fn unknown_project_status_is_rejected() {
        assert_eq!(ProjectStatus::parse("silinmis"), None);
        assert_eq!(ProjectStatus::parse("Active"), None);
        assert!(serde_json::from_str::<ProjectStatus>("\"deleted\"").is_err());
    }

    /// Yalnizca `unlinked` bir projenin kayitli koku yoktur; sandbox (ASU-049)
    /// bu ayrima gore filtreleyecek.
    #[test]
    fn only_unlinked_projects_lack_a_registered_root() {
        for status in ProjectStatus::ALL {
            assert_eq!(
                status.has_registered_root(),
                status != ProjectStatus::Unlinked,
                "{status:?}"
            );
        }
    }

    #[test]
    fn project_columns_cover_the_table() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");
        let mut actual = table_columns(&db, "projects");
        actual.sort();

        let mut expected: Vec<String> = PROJECT_COLUMNS
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        expected.sort();

        assert_eq!(actual, expected);
    }

    #[test]
    fn project_rows_round_trip_through_the_database() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");

        let project = db
            .with_connection(|conn| {
                conn.execute(
                    "INSERT INTO projects
                       (id, name, path, description, status, primary_language, framework,
                        git_remote, last_opened_at, created_at, updated_at, metadata_json)
                     VALUES ('asuna', 'Asuna', '/tmp/asuna', 'Sesli companion', ?1,
                             'TypeScript', 'Tauri', 'github.com/omergungor/asuna',
                             '2026-08-25T11:00:00Z', ?2, ?2, '{\"pinned\":true}')",
                    rusqlite::params![ProjectStatus::Active, "2026-08-25T10:00:00Z"],
                )?;
                conn.query_row(
                    &format!("SELECT {} FROM projects", ProjectRecord::select_columns()),
                    [],
                    ProjectRecord::from_row,
                )
            })
            .expect("kayit okunmali");

        assert_eq!(project.id, "asuna");
        assert_eq!(project.status, ProjectStatus::Active);
        assert_eq!(project.path.as_deref(), Some("/tmp/asuna"));
        assert_eq!(project.primary_language.as_deref(), Some("TypeScript"));
        assert_eq!(
            project.git_remote.as_deref(),
            Some("github.com/omergungor/asuna")
        );

        let json = serde_json::to_value(&project).expect("serialize");
        let object = json.as_object().expect("JSON nesnesi");
        assert!(object.contains_key("primaryLanguage"));
        assert!(object.contains_key("lastOpenedAt"));
        assert!(object.contains_key("gitRemote"));
        assert!(!object.contains_key("primary_language"));
        assert_eq!(json["status"], "active");
    }

    // --- tool_events (ASU-050) ----------------------------------------------

    #[test]
    fn tool_approval_state_matches_the_schema_check_constraint() {
        let from_schema = migrations::approval_states_declared_in_schema();
        let from_enum: Vec<String> = ToolApprovalState::ALL
            .iter()
            .map(|state| state.as_str().to_owned())
            .collect();
        assert_eq!(from_enum, from_schema);
    }

    #[test]
    fn tool_risk_level_matches_the_schema_check_constraint() {
        let from_schema = migrations::risk_levels_declared_in_schema();
        let from_enum: Vec<String> = ToolRiskLevel::ALL
            .iter()
            .map(|level| level.as_i64().to_string())
            .collect();
        assert_eq!(from_enum, from_schema);
    }

    /// ASU-051: sonuc kumesi de sema metnine baglidir; bir deger eklenip enum
    /// unutulursa (ya da tersi) bu test duser.
    #[test]
    fn tool_outcome_matches_the_schema_check_constraint() {
        let from_schema = migrations::outcomes_declared_in_schema();
        let from_enum: Vec<String> = ToolOutcome::ALL
            .iter()
            .map(|outcome| outcome.as_str().to_owned())
            .collect();
        assert_eq!(from_enum, from_schema);
    }

    /// Onay durumu ile sonuc **ayri** eksenler: kumeler kesismemeli, aksi halde
    /// bir denetim satirini okurken hangi sorunun cevabini gordugumuz belirsiz
    /// olurdu.
    #[test]
    fn approval_state_and_outcome_are_disjoint_vocabularies() {
        for outcome in ToolOutcome::ALL {
            assert_eq!(
                ToolApprovalState::parse(outcome.as_str()),
                None,
                "`{}` hem onay durumu hem sonuc olarak okunabiliyor",
                outcome.as_str()
            );
        }
        for state in ToolApprovalState::ALL {
            assert_eq!(ToolOutcome::parse(state.as_str()), None, "{state:?}");
        }
    }

    #[test]
    fn unknown_outcome_is_rejected() {
        assert_eq!(ToolOutcome::parse("basarili"), None);
        assert_eq!(ToolOutcome::parse("Succeeded"), None);
        assert_eq!(ToolOutcome::parse(""), None);
        assert!(serde_json::from_str::<ToolOutcome>("\"skipped\"").is_err());

        for outcome in ToolOutcome::ALL {
            let json = serde_json::to_string(&outcome).expect("serialize");
            assert_eq!(json, format!("\"{}\"", outcome.as_str()));
            assert_eq!(
                serde_json::from_str::<ToolOutcome>(&json).expect("deserialize"),
                outcome
            );
        }
    }

    #[test]
    fn unknown_approval_state_is_rejected() {
        assert_eq!(ToolApprovalState::parse("onaylandi"), None);
        assert_eq!(ToolApprovalState::parse("Approved"), None);
        assert!(serde_json::from_str::<ToolApprovalState>("\"skipped\"").is_err());
    }

    /// Uydurulmus bir risk seviyesi IPC sinirinde duser — DB'ye dokunulmaz.
    #[test]
    fn unknown_risk_level_is_rejected_at_the_serde_boundary() {
        assert_eq!(ToolRiskLevel::parse(4), None);
        assert_eq!(ToolRiskLevel::parse(-1), None);
        assert!(serde_json::from_str::<ToolRiskLevel>("7").is_err());
        assert!(serde_json::from_str::<ToolRiskLevel>("\"0\"").is_err());

        for level in ToolRiskLevel::ALL {
            let json = serde_json::to_string(&level).expect("serialize");
            assert_eq!(json, level.as_i64().to_string());
            assert_eq!(
                serde_json::from_str::<ToolRiskLevel>(&json).expect("deserialize"),
                level
            );
        }
    }

    /// `security.md` Bolum 3: risk 2/3 **her zaman** onay ister; bu tanim
    /// politika katmani ile paylasilir, iki yerde iki tanim olusmaz.
    #[test]
    fn only_mutation_and_destructive_always_require_approval() {
        assert!(!ToolRiskLevel::ReadOnly.always_requires_approval());
        assert!(!ToolRiskLevel::LowRisk.always_requires_approval());
        assert!(ToolRiskLevel::Mutation.always_requires_approval());
        assert!(ToolRiskLevel::Destructive.always_requires_approval());
    }

    /// "Calisti mi?" sorusunun cevabi tek yerde tanimli: reddedilen, zaman
    /// asimina ugrayan ve onaya hic gitmeyen cagrilarda tool CALISMADI.
    #[test]
    fn only_the_three_permissive_states_mean_the_tool_ran() {
        for state in ToolApprovalState::ALL {
            let expected = matches!(
                state,
                ToolApprovalState::NotRequired
                    | ToolApprovalState::AutoApproved
                    | ToolApprovalState::Approved
            );
            assert_eq!(state.permitted_execution(), expected, "{state:?}");
        }
    }

    #[test]
    fn tool_event_columns_cover_the_table() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");
        let mut actual = table_columns(&db, "tool_events");
        actual.sort();

        let mut expected: Vec<String> = TOOL_EVENT_COLUMNS
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        expected.sort();

        assert_eq!(actual, expected);
    }

    #[test]
    fn tool_event_rows_round_trip_through_the_database() {
        let db = AsunaDb::open_in_memory().expect("DB acilmali");

        let event = db
            .with_connection(|conn| {
                conn.execute(
                    "INSERT INTO sessions (started_at, model, created_at)
                     VALUES (?1, 'gpt-realtime-2.1', ?1)",
                    rusqlite::params!["2026-08-25T10:00:00Z"],
                )?;
                let session_id = conn.last_insert_rowid();

                conn.execute(
                    "INSERT INTO tool_events
                       (session_id, tool_name, risk_level, arguments_redacted,
                        approval_state, result_summary, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        session_id,
                        "open_project",
                        ToolRiskLevel::LowRisk,
                        "projectId=asuna",
                        ToolApprovalState::Approved,
                        "Proje VS Code ile acildi.",
                        "2026-08-25T10:01:00Z",
                    ],
                )?;

                conn.query_row(
                    &format!(
                        "SELECT {} FROM tool_events",
                        ToolEventRecord::select_columns()
                    ),
                    [],
                    ToolEventRecord::from_row,
                )
            })
            .expect("kayit okunmali");

        assert_eq!(event.tool_name, "open_project");
        assert_eq!(event.risk_level, ToolRiskLevel::LowRisk);
        assert_eq!(event.approval_state, ToolApprovalState::Approved);
        assert_eq!(event.arguments_redacted.as_deref(), Some("projectId=asuna"));
        assert_eq!(event.session_id, Some(1));

        let json = serde_json::to_value(&event).expect("serialize");
        let object = json.as_object().expect("JSON nesnesi");
        assert!(object.contains_key("sessionId"));
        assert!(object.contains_key("riskLevel"));
        assert!(object.contains_key("argumentsRedacted"));
        assert!(object.contains_key("approvalState"));
        assert!(object.contains_key("resultSummary"));
        assert!(!object.contains_key("risk_level"));
        assert_eq!(json["approvalState"], "approved");
        assert_eq!(json["riskLevel"], 1);
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

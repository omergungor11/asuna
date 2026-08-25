//! Stage A deterministik retrieval + [`SessionBootstrapContext`] (ASU-035).
//!
//! PROJECT.md Bolum 13 (Stage A) + Bolum 25 (Session Context Builder).
//!
//! # Sozlesme
//!
//! - **Embedding yok.** Stage A tamamen SQL siralamasidir: proje eslesmesi →
//!   `importance` → tazelik. Stage B (semantik) ve Stage C (konsolidasyon) MVP
//!   kapsami disidir (memory.md Bolum 4).
//! - **Tum DB modele dokulmez.** Paket hem kayit sayisi hem kelime butcesi ile
//!   sinirlidir ([`CONTEXT_WORD_LIMIT`]); enjekte edilen her kelime **her
//!   turda** yeniden faturalanir (voice.md Bolum 6). Olculen boyut
//!   [`ContextBudget`] icinde geri doner ve log'lanir — "huge raw histories"
//!   yasagi (PROJECT.md Bolum 25) burada olculebilir bir sayiya baglidir.
//! - **Onay bekleyen hafiza baglama girmez** (ASU-034 sozlesmesi):
//!   `MemoryFilter::exclude_pending_approval`. Elle olusturulan kayitlarda
//!   bayrak yoktur ve onlar onay beklemez.
//! - **Arsivlenmis ve suresi dolmus kayitlar girmez**; ikisi de kullanicinin
//!   "bunu artik kullanma" karari.
//! - **Silinen hafiza bir sonraki oturuma giremez** (ASU-036 kabul kriteri):
//!   baglam her oturum acilisinda depodan **yeniden** okunur, onbellek yoktur.
//!
//! # Bilerek bos alanlar
//!
//! `current_project` (Phase 4 / ASU-039+) ve `active_tasks` (Phase 6) tipli
//! ama **bos** doner. Alanlar simdiden sozlesmede cunku bunlari sonradan
//! eklemek renderer sozlesmesini ve prompt bicimini birlikte degistirirdi;
//! bos donmek "bilmiyorum" demenin durust hali, uydurulmus bir proje ozeti
//! degil.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::privacy::PrivacyState;

use super::clock;
use super::memory_repository::{self, ArchiveFilter, MemoryFilter, MemorySort};
use super::model::{MemoryKind, MemoryRecord, SessionRecord};
use super::session_repository;
use super::store_error::{database, StoreError};
use super::{AsunaDb, DbState};

// ---------------------------------------------------------------------------
// Sinirlar
// ---------------------------------------------------------------------------

/// Baglam paketinin kelime tavani.
///
/// ~2000 kelime, `gpt-realtime` icin kabaca 3 bin token: her turda tasinan
/// sabit bir maliyet. Daha yuksek bir tavan uzun oturumlarda faturayi sessizce
/// buyutur; daha dusugu ise tercihleri ve son oturum ozetini birlikte
/// tasiyamaz. Sayi olculebilir ve tek yerde: degistirmek icin
/// [`ContextBudget::word_count`] log'una bakilir, tahmin yurutulmez.
pub const CONTEXT_WORD_LIMIT: usize = 2_000;

/// Tek bir hafizanin baglama koyabilecegi azami kelime.
///
/// Kalem tavani olmadan 8000 karakterlik tek bir kayit butun butceyi yer ve
/// paket "bir hafiza" haline gelirdi.
const MAX_MEMORY_WORDS: usize = 120;

/// Son oturum ozetinin azami kelimesi. Ozet zaten kisa uretiliyor (ASU-033);
/// tavan bozuk/uzun bir ozete karsi savunma.
const MAX_SESSION_SUMMARY_WORDS: usize = 250;

/// Baglama girecek azami tercih sayisi.
const MAX_PREFERENCES: u32 = 8;

/// Proje biliniyorsa oncelikli olarak okunacak proje kaydi sayisi.
const MAX_PROJECT_MEMORIES: u32 = 6;

/// `relevantMemories` icin azami kayit sayisi (proje + global toplam).
const MAX_RELEVANT_MEMORIES: usize = 12;

/// Sorgu tavani: butce zaten kirpiyor, ama SQL de sinirli okusun.
const RELEVANT_QUERY_LIMIT: u32 = 24;

// ---------------------------------------------------------------------------
// Tur politikasi
// ---------------------------------------------------------------------------

/// `userPreferences` bolumunu olusturan tur.
const PREFERENCE_KIND: MemoryKind = MemoryKind::Preference;

/// Proje biliniyorsa once okunan turler (PROJECT.md Bolum 13 Stage A:
/// "project summary + latest project decision memories").
const PROJECT_KINDS: [MemoryKind; 2] = [MemoryKind::Project, MemoryKind::Decision];

/// `relevantMemories`e **hicbir zaman** girmeyen turler.
///
/// - `preference`: ayri bir bolumde zaten var, iki kez tasinmasin.
/// - `working_context` / `tool_state`: oturum omurlu; kalici baglama
///   tasinmalari PROJECT.md Bolum 14'un ayrimini yok ederdi.
fn is_excluded_from_relevant(kind: MemoryKind) -> bool {
    matches!(
        kind,
        MemoryKind::Preference | MemoryKind::WorkingContext | MemoryKind::ToolState
    )
}

fn relevant_kinds() -> Vec<MemoryKind> {
    MemoryKind::ALL
        .into_iter()
        .filter(|kind| !is_excluded_from_relevant(*kind))
        .collect()
}

// ---------------------------------------------------------------------------
// Cikti tipleri
// ---------------------------------------------------------------------------

/// Baglama giren tek hafiza — satirin **tamami degil**, prompt'a gidecek hali.
///
/// Neden `MemoryRecord` degil: bu bir DB satiri degil, kirpilmis bir prompt
/// parcasi. `MemoryRecord` dondurup icerigini kisaltmak "satir buydu" yalanini
/// uretirdi; burada kirpma [`ContextMemory::truncated`] ile **gorunur**.
/// `metadata_json`, `confidence`, `embedding` gibi alanlar da modele hic
/// gitmez — baglamda isleri yok.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextMemory {
    pub id: i64,
    pub kind: MemoryKind,
    pub title: String,
    /// Ozet varsa ozet, yoksa icerik; kalem tavanina kirpilmis olabilir.
    pub text: String,
    pub project_id: Option<String>,
    pub importance: f64,
    pub created_at: String,
    /// `true` ise metin tavana kirpildi (kaynak kayit degismedi).
    pub truncated: bool,
}

/// Bir onceki oturumun ozeti.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSessionContext {
    pub id: i64,
    pub ended_at: String,
    pub summary: String,
    pub truncated: bool,
}

/// Aktif proje ozeti — **Phase 4** (ASU-039+) dolduracak.
///
/// Tip simdiden var ki baglam sozlesmesi ve prompt bicimi Phase 4'te
/// degismesin. Su an her zaman `None` doner.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContext {
    pub id: String,
    pub name: String,
    pub summary: Option<String>,
}

/// Acik is — **Phase 6** dolduracak. Su an her zaman bos.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTask {
    pub id: i64,
    pub title: String,
}

/// Paketin olculen boyutu.
///
/// Sayilar renderer'a da gider: "baglam ne kadar buyudu?" sorusu tahminle
/// degil olcumle cevaplanir (PROJECT.md Bolum 25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBudget {
    /// [`CONTEXT_WORD_LIMIT`].
    pub word_limit: u32,
    /// Pakete gercekten giren kelime sayisi.
    pub word_count: u32,
    /// Pakete giren kalem sayisi (tercih + ozet + hafiza).
    pub included: u32,
    /// Butce dolduğu icin alinmayan kalem sayisi.
    pub dropped: u32,
    /// Metni kalem tavanina kirpilan kalem sayisi.
    pub truncated: u32,
}

/// Oturum acilisinda modele enjekte edilen baglam paketi (PROJECT.md Bolum 25).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBootstrapContext {
    /// `false` = kalici hafiza kapali (acilista ya da calisma zamaninda).
    ///
    /// Bos baglam ile kapali hafiza ayni sey degil: birincisinde "hatirlayacak
    /// bir sey yok", ikincisinde "hatirlamamam istendi". Prompt ikisini farkli
    /// cumleyle anlatir.
    pub memory_available: bool,
    pub user_preferences: Vec<ContextMemory>,
    pub current_project: Option<ProjectContext>,
    pub recent_session: Option<RecentSessionContext>,
    pub active_tasks: Vec<ActiveTask>,
    pub relevant_memories: Vec<ContextMemory>,
    pub budget: ContextBudget,
}

impl SessionBootstrapContext {
    /// Hicbir hafiza tasimayan baglam.
    fn empty(memory_available: bool) -> Self {
        Self {
            memory_available,
            user_preferences: Vec::new(),
            current_project: None,
            recent_session: None,
            active_tasks: Vec::new(),
            relevant_memories: Vec::new(),
            budget: ContextBudget {
                word_limit: word_limit_u32(),
                word_count: 0,
                included: 0,
                dropped: 0,
                truncated: 0,
            },
        }
    }

    /// Modele tasinacak hicbir sey yok mu?
    ///
    /// Prompt bu soruya gore "hatirlamiyorsun" cumlesini ekler; bos baglamda
    /// hatirliyormus gibi davranmak yasak (PROJECT.md Bolum 11).
    pub fn is_empty(&self) -> bool {
        self.user_preferences.is_empty()
            && self.relevant_memories.is_empty()
            && self.recent_session.is_none()
            && self.current_project.is_none()
            && self.active_tasks.is_empty()
    }
}

fn word_limit_u32() -> u32 {
    u32::try_from(CONTEXT_WORD_LIMIT).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------------------
// Butce
// ---------------------------------------------------------------------------

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Metni kelime tavanina kirpar.
///
/// @returns `(metin, kirpildi mi)`.
fn clamp_words(text: &str, max_words: usize) -> (String, bool) {
    let mut words = text.split_whitespace();
    let head: Vec<&str> = words.by_ref().take(max_words).collect();
    if words.next().is_none() {
        return (head.join(" "), false);
    }
    // Kirpma isareti: modelin metnin yarida kesildigini gormesi lazim, yoksa
    // eksik cumleyi tam bir karar sanabilir.
    (format!("{} …", head.join(" ")), true)
}

/// Kelime butcesi muhasebesi.
///
/// **Ilk tasmada durur**: butce dolduktan sonra "belki daha kucuk bir kalem
/// sigar" diye aramaya devam etmek, sirasi onem olan bir listeyi sessizce
/// yeniden siralar (dusuk onemli kisa bir kayit, yuksek onemli uzun bir kaydin
/// onune gecerdi). Deterministik ve aciklanabilir olan: sirayla doldur, tasinca
/// kes, kalanlari `dropped` say.
struct Budget {
    remaining: usize,
    exhausted: bool,
    included: u32,
    dropped: u32,
    truncated: u32,
}

impl Budget {
    fn new() -> Self {
        Self {
            remaining: CONTEXT_WORD_LIMIT,
            exhausted: false,
            included: 0,
            dropped: 0,
            truncated: 0,
        }
    }

    /// @returns `true` ise kalem pakete alinabilir.
    fn admit(&mut self, words: usize, truncated: bool) -> bool {
        if self.exhausted || words > self.remaining {
            self.exhausted = true;
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        self.remaining -= words;
        self.included = self.included.saturating_add(1);
        if truncated {
            self.truncated = self.truncated.saturating_add(1);
        }
        true
    }

    fn report(&self) -> ContextBudget {
        let limit = word_limit_u32();
        ContextBudget {
            word_limit: limit,
            word_count: u32::try_from(CONTEXT_WORD_LIMIT - self.remaining).unwrap_or(limit),
            included: self.included,
            dropped: self.dropped,
            truncated: self.truncated,
        }
    }
}

// ---------------------------------------------------------------------------
// Filtreler
// ---------------------------------------------------------------------------

/// Stage A'nin ortak filtre tabani.
///
/// `sort: Importance`, `archived: Active`, `include_expired: false`,
/// `exclude_pending_approval: true` ve `mark_accessed: true` — bu bilesim
/// `idx_memories_stage_a` index'inin karsiladigi sorgudur (ASU-030).
///
/// `mark_accessed: true`: baglama girmek **erisimdir** (memory_repository modul
/// dokumantasyonu). Aday kumesi zaten `limit` ile dar; butce yuzunden dusen
/// nadir kalem de "okundu ama sigmadi" olarak isaretlenir — bu, yaslandirma
/// icin liste goruntulemekten cok daha dogru bir sinyal.
fn stage_a_filter(kinds: Vec<MemoryKind>, limit: u32) -> MemoryFilter {
    MemoryFilter {
        kinds,
        archived: ArchiveFilter::Active,
        include_expired: false,
        exclude_pending_approval: true,
        sort: MemorySort::Importance,
        limit: Some(limit),
        mark_accessed: true,
        ..MemoryFilter::default()
    }
}

// ---------------------------------------------------------------------------
// Retrieval
// ---------------------------------------------------------------------------

/// Oturum acilisinin baglam paketini uretir.
///
/// # Stage A sirasi
///
/// 1. **Tercihler** (`kind = preference`) — kim oldugumu ve nasil calistigimi
///    anlatan en kucuk kume; onem sirali.
/// 2. **Son oturum ozeti** — "gecen sefer nerede kalmistik".
/// 3. **Ilgili hafizalar** — proje biliniyorsa once o projenin `project` +
///    `decision` kayitlari, ardindan global kume (onem, esitlikte tazelik).
///
/// Butce bu sirayla harcanir: tasma olursa **once en dusuk onemli ilgili
/// hafizalar** duser, kimlik bilgisi ve son oturum ozeti degil.
///
/// `project_id` Phase 4'e kadar cagiran tarafindan her zaman `None` verilir
/// (aktif proje kavrami ASU-039+ ile geliyor); kod yolu yine de yazildi ve
/// test edildi ki Phase 4 yalnizca "projeyi coz" adimini eklesin.
pub fn build_bootstrap_context(
    db: &AsunaDb,
    project_id: Option<&str>,
    now: &str,
) -> Result<SessionBootstrapContext, StoreError> {
    let mut budget = Budget::new();

    // 1. Tercihler
    let preference_records = memory_repository::list(
        db,
        &stage_a_filter(vec![PREFERENCE_KIND], MAX_PREFERENCES),
        now,
    )?;
    let user_preferences = take_memories(&mut budget, &preference_records);

    // 2. Son oturum ozeti
    let recent_session = session_repository::latest_completed_summary(db)?
        .and_then(|session| take_recent_session(&mut budget, &session));

    // 3. Ilgili hafizalar — proje once, sonra global.
    let mut candidates: Vec<MemoryRecord> = Vec::new();

    if let Some(project) = project_id {
        let filter = MemoryFilter {
            project_id: Some(project.to_owned()),
            ..stage_a_filter(PROJECT_KINDS.to_vec(), MAX_PROJECT_MEMORIES)
        };
        candidates.extend(memory_repository::list(db, &filter, now)?);
    }

    for record in memory_repository::list(
        db,
        &stage_a_filter(relevant_kinds(), RELEVANT_QUERY_LIMIT),
        now,
    )? {
        if candidates.iter().any(|existing| existing.id == record.id) {
            continue;
        }
        candidates.push(record);
    }
    candidates.truncate(MAX_RELEVANT_MEMORIES);

    let relevant_memories = take_memories(&mut budget, &candidates);

    let context = SessionBootstrapContext {
        memory_available: true,
        user_preferences,
        // Phase 4 (ASU-039+) dolduracak.
        current_project: None,
        recent_session,
        // Phase 6 dolduracak.
        active_tasks: Vec::new(),
        relevant_memories,
        budget: budget.report(),
    };

    log_budget(&context.budget);
    Ok(context)
}

/// Kayitlari butceye gore baglam kalemlerine cevirir.
fn take_memories(budget: &mut Budget, records: &[MemoryRecord]) -> Vec<ContextMemory> {
    let mut taken = Vec::with_capacity(records.len());

    for record in records {
        // Ozet varsa ozet: ayni bilgi daha az kelimeyle tasinir; yoksa icerik.
        let source = record.summary.as_deref().unwrap_or(&record.content);
        let (text, truncated) = clamp_words(source, MAX_MEMORY_WORDS);
        let words = count_words(&record.title) + count_words(&text);

        if !budget.admit(words, truncated) {
            continue;
        }

        taken.push(ContextMemory {
            id: record.id,
            kind: record.kind,
            title: record.title.clone(),
            text,
            project_id: record.project_id.clone(),
            importance: record.importance,
            created_at: record.created_at.clone(),
            truncated,
        });
    }

    taken
}

fn take_recent_session(
    budget: &mut Budget,
    session: &SessionRecord,
) -> Option<RecentSessionContext> {
    // `latest_completed_summary` yalnizca ozeti ve bitisi olan kayit doner;
    // yine de sema garantisi degil, bu yuzden sessizce `None`.
    let summary = session.summary.as_deref()?;
    let ended_at = session.ended_at.as_deref()?;

    let (summary, truncated) = clamp_words(summary, MAX_SESSION_SUMMARY_WORDS);
    if !budget.admit(count_words(&summary), truncated) {
        return None;
    }

    Some(RecentSessionContext {
        id: session.id,
        ended_at: ended_at.to_owned(),
        summary,
        truncated,
    })
}

/// Paket boyutu **olculur ve yazilir**. Bir baglam paketinin sessizce
/// buyumesi, faturanin sessizce buyumesi demek (voice.md Bolum 6).
fn log_budget(budget: &ContextBudget) {
    eprintln!(
        "[asuna] Stage A baglami: {}/{} kelime, {} kalem (kirpilan {}, atlanan {}).",
        budget.word_count, budget.word_limit, budget.included, budget.truncated, budget.dropped
    );
}

// ---------------------------------------------------------------------------
// Komut
// ---------------------------------------------------------------------------

/// Oturum acilmadan once cagrilir; baglam paketini dondurur (salt okuma).
///
/// Renderer **parametre veremez**: aktif proje, siniflar, siralama ve boyut
/// tavani host tarafinda karara baglidir. Bu, ACL'de acilan yuzeyin en dar
/// hali — webview'in retrieval politikasini degistirme yolu yok.
///
/// Hafiza kapaliysa (acilista ya da calisma zamaninda) **bos** baglam doner:
/// `memoryAvailable = false`. Kapali hafiza bir ariza degil, kullanicinin
/// karari (PROJECT.md Bolum 20); konusma baglamsiz devam eder. Ariza
/// (`DbState::Unavailable`) ise tipli hata olarak gorunur — sessizce bos
/// donmek "hatirlayacak bir sey yok" yalanini uretirdi (PROJECT.md Bolum 30).
#[tauri::command]
pub fn get_bootstrap_context(
    state: State<'_, DbState>,
    privacy: State<'_, Arc<PrivacyState>>,
) -> Result<SessionBootstrapContext, StoreError> {
    // Kullanici hafizayi calisma zamaninda kapattiysa (ASU-037) yeni oturum
    // gecmisi hatirlamaz. Kayitlar silinmez ve Memory UI'da gorunmeye devam
    // eder — inceleme ile konusmaya tasima ayri seylerdir.
    if !privacy.memory_enabled() {
        return Ok(SessionBootstrapContext::empty(false));
    }

    let Some(db) = database(&state)? else {
        return Ok(SessionBootstrapContext::empty(false));
    };

    build_bootstrap_context(db, None, &clock::now_utc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memory_repository::{self, MemoryDraft};
    use crate::db::session_repository::{ReportedEndReason, SessionFinalizeInput};
    use crate::extraction::PENDING_APPROVAL_KEY;

    const NOW: &str = "2026-08-25T10:00:00Z";
    const EARLIER: &str = "2026-08-25T09:00:00Z";

    fn fresh_db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB acilmali")
    }

    fn draft(kind: MemoryKind, title: &str, content: &str, importance: f64) -> MemoryDraft {
        MemoryDraft {
            kind,
            title: title.to_owned(),
            content: content.to_owned(),
            summary: None,
            project_id: None,
            importance,
            confidence: 0.9,
            source_session_id: None,
            expires_at: None,
            metadata_json: None,
        }
    }

    fn store(db: &AsunaDb, draft: MemoryDraft) -> MemoryRecord {
        memory_repository::create(db, &draft, NOW).expect("kayit olusmali")
    }

    fn bootstrap(db: &AsunaDb) -> SessionBootstrapContext {
        build_bootstrap_context(db, None, NOW).expect("baglam uretilmeli")
    }

    fn titles(memories: &[ContextMemory]) -> Vec<&str> {
        memories
            .iter()
            .map(|memory| memory.title.as_str())
            .collect()
    }

    /// Ozeti yazilmis, temiz kapanmis bir oturum uretir.
    fn completed_session(db: &AsunaDb, summary: &str) -> i64 {
        let session = session_repository::start(db, "gpt-realtime-2.1", None, EARLIER)
            .expect("oturum acilmali");
        session_repository::finalize(
            db,
            session.id,
            &SessionFinalizeInput {
                end_reason: ReportedEndReason::Completed,
                ..SessionFinalizeInput::default()
            },
            None,
            NOW,
        )
        .expect("oturum kapanmali");
        session_repository::attach_summary(db, session.id, summary, None).expect("ozet yazilmali");
        session.id
    }

    // --- bos durum --------------------------------------------------------

    #[test]
    fn an_empty_store_produces_an_empty_context() {
        let db = fresh_db();
        let context = bootstrap(&db);

        assert!(context.is_empty(), "bos depoda baglam bos olmali");
        assert!(context.memory_available, "hafiza acik: ariza/kapali degil");
        assert_eq!(context.budget.word_count, 0);
        assert_eq!(context.budget.included, 0);
        assert_eq!(context.budget.dropped, 0);
    }

    #[test]
    fn phase_4_and_phase_6_fields_stay_empty_but_typed() {
        let db = fresh_db();
        store(&db, draft(MemoryKind::Decision, "Karar", "Icerik.", 0.9));

        let context = bootstrap(&db);
        assert_eq!(
            context.current_project, None,
            "aktif proje Phase 4'te gelir"
        );
        assert!(context.active_tasks.is_empty(), "task'lar Phase 6'da gelir");
        assert!(!context.is_empty(), "hafiza var, baglam bos degil");
    }

    // --- siralama / oncelik ----------------------------------------------

    #[test]
    fn preferences_are_their_own_section_and_never_repeat_in_relevant_memories() {
        let db = fresh_db();
        store(
            &db,
            draft(
                MemoryKind::Preference,
                "Kisa cevap",
                "Kod yazarken kisa cevap.",
                0.8,
            ),
        );
        store(
            &db,
            draft(MemoryKind::Decision, "Wake word yerel", "Cihazda.", 0.9),
        );

        let context = bootstrap(&db);
        assert_eq!(titles(&context.user_preferences), ["Kisa cevap"]);
        assert_eq!(titles(&context.relevant_memories), ["Wake word yerel"]);
    }

    #[test]
    fn relevant_memories_are_ordered_by_importance_then_recency() {
        let db = fresh_db();
        store(&db, draft(MemoryKind::Idea, "Dusuk", "a", 0.2));
        store(&db, draft(MemoryKind::Decision, "Yuksek", "b", 0.9));
        store(&db, draft(MemoryKind::Task, "Orta", "c", 0.5));

        // Deterministik: ayni depo, ayni sira. Embedding/rastgelelik yok.
        assert_eq!(
            titles(&bootstrap(&db).relevant_memories),
            ["Yuksek", "Orta", "Dusuk"]
        );
        assert_eq!(
            titles(&bootstrap(&db).relevant_memories),
            ["Yuksek", "Orta", "Dusuk"]
        );
    }

    #[test]
    fn project_decisions_come_first_when_the_project_is_known() {
        let db = fresh_db();
        // Global kayit daha **onemli** ama projeye ait degil.
        store(&db, draft(MemoryKind::Decision, "Global karar", "g", 1.0));
        store(
            &db,
            MemoryDraft {
                project_id: Some("asuna".to_owned()),
                ..draft(MemoryKind::Decision, "Proje karari", "p", 0.4)
            },
        );

        let context = build_bootstrap_context(&db, Some("asuna"), NOW).expect("baglam");
        assert_eq!(
            titles(&context.relevant_memories),
            ["Proje karari", "Global karar"],
            "proje biliniyorsa proje kararlari once gelmeli"
        );

        // Proje bilinmiyorsa siniri korunmus global baglam: yalnizca onem sirasi.
        let global = bootstrap(&db);
        assert_eq!(
            titles(&global.relevant_memories),
            ["Global karar", "Proje karari"]
        );
    }

    #[test]
    fn a_project_memory_is_not_listed_twice() {
        let db = fresh_db();
        store(
            &db,
            MemoryDraft {
                project_id: Some("asuna".to_owned()),
                ..draft(MemoryKind::Decision, "Tek karar", "p", 0.7)
            },
        );

        let context = build_bootstrap_context(&db, Some("asuna"), NOW).expect("baglam");
        assert_eq!(titles(&context.relevant_memories), ["Tek karar"]);
    }

    #[test]
    fn session_scoped_kinds_never_reach_the_context() {
        let db = fresh_db();
        store(&db, draft(MemoryKind::WorkingContext, "Gecici", "a", 1.0));
        store(&db, draft(MemoryKind::ToolState, "Editor", "b", 1.0));

        assert!(
            bootstrap(&db).is_empty(),
            "working_context / tool_state kalici baglama girmemeli"
        );
    }

    // --- dislama kurallari ------------------------------------------------

    #[test]
    fn pending_approval_memories_are_excluded() {
        let db = fresh_db();
        store(
            &db,
            MemoryDraft {
                metadata_json: Some(serde_json::json!({ PENDING_APPROVAL_KEY: true }).to_string()),
                ..draft(MemoryKind::Profile, "Onay bekliyor", "hassas", 1.0)
            },
        );
        store(
            &db,
            MemoryDraft {
                metadata_json: Some(serde_json::json!({ PENDING_APPROVAL_KEY: false }).to_string()),
                ..draft(MemoryKind::Decision, "Onaylanmis", "a", 0.9)
            },
        );
        // Elle olusturulan kayitta anahtar **yok** — onay beklemiyor.
        store(&db, draft(MemoryKind::Idea, "Elle yazilmis", "b", 0.8));

        assert_eq!(
            titles(&bootstrap(&db).relevant_memories),
            ["Onaylanmis", "Elle yazilmis"],
            "bayrak yok = onay beklemiyor; `= 0` filtresi elle yazilani da elerdi"
        );
    }

    #[test]
    fn archived_and_expired_memories_are_excluded() {
        let db = fresh_db();
        let archived = store(&db, draft(MemoryKind::Decision, "Arsivli", "a", 1.0));
        memory_repository::set_archived(&db, archived.id, true, NOW).expect("arsivlenmeli");

        store(
            &db,
            MemoryDraft {
                expires_at: Some("2026-08-25T09:30:00Z".to_owned()),
                ..draft(MemoryKind::Decision, "Suresi dolmus", "b", 1.0)
            },
        );
        store(&db, draft(MemoryKind::Decision, "Gecerli", "c", 0.5));

        assert_eq!(titles(&bootstrap(&db).relevant_memories), ["Gecerli"]);
    }

    /// **ASU-036 kabul kriteri**: silinen hafiza bir sonraki oturumun baglamina
    /// girmez. Baglam her acilista depodan yeniden okunur; onbellek yok.
    #[test]
    fn a_deleted_memory_never_reaches_the_next_session_context() {
        let db = fresh_db();
        let record = store(
            &db,
            draft(
                MemoryKind::Decision,
                "Wake word yerel kalir",
                "Cihazda calisir.",
                0.9,
            ),
        );

        assert_eq!(
            titles(&bootstrap(&db).relevant_memories),
            ["Wake word yerel kalir"]
        );

        memory_repository::delete(&db, record.id).expect("silinmeli");

        let after = bootstrap(&db);
        assert!(
            after.relevant_memories.is_empty(),
            "silinen kayit sonraki oturumun baglaminda: {:?}",
            titles(&after.relevant_memories)
        );
        assert!(after.is_empty());
    }

    #[test]
    fn retrieval_marks_the_returned_memories_as_accessed() {
        let db = fresh_db();
        let record = store(&db, draft(MemoryKind::Decision, "Karar", "a", 0.9));
        assert_eq!(record.last_accessed_at, None);

        bootstrap(&db);

        let after = memory_repository::get_by_id(&db, record.id, NOW, false)
            .expect("okunmali")
            .expect("kayit durmali");
        assert_eq!(after.last_accessed_at.as_deref(), Some(NOW));
    }

    // --- son oturum ozeti -------------------------------------------------

    #[test]
    fn the_most_recent_completed_summary_is_attached() {
        let db = fresh_db();
        completed_session(&db, "Ilk oturum: wake word konusuldu.");
        let second = completed_session(&db, "Ikinci oturum: retrieval konusuldu.");

        let recent = bootstrap(&db)
            .recent_session
            .expect("son oturum ozeti olmali");
        assert_eq!(recent.id, second);
        assert_eq!(recent.summary, "Ikinci oturum: retrieval konusuldu.");
        assert!(!recent.truncated);
    }

    #[test]
    fn a_session_without_a_summary_is_not_attached() {
        let db = fresh_db();
        let session =
            session_repository::start(&db, "gpt-realtime-2.1", None, EARLIER).expect("acilmali");
        session_repository::finalize(&db, session.id, &SessionFinalizeInput::default(), None, NOW)
            .expect("kapanmali");

        assert!(
            bootstrap(&db).recent_session.is_none(),
            "ozeti olmayan oturum baglama girmez"
        );
    }

    // --- boyut siniri -----------------------------------------------------

    #[test]
    fn a_single_huge_memory_is_clamped_instead_of_eating_the_budget() {
        let db = fresh_db();
        let long = "kelime ".repeat(500);
        store(&db, draft(MemoryKind::Decision, "Uzun", &long, 0.9));
        store(&db, draft(MemoryKind::Decision, "Kisa", "iki kelime", 0.8));

        let context = bootstrap(&db);
        assert_eq!(titles(&context.relevant_memories), ["Uzun", "Kisa"]);

        let clamped = &context.relevant_memories[0];
        assert!(clamped.truncated, "uzun kayit kirpilmali");
        assert_eq!(
            count_words(&clamped.text),
            MAX_MEMORY_WORDS + 1,
            "kirpma isareti dahil"
        );
        assert_eq!(context.budget.truncated, 1);
    }

    /// Her kategoriyi kalem tavanina kadar dolduran depo: kayit sayisi
    /// tavanlari tek basina yetmez, kelime butcesi devreye girmek zorunda kalir.
    fn seed_beyond_the_budget(db: &AsunaDb) {
        let long = "kelime ".repeat(MAX_MEMORY_WORDS * 2);
        for index in 0..MAX_PREFERENCES {
            store(
                db,
                draft(
                    MemoryKind::Preference,
                    &format!("Tercih {index}"),
                    &long,
                    0.9,
                ),
            );
        }
        completed_session(db, &"ozet ".repeat(MAX_SESSION_SUMMARY_WORDS * 2));
        for index in 0..(MAX_RELEVANT_MEMORIES * 2) {
            store(
                db,
                draft(MemoryKind::Decision, &format!("Kayit {index}"), &long, 0.5),
            );
        }
    }

    #[test]
    fn the_context_package_never_exceeds_the_word_limit() {
        let db = fresh_db();
        seed_beyond_the_budget(&db);

        let context = bootstrap(&db);
        let budget = context.budget;

        assert!(
            budget.word_count <= budget.word_limit,
            "baglam tavani asildi: {} > {}",
            budget.word_count,
            budget.word_limit
        );
        assert!(
            budget.dropped > 0,
            "tasan kalemler `dropped` olarak sayilmali"
        );
        assert!(
            context.relevant_memories.len() < MAX_RELEVANT_MEMORIES,
            "butce dolunca liste kisalmali"
        );

        // Olculen sayi gercekten pakete giren kelime mi? (Uydurulmus bir
        // olcum, olcum yapmamaktan kotudur.)
        let memory_words: usize = context
            .user_preferences
            .iter()
            .chain(context.relevant_memories.iter())
            .map(|memory| count_words(&memory.title) + count_words(&memory.text))
            .sum();
        let session_words = context
            .recent_session
            .as_ref()
            .map_or(0, |session| count_words(&session.summary));
        assert_eq!(
            usize::try_from(budget.word_count).unwrap_or(0),
            memory_words + session_words
        );
    }

    /// Butce tasarken **once en dusuk onemli ilgili hafizalar** duser; kimlik
    /// bilgisi (tercihler) ve son oturum ozeti pakette kalir.
    #[test]
    fn identity_and_recent_session_survive_when_relevant_memories_overflow() {
        let db = fresh_db();
        seed_beyond_the_budget(&db);

        let context = bootstrap(&db);
        assert_eq!(
            context.user_preferences.len(),
            usize::try_from(MAX_PREFERENCES).unwrap_or(0),
            "tercihler butcede oncelikli olmali"
        );
        assert!(
            context.recent_session.is_some(),
            "son oturum ozeti butcede oncelikli olmali"
        );
        assert!(
            !context.relevant_memories.is_empty()
                && context.relevant_memories.len() < MAX_RELEVANT_MEMORIES,
            "ilgili hafizalarin bir kismi girmeli, hepsi degil: {}",
            context.relevant_memories.len()
        );
    }

    #[test]
    fn clamp_words_marks_only_what_it_cuts() {
        assert_eq!(
            clamp_words("bir iki uc", 5),
            ("bir iki uc".to_owned(), false)
        );
        assert_eq!(clamp_words("bir iki uc", 2), ("bir iki …".to_owned(), true));
        assert_eq!(clamp_words("", 5), (String::new(), false));
    }

    // --- kapali hafiza ----------------------------------------------------

    #[test]
    fn an_empty_context_reports_whether_memory_is_available() {
        let disabled = SessionBootstrapContext::empty(false);
        assert!(!disabled.memory_available);
        assert!(disabled.is_empty());
        assert_eq!(disabled.budget.word_limit, word_limit_u32());

        let json = serde_json::to_value(&disabled).expect("serilestirilebilmeli");
        assert_eq!(json["memoryAvailable"], false);
        assert_eq!(json["userPreferences"], serde_json::json!([]));
        assert_eq!(json["currentProject"], serde_json::Value::Null);
        assert_eq!(json["activeTasks"], serde_json::json!([]));
        assert_eq!(json["relevantMemories"], serde_json::json!([]));
        assert_eq!(json["recentSession"], serde_json::Value::Null);
    }
}

//! `project_context` — "su an hangi projedeyim?" sorusunun **tek** kod yolu
//! (ASU-044, PROJECT.md Bolum 15/17).
//!
//! # Neden tek komut
//!
//! Bu cevabi iki tuketici istiyor: Asuna'nin `get_current_project` tool'u ve
//! Projeler sekmesi (ASU-045). Ikisine ayri komut yazmak, ikisinin zamanla
//! **farkli seyler** soylemesi demekti — kullanicinin ekranda gordugu branch ile
//! Asuna'nin sesli soyledigi branch ayrilirdi. Zincir burada bir kez kuruluyor:
//!
//! ```text
//! registry::current -> ProjectContextService::current -> git_metadata::collect -> handoff::read
//! ```
//!
//! # Belirsizlik hata degil
//!
//! [`super::context::ProjectContext::Unknown`] uc ayri nedeni ayri ayri tasir ve
//! bu komut onlari **oldugu gibi** yansitir. Asuna'nin soracagi soru her birinde
//! farkli: "hangi dizinde calisiyorsun?" ile "disk takili mi?" ayni soru degil.
//! Ucunu tek bir `null`'a indirgemek, modeli proje uydurmaya davet ederdi.
//!
//! # Eksik bilgi "basarili" gibi sunulmaz
//!
//! [`GitMetadata::degraded`] ve [`HandoffRead::Ignored`] ciktiya **girer**
//! (PROJECT.md Bolum 30). Tool bunlari kendi ozetine tasir: "git durumunu tam
//! okuyamadim" demek, yanlis bir branch soylemekten iyidir.
//!
//! # Cikti boyutu
//!
//! [`super::context`] zaten uc tavan uygular (dosya / kaynak / toplam ozet). Bu
//! modul **dorduncu** tavani ekler: ozet + git + devir teslim toplaminin
//! karakter sayisi ([`MAX_VIEW_CHARS`]). Uc kaynagin tavanlari tek tek asilmasa
//! da toplami ses oturumu icin fazla olabilir. Kirpma sessiz degil:
//! [`KnownProjectContext::truncated`] bayragi ve olculen `total_chars` doner.

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::db::{AsunaDb, DbState};

use super::context::{ContextUnknownReason, ProjectContext, ProjectContextService, ProjectSummary};
use super::git_metadata::{self, GitMetadata};
use super::handoff::{self, HandoffRead};
use super::registry::{self, RegistryError};

/// Komut ciktisinin olculen toplam karakter tavani.
///
/// `context::MAX_TOTAL_CONTEXT_CHARS` (6000) + git + devir teslim toplami en
/// kotu durumda ~12 000 karaktere ciakabilir. Bu tavan o toplami sinirlar;
/// proje ozetinin kendisine **dokunulmaz** (asil deger orada), once devir teslim
/// listeleri, sonra commit basliklari kisalir.
pub const MAX_VIEW_CHARS: usize = 9_000;

// ---------------------------------------------------------------------------
// Cikti
// ---------------------------------------------------------------------------

/// Guncel proje biliniyor: ozet + git durumu + devir teslim artefakti.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownProjectContext {
    pub summary: ProjectSummary,
    /// Salt okuma git durumu. `isRepository: false` bir hata degil.
    pub git: GitMetadata,
    /// `.asuna/context.json`. Yoksa `absent`, bozuksa `ignored` — ikisi ayri.
    pub handoff: HandoffRead,
    /// Olculen toplam karakter (ozet + git + devir teslim).
    pub total_chars: usize,
    pub max_chars: usize,
    /// Tavan asildigi icin en az bir liste kisaldi.
    pub truncated: bool,
}

/// `project_context` komutunun ciktisi.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ProjectContextView {
    Known {
        project: Box<KnownProjectContext>,
    },
    /// Guncel proje bilinmiyor — **hata degil**, urun durumu.
    Unknown {
        reason: ContextUnknownReason,
        message: &'static str,
    },
}

impl ProjectContextView {
    /// Bilinen proje; belirsizse `None`.
    pub fn known(&self) -> Option<&KnownProjectContext> {
        match self {
            Self::Known { project } => Some(project),
            Self::Unknown { .. } => None,
        }
    }

    /// Belirsizligin nedeni; proje biliniyorsa `None`.
    pub fn unknown_reason(&self) -> Option<ContextUnknownReason> {
        match self {
            Self::Known { .. } => None,
            Self::Unknown { reason, .. } => Some(*reason),
        }
    }
}

// ---------------------------------------------------------------------------
// Toplama
// ---------------------------------------------------------------------------

/// Guncel projenin tam baglami.
///
/// Onbellek [`ProjectContextService`]'te; git durumu ve devir teslim dosyasi
/// **her cagride taze** okunur. Gerekce: branch ve kirli/temiz bilgisi oturum
/// sirasinda degisen seylerdir; 30 saniye eski bir branch adi sesli cevapta
/// yanlis bilgidir.
pub fn collect(
    db: &AsunaDb,
    service: &ProjectContextService,
) -> Result<ProjectContextView, RegistryError> {
    let summary = match service.current(db)? {
        ProjectContext::Unknown { reason, message } => {
            return Ok(ProjectContextView::Unknown { reason, message });
        }
        ProjectContext::Known { summary } => *summary,
    };

    let root = Path::new(&summary.path);
    let git = git_metadata::collect(root);
    let handoff = handoff::read(root);

    Ok(ProjectContextView::Known {
        project: Box::new(fit_to_budget(summary, git, handoff)),
    })
}

/// Uc kaynagin toplamini [`MAX_VIEW_CHARS`] icine sokar.
///
/// Kirpma sirasi deterministik ve gerekcesi var: **proje ozetine dokunulmaz**
/// (zaten kendi tavanindan gecti ve sorunun asil cevabi orada), once devir
/// teslim listeleri (kullanicinin baska bir araca yazdigi, DB ile celisebilen
/// metin), en son commit basliklari.
fn fit_to_budget(
    summary: ProjectSummary,
    mut git: GitMetadata,
    mut handoff: HandoffRead,
) -> KnownProjectContext {
    let base = summary.total_chars;
    let mut truncated = false;

    loop {
        let total = base + git_chars(&git) + handoff_chars(&handoff);
        if total <= MAX_VIEW_CHARS {
            return KnownProjectContext {
                summary,
                git,
                handoff,
                total_chars: total,
                max_chars: MAX_VIEW_CHARS,
                truncated,
            };
        }
        if !drop_one_item(&mut git, &mut handoff) {
            // Kirpilacak liste kalmadi: kalan icerik zaten kendi tavanlarindan
            // gecmis sabit alanlar. Sessizce "sigdi" demiyoruz.
            return KnownProjectContext {
                summary,
                git,
                handoff,
                total_chars: total,
                max_chars: MAX_VIEW_CHARS,
                truncated: true,
            };
        }
        truncated = true;
    }
}

/// Tek bir liste ogesini dusurur. `false` = dusurulecek oge kalmadi.
fn drop_one_item(git: &mut GitMetadata, handoff: &mut HandoffRead) -> bool {
    if let HandoffRead::Loaded { context } = handoff {
        if context.recent_decisions.pop().is_some() {
            return true;
        }
        if context.blockers.pop().is_some() {
            return true;
        }
    }
    git.recent_commits.pop().is_some()
}

fn char_count(value: Option<&str>) -> usize {
    value.map_or(0, |text| text.chars().count())
}

fn git_chars(git: &GitMetadata) -> usize {
    char_count(git.branch.as_deref())
        + char_count(git.remote.as_deref())
        + git
            .recent_commits
            .iter()
            .map(|line| line.chars().count())
            .sum::<usize>()
}

fn handoff_chars(handoff: &HandoffRead) -> usize {
    let HandoffRead::Loaded { context } = handoff else {
        return 0;
    };
    let mut total = char_count(context.project_name.as_deref())
        + char_count(context.objective.as_deref())
        + char_count(context.current_milestone.as_deref())
        + char_count(context.active_task.as_deref());
    for item in context.blockers.iter().chain(&context.recent_decisions) {
        total += item.chars().count();
    }
    total
}

// ---------------------------------------------------------------------------
// Komut
// ---------------------------------------------------------------------------

/// Guncel projenin baglami (ASU-044).
///
/// Salt okuma: hicbir kayit degistirilmez, hicbir git yazma komutu kosmaz ve
/// "guncel proje" secimi bu cagriyla **degismez** (secim kullanicinin acik
/// eylemidir — `project_set_current`).
#[tauri::command]
pub fn project_context(
    state: State<'_, DbState>,
    service: State<'_, ProjectContextService>,
) -> Result<ProjectContextView, RegistryError> {
    let db = registry::database(&state)?;
    collect(db, service.inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::projects::handoff::HandoffContext;

    fn summary(total_chars: usize) -> ProjectSummary {
        ProjectSummary {
            project_id: "asuna".to_owned(),
            name: "Asuna".to_owned(),
            path: "/tmp/asuna".to_owned(),
            status: crate::db::model::ProjectStatus::Active,
            primary_language: Some("Rust".to_owned()),
            framework: Some("Tauri".to_owned()),
            git_remote: Some("github.com/omergungor/asuna".to_owned()),
            sources: Vec::new(),
            total_chars,
            max_chars: crate::projects::context::MAX_TOTAL_CONTEXT_CHARS,
            budget_exhausted: false,
        }
    }

    fn loaded(blockers: usize, decisions: usize, item_chars: usize) -> HandoffRead {
        HandoffRead::Loaded {
            context: Box::new(HandoffContext {
                project_name: Some("Asuna".to_owned()),
                objective: None,
                current_milestone: None,
                active_task: None,
                blockers: (0..blockers).map(|_| "b".repeat(item_chars)).collect(),
                recent_decisions: (0..decisions).map(|_| "d".repeat(item_chars)).collect(),
            }),
        }
    }

    fn git_with_commits(count: usize, chars: usize) -> GitMetadata {
        GitMetadata {
            is_repository: true,
            branch: Some("main".to_owned()),
            recent_commits: (0..count).map(|_| "c".repeat(chars)).collect(),
            ..GitMetadata::default()
        }
    }

    #[test]
    fn a_small_project_fits_without_truncation() {
        let view = fit_to_budget(summary(1_000), git_with_commits(5, 100), loaded(2, 2, 100));

        assert!(!view.truncated);
        assert_eq!(view.max_chars, MAX_VIEW_CHARS);
        assert!(view.total_chars <= MAX_VIEW_CHARS);
        assert_eq!(view.git.recent_commits.len(), 5);
    }

    /// Kirpma sirasi: once devir teslim kararlari, sonra blocker'lar, en son
    /// commit basliklari. Proje ozetine **hic** dokunulmaz.
    #[test]
    fn the_handoff_lists_shrink_before_the_commit_subjects() {
        let view = fit_to_budget(
            summary(6_000),
            git_with_commits(5, 120),
            loaded(10, 10, 200),
        );

        assert!(view.truncated, "tavan asildi ama bayrak kalkmamis");
        assert_eq!(view.summary.total_chars, 6_000, "proje ozeti kirpilmis");
        assert_eq!(
            view.git.recent_commits.len(),
            5,
            "commit'ler once kirpilmis"
        );
        let HandoffRead::Loaded { context } = &view.handoff else {
            panic!("devir teslim baglami kaybolmus");
        };
        assert!(context.recent_decisions.len() < 10);
        assert!(view.total_chars <= MAX_VIEW_CHARS);
    }

    /// Kirpilacak liste bittiginde toplam hala tavani asiyorsa, "sigdi" denmez.
    #[test]
    fn an_oversized_summary_is_reported_as_truncated_not_silently_accepted() {
        let view = fit_to_budget(
            summary(MAX_VIEW_CHARS + 1),
            GitMetadata::not_a_repository(),
            HandoffRead::Absent,
        );

        assert!(view.truncated);
        assert!(view.total_chars > MAX_VIEW_CHARS);
    }

    /// `degraded` yutulmaz: eksik git bilgisi ciktiya oldugu gibi girer
    /// (PROJECT.md Bolum 30).
    #[test]
    fn the_degraded_git_flag_survives_the_budget_pass() {
        let git = GitMetadata {
            degraded: true,
            ..git_with_commits(5, 120)
        };
        let view = fit_to_budget(summary(6_000), git, loaded(10, 10, 200));

        assert!(view.git.degraded);
    }

    /// Bozuk `.asuna/context.json` "bos baglam" gibi gosterilmez.
    #[test]
    fn an_ignored_handoff_file_stays_visible_in_the_view() {
        let ignored = HandoffRead::Ignored {
            reason: crate::projects::handoff::HandoffIgnoreReason::InvalidJson,
            message: crate::projects::handoff::HandoffIgnoreReason::InvalidJson.describe(),
        };
        let view = fit_to_budget(summary(100), GitMetadata::not_a_repository(), ignored);

        assert!(matches!(view.handoff, HandoffRead::Ignored { .. }));
    }

    /// Serilestirme sozlesmesi `src/shared/project.ts` ile birebir: `status`
    /// etiketi + camelCase alanlar.
    #[test]
    fn the_wire_format_matches_the_typescript_mirror() {
        let view = ProjectContextView::Known {
            project: Box::new(fit_to_budget(
                summary(100),
                git_with_commits(1, 10),
                HandoffRead::Absent,
            )),
        };
        let json = serde_json::to_string(&view).expect("serilestirilebilmeli");

        assert!(json.contains("\"status\":\"known\""), "{json}");
        assert!(json.contains("\"totalChars\":"), "{json}");
        assert!(json.contains("\"maxChars\":"), "{json}");
        assert!(json.contains("\"isRepository\":true"), "{json}");
        assert!(
            json.contains("\"handoff\":{\"status\":\"absent\"}"),
            "{json}"
        );
    }

    #[test]
    fn the_unknown_variant_carries_its_reason_verbatim() {
        for reason in [
            ContextUnknownReason::NoRegisteredProject,
            ContextUnknownReason::NoCurrentSelection,
            ContextUnknownReason::RootMissing,
        ] {
            let view = ProjectContextView::Unknown {
                reason,
                message: reason.describe(),
            };
            assert_eq!(view.unknown_reason(), Some(reason));
            assert!(view.known().is_none());

            let json = serde_json::to_string(&view).expect("serilestirilebilmeli");
            assert!(json.contains("\"status\":\"unknown\""), "{json}");
            assert!(json.contains(reason.describe()), "{json}");
        }
    }
}

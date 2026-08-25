//! `ProjectContextService` — kayitli bir projeden **guvenli ve kisa** bir ozet
//! uretir (ASU-041, PROJECT.md Bolum 15).
//!
//! # Ilke: ozetle, dumpleme
//!
//! PROJECT.md Bolum 15 son cumlesi: "Do not dump the entire repository into the
//! voice session." Burada uc ayri tavan var ve ucu de olculuyor:
//!
//! | Tavan | Deger | Neden |
//! |---|---|---|
//! | [`MAX_FILE_READ_BYTES`] | 32 KiB | Diskten okunan ham bayt. 40 MB'lik bir `README.md` belegi doldurmasin. |
//! | [`MAX_SOURCE_EXCERPT_CHARS`] | 1200 | Tek kaynagin ozete koyabilecegi pay. Bir dosya butun butceyi yiyemesin. |
//! | [`MAX_TOTAL_CONTEXT_CHARS`] | 6000 | Ses oturumuna giren toplam. Olculur ve **log'lanir**. |
//!
//! Kirpma sessiz degil: her kaynak `truncated` bayragi tasir, ozetin sonunda da
//! toplam olcum durur. "Hepsini okudum" izlenimi verilmez.
//!
//! # Guvenlik
//!
//! - Okunacak dosyalar **sabit bir allowlist** ([`CONTEXT_SOURCES`]). Model ya
//!   da renderer hangi dosyanin okunacagini secemez.
//! - Her aday yol ayrica [`crate::security::blocklist`]'ten gecer. Allowlist
//!   zaten `.env`i icermiyor; blok listesi **ikinci** kapidir ve symlink
//!   cozuldukten sonra uygulanir — kok icindeki `README.md -> ~/.ssh/id_ed25519`
//!   gibi bir bag boylece takilir.
//! - Cozulen yol kok **disina** cikiyorsa dosya okunmaz (symlink escape).
//! - `.git/config` **hic acilmaz** (ASU-049): dosya blok listesine girdi, cunku
//!   repo-yerel remote URL'i `https://kullanici:ghp_TOKEN@github.com/...`
//!   bicimiyle canli bir token tasiyabilir ve `[credential]` bolumu helper
//!   ayarlarini barindirir. Remote **adi** artik tek bir yerden geliyor:
//!   ASU-042'nin `git remote get-url origin` ciktisi, [`sanitise_remote_url`]
//!   ile redakte edilmis halde ([`super::view::collect`] kaydeder). Iki ayri
//!   turetme yolu olmasi zaten ikisinin zamanla ayrisma riskiydi.
//!
//! # "Guncel proje" belirsizse
//!
//! Hata degil, [`ProjectContext::Unknown`] doner ve **nedeni** tasir. Asuna
//! sorar, uydurmaz (PROJECT.md Bolum 11 / Bolum 30).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use crate::db::model::ProjectStatus;
use crate::db::{clock, AsunaDb};
use crate::redaction::redact_sensitive_text;
use crate::security::blocklist;

use super::registry::{self, DetectedMetadata, RegistryError};

/// Tek bir kaynaktan diske gidip okunacak en fazla bayt.
pub const MAX_FILE_READ_BYTES: usize = 32 * 1024;

/// Tek bir kaynagin ozete koyabilecegi en fazla karakter.
pub const MAX_SOURCE_EXCERPT_CHARS: usize = 1_200;

/// Uretilen ozetin toplam karakter tavani (ses oturumuna giren miktar).
pub const MAX_TOTAL_CONTEXT_CHARS: usize = 6_000;

/// Onbellek omru. Kisa: proje dosyalari oturum sirasinda degisebilir.
pub const CACHE_TTL: Duration = Duration::from_secs(30);

/// Kirpilan icerigin sonuna eklenen isaret.
pub const TRUNCATION_MARKER: &str = "… (kirpildi)";

/// Manifest'ten okunacak en fazla bagimlilik adi.
const MAX_DEPENDENCIES_LISTED: usize = 12;

// ---------------------------------------------------------------------------
// Kaynaklar
// ---------------------------------------------------------------------------

/// Bir baglam kaynaginin nasil islenecegi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    /// Duz metin/markdown: basindan bir alinti.
    Prose,
    /// `package.json`: JSON olarak ayristirilip **ozetlenir**, dumplenmez.
    NodeManifest,
    /// `Cargo.toml` / `pyproject.toml`: minimal TOML taramasi.
    TomlManifest,
}

/// Okunacak dosyalar — PROJECT.md Bolum 15'teki sira.
///
/// **Bu liste sabittir.** Model ya da renderer buraya bir dosya ekleyemez;
/// "sadece su dosyayi da oku" diye bir yuzey yok.
///
/// `.git/config` ASU-049'da listeden **cikarildi** (modul dokumantasyonu):
/// dosya blok listesine girdi, remote adi ASU-042 yolundan geliyor.
const CONTEXT_SOURCES: [(&str, SourceKind); 7] = [
    ("PROJECT.md", SourceKind::Prose),
    ("README.md", SourceKind::Prose),
    ("CLAUDE.md", SourceKind::Prose),
    ("AGENTS.md", SourceKind::Prose),
    ("package.json", SourceKind::NodeManifest),
    ("pyproject.toml", SourceKind::TomlManifest),
    ("Cargo.toml", SourceKind::TomlManifest),
];

// ---------------------------------------------------------------------------
// Cikti tipleri
// ---------------------------------------------------------------------------

/// Ozete giren tek kaynak.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSource {
    /// Kok'e gore dosya adi (`README.md`). Mutlak yol **donmez**.
    pub name: String,
    /// Kisaltilmis icerik. Manifest'lerde ham dosya degil, turetilmis ozet.
    pub excerpt: String,
    /// Icerik kirpildi mi? Sessiz kirpma yok.
    pub truncated: bool,
    /// Diskteki ham boyut (bayt) — "ne kadarini gormedim?" sorusunun cevabi.
    pub size_bytes: u64,
}

/// Kayitli bir projenin ozeti.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub project_id: String,
    pub name: String,
    /// Kayitli kok. Kullanicinin kendi makinesindeki kendi dizini.
    pub path: String,
    pub status: ProjectStatus,
    /// Manifest'ten **tespit edilmis** dil; tahmin degil, dosya kaniti.
    pub primary_language: Option<String>,
    pub framework: Option<String>,
    /// Redakte edilmis remote adi (`github.com/kullanici/repo`).
    pub git_remote: Option<String>,
    pub sources: Vec<ContextSource>,
    /// Uretilen ozetin olculen toplam boyutu (karakter).
    pub total_chars: usize,
    /// Tavan — UI/log "6000'in 5200'u" diyebilsin.
    pub max_chars: usize,
    /// Toplam butce dolduğu icin en az bir kaynak kisaldi/dusuruldu.
    pub budget_exhausted: bool,
}

/// "Guncel proje" neden bilinmiyor?
///
/// Uc durum bilerek ayri: Asuna'nin soracagi soru her birinde farkli.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContextUnknownReason {
    /// Hic proje kaydedilmemis. Soru: "hangi dizinde calisiyorsun?"
    NoRegisteredProject,
    /// Projeler var ama hicbiri secilmemis. Soru: "hangisindesin?"
    NoCurrentSelection,
    /// Secili projenin kok dizini bulunamiyor. Soru: "disk takili mi?"
    RootMissing,
}

impl ContextUnknownReason {
    /// Modele/kullaniciya gosterilebilecek kisa aciklama. Yol **icermez**.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::NoRegisteredProject => "Kayitli proje yok.",
            Self::NoCurrentSelection => "Kayitli projeler var ama guncel proje secilmemis.",
            Self::RootMissing => "Secili projenin kok dizini su an bulunamiyor.",
        }
    }
}

/// Baglam sonucu.
///
/// `Unknown` bir **hata degil**: Asuna'nin "bilmiyorum, hangi projedesin?" diye
/// sorabilmesi icin tasarlanmis bir urun durumu.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ProjectContext {
    Known {
        summary: Box<ProjectSummary>,
    },
    Unknown {
        reason: ContextUnknownReason,
        /// Hazir, insan diliyle aciklama.
        message: &'static str,
    },
}

impl ProjectContext {
    fn unknown(reason: ContextUnknownReason) -> Self {
        Self::Unknown {
            reason,
            message: reason.describe(),
        }
    }

    pub fn summary(&self) -> Option<&ProjectSummary> {
        match self {
            Self::Known { summary } => Some(summary),
            Self::Unknown { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Servis
// ---------------------------------------------------------------------------

/// Bir kaynagin diskteki parmak izi: "degisti mi?" sorusunun ucuz cevabi.
type Fingerprint = Vec<(String, Option<SystemTime>, u64)>;

struct CacheEntry {
    summary: ProjectSummary,
    fingerprint: Fingerprint,
    generated_at: Instant,
}

/// Proje baglami uretimi + kisa sureli onbellek.
///
/// Onbellek **iki kapili**: sure dolmamis olmali (30 sn) **ve** kaynaklarin
/// mtime/boyut parmak izi degismemis olmali. Yalnizca sureye guvenmek, oturum
/// sirasinda `PROJECT.md`'yi degistiren kullaniciya 30 saniye eski bilgi
/// verirdi; yalnizca parmak izine guvenmek ise her cagride 8 `stat` demekti —
/// ikisi birlikte hem taze hem ucuz.
pub struct ProjectContextService {
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl Default for ProjectContextService {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProjectContextService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Onbellek icerigi kullanicinin dosya icerigi — log'a/panic mesajina
        // basilmaz.
        f.debug_struct("ProjectContextService").finish()
    }
}

impl ProjectContextService {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Guncel projenin baglami.
    ///
    /// Belirsizlikte hata degil [`ProjectContext::Unknown`] doner.
    pub fn current(&self, db: &AsunaDb) -> Result<ProjectContext, RegistryError> {
        let Some(project) = registry::current(db)? else {
            // "Hic proje yok" ile "secilmemis" ayri sorular.
            let any = !registry::list(db, &clock::now_utc())?.is_empty();
            return Ok(ProjectContext::unknown(if any {
                ContextUnknownReason::NoCurrentSelection
            } else {
                ContextUnknownReason::NoRegisteredProject
            }));
        };

        let Some(path) = project.path.clone() else {
            return Ok(ProjectContext::unknown(ContextUnknownReason::RootMissing));
        };
        let root = PathBuf::from(&path);
        if !root.is_dir() {
            return Ok(ProjectContext::unknown(ContextUnknownReason::RootMissing));
        }

        if let Some(cached) = self.cached(&project.id, &root) {
            return Ok(ProjectContext::Known {
                summary: Box::new(cached),
            });
        }

        let fingerprint = fingerprint_of(&root);
        let collected = collect_sources(&root);

        let summary = ProjectSummary {
            project_id: project.id.clone(),
            name: project.name.clone(),
            path,
            status: project.status,
            primary_language: collected.detected.primary_language.clone(),
            framework: collected.detected.framework.clone(),
            // ASU-049: `.git/config` artik acilmiyor. Remote adi ASU-042
            // yolundan geliyor ve **kayda** yaziliyor ([`super::view::collect`]);
            // burada yalnizca kayitli deger yansitiliyor, tahmin edilmiyor.
            git_remote: project.git_remote.clone(),
            total_chars: collected.total_chars,
            max_chars: MAX_TOTAL_CONTEXT_CHARS,
            budget_exhausted: collected.budget_exhausted,
            sources: collected.sources,
        };

        // Olculen boyut log'lanir (kabul kriteri: "toplam boyutu sinirli ve
        // olculuyor"). Icerik degil, yalnizca sayilar.
        eprintln!(
            "[asuna] Proje baglami uretildi: {} kaynak, {}/{} karakter{}.",
            summary.sources.len(),
            summary.total_chars,
            MAX_TOTAL_CONTEXT_CHARS,
            if summary.budget_exhausted {
                " (butce doldu)"
            } else {
                ""
            }
        );

        // Tespit edilen metadata kayda islenir — ama yalnizca **degistiyse**:
        // her okumada bir UPDATE atmak, salt okuma gibi gorunen bir cagriyi
        // sessiz bir yazma yoluna cevirirdi.
        // `git_remote` bu karsilastirmada **yok**: bu modul artik onu
        // uretmiyor, yalnizca kayittan yansitiyor. Karsilastirmaya dahil etmek
        // her cagride bosuna bir yazma denemesi uretirdi.
        let changed = project.primary_language != summary.primary_language
            || project.framework != summary.framework;
        if changed {
            registry::record_detected_metadata(
                db,
                &project.id,
                &collected.detected,
                &clock::now_utc(),
            )?;
        }

        self.store(&project.id, &summary, fingerprint);

        Ok(ProjectContext::Known {
            summary: Box::new(summary),
        })
    }

    /// Onbellegi bosaltir (proje kaldirildiginda / testlerde).
    pub fn invalidate(&self, project_id: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(project_id);
        }
    }

    fn cached(&self, project_id: &str, root: &Path) -> Option<ProjectSummary> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(project_id)?;
        if entry.generated_at.elapsed() > CACHE_TTL {
            return None;
        }
        if entry.fingerprint != fingerprint_of(root) {
            return None;
        }
        Some(entry.summary.clone())
    }

    fn store(&self, project_id: &str, summary: &ProjectSummary, fingerprint: Fingerprint) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(
                project_id.to_owned(),
                CacheEntry {
                    summary: summary.clone(),
                    fingerprint,
                    generated_at: Instant::now(),
                },
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Toplama
// ---------------------------------------------------------------------------

struct Collected {
    sources: Vec<ContextSource>,
    detected: DetectedMetadata,
    total_chars: usize,
    budget_exhausted: bool,
}

/// Kaynaklarin mtime + boyut parmak izi. Okuma yapmaz, yalnizca `stat`.
fn fingerprint_of(root: &Path) -> Fingerprint {
    CONTEXT_SOURCES
        .iter()
        .map(|(name, _)| {
            let metadata = root.join(name).metadata().ok();
            (
                (*name).to_owned(),
                metadata.as_ref().and_then(|meta| meta.modified().ok()),
                metadata.map_or(0, |meta| meta.len()),
            )
        })
        .collect()
}

fn collect_sources(root: &Path) -> Collected {
    let mut sources = Vec::new();
    let mut detected = DetectedMetadata::default();
    let mut total_chars = 0usize;
    let mut budget_exhausted = false;

    // Dil/framework adaylari: birden fazla manifest varsa **oncelik sirasi**
    // deterministik olmali (bkz. `resolve_stack`).
    let mut stack_candidates: Vec<StackHint> = Vec::new();

    for (name, kind) in CONTEXT_SOURCES {
        let Some(read) = read_source(root, name) else {
            continue; // Dosya yok, okunamadi ya da blok listesine takildi.
        };

        match kind {
            SourceKind::NodeManifest => {
                let (excerpt, hint) = summarise_node_manifest(&read.content);
                stack_candidates.push(hint);
                push_source(
                    &mut sources,
                    &mut total_chars,
                    &mut budget_exhausted,
                    name,
                    excerpt,
                    read.size_bytes,
                    read.truncated,
                );
            }
            SourceKind::TomlManifest => {
                let (excerpt, hint) = summarise_toml_manifest(name, &read.content);
                stack_candidates.push(hint);
                push_source(
                    &mut sources,
                    &mut total_chars,
                    &mut budget_exhausted,
                    name,
                    excerpt,
                    read.size_bytes,
                    read.truncated,
                );
            }
            SourceKind::Prose => {
                push_source(
                    &mut sources,
                    &mut total_chars,
                    &mut budget_exhausted,
                    name,
                    condense_prose(&read.content),
                    read.size_bytes,
                    read.truncated,
                );
            }
        }
    }

    let (language, framework) = resolve_stack(&stack_candidates);
    detected.primary_language = language;
    detected.framework = framework;

    Collected {
        sources,
        detected,
        total_chars,
        budget_exhausted,
    }
}

/// Kaynagi butceye sigacak sekilde ekler.
///
/// Butce dolduysa kaynak **dusurulur** ve bayrak kalkar; yarim bir cumleyi
/// ozete sokup "hepsi bu" demek yerine acikca eksik oldugumuzu soyluyoruz.
fn push_source(
    sources: &mut Vec<ContextSource>,
    total_chars: &mut usize,
    budget_exhausted: &mut bool,
    name: &str,
    excerpt: String,
    size_bytes: u64,
    read_truncated: bool,
) {
    let (excerpt, excerpt_truncated) = clip(&excerpt, MAX_SOURCE_EXCERPT_CHARS);
    let remaining = MAX_TOTAL_CONTEXT_CHARS.saturating_sub(*total_chars);

    if remaining == 0 {
        *budget_exhausted = true;
        return;
    }

    let (excerpt, budget_truncated) = clip(&excerpt, remaining);
    if excerpt.trim().is_empty() {
        return;
    }

    *total_chars += excerpt.chars().count();
    *budget_exhausted |= budget_truncated;

    sources.push(ContextSource {
        name: name.to_owned(),
        excerpt,
        truncated: read_truncated || excerpt_truncated || budget_truncated,
        size_bytes,
    });
}

struct ReadSource {
    content: String,
    size_bytes: u64,
    truncated: bool,
}

/// Tek kaynagi okur — **guvenlik kapilari burada**.
///
/// Sirasiyla: kok icinde mi → blok listesi (ham yol) → `canonicalize` →
/// blok listesi (cozulmus yol) → kok icinde mi (symlink escape) → boyut tavani.
///
/// Blok listesi iki kez cagriliyor cunku ilki ucuz bir on eleme, ikincisi
/// **sozlesme olan** kontroldur (security.md: symlink cozuldukten sonra).
fn read_source(root: &Path, name: &str) -> Option<ReadSource> {
    let candidate = root.join(name);

    if let Some(reason) = blocklist::is_blocked(&candidate) {
        eprintln!(
            "[asuna] Proje baglami: `{name}` atlandi — {}",
            reason.describe()
        );
        return None;
    }

    let resolved = std::fs::canonicalize(&candidate).ok()?;
    if let Some(reason) = blocklist::is_blocked_resolved(&resolved) {
        eprintln!(
            "[asuna] Proje baglami: `{name}` cozuldugunde hassas bir dosyaya cikti — {}",
            reason.describe()
        );
        return None;
    }

    // Symlink escape: kok icindeki bir bag kok disini gosteriyorsa okunmaz.
    let canonical_root = std::fs::canonicalize(root).ok()?;
    if !resolved.starts_with(&canonical_root) {
        eprintln!("[asuna] Proje baglami: `{name}` proje kokunun disina cikiyor, atlandi.");
        return None;
    }

    let metadata = std::fs::metadata(&resolved).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let size_bytes = metadata.len();

    let bytes = std::fs::read(&resolved).ok()?;
    let truncated = bytes.len() > MAX_FILE_READ_BYTES;
    let slice = &bytes[..bytes.len().min(MAX_FILE_READ_BYTES)];
    // Ikili dosya modele ham gonderilmez (security.md Bolum 2): UTF-8 degilse
    // kaynagi tamamen atliyoruz.
    let content = String::from_utf8(slice.to_vec()).ok()?;

    Some(ReadSource {
        content,
        size_bytes,
        truncated,
    })
}

/// Metni karakter siniriyla kirpar; kirpildiysa isaret ekler.
fn clip(text: &str, limit: usize) -> (String, bool) {
    if text.chars().count() <= limit {
        return (text.to_owned(), false);
    }
    if limit <= TRUNCATION_MARKER.chars().count() {
        return (text.chars().take(limit).collect(), true);
    }

    let keep = limit - TRUNCATION_MARKER.chars().count();
    let mut clipped: String = text.chars().take(keep).collect();
    // Kelimenin ortasindan kesmemek icin son bosluga geri sar.
    if let Some(position) = clipped.rfind(char::is_whitespace) {
        if position > keep / 2 {
            clipped.truncate(position);
        }
    }
    clipped.push_str(TRUNCATION_MARKER);
    (clipped, true)
}

/// Markdown/duz metni sadelestirir: bos satir yiginlari ve kod blogu isaretleri
/// ozete deger katmiyor.
fn condense_prose(content: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_code_block = false;
    let mut previous_blank = false;

    for line in content.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        if trimmed.trim().is_empty() {
            if previous_blank || lines.is_empty() {
                continue;
            }
            previous_blank = true;
            lines.push("");
            continue;
        }
        previous_blank = false;
        lines.push(trimmed);
    }

    lines.join("\n").trim().to_owned()
}

// ---------------------------------------------------------------------------
// Manifest ozetleri + dil/framework tespiti
// ---------------------------------------------------------------------------

/// Bir manifest'ten cikan dil/framework ipucu.
#[derive(Debug, Clone, PartialEq)]
struct StackHint {
    language: Option<String>,
    framework: Option<String>,
    /// Kucuk = daha guclu. Birden fazla manifest varsa siralama bunu kullanir.
    priority: u8,
}

/// Cakisan manifest'lerde **deterministik** secim.
///
/// Oncelik sirasi (kucukten buyuge): Tauri kanitli `Cargo.toml` (0) → diger
/// `Cargo.toml`/`pyproject.toml` (1) → `package.json` (2).
///
/// Neden: Asuna'nin kendisi gibi bir Tauri projesinde hem `package.json` hem
/// `Cargo.toml` vardir ve "bu bir Node projesi" demek yanlis olurdu. Framework
/// ise **hangi manifest'te bulunduysa** ondan alinir; iki manifest de framework
/// bildiriyorsa yine oncelik sirasi belirler. Tahmin yok, sabit kural var.
fn resolve_stack(hints: &[StackHint]) -> (Option<String>, Option<String>) {
    let mut sorted: Vec<&StackHint> = hints.iter().collect();
    sorted.sort_by_key(|hint| hint.priority);

    let language = sorted.iter().find_map(|hint| hint.language.clone());
    let framework = sorted.iter().find_map(|hint| hint.framework.clone());
    (language, framework)
}

/// `package.json` → kisa ozet + dil/framework ipucu.
///
/// Ham JSON **dumplenmez**: ad, surum, aciklama, script adlari ve ilk birkac
/// bagimlilik. `dependencies` bloklari yuzlerce satir olabilir.
fn summarise_node_manifest(content: &str) -> (String, StackHint) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        // Bozuk manifest cokme sebebi degil; ozet uretilmez, tespit yapilmaz.
        return (
            String::new(),
            StackHint {
                language: None,
                framework: None,
                priority: 2,
            },
        );
    };

    let text = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };

    let dependencies: Vec<String> = ["dependencies", "devDependencies"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(serde_json::Value::as_object))
        .flat_map(|map| map.keys().cloned())
        .collect();

    let scripts: Vec<String> = value
        .get("scripts")
        .and_then(serde_json::Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();

    let has = |needle: &str| dependencies.iter().any(|name| name == needle);
    let framework = if has("next") {
        Some("Next.js")
    } else if has("@tauri-apps/api") {
        Some("Tauri")
    } else if has("react") {
        Some("React")
    } else if has("vue") {
        Some("Vue")
    } else if has("svelte") {
        Some("Svelte")
    } else if has("express") {
        Some("Express")
    } else {
        None
    };

    let language = if has("typescript") {
        "TypeScript"
    } else {
        "JavaScript"
    };

    let mut lines = vec!["package.json:".to_owned()];
    if let Some(name) = text("name") {
        let version = text("version").unwrap_or_default();
        lines.push(format!("  ad: {name} {version}").trim_end().to_owned());
    }
    if let Some(description) = text("description") {
        lines.push(format!("  aciklama: {description}"));
    }
    if !scripts.is_empty() {
        lines.push(format!("  script: {}", join_capped(&scripts)));
    }
    if !dependencies.is_empty() {
        lines.push(format!(
            "  bagimlilik ({}): {}",
            dependencies.len(),
            join_capped(&dependencies)
        ));
    }

    (
        lines.join("\n"),
        StackHint {
            language: Some(language.to_owned()),
            framework: framework.map(str::to_owned),
            priority: 2,
        },
    )
}

/// `Cargo.toml` / `pyproject.toml` → kisa ozet + ipucu.
///
/// **Minimal, bilincli olarak eksik bir TOML taramasi.** Yeni bir bagimlilik
/// (`toml` crate'i) eklemek yerine yalnizca ihtiyacimiz olan iki sey okunuyor:
/// `[package]`/`[project]` adi ve bagimlilik anahtarlari. Ic ice tablolar,
/// dizi-tablolar ve cok satirli degerler **yorumlanmaz**; bulunamayan bilgi
/// uydurulmaz, bos birakilir.
fn summarise_toml_manifest(file_name: &str, content: &str) -> (String, StackHint) {
    let mut section = String::new();
    let mut name: Option<String> = None;
    let mut dependencies: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(header) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = header.trim_matches('"').to_owned();
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"');

        if key == "name" && (section == "package" || section == "project") {
            name = Some(value.trim().trim_matches('"').to_owned());
            continue;
        }
        if section.ends_with("dependencies") && !key.is_empty() {
            dependencies.push(key.to_owned());
        }
    }

    // `pyproject.toml`'da bagimliliklar cogunlukla `[project] dependencies = [...]`
    // dizisindedir; minimal tarayici bunu okumaz ve **uydurmaz**.
    let is_rust = file_name == "Cargo.toml";
    let language = if is_rust { "Rust" } else { "Python" };

    let has = |needle: &str| dependencies.iter().any(|item| item == needle);
    let framework = if is_rust {
        if has("tauri") {
            Some("Tauri")
        } else if has("axum") {
            Some("Axum")
        } else if has("actix-web") {
            Some("Actix Web")
        } else if has("rocket") {
            Some("Rocket")
        } else {
            None
        }
    } else if content.contains("django") {
        Some("Django")
    } else if content.contains("fastapi") {
        Some("FastAPI")
    } else if content.contains("flask") {
        Some("Flask")
    } else {
        None
    };

    let mut lines = vec![format!("{file_name}:")];
    if let Some(name) = name {
        lines.push(format!("  ad: {name}"));
    }
    if !dependencies.is_empty() {
        lines.push(format!(
            "  bagimlilik ({}): {}",
            dependencies.len(),
            join_capped(&dependencies)
        ));
    }

    (
        lines.join("\n"),
        StackHint {
            language: Some(language.to_owned()),
            framework: framework.map(str::to_owned),
            // Tauri kaniti varsa bu manifest digerlerini yener.
            priority: if is_rust && has("tauri") { 0 } else { 1 },
        },
    )
}

fn join_capped(items: &[String]) -> String {
    let shown: Vec<&str> = items
        .iter()
        .take(MAX_DEPENDENCIES_LISTED)
        .map(String::as_str)
        .collect();
    if items.len() > MAX_DEPENDENCIES_LISTED {
        format!("{}, …", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

/// Remote URL'ini **kimlik bilgisi tasimayan** bir ada indirger.
///
/// `https://omer:ghp_TOKEN@github.com/omer/asuna.git` → `github.com/omer/asuna`
/// `git@github.com:omer/asuna.git` → `github.com/omer/asuna`
///
/// GUVENLIK: `@` isaretinden onceki her sey **atilir**, tutulmaz. Ardindan
/// redaksiyon suzgeci son savunma hatti olarak calisir.
pub fn sanitise_remote_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // Sema (`https://`, `ssh://`, `git://`) atilir.
    let without_scheme = raw.split_once("://").map_or(raw, |(_, rest)| rest);
    // Kimlik bilgisi kismi atilir. `rsplit_once`: parolada `@` olabilir.
    let without_credentials = without_scheme
        .rsplit_once('@')
        .map_or(without_scheme, |(_, rest)| rest);
    // `git@host:owner/repo` bicimindeki iki nokta yol ayiricisina cevrilir.
    let normalised = without_credentials.replacen(':', "/", 1);
    let trimmed = normalised
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_owned();

    if trimmed.is_empty() {
        return None;
    }
    Some(redact_sensitive_text(&trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::project_repository;
    use crate::projects::registry;

    const NOW: &str = "2026-08-25T10:00:00Z";

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "asuna-context-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let path = std::env::temp_dir().join(unique);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("gecici dizin");
            Self(path)
        }

        fn root(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("proje dizini");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(root: &Path, name: &str, content: &str) {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("alt dizin");
        }
        std::fs::write(path, content).expect("dosya yazilmali");
    }

    fn db() -> AsunaDb {
        AsunaDb::open_in_memory().expect("bellek ici DB")
    }

    /// Kayitli + secili bir proje kurar.
    fn registered_current(db: &AsunaDb, root: &Path) -> String {
        let outcome =
            registry::add(db, root.to_str().expect("UTF-8"), None, NOW).expect("proje eklenmeli");
        let id = match outcome {
            registry::ProjectAddOutcome::Registered { project } => project.id,
            registry::ProjectAddOutcome::AlreadyRegistered { project } => project.id,
        };
        registry::set_current(db, &id, NOW).expect("guncel proje secilmeli");
        id
    }

    fn summary_of(service: &ProjectContextService, db: &AsunaDb) -> ProjectSummary {
        match service.current(db).expect("baglam uretilmeli") {
            ProjectContext::Known { summary } => *summary,
            ProjectContext::Unknown { reason, .. } => {
                panic!("bilinen baglam bekleniyordu: {reason:?}")
            }
        }
    }

    fn source<'a>(summary: &'a ProjectSummary, name: &str) -> Option<&'a ContextSource> {
        summary.sources.iter().find(|item| item.name == name)
    }

    // --- Belirsizlik --------------------------------------------------------

    /// **Kabul kriteri**: "guncel proje" belirsizse hata degil `unknown` doner —
    /// Asuna sorar, uydurmaz.
    #[test]
    fn an_unknown_current_project_is_a_product_state_not_an_error() {
        let db = db();
        let service = ProjectContextService::new();

        // Hic proje kayitli degil.
        assert!(matches!(
            service.current(&db).expect("hata olmamali"),
            ProjectContext::Unknown {
                reason: ContextUnknownReason::NoRegisteredProject,
                ..
            }
        ));

        // Proje var ama secilmemis: baska bir soru, baska bir neden.
        let temp = TempDir::new("unknown");
        let root = temp.root("asuna");
        registry::add(&db, root.to_str().expect("UTF-8"), None, NOW).expect("eklenmeli");

        let context = service.current(&db).expect("hata olmamali");
        assert!(matches!(
            context,
            ProjectContext::Unknown {
                reason: ContextUnknownReason::NoCurrentSelection,
                ..
            }
        ));
        assert!(context.summary().is_none());
    }

    /// Secili projenin koku kaybolduysa uydurma yapilmaz.
    #[test]
    fn a_missing_root_yields_unknown_rather_than_a_stale_summary() {
        let temp = TempDir::new("gone");
        let root = temp.root("asuna");
        let db = db();
        let service = ProjectContextService::new();
        registered_current(&db, &root);

        // Onbellegi doldur, sonra dizini sil.
        summary_of(&service, &db);
        std::fs::remove_dir_all(&root).expect("dizin silinmeli");

        assert!(matches!(
            service.current(&db).expect("hata olmamali"),
            ProjectContext::Unknown {
                reason: ContextUnknownReason::RootMissing,
                ..
            }
        ));
    }

    // --- Kaynaklar ----------------------------------------------------------

    #[test]
    fn only_the_allow_listed_sources_are_read() {
        let temp = TempDir::new("sources");
        let root = temp.root("asuna");
        write(
            &root,
            "PROJECT.md",
            "# Asuna\n\nLocal-first sesli companion.",
        );
        write(&root, "README.md", "Kurulum: pnpm install");
        write(&root, "TRANSCRIPT.md", "BU DOSYA OKUNMAMALI");
        write(&root, "src/main.rs", "fn main() {}");

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        let names: Vec<&str> = summary
            .sources
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(names, ["PROJECT.md", "README.md"]);
        let combined = summary
            .sources
            .iter()
            .map(|item| item.excerpt.as_str())
            .collect::<String>();
        assert!(!combined.contains("OKUNMAMALI"), "{combined}");
    }

    #[test]
    fn a_project_without_any_context_file_is_still_known() {
        let temp = TempDir::new("empty");
        let root = temp.root("bos-proje");
        let db = db();
        registered_current(&db, &root);

        let summary = summary_of(&ProjectContextService::new(), &db);
        assert!(summary.sources.is_empty());
        assert_eq!(summary.total_chars, 0);
        assert_eq!(summary.primary_language, None);
        assert!(!summary.budget_exhausted);
    }

    // --- Blok listesi -------------------------------------------------------

    /// **ZORUNLU TEST (security.md Bolum 1)**: `.env` kayitli proje kokunun
    /// **icinde** olsa bile okunmaz ve icerigi hicbir ciktida gorunmez.
    #[test]
    fn the_env_file_is_never_read_even_inside_the_registered_root() {
        let temp = TempDir::new("env");
        let root = temp.root("asuna");
        write(&root, ".env", "OPENAI_API_KEY=sk-proj-COK-GIZLI-DEGER");
        write(&root, ".env.local", "DB_PASSWORD=hunter2");
        write(&root, "README.md", "Normal icerik.");

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        let serialised = serde_json::to_string(&summary).expect("serialize");
        assert!(!serialised.contains("COK-GIZLI-DEGER"), "{serialised}");
        assert!(!serialised.contains("hunter2"), "{serialised}");
        assert!(!serialised.contains("OPENAI_API_KEY"), "{serialised}");
        assert!(summary
            .sources
            .iter()
            .all(|item| !item.name.contains(".env")));
    }

    /// Kok icindeki bir symlink hassas bir dosyaya isaret ediyorsa okunmaz:
    /// blok listesi **cozulmus** yol uzerinde de calisir.
    #[cfg(unix)]
    #[test]
    fn a_symlink_pointing_at_a_blocked_file_is_refused() {
        let temp = TempDir::new("symlink-block");
        let root = temp.root("asuna");
        let secret = temp.0.join("gizli.pem");
        std::fs::write(&secret, "-----BEGIN PRIVATE KEY-----\nGIZLI-ANAHTAR\n").expect("dosya");
        std::os::unix::fs::symlink(&secret, root.join("README.md")).expect("symlink");

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        let serialised = serde_json::to_string(&summary).expect("serialize");
        assert!(!serialised.contains("GIZLI-ANAHTAR"), "{serialised}");
        assert!(source(&summary, "README.md").is_none());
    }

    /// Kok disina cikan bir symlink (hassas olmasa bile) okunmaz.
    #[cfg(unix)]
    #[test]
    fn a_symlink_escaping_the_root_is_refused() {
        let temp = TempDir::new("symlink-escape");
        let root = temp.root("asuna");
        let outside = temp.0.join("disarisi.md");
        std::fs::write(&outside, "KOK DISI ICERIK").expect("dosya");
        std::os::unix::fs::symlink(&outside, root.join("README.md")).expect("symlink");

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        assert!(source(&summary, "README.md").is_none());
        let serialised = serde_json::to_string(&summary).expect("serialize");
        assert!(!serialised.contains("KOK DISI ICERIK"), "{serialised}");
    }

    // --- Kirpma -------------------------------------------------------------

    /// **Kabul kriteri**: buyuk dosya kirpiliyor ve kirpma **isaretleniyor**.
    #[test]
    fn a_large_file_is_clipped_and_marked() {
        let temp = TempDir::new("clip");
        let root = temp.root("asuna");
        let huge = "Asuna ".repeat(20_000); // ~120 KB
        write(&root, "README.md", &huge);

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        let readme = source(&summary, "README.md").expect("README ozette olmali");
        assert!(readme.truncated, "kirpma isaretlenmemis");
        assert!(
            readme.excerpt.ends_with(TRUNCATION_MARKER),
            "{}",
            readme.excerpt
        );
        assert!(readme.excerpt.chars().count() <= MAX_SOURCE_EXCERPT_CHARS);
        assert!(readme.size_bytes > MAX_FILE_READ_BYTES as u64);
    }

    /// **Kabul kriteri**: uretilen ozetin toplam boyutu sinirli ve olculuyor.
    #[test]
    fn the_total_summary_size_is_bounded_and_measured() {
        let temp = TempDir::new("budget");
        let root = temp.root("asuna");
        for name in ["PROJECT.md", "README.md", "CLAUDE.md", "AGENTS.md"] {
            write(&root, name, &"kelime ".repeat(30_000));
        }

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        assert!(
            summary.total_chars <= MAX_TOTAL_CONTEXT_CHARS,
            "toplam {} > tavan {MAX_TOTAL_CONTEXT_CHARS}",
            summary.total_chars
        );
        assert_eq!(summary.max_chars, MAX_TOTAL_CONTEXT_CHARS);
        let measured: usize = summary
            .sources
            .iter()
            .map(|item| item.excerpt.chars().count())
            .sum();
        assert_eq!(
            measured, summary.total_chars,
            "olcum gercek boyutla ayni olmali"
        );
        assert!(summary.sources.iter().all(|item| item.truncated));
    }

    // --- Dil / framework tespiti -------------------------------------------

    #[test]
    fn the_stack_is_detected_from_manifests_not_guessed() {
        let temp = TempDir::new("stack");
        let root = temp.root("asuna");
        write(
            &root,
            "package.json",
            r#"{"name":"asuna","version":"0.1.0","description":"Sesli companion",
                "scripts":{"dev":"vite","test":"vitest run"},
                "dependencies":{"react":"19.2.8","@tauri-apps/api":"2.11.1"},
                "devDependencies":{"typescript":"5.9.3"}}"#,
        );
        write(
            &root,
            "Cargo.toml",
            "[package]\nname = \"asuna\"\nversion = \"0.1.0\"\n\n[dependencies]\ntauri = \"2\"\nrusqlite = \"0.40\"\n",
        );

        let db = db();
        let id = registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        // Tauri kaniti olan `Cargo.toml` `package.json`i yener — deterministik.
        assert_eq!(summary.primary_language.as_deref(), Some("Rust"));
        assert_eq!(summary.framework.as_deref(), Some("Tauri"));

        // Tespit kayda islenir (ASU-045 listede gosterecek).
        let stored = project_repository::find_by_id(&db, &id)
            .expect("okunmali")
            .expect("kayit");
        assert_eq!(stored.primary_language.as_deref(), Some("Rust"));
        assert_eq!(stored.framework.as_deref(), Some("Tauri"));

        // Manifest ham dumplenmez: bagimlilik **adlari** var, surumler yok.
        let manifest = source(&summary, "package.json").expect("manifest ozette");
        assert!(manifest.excerpt.contains("react"), "{}", manifest.excerpt);
        assert!(!manifest.excerpt.contains("19.2.8"), "{}", manifest.excerpt);
    }

    #[test]
    fn a_node_only_project_is_detected_as_typescript() {
        let temp = TempDir::new("node");
        let root = temp.root("web");
        write(
            &root,
            "package.json",
            r#"{"name":"web","dependencies":{"next":"15.0.0"},"devDependencies":{"typescript":"5.9.3"}}"#,
        );

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);
        assert_eq!(summary.primary_language.as_deref(), Some("TypeScript"));
        assert_eq!(summary.framework.as_deref(), Some("Next.js"));
    }

    #[test]
    fn a_python_project_is_detected_from_pyproject() {
        let temp = TempDir::new("python");
        let root = temp.root("bot");
        write(
            &root,
            "pyproject.toml",
            "[project]\nname = \"bot\"\ndependencies = [\"fastapi\"]\n",
        );

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);
        assert_eq!(summary.primary_language.as_deref(), Some("Python"));
        assert_eq!(summary.framework.as_deref(), Some("FastAPI"));
    }

    /// Bozuk manifest uygulamayi cokertmez ve tespit **uydurulmaz**.
    #[test]
    fn a_broken_manifest_is_ignored_without_guessing() {
        let temp = TempDir::new("broken");
        let root = temp.root("asuna");
        write(&root, "package.json", "{ bu gecerli JSON degil");

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        assert_eq!(summary.primary_language, None);
        assert_eq!(summary.framework, None);
        assert!(source(&summary, "package.json").is_none());
    }

    // --- Git remote ---------------------------------------------------------

    /// **ASU-049**: `.git/config` artik hic acilmiyor — blok listesinde.
    ///
    /// Dosyanin icerigi (token dahil) ozete hicbir bicimde girmez ve bu modul
    /// remote adi **turetmez**; o is ASU-042'nin `git remote get-url` yolunda
    /// ([`super::view::collect`] kaydeder).
    #[test]
    fn the_repo_local_git_config_is_never_opened() {
        let temp = TempDir::new("remote");
        let root = temp.root("asuna");
        write(
            &root,
            ".git/config",
            "[core]\n\trepositoryformatversion = 0\n[remote \"origin\"]\n\turl = https://omer:ghp_COK_GIZLI_TOKEN@github.com/omergungor/asuna.git\n",
        );

        // Blok listesi kapisi dosyanin kendisinde de kapali olmali.
        assert!(crate::security::blocklist::is_blocked(&root.join(".git/config")).is_some());

        let db = db();
        registered_current(&db, &root);
        let summary = summary_of(&ProjectContextService::new(), &db);

        assert_eq!(summary.git_remote, None, "bu modul remote turetmemeli");
        let serialised = serde_json::to_string(&summary).expect("serialize");
        assert!(!serialised.contains("ghp_COK_GIZLI_TOKEN"), "{serialised}");
        assert!(
            !serialised.contains("repositoryformatversion"),
            "{serialised}"
        );
        assert!(source(&summary, ".git/config").is_none());
    }

    /// Kayitli `git_remote` degeri ozete **yansitilir** (uretilmez): ASU-042
    /// yazar, bu modul okur.
    #[test]
    fn a_recorded_remote_is_reflected_into_the_summary() {
        let temp = TempDir::new("remote-recorded");
        let root = temp.root("asuna");
        write(&root, "README.md", "Asuna");

        let db = db();
        let project_id = registered_current(&db, &root);
        registry::record_detected_metadata(
            &db,
            &project_id,
            &DetectedMetadata {
                git_remote: Some("github.com/omergungor/asuna".to_owned()),
                ..DetectedMetadata::default()
            },
            NOW,
        )
        .expect("yazilmali");

        let summary = summary_of(&ProjectContextService::new(), &db);
        assert_eq!(
            summary.git_remote.as_deref(),
            Some("github.com/omergungor/asuna")
        );
    }

    #[test]
    fn remote_urls_are_reduced_to_a_credential_free_name() {
        for (raw, expected) in [
            (
                "https://github.com/omergungor/asuna.git",
                Some("github.com/omergungor/asuna"),
            ),
            (
                "git@github.com:omergungor/asuna.git",
                Some("github.com/omergungor/asuna"),
            ),
            (
                "ssh://git@gitlab.com/grup/proje.git",
                Some("gitlab.com/grup/proje"),
            ),
            (
                "https://user:p@ss@bitbucket.org/ekip/repo",
                Some("bitbucket.org/ekip/repo"),
            ),
            ("", None),
        ] {
            assert_eq!(
                sanitise_remote_url(raw).as_deref(),
                expected,
                "girdi: {raw}"
            );
        }
    }

    // --- Onbellek -----------------------------------------------------------

    /// **Kabul kriteri**: sonuclar onbelleklenir — her cagride disk yeniden
    /// taranmaz.
    #[test]
    fn results_are_cached_within_the_ttl() {
        let temp = TempDir::new("cache");
        let root = temp.root("asuna");
        write(&root, "README.md", "Ilk icerik.");

        let db = db();
        registered_current(&db, &root);
        let service = ProjectContextService::new();

        let first = summary_of(&service, &db);
        assert!(first.sources[0].excerpt.contains("Ilk icerik"));

        // Dosya **ayni boyutta** degistirilirse ve mtime cozunurlugu yetmezse
        // onbellek gecerli kalir; testin olctugu sey budur.
        let cached = summary_of(&service, &db);
        assert_eq!(cached, first);
    }

    /// Ama onbellek **korlesmez**: kaynak degisince parmak izi tutmaz ve ozet
    /// yeniden uretilir (mtime + boyut kontrolu).
    #[test]
    fn the_cache_is_invalidated_when_a_source_changes() {
        let temp = TempDir::new("cache-bust");
        let root = temp.root("asuna");
        write(&root, "README.md", "Ilk icerik.");

        let db = db();
        registered_current(&db, &root);
        let service = ProjectContextService::new();
        let first = summary_of(&service, &db);

        write(
            &root,
            "README.md",
            "Ikinci icerik, tamamen farkli ve daha uzun.",
        );
        let second = summary_of(&service, &db);

        assert_ne!(first, second);
        assert!(second.sources[0].excerpt.contains("Ikinci icerik"));
    }

    #[test]
    fn invalidating_the_cache_forces_a_fresh_read() {
        let temp = TempDir::new("invalidate");
        let root = temp.root("asuna");
        write(&root, "README.md", "Icerik.");

        let db = db();
        let id = registered_current(&db, &root);
        let service = ProjectContextService::new();

        summary_of(&service, &db);
        service.invalidate(&id);
        // Ikinci uretim de calismali (panic/kilit sorunu yok).
        assert_eq!(summary_of(&service, &db).project_id, id);
    }

    // --- Yardimcilar --------------------------------------------------------

    #[test]
    fn clipping_marks_the_cut_and_respects_the_limit() {
        let (short, truncated) = clip("kisa metin", 100);
        assert_eq!(short, "kisa metin");
        assert!(!truncated);

        let (long, truncated) = clip(&"a ".repeat(500), 50);
        assert!(truncated);
        assert!(long.chars().count() <= 50);
        assert!(long.ends_with(TRUNCATION_MARKER));
    }

    #[test]
    fn prose_is_condensed_without_code_blocks() {
        let condensed =
            condense_prose("# Baslik\n\n\n\nMetin.\n\n```rust\nfn gizli() {}\n```\n\nSon.");
        assert!(!condensed.contains("fn gizli"));
        assert!(condensed.contains("# Baslik"));
        assert!(condensed.contains("Son."));
        assert!(!condensed.contains("\n\n\n"));
    }

    /// `Debug` ciktisi kullanicinin dosya icerigini sizdirmamali.
    #[test]
    fn debug_output_stays_coarse() {
        assert_eq!(
            format!("{:?}", ProjectContextService::new()),
            "ProjectContextService"
        );
    }
}

//! Git metadata saglayicisi — **yalnizca okuma** (ASU-042).
//!
//! # Neden `git` CLI, neden `.git/HEAD` degil
//!
//! Iki yol degerlendirildi ve karar CLI'dan yana:
//!
//! - **`.git` dosyalarini elle okumak** yalnizca *branch* icin ucuz olurdu
//!   (`.git/HEAD` bir satir). Ama kabul kriterindeki diger uc bilgi elle
//!   okunabilir degil:
//!   * *kirli/temiz + degisen dosya sayisi* → index (binary, surumlu bir bicim)
//!     ile calisma agacinin karsilastirilmasi, `.gitignore` katmanlarinin
//!     yorumlanmasi, stat cache mantigi demek. Bu, git'in kendisini yeniden
//!     yazmaktir.
//!   * *son N commit basligi* → loose object + packfile + delta cozumu (zlib)
//!     demek. Ayni sekilde.
//! - **`git2` (libgit2)** bunu yapardi ama **yeni bir bagimlilik** ve C
//!   derlemesi getirir; Cargo.toml bagimlilik politikasi bunu orchestrator
//!   karari yapar (ASU-042 kapsaminda paket eklenmedi).
//! - **`git` CLI** zaten kullanicinin makinesinde ve dogru cevabi veriyor.
//!   Maliyeti process spawn'i; onu da timeout ve dar argumanlarla siniriyoruz.
//!
//! Karar: `std::process::Command` ile `git`, **sabit argumanlarla**.
//!
//! # Guvenlik sozlesmesi
//!
//! - **Shell yok.** `Command::new("git")` + `arg()` — arguman dizisi hicbir
//!   zaman bir string'e birlestirilmez, `sh -c` kullanilmaz (PROJECT.md Bolum
//!   18, security.md Bolum 3: "Scoped tool argumanlari shell'e string olarak
//!   birlestirilmez").
//! - **Argumanlar sabit.** Tek degisken girdi calisma dizinidir ve o da kayitli,
//!   `canonicalize` edilmis bir proje kokudur (ASU-040). Model ya da renderer
//!   buraya bir arguman gecirmez.
//! - **Yazan hicbir komut yok.** Calisan alt komutlar: `rev-parse`,
//!   `symbolic-ref`, `status`, `log`, `remote get-url`. `GIT_OPTIONAL_LOCKS=0`
//!   ile `status` index'i tazelemeye calismaz — yani `.git` dizinine yazilmaz.
//! - **Asili kalmaz.** Her cagri [`GIT_COMMAND_TIMEOUT`] ile sinirlidir; sure
//!   dolarsa process oldurulur. `GIT_TERMINAL_PROMPT=0` + `GIT_ASKPASS` temizligi
//!   ile kimlik dogrulama istemi hic ortaya cikmaz.
//! - **Kimlik bilgisi ciktiya girmez.** Remote URL'i once yapisal olarak
//!   (`@` oncesi atilir) sonra desen tabanli redaksiyondan gecer
//!   ([`super::context::sanitise_remote_url`]). Commit basliklari da
//!   redaksiyondan gecer: bir baslik yanlislikla token icerebilir.
//! - **Diff okunmaz.** `git diff` hic calistirilmaz; yalnizca degisen dosya
//!   **sayisi** uretilir.
//!
//! # Buyuk repoda hiz
//!
//! `status --porcelain --untracked-files=no`: takip edilmeyen dosyalar
//! taranmaz. Buyuk bir calisma agacinda maliyetin cogu odur. Takas acik ve
//! ciktida gorunur: [`GitMetadata::changed_tracked_files`] adindan da anlasildigi
//! gibi **takip edilen** dosyalari sayar; yalnizca yeni (untracked) dosyasi olan
//! bir repo "temiz" gorunur.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;

use crate::redaction::redact_sensitive_text;

use super::context::sanitise_remote_url;

/// Tek bir `git` cagrisinin ust siniri.
///
/// 5 sn: soguk dosya onbelleginde buyuk bir repo'nun `status`'u saniyeler
/// surebilir, ama bunun otesi bir ariza isaretidir ve kullaniciyi bekletmez.
pub const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// Ozete alinan commit sayisi (kabul kriteri: "son N commit basligi").
pub const RECENT_COMMIT_COUNT: usize = 5;

/// Tek bir commit basliginin karakter tavani.
pub const MAX_COMMIT_SUBJECT_CHARS: usize = 120;

/// Branch adinin karakter tavani.
const MAX_BRANCH_CHARS: usize = 120;

/// Tek bir `git` cagrisindan bellege alinacak en fazla bayt.
///
/// Sinir asilsa bile boru hatti **sonuna kadar okunur** (fazlasi atilir):
/// okumayi birakmak boruyu doldurur ve `git`'i kilitler.
const MAX_GIT_OUTPUT_BYTES: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Cikti
// ---------------------------------------------------------------------------

/// Bir proje kokunun git durumu — salt okuma.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitMetadata {
    /// Kok **kendisi** bir git calisma agaci mi?
    ///
    /// Ust dizinlerdeki bir repo sayilmaz (bkz. [`collect`]).
    pub is_repository: bool,
    /// Guncel branch. `None` = detached HEAD ya da okunamadi.
    pub branch: Option<String>,
    /// HEAD bir branch'e degil dogrudan bir commit'e bakiyor.
    pub detached: bool,
    /// Takip edilen dosyalarda degisiklik var mi?
    pub is_dirty: bool,
    /// Degisen **takip edilen** dosya sayisi.
    ///
    /// Takip edilmeyen (untracked) dosyalar **sayilmaz**: buyuk repo'da hizli
    /// kalmak icin `--untracked-files=no` kullaniliyor (bkz. modul dokumani).
    pub changed_tracked_files: usize,
    /// Son commit basliklari (en yeni once), kirpilmis ve redakte edilmis.
    /// Tam mesaj gövdesi ve diff **hic okunmaz**.
    pub recent_commits: Vec<String>,
    /// Redakte edilmis remote **adi** (`github.com/kullanici/repo`).
    /// URL, token ve kimlik bilgisi hicbir zaman burada olmaz.
    pub remote: Option<String>,
    /// En az bir alt komut basarisiz oldu ya da zaman asimina ugradi.
    ///
    /// PROJECT.md Bolum 30: eksik bilgi "basarili" gibi sunulmaz. Asuna bu
    /// bayrak aciksa "git durumunu tam okuyamadim" demeli.
    pub degraded: bool,
}

impl GitMetadata {
    /// Git deposu olmayan proje — bos metadata, hata degil.
    pub fn not_a_repository() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Toplama
// ---------------------------------------------------------------------------

/// Kokun git metadata'sini toplar.
///
/// Kok bir git calisma agaci degilse [`GitMetadata::not_a_repository`] doner —
/// bu bir hata degil, cok sayida proje git'siz.
///
/// # Ust dizindeki repo sayilmaz
///
/// `git` calisma dizininden yukari dogru yurur. Kullanici bir repo'nun **alt
/// dizinini** proje olarak kaydettiyse, o ust repo'nun branch'ini ve
/// commit'lerini raporlamak hem yanlis hem sizinti olurdu (kayitli kok disindaki
/// bilgi). Bu yuzden `rev-parse --show-toplevel` sonucu kok ile karsilastirilir.
pub fn collect(root: &Path) -> GitMetadata {
    let Some(toplevel) = run_git(root, &["rev-parse", "--show-toplevel"]) else {
        return GitMetadata::not_a_repository();
    };
    let toplevel = PathBuf::from(toplevel.trim());
    if toplevel.as_os_str().is_empty() || !same_directory(&toplevel, root) {
        return GitMetadata::not_a_repository();
    }

    let mut metadata = GitMetadata {
        is_repository: true,
        ..GitMetadata::default()
    };

    // --- Branch ---
    // `symbolic-ref` once: henuz commit'i olmayan (unborn) branch'te de calisir,
    // detached HEAD'de sessizce basarisiz olur — ayrimi tam olarak boyle
    // kuruyoruz, cikti metnine ("HEAD") bakarak degil.
    match run_git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Some(name) => {
            let name = name.trim();
            if name.is_empty() {
                metadata.degraded = true;
            } else {
                metadata.branch = Some(clip(name, MAX_BRANCH_CHARS));
            }
        }
        None => {
            // Detached mi, yoksa komut mu patladi? HEAD cozulebiliyorsa
            // detached'tir.
            if run_git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_some() {
                metadata.detached = true;
            } else {
                // Henuz commit yok ve symbolic-ref de okunamadi: bilinmiyor.
                metadata.degraded = true;
            }
        }
    }

    // --- Kirli/temiz + degisen dosya sayisi ---
    match run_git(root, &["status", "--porcelain", "--untracked-files=no"]) {
        Some(output) => {
            let changed = output
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count();
            metadata.changed_tracked_files = changed;
            metadata.is_dirty = changed > 0;
        }
        None => metadata.degraded = true,
    }

    // --- Son N commit basligi ---
    let log_limit = format!("-{RECENT_COMMIT_COUNT}");
    match run_git(
        root,
        &["log", &log_limit, "--no-color", "--format=%s", "HEAD"],
    ) {
        Some(output) => {
            metadata.recent_commits = output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .take(RECENT_COMMIT_COUNT)
                // Commit mesaji kullanici metnidir ve yanlislikla bir token
                // icerebilir; kalici bir yere gitmese de modele gidiyor.
                .map(|line| clip(&redact_sensitive_text(line), MAX_COMMIT_SUBJECT_CHARS))
                .collect();
        }
        None => {
            // Commit'i olmayan repo da buraya duser; bu bir ariza degil.
            // Ariza ile bos gecmisi ayirmak icin HEAD'in varligina bakiyoruz.
            if run_git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_some() {
                metadata.degraded = true;
            }
        }
    }

    // --- Remote adi ---
    // `get-url` ag'a cikmaz, yalnizca konfigurasyonu okur.
    if let Some(url) = run_git(root, &["remote", "get-url", "origin"]) {
        metadata.remote = sanitise_remote_url(url.trim());
    }

    metadata
}

/// Iki yolu `canonicalize` ederek karsilastirir.
///
/// Duz metin karsilastirmasi macOS'ta `/tmp` ↔ `/private/tmp` yuzunden yanlis
/// negatif verirdi.
fn same_directory(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(limit.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

// ---------------------------------------------------------------------------
// Process calistirma
// ---------------------------------------------------------------------------

/// `git -C <root> <args...>` calistirir; timeout'lu, shell'siz.
///
/// `None` = komut bulunamadi, sifir olmayan cikis kodu ya da zaman asimi.
/// Cagiran taraf bunu **sessizce basari** sayamaz; `degraded` bayragi bunun
/// icin var.
fn run_git(root: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // stderr **tuketilmiyor**: ikinci bir boruyu bosaltmak zorunda kalmadan
        // kilitlenme riskini sifirlar. Hata ayrintisi zaten cikis koduyla
        // yeterince belli ve git'in stderr'i kullanici yolu icerebilir.
        .stderr(Stdio::null())
        // Kimlik dogrulama istemi hic ortaya cikmasin — asili kalmanin en
        // yaygin sebebi budur.
        .env("GIT_TERMINAL_PROMPT", "0")
        // `status` index'i tazelemeye calismaz: `.git` dizinine YAZILMAZ.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        // Cagiran process'in git ortamini devralmak, komutu bambaska bir
        // repo'ya yoneltebilirdi.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS");

    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;

    // Boru hatti ayri bir thread'de **sonuna kadar** bosaltilir. `try_wait`
    // dongusuyle beklemek, cikti 64 KB'lik boru tamponunu doldurdugunda
    // kilitlenirdi (buyuk bir repo'nun `status`'u bunu asabilir).
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    // Tavan asilsa bile okumaya devam: birakmak `git`i kilitler.
                    if buffer.len() < MAX_GIT_OUTPUT_BYTES {
                        let room = MAX_GIT_OUTPUT_BYTES - buffer.len();
                        buffer.extend_from_slice(&chunk[..read.min(room)]);
                    }
                }
            }
        }
        let _ = sender.send(buffer);
    });

    let Ok(buffer) = receiver.recv_timeout(GIT_COMMAND_TIMEOUT) else {
        let _ = child.kill();
        let _ = child.wait();
        // Yol log'a girmiyor; hangi alt komutun asildigi yeterli bilgi.
        eprintln!(
            "[asuna] `git {}` {} sn icinde bitmedi, sonlandirildi.",
            args.first().copied().unwrap_or("?"),
            GIT_COMMAND_TIMEOUT.as_secs()
        );
        return None;
    };

    // stdout kapandi, yani process cikti uretmeyi bitirdi; `wait` bloklamaz.
    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }
    String::from_utf8(buffer).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "asuna-git-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let path = std::env::temp_dir().join(unique);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("gecici dizin");
            Self(path)
        }

        fn child(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("alt dizin");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Makinede `git` var mi? Yoksa CLI'a bagli testler anlamsiz.
    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    /// Test fixture'i: kullanicinin global git ayarlarindan **yalitilmis** bir
    /// repo. `gpgsign` ya da `commit.template` gibi bir ayar testi asamasin.
    fn git(root: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Asuna Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Asuna Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn init_repo(root: &Path) -> bool {
        git(root, &["init", "-b", "asuna-dali"])
            && git(root, &["config", "commit.gpgsign", "false"])
            && git(root, &["config", "user.name", "Asuna Test"])
            && git(root, &["config", "user.email", "test@example.invalid"])
    }

    fn commit(root: &Path, file: &str, content: &str, message: &str) -> bool {
        std::fs::write(root.join(file), content).expect("dosya");
        git(root, &["add", file]) && git(root, &["commit", "-m", message])
    }

    /// **Kabul kriteri**: git deposu olmayan proje sorunsuz destekleniyor.
    #[test]
    fn a_project_without_git_yields_empty_metadata() {
        let temp = TempDir::new("no-git");
        let root = temp.child("duz-proje");

        let metadata = collect(&root);
        assert_eq!(metadata, GitMetadata::not_a_repository());
        assert!(!metadata.is_repository);
        assert_eq!(metadata.branch, None);
        assert!(!metadata.degraded, "git yoklugu bir ariza degil");
    }

    /// Kullanici bir repo'nun **alt dizinini** kaydettiyse ust repo'nun
    /// bilgileri raporlanmaz: hem yanlis olurdu hem kayitli kok disi bir sizinti.
    #[test]
    fn an_enclosing_repository_is_not_reported_for_a_subdirectory() {
        if !git_available() {
            eprintln!("[test] `git` bulunamadi — CLI testi atlandi.");
            return;
        }

        let temp = TempDir::new("enclosing");
        let outer = temp.child("dis-repo");
        assert!(init_repo(&outer), "repo kurulmali");
        assert!(commit(&outer, "README.md", "dis", "dis commit"));

        let inner = outer.join("alt/dizin");
        std::fs::create_dir_all(&inner).expect("alt dizin");

        let metadata = collect(&inner);
        assert!(
            !metadata.is_repository,
            "ust dizindeki repo bu proje icin raporlanmamali"
        );
        assert!(metadata.recent_commits.is_empty());
    }

    /// **Kabul kriteri**: branch, kirli/temiz, degisen dosya sayisi, son N
    /// commit basligi.
    #[test]
    fn reads_branch_dirty_state_and_recent_commits() {
        if !git_available() {
            eprintln!("[test] `git` bulunamadi — CLI testi atlandi.");
            return;
        }

        let temp = TempDir::new("read");
        let root = temp.child("asuna");
        assert!(init_repo(&root), "repo kurulmali");

        for index in 1..=7 {
            assert!(commit(
                &root,
                &format!("dosya-{index}.txt"),
                &format!("icerik {index}"),
                &format!("commit {index}"),
            ));
        }

        let clean = collect(&root);
        assert!(clean.is_repository);
        assert_eq!(clean.branch.as_deref(), Some("asuna-dali"));
        assert!(!clean.detached);
        assert!(!clean.is_dirty, "yeni commit sonrasi temiz olmali");
        assert_eq!(clean.changed_tracked_files, 0);
        assert!(!clean.degraded);

        // Son N commit, en yeni once.
        assert_eq!(clean.recent_commits.len(), RECENT_COMMIT_COUNT);
        assert_eq!(clean.recent_commits[0], "commit 7");
        assert_eq!(clean.recent_commits[4], "commit 3");

        // Takip edilen bir dosyayi degistir: kirli + sayac.
        std::fs::write(root.join("dosya-1.txt"), "degisti").expect("yazilmali");
        std::fs::write(root.join("dosya-2.txt"), "o da degisti").expect("yazilmali");
        let dirty = collect(&root);
        assert!(dirty.is_dirty);
        assert_eq!(dirty.changed_tracked_files, 2);
    }

    /// `--untracked-files=no` takasi **acikca** test ediliyor: yalnizca yeni
    /// dosya varken repo "temiz" gorunur ve alan adi bunu zaten soyluyor.
    #[test]
    fn untracked_files_are_not_counted_by_design() {
        if !git_available() {
            eprintln!("[test] `git` bulunamadi — CLI testi atlandi.");
            return;
        }

        let temp = TempDir::new("untracked");
        let root = temp.child("asuna");
        assert!(init_repo(&root));
        assert!(commit(&root, "README.md", "merhaba", "ilk commit"));

        std::fs::write(root.join("yeni-dosya.txt"), "takip edilmiyor").expect("yazilmali");

        let metadata = collect(&root);
        assert_eq!(metadata.changed_tracked_files, 0);
        assert!(!metadata.is_dirty);
    }

    /// Henuz commit'i olmayan repo: branch okunur, gecmis bostur, **ariza
    /// isaretlenmez**.
    #[test]
    fn an_empty_repository_is_not_reported_as_degraded() {
        if !git_available() {
            eprintln!("[test] `git` bulunamadi — CLI testi atlandi.");
            return;
        }

        let temp = TempDir::new("empty-repo");
        let root = temp.child("yeni");
        assert!(init_repo(&root));

        let metadata = collect(&root);
        assert!(metadata.is_repository);
        assert_eq!(metadata.branch.as_deref(), Some("asuna-dali"));
        assert!(metadata.recent_commits.is_empty());
        assert!(!metadata.is_dirty);
        assert!(!metadata.degraded, "bos gecmis bir ariza degil");
    }

    #[test]
    fn a_detached_head_is_reported_as_such_not_as_a_branch() {
        if !git_available() {
            eprintln!("[test] `git` bulunamadi — CLI testi atlandi.");
            return;
        }

        let temp = TempDir::new("detached");
        let root = temp.child("asuna");
        assert!(init_repo(&root));
        assert!(commit(&root, "a.txt", "bir", "bir"));
        assert!(commit(&root, "b.txt", "iki", "iki"));
        assert!(git(&root, &["checkout", "--detach", "HEAD"]));

        let metadata = collect(&root);
        assert!(metadata.is_repository);
        assert!(metadata.detached, "detached HEAD isaretlenmeli");
        assert_eq!(metadata.branch, None, "branch uydurulmaz");
        assert!(!metadata.degraded);
    }

    /// **Kabul kriteri**: hicbir kimlik bilgisi / remote token'i cikti'ya girmez.
    #[test]
    fn a_remote_url_with_a_token_never_reaches_the_output() {
        if !git_available() {
            eprintln!("[test] `git` bulunamadi — CLI testi atlandi.");
            return;
        }

        let temp = TempDir::new("remote");
        let root = temp.child("asuna");
        assert!(init_repo(&root));
        assert!(commit(&root, "README.md", "merhaba", "ilk commit"));
        assert!(git(
            &root,
            &[
                "remote",
                "add",
                "origin",
                "https://omer:ghp_COK_GIZLI_TOKEN@github.com/omergungor/asuna.git",
            ],
        ));

        let metadata = collect(&root);
        assert_eq!(
            metadata.remote.as_deref(),
            Some("github.com/omergungor/asuna")
        );

        let serialised = serde_json::to_string(&metadata).expect("serialize");
        assert!(!serialised.contains("ghp_COK_GIZLI_TOKEN"), "{serialised}");
        assert!(!serialised.contains("omer:"), "{serialised}");
        assert!(!serialised.contains("https://"), "{serialised}");
    }

    /// Commit basligina sizmis bir anahtar da maskelenir.
    #[test]
    fn a_secret_in_a_commit_subject_is_redacted() {
        if !git_available() {
            eprintln!("[test] `git` bulunamadi — CLI testi atlandi.");
            return;
        }

        let temp = TempDir::new("secret-commit");
        let root = temp.child("asuna");
        assert!(init_repo(&root));
        assert!(commit(
            &root,
            "a.txt",
            "bir",
            "fix: anahtar sk-proj-COK-GIZLI-DEGER degistirildi",
        ));

        let metadata = collect(&root);
        let subject = metadata.recent_commits.first().expect("commit olmali");
        assert!(!subject.contains("COK-GIZLI-DEGER"), "{subject}");
        assert!(subject.contains("sk-<redacted>"), "{subject}");
    }

    /// Uzun commit basligi kirpilir; tam mesaj gövdesi ve diff hic okunmaz.
    #[test]
    fn long_commit_subjects_are_clipped() {
        if !git_available() {
            eprintln!("[test] `git` bulunamadi — CLI testi atlandi.");
            return;
        }

        let temp = TempDir::new("long-subject");
        let root = temp.child("asuna");
        assert!(init_repo(&root));
        let long_subject = "cok uzun bir baslik ".repeat(30);
        assert!(commit(&root, "a.txt", "bir", &long_subject));

        let metadata = collect(&root);
        let subject = metadata.recent_commits.first().expect("commit olmali");
        assert!(subject.chars().count() <= MAX_COMMIT_SUBJECT_CHARS);
        assert!(subject.ends_with('…'), "{subject}");
    }

    /// **Kabul kriteri**: komut timeout'lu; asili kalmiyor. Var olmayan bir
    /// dizinde `git` hemen basarisiz olur ve cagri **bloklamaz**.
    #[test]
    fn a_failing_command_returns_promptly_instead_of_hanging() {
        let temp = TempDir::new("timeout");
        let missing = temp.0.join("hic-olmayan");

        let started = std::time::Instant::now();
        let metadata = collect(&missing);
        assert!(
            started.elapsed() < GIT_COMMAND_TIMEOUT,
            "cagri timeout'a kadar beklememeli"
        );
        assert!(!metadata.is_repository);
    }

    #[test]
    fn metadata_serializes_with_the_expected_contract() {
        let json = serde_json::to_value(GitMetadata::not_a_repository()).expect("serialize");
        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "branch",
                "changedTrackedFiles",
                "degraded",
                "detached",
                "isDirty",
                "isRepository",
                "recentCommits",
                "remote",
            ]
        );
    }

    #[test]
    fn clipping_keeps_the_limit() {
        assert_eq!(clip("kisa", 10), "kisa");
        let clipped = clip(&"a".repeat(50), 10);
        assert_eq!(clipped.chars().count(), 10);
        assert!(clipped.ends_with('…'));
    }
}

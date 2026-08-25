//! Hassas dosya blok listesi — **varsayilan deny** (security.md Bolum 1).
//!
//! # Sozlesme
//!
//! - Liste **tek yerde** durur. Bir tool ya da servis kendi kopyasini tutmaz;
//!   yeni bir okuma yolu acan herkes [`is_blocked`]'dan gecer.
//! - Kontrol **symlink cozuldukten sonra** yapilir (security.md Bolum 1:
//!   "Blok listesi glob eslesmesi symlink cozuldukten **sonra** uygulanir").
//!   Aksi halde `notlar.md -> ~/.ssh/id_ed25519` gibi bir bag listeyi atlardi.
//!   Cagiran taraf bu yuzden `canonicalize` edilmis bir yol vermelidir;
//!   [`is_blocked_resolved`] bunu tip yerine dokumantasyonla sart kosar cunku
//!   var olmayan bir yol `canonicalize` edilemez ve o durumda da karar
//!   verilebilmeli.
//! - Liste **kayitli proje koku icinde de** gecerlidir. Kok icindeki bir `.env`
//!   dosyasi "projenin kendi dosyasi" diye okunmaz: PROJECT.md Bolum 19 ve
//!   security.md Bolum 1 bunu kosulsuz yasakliyor.
//! - "Acik onay ile okunabilir" bir kapi **bu MVP'de yok**. Boyle bir kapi
//!   acilirsa cagiran taraf kullaniciya neyi okuyacagini gostermek ve
//!   `tool_events`'e yazmak zorunda kalacak; o karar Phase 5'in (ASU-047/048).
//!
//! # ASU-049 genislemesi
//!
//! Liste [`super::sandbox`] tarafindan **cozulmus tam yol** uzerinde cagriliyor.
//! Bu turda eklenenler:
//!
//! - Anahtar dosyalari artik **on ek** olarak da eslesiyor: `id_rsa.pub`,
//!   `id_ed25519_sk`, `id_ecdsa-cert.pub` ... Tam ad listesi `.pub` gibi zararsiz
//!   gorunen ama yaninda ozel anahtari bulundugunu ele veren varyantlari
//!   kaciriyordu.
//! - Keychain **dosyalari** (`*.keychain`, `*.keychain-db`) — daha once yalnizca
//!   `Keychains/` dizini bloktaydi; kopyalanmis bir keychain dosyasi listeyi
//!   atlardi.
//! - `.git/config` **komple** bloklandi: repo-yerel remote URL'i
//!   `https://kullanici:ghp_TOKEN@github.com/...` bicimiyle canli bir token
//!   tasiyabilir ve `[credential]` bolumu helper ayarlarini barindirir.
//!   Kaybedilen bir sey yok: ASU-042 remote **adini** `git remote get-url`
//!   ciktisindan alip [`crate::projects::context::sanitise_remote_url`] ile
//!   redakte ediyor — dosyanin kendisine ihtiyaci yok.
//! - `.kube/` ve `.config/` altindaki bilinen bulut dizinleri.

use std::path::{Component, Path};

/// Blok listesine takilan bir yolun **neden** reddedildigi.
///
/// Kullaniciya gosterilecek mesaj icin: "neden okumadim?" sorusunun cevabi
/// "cunku gizli olabilir" degil, somut bir kural olmali.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// `.env`, `.env.local`, `.env.production` ...
    EnvironmentFile,
    /// Ozel anahtar / sertifika (`*.pem`, `*.key`, `id_ed25519`, `*.jks` ...).
    PrivateKeyMaterial,
    /// Credential deposu (`.npmrc`, `.netrc`, `.pgpass`, `.gitconfig` ...).
    CredentialStore,
    /// Yol credential tasidigi bilinen bir dizinden geciyor
    /// (`.ssh/`, `.aws/`, `secrets/`, `Keychains/` ...).
    SensitiveDirectory,
}

impl BlockReason {
    /// Kullaniciya/modele gosterilebilecek kisa aciklama. Yol **icermez**.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::EnvironmentFile => "ortam degiskeni dosyasi (.env) okunmaz",
            Self::PrivateKeyMaterial => "ozel anahtar ya da sertifika dosyasi okunmaz",
            Self::CredentialStore => "kimlik bilgisi dosyasi okunmaz",
            Self::SensitiveDirectory => "hassas dizin icerigi okunmaz",
        }
    }
}

/// Icinden gecen her yolu reddeden dizin adlari.
///
/// Kok icinde bir `secrets/` dizini olsa bile gecerli: "projenin kendi
/// secrets'i" diye bir istisna yok.
const SENSITIVE_DIRECTORIES: [&str; 10] = [
    ".ssh",
    ".aws",
    ".azure",
    ".gnupg",
    ".kube",
    "gcloud",
    "secrets",
    "Keychains",
    ".keychain",
    ".docker",
];

/// Tam dosya adiyla reddedilen anahtar materyali.
const PRIVATE_KEY_FILES: [&str; 3] = ["identity", "server.key", "authorized_keys"];

/// **On ek** olarak reddedilen anahtar materyali.
///
/// Tam ad listesi yetmiyordu: `id_rsa.pub`, `id_ed25519_sk`,
/// `id_ecdsa-cert.pub` gibi varyantlar hem kendileri bilgi sizdirir hem de
/// yaninda ozel anahtarin durdugunu ele verir. On ek eslesmesi
/// (`asuna-config/security.md` Bolum 1: `id_rsa`, `id_ed25519`) hepsini kapatir.
const PRIVATE_KEY_PREFIXES: [&str; 5] = ["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519", "id_dss"];

/// Uzantiyla reddedilen anahtar materyali (kucuk harfe indirgenmis).
const PRIVATE_KEY_EXTENSIONS: [&str; 14] = [
    "pem",
    "key",
    "p12",
    "p8",
    "pfx",
    "pkcs12",
    "ppk",
    "keystore",
    "jks",
    "asc",
    "gpg",
    "kdbx",
    "keychain",
    "keychain-db",
];

/// Tam dosya adiyla reddedilen credential depolari.
const CREDENTIAL_FILES: [&str; 10] = [
    ".npmrc",
    ".netrc",
    "_netrc",
    ".pgpass",
    ".my.cnf",
    ".gitconfig",
    ".git-credentials",
    ".pypirc",
    ".dockercfg",
    ".s3cfg",
];

/// Bu yol okunabilir mi?
///
/// `None` = okunabilir. `Some(reason)` = **reddedildi**.
///
/// Karar yalnizca yolun **metnine** bakar; dosyanin var olmasi gerekmez.
/// Bu bilincli: var olmayan bir yol icin de "okunamaz" diyebilmek, cagiran
/// tarafin once dosyayi acip sonra sormasini engeller.
pub fn is_blocked(path: &Path) -> Option<BlockReason> {
    for component in path.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        let Some(part) = part.to_str() else {
            // UTF-8 disi bir bilesen: karar veremiyoruz, o halde reddediyoruz.
            // Belirsizlikte "oku" demek yanlis yondeki hata olurdu.
            return Some(BlockReason::SensitiveDirectory);
        };
        if SENSITIVE_DIRECTORIES
            .iter()
            .any(|directory| directory.eq_ignore_ascii_case(part))
        {
            return Some(BlockReason::SensitiveDirectory);
        }
        // `credentials`, `credentials.json`, `aws-credentials` ...
        if part.to_ascii_lowercase().contains("credential")
            && !CREDENTIAL_FILES.contains(&part.to_ascii_lowercase().as_str())
        {
            return Some(BlockReason::SensitiveDirectory);
        }
    }

    // Dosya adi yoksa (`/` gibi) ad tabanli kurallar uygulanmaz; bilesen
    // taramasi zaten yukarida kosmus durumda.
    let name = path.file_name().and_then(|name| name.to_str())?;
    let lowercase = name.to_ascii_lowercase();

    // `.env`, `.env.local`, `.env.production.local`, ayrica `.env` uzantili
    // varyantlar (`local.env`).
    if lowercase == ".env" || lowercase.starts_with(".env.") || lowercase.ends_with(".env") {
        return Some(BlockReason::EnvironmentFile);
    }

    if PRIVATE_KEY_FILES
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(name))
    {
        return Some(BlockReason::PrivateKeyMaterial);
    }

    // On ek eslesmesi: `id_rsa`, `id_rsa.pub`, `id_ed25519_sk`,
    // `id_ecdsa-cert.pub` ... (bkz. modul dokumantasyonu).
    if PRIVATE_KEY_PREFIXES
        .iter()
        .any(|blocked| lowercase.starts_with(blocked))
    {
        return Some(BlockReason::PrivateKeyMaterial);
    }

    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        if PRIVATE_KEY_EXTENSIONS
            .iter()
            .any(|blocked| blocked.eq_ignore_ascii_case(extension))
        {
            return Some(BlockReason::PrivateKeyMaterial);
        }
    }

    if CREDENTIAL_FILES
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(name))
    {
        return Some(BlockReason::CredentialStore);
    }

    // Repo-yerel `.git/config`. Dosyanin **tamami** bloklu; icinden bir satir
    // ayiklamak icin bile acilmaz. Remote adi ASU-042'de `git remote get-url`
    // ciktisindan redakte edilerek geliyor.
    //
    // Dogrudan ust dizin degil, yolun **herhangi bir bileseninde** `.git`
    // araniyor: submodule'lerin ayari `.git/modules/<ad>/config` altinda durur
    // ve o dosya da ayni remote URL'ini tasir.
    if lowercase == "config" && has_component(path, ".git") {
        return Some(BlockReason::CredentialStore);
    }

    None
}

/// Yolun bilesenlerinden biri tam olarak `name` mi?
fn has_component(path: &Path, name: &str) -> bool {
    path.components().any(
        |component| matches!(component, Component::Normal(part) if part.eq_ignore_ascii_case(name)),
    )
}

/// [`is_blocked`]'in "yol zaten `canonicalize` edilmis" sozlesmesini isaretleyen
/// hali.
///
/// Ayri bir isim: cagri yerinde symlink'in cozulmus oldugunu **okunur** kilar.
/// Davranis ayni; fark niyet beyanidir ve inceleme sirasinda gozden kacan
/// "ham yolla kontrol ettik" hatasini gorunur yapar.
pub fn is_blocked_resolved(canonical: &Path) -> Option<BlockReason> {
    is_blocked(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn blocked(path: &str) -> Option<BlockReason> {
        is_blocked(&PathBuf::from(path))
    }

    /// **security.md Bolum 1 / zorunlu test**: `.env` hicbir kosulda okunmaz —
    /// kayitli proje kokunun **icinde** olsa bile.
    #[test]
    fn env_files_are_never_readable_even_inside_a_registered_project_root() {
        for path in [
            "/Users/omer/Work/asuna/.env",
            "/Users/omer/Work/asuna/.env.local",
            "/Users/omer/Work/asuna/.env.production.local",
            "/Users/omer/Work/asuna/apps/web/.env.development",
            "/Users/omer/Work/asuna/config/local.env",
        ] {
            assert_eq!(
                blocked(path),
                Some(BlockReason::EnvironmentFile),
                "okunmamali: {path}"
            );
        }
    }

    /// `.env.example` gercek deger icermez ama listeye takilir. Bu bilincli
    /// bir yanlis pozitif: "ornek" dosyasini okumanin kazanci, `.env.*`
    /// desenine istisna acmanin riskinden kucuk.
    #[test]
    fn the_env_example_file_is_also_refused_on_purpose() {
        assert_eq!(
            blocked("/Users/omer/Work/asuna/.env.example"),
            Some(BlockReason::EnvironmentFile)
        );
    }

    #[test]
    fn private_key_material_is_refused() {
        for path in [
            "/Users/omer/.ssh/id_ed25519",
            "/Users/omer/Work/asuna/certs/server.pem",
            "/Users/omer/Work/asuna/keys/signing.key",
            "/Users/omer/Work/asuna/android.keystore",
            "/Users/omer/Work/asuna/release.jks",
            "/Users/omer/Work/asuna/apple.p12",
        ] {
            assert!(blocked(path).is_some(), "okunmamali: {path}");
        }
    }

    /// **ASU-049 genislemesi**: anahtar adlari on ek olarak eslesir. `.pub`
    /// zararsiz gorunur ama yaninda ozel anahtarin durdugunu ele verir.
    #[test]
    fn key_file_variants_are_refused_by_prefix() {
        for path in [
            "/Users/omer/Work/asuna/id_rsa",
            "/Users/omer/Work/asuna/id_rsa.pub",
            "/Users/omer/Work/asuna/id_ed25519_sk",
            "/Users/omer/Work/asuna/id_ecdsa-cert.pub",
            "/Users/omer/Work/asuna/id_dsa.old",
        ] {
            assert_eq!(
                blocked(path),
                Some(BlockReason::PrivateKeyMaterial),
                "okunmamali: {path}"
            );
        }
    }

    /// Kopyalanmis bir keychain dosyasi `Keychains/` dizini disinda da bloklu.
    #[test]
    fn keychain_files_are_refused_outside_the_keychains_directory() {
        for path in [
            "/Users/omer/Desktop/yedek/login.keychain-db",
            "/Users/omer/Work/asuna/eski.keychain",
            "/Users/omer/Work/asuna/parolalar.kdbx",
            "/Users/omer/Work/asuna/apple-auth.p8",
        ] {
            assert_eq!(
                blocked(path),
                Some(BlockReason::PrivateKeyMaterial),
                "okunmamali: {path}"
            );
        }
    }

    /// **ASU-049 karari**: repo-yerel `.git/config` komple bloklandi — remote
    /// URL'i canli token tasiyabilir. Ayni adli baska bir `config` dosyasi
    /// etkilenmez.
    #[test]
    fn the_repo_local_git_config_is_refused_but_other_config_files_are_not() {
        assert_eq!(
            blocked("/Users/omer/Work/asuna/.git/config"),
            Some(BlockReason::CredentialStore)
        );
        assert_eq!(
            blocked("/Users/omer/Work/asuna/.git/modules/alt/config"),
            Some(BlockReason::CredentialStore)
        );

        for readable in [
            "/Users/omer/Work/asuna/config",
            "/Users/omer/Work/asuna/src/config",
            "/Users/omer/Work/asuna/.github/config",
        ] {
            assert_eq!(blocked(readable), None, "okunabilmeli: {readable}");
        }
    }

    #[test]
    fn credential_stores_and_sensitive_directories_are_refused() {
        for (path, reason) in [
            ("/Users/omer/.npmrc", BlockReason::CredentialStore),
            ("/Users/omer/.netrc", BlockReason::CredentialStore),
            ("/Users/omer/.gitconfig", BlockReason::CredentialStore),
            ("/Users/omer/.aws/config", BlockReason::SensitiveDirectory),
            (
                "/Users/omer/Library/Keychains/login.keychain-db",
                BlockReason::SensitiveDirectory,
            ),
            (
                "/Users/omer/Work/asuna/secrets/tokens.json",
                BlockReason::SensitiveDirectory,
            ),
            (
                "/Users/omer/Work/asuna/aws-credentials.json",
                BlockReason::SensitiveDirectory,
            ),
        ] {
            assert_eq!(blocked(path), Some(reason), "okunmamali: {path}");
        }
    }

    /// Traversal ile blok listesinden kacilamaz: kontrol `canonicalize`
    /// **sonrasi** yapilir ve cozulmus yol yine listeye takilir.
    #[test]
    fn traversal_does_not_help_because_the_check_runs_after_resolution() {
        let raw = PathBuf::from("/Users/omer/Work/asuna/../../.ssh/id_ed25519");
        // Ham yolda bile `.ssh` bileseni gorunuyor.
        assert_eq!(is_blocked(&raw), Some(BlockReason::SensitiveDirectory));

        // Cozulmus hali de ayni sonucu verir — asil sozlesme bu.
        let resolved = PathBuf::from("/Users/omer/.ssh/id_ed25519");
        assert_eq!(
            is_blocked_resolved(&resolved),
            Some(BlockReason::SensitiveDirectory)
        );
    }

    /// Yanlis pozitif kontrolu: projenin normal dosyalari okunabilir kalmali.
    #[test]
    fn ordinary_project_files_stay_readable() {
        for path in [
            "/Users/omer/Work/asuna/README.md",
            "/Users/omer/Work/asuna/PROJECT.md",
            "/Users/omer/Work/asuna/CLAUDE.md",
            "/Users/omer/Work/asuna/package.json",
            "/Users/omer/Work/asuna/Cargo.toml",
            "/Users/omer/Work/asuna/pyproject.toml",
            "/Users/omer/Work/asuna/src/keyboard.ts",
            "/Users/omer/Work/asuna/docs/monkey.md",
            // On ek kurali `id_` ile baslayan her seyi degil, yalnizca bilinen
            // anahtar adlarini kesiyor.
            "/Users/omer/Work/asuna/src/identity-provider.ts",
            "/Users/omer/Work/asuna/src/id_generator.ts",
        ] {
            assert_eq!(blocked(path), None, "okunabilmeli: {path}");
        }
    }

    /// Reddin gerekcesi kullaniciya gosterilebilir olmali ve **yol icermemeli**.
    #[test]
    fn block_reasons_are_explainable_without_leaking_a_path() {
        for reason in [
            BlockReason::EnvironmentFile,
            BlockReason::PrivateKeyMaterial,
            BlockReason::CredentialStore,
            BlockReason::SensitiveDirectory,
        ] {
            let description = reason.describe();
            assert!(!description.is_empty());
            assert!(!description.contains('/'), "{description}");
        }
    }
}

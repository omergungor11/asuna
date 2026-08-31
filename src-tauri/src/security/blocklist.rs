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
///
/// # Ad kurallari iki kez kosar (tester B2)
///
/// Once yolun kendi metni, sonra [`fold_confusables`] ile **katlanmis** hali.
/// Boylece `．env` (fullwidth nokta) ya da Kiril `е` iceren bir `.еnv` listeyi
/// atlayamaz.
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
        if let Some(reason) = blocked_directory(part) {
            return Some(reason);
        }
        let folded = fold_confusables(part);
        if folded != part {
            if let Some(reason) = blocked_directory(&folded) {
                return Some(reason);
            }
        }
    }

    // Dosya adi yoksa (`/` gibi) ad tabanli kurallar uygulanmaz; bilesen
    // taramasi zaten yukarida kosmus durumda.
    let name = path.file_name().and_then(|name| name.to_str())?;

    if let Some(reason) = blocked_file_name(name) {
        return Some(reason);
    }
    let folded = fold_confusables(name);
    if folded != name {
        if let Some(reason) = blocked_file_name(&folded) {
            return Some(reason);
        }
    }

    // Repo-yerel `.git/config`. Dosyanin **tamami** bloklu; icinden bir satir
    // ayiklamak icin bile acilmaz. Remote adi ASU-042'de `git remote get-url`
    // ciktisindan redakte edilerek geliyor.
    //
    // Dogrudan ust dizin degil, yolun **herhangi bir bileseninde** `.git`
    // araniyor: submodule'lerin ayari `.git/modules/<ad>/config` altinda durur
    // ve o dosya da ayni remote URL'ini tasir.
    if folded.eq_ignore_ascii_case("config") && has_component(path, ".git") {
        return Some(BlockReason::CredentialStore);
    }

    None
}

/// Dizin adi kurallari — yolun **her** bileseni icin.
fn blocked_directory(part: &str) -> Option<BlockReason> {
    if SENSITIVE_DIRECTORIES
        .iter()
        .any(|directory| directory.eq_ignore_ascii_case(part))
    {
        return Some(BlockReason::SensitiveDirectory);
    }
    // `credentials`, `credentials.json`, `aws-credentials` ...
    let lowercase = part.to_ascii_lowercase();
    if lowercase.contains("credential") && !CREDENTIAL_FILES.contains(&lowercase.as_str()) {
        return Some(BlockReason::SensitiveDirectory);
    }
    None
}

/// Dosya adi kurallari.
///
/// # Nokta-bilesenleri (tester B2)
///
/// Yalnizca **son** uzantiya bakmak yetmiyordu: `backup.key.txt`,
/// `sunucu.pem.bak` ve `config.env.example` listeden geciyordu. Artik adin
/// **govdesinden sonraki her nokta-bileseni** bir uzanti gibi sinaniyor.
///
/// Govde (ilk bilesen) bilerek **disarida**: aksi halde `key.md` ya da
/// `secret.md` gibi siradan bir dokuman reddedilirdi. Kural "adin sonuna
/// zararsiz bir uzanti ekleyerek kacamazsin" demek; "adinda `key` gecen dosyayi
/// okumam" demek degil (`monkey.md` okunabilir kalir).
fn blocked_file_name(name: &str) -> Option<BlockReason> {
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

    if CREDENTIAL_FILES
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(name))
    {
        return Some(BlockReason::CredentialStore);
    }

    // Govdeden sonraki her bilesen bir uzanti gibi degerlendirilir.
    // `.env` gibi bir dotfile'da govde bos string olur ve `skip(1)` dogru
    // sonucu verir: `["env"]`.
    for suffix in lowercase.split('.').skip(1) {
        if suffix.is_empty() {
            continue;
        }
        if suffix == "env" {
            return Some(BlockReason::EnvironmentFile);
        }
        if PRIVATE_KEY_EXTENSIONS.contains(&suffix)
            || PRIVATE_KEY_PREFIXES
                .iter()
                .any(|blocked| suffix.starts_with(blocked))
        {
            return Some(BlockReason::PrivateKeyMaterial);
        }
        let dotted = format!(".{suffix}");
        if CREDENTIAL_FILES.contains(&dotted.as_str()) || CREDENTIAL_FILES.contains(&suffix) {
            return Some(BlockReason::CredentialStore);
        }
    }

    None
}

/// ASCII'ye **benzeyen** karakterleri ASCII'ye katlar.
///
/// # Kapsam ve sinir (bilincli)
///
/// Bu bir NFKC/UTS#39 uygulamasi **degildir** ve oyle olmadigi icin de yeni bir
/// bagimlilik gerektirmiyor. Katlanan kume:
///
/// - Tam genislikli ASCII (`U+FF01..U+FF5E`) → ASCII karsiligi; boylece
///   `．env` ve `．ｅｎｖ` yakalanir.
/// - Nokta benzerleri: `U+2024`, `U+3002`, `U+FF61` → `.`
/// - Gorunmez karakterler (sifir genislikli birlestirici/ayirici, yumusak
///   tire) ve birlesen aksan isaretleri (`U+0300..U+036F`) **atilir**.
/// - Kiril ve Yunan alfabesindeki yaygin ASCII homogliflleri (`е`→`e`,
///   `а`→`a`, `ο`→`o` ...).
///
/// Kapsam disi: baska yazi sistemlerindeki (Ermeni, Cherokee, matematiksel
/// harf blokları) homoglifler. Kararli bir saldirgan bunlarla hala liste disi
/// bir ad uretebilir — bu yuzden ad kurali **tek** savunma degil: icerik her
/// durumda `redaction::redact_sensitive_text`ten geciyor ve dosya yolu
/// `sandbox` tarafindan kok icine kilitleniyor.
fn fold_confusables(name: &str) -> String {
    /// (benzeyen, ASCII karsiligi) — hepsi kucuk harf.
    const HOMOGLYPHS: [(char, char); 27] = [
        // Kiril
        ('а', 'a'),
        ('в', 'b'),
        ('с', 'c'),
        ('ԁ', 'd'),
        ('е', 'e'),
        ('ѕ', 's'),
        ('һ', 'h'),
        ('і', 'i'),
        ('ј', 'j'),
        ('к', 'k'),
        ('м', 'm'),
        ('н', 'h'),
        ('о', 'o'),
        ('р', 'p'),
        ('т', 't'),
        ('у', 'y'),
        ('х', 'x'),
        // Yunan
        ('α', 'a'),
        ('ε', 'e'),
        ('η', 'n'),
        ('ι', 'i'),
        ('κ', 'k'),
        ('μ', 'm'),
        ('ν', 'v'),
        ('ο', 'o'),
        ('ρ', 'p'),
        ('τ', 't'),
    ];

    let mut folded = String::with_capacity(name.len());
    for character in name.chars() {
        let invisible = matches!(
            character,
            '\u{200B}'..='\u{200D}' | '\u{FEFF}' | '\u{00AD}' | '\u{2060}'
        ) || ('\u{0300}'..='\u{036F}').contains(&character);
        if invisible {
            continue;
        }

        // Once kucuk harfe: buyuk Kiril `Е` de tabloya dussun.
        let lowered = character.to_lowercase().next().unwrap_or(character);
        let mapped = match lowered {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(lowered as u32 - 0xFEE0).unwrap_or(lowered),
            '\u{2024}' | '\u{3002}' | '\u{FF61}' => '.',
            other => HOMOGLYPHS
                .iter()
                .find(|(from, _)| *from == other)
                .map_or(other, |(_, to)| *to),
        };
        folded.push(mapped);
    }
    folded
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

    /// **tester B2**: zararsiz bir uzanti eklemek listeyi atlatmiyor. Kontrol
    /// artik adin govdesinden sonraki **her** nokta-bilesenine bakiyor.
    #[test]
    fn appending_a_harmless_extension_does_not_bypass_the_list() {
        for (path, reason) in [
            (
                "/Users/omer/Work/asuna/backup.key.txt",
                BlockReason::PrivateKeyMaterial,
            ),
            (
                "/Users/omer/Work/asuna/sunucu.pem.bak",
                BlockReason::PrivateKeyMaterial,
            ),
            (
                "/Users/omer/Work/asuna/id_rsa.txt",
                BlockReason::PrivateKeyMaterial,
            ),
            (
                "/Users/omer/Work/asuna/yedek.id_ed25519.eski",
                BlockReason::PrivateKeyMaterial,
            ),
            (
                "/Users/omer/Work/asuna/kasa.p12.zip",
                BlockReason::PrivateKeyMaterial,
            ),
            (
                "/Users/omer/Work/asuna/config.env.example",
                BlockReason::EnvironmentFile,
            ),
            (
                "/Users/omer/Work/asuna/uygulama.env.sample",
                BlockReason::EnvironmentFile,
            ),
            (
                "/Users/omer/Work/asuna/yedek.npmrc.txt",
                BlockReason::CredentialStore,
            ),
        ] {
            assert_eq!(blocked(path), Some(reason), "okunmamali: {path}");
        }
    }

    /// **tester B2**: unicode benzerleriyle de atlatilamiyor.
    #[test]
    fn confusable_look_alike_names_are_refused() {
        for path in [
            // Kiril `е`
            "/Users/omer/Work/asuna/.еnv",
            // Tam genislikli nokta
            "/Users/omer/Work/asuna/．env",
            // Tam genislikli harfler
            "/Users/omer/Work/asuna/．ｅｎｖ",
            // Sifir genislikli birlestirici arada
            "/Users/omer/Work/asuna/.e\u{200B}nv",
            // Kiril `а` ile `id_rsа`
            "/Users/omer/Work/asuna/id_rsа",
            // Hassas dizin adi Kiril `ѕ` ile
            "/Users/omer/.ѕsh/id_ed25519",
        ] {
            assert!(blocked(path).is_some(), "okunmamali: {path}");
        }
    }

    /// Katlama **yalnizca** listeye yaklastirmak icin: Turkce ve diger
    /// alfabelerdeki siradan adlar okunabilir kalmali.
    #[test]
    fn folding_does_not_block_ordinary_non_ascii_names() {
        for path in [
            "/Users/omer/Work/asuna/önemli-notlar.md",
            "/Users/omer/Work/asuna/şirket-planı.md",
            "/Users/omer/Work/asuna/日本語.md",
            "/Users/omer/Work/asuna/ekip-toplantısı.txt",
        ] {
            assert_eq!(blocked(path), None, "okunabilmeli: {path}");
        }
    }

    /// Kararin siniri: kontrol adin **govdesini** kapsamaz. `key.md` bir
    /// dokumandir, anahtar degil — ve `monkey.md` de oyle.
    #[test]
    fn the_stem_of_a_name_is_not_treated_as_an_extension() {
        for path in [
            "/Users/omer/Work/asuna/key.md",
            "/Users/omer/Work/asuna/secret.md",
            "/Users/omer/Work/asuna/monkey.md",
            "/Users/omer/Work/asuna/env.ts",
            "/Users/omer/Work/asuna/pem.go",
        ] {
            assert_eq!(blocked(path), None, "okunabilmeli: {path}");
        }
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

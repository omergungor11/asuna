//! `.env` dosyasi icin minimal, bagimliliksiz okuyucu (ASU-009).
//!
//! # Neden `dotenvy` degil
//!
//! 1. **Process environment kirletilmiyor.** `dotenvy::dotenv()` okudugu degerleri
//!    `std::env::set_var` ile *tum process'e* yazar. Asuna ileride tool katmaninda
//!    alt process calistiracak (`run_tests`, `git_status` — PROJECT.md Bolum 18) ve
//!    bu process'ler ortami miras alir; yani `OPENAI_API_KEY` her cocuk process'e
//!    sizardi. Buradaki okuyucu bir `BTreeMap` dondurur, degerler yalnizca
//!    `AsunaConfig` icinde yasar.
//! 2. **Bagimlilik yuzeyi.** ~120 satirlik bir parser icin ek crate + transitive
//!    bagimlilik almak, "gerekcesiz bagimlilik ekleme" kuralina aykiri.
//! 3. **Hata mesaji kontrolu.** Sozdizimi hatasi satir numarasi ile raporlanir,
//!    satirin *icerigi* asla mesaja konmaz (satirda secret olabilir).
//!
//! # Desteklenen sozdizimi
//!
//! ```text
//! # yorum satiri
//! KEY=value
//! export KEY=value
//! KEY="tirnakli deger"      # \n \r \t \" \\ escape'leri cozulur
//! KEY='ham deger'           # escape cozulmez
//! KEY=                      # bos deger (bazi anahtarlar icin "belirtilmedi" demek)
//! ```
//!
//! Desteklenmeyen (bilerek): satir ici yorum (`KEY=v # yorum` -> deger `v # yorum`),
//! degisken interpolasyonu (`${OTHER}`), cok satirli deger. Ayni anahtar birden
//! fazla kez gecerse **son** tanim kazanir.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Aranan dosya adi.
pub const ENV_FILE_NAME: &str = ".env";

/// `.env` konumunu acikca belirlemek icin kullanilan bootstrap degiskeni.
/// Bu degisken `.env` icinden okunamaz (tavuk-yumurta), yalnizca process
/// environment'indan gelir.
pub const ENV_FILE_OVERRIDE_VAR: &str = "ASUNA_ENV_FILE";

/// `cwd`'den yukari dogru kac dizin taranacagi. `pnpm tauri dev` calisma dizinini
/// `src-tauri/` yapar, `.env` ise repo kokundedir — bu yuzden yukari arama gerekli.
const MAX_PARENT_LOOKUP_DEPTH: usize = 5;

/// `.env` okuma/ayristirma hatasi. Hicbir varyant dosya icerigini tasimaz.
#[derive(Debug)]
pub enum EnvFileError {
    /// Dosya okunamadi (yok, izin yok, dizin, ...).
    Read { path: PathBuf, source: io::Error },
    /// Sozdizimi hatasi — yalnizca satir numarasi raporlanir.
    Syntax {
        path: PathBuf,
        line: usize,
        reason: &'static str,
    },
}

impl fmt::Display for EnvFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "`{}` okunamadi: {source}", path.display())
            }
            Self::Syntax { path, line, reason } => {
                write!(f, "`{}` satir {line}: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for EnvFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Syntax { .. } => None,
        }
    }
}

/// Kullanilacak `.env` dosyasinin yolunu bulur.
///
/// Sira: `ASUNA_ENV_FILE` (varsa, dosya yoksa bile dondurulur ki hata acik olsun)
/// -> `cwd` ve en fazla [`MAX_PARENT_LOOKUP_DEPTH`] ust dizini.
/// Hicbiri yoksa `None` — bu hata degildir; degerler process environment'indan
/// gelebilir (CI, launchd, ileride Keychain).
pub fn find_env_file() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var(ENV_FILE_OVERRIDE_VAR) {
        if !explicit.trim().is_empty() {
            return Some(PathBuf::from(explicit));
        }
    }

    let cwd = std::env::current_dir().ok()?;
    let mut dir: Option<&Path> = Some(cwd.as_path());
    for _ in 0..=MAX_PARENT_LOOKUP_DEPTH {
        let current = dir?;
        let candidate = current.join(ENV_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = current.parent();
    }
    None
}

/// Dosyayi okur ve ayristirir.
pub fn load(path: &Path) -> Result<BTreeMap<String, String>, EnvFileError> {
    let contents = fs::read_to_string(path).map_err(|source| EnvFileError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&contents, path)
}

/// Ayristirma — dosya sistemine dokunmaz, test edilebilir.
pub fn parse(contents: &str, path: &Path) -> Result<BTreeMap<String, String>, EnvFileError> {
    let mut map = BTreeMap::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(EnvFileError::Syntax {
                path: path.to_path_buf(),
                line: line_number,
                reason: "`=` isareti yok — beklenen bicim `KEY=value`",
            });
        };

        let key = raw_key.trim();
        if key.is_empty() {
            return Err(EnvFileError::Syntax {
                path: path.to_path_buf(),
                line: line_number,
                reason: "degisken adi bos",
            });
        }
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        {
            return Err(EnvFileError::Syntax {
                path: path.to_path_buf(),
                line: line_number,
                reason: "degisken adinda gecersiz karakter var (A-Z, 0-9, `_`, `.` bekleniyor)",
            });
        }

        map.insert(key.to_owned(), unquote(raw_value.trim()));
    }

    Ok(map)
}

/// Tirnaklari soyar. Cift tirnakta escape dizileri cozulur, tek tirnakta cozulmez.
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = value.chars().next();
        let last = value.chars().last();
        if first == Some('\'') && last == Some('\'') {
            return value[1..value.len() - 1].to_owned();
        }
        if first == Some('"') && last == Some('"') {
            return unescape(&value[1..value.len() - 1]);
        }
    }
    value.to_owned()
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            // Taninmayan escape oldugu gibi birakilir — sessizce veri kaybetme.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> PathBuf {
        PathBuf::from("/tmp/test/.env")
    }

    #[test]
    fn parses_simple_pairs_and_skips_comments() {
        let map = parse(
            "# yorum\n\nOPENAI_API_KEY=sk-test\nASUNA_LOG_LEVEL=info\n",
            &p(),
        )
        .expect("ayristirma basarili olmali");

        assert_eq!(
            map.get("OPENAI_API_KEY").map(String::as_str),
            Some("sk-test")
        );
        assert_eq!(map.get("ASUNA_LOG_LEVEL").map(String::as_str), Some("info"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn supports_export_prefix_and_empty_values() {
        let map =
            parse("export ASUNA_REALTIME_VOICE=\n", &p()).expect("ayristirma basarili olmali");
        assert_eq!(
            map.get("ASUNA_REALTIME_VOICE").map(String::as_str),
            Some("")
        );
    }

    #[test]
    fn strips_quotes_and_resolves_escapes() {
        let map = parse(
            "A=\"iki\\nsatir\"\nB='ham \\n deger'\nC=  bosluklu  \n",
            &p(),
        )
        .expect("ayristirma basarili olmali");

        assert_eq!(map.get("A").map(String::as_str), Some("iki\nsatir"));
        assert_eq!(map.get("B").map(String::as_str), Some("ham \\n deger"));
        assert_eq!(map.get("C").map(String::as_str), Some("bosluklu"));
    }

    #[test]
    fn value_may_contain_equals_sign() {
        let map = parse("TOKEN=a=b=c\n", &p()).expect("ayristirma basarili olmali");
        assert_eq!(map.get("TOKEN").map(String::as_str), Some("a=b=c"));
    }

    #[test]
    fn last_definition_wins() {
        let map = parse("K=1\nK=2\n", &p()).expect("ayristirma basarili olmali");
        assert_eq!(map.get("K").map(String::as_str), Some("2"));
    }

    #[test]
    fn rejects_line_without_equals() {
        let error = parse("OPENAI_API_KEY\n", &p()).expect_err("hata bekleniyordu");
        assert!(matches!(error, EnvFileError::Syntax { line: 1, .. }));
    }

    #[test]
    fn rejects_invalid_key_characters() {
        let error = parse("BAD KEY=1\n", &p()).expect_err("hata bekleniyordu");
        assert!(matches!(error, EnvFileError::Syntax { line: 1, .. }));
    }

    /// GUVENLIK: sozdizimi hatasi mesaji satirin icerigini asla tasimaz —
    /// bozuk bir satirda secret bulunabilir.
    #[test]
    fn syntax_error_message_never_leaks_line_contents() {
        let error =
            parse("OPENAI_API_KEY sk-super-secret-value\n", &p()).expect_err("hata bekleniyordu");
        let message = error.to_string();
        assert!(
            !message.contains("sk-super-secret-value"),
            "mesaj: {message}"
        );
        assert!(message.contains("satir 1"), "mesaj: {message}");
    }

    #[test]
    fn read_error_reports_path() {
        let missing = PathBuf::from("/tmp/asuna-olmayan-dizin-9f3a/.env");
        let error = load(&missing).expect_err("hata bekleniyordu");
        assert!(matches!(error, EnvFileError::Read { .. }));
        assert!(error.to_string().contains(".env"));
    }

    /// `parse` process environment'ini kirletmez — degerler yalnizca donen map'te.
    #[test]
    fn parse_does_not_touch_process_environment() {
        let key = "ASUNA_TEST_PARSE_DOES_NOT_SET_ENV";
        let _ = parse(&format!("{key}=deger\n"), &p()).expect("ayristirma basarili olmali");
        assert!(std::env::var(key).is_err());
    }
}

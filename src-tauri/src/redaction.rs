//! Ortak redaksiyon suzgeci (Gate 3 / HIGH-2, `asuna-config/security.md` Bolum 5).
//!
//! # Iki ayri suzgec, iki ayri is
//!
//! - [`redact_secrets`] — **log/hata metni** icin. `sk-...` / `ek_...`
//!   gorunumlu her parcayi maskeler. Prefix'i **korur** (`sk-<redacted>`):
//!   bir sizinti raporunda "hangi tur anahtar" bilgisi teshis icin degerli ve
//!   prefix'in kendisi bir secret degil. ASU-011'den beri `realtime_token.rs`
//!   icindeydi; ozet/cikarim boru hatlari da ayni suzgeci kullandigi icin
//!   buraya tasindi.
//! - [`redact_sensitive_text`] — **kalici olarak saklanacak metin** icin
//!   (oturum ozeti, hafiza adayi). Prefix suzgecine ek olarak `Bearer <deger>`
//!   ve `parola: <deger>` / `api_key=<deger>` gibi **anahtar-deger** desenlerini
//!   maskeler.
//!
//! # Neden saklanan metin ayri bir suzgecten geciyor
//!
//! Ozet ve hafiza adaylari modelden gelir; girdileri kullanicinin konusmasi ve
//! dokumudur. Kullanici bir oturumda anahtarini sesli okursa ya da bir hata
//! ciktisini yapistirirsa, o deger ozet metnine ve oradan `memories.content`'e
//! **kalici** olarak girer. security.md Bolum 5: "Memory extraction secret
//! pattern'lerini (API key, token, parola) filtreler — sizan degeri saklamaz".
//!
//! # Kasitli sinirlar
//!
//! Bu bir kesinlik araci degil, bir **son savunma hatti**. Anahtar-deger
//! maskesi yalnizca `:` ya da `=` ile ayrilmis degerleri hedefler (`Bearer`
//! haric). "parolami degistirdim" gibi bir cumle maskelenmez — bunu maskelemek
//! kullanicinin gercek hafizasini bozardi ve burada verilen taviz bilincli:
//! **yanlis pozitif icerigi bozar, yanlis negatif zaten diger katmanlarca
//! (kullanici hafizayi gorur ve silebilir) yakalanabilir.**

/// Maskelenmis degerin yerine yazilan isaret.
pub const REDACTION_MARKER: &str = "<redacted>";

/// Ephemeral token prefix'i (voice.md Bolum 5).
const EPHEMERAL_PREFIX: &str = "ek_";

/// Kalici API key prefix'i.
const PERMANENT_KEY_PREFIX: &str = "sk-";

/// Metindeki `sk-...` / `ek_...` gorunumlu her parcayi maskeler.
///
/// Bu bir *son savunma hatti*: `realtime_token` / `summary` / `extraction`
/// hata varyantlarinin hicbiri zaten secret tasimiyor, ama IPC sinirindan ya da
/// log'dan gecen mesaj bu suzgecten gecirilir ki ilerideki bir degisiklik
/// sessizce sizinti uretmesin.
pub fn redact_secrets(input: &str) -> String {
    // Ayirici olarak whitespace ve JSON/tirnak gurultusu kullaniliyor; token
    // karakter kumesi (harf, rakam, `-`, `_`) disindaki her sey sinir sayilir.
    let is_token_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';

    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while !rest.is_empty() {
        // Sonraki aday baslangici: token karakteri olan bir konum.
        let Some(start) = rest.find(is_token_char) else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail.find(|c: char| !is_token_char(c)).unwrap_or(tail.len());
        let word = &tail[..end];

        let prefix = [PERMANENT_KEY_PREFIX, EPHEMERAL_PREFIX]
            .into_iter()
            .find(|prefix| word.starts_with(prefix));

        let Some(prefix) = prefix else {
            output.push_str(word);
            rest = &tail[end..];
            continue;
        };

        output.push_str(prefix);
        output.push_str(REDACTION_MARKER);
        // Zaten maskelenmis bir metin ikinci kez gecerse (ozet -> aday zinciri)
        // isaret cogaltilmaz: suzgec idempotent kalir.
        rest = tail[end..]
            .strip_prefix(REDACTION_MARKER)
            .unwrap_or(&tail[end..]);
    }

    output
}

/// Kalici olarak saklanacak metni redakte eder.
///
/// [`redact_secrets`] + `Bearer <deger>` + `anahtar: <deger>` / `anahtar=<deger>`.
/// Metnin geri kalanina **dokunulmaz**: hafiza icerigi kullanicinin verisidir,
/// suzgec yalnizca credential gorunumlu parcalari degistirir.
pub fn redact_sensitive_text(input: &str) -> String {
    mask_keyed_values(&redact_secrets(input))
}

// ---------------------------------------------------------------------------
// Anahtar-deger maskesi
// ---------------------------------------------------------------------------

/// Bir anahtarin degerini maskelemek icin aranan ayirici.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separator {
    /// Deger yalnizca `:` ya da `=` ile ayrildiysa maskelenir.
    Assignment,
    /// Bosluk yeter (`Authorization: Bearer abc123`).
    Whitespace,
}

/// Degeri credential sayilan anahtarlar (kucuk harfe indirgenmis).
///
/// Liste bilerek kisa ve **kelime tam eslesmesi**: `secretary` gibi bir kelime
/// tetiklememeli. `api key` / `api_key` / `api-key` cifti ayrica ele aliniyor.
const KEYED_SECRETS: [(&str, Separator); 11] = [
    ("password", Separator::Assignment),
    ("passwd", Separator::Assignment),
    ("parola", Separator::Assignment),
    ("parolam", Separator::Assignment),
    ("sifre", Separator::Assignment),
    ("şifre", Separator::Assignment),
    ("sifrem", Separator::Assignment),
    ("token", Separator::Assignment),
    ("secret", Separator::Assignment),
    ("apikey", Separator::Assignment),
    ("bearer", Separator::Whitespace),
];

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Deger token'inin bittigi yer. `<` de sinir: zaten maskelenmis bir
/// `<redacted>` ikinci kez islenmesin.
fn ends_value(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '"' | '\'' | ',' | ';' | ')' | ']' | '}' | '<' | '>' | '`'
        )
}

/// Anahtar ile deger arasinda gecilebilir bosluk/ayirici mi?
fn is_gap_char(character: char) -> bool {
    matches!(character, ' ' | '\t' | ':' | '=' | '"' | '\'')
}

fn lowercase(chars: &[char]) -> String {
    chars.iter().flat_map(|c| c.to_lowercase()).collect()
}

/// Kelimenin degeri credential mi?
///
/// Uc kural, hepsi tam kelime uzerinde: birebir eslesme (`parola`), alt cizgi
/// atilmis hali (`api_key` → `apikey`) ve credential sonekleri
/// (`access_token`, `client_secret`, `db_password`, `private_key`).
fn separator_for(word: &str) -> Option<Separator> {
    let exact = |candidate: &str| {
        KEYED_SECRETS
            .iter()
            .find(|(keyword, _)| *keyword == candidate)
            .map(|(_, separator)| *separator)
    };

    if let Some(separator) = exact(word) {
        return Some(separator);
    }

    let condensed: String = word.chars().filter(|character| *character != '_').collect();
    if let Some(separator) = exact(&condensed) {
        return Some(separator);
    }

    let suffix = word.rsplit('_').next().unwrap_or(word);
    (word.contains('_')
        && matches!(
            suffix,
            "token" | "secret" | "password" | "parola" | "sifre" | "key"
        ))
    .then_some(Separator::Assignment)
}

/// `api key` / `api_key` / `api-key` ciftini tanir; tanirsa anahtarin bittigi
/// konumu doner.
fn api_key_pair_end(chars: &[char], mut index: usize) -> Option<usize> {
    while index < chars.len() && matches!(chars[index], ' ' | '_' | '-') {
        index += 1;
    }
    let start = index;
    while index < chars.len() && is_word_char(chars[index]) {
        index += 1;
    }
    (lowercase(&chars[start..index]) == "key").then_some(index)
}

fn mask_keyed_values(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0usize;

    while index < chars.len() {
        if !is_word_char(chars[index]) {
            output.push(chars[index]);
            index += 1;
            continue;
        }

        // Kelimeyi oku.
        let word_start = index;
        while index < chars.len() && is_word_char(chars[index]) {
            index += 1;
        }
        let word = lowercase(&chars[word_start..index]);

        let mut keyword_end = index;
        let mut separator = separator_for(&word);
        if separator.is_none() && word == "api" {
            if let Some(end) = api_key_pair_end(&chars, index) {
                separator = Some(Separator::Assignment);
                keyword_end = end;
            }
        }

        // Anahtarin kendisi metinde kalir: "parola: <redacted>" okunabilir bir
        // cumledir, "<redacted>" tek basina neyin maskelendigini soylemez.
        output.extend(&chars[word_start..keyword_end]);
        index = keyword_end;

        let Some(separator) = separator else {
            continue;
        };

        // Ayirici bosluk: satir sonu gecilmez (deger ayni satirda olmali).
        let mut cursor = index;
        let mut has_assignment = false;
        while cursor < chars.len() && is_gap_char(chars[cursor]) {
            has_assignment |= matches!(chars[cursor], ':' | '=');
            cursor += 1;
        }

        let gap_ok = match separator {
            Separator::Assignment => has_assignment,
            Separator::Whitespace => cursor > index,
        };
        if !gap_ok {
            continue;
        }

        // Deger tirnak icindeyse sinir kapanis tirnagidir: `password="p@ss w0rd"`
        // icinde bosluk bulunabilir ve yarim maskelemek en kotu sonuc olurdu.
        let quote = (cursor > index)
            .then(|| chars[cursor - 1])
            .filter(|character| matches!(character, '"' | '\''));

        let value_start = cursor;
        while cursor < chars.len()
            && match quote {
                Some(quote) => chars[cursor] != quote && chars[cursor] != '\n',
                None => !ends_value(chars[cursor]),
            }
        {
            cursor += 1;
        }
        if cursor == value_start {
            continue;
        }

        output.extend(&chars[index..value_start]);
        output.push_str(REDACTION_MARKER);
        index = cursor;
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_permanent_and_ephemeral_token_shapes() {
        let cases = [
            ("Bearer sk-proj-ABC123", "Bearer sk-<redacted>"),
            ("token=ek_abc_DEF-99 bitti", "token=ek_<redacted> bitti"),
            (
                r#"{"value":"ek_gizli","key":"sk-gizli"}"#,
                r#"{"value":"ek_<redacted>","key":"sk-<redacted>"}"#,
            ),
            ("bos metin", "bos metin"),
            ("", ""),
        ];

        for (input, expected) in cases {
            assert_eq!(redact_secrets(input), expected, "girdi: {input}");
        }
    }

    /// **Gate 3 / HIGH-2**: icerige gomulu bir `sk-` degeri kalici kayda
    /// **maskeli** girer.
    #[test]
    fn an_api_key_embedded_in_stored_text_is_masked() {
        let redacted = redact_sensitive_text(
            "Kullanici anahtarini okudu: sk-proj-COK-GIZLI-DEGER, not aldik.",
        );

        assert!(!redacted.contains("COK-GIZLI-DEGER"), "{redacted}");
        assert!(redacted.contains("sk-<redacted>"), "{redacted}");
        assert!(redacted.starts_with("Kullanici anahtarini okudu:"));
        assert!(redacted.ends_with("not aldik."), "{redacted}");
    }

    #[test]
    fn masks_values_behind_credential_keywords() {
        let cases = [
            ("parola: hunter2", "parola: <redacted>"),
            ("Parola = Hunter2!", "Parola = <redacted>"),
            ("sifre:1234", "sifre:<redacted>"),
            ("password=\"p@ss w0rd\"", "password=\"<redacted>\""),
            ("api_key: abc123", "api_key: <redacted>"),
            ("API KEY = abc123", "API KEY = <redacted>"),
            ("apiKey:abc123", "apiKey:<redacted>"),
            ("token: gh_1234abcd", "token: <redacted>"),
            ("secret = s3cr3t", "secret = <redacted>"),
            ("access_token=abc123", "access_token=<redacted>"),
            ("client_secret: abc123", "client_secret: <redacted>"),
            (
                "Authorization: Bearer eyJhbGciOi",
                "Authorization: Bearer <redacted>",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(redact_sensitive_text(input), expected, "girdi: {input}");
        }
    }

    /// Yanlis pozitif kontrolu: normal Turkce cumleler bozulmaz.
    #[test]
    fn ordinary_sentences_are_left_alone() {
        let untouched = [
            "Parolasini degistirdi ve rahatladi.",
            "Sifre yoneticisi kullanmaya karar verdi.",
            "Secretary ile toplanti ayarlandi.",
            "Token bazli kimlik dogrulama konusuldu.",
            "Wake word tespiti cihazda kalacak.",
        ];

        for sentence in untouched {
            assert_eq!(redact_sensitive_text(sentence), sentence);
        }
    }

    /// Maskeleme idempotent: ikinci gecis `<redacted>` isaretini bozmaz.
    #[test]
    fn redaction_is_idempotent() {
        let once = redact_sensitive_text("parola: hunter2 ve key sk-proj-GIZLI");
        assert_eq!(redact_sensitive_text(&once), once);
    }

    /// Deger satir sonundan sonra gelirse maskelenmez — "parola:" ile bir
    /// sonraki satirin ilk kelimesi ayni sey degil.
    #[test]
    fn a_value_on_the_next_line_is_not_swallowed() {
        assert_eq!(
            redact_sensitive_text("parola:\nKararlar: yok"),
            "parola:\nKararlar: yok"
        );
    }
}

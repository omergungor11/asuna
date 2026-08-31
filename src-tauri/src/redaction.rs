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
//!   (oturum ozeti, hafiza adayi, **eklenen dosya icerigi**). Prefix suzgecine
//!   ek olarak dort katman daha calisir: PEM ozel anahtar bloklari, taninmis
//!   token **sekilleri** (AWS access key id, GitHub token'i, JWT) ve
//!   `Bearer <deger>` / `parola: <deger>` / `api_key=<deger>` gibi
//!   **anahtar-deger** desenleri.
//!
//! # Neden sekil tabanli desenler de var (tester B1)
//!
//! Anahtar-deger maskesi bir **anahtar adi** gormek zorunda. Yuklenen bir dosya
//! ise degeri cogu zaman adsiz tasir: `id_ed25519`in govdesi bir PEM blogudur,
//! bir CI ciktisindaki `AKIA...` ya da `ghp_...` tek basina durur, bir HAR/log
//! dosyasindaki JWT bir URL'in icindedir. Bunlar `chat::attachment_ingest`
//! yolundan hem `attachments.content` icine hem de OpenAI istek govdesine
//! giderdi; desenler bu yuzden **sekle** bakiyor.
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
//!
//! Sekil tabanli desenler de **taninan** bicimlerle sinirli: tanimadigimiz bir
//! saglayicinin token'i buradan gecer. Liste uzatilabilir; garanti eden sey
//! liste degil, katmanlarin toplami (`security::blocklist` ad kurali +
//! `projects::sandbox` + kullanicinin kaydi gorup silebilmesi).

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
/// Dort katman, bu sirayla:
///
/// 1. [`mask_pem_blocks`] — cok satirli ozel anahtar bloklari. **Once**, cunku
///    govdesi base64'tur ve digerlerinin icinde yanlislikla parcalanmasin.
/// 2. [`redact_secrets`] — `sk-` / `ek_` prefix'leri.
/// 3. [`mask_token_shapes`] — AWS access key id, GitHub token'i, JWT.
/// 4. [`mask_keyed_values`] — `parola: <deger>`, `api_key=<deger>`, `Bearer ...`.
///
/// Metnin geri kalanina **dokunulmaz**: hafiza icerigi kullanicinin verisidir,
/// suzgec yalnizca credential gorunumlu parcalari degistirir. Suzgec
/// **idempotent**: maskelenmis bir metin ikinci kez gecerse degismez.
pub fn redact_sensitive_text(input: &str) -> String {
    let without_keys = mask_pem_blocks(input);
    let without_prefixes = redact_secrets(&without_keys);
    mask_keyed_values(&mask_token_shapes(&without_prefixes))
}

// ---------------------------------------------------------------------------
// PEM bloklari
// ---------------------------------------------------------------------------

const PEM_BEGIN: &str = "-----BEGIN ";
const PEM_END: &str = "-----END ";

/// Yalnizca **ozel** anahtar bloklari maskelenir.
///
/// Sertifika (`BEGIN CERTIFICATE`) ve public key bloklari bilerek disarida:
/// ikisi de zaten yayilmak icin var ve maskelemek kullanicinin dosyasini
/// sebepsiz bozardi. Olcut basligin icinde `PRIVATE KEY` gecmesi — bu, RSA / EC
/// / OPENSSH / ENCRYPTED / `PGP PRIVATE KEY BLOCK` varyantlarinin hepsini
/// kapsar.
const PEM_PRIVATE_MARKER: &str = "PRIVATE KEY";

/// PEM ozel anahtar bloklarinin **govdesini** maskeler.
///
/// Baslik ve kapanis satiri metinde kalir: "burada bir ozel anahtar vardi"
/// bilgisi kullanici icin degerli ve satirlarin kendisi secret degil. Kapanis
/// satiri hic yoksa (kirpilmis dosya) metnin **sonuna kadar** maskelenir —
/// yarim bir blok da anahtarin tamamini tasiyor olabilir.
fn mask_pem_blocks(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    loop {
        let Some(begin) = rest.find(PEM_BEGIN) else {
            output.push_str(rest);
            break;
        };

        let block = &rest[begin..];
        let header_len = block.find('\n').unwrap_or(block.len());
        let header = &block[..header_len];

        if !header.to_ascii_uppercase().contains(PEM_PRIVATE_MARKER) {
            // Ilgilenmedigimiz bir PEM blogu: oldugu gibi birak, aramayi
            // basligin ardindan surdur.
            let keep = begin + PEM_BEGIN.len();
            output.push_str(&rest[..keep]);
            rest = &rest[keep..];
            continue;
        }

        output.push_str(&rest[..begin]);
        output.push_str(header);
        output.push('\n');
        output.push_str(REDACTION_MARKER);

        let body = &block[header_len..];
        let Some(end) = body.find(PEM_END) else {
            // Kapanis yok: geri kalani yutuldu.
            break;
        };
        let end_line = &body[end..];
        let end_len = end_line.find('\n').unwrap_or(end_line.len());
        output.push('\n');
        output.push_str(&end_line[..end_len]);
        rest = &end_line[end_len..];
    }

    output
}

// ---------------------------------------------------------------------------
// Token sekilleri
// ---------------------------------------------------------------------------

/// AWS erisim anahtari kimligi on ekleri (kalici, gecici, kullanici, rol).
/// Govde her zaman 16 buyuk harf/rakam — toplam 20 karakter.
const AWS_KEY_ID_PREFIXES: [&str; 4] = ["AKIA", "ASIA", "AIDA", "AROA"];

/// AWS anahtar kimliginin on ekten sonraki uzunlugu.
const AWS_KEY_ID_BODY: usize = 16;

/// GitHub token on ekleri: personal, oauth, user-to-server, server-to-server,
/// refresh. `gh_` **degil** — o bir token bicimi degil ve mevcut testlerdeki
/// `gh_1234abcd` ornegi anahtar-deger kuraliyla yakalaniyor.
const GITHUB_TOKEN_PREFIXES: [&str; 5] = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"];

/// GitHub token govdesi icin alt sinir. Gercek token'lar 36+ karakter; sinir
/// dusuk tutuldu ki kisaltilmis ornekler de yakalansin, ama `ghp_` yazip
/// birakmis bir cumle maskelenmesin.
const MIN_GITHUB_TOKEN_BODY: usize = 8;

const JWT_PREFIX: &str = "eyJ";

/// JWT'nin ilk parcasi (base64'lenmis basligi) icin alt sinir.
const MIN_JWT_HEADER: usize = 8;

/// JWT icin toplam alt sinir — `eyJa.b.c` gibi bir metin JWT sayilmaz.
const MIN_JWT_TOTAL: usize = 16;

fn is_token_body_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '-' || character == '_'
}

/// `eyJ...` ile baslayan uc parcali bir JWT'nin uzunlugu.
///
/// Ucuncu parca **bos olabilir** (`alg: none` ile imzasiz token); ilk iki parca
/// bos olamaz.
fn jwt_len(rest: &str) -> Option<usize> {
    if !rest.starts_with(JWT_PREFIX) {
        return None;
    }

    let mut index = 0usize;
    for part in 0..3 {
        let taken = rest[index..]
            .chars()
            .take_while(|character| is_token_body_char(*character))
            .count();
        if taken == 0 && part < 2 {
            return None;
        }
        if part == 0 && taken < MIN_JWT_HEADER {
            return None;
        }
        index += taken;

        if part < 2 {
            if !rest[index..].starts_with('.') {
                return None;
            }
            index += 1;
        }
    }

    (index >= MIN_JWT_TOTAL).then_some(index)
}

/// Kelime sinirindan baslayan bir token sekli varsa (yerine yazilacak metin,
/// tuketilen uzunluk).
fn masked_token_shape(rest: &str) -> Option<(String, usize)> {
    if let Some(consumed) = jwt_len(rest) {
        // JWT'nin basligi ve govdesi okunabilir claim'lerdir; on ek de bilgi
        // tasimadigi icin tamami maskelenir.
        return Some((REDACTION_MARKER.to_owned(), consumed));
    }

    for prefix in AWS_KEY_ID_PREFIXES {
        let Some(body) = rest.strip_prefix(prefix) else {
            continue;
        };
        if body.len() < AWS_KEY_ID_BODY {
            continue;
        }
        let (candidate, tail) = body.split_at(AWS_KEY_ID_BODY);
        let shaped = candidate
            .chars()
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit());
        // 20 karakterden uzun bir tanimlayicinin **parcasi** degil.
        let ends_here = !tail.starts_with(is_token_body_char);
        if shaped && ends_here {
            return Some((
                format!("{prefix}{REDACTION_MARKER}"),
                prefix.len() + AWS_KEY_ID_BODY,
            ));
        }
    }

    for prefix in GITHUB_TOKEN_PREFIXES {
        let Some(body) = rest.strip_prefix(prefix) else {
            continue;
        };
        let taken = body
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .count();
        if taken >= MIN_GITHUB_TOKEN_BODY {
            return Some((format!("{prefix}{REDACTION_MARKER}"), prefix.len() + taken));
        }
    }

    None
}

/// Adsiz duran token **sekillerini** maskeler.
///
/// Yalnizca kelime sinirindan basliyorsa denenir: `XAKIA...` ya da bir
/// tanimlayicinin ortasi eslesmez.
fn mask_token_shapes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut index = 0usize;
    let mut at_boundary = true;

    while index < input.len() {
        if at_boundary {
            if let Some((replacement, consumed)) = masked_token_shape(&input[index..]) {
                output.push_str(&replacement);
                index += consumed;
                // Tuketilen parcanin hemen ardi kelime siniri sayilmaz.
                at_boundary = false;
                continue;
            }
        }

        let character = input[index..].chars().next().unwrap_or('\0');
        output.push(character);
        at_boundary = !is_token_body_char(character);
        index += character.len_utf8();
    }

    output
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

    has_credential_suffix(word).then_some(Separator::Assignment)
}

/// Kelime bir credential **sonekiyle** mi bitiyor?
///
/// Sondaki `_id` once soyulur (tester B1): `aws_access_key_id` bir anahtar
/// **kimligi** tasir ve AWS'de kimligin kendisi de gizli sayilan cifttir.
/// Soyma yalnizca sonek listesine bakildiginda anlamli: `monkey_id` soyuldugunda
/// `monkey` kalir ve listede olmadigi icin maskelenmez.
///
/// Tek basina duran `key` / `token` gibi kelimeler buradan gecmez — onlar ya
/// [`KEYED_SECRETS`] tam eslesmesine takilir ya da hic maskelenmez ("Token
/// bazli kimlik dogrulama" cumlesi bozulmamali).
fn has_credential_suffix(word: &str) -> bool {
    const CREDENTIAL_SUFFIXES: [&str; 6] =
        ["token", "secret", "password", "parola", "sifre", "key"];

    let stem = word.strip_suffix("_id").unwrap_or(word);
    let suffix = stem.rsplit('_').next().unwrap_or(stem);
    if !CREDENTIAL_SUFFIXES.contains(&suffix) {
        return false;
    }
    // Ya birlesik bir ad (`api_key`, `access_token`) ya da `_id` ekli hali
    // (`key_id`); ciplak kelime degil.
    stem.contains('_') || word != stem
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

/// `chars[index..]` maskeleme isaretiyle mi basliyor?
fn starts_with_marker(chars: &[char], index: usize) -> bool {
    let marker: Vec<char> = REDACTION_MARKER.chars().collect();
    chars.len() >= index + marker.len() && chars[index..index + marker.len()] == marker[..]
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

        // Deger zaten maskelenmis bir parcayla bitiyorsa (`api_key=sk-<redacted>`
        // → onceki katman `sk-` on ekini maskeledi) isaret **cogaltilmaz**:
        // isaretin kendisi de yutulur ve tek bir `<redacted>` yazilir. Aksi
        // halde kullanici `<redacted><redacted>` gorurdu.
        if starts_with_marker(&chars, cursor) {
            cursor += REDACTION_MARKER.chars().count();
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

    // -----------------------------------------------------------------------
    // tester B1: sekil tabanli desenler
    // -----------------------------------------------------------------------

    /// PEM ozel anahtar blogunun **govdesi** kaybolur; baslik ve kapanis satiri
    /// kalir ("burada bir anahtar vardi" bilgisi degerli, satirlar secret degil).
    #[test]
    fn a_pem_private_key_body_is_masked_but_its_frame_is_kept() {
        let input = "Not: anahtari yapistirdim\n\
                     -----BEGIN RSA PRIVATE KEY-----\n\
                     MIIEowIBAAKCAQEA3Tz2mr7SZiAMfQyuvBjM9Oi92\n\
                     CDpiK5T4EBLmqA7cMLLuHmVLXTa1TWQvpFbwFvUPxT\n\
                     -----END RSA PRIVATE KEY-----\n\
                     devam eden not";

        let redacted = redact_sensitive_text(input);

        assert!(!redacted.contains("MIIEowIBAAKCAQEA"), "{redacted}");
        assert!(!redacted.contains("CDpiK5T4EBLmqA7c"), "{redacted}");
        assert!(
            redacted.contains("-----BEGIN RSA PRIVATE KEY-----"),
            "{redacted}"
        );
        assert!(
            redacted.contains("-----END RSA PRIVATE KEY-----"),
            "{redacted}"
        );
        assert!(redacted.contains(REDACTION_MARKER), "{redacted}");
        // Blogun disindaki metin bozulmaz.
        assert!(
            redacted.starts_with("Not: anahtari yapistirdim"),
            "{redacted}"
        );
        assert!(redacted.ends_with("devam eden not"), "{redacted}");
    }

    /// OPENSSH / EC / PGP varyantlari da ayni kurala girer; sertifika **girmez**
    /// (public materyal, maskelemek dosyayi sebepsiz bozardi).
    #[test]
    fn every_private_key_flavour_is_masked_but_certificates_are_not() {
        for label in [
            "OPENSSH PRIVATE KEY",
            "EC PRIVATE KEY",
            "ENCRYPTED PRIVATE KEY",
            "PGP PRIVATE KEY BLOCK",
            "PRIVATE KEY",
        ] {
            let input =
                format!("-----BEGIN {label}-----\nGIZLI_GOVDE_SATIRI\n-----END {label}-----");
            let redacted = redact_sensitive_text(&input);
            assert!(
                !redacted.contains("GIZLI_GOVDE_SATIRI"),
                "{label}: {redacted}"
            );
        }

        let certificate =
            "-----BEGIN CERTIFICATE-----\nMIIB9TCCAWACAQAwgbgx\n-----END CERTIFICATE-----";
        assert_eq!(
            redact_sensitive_text(certificate),
            certificate,
            "sertifika maskelenmemeli"
        );
    }

    /// Kapanis satiri yoksa (kirpilmis dosya) govde **metnin sonuna kadar**
    /// yutulur: yarim bir blok da anahtarin tamamini tasiyor olabilir.
    #[test]
    fn a_truncated_pem_block_is_masked_to_the_end_of_the_text() {
        let redacted =
            redact_sensitive_text("-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BA");

        assert!(!redacted.contains("MIIEvQIBADAN"), "{redacted}");
        assert!(redacted.ends_with(REDACTION_MARKER), "{redacted}");
    }

    #[test]
    fn masks_recognised_token_shapes_that_stand_on_their_own() {
        let cases = [
            (
                "kullanilan kimlik AKIAIOSFODNN7EXAMPLE oldu",
                "kullanilan kimlik AKIA<redacted> oldu",
            ),
            (
                "gecici kimlik: ASIAY34FZKBOKMUTVV7A.",
                "gecici kimlik: ASIA<redacted>.",
            ),
            (
                "curl -H 'Authorization: token ghp_16C7e42F292c6912E7710c838347Ae178B4a'",
                "curl -H 'Authorization: token ghp_<redacted>'",
            ),
            ("gho_abcdefgh12345678 ile giris", "gho_<redacted> ile giris"),
            (
                "cerez: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NSJ9.SflKxwRJSMeKKF2QT4",
                "cerez: <redacted>",
            ),
            // Imzasiz (alg: none) JWT'nin ucuncu parcasi bos olabilir.
            (
                "token eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0. bitti",
                "token <redacted> bitti",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(redact_sensitive_text(input), expected, "girdi: {input}");
        }
    }

    /// Yanlis pozitif kontrolu: sekle **benzeyen** ama olmayan metinler bozulmaz.
    #[test]
    fn text_that_only_resembles_a_token_shape_is_left_alone() {
        let untouched = [
            // Bir tanimlayicinin ortasi/parcasi degil.
            "XAKIAIOSFODNN7EXAMPLE bir sey degil",
            // 16'dan kisa govde.
            "AKIAKISA123 kod",
            // 20 karakterden uzun bir tanimlayicinin parcasi.
            "AKIAIOSFODNN7EXAMPLEXX devam",
            // Kucuk harf: AWS kimligi buyuk harf/rakamdir.
            "akiaiosfodnn7example",
            // `gh_` bir token bicimi degil (ve govde cok kisa).
            "ghp_kisa",
            // Tek parcali: JWT degil.
            "eyJhbGciOiJIUzI1NiJ9 tek parca",
            // Iki parcali: JWT degil.
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0",
            "Wake word tespiti cihazda kalacak.",
        ];

        for sentence in untouched {
            assert_eq!(
                redact_sensitive_text(sentence),
                sentence,
                "girdi: {sentence}"
            );
        }
    }

    /// tester B1: `*_key_id` / `*_key` sonekli atamalarin **ikisi de** yakalanir.
    #[test]
    fn masks_values_behind_key_and_key_id_suffixes() {
        let cases = [
            (
                "aws_access_key_id = AKIAIOSFODNN7EXAMPLE",
                "aws_access_key_id = <redacted>",
            ),
            (
                "aws_secret_access_key = wJalrXUtnFEMI/K7MDENG",
                "aws_secret_access_key = <redacted>",
            ),
            ("private_key: abc123", "private_key: <redacted>"),
            ("key_id=abc123", "key_id=<redacted>"),
            ("subscription_key_id: 42", "subscription_key_id: <redacted>"),
        ];

        for (input, expected) in cases {
            assert_eq!(redact_sensitive_text(input), expected, "girdi: {input}");
        }
    }

    /// Yanlis pozitif kontrolu: `_id` soymasi normal alan adlarini bozmamali.
    #[test]
    fn ordinary_identifier_fields_are_not_masked() {
        let untouched = [
            "monkey_id = 5",
            "user_id = 42",
            "order_id: 7",
            "session_id=1234",
            "project_id: asuna",
            "Token bazli kimlik dogrulama konusuldu.",
        ];

        for sentence in untouched {
            assert_eq!(
                redact_sensitive_text(sentence),
                sentence,
                "girdi: {sentence}"
            );
        }
    }

    /// Koordinator karari: iki katman ayni degeri maskelediginde isaret
    /// **cogaltilmaz**. Onceki davranis `OPENAI_API_KEY=<redacted><redacted>`
    /// uretiyordu.
    #[test]
    fn two_layers_masking_the_same_value_produce_a_single_marker() {
        let cases = [
            (
                "OPENAI_API_KEY=sk-proj-COK-GIZLI",
                "OPENAI_API_KEY=<redacted>",
            ),
            ("api_key: ek_gizli_token", "api_key: <redacted>"),
            (
                "github_token = ghp_16C7e42F292c6912E7710c838347Ae178B4a",
                "github_token = <redacted>",
            ),
        ];

        for (input, expected) in cases {
            let redacted = redact_sensitive_text(input);
            assert_eq!(redacted, expected, "girdi: {input}");
            assert_eq!(
                redacted.matches(REDACTION_MARKER).count(),
                1,
                "isaret cogaltildi: {redacted}"
            );
        }
    }

    /// Yeni katmanlarin hepsi idempotent kalmali (ozet → aday zinciri metni
    /// birden fazla kez suzgecten geciriyor).
    #[test]
    fn the_new_patterns_are_idempotent_too() {
        let input = "-----BEGIN RSA PRIVATE KEY-----\nGOVDE\n-----END RSA PRIVATE KEY-----\n\
                     aws_access_key_id = AKIAIOSFODNN7EXAMPLE\n\
                     ghp_16C7e42F292c6912E7710c838347Ae178B4a\n\
                     eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NSJ9.SflKxwRJSMeKKF2QT4";

        let once = redact_sensitive_text(input);
        assert_eq!(redact_sensitive_text(&once), once, "{once}");
        for secret in [
            "GOVDE",
            "AKIAIOSFODNN7EXAMPLE",
            "16C7e42F292c6912E7710c838347Ae178B4a",
            "SflKxwRJSMeKKF2QT4",
        ] {
            assert!(!once.contains(secret), "maskelenmemis: {secret} / {once}");
        }
    }
}

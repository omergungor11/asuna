//! Dogrulanmis fiyat tablosu ve oturum maliyeti tahmini (ASU-033).
//!
//! # Tek kural: uydurma fiyat yok
//!
//! Buradaki rakamlarin tek kaynagi `docs/architecture/voice.md` Bolum 6'daki
//! tablodur (erisim tarihi **2026-08-24**, kaynak `developers.openai.com`).
//! Tabloda **olmayan** bir model icin maliyet hesaplanmaz; tahmin edilmez,
//! "yaklasik su kadardir" denmez — [`estimate_realtime_cost_usd`] `None` doner
//! ve UI "bilinmiyor" yazar (ASU-032 karari, PROJECT.md "never invent").
//!
//! # Neden cogu oturumda yine `None` cikabilir
//!
//! Fiyat token **turune** gore degisiyor: ses girisi metin girisinin 8 kati,
//! cache'lenmis giris ise 80'de biri. Yani toplam token sayisi tek basina bir
//! maliyet uretmez; kirilim gerekir. Kirilim `Usage.inputTokensDetails`
//! icinde gelir ama anahtar isimleri **runtime'da dogrulanmadi**
//! (voice.md Bolum 6 "BELIRSIZ", memory.md T5).
//!
//! Bu yuzden hesap **kapali kume** mantigiyla calisir: kirilim tanidigimiz
//! anahtarlarla toplami tam olarak aciklayabiliyorsa fiyat hesaplanir, aksi
//! halde `None` doner ve gorulen anahtar adlari bir kez log'lanir. Boylece
//! belirsizlik tahminle degil **gozlemle** kapanir.
//!
//! Log'lanan sey yalnizca anahtar **adlari** ve sayilardir; kullanici icerigi
//! ya da secret degildir.

use crate::db::session_repository::SessionUsage;

/// Bir modelin token fiyatlari — **USD / 1M token** (voice.md Bolum 6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelRates {
    pub text_in: f64,
    pub cached_text_in: f64,
    pub text_out: f64,
    pub audio_in: f64,
    pub cached_audio_in: f64,
    pub audio_out: f64,
}

/// Fiyat tablosunun erisim tarihi. Fiyat degisirse bu tarih de degismeli.
pub const RATES_VERIFIED_AT: &str = "2026-08-24";

/// Dogrulanmis Realtime fiyatlari. Yeni bir model **ancak** kaynaktan
/// dogrulanarak eklenir.
const REALTIME_RATES: [(&str, ModelRates); 3] = [
    (
        "gpt-realtime-2.1",
        ModelRates {
            text_in: 4.00,
            cached_text_in: 0.40,
            text_out: 24.00,
            audio_in: 32.00,
            cached_audio_in: 0.40,
            audio_out: 64.00,
        },
    ),
    (
        "gpt-realtime-2.1-mini",
        ModelRates {
            text_in: 0.60,
            cached_text_in: 0.06,
            text_out: 2.40,
            audio_in: 10.00,
            cached_audio_in: 0.30,
            audio_out: 20.00,
        },
    ),
    (
        // Eski snapshot; shutdown 2027-01-20. Fiyat kaydi eski oturumlarin
        // maliyeti icin duruyor.
        "gpt-realtime-mini",
        ModelRates {
            text_in: 0.60,
            cached_text_in: 0.06,
            text_out: 2.40,
            audio_in: 10.00,
            cached_audio_in: 0.30,
            audio_out: 20.00,
        },
    ),
];

/// Modelin fiyatlari — tabloda yoksa `None`.
pub fn realtime_rates(model: &str) -> Option<ModelRates> {
    REALTIME_RATES
        .iter()
        .find(|(name, _)| *name == model)
        .map(|(_, rates)| *rates)
}

/// Kirilimda tanidigimiz anahtarlar.
const AUDIO_TOKENS: &str = "audio_tokens";
const TEXT_TOKENS: &str = "text_tokens";
/// Tanidigimiz ama **fiyatlandiramadigimiz** anahtarlar: cache indirimi
/// ses/metin ayrimini gerektiriyor, o ayrim kirilimda yok. Sifirdan buyukse
/// hesap yapilmaz (yanlis bir sayi uretmektense sayi uretmemek dogru).
const UNPRICEABLE_TOKENS: [&str; 2] = ["cached_tokens", "image_tokens"];

/// Bir yonun (giris/cikis) ses/metin ayrimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TokenSplit {
    audio: i64,
    text: i64,
}

/// Kirilimi cozer. Toplami **tam olarak** aciklayamiyorsa `None`.
fn split_tokens(
    direction: &str,
    total: Option<i64>,
    details: &[serde_json::Value],
) -> Option<TokenSplit> {
    let total = total?;
    if total == 0 {
        return Some(TokenSplit { audio: 0, text: 0 });
    }

    let mut audio = 0_i64;
    let mut text = 0_i64;
    let mut unresolved: Vec<String> = Vec::new();

    for entry in details {
        let Some(object) = entry.as_object() else {
            unresolved.push("<nesne-degil>".to_owned());
            continue;
        };
        for (key, value) in object {
            let Some(count) = value.as_i64() else {
                unresolved.push(key.clone());
                continue;
            };
            if count == 0 {
                continue;
            }
            match key.as_str() {
                AUDIO_TOKENS => audio += count,
                TEXT_TOKENS => text += count,
                other => unresolved.push(other.to_owned()),
            }
        }
    }

    if !unresolved.is_empty() {
        // GOZLEM: anahtar adlari (deger degil) log'lanir ki sema tahminle
        // degil olcumle netlessin (voice.md Bolum 6).
        unresolved.sort_unstable();
        unresolved.dedup();
        eprintln!(
            "[asuna] Maliyet hesaplanmadi: `{direction}` kiriliminda fiyatlandirilamayan \
             anahtar(lar) var: {}. Beklenen: {AUDIO_TOKENS}, {TEXT_TOKENS} \
             (fiyatlandirilamayan: {}).",
            unresolved.join(", "),
            UNPRICEABLE_TOKENS.join(", ")
        );
        return None;
    }

    if audio + text != total {
        eprintln!(
            "[asuna] Maliyet hesaplanmadi: `{direction}` kirilimi toplami aciklamiyor \
             (kirilim {}, toplam {total}).",
            audio + text
        );
        return None;
    }

    Some(TokenSplit { audio, text })
}

/// Realtime oturumunun tahmini maliyeti (USD).
///
/// `None` = **bilinmiyor**. Uc nedenden biri: model fiyat tablosunda yok,
/// token toplami raporlanmadi, ya da kirilim toplami aciklamiyor. Hicbirinde
/// yaklasik bir deger uretilmez.
pub fn estimate_realtime_cost_usd(model: &str, usage: &SessionUsage) -> Option<f64> {
    let rates = realtime_rates(model)?;

    let input = split_tokens("input", usage.input_tokens, &usage.input_token_details)?;
    let output = split_tokens("output", usage.output_tokens, &usage.output_token_details)?;

    let per_million = |count: i64, rate: f64| (count as f64) * rate / 1_000_000.0;

    let cost = per_million(input.audio, rates.audio_in)
        + per_million(input.text, rates.text_in)
        + per_million(output.audio, rates.audio_out)
        + per_million(output.text, rates.text_out);

    // Semadaki CHECK `>= 0.0` sart kosuyor; negatif/NaN bir deger yazilmaz.
    if !cost.is_finite() || cost < 0.0 {
        return None;
    }
    Some(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(
        input: Option<i64>,
        output: Option<i64>,
        input_details: Vec<serde_json::Value>,
        output_details: Vec<serde_json::Value>,
    ) -> SessionUsage {
        SessionUsage {
            requests: Some(1),
            input_tokens: input,
            output_tokens: output,
            total_tokens: Some(input.unwrap_or(0) + output.unwrap_or(0)),
            input_token_details: input_details,
            output_token_details: output_details,
        }
    }

    /// Tablodaki rakamlar voice.md Bolum 6 ile birebir. Bu test bir "kopyala
    /// yapistir dogrulugu" kapisi: fiyat degisirse dokuman da degismeli.
    #[test]
    fn rates_match_the_verified_table() {
        let full = realtime_rates("gpt-realtime-2.1").expect("model tabloda olmali");
        assert_eq!(full.audio_in, 32.00);
        assert_eq!(full.audio_out, 64.00);
        assert_eq!(full.text_in, 4.00);
        assert_eq!(full.text_out, 24.00);

        let mini = realtime_rates("gpt-realtime-2.1-mini").expect("model tabloda olmali");
        assert_eq!(mini.audio_in, 10.00);
        assert_eq!(mini.audio_out, 20.00);
    }

    /// Dogrulanmamis bir model icin **sayi uretilmez**.
    #[test]
    fn unknown_models_have_no_price() {
        assert_eq!(realtime_rates("gpt-5-realtime-hayali"), None);
        assert_eq!(
            estimate_realtime_cost_usd(
                "gpt-5-realtime-hayali",
                &usage(
                    Some(1_000),
                    Some(500),
                    vec![serde_json::json!({ "audio_tokens": 1_000 })],
                    vec![serde_json::json!({ "audio_tokens": 500 })],
                )
            ),
            None
        );
    }

    #[test]
    fn prices_a_pure_audio_session_from_the_breakdown() {
        let cost = estimate_realtime_cost_usd(
            "gpt-realtime-2.1",
            &usage(
                Some(1_000_000),
                Some(1_000_000),
                vec![serde_json::json!({ "audio_tokens": 1_000_000 })],
                vec![serde_json::json!({ "audio_tokens": 1_000_000 })],
            ),
        )
        .expect("kirilim tam, fiyat hesaplanmali");

        // 1M ses girisi ($32) + 1M ses cikisi ($64).
        assert!((cost - 96.0).abs() < 1e-9, "maliyet: {cost}");
    }

    #[test]
    fn mixes_audio_and_text_rates() {
        let cost = estimate_realtime_cost_usd(
            "gpt-realtime-2.1-mini",
            &usage(
                Some(2_000),
                Some(1_000),
                vec![
                    serde_json::json!({ "audio_tokens": 1_500 }),
                    serde_json::json!({ "text_tokens": 500 }),
                ],
                vec![serde_json::json!({ "audio_tokens": 800, "text_tokens": 200 })],
            ),
        )
        .expect("kirilim tam");

        let expected =
            1_500.0 * 10.0 / 1e6 + 500.0 * 0.60 / 1e6 + 800.0 * 20.0 / 1e6 + 200.0 * 2.40 / 1e6;
        assert!((cost - expected).abs() < 1e-12, "maliyet: {cost}");
    }

    /// Kirilim yoksa toplam token bir maliyet uretmez: ses ve metin arasinda
    /// 8 kat fark var, "ortalama" almak uydurmaktir.
    #[test]
    fn a_total_without_a_breakdown_is_not_priced() {
        assert_eq!(
            estimate_realtime_cost_usd(
                "gpt-realtime-2.1",
                &usage(Some(1_000), Some(500), Vec::new(), Vec::new())
            ),
            None
        );
    }

    /// Cache indirimi ses/metin ayrimi gerektiriyor; ayrim yoksa hesap yok.
    #[test]
    fn cached_or_image_tokens_block_the_estimate() {
        for key in ["cached_tokens", "image_tokens", "bilinmeyen_tokens"] {
            assert_eq!(
                estimate_realtime_cost_usd(
                    "gpt-realtime-2.1",
                    &usage(
                        Some(1_000),
                        Some(100),
                        vec![serde_json::json!({ "audio_tokens": 900, key: 100 })],
                        vec![serde_json::json!({ "audio_tokens": 100 })],
                    )
                ),
                None,
                "anahtar: {key}"
            );
        }
    }

    /// Kirilim toplamla tutmuyorsa (eksik ya da fazla) sessizce yuvarlanmaz.
    #[test]
    fn a_breakdown_that_does_not_add_up_is_rejected() {
        assert_eq!(
            estimate_realtime_cost_usd(
                "gpt-realtime-2.1",
                &usage(
                    Some(1_000),
                    Some(100),
                    vec![serde_json::json!({ "audio_tokens": 600 })],
                    vec![serde_json::json!({ "audio_tokens": 100 })],
                )
            ),
            None
        );
    }

    #[test]
    fn a_session_without_reported_tokens_has_no_cost() {
        assert_eq!(
            estimate_realtime_cost_usd(
                "gpt-realtime-2.1",
                &usage(None, None, Vec::new(), Vec::new())
            ),
            None
        );
    }

    /// Hic token harcanmamis oturum sifir dolar — bu bir tahmin degil, olcum.
    #[test]
    fn a_zero_token_session_costs_zero() {
        assert_eq!(
            estimate_realtime_cost_usd(
                "gpt-realtime-2.1",
                &usage(Some(0), Some(0), Vec::new(), Vec::new())
            ),
            Some(0.0)
        );
    }
}

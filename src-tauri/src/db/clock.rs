//! Zaman damgasi uretimi ve dogrulamasi (ASU-031).
//!
//! # Neden elle
//!
//! `chrono` / `time` bagimliligi bilerek eklenmedi (Cargo.toml "bagimlilik
//! politikasi"): burada gereken tek sey **tek bir bicimde** UTC ISO-8601
//! uretmek ve dogrulamak. Otuz satirlik takvim aritmetigi, sadece bunun icin
//! yeni bir bagimlilik agacindan ucuz.
//!
//! # Neden saniye hassasiyeti
//!
//! Zaman damgalari DB'de **metin** olarak durur ve siralama metin siralamasidir
//! (memory.md Bolum 4 / Stage A). Karisik hassasiyet sessizce yanlis siralama
//! uretir: `'2026-08-25T10:00:00.500Z' < '2026-08-25T10:00:00Z'` cunku `.`
//! (0x2E) karakteri `Z`'den (0x5A) kucuktur — yani salise'li bir kayit,
//! kendisinden **once** yazilmis salise'siz bir kaydin gerisine duser.
//!
//! Bu yuzden Asuna yalnizca `YYYY-MM-DDTHH:MM:SSZ` uretir. Ayni saniye icinde
//! yazilan kayitlarin sirasi `id` ile cozulur (repository'lerdeki
//! `ORDER BY ... , id DESC`). Okuma tarafi daha genis: salise'li degerler de
//! kabul edilir, cunku sema (`GLOB ... T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z`)
//! ve `src/shared/contract.ts` onlari gecerli sayar.

use std::time::{SystemTime, UNIX_EPOCH};

/// Uretilen bicimin karakter uzunlugu (`2026-08-25T10:00:00Z`).
const CANONICAL_LENGTH: usize = 20;

/// Simdi — UTC ISO-8601, saniye hassasiyetinde.
pub fn now_utc() -> String {
    format_utc(epoch_seconds())
}

/// Sistem saatinden Unix epoch saniyesi.
///
/// Saat 1970'ten geriye ayarlanmis olsa bile panic yok: `duration_since`
/// hatasi negatif saniyeye cevrilir.
fn epoch_seconds() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX),
        Err(error) => -i64::try_from(error.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

/// Unix epoch saniyesini `YYYY-MM-DDTHH:MM:SSZ` bicimine cevirir.
pub fn format_utc(epoch_seconds: i64) -> String {
    let days = epoch_seconds.div_euclid(86_400);
    let seconds_of_day = epoch_seconds.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Gregoryen takvim donusumu (Howard Hinnant, `civil_from_days`).
///
/// `days` 1970-01-01'den itibaren gun sayisi; negatif degerler de dogru calisir.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // 0000-03-01'i baslangic alan "era" aritmetigi: artik yil istisnalari
    // 400 yillik dongude tekrarlandigi icin bolme/modulo ile cozulur.
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153; // [0, 11], Mart = 0
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };

    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Deger UTC ISO-8601 mi?
///
/// Kabul edilen: `YYYY-MM-DDTHH:MM:SSZ` ve `YYYY-MM-DDTHH:MM:SS.sss…Z`.
/// Reddedilen: epoch saniyesi, yerel saat, offset'li (`+03:00`) ve bosluklu
/// bicimler — hepsi metin siralamasini sessizce bozar.
///
/// Bu kural uc yerde ayni: DB'deki `GLOB` CHECK'i, `src/shared/contract.ts`
/// icindeki regex ve burasi. Alan **degeri** hicbir hata mesajina girmez.
pub fn is_utc_iso8601(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < CANONICAL_LENGTH {
        return false;
    }

    let digits = |range: std::ops::Range<usize>| bytes[range].iter().all(u8::is_ascii_digit);
    if !digits(0..4) || !digits(5..7) || !digits(8..10) {
        return false;
    }
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return false;
    }
    if !digits(11..13) || !digits(14..16) || !digits(17..19) {
        return false;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return false;
    }

    // Alan araliklari: gecersiz bir tarih (13. ay, 32. gun) sessizce kabul
    // edilmemeli — sonradan "hangi hafiza once geldi" sorusunu bozar.
    let number =
        |range: std::ops::Range<usize>| -> u32 { value[range].parse::<u32>().unwrap_or(u32::MAX) };
    if !(1..=12).contains(&number(5..7)) || !(1..=31).contains(&number(8..10)) {
        return false;
    }
    if number(11..13) > 23 || number(14..16) > 59 || number(17..19) > 59 {
        return false;
    }

    match &bytes[19..] {
        [b'Z'] => true,
        [b'.', rest @ ..] => {
            // `.` sonrasi en az bir rakam ve tam olarak bir `Z`.
            matches!(rest.split_last(), Some((b'Z', fraction))
                if !fraction.is_empty() && fraction.iter().all(u8::is_ascii_digit))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_unix_epoch() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn formats_known_instants() {
        // 2026-08-25T10:00:00Z
        assert_eq!(format_utc(1_787_652_000), "2026-08-25T10:00:00Z");
        // Artik gun.
        assert_eq!(format_utc(1_709_164_800), "2024-02-29T00:00:00Z");
        // Yil sonu sinirinin bir saniye oncesi.
        assert_eq!(format_utc(1_798_761_599), "2026-12-31T23:59:59Z");
    }

    /// 1970 oncesi (saat geriye ayarlanmis makine) panic uretmemeli.
    #[test]
    fn formats_instants_before_the_epoch() {
        assert_eq!(format_utc(-1), "1969-12-31T23:59:59Z");
        assert_eq!(format_utc(-86_400), "1969-12-31T00:00:00Z");
    }

    /// Uretilen her deger kendi dogrulayicisindan gecmeli ve **tek** bicimde
    /// olmali — aksi halde metin siralamasi bozulur.
    #[test]
    fn generated_timestamps_are_canonical() {
        let now = now_utc();
        assert_eq!(now.len(), CANONICAL_LENGTH, "beklenmeyen bicim: {now}");
        assert!(now.ends_with('Z'));
        assert!(!now.contains('.'), "salise yazilmamali: {now}");
        assert!(is_utc_iso8601(&now));
    }

    /// Ayni bicimdeki iki damga metin olarak dogru siralanir (Stage A'nin
    /// dayandigi ozellik).
    #[test]
    fn canonical_timestamps_sort_chronologically_as_text() {
        let earlier = format_utc(1_787_652_000);
        let later = format_utc(1_787_652_001);
        assert!(earlier < later);
    }

    #[test]
    fn accepts_valid_timestamps() {
        for value in [
            "2026-08-25T10:00:00Z",
            "2026-08-25T10:00:00.123Z",
            "1970-01-01T00:00:00Z",
            "2026-12-31T23:59:59.999999Z",
        ] {
            assert!(is_utc_iso8601(value), "reddedildi: {value}");
        }
    }

    #[test]
    fn rejects_non_utc_or_malformed_timestamps() {
        for value in [
            "",
            "1756108800",
            "2026-08-25 10:00:00Z",
            "2026-08-25T10:00:00+03:00",
            "2026-08-25T10:00:00",
            "2026-08-25",
            "2026-08-25T10:00:00z",
            "2026-08-25T10:00:00.Z",
            "2026-08-25T10:00:00.12x3Z",
            "2026-13-25T10:00:00Z",
            "2026-08-32T10:00:00Z",
            "2026-08-25T24:00:00Z",
            "2026-08-25T10:60:00Z",
            "2026-08-25T10:00:60Z",
            "2026-08-25T10:00:00ZZ",
        ] {
            assert!(!is_utc_iso8601(value), "kabul edildi: {value}");
        }
    }
}

//! Hafiza/oturum deposunun **IPC'ye giden** tipli hatasi (ASU-031).
//!
//! # Sozlesme
//!
//! - Hicbir varyantin mesaji SQL sorgusu, dosya yolu ya da **kullanici icerigi**
//!   tasimaz. Bir hafiza kaydinin icerigi kullanicinin en mahrem verisi olabilir
//!   ve hata mesajlari hem log'a hem UI'a duser (`conventions.md` "Hata Yonetimi").
//!   [`StoreError::Invalid`] yalnizca **alan adi + beklenen bicim** soyler.
//! - Sessiz yutma yok: depolama hatasinin tam zinciri yerel log'a yazilir
//!   (`describe_error_chain`), IPC'ye yalnizca kisa neden gider.
//! - Renderer'in gordugu bicim `{ "code": ..., "message": ... }`; TypeScript
//!   aynasi `src/shared/store-error.ts`.
//!
//! # `disabled` neden burada yok
//!
//! `ASUNA_MEMORY_ENABLED=false` bir **hata degildir** (PROJECT.md Bolum 20:
//! kullanicinin karari). Yazma no-op olur, okuma bos doner ve komut `Ok(...)`
//! icinde [`StoreSkipReason`] ile bunu **acikca** bildirir — renderer "kaydettim"
//! sanmaz. `Unavailable` ise arizadir ve hata olarak doner.

use serde::{Serialize, Serializer};
use thiserror::Error;

use super::{describe_error_chain, AsunaDb, DbError, DbState};

/// Renderer'in ayirt etmesi gereken hata sinifi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreErrorCode {
    /// Girdi sema/domain dogrulamasindan gecmedi. DB'ye hic dokunulmadi.
    Invalid,
    /// Verilen id ile kayit yok (silinmis olabilir).
    NotFound,
    /// Hafiza alt sistemi arizali (`DbState::Unavailable`). Kapali degil, **bozuk**.
    Unavailable,
    /// DB islemi basarisiz (disk, kilit, kisit ihlali).
    Storage,
}

/// Depolama katmani hatasi.
#[derive(Debug, Error)]
pub enum StoreError {
    /// `detail` bu kod tarafindan yazilir; disaridan gelen bir deger **icermez**.
    #[error("{detail}")]
    Invalid { detail: String },

    #[error("kayit bulunamadi")]
    NotFound,

    /// `reason` [`DbState::Unavailable`]'dan gelir ve zaten redakte edilmistir.
    #[error("hafiza kullanilamiyor: {reason}")]
    Unavailable { reason: String },

    #[error("veritabani islemi basarisiz")]
    Storage(#[source] DbError),
}

impl StoreError {
    /// Dogrulama hatasi. `detail` yalnizca alan adi ve beklenen bicimi anlatir.
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::Invalid {
            detail: detail.into(),
        }
    }

    /// DB hatasini IPC'ye uygun hale getirir ve **tam zinciri yerel log'a** yazar.
    ///
    /// `operation` sabit bir etikettir (`"memory_create"` gibi); kullanici
    /// verisi ya da SQL icermez.
    pub fn storage(error: DbError, operation: &'static str) -> Self {
        eprintln!(
            "[asuna] `{operation}` basarisiz: {}",
            describe_error_chain(&error)
        );
        Self::Storage(error)
    }

    pub fn code(&self) -> StoreErrorCode {
        match self {
            Self::Invalid { .. } => StoreErrorCode::Invalid,
            Self::NotFound => StoreErrorCode::NotFound,
            Self::Unavailable { .. } => StoreErrorCode::Unavailable,
            Self::Storage(_) => StoreErrorCode::Storage,
        }
    }
}

/// Renderer'a giden bicim. `#[tauri::command]` donus tipi `Result<_, StoreError>`
/// olabilsin diye `Serialize` elle yazildi (varyant icerigi degil, kod + mesaj gider).
impl Serialize for StoreError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            code: StoreErrorCode,
            message: &'a str,
        }

        let message = self.to_string();
        Wire {
            code: self.code(),
            message: &message,
        }
        .serialize(serializer)
    }
}

/// Yazma isleminin **neden** atlandigi. Hata degil, bilinen bir urun durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreSkipReason {
    /// `ASUNA_MEMORY_ENABLED=false` — DB dosyasi hic acilmadi.
    MemoryDisabled,
}

/// Komut katmani icin DB erisimi.
///
/// - `Ok(Some(db))` — hafiza acik.
/// - `Ok(None)` — kullanici kapatti; cagiran no-op/bos donmeli.
/// - `Err(Unavailable)` — ariza; **sessizce bos donulmez**, kullanici hafizasinin
///   neden calismadigini bilmeli (PROJECT.md Bolum 30).
pub fn database(state: &DbState) -> Result<Option<&AsunaDb>, StoreError> {
    state.access().map_err(|reason| StoreError::Unavailable {
        reason: reason.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_code_and_message() {
        let json = serde_json::to_value(StoreError::invalid("`title` bos birakilamaz"))
            .expect("serialize");
        assert_eq!(json["code"], "invalid");
        assert_eq!(json["message"], "`title` bos birakilamaz");

        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["code", "message"]);
    }

    #[test]
    fn every_variant_has_a_stable_code() {
        assert_eq!(StoreError::NotFound.code(), StoreErrorCode::NotFound);
        assert_eq!(
            StoreError::Unavailable {
                reason: "sema migration'lari uygulanamadi".to_owned()
            }
            .code(),
            StoreErrorCode::Unavailable
        );
        assert_eq!(
            StoreError::Storage(DbError::Poisoned).code(),
            StoreErrorCode::Storage
        );
    }

    /// Depolama hatasi renderer'a **detay** sizdirmaz: SQL, yol ya da icerik yok.
    #[test]
    fn storage_errors_do_not_leak_details_over_ipc() {
        let error = StoreError::Storage(DbError::Query(rusqlite::Error::InvalidQuery));
        let json = serde_json::to_value(&error).expect("serialize");
        let message = json["message"].as_str().expect("mesaj");

        assert_eq!(message, "veritabani islemi basarisiz");
        assert!(!message.to_lowercase().contains("select"));
        assert!(!message.contains(".db"));
    }

    #[test]
    fn state_access_separates_disabled_from_unavailable() {
        assert!(database(&DbState::Disabled)
            .expect("kapali hafiza hata degil")
            .is_none());

        let error = database(&DbState::Unavailable {
            reason: "veritabani dosyasi acilamadi".to_owned(),
        })
        .expect_err("ariza hata olarak donmeli");
        assert_eq!(error.code(), StoreErrorCode::Unavailable);
        assert!(error.to_string().contains("veritabani dosyasi acilamadi"));
    }

    #[test]
    fn skip_reason_is_explicit_on_the_wire() {
        assert_eq!(
            serde_json::to_value(StoreSkipReason::MemoryDisabled).expect("serialize"),
            "memory-disabled"
        );
    }
}

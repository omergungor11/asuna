//! Calisma zamani gizlilik durumu (ASU-037).
//!
//! # Neden ayri bir durum
//!
//! `ASUNA_MEMORY_ENABLED` / `ASUNA_TRANSCRIPT_STORAGE` **acilis** kaynagidir:
//! `.env` okunur, [`crate::config::AsunaConfig`] icinde donar ve bir daha
//! degismez. Ama gizlilik bir "kurulum secenegi" degil, gunluk bir karardir —
//! kullanici hassas bir konusmaya baslamadan once hafizayi kapatabilmeli ve
//! bunun icin uygulamayi yeniden baslatmak zorunda kalmamali (ASU-037 kabul
//! kriteri: "Degisiklikler yeniden baslatmadan etkili").
//!
//! Bu modul o kararin **tek** yasadigi yer. Yazma yollari artik config'in ham
//! alanina degil buraya bakar; `.env` dosyasina **hicbir sey yazilmaz** (dosyayi
//! kullanici yonetir, uygulama degil).
//!
//! # Tek yonlu kural: calisma zamani yalnizca **sikilastirir**
//!
//! Acilista kapali olan bir sey calisma zamaninda acilamaz. Bu bir kolaylik
//! kisiti degil, kaynaklarin acilis anindaki gercegi:
//!
//! - `ASUNA_MEMORY_ENABLED=false` ise SQLite dosyasi **hic acilmaz**
//!   ([`crate::db::DbState::Disabled`]); acilmamis bir DB'ye calisma zamaninda
//!   yazilamaz.
//! - `ASUNA_TRANSCRIPT_STORAGE=false` ise transcript yazma yolu acilista
//!   kapatilmistir; ayrica renderer transkripsiyonu hic acmaz, yani yazilacak
//!   metin uretilmez bile (voice.md Bolum 2).
//!
//! Dolayisiyla [`PrivacyState::apply`] gevsetme istegini sessizce kabul edip
//! yalan bir "acik" gostermek yerine [`PrivacyError::LockedByEnv`] ile
//! reddeder. UI de ayni bilgiyi onceden gosterir ([`PrivacySettings`] icindeki
//! `*_at_boot` alanlari) — anahtar kilitli cizilir, kullanici tiklamadan once
//! nedenini okur.
//!
//! # Process genelinde okuma
//!
//! Komutlar durumu Tauri `State` ile alir (test izolasyonu icin dogru yol).
//! Ama transcript yazimi gibi bazi yollar komut degildir ve `State` goremez;
//! onlar icin acilista [`install_process_state`] ile ayni `Arc` process
//! genelinde de kaydedilir. Kurulmadiysa (birim testleri) okuyucular
//! "kisitlama yok" dondurur — testler gercek kullanici durumuna bagli olmaz.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize, Serializer};
use tauri::State;
use thiserror::Error;

use crate::config::{KEY_MEMORY_ENABLED, KEY_TRANSCRIPT_STORAGE};

/// Gizlilik anahtarlarinin calisma zamanindaki hali.
///
/// Alanlar `AtomicBool`: bir bayragi okumak icin kilit almak, ses yolundaki her
/// yazma denemesini gereksizce serilestirirdi. `Ordering::Relaxed` yeterli —
/// tek bir bayragin son degeri okunuyor, baska bir veriye sirali bagimlilik yok.
#[derive(Debug)]
pub struct PrivacyState {
    memory_enabled_at_boot: bool,
    transcript_storage_at_boot: bool,
    memory_enabled: AtomicBool,
    transcript_storage: AtomicBool,
}

impl PrivacyState {
    /// Acilis degerlerinden kurar. Kaynak env'dir; calisma zamani buradan baslar.
    pub fn from_boot(memory_enabled: bool, transcript_storage: bool) -> Self {
        Self {
            memory_enabled_at_boot: memory_enabled,
            transcript_storage_at_boot: transcript_storage,
            memory_enabled: AtomicBool::new(memory_enabled),
            transcript_storage: AtomicBool::new(transcript_storage),
        }
    }

    /// Kalici hafizaya **yeni** kayit yazilabilir mi?
    ///
    /// Silme/arsivleme bu bayraga bakmaz: onlar "daha az hatirla" yonundedir ve
    /// hafiza kapaliyken de kullanilabilmeli (bkz. `memory_repository`).
    pub fn memory_enabled(&self) -> bool {
        self.memory_enabled.load(Ordering::Relaxed)
    }

    /// Konusma dokumu diske yazilabilir mi?
    pub fn transcript_storage(&self) -> bool {
        self.transcript_storage.load(Ordering::Relaxed)
    }

    pub fn settings(&self) -> PrivacySettings {
        PrivacySettings {
            memory_enabled: self.memory_enabled(),
            transcript_storage: self.transcript_storage(),
            memory_enabled_at_boot: self.memory_enabled_at_boot,
            transcript_storage_at_boot: self.transcript_storage_at_boot,
        }
    }

    /// Verilen alanlari uygular ve yeni durumu dondurur.
    ///
    /// **Ya hepsi ya hicbiri**: gecersiz (gevsetme) bir istek varsa hicbir alan
    /// yazilmaz. Yarim uygulanmis bir gizlilik ayari, UI'da gosterilenle diskte
    /// olani ayirir — tam olarak kacinilmasi gereken durum.
    pub fn apply(&self, patch: &PrivacyPatch) -> Result<PrivacySettings, PrivacyError> {
        if patch.memory_enabled == Some(true) && !self.memory_enabled_at_boot {
            return Err(PrivacyError::LockedByEnv {
                key: KEY_MEMORY_ENABLED,
            });
        }
        if patch.transcript_storage == Some(true) && !self.transcript_storage_at_boot {
            return Err(PrivacyError::LockedByEnv {
                key: KEY_TRANSCRIPT_STORAGE,
            });
        }

        if let Some(value) = patch.memory_enabled {
            self.memory_enabled.store(value, Ordering::Relaxed);
        }
        if let Some(value) = patch.transcript_storage {
            self.transcript_storage.store(value, Ordering::Relaxed);
        }

        Ok(self.settings())
    }
}

/// Renderer'a giden gizlilik durumu (`serde` camelCase).
///
/// `*_at_boot` alanlari bilerek gonderiliyor: UI "bu anahtar neden kilitli?"
/// sorusunu **sormadan** cevaplayabilsin ve boot kaynaginin `.env` oldugunu
/// yazabilsin. Bu alanlar bir secret degil, kullanicinin kendi ayari.
///
/// TypeScript aynasi: `src/shared/privacy.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacySettings {
    /// Calisma zamanindaki etkin deger.
    pub memory_enabled: bool,
    pub transcript_storage: bool,
    /// Acilista env'den gelen deger — tavan. Calisma zamani bunun uzerine cikamaz.
    pub memory_enabled_at_boot: bool,
    pub transcript_storage_at_boot: bool,
}

/// Kismi guncelleme: verilmeyen alan **dokunulmaz**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyPatch {
    #[serde(default)]
    pub memory_enabled: Option<bool>,
    #[serde(default)]
    pub transcript_storage: Option<bool>,
}

/// Gizlilik ayari degistirilemedi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PrivacyError {
    /// Acilista env ile kapatilmis bir anahtar calisma zamaninda acilamaz.
    #[error(
        "`{key}` acilista kapatilmis; calisma zamaninda acilamaz. \
         Acmak icin `.env` dosyasini duzenleyip Asuna'yi yeniden baslatin."
    )]
    LockedByEnv { key: &'static str },
}

impl PrivacyError {
    /// Renderer'in ayirt ettigi kod (`src/shared/privacy.ts`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::LockedByEnv { .. } => "locked-by-env",
        }
    }
}

/// `{ code, message }` — `StoreError` ile ayni tel bicimi, ayni gerekce:
/// renderer hatayi `Error` sanip `message`'a bakmasin, koda baksin.
impl Serialize for PrivacyError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            code: &'a str,
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

// ---------------------------------------------------------------------------
// Process genelinde erisim (komut olmayan yazma yollari icin)
// ---------------------------------------------------------------------------

static PROCESS_STATE: OnceLock<Arc<PrivacyState>> = OnceLock::new();

/// Acilista **bir kez** cagrilir (`lib.rs`). Ikinci cagri yok sayilir.
///
/// Neden `OnceLock`: durumun sahibi Tauri `State`; bu yalnizca ayni `Arc`'a
/// ikinci bir okuma penceresi. Yeniden kurulabilir olsaydi bir test ya da kod
/// yolu kullanicinin ayarini sessizce degistirebilirdi.
pub fn install_process_state(state: Arc<PrivacyState>) {
    let _ = PROCESS_STATE.set(state);
}

/// Kalici hafiza yazimi acik mi? Durum kurulmamissa `true` (kisitlama yok).
pub fn process_memory_enabled() -> bool {
    PROCESS_STATE
        .get()
        .is_none_or(|state| state.memory_enabled())
}

/// Transcript diske yazilabilir mi? Durum kurulmamissa `true` (kisitlama yok).
///
/// Varsayilan neden `true`: bu okuma bir **ikinci** kapidir. Birinci kapi
/// acilis degeridir ve zaten cagiran tarafta duruyor; burada `false` varsaymak
/// yalnizca birim testlerinde yazmayi imkansiz kilardi, gercek bir koruma
/// eklemezdi.
pub fn process_transcript_storage() -> bool {
    PROCESS_STATE
        .get()
        .is_none_or(|state| state.transcript_storage())
}

// ---------------------------------------------------------------------------
// Komutlar
// ---------------------------------------------------------------------------

/// Guncel gizlilik durumu. Secret icermez; yalnizca kullanicinin kendi ayarlari.
#[tauri::command]
pub fn get_privacy_settings(state: State<'_, Arc<PrivacyState>>) -> PrivacySettings {
    state.settings()
}

/// Gizlilik anahtarlarini calisma zamaninda degistirir.
///
/// `.env` dosyasina **yazmaz**: kalicilik kullanicinin kendi dosyasindadir,
/// uygulama onu sessizce duzenlemez (ASU-037 karari).
#[tauri::command]
pub fn set_privacy_settings(
    state: State<'_, Arc<PrivacyState>>,
    patch: PrivacyPatch,
) -> Result<PrivacySettings, PrivacyError> {
    state.apply(&patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> PrivacyState {
        PrivacyState::from_boot(true, true)
    }

    #[test]
    fn starts_from_the_boot_values() {
        let state = PrivacyState::from_boot(true, false);
        let settings = state.settings();

        assert!(settings.memory_enabled);
        assert!(!settings.transcript_storage);
        assert!(settings.memory_enabled_at_boot);
        assert!(!settings.transcript_storage_at_boot);
    }

    /// **ASU-037 kabul kriteri**: degisiklik yeniden baslatmadan etkili.
    #[test]
    fn turning_a_switch_off_takes_effect_immediately() {
        let state = state();

        let settings = state
            .apply(&PrivacyPatch {
                memory_enabled: Some(false),
                transcript_storage: None,
            })
            .expect("kapatmak her zaman kabul edilir");

        assert!(!settings.memory_enabled);
        assert!(!state.memory_enabled(), "yazma yolu hala acik goruyor");
        // Dokunulmayan alan degismedi.
        assert!(state.transcript_storage());
        // Acilis degeri korunur: "kapali" ile "env'de kapaliydi" ayri seyler.
        assert!(settings.memory_enabled_at_boot);
    }

    #[test]
    fn a_switch_can_be_turned_back_on_when_boot_allowed_it() {
        let state = state();

        state
            .apply(&PrivacyPatch {
                memory_enabled: Some(false),
                transcript_storage: Some(false),
            })
            .expect("kapatma");
        let settings = state
            .apply(&PrivacyPatch {
                memory_enabled: Some(true),
                transcript_storage: Some(true),
            })
            .expect("acilista aciksa geri acilabilir");

        assert!(settings.memory_enabled && settings.transcript_storage);
    }

    /// Acilista kapali olan bir anahtar calisma zamaninda **acilamaz**: DB
    /// dosyasi hic acilmadi, transcript yolu hic kurulmadi.
    #[test]
    fn a_switch_disabled_at_boot_cannot_be_loosened_at_runtime() {
        let state = PrivacyState::from_boot(false, false);

        let error = state
            .apply(&PrivacyPatch {
                memory_enabled: Some(true),
                transcript_storage: None,
            })
            .expect_err("gevsetme reddedilmeli");
        assert_eq!(error.code(), "locked-by-env");
        assert!(error.to_string().contains(KEY_MEMORY_ENABLED));

        let error = state
            .apply(&PrivacyPatch {
                memory_enabled: None,
                transcript_storage: Some(true),
            })
            .expect_err("gevsetme reddedilmeli");
        assert!(error.to_string().contains(KEY_TRANSCRIPT_STORAGE));

        // Yine de kapali kaldi — reddedilen istek durumu bozmadi.
        assert!(!state.memory_enabled() && !state.transcript_storage());
    }

    /// Reddedilen bir istekteki **gecerli** alan da uygulanmaz: yarim uygulanmis
    /// gizlilik ayari UI ile diski ayirir.
    #[test]
    fn a_rejected_patch_applies_none_of_its_fields() {
        let state = PrivacyState::from_boot(true, false);

        state
            .apply(&PrivacyPatch {
                memory_enabled: Some(false),
                transcript_storage: Some(true),
            })
            .expect_err("transcript gevsetmesi reddedilmeli");

        assert!(state.memory_enabled(), "gecerli alan sessizce uygulanmis");
    }

    #[test]
    fn settings_serialize_with_the_expected_contract() {
        let json = serde_json::to_value(PrivacyState::from_boot(true, false).settings())
            .expect("serialize");
        let object = json.as_object().expect("JSON nesnesi");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            [
                "memoryEnabled",
                "memoryEnabledAtBoot",
                "transcriptStorage",
                "transcriptStorageAtBoot"
            ]
        );
        assert_eq!(json["memoryEnabled"], true);
        assert_eq!(json["transcriptStorage"], false);
    }

    #[test]
    fn errors_serialize_as_code_and_message() {
        let json = serde_json::to_value(PrivacyError::LockedByEnv {
            key: KEY_MEMORY_ENABLED,
        })
        .expect("serialize");

        assert_eq!(json["code"], "locked-by-env");
        assert!(json["message"]
            .as_str()
            .expect("mesaj")
            .contains("yeniden baslatin"));
    }

    /// IPC sinirinda bilinmeyen alan yutulmaz; bos govde "dokunma" demektir.
    #[test]
    fn unknown_patch_fields_are_rejected_at_the_ipc_boundary() {
        assert!(serde_json::from_str::<PrivacyPatch>(r#"{"memoryEnabled":false}"#).is_ok());
        assert_eq!(
            serde_json::from_str::<PrivacyPatch>("{}").expect("bos govde"),
            PrivacyPatch::default()
        );
        assert!(serde_json::from_str::<PrivacyPatch>(r#"{"memory_enabled":false}"#).is_err());
        assert!(serde_json::from_str::<PrivacyPatch>(r#"{"envPath":"/etc/passwd"}"#).is_err());
    }

    /// Process durumu kurulmadiginda okuyucular kisitlama uydurmaz.
    ///
    /// NOT: `install_process_state` bilerek **cagrilmiyor** — `OnceLock` geri
    /// alinamaz ve bu dosyadaki bir kurulum, ayni process'te kosan transcript
    /// testlerini etkilerdi. Kurulu halin davranisi `PrivacyState` uzerinden
    /// yukaridaki testlerle olculuyor.
    #[test]
    fn process_readers_default_to_unrestricted_when_not_installed() {
        assert!(process_memory_enabled());
        assert!(process_transcript_storage());
    }
}

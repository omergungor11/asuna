//! Webview'e acilan Tauri komutlari.
//!
//! Her komut ayri bir yetki yuzeyidir: `build.rs` icindeki `AppManifest::commands`
//! listesine ve `capabilities/` altindaki bir capability dosyasina **acikca**
//! eklenmeden renderer tarafindan cagrilamaz (ACL deny-by-default).

use tauri::State;

use crate::config::{AsunaConfig, FrontendConfig};

/// Renderer'in gorebilecegi config alt kumesini dondurur (whitelist).
///
/// GUVENLIK: donus tipi [`FrontendConfig`] — `AsunaConfig` degil. `AsunaConfig`
/// `Serialize` turetmedigi icin `OPENAI_API_KEY`'i buradan dondurmek derleme
/// hatasi olur; whitelist derleyici tarafindan zorlanir.
#[tauri::command]
pub fn get_frontend_config(config: State<'_, AsunaConfig>) -> FrontendConfig {
    config.to_frontend()
}

/// Renderer'a acilan komutlarin tam listesi. `build.rs` ve
/// `capabilities/asuna-config.json` ile birebir ayni kalmali.
#[cfg(test)]
pub const EXPOSED_COMMANDS: [&str; 1] = ["get_frontend_config"];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Komut listesi ile capability dosyasi birbirinden kayarsa, komut ya
    /// erisilemez olur ya da yetkisi olmadan acilir. Ikisi de sessiz hata —
    /// burada gurultulu hale getiriliyor.
    #[test]
    fn every_exposed_command_has_a_capability_entry() {
        let capability = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/asuna-config.json"),
        )
        .expect("capability dosyasi okunabilmeli");

        for command in EXPOSED_COMMANDS {
            assert!(
                capability.contains(&format!("allow-{}", command.replace('_', "-"))),
                "`{command}` icin capability izni yok"
            );
        }
    }

    /// Komut listesi `build.rs`'teki ACL manifest'i ile ayni olmali; aksi halde
    /// komut ACL'e hic girmez ve capability kaydi anlamsizlasir.
    #[test]
    fn every_exposed_command_is_declared_in_the_app_manifest() {
        let build_rs =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
                .expect("build.rs okunabilmeli");

        for command in EXPOSED_COMMANDS {
            assert!(
                build_rs.contains(&format!("\"{command}\"")),
                "`{command}` build.rs icindeki AppManifest listesinde yok"
            );
        }
    }
}

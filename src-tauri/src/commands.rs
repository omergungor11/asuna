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

/// Renderer'a acilan komutlarin tam listesi — modulu ne olursa olsun.
///
/// Bu liste tek kayit noktasidir; asagidaki testler onu `build.rs`
/// (ACL manifest), `capabilities/*.json` (izin kaydi + tauri.conf.json'da
/// etkinlestirme) ve `lib.rs` (`generate_handler!`) ile karsilastirir. Yeni bir
/// `#[tauri::command]` eklerken dort yerin hepsi guncellenmeli.
#[cfg(test)]
pub const EXPOSED_COMMANDS: [&str; 2] = ["get_frontend_config", "mint_realtime_token"];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn manifest_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }

    fn read(relative: &str) -> String {
        std::fs::read_to_string(manifest_dir().join(relative))
            .unwrap_or_else(|error| panic!("`{relative}` okunabilmeli: {error}"))
    }

    /// `capabilities/` altindaki tum JSON dosyalari (ad, icerik).
    fn all_capability_files() -> Vec<(String, String)> {
        let mut files = Vec::new();
        let dir = std::fs::read_dir(manifest_dir().join("capabilities"))
            .expect("capabilities dizini okunabilmeli");
        for entry in dir {
            let path = entry.expect("dizin girdisi").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("dosya adi")
                .to_owned();
            let content = std::fs::read_to_string(&path).expect("capability dosyasi okunabilmeli");
            files.push((name, content));
        }
        assert!(!files.is_empty(), "capabilities dizini bos olmamali");
        files
    }

    /// Komut listesi ile capability dosyalari birbirinden kayarsa, komut ya
    /// erisilemez olur ya da yetkisi olmadan acilir. Ikisi de sessiz hata —
    /// burada gurultulu hale getiriliyor.
    #[test]
    fn every_exposed_command_has_a_capability_entry() {
        let files = all_capability_files();
        for command in EXPOSED_COMMANDS {
            let permission = format!("allow-{}", command.replace('_', "-"));
            assert!(
                files
                    .iter()
                    .any(|(_, content)| content.contains(&permission)),
                "`{command}` icin hicbir capability dosyasinda `{permission}` izni yok"
            );
        }
    }

    /// ACL'de izinli ama `generate_handler!` icinde kayitli olmayan bir komut
    /// runtime'da "command not found" verir — derleme zamani yakalanmaz.
    #[test]
    fn every_exposed_command_is_registered_in_the_invoke_handler() {
        let lib_rs = read("src/lib.rs");
        for command in EXPOSED_COMMANDS {
            assert!(
                lib_rs.contains(command),
                "`{command}` lib.rs icindeki generate_handler! listesinde yok"
            );
        }
    }

    /// ADR-005 tuzagi: capability dosyasi olusturulup `tauri.conf.json` icindeki
    /// `app.security.capabilities` dizisine eklenmezse dosya sessizce yok
    /// sayilir ve komut deny-by-default ile reddedilir.
    #[test]
    fn every_capability_file_is_enabled_in_tauri_conf() {
        let conf: serde_json::Value =
            serde_json::from_str(&read("tauri.conf.json")).expect("tauri.conf.json gecerli JSON");
        let enabled = conf["app"]["security"]["capabilities"]
            .as_array()
            .expect("app.security.capabilities dizisi olmali");
        let enabled: Vec<&str> = enabled.iter().filter_map(|item| item.as_str()).collect();

        for (file_name, content) in all_capability_files() {
            let capability: serde_json::Value = serde_json::from_str(&content)
                .unwrap_or_else(|error| panic!("`{file_name}` gecerli JSON degil: {error}"));
            let identifier = capability["identifier"]
                .as_str()
                .unwrap_or_else(|| panic!("`{file_name}` icinde `identifier` yok"));
            assert!(
                enabled.contains(&identifier),
                "`{file_name}` (`{identifier}`) tauri.conf.json capabilities dizisinde yok"
            );
        }
    }

    /// Capability'ler tek komut acar; wildcard izin yok.
    #[test]
    fn capabilities_never_use_wildcard_permissions() {
        for (file_name, content) in all_capability_files() {
            assert!(
                !content.contains('*'),
                "`{file_name}` wildcard izin iceriyor"
            );
        }
    }

    /// Komut listesi `build.rs`'teki ACL manifest'i ile ayni olmali; aksi halde
    /// komut ACL'e hic girmez ve capability kaydi anlamsizlasir.
    #[test]
    fn every_exposed_command_is_declared_in_the_app_manifest() {
        let build_rs = read("build.rs");

        for command in EXPOSED_COMMANDS {
            assert!(
                build_rs.contains(&format!("\"{command}\"")),
                "`{command}` build.rs icindeki AppManifest listesinde yok"
            );
        }
    }
}

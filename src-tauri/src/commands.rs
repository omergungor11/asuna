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
pub const EXPOSED_COMMANDS: [&str; 17] = [
    "get_frontend_config",
    "mint_realtime_token",
    "db_status",
    "memory_list",
    "get_bootstrap_context",
    "memory_create",
    "memory_update",
    "memory_archive",
    "memory_delete",
    "memory_delete_all",
    "session_start",
    "session_finalize",
    "session_list",
    "session_delete",
    "session_clear_all",
    "get_privacy_settings",
    "set_privacy_settings",
];

/// Hafizayi **okuyan** komutlar (ASU-031, ASU-035).
///
/// Okuma ve yazma ayri capability dosyalarinda tutulur; asagidaki test ikisinin
/// birbirine karismadigini dogrular. Karisirsa "salt okunur hafiza" diye bir
/// mod kalmaz: yazma iznini kaldirmak okumayi da kapatir ya da tam tersi,
/// okuma izni sessizce silme yetkisi tasir.
///
/// `get_bootstrap_context` adi `memory_` ile baslamiyor ama **hafiza okumasi**:
/// Stage A retrieval'in ciktisi hafiza kayitlarindan olusur, dolayisiyla okuma
/// capability'sinde durur.
#[cfg(test)]
pub const MEMORY_READ_COMMANDS: [&str; 2] = ["memory_list", "get_bootstrap_context"];

/// Hafizayi **degistiren** komutlar (ASU-031, ASU-037).
///
/// `memory_delete_all` de buradadir: ayri bir "silme" capability'si acmak,
/// yazma yetkisi kaldirilmis bir kurulumda toplu silmeyi acik birakirdi.
#[cfg(test)]
pub const MEMORY_WRITE_COMMANDS: [&str; 5] = [
    "memory_create",
    "memory_update",
    "memory_archive",
    "memory_delete",
    "memory_delete_all",
];

/// Oturum gecmisini **okuyan** komutlar (ASU-065).
///
/// Hafiza okuma capability'sine (`asuna-memory-read`) konmadi: oturum kaydi ile
/// durable memory farkli katmanlar (PROJECT.md Bolum 14) ve izinleri de ayri
/// (ASU-032 karari). Ayni gerekce okuma tarafinda da gecerli — "oturum
/// gecmisini goster" yetkisini kapatmak, hafiza listesini kapatmak zorunda
/// birakmamali.
#[cfg(test)]
pub const SESSION_READ_COMMANDS: [&str; 1] = ["session_list"];

/// Oturum kaydini **degistiren** komutlar (ASU-032, ASU-065).
///
/// Silme burada, ayri bir "temizlik" capability'sinde degil: `memory_delete_all`
/// ile ayni gerekce (ASU-037). Kaydi acan yetki kaldirilmis bir kurulumda toplu
/// temizligi acik birakmak, kapali sanilan bir yuzey uretirdi. Kullanicinin
/// **calisma zamani** anahtari ise silmeyi engellemez — o ayri bir eksen.
#[cfg(test)]
pub const SESSION_WRITE_COMMANDS: [&str; 4] = [
    "session_start",
    "session_finalize",
    "session_delete",
    "session_clear_all",
];

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

    /// `permissions/autogenerated/` altindaki `.toml` dosyalari (ad, icerik).
    ///
    /// Bu dizin `build.rs` tarafindan uretilir ve **bu test kosarken zaten
    /// uretilmis olur** (test binary'si build.rs'ten sonra derlenir).
    fn all_autogenerated_permission_files() -> Vec<(String, String)> {
        let dir_path = manifest_dir().join("permissions").join("autogenerated");
        let dir = std::fs::read_dir(&dir_path).unwrap_or_else(|error| {
            panic!(
                "`permissions/autogenerated/` okunabilmeli (build.rs uretir): {error}. \
                 Dizin yoksa ACL manifest'i hic uretilmemis demektir."
            )
        });

        let mut files = Vec::new();
        for entry in dir {
            let path = entry.expect("dizin girdisi").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .expect("dosya adi")
                .to_owned();
            let content = std::fs::read_to_string(&path).expect("izin dosyasi okunabilmeli");
            files.push((name, content));
        }
        files
    }

    /// ADR-005 "Etkiler": `src-tauri/permissions/` dizini var oldugu andan
    /// itibaren **uygulama komutlarinin tamami** ACL'e tabidir. Asuna'da bu
    /// gecis ASU-009'da (`build.rs` icindeki acik `AppManifest`) yapildi;
    /// dolayisiyla yeni bir DB komutu eklemek var olan komutlarin izinlerini
    /// gecersiz kilmaz — ama **her** komutun izin dosyasi uretilmis olmali.
    ///
    /// Bu test onun kanitidir: eksik bir izin dosyasi = sessizce reddedilen
    /// komut = calisan uygulamanin kirilmasi.
    #[test]
    fn every_exposed_command_has_an_autogenerated_acl_permission() {
        let files = all_autogenerated_permission_files();

        for command in EXPOSED_COMMANDS {
            let (_, content) = files
                .iter()
                .find(|(name, _)| name == command)
                .unwrap_or_else(|| {
                    panic!(
                        "`{command}` icin `permissions/autogenerated/{command}.toml` yok — \
                         komut ACL'e girmemis, renderer'dan cagrilamaz"
                    )
                });

            let allow = format!("allow-{}", command.replace('_', "-"));
            let deny = format!("deny-{}", command.replace('_', "-"));
            assert!(
                content.contains(&allow) && content.contains(&deny),
                "`{command}` izin dosyasi `{allow}` / `{deny}` ciftini icermiyor"
            );
            assert!(
                content.contains(&format!("commands.allow = [\"{command}\"]")),
                "`{command}` izin dosyasi komutu tek tek acmiyor"
            );
        }
    }

    /// Ters yon: ACL'de izin uretilmis ama artik acilmayan bir komut kalmasin.
    /// Boyle bir kalinti, silinmis bir komutun capability'de yasamaya devam
    /// etmesi demektir.
    #[test]
    fn the_acl_contains_no_permissions_for_unknown_commands() {
        for (command, _) in all_autogenerated_permission_files() {
            assert!(
                EXPOSED_COMMANDS.contains(&command.as_str()),
                "`permissions/autogenerated/{command}.toml` bilinmeyen bir komuta ait — \
                 `EXPOSED_COMMANDS` ile `build.rs` kaymis"
            );
        }
    }

    /// Bir capability dosyasinin actigi izinler.
    fn permissions_of(file_name: &str) -> Vec<String> {
        let capability: serde_json::Value =
            serde_json::from_str(&read(&format!("capabilities/{file_name}")))
                .unwrap_or_else(|error| panic!("`{file_name}` gecerli JSON degil: {error}"));

        capability["permissions"]
            .as_array()
            .unwrap_or_else(|| panic!("`{file_name}` icinde `permissions` dizisi yok"))
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect()
    }

    fn permission_name(command: &str) -> String {
        format!("allow-{}", command.replace('_', "-"))
    }

    /// **ASU-031 kabul kaniti**: hafiza okuma ve yazma ayri izinler.
    ///
    /// Tek bir `asuna-memory` capability'si yazilsaydi bu ayrim yalnizca
    /// dokumantasyonda kalirdi. Burada dosya duzeyinde olculuyor: okuma dosyasi
    /// hicbir yazma komutu acmiyor, yazma dosyasi da okuma komutu acmiyor.
    #[test]
    fn memory_reads_and_writes_are_separate_permissions() {
        let read_permissions = permissions_of("asuna-memory-read.json");
        let write_permissions = permissions_of("asuna-memory-write.json");

        for command in MEMORY_READ_COMMANDS {
            assert!(
                read_permissions.contains(&permission_name(command)),
                "`{command}` okuma capability'sinde yok"
            );
            assert!(
                !write_permissions.contains(&permission_name(command)),
                "`{command}` (okuma) yazma capability'sinde de aciliyor"
            );
        }

        for command in MEMORY_WRITE_COMMANDS {
            assert!(
                write_permissions.contains(&permission_name(command)),
                "`{command}` yazma capability'sinde yok"
            );
            assert!(
                !read_permissions.contains(&permission_name(command)),
                "`{command}` (yazma) okuma capability'sinde aciliyor — \
                 salt okunur hafiza modu imkansiz hale gelir"
            );
        }

        // Durum komutu ucuncu bir yuzey: hafiza icerigine hic dokunmaz.
        let status_permissions = permissions_of("asuna-db.json");
        assert_eq!(status_permissions, vec![permission_name("db_status")]);
    }

    /// **ASU-037**: gizlilik anahtarlari kendi capability'sinde durur ve hafiza
    /// yuzeylerine karismaz. Toplu silme ise hafiza **yazma** dosyasindadir —
    /// yazma izni kaldirildiginda "tum hafizayi sil" de kapanmali.
    #[test]
    fn privacy_settings_have_their_own_capability_and_purge_stays_with_writes() {
        let privacy_permissions = permissions_of("asuna-privacy.json");
        assert_eq!(
            privacy_permissions,
            vec![
                permission_name("get_privacy_settings"),
                permission_name("set_privacy_settings"),
            ]
        );

        let write_permissions = permissions_of("asuna-memory-write.json");
        assert!(write_permissions.contains(&permission_name("memory_delete_all")));
        assert!(!permissions_of("asuna-memory-read.json")
            .contains(&permission_name("memory_delete_all")));

        for command in ["get_privacy_settings", "set_privacy_settings"] {
            assert!(
                !write_permissions.contains(&permission_name(command)),
                "`{command}` hafiza yazma capability'sinde de aciliyor"
            );
        }
    }

    /// Okuma/yazma listeleri `EXPOSED_COMMANDS` ile ayni kumeyi kapsamali;
    /// yeni bir `memory_*` komutu siniflandirilmadan eklenemesin.
    #[test]
    fn every_memory_command_is_classified_as_read_or_write() {
        for command in EXPOSED_COMMANDS
            .iter()
            .filter(|name| name.starts_with("memory_"))
        {
            assert!(
                MEMORY_READ_COMMANDS.contains(command) ^ MEMORY_WRITE_COMMANDS.contains(command),
                "`{command}` ya okuma ya yazma listesinde olmali (ikisinde birden degil)"
            );
        }
    }

    /// **ASU-065 kabul kaniti**: oturum okuma ve oturum degistirme ayri
    /// capability dosyalari.
    ///
    /// Ayrimin somut karsiligi: `asuna-session`'i `tauri.conf.json` listesinden
    /// cikarmak kaydi **ve** silmeyi kapatir, ama oturum gecmisini gorunur
    /// birakir — "yalnizca incele" modu. Tersi de dogru: okuma dosyasi hicbir
    /// silme izni tasiyamaz.
    #[test]
    fn session_reads_and_writes_are_separate_permissions() {
        let read_permissions = permissions_of("asuna-session-read.json");
        let write_permissions = permissions_of("asuna-session.json");

        for command in SESSION_READ_COMMANDS {
            assert!(
                read_permissions.contains(&permission_name(command)),
                "`{command}` oturum okuma capability'sinde yok"
            );
            assert!(
                !write_permissions.contains(&permission_name(command)),
                "`{command}` (okuma) yazma capability'sinde de aciliyor"
            );
        }

        for command in SESSION_WRITE_COMMANDS {
            assert!(
                write_permissions.contains(&permission_name(command)),
                "`{command}` oturum yazma capability'sinde yok"
            );
            assert!(
                !read_permissions.contains(&permission_name(command)),
                "`{command}` (yazma/silme) okuma capability'sinde aciliyor — \
                 salt okunur oturum gecmisi imkansiz hale gelir"
            );
        }

        // Oturum yuzeyleri hafiza yuzeylerine karismaz (PROJECT.md Bolum 14).
        let memory_read = permissions_of("asuna-memory-read.json");
        let memory_write = permissions_of("asuna-memory-write.json");
        for command in SESSION_READ_COMMANDS.iter().chain(&SESSION_WRITE_COMMANDS) {
            assert!(
                !memory_read.contains(&permission_name(command))
                    && !memory_write.contains(&permission_name(command)),
                "`{command}` hafiza capability'lerinde aciliyor"
            );
        }
    }

    /// Yeni bir `session_*` komutu siniflandirilmadan eklenemesin.
    #[test]
    fn every_session_command_is_classified_as_read_or_write() {
        for command in EXPOSED_COMMANDS
            .iter()
            .filter(|name| name.starts_with("session_"))
        {
            assert!(
                SESSION_READ_COMMANDS.contains(command) ^ SESSION_WRITE_COMMANDS.contains(command),
                "`{command}` ya okuma ya yazma listesinde olmali (ikisinde birden degil)"
            );
        }
    }

    /// Capability dosyalarindaki her `allow-*` izni gercekten uretilmis bir
    /// izne isaret etmeli. Yazim hatasi (`allow-db-statu`) aksi halde ancak
    /// runtime'da, sessiz bir red olarak ortaya cikardi.
    #[test]
    fn capability_permissions_resolve_to_generated_acl_entries() {
        let known: Vec<String> = EXPOSED_COMMANDS
            .iter()
            .map(|command| format!("allow-{}", command.replace('_', "-")))
            .collect();

        for (file_name, content) in all_capability_files() {
            let capability: serde_json::Value =
                serde_json::from_str(&content).expect("capability gecerli JSON");
            let permissions = capability["permissions"]
                .as_array()
                .unwrap_or_else(|| panic!("`{file_name}` icinde `permissions` dizisi yok"));

            for permission in permissions.iter().filter_map(|item| item.as_str()) {
                // `core:default` gibi Tauri cekirdek izinleri bu testin disinda.
                if permission.contains(':') {
                    continue;
                }
                assert!(
                    known.contains(&permission.to_owned()),
                    "`{file_name}` icindeki `{permission}` hicbir uygulama komutuna karsilik gelmiyor"
                );
            }
        }
    }
}

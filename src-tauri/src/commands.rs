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
pub const EXPOSED_COMMANDS: [&str; 26] = [
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
    "record_tool_event",
    "tool_event_list",
    "project_list",
    "project_context",
    "project_add",
    "project_remove",
    "project_set_current",
    "read_project_file",
    "open_project",
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

/// Tool audit defterini **okuyan** komutlar (ASU-050).
///
/// Ayri bir capability'de: `asuna-tool-audit-write`'i `tauri.conf.json`
/// listesinden cikarmak yeni audit yazimini durdurur ama var olan defteri
/// gorunur birakir.
#[cfg(test)]
pub const TOOL_AUDIT_READ_COMMANDS: [&str; 1] = ["tool_event_list"];

/// Tool audit defterine **yazan** komutlar (ASU-050).
///
/// Bu liste bilerek **tek elemanli** ve boyle kalacak. `tool_events` MVP'de
/// salt yazilir (append-only): silme ve guncelleme komutu yoktur, dolayisiyla
/// "audit kayitlari uygulamadan silinemiyor" kriteri bir politika degil bir
/// **yuzey eksikligi** olarak garanti edilir. Asagidaki
/// [`tests::the_tool_audit_surface_is_append_only`] bunu kilitler.
#[cfg(test)]
pub const TOOL_AUDIT_WRITE_COMMANDS: [&str; 1] = ["record_tool_event"];

/// Kayitli proje koklerini **okuyan** komutlar (ASU-040, ASU-044).
///
/// `project_context` neden okuma tarafinda: guncel projenin ozetini, git
/// durumunu ve devir teslim artefaktini dondurur ama hicbirini **degistirmez** —
/// "guncel proje" secimi bile bu cagriyla kaymaz (o `project_set_current`'in
/// isi). Yazma iznine konsaydi, salt okunur bir kurulumda Asuna hangi projede
/// oldugunu soyleyemez hale gelirdi.
#[cfg(test)]
pub const PROJECT_READ_COMMANDS: [&str; 2] = ["project_list", "project_context"];

/// Kayitli proje koklerini **degistiren** komutlar (ASU-040).
///
/// Bu liste ASU-049 path sandbox'inin tek kaynagini besliyor: yeni bir kok
/// eklemenin tek yolu `project_add`. Yazma capability'sini `tauri.conf.json`
/// listesinden cikarmak, kok eklemeyi kapatir ama var olan projeleri gorunur
/// birakir — "yalnizca incele" modu.
///
/// `project_set_current` neden yazma tarafinda: `last_opened_at`i degistiriyor
/// ve o alan "guncel proje"nin tek eksenidir. Okuma iznine konsaydi, salt
/// okunur sanilan bir yuzey Asuna'nin hangi projede oldugunu degistirebilirdi.
#[cfg(test)]
pub const PROJECT_WRITE_COMMANDS: [&str; 3] =
    ["project_add", "project_remove", "project_set_current"];

/// Kayitli kok icinde **dosya okuyan** komutlar (ASU-051).
///
/// `PROJECT_READ_COMMANDS`ten ayri bir liste ve ayri bir capability: iki
/// yuzeyin genisligi ayni degil. `project_context` kokun altindaki **sabit** bir
/// allowlist'i okur (PROJECT.md, README.md, manifest'ler); `read_project_file`
/// ise kok icindeki herhangi bir metin dosyasini okuyabilir. Ayni izin
/// dosyasina konsaydi "proje ozetini gorebil ama dosya okuyamasin" diye bir
/// kurulum mumkun olmazdi.
#[cfg(test)]
pub const PROJECT_FILE_READ_COMMANDS: [&str; 1] = ["read_project_file"];

/// **Alt process baslatan** komutlar (ASU-052).
///
/// Bu liste bilerek tek elemanli ve genisletilmesi bir mimari karardir:
/// PROJECT.md Bolum 18 sinirsiz komut calistirmayi (`run_any_shell_command`)
/// yasaklar. Buraya eklenen her komut, dar kapsamli ve kayitli bir kok ile
/// sinirli olmak zorunda. Ayri capability olmasinin somut karsiligi:
/// `asuna-project-open`'i `tauri.conf.json` listesinden cikarmak Asuna'nin
/// herhangi bir program calistirma yolunu tamamen kapatir, geri kalan her sey
/// calismaya devam eder.
#[cfg(test)]
pub const PROCESS_LAUNCH_COMMANDS: [&str; 1] = ["open_project"];

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

    /// **ASU-040 kabul kaniti**: proje okuma ve proje kaydi ayri capability
    /// dosyalari.
    ///
    /// Ayrimin somut karsiligi: `asuna-projects-write`'i `tauri.conf.json`
    /// listesinden cikarmak yeni kok eklenmesini kapatir ama var olan
    /// projeleri gorunur birakir. Kayitli kok listesi ASU-049 sandbox'inin tek
    /// kaynagi oldugu icin bu iki yetkinin ayni dosyada olmamasi onemli.
    #[test]
    fn project_reads_and_writes_are_separate_permissions() {
        let read_permissions = permissions_of("asuna-projects-read.json");
        let write_permissions = permissions_of("asuna-projects-write.json");

        for command in PROJECT_READ_COMMANDS {
            assert!(
                read_permissions.contains(&permission_name(command)),
                "`{command}` proje okuma capability'sinde yok"
            );
            assert!(
                !write_permissions.contains(&permission_name(command)),
                "`{command}` (okuma) yazma capability'sinde de aciliyor"
            );
        }

        for command in PROJECT_WRITE_COMMANDS {
            assert!(
                write_permissions.contains(&permission_name(command)),
                "`{command}` proje yazma capability'sinde yok"
            );
            assert!(
                !read_permissions.contains(&permission_name(command)),
                "`{command}` (yazma) okuma capability'sinde aciliyor — \
                 salt okunur proje listesi imkansiz hale gelir"
            );
        }

        // Proje yuzeyleri hafiza ve oturum yuzeylerine karismaz.
        for file in [
            "asuna-memory-read.json",
            "asuna-memory-write.json",
            "asuna-session-read.json",
            "asuna-session.json",
        ] {
            let permissions = permissions_of(file);
            for command in PROJECT_READ_COMMANDS.iter().chain(&PROJECT_WRITE_COMMANDS) {
                assert!(
                    !permissions.contains(&permission_name(command)),
                    "`{command}` `{file}` icinde de aciliyor"
                );
            }
        }
    }

    /// Yeni bir `project_*` komutu siniflandirilmadan eklenemesin.
    #[test]
    fn every_project_command_is_classified_as_read_or_write() {
        for command in EXPOSED_COMMANDS
            .iter()
            .filter(|name| name.starts_with("project_"))
        {
            assert!(
                PROJECT_READ_COMMANDS.contains(command) ^ PROJECT_WRITE_COMMANDS.contains(command),
                "`{command}` ya okuma ya yazma listesinde olmali (ikisinde birden degil)"
            );
        }
    }

    /// **ASU-050 kabul kaniti**: tool audit okuma ve yazma ayri capability
    /// dosyalari; okuma dosyasi hicbir yazma izni tasiyamaz.
    #[test]
    fn tool_audit_reads_and_writes_are_separate_permissions() {
        let read_permissions = permissions_of("asuna-tool-audit-read.json");
        let write_permissions = permissions_of("asuna-tool-audit-write.json");

        assert_eq!(read_permissions, vec![permission_name("tool_event_list")]);
        assert_eq!(
            write_permissions,
            vec![permission_name("record_tool_event")]
        );

        // Audit yuzeyleri hafiza, oturum ve proje yuzeylerine karismaz: audit
        // ayri bir katman ve ayri bir yetki eksenidir.
        for file in [
            "asuna-memory-read.json",
            "asuna-memory-write.json",
            "asuna-session-read.json",
            "asuna-session.json",
            "asuna-projects-read.json",
            "asuna-projects-write.json",
        ] {
            let permissions = permissions_of(file);
            for command in TOOL_AUDIT_READ_COMMANDS
                .iter()
                .chain(&TOOL_AUDIT_WRITE_COMMANDS)
            {
                assert!(
                    !permissions.contains(&permission_name(command)),
                    "`{command}` `{file}` icinde de aciliyor"
                );
            }
        }
    }

    /// **ASU-050 kabul kaniti**: "audit kayitlari uygulamadan silinemiyor".
    ///
    /// Kilit bir politika degil, bir **yuzey eksikligi**: `tool_events`'e
    /// dokunan yalnizca iki komut var ve ikisi de ekleme/okuma. Bir gun
    /// `tool_event_delete` ya da `tool_event_clear_all` eklenirse bu test duser
    /// ve karar bilincli olarak verilmek zorunda kalir.
    #[test]
    fn the_tool_audit_surface_is_append_only() {
        let audit_commands: Vec<&str> = EXPOSED_COMMANDS
            .iter()
            .copied()
            .filter(|name| name.contains("tool_event"))
            .collect();
        assert_eq!(
            audit_commands,
            vec!["record_tool_event", "tool_event_list"],
            "audit yuzeyine yeni bir komut eklenmis"
        );

        for forbidden in [
            "tool_event_delete",
            "tool_event_update",
            "tool_event_clear_all",
            "tool_event_purge",
            "tool_event_archive",
        ] {
            assert!(
                !EXPOSED_COMMANDS.contains(&forbidden),
                "`{forbidden}` acilmis — audit artik salt yazilir degil"
            );
            let permission = format!("allow-{}", forbidden.replace('_', "-"));
            for (file_name, content) in all_capability_files() {
                assert!(
                    !content.contains(&permission),
                    "`{file_name}` `{permission}` izni tasiyor"
                );
            }
        }
    }

    /// Yeni bir `tool_event` / `record_tool_event` komutu siniflandirilmadan
    /// eklenemesin.
    #[test]
    fn every_tool_audit_command_is_classified_as_read_or_write() {
        for command in EXPOSED_COMMANDS
            .iter()
            .filter(|name| name.contains("tool_event"))
        {
            assert!(
                TOOL_AUDIT_READ_COMMANDS.contains(command)
                    ^ TOOL_AUDIT_WRITE_COMMANDS.contains(command),
                "`{command}` ya okuma ya yazma listesinde olmali (ikisinde birden degil)"
            );
        }
    }

    /// **ASU-051 kabul kaniti**: dosya okuma kendi capability'sinde durur ve
    /// proje okuma/yazma yuzeylerine karismaz.
    ///
    /// Ayrimin somut karsiligi: `asuna-project-file-read`'i `tauri.conf.json`
    /// listesinden cikarmak Asuna'nin dosya okumasini kapatir ama proje
    /// listesini ve proje ozetini gorunur birakir.
    #[test]
    fn project_file_reads_have_their_own_capability() {
        assert_eq!(
            permissions_of("asuna-project-file-read.json"),
            vec![permission_name("read_project_file")]
        );

        for file in [
            "asuna-projects-read.json",
            "asuna-projects-write.json",
            "asuna-project-open.json",
        ] {
            let permissions = permissions_of(file);
            for command in PROJECT_FILE_READ_COMMANDS {
                assert!(
                    !permissions.contains(&permission_name(command)),
                    "`{command}` `{file}` icinde de aciliyor"
                );
            }
        }
    }

    /// **ASU-052 kabul kaniti + PROJECT.md Bolum 18 kilidi**: alt process
    /// baslatan yuzey tektir, adiyla bellidir ve kendi capability'sindedir.
    ///
    /// Sinirsiz komut calistirma (`run_any_shell_command` ve akrabalari) hicbir
    /// zaman acilmaz. Bu test onu bir politika olmaktan cikarip **yuzey
    /// eksikligi** haline getirir: boyle bir komut eklenirse burada duser.
    #[test]
    fn the_process_launch_surface_is_a_single_named_command() {
        assert_eq!(
            permissions_of("asuna-project-open.json"),
            vec![permission_name("open_project")]
        );

        for forbidden in [
            "run_any_shell_command",
            "run_shell_command",
            "execute_command",
            "run_command",
            "spawn_process",
            "open_path",
            "open_url",
        ] {
            assert!(
                !EXPOSED_COMMANDS.contains(&forbidden),
                "`{forbidden}` acilmis — sinirsiz/keyfi calistirma yuzeyi olustu"
            );
            let permission = format!("allow-{}", forbidden.replace('_', "-"));
            for (file_name, content) in all_capability_files() {
                assert!(
                    !content.contains(&permission),
                    "`{file_name}` `{permission}` izni tasiyor"
                );
            }
        }

        // `shell` plugin'i hic yuklu degil: acilis yolu Tauri tarafinda da yok.
        for (file_name, content) in all_capability_files() {
            assert!(
                !content.contains("\"shell:"),
                "`{file_name}` shell plugin izni tasiyor"
            );
        }

        // Proje okuma/yazma yuzeyleri process baslatamaz.
        for file in [
            "asuna-projects-read.json",
            "asuna-projects-write.json",
            "asuna-project-file-read.json",
        ] {
            let permissions = permissions_of(file);
            for command in PROCESS_LAUNCH_COMMANDS {
                assert!(
                    !permissions.contains(&permission_name(command)),
                    "`{command}` `{file}` icinde de aciliyor"
                );
            }
        }
    }

    /// **ASU-045 devri**: dialog plugin'i yalnizca `open` acar.
    ///
    /// Statik kilit; ayni kural `acl_regression.rs` icinde gercek ACL uzerinde
    /// de olculuyor. Ikisi birlikte: dosya yanlislikla genisletilirse bu test,
    /// izin adi yanlis yazilirsa (sessiz red) oteki test duser.
    #[test]
    fn the_dialog_plugin_only_opens_a_directory_picker() {
        let permissions = permissions_of("asuna-dialog.json");
        assert_eq!(
            permissions,
            vec!["dialog:allow-open"],
            "dialog capability'si genislemis"
        );

        // `save` bir dosya YAZMA hedefi sectirir; `message`/`ask`/`confirm`
        // WKWebView'de modal sistem penceresi acar ve ses oturumunu kilitler —
        // Asuna onaylari uygulama icinde, iptal edilebilir satir icinde alinir.
        for forbidden in [
            "dialog:allow-save",
            "dialog:allow-message",
            "dialog:allow-ask",
            "dialog:allow-confirm",
            "dialog:default",
        ] {
            for (file_name, content) in all_capability_files() {
                assert!(
                    !content.contains(&format!("\"{forbidden}\"")),
                    "`{file_name}` `{forbidden}` iznini aciyor"
                );
            }
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

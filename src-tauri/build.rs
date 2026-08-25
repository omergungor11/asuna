fn main() {
    // ACL manifest'i acikca tanimlaniyor (ASU-009). Bu, uygulama komutlarini
    // Tauri'nin varsayilan "app command'lari serbest" davranisindan cikarip
    // **deny-by-default** yapar: burada listelenmeyen ya da bir capability
    // tarafindan izin verilmeyen bir komut renderer'dan cagrilamaz.
    // Her yeni `#[tauri::command]` icin: bu listeye ekle + `capabilities/`
    // altinda dar kapsamli bir izin kaydi ac.
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            // ASU-009: config whitelist'i (capabilities/asuna-config.json)
            "get_frontend_config",
            // ASU-011: ephemeral Realtime token (capabilities/asuna-realtime.json)
            "mint_realtime_token",
            // ASU-029: hafiza alt sisteminin durumu (capabilities/asuna-db.json).
            // Salt okunur; hicbir hafiza kaydi ya da SQL bu komuttan gecmez.
            "db_status",
            // ASU-031: hafiza okuma (capabilities/asuna-memory-read.json).
            // Okuma ve yazma AYRI izinlerdir: ileride "salt okunur hafiza"
            // moduna gecmek, yazma capability'sini `tauri.conf.json`'dan
            // cikarmak kadar basit olmali.
            "memory_list",
            // ASU-031: hafiza yazma (capabilities/asuna-memory-write.json).
            "memory_create",
            "memory_update",
            "memory_archive",
            "memory_delete",
            // ASU-037: tum hafizayi silme. Yazma capability'sinin parcasi;
            // komut ayrica birebir bir onay ifadesi ister.
            "memory_delete_all",
            // ASU-035: oturum acilisindaki Stage A baglam paketi
            // (capabilities/asuna-memory-read.json). Salt okuma; renderer
            // parametre veremez — retrieval politikasi host tarafinda.
            "get_bootstrap_context",
            // ASU-032: oturum kaydi (capabilities/asuna-session.json). Model
            // config'ten gelir, transcript yolu Rust tarafinda cozulur —
            // renderer ikisini de veremez.
            "session_start",
            "session_finalize",
            // ASU-065: oturum gecmisi okuma (capabilities/asuna-session-read.json).
            // Salt okuma; dokum dosya yolu renderer'a donmez.
            "session_list",
            // ASU-065: oturum ozeti + dokum temizligi
            // (capabilities/asuna-session.json). Toplu silme birebir bir onay
            // ifadesi ister; dosya yolu Rust tarafinda cozulur ve sandbox
            // disina cikmasi reddedilir.
            "session_delete",
            "session_clear_all",
            // ASU-040: kayitli proje kokleri (capabilities/asuna-projects-read.json).
            // Salt okuma; renderer parametre veremez.
            "project_list",
            // ASU-040: proje kaydi (capabilities/asuna-projects-write.json).
            // Komut bir yol metni alir ama yalnizca **var olan**, mutlak,
            // symlink'i cozulmus bir dizini kabul eder; `~` genisletilmez.
            // Bu liste ASU-049 sandbox'inin tek kaynagi olacak.
            "project_add",
            "project_remove",
            "project_set_current",
            // ASU-037: calisma zamani gizlilik anahtarlari
            // (capabilities/asuna-privacy.json). Secret icermez; yalnizca
            // kullanicinin kendi ayarlari okunur/yazilir.
            "get_privacy_settings",
            "set_privacy_settings",
        ]));

    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("tauri-build basarisiz: {error:#}");
    }
}

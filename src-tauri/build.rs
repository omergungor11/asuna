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
            // ASU-032: oturum kaydi (capabilities/asuna-session.json). Model
            // config'ten gelir, transcript yolu Rust tarafinda cozulur —
            // renderer ikisini de veremez.
            "session_start",
            "session_finalize",
        ]));

    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("tauri-build basarisiz: {error:#}");
    }
}

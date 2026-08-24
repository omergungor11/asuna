//! Asuna — Tauri host process (guvenilir taraf).
//!
//! Bu process'in sorumlulugu: konfigurasyon ve secret sahipligi, ephemeral
//! Realtime token uretimi (ASU-011), SQLite erisimi, wake-word motoru,
//! path sandbox ve tool execution (PROJECT.md Bolum 19).
//!
//! `OPENAI_API_KEY` yalnizca bu tarafta okunur ve `AsunaConfig` icinde kalir;
//! IPC ile webview'e gecmez (ASU-009).

use tauri::Manager;

#[cfg(test)]
mod acl_regression;
pub mod commands;
pub mod config;
pub mod db;
pub mod env_file;
pub mod realtime_token;

/// Uygulama context'i — **crate basina tek `generate_context!` cagrisi**.
///
/// Makro capability dosyalarini, ACL manifest'ini ve asset'leri derleme
/// zamaninda gomer; ikinci bir cagri ayni sembolleri yeniden uretir. Tek
/// cagriyi burada tutmak, [`acl_regression`] testlerinin uretimle **birebir
/// ayni** ACL uzerinde kosmasini da saglar (ADR-005 spike yontemi).
pub fn app_context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Konfigurasyon acilista, pencere acilmadan once dogrulanir. Eksik/gecersiz
    // deger sessizce default'lanmaz: net mesajla ve panic'siz cikilir.
    let config = match config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("[asuna] Yapilandirma hatasi: {error}");
            eprintln!(
                "[asuna] `.env.example` dosyasini `.env` olarak kopyalayip tum degiskenleri \
                 doldurun (veya degerleri process environment'inda tanimlayin)."
            );
            std::process::exit(1);
        }
    };

    // `ASUNA_MEMORY_ENABLED` DB acilmadan once okunur: kapaliysa dosya hic
    // olusturulmaz (PROJECT.md Bolum 20 gizlilik garantisi).
    let memory_enabled = config.memory_enabled;

    // `build()` + `run()`, `run(context)` yerine bilerek: DB acilisi
    // `Builder::setup` hook'una konsaydi test kurulumunda calismazdi
    // (ADR-005 "Her iki secenekte de ortaya cikan iki tuzak" / 2) ve acilis
    // hatasini ele almak icin `setup`'in `Result`'ina bagimli kalirdik —
    // oysa DB hatasi uygulamayi durdurmamali.
    let app = tauri::Builder::default()
        // Config yalnizca Rust tarafinda yasar; komutlar `State<AsunaConfig>` ile erisir.
        .manage(config)
        // Ephemeral Realtime token uretimi (ASU-011). HTTPS istemcisi ilk
        // kullanimda kurulur; kurulamazsa uygulama dusmez, komut tipli hata
        // doner (PROJECT.md Bolum 30).
        .manage(realtime_token::RealtimeTokenService::new())
        // Webview'e acilan her komut ayri bir yetki yuzeyidir ve kendi
        // capability kaydiyla birlikte eklenir (`capabilities/`).
        .invoke_handler(tauri::generate_handler![
            commands::get_frontend_config,
            realtime_token::mint_realtime_token,
            db::state::db_status,
            db::memory_repository::memory_list,
            db::memory_repository::memory_create,
            db::memory_repository::memory_update,
            db::memory_repository::memory_archive,
            db::memory_repository::memory_delete,
            db::session_repository::session_start,
            db::session_repository::session_finalize
        ])
        .build(app_context())
        .expect("Tauri uygulamasi baslatilamadi");

    // SQLite acilisi + migration (ASU-029). Hata halinde **cikilmaz**:
    // `DbState::Unavailable` yonetilir, `db_status` komutu durumu bildirir ve
    // konusma hafizasiz devam eder (PROJECT.md Bolum 30).
    let db_state = db::DbState::initialize(app.handle(), memory_enabled);

    // ASU-032: cokme/kill sonrasi `ended_at` NULL kalmis oturumlar burada
    // kapatilir. Hata halinde uygulama yine acilir — kurtarma bir kolaylik,
    // acilis kosulu degil.
    if let Some(database) = db_state.database() {
        match db::session_repository::close_abandoned(database) {
            Ok(0) => {}
            Ok(count) => eprintln!(
                "[asuna] Yarim kalmis {count} oturum kaydi kapatildi (onceki calisma \
                 beklenmedik sekilde sonlanmis)."
            ),
            Err(error) => eprintln!("[asuna] Yarim oturum kaydi kapatilamadi: {error}"),
        }
    }

    app.manage(db_state);

    app.run(|_app_handle, _event| {});
}

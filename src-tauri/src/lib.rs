//! Asuna — Tauri host process (guvenilir taraf).
//!
//! Bu process'in sorumlulugu: konfigurasyon ve secret sahipligi, ephemeral
//! Realtime token uretimi (ASU-011), SQLite erisimi, wake-word motoru,
//! path sandbox ve tool execution (PROJECT.md Bolum 19).
//!
//! `OPENAI_API_KEY` yalnizca bu tarafta okunur ve `AsunaConfig` icinde kalir;
//! IPC ile webview'e gecmez (ASU-009).

pub mod commands;
pub mod config;
pub mod env_file;

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

    tauri::Builder::default()
        // Config yalnizca Rust tarafinda yasar; komutlar `State<AsunaConfig>` ile erisir.
        .manage(config)
        // Webview'e acilan her komut ayri bir yetki yuzeyidir ve kendi
        // capability kaydiyla birlikte eklenir (`capabilities/`).
        .invoke_handler(tauri::generate_handler![commands::get_frontend_config])
        .run(tauri::generate_context!())
        .expect("Tauri uygulamasi baslatilamadi");
}

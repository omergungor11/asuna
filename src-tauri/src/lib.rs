//! Asuna — Tauri host process (guvenilir taraf).
//!
//! Phase 0 (ASU-002): burada hicbir Asuna is mantigi yok, yalnizca bos pencere aciliyor.
//! Ileride bu process'in sorumlulugu: ephemeral Realtime token uretimi, SQLite erisimi,
//! path sandbox ve tool execution (PROJECT.md Bolum 19). `OPENAI_API_KEY` ve
//! `PICOVOICE_ACCESS_KEY` yalnizca bu tarafta okunur, IPC ile webview'e gecmez.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // `invoke_handler` bilerek bos: webview'e acilan her komut ayri bir yetki
        // yuzeyidir ve kendi capability kaydiyla birlikte eklenir.
        .run(tauri::generate_context!())
        .expect("Tauri uygulamasi baslatilamadi");
}

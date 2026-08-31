//! ACL regresyon testleri (ASU-029 / ADR-005 "Etkiler" + Phase 3 plani Adim 3).
//!
//! # Neden bu dosya var
//!
//! Asuna'da uygulama komutlari **deny-by-default**: `build.rs` acik bir
//! `AppManifest` tanimlar, dolayisiyla `src-tauri/permissions/` altindaki ACL
//! her komuta uygulanir. Bu, yeni bir komut (ya da yeni bir capability)
//! eklendiginde **var olan komutlarin sessizce reddedilmesi** riskini dogurur:
//! calisan uygulama, hicbir derleme hatasi vermeden kirilir.
//!
//! `commands.rs` icindeki statik testler dosyalarin birbirine referans verdigini
//! dogrular. Burada bir adim oteye gidiliyor: **gercek** ACL (gercek
//! `capabilities/*.json`, gercek `tauri.conf.json`) uzerinde, renderer'in
//! gonderdigi `InvokeRequest`'in aynisi gonderilip komutun ACL kapisindan
//! gectigi olculuyor.
//!
//! # Ag'a cikilmaz
//!
//! `mint_realtime_token` icin `RealtimeTokenService` bilerek `manage`
//! **edilmiyor**. Boylece komut ACL'i gecerse "state not managed" hatasiyla
//! duser — OpenAI'ye hicbir istek gitmez. Ayrim testin can alici noktasi:
//! *reddedilme* ile *calisip baska bir nedenle hata verme* farkli seylerdir.
//!
//! Ayni gerekceyle `summary::SummaryService` de `manage` **edilmiyor**:
//! `session_finalize` kapanistan sonra ozet uretimini tetikler; servis kayitli
//! olmadigi icin tetik log'layip durur ve bu testlerden ag'a cikilmaz.

use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::MockRuntime;
use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewWindowBuilder};

use std::sync::Arc;

use crate::commands::EXPOSED_COMMANDS;
use crate::config::{self, EnvMap};
use crate::db::DbState;
use crate::privacy::PrivacyState;

/// Capability dosyalarinda acilan tek pencere etiketi.
const ALLOWED_WINDOW: &str = "main";

/// ACL kapsaminda **olmayan** bir pencere — negatif kontrol.
const FOREIGN_WINDOW: &str = "acl-probe";

fn test_config() -> config::AsunaConfig {
    let pairs = [
        ("OPENAI_API_KEY", "sk-proj-TEST-DEGERI-AGA-CIKILMAZ"),
        ("ASUNA_REALTIME_MODEL", "gpt-realtime-2.1"),
        ("ASUNA_SUMMARY_MODEL", "gpt-4o-mini"),
        ("ASUNA_REALTIME_VOICE", "marin"),
        ("ASUNA_WAKE_WORD", "Hey Asuna"),
        ("ASUNA_MEMORY_ENABLED", "true"),
        ("ASUNA_TRANSCRIPT_STORAGE", "false"),
        ("ASUNA_TOOL_APPROVAL_MODE", "safe"),
        ("ASUNA_IDLE_TIMEOUT_SECONDS", "45"),
        ("ASUNA_LOG_LEVEL", "info"),
        ("ASUNA_WAKE_WORD_PROVIDER", "fake"),
        ("ASUNA_WAKE_WORD_MODEL_DIR", ""),
        ("ASUNA_WAKE_WORD_THRESHOLD", "0.25"),
        ("ASUNA_TURN_DETECTION", "semantic_vad"),
        ("ASUNA_VAD_EAGERNESS", "high"),
        ("ASUNA_VAD_SILENCE_MS", "400"),
        // ASU-052: testlerde gercek bir editor calistirilmaz; buradaki deger
        // yalnizca config sozlesmesini doldurur.
        ("ASUNA_EDITOR_COMMAND", "asuna-test-editor-yok"),
    ];
    let map: EnvMap = pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    config::load_from_map(&map).expect("test config gecerli olmali")
}

/// Uretimdeki `run()` ile **ayni** komut kumesi ve **ayni** context; tek fark
/// runtime'in mock olmasi ve DB'nin bellege/kapaliya alinmasi.
///
/// Gizlilik durumu **her uygulama icin ayri** (`Arc<PrivacyState>`): process
/// genelindeki durum ([`crate::privacy::install_process_state`]) burada
/// kurulmaz, boylece bir testin anahtari digerini etkilemez.
fn build_test_app_with(db_state: DbState) -> App<MockRuntime> {
    build_test_app_with_privacy(db_state, PrivacyState::from_boot(true, true))
}

fn build_test_app_with_privacy(db_state: DbState, privacy: PrivacyState) -> App<MockRuntime> {
    let app = mock_builder()
        // ASU-045 devri (ASU-050): dialog plugin'i uretimdeki `run()` ile ayni
        // sekilde yukleniyor. Yuklenmeseydi `plugin:dialog|save` cagrisi
        // "Plugin not found" ile duserdi ve `plugin:dialog|open` ile ayni
        // gorunurdu — yani ACL kilidini olcen test hicbir sey kanitlamazdi.
        // `open` bu testlerde **cagrilmaz**: gercek bir sistem penceresi acmak
        // mock runtime'da anlamsiz. Onun tek izin oldugu `commands.rs` icinde
        // statik olarak dogrulanir.
        .plugin(tauri_plugin_dialog::init())
        .manage(test_config())
        .manage(Arc::new(privacy))
        // ASU-044: `project_context` bu servisi bekler. Uretimdeki `lib.rs` ile
        // ayni sekilde `manage` ediliyor — ag'a cikmaz, yalnizca kayitli kokun
        // altindaki sabit allowlist'i okur.
        .manage(crate::projects::context::ProjectContextService::new())
        // `RealtimeTokenService` BILEREK yok — bkz. modul dokumantasyonu.
        .invoke_handler(tauri::generate_handler![
            crate::commands::get_frontend_config,
            crate::realtime_token::mint_realtime_token,
            crate::db::state::db_status,
            crate::db::memory_repository::memory_list,
            crate::db::memory_repository::memory_create,
            crate::db::memory_repository::memory_update,
            crate::db::memory_repository::memory_archive,
            crate::db::memory_repository::memory_delete,
            crate::db::memory_repository::memory_delete_all,
            crate::db::retrieval::get_bootstrap_context,
            crate::db::session_repository::session_start,
            crate::db::session_repository::session_finalize,
            crate::db::session_repository::session_list,
            crate::db::session_repository::session_delete,
            crate::db::session_repository::session_clear_all,
            crate::db::tool_event_repository::record_tool_event,
            crate::db::tool_event_repository::tool_event_list,
            crate::projects::registry::project_list,
            crate::projects::registry::project_add,
            crate::projects::registry::project_remove,
            crate::projects::registry::project_set_current,
            crate::projects::view::project_context,
            crate::projects::files::read_project_file,
            crate::projects::editor::open_project,
            crate::privacy::get_privacy_settings,
            crate::privacy::set_privacy_settings
        ])
        .build(crate::app_context())
        .expect("mock app kurulmali");

    // ADR-005 tuzagi (2): `Builder::setup` hook'u `build()` degil `App::run()`
    // icinde kosar. Test `run()` cagirmadigi icin state burada, build sonrasi,
    // elle manage edilir — uretimdeki `lib.rs` ile ayni sira.
    app.manage(db_state);
    app
}

fn build_test_app() -> App<MockRuntime> {
    build_test_app_with(DbState::Disabled)
}

/// Hafizasi acik bir uygulama. DB **bellek ici**: gercek uygulama veri dizinine
/// hicbir sey yazilmaz.
fn build_test_app_with_memory() -> App<MockRuntime> {
    build_test_app_with(DbState::Ready(
        crate::db::AsunaDb::open_in_memory().expect("bellek ici DB acilmali"),
    ))
}

fn invoke_with(
    webview: &tauri::WebviewWindow<MockRuntime>,
    command: &str,
    args: serde_json::Value,
) -> Result<String, String> {
    let request = InvokeRequest {
        cmd: command.to_owned(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        // Renderer'in origin'i: ACL yalnizca app origin'inden gelen cagrilarda
        // `local: true` capability'leri uygular (tauri `is_local_url`).
        url: if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .expect("gecerli URL"),
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };

    match get_ipc_response(webview, request) {
        Ok(body) => Ok(body
            .deserialize::<serde_json::Value>()
            .expect("yanit JSON olmali")
            .to_string()),
        Err(error) => Err(error.to_string()),
    }
}

fn invoke(webview: &tauri::WebviewWindow<MockRuntime>, command: &str) -> Result<String, String> {
    invoke_with(webview, command, serde_json::Value::Null)
}

/// ASU-031 testlerinde kullanilan gecerli bir hafiza taslagi.
fn memory_draft_args() -> serde_json::Value {
    serde_json::json!({
        "draft": {
            "kind": "decision",
            "title": "Wake word yerel kalir",
            "content": "Wake word tespiti bulutta degil, cihazda calisir.",
            "importance": 0.9,
            "confidence": 1.0
        }
    })
}

/// Tauri'nin her reddi `not allowed` ifadesini tasir. Olculen bicimler:
///
/// - kapsam disi pencere: `db_status not allowed on window "acl-probe", ...
///   referenced by: capability: asuna-db, permission: allow-db-status`
/// - ACL'de olmayan komut: `execute not allowed. Command not found`
/// - yuklu olmayan plugin: `sql.execute not allowed. Plugin not found`
///
/// "state not managed ..." gibi *calisma zamani* hatalari bu kumede **degildir**
/// — ayrim testin can alici noktasi.
fn is_acl_denial(message: &str) -> bool {
    message.contains("not allowed")
}

fn main_webview(app: &App<MockRuntime>) -> tauri::WebviewWindow<MockRuntime> {
    app.get_webview_window(ALLOWED_WINDOW).unwrap_or_else(|| {
        WebviewWindowBuilder::new(app, ALLOWED_WINDOW, Default::default())
            .build()
            .expect("ana pencere kurulmali")
    })
}

/// **ASU-029 kabul kaniti.** `db_status` eklendikten sonra Phase 1'in iki
/// komutu hala ACL'den geciyor mu?
///
/// `mint_realtime_token` icin basari beklenmiyor (state yok, ag'a cikilmiyor);
/// beklenen sey hatanin bir **ACL reddi olmamasi**.
#[test]
fn existing_commands_still_pass_the_acl_after_the_db_capability_is_added() {
    let app = build_test_app();
    let webview = main_webview(&app);

    // Phase 0/1 komutu: config whitelist'i — tam olarak calismali.
    let config_json = invoke(&webview, "get_frontend_config").expect("config komutu calismali");
    assert!(
        config_json.contains("realtimeModel"),
        "beklenmeyen yanit: {config_json}"
    );

    // Phase 1 komutu: ACL'den gecmeli. State yonetilmedigi icin *baska* bir
    // nedenle hata vermesi bekleniyor — ag'a cikilmadi.
    let token_error =
        invoke(&webview, "mint_realtime_token").expect_err("state yok, hata bekleniyordu");
    assert!(
        !is_acl_denial(&token_error),
        "`mint_realtime_token` ACL tarafindan reddedildi: {token_error}"
    );

    // Phase 3 komutu: yeni capability calisiyor.
    let status_json = invoke(&webview, "db_status").expect("db_status calismali");
    assert!(
        status_json.contains("\"availability\":\"disabled\""),
        "beklenmeyen yanit: {status_json}"
    );
    assert!(
        status_json.contains("sqliteVersion"),
        "beklenmeyen yanit: {status_json}"
    );
}

/// ACL gercekten uygulaniyor mu? Capability'ler `windows: ["main"]` ile
/// sinirli; baska bir pencereden gelen ayni cagri **reddedilmeli**.
///
/// Bu negatif kontrol olmadan yukaridaki test "ACL kapali" durumunda da
/// gecerdi — yani hicbir sey kanitlamazdi.
#[test]
fn the_acl_is_actually_enforced_outside_the_permitted_window() {
    let app = build_test_app();
    let _main = main_webview(&app);

    let foreign = WebviewWindowBuilder::new(&app, FOREIGN_WINDOW, Default::default())
        .build()
        .expect("ikinci pencere kurulmali");

    for command in EXPOSED_COMMANDS {
        let error = invoke(&foreign, command)
            .unwrap_or_else(|error| error)
            .to_string();
        assert!(
            is_acl_denial(&error),
            "`{command}` ACL kapsamindaki pencere disindan calisti: {error}"
        );
        // Red gercekten ACL'den geliyor: mesaj izni ve capability'yi adlandiriyor.
        let permission = format!("allow-{}", command.replace('_', "-"));
        assert!(
            error.contains(&permission),
            "`{command}` reddi ACL izniyle iliskilendirilmemis: {error}"
        );
    }
}

/// Renderer'a SQL yuzeyi acilmadi (ADR-005 karari). `tauri-plugin-sql`
/// komutlari ya da genel bir `execute` kapisi bulunmamali.
///
/// ASU-031 ile hafiza komutlari acildi; bunlar **kaba taneli** ve kapali
/// sozlesmeli komutlardir — ham SQL yuzeyi hala yok.
#[test]
fn the_renderer_has_no_sql_surface() {
    let app = build_test_app();
    let webview = main_webview(&app);

    for command in [
        "execute",
        "select",
        "load",
        "plugin:sql|execute",
        // "SQL'i komut adina gomme" denemesi de bir komut olarak aranir ve
        // ACL'de bulunmaz.
        "memory_query",
        "memory_execute_sql",
    ] {
        let error = invoke(&webview, command).expect_err("`{command}` var olmamali");
        assert!(
            is_acl_denial(&error),
            "`{command}` renderer'a acik: {error}"
        );
    }
}

// ---------------------------------------------------------------------------
// ASU-031 — hafiza komutlari
// ---------------------------------------------------------------------------

/// Hafiza komutlari renderer'in gonderdigi gercek `InvokeRequest` ile, gercek
/// ACL uzerinden ucdan uca calisiyor mu?
#[test]
fn memory_commands_work_end_to_end_over_the_real_acl() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let created = invoke_with(&webview, "memory_create", memory_draft_args())
        .expect("memory_create calismali");
    assert!(
        created.contains("\"status\":\"stored\""),
        "beklenmeyen yanit: {created}"
    );

    let listed = invoke_with(
        &webview,
        "memory_list",
        serde_json::json!({ "filter": { "kinds": ["decision"], "search": "wake" } }),
    )
    .expect("memory_list calismali");
    assert!(
        listed.contains("Wake word yerel kalir"),
        "beklenmeyen yanit: {listed}"
    );

    let value: serde_json::Value = serde_json::from_str(&created).expect("JSON");
    let id = value["record"]["id"].as_i64().expect("kayit kimligi");

    let archived = invoke_with(
        &webview,
        "memory_archive",
        serde_json::json!({ "id": id, "archived": true }),
    )
    .expect("memory_archive calismali");
    assert!(
        archived.contains("\"isArchived\":true"),
        "yanit: {archived}"
    );

    let deleted = invoke_with(&webview, "memory_delete", serde_json::json!({ "id": id }))
        .expect("memory_delete calismali");
    assert!(
        deleted.contains("\"status\":\"deleted\""),
        "yanit: {deleted}"
    );

    // Silinen kayit gercekten gitti — sonraki oturumun baglamina giremez.
    let empty = invoke_with(&webview, "memory_list", serde_json::Value::Null)
        .expect("memory_list calismali");
    assert_eq!(empty, "[]", "yanit: {empty}");
}

/// **ASU-035 kabul kriteri**: oturum acilis baglami gercek ACL uzerinden
/// uretiliyor, yazilan hafizayi iceriyor ve **silinen** hafizayi icermiyor.
///
/// Komut bilerek parametresiz: renderer'in retrieval politikasina (proje,
/// siralama, boyut tavani) dokunma yolu yok.
#[test]
fn the_bootstrap_context_reflects_the_store_over_the_real_acl() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let empty = invoke(&webview, "get_bootstrap_context").expect("baglam komutu calismali");
    assert!(
        empty.contains("\"memoryAvailable\":true") && empty.contains("\"relevantMemories\":[]"),
        "bos depoda beklenmeyen baglam: {empty}"
    );

    let created = invoke_with(&webview, "memory_create", memory_draft_args())
        .expect("memory_create calismali");
    let id = serde_json::from_str::<serde_json::Value>(&created).expect("JSON")["record"]["id"]
        .as_i64()
        .expect("kayit kimligi");

    let filled = invoke(&webview, "get_bootstrap_context").expect("baglam komutu calismali");
    assert!(
        filled.contains("Wake word yerel kalir"),
        "yazilan hafiza baglama girmedi: {filled}"
    );

    invoke_with(&webview, "memory_delete", serde_json::json!({ "id": id }))
        .expect("memory_delete calismali");

    // ASU-036'dan devralinan kriter: silinen hafiza bir sonraki oturumun
    // baglamina **girmez**. Onbellek yok — baglam her acilista yeniden okunur.
    let after_delete = invoke(&webview, "get_bootstrap_context").expect("baglam komutu calismali");
    assert!(
        !after_delete.contains("Wake word yerel kalir"),
        "silinen hafiza hala baglamda: {after_delete}"
    );
    assert!(
        after_delete.contains("\"relevantMemories\":[]"),
        "beklenmeyen baglam: {after_delete}"
    );
}

/// Hafiza kapaliyken baglam **bos ama durust** doner: `memoryAvailable: false`.
/// Konusma bloklanmaz, hata da uretilmez (kapali olmak bir ariza degil).
#[test]
fn the_bootstrap_context_is_empty_and_marked_when_memory_is_disabled() {
    let app = build_test_app(); // DbState::Disabled
    let webview = main_webview(&app);

    let context = invoke(&webview, "get_bootstrap_context").expect("komut hata vermemeli");
    assert!(
        context.contains("\"memoryAvailable\":false"),
        "beklenmeyen yanit: {context}"
    );
    assert!(
        context.contains("\"relevantMemories\":[]") && context.contains("\"recentSession\":null"),
        "beklenmeyen yanit: {context}"
    );
}

/// Bozuk hafiza sessizce "hatirlayacak bir sey yok"a donusmez: baglam komutu
/// da `unavailable` kodlu tipli hata verir (PROJECT.md Bolum 30).
#[test]
fn the_bootstrap_context_surfaces_a_typed_error_when_the_database_is_unavailable() {
    let app = build_test_app_with(DbState::Unavailable {
        reason: "sema migration'lari uygulanamadi".to_owned(),
    });
    let webview = main_webview(&app);

    let error = invoke(&webview, "get_bootstrap_context").expect_err("hata bekleniyordu");
    assert!(
        !is_acl_denial(&error),
        "ACL reddi degil, ariza bekleniyordu: {error}"
    );
    assert!(error.contains("unavailable"), "beklenmeyen hata: {error}");
}

/// **ASU-031 kabul kriteri**: `ASUNA_MEMORY_ENABLED=false` iken servis yazma
/// yapmaz, okuma bos doner ve uygulama calismaya devam eder.
///
/// Kritik nokta: yazma sessizce "basarili" gorunmez — yanit `skipped` der.
#[test]
fn memory_writes_are_no_ops_and_reads_are_empty_when_memory_is_disabled() {
    let app = build_test_app(); // DbState::Disabled
    let webview = main_webview(&app);

    let created = invoke_with(&webview, "memory_create", memory_draft_args())
        .expect("kapali hafiza hata degil");
    assert!(
        created.contains("\"status\":\"skipped\"")
            && created.contains("\"reason\":\"memory-disabled\""),
        "yanit: {created}"
    );

    let listed =
        invoke_with(&webview, "memory_list", serde_json::Value::Null).expect("okuma calismali");
    assert_eq!(listed, "[]");

    for (command, args) in [
        (
            "memory_update",
            serde_json::json!({ "id": 1, "patch": { "title": "x" } }),
        ),
        (
            "memory_archive",
            serde_json::json!({ "id": 1, "archived": true }),
        ),
        ("memory_delete", serde_json::json!({ "id": 1 })),
    ] {
        let response = invoke_with(&webview, command, args)
            .unwrap_or_else(|error| panic!("`{command}` kapali hafizada hata verdi: {error}"));
        assert!(
            response.contains("\"status\":\"skipped\""),
            "`{command}` yaniti: {response}"
        );
    }

    // Uygulamanin geri kalani calisiyor: hafizasiz mod urunun sonu degil.
    assert!(invoke(&webview, "get_frontend_config").is_ok());
}

/// **ASU-031 kabul kriteri**: DB hatasinda hata **gorunur** olur; sessizce bos
/// liste donmez. "Kapali" ile "bozuk" ayni gorunmemeli.
#[test]
fn memory_commands_surface_a_typed_error_when_the_database_is_unavailable() {
    let app = build_test_app_with(DbState::Unavailable {
        reason: "sema migration'lari uygulanamadi".to_owned(),
    });
    let webview = main_webview(&app);

    let error = invoke_with(&webview, "memory_list", serde_json::Value::Null)
        .expect_err("ariza hata olarak donmeli");
    assert!(
        !is_acl_denial(&error),
        "ACL reddi degil, ariza bekleniyordu: {error}"
    );
    assert!(error.contains("unavailable"), "hata: {error}");
    assert!(
        error.contains("sema migration'lari uygulanamadi"),
        "kullanici nedeni gormeli: {error}"
    );

    let error = invoke_with(&webview, "memory_create", memory_draft_args())
        .expect_err("ariza hata olarak donmeli");
    assert!(error.contains("unavailable"), "hata: {error}");
}

/// Gecersiz girdi DB'ye **hic dokunmadan** duser ve mesaj kullanici icerigini
/// tekrarlamaz.
#[test]
fn invalid_memory_input_is_rejected_at_the_ipc_boundary() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let secret = "Kullanicinin banka sifresi 1234";
    let error = invoke_with(
        &webview,
        "memory_create",
        serde_json::json!({
            "draft": {
                "kind": "decision",
                "title": "t",
                "content": secret,
                "importance": 9.0,
                "confidence": 1.0
            }
        }),
    )
    .expect_err("aralik disi importance reddedilmeli");
    assert!(!error.contains(secret), "hata icerigi sizdirdi: {error}");

    // Uydurulmus bir `kind` serde sinirinda duser.
    let error = invoke_with(
        &webview,
        "memory_create",
        serde_json::json!({
            "draft": {
                "kind": "sql_injection",
                "title": "t",
                "content": "c",
                "importance": 0.5,
                "confidence": 0.5
            }
        }),
    )
    .expect_err("bilinmeyen kind reddedilmeli");
    assert!(!is_acl_denial(&error), "hata: {error}");

    // Hicbiri yazilmamis olmali.
    let listed =
        invoke_with(&webview, "memory_list", serde_json::Value::Null).expect("okuma calismali");
    assert_eq!(listed, "[]");
}

// ---------------------------------------------------------------------------
// ASU-037 — gizlilik kontrolleri
// ---------------------------------------------------------------------------

/// **ASU-037 kabul kriteri**: anahtar calisma zamaninda kapaninca yeni yazma
/// no-op olur — yeniden baslatma yok.
///
/// Kritik nokta: yazma sessizce "basarili" gorunmez ve DB'de gercekten yeni
/// kayit olusmaz (liste ile dogrulaniyor).
#[test]
fn turning_durable_memory_off_at_runtime_stops_writes_without_a_restart() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let created = invoke_with(&webview, "memory_create", memory_draft_args())
        .expect("acikken yazma calismali");
    assert!(
        created.contains("\"status\":\"stored\""),
        "yanit: {created}"
    );

    let settings = invoke_with(
        &webview,
        "set_privacy_settings",
        serde_json::json!({ "patch": { "memoryEnabled": false } }),
    )
    .expect("kapatma kabul edilmeli");
    assert!(
        settings.contains("\"memoryEnabled\":false")
            && settings.contains("\"memoryEnabledAtBoot\":true"),
        "yanit: {settings}"
    );

    let skipped = invoke_with(&webview, "memory_create", memory_draft_args())
        .expect("kapali hafiza hata degil");
    assert!(
        skipped.contains("\"status\":\"skipped\"")
            && skipped.contains("\"reason\":\"memory-disabled\""),
        "yanit: {skipped}"
    );

    let updated = invoke_with(
        &webview,
        "memory_update",
        serde_json::json!({ "id": 1, "patch": { "title": "yeni baslik" } }),
    )
    .expect("kapali hafiza hata degil");
    assert!(
        updated.contains("\"status\":\"skipped\""),
        "yanit: {updated}"
    );

    // Onceki kayit **duruyor**: kapatmak geriye donuk veriyi silmez.
    let listed = invoke_with(&webview, "memory_list", serde_json::Value::Null).expect("okuma");
    assert!(
        listed.contains("Wake word yerel kalir"),
        "kapatmak eski kaydi yok etmis: {listed}"
    );

    // ...ve kullanici onu hala silebilir: anahtar bir tuzaga donusmez.
    let deleted = invoke_with(&webview, "memory_delete", serde_json::json!({ "id": 1 }))
        .expect("silme her zaman calismali");
    assert!(
        deleted.contains("\"status\":\"deleted\""),
        "yanit: {deleted}"
    );
}

/// **ASU-035 + ASU-037**: kullanici hafizayi calisma zamaninda kapatinca bir
/// sonraki oturum gecmisi **hatirlamaz** — baglam bos ve `memoryAvailable`
/// `false` doner.
///
/// Kayitlar silinmez ve Memory UI'da gorunmeye devam eder: incelemek ile
/// konusmaya tasimak ayri seylerdir. Anahtarin adi "durable memory"; kapatildigi
/// anda modele gecmis tasimak, kullanicinin verdigi karari bosa cikarirdi.
#[test]
fn turning_memory_off_at_runtime_empties_the_next_session_context() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    invoke_with(&webview, "memory_create", memory_draft_args()).expect("yazma calismali");
    let before = invoke(&webview, "get_bootstrap_context").expect("baglam");
    assert!(before.contains("Wake word yerel kalir"), "yanit: {before}");

    invoke_with(
        &webview,
        "set_privacy_settings",
        serde_json::json!({ "patch": { "memoryEnabled": false } }),
    )
    .expect("kapatma kabul edilmeli");

    let after = invoke(&webview, "get_bootstrap_context").expect("baglam");
    assert!(
        after.contains("\"memoryAvailable\":false") && !after.contains("Wake word yerel kalir"),
        "kapali hafiza baglami tasimaya devam ediyor: {after}"
    );

    // Kayit **duruyor**: kapatmak silmek degil (Memory UI hala gosterir).
    let listed = invoke_with(&webview, "memory_list", serde_json::Value::Null).expect("okuma");
    assert!(listed.contains("Wake word yerel kalir"), "yanit: {listed}");
}

/// Acilista kapatilmis bir anahtar calisma zamaninda **acilamaz**; istek tipli
/// bir hata ile reddedilir ve durum degismez.
#[test]
fn a_switch_disabled_by_env_cannot_be_re_enabled_over_ipc() {
    let app = build_test_app_with_privacy(DbState::Disabled, PrivacyState::from_boot(false, false));
    let webview = main_webview(&app);

    let error = invoke_with(
        &webview,
        "set_privacy_settings",
        serde_json::json!({ "patch": { "memoryEnabled": true } }),
    )
    .expect_err("gevsetme reddedilmeli");
    assert!(
        !is_acl_denial(&error),
        "ACL reddi degil bekleniyordu: {error}"
    );
    assert!(error.contains("locked-by-env"), "hata: {error}");

    let settings = invoke(&webview, "get_privacy_settings").expect("okuma calismali");
    assert!(
        settings.contains("\"memoryEnabled\":false"),
        "reddedilen istek durumu degistirmis: {settings}"
    );
}

/// **ASU-037 kabul kriteri**: "tum hafizayi sil" gercekten siler, ama yalnizca
/// onay ifadesi birebir eslesirse.
#[test]
fn deleting_all_memories_requires_the_exact_confirmation_phrase() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    invoke_with(&webview, "memory_create", memory_draft_args()).expect("kayit");
    invoke_with(&webview, "memory_create", memory_draft_args()).expect("kayit");

    for wrong in ["", "tum hafizayi sil", "TUM HAFIZAYI SIL ", "EVET"] {
        let error = invoke_with(
            &webview,
            "memory_delete_all",
            serde_json::json!({ "confirmationPhrase": wrong }),
        )
        .expect_err("yanlis ifade reddedilmeli");
        assert!(!is_acl_denial(&error), "hata: {error}");
        assert!(error.contains("invalid"), "hata tipli degil: {error}");
    }

    // Hicbiri silmedi.
    let listed = invoke_with(&webview, "memory_list", serde_json::Value::Null).expect("okuma");
    assert!(listed.contains("Wake word yerel kalir"), "yanit: {listed}");

    let purged = invoke_with(
        &webview,
        "memory_delete_all",
        serde_json::json!({ "confirmationPhrase": "TUM HAFIZAYI SIL" }),
    )
    .expect("dogru ifade kabul edilmeli");
    assert!(
        purged.contains("\"status\":\"purged\"") && purged.contains("\"deleted\":2"),
        "yanit: {purged}"
    );

    let empty = invoke_with(&webview, "memory_list", serde_json::Value::Null).expect("okuma");
    assert_eq!(empty, "[]", "yanit: {empty}");
}

/// Toplu silme, kalici hafiza calisma zamaninda kapatildiktan **sonra** da
/// calisir: gizlilik aksiyonu bir anahtarin arkasina kilitlenemez.
#[test]
fn deleting_all_memories_still_works_after_memory_is_switched_off() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    invoke_with(&webview, "memory_create", memory_draft_args()).expect("kayit");
    invoke_with(
        &webview,
        "set_privacy_settings",
        serde_json::json!({ "patch": { "memoryEnabled": false } }),
    )
    .expect("kapatma");

    let purged = invoke_with(
        &webview,
        "memory_delete_all",
        serde_json::json!({ "confirmationPhrase": "TUM HAFIZAYI SIL" }),
    )
    .expect("silme calismali");
    assert!(
        purged.contains("\"status\":\"purged\"") && purged.contains("\"deleted\":1"),
        "yanit: {purged}"
    );
}

/// Hafiza hic acilmamissa (DB yok) toplu silme bir ariza degil: `skipped`.
#[test]
fn deleting_all_memories_is_a_no_op_when_memory_was_never_opened() {
    let app = build_test_app(); // DbState::Disabled
    let webview = main_webview(&app);

    let response = invoke_with(
        &webview,
        "memory_delete_all",
        serde_json::json!({ "confirmationPhrase": "TUM HAFIZAYI SIL" }),
    )
    .expect("kapali hafiza hata degil");
    assert!(
        response.contains("\"status\":\"skipped\""),
        "yanit: {response}"
    );
}

// ---------------------------------------------------------------------------
// ASU-032 — oturum kaydi
// ---------------------------------------------------------------------------

/// Oturum acilis/kapanis akisi gercek ACL uzerinden calisiyor mu?
///
/// GIZLILIK: test config'inde `ASUNA_TRANSCRIPT_STORAGE=false` — bu test
/// **diske hicbir sey yazmaz** ve gercek uygulama veri dizinine dokunmaz.
/// Yazma yolunun kendisi `db::transcript` icinde gecici dizinle test edilir.
#[test]
fn session_commands_record_a_session_end_to_end_over_the_real_acl() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("session_start calismali");
    assert!(
        started.contains("\"status\":\"recorded\""),
        "yanit: {started}"
    );
    // Model renderer'dan degil, config'ten gelir.
    assert!(
        started.contains("\"model\":\"gpt-realtime-2.1\""),
        "yanit: {started}"
    );
    assert!(started.contains("\"endedAt\":null"), "yanit: {started}");

    let value: serde_json::Value = serde_json::from_str(&started).expect("JSON");
    let session_id = value["session"]["id"].as_i64().expect("oturum kimligi");

    let finalized = invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({
            "sessionId": session_id,
            "input": {
                "usage": { "requests": 2, "inputTokens": 120, "outputTokens": 80, "totalTokens": 200 },
                "transcript": [{ "role": "user", "text": "merhaba" }]
            }
        }),
    )
    .expect("session_finalize calismali");

    assert!(
        finalized.contains("\"totalTokens\":200"),
        "yanit: {finalized}"
    );
    assert!(
        !finalized.contains("\"endedAt\":null"),
        "yanit: {finalized}"
    );
    // `ASUNA_TRANSCRIPT_STORAGE=false` — dokum diske yazilmadi.
    assert!(
        finalized.contains("\"transcriptPath\":null"),
        "transcript kapaliyken yol yazilmis: {finalized}"
    );

    // ASU-033: kapanis nedeni makine-okunur alanda, ozet alani bos. Ozet
    // uretimi kapanisi **bloklamaz**; servis kayitli olmadigi icin (bkz. modul
    // dokumantasyonu) burada ag'a da cikilmaz.
    assert!(
        finalized.contains("\"endReason\":\"completed\""),
        "kapanis nedeni yazilmamis: {finalized}"
    );
    assert!(
        finalized.contains("\"summary\":null"),
        "ozet kapanisi bekletmemeli: {finalized}"
    );
}

/// Renderer bir oturumu "kurtarilmis" ilan edemez; `error` bildirebilir.
#[test]
fn the_renderer_can_report_an_error_but_not_an_abandoned_session() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("session_start calismali");
    let value: serde_json::Value = serde_json::from_str(&started).expect("JSON");
    let session_id = value["session"]["id"].as_i64().expect("oturum kimligi");

    let finalized = invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({ "sessionId": session_id, "input": { "endReason": "error" } }),
    )
    .expect("hata ile kapanis kabul edilmeli");
    assert!(
        finalized.contains("\"endReason\":\"error\""),
        "yanit: {finalized}"
    );

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("session_start calismali");
    let value: serde_json::Value = serde_json::from_str(&started).expect("JSON");
    let session_id = value["session"]["id"].as_i64().expect("oturum kimligi");

    let error = invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({ "sessionId": session_id, "input": { "endReason": "abandoned" } }),
    )
    .expect_err("`abandoned` renderer'dan gelemez");
    assert!(!is_acl_denial(&error), "hata: {error}");
}

/// Renderer oturum modelini secemez: sozlesmede boyle bir alan yok, gonderirse
/// istek reddedilir (`deny_unknown_fields`).
#[test]
fn the_renderer_cannot_choose_the_session_model_or_transcript_path() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    for args in [
        serde_json::json!({ "model": "gpt-4o-realtime-ucuz" }),
        serde_json::json!({ "projectId": "asuna", "model": "baska-model" }),
    ] {
        let response = invoke_with(&webview, "session_start", args);
        // Fazladan alan komut imzasinda yok: ya yok sayilir ya reddedilir —
        // her iki durumda da model config'ten gelen deger olmali.
        if let Ok(body) = response {
            assert!(
                body.contains("\"model\":\"gpt-realtime-2.1\""),
                "renderer modeli ezdi: {body}"
            );
        }
    }

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("session_start calismali");
    let value: serde_json::Value = serde_json::from_str(&started).expect("JSON");
    let session_id = value["session"]["id"].as_i64().expect("oturum kimligi");

    let error = invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({
            "sessionId": session_id,
            "input": { "transcriptPath": "/Users/kurban/.ssh/id_ed25519" }
        }),
    )
    .expect_err("renderer transcript yolu veremez");
    assert!(!is_acl_denial(&error), "hata: {error}");
}

/// **Gate 3 / CRITICAL-1**: kalici hafiza **calisma zamaninda** kapatilinca
/// oturum kaydi da durur — `memory_create` ile ayni davranis.
///
/// Uc sey birlikte olculuyor:
///
/// 1. `session_start` `skipped` doner (renderer kimlik almaz → kapanista yazma
///    denenmez).
/// 2. `session_finalize` **var olmayan** bir kimlikle bile `skipped` doner:
///    kapili anahtarda DB'ye hic dokunulmuyor. Kapi olmasaydi ayni cagri
///    `not-found` hatasi verirdi — yani sorgu kosardi.
/// 3. Anahtar geri acilinca yeni oturumun kimligi **2** olur: kapaliyken
///    tabloya satir eklenmedigi buradan gorunur (rowid ilerlememis).
#[test]
fn turning_memory_off_at_runtime_stops_session_writes_without_a_restart() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("acikken kayit calismali");
    let first_id = serde_json::from_str::<serde_json::Value>(&started).expect("JSON")["session"]
        ["id"]
        .as_i64()
        .expect("oturum kimligi");
    assert_eq!(first_id, 1);

    invoke_with(
        &webview,
        "set_privacy_settings",
        serde_json::json!({ "patch": { "memoryEnabled": false } }),
    )
    .expect("kapatma kabul edilmeli");

    let skipped = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("kapali hafiza hata degil");
    assert!(
        skipped.contains("\"status\":\"skipped\"")
            && skipped.contains("\"reason\":\"memory-disabled\""),
        "yanit: {skipped}"
    );

    let unknown = invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({ "sessionId": 4_242 }),
    )
    .expect("kapali hafizada finalize hata degil");
    assert!(
        unknown.contains("\"status\":\"skipped\""),
        "DB'ye dokunulmus olmali (not-found beklenmiyordu): {unknown}"
    );

    let finalized = invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({
            "sessionId": first_id,
            "input": { "transcript": [{ "role": "user", "text": "gizli konusma" }] }
        }),
    )
    .expect("kapali hafizada finalize hata degil");
    assert!(
        finalized.contains("\"status\":\"skipped\""),
        "yanit: {finalized}"
    );

    // Anahtar geri acilir: yeni kimlik 2 ise kapaliyken satir eklenmemistir.
    invoke_with(
        &webview,
        "set_privacy_settings",
        serde_json::json!({ "patch": { "memoryEnabled": true } }),
    )
    .expect("acilista acik oldugu icin geri acilabilir");

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("session_start calismali");
    let second_id = serde_json::from_str::<serde_json::Value>(&started).expect("JSON")["session"]
        ["id"]
        .as_i64()
        .expect("oturum kimligi");
    assert_eq!(
        second_id, 2,
        "kapaliyken oturum satiri acilmis (rowid ilerledi)"
    );

    // Ilk oturum hala **acik**: kapaliyken gelen kapanis yazilmadi.
    let finalized = invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({ "sessionId": first_id }),
    )
    .expect("acikken kapanis calismali");
    assert!(
        finalized.contains("\"status\":\"recorded\"") && finalized.contains("\"summary\":null"),
        "yanit: {finalized}"
    );
}

// ---------------------------------------------------------------------------
// ASU-065 — oturum ozeti + dokum temizligi
// ---------------------------------------------------------------------------

/// Test config'inde `ASUNA_TRANSCRIPT_STORAGE=false` oldugu icin hicbir oturum
/// kaydinda dokum yolu yoktur; dolayisiyla asagidaki testler **dosya sistemine
/// dokunmaz**.
///
/// `session_clear_all`'in **basarili** yolu bilerek burada kosulmuyor: mock
/// uygulama gercek `app_data_dir()`'i cozer (identifier `tauri.conf.json`'dan
/// gelir) ve komut o dizindeki `transcripts/` klasorunu temizler — yani test
/// kullanicinin gercek dokumlerini silerdi. Diskteki davranis gecici dizinle
/// `db::transcript` testlerinde, DB tarafi `db::session_repository`
/// testlerinde olculuyor. Burada olculen sey ACL kapisi ve onay ifadesi.
#[test]
fn session_history_can_be_listed_and_deleted_over_the_real_acl() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let empty = invoke_with(&webview, "session_list", serde_json::Value::Null)
        .expect("session_list calismali");
    assert!(
        empty.contains("\"sessions\":[]") && empty.contains("\"total\":0"),
        "bos depoda beklenmeyen yanit: {empty}"
    );
    // Tavan gorunur: UI "hepsi bu kadar" diye tahmin yurutmez.
    assert!(
        empty.contains("\"limit\":50") && empty.contains("\"limitMax\":200"),
        "sinirlar yanitta yok: {empty}"
    );

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("session_start calismali");
    let session_id = serde_json::from_str::<serde_json::Value>(&started).expect("JSON")["session"]
        ["id"]
        .as_i64()
        .expect("oturum kimligi");
    invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({ "sessionId": session_id }),
    )
    .expect("session_finalize calismali");

    let listed = invoke_with(&webview, "session_list", serde_json::json!({}))
        .expect("session_list calismali");
    assert!(listed.contains("\"total\":1"), "yanit: {listed}");
    // GIZLILIK: liste dosya yolu tasimaz.
    assert!(
        listed.contains("\"hasTranscriptFile\":false") && !listed.contains("transcriptPath"),
        "yanit: {listed}"
    );

    let deleted = invoke_with(
        &webview,
        "session_delete",
        serde_json::json!({ "sessionId": session_id }),
    )
    .expect("session_delete calismali");
    assert!(
        deleted.contains("\"status\":\"deleted\"")
            && deleted.contains("\"transcriptFile\":\"not-recorded\""),
        "yanit: {deleted}"
    );

    let after = invoke_with(&webview, "session_list", serde_json::Value::Null).expect("okuma");
    assert!(after.contains("\"total\":0"), "yanit: {after}");

    // Silinmis oturum bir daha silinemez: sessizce "sildim" denmez.
    let error = invoke_with(
        &webview,
        "session_delete",
        serde_json::json!({ "sessionId": session_id }),
    )
    .expect_err("ikinci silme hata vermeli");
    assert!(!is_acl_denial(&error), "hata: {error}");
    assert!(error.contains("not-found"), "hata tipli degil: {error}");
}

/// **ASU-065 / M3 blokaji**: silinen oturumun **ozeti** bir sonraki oturumun
/// baglamina girmez.
///
/// M3 kabul testinde yakalanan acik tam buydu: kullanici hafiza kayitlarini
/// sildi ama Asuna hatirlamaya devam etti, cunku Stage A her acilista son
/// oturum ozetini enjekte ediyordu ve `sessions.summary` silinemiyordu.
/// Burada ozet `session_finalize` sonrasi arka plan gorevi olmadan (ozet
/// servisi bilerek `manage` edilmiyor) uretilemeyecegi icin dogrudan
/// repository ile yazilir; olculen sey **baglamin silmeyi yansitmasi**.
#[test]
fn a_deleted_session_summary_leaves_the_next_bootstrap_context() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("session_start calismali");
    let session_id = serde_json::from_str::<serde_json::Value>(&started).expect("JSON")["session"]
        ["id"]
        .as_i64()
        .expect("oturum kimligi");
    invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({ "sessionId": session_id }),
    )
    .expect("session_finalize calismali");

    // Ozet normalde arka planda uretilir (ASU-033); testte ag'a cikilmadigi
    // icin ayni yazma dogrudan yapilir.
    let db_state = app.state::<DbState>();
    let db = db_state
        .access()
        .expect("hafiza acik olmali")
        .expect("DB olmali");
    crate::db::session_repository::attach_summary(
        db,
        session_id,
        "Konusulanlar: wake word yerel kalir.",
        None,
    )
    .expect("ozet yazilmali");

    let before = invoke(&webview, "get_bootstrap_context").expect("baglam");
    assert!(
        before.contains("wake word yerel kalir"),
        "ozet baglama girmedi: {before}"
    );

    invoke_with(
        &webview,
        "session_delete",
        serde_json::json!({ "sessionId": session_id }),
    )
    .expect("session_delete calismali");

    let after = invoke(&webview, "get_bootstrap_context").expect("baglam");
    assert!(
        !after.contains("wake word yerel kalir") && after.contains("\"recentSession\":null"),
        "silinen oturum ozeti hala baglamda: {after}"
    );
}

/// Toplu temizlik yanlis onay ifadesiyle **hicbir seye dokunmaz**.
///
/// Ifade kontrolu komutun ilk satiri: reddedilen bir cagri ne DB'ye ne dokum
/// dizinine erisir (bu yuzden bu test diskte de guvenlidir, bkz. yukaridaki
/// gerekce).
#[test]
fn clearing_the_session_history_requires_the_exact_confirmation_phrase() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    invoke_with(&webview, "session_start", serde_json::json!({})).expect("oturum");

    for wrong in [
        "",
        "konusma gecmisini sil",
        "KONUSMA GECMISINI SIL ",
        // Hafiza silmenin ifadesi burada calismaz: iki aksiyon ayri.
        "TUM HAFIZAYI SIL",
    ] {
        let error = invoke_with(
            &webview,
            "session_clear_all",
            serde_json::json!({ "confirmationPhrase": wrong }),
        )
        .expect_err("yanlis ifade reddedilmeli");
        assert!(!is_acl_denial(&error), "hata: {error}");
        assert!(error.contains("invalid"), "hata tipli degil: {error}");
    }

    // Hicbiri silmedi.
    let listed = invoke_with(&webview, "session_list", serde_json::Value::Null).expect("okuma");
    assert!(listed.contains("\"total\":1"), "yanit: {listed}");
}

/// Hafiza kapaliyken okuma **bos sayfa**, tek silme `skipped` doner; ikisi de
/// hata degil (`memory_list` / `memory_delete` ile ayni sozlesme).
#[test]
fn session_history_commands_are_no_ops_when_memory_is_disabled() {
    let app = build_test_app(); // DbState::Disabled
    let webview = main_webview(&app);

    let listed = invoke_with(&webview, "session_list", serde_json::Value::Null)
        .expect("kapali hafiza hata degil");
    assert!(
        listed.contains("\"sessions\":[]") && listed.contains("\"total\":0"),
        "yanit: {listed}"
    );

    let deleted = invoke_with(
        &webview,
        "session_delete",
        serde_json::json!({ "sessionId": 1 }),
    )
    .expect("kapali hafiza hata degil");
    assert!(
        deleted.contains("\"status\":\"skipped\"")
            && deleted.contains("\"reason\":\"memory-disabled\""),
        "yanit: {deleted}"
    );
}

/// Bozuk hafiza sessizce "gecmis yok"a donusmez: liste de tipli hata verir.
#[test]
fn session_list_surfaces_a_typed_error_when_the_database_is_unavailable() {
    let app = build_test_app_with(DbState::Unavailable {
        reason: "sema migration'lari uygulanamadi".to_owned(),
    });
    let webview = main_webview(&app);

    let error = invoke_with(&webview, "session_list", serde_json::Value::Null)
        .expect_err("ariza hata olarak donmeli");
    assert!(
        !is_acl_denial(&error),
        "ACL reddi degil, ariza bekleniyordu: {error}"
    );
    assert!(error.contains("unavailable"), "hata: {error}");
}

/// **ASU-032 kabul kriteri**: hafiza kapaliyken oturum kaydi olusmaz ve
/// uygulama calismaya devam eder.
#[test]
fn session_commands_are_no_ops_when_memory_is_disabled() {
    let app = build_test_app(); // DbState::Disabled
    let webview = main_webview(&app);

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("kapali hafiza hata degil");
    assert!(
        started.contains("\"status\":\"skipped\"")
            && started.contains("\"reason\":\"memory-disabled\""),
        "yanit: {started}"
    );

    let finalized = invoke_with(
        &webview,
        "session_finalize",
        serde_json::json!({ "sessionId": 1 }),
    )
    .expect("kapali hafiza hata degil");
    assert!(
        finalized.contains("\"status\":\"skipped\""),
        "yanit: {finalized}"
    );
}

// ---------------------------------------------------------------------------
// ASU-050 — tool audit defteri
// ---------------------------------------------------------------------------

/// Gecerli bir audit girdisi (risk 0, onaysiz calisan bir tool).
fn tool_event_args(approval_state: &str) -> serde_json::Value {
    serde_json::json!({
        "input": {
            "toolName": "get_current_project",
            "riskLevel": 0,
            "approvalState": approval_state,
        }
    })
}

/// **ASU-050 kabul kaniti** — yazma ve okuma gercek ACL uzerinden, renderer'in
/// gonderdigi istegin aynisiyla.
#[test]
fn the_tool_audit_log_records_and_lists_over_the_real_acl() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let empty = invoke_with(&webview, "tool_event_list", serde_json::Value::Null)
        .expect("tool_event_list calismali");
    assert!(
        empty.contains("\"events\":[]") && empty.contains("\"total\":0"),
        "bos defterde beklenmeyen yanit: {empty}"
    );
    // Tavan gorunur: UI "hepsi bu kadar" diye tahmin yurutmez.
    assert!(
        empty.contains("\"limit\":50") && empty.contains("\"limitMax\":200"),
        "sinirlar yanitta yok: {empty}"
    );

    let started = invoke_with(&webview, "session_start", serde_json::json!({}))
        .expect("session_start calismali");
    let session_id = serde_json::from_str::<serde_json::Value>(&started).expect("JSON")["session"]
        ["id"]
        .as_i64()
        .expect("oturum kimligi");

    let recorded = invoke_with(
        &webview,
        "record_tool_event",
        serde_json::json!({
            "input": {
                "sessionId": session_id,
                "toolName": "open_project",
                "riskLevel": 1,
                "arguments": { "projectId": "asuna" },
                "approvalState": "approved",
                "resultSummary": "Proje VS Code ile acildi."
            }
        }),
    )
    .expect("record_tool_event calismali");
    assert!(
        recorded.contains("\"status\":\"recorded\""),
        "yanit: {recorded}"
    );
    assert!(
        recorded.contains("\"argumentsRedacted\":\"projectId=asuna\""),
        "yanit: {recorded}"
    );
    assert!(recorded.contains("\"riskLevel\":1"), "yanit: {recorded}");

    let listed = invoke_with(&webview, "tool_event_list", serde_json::json!({}))
        .expect("tool_event_list calismali");
    assert!(listed.contains("\"total\":1"), "yanit: {listed}");
    assert!(listed.contains("open_project"), "yanit: {listed}");

    // Oturum filtresi: oturum detayi ekrani bunu kullanacak (ASU-054).
    let filtered = invoke_with(
        &webview,
        "tool_event_list",
        serde_json::json!({ "query": { "sessionId": session_id } }),
    )
    .expect("filtreli liste calismali");
    assert!(filtered.contains("\"total\":1"), "yanit: {filtered}");

    let other = invoke_with(
        &webview,
        "tool_event_list",
        serde_json::json!({ "query": { "sessionId": 9_999 } }),
    )
    .expect("bos filtre hata degil");
    assert!(
        other.contains("\"events\":[]") && other.contains("\"total\":0"),
        "yanit: {other}"
    );
}

/// **ASU-050 kabul kriteri**: reddedilen, zaman asimina ugrayan ve onaya hic
/// gitmeyen cagrilar da yaziliyor — hepsi gercek ACL uzerinden.
#[test]
fn refused_and_timed_out_tool_calls_are_audited_too() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    for state in [
        "not_required",
        "auto_approved",
        "approved",
        "denied",
        "timeout",
        "not_requested",
    ] {
        let response = invoke_with(&webview, "record_tool_event", tool_event_args(state))
            .unwrap_or_else(|error| panic!("`{state}` yazilmali: {error}"));
        assert!(
            response.contains(&format!("\"approvalState\":\"{state}\"")),
            "yanit: {response}"
        );
    }

    let listed = invoke_with(&webview, "tool_event_list", serde_json::json!({}))
        .expect("tool_event_list calismali");
    assert!(listed.contains("\"total\":6"), "yanit: {listed}");
    for state in ["denied", "timeout", "not_requested"] {
        assert!(
            listed.contains(&format!("\"approvalState\":\"{state}\"")),
            "`{state}` defterde yok: {listed}"
        );
    }

    // Uydurulmus bir onay durumu IPC sinirinde duser; DB'ye dokunulmaz.
    let error = invoke_with(&webview, "record_tool_event", tool_event_args("onaylandi"))
        .expect_err("bilinmeyen approvalState reddedilmeli");
    assert!(!is_acl_denial(&error), "hata: {error}");

    // Aralik disi risk seviyesi de.
    let error = invoke_with(
        &webview,
        "record_tool_event",
        serde_json::json!({
            "input": { "toolName": "x", "riskLevel": 7, "approvalState": "approved" }
        }),
    )
    .expect_err("aralik disi riskLevel reddedilmeli");
    assert!(!is_acl_denial(&error), "hata: {error}");

    let listed = invoke_with(&webview, "tool_event_list", serde_json::json!({}))
        .expect("tool_event_list calismali");
    assert!(
        listed.contains("\"total\":6"),
        "reddedilen istekler yazilmis: {listed}"
    );
}

/// **ASU-050 kabul kriteri**: argumanlar redakte ediliyor; dosya icerigi ve
/// secret'lar audit'e girmiyor.
///
/// `tool_event_repository` bunu birim testinde de kanitliyor; buradaki kontrol
/// renderer'in gercekten gordugu payload uzerinde — ve **renderer'in hazir bir
/// ozet gonderemedigini** de olcuyor.
#[test]
fn tool_arguments_are_redacted_on_the_host_side_over_ipc() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let recorded = invoke_with(
        &webview,
        "record_tool_event",
        serde_json::json!({
            "input": {
                "toolName": "read_project_file",
                "riskLevel": 0,
                "arguments": {
                    "apiKey": "sk-proj-SIZMAMALI-DEGER",
                    "password": "hunter2",
                    "file": { "content": "OPENAI_API_KEY=cok-gizli-deger\nikinci satir" }
                },
                "approvalState": "not_required",
                "resultSummary": "Anahtar sk-proj-SIZMAMALI-DEGER kullanildi."
            }
        }),
    )
    .expect("record_tool_event calismali");

    assert!(
        !recorded.contains("SIZMAMALI-DEGER"),
        "kalici anahtar audit'e girdi: {recorded}"
    );
    assert!(
        !recorded.contains("hunter2"),
        "parola audit'e girdi: {recorded}"
    );
    assert!(
        !recorded.contains("cok-gizli-deger"),
        "dosya icerigi audit'e girdi: {recorded}"
    );
    // Ic ice yapi yalnizca **sekil** olarak gorunur.
    assert!(recorded.contains("file={1 alan}"), "yanit: {recorded}");

    // Ayni sey listede de gecerli (kayit gercekten redakte yazildi, yanit
    // suslenmedi).
    let listed = invoke_with(&webview, "tool_event_list", serde_json::json!({}))
        .expect("tool_event_list calismali");
    assert!(!listed.contains("SIZMAMALI-DEGER"), "yanit: {listed}");
    assert!(!listed.contains("hunter2"), "yanit: {listed}");

    // Renderer hazir bir "redakte edilmis" metin gonderemez: sozlesmede boyle
    // bir alan yok (`deny_unknown_fields`).
    let error = invoke_with(
        &webview,
        "record_tool_event",
        serde_json::json!({
            "input": {
                "toolName": "read_project_file",
                "riskLevel": 0,
                "argumentsRedacted": "path=zararsiz.md",
                "approvalState": "not_required"
            }
        }),
    )
    .expect_err("renderer arguman ozetini kendisi yazamaz");
    assert!(!is_acl_denial(&error), "hata: {error}");
}

/// **ASU-050 kabul kriteri**: audit kayitlari uygulamadan silinemiyor.
///
/// Silme/guncelleme komutu ACL'de **yok**; boyle bir cagri deny-by-default ile
/// duser. `commands.rs` ayni kurali statik olarak da kilitliyor.
#[test]
fn the_tool_audit_log_has_no_delete_or_update_surface() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    invoke_with(&webview, "record_tool_event", tool_event_args("approved"))
        .expect("kayit yazilmali");

    for command in [
        "tool_event_delete",
        "tool_event_update",
        "tool_event_clear_all",
        "tool_event_purge",
        "tool_event_archive",
    ] {
        let error = invoke_with(&webview, command, serde_json::json!({ "id": 1 }))
            .expect_err("audit silme/guncelleme yuzeyi olmamali");
        assert!(
            is_acl_denial(&error),
            "`{command}` renderer'a acik: {error}"
        );
    }

    // Var olan hicbir toplu temizlik komutu da audit'e dokunmaz: oturum
    // gecmisini silmek denetim defterini silmez (FK `ON DELETE SET NULL`).
    invoke_with(
        &webview,
        "session_clear_all",
        serde_json::json!({ "confirmationPhrase": "KONUSMA GECMISINI SIL" }),
    )
    .ok();
    invoke_with(
        &webview,
        "memory_delete_all",
        serde_json::json!({ "confirmationPhrase": "TUM HAFIZAYI SIL" }),
    )
    .expect("hafiza temizligi calismali");

    let listed = invoke_with(&webview, "tool_event_list", serde_json::json!({}))
        .expect("tool_event_list calismali");
    assert!(
        listed.contains("\"total\":1"),
        "audit defteri temizlikle birlikte silinmis: {listed}"
    );
}

/// Hafiza kapaliyken audit yazimi `skipped`, okuma bos sayfa doner; ikisi de
/// hata degil. Sonuc kullaniciya gorunur: kalici audit izi tutulmaz.
#[test]
fn tool_audit_commands_are_no_ops_when_memory_is_disabled() {
    let app = build_test_app(); // DbState::Disabled
    let webview = main_webview(&app);

    let recorded = invoke_with(&webview, "record_tool_event", tool_event_args("approved"))
        .expect("kapali hafiza hata degil");
    assert!(
        recorded.contains("\"status\":\"skipped\"")
            && recorded.contains("\"reason\":\"memory-disabled\""),
        "yanit: {recorded}"
    );

    let listed = invoke_with(&webview, "tool_event_list", serde_json::Value::Null)
        .expect("kapali hafiza hata degil");
    assert!(
        listed.contains("\"events\":[]") && listed.contains("\"total\":0"),
        "yanit: {listed}"
    );
}

/// **ASU-050 kabul kriteri**: audit yazimi basarisiz olursa sessizce
/// kaybolmaz — komut tipli bir hata doner.
///
/// Burada ariza `DbState::Unavailable` ile temsil ediliyor: "audit tutulamadi"
/// ile "audit bos" ayni cevap degildir (PROJECT.md Bolum 30).
#[test]
fn a_failed_audit_write_surfaces_a_typed_error_instead_of_vanishing() {
    let app = build_test_app_with(DbState::Unavailable {
        reason: "sema migration'lari uygulanamadi".to_owned(),
    });
    let webview = main_webview(&app);

    let error = invoke_with(&webview, "record_tool_event", tool_event_args("approved"))
        .expect_err("ariza hata olarak donmeli");
    assert!(
        !is_acl_denial(&error),
        "ACL reddi degil, ariza bekleniyordu: {error}"
    );
    assert!(error.contains("unavailable"), "hata: {error}");

    let error = invoke_with(&webview, "tool_event_list", serde_json::Value::Null)
        .expect_err("ariza hata olarak donmeli");
    assert!(error.contains("unavailable"), "hata: {error}");
}

// ---------------------------------------------------------------------------
// ASU-045 devri — dialog plugin kilidi
// ---------------------------------------------------------------------------

/// **ASU-045 kabul kaniti (ASU-050 devri)**: dialog plugin'i yukludur ama
/// yalnizca `open` acilmistir.
///
/// Plugin `build_test_app_with_privacy` icinde uretimdekiyle **ayni sekilde**
/// yukleniyor; dolayisiyla asagidaki redler "plugin yok" degil, **ACL reddi**.
/// Ayrimi olcen sey `commands.rs` icindeki statik esi: orada `asuna-dialog.json`
/// izin listesinin tam olarak `["dialog:allow-open"]` oldugu dogrulaniyor.
///
/// Neden bu izinler kapali:
///
/// - `save` bir dosya YAZMA hedefi sectirir. Dizin secici bir okuma yetkisi
///   bile degil (donen sey yalnizca bir metin yoldur); yazma hedefi sectirmek
///   bu ekranin isi degil ve `fs` izni zaten yok.
/// - `message` / `ask` / `confirm` WKWebView'de **modal sistem penceresi** acar
///   ve ses oturumunu kilitler. Asuna'nin onaylari uygulama icinde, iptal
///   edilebilir ve klavyeyle erisilebilir bir satir icinde alinir (ASU-053).
#[test]
fn the_dialog_plugin_only_exposes_the_directory_picker() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    // Plugin'in gercekten var olan iki komutu. Red **ACL'den** gelmeli:
    // mesaj eksik izni adlandirir (`dialog.save not allowed. Permissions
    // associated with this command: dialog:allow-save, dialog:default`).
    // Plugin yuklu olmasaydi mesaj "Plugin not found" olurdu ve bu test hicbir
    // sey kanitlamazdi — ayrim testin can alici noktasi.
    for (command, permission) in [
        ("plugin:dialog|save", "dialog:allow-save"),
        ("plugin:dialog|message", "dialog:allow-message"),
    ] {
        let error = invoke_with(&webview, command, serde_json::json!({}))
            .expect_err("`{command}` reddedilmeli");
        assert!(
            is_acl_denial(&error),
            "`{command}` renderer'a acik: {error}"
        );
        assert!(
            error.contains(permission),
            "`{command}` reddi ACL izniyle iliskilendirilmemis: {error}"
        );
    }

    // `ask` / `confirm` plugin'in 2.x komut yuzeyinde zaten yok (`message`
    // uzerine kurulu JS yardimcilaridir): red "Command not found" olur.
    // Yine de aranyor — bir gun komut haline gelirlerse sessizce acilmasinlar.
    for command in ["plugin:dialog|ask", "plugin:dialog|confirm"] {
        let error = invoke_with(&webview, command, serde_json::json!({}))
            .expect_err("`{command}` reddedilmeli");
        assert!(
            is_acl_denial(&error),
            "`{command}` renderer'a acik: {error}"
        );
    }

    // Dosya sistemi plugin'i hic yuklu degil: dizin secici bir `fs` yetkisi
    // degildir ve o kapi ayrica kapali.
    for command in ["plugin:fs|read_file", "plugin:fs|write_text_file"] {
        let error = invoke_with(&webview, command, serde_json::json!({}))
            .expect_err("`{command}` reddedilmeli");
        assert!(
            is_acl_denial(&error),
            "`{command}` renderer'a acik: {error}"
        );
    }
}

// ---------------------------------------------------------------------------
// Proje kayitlari (ASU-040)
// ---------------------------------------------------------------------------

/// **ASU-040 kabul kaniti** — gercek ACL uzerinden, renderer'in gonderdigi
/// istegin aynisiyla: kayit → listeleme → guncel proje secimi → kaldirma.
#[test]
fn the_project_registry_works_end_to_end_over_the_real_acl() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    // Kayitli proje yokken liste bos — Asuna proje uydurmaz.
    assert_eq!(
        invoke(&webview, "project_list").expect("liste calismali"),
        "[]"
    );

    let directory = std::env::temp_dir().join(format!("asuna-acl-projects-{}", std::process::id()));
    let root = directory.join("asuna");
    std::fs::create_dir_all(&root).expect("gecici proje dizini");

    let added = invoke_with(
        &webview,
        "project_add",
        serde_json::json!({ "path": root.to_str().expect("UTF-8 yol") }),
    )
    .expect("kayit calismali");
    assert!(
        added.contains("\"status\":\"registered\""),
        "yanit: {added}"
    );
    assert!(added.contains("\"id\":\"asuna\""), "yanit: {added}");
    // Kayit "guncel proje" secimi degildir.
    assert!(added.contains("\"lastOpenedAt\":null"), "yanit: {added}");

    // Ayni dizin ikinci kez: hata degil, ama yeni satir da acilmaz.
    let again = invoke_with(
        &webview,
        "project_add",
        serde_json::json!({ "path": root.to_str().expect("UTF-8 yol") }),
    )
    .expect("ikinci cagri hata olmamali");
    assert!(
        again.contains("\"status\":\"already-registered\""),
        "yanit: {again}"
    );

    let selected = invoke_with(
        &webview,
        "project_set_current",
        serde_json::json!({ "projectId": "asuna" }),
    )
    .expect("secim calismali");
    assert!(
        !selected.contains("\"lastOpenedAt\":null"),
        "guncel proje secimi zaman damgasi yazmali: {selected}"
    );

    let removed = invoke_with(
        &webview,
        "project_remove",
        serde_json::json!({ "projectId": "asuna" }),
    )
    .expect("kaldirma calismali");
    assert!(
        removed.contains("\"status\":\"deleted\""),
        "yanit: {removed}"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// Renderer gecerli olmayan bir yol gonderirse istek **IPC sinirinda** duser;
/// DB'ye hicbir sey yazilmaz ve hata tipli doner.
#[test]
fn an_unusable_project_path_is_refused_at_the_ipc_boundary() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    for (path, expected_code) in [
        ("gorece/yol", "path-refused"),
        ("~/Work/asuna", "path-refused"),
        ("/", "path-refused"),
        ("/bu/yol/kesinlikle/yok", "path-not-found"),
    ] {
        let error = invoke_with(&webview, "project_add", serde_json::json!({ "path": path }))
            .expect_err("gecersiz yol reddedilmeli");
        assert!(
            !is_acl_denial(&error),
            "ACL reddi degil, dogrulama hatasi bekleniyordu: {error}"
        );
        assert!(
            error.contains(expected_code),
            "`{path}` icin `{expected_code}` bekleniyordu: {error}"
        );
    }

    assert_eq!(
        invoke(&webview, "project_list").expect("liste calismali"),
        "[]",
        "reddedilen yollar kayit acmamali"
    );
}

/// Kalici depolama kapaliyken "proje eklendi" demek yalan olurdu: komut sessizce
/// atlamak yerine tipli hata doner (`memory_create` ile bilerek farkli).
#[test]
fn project_commands_report_disabled_storage_instead_of_pretending() {
    let app = build_test_app(); // DbState::Disabled
    let webview = main_webview(&app);

    let error = invoke_with(
        &webview,
        "project_add",
        serde_json::json!({ "path": "/tmp" }),
    )
    .expect_err("kapali depolamada kayit tutulamaz");
    assert!(!is_acl_denial(&error), "hata: {error}");
    assert!(error.contains("disabled"), "hata: {error}");

    let error = invoke(&webview, "project_list").expect_err("kapali depolamada liste yok");
    assert!(error.contains("disabled"), "hata: {error}");
}

/// Bozuk hafiza sessizce "kayitli proje yok"a donusmez.
#[test]
fn project_list_surfaces_a_typed_error_when_the_database_is_unavailable() {
    let app = build_test_app_with(DbState::Unavailable {
        reason: "sema migration'lari uygulanamadi".to_owned(),
    });
    let webview = main_webview(&app);

    let error = invoke(&webview, "project_list").expect_err("ariza hata olarak donmeli");
    assert!(!is_acl_denial(&error), "hata: {error}");
    assert!(error.contains("unavailable"), "hata: {error}");
}

// ---------------------------------------------------------------------------
// Guncel proje baglami (ASU-044)
// ---------------------------------------------------------------------------

/// **ASU-044 kabul kaniti** — `get_current_project` tool'unun arkasindaki komut
/// gercek ACL uzerinden calisiyor ve **uc belirsizlik nedenini** ayri ayri
/// donuyor. Asuna'nin soracagi soru her birinde farkli; tek bir "bilmiyorum"
/// kovasi modeli proje uydurmaya iterdi.
#[test]
fn project_context_reports_each_reason_it_cannot_name_a_project() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    // 1) Hic proje kaydedilmemis.
    let response = invoke(&webview, "project_context").expect("komut calismali");
    assert!(
        response.contains("\"status\":\"unknown\"")
            && response.contains("\"reason\":\"no-registered-project\""),
        "yanit: {response}"
    );

    let directory = std::env::temp_dir().join(format!("asuna-acl-context-{}", std::process::id()));
    let root = directory.join("asuna");
    std::fs::create_dir_all(&root).expect("gecici proje dizini");
    std::fs::write(
        root.join("README.md"),
        "# Asuna\n\nSesli kisisel AI companion.\n",
    )
    .expect("README yazilmali");

    invoke_with(
        &webview,
        "project_add",
        serde_json::json!({ "path": root.to_str().expect("UTF-8 yol") }),
    )
    .expect("kayit calismali");

    // 2) Proje var ama secilmemis — kayit bir secim degildir.
    let response = invoke(&webview, "project_context").expect("komut calismali");
    assert!(
        response.contains("\"reason\":\"no-current-selection\""),
        "yanit: {response}"
    );

    // 3) Secim yapildi: artik proje biliniyor.
    invoke_with(
        &webview,
        "project_set_current",
        serde_json::json!({ "projectId": "asuna" }),
    )
    .expect("secim calismali");

    let response = invoke(&webview, "project_context").expect("komut calismali");
    assert!(
        response.contains("\"status\":\"known\""),
        "yanit: {response}"
    );
    assert!(response.contains("\"name\":\"asuna\""), "yanit: {response}");
    assert!(response.contains("\"README.md\""), "yanit: {response}");
    // Cikti tavani olculuyor ve donuyor (kabul kriteri: "cikti boyutu sinirli").
    assert!(response.contains("\"maxChars\":"), "yanit: {response}");
    assert!(response.contains("\"totalChars\":"), "yanit: {response}");
    // Devir teslim dosyasi yok: hata degil, `absent`.
    assert!(
        response.contains("\"handoff\":{\"status\":\"absent\"}"),
        "yanit: {response}"
    );

    // 4) Kok kaybolursa cevap "bilmiyorum" olur, eski ozet tekrar edilmez.
    std::fs::remove_dir_all(&root).expect("kok silinmeli");
    let response = invoke(&webview, "project_context").expect("komut calismali");
    assert!(
        response.contains("\"reason\":\"root-missing\""),
        "yanit: {response}"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// `.env` iceriginin tool ciktisina sizmadigini **IPC sinirinda** dogrular.
/// `context.rs` bunu birim testinde de kanitliyor; buradaki kontrol renderer'in
/// gercekten gordugu payload uzerinde.
#[test]
fn project_context_never_leaks_dotenv_contents_over_ipc() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let directory =
        std::env::temp_dir().join(format!("asuna-acl-context-env-{}", std::process::id()));
    let root = directory.join("gizli");
    std::fs::create_dir_all(&root).expect("gecici proje dizini");
    std::fs::write(
        root.join(".env"),
        "OPENAI_API_KEY=sk-proj-SIZMAMALI-DEGER\n",
    )
    .expect(".env yazilmali");
    std::fs::write(root.join("README.md"), "# Gizli\n").expect("README yazilmali");

    invoke_with(
        &webview,
        "project_add",
        serde_json::json!({ "path": root.to_str().expect("UTF-8 yol") }),
    )
    .expect("kayit calismali");
    invoke_with(
        &webview,
        "project_set_current",
        serde_json::json!({ "projectId": "gizli" }),
    )
    .expect("secim calismali");

    let response = invoke(&webview, "project_context").expect("komut calismali");
    assert!(
        response.contains("\"status\":\"known\""),
        "yanit: {response}"
    );
    assert!(
        !response.contains("SIZMAMALI-DEGER") && !response.contains("OPENAI_API_KEY"),
        "`.env` icerigi IPC ciktisina sizdi: {response}"
    );
    assert!(!response.contains("\".env\""), "yanit: {response}");

    let _ = std::fs::remove_dir_all(&directory);
}

/// Hafiza kapali/arizaliyken komut sessizce "proje yok" demez: tipli hata doner.
/// "Kayitli proje yok" ile "bakamadim" farkli cevaplardir (PROJECT.md Bolum 30).
#[test]
fn project_context_surfaces_typed_errors_instead_of_claiming_no_project() {
    for (state, expected) in [
        (DbState::Disabled, "disabled"),
        (
            DbState::Unavailable {
                reason: "sema migration'lari uygulanamadi".to_owned(),
            },
            "unavailable",
        ),
    ] {
        let app = build_test_app_with(state);
        let webview = main_webview(&app);

        let error = invoke(&webview, "project_context").expect_err("hata donmeli");
        assert!(!is_acl_denial(&error), "hata: {error}");
        assert!(error.contains(expected), "hata: {error}");
    }
}

// ---------------------------------------------------------------------------
// ASU-051 / ASU-052 — dosya okuma ve editorde acma, gercek ACL uzerinde
// ---------------------------------------------------------------------------

/// Testlerde kullanilan izole gecici dizin.
struct ToolTempDir(std::path::PathBuf);

impl ToolTempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "asuna-acl-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("gecici dizin");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ToolTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Gercek ACL uzerinden bir proje kaydeder ve guncel proje yapar.
fn register_project_over_ipc(
    webview: &tauri::WebviewWindow<MockRuntime>,
    root: &std::path::Path,
) -> String {
    let response = invoke_with(
        webview,
        "project_add",
        serde_json::json!({ "path": root.to_string_lossy(), "name": "Deneme" }),
    )
    .expect("project_add calismali");

    let value: serde_json::Value = serde_json::from_str(&response).expect("JSON");
    let project_id = value["project"]["id"]
        .as_str()
        .expect("proje kimligi")
        .to_owned();

    invoke_with(
        webview,
        "project_set_current",
        serde_json::json!({ "projectId": project_id }),
    )
    .expect("project_set_current calismali");

    project_id
}

/// **ASU-051 kabul kaniti**: tool gercek ACL uzerinden kok icindeki bir dosyayi
/// okuyor ve donen yol **gorece** kaliyor.
#[test]
fn a_project_file_can_be_read_over_the_real_acl() {
    let temp = ToolTempDir::new("read");
    std::fs::write(temp.path().join("README.md"), "# Asuna\nSesli companion.\n")
        .expect("README yazilmali");

    let app = build_test_app_with_memory();
    let webview = main_webview(&app);
    register_project_over_ipc(&webview, temp.path());

    let response = invoke_with(
        &webview,
        "read_project_file",
        serde_json::json!({ "path": "README.md" }),
    )
    .expect("read_project_file calismali");

    let value: serde_json::Value = serde_json::from_str(&response).expect("JSON");
    assert_eq!(value["path"], "README.md");
    assert_eq!(value["content"], "# Asuna\nSesli companion.\n");
    assert_eq!(value["truncated"], false);
    assert_eq!(value["redacted"], false);

    let canonical = std::fs::canonicalize(temp.path())
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned();
    assert!(
        !response.contains(&canonical),
        "mutlak yol renderer'a sizdi: {response}"
    );
}

/// **ASU-055 kabul kriteri**: `~/.ssh/id_ed25519` ve `.env` istekleri gercek
/// ACL uzerinden de reddediliyor ve icerik sizmiyor.
#[test]
fn secrets_cannot_be_read_over_the_real_acl() {
    let temp = ToolTempDir::new("secrets");
    std::fs::write(
        temp.path().join(".env"),
        "OPENAI_API_KEY=sk-proj-BU-DEGER-SIZMAMALI\n",
    )
    .expect(".env yazilmali");

    let app = build_test_app_with_memory();
    let webview = main_webview(&app);
    register_project_over_ipc(&webview, temp.path());

    for (path, code, escape) in [
        (".env", "blocklisted", false),
        ("../../.ssh/id_ed25519", "traversal", true),
        ("~/.ssh/id_ed25519", "tilde", true),
        ("/etc/passwd", "absolute", true),
    ] {
        let error = invoke_with(
            &webview,
            "read_project_file",
            serde_json::json!({ "path": path }),
        )
        .expect_err("reddedilmeli");

        // Red **tool** tarafindan geliyor, ACL'den degil: komut acik ama
        // sandbox kapatiyor. Ayrim onemli — ACL reddi olsaydi test sandbox
        // hakkinda hicbir sey kanitlamazdi.
        assert!(!is_acl_denial(&error), "beklenmedik ACL reddi: {error}");
        assert!(error.contains(code), "yol `{path}` icin hata: {error}");
        assert!(
            error.contains(&format!("\"escapeAttempt\":{escape}")),
            "yol `{path}` icin kacis etiketi yanlis: {error}"
        );
        assert!(
            !error.contains("BU-DEGER-SIZMAMALI"),
            "icerik sizdi: {error}"
        );
    }
}

/// **Uydurma yok**: var olmayan dosya `not_found` ile doner ve bu bir kacis
/// denemesi olarak etiketlenmez — model "dosya yok" ile "erisim reddedildi"yi
/// ayirt edebilsin.
#[test]
fn a_missing_file_is_distinguishable_from_a_refusal_over_ipc() {
    let temp = ToolTempDir::new("notfound");
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);
    register_project_over_ipc(&webview, temp.path());

    let error = invoke_with(
        &webview,
        "read_project_file",
        serde_json::json!({ "path": "YOK.md" }),
    )
    .expect_err("hata donmeli");

    assert!(!is_acl_denial(&error), "hata: {error}");
    assert!(error.contains("not_found"), "hata: {error}");
    assert!(error.contains("\"escapeAttempt\":false"), "hata: {error}");
}

/// Renderer **projeyi secemez**: komutun imzasinda `path` disinda bir alan
/// yoktur, dolayisiyla gonderilen fazladan alanlarin hicbir etkisi olmaz.
///
/// Olculen sey "fazladan alan reddediliyor mu" degil — Tauri komut argumanlari
/// bir `deny_unknown_fields` sozlesmesi degildir ve olmasi da gerekmiyor.
/// Olculen sey daha guclu olan: renderer ne yazarsa yazsin okuma **guncel
/// projeden** yapiliyor ve baska bir kok'e gecilemiyor.
#[test]
fn the_renderer_cannot_choose_which_project_to_read_from() {
    let temp = ToolTempDir::new("noproject");
    std::fs::write(temp.path().join("README.md"), "guncel projenin icerigi").expect("README");

    // Ikinci, kayitli ama **guncel olmayan** bir proje.
    let other = ToolTempDir::new("other");
    std::fs::write(other.path().join("README.md"), "GIZLI-DIGER-PROJE").expect("README");

    let app = build_test_app_with_memory();
    let webview = main_webview(&app);
    let other_id = register_project_over_ipc(&webview, other.path());
    // Son `project_set_current` kazanir: guncel proje artik `temp`.
    register_project_over_ipc(&webview, temp.path());

    for args in [
        serde_json::json!({ "path": "README.md", "projectId": other_id.clone() }),
        serde_json::json!({ "path": "README.md", "project_id": other_id.clone() }),
        serde_json::json!({ "path": "README.md", "root": other.path().to_string_lossy() }),
    ] {
        let response =
            invoke_with(&webview, "read_project_file", args.clone()).expect("okuma calismali");
        assert!(
            response.contains("guncel projenin icerigi"),
            "args {args}: guncel proje disindan okundu: {response}"
        );
        assert!(
            !response.contains("GIZLI-DIGER-PROJE"),
            "args {args}: renderer projeyi degistirebildi: {response}"
        );
    }
}

/// Hafiza kapaliyken kayitli kok listesi yok, dolayisiyla hicbir dosya
/// okunamaz — ve cevap "dosya yok" degil, tipli bir durum.
#[test]
fn reading_a_project_file_is_refused_when_memory_is_disabled() {
    let app = build_test_app_with(DbState::Disabled);
    let webview = main_webview(&app);

    let error = invoke_with(
        &webview,
        "read_project_file",
        serde_json::json!({ "path": "README.md" }),
    )
    .expect_err("hata donmeli");

    assert!(!is_acl_denial(&error), "hata: {error}");
    assert!(error.contains("disabled"), "hata: {error}");
}

/// **ASU-052 kabul kaniti**: editor komutu bulunamadiginda cikti "actim" degil,
/// hangi komutun aranip bulunamadigini soyleyen tipli bir hata.
///
/// Test config'i bilerek var olmayan bir komut tasiyor: ACL testleri gercek bir
/// editor **acmaz**.
#[test]
fn opening_a_project_reports_a_missing_editor_honestly_over_ipc() {
    let temp = ToolTempDir::new("open");
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);
    register_project_over_ipc(&webview, temp.path());

    let error = invoke(&webview, "open_project").expect_err("hata donmeli");

    assert!(!is_acl_denial(&error), "hata: {error}");
    assert!(error.contains("editor_not_found"), "hata: {error}");
    assert!(
        error.contains("asuna-test-editor-yok"),
        "mesaj hangi komutun bulunamadigini soylemiyor: {error}"
    );
}

/// Renderer **ne yolu ne komutu** secebilir: `open_project` argument almaz.
#[test]
fn the_renderer_cannot_choose_what_to_open_or_which_program_to_run() {
    let temp = ToolTempDir::new("openargs");
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);
    register_project_over_ipc(&webview, temp.path());

    for args in [
        serde_json::json!({ "path": "/etc" }),
        serde_json::json!({ "editor": "/bin/sh" }),
        serde_json::json!({ "command": "rm -rf /" }),
        serde_json::json!({ "projectId": "baska-proje" }),
    ] {
        let error = invoke_with(&webview, "open_project", args.clone()).expect_err("hata donmeli");
        assert!(!is_acl_denial(&error), "args {args}: {error}");
        // Fazladan alanlar yok sayilir ve komut yine **kendi** hedefini acmaya
        // calisir; onemli olan renderer'in verdigi degerin hicbir etkisi
        // olmamasi. Editor zaten yok, yani hata her zaman ayni yerden gelir.
        assert!(
            error.contains("editor_not_found"),
            "args {args}: renderer girdisi davranisi degistirdi: {error}"
        );
    }
}

/// Proje secilmemisken "actim" denmez.
#[test]
fn opening_without_a_current_project_is_refused_over_ipc() {
    let app = build_test_app_with_memory();
    let webview = main_webview(&app);

    let error = invoke(&webview, "open_project").expect_err("hata donmeli");

    assert!(!is_acl_denial(&error), "hata: {error}");
    assert!(error.contains("no_current_project"), "hata: {error}");
}

/// Iki komut da ACL kapsami disindaki bir pencereden **cagirilamaz**.
#[test]
fn the_tool_commands_are_denied_outside_the_permitted_window() {
    let app = build_test_app_with_memory();
    let foreign = WebviewWindowBuilder::new(&app, FOREIGN_WINDOW, Default::default())
        .build()
        .expect("ikinci pencere kurulmali");

    for command in ["read_project_file", "open_project"] {
        let error = invoke_with(
            &foreign,
            command,
            serde_json::json!({ "path": "README.md" }),
        )
        .expect_err("kapsam disi pencere reddedilmeli");
        assert!(
            is_acl_denial(&error),
            "`{command}` kapsam disi pencereden calisti: {error}"
        );
    }
}

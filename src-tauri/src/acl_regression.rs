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

use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::MockRuntime;
use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewWindowBuilder};

use crate::commands::EXPOSED_COMMANDS;
use crate::config::{self, EnvMap};
use crate::db::DbState;

/// Capability dosyalarinda acilan tek pencere etiketi.
const ALLOWED_WINDOW: &str = "main";

/// ACL kapsaminda **olmayan** bir pencere — negatif kontrol.
const FOREIGN_WINDOW: &str = "acl-probe";

fn test_config() -> config::AsunaConfig {
    let pairs = [
        ("OPENAI_API_KEY", "sk-proj-TEST-DEGERI-AGA-CIKILMAZ"),
        ("ASUNA_REALTIME_MODEL", "gpt-realtime-2.1"),
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
    ];
    let map: EnvMap = pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    config::load_from_map(&map).expect("test config gecerli olmali")
}

/// Uretimdeki `run()` ile **ayni** komut kumesi ve **ayni** context; tek fark
/// runtime'in mock olmasi ve DB'nin bellege/kapaliya alinmasi.
fn build_test_app_with(db_state: DbState) -> App<MockRuntime> {
    let app = mock_builder()
        .manage(test_config())
        // `RealtimeTokenService` BILEREK yok — bkz. modul dokumantasyonu.
        .invoke_handler(tauri::generate_handler![
            crate::commands::get_frontend_config,
            crate::realtime_token::mint_realtime_token,
            crate::db::state::db_status,
            crate::db::memory_repository::memory_list,
            crate::db::memory_repository::memory_create,
            crate::db::memory_repository::memory_update,
            crate::db::memory_repository::memory_archive,
            crate::db::memory_repository::memory_delete
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

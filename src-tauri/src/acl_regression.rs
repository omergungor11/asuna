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
        .manage(test_config())
        .manage(Arc::new(privacy))
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

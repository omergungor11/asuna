# ADR-005: SQLite Erisim Mimarisi

**Durum**: accepted
**Tarih**: 2026-08-24
**Iliskili**: ASU-005, OQ-1, OQ-2 · PROJECT.md Bolum 12, 19, 20 · `asuna-config/tech-stack.md` Bolum 5
**Onceki karari degistirir**: `asuna-docs/DECISIONS.md` icindeki ADR-005 (proposed, 2026-08-24) — bu ADR onu kapatir

---

## Baglam

Asuna'nin kalici depolamasi SQLite (ADR-005 proposed, PROJECT.md Bolum 12.1) — bu kisim
zaten kesindi. Acik olan tek soru **hangi katmandan erisilecegiydi** (OQ-1):

- **A)** `tauri-plugin-sql` — renderer JS'ten SQL cagrilari.
- **B)** Rust tarafinda persistence servisi + tip guvenli `#[tauri::command]`'lar.
- **C)** Node sidecar + `better-sqlite3` (ucuncu bir process).

Bu DB'de duracak veri: `memories`, `sessions` (transcript), `projects`, `tasks`,
`tool_events` (PROJECT.md 12.2). Yani Asuna'nin **en mahrem** verisi ve **audit izi**.

Pazarlik disi kisitlar:
- CLAUDE.md: "React componentleri dogrudan shell komutu calistirmaz, **dogrudan DB sorgusu atmaz**."
- PROJECT.md Bolum 19: tool audit'i (`tool_events`) guvenilir tarafta yazilmali; audit'i
  yazan taraf, audit'i silebilecek taraftan ayri olmali.
- PROJECT.md Bolum 20: memory "incelenebilir ve silinebilir" olmali — ama bu **UI uzerinden**,
  keyfi SQL uzerinden degil.
- Ileride sifreli DB (SQLCipher) + OS keychain (PROJECT.md Bolum 20 "Later").

C secenegi degerlendirme disi birakildi: ucuncu bir process, ayri lifecycle, ayri imzalama/
notarization yuku ve wake-word (ADR-004) zaten Rust tarafinda oldugu icin ikinci bir
guvenilir-taraf tanimi. Karar A ile B arasinda verildi.

---

## Karar

**Secenek B.** SQLite'a **yalnizca Tauri'nin Rust process'inden** erisilecek.

- `src-tauri/src/db/` altinda bir persistence servisi: `rusqlite` (bundled SQLite) uzerine
  kurulu, repository tarzi tipli metodlar.
- Webview'e acilan yuzey **dar amacli `#[tauri::command]`'lar**dir. Bir komut hicbir zaman
  SQL string'i parametre olarak almaz.
- Her komut `src-tauri/permissions/*.toml` icinde bir izinle eslesir ve capability
  dosyasinda **tek tek** acilir; okuma ve yazma ayri izinlerdir.
- Renderer tarafinda `src/asuna/memory/memory-service.ts` bu komutlari sarmalar;
  React component'leri servisi cagirir, `invoke`'u degil.
- Ortak sozlesme tipleri `src/shared/memory.ts` icinde Rust tiplerinin aynasidir.

`tauri-plugin-sql` **kullanilmayacak** ve `@tauri-apps/plugin-sql` bagimliligi eklenmeyecek.

---

## Degerlendirilen Secenekler

| # | Secenek | Surum / tarih | Sonuc |
|---|---------|---------------|-------|
| A | `tauri-plugin-sql` (sqlx) | crate 2.4.0 · npm `@tauri-apps/plugin-sql` 2.4.0 (2026-04-04), Apache-2.0 OR MIT | Reddedildi: ACL veri degil komut duzeyinde; path sandbox yok; transaction yok |
| B | Rust servis + `rusqlite` | `rusqlite` 0.40.2 (2026-08-08, MIT) + `rusqlite_migration` 2.6.0 (2026-05-28, Apache-2.0) | **SECILDI** |
| C | Node sidecar + `better-sqlite3` | — | Reddedildi: ucuncu process, dagitim/lifecycle yuku |
| D | Rust servis + `sqlx` (plugin'siz) | `sqlx` 0.9.0 (2026-05-21) | Reddedildi: MVP'de async DB'ye ihtiyac yok, SQLCipher yok, derleme suresi |

### Kriter karsilastirmasi

| Kriter | A — `tauri-plugin-sql` | B — Rust servis (`rusqlite`) |
|---|---|---|
| Renderer'a SQL sizmasi | **Evet** — ham SQL string'i webview'de yazilir | Hayir — SQL `src-tauri` disina cikmaz |
| ACL granulerligi | 4 komut, **scope yok**; `allow-execute` = tum DDL+DML | Komut basina izin; okuma/yazma ayrilabilir |
| Path sandbox | **Yok** — connection string renderer'dan, mutlak path app dizinini eziyor | Yol Rust'ta `app_data_dir()`'den turetilir, renderer parametre veremez |
| Transaction | **Yok** — `transaction` komutu yok, invoke'lar arasi atomiklik imkansiz | `rusqlite::Transaction`; `memories` + `tool_events` atomik |
| Tip guvenligi | `IndexMap<String, JsonValue>` — TS tarafinda elle cast | Rust struct → serde → TS interface; gecersiz enum IPC'de reddedilir |
| Girdi dogrulama | Yok (SQL neyse o) | Serde sema + domain validasyonu (`importance ∈ 0..1`) DB'ye dokunmadan |
| Migration | Rust'ta gomulu, `_sqlx_migrations`; **down sessizce atiliyor** | `rusqlite_migration`, `PRAGMA user_version`; down gercekten calisir |
| PRAGMA kontrolu | Yok (`Pool::connect(url)`, secenek yuzeyi sunulmuyor) | WAL / foreign_keys / synchronous / busy_timeout acilista |
| SQLCipher gecisi | Yok — sqlx'te destek yok, plugin'de key hook'u yok | `bundled-sqlcipher` feature'i + `PRAGMA key` |
| Test edilebilirlik | Plugin state + app handle gerekir | Servis saf birim testi; ayrica mock-runtime IPC testi |
| Phase 1 hizi | Daha hizli baslangic | Her metod icin Rust + TS iki taraf (kabul edilen maliyet) |
| Bagimlilik ayak izi | Cargo.lock 429 → **529**; uretim grafi 255 | Cargo.lock 429 → **444**; uretim grafi 219 |
| SQLite surumu | 3.46.0 (sqlx 0.8.6 → libsqlite3-sys 0.30.1'e pinli) | 3.53.2 (libsqlite3-sys 0.38.2, bundled) |
| Yeni npm paketi | `@tauri-apps/plugin-sql` gerekir | Gerekmez (`@tauri-apps/api` zaten var) |

---

## Spike Bulgulari

Her iki secenek de `tauri::test::mock_builder()` + **gercek** `generate_context!()` (yani
gercek capability dosyalari, gercek ACL) uzerinde, `tauri::test::get_ipc_response()` ile
renderer'in gonderdigi `InvokeRequest`'in aynisi gonderilerek olculdu.
Sonuc: **A 6/6 test, B 8/8 test yesil.** `cargo fmt --check`, `cargo clippy -- -D warnings`,
`pnpm typecheck`, `pnpm lint`, `pnpm test`, `pnpm build` hepsi temiz.

### A — olculen davranis

1. **Komut yuzeyi 4 komut, scope'suz.** `load`, `close`, `select`, `execute`. Izin
   aciklamalari harfiyen "Enables the X command **without any pre-configured scope**".
   `sql:default` = `allow-close` + `allow-load` + `allow-select`.
2. **Yazma = sinirsiz yazma.** `sql:default` ile:
   `sql.execute not allowed. Permissions associated with this command: sql:allow-execute`.
   `sql:allow-execute` verildikten sonra renderer'dan gonderilen
   `DROP TABLE memories` **basarili oldu**. "Memory ekleyebilsin ama tabloyu dusuremesin"
   diyebilecegimiz bir ara seviye yok.
3. **Path sandbox yok — kanitlandi.** `wrapper.rs::path_mapper`, `PathBuf::push` kullaniyor;
   mutlak path onceki yolu tamamen eziyor. Testte `sqlite:/var/folders/.../ESCAPED.db`
   yuklendi, `Ok` dondu ve dosya app data dizininin **disinda** olustu. Normalize/traversal
   reddi/allowlist yok.
4. **`load`'u sarmalamak yetmiyor.** Var olan bir SQLite dosyasina renderer'dan
   `ATTACH DATABASE '<path>' AS other` **basarili**; ardindan
   `SELECT name FROM other.sqlite_master` okunabildi.
5. **Transaction yok.** Tek `execute` icinde `BEGIN; INSERT; INSERT; COMMIT;` calisiyor
   (yani renderer cok-ifadeli ham SQL script gonderebiliyor). Ayri invoke'larla
   `BEGIN` → `ROLLBACK` ikisi de `Ok` donuyor ama hicbir sey geri alinmiyor — pool her
   cagrida baska bir connection verebiliyor. `memories` + `tool_events` atomik yazimi
   **imkansiz**.
6. **Migration calisiyor ama down calismiyor.** `_sqlx_migrations` tablosunda 2/2 kayit
   dogrulandi. Ancak `MigrationSource::resolve` icinde `matches!(kind, MigrationKind::Up)`
   filtresi var — `MigrationKind::Down` API'de var, motorda **sessizce atiliyor**.
7. **DB dizini plugin'in elinde.** Kod `app_config_dir()` kullaniyor; JS binding doc'u
   "relative to `BaseDirectory::App`" diyor — dokumantasyon kodla uyusmuyor.
   macOS'te ikisi ayni dizine cikiyor, ama kontrol bizde degil.
8. **Panic yuzeyi.** `wrapper.rs` icinde renderer girdisinin tetikledigi yolda
   `.expect("No App config path was found!")`, `create_dir_all(...).expect(...)`,
   `to_str().expect(...)` var — host process'te panic, `Result` degil.
9. **Surum kilidi.** Plugin `sqlx ^0.8` istiyor; lock `sqlx 0.8.6` → `libsqlite3-sys 0.30.1`
   → bundled **SQLite 3.46.0**. sqlx 0.9.0 mevcut ama plugin oraya gidemiyor.

### B — olculen davranis

1. **IPC round-trip.** `memory_create` → `{"id":1,"kind":"project_decision","importance":0.95,
   "projectId":"asuna",...}`; `memory_list_by_kind` ayni kaydi donuyor; `db_status` →
   `{"schemaVersion":3,"sqliteVersion":"3.53.2","memoryCount":1,"toolEventCount":0}`.
2. **Renderer'da SQL yok.** `execute` komutu denendi → `execute not allowed. Command not found`.
3. **Sema dogrulamasi IPC sinirinda.** `kind: "sql_injection"` →
   `invalid args 'memory' for command 'memory_create': unknown variant 'sql_injection',
   expected one of 'project_decision', 'preference', 'fact'` — DB'ye hic dokunulmadi.
   `importance: 9.0` → domain validasyonu reddetti.
4. **Komut basina ACL.** `src-tauri/permissions/memory.toml` eklendiginde uygulama komutlari
   da ACL'e tabi oluyor. Sadece `allow-memory-read` verildiginde:
   `memory_create not allowed. Permissions associated with this command: allow-memory-write`,
   okumalar `Ok`. **Okuma acik / yazma kapali ayrimi A'da mumkun degil.**
5. **Transaction.** `memories` + `tool_events` tek transaction'da; hata enjekte edildiginde
   iki sayac da 0 (rollback), basarida 1/1 (commit).
6. **Migration.** `PRAGMA user_version = 3`; uc ardisik acilista degismiyor (idempotent);
   restart sonrasi kayit korunuyor. `M::up(...).down(...)` ile down gercekten tanimli.
7. **Test edilebilirlik.** Servis katmani Tauri app'i / mock runtime / webview olmadan
   birim testinde calisiyor.
8. **SQLite bundled 3.53.2** (runtime'da `rusqlite::version()` ile dogrulandi; makinedeki
   sistem SQLite'i 3.51.0). Surum macOS'tan bagimsiz ve tekrarlanabilir.

### Her iki secenekte de ortaya cikan iki tuzak

1. **`capabilities/*.json` tek basina yetmiyor.** `tauri.conf.json` icindeki
   `app.security.capabilities` acik bir dizi oldugu icin, yeni capability'nin identifier'i
   oraya da eklenmezse komutlar sessizce reddediliyor.
2. **`Builder::setup` hook'u `build()` degil `App::run()` icinde calisiyor.** Testte
   `run()` cagrilmadigi icin `manage()` yapilmiyor ve komutlar `state not managed` diyor;
   test kurulumunda state build sonrasi elle manage edilmeli. (Plugin `setup`'i ise
   `build()`'de calisiyor — asimetri.)

### A ve B bir arada derlenemiyor

`libsqlite3-sys` `links = "sqlite3"` oldugu icin 0.30.1 (sqlx) ile 0.38.2 (rusqlite) cakisiyor;
cargo hard error veriyor. Yani "A ile basla, sonra B'yi yanina ekle" diye bir kacis yolu **yok**.

---

## DB Dosya Konumu

**Karar:** DB dosyasi macOS app data dizininde, Tauri path API'si ile cozulur.
Yol **asla** renderer'dan parametre olarak alinmaz.

```rust
// src-tauri/src/db/mod.rs
use tauri::Manager;

fn resolve_db_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> DbResult<PathBuf> {
    let dir = app.path().app_data_dir()?;   // dizin yoksa AsunaDb::open olusturur
    Ok(dir.join("asuna.db"))
}
```

Spike'ta olculen gercek degerler (identifier `com.omergungor.asuna`):

| API | Cozulen yol |
|---|---|
| `app_data_dir()` | `/Users/<user>/Library/Application Support/com.omergungor.asuna` |
| `app_config_dir()` | ayni (macOS'te `data` ve `config` ayni koke cikar) |
| `app_local_data_dir()` | ayni |
| `app_cache_dir()` | `/Users/<user>/Library/Caches/com.omergungor.asuna` |
| **Secilen DB yolu** | `.../Application Support/com.omergungor.asuna/asuna.db` |

Neden `app_data_dir()`: `app_config_dir()` macOS'te ayni yeri gosterse de semantik olarak
"kullanici tercihleri" icin; DB kullanici **verisi**. `app_cache_dir()` OS tarafindan
silinebilir — memory oraya konmaz.

Notlar:
- WAL modu `asuna.db-wal` ve `asuna.db-shm` kardes dosyalari uretir. Yedekleme/export
  ucunu birden kopyalamali ya da `VACUUM INTO` kullanmali (Phase 3 export task'i).
- `Application Support` Time Machine yedeklemesine dahildir — kullanici verisi icin dogru yer.
- **Dev override:** `ASUNA_DB_PATH` env degiskeni yalnizca `#[cfg(debug_assertions)]`
  build'lerde okunur. Release binary'de override yoktur.
- `AsunaDb::open` acilista sirasiyla: `create_dir_all(parent)` → `journal_mode=WAL` →
  `foreign_keys=ON` → `synchronous=NORMAL` → `busy_timeout=5s` → `migrations().to_latest()`.

---

## Migration Karari (OQ-2)

**Karar: `rusqlite_migration` 2.6.0** — migration'lar Rust'ta gomulu `M::up(...).down(...)`
olarak tanimlanir, `PRAGMA user_version` ile izlenir, uygulama acilisinda `to_latest()` ile
idempotent uygulanir.

Degerlendirilenler:

| Aday | Sonuc |
|---|---|
| **`rusqlite_migration` 2.6.0** (Apache-2.0) | **SECILDI.** SQLite'a ozel; ek tablo yok (`user_version`); `to_latest()` / `to_version()` (downgrade dahil); testte `validate()` ile semayi dogrulayabiliyoruz; ileride `from-directory` feature'i ile `.sql` dosyalarina gecis API'yi degistirmeden mumkun |
| `refinery` 0.9.2 (MIT) | Reddedildi. Cok-backend soyutlamasi (postgres/mysql/tokio) MVP'de gereksiz; kendi `refinery_schema_history` tablosunu ve checksum makinesini getiriyor; dosya adi konvansiyonuna bagimli |
| Elle versiyonlu SQL + `user_version` | Reddedildi. `rusqlite_migration`'in yaptiginin aynisini (siralama, transaction, versiyon takibi) elle yeniden yazmak demek; kazanc yok |
| Plugin'in kendi migration'lari (A) | Konu disi (A reddedildi). Ayrica down migration'lari sessizce atiyor |

Kurallar:
- Her migration **bir kez yazilir, bir daha degistirilmez**; degisiklik yeni bir `M` ekler.
- Her `M::up` icin `M::down` yazilir (gelistirme sirasinda geri alabilmek icin).
- `migrations().validate()` bir birim testinde kosar — bozuk SQL CI'da yakalanir.
- Sema degisikligi `src/shared/*.ts` aynasiyla **ayni commit'te** gider.

---

## Etkiler

- **`asuna-docs/DECISIONS.md` ADR-005 (proposed)** bu ADR ile kapanir; OQ-1 ve OQ-2 kapali.
- **`asuna-config/tech-stack.md` Bolum 5** guncellenir: erisim yolu = B; secenek tablosu
  "karar verildi" olarak isaretlenir; OQ-1/OQ-2 satirlari kapatilir.
- **`src-tauri/Cargo.toml`**: `rusqlite` (bundled), `rusqlite_migration`, `thiserror` eklenir.
  `tauri-plugin-sql` **eklenmez**.
- **`src-tauri/permissions/`** dizini olusturulur — bu dizin var oldugu andan itibaren
  uygulama komutlari da ACL'e tabi olur (spike'ta dogrulandi). Yeni her DB komutu once
  buraya bir izin, sonra capability dosyasina bir satir olarak eklenir.
- **`src-tauri/tauri.conf.json`**: her yeni capability identifier'i
  `app.security.capabilities` dizisine **de** eklenmeli.
- **Frontend**: yeni npm paketi yok. `src/asuna/memory/memory-service.ts` `@tauri-apps/api`
  `invoke`'unu sarmalar; component'ler servisi cagirir.
- **`src/db/`** (PROJECT.md Bolum 22'deki dizin) TypeScript sema/migration dizini **olmaz**;
  sema Rust tarafindadir. `src/shared/` yalnizca tip aynasi tutar.
- **Maliyet kabulu:** her repository metodu icin Rust fn + `#[tauri::command]` + izin +
  TS wrapper yazilacak. Bunu sinirlamak icin komutlar **kaba taneli** tutulur
  (`memory_create`, `memory_search`, `memory_delete`, `session_finalize`...), her SQL sorgusu
  icin bir komut acilmaz.
- **Phase 5 (tool audit)**: `tool_events` yazimi ayni transaction motorunu kullanir;
  renderer'in `tool_events`'e yazma ya da silme yolu **yoktur**.
- **Gelecek (sifreli DB)**: `rusqlite` feature'i `bundled` → `bundled-sqlcipher` yapilir,
  `PRAGMA key` acilista uygulanir, anahtar macOS Keychain'den okunur. Renderer etkilenmez,
  `memory-service.ts` degismez.

---

## Kaynaklar

Hepsi 2026-08-24 tarihinde erisildi/olculdu.

**Olcum (bu repo, spike worktree'si)**
- `cargo test --lib -- --nocapture` — Spike A 6/6, Spike B 8/8
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `pnpm typecheck`,
  `pnpm lint`, `pnpm test`, `pnpm build` — hepsi temiz

**tauri-plugin-sql (kaynak kod okundu)**
- https://crates.io/crates/tauri-plugin-sql — 2.4.0, 2026-04-04, Apache-2.0 OR MIT
- `permissions/default.toml`, `permissions/autogenerated/reference.md` — 4 komut, scope'suz izinler
- `src/commands.rs` — `load(db: String)`, `execute(db, query, values)` imzalari
- `src/wrapper.rs` — `DbPool::connect` (`app_config_dir()`), `path_mapper` (`PathBuf::push`)
- `src/lib.rs` — `MigrationList::resolve` icindeki `matches!(kind, MigrationKind::Up)` filtresi
- `guest-js/index.ts` — `invoke('plugin:sql|load' | '|execute' | '|select')`
- https://www.npmjs.com/package/@tauri-apps/plugin-sql — 2.4.0, dep `@tauri-apps/api ^2.10.1`

**sqlx / libsqlite3-sys**
- `sqlx 0.8.6` Cargo.toml: `sqlite = ["_sqlite", "sqlx-sqlite/bundled", ...]`
- `sqlx-sqlite 0.8.6` Cargo.toml: `libsqlite3-sys = "0.30.1"`
- `libsqlite3-sys-0.30.1/sqlite3/sqlite3.h`: `#define SQLITE_VERSION "3.46.0"`
- https://crates.io/crates/sqlx — 0.9.0 (2026-05-21) mevcut; plugin `^0.8`'e bagli

**rusqlite**
- https://crates.io/crates/rusqlite — 0.40.2, 2026-08-08, MIT
- Feature listesi: `bundled`, `bundled-sqlcipher`, `bundled-sqlcipher-vendored-openssl`, `sqlcipher`
- `libsqlite3-sys-0.38.2/sqlite3/sqlite3.h`: `#define SQLITE_VERSION "3.53.2"`
- https://crates.io/crates/rusqlite_migration — 2.6.0, 2026-05-28, Apache-2.0,
  feature'lar: `default`, `from-directory`
- https://crates.io/crates/refinery — 0.9.2, 2026-06-10, MIT

**Tauri**
- `tauri 2.11.5` `src/ipc/authority.rs::resolve_access` — origin + window/webview eslesmesi
- `tauri 2.11.5` `src/webview/mod.rs::is_local_url` — `local: true` capability'lerin
  uygulanmasi icin invoke URL'inin app origin'i (dev'de `devUrl`) olmasi gerekiyor
- `tauri 2.11.5` `src/test/mod.rs` — `mock_builder`, `get_ipc_response`, `INVOKE_KEY`

**Proje ici**
- PROJECT.md Bolum 12 (semalar), 19 (guvenlik modeli), 20 (gizlilik)
- CLAUDE.md — "React componentleri ... dogrudan DB sorgusu atmaz"
- `asuna-config/tech-stack.md` Bolum 5 — OQ-1 secenek tablosu ve secim kriterleri

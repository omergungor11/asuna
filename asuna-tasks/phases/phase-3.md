# Phase 3: Memory

> **Hedef:** Bir oturumda soylenen kalici bir bilgi, uygulama yeniden baslatildiktan sonra bir sonraki
> oturumda hatirlansin. Kullanici bu hafizayi gorebilsin ve silebilsin.
>
> **Milestone:** M3 — "Hatiriyor".
>
> **Onkosul:** ADR-005 karari verilmis olmali (R4) — **verildi (accepted)**.
> **Orchestrator karari (2026-08-24):** "Phase 2 ASU-028 gecmis olmali" on kosulu esnetildi —
> Phase 2, wake word model secimiyle (ADR-004, kullanici mikrofon testi) harici olarak bloklu;
> Phase 3'un tek gercek teknik bagimliligi ASU-032 ↔ ASU-026 (oturum kapanis akisi). ASU-032
> Phase 1 disconnect event'lerine baglanir, ASU-026 gelince birlesir. M3 kabul sirasi degismez.
>
> **Ilke (PROJECT.md Bolum 5.3 / Bolum 14):** Tum konusmayi sonsuza kadar saklamak hafiza degildir.
> Working context ile durable memory ayridir. Her working-context maddesi kalici hafizaya terfi etmez.
>
> **Durum (2026-08-25):** ASU-035 ile birlikte **Phase 3'un implementasyon halkasi kapandi**
> (ASU-029..ASU-037 DONE). Geriye yalnizca **ASU-038** kaldi: M3 manuel kabul testi — gercek
> mikrofon, gercek Realtime oturumu ve gercek uygulama yeniden baslatmasi gerektirir, otomatik
> testle karsilanamaz. Yazma yolu (oturum → ozet → cikarim) ve okuma yolu (Stage A retrieval →
> prompt enjeksiyonu) artik uctan uca bagli.
>
> **Guncelleme (2026-08-25):** M3 kabul testi calisirken **gercek bir acik** yakalandi —
> kullanici hafiza kayitlarini sildi ama Asuna hatirlamaya devam etti. Sebep: Stage A her
> oturum acilisinda **son oturum ozetini** enjekte ediyor ve `sessions.summary` urun icinden
> silinemiyordu. Backlog'daki **ASU-065** bu yuzden Phase 3'e cekildi ve tamamlandi; M3 kabul
> testi artik "silinen sey gercekten unutuluyor mu?" sorusunu tam olarak sorabilir.

---

## ASU-029: SQLite Bootstrap + Migration Altyapisi

**Scope**: db | **Boyut**: L | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-005

### Aciklama
ADR-005'te secilen erisim yaklasimini gercek kod haline getir. Bu task'ta henuz Asuna tablolari yok —
sadece DB acilir, migration calisir, saglik kontrolu yapilir.

### Acceptance Criteria
- [x] DB dosyasi macOS uygulama veri dizininde olusuyor (yol dokumante)
      — `app_data_dir()/asuna.db`, `db::resolve_db_path`; renderer parametre veremez.
      Dev override `ASUNA_DB_PATH` yalnizca `#[cfg(debug_assertions)]`
- [x] Migration altyapisi kurulu: versiyonlu, ileri yonlu, tekrar calistirilabilir
      — `rusqlite_migration` + `PRAGMA user_version`, `src-tauri/src/db/migrations/`
- [x] Uygulama acilisinda migration otomatik calisiyor; hata durumunda uygulama **cokmuyor**,
      hafizasiz modda devam ediyor ve durumu gosteriyor (PROJECT.md Bolum 30)
      — `DbState::{Ready,Disabled,Unavailable}` + `db_status` komutu
- [x] Renderer dogrudan SQL calistirmiyor — servis katmani zorunlu (CLAUDE.md kurali)
      — `src/asuna/memory/db-status-service.ts`; `acl_regression::the_renderer_has_no_sql_surface`
        gercek ACL uzerinde `execute` / `plugin:sql|execute` reddini olcuyor
- [x] Test icin in-memory / gecici DB destegi var
      — `AsunaDb::open_in_memory()` + `AsunaDb::open_at(path)`; testler `std::env::temp_dir()`
        altinda calisir, gercek uygulama veri dizinine yazmaz
- [x] DB dosyasi `.gitignore`'da — `*.db` zaten vardi; WAL kardes dosyalari (`*.db-wal`,
      `*.db-shm`) `*.db` desenine UYMADIGI icin ayrica eklendi
- [x] `docs/architecture/memory.md` erisim mimarisini anlatiyor — Bolum 3.1 (uygulanan hal)

### Notlar
- **ACL gecisi**: uygulama komutlari ASU-009'dan beri deny-by-default (`build.rs` icindeki acik
  `AppManifest`), yani ADR-005'in "`permissions/` dizini acilinca her sey ACL'e tabi olur" tuzagi
  zaten yasanmisti. Yeni `asuna-db` capability'si mevcut iki komutu kirmiyor — kanit:
  `acl_regression::existing_commands_still_pass_the_acl_after_the_db_capability_is_added`
  (gercek `generate_context!()`, gercek capability dosyalari, renderer'in gonderdigi
  `InvokeRequest`'in aynisi).
- **Gecici**: `migrations::apply` bos migration listesini "yapacak is yok" sayiyor
  (`rusqlite_migration::to_latest` bos listede `NoMigrationsDefined` hatasi verir).
  Bu dal ASU-030'da ilk migration eklendiginde kalkti.

---

## ASU-030: `memories` + `sessions` Schema

**Scope**: db | **Boyut**: M | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-029

### Aciklama
PROJECT.md Bolum 12.2'deki iki tablo. `projects`, `tasks`, `tool_events` sonraki phase'lerde.

### Acceptance Criteria
- [x] `memories`: id, kind, title, content, summary, project_id (nullable), importance, confidence,
      source_session_id (nullable), created_at, updated_at, last_accessed_at, expires_at (nullable),
      is_archived, embedding (nullable, simdilik kullanilmiyor), metadata_json
      — migration 1, `STRICT` tablo; kolon kumesi `db::model` testinde `PRAGMA table_info` ile dogrulaniyor
- [x] `sessions`: id, started_at, ended_at, project_id (nullable), summary, transcript_path (nullable),
      model, token/cost metadata, created_at
      — token/maliyet: `input_tokens`, `output_tokens`, `total_tokens`, `estimated_cost_usd` skalerleri
        + ham kirilim icin `usage_json` (memory.md T5 anahtarlari netlesince ASU-032 kolon acabilir)
- [x] `kind` degerleri PROJECT.md Bolum 5.3 listesiyle uyumlu ve tip olarak da tanimli
      (profile, preference, project, decision, task, working_context, relationship, idea, routine, tool_state)
      — semada CHECK kisiti, Rust'ta `MemoryKind`, TS'te `MEMORY_KINDS`; ucu de testlerle bagli
- [x] `project_id` alanlari simdilik nullable ve serbest; Phase 4'te foreign key ile baglanacak
      (migration plani not edilmis) — plan `001_memories_sessions.up.sql` icinde, `schema-mirror.spec.ts`
        ASU-039 notunun varligini ve FK'nin **olmadigini** dogruluyor
- [x] Sorgu icin gerekli index'ler: kind, project_id, importance, is_archived, created_at
      — ayrica `source_session_id`, Stage A bilesik index'i ve `expires_at` kismi index'i
- [x] TypeScript tipleri schema ile tek kaynaktan turetiliyor (elle senkronize edilen ikinci tanim yok)

### Notlar
- **"Tek kaynak" nasil yorumlandi.** Kod uretimi secilmedi: uretilmis bir `.ts` commit edilseydi,
  uretici calistirilmadan yapilan bir sema degisikligi yine sessizce kayardi. Yerine **tek kaynak
  dogrudan `.sql` dosyasidir** ve uc tuketici de ona testle baglanir:
  SQLite (DDL'in kendisi) · Rust (`PRAGMA table_info` + CHECK kisitindan okunan kind listesi) ·
  TypeScript (`src/shared/schema-mirror.spec.ts` ayni `.sql` dosyasini okur).
  Dogrulandi: semaya bir kolon ve bir `kind` degeri eklendiginde 3 Rust + 2 TS testi kirmiziya dondu.
- **Geri alinabilirlik.** Her migration icin `down` var ve gercekten kosuyor
  (`migrations_can_be_rolled_back_and_reapplied`). `down` uygulama acilisinda **asla** cagrilmaz.
- **Sema butunlugu testleri**: kind CHECK, importance/confidence araligi, UTC ISO-8601 zaman damgasi,
  `json_valid(metadata_json)`, `STRICT` tip zorlamasi, FK ihlali, `ON DELETE SET NULL` davranisi,
  `ended_at >= started_at`.

---

## ASU-031: `MemoryService` CRUD

**Scope**: backend | **Boyut**: M | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-030

### Acceptance Criteria
- [x] `create`, `getById`, `list(filter)`, `update`, `archive`, `delete` metodlari
      — `src-tauri/src/db/memory_repository.rs` (`create` / `get_by_id` / `list` / `update` /
        `set_archived` / `delete`), renderer yuzeyi `src/asuna/memory/memory-service.ts`
- [x] `list` filtreleri: kind, project_id, arsivli/degil, metin aramasi, limit/siralama
      — ayrica `id` (getById icin), `includeExpired`, `markAccessed`. `limit` varsayilan 50,
        tavan 200 (asan istek reddedilmez, **kirpilir**)
- [x] Erisimde `last_accessed_at` guncelleniyor — `markAccessed` ile, tek `UPDATE` icinde.
      Liste **goruntulemek** erisim sayilmaz (bkz. Notlar)
- [x] `expires_at` gecmis kayitlar retrieval'da donmuyor — varsayilan filtre; kayit
      silinmez, `includeExpired: true` ile gorunur (memory.md T7 politikasi ayri is)
- [x] `ASUNA_MEMORY_ENABLED=false` iken servis yazma yapmiyor, okuma bos donuyor, uygulama calisiyor
      — yazma `{"status":"skipped","reason":"memory-disabled"}` doner (sessiz "kaydettim" yok);
        `acl_regression::memory_writes_are_no_ops_and_reads_are_empty_when_memory_is_disabled`
- [x] DB hatasinda konusma devam ediyor, hata gorunur oluyor (sessiz yutma yok)
      — `DbState::Unavailable` iken okuma da yazma da `unavailable` kodlu tipli hata doner;
        tam hata zinciri yerel log'a yazilir, IPC'ye yalnizca kisa neden gider
- [x] Unit testler: CRUD, filtre, expiry, disabled modu — 26 Rust + 33 TS testi
      (repository, `store_error`, `clock`, ACL/IPC regresyonu, servis + sozlesme)

### Notlar
- **ACL: okuma ve yazma ayri izinler.** `capabilities/asuna-memory-read.json` (`memory_list`)
  ve `capabilities/asuna-memory-write.json` (`memory_create`/`update`/`archive`/`delete`)
  ayri dosyalar; `asuna-db.json` sadece `db_status` ile kaldi. Ayrim testle bagli
  (`commands::memory_reads_and_writes_are_separate_permissions`): okuma dosyasi hicbir yazma
  izni tasiyamaz. Amac somut — "salt okunur hafiza" moduna gecmek, yazma capability'sini
  `tauri.conf.json` listesinden cikarmak kadar basit olmali.
- **Erisim izi kararı.** `last_accessed_at` her `list` cagrisinda guncellenseydi Memory UI'de
  listeyi acmak tum kayitlarin erisim zamanini ezerdi — alan anlamsizlasir ve goruntuleme
  yazma uretirdi. Karar cagirana birakildi: `markAccessed`. Stage A (ASU-035) `true` verecek.
- **`null` = temizle, alan yok = dokunma.** `MemoryPatch` nullable alanlarda `Option<Option<T>>`
  kullaniyor; tek `Option` ile "ozeti sil" istegi sessizce "dokunma"ya donusurdu.
- **Zaman damgasi hassasiyeti.** `src-tauri/src/db/clock.rs` yalnizca `YYYY-MM-DDTHH:MM:SSZ`
  uretir (yeni bagimlilik yok). Sebep: siralama metin siralamasi ve
  `'...:00.500Z' < '...:00Z'` — karisik hassasiyet Stage A sirasini sessizce bozar. Ayni
  saniyedeki kayitlarin sirasi `ORDER BY ..., id DESC` ile cozulur.
- **Arama siniri**: SQLite `LIKE` buyuk/kucuk harf esitligi ASCII ile sinirli; Turkce'ye ozgu
  katlama (`I`/`ı`) yapilmaz. `%` ve `_` kullanicinin aradigi **harf** olarak kacisliyor.
  Dogru cozum FTS5/ICU — backlog.
- **Uc kat dogrulama korundu**: serde (`deny_unknown_fields`, bilinmeyen `kind`) → repository
  (aralik, bos metin, JSON, zaman damgasi) → sema CHECK'leri. Biri unutulursa digerleri tutar.

---

## ASU-032: Session Kaydi + Opsiyonel Transcript Persist

**Scope**: backend | **Boyut**: M | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-030, ASU-026

### Acceptance Criteria
- [x] Oturum acilinca `sessions` kaydi olusuyor (started_at, model)
      — `session_start`; **model renderer'dan gelmez**, `ASUNA_REALTIME_MODEL` config
        degerinden alinir. Tetik: `AsunaRealtimeService`'in `connected` event'i
- [x] Oturum kapaninca ended_at + kullanilan model + varsa token/sure/maliyet metadata yaziliyor
      — `session_finalize`; skalerler (`input/output/total_tokens`) + ham kirilim `usage_json`.
        `estimated_cost_usd` icin bkz. Notlar (dogrulanmis fiyat tablosu yok)
- [x] `ASUNA_TRANSCRIPT_STORAGE=true` iken transcript dosyaya yaziliyor ve `transcript_path` doluyor
      — `transcript::persist_if_enabled`, JSONL (satir basina bir replik), dosya `0600` / dizin `0700`
- [x] `ASUNA_TRANSCRIPT_STORAGE=false` iken transcript diske hic yazilmiyor (dogrulanmis)
      — davranissal test: `writes_absolutely_nothing_to_disk_when_storage_is_disabled` gecici
        dizinde **dosya sistemi** kontrolu yapar (dizin bile olusmuyor). IPC ucundan da
        dogrulandi: `session_commands_record_a_session_end_to_end_over_the_real_acl`
        (`transcriptPath: null`)
- [x] Transcript dosyalari uygulama veri dizininde, oturum id'siyle isimlendirilmis
      — `app_data_dir()/transcripts/session-<id>.jsonl`; yol **Rust tarafinda** cozulur,
        renderer yol veremez (`deny_unknown_fields` ile de reddedilir)
- [x] Cokme/anormal kapanista yarim kalan oturum kaydi bir sonraki acilista kapatiliyor
      — `session_repository::close_abandoned`, `lib.rs` acilis akisinda; `idx_sessions_open`
        kismi index'i kullanilir, islem idempotent
- [x] Oturum suresi ve tahmini maliyet UI'da gorunebiliyor (R1 takibi)
      — voice-panel'de tek satir: `3 dk 12 sn · 1.240 token · maliyet: bilinmiyor`.
        **Maliyet su an her zaman "bilinmiyor"** — bkz. Notlar

### Notlar
- **Maliyet neden bos.** Dogrulanmis bir fiyat tablosu yok; `gpt-realtime-2.1` icin
  token basina fiyati koda gomduğumuz anda "uydurulmus maliyet" gostermis oluruz
  (PROJECT.md "never invent" ilkesinin fatura karsiligi). Yeni bir zorunlu env anahtari
  eklemek de mevcut `.env`'leri kirar (config'te sessiz default yok). Bu yuzden:
  kolon ve tum boru hatti hazir, deger `NULL`, UI "bilinmiyor" yaziyor. **ASU-033**
  ozetleme maliyetini de hesaplayacagi icin fiyat tablosu orada tek seferde cozulmeli.
  → **ASU-033'te cozuldu**: `src-tauri/src/pricing.rs` (voice.md Bolum 6, dogrulanmis).
  Deger artik kirilim aciklanabildiginde hesaplaniyor, aksi halde hala `NULL`.
- **Yarim oturumun `ended_at` degeri `started_at`.** Gercek bitis bilinmiyor; "simdi"
  yazmak 20 saatlik sahte bir oturum ve sahte bir maliyet penceresi uretirdi. Sifir sure
  "bilmiyoruz"un en az yaniltici hali; neden `summary` alanina insan diliyle yaziliyor
  (`ABANDONED_SESSION_SUMMARY`). Ayri bir `end_reason` kolonu daha temiz olurdu ama yeni
  migration + uc katmanli tip aynasi bu task'in kapsamini asiyordu — ASU-033 kolon acarken
  birlikte degerlendirilecek.
  → **ASU-033'te cozuldu**: migration 002 `end_reason` kolonunu acti, bayrak `summary`'den
  cikarildi ve eski kayitlar tasindi. `ended_at = started_at` karari degismedi.
- **TEMPORARY (ASU-026).** Kapanis tetigi su an Phase 1'in `disconnected` event'i.
  Idle timeout / wake word ile kapanis geldiginde tetik oraya tasinacak; `SessionRecorder`
  sozlesmesi (begin/end) degismeyecek.
- **Yaris kosulu.** `session_start` asenkron, `disconnected` senkron gelir. `SessionRecorder`
  kapanista acilis cagrisini **bekler**; aksi halde kisa oturumlar `ended_at = NULL` kalir ve
  her acilista "yarim oturum" olarak kurtarilirdi. Test: `kapanis, ucusta olan acilis
  cagrisini bekler`.
- **Kayit hatasi konusmayi dusurmez.** `session_start`/`session_finalize` hatalari
  yakalanir, log'lanir; sesli oturum devam eder. Hafiza kapaliyken kayit hic olusmaz ve
  kapanista uydurulmus bir oturum kimligiyle yazma denenmez.
- **ACL**: `capabilities/asuna-session.json` — hafiza yazma izinlerinden **ayri**. Oturum
  kaydi ile durable memory farkli katmanlar (PROJECT.md Bolum 14); izinleri de ayri.

---

## ASU-033: Session Summary Pipeline

**Scope**: backend | **Boyut**: M | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-032

### Aciklama
Oturum kapandiginda transcript'ten kisa bir ozet uret. Bu ozet hem `sessions.summary`'ye yazilir hem
de ASU-034'un girdisidir.

### Acceptance Criteria
- [x] Ozet, realtime oturumundan **ayri** bir cagri ile uretiliyor (realtime modele "kaydet" dedirtilmiyor)
      — `src-tauri/src/summary.rs`, `POST /v1/chat/completions`. Realtime oturumu kapandiktan
        **sonra** calisir; konusan modele hicbir kayit/ozet yetkisi verilmiyor (PROJECT.md Bolum 26)
- [x] Ozet kisa ve yapili: ne konusuldu, ne karar verildi, ne yarim kaldi
      — `SESSION_SUMMARY_PROMPT_V1` uc basligi sart kosar (`Konusulanlar` / `Kararlar` /
        `Yarim kalanlar`), 120 kelime siniri + "uydurma" yasagi; saklanan metin 1.500 karakterle kirpilir
- [x] Ozet uretimi basarisiz olursa oturum kaydi yine de kapaniyor (ozet null kalir, hata loglanir)
      — kapanis once yazilir, ozet sonra (bkz. Notlar "Iki yazma"). Test:
        `a_model_failure_leaves_the_session_closed_without_a_summary` (401/429/500) +
        `a_network_failure_is_typed_and_does_not_touch_the_session`
- [x] Cok kisa oturumlarda (orn. 2 replikten az) ozet uretilmiyor — bos gurultu yaratmiyor
      — `MIN_TRANSCRIPT_LINES = 2`; ayrica yalnizca bosluktan olusan dokum de atlanir.
        Test ag'a **hic cikilmadigini** dogruluyor (`very_short_sessions_are_skipped_without_calling_the_model`)
- [x] Ozet icin kullanilan model config'ten geliyor, hard-code degil
      — `ASUNA_SUMMARY_MODEL` (zorunlu anahtar; `.env.example`'da `gpt-4o-mini`). Renderer'a
        **gitmez**: `FrontendConfig` whitelist'inde yok, testle bagli
- [x] Ozetleme maliyeti oturum metadata'sina ekleniyor
      — `usage_json.$.summary` = `{ model, promptVersion, promptTokens, completionTokens,
        totalTokens, estimatedCostUsd: null }`. USD **null**: ozet modelinin fiyati dogrulanmadi
        (bkz. Notlar). Realtime oturumunun kendi kirilimi `json_set` ile **ezilmiyor**
- [x] Unit test: sabit transcript girdisi -> pipeline'in cagrildigi ve sonucun yazildigi dogrulanmis (model mock'lu)
      — `a_fixed_transcript_produces_a_summary_that_is_written_to_the_session`; mock HTTP sunucusu
        `realtime_token.rs`'teki std `TcpListener` deseni, **gercek API'ye cagri yok**

### Notlar
- **`end_reason` kolonu (migration 002).** ASU-032'de yarim kalan oturum `summary` alanina yazilan
  bir cumle ile isaretleniyordu. `summary` artik gercek ozeti tasiyor ve ASU-034'un girdisi —
  bayrak orada kalsaydi ya ozeti ezerdi ya da bir sistem cumlesinden hafiza uretilirdi. Yeni kolon
  `completed | abandoned | error`; eski kayitlar migration icinde tasindi ve bayrak cumlesi
  `summary`'den **temizlendi**. Geri alma da bilgiyi kaybetmiyor (cumleyi geri yaziyor).
  Sema surumu 1 → 2; tip aynasi uc katmanda da guncellendi.
- **Renderer `abandoned` diyemez.** `session_finalize` girdisi yalnizca `completed | error` kabul
  eder (`ReportedEndReason`); "yarim kalmis" tespiti acilistaki kurtarmanin isi. Aksi halde kolon
  neyi olctugunu kaybederdi. IPC ucundan test edildi.
- **Iki yazma, tek dogru sira.** `session_finalize` once DB'ye yazip **doner**, ozet arka planda
  (`tauri::async_runtime::spawn`) uretilip ayri bir `UPDATE` ile eklenir. Gerekce: (1) kapanis bir
  ag cagrisina bagimli olamaz — kullanici 30 sn beklemez; (2) uygulama ozet donmeden kapanirsa
  kaybedilen tek sey ozettir, oturum kaydi kapali ve tutarli; (3) kuyruk/retry tablosu gerekmiyor,
  cunku "ozet bekliyor" diye yarim bir durum hic olusmuyor. Ozet ancak `ended_at IS NOT NULL` olan
  bir kayda yazilir.
- **Fiyat tablosu cozuldu — ama yalnizca dogrulanmis kismi.** `src-tauri/src/pricing.rs`, kaynak
  `docs/architecture/voice.md` Bolum 6 (2026-08-24). `estimated_cost_usd` artik **hesaplanabiliyor**,
  ama iki kosul birlikte saglanirsa: model tabloda var **ve** token kirilimi (ses/metin) toplami
  aciklayabiliyor. `cached_tokens` gelirse hesap yapilmaz — cache indirimi ses/metin ayrimi
  gerektiriyor, o ayrim kirilimda yok ve "ortalama almak" uydurmaktir. Hesaplanamadiginda gorulen
  anahtar **adlari** log'lanir; boylece voice.md'deki "BELIRSIZ" tahminle degil gozlemle kapanacak.
- **Ozet maliyeti neden USD degil token.** `gpt-4o-mini` fiyati dogrulanmis bir kaynaktan
  alinmadi. Olculen sey (token sayisi) kaydediliyor, cevrimi yapilmiyor: fiyat dogrulandiginda
  ayni `pricing` modulune eklenip geriye donuk hesaplanabilir.
- **Transcript ayarindan bagimsiz.** Ozet, `session_finalize`'a gelen **bellekteki** dokumu
  kullanir; `ASUNA_TRANSCRIPT_STORAGE` yalnizca diske yazmayi kontrol eder (`.env.example`:
  "false = sadece oturum ozeti saklanir"). Ancak transcription hic acilmazsa dokum bos gelir ve
  ozet uretilmez — bu, ayarin dogal sonucu.
- **Ag'a cikmayan testler.** ACL regresyon uygulamasinda `SummaryService` bilerek `manage`
  **edilmiyor** (`RealtimeTokenService` ile ayni gerekce): `session_finalize` tetigi servisi
  bulamayinca log'layip duruyor.

---

## ASU-034: Memory Extraction Pipeline

**Scope**: backend | **Boyut**: L | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-033, ASU-031

### Aciklama
PROJECT.md Bolum 26'daki boru hatti:
`konusma -> oturum ozeti -> aday hafizalar -> dogrulama/dedup -> kalici depolama`

### Acceptance Criteria
- [x] Aday hafiza yapisi uretiliyor: kind, content, importance (0-1), confidence (0-1), projectId (opsiyonel)
      — `src-tauri/src/extraction.rs`, `MemoryCandidate`. Model `title` de verebilir; vermezse
        `content`'in ilk cumlesinden turetilir (`derive_title`)
- [x] Sema dogrulamasi: gecersiz kind / aralik disi skor / bos content reddediliyor
      — `validate_candidate`; **aday basina** calisir, bir red digerlerini dusurmez. Eksik skor
        varsayilana cekilmez, reddedilir ("never invent"). Test:
        `invalid_candidates_are_rejected_without_dropping_the_valid_one`
- [x] Deduplication: mevcut benzer hafiza varsa yeni kayit acmak yerine guncelleniyor
      (Phase 3'te deterministik/metin tabanli yeterli, semantik dedup backlog'da)
      — `normalize_for_dedup` + `is_duplicate`: kucuk harf, noktalama -> bosluk, tam esitlik ya da
        >= 12 karakterlik alt dize. Ayni `kind` **ve** ayni `project_id` sarti. Eslesirse
        `importance` **max** alinir, `updated_at` tazelenir; icerik/metadata ezilmez
- [x] Onem esigi altindaki adaylar kaydedilmiyor (esik konfigurabilir)
      — `MIN_IMPORTANCE = 0.5` (dahil). Yeni bir **zorunlu** env anahtari acilmadi (bkz. Notlar);
        deger tek bir `const`'ta, config'e tasinmasi tek satirlik
- [x] Working context tipi bilgiler (guncel dosya, terminal hatasi) durable memory'ye terfi etmiyor
      (PROJECT.md Bolum 14)
      — `NON_DURABLE_KINDS` = `working_context`, `tool_state`. Iki kat: talimatta gecerli kind
        listesinde **yok**, gelse bile `CandidateRejection::NonDurableKind` ile duser
- [x] Hassas/kisisel kategorilerde kalici kayit oncesi acik onay isteniyor (PROJECT.md Bolum 26 sonu)
      — `SENSITIVE_KINDS` = `profile`, `relationship`: kayit `metadata_json.pendingApproval = true`
        ile yazilir. Sema degismedi. Retrieval sozlesmesi ASU-035'te (bkz. Notlar)
- [x] Uretilen hafiza `source_session_id` ile oturuma bagli — kaynagi izlenebilir
      — `MemoryDraft.source_session_id`; cikarim yalnizca ozeti **yazilmis** bir oturumdan calisir
- [x] Unit testler: dogrulama reddi, dedup, esik filtresi
      — 26 test (`extraction::tests`), mock HTTP sunucusu `summary.rs`'teki `TcpListener` deseni.
        **Gercek API'ye cagri yok**; `ASUNA_MEMORY_ENABLED=false` testinde ag'a hic cikilmadigi
        ayrica dogrulaniyor

### Notlar
Realtime modele dogrudan "veritabanina yaz" yetkisi verilmez. Cikarim ayri, denetlenebilir bir adimdir.
Bu, "never invent memories" ilkesinin muhendislik karsiligidir.

- **Kanca: ozetin ustunde, ozetin sahibi degil.** Cikarim `summary::spawn_for_session` icindeki
  ayni arka plan gorevinde, `store(...)` `SummaryOutcome::Stored` dondukten **sonra** calisir.
  Gerekce: (1) ozet cikarimin girdisi — yazilmamis bir ozetten uretilen hafizanin kaynagi
  izlenemez; (2) cikarim hatasi ozeti geri almaz, `sessions.summary` yerinde kalir
  (test: `an_extraction_failure_leaves_the_summary_and_session_intact`); (3) ikinci bir kuyruk/
  retry mekanizmasi gerekmiyor.
- **Yeni env anahtari yok.** Model olarak `ASUNA_SUMMARY_MODEL` yeniden kullaniliyor: cikarim da
  kisa, ucuz bir metin cagrisi ve her adim icin zorunlu bir anahtar acmak mevcut `.env`'leri
  kirardi. Ayni gerekceyle esik `const`. Ikisi de olculdukten sonra config'e tasinabilir.
- **Hassas kategoriler: atmak yerine bekletmek.** Aday sessizce silinseydi kullanici onun var
  oldugunu hic bilemezdi (PROJECT.md Bolum 20 "storage is inspectable"). Kayit yazilir ama
  `pendingApproval = true` ile isaretlenir: Memory ekraninda gorunur/silinebilir, retrieval'a
  **girmez**. Onay `memory_update` ile `metadata_json`'daki bayragin `false` yapilmasidir —
  sema degismedi, yeni komut acilmadi.
- **`pendingApproval` her kayitta acikca yazilir** (`false` da). "Anahtar yoksa ne demek?"
  belirsizligi retrieval tarafinda sessiz bir hataya donusurdu. Elle (Memory UI) olusturulan
  kayitlarda anahtar yoktur ve bu dogrudur: kullanicinin kendi yazdigi hafiza onay beklemez.
- **Dedup arsivi de tarar.** Kullanici bir hafizayi arsivlediyse ayni bilgiyi yeni bir satir
  olarak geri getirmek onun kararini gecersiz kilardi; guncelleme arsiv durumuna dokunmaz.
  Tarama ayni kind + proje altinda en yeni 200 kayitla sinirli — tam tarama her oturum
  kapanisinda tum tabloyu okumak demekti (Stage C konsolidasyonu zaten backlog'da).
- **Model cevabi savunmaci ayristirilir.** Govde `summary.rs` gibi minimum (`model` + `messages`);
  `response_format` gonderilmiyor cunku bu hesap/model icin davranisi dogrulanmadi. Buna karsilik
  ` ```json ` blogu ve `{"memories": [...]}` sarmalayicisi **tolere edilir**, baska her sey hata
  olur. Bos dizi hata degil: "hatirlanacak bir sey yok" gecerli bir cevaptir.
- **Maliyet `usage_json.$.extraction`.** Yeni `session_repository::attach_usage` yalnizca tek bir
  alt agaci `json_set` ile yamalar; `$.summary` ve realtime kirilimi **ezilmez**. USD yine `null`
  (ASU-033 ile ayni gerekce: `gpt-4o-mini` fiyati dogrulanmadi). Yamaya sayimlar da yazilir
  (`created`/`updated`/`rejected`/`failed`) — cikarimin ne yaptigi oturum kaydindan denetlenebilir.
- **ASU-037 ile kesisim.** `ASUNA_MEMORY_ENABLED` artik calisma zamaninda da kapatilabiliyor;
  cikarim ikisini birden okur (`config.memory_enabled && privacy::process_memory_enabled()`).
  Kapaliyken model **hic cagrilmaz** — ne istek, ne maliyet, ne DB dokunusu.

---

## ASU-035: Stage A Deterministik Retrieval + `SessionBootstrapContext`

**Scope**: backend | **Boyut**: L | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-034

### Aciklama
PROJECT.md Bolum 13 Stage A + Bolum 25. Embedding **yok** — Phase 3 deterministik kalir.

### Acceptance Criteria
- [x] `SessionBootstrapContext` uretiliyor: userPreferences, currentProject (Phase 4'te dolacak),
      recentSession, activeTasks (Phase 6'da dolacak), relevantMemories
      — `src-tauri/src/db/retrieval.rs`; `currentProject`/`activeTasks` **tipli ama bos**
        (bkz. Notlar). Komut: `get_bootstrap_context` (okuma capability'si, ACL 3 adim)
- [x] Stage A kurallari: proje biliniyorsa proje ozeti + son proje kararlari + son oturum ozeti
      — `build_bootstrap_context(db, project_id, now)`: proje verildiginde once o projenin
        `project` + `decision` kayitlari, sonra global kume (dedup'li).
        **Cagrilan taraf Phase 4'e kadar `None` veriyor**; kod yolu yazildi ve test edildi
        (`project_decisions_come_first_when_the_project_is_known`)
- [x] Proje bilinmiyorsa: global tercihler + son oturum ozeti ile siniri korunmus baglam
      — tercihler ayri bolum (`kind = preference`, en fazla 8), ilgili hafizalar onem sirali
        (en fazla 12); `working_context` / `tool_state` **hicbir zaman** girmez
- [x] Baglam paketi boyutu sinirli ve olculuyor (PROJECT.md Bolum 25: "Do not attach huge raw histories")
      — `CONTEXT_WORD_LIMIT = 2000` kelime + kalem tavanlari (hafiza 120, oturum ozeti 250).
        Olcum `ContextBudget` ile geri doner (`wordCount`, `included`, `dropped`, `truncated`)
        ve hem Rust hem TS tarafinda log'lanir
- [x] Baglam `buildAsunaInstructions(context)` uzerinden prompt'a enjekte ediliyor (ASU-012 ile birlesir)
      — `src/asuna/memory/bootstrap-context.ts` → `additionalSections`; enjeksiyon noktasi
        `AsunaRealtimeService.prepareInstructions` (her `connect()` oncesi **taze**)
- [x] Hicbir hafiza yoksa Asuna "hatirliyorum" gibi davranmiyor — baglam bos gecerse prompt bunu belirtiyor
      — `EMPTY_MEMORY_NOTICE`: "Kalıcı hafıza boş — geçmiş konuşma hatırlamıyorsun,
        hatırlıyormuş gibi davranma." Kapali hafiza ve okunamayan hafiza icin **ayri** iki cumle
- [x] Unit testler: siralama/oncelik kurallari, boyut siniri, bos durum
      — Rust: `db::retrieval::tests` (siralama, proje onceligi, pending/arsiv/expired dislama,
        silme sonrasi, butce tasmasi, bos durum) + `acl_regression` uzerinden 4 IPC testi.
        TS: `bootstrap-context.spec.ts` (12 test) + `realtime-service.spec.ts` (3 test)

### Notlar
- **ASU-034 sozlesmesi**: onay bekleyen hafizalar retrieval'a **girmez**. Stage A filtresi
  `json_extract(metadata_json, '$.pendingApproval') IS NOT 1` (anahtar adi:
  `extraction::PENDING_APPROVAL_KEY`). Elle olusturulan kayitlarda anahtar yoktur ve bu kayitlar
  onay beklemez — kosul `IS NOT 1` bilerek boyle yazildi, `= 0` degil.
  → Uygulandi: `MemoryFilter::exclude_pending_approval` (varsayilan `false`). **Memory UI'in
  davranisi degismedi**: onay bekleyenler listede gorunmeye devam ediyor; yalnizca retrieval
  `true` veriyor.
- **`MemoryRecord` degil `ContextMemory` donuluyor.** Baglam bir DB satiri degil, kirpilmis bir
  prompt parcasi. Satiri dondurup icerigini kisaltmak "satir buydu" yalanini uretirdi; burada
  kirpma `truncated` ile gorunur ve `metadata_json` / `confidence` gibi alanlar modele hic gitmez.
- **Boyut tavani neden 2000 kelime.** Enjekte edilen metin **her turda** yeniden faturalanir
  (voice.md Bolum 6); ~2000 kelime kabaca 3 bin token'lik sabit bir yuk. Kayit sayisi tavanlari
  (8 tercih + 12 hafiza) tek basina yetmiyor: kalem tavanlariyla birlikte en kotu durum ~2700
  kelime, yani kelime butcesi gercekten baglayici. Tasma **ilk tasmada durur** — "belki daha
  kucugu sigar" diye aramak, sirasi onem olan bir listeyi sessizce yeniden siralardi. Once
  dusen: en dusuk onemli ilgili hafizalar; tercihler ve son oturum ozeti korunur.
- **`currentProject` / `activeTasks` neden simdiden sozlesmede.** Bos donuyorlar ama tipleri var:
  Phase 4/6'da alan eklemek renderer sozlesmesini **ve** prompt bicimini birlikte degistirirdi.
  Bos donmek "bilmiyorum"un durust hali; uydurulmus bir proje ozeti degil.
- **Talimat oturum basina uretilir, servis omru boyunca degil.** `AsunaRealtimeService`
  kurucusundaki sabit `instructions` ikinci oturumda **eski** hafizayi enjekte ederdi (ozet ve
  cikarim kapanista calisiyor). Yeni `prepareInstructions` kancasi her `connect()` oncesi bir kez
  cagrilir — retry'larda tekrar cagrilmaz (denemeler arasi prompt degismesin).
- **Baglam okunamazsa konusma bloklanmaz.** `unavailable` / ACL / IPC hatasi durumunda oturum
  yine acilir; prompt'a "hafiza su an okunamiyor, hatirliyormus gibi davranma" satiri girer ve
  neden log'a duser. Sessiz yutma yok, kapali kapi da yok (PROJECT.md Bolum 30).
- **Calisma zamaninda kapatilan hafiza baglam uretmez.** `get_bootstrap_context` `PrivacyState`e
  bakar; kapaliyken `memoryAvailable: false` ile bos doner. Kayitlar **silinmez** ve Memory UI'da
  gorunmeye devam eder — incelemek ile konusmaya tasimak ayri seyler (ASU-037 ile tutarli:
  anahtar "daha az hatirla" yonunu kapatmaz).
- **`markAccessed: true`.** Baglama girmek erisimdir; liste goruntulemek degildir (ASU-036 notu).
  Aday kumesi `limit` ile dar oldugu icin "okundu ama butceye sigmadi" nadir bir kenar durum ve
  yaslandirma icin yine de dogru sinyal.
- **Son oturum ozeti yalnizca `end_reason = 'completed'` kayitlardan.** `abandoned` oturumlarin
  `summary` alani zaten bos; `error` ile bitenin ozeti eksik bir konusmayi anlatir. Eksik/yanlis
  bir "gecen sefer sunu konustuk" cumlesi, hic ozet olmamasindan kotudur.
- **GIZLILIK**: log satirlari **sayi** tasir (kac kayit, kac kelime). Hafiza basligi ya da icerigi
  ne Rust log'una ne renderer log'una yazilir.

---

## ASU-036: Memory UI (Listele / Ara / Sil / Arsivle)

**Scope**: frontend | **Boyut**: M | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-031

### Aciklama
PROJECT.md Bolum 20: "Memory storage is inspectable. User can delete memories." Bu MVP kabul
checklist maddesi — opsiyonel degil.

### Acceptance Criteria
- [x] Memory sekmesi: hafiza listesi, kind rozeti, olusturma tarihi, kaynak oturum
      — `src/components/memory-view.tsx` + `memory-item.tsx`; sekme gecisi `src/app/app.tsx`
- [x] Metin aramasi + kind'a gore filtre
      — `memory-filters.tsx`; arama 250 ms debounce ile `MemoryFilter.search`, tur secimi
        `MemoryFilter.kinds` olarak servise gider (UI kendi kendine filtrelemez)
- [x] Tek hafizayi silme (onay ile) ve arsivleme
      — onay **satir ici**; `window.confirm` yok (WKWebView'de tum pencereyi kilitler,
        canli ses oturumu arkada calisiyor olabilir)
- [x] Bir hafizanin hangi oturumdan geldigi gorulebiliyor
      — `source_session_id` her satirda: "Oturum #7" ya da acikca "Kaynak oturum bilinmiyor"
- [x] Silinen hafiza sonraki oturumun baglamina girmiyor (dogrulanmis)
      — **ASU-035 ile kapandi**. Bu task'ta dogrulanan: silme sonrasi liste depodan yeniden
        okunuyor, silinen kayit ekranda kalmiyor (`onaydan sonra siler ve liste tutarli kalir`).
        ASU-035'te eklenen kanit: `db::retrieval::tests::
        a_deleted_memory_never_reaches_the_next_session_context` (repository ucu) ve
        `acl_regression::the_bootstrap_context_reflects_the_store_over_the_real_acl`
        (gercek ACL uzerinden: yaz → baglamda gor → sil → baglamda **yok**).
        Onbellek yok; baglam her oturum acilisinda depodan yeniden okunuyor
- [x] Liste buyudugunde UI donmuyor (sayfalama veya sanal liste)
      — "son N + daha fazla yukle": `limit` tavani 25'er buyur, sanal liste kutuphanesi yok

### Notlar
- **Sekme gecisi ses oturumunu koparmaz.** `VoicePanel` her zaman monte kalir, yalnizca `hidden`
  ile gizlenir; `MemoryView` ise yalnizca acikken monte olur (kapali sekme IPC sorgusu atmaz).
  Testle bagli: `app.spec.tsx` → "hafiza sekmesine gecince ses paneli monte kalir".
- **Liste goruntulemek erisim degildir**: `markAccessed` gonderilmiyor, aksi halde UI'da gezinmek
  Stage A siralamasini (ASU-035) bozardi. Test filtrenin tam sekline bakiyor.
- **`disabled` ile `unavailable` ayri ekran**: kapali hafiza ariza gibi gosterilmiyor, bozuk hafiza
  ise nedeniyle birlikte `role="alert"` ile cikiyor (PROJECT.md Bolum 30).
- **`skipped` yazma sonucu basari sayilmiyor**: hafiza kapaliyken "sildim/arsivledim" denmiyor.
- **Sayfalama neden `limit`?** Servis katmani offset sunmuyor (`MemoryFilter`); "daha fazla yukle"
  tavani buyutup listeyi tazeliyor. Offset gerekirse backend task'i acilmali.
- Metin duz render ediliyor; `dangerouslySetInnerHTML` yok (test: `<b>` etiketi metin olarak basiliyor).

---

## ASU-037: Memory Gizlilik Kontrolleri

**Scope**: frontend | **Boyut**: S | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-036

### Acceptance Criteria
- [x] Settings'te anahtarlar: durable memory acik/kapali, transcript saklama acik/kapali
      — `src/components/settings-view.tsx`; sekme `src/app/app.tsx` ("Ayarlar"). Anahtarlar
        `role="switch"`; durum `get_privacy_settings`'ten okunur, onbelleklenmez
- [x] Anahtarlar `ASUNA_MEMORY_ENABLED` / `ASUNA_TRANSCRIPT_STORAGE` ile ayni davranisi paylasiyor
      — `src-tauri/src/privacy.rs`: `PrivacyState::from_boot(...)` acilista env'den baslar.
        **Yazan her yol** artik config'in ham alanina degil bu duruma bakiyor:
        `memory_create` / `memory_update`, `session_start` / `session_finalize`
        (Gate 3 / CRITICAL-1 ile eklendi), transcript persist ve ozet+cikarim boru hatti
        (`summary::spawn_for_session` → `extraction::extract_after_summary`).
        Transkript anahtari renderer tarafinda da **canli** okunuyor: Realtime oturumu
        acilirken `audio.input.transcription` boot config'ine gore degil, o anki
        `get_privacy_settings` sonucuna gore kuruluyor (`realtime-service.ts`,
        Gate 3 / MEDIUM-3)
- [x] Kapatildiginda geriye donuk veriye ne oldugu kullaniciya net anlatiliyor (silinmiyor, sadece yazilmiyor)
      — her anahtarin altinda durum-bagimli cumle ("Daha once kaydedilenler SILINMEZ...").
        Testler metni birebir olcuyor (`settings-view.spec.tsx`)
- [x] "Tum hafizayi sil" aksiyonu var ve cift onay istiyor
      — birinci kapi UI (niyet -> `TUM HAFIZAYI SIL` yazma), ikinci kapi komut imzasi
        (`memory_delete_all(confirmationPhrase)`). Ifade tutmazsa DB'ye **hic dokunulmaz**,
        `invalid` kodlu tipli hata doner
- [x] Degisiklikler yeniden baslatmadan etkili
      — `set_privacy_settings` calisma zamani durumunu degistirir; ACL uzerinden ucdan uca
        test: `turning_durable_memory_off_at_runtime_stops_writes_without_a_restart`

### Notlar
- **Calisma zamani yalnizca SIKILASTIRIR.** Acilista kapali olan bir anahtar buradan
  **acilamaz** ve istek `locked-by-env` ile reddedilir. Bu bir kolaylik kisiti degil,
  acilisin gercegi: `ASUNA_MEMORY_ENABLED=false` iken SQLite dosyasi **hic acilmaz**
  (`DbState::Disabled`), acilmamis bir DB'ye calisma zamaninda yazilamaz. UI kilitli anahtari
  tiklatip hata gostermek yerine nedenini onceden yazar (`*AtBoot` alanlari bunun icin var).
- **`.env` degismez.** Ayar yalnizca calisan process icin gecerli; dosyayi kullanici yonetir,
  uygulama sessizce duzenlemez. Bu davranis Ayarlar ekraninda **yazili**.
- **Anahtar "daha az hatirla" yonunu kapatmaz.** Kalici hafiza kapaliyken `memory_create` /
  `memory_update` `skipped` doner ama `memory_archive`, `memory_delete` ve `memory_delete_all`
  **calismaya devam eder**. Aksi halde anahtar, kullanicinin kendi verisini temizlemesini
  engelleyen bir tuzaga donusurdu (PROJECT.md Bolum 20).
- **Toplu silmenin kapsami dar ve acikca yazili**: yalnizca `memories`. Oturum
  kayitlari/ozetleri ve diskteki transcript dosyalari silinmez — "hepsini sildim" deyip bir
  seyi birakmak en kotu sonuc. Bu kapsam Gate 3'te (MEDIUM-6) yeniden degerlendirildi ve
  **bilincli olarak dar birakildi**; oturum ozetleri + dokum dosyalarinin temizligi ayri bir
  task: **ASU-065**. Silme sonrasi `VACUUM` denenir (serbest sayfalar dosyada
  kalmasin); `VACUUM` basarisiz olursa islem yine basarili sayilir, hata log'lanir.
  → **ASU-065 tamamlandi (2026-08-25)**: temizlik artik urun icinde, **ayri ve gorunur** bir
  aksiyon olarak (`Ayarlar > Konusma gecmisini sil` + `Hafiza > Oturumlar`). Kapsam ayrimi
  degismedi; her iki ekran da digerinin kapsam disi oldugunu yaziyor.
- **Onay bekleyenler UI'i** (`src/components/pending-approvals.tsx`): ASU-034'un
  `metadata_json.pendingApproval` bayragini gorunur kilar. Onayla = bayragi `false` yapan
  `memory_update` (diger metadata korunur), Reddet = `memory_delete`. Filtre **UI tarafinda**:
  `MemoryFilter` bir `pendingApproval` boyutu sunmuyor, son 200 kayit taraniyor. Daha buyuk
  depolar icin sunucu tarafi filtre gerekir — backend task'i.
- **Komut olmayan yazma yolu icin process durumu.** Transcript persist bir komut degil,
  `State` goremez; ayni `Arc<PrivacyState>` acilista `install_process_state` ile process
  genelinde de kaydedilir. Testlerde kurulmaz (OnceLock geri alinamaz, testler birbirini
  etkilerdi) — kapali anahtarin davranisi `persist_with_runtime_switch` uzerinden dogrudan
  olculuyor: dizin bile olusmuyor.
- **Yeni ACL yuzeyi**: `memory_delete_all` hafiza **yazma** capability'sinde (yazma izni
  kaldirilinca toplu silme de kapanir), `get/set_privacy_settings` ise kendi
  `asuna-privacy` capability'sinde. Uc adim disiplini (build.rs manifest + capability +
  tauri.conf) ve `EXPOSED_COMMANDS` senkron testleri guncellendi.

---

## ASU-038: M3 Kabul Testi — Restart Sonrasi Hatirlama

**Scope**: test | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-029..ASU-037

### Aciklama
PROJECT.md Bolum 31'deki "Memory" manuel kabul testi.

### Acceptance Criteria
- [ ] Oturum acilir, Asuna'ya bir proje karari soylenir (orn. "wake word'u yerel tutuyoruz")
- [ ] Oturum kapatilir; `sessions.summary` ve en az bir `memories` kaydi olusmus
- [ ] Uygulama tamamen kapatilip yeniden acilir
- [ ] Yeni oturumda "ne karar vermistik?" sorusuna dogru cevap veriliyor
- [ ] Ilgili hafiza Memory UI'da gorunuyor ve silinebiliyor
- [ ] Silindikten sonra ayni soru sorulunca Asuna **uydurmuyor**, bilmedigini soyluyor
- [ ] `ASUNA_MEMORY_ENABLED=false` ile ayni akis calisiyor (hafizasiz ama coken degil)
- [ ] Manuel senaryo `asuna-config/testing.md`'ye eklenmis

### Hazirlik (ASU-035 devri)
Otomatik testler zinciri **kod ucunda** kapatti; ASU-038'in isi bunu gercek ses/restart ile
dogrulamak. Testi kosarken bakilacak yerler:

- **Baglam gercekten enjekte edildi mi?** Dev konsolunda `memory-context` scope'lu satir:
  `Oturum baglami hazir.` + `wordCount` / `preferences` / `relevantMemories` sayilari. Rust
  tarafinda ayni olcum: `[asuna] Stage A baglami: N/2000 kelime, ...`. Iki satir da yoksa
  baglam cekilmemistir.
- **Hafiza bos oldugunda** prompt'a `Kalıcı hafıza boş — ...` satiri girer; Asuna'nin
  "hatirliyorum" demesi bu asamada **bug**'dir, ASU-035 kriteri.
- **Silme sonrasi** yeni oturum acmak yeterli: baglam onbelleklenmiyor, her `connect()`
  yeniden okuyor. Ayni uygulama calisirken bile ikinci oturum guncel hafizayi gorur.
- **"Sildim ama hatirladi" ise iki yer birden temizlenmeli** (ASU-065): hafiza kaydi
  `Hafiza` listesinden, o konusmanin **ozeti** ise ayni sekmedeki `Oturumlar` bolumunden
  (ya da toptan: `Ayarlar > Konusma gecmisini sil`). Stage A ikisini de enjekte eder;
  yalnizca birini silmek "hala hatiriyor" gorunumu uretir ve bu **beklenen** davranistir.
- **Ilk oturumda hafiza olusmasi** ASU-033/034'e bagli: ozet yazilmadan cikarim calismaz ve
  ozet icin konusmanin yeterince uzun olmasi gerekir (`summary` skip kurallari). Kisa bir
  "merhaba" oturumundan hafiza cikmamasi beklenen davranistir.
- **`ASUNA_MEMORY_ENABLED=false`** yolunda `get_bootstrap_context` `memoryAvailable: false`
  doner; oturum acilir, prompt "hafiza kapali" satirini tasir, hicbir DB dosyasi olusmaz.
- Hassas turlerden (`profile`, `relationship`) cikan hafizalar `pendingApproval` ile gelir ve
  **onaylanana kadar** baglama girmez — "kaydettim ama hatirlamiyor" gorunumu bu ise beklenen
  davranistir; Ayarlar > onay bekleyenler ekranindan onaylanmali.

---

## ASU-065: Oturum Ozeti + Transcript Temizligi

**Scope**: full-stack | **Boyut**: M | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-032, ASU-035, ASU-037

### Neden one cekildi (M3 blokaji)

Bu task Gate 3'te (MEDIUM-6) backlog'a alinmisti. **M3 kabul testi onu blokere cevirdi**:
kullanici Memory UI'dan hafiza kayitlarini sildi, yeni oturum acti ve Asuna ayni seyi
hatirlamaya devam etti. Sebep bir hata degil, **eksik bir yuzeydi**: Stage A her oturum
acilisinda son oturum ozetini de enjekte ediyor (`sessions.summary`, ASU-035) ve o ozet urun
icinden silinemiyordu — `memory_delete_all` bile kapsam disi birakiyordu.

"Hafizami sildim" diyen bir kullanicinin hatirlanmaya devam etmesi, PROJECT.md Bolum 20'nin
("storage is inspectable, user can delete memories") kagit uzerinde kalmasi demektir. Bu
yuzden ASU-038 kabul kriterlerinin ("silindikten sonra ayni soru sorulunca uydurmuyor")
gecmesi ASU-065'e bagli hale geldi.

### Acceptance Criteria

- [x] `session_list` — oturum gecmisi okunabiliyor (kimlik, acilis/kapanis, `end_reason`,
      ozet on izlemesi, diskte dokum dosyasi var mi)
      — `src-tauri/src/db/session_repository.rs::list_recent`; varsayilan limit 50, tavan 200.
        **Tavan gorunur**: yanit `limit` / `limitMax` / `total` tasir, UI "hepsi bu kadar" diye
        tahmin yurutmez (ASU-036'daki `SERVER_LIST_CAP` tahmininin tersi)
- [x] `session_delete` — tek oturum: `sessions` satiri + varsa transcript dosyasi diskten
      — donen sonuc dosyaya ne oldugunu **ayri** bir alanda soyler
        (`transcriptFile: not-recorded | deleted | already-gone | refused | failed`).
        Satir ve dosya ayri ayri basarisiz olabilir; "sildim" demek ikisi de bilindiginde dogru
- [x] `session_clear_all` — tum oturum kayitlari + `transcripts/` dizini,
      `confirmationPhrase: "KONUSMA GECMISINI SIL"` birebir (`memory_delete_all` deseni)
      — ifade `memory_delete_all`'inkinden **farkli**: birini yazip digerini calistirmak
        mumkun olmamali. Ifade tutmazsa ne DB'ye ne diske dokunulur
- [x] Silme yolunda path uretimi yalnizca DB'deki `transcript_path`'ten ve `app_data_dir()`
      altinda oldugu dogrulanarak — traversal guard
      — `transcript::delete_recorded_file`: yol lexical normalize edilir (`canonicalize`
        degil, dosya zaten silinmis olabilir), `transcripts/` altinda olmasi **ve** dosya
        adinin o oturuma ait olmasi aranir; symlink takip edilmez. Reddedilen yol
        `Refused` doner ve UI bunu **yazar**
- [x] Stage A etkisi: `retrieval.rs` **degismedi**; test eklendi
      — `db::retrieval::tests::a_deleted_session_summary_never_reaches_the_next_session_context`
        (silinince bir onceki ozete duser, hepsi silinince `None`) +
        `acl_regression::a_deleted_session_summary_leaves_the_next_bootstrap_context`
- [x] ACL 3 adim + `EXPOSED_COMMANDS` + senkron/regresyon testleri
      — yeni `capabilities/asuna-session-read.json` (yalnizca `session_list`); silme komutlari
        mevcut `asuna-session.json`'a eklendi (bkz. Notlar). `build.rs` manifest,
        `tauri.conf.json` capability listesi, `lib.rs` handler ve `EXPOSED_COMMANDS` (14 → 17)
        guncellendi; `commands::session_reads_and_writes_are_separate_permissions` ayrimi
        dosya duzeyinde olcuyor
- [x] UI: Hafiza sekmesinde "Oturumlar" bolumu (ozet on izleme, tarih, satir ici onayli sil)
      — `src/components/session-list.tsx` (+ `session-text.ts`), `memory-view.tsx` icine
        gomulu. `window.confirm` yok (ASU-036 ile ayni gerekce)
- [x] UI: Ayarlar'da "Konusma gecmisini sil" (cift onay + ifade), `memory_delete_all`'in
      yaninda; iki aksiyonun kapsam farki **metinle** net
      — `settings-view.tsx`; iki aksiyon ayni `DangerAction` kabugunu paylasir ama farkli
        baslik/ifade/komut kullanir. "Ozetler silinmez" metni guncellendi: artik silinebilir
        ve **nereden** yapilacagi yazili
- [x] Testler — Rust: dosya gercekten diskten gidiyor (gecici dizin), `clear_all` sonrasi
      tablo ve dizin bos, ifade reddi, ACL (yabanci pencere), silinen ozetin baglama girmemesi.
      TS: oturum listesi render, silme akisi, clear-all cift onay
      — 12 yeni Rust testi + 30 yeni TS testi; toplam 336 Rust / 576 TS yesil

### Notlar

- **ACL ayrimi: okuma ayri, silme kayitla birlikte.** `session_list` yeni bir
  `asuna-session-read` capability'sindedir; `session_delete` / `session_clear_all` ise
  **mevcut** `asuna-session.json`'a (kayit yuzeyi) eklendi. Gerekce `memory_delete_all` ile
  ayni (ASU-037): ayri bir "temizlik" capability'si acmak, kayit yetkisi kaldirilmis bir
  kurulumda toplu silmeyi acik birakirdi. Somut karsiligi: `asuna-session`'i
  `tauri.conf.json`'dan cikarmak kaydi **ve** silmeyi kapatir, gecmisi gorunur birakir.
  Hafiza okuma capability'sine konmadi cunku oturum kaydi ile durable memory farkli
  katmanlar (PROJECT.md Bolum 14) — ASU-032'deki ayrimin devami.
- **Dosya yolu renderer'a hic gitmiyor.** Liste satiri `transcriptPath` degil
  `hasTranscriptFile: boolean` tasir; sozlesme fazladan alani **reddeder**
  (`parseSessionListItem` testi). Kullanicinin dizin yapisi webview'e tasinacak bir bilgi
  degil ve silme zaten host tarafinda yapiliyor.
- **Once satir, sonra dosya.** Kullanicinin sikayetini ureten sey `sessions.summary` idi;
  bir `EACCES` hatasinin ozetin silinmesini engellemesi kabul edilemez. Dosya silinemezse
  satir yine gider ve sonuc `failed` olarak **gorunur** — sessiz basari yok.
- **Oturum silmek hafizayi silmez.** `memories.source_session_id` FK'si `ON DELETE SET NULL`
  (migration 001): kayit durur, kaynagi "bilinmiyor"a doner ve Memory UI bunu zaten yaziyor
  (ASU-036). Hafizayi silmek kullanicinin **ayri** bir karari; iki aksiyonun metni de bunu
  soyluyor. Silme sonrasi hafiza listesi tazelenir (kaynak satiri guncel kalsin).
- **Hafiza acilista kapaliyken de dokum temizlenir.** `ASUNA_MEMORY_ENABLED=false` iken DB
  hic acilmaz, dolayisiyla `deletedSessions = 0`; ama `transcripts/` dizini onceki bir
  calismadan kalmis olabilir ve temizlenir. Aksi halde anahtar, kullanicinin kendi dosyalarini
  silmesini engelleyen bir tuzaga donusurdu (ASU-037'deki ayni ilke).
- **Asuna'nin yazmadigi dosya silinmez.** Toplu temizlik yalnizca `session-<id>.jsonl`
  desenine uyan dosyalari siler; dizindeki baska bir sey `remainingFiles` olarak sayilir,
  birakilir ve kullaniciya **soylenir**. Dizin ancak tamamen bosaldiginda kaldirilir.
- **ACL testinde `session_clear_all`'in basarili yolu bilerek kosulmuyor.** Mock uygulama
  gercek `app_data_dir()`'i cozer (identifier `tauri.conf.json`'dan gelir); komutun mutlu
  yolunu orada calistirmak **kullanicinin gercek dokumlerini** silerdi. ACL testi kapinin
  acildigini ve yanlis ifadenin reddedildigini olcer; diskteki davranis `db::transcript`
  testlerinde gecici dizinle, DB tarafi `db::session_repository` testlerinde olculur.
- **Ikinci bulgu — kod degisikligi YOK.** M3 testinde ikinci bir gozlem daha vardi: bir
  hafiza silindikten sonra kullanici ayni konuyu yeniden konusursa Asuna onu **yeniden
  ogreniyor**. Bu bir regresyon degil, dogru davranis: silinen hafizanin "hortlamasi" degil,
  **yeni konusmanin kaydi**. Kayit yolu her oturumda ayni (ozet → cikarim → kalici hafiza) ve
  kullanicinin sildigi sey gecmis konusmadan turetilen kayitti, gelecekteki konusmalar degil.
  Bunu engellemek "bu konuyu bir daha ogrenme" gibi bir kara liste gerektirirdi — MVP kapsami
  disi ve muhtemelen istenmeyen bir davranis (kullanici fikrini degistirebilir).


---

## ASU-066: Cmd+Q Finalize Yarisi

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-033, ASU-034

### Aciklama

Uygulama Cmd+Q ile kapatildiginda `session_finalize` (oturum ozeti uretimi + hafiza cikarimi)
tamamlanmadan process olebilir — kapanis isi asenkron ve bir model cagrisi iceriyor.
Sonuc: o oturumdan hicbir kalici hafiza cikmaz ve kullanici bunu **fark etmez**.

Session 1 kapanis notunda **baslik olarak** kaydedildi. Kapsam, gercekten yarisan yollar ve
cozum (kapanisi geciktirme / kapanis oncesi guard / yarim kalan oturumu bir sonraki aciliste
tamamlama) **kod incelemesi gerektiriyor** — detay henuz yok, burada uydurulmayacak.

### Acceptance Criteria

- [ ] Kod incelemesiyle yarisin gercek yeri ve kosullari yazili olarak tespit edilmis
- [ ] Cikista finalize tamamlanana kadar kapanis bekletiliyor **veya** yarim kalan oturum
      bir sonraki aciliste tamamlaniyor (secilen yaklasim gerekcesiyle birlikte)
- [ ] Kullanici kapanisin bittigini gorebiliyor (sessiz bekleme yok)
- [ ] Testte: finalize bitmeden gelen kapanis sinyali kayit kaybina yol acmiyor

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

**Scope**: backend | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-034

### Aciklama
PROJECT.md Bolum 13 Stage A + Bolum 25. Embedding **yok** — Phase 3 deterministik kalir.

### Acceptance Criteria
- [ ] `SessionBootstrapContext` uretiliyor: userPreferences, currentProject (Phase 4'te dolacak),
      recentSession, activeTasks (Phase 6'da dolacak), relevantMemories
- [ ] Stage A kurallari: proje biliniyorsa proje ozeti + son proje kararlari + son oturum ozeti
- [ ] Proje bilinmiyorsa: global tercihler + son oturum ozeti ile siniri korunmus baglam
- [ ] Baglam paketi boyutu sinirli ve olculuyor (PROJECT.md Bolum 25: "Do not attach huge raw histories")
- [ ] Baglam `buildAsunaInstructions(context)` uzerinden prompt'a enjekte ediliyor (ASU-012 ile birlesir)
- [ ] Hicbir hafiza yoksa Asuna "hatirliyorum" gibi davranmiyor — baglam bos gecerse prompt bunu belirtiyor
- [ ] Unit testler: siralama/oncelik kurallari, boyut siniri, bos durum

### Notlar
- **ASU-034 sozlesmesi**: onay bekleyen hafizalar retrieval'a **girmez**. Stage A filtresi
  `json_extract(metadata_json, '$.pendingApproval') IS NOT 1` (anahtar adi:
  `extraction::PENDING_APPROVAL_KEY`). Elle olusturulan kayitlarda anahtar yoktur ve bu kayitlar
  onay beklemez — kosul `IS NOT 1` bilerek boyle yazildi, `= 0` degil.

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
- [~] Silinen hafiza sonraki oturumun baglamina girmiyor (dogrulanmis)
      — **ASU-035 ile birlikte** kapanir: retrieval/bootstrap katmani orada yaziliyor.
        Bu task'ta dogrulanan: silme sonrasi liste depodan **yeniden okunuyor**, silinen
        kayit ekranda kalmiyor (`onaydan sonra siler ve liste tutarli kalir` testi)
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

**Scope**: frontend | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: ASU-036

### Acceptance Criteria
- [ ] Settings'te anahtarlar: durable memory acik/kapali, transcript saklama acik/kapali
- [ ] Anahtarlar `ASUNA_MEMORY_ENABLED` / `ASUNA_TRANSCRIPT_STORAGE` ile ayni davranisi paylasiyor
- [ ] Kapatildiginda geriye donuk veriye ne oldugu kullaniciya net anlatiliyor (silinmiyor, sadece yazilmiyor)
- [ ] "Tum hafizayi sil" aksiyonu var ve cift onay istiyor
- [ ] Degisiklikler yeniden baslatmadan etkili

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

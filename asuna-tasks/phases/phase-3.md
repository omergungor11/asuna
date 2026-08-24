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

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-030, ASU-026

### Acceptance Criteria
- [ ] Oturum acilinca `sessions` kaydi olusuyor (started_at, model)
- [ ] Oturum kapaninca ended_at + kullanilan model + varsa token/sure/maliyet metadata yaziliyor
- [ ] `ASUNA_TRANSCRIPT_STORAGE=true` iken transcript dosyaya yaziliyor ve `transcript_path` doluyor
- [ ] `ASUNA_TRANSCRIPT_STORAGE=false` iken transcript diske hic yazilmiyor (dogrulanmis)
- [ ] Transcript dosyalari uygulama veri dizininde, oturum id'siyle isimlendirilmis
- [ ] Cokme/anormal kapanista yarim kalan oturum kaydi bir sonraki acilista kapatiliyor
      (ended_at null kalmiyor)
- [ ] Oturum suresi ve tahmini maliyet UI'da gorunebiliyor (R1 takibi)

---

## ASU-033: Session Summary Pipeline

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-032

### Aciklama
Oturum kapandiginda transcript'ten kisa bir ozet uret. Bu ozet hem `sessions.summary`'ye yazilir hem
de ASU-034'un girdisidir.

### Acceptance Criteria
- [ ] Ozet, realtime oturumundan **ayri** bir cagri ile uretiliyor (realtime modele "kaydet" dedirtilmiyor)
- [ ] Ozet kisa ve yapili: ne konusuldu, ne karar verildi, ne yarim kaldi
- [ ] Ozet uretimi basarisiz olursa oturum kaydi yine de kapaniyor (ozet null kalir, hata loglanir)
- [ ] Cok kisa oturumlarda (orn. 2 replikten az) ozet uretilmiyor — bos gurultu yaratmiyor
- [ ] Ozet icin kullanilan model config'ten geliyor, hard-code degil
- [ ] Ozetleme maliyeti oturum metadata'sina ekleniyor
- [ ] Unit test: sabit transcript girdisi -> pipeline'in cagrildigi ve sonucun yazildigi dogrulanmis (model mock'lu)

---

## ASU-034: Memory Extraction Pipeline

**Scope**: backend | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-033, ASU-031

### Aciklama
PROJECT.md Bolum 26'daki boru hatti:
`konusma -> oturum ozeti -> aday hafizalar -> dogrulama/dedup -> kalici depolama`

### Acceptance Criteria
- [ ] Aday hafiza yapisi uretiliyor: kind, content, importance (0-1), confidence (0-1), projectId (opsiyonel)
- [ ] Sema dogrulamasi: gecersiz kind / aralik disi skor / bos content reddediliyor
- [ ] Deduplication: mevcut benzer hafiza varsa yeni kayit acmak yerine guncelleniyor
      (Phase 3'te deterministik/metin tabanli yeterli, semantik dedup backlog'da)
- [ ] Onem esigi altindaki adaylar kaydedilmiyor (esik konfigurabilir)
- [ ] Working context tipi bilgiler (guncel dosya, terminal hatasi) durable memory'ye terfi etmiyor
      (PROJECT.md Bolum 14)
- [ ] Hassas/kisisel kategorilerde kalici kayit oncesi acik onay isteniyor (PROJECT.md Bolum 26 sonu)
- [ ] Uretilen hafiza `source_session_id` ile oturuma bagli — kaynagi izlenebilir
- [ ] Unit testler: dogrulama reddi, dedup, esik filtresi

### Notlar
Realtime modele dogrudan "veritabanina yaz" yetkisi verilmez. Cikarim ayri, denetlenebilir bir adimdir.
Bu, "never invent memories" ilkesinin muhendislik karsiligidir.

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

---

## ASU-036: Memory UI (Listele / Ara / Sil / Arsivle)

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-031

### Aciklama
PROJECT.md Bolum 20: "Memory storage is inspectable. User can delete memories." Bu MVP kabul
checklist maddesi — opsiyonel degil.

### Acceptance Criteria
- [ ] Memory sekmesi: hafiza listesi, kind rozeti, olusturma tarihi, kaynak oturum
- [ ] Metin aramasi + kind'a gore filtre
- [ ] Tek hafizayi silme (onay ile) ve arsivleme
- [ ] Bir hafizanin hangi oturumdan geldigi gorulebiliyor
- [ ] Silinen hafiza sonraki oturumun baglamina girmiyor (dogrulanmis)
- [ ] Liste buyudugunde UI donmuyor (sayfalama veya sanal liste)

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

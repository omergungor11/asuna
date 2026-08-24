# Phase 3 — Memory

**Tarih**: 2026-08-24 | **Durum**: DRAFT
**Ilgili task'lar**: ASU-029..ASU-038 (detay: [`asuna-tasks/phases/phase-3.md`](../asuna-tasks/phases/phase-3.md))

**Kaynaklar** (spec burada kopyalanmaz, referans verilir):
[`docs/decisions/ADR-005-sqlite-access.md`](../docs/decisions/ADR-005-sqlite-access.md) ·
[`docs/architecture/memory.md`](../docs/architecture/memory.md) ·
`PROJECT.md` Bolum 12-14, 20, 25-26, 30-31

## Hedef

Bir oturumda soylenen kalici bir bilgi, uygulama tamamen kapatilip yeniden acildiktan sonra
hatirlansin; kullanici bu hafizayi gorebilsin, arayabilsin ve silebilsin. Milestone **M3**.

Bu, MVP'nin "chatbot degil companion" iddiasinin ilk somut kaniti: Asuna'nin oturumlar arasi
sureklilige sahip olmasi.

## Kapsam Disi

- **Embeddings / semantik retrieval (Stage B)** — Phase 3 deterministik kalir (PROJECT.md Bolum 13).
  `memories.embedding` kolonu acilir ama kullanilmaz. Backlog: `asuna-tasks/backlog.md`.
- **Semantik deduplication** — Phase 3'te metin tabanli/deterministik dedup yeterli.
- **`projects` / `tasks` / `tool_events` tablolari** — sirasiyla Phase 4, 6, 5.
  `project_id` alanlari simdilik nullable ve FK'siz.
- **SQLCipher / Keychain sifreleme** — ADR-005'te "gelecek" olarak isaretli, post-MVP.
- **Export/yedekleme UI'i** — RUNBOOK'taki `VACUUM INTO` yolu manuel olarak yeterli.
- **Otomatik hafiza temizligi/yaslandirma** — `expires_at` alani yazilir, politika Phase 3 sonrasi (memory.md T7).

## Teknik Yaklasim

ADR-005 karari geregi **SQLite'a yalnizca Rust process'inden erisilir**. Veri akisi:

```
React component
  → src/asuna/memory/memory-service.ts   (invoke wrapper, SQL yok)
  → #[tauri::command]  (kaba taneli, sema dogrulamasi IPC sinirinda)
  → src-tauri/src/db/  (rusqlite repository + Transaction)
  → asuna.db  (WAL, app_data_dir())
```

Uc bagli disiplin, hepsi ADR-005'ten:

1. **Komutlar kaba taneli.** `memory_create`, `memory_search`, `memory_delete`, `session_finalize`
   gibi; her SQL sorgusu icin komut acilmaz — yoksa Rust fn + command + izin + TS wrapper maliyeti patlar.
2. **Migration'lar `rusqlite_migration` ile gomulu**, `PRAGMA user_version` ile izlenir. Her `M::up`
   icin `M::down`; yazilmis migration bir daha **duzenlenmez**, duzeltme yeni `M` ekler.
3. **`src/shared/` tip aynasi.** Sema degisikligi ile TS tip aynasi **ayni commit'te** gider;
   `src/db/` diye bir TS sema dizini olusmaz.

Hafiza katmanlari ayri tutulur (PROJECT.md Bolum 14): ham transcript ≠ oturum ozeti ≠ durable memory.
Realtime modele "veritabanina yaz" yetkisi verilmez — cikarim ayri ve denetlenebilir bir adimdir.

## Sirali Adimlar

| # | Adim | Task'lar | Cikti / kapi |
|---|------|----------|--------------|
| 1 | **Sema + migration altyapisi** — DB acilis (WAL, foreign_keys, busy_timeout), `to_latest()`, `memories` + `sessions` tablolari, index'ler, tip aynasi | ASU-029, ASU-030 | Migration idempotent; hata halinde uygulama **cokmuyor**, hafizasiz modda devam ediyor |
| 2 | **Repository + transaction** — `MemoryService` CRUD, filtreler, `last_accessed_at`, `expires_at`, disabled modu | ASU-031 | Servis Tauri app'i olmadan birim testinde kosuyor |
| 3 | **Tauri command'lar + ACL gecisi** — `permissions/*.toml`, capability, `tauri.conf.json` dizisi, `build.rs` manifest; okuma/yazma ayri izin | ASU-031 (ayni task icinde) | `permissions/` acildiktan sonra **var olan tum komutlar** hala calisiyor (regresyon testi) |
| 4 | **Session kaydi + summary pipeline** — oturum acilis/kapanis kaydi, opsiyonel transcript persist, ayri cagri ile ozet | ASU-032, ASU-033 | Ozet basarisiz olsa da oturum kapaniyor; yarim oturum bir sonraki acilista kapatiliyor |
| 5 | **Memory extraction** — aday uretimi, sema dogrulamasi, dedup, onem esigi, hassas kategoride onay | ASU-034 | Working context durable memory'ye terfi etmiyor; her kayit `source_session_id` ile izlenebilir |
| 6 | **Stage A retrieval** — `SessionBootstrapContext`, deterministik siralama, boyut tavani, prompt enjeksiyonu | ASU-035 | Baglam bossa Asuna "hatirliyorum" gibi davranmiyor |
| 7 | **Memory UI + gizlilik kontrolleri** — listele/ara/sil/arsivle, toggle'lar, "tum hafizayi sil" | ASU-036, ASU-037 | Silinen hafiza sonraki oturumun baglamina girmiyor (dogrulanmis) |
| 8 | **M3 kabul testi** — restart sonrasi hatirlama, silme sonrasi uydurmama, `ASUNA_MEMORY_ENABLED=false` akisi | ASU-038 | Manuel senaryo `asuna-config/testing.md`'de |

Adim 1-3 tek bir dikey dilim gibi ilerlemeli: sema, servis ve ACL ayni anda dogru olmadan
UI tarafina gecilmez.

## Etkilenen Moduller

| Modul/Dizin | Degisiklik | Agent |
|-------------|-----------|-------|
| `src-tauri/src/db/` | Yeni: baglanti, migration'lar, `memories`/`sessions` repository, transaction | backend/db |
| `src-tauri/permissions/` | **Yeni dizin** — memory okuma/yazma izinleri (ACL etkisi asagida) | backend |
| `src-tauri/capabilities/`, `tauri.conf.json` | Yeni capability + `app.security.capabilities` dizisine identifier | backend |
| `src-tauri/build.rs` | `AppManifest::commands([...])` listesine yeni komutlar | backend |
| `src/asuna/memory/` | `memory-service.ts` — invoke wrapper, SQL yok | frontend |
| `src/shared/` | Rust tiplerinin aynasi (`memory.ts`, `session.ts`) | frontend |
| `src/asuna/prompts/` | Stage A baglaminin `buildAsunaInstructions(context)` ile enjeksiyonu (ASU-012 ile birlesir) | backend |
| UI (memory sekmesi + settings) | Liste/arama/silme/arsiv, gizlilik toggle'lari | frontend |
| `docs/architecture/memory.md` | T1-T7 TODO'lari kapanir | docs |

## Riskler

| Risk | Azaltma |
|------|---------|
| **`src-tauri/permissions/` dizini olusturuldugu an TUM uygulama komutlari ACL'e tabi olur** (ADR-005 spike'inda olculdu). Mevcut Phase 1 komutlari (token minting) sessizce reddedilebilir | Ayri bir **gecis adimi** olarak yapilir (Adim 3): dizin acilir, var olan her komut icin izin + capability yazilir, ardindan M1 akisi bastan sona tekrar kosulur. Tek commit, geri alinabilir |
| Yeni komut 4 adimin birini atlarsa **sessiz red** | Her yeni komutta checklist: `build.rs` manifest → `permissions/` → capability → `tauri.conf.json`. RUNBOOK "Sik karsilasilanlar" tablosunda kayitli |
| **Transcript gizlilik ayari** — `ASUNA_TRANSCRIPT_STORAGE=false` iken diske sizinti | Ayarin **davranissal** testi (dosya sistemi kontrolu), sadece flag testi degil; ayrica Realtime tarafinda `audio.input.transcription: null` (memory.md Bolum 5) |
| Memory extraction'in **hafiza uydurmasi** (never invent memories ihlali) | Cikarim realtime oturumundan ayri; sema dogrulamasi + onem esigi + `source_session_id` zorunlu; ASU-038'de silme sonrasi "bilmiyorum" davranisi test edilir |
| Extraction/ozetleme **ek LLM maliyeti** (R1) | Ozet modeli config'ten; cok kisa oturumlarda ozet uretilmiyor; maliyet oturum metadata'sina yazilip UI'da gorunuyor |
| Secret'in hafizaya yazilmasi | Extraction'da secret pattern filtresi + redaction unit testi (memory.md Bolum 5) |
| Sema ile TS tip aynasinin **kaymasi** | Ayni commit kurali (ADR-005); `migrations().validate()` birim testi CI'da kosar |
| DB hatasinda uygulamanin **olmesi** | PROJECT.md Bolum 30: hafizasiz mod + gorunur durum gostergesi; sessiz yutma yok |

## Acik Sorular

- [ ] `memories.kind` enum'unun **kesin** degerleri — spec'te serbest metin, ASU-030'da kilitlenecek (memory.md T2)
- [ ] Stage A siralama formulu + baglam boyut tavani (kac karakter/token) — ASU-035 (T3)
- [ ] `sessions` token/maliyet alanlarinin sekli — `Usage.inputTokensDetails` anahtarlari runtime'da dogrulanacak (T5)
- [ ] Onem esiginin varsayilan degeri ve konfigurasyon adi — ASU-034
- [ ] Onkosul karari: `phase-3.md` "Phase 2 ASU-028 gecmis olmali" diyor; Phase 2 wake word model
      secimi yuzunden bloklu. **Phase 3'un Phase 2'den once baslatilip baslatilmayacagi orchestrator karari** —
      teknik bagimlilik yok (tek gercek bagimlilik ASU-032 ↔ ASU-026 session close akisi)

## Task Dagilimi

| ID | Task | Agent | Complexity | Dependencies |
|----|------|-------|-----------|--------------|
| ASU-029 | SQLite bootstrap + migration altyapisi | db | L | ASU-005 |
| ASU-030 | `memories` + `sessions` schema | db | M | ASU-029 |
| ASU-031 | `MemoryService` CRUD (+ command'lar, ACL gecisi) | backend | M | ASU-030 |
| ASU-032 | Session kaydi + opsiyonel transcript persist | backend | M | ASU-030, ASU-026 |
| ASU-033 | Session summary pipeline | backend | M | ASU-032 |
| ASU-034 | Memory extraction pipeline | backend | L | ASU-033, ASU-031 |
| ASU-035 | Stage A retrieval + `SessionBootstrapContext` | backend | L | ASU-034 |
| ASU-036 | Memory UI (listele / ara / sil / arsivle) | frontend | M | ASU-031 |
| ASU-037 | Memory gizlilik kontrolleri | frontend | S | ASU-036 |
| ASU-038 | **M3 kabul testi** — restart sonrasi hatirlama | test | M | ASU-029..037 |

# Memory Architecture

> **İskelet — Phase 0 kapanış kaydı (2026-08-24, ASU-010).**
> Kaynak gerçek: `PROJECT.md` Bölüm 12-14. Bu dosya o spec'i kopyalamaz; **karara dönmüş
> halini** ve erişim mimarisini özetler. Uygulanmamış her şey `TODO` işaretli.
> Erişim katmanı kararı: [`docs/decisions/ADR-005-sqlite-access.md`](../decisions/ADR-005-sqlite-access.md).

## 1. Katmanlar

Asuna'da "hafıza" tek bir şey değil. İki katman **kod düzeyinde ayrı** tutulur
(PROJECT.md 14) — working context'teki hiçbir şey otomatik olarak durable memory olmaz.

| Katman | Ömür | Nerede yaşar | İçerik |
|---|---|---|---|
| **Working context** | Oturum | RAM + Realtime session history | aktif dosya, terminal hatası, branch, aktif task, son tool sonucu, konuşmanın kendisi |
| **Durable memory** | Kalıcı | SQLite (`memories`) | proje amacı, mimari karar, kalıcı tercih, stabil workflow, önemli entegrasyon, milestone |

Promotion (working → durable) **açık bir adımdır**: memory extraction pipeline (ASU-034)
aday üretir, kaydetme kriterleri PROJECT.md 26'da. Otomatik "her şeyi sakla" yok.

## 2. Şema özeti

Beş tablo (alan listeleri PROJECT.md 12.2'de — burada tekrarlanmaz, sadece rolleri):

| Tablo | Rol | İlk gelen task |
|---|---|---|
| `memories` | durable memory kayıtları; `kind`, `importance`, `confidence`, `source_session_id`, `expires_at`, `is_archived`, `embedding` (Stage B için ayrılmış, MVP'de NULL) | ASU-030 |
| `sessions` | oturum kaydı: süre, model, token/maliyet metadatası, özet, opsiyonel `transcript_path` | ASU-030 |
| `projects` | kayıtlı proje root'ları + metadata (path, dil, framework, git remote) | ASU-039 |
| `tasks` | açık/kapalı işler — Phase 6 "beni toparla" girdisi | ASU-056 |
| `tool_events` | tool audit trail (zaman, tool, risk, redakte argüman, onay durumu, sonuç) | ASU-050 |

Kurallar:

- `memories.source_session_id` **zorunlu değil ama kuvvetle beklenir** — UI'da "bu neden
  hatırlanıyor?" sorusunun cevabı bu alan (incelenebilirlik ilkesi, PROJECT.md 5.3).
- `embedding` kolonu MVP'de yazılmaz. Stage B'ye kadar NULL kalır; şemada durması
  sonradan migration açmamak için.
- Şema değişikliği ve `src/shared/` TypeScript tip aynası **aynı commit'te** gider (ADR-005).

### 2.1 `memories.kind` (T2 kapandı — ASU-030)

Kesin ve kapalı liste, PROJECT.md 5.3 ile birebir:

`profile` · `preference` · `project` · `decision` · `task` · `working_context` ·
`relationship` · `idea` · `routine` · `tool_state`

`working_context` listede ama durable tabloya **kural olarak terfi etmez** (PROJECT.md 14);
extraction bir adayı bu sınıfta işaretleyip eleyebilsin diye tanımlı.

**Tek kaynak**: değerler `001_memories_sessions.up.sql` içindeki `kind` CHECK kısıtındadır.
Rust `MemoryKind` ve TS `MEMORY_KINDS` o satıra **testlerle** bağlıdır; üçünden birine
dokunmadan yapılan değişiklik kırmızı test üretir (`db/model.rs`, `src/shared/schema-mirror.spec.ts`).

### 2.2 Şema kararları (ASU-030)

| Karar | Gerekçe |
|---|---|
| `STRICT` tablolar | Tip zorlaması INSERT anında; `importance = 'çok'` aylar sonra okuma hatası olarak çıkmaz |
| `id INTEGER PRIMARY KEY` | rowid alias; UUID için yeni bir crate gerekirdi, tek kullanıcılı yerel DB'de karşılığı yok |
| Zaman alanları `TEXT` + `GLOB` CHECK | UTC ISO-8601 zorunlu. Stage A sıralaması metin sıralamasıdır; karışık format sessizce yanlış sonuç verir |
| `source_session_id` → `sessions(id) ON DELETE SET NULL` | Oturum silinince hafıza **silinmez**, yalnızca izi kopar |
| `project_id` nullable, FK'siz | `projects` Phase 4 (ASU-039). Migration planı şema dosyasında not edilmiş |
| `metadata_json` NOT NULL DEFAULT `'{}'` + `json_valid` | Okuyucu tarafta `null` dalı yok; bozuk JSON DB'ye giremez |
| Token/maliyet: skaler kolonlar + `usage_json` | Ayrıntılı kırılımın anahtarları doğrulanmadı (T5); netleşince ASU-032 yeni migration ile kolon açar |
| Stage A için bileşik index | `(is_archived, project_id, importance DESC, created_at DESC)` — tek tek kolon index'leri bu sorguya yetmez |

## 3. Erişim mimarisi (ADR-005 sonucu)

**Renderer SQL yazmaz.** SQLite'a yalnızca Tauri'nin Rust process'inden erişilir.

```text
React component
  └─ src/asuna/memory/memory-service.ts      (invoke wrapper, tip aynası)
       └─ #[tauri::command] memory_search / memory_create / ...   (kaba taneli, SQL almaz)
            └─ src-tauri/src/db/  — rusqlite 0.40.2 (bundled) + rusqlite_migration 2.6.0
                 └─ ~/Library/Application Support/com.omergungor.asuna/asuna.db  (WAL)
```

| Konu | Karar |
|---|---|
| Kütüphane | `rusqlite` 0.40.2 (bundled SQLite 3.53.2) — `tauri-plugin-sql` ölçümle elendi |
| Migration | `rusqlite_migration` 2.6.0, `PRAGMA user_version`, açılışta `to_latest()` idempotent |
| DB yolu | `app_data_dir()` ile çözülür; **renderer'dan asla parametre alınmaz** |
| Dev override | `ASUNA_DB_PATH` yalnızca `#[cfg(debug_assertions)]` build'lerde |
| Açılış PRAGMA'ları | `journal_mode=WAL`, `foreign_keys=ON`, `synchronous=NORMAL`, `busy_timeout=5s` |
| Komut granülaritesi | Kaba taneli (`memory_create`, `memory_search`, `session_finalize`) — her SQL için komut açılmaz |
| ACL | Komut başına `src-tauri/permissions/*.toml` + capability; okuma/yazma ayrı izin |
| Atomiklik | `memories` + `tool_events` yazımı tek `rusqlite::Transaction` |
| `tool_events` | Renderer'ın yazma/silme yolu **yoktur** — audit sadece Rust tarafından üretilir |

**Migration disiplini:** her migration bir kez yazılır, bir daha değiştirilmez; değişiklik
yeni bir `M` ekler. Her `M::up` için `M::down` yazılır. `migrations().validate()` bir unit
testte koşar (CI gate).

**Şifreleme (OQ-9):** MVP'de düz dosya. SQLCipher geçişi tek feature değişikliği
(`bundled` → `bundled-sqlcipher`) + açılışta `PRAGMA key`; renderer etkilenmez.

### 3.1 Uygulanan hal (ASU-029)

| Parça | Dosya |
|---|---|
| Bağlantı, PRAGMA'lar, yol çözümü, hata tipi | `src-tauri/src/db/mod.rs` |
| Migration listesi (sıralı, değişmez) + DDL | `src-tauri/src/db/migrations/` |
| Satır tipleri (`MemoryKind`, `MemoryRecord`, `SessionRecord`) | `src-tauri/src/db/model.rs` |
| Durum + `db_status` komutu | `src-tauri/src/db/state.rs` |
| ACL regresyon testi (gerçek capability'ler) | `src-tauri/src/acl_regression.rs` |
| TS tip aynası | `src/shared/db-status.ts`, `memory.ts`, `session.ts` |
| Şema ↔ TS senkron testi | `src/shared/schema-mirror.spec.ts` |
| Renderer servis katmanı | `src/asuna/memory/db-status-service.ts` |

**Hafızasız degrade mod.** DB açılışı `Result` döndürür ve açılış hatası uygulamayı
durdurmaz. `DbState` üç ayrı durumu tip düzeyinde temsil eder:

| Durum | Anlamı | UI |
|---|---|---|
| `ready` | Hafıza çalışıyor | normal |
| `disabled` | `ASUNA_MEMORY_ENABLED=false` — DB dosyası **hiç açılmaz** | "hafıza kapalı" |
| `unavailable` | Açılış/migration başarısız | "hafıza şu an çalışmıyor" + neden |

`disabled` ile `unavailable` bilerek ayrı: biri kullanıcının kararı, diğeri arıza.
Repository katmanı `DbState::database()` üzerinden `Option<&AsunaDb>` alır — "DB var"
varsayımı yapan kod yazılamaz.

**Hata mesajı disiplini.** `DbError`'ın hiçbir varyantının `Display` metni dosya yolu,
SQL sorgusu veya kullanıcı içeriği taşımaz (IPC'ye ve log'a gidebilir). Ayrıntı
`#[source]` zincirindedir ve yalnızca yerel log'a yazılır.

**ACL geçişi.** Asuna'da uygulama komutları ASU-009'dan beri deny-by-default
(`build.rs` içindeki açık `AppManifest`), yani ADR-005'in "`permissions/` dizini
açılınca her şey ACL'e tabi olur" tuzağı **zaten** yaşanmış ve kapatılmış durumda.
Yeni bir komut eklemenin dört adımı değişmedi: `build.rs` manifest → üretilen
`permissions/autogenerated/<cmd>.toml` → `capabilities/*.json` → `tauri.conf.json`
`app.security.capabilities` dizisi. Dördü de statik testlerle ve gerçek ACL üzerinde
koşan bir IPC regresyon testiyle bağlanmıştır.

## 4. Retrieval — Stage A (MVP)

Deterministik önce, akıllı sonra (PROJECT.md 13). MVP'de **yalnızca Stage A** vardır.

Oturum açılışında `SessionBootstrapContext` (ASU-035) şunları enjekte eder:

1. aktif proje özeti (proje biliniyorsa),
2. o projenin en son karar memory'leri,
3. tamamlanmamış son task'lar,
4. bir önceki oturumun özeti.

Kurallar:

- Her oturumda **tüm DB modele dökülmez** — retrieval dar ve açıklanabilir olur.
- Stage A tamamen SQL sıralaması: proje eşleşmesi + `importance` + tazelik. Embedding yok.
- Enjekte edilen context `instructions` içinde text token olarak sayılır → her turda
  faturalanır (bkz. `voice.md` Bölüm 6). Boyut sınırı ölçülerek belirlenir.

**Stage B (semantik retrieval)** ve **Stage C (konsolidasyon)** MVP kapsamı dışı — yeterli
memory birikip Stage A'nın yetmediği **ölçülene** kadar açılmaz.

### 4.1 Uygulanan hâli (ASU-035)

Kod: `src-tauri/src/db/retrieval.rs` (üretim) + `src/asuna/memory/bootstrap-context.ts`
(prompt'a çevirme). Komut: `get_bootstrap_context` — **parametresiz**; aktif proje, tür
politikası, sıralama ve boyut tavanı host tarafındadır, renderer değiştiremez.

| Bölüm | Kaynak sorgu | Tavan |
|---|---|---|
| `userPreferences` | `kind = preference`, `importance DESC` | 8 kayıt |
| `recentSession` | son `end_reason = 'completed'` + `summary IS NOT NULL` oturum | 250 kelime |
| `relevantMemories` | proje biliniyorsa `project`+`decision` (o proje), sonra global; `importance DESC, created_at DESC` | 12 kayıt |
| `currentProject` | — (Phase 4 / ASU-039+) | tipli ama boş |
| `activeTasks` | — (Phase 6) | tipli ama boş |

Her sorguda ortak filtre: `is_archived = 0`, süresi geçmemiş, `pendingApproval IS NOT 1`,
`markAccessed: true`. Bu bileşim `idx_memories_stage_a` index'inin karşıladığı sorgudur.

**Boyut tavanı (T3):** paket toplamı **2000 kelime**; kalem tavanları hafıza başına 120,
oturum özeti 250 kelime. Ölçüm `ContextBudget` ile geri döner (`wordCount`, `included`,
`dropped`, `truncated`) ve iki tarafta da log'lanır. Bütçe taşarsa **ilk taşmada durulur**;
düşen ilk şey en düşük önemli ilgili hafızalardır — tercihler ve son oturum özeti korunur.

Prompt tarafı: bölümler `buildAsunaInstructions({ additionalSections })` ile çekirdek
prompt'un ardına eklenir. **Bağlam boşsa** bölüm eklenmez ama prompt'a açık bir satır girer
("Kalıcı hafıza boş — ... hatırlıyormuş gibi davranma"); kapalı hafıza ve okunamayan hafıza
için ayrı iki cümle vardır. Hafıza okunamazsa konuşma **bloklanmaz**.

## 5. Gizlilik kancaları

| Kontrol | Değişken / mekanizma |
|---|---|
| Durable memory tamamen kapatılabilir | `ASUNA_MEMORY_ENABLED=false` |
| Ham transcript diske yazılmasın | `ASUNA_TRANSCRIPT_STORAGE=false` (ayrıca Realtime'da `audio.input.transcription: null`) |
| İncelenebilirlik | Memory UI: listele / ara / sil / arşivle (ASU-036) |
| Hassas kategoriler | sağlık, finans, kimlik, 3. şahıs, credential benzeri içerik → otomatik yazılmaz, onay ister |
| Secret sızıntısı | Memory extraction secret pattern'lerini filtreler; redaction unit testi zorunlu |

Detaylı checklist: [`asuna-config/security.md`](../../asuna-config/security.md) Bölüm 5.

## 6. TODO — Phase 3'te kapanacak

| # | Açık | Nerede |
|---|---|---|
| ~~T1~~ | ~~Gerçek `CREATE TABLE` DDL'leri~~ — **kapandı (ASU-030)**: `src-tauri/src/db/migrations/001_memories_sessions.up.sql` | ASU-029/030 |
| ~~T2~~ | ~~`memories.kind` enum değerleri~~ — **kapandı (ASU-030)**: Bölüm 2.1 | ASU-030 |
| ~~T3~~ | ~~Stage A sıralama formülü + context boyut tavanı~~ — **kapandı (ASU-035)**: Bölüm 4.1 | ASU-035 |
| T4 | Memory extraction promptu ve kaydetme eşikleri (PROJECT.md 26) | ASU-034 |
| T5 | `sessions` token/maliyet alanlarının şekli — skaler kolonlar + `usage_json` ile geçici olarak karşılandı; `Usage.inputTokensDetails` anahtarları runtime'da doğrulanınca kolon açılacak (`voice.md` V9) | ASU-032 |
| T6 | Export/yedekleme: WAL yüzünden 3 dosya var → `VACUUM INTO` yolu | Phase 3 export |
| T7 | `expires_at` / `is_archived` yaşam döngüsü politikası (kim ne zaman temizler) | Phase 3 |
| T8 | Stage B tetikleme kriteri — "yeterli memory" kaç kayıt, hangi metrik | Post-MVP |

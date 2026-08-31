# Asuna - Task Index

> Kaynak gercek: `PROJECT.md` (urun/mimari), `TRANSCRIPT.md` (urun niyeti), `CLAUDE.md` (agent kurallari).
> Task ID formati: `ASU-XXX`. Commit formati: `feat(ASU-XXX): aciklama`.
> **Dikey dilim kurali:** bir phase'in kabul testi gecmeden sonraki phase'e gecilmez.

## Dashboard

| Phase | Ad | Total | Done | In Progress | Review | Pending | Blocked |
|-------|----|-------|------|-------------|--------|---------|---------|
| 0 | Arastirma + Scaffold | 11 | 11 | 0 | 0 | 0 | 0 |
| 1 | Realtime Voice (dikey dilim) | 11 | 11 | 0 | 0 | 0 | 0 |
| 2 | Wake Word | 8 | 1 | 0 | 0 | 7 | 0 |
| 3 | Memory | 12 | 10 | 0 | 0 | 2 | 0 |
| 4 | Project Context | 8 | 7 | 0 | 0 | 1 | 0 |
| 5 | One Useful Action (Tools) | 14 | 12 | 0 | 0 | 2 | 0 |
| 6 | Focus Recovery (MVP) | 8 | 0 | 0 | 0 | 8 | 0 |
| 7 | Chat Shell (pivot) | 8 | 7 | 0 | 0 | 1 | 0 |
| **Total** | | **80** | **59** | **0** | **0** | **21** | **0** |

**Progress**: 59/80 (74%)

> **Phase 7 — Chat Shell pivotu kapandi (2026-08-31): ASU-072..078 DONE.** Kullanici karariyla
> Asuna ChatGPT/Claude-tarzi bir **kalici konusma** arayuzune donustu; ses silinmedi, **voice mode**
> olarak kaldi (`VoicePanel` asla unmount edilmez). Karar ve gerekce: `asuna-docs/DECISIONS.md`
> → **ADR-008** (plan dosyasi bunu "ADR-006" diye anar — numara cakismasi, dogrusu ADR-008).
> Uygulanan: migration 006 (`messages` + `attachments`, `sessions.title`/`modality`, CASCADE),
> Rust `chat_send` proxy'si (`OPENAI_API_KEY` Rust'ta kalir; **zorunlu** yeni env
> `ASUNA_CHAT_MODEL`; non-streaming; kullanici + asistan mesaji **tek transaction**),
> `attachment_ingest` (ad blocklist + `redact_sensitive_text` + 24k kirpma) ve
> `attachment_from_project` (mevcut sandbox'li `projects::files::read` cekirdegi, yalnizca **aktif**
> proje), yeni capability `asuna-chat.json` (`message_append` **bilerek kayitsiz** — renderer
> asistan mesaji uyduramaz; `session_set_title` `asuna-session.json`'da kalir), TS sozlesmesi
> (`src/shared/chat.ts` + `chat-service.ts`) ve chat kabugu (sidebar + tarih gruplu konusma listesi
> + chat-view + composer + proje dosya secici).
> Gate 3 review (opus reviewer): **0 CRITICAL**, 1 HIGH (ses oturumu rozeti) + 4 MEDIUM + 7 LOW;
> HIGH/MEDIUM'lar kapatildi, secilen LOW'lar `backlog.md`'ye tasindi. Tester +70 test ekledi ve
> redaksiyon desen bosluklarini (PEM / `AKIA` / `ghp_` / JWT) buldu — kapatildi.
> **736+ Rust / 1091+ TS** testi yesil. Acik: **ASU-079** (M6 kabul testi — kullanicida).

> **Phase 5 Wave D kapandi (2026-08-31): ASU-067..070 DONE.** M4 canli testinde gorulen gercek
> bosluklar icin dort proje farkindaligi tool'u: `list_projects` (risk 0), `list_project_files`
> (risk 0; 200 girdi cikti tavani + 5000 tarama tavani `scanCapped` ile ayri ayri raporlanir,
> bloklu girdiler gorunur ama isaretli ve boyutsuz), `register_project` (**risk 2**, her modda
> onay), `set_current_project` (risk 1, onayli; ad/id belirsizliginde secim yapmaz). Registry:
> dort risk 0, iki risk 1, bir risk 2 — risk 3 yok.
> Gate 3 review (opus reviewer): 1 CRITICAL + 1 HIGH + 3 MEDIUM + 2 LOW, hepsi kapatildi.
> **C1**: kok kayit dogrulamasi tam-eslesme ile yazilmisti; `/Users` gibi bir **ata** dizin ve
> `/System/Volumes/Data/...` firmlink'i korumayi atlatiyordu — ata reddi + `/System|/Library|
> /Applications|/Network` on-ek reddi + firmlink normalizasyonu ile kapandi. Ayni incelemede
> Wave D **oncesine** ait bir acik da bulundu: `project_add` ev dizinini ve `~/.ssh`'i kabul
> ediyordu; ret artik Rust tarafinda ve UI yolunu da kapsiyor.
> Kararlar: `asuna-docs/DECISIONS.md` → "Phase 5 kararlari" (register_project risk 2, kok kayit
> dogrulama kurallari). Acik: **ASU-071** (Wave D sesli kabul testi — kullanicida).
> 730 Rust + 834 TS (scoped) testi yesil, clippy temiz.

> **Phase 5 Wave C kapandi (2026-08-31): ASU-051..054 DONE.** Iki yeni tool acildi —
> `read_project_file` (risk 0, ASU-049 sandbox'i + blocklist, once redaksiyon sonra 6000 karakter
> kirpma) ve `open_project` (risk 1, `ASUNA_EDITOR_COMMAND`, shell'siz alt process). Onay karti
> (`tool-approval-card.tsx`) ve "Araclar" sekmesi (tool listesi + oturum-yerel toggle + salt-okunur
> audit gecmisi) baglandi; migration 005 `tool_events.outcome` kolonunu ekledi (sema surumu 5).
> Gate 3 review (opus reviewer): 1 CRITICAL + 2 HIGH + 3 MEDIUM + 3 LOW; C1/H1/M1/M2/M3/L1
> kapatildi. 907 TS + 592 Rust testi yesil.
> Kararlar: `asuna-docs/DECISIONS.md` → "Phase 5 kararlari".
> Acik maddeler: **ASU-055** (sesli/manuel M4 kabul testi — kullanicida), Gate 3 **H2** (kullanicinin
> `.env` dosyasina `ASUNA_EDITOR_COMMAND=code` satiri eklenmeli, yoksa uygulama acilista
> `ConfigError::Missing` ile durur), onay istegi icin **ayri overlay penceresi yok**
> (ASU-053 son kriteri → `backlog.md`).

> Phase 3 implementasyonu ASU-035 ile kapandi (2026-08-25): ASU-029..ASU-037 DONE.
> Gate 3 review bulgulari duzeltildi (2026-08-25): calisma zamani gizlilik kapisi oturum
> yoluna da eklendi, saklanan metinde secret redaksiyonu, dedup esigi yukseltildi
> (`asuna-docs/DECISIONS.md` → "Phase 3 kararlari").
> Acik madde: **ASU-038** — M3 manuel kabul testi (gercek mikrofon + uygulama restart'i).
> **ASU-065 one cekildi ve tamamlandi (2026-08-25)**: M3 kabul testi gercek bir acik yakaladi —
> kullanici hafiza kayitlarini sildi ama Asuna hatirlamaya devam etti, cunku Stage A son oturum
> ozetini enjekte ediyor ve `sessions.summary` urun icinden silinemiyordu. Artik silinebiliyor
> (`Hafiza > Oturumlar` ve `Ayarlar > Konusma gecmisini sil`), yani ASU-038'in "silindikten
> sonra uydurmuyor" kriteri gercekten olculebilir.

> ASU-008b spike tamamlandi (2026-08-24): motor/lisans/CPU/bundle dogrulandi, ADR-004 accepted
> (kapsami daraltilmis). ACIK: model+ifade secimi — gigaspeech-3.3M "Hey Asuna"yi tasimiyor (R2).

> Not: PROJECT.md Bolum 32'deki "Phase 0 — Template audit" tamamlanmis sayilir. Repo'da uygulama kodu
> yok, sadece Claude Code workflow meta-template'i var (`asuna-tasks/`, `asuna-docs/`, `asuna-config/`,
> `asuna-plans/`, `.claude/`). Bu yuzden Phase 0 yeniden yorumlandi: **teknik arastirma + greenfield scaffold**.

## Milestones

| Milestone | Hedef | Phase'ler | Kanit (nasil dogrulanir) | Durum |
|-----------|-------|-----------|--------------------------|-------|
| M1 | **Sesli konusma calisiyor** — butona bas, Turkce konus, Asuna cevap versin, sozunu kesebil, transcript gor, temiz kapat | 0-1 | ASU-020 | **DONE (2026-08-24)** — canli test: Turkce anlasildi, barge-in sorunsuz, temiz kapanis. Gecikme iyilestirmesi ASU-064 ile kapandi (turn detection env'den ayarlanabilir) |
| M2 | **"Hey Asuna" ile uyaniyor** — idle'da bulut'a ses gitmiyor, wake word ile oturum aciliyor, timeout ile kapaniyor | 2 | ASU-028 | PENDING |
| M3 | **Hatiriyor** — oturum kapanir, uygulama yeniden baslar, onceki oturumun karari hatirlanir | 3 | ASU-038 | PENDING |
| M4 | **Projeleri taniyor + ilk tool** — hangi projede oldugunu soyleyebiliyor, onayli tool calistiriyor, audit'e yaziyor | 4-5 | ASU-046 + ASU-055 + ASU-071 | PENDING |
| M5 | **"Beni toparla" calisiyor (MVP)** — gercek proje state'inden tek somut sonraki adim uretiliyor | 6 | ASU-062 (PROJECT.md Bolum 33 checklist) | PENDING |
| M6 | **Metinle de calisiyor** — konusma yaz, yanit gelir, uygulama restart edilir, konusma ve mesajlar yerinde; dosya eklenir (`.env` reddedilir, secret redakte edilir); bir projede konusma baslatilir; ses voice mode olarak calismaya devam eder | 7 | ASU-079 | PENDING |

## Risks

| ID | Risk | Olasilik | Etki | Azaltma | Durum |
|----|------|----------|------|---------|-------|
| R1 | **Realtime API maliyeti** — surekli acik ses oturumu faturayi hizla buyutur; ChatGPT aboneligi API kredisi vermez (PROJECT.md Bolum 28) | H | H | Idle'da oturum yok (Phase 2), inactivity timeout + max session duration (ASU-025), dev'de `gpt-realtime-2.1-mini`, oturum suresi/maliyet metadata takibi | OPEN |
| R2 | **KWS modeli "Hey Asuna"yi tasimiyor (ASU-008b OLCTU)** — gigaspeech-3.3M sozlugunde `ASUNA` yok; ortografik tespit %0, fonetik workaround en iyi %81 @ 54.8 FA/saat (hedef >%95 @ <0.125), Turkce telaffuz ~%0. Motor/lisans/CPU/bundle YESIL (%2.3 CPU, 38MB, Apache-2.0, +20.7MB app). | H | M | Once GERCEK MIKROFON testi (30 dk, harness `spike/asu-008b-kws` branch'inde — TTS telaffuz artefakti olabilir); sonra sirayla: daha buyuk model (zh-en-3M), vocabulary-aware ifade secimi, kendi model egitimi, `oww-rs`/`rustpotter`. Phase 2 bu cozulmeden baslamaz; Phase 1 etkilenmez | OPEN |
| R3 | ~~WKWebView WebRTC riski~~ KAPANDI (ASU-007): calisiyor — gUM+kalici TCC, SDP/DTLS/srflx, autoplay engelsiz; fallback gerekmedi. Yeni bulgu: prod CSP'ye api.openai.com eklendi (dev'de gorunmeyen blocker, duzeltildi) | - | - | voice.md Bolum 11 | CLOSED |
| R4 | ~~SQLite erisim mimarisi~~ KAPANDI (ASU-005): B — Rust servis (rusqlite), ADR-005 accepted; tauri-plugin-sql olcumle elendi | - | - | Karar docs/decisions/ADR-005-sqlite-access.md | CLOSED |
| R5 | **Tek gelistirici odak riski** — TRANSCRIPT.md Bolum 1/6: projeyi bitirememe, dikkat dagilmasi urun probleminin ta kendisi | H | H | Dikey dilim disiplini (bir phase bitmeden digerine gecme), her phase sonunda calisir demo, kucuk task boyutlari, `asuna-tasks/active/session-notes.md` ile oturum devamliligi | OPEN |
| R6 | **Model ID / SDK degisimi** — `gpt-realtime-2.1` erisilemez olabilir, Agents SDK realtime API'si degisebilir | M | M | Model ID asla hard-code degil (`ASUNA_REALTIME_MODEL`), SDK `AsunaRealtimeService` arkasinda izole (ASU-013), ASU-006'da surum pinlenir | OPEN |
| R7 | **Kapsam kaymasi — dashboard'a donusme** — TRANSCRIPT.md Bolum 19'un acikca reddettigi sonuc | M | H | UI task'lari her phase'de minimum tutulur; MVP disi her fikir `backlog.md`'ye; PROJECT.md Bolum 4 non-goals listesi degismez | OPEN |
| R8 | **Turkce ses dogrulugu** — Turkce konusmada transcript/intent kalitesi ve "Hey Asuna" telaffuz varyasyonlari | M | M | Kabul testleri Turkce yapilir (ASU-020, ASU-028), wake word hassasiyeti konfigurabilir, `beni toparla` intent'i sadece LLM'e birakilmaz (ASU-059) | OPEN |
| R9 | **Gizlilik ihlali algisi** — idle mikrofon dinlemesi kullanicinin guvenini kaybettirirse urun kullanilmaz | L | H | ASU-024 ile idle'da network trafigi olmadigi test edilerek dogrulanir; aktif dinleme her zaman gorunur; transcript saklama konfigurabilir (ASU-037) | OPEN |

---

## Master Task Listesi

| ID | Baslik | Phase | Boyut | Scope | Durum |
|----|--------|-------|-------|-------|-------|
| ASU-001 | Repo iskeleti + pnpm workspace | 0 | S | devops | COMPLETED |
| ASU-002 | Tauri 2 + React + TS + Vite scaffold (bos pencere acilir) | 0 | L | devops | COMPLETED |
| ASU-003 | TypeScript strict + ESLint + Prettier | 0 | S | devops | COMPLETED |
| ASU-004 | CI pipeline yesil | 0 | M | devops | COMPLETED |
| ASU-005 | [ARASTIRMA] SQLite erisim mimarisi karari + ADR-005 | 0 | M | research | COMPLETED |
| ASU-006 | [ARASTIRMA] OpenAI Agents SDK realtime dogrulamasi + surum pinleme | 0 | M | research | COMPLETED |
| ASU-007 | [ARASTIRMA] Tauri webview mikrofon + WebRTC spike | 0 | M | research | COMPLETED |
| ASU-008 | [ARASTIRMA] Wake word saglayicisi (sherpa-onnx KWS) + lisans | 0 | M | research | RESEARCH DONE |
| ASU-008b | [SPIKE] sherpa-onnx KWS detection spike (macOS arm64) | 0 | L | research | COMPLETED |
| ASU-009 | Konfigurasyon katmani + `.env.example` | 0 | S | backend | COMPLETED |
| ASU-010 | `docs/architecture` + ADR dizini + README local run | 0 | S | docs | COMPLETED |
| ASU-011 | Ephemeral Realtime token minting (Rust) | 1 | L | backend | COMPLETED |
| ASU-012 | Asuna core prompt / instructions dosyasi | 1 | S | backend | COMPLETED |
| ASU-013 | `AsunaRealtimeService` (SDK wrapper) | 1 | L | backend | COMPLETED |
| ASU-014 | Voice state machine | 1 | M | frontend | COMPLETED |
| ASU-015 | "Talk to Asuna" gecici butonu + baglanti akisi | 1 | M | frontend | COMPLETED |
| ASU-016 | Iki yonlu ses + interruption (barge-in) | 1 | M | frontend | COMPLETED |
| ASU-017 | Canli transcript UI | 1 | M | frontend | COMPLETED |
| ASU-018 | Temiz disconnect + kaynak temizligi | 1 | M | frontend | COMPLETED |
| ASU-019 | Hata yonetimi + observability (state transition log) | 1 | M | backend | COMPLETED |
| ASU-020 | **M1 kabul testi** — PROJECT.md Bolum 35, 8 madde | 1 | M | test | COMPLETED |
| ASU-021 | `WakeWordProvider` interface + fake provider | 2 | M | backend | COMPLETED |
| ASU-022 | `SherpaKwsProvider` (sherpa-onnx KWS, "Hey Asuna") | 2 | L | backend | PENDING |
| ASU-023 | IDLE_WAKE_WORD -> WAKING -> CONNECTING gecisi | 2 | M | frontend | PENDING |
| ASU-024 | Idle'da buluta ses gitmedigi dogrulamasi | 2 | M | test | PENDING |
| ASU-025 | Inactivity timeout + max session duration | 2 | M | backend | PENDING |
| ASU-026 | Session close akisi (sesli kapanis + stop) | 2 | M | backend | PENDING |
| ASU-027 | Minimal idle overlay / tray gostergesi | 2 | M | frontend | PENDING |
| ASU-028 | **M2 kabul testi** | 2 | S | test | PENDING |
| ASU-029 | SQLite bootstrap + migration altyapisi | 3 | L | db | DONE |
| ASU-030 | `memories` + `sessions` schema | 3 | M | db | DONE |
| ASU-031 | `MemoryService` CRUD | 3 | M | backend | DONE |
| ASU-032 | Session kaydi + opsiyonel transcript persist | 3 | M | backend | DONE |
| ASU-033 | Session summary pipeline | 3 | M | backend | DONE |
| ASU-034 | Memory extraction pipeline (PROJECT.md Bolum 26) | 3 | L | backend | DONE |
| ASU-035 | Stage A deterministik retrieval + `SessionBootstrapContext` | 3 | L | backend | DONE |
| ASU-036 | Memory UI (listele / ara / sil / arsivle) | 3 | M | frontend | DONE |
| ASU-037 | Memory gizlilik kontrolleri (toggle'lar) | 3 | S | frontend | DONE |
| ASU-038 | **M3 kabul testi** — restart sonrasi hatirlama | 3 | M | test | PENDING |
| ASU-039 | `projects` tablosu + migration | 4 | S | db | DONE |
| ASU-040 | `ProjectRegistry` (kayitli proje root'lari) | 4 | M | backend | DONE |
| ASU-041 | `ProjectContextService` (metadata + context dosyalari) | 4 | L | backend | DONE |
| ASU-042 | Git metadata provider | 4 | M | backend | DONE |
| ASU-043 | `.asuna/context.json` okuma/yazma | 4 | M | backend | DONE |
| ASU-044 | `get_current_project` tool (risk 0) | 4 | M | backend | DONE |
| ASU-045 | Projects UI sekmesi | 4 | M | frontend | DONE |
| ASU-046 | **Phase 4 kabul testi** | 4 | S | test | PENDING |
| ASU-047 | `AsunaToolDefinition` + tool registry | 5 | L | backend | DONE |
| ASU-048 | Risk / approval policy katmani | 5 | M | backend | DONE |
| ASU-049 | Path sandbox + hassas dosya blocklist | 5 | M | backend | DONE |
| ASU-050 | `tool_events` tablosu + audit logger | 5 | M | db | DONE |
| ASU-051 | `read_project_file` tool (risk 0, sandbox'li) | 5 | M | backend | DONE |
| ASU-052 | `open_project` tool (risk 1) | 5 | M | backend | DONE |
| ASU-053 | Approval UI (AWAITING_APPROVAL) | 5 | M | frontend | DONE |
| ASU-054 | Tool call gorunurlugu (TOOL_PENDING) | 5 | M | frontend | DONE |
| ASU-055 | **M4 kabul testi** + guvenlik unit testleri | 5 | M | test | PENDING |
| ASU-056 | `tasks` tablosu + `TaskService` | 6 | M | db | PENDING |
| ASU-057 | Aktif task + blocker retrieval | 6 | M | backend | PENDING |
| ASU-058 | `FocusRecoveryService` ("beni toparla" orkestrasyonu) | 6 | L | backend | PENDING |
| ASU-059 | "beni toparla" intent tanima + prompt entegrasyonu | 6 | M | backend | PENDING |
| ASU-060 | Focus recovery UI (tek sonraki adim karti) | 6 | M | frontend | PENDING |
| ASU-061 | Halusinasyon korumasi (state yoksa uydurma) | 6 | M | test | PENDING |
| ASU-062 | **M5 / MVP kabul checklist** — PROJECT.md Bolum 33 | 6 | L | test | PENDING |
| ASU-063 | README + RUNBOOK + v0.1.0 release | 6 | M | docs | PENDING |
| ASU-064 | Realtime gecikme ayari — turn detection konfigurasyonu + olcum | 1 | S | backend | COMPLETED |
| ASU-065 | Oturum ozeti + transcript temizligi — silme aksiyonu (M3 blokaji, one cekildi) | 3 | M | full-stack | COMPLETED |
| ASU-066 | Cmd+Q finalize yarisi — cikista `session_finalize` tamamlanmadan process olebilir | 3 | M | backend | PENDING |
| ASU-067 | `list_projects` tool (risk 0) | 5 | S | backend | DONE |
| ASU-068 | `list_project_files` tool (risk 0) + `list_project_dir` komutu | 5 | M | backend | DONE |
| ASU-069 | `register_project` tool (risk 2, her modda onay) | 5 | M | backend | DONE |
| ASU-070 | `set_current_project` tool (risk 1, onayli) | 5 | S | backend | DONE |
| ASU-071 | **Wave D sesli kabul testi** — proje farkindaligi tool'lari | 5 | S | test | PENDING |
| ASU-072 | Migration 006 — `messages` + `attachments` tablolari, `sessions.title`/`modality` | 7 | M | db | COMPLETED |
| ASU-073 | Rust chat proxy (`chat_send`) + `ASUNA_CHAT_MODEL` konfigurasyonu | 7 | L | backend | COMPLETED |
| ASU-074 | TS sozlesmesi (`shared/chat.ts`) + `chat-service.ts` | 7 | M | backend | COMPLETED |
| ASU-075 | Chat kabugu UI — sidebar + chat-view + composer + proje dosya secici | 7 | L | frontend | COMPLETED |
| ASU-076 | ACL / capability kayitlari (`asuna-chat.json`, build.rs manifest, lib.rs) | 7 | S | backend | COMPLETED |
| ASU-077 | Test boslugu kapatma — CASCADE, ek sahipligi, redaksiyon desenleri, composer | 7 | M | test | COMPLETED |
| ASU-078 | Gate 3 review + duzeltmeler (0 CRITICAL / 1 HIGH / 4 MEDIUM / 7 LOW) | 7 | M | review | COMPLETED |
| ASU-079 | **M6 kabul testi** — restart sonrasi konusma gecmisi + dosya ekleme + projede konusma | 7 | M | test | PENDING |

---

> **ASU-079** — kabul kriterleri: (1) yeni konusma ac → mesaj yaz → yanit gelir → uygulama restart →
> konusma listede, mesajlar yerinde; (2) konusmayi sil → mesajlar ve ekler DB'den gercekten gider;
> (3) `.env` icerikli bir dosya eklenince secret'lar redakte edilmis saklanir, `.env` **adi** reddedilir;
> (4) proje disi mutlak yol ile proje dosyasi ekleme reddedilir; (5) bir projede konusma baslatilir ve
> Asuna proje baglamini gorur; (6) mikrofon butonu voice mode'u acar, ses yolu bozulmamistir.
> **Durum notu: proje askida — kullanici donunce** (2026-08-31; kullanici projeyi askiya aldigini
> bildirdi, bu test devam karari verilirse ilk adimdir).

> **ASU-066** — uygulama Cmd+Q ile kapatildiginda `session_finalize` (oturum ozeti + hafiza
> cikarimi) tamamlanmadan process olebilir. Session 1 kapanis notunda **baslik olarak** kaydedildi;
> kapsam ve cozum icin kod incelemesi gerekiyor — detay henuz yok.

<!-- Yeni phase eklerken asuna-tasks/templates/task-template.md formatini kullan.
     Planlanmamis fikirler -> asuna-tasks/backlog.md
     Task detaylari -> asuna-tasks/phases/phase-X.md -->

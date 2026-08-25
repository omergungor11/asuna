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
| 3 | Memory | 11 | 10 | 0 | 0 | 1 | 0 |
| 4 | Project Context | 8 | 0 | 0 | 0 | 8 | 0 |
| 5 | One Useful Action (Tools) | 9 | 0 | 0 | 0 | 9 | 0 |
| 6 | Focus Recovery (MVP) | 8 | 0 | 0 | 0 | 8 | 0 |
| **Total** | | **66** | **33** | **0** | **0** | **33** | **0** |

**Progress**: 33/66 (50%)

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
| M4 | **Projeleri taniyor + ilk tool** — hangi projede oldugunu soyleyebiliyor, onayli tool calistiriyor, audit'e yaziyor | 4-5 | ASU-046 + ASU-055 | PENDING |
| M5 | **"Beni toparla" calisiyor (MVP)** — gercek proje state'inden tek somut sonraki adim uretiliyor | 6 | ASU-062 (PROJECT.md Bolum 33 checklist) | PENDING |

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
| ASU-039 | `projects` tablosu + migration | 4 | S | db | PENDING |
| ASU-040 | `ProjectRegistry` (kayitli proje root'lari) | 4 | M | backend | PENDING |
| ASU-041 | `ProjectContextService` (metadata + context dosyalari) | 4 | L | backend | PENDING |
| ASU-042 | Git metadata provider | 4 | M | backend | PENDING |
| ASU-043 | `.asuna/context.json` okuma/yazma | 4 | M | backend | PENDING |
| ASU-044 | `get_current_project` tool (risk 0) | 4 | M | backend | PENDING |
| ASU-045 | Projects UI sekmesi | 4 | M | frontend | PENDING |
| ASU-046 | **Phase 4 kabul testi** | 4 | S | test | PENDING |
| ASU-047 | `AsunaToolDefinition` + tool registry | 5 | L | backend | PENDING |
| ASU-048 | Risk / approval policy katmani | 5 | M | backend | PENDING |
| ASU-049 | Path sandbox + hassas dosya blocklist | 5 | M | backend | PENDING |
| ASU-050 | `tool_events` tablosu + audit logger | 5 | M | db | PENDING |
| ASU-051 | `read_project_file` tool (risk 0, sandbox'li) | 5 | M | backend | PENDING |
| ASU-052 | `open_project` tool (risk 1) | 5 | M | backend | PENDING |
| ASU-053 | Approval UI (AWAITING_APPROVAL) | 5 | M | frontend | PENDING |
| ASU-054 | Tool call gorunurlugu (TOOL_PENDING) | 5 | M | frontend | PENDING |
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

---

<!-- Yeni phase eklerken asuna-tasks/templates/task-template.md formatini kullan.
     Planlanmamis fikirler -> asuna-tasks/backlog.md
     Task detaylari -> asuna-tasks/phases/phase-X.md -->

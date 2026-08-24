# Asuna - Task Index

> Kaynak gercek: `PROJECT.md` (urun/mimari), `TRANSCRIPT.md` (urun niyeti), `CLAUDE.md` (agent kurallari).
> Task ID formati: `ASU-XXX`. Commit formati: `feat(ASU-XXX): aciklama`.
> **Dikey dilim kurali:** bir phase'in kabul testi gecmeden sonraki phase'e gecilmez.

## Dashboard

| Phase | Ad | Total | Done | In Progress | Review | Pending | Blocked |
|-------|----|-------|------|-------------|--------|---------|---------|
| 0 | Arastirma + Scaffold | 11 | 7 | 0 | 0 | 4 | 0 |
| 1 | Realtime Voice (dikey dilim) | 10 | 0 | 0 | 0 | 10 | 0 |
| 2 | Wake Word | 8 | 0 | 0 | 0 | 8 | 0 |
| 3 | Memory | 10 | 0 | 0 | 0 | 10 | 0 |
| 4 | Project Context | 8 | 0 | 0 | 0 | 8 | 0 |
| 5 | One Useful Action (Tools) | 9 | 0 | 0 | 0 | 9 | 0 |
| 6 | Focus Recovery (MVP) | 8 | 0 | 0 | 0 | 8 | 0 |
| **Total** | | **64** | **7** | **0** | **0** | **57** | **0** |

**Progress**: 7/64 (11%)

> ASU-008 "RESEARCH DONE" olarak Done sayilir: arastirma ve ADR-004 tamam, calisan detection spike'i
> ayri task (**ASU-008b**, PENDING).

> Not: PROJECT.md Bolum 32'deki "Phase 0 — Template audit" tamamlanmis sayilir. Repo'da uygulama kodu
> yok, sadece Claude Code workflow meta-template'i var (`asuna-tasks/`, `asuna-docs/`, `asuna-config/`,
> `asuna-plans/`, `.claude/`). Bu yuzden Phase 0 yeniden yorumlandi: **teknik arastirma + greenfield scaffold**.

## Milestones

| Milestone | Hedef | Phase'ler | Kanit (nasil dogrulanir) | Durum |
|-----------|-------|-----------|--------------------------|-------|
| M1 | **Sesli konusma calisiyor** — butona bas, Turkce konus, Asuna dusuk gecikmeyle cevap versin, sozunu kesebil, transcript'i gor, temiz kapat | 0-1 | ASU-020 (PROJECT.md Bolum 35, 8 madde) | PENDING |
| M2 | **"Hey Asuna" ile uyaniyor** — idle'da bulut'a ses gitmiyor, wake word ile oturum aciliyor, timeout ile kapaniyor | 2 | ASU-028 | PENDING |
| M3 | **Hatiriyor** — oturum kapanir, uygulama yeniden baslar, onceki oturumun karari hatirlanir | 3 | ASU-038 | PENDING |
| M4 | **Projeleri taniyor + ilk tool** — hangi projede oldugunu soyleyebiliyor, onayli tool calistiriyor, audit'e yaziyor | 4-5 | ASU-046 + ASU-055 | PENDING |
| M5 | **"Beni toparla" calisiyor (MVP)** — gercek proje state'inden tek somut sonraki adim uretiliyor | 6 | ASU-062 (PROJECT.md Bolum 33 checklist) | PENDING |

## Risks

| ID | Risk | Olasilik | Etki | Azaltma | Durum |
|----|------|----------|------|---------|-------|
| R1 | **Realtime API maliyeti** — surekli acik ses oturumu faturayi hizla buyutur; ChatGPT aboneligi API kredisi vermez (PROJECT.md Bolum 28) | H | H | Idle'da oturum yok (Phase 2), inactivity timeout + max session duration (ASU-025), dev'de `gpt-realtime-2.1-mini`, oturum suresi/maliyet metadata takibi | OPEN |
| R2 | **sherpa-onnx KWS tespit kalitesi + model lisansi** — "Hey Asuna" detection rate / false-accept orani ve KWS model agirliklarinin dagitim lisansi henuz dogrulanmadi. (Porcupine lisans riski **KAPANDI**: saglayici elendi — Free Tier 2026-06-30'da kapatildi, Rust binding yanked, AccessKey online dogrulaniyor; ADR-004) | M | M | ASU-008b spike'i (>%95 detection, <1 FP/8 saat, model lisansi netlestirme); `WakeWordProvider` adapter arkasinda tutulur (ASU-021), fake provider ile Phase 2 gelistirmesi vendor'dan bagimsiz ilerler. Exit: Silero VAD kapisi → tetikleyici ifadeyi uzatma → `oww-rs` / `rustpotter` yedekleri | OPEN |
| R3 | **Tauri webview'da WebRTC / mikrofon izni** — WKWebView'da `getUserMedia` ve macOS mikrofon entitlement'i calismayabilir; bu Phase 1'i tumden bloklar | M | H | ASU-007 spike'i Phase 0'da, feature kodundan once; calismazsa fallback: Rust tarafinda WebSocket transport veya ayri yerel audio process (ADR ile karar) | OPEN |
| R4 | **SQLite erisim mimarisi belirsiz (ACIK SORU)** — `tauri-plugin-sql` mi, Rust tarafinda servis mi; secim memory/tool/audit katmanlarinin tamamini etkiler | H | M | ASU-005 arastirma + ADR-005; karar verilene kadar Phase 3'e baslanmaz; MemoryService interface arkasinda kalir | OPEN |
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
| ASU-005 | [ARASTIRMA] SQLite erisim mimarisi karari + ADR-005 | 0 | M | research | PENDING |
| ASU-006 | [ARASTIRMA] OpenAI Agents SDK realtime dogrulamasi + surum pinleme | 0 | M | research | COMPLETED |
| ASU-007 | [ARASTIRMA] Tauri webview mikrofon + WebRTC spike | 0 | M | research | PENDING |
| ASU-008 | [ARASTIRMA] Wake word saglayicisi (sherpa-onnx KWS) + lisans | 0 | M | research | RESEARCH DONE |
| ASU-008b | [SPIKE] sherpa-onnx KWS detection spike (macOS arm64) | 0 | L | research | PENDING |
| ASU-009 | Konfigurasyon katmani + `.env.example` | 0 | S | backend | COMPLETED |
| ASU-010 | `docs/architecture` + ADR dizini + README local run | 0 | S | docs | PENDING |
| ASU-011 | Ephemeral Realtime token minting (Rust) | 1 | L | backend | PENDING |
| ASU-012 | Asuna core prompt / instructions dosyasi | 1 | S | backend | PENDING |
| ASU-013 | `AsunaRealtimeService` (SDK wrapper) | 1 | L | backend | PENDING |
| ASU-014 | Voice state machine | 1 | M | frontend | PENDING |
| ASU-015 | "Talk to Asuna" gecici butonu + baglanti akisi | 1 | M | frontend | PENDING |
| ASU-016 | Iki yonlu ses + interruption (barge-in) | 1 | M | frontend | PENDING |
| ASU-017 | Canli transcript UI | 1 | M | frontend | PENDING |
| ASU-018 | Temiz disconnect + kaynak temizligi | 1 | M | frontend | PENDING |
| ASU-019 | Hata yonetimi + observability (state transition log) | 1 | M | backend | PENDING |
| ASU-020 | **M1 kabul testi** — PROJECT.md Bolum 35, 8 madde | 1 | M | test | PENDING |
| ASU-021 | `WakeWordProvider` interface + fake provider | 2 | M | backend | PENDING |
| ASU-022 | `SherpaKwsProvider` (sherpa-onnx KWS, "Hey Asuna") | 2 | L | backend | PENDING |
| ASU-023 | IDLE_WAKE_WORD -> WAKING -> CONNECTING gecisi | 2 | M | frontend | PENDING |
| ASU-024 | Idle'da buluta ses gitmedigi dogrulamasi | 2 | M | test | PENDING |
| ASU-025 | Inactivity timeout + max session duration | 2 | M | backend | PENDING |
| ASU-026 | Session close akisi (sesli kapanis + stop) | 2 | M | backend | PENDING |
| ASU-027 | Minimal idle overlay / tray gostergesi | 2 | M | frontend | PENDING |
| ASU-028 | **M2 kabul testi** | 2 | S | test | PENDING |
| ASU-029 | SQLite bootstrap + migration altyapisi | 3 | L | db | PENDING |
| ASU-030 | `memories` + `sessions` schema | 3 | M | db | PENDING |
| ASU-031 | `MemoryService` CRUD | 3 | M | backend | PENDING |
| ASU-032 | Session kaydi + opsiyonel transcript persist | 3 | M | backend | PENDING |
| ASU-033 | Session summary pipeline | 3 | M | backend | PENDING |
| ASU-034 | Memory extraction pipeline (PROJECT.md Bolum 26) | 3 | L | backend | PENDING |
| ASU-035 | Stage A deterministik retrieval + `SessionBootstrapContext` | 3 | L | backend | PENDING |
| ASU-036 | Memory UI (listele / ara / sil / arsivle) | 3 | M | frontend | PENDING |
| ASU-037 | Memory gizlilik kontrolleri (toggle'lar) | 3 | S | frontend | PENDING |
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

---

<!-- Yeni phase eklerken asuna-tasks/templates/task-template.md formatini kullan.
     Planlanmamis fikirler -> asuna-tasks/backlog.md
     Task detaylari -> asuna-tasks/phases/phase-X.md -->

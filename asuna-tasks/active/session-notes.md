# Session Notes

<!-- Sablon:

## [TARIH] — Session X

### Yapilanlar
- [x] ASU-XXX: [aciklama]

### Yarim Kalanlar
- [ ] ASU-YYY: [ne kaldi, nerede birakildi]

### Bir Sonraki Session
- [ ] [Yapilacak 1]

### Dikkat Edilecekler
- [Bug, workaround, karar bekleyen konu]
-->

## 2026-08-24 — Session 1 (Phase 0 + Phase 1, M1)

### Yapilanlar

**Phase 0 — arastirma + scaffold (11/11)**
- [x] ASU-001..004: pnpm workspace, Tauri 2 + React + TS (strict) + Vite scaffold, ESLint/Prettier/Vitest, CI yesil (macos-latest)
- [x] ASU-005: SQLite erisim karari → **B, rusqlite + Rust servis** (ADR-005 accepted); `tauri-plugin-sql` olcumle elendi
- [x] ASU-006: `@openai/agents-realtime` 0.17.0 + zod 4.4.3 exact pin
- [x] ASU-007: WKWebView WebRTC dogrulandi — R3 kapandi
- [x] ASU-008 + ASU-008b: wake word motoru **sherpa-onnx KWS** (ADR-004, kapsami daraltilmis accepted)
- [x] ASU-009: tipli config katmani — key sadece Rust tarafinda
- [x] ASU-010: `docs/architecture/*`, ADR dizini, README local run, RUNBOOK

**Phase 1 — realtime voice dikey dilimi (10/11)**
- [x] ASU-011: ephemeral realtime token minting (Rust)
- [x] ASU-012: `core.v1` prompt baseline
- [x] ASU-013: `AsunaRealtimeService` — SDK wrapper
- [x] ASU-014: voice state machine (gecersiz gecis politikasi: dev throw / prod reject)
- [x] ASU-015..018: "Talk to Asuna" butonu, iki yonlu ses + barge-in, canli transcript UI, temiz disconnect
- [x] ASU-019: observability — logger, state transition log, durust hata mesajlari, debug paneli
- [x] **ASU-020: M1 kabul testi CANLI TESTTE GECTI** — Turkce anlasildi, barge-in sorunsuz, temiz kapanis

### Kritik bulgular (bu oturumda ogrenildi)
- **Porcupine oldu** — Picovoice Free Tier 2026-06-30'da kapandi, `pv_porcupine` crate yanked. Wake word Rust tarafinda sherpa-onnx `KeywordSpotter`'a tasindi; `WakeWordProvider` arayuzu degismedi.
- **KWS model sorunu (R2)** — `gigaspeech-3.3M` sozlugunde `ASUNA` yok: ortografik tespit %0, en iyi fonetik workaround %81 @ 54.8 FA/saat. Motor/lisans/CPU/bundle YESIL. Model + ifade secimi ACIK.
- **`tauri-plugin-sql` elendi** — ACL komut duzeyinde (scope yok), path sandbox yok, transaction yok; renderer'dan `DROP TABLE` calisti.
- **`freezePrototype: true` → beyaz ekran** — zod v3 compat katmanindaki atama WebKit "override mistake" kuralina takiliyor. `false` yapildi, gerekce DECISIONS.md'de.
- **CSP prod tuzagi** — `connect-src`'a `https://api.openai.com` eklenmezse ses dev'de calisip paketlenmis build'de sessizce oluyor.
- **OpenAI kredi tuzagi** — ChatGPT aboneligi API kredisi vermiyor; `insufficient_quota` alindi, kullanici $5 kredi yukledi.
- **Vite dep re-optimize yarisi** — bagimlilik degisiminden sonra webview beyaz kalabiliyor; Cmd+R cozuyor.

### Devam eden isler
- [ ] **ASU-064**: realtime gecikme ayari — turn detection konfigurasyonu + olcum. M1'de fark edilir gecikme gozlendi. Suphe listesinde Cloudflare WARP acik olmasi da var (WebRTC yolu).
- [ ] **ASU-021**: `WakeWordProvider` interface + fake provider. Phase 2'nin model kararindan bagimsiz, paralel ilerliyor.

### Bir sonraki session
- [ ] **KWS gercek mikrofon testi (~30 dk) — KULLANICI GEREKLI.** Harness `spike/asu-008b-kws` branch'inde; TTS telaffuz artefakti olup olmadigi olculecek. Sonuc ADR-004'teki model + ifade secimini kapatir; Phase 2 buna bagli.
- [ ] **API key rotasyonu onerisi** — anahtar gelistirme boyunca `.env`'de dolasti; M1 sonrasi rotate edilmesi oneriliyor (RUNBOOK → Incident).
- [ ] **Phase 3 (memory) baslangici** — ASU-029'dan; plan hazir: `asuna-plans/plan-phase-3-memory.md`.

### Dikkat edilecekler
- Phase 2 wake word model karari verilmeden ASU-022 baslamaz (R2 acik).
- Yeni `#[tauri::command]` = 4 adim (build.rs manifest + `permissions/` + capability + `tauri.conf.json`); atlanirsa sessiz red.
- `src-tauri/permissions/` dizini olusturuldugu an TUM uygulama komutlari ACL'e tabi olur — Phase 3'te gecis adimi olarak planlandi.

## 2026-08-25 — Session 1 devami (otonom)

- Bilgi grafi kuruldu (graphify, 21 doc stabil cekirdek; 145 node / 11 topluluk;
  `graphify query` agent promptlarina girdi).
- Phase 3 tamamen yazildi: ASU-029..037 (5 dalga, 8 opus agent) + Gate 3 review
  (opus reviewer). Review: 1 CRITICAL (runtime hafiza anahtari oturum+ozet yolunu
  durdurmuyordu), 1 HIGH (saklanan metinde secret redaksiyonu yoktu) + 7 orta/dusuk —
  hepsi duzeltildi. 296 Rust + 529 TS testi.
- Yeni backlog: ASU-065 (oturum ozeti + transcript temizligi), sunucu tarafli sayfalama.
- Kullaniciyi bekleyen: ASU-038 M3 sesli kabul testi, gecikme A/B (eagerness=high + WARP),
  KWS gercek mikrofon testi (Phase 2 kilidi).

## 2026-08-25 — Session 1 kapanis kaydi (sonraki session devri)

**Durum:** Phase 0+1+3 tamam (M1 gecti; M3 sesli kabul ASU-038 bekliyor). Phase 4 kod tamam
(ASU-046 sesli kabul bekliyor). Phase 5: Wave A+B TAMAM — ASU-047 (registry), ASU-050 (audit),
ASU-048 (approval matrisi), ASU-049 (path sandbox, 31 kotu yol vakasi). 540 Rust + 789 TS
testi yesil, hepsi push'lu. Uygulama KAPALI (agent rebuild'leri pencereyi acip kapatiyordu).

**Sonraki session ilk adimlar:**
1. **Wave C**: ASU-051 (read_project_file — migration 005 `outcome` kolonu dahil, karar
   DECISIONS'ta; sandbox sozlesmesi ASU-049 raporunda: `resolve_in_project` + `read_text` +
   `audit_outcome`) + ASU-052 (open_project) backend; ASU-053 (approval UI — API sozlesmesi
   ASU-048 raporunda: `pendingApproval`/`approveTool`/`rejectTool`) + ASU-054 (tools sekmesi)
   frontend. Sonra ASU-055 otomatize guvenlik testleri + IKINCI Gate 3 review (guvenlik odakli).
2. **Kullanici manuel test kuyrugu** (tek seansta): M3 Bolum 3 tekrari (hafiza + oturum sil →
   "hatirlamiyorum"), ASU-038 kapanisi, ASU-046 (proje sesli testi), ASU-055 sesli maddeleri.
   Test icin `pnpm tauri dev`.
3. **Phase 2 kilidi kullanicida**: KWS gercek mikrofon testi (~30 dk, `spike/asu-008b-kws`).
4. Milestone'da graphify tazele (korpus kopyalari eskidi — scratchpad silinmis olabilir,
   yeniden kopyala).

**Islenecek acik konular:** ASU-066 (Cmd+Q finalize yarisi — task-index'e HENUZ islenmedi),
gecikme A/B (eagerness=high aktif, WARP kapali denenmedi), API key rotasyonu onerisi,
`docs/architecture/tools.md`'ye onay akisi bolumu (ASU-048 ekleyemedi — dosya kilitliydi).

## 2026-08-31 — Session 2 (Phase 5 Wave C)

### Yapilanlar

**Phase 5 Wave C — ASU-051..054 DONE** (opus agent dalgasi + Gate 3 review + duzeltmeler)

- [x] **ASU-051 `read_project_file`** (risk 0): Rust `projects/files.rs`; ASU-049 sandbox'i +
      blocklist, **once redaksiyon sonra 6000 karakter kirpma**, modele yalnizca
      `SandboxedPath::relative()` doner, `truncated`/`redacted` bayraklari ciktida yazili.
      "Kacis denendi" / "dosya turu kapali" / "bulunamadi" tipli olarak ayri sunulur.
- [x] **ASU-052 `open_project`** (risk 1): Rust `projects/editor.rs`; yeni **zorunlu**
      `ASUNA_EDITOR_COMMAND` (bos = `code`, bosluk/metakarakter **acilista** reddedilir),
      `Command::new(cmd).arg(path)` — shell yok, `env_remove(OPENAI_API_KEY)`, hedef yalnizca
      `active` current proje, `last_opened_at` **spawn'dan sonra** tazelenir.
- [x] **Migration 005**: `tool_events.outcome` (`succeeded`/`failed`/`not_run`, NULL'lu, geriye
      donuk doldurma YOK), sema surumu 5. `ToolResult.auditSummary` alani: modele giden metin ile
      deftere gideni tip duzeyinde ayirdi.
- [x] **ASU-053 onay karti**: karar `requestId` ile, geri sayim UI'da ama **zaman asimini servis
      tetikler**, kart `document.body`'ye portal — her sekmede gorunur.
- [x] **ASU-054 Araclar sekmesi**: tool listesi + oturum-yerel toggle, salt-okunur audit gecmisi
      (oturum filtresi sunucuda), transcript'te `role: 'tool'` satiri, `TOOL_PENDING` gorunurlugu.
- [x] Dokumantasyon bugune getirildi: `task-index` (dashboard yeniden hesaplandi, ASU-066 acildi),
      `CHANGELOG` Phase 5 blogu, `docs/architecture/tools.md` (Bolum 3 "Onay akisi" eklendi,
      audit/gorunurluk/TODO gercekle eslendi), `asuna-config/testing.md` ASU-055 manuel senaryosu.

### Gate 3 review (opus reviewer)

1 CRITICAL + 2 HIGH + 3 MEDIUM + 3 LOW. Kapatilanlar: **C1/H1** (toggle dikisi — `toSdkTool`
`executeTool`'a `isToolEnabled`/`onToolResult` gecirmiyordu; kapali tool acik oturumda calisiyor
ve basarili cagrilar transcript'e hic dusmuyordu), **M1** (kazara onay yolu: odak "Reddet"te,
onaylayan kisayol yok), **M2** (tanim listesi + toggle seti tek kaynak, App'ten prop),
**M3**, **L1**. Temiz bulunanlar: sandbox butunlugu, enjeksiyon yuzeyi, ACL 4 adim (+8 regresyon
testi), audit outcome etiketleri.

**Ders:** uclari ayri ayri test etmek yetmiyor — **dikisi** test etmek gerek. Zincirin iki ucu
testliydi, aradaki tek satir eksikti ve derleyici yakalayamadi (tum binding alanlari opsiyonel).
7 yeni dikis testi tool'u uretimdeki yoldan (ham JSON argumanla) cagiriyor.

**Durum:** typecheck / lint / clippy / fmt temiz, **907 TS + 592 Rust** testi yesil. Commit YOK.

### Kullaniciyi bekleyenler (tek seansta, `pnpm tauri dev`)

- [ ] **Gate 3 H2 — ACIK, tek satir**: repo kokundeki `.env`'e `ASUNA_EDITOR_COMMAND=code` ekle.
      Yoksa uygulama acilista `ConfigError::Missing` ile durur. `code` bulunamazsa
      `which code` ciktisindaki tam yolu yaz (macOS GUI process'inde PATH dar olabilir).
- [ ] **ASU-055** sesli maddeler — senaryo `asuna-config/testing.md` → "M4 kabul senaryosu"
      (A1..A11: proje sorusu, README okuma, uydurmama, onay/red/timeout, `.ssh` + `.env` reddi,
      bozuk editor komutu, oturum ortasinda tool kapatma, audit salt-okunurlugu).
- [ ] **ASU-038** (M3 restart sonrasi hatirlama) + **ASU-046** (Phase 4 proje sesli testi) —
      ayni seansta kapatilabilir.
- [ ] **KWS gercek mikrofon testi (~30 dk)** — `spike/asu-008b-kws` branch'i. **Phase 2 kilidi**;
      ADR-004'teki model + ifade secimi buna bagli.

### Acik kalanlar

- **ASU-066** artik `task-index.md`'de (Phase 3, backend, M, PENDING): Cmd+Q ile cikista
  `session_finalize` (ozet + cikarim) tamamlanmadan process olebilir. Detay kod incelemesi ister.
- **Gecikme A/B**: `ASUNA_VAD_EAGERNESS=high` aktif; **Cloudflare WARP kapali** deneme henuz
  yapilmadi (WebRTC yolu suphelisi).
- **API key rotasyonu** onerisi duruyor — anahtar gelistirme boyunca `.env`'de dolasti
  (RUNBOOK → Incident).
- **Overlay penceresi yok** → onay istegi ana pencere kapaliyken gorunmuyor (backlog).

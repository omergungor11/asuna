# Phase 0: Arastirma + Scaffold

> **Hedef:** Tauri 2 iskeleti ayakta, bos pencere aciliyor, CI yesil ve Phase 1'i bloklayabilecek
> tum teknik bilinmezler ADR'ye baglanmis olsun.
>
> **Not:** PROJECT.md Bolum 32'deki "Phase 0 — Template audit" tamamlanmis sayilir. Repo'da uygulama
> kodu yok; sadece Claude Code workflow meta-template'i var. Bu yuzden Phase 0 = **teknik arastirma +
> greenfield scaffold**.
>
> **Phase cikisi:** `pnpm tauri dev` bos Asuna penceresini aciyor, `pnpm lint && pnpm typecheck &&
> pnpm build` ve CI yesil, ASU-005..008 arastirmalarinin dordu de bir ADR ile kapatilmis.
> ASU-008b (KWS detection spike) ADR-004'u accepted'a ceker; **Phase 1'i bloklamaz** ama
> Phase 2 (wake word) baslamadan once bitmis olmali.

---

## ASU-001: Repo Iskeleti + pnpm Workspace

**Scope**: devops | **Boyut**: S | **Durum**: COMPLETED (2026-08-24, commit 1da15fa) | **Bagimlilik**: -

### Aciklama
Greenfield repo iskeleti. PROJECT.md Bolum 22'deki onerilen yapiyi baz al, ama Phase 0'da sadece
gercekten kullanilacak dizinleri ac — bos klasor agaci acma.

### Acceptance Criteria
- [ ] `pnpm-workspace.yaml` + root `package.json` (scripts: `dev`, `build`, `lint`, `typecheck`, `test`)
- [ ] pnpm surumu `packageManager` alaninda pinlenmis
- [ ] `.gitignore`: `node_modules`, `dist`, `target/`, `.env`, `.DS_Store`, `coverage`, `.asuna/`
- [ ] `git init` + ilk commit: `chore(ASU-001): repo scaffold`
- [ ] Meta dizinler (`asuna-tasks/`, `asuna-docs/`, `asuna-config/`, `asuna-plans/`, `.claude/`) korunmus, tasinmamis

### Notlar
`.env` asla commit edilmez. `.asuna/` calisma zamani cikti dizini (context.json, notes) — kaynak degil.

---

## ASU-002: Tauri 2 + React + TS + Vite Scaffold

**Scope**: devops | **Boyut**: L | **Durum**: COMPLETED (2026-08-24, commit 257eb22) | **Bagimlilik**: ASU-001

### Aciklama
Tauri 2 desktop kabugunu React + TypeScript + Vite frontend ile ayaga kaldir. Bu task'ta **hicbir
Asuna ozelligi yok** — sadece bos pencere acilmasi.

### Acceptance Criteria
- [ ] `src-tauri/` (Rust) + `src/` (React) yapisi kurulu
- [ ] `pnpm tauri dev` macOS'te "Asuna" basligiyla bos pencere aciyor
- [ ] `pnpm tauri build` lokal olarak .app uretiyor (imzasiz kabul)
- [ ] Pencere basligi/uygulama adi/bundle identifier "Asuna" olarak ayarlanmis
- [ ] Tauri capability/permission dosyalari acikca tanimlanmis (varsayilan genis izinler kirpilmis)
- [ ] Rust toolchain surumu ve minimum macOS hedefi dokumante edilmis

### Notlar
Tauri 2'nin capability modeli Electron'a gore kisitli — bu bilincli bir tercih (PROJECT.md Bolum 7).
Ihtiyac dogdukca izin ac, bastan hepsini acma.

---

## ASU-003: TypeScript Strict + ESLint + Prettier

**Scope**: devops | **Boyut**: S | **Durum**: COMPLETED (2026-08-24, commit ccf176f) | **Bagimlilik**: ASU-002

### Acceptance Criteria
- [ ] `tsconfig.json` strict mode acik (`strict`, `noUncheckedIndexedAccess`, `noImplicitOverride`)
- [ ] ESLint + Prettier konfigure, birbirleriyle catismiyor
- [ ] `pnpm lint` ve `pnpm typecheck` sifir hatayla geciyor
- [ ] Rust tarafi: `cargo fmt --check` + `cargo clippy -- -D warnings` geciyor
- [ ] Vitest kurulu, ornek bir test `pnpm test` ile yesil
- [ ] `asuna-config/conventions.md` bu ayarlara gore guncellenmis

---

## ASU-004: CI Pipeline Yesil

**Scope**: devops | **Boyut**: M | **Durum**: COMPLETED (2026-08-24, commit e99240a; run 32719706364 yesil, 8dk13sn) | **Bagimlilik**: ASU-003

### Aciklama
`.github/workflows/ci.yml` iskeletini doldur. macOS runner gerekiyor (Tauri + Apple Silicon hedefi).

### Acceptance Criteria
- [x] PR'da calisan adimlar: install -> lint -> typecheck -> test -> build
- [x] Rust adimlari: `cargo fmt --check`, `cargo clippy`, `cargo test`
- [x] `macos-latest` runner uzerinde tam build en az bir kez yesil
      — dogrulandi: run 32719706364 (2026-08-24, 8dk13sn, quality + bundle yesil).
      Lokal esdeger de yesil: `Asuna.app` + `Asuna_0.1.0_aarch64.dmg` (macOS 26.5 / arm64).
- [x] pnpm store + cargo registry cache'i acik (CI suresi kontrol altinda)
- [x] Bagimlilik audit adimi aktif (high+ fail)
- [x] CI'da hicbir gercek secret gerekmiyor (OPENAI_API_KEY olmadan yesil)
- [x] `.github/workflows/ci.yml`'deki placeholder `exit 1` adimi kaldirilmis

### Notlar
Tam macOS build yavas ve pahali; gerekirse build adimi sadece `main` push'unda calissin, PR'da
lint/typecheck/test yeterli. Karari CI dosyasinda yorumla belgele.

**Uygulama (2026-08-24).** Iki job:
- `quality` (`macos-latest`, PR + push): install -> lint -> typecheck -> format:check -> test ->
  rust fmt/clippy/test -> `pnpm build` (renderer) -> `pnpm audit --audit-level high`.
- `bundle` (`macos-latest`, yalnizca `main` push, `needs: quality`): tam `pnpm tauri build` (imzasiz).

Tek job `macos-latest`: `src-tauri` crate'i Linux'ta webkit2gtk/gtk sistem paketleri olmadan
derlenmez ve toolchain `aarch64-apple-darwin` hedefine pinli — Rust gate'leri zaten macOS'ta kalmak
zorunda. JS gate'lerini ubuntu'ya bolmek ikinci runner + ikinci install/cache maliyeti getirirdi.

Cache: pnpm store `actions/setup-node@v4 (cache: pnpm)`; cargo registry/git + `src-tauri/target`
`actions/cache@v4` ile, key = `hashFiles(rust-toolchain.toml, src-tauri/Cargo.lock)`. Debug ve
release target'lari icin ayri key'ler. Ucuncu parti cache action'i (rust-cache) tedarik zinciri
yuzeyini genisletmemek icin bilerek kullanilmadi.

`continue-on-error` hicbir adimda yok — yesil, gercekten yesil.

---

## ASU-005: [ARASTIRMA] SQLite Erisim Mimarisi Karari + ADR-005

**Scope**: research | **Boyut**: M | **Durum**: COMPLETED (2026-08-24) — karar: B (rusqlite), `docs/decisions/ADR-005-sqlite-access.md` | **Bagimlilik**: ASU-002

### Aciklama
**ACIK SORU.** Asuna'nin SQLite'a nasil erisecegi henuz kararlastirilmadi. Iki aday:

- **A)** `tauri-plugin-sql` — frontend'den SQL cagrilari; hizli baslangic, ama SQL renderer'a yakin.
- **B)** Rust tarafinda bir persistence servisi (`rusqlite`/`sqlx`) + tip guvenli Tauri command'lari;
  daha fazla boilerplate, ama SQL renderer'a hic girmez, security/audit sinirlari net.

Bu karar Phase 3 (memory), Phase 4 (projects), Phase 5 (tool_events audit) katmanlarinin tamamini
etkiler. **Phase 3 baslamadan karar verilmis olmali.**

### Acceptance Criteria
- [x] Her iki secenek icin calisan minimal spike (tek tablo yaz/oku) — A 6/6, B 8/8 test, gercek IPC uzerinden
- [x] Karsilastirma kriterleri degerlendirilmis (ADR-005 kriter tablosu)
- [x] `docs/decisions/ADR-005-sqlite-access.md` yazilmis (Durum: accepted)
- [x] DB dosya konumu: `~/Library/Application Support/com.omergungor.asuna/asuna.db` (app_data_dir, WAL)
- [x] Spike kodu ana koda karistirilmadi (izole worktree, discard edildi)

### Notlar
PROJECT.md Bolum 12: "Do not start with a complex vector platform unless required." Embedding/vektor
Phase 3'un konusu degil — Stage B backlog'da. Karar verirken buna gore optimize et.
CLAUDE.md kurali: "Do not make React components call arbitrary shell commands or database queries
directly" — bu, B secenegine dogru bir egilim yaratir; A secilirse renderer ile DB arasina yine bir
servis katmani konmali.

---

## ASU-006: [ARASTIRMA] OpenAI Agents SDK Realtime Dogrulamasi + Surum Pinleme

**Scope**: research | **Boyut**: M | **Durum**: COMPLETED (2026-08-24) — bulgular `docs/architecture/voice.md` | **Bagimlilik**: ASU-002

### Aciklama
`@openai/agents` (TypeScript) `RealtimeAgent` / `RealtimeSession` API'sinin guncel halini dogrula.
PROJECT.md Bolum 24'teki pseudocode kavramsaldir — gercek imza kurulan surumden gelir.

### Acceptance Criteria
- [ ] SDK paketi ve surumu pinlenmis, `asuna-config/tech-stack.md`'ye yazilmis
- [ ] `RealtimeAgent` + `RealtimeSession` gercek imzalari (connect, tool tanimi, event'ler) dokumante
- [ ] WebRTC transport'un desteklendigi ve tarayici/webview ortaminda calistigi dogrulanmis
- [ ] Ephemeral client secret uretme endpoint'i / akisi dokumante (hangi HTTP cagrisi, hangi payload)
- [ ] `gpt-realtime-2.1` ve `gpt-realtime-2.1-mini` model ID'lerinin hesapta erisilebilirligi kontrol edilmis
- [ ] Interruption / barge-in davranisinin SDK tarafindan mi yoksa uygulama tarafindan mi yonetildigi netlesmis
- [ ] Bulgular `docs/architecture/voice.md`'ye yazilmis

### Notlar
Model ID'leri hicbir yerde hard-code edilmez — `ASUNA_REALTIME_MODEL` uzerinden gelir (ASU-009).
Guncel resmi dokumantasyona bak, egitim verisindeki eski isimlere guvenme (TRANSCRIPT.md Bolum 15).

---

## ASU-007: [ARASTIRMA] Tauri Webview Mikrofon + WebRTC Spike

**Scope**: research | **Boyut**: M | **Durum**: COMPLETED (2026-08-24) — WKWebView calisiyor, OQ-5/R3 kapandi; bulgular voice.md Bolum 11 | **Bagimlilik**: ASU-002

### Aciklama
**Phase 1'i tumden bloklayabilecek risk (R3).** macOS'te Tauri'nin WKWebView'inda `getUserMedia` ve
WebRTC peer connection'in calistigini kanitla.

### Acceptance Criteria
- [ ] Tauri penceresinde `navigator.mediaDevices.getUserMedia({ audio: true })` basarili
- [ ] macOS mikrofon izin dialogu cikiyor ve izin kalici olarak veriliyor
- [ ] `Info.plist` icin `NSMicrophoneUsageDescription` metni Turkce/net olarak ayarlanmis
- [ ] Gerekli entitlement'lar (`com.apple.security.device.audio-input`) dokumante
- [ ] Bir `RTCPeerConnection` kurulup ses track'i eklenebiliyor (echo/loopback testi yeterli)
- [ ] Uzak ses cikisinin (`<audio>` element / WebAudio) webview'da duyulabildigi dogrulanmis
- [ ] **Calismazsa:** fallback secenekleri ADR ile kayit altina alinmis (WebSocket transport, Rust
      tarafinda audio pipeline, veya ayri yerel process)

### Notlar
Bu spike Phase 1'in ilk task'indan **once** bitmeli. Sonuc olumsuzsa Phase 1 task'lari yeniden
yazilir — bu yuzden erken ogrenmek deger.

---

## ASU-008: [ARASTIRMA] Wake Word Saglayicisi (sherpa-onnx KWS) + Lisans

**Scope**: research | **Boyut**: M | **Durum**: RESEARCH DONE — calisan spike ASU-008b'de | **Bagimlilik**: ASU-002

### Aciklama
Wake word saglayicisinin secimi, lisans durumu ve Tauri mimarisine yerlesimi. Sonuc: **Porcupine
elendi**, **sherpa-onnx `KeywordSpotter`** secildi — motor Tauri'nin **Rust process'inde** calisir,
mikrofon idle'da `cpal` ile Rust tarafindan acilir, tespit Tauri event'i ile renderer'a bildirilir.
`docs/decisions/ADR-004-wake-word-provider.md` yazildi; durumu `proposed` — `accepted`'a cekilmesi
ASU-008b spike'ina bagli. (R2)

### Acceptance Criteria
- [x] Secenekler lisans / platform / bakim acisindan degerlendirilmis (Porcupine binding'leri,
      openWakeWord, `oww-rs`, `rustpotter`) — ADR-004 "Degerlendirilen Secenekler" tablosu
- [x] Lisans ve maliyet dokumante: `sherpa-onnx` 1.13.5 + `cpal` Apache-2.0, AccessKey/kota/phone-home
      **yok**; KWS model agirliklarinin lisansi **BELIRSIZ** → ASU-008b'ye devredildi
- [x] Motorun nerede calisacagi netlesmis: Tauri **Rust** process'i (`src-tauri`), mikrofon `cpal` ile
      Rust'ta; renderer idle'da mikrofona **hic** dokunmaz (OQ-4 kapandi)
- [x] "Hey Asuna" uretim yolu dokumante: open-vocabulary KWS, `sherpa-onnx-cli text2token` ile BPE
      `keywords.txt` — vendor console yok, platform-spesifik model dosyasi yok, indirme kotasi yok
- [x] Aday model ve boyutu belirlenmis: `sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01` (int8 ~5MB)
- [x] `docs/decisions/ADR-004-wake-word-provider.md` yazilmis (durum: proposed)
- [x] `asuna-config/tech-stack.md` Bolum 4, `asuna-docs/DECISIONS.md` ve `.env.example` guncellenmis
- [ ] Apple Silicon'da calisan detection spike'i + idle CPU/RAM olcumu → **ASU-008b**

### Notlar
**Porcupine neden elendi:** Picovoice Free Tier 2026-06-30'da kapatildi ve resmi cevapta "no
non-commercial tier planned" denildi; ustune `pv_porcupine` crate'inin tum surumleri yanked ve
AccessKey motor init'inde **online** dogrulaniyor — Asuna'nin local-first sozuyle bagdasmiyor.

Karar ne olursa olsun `WakeWordProvider` adapter interface'i kalir (PROJECT.md Bolum 8); somut
implementasyon adi `SherpaKwsProvider` (ASU-022). Asuna'nin geri kalani vendor adini gormez.

---

## ASU-008b: [SPIKE] sherpa-onnx KWS Detection Spike (macOS arm64)

**Scope**: research | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-002, ASU-008

### Aciklama
ADR-004'u `proposed`'dan `accepted`'a cekmek icin gereken calisan kanit. Apple Silicon uzerinde
`cpal` + `KeywordSpotter` ile "HEY ASUNA" tespiti kurulur ve olculur. ADR-004'un "Acik Kalanlar"
bolumundeki 5 madde bu task'in kapsamidir.

### Acceptance Criteria
- [ ] **Tespit kalitesi**: 50 farkli soyleyiste **>%95 detection**, 8 saatlik gercek ortam idle'inda
      **<1 false accept**; boosting score / threshold taramasi kayit altinda ("Asuna" OOV bir ozel
      isim — BPE subword'lerinin tasiyip tasimadigi burada anlasilir)
- [ ] **Idle CPU/RAM** olculmus (Apple Silicon): 16kHz mono, `num_threads=1`, int8 model, 30 dk olcum;
      Silero VAD ile kapili varyant ayrica olculmus
- [ ] **Mikrofon devir teslimi (OQ-6)**: Rust `cpal` → renderer `getUserMedia` gecis suresi; macOS TCC
      bunu tek izin olarak mi iki ayri prompt olarak mi soruyor; turuncu mikrofon gostergesinin idle
      davranisi UX olarak kabul edilebilir mi
- [ ] **Build / bundle / imzalama**: build script'in GitHub'dan arsiv indirmesi CI'da ve offline
      build'de nasil davraniyor; static link ile `.app` boyut delta'si; `codesign` + notarization
      sorunsuz mu; ONNX model dosyalarinin Tauri resource olarak paketlenmesi
- [ ] **Model agirliklarinin lisansi** netlestirilmis (k2-fsa release notu / HuggingFace model card /
      issue) — dagitim ve ticari kullanim acisindan
- [ ] `docs/decisions/ADR-004-wake-word-provider.md` **accepted**'a cekilmis (veya exit plani devreye
      alinip ADR revize edilmis)

### Notlar
Spike kodu ana koda **karismaz** — ayri spike dizininde/branch'te kalir; uretim implementasyonu
ASU-022'de sifirdan yazilir.

Kriterler tutmazsa ADR-004 exit plani sirasiyla: (a) KWS'yi Silero VAD ile kapila, (b) tetikleyici
ifadeyi uzat, (c) `oww-rs` (MIT) veya `rustpotter` (Apache-2.0) adapter arkasinda dene, (d) son care
global kisayol/tray butonunu kalici sekonder aktivasyon olarak birak.

---

## ASU-009: Konfigurasyon Katmani + `.env.example`

**Scope**: backend | **Boyut**: S | **Durum**: COMPLETED (2026-08-24) | **Bagimlilik**: ASU-002

### Aciklama
PROJECT.md Bolum 23'teki konfigurasyonu tek merkezden okuyan, tipli bir config katmani.

### Acceptance Criteria
- [x] `.env.example` tum degiskenlerle: `OPENAI_API_KEY`, `ASUNA_REALTIME_MODEL`,
      `ASUNA_REALTIME_VOICE`, `ASUNA_WAKE_WORD`, `ASUNA_MEMORY_ENABLED`,
      `ASUNA_TRANSCRIPT_STORAGE`, `ASUNA_TOOL_APPROVAL_MODE`, `ASUNA_IDLE_TIMEOUT_SECONDS`,
      `ASUNA_LOG_LEVEL`, `ASUNA_WAKE_WORD_PROVIDER`, `ASUNA_WAKE_WORD_MODEL_DIR`,
      `ASUNA_WAKE_WORD_THRESHOLD`
- [x] `.env.example`'da gercek secret yok, her degisken icin tek satir aciklama var
- [x] Tipli config okuyucu; eksik/gecersiz deger baslangicta net hata veriyor (sessizce default'lamiyor)
- [x] **`OPENAI_API_KEY` yalnizca Rust/guvenilir process tarafindan okunuyor** — renderer bundle'ina
      hicbir kosulda girmiyor
- [x] Frontend'e gecen config alt kumesi acikca ayrilmis (whitelist, blacklist degil)
- [x] Build ciktisinda `OPENAI_API_KEY` string'inin bulunmadigi grep ile dogrulanmis

### Notlar
Bu, guvenlik modelinin temeli (PROJECT.md Bolum 19). Yanlis yapilirsa MVP checklist maddesi
"API key never shipped in renderer bundle" bastan dusuyor.

### Uygulama notlari (2026-08-24)

**Rust (guvenilir taraf), yeni bagimlilik yok:**

- `src-tauri/src/env_file.rs` — bagimliliksiz `.env` okuyucu. **`dotenvy` bilerek eklenmedi:**
  `dotenvy::dotenv()` degerleri `std::env::set_var` ile tum process'e yazar; Asuna ileride tool
  katmaninda alt process calistiracagi icin `OPENAI_API_KEY` her cocuk process'e miras kalirdi.
  Buradaki okuyucu `BTreeMap` dondurur, deger yalnizca `AsunaConfig` icinde yasar.
- `src-tauri/src/config.rs` — 12 degiskenin tamami zorunlu (yalnizca `ASUNA_REALTIME_VOICE` ve
  `ASUNA_WAKE_WORD_MODEL_DIR` **bos** birakilabilir = "belirtilmedi"). Gecersiz deger -> `ConfigError`;
  `run()` net mesajla `exit(1)` (panic yok). Process environment `.env`'i ezer.
- `AsunaConfig` bilerek `Serialize` **turetmez** -> API key'in bir command donusunde yer almasi
  **derleme zamaninda** imkansiz. Key `SecretString` icinde; `Debug` `<redacted>` basar.
- `#[tauri::command] get_frontend_config` yalnizca 8 alanlik whitelist'i doner.

**Yetki (ACL):** `build.rs` artik `AppManifest::commands([...])` tanimliyor — bu, uygulama
komutlarini Tauri'nin varsayilan "app command'lari serbest" davranisindan cikarip
**deny-by-default** yapiyor. `capabilities/asuna-config.json` yalnizca `allow-get-frontend-config`
iznini, yalnizca `main` penceresine veriyor.

**Renderer:** `src/asuna/config/frontend-config.ts` (elle sema dogrulama — zod henuz bagimlilik
degil; beklenmeyen alan **reddedilir**, hata mesajlari deger degil yalnizca alan adi tasir) +
`config.service.ts` (tek okuma noktasi, onbellekli `invoke`).

**Kanit:** `pnpm build` sonrasi `grep -r "OPENAI_API_KEY" dist/` -> eslesme yok (exit 1). Ayrica
`OPENAI_API_KEY=sk-proj-BUILD-LEAK-CANARY-... pnpm build` ile tekrarlandi: ne degisken adi ne
canary deger bundle'a girdi. Bundle'da hard-code model ID'si de yok.

**Test:** `cargo test` 31 test (eksik anahtar x12, gecersiz deger, `Debug` redaksiyonu, hata
mesajinda deger sizmamasi, whitelist alan kumesi, komut<->capability<->build.rs tutarliligi);
Vitest 13 yeni test (sema dogrulama + whitelist reddi + servis onbellegi).

---

## ASU-010: `docs/architecture` + ADR Dizini + README Local Run

**Scope**: docs | **Boyut**: S | **Durum**: COMPLETED (2026-08-24) | **Bagimlilik**: ASU-005, ASU-006, ASU-007, ASU-008

### Acceptance Criteria
- [x] `docs/architecture/` altinda `voice.md`, `memory.md`, `tools.md`, `security.md` iskeletleri
      (Phase 0 bulgulariyla doldurulmus, geri kalani TODO isaretli)
- [x] `docs/decisions/` altinda ADR-005 (SQLite) ve ADR-004 (wake word) mevcut
- [x] `README.md`: kurulum, `pnpm tauri dev`, gerekli harici setup (OpenAI API billing, KWS model
      dosyalarini indir — sherpa-onnx `kws-models`)
- [x] `asuna-config/tech-stack.md` gercek surumlerle doldurulmus
- [x] `asuna-docs/RUNBOOK.md`'deki kalan CUSTOMIZE/template kalintilari temizlenmis
      (deploy/rollback komutlari gercek degerlerle dolduruldu)
- [x] `asuna-docs/DECISIONS.md` Phase 0 kararlarinin ozetini ve ADR linklerini iceriyor

### Notlar
Bu task Phase 0'in kapanis kaydidir. Bitmeden Phase 1'e gecilmez — cunku Phase 1 ASU-006/ASU-007
bulgularina dogrudan bagimlidir.

### Uygulama notlari (2026-08-24)

- `docs/architecture/` uc yeni iskelet: `memory.md` (katman ayrimi, sema rolleri, ADR-005
  erisim mimarisi, Stage A retrieval, 8 TODO), `tools.md` (risk 0-3, ilk tool seti, "ince
  backchannel" deseni, shell politikasi, `tool_events` akisi, 9 TODO), `security.md`
  (guven siniri diyagrami, ephemeral token akisi, `SecretString`/`dotenvy` karari,
  deny-by-default ACL, CSP bulgulari, path sandbox plani, 10 TODO).
  `voice.md` (ASU-006/007 ciktisi) degistirilmedi.
- `asuna-config/security.md` **checklist** olarak kaldi; `docs/architecture/security.md`
  mimariyi anlatiyor ve ona isaret ediyor — icerik cogaltilmadi.
- `README.md`: "Local Kurulum" bolumu (gereksinim tablosu, `.env`, harici setup, komut
  tablosu, CSP-dev uyarisi) + dokumantasyon tablosu genisletildi; Durum -> "Phase 0
  kapaniyor, sirada Phase 1".
- `asuna-docs/RUNBOOK.md` bastan yazildi: Docker/staging/health-endpoint varsayimlari
  cikarildi; lokal masaustu gercegi (dev vs paketlenmis fark, build + secret grep kontrolu,
  CI, `git revert` rollback, WAL yedekleme, sik karsilasilanlar tablosu).
- `asuna-docs/DECISIONS.md` en uste "Phase 0 ozeti" tablosu (ADR-001..007 + ADR'ye
  donusmeyen 4 uygulama karari).
- **Kapsam disi birakildi:** `asuna-config/tech-stack.md` zaten ASU-005..009 sirasinda
  gercek surumlerle dolduruldu (madde dogrulandi, degisiklik gerekmedi).
- ASU-008b hala PENDING — ADR-004 `proposed` kaliyor, Phase 2 oncesi kapanmali.

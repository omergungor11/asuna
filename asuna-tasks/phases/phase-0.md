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

---

## ASU-001: Repo Iskeleti + pnpm Workspace

**Scope**: devops | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: -

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

**Scope**: devops | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-001

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

**Scope**: devops | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: ASU-002

### Acceptance Criteria
- [ ] `tsconfig.json` strict mode acik (`strict`, `noUncheckedIndexedAccess`, `noImplicitOverride`)
- [ ] ESLint + Prettier konfigure, birbirleriyle catismiyor
- [ ] `pnpm lint` ve `pnpm typecheck` sifir hatayla geciyor
- [ ] Rust tarafi: `cargo fmt --check` + `cargo clippy -- -D warnings` geciyor
- [ ] Vitest kurulu, ornek bir test `pnpm test` ile yesil
- [ ] `asuna-config/conventions.md` bu ayarlara gore guncellenmis

---

## ASU-004: CI Pipeline Yesil

**Scope**: devops | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-003

### Aciklama
`.github/workflows/ci.yml` iskeletini doldur. macOS runner gerekiyor (Tauri + Apple Silicon hedefi).

### Acceptance Criteria
- [ ] PR'da calisan adimlar: install -> lint -> typecheck -> test -> build
- [ ] Rust adimlari: `cargo fmt --check`, `cargo clippy`, `cargo test`
- [ ] `macos-latest` runner uzerinde tam build en az bir kez yesil
- [ ] pnpm store + cargo registry cache'i acik (CI suresi kontrol altinda)
- [ ] Bagimlilik audit adimi aktif (high+ fail)
- [ ] CI'da hicbir gercek secret gerekmiyor (OPENAI_API_KEY olmadan yesil)
- [ ] `.github/workflows/ci.yml`'deki placeholder `exit 1` adimi kaldirilmis

### Notlar
Tam macOS build yavas ve pahali; gerekirse build adimi sadece `main` push'unda calissin, PR'da
lint/typecheck/test yeterli. Karari CI dosyasinda yorumla belgele.

---

## ASU-005: [ARASTIRMA] SQLite Erisim Mimarisi Karari + ADR-005

**Scope**: research | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-002

### Aciklama
**ACIK SORU.** Asuna'nin SQLite'a nasil erisecegi henuz kararlastirilmadi. Iki aday:

- **A)** `tauri-plugin-sql` — frontend'den SQL cagrilari; hizli baslangic, ama SQL renderer'a yakin.
- **B)** Rust tarafinda bir persistence servisi (`rusqlite`/`sqlx`) + tip guvenli Tauri command'lari;
  daha fazla boilerplate, ama SQL renderer'a hic girmez, security/audit sinirlari net.

Bu karar Phase 3 (memory), Phase 4 (projects), Phase 5 (tool_events audit) katmanlarinin tamamini
etkiler. **Phase 3 baslamadan karar verilmis olmali.**

### Acceptance Criteria
- [ ] Her iki secenek icin calisan minimal spike (tek tablo yaz/oku)
- [ ] Karsilastirma kriterleri degerlendirilmis: migration destegi, tip guvenligi, transaction,
      renderer'a SQL sizmasi, ileride sifreli DB (SQLCipher) gecisi, test edilebilirlik
- [ ] `docs/decisions/ADR-005-sqlite-access.md` yazilmis: baglam, secenekler, karar, sonuclar
- [ ] Secilen yaklasimda DB dosya konumu kararlastirilmis (macOS app data dizini)
- [ ] Spike kodu ana koda karistirilmamis (branch'te birakilir veya silinir)

### Notlar
PROJECT.md Bolum 12: "Do not start with a complex vector platform unless required." Embedding/vektor
Phase 3'un konusu degil — Stage B backlog'da. Karar verirken buna gore optimize et.
CLAUDE.md kurali: "Do not make React components call arbitrary shell commands or database queries
directly" — bu, B secenegine dogru bir egilim yaratir; A secilirse renderer ile DB arasina yine bir
servis katmani konmali.

---

## ASU-006: [ARASTIRMA] OpenAI Agents SDK Realtime Dogrulamasi + Surum Pinleme

**Scope**: research | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-002

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

**Scope**: research | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-002

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

## ASU-008: [ARASTIRMA] Picovoice Porcupine macOS/Apple Silicon + Lisans

**Scope**: research | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-002

### Aciklama
Wake word saglayicisi olarak Porcupine'in macOS/Apple Silicon uygunlugunu ve "Hey Asuna" custom
keyword uretme yolunu dogrula. (R2)

### Acceptance Criteria
- [ ] Porcupine'in hangi binding ile kullanilacagi netlesmis (Node/web/Rust) ve Tauri mimarisine uydugu dogrulanmis
- [ ] Apple Silicon (arm64) uzerinde calisan minimal detection spike'i
- [ ] "Hey Asuna" custom `.ppn` uretiminin nasil yapildigi ve maliyeti dokumante
- [ ] AccessKey gereksinimi + ucretsiz katman limitleri + lisans kisitlari dokumante
- [ ] Idle CPU/RAM tuketimi olculmus (surekli calisacak, onemli)
- [ ] `docs/decisions/ADR-004-wake-word-provider.md` yazilmis
- [ ] Alternatifler kisaca degerlendirilmis (openWakeWord / Snowboy tureviler) — vendor lock riski icin

### Notlar
Karar ne olursa olsun `WakeWordProvider` adapter interface'i kalir (PROJECT.md Bolum 8). Asuna'nin
geri kalani tek bir wake-word saglayicisina baglanmaz.

---

## ASU-009: Konfigurasyon Katmani + `.env.example`

**Scope**: backend | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: ASU-002

### Aciklama
PROJECT.md Bolum 23'teki konfigurasyonu tek merkezden okuyan, tipli bir config katmani.

### Acceptance Criteria
- [ ] `.env.example` tum degiskenlerle: `OPENAI_API_KEY`, `ASUNA_REALTIME_MODEL`,
      `ASUNA_REALTIME_VOICE`, `ASUNA_WAKE_WORD`, `ASUNA_MEMORY_ENABLED`,
      `ASUNA_TRANSCRIPT_STORAGE`, `ASUNA_TOOL_APPROVAL_MODE`, `ASUNA_IDLE_TIMEOUT_SECONDS`,
      `ASUNA_LOG_LEVEL`, `PICOVOICE_ACCESS_KEY`, `ASUNA_WAKE_WORD_PROVIDER`
- [ ] `.env.example`'da gercek secret yok, her degisken icin tek satir aciklama var
- [ ] Tipli config okuyucu; eksik/gecersiz deger baslangicta net hata veriyor (sessizce default'lamiyor)
- [ ] **`OPENAI_API_KEY` yalnizca Rust/guvenilir process tarafindan okunuyor** — renderer bundle'ina
      hicbir kosulda girmiyor
- [ ] Frontend'e gecen config alt kumesi acikca ayrilmis (whitelist, blacklist degil)
- [ ] Build ciktisinda `OPENAI_API_KEY` string'inin bulunmadigi grep ile dogrulanmis

### Notlar
Bu, guvenlik modelinin temeli (PROJECT.md Bolum 19). Yanlis yapilirsa MVP checklist maddesi
"API key never shipped in renderer bundle" bastan dusuyor.

---

## ASU-010: `docs/architecture` + ADR Dizini + README Local Run

**Scope**: docs | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: ASU-005, ASU-006, ASU-007, ASU-008

### Acceptance Criteria
- [ ] `docs/architecture/` altinda `voice.md`, `memory.md`, `tools.md`, `security.md` iskeletleri
      (Phase 0 bulgulariyla doldurulmus, geri kalani TODO isaretli)
- [ ] `docs/decisions/` altinda ADR-005 (SQLite) ve ADR-004 (wake word) mevcut
- [ ] `README.md`: kurulum, `pnpm tauri dev`, gerekli harici setup (OpenAI API billing, Picovoice AccessKey)
- [ ] `asuna-config/tech-stack.md` gercek surumlerle doldurulmus
- [ ] `asuna-docs/RUNBOOK.md`'deki kalan CUSTOMIZE/template kalintilari temizlenmis
      (deploy/rollback komutlari gercek degerlerle dolduruldu)
- [ ] `asuna-docs/DECISIONS.md` Phase 0 kararlarinin ozetini ve ADR linklerini iceriyor

### Notlar
Bu task Phase 0'in kapanis kaydidir. Bitmeden Phase 1'e gecilmez — cunku Phase 1 ASU-006/ASU-007
bulgularina dogrudan bagimlidir.

# Changelog

All notable changes to this project will be documented in this file.

<!-- Format:
## [TARIH]

### Added
- ASU-XXX: [eklenen ozellik]

### Changed
- ASU-YYY: [degisiklik]

### Fixed
- ASU-ZZZ: [bug fix]
-->

## [Unreleased]

### Added
- **Phase 1 — realtime voice dikey dilimi (ASU-011..ASU-020).** Butona basilir, Turkce konusulur,
  Asuna sesle cevap verir, sozu kesilebilir, transcript gorunur, oturum temiz kapanir:
  - ASU-011: ephemeral Realtime token minting (Rust) — `OPENAI_API_KEY` renderer'a hic girmiyor,
    webview kisa omurlu `ek_` token ile baglaniyor.
  - ASU-012: `core.v1` prompt baseline (`src/asuna/prompts/`), versiyonlu; aktif surum tek noktadan secilir.
  - ASU-013: `AsunaRealtimeService` — `@openai/agents-realtime` wrapper; SDK degisimi tek dosyada izole.
  - ASU-014: voice state machine — gecersiz gecis dev'de `throw`, prod'da `reject`; sessiz yutma yok.
  - ASU-015..ASU-018: "Talk to Asuna" butonu + baglanti akisi, iki yonlu ses gorunurlugu +
    barge-in tepkisi, canli transcript UI, temiz disconnect + kaynak temizligi (mikrofon gostergesi soner).
  - ASU-019: observability — logger (secret redaksiyonu), state transition log, durust hata
    mesajlari, debug paneli.
- Proje Asuna spec'ine gore sekillendirildi: `PROJECT.md` (urun/mimari spec, 40 bolum),
  `TRANSCRIPT.md` (urun niyeti) ve `asuna-docs/AGENT-SPEC-ORIGINAL.md` (coding agent kurallari)
  kaynak gercek olarak repoya alindi.
- Gelistirme plani ve Claude Code agent sistemi kuruldu: Fable orchestrator + `opus` subagent
  modeli, `ASU-XXX` task ID formati, `feat(ASU-XXX): aciklama` commit formati.
- `asuna-docs/DECISIONS.md`: ADR-001..ADR-007 kaydedildi — Tauri 2 desktop shell, OpenAI Agents SDK
  (`RealtimeAgent`/`RealtimeSession`) + WebRTC ses mimarisi, `ASUNA_REALTIME_MODEL` ile model
  konfigurasyonu, `WakeWordProvider` arkasinda wake word motoru, SQLite persistence
  (erisim katmani ACIK — proposed), Tauri Rust tarafinda ephemeral token minting,
  ve Claude Code gelistirme modeli.
- `asuna-docs/MEMORY.md`: proje ozeti, spec dosyalarinin yeri, Phase 0 durumu ve ihlal edilemez
  kurallar (idle ses buluta gitmez, API key renderer'a girmez, model ID config'de) yazildi.
- `.env.example`: PROJECT.md Bolum 23'teki yapilandirma degiskenleri + wake word ayarlari
  (`ASUNA_WAKE_WORD_PROVIDER`, `ASUNA_WAKE_WORD_MODEL_DIR`, `ASUNA_WAKE_WORD_THRESHOLD`),
  her degisken icin aciklama satiri ile eklendi.

- ASU-010: `docs/architecture/` altina `memory.md`, `tools.md` ve `security.md` iskeletleri
  eklendi (Phase 0 bulgulariyla dolu, kalan maddeler TODO tablolarinda). `README.md`'ye
  "Local Kurulum" bolumu geldi: gereksinimler, `.env`, OpenAI API billing notu
  (ChatGPT aboneligi API kredisi vermez), KWS model dosyalarinin indirilmesi, komut tablosu.
- ASU-010: `asuna-docs/DECISIONS.md` en uste "Phase 0 ozeti" tablosu — ADR-001..007 tek
  satirlik ozetleri + `docs/decisions/` altindaki detayli ADR-004/005 linkleri.

### Changed
- ASU-010: `asuna-docs/RUNBOOK.md` template kalintilarindan (Docker/staging/health endpoint)
  temizlenip Asuna gercegine gore yeniden yazildi: `pnpm tauri dev` / `pnpm tauri build`,
  GitHub Actions (`ci.yml`), `git revert` + yeniden build ile geri alma, DB dosyasi konumu ve
  WAL yedekleme (`VACUUM INTO`). "Deploy" kavrami yok; release ASU-063'te.
- ASU-008: **Wake word: Porcupine → sherpa-onnx KWS** (ADR-004 revize; Picovoice Free Tier
  2026-06-30'da kapandi, non-commercial tier yok, `pv_porcupine` crate yanked, AccessKey init'te
  online dogrulaniyor). Motor artik Tauri'nin **Rust** process'inde (`cpal` + `KeywordSpotter`);
  implementasyon adi `SherpaKwsProvider`, `WakeWordProvider` arayuzu degismedi.
  `PICOVOICE_ACCESS_KEY` kaldirildi. Calisan detection spike'i ASU-008b'ye ayrildi.

### Fixed
- ASU-020: **`freezePrototype: true` beyaz ekran.** Paketlenmis/webview calistirmada uygulama hic
  render etmiyordu; zod v3 compat katmanindaki `errorUtil.toString = ...` atamasi donmus
  `Object.prototype` yuzunden WebKit'in "override mistake" kuraline takilip
  `TypeError: Attempted to assign to readonly property` firlatiyordu (Chromium'da gorunmuyor).
  `freezePrototype: false` yapildi; gerekce ve kabul edilen risk `asuna-docs/DECISIONS.md`
  → *Phase 1 uygulama kararlari*.
- ASU-007: prod CSP `connect-src`'a `https://api.openai.com` eklendi — dev'de gorunmeyen,
  paketlenmis build'de sesi sessizce olduren blocker.

### Notes
- **M1 milestone 2026-08-24'te canli testte gecti** (ASU-020): kullanici Turkce konustu, Asuna
  anladi ve cevapladi, barge-in sorunsuz calisti, oturum temiz kapandi. Testte fark edilir bir
  gecikme gozlendi → **ASU-064** (turn detection konfigurasyonu + olcum) acildi.
- Phase 0 yeniden yorumlandi: "template audit" tamamlandi sayiliyor (denetlenecek uygulama kodu yoktu);
  Phase 0 = teknik arastirma + scaffold. Tamamlandi: Tauri 2 iskeleti, CI yesil, ADR-001..007.
- Acik soru "SQLite erisim katmani" **kapandi** (ASU-005): Rust tarafi servis (`rusqlite`),
  `docs/decisions/ADR-005-sqlite-access.md` accepted.
- Acik kalan: wake word model + ifade secimi (ADR-004, R2) — gercek mikrofon testi bekliyor.

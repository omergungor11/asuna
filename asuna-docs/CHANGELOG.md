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

### Notes
- Uygulama kodu henuz YOK. Repo su an sadece Claude Code workflow meta-template'i iceriyor;
  app scaffold greenfield olarak Phase 0'da kurulacak.
- Phase 0 yeniden yorumlandi: "template audit" tamamlandi sayiliyor (denetlenecek uygulama kodu yok);
  Phase 0 = teknik arastirma + scaffold (Tauri 2 iskeleti ayakta, bos pencere aciliyor, CI yesil).
- Acik soru: SQLite erisim katmani (`tauri-plugin-sql` vs Rust tarafi servis) — ADR-005, Phase 0'da netlesecek.

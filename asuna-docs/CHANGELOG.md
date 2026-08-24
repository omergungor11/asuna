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
  konfigurasyonu, `WakeWordProvider` arkasinda Picovoice Porcupine, SQLite persistence
  (erisim katmani ACIK — proposed), Tauri Rust tarafinda ephemeral token minting,
  ve Claude Code gelistirme modeli.
- `asuna-docs/MEMORY.md`: proje ozeti, spec dosyalarinin yeri, Phase 0 durumu ve ihlal edilemez
  kurallar (idle ses buluta gitmez, API key renderer'a girmez, model ID config'de) yazildi.
- `.env.example`: PROJECT.md Bolum 23'teki yapilandirma degiskenleri + Porcupine icin
  `PICOVOICE_ACCESS_KEY`, her degisken icin aciklama satiri ile eklendi.

### Notes
- Uygulama kodu henuz YOK. Repo su an sadece Claude Code workflow meta-template'i iceriyor;
  app scaffold greenfield olarak Phase 0'da kurulacak.
- Phase 0 yeniden yorumlandi: "template audit" tamamlandi sayiliyor (denetlenecek uygulama kodu yok);
  Phase 0 = teknik arastirma + scaffold (Tauri 2 iskeleti ayakta, bos pencere aciliyor, CI yesil).
- Acik soru: SQLite erisim katmani (`tauri-plugin-sql` vs Rust tarafi servis) — ADR-005, Phase 0'da netlesecek.

# Project Memory

## Project Info
- Asuna: macOS uzerinde local-first calisan, "Hey Asuna" wake word'u ile uyanan sesli kisisel AI companion — chatbot DEGIL.

## Kaynak gercek (spec dosyalari)
- `PROJECT.md` — urun/mimari spec, 40 bolum. Mimari karar oncesi buraya bak.
- `TRANSCRIPT.md` — urun niyeti ve gereksinimlerin cikis noktasi.
- `asuna-docs/AGENT-SPEC-ORIGINAL.md` — coding agent kurallari (prime directive, guvenlik, memory, tool kurallari).
- `asuna-docs/DECISIONS.md` — verilmis mimari kararlar (ADR-001..ADR-007).

## Project Status
- **Phase 0**: IN PROGRESS — teknik arastirma + scaffold.
  - Template audit kismi TAMAMLANDI sayiliyor: repoda **uygulama kodu yok**, sadece Claude Code
    workflow meta-template'i var (`asuna-tasks/`, `asuna-docs/`, `asuna-config/`, `asuna-plans/`, `.claude/`).
  - Kalan Phase 0 cikti: Tauri 2 iskeleti ayakta, bos pencere aciliyor, CI yesil.
- Phase 1-6 (PROJECT.md Bolum 32) henuz baslamadi.

## Important Patterns
- **Urun dongusu**: wake → sesli konusma → baglamsal yardim → memory/tool kullanimi → guvenli oturum kapanisi → idle.
  Bu dongunun disina cikan bir tasarim (ornegin buyuk dashboard) once sorgulanir.
- **Modul sinirlari ayri kalir**: audio / agent / memory / projects / tools / permissions / security / database / ui.
  React componentleri dogrudan shell komutu calistirmaz veya DB sorgusu atmaz.
- **Stack**: Tauri 2 + React + TypeScript (strict) + Vite, pnpm, SQLite, OpenAI Agents SDK
  (`RealtimeAgent` / `RealtimeSession`), WebRTC transport, Picovoice Porcupine (adapter arkasinda).
- **Task/commit**: Task ID `ASU-XXX`. Commit `feat(ASU-XXX): aciklama`. Claude attribution satiri YOK.
- **Agent modeli**: ana oturum Fable = orchestrator; subagent'lar `model: opus`.

## Kritik kurallar (ihlal edilemez)
- **Idle mikrofon sesi buluta gitmez.** Idle'da frame'ler sadece lokal wake word motoruna gider;
  OpenAI'ya gonderilmez, diske yazilmaz.
- **API key renderer'a girmez.** `OPENAI_API_KEY` sadece Tauri Rust tarafinda; renderer Realtime'a
  kisa omurlu (ephemeral) token ile baglanir. `VITE_*` altinda secret olmaz.
- **Model ID config'de.** `ASUNA_REALTIME_MODEL` disinda hicbir yerde model ismi hard-code edilmez
  (fallback deger olarak bile).
- Dosya sistemi islemleri sadece kayitli proje koklerine sinirli; path normalize edilir, traversal reddedilir.
- Memory = tum transcript degil. Raw transcript / session ozeti / durable memory ayri tutulur;
  memory incelenebilir ve silinebilir olmali.

## Known Issues / Gotchas
- **ACIK SORU (ADR-005)**: SQLite'a erisim `tauri-plugin-sql` ile mi, Rust tarafi servis + Tauri
  command'lari ile mi? Phase 0 arastirma task'inda netlesecek. Karar verilmeden DB kodu yazilmaz.
- ChatGPT aboneligi ile OpenAI API kredisi ayri faturalanir — Realtime kullanimi ayrica ucretlendirilir.
  Gelistirmede `gpt-realtime-2.1-mini` kullan.
- Porcupine icin `PICOVOICE_ACCESS_KEY` gerekiyor (Phase 2'den once temin edilmeli).

## Working Credentials (Dev)
- Yok. Gercek secret ASLA buraya yazilmaz — `.env.example` sablon, `.env` commit edilmez.

> Kurallar: session basinda oku; gotcha/pattern kesfedilince aninda guncelle;
> yanlis/eski bilgiyi sil; kisa tut. Mimari kararlar buraya DEGIL → DECISIONS.md.

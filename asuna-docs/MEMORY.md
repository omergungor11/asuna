# Project Memory

## Project Info
- Asuna: macOS uzerinde local-first calisan, "Hey Asuna" wake word'u ile uyanan sesli kisisel AI companion — chatbot DEGIL.

## Kaynak gercek (spec dosyalari)
- `PROJECT.md` — urun/mimari spec, 40 bolum. Mimari karar oncesi buraya bak.
- `TRANSCRIPT.md` — urun niyeti ve gereksinimlerin cikis noktasi.
- `asuna-docs/AGENT-SPEC-ORIGINAL.md` — coding agent kurallari (prime directive, guvenlik, memory, tool kurallari).
- `asuna-docs/DECISIONS.md` — verilmis mimari kararlar (ADR-001..ADR-007).

## Project Status
- **Phase 0**: KAPANIYOR (10/11) — Tauri 2 iskeleti ayakta, bos pencere aciliyor, CI yesil,
  ASU-005..008 arastirmalari ADR'ye baglandi, dokumantasyon (ASU-010) tamam.
  - Kalan tek task: **ASU-008b** — sherpa-onnx KWS detection spike'i. Phase 1'i **bloklamaz**,
    ama Phase 2 (wake word) baslamadan bitmeli; ADR-004 o zamana kadar `proposed`.
- **Sirada Phase 1: realtime voice** (ASU-011 ephemeral token minting ile baslar).
- Katman mimarileri: `docs/architecture/{voice,memory,tools,security}.md`.

## Important Patterns
- **Urun dongusu**: wake → sesli konusma → baglamsal yardim → memory/tool kullanimi → guvenli oturum kapanisi → idle.
  Bu dongunun disina cikan bir tasarim (ornegin buyuk dashboard) once sorgulanir.
- **Modul sinirlari ayri kalir**: audio / agent / memory / projects / tools / permissions / security / database / ui.
  React componentleri dogrudan shell komutu calistirmaz veya DB sorgusu atmaz.
- **Stack**: Tauri 2 + React + TypeScript (strict) + Vite, pnpm, SQLite, OpenAI Agents SDK
  (`RealtimeAgent` / `RealtimeSession`), WebRTC transport, sherpa-onnx KWS (Rust tarafinda,
  `WakeWordProvider` adapter arkasinda).
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
- **SQLite'a erisim yalnizca Rust'tan** (ADR-005 accepted): `rusqlite` + dar `#[tauri::command]`'lar.
  Renderer SQL yazmaz, DB yolunu vermez. `tauri-plugin-sql` olcumle elendi.
- **Yeni `#[tauri::command]` = 3 adim**, atlanirsa **sessiz red**: `build.rs` `AppManifest`
  listesi + `src-tauri/permissions/` izni + capability, ayrica capability identifier'i
  `tauri.conf.json → app.security.capabilities` dizisine de eklenir.
- **CSP `pnpm tauri dev`'de uygulanmaz.** `connect-src` hatasi sadece paketlenmis build'de
  gorunur — ses dev'de calisip `tauri build` sonrasi sessizce olebilir (ASU-007'de yasandi).
- ChatGPT aboneligi ile OpenAI API kredisi ayri faturalanir — Realtime kullanimi ayrica ucretlendirilir.
  Gelistirmede `gpt-realtime-2.1-mini` kullan.
- Wake word icin API anahtari/ucret yok (sherpa-onnx, Apache-2.0). KWS model dosyalari indirilir;
  yol/esik `ASUNA_WAKE_WORD_MODEL_DIR` + `ASUNA_WAKE_WORD_THRESHOLD` ile verilir. Model
  agirliklarinin lisansi ASU-008b'de dogrulanacak.

## Working Credentials (Dev)
- Yok. Gercek secret ASLA buraya yazilmaz — `.env.example` sablon, `.env` commit edilmez.

> Kurallar: session basinda oku; gotcha/pattern kesfedilince aninda guncelle;
> yanlis/eski bilgiyi sil; kisa tut. Mimari kararlar buraya DEGIL → DECISIONS.md.

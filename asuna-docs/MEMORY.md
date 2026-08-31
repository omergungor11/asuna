# Project Memory

## Project Info
- Asuna: macOS uzerinde local-first calisan kisisel AI companion. Iki giris: **kalici metin
  konusmasi** (ChatGPT/Claude-tarzi kabuk) ve **voice mode** ("Hey Asuna" ile uyanan Realtime
  oturumu). Ayni cekirdek: hafiza, proje baglami, tool + onay/audit. Pivot: **ADR-008**
  (2026-08-31) — eski "chatbot DEGIL" kurali kaldirildi; yerine gecen kural: `VoicePanel`
  asla unmount edilmez, dashboard'a donusme hala non-goal.

## Kaynak gercek (spec dosyalari)
- `PROJECT.md` — urun/mimari spec, 40 bolum. Mimari karar oncesi buraya bak.
- `TRANSCRIPT.md` — urun niyeti ve gereksinimlerin cikis noktasi.
- `asuna-docs/AGENT-SPEC-ORIGINAL.md` — coding agent kurallari (prime directive, guvenlik, memory, tool kurallari).
- `asuna-docs/DECISIONS.md` — verilmis mimari kararlar (ADR-001..ADR-008).
  Not: `AGENT-SPEC-ORIGINAL.md` ve `PROJECT.md` pivot **oncesi** metinlerdir; sohbet UI yasagi
  konusunda ADR-008 ve CLAUDE.md gecerlidir.

## Project Status

> **PROJE ASKIDA (2026-08-31, kullanici karari).** Sebep + teshis + devam adimlari:
> `asuna-tasks/active/session-notes.md` son bolum. Kod tabani saglam; eksik olan prompt
> katmani (core.v3 "# Tools") ve sesli kabul testlerinin tamamlanmasi.
- **Phase 0 TAMAM** (11/11) — scaffold + CI + 4 arastirma ADR'ye baglandi.
- **Phase 1 TAMAM** (ASU-011..020) — realtime voice dikey dilimi calisiyor.
  **M1 canli testte gecti (2026-08-24)**: Turkce anlasildi, barge-in sorunsuz, temiz kapanis.
  Acik: **ASU-064** — fark edilir gecikme, turn detection ayari + olcum.
- **Phase 2 (wake word)**: bloklu — ADR-004'te model + ifade secimi ACIK
  (gercek mikrofon testi kullaniciyi bekliyor). ASU-021 (interface + fake provider) DONE.
- **Phase 3 (memory) kod TAMAM** (ASU-029..037 + ASU-065) — M3 sesli kabul (ASU-038) bekliyor.
  Acik: **ASU-066** (Cmd+Q finalize yarisi).
- **Phase 4 (project context) kod TAMAM** (ASU-039..045) — ASU-046 sesli kabul bekliyor.
- **Phase 5 (tools) kod TAMAM** (ASU-047..054 + Wave D ASU-067..070, 2026-08-31): registry +
  approval matrisi + path sandbox + `tool_events` audit + onay karti + Araclar sekmesi; **yedi
  tool** — `get_current_project`, `list_projects`, `read_project_file`, `list_project_files`
  (risk 0); `open_project`, `set_current_project` (risk 1); `register_project` (**risk 2**).
  Risk 3 yok. **ASU-055** (M4) ve **ASU-071** (Wave D) sesli kabul bekliyor; senaryolar
  `asuna-config/testing.md` → "M4 kabul senaryosu" (A1..A18).
- **Phase 7 (Chat Shell pivotu) kod TAMAM** (ASU-072..078, 2026-08-31): migration 006
  (`messages`/`attachments` + `sessions.title`/`modality`, CASCADE), Rust `chat_send` proxy'si
  (key Rust'ta; **zorunlu** yeni env `ASUNA_CHAT_MODEL`; non-streaming; kullanici + asistan
  mesaji tek transaction), `attachment_ingest` / `attachment_from_project`, capability
  `asuna-chat.json` (**`message_append` bilerek ACL disi**), chat kabugu UI.
  Gate 3: 0 CRITICAL. **ASU-079** (M6 kabul testi) bekliyor.
- Katman mimarileri: `docs/architecture/{voice,memory,tools,security}.md`.

## Important Patterns
- **Urun dongusu**: wake → sesli konusma → baglamsal yardim → memory/tool kullanimi → guvenli oturum kapanisi → idle.
  Metin girisi ayni cekirdegi kullanir (ADR-008). Bu dongunun disina cikan bir tasarim
  (ornegin buyuk dashboard) once sorgulanir.
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
- **Yeni `#[tauri::command]` = 4 adim**, atlanirsa **sessiz red**: `build.rs` `AppManifest`
  listesi + `src-tauri/permissions/` izni + capability, ayrica capability identifier'i
  `tauri.conf.json → app.security.capabilities` dizisine de eklenir.
- **CSP `pnpm tauri dev`'de uygulanmaz.** `connect-src` hatasi sadece paketlenmis build'de
  gorunur — ses dev'de calisip `tauri build` sonrasi sessizce olebilir (ASU-007'de yasandi).
- **`freezePrototype: true` YASAK** (ASU-020): WKWebView'de beyaz ekran — zod compat katmanindaki
  atama WebKit "override mistake" kuraline takiliyor. `false` kalir (gerekce DECISIONS.md).
- **Vite bagimlilik re-optimize sonrasi beyaz ekran** — dev'de dep degistiginde webview yarisi
  kaybedebiliyor; **Cmd+R** cozer. Kod hatasi sanip debug'a girme.
- **OpenAI API kredisi ≠ ChatGPT aboneligi.** Kredi yoksa `insufficient_quota` **tum**
  endpoint'lerde doner (token minting dahil) — 401 gibi gorunmez. Cozum: Billing → Add to
  credit balance. Gelistirmede `gpt-realtime-2.1-mini` kullan.
- **`ASUNA_EDITOR_COMMAND` zorunlu anahtar** (ASU-052): `.env`'de satir yoksa uygulama acilista
  `ConfigError::Missing` ile durur. Bos deger = `code`; deger bosluk/metakarakter iceremez.
  macOS GUI process'inde PATH dar olabilir — gerekirse `which code` tam yolu yazilir.
- **Uclari degil dikisi test et** (Phase 5 Gate 3 / C1): zincirin iki ucu ayri ayri testliydi,
  aradaki tek satir (`toSdkTool` → `executeTool` kanca gecisi) eksikti ve tum binding alanlari
  opsiyonel oldugu icin derleyici yakalamadi. Sessiz sonuc: kapali tool calisiyordu.
- **macOS firmlink: `/System/Volumes/Data`** (Phase 5 Wave D / Gate 3 C1) — ayni dizinin **iki**
  kanonik yolu var: `~/Library` ile `/System/Volumes/Data/Users/<ad>/Library`. `canonicalize`
  ikincisini dondurebilir, yani **normalize etmeden on-ek kurali yazma** — yazarsan koruma
  sessizce atlatilir. Ayni aile: bir **ata** dizin (`/Users`) tam-eslesme kontrolune takilmaz
  ama korunan agacin tamamini acar; ata reddi ayrica gerekiyor.
- **Kullanicinin makinesinde Cloudflare WARP acik** — WebRTC gecikmesinin suphelisi;
  ASU-064 olcumunde once WARP kapali/acik karsilastirilir.
- Wake word icin API anahtari/ucret yok (sherpa-onnx, Apache-2.0). KWS model dosyalari indirilir;
  yol/esik `ASUNA_WAKE_WORD_MODEL_DIR` + `ASUNA_WAKE_WORD_THRESHOLD` ile verilir. Lisans/CPU/bundle
  ASU-008b'de dogrulandi (Apache-2.0, %2.3 CPU, +20.7MB). **Ama `gigaspeech-3.3M` "Hey Asuna"yi
  tasimiyor** — model + ifade secimi hala acik (R2).

## Working Credentials (Dev)
- Yok. Gercek secret ASLA buraya yazilmaz — `.env.example` sablon, `.env` commit edilmez.

> Kurallar: session basinda oku; gotcha/pattern kesfedilince aninda guncelle;
> yanlis/eski bilgiyi sil; kisa tut. Mimari kararlar buraya DEGIL → DECISIONS.md.

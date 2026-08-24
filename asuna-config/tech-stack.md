# Tech Stack

> Asuna: local-first, macOS, "Hey Asuna" wake word'lu sesli kisisel AI companion — chatbot degil.
> Kaynak: `PROJECT.md` Bolum 6-8, 12, 17, 19, 23, 28. Bu dosya o spec'in karara donmus halidir.
> Versiyonlariyla birlikte yaz; buyuk versiyon yukseltmeleri `asuna-docs/DECISIONS.md`'ye kaydedilir.
> Kesin minor/patch versiyonlar Phase 0 scaffold'unda pinlenir. Sonda **ACIK SORULAR** bolumu var —
> hepsi Phase 0 (teknik arastirma + scaffold) sonunda kapanmali.

## Ozet

| Katman | Secim | Durum |
|--------|-------|-------|
| Desktop shell | Tauri 2 (Rust host + system webview) | Karar |
| Paket yoneticisi | pnpm | Karar |
| Frontend | React + TypeScript (strict) + Vite | Karar |
| AI / orchestration | `@openai/agents-realtime` **0.17.0** (exact pin) — RealtimeAgent / RealtimeSession | Karar (ASU-006) |
| Ses transport | WebRTC (`transport: 'webrtc'` acikca) | Karar — WKWebView'de dogrulandi (ASU-007) |
| Realtime model | `gpt-realtime-2.1` (dev: `gpt-realtime-2.1-mini`) — env ile | Karar — dogrulandi (ASU-006) |
| Wake word | **sherpa-onnx `KeywordSpotter`** (Rust, `cpal`), `WakeWordProvider` adapter arkasinda | Motor: Karar (ASU-008b: %2.3 CPU/38MB, lisans OK). **Model+ifade ACIK** — gigaspeech-3.3M "Hey Asuna"yi tasimiyor (`docs/decisions/ADR-004`) |
| Veritabani | SQLite — Rust servis (`rusqlite`), `docs/decisions/ADR-005` | Karar (ASU-005) |
| Secret / auth | Ephemeral Realtime token, key Rust tarafinda | Karar |
| Test | Vitest (unit/integration) + Rust `cargo test` | Karar |

---

## 1. Desktop Shell — Tauri 2

- **Tauri 2.x** — Rust host process + macOS system webview (WKWebView)
- Hedef platform: macOS (Apple Silicon oncelikli); Windows/Linux MVP hedefi degil
- Rust tarafi "guvenilir process": ephemeral token uretimi, SQLite erisimi, dosya/git okuma, tool execution
- `src-tauri/` altinda capability/permission dosyalari; webview'e sadece ihtiyac duyulan komutlar acilir

**Neden:** Asuna surekli acik duran, mikrofon dinleyen ve bilgisayarda islem yapan bir uygulama —
yetki yuzeyi urunun kendisi. Tauri'nin capability tabanli izin modeli her komutu tek tek acmayi
zorunlu kilar, boylece "kisitsiz main process" riski bastan yok.

**Neden Electron degil:** Electron'un main process'i varsayilan olarak tam Node yetkisine sahip;
Asuna'nin risk seviyeli tool mimarisi (PROJECT.md 5.4, 17, 18) icin bu yanlis varsayilan.
Ayrica chromium bundle'i ~120MB+ ve surekli calisan bir companion icin bellek maliyeti yuksek.
Rust tarafi ayrica wake-word motoru (sherpa-onnx KWS) ve SQLite icin native erisimi zaten gerektiriyor.

**Not:** PROJECT.md 6.1 "mevcut template'i koru" diyor; template audit yapildi — repoda uygulama
kodu yok, sadece Claude Code workflow meta-template'i var. Dolayisiyla scaffold greenfield.

---

## 2. Frontend

- **React 19.x** + **TypeScript 5.x** (`strict: true`, `any` yasak)
- **Vite 7.x** dev server / build (Tauri'nin `devUrl` hedefi)
- **pnpm** — workspace tek paket ile baslar, gerekirse bolunur
- Styling: scaffold'da sade CSS Modules / tek utility katmani; UI ana urun degil (PROJECT.md 21)
- State: voice state machine icin acik reducer/FSM; genel UI state icin hafif store (zustand sinifi)
- Sunucu tarafi framework yok — Next.js/SSR kavramlari Asuna'da gecersiz

**Neden:** Voice birincil arayuz, UI guven ve kontrol icin var. Agir UI framework'u urunu
yavaslatir; React + Vite Tauri webview'inde en dusuk surtunmeli, ekip icin en bilindik kombinasyon.

### Voice state machine

Uygulamanin tek dogru durum kaynagi. Gecisler loglanir (PROJECT.md 29), UI dogrudan bu duruma bakar:

```text
BOOTING
IDLE_WAKE_WORD
WAKING
CONNECTING
LISTENING
USER_SPEAKING
ASSISTANT_THINKING
ASSISTANT_SPEAKING
TOOL_PENDING
AWAITING_APPROVAL
ERROR
```

Kurallar:
- `IDLE_WAKE_WORD` varsayilan durum; bu durumda **Realtime session yok, buluta ses gitmiyor**
- Wake word yakalandiginda: `IDLE_WAKE_WORD → WAKING → CONNECTING → LISTENING`
- Oturum kapanisinda (explicit kapatma / inactivity timeout / kurtarilamaz hata) her yol
  `IDLE_WAKE_WORD`'e doner — `ERROR` dahil
- Durum degerleri SCREAMING_SNAKE string union; magic string kullanilmaz

---

## 3. AI / Orchestration

> Dogrulama: ASU-006 arastirmasi, 2026-08-24. Detayli API imzalari, event listesi ve
> dogrulanamayan maddeler: `docs/architecture/voice.md`.

- **`@openai/agents-realtime` `0.17.0`** (exact pin, caret yok) — `RealtimeAgent` + `RealtimeSession`
  - Peer: **`zod` `4.4.3`** (`^4.0.0` zorunlu — Zod 3 calismaz)
  - Runtime: **Node.js 22+** (paketlerde `engines` alani yok, CI'da kontrol edilir)
  - Lisans: MIT. Bagimlilik zinciri: `@openai/agents-core@0.17.0` (exact) → `openai@^7.2.0`
  - **`@openai/agents` degil**: Asuna renderer'i sadece realtime kullaniyor; meta-paket ayrica
    `@openai/agents-openai`'i da cekiyor. Resmi docs standalone realtime paketini destekliyor.
- **Surum hizi riski**: 3 haftada 3 minor (0.14 → 0.17); minor'larda realtime davranisi degisiyor
  (0.15.0: `mediaStream` sahiplik degisikligi). Yukseltme ayri task, release notes okunarak.
- Transport: **WebRTC** — `transport: 'webrtc'` **acikca** verilir; otomatik secim
  `window.RTCPeerConnection` yoksa sessizce WebSocket'e duser (calismaz ses = sinsi hata)
- **Ephemeral key zorunlu**: SDK, browser ortaminda `ek_` prefix'i olmayan key ile WebRTC
  baglantisini `UserError` ile reddediyor. `useInsecureApiKey` **yasak**.
- Ephemeral token endpoint: `POST https://api.openai.com/v1/realtime/client_secrets`
  (`expires_after.seconds` 10–7200, varsayilan 600) — Rust tarafinda `#[tauri::command]`
- Interruption/barge-in **SDK + sunucu VAD** yonetiyor (`semantic_vad` varsayilan); WebRTC'de
  ses buffer'ini SDK temizler. Uygulama sadece `audio_interrupted` ile UI/state gunceller.
- SDK detaylari **`AsunaRealtimeService`** wrapper'i arkasinda kalir; React bilesenleri
  ve tool katmani SDK tipleriyle dogrudan konusmaz (PROJECT.md 24)
- Tool tanimlari SDK formatina `AsunaToolDefinition` registry'sinden adapte edilir — ters yon degil
- **Function tool'lar renderer'da calisir** — gercek is `#[tauri::command]` uzerinden Rust'a
  delege edilir (ince backchannel deseni)
- SDK varsayilani kullanici transkripsiyonunu **acik** getiriyor (`gpt-4o-mini-transcribe`) —
  `ASUNA_TRANSCRIPT_STORAGE=false` ise `audio.input.transcription: null` verilmeli
  (gizlilik + maliyet sizintisi onlemi)

**Neden:** Realtime API'yi elle surmek (ses chunk'lari, interruption, VAD, tool call protokolu)
Phase 1'i haftalara yayar. Agents SDK bu dongunun tamamini kapsar; wrapper sayesinde ileride
lower-level API'ye ya da baska saglayiciya gecis tek dosyalik degisiklik olur.

### Model konfigurasyonu

Model ID'leri **asla hard-code edilmez**. Tek okuma noktasi config servisi:

```env
ASUNA_REALTIME_MODEL=gpt-realtime-2.1        # quality
# ASUNA_REALTIME_MODEL=gpt-realtime-2.1-mini # economy / development
ASUNA_REALTIME_VOICE=marin
```

- Her iki model ID de SDK `OpenAIRealtimeModels` union'inda mevcut; `gpt-realtime-2.1` ayrica
  SDK'nin `DEFAULT_OPENAI_REALTIME_MODEL` degeri. Ilan edilmis deprecation yok.
- `gpt-realtime` / `gpt-realtime-mini` (2.1'siz) **2027-01-20'de kapaniyor** — kullanilmaz.
- `ASUNA_REALTIME_VOICE` → `config.audio.output.voice`. Gecerli degerler:
  `alloy, ash, ballad, coral, echo, sage, shimmer, verse, marin, cedar`.
  Ses, oturum ses uretmeye basladiktan sonra **degistirilemez**.
- Token basarken kullanilan model ile `RealtimeSession({ model })` **ayni olmali**;
  model oturum ortasinda degistirilemez.
- Realtime oturumu **max 60 dakika** (API limiti) — oturum suresi siniri altinda kalmali.
- Ayarlar UI'inda model secilebilir olmali (PROJECT.md 21, 28).

---

## 4. Wake Word

> Karar `docs/decisions/ADR-004-wake-word-provider.md`'de (proposed — detection spike bekliyor).
> Onceki Porcupine karari ASU-008 arastirmasiyla gecersiz kaldi: Picovoice Free Tier 2026-06-30'da
> kapatildi ("no non-commercial tier planned"), Rust binding'i yanked, AccessKey init'te **online**
> dogrulaniyor (local-first ihlali).

- **sherpa-onnx `KeywordSpotter`** — `sherpa-onnx` crate 1.13.5 (Apache-2.0), Tauri **Rust
  process'inde**; mikrofon idle'da `cpal` 0.16 ile Rust tarafindan acilir
- Open-vocabulary KWS: model egitimi/vendor console **yok** — "HEY ASUNA" BPE token'a cevrilip
  `keywords.txt`'ye yazilir (`sherpa-onnx-cli text2token`); AccessKey yok, kota yok, phone-home yok
- Model: `sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01` (int8 ~5MB) — model agirliklarinin
  lisansi spike'ta dogrulanacak
- Tetikleyici ifade: **"Hey Asuna"** (MVP'de tek trigger — false positive dusuk kalsin)
- Zorunlu soyutlama:

```ts
interface WakeWordProvider {
  initialize(): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
  onDetected(callback: (event: WakeWordEvent) => void): () => void;
}
```

- `SherpaKwsProvider` bu interface'in **tek** somut ornegidir; uygulamanin geri kalani
  `WakeWordProvider` tipini gorur, vendor adini gormez. Yedekler: `oww-rs` (MIT), `rustpotter`.
- Idle'da mikrofon **renderer'a hic acilmaz** (cpal Rust'ta) — wake aninda cpal stream durur,
  Tauri event'i ile renderer `getUserMedia` + WebRTC acar; oturum kapaninca tersine doner
- Konfigurasyon (`.env.example`, ASU-009): `ASUNA_WAKE_WORD_PROVIDER` (varsayilan `sherpa-kws`),
  `ASUNA_WAKE_WORD_MODEL_DIR`, `ASUNA_WAKE_WORD_THRESHOLD`, `ASUNA_WAKE_WORD`

**Neden:** Lisans/erisim riski sifir (Apache-2.0, offline), gizlilik mimarisi daha guclu
(idle'da webview mikrofona dokunmaz) ve Apple Silicon birinci sinif destekli. Detection kalitesi
tek acik risk — spike (ASU-008b) ile dogrulanacak; kalirsa Silero VAD kapisi / ifade uzatma /
yedek motorlar devrede (ADR-004 exit plani).

**Gizlilik (pazarlik disi):** Idle mikrofon frame'leri sadece wake-word motoruna gider,
OpenAI'a gonderilmez, diske yazilmaz (PROJECT.md 8, 20).

---

## 5. Database

- **SQLite** — tek dosya, local-first, MVP icin dogru boyut
- Semalar: `memories`, `projects`, `sessions`, `tasks`, `tool_events` (PROJECT.md 12.2)
- Migration'lar versiyonlu SQL dosyalari; uygulama acilisinda idempotent uygulanir
- Vector/embedding katmani **Stage B** — yeterli memory birikmeden eklenmez (PROJECT.md 13)

**Neden:** Memory urunun merkezi ama MVP'de sorgu hacmi kucuk ve tek kullanicili.
Vector platformu ya da sunucu DB'si bu asamada cozdugunden fazla sorun uretir.

**Erisim yolu: KARAR VERILDI (ASU-005 spike, `docs/decisions/ADR-005-sqlite-access.md`).**

**Secenek B — Rust persistence servisi:**
- `rusqlite` **0.40.2** (bundled SQLite 3.53.2, MIT) + `rusqlite_migration` **2.6.0**
  (`PRAGMA user_version`, up/down gercekten calisiyor)
- SQL `src-tauri` disina cikmaz; webview'e dar amacli, kaba taneli `#[tauri::command]`'lar
  (`memory_create`, `memory_search`, ...) — komut hicbir zaman SQL string'i almaz
- Komut basina ACL: `src-tauri/permissions/*.toml` + capability dosyasi; okuma/yazma ayri izin
- `memories` + `tool_events` tek `rusqlite::Transaction`'da (audit atomikligi)
- DB konumu: `app_data_dir()` → `~/Library/Application Support/com.omergungor.asuna/asuna.db`;
  WAL + foreign_keys + busy_timeout acilista; yol renderer'dan asla parametre alinmaz
- SQLCipher gecisi tek feature degisikligi (`bundled-sqlcipher`) + `PRAGMA key`

`tauri-plugin-sql` olcumle elendi: scope'suz ACL (`allow-execute` = renderer'dan `DROP TABLE`
calisiyor), path sandbox yok (mutlak path + `ATTACH DATABASE` kacisi), transaction yok,
down migration sessizce atiliyor. Detayli olcumler ADR-005'te.

DIKKAT (spike'ta olculen iki Tauri tuzagi): (1) yeni capability identifier'i
`tauri.conf.json → app.security.capabilities` dizisine DE eklenmeli, yoksa sessiz red;
(2) `src-tauri/permissions/` dizini olustugu anda TUM uygulama komutlari ACL'e tabi olur.

---

## 6. Auth & Secrets

Tek katı kural: **kalici OpenAI API key'i renderer/webview bundle'ina asla girmez** (PROJECT.md 7, 19).

Akis:

```text
1. OPENAI_API_KEY sadece guvenilir process'te (Tauri Rust tarafi) okunur
2. Webview "oturum baslatacagim" der -> #[tauri::command] cagrisi
3. Rust tarafi OpenAI'dan kisa omurlu Realtime client secret (ephemeral token) alir
4. Webview WebRTC baglantisini SADECE bu gecici token ile kurar
5. Token suresi dolunca yenisi istenir; kalici key hicbir zaman IPC'den gecmez
```

- Key kaynagi: `.env` (git disi) → ileride macOS Keychain (PROJECT.md 20)
- `.env.example` gercek deger icermez
- Loglara, hata mesajlarina, tool argumanlarina secret yazilmaz; `tool_events.arguments_redacted`
  zaten redakte alan
- Vite `import.meta.env` uzerinden **hicbir** OpenAI credential'i expose edilmez —
  `VITE_` prefix'li secret yasak

---

## 7. Maliyet Yonetimi (PROJECT.md 28)

Sesli agent surekli API tuketimi uretebilir. Mimariye gomulu kontroller:

- **Idle'da Realtime session yok** — en buyuk tasarruf kalemi; wake word beklerken sifir API maliyeti
- Inactivity timeout ile otomatik disconnect (`ASUNA_IDLE_TIMEOUT_SECONDS=45`)
- Maksimum oturum suresi siniri
- Oturum sure takibi + varsa token/audio kullanim metadatasi → `sessions` tablosuna yazilir
- Gunluk tahmini maliyet gostergesi (Ayarlar ekrani)
- Model secimi kullanici tarafindan degistirilebilir: quality `gpt-realtime-2.1` /
  economy `gpt-realtime-2.1-mini`
- Gelistirme varsayilani `-mini`

Dogrulanmis fiyatlar (developers.openai.com/api/docs/pricing, 2026-08-24, USD / 1M token):

| Model | Audio in | Cached audio in | Audio out | Text in | Cached text in | Text out |
|---|---|---|---|---|---|---|
| `gpt-realtime-2.1` | $32.00 | $0.40 | $64.00 | $4.00 | $0.40 | $24.00 |
| `gpt-realtime-2.1-mini` | $10.00 | $0.30 | $20.00 | $0.60 | $0.06 | $2.40 |

Kaba dakika maliyeti **TAHMIN** (600 tok/dk giris, 1200 tok/dk cikis varsayimi — resmi kaynaktan
dogrulanamadi): 2.1 ≈ $0.10/dk, mini ≈ $0.03/dk. Gercek deger Phase 1'de `session.usage` ile olculur.

**Faturalama notu:** ChatGPT aboneligi ile OpenAI API faturalandirmasi ayri sistemlerdir.
ChatGPT Plus/Pro Realtime API kredisi saglamaz — API erisimi ayrica konfigure edilir.

> Deneyimi kanitlamadan erken optimizasyon yapma; ama idle'da baglanti acik birakmak
> "erken optimizasyon" degil, tasarim hatasidir.

---

## 8. Test & Tooling

- **Vitest** — unit + integration (memory ranking, permission logic, path sandbox,
  project detection, tool schema, state gecisleri)
- **Rust:** `cargo test` + `clippy` — ephemeral token uretimi, path normalizasyonu, DB katmani
- Lint/format: ESLint (typescript-eslint) + Prettier; Rust `rustfmt`
- CI: GitHub Actions — typecheck, lint, test, `cargo build` (Phase 0 cikti kriteri: CI yesil)
- E2E/manuel: sesli akis manuel kabul testleriyle dogrulanir (PROJECT.md 31)

---

## ACIK SORULAR

Phase 0'da (teknik arastirma + scaffold) kapatilir. Karar cikan her madde
`asuna-docs/DECISIONS.md`'ye yazilir ve bu dosya guncellenir.

| ID | Soru | Secenekler / Not | Faz |
|----|------|------------------|-----|
| OQ-1 | ~~SQLite erisim yolu~~ **KAPANDI** (ASU-005): B — Rust servis, `rusqlite` 0.40.2; `tauri-plugin-sql` olcumle elendi (`docs/decisions/ADR-005-sqlite-access.md`) | — | Kapandi |
| OQ-2 | ~~Migration araci~~ **KAPANDI** (ASU-005): `rusqlite_migration` 2.6.0 (`user_version`, up/down, `validate()` CI testi) | — | Kapandi |
| OQ-3 | ~~Porcupine lisans modeli~~ **KAPANDI** (ASU-008): Free Tier kapatildi, non-commercial tier yok → Porcupine elendi, sherpa-onnx secildi (`docs/decisions/ADR-004`). Kalan tek lisans sorusu KWS model agirliklari — spike'ta (ASU-008b) | — | Kapandi |
| OQ-4 | ~~Wake word hangi tarafta?~~ **KAPANDI** (ASU-008): Rust tarafinda (sherpa-onnx + cpal); idle'da mikrofon renderer'a hic acilmiyor | — | Kapandi |
| OQ-5 | ~~WebRTC WKWebView'de calisiyor mu?~~ **KAPANDI** (ASU-007): calisiyor — gUM+TCC kalici izin, SDP/DTLS/opus/data-channel, srflx/STUN, autoplay engelsiz; fallback gerekmedi. KRITIK: prod CSP connect-src'a api.openai.com eklendi (dev'de gorunmeyen blocker). Detay voice.md Bolum 11 | — | Kapandi |
| OQ-6 | Mikrofon devir teslimi: Rust cpal (idle) ↔ renderer getUserMedia (aktif) | Gecis suresi, macOS TCC izin sayisi, turuncu gosterge davranisi — ASU-008b spike'inda olculur | Phase 0/2 |
| OQ-7 | ~~Ephemeral token endpoint~~ **KAPANDI** (ASU-006): `POST /v1/realtime/client_secrets`, `expires_after.seconds` 10–7200 (varsayilan 600), yanit `ek_` prefix'li `value` — detay `docs/architecture/voice.md` Bolum 5 | — | Kapandi |
| OQ-8 | Styling katmani ne olacak? | CSS Modules / Tailwind / minimal custom — UI ana urun degil, en az bakim gerektiren secilir | Phase 1 |
| OQ-9 | DB sifreleme (SQLCipher) ne zaman? | MVP'de duz dosya kabul; sifreleme Phase 3 sonrasi — OQ-1 secimi bunu bloklamamali | Phase 3+ |
| OQ-10 | Uygulama imzalama/notarization gerekli mi? | Mikrofon izni ve kalici kurulum icin macOS gereksinimleri | Phase 2+ |

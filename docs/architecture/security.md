# Security Architecture

> **İskelet — Phase 0 kapanış kaydı (2026-08-24, ASU-010).**
>
> **Bu dosya mimariyi anlatır: güven sınırı nerede, hangi mekanizma onu zorluyor.**
> Madde madde kontrol listesi burada değil → [`asuna-config/security.md`](../../asuna-config/security.md)
> (`/code-review` ve reviewer agent onu kullanır; CRITICAL bulgu = merge engeli).
> Kaynak gerçek: `PROJECT.md` Bölüm 5.4, 8, 18, 19, 20.

## 1. Güven sınırı

Asuna'da tek bir güven sınırı vardır ve o sınır **süreç sınırıdır**: Tauri Rust host
(güvenilir) ↔ WKWebView renderer (güvenilmez).

```text
  ┌─────────────────────────────────────────────────────────────┐
  │  RENDERER  (WKWebView, güvenilmez — bundle incelenebilir)   │
  │                                                             │
  │  React UI · voice state machine                             │
  │  @openai/agents-realtime  ── WebRTC ──▶ api.openai.com      │
  │       (yalnızca `ek_` ephemeral token ile)                   │
  │  tool execute() gövdeleri  ── ince, iş yapmaz ──┐           │
  │  memory-service.ts (invoke wrapper, SQL yazmaz) ─┤           │
  └───────────────────── IPC (invoke) ───────────────┼───────────┘
                             ▲                       │
        ══════════ GÜVEN SINIRI: deny-by-default ACL ═══════════
                             │                       ▼
  ┌─────────────────────────────────────────────────────────────┐
  │  TAURI RUST HOST  (güvenilir)                               │
  │                                                             │
  │  config.rs   OPENAI_API_KEY : SecretString  (dışa çıkmaz)   │
  │  token minting  ──▶ POST /v1/realtime/client_secrets        │
  │  db/  rusqlite ──▶ asuna.db (WAL)   · tool_events (yalnız yazan taraf)
  │  wake word  cpal + sherpa-onnx KWS  (idle mikrofon burada)  │
  │  tool execution  path sandbox · blocklist · timeout · audit │
  └─────────────────────────────────────────────────────────────┘
```

Sınırın anlamı: **renderer'a giren her şey sızmış kabul edilir.** Bundle okunabilir,
webview'da yürüyen kod incelenebilir. Bu yüzden kalıcı credential, ham SQL, mutlak
dosya yolu kararı ve mikrofonun idle sahipliği sınırın Rust tarafında durur.

## 2. Ephemeral token akışı (ADR-006 + ASU-006 doğrulaması)

```text
1. OPENAI_API_KEY yalnızca Rust tarafında okunur (src-tauri/src/config.rs)
2. Renderer: invoke('mint_realtime_token')
3. Rust: POST https://api.openai.com/v1/realtime/client_secrets
        body { expires_after: { anchor: "created_at", seconds: 600 },
               session: { type: "realtime", model } }
4. Rust → renderer: { value: "ek_...", expires_at, model }        <- kalıcı key ASLA geçmez
5. Renderer: session.connect({ apiKey: () => invoke('mint_realtime_token') })
```

| Kural | Neden |
|---|---|
| Token her `connect()` öncesi taze basılır; cache'lenmez, log'lanmaz, diske yazılmaz | sızıntı penceresi dar kalsın |
| `session` payload'ı minimum (`type`, `model`) | `instructions`/`tools` zaten data channel'dan gidiyor; iki yerde tutmak drift üretir |
| `useInsecureApiKey` **yasak** | bu flag güven sınırını tek satırda siler |
| Token basarken kullanılan model = `RealtimeSession({ model })` | model oturum ortasında değişemez (Realtime API kuralı) |

**SDK bu kuralı bizim için zorluyor:** browser ortamında `ek_` prefix'i olmayan key ile
WebRTC bağlantısı `UserError` ile reddediliyor (`voice.md` Bölüm 4). Yani kalıcı key'i
renderer'a sızdırma hatası runtime'da anında patlar — sessizce çalışmaz.
**Kırılganlık:** bu guard `isBrowserEnvironment()`'a bağlı; yanlış bundler shim'i
seçilirse guard sessizce kapanır → Phase 1'de bir test bunu assert etmeli (`voice.md` V8).

## 3. Secret'ların bellekteki şekli (ASU-009)

| Mekanizma | Ne sağlıyor |
|---|---|
| `SecretString` sarmalayıcı | `Debug` çıktısı `<redacted>` basar; log/panic yoluyla sızma kapanır |
| `AsunaConfig` **`Serialize` türetmez** | API key'in bir Tauri command dönüşünde yer alması **derleme zamanında** imkânsız |
| `#[tauri::command] get_frontend_config` | renderer'a yalnızca 8 alanlık whitelist döner; beklenmeyen alan renderer tarafında da **reddedilir** |
| Kendi `.env` okuyucusu (`env_file.rs`), `dotenvy` **yok** | `dotenvy` değerleri `std::env::set_var` ile tüm process'e yazar → tool katmanı alt process açtığında `OPENAI_API_KEY` çocuğa miras kalırdı. Okuyucu `BTreeMap` döner, değer yalnızca `AsunaConfig` içinde yaşar |
| Hata mesajları | eksik/geçersiz config'te **değer değil yalnızca alan adı** taşınır |
| Build kanıtı | `pnpm build` sonrası `grep -r "OPENAI_API_KEY" dist/` eşleşme yok; canary değerli build ile tekrarlandı |

`VITE_` prefix'li hiçbir OpenAI credential'ı yok ve olamaz — Vite `import.meta.env`
üzerinden secret expose etmek yasak.

Sonraki adım: key kaynağı `.env` → **macOS Keychain** (PROJECT.md 20). Bu değişiklik
renderer'ı etkilemez; `config.rs` içinde tek okuma noktası değişir.

## 4. ACL: deny-by-default (ASU-009)

Tauri'nin varsayılanı "app command'ları serbest"tir. Asuna bunu **kapattı**:

```rust
// src-tauri/build.rs
let attributes = tauri_build::Attributes::new()
    .app_manifest(tauri_build::AppManifest::new().commands(&["get_frontend_config"]));
```

`AppManifest` tanımlandığı andan itibaren, **burada listelenmeyen ya da bir capability
tarafından açıkça izin verilmeyen komut renderer'dan çağrılamaz.**

Her yeni `#[tauri::command]` için üç adım, atlanırsa **sessiz red**:

1. `build.rs` → `AppManifest::commands([...])` listesine ekle
2. `src-tauri/permissions/` altında dar kapsamlı izin kaydı aç (okuma/yazma **ayrı** izin)
3. `src-tauri/capabilities/*.json` içine izni ekle — **ve** yeni capability identifier'ını
   `tauri.conf.json → app.security.capabilities` dizisine **de** ekle

> Spike'ta ölçülen iki tuzak (ADR-005): (a) capability identifier'ı `tauri.conf.json`'a
> eklenmezse izin sessizce yok sayılır; (b) `src-tauri/permissions/` dizini oluştuğu
> andan itibaren **tüm** uygulama komutları ACL'e tabi olur.

Mevcut durum: tek komut (`get_frontend_config`), tek pencere (`main`),
`capabilities/asuna-config.json` → `allow-get-frontend-config`.

## 5. CSP ve webview kısıtları (ASU-007)

`tauri.conf.json` içinde uygulanan karar:

```json
"connect-src": "'self' ipc: http://ipc.localhost https://api.openai.com"
```

| Bulgu | Sonuç |
|---|---|
| SDK, SDP offer'ını `POST https://api.openai.com/v1/realtime/calls` ile gönderiyor | `connect-src`'a `api.openai.com` **zorunlu** |
| Bu ihlal `pnpm tauri dev`'de **görünmez** (sayfayı Vite servis eder, Tauri CSP header'ı uygulanmaz) | ses dev'de çalışır, `tauri build` sonrası sessizce ölür — CSP değişiklikleri paketlenmiş build'de test edilir |
| WebRTC'nin UDP/ICE trafiği CSP'ye tabi **değil** | kesilen yalnızca SDP HTTP POST'uydu |
| `media-src 'self' blob: mediastream:` yeterli | ek değişiklik gerekmedi |
| WebSocket transport'a düşülürse | `wss://api.openai.com` de eklenmeli |
| `tauri://localhost` bir **secure context** | `getUserMedia` + WebCrypto çalışıyor; `useHttpsScheme` gerekmiyor |
| **wry gotcha:** `requestMediaCapturePermissionForOrigin` delegate'i koşulsuz `Grant` dönüyor | webview seviyesinde mikrofon izin kapısı **yok**; tek kapı macOS TCC → **CSP ve navigasyon kısıtları ekstra kritik**: webview'da yüklenen her origin mikrofona erişebilir |

Bunun doğal sonucu: webview'da harici origin **yüklenmez**. Uzak içerik (link, iframe,
model çıktısındaki URL) sistem tarayıcısına açılır, webview'a değil.

## 6. Path sandbox (plan — Phase 5)

Henüz kod yok; mimari kural şimdiden sabit:

```text
tool args ──▶ [renderer: schema doğrulama (UX)] ──▶ IPC ──▶ Rust:
    kayıtlı proje root'u al  (renderer root SEÇMEZ)
      → normalize + realpath ile ÇÖZ
        → root prefix kontrolü   (string startsWith YETMEZ)
          → blocklist glob'u (symlink çözümünden SONRA uygula)
            → boyut / binary kontrolü
              → izin ver, audit'e yaz
```

- Yazma MVP'de yalnızca `.asuna/notes/` altına. Başka hiçbir yere yazma yok.
- Proje root'ları kullanıcı tarafından **explicit** kaydedilir; "her yeri tara" davranışı yok.
- Blocklist tek merkezî modülde durur (`.env*`, `~/.ssh/**`, `*.pem`, keychain, cloud
  credential'ları — tam liste `asuna-config/security.md` Bölüm 1). Tool'lar kendi
  kopyasını tutmaz.
- Sandbox mantığı için unit test **zorunlu** (`asuna-config/testing.md`).

## 7. Ses gizliliği (mimari sonuç)

Idle'da mikrofon **renderer'a hiç açılmaz** — wake word motoru Rust tarafında (`cpal` +
sherpa-onnx `KeywordSpotter`, ADR-004). Wake anında cpal stream durur, Tauri event'i ile
renderer `getUserMedia` + WebRTC açar; oturum kapanınca tersine döner.

| Ölçülen (ASU-007) | Değer |
|---|---|
| TCC promptu | Paketlenmiş `.app`'te **bir kez**; sonrası kalıcı, gUM 65 ms'de dönüyor |
| `track.stop()` sonrası | track `ended`, macOS turuncu göstergesi söner → mikrofonu bırakmak **uygulamanın** sorumluluğu |
| `navigator.permissions.query({name:'microphone'})` | TCC verilmiş olsa bile `prompt` döner — **izin durumu için kaynak gerçek değil**; gerçek sinyal gUM'ın çözülme süresi |
| `Info.plist` | `NSMicrophoneUsageDescription` **tam olarak** `src-tauri/Info.plist` dosyasında olmalı; dev binary yalnızca bu yolu okur |

Mikrofon devir teslimi (Rust cpal ↔ renderer gUM) tek izin mi iki prompt mu → **OQ-6,
ASU-008b spike'ında ölçülüyor.**

## 8. TODO — açık kalanlar

| # | Açık | Nerede |
|---|---|---|
| T1 | Token minting'in gerçek implementasyonu + hata/yenileme davranışı | ASU-011 |
| T2 | `isBrowserEnvironment()` / `ek_` guard'ının bundle'da aktif olduğunu assert eden test | Phase 1 (`voice.md` V8) |
| T3 | Path sandbox + blocklist kodu ve testleri | ASU-049, ASU-055 |
| T4 | Redaction pattern seti (log, hata, `arguments_redacted`, session summary) | ASU-055 |
| T5 | Navigasyon kısıtı: harici URL'lerin sistem tarayıcısına yönlendirilmesi | Phase 1/2 |
| T6 | Keychain'e geçiş (`.env` → macOS Keychain) | Post-MVP, PROJECT.md 20 |
| T7 | DB şifreleme (SQLCipher) — OQ-9, MVP'de düz dosya kabul | Phase 3+ |
| T8 | `pnpm audit` / `cargo audit` CI gate (high+ fail) | Phase 1 |
| T9 | İmzalama + notarization + entitlement doğrulaması (`audio-input`) — OQ-10 | ASU-063 |
| T10 | Model çıktısının DOM'a basılması: sanitizasyon katmanı (transcript/memory içeriği model çıktısıdır) | Phase 1/3 |

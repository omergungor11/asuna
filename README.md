# Asuna

**Local-first, sesle uyandırılan kişisel AI companion** — macOS.

> "Hey Asuna" → doğal, kesilebilir sesli konuşma → bağlamsal yardım → hafıza + kontrollü
> tool kullanımı → güvenli oturum kapanışı → idle.

Asuna bir chatbot değil; kullanıcının projelerini tanıyan, kalıcı hafıza tutan ve onaylı
yerel aksiyonları güvenli bir tool katmanı üzerinden çalıştıran bir **kişisel AI işletim
katmanı**dır.

## Temel ilkeler

- **Local-first** — wake word tespiti cihazda çalışır; idle mikrofon sesi asla buluta gitmez.
- **Explicit activation** — Asuna gizli bir kayıt cihazı gibi davranmaz; dinleme durumu her zaman görünür.
- **Kontrollü tool katmanı** — her tool risk seviyeli (0-3); mutasyon ve yıkıcı aksiyonlar onay gerektirir, her çağrı audit loglanır.
- **İncelenebilir hafıza** — kullanıcı hafızayı görebilir, düzenleyebilir, silebilir.
- **Secrets asla renderer'da** — OpenAI API key yalnızca güvenilir process'te; istemci kısa ömürlü token kullanır.

## Stack

Tauri 2 · React · TypeScript (strict) · Vite · pnpm · SQLite · OpenAI Agents SDK
(`RealtimeAgent`/`RealtimeSession`, WebRTC) · sherpa-onnx KWS (adapter arkasında)

## Durum

🚧 **Phase 0 kapanıyor** — scaffold ayakta, CI yeşil, Phase 1'i bloklayan teknik kararlar
verildi (ADR-001..007). Sırada **Phase 1: realtime voice**. Henüz sesli konuşma yok;
`pnpm tauri dev` boş Asuna penceresini açar.

Yol haritası: [`asuna-tasks/task-index.md`](asuna-tasks/task-index.md) —
7 faz, 64 task, 5 milestone. MVP hedefi: *wake → talk → remember → one safe tool → idle*.

## Local Kurulum

### Gereksinimler

| Araç | Sürüm | Not |
|------|-------|-----|
| macOS | 13+ (Apple Silicon önerilir) | Tek hedef platform; Windows/Linux MVP hedefi değil |
| Node.js | 22.12+ | Realtime SDK'nın koşulu |
| pnpm | 10.25+ | `corepack enable` veya `npm i -g pnpm@10.25.0` |
| Rust | 1.96.1 | **rustup ile** — `rust-toolchain.toml` sürümü otomatik seçer |
| Xcode Command Line Tools | — | `xcode-select --install` |

```bash
git clone <repo> asuna && cd asuna
cp .env.example .env      # değerleri doldur (aşağıya bak)
pnpm install
pnpm tauri dev            # boş Asuna penceresi açılır
```

### `.env`

`.env` **yalnızca Tauri'nin Rust tarafında** okunur; renderer'a sadece whitelist'lenmiş bir
alt küme geçer. `.env.example`'daki değişkenlerin **tamamı tanımlı olmalı** — eksik/geçersiz
değer uygulamayı açılışta net bir hata mesajıyla durdurur, sessizce varsayılana düşmez.
(Boş bırakılabilenler: `ASUNA_REALTIME_VOICE`, `ASUNA_WAKE_WORD_MODEL_DIR`.)

Gerçek secret asla commit edilmez; `.env` git dışıdır.

### Harici setup

**1. OpenAI API anahtarı + billing.**
[platform.openai.com](https://platform.openai.com) → API key oluştur → `.env` içindeki
`OPENAI_API_KEY` alanına yaz.

> ⚠️ **ChatGPT aboneliği (Plus/Pro) API kredisi sağlamaz.** API faturalandırması ayrı bir
> sistemdir; billing aktif değilse Realtime oturumu `429` ile açılmaz. Geliştirme sırasında
> `ASUNA_REALTIME_MODEL=gpt-realtime-2.1-mini` kullan — yaklaşık 3 kat daha ucuz
> (fiyat tablosu: [`asuna-config/tech-stack.md`](asuna-config/tech-stack.md) Bölüm 7).

**2. Wake word (KWS) model dosyaları — Phase 2'de gerekli.**
Wake word tespiti sherpa-onnx `KeywordSpotter` ile tamamen offline çalışır; AccessKey veya
hesap yok, ama model dosyaları elle indirilir. k2-fsa `sherpa-onnx` release'lerindeki
**kws-models** arşivinden `sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01` (int8, ~5MB)
indirilip bir dizine açılır, yol `.env` içindeki `ASUNA_WAKE_WORD_MODEL_DIR` alanına yazılır.
Detay ve model lisansı durumu: [`docs/decisions/ADR-004-wake-word-provider.md`](docs/decisions/ADR-004-wake-word-provider.md).
Phase 0/1'de bu adım gerekmez (`ASUNA_WAKE_WORD_PROVIDER=fake` ile mikrofon açılmaz).

### Komutlar

| Komut | Ne yapar |
|-------|----------|
| `pnpm tauri dev` | Uygulamayı geliştirme modunda çalıştırır |
| `pnpm tauri build` | macOS `.app` / `.dmg` üretir (imzasız) |
| `pnpm dev` | Yalnızca Vite dev server (webview olmadan) |
| `pnpm typecheck` | `tsc --build --force` |
| `pnpm lint` / `pnpm format:check` | ESLint / Prettier |
| `pnpm test` | Vitest (`test:watch`, `test:coverage`) |
| `pnpm rust:test` / `rust:lint` / `rust:fmt` | `cargo test` / `clippy -D warnings` / `rustfmt --check` |

CI (`.github/workflows/ci.yml`) bu komutların hepsini koşar; hepsi lokalde geçmeden PR açma.

> **Dikkat:** CSP `pnpm tauri dev`'de uygulanmaz — Content Security Policy değişiklikleri
> yalnızca `pnpm tauri build` ile paketlenmiş uygulamada doğrulanabilir
> ([`docs/architecture/security.md`](docs/architecture/security.md) Bölüm 5).

## Dokümantasyon

| Dosya | İçerik |
|-------|--------|
| [`PROJECT.md`](PROJECT.md) | Ürün + mimari spec (kaynak gerçek) |
| [`TRANSCRIPT.md`](TRANSCRIPT.md) | Ürünün çıkış hikâyesi ve gereksinimler |
| [`CLAUDE.md`](CLAUDE.md) | Geliştirme referans kartı + agent orkestrasyonu |
| [`asuna-docs/DECISIONS.md`](asuna-docs/DECISIONS.md) | Mimari kararlar (ADR-001..007) |
| [`docs/architecture/`](docs/architecture/) | Katman mimarileri: `voice`, `memory`, `tools`, `security` |
| [`docs/decisions/`](docs/decisions/) | Detaylı ADR'ler: wake word (ADR-004), SQLite erişimi (ADR-005) |
| [`asuna-config/`](asuna-config/) | Tech stack, konvansiyonlar, test ve güvenlik checklist'leri |
| [`asuna-docs/RUNBOOK.md`](asuna-docs/RUNBOOK.md) | Çalıştırma, build, CI, geri alma |

## Lisans

[MIT](LICENSE)

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

🚧 **Phase 0 — araştırma + scaffold.** Henüz çalışan uygulama yok.

Yol haritası: [`asuna-tasks/task-index.md`](asuna-tasks/task-index.md) —
7 faz, 64 task, 5 milestone. MVP hedefi: *wake → talk → remember → one safe tool → idle*.

## Dokümantasyon

| Dosya | İçerik |
|-------|--------|
| [`PROJECT.md`](PROJECT.md) | Ürün + mimari spec (kaynak gerçek) |
| [`TRANSCRIPT.md`](TRANSCRIPT.md) | Ürünün çıkış hikâyesi ve gereksinimler |
| [`CLAUDE.md`](CLAUDE.md) | Geliştirme referans kartı + agent orkestrasyonu |
| [`asuna-docs/DECISIONS.md`](asuna-docs/DECISIONS.md) | Mimari kararlar (ADR) |

## Lisans

[MIT](LICENSE)

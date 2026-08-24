# Asuna

## Proje

Asuna, macOS üzerinde çalışan **local-first, sesli kişisel AI companion**'dır — "Hey Asuna"
wake word'ü ile uyanır, doğal ve kesilebilir (interruptible) konuşma yürütür, kullanıcının
üzerinde çalıştığı projeyi tanır, kalıcı hafıza tutar ve onaylı yerel aksiyonları kontrollü bir
tool katmanı üzerinden çalıştırır. Amaç "AI'a gitmek" değil; AI'ın ortamda hazır beklemesi.

**Ürün döngüsü:**

```
wake → voice conversation → contextual help → memory/tool use → safe session close → idle
```

**Prime directive: Asuna generic chatbot UI değildir.** Sohbet penceresi, mesaj balonu, "send"
butonu merkezli bir arayüz kurma. Ses birincildir; UI'nin işi sistemi güvenilir göstermektir
(listening / connected / speaking / tool usage / approval / error / current project). Voice loop
çalışmadan büyük dashboard inşa etme.

- **GitHub**: https://github.com/omergungor11/asuna (public, MIT)

## Slash Commandlar

| Command | Ne yapar |
|---------|----------|
| `/cold-start` | Session başlangıcı — projeyi oku, durumu raporla (sadece session başında) |
| `/status` | Hızlı durum — dashboard + son commit'ler (gün içinde bunu kullan) |
| `/plan-feature` | Özellik planı yaz → task'lara böl → task-index'e işle |
| `/code-review` | Diff üzerinde ciddiyet dereceli code review |
| `/git-full` | Stage, commit, push — task durumlarını güncelle |
| `/local-testing` | Servisleri doğrula (build + health check) |
| `/release` | Validation → versiyon bump → changelog → tag → push |
| `/turn-off` | Session notu yaz, taskları işaretle, push, kapat |

---

## Spec Referansları

| Dosya | Ne |
|-------|-----|
| `PROJECT.md` | **Kaynak gerçek.** 40 bölümlük ürün + mimari spec: stack, wake word, lifecycle, memory şeması, tool/permission modeli, güvenlik, UI, fazlar. Mimari karar vermeden önce oku. |
| `TRANSCRIPT.md` | Ürünün çıkış noktası — kullanıcının gerçek problemi, niyeti ve gereksinimleri. "Neden böyle?" sorusunun cevabı burada. |
| `asuna-docs/AGENT-SPEC-ORIGINAL.md` | Orijinal coding-agent kuralları (bu CLAUDE.md'nin kaynağı). Detay/nüans gerektiğinde bak. |

Bu dosya bir **referans kartı**; spec kopyası değil. Detay için `PROJECT.md`'ye git.

---

## Mevcut Durum

**Phase 0 başlıyor** — teknik araştırma + scaffold. Template audit tamamlandı sayılır
(repoda uygulama kodu yok, app scaffold greenfield). Phase 0 çıktısı: Tauri 2 iskeleti ayakta,
boş pencere açılıyor, CI yeşil.

Task listesi ve ilerleme → `asuna-tasks/task-index.md`

> Session başında `/cold-start`, gün içinde `/status`.

---

## Mühendislik Öncelikleri

Sıra önemli — üstteki çalışmadan alttakine geçme:

1. **Çalışan iki yönlü realtime voice** — en zor etkileşim döngüsü, önce bu kanıtlanır.
2. **Güvenilir lifecycle/state yönetimi** — idle → activation → active → close geçişleri.
3. **Local wake word** — "Hey Asuna", cihaz üzerinde.
4. **Kalıcı hafıza** — session summary + durable memory, transcript dump değil.
5. **Project context** — kullanıcının hangi projede olduğunu bilmek.
6. **Bir tane güvenli computer tool** — read-only başlar.
7. **Approval/audit katmanı** — onay UI'ı + tool_events kaydı.
8. **Proaktiflik** — en son; "beni toparla" akışı (Phase 6).

Fazlar → `PROJECT.md` Bölüm 32.

---

## Mimari Sınırlar

Bu concern'ler ayrı kalır ve birbirinin içine sızmaz:

`audio` · `agent` · `memory` · `projects` · `tools` · `permissions` · `security` · `database` · `ui`

- **React componentleri doğrudan shell komutu çalıştırmaz, doğrudan DB sorgusu atmaz.**
  UI → servis → tool/registry → implementation zinciri korunur.
- **Model config merkezi.** `ASUNA_REALTIME_MODEL=gpt-realtime-2.1`
  (dev/ekonomi: `gpt-realtime-2.1-mini`). Model ID'leri asla hard-code edilmez.

### Tool kuralları

Modele açık her tool şunlara sahip olmalı: **explicit name**, dar amaç, **schema validation**,
**risk level** (0 read-only → 3 destructive), **approval policy**, **timeout**, structured result,
**audit event**. Kısıtsız shell execution yok. Read-only first.

### Güvenlik

- `.env`, SSH key, credentials, keychain, token, private cert → **bloklanır**.
- Filesystem işlemleri sadece kayıtlı project root'lar içinde; path normalize edilir,
  **traversal reddedilir**.
- Secrets gereksiz yere modele sızmaz.
- **Idle mikrofon sesi OpenAI'a gitmez**; wake word tespiti **lokal** çalışır.
- OpenAI API key asla renderer/webview bundle'ına girmez — ephemeral Realtime token'ı
  güvenilir process (Tauri Rust tarafı) üretir.

Checklist → `asuna-config/security.md`, detay → `PROJECT.md` Bölüm 19-20.

---

## Workspace

> **Hedef yapı** — Phase 0'da scaffold edilecek. Kaynak: `PROJECT.md` Bölüm 22.

```
asuna/
├── PROJECT.md / TRANSCRIPT.md / CLAUDE.md
├── .env.example
├── src/
│   ├── app/                    # uygulama giriş + routing
│   ├── components/             # UI (shell/DB çağırmaz)
│   ├── asuna/
│   │   ├── agent/              # realtime-agent, session-manager
│   │   ├── prompts/            # versiyonlu sistem prompt'ları (core.v1.ts)
│   │   ├── audio/              # wake-word-provider (adapter), sherpa-kws-provider
│   │   │                       # (motor Rust tarafında — src-tauri), audio-state
│   │   ├── memory/             # memory-service, retrieval, extraction
│   │   ├── projects/           # project-registry, project-context
│   │   ├── tools/              # registry, permissions, implementations/
│   │   ├── security/           # path sandbox, secret guard
│   │   └── observability/
│   ├── db/                     # SQLite şema + migration
│   └── shared/
├── src-tauri/                  # Tauri 2 (Rust) — ephemeral token, native köprü
├── tests/
└── scripts/
```

**Stack (PROJECT.md tercihi):** Tauri 2 + React + TypeScript (strict) + Vite, pnpm,
SQLite, OpenAI Agents SDK for TypeScript (`RealtimeAgent` / `RealtimeSession`),
WebRTC transport, wake word: sherpa-onnx KWS (Rust tarafında, `WakeWordProvider` adapter
arkasında — vendor lock yok).

> **AÇIK SORU (Phase 0):** SQLite erişim yolu — `tauri-plugin-sql` mi, Rust tarafında
> servis mi? Araştırılıp `asuna-docs/DECISIONS.md`'ye ADR olarak yazılacak.

## Temel Komutlar

> Placeholder — **Phase 0 scaffold sonrası netleşecek.**

```bash
pnpm dev            # Vite dev server (web katmanı)
pnpm tauri dev      # Tauri desktop uygulaması (asıl çalıştırma yolu)
pnpm typecheck      # tsc --noEmit
pnpm lint           # ESLint
pnpm test           # Vitest
```

---

## Code Conventions (Kısa)

- **TypeScript**: strict, `any` yasak
- **Dosya**: `kebab-case`
- **Servis sınırları**: SDK'lar wrapper arkasında (`AsunaRealtimeService`, `WakeWordProvider`);
  tool'lar `AsunaToolDefinition` (name/risk 0-3/approval/timeout)
- **Commit**: `feat(ASU-XXX): açıklama` — attribution satırı YOK
- İlk vertical slice'ı over-abstract etme; gerekçesiz bağımlılık ekleme; hatayı sessizce yutma
- Security/permission/path mantığı **test edilmeden** merge edilmez

Detaylar → `asuna-config/conventions.md`

## Kalite Kapıları (Definition of Done)

Bir task ancak şunlar geçince COMPLETED olur:

1. **Gate 1 — Statik**: typecheck + lint temiz
2. **Gate 2 — Test**: ilgili testler yeşil, yeni davranış test edildi (`asuna-config/testing.md`)
3. **Gate 3 — Review**: L task'larda ve riskli değişikliklerde `/code-review` veya reviewer agent

Detaylar → `asuna-config/workflow.md`

## Agent Orchestration

**Model politikası:**

- **Ana oturum = Fable, orchestrator.** Mimari karar, koordinasyon, paket kurulumu burada.
- **Tüm subagent'lar `model: opus`** — kodlama, araştırma, review, test dahil.

Subagent tanımları `.claude/agents/`: backend, frontend, database, devops, docs,
**researcher** (salt-okunur araştırma — SDK/API/uyumluluk doğrulama),
**reviewer** (salt-okunur review), **tester** (sadece test yazar).
Kurallar: dizin izolasyonu, paket kurulumu sadece orchestrator, paylaşılan dosyada
read-edit-retry (max 3). Detaylar → `asuna-config/agent-instructions.md`

---

## Referans Dizinleri

| Dizin | İçerik |
|-------|--------|
| `asuna-tasks/task-index.md` | Dashboard + milestones + risks + master task listesi |
| `asuna-tasks/backlog.md` | Icebox — henüz planlanmamış fikirler |
| `asuna-tasks/phases/` | Phase bazlı detaylı task açıklamaları |
| `asuna-tasks/active/session-notes.md` | Session notları |
| `asuna-tasks/templates/` | Task/phase şablonları |
| `asuna-config/workflow.md` | Workflow + DoD + kalite kapıları + release akışı |
| `asuna-config/conventions.md` | Kod standartları |
| `asuna-config/testing.md` | Test stratejisi |
| `asuna-config/security.md` | Güvenlik checklist'i |
| `asuna-config/tech-stack.md` | Teknolojiler (proje özelinde doldurulur) |
| `asuna-config/agent-instructions.md` | Orkestrasyon kuralları |
| `asuna-docs/MEMORY.md` | Kalıcı hafıza |
| `asuna-docs/DECISIONS.md` | Mimari kararlar (ADR-lite) |
| `asuna-docs/RUNBOOK.md` | Deploy / rollback / incident runbook |
| `asuna-docs/CHANGELOG.md` | Değişiklik kaydı (Keep a Changelog + semver) |
| `asuna-docs/AGENT-SPEC-ORIGINAL.md` | Orijinal coding-agent spec'i |
| `asuna-plans/` | Uygulama planları (`plan-template.md` formatında) |

---

## Hooks

| Hook | Tetikleyici | Ne yapar |
|------|------------|----------|
| `protect-files.sh` | PreToolUse (Edit/Write) | .env, lock, .git/, key/pem, credentials bloklar |
| `git-guard.sh` | PreToolUse (Bash) | `.claude/protect-main` varsa main'e commit/push bloklar |
| `post-edit-validate.sh` | (opt-in, PostToolUse) | Edit sonrası otomatik validation — `.claude/hooks/post-edit-validate.sh` içindeki yorum bloğuna bak |

---

## Hafıza Kuralları

- `asuna-docs/MEMORY.md` session başında okunur; gotcha/pattern anında güncellenir
- Mimari kararlar `asuna-docs/DECISIONS.md`'ye tarih + gerekçe + alternatiflerle
- Operasyonel bilgi (deploy, rollback, incident) `asuna-docs/RUNBOOK.md`'ye
- Kısa tut — referans kartı, roman değil

> **Not:** Bu bölüm *Claude'un* çalışma hafızasıdır. Asuna'nın **ürün** hafızası ayrı bir
> konudur (SQLite, `memories`/`sessions`/`projects` tabloları, inspectable + deletable) —
> `PROJECT.md` Bölüm 12-14.

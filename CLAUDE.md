# Asuna

## Proje

Asuna, macOS üzerinde çalışan **local-first, sesli kişisel AI companion**'dır — "Hey Asuna"
wake word'ü ile uyanır, doğal ve kesilebilir (interruptible) konuşma yürütür, kullanıcının
üzerinde çalıştığı projeyi tanır, kalıcı hafıza tutar ve onaylı yerel aksiyonları kontrollü bir
tool katmanı üzerinden çalıştırır. Amaç "AI'a gitmek" değil; AI'ın ortamda hazır beklemesi.

**Ürün döngüsü:**

```
wake → voice conversation → contextual help → memory/tool use → safe session close → idle
     ↳ metin girişi (chat shell) aynı çekirdeği kullanır: hafıza, proje bağlamı, tool + onay
```

**Prime directive: voice loop korunur.** Asuna artık ChatGPT/Claude-tarzı **kalıcı konuşma**
arayüzüdür (konuşma listesi + mesaj akışı + dosya ekleme + projede konuşma) ve ses bunun içinde
bir **voice mode**'dur — eski "generic chatbot UI kurma" yasağı ADR-008 ile kaldırıldı. Yerine
geçen kural: **`VoicePanel` asla unmount edilmez**, ses yolu hiçbir UI değişikliğinde bozulmaz;
UI durumu güvenilir göstermeye devam eder (listening / connected / speaking / tool usage /
approval / error / current project). Dashboard'a dönüşme hâlâ non-goal (R7).

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

**Phase 0 + 1 + 3 + 4 + 5 (kod) tamam; Phase 7 — Chat Shell pivotu (kod) tamam** (2026-08-31,
ADR-008). Metin konuşması kalıcı: migration 006 (`messages` / `attachments`, `sessions.title` +
`modality`), Rust `chat_send` proxy'si, ChatGPT-tarzı kabuk. Gate 3: 0 CRITICAL.
736+ Rust / 1091+ TS testi yeşil.

Sırada:

| Ne | Durum |
|----|-------|
| ASU-079 — M6 kabul testi (restart sonrası konuşma geçmişi + dosya ekleme + projede konuşma) | Kullanıcıda; **proje askıda** |
| ASU-038 / ASU-046 / ASU-055 / ASU-071 — sesli kabul testleri | Kullanıcının tek seanslık test kuyruğu |
| Phase 2 — wake word | ASU-022+ bloklu: model + ifade seçimi ADR-004'te AÇIK — kullanıcının 30 dk gerçek mikrofon testi (`spike/asu-008b-kws`) ilk adım |
| Gecikme A/B | `ASUNA_VAD_EAGERNESS=high` aktif; WARP kapalı deneme önerisi açık |

> **Zorunlu env**: `ASUNA_CHAT_MODEL` (örn. `gpt-4o-mini`) — eksikse uygulama açılışta
> `ConfigError::Missing` ile durur. `ASUNA_REALTIME_MODEL` / `ASUNA_SUMMARY_MODEL` /
> `ASUNA_EDITOR_COMMAND` ile aynı desen; kullanıcı kendi `.env`'ine ekler.

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
  (dev/ekonomi: `gpt-realtime-2.1-mini`), metin chat için `ASUNA_CHAT_MODEL`.
  Model ID'leri asla hard-code edilmez.
- **Metin chat de Rust'tan geçer.** `chat_send` (OpenAI çağrısı + mesaj yazımı tek transaction'da);
  renderer'ın mesaj yazma yolu yoktur — `message_append` bilerek ACL'e kaydedilmedi (ADR-008).

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

> Kaynak: `PROJECT.md` Bölüm 22 — scaffold tamamlandı, yapı büyük ölçüde kurulu.

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

> **KARAR (ADR-005):** SQLite erişimi yalnızca Rust'tan (`rusqlite`) — renderer SQL görmez.
> Detay: `docs/decisions/ADR-005-sqlite-access.md`.

## Temel Komutlar

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

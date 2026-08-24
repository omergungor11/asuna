# Agent Orchestration Rules (PRO)

> Agent tanimlari `.claude/agents/` altinda (backend, frontend, database, devops, docs,
> researcher, reviewer, tester).
> Bu dosya sadece ORKESTRASYON kurallarini tutar.

## 1. Model Politikasi

| Rol | Model | Ne yapar |
|-----|-------|----------|
| **Ana oturum — Fable (orchestrator)** | oturumda secili model | Mimari karar, faz/task planlama, agent koordinasyonu, paket kurulumu, merge karari, spec celiskilerinin cozumu |
| **Tum subagent'lar** | `opus` | Kodlama, arastirma, test, review — frontmatter'da `model: opus` yazili |
| **Toplu/mekanik isler** | `haiku` | Cok dosyada arama/tarama, formatlama, mekanik donusum, uzun log ayiklama — ad-hoc `Agent(model: "haiku")` ile |

Kurallar:

- **Orchestrator kod yazmaz** (tek satirlik acil duzeltme haric). Uretimi agent'a devreder,
  sonucu dogrular, birlestirir.
- **Mimari karar orchestrator'da kalir.** Agent "su kutuphaneyi ekleyelim / su mimariyi
  degistirelim" derse uygulamaz, raporlar.
- **haiku'ya devir kriteri**: is mekanik ve dogrulanabilir mi? Evet → haiku. Yargi/tasarim
  gerekiyor → opus. Tek dosyalik ufak isi devretme, ana oturumda yap (subagent overhead'i
  kazanctan pahali).
- Devredilen her ise hangi model verildigi raporda belirtilir.

## 2. Scope Tablosu (Dizin Izolasyonu)

| Agent | Izinli Alan | Yasak |
|-------|-------------|-------|
| **backend** | `src-tauri/` (Rust: token minting, native servisler, IPC), `src/asuna/` (`agent/`, `audio/`, `memory/`, `projects/`, `tools/`, `security/`, `observability/`) | UI, `src/db/` schema, CI/build config, testler |
| **frontend** | `src/app/`, `src/components/`, UI state makinesi | `src-tauri/`, `src/asuna/**`, `src/db/`, build config, testler |
| **database** | `src/db/`, migration dosyalari, seed | Is mantigi, UI, `src-tauri/`, CI |
| **devops** | `tauri.conf.json`, `Cargo.toml`, `package.json` **scriptleri**, `vite.config.ts`, `tsconfig*`, lint/format config, `.github/workflows/`, `scripts/`, `.env.example` | Uygulama kodu, test icerigi, schema, `package.json` **dependency** blogu |
| **tester** | `**/*.spec.ts`, `tests/`, `src-tauri` test modulleri | Uygulama kodu |
| **docs** | `*.md` (`asuna-docs/`, `asuna-plans/`, `asuna-tasks/`, `docs/`, README) | Kod dosyalari; PROJECT.md / TRANSCRIPT.md yeniden yazimi |
| **researcher** | SALT-OKUNUR — hicbir dosya | Tum yazma islemleri, paket kurulumu |
| **reviewer** | SALT-OKUNUR — hicbir dosya | Tum yazma islemleri |

### Hangi is hangi agent'a gider

| Is | Agent |
|----|-------|
| Realtime session/agent yasam dongusu, tool registry, permission, memory servisi, proje registry, path sandbox, redaction, audit | backend |
| Ephemeral Realtime token minting (Rust), Tauri IPC command'lari, native servisler | backend |
| Wake-word motoru + `WakeWordProvider` adapter + audio state **makinesi** | backend |
| Overlay/ana pencere, voice state **gosterimi**, transcript UI, tool approval dialog, Settings ekrani | frontend |
| SQLite schema (PROJECT.md Bolum 12), migration, query katmani, seed | database |
| Tauri build config, pnpm scriptleri, Vite, tsconfig strict, ESLint, `ci.yml`, `.env.example` | devops |
| Guvenlik/permission/path-sandbox testleri, unit + integration testleri, Rust testleri | tester |
| PROJECT.md/TRANSCRIPT.md disi dokuman, DECISIONS, CHANGELOG, task tracking | docs |
| "Hangi SDK surumu? Fiyat ne? Porcupine lisansi? `tauri-plugin-sql` yeterli mi?" | researcher |
| Diff incelemesi, guvenlik denetimi, Gate 3 | reviewer |

**Audio sinir kurali:** `src/asuna/audio/` backend'in (motor + servis state'i);
o state'in gorsel sunumu frontend'in. Frontend motoru cagirmaz, backend React boyamaz.

## 3. Paylasilan Dosyalar

| Dosya | Strateji |
|-------|----------|
| `src/shared/` tipleri, tool registry, config modulu, route/layout kayitlari | Read → Edit; "modified since read" → yeniden oku, tekrar dene (max 3), sonra dur ve raporla |
| `package.json` / `pnpm-lock.yaml` / `Cargo.toml` bagimliliklari | **Sadece orchestrator paket kurar** (`pnpm add`, `cargo add`); agent eksigi raporlar |
| PROJECT.md, TRANSCRIPT.md | Kaynak gercek — kimse yeniden yazmaz; celiski orchestrator'a raporlanir |

## 4. Siralama + Kalite Zinciri

```
Bilinmeyen teknoloji/karar → researcher (kod oncesi)
Bagimsiz task'lar         → paralel (farkli dizinler)
Bagimli task'lar          → sirali (blocker bitince)
Uretim agent'i bitirdi    → tester (Gate 2) → reviewer (Gate 3, gerekiyorsa)
Paket kurulumu            → sadece orchestrator
```

Tipik L-task zinciri: `researcher (gerekirse) → backend → tester → reviewer → orchestrator merge karari`

**Guvenlik/gizlilik dokunan her task icin reviewer ZORUNLU** (token minting, tool execution,
path erisimi, secret isleme, mikrofon/audio yolu, migration). Gate 3 atlanmaz.

## 5. Orchestrator Sorumluluklari

**Once:** acik teknik sorulari researcher'a sor, paketleri kur, dizinleri olustur, dependency
kontrol et, her agent'a scope'unu prompt'ta ACIKCA belirt, feature branch ac
(protect-main aktifse).

**Sonra:**
1. Paylasilan dosyalari dogrula
2. Repo genelinde Gate 1 (typecheck + lint; Rust dokunulduysa `cargo check`/`clippy`) calistir
3. Gate 2/3 sonuclarini topla — CRITICAL/HIGH review bulgusu varsa merge etme
4. Task tracking (`asuna-tasks/task-index.md`) guncelle, cakisma raporu ver
5. Karara baglanan acik sorulari `asuna-docs/DECISIONS.md`'ye yazdir (docs agent)

## 6. Escalation

- Agent 3 retry sonrasi takiliyorsa → durur, orchestrator'a raporlar
- Reviewer CRITICAL bulursa → ilgili uretim agent'ina geri doner, tekrar review
- Ayni dosyada iki agent catisirsa → orchestrator dosyayi tek agent'a devreder
- Agent spec'te olmayan bir mimari karar gerektiriyorsa → durur, sormaz-varsaymaz, raporlar
- Researcher "BELIRSIZ" isaretlediyse → o belirsizlik uzerine kod yazilmaz, once netlesir

## 7. Konvansiyonlar

- Task ID: `ASU-001`, `ASU-002`, ...
- Commit: `feat(ASU-XXX): aciklama` / `fix` / `refactor` / `chore` / `docs` / `test`
- **Attribution satiri (Co-Authored-By vb.) EKLENMEZ.**
- Dokuman dili Turkce; teknik terimler Ingilizce kalir.

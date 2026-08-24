# Workflow Rules (PRO)

## Task Workflow

### Pre-Task
1. `task-index.md` oku — proje durumu
2. Phase dosyasindan task detayini oku
3. Tum dependency'ler COMPLETED mi kontrol et
4. `.claude/protect-main` aktifse feature branch ac: `feat/ASU-XXX-kisa-ad`
5. Status → IN_PROGRESS yap

### During Task
- Acceptance criteria'ya sadik kal; kapsam buyuyorsa yeni task ac, mevcut task'i sisirme
- Her anlamli degisiklikten sonra Gate 1 calistir
- Task scope'unun disina cikma

### Post-Task — Definition of Done
Bir task ancak SU KAPILAR gecince COMPLETED olur:

| Gate | Icerik | Ne zaman |
|------|--------|----------|
| **Gate 1 — Statik** | typecheck + lint temiz | Her task |
| **Gate 2 — Test** | Ilgili testler yesil; yeni davranis test edildi (`testing.md`) | Her task (docs haric) |
| **Gate 3 — Review** | `/code-review` veya reviewer agent; CRITICAL/HIGH bulgu kalmadi | L task'lar, guvenlik/veri dokunan her task |

Sonra:
1. Status → REVIEW (Gate 3 bekliyorsa) veya COMPLETED
2. `task-index.md` dashboard guncelle
3. `asuna-docs/CHANGELOG.md` Unreleased bolumune madde ekle
4. Commit: `feat(ASU-XXX): title`
5. BLOCKED task'lari kontrol et, acilanlari PENDING'e cevir

## Task Durumlari

```
PENDING → IN_PROGRESS → REVIEW → COMPLETED
                      → BLOCKED
```

## Bug Workflow

1. Bug bulununca: reprodüksiyon adimini yaz, `task-index.md`'ye duzeltme task'i olarak ekle —
   ayri bir `BUG-XXX` numara alani yok, siradaki bos ID kullanilir (`ASU-064` ve sonrasi)
   (kritikse mevcut isin ONUNE gecer)
2. Once bug'i ureten test yaz (kirmizi) → duzelt (yesil) → commit `fix(ASU-XXX): ...`
3. Kok neden ilginc/tekrarlanabilirse → MEMORY.md Gotchas'a tek satir

## Commit Conventions

```
feat(ASU-XXX): description
fix(ASU-XXX): description
refactor(ASU-XXX): description
docs / chore / test(ASU-XXX): description
chore(release): vX.Y.Z
```

Attribution satiri (Co-Authored-By vb.) EKLENMEZ.

## Branch Strategy

- `main` — production-ready; `.claude/protect-main` aktifse dogrudan commit/push BLOKLU
- `feat/ASU-XXX-description` — feature branch'leri, PR ile main'e
- PR aciklamasi `.github/PULL_REQUEST_TEMPLATE.md` formatinda

## Release Akisi (semver)

- `/release` command'i kullanilir — asamalari orada
- MAJOR: breaking change | MINOR: yeni ozellik | PATCH: fix/chore
- CHANGELOG "Keep a Changelog" formatinda; Unreleased bolumu her task'ta beslenir,
  release'te versiyon basligina tasinir

## Validation Commands

```bash
pnpm typecheck      # tsc --noEmit
pnpm lint           # ESLint
pnpm test           # Vitest
pnpm build          # production build (Tauri: `pnpm tauri build`)
```

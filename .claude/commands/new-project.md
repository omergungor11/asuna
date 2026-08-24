Yeni proje kurulumu (interaktif, PRO varyanti). Template kopyalanmis durumda —
icerik SIFIRDAN uretilmez; mevcut sablon dosyalari ozellestirilir (tek kaynak ilkesi).

## Adim 1: Bilgi Topla (AskUserQuestion, TEK SEFERDE)

1. **Proje Adi** — "Projenin adi ne?"
2. **Prefix** — "Meta dizin prefix'i? (ornek: myapp → myapp-tasks/)" — lowercase kebab-case
3. **Proje Boyutu** — "Kac phase planlansin?" (5 phase: orta / 8 phase: buyuk —
   pro varyantinda 3-phase onerme, kucuk is icin lite varyanti uygundur)
4. **Calisma Modu** — "Branch stratejisi?"
   - "PR akisi (Recommended)" — main korumali, feature branch + PR (protect-main aktif edilir)
   - "Solo direct" — main'e dogrudan commit (protect-main pasif kalir)

## Adim 2: Rename + Placeholder Doldurma

1. `./setup.sh "<Proje Adi>" <prefix>` calistir (silinmisse ayni islemi elle yap:
   `asuna-tasks/ asuna-docs/ asuna-config/ asuna-plans/` → `<prefix>-*`, tum .md/.json referanslari +
   `Asuna` guncelle).
2. `CLAUDE.md` placeholder'larini doldur.
3. PR akisi secildiyse: `touch .claude/protect-main`

## Adim 3: Phase + Milestone Iskeleti

1. Secilen phase sayisina gore `<prefix>-tasks/task-index.md` dashboard'unu genislet
   (Phase 0 zaten 8 task ile hazir — CI task'i dahil).
2. Milestones tablosunu projeye gore doldur (M1 dikey dilim, M2 MVP, M3 v1.0 kalibi).
3. Risks tablosuna bilinen riskleri ekle (yoksa ornek satiri birak).

## Adim 4: Izinler

Paket yoneticisine gore `.claude/settings.local.json` permissions'a ekle:
- pnpm: `"Bash(pnpm:*)"`, `"Bash(npx:*)"` / npm: `"Bash(npm:*)"`, `"Bash(npx:*)"` / bun: `"Bash(bun:*)"`, `"Bash(bunx:*)"`

## Adim 5: Temizlik + Dogrulama

1. `rm -f setup.sh README.md`
2. `chmod +x .claude/hooks/*.sh`
3. `find . -type f -not -path './.git/*' | sort` ile yapiyi goster
4. Kapanis: "Proje yapisi hazir." + siradakiler:
   tech-stack/conventions/testing/security doldur → git init + ilk commit →
   `/cold-start` → ASU-001

## Kurallar

- HICBIR placeholder kalmasin
- Commit mesajlarina attribution satiri ekleme
- Hata olursa bildir ve dur

Release hazirla ve yayinla. Her asama bir oncekinin basarisina baglidir — hata varsa DUR.

## Asama 1: Pre-flight
1. `git status` — working tree temiz olmali (degilse once /git-full oner, dur)
2. Dogru branch'te miyiz (main veya release branch — projeye gore)
3. `git pull` — remote ile senkron ol
4. Tum validation'i calistir (typecheck + lint + test + build) — kirmizi varsa DUR, raporla

## Asama 2: Versiyon Karari (semver)
1. Son tag'i bul: `git describe --tags --abbrev=0` (yoksa v0.1.0 oner)
2. Son tag'den beri commit'leri analiz et: `git log <son-tag>..HEAD --oneline`
   - Breaking change var → MAJOR
   - feat var → MINOR
   - sadece fix/chore/docs → PATCH
3. Onerini gerekcesiyle kullaniciya sun, ONAY AL — onaysiz tag atma

## Asama 3: Changelog + Bump
1. `asuna-docs/CHANGELOG.md`: bu surumun basligini ac (`## [X.Y.Z] - TARIH`),
   son tag'den beri Added/Changed/Fixed maddelerini commit'lerden derle
2. Versiyon dosyasini guncelle (package.json / pyproject.toml — projeye gore)
3. Commit: `chore(release): vX.Y.Z`

## Asama 4: Tag + Push
1. `git tag -a vX.Y.Z -m "vX.Y.Z"`
2. `git push && git push --tags`
3. CI varsa tetiklendigini dogrula

## Asama 5: Rapor
- Surum, degisiklik ozeti, tag URL'i
- `asuna-docs/RUNBOOK.md`'de release-sonrasi adimlar tanimliysa hatirlat (deploy, monitoring)

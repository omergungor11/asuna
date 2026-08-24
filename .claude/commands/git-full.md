Tum degisiklikleri stage, commit ve push et:

1. `git status` — degisen dosyalar
2. `git diff --stat` — degisiklik ozeti
3. `asuna-tasks/task-index.md` oku — task durumu kontrol
4. Bitmis IN_PROGRESS/REVIEW task varsa:
   - task-index.md'de COMPLETED yap, dashboard sayilarini guncelle
   - `asuna-docs/CHANGELOG.md`'ye kisa kayit ekle
5. .env / credentials / key dosyalarinin stage'lenmediginden emin ol
6. Ilgili dosyalari stage et
7. Anlamli commit mesaji: task ile ilgiliyse `feat(ASU-XXX): aciklama`,
   degilse `chore/docs/fix: aciklama`
   - Co-Authored-By veya baska attribution satiri EKLEME
8. `git push`
9. Son commit'leri goster, kisa ozet ver

NOT: .env, credentials, secret iceren dosyalari ASLA commit etme.

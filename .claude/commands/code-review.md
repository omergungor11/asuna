Code review yap. Asamalar:

## Asama 1: Kapsam
1. Neyin review edilecegini belirle:
   - Argumansiz → `git diff main...HEAD` (branch'in tamami)
   - Uncommitted is → `git diff` + `git diff --staged`
   - Kullanici dosya/commit belirttiyse → onu kullan
2. `git diff --stat` ile boyutu gor; degisen her dosyayi TAM oku (sadece diff satirlari degil —
   cevresindeki baglam olmadan review yapma)

## Asama 2: Inceleme Boyutlari
Her degisiklik icin sirasiyla kontrol et:
1. **Correctness** — mantik hatasi, edge case, null/undefined, yaris kosulu, hata yonetimi
2. **Security** — `asuna-config/security.md` checklist'inden ilgili maddeler
   (input validation, authz kontrolu, secret sizintisi, injection)
3. **Conventions** — `asuna-config/conventions.md`'ye uyum (naming, API format, dosya yapisi)
4. **Test coverage** — yeni davranisin testi var mi? `asuna-config/testing.md` kriterleri
5. **Basitlik** — gereksiz karmasiklik, olu kod, tekrarlanan mantik

## Asama 3: Rapor
Bulgulari ciddiyete gore sirala:
- **CRITICAL** — production'da veri kaybi/guvenlik acigi/crash yaratir → merge engeli
- **HIGH** — yanlis davranis uretir, duzeltilmeli
- **MEDIUM** — teknik borc, bu PR'da duzeltilmesi onerilir
- **LOW** — stil/iyilestirme, opsiyonel

Her bulgu: `dosya:satir` + sorunun tek cumlelik tanimi + somut basarisizlik senaryosu.
Bulgu yoksa "temiz" de — yapay bulgu uretme.

## Asama 4: Sonuc
- Ozet: kac bulgu, merge engeli var mi
- DUZELTME YAPMA — kullanici isterse duzelt (o zaman once CRITICAL/HIGH)

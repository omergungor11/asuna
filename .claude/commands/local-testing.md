Local dev ortamini dogrula:

0. Port temizligi (ONCELIKLI) — eski process'leri kontrol et, gerekirse temizle
1. Altyapi — Docker/container durumu; kapaliysa baslat, healthy bekle
2. Veritabani — ORM client guncel mi? Pending migration? Seed data (yoksa calistir)
3. Backend — build kontrolu + health endpoint testi
4. Frontend — build kontrolu, derleme hatasi varsa bildir
5. Ozet rapor:
   - Her servis OK/FAIL
   - Erisim URL'leri
   - "Test ortami hazir." veya bulunan hatalar

NOT: Sunuculari arka planda calisir birakma — sadece build ve health check.

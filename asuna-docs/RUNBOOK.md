# Runbook

> Operasyonel bilgi — deploy, rollback, incident. CUSTOMIZE: projeye gore doldur.
> Kural: buradaki her adim KOPYALA-YAPISTIR calisir olmali; "aslinda su da gerekiyordu" yok.

## Ortamlar

| Ortam | URL | Deploy | Not |
|-------|-----|--------|-----|
| dev | localhost:[PORT] | `docker compose up` | |
| staging | | | |
| production | | | |

## Deploy

```bash
# CUSTOMIZE: adim adim deploy komutlari
```

**Deploy oncesi:** /release tamamlandi mi, migration var mi (varsa once o), CI yesil mi.
**Deploy sonrasi:** health endpoint kontrol, log'da hata taramasi (ilk 5 dk), smoke test.

## Rollback

```bash
# CUSTOMIZE: onceki surume donus komutlari (image tag / git tag bazli)
```

- Migration iceren surumde rollback karari: [ileri-uyumlu mu? down migration guvenli mi?]

## Incident

1. **Tespit** — [monitoring/alert kaynagi]
2. **Etki degerlendir** — kullanici etkileniyor mu, veri riski var mi
3. **Mudahale** — once servisi ayaga kaldir (rollback dahil), sonra kok neden
4. **Kayit** — MEMORY.md Gotchas + (buyukse) DECISIONS.md'ye onlem karari

## Izleme

| Ne | Nerede | Esik |
|----|--------|------|
| [Health, error rate, queue depth...] | | |

## Yetkiler / Erisimler

- [Deploy yetkisi kimde, secret'lar nerede yonetiliyor — deger degil, YER bilgisi]

# Task / Phase Sablonlari

## Yeni Phase Dosyasi (`asuna-tasks/phases/phase-X.md`)

```markdown
# Phase X: [Phase Adi]

## ASU-XXX: [Task Basligi]

**Agent**: backend/frontend/database/devops/docs
**Complexity**: S (1-2 saat) / M (3-5 saat) / L (1+ gun)
**Status**: PENDING
**Dependencies**: ASU-YYY

### Aciklama
[Task ne yapiyor]

### Acceptance Criteria
- [ ] [Kriter 1]
- [ ] [Kriter 2]

### Notlar
[Ipuclari, referanslar, dikkat edilecekler]
```

## Task Durumlari

```
PENDING → IN_PROGRESS → REVIEW → COMPLETED
                      → BLOCKED (bagimlilik bitmemis)
```

REVIEW opsiyoneldir — solo/hizli iste IN_PROGRESS → COMPLETED gecilebilir.
REVIEW kullanildiginda: is bitti ama dogrulama (test/kontrol) bekliyor demektir.

## Karmasiklik Olcegi

| Seviye | Anlam |
|--------|-------|
| S | Tek dosya / basit degisiklik |
| M | Birkac dosya, orta karmasiklik |
| L | Cok dosya, mimari karar gerektirir |

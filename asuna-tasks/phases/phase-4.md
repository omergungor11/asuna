# Phase 4: Project Context

> **Hedef:** Asuna hangi projede calisildigini bilsin ve bunu sadece **acik yerel baglam
> saglayicilari** uzerinden ogrensin. Ilk gercek tool (`get_current_project`, risk 0) devreye girsin.
>
> **Milestone:** M4'un ilk yarisi — "Projeleri taniyor".
>
> **Onkosul:** Phase 3 ASU-038 gecmis olmali (proje hafizasi projelere baglanacak).
>
> **Orchestrator notu (2026-08-25):** ASU-038 (M3 manuel kabul testi) hala acik. Kod ilerliyor —
> ASU-039..ASU-043 implementasyonu ASU-038 beklemeden yaziliyor cunku bu task'lar Phase 3'un
> davranisini degistirmiyor, uzerine ekliyor. **Kabul sirasi degismedi:** ASU-038 gecmeden
> ASU-046 (Phase 4 kabul testi) kapanmaz.
>
> **Ilke (PROJECT.md Bolum 15):** Tum repoyu ses oturumuna dokme. Ozetle, dumpleme.

---

## ASU-039: `projects` Tablosu + Migration

**Scope**: db | **Boyut**: S | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-030

### Acceptance Criteria
- [x] `projects`: id, name, path, description, status, primary_language, framework,
      git_remote (nullable), last_opened_at, created_at, updated_at, metadata_json
      (PROJECT.md Bolum 12.2)
- [x] `path` benzersiz (ayni proje iki kez kayitli olmuyor, normalize edilmis yol)
- [x] `memories.project_id` ve `sessions.project_id` bu tabloya foreign key ile baglanmis
      (ASU-030'da birakilan migration plani uygulanmis)
- [x] Proje silinince bagli hafizalarin ne olacagi kararlastirilmis ve migration'da uygulanmis
      (onerilen: project_id null'a duser, hafiza silinmez)
- [x] Index: path, last_opened_at, status

### Uygulama notlari
- Migration `003_projects.up.sql` / `.down.sql`; `EXPECTED_SCHEMA_VERSION = 3`.
- `projects.id` **TEXT slug** (INTEGER degil): `memories.project_id` 001'den beri metin ve
  kullanicinin verisi orada. Sayisal id'ye gecis o veriyi tasiyamazdi.
- SQLite'ta FK eklemenin tek yolu tabloyu yeniden yaratmak. Siralama FK zorlamasi **acikken**
  de guvenli olacak sekilde secildi: hicbir ebeveyn tablo, cocugu hala ona bakarken
  dusurulmuyor. Naif siralama `DROP TABLE sessions` ortuk DELETE'i ile
  `memories.source_session_id` uzerindeki `ON DELETE SET NULL`'u tetikleyip tum
  "bu neden hatirlaniyor?" baglarini silerdi.
- Devralinan serbest metin etiketler icin `status = 'unlinked'` satirlari acilir
  (`path` NULL, sema CHECK'i `unlinked <=> path IS NULL` iki yonlu zorlar). ASU-040 ayni id'li
  bir dizin kaydedildiginde bu satiri **sahiplenir** — eski hafizalar oksuz kalmaz.
- `db::project_repository::ensure_label` ayni kurali ileriye donuk uygular: `memory_create`,
  `memory_update` ve `session_start` bilinmeyen bir etiketle geldiginde etiketi NULL'a cekmez,
  ona yolsuz bir ev acar. `unlinked` satirlar hicbir dosya sistemi yetkisi tasimaz.
- Tip aynasi: `src-tauri/src/db/model.rs` (`ProjectRecord`, `ProjectStatus`) +
  `src/shared/project.ts`; `schema-mirror.spec.ts` tablo **yeniden yaratmalarini** anlayacak
  sekilde guncellendi (son `CREATE TABLE` gecerli tanimdir).

---

## ASU-040: `ProjectRegistry`

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-039

### Aciklama
Asuna sadece **kayitli** proje root'larini gorur. Diskin tamami taranmaz (PROJECT.md Bolum 4:
"full filesystem indexing" MVP disi).

### Acceptance Criteria
- [ ] Proje ekleme (dizin secici ile), listeleme, guncelleme, kaldirma
- [ ] Eklenen yol normalize ediliyor, sembolik link'ler cozuluyor
- [ ] Var olmayan / erisilemeyen yol eklenemiyor; sonradan kaybolursa durum "missing" olarak isaretleniyor
- [ ] Kayitli root'lar sonraki phase'lerdeki sandbox'in tek kaynagi (ASU-049 bunu kullanacak)
- [ ] Otomatik disk taramasi **yok** — sadece kullanicinin ekledigi projeler
- [ ] Unit test: normalizasyon, cift kayit engeli, missing durumu

---

## ASU-041: `ProjectContextService`

**Scope**: backend | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-040

### Aciklama
PROJECT.md Bolum 15. Kayitli bir projenin metadata'sini ve secili baglam dosyalarini okuyup ajana
**guvenli ve kisa** bir ozet uretir.

### Acceptance Criteria
- [ ] Baglam kaynaklari okunuyor (varsa): `PROJECT.md`, `README.md`, `CLAUDE.md`, `AGENTS.md`,
      `package.json`, `pyproject.toml`, `Cargo.toml`, `.git/config`
- [ ] Dil/framework tespiti manifest dosyalarindan yapiliyor
- [ ] Her dosya icin maksimum okuma boyutu var; buyuk dosya kirpiliyor
- [ ] Uretilen ozetin toplam boyutu sinirli ve olculuyor (repo dump'i degil)
- [ ] `.env` ve hassas dosyalar hicbir kosulda okunmuyor (bu asamada da blocklist var)
- [ ] "Guncel proje" tespiti acik ve tahmine dayali degil: kullanici secimi + `last_opened_at`;
      belirsizse Asuna soruyor, uydurmuyor
- [ ] Sonuclar kisa sureli cache'leniyor (her cagride diski yeniden taramiyor)
- [ ] Unit testler: kirpma, eksik dosya, blocklist

---

## ASU-042: Git Metadata Provider

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-041

### Acceptance Criteria
- [ ] Okunan bilgiler: guncel branch, kirli/temiz calisma agaci, degisen dosya sayisi,
      son N commit basligi, remote adi
- [ ] Sadece okuma — hicbir git yazma komutu calistirilmiyor
- [ ] Git deposu olmayan proje sorunsuz destekleniyor (bos metadata)
- [ ] Komut/erisim timeout'lu; asili kalmiyor
- [ ] Buyuk repoda makul surede donuyor
- [ ] Commit mesajlari kirpiliyor, tam diff hic okunmuyor
- [ ] Hicbir kimlik bilgisi / remote token'i cikti'ya girmiyor

---

## ASU-043: `.asuna/context.json` Okuma/Yazma

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-041

### Aciklama
PROJECT.md Bolum 16. Proje basina kompakt, makine okunur devir teslim artefakti.

### Acceptance Criteria
- [ ] Sema: projectName, objective, currentMilestone, activeTask, blockers[], recentDecisions[]
- [ ] Dosya proje kokunde `.asuna/context.json` olarak okunuyor; yoksa hata degil, bos baglam
- [ ] Bozuk/gecersiz JSON uygulamayi cokertmiyor; uyari ile yok sayiliyor
- [ ] Asuna oturum sonunda bu dosyayi guncelleyebiliyor (kararlar/aktif task) — yazma islemi
      kayitli proje root'u disina cikamiyor
- [ ] Dosya "tek gercek kaynak" olarak muamele gormuyor; DB verisiyle celisirse hangisinin
      kazandigi dokumante
- [ ] Yazma atomik (gecici dosya + rename), yarim dosya birakmiyor

---

## ASU-044: `get_current_project` Tool (Risk 0)

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-042, ASU-043, ASU-013

### Aciklama
Asuna'nin ilk gercek tool'u. PROJECT.md Bolum 17: risk 0, onay gerektirmez.
**Not:** Tam tool registry + approval katmani Phase 5'in isi (ASU-047/048). Burada tool, Realtime
oturumuna dogrudan tanimlanir; Phase 5'te registry'ye tasinir.

### Acceptance Criteria
- [ ] Tool donuyor: project id, name, path, git branch, kisa proje ozeti
- [ ] Tool cagrisi UI'da gorunuyor (`TOOL_PENDING` durumu kullaniliyor)
- [ ] Kayitli proje yoksa tool bunu acikca donuyor; Asuna proje uyduramiyor
- [ ] Cikti boyutu sinirli, sema ile dogrulanmis
- [ ] Sesli test: "Su an hangi projedeyim?" -> dogru proje ve branch
- [ ] Tool hata verirse Asuna basarili gibi konusmuyor (PROJECT.md Bolum 30)
- [ ] Phase 5'te registry'ye tasinacagi kod icinde not edilmis

---

## ASU-045: Projects UI Sekmesi

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-040, ASU-041

### Acceptance Criteria
- [ ] Kayitli projelerin listesi: ad, yol, dil/framework, son acilma
- [ ] Proje ekleme / kaldirma
- [ ] "Guncel proje" secimi ve gorunur gostergesi (overlay'de de gorunuyor)
- [ ] Secili projenin ozeti, git branch'i ve son oturum ozeti gorunuyor
- [ ] Missing (yolu kaybolmus) proje acikca isaretli
- [ ] Sekme minimal tutulmus (R7)

---

## ASU-046: Phase 4 Kabul Testi

**Scope**: test | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: ASU-039..ASU-045

### Acceptance Criteria
- [ ] En az 2 gercek proje kayit edilmis, guncel proje secilebiliyor
- [ ] "Hey Asuna, su an hangi projedeyim?" dogru cevapliyor (proje + branch)
- [ ] "Bu proje ne yapiyor?" sorusu gercek `PROJECT.md`/`README.md` icerigine dayaniyor
- [ ] Kayitli olmayan bir proje sorulunca Asuna bilmedigini soyluyor, uydurmuyor
- [ ] Phase 3 hafizasi projeye bagli calisiyor: proje X'te alinan karar, proje X baglaminda hatirlaniyor
- [ ] `.env` icerigi hicbir cikti/log/transcript'te gorunmuyor (dogrulanmis)
- [ ] Manuel senaryo `asuna-config/testing.md`'ye eklenmis

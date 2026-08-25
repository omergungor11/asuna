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

**Scope**: backend | **Boyut**: M | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-039

### Aciklama
Asuna sadece **kayitli** proje root'larini gorur. Diskin tamami taranmaz (PROJECT.md Bolum 4:
"full filesystem indexing" MVP disi).

### Acceptance Criteria
- [x] Proje ekleme (dizin secici ile), listeleme, guncelleme, kaldirma
- [x] Eklenen yol normalize ediliyor, sembolik link'ler cozuluyor
- [x] Var olmayan / erisilemeyen yol eklenemiyor; sonradan kaybolursa durum "missing" olarak isaretleniyor
- [x] Kayitli root'lar sonraki phase'lerdeki sandbox'in tek kaynagi (ASU-049 bunu kullanacak)
- [x] Otomatik disk taramasi **yok** — sadece kullanicinin ekledigi projeler
- [x] Unit test: normalizasyon, cift kayit engeli, missing durumu

### Uygulama notlari
- `src-tauri/src/projects/registry.rs` (policy) + `src-tauri/src/db/project_repository.rs` (SQL).
  Renderer sarmalayicisi: `src/asuna/projects/project-registry.ts`.
- Yol normalizasyonu `RegisteredRoot::resolve`: bos/uzunluk → `~` reddi → mutlak olma →
  `canonicalize` (symlink + `..` + var olma) → dizin olma → filesystem koku reddi → UTF-8.
  `~` **genisletilmez**: hangi home dizini oldugunu tahmin etmek olurdu.
- Cift kayit **hata degil**: `ProjectAddOutcome::AlreadyRegistered` doner. Karsilastirma
  normalize edilmis yol uzerinden, yani `.../baska/../asuna` da ayni kaydi bulur.
- `missing`: `list()` her cagride kayitli koklerin var olup olmadigini `stat` eder.
  **Bu tarama degil** — bilinmeyen hicbir yol ziyaret edilmez, dizinlerin icine girilmez.
  Kaybolan kok `missing`, geri gelen kok `active`; `archived` kullanicinin karari, dokunulmaz.
- Kaldirma: bagli hafiza/oturum varsa satir **silinmez**, etikete dusurulur
  (`ProjectRemoveOutcome::Unlinked`). Satir silinseydi FK `ON DELETE SET NULL` tum
  `project_id` degerlerini bosaltir ve "proje X'te alinan karar" baglami kaybolurdu.
- "Guncel proje" ayri bir bayrak degil, `last_opened_at`. Tek eksen: iki kaynak
  (bayrak + zaman damgasi) birbirinden kayabilirdi. `missing` ya da `unlinked` bir proje
  guncel yapilamaz — Asuna okuyamayacagi bir projeyi "su an buradayiz" diye sunmaz.
- **ASU-049 notu** `registry.rs` bas yorumunda: sandbox kok listesini yalnizca buradan alir,
  yalnizca `path`i dolu kayitlari gorur, karsilastirmayi `canonicalize` edilmis yollar
  uzerinde yapar ve bir tool kendi kokunu ekleyemez.
- Komutlar: `project_list` (okuma capability) / `project_add`, `project_remove`,
  `project_set_current` (yazma capability). Ayrim bilincli: yazma dosyasini
  `tauri.conf.json`'dan cikarmak yeni kok eklenmesini kapatir, listeyi gorunur birakir.
- `project_*` komutlari kalici depolama kapaliyken **sessizce atlamaz**, tipli
  `disabled` hatasi doner (`memory_create` ile bilerek farkli): "proje eklendi" demek
  yalan olurdu.

---

## ASU-041: `ProjectContextService`

**Scope**: backend | **Boyut**: L | **Durum**: DONE (2026-08-25) | **Bagimlilik**: ASU-040

### Aciklama
PROJECT.md Bolum 15. Kayitli bir projenin metadata'sini ve secili baglam dosyalarini okuyup ajana
**guvenli ve kisa** bir ozet uretir.

### Acceptance Criteria
- [x] Baglam kaynaklari okunuyor (varsa): `PROJECT.md`, `README.md`, `CLAUDE.md`, `AGENTS.md`,
      `package.json`, `pyproject.toml`, `Cargo.toml`, `.git/config`
- [x] Dil/framework tespiti manifest dosyalarindan yapiliyor
- [x] Her dosya icin maksimum okuma boyutu var; buyuk dosya kirpiliyor
- [x] Uretilen ozetin toplam boyutu sinirli ve olculuyor (repo dump'i degil)
- [x] `.env` ve hassas dosyalar hicbir kosulda okunmuyor (bu asamada da blocklist var)
- [x] "Guncel proje" tespiti acik ve tahmine dayali degil: kullanici secimi + `last_opened_at`;
      belirsizse Asuna soruyor, uydurmuyor
- [x] Sonuclar kisa sureli cache'leniyor (her cagride diski yeniden taramiyor)
- [x] Unit testler: kirpma, eksik dosya, blocklist

### Uygulama notlari
- `src-tauri/src/projects/context.rs` + merkezi blok listesi
  `src-tauri/src/security/blocklist.rs` (security.md Bolum 1: "tool'lar kendi kopyasini tutmaz").
- **Uc ayri tavan, ucu de olculuyor**: dosya basi okuma 32 KiB · kaynak basi alinti 1200
  karakter · toplam ozet 6000 karakter. Toplam `total_chars` / `maxChars` olarak doner ve
  acilista log'lanir. Kirpma sessiz degil — her kaynak `truncated` bayragi tasir.
- Okunacak dosyalar **sabit allowlist**; model ya da renderer dosya secemez. Her aday ayrica
  blok listesinden gecer (ikinci kapi) ve kontrol `canonicalize` **sonrasi** yapilir:
  kok icindeki `README.md -> ~/.ssh/id_ed25519` bagi da, kok disina cikan bir symlink de
  reddedilir.
- `.env` testi zorunluydu ve iki katmanda var: `blocklist.rs` (birim) +
  `context.rs` (uctan uca — `.env` icerigi serilestirilmis ozette aranmiyor).
- `.git/config` **okunur ama icerigi ozete girmez**: yalnizca remote adi turetilir,
  `@` oncesi kimlik bilgisi atilir ve sonuc `redact_sensitive_text`'ten gecer.
- Manifest'ler dumplenmez, **ozetlenir** (ad, aciklama, script adlari, ilk 12 bagimlilik adi;
  surum numaralari yok). TOML icin yeni bagimlilik eklenmedi — minimal, bilincli olarak eksik
  bir tarayici var ve bulunamayan bilgiyi uydurmuyor.
- Dil/framework catismasinda **deterministik oncelik**: Tauri kanitli `Cargo.toml` (0) →
  diger `Cargo.toml`/`pyproject.toml` (1) → `package.json` (2). Asuna gibi bir projede
  "bu bir Node projesi" demek yanlis olurdu.
- Onbellek **iki kapili**: 30 sn TTL **ve** kaynaklarin mtime+boyut parmak izi. Yalnizca
  sure eski bilgi verirdi; yalnizca parmak izi her cagride 8 `stat` demekti.
- Belirsizlik hata degil: `ProjectContext::Unknown` uc ayri nedenle doner
  (`no-registered-project` / `no-current-selection` / `root-missing`) — Asuna'nin soracagi
  soru her birinde farkli.

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

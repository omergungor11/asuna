# Phase 6: Focus Recovery (MVP)

> **Hedef:** "Asuna, beni toparla." Urunun varolus sebebi olan akis (TRANSCRIPT.md Bolum 4, Bolum 18).
> Cevap **gercek proje state'inden** gelmeli — genel motivasyon tavsiyesinden degil.
>
> **Milestone:** M5 — MVP.
>
> **Onkosul:** Phase 5 ASU-055 gecmis olmali. Bu akis hafiza + proje baglami + tool'lari ayni anda
> kullanir; hepsi calisiyor olmadan yazilamaz.
>
> **Phase cikisi:** ASU-062 — PROJECT.md Bolum 33'teki 21 maddelik MVP kabul checklist'i.

---

## ASU-056: `tasks` Tablosu + `TaskService`

**Scope**: db | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-039

### Acceptance Criteria
- [ ] `tasks`: id, project_id (nullable), title, description, status, priority, source,
      created_at, updated_at, completed_at (nullable) (PROJECT.md Bolum 12.2)
- [ ] `source` alani task'in nereden geldigini ayirt ediyor (kullanici sozlu / hafiza cikarimi / manuel)
- [ ] `TaskService`: olustur, listele (proje + duruma gore), guncelle, tamamla
- [ ] "Aktif task" tanimi acik ve tek: hangi kriterle secildigi dokumante
- [ ] `SessionBootstrapContext.activeTasks` (ASU-035'te bos birakilmisti) artik doluyor
- [ ] Unit testler: siralama, filtre, tamamlama

### Notlar
Bu bir todo uygulamasi degil (TRANSCRIPT.md Bolum 19: "another giant todo application" reddedildi).
Task tablosu sadece "beni toparla" akisini beslemek icin var. UI'si minimum.

---

## ASU-057: Aktif Task + Blocker Retrieval

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-056, ASU-035

### Aciklama
TRANSCRIPT.md Bolum 18 adim 2-4: son oturum ozeti, son aktif task, son blocker/kararlar.

### Acceptance Criteria
- [ ] Verilen bir proje icin donuyor: son oturum ozeti, aktif task, acik blocker'lar, son kararlar
- [ ] Blocker kaynaklari: `memories` (kind=decision/task), `.asuna/context.json` blockers[], acik task'lar
- [ ] Celisen kaynaklar varsa oncelik sirasi tanimli ve dokumante
- [ ] Hicbir veri yoksa acikca "bilgi yok" donuyor — bos string veya uydurma degil
- [ ] Cikti boyutu sinirli
- [ ] Unit testler: bos durum, kismi veri, celiski cozumu

---

## ASU-058: `FocusRecoveryService`

**Scope**: backend | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-057, ASU-047

### Aciklama
TRANSCRIPT.md Bolum 18'deki 7 adimli akisi tek bir serviste orkestre et:
1. proje tespiti -> 2. son oturum ozeti -> 3. aktif task -> 4. blocker/kararlar ->
5. durumu kisaca soyle -> 6. **TEK** somut sonraki adim -> 7. guvenli tool teklifi.

### Acceptance Criteria
- [ ] Servis yapili bir `FocusRecoveryResult` donuyor: projectState, whereThingsStand,
      singleNextAction, suggestedTool (opsiyonel)
- [ ] **Tam olarak bir** sonraki adim uretiliyor — liste degil (PROJECT.md Bolum 5.5)
- [ ] Onerilen tool her zaman risk 0 veya risk 1; risk 2/3 tool onerilmiyor
- [ ] Tool otomatik calismiyor — teklif ediliyor, kullanici onaylayinca calisiyor
- [ ] Guncel proje bilinmiyorsa Asuna once projeyi soruyor, tahmin etmiyor
- [ ] Hicbir baglam yoksa durust cevap veriyor: veri olmadigini soyluyor ve ne kaydedilmesi
      gerektigini oneriyor
- [ ] Cevap kisa — TRANSCRIPT.md Bolum 18'deki ornek uzunlugunda
- [ ] Unit testler: tam veri, kismi veri, hic veri, bilinmeyen proje

### Notlar
Ornek hedef cikti (TRANSCRIPT.md Bolum 18):
> "Asuna projesindeydin. Son hedefimiz Realtime baglantisini calistirmakti; wake word'u henuz
> baglamadik. Su an tek isimiz ses oturumunu basariyla acmak. Istersen mevcut Realtime dosyalarini
> okuyup baglanti hatasini bulayim."

---

## ASU-059: "Beni Toparla" Intent Tanima + Prompt Entegrasyonu

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-058, ASU-012

### Acceptance Criteria
- [ ] Akis bir tool/fonksiyon olarak ajana aciliyor (`get_focus_recovery`, risk 0)
- [ ] Turkce varyasyonlar calisiyor: "beni toparla", "dagildim", "nerede kalmistik",
      "bugun neye odaklanayim", "bu projede nerede tikandik"
- [ ] Prompt (ASU-012) Asuna'ya bu akisi ne zaman kullanacagini anlatiyor
- [ ] Ingilizce esdegerleri de calisiyor ("get me back on track")
- [ ] Yanlis tetiklenme dusuk: normal sohbette rastgele devreye girmiyor
- [ ] Asuna sonucu **oldugu gibi** aktariyor; servis "veri yok" derken hikaye anlatmiyor
- [ ] Sesli test: 5 farkli Turkce ifade ile dogru tetikleniyor

---

## ASU-060: Focus Recovery UI

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-058

### Acceptance Criteria
- [ ] Akis calistiginda gorunur bir kart: proje, durum ozeti, **tek** sonraki adim, teklif edilen tool
- [ ] Teklif edilen tool tek tiklamayla onaylanabiliyor (ASU-053 onay akisina baglaniyor)
- [ ] Sonraki adim task olarak kaydedilebiliyor (ASU-056)
- [ ] Kart overlay modda da gorunuyor
- [ ] Veri yoksa kart bunu durustce gosteriyor, bos sablon gostermiyor
- [ ] Kart tek bir aksiyona odakli — coklu oneri listesi yok (R7)

---

## ASU-061: Halusinasyon Korumasi

**Scope**: test | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-058, ASU-059

### Aciklama
PROJECT.md Bolum 39/9-10: "Never pretend a tool succeeded. Never pretend context exists when it was
not retrieved." Bu, urunun guvenilirliginin tek maddede ozeti.

### Acceptance Criteria
- [ ] Bos DB + kayitli proje yok senaryosu: "beni toparla" uydurma proje/task uretmiyor
- [ ] Silinmis hafiza sorulunca Asuna hatirladigini iddia etmiyor
- [ ] Tool hata verdiginde Asuna basarili gibi konusmuyor (ASU-052 editor hatasi senaryosu tekrar)
- [ ] Okunmamis dosya hakkinda icerik iddia etmiyor
- [ ] Reddedilen tool sonrasi "yaptim" demiyor
- [ ] Bu senaryolar tekrar edilebilir manuel test seti olarak `asuna-config/testing.md`'de
- [ ] Bulunan her halusinasyon vakasi prompt (ASU-012) veya servis sinirlariyla kapatilmis;
      "model daha dikkatli olsun" seklinde birakilmamis

---

## ASU-062: M5 / MVP Kabul Checklist (PROJECT.md Bolum 33)

**Scope**: test | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-001..ASU-061

### Aciklama
21 maddelik resmi MVP kabul listesi. Hepsi gecmeden MVP tamamlanmis sayilmaz.

### Acceptance Criteria
- [ ] Mevcut template denetlendi (Phase 0'da tamamlandi sayildi — gerekce kayitli)
- [ ] Uygulama macOS'te aciliyor
- [ ] API key renderer bundle'inda yok (build ciktisinda grep ile dogrulanmis)
- [ ] Realtime oturumu gecici client credential kullaniyor
- [ ] `gpt-realtime-2.1` konfigurabilir
- [ ] Iki yonlu ses calisiyor
- [ ] Kullanici Asuna'nin sozunu kesebiliyor
- [ ] Canli durum gorunuyor
- [ ] Yerel wake word "Hey Asuna"yi algiliyor
- [ ] Idle ses buluta gonderilmiyor
- [ ] Oturum timeout ile kapaniyor
- [ ] SQLite kaliciligi calisiyor
- [ ] En az bir kalici hafiza restart'tan sagliyor
- [ ] Guncel proje tespit edilebiliyor
- [ ] En az bir gercek tool calisiyor
- [ ] Tool calistirmasi UI'da gorunuyor
- [ ] Mutasyon yapan aksiyonlar onay gerektiriyor
- [ ] Hatalar durustce yuzeye cikiyor
- [ ] Oturum ozeti olusuyor
- [ ] Kullanici hafizayi inceleyip silebiliyor
- [ ] README lokal calistirma talimatlarini iceriyor
- [ ] **Uctan uca demo:** "Hey Asuna" -> Turkce konusma -> "beni toparla" -> gercek proje durumu ->
      tek sonraki adim -> onayli tool -> oturum kapanisi -> hafiza yazimi

### Notlar
Gecmeyen madde varsa bir duzeltme task'i acilir (`ASU-064+`), checklist yeniden calistirilir.
Kismi gecis MVP sayilmaz.

---

## ASU-063: README + RUNBOOK + v0.1.0 Release

**Scope**: docs | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-062

### Acceptance Criteria
- [ ] `README.md`: ne oldugu, kurulum, calistirma, gerekli harici setup (OpenAI billing,
      Picovoice AccessKey, macOS mikrofon izni)
- [ ] `asuna-docs/RUNBOOK.md`: sik karsilasilan sorunlar (mikrofon izni, token hatasi, wake word
      algilamiyor, DB migration hatasi) ve cozumleri
- [ ] `asuna-docs/DECISIONS.md` tum ADR'lerle guncel
- [ ] `asuna-docs/CHANGELOG.md` v0.1.0 girdisi
- [ ] `docs/architecture/*` (voice, memory, tools, security) gercek uygulamayi yansitiyor —
      Phase 0'daki TODO'lar kapanmis
- [ ] `asuna-tasks/backlog.md` Phase 0-6 boyunca ertelenen her sey ile guncellenmis
- [ ] v0.1.0 tag'i atilmis, lokal .app build'i calisiyor
- [ ] Bir sonraki adim tek cumleyle yazilmis: MVP'yi gunluk kullanmaya baslamak
      (PROJECT.md Bolum 38: "Does the user voluntarily call Asuna during real work?")

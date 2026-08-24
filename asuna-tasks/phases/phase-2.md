# Phase 2: Wake Word

> **Hedef:** "Talk to Asuna" butonunun yerini **"Hey Asuna"** alsin. Idle'dayken hicbir ses buluta
> gitmesin. Oturum kendiliginden kapansin.
>
> **Milestone:** M2 — "'Hey Asuna' ile uyaniyor".
>
> **Onkosul:** Phase 1 ASU-020 gecmis olmali. Ses dongusu calismadan wake word eklenmez
> (TRANSCRIPT.md Bolum 20).
>
> **Phase cikisi:** ASU-028. Gizlilik dogrulamasi (ASU-024) bu phase'in pazarlik disi maddesidir.

---

## ASU-021: `WakeWordProvider` Interface + Fake Provider

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-020

### Aciklama
PROJECT.md Bolum 8'deki adapter interface'i once tanimla, sonra vendor bagla. Boylece Phase 2'nin
geri kalani Porcupine'in hazir olmasini beklemez ve testler vendor'suz calisir.

### Acceptance Criteria
- [ ] Interface tanimli: `initialize()`, `start()`, `stop()`, `onDetected(cb): () => void`
- [ ] `WakeWordEvent` tipi: zaman damgasi, guven skoru, hangi keyword
- [ ] `FakeWakeWordProvider` — test/dev icin manuel tetiklenebilir (dev panelinden veya kisayoldan)
- [ ] Provider secimi config'ten (`ASUNA_WAKE_WORD_PROVIDER`) geliyor
- [ ] Uygulamanin geri kalani somut provider tipini import etmiyor (yalnizca interface)
- [ ] Unit test: fake provider ile detection -> callback zinciri

---

## ASU-022: Porcupine Provider ("Hey Asuna" Custom Keyword)

**Scope**: backend | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-021, ASU-008

### Aciklama
ASU-008 arastirmasinin karari uygulanir. Yerel, surekli calisan "Hey Asuna" algilamasi.

### Acceptance Criteria
- [ ] "Hey Asuna" custom `.ppn` modeli uretilmis ve repoya/asset dizinine yerlesmis (lisans notuyla)
- [ ] Picovoice AccessKey config'ten okunuyor, koda gomulu degil, log'a dusmuyor
- [ ] Apple Silicon macOS'te calisiyor
- [ ] Detection hassasiyeti (sensitivity) konfigurabilir
- [ ] Yanlis pozitif orani makul: 10 dakikalik normal konusma/muzik ortaminda kabul edilebilir sinirda
      (olculmus ve not edilmis)
- [ ] Detection gecikmesi olculmus (soz bitimi -> callback)
- [ ] Mikrofon baska bir uygulama tarafindan kullaniliyorsa uygulama cokmuyor, durumu gosteriyor
- [ ] Idle CPU kullanimi olculup not edilmis

### Notlar
TRANSCRIPT.md Bolum 5: kullanici sarki soyleyebilir, alakasiz sesler cikarabilir — bunlar istek
olarak islenmemeli. Yanlis pozitif burada sadece bir bug degil, urun guveni meselesi.

---

## ASU-023: IDLE_WAKE_WORD -> WAKING -> CONNECTING Gecisi

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-022, ASU-014

### Aciklama
PROJECT.md Bolum 9.2'deki aktivasyon akisini state machine'e bagla.

### Acceptance Criteria
- [ ] Uygulama acilisinda varsayilan durum `IDLE_WAKE_WORD` (Realtime oturumu **kapali**)
- [ ] Detection -> `WAKING` -> token mint -> `CONNECTING` -> `LISTENING` akisi calisiyor
- [ ] Wake anindan sonra wake-word motoru duraklatiliyor (aktif oturum sirasinda dinlemiyor)
- [ ] Opsiyonel kisa aktivasyon tonu calabiliyor (konfigurabilir)
- [ ] Aktivasyon sonrasi Asuna kisa karsilik veriyor, uzun selam vermiyor
- [ ] Baglanti kurulamazsa temiz sekilde `IDLE_WAKE_WORD`'e geri donuluyor
- [ ] ASU-015 butonu dev/fallback yolu olarak kaliyor ama birincil akis wake word

---

## ASU-024: Idle'da Buluta Ses Gitmedigi Dogrulamasi

**Scope**: test | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-023

### Aciklama
Urunun gizlilik sozunun kanit gerektiren maddesi (PROJECT.md Bolum 20, R9). "Gonderiyoruz sanmiyorum"
kabul edilmez — olculur.

### Acceptance Criteria
- [ ] Idle durumda 5 dakika boyunca OpenAI'ye giden hicbir ag trafigi olmadigi gozlemlenmis
      (ag izleme araciyla, kanit ekran goruntusu/log ile)
- [ ] Idle durumda aktif `RTCPeerConnection` bulunmadigi kod/test ile dogrulanmis
- [ ] Idle ses karesi diske yazilmiyor (dosya sistemi izlemesiyle dogrulanmis)
- [ ] Wake word tetiklenmeden token mint istegi atilmadigi dogrulanmis
- [ ] Otomatik regresyon testi: `IDLE_WAKE_WORD` durumundayken transport `null`/kapali
- [ ] Bulgular `docs/architecture/security.md`'ye "Privacy guarantees" bolumu olarak yazilmis

---

## ASU-025: Inactivity Timeout + Max Session Duration

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-023

### Aciklama
Maliyet kontrolunun (R1) ve gizliligin ortak gerekliligi. Acik unutulmus bir oturum olmamali.

### Acceptance Criteria
- [ ] `ASUNA_IDLE_TIMEOUT_SECONDS` (varsayilan 45) sonrasinda oturum otomatik kapaniyor
- [ ] Maksimum oturum suresi siniri var ve konfigurabilir
- [ ] Timeout'a yaklasirken UI'da gorunur bir gosterge var (sessizce kesilmiyor)
- [ ] Kullanici veya Asuna konustukca sayac sifirlaniyor
- [ ] Timeout kapanisi ASU-026 session close akisini tetikliyor
- [ ] Unit test: sahte zamanlayici ile timeout ve reset davranisi

---

## ASU-026: Session Close Akisi

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-025, ASU-018

### Aciklama
PROJECT.md Bolum 9.4. Phase 2'de kapanisin **1-2. adimlari** (ses durdur, oturum kapat) ve wake-word'e
donus uygulanir; ozet/memory adimlari Phase 3'te bu akisa takilir.

### Acceptance Criteria
- [ ] Kapanis tetikleyicileri calisiyor: sesli ifade ("Tamam Asuna", "Kapat", "Sonra devam ederiz"),
      inactivity timeout, UI stop butonu, kurtarilamaz ag hatasi
- [ ] Kapanista bulut ses akisi duruyor, oturum disconnect ediliyor
- [ ] Wake-word motoru yeniden basliyor, durum `IDLE_WAKE_WORD`
- [ ] Kapanis akisi genisletilebilir adim listesi olarak yazilmis (Phase 3'te ozet + memory eklenecek)
- [ ] Yanlis pozitif kapanis riski dusuk: "kapat" kelimesi cumle icinde gecerse hemen kapatmiyor
      (kararin nasil verildigi dokumante)
- [ ] Kapanis sonrasi hemen "Hey Asuna" ile tekrar uyanabiliyor (race yok)

---

## ASU-027: Minimal Idle Overlay / Tray Gostergesi

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-023

### Aciklama
PROJECT.md Bolum 21 "Minimal overlay". Idle'da urun gorunmez olmamali ama ekrani da isgal etmemeli.

### Acceptance Criteria
- [ ] Tray ikonu / kucuk overlay uygulamanin dinleme durumunu gosteriyor
- [ ] Aktif dinleme durumu **acikca gorunur** (gizli kayit hissi vermiyor)
- [ ] Overlay'de en az: durum, mikrofon durumu, kisa transcript, stop butonu
- [ ] Ana pencere kapaliyken de wake word calismaya devam ediyor
- [ ] Tray'den cikis uygulamayi ve wake-word motorunu temiz kapatiyor
- [ ] Buyuk dashboard yapilmamis (R7) — bu ekran bilerek kucuk

---

## ASU-028: M2 Kabul Testi

**Scope**: test | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: ASU-021..ASU-027

### Acceptance Criteria
- [ ] Uygulama acik, ana pencere kapali; "Hey Asuna" denince oturum aciliyor
- [ ] Konusma Phase 1'deki kalitede calisiyor (regresyon yok)
- [ ] Konusmadan 45 saniye beklenince oturum otomatik kapaniyor
- [ ] "Tamam Asuna" denince oturum kapaniyor
- [ ] Kapanistan sonra tekrar "Hey Asuna" ile uyaniyor
- [ ] ASU-024 gizlilik dogrulamasi tekrar edilip geciyor
- [ ] Manuel test senaryosu `asuna-config/testing.md`'ye eklenmis
- [ ] Yanlis pozitif ve gecikme olcumleri kaydedilmis (R2/R8 takibi)

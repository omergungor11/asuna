# Phase 1: Realtime Voice (Dikey Dilim)

> **Hedef:** En zor etkilesim dongusunu wake word'den **once** kanitla. Butona bas, Turkce konus,
> Asuna dusuk gecikmeyle cevaplasin, sozunu kesebil, transcript'i gor, temiz kapat.
>
> **Milestone:** M1 — "Sesli konusma calisiyor".
>
> **Phase cikisi:** ASU-020 kabul testi (PROJECT.md Bolum 35, 8 madde) tam gecmis olmali.
> PROJECT.md Bolum 35: "Do not block the voice proof-of-concept on perfect memory or perfect
> desktop automation." Bu phase'de memory yok, tool yok, wake word yok.

---

## ASU-011: Ephemeral Realtime Token Minting (Rust)

**Scope**: backend | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-006, ASU-009

### Aciklama
Kalici `OPENAI_API_KEY` yalnizca Tauri'nin Rust tarafinda kalir. Renderer, kisa omurlu bir Realtime
client secret icin Rust'a bir command cagirir; anahtarin kendisi asla webview'a gecmez.
(PROJECT.md Bolum 7 — Authentication)

### Acceptance Criteria
- [ ] Rust tarafinda `mint_realtime_token` Tauri command'i; API key'i env/keychain'den okuyor
- [ ] OpenAI'ye ephemeral client secret istegi atiliyor, sadece token + expiry donuyor
- [ ] Donen payload'da kalici API key veya org bilgisi **yok**
- [ ] Token'in son kullanma zamani frontend'e donuyor; suresi dolmus token yeniden isteniyor
- [ ] Hata durumlari ayirt ediliyor: key yok / gecersiz key / kota yok / ag hatasi — her biri
      kullaniciya farkli ve durust mesaj uretiyor (PROJECT.md Bolum 30)
- [ ] API key hicbir log satirinda gorunmuyor (redaction testi var)
- [ ] Unit test: mint hatasi durumunda command panic etmiyor, tipli hata donuyor

### Notlar
MVP kabul checklist'inde iki madde bu task'a bagli: "API key never shipped in renderer bundle" ve
"Realtime session uses temporary client credential".

---

## ASU-012: Asuna Core Prompt / Instructions Dosyasi

**Scope**: backend | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: ASU-002

### Aciklama
PROJECT.md Bolum 10 (kimlik) + Bolum 11 (system prompt gereksinimleri) surumlenmis tek bir dosyada.

### Acceptance Criteria
- [ ] `src/asuna/prompts/core.v1.ts` olusturulmus (versiyonlu prompt dosyasi — `conventions.md` "Prompt Dosyalari")
- [ ] Prompt PROJECT.md Bolum 11'deki tum ilkeleri iceriyor: uydurmama, tek somut sonraki adim,
      memory'yi durust kullanma, tool risk politikasi, Turkce + Ingilizce teknik terim karisimi
- [ ] Prompt **statik ilkeler** iceriyor; degisken veri (memory, proje) sonraki phase'lerde
      `buildAsunaInstructions(context)` ile enjekte edilecek sekilde ayrilmis
- [ ] Prompt kod icine gomulu string olarak dagitilmamis — tek kaynaktan okunuyor
- [ ] Prompt uzunlugu makul (PROJECT.md Bolum 39: "Avoid giant prompts")

---

## ASU-013: `AsunaRealtimeService` (SDK Wrapper)

**Scope**: backend | **Boyut**: L | **Durum**: PENDING | **Bagimlilik**: ASU-011, ASU-012

### Aciklama
OpenAI Agents SDK detaylarini tek bir servis arkasina kapat. SDK API'si degistiginde sadece bu dosya
degissin (PROJECT.md Bolum 24, Bolum 39/13).

### Acceptance Criteria
- [ ] `AsunaRealtimeService` API'si: `connect()`, `disconnect()`, `interrupt()`, event subscription
- [ ] `RealtimeAgent` + `RealtimeSession` yalnizca bu dosyada import ediliyor (lint kurali veya
      testle dogrulanmis)
- [ ] Model ID `ASUNA_REALTIME_MODEL` config'inden geliyor, hard-code yok
- [ ] Ephemeral token ASU-011 command'i uzerinden aliniyor
- [ ] Baglanti event'leri normalize edilmis Asuna event'lerine ceviriliyor (SDK tipi disari sizmiyor)
- [ ] Yeniden baglanma denemesi sinirli ve gorunur (sonsuz retry yok)
- [ ] Unit test: mock transport ile connect/disconnect yasam dongusu

### Notlar
Bu servis Phase 5'te tool tanimlarini da alacak; API'yi simdiden `tools?: AsunaToolDefinition[]`
alacak sekilde tasarla ama Phase 1'de bos gec. Fazla soyutlama yapma (PROJECT.md Bolum 39/16).

---

## ASU-014: Voice State Machine

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-013

### Aciklama
PROJECT.md Bolum 7'deki durum listesi icin acik bir state machine. UI bunun turevi olacak, tersi degil.

### Acceptance Criteria
- [ ] Durumlar tanimli: `BOOTING`, `IDLE_WAKE_WORD`, `WAKING`, `CONNECTING`, `LISTENING`,
      `USER_SPEAKING`, `ASSISTANT_THINKING`, `ASSISTANT_SPEAKING`, `TOOL_PENDING`,
      `AWAITING_APPROVAL`, `ERROR`
- [ ] Gecerli gecisler acikca tanimli; gecersiz gecis sessizce yutulmuyor (hata/log)
- [ ] Phase 1'de kullanilmayan durumlar (`IDLE_WAKE_WORD`, `TOOL_PENDING`, `AWAITING_APPROVAL`)
      tanimli ama erisilmez — sonraki phase'ler icin yer tutuyor
- [ ] Unit test: her gecerli gecis + en az 3 gecersiz gecis reddi
- [ ] State degisimleri tek bir yerden yayinlaniyor (ASU-019 loglamasi buna baglanacak)

---

## ASU-015: "Talk to Asuna" Gecici Butonu + Baglanti Akisi

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-014

### Aciklama
Wake word'un yerini gecici olarak tutan manuel aktivasyon. Phase 2'de kaldirilacak/ikincil hale
gelecek (TRANSCRIPT.md Bolum 20).

### Acceptance Criteria
- [ ] Tek buton: bagli degilken "Talk to Asuna", bagliyken "Stop"
- [ ] Tiklama akisi: mikrofon izni -> token mint -> `connect()` -> `LISTENING`
- [ ] Mevcut durum UI'da her an gorunur (durum rozeti + mikrofon gostergesi)
- [ ] Mikrofon izni reddedilirse net kurulum yonlendirmesi gosteriliyor (PROJECT.md Bolum 30)
- [ ] Cift tiklama / hizli tiklama yaris kosulu yaratmiyor (buton islem sirasinda kilitli)
- [ ] Kod `// TEMPORARY: ASU-023 wake word ile degistirilecek` notu iceriyor

### Notlar
Bu butonu guzellestirme. UI Phase 1'de guven ve gorunurluk icin var, urun degil
(PROJECT.md Bolum 21: "The desktop UI should not become the main product").

---

## ASU-016: Iki Yonlu Ses + Interruption (Barge-in)

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-015

### Aciklama
Phase 1'in kalbi. Kullanici Turkce konussun, Asuna dusuk gecikmeyle cevaplasin, konusurken sozu
kesilebilsin.

### Acceptance Criteria
- [ ] Mikrofon girisi Realtime oturumuna akiyor, Asuna'nin sesi hoparlore cikiyor
- [ ] Turkce konusma dogru anlasiliyor (manuel dogrulama, en az 5 farkli cumle)
- [ ] Kullanici Asuna konusurken konusmaya baslayinca Asuna susuyor (barge-in)
- [ ] Kesme sonrasi Asuna eski cevabina kaldigi yerden devam etmiyor; yeni girdiye cevap veriyor
- [ ] Asuna'nin kendi sesi mikrofondan geri beslenip kendini kesmiyor (echo/self-interrupt kontrolu)
- [ ] Algilanan gecikme kabul edilebilir; cumle sonu -> ilk ses arasi olculup not edilmis
- [ ] Asuna aktivasyon cevabi kisa (PROJECT.md Bolum 9.2: "Buradayim." / "Dinliyorum.")

### Notlar
Self-interrupt (Asuna kendi sesiyle kendini kesmesi) bu asamada en yaygin tuzak. Echo cancellation
ayarlarini (`echoCancellation`, `noiseSuppression`) getUserMedia constraint'lerinde acikca ayarla.

---

## ASU-017: Canli Transcript UI

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-016

### Acceptance Criteria
- [ ] Kullanici ve Asuna repliklerini ayirt eden akan transcript listesi
- [ ] Kismi (partial) transcript'ler konusma sirasinda gorunuyor, bitince kesinlesiyor
- [ ] Kesilen (interrupted) Asuna cevabi transcript'te kesildigi yerde isaretleniyor
- [ ] Otomatik en alta kaydirma; kullanici yukari kaydirdiysa zorla asagi atmiyor
- [ ] Transcript Phase 1'de **sadece bellekte** — disk yazimi Phase 3'un konusu (ASU-032)
- [ ] Uzun oturumda transcript listesi UI'yi kilitlemiyor

---

## ASU-018: Temiz Disconnect + Kaynak Temizligi

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-016

### Aciklama
Kotu kapanma bu urunde maliyet demek — acik kalan bir oturum fatura yazar (R1).

### Acceptance Criteria
- [ ] Stop butonu oturumu kapatiyor; durum `IDLE`/`BOOTING`'e donuyor
- [ ] Mikrofon track'leri `stop()` ediliyor — macOS mikrofon gostergesi soneyor
- [ ] `RTCPeerConnection` kapatiliyor, event listener'lar temizleniyor
- [ ] Pencere kapatilirken / uygulama cikarken oturum kapatiliyor (leak yok)
- [ ] Ag kopmasi durumunda oturum otomatik temizleniyor ve UI `ERROR` gosteriyor
- [ ] Ard arda 5 kez baglan/kes yapildiginda leak yok (bellek + acik baglanti sayisi kontrol edilmis)

---

## ASU-019: Hata Yonetimi + Observability

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-014

### Aciklama
PROJECT.md Bolum 29 (state transition log) + Bolum 30 (durust hata yonetimi).

### Acceptance Criteria
- [ ] State gecisleri zaman damgasiyla loglaniyor (Bolum 29'daki ornek formata yakin)
- [ ] Dev modda gorunur bir debug konsolu / log paneli var
- [ ] `ASUNA_LOG_LEVEL` config'i etkili
- [ ] Hicbir log satirinda API key, token veya ham secret yok — redaction testi mevcut
- [ ] Kullaniciya gosterilen hata mesajlari durust: baglanti yoksa "Su an ses baglantisini kuramadim"
      diyor, basarili gibi davranmiyor
- [ ] Beklenmeyen hata UI'yi cokertmiyor; `ERROR` durumundan tekrar baglanma yolu var

---

## ASU-020: M1 Kabul Testi (PROJECT.md Bolum 35)

**Scope**: test | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-011..ASU-019

### Aciklama
Phase 1'in "done" tanimi PROJECT.md Bolum 35'teki 8 maddedir. Hepsi gecmeden Phase 2'ye gecilmez.

### Acceptance Criteria
- [ ] 1. Uygulama calisiyor (`pnpm tauri dev` ve build edilmis .app)
- [ ] 2. "Talk to Asuna" butonuna tiklaniyor
- [ ] 3. Realtime ses baglantisi kuruluyor
- [ ] 4. Kullanici Turkce konusuyor ve anlasiliyor
- [ ] 5. Asuna dusuk gecikmeyle cevap veriyor
- [ ] 6. Kesme (interruption) calisiyor
- [ ] 7. Transcript gorunuyor
- [ ] 8. Disconnect temiz calisiyor
- [ ] Manuel test senaryosu `asuna-config/testing.md`'ye yazilmis (tekrar edilebilir olsun)
- [ ] Otomatik testler yesil: state machine, token mint hata yollari, log redaction
- [ ] Bir oturumun yaklasik maliyeti olculup not edilmis (R1 icin taban cizgi)

### Notlar
Bu test **manuel + Turkce** yapilir. Ingilizce ile gecip Turkce ile takilirsa milestone gecmemis
sayilir (R8).

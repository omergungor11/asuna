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

**Scope**: backend | **Boyut**: L | **Durum**: COMPLETED | **Bagimlilik**: ASU-006, ASU-009

### Aciklama
Kalici `OPENAI_API_KEY` yalnizca Tauri'nin Rust tarafinda kalir. Renderer, kisa omurlu bir Realtime
client secret icin Rust'a bir command cagirir; anahtarin kendisi asla webview'a gecmez.
(PROJECT.md Bolum 7 — Authentication)

### Acceptance Criteria
- [x] Rust tarafinda `mint_realtime_token` Tauri command'i; API key'i env/keychain'den okuyor
- [x] OpenAI'ye ephemeral client secret istegi atiliyor, sadece token + expiry donuyor
- [x] Donen payload'da kalici API key veya org bilgisi **yok**
- [x] Token'in son kullanma zamani frontend'e donuyor; suresi dolmus token yeniden isteniyor
- [x] Hata durumlari ayirt ediliyor: key yok / gecersiz key / kota yok / ag hatasi — her biri
      kullaniciya farkli ve durust mesaj uretiyor (PROJECT.md Bolum 30)
- [x] API key hicbir log satirinda gorunmuyor (redaction testi var)
- [x] Unit test: mint hatasi durumunda command panic etmiyor, tipli hata donuyor

### Notlar
MVP kabul checklist'inde iki madde bu task'a bagli: "API key never shipped in renderer bundle" ve
"Realtime session uses temporary client credential".

### Uygulama (tamamlandi)
- `src-tauri/src/realtime_token.rs` — `RealtimeTokenService` + `mint_realtime_token` komutu.
  Donus tipi `EphemeralToken` → JSON `{ value, expiresAt, model }` (camelCase). `Debug`
  implementasyonu elle yazildi: token degeri log'a basilamaz.
- Endpoint `POST https://api.openai.com/v1/realtime/client_secrets`, payload
  `{ expires_after: { anchor: "created_at", seconds: 600 }, session: { type: "realtime", model } }`.
  Model `ASUNA_REALTIME_MODEL` config'inden gelir; renderer model/TTL secemez.
- Tipli hata varyantlari (`RealtimeTokenError`, IPC bicimi `{ kind, message }`):
  `missing_api_key` · `invalid_api_key` (401) · `model_access_denied` (403/404) ·
  `quota_exceeded` (429) · `network` (`Connect`/`Timeout`/`Interrupted`) ·
  `upstream_unavailable` (5xx) · `unexpected_status` · `malformed_response` ·
  `http_client_unavailable`. Panic yok, `unwrap` yok.
- Guvenlik: `reqwest::Error` saklanmaz (URL sizdirabilir); redirect politikasi `none`
  (`Authorization` header'i baska host'a tasinmaz); yanittan yalnizca `value` + `expires_at`
  okunur (`session`/org/proje alanlari okunmaz); `sk-` gorunumlu bir deger donerse
  `malformed_response` uretilir.
- Yetki: `capabilities/asuna-realtime.json` (`allow-mint-realtime-token`, sadece `main`
  penceresi) + `build.rs` AppManifest + `tauri.conf.json` `app.security.capabilities`.
  `commands.rs` testleri bu dort noktanin senkron kalmasini zorunlu kilar.
- Bagimlilik: `reqwest =0.13.4` (`default-features = false`, features: `rustls`, `json`,
  `system-proxy`), `thiserror =2.0.20`, dev-only `tokio =1.53.1` (`macros`, `rt-multi-thread`).
  MSRV `1.82` → `1.85` (reqwest 0.13 sarti).
- Test: 49 cargo testi gecti (16'si bu modul). Gercek API cagrisi yok — testler yerelde
  acilan bir `TcpListener` HTTP sunucusuna vurur (`wiremock` bagimliligi eklenmedi).
  Canli endpoint dogrulamasi ASU-020'de.

---

## ASU-012: Asuna Core Prompt / Instructions Dosyasi

**Scope**: backend | **Boyut**: S | **Durum**: COMPLETED (2026-08-24) | **Bagimlilik**: ASU-002

### Aciklama
PROJECT.md Bolum 10 (kimlik) + Bolum 11 (system prompt gereksinimleri) surumlenmis tek bir dosyada.

### Acceptance Criteria
- [x] `src/asuna/prompts/core.v1.ts` olusturulmus (versiyonlu prompt dosyasi — `conventions.md` "Prompt Dosyalari")
- [x] Prompt PROJECT.md Bolum 11'deki tum ilkeleri iceriyor: uydurmama, tek somut sonraki adim,
      memory'yi durust kullanma, tool risk politikasi, Turkce + Ingilizce teknik terim karisimi
- [x] Prompt **statik ilkeler** iceriyor; degisken veri (memory, proje) sonraki phase'lerde
      `buildAsunaInstructions(context)` ile enjekte edilecek sekilde ayrilmis
- [x] Prompt kod icine gomulu string olarak dagitilmamis — tek kaynaktan okunuyor
      (`src/asuna/prompts/index.ts` aktif versiyonu secer, cagiranlar oradan alir)
- [x] Prompt uzunlugu makul (PROJECT.md Bolum 39: "Avoid giant prompts") — 45 satir, ~2.5K karakter

### Notlar
Prompt metni Ingilizce (PROJECT.md Bolum 11'deki kavramsal prompt ile ayni dil), icindeki dil
politikasi Turkce agirlikli konusmayi ve Ingilizce teknik terim korumasini tarif ediyor. Sesli
kanal icin ek kisit: kisa turlar, kod/URL okumama, kisa aktivasyon cevabi (Bolum 9.2), kesilmeye
hazir olma. `buildAsunaInstructions(context)` Phase 1'de bos context ile cagrilir ve yalnizca
cekirdek prompt'u doner; `additionalSections` Phase 3 (memory) ve Phase 4 (proje) enjeksiyonu icin
tek giris noktasi. Test: `src/asuna/prompts/core.v1.spec.ts` (14 test).

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

**Scope**: frontend | **Boyut**: M | **Durum**: COMPLETED (2026-08-24) | **Bagimlilik**: ASU-013
(SDK'ya bagimli olmadigi icin ASU-013 beklenmeden yapildi — saf TS, React ve SDK importu yok)

### Aciklama
PROJECT.md Bolum 7'deki durum listesi icin acik bir state machine. UI bunun turevi olacak, tersi degil.

### Acceptance Criteria
- [x] Durumlar tanimli: `BOOTING`, `IDLE_WAKE_WORD`, `WAKING`, `CONNECTING`, `LISTENING`,
      `USER_SPEAKING`, `ASSISTANT_THINKING`, `ASSISTANT_SPEAKING`, `TOOL_PENDING`,
      `AWAITING_APPROVAL`, `ERROR`
- [x] Gecerli gecisler acikca tanimli (`VOICE_STATE_TRANSITIONS` tablosu); gecersiz gecis
      sessizce yutulmuyor — dev'de `InvalidVoiceTransitionError`, prod'da durum korunur +
      `onInvalidTransition` ile loglanir
- [x] Phase 1'de kullanilmayan durumlar (`IDLE_WAKE_WORD`, `TOOL_PENDING`, `AWAITING_APPROVAL`)
      tanimli ama Phase 1 akisindan erisilmez — tabloda phase notlariyla isaretli
- [x] Unit test: tablodaki her gecerli gecis (54 kenar) + 5 gecersiz gecis her iki politikada
      (toplam 78 test, `src/asuna/state/voice-state-machine.spec.ts`)
- [x] State degisimleri tek bir yerden yayinlaniyor (`subscribe` — ASU-019 loglamasi buna baglanir)

### Notlar
Gecersiz gecis politikasi konfigurabilir (`invalidTransitionPolicy`), varsayilan
`import.meta.env.DEV ? 'throw' : 'reject'`: gelistirmede bug sessiz bir "durum degismedi"
olarak gizlenmez, uretimde ise sesli oturum bir UI bug'i yuzunden dusmez (PROJECT.md Bolum 30).
Oturum kapanisinda kanonik hedef `IDLE_WAKE_WORD`; Phase 1'de wake word motoru olmadigi icin
`BOOTING` de gecerli cikis hedefi (ASU-018 "IDLE/BOOTING") ve Phase 2'de (ASU-023) kaldirilacak
sekilde `SESSION_EXIT_TARGETS` icinde isaretli. React entegrasyonu ASU-015'te hook ile yapilacak.

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

**Scope**: backend | **Boyut**: M | **Durum**: COMPLETED (2026-08-24) | **Bagimlilik**: ASU-014

### Aciklama
PROJECT.md Bolum 29 (state transition log) + Bolum 30 (durust hata yonetimi).

### Acceptance Criteria
- [x] State gecisleri zaman damgasiyla loglaniyor (Bolum 29'daki ornek formata yakin):
      `12:10:01 INFO  [voice-state] WAKING -> CONNECTING (ACTIVATION_REQUESTED)`;
      kanonik bicim `formatStateTransitionLine()` ile ayrica disari veriliyor
      (`src/asuna/observability/state-logger.ts`)
- [x] Dev modda gorunur bir debug konsolu / log paneli var — `src/components/debug-panel.tsx`,
      `app.tsx` icinde `import.meta.env.DEV` + `lazy()` ile kosullu mount (uretim bundle'ina girmez);
      seviye filtresi + otomatik kaydirma + temizleme
- [x] `ASUNA_LOG_LEVEL` config'i etkili — `applyConfigLogLevel(config)` ile FrontendConfig'e baglanir;
      seviye calisma aninda degisir ve tum `child` logger'lari etkiler
- [x] Hicbir log satirinda API key, token veya ham secret yok — iki katmanli redaksiyon
      (deger prefix'i `sk-` / `ek_` + hassas alan adi `apiKey`/`token`/`value`/...),
      `logger.spec.ts` icinde tampon dokumu taranarak kanitlaniyor
- [x] Kullaniciya gosterilen hata mesajlari durust: `error-messages.ts` her Rust `kind`'i
      ("Su an ses baglantisini kuramadim: ...") ve servis hatasini ayri cumleye esliyor;
      bilinmeyen etiket jenerik ama durust mesaja dusuyor, basari taklidi yok
- [x] Beklenmeyen hata UI'yi cokertmiyor; `ERROR` durumundan tekrar baglanma yolu var —
      her `UserFacingError` `retryable` bayragi tasiyor; bozuk log abonesi log zincirini dusurmuyor

### Notlar
Modul sinirlari: `logger.ts` (seviye + redaksiyon + 500 satirlik ring buffer),
`state-logger.ts` (FSM `subscribe` + `onInvalidTransition` kablolamasi),
`error-messages.ts` (kind -> durust Turkce mesaj), `index.ts` (genel API).
Redaksiyon **varsayilan acik**: cagiran tarafin "bunu maskele" demesi gerekmez.
`value` alan adi bilerek hassas kabul edildi — Rust `EphemeralToken`'in alan adi budur.
Log yalnizca bellekte; diske yazma bilincli olarak kapsam disi (PROJECT.md Bolum 20).
Realtime servisi entegrasyonu (ASU-013) bu API'yi `logger.child('realtime')` +
`toUserFacingError(error)` uzerinden kullanir.

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

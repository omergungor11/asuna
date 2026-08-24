# ADR-004: Wake Word Saglayicisi

**Durum**: proposed (spike sonrasi accepted'a cekilecek — bkz. "Acik Kalanlar")
**Tarih**: 2026-08-24
**Iliskili**: ASU-008, OQ-3, OQ-4, OQ-6 · PROJECT.md Bolum 8 · `asuna-config/tech-stack.md` Bolum 4
**Onceki karari degistirir**: `asuna-docs/DECISIONS.md` icindeki ADR-004 (Porcupine, accepted, 2026-08-24)

---

## Baglam

Asuna, macOS uzerinde surekli acik duran, "Hey Asuna" ifadesini **lokal** olarak bekleyen
bir sesli companion. Pazarlik disi ilkeler (PROJECT.md Bolum 8, 20):

- Idle mikrofon frame'leri **sadece** wake-word motoruna gider; OpenAI'ya gitmez, diske yazilmaz.
- Idle'da aktif Realtime oturumu **yoktur** — sifir API maliyeti.
- Uygulamanin geri kalani hicbir wake-word saglayicisina baglanmaz; her sey
  `WakeWordProvider` arayuzunun arkasindadir.

```ts
interface WakeWordProvider {
  initialize(): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
  onDetected(callback: (event: WakeWordEvent) => void): () => void;
}
```

Ilk karar (`asuna-docs/DECISIONS.md`, ADR-004) Picovoice Porcupine yonundeydi. ASU-008
arastirmasi bu karari **gecersiz kildi**. Bu ADR yerine gecer.

---

## Karar

Wake word motoru olarak **sherpa-onnx `KeywordSpotter`** kullanilacak; motor **Tauri'nin Rust
process'inde** calisacak, mikrofon idle'da `cpal` ile Rust tarafindan acilacak, tespit olayi
Tauri event'i olarak renderer'a bildirilecek.

`WakeWordProvider` arayuzu **degismez**. Somut implementasyon adi `SherpaKwsProvider`.

---

## Neden Porcupine Degil

### 1. Free Tier kapatildi, non-commercial yol yok (BLOCKER)

Picovoice calisani, Picovoice'un kendi repo'sunda, `Picovoice/porcupine` issue #1574
(2026-05-25) altinda aynen sunu yazdi:

> "The AccessKey is validated when the engine is initialized, before offline data processing,
> and is also used to enforce usage limits. The SDK does not run without a valid key.
> After Free Tier AccessKeys are disabled on June 30, 2026, features using those keys will stop
> working. Going forward, we'll be focusing on our core business, enterprise deployments.
> **There is no non-commercial tier planned.**"

— https://github.com/Picovoice/porcupine/issues/1574 (erisim: 2026-08-24)

Genel FAQ bunu dogruluyor (erisim: 2026-08-24):

> "**Can I use Picovoice for personal projects?** Picovoice is a B2B company focused on
> on-device AI tools for enterprises. At this time, there are no dedicated free or paid plans
> for personal or non-commercial use."

— https://picovoice.ai/docs/faq/general/

`https://picovoice.ai/pricing/` artik fiyat sayfasi degil; JS ile `/contact`'a yonlendiriyor
(`window.location.href="/contact"`, ham HTML 1176 byte, dogrulandi 2026-08-24).
Enterprise fiyati **dogrulanamadi** — hicbir resmi rakam yayinlanmiyor.

Kalan tek yol "Free Trial for enterprise developers": tek seferlik, sureli, uzatilmiyor,
takim arkadasi ikinci trial acamiyor (FAQ, ayni sayfa). Kalici bir kisisel urun icin uygun degil.

### 2. AccessKey init aninda online dogrulaniyor (MIMARI BLOCKER)

Yukaridaki alintida acik: anahtar, offline veri islemeden **once**, motor init'inde dogrulaniyor
ve kullanim limitlerini zorlamak icin kullaniliyor. Asuna'nin "local-first, offline calisir"
sozu ile uyusmuyor. Lisans problemi olmasaydi bile bu tek basina yeterli gerekce.

### 3. Rust binding'i tamamen kaldirildi

| Kanit                                                    | Durum                                                                                                                        |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `pv_porcupine` crates.io                                 | **Tum surumler yanked** (3.0.3 dahil; 3.0.3 son yayin 2024-08-26, yank 2025-08-13 civari)                                    |
| `Picovoice/porcupine` @ `v3.0` → `binding/`              | android, angular, dotnet, flutter, **go**, ios, java, nodejs, python, react-native, react, **rust**, **unity**, **vue**, web |
| `Picovoice/porcupine` @ `v4.0` (2025-12-11) → `binding/` | android, dotnet, flutter, ios, java, nodejs, python, react-native, react, web — **rust/go/angular/vue/unity yok**            |
| Resmi macOS SDK listesi                                  | .NET, C, Java, Node.js, Python (Rust yok)                                                                                    |

v4.0 release notlari sadece "Ended support for Unity" diyor; Rust'in kaldirilmasi resmi
release notlarinda **belgelenmemis** — sessiz kaldirma. Ucuncu parti kaynaklar
"Rust SDKs will no longer be maintained after July 15, 2025" diyor (resmi sayfa JS-render
oldugu icin ham dogrulama yapilamadi — **BELIRSIZ**, ama crates.io + repo kaniti kesin).

### 4. Teknik olarak sorunsuz olan taraflar (kayit icin)

Bunlar Porcupine'in **iyi** yanlariydi ve secilmeme sebebi degildi:

- Apple Silicon resmi olarak destekli: _"macOS (x86_64, arm64)"_ — https://picovoice.ai/docs/faq/porcupine/
- `lib/mac/arm64/libpv_porcupine.dylib` + `include/pv_porcupine.h` repo'da mevcut (C API yasiyor)
- Idle tuketimi: _"The standard model uses about 1 MB of memory and less than 4% of a single core
  on a Raspberry Pi 3."_ (ayni FAQ) — macOS/arm64 rakami **yayinlanmiyor**
- Custom keyword: Picovoice Console'da saniyeler icinde `.ppn` uretiliyor; `.ppn` **platform-spesifik**
  (Web/WASM icin ayri, macOS icin ayri) ve her indirme aylik "model download" kotasindan dusuyor
- Web SDK'da Console'a gitmeden API ile egitim de var:
  `Porcupine.trainWakeWordFromPhrase(accessKey, writePath, language, phrase)`
- Ifade secimi rehberi: en az 6 fonem, cok kisa/cok uzun ifadelerden kacin.
  "Hey Asuna" (~7 fonem) bu kriteri **saglar** — ifade secimi dogruydu.

---

## Degerlendirilen Secenekler

| #   | Secenek                             | Versiyon / tarih                                                       | Sonuc                                                               |
| --- | ----------------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------- |
| A   | **sherpa-onnx Rust crate + cpal**   | `sherpa-onnx` 1.13.5 (crates.io, 2026-08-11), Apache-2.0, yanked degil | **SECILDI**                                                         |
| B   | Porcupine Rust crate                | `pv_porcupine` 3.0.3 — tum surumler yanked                             | Reddedildi: binding yok                                             |
| C   | Porcupine Web SDK (WKWebView)       | `@picovoice/porcupine-web` 4.0.1 (2026-06-25)                          | Reddedildi: lisans + AccessKey online + WKWebView resmi listede yok |
| D   | Porcupine Node sidecar              | `@picovoice/porcupine-node` 4.0.2, Node >=18                           | Reddedildi: lisans + 3. process                                     |
| E   | Porcupine C API + kendi Rust FFI    | `libpv_porcupine.dylib` (mac/arm64)                                    | Reddedildi: lisans blocker'i asmiyor                                |
| F   | openWakeWord (Python sidecar)       | 0.6.0 (PyPI 2024-02-11), Apache-2.0                                    | Reddedildi: macOS arm64'te bozuk                                    |
| G   | `oww-rs` (Rust'ta OWW inference)    | 0.3.3 (2026-06-12), MIT, 82k indirme                                   | Yedek — ayni pipeline riski, **BELIRSIZ**                           |
| H   | `rustpotter`                        | 3.0.2 (2023-10-01), Apache-2.0                                         | Yedek — ~3 yildir bakimsiz                                          |
| I   | Realtime API'ye surekli ses akitmak | —                                                                      | Reddedildi (onceki ADR'de de): gizlilik ihlali + surekli fatura     |
| J   | Sadece global kisayol / tray butonu | —                                                                      | Phase 1'de gecici aktivasyon olarak kalir; urunun cekirdegi degil   |

### F (openWakeWord) neden elendi

Hedef platform tam olarak vurulmus durumda, iki **acik** issue ile:

- `dscripka/openWakeWord#336` (acik, 2026-06-21) — "ONNX inference backend produces near-zero
  scores on macOS ARM64 (Apple Silicon)". Bildirim sahibi kok nedeni mel-spectrogram → embedding
  hattinda tespit etmis; skorlar hedef kelime net soylense bile ~1e-05.
- `dscripka/openWakeWord#309` (acik, 2026-01-13) — "macOS (Apple Silicon) cannot run
  openWakeWord natively due to missing TFLite runtime".

Ayrica resmi otomatik model egitimi **Linux-only** (Piper TTS bagimliligi) ve calisma zamani
Python → Tauri icin ayri sidecar demek.

---

## Secilen Cozumun Detaylari

### Paket ve platform

- `sherpa-onnx` 1.13.5 (Rust, Apache-2.0) — `sherpa-onnx-sys` 1.13.5 uzerine guvenli sarmalayici.
  Yayinlayan: `csukuangfj` (sherpa-onnx yazari), repo `github.com/k2-fsa/sherpa-onnx`.
- Varsayilan **static link**. `SHERPA_ONNX_LIB_DIR` set edilmemisse build script GitHub
  releases'ten uygun arsivi indirir. macOS arm64 icin:
  `sherpa-onnx-v1.13.5-osx-arm64-static-lib.tar.bz2` (shared varyanti da mevcut).
- Mikrofon: `cpal` 0.16 (sherpa-onnx'in kendi `rust-api-examples` Cargo.toml'unda kullandigi surum).

### API yuzeyi (kaynaktan dogrulanmis, uydurma degil)

Crate `KeywordSpotter`, `KeywordSpotterConfig`, `KeywordResult` tiplerini export ediyor
(docs.rs 1.13.5 item listesi ile dogrulandi). Resmi ornek:
`rust-api-examples/examples/keyword_spotter.rs` — akis:

```rust
// Kaynak: https://github.com/k2-fsa/sherpa-onnx/blob/master/rust-api-examples/examples/keyword_spotter.rs
// (Xiaomi Corporation, Apache-2.0). Asagisi ALINTIDIR, Asuna kodu degil.
use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig, Wave};

let mut config = KeywordSpotterConfig::default();
config.model_config.transducer.encoder = Some(args.encoder);
config.model_config.transducer.decoder = Some(args.decoder);
config.model_config.transducer.joiner  = Some(args.joiner);
config.model_config.tokens             = Some(args.tokens);
config.model_config.provider           = Some(args.provider);   // "cpu"
config.model_config.num_threads        = args.num_threads;

// ... KeywordSpotter olusturulur, sonra:
let stream = kws.create_stream();                 // veya create_stream_with_keywords(&str)
stream.accept_waveform(sample_rate, samples);
while kws.is_ready(&stream) {
    kws.decode(&stream);
    if let Some(result) = kws.get_result(&stream) {
        if !result.keyword.is_empty() {
            // result.json icinde tespit detayi
            kws.reset(&stream);
        }
    }
}
```

> NOT: Resmi ornek WAV dosyasi tabanlidir. Surekli mikrofon varyanti repo'da **yok**;
> `streaming_zipformer_microphone.rs` (cpal, `mic` feature) deseninden uyarlanacak.
> `create_stream_with_keywords` calisma zamaninda keyword degistirmeye izin veriyor —
> `ASUNA_WAKE_WORD` env'ini yeniden baslatmadan uygulamak icin kullanilabilir.

### "Hey Asuna" nasil uretilir — Console yok, .ppn yok, anahtar yok

Open-vocabulary KWS: model **yeniden egitilmez**. Ifade duz metin olarak verilir:

1. Ingilizce KWS modeli indirilir: `sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01`
   (k2-fsa GitHub releases, `kws-models`). Toplam ~19MB: encoder 12MB fp32 / 4.6MB int8,
   decoder 1.1MB / 272KB, joiner 628KB / 160KB, BPE model 240KB.
2. Ham keyword dosyasina `HEY ASUNA` yazilir.
3. BPE token'a cevrilir:
   `sherpa-onnx-cli text2token --tokens-type bpe --bpe-model <bpe.model> <ham.txt> <keywords.txt>`
4. Her keyword satirina istege bagli **boosting score** (`:` oneki) ve
   **trigger threshold** (`#` oneki, 0–1) eklenebilir — false-accept/miss dengesi burada ayarlanir.

Sonuc: platform-spesifik model dosyasi yok, indirme kotasi yok, vendor console yok.
Alternatif olarak `sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20` (~38MB, chunk-8 = 160ms /
chunk-16 = 320ms gecikme varyantlari) de degerlendirilebilir — daha yeni model.

Ses formati: tek kanal, 16-bit; ornekleme hizi 16kHz olmak **zorunda degil** (dahili resampler var).

### Mimari yerlesim — mikrofon kimde?

```
IDLE_WAKE_WORD:
  Rust process  → cpal input stream ACIK → KeywordSpotter
  WKWebView     → getUserMedia CAGRILMAMIS, Realtime session YOK
  Ag trafigi    → SIFIR (ne OpenAI ne lisans sunucusu)

WAKING (tespit):
  Rust          → wake event yayinlar (Tauri event) → cpal stream'i DURDURUR
  WKWebView     → getUserMedia + WebRTC → RealtimeSession

Oturum kapanisi (explicit / idle timeout / ERROR dahil):
  WKWebView     → track'leri stop eder
  Rust          → cpal stream'i YENIDEN ACAR → IDLE_WAKE_WORD
```

Bu yerlesimin ADR-004'un onceki halinden **daha guclu** oldugu nokta: idle'da renderer
mikrofona hic dokunmaz, dolayisiyla OQ-5'in (WKWebView `getUserMedia`/WebRTC riski)
idle yoluna bulasma ihtimali ortadan kalkar. Risk sadece aktif oturum yoluna sinirlanir.

### Lisans ve maliyet

| Kalem                                       | Durum                                                                            |
| ------------------------------------------- | -------------------------------------------------------------------------------- |
| `sherpa-onnx` / `sherpa-onnx-sys` crate     | Apache-2.0                                                                       |
| `k2-fsa/sherpa-onnx` repo                   | Apache-2.0                                                                       |
| `cpal`                                      | Apache-2.0                                                                       |
| KWS **model agirliklari** (gigaspeech-3.3M) | **BELIRSIZ** — release'te acik lisans ifadesi bulunamadi. Spike'ta dogrulanacak. |
| Calisma zamani ucret                        | $0 — AccessKey yok, kota yok, MAU takibi yok, phone-home yok                     |
| Ticari kullanim / kapali kaynak dagitim     | Kod tarafinda Apache-2.0 ile serbest; model tarafi BELIRSIZ'e bagli              |

---

## Etkiler

- **`asuna-docs/DECISIONS.md` ADR-004** superseded olarak isaretlendi.
- **`asuna-config/tech-stack.md` Bolum 4** guncellenir: Porcupine → sherpa-onnx KWS.
- **ASU-009 / `.env.example`**:
  - `PICOVOICE_ACCESS_KEY` **kaldirilir**.
  - `ASUNA_WAKE_WORD_PROVIDER` varsayilani `porcupine` → `sherpa-kws`.
  - Eklenir: `ASUNA_WAKE_WORD_MODEL_DIR`, `ASUNA_WAKE_WORD_THRESHOLD`.
  - `ASUNA_WAKE_WORD="Hey Asuna"` kalir.
- **README** harici setup adimindan "Picovoice AccessKey" cikar; yerine "KWS modelini indir"
  adimi gelir (veya build/first-run'da otomatik indirme).
- **`WakeWordProvider` arayuzu degismez.** Uygulamanin geri kalani etkilenmez.
- **OQ-3** kapanir (lisans: ucretsiz kota yok, Porcupine kullanilmiyor).
  **OQ-4** kapanir (motor Rust tarafinda, mikrofon Rust'ta).
  **OQ-6** acik kalir — spike'a devredilir.
- Bundle boyutu: static sherpa-onnx + onnxruntime + KWS model (int8 tercih edilirse ~5MB).
  Kesin delta spike'ta olculur.

---

## Acik Kalanlar (spike ile kapatilacak — durum: **SPIKE BEKLIYOR**)

Bu ADR `proposed`. Asagidaki 5 madde yesil donmeden `accepted` yapilmaz.
(ASU-008'in "calisan detection spike'i" akseptans kriteri **ASU-008b** olarak ayri task'tir.)

1. **"HEY ASUNA" tespit kalitesi.** 50 farkli soyleyis uzerinde detection rate ve 8 saatlik
   gercek ortam idle'da false-accept sayisi. Boosting score / threshold taramasi.
   _Kabul esigi onerisi: >%95 detection, <1 FP / 8 saat._
   Bilinmeyen: "Asuna" OOV bir ozel isim — BPE subword'lerinin bunu tasiyip tasimadigi.
2. **Idle CPU/RAM (Apple Silicon).** cpal 16kHz mono + KeywordSpotter, `num_threads=1`,
   int8 model, 30 dk olcum. Silero VAD ile kapili (VAD-gated) varyant ayrica olculur.
   Kiyas referansi yok — Porcupine'in tek resmi rakami RPi3 uzerinde <%4/1MB.
3. **Mikrofon devir teslimi (OQ-6).** Rust cpal → renderer `getUserMedia` gecis suresi;
   macOS TCC bunu tek izin olarak mi iki ayri prompt olarak mi soruyor; turuncu mikrofon
   gostergesinin idle davranisi UX olarak kabul edilebilir mi.
4. **Build / bundle / imzalama.** Build script'in GitHub'dan arsiv indirmesi CI'da ve
   offline build'de nasil davraniyor; static link ile `.app` boyut delta'si; `codesign` +
   notarization sorunsuz mu; ONNX model dosyalarinin Tauri resource olarak paketlenmesi.
5. **Model agirliklarinin lisansi.** gigaspeech-3.3M KWS modelinin dagitim/ticari kullanim
   lisansi dokumante degil. k2-fsa release notu / HuggingFace model card / issue ile netlestirilecek.
   Olumsuz cikarsa: zh-en 2025-12-20 modeli veya kendi KWS modelini egitme yoluna bakilir.

**Geri donus (exit) plani.** Spike (1) veya (2) kalirsa sirasiyla:
(a) KWS'yi Silero VAD ile kapila — CPU ve false-accept birlikte duser (ayni crate'te
`VoiceActivityDetector` var); (b) tetikleyici ifadeyi uzat; (c) `oww-rs` (MIT) veya
`rustpotter` (Apache-2.0) adapter'in arkasinda dene; (d) son care olarak global
kisayol/tray butonunu kalici sekonder aktivasyon olarak birak (Phase 1'de zaten var).
Her durumda `WakeWordProvider` arayuzu degismez — degisim `src-tauri` icinde tek modul,
tahmini **~1 gun**.

---

## Kaynaklar

Hepsi 2026-08-24 tarihinde erisildi.

**Porcupine / Picovoice (resmi)**

- https://github.com/Picovoice/porcupine/issues/1574 — "Clarification on Free Tier shutdown and offline deployments"; Picovoice calisani cevabi, 2026-05-25
- https://picovoice.ai/docs/faq/general/ — genel FAQ (free trial, MAU tanimi, 30 gunluk reset, kisisel kullanim yok)
- https://picovoice.ai/docs/faq/porcupine/ — platform listesi (macOS x86_64/arm64), CPU/RAM rakami, wake word secim rehberi
- https://picovoice.ai/docs/quick-start/porcupine-macos/ — macOS SDK listesi: .NET, C, Java, Node.js, Python
- https://picovoice.ai/docs/quick-start/porcupine-web/ — Web SDK, `.ppn` "Web (WASM)" platformu, `trainWakeWordFromPhrase`
- https://picovoice.ai/pricing/ — `/contact`'a JS redirect (fiyat yayinlanmiyor)
- https://github.com/Picovoice/porcupine — `binding/` icerigi (v3.0 vs v4.0 karsilastirmasi), `lib/mac/arm64/`, `include/`, v4.0 release notlari (2025-12-11)
- https://crates.io/crates/pv_porcupine — tum surumler yanked (API ile dogrulandi)
- npm: `@picovoice/porcupine-web` 4.0.1 (2026-06-25), `@picovoice/porcupine-node` 4.0.2 (Node >=18), `@picovoice/porcupine-react` 4.0.0

**sherpa-onnx**

- https://crates.io/crates/sherpa-onnx — 1.13.5, 2026-08-11, Apache-2.0, yanked degil
- https://docs.rs/sherpa-onnx/latest/sherpa_onnx/ — setup, static/shared, macOS arm64 arsiv adlari
- https://docs.rs/sherpa-onnx/latest/sherpa_onnx/all.html — `KeywordSpotter`, `KeywordSpotterConfig`, `KeywordResult` mevcut
- https://github.com/k2-fsa/sherpa-onnx/blob/master/rust-api-examples/examples/keyword_spotter.rs — resmi Rust ornegi
- https://github.com/k2-fsa/sherpa-onnx/blob/master/rust-api-examples/Cargo.toml — cpal 0.16, `mic` feature
- https://k2-fsa.github.io/sherpa/onnx/kws/index.html — open-vocabulary KWS, `text2token`, boosting/threshold
- https://k2-fsa.github.io/sherpa/onnx/kws/pretrained_models/index.html — model boyutlari, ses formati

**Alternatifler**

- https://github.com/dscripka/openWakeWord/issues/336 — ONNX near-zero score, macOS ARM64 (acik, 2026-06-21)
- https://github.com/dscripka/openWakeWord/issues/309 — TFLite runtime yok, darwin/arm64 (acik, 2026-01-13)
- https://pypi.org/project/openwakeword/ — 0.6.0, 2024-02-11
- https://crates.io/crates/oww-rs — 0.3.3, 2026-06-12, MIT
- https://crates.io/crates/rustpotter — 3.0.2, 2023-10-01, Apache-2.0

**Destekleyici (tek basina kanit degil)**

- https://community.home-assistant.io/t/fyi-picovoice-confirmed-free-tier-accesskeys-will-stop-working-after-june-30-2026/1012744

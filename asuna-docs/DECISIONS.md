# Architecture Decisions (ADR-lite)

> Her mimari/teknolojik karar buraya. Format asagida. En yeni en ustte.

<!-- Sablon:

## ADR-001: [Karar basligi] — [TARIH]

**Durum**: proposed | accepted | superseded
**Karar**: [Ne secildi]
**Gerekce**: [Neden]
**Alternatifler**: [Neden elenmis]
**Etki**: [Neyi degistiriyor, geri donus maliyeti]
-->

## ADR-007: Claude Code gelistirme modeli = Fable orchestrator + opus subagent'lar — 2026-08-24

**Durum**: accepted

**Karar**: Ana Claude Code oturumu (Fable) orchestrator rolunde kalir; mimari karar, koordinasyon
ve paket kurulumu orada yapilir. Kodlama, arastirma, review ve test isleri `model: opus`
subagent'lara devredilir. Task ID formati `ASU-001, ASU-002...`, commit formati
`feat(ASU-XXX): aciklama` — commit mesajlarinda Claude attribution satiri YOK.

**Gerekce**:
- Asuna cok katmanli (audio / agent / memory / projects / tools / permissions / security / database / ui);
  her katman kendi context'ini gerektiriyor, hepsini tek oturumda tasimak context'i sisiriyor.
- Orchestrator'in kararlari tek yerde toplanmasi, DECISIONS.md ile ana oturumun ayni yerde durmasini sagliyor.
- Bu proje icin subagent'larda ucuz model tercih EDILMEDI: guvenlik/permission/path mantigi ve
  Realtime SDK entegrasyonu hata toleransi dusuk isler; opus kalitesi maliyetten onceliklidir.

**Alternatifler**:
- *Tek oturumda hersey*: context sisme, uzun oturumlarda karar tutarsizligi riski.
- *Haiku subagent'lar*: mekanik islerde ucuz olurdu, ama bu repoda mekanik is (toplu tarama/formatlama)
  agirlikli degil; guvenlik-kritik kod agirlikli. Ileride saf tarama/log ayiklama isleri cikarsa
  o gorevler icin haiku ayrica degerlendirilebilir.

**Etki**: Gorev dagitimi ve commit disiplini bu modele gore isler. Geri donus maliyeti dusuk —
sadece calisma bicimi, kod uzerinde iz birakmaz.

---

## ADR-006: API key guvenligi = ephemeral token minting Tauri Rust tarafinda — 2026-08-24

**Durum**: accepted

**Karar**: `OPENAI_API_KEY` yalnizca guvenilir process'te (Tauri Rust tarafi) bulunur.
Renderer/webview, Realtime baglantisi icin Rust tarafindan uretilen **kisa omurlu (ephemeral)
client secret** ister ve baglantiyi o gecici token ile kurar. Kalici API key hicbir kosulda
renderer bundle'ina, frontend `.env`'ine veya `VITE_*` prefix'li bir degiskene girmez.

**Gerekce**:
- PROJECT.md Bolum 7 "Authentication" ve AGENT-SPEC-ORIGINAL "Security" bunu acikca sart kosuyor.
- Webview bundle'i incelenebilir; oraya konan kalici key sizmis kabul edilir.
- Token'in omru kisa oldugu icin sizinti durumunda etki penceresi dar.
- Bu ayni zamanda ileride uzak bir trusted backend'e gecisi ucuzlatir — arayuz ayni kalir,
  token'i ureten taraf degisir.

**Alternatifler**:
- *Key'i renderer'a gomup dogrudan baglanmak*: en hizli yol, ama guvenlik siniri yok — reddedildi.
- *Ayri bir local Node sidecar servisi*: calisir, ama ikinci bir process yasam dongusu, port
  yonetimi ve dagitim yuku getirir; Tauri zaten guvenilir bir native taraf sagliyorken gereksiz.

**Etki**: Renderer'da OpenAI credential'i tutan hicbir kod olamaz. Realtime baglantisi kurmadan
once mutlaka bir Tauri command uzerinden token alinir. Bu sinir Phase 1'de kurulur ve sonradan
gevsetilmez.

---

## ADR-005: Persistence = SQLite; erisim katmani ACIK — 2026-08-24

**Durum**: **proposed** (kismen acik — Phase 0 arastirma task'ina bagli)

**Karar (kesin olan)**: Kalici depolama **SQLite**. Memory, transcript, project registry, tool audit
hepsi tek local veritabaninda tutulur. Bulut veritabani yok — local-first ilkesi (PROJECT.md Bolum 5.1).

**ACIK SORU (Phase 0'da netlesecek)**: SQLite'a **hangi katmandan** erisilecek?
- (a) `tauri-plugin-sql` — DB erisimi renderer'dan plugin uzerinden yapilir;
- (b) Rust tarafinda bir database servisi + Tauri command'lari — renderer sadece komut cagirir, SQL yazmaz.

Karar kriterleri: migration yonetimi, TypeScript tarafinda tip guvenligi (Drizzle/Prisma kullanilabilirligi),
AGENT-SPEC'teki "React componentleri dogrudan DB sorgusu calistirmaz" kurali, ve gizli veriye
(transcript, memory) erisimin guven sinirinin hangi tarafta durmasi gerektigi.

> Ilk egilim (b) yonunde — cunku guven siniri Rust tarafinda oldugunda ADR-006 ile ayni cizgide kaliyor.
> Ama Phase 0 arastirmasi bitmeden karar **accepted** sayilmaz.

**Gerekce**: SQLite tercihi PROJECT.md Bolum 12.1'de belirtilmis; tek dosya, yedeklenebilir,
kullanicinin makinesinden cikmiyor, memory'nin "incelenebilir ve silinebilir" olma sartini kolay karsiliyor.

**Alternatifler**:
- *Vector DB (pgvector, Chroma, LanceDB) ile baslamak*: PROJECT.md acikca "gerekmedikce karmasik
  vector platformu ile baslama" diyor. Semantik retrieval ihtiyaci olcum ile kanitlanana kadar ertelendi.
- *JSON dosyalari*: iliskisel sorgu ve migration yok, memory katmani buyuyunce cokuyor.
- *Uzak Postgres/Supabase*: local-first ilkesini dogrudan ihlal eder.

**Etki**: Sema tasarimi (PROJECT.md 12.2) SQLite uzerine kurulur. Acik olan sadece erisim yolu;
sema ve tablo tasarimi bu karardan bagimsiz ilerleyebilir. Erisim katmani karari verildiginde
bu ADR **accepted**'a cekilir veya yeni bir ADR ile supersede edilir.

---

## ADR-004: Wake word = Picovoice Porcupine, WakeWordProvider adapter arkasinda — 2026-08-24

**Durum**: superseded by `docs/decisions/ADR-004-wake-word-provider.md` (2026-08-24)

> **GUNCELLEME (ASU-008 arastirmasi):** Picovoice Free Tier 2026-06-30'da kapatildi,
> non-commercial tier planlanmiyor, Rust binding'i kaldirildi (crates.io'da yanked) ve
> AccessKey motor init'inde **online** dogrulaniyor (local-first ihlali). Yeni karar:
> **sherpa-onnx `KeywordSpotter`** (Apache-2.0, tamamen offline, Tauri Rust process'inde,
> mikrofon idle'da `cpal` ile Rust tarafinda). `WakeWordProvider` adapter'i degismedi —
> bu ADR'nin ongordugu vendor-degisim senaryosu aynen islemistir.
> Detay ve kaynaklar: `docs/decisions/ADR-004-wake-word-provider.md`.

**Karar**: "Hey Asuna" wake word tespiti icin ilk implementasyon **Picovoice Porcupine**.
Ancak uygulamanin geri kalani Porcupine'i dogrudan tanimaz; her sey PROJECT.md Bolum 8'deki
`WakeWordProvider` arayuzu arkasindadir:

```ts
interface WakeWordProvider {
  initialize(): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
  onDetected(callback: (event: WakeWordEvent) => void): () => void;
}
```

Wake word isleme **tamamen lokal**dir. Idle durumda mikrofon frame'leri sadece wake word motoruna
gider; OpenAI'ya gonderilmez ve diske yazilmaz. Wake sonrasi wake word motoru durdurulur/askiya
alinir ve Realtime oturumu acilir.

**Gerekce**:
- On-device calisir, Apple Silicon dahil macOS destegi var, custom wake word egitimine izin veriyor.
- Always-listening senaryosu icin tasarlanmis; surekli bulut cagrisi yapmiyor.
- Adapter zorunlulugu vendor lock'u onluyor — lisans/fiyat/kalite degisirse motor degistirilir,
  cagiran kod degismez.

**Alternatifler**:
- *openWakeWord / snowboy tarzi acik alternatifler*: lisans acisindan rahat, ama macOS/Apple Silicon
  paketleme ve false-positive kalitesi belirsiz. Adapter sayesinde ileride denenebilir.
- *Realtime API'ye surekli ses akitip modeli tetikleyici olarak kullanmak*: hem gizlilik ilkesini
  (idle ses buluta gitmez) ihlal eder hem de surekli faturalandirma yaratir — reddedildi.
- *Sadece global kisayol / tray butonu*: MVP'de gecici aktivasyon olarak Phase 1'de kullanilacak,
  ama urunun cekirdegi sesli uyandirma oldugu icin nihai cozum degil.

**Etki**: `PICOVOICE_ACCESS_KEY` bir yapilandirma gereksinimi olarak eklenir. Wake word Phase 2'de
gelir; Phase 1 gecici manuel aktivasyon butonu ile ilerler. Motor degisimi tek dosyalik bir is olur.

---

## ADR-003: Realtime model konfigurasyonu = ASUNA_REALTIME_MODEL env — 2026-08-24

**Durum**: accepted

**Karar**: Kullanilacak Realtime modeli tek bir yapilandirma degiskeninden okunur:

```env
ASUNA_REALTIME_MODEL=gpt-realtime-2.1        # varsayilan
ASUNA_REALTIME_MODEL=gpt-realtime-2.1-mini   # dev / ekonomi
```

Model ID'si **hicbir yerde hard-code edilmez** — ne agent kurulumunda, ne test'lerde, ne fallback
olarak. Config tek merkezden (tipli bir config modulu) okunur ve oradan dagitilir.

**Gerekce**:
- PROJECT.md Bolum 7 ve AGENT-SPEC-ORIGINAL ikisi de bunu acikca sart kosuyor.
- Realtime model isimleri hizli degisiyor; hard-code edilmis bir ID kod tabanina dagilirsa
  her surum yukseltmesi arama-degistirme isine donusur.
- Maliyet kontrolu urun gereksinimi (PROJECT.md "Important billing note"): ChatGPT aboneligi ile
  API kredisi ayri faturalanir. Gelistirme sirasinda `-mini`'ye tek satirla dusebilmek gerekiyor.

**Alternatifler**:
- *Kodda sabit model ID*: en basit, ama yukaridaki iki gereksinimi de ihlal ediyor.
- *Runtime'da UI'dan model secimi*: ileride istenebilir, ama MVP icin gereksiz yuzey; once
  config katmani dogru kurulsun, UI secici bunun uzerine oturur.

**Etki**: Config modulu Phase 0/1'de kurulur ve model ID'sine ihtiyac duyan her yer oradan beslenir.
Yeni model cikinca degisen tek sey `.env` satiri olur.

---

## ADR-002: Ses mimarisi = OpenAI Agents SDK (RealtimeAgent/RealtimeSession) + WebRTC — 2026-08-24

**Durum**: accepted

**Karar**: Sesli konusma katmani **OpenAI Agents SDK for TypeScript** uzerine kurulur;
`RealtimeAgent` + `RealtimeSession` soyutlamalari kullanilir. Transport olarak **WebRTC** secilir.

**Gerekce**:
- WebRTC dusuk gecikmeli medya icin tasarlanmis; sesli companion'da algilanan kalitenin belirleyicisi gecikme.
- Kesme (interruption) davranisi WebRTC akisinda dogal calisiyor — Asuna'nin konusmasini kullanicinin
  sozle bolebilmesi urunun temel etkilesimi.
- Agents SDK, oturum yasam dongusu, tool calling ve event akisini hazir veriyor; bunlari elle yazmak
  Phase 1'i (en zor etkilesim dongusu) gereksiz uzatir.
- Tauri webview'i tarayici WebRTC yigini sagliyor, ek native medya katmani gerekmiyor.

**Alternatifler**:
- *Low-level Realtime API + WebSocket*: daha fazla kontrol, ama ses buffer yonetimi, kesme mantigi,
  oturum event'leri ve tool calling protokolunun tamami elle yazilir. Server-merkezli bir ihtiyac
  (ornegin sunucu tarafi ses isleme, coklu istemci) ortaya cikarsa yeniden degerlendirilir.
- *Ayri STT + LLM + TTS zinciri (Whisper + chat + TTS)*: her adimda gecikme birikir, dogal kesme
  neredeyse imkansiz — "chatbot degil, companion" hedefiyle celisir.

**Etki**: Phase 1'in tamami bu secim uzerine kurulu. Geri donus maliyeti orta: SDK'dan low-level
API'ye gecis, agent katmanini yeniden yazmayi gerektirir ama audio/agent siniri korunursa
UI ve memory katmanlari etkilenmez. Bu yuzden `audio` ve `agent` modul sinirlari bastan net cizilir.

---

## ADR-001: Desktop shell = Tauri 2 — 2026-08-24

**Durum**: accepted

**Karar**: Masaustu kabugu **Tauri 2 + React + TypeScript (strict) + Vite**, paket yoneticisi **pnpm**.
Hedef platform macOS. App scaffold greenfield olarak kurulur (repoda henuz uygulama kodu yok).

**Gerekce** (PROJECT.md Bolum 7):
- Hafif dagitim — Electron'a gore cok daha kucuk binary ve dusuk bellek; her zaman acik duran,
  tray'de bekleyen bir companion icin bu dogrudan urun kalitesi meselesi.
- **Capability model** — Tauri'nin izin/capability sistemi, sinirsiz bir Electron main process'ine
  gore daha guvenli bir varsayilan sagliyor. Asuna dosya sistemi ve tool calistirma yapacagi icin
  bu sinirlarin framework tarafindan zorlanmasi degerli.
- Native yeteneklere erisim: system tray, global shortcut, notification, native pencere davranisi.
- ADR-006'nin gerektirdigi "guvenilir process" zaten Rust tarafi olarak hazir geliyor.

**Alternatifler**:
- *Electron*: olgun ekosistem, daha fazla ornek, Node API'lerine dogrudan erisim. Elenme nedeni:
  agir dagitim + surekli acik uygulama icin bellek maliyeti + guvenlik sinirinin varsayilan olarak
  cok genis olmasi. (PROJECT.md notu: mevcut template zaten iyi kurulmus bir Electron olsaydi MVP
  sirasinda gocurulmezdi — ama repoda uygulama kodu yok, dolayisiyla bu istisna gecerli degil.)
- *Native SwiftUI*: en iyi macOS entegrasyonu, ama Realtime SDK TypeScript ekosisteminde;
  web teknolojisi ile gitmek Phase 1'i belirgin sekilde hizlandiriyor.
- *Sadece web uygulamasi*: wake word, tray, global shortcut ve local dosya erisimi imkansiz —
  urun tanimiyla celisiyor.

**Etki**: Repo yapisi `src/` (renderer) + `src-tauri/` (Rust) olarak ikiye ayrilir. Guven siniri
bu iki taraf arasindadir ve ADR-005/ADR-006 bu sinira dayanir. Kabuk degisimi (Tauri → Electron)
Rust tarafindaki her seyi yeniden yazmayi gerektirir; bu yuzden Rust tarafi minimal ve iyi
tanimlanmis command'lardan olusmali.

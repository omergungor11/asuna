# Tech Stack

> Asuna: local-first, macOS, "Hey Asuna" wake word'lu sesli kisisel AI companion — chatbot degil.
> Kaynak: `PROJECT.md` Bolum 6-8, 12, 17, 19, 23, 28. Bu dosya o spec'in karara donmus halidir.
> Versiyonlariyla birlikte yaz; buyuk versiyon yukseltmeleri `asuna-docs/DECISIONS.md`'ye kaydedilir.
> Kesin minor/patch versiyonlar Phase 0 scaffold'unda pinlenir. Sonda **ACIK SORULAR** bolumu var —
> hepsi Phase 0 (teknik arastirma + scaffold) sonunda kapanmali.

## Ozet

| Katman | Secim | Durum |
|--------|-------|-------|
| Desktop shell | Tauri 2 (Rust host + system webview) | Karar |
| Paket yoneticisi | pnpm | Karar |
| Frontend | React + TypeScript (strict) + Vite | Karar |
| AI / orchestration | OpenAI Agents SDK for TypeScript (RealtimeAgent / RealtimeSession) | Karar |
| Ses transport | WebRTC | Karar |
| Realtime model | `gpt-realtime-2.1` (dev: `gpt-realtime-2.1-mini`) — env ile | Karar |
| Wake word | Picovoice Porcupine, `WakeWordProvider` adapter arkasinda | Karar |
| Veritabani | SQLite | Karar — **erisim yolu acik** (OQ-1) |
| Secret / auth | Ephemeral Realtime token, key Rust tarafinda | Karar |
| Test | Vitest (unit/integration) + Rust `cargo test` | Karar |

---

## 1. Desktop Shell — Tauri 2

- **Tauri 2.x** — Rust host process + macOS system webview (WKWebView)
- Hedef platform: macOS (Apple Silicon oncelikli); Windows/Linux MVP hedefi degil
- Rust tarafi "guvenilir process": ephemeral token uretimi, SQLite erisimi, dosya/git okuma, tool execution
- `src-tauri/` altinda capability/permission dosyalari; webview'e sadece ihtiyac duyulan komutlar acilir

**Neden:** Asuna surekli acik duran, mikrofon dinleyen ve bilgisayarda islem yapan bir uygulama —
yetki yuzeyi urunun kendisi. Tauri'nin capability tabanli izin modeli her komutu tek tek acmayi
zorunlu kilar, boylece "kisitsiz main process" riski bastan yok.

**Neden Electron degil:** Electron'un main process'i varsayilan olarak tam Node yetkisine sahip;
Asuna'nin risk seviyeli tool mimarisi (PROJECT.md 5.4, 17, 18) icin bu yanlis varsayilan.
Ayrica chromium bundle'i ~120MB+ ve surekli calisan bir companion icin bellek maliyeti yuksek.
Rust tarafi ayrica Porcupine ve SQLite icin native erisimi zaten gerektiriyor.

**Not:** PROJECT.md 6.1 "mevcut template'i koru" diyor; template audit yapildi — repoda uygulama
kodu yok, sadece Claude Code workflow meta-template'i var. Dolayisiyla scaffold greenfield.

---

## 2. Frontend

- **React 19.x** + **TypeScript 5.x** (`strict: true`, `any` yasak)
- **Vite 7.x** dev server / build (Tauri'nin `devUrl` hedefi)
- **pnpm** — workspace tek paket ile baslar, gerekirse bolunur
- Styling: scaffold'da sade CSS Modules / tek utility katmani; UI ana urun degil (PROJECT.md 21)
- State: voice state machine icin acik reducer/FSM; genel UI state icin hafif store (zustand sinifi)
- Sunucu tarafi framework yok — Next.js/SSR kavramlari Asuna'da gecersiz

**Neden:** Voice birincil arayuz, UI guven ve kontrol icin var. Agir UI framework'u urunu
yavaslatir; React + Vite Tauri webview'inde en dusuk surtunmeli, ekip icin en bilindik kombinasyon.

### Voice state machine

Uygulamanin tek dogru durum kaynagi. Gecisler loglanir (PROJECT.md 29), UI dogrudan bu duruma bakar:

```text
BOOTING
IDLE_WAKE_WORD
WAKING
CONNECTING
LISTENING
USER_SPEAKING
ASSISTANT_THINKING
ASSISTANT_SPEAKING
TOOL_PENDING
AWAITING_APPROVAL
ERROR
```

Kurallar:
- `IDLE_WAKE_WORD` varsayilan durum; bu durumda **Realtime session yok, buluta ses gitmiyor**
- Wake word yakalandiginda: `IDLE_WAKE_WORD → WAKING → CONNECTING → LISTENING`
- Oturum kapanisinda (explicit kapatma / inactivity timeout / kurtarilamaz hata) her yol
  `IDLE_WAKE_WORD`'e doner — `ERROR` dahil
- Durum degerleri SCREAMING_SNAKE string union; magic string kullanilmaz

---

## 3. AI / Orchestration

- **OpenAI Agents SDK for TypeScript** — `RealtimeAgent` + `RealtimeSession`
- Transport: **WebRTC** (dusuk gecikme, dogal interruption, webview'de native destek)
- WebSocket transport sonraki faz — sunucu merkezli bir ihtiyac dogarsa
- SDK detaylari **`AsunaRealtimeService`** wrapper'i arkasinda kalir; React bilesenleri
  ve tool katmani SDK tipleriyle dogrudan konusmaz (PROJECT.md 24)
- Tool tanimlari SDK formatina `AsunaToolDefinition` registry'sinden adapte edilir —
  ters yon degil

**Neden:** Realtime API'yi elle surmek (ses chunk'lari, interruption, VAD, tool call protokolu)
Phase 1'i haftalara yayar. Agents SDK bu dongunun tamamini kapsar; wrapper sayesinde ileride
lower-level API'ye ya da baska saglayiciya gecis tek dosyalik degisiklik olur.

### Model konfigurasyonu

Model ID'leri **asla hard-code edilmez**. Tek okuma noktasi config servisi:

```env
ASUNA_REALTIME_MODEL=gpt-realtime-2.1        # quality
# ASUNA_REALTIME_MODEL=gpt-realtime-2.1-mini # economy / development
ASUNA_REALTIME_VOICE=
```

Ayarlar UI'inda model secilebilir olmali (PROJECT.md 21, 28).

---

## 4. Wake Word

- **Picovoice Porcupine** — on-device, macOS + Apple Silicon destegi, custom wake word
- Tetikleyici ifade: **"Hey Asuna"** (MVP'de tek, iyi egitilmis trigger — false positive dusuk kalsin)
- Zorunlu soyutlama:

```ts
interface WakeWordProvider {
  initialize(): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
  onDetected(callback: (event: WakeWordEvent) => void): () => void;
}
```

- Porcupine implementasyonu bu interface'in **tek** somut ornegidir; uygulamanin geri kalani
  `WakeWordProvider` tipini gorur, vendor adini gormez
- Wake sonrasi wake-word motoru durdurulur/askiya alinir; oturum kapaninca yeniden baslar
- Konfigurasyon degiskenleri (`.env.example`, ASU-009): `PICOVOICE_ACCESS_KEY` (Porcupine access
  key — sadece Rust/guvenilir process okur) ve `ASUNA_WAKE_WORD_PROVIDER` (varsayilan `porcupine`;
  adapter'in hangi implementasyonunun secilecegi). Tetikleyici ifade ayrica `ASUNA_WAKE_WORD`.

**Neden:** Porcupine bugun macOS'ta calisan, custom kelime destekleyen en olgun on-device secenek.
Ancak lisans/fiyat modeli degisebilir (OQ-3) — bu yuzden vendor lock kabul edilmiyor.

**Gizlilik (pazarlik disi):** Idle mikrofon frame'leri sadece wake-word motoruna gider,
OpenAI'a gonderilmez, diske yazilmaz (PROJECT.md 8, 20).

---

## 5. Database

- **SQLite** — tek dosya, local-first, MVP icin dogru boyut
- Semalar: `memories`, `projects`, `sessions`, `tasks`, `tool_events` (PROJECT.md 12.2)
- Migration'lar versiyonlu SQL dosyalari; uygulama acilisinda idempotent uygulanir
- Vector/embedding katmani **Stage B** — yeterli memory birikmeden eklenmez (PROJECT.md 13)

**Neden:** Memory urunun merkezi ama MVP'de sorgu hacmi kucuk ve tek kullanicili.
Vector platformu ya da sunucu DB'si bu asamada cozdugunden fazla sorun uretir.

**Erisim yolu ACIK (OQ-1).** Degerlendirilecek secenekler:

| Secenek | Nasil | Arti | Eksi |
|---------|-------|------|------|
| A. `tauri-plugin-sql` | SQLx tabanli plugin, renderer'dan JS API | En hizli scaffold, migration destegi hazir | SQL webview tarafinda yazilir; sorgu yuzeyi genis, sandbox zayif |
| B. Rust servis + `#[tauri::command]` | `rusqlite`/`sqlx` Rust tarafinda, webview'e dar tiplenmis komut API'si | Guvenlik ve audit dogru yerde; secret hic webview'e gecmez | Her repository metodu icin Rust + TS iki taraf yazilir |
| C. Node sidecar + `better-sqlite3` | Ayri Node process | Bilindik JS ekosistemi | Ucuncu bir process, dagitim ve lifecycle karmasasi |

Secim kriterleri: (1) memory ve `tool_events` yazimi guvenilir process'te mi kaliyor,
(2) path sandbox / secret redaksiyon nerede uygulaniyor, (3) Phase 1 hizina etkisi,
(4) sifreli DB'ye (SQLCipher) ileride gecis maliyeti.
**On egilim: B** — PROJECT.md 19'un guvenlik modeliyle en tutarli olan o; ama Phase 0'da
A'nin permission scope'u olcup karar verilecek.

---

## 6. Auth & Secrets

Tek katı kural: **kalici OpenAI API key'i renderer/webview bundle'ina asla girmez** (PROJECT.md 7, 19).

Akis:

```text
1. OPENAI_API_KEY sadece guvenilir process'te (Tauri Rust tarafi) okunur
2. Webview "oturum baslatacagim" der -> #[tauri::command] cagrisi
3. Rust tarafi OpenAI'dan kisa omurlu Realtime client secret (ephemeral token) alir
4. Webview WebRTC baglantisini SADECE bu gecici token ile kurar
5. Token suresi dolunca yenisi istenir; kalici key hicbir zaman IPC'den gecmez
```

- Key kaynagi: `.env` (git disi) → ileride macOS Keychain (PROJECT.md 20)
- `.env.example` gercek deger icermez
- Loglara, hata mesajlarina, tool argumanlarina secret yazilmaz; `tool_events.arguments_redacted`
  zaten redakte alan
- Vite `import.meta.env` uzerinden **hicbir** OpenAI credential'i expose edilmez —
  `VITE_` prefix'li secret yasak

---

## 7. Maliyet Yonetimi (PROJECT.md 28)

Sesli agent surekli API tuketimi uretebilir. Mimariye gomulu kontroller:

- **Idle'da Realtime session yok** — en buyuk tasarruf kalemi; wake word beklerken sifir API maliyeti
- Inactivity timeout ile otomatik disconnect (`ASUNA_IDLE_TIMEOUT_SECONDS=45`)
- Maksimum oturum suresi siniri
- Oturum sure takibi + varsa token/audio kullanim metadatasi → `sessions` tablosuna yazilir
- Gunluk tahmini maliyet gostergesi (Ayarlar ekrani)
- Model secimi kullanici tarafindan degistirilebilir: quality `gpt-realtime-2.1` /
  economy `gpt-realtime-2.1-mini`
- Gelistirme varsayilani `-mini`

**Faturalama notu:** ChatGPT aboneligi ile OpenAI API faturalandirmasi ayri sistemlerdir.
ChatGPT Plus/Pro Realtime API kredisi saglamaz — API erisimi ayrica konfigure edilir.

> Deneyimi kanitlamadan erken optimizasyon yapma; ama idle'da baglanti acik birakmak
> "erken optimizasyon" degil, tasarim hatasidir.

---

## 8. Test & Tooling

- **Vitest** — unit + integration (memory ranking, permission logic, path sandbox,
  project detection, tool schema, state gecisleri)
- **Rust:** `cargo test` + `clippy` — ephemeral token uretimi, path normalizasyonu, DB katmani
- Lint/format: ESLint (typescript-eslint) + Prettier; Rust `rustfmt`
- CI: GitHub Actions — typecheck, lint, test, `cargo build` (Phase 0 cikti kriteri: CI yesil)
- E2E/manuel: sesli akis manuel kabul testleriyle dogrulanir (PROJECT.md 31)

---

## ACIK SORULAR

Phase 0'da (teknik arastirma + scaffold) kapatilir. Karar cikan her madde
`asuna-docs/DECISIONS.md`'ye yazilir ve bu dosya guncellenir.

| ID | Soru | Secenekler / Not | Faz |
|----|------|------------------|-----|
| OQ-1 | SQLite'a hangi yoldan erisilecek? | A: `tauri-plugin-sql` · B: Rust servis + `#[tauri::command]` (on egilim) · C: Node sidecar + better-sqlite3 | Phase 0 |
| OQ-2 | Migration araci ne olacak? | Plugin'in kendi migration'lari / elle versiyonlu SQL / Rust tarafinda `refinery` — OQ-1'e bagli | Phase 0 |
| OQ-3 | Porcupine lisans modeli ve maliyeti kisisel kullanimda ne? | Ucretsiz kota yeterli mi, custom wake word egitimi neyi gerektiriyor | Phase 0 |
| OQ-4 | Porcupine hangi tarafta calisacak? | Rust binding (mikrofon Rust'ta) vs Web SDK (mikrofon webview'de) — mikrofon izni ve idle ses akisi buna bagli | Phase 0 |
| OQ-5 | Agents SDK'nin WebRTC transport'u Tauri WKWebView'inde sorunsuz calisiyor mu? | `getUserMedia` izinleri, WKWebView WebRTC destegi — Phase 1'in en buyuk teknik riski | Phase 0 |
| OQ-6 | Mikrofon iki tuketici arasinda nasil paylasilacak? | Wake word motoru ve Realtime session ayni cihazi ayni anda tutabilir mi; devir teslim protokolu | Phase 0/2 |
| OQ-7 | Ephemeral token OpenAI'dan hangi endpoint/akisla alinacak? | Guncel resmi dokumana gore dogrulanacak; SDK'nin onerdigi akis tercih edilir | Phase 0/1 |
| OQ-8 | Styling katmani ne olacak? | CSS Modules / Tailwind / minimal custom — UI ana urun degil, en az bakim gerektiren secilir | Phase 1 |
| OQ-9 | DB sifreleme (SQLCipher) ne zaman? | MVP'de duz dosya kabul; sifreleme Phase 3 sonrasi — OQ-1 secimi bunu bloklamamali | Phase 3+ |
| OQ-10 | Uygulama imzalama/notarization gerekli mi? | Mikrofon izni ve kalici kurulum icin macOS gereksinimleri | Phase 2+ |

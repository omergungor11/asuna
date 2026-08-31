# Session Notes

<!-- Sablon:

## [TARIH] — Session X

### Yapilanlar
- [x] ASU-XXX: [aciklama]

### Yarim Kalanlar
- [ ] ASU-YYY: [ne kaldi, nerede birakildi]

### Bir Sonraki Session
- [ ] [Yapilacak 1]

### Dikkat Edilecekler
- [Bug, workaround, karar bekleyen konu]
-->

## 2026-08-24 — Session 1 (Phase 0 + Phase 1, M1)

### Yapilanlar

**Phase 0 — arastirma + scaffold (11/11)**
- [x] ASU-001..004: pnpm workspace, Tauri 2 + React + TS (strict) + Vite scaffold, ESLint/Prettier/Vitest, CI yesil (macos-latest)
- [x] ASU-005: SQLite erisim karari → **B, rusqlite + Rust servis** (ADR-005 accepted); `tauri-plugin-sql` olcumle elendi
- [x] ASU-006: `@openai/agents-realtime` 0.17.0 + zod 4.4.3 exact pin
- [x] ASU-007: WKWebView WebRTC dogrulandi — R3 kapandi
- [x] ASU-008 + ASU-008b: wake word motoru **sherpa-onnx KWS** (ADR-004, kapsami daraltilmis accepted)
- [x] ASU-009: tipli config katmani — key sadece Rust tarafinda
- [x] ASU-010: `docs/architecture/*`, ADR dizini, README local run, RUNBOOK

**Phase 1 — realtime voice dikey dilimi (10/11)**
- [x] ASU-011: ephemeral realtime token minting (Rust)
- [x] ASU-012: `core.v1` prompt baseline
- [x] ASU-013: `AsunaRealtimeService` — SDK wrapper
- [x] ASU-014: voice state machine (gecersiz gecis politikasi: dev throw / prod reject)
- [x] ASU-015..018: "Talk to Asuna" butonu, iki yonlu ses + barge-in, canli transcript UI, temiz disconnect
- [x] ASU-019: observability — logger, state transition log, durust hata mesajlari, debug paneli
- [x] **ASU-020: M1 kabul testi CANLI TESTTE GECTI** — Turkce anlasildi, barge-in sorunsuz, temiz kapanis

### Kritik bulgular (bu oturumda ogrenildi)
- **Porcupine oldu** — Picovoice Free Tier 2026-06-30'da kapandi, `pv_porcupine` crate yanked. Wake word Rust tarafinda sherpa-onnx `KeywordSpotter`'a tasindi; `WakeWordProvider` arayuzu degismedi.
- **KWS model sorunu (R2)** — `gigaspeech-3.3M` sozlugunde `ASUNA` yok: ortografik tespit %0, en iyi fonetik workaround %81 @ 54.8 FA/saat. Motor/lisans/CPU/bundle YESIL. Model + ifade secimi ACIK.
- **`tauri-plugin-sql` elendi** — ACL komut duzeyinde (scope yok), path sandbox yok, transaction yok; renderer'dan `DROP TABLE` calisti.
- **`freezePrototype: true` → beyaz ekran** — zod v3 compat katmanindaki atama WebKit "override mistake" kuralina takiliyor. `false` yapildi, gerekce DECISIONS.md'de.
- **CSP prod tuzagi** — `connect-src`'a `https://api.openai.com` eklenmezse ses dev'de calisip paketlenmis build'de sessizce oluyor.
- **OpenAI kredi tuzagi** — ChatGPT aboneligi API kredisi vermiyor; `insufficient_quota` alindi, kullanici $5 kredi yukledi.
- **Vite dep re-optimize yarisi** — bagimlilik degisiminden sonra webview beyaz kalabiliyor; Cmd+R cozuyor.

### Devam eden isler
- [ ] **ASU-064**: realtime gecikme ayari — turn detection konfigurasyonu + olcum. M1'de fark edilir gecikme gozlendi. Suphe listesinde Cloudflare WARP acik olmasi da var (WebRTC yolu).
- [ ] **ASU-021**: `WakeWordProvider` interface + fake provider. Phase 2'nin model kararindan bagimsiz, paralel ilerliyor.

### Bir sonraki session
- [ ] **KWS gercek mikrofon testi (~30 dk) — KULLANICI GEREKLI.** Harness `spike/asu-008b-kws` branch'inde; TTS telaffuz artefakti olup olmadigi olculecek. Sonuc ADR-004'teki model + ifade secimini kapatir; Phase 2 buna bagli.
- [ ] **API key rotasyonu onerisi** — anahtar gelistirme boyunca `.env`'de dolasti; M1 sonrasi rotate edilmesi oneriliyor (RUNBOOK → Incident).
- [ ] **Phase 3 (memory) baslangici** — ASU-029'dan; plan hazir: `asuna-plans/plan-phase-3-memory.md`.

### Dikkat edilecekler
- Phase 2 wake word model karari verilmeden ASU-022 baslamaz (R2 acik).
- Yeni `#[tauri::command]` = 4 adim (build.rs manifest + `permissions/` + capability + `tauri.conf.json`); atlanirsa sessiz red.
- `src-tauri/permissions/` dizini olusturuldugu an TUM uygulama komutlari ACL'e tabi olur — Phase 3'te gecis adimi olarak planlandi.

## 2026-08-25 — Session 1 devami (otonom)

- Bilgi grafi kuruldu (graphify, 21 doc stabil cekirdek; 145 node / 11 topluluk;
  `graphify query` agent promptlarina girdi).
- Phase 3 tamamen yazildi: ASU-029..037 (5 dalga, 8 opus agent) + Gate 3 review
  (opus reviewer). Review: 1 CRITICAL (runtime hafiza anahtari oturum+ozet yolunu
  durdurmuyordu), 1 HIGH (saklanan metinde secret redaksiyonu yoktu) + 7 orta/dusuk —
  hepsi duzeltildi. 296 Rust + 529 TS testi.
- Yeni backlog: ASU-065 (oturum ozeti + transcript temizligi), sunucu tarafli sayfalama.
- Kullaniciyi bekleyen: ASU-038 M3 sesli kabul testi, gecikme A/B (eagerness=high + WARP),
  KWS gercek mikrofon testi (Phase 2 kilidi).

## 2026-08-25 — Session 1 kapanis kaydi (sonraki session devri)

**Durum:** Phase 0+1+3 tamam (M1 gecti; M3 sesli kabul ASU-038 bekliyor). Phase 4 kod tamam
(ASU-046 sesli kabul bekliyor). Phase 5: Wave A+B TAMAM — ASU-047 (registry), ASU-050 (audit),
ASU-048 (approval matrisi), ASU-049 (path sandbox, 31 kotu yol vakasi). 540 Rust + 789 TS
testi yesil, hepsi push'lu. Uygulama KAPALI (agent rebuild'leri pencereyi acip kapatiyordu).

**Sonraki session ilk adimlar:**
1. **Wave C**: ASU-051 (read_project_file — migration 005 `outcome` kolonu dahil, karar
   DECISIONS'ta; sandbox sozlesmesi ASU-049 raporunda: `resolve_in_project` + `read_text` +
   `audit_outcome`) + ASU-052 (open_project) backend; ASU-053 (approval UI — API sozlesmesi
   ASU-048 raporunda: `pendingApproval`/`approveTool`/`rejectTool`) + ASU-054 (tools sekmesi)
   frontend. Sonra ASU-055 otomatize guvenlik testleri + IKINCI Gate 3 review (guvenlik odakli).
2. **Kullanici manuel test kuyrugu** (tek seansta): M3 Bolum 3 tekrari (hafiza + oturum sil →
   "hatirlamiyorum"), ASU-038 kapanisi, ASU-046 (proje sesli testi), ASU-055 sesli maddeleri.
   Test icin `pnpm tauri dev`.
3. **Phase 2 kilidi kullanicida**: KWS gercek mikrofon testi (~30 dk, `spike/asu-008b-kws`).
4. Milestone'da graphify tazele (korpus kopyalari eskidi — scratchpad silinmis olabilir,
   yeniden kopyala).

**Islenecek acik konular:** ASU-066 (Cmd+Q finalize yarisi — task-index'e HENUZ islenmedi),
gecikme A/B (eagerness=high aktif, WARP kapali denenmedi), API key rotasyonu onerisi,
`docs/architecture/tools.md`'ye onay akisi bolumu (ASU-048 ekleyemedi — dosya kilitliydi).

## 2026-08-31 — Session 2 (Phase 5 Wave C)

### Yapilanlar

**Phase 5 Wave C — ASU-051..054 DONE** (opus agent dalgasi + Gate 3 review + duzeltmeler)

- [x] **ASU-051 `read_project_file`** (risk 0): Rust `projects/files.rs`; ASU-049 sandbox'i +
      blocklist, **once redaksiyon sonra 6000 karakter kirpma**, modele yalnizca
      `SandboxedPath::relative()` doner, `truncated`/`redacted` bayraklari ciktida yazili.
      "Kacis denendi" / "dosya turu kapali" / "bulunamadi" tipli olarak ayri sunulur.
- [x] **ASU-052 `open_project`** (risk 1): Rust `projects/editor.rs`; yeni **zorunlu**
      `ASUNA_EDITOR_COMMAND` (bos = `code`, bosluk/metakarakter **acilista** reddedilir),
      `Command::new(cmd).arg(path)` — shell yok, `env_remove(OPENAI_API_KEY)`, hedef yalnizca
      `active` current proje, `last_opened_at` **spawn'dan sonra** tazelenir.
- [x] **Migration 005**: `tool_events.outcome` (`succeeded`/`failed`/`not_run`, NULL'lu, geriye
      donuk doldurma YOK), sema surumu 5. `ToolResult.auditSummary` alani: modele giden metin ile
      deftere gideni tip duzeyinde ayirdi.
- [x] **ASU-053 onay karti**: karar `requestId` ile, geri sayim UI'da ama **zaman asimini servis
      tetikler**, kart `document.body`'ye portal — her sekmede gorunur.
- [x] **ASU-054 Araclar sekmesi**: tool listesi + oturum-yerel toggle, salt-okunur audit gecmisi
      (oturum filtresi sunucuda), transcript'te `role: 'tool'` satiri, `TOOL_PENDING` gorunurlugu.
- [x] Dokumantasyon bugune getirildi: `task-index` (dashboard yeniden hesaplandi, ASU-066 acildi),
      `CHANGELOG` Phase 5 blogu, `docs/architecture/tools.md` (Bolum 3 "Onay akisi" eklendi,
      audit/gorunurluk/TODO gercekle eslendi), `asuna-config/testing.md` ASU-055 manuel senaryosu.

### Gate 3 review (opus reviewer)

1 CRITICAL + 2 HIGH + 3 MEDIUM + 3 LOW. Kapatilanlar: **C1/H1** (toggle dikisi — `toSdkTool`
`executeTool`'a `isToolEnabled`/`onToolResult` gecirmiyordu; kapali tool acik oturumda calisiyor
ve basarili cagrilar transcript'e hic dusmuyordu), **M1** (kazara onay yolu: odak "Reddet"te,
onaylayan kisayol yok), **M2** (tanim listesi + toggle seti tek kaynak, App'ten prop),
**M3**, **L1**. Temiz bulunanlar: sandbox butunlugu, enjeksiyon yuzeyi, ACL 4 adim (+8 regresyon
testi), audit outcome etiketleri.

**Ders:** uclari ayri ayri test etmek yetmiyor — **dikisi** test etmek gerek. Zincirin iki ucu
testliydi, aradaki tek satir eksikti ve derleyici yakalayamadi (tum binding alanlari opsiyonel).
7 yeni dikis testi tool'u uretimdeki yoldan (ham JSON argumanla) cagiriyor.

**Durum:** typecheck / lint / clippy / fmt temiz, **907 TS + 592 Rust** testi yesil. Commit YOK.

### Kullaniciyi bekleyenler (tek seansta, `pnpm tauri dev`)

- [ ] **Gate 3 H2 — ACIK, tek satir**: repo kokundeki `.env`'e `ASUNA_EDITOR_COMMAND=code` ekle.
      Yoksa uygulama acilista `ConfigError::Missing` ile durur. `code` bulunamazsa
      `which code` ciktisindaki tam yolu yaz (macOS GUI process'inde PATH dar olabilir).
- [ ] **ASU-055** sesli maddeler — senaryo `asuna-config/testing.md` → "M4 kabul senaryosu"
      (A1..A11: proje sorusu, README okuma, uydurmama, onay/red/timeout, `.ssh` + `.env` reddi,
      bozuk editor komutu, oturum ortasinda tool kapatma, audit salt-okunurlugu).
- [ ] **ASU-038** (M3 restart sonrasi hatirlama) + **ASU-046** (Phase 4 proje sesli testi) —
      ayni seansta kapatilabilir.
- [ ] **KWS gercek mikrofon testi (~30 dk)** — `spike/asu-008b-kws` branch'i. **Phase 2 kilidi**;
      ADR-004'teki model + ifade secimi buna bagli.

### Acik kalanlar

- **ASU-066** artik `task-index.md`'de (Phase 3, backend, M, PENDING): Cmd+Q ile cikista
  `session_finalize` (ozet + cikarim) tamamlanmadan process olebilir. Detay kod incelemesi ister.
- **Gecikme A/B**: `ASUNA_VAD_EAGERNESS=high` aktif; **Cloudflare WARP kapali** deneme henuz
  yapilmadi (WebRTC yolu suphelisi).
- **API key rotasyonu** onerisi duruyor — anahtar gelistirme boyunca `.env`'de dolasti
  (RUNBOOK → Incident).
- **Overlay penceresi yok** → onay istegi ana pencere kapaliyken gorunmuyor (backlog).

## 2026-08-31 — Session 2 devami (Phase 5 Wave D)

### Yapilanlar

**Phase 5 Wave D — ASU-067..070 DONE** (opus agent dalgasi + guvenlik odakli Gate 3 + duzeltmeler)

Cikis noktasi tasarim tahmini degil, **M4 canli testinde gorulen gercek bosluklardi**: kullanici
UI'dan proje ekledi ama Asuna'nin registry'ye bakacak hicbir tool'u yoktu
(`get_current_project` yalnizca **tek** projeyi gorur), ve "freelancer klasorunde ne var?"
cevaplanamadi cunku `read_project_file` dosya **adini** bilmek zorunda.

- [x] **ASU-067 `list_projects`** (risk 0): mevcut `project_list` komutu sarildi, yeni Rust
      yuzeyi yok. "Guncel proje" TS tarafinda `registry::current` SQL'inin aynasi olan saf bir
      fonksiyonla turetiliyor. Deftere yol degil **sayi** yazilir.
- [x] **ASU-068 `list_project_files`** (risk 0) + `list_project_dir` komutu: tek seviye listeleme
      (ozyineleme YOK), kendi capability'si (`asuna-project-dir-list`) — dosya okumadan ayri.
      Iki tavan ayri raporlanir: 200 girdi cikti (`truncated`) ve 5000 girdi tarama
      (`scanCapped`). Bloklu girdiler **gorunur ama isaretli** ve boyutsuz; dosya icerigi hicbir
      kosulda donmez. `sandbox::resolve_project_root` eklendi (kok'un kendisi dizin hedefi icin).
- [x] **ASU-069 `register_project`** (**risk 2**, her modda onay): `project_add` sarildi, sema tek
      alanli (`path`) — model proje adi uyduramaz.
- [x] **ASU-070 `set_current_project`** (risk 1, onayli): model **ad** verir, tool kimligi cozer
      (tam eslesme + Turkce yerel kucultme); belirsizlikte **secim yapmaz**.
- [x] Kayitlar: `task-index` (dashboard 52/72, ASU-067..071 eklendi), `CHANGELOG` Wave D bloklari,
      `DECISIONS` iki yeni karar, `testing.md` A12..A18, `MEMORY.md` firmlink gotcha'si.

### Gate 3 review (opus reviewer, guvenlik odakli)

1 CRITICAL + 1 HIGH + 3 MEDIUM + 2 LOW; hepsi kapatildi.

- **C1 (CRITICAL) — kok kayit dogrulamasi atlatilabiliyordu.** `refuse_unsuitable_root` "ev
  dizininin kendisi"ni ve sistem dizinlerini **tam eslesme** ile kontrol ediyordu. Iki bypass:
  (a) bir **ata** dizin (`/Users`, `/`, `/System/Volumes/Data`) tek kayitla butun kullanici
  agacini okunabilir alana sokuyordu; (b) macOS **firmlink**'i
  (`/System/Volumes/Data/Users/<ad>/Library`) ayni dizinin ikinci kanonik yolu oldugu icin
  `~/Library` oneki tutmuyordu — arkasinda `~/.config/gh/hosts.yml` token'i, Application Support,
  `.zsh_history`, yani blocklist'in **ada gore yakalamadigi** seyler. Duzeltme uc parcali: ata
  reddi + `/System|/Library|/Applications|/Network` on-ek reddi + firmlink normalizasyonu
  (`strip_data_volume`, butun karsilastirmalardan once). `/private` ve `/var` bilincli olarak tam
  eslesme kaldi (gecici dizinler + mesru `/Volumes` kokleri). Gerekce DECISIONS'ta.
- **H1** — `matchProjects` kimlik eslesmesinde belirsizligi yutuyordu (kimlikler adlarin slug'i,
  yani ayri isim uzayi degil). Artik iki kume birlestiriliyor → `ambiguous_project`.
- **M1** — onay karti yolu 64 karakterde **sonundan** kesiyordu; yol gibi gorunen degerler icin
  ayri tavan (160) + **ortadan** kirpma. C1 ile birlesince denetlenecek tek ucu gizliyordu.
- **M2** — `read_dir` sinirsiz tuketiliyordu (`MAX_SCANNED_ENTRIES = 5 000` + `scanCapped`).
- **M3 (orchestrator karari)** — `register_project` risk 1 → **risk 2**: kalici yetki genisletmesi
  "dusuk risk" etiketi tasiyamaz, ayrica risk 2+ tanimlar `requiresApproval` olmadan **kayit
  edilemiyor** (koruma ayara degil tanima baglandi).
- **L1/L2** kapandi (bloklu girdide boyut donmuyor; yaniltici yorum duzeltildi).

**Wave D oncesine ait acik da bu incelemede bulundu:** `project_add` ev dizinini ve `~/.ssh`'i
**kabul ediyordu**. UI akisinda daha az onemliydi; tool yuzeyi acildigi anda kritik. Ret Rust
tarafinda ve `project_add`in butun cagiranlarini (UI dahil) kapsiyor — renderer'a guvenilmedi.

**Durum:** clippy temiz, **730 Rust + 834 TS (scoped)** testi yesil. Commit'ler: `df16a11`
(Wave D), `56ebd69` (asagidaki sizinti duzeltmesi).

### Paralel oturum koordinasyonu — chat kabugu

Repoda ayni anda ikinci bir oturum (**chat kabugu**) calisti. Anlasma: dosya sinirlari —
`asuna-plans/plan-chat-shell.md`, `src/app`, `src/components`, `src/shared/chat.ts`,
`src-tauri/src/chat.rs` ve db dosyalari o oturuma ait; Wave D bunlara dokunmadi ve **chat-shell
task'lari task-index'e islenmedi** (o oturum kendi kaydini yapar).

- **Sizinti + duzeltme:** `df16a11` paralel oturumun commit'lenmemis `pub mod chat` satirini
  yanlislikla icerdi; `chat.rs` commit'te olmadigi icin `main` derlenmiyordu. `56ebd69` satiri
  geri aldi — chat kabugu kendi commit'iyle yeniden ekleyecek. **Ders:** paralel oturumda
  `git add -A` degil, dosya listesiyle stage.
- **Acik sozlesme kirilmasi:** `ProjectDirectoryView` artik `scanCapped` tasiyor;
  `src/components/composer.spec.tsx:36` fixture'ina `scanCapped: false` satiri eklenmeli.
  Dosya oteki oturuma ait oldugu icin **dokunulmadi** → `pnpm typecheck` o tek hatayla kirmizi.
  Chat kabugu commit'i ile kapanmali.

### Kullaniciyi bekleyenler (guncel kuyruk, tek seansta `pnpm tauri dev`)

- [ ] **ASU-071** — Wave D sesli maddeleri: `asuna-config/testing.md` → M4 senaryosu **A12..A18**
      ("hangi projelerim var", "su klasoru ekle" onay kartiyla + red, `/Users` kaydettirme reddi,
      "icinde ne var", belirsiz ad, olmayan proje). En az iki kayitli proje gerekiyor; A17 icin
      ikisinin adi buyuk/kucuk harf disinda ayni olmali.
- [ ] **ASU-055** (A1..A11), **ASU-038** (M3 restart), **ASU-046** (Phase 4) — onceki kuyruk
      duruyor; ayni seansta kapatilabilir.
- [ ] **Gate 3 H2** — `.env`'e `ASUNA_EDITOR_COMMAND=code` satiri (hala acik, yoksa acilista durur).
- [ ] **KWS gercek mikrofon testi (~30 dk)** — `spike/asu-008b-kws`. **Phase 2 kilidi**.

**Uygulama KAPALI** — chat kabugu oturumu commit'ini atip `composer.spec.tsx` fixture'ini
duzeltince acilacak (o ana kadar typecheck kirmizi).

## 2026-08-31 — Session 3 (Chat Shell pivotu, paralel oturum)

### Karar: PIVOT (kullanici, 2026-08-31)

Asuna ChatGPT/Claude-tarzi bir **kalici konusma** arayuzune donustu. CLAUDE.md'nin "generic
chatbot UI kurma" prime directive'i **degistirildi**; ses silinmedi, **voice mode** olarak kaldi
(`VoicePanel` asla unmount edilmez). Karar + gerekce + degerlendirilen alternatifler:
`asuna-docs/DECISIONS.md` → **ADR-008**.
**Numara notu:** `asuna-plans/plan-chat-shell.md` bu karari "ADR-006 olacak" diye anar; ADR-006 ve
ADR-007 2026-08-24'te alinmisti, dogru numara **ADR-008**. Plan icindeki referans eski.

### Yapilanlar — ASU-072..078 DONE

- [x] **ASU-072** migration 006: `messages` + `attachments` (STRICT), `sessions.title`/`modality`,
      `session_id` uzerinden **CASCADE**. Yeni "conversations" tablosu **acilmadi** — konusma
      mevcut `sessions` satiridir (ozet/`end_reason`/`project_id`/`source_session_id` zaten orada).
- [x] **ASU-073** Rust `chat.rs`: `chat_send` (son 40 mesaj + ekler, non-streaming, kullanici +
      asistan mesaji **tek transaction**), `attachment_ingest` (ad blocklist +
      `redact_sensitive_text` + 24k kirpma), `attachment_from_project` (mevcut sandbox'li
      `projects::files::read` **cekirdegi** — kopyalama yok; V1: yalnizca **aktif** proje).
      Yeni **zorunlu** env `ASUNA_CHAT_MODEL`.
- [x] **ASU-074** TS sozlesmesi: `src/shared/chat.ts` + `src/asuna/agent/chat-service.ts`.
- [x] **ASU-075** UI kabugu: sidebar (tarih gruplu konusma listesi) + `chat-view` + `composer` +
      `project-file-picker`. `VoicePanel` monte kaliyor.
- [x] **ASU-076** ACL: yeni capability `asuna-chat.json`; **`message_append` bilerek kayitsiz**
      (renderer asistan mesaji uyduramaz), `session_set_title` `asuna-session.json`'da.
- [x] **ASU-077** tester: +70 test; redaksiyon desen bosluklari bulundu (PEM / `AKIA` / `ghp_` /
      JWT) ve kapatildi.
- [x] **ASU-078** Gate 3: **0 CRITICAL**, 1 HIGH (ses oturumu rozeti) + 4 MEDIUM + 7 LOW;
      HIGH/MEDIUM kapatildi, secilen LOW'lar backlog'a. **736+ Rust / 1091+ TS** testi yesil.

### Paralel oturum koordinasyonu (iki oturum ayni repoda)

- **Wave D (ASU-067..070) ve migration 007 diger oturumun isiydi**; bu oturum onlara dokunmadi.
  ASU-071 numarasi Wave D sesli kabul testine gitti (commit `c9f24be`), bu yuzden Chat Shell
  task'lari **ASU-072**'den basliyor.
- Anlasma: dosya sinirlari + `git add -A` degil **dosya listesiyle** stage (Wave D commit'i
  `df16a11` bir kez commit'lenmemis `pub mod chat` satirini sizdirdi, `56ebd69` geri aldi).

### Kullaniciyi bekleyenler

- [ ] **`.env`'e `ASUNA_CHAT_MODEL=gpt-4o-mini` satiri** — hook `.env`'e dokunamiyor; satir yoksa
      uygulama acilista `ConfigError::Missing` ile durur (`ASUNA_SUMMARY_MODEL` /
      `ASUNA_EDITOR_COMMAND` ile ayni desen).
- [ ] **ASU-079 — M6 kabul testi**: restart sonrasi konusma gecmisi + dosya ekleme (`.env` adi
      reddi, secret redaksiyonu) + projede konusma + voice mode'un bozulmadigi.
- [ ] Onceki kuyruk duruyor: ASU-071 (A12..A18), ASU-055 (A1..A11), ASU-038, ASU-046, KWS gercek
      mikrofon testi, `ASUNA_EDITOR_COMMAND=code`.

### Proje durumu — ASKIDA

Kullanici, paralel oturuma (**asuna-81**) projeyi askiya aldigini iletti ("beklenen seviyede
gelisme olmadi"). Bu oturumun kapanisi **toparla-ve-commit'le** modunda yapildi: Chat Shell isi
tamamlandi ve commit'lendi, **yeni is acilmadi**. Chat Shell tarafinda proje devam ederse ilk
adim **ASU-079 (M6 kabul testi)**.
Kaynak: kullanici → asuna-81 oturumu → orchestrator, 2026-08-31.
Askiya alma kararinin teshisi ve genel devam listesi asagida: *"Session 2 kapanis: PROJE ASKIYA
ALINDI"*.

## 2026-08-31 — Session 2 kapanis: PROJE ASKIYA ALINDI

Kullanici karari: "beklenen seviyede gelistirme yapilamadi" — sesli deneyim hedeflenen
"Jarvis" hissine ulasmadi. Somut sikayetler: Asuna surekli dosya yolu soruyor, aktif proje
klasorunu kendiliginden okumuyor.

**Teknik teshis (cozulmemis ama anlasilmis):** Wave C/D toollari testlerden gecti; sorun
buyuk olcude PROMPT katmaninda — `core.v2`'de "# Tools" bolumu yok, model toollari nasil
zincirleyecegini (once list_projects/list_project_files, sonra read_project_file; kullaniciya
yol sormak yerine kesfet) bilmiyor. Backend agent bunu Wave C'de isaretlemisti (core.v3
karari orchestrator'a birakildi); sesli deneyim tam bu eksige takildi. Ikinci etken:
dev'de her rebuild mikrofon iznini dusurdu (TCC + imzasiz debug binary), test akisi surekli
kesildi — RUNBOOK'a islendi/islenecek.

**Askidaki durum:** main temiz ve push'lu (c9f24be'ye kadar benim tarafim). Chat Shell
(asuna-dd oturumu) kendi duzeltmelerini tek commit'te toplayip push'layacak (ASU-072+).
730 Rust / 834+ TS testi yesil. 52/72 task DONE. M1 kapali; M3/M4 sesli kabulleri yarim
(oturum 10'da hafiza cikarimi calisti — 4 kayit; restart-hatirlama adimi tamamlanamadi).

**Devam edilirse ilk adimlar (oncelik sirasiyla):**
1. `core.v3` prompt — "# Tools" bolumu: kesfet-once-sor, tool zincirleme, aktif proje varsayimi.
2. Dev mikrofon izni kalici cozumu (stabil imza / RUNBOOK adimi).
3. M3+M4 sesli kabullerin tek seansta bitirilmesi (testing.md A1..A18 hazir).
4. ASU-066 (Cmd+Q finalize yarisi), pricing cached_tokens kalemi, Gate 3 L3/L4/L5.

> Guncelleme: Chat Shell pivotu **09692b2** ile main'e indi (ASU-072..079; 760 Rust + 1102 TS
> yesil; ADR-008). Kalan kullanici adimlari: proje devam ederse ASU-079 (M6 kabul) + core.v3.
> `.env`'e ASUNA_CHAT_MODEL=gpt-4o-mini bu oturumda EKLENDI (yapilacak is degil).

# Changelog

All notable changes to this project will be documented in this file.

<!-- Format:
## [TARIH]

### Added
- ASU-XXX: [eklenen ozellik]

### Changed
- ASU-YYY: [degisiklik]

### Fixed
- ASU-ZZZ: [bug fix]
-->

## [Unreleased]

### Added (2026-08-31 — Phase 5: Tools, Wave A+B+C; M4 kabul testi bekliyor)

- `AsunaToolDefinition` + tool registry: sozlesme kayit aninda zorlanir (snake_case ad,
  timeout 1..120 000 ms, risk 2/3 icin zorunlu onay), calistirma tek yoldan
  (`executeTool` — sema dogrulamasi + timeout + `AbortSignal` + yapisal `ToolResult`) (ASU-047)
- Risk/approval politikasi: risk x mod matrisi tek fonksiyonda (`resolveApproval`); risk 2/3
  konfigurasyonla atlanamaz, onay 60 sn'de cozulmezse varsayilan **reddet** (ASU-048)
- Path sandbox + hassas dosya blocklist (Rust): leksik `..` cozumu → `canonicalize`,
  tipli `SandboxViolation`, 256 KiB tavani, binary reddi; 31 kotu yol vakasi (ASU-049)
- `tool_events` tablosu + audit logger (migration 004): her cagri — onaylanan, reddedilen,
  hata veren, timeout olan — yazilir; argumanlar host tarafinda ozetlenip redakte edilir;
  append-only (ASU-050)
- `read_project_file` tool (risk 0): kayitli proje root'u icinde okuma, sandbox + blocklist,
  once redaksiyon sonra 6000 karakter kirpma; `truncated`/`redacted` bayraklari ciktida
  goruntulenir, modele yalnizca proje-goreli yol doner. "Kacis denendi" / "dosya turu kapali" /
  "bulunamadi" ayri ayri sunulur — model icerik uydurmaya davet edilmez (ASU-051)
- `open_project` tool (risk 1): projeyi konfigure edilmis editorde acar; yeni **zorunlu**
  `ASUNA_EDITOR_COMMAND` anahtari (bos = `code`), alt process `Command::new(cmd).arg(path)` ile
  shell'siz kurulur, `last_opened_at` yalnizca process gercekten baslatilinca tazelenir (ASU-052)
- Onay karti (`AWAITING_APPROVAL`): tool adi, insan diliyle ne yapilacagi, risk seviyesi,
  redakte edilmis argumanlar ve geri sayim; karar `requestId` ile verilir, kart `document.body`'ye
  portal edilir — sekme degistirilse de ekranda kalir (ASU-053)
- "Araclar" sekmesi: modele acik tool listesi (risk + onay politikasi), tool basina oturum-yerel
  ac/kapa, salt-okunur audit gecmisi + oturum filtresi (ASU-054)
- Transcript'te `role: 'tool'` satiri + `TOOL_PENDING` gorunurlugu: calisan tool'un adi, sonucu
  ve `outcome` etiketi akista gorunur; dosya icerigi transcript'e girmez (ASU-054)
- `tool_events.outcome` kolonu (migration 005, sema surumu 5): `succeeded` / `failed` /
  `not_run`. "Calisti mi" (`approval_state`) ile "basardi mi" ayri eksenler; eski satirlar
  `NULL` kalir, geriye donuk doldurma yapilmaz (ASU-051)
- `ToolResult.auditSummary`: modele giden metin ile deftere/transcript'e giden ozet tip
  duzeyinde ayrildi — dosya icerigi audit'e girmez (ASU-051)

### Security (2026-08-31 — Phase 5 Gate 3 review)

- **Kazara onay yolu kapatildi** (ASU-053 / Gate 3 M1): onay karti acildiginda odak **"Reddet"**
  butonunda; tek klavye kisayolu `Esc` = reddet, **onaylayan kisayol yok**. Ilk surumde Enter
  onayliyordu ve kart tam kullanici Enter'a basarken acilirsa risk 1+ bir aksiyon refleksle
  onaylanabiliyordu. Onay artik yalnizca kasitli bir eylemle verilir.
- **Tool kapatma dikisi tamir edildi** (ASU-054 / Gate 3 C1+H1): `toSdkTool` icindeki
  `executeTool` cagrisi calisma zamani kancalarini (`isToolEnabled`, `onToolResult`)
  gecirmiyordu — acik bir oturumun ortasinda kapatilan tool calismaya devam ediyor ve basarili
  cagrilar transcript'e hic dusmuyordu. Kapatma artik iki katmanli: baglanista liste suzulur,
  her cagrida `isEnabled` yeniden sorulur (reddedilen cagri `not_run` olarak deftere gecer).
  7 yeni "dikis" testi tool'u uretimdeki yoldan (ham JSON argumanla) cagiriyor.
- Tool tanim listesi ve toggle anahtar seti kompozisyon kokunde (App) tek kez kurulur ve ayni
  ornekler hem oturuma hem "Araclar" sekmesine verilir (Gate 3 M2) — ekranda "Kapali" gorunen
  bir tool'un calisiyor olmasi mumkun degil.


### Added (2026-08-25 — Phase 3: Memory, kod tamam; M3 kabul testi bekliyor)

- SQLite bootstrap + migration altyapisi, `memories`/`sessions` semasi (ASU-029/030)
- MemoryService CRUD + okuma/yazma ayrik ACL (ASU-031)
- Oturum kaydi + opsiyonel transcript persist — kapaliyken diske hicbir sey yazilmadigi
  dosya-sistemi taramasiyla testli (ASU-032)
- Session summary pipeline (`ASUNA_SUMMARY_MODEL`, ayri text-model cagrisi) + `end_reason`
  migration'i (ASU-033)
- Memory extraction: dogrulama, deterministik dedup, onem esigi, hassas kategorilerde
  `pendingApproval` bekletme (ASU-034)
- Stage A deterministik retrieval + `SessionBootstrapContext` → prompt enjeksiyonu;
  bos hafizada "hatirliyormus gibi davranma" satiri (ASU-035)
- Memory UI (listele/ara/sil/arsivle) + Ayarlar sekmesi: runtime gizlilik anahtarlari,
  "tum hafizayi sil" (cift onay + phrase), onay bekleyenler kuyrugu (ASU-036/037)
- Gate 3 review duzeltmeleri: runtime gizlilik kapisi tum yazma yollarinda, saklanan
  metinde secret redaksiyonu (`redaction.rs`), dedup esigi 40 + %80 oran, dosya izinleri
  0600/0700, 200 kayit tavani gorunur


### Added
- **Phase 1 — realtime voice dikey dilimi (ASU-011..ASU-020).** Butona basilir, Turkce konusulur,
  Asuna sesle cevap verir, sozu kesilebilir, transcript gorunur, oturum temiz kapanir:
  - ASU-011: ephemeral Realtime token minting (Rust) — `OPENAI_API_KEY` renderer'a hic girmiyor,
    webview kisa omurlu `ek_` token ile baglaniyor.
  - ASU-012: `core.v1` prompt baseline (`src/asuna/prompts/`), versiyonlu; aktif surum tek noktadan secilir.
  - ASU-013: `AsunaRealtimeService` — `@openai/agents-realtime` wrapper; SDK degisimi tek dosyada izole.
  - ASU-014: voice state machine — gecersiz gecis dev'de `throw`, prod'da `reject`; sessiz yutma yok.
  - ASU-015..ASU-018: "Talk to Asuna" butonu + baglanti akisi, iki yonlu ses gorunurlugu +
    barge-in tepkisi, canli transcript UI, temiz disconnect + kaynak temizligi (mikrofon gostergesi soner).
  - ASU-019: observability — logger (secret redaksiyonu), state transition log, durust hata
    mesajlari, debug paneli.
- Proje Asuna spec'ine gore sekillendirildi: `PROJECT.md` (urun/mimari spec, 40 bolum),
  `TRANSCRIPT.md` (urun niyeti) ve `asuna-docs/AGENT-SPEC-ORIGINAL.md` (coding agent kurallari)
  kaynak gercek olarak repoya alindi.
- Gelistirme plani ve Claude Code agent sistemi kuruldu: Fable orchestrator + `opus` subagent
  modeli, `ASU-XXX` task ID formati, `feat(ASU-XXX): aciklama` commit formati.
- `asuna-docs/DECISIONS.md`: ADR-001..ADR-007 kaydedildi — Tauri 2 desktop shell, OpenAI Agents SDK
  (`RealtimeAgent`/`RealtimeSession`) + WebRTC ses mimarisi, `ASUNA_REALTIME_MODEL` ile model
  konfigurasyonu, `WakeWordProvider` arkasinda wake word motoru, SQLite persistence
  (erisim katmani ACIK — proposed), Tauri Rust tarafinda ephemeral token minting,
  ve Claude Code gelistirme modeli.
- `asuna-docs/MEMORY.md`: proje ozeti, spec dosyalarinin yeri, Phase 0 durumu ve ihlal edilemez
  kurallar (idle ses buluta gitmez, API key renderer'a girmez, model ID config'de) yazildi.
- `.env.example`: PROJECT.md Bolum 23'teki yapilandirma degiskenleri + wake word ayarlari
  (`ASUNA_WAKE_WORD_PROVIDER`, `ASUNA_WAKE_WORD_MODEL_DIR`, `ASUNA_WAKE_WORD_THRESHOLD`),
  her degisken icin aciklama satiri ile eklendi.

- ASU-010: `docs/architecture/` altina `memory.md`, `tools.md` ve `security.md` iskeletleri
  eklendi (Phase 0 bulgulariyla dolu, kalan maddeler TODO tablolarinda). `README.md`'ye
  "Local Kurulum" bolumu geldi: gereksinimler, `.env`, OpenAI API billing notu
  (ChatGPT aboneligi API kredisi vermez), KWS model dosyalarinin indirilmesi, komut tablosu.
- ASU-010: `asuna-docs/DECISIONS.md` en uste "Phase 0 ozeti" tablosu — ADR-001..007 tek
  satirlik ozetleri + `docs/decisions/` altindaki detayli ADR-004/005 linkleri.

### Changed
- ASU-010: `asuna-docs/RUNBOOK.md` template kalintilarindan (Docker/staging/health endpoint)
  temizlenip Asuna gercegine gore yeniden yazildi: `pnpm tauri dev` / `pnpm tauri build`,
  GitHub Actions (`ci.yml`), `git revert` + yeniden build ile geri alma, DB dosyasi konumu ve
  WAL yedekleme (`VACUUM INTO`). "Deploy" kavrami yok; release ASU-063'te.
- ASU-008: **Wake word: Porcupine → sherpa-onnx KWS** (ADR-004 revize; Picovoice Free Tier
  2026-06-30'da kapandi, non-commercial tier yok, `pv_porcupine` crate yanked, AccessKey init'te
  online dogrulaniyor). Motor artik Tauri'nin **Rust** process'inde (`cpal` + `KeywordSpotter`);
  implementasyon adi `SherpaKwsProvider`, `WakeWordProvider` arayuzu degismedi.
  `PICOVOICE_ACCESS_KEY` kaldirildi. Calisan detection spike'i ASU-008b'ye ayrildi.

### Fixed
- ASU-020: **`freezePrototype: true` beyaz ekran.** Paketlenmis/webview calistirmada uygulama hic
  render etmiyordu; zod v3 compat katmanindaki `errorUtil.toString = ...` atamasi donmus
  `Object.prototype` yuzunden WebKit'in "override mistake" kuraline takilip
  `TypeError: Attempted to assign to readonly property` firlatiyordu (Chromium'da gorunmuyor).
  `freezePrototype: false` yapildi; gerekce ve kabul edilen risk `asuna-docs/DECISIONS.md`
  → *Phase 1 uygulama kararlari*.
- ASU-007: prod CSP `connect-src`'a `https://api.openai.com` eklendi — dev'de gorunmeyen,
  paketlenmis build'de sesi sessizce olduren blocker.

### Notes
- **M1 milestone 2026-08-24'te canli testte gecti** (ASU-020): kullanici Turkce konustu, Asuna
  anladi ve cevapladi, barge-in sorunsuz calisti, oturum temiz kapandi. Testte fark edilir bir
  gecikme gozlendi → **ASU-064** (turn detection konfigurasyonu + olcum) acildi.
- Phase 0 yeniden yorumlandi: "template audit" tamamlandi sayiliyor (denetlenecek uygulama kodu yoktu);
  Phase 0 = teknik arastirma + scaffold. Tamamlandi: Tauri 2 iskeleti, CI yesil, ADR-001..007.
- Acik soru "SQLite erisim katmani" **kapandi** (ASU-005): Rust tarafi servis (`rusqlite`),
  `docs/decisions/ADR-005-sqlite-access.md` accepted.
- Acik kalan: wake word model + ifade secimi (ADR-004, R2) — gercek mikrofon testi bekliyor.

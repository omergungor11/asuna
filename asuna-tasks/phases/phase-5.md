# Phase 5: One Useful Action (Tools)

> **Hedef:** Tek bir gercek, faydali bilgisayar aksiyonu — onay katmani, sandbox ve audit ile birlikte.
> "Asuna, bu projeyi ac" calissin ve her sey kayit altinda olsun.
>
> **Milestone:** M4'un ikinci yarisi — "ilk tool".
>
> **Onkosul:** Phase 4 ASU-046 gecmis olmali.
>
> **Ilke (PROJECT.md Bolum 18):** `run_any_shell_command` gibi sinirsiz bir tool **asla** modele
> acilmaz. Once read-only, sonra dar kapsamli aksiyonlar.

---

## ASU-047: `AsunaToolDefinition` + Tool Registry

**Scope**: backend | **Boyut**: L | **Durum**: DONE | **Bagimlilik**: ASU-044

### Aciklama
PROJECT.md Bolum 17. ASU-044'te dogrudan tanimlanan tool bu registry'ye tasinir.

### Acceptance Criteria
- [x] `AsunaToolDefinition`: name, description, risk (0|1|2|3), requiresApproval,
      input schema, `execute(args, ctx): Promise<ToolResult>`
- [x] Registry: kayit, listeleme, isimle cozme; ayni isim iki kez kayit olamiyor
- [x] Her tool cagrisinda argumanlar semaya gore dogrulaniyor; gecersizse calistirilmiyor
- [x] Her tool'un timeout'u var; asili kalan tool oturumu kilitlemiyor
- [x] `ToolResult` yapili: basari/hata, ozet, opsiyonel veri — serbest metin degil
- [x] `AsunaRealtimeService` tool listesini registry'den aliyor (ASU-013'te birakilan API kullanildi)
- [x] ASU-044 `get_current_project` registry'ye tasinmis, davranisi bozulmamis
- [x] Unit testler: sema reddi, timeout, cift kayit

### Notlar (2026-08-25)
- `src/asuna/tools/registry.ts`: `ToolRegistry` (`register` / `list` / `resolve` / `has` / `size`)
  + `executeTool(definition, args, context, options?)`. Sozlesme **kayit aninda** zorlanir:
  snake_case ad, min aciklama uzunlugu, 1..120 000 ms timeout, risk 2/3 icin
  `requiresApproval: true`. Gecersiz tanim modele hic acilmaz.
- Sema `AsunaToolDefinition.parameters` alaninda ve **tek kaynak**: hem `executeTool`
  dogrulamasi hem SDK'ya giden JSON Schema ayni zod object'inden uretilir. Parametresiz
  tool'lar `NO_TOOL_ARGUMENTS` (`z.strictObject({})`) kullanir — uydurma parametre
  sessizce atilmaz, reddedilir.
- `executeTool` hicbir kosulda `throw` etmez; her yol yapisal `ToolResult` doner.
  `errorKind` sabitleri `TOOL_ERROR_KINDS`: `invalid_arguments` (execute **cagrilmadan**),
  `timeout`, `aborted`, `tool_failed`. Sema hatasi ozetine reddedilen **deger** yazilmaz.
- Timeout `Promise.race` + `AbortController`: sure dolunca `ToolContext.signal` abort edilir
  ve cagri yapisal `timeout` sonucuyla doner ("bitmedi", "basarisiz oldu" degil — arkadaki is
  kendiliginden durmaz). `realtime-service.ts` adaptoru SDK'nin iptal sinyalini
  `options.signal` ile sarmalayiciya devreder.
- Registry SDK'dan bagimsiz kaldi: `src/asuna/tools/` altinda `@openai/*` import'u yok
  (`sdk-import-boundary.spec.ts` degismeden gecti). `toSdkTool` artik `definition.parameters`
  kullaniyor ve `execute` govdesinde `executeTool`'u cagiriyor; `use-asuna-session`
  `DEFAULT_ASUNA_TOOLS` yerine `asunaToolRegistry.list()` geciyor.
- Gate'ler: `pnpm typecheck`, `pnpm lint`, `pnpm test` (43 dosya / 681 test), `pnpm format:check`
  yesil. Rust'a dokunulmadi.
- ASU-048 kancasi: `executeTool` icinde sema dogrulamasindan **sonra**, `run()` cagrisindan
  **once** tek bir onay kapisi yeri var (risk + `requiresApproval` + mod → onayla/reddet).
  ASU-050 audit yazimi da ayni sarmalayicidan beslenir (giris/cikis tek noktada).

---

## ASU-048: Risk / Approval Policy Katmani

**Scope**: backend | **Boyut**: M | **Durum**: DONE | **Bagimlilik**: ASU-047

### Aciklama
PROJECT.md Bolum 5.4 risk seviyeleri + `ASUNA_TOOL_APPROVAL_MODE`.

### Acceptance Criteria
- [x] Risk 0: onaysiz calisiyor (iki modda da; `requiresApproval: true` diyen bir tanim
      yine de sorulur — tanim sikilastirabilir, gevsetemez)
- [x] Risk 1: `ASUNA_TOOL_APPROVAL_MODE` ile konfigurabilir (safe modda onay ister)
- [x] Risk 2 ve 3: **her zaman** acik onay istiyor; konfigurasyonla atlanamiyor
- [x] Onay bekleyen tool `AWAITING_APPROVAL` durumunu tetikliyor
- [x] Onay zaman asimina ugrarsa tool **calismiyor** (varsayilan reddet) — 60 sn,
      hem serviste (otomatik `reject`) hem `executeTool` kapisinda
- [x] Onay karari tool cagrisi basina; "hepsine izin ver" MVP'de yok
      (`alwaysApprove`/`alwaysReject` SDK secenekleri **kullanilmiyor**)
- [x] Politika kararlari unit test edilmis (her risk seviyesi x her mod matrisi)

### Notlar (2026-08-25)

**Karar matrisi** — `src/asuna/tools/approval-policy.ts`, `resolveApproval(risk, requiresApproval, mode)`:

| Risk | `requiresApproval` | `safe` | `always` |
|------|--------------------|--------|----------|
| 0 | `false` | onaysiz | onaysiz |
| 0 | `true` | ONAY | ONAY |
| 1 | herhangi | ONAY | ONAY |
| 2 / 3 | herhangi | ONAY | ONAY |

Uc karar ve gerekceleri:

1. **Risk 2/3 mod tablosuna bakmadan** doner — `ASUNA_TOOL_APPROVAL_MODE` bu satirlari
   gevsetemez (`security.md` Bolum 3).
2. **`always` modunda risk 0 da onay istemez.** Kabul kriteri kosulsuz ("Risk 0: onaysiz
   calisiyor") ve sesli bir oturumda `get_current_project` icin kart cikarmak onay
   yorgunlugu uretir — asil onemli olan risk 2/3 karti da refleksle onaylanir.
   `always`'in anlami "risk 0'i da sor" degil, **kilit**: ileride gevsetici bir mod
   eklense bile hicbir riski otomatik gecirmez.
3. **Mevcut iki mod risk 1'de ayni davranir** ve bu dokumante edilmis bir durum, kesfedilecek
   bir surpriz degil (`approval-policy.spec.ts` bunu ayrica olcuyor). Fark, gevsetici bir mod
   (`auto`/`trusted`) eklendiginde ortaya cikar; degisecek tek yer `RISK_1_NEEDS_APPROVAL`
   tablosu ve tablo `Record<ToolApprovalMode, …>` oldugu icin yeni mod eklemek orada derleme
   hatasi verir. Bu yuzden `auto_approved` audit durumu **su an uretilmiyor** — etiketleme
   fonksiyonu (`approvalStateFor`) yine de dogru degeri veriyor ki o mod geldiginde defter
   "gerekmiyordu" diye yalan soylemesin.

**Iki katmanli uygulama.** (a) SDK katmani: `toSdkTool` icinde `needsApproval` artik statik
boolean degil politika fonksiyonu; `true` donunce SDK `execute`'u **hic** cagirmaz, once
`tool_approval_requested` cikar. (b) Calistirma kapisi: `executeTool` ayni karari bagimsiz
olarak yeniden hesaplar ve `options.approvalGate` ile **kanit** sorar. Serviste kanit
`approveToolCall` ile yazilir ve tek cagri icin gecerlidir; kanit yoksa kapi `denied` doner.
Yani onay akisini atlayan bir cagri (SDK politikasi yanlis baglansa bile) calismaz.

**Varsayilan = CALISTIRMA.** `approvalGate` verilmemisse onay gerektiren tool calismaz
(`not_requested`), gate firlatirsa `denied`, gate 60 sn icinde cozulmezse `timeout`.
`TOOL_ERROR_KINDS.denied` bu ucunu modele tek bicimde anlatir ("yapmadim"); ayrimi audit
tasir. Tool'un kendi `timeoutMs` sayaci **onaydan sonra** baslar — kullanicinin dusunme
suresi tool'un calisma butcesini yemez.

**Audit (ASU-050 ile birlesim).** `executeTool` her cikis yolunda `onAudit`'i tam bir kez
cagirir (sema reddi, onay reddi, timeout, hata, basari). Onay **hic calismadan** reddedildiginde
satiri servis yazar (`denied`/`timeout`), onaylandiginda ikinci satir yazilmaz — cagri zaten
calisip kendi `approved` satirini uretir. Oturum kapanirken cevaplanmamis kalan istekler de
`denied` olarak deftere gecer, sessizce dusmez.

**`sessionId` korelasyonu (ASU-050 notu #2 kapandi).** `ToolContext.sessionId` artik
`number | null` ve gercek `sessions.id` degerini tasiyor: `SessionRecorder.currentSessionId`
-> `useAsunaSession` -> `AsunaRealtimeService.resolveSessionId` -> `ToolContext` + audit satiri.
Her cagrida yeniden okunur (`session_start` asenkron doner); bilinmiyorsa alan **gonderilmez**,
uydurulmaz.

**Durum makinesi.** `ASSISTANT_THINKING -> AWAITING_APPROVAL` ve
`ASSISTANT_SPEAKING -> AWAITING_APPROVAL` kenarlari eklendi: onay bekleyen tool `TOOL_PENDING`'e
ugramaz, cunku SDK onay verilene kadar `execute`'u cagirmaz — "calisiyor" durumundan gecmek
olmayan bir isi olmus gibi gostermek olurdu.

**Gate'ler:** `pnpm typecheck`, `pnpm lint`, `pnpm test` (46 dosya / 789 test), `pnpm format:check`
yesil. Rust'a dokunulmadi (onay tamamen renderer/SDK katmaninda).

### ASU-053 icin API sozlesmesi

`useAsunaSession` dondurur:
- `pendingApproval: PendingToolApproval | null` — `{ requestId, toolName, description, risk,
  argumentsPreview, timeoutMs, requestedAtMs }`. Kart "izin ver?" demek yerine ne yapilacagini
  gosterebilsin diye tool aciklamasi + risk + **redakte edilmis** arguman ozeti birlikte gelir;
  geri sayim `requestedAtMs + timeoutMs`.
- `approveTool(requestId)` / `rejectTool(requestId)` — karar **kimlikle** verilir; "sonuncuyu
  onayla" yok.

Servis event'leri: `tool_approval_requested` (yukaridaki alanlar) ve `tool_approval_resolved`
(`{ requestId, toolName, outcome: 'approved' | 'denied' | 'timeout' }`). Zaman asimini UI
tetiklemez, yalnizca gosterir: otomatik reddetme serviste.

---

## ASU-049: Path Sandbox + Hassas Dosya Blocklist

**Scope**: backend | **Boyut**: M | **Durum**: DONE | **Bagimlilik**: ASU-047, ASU-040

### Aciklama
PROJECT.md Bolum 19 "Filesystem sandbox". Bu, guvenlik modelinin en cok test edilmesi gereken parcasi.

### Acceptance Criteria
- [x] Tum dosya erisimleri kayitli proje root'una gore normalize edilip cozuluyor
      (`security::sandbox::resolve_in_project`; kok listesi **yalnizca** registry'den, `active` kayitlar)
- [x] Path traversal reddediliyor: `../../.ssh/id_ed25519`, mutlak yol, `~` genislemesi, sembolik link
      ile disari cikma — her biri **ayri** `SandboxViolation` varyanti
- [x] Blocklist: `.env*`, `*.pem`, `*.key`, `id_rsa*`, `id_ed25519*`, `.ssh/`, keychain, credential
      dosyalari, `.git/config` (dosya **komple** bloklu)
- [x] Maksimum dosya boyutu siniri (`MAX_READABLE_FILE_BYTES` 256 KiB, asan **reddedilir**);
      binary dosya reddi (ilk 8 KiB'de NUL / %10 kontrol bayti / gecersiz UTF-8)
- [x] Reddedilen erisim sessizce bos donmuyor — tipli `SandboxViolation` +
      `audit_outcome()` → `(approval_state: not_requested, result_summary)`
- [x] **Kapsamli unit test seti** — `case_01`..`case_31`, 31 kotu yol vakasi (min. 15) +
      4 pozitif kontrol; gercek temp dizin ve gercek symlink
- [x] `docs/architecture/security.md` sandbox kurallarini anlatiyor (Bolum 6 yeniden yazildi;
      T3 kapandi, `tools.md` T4/T5/T6 kapandi)

### Uygulama Notlari

**Cozum sirasi.** `..` bilesenleri **leksik** olarak, dosya sistemine sorulmadan cozulur;
`canonicalize` ondan **sonra** gelir. Iki kazanci var: (1) var olmayan bir yol icin de karar
verilebilir — "dosya yok" ile "kacis denendi" ayni gorunmez; (2) `link/../x`, `link`in
**hedefinin** ustune degil kok icindeki `x`e cozulur. Kabuk semantigi bilerek terk edildi,
daha kisitlayici yorum secildi. `canonicalize` yine de sart: leksik cozum symlink gormez.

**Yol yoksa.** Aday canonicalize edilemiyorsa **var olan en yakin ataya** kadar geri sarilir,
canonicalize edilir ve kalan (leksik olarak temizlenmis, `..` icermeyen) bilesenler eklenir.

**Percent-encoding decode EDILMEZ.** `%2F` cozmek "hangi katman kac kez cozer?" sorusunu acar.
Ham metin tek bir dosya adi bileseni olur: kokten kacamaz, yalnizca anlamsizlasir ve okuma
`not_found` ile duser (`case_26` bunu assert eder).

**Kontrol sirasi: traversal > blocklist.** `../../.ssh/id_ed25519` → `Traversal`,
`Blocklisted` degil. Kacis, adin ne oldugu sorulmadan **once** karara baglanir.

**Blocklist cozulmus TAM yol uzerinde.** Kok'un kendi bilesenleri de taranir: `~/.ssh` ya da
`~/secrets/x` proje olarak kaydedilse bile altindaki dosyalar `Blocklisted` doner (`case_19`).
Bilincli yanlis pozitif.

**Tip duzeyinde koruma.** `SandboxedPath` yalnizca `resolve_in_*` ile uretilebilir
(`RegisteredRoot` ile ayni desen); bir fonksiyon `&Path` yerine bu tipi aliyorsa kontrolun
yapildigi derleme zamaninda okunur.

### ASU-041/042'de yapilan degisiklik (dikkat)

`.git/config` blok listesine girdigi icin `projects::context` o dosyayi **artik acmiyor**:
`CONTEXT_SOURCES`ten cikarildi, `SourceKind::GitConfig` ve `git_remote_from_config` silindi.
Remote adinin **tek** kaynagi artik ASU-042'nin `git remote get-url origin` ciktisi;
`projects::view::collect` onu `sanitise_remote_url`'den gecirip kayda isliyor,
`context::current` yalnizca kayitli degeri yansitiyor. Iki ayri turetme yolu tutmak zaten
ikisinin zamanla ayrisma riskiydi. Regresyon testi: `view::tests::
the_git_remote_reaches_the_summary_through_the_git_cli_path` (gercek `git init` + token'li
remote URL; token cikti JSON'unda yok).

### ASU-051 icin entegrasyon sozlesmesi

```rust
let path = sandbox::resolve_in_project(db, &project_id, &args.path)?;   // Result<SandboxedPath, _>
let file = sandbox::read_text(&path)?;                                   // Result<SandboxedFile, _>
// hata yolunda:
let SandboxAuditOutcome { approval_state, result_summary } = violation.audit_outcome();
```

- `SandboxedFile.text` **kirpilmamis**. Kirpma ASU-051'in isi ve butcesi
  `MAX_READABLE_FILE_BYTES`in **altinda** olmali; kirpildigi ciktida soylenmeli.
- Modele/UI'a giden yol metni `SandboxedPath::relative()` — mutlak yol **donulmez**.
- `violation.is_escape_attempt()` "kacis denendi" ile "dosya yok"u ayirir; ikisi kullaniciya
  ayni sekilde sunulmamali.
- Sandbox komut **acmaz**; ACL yuzeyi ASU-051/052'nin isi.

---

## ASU-050: `tool_events` Tablosu + Audit Logger

**Scope**: db | **Boyut**: M | **Durum**: DONE | **Bagimlilik**: ASU-047, ASU-030

### Acceptance Criteria
- [x] `tool_events`: id, session_id, tool_name, risk_level, arguments_redacted, approval_state,
      result_summary, created_at (PROJECT.md Bolum 12.2) — migration 004, `STRICT`, sema surumu 4
- [x] **Her** tool cagrisi yaziliyor: onaylanan, reddedilen, hata veren, timeout olan
      (`approval_state` alti degeri de yaziliyor; ACL testi hepsini ucdan uca olcuyor)
- [x] Argumanlar redakte ediliyor; dosya icerigi ve secret'lar audit'e girmiyor
      (ozetleme + redaksiyon **host tarafinda**; ic ice yapilar yalnizca sekil olarak yazilir)
- [x] Audit yazimi basarisiz olursa bu durum gorunur oluyor (sessiz kayip yok)
      — Rust tipli hata + `audit.ts` `{ status: 'failed', error }` + `error` seviyesinde log
- [x] Audit kayitlari UI'dan gorunebiliyor (Tools sekmesi veya oturum detayi)
      — **veri yolu hazir**: `tool_event_list` (oturum filtreli, tavanli) + `listToolEvents()`;
      ekranin kendisi ASU-054'un isi
- [x] Audit kayitlari uygulamadan silinemiyor (MVP'de salt yazilir) — silme/guncelleme komutu YOK;
      `session_delete` de silmez (FK `ON DELETE SET NULL`)
- [x] Unit test: redaction, reddedilen cagrinin da yazilmasi

### Uygulama Notlari

**Sema (migration 004, geri alinabilir).** `session_id` FK'si bilerek `ON DELETE SET NULL`:
`CASCADE` olsaydi "konusma gecmisini sil" dugmesi ayni zamanda audit defterini silen bir
primitif olurdu ve "audit silinemez" kriteri dolayli olarak delinirdi. Uzunluk tavanlari
(`tool_name` 64, arguman/sonuc ozeti 512) semada da CHECK olarak yazili — Rust kirpmayi bir gun
atlarsa satir INSERT aninda duser.

**`approval_state` kumesi (6 deger).** `not_required` (risk 0), `auto_approved` (ayar izin verdi),
`approved`, `denied`, `timeout` (varsayilan reddet), `not_requested` (onay asamasina hic
gelinmedi — sema reddi, bilinmeyen tool, on-kontrol). `not_required` ile `not_requested` ayri:
birinde onay GEREKMEDI, otekinde onay SORULAMADI.

**Arguman ozeti bicimi.** Tek satir, alfabetik `anahtar=deger`. Metinler 64 karakterde kirpilir,
sayilar/bool oldugu gibi, **dizi ve nesne yalnizca sekil** (`[3 oge]` / `{2 alan}`). Ic ice icerik
hicbir zaman serilestirilmez — "dosya icerigi audit'e girmez" bir uzunluk tahminine degil bicimin
kendisine bagli. Sonra `redact_sensitive_text` + 512 karakter tavani.

**Append-only kilidi uc katmanda.** Repository'de `DELETE`/`UPDATE` yolu yok (kaynak metnini
okuyan test), `EXPOSED_COMMANDS` icinde `tool_event` iceren yalnizca iki komut var (statik test),
ACL'de `tool_event_delete` vb. deny-by-default ile duser (regresyon testi).

### ASU-051 / ASU-052 icin acik nokta — **KAPANDI** (migration 005)

12.2'nin kolon listesinde **yapisal bir basari/hata alani yok**; `result_summary` metni ikisini de
tasiyor. `approval_state` "calisti mi"yi soyler ama "calisti ve basarili miydi"yi soylemez.

Orchestrator karari alindi (DECISIONS.md "Phase 5 kararlari") ve ASU-051 kapsaminda uygulandi:
migration 005 `tool_events.outcome TEXT CHECK (outcome IS NULL OR outcome IN
('succeeded','failed','not_run'))`. Sema surumu **5**.

- **NULL'a izin var ve geriye donuk doldurma YOK.** 004 doneminde yazilmis satirlarda bu bilgi
  yoktu ve cikarilamaz (`approved` bir satirin basarili bittigini soylemez). 002'nin `end_reason`
  doldurmasindan farki tam da bu: orada durum kaydin kendisinden okunabiliyordu.
- **Iki eksen bagimsiz.** `approved` + `failed` gecerli ve sik: kullanici izin verdi, is calisti
  ve patladi. Kumeler kesismiyor (`ToolApprovalState` ile `ToolOutcome`) — bir satiri okurken
  hangi sorunun cevabini gordugumuz belirsiz kalmasin diye testle kilitli.
- **`failed` ile `not_run` ayrimi yan etki sorusudur.** Tool CALISTI ve yapamadiysa `failed`
  (sandbox reddi, timeout, editor bulunamadi); `execute` HIC cagrilmadiysa `not_run` (sema reddi,
  onay reddi/zaman asimi, kapatilmis tool, baslamadan iptal).
- Zincir: `.sql` → Rust `ToolOutcome` → `shared/tool-event.ts` `TOOL_OUTCOMES`; ucu de testle
  bagli (`migrations::outcomes_declared_in_schema`, `schema-mirror.spec.ts`).

---

## ASU-051: `read_project_file` Tool (Risk 0, Sandbox'li)

**Scope**: backend | **Boyut**: M | **Durum**: DONE (UI baglandi; sesli maddeler ASU-055'te) | **Bagimlilik**: ASU-049, ASU-050

### Acceptance Criteria
- [x] Tool kayitli proje root'u icindeki bir dosyayi okuyor
- [x] ASU-049 sandbox'i ve blocklist'i uyguluyor
- [x] Buyuk dosya kirpiliyor ve kirpildigi cikti'da belirtiliyor
- [ ] Cikti UI'da gorunuyor (hangi dosya okundu)
      — **veri yolu hazir**: `tool_result` event'i + `TranscriptLine` `role: 'tool'` varyanti;
      ekranin kendisi ASU-054'un (frontend) isi
- [x] Her cagri `tool_events`'e yaziliyor
- [ ] Sesli test: "Bu projenin README'sinde ne yaziyor?" gercek icerikten cevapliyor
- [ ] Var olmayan dosyada Asuna icerik uydurmuyor
      — mekanizma yerinde ve test edildi (`not_found` ayri kod + ozet "UYDURMA" diyor);
      **davranis** kanitini sesli test verecek

### Uygulama Notlari

**Kok secimini renderer yapmaz.** Komut `project_id` **almaz**: hedef her zaman
`registry::current`. Sozlesmede `project_id` parametresi vardi, bilerek daraltildi —
renderer'in kayitli kokler arasinda dolasabilmesi, sandbox'in kapattigi yuzeyi
yandan acardi. Model de secim yapamaz: sema tek alanli (`path`) ve `strictObject`.

**Kirpma butcesi karakter, 6 000.** Donen metin **modele** gidiyor, dolayisiyla butce
bayt degil karakter. Deger `context::MAX_TOTAL_CONTEXT_CHARS` ile ayni: "modele bir
seferde ne kadar metin gider" sorusunun bu repoda zaten kabul edilmis cevabi. Sandbox
tavaninin (256 KiB) cok altinda — okuma reddi ile kirpma birbirinin yerine gecmiyor
(ASU-049 sozlesmesi). Kirpma sessiz degil: `truncated` + gercek `sizeBytes` + model
ciktisinda "tamamini gormedin" uyarisi.

**Icerik redaksiyondan geciyor.** Blok listesi hassas **dosyalari** kapatir ama siradan
bir kaynak dosyasina gomulmus token'i kapatmaz; donen metin `redact_sensitive_text`ten
gecer ve bir sey maskelendiyse `redacted: true` bunu **soyler** (sessizce degistirilmis
icerik, "gordugum sey bu muydu?" sorusunu cevapsiz birakirdi).

**`ToolResult.auditSummary` eklendi (yeni alan).** Bu tool modele **icerik** verir; ayni
metnin `tool_events.result_summary`e dusmesi migration 004'un acikca yasakladigi seydi
("dosya icerigi audit'e girmez"). Host 512 karakterde kirptigi icin sizinti kucuk olurdu
ama yine de sizintiydi. Yeni alan ayrimi tip duzeyinde aciyor: deftere (ve ASU-054
transcript satirina) ne yazilacagi tool'un bilincli karari, uzunluk tavaninin yan etkisi
degil. Verilmezse `summary` kullanilir — mevcut tool'larin davranisi degismedi.

**Uc ret ayri sunuluyor.** `escapeAttempt` (host'un karari, renderer hesaplamaz) →
"ERISIM REDDEDILDI"; `blocklisted` → "bu dosya turu kapali, kural gevsetilemez";
`not_found` → "BULUNAMADI, icerik UYDURMA". Ucunu tek bir "okuyamadim" kovasina
indirgemek, modelin en olasi kacamagini (icerik uydurmak) davet ederdi.

**`approval_state` sandbox'in onerdigi degil registry'nin karari.** `SandboxAuditOutcome`
`not_requested` oneriyor (o sozlesme Rust tarafinda calisan bir tool runner varsayimiyla
yazilmisti); gerceklesen mimaride tool risk 0 ve onay **gerekmedi**, yani dogru etiket
`not_required`. Sandbox'in `result_summary`si oldugu gibi kullaniliyor (redaksiyondan
gecmis, yol tasimayan tek satir). Sandbox reddi `outcome: 'failed'` — tool **calisti** ve
okuyamadi; `not_run` demek yan etkisi olabilecek bir cagriyi "hic olmadi" saymak olurdu.

---

## ASU-052: `open_project` Tool (Risk 1)

**Scope**: backend | **Boyut**: M | **Durum**: DONE (onay karti baglandi; sesli maddeler ASU-055'te) | **Bagimlilik**: ASU-048, ASU-050

### Aciklama
PROJECT.md Bolum 32 Phase 5'in acik hedefi: "open current project in editor".

### Acceptance Criteria
- [x] Kayitli bir projeyi konfigure edilmis editorde aciyor (varsayilan VS Code)
- [x] Editor komutu konfigurabilir; bulunamazsa **durust hata**:
      "Projeyi acmayi denedim ama VS Code komutu bulunamadi" (PROJECT.md Bolum 30)
- [ ] Risk 1 -> safe modda onay UI'si cikiyor
      — **politika + event yerinde ve test edildi** (`resolveApproval` her modda
      `needs_approval`, `tool_approval_requested` yayinlaniyor); kartin kendisi ASU-053
- [x] Sadece kayitli proje yollari acilabiliyor (keyfi yol acilamiyor)
- [x] Basari/hatasi `tool_events`'e yaziliyor
- [x] `last_opened_at` guncelleniyor
- [x] Shell enjeksiyonuna kapali (yol arguman olarak gecirilir, string birlestirme ile komut kurulmaz)

### Uygulama Notlari

**Renderer hicbir sey secemez.** Komut argument **almaz**: ne acilacak yol, ne proje,
ne editor. Hedef `registry::current` (yalnizca `active`), komut `ASUNA_EDITOR_COMMAND`.
Modelin "hangi programi calistirayim?" diye bir parametresi olsaydi bu, adi
`open_project` olan bir genel komut calistiricisi olurdu (PROJECT.md Bolum 18).

**Shell yok — iki katmanda.** (a) Alt process `Command::new(editor).arg(path)` ile,
**arguman vektoru** olarak kurulur; komut metni string birlestirmeyle uretilmez. Test
gercek bir alt process kosturur: dizin adi `proje; rm -rf $HOME && echo pwned` iken
sahte editor **tek** arguman gorur ve dizindeki kanit dosyasi yerinde kalir.
(b) `ASUNA_EDITOR_COMMAND` bosluk ya da kabuk metakarakteri iceremez — `code --wait`
gibi bir deger **acilista** reddedilir. Ikincisi guvenlik icin gerekli degil (kabuk
yok, calisamazdi); amaci belirsizligi kapatmak: kullanici sessizce "code --wait" adinda
bir dosya aranmasini degil, net bir hata gormeli.

**Cocuga anahtar mirasi kapali.** `.env` okuyucusu `std::env::set_var` cagirmadigi icin
`OPENAI_API_KEY` zaten process environment'inda olmayabilir (`security.md` "dotenvy yok"
karari tam bu senaryo icindi); kullanici kendi kabugunda export etmisse diye alt process
`env_remove(OPENAI_API_KEY)` ile aciliyor. stdio `null`: editor Asuna'nin log akisina
yazamaz.

**Once baslat, sonra kaydet.** `last_opened_at` yalnizca process gercekten baslatildiktan
sonra tazeleniyor — tersi olsaydi bulunamayan bir editorde "en son acilan proje" degisir,
yani olmayan bir olayin izi kalirdi. Test bunu ayrica olcuyor.

**Cikis kodu beklenmiyor.** GUI editoru saatlerce acik kalir; `wait()` sesli oturumu
kilitlerdi. Cikti bu yuzden "baslatildi" der, "kullanici gordu" demez.

**Tanim kendi onayini istiyor** (`requiresApproval: true`), yalnizca risk 1'e guvenmiyor:
`resolveApproval` bir tanimin talebini gevsetemez, dolayisiyla ileride risk 1'i otomatik
geciren bir mod eklense bile bu tool sorulmaya devam eder.

### ACIK — orchestrator/kullanici aksiyonu

`ASUNA_EDITOR_COMMAND` **zorunlu anahtar** olarak eklendi (`ALL_KEYS`; `.env.example`
guncel, bos deger = `code`). Repo kokundeki mevcut `.env` bu satiri **icermiyor**, yani
eklenmeden `pnpm tauri dev` acilista `ConfigError::Missing` ile durur (ASU-033 LOW-11 ile
ayni sinif). Tek satir: `ASUNA_EDITOR_COMMAND=code`.

---

## ASU-053: Approval UI

**Scope**: frontend | **Boyut**: M | **Durum**: DONE (sesli maddeler ASU-055'te; overlay penceresi backlog'da) | **Bagimlilik**: ASU-048

### Acceptance Criteria
- [x] `AWAITING_APPROVAL` durumunda net bir onay karti gorunuyor
      (`tool-approval-card.tsx`; durum rozeti de "Onay bekliyor" der)
- [x] Kart gosteriyor: tool adi, ne yapacagi (insan diliyle), risk seviyesi, redakte edilmis argumanlar
- [x] Onayla / Reddet butonlari; klavye ile de erisilebilir (odak "Reddet"e gelir,
      Esc = reddet; **onaylayan kisayol yok** — bkz. Gate 3 M1)
- [x] Onay bekleme suresi gorunur; zaman asiminda otomatik reddediliyor
      (geri sayim UI'da, **reddetme serviste** — ASU-048)
- [ ] Asuna onay beklerken konusmaya devam edip "yaptim" demiyor — sesli test
- [ ] Reddedilen aksiyon sonrasi Asuna durumu dogru anlatiyor — sesli test
- [ ] Overlay modda da onay istegi gorunuyor (ana pencere kapaliyken kaybolmuyor)
      — **ayri overlay penceresi henuz yok**, bkz. not

### Notlar (2026-08-31, frontend)

`src/components/tool-approval-card.tsx` + `tool-text.ts`; `voice-panel.tsx`'ten baglandi.

- **Karar `requestId` ile.** Buton ve klavye ayni `decide()` yolundan gecer, ikinci karar
  gonderilmez (buton `disabled`). "Sonuncuyu onayla" yolu yok.
- **UI zaman asimi tetiklemez.** Geri sayim `requestedAtMs + timeoutMs`'ten saniyede bir
  cizilir; sure dolunca kart hicbir sey cagirmaz (`sure dolunca kendisi reddetmez` testi).
  Iki tarafin ayri saatlerle ayni karari vermesi istenmedi.
- **Onaylayan klavye kisayolu YOK (Gate 3 / M1).** Ilk surumde kart odagi kendine aliyor ve
  Enter onayliyordu; kart tam kullanici Enter'a basarken acilirsa risk 1+ bir aksiyon
  refleksle onaylanabilirdi. Simdi acilista odak **"Reddet"** butonunda, tek kisayol
  `Esc` = reddet. Onay yalnizca kasitli bir eylemle verilir (butona tiklamak ya da butona
  Tab'layip Enter). Kaza ile basilan tusun sonucu her zaman "calistirma" yonunde —
  ASU-048'in varsayilani da reddetmek.
- **Overlay:** `tauri.conf.json` tek pencere tanimliyor (`main`), ayri bir overlay penceresi
  yok. Kart bu yuzden `document.body`'ye **portal** edilir ve `position: fixed` durur: Hafiza /
  Projeler / Araclar / Ayarlar sekmesine gecildiginde Konusma paneli `hidden` olsa da onay
  istegi ekranda kalir. "Ana pencere kapaliyken gorunur" maddesi overlay penceresi (Phase 6 /
  ayri task) gelmeden karsilanamaz — kapatilmadi.
- Karar sonrasi kart kapanir, yerinde 6 sn'lik bir durum satiri kalir (`Reddettin — araç
  çalıştırılmadı.` / zaman asimi / oturum kapandi). Transcript'e **girmez**; oradaki tool
  satiri ASU-054'un isi.
- Testler: `tool-approval-card.spec.tsx` (15) + `voice-panel.spec.tsx`'e 4 test
  (kart gorunumu, portal hedefi, onay/red kimligi, `TOOL_PENDING` rozeti).

---

## ASU-054: Tool Call Gorunurlugu

**Scope**: frontend | **Boyut**: M | **Durum**: DONE (uctan uca dogrulama ASU-055'te) | **Bagimlilik**: ASU-047, ASU-050

### Aciklama
PROJECT.md Bolum 19: "The user should never wonder whether the agent is silently modifying the computer."

### Acceptance Criteria
- [x] Tool calisirken `TOOL_PENDING` durumu ve tool adi gorunuyor (rozet + "Aktif araç" satiri)
- [x] Tool sonucu (basari/hata + kisa ozet) transcript akisinda gorunuyor
      (`role: 'tool'` satiri: ad, ozet, `outcome` etiketi, risk)
- [x] Tools sekmesi: etkin tool listesi, risk seviyeleri, onay politikasi
- [x] Tool audit gecmisi goruntulenebiliyor (oturuma gore filtreli)
- [x] Tool tek tek devre disi birakilabiliyor (paylasilan `ToolToggleStore`)
- [ ] Gizli/gorunmez tool calistirma yolu yok (dogrulanmis) — UI tarafi dogrulandi
      (defterde silme/duzenleme yolu yok), uctan uca dogrulama ASU-055

### Backend sozlesmesi (ASU-051 ile birlikte yazildi)

`useAsunaSession()` **ek olarak** sunlari dondurur:

| Alan | Tip | Anlam |
|---|---|---|
| `tools` | `readonly ToolSummary[]` | Registry'den turetilir. `approval` alani ASU-048 matrisinin **ayni** fonksiyonundan (`resolveApproval`) gelir — ekranda yazan politika ile cagri aninda uygulanan politika ayrisamaz. `ASUNA_TOOL_APPROVAL_MODE` okunana kadar **en siki** mod (`always`) varsayilir. |
| `setToolEnabled(name, enabled)` | `void` | Oturum-yerel (bellekte; kalici ayar degil). |

`TranscriptLine` artik bir **birlesim**: `SpokenTranscriptLine` (`user`/`assistant`, alanlar
degismedi) + `ToolTranscriptLine` (`role: 'tool'`, `toolName`, `text`, `status: 'completed'`,
`interrupted: false`, `outcome`, `risk`, `approvalState`). `text`/`status`/`interrupted` her iki
varyantta da var, yani `role` ayrimini yapmayan mevcut tuketiciler kirilmaz.

**Kapatmanin iki katmani** ("gizli calistirma yolu yok" kriterinin kod karsiligi):

1. Kapali tool **modele verilen listeden dusurulur** — `AsunaRealtimeService` `connect()`
   sirasinda suzer.
2. `executeTool` **her cagrida** `isEnabled`i yeniden sorar. Ikincisi sart: SDK'ya verilen tool
   seti oturum boyunca **sabit**, yani acik bir oturumun ortasinda kapatilan tool'u model yine
   cagirabilir. O cagri calismaz (`errorKind: 'disabled'`) ve `tool_events`'e `not_run` olarak
   **yazilir**.

Sonuc: **acik oturumda** kapatma → "cagri reddedilir"; **sonraki oturumda** → "tool hic gorunmez".
Canli oturumun tool listesini guncelleyen bir `session.update` yolu **kullanilmadi** —
`RealtimeSessionPort` boyle bir yuzey acmiyor ve acmak SDK sozlesmesini genisletmek demekti.

**Gate 3 bulgusu C1/H1 — dikis kopmustu, tamir edildi + testle cakildi.** Zincirin iki ucu
(`App → ToolToggleStore → use-asuna-session → toolRuntime` ve `executeTool` icindeki kapi)
ayri ayri test edilmisti; **aradaki tek satir** eksikti: `toSdkTool` icindeki `executeTool`
cagrisi `runtime.isToolEnabled` ve `runtime.onToolResult`'i gecirmiyordu. `ToolRuntimeBindings`
alanlarinin hepsi opsiyonel oldugu icin derleyici bunu yakalayamaz ve sessiz sonuc tam olarak
sozun tersiydi: kapali tool acik oturumda **calisiyordu** (risk 0'da onay da yok, audit'e
`succeeded` dusuyordu) ve basarili hicbir cagri transcript'e **hic** dusmuyordu — `tool_result`
yalnizca onay reddi/timeout yolundan cikiyordu.

Ders kayda geciyor: "uclari test et" yetmiyor, **dikisi** test etmek gerek. Yeni
`toSdkTool — calisma zamani kancalarinin dikisi` bloku (7 test) tool'u uretimdeki yoldan
cagiriyor — SDK `execute`'umuzu `invoke` adiyla acar ve ham JSON arguman metni verir, yani
test parse + sema + kapi + audit + sonuc kancasi zincirinin tamamindan geciyor. Ikisi ucdan
uca: `AsunaRealtimeService`'in urettigi gercek `toolRuntime` ile. Dikis yeniden koparilirsa
bu blokta 5 test duser (dogrulandi).

**Transcript'e icerik girmez.** `tool_result` event'inin `summary` alani
`ToolResult.auditSummary` (yoksa `summary`) degeridir — "README.md okundu (2.1 KB, kirpildi)".
Dosya icerigi ne ekrana dokulur ne bellekte ikinci bir kopya olarak durur. Tool satirlari
**kalici dokume yazilmaz** (`transcript_lines` yalnizca `user`/`assistant` tanir); tool
cagrilarinin kalici kaydi `tool_events`.

### Notlar (2026-08-31, frontend)

`src/components/tools-view.tsx` + `app.tsx` "Araçlar" sekmesi; metinler `tool-text.ts`'te.

- **Sekme yalnizca acikken monte olur** (Hafiza/Ayarlar ile ayni kural): kapali sekme denetim
  defterini sorgulamaz. Onay karti bundan bagimsiz — ses panelinden portal edildigi icin bu
  sekme acikken de gorunur.
- **Audit salt okunur.** Ekranda silme/duzenleme dugmesi yok ve olmayacak; `tools-view.spec.tsx`
  bunu dugme etiketlerini tarayarak assert eder. "Salt okunurdur" cumlesi ekranda yazili.
- **Oturum filtresi sunucuya gider** (`listToolEvents({ limit, sessionId })`); istemcide
  kirpma yok — tavanli bir sayfayi istemcide filtrelemek kayit gizlerdi. Secenekler gorunen
  kayitlarin `sessionId` kumesinden turer; ayri bir "oturumlari listele" cagrisi yapilmaz.
- **`outcome` (migration 005) `null` olabilir** — kolon eklenmeden yazilmis satirlar.
  Etiket o zaman **basilmaz**; "başarılı" varsayilmaz. Dolu oldugunda `başarılı` / `hata` /
  `çalışmadı` etiketi ve `data-outcome` rengi cikar.
- **Tanim listesi VE anahtar seti kabukta kurulur (Gate 3 / M2).** `App` ikisini de bir kez
  olusturur ve **ayni ornekleri** hem `useAsunaSession` secenegine (`options.tools`,
  `options.toolToggles`) hem `ToolsView`'e (`definitions`, `toggles` — ikisi de **zorunlu**
  prop) verir. Iki ayri kaynak iki ayri yalan uretirdi: ayri store → ekranda "Kapalı" gorunen
  tool calismaya devam eder; ayri liste (sekme `asunaToolRegistry`'yi kendi okusa) → oturuma
  daraltilmis bir liste verildigi anda sekme modele **acik olmayan** tool'lari "Açık"
  gosterir. `ToolsViewPort` bu yuzden yalnizca audit okumasi tasir; tool listesi porttan
  gelmez.
- **Liste `buildToolSummaries` ile turer** — `useAsunaSession().tools` ile ayni fonksiyon ve
  ayni girdiler, yani ekrandaki politika ile cagri aninda uygulanan politika ayrisamaz. Onay
  modu config'ten okunana kadar **en siki** mod (`always`) varsayilir. Tanim listesi bossa
  ekran durustce "Modele açık bir araç yok." der.
- **Transcript tool satiri** artik hook'un `ToolTranscriptLine` varyantiyla tiplenir;
  `ROLE_LABELS` yeniden `Record<TranscriptLine['role'], string>` (yeni bir rol eklenirse
  derleme hatasi verir, etiketsiz satir degil).
- Testler: `tools-view.spec.tsx` (12), `tool-approval-card.spec.tsx` (16),
  `transcript-view.spec.tsx`'e 4 tool satiri testi, `voice-panel.spec.tsx`'e 4 test,
  `app.spec.tsx`'e sekme testi. Toplam paket: 51 dosya / 886 test yesil.

---

## ASU-055: M4 Kabul Testi + Guvenlik Unit Testleri

**Scope**: test | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-047..ASU-054

### Acceptance Criteria
- [ ] "Su an hangi projedeyim?" -> `get_current_project` calisiyor, UI'da gorunuyor
- [ ] "Bu projeyi VS Code'da ac" -> onay isteniyor, onaylaninca aciliyor, audit'e yaziliyor
- [ ] Ayni istek reddedilince proje **acilmiyor** ve Asuna acildigini iddia etmiyor
- [ ] "`~/.ssh/id_ed25519` dosyasini oku" -> reddediliyor, audit'e yaziliyor, icerik sizmiyor
- [ ] "`.env` dosyasini oku" -> reddediliyor
- [ ] Editor komutu kasitli bozulunca durust hata mesaji geliyor
- [ ] Sandbox unit test seti yesil (min. 15 kotu yol vakasi)
- [ ] Approval policy matris testi yesil
- [x] Manuel senaryo `asuna-config/testing.md`'ye eklenmis
      (→ "M4 kabul senaryosu — Phase 5 tool'lari", A1..A11 + on kosullar)

### Notlar
Bu, guvenlik modelinin ilk gercek sinavi. Bir madde bile gecmezse Phase 6'ya gecilmez —
reviewer agent + `asuna-config/security.md` checklist'i ile acik kapatilir.

---

# Wave D: Proje Farkindaligi Tool'lari (ASU-067..070)

> **Cikis noktasi:** M4 canli testinde ortaya cikan **gercek** bosluklar, tasarim
> tahmini degil. Kullanici UI'dan proje ekledi ama Asuna'nin registry'ye bakacak
> hicbir tool'u yoktu (`get_current_project` yalnizca **tek** projeyi gorur ve
> secim yoksa "bilmiyorum" der). Ayrica "freelancer klasorunde ne var?"
> cevaplanamadi: `read_project_file` dosya **adini** bilmek zorunda, model de ad
> uyduramaz.
>
> **Ilke degismedi:** once salt okuma. Wave D iki risk 0 tool ekliyor; kalan
> ikisi onayli aksiyon. Risk 3 hala **yok**.

Wave D sonrasi registry: **yedi tool** — `get_current_project`, `list_projects`,
`read_project_file`, `list_project_files` (risk 0); `open_project`,
`set_current_project` (risk 1); `register_project` (**risk 2** — Gate 3 M3,
asagida). Onaya tabi ucu de her modda sorulur.

---

## ASU-067: `list_projects` Tool (Risk 0)

**Scope**: backend | **Boyut**: S | **Durum**: DONE (sesli madde ASU-071'de) | **Bagimlilik**: ASU-047, ASU-040

### Acceptance Criteria
- [x] Tool kayitli projeleri donduruyor: ad, kimlik, yol, durum
- [x] Hangisinin **guncel proje** oldugu ciktida isaretli
- [x] Bos listede "kayitli proje yok" diyor ve proje uydurmayi yasakliyor
- [x] Yeni Rust yuzeyi acilmadi (mevcut `project_list` komutu sariliyor)
- [x] `tool_events`'e yol degil **sayi** yaziliyor
- [ ] Sesli test: "Hangi projelerim var?" gercek listeden cevapliyor

### Uygulama Notlari

**Yeni komut yok.** `project_list` zaten Projeler sekmesini besliyor (ASU-040,
`asuna-projects-read` capability'si) ve salt okuma: hicbir dizinin **icine**
girmez, yalnizca kayitli koklerin var olup olmadigini `stat` ile tazeler. Tool
onu sariyor; ACL yuzeyi genislemedi.

**"Guncel proje" TS tarafinda turetiliyor.** Semada bir `current` kolonu yok —
guncel proje = en son acilan kayitli proje (`registry::current` →
`most_recently_opened`). `pickCurrentProjectId` o SQL'in aynasi ve saf bir
fonksiyon olarak test ediliyor: `last_opened_at` dolu + `unlinked` degil, en
yeni once, esitlikte kimlik artan. Ikinci bir IPC (`project_context`)
yapilmadi — o komut dosya okuyup git calistiriyor, bir liste sorusu icin
fazlasiyla pahali.

**Yol modele oldugu gibi gidiyor.** `get_current_project` de guncel projenin
mutlak yolunu modele veriyor (ASU-044); listede farkli bir kural uygulamak ayni
bilgiyi iki ayri gizlilik seviyesinde tutmak olurdu. **Defter ayri:**
`auditSummary` yalnizca "N kayitli proje listelendi" — `tool_events` yol
gormez.

---

## ASU-068: `list_project_files` Tool (Risk 0) + `list_project_dir` komutu

**Scope**: backend | **Boyut**: M | **Durum**: DONE (sesli madde ASU-071'de) | **Bagimlilik**: ASU-049, ASU-051

### Acceptance Criteria
- [x] Tool guncel proje koku icindeki bir **dizini** listeliyor (ad, tur, boyut)
- [x] Bos `path` = proje koku
- [x] Dosya verildiginde tipli hata (`not_a_directory`), `not_found`'dan ayri
- [x] Kacis / `~` / mutlak / symlink → mevcut `SandboxViolation` yollari
- [x] Ozyineleme **yok**; uretilmis dizinler listede tek satir
- [x] 200 girdi tavani, asilirsa `truncated: true` ve kirpma ciktida yaziyor
- [x] Blok listesindeki girdiler **gorunur ama `blocked: true`**
- [x] Dosya **icerigi** hicbir kosulda donmuyor
- [x] 4 adim ACL + `acl_regression.rs` (kacis yollari + kapsam disi pencere)
- [x] `auditSummary` yalnizca "N girdi listelendi: <path>"
- [ ] Sesli test: "Bu klasorde ne var?" gercek listeden cevapliyor

### Uygulama Notlari

**Tek yeni Rust komutu:** `list_project_dir(path)` →
`src-tauri/src/projects/listing.rs`. Kendi capability'si var
(`asuna-project-dir-list`) ve `asuna-project-file-read`'ten **ayri**: dosya
okuma ICERIK dondurur, listeleme yalnizca AD/TUR/BOYUT. Ayni izin dosyasina
konsaydi "dizinleri gorebil ama dosya icerigi okuyamasin" (ya da tersi) diye bir
kurulum mumkun olmazdi. `commands.rs` bunu iki testle kilitliyor
(`project_directory_listing_has_its_own_capability`,
`the_directory_surface_is_read_only`).

**`sandbox::resolve_project_root` eklendi.** `resolve_in_root` kok'un kendisi
icin `Empty` doner — bir **dosya** hedefi icin dogru karar, bir **dizin** hedefi
icin degil. `resolve_in_project` artik yeni fonksiyonu cagiriyor; fonksiyon kok'u
canonicalize eder, dizin oldugunu dogrular ve **kok'u de blok listesinden
gecirir**. Bu bir politika degisikligi degil, var olan kuralin dizin yoluna
dusen yarisi (`sandbox.rs` modul dokumantasyonu: "blok listesi cozulmus tam yol
uzerinde calisir").

**Ozyineleme neden yok.** Tek seviye listeleniyor; alt dizini merak eden model
ayrica soruyor. Ozyineleme bagimlilik/build dizinlerine denk geldiginde sesli
bir oturuma on binlerce satir bosaltirdi ve "ne kadarini gordum?" sorusunu
bulaniklastirirdi. **Yan etkisi tam olarak istenen sey:** bu dizinler listede
tek satir olarak gorunur, icleri acilmaz. Ayrica **kendileri** icin bir istisna
yazilmadi — biri dogrudan listelenmek istenirse 200 girdi tavani devreye giriyor
ve kirpildigi soyleniyor.

**Blok listesindeki girdiler gizlenmiyor, isaretleniyor.** `.env` listede
gorunur ve `blocked: true` tasir. Gizlemek kullaniciyi "neden gormuyor?" diye
sasirtirdi; **isim bir sizinti degil**, icerik yolu (`read_project_file`) blok
listesi tarafindan kapali kalmaya devam ediyor ve bu modul hicbir dosya acmiyor.
`blocked` ayrica su iki durumu da kapsiyor: kok disina cikan / kirik symlink
(model okunabilir sanip cagri israf etmesin) ve UTF-8'e cevrilemeyen ad.

**Yarim liste yok.** Tek bir dizin girdisi okunamazsa cagri tipli olarak duser.
Yarim bir liste "bu dizinde bunlar var" diye sunulurdu ve eksigi gorunmezdi.

---

## ASU-069: `register_project` Tool (Risk 2, HER modda onay)

**Scope**: backend | **Boyut**: M | **Durum**: DONE (sesli madde ASU-071'de) | **Bagimlilik**: ASU-048, ASU-040

### Acceptance Criteria
- [x] Tool mevcut `project_add` komutunu sariyor (yeni Rust yuzeyi yok)
- [x] `resolveApproval` **her modda** `needs_approval` (tanimin kendi talebi)
- [x] Onay kartinda yol net gorunuyor (`path=/Users/...`)
- [x] Reddedilince hicbir kok kaydedilmiyor, "kaydettim" denmiyor
- [x] Host tarafinda ek dogrulama: ev dizini, `~/Library`, sistem dizinleri, blok listesi
- [x] Kayit guncel projeyi **degistirmiyor** ve ozet bunu soyluyor
- [ ] Sesli test: "Su klasoru projelerime ekle" onay isteyip ekliyor

### `project_add` dogrulama incelemesi (bulgu)

**ASU-069'dan ONCE `RegisteredRoot::resolve` sunlari dogruluyordu:**

| Kontrol | Durum |
|---|---|
| Bos / 4096 karakter tavani | vardi |
| `~` reddi (genisletme yok) | vardi |
| NUL bayti reddi | vardi |
| Mutlak yol sarti | vardi |
| `canonicalize` (var olma + symlink + `..` cozumu) | vardi |
| Dizin olma sarti | vardi |
| Filesystem koku (`/`) reddi | vardi |
| UTF-8 sarti | vardi |
| **Ev dizininin kendisi (`$HOME`)** | **YOKTU** |
| **`~/Library` ve alti** | **YOKTU** |
| **Sistem dizinleri (`/usr`, `/etc`, `/Applications` ...)** | **YOKTU** |
| **Blok listesindeki dizinler (SSH/cloud/secrets)** | **YOKTU** |

Yani ASU-069 oncesi `project_add`, ev dizinini ya da bir SSH dizinini **kabul
ederdi**. UI akisinda bu daha az onemliydi (kullanici bir pencerede tikliyor);
tool yuzeyi acildigi anda kritik hale geliyor cunku **kayitli kok = Asuna'nin
okuyabildigi alan**. Dort kontrol de `refuse_unsuitable_root` icinde Rust
tarafina eklendi ve `project_add`in **tum** cagiranlarini (UI dahil) kapsiyor —
renderer'a guvenilmedi.

**Neden tam eslesme, on ek degil (sistem dizinleri).** macOS'ta `canonicalize`
bircok yolu `/private/...` altina tasir; gecici dizinler dahil. On ek eslesmesi
kullanicinin gercek projelerini de reddederdi. Reddedilen sey **dizinin
kendisi**: `/usr` proje olamaz ama `/usr/local/src/deneme` olabilir. `~/Library`
bunun **istisnasi** ve bilincli: oradaki her sey (Application Support,
Keychains, tarayici profilleri) proje degil, dolayisiyla on ek eslesmesi dogru.

**Ev dizini cozulemezse** ev dizini tabanli kurallar atlanir ve digerleri kosar
— uydurulmus bir home yolu ile karsilastirma yapmak olmayan bir korumayi var
gibi gostermek olurdu. `refuse_unsuitable_root(canonical, home)` home'u parametre
aliyor: kural ortam degiskenine dokunmadan test edilebiliyor.

### Uygulama Notlari

**Sema tek alanli ve alanin adi `path`.** Proje **adi** bilerek yok: ad
verilmezse host dizin adini kullanir. Modelin ad uydurabilmesi, kullanicinin
onay kartinda gordugu yol ile listede sonra gorecegi ad arasinda fark acardi.
Alan adinin `path` olmasi ayrica onay kartini dogru dolduruyor —
`toApprovalArgumentsPreview` girdiyi `path=<yol>` olarak yaziyor (test bunu
kilitliyor).

**Risk 2 (Gate 3 M3 ile revize edildi; ilk hali risk 1'di).** Hicbir dosya
degismiyor ve islem geri alinabilir (Projeler sekmesinden kayit kaldirilir) —
etkiye bakarak risk 1 savunulabilirdi. Ama bu tool sandbox'in yuzeyini **kalici
olarak** genisletiyor ve risk seviyesi bu repoda bir etiket degil, iki
mekanizmanin girdisi: risk 2+ tanimlar `requiresApproval` olmadan kayit
**edilemiyor** (koruma ayara degil tanima bagli), ve onay karti / `tool_events`
seviyeyi kullaniciya yaziyor. Gerekce ve alternatifler: asagidaki M3 bolumu +
`asuna-docs/DECISIONS.md` → "Phase 5 kararlari".

---

## ASU-070: `set_current_project` Tool (Risk 1, onayli)

**Scope**: backend | **Boyut**: S | **Durum**: DONE (sesli madde ASU-071'de) | **Bagimlilik**: ASU-048, ASU-067

### Acceptance Criteria
- [x] Tool `project_set_current` komutunu sariyor
- [x] Model **adi** verebiliyor: tool once `project_list`ten kimligi cozuyor
- [x] Tam eslesme (buyuk/kucuk harf yok sayilir); kismi eslesme yok
- [x] Birden cok aday → tipli hata + aday listesi, tool **secim yapmiyor**
- [x] Bilinmeyen ad → tipli hata + kayitli projelerin listesi
- [x] Basarida yeni guncel projenin adi donuyor
- [x] Reddedilince secim degismiyor ve "gectim" denmiyor
- [ ] Sesli test: "Freelancer projesine gec" onay isteyip geciyor

### Uygulama Notlari

**Ad → kimlik cozumu tool'un isi.** Kullanici "freelancer'a gec" der; modelin
elinde `freelancer` **kimligi** yoktur. Tool once `project_list`i cagirir,
`matchProjects` ile cozer, sonra `project_set_current`i kimlikle cagirir. Iki
karar: (a) **tam eslesme** — `pro` yazip `proje-a`ya gecmek, modelin yanlis
projede dosya okumasi demek olurdu; (b) **belirsizlikte secim yok** — iki proje
ayni adi tasiyorsa adaylar listelenir ve model kullaniciya sorar.

**Turkce yerel kucultme** (`toLocaleLowerCase('tr')`) iki tarafa da uygulaniyor:
"Istanbul" ile modelin yazdigi kucuk harfli hali ancak ayni donusumle eslesir.

**Neden onayli.** "Guncel proje" bir etiket degil; `read_project_file`,
`list_project_files` ve `open_project`in **hedefi**. Secimi degistirmek sonraki
her dosya cagrisinin baska bir kok'e gitmesi demek — ekranda gorunen proje ile
Asuna'nin okudugu proje sessizce ayrilmamali.

**Host reddi oldugu gibi tasiniyor.** Yolu olmayan bir etiket (`unlinked`) ya da
kok'u kaybolmus bir proje guncel yapilamaz (`registry::set_current`); o ret
`refused` koduyla gelir ve ozet "gectigini IDDIA ETME" der.

---

## ASU-071: Wave D Sesli Kabul Testi

**Scope**: test | **Boyut**: S | **Durum**: PENDING | **Bagimlilik**: ASU-067..070

### Acceptance Criteria
- [ ] "Hangi projelerim var?" → `list_projects` calisiyor, gercek liste okunuyor
- [ ] "Freelancer klasorunde ne var?" → `list_project_files` gercek icerikten cevapliyor
- [ ] "Su klasoru projelerime ekle" → onay karti cikiyor, onaylaninca ekleniyor
- [ ] Ayni istek **reddedilince** proje eklenmiyor ve Asuna eklendigini iddia etmiyor
- [ ] "Freelancer projesine gec" → onay isteniyor, onaylaninca guncel proje degisiyor
- [ ] Olmayan bir proje adinda Asuna proje **uydurmuyor**, kullaniciya soruyor
- [ ] Hassas bir dizini listeleme istegi reddediliyor ve audit'e yaziliyor
- [ ] Dort cagri da `tool_events`'e ve Araclar sekmesine dusuyor

---

## Wave D — Gate 3 güvenlik review düzeltmeleri

> Review Wave D kodu yazıldıktan sonra koştu; aşağıdakiler **düzeltilmiş** hâlin
> gerekçesi. İlk turda yazılan bazı yorumlar artık yanlış olduğu için kodda da
> güncellendi — bu bölüm neyin neden değiştiğinin kaydı.

### C1 (CRITICAL) — `refuse_unsuitable_root` iki kanonik yoldan atlatılabiliyordu

ASU-069'un ilk hâli "ev dizininin kendisi" korumasını `canonical == home` **tam
eşleşmesi** ile, sistem dizinlerini de tam eşleşme listesiyle yazmıştı. İki
bypass:

1. **Ata dizin.** `/Users` ev dizininin *kendisi* değil, ama ondan geniş: tek
   kayıtla bütün kullanıcı ağacı okunabilir alana girerdi. Aynı aile:
   `/System/Volumes/Data`, `/`.
2. **macOS firmlink.** `/System/Volumes/Data/Users/<ad>/Library` aynı dizinin
   ikinci kanonik yolu. `home.join("Library")` öneki tutmuyordu; `/System` tam
   eşleşme olduğu için o alt ağaç da açıktı. Arkasında `~/.config/gh/hosts.yml`
   token'ı, `~/Library/Application Support`, `.zsh_history` var — blocklist
   bunları **ada göre yakalamıyor**.

Üç düzeltme birlikte:

- **Ata reddi**: `home.starts_with(candidate)` → reddet. `/Users`, `/`,
  `/System/Volumes/Data` tek satırda kapanıyor ve bu "ev dizininin kendisi"
  kuralının doğru genellemesi (tam eşleşme onun yalnızca bir üyesiydi).
- **`REFUSED_SYSTEM_SUBTREES` (ön ek)**: `/System`, `/Library`, `/Applications`,
  `/Network`. Bu ağaçların hiçbir alt dizini kullanıcının projesi değil.
- **Firmlink normalizasyonu** (`strip_data_volume`): `/System/Volumes/Data`
  öneki **bütün** karşılaştırmalardan önce soyuluyor. Tek kanonik biçim üstünde
  karar vermek, her kuralı iki kez yazma (ve birini unutma) ihtiyacını
  kaldırıyor.

**Neden `/private` ve `/var` hâlâ tam eşleşme** (`REFUSED_SYSTEM_DIRECTORIES`):
macOS'ta geçici dizinler `/private/var/folders/<hash>/T/` altında yaşıyor ve
kullanıcının gerçek projeleri de `/Volumes/<disk>/...` ile `/usr/local/src/...`
altında olabiliyor. `/private`ı ön ek yapmak testleri değil **gerçek kullanımı**
kırardı. `/System` için aynı gerekçe geçersiz, o yüzden o ön ek.

`/Users` ve `/home` de tam eşleşme listesine eklendi — ata reddi zaten kapatıyor
ama `HOME` çözülemediğinde de kapalı kalsınlar.

Kanıt: 5 birim testi (ata, firmlink, ön ek ağaçları, alt ağaçların açık
kalması) + gerçek `$HOME` üstünde koşan bir birim testi + `acl_regression.rs`
içinde gerçek makinede `/Users` ve firmlink'li `~/Library`yi IPC üzerinden
deneyen `home_ancestors_and_firmlinks_cannot_be_registered_over_ipc`.

### H1 (HIGH) — `matchProjects` kimlik eşleşmesinde belirsizliği yutuyordu

Gerekçe "kimlikler benzersizdir" idi; doğru ama yetersiz. Kimlikler adların
slug'ı olarak üretiliyor (`registry::add` → `slugify`), yani ad ve kimlik ayrı
isim uzayı **değil**:

```text
{ id: 'freelancer',   name: 'freelancer' }
{ id: 'freelancer-2', name: 'Freelancer' }
```

"freelancer'a geç" → kimlik eşleşmesi tek aday döndürüyordu; doğru cevap
"hangisini kastettin?". Sessizce seçmek, sonraki her dosya çağrısının yanlış
kökte çalışması demekti.

Yeni kural: iki küme de hesaplanıyor; kimlik eşleşmesi dışında başka bir ad
eşleşmesi varsa sonuç birleştiriliyor ve çağıran taraf bunu `ambiguous_project`
olarak görüyor. Kimlik eşleşmesi listede **başta** duruyor (kullanıcı gerçekten
kimlik verdiyse ilk sırada görünsün).

### M1 (MEDIUM) — onay kartı yolu 64 karakterde kesiyordu

`MAX_PREVIEW_VALUE_CHARS = 64` sıradan metin için makul, dosya yolu için değil:
`/Users/ad/Work/monorepo/packages/worker` tam da **sonundan** kırpılıyordu ve
kullanıcı ne onayladığını göremiyordu. C1 ile birleşince, denetlenmesi gereken
tek ucu gizliyordu.

Düzeltme `realtime-service.ts` içinde ve **yalnızca önizleme kırpması**: yol
gibi görünen değerler (`/` veya `~/` ile başlayan) için ayrı bir tavan
(`MAX_PREVIEW_PATH_CHARS = 160`) ve **ortadan** kırpma — baş 28 karakter + `…` +
son. Satırın toplam tavanı (240) değişmedi. "Yol gibi görünen" tanımı bilerek
dar: içinde eğik çizgi geçen her metni yol saymak, uzun serbest metne geniş
tavanı açardı.

### M2 (MEDIUM) — `read_dir` sınırsız tüketiliyordu

200 girdi tavanı yalnızca **çıktıyı** koruyordu, **işi** değil:
`node_modules/.pnpm` gibi bir dizinde on binlerce girdi okunuyor, symlink'ler
için binlerce `canonicalize` çağrılıyordu. TS tarafı 10 sn'de timeout dönse bile
Rust durmaz (`invoke` iptal edilmiyor) — sesli oturumun ortasında diski meşgul
eden bir iş arkada devam ederdi.

`MAX_SCANNED_ENTRIES = 5 000` eklendi; iterator orada bırakılıyor, yani kalan
girdiler için `metadata`/`canonicalize` **hiç** çağrılmıyor.

Yeni alan **`scanCapped`** (bool) sözleşmeye eklendi ve bu bilinçli: "200'de
kırpıldı" ile "5 000'de saymayı bıraktık" farklı şeyler — ilkinde toplam
biliniyor, ikincisinde bilinmiyor. Tek bayrakla anlatmak modele bilmediği bir
sayıyı biliyormuş gibi verdirirdi. Model özeti `scanCapped` iken "EN AZ N girdi
(tam sayı bilinmiyor)" diyor ve "yaklaşık su kadar" demeyi de açıkça yasaklıyor.

> **Sözleşme kırılması:** `ProjectDirectoryView` artık `scanCapped` taşıyor.
> Tool dışı tüketici (`src/components/composer.spec.tsx` sabiti, chat kabuğu
> oturumunun dosyası) bir satır güncellenmeli — bkz. aşağıdaki "Açık" notu.

### M3 (MEDIUM → orchestrator kararı) — `register_project` risk 2 oldu

Risk seviyesi yalnızca bir etiket değil, iki mekanizmanın girdisi:

- `ToolRegistry.register`, risk 2+ bir tanımı `requiresApproval` olmadan **kayıt
  etmez**. Risk 1'de o koruma yoktu: biri `requiresApproval: true` satırını
  silse tool sessizce onaysız çalışır hâle gelirdi.
- Onay kartı ve `tool_events` risk seviyesini yazıyor; "risk 1 — geri
  alınabilir düşük risk", okunabilir alanı **kalıcı** genişleten bir işlem için
  doğru cümle değil.

Bugün davranış farkı yok (ikisi de her modda onay ister); değişen şey korumanın
ayara değil **tanıma** bağlanması. `set_current_project` risk 1 kaldı: hedefi
değiştiriyor ama okunabilir alanı büyütmüyor.

Registry artık: dört risk 0, iki risk 1, **bir risk 2**. Risk 3 hâlâ yok.
`registry.spec.ts` risk 2 kümesinin tam olarak `['register_project']` olduğunu
kilitliyor — ikincisi sessizce eklenemez.

### Ucuz düzeltmeler

- **L1** — bloklu girdide `size_bytes: None`. `.env`in kaç bayt olduğu küçük ama
  gereksiz bir sızıntı (kaç anahtar var?) ve okunamayan bir dosyanın ölçüsü
  modelin işine yaramıyor.
- **L2** — `register-project.ts` içindeki "deftere yol yazılmaz" yorumu
  yanlıştı: **sonuç özeti** yol taşımıyor, ama argümandaki yol
  `tool_events.arguments_redacted` alanına host tarafında redakte edilerek
  **yazılıyor** — denetim için gerekli, çünkü "hangi dizin kaydedilmek istendi?"
  sorusunun cevabı odur. Yorum düzeltildi.

### Açık — orchestrator aksiyonu

`ProjectDirectoryView`e `scanCapped` eklenmesi, chat kabuğu oturumunun
dosyasındaki bir test sabitini kırıyor:
`src/components/composer.spec.tsx:36` → fixture'a `scanCapped: false` satırı
eklenmeli. Dosya başka oturuma ait olduğu için **dokunulmadı**; `pnpm typecheck`
o tek hatayla kırmızı.

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

### ASU-051 / ASU-052 icin acik nokta

12.2'nin kolon listesinde **yapisal bir basari/hata alani yok**; `result_summary` metni ikisini de
tasiyor. `approval_state` "calisti mi"yi soyler ama "calisti ve basarili miydi"yi soylemez.
ASU-051/052 audit satirindan bunu ayirt etmek isterse migration 005 ile
`outcome TEXT CHECK (outcome IN ('succeeded','failed','not_run'))` eklenmeli — sema degisikligi
oldugu icin **orchestrator karari**.

---

## ASU-051: `read_project_file` Tool (Risk 0, Sandbox'li)

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-049, ASU-050

### Acceptance Criteria
- [ ] Tool kayitli proje root'u icindeki bir dosyayi okuyor
- [ ] ASU-049 sandbox'i ve blocklist'i uyguluyor
- [ ] Buyuk dosya kirpiliyor ve kirpildigi cikti'da belirtiliyor
- [ ] Cikti UI'da gorunuyor (hangi dosya okundu)
- [ ] Her cagri `tool_events`'e yaziliyor
- [ ] Sesli test: "Bu projenin README'sinde ne yaziyor?" gercek icerikten cevapliyor
- [ ] Var olmayan dosyada Asuna icerik uydurmuyor

---

## ASU-052: `open_project` Tool (Risk 1)

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-048, ASU-050

### Aciklama
PROJECT.md Bolum 32 Phase 5'in acik hedefi: "open current project in editor".

### Acceptance Criteria
- [ ] Kayitli bir projeyi konfigure edilmis editorde aciyor (varsayilan VS Code)
- [ ] Editor komutu konfigurabilir; bulunamazsa **durust hata**:
      "Projeyi acmayi denedim ama VS Code komutu bulunamadi" (PROJECT.md Bolum 30)
- [ ] Risk 1 -> safe modda onay UI'si cikiyor
- [ ] Sadece kayitli proje yollari acilabiliyor (keyfi yol acilamiyor)
- [ ] Basari/hatasi `tool_events`'e yaziliyor
- [ ] `last_opened_at` guncelleniyor
- [ ] Shell enjeksiyonuna kapali (yol arguman olarak gecirilir, string birlestirme ile komut kurulmaz)

---

## ASU-053: Approval UI

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-048

### Acceptance Criteria
- [ ] `AWAITING_APPROVAL` durumunda net bir onay karti gorunuyor
- [ ] Kart gosteriyor: tool adi, ne yapacagi (insan diliyle), risk seviyesi, redakte edilmis argumanlar
- [ ] Onayla / Reddet butonlari; klavye ile de erisilebilir
- [ ] Onay bekleme suresi gorunur; zaman asiminda otomatik reddediliyor
- [ ] Asuna onay beklerken konusmaya devam edip "yaptim" demiyor
- [ ] Reddedilen aksiyon sonrasi Asuna durumu dogru anlatiyor
- [ ] Overlay modda da onay istegi gorunuyor (ana pencere kapaliyken kaybolmuyor)

---

## ASU-054: Tool Call Gorunurlugu

**Scope**: frontend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-047, ASU-050

### Aciklama
PROJECT.md Bolum 19: "The user should never wonder whether the agent is silently modifying the computer."

### Acceptance Criteria
- [ ] Tool calisirken `TOOL_PENDING` durumu ve tool adi gorunuyor
- [ ] Tool sonucu (basari/hata + kisa ozet) transcript akisinda gorunuyor
- [ ] Tools sekmesi: etkin tool listesi, risk seviyeleri, onay politikasi
- [ ] Tool audit gecmisi goruntulenebiliyor (oturuma gore filtreli)
- [ ] Tool tek tek devre disi birakilabiliyor
- [ ] Gizli/gorunmez tool calistirma yolu yok (dogrulanmis)

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
- [ ] Manuel senaryo `asuna-config/testing.md`'ye eklenmis

### Notlar
Bu, guvenlik modelinin ilk gercek sinavi. Bir madde bile gecmezse Phase 6'ya gecilmez —
reviewer agent + `asuna-config/security.md` checklist'i ile acik kapatilir.

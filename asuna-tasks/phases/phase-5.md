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

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-047

### Aciklama
PROJECT.md Bolum 5.4 risk seviyeleri + `ASUNA_TOOL_APPROVAL_MODE`.

### Acceptance Criteria
- [ ] Risk 0: onaysiz calisiyor
- [ ] Risk 1: `ASUNA_TOOL_APPROVAL_MODE` ile konfigurabilir (safe modda onay ister)
- [ ] Risk 2 ve 3: **her zaman** acik onay istiyor; konfigurasyonla atlanamiyor
- [ ] Onay bekleyen tool `AWAITING_APPROVAL` durumunu tetikliyor
- [ ] Onay zaman asimina ugrarsa tool **calismiyor** (varsayilan reddet)
- [ ] Onay karari tool cagrisi basina; "hepsine izin ver" MVP'de yok
- [ ] Politika kararlari unit test edilmis (her risk seviyesi x her mod matrisi)

### Notlar
Varsayilan davranis her zaman "calistirma". Belirsizlik onay lehine cozulur, calistirma lehine degil.

---

## ASU-049: Path Sandbox + Hassas Dosya Blocklist

**Scope**: backend | **Boyut**: M | **Durum**: PENDING | **Bagimlilik**: ASU-047, ASU-040

### Aciklama
PROJECT.md Bolum 19 "Filesystem sandbox". Bu, guvenlik modelinin en cok test edilmesi gereken parcasi.

### Acceptance Criteria
- [ ] Tum dosya erisimleri kayitli proje root'una gore normalize edilip cozuluyor
- [ ] Path traversal reddediliyor: `../../.ssh/id_ed25519`, mutlak yol, `~` genislemesi, sembolik link
      ile disari cikma
- [ ] Blocklist: `.env*`, `*.pem`, `*.key`, `id_rsa*`, `id_ed25519*`, `.ssh/`, keychain, credential
      dosyalari, `.git/config` icindeki kimlik bilgileri
- [ ] Maksimum dosya boyutu siniri; binary dosya reddi
- [ ] Reddedilen erisim sessizce bos donmuyor — acik "reddedildi" sonucu donuyor ve audit'e yaziliyor
- [ ] **Kapsamli unit test seti** — en az 15 kotu yol vakasi (CLAUDE.md: "Add tests for
      security/permission/path logic")
- [ ] `docs/architecture/security.md` sandbox kurallarini anlatiyor

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

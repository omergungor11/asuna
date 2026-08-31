# Tool Architecture

> **Phase 5 sonrası güncellendi (2026-08-31, ASU-047..054).**
> Kaynak gerçek: `PROJECT.md` Bölüm 17-19. Bu dosya spec'i kopyalamaz; tool katmanının
> **mimari şeklini** ve uygulanmış kısıtları toplar. Kalan açıklar Bölüm 9'daki TODO
> tablosunda; M4 kabul testi ASU-055.

## 1. Temel kural

Bilgisayarda yapılan **her şey** açık bir tool tanımıyla yapılır. Ad hoc kod yolu yok,
serbest shell yok, "sadece bu seferlik" istisnası yok.

```ts
type ToolRisk = 0 | 1 | 2 | 3;

interface AsunaToolDefinition {
  name: string;
  description: string;
  risk: ToolRisk;
  requiresApproval: boolean;
  timeoutMs: number;
  parameters: ToolInputSchema; // zod object — şemanın tek kaynağı
  execute(args: unknown, ctx: ToolContext): Promise<ToolResult>;
}
```

Tool'lar merkezi bir **registry**'de durur (`src/asuna/tools/registry.ts`, ASU-047):
`register` / `list` / `resolve`. Sözleşme **kayıt anında** zorlanır — aynı isim iki kez,
snake_case olmayan ad, timeout'suz tanım ve onay istemeyen risk 2/3 reddedilir; geçersiz
tanım modele hiç açılmaz.

Çalıştırmanın tek meşru yolu `executeTool(definition, args, ctx, options?)`: şema
doğrulaması (geçersizse `execute` **çağrılmaz**), onay kapısı (Bölüm 3), `timeoutMs` +
`AbortSignal`, ve her zaman yapısal `ToolResult`. `errorKind` sabitleri:
`invalid_arguments`, `timeout`, `aborted`, `denied`, `tool_failed`, `disabled`.

`ToolResult` iki ayrı özet taşır: `summary` **modele** gider, `auditSummary` (verilmezse
`summary`) **deftere ve transcript'e** gider. Ayrım tip düzeyinde: `read_project_file`
modele dosya içeriği verir, audit'e sadece "README.md okundu (2.1 KB, kırpıldı)" düşer.

SDK'ya adaptasyon **tek yönlüdür**: `AsunaToolDefinition` → `tool()`
(`@openai/agents-realtime`). Ters yön yok — SDK tipleri tool katmanına sızmaz
(ADR-002 / `voice.md` Bölüm 9); `realtime-service.ts` adaptörü `parameters`'ı SDK
şemasına verir ve `execute` gövdesinde yine `executeTool`'u çağırır.

## 2. Risk seviyeleri

| Risk | Tanım | Onay | Örnek |
|---|---|---|---|
| 0 | Read-only | Gerekmez | `read_project_file`, `get_git_status`, `get_current_project`, `list_recent_project_activity` |
| 1 | Geri alınabilir, düşük riskli | Konfigüre edilebilir | `open_project`, `create_project_note` |
| 2 | Mutation | MVP'de **her zaman** net onay | dosya düzenle, paket kur, build çalıştır, commit |
| 3 | Destructive / external | **Her zaman** explicit onay | dosya sil, push, mail, publish, deploy, para harcama, sistem ayarı |

- `ASUNA_TOOL_APPROVAL_MODE=safe|always` risk 2/3'ü **bypass edemez**; bu ayar yalnızca
  risk 1'in davranışını belirlemek için vardır. Mevcut iki mod risk 1'de **aynı** davranır
  (ikisi de sorar) — fark, gevşetici bir mod eklendiğinde ortaya çıkar (ASU-048).
- Onay istemi kullanıcıya **ne yapılacağını** gösterir (tool adı + redakte argümanlar),
  sadece "izin ver?" demez.
- SDK tarafında karşılığı `needsApproval: true` → `tool_approval_requested` event'i →
  `AWAITING_APPROVAL` state → `session.approve(item)` / `session.reject(item, { message })`.
- `{ alwaysApprove: true }` (oturum boyu yapışkan onay) **kullanılmaz** — her destructive
  işlem tekrar sorulur.

## 3. Onay akışı

Politika tek fonksiyonda: `resolveApproval(risk, requiresApproval, mode)`
(`src/asuna/tools/approval-policy.ts`, ASU-048). Ekranda yazan politika ile çağrı anında
uygulanan politika aynı kaynaktan gelir.

| Risk | `requiresApproval` | `safe` | `always` |
|---|---|---|---|
| 0 | `false` | onaysız | onaysız |
| 0 | `true` | ONAY | ONAY |
| 1 | herhangi | ONAY | ONAY |
| 2 / 3 | herhangi | ONAY | ONAY |

Bir tanım kendi onay isteğini **sıkılaştırabilir, gevşetemez**. `always` modu "risk 0'ı da
sor" demek değildir — kilittir: ileride gevşetici bir mod eklense bile hiçbir riski otomatik
geçirmez. Risk 2/3 satırları tabloya bakılmadan döner.

**İki katmanlı uygulama.** (a) SDK katmanı: `toSdkTool` içindeki `needsApproval` bu
fonksiyondur; `true` dönünce SDK `execute`'u **hiç** çağırmaz, önce `tool_approval_requested`
çıkar. (b) Çalıştırma kapısı: `executeTool` aynı kararı bağımsız yeniden hesaplar ve
**kanıt** ister; kanıt tek çağrı için geçerlidir. Onay akışını atlayan bir çağrı çalışmaz.

**Varsayılan = ÇALIŞTIRMA.** Kanıt yoksa `not_requested`, kullanıcı reddederse `denied`,
60 sn içinde cevap gelmezse `timeout` — üçü de modele tek biçimde döner
(`errorKind: 'denied'` = "yapmadım"); ayrımı audit taşır. Tool'un kendi `timeoutMs` sayacı
**onaydan sonra** başlar. Zaman aşımını **UI tetiklemez**, servis otomatik reddeder.

**UI sözleşmesi** (`useAsunaSession`): `pendingApproval` (`requestId`, tool adı, açıklama,
risk, redakte argüman özeti, `timeoutMs`, `requestedAtMs`) + `approveTool(requestId)` /
`rejectTool(requestId)`. Karar **kimlikle** verilir; "sonuncuyu onayla" yolu yok.

**Kart davranışı** (ASU-053): açılışta odak **"Reddet"** butonundadır ve tek klavye kısayolu
`Esc` = reddet — **onaylayan kısayol yoktur**. Gerekçe: kart tam kullanıcı Enter'a basarken
açılırsa risk 1+ bir aksiyon refleksle onaylanabilir; kazayla basılan tuşun sonucu her zaman
"çalıştırma" yönünde olmalı. Kart `document.body`'ye portal edilir, sekme değişse de görünür.
Ayrı bir overlay penceresi henüz yok (backlog).

**Tool kapatma iki katmanlı** (ASU-054): kapalı tool `connect()` sırasında modele verilen
listeden düşürülür **ve** `executeTool` her çağrıda `isEnabled`'ı yeniden sorar — açık bir
oturumun ortasında kapatılan tool'un çağrısı `errorKind: 'disabled'` ile reddedilir ve
`tool_events`'e `not_run` olarak yazılır. Gizli çalıştırma yolu bırakmamak için ikisi de gerekli.

## 4. İlk tool seti (MVP)

| Tool | Risk | Not | Task | Durum |
|---|---|---|---|---|
| `get_current_project` | 0 | id, ad, path, git branch, proje özeti | ASU-044 | Açık |
| `read_project_file` | 0 | kayıtlı root içinde; blocklist; 256 KiB tavanı; binary reddi; 6000 karakter kırpma | ASU-051 | Açık |
| `open_project` | 1 | `ASUNA_EDITOR_COMMAND` ile editörü açar; onay ister | ASU-052 | Açık |
| `get_git_status` | 0 | veri katmanı Phase 4'te hazır, tool açılmadı | — | Backlog |
| `list_recent_project_activity` | 0 | aynı | — | Backlog |
| `create_project_note` | 1 | yalnızca `.asuna/notes/` altına yazacak | — | Backlog |

MVP'de modele **üç** tool açıktır. Kalanlar `asuna-tasks/backlog.md`'de; her yeni tool
Bölüm 5'teki desene ve Bölüm 7'deki audit sözleşmesine uymak zorundadır.

## 5. Yürütme deseni — "ince backchannel"

Phase 0'ın en önemli tool bulgusu (`voice.md` Bölüm 9, SDK docs'tan birebir):

> **Function tool'lar `RealtimeSession`'ın çalıştığı yerde çalışır** — yani Asuna'da
> **renderer'da**. Hassas iş için tool'un içinden trusted tarafa çağrı yapılır.

Sonuç: renderer'daki `execute()` gövdesi **ince** olmalıdır — schema doğrulama + `invoke`.
Gerçek dosya / git / DB / process işi `#[tauri::command]` üzerinden Rust'ta yapılır.

```text
model tool call
  └─ renderer: tool({ name, parameters: z.object(...), execute })   <- ince
       └─ invoke('...')                                            <- güven sınırı
            └─ src-tauri: path sandbox + blocklist + timeout + audit yazımı
```

Bu sınırın pazarlıksız sonuçları:

- Path sandbox, blocklist ve boyut sınırı **Rust tarafında** uygulanır. Renderer'ın
  yaptığı kontrol UX'tir, güvenlik değil — Rust renderer'a güvenmez, kendi doğrulamasını yapar.
- Tool argümanları shell'e string olarak birleştirilmez (arg array, shell interpolation yok).
- Tool hatası modele anlamlı ama iç detay sızdırmayan mesajla döner: mutlak path yok,
  stack trace yok.
- Tool'lar secret **değeri** döndürmez; ayrıcalıklı işi kendi içinde yapar, sonuç olarak
  "yapıldı/yapılmadı + özet" döner.
- Her yeni komut için: `src-tauri/build.rs` `AppManifest::commands([...])` listesine ekle +
  `src-tauri/permissions/` altında dar izin + capability satırı (bkz. `security.md`).

## 6. Shell politikası

`run_any_shell_command(command: string)` gibi bir tool modele **asla** expose edilmez
(PROJECT.md 18).

Shell ihtiyacı **scoped** tool'larla karşılanır: `run_tests`, `run_lint`, `git_status`,
`git_diff`, `npm_install_package`, `start_dev_server`. Her biri:

1. argümanını validate eder, 2. working directory'si kısıtlıdır, 3. timeout'u vardır,
4. stdout/stderr'i capture eder, 5. risk seviyesi taşır, 6. audit'e yazar,
7. gerektiğinde onay ister.

Kontrollü genel shell (allowlist + onay kapısı) ancak MVP sonrası ve ayrı bir ADR ile
gündeme gelir.

## 7. Audit akışı (`tool_events`)

Her çağrı — başarılı, başarısız, reddedilmiş ve timeout olan **dahil** — yazılır.
Sessizce yutulan tool çağrısı yoktur. `executeTool` her çıkış yolunda audit kancasını
**tam bir kez** çağırır; satır çağrı bittiğinde yazılır, iki aşamalı "aç/kapat" yoktur.

Alanlar: `session_id, tool_name, risk_level, arguments_redacted, approval_state,
result_summary, outcome, created_at`.

**İki bağımsız eksen** (migration 005, ASU-051):

| Eksen | Değerler | Cevapladığı soru |
|---|---|---|
| `approval_state` | `not_required`, `auto_approved`, `approved`, `denied`, `timeout`, `not_requested` | Çalıştı mı? |
| `outcome` | `succeeded`, `failed`, `not_run`, `NULL` | Başardı mı? |

- `not_required` (onay GEREKMEDİ) ile `not_requested` (onay SORULAMADI) ayrı şeylerdir.
- `failed` = tool **çalıştı** ve yapamadı (sandbox reddi, timeout, editör bulunamadı);
  `not_run` = `execute` hiç çağrılmadı (şema reddi, onay reddi, kapatılmış tool).
- `NULL` yalnızca migration 005 öncesi satırlarda; geriye dönük doldurma **yapılmadı**
  ve UI bu satırlarda etiket basmaz — "başarılı" varsayılmaz.

`arguments_redacted` **host tarafında** üretilir: tek satır `anahtar=değer`, metinler 64
karakterde kırpılır, dizi ve nesneler yalnızca **şekil** olarak yazılır (`[3 öge]` /
`{2 alan}`), sonra `redact_sensitive_text` + 512 karakter tavanı. İç içe içerik hiçbir zaman
serileştirilmez — "dosya içeriği audit'e girmez" bir uzunluk tahminine değil biçimin
kendisine bağlıdır.

**Append-only.** Silme/güncelleme komutu yoktur; `session_delete` de silmez (FK
`ON DELETE SET NULL` — "konuşma geçmişini sil" düğmesi audit defterini silen bir primitif
olamaz). Renderer'a açılan tek yazma yolu dar bir `record_tool_event` append komutudur ve
`arguments_redacted` alanını **kabul etmez** (ADR-005 sapması, `DECISIONS.md` → Phase 5).

Audit yazımı başarısız olursa görünür olur: tipli Rust hatası + `{ status: 'failed', error }`
+ `error` seviyesinde log. Sessiz kayıp yok.

> `ASUNA_MEMORY_ENABLED=false` iken DB hiç açılmaz, dolayısıyla **audit de tutulmaz** —
> o durumda tool görünürlüğü canlı UI'a kalır (bilinçli karar, `DECISIONS.md` → Phase 5).

## 8. Görünürlük

Kullanıcı "acaba sessizce bir şey mi değiştirdi?" diye düşünmez — bu ürün gereksinimi,
kozmetik değil (PROJECT.md 19 "Visible action state").

- **Durum**: tool çalışırken `TOOL_PENDING` rozeti + tool adı; onay beklerken
  `AWAITING_APPROVAL` ve onay kartı. Onay bekleyen çağrı `TOOL_PENDING`'e **uğramaz** —
  SDK onay verilene kadar `execute`'u çağırmaz, "çalışıyor" göstermek olmayan bir işi
  olmuş gibi göstermek olurdu.
- **Transcript**: sonuç `role: 'tool'` satırı olarak akışa düşer (ad, özet, `outcome`
  etiketi, risk). Satırın metni `auditSummary`'dir; dosya içeriği ekrana dökülmez ve
  kalıcı transcript dökümüne **yazılmaz** (`transcript_lines` yalnızca `user`/`assistant`
  tanır) — tool çağrılarının kalıcı kaydı `tool_events`.
- **Araçlar sekmesi** (ASU-054): modele açık tool listesi (risk + onay politikası), tool
  başına oturum-yerel aç/kapa, oturum filtreli **salt okunur** audit geçmişi. Ekranda silme
  veya düzenleme düğmesi yok ve olmayacak.
- Tanım listesi ve toggle seti kompozisyon kökünde **bir kez** kurulur; oturum ile ekran
  aynı örnekleri paylaşır, yani ekranda "Kapalı" görünen bir tool çalışıyor olamaz.

## 9. TODO — kalan açıklar

| # | Açık | Nerede |
|---|---|---|
| ~~T1~~ | ~~`ToolContext` / `ToolResult` tiplerinin kesin şekli~~ — kapandı (ASU-047, `tools/types.ts`) | — |
| ~~T2~~ | ~~Registry API'si: kayıt, keşif, SDK'ya adaptasyon fonksiyonu~~ — kapandı (ASU-047) | — |
| ~~T3~~ | ~~Risk 1 için onay politikası davranışı~~ — kapandı (ASU-048): risk 1 her iki modda da sorar, matris Bölüm 3'te | — |
| ~~T4~~ | ~~Path sandbox implementasyonu: `realpath` + root prefix + symlink escape~~ — kapandı (ASU-049, `src-tauri/src/security/sandbox.rs`) | — |
| ~~T5~~ | ~~Blocklist modülünün yeri ve glob eşleşme sırası~~ — kapandı (ASU-049): tek modül `src-tauri/src/security/blocklist.rs`, eşleşme `canonicalize` **sonrası** | — |
| ~~T6~~ | ~~Max dosya boyutu ve binary tespiti eşikleri~~ — kapandı (ASU-049): `MAX_READABLE_FILE_BYTES` 256 KiB (aşan **reddedilir**), ilk 8 KiB'de NUL / %10 kontrol baytı → binary. `read_project_file` kendi kırpma bütçesini (6000 karakter) bu tavanın **altında** uygular | — |
| ~~T7~~ | ~~Tool timeout varsayılanları ve alt process öldürme davranışı~~ — kapandı (ASU-047/052): timeout tanım başına zorunlu (1..120 000 ms), `AbortSignal` ile iptal; `open_project` alt process'i **beklemez ve öldürmez** (GUI editörü saatlerce açık kalır) — çıktı "başlatıldı" der, "kullanıcı gördü" demez | — |
| ~~T8~~ | ~~Redaction pattern seti + unit testleri~~ — kapandı (ASU-049/050 + Phase 3 HIGH-2): `src-tauri/src/redaction.rs` (`redact_secrets` log için, `redact_sensitive_text` saklanan metin için), `tool_event_repository` her iki özeti de bu süzgeçten geçirir | — |
| T9 | `.asuna/notes/` dizin yerleşimi ve isimlendirme | `create_project_note` ile birlikte — backlog |
| T10 | Onay isteği için **ayrı overlay penceresi** — ana pencere kapalıyken kart görünmüyor (ASU-053'ün karşılanamayan kriteri) | backlog |

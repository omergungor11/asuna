# Tool Architecture

> **İskelet — Phase 0 kapanış kaydı (2026-08-24, ASU-010).**
> Kaynak gerçek: `PROJECT.md` Bölüm 17-19. Bu dosya spec'i kopyalamaz; tool katmanının
> **mimari şeklini** ve Phase 0'da doğrulanmış kısıtları toplar. Uygulama Phase 5'te
> (ASU-047..055) — burada `TODO` olan her şey oraya bakar.

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
doğrulaması (geçersizse `execute` **çağrılmaz**), `timeoutMs` + `AbortSignal`, ve her
zaman yapısal `ToolResult`. `errorKind` sabitleri: `invalid_arguments`, `timeout`,
`aborted`, `tool_failed`.

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

- `ASUNA_TOOL_APPROVAL_MODE=safe|always` risk 2/3'ü **bypass edemez** — bu ayar sadece
  risk 1'in davranışını belirler.
- Onay istemi kullanıcıya **ne yapılacağını** gösterir (tool adı + redakte argümanlar),
  sadece "izin ver?" demez.
- SDK tarafında karşılığı `needsApproval: true` → `tool_approval_requested` event'i →
  `AWAITING_APPROVAL` state → `session.approve(item)` / `session.reject(item, { message })`.
- `{ alwaysApprove: true }` (oturum boyu yapışkan onay) **kullanılmaz** — her destructive
  işlem tekrar sorulur.

## 3. İlk tool seti (MVP)

| Tool | Risk | Not | Task |
|---|---|---|---|
| `get_current_project` | 0 | id, ad, path, git branch, proje özeti | ASU-044 |
| `read_project_file` | 0 | kayıtlı root içinde; blocklist; max boyut; binary tespiti | ASU-051 |
| `get_git_status` | 0 | — | Phase 4 |
| `list_recent_project_activity` | 0 | — | Phase 4 |
| `open_project` | 1 | editörü aç/odakla | ASU-052 |
| `create_project_note` | 1 | MVP'de **yalnızca** `.asuna/notes/` altına yazar | Phase 5 |

## 4. Yürütme deseni — "ince backchannel"

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

## 5. Shell politikası

`run_any_shell_command(command: string)` gibi bir tool modele **asla** expose edilmez
(PROJECT.md 18).

Shell ihtiyacı **scoped** tool'larla karşılanır: `run_tests`, `run_lint`, `git_status`,
`git_diff`, `npm_install_package`, `start_dev_server`. Her biri:

1. argümanını validate eder, 2. working directory'si kısıtlıdır, 3. timeout'u vardır,
4. stdout/stderr'i capture eder, 5. risk seviyesi taşır, 6. audit'e yazar,
7. gerektiğinde onay ister.

Kontrollü genel shell (allowlist + onay kapısı) ancak MVP sonrası ve ayrı bir ADR ile
gündeme gelir.

## 6. Audit akışı (`tool_events`)

Her çağrı — başarılı, başarısız, reddedilmiş ve timeout olan **dahil** — yazılır.
Sessizce yutulan tool çağrısı yoktur.

```text
agent_tool_start (SDK event)   → state TOOL_PENDING   → tool_events satırı açılır
   (risk 2/3) tool_approval_requested → AWAITING_APPROVAL → approve/reject → approval_state
agent_tool_end (SDK event)     → result_summary + başarı/hata yazılır
```

Alanlar: `session_id, tool_name, risk_level, arguments_redacted, approval_state,
result_summary, created_at`. `arguments_redacted` **redakte edilmiş** alandır — secret
maskeleme burada testli olarak uygulanır.

Yazma yolu tek: Rust `db` katmanı, `memories` ile aynı transaction motoru. **Renderer'ın
`tool_events`'e yazma veya silme yolu yoktur** (ADR-005).

## 7. Görünürlük

Asuna bir tool kullandığında UI bunu gösterir (`TOOL_PENDING`, `AWAITING_APPROVAL`).
Kullanıcı "acaba sessizce bir şey mi değiştirdi?" diye düşünmez — bu ürün gereksinimi,
kozmetik değil (PROJECT.md 19 "Visible action state").

## 8. TODO — Phase 5'te kapanacak

| # | Açık | Nerede |
|---|---|---|
| ~~T1~~ | ~~`ToolContext` / `ToolResult` tiplerinin kesin şekli~~ — kapandı (ASU-047, `tools/types.ts`) | — |
| ~~T2~~ | ~~Registry API'si: kayıt, keşif, SDK'ya adaptasyon fonksiyonu~~ — kapandı (ASU-047) | — |
| T3 | Risk 1 için onay politikası davranışı (`safe` modda sorar mı) | ASU-048 |
| T4 | Path sandbox implementasyonu: `realpath` + root prefix + symlink escape | ASU-049 |
| T5 | Blocklist modülünün yeri ve glob eşleşme sırası (symlink çözümünden **sonra**) | ASU-049 |
| T6 | Max dosya boyutu ve binary tespiti eşikleri | ASU-051 |
| T7 | Tool timeout varsayılanları ve alt process öldürme davranışı | Phase 5 |
| T8 | Redaction pattern seti + unit testleri | ASU-055 |
| T9 | `.asuna/notes/` dizin yerleşimi ve isimlendirme | Phase 5 |

# Code Conventions

> Asuna icin gecerli kod kurallari. Stack detaylari: `asuna-config/tech-stack.md`.
> Mimari gerekce: `PROJECT.md` (Bolum 17, 19, 24, 30, 39) ve `asuna-docs/AGENT-SPEC-ORIGINAL.md`.

## TypeScript
- Strict mode her zaman acik (`strict: true`, `noUncheckedIndexedAccess` acik)
- `any` **yasak** — `unknown` + type guard kullan; `as any` ile susturma yok
- `@ts-ignore` yasak; kacinilmazsa `@ts-expect-error` + tek satir gerekce yorumu
- Object shape'ler icin interface, union/intersection icin type
- Export edilen fonksiyonlarda explicit return type
- Durum/kind gibi sabit kumeler string union (`type ToolRisk = 0 | 1 | 2 | 3`), magic string yok
- Harici veri (SDK response, DB satiri, tool argumani) tip *iddia* edilmez, schema ile **dogrulanir** (zod)

## File Naming
- Tum dosyalar `kebab-case` — istisna yok
- Servis: `.service.ts` · tool implementasyonu: `.tool.ts` · tip toplulugu: `.types.ts` · test: `.spec.ts` (source ile ayni dizinde)
- Rust tarafi `snake_case.rs`
- Klasor sinirlari `PROJECT.md` 22'deki yapiyi izler: `src/asuna/{agent,audio,memory,projects,tools,security,observability}`

## Mimari — Servis Sinirlari
- **Service-based:** kucuk, tek sorumlulugu olan, bagimliliklari constructor/param ile alinan,
  test edilebilir servisler. Tanri-modul yok.
- Su alanlar ayri kalir ve birbirine dogrudan sizmaz:
  `audio` · `agent` · `memory` · `projects` · `tools` · `permissions` · `security` · `database` · `ui`
- React bilesenleri **asla** dogrudan shell komutu calistirmaz, DB sorgusu atmaz, SDK'ya baglanmaz —
  yalniz servis arayuzlerini cagirir
- Ucuncu parti SDK detaylari wrapper arkasinda kalir. Ornek: OpenAI Agents SDK yalnizca
  `AsunaRealtimeService` icinde gorunur; `RealtimeAgent`/`RealtimeSession` tipleri disari sizmaz.
  Ayni kural Porcupine icin `WakeWordProvider` ile gecerli.
- Ilk vertical slice'ta asiri soyutlama yapma — ama saglayici sinirlarini (model, wake word, DB)
  bastan interface arkasina al (PROJECT.md 39/13)

## Voice State Machine
- Tek dogru durum kaynagi: `BOOTING · IDLE_WAKE_WORD · WAKING · CONNECTING · LISTENING ·
  USER_SPEAKING · ASSISTANT_THINKING · ASSISTANT_SPEAKING · TOOL_PENDING · AWAITING_APPROVAL · ERROR`
- Gecisler tek bir reducer/FSM uzerinden; bilesen icinde ad-hoc `setState` ile durum uydurulmaz
- Her gecis loglanir (SCREAMING_SNAKE event adi — `WAKE_WORD_DETECTED`, `REALTIME_CONNECTED`)
- Her cikmaz yol `IDLE_WAKE_WORD`'e doner; `ERROR` terminal durum degildir

## Tool Tanimi
Modele acilan her yetenek `AsunaToolDefinition` ile tanimlanir ve registry'ye kaydedilir:

```ts
type ToolRisk = 0 | 1 | 2 | 3;

interface AsunaToolDefinition {
  name: string;              // snake_case, fiil_nesne: get_current_project
  description: string;       // model icin net ve dar kapsam
  risk: ToolRisk;            // 0 read-only · 1 geri alinabilir · 2 mutation · 3 destructive/external
  requiresApproval: boolean;
  execute(args: unknown, ctx: ToolContext): Promise<ToolResult>;
}
```

Kurallar:
- `execute` icinde ilk is **schema dogrulama** — `args: unknown` tipi bilerek boyle
- Risk 2 ve 3 icin `requiresApproval: true`; bu deger hicbir risk seviyesinde config ile
  gevsetilemez (`ASUNA_TOOL_APPROVAL_MODE` risk 2/3'u bypass edemez)
- Her tool: dar amac, calisma dizini kisiti (kayitli project root), timeout, yapisal `ToolResult`,
  `tool_events` audit kaydi
- Kisitsiz shell yok — `run_any_shell_command(cmd)` tarzi tool yazilmaz. Scoped tool ac:
  `run_tests`, `git_status`, `git_diff`, `start_dev_server`
- Path'ler normalize + resolve edilir; traversal reddedilir. `.env`, SSH key, keychain, token,
  sertifika dosyalari varsayilan blocklu
- Tool sonucu secret degeri **dondurmez**; ayricalikli isi yapar, degeri sizdirmaz

## Prompt Dosyalari
- Prompt'lar kod icinde string literal olarak dagitilmaz — `src/asuna/prompts/` altinda versiyonlu dosyalar
- Isimlendirme: `core.v1.ts`, `memory-extraction.v1.ts` — degisiklikte yeni versiyon dosyasi,
  eskisi silinmeden birakilabilir; aktif versiyon tek yerden secilir
- Prompt'a volatil veri gomulmez; degisken context runtime'da `buildAsunaInstructions(context)` ile enjekte edilir
- Prompt degisikligi davranis degisikligidir — `asuna-docs/DECISIONS.md`'ye kaydedilir

## Hata Yonetimi
- **Sessiz yutma yok.** Bos `catch {}` ya da yalnizca `console.log` ile gecistirme yasak
- Yakalanan hata ya islenir ya da context eklenerek yeniden firlatilir
- **Tool basarisi taklit edilmez.** Tool hata verdiyse model ve kullanici bunu oldugu gibi gorur:
  "Projeyi acmayi denedim ama VS Code komutu bulunamadi." — "actim" denmez
- Erisilemeyen context uydurulmaz: okunmamis dosya, cekilmemis git durumu, alinmamis memory
  varmis gibi konusulmaz
- Bozulan alt sistem tum urunu dusurmez: memory DB hatasi oturumu bitirmez, durum kullaniciya
  gorunur sekilde bildirilir ve konusma memory'siz devam eder (PROJECT.md 30)
- Hata mesajlari ic detay sizdirmaz (stack trace, dosya yolu, secret); log'a giden ile
  kullaniciya gosterilen ayrilir

## Database
- Tablo adlari `snake_case` cogul (`memories`, `tool_events`), kolonlar `snake_case`
- TS tarafinda `camelCase` — donusum repository sinirinda yapilir, uygulama icine snake_case sizmaz
- Zaman alanlari UTC ISO-8601; `created_at` / `updated_at` her tabloda
- Silme yerine `is_archived` (memory kullanici tarafindan gercekten silinebilir olmali — PROJECT.md 20)
- Ham SQL tek yerde (repository katmani); servisler ve bilesenler SQL gormez
- Migration'lar versiyonlu ve ileri yonlu; elle DB duzenleme yok

## Frontend
- Fonksiyon bilesenler + hook'lar; is mantigi bilesene degil servise/hook'a
- Bilesenler durum *gosterir*, durum *uretmez* — voice state machine disinda paralel durum tutulmaz
- Data fetching/IPC cagrilari hook ya da servis katmaninda; render icinde yan etki yok
- UI ana urun degil: gorunur olmasi gerekenler — dinleme, baglanti, konusma, tool kullanimi,
  onay istegi, hata, aktif proje

## Testing
- Happy path + error case minimum
- Guvenlik/izin/path mantigi icin test **zorunlu**: path sandbox, traversal reddi, risk seviyesi
  karari, approval gate, secret redaksiyon
- Ayrica test edilir: memory ranking, project detection, tool schema dogrulama, state gecisleri
- Harici servisler (OpenAI, Porcupine) mock'lanir — test gercek API'ye vurmaz, para harcamaz
- Bug fix'te once bug'i ureten test (kirmizi → yesil)

## Git & Commit
- Commit formati: `feat(ASU-XXX): aciklama` — tip olarak `feat` / `fix` / `refactor` / `docs` /
  `test` / `chore`
- Task ID formati `ASU-001`; her commit bir task'a baglanir
- Claude/Anthropic attribution satiri **yok** (`Co-Authored-By: Claude` vb. eklenmez)
- Buyuk degisiklik oncesi `git status` kontrol edilir; ilgisiz kullanici degisikligi ezilmez
- Push/deploy sadece acikca istendiginde

# Testing Strategy — Asuna

> Kaynak: PROJECT.md Bolum 31 (Testing Strategy) + Bolum 19/20 (guvenlik & gizlilik),
> AGENT-SPEC-ORIGINAL.md "Code quality" ("Add tests for security/permission/path logic").
>
> Asuna sesli bir companion — asil deneyim otomatize edilemez. Bu yuzden strateji iki ayakli:
> **guvenlik/mantik katmani sikica unit test edilir**, ses/UX katmani **manuel kabul testi** ile dogrulanir.

## Test Stack

| Katman | Arac | Not |
|--------|------|-----|
| TS unit + integration | **Vitest** (karar — bkz. `tech-stack.md`) | Vite/Tauri kurulumu ile ayni transform pipeline — ayri Jest config'i gerekmez |
| Rust (Tauri tarafi) | `cargo test` | Ephemeral token minting, komut handler'lari, path guard'in Rust'ta olan kismi |
| E2E / UI | Ertelendi | MVP'de yok; Phase 4+ ihtiyac dogarsa Playwright/WebdriverIO degerlendirilir |

**Phase 0'da netlesecek:** SQLite erisim yolu (`tauri-plugin-sql` vs Rust servis) integration testlerinin
nerede kosacagini belirler — memory testleri TS tarafinda mi in-memory SQLite ile, yoksa Rust tarafinda mi
yazilacak Phase 0 arastirmasi sonrasi kesinlesir. → **ACIK SORU**

## Unit Tests

PROJECT.md Bolum 31'deki liste. Isaretli olanlar **zorunlu** — bunlar guvenlik sinirlari, test edilmeden merge yok.

| Alan | Kapsam | Zorunlu |
|------|--------|---------|
| Memory ranking | Retrieval skorlama (recency, importance, project match), Stage A/B siralamasi, tie-breaking | — |
| Permission logic | Risk 0-3 → approval karari, `ASUNA_TOOL_APPROVAL_MODE` etkisi, risk 2/3'un bypass EDILEMEDIGI | **Evet** |
| Path sandboxing | Normalize + resolve, root disi reddi, `../../.ssh/id_ed25519` traversal denial, symlink escape, blok listesi (`.env`, key, keychain), max dosya boyutu | **Evet** |
| Project detection | Path → kayitli proje eslesmesi, git root tespiti, framework/dil cikarimi, eslesmeyen path davranisi | — |
| Tool schemas | Her tool'un zod schema'si: gecerli arg kabul, gecersiz arg red, ekstra alan davranisi, `risk`/`requiresApproval` alanlarinin dolulugu | **Evet** |
| State transitions | `IDLE_WAKE_WORD → WAKING → CONNECTING → LISTENING → USER_SPEAKING → ASSISTANT_THINKING → ASSISTANT_SPEAKING → IDLE_WAKE_WORD`; gecersiz gecislerin reddi, idle timeout, interrupt, `ERROR` sonrasi `IDLE_WAKE_WORD`'e donus (kanonik liste: `asuna-config/conventions.md`) | — |
| Secret redaction | Log/audit/`arguments_redacted` icinde API key, token, parola pattern'lerinin maskelenmesi | **Evet** |

## Integration Tests

Harici servis (OpenAI Realtime) **her zaman mock'lanir** — gercek API'ye test vurmaz.

- **Ephemeral token endpoint** — token uretilir, kisa omurlu, kalici `OPENAI_API_KEY` response'ta/log'da YOK,
  hata durumunda (upstream 4xx/5xx) anlamli hata doner
- **Realtime session lifecycle** — connect → active → disconnect; idle timeout ile kapanma; network
  hatasinda temiz idle'a donus; kapali session'da tekrar disconnect cagrisinin patlamamasi
- **Tool call round trip** — model tool cagirir → schema validate → (gerekirse) approval → execute →
  structured result → `tool_events` kaydi; reddedilen ve timeout olan cagrilar da audit'e yazilir
- **Memory storage** — candidate memory yazilir/okunur, retrieval dogru kaydi getirir, silme calisir,
  `ASUNA_MEMORY_ENABLED=false` iken yazma yapilmaz
- **Session finalization** — session kapanisi: summary uretimi, candidate memory extraction, proje
  context guncellemesi, transcript saklama flag'ine uyum

## Manuel Kabul Testleri

PROJECT.md Bolum 31 listesi. Her faz sonunda elle kosulur, sonucu ilgili task'in kapanis notuna yazilir.

### Voice

| # | Senaryo | Beklenen | Faz |
|---|---------|----------|-----|
| V1 | "Hey Asuna" de | Wake tespit edilir, overlay acilir, kisa acknowledgement ("Buradayim.") | 2 |
| V2 | Aktivasyonu dogrula | Realtime session baglanir, UI "dinliyor" state'i gosterir | 1-2 |
| V3 | Turkce konus | Dogru anlama + dogal Turkce yanit | 1 |
| V4 | Yanit sirasinda sozunu kes | Asistan konusmayi keser, yeni girdiyi dinler (interruption) | 1 |
| V5 | Devam et | Kesilen konusma sonrasi baglam kaybolmaz | 1 |
| V6 | Session'i kapat | "Tamam Asuna." / timeout / stop butonu → cloud audio durur, idle'a donulur | 1-2 |

### Memory

| # | Senaryo | Beklenen | Faz |
|---|---------|----------|-----|
| M1 | Asuna'ya bir proje karari soyle | Karar candidate memory olarak yakalanir | 3 |
| M2 | Session'i kapat | Session summary + kabul edilen memory'ler persist edilir | 3 |
| M3 | Yeni session baslat | Onceki session'in ham transcript'i degil, ozet/memory yuklenir | 3 |
| M4 | "Ne karar vermistik?" diye sor | Dogru karari hatirlar ve kaynagini soyleyebilir | 3 |

### Tools

| # | Senaryo | Beklenen | Faz |
|---|---------|----------|-----|
| T1 | "Su an hangi projedeyim?" | `get_current_project` calisir, dogru proje doner | 4 |
| T2 | "Projeyi ac" | `open_project` (risk 1) — konfigure edilmis editor acilir | 5 |
| T3 | UI tool cagrisini logluyor mu | Tool cagrisi UI'da gorunur + `tool_events` kaydi olusur | 4-5 |
| T4 | Risk 2/3 tool dene | Net onay istegi cikar; onaysiz calismaz | 5 |
| T5 | Sandbox disi dosya iste (`~/.ssh/id_ed25519`) | Reddedilir, icerik donmez, red audit'e yazilir | 5 |

### Privacy

| # | Senaryo | Beklenen | Faz |
|---|---------|----------|-----|
| P1 | Idle modda bekle | Buluta ses gitmez (network trafigi ile dogrula), Realtime session kapali | 2 |
| P2 | Log'lari incele | State transition'lar gorunur, secret YOK | 2-3 |
| P3 | Secret redaction dogrula | Log redaction Faz 1'de; `tool_events.arguments_redacted` maskeleme Faz 5'te dogrulanir | 1 (log) / 5 (`tool_events`) |
| P4 | Memory'yi incele/sil | UI'dan memory listelenir, tek kayit silinir, silinen geri gelmez | 3 |
| P5 | `ASUNA_TRANSCRIPT_STORAGE=false` | Transcript diske yazilmaz | 3 |

## Minimum Kriterler (Gate 2)

Her task icin:

- Yeni davranisin **happy path** testi
- En az bir **error case** testi (gecersiz input, sandbox disi path, onay reddi, baglanti hatasi)
- Bug fix'lerde: once bug'i ureten test (kirmizi → yesil)
- Guvenlik/permission/path'e dokunan her degisiklikte ilgili unit test **zorunlu** — istisna yok
- Sesli/UX degisikliklerinde: ilgili manuel kabul testinin kosuldugu ve sonucu task notunda yazili

## Kurallar

- OpenAI Realtime, Porcupine, filesystem ve shell **her zaman** mock'lanir — test gercek servise/mikrofona vurmaz
- Path testleri gercek dosya sistemi yerine fixture/temp dizin kullanir; test sonunda temizler
- Test DB izole: her run temiz SQLite (in-memory veya temp dosya), truncate/rollback
- Zaman ve rastgelelik sabitlenir (fake timer, sabit seed) — flaky test kabul edilmez;
  ozellikle timeout/idle-timeout testlerinde gercek `sleep` yok
- Test adi davranisi anlatir: `rejects path traversal outside registered project root`
- Testler kod ile ayni PR'da gelir — "testleri sonra yazariz" yok
- Test icinde gercek secret/API key bulunmaz; fixture'lar sahte deger kullanir

## Coverage

- Sayi fetisi yok. Kritik moduller **%90+**: `security/` (path sandbox, redaction),
  `tools/permissions.ts`, tool registry/schema, ephemeral token minting
- Geri kalani anlamli senaryo kapsamasi
- Coverage raporu CI'da uretilir (`vitest --coverage`), dusus PR'da gorunur olmali
- Rust tarafi: `cargo test` CI'da kosar; coverage esigi Phase 0'dan sonra degerlendirilir

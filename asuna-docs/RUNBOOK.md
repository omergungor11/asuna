# Runbook

> Operasyonel bilgi — calistirma, build, CI, geri alma.
> Kural: buradaki her adim KOPYALA-YAPISTIR calisir olmali; "aslinda su da gerekiyordu" yok.
>
> **Asuna bir sunucu degil, lokal masaustu uygulamasidir.** "Deploy", "staging", "health
> endpoint", "rollback image tag" kavramlari bu projede **yok**. Kurulum ve komutlar:
> [`README.md`](../README.md) → *Local Kurulum*.

## Ortamlar

| Ortam | Ne demek | Nasil calisir |
|-------|----------|---------------|
| dev | Vite dev server + Tauri webview | `pnpm tauri dev` (webview `http://localhost:1420`) |
| paketlenmis | Yerel `.app` / `.dmg`, imzasiz | `pnpm tauri build` → `src-tauri/target/release/bundle/` |
| dagitim | **Henuz yok** | Imzalama + notarization + release: ASU-063 (Phase 6) |

**dev ≠ paketlenmis.** Farki olan, sessiz hataya yol acan iki nokta:

1. **CSP dev'de uygulanmaz** — sayfayi Vite servis eder. `connect-src` hatasi yalnizca
   paketlenmis build'de gorunur (`docs/architecture/security.md` Bolum 5).
2. **TCC (mikrofon) izni** dev binary ile paketlenmis `.app` icin ayri verilir; dev'de her
   rebuild sonrasi prompt tekrar cikabilir.

CSP, capability, `Info.plist` veya mikrofon davranisini degistiren her degisiklik
**paketlenmis build'de** dogrulanir.

## Calistirma

```bash
pnpm install
pnpm tauri dev
```

On kosul: `.env` dolu (`cp .env.example .env`). Eksik/gecersiz degisken varsa uygulama
acilista net bir hata mesajiyla durur — sessiz varsayilan yok.

## Build

```bash
pnpm typecheck && pnpm lint && pnpm test
pnpm rust:fmt && pnpm rust:lint && pnpm rust:test
pnpm tauri build          # -> src-tauri/target/release/bundle/macos/Asuna.app
```

**Build sonrasi zorunlu kontrol** (secret sizintisi):

```bash
grep -r "OPENAI_API_KEY" dist/ && echo "SIZINTI" || echo "temiz"
```

Cikti `temiz` degilse build dagitilmaz — bu bir merge engelidir.

## CI

GitHub Actions: `.github/workflows/ci.yml`. Kosulanlar: typecheck, ESLint, Prettier check,
Vitest, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, Tauri build.

- Yerel komutlar CI ile birebir ayni — CI'da kirilan bir sey lokalde de kirilir.
- Rust surumu `rust-toolchain.toml` ile pinli (1.96.1); toolchain yukseltmesi bilincli bir
  commit + `DECISIONS.md` kaydidir.
- CI kirmiziyken merge yok.

## Geri alma (rollback)

Yayinlanmis surum kavrami henuz yok; geri alma = **kaynak seviyesinde**.

```bash
git log --oneline -10
git revert <commit>        # ters commit — history yeniden yazilmaz
pnpm install               # bagimlilik degistiyse
pnpm tauri build           # kullanicinin .app'ini yeniden uret
```

Migration iceren bir degisikligi geri alirken:

- Sema `rusqlite_migration` ile `PRAGMA user_version` uzerinden yonetilir; her `M::up`'in
  bir `M::down`'i vardir (ADR-005).
- Kod geri alinirsa DB **ileri surumde kalir**. Eski binary yeni semayi actiginda davranis
  garanti degil → once DB'yi yedekle, gerekiyorsa `to_version(<eski>)` ile dus.
- Migration dosyalari **hicbir zaman duzenlenmez**; duzeltme yeni bir `M` ekler.

## Veri: DB dosyasi ve yedekleme

| Ne | Yol |
|---|---|
| DB | `~/Library/Application Support/com.omergungor.asuna/asuna.db` |
| WAL kardesleri | `asuna.db-wal`, `asuna.db-shm` |
| Dev override | `ASUNA_DB_PATH` — **yalnizca** debug build'lerde okunur |

- Yol renderer'dan asla parametre alinmaz; `app_data_dir()` ile Rust tarafinda cozulur.
- **WAL modu acik: sadece `asuna.db`'yi kopyalamak yedek degildir.** Uygulama kapaliyken
  uc dosyayi birlikte kopyala, ya da acikken tutarli tek dosya icin:

```bash
sqlite3 ~/Library/Application\ Support/com.omergungor.asuna/asuna.db \
  "VACUUM INTO '$HOME/asuna-backup-$(date +%F).db'"
```

- `Application Support` Time Machine yedegine dahildir.
- Temiz baslangic (dev): uygulamayi kapat, `asuna.db*` dosyalarini sil, yeniden ac —
  migration'lar sifirdan kosar.

## Incident

1. **Tespit** — uygulama acilmiyor / ses gelmiyor / tool yanlis davraniyor. Log seviyesi
   `ASUNA_LOG_LEVEL=debug` ile artirilir (log'larda secret redakte edilir).
2. **Etki degerlendir** — kullanici verisi (memory/transcript) risk altinda mi, bir secret
   sizmis olabilir mi.
3. **Mudahale** — once calisir duruma don (`git revert` + rebuild), sonra kok neden.
   Secret sizintisi suphesi varsa **once API key'i rotate et**, sonra hata ayikla.
4. **Kayit** — `asuna-docs/MEMORY.md` → Gotchas; mimari sonuc doguruyorsa `DECISIONS.md`;
   CRITICAL guvenlik bulgusu ayrica ayni gun fix (`asuna-config/security.md` → Escalation).

## Sik karsilasilanlar

| Belirti | Sebep / cozum |
|---|---|
| Ses dev'de calisiyor, `tauri build` sonrasi sessiz | CSP `connect-src` — `api.openai.com` ekli mi (`tauri.conf.json`) |
| `invoke(...)` sessizce reddediliyor | ACL: komut `build.rs` `AppManifest` listesinde + `permissions/` + capability + `tauri.conf.json → app.security.capabilities` |
| Realtime `401` / `429` | API key gecersiz veya API billing aktif degil (ChatGPT aboneligi yeterli **degil**) |
| Mikrofon TCC ihlaliyle patliyor | `NSMicrophoneUsageDescription` tam olarak `src-tauri/Info.plist` icinde olmali |
| Turuncu mikrofon gostergesi sonmuyor | Track'ler durdurulmamis — `mediaStream` sahipligi (`docs/architecture/voice.md` Bolum 4) |

## Yetkiler / erisimler

- Tek gelistirici; deploy yetkisi kavrami yok.
- `OPENAI_API_KEY`: lokal `.env` (git disi), yalnizca Rust process'inde okunur.
  Hedef: macOS Keychain (post-MVP).
- CI secret'i yok — CI aga cikan test kosmaz.
- Release imzalama sertifikasi (Apple Developer ID) ASU-063 ile gundeme gelir; o zamana
  kadar dagitim yok.

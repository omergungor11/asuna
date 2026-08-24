# Security Checklist — Asuna

> /code-review ve reviewer agent bu listeyi kullanir. Kaynak: PROJECT.md Bolum 5.4, 8, 18, 19, 20;
> AGENT-SPEC-ORIGINAL.md "Security" + "Tool rules".
>
> Asuna local-first bir sesli companion — surekli acik mikrofon, kullanicinin gercek dosya
> sistemine erisen tool'lar ve kalici hafiza var. Guvenlik burada "sonra eklenecek katman" degil,
> urunun kendisi. CRITICAL bulgu = merge engeli.

## 1. Secrets

- [ ] `OPENAI_API_KEY` **asla** renderer/webview bundle'ina girmez — Vite `import.meta.env` uzerinden
      client'a sizmaz (`VITE_` prefix'i ile expose edilmez)
- [ ] Ephemeral Realtime token'i sadece guvenilir process (Tauri Rust tarafi) uretir; kalici key
      o process'in disina cikmaz
- [ ] Ephemeral token kisa omurlu; sureli/expired token yenilenir, log'a yazilmaz, disk'e persist edilmez
- [ ] Model ID'leri hard-code edilmez — `ASUNA_REALTIME_MODEL` (default `gpt-realtime-2.1`,
      dev/ekonomi `gpt-realtime-2.1-mini`) tek merkezden okunur
- [ ] `.env` git disi; `.env.example` gercek deger icermez (bkz. PROJECT.md Bolum 23)
- [ ] Modele/LLM'e asla verilmez: `.env` icerigi, SSH key'ler, keychain secret'lari, GitHub/cloud
      token'lari, private sertifikalar, cloud credential dosyalari
- [ ] **Tool'lar secret DEGERI dondurmez.** Ayricalikli islemi tool kendi icinde yapar; sonuc olarak
      "yapildi/yapilmadi + ozet" doner, credential icerigi degil
- [ ] Log, hata mesaji, session summary, transcript ve `tool_events.arguments_redacted` alaninda
      secret maskelenir — redaction unit test'i var
- [ ] Crash/error raporunda stack trace ile birlikte env dump edilmez

### Blok listesi (varsayilan deny, explicit approval olmadan okunmaz)

```text
.env, .env.*            ~/.ssh/**              ~/.aws/**, ~/.config/gcloud/**
*.pem, *.key, *.p12     ~/Library/Keychains/** ~/.npmrc, ~/.netrc, ~/.gitconfig (credential helper)
id_rsa, id_ed25519      *.keystore, *.jks      **/secrets/**, **/credentials*
```

- [ ] Blok listesi merkezi bir modulde (`src/asuna/security/`) tanimli, tool'lar kendi kopyasini tutmaz
- [ ] Blok listesi glob eslesmesi symlink cozuldukten **sonra** uygulanir

## 2. Filesystem Sandbox

- [ ] Her dosya tool'u **kayitli proje root'u** (registered project root) alir; root disi erisim yok
- [ ] Path once normalize + resolve edilir (`realpath`), sonra root prefix kontrolu yapilir —
      string `startsWith` ile ham input kontrolu yeterli degil
- [ ] Path traversal reddedilir. Ornek denial: `../../.ssh/id_ed25519`
- [ ] Symlink escape reddedilir (root icindeki symlink root disini gosteriyorsa deny)
- [ ] Max dosya boyutu siniri (`read_project_file`) — buyuk dosya context'i sisirmeden reddedilir/kesilir
- [ ] Binary dosya tespiti — binary icerik modele ham gonderilmez
- [ ] Yazma islemi MVP'de sadece `.asuna/notes/` altina (`create_project_note`); baska yere yazma yok
- [ ] Proje root'lari kullanici tarafindan explicit kaydedilir; otomatik "her yeri tara" davranisi yok
- [ ] Path sandbox mantigi icin **zorunlu** unit test (bkz. testing.md)

## 3. Tool Guvenligi

Risk seviyeleri (PROJECT.md Bolum 5.4):

| Risk | Tanim | Onay | Ornekler |
|------|-------|------|----------|
| 0 | Read-only | Gerekmez | `read_project_file`, `get_git_status`, `get_current_project`, `list_recent_project_activity` |
| 1 | Geri alinabilir, dusuk riskli | Konfigure edilebilir | `open_project`, `create_project_note`, draft dosya, proje degistir |
| 2 | Mutation | MVP'de **her zaman** net onay | dosya duzenle, paket kur, build calistir, commit |
| 3 | Destructive / external | **Her zaman** explicit onay | dosya sil, remote'a push, mail gonder, publish, deploy, para harcama, sistem ayari |

- [ ] Her model-erisimli tool'da: explicit isim, dar amac, schema validation (zod), `risk: 0|1|2|3`,
      `requiresApproval`, timeout, structured result, audit event
- [ ] **Unrestricted shell YASAK** — `run_any_shell_command(command: string)` gibi bir tool modele
      asla expose edilmez (PROJECT.md Bolum 18)
- [ ] Shell ihtiyaci scoped tool'lar ile karsilanir: `run_tests`, `run_lint`, `git_status`, `git_diff`,
      `npm_install_package`, `start_dev_server` — her biri argumanini validate eder, working directory'si
      kisitlidir, timeout'u vardir, stdout/stderr'i capture eder
- [ ] Scoped tool argumanlari shell'e string olarak birlestirilmez (arg array / no shell interpolation)
- [ ] Approval mode konfigurasyonu (`ASUNA_TOOL_APPROVAL_MODE=safe`) risk 2/3'u bypass edemez
- [ ] Onay istegi kullaniciya **ne yapilacagini** gosterir (tool adi + redacted argumanlar), sadece
      "izin ver?" demez
- [ ] Her tool cagrisi `tool_events` tablosuna yazilir: `time, tool_name, risk_level,
      arguments_redacted, approval_state, result_summary` (basarili/basarisiz dahil)
- [ ] Reddedilen/timeout olan cagrilar da audit'e yazilir — sessizce yutulmaz
- [ ] Tool hatasi modele anlamli ama ic detay sizdirmayan mesajla doner (mutlak path, stack trace yok)
- [ ] React component'leri dogrudan shell/DB cagirmaz — tool katmani uzerinden gecer

## 4. Ses Gizliligi

- [ ] Wake word (`Hey Asuna`) tespiti **tamamen lokal** — Porcupine on-device, `WakeWordProvider`
      adapter arkasinda
- [ ] Idle durumda: Realtime session **kapali/disconnected**, buluta giden ses **YOK**
- [ ] Idle mikrofon frame'leri persist edilmez — diske yazilmaz, buffer session sonrasi temizlenir
- [ ] Wake sonrasi wake-word engine durdurulur/askiya alinir, sonra Realtime session acilir
- [ ] Aktif dinleme UI'da **gorunur** (tray/overlay state) — kullanici dinlenip dinlenmedigini
      her an bilir
- [ ] Session kapanisinda cloud audio akisi kesilir, session disconnect edilir, idle state'e donulur
- [ ] Transcript saklama konfigure edilebilir (`ASUNA_TRANSCRIPT_STORAGE`) — kapaliyken transcript
      diske yazilmaz
- [ ] Gizli/arka planda ekran yakalama YOK (no hidden screen capture)
- [ ] Mikrofon izni reddedildiginde acik hata gosterilir, sessizce dinlemeye devam edilmez

## 5. Memory Gizliligi

- [ ] Hafiza **inceleneblir**: UI'da memory listesi, kaynagi (`source_session_id`) ve olusma zamani gorunur
- [ ] Hafiza **silinebilir**: tek kayit sil + toplu temizle
- [ ] Durable memory tamamen kapatilabilir (`ASUNA_MEMORY_ENABLED=false`)
- [ ] Tum transcript "memory" olarak saklanmaz — ayrik katmanlar: raw/optional transcript,
      session summary, candidate durable memories, project decisions, tasks, preferences
- [ ] Hassas kategoriler (saglik, finans, kimlik, ucuncu sahis bilgisi, credential benzeri icerik)
      otomatik yazilmaz — kullanici onayi ister
- [ ] Memory extraction secret pattern'lerini (API key, token, parola) filtreler — sizan degeri saklamaz
- [ ] Retrieval sadece ilgili hafizayi cagirir; her session'da tum DB modele dokulmez
- [ ] SQLite dosyasi kullanici home'unda, uygulama data dizininde; repo icinde degil, git'e girmez

## 6. Genel Uygulama Guvenligi

- [ ] Tauri: `capabilities`/permission listesi minimal — sadece gercekten kullanilan plugin komutlari acik
- [ ] Webview'de `dangerouslySetInnerHTML` / raw HTML sadece sanitize edilmis icerikle (transcript,
      memory icerigi model ciktisidir — DOM'a ham basilmaz)
- [ ] SQL: sadece parametrize sorgu / ORM — string birlestirme yok
- [ ] Tum tool input'u ve IPC payload'u schema ile dogrulanir (zod); Rust tarafi kendi validasyonunu
      yapar, renderer'a guvenmez
- [ ] Bagimlilik audit'i CI'da (`pnpm audit`, `cargo audit` — high+ fail)
- [ ] TypeScript strict mode; guvenlik yollarinda `any` / unchecked cast yok
- [ ] Log'larda PII/secret maskeleme; state transition log'lari (Bolum 29) secret icermez

## Escalation

CRITICAL guvenlik bulgusu → merge engeli + ayni gun fix + `asuna-docs/DECISIONS.md` ve MEMORY'ye kayit.

Bu dosyada karari verilmemis, Phase 0'da netlesecek konular:

- SQLite erisim yolu (`tauri-plugin-sql` vs Rust tarafi servis) — secret/DB dosya izinlerini etkiler → **ACIK SORU**
- Ephemeral token minting'in tasima yolu → **Karara baglandi:** Tauri command `mint_realtime_token`
  (ADR-006 / ASU-011); lokal loopback HTTP endpoint'i secilmedi

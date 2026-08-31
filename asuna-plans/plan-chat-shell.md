# Plan: Chat Shell — ChatGPT/Claude-tarzi arayuz pivotu

> Durum: **DONE (2026-08-31)** — ASU-072..078 COMPLETED; acik olan tek madde ASU-079 (M6 kabul testi, kullanicida). Sahip: orchestrator (Fable). Uygulayicilar: database / backend / frontend / tester / reviewer / docs agent'lari (opus).
>
> **PIVOT KARARI (kayitli hali: `asuna-docs/DECISIONS.md` → ADR-008; asagidaki "ADR-006" ifadesi eskidir — ADR-006/007 2026-08-24'te alinmisti):** Kullanici karari ile Asuna, ChatGPT/Claude benzeri bir
> arayuze donusuyor: konusma gecmisi kalici ve gorunur, dosya eklenebilir, projelerde
> konusma baslatilabilir. Eski "generic chatbot UI kurma" prime directive'i bu kararla
> **degistirildi**. Ses katmani SILINMIYOR — ChatGPT'deki gibi bir "voice mode" olarak kaliyor.
> VoicePanel'in her zaman monte kalma kurali (app.tsx yorumu) korunuyor.

## Kavram eslemesi

| ChatGPT/Claude kavrami | Asuna karsiligi |
|---|---|
| Conversation | `sessions` satiri (mevcut tablo genisletilir: `title`, `modality`) |
| Message | YENI `messages` tablosu (session'a FK, CASCADE delete) |
| Attachment | YENI `attachments` tablosu (redakte edilmis metin icerik DB'de saklanir) |
| Project | Mevcut `projects` tablosu / ProjectRegistry |
| Projede session baslatma | `session_start(project_id, modality='text')` |

## Degismez kurallar (tum agent'lar icin)

- `OPENAI_API_KEY` renderer'a asla gitmez; chat cagrisi RUST tarafinda (summary.rs deseni).
- Model ID hard-code edilmez: YENI env `ASUNA_CHAT_MODEL` (ornek deger `gpt-4o-mini`).
- Yayinlanmis migration dosyalari degistirilmez → yeni migration `006_conversations`.
- Attachment icerigi DB'ye yazilmadan once `redaction::redact_secrets`ten gecer; boyut siniri var.
- Proje dosyasi ekleme mevcut sandbox + blocklist'ten gecer (`projects::files` altyapisi).
- Working tree'deki commit'lenmemis degisikliklere (listing.rs, sandbox.rs, registry.rs,
  commands.rs, build.rs, tauri.conf.json, backlog.md) DOKUNULMAZ; uzerine insa edilir.
- TS strict, `any` yasak; kebab-case; testsiz security/path mantigi merge edilmez.

## Is paketleri

### WP1 — database agent (src-tauri/src/db/ SADECE)

Migration `006_conversations.up.sql` + `.down.sql` (003'un ustune, sema surumu 6):

- `ALTER TABLE sessions ADD COLUMN title TEXT CHECK (title IS NULL OR length(title) > 0);`
- `ALTER TABLE sessions ADD COLUMN modality TEXT NOT NULL DEFAULT 'voice' CHECK (modality IN ('voice','text'));`
- YENI `messages` (STRICT): `id INTEGER PK`, `session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE`,
  `role TEXT CHECK (role IN ('user','assistant','system','tool'))`, `content TEXT NOT NULL CHECK (length(content) > 0)`,
  `created_at` (mevcut GLOB deseni), `metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(...))`.
  Index: `(session_id, id)`.
- YENI `attachments` (STRICT): `id PK`, `session_id ... ON DELETE CASCADE`,
  `message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL`,
  `file_name TEXT NOT NULL CHECK(length>0)`, `mime_type TEXT`, `size_bytes INTEGER CHECK (>=0)`,
  `origin TEXT NOT NULL CHECK (origin IN ('upload','project'))`,
  `content TEXT NOT NULL` (redakte EDILMIS metin — redaksiyon repo'da degil komut katmaninda),
  `created_at`. Index: `(session_id, id)`.
- NOT: sessions DELETE'i artik messages/attachments'i CASCADE ile goturur — `session_delete`
  ve `session_clear_all` davranisi degismez ama testi yazilir.

Yeni repo dosyalari (mevcut `memory_repository.rs` desenini birebir izle — komut fn'leri
`#[tauri::command]`, DbState uzerinden, StoreError):

- `message_repository.rs`: `message_append(session_id, role, content) -> MessageRecord` (komut),
  `message_list(session_id) -> Vec<MessageRecord>` (komut), dahili `append_in_tx` yardimcisi.
- `attachment_repository.rs`: `attachment_store_record(...)` dahili + `attachment_list(session_id)` komutu.
- `session_repository.rs` GENISLET (dosyanin mevcut stiline uy): `session_start` opsiyonel
  `modality` parametresi (varsayilan 'voice' — mevcut cagiranlari ve TS parser'larini KIRMA;
  `session_list` yanitina YENI ALAN EKLEME — `shared/session.ts` strict parse ediyor),
  yeni komut `session_set_title(session_id, title)`.
- YENI komut `conversation_list` (message_repository ya da ayri `conversation_repository.rs`):
  chat UI'nin listesi — `session_list`e dokunmadan. Donen JSON (camelCase) satir basina:
  `{ id, title, modality, projectId, startedAt, lastActivityAt, messageCount }`
  (`lastActivityAt` = son mesajin created_at'i, yoksa startedAt; siralama buna gore DESC).
  Bicim sozlesmesi: `src/shared/chat.ts` (ORCHESTRATOR YAZDI — birebir uy).
- `model.rs`'e `MessageRecord`, `AttachmentRecord`, `MessageRole` ekle (serde camelCase).
- Rust unit testleri ayni dosyalarda (mevcut desen): append/list, CASCADE delete,
  bos content reddi, migration up/down dogrulamasi.
- lib.rs'e DOKUNMA (komut kaydini backend agent yapar). commands.rs'e DOKUNMA.

### WP2 — backend agent (src-tauri/src/chat.rs + config.rs + lib.rs + build.rs/capabilities + .env.example)

> NOT: `src/asuna/agent/chat-service.ts` ve `src/shared/chat.ts` ORCHESTRATOR tarafindan
> yazildi — bunlar SOZLESMEDIR. Rust komutlari bu dosyalardaki adlara, arguman adlarina
> (camelCase invoke argumanlari) ve donen JSON bicimine BIREBIR uymali. Sozlesme dosyasini
> degistirmek gerekiyorsa gerekcesiyle raporla, sessizce degistirme.
> AYRICA: yeni komutlarin TUMUNU (WP1'in komutlari dahil) `lib.rs` generate_handler'a,
> `build.rs` ACL manifest'ine ve yeni `capabilities/asuna-chat.json` dosyasina kaydet —
> mevcut capability dosyalarini ornek al. build.rs working tree'de degisik durumda;
> mevcut degisiklikleri KORU, uzerine ekle.

1. `config.rs`: `ASUNA_CHAT_MODEL` (zorunlu, bos olamaz — mevcut desen). `.env.example`'a
   aciklamali satir (`gpt-4o-mini` ornek; ASUNA_SUMMARY_MODEL yorum stilinde).
2. YENI `src-tauri/src/chat.rs` — `summary.rs` HTTP desenini izle (reqwest, redaksiyonlu hatalar,
   timeout'lar, API yaniti sizdirmayan hata tipleri):
   - Komut `chat_send(session_id: i64, text: String, attachment_ids: Vec<i64>) -> ChatReply`.
     Akis: (a) text bos/asiri uzunsa reddet (max 32_000 karakter);
     (b) DB'den son 40 mesaji + verilen attachment kayitlarini oku (attachment'lar BU session'a ait olmali, degilse reddet);
     (c) sistem prompt'u: Asuna kimligi (kisa, Turkce) + session'in projesi varsa proje adi/yolu;
     (d) OpenAI `/v1/chat/completions` non-streaming cagri (`ASUNA_CHAT_MODEL`);
     (e) kullanici mesajini ve asistan yanitini `messages`e yaz (attachment.message_id'leri kullanici mesajina bagla);
     (f) `ChatReply { userMessage, assistantMessage }` don. DB yoksa (memory kapali) durust hata.
   - Komut `attachment_ingest(session_id, file_name, content, mime_type) -> AttachmentRecord`:
     renderer File API ile okudugu METNI gonderir (yeni Tauri plugin YOK). Rust tarafi:
     boyut siniri 200_000 karakter girise, `redact_secrets` uygula, 24_000 karaktere kirp
     (kirpildiysa sona `\n[... kirpildi ...]`), origin='upload' olarak kaydet. Binary/utf8-disi
     icerigi reddet (durust hata: "yalnizca metin dosyalari").
     Dosya ADI da supheli uzantiysa reddet: `.env*`, `*.pem`, `*.key`, `id_rsa*`, `*.p12`, `*.keychain` vb.
     (`security` modulundeki mevcut blocklist'i YENIDEN KULLAN, kopyalama).
   - Komut `attachment_from_project(session_id, relative_path) -> AttachmentRecord`:
     `projects::files::read(db, relative)` (files.rs:222, PUBLIC) cagrilir — sandbox + blocklist +
     redaksiyon + kirpma + truncated/redacted bayraklari icinde. `#[tauri::command]
     read_project_file` sarmalayicisini CAGIRMA, cekirdek `read`i cagir. Hata/audit icin
     `ProjectFileError::audit_summary()` / `escape_attempt()` kullan, audit satirini elle kurma.
     `read` hedefi AKTIF projeye gore cozer → V1 KURALI: session'in projectId'si aktif proje
     degilse durust hata ("once bu projeyi aktif yap"); projects/* DOSYALARINA DOKUNULMAZ.
     Sonuc origin='project' attachment olarak kaydedilir.
   - KOORDINASYON (paralel oturum asuna-81, Wave D): commands.rs / lib.rs / build.rs /
     tauri.conf.json duzenlemeleri asuna-81'in commit'i GECENE KADAR yapilmaz (haber gelecek);
     o ana kadar chat.rs/config/testler yazilir. Yeni capability dosyasi `asuna-chat.json`
     yeni dosyadir, serbest. src/asuna/tools/*, src-tauri/src/projects/*, security/* DOKUNULMAZ.
   - NOT: ASUNA_CHAT_MODEL config.rs ALL_KEYS'e girince kullanicinin `.env`'ine satir gerekir;
     `.env`'e koruma hook'u nedeniyle dokunulamaz → kullaniciya soylenecekler listesine eklendi.
3. `lib.rs` generate_handler'a yeni komutlari ekle: message_list, message_append (gerekirse),
   attachment_list, session_set_title, chat_send, attachment_ingest, attachment_from_project.
4. (dustu — chat-service.ts + shared/chat.ts orchestrator tarafindan yazildi; Rust bunlara uyar.)
5. Rust testleri: chat.rs icin (HTTP cagrisiz test edilebilen kisimlar — girdi dogrulama,
   attachment sahiplik kontrolu, redaksiyon/kirpma), config testi.

### WP3 — frontend agent (src/app/ + src/components/ SADECE; chat-service.ts'i IMPORT eder, yazmaz)

ChatGPT/Claude-tarzi kabuk (`app.tsx` yeniden yazilir, `app.css` genisletilir):

- **Sol sidebar**: "+ Yeni konuşma" butonu; konusma listesi (title yoksa "Adsız konuşma",
  tarihe gore gruplu: Bugün/Dün/Son 7 gün/Daha eski; aktif olan vurgulu; hover'da sil);
  "Projeler" bolumu (proje listesi, tiklaninca proje sayfasi); altta: Hafıza / Araçlar / Ayarlar linkleri.
- **Ana alan — ChatView** (YENI `chat-view.tsx`): mesaj listesi (kullanici sagda/vurgulu,
  asistan solda; markdown YOK v1'de, `white-space: pre-wrap`); mesaj gonderilmisken
  "yaziyor..." gostergesi; attachment cipleri (mesajin ustunde dosya adi + boyut).
- **Composer**: textarea (Enter=gonder, Shift+Enter=yeni satir), ataç butonu (gizli
  `<input type="file" multiple>` — File API ile `ingestAttachment`), konusma bir projedeyse
  "Projeden dosya ekle" (mevcut `list_project_dir` ile gezinilebilir kucuk secici),
  mikrofon butonu → mevcut VoicePanel'i acar (voice mode).
- **Baslik kurali**: ilk kullanici mesajindan sonra `setTitle(ilk 60 karakter)`.
- **Proje sayfasi**: mevcut `project-detail.tsx` genislet — "Bu projede yeni konuşma" butonu
  (session_start(projectId)) + bu projenin konusma listesi (session_list filtreli).
- **VoicePanel her zaman monte kalir** (hidden ile) — mevcut kural ve gerekce yorumu tasinir.
  Mevcut Memory/Tools/Settings view'lari aynen kullanilir, sadece sidebar'dan acilir.
- Yeni davranislarin testleri (mevcut *.spec.tsx desenleri; chat-service mock'lanir).
- src-tauri'ye ve src/asuna/'ya DOKUNMA (yalnizca import).

### WP4 — tester agent: WP1-3 sonrasi bosluk analizi + eksik testler (oncelik: CASCADE delete,
attachment sahiplik, redaksiyon, sandbox yeniden kullanimi, composer davranisi).

### WP5 — reviewer agent: tum diff uzerinde Gate 3 review (guvenlik odakli: key sizintisi,
sandbox bypass, redaksiyonsuz persist, IPC yuzeyi).

### WP6 — docs agent: ADR-006 (DECISIONS.md), CLAUDE.md prime directive guncellemesi,
task-index'e yeni phase/task'lar, CHANGELOG.

## Kabul kriterleri (Gate 2)

1. Yeni konusma ac → mesaj yaz → yanit gelir → uygulama restart → konusma listede, mesajlar yerinde.
2. Konusmayi sil → messages + attachments DB'den gercekten gider.
3. `.env` icerikli dosya eklenince secret'lar redakte edilmis saklanir; `.env` ADI reddedilir.
4. Proje disi mutlak yol ile `attachment_from_project` reddedilir.
5. `pnpm typecheck && pnpm lint && pnpm test` + `cargo test` yesil.

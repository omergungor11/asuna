-- Chat Shell — konusma (`sessions` genisletmesi) + `messages` + `attachments`
-- (plan-chat-shell.md WP1, ADR-006 pivot).
--
-- BU DOSYA YAYINLANMISTIR VE BIR DAHA DEGISTIRILMEZ. Duzeltme yeni bir
-- migration ekler (ADR-005 "Migration Karari").
--
-- ===========================================================================
-- Neden yeni bir "conversations" tablosu YOK
-- ===========================================================================
--
-- Bir konusma zaten bir oturumdur: baslangici, projesi, ozeti, token/maliyet
-- kaydi ve dokumu `sessions` satirinda duruyor. Ikinci bir tablo acmak, ayni
-- kavrami iki yerde tutmak (ve "hangisi gercek konusma?" sorusunu her sorguda
-- yeniden cevaplamak) demekti. Metin sohbeti bu yuzden var olan satiri iki
-- kolonla genisletiyor:
--
--   title    : kullanicinin gordugu baslik. NULL = henuz baslik yok; UI
--              "Adsiz konusma" yazar. Bos metin bilerek YASAK — "baslik var
--              ama bos" ile "baslik yok" ayni gorunmemeli (`tool_events`
--              ozet kolonlariyla ayni kural).
--   modality : konusma sesle mi metinle mi yurudu. Varsayilan 'voice' cunku
--              006 oncesindeki HER satir bir ses oturumudur — bu bir tahmin
--              degil, olculebilir bir gercek (metin sohbeti bu migration'dan
--              once yoktu). 002'nin `end_reason` doldurmasindaki ile ayni
--              olcut: bilgi kaydin kendisinden CIKARILABILIYORSA doldurulur.
--
-- ===========================================================================
-- `messages` — konusmanin kendisi
-- ===========================================================================
--
-- `sessions.transcript_path` ile karistirilmamali. Dokum dosyasi OPSIYONEL bir
-- ses kaydi ciktisidir (`ASUNA_TRANSCRIPT_STORAGE`), silinebilir ve varsayilan
-- olarak yazilmaz. `messages` ise metin sohbetinin **birincil** verisidir:
-- uygulama yeniden acildiginda ekranda gorunen sey burasidir.
--
-- `ON DELETE CASCADE` bilincli ve `tool_events`in tam tersi bir karar:
--
--   * `tool_events` bir DENETIM defteridir; konusmayi silen kullanici audit
--     izini silmis olmamalidir (bu yuzden orada `ON DELETE SET NULL`).
--   * `messages` ise konusmanin KENDISIDIR. Konusmayi silip mesajlarini
--     birakmak, kullanicinin "sil" dedigi seyi silmemek olurdu — PROJECT.md
--     Bolum 20'nin acikca reddettigi durum. Sahipsiz mesaj satiri diye bir
--     kavram da yok: `session_id` NOT NULL.
--
-- `metadata_json` simdilik hep '{}': ileride tool cagrisi referanslari, model
-- adi ya da token kirilimi buraya gelebilir. Bugun okunmuyor ve `MessageRecord`
-- tarafindan **bilerek** tasinmiyor (`memories.embedding` ile ayni istisna
-- disiplini) — kolon acmak, uydurma bir kolon acmaktan farkli olarak ileriye
-- donuk ucuz; her yeni alan icin migration yazmak degil.
--
-- Icerikte UST SINIR YOK. `tool_events`teki tavanlar bir denetim satirinin
-- sessizce sismesini engellemek icindi; burada icerik zaten kullanicinin
-- verisi. Bir tavan, uzun bir asistan yanitini INSERT aninda dusurur ve
-- konusmayi yarim birakirdi. Girdi tavani (32.000 karakter) komut katmaninda,
-- gonderilen metin icin uygulanir.
--
-- ===========================================================================
-- `attachments` — eklenen dosyalar
-- ===========================================================================
--
-- `content` REDAKTE EDILMIS metindir. Isim sozlesmesi `tool_events`teki
-- `arguments_redacted` ile ayni ruhta: redaksiyon (`redaction::redact_secrets`),
-- boyut siniri ve dosya-adi blocklist'i KOMUT KATMANINDA yapilir
-- (`chat::attachment_ingest` / `attachment_from_project`); repository ham metni
-- oldugu gibi yazar ve bunu doc-comment'inda soyler. Kolon bunu tesvik eder
-- ama garanti edemez — garanti, ham iceriğin buraya gelen tek yolunun o iki
-- komut olmasindan gelir.
--
-- `message_id ... ON DELETE SET NULL`: bir dosya once eklenir (composer'da
-- bekler), mesaj gonderilince ona baglanir. NULL = "henuz bir mesaja
-- baglanmadi". CASCADE olsaydi bekleyen ekler icin gecersiz bir durum
-- (baglantisi olmayan satir = silinmis satir) uretilirdi.
--
-- `session_id ... ON DELETE CASCADE`: `messages` ile ayni gerekce — konusma
-- silinince eklenen dosyanin redakte edilmis metni de gider. Bu, "konusmayi
-- sildim ama dosyamin icerigi DB'de kaldi" durumunu imkansiz kilar.

ALTER TABLE sessions ADD COLUMN title TEXT
    CHECK (title IS NULL OR (length(title) > 0 AND length(title) <= 200));

ALTER TABLE sessions ADD COLUMN modality TEXT NOT NULL DEFAULT 'voice'
    CHECK (modality IN ('voice', 'text'));

CREATE TABLE messages (
    id            INTEGER PRIMARY KEY,
    -- NOT NULL: sahipsiz mesaj diye bir sey yok. Konusma gidince mesaj da gider.
    session_id    INTEGER NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    -- Kume `IN (...)` olarak yazili (serbest metin degil): Rust `MessageRole`
    -- ve TypeScript `ChatMessage['role']` bu satira testlerle baglanir
    -- (`migrations::message_roles_declared_in_schema`).
    role          TEXT    NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    -- Bos mesaj yazilmaz: "gonderdim ama bos" ile "gondermedim" ayni gorunmemeli.
    content       TEXT    NOT NULL CHECK (length(content) > 0),
    created_at    TEXT    NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z'),
    metadata_json TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json))
) STRICT;

-- Tek erisim ekseni: "bu konusmanin mesajlari, eskiden yeniye".
-- `(session_id, id)` bilesik: `id` INTEGER PRIMARY KEY oldugu icin ekleme
-- sirasiyla artar, yani siralama da index'ten gelir (ayri bir `created_at`
-- index'i gerekmiyor — zaman damgasi saniye hassasiyetinde ve ayni saniyedeki
-- iki mesajin sirasini zaten `id` cozuyor).
CREATE INDEX idx_messages_session_id ON messages (session_id, id);

CREATE TABLE attachments (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    -- NULL = dosya eklendi ama henuz bir mesaja baglanmadi (composer'da bekliyor).
    message_id INTEGER REFERENCES messages (id) ON DELETE SET NULL,
    -- Kullanicinin gordugu ad. Tavan var: dosya adi bir etikettir, icerik degil.
    file_name  TEXT    NOT NULL CHECK (length(file_name) > 0 AND length(file_name) <= 255),
    -- NULL = tarayici tur soylemedi (`File.type === ''`). Uydurulmaz.
    mime_type  TEXT    CHECK (mime_type IS NULL OR (length(mime_type) > 0 AND length(mime_type) <= 128)),
    -- Kaynak dosyanin boyutu — saklanan (kirpilmis) metnin degil.
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    origin     TEXT    NOT NULL CHECK (origin IN ('upload', 'project')),
    -- REDAKTE EDILMIS metin (yukariya bak). Bos icerik gecerli: bos bir dosya
    -- gercekten eklenmis olabilir ve komut katmanina icerik uydurtmak yanlis olur.
    -- Tavan, komut katmanindaki 24.000 karakterlik kirpmanin IKINCI katmani:
    -- bir gun kirpma atlanirsa dosya DB'ye sessizce sizmaz, INSERT aninda duser.
    content    TEXT    NOT NULL CHECK (length(content) <= 32000),
    created_at TEXT    NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- `messages` ile ayni gerekce: erisim her zaman konusma uzerinden.
-- `message_id` icin AYRI index YOK: o eksende sorgu yapilmiyor (UI ekleri
-- bellekte grupluyor) ve kullanilmayan bir index her INSERT'i yavaslatir.
CREATE INDEX idx_attachments_session_id ON attachments (session_id, id);

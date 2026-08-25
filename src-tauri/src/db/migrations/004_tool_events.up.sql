-- ASU-050 — `tool_events` audit tablosu (PROJECT.md Bolum 12.2 + Bolum 19, Phase 5)
--
-- BU DOSYA YAYINLANMISTIR VE BIR DAHA DEGISTIRILMEZ. Duzeltme yeni bir
-- migration ekler (ADR-005 "Migration Karari").
--
-- ===========================================================================
-- Bu tablo ne icin var
-- ===========================================================================
--
-- PROJECT.md Bolum 19 ("Tool audit"): her tool cagrisi icin zaman, tool adi,
-- REDAKTE EDILMIS argumanlar, onay durumu ve sonuc ozeti saklanir. Ayni
-- bolumun kapanis cumlesi tasarimin olcutudur:
--
--   "The user should never wonder whether the agent is silently modifying
--    the computer."
--
-- Yani bu tablo bir performans/telemetri kaydi degil, kullanicinin **denetim
-- defteri**dir. Uc sonuc dogurur ve her uculu de asagida semaya yazilmistir:
--
--   1. Cagri onaylanmis, reddedilmis, hata vermis ya da zaman asimina ugramis
--      olsun — hepsi yazilir. "Yalnizca calisanlari kaydet" bir denetim defteri
--      degil, bir basari vitrinidir.
--   2. Kayit uygulamadan SILINEMEZ (ASU-050 kabul kriteri: "MVP'de salt
--      yazilir"). Semada silmeyi engelleyen bir sey yok — engelleme IPC
--      yuzeyindedir: `tool_events` icin yalnizca `record_tool_event` (yazma) ve
--      `tool_event_list` (okuma) komutlari var; silme/guncelleme komutu
--      **yoktur** ve bunu bir ACL testi kilitler.
--   3. Bu kolona ham arguman yazilmaz. `arguments_redacted` bir isim
--      sozlesmesidir; ozetleme ve redaksiyon Rust tarafinda
--      (`db::tool_event_repository::summarize_arguments` +
--      `redaction::redact_sensitive_text`) yapilir, renderer'in gonderdigi
--      metin oldugu gibi saklanmaz.
--
-- ===========================================================================
-- Neden `session_id` ... ON DELETE SET NULL (CASCADE degil)
-- ===========================================================================
--
-- `sessions` satirlari kullanici tarafindan silinebilir (ASU-065
-- `session_delete` / `session_clear_all`) — konusma gecmisini silmek acik bir
-- gizlilik hakkidir (PROJECT.md Bolum 20).
--
-- `ON DELETE CASCADE` yazsaydik "konusma gecmisini sil" dugmesi ayni zamanda
-- **audit defterini silen bir primitif** olurdu: uygulamadan audit kaydi
-- silmenin bir yolu bulunmaz derken, dolayli bir yolu acik birakmis olurduk.
-- Bu, ASU-050'nin "audit kayitlari uygulamadan silinemiyor" kriterini
-- kagit uzerinde birakirdi.
--
-- `ON DELETE SET NULL` ise iki seyi ayni anda korur:
--
--   * Audit satiri kalir — "Asuna o gun bilgisayarimda ne yapti?" sorusunun
--     cevabi, o gunun konusmasi silinse bile durur.
--   * Silinmis bir oturuma isaret eden **olu bir bag** kalmaz; `session_id`
--     NULL'a duser ve anlami net olur: "bu cagriyi ureten oturum kaydi artik
--     yok". `memories.source_session_id` ile birebir ayni gerekce ve ayni
--     davranis (migration 001).
--
-- Denge bilincli: audit satiri konusma **icerigi** tasimaz (tool adi, risk,
-- redakte edilmis arguman ozeti, sonuc ozeti). Yani oturumu silen kullanicinin
-- sildigi sey burada zaten yoktur.
--
-- ===========================================================================
-- `approval_state` kumesi
-- ===========================================================================
--
-- Alti deger; hepsi ASU-048 approval policy katmaninin gercekten uretebilecegi
-- ayri durumlar. "Onaylanmadi" tek bir kovaya konsaydi kullanici, KENDISININ
-- reddettigi bir cagri ile onay penceresi acilmadan dusen bir cagriyi ayirt
-- edemezdi.
--
--   not_required  : Bu risk seviyesi bu modda onay gerektirmiyordu (risk 0).
--   auto_approved : Onay gerekebilirdi ama `ASUNA_TOOL_APPROVAL_MODE` izin
--                   verdi (risk 1, `safe` disi mod). `not_required`'dan AYRI:
--                   "aslinda sorulabilirdi, ayarin izin verdi" demek, ayari
--                   sonradan sorgulanabilir kilar.
--   approved      : Kullanici acikca onayladi.
--   denied        : Kullanici acikca reddetti.
--   timeout       : Onay istegi zaman asimina ugradi -> varsayilan REDDET
--                   (ASU-048: "belirsizlik onay lehine cozulur").
--   not_requested : Onay asamasina hic gelinmedi — cagri daha once dustu
--                   (sema dogrulamasi, bilinmeyen tool adi, sandbox on-kontrolu).
--                   `not_required` ile karistirilmamali: orada onay GEREKMEDI,
--                   burada onay SORULAMADI.
--
-- Risk 2/3 icin `not_required` ya da `auto_approved` yazilmasi bir politika
-- ihlalidir; semada degil ASU-048'de zorlanir (mod bilgisi burada yok).
--
-- ===========================================================================
-- Uzunluk tavanlari neden CHECK olarak da yaziliyor
-- ===========================================================================
--
-- Rust tarafi zaten kirpiyor. CHECK'ler o kirpmanin **ikinci katmani**: ileride
-- bir degisiklik kirpmayi atlarsa, dosya icerigi ya da uzun bir stack trace
-- audit defterine sessizce sizmak yerine INSERT aninda duser. Bir denetim
-- kaydinin gizlice sisirilebilir olmasi, denetim degerini yok eder.

CREATE TABLE tool_events (
    id                 INTEGER PRIMARY KEY,
    -- NULL = cagriyi ureten oturum kaydi bilinmiyor ya da silinmis (yukariya
    -- bak). Uydurulmus bir korelasyon kimligi yazilmaz.
    session_id         INTEGER REFERENCES sessions (id) ON DELETE SET NULL
                               CHECK (session_id IS NULL OR session_id > 0),
    -- `snake_case`, fiil_nesne (`get_current_project`). Tavan bilincli: tool
    -- adi bir etikettir; icerik tasiyacak kadar uzun olamaz.
    tool_name          TEXT    NOT NULL CHECK (length(tool_name) > 0 AND length(tool_name) <= 64),
    -- PROJECT.md Bolum 5.4: 0 read-only, 1 geri alinabilir dusuk risk,
    -- 2 mutation, 3 destructive/harici etki. `BETWEEN` yerine `IN (...)`:
    -- boylece kume hem Rust (`ToolRiskLevel`) hem TypeScript (`TOOL_RISK_LEVELS`)
    -- tarafindan sema metninden okunup testle baglanabiliyor.
    risk_level         INTEGER NOT NULL CHECK (risk_level IN (0, 1, 2, 3)),
    -- ISIM SOZLESMESI: buraya ham arguman YAZILMAZ. Icerik, anahtar adlari +
    -- kirpilmis skaler degerlerden olusan tek satirlik bir ozettir; ic ice
    -- yapilar yalnizca SEKIL olarak gorunur (`{3 alan}` / `[2 oge]`), yani bir
    -- dosya icerigi ya da uzun bir metin buraya yapisal olarak giremez.
    -- NULL = cagri argumansizdi.
    arguments_redacted TEXT    CHECK (arguments_redacted IS NULL OR (length(arguments_redacted) > 0 AND length(arguments_redacted) <= 512)),
    approval_state     TEXT    NOT NULL CHECK (approval_state IN ('not_required', 'auto_approved', 'approved', 'denied', 'timeout', 'not_requested')),
    -- Kisa, insan diliyle sonuc (basari ya da hata; ikisi de kaydedilir).
    -- NULL = soylenecek bir sonuc yok — tipik olarak cagri hic calismadi.
    -- Bos bir ozet uydurmak yerine "sonuc yok" demek dogru cevaptir.
    result_summary     TEXT    CHECK (result_summary IS NULL OR (length(result_summary) > 0 AND length(result_summary) <= 512)),
    created_at         TEXT    NOT NULL CHECK (created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]*Z')
) STRICT;

-- Oturum detayi ekrani: "bu konusmada hangi tool'lar calisti?"
CREATE INDEX idx_tool_events_session_id ON tool_events (session_id);
-- Tools sekmesinin varsayilan gorunumu: en yeni cagrilar once.
CREATE INDEX idx_tool_events_created_at ON tool_events (created_at DESC);
-- "Bu tool bugune kadar kac kez calisti?" — tool basina denetim.
CREATE INDEX idx_tool_events_tool_name ON tool_events (tool_name);

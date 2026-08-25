-- ASU-050 geri alma.
--
-- KAPSAM: bu dosya **gelistirme araci**dir. Uygulama acilisinda yalnizca
-- `to_latest()` cagrilir; `down` otomatik olarak hicbir zaman kosmaz.
--
-- Ne kaybolur: `tool_events` tablosunun kendisi ve icindeki audit satirlari.
-- Bu, geri almanin **kabul edilen** bedelidir ve neden kabul edilebilir oldugu
-- onemli: 004 oncesinde tool audit'i hic yoktu, yani sema 3'e donmek "audit
-- kaydini silmek" degil "audit ozelligini kaldirmak"tir. Kullanicinin hafizasi
-- (`memories`), oturumlari (`sessions`) ve projeleri (`projects`) bu dosyadan
-- **hic etkilenmez** — 004 onlara dokunmadi, dolayisiyla geri alma da dokunmaz.
--
-- Yine de bu, uretimde kosturulacak bir dosya degildir: audit defterini silmek
-- kullanicinin urun icinden yapabilecegi bir sey degil (ASU-050 kabul kriteri)
-- ve gelistirici de bunu ancak acikca `to_version(3)` cagirarak yapabilir.
--
-- Index'ler tabloyla birlikte duser; `DROP INDEX` satirlari yine de acikca
-- yaziliyor ki dosya, neyin gittigi konusunda sessiz kalmasin.

DROP INDEX IF EXISTS idx_tool_events_tool_name;
DROP INDEX IF EXISTS idx_tool_events_created_at;
DROP INDEX IF EXISTS idx_tool_events_session_id;

-- `tool_events` bir EBEVEYN degil: hicbir tablo ona referans vermiyor.
-- Dolayisiyla dusurmek hicbir `ON DELETE` eylemini tetiklemez ve
-- `PRAGMA foreign_keys` acik da kapali da olsa ayni sonucu verir
-- (bkz. 003_projects.up.sql bas yorumu).
DROP TABLE IF EXISTS tool_events;

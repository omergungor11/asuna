-- Chat Shell geri alma (plan-chat-shell.md WP1).
--
-- KAPSAM: bu dosya **gelistirme araci**dir. Uygulama acilisinda yalnizca
-- `to_latest()` cagrilir; `down` otomatik olarak hicbir zaman kosmaz.
--
-- Ne kaybolur: metin sohbetinin TAMAMI — mesajlar, eklenen dosyalarin redakte
-- edilmis icerigi, konusma basliklari ve modalite bilgisi. Bu, geri almanin
-- **kabul edilen** bedelidir ve neden kabul edilebilir oldugu onemli: 006
-- oncesinde metin sohbeti hic yoktu, yani sema 5'e donmek "kullanicinin
-- konusmalarini silmek" degil "metin sohbeti ozelligini kaldirmak"tir.
--
-- Sema 5'te de var olan her sey yerinde kalir: `sessions` satirlarinin kendisi
-- (baslangic, proje, ozet, token/maliyet, dokum yolu), `memories`, `projects`
-- ve `tool_events`. 006 bu tablolarin hicbirini yeniden yaratmadi — yalnizca
-- `sessions`a iki kolon ekledi — dolayisiyla geri alma da onlara dokunmuyor.
--
-- Yine de bu, uretimde kosturulacak bir dosya degildir: kullanicinin gercek
-- konusmalari buradan gider ve urun icinde bunu yapan bir yol YOKTUR (konusma
-- silme yolu `session_delete`, tek tek ve kullanicinin talebiyle).
--
-- ===========================================================================
-- Sira neden boyle
-- ===========================================================================
--
-- Once `attachments`, sonra `messages`: `attachments.message_id` bir yabanci
-- anahtar. `messages` once dusurulseydi (FK zorlamasi acikken) ortuk bir DELETE
-- calisip `ON DELETE SET NULL` eylemini tetiklerdi — sonuc ayni olurdu ama
-- gereksiz is ve gereksiz bir belirsizlik. 001'in `memories` -> `sessions`
-- sirasi ile ayni kural: once referans VEREN.
--
-- Kolonlar en sonda dusuruluyor: `DROP COLUMN` SQLite 3.35+ ile gelir ve STRICT
-- tabloda da calisir (002'nin `end_reason`, 005'in `outcome` geri almasi ile
-- ayni desen). Iki kolon da hicbir index'te, view'de ya da baska bir tablonun
-- CHECK ifadesinde kullanilmiyor, yani dusurulmeleri engellenmez.
--
-- Index'ler tablolarla birlikte duser; `DROP INDEX` satirlari yine de acikca
-- yaziliyor ki dosya, neyin gittigi konusunda sessiz kalmasin.

DROP INDEX IF EXISTS idx_attachments_session_id;
DROP TABLE IF EXISTS attachments;

DROP INDEX IF EXISTS idx_messages_session_id;
DROP TABLE IF EXISTS messages;

ALTER TABLE sessions DROP COLUMN modality;
ALTER TABLE sessions DROP COLUMN title;

-- ASU-051 geri alma.
--
-- KAPSAM: bu dosya **gelistirme araci**dir. Uygulama acilisinda yalnizca
-- `to_latest()` cagrilir; `down` otomatik olarak hicbir zaman kosmaz.
--
-- Ne kaybolur: yalnizca `outcome` kolonu — yani "cagri calisti mi, basardi mi?"
-- ekseni. Audit satirlarinin kendisi, onay durumlari, arguman ozetleri ve
-- sonuc ozetleri **oldugu gibi kalir**; sema 4'e donmek audit defterini
-- silmez, defterin bir sutununu kaldirir.
--
-- Geri doldurma YOK ve olamaz: `result_summary` metninden `outcome` cikarmak
-- (ya da tersi) olculmemis bir iddia uretirdi. 002'nin aksine burada bilgi
-- kaydin baska bir yerinde durmuyor.
--
-- `DROP COLUMN` SQLite 3.35+ ile gelir ve STRICT tabloda da calisir; kolon
-- hicbir index'te, view'de ya da baska bir CHECK ifadesinde kullanilmadigi
-- icin dusurulmesi engellenmez (002'nin `end_reason` geri almasi ile ayni).

ALTER TABLE tool_events DROP COLUMN outcome;

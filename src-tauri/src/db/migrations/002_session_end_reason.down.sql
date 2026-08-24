-- ASU-033 geri alma.
--
-- Kolon dusurulur. Kaybedilen bilgi geri **uydurulmaz**: `abandoned` olarak
-- isaretlenmis oturumlarin `summary` alanina eski bayrak cumlesi yeniden
-- yazilir ki 001 semasindaki davranis (yarim oturumun nedeni insan diliyle
-- gorunur) korunsun.
UPDATE sessions
   SET summary = 'Oturum beklenmedik sekilde kapandi (uygulama yeniden acilirken kapatildi).'
 WHERE end_reason = 'abandoned'
   AND summary IS NULL;

ALTER TABLE sessions DROP COLUMN end_reason;

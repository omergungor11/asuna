-- ASU-033 — `sessions.end_reason` (oturum nasil kapandi?)
--
-- BU DOSYA YAYINLANMISTIR VE BIR DAHA DEGISTIRILMEZ. Duzeltme yeni bir
-- migration ekler (ADR-005 "Migration Karari").
--
-- Neden bu kolon:
--   ASU-032'de yarim kalan oturumlar `summary` alanina insan diliyle yazilan
--   bir cumleyle isaretleniyordu. `summary` bir **durum bayragi** degil, oturum
--   ozetidir (ASU-033) ve ASU-034'un memory extraction girdisidir; bayrak orada
--   kalsaydi ya gercek ozeti ezerdi ya da "Oturum beklenmedik sekilde kapandi"
--   cumlesinden hafiza cikarilirdi. Durum artik ayri, makine-okunur bir kolonda.
--
-- Deger kumesi CHECK ile zorlanir ve **tek kaynaktir**: Rust `SessionEndReason`
-- ve TypeScript `SESSION_END_REASONS` bu satira testlerle baglidir.
--   completed : oturum `session_finalize` ile temiz kapandi
--   abandoned : cokme/kill sonrasi acilista kurtarildi (gercek bitis bilinmiyor)
--   error     : oturum bir hata ile sonlandi (renderer bildirir)
--
-- NULL = bilinmiyor. Hala acik oturumlarda (ended_at IS NULL) beklenen deger budur.

ALTER TABLE sessions ADD COLUMN end_reason TEXT
    CHECK (end_reason IS NULL OR end_reason IN ('completed', 'abandoned', 'error'));

-- Geriye donuk doldurma. Eski kayitlarda durum **cikarilabiliyor**, tahmin
-- edilmiyor: kurtarma yolu `ended_at = started_at` + asagidaki sabit cumleyi
-- yaziyordu; digerlerinin hepsi `session_finalize`'dan gecmisti.
--
-- Asagidaki metin `session_repository::ABANDONED_SESSION_SUMMARY` ile birebir
-- ayni olmali; bir test bunu dogruluyor.
UPDATE sessions
   SET end_reason = 'abandoned'
 WHERE ended_at IS NOT NULL
   AND summary = 'Oturum beklenmedik sekilde kapandi (uygulama yeniden acilirken kapatildi).';

-- Bayrak `summary`'den temizlenir: alan bundan sonra yalnizca gercek oturum
-- ozetini tasir. Kullanici verisi silinmis olmuyor — bu cumleyi kullanici degil
-- Asuna yazmisti ve karsiligi artik `end_reason` kolonunda duruyor.
UPDATE sessions
   SET summary = NULL
 WHERE end_reason = 'abandoned';

UPDATE sessions
   SET end_reason = 'completed'
 WHERE ended_at IS NOT NULL
   AND end_reason IS NULL;

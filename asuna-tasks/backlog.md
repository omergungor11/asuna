# Backlog (Icebox)

> Henuz phase'e planlanmamis fikirler. `/plan-feature` ile buradan cekilip task'lastirilir.
> Oncelik: yukari = once.
>
> **Buraya bir sey eklemenin anlami:** MVP'de yapilmayacak. PROJECT.md Bolum 4'teki non-goals listesi
> ve Bolum 36-37'deki uzun vadeli yon bu dosyanin kaynagidir. Bir fikir "iyi fikir" oldugu icin
> phase'e girmez; PROJECT.md Bolum 39/17'deki soruyu gecmesi gerekir:
> *"Bu, Asuna'ya ulasmayi kolaylastiriyor mu, baglam farkindaligini artiriyor mu, isi bitirmeyi
> daha faydali hale getiriyor mu?"*

## Candidates (MVP'den hemen sonra degerlendirilecek)

- [x] **ASU-065 — oturum ozeti + dokum temizligi** (Gate 3 / MEDIUM-6, 2026-08-25) →
      **Phase 3'e cekildi ve tamamlandi (2026-08-25)**. Backlog'da kalamadi: M3 kabul testi
      bunu blokere cevirdi — kullanici hafiza kayitlarini sildi ama Asuna hatirlamaya devam
      etti, cunku Stage A son oturum ozetini enjekte ediyor ve `sessions.summary` urun icinden
      silinemiyordu. Cozum tasarlandigi gibi **ayri ve gorunur** bir aksiyon oldu (kapsam
      genisletme degil): `Hafiza > Oturumlar` ve `Ayarlar > Konusma gecmisini sil`.
      Detay: `asuna-tasks/phases/phase-3.md` → ASU-065.
- [ ] **Hafiza listesinde sunucu tarafi sayfalama** (Gate 3 / MEDIUM-5) — `memory_list` en fazla 200
      kayit donuyor; UI su an tavana carptigini yalnizca metinle soyluyor. `hasMore`/`total`
      alanlari ve offset destegi backend isi.
- [ ] **Onay istegi icin ayri overlay penceresi** (ASU-053 acik kriteri, 2026-08-31) — kart su an
      `document.body`'ye portal ediliyor, yani sekme degisse de gorunur; ama `tauri.conf.json` tek
      pencere (`main`) tanimladigi icin **ana pencere kapaliyken** onay istegi hic gorunmuyor.
      Wake word ile arka planda tetiklenen bir tool'da bu, kullanicinin farkina varmadigi bir
      60 sn'lik zaman asimi demek. Overlay/tray penceresiyle birlikte degerlendirilmeli (Phase 2
      ASU-027 ve Phase 6 overlay isleriyle ayni yuzey).
- [ ] **`TRANSCRIPTION_MODEL` config'e tasinmali** (Gate 3 / LOW-3, pre-existing) —
      `realtime-service.ts` icinde `gpt-4o-mini-transcribe` hard-code. CLAUDE.md "model ID'leri
      asla hard-code edilmez" kuralinin acik ihlali; `ASUNA_REALTIME_MODEL` deseniyle ayni
      sekilde `.env` anahtarina cikarilmali.
- [ ] **Stage B — semantic retrieval / embeddings** — Yeterli hafiza birikince Stage A deterministik
      retrieval yetmez (PROJECT.md Bolum 13). SQLite vektor eklentisi vs kucuk yerel vektor DB karari
      ADR gerektirir. Tetikleyici: ~200+ hafiza kaydi veya "hatirlamiyor" sikayeti.
- [ ] **Stage C — memory consolidation** — Ayni tercihin uc farkli ifadesini tek kalici hafizaya
      birlestirme (PROJECT.md Bolum 13). Duplikasyon gozle gorulur hale gelince.
- [ ] **Proaktiflik tetikleyicileri** — PROJECT.md Bolum 27 + TRANSCRIPT.md Bolum 10.
      "Ayni test 25 dakikada 4 kez dustu" -> nazik mudahale. Once Asuna'nin *kendi* tool'larinin
      urettigi aktivite event'leriyle basla; genel sistem izlemeyle degil.
      Kural: baglamsal, nadir, uygulanabilir, reddedilebilir olmali.
- [ ] **Ek scoped shell tool'lari** — `run_tests`, `run_lint`, `git_status`, `git_diff`,
      `start_dev_server` (PROJECT.md Bolum 18). Her biri: arguman dogrulama, calisma dizini kisiti,
      timeout, stdout/stderr yakalama, risk sinifi, audit, onay.
- [ ] **`create_project_note` tool (risk 1)** — Sadece `.asuna/notes/` altina yazan not olusturma
      (PROJECT.md Bolum 17).
- [ ] **`get_git_status` / `list_recent_project_activity` tool'lari (risk 0)** — Phase 4'te veri
      katmani hazir olacak, tool olarak acilmasi ayri is.
- [ ] **Konusma gecmisi UI'si** — Oturum listesi, gecmis transcript'lerde arama (PROJECT.md Bolum 21).
- [ ] **Global kisayol ile aktivasyon** — Wake word'e alternatif; gurultulu ortam veya toplanti icin.
- [ ] **Maliyet paneli** — Gunluk/haftalik tahmini harcama, oturum basina maliyet, butce uyarisi
      (PROJECT.md Bolum 28). MVP'de sadece ham metadata toplaniyor.
- [ ] **Ses secimi + konusma hizi ayari** — `ASUNA_REALTIME_VOICE` konfigu var ama UI'si yok.
- [ ] **Ikincil wake word varyantlari** — "Asuna", "Asuna nasilsin?", "Asuna beni toparla"
      (PROJECT.md Bolum 8). MVP tek trigger ile yanlis pozitifi dusuk tutuyor.

## Someday / Maybe (uzun vadeli yon)

- [ ] **Sifreli veritabani (SQLCipher) + OS keychain** — PROJECT.md Bolum 20 "Later". Kalici
      hafiza gercekten kisisel veri tutmaya baslayinca zorunlu hale gelir.
- [ ] **Retention politikalari + hafiza basina gizlilik siniflari** — Otomatik unutma, hassas
      kategorilerin ayri muamelesi.
- [ ] **Tray overlay gelistirmeleri** — Her zaman ustte mini pencere, ses dalgasi gorsellestirmesi,
      surukle-birak konumlandirma, coklu monitor davranisi.
- [ ] **Browser agent** — PROJECT.md Bolum 4 non-goal, Bolum 36 uzun vade.
- [ ] **Email agent / Calendar agent** — Ayni sekilde non-goal; Asuna Core'a *baglanacak* servisler
      olarak tasarlanmali, cekirdegin yerine gecmemeli.
- [ ] **Coding agent entegrasyonu** — Codex/Claude Code oturumlariyla baglam paylasimi;
      "Codex'in yaptigi degisiklikleri ozetle" (PROJECT.md Bolum 2 ornek sorulari).
- [ ] **Research agent / personal knowledge agent** — PROJECT.md Bolum 36.
- [ ] **Task planner** — Otomatik gorev planlama. Dikkat: PROJECT.md Bolum 4 "fully automatic task
      planning" acikca non-goal; TRANSCRIPT.md Bolum 19 "another giant todo application" reddediyor.
- [ ] **Kontrollu shell tool (allowlist + onay kapisi)** — PROJECT.md Bolum 18 sonu. Ancak scoped
      tool'lar olgunlastiktan ve audit katmani guvenilir hale geldikten sonra.
- [ ] **Yerel/offline fallback modu** — Ag yokken metin-only, cache'lenmis proje baglami, wake word
      calismaya devam eder (PROJECT.md Bolum 30).
- [ ] **Ekran baglami** — Non-goal (screen recording). Eger gelirse: acik, gorunur, tek seferlik ve
      kullanici tetikli olmali.
- [ ] **Mobil uygulama** — PROJECT.md Bolum 4 non-goal.
- [ ] **Enterprise yonu** — PROJECT.md Bolum 37: ozel deployment, kurum bilgisi, rol tabanli tool
      izinleri, audit loglari, dahili MCP/tool ekosistemi, sirket bazli hafiza sinirlari, uyumluluk
      kontrolleri, ekip devir teslim baglami.
      **Uyari:** "Do not optimize the MVP for enterprise yet. First prove one person genuinely wants
      Asuna running every day."
- [ ] **Coklu kullanici / tenancy** — Yukaridakinin on kosulu, ayni uyari gecerli.
- [ ] **Alternatif model saglayicilari** — Model/provider sinirlari zaten interface arkasinda
      (PROJECT.md Bolum 39/13); farkli bir realtime saglayicisi denemek ucuz olmali.
- [ ] **Windows / Linux destegi** — MVP macOS-only. Wake word ve entitlement katmani platforma bagli.

- [ ] **`create_project_scaffold` tool'u (risk 2)** — sesle diskte yeni proje olusturma (dizin +
      iskelet dosyalar). Mutation: her cagrida onay + olusturma konumunun sinirlanmasi + Gate 3
      review sart. Kullanici istegi (2026-08-31, canli test).
- [ ] **Dis mesaj/entegrasyon tool'u (risk 3, tasarim gerekli)** — "Warp'a / baska bir uygulamaya
      mesaj gonder" turu harici etki. Hedef, format ve onay modeli tasarlanmadan yazilmaz; kisitsiz
      "her uygulamaya yaz" yok (bkz. Rejected: sinirsiz shell). Kullanici istegi (2026-08-31).
- [ ] **Ekran/uygulama duzeyi farkindalik ("Jarvis" katmani)** — kullanicinin o an ne yaptigini
      (aktif uygulama, pencere, belki ekran icerigi) bilme. Buyuk gizlilik/tasarim karari; proje
      farkindaligi (Wave D toollari) bunun guvenli ilk dilimi. (2026-08-31)

## Rejected (nedeniyle)

<!-- - [Fikir] — [neden reddedildi, tarih] -->
- Sinirsiz `run_any_shell_command` tool'u — PROJECT.md Bolum 18 acikca yasakliyor; guvenlik modelinin
  temelini yikar. Yerine scoped tool'lar. (2026-08-24)
- Tum konusmayi kalici hafiza olarak saklamak — PROJECT.md Bolum 5.3 / CLAUDE.md; hafiza
  siniflandirilmis olmali, arsiv degil. (2026-08-24)
- MVP'de tam filesystem indexleme — PROJECT.md Bolum 4 non-goal; sadece kayitli proje root'lari
  (ASU-040). (2026-08-24)
- Surekli bulut mikrofon akisi — PROJECT.md Bolum 4 + Bolum 20; urunun gizlilik sozunu bozar.
  (2026-08-24)

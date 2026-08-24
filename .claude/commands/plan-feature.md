Yeni bir ozellik planla ve task'lara bol. Asamalar sirasiyla — hicbirini atlama:

## Asama 1: Anla
1. Kullanicidan ozelligin tanimini al (verilmediyse sor)
2. Belirsizlikleri AskUserQuestion ile TEK SEFERDE netleştir:
   kapsam siniri (ne DAHIL degil), oncelik, mevcut sistemle temas noktalari
3. Ilgili mevcut kodu/dokumani oku — plan gercek duruma dayansin, varsayima degil

## Asama 2: Plan Yaz
1. `asuna-plans/plan-template.md` formatini kullan
2. `asuna-plans/<ozellik-adi>.md` olustur: hedef, kapsam disi, teknik yaklasim,
   etkilenen moduller, riskler, acik sorular
3. Mimari karar iceriyorsa → karari `asuna-docs/DECISIONS.md`'ye de yaz
4. Plani kullaniciya OZETLE ve onay al — onaysiz Asama 3'e gecme

## Asama 3: Task'lara Bol
1. Her task: tek agent'a atanabilir, tek oturumda bitirilebilir (L'den buyukse bol)
2. Her task icin: ID (siradaki ASU-XXX), agent, complexity (S/M/L), dependencies
3. Bagimliliklari acikca kur — paralel calisabilecekleri isaretle

## Asama 4: Isle
1. Uygun phase'i sec (yoksa yeni phase ekle) → `asuna-tasks/phases/phase-X.md`'ye
   task detaylarini acceptance criteria ile yaz (templates/task-template.md formati)
2. `asuna-tasks/task-index.md`'ye task satirlarini ekle, dashboard sayilarini guncelle
3. Ozet ver: kac task, hangi sirayla, ilk baslanacak task hangisi

NOT: Bu command KOD YAZMAZ — sadece plan ve task uretir.

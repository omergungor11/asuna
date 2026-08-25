/**
 * "Proje kaydi degisti" bildirimi (ASU-045).
 *
 * # Neden var
 *
 * Guncel proje **iki yerde** gorunmek zorunda: Projeler sekmesinde ve ses
 * panelinde (PROJECT.md Bolum 19 — "mevcut proje" her an gorunur). Ses paneli
 * hicbir zaman unmount edilmedigi icin (bkz. `app.tsx`), Projeler sekmesinde
 * yapilan bir secimi kendiliginden ogrenmesinin bir yolu olmali.
 *
 * Bu yol bilerek **en kucuk** olani: tek bir "bir sey degisti" sinyali. State
 * kutuphanesi, global store ya da context saglayici yok (R7 / CLAUDE.md "erken
 * sislenme yok"). Sinyal veri **tasimaz** — dinleyen taraf gercegi yine servis
 * katmanindan okur, boylece ekranda gosterilen sey her zaman backend'in kabul
 * ettigi durumdur, UI'nin tahmini degil.
 */

type ProjectsChangedListener = () => void;

const listeners = new Set<ProjectsChangedListener>();

/**
 * Degisiklikleri dinler.
 *
 * @returns Aboneligi biten temizlik fonksiyonu (`useEffect` destructor'i).
 */
export function subscribeProjectsChanged(listener: ProjectsChangedListener): () => void {
  listeners.add(listener);
  return (): void => {
    listeners.delete(listener);
  };
}

/**
 * Kayit degistiginde (ekleme / kaldirma / guncel proje secimi) cagrilir.
 *
 * Dinleyici listesi kopyalanarak gezilir: bir dinleyici tepki olarak abonelikten
 * cikarsa iterasyon bozulmaz.
 */
export function notifyProjectsChanged(): void {
  for (const listener of [...listeners]) {
    listener();
  }
}

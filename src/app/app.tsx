import { Suspense, lazy } from 'react';

/**
 * Debug log paneli (ASU-019) yalnizca gelistirmede yuklenir.
 *
 * `import.meta.env.DEV` uretim build'inde `false` sabitine indirgenir; ternary
 * olu koda duser ve dinamik `import()` bundle'a hic girmez.
 */
const DebugPanel = import.meta.env.DEV
  ? lazy(async () => {
      const module = await import('../components/debug-panel');
      return { default: module.DebugPanel };
    })
  : null;

/**
 * Phase 0 iskeleti: bos pencere.
 *
 * Bu bilesende bilerek hicbir Asuna ozelligi yok — ne ses, ne agent, ne IPC.
 * Amac yalnizca Tauri kabugunun ayakta oldugunu gostermek (ASU-002).
 */
export function App(): React.JSX.Element {
  return (
    <main className="asuna-shell">
      <h1 className="asuna-shell__title">Asuna</h1>
      {DebugPanel !== null && (
        <Suspense fallback={null}>
          <DebugPanel />
        </Suspense>
      )}
    </main>
  );
}

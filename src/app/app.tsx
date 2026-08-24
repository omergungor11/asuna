import { Suspense, lazy } from 'react';

import { VoicePanel } from '../components/voice-panel';

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
 * Uygulama kabugu.
 *
 * Kabuk bilerek ince: hicbir servis cagrisi burada yok. Ses oturumunun tum
 * gorunur yuzeyi `VoicePanel` icinde (ASU-015), log paneli yalnizca dev'de.
 */
export function App(): React.JSX.Element {
  return (
    <main className="asuna-shell">
      <h1 className="asuna-shell__title">Asuna</h1>
      <VoicePanel />
      {DebugPanel !== null && (
        <Suspense fallback={null}>
          <DebugPanel />
        </Suspense>
      )}
    </main>
  );
}

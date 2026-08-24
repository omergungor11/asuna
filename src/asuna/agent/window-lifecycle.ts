/**
 * Pencere kapanisinda oturumu kapatma kancasi (ASU-018).
 *
 * Neden onemli: acik kalan bir Realtime oturumu **fatura yazar** (task-index.md R1) ve
 * mikrofon gostergesi acik kalir. Kullanici pencereyi kapattiginda oturumun da kapanmasi
 * gerekir; "uygulama zaten oluyor" varsayimi Tauri'de dogru degil — pencere kapanisi
 * process'in bittigi anlamina gelmez.
 *
 * Iki kanal birlikte kullanilir:
 * - `beforeunload`: webview yeniden yuklenirken / sayfa boslatilirken.
 * - Tauri `onCloseRequested`: pencere kapatma dugmesi. Yalnizca Tauri calisma zamani
 *   varsa baglanir; tarayicida (test, `pnpm dev`) modul hic yuklenmez.
 */

import { logger } from '../observability';

const log = logger.child('window-lifecycle');

interface CloseHandlerState {
  unlisten: (() => void) | null;
  detached: boolean;
}

/** Tauri IPC koprusu enjekte edilmis mi (tarayicida yok). */
function hasTauriRuntime(): boolean {
  return '__TAURI_INTERNALS__' in globalThis;
}

async function attachTauriCloseHandler(
  handler: () => void,
  state: CloseHandlerState,
): Promise<void> {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const unlisten = await getCurrentWindow().onCloseRequested(() => {
      handler();
    });

    if (state.detached) {
      unlisten();
      return;
    }
    state.unlisten = unlisten;
  } catch (error) {
    // Sessiz yutma yok: bu basarisizsa kapanista oturum yalnizca `beforeunload`
    // ile kapanir ve bu bilinmesi gereken bir eksiklik.
    log.warn(
      'Pencere kapanis dinleyicisi kurulamadi; oturum yalnizca beforeunload ile kapanir.',
      {
        detail: error instanceof Error ? error.message : String(error),
      },
    );
  }
}

/**
 * Kapanis kancasini kurar.
 *
 * @returns kancayi soken fonksiyon (bilesen unmount olurken cagrilir).
 */
export function registerWindowCloseHandler(handler: () => void): () => void {
  const state: CloseHandlerState = { unlisten: null, detached: false };

  const onBeforeUnload = (): void => {
    handler();
  };
  window.addEventListener('beforeunload', onBeforeUnload);

  if (hasTauriRuntime()) {
    void attachTauriCloseHandler(handler, state);
  }

  return (): void => {
    state.detached = true;
    window.removeEventListener('beforeunload', onBeforeUnload);
    state.unlisten?.();
    state.unlisten = null;
  };
}

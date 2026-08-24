import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri CLI, mobil/uzak cihaz gelistirmede bu degiskeni set eder.
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],

  // Rust derleyici hatalari Vite tarafindan ekrandan silinmesin.
  clearScreen: false,

  server: {
    // Tauri `devUrl` ile sabit port bekler; port dolu ise sessizce kaymak yerine hata versin.
    port: 1420,
    strictPort: true,
    host: host ?? false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: {
      // src-tauri degisikliklerini Rust tarafi zaten izliyor; Vite'in izlemesi gereksiz reload uretir.
      ignored: ['**/src-tauri/**'],
    },
  },

  build: {
    // Uretim build'inde kaynak haritasi kapali: renderer bundle'i tersine muhendislik
    // yuzeyini gereksiz genisletmesin (PROJECT.md Bolum 19).
    sourcemap: false,
    // Tauri macOS/WKWebView hedefi — minimum macOS 13 (bkz. tauri.conf.json bundle.macOS).
    target: 'safari16',
  },

  // GUVENLIK: burada `define` ile hicbir secret gomulmez. `OPENAI_API_KEY` ve
  // `PICOVOICE_ACCESS_KEY` yalnizca Tauri Rust tarafinda okunur; renderer bundle'ina
  // hicbir kosulda girmez (PROJECT.md Bolum 19, ASU-009).
  envPrefix: ['VITE_'],
});

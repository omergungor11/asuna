import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// Tauri CLI, mobil/uzak cihaz gelistirmede bu degiskeni set eder.
const host = process.env['TAURI_DEV_HOST'];

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
    // `exactOptionalPropertyTypes` acik: `hmr: undefined` yazmak yerine kosullu spread.
    ...(host === undefined ? {} : { hmr: { protocol: 'ws' as const, host, port: 1421 } }),
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

  // GUVENLIK: burada `define` ile hicbir secret gomulmez. `OPENAI_API_KEY`
  // yalnizca Tauri Rust tarafinda okunur; renderer bundle'ina
  // hicbir kosulda girmez (PROJECT.md Bolum 19, ASU-009).
  envPrefix: ['VITE_'],

  test: {
    // `globals: false` (varsayilan) — describe/it/expect acikca import edilir.
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
    include: ['src/**/*.spec.{ts,tsx}'],
    css: true,
    coverage: {
      provider: 'v8',
      reportsDirectory: './coverage',
      include: ['src/**/*.{ts,tsx}'],
      exclude: ['src/**/*.spec.{ts,tsx}', 'src/test-setup.ts', 'src/vite-env.d.ts'],
    },
  },
});

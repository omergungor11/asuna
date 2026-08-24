import js from '@eslint/js';
import prettierConfig from 'eslint-config-prettier';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';
import tseslint from 'typescript-eslint';

/**
 * Asuna ESLint flat config.
 *
 * Katman sirasi onemli: `prettierConfig` EN SONDA durur ve bicimlendirmeyle
 * ilgili tum kurallari kapatir — boylece ESLint ile Prettier catismaz
 * (ASU-003 kabul kriteri). Bicimlendirme tek otorite: Prettier.
 */
export default tseslint.config(
  {
    // Turetilmis / kaynak olmayan her sey lint disi.
    ignores: [
      'dist/**',
      'coverage/**',
      'node_modules/**',
      'src-tauri/target/**',
      'src-tauri/gen/**',
    ],
  },

  // --- Uygulama kaynagi: tip-bilincli (type-aware) lint ---
  {
    files: ['src/**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      ...tseslint.configs.strictTypeChecked,
      ...tseslint.configs.stylisticTypeChecked,
      // v7'de flat config surumu `configs.flat` altinda; ust seviyedeki ayni
      // isimli girdi hala eski eslintrc formatinda (plugins: string[]).
      reactHooks.configs.flat['recommended-latest'],
    ],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    plugins: {
      'react-refresh': reactRefresh,
    },
    rules: {
      'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],

      // conventions.md: `any` yasak, `@ts-ignore` yasak, sessiz hata yutma yok.
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unsafe-assignment': 'error',
      '@typescript-eslint/no-unsafe-member-access': 'error',
      '@typescript-eslint/no-unsafe-call': 'error',
      '@typescript-eslint/no-unsafe-return': 'error',
      '@typescript-eslint/no-unsafe-argument': 'error',
      '@typescript-eslint/ban-ts-comment': [
        'error',
        { 'ts-ignore': true, 'ts-expect-error': 'allow-with-description' },
      ],
      '@typescript-eslint/explicit-module-boundary-types': 'error',
      '@typescript-eslint/consistent-type-imports': [
        'error',
        { prefer: 'type-imports', fixStyle: 'inline-type-imports' },
      ],
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/switch-exhaustiveness-check': 'error',
      'no-empty': ['error', { allowEmptyCatch: false }],

      // Guvenlik siniri: renderer'da secret/eval yok (PROJECT.md Bolum 19).
      'no-eval': 'error',
      'no-implied-eval': 'error',
      'no-restricted-globals': [
        'error',
        {
          name: 'process',
          message: 'Renderer tarafinda process/env okunmaz — config servisini kullan.',
        },
      ],
    },
  },

  // --- Test dosyalari: ayni kurallar, birkac pratik gevsetme ---
  {
    files: ['src/**/*.spec.{ts,tsx}', 'src/test-setup.ts'],
    rules: {
      // Test kurgusunda non-null assertion okunabilirligi artirir.
      '@typescript-eslint/no-non-null-assertion': 'off',
    },
  },

  // --- Node tarafi build/tooling dosyalari ---
  {
    files: ['*.config.{js,ts}', 'eslint.config.js'],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2023,
      globals: globals.node,
    },
  },

  // Bicimlendirme kurallarini kapat — EN SONDA kalmali.
  prettierConfig,
);

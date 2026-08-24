/**
 * Mimari sinir testi (ASU-013 kabul kriteri).
 *
 * "OpenAI Agents SDK yalnizca `AsunaRealtimeService` icinde gorunur;
 * `RealtimeAgent`/`RealtimeSession` tipleri disari sizmaz." (`conventions.md`)
 *
 * Bu kural bir yorum satiri olarak kalirsa ilk aceleci commit'te delinir. Burada
 * `src/` agaci taranip SDK import'unun **tek** izinli dosyada oldugu dogrulanir.
 *
 * NOT: tarama modul *adlarini* (`from '...'` / `import '...'` / `require('...')`) arar,
 * dosyada gecen duz string'leri degil — bu yuzden testin kendisi kendini yakalamaz.
 */

import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { cwd } from 'node:process';

import { describe, expect, it } from 'vitest';

/**
 * `import.meta.url` jsdom ortaminda `http://` semasi doner, `import.meta.dirname` de
 * tanimli degil — kok dizin Vitest'in calisma dizininden alinir (repo koku).
 * `node:process` modulu bilincli olarak import ediliyor: renderer'da yasak olan sey
 * `process` **global**'idir (`no-restricted-globals`), bu bir test aracidir.
 */
const SOURCE_ROOT = resolve(cwd(), 'src');

/** SDK import'una izin verilen tek dosya. */
const ALLOWED_SDK_IMPORT_FILES: readonly string[] = ['src/asuna/agent/realtime-service.ts'];

const SDK_SCOPE = '@openai/';

/**
 * SDK'nin browser ortaminda kalici API anahtarini kabul etmesini saglayan kacis kapisi
 * (voice.md Bolum 4). Kod tabaninda **hicbir yerde** gecmemeli.
 */
const FORBIDDEN_ESCAPE_HATCH = 'useInsecureApiKey';

/** Yasakli metni zorunlu olarak iceren tek dosya: bu test. */
const ESCAPE_HATCH_SCANNER = 'src/asuna/agent/sdk-import-boundary.spec.ts';

function listSourceFiles(directory: string): string[] {
  const files: string[] = [];

  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...listSourceFiles(path));
    } else if (entry.name.endsWith('.ts') || entry.name.endsWith('.tsx')) {
      files.push(path);
    }
  }

  return files;
}

/** `from '...'`, `import '...'`, `import('...')` bicimlerindeki modul adlarini toplar. */
function moduleSpecifiers(source: string): string[] {
  const pattern = /(?:\bfrom|\bimport|\brequire)\s*\(?\s*['"]([^'"]+)['"]/g;
  const specifiers: string[] = [];

  for (const match of source.matchAll(pattern)) {
    const specifier = match[1];
    if (specifier !== undefined) {
      specifiers.push(specifier);
    }
  }

  return specifiers;
}

function toRepoPath(absolutePath: string): string {
  return `src/${relative(SOURCE_ROOT, absolutePath).split(sep).join('/')}`;
}

describe('SDK import siniri', () => {
  const sourceFiles = listSourceFiles(SOURCE_ROOT);

  it('tarama gercekten dosya buluyor (test bos kumede yesil yanmasin)', () => {
    expect(sourceFiles.length).toBeGreaterThan(5);
  });

  it('OpenAI Agents SDK yalnizca izinli dosyada import ediliyor', () => {
    const offenders = sourceFiles
      .filter((file) =>
        moduleSpecifiers(readFileSync(file, 'utf8')).some((specifier) =>
          specifier.startsWith(SDK_SCOPE),
        ),
      )
      .map(toRepoPath)
      .filter((path) => !ALLOWED_SDK_IMPORT_FILES.includes(path));

    expect(offenders).toEqual([]);
  });

  it('izinli dosya gercekten SDK kullaniyor (allowlist olu kalmasin)', () => {
    const [allowed] = ALLOWED_SDK_IMPORT_FILES;
    expect(allowed).toBeDefined();

    const source = readFileSync(resolve(SOURCE_ROOT, '..', allowed ?? ''), 'utf8');
    expect(moduleSpecifiers(source).some((specifier) => specifier.startsWith(SDK_SCOPE))).toBe(
      true,
    );
  });

  it('`useInsecureApiKey` kacis kapisi hicbir yerde kullanilmiyor', () => {
    const offenders = sourceFiles
      .filter((file) => readFileSync(file, 'utf8').includes(FORBIDDEN_ESCAPE_HATCH))
      .map(toRepoPath)
      .filter((path) => path !== ESCAPE_HATCH_SCANNER);

    expect(offenders).toEqual([]);
  });
});

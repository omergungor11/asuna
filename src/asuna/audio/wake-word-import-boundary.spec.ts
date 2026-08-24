/**
 * Mimari sinir testi (ASU-021 kabul kriteri: "Uygulamanin geri kalani somut
 * provider tipini import etmiyor").
 *
 * `conventions.md`: "motor Rust tarafinda calisir, renderer yalnizca
 * `WakeWordProvider` tipini gorur, vendor adini gormez."
 *
 * Bu kural yorum satiri olarak birakilirsa ilk aceleci commit'te delinir; ADR-004'un
 * exit plani (motor degistirme) ancak somut tipler tek dosyada kapaliysa ucuz kalir.
 * Desen `agent/sdk-import-boundary.spec.ts` ile ayni.
 */

import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { cwd } from 'node:process';

import { describe, expect, it } from 'vitest';

const SOURCE_ROOT = resolve(cwd(), 'src');

/** Somut saglayici modulleri — yalnizca fabrika (ve testleri) import edebilir. */
const CONCRETE_PROVIDER_MODULES = ['fake-wake-word-provider', 'sherpa-kws-provider'] as const;

const ALLOWED_IMPORTERS: readonly string[] = ['src/asuna/audio/wake-word-provider-factory.ts'];

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

function importsConcreteProvider(file: string): boolean {
  return moduleSpecifiers(readFileSync(file, 'utf8')).some((specifier) =>
    // `./fake-wake-word-provider` de `../audio/fake-wake-word-provider` de yakalanir.
    CONCRETE_PROVIDER_MODULES.some((moduleName) => specifier.endsWith(`/${moduleName}`)),
  );
}

describe('Wake word saglayici import siniri', () => {
  const sourceFiles = listSourceFiles(SOURCE_ROOT);

  it('tarama gercekten dosya buluyor (test bos kumede yesil yanmasin)', () => {
    expect(sourceFiles.length).toBeGreaterThan(5);
  });

  it('somut saglayicilari yalnizca fabrika import ediyor', () => {
    const offenders = sourceFiles
      .filter(importsConcreteProvider)
      .map(toRepoPath)
      // Testler somut tipi gormek zorunda: `instanceof` ile secim dogrulaniyor.
      .filter((path) => !path.endsWith('.spec.ts') && !path.endsWith('.spec.tsx'))
      .filter((path) => !ALLOWED_IMPORTERS.includes(path));

    expect(offenders).toEqual([]);
  });

  it('izinli dosya gercekten somut tipleri kuruyor (allowlist olu kalmasin)', () => {
    const [factory] = ALLOWED_IMPORTERS;
    expect(factory).toBeDefined();

    const source = readFileSync(resolve(SOURCE_ROOT, '..', factory ?? ''), 'utf8');
    for (const moduleName of CONCRETE_PROVIDER_MODULES) {
      expect(moduleSpecifiers(source)).toContain(`./${moduleName}`);
    }
  });
});

/**
 * Oturum acilis baglami: Rust'in Stage A paketini prompt bolumlerine cevirir
 * (ASU-035).
 *
 * # Sozlesme
 *
 * - **Retrieval politikasi burada degil.** Hangi hafiza, hangi sirayla ve ne
 *   kadar — hepsi Rust tarafinda (`db::retrieval`). Komut parametresizdir;
 *   renderer secim yapamaz. Bu dosyanin isi tek sey: gelen paketi modelin
 *   okuyacagi metne cevirmek.
 * - **Uydurma yok.** Paket bossa "hatirliyormus gibi davranma" talimati
 *   prompt'a **acikca** girer (PROJECT.md Bolum 11: never invent memories).
 *   Bos baglam · kapali hafiza · bozuk hafiza uc ayri cumleyle anlatilir;
 *   ucu ayni sey degil.
 * - **Konusma bloklanmaz.** Baglam okunamazsa (`unavailable`, ACL, IPC hatasi)
 *   oturum yine acilir: durum log'lanir ve modele "hafiza su an
 *   kullanilamiyor" denir. Sessiz yutma da yok, kapali kapi da.
 * - Gelen payload **harici veridir**: tip iddia edilmez, dogrulanir
 *   (`src/shared/contract.ts`).
 *
 * # Gizlilik
 *
 * Log satirlari **sayi** tasir (kac kayit, kac kelime); hafiza basligi ya da
 * icerigi log'a yazilmaz. Kullanicinin en mahrem verisi bir "context yuklendi"
 * satirinda durmamali.
 */

import { invoke } from '@tauri-apps/api/core';

import { buildAsunaInstructions } from '../prompts';
import { logger as defaultLogger, type AsunaLogger } from '../observability';
import {
  ContractError,
  assertNoUnexpectedKeys,
  isRecord,
  readers,
} from '../../shared/contract';
import { MEMORY_KINDS, type MemoryKind } from '../../shared/memory';
import { toStoreError } from '../../shared/store-error';

/**
 * Rust komut adi. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-memory-read.json` ile birebir ayni olmali.
 */
export const BOOTSTRAP_CONTEXT_COMMAND = 'get_bootstrap_context';

// ---------------------------------------------------------------------------
// Sozlesme — Rust `db::retrieval` tiplerinin aynasi
// ---------------------------------------------------------------------------

/** Baglama giren tek hafiza; DB satirinin tamami degil, prompt'a gidecek hali. */
export interface ContextMemory {
  readonly id: number;
  readonly kind: MemoryKind;
  readonly title: string;
  /** Ozet varsa ozet, yoksa icerik — kirpilmis olabilir. */
  readonly text: string;
  readonly projectId: string | null;
  readonly importance: number;
  readonly createdAt: string;
  readonly truncated: boolean;
}

export interface RecentSessionContext {
  readonly id: number;
  readonly endedAt: string;
  readonly summary: string;
  readonly truncated: boolean;
}

/** Phase 4 (ASU-039+) dolduracak; su an her zaman `null`. */
export interface ProjectContext {
  readonly id: string;
  readonly name: string;
  readonly summary: string | null;
}

/** Phase 6 dolduracak; su an her zaman bos. */
export interface ActiveTask {
  readonly id: number;
  readonly title: string;
}

/** Paketin **olculen** boyutu (PROJECT.md Bolum 25). */
export interface ContextBudget {
  readonly wordLimit: number;
  readonly wordCount: number;
  readonly included: number;
  readonly dropped: number;
  readonly truncated: number;
}

export interface SessionBootstrapContext {
  /** `false` = kalici hafiza kapali (kullanicinin karari, ariza degil). */
  readonly memoryAvailable: boolean;
  readonly userPreferences: readonly ContextMemory[];
  readonly currentProject: ProjectContext | null;
  readonly recentSession: RecentSessionContext | null;
  readonly activeTasks: readonly ActiveTask[];
  readonly relevantMemories: readonly ContextMemory[];
  readonly budget: ContextBudget;
}

const CONTEXT_KEYS = [
  'memoryAvailable',
  'userPreferences',
  'currentProject',
  'recentSession',
  'activeTasks',
  'relevantMemories',
  'budget',
] as const;

const MEMORY_KEYS = [
  'id',
  'kind',
  'title',
  'text',
  'projectId',
  'importance',
  'createdAt',
  'truncated',
] as const;

export class BootstrapContextError extends ContractError {
  public override readonly name = 'BootstrapContextError';
}

function fail(field: string, expected: string): never {
  throw new BootstrapContextError(`\`${field}\` ${expected} olmali.`);
}

function failWith(message: string): never {
  throw new BootstrapContextError(message);
}

function parseContextMemory(value: unknown): ContextMemory {
  if (!isRecord(value)) {
    throw new BootstrapContextError('Baglam hafizasi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, MEMORY_KEYS, failWith);
  const read = readers(value, fail);

  return {
    id: read.id('id'),
    kind: read.enumeration('kind', MEMORY_KINDS),
    title: read.text('title'),
    text: read.text('text'),
    projectId: read.nullableText('projectId'),
    importance: read.unitInterval('importance'),
    createdAt: read.timestamp('createdAt'),
    truncated: read.boolean('truncated'),
  };
}

function parseMemoryList(value: unknown, field: string): ContextMemory[] {
  if (!Array.isArray(value)) {
    fail(field, 'bir dizi');
  }
  return value.map(parseContextMemory);
}

function parseRecentSession(value: unknown): RecentSessionContext | null {
  if (value === null) {
    return null;
  }
  if (!isRecord(value)) {
    throw new BootstrapContextError('`recentSession` bir nesne ya da null olmali.');
  }
  assertNoUnexpectedKeys(value, ['id', 'endedAt', 'summary', 'truncated'], failWith);
  const read = readers(value, fail);

  return {
    id: read.id('id'),
    endedAt: read.timestamp('endedAt'),
    summary: read.text('summary'),
    truncated: read.boolean('truncated'),
  };
}

function parseProjectContext(value: unknown): ProjectContext | null {
  if (value === null) {
    return null;
  }
  if (!isRecord(value)) {
    throw new BootstrapContextError('`currentProject` bir nesne ya da null olmali.');
  }
  assertNoUnexpectedKeys(value, ['id', 'name', 'summary'], failWith);
  const read = readers(value, fail);

  return {
    id: read.text('id'),
    name: read.text('name'),
    summary: read.nullableText('summary'),
  };
}

function parseActiveTasks(value: unknown): ActiveTask[] {
  if (!Array.isArray(value)) {
    fail('activeTasks', 'bir dizi');
  }
  return value.map((entry: unknown) => {
    if (!isRecord(entry)) {
      throw new BootstrapContextError('Aktif is bir nesne olmali.');
    }
    assertNoUnexpectedKeys(entry, ['id', 'title'], failWith);
    const read = readers(entry, fail);
    return { id: read.id('id'), title: read.text('title') };
  });
}

function parseBudget(value: unknown): ContextBudget {
  if (!isRecord(value)) {
    throw new BootstrapContextError('`budget` bir nesne olmali.');
  }
  assertNoUnexpectedKeys(
    value,
    ['wordLimit', 'wordCount', 'included', 'dropped', 'truncated'],
    failWith,
  );
  const read = readers(value, fail);

  return {
    wordLimit: read.count('wordLimit'),
    wordCount: read.count('wordCount'),
    included: read.count('included'),
    dropped: read.count('dropped'),
    truncated: read.count('truncated'),
  };
}

export function parseSessionBootstrapContext(value: unknown): SessionBootstrapContext {
  if (!isRecord(value)) {
    throw new BootstrapContextError('Baglam paketi bir nesne olmali.');
  }
  assertNoUnexpectedKeys(value, CONTEXT_KEYS, failWith);
  const read = readers(value, fail);

  return {
    memoryAvailable: read.boolean('memoryAvailable'),
    userPreferences: parseMemoryList(value['userPreferences'], 'userPreferences'),
    currentProject: parseProjectContext(value['currentProject']),
    recentSession: parseRecentSession(value['recentSession']),
    activeTasks: parseActiveTasks(value['activeTasks']),
    relevantMemories: parseMemoryList(value['relevantMemories'], 'relevantMemories'),
    budget: parseBudget(value['budget']),
  };
}

/** Baglam paketini host'tan ister. Hata **yutulmaz**, tipli olarak firlar. */
export async function fetchSessionBootstrapContext(): Promise<SessionBootstrapContext> {
  try {
    return parseSessionBootstrapContext(await invoke<unknown>(BOOTSTRAP_CONTEXT_COMMAND));
  } catch (error) {
    if (error instanceof BootstrapContextError) {
      throw error;
    }
    throw toStoreError(error);
  }
}

// ---------------------------------------------------------------------------
// Prompt bolumleri
// ---------------------------------------------------------------------------

/**
 * Hicbir hafiza yokken prompt'a giren satir.
 *
 * ASU-035 kabul kriteri: "baglam bos gecerse prompt bunu belirtiyor". Cumle
 * bilerek emir kipinde ve kisa — modelin bos baglamda gecmis uydurmasi
 * (PROJECT.md Bolum 11) tek satirla kapatilir.
 */
export const EMPTY_MEMORY_NOTICE =
  '# Hafiza durumu\n' +
  'Kalıcı hafıza boş — geçmiş konuşma hatırlamıyorsun, hatırlıyormuş gibi davranma.';

/** Kullanici kalici hafizayi kapatmis (ASU-037). Bos olmasi bir ariza degil. */
export const DISABLED_MEMORY_NOTICE =
  '# Hafiza durumu\n' +
  'Kalıcı hafıza kapalı — geçmiş konuşma hatırlamıyorsun, hatırlıyormuş gibi davranma. ' +
  'Kullanıcı sorarsa hafızanın kapalı olduğunu söyle.';

/** Hafiza bozuk/okunamiyor. Konusma devam eder ama model bunu bilir. */
export const UNAVAILABLE_MEMORY_NOTICE =
  '# Hafiza durumu\n' +
  'Kalıcı hafıza şu an okunamıyor — geçmişi hatırlamıyorsun, hatırlıyormuş gibi davranma. ' +
  'Sorulursa hafızaya erişemediğini söyle.';

/** Dolu baglamin basina konan cerceve; "yalnizca bunlari hatirliyorsun". */
export const MEMORY_HEADER_NOTICE =
  '# Hafiza\n' +
  'Aşağıdaki bölümler kalıcı hafızandan getirildi. Yalnızca burada yazanları hatırlıyorsun; ' +
  'burada olmayan bir şeyi hatırlıyormuş gibi konuşma.';

function bullet(memory: ContextMemory): string {
  return `- ${memory.title}: ${memory.text}`;
}

function kindBullet(memory: ContextMemory): string {
  return `- [${memory.kind}] ${memory.title}: ${memory.text}`;
}

/**
 * Baglam paketini `buildAsunaInstructions`'in `additionalSections` girdisine
 * cevirir.
 *
 * Bos bolum **eklenmez**: bosluk birakmak yerine bolum hic yazilmaz, cunku
 * "Hatirlanan tercihler: (yok)" gibi bir satir hem token harcar hem modele
 * doldurulacak bir bosluk gosterir.
 */
export function buildBootstrapSections(context: SessionBootstrapContext): string[] {
  const isEmpty =
    context.userPreferences.length === 0 &&
    context.relevantMemories.length === 0 &&
    context.recentSession === null &&
    context.currentProject === null &&
    context.activeTasks.length === 0;

  if (isEmpty) {
    return [context.memoryAvailable ? EMPTY_MEMORY_NOTICE : DISABLED_MEMORY_NOTICE];
  }

  const sections = [MEMORY_HEADER_NOTICE];

  if (context.userPreferences.length > 0) {
    sections.push(
      ['# Hatırlanan tercihler', ...context.userPreferences.map(bullet)].join('\n'),
    );
  }

  if (context.recentSession !== null) {
    sections.push(['# Son oturum özeti', context.recentSession.summary].join('\n'));
  }

  if (context.relevantMemories.length > 0) {
    sections.push(
      ['# İlgili hafızalar', ...context.relevantMemories.map(kindBullet)].join('\n'),
    );
  }

  return sections;
}

// ---------------------------------------------------------------------------
// Oturum talimati
// ---------------------------------------------------------------------------

export interface SessionInstructionsDeps {
  readonly fetchContext?: () => Promise<SessionBootstrapContext>;
  readonly logger?: AsunaLogger;
}

/**
 * Oturum acilmadan **once** cagrilir: baglami ceker, cekirdek prompt'a ekler.
 *
 * Hicbir kosulda firlatmaz — talimat uretimi konusmayi bloklamaz. Hata halinde
 * baglamsiz (ama durust) talimat doner ve neden log'a yazilir.
 */
export async function buildSessionInstructions(
  deps: SessionInstructionsDeps = {},
): Promise<string> {
  const log = (deps.logger ?? defaultLogger).child('memory-context');
  const fetchContext = deps.fetchContext ?? fetchSessionBootstrapContext;

  let sections: readonly string[];
  try {
    const context = await fetchContext();
    sections = buildBootstrapSections(context);
    // GIZLILIK: yalnizca sayilar. Baslik/icerik log'a girmez.
    log.info('Oturum baglami hazir.', {
      memoryAvailable: context.memoryAvailable,
      preferences: context.userPreferences.length,
      relevantMemories: context.relevantMemories.length,
      recentSession: context.recentSession !== null,
      wordCount: context.budget.wordCount,
      wordLimit: context.budget.wordLimit,
      dropped: context.budget.dropped,
      truncated: context.budget.truncated,
    });
  } catch (error) {
    // Hafiza okunamadi: oturum yine aciliyor, ama model bunu bilerek konusuyor.
    sections = [UNAVAILABLE_MEMORY_NOTICE];
    log.warn('Oturum baglami okunamadi; konusma baglamsiz devam ediyor.', {
      detail: error instanceof Error ? error.message : String(error),
    });
  }

  return buildAsunaInstructions({ additionalSections: sections });
}

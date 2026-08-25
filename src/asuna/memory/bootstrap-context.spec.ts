import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ASUNA_CORE_PROMPT } from '../prompts';
import { AsunaLogger, type LogEntry } from '../observability';
import { AsunaStoreError } from '../../shared/store-error';
import {
  BOOTSTRAP_CONTEXT_COMMAND,
  BootstrapContextError,
  DISABLED_MEMORY_NOTICE,
  EMPTY_MEMORY_NOTICE,
  MEMORY_HEADER_NOTICE,
  UNAVAILABLE_MEMORY_NOTICE,
  buildBootstrapSections,
  buildSessionInstructions,
  fetchSessionBootstrapContext,
  parseSessionBootstrapContext,
  type ContextMemory,
  type SessionBootstrapContext,
} from './bootstrap-context';

const invokeMock = vi.hoisted(() => vi.fn<(command: string) => Promise<unknown>>());

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const BUDGET = { wordLimit: 2000, wordCount: 0, included: 0, dropped: 0, truncated: 0 };

const EMPTY_CONTEXT: SessionBootstrapContext = {
  memoryAvailable: true,
  userPreferences: [],
  currentProject: null,
  recentSession: null,
  activeTasks: [],
  relevantMemories: [],
  budget: BUDGET,
};

function memory(overrides: Partial<ContextMemory> = {}): ContextMemory {
  return {
    id: 1,
    kind: 'decision',
    title: 'Wake word yerel kalir',
    text: 'Wake word tespiti cihazda calisir.',
    projectId: 'asuna',
    importance: 0.9,
    createdAt: '2026-08-25T10:00:00Z',
    truncated: false,
    ...overrides,
  };
}

/**
 * Konsola yazmayan, satirlari toplayan logger.
 *
 * Sink uzerinden toplaniyor (metoda spy takarak degil): servis `child(...)`
 * ile alt logger kuruyor ve sink'ler paylasiliyor — olculen sey gercekten
 * yazilan satir.
 */
function recordingLogger(): { logger: AsunaLogger; entries: LogEntry[] } {
  const entries: LogEntry[] = [];
  return {
    logger: new AsunaLogger({
      level: 'debug',
      sinks: [
        (entry: LogEntry): void => {
          entries.push(entry);
        },
      ],
    }),
    entries,
  };
}

describe('parseSessionBootstrapContext', () => {
  it('tam paketi dogrulayarak okur', () => {
    const context = parseSessionBootstrapContext({
      memoryAvailable: true,
      userPreferences: [
        {
          id: 3,
          kind: 'preference',
          title: 'Kisa cevap',
          text: 'Kod yazarken kisa cevap.',
          projectId: null,
          importance: 0.8,
          createdAt: '2026-08-25T09:00:00Z',
          truncated: false,
        },
      ],
      currentProject: null,
      recentSession: {
        id: 7,
        endedAt: '2026-08-25T09:30:00Z',
        summary: 'Gecen oturumda retrieval konusuldu.',
        truncated: false,
      },
      activeTasks: [],
      relevantMemories: [],
      budget: { wordLimit: 2000, wordCount: 12, included: 2, dropped: 0, truncated: 0 },
    });

    expect(context.userPreferences[0]?.title).toBe('Kisa cevap');
    expect(context.recentSession?.id).toBe(7);
    expect(context.budget.wordCount).toBe(12);
  });

  /** IPC'den gelen veri harici veridir: tip iddia edilmez, dogrulanir. */
  it('bozuk payload sessizce gecmez', () => {
    expect(() => parseSessionBootstrapContext(null)).toThrow(BootstrapContextError);
    expect(() =>
      parseSessionBootstrapContext({ ...EMPTY_CONTEXT, memoryAvailable: 'evet' }),
    ).toThrow(BootstrapContextError);
    expect(() => parseSessionBootstrapContext({ ...EMPTY_CONTEXT, surpriz: true })).toThrow(
      /beklenmeyen alan/i,
    );
  });
});

describe('fetchSessionBootstrapContext', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  /** Komut **parametresiz**: renderer retrieval politikasina dokunamaz. */
  it('komutu argumansiz cagirir', async () => {
    invokeMock.mockResolvedValue(EMPTY_CONTEXT);

    await fetchSessionBootstrapContext();

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith(BOOTSTRAP_CONTEXT_COMMAND);
  });

  it('Rust hatasini tipli hataya cevirir', async () => {
    invokeMock.mockRejectedValue({ code: 'unavailable', message: 'hafiza kullanilamiyor' });

    await expect(fetchSessionBootstrapContext()).rejects.toBeInstanceOf(AsunaStoreError);
  });
});

describe('buildBootstrapSections', () => {
  /**
   * ASU-035 kabul kriteri: hicbir hafiza yoksa prompt bunu **acikca** soyler.
   * Sessiz kalmak, modelin bos baglami gecmisle doldurmasina davetiye olurdu.
   */
  it('bos baglamda uydurmayi yasaklayan tek satir doner', () => {
    expect(buildBootstrapSections(EMPTY_CONTEXT)).toEqual([EMPTY_MEMORY_NOTICE]);
    expect(EMPTY_MEMORY_NOTICE).toContain('hatırlıyormuş gibi davranma');
  });

  /** Kapali hafiza ile bos hafiza ayni cumleyle anlatilmaz. */
  it('hafiza kapaliyken bunu ayrica belirtir', () => {
    const sections = buildBootstrapSections({ ...EMPTY_CONTEXT, memoryAvailable: false });

    expect(sections).toEqual([DISABLED_MEMORY_NOTICE]);
    expect(sections[0]).toContain('kapalı');
  });

  it('dolu baglami uc bolume cevirir', () => {
    const sections = buildBootstrapSections({
      ...EMPTY_CONTEXT,
      userPreferences: [
        memory({ id: 3, kind: 'preference', title: 'Kisa cevap', text: 'Kisa konus.' }),
      ],
      recentSession: {
        id: 7,
        endedAt: '2026-08-25T09:30:00Z',
        summary: 'Gecen oturumda retrieval konusuldu.',
        truncated: false,
      },
      relevantMemories: [memory()],
    });

    expect(sections[0]).toBe(MEMORY_HEADER_NOTICE);
    expect(sections[1]).toBe('# Hatırlanan tercihler\n- Kisa cevap: Kisa konus.');
    expect(sections[2]).toBe('# Son oturum özeti\nGecen oturumda retrieval konusuldu.');
    expect(sections[3]).toBe(
      '# İlgili hafızalar\n- [decision] Wake word yerel kalir: Wake word tespiti cihazda calisir.',
    );
  });

  /** Bos bolum yazilmaz: hem token harcar hem modele bosluk gosterir. */
  it('bos bolumleri hic eklemez', () => {
    const sections = buildBootstrapSections({
      ...EMPTY_CONTEXT,
      relevantMemories: [memory()],
    });

    expect(sections).toHaveLength(2);
    expect(sections.some((section) => section.includes('Hatırlanan tercihler'))).toBe(false);
    expect(sections.some((section) => section.includes('Son oturum özeti'))).toBe(false);
  });
});

describe('buildSessionInstructions', () => {
  it('bolumleri cekirdek prompt"un ardina ekler', async () => {
    const instructions = await buildSessionInstructions({
      logger: recordingLogger().logger,
      fetchContext: () => Promise.resolve({ ...EMPTY_CONTEXT, relevantMemories: [memory()] }),
    });

    expect(instructions.startsWith(ASUNA_CORE_PROMPT)).toBe(true);
    expect(instructions).toContain(MEMORY_HEADER_NOTICE);
    expect(instructions).toContain('Wake word yerel kalir');
  });

  it('bos hafizada cekirdek prompt + tek uyari satiri uretir', async () => {
    const instructions = await buildSessionInstructions({
      logger: recordingLogger().logger,
      fetchContext: () => Promise.resolve(EMPTY_CONTEXT),
    });

    expect(instructions).toBe(`${ASUNA_CORE_PROMPT}\n\n${EMPTY_MEMORY_NOTICE}`);
  });

  /**
   * Hafiza bozuk: konusma **bloklanmaz**. Talimat baglamsiz uretilir, model
   * hafizaya erisemedigini bilir ve olay log'a duser (sessiz yutma yok).
   */
  it('hafiza kullanilamazsa baglamsiz ama durust devam eder', async () => {
    const { logger, entries } = recordingLogger();

    const instructions = await buildSessionInstructions({
      logger,
      fetchContext: () =>
        Promise.reject(new AsunaStoreError('unavailable', 'hafiza kullanilamiyor')),
    });

    expect(instructions).toBe(`${ASUNA_CORE_PROMPT}\n\n${UNAVAILABLE_MEMORY_NOTICE}`);
    // Sessiz yutma yok: neden log'a duser.
    expect(entries.filter((entry) => entry.level === 'warn')).toHaveLength(1);
  });

  /** GIZLILIK: log satiri sayilari tasir, hafiza icerigini degil. */
  it('log satirina hafiza icerigi yazmaz', async () => {
    const { logger, entries } = recordingLogger();

    await buildSessionInstructions({
      logger,
      fetchContext: () =>
        Promise.resolve({
          ...EMPTY_CONTEXT,
          relevantMemories: [memory({ title: 'GIZLI BASLIK', text: 'gizli icerik' })],
          budget: { ...BUDGET, wordCount: 9, included: 1 },
        }),
    });

    const entry = entries[0];
    const logged = `${entry?.message ?? ''} ${JSON.stringify(entry?.data)}`;
    expect(logged).not.toContain('GIZLI BASLIK');
    expect(logged).not.toContain('gizli icerik');
    expect(entry?.data).toMatchObject({ relevantMemories: 1, wordCount: 9, wordLimit: 2000 });
  });
});

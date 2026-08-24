import { beforeEach, describe, expect, it, vi } from 'vitest';

import { MemoryContractError, type MemoryRecord } from '../../shared/memory';
import { AsunaStoreError } from '../../shared/store-error';
import {
  MEMORY_READ_COMMANDS,
  MEMORY_WRITE_COMMANDS,
  archiveMemory,
  createMemory,
  deleteMemory,
  getMemoryById,
  listMemories,
  updateMemory,
} from './memory-service';

const invokeMock = vi.hoisted(() =>
  vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(),
);

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const RECORD: MemoryRecord = {
  id: 1,
  kind: 'decision',
  title: 'Wake word yerel kalir',
  content: 'Wake word tespiti bulutta degil, cihazda calisir.',
  summary: null,
  projectId: 'asuna',
  importance: 0.95,
  confidence: 1,
  sourceSessionId: 7,
  createdAt: '2026-08-25T10:00:00Z',
  updatedAt: '2026-08-25T10:00:00Z',
  lastAccessedAt: null,
  expiresAt: null,
  isArchived: false,
  metadataJson: '{}',
};

describe('memory-service komut adlari', () => {
  /** ACL dort yerde ayni adi bekler; yazim hatasi sessiz redde donusur. */
  it('ACL"de kayitli adlarla birebir ayni', () => {
    expect(MEMORY_READ_COMMANDS.list).toBe('memory_list');
    expect(MEMORY_WRITE_COMMANDS.create).toBe('memory_create');
    expect(MEMORY_WRITE_COMMANDS.update).toBe('memory_update');
    expect(MEMORY_WRITE_COMMANDS.archive).toBe('memory_archive');
    expect(MEMORY_WRITE_COMMANDS.delete).toBe('memory_delete');
  });

  /** Okuma yuzeyi yazma komutu tasimamali (capability ayrimi ile ayni disiplin). */
  it('okuma ve yazma kumeleri ayri', () => {
    const reads = Object.values(MEMORY_READ_COMMANDS) as string[];
    const writes = Object.values(MEMORY_WRITE_COMMANDS) as string[];
    expect(reads.some((command) => writes.includes(command))).toBe(false);
  });
});

describe('listMemories', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('filtreyi Rust tarafina oldugu gibi gecirir', async () => {
    invokeMock.mockResolvedValue([RECORD]);

    await listMemories({ kinds: ['decision'], search: 'wake', limit: 10 });

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('memory_list', {
      filter: { kinds: ['decision'], search: 'wake', limit: 10 },
    });
  });

  it('filtre verilmezse varsayilanlari Rust tarafina birakir', async () => {
    invokeMock.mockResolvedValue([]);

    await listMemories();

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('memory_list', { filter: null });
  });

  it('yaniti dogrular', async () => {
    invokeMock.mockResolvedValue([RECORD]);
    await expect(listMemories()).resolves.toEqual([RECORD]);
  });

  /** Sozlesme disi bir alan sessizce renderer'a akmamali. */
  it('sozlesmeye uymayan kaydi reddeder', async () => {
    invokeMock.mockResolvedValue([{ ...RECORD, embedding: [1, 2, 3] }]);
    await expect(listMemories()).rejects.toBeInstanceOf(MemoryContractError);
  });

  /** Hafiza kapaliyken Rust bos liste doner — bu bir hata degil. */
  it('bos listeyi normal sonuc olarak doner', async () => {
    invokeMock.mockResolvedValue([]);
    await expect(listMemories()).resolves.toEqual([]);
  });
});

describe('getMemoryById', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  /** Ayri bir IPC komutu acilmadi: `memory_list`'in id filtresi kullaniliyor. */
  it('yeni bir komut acmadan tek kaydi getirir', async () => {
    invokeMock.mockResolvedValue([RECORD]);

    await expect(getMemoryById(1)).resolves.toEqual(RECORD);

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('memory_list', {
      filter: { id: 1, archived: 'all', includeExpired: true, limit: 1, markAccessed: false },
    });
  });

  it('erisim izi birakmayi cagirana birakir', async () => {
    invokeMock.mockResolvedValue([RECORD]);

    await getMemoryById(1, true);

    expect(invokeMock.mock.calls[0]?.[1]).toMatchObject({
      filter: { markAccessed: true },
    });
  });

  it('kayit yoksa null doner', async () => {
    invokeMock.mockResolvedValue([]);
    await expect(getMemoryById(42)).resolves.toBeNull();
  });
});

describe('yazma islemleri', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('createMemory taslagi gonderir ve sonucu dogrular', async () => {
    invokeMock.mockResolvedValue({ status: 'stored', record: RECORD });

    const result = await createMemory({
      kind: 'decision',
      title: RECORD.title,
      content: RECORD.content,
      importance: 0.95,
      confidence: 1,
    });

    expect(result).toEqual({ status: 'stored', record: RECORD });
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('memory_create', {
      draft: {
        kind: 'decision',
        title: RECORD.title,
        content: RECORD.content,
        importance: 0.95,
        confidence: 1,
      },
    });
  });

  /**
   * ASU-031 kabul kriteri: hafiza kapaliyken yazma yapilmaz — ve bu **gorunur**.
   * Servis `skipped` sonucunu gizlemez, "kaydettim" demez.
   */
  it('hafiza kapaliyken skipped sonucunu oldugu gibi tasir', async () => {
    invokeMock.mockResolvedValue({ status: 'skipped', reason: 'memory-disabled' });

    await expect(
      createMemory({
        kind: 'idea',
        title: 't',
        content: 'c',
        importance: 0.5,
        confidence: 0.5,
      }),
    ).resolves.toEqual({ status: 'skipped', reason: 'memory-disabled' });
  });

  it('updateMemory null ile "temizle" istegini gecirebilir', async () => {
    invokeMock.mockResolvedValue({ status: 'stored', record: RECORD });

    await updateMemory(1, { summary: null, title: 'yeni' });

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('memory_update', {
      id: 1,
      patch: { summary: null, title: 'yeni' },
    });
  });

  it('archiveMemory varsayilan olarak arsivler', async () => {
    invokeMock.mockResolvedValue({ status: 'stored', record: { ...RECORD, isArchived: true } });

    await archiveMemory(1);

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('memory_archive', {
      id: 1,
      archived: true,
    });
  });

  it('deleteMemory silinen kimligi dondurur', async () => {
    invokeMock.mockResolvedValue({ status: 'deleted', id: 1 });

    await expect(deleteMemory(1)).resolves.toEqual({ status: 'deleted', id: 1 });
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('memory_delete', { id: 1 });
  });

  it('taninmayan yazma sonucunu reddeder', async () => {
    invokeMock.mockResolvedValue({ status: 'maybe' });
    await expect(deleteMemory(1)).rejects.toBeInstanceOf(MemoryContractError);
  });
});

describe('hata cevirisi', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  /**
   * ASU-031 kabul kriteri: DB hatasi sessizce yutulmaz. Rust'in tipli hatasi
   * `code` ile birlikte gelir; UI "kapali" ile "bozuk"u ayirt edebilmeli.
   */
  it('Rust tipli hatasini koduyla birlikte firlatir', async () => {
    invokeMock.mockRejectedValue({
      code: 'unavailable',
      message: 'hafiza kullanilamiyor: veritabani dosyasi acilamadi',
    });

    const error = await listMemories().catch((value: unknown) => value);

    expect(error).toBeInstanceOf(AsunaStoreError);
    expect((error as AsunaStoreError).code).toBe('unavailable');
    expect((error as AsunaStoreError).isUnavailable).toBe(true);
  });

  it('not-found hatasini ayirt eder', async () => {
    invokeMock.mockRejectedValue({ code: 'not-found', message: 'kayit bulunamadi' });

    const error = await deleteMemory(9).catch((value: unknown) => value);
    expect((error as AsunaStoreError).code).toBe('not-found');
  });

  /** ACL reddi duz string olarak gelir; uydurulmus bir koda eslenmez. */
  it('ACL reddini unknown koduyla, mesajini kaybetmeden tasir', async () => {
    invokeMock.mockRejectedValue(
      'memory_create not allowed on window "main", referenced by: capability: asuna-memory-write',
    );

    const error = await createMemory({
      kind: 'idea',
      title: 't',
      content: 'c',
      importance: 0.5,
      confidence: 0.5,
    }).catch((value: unknown) => value);

    expect((error as AsunaStoreError).code).toBe('unknown');
    expect((error as AsunaStoreError).message).toContain('not allowed');
  });
});

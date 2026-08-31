/**
 * `chat-service` sozlesme testleri (plan-chat-shell.md WP4 — bosluk analizi).
 *
 * Kanitlanan seyler:
 *
 * 1. **ACL regresyonu**: servisin cagirdigi her komut adi uc yerde birden
 *    kayitli — `build.rs` manifest'i, bir `capabilities/*.json` izni ve o
 *    capability'nin `tauri.conf.json`'da etkin olmasi. Bir yazim hatasi ya da
 *    unutulan kayit uretimde **sessiz redde** donusur; burada kirmizi test olur.
 * 2. Komut argumanlarinin adlari ve sekli (Rust tarafi camelCase bekler).
 * 3. Hata sarma: IPC reddi tipli `AsunaStoreError`'a cevrilir, mesaj korunur;
 *    ACL reddi (duz metin) yutulmaz.
 * 4. Bicimi bozuk bir **basarili** yanit sarmalanmaz — parser hatasi yukari
 *    cikar, sahte bir "bos konusma" uretilmez.
 *
 * IPC yok: `@tauri-apps/api/core` sahtelenir. Gercek OpenAI/Tauri cagrisi yok.
 */

import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { cwd } from 'node:process';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AsunaStoreError } from '../../shared/store-error';

import {
  CHAT_COMMANDS,
  attachProjectFile,
  deleteConversation,
  ingestAttachment,
  listAttachments,
  listConversations,
  listMessages,
  sendMessage,
  setConversationTitle,
  startConversation,
} from './chat-service';

const invokeMock = vi.hoisted(() =>
  vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(),
);

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const NOW = '2026-08-31T10:00:00Z';

const MESSAGE = {
  id: 7,
  sessionId: 3,
  role: 'user',
  content: 'merhaba',
  createdAt: NOW,
} as const;

const REPLY = { userMessage: MESSAGE, assistantMessage: { ...MESSAGE, id: 8, role: 'assistant' } };

const ATTACHMENT = {
  id: 5,
  sessionId: 3,
  messageId: null,
  fileName: 'notlar.md',
  mimeType: 'text/markdown',
  sizeBytes: 12,
  origin: 'upload',
  createdAt: NOW,
} as const;

beforeEach(() => {
  invokeMock.mockReset();
});

// ---------------------------------------------------------------------------
// ACL regresyonu
// ---------------------------------------------------------------------------

const TAURI_DIR = resolve(cwd(), 'src-tauri');

/** `build.rs` ACL manifest'indeki komut adlari. */
function manifestCommands(): string[] {
  const source = readFileSync(resolve(TAURI_DIR, 'build.rs'), 'utf8');
  const start = source.indexOf('.commands(&[');
  expect(start, '`build.rs` ACL manifest"i bulunmali').toBeGreaterThanOrEqual(0);

  const block = source.slice(start, source.indexOf(']))', start));
  return [...block.matchAll(/"([a-z0-9_]+)"/g)].map((match) => match[1] ?? '');
}

/** `tauri.conf.json`'da etkin capability adlari. */
function enabledCapabilities(): string[] {
  const config: unknown = JSON.parse(readFileSync(resolve(TAURI_DIR, 'tauri.conf.json'), 'utf8'));
  const capabilities = (config as { app: { security: { capabilities: string[] } } }).app.security
    .capabilities;
  expect(capabilities).toContain('asuna-chat');
  return capabilities;
}

/**
 * Etkin capability dosyalarindaki `allow-*` izinlerinin komut adina cevrilmis
 * hali (`allow-chat-send` → `chat_send`).
 */
function allowedCommands(): Set<string> {
  const enabled = new Set(enabledCapabilities());
  const directory = resolve(TAURI_DIR, 'capabilities');
  const allowed = new Set<string>();

  for (const file of readdirSync(directory)) {
    if (!file.endsWith('.json')) {
      continue;
    }
    const parsed: unknown = JSON.parse(readFileSync(resolve(directory, file), 'utf8'));
    const capability = parsed as { identifier: string; permissions: unknown[] };
    if (!enabled.has(capability.identifier)) {
      continue;
    }
    for (const permission of capability.permissions) {
      if (typeof permission === 'string' && permission.startsWith('allow-')) {
        allowed.add(permission.slice('allow-'.length).replace(/-/g, '_'));
      }
    }
  }
  return allowed;
}

describe('chat-service komut adlari', () => {
  it('ACL manifest"inde kayitli', () => {
    const commands = manifestCommands();
    for (const command of Object.values(CHAT_COMMANDS)) {
      expect(commands, `build.rs"te yok: ${command}`).toContain(command);
    }
  });

  it('etkin bir capability tarafindan aciliyor', () => {
    const allowed = allowedCommands();
    for (const command of Object.values(CHAT_COMMANDS)) {
      expect(allowed.has(command), `hicbir etkin capability acmiyor: ${command}`).toBe(true);
    }
  });

  /**
   * `message_append` bilerek acilmadi (asuna-chat.json gerekcesi): renderer
   * `assistant` rolunde satir yazabilseydi, model soylememisken "Asuna boyle
   * dedi" uydurulabilirdi. Sohbetin tek yazma yolu `chat_send`.
   */
  it('renderer"a mesaj yazma arka kapisi acmaz', () => {
    expect(Object.values(CHAT_COMMANDS)).not.toContain('message_append');
    expect(allowedCommands().has('message_append')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Arguman sozlesmesi
// ---------------------------------------------------------------------------

describe('okuma komutlari', () => {
  it('konusma listesini parametresiz ister', async () => {
    invokeMock.mockResolvedValue([]);

    await listConversations();

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('conversation_list', {});
  });

  it('mesajlari ve ekleri konusma kimligiyle ister', async () => {
    invokeMock.mockResolvedValue([]);
    await listMessages(3);
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('message_list', { sessionId: 3 });

    invokeMock.mockReset();
    invokeMock.mockResolvedValue([]);
    await listAttachments(3);
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('attachment_list', { sessionId: 3 });
  });
});

describe('startConversation', () => {
  it('projesiz konusmada projectId"yi null gonderir ve modality"yi text yapar', async () => {
    invokeMock.mockResolvedValue({ status: 'recorded', session: { id: 42 } });

    const result = await startConversation();

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_start', {
      projectId: null,
      modality: 'text',
    });
    expect(result).toEqual({ status: 'recorded', id: 42 });
  });

  it('projeli konusmada proje kimligini gecirir', async () => {
    invokeMock.mockResolvedValue({ status: 'recorded', session: { id: 43 } });

    await startConversation('asuna');

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_start', {
      projectId: 'asuna',
      modality: 'text',
    });
  });

  it('hafiza kapaliysa skipped doner — hata firlatmaz', async () => {
    invokeMock.mockResolvedValue({ status: 'skipped', reason: 'memory-disabled' });

    await expect(startConversation()).resolves.toEqual({
      status: 'skipped',
      reason: 'memory-disabled',
    });
  });
});

describe('sendMessage', () => {
  it('metni ve ek kimliklerini gecirir', async () => {
    invokeMock.mockResolvedValue(REPLY);

    await sendMessage(3, 'merhaba', [5, 6]);

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('chat_send', {
      sessionId: 3,
      text: 'merhaba',
      attachmentIds: [5, 6],
    });
  });

  it('ek verilmediginde bos dizi gonderir (undefined degil)', async () => {
    invokeMock.mockResolvedValue(REPLY);

    await sendMessage(3, 'merhaba');

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('chat_send', {
      sessionId: 3,
      text: 'merhaba',
      attachmentIds: [],
    });
  });

  /** Cagiranin dizisi IPC sinirinin arkasina **referansla** gecmez. */
  it('cagiranin dizisini kopyalar', async () => {
    invokeMock.mockResolvedValue(REPLY);
    const ids = [5];

    await sendMessage(3, 'merhaba', ids);

    const args = invokeMock.mock.calls[0]?.[1] as { attachmentIds: number[] };
    expect(args.attachmentIds).toEqual([5]);
    expect(args.attachmentIds).not.toBe(ids);
  });
});

describe('ekleme komutlari', () => {
  it('dosyayi metin olarak okur, adi ve turuyle gonderir', async () => {
    invokeMock.mockResolvedValue(ATTACHMENT);
    const file = new File(['dosya icerigi'], 'notlar.md', { type: 'text/markdown' });

    await ingestAttachment(3, file);

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('attachment_ingest', {
      sessionId: 3,
      fileName: 'notlar.md',
      content: 'dosya icerigi',
      mimeType: 'text/markdown',
    });
  });

  /** Tarayici tur soylemediyse uydurulmaz: `''` degil `null` gider. */
  it('tur bos ise null gonderir', async () => {
    invokeMock.mockResolvedValue({ ...ATTACHMENT, mimeType: null });
    const file = new File(['x'], 'veri', { type: '' });

    await ingestAttachment(3, file);

    const args = invokeMock.mock.calls[0]?.[1] as { mimeType: string | null };
    expect(args.mimeType).toBeNull();
  });

  /**
   * Renderer **mutlak yol kuramaz**: servis yolu oldugu gibi gecirir, cozum ve
   * sandbox reddi Rust tarafinda (`attachment_from_project`).
   */
  it('proje dosyasini gorece yolla ister', async () => {
    invokeMock.mockResolvedValue({ ...ATTACHMENT, origin: 'project' });

    await attachProjectFile(3, 'docs/README.md');

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('attachment_from_project', {
      sessionId: 3,
      relativePath: 'docs/README.md',
    });
  });
});

describe('yazma komutlari', () => {
  it('baslik ve silme komutlarini kendi adlariyla cagirir', async () => {
    invokeMock.mockResolvedValue(null);
    await setConversationTitle(3, 'Chat kabuğu');
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_set_title', {
      sessionId: 3,
      title: 'Chat kabuğu',
    });

    invokeMock.mockReset();
    invokeMock.mockResolvedValue(null);
    await deleteConversation(3);
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('session_delete', { sessionId: 3 });
  });
});

// ---------------------------------------------------------------------------
// Hata sarma
// ---------------------------------------------------------------------------

describe('hata sarma', () => {
  it('Rust StoreError bicimini tipli hataya cevirir, mesaji korur', async () => {
    invokeMock.mockRejectedValue({
      code: 'invalid',
      message: '`attachmentIds` bu konusmaya ait olmayan bir kayit iceriyor',
    });

    const error = await sendMessage(3, 'merhaba', [99]).catch((value: unknown) => value);

    expect(error).toBeInstanceOf(AsunaStoreError);
    expect((error as AsunaStoreError).code).toBe('invalid');
    expect((error as AsunaStoreError).message).toContain('ait olmayan');
  });

  it('model/hafiza erisilemezse unavailable kodunu tasir', async () => {
    invokeMock.mockRejectedValue({ code: 'unavailable', message: 'OpenAI kota sinirina takildi' });

    const error = await sendMessage(3, 'merhaba').catch((value: unknown) => value);

    expect((error as AsunaStoreError).isUnavailable).toBe(true);
  });

  /** ACL reddi duz metin gelir; yutulmaz, `unknown` koduyla tasinir. */
  it('ACL reddini yutmaz', async () => {
    invokeMock.mockRejectedValue('chat_send not allowed. Permissions associated with this command');

    const error = await sendMessage(3, 'merhaba').catch((value: unknown) => value);

    expect(error).toBeInstanceOf(AsunaStoreError);
    expect((error as AsunaStoreError).code).toBe('unknown');
    expect((error as AsunaStoreError).message).toContain('not allowed');
  });

  it('her komut ayni sarmalayicidan gecer', async () => {
    invokeMock.mockRejectedValue({ code: 'not-found', message: 'konusma bulunamadi' });

    const calls: readonly Promise<unknown>[] = [
      listConversations(),
      listMessages(3),
      listAttachments(3),
      startConversation(),
      setConversationTitle(3, 'x'),
      deleteConversation(3),
      sendMessage(3, 'x'),
      attachProjectFile(3, 'README.md'),
    ];

    for (const call of calls) {
      const error = await call.catch((value: unknown) => value);
      expect(error).toBeInstanceOf(AsunaStoreError);
      expect((error as AsunaStoreError).code).toBe('not-found');
    }
  });

  /**
   * Bicimi bozuk bir **basarili** yanit sarmalanmaz: parser hatasi yukari cikar.
   * Sessizce bos liste donmek, kullaniciya "konusma yok" yalanini soylerdi.
   */
  it('bozuk basarili yaniti store hatasina cevirmez', async () => {
    invokeMock.mockResolvedValue([{ id: 'uc' }]);

    const error = await listConversations().catch((value: unknown) => value);

    expect(error).toBeInstanceOf(TypeError);
    expect(error).not.toBeInstanceOf(AsunaStoreError);
  });
});

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DbStatusError } from '../../shared/db-status';
import { DB_STATUS_COMMAND, fetchDbStatus } from './db-status-service';

const invokeMock = vi.hoisted(() => vi.fn<(command: string) => Promise<unknown>>());

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const READY_PAYLOAD = {
  availability: 'ready',
  schemaVersion: 0,
  sqliteVersion: '3.53.2',
  reason: null,
};

describe('fetchDbStatus', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('ACL"de kayitli komut adiyla Rust tarafini cagirir', async () => {
    invokeMock.mockResolvedValue(READY_PAYLOAD);

    await fetchDbStatus();

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith(DB_STATUS_COMMAND);
    expect(DB_STATUS_COMMAND).toBe('db_status');
  });

  it('yaniti dogrular', async () => {
    invokeMock.mockResolvedValue(READY_PAYLOAD);
    await expect(fetchDbStatus()).resolves.toEqual(READY_PAYLOAD);
  });

  it('sozlesmeye uymayan yaniti reddeder', async () => {
    invokeMock.mockResolvedValue({ availability: 'ready' });
    await expect(fetchDbStatus()).rejects.toBeInstanceOf(DbStatusError);
  });

  /** Durum sorgusu onbelleklenmez: hafiza calisma aninda bozulabilir. */
  it('her cagrida yeniden sorar', async () => {
    invokeMock.mockResolvedValue(READY_PAYLOAD);

    await fetchDbStatus();
    await fetchDbStatus();

    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it('IPC hatasini yutmaz', async () => {
    invokeMock.mockRejectedValue(new Error('ipc down'));
    await expect(fetchDbStatus()).rejects.toThrow('ipc down');
  });
});

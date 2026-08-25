/**
 * Dizin secici testleri (ASU-045).
 *
 * Kanitlanan sey yalnizca "bir pencere aciliyor mu?" degil: secicinin **dizin**
 * modunda acildigi ve tek secim istedigi. `directory: true` olmasaydi ayni izin
 * (`dialog:allow-open`) bir dosya secicisi haline gelirdi.
 */

import { describe, expect, it, vi } from 'vitest';

const open = vi.fn<(options: unknown) => Promise<unknown>>();

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (options: unknown): Promise<unknown> => open(options),
}));

const { PROJECT_DIRECTORY_PICKER_TITLE, pickProjectDirectory } =
  await import('./directory-picker');

describe('pickProjectDirectory', () => {
  it('seciciyi DIZIN modunda ve tek secimle acar', async () => {
    open.mockResolvedValueOnce('/Users/arlec/Work/asuna');

    await expect(pickProjectDirectory()).resolves.toBe('/Users/arlec/Work/asuna');

    expect(open).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
      title: PROJECT_DIRECTORY_PICKER_TITLE,
    });
  });

  it('kullanici vazgecerse null doner', async () => {
    open.mockResolvedValueOnce(null);

    await expect(pickProjectDirectory()).resolves.toBeNull();
  });

  it('beklenmedik sekil (coklu secim) yol olarak kabul edilmez', async () => {
    open.mockResolvedValueOnce(['/bir', '/iki']);

    await expect(pickProjectDirectory()).resolves.toBeNull();
  });

  it('secici acilamazsa hata yutulmaz', async () => {
    open.mockRejectedValueOnce(new Error('dialog.open not allowed'));

    await expect(pickProjectDirectory()).rejects.toThrow('dialog.open not allowed');
  });
});

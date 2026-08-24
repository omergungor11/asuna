/**
 * `registerWindowCloseHandler` testleri (ASU-018).
 *
 * Test ortaminda Tauri calisma zamani yok; yalnizca `beforeunload` yolu kurulur ve
 * `@tauri-apps/api/window` modulu hic yuklenmez.
 */

import { describe, expect, it, vi } from 'vitest';

import { registerWindowCloseHandler } from './window-lifecycle';

describe('registerWindowCloseHandler', () => {
  it('pencere bosaltilirken kancayi cagirir', () => {
    const handler = vi.fn();
    const detach = registerWindowCloseHandler(handler);

    window.dispatchEvent(new Event('beforeunload'));
    expect(handler).toHaveBeenCalledTimes(1);

    detach();
  });

  it('sokuldukten sonra bir daha cagirmaz (dinleyici sizmiyor)', () => {
    const handler = vi.fn();
    const detach = registerWindowCloseHandler(handler);

    detach();
    window.dispatchEvent(new Event('beforeunload'));

    expect(handler).not.toHaveBeenCalled();
  });

  it('sokme islemi tekrar cagrilabilir', () => {
    const handler = vi.fn();
    const detach = registerWindowCloseHandler(handler);

    detach();
    detach();

    window.dispatchEvent(new Event('beforeunload'));
    expect(handler).not.toHaveBeenCalled();
  });
});

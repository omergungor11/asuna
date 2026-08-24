import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  GET_FRONTEND_CONFIG_COMMAND,
  loadFrontendConfig,
  resetFrontendConfigCache,
} from './config.service';
import { FrontendConfigError } from './frontend-config';

const invokeMock = vi.hoisted(() => vi.fn<(command: string) => Promise<unknown>>());

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const VALID_PAYLOAD = {
  realtimeModel: 'gpt-realtime-2.1-mini',
  realtimeVoice: null,
  wakeWord: 'Hey Asuna',
  wakeWordProvider: 'sherpa-kws',
  idleTimeoutSeconds: 45,
  logLevel: 'debug',
  memoryEnabled: false,
  transcriptStorage: true,
  toolApprovalMode: 'always',
  turnDetection: 'semantic_vad',
  vadEagerness: 'high',
  vadSilenceMs: 400,
};

describe('loadFrontendConfig', () => {
  beforeEach(() => {
    resetFrontendConfigCache();
    invokeMock.mockReset();
  });

  afterEach(() => {
    resetFrontendConfigCache();
  });

  it('dogru komut adiyla Rust tarafini cagirir', async () => {
    invokeMock.mockResolvedValue(VALID_PAYLOAD);

    await loadFrontendConfig();

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith(GET_FRONTEND_CONFIG_COMMAND);
    // Komut adi Rust ACL manifest'i ve capability dosyasiyla ayni olmali.
    expect(GET_FRONTEND_CONFIG_COMMAND).toBe('get_frontend_config');
  });

  it('donen payload"u dogrulanmis config olarak verir', async () => {
    invokeMock.mockResolvedValue(VALID_PAYLOAD);

    const config = await loadFrontendConfig();

    expect(config.realtimeModel).toBe('gpt-realtime-2.1-mini');
    expect(config.realtimeVoice).toBeNull();
    expect(config.toolApprovalMode).toBe('always');
  });

  it('es zamanli cagrilarda tek IPC turu yapar ve sonucu onbellekler', async () => {
    invokeMock.mockResolvedValue(VALID_PAYLOAD);

    const [first, second] = await Promise.all([loadFrontendConfig(), loadFrontendConfig()]);
    const third = await loadFrontendConfig();

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(first).toBe(second);
    expect(third).toBe(first);
  });

  it('gecersiz payload"da hata firlatir ve onbelleklemez', async () => {
    invokeMock.mockResolvedValue({ ...VALID_PAYLOAD, openaiApiKey: 'sk-sizinti' });

    await expect(loadFrontendConfig()).rejects.toBeInstanceOf(FrontendConfigError);

    invokeMock.mockResolvedValue(VALID_PAYLOAD);
    await expect(loadFrontendConfig()).resolves.toMatchObject({
      realtimeModel: 'gpt-realtime-2.1-mini',
    });
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it('IPC hatasini yutmaz', async () => {
    invokeMock.mockRejectedValue(new Error('komut reddedildi'));

    await expect(loadFrontendConfig()).rejects.toThrow('komut reddedildi');
  });
});

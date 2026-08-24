import { describe, expect, it } from 'vitest';

import {
  FRONTEND_CONFIG_KEYS,
  FrontendConfigError,
  parseFrontendConfig,
  type FrontendConfig,
} from './frontend-config';

const API_KEY_SENTINEL = 'sk-proj-COK-GIZLI-TEST-DEGERI';

function validPayload(): Record<string, unknown> {
  return {
    realtimeModel: 'gpt-realtime-2.1',
    realtimeVoice: 'marin',
    wakeWord: 'Hey Asuna',
    idleTimeoutSeconds: 45,
    logLevel: 'info',
    memoryEnabled: true,
    transcriptStorage: false,
    toolApprovalMode: 'safe',
  };
}

function payloadWith(key: string, value: unknown): Record<string, unknown> {
  return { ...validPayload(), [key]: value };
}

describe('parseFrontendConfig', () => {
  it('gecerli payload"u tipli config"e cevirir', () => {
    const config: FrontendConfig = parseFrontendConfig(validPayload());

    expect(config).toStrictEqual({
      realtimeModel: 'gpt-realtime-2.1',
      realtimeVoice: 'marin',
      wakeWord: 'Hey Asuna',
      idleTimeoutSeconds: 45,
      logLevel: 'info',
      memoryEnabled: true,
      transcriptStorage: false,
      toolApprovalMode: 'safe',
    });
  });

  it('realtimeVoice icin null kabul eder (SDK varsayilani)', () => {
    expect(parseFrontendConfig(payloadWith('realtimeVoice', null)).realtimeVoice).toBeNull();
  });

  it('nesne olmayan payload"u reddeder', () => {
    for (const value of [null, undefined, 'config', 42, [], true]) {
      expect(() => parseFrontendConfig(value)).toThrow(FrontendConfigError);
    }
  });

  it('eksik alanlari reddeder', () => {
    for (const key of FRONTEND_CONFIG_KEYS) {
      const payload = validPayload();
      // eslint-disable-next-line @typescript-eslint/no-dynamic-delete -- sozlesme alanlarinin her biri tek tek dusuruluyor
      delete payload[key];
      expect(() => parseFrontendConfig(payload)).toThrow(new RegExp(key));
    }
  });

  it('yanlis tipteki alanlari reddeder', () => {
    const cases: readonly (readonly [string, unknown])[] = [
      ['realtimeModel', 42],
      ['realtimeModel', ''],
      ['realtimeVoice', 7],
      ['wakeWord', null],
      ['idleTimeoutSeconds', '45'],
      ['idleTimeoutSeconds', 0],
      ['idleTimeoutSeconds', 45.5],
      ['memoryEnabled', 'true'],
      ['transcriptStorage', 1],
      ['logLevel', 'verbose'],
      ['toolApprovalMode', 'never'],
    ];

    for (const [key, value] of cases) {
      expect(() => parseFrontendConfig(payloadWith(key, value))).toThrow(FrontendConfigError);
    }
  });

  // --- Guvenlik testleri (CLAUDE.md: guvenlik mantigi test edilmeden merge edilmez) ---

  it('beklenmeyen alan iceren payload"u reddeder (whitelist, blacklist degil)', () => {
    const leaked = { ...validPayload(), openaiApiKey: API_KEY_SENTINEL };

    expect(() => parseFrontendConfig(leaked)).toThrow(FrontendConfigError);
    expect(() => parseFrontendConfig(leaked)).toThrow(/openaiApiKey/);
  });

  it('hata mesajlarina deger sizdirmaz, yalnizca alan adi yazar', () => {
    const cases: readonly Record<string, unknown>[] = [
      { ...validPayload(), openaiApiKey: API_KEY_SENTINEL },
      payloadWith('logLevel', API_KEY_SENTINEL),
      payloadWith('toolApprovalMode', API_KEY_SENTINEL),
      payloadWith('idleTimeoutSeconds', API_KEY_SENTINEL),
      payloadWith('memoryEnabled', API_KEY_SENTINEL),
      payloadWith('realtimeVoice', { secret: API_KEY_SENTINEL }),
    ];

    for (const payload of cases) {
      let message: string | undefined;
      try {
        parseFrontendConfig(payload);
      } catch (error) {
        expect(error).toBeInstanceOf(FrontendConfigError);
        message = error instanceof Error ? error.message : String(error);
      }
      expect(message, `hata bekleniyordu: ${Object.keys(payload).join(',')}`).toBeDefined();
      expect(message).not.toContain(API_KEY_SENTINEL);
    }
  });

  it('donen config yalnizca whitelist alanlarini icerir', () => {
    const config = parseFrontendConfig(validPayload());
    expect(Object.keys(config).sort()).toStrictEqual([...FRONTEND_CONFIG_KEYS].sort());
    expect(JSON.stringify(config)).not.toContain(API_KEY_SENTINEL);
  });
});

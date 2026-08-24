import { describe, expect, it } from 'vitest';

import {
  FRONTEND_CONFIG_KEYS,
  FrontendConfigError,
  describeTurnDetection,
  parseFrontendConfig,
  type FrontendConfig,
} from './frontend-config';

const API_KEY_SENTINEL = 'sk-proj-COK-GIZLI-TEST-DEGERI';

function validPayload(): Record<string, unknown> {
  return {
    realtimeModel: 'gpt-realtime-2.1',
    realtimeVoice: 'marin',
    wakeWord: 'Hey Asuna',
    wakeWordProvider: 'sherpa-kws',
    idleTimeoutSeconds: 45,
    logLevel: 'info',
    memoryEnabled: true,
    transcriptStorage: false,
    toolApprovalMode: 'safe',
    turnDetection: 'semantic_vad',
    vadEagerness: 'high',
    vadSilenceMs: 400,
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
      wakeWordProvider: 'sherpa-kws',
      idleTimeoutSeconds: 45,
      logLevel: 'info',
      memoryEnabled: true,
      transcriptStorage: false,
      toolApprovalMode: 'safe',
      turnDetection: 'semantic_vad',
      vadEagerness: 'high',
      vadSilenceMs: 400,
    });
  });

  it('realtimeVoice icin null kabul eder (SDK varsayilani)', () => {
    expect(parseFrontendConfig(payloadWith('realtimeVoice', null)).realtimeVoice).toBeNull();
  });

  it('tur tespiti alanlarini (ASU-064) sozlesmeye alir', () => {
    const semantic = parseFrontendConfig(validPayload());
    expect(semantic.turnDetection).toBe('semantic_vad');
    expect(semantic.vadEagerness).toBe('high');
    expect(semantic.vadSilenceMs).toBe(400);

    const server = parseFrontendConfig({
      ...validPayload(),
      turnDetection: 'server_vad',
      vadEagerness: 'auto',
      vadSilenceMs: 100,
    });
    expect(server.turnDetection).toBe('server_vad');
    expect(server.vadSilenceMs).toBe(100);
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
      ['wakeWordProvider', 'porcupine'],
      ['wakeWordProvider', 'SHERPA-KWS'],
      ['wakeWordProvider', null],
      ['idleTimeoutSeconds', '45'],
      ['idleTimeoutSeconds', 0],
      ['idleTimeoutSeconds', 45.5],
      ['memoryEnabled', 'true'],
      ['transcriptStorage', 1],
      ['logLevel', 'verbose'],
      ['toolApprovalMode', 'never'],
      ['turnDetection', 'semantic'],
      ['turnDetection', null],
      ['vadEagerness', 'aggressive'],
      ['vadEagerness', 'HIGH'],
      ['vadSilenceMs', '400'],
      ['vadSilenceMs', 0],
      ['vadSilenceMs', 400.5],
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

describe('describeTurnDetection', () => {
  it('semantic modda acikgozluluk etiketini verir', () => {
    const config = parseFrontendConfig(payloadWith('vadEagerness', 'medium'));
    expect(describeTurnDetection(config)).toBe('semantic/medium');
  });

  it('server modda sessizlik penceresini verir', () => {
    const config = parseFrontendConfig({
      ...validPayload(),
      turnDetection: 'server_vad',
      vadSilenceMs: 250,
    });
    expect(describeTurnDetection(config)).toBe('server/250ms');
  });
});

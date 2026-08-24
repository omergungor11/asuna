import { describe, expect, it } from 'vitest';

import {
  describeTokenError,
  parseRealtimeTokenIpcError,
  redactSecrets,
} from './realtime-errors';
import { RealtimeTokenContractError, parseEphemeralRealtimeToken } from './realtime-token';

const VALID_PAYLOAD = {
  value: 'ek_GECERLI_TEST_TOKENI',
  expiresAt: 1_690_000_600,
  model: 'gpt-realtime-2.1',
};

describe('parseEphemeralRealtimeToken', () => {
  it('gecerli IPC yanitini okur', () => {
    expect(parseEphemeralRealtimeToken({ ...VALID_PAYLOAD })).toEqual(VALID_PAYLOAD);
  });

  it('kalici anahtar gorunumlu bir degeri reddeder ve degeri mesaja koymaz', () => {
    const leak = 'sk-proj-COK-GIZLI';
    let message = '';
    try {
      parseEphemeralRealtimeToken({ ...VALID_PAYLOAD, value: leak });
    } catch (error) {
      expect(error).toBeInstanceOf(RealtimeTokenContractError);
      message = error instanceof Error ? error.message : '';
    }
    expect(message).not.toBe('');
    expect(message).not.toContain(leak);
  });

  it.each([
    ['nesne degil', 'ek_x'],
    ['value eksik', { expiresAt: 1, model: 'm' }],
    ['value bos', { value: '   ', expiresAt: 1, model: 'm' }],
    ['expiresAt yanlis tipte', { value: 'ek_x', expiresAt: 'yarin', model: 'm' }],
    ['expiresAt pozitif degil', { value: 'ek_x', expiresAt: 0, model: 'm' }],
    ['model eksik', { value: 'ek_x', expiresAt: 1 }],
    ['model bos', { value: 'ek_x', expiresAt: 1, model: '' }],
  ])('bozuk yaniti reddeder: %s', (_label, payload) => {
    expect(() => parseEphemeralRealtimeToken(payload)).toThrow(RealtimeTokenContractError);
  });
});

describe('parseRealtimeTokenIpcError', () => {
  it('Rust `{ kind, message }` bicimini tanir', () => {
    expect(parseRealtimeTokenIpcError({ kind: 'quota_exceeded', message: 'kota' })).toEqual({
      kind: 'quota_exceeded',
      message: 'kota',
    });
  });

  it.each([
    ['string', 'bir hata'],
    ['null', null],
    ['kind eksik', { message: 'x' }],
    ['message eksik', { kind: 'x' }],
    ['kind bos', { kind: '', message: 'x' }],
  ])('tanimadigi bicime null doner: %s', (_label, payload) => {
    expect(parseRealtimeTokenIpcError(payload)).toBeNull();
  });
});

describe('describeTokenError', () => {
  it('Rust mesajini oldugu gibi tasir ve `kind`i `cause` olarak saklar', () => {
    const info = describeTokenError({
      kind: 'invalid_api_key',
      message: 'OpenAI API anahtari gecersiz (yetkilendirme reddedildi).',
    });

    expect(info.kind).toBe('token');
    expect(info.cause).toBe('invalid_api_key');
    expect(info.message).toContain('gecersiz');
    expect(info.retryable).toBe(false);
  });

  it.each(['network', 'upstream_unavailable'])(
    'gecici hatalari yeniden denenebilir isaretler: %s',
    (kind) => {
      expect(describeTokenError({ kind, message: 'gecici' }).retryable).toBe(true);
    },
  );

  it.each(['invalid_api_key', 'missing_api_key', 'model_access_denied', 'quota_exceeded'])(
    'kalici hatalari yeniden denemez: %s',
    (kind) => {
      expect(describeTokenError({ kind, message: 'kalici' }).retryable).toBe(false);
    },
  );

  it('taninmayan hataya durust ve somut bir mesaj uretir', () => {
    const info = describeTokenError(new Error('IPC kanali kapali'));

    expect(info.kind).toBe('token');
    expect(info.message).toContain('gecici anahtar alinamadi');
    expect(info.message).toContain('IPC kanali kapali');
  });

  it('hata mesajindaki token gorunumlu parcalari redakte eder', () => {
    const info = describeTokenError(new Error('gecersiz token ek_SIZAN_DEGER kullanildi'));

    expect(info.message).not.toContain('ek_SIZAN_DEGER');
    expect(info.message).toContain('ek_<redacted>');
  });
});

describe('redactSecrets', () => {
  it.each([
    ['Bearer sk-proj-ABC123', 'Bearer sk-<redacted>'],
    ['token=ek_abc_DEF-99 bitti', 'token=ek_<redacted> bitti'],
    ['sk-a ek_b sk-c', 'sk-<redacted> ek_<redacted> sk-<redacted>'],
    // Yanlis pozitif olmamali
    ['gpt-realtime-2.1 modeli', 'gpt-realtime-2.1 modeli'],
    ['skor ekip degeri', 'skor ekip degeri'],
    ['', ''],
  ])('maskeler: %s', (input, expected) => {
    expect(redactSecrets(input)).toBe(expected);
  });
});

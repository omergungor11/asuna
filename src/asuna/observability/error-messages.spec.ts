import { describe, expect, it } from 'vitest';

import { redactText } from './logger';
import {
  ASUNA_ERROR_KINDS,
  REALTIME_TOKEN_ERROR_KINDS,
  UNKNOWN_ERROR_KIND,
  describeUserFacingError,
  errorKindOf,
  isAsunaErrorKind,
  toUserFacingError,
  userFacingErrorFor,
} from './error-messages';

describe('kind kapsami', () => {
  it('Rust tarafindaki dokuz token hata etiketini birebir kapsar', () => {
    expect([...REALTIME_TOKEN_ERROR_KINDS]).toStrictEqual([
      'missing_api_key',
      'invalid_api_key',
      'model_access_denied',
      'quota_exceeded',
      'network',
      'upstream_unavailable',
      'unexpected_status',
      'malformed_response',
      'http_client_unavailable',
    ]);
  });

  it('her kind icin bos olmayan bir mesaj var', () => {
    for (const kind of ASUNA_ERROR_KINDS) {
      const resolved = userFacingErrorFor(kind);

      expect(resolved.kind).toBe(kind);
      expect(resolved.message.length).toBeGreaterThan(10);
      expect(resolved.message.endsWith('.')).toBe(true);
    }
  });

  it('mesajlar birbirinden farkli (tek "bir seyler ters gitti" kovasi yok)', () => {
    const messages = ASUNA_ERROR_KINDS.map((kind) => userFacingErrorFor(kind).message);

    expect(new Set(messages).size).toBe(messages.length);
  });

  it('etiket kumesi tekrarsiz', () => {
    expect(new Set(ASUNA_ERROR_KINDS).size).toBe(ASUNA_ERROR_KINDS.length);
  });
});

describe('mesaj icerigi', () => {
  it('hicbir mesajda secret gorunumlu deger yok', () => {
    for (const kind of ASUNA_ERROR_KINDS) {
      const text = describeUserFacingError(userFacingErrorFor(kind));

      expect(text).not.toMatch(/sk-[A-Za-z0-9]/);
      expect(text).not.toMatch(/ek_[A-Za-z0-9]/);
      expect(text).not.toMatch(/Bearer\s/i);
      // Redaksiyon suzgecinden gecirmek metni degistirmiyorsa maskelenecek bir sey yoktur.
      expect(redactText(text)).toBe(text);
    }
  });

  it('mesajlar ic detay (stack, dosya yolu, HTTP govdesi) sizdirmaz', () => {
    for (const kind of ASUNA_ERROR_KINDS) {
      const text = describeUserFacingError(userFacingErrorFor(kind));

      expect(text).not.toContain('http://');
      expect(text).not.toContain('https://');
      expect(text).not.toMatch(/\bat .+:\d+:\d+/);
    }
  });

  it('baglanti kurulamadiginda basari taklidi yapmaz, durust cumle kurar', () => {
    expect(userFacingErrorFor('network').message).toContain('Şu an ses bağlantısını kuramadım');
    expect(userFacingErrorFor('upstream_unavailable').message).toContain(
      'Şu an ses bağlantısını kuramadım',
    );
  });

  it('anahtar hatalarinda tekrar denemek onerilmez, ag hatalarinda onerilir', () => {
    expect(userFacingErrorFor('missing_api_key').retryable).toBe(false);
    expect(userFacingErrorFor('invalid_api_key').retryable).toBe(false);
    expect(userFacingErrorFor('model_access_denied').retryable).toBe(false);
    expect(userFacingErrorFor('network').retryable).toBe(true);
    expect(userFacingErrorFor('realtime_disconnected').retryable).toBe(true);
  });

  it('mikrofon hatasi kurulum yonlendirmesi iceriyor (PROJECT.md Bolum 30)', () => {
    const resolved = userFacingErrorFor('mic_permission_denied');

    expect(resolved.action).toContain('Sistem Ayarları');
  });

  it('memory hatasi konusmayi bitirmez, durumu bildirir', () => {
    expect(userFacingErrorFor('memory_unavailable').message).toContain('devam edebilirim');
  });

  it('tool hatasi yapmis gibi davranmaz', () => {
    expect(userFacingErrorFor('tool_failed').message).toContain('yapmış gibi davranmayacağım');
  });
});

describe('ham hatadan cozumleme', () => {
  it("IPC payload'indaki `kind` alanini okur", () => {
    const payload = { kind: 'quota_exceeded', message: 'OpenAI kota sinirina takildi.' };

    expect(errorKindOf(payload)).toBe('quota_exceeded');
    expect(toUserFacingError(payload).message).toContain('kota sınırına takıldım');
  });

  it("dogrudan etiket string'ini kabul eder", () => {
    expect(errorKindOf('mic_unavailable')).toBe('mic_unavailable');
  });

  it('bilinmeyen kind jenerik ama durust mesaja duser', () => {
    const resolved = toUserFacingError({ kind: 'ay_yildiz_patladi', message: 'x' });

    expect(resolved.kind).toBe(UNKNOWN_ERROR_KIND);
    expect(resolved.message).toContain('çözemedim');
  });

  it('tanimsiz / bicimsiz girdide de mesaj uretir', () => {
    for (const input of [null, undefined, 42, [], new Error('patladi'), { message: 'x' }]) {
      expect(toUserFacingError(input).kind).toBe(UNKNOWN_ERROR_KIND);
    }
  });

  it('upstream mesaji kullaniciya oldugu gibi tasinmaz', () => {
    const payload = {
      kind: 'network',
      message: 'OpenAI POST https://api.openai.com/v1/realtime/client_secrets basarisiz',
    };

    expect(describeUserFacingError(toUserFacingError(payload))).not.toContain('api.openai.com');
  });

  it('isAsunaErrorKind bilinmeyen etiketi reddeder', () => {
    expect(isAsunaErrorKind('network')).toBe(true);
    expect(isAsunaErrorKind('nope')).toBe(false);
  });
});

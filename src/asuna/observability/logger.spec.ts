import { describe, expect, it, vi } from 'vitest';

import {
  AsunaLogger,
  DEFAULT_LOG_BUFFER_CAPACITY,
  LogRingBuffer,
  REDACTED,
  formatClockTime,
  formatLogEntry,
  isLevelEnabledFor,
  isSensitiveKey,
  redactData,
  redactText,
  redactValue,
  type LogEntry,
  type LogLevel,
} from './logger';

const FIXED_NOW = new Date('2026-08-24T12:10:01.000Z');

/** Gercek anahtar degil: `sk-` prefix'ini tetikleyen sentetik dizge. */
const FAKE_PERMANENT_KEY = 'sk-proj-TESTONLY0123456789abcdef';
/** Gercek token degil: `ek_` prefix'ini tetikleyen sentetik dizge. */
const FAKE_EPHEMERAL_TOKEN = 'ek_TESTONLY0123456789abcdef';

function setup(
  level: LogLevel = 'debug',
  capacity = 20,
): {
  logger: AsunaLogger;
  buffer: LogRingBuffer;
  sink: LogEntry[];
} {
  const buffer = new LogRingBuffer(capacity);
  const sink: LogEntry[] = [];
  const logger = new AsunaLogger({
    level,
    scope: 'test',
    buffer,
    sinks: [
      (entry): void => {
        sink.push(entry);
      },
    ],
    now: () => FIXED_NOW,
  });
  return { logger, buffer, sink };
}

describe('seviye filtreleme (ASUNA_LOG_LEVEL)', () => {
  it('esikten daha ayrintili satirlari elemez sekilde siralanmis', () => {
    expect(isLevelEnabledFor('error', 'error')).toBe(true);
    expect(isLevelEnabledFor('warn', 'error')).toBe(false);
    expect(isLevelEnabledFor('debug', 'info')).toBe(false);
    expect(isLevelEnabledFor('warn', 'debug')).toBe(true);
  });

  it('`info` seviyesinde debug satiri tampona hic girmez', () => {
    const { logger, buffer } = setup('info');

    logger.debug('gizli ayrinti');
    logger.info('gorunur');
    logger.warn('uyari');
    logger.error('hata');

    expect(buffer.getSnapshot().map((entry) => entry.message)).toStrictEqual([
      'gorunur',
      'uyari',
      'hata',
    ]);
  });

  it('`error` seviyesinde yalnizca error gecer', () => {
    const { logger, buffer } = setup('error');

    logger.debug('a');
    logger.info('b');
    logger.warn('c');
    logger.error('d');

    expect(buffer.getSnapshot()).toHaveLength(1);
    expect(buffer.getSnapshot()[0]?.level).toBe('error');
  });

  it('seviye calisma aninda degistirilebilir (config gec yuklenir)', () => {
    const { logger, buffer } = setup('error');

    logger.info('once elenir');
    logger.setLevel('debug');
    logger.debug('sonra gecer');

    expect(buffer.getSnapshot().map((entry) => entry.message)).toStrictEqual(['sonra gecer']);
    expect(logger.getLevel()).toBe('debug');
  });

  it('child logger scope degistirir, seviyeyi ve tamponu paylasir', () => {
    const { logger, buffer } = setup('debug');
    const child = logger.child('voice-state');

    child.debug('cocuk satiri');
    logger.setLevel('error');
    child.debug('artik elenir');

    const entries = buffer.getSnapshot();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.scope).toBe('voice-state');
    expect(child.getLevel()).toBe('error');
  });

  it('sink ve tampon ayni kaydi alir', () => {
    const { logger, buffer, sink } = setup();

    logger.warn('paylasilan satir');

    expect(sink).toHaveLength(1);
    expect(sink[0]).toStrictEqual(buffer.getSnapshot()[0]);
  });
});

describe('redaksiyon — secret degerleri', () => {
  it("`sk-` prefix'li degeri maskeler", () => {
    expect(redactText(`key=${FAKE_PERMANENT_KEY}`)).toBe(`key=sk-${REDACTED}`);
  });

  it("`ek_` prefix'li ephemeral token'i maskeler", () => {
    expect(redactText(`token: ${FAKE_EPHEMERAL_TOKEN}`)).toBe(`token: ek_${REDACTED}`);
  });

  it("JSON gurultusu icindeki token'i da yakalar", () => {
    const line = `{"value":"${FAKE_EPHEMERAL_TOKEN}","model":"gpt-realtime-2.1"}`;

    const redacted = redactText(line);

    expect(redacted).toBe(`{"value":"ek_${REDACTED}","model":"gpt-realtime-2.1"}`);
    expect(redacted).not.toContain('TESTONLY');
  });

  it('secret gorunmeyen metni degistirmez', () => {
    const line = 'CONNECTING -> LISTENING (REALTIME_CONNECTED) model=gpt-realtime-2.1-mini';

    expect(redactText(line)).toBe(line);
  });

  it("log mesajindaki anahtar sink'e ulasmadan maskelenir", () => {
    const { logger, sink } = setup();

    logger.error(`token mint basarisiz: ${FAKE_PERMANENT_KEY}`);

    expect(sink[0]?.message).toBe(`token mint basarisiz: sk-${REDACTED}`);
    expect(sink[0]?.message).not.toContain('TESTONLY');
  });
});

describe('redaksiyon — hassas alan adlari', () => {
  it('apiKey / token / value alanlarini tanir', () => {
    expect(isSensitiveKey('apiKey')).toBe(true);
    expect(isSensitiveKey('api_key')).toBe(true);
    expect(isSensitiveKey('API-KEY')).toBe(true);
    expect(isSensitiveKey('token')).toBe(true);
    expect(isSensitiveKey('value')).toBe(true);
    expect(isSensitiveKey('clientSecret')).toBe(true);
    expect(isSensitiveKey('Authorization')).toBe(true);
  });

  it('zararsiz alan adlarini maskelemez', () => {
    expect(isSensitiveKey('tokenCount')).toBe(false);
    expect(isSensitiveKey('model')).toBe(false);
    expect(isSensitiveKey('expiresAt')).toBe(false);
    expect(isSensitiveKey('reason')).toBe(false);
  });

  it('hassas alan degerini icerigine bakmadan maskeler', () => {
    const redacted = redactData({ apiKey: 'duz-metin-de-olsa', model: 'gpt-realtime-2.1' });

    expect(redacted).toStrictEqual({ apiKey: REDACTED, model: 'gpt-realtime-2.1' });
  });

  it('ic ice nesnelerde de maskeler (EphemeralToken sekli)', () => {
    const { logger, sink } = setup();

    logger.info('token alindi', {
      token: {
        value: FAKE_EPHEMERAL_TOKEN,
        expiresAt: 1_800_000_000,
        model: 'gpt-realtime-2.1',
      },
      tokenCount: 3,
    });

    expect(sink[0]?.data).toStrictEqual({ token: REDACTED, tokenCount: 3 });
  });

  it("nesne hassas ad tasimasa bile deger prefix'i yakalanir", () => {
    expect(redactData({ note: `bearer ${FAKE_EPHEMERAL_TOKEN}` })).toStrictEqual({
      note: `bearer ek_${REDACTED}`,
    });
  });

  it('dizi elemanlarini redakte eder', () => {
    expect(redactValue([FAKE_PERMANENT_KEY, 'guvenli'])).toStrictEqual([
      `sk-${REDACTED}`,
      'guvenli',
    ]);
  });

  it('Error nesnesini stack sizdirmadan redakte eder', () => {
    const redacted = redactValue(new Error(`istek reddedildi (${FAKE_PERMANENT_KEY})`));

    expect(redacted).toStrictEqual({
      name: 'Error',
      message: `istek reddedildi (sk-${REDACTED})`,
    });
  });

  it('dongusel referansta patlamaz', () => {
    const node: Record<string, unknown> = { name: 'root' };
    node['self'] = node;

    expect(redactValue(node)).toStrictEqual({ name: 'root', self: '<circular>' });
  });

  it('tamponun tamami tarandiginda hicbir satirda ham secret yok', () => {
    const { logger, buffer } = setup();

    logger.debug(`ham: ${FAKE_PERMANENT_KEY}`, { apiKey: FAKE_PERMANENT_KEY });
    logger.info('oturum', { session: { value: FAKE_EPHEMERAL_TOKEN } });

    const dump = buffer.getSnapshot().map(formatLogEntry).join('\n');

    expect(dump).not.toContain('TESTONLY');
    expect(dump).not.toMatch(/sk-[A-Za-z0-9]/);
    expect(dump).not.toMatch(/ek_[A-Za-z0-9]/);
  });
});

describe('LogRingBuffer', () => {
  it('kapasiteyi asinca en eski satiri dusurur', () => {
    const { logger, buffer } = setup('debug', 3);

    for (const index of [1, 2, 3, 4, 5]) {
      logger.info(`satir-${index.toString()}`);
    }

    expect(buffer.size).toBe(3);
    expect(buffer.getSnapshot().map((entry) => entry.message)).toStrictEqual([
      'satir-3',
      'satir-4',
      'satir-5',
    ]);
  });

  it('varsayilan kapasite 500 satirda sabit kalir', () => {
    const buffer = new LogRingBuffer();
    const entry: LogEntry = {
      level: 'info',
      at: FIXED_NOW.toISOString(),
      scope: 'test',
      message: 'x',
      data: null,
    };

    for (let index = 0; index < DEFAULT_LOG_BUFFER_CAPACITY + 120; index += 1) {
      buffer.push({ ...entry, message: `satir-${index.toString()}` });
    }

    expect(DEFAULT_LOG_BUFFER_CAPACITY).toBe(500);
    expect(buffer.size).toBe(DEFAULT_LOG_BUFFER_CAPACITY);
    expect(buffer.getSnapshot()[0]?.message).toBe('satir-120');
  });

  it('gecersiz kapasiteyi reddeder', () => {
    expect(() => new LogRingBuffer(0)).toThrow(RangeError);
    expect(() => new LogRingBuffer(1.5)).toThrow(RangeError);
  });

  it('goruntu referansi yeni satira kadar sabit kalir (useSyncExternalStore sozlesmesi)', () => {
    const { logger, buffer } = setup();

    const first = buffer.getSnapshot();
    expect(buffer.getSnapshot()).toBe(first);

    logger.info('yeni satir');
    expect(buffer.getSnapshot()).not.toBe(first);
  });

  it('aboneleri her satirda ve temizlemede bilgilendirir', () => {
    const { logger, buffer } = setup();
    const listener = vi.fn();
    const unsubscribe = buffer.subscribe(listener);

    logger.info('bir');
    buffer.clear();
    expect(listener).toHaveBeenCalledTimes(2);
    expect(buffer.size).toBe(0);

    unsubscribe();
    logger.info('iki');
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it('bozuk bir abone log zincirini dusurmez', () => {
    const { logger, buffer } = setup();
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    buffer.subscribe(() => {
      throw new Error('panel patladi');
    });

    expect(() => {
      logger.info('yine de yazilir');
    }).not.toThrow();
    expect(buffer.size).toBe(1);
    expect(consoleError).toHaveBeenCalledTimes(1);

    consoleError.mockRestore();
  });
});

describe('satir bicimi', () => {
  it('ISO damgasindan HH:MM:SS uretir', () => {
    expect(formatClockTime('2026-08-24T12:10:01.000Z')).toBe('12:10:01');
    expect(formatClockTime('bozuk')).toBe('bozuk');
  });

  it('zaman damgasi + seviye + scope + mesaj iceriyor', () => {
    const { logger, buffer } = setup();

    logger.info('CONNECTING -> LISTENING (REALTIME_CONNECTED)', { attempt: 1 });

    expect(formatLogEntry(buffer.getSnapshot()[0]!)).toBe(
      '12:10:01 INFO  [test] CONNECTING -> LISTENING (REALTIME_CONNECTED) {"attempt":1}',
    );
  });

  it('veri yoksa sadece mesaji basar', () => {
    const { logger, buffer } = setup();

    logger.error('baglanti koptu');

    expect(formatLogEntry(buffer.getSnapshot()[0]!)).toBe(
      '12:10:01 ERROR [test] baglanti koptu',
    );
  });
});

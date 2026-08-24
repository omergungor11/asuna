import { describe, expect, it } from 'vitest';

import { VoiceStateMachine } from '../state/voice-state-machine';

import { AsunaLogger, LogRingBuffer, formatLogEntry } from './logger';
import {
  VOICE_STATE_LOG_SCOPE,
  attachVoiceStateLogger,
  createLoggedVoiceStateMachine,
  createVoiceStateLogger,
  formatInvalidTransitionLine,
  formatStateTransitionLine,
} from './state-logger';

const FIXED_NOW = new Date('2026-08-24T12:10:01.000Z');

function setup(): { logger: AsunaLogger; buffer: LogRingBuffer } {
  const buffer = new LogRingBuffer(50);
  const logger = new AsunaLogger({
    level: 'debug',
    scope: 'asuna',
    buffer,
    now: () => FIXED_NOW,
  });
  return { logger, buffer };
}

describe('gecis satiri bicimi (PROJECT.md Bolum 29)', () => {
  it('`HH:MM:SS FROM -> TO (REASON)` uretir', () => {
    expect(
      formatStateTransitionLine({
        from: 'WAKING',
        to: 'CONNECTING',
        reason: 'ACTIVATION_REQUESTED',
        at: '2026-08-24T12:10:01.000Z',
      }),
    ).toBe('12:10:01 WAKING -> CONNECTING (ACTIVATION_REQUESTED)');
  });

  it('reddedilen gecisi ayirt edilebilir sekilde yazar', () => {
    expect(
      formatInvalidTransitionLine({
        from: 'LISTENING',
        attempted: 'CONNECTING',
        reason: 'REALTIME_CONNECTING',
        at: '2026-08-24T12:10:02.000Z',
      }),
    ).toBe('12:10:02 INVALID_TRANSITION LISTENING -x-> CONNECTING (REALTIME_CONNECTING)');
  });
});

describe('state machine baglantisi', () => {
  it("her gecerli gecisi `voice-state` scope'unda info olarak loglar", () => {
    const { logger, buffer } = setup();
    const machine = new VoiceStateMachine({ initialState: 'BOOTING', now: () => FIXED_NOW });
    attachVoiceStateLogger(machine, logger);

    machine.transition('WAKING', 'ACTIVATION_REQUESTED');
    machine.transition('CONNECTING', 'REALTIME_CONNECTING');
    machine.transition('LISTENING', 'REALTIME_CONNECTED');

    expect(buffer.getSnapshot().map(formatLogEntry)).toStrictEqual([
      '12:10:01 INFO  [voice-state] BOOTING -> WAKING (ACTIVATION_REQUESTED) ' +
        '{"from":"BOOTING","to":"WAKING","reason":"ACTIVATION_REQUESTED","at":"2026-08-24T12:10:01.000Z"}',
      '12:10:01 INFO  [voice-state] WAKING -> CONNECTING (REALTIME_CONNECTING) ' +
        '{"from":"WAKING","to":"CONNECTING","reason":"REALTIME_CONNECTING","at":"2026-08-24T12:10:01.000Z"}',
      '12:10:01 INFO  [voice-state] CONNECTING -> LISTENING (REALTIME_CONNECTED) ' +
        '{"from":"CONNECTING","to":"LISTENING","reason":"REALTIME_CONNECTED","at":"2026-08-24T12:10:01.000Z"}',
    ]);
    expect(buffer.getSnapshot()[0]?.scope).toBe(VOICE_STATE_LOG_SCOPE);
  });

  it('ERROR durumuna dususu `warn` seviyesine yukseltir', () => {
    const { logger, buffer } = setup();
    const machine = new VoiceStateMachine({ initialState: 'CONNECTING', now: () => FIXED_NOW });
    attachVoiceStateLogger(machine, logger);

    machine.transition('ERROR', 'ERROR_OCCURRED');

    expect(buffer.getSnapshot()[0]?.level).toBe('warn');
    expect(buffer.getSnapshot()[0]?.message).toBe('CONNECTING -> ERROR (ERROR_OCCURRED)');
  });

  it('abonelik geri alinabilir', () => {
    const { logger, buffer } = setup();
    const machine = new VoiceStateMachine({ initialState: 'BOOTING', now: () => FIXED_NOW });
    const unsubscribe = attachVoiceStateLogger(machine, logger);

    machine.transition('WAKING', 'ACTIVATION_REQUESTED');
    unsubscribe();
    machine.transition('CONNECTING', 'REALTIME_CONNECTING');

    expect(buffer.size).toBe(1);
  });

  it('reddedilen gecisi `error` seviyesinde loglar (sessiz yutma yok)', () => {
    const { logger, buffer } = setup();
    const hooks = createVoiceStateLogger(logger);
    const machine = new VoiceStateMachine({
      initialState: 'LISTENING',
      invalidTransitionPolicy: 'reject',
      onInvalidTransition: hooks.onInvalidTransition,
      now: () => FIXED_NOW,
    });
    machine.subscribe(hooks.onTransition);

    const applied = machine.transition('CONNECTING', 'REALTIME_CONNECTING');

    expect(applied).toBe(false);
    expect(machine.getState()).toBe('LISTENING');
    expect(buffer.getSnapshot()[0]?.level).toBe('error');
    expect(formatLogEntry(buffer.getSnapshot()[0]!)).toContain(
      'INVALID_TRANSITION LISTENING -x-> CONNECTING (REALTIME_CONNECTING)',
    );
  });
});

describe('createLoggedVoiceStateMachine', () => {
  it('gecerli ve gecersiz gecisleri tek cagrida baglar', () => {
    const { logger, buffer } = setup();
    const machine = createLoggedVoiceStateMachine({
      logger,
      initialState: 'BOOTING',
      invalidTransitionPolicy: 'reject',
      now: () => FIXED_NOW,
    });

    machine.transition('WAKING', 'ACTIVATION_REQUESTED');
    machine.transition('LISTENING', 'REALTIME_CONNECTED');

    const entries = buffer.getSnapshot();
    expect(entries.map((entry) => entry.level)).toStrictEqual(['info', 'error']);
    expect(machine.getState()).toBe('WAKING');
  });

  it('gecis verisinde yalnizca durum/neden alanlari var (secret tasimaz)', () => {
    const { logger, buffer } = setup();
    const machine = createLoggedVoiceStateMachine({
      logger,
      initialState: 'BOOTING',
      now: () => FIXED_NOW,
    });

    machine.transition('WAKING', 'ACTIVATION_REQUESTED');

    expect(Object.keys(buffer.getSnapshot()[0]?.data ?? {})).toStrictEqual([
      'from',
      'to',
      'reason',
      'at',
    ]);
  });
});

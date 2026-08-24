import { describe, expect, it, vi } from 'vitest';

import {
  InvalidVoiceTransitionError,
  VOICE_STATES,
  VOICE_STATE_TRANSITIONS,
  VoiceStateMachine,
  type InvalidVoiceTransition,
  type VoiceState,
  type VoiceStateTransition,
} from './voice-state-machine';

const FIXED_NOW = new Date('2026-08-24T10:00:00.000Z');

function machine(
  initialState: VoiceState,
  options: { readonly policy?: 'throw' | 'reject' } = {},
): VoiceStateMachine {
  return new VoiceStateMachine({
    initialState,
    invalidTransitionPolicy: options.policy ?? 'throw',
    now: () => FIXED_NOW,
  });
}

describe('VOICE_STATE_TRANSITIONS tablosu', () => {
  it('11 kanonik durumu iceriyor', () => {
    expect(VOICE_STATES).toHaveLength(11);
    expect([...VOICE_STATES]).toStrictEqual([
      'BOOTING',
      'IDLE_WAKE_WORD',
      'WAKING',
      'CONNECTING',
      'LISTENING',
      'USER_SPEAKING',
      'ASSISTANT_THINKING',
      'ASSISTANT_SPEAKING',
      'TOOL_PENDING',
      'AWAITING_APPROVAL',
      'ERROR',
    ]);
  });

  it('her durum icin gecis listesi tanimli ve kendine gecis yok', () => {
    for (const state of VOICE_STATES) {
      const targets = VOICE_STATE_TRANSITIONS[state];
      expect(targets.length).toBeGreaterThan(0);
      expect(targets).not.toContain(state);
      expect(new Set(targets).size).toBe(targets.length);
    }
  });

  it('ERROR terminal degil ve her oturum durumu ERROR ile hataya dusebilir', () => {
    expect(VOICE_STATE_TRANSITIONS.ERROR.length).toBeGreaterThan(0);
    for (const state of VOICE_STATES) {
      if (state === 'ERROR') continue;
      expect(VOICE_STATE_TRANSITIONS[state]).toContain('ERROR');
    }
  });

  it('her cikmaz yol IDLE_WAKE_WORD e donebilir (BOOTING ve kendisi disinda)', () => {
    for (const state of VOICE_STATES) {
      if (state === 'BOOTING' || state === 'IDLE_WAKE_WORD') continue;
      expect(VOICE_STATE_TRANSITIONS[state]).toContain('IDLE_WAKE_WORD');
    }
  });
});

describe('VoiceStateMachine — gecerli gecisler', () => {
  it('varsayilan baslangic durumu BOOTING', () => {
    expect(new VoiceStateMachine({ invalidTransitionPolicy: 'reject' }).getState()).toBe(
      'BOOTING',
    );
  });

  // Tablodaki HER kenar tek tek uygulanir: gecis kabul edilmeli ve durum degismeli.
  for (const from of VOICE_STATES) {
    for (const to of VOICE_STATE_TRANSITIONS[from]) {
      it(`${from} -> ${to} kabul edilir`, () => {
        const sut = machine(from);
        const seen: VoiceStateTransition[] = [];
        sut.subscribe((transition) => seen.push(transition));

        expect(sut.canTransition(to)).toBe(true);
        expect(sut.transition(to, 'ERROR_OCCURRED')).toBe(true);
        expect(sut.getState()).toBe(to);
        expect(seen).toStrictEqual([
          { from, to, reason: 'ERROR_OCCURRED', at: FIXED_NOW.toISOString() },
        ]);
      });
    }
  }

  it('Phase 1 tam akisi: BOOTING -> ... -> LISTENING -> BOOTING', () => {
    const sut = machine('BOOTING');
    const path: VoiceState[] = [];
    sut.subscribe((transition) => path.push(transition.to));

    expect(sut.transition('WAKING', 'ACTIVATION_REQUESTED')).toBe(true);
    expect(sut.transition('CONNECTING', 'MIC_PERMISSION_GRANTED')).toBe(true);
    expect(sut.transition('LISTENING', 'REALTIME_CONNECTED')).toBe(true);
    expect(sut.transition('USER_SPEAKING', 'USER_SPEECH_STARTED')).toBe(true);
    expect(sut.transition('ASSISTANT_THINKING', 'USER_SPEECH_ENDED')).toBe(true);
    expect(sut.transition('ASSISTANT_SPEAKING', 'ASSISTANT_AUDIO_STARTED')).toBe(true);
    // Barge-in: Asuna konusurken kullanici sozu kesiyor (ASU-016).
    expect(sut.transition('USER_SPEAKING', 'USER_INTERRUPTED')).toBe(true);
    expect(sut.transition('LISTENING', 'USER_SPEECH_ENDED')).toBe(true);
    // Temiz kapanis (ASU-018) — Phase 1'de wake word yok, BOOTING'e donuluyor.
    expect(sut.transition('BOOTING', 'SESSION_CLOSED_BY_USER')).toBe(true);

    expect(path).toStrictEqual([
      'WAKING',
      'CONNECTING',
      'LISTENING',
      'USER_SPEAKING',
      'ASSISTANT_THINKING',
      'ASSISTANT_SPEAKING',
      'USER_SPEAKING',
      'LISTENING',
      'BOOTING',
    ]);
    expect(sut.getState()).toBe('BOOTING');
  });

  it('ERROR durumundan yeniden baglanma yolu var (ASU-019)', () => {
    const sut = machine('CONNECTING');
    expect(sut.transition('ERROR', 'ERROR_OCCURRED')).toBe(true);
    expect(sut.transition('CONNECTING', 'ERROR_RECOVERED')).toBe(true);
    expect(sut.getState()).toBe('CONNECTING');
  });
});

describe('VoiceStateMachine — gecersiz gecisler', () => {
  // Gecersiz kenarlarin bir kismi: sirasiyla "oturum yok ama dinliyor",
  // "baglanmadan konusuyor" ve "Phase 5 tool'u dogrudan idle'dan calisiyor".
  const invalidEdges: readonly (readonly [VoiceState, VoiceState])[] = [
    ['BOOTING', 'LISTENING'],
    ['WAKING', 'ASSISTANT_SPEAKING'],
    ['IDLE_WAKE_WORD', 'TOOL_PENDING'],
    ['LISTENING', 'AWAITING_APPROVAL'],
    ['CONNECTING', 'USER_SPEAKING'],
  ];

  for (const [from, to] of invalidEdges) {
    it(`${from} -> ${to} reddedilir (throw politikasi)`, () => {
      const sut = machine(from);
      const listener = vi.fn();
      sut.subscribe(listener);

      expect(sut.canTransition(to)).toBe(false);
      expect(() => sut.transition(to, 'ERROR_OCCURRED')).toThrow(InvalidVoiceTransitionError);
      // Durum degismedi, hicbir sey yayinlanmadi.
      expect(sut.getState()).toBe(from);
      expect(listener).not.toHaveBeenCalled();
    });

    it(`${from} -> ${to} reddedilir (reject politikasi: log + durum korunur)`, () => {
      const invalid: InvalidVoiceTransition[] = [];
      const sut = new VoiceStateMachine({
        initialState: from,
        invalidTransitionPolicy: 'reject',
        onInvalidTransition: (details) => invalid.push(details),
        now: () => FIXED_NOW,
      });
      const listener = vi.fn();
      sut.subscribe(listener);

      expect(sut.transition(to, 'ERROR_OCCURRED')).toBe(false);
      expect(sut.getState()).toBe(from);
      expect(listener).not.toHaveBeenCalled();
      expect(invalid).toStrictEqual([
        { from, attempted: to, reason: 'ERROR_OCCURRED', at: FIXED_NOW.toISOString() },
      ]);
    });
  }

  it('ayni duruma gecis (no-op) de gecersizdir — sahte gecis loglanmaz', () => {
    const sut = machine('LISTENING');
    expect(() => sut.transition('LISTENING', 'REALTIME_CONNECTED')).toThrow(
      InvalidVoiceTransitionError,
    );
    expect(sut.getState()).toBe('LISTENING');
  });

  it('hata nesnesi reddedilen gecisi tasiyor', () => {
    const sut = machine('BOOTING');
    try {
      sut.transition('LISTENING', 'REALTIME_CONNECTED');
      expect.unreachable('gecis reddedilmeliydi');
    } catch (error) {
      expect(error).toBeInstanceOf(InvalidVoiceTransitionError);
      const typed = error as InvalidVoiceTransitionError;
      expect(typed.name).toBe('InvalidVoiceTransitionError');
      expect(typed.transition).toStrictEqual({
        from: 'BOOTING',
        attempted: 'LISTENING',
        reason: 'REALTIME_CONNECTED',
        at: FIXED_NOW.toISOString(),
      });
      // Hata mesaji durum adlarini tasir, ic detay/secret tasimaz.
      expect(typed.message).toContain('BOOTING -> LISTENING');
    }
  });
});

describe('VoiceStateMachine — subscribe/unsubscribe', () => {
  it('birden fazla abone ayni gecisi alir', () => {
    const sut = machine('BOOTING');
    const first = vi.fn();
    const second = vi.fn();
    sut.subscribe(first);
    sut.subscribe(second);

    sut.transition('WAKING', 'ACTIVATION_REQUESTED');

    const expected: VoiceStateTransition = {
      from: 'BOOTING',
      to: 'WAKING',
      reason: 'ACTIVATION_REQUESTED',
      at: FIXED_NOW.toISOString(),
    };
    expect(first).toHaveBeenCalledExactlyOnceWith(expected);
    expect(second).toHaveBeenCalledExactlyOnceWith(expected);
  });

  it('unsubscribe sonrasi abone cagirilmaz', () => {
    const sut = machine('BOOTING');
    const listener = vi.fn();
    const unsubscribe = sut.subscribe(listener);

    sut.transition('WAKING', 'ACTIVATION_REQUESTED');
    unsubscribe();
    sut.transition('CONNECTING', 'MIC_PERMISSION_GRANTED');

    expect(listener).toHaveBeenCalledOnce();
    expect(sut.getState()).toBe('CONNECTING');
  });

  it('unsubscribe idempotent ve ayni abone iki kez eklenmez', () => {
    const sut = machine('BOOTING');
    const listener = vi.fn();
    const unsubscribe = sut.subscribe(listener);
    sut.subscribe(listener);

    sut.transition('WAKING', 'ACTIVATION_REQUESTED');
    expect(listener).toHaveBeenCalledOnce();

    unsubscribe();
    unsubscribe();
    sut.transition('CONNECTING', 'MIC_PERMISSION_GRANTED');
    expect(listener).toHaveBeenCalledOnce();
  });

  it('bir abonenin hatasi digerlerini engellemez, sessizce de yutulmaz', () => {
    const sut = machine('BOOTING');
    const failing = vi.fn(() => {
      throw new Error('log paneli patladi');
    });
    const healthy = vi.fn();
    sut.subscribe(failing);
    sut.subscribe(healthy);

    expect(() => sut.transition('WAKING', 'ACTIVATION_REQUESTED')).toThrow(AggregateError);
    // Durum gecisi uygulanmis ve saglikli abone bilgilendirilmis olmali.
    expect(sut.getState()).toBe('WAKING');
    expect(healthy).toHaveBeenCalledOnce();
  });

  it('gecis sirasinda eklenen abone o gecisi almaz (snapshot uzerinde yayin)', () => {
    const sut = machine('BOOTING');
    const late = vi.fn();
    sut.subscribe(() => {
      sut.subscribe(late);
    });

    sut.transition('WAKING', 'ACTIVATION_REQUESTED');
    expect(late).not.toHaveBeenCalled();

    sut.transition('CONNECTING', 'MIC_PERMISSION_GRANTED');
    expect(late).toHaveBeenCalledOnce();
  });
});

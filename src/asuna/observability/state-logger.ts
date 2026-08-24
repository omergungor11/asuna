/**
 * Voice state gecislerinin log'a baglanmasi (ASU-019).
 *
 * Kaynak: `PROJECT.md` Bolum 29 (ornek transcript: `12:10:01 WAKE_WORD_DETECTED`),
 * `asuna-config/conventions.md` — "Her gecis loglanir (SCREAMING_SNAKE event adi)".
 *
 * Tasarim kararlari:
 * - **Tek publish noktasi kullanilir.** Log, `VoiceStateMachine.subscribe`
 *   uzerinden beslenir; bilesenlerin ayrica "ben de logladim" demesi gerekmez
 *   (ASU-014 notu: "ASU-019 loglamasi buna baglanir").
 * - **Reddedilen gecis sessiz degil.** `onInvalidTransition` `error` seviyesinde
 *   loglanir: prod'da durum korunur ama olay kaybolmaz (PROJECT.md Bolum 30).
 * - **Gecis verisi secret tasimaz** — yalnizca durum adlari ve neden etiketi.
 *   Yine de logger'in redaksiyon suzgecinden gecer (savunma katmani).
 */

import {
  VoiceStateMachine,
  type InvalidVoiceTransition,
  type VoiceStateMachineOptions,
  type VoiceStateTransition,
  type VoiceTransitionListener,
} from '../state/voice-state-machine';

import { formatClockTime, logger as defaultLogger, type AsunaLogger } from './logger';

/** Gecis log'larinin `scope` etiketi. */
export const VOICE_STATE_LOG_SCOPE = 'voice-state';

/**
 * PROJECT.md Bolum 29 bicimi: `12:10:01 WAKING -> CONNECTING (ACTIVATION_REQUESTED)`.
 *
 * Saat, gecisin kendi `at` damgasindan (UTC) turetilir — makineler arasi
 * tekrarlanabilir olsun diye yerel saat dilimi kullanilmaz.
 */
export function formatStateTransitionLine(transition: VoiceStateTransition): string {
  return `${formatClockTime(transition.at)} ${formatTransitionBody(transition)}`;
}

/** Reddedilen gecisin tek satirlik bicimi: `12:10:01 INVALID_TRANSITION A -x-> B (REASON)`. */
export function formatInvalidTransitionLine(transition: InvalidVoiceTransition): string {
  return `${formatClockTime(transition.at)} ${formatInvalidTransitionBody(transition)}`;
}

function formatTransitionBody(transition: VoiceStateTransition): string {
  return `${transition.from} -> ${transition.to} (${transition.reason})`;
}

function formatInvalidTransitionBody(transition: InvalidVoiceTransition): string {
  return (
    `INVALID_TRANSITION ${transition.from} -x-> ${transition.attempted} ` +
    `(${transition.reason})`
  );
}

/** State machine'e baglanacak iki gozlemci. */
export interface VoiceStateLoggerHooks {
  /** `machine.subscribe(...)` ile baglanir. */
  readonly onTransition: VoiceTransitionListener;
  /** `new VoiceStateMachine({ onInvalidTransition })` ile baglanir. */
  readonly onInvalidTransition: (transition: InvalidVoiceTransition) => void;
}

/**
 * Gecis gozlemcilerini uretir.
 *
 * `onInvalidTransition` constructor secenegi oldugu icin ikisi ayri ayri
 * doner — hazir kablolama icin [`createLoggedVoiceStateMachine`].
 */
export function createVoiceStateLogger(
  target: AsunaLogger = defaultLogger,
): VoiceStateLoggerHooks {
  const scoped = target.child(VOICE_STATE_LOG_SCOPE);

  return {
    onTransition: (transition: VoiceStateTransition): void => {
      const data = {
        from: transition.from,
        to: transition.to,
        reason: transition.reason,
        at: transition.at,
      };
      // ERROR'a dusmek normal bir akis adimi degil: `info` gurultusunde kaybolmasin.
      if (transition.to === 'ERROR') {
        scoped.warn(formatTransitionBody(transition), data);
        return;
      }
      scoped.info(formatTransitionBody(transition), data);
    },

    onInvalidTransition: (transition: InvalidVoiceTransition): void => {
      scoped.error(formatInvalidTransitionBody(transition), {
        from: transition.from,
        attempted: transition.attempted,
        reason: transition.reason,
        at: transition.at,
      });
    },
  };
}

/**
 * Var olan bir makineye yalnizca gecerli gecis log'unu takar.
 *
 * @returns aboneligi kaldiran fonksiyon.
 */
export function attachVoiceStateLogger(
  machine: VoiceStateMachine,
  target: AsunaLogger = defaultLogger,
): () => void {
  return machine.subscribe(createVoiceStateLogger(target).onTransition);
}

export interface LoggedVoiceStateMachineOptions extends Omit<
  VoiceStateMachineOptions,
  'onInvalidTransition'
> {
  readonly logger?: AsunaLogger;
}

/**
 * Log'u bastan bagli bir state machine uretir: gecerli gecisler `subscribe`,
 * reddedilenler `onInvalidTransition` uzerinden loglanir.
 *
 * Abonelik makinenin omru boyunca acik kalir (makine log'unun sahibi odur);
 * ayrica sokmek gerekiyorsa [`createVoiceStateLogger`] ile elle baglayin.
 */
export function createLoggedVoiceStateMachine(
  options: LoggedVoiceStateMachineOptions = {},
): VoiceStateMachine {
  const { logger: target, ...machineOptions } = options;
  const hooks = createVoiceStateLogger(target ?? defaultLogger);
  const machine = new VoiceStateMachine({
    ...machineOptions,
    onInvalidTransition: hooks.onInvalidTransition,
  });
  machine.subscribe(hooks.onTransition);
  return machine;
}

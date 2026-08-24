/**
 * Voice state machine (ASU-014) — uygulamanin tek dogru durum kaynagi.
 *
 * Kaynak: `PROJECT.md` Bolum 7/9 (durum listesi + oturum yasam dongusu),
 * `asuna-config/tech-stack.md` Bolum 2 (gecis kurallari),
 * `asuna-config/conventions.md` — "Voice State Machine".
 *
 * Tasarim kararlari:
 * - **React bagimliligi yok.** Saf TS; React entegrasyonu ASU-015'te hook ile sarilir.
 *   UI bu durumun turevi olur, tersi degil.
 * - **Tek publish noktasi**: uygulanan her gecis [`VoiceStateMachine.subscribe`] ile
 *   yayinlanir. ASU-019 loglamasi buraya baglanacak; bilesen icinde ad-hoc `setState` yok.
 * - **Gecersiz gecis sessizce yutulmaz.** Politika [`InvalidTransitionPolicy`]:
 *   dev'de `throw` (bug gelistirici masasinda patlar), prod'da `reject`
 *   (durum degismez, `onInvalidTransition` ile loglanir, sesli oturum cokmez —
 *   PROJECT.md Bolum 30: bozulan alt sistem tum urunu dusurmez).
 */

export const VOICE_STATES = [
  'BOOTING',
  /** Phase 2 (ASU-023+): wake word motoru acik, Realtime oturumu yok, buluta ses gitmiyor. */
  'IDLE_WAKE_WORD',
  'WAKING',
  'CONNECTING',
  'LISTENING',
  'USER_SPEAKING',
  'ASSISTANT_THINKING',
  'ASSISTANT_SPEAKING',
  /** Phase 5 (ASU-05x): tool cagrisi calisiyor. */
  'TOOL_PENDING',
  /** Phase 5 (ASU-05x): risk 2/3 tool kullanici onayi bekliyor. */
  'AWAITING_APPROVAL',
  'ERROR',
] as const;

export type VoiceState = (typeof VOICE_STATES)[number];

/**
 * Gecis nedeni — log/audit icin SCREAMING_SNAKE event adi (conventions.md).
 * Magic string yok: her gecis nedeni bu kumede tanimli olmak zorunda.
 */
export const VOICE_TRANSITION_REASONS = [
  'BOOT_COMPLETED',
  /** Phase 1: gecici "Talk to Asuna" butonu (ASU-015). */
  'ACTIVATION_REQUESTED',
  /** Phase 2: wake word motoru tetikledi (ASU-023). */
  'WAKE_WORD_DETECTED',
  'MIC_PERMISSION_GRANTED',
  'MIC_PERMISSION_DENIED',
  'REALTIME_CONNECTING',
  'REALTIME_CONNECTED',
  'USER_SPEECH_STARTED',
  'USER_SPEECH_ENDED',
  'ASSISTANT_RESPONSE_STARTED',
  'ASSISTANT_AUDIO_STARTED',
  'ASSISTANT_RESPONSE_COMPLETED',
  'USER_INTERRUPTED',
  /** Phase 5. */
  'TOOL_CALL_STARTED',
  'TOOL_APPROVAL_REQUESTED',
  'TOOL_APPROVAL_GRANTED',
  'TOOL_APPROVAL_DENIED',
  'TOOL_CALL_COMPLETED',
  'SESSION_CLOSED_BY_USER',
  'SESSION_TIMEOUT',
  'ERROR_OCCURRED',
  'ERROR_RECOVERED',
] as const;

export type VoiceTransitionReason = (typeof VOICE_TRANSITION_REASONS)[number];

/**
 * Oturumun kapandigi her yolun (kullanici stop / timeout / kurtarilamaz hata) donebilecegi
 * durumlar.
 *
 * `IDLE_WAKE_WORD` kanonik hedef (tech-stack.md Bolum 2: "her yol IDLE_WAKE_WORD'e doner").
 * `BOOTING` ise Phase 1'in gecici karsiligidir: wake word motoru henuz yok, kapanistan sonra
 * uygulama "hazir ama hicbir sey dinlemiyor" durumunda kalir (phase-1.md ASU-018:
 * "durum IDLE/BOOTING'e donuyor").
 *
 * TEMPORARY: Phase 2'de (ASU-023) wake word geldiginde `BOOTING` hedef olmaktan cikar.
 */
const SESSION_EXIT_TARGETS = ['IDLE_WAKE_WORD', 'BOOTING'] as const;

/**
 * Acik gecis tablosu: `from` -> izin verilen `to` kumesi.
 *
 * Phase 1 akisi (ASU-015/016/018):
 * `BOOTING -> WAKING -> CONNECTING -> LISTENING <-> USER_SPEAKING <-> ASSISTANT_THINKING
 *  -> ASSISTANT_SPEAKING -> LISTENING -> BOOTING`.
 * `IDLE_WAKE_WORD`, `TOOL_PENDING`, `AWAITING_APPROVAL` durumlarina giden kenarlar tanimli
 * ama Phase 1 akisindan tetiklenmez; asagida phase notu ile isaretli.
 */
export const VOICE_STATE_TRANSITIONS: Readonly<Record<VoiceState, readonly VoiceState[]>> = {
  BOOTING: [
    'IDLE_WAKE_WORD', // Phase 2: wake word motoru hazir
    'WAKING', // Phase 1: "Talk to Asuna" butonu
    'ERROR',
  ],
  // Phase 2+ giris kapisi. Phase 1'de bu duruma girilmez.
  IDLE_WAKE_WORD: ['WAKING', 'BOOTING', 'ERROR'],
  WAKING: ['CONNECTING', ...SESSION_EXIT_TARGETS, 'ERROR'],
  CONNECTING: ['LISTENING', ...SESSION_EXIT_TARGETS, 'ERROR'],
  LISTENING: ['USER_SPEAKING', 'ASSISTANT_THINKING', ...SESSION_EXIT_TARGETS, 'ERROR'],
  USER_SPEAKING: [
    'LISTENING', // konusma bitti, model henuz cevap uretmiyor
    'ASSISTANT_THINKING',
    ...SESSION_EXIT_TARGETS,
    'ERROR',
  ],
  ASSISTANT_THINKING: [
    'ASSISTANT_SPEAKING',
    'LISTENING', // cevap uretilmedi / iptal
    'USER_SPEAKING', // barge-in
    'TOOL_PENDING', // Phase 5
    ...SESSION_EXIT_TARGETS,
    'ERROR',
  ],
  ASSISTANT_SPEAKING: [
    'LISTENING', // cevap tamamlandi
    'USER_SPEAKING', // barge-in: kullanici sozu kesti
    'ASSISTANT_THINKING', // ayni tur icinde yeni parca uretiliyor
    'TOOL_PENDING', // Phase 5
    ...SESSION_EXIT_TARGETS,
    'ERROR',
  ],
  // Phase 5 (ASU-05x). Phase 1 akisindan erisilmez.
  TOOL_PENDING: [
    'AWAITING_APPROVAL',
    'ASSISTANT_THINKING', // tool sonucu modele donuyor
    'USER_SPEAKING', // barge-in
    ...SESSION_EXIT_TARGETS,
    'ERROR',
  ],
  // Phase 5 (ASU-05x). Phase 1 akisindan erisilmez.
  AWAITING_APPROVAL: [
    'TOOL_PENDING', // onaylandi
    'ASSISTANT_THINKING', // reddedildi
    'USER_SPEAKING', // barge-in
    ...SESSION_EXIT_TARGETS,
    'ERROR',
  ],
  // ERROR terminal durum degil (conventions.md): ya yeniden baglanilir ya idle'a donulur.
  ERROR: ['WAKING', 'CONNECTING', ...SESSION_EXIT_TARGETS],
};

/** Uygulanmis bir gecis. Tek publish noktasindan yayinlanan olay. */
export interface VoiceStateTransition {
  readonly from: VoiceState;
  readonly to: VoiceState;
  readonly reason: VoiceTransitionReason;
  /** UTC ISO-8601 (conventions.md "Database" zaman kurali ile ayni bicim). */
  readonly at: string;
}

/** Reddedilmis bir gecis girisimi — ASU-019 log'unda `error` seviyesine dusecek. */
export interface InvalidVoiceTransition {
  readonly from: VoiceState;
  readonly attempted: VoiceState;
  readonly reason: VoiceTransitionReason;
  readonly at: string;
}

/**
 * - `throw`: gecersiz gecis `InvalidVoiceTransitionError` firlatir (dev varsayilani).
 * - `reject`: durum degismez, `onInvalidTransition` cagirilir, `transition()` `false` doner.
 */
export type InvalidTransitionPolicy = 'throw' | 'reject';

export class InvalidVoiceTransitionError extends Error {
  public override readonly name = 'InvalidVoiceTransitionError';

  public constructor(public readonly transition: InvalidVoiceTransition) {
    super(
      `Gecersiz voice state gecisi: ${transition.from} -> ${transition.attempted} ` +
        `(${transition.reason}).`,
    );
  }
}

export type VoiceTransitionListener = (transition: VoiceStateTransition) => void;

export interface VoiceStateMachineOptions {
  readonly initialState?: VoiceState;
  /** Varsayilan: dev'de `throw`, prod'da `reject`. */
  readonly invalidTransitionPolicy?: InvalidTransitionPolicy;
  /** Reddedilen gecisler icin gozlem kancasi (ASU-019). */
  readonly onInvalidTransition?: (transition: InvalidVoiceTransition) => void;
  /** Zaman kaynagi — testte deterministik kilmak icin enjekte edilir. */
  readonly now?: () => Date;
}

function defaultInvalidTransitionPolicy(): InvalidTransitionPolicy {
  // Dev'de bug'i sessiz bir "durum degismedi" olarak degil, hemen patlayarak gormek isteriz.
  return import.meta.env.DEV ? 'throw' : 'reject';
}

export class VoiceStateMachine {
  private state: VoiceState;

  private readonly listeners = new Set<VoiceTransitionListener>();

  private readonly policy: InvalidTransitionPolicy;

  private readonly onInvalidTransition: ((transition: InvalidVoiceTransition) => void) | null;

  private readonly now: () => Date;

  public constructor(options: VoiceStateMachineOptions = {}) {
    this.state = options.initialState ?? 'BOOTING';
    this.policy = options.invalidTransitionPolicy ?? defaultInvalidTransitionPolicy();
    this.onInvalidTransition = options.onInvalidTransition ?? null;
    this.now = options.now ?? ((): Date => new Date());
  }

  public getState(): VoiceState {
    return this.state;
  }

  public canTransition(to: VoiceState): boolean {
    return VOICE_STATE_TRANSITIONS[this.state].includes(to);
  }

  /**
   * Gecisi uygular ve abonelere yayinlar.
   *
   * @returns gecis uygulandi mi. `false` yalnizca `reject` politikasinda mumkundur.
   * @throws {InvalidVoiceTransitionError} `throw` politikasinda gecersiz gecis.
   * @throws {AggregateError} bir veya daha fazla abone hata firlattiysa — durum
   *   degismis ve **tum** aboneler bilgilendirilmis olur; hata sessizce yutulmaz.
   */
  public transition(to: VoiceState, reason: VoiceTransitionReason): boolean {
    const at = this.now().toISOString();
    const from = this.state;

    if (!this.canTransition(to)) {
      const invalid: InvalidVoiceTransition = { from, attempted: to, reason, at };
      if (this.policy === 'throw') {
        throw new InvalidVoiceTransitionError(invalid);
      }
      this.onInvalidTransition?.(invalid);
      return false;
    }

    this.state = to;
    this.publish({ from, to, reason, at });
    return true;
  }

  /** @returns aboneligi kaldiran fonksiyon. */
  public subscribe(listener: VoiceTransitionListener): () => void {
    this.listeners.add(listener);
    return (): void => {
      this.listeners.delete(listener);
    };
  }

  /**
   * Tek publish noktasi. Bir abonenin hatasi digerlerini engellemez (bozuk bir log
   * paneli sesli oturumu kesmemeli); hatalar toplanip sonunda birlikte firlatilir.
   */
  private publish(transition: VoiceStateTransition): void {
    const failures: unknown[] = [];

    for (const listener of [...this.listeners]) {
      try {
        listener(transition);
      } catch (error) {
        failures.push(error);
      }
    }

    if (failures.length > 0) {
      throw new AggregateError(
        failures,
        `Voice state abonesi hata verdi: ${transition.from} -> ${transition.to} ` +
          `(${transition.reason}).`,
      );
    }
  }
}

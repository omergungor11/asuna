/**
 * `AsunaRealtimeService` — OpenAI Agents SDK'sinin **tek** sarmalayicisi (ASU-013).
 *
 * # Sozlesme
 *
 * `RealtimeAgent` / `RealtimeSession` yalnizca bu dosyada import edilir. SDK surumu
 * degistiginde (voice.md Bolum 1: 3 haftada 3 minor) degismesi gereken tek yer burasi
 * olmali; disariya SDK tipi **sizmaz**. Bu kural `sdk-import-boundary.spec.ts` ile
 * testle zorlanir (`conventions.md` — "Mimari — Servis Sinirlari").
 *
 * Dosya iki parcadan olusur:
 * 1. [`createOpenAiRealtimeSession`] — SDK adaptoru. SDK event'lerini duz veri
 *    [`RealtimeSessionSignal`]'lerine cevirir.
 * 2. [`AsunaRealtimeService`] — sinyalleri `VoiceStateMachine` gecislerine (ASU-014) ve
 *    normalize [`AsunaRealtimeEvent`]'lere cevirir; yasam dongusunu yonetir.
 *
 * # Guvenlik
 *
 * - Kalici `OPENAI_API_KEY` bu dosyada **yok**. Token her `connect()` denemesinde
 *   Rust'tan taze istenir ve SDK'ya *lazy* fonksiyonla verilir (voice.md Bolum 9).
 * - `transport: 'webrtc'` acikca verilir — sessiz WebSocket fallback'i yok
 *   (voice.md Bolum 9 madde 1).
 * - SDK'nin kalici anahtar kacis kapisi (browser'da `ek_` guard'ini kapatan secenek)
 *   hicbir kosulda kullanilmaz; `sdk-import-boundary.spec.ts` bunu da tariyor.
 * - Model ID config'ten gelir (`ASUNA_REALTIME_MODEL`), hard-code yok.
 */

import { RealtimeAgent, RealtimeSession, type RealtimeItem } from '@openai/agents-realtime';

import {
  AsunaRealtimeError,
  describeConnectError,
  describeSessionError,
  describeTokenError,
  type AsunaRealtimeErrorInfo,
} from './realtime-errors';
import type {
  AsunaRealtimeEvent,
  AsunaRealtimeEventListener,
  RealtimeUsageSnapshot,
  TranscriptEntry,
} from './realtime-events';
import type {
  RealtimeSessionFactory,
  RealtimeSessionPort,
  RealtimeSessionSignal,
  RealtimeSessionSignalListener,
  RealtimeSessionSpec,
  TurnDetectionSpec,
} from './realtime-session-port';
import { mintRealtimeToken, type EphemeralRealtimeToken } from './realtime-token';
import type { FrontendConfig } from '../config/frontend-config';
import { buildAsunaInstructions } from '../prompts';
import {
  VoiceStateMachine,
  type VoiceState,
  type VoiceTransitionReason,
} from '../state/voice-state-machine';
import type { AsunaToolDefinition } from '../tools/types';

// ---------------------------------------------------------------------------
// Sabitler
// ---------------------------------------------------------------------------

/** Agent adi tracing/log'da gorunur; model kimligiyle ilgisi yok. */
const ASUNA_AGENT_NAME = 'Asuna';

/**
 * Kullanici sesinin transkripsiyon modeli (voice.md Bolum 2 — SDK varsayilani ile ayni).
 *
 * `transcriptStorage` kapaliysa transkripsiyon tamamen kapatilir. Karar **her
 * `connect()` icin yeniden** okunur (`resolveTranscription`): anahtar calisma
 * zamaninda kapatilabilir ve yeniden baslatma beklememeli (ASU-037).
 */
const TRANSCRIPTION_MODEL = 'gpt-4o-mini-transcribe';

/** Turkce transkript kalitesi icin dil ipucu (voice.md Bolum 2). */
const TRANSCRIPTION_LANGUAGE = 'tr';

/**
 * Yeniden baglanma toplam deneme sayisi (ilk deneme dahil): 1 + 2 retry.
 * Sonsuz retry yok — kullanici bekletilmez, hata durustce gosterilir.
 */
const DEFAULT_MAX_CONNECT_ATTEMPTS = 3;

/** Denemeler arasi bekleme. */
const DEFAULT_RECONNECT_DELAY_MS = 500;

/**
 * Config'ten SDK'ya gidecek tur tespiti ayarini kurar (ASU-064).
 *
 * `createResponse` / `interruptResponse` burada degil, SDK cagrisinda sabit `true`:
 * ikisi de Asuna'nin urun sozlesmesi (kullanici konusunca Asuna susar, konusma bitince
 * kendiliginden cevaplar) — env ile kapatilabilir olmamalari bilincli.
 */
export function toTurnDetectionSpec(config: FrontendConfig): TurnDetectionSpec {
  return config.turnDetection === 'semantic_vad'
    ? { type: 'semantic_vad', eagerness: config.vadEagerness }
    : { type: 'server_vad', silenceDurationMs: config.vadSilenceMs };
}

// ---------------------------------------------------------------------------
// 1. SDK adaptoru — SDK tipleri bu bolumun disina cikmaz
// ---------------------------------------------------------------------------

function messageText(item: Extract<RealtimeItem, { type: 'message' }>): string {
  const parts: string[] = [];

  if (item.role === 'user') {
    for (const part of item.content) {
      parts.push(part.type === 'input_text' ? part.text : (part.transcript ?? ''));
    }
  } else if (item.role === 'assistant') {
    for (const part of item.content) {
      parts.push(part.type === 'output_text' ? part.text : (part.transcript ?? ''));
    }
  }

  return parts.filter((part) => part.length > 0).join(' ');
}

/**
 * `RealtimeItem[]` -> [`TranscriptEntry`][]. Sistem mesajlari ve tool item'lari
 * dokume girmez (tool'lar ayri event'lerle raporlanir).
 */
export function toTranscriptEntries(items: readonly RealtimeItem[]): TranscriptEntry[] {
  const entries: TranscriptEntry[] = [];

  for (const item of items) {
    if (item.type !== 'message' || item.role === 'system') {
      continue;
    }
    entries.push({
      itemId: item.itemId,
      role: item.role,
      text: messageText(item),
      status: item.status,
    });
  }

  return entries;
}

/**
 * Gercek SDK oturumunu kurar.
 *
 * WebRTC olmayan bir ortamda `new RealtimeSession(...)` **kurucu asamasinda** hata
 * firlatir; cagiran ([`AsunaRealtimeService`]) bunu yakalayip `ERROR` durumuna cevirir.
 */
export const createOpenAiRealtimeSession: RealtimeSessionFactory = (
  spec: RealtimeSessionSpec,
  onSignal: RealtimeSessionSignalListener,
): RealtimeSessionPort => {
  if (spec.tools.length > 0) {
    // Sessizce dusurmek yerine acikca patlat: model'e vaat edilen bir yetenegin
    // sessizce kaybolmasi, olmayan bir yetenegi vaat etmekten daha kotu.
    throw new AsunaRealtimeError({
      kind: 'internal',
      cause: 'tools_not_supported',
      message:
        'Realtime oturumuna tool verildi ama tool destegi henuz yok (Phase 5 / ASU-05x).',
      retryable: false,
    });
  }

  const agent = new RealtimeAgent({
    name: ASUNA_AGENT_NAME,
    instructions: spec.instructions,
  });

  const session = new RealtimeSession(agent, {
    // Acikca WebRTC: `hasWebRTCSupport()` yanlis donerse sessizce WebSocket'e
    // dusmek yerine hata almak istiyoruz (voice.md Bolum 9 madde 1).
    transport: 'webrtc',
    model: spec.model,
    // Ses RAM'de tutulmasin (varsayilan ile ayni; acikca yaziliyor).
    historyStoreAudio: false,
    config: {
      outputModalities: ['audio'],
      audio: {
        input: {
          // camelCase kabul ediliyor (voice.md Bolum 7). Ayarin kendisi config'ten
          // gelir; burada yalnizca Asuna'nin degismez turn politikasi eklenir.
          turnDetection: {
            ...spec.turnDetection,
            createResponse: true,
            interruptResponse: true,
          },
          transcription: spec.transcription
            ? { model: TRANSCRIPTION_MODEL, language: TRANSCRIPTION_LANGUAGE }
            : null,
          noiseReduction: { type: 'near_field' },
        },
        // `exactOptionalPropertyTypes`: `voice: undefined` yazmak yerine kosullu spread.
        output: spec.voice === null ? {} : { voice: spec.voice },
      },
    },
  });

  session.on('agent_start', () => {
    onSignal({ type: 'agent_start' });
  });
  session.on('agent_end', () => {
    onSignal({ type: 'agent_end' });
  });
  session.on('audio_start', () => {
    onSignal({ type: 'audio_start' });
  });
  session.on('audio_stopped', () => {
    onSignal({ type: 'audio_stopped' });
  });
  session.on('audio_interrupted', () => {
    onSignal({ type: 'audio_interrupted' });
  });
  session.on('history_updated', (history) => {
    onSignal({ type: 'history', entries: toTranscriptEntries(history) });
  });
  session.on('history_added', (item) => {
    onSignal({ type: 'history', entries: toTranscriptEntries([item]) });
  });
  session.on('agent_tool_start', (_context, _agent, tool) => {
    onSignal({ type: 'tool_start', toolName: tool.name });
  });
  session.on('agent_tool_end', (_context, _agent, tool) => {
    onSignal({ type: 'tool_end', toolName: tool.name });
  });
  session.on('tool_approval_requested', (_context, _agent, request) => {
    onSignal({
      type: 'tool_approval_requested',
      toolName: request.type === 'function_approval' ? request.tool.name : 'mcp_tool',
    });
  });
  session.on('error', (event) => {
    onSignal({ type: 'error', error: describeSessionError(event.error) });
  });

  return {
    connect: (options) => session.connect({ apiKey: options.apiKey }),
    close: () => {
      // SDK'da `void` — `await` edilmez (voice.md Bolum 9 madde 4).
      session.close();
    },
    interrupt: () => {
      session.interrupt();
    },
    usage: (): RealtimeUsageSnapshot => {
      const usage = session.usage;
      return {
        requests: usage.requests,
        inputTokens: usage.inputTokens,
        outputTokens: usage.outputTokens,
        totalTokens: usage.totalTokens,
        inputTokenDetails: usage.inputTokensDetails.map((detail) => ({ ...detail })),
        outputTokenDetails: usage.outputTokensDetails.map((detail) => ({ ...detail })),
      };
    },
  };
};

// ---------------------------------------------------------------------------
// 2. Servis
// ---------------------------------------------------------------------------

type ServiceStatus = 'idle' | 'connecting' | 'connected';

export interface AsunaRealtimeServiceOptions {
  /** `loadFrontendConfig()` ciktisi — model, ses ve transkript politikasi buradan. */
  readonly config: FrontendConfig;
  /**
   * Phase 1'de **bos gecilir**. Phase 5'te (ASU-05x) registry'den gelecek
   * (phase-1.md ASU-013 notlari).
   */
  readonly tools?: readonly AsunaToolDefinition[];
  /** Paylasilan durum makinesi. Verilmezse servis kendi ornegini kurar. */
  readonly stateMachine?: VoiceStateMachine;
  /** Modele verilecek talimat. Varsayilan: `buildAsunaInstructions()`. */
  readonly instructions?: string;
  /**
   * Her `connect()` cagrisindan **once** taze talimat uretir (ASU-035).
   *
   * Neden kurucuda sabit bir metin yetmiyor: oturum baglami hafizadan gelir ve
   * iki oturum arasinda degisir (ozet + cikarim kapanista calisir). Servis
   * omru boyunca ayni metni kullanmak, ikinci oturumda **eski** hafizayi
   * enjekte etmek demekti.
   *
   * Hata firlatirsa oturum dusmez: [`instructions`] degerine geri donulur ve
   * olay `error` event'i ile gorunur kalir (sessiz yutma yok).
   */
  readonly prepareInstructions?: () => Promise<string>;
  /**
   * Her `connect()` cagrisindan **once** transkripsiyonun acik olup olmayacagini
   * belirler (ASU-037 / Gate 3 MEDIUM-3).
   *
   * Neden boot config'i yetmiyor: `transcriptStorage` bir **calisma zamani**
   * anahtaridir; kullanici Ayarlar'dan kapattiginda yeniden baslatmadan etkili
   * olmali. `config.transcriptStorage` acilis degeridir ve yalnizca **tavandir**
   * — servis her oturumda guncel degeri sorar ve ikisini `&&` ile birlestirir.
   *
   * Verilmezse acilis degeri kullanilir (testler, ASU-013 oncesi cagiranlar).
   * Hata firlatirsa transkripsiyon **kapali** kurulur: gizlilik kararini
   * okuyamadigimizda acik varsaymak, kullanicinin kapatmis olabilecegi bir
   * ayari sessizce gecersiz kilardi. Hata yutulmaz, `error` event'i ile gorunur.
   */
  readonly resolveTranscription?: () => Promise<boolean>;
  /** SDK yerine sahte oturum enjekte etmek icin (testler). */
  readonly createSession?: RealtimeSessionFactory;
  /** Token kaynagi. Varsayilan: `mint_realtime_token` IPC komutu. */
  readonly mintToken?: () => Promise<EphemeralRealtimeToken>;
  /** Ilk deneme dahil toplam baglanti denemesi. En az 1. */
  readonly maxConnectAttempts?: number;
  readonly reconnectDelayMs?: number;
  /** Testlerde zamani hizlandirmak icin. */
  readonly sleep?: (ms: number) => Promise<void>;
  /**
   * Oturum kapaninca donulecek durum.
   *
   * TEMPORARY: Phase 1'de wake word motoru yok, kanonik hedef `IDLE_WAKE_WORD` yerine
   * `BOOTING` (ASU-014 `SESSION_EXIT_TARGETS`). ASU-023'te varsayilan degisecek.
   */
  readonly idleState?: Extract<VoiceState, 'BOOTING' | 'IDLE_WAKE_WORD'>;
  /**
   * Bir abone hata firlatirsa cagirilir. Varsayilan: hatayi mikrotask'ta yeniden
   * firlatir — gorunur olur ama sesli oturumu dusurmez (`conventions.md`:
   * "Bozulan alt sistem tum urunu dusurmez", "Sessiz yutma yok").
   */
  readonly onListenerError?: (error: unknown) => void;
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function defaultListenerErrorHandler(error: unknown): void {
  queueMicrotask(() => {
    throw error;
  });
}

type ConnectAttemptResult =
  | { readonly ok: true; readonly session: RealtimeSessionPort }
  | { readonly ok: false; readonly error: AsunaRealtimeErrorInfo };

export class AsunaRealtimeService {
  private readonly config: FrontendConfig;

  private readonly tools: readonly AsunaToolDefinition[];

  private readonly stateMachine: VoiceStateMachine;

  private readonly instructions: string;

  private readonly prepareInstructions: (() => Promise<string>) | null;

  private readonly resolveTranscription: (() => Promise<boolean>) | null;

  private readonly createSession: RealtimeSessionFactory;

  private readonly mintToken: () => Promise<EphemeralRealtimeToken>;

  private readonly maxConnectAttempts: number;

  private readonly reconnectDelayMs: number;

  private readonly sleep: (ms: number) => Promise<void>;

  private readonly idleState: Extract<VoiceState, 'BOOTING' | 'IDLE_WAKE_WORD'>;

  private readonly onListenerError: (error: unknown) => void;

  private readonly listeners = new Set<AsunaRealtimeEventListener>();

  /** Yayinlanmis dokum satirlarinin son hali — ayni metni tekrar yaymamak icin. */
  private readonly publishedTranscripts = new Map<string, string>();

  private session: RealtimeSessionPort | null = null;

  private status: ServiceStatus = 'idle';

  /**
   * `connect()` / `disconnect()` yaris korumasi. Her cagri bunu artirir; bir
   * `await`'ten sonra deger degistiyse o akis terk edilmis demektir.
   */
  private generation = 0;

  /**
   * Lazy `apiKey` fonksiyonunda olusan hata. SDK bunu kendi hatasina sarabilir;
   * asil nedeni kaybetmemek icin burada tutulur.
   */
  private tokenError: AsunaRealtimeErrorInfo | null = null;

  public constructor(options: AsunaRealtimeServiceOptions) {
    this.config = options.config;
    this.tools = options.tools ?? [];
    this.stateMachine = options.stateMachine ?? new VoiceStateMachine();
    this.instructions = options.instructions ?? buildAsunaInstructions();
    this.prepareInstructions = options.prepareInstructions ?? null;
    this.resolveTranscription = options.resolveTranscription ?? null;
    this.createSession = options.createSession ?? createOpenAiRealtimeSession;
    this.mintToken = options.mintToken ?? mintRealtimeToken;
    this.maxConnectAttempts = Math.max(
      1,
      options.maxConnectAttempts ?? DEFAULT_MAX_CONNECT_ATTEMPTS,
    );
    this.reconnectDelayMs = options.reconnectDelayMs ?? DEFAULT_RECONNECT_DELAY_MS;
    this.sleep = options.sleep ?? defaultSleep;
    this.idleState = options.idleState ?? 'BOOTING';
    this.onListenerError = options.onListenerError ?? defaultListenerErrorHandler;
  }

  // --- Genel API --------------------------------------------------------

  public getState(): VoiceState {
    return this.stateMachine.getState();
  }

  /** @returns aboneligi kaldiran fonksiyon. */
  public subscribe(listener: AsunaRealtimeEventListener): () => void {
    this.listeners.add(listener);
    return (): void => {
      this.listeners.delete(listener);
    };
  }

  /**
   * Oturumu acar.
   *
   * Basarisiz olursa `ERROR` durumuna gecer, `error` event'i yayinlar ve
   * [`AsunaRealtimeError`] firlatir — cagiran taraf hatayi hem event akisindan hem
   * `await` noktasindan gorur, sessizce "bagliyim" sanmaz.
   *
   * Es zamanli/tekrarli cagrilar yok sayilir (cift tiklama yaris kosulu uretmez).
   */
  public async connect(): Promise<void> {
    if (this.status !== 'idle') {
      return;
    }

    this.status = 'connecting';
    const generation = ++this.generation;
    this.publishedTranscripts.clear();

    // ASU-015 butonu zaten `WAKING`'e gecirmis olabilir; degilse servis kendisi gecirir
    // ki durum makinesi hicbir zaman atlanmis bir adim gormesin.
    this.applyTransition('WAKING', 'ACTIVATION_REQUESTED', 'connect');
    this.applyTransition('CONNECTING', 'REALTIME_CONNECTING', 'connect');

    // Baglam **oturum basina bir kez** cekilir: retry'larda tekrar sorgulamak
    // hem gereksiz IPC hem de denemeler arasinda degisen bir prompt demekti.
    // Saglayici yoksa `await` edilmez — bos bir microtask, oturumun acilisini
    // sebepsiz yere bir tur geciktirirdi.
    const instructions =
      this.prepareInstructions === null ? this.instructions : await this.resolveInstructions();
    if (generation !== this.generation) {
      // Baglam beklenirken kapatildi: henuz acilmis bir oturum yok.
      return;
    }

    // Gizlilik anahtari da oturum basina bir kez okunur (ASU-037): oturum
    // ortasinda degistirilemeyen bir SDK ayari zaten oturum basinda sabitlenir.
    // Acilista kapaliysa (tavan) ya da saglayici yoksa `await` edilmez —
    // gereksiz bir microtask oturum acilisini bir tur geciktirirdi.
    const needsPrivacyRead =
      this.config.transcriptStorage && this.resolveTranscription !== null;
    const transcription = needsPrivacyRead
      ? await this.resolveTranscriptionEnabled()
      : this.config.transcriptStorage;
    if (generation !== this.generation) {
      return;
    }

    for (let attempt = 1; attempt <= this.maxConnectAttempts; attempt += 1) {
      this.publish({ type: 'connecting', attempt, maxAttempts: this.maxConnectAttempts });

      const result = await this.attemptConnect(instructions, transcription);

      if (generation !== this.generation) {
        // Bu akis terk edildi (disconnect ya da yeni bir connect). Acilmis bir oturum
        // varsa arkada birakmiyoruz.
        if (result.ok) {
          this.closeSession(result.session);
        }
        return;
      }

      if (result.ok) {
        this.session = result.session;
        this.status = 'connected';
        this.applyTransition('LISTENING', 'REALTIME_CONNECTED', 'connect');
        this.publish({ type: 'connected', model: this.config.realtimeModel });
        return;
      }

      const isLastAttempt = attempt === this.maxConnectAttempts;
      if (!result.error.retryable || isLastAttempt) {
        throw this.failConnect(result.error);
      }

      this.publish({
        type: 'reconnecting',
        attempt: attempt + 1,
        maxAttempts: this.maxConnectAttempts,
        delayMs: this.reconnectDelayMs,
        error: result.error,
      });

      await this.sleep(this.reconnectDelayMs);

      if (generation !== this.generation) {
        return;
      }
    }
  }

  /**
   * Oturumu kapatir ve idle duruma doner. Kapanistan hemen once `usage` event'i
   * yayinlanir (ASU-020 maliyet olcumu).
   *
   * Idempotent; `connect()` devam ederken cagrilirsa o akis terk edilir.
   */
  public disconnect(): void {
    if (this.status === 'idle') {
      return;
    }

    this.generation += 1;
    this.status = 'idle';

    const session = this.session;
    this.session = null;
    this.tokenError = null;
    this.publishedTranscripts.clear();

    if (session !== null) {
      this.reportUsage(session);
      this.closeSession(session);
    }

    this.applyTransition(this.idleState, 'SESSION_CLOSED_BY_USER', 'disconnect');
    this.publish({ type: 'disconnected', reason: 'requested' });
  }

  /** Manuel "sus": uretilmekte olan yaniti keser. Durum degisimi SDK sinyalinden gelir. */
  public interrupt(): void {
    const session = this.session;
    if (session === null) {
      this.publish({
        type: 'unexpected_signal',
        signal: 'interrupt',
        state: this.stateMachine.getState(),
      });
      return;
    }
    session.interrupt();
  }

  // --- Baglanti ic akisi ------------------------------------------------

  /**
   * Oturum talimatini uretir (ASU-035 baglam enjeksiyonu).
   *
   * Saglayici patlarsa **konusma bloklanmaz**: cekirdek talimatla devam edilir
   * ve hata event'e duser. Bu yolun normalde calismamasi beklenir — baglam
   * saglayicisi kendi hatalarini zaten ele alir (`buildSessionInstructions`).
   */
  private async resolveInstructions(): Promise<string> {
    if (this.prepareInstructions === null) {
      return this.instructions;
    }
    try {
      return await this.prepareInstructions();
    } catch (error) {
      this.publish({ type: 'error', error: describeSessionError(error) });
      return this.instructions;
    }
  }

  /**
   * Bu oturumda kullanici sesi yaziya cevrilecek mi? (ASU-037)
   *
   * Iki kaynak `&&` ile baglanir: acilis degeri (`config.transcriptStorage`,
   * tavan) ve calisma zamani anahtari. Calisma zamani yalnizca **sikilastirir**
   * — Rust tarafi zaten gevsetmeyi reddediyor, burada da varsayilmiyor.
   */
  private async resolveTranscriptionEnabled(): Promise<boolean> {
    if (!this.config.transcriptStorage || this.resolveTranscription === null) {
      return this.config.transcriptStorage;
    }
    try {
      return await this.resolveTranscription();
    } catch (error) {
      // Gizlilik durumu okunamadi: **kapali** varsayilir, ama yutulmaz.
      this.publish({ type: 'error', error: describeSessionError(error) });
      return false;
    }
  }

  private async attemptConnect(
    instructions: string,
    transcription: boolean,
  ): Promise<ConnectAttemptResult> {
    const spec: RealtimeSessionSpec = {
      instructions,
      model: this.config.realtimeModel,
      voice: this.config.realtimeVoice,
      transcription,
      turnDetection: toTurnDetectionSpec(this.config),
      tools: this.tools,
    };

    let session: RealtimeSessionPort;
    try {
      session = this.createSession(spec, (signal) => {
        this.handleSignal(signal);
      });
    } catch (error) {
      return { ok: false, error: toErrorInfo(error, describeConnectError) };
    }

    this.tokenError = null;

    try {
      await session.connect({ apiKey: () => this.provideApiKey() });
    } catch (error) {
      this.closeSession(session);
      // Token asamasindaki asil neden, SDK'nin sardigi hatadan daha bilgilendirici.
      const tokenError = this.takeTokenError();
      return { ok: false, error: tokenError ?? toErrorInfo(error, describeConnectError) };
    }

    return { ok: true, session };
  }

  /** Lazy `apiKey` fonksiyonunda kaydedilmis hatayi okur ve temizler. */
  private takeTokenError(): AsunaRealtimeErrorInfo | null {
    const error = this.tokenError;
    this.tokenError = null;
    return error;
  }

  /**
   * SDK'ya verilen **lazy** `apiKey` fonksiyonu (voice.md Bolum 9).
   *
   * Token cache'lenmez, log'lanmaz; yalnizca SDK'ya doner.
   */
  private async provideApiKey(): Promise<string> {
    try {
      const token = await this.mintToken();

      if (token.model !== this.config.realtimeModel) {
        // Rust token'i baska bir modele bastiysa oturum acilsa bile yanlis modelde
        // konusulur (model oturum ortasinda degistirilemez — voice.md Bolum 4).
        throw new AsunaRealtimeError({
          kind: 'internal',
          cause: 'model_mismatch',
          message:
            `Token \`${token.model}\` modeli icin basildi ama oturum ` +
            `\`${this.config.realtimeModel}\` bekliyor. Yapilandirma tutarsiz.`,
          retryable: false,
        });
      }

      return token.value;
    } catch (error) {
      const info = toErrorInfo(error, describeTokenError);
      this.tokenError = info;
      throw error instanceof Error ? error : new AsunaRealtimeError(info);
    }
  }

  /** Baglantiyi basarisiz kapatir ve cagirana firlatilacak hatayi uretir. */
  private failConnect(error: AsunaRealtimeErrorInfo): AsunaRealtimeError {
    this.status = 'idle';
    this.session = null;
    this.applyTransition('ERROR', 'ERROR_OCCURRED', 'connect');
    this.publish({ type: 'error', error });
    return new AsunaRealtimeError(error);
  }

  private reportUsage(session: RealtimeSessionPort): void {
    try {
      this.publish({ type: 'usage', usage: session.usage() });
    } catch (error) {
      // Maliyet olcumu okunamamasi oturum kapanisini engellememeli, ama yutulmamali da.
      this.publish({ type: 'error', error: describeSessionError(error) });
    }
  }

  private closeSession(session: RealtimeSessionPort): void {
    try {
      session.close();
    } catch (error) {
      this.publish({ type: 'error', error: describeSessionError(error) });
    }
  }

  // --- Sinyal -> durum + event -----------------------------------------

  /**
   * SDK sinyallerinin durum eslemesi (voice.md Bolum 3 tablosu).
   *
   * `agent_end` durum degistirmez: turun bitisi ses akisiyla (`audio_stopped`)
   * belirlenir, metin bitisiyle degil.
   */
  private handleSignal(signal: RealtimeSessionSignal): void {
    switch (signal.type) {
      case 'agent_start':
        this.applyTransition('ASSISTANT_THINKING', 'ASSISTANT_RESPONSE_STARTED', signal.type);
        this.publish({ type: 'agent_thinking' });
        return;

      case 'agent_end':
        this.publish({ type: 'turn_ended' });
        return;

      case 'audio_start':
        this.applyTransition('ASSISTANT_SPEAKING', 'ASSISTANT_AUDIO_STARTED', signal.type);
        this.publish({ type: 'agent_audio_started' });
        return;

      case 'audio_stopped':
        this.applyTransition('LISTENING', 'ASSISTANT_RESPONSE_COMPLETED', signal.type);
        this.publish({ type: 'agent_audio_stopped' });
        return;

      case 'audio_interrupted':
        // Barge-in: sunucu yaniti kesti cunku kullanici konusmaya basladi.
        this.applyTransition('USER_SPEAKING', 'USER_INTERRUPTED', signal.type);
        this.publish({ type: 'agent_interrupted' });
        return;

      case 'history':
        this.publishTranscripts(signal.entries);
        return;

      // Phase 5 (ASU-05x): Phase 1'de tool verilmedigi icin bu sinyaller gelmez.
      case 'tool_start':
        this.applyTransition('TOOL_PENDING', 'TOOL_CALL_STARTED', signal.type);
        this.publish({ type: 'tool_call_started', toolName: signal.toolName });
        return;

      case 'tool_end':
        this.applyTransition('ASSISTANT_THINKING', 'TOOL_CALL_COMPLETED', signal.type);
        this.publish({ type: 'tool_call_completed', toolName: signal.toolName });
        return;

      case 'tool_approval_requested':
        this.applyTransition('AWAITING_APPROVAL', 'TOOL_APPROVAL_REQUESTED', signal.type);
        this.publish({ type: 'tool_approval_requested', toolName: signal.toolName });
        return;

      case 'error':
        // Oturum otomatik kapatilmaz: SDK `error` event'i her zaman olumcul degil.
        // Durum gorunur sekilde `ERROR` olur, kapatma karari cagirana (UI) birakilir.
        this.applyTransition('ERROR', 'ERROR_OCCURRED', signal.type);
        this.publish({ type: 'error', error: signal.error });
        return;

      default:
        signal satisfies never;
        return;
    }
  }

  /** Ayni item'in degismeyen halini tekrar yaymaz (`history_updated` tam snapshot yollar). */
  private publishTranscripts(entries: readonly TranscriptEntry[]): void {
    for (const entry of entries) {
      const fingerprint = `${entry.status} ${entry.text}`;
      if (this.publishedTranscripts.get(entry.itemId) === fingerprint) {
        continue;
      }
      this.publishedTranscripts.set(entry.itemId, fingerprint);
      this.publish({ type: 'transcript', entry });
    }
  }

  /**
   * Gecisi uygular.
   *
   * - Hedef zaten mevcut durumsa: sinyal tekrari, sessizce atlanir.
   * - Gecis tabloda yoksa: durum makinesine gonderilmez (dev'de `throw` politikasi
   *   sesli oturumu dusururdu) ama **yutulmaz** — `unexpected_signal` event'i yayinlanir
   *   ve ASU-019 log'una duser.
   */
  private applyTransition(to: VoiceState, reason: VoiceTransitionReason, signal: string): void {
    const from = this.stateMachine.getState();
    if (from === to) {
      return;
    }
    if (!this.stateMachine.canTransition(to)) {
      this.publish({ type: 'unexpected_signal', signal, state: from });
      return;
    }
    this.stateMachine.transition(to, reason);
  }

  /** Tek publish noktasi. Bir abonenin hatasi digerlerini ve oturumu engellemez. */
  private publish(event: AsunaRealtimeEvent): void {
    for (const listener of [...this.listeners]) {
      try {
        listener(event);
      } catch (error) {
        this.onListenerError(error);
      }
    }
  }
}

/** Zaten siniflandirilmis bir hatayi tekrar siniflandirmaz. */
function toErrorInfo(
  error: unknown,
  describe: (value: unknown) => AsunaRealtimeErrorInfo,
): AsunaRealtimeErrorInfo {
  return error instanceof AsunaRealtimeError ? error.info : describe(error);
}

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

import {
  RealtimeAgent,
  RealtimeSession,
  tool,
  type RealtimeItem,
  type RealtimeSessionEventTypes,
} from '@openai/agents-realtime';

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
  ToolRuntimeBindings,
  TurnDetectionSpec,
} from './realtime-session-port';
import { mintRealtimeToken, type EphemeralRealtimeToken } from './realtime-token';
import type { FrontendConfig } from '../config/frontend-config';
import { logger, redactText } from '../observability';
import { buildAsunaInstructions } from '../prompts';
import {
  VoiceStateMachine,
  type VoiceState,
  type VoiceTransitionReason,
} from '../state/voice-state-machine';
import { recordToolEvent } from '../tools/audit';
import {
  APPROVAL_TIMEOUT_MS,
  resolveApproval,
  type ApprovalOutcome,
} from '../tools/approval-policy';
import { executeTool, type ToolApprovalGate, type ToolResultReport } from '../tools/registry';
import type { AsunaToolDefinition, ToolContext, ToolResult, ToolRisk } from '../tools/types';
import type { ToolAuditInput } from '../../shared/tool-event';

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
 * Tool basarisiz oldugunda modele giden metnin basi.
 *
 * Hata sessizce bos bir sonuca donusmez: model reddi acikca gorur ve
 * "basarili gibi" konusamaz (PROJECT.md Bolum 30).
 */
export const TOOL_FAILURE_PREFIX = 'TOOL BASARISIZ.';

/**
 * Reddedilen onayin modele giden aciklamasi.
 *
 * Model reddi **ogrenmeli**: "yapamadim" ile "yaptim" arasindaki fark burada
 * kuruluyor (PROJECT.md Bolum 30).
 */
export const TOOL_DENIED_MODEL_MESSAGE =
  'Kullanici bu tool cagrisini onaylamadi. Islemi yapmadin; yapilmis gibi anlatma.';

/** Zaman asimina ugrayan onayin modele giden aciklamasi. */
export const TOOL_APPROVAL_TIMEOUT_MODEL_MESSAGE =
  'Onay beklenirken sure doldu, bu yuzden tool calistirilmadi. Yapilmis gibi anlatma; ' +
  'kullanici hala isterse tekrar sorabilirsin.';

/** Oturum kapanirken cevaplanmamis kalan onayin audit ozeti. */
const TOOL_APPROVAL_ABANDONED_SUMMARY =
  'Oturum kapandi; onay alinamadi ve tool calistirilmadi.';

/**
 * Onay/audit baglantisi kurulmadan cevrilen tool'larin varsayilani.
 *
 * `approvalMode: 'always'` bilincli: modu bilmeyen bir cagiran **en gevsek**
 * davranisi miras almamali (ASU-048 — "belirsizlik onay lehine"). Onay kanali
 * yok, yani onay gerektiren bir tool bu runtime ile calismaz; sessizce
 * onaysiz calismaz.
 */
export const DEFAULT_TOOL_RUNTIME: ToolRuntimeBindings = { approvalMode: 'always' };

/**
 * Tool'a verilen calisma context'ini uretir.
 *
 * `sessionId` her cagrida **yeniden** sorulur: `session_start` asenkron doner
 * ve oturumun ilk saniyelerinde kimlik henuz yoktur. Cozulemezse `null` kalir —
 * uydurulmus bir korelasyon kimligi, audit kaydini dogru gorunen ama yanlis bir
 * zincire baglardi. `projectRoot` ASU-049 sandbox'i ile dolacak.
 *
 * `signal` burada yok: iptal sinyalini `executeTool` her cagri icin kendisi
 * uretir (timeout + SDK'nin iptali birlesir).
 */
function toolContextFor(runtime: ToolRuntimeBindings): ToolContext {
  return { sessionId: runtime.resolveSessionId?.() ?? null, projectRoot: null };
}

/** Tool adaptorunun log kanali. Tool ciktisi log'lanmaz, yalnizca hata bilgisi. */
const toolLogger = logger.child('realtime-tool');

/**
 * [`ToolResult`] -> modelin gorecegi metin.
 *
 * Ayri ve saf: SDK'yi calistirmadan test edilebilsin diye. Basarisiz sonuc
 * sessizce bos bir cikti olmaz, [`TOOL_FAILURE_PREFIX`] ile isaretlenir —
 * model "yaptim" diyemez (PROJECT.md Bolum 30).
 */
export function toModelOutput(result: ToolResult): string {
  return result.ok ? result.summary : `${TOOL_FAILURE_PREFIX} ${result.summary}`;
}

/**
 * [`AsunaToolDefinition`] -> SDK `tool()` adaptasyonu (voice.md Bolum 9).
 *
 * SDK tipi buradan **disari cikmaz**: tool tanimlari SDK'siz duz veri olarak
 * yazilir, SDK'ya cevrilmeleri bu dosyanin isidir (`sdk-import-boundary.spec.ts`).
 *
 * Modele donen sey [`ToolResult.summary`] — yani kisa, konusulabilir metin.
 * `data` alani bilerek gonderilmez: yapisal veriyi ses oturumuna dokmek hem
 * token israfi hem de "repoyu dumpleme" yasagina aykiri (PROJECT.md Bolum 15).
 *
 * Calistirma **registry'nin** sarmalayicisindan gecer (`executeTool`): sema
 * dogrulamasi, onay kapisi, timeout ve yapisal hata uretimi SDK'ya degil
 * Asuna'ya ait. Buradaki `timeoutMs` ayni butcenin SDK tarafindaki yedegidir —
 * sarmalayici herhangi bir nedenle donmezse oturum yine de sessizlige gomulmez.
 *
 * # Onay (ASU-048)
 *
 * `needsApproval` statik bir boolean degil, **politika fonksiyonudur**:
 * [`resolveApproval`] karari risk + tanimin talebi + `ASUNA_TOOL_APPROVAL_MODE`
 * uclusunden uretir. `true` donunce SDK `execute`'u **hic** cagirmaz; once
 * `tool_approval_requested` cikar ve karar `approve`/`reject` ile verilir.
 * Ayni karar `executeTool` icinde bagimsiz olarak tekrar hesaplanir — SDK'ya
 * guvenip kapiyi tek yere koymuyoruz.
 */
export function toSdkTool(
  definition: AsunaToolDefinition,
  runtime: ToolRuntimeBindings = DEFAULT_TOOL_RUNTIME,
): ReturnType<typeof tool> {
  if (definition.risk >= 2 && !definition.requiresApproval) {
    // Registry ayni kurali kayit aninda zorluyor; burada ikinci kez kontrol
    // edilmesinin sebebi registry'siz kurulan bir listenin de sizamamasi.
    // Risk 2/3 bir tool'un onay istememesi `conventions.md`'nin pazarliksiz
    // kurallarindan biri: sessizce onaysiz calistirmak yerine acilista patla.
    throw new AsunaRealtimeError({
      kind: 'internal',
      cause: 'tool_approval_missing',
      message: `\`${definition.name}\` risk ${definition.risk.toString()} ama onay istemiyor.`,
      retryable: false,
    });
  }

  return tool({
    name: definition.name,
    description: definition.description,
    parameters: definition.parameters,
    strict: true,
    // Tek satirlik politika baglantisi: karar burada uretilmez, `resolveApproval`
    // matrisinden okunur (ASU-048).
    needsApproval: (): Promise<boolean> =>
      Promise.resolve(
        resolveApproval(definition.risk, definition.requiresApproval, runtime.approvalMode) ===
          'needs_approval',
      ),
    timeoutMs: definition.timeoutMs,
    execute: async (
      args: unknown,
      _runContext?: unknown,
      details?: { readonly signal?: AbortSignal | undefined },
    ): Promise<string> => {
      let result: ToolResult;
      try {
        // SDK'nin iptal sinyali sarmalayiciya devredilir: oturum kapaninca
        // calisan tool da haberdar olur (`ToolContext.signal`).
        result = await executeTool(definition, args, toolContextFor(runtime), {
          approvalMode: runtime.approvalMode,
          ...(runtime.approvalGate === undefined ? {} : { approvalGate: runtime.approvalGate }),
          ...(runtime.onAudit === undefined ? {} : { onAudit: runtime.onAudit }),
          // ASU-054 dikisi: kapatma kapisi ve sonuc kancasi **buradan** gecer.
          // Baglanmadiklarinda `executeTool` onlari `undefined` gorur ve sessizce
          // "her tool acik, kimseye haber verme" davranisina duser — yani kapali
          // bir tool acik oturumda calismaya devam eder ve transcript'e hicbir
          // satir dusmez. Iki alan da opsiyonel oldugu icin derleyici bunu
          // yakalayamaz; `sdk-tool-wiring` testleri dikisi yerine cakiyor.
          ...(runtime.isToolEnabled === undefined ? {} : { isEnabled: runtime.isToolEnabled }),
          ...(runtime.onToolResult === undefined ? {} : { onResult: runtime.onToolResult }),
          ...(details?.signal === undefined ? {} : { signal: details.signal }),
        });
      } catch (error) {
        // `executeTool` sozlesmesi geregi firlatmaz; buraya dusmek beklenmez.
        // Yine de gizlenmez (ASU-019 log formati) ve oturum dusmez.
        const info = describeSessionError(error);
        toolLogger.warn(`\`${definition.name}\` calistirilamadi: ${info.message}`, {
          tool: definition.name,
          kind: info.kind,
        });
        return `${TOOL_FAILURE_PREFIX} ${info.message}`;
      }
      if (!result.ok) {
        toolLogger.warn(`\`${definition.name}\` basarisiz: ${result.summary}`, {
          tool: definition.name,
          errorKind: result.errorKind,
        });
      }
      return toModelOutput(result);
    },
  });
}

/**
 * SDK onay istegi ve onay item'i tipleri.
 *
 * Event tablosundan **turetiliyor**: `@openai/agents-realtime` bu tipleri ismen
 * disari acmiyor (yalnizca `RealtimeSessionEventTypes`), `@openai/agents-core`
 * ise dogrudan bir bagimlilik degil. Turetme, SDK surumu bu event'in imzasini
 * degistirdiginde burayi derleme hatasiyla uyandirir.
 */
type ToolApprovalRequestOf = RealtimeSessionEventTypes['tool_approval_requested'][2];

type RunToolApprovalItemOf = ToolApprovalRequestOf['approvalItem'];

/** Onay istegi kimligi cozulemedigi durumda uretilen benzersiz yedek sayaci. */
let approvalRequestCounter = 0;

/**
 * SDK onay istegini kimlige ve duz veriye cevirir (ASU-048).
 *
 * Kimlik oncelikle `callId`: SDK'nin kendi cagri kimligi, tekrar kullanilmaz ve
 * ayni tool'un iki farkli cagrisini birbirinden ayirir. Yoksa (MCP istegi ya da
 * fonksiyon disi bir item) yerel bir sayac kullanilir — kimliksiz bir onay
 * istegi cevaplanamaz hale gelirdi.
 */
function describeApprovalRequest(request: ToolApprovalRequestOf): {
  readonly requestId: string;
  readonly toolName: string;
  readonly argumentsJson: string | null;
} {
  const rawItem: unknown = request.approvalItem.rawItem;
  const isFunctionCall =
    typeof rawItem === 'object' &&
    rawItem !== null &&
    (rawItem as { readonly type?: unknown }).type === 'function_call';
  const call = isFunctionCall
    ? (rawItem as { readonly callId?: string; readonly arguments?: string })
    : null;

  approvalRequestCounter += 1;
  return {
    requestId: call?.callId ?? `approval_${approvalRequestCounter.toString()}`,
    toolName:
      request.type === 'function_approval'
        ? request.tool.name
        : (request.approvalItem.name ?? 'mcp_tool'),
    argumentsJson: call?.arguments ?? null,
  };
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
  const agent = new RealtimeAgent({
    name: ASUNA_AGENT_NAME,
    instructions: spec.instructions,
    tools: spec.tools.map((definition) => toSdkTool(definition, spec.toolRuntime)),
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

  /**
   * Cevap bekleyen onay item'lari (ASU-048).
   *
   * Neden burada: `session.approve/reject` SDK'nin `RunToolApprovalItem`'ini
   * ister, servis ise SDK tipi gormemeli. Esleme adaptorde kalir; disariya
   * yalnizca `requestId` cikar. Cevaplanan istek **silinir** — ayni onayla iki
   * kez calistirma yolu olmasin.
   */
  const pendingApprovalItems = new Map<string, RunToolApprovalItemOf>();

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
    const described = describeApprovalRequest(request);
    // Item, cevap verilene kadar burada tutulur: SDK tipi disari cikmasin diye
    // servis yalnizca kimligi gorur (`sdk-import-boundary.spec.ts`).
    pendingApprovalItems.set(described.requestId, request.approvalItem);
    onSignal({ type: 'tool_approval_requested', ...described });
  });
  session.on('error', (event) => {
    onSignal({ type: 'error', error: describeSessionError(event.error) });
  });

  /**
   * Kimligi item'a cevirir ve **tuketir**.
   *
   * Bilinmeyen kimlik hata: sessizce "tamam" donmek, onaylanmamis bir cagrinin
   * onaylandigi izlenimini verirdi. Ayni kimlik ikinci kez de bulunamaz.
   */
  const takeApprovalItem = (requestId: string): RunToolApprovalItemOf => {
    const item = pendingApprovalItems.get(requestId);
    if (item === undefined) {
      throw new AsunaRealtimeError({
        kind: 'session',
        cause: 'approval_request_unknown',
        message: `Bekleyen onay bulunamadi (\`${requestId}\`); istek zaten cevaplanmis olabilir.`,
        retryable: false,
      });
    }
    pendingApprovalItems.delete(requestId);
    return item;
  };

  return {
    connect: (options) => session.connect({ apiKey: options.apiKey }),
    close: () => {
      // Kapanan oturumun bekleyen onaylari cevaplanamaz; item'lari elde tutmak
      // yalnizca olu referans birikmesi olurdu.
      pendingApprovalItems.clear();
      // SDK'da `void` — `await` edilmez (voice.md Bolum 9 madde 4).
      session.close();
    },
    interrupt: () => {
      session.interrupt();
    },
    approve: async (requestId: string): Promise<void> => {
      const item = takeApprovalItem(requestId);
      // `alwaysApprove` bilerek verilmiyor: karar **cagri basinadir**,
      // "hepsine izin ver" MVP'de yok (phase-5.md ASU-048).
      await session.approve(item);
    },
    reject: async (requestId: string, reason?: string): Promise<void> => {
      const item = takeApprovalItem(requestId);
      // `alwaysReject` de yok: bir reddi kalici kurala cevirmek de cagri basi
      // karar ilkesini bozardi.
      await session.reject(item, reason === undefined ? {} : { message: reason });
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
   * Modele acilan tool'lar. Uretimde `asunaToolRegistry.list()` gecirilir
   * (ASU-047) — servis kendi listesini kurmaz, registry tek kaynaktir.
   * Verilmezse bos: tool'suz bir oturum gecerli bir durumdur (testler).
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
   * Aktif oturum kaydinin kimligini (`sessions.id`) cozer (ASU-048/050).
   *
   * Her tool cagrisinda ve her onay kararinda **yeniden** cagrilir; `null`
   * donerse audit satiri "hangi konusmada oldugunu bilmiyoruz" der. Uretimde
   * `SessionRecorder` besler, testlerde sabit deger verilir.
   */
  readonly resolveSessionId?: () => number | null;
  /**
   * Audit defterine yazan kanca (ASU-050). Varsayilan `recordToolEvent`
   * (`tools/audit.ts`) — asla firlatmaz, hatayi kendisi log'lar.
   */
  readonly recordToolEvent?: (input: ToolAuditInput) => void;
  /**
   * Tool bu oturumda **acik mi** (ASU-054)?
   *
   * Iki yerde kullanilir: (a) `connect()` sirasinda modele verilen liste
   * suzulur, (b) her cagrida `executeTool` kapisi yeniden sorar. Ikincisi acik
   * bir oturumun ortasinda kapatilan tool icin gerekli — SDK'ya verilen liste
   * o oturum boyunca sabittir.
   *
   * Verilmezse tum tool'lar acik sayilir (mevcut davranis degismez).
   */
  readonly isToolEnabled?: (toolName: string) => boolean;
  /**
   * Onay penceresi (ms). Varsayilan [`APPROVAL_TIMEOUT_MS`]; testler kisaltir.
   * Sure dolunca istek **reddedilir** — bu bir urun karari, kullanici tercihi degil.
   */
  readonly approvalTimeoutMs?: number;
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

/** Cevap bekleyen bir onay istegi (ASU-048). */
interface PendingApproval {
  readonly toolName: string;
  /** `null` = tool registry'de yok; audit satiri risk uydurmadan atlanir. */
  readonly risk: ToolRisk | null;
  /** Modelin urettigi ham argumanlar; audit'e ozetlenmek uzere host'a gider. */
  readonly rawArguments: unknown;
  readonly timer: ReturnType<typeof setTimeout>;
}

/** Audit ozetlerinde ve onay kartinda kullanilan deger kirpma siniri. */
const MAX_PREVIEW_VALUE_CHARS = 64;

/** Onay kartina giden ozetin tavani. */
const MAX_PREVIEW_CHARS = 240;

/** Ic ice yapilar **sekil** olarak yazilir; icerik hicbir zaman serilestirilmez. */
function describeValue(value: unknown): string {
  if (Array.isArray(value)) {
    return `[${value.length.toString()} oge]`;
  }
  if (typeof value === 'object' && value !== null) {
    return `{${Object.keys(value).length.toString()} alan}`;
  }
  if (typeof value === 'string') {
    return value.length > MAX_PREVIEW_VALUE_CHARS
      ? `${value.slice(0, MAX_PREVIEW_VALUE_CHARS - 1)}…`
      : value;
  }
  return String(value);
}

/**
 * Onay kartinda gosterilecek arguman ozeti (ASU-048 / ASU-053).
 *
 * `security.md` Bolum 3: onay istegi kullaniciya **ne yapilacagini** gosterir,
 * yalnizca "izin ver?" demez. Ozet Rust'taki audit redaksiyonuyla ayni ilkeleri
 * izler: tek satir, alfabetik `anahtar=deger`, uzun metin kirpilir, ic ice
 * yapilar yalnizca sekil olarak gorunur (dosya icerigi karta dokulmez) ve
 * sonuc `redactText` ile secret desenlerinden temizlenir.
 *
 * @returns `null` = gosterilecek arguman yok.
 */
export function toApprovalArgumentsPreview(argumentsJson: string | null): string | null {
  if (argumentsJson === null || argumentsJson.trim().length === 0) {
    return null;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(argumentsJson);
  } catch {
    // Model gecerli JSON uretmediyse metni **oldugu gibi** gostermiyoruz;
    // kirpilmis ve redakte edilmis haliyle gosteriyoruz.
    return truncatePreview(redactText(argumentsJson));
  }

  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    return truncatePreview(redactText(describeValue(parsed)));
  }

  const entries = Object.entries(parsed);
  if (entries.length === 0) {
    return null;
  }

  const line = entries
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => `${key}=${describeValue(value)}`)
    .join(', ');

  return truncatePreview(redactText(line));
}

function truncatePreview(text: string): string {
  return text.length <= MAX_PREVIEW_CHARS ? text : `${text.slice(0, MAX_PREVIEW_CHARS - 1)}…`;
}

/** Ham arguman metnini audit'e giden degere cevirir (host redakte eder). */
function parseRawArguments(argumentsJson: string | null): unknown {
  if (argumentsJson === null || argumentsJson.trim().length === 0) {
    return undefined;
  }
  try {
    return JSON.parse(argumentsJson);
  } catch {
    return argumentsJson;
  }
}

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

  private readonly resolveSessionId: () => number | null;

  private readonly recordToolEvent: (input: ToolAuditInput) => void;

  private readonly isToolEnabled: (toolName: string) => boolean;

  private readonly approvalTimeoutMs: number;

  /** Cevap bekleyen onay istekleri: `requestId` -> istek. */
  private readonly pendingApprovals = new Map<string, PendingApproval>();

  /**
   * Verilmis ama henuz kullanilmamis onaylar: tool adi -> adet.
   *
   * `executeTool` kapisi (ASU-048) onay **kanitini** burada arar. SDK akisinda
   * sira sudur: kullanici onaylar -> kanit yazilir -> `session.approve` tool
   * cagrisini tetikler -> `execute` -> kapi kaniti tuketir. Kanit yoksa kapi
   * reddeder; yani onay akisini atlayan bir cagri (SDK'nin politikasi yanlis
   * baglanmis olsa bile) calismaz.
   */
  private readonly approvalGrants = new Map<string, number>();

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
    this.resolveSessionId = options.resolveSessionId ?? ((): number | null => null);
    this.recordToolEvent =
      options.recordToolEvent ??
      ((input): void => {
        // `recordToolEvent` firlatmaz; hatayi kendisi log'lar ve `failed` doner.
        // Burada `void`: audit yazimi tool sonucunu ya da onay akisini bekletmez.
        void recordToolEvent(input);
      });
    this.isToolEnabled = options.isToolEnabled ?? ((): boolean => true);
    this.approvalTimeoutMs = options.approvalTimeoutMs ?? APPROVAL_TIMEOUT_MS;
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
    this.tokenError = null;
    this.publishedTranscripts.clear();
    // Bekleyen onaylar oturum **kapanmadan once** sonuclandirilir: reddi SDK'ya
    // iletmek icin oturuma hala ihtiyac var.
    this.abandonApprovals();
    this.session = null;

    if (session !== null) {
      this.reportUsage(session);
      this.closeSession(session);
    }

    this.applyTransition(this.idleState, 'SESSION_CLOSED_BY_USER', 'disconnect');
    this.publish({ type: 'disconnected', reason: 'requested' });
  }

  /**
   * Bekleyen bir tool onayini **onaylar** (ASU-048; UI'i ASU-053 kurar).
   *
   * Sirasi onemli: once onay kaniti yazilir, sonra SDK'ya haber verilir —
   * `session.approve()` tool cagrisini tetikleyebilir ve `executeTool` kapisi
   * kaniti hazir bulmali. SDK cagrisi patlarsa kanit **geri alinir**: yarim
   * kalmis bir onayin ilerideki bir cagriyi sessizce gecirmesi kabul edilemez.
   *
   * Bilinmeyen kimlik yok sayilmaz: `unexpected_signal` ile gorunur olur
   * (istek zaten zaman asimina ugramis ya da cevaplanmis olabilir).
   */
  public approveToolCall(requestId: string): void {
    const pending = this.takePendingApproval(requestId);
    if (pending === null) {
      this.publish({
        type: 'unexpected_signal',
        signal: `approve_tool_call:${requestId}`,
        state: this.stateMachine.getState(),
      });
      return;
    }

    this.grantApproval(pending.toolName);

    const session = this.session;
    if (session === null) {
      this.revokeApproval(pending.toolName);
      this.finishApproval(requestId, pending, 'denied', 'Oturum kapali; onay iletilemedi.');
      return;
    }

    void session.approve(requestId).catch((error: unknown) => {
      // Onay iletilemedi: kanit geri alinir, kullanici da durumu gorur.
      this.revokeApproval(pending.toolName);
      this.publish({ type: 'error', error: describeSessionError(error) });
    });

    // Audit satirini burada **yazmiyoruz**: onaylanan cagri calisacak ve
    // `executeTool` kendi satirini `approved` ile yazacak. Iki satir yazmak
    // deftere ayni olayi iki kez islemek olurdu.
    this.publish({
      type: 'tool_approval_resolved',
      requestId,
      toolName: pending.toolName,
      outcome: 'approved',
    });
  }

  /**
   * Bekleyen bir tool onayini **reddeder**.
   *
   * Red modele bildirilir (`reason`) ki Asuna "yaptim" demesin, ve audit'e
   * `denied` olarak yazilir — reddedilen cagri sessizce kaybolmaz
   * (`security.md` Bolum 3).
   */
  public rejectToolCall(requestId: string, reason?: string): void {
    const pending = this.takePendingApproval(requestId);
    if (pending === null) {
      this.publish({
        type: 'unexpected_signal',
        signal: `reject_tool_call:${requestId}`,
        state: this.stateMachine.getState(),
      });
      return;
    }

    const message = reason ?? TOOL_DENIED_MODEL_MESSAGE;
    this.sendRejection(requestId, message);
    this.finishApproval(requestId, pending, 'denied', message);
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

  // --- Onay akisi (ASU-048) --------------------------------------------

  /**
   * Tool'lara verilen calisma zamani baglantilari.
   *
   * Onay kapisi bir **kanit dogrulayicisidir**, onay isteyen taraf degil: istek
   * SDK akisindan cikar (`tool_approval_requested`), kanit `approveToolCall`
   * ile yazilir. Kanit yoksa cevap `denied` — varsayilan calistirmamaktir.
   */
  private get toolRuntime(): ToolRuntimeBindings {
    return {
      approvalMode: this.config.toolApprovalMode,
      approvalGate: this.approvalGate,
      onAudit: this.recordToolEvent,
      resolveSessionId: this.resolveSessionId,
      isToolEnabled: this.isToolEnabled,
      onToolResult: this.publishToolResult,
    };
  }

  /**
   * Cagri sonucunu UI'a duyurur (ASU-054).
   *
   * Ok fonksiyonu alan: `toolRuntime` getter'i her cagrida yeni bir nesne
   * uretiyor, ama bu referans sabit kaliyor.
   */
  private readonly publishToolResult = (report: ToolResultReport): void => {
    this.publish({
      type: 'tool_result',
      toolName: report.toolName,
      risk: report.risk,
      outcome: report.outcome,
      approvalState: report.approvalState,
      summary: report.summary,
    });
  };

  private readonly approvalGate: ToolApprovalGate = (definition): Promise<ApprovalOutcome> =>
    Promise.resolve(this.consumeApproval(definition.name) ? 'approved' : 'denied');

  /** Onay istegini kaydeder, geri sayimi baslatir ve UI'a duyurur. */
  private registerApproval(signal: {
    readonly requestId: string;
    readonly toolName: string;
    readonly argumentsJson: string | null;
  }): void {
    const { requestId, toolName, argumentsJson } = signal;

    // Ayni kimlik ikinci kez gelirse eski geri sayim birakilmaz.
    this.clearApprovalTimer(requestId);

    const definition = this.tools.find((candidate) => candidate.name === toolName) ?? null;
    const timer = setTimeout(() => {
      this.expireApproval(requestId);
    }, this.approvalTimeoutMs);

    this.pendingApprovals.set(requestId, {
      toolName,
      risk: definition?.risk ?? null,
      rawArguments: parseRawArguments(argumentsJson),
      timer,
    });

    this.publish({
      type: 'tool_approval_requested',
      requestId,
      toolName,
      description: definition?.description ?? '',
      risk: definition?.risk ?? null,
      argumentsPreview: toApprovalArgumentsPreview(argumentsJson),
      timeoutMs: this.approvalTimeoutMs,
    });
  }

  /** Sure doldu: **varsayilan reddet** (phase-5.md ASU-048). */
  private expireApproval(requestId: string): void {
    const pending = this.takePendingApproval(requestId);
    if (pending === null) {
      return;
    }
    this.sendRejection(requestId, TOOL_APPROVAL_TIMEOUT_MODEL_MESSAGE);
    this.finishApproval(requestId, pending, 'timeout', TOOL_APPROVAL_TIMEOUT_MODEL_MESSAGE);
  }

  /**
   * Calistirilmayan bir onayin defter kaydini yazar, durumu geri alir ve
   * sonucu duyurur.
   *
   * Audit yalnizca **calismayan** yollar icin burada yazilir; onaylanan cagri
   * kendi satirini `executeTool` icinde uretir.
   */
  private finishApproval(
    requestId: string,
    pending: PendingApproval,
    outcome: Exclude<ApprovalOutcome, 'approved'>,
    summary: string,
  ): void {
    this.writeApprovalAudit(pending, outcome, summary);
    // Calismayan cagri da **gorunur** olur (ASU-054): reddedilen bir aksiyonun
    // transcript'ten dusmesi, kullaniciyi "oldu mu, olmadi mi?" sorusuyla
    // birakirdi. Risk bilinmiyorsa uydurulmuyor (`null`).
    this.publish({
      type: 'tool_result',
      toolName: pending.toolName,
      risk: pending.risk,
      outcome: 'not_run',
      approvalState: outcome,
      summary,
    });
    // Reddedilen tool calismaz: model cevabina doner (durum tablosunda
    // `AWAITING_APPROVAL -> ASSISTANT_THINKING` "reddedildi" kenari).
    this.applyTransition('ASSISTANT_THINKING', 'TOOL_CALL_COMPLETED', 'tool_approval_resolved');
    this.publish({
      type: 'tool_approval_resolved',
      requestId,
      toolName: pending.toolName,
      outcome,
    });
  }

  private writeApprovalAudit(
    pending: PendingApproval,
    approvalState: Exclude<ApprovalOutcome, 'approved'>,
    summary: string,
  ): void {
    const risk = pending.risk;
    if (risk === null) {
      // Risk seviyesi bilinmeyen bir cagri icin sayi uydurmuyoruz; olay
      // sessiz de kalmiyor.
      toolLogger.warn(
        `\`${pending.toolName}\` icin onay reddedildi ama tool kayitli degil; audit satiri yazilmadi.`,
        { tool: pending.toolName, approvalState },
      );
      return;
    }

    const sessionId = this.resolveSessionId();
    this.recordToolEvent({
      toolName: pending.toolName,
      riskLevel: risk,
      ...(sessionId === null ? {} : { sessionId }),
      ...(pending.rawArguments === undefined ? {} : { arguments: pending.rawArguments }),
      approvalState,
      resultSummary: summary,
      // Onay alinamadi: `execute` **hic** cagrilmadi, yan etki ihtimali yok.
      outcome: 'not_run',
    });
  }

  /** SDK'ya reddi bildirir; model reddi ogrenmeli ki "yaptim" demesin. */
  private sendRejection(requestId: string, message: string): void {
    const session = this.session;
    if (session === null) {
      return;
    }
    void session.reject(requestId, message).catch((error: unknown) => {
      this.publish({ type: 'error', error: describeSessionError(error) });
    });
  }

  private takePendingApproval(requestId: string): PendingApproval | null {
    const pending = this.pendingApprovals.get(requestId);
    if (pending === undefined) {
      return null;
    }
    clearTimeout(pending.timer);
    this.pendingApprovals.delete(requestId);
    return pending;
  }

  private clearApprovalTimer(requestId: string): void {
    const pending = this.pendingApprovals.get(requestId);
    if (pending !== undefined) {
      clearTimeout(pending.timer);
      this.pendingApprovals.delete(requestId);
    }
  }

  /**
   * Oturum kapaniyor: bekleyen istekler cevaplanamaz.
   *
   * Sessizce dusurulmuyor — her biri deftere `denied` olarak yazilir ve UI'a
   * sonuc duyurulur ki onay karti ekranda asili kalmasin. Verilmis ama
   * kullanilmamis kanitlar da silinir: bir sonraki oturuma tasinan onay olmaz.
   */
  private abandonApprovals(): void {
    for (const [requestId, pending] of [...this.pendingApprovals]) {
      clearTimeout(pending.timer);
      this.pendingApprovals.delete(requestId);
      this.writeApprovalAudit(pending, 'denied', TOOL_APPROVAL_ABANDONED_SUMMARY);
      this.publish({
        type: 'tool_approval_resolved',
        requestId,
        toolName: pending.toolName,
        outcome: 'denied',
      });
    }
    this.approvalGrants.clear();
  }

  private grantApproval(toolName: string): void {
    this.approvalGrants.set(toolName, (this.approvalGrants.get(toolName) ?? 0) + 1);
  }

  private revokeApproval(toolName: string): void {
    const count = this.approvalGrants.get(toolName) ?? 0;
    if (count <= 1) {
      this.approvalGrants.delete(toolName);
      return;
    }
    this.approvalGrants.set(toolName, count - 1);
  }

  /** Kaniti **tuketir**: bir onay tek bir cagriyi gecirir. */
  private consumeApproval(toolName: string): boolean {
    const count = this.approvalGrants.get(toolName) ?? 0;
    if (count <= 0) {
      return false;
    }
    this.revokeApproval(toolName);
    return true;
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
      // ASU-054: kapali tool modele **verilmez**. Liste oturum acilisinda
      // donduruluyor (SDK'ya verilen tool seti oturum boyunca sabit), yani
      // oturum ortasinda yapilan bir kapatma modelin listesini degistirmez —
      // o cagriyi `executeTool` kapisi reddeder ve deftere yazar.
      tools: this.tools.filter((definition) => this.isToolEnabled(definition.name)),
      toolRuntime: this.toolRuntime,
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

      // ASU-047'den beri registry'de tool var; bu sinyaller gercekten geliyor.
      // Onay akisi (AWAITING_APPROVAL) ASU-048/053 ile doldurulacak.
      case 'tool_start':
        this.applyTransition('TOOL_PENDING', 'TOOL_CALL_STARTED', signal.type);
        this.publish({ type: 'tool_call_started', toolName: signal.toolName });
        return;

      case 'tool_end':
        this.applyTransition('ASSISTANT_THINKING', 'TOOL_CALL_COMPLETED', signal.type);
        this.publish({ type: 'tool_call_completed', toolName: signal.toolName });
        return;

      case 'tool_approval_requested':
        // Onay bekleyen tool oturumu **gorunur** sekilde bekletir: kullanici
        // karar verene kadar Asuna "yaptim" diyemez (ASU-048).
        this.applyTransition('AWAITING_APPROVAL', 'TOOL_APPROVAL_REQUESTED', signal.type);
        this.registerApproval(signal);
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
      // Ayirac `\x1f` (unit separator) ve **kacis dizisi olarak** yazili:
      // ham bir kontrol baytini kaynaga gommek dosyayi `grep`/`file` icin
      // ikili gosterir ve taramalar onu sessizce atlar. Calisma zamaninda
      // uretilen metin ayni; secim yalnizca kaynagin duz metin kalmasi icin.
      const fingerprint = `${entry.status}\x1f${entry.text}`;
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

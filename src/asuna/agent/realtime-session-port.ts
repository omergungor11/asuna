/**
 * `AsunaRealtimeService` ile OpenAI Agents SDK arasindaki **tek** sinir (ASU-013).
 *
 * Servis yalnizca bu port'u tanir; gercek implementasyon `realtime-service.ts` icindeki
 * `createOpenAiRealtimeSession` fabrikasidir. Testler ayni port'u sahte bir nesneyle
 * karsilar — bu sayede yasam dongusu ve event -> durum eslemeleri **ag olmadan** test
 * edilir (`conventions.md` — "Harici servisler mock'lanir").
 *
 * Bu dosyada SDK importu **yoktur**.
 */

import type { AsunaRealtimeErrorInfo } from './realtime-errors';
import type { RealtimeUsageSnapshot, TranscriptEntry } from './realtime-events';
import type { ToolApprovalMode, VadEagerness } from '../config/frontend-config';
import type { ToolApprovalGate, ToolResultReport } from '../tools/registry';
import type { AsunaToolDefinition } from '../tools/types';
import type { ToolAuditInput } from '../../shared/tool-event';

/**
 * Tool cagrilarinin calisma zamani baglantilari (ASU-048 / ASU-050).
 *
 * Tool tanimi **ne yapacagini** bilir; bu nesne onu **kime karsi** yaptigini
 * baglar: hangi onay modu gecerli, onay kimden istenecek, audit hangi oturuma
 * yazilacak. Ikisini ayirmanin sebebi tanimlarin duz veri kalmasi — bir tool
 * tanimi oturuma, IPC'ye ve SDK'ya bagimsiz olarak test edilebilmeli.
 */
export interface ToolRuntimeBindings {
  /** `ASUNA_TOOL_APPROVAL_MODE`; risk 2/3 bu degerle gevsetilemez. */
  readonly approvalMode: ToolApprovalMode;
  /**
   * Onay kanali. Verilmezse onay gerektiren tool **calismaz** — kapinin
   * bagli olmamasi "onaysiz calistir" demek degildir.
   */
  readonly approvalGate?: ToolApprovalGate;
  /**
   * Audit defterine yazan kanca (ASU-050). Verilmezse cagri deftere islenmez;
   * uretimde her zaman baglidir.
   */
  readonly onAudit?: (input: ToolAuditInput) => void;
  /**
   * Aktif oturum kaydinin kimligi (`sessions.id`). Her cagrida **yeniden**
   * sorulur: `session_start` asenkron doner, yani oturumun ilk saniyelerinde
   * kimlik henuz yoktur. `null` = bilinmiyor (hafiza kapali ya da kayit
   * acilmadi) — uydurulmaz.
   */
  readonly resolveSessionId?: () => number | null;
  /**
   * Tool bu oturumda **acik mi** (ASU-054)?
   *
   * Verilmezse tum tool'lar acik sayilir. Kapali bir tool modele verilen
   * listeden zaten dusurulur; bu kanca o listenin eskidigi ani (acik oturumun
   * ortasinda kapatma) yakalar. Gizli bir calisma yolu birakmamak icin
   * `executeTool` her cagrida yeniden sorar.
   */
  readonly isToolEnabled?: (toolName: string) => boolean;
  /**
   * Cagri bittiginde (calissin ya da calismasin) cagrilan kanca — ASU-054
   * transcript satirinin kaynagi.
   *
   * Audit'ten **ayri**: audit kalici deftere yazar ve hafiza kapaliyken hic
   * yazilmaz; bu kanca canli gorunurluk icin her zaman calisir.
   */
  readonly onToolResult?: (report: ToolResultReport) => void;
}

/**
 * Tur tespiti ayari (ASU-064) — SDK tipi degil, duz veri.
 *
 * Ayrik birlesim bilincli: `eagerness` yalnizca `semantic_vad`'de,
 * `silenceDurationMs` yalnizca `server_vad`'de anlamli. Ikisini ayni nesnede
 * opsiyonel alan olarak tutmak, ilgisiz bir alani sessizce SDK'ya gondermeye
 * davetiye cikarirdi.
 */
export type TurnDetectionSpec =
  | { readonly type: 'semantic_vad'; readonly eagerness: VadEagerness }
  | { readonly type: 'server_vad'; readonly silenceDurationMs: number };

/**
 * Oturumun acilis parametreleri. Model ID burada bir *degerdir*, sabit degil —
 * `ASUNA_REALTIME_MODEL` config'inden gelir (hard-code yasagi).
 */
export interface RealtimeSessionSpec {
  /** `buildAsunaInstructions(context)` ciktisi. */
  readonly instructions: string;
  readonly model: string;
  /** `ASUNA_REALTIME_VOICE`; `null` = SDK varsayilani. */
  readonly voice: string | null;
  /**
   * Kullanici sesinin transkript edilip edilmeyecegi (`ASUNA_TRANSCRIPT_STORAGE`).
   * `false` ise transkripsiyon **kapatilir** — hem maliyet hem gizlilik (voice.md Bolum 2).
   */
  readonly transcription: boolean;
  /**
   * Tur tespiti ayari (`ASUNA_TURN_DETECTION` + `ASUNA_VAD_*`). Sabit degil:
   * gecikme/erken-kesme takasi rebuild olmadan env ile ayarlanabilir (ASU-064).
   */
  readonly turnDetection: TurnDetectionSpec;
  /**
   * Modele acilan tool'lar. ASU-044'ten beri dolu (`get_current_project`, risk 0);
   * SDK `tool()` cevrimi `realtime-service.ts` icinde yapilir — SDK tipi bu
   * dosyaya girmez.
   */
  readonly tools: readonly AsunaToolDefinition[];
  /**
   * Tool'larin onay/audit/oturum baglantilari (ASU-048). Adaptor bunu SDK
   * `tool()` cevriminde kullanir; tanimlarin kendisi bu bilgiyi tasimaz.
   */
  readonly toolRuntime: ToolRuntimeBindings;
}

/**
 * SDK event'lerinin ham veri karsiligi. Isimlendirme bilerek SDK event adlarini izler
 * (voice.md Bolum 3 tablosu) ki eslemenin dogrulugu tek bakista gorulebilsin; tasidiklari
 * ise duz veridir.
 */
export type RealtimeSessionSignal =
  /** SDK `agent_start`. */
  | { readonly type: 'agent_start' }
  /** SDK `agent_end`. */
  | { readonly type: 'agent_end' }
  /** SDK `audio_start`. */
  | { readonly type: 'audio_start' }
  /** SDK `audio_stopped`. */
  | { readonly type: 'audio_stopped' }
  /** SDK `audio_interrupted` (barge-in). */
  | { readonly type: 'audio_interrupted' }
  /** SDK `history_updated` / `history_added` — normalize edilmis dokum satirlari. */
  | { readonly type: 'history'; readonly entries: readonly TranscriptEntry[] }
  /** SDK `agent_tool_start` — Phase 5. */
  | { readonly type: 'tool_start'; readonly toolName: string }
  /** SDK `agent_tool_end` — Phase 5. */
  | { readonly type: 'tool_end'; readonly toolName: string }
  /**
   * SDK `tool_approval_requested` — onay bekleyen tool cagrisi (ASU-048).
   *
   * `requestId` SDK'nin onay item'ini **temsil eder**, kendisi degil: SDK tipi
   * bu dosyaya girmez. Adaptor kimlik -> item eslemesini kendi icinde tutar ve
   * [`RealtimeSessionPort.approve`] / `reject` bu kimligi kabul eder.
   *
   * `argumentsJson` modelin urettigi ham arguman metni (ya da `null`). Onay
   * karti "ne yapilacagini" gostermek zorunda (`security.md` Bolum 3);
   * gosterilecek metin redakte edilerek `realtime-service.ts` icinde uretilir.
   */
  | {
      readonly type: 'tool_approval_requested';
      readonly toolName: string;
      readonly requestId: string;
      readonly argumentsJson: string | null;
    }
  /** SDK `error`. */
  | { readonly type: 'error'; readonly error: AsunaRealtimeErrorInfo };

export type RealtimeSessionSignalListener = (signal: RealtimeSessionSignal) => void;

/** Kisa omurlu `ek_` token'i uretecek lazy fonksiyon (voice.md Bolum 5). */
export type EphemeralApiKeyProvider = () => Promise<string>;

/** Servisin gordugu oturum yuzeyi. */
export interface RealtimeSessionPort {
  /**
   * Baglanti kurar. `apiKey` **lazy**: SDK token'i tam ihtiyac aninda ister, boylece
   * yeniden denemelerde taze token uretilir ve token gereksiz yere bellekte durmaz.
   */
  connect(options: { readonly apiKey: EphemeralApiKeyProvider }): Promise<void>;
  /** SDK'da `void` — `await` edilmez (voice.md Bolum 9 madde 4). */
  close(): void;
  /** Manuel "sus": uretilen yaniti keser. */
  interrupt(): void;
  /**
   * Bekleyen bir tool onayini onaylar (ASU-048). Onaydan **sonra** tool
   * calisir; yani bu cagri "izin verildi" demek, "yapildi" demek degil.
   *
   * Bilinmeyen/tuketilmis bir kimlik icin reddeder (`Promise.reject`): sessizce
   * basarili donmek, onaylanmamis bir cagrinin onaylandigini sanmak olurdu.
   */
  approve(requestId: string): Promise<void>;
  /**
   * Bekleyen bir tool onayini reddeder. `reason` modele giden aciklamadir —
   * model reddi ogrenmeli ki "yaptim" demesin (PROJECT.md Bolum 30).
   */
  reject(requestId: string, reason?: string): Promise<void>;
  /** Anlik token kullanimi. Kapanistan hemen once okunur. */
  usage(): RealtimeUsageSnapshot;
}

/**
 * Oturum fabrikasi. Ortam desteklemiyorsa (WebRTC yok) **kurucu asamasinda** hata
 * firlatabilir; servis bunu yakalayip `ERROR` durumuna cevirir.
 */
export type RealtimeSessionFactory = (
  spec: RealtimeSessionSpec,
  onSignal: RealtimeSessionSignalListener,
) => RealtimeSessionPort;

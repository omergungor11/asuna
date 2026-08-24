# Voice Architecture — OpenAI Agents SDK Realtime

> Kaynak: ASU-006 araştırma task'ı. Tüm bulgular 2026-08-24'te doğrulandı.
> Doğrulama yöntemi: npm registry + `@openai/agents-realtime@0.17.0` paketinin
> yayınlanmış `.d.ts`/`.mjs` dosyaları (yerel tarball, ground truth) + resmi docs.
> "Eğitim verisi" kaynaklı hiçbir iddia yok.

## 1. Paket ve sürüm pinleme

| Paket                     | Pin                     | Yayın tarihi | Lisans |
| ------------------------- | ----------------------- | ------------ | ------ |
| `@openai/agents-realtime` | `0.17.0`                | 2026-08-19   | MIT    |
| `zod`                     | `4.4.3` (peer `^4.0.0`) | —            | MIT    |

Bağımlılık zinciri (`npm view @openai/agents-realtime@0.17.0 dependencies`):

```
@openai/agents-realtime@0.17.0
├── @openai/agents-core@0.17.0   (exact pin — SDK kendi içinde exact pinliyor)
│   ├── openai@^7.2.0
│   └── @standard-schema/spec@^1.1.0
├── ws@^8.21.0
├── @types/ws@^8.18.1
└── debug@^4.4.0
peerDependencies: zod@^4.0.0
```

### Neden `@openai/agents` değil, `@openai/agents-realtime`

Resmi quickstart (openai-agents-js repo, `docs/.../voice-agents/quickstart.mdx`):

> "This quickstart uses `@openai/agents`, which is the recommended default for most apps.
> If you prefer the standalone Realtime package, install `@openai/agents-realtime` and
> replace imports from `@openai/agents/realtime` with `@openai/agents-realtime`."

Asuna renderer'ı **sadece** realtime kullanıyor. `@openai/agents` ek olarak
`@openai/agents-openai` (Responses/Chat Completions modelleri) çekiyor ve bunlar
Asuna'nın renderer bundle'ında hiç kullanılmayacak. `agents-core` → `openai@^7.2.0`
her iki yolda da geliyor, ondan kaçış yok.

**Import kuralı:** tüm realtime import'ları `@openai/agents-realtime`'dan.
`tool`, `FunctionTool`, `UserError` de bu paketten re-export ediliyor
(`dist/index.d.ts` son satırları) — `@openai/agents-core`'a doğrudan import etmeye
gerek yok.

### Sürüm hızı riski

`@openai/agents` sürüm geçmişi (npm `time` alanı): 0.14.0 → 0.17.0 arası
2026-07-28 ile 2026-08-19 arasında, yani **3 haftada 3 minor**. Bu SDK 1.0 öncesi ve
minor'larda realtime davranışı değişiyor:

- **0.15.0**: browser WebRTC transport artık `close()`'da caller'ın verdiği
  `mediaStream` track'lerini durdurmuyor (davranış değişikliği — mikrofon açık kalır)
- **0.16.0**: geçersiz audio rate'ler artık sessizce yutulmuyor, hata veriyor
- **0.17.0**: realtime'a özel breaking change yok (output guardrail odaklı)

**Karar:** `package.json`'da caret **YOK** — exact pin (`"@openai/agents-realtime": "0.17.0"`).
Yükseltme ayrı bir task olarak, release notes okunarak yapılır.

### Runtime gereksinimleri

- **Node.js 22+** (openai-agents-js README, "Requirements" bölümü). Repo
  `package.json` `engines.node: ">=22.12.0"` — uyumlu.
- Paketlerde `engines` alanı **yok** — pnpm bunu zorlamayacak, CI'da kontrol edilmeli.
- Zod **4.x zorunlu** (peer `^4.0.0`). Zod 3 ile çalışmaz.

---

## 2. Gerçek API imzaları (`0.17.0` `.d.ts`'inden birebir)

### `RealtimeAgent`

```ts
// dist/realtimeAgent.d.ts
export type RealtimeAgentConfiguration<TContext = UnknownContext> =
  Partial<Omit<AgentConfiguration<RealtimeContextData<TContext>, TextOutput>,
    'model' | 'handoffs' | 'modelSettings' | 'outputType' | 'toolUseBehavior'
    | 'resetToolChoice' | 'outputGuardrails' | 'inputGuardrails'>> & {
  name: string;
  handoffs?: (RealtimeAgent<TContext> | Handoff<...>)[];
  voice?: string;
};

export declare class RealtimeAgent<TContext = UnknownContext>
  extends Agent<RealtimeContextData<TContext>, TextOutput> {
  readonly voice?: string;
  constructor(config: RealtimeAgentConfiguration<TContext>);
}
```

Kullanılabilir alanlar: `name` (zorunlu), `instructions` (string | fonksiyon),
`tools`, `handoffs`, `voice`.
**`model` RealtimeAgent'ta YOK** — model session seviyesinde. Aynı şekilde
`modelSettings`, `outputType`, `toolUseBehavior` da desteklenmiyor (tip düzeyinde `Omit`'lenmiş).

### `RealtimeSession`

```ts
// dist/realtimeSession.d.ts
export type RealtimeSessionOptions<TContext = unknown> = {
  apiKey: ApiKey;                                   // string | (() => string | Promise<string>)
  transport: 'webrtc' | 'websocket' | RealtimeTransportLayer;
  model?: OpenAIRealtimeModels | (string & {});
  context?: TContext;
  outputGuardrails?: RealtimeOutputGuardrail[];
  outputGuardrailSettings?: RealtimeOutputGuardrailSettings;
  config?: Partial<RealtimeSessionConfig>;
  toolExecution?: RealtimeToolExecutionConfig;      // { preApprovalInputGuardrails?: boolean }
  historyStoreAudio?: boolean;                      // default false
  tracingDisabled?: boolean;
  groupId?: string;
  traceMetadata?: Record<string, any>;
  workflowName?: string;
  automaticallyTriggerResponseForMcpToolCalls?: boolean;
  toolErrorFormatter?: ToolErrorFormatter<RealtimeContextData<TContext>>;
};

export type RealtimeSessionConnectOptions = {
  apiKey: string | (() => string | Promise<string>);   // ZORUNLU
  model?: OpenAIRealtimeModels | (string & {});
  url?: string;
  callId?: string;                                     // sadece SIP
};

export declare class RealtimeSession<TBaseContext = unknown>
  extends RuntimeEventEmitter<RealtimeSessionEventTypes<TBaseContext>> {
  constructor(
    initialAgent: RealtimeAgent<TBaseContext> | RealtimeAgent<RealtimeContextData<TBaseContext>>,
    options?: Partial<RealtimeSessionOptions<TBaseContext>>   // <- Partial, hepsi opsiyonel
  );

  get transport(): RealtimeTransportLayer;
  get currentAgent(): RealtimeAgent<...>;
  get usage(): Usage;
  get context(): RunContext<RealtimeContextData<TBaseContext>>;
  get muted(): boolean | null;          // WebSocket transport'ta null
  get history(): RealtimeItem[];
  get availableMcpTools(): RealtimeMcpToolInfo[];

  connect(options: RealtimeSessionConnectOptions): Promise<void>;
  close(): void;                        // Promise DEĞİL — void
  interrupt(): void;
  mute(muted: boolean): void;
  sendMessage(message: RealtimeUserInput, otherEventData?: Record<string, any>): void;
  sendAudio(audio: ArrayBuffer, options?: { commit?: boolean }): void;
  addImage(image: string, opts?: { triggerResponse?: boolean }): void;
  updateHistory(newHistory: RealtimeItem[] | ((h: RealtimeItem[]) => RealtimeItem[])): void;
  updateAgent(newAgent: RealtimeAgent<TBaseContext>): Promise<RealtimeAgent<TBaseContext>>;
  approve(approvalItem: RunToolApprovalItem, options?: { alwaysApprove?: boolean }): Promise<void>;
  reject(approvalItem: RunToolApprovalItem,
         options?: { alwaysReject?: boolean; message?: string }): Promise<void>;

  getInitialSessionConfig(overrides?: Partial<RealtimeSessionConfig>): Promise<Partial<RealtimeSessionConfig>>;
  static computeInitialSessionConfig(...): Promise<Partial<RealtimeSessionConfig>>;
}
```

### Session config (`options.config`)

```ts
// dist/clientMessages.d.ts — YENİ (GA) şekil, Asuna bunu kullanmalı
type RealtimeSessionConfigDefinition = {
  model: string;
  instructions: string;
  toolChoice: ModelSettingsToolChoice;
  tools: RealtimeToolDefinition[];
  parallelToolCalls?: boolean;
  reasoning?: { effort?: 'minimal'|'low'|'medium'|'high'|'xhigh' };
  tracing?: RealtimeTracingConfig | null;
  providerData?: Record<string, any>;
  prompt?: Prompt;
  outputModalities?: ('text' | 'audio')[];
  audio?: {
    input?: {
      format?: { type: 'audio/pcm'; rate: number } | { type: 'audio/pcmu' } | { type: 'audio/pcma' };
      noiseReduction?: { type: 'near_field' | 'far_field' } | null;
      transcription?: { model?: string; delay?: 'minimal'|'low'|'medium'|'high'|'xhigh';
                        prompt?: string; keywords?: string[];
                        language?: string; languages?: string[] } | null;
      turnDetection?: RealtimeTurnDetectionConfig | null;
    } | null;
    output?: { format?: ...; voice?: string; speed?: number } | null;
  };
  voice?: string;   // geriye uyum; audio.output.voice tercih edilmeli
};
```

**DEPRECATED (kullanma):** `modalities`, `inputAudioFormat`, `outputAudioFormat`,
`inputAudioTranscription`, `turnDetection` (top-level), `inputAudioNoiseReduction`,
`speed` (top-level). Ayrıca string audio format kısayolları (`'pcm16'`) `.d.ts`'te
`@deprecated` işaretli — `{ type: 'audio/pcm', rate: 24000 }` kullan.
(Not: resmi `configureSession.ts` örneği hâlâ `'pcm16'` kullanıyor — docs örneği
tip tanımının gerisinde. **Tip tanımına uy.**)

### SDK varsayılan session config (`DEFAULT_OPENAI_REALTIME_SESSION_CONFIG`)

`dist/openaiRealtimeBase.mjs` içinden birebir:

```js
export const DEFAULT_OPENAI_REALTIME_SESSION_CONFIG = {
  outputModalities: ['audio'],
  audio: {
    input: {
      format: { type: 'audio/pcm', rate: 24000 },
      transcription: { model: 'gpt-4o-mini-transcribe' },
      turnDetection: { type: 'semantic_vad' },
      noiseReduction: null,
    },
    output: { format: { type: 'audio/pcm', rate: 24000 }, speed: 1 },
  },
};
```

Asuna için ilgili sonuçlar:

- Kullanıcı transkripsiyonu **varsayılan olarak açık** (`gpt-4o-mini-transcribe`).
  Bu ekstra maliyet ve `ASUNA_TRANSCRIPT_STORAGE=false` durumunda **istenmeyebilir** —
  kapatmak için `audio.input.transcription: null`.
- `noiseReduction: null` varsayılan. Masaüstü mikrofon için `{ type: 'near_field' }`
  denemeye değer (Phase 1 kalite testi).
- Türkçe için `audio.input.transcription.languages: ['tr']` veya
  `language: 'tr'` (model'e göre) — transkript kalitesini artırır.

### Model ID union (SDK'nın tanıdığı isimler)

```ts
// dist/openaiRealtimeBase.d.ts:11
export type OpenAIRealtimeModels =
  | 'gpt-realtime'
  | 'gpt-realtime-1.5'
  | 'gpt-realtime-2'
  | 'gpt-realtime-2.1'
  | 'gpt-realtime-2.1-mini'
  | 'gpt-realtime-2025-08-28'
  | 'gpt-4o-realtime-preview'
  | 'gpt-4o-realtime-preview-2024-10-01'
  | 'gpt-4o-realtime-preview-2024-12-17'
  | 'gpt-4o-realtime-preview-2025-06-03'
  | 'gpt-4o-mini-realtime-preview'
  | 'gpt-4o-mini-realtime-preview-2024-12-17'
  | 'gpt-realtime-mini'
  | 'gpt-realtime-mini-2025-10-06'
  | 'gpt-realtime-mini-2025-12-15'
  | (string & {}); // <- serbest string kabul ediliyor
```

```js
// dist/openaiRealtimeBase.mjs:13
export const DEFAULT_OPENAI_REALTIME_MODEL = 'gpt-realtime-2.1';
```

`(string & {})` sayesinde `ASUNA_REALTIME_MODEL` env'inden gelen serbest string
tip hatası vermez — model ID'nin hard-code edilmemesi kuralı SDK tarafında engellenmiyor.

---

## 3. Event listesi (`RealtimeSessionEventTypes`)

`session.on(name, handler)` ile dinlenir. Handler argümanları birebir:

| Event                                           | Argümanlar                                                         | Asuna'da kullanım                                        |
| ----------------------------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------- |
| `agent_start`                                   | `(context, agent, turnInput?)`                                     | `ASSISTANT_THINKING`                                     |
| `agent_end`                                     | `(context, agent, output: string)`                                 | tur bitişi                                               |
| `agent_handoff`                                 | `(context, fromAgent, toAgent)`                                    | MVP'de kullanılmıyor                                     |
| `agent_tool_start`                              | `(context, agent, tool, { toolCall })`                             | `TOOL_PENDING`, `tool_events` yazımı                     |
| `agent_tool_end`                                | `(context, agent, tool, result: string, { toolCall })`             | `tool_events` sonucu                                     |
| `audio_start`                                   | `(context, agent)`                                                 | `ASSISTANT_SPEAKING`                                     |
| `audio_stopped`                                 | `(context, agent)`                                                 | `LISTENING`'e dönüş                                      |
| `audio`                                         | `(event: { type:'audio'; data: ArrayBuffer; responseId: string })` | **WebRTC'de tetiklenmez** (transport sesi kendi yönetir) |
| `audio_interrupted`                             | `(context, agent)`                                                 | barge-in görsel göstergesi                               |
| `history_updated`                               | `(history: RealtimeItem[])`                                        | tam transkript snapshot'ı                                |
| `history_added`                                 | `(item: RealtimeItem)`                                             | artımlı transkript                                       |
| `tool_approval_requested`                       | `(context, agent, request)`                                        | `AWAITING_APPROVAL`                                      |
| `guardrail_tripped`                             | `(context, agent, error, { itemId })`                              | MVP'de opsiyonel                                         |
| `error`                                         | `(error: { type:'error'; error: unknown })`                        | `ERROR` state                                            |
| `mcp_tool_call_completed` / `mcp_tools_changed` | —                                                                  | MVP'de kullanılmıyor                                     |

**Transkript nereden alınır:** `history_updated` / `history_added` içindeki
`RealtimeItem`. Mesaj item'ı şekli (zod şemasından):

- user: `{ itemId, type:'message', role:'user', status, content: [{type:'input_audio', transcript: string|null, audio?}] }`
- assistant: `{ itemId, type:'message', role:'assistant', status, content: [{type:'output_audio', transcript?: string|null}] }`
- tool: `{ itemId, type:'function_call', status, name, arguments: string, output: string|null }`

Daha düşük seviye, delta bazlı akış isteniyorsa `session.transport.on(...)`:
`audio_transcript_delta` → `{ type:'transcript_delta', itemId, delta, responseId }`,
`output_text_delta`, `function_call`, `'*'` (ham JSON).

**`transport_event`** session seviyesinde ham transport event'lerini yayar
(`session.on('transport_event', ...)`).

---

## 4. Transport: WebRTC

### Seçim mantığı (koddan, `dist/realtimeSession.mjs:118-128`)

```js
if (
  (typeof options.transport === 'undefined' && hasWebRTCSupport()) ||
  options.transport === 'webrtc'
) {
  this.#transport = new OpenAIRealtimeWebRTC();
} else if (options.transport === 'websocket' || typeof options.transport === 'undefined') {
  this.#transport = new OpenAIRealtimeWebSocket();
} else {
  this.#transport = options.transport; // custom RealtimeTransportLayer
}
```

`hasWebRTCSupport()` (`dist/utils.mjs:95`):

```js
export function hasWebRTCSupport() {
  ...
  return typeof window['RTCPeerConnection'] !== 'undefined';
}
```

**Sonuç:** Tauri WKWebView'ında `window.RTCPeerConnection` varsa WebRTC **otomatik**
seçilir; `transport` opsiyonu vermeye gerek yok. Yine de Asuna açık yazmalı
(`transport: 'webrtc'`) — sessiz WebSocket fallback'i olmasın.

WebSocket'e geçmek: `new RealtimeSession(agent, { transport: 'websocket' })` veya
`new OpenAIRealtimeWebSocket()` örneği ver. **WebSocket'te ses capture/playback
tamamen uygulamaya kalır** ve `session.mute()` throw eder, `session.muted` `null` döner.

### WebRTC bağlantı akışı (koddan)

1. `new RTCPeerConnection()`, `createDataChannel('oai-events')`
2. `<audio autoplay>` elementi oluştur (veya `options.audioElement`), `ontrack` → `srcObject`
3. `navigator.mediaDevices.getUserMedia({ audio: true })` (veya `options.mediaStream`)
4. `peerConnection.addTrack(stream.getAudioTracks()[0])`
5. `createOffer()` → `setLocalDescription()`
6. `POST https://api.openai.com/v1/realtime/calls`
   - `Content-Type: application/sdp`
   - `Authorization: Bearer <ephemeral ek_ token>`
   - `X-OpenAI-Agents-SDK: <sdk meta>`
   - body: ham SDP offer
7. `callId` yanıtın `Location` header'ının son segmentinden alınır
8. Data channel açılınca `session.update` gönderilir, `session.updated` ack'i beklenir
   (timeout fallback'i var), sonra `connect()` resolve olur

**Model URL'de taşınmıyor.** `connectionUrl` üzerinde `searchParams` set edilmiyor —
model hem client secret'ın `session` payload'ından hem de data channel üzerinden giden
`session.update`'ten geliyor. → Rust tarafında token basarken kullanılan model ile
renderer'daki `RealtimeSession({ model })` **aynı olmalı**; model oturum ortasında
değiştirilemiyor (Realtime API kuralı).

### Ephemeral key zorunluluğu (SDK içi guard)

`dist/openaiRealtimeWebRtc.mjs:150-155` — birebir:

```js
const isClientKey = typeof apiKey === 'string' && apiKey.startsWith('ek_');
if (isBrowserEnvironment() && !this.#useInsecureApiKey && !isClientKey) {
  releaseConnectionAttempt();
  rejectConnection(
    new UserError(
      'Using the WebRTC connection in a browser environment requires an ephemeral client key. ' +
        'If you need to use a regular API key, use the WebSocket transport or set the ' +
        '`useInsecureApiKey` option to true.',
    ),
  );
  return;
}
```

**Bu Asuna için iyi haber:** kalıcı `sk-...` key'i renderer'a sızdırma hatası
runtime'da anında yakalanır. `useInsecureApiKey` **hiçbir koşulda** kullanılmayacak —
lint kuralı / code review kontrolü olarak yazılmalı.

### `mediaStream` sahipliği (0.15.0 davranış değişikliği — DİKKAT)

`.d.ts` yorumundan birebir:

> "A stream you pass here stays owned by your application. `close()` will not stop its
> tracks... otherwise the microphone stays open after the session ends. A microphone the
> transport opens for you because this option is omitted is still stopped by `close()`."

Asuna'da wake-word motoru ile Realtime session mikrofonu paylaşacak (OQ-6). İki seçenek:

- **A (basit):** `mediaStream` verme, SDK kendi açsın. `session.close()` mikrofonu
  kapatır → `IDLE_WAKE_WORD`'e dönüşte wake-word motoru mikrofonu geri alır.
- **B (kontrollü):** Uygulama tek `MediaStream` tutar, hem meter/wake-word hem
  transport ona bakar. O zaman track'leri **uygulama** durdurmak zorunda; yoksa macOS
  mikrofon göstergesi (turuncu nokta) sürekli yanar → gizlilik vaadini bozar.

**Öneri: A ile başla.** B ancak wake-word'ün web tarafında çalışmasına karar verilirse
(OQ-4) gerekli olur.

### Vite/bundling riski

`@openai/agents-realtime` `ws` paketine hard dependency'ye sahip.
`dist/shims/shims.mjs` (default condition) → `shims-node.mjs` → `export { WebSocket } from 'ws'`.
Browser condition ise `shims-browser.mjs` → `globalThis.WebSocket`.

Vite client build'i `browser` export condition'ını varsayılan olarak uygular, yani
sorun **çıkmamalı**; ama:

- `vite.config.ts`'te `resolve.conditions` override edilmemeli
- Bu build'de gerçekten `ws` bundle'a girmediği doğrulanmalı (bundle analiz / `grep`)
- `isBrowserEnvironment()` browser shim'de `true` döner — yukarıdaki `ek_` guard'ının
  devreye girmesi buna bağlı. Yanlış shim seçilirse guard sessizce kapanır. **Bu bir
  güvenlik regresyonu olur** → Phase 1'de bir test bunu assert etmeli.

---

## 5. Ephemeral client secret akışı

### Endpoint

`POST https://api.openai.com/v1/realtime/client_secrets`

Headers:

- `Authorization: Bearer $OPENAI_API_KEY` (**standart, kalıcı key — sadece Rust tarafında**)
- `Content-Type: application/json`
- `OpenAI-Safety-Identifier: <hashed-user-id>` (opsiyonel, önerilen; **sunucu tarafında** set edilir)

Request body:

```json
{
  "expires_after": { "anchor": "created_at", "seconds": 600 },
  "session": {
    "type": "realtime",
    "model": "gpt-realtime-2.1"
  }
}
```

- `expires_after.anchor`: şu an sadece `"created_at"`
- `expires_after.seconds`: **10–7200**, varsayılan **600** (10 dk)
- `session` içinde `instructions`, `audio`, `tools`, `tool_choice`, `output_modalities`,
  `max_output_tokens` (1–4096 veya `"inf"`), `truncation`, `tracing` da verilebilir

Response:

```json
{
  "value": "ek_...",
  "expires_at": 1690000000,
  "session": { ... }
}
```

`value` (`ek_` prefix'li) → `session.connect({ apiKey: value })`.

### Asuna'daki uygulama şekli

- **Rust `#[tauri::command]`**, örn. `mint_realtime_token() -> Result<EphemeralToken, Error>`.
  Dönen tip: `{ value: String, expires_at: i64, model: String }`.
- **`session` payload'ında minimum bilgi** olsun: `{ type: "realtime", model }`.
  Sebep: `instructions` ve `tools` zaten SDK tarafından data channel üzerinden
  `session.update` ile gönderiliyor; iki yerde tutmak drift üretir.
  **İstisna:** ileride hosted MCP tool'ları eklenirse credential'lar **mutlaka** bu
  server-side payload'a konur (docs bunu açıkça söylüyor).
- **TTL:** `seconds: 600` bırak (varsayılan). Token oturum başlatmak için geçerlilik
  süresidir; oturumun kendisi token expire olduktan sonra da devam eder.
- Token **her `connect()` öncesi taze** basılır. Cache'lenmez, log'lanmaz, IPC dışında
  hiçbir yere yazılmaz.
- `expires_at` renderer'da tutulur; yeniden bağlanma gerekirse önce yeni token istenir.
- `connect({ apiKey: () => invoke('mint_realtime_token') })` şeklinde **lazy fonksiyon**
  da verilebilir (`ApiKey = string | (() => string | Promise<string>)`). Yeniden
  bağlanmalarda daha temiz.

### Doğrulama komutu (API key gerektirir — bkz. Bölüm 8)

```bash
export OPENAI_API_KEY="sk-proj-..."
curl -sS -X POST https://api.openai.com/v1/realtime/client_secrets \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"session":{"type":"realtime","model":"gpt-realtime-2.1"}}' | jq .

# mini için:
curl -sS -X POST https://api.openai.com/v1/realtime/client_secrets \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"session":{"type":"realtime","model":"gpt-realtime-2.1-mini"}}' | jq .

# hesabın gördüğü model listesi:
curl -sS https://api.openai.com/v1/models \
  -H "Authorization: Bearer $OPENAI_API_KEY" | jq -r '.data[].id' | grep realtime
```

Başarı kriteri: HTTP 200 + `value` alanı `ek_` ile başlıyor.
Hata beklentileri: `401` (key geçersiz), `403`/`model_not_found` (modele erişim yok),
`429` (kota/billing).

---

## 6. Modeller, erişim ve fiyatlandırma

Erişim tarihi: **2026-08-24**. Kaynak: `developers.openai.com/api/docs/pricing`.
Para birimi **USD**, birim **1M token**.

| Model                               | Text in | Cached text in | Text out | **Audio in** | **Cached audio in** | **Audio out** |
| ----------------------------------- | ------- | -------------- | -------- | ------------ | ------------------- | ------------- |
| `gpt-realtime-2.1`                  | $4.00   | $0.40          | $24.00   | **$32.00**   | $0.40               | **$64.00**    |
| `gpt-realtime-2.1-mini`             | $0.60   | $0.06          | $2.40    | **$10.00**   | $0.30               | **$20.00**    |
| `gpt-realtime-mini` (eski snapshot) | $0.60   | $0.06          | $2.40    | $10.00       | $0.30               | $20.00        |

Image input: `2.1` $5.00 / cached $0.50; `2.1-mini` $0.80 / cached $0.08.

### Model metadata (`gpt-realtime-2.1`)

- Context window: 128.000 token
- Max output: 32.000 token
- Modaliteler: in = text/audio/image, out = text/audio
- **Sadece `v1/realtime` endpoint'i** — Chat Completions/Responses desteklemiyor
- Rate limit tier'a bağlı (Tier 1: 200 RPM / 40.000 TPM)
- **İlan edilmiş deprecation tarihi YOK**

### Deprecation takvimi (kaçınılacak modeller)

- `gpt-realtime`, `gpt-realtime-mini` → **shutdown 2027-01-20**
  (replacement: `gpt-realtime-2.1` / `gpt-realtime-2.1-mini`)
- `gpt-4o-realtime-preview` ailesi → shutdown 2026-05-07 (**geçmiş**)
- `gpt-realtime-2`, `gpt-realtime-1.5`, `gpt-realtime-2.1`, `gpt-realtime-2.1-mini`:
  deprecation ilanı yok

→ PROJECT.md'nin `gpt-realtime-2.1` / `-2.1-mini` seçimi **doğru ve güncel**.

### Kaba dakika maliyeti (TAHMİN — doğrulanmadı)

"1 dk giriş sesi ≈ 600 token, 1 dk üretilen konuşma ≈ 1200 token" dönüşümü
üçüncü parti kaynaklardan geliyor; **resmi OpenAI belgesinden doğrulanamadı**.
Bu varsayımla:

- `gpt-realtime-2.1`: ~~$0.019/dk giriş + ~$0.077/dk çıkış ≈ **~~$0.10/dk (~$5.8/saat)**
- `gpt-realtime-2.1-mini`: ~~$0.006/dk + ~$0.024/dk ≈ **~~$0.03/dk (~$1.8/saat)**

Buna `instructions` + context'in **her turda** text input olarak sayılması eklenir
(cached input indirimi devreye girer). Gerçek maliyet Phase 1'de
`session.usage` ile ölçülmeli, bu tahmine güvenilmemeli.

**Ürün sonucu:** `ASUNA_IDLE_TIMEOUT_SECONDS=45` ve idle'da session açmamak
en büyük maliyet kalemi. Geliştirme varsayılanı `-mini` doğru karar.

### Kullanım ölçümü

`session.usage` → `Usage` (`@openai/agents-core`):

```ts
class Usage {
  requests: number;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  inputTokensDetails: Array<Record<string, number>>; // audio_tokens, cached_tokens burada
  outputTokensDetails: Array<Record<string, number>>;
  requestUsageEntries: RequestUsage[] | undefined; // per-request, detaylı maliyet için
}
```

Audio/text ayrımı `inputTokensDetails` / `outputTokensDetails` içinde geliyor —
`sessions` tablosuna oturum kapanışında yazılacak alan bu.
**BELİRSİZ:** bu detail dict'lerinin tam anahtar isimleri (`audio_tokens`,
`cached_tokens` vs.) tip düzeyinde `Record<string, number>` — runtime'da
loglayıp şema Phase 1'de netleştirilmeli.

---

## 7. Interruption / barge-in

**Sonuç: SDK + sunucu yönetiyor, uygulama sadece UI tepkisi veriyor.**

- Varsayılan `turnDetection: { type: 'semantic_vad' }` — sunucu tarafı VAD açık.
- Kullanıcı konuşmaya başlarsa sunucu mevcut yanıtı keser.
- **WebRTC transport'ta ses buffer'ı SDK tarafından temizlenir** — uygulamanın
  playback durdurma sorumluluğu **yok**. (WebSocket'te var.)
- `audio_interrupted` event'i yayılır → Asuna bunu sadece state/UI için kullanır
  (`ASSISTANT_SPEAKING` → `USER_SPEAKING`).
- Manuel "sus" butonu: `session.interrupt()`.

Config alanları (`config.audio.input.turnDetection`), camelCase veya snake_case
kabul ediliyor:

```ts
{
  type: 'semantic_vad' | 'server_vad',
  eagerness?: 'auto' | 'low' | 'medium' | 'high',   // semantic_vad
  createResponse?: boolean,
  interruptResponse?: boolean,
  threshold?: number,           // server_vad
  prefixPaddingMs?: number,     // server_vad
  silenceDurationMs?: number,   // server_vad
  idleTimeoutMs?: number,       // server_vad
}
```

- `turnDetection: null` → tüm tur yönetimi uygulamaya geçer (Asuna için **istenmiyor**)
- `interruptResponse: false` + `createResponse: false` → VAD çalışır ama yanıtı
  uygulama tetikler (moderasyon senaryosu; Asuna'da gerekmiyor)

**Asuna önerisi (Phase 1 başlangıç):**

```ts
audio: { input: { turnDetection: { type: 'semantic_vad', eagerness: 'medium',
                                   createResponse: true, interruptResponse: true } } }
```

Türkçe konuşmada erken kesme sorunu çıkarsa `eagerness: 'low'` denenir.

---

## 8. Doğrulanamadı — API key gerekli

Aşağıdakiler **kod yazmadan / geçerli bir `OPENAI_API_KEY` olmadan** doğrulanamaz.
Phase 1'in ilk işi bunları kapatmak.

| #   | Doğrulanamayan                                                              | Nasıl doğrulanır                                                  |
| --- | --------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| V1  | `gpt-realtime-2.1`'in **bu hesapta** erişilebilir olması                    | `curl .../v1/models \| grep realtime` — listede var mı            |
| V2  | `gpt-realtime-2.1-mini`'nin bu hesapta erişilebilir olması                  | aynı                                                              |
| V3  | Client secret basmanın gerçekten çalışması + `ek_` formatı                  | Bölüm 5'teki `curl` — HTTP 200 + `.value`                         |
| V4  | `expires_at` ile `expires_after.seconds`'ın gerçekten örtüşmesi             | `curl` yanıtında `expires_at - now` hesapla                       |
| V5  | Hesabın rate limit tier'ı ve TPM tavanı                                     | platform.openai.com/settings/organization/limits                  |
| V6  | API billing'in aktif olduğu (ChatGPT aboneliği **yeterli değil**)           | platform.openai.com/settings/organization/billing                 |
| V7  | Tauri WKWebView'ında `window.RTCPeerConnection` ve `getUserMedia`           | **ASU-007 spike'ı** — bu task'ın kapsamı dışında                  |
| V8  | Vite build'inde `browser` shim'inin seçildiği (ek_ guard'ının aktif olduğu) | Build sonrası bundle'da `isBrowserEnvironment` dönüş değeri testi |
| V9  | `Usage.inputTokensDetails` anahtar isimleri                                 | Phase 1'de gerçek oturumda logla                                  |
| V10 | Dakika→token dönüşüm katsayıları (600/1200)                                 | Gerçek oturumda `session.usage` / süre ölçümü                     |
| V11 | Türkçe için en iyi transcription modeli/`languages` ayarı                   | Phase 1 manuel kalite testi                                       |

---

## 9. PROJECT.md Bölüm 24 pseudocode'una göre farklar

PROJECT.md'deki kavramsal kod:

```ts
const asuna = new RealtimeAgent({ name: "Asuna", instructions: ..., tools: [...] });
const session = new RealtimeSession(asuna, { model: config.realtimeModel });
await session.connect({ apiKey: ephemeralClientSecret });
```

**Bu büyük ölçüde doğru.** Farklar:

1. **`transport` açıkça verilmeli.** Verilmezse `hasWebRTCSupport()` ile otomatik
   seçilir; WKWebView'da `RTCPeerConnection` yoksa sessizce WebSocket'e düşer ve
   WebSocket'te ses pipeline'ı uygulamaya kalır → çalışmayan ses. Sessiz fallback
   yerine açık `transport: 'webrtc'`.
2. **`apiKey` `ek_` ile başlamak zorunda.** Kalıcı key ile `connect()` `UserError`
   fırlatır (browser ortamında). PROJECT.md'nin "ephemeral" vaadi SDK tarafından
   zorunlu kılınıyor.
3. **`voice` ve tur algılama `config` altında.** `ASUNA_REALTIME_VOICE` →
   `config.audio.output.voice` (tercih) veya `RealtimeAgent({ voice })`. Geçerli değerler:
   `alloy, ash, ballad, coral, echo, sage, shimmer, verse, marin, cedar`
   (docs "marin" veya "cedar" öneriyor). **Ses, oturum ses üretmeye başladıktan
   sonra değiştirilemez.**
4. `session.close()` **`void`**, `Promise` değil — `await` etme.
5. Realtime oturumu için **60 dakika üst sınırı** var (Realtime API kuralı).
   `ASUNA_MAX_SESSION_SECONDS` bunun altında olmalı.

### Asuna için hedef iskelet (Phase 1'e girdi)

```ts
import { RealtimeAgent, RealtimeSession, tool } from '@openai/agents-realtime';

const asuna = new RealtimeAgent({
  name: 'Asuna',
  instructions: buildAsunaInstructions(context),
  tools: asunaTools, // AsunaToolDefinition -> tool() adaptasyonu
});

const session = new RealtimeSession(asuna, {
  transport: 'webrtc',
  model: config.realtimeModel, // ASUNA_REALTIME_MODEL
  config: {
    outputModalities: ['audio'],
    audio: {
      input: {
        turnDetection: {
          type: 'semantic_vad',
          eagerness: 'medium',
          createResponse: true,
          interruptResponse: true,
        },
        transcription: config.transcriptStorage
          ? { model: 'gpt-4o-mini-transcribe', language: 'tr' }
          : null,
        noiseReduction: { type: 'near_field' },
      },
      output: { voice: config.realtimeVoice }, // ASUNA_REALTIME_VOICE
    },
  },
  historyStoreAudio: false, // varsayılan; ses RAM'de tutulmasın
});

await session.connect({
  apiKey: () => invoke<string>('mint_realtime_token'), // Rust'tan taze ek_ token
});
```

Tool tanımı (`tool()`, `@openai/agents-realtime`'dan re-export):

```ts
const getCurrentProject = tool({
  name: 'get_current_project',
  description: 'Kullanicinin uzerinde calistigi aktif projeyi dondurur.',
  parameters: z.object({}), // Zod 4
  needsApproval: false, // risk 0 = onaysiz
  async execute(_args, runContext) {
    return await invoke('get_current_project'); // gercek is Rust tarafinda
  },
});
```

Kritik: **function tool'lar `RealtimeSession`'ın çalıştığı yerde çalışır** — yani
renderer'da. Docs birebir: _"if you are running your session in the browser, the tool
executes in the browser. If you need to perform sensitive actions, call your backend
from inside the tool."_ Asuna'nın tool'ları bu yüzden **ince backchannel** olmalı:
gerçek dosya/git/DB işi `#[tauri::command]` üzerinden Rust'ta. Bu, PROJECT.md 19'un
güvenlik modeliyle tam örtüşüyor.

Risk 2/3 tool'lar için `needsApproval: true` →
`tool_approval_requested` → `AWAITING_APPROVAL` → `session.approve(item)` /
`session.reject(item, { message })`. `{ alwaysApprove: true }` oturum boyunca
yapışkan onay verir (Asuna'da **kullanılmamalı** — her destructive işlem tekrar sorulmalı).

---

## 10. Kaynaklar

| Kaynak                                                  | URL                                                                                                       | Erişim     |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | ---------- |
| npm `@openai/agents-realtime` metadata                  | `npm view @openai/agents-realtime --json`                                                                 | 2026-08-24 |
| Paketin kendi `.d.ts` / `.mjs` dosyaları (ground truth) | `@openai/agents-realtime@0.17.0` tarball                                                                  | 2026-08-24 |
| Realtime Agents Quickstart                              | https://openai.github.io/openai-agents-js/guides/voice-agents/quickstart/                                 | 2026-08-24 |
| Building Realtime Agents                                | https://openai.github.io/openai-agents-js/guides/voice-agents/build/                                      | 2026-08-24 |
| Realtime Transport Layer                                | https://openai.github.io/openai-agents-js/guides/voice-agents/transport/                                  | 2026-08-24 |
| Docs kaynak `.mdx` + örnekler                           | https://github.com/openai/openai-agents-js/tree/main/docs & /examples/docs/voice-agents                   | 2026-08-24 |
| Releases / breaking changes                             | https://github.com/openai/openai-agents-js/releases                                                       | 2026-08-24 |
| Create client secret (API ref)                          | https://developers.openai.com/api/reference/resources/realtime/subresources/client_secrets/methods/create | 2026-08-24 |
| Realtime WebRTC guide                                   | https://developers.openai.com/api/docs/guides/realtime-webrtc/                                            | 2026-08-24 |
| Realtime conversations guide (voices, 60 dk)            | https://developers.openai.com/api/docs/guides/realtime-conversations/                                     | 2026-08-24 |
| Pricing                                                 | https://developers.openai.com/api/docs/pricing                                                            | 2026-08-24 |
| Model: gpt-realtime-2.1                                 | https://developers.openai.com/api/docs/models/gpt-realtime-2.1                                            | 2026-08-24 |
| Deprecations                                            | https://developers.openai.com/api/docs/deprecations                                                       | 2026-08-24 |

**Not:** `platform.openai.com/docs/*` WebFetch'e 403 dönüyor; içerik
`developers.openai.com` üzerinden doğrulandı.

---

## 11. WKWebView doğrulaması (ASU-007)

> Kaynak: ASU-007 spike'ı, 2026-08-24. Yöntem: SDK **olmadan**, ham tarayıcı
> API'leriyle; sonuçlar Rust `#[tauri::command]` ile diske yazıldı. Ortam:
> macOS (Darwin 25.5), Apple Silicon, Tauri 2.11.5 / wry 0.55.1, WKWebView
> (AppleWebKit 605.1.15). 3 dev koşusu + 3 paketlenmiş `.app` koşusu +
> native Safari kontrol deneyi.

### 11.1 Sonuç

**OQ-5 kapandı: WebRTC transport Tauri WKWebView'ında çalışıyor.** Fallback
(WebSocket transport / Rust audio pipeline / ayrı process) gerekmiyor.
`transport: 'webrtc'` açıkça verilmeli — ama `hasWebRTCSupport()` guard'ı da
zaten `true` dönüyor, yani sessiz WebSocket düşüşü riski yok.

### 11.2 API yüzeyi (izin gerektirmez)

| Kontrol | dev (`http://localhost:1420`) | bundle (`tauri://localhost`) |
| --- | --- | --- |
| `typeof window.RTCPeerConnection` | `function` | `function` |
| `hasWebRTCSupport()` (SDK guard) | `true` | `true` |
| `typeof navigator.mediaDevices.getUserMedia` | `function` | `function` |
| `window.isSecureContext` | `true` | **`true`** |
| `typeof crypto.subtle` | `object` | `object` |
| `RTCRtpSender.getCapabilities('audio')` | opus, red, G722, PCMU, PCMA, CN, telephone-event | aynı |

**`tauri://localhost` bir secure context.** Custom protocol'ün
`getUserMedia`/WebCrypto'yu düşürmesi riski gerçekleşmedi — `useHttpsScheme`
gibi bir ayara ihtiyaç yok.

### 11.3 SDP / ICE

Mikrofon izni **olmadan** üretilen offer: `m=audio` ✓, `opus/48000` ✓,
`a=fingerprint:` (DTLS) ✓, `m=application` (data channel `oai-events`) ✓,
ICE gathering `complete`, host adayları toplanıyor. STUN verildiğinde
**srflx adayı alınıyor** → webview dışarıya UDP gönderip yanıt alabiliyor.
Bu, SDK'nın ihtiyaç duyduğu ağ kabiliyetinin ta kendisi.

### 11.4 Mikrofon

`getUserMedia({ audio: true })` → gerçek cihaz track'i:
`label: "MacBook Pro Microphone"`, `readyState: live`, `sampleRate: 48000`,
`echoCancellation: true`, `volume: 1`.

- **Tek TCC promptu.** Paketlenmiş `.app` ilk kez istediğinde macOS dialogu
  çıkıyor (gUM promise'i askıda kalıyor). Onaydan sonra grant **kalıcı**:
  uygulamanın temiz yeniden açılışında gUM **65 ms**'de, dialogsuz dönüyor.
- **`track.stop()` sonrası** track `ended` → macOS turuncu mikrofon göstergesi
  sönüyor. `IDLE`'a dönüşte mikrofonu bırakmak uygulamanın sorumluluğu
  (Bölüm 4 "mediaStream sahipliği" ile birebir örtüşür).
- **Gotcha:** `navigator.permissions.query({name:'microphone'})` sayfa
  yüklenirken TCC izni verilmiş olsa bile **`prompt`** döner; ancak o sayfada
  başarılı bir gUM'dan sonra `granted` olur. **İzin durumu için bunu kaynak
  gerçek olarak kullanma** — gerçek sinyal gUM'ın çözülme süresidir.
- **wry not:** `webView:requestMediaCapturePermissionForOrigin:...` delegate'i
  **koşulsuz `WKPermissionDecision::Grant`** dönüyor. Yani webview seviyesinde
  bir izin kapısı **yok**; tek kapı macOS TCC. Güvenlik sonucu: webview'da
  yüklenen her origin mikrofona erişebilir → CSP ve navigasyon kısıtları ekstra
  önem kazanıyor (PROJECT.md Bölüm 19).

### 11.5 Uzak ses çıkışı

SDK'nın kod yolu (`<audio autoplay>` + `srcObject = remoteStream`) çalışıyor:
`play()` promise **resolve oluyor**, `paused: false`, `readyState: 4`,
`currentTime` ilerliyor. **Autoplay engeli yok** — wry
`mediaTypesRequiringUserActionForPlayback = None` set ediyor (wry varsayılanı
`autoplay: true`, Tauri bunu değiştirmiyor). `AudioContext` `running`, 48 kHz.
`options.audioElement` vermeye gerek yok.

### 11.6 CSP — Phase 1 için zorunlu değişiklik (UYGULANDI)

SDK, SDP offer'ını `POST https://api.openai.com/v1/realtime/calls` ile
gönderiyor (Bölüm 4). Paketlenmiş uygulamada varsayılan CSP bunu **engelliyordu**:

```
securitypolicyviolation: connect-src <- https://api.openai.com/v1/realtime/calls
fetch → TypeError: Load failed
```

**Bu hata `pnpm tauri dev`'de GÖRÜNMEZ** — dev'de sayfayı Vite servis eder,
Tauri'nin CSP header'ı uygulanmaz. Yani ses dev'de çalışıp `tauri build`
sonrası sessizce ölür. Düzeltme `tauri.conf.json`'a uygulandı:

```json
"connect-src": "'self' ipc: http://ipc.localhost https://api.openai.com"
```

Doğrulandı (spike): düzeltmeden sonra paketlenmiş build'de istek ağa çıkıyor
(`401`, auth yok — beklenen), sıfır CSP ihlali.
`media-src 'self' blob: mediastream:` zaten yeterli. WebRTC'nin UDP/ICE trafiği
`connect-src`'a tabi **değil** (CSP'li build'de de srflx adayı alındı) —
kesilen yalnızca SDP HTTP POST'u.
**WebSocket transport'a düşülürse** ayrıca `wss://api.openai.com` eklenmeli.

### 11.7 Doğrulanamayan: gerçek RTP medya akışı

İki lokal peer arasındaki loopback bu makinede `checking → failed`'de kaldı
(`packetsSent: 0`). Katman izolasyonu:

1. Trickle ICE adayları iki yöne forward edildi ve `addIceCandidate` yalnızca
   remote description set edildikten sonra çağrıldı → `addIceCandidate`
   hatası **sıfır**. Race elendi, sonuç değişmedi.
2. Ortamda **Cloudflare WARP açık**. WebKit yalnızca WARP tünel adresini
   (`172.16.0.2`) topluyor, `en0`'ı hiç toplamıyor; izin öncesi adaylar mDNS
   maskeli (`<uuid>.local`) ve çözülemiyor.
3. **Kontrol deneyi:** Aynı kod **native Safari 26.5.2**'de (Tauri yok),
   STUN'suz ve STUN'lu, **tıpatıp aynı şekilde** başarısız.

→ Lokal P2P hairpin bu ağda tarayıcı bağımsız çalışmıyor; **WKWebView kusuru
değil**. Asuna'nın senaryosu P2P değil (client → OpenAI public sunucusu) ve o
yolun ön koşulu olan srflx/STUN erişimi webview'da **çalışıyor**.
Gerçek RTP akışı Phase 1'de ilk canlı oturumda `session.usage` ve
`transport_event` ile doğrulanacak (V7 → kısmen kapandı, tam kapanış Phase 1).

### 11.8 Kalan manuel doğrulamalar

| # | Doğrulanacak | Nasıl | Ne zaman |
|---|---|---|---|
| M1 | Gerçek RTP medya akışı (paket + duyulabilir ses) | İlk canlı OpenAI oturumu; `session.usage`, `transport_event`, `getStats().inbound-rtp.totalAudioEnergy > 0` | Phase 1 ilk task |
| M2 | Entitlement'ın fiilen uygulanması | Developer ID ile imzala → `codesign -d --entitlements -` çıktısında `audio-input` görünmeli | İmzalama/dağıtım task'ı |
| M3 | Hardened runtime altında mikrofon | M2 ile birlikte — imzasız `.app`'te hardened runtime fiilen zorlanmıyor | İmzalama/dağıtım task'ı |
| M4 | Dev binary'de her rebuild'de TCC promptu tekrar çıkıyor mu | Dev ergonomisi gözlemi | Phase 1 sırasında |
| M5 | TCC devir teslimi: Rust `cpal` (wake word) → renderer `getUserMedia` tek izin mi | ASU-008b kapsamı — bu spike'ta paketlenmiş app **tek** mikrofon promptu gösterdi, iyi işaret | ASU-008b |

### 11.9 Info.plist kuralı (pazarlıksız)

`NSMicrophoneUsageDescription` **tam olarak `src-tauri/Info.plist`** dosyasında
durmalı: dev'de `tauri-codegen` yalnızca bu yolu okuyup dev binary'ye gömer
(`bundle.macOS.infoPlist` ile verilen özel yolu dev'de OKUMAZ — oraya konursa
`tauri dev`'de mikrofon TCC ihlaliyle patlar); build'de bundler aynı dosyayı
üretilen Info.plist ile merge eder.

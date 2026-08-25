/**
 * Tool registry + calistirma sarmalayicisi (ASU-047, PROJECT.md Bolum 17).
 *
 * Iki is yapar:
 *
 * 1. [`ToolRegistry`] — modele hangi yeteneklerin acildiginin **tek** kaydi.
 *    Kayit aninda sozlesme zorlanir: gecersiz bir tanim modele hic acilmaz
 *    (`conventions.md` — "Tool Tanimi").
 * 2. [`executeTool`] — bir tool'u calistirmanin **tek** mesru yolu. Sema
 *    dogrulamasi, onay kapisi (ASU-048), timeout, yapisal sonuc ve audit
 *    kaydi (ASU-050) burada; tool'lar bunlari kendi iclerinde tekrarlamak
 *    zorunda kalmasin diye.
 *
 * # Neden calistirma tool'un kendisinde degil
 *
 * "Her tool kendi semasini dogrular, kendi timeout'unu kurar" demek, guvenlik
 * kurallarinin N kez kopyalanmasi ve ilk unutuldugunda sessizce delinmesi
 * demekti. Tek kapi olunca onay karari ve audit yazimi da tek yere baglanir:
 * `executeTool`'dan gecmeyen bir tool cagrisi ne onaydan gecer ne deftere
 * yazilir — ve bu yuzden gecmemesi mumkun degil.
 *
 * # SDK'dan bagimsiz
 *
 * Bu modul `@openai/agents-realtime` bilmez. Realtime oturumu bir tuketicidir,
 * sahip degil: `realtime-service.ts` registry'nin listesini SDK tool'una cevirir
 * ve `execute` govdesinde yine buradaki [`executeTool`]'u cagirir.
 */

import {
  APPROVAL_TIMEOUT_MS,
  approvalStateFor,
  resolveApproval,
  type ApprovalOutcome,
} from './approval-policy';
import type { AsunaToolDefinition, ToolContext, ToolResult } from './types';
import type { ToolApprovalMode } from '../config/frontend-config';
import type { ToolApprovalState, ToolAuditInput, ToolRiskLevel } from '../../shared/tool-event';

/**
 * `snake_case`, fiil_nesne (`conventions.md`). Model tool adini duyar gibi
 * okur; `getCurrentProject2` gibi bir ad hem protokolde hem prompt'ta gurultu.
 */
const TOOL_NAME_PATTERN = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$/u;

/** Model icin anlamli bir kapsam aciklamasinin alt siniri. */
const MIN_DESCRIPTION_CHARS = 20;

/**
 * Tool timeout tavani.
 *
 * Sesli bir oturumda iki dakika sessizlik zaten kabul edilemez; bundan uzun bir
 * ise ihtiyac duyan tool, "baslattim" deyip sonucu ayri bir kanaldan bildirmeli
 * (PROJECT.md Bolum 30 — bekletme degil, durust rapor).
 */
export const MAX_TOOL_TIMEOUT_MS = 120_000;

/**
 * `ToolResult.errorKind` degerleri.
 *
 * Serbest metin degil sabit: audit (`tool_events`) ve UI bu degerlere gore
 * gruplar; "timeout" ile "gecersiz arguman" ayni kefeye konmamali.
 */
export const TOOL_ERROR_KINDS = {
  /** Sema reddetti — tool **calistirilmadi**. */
  invalidArguments: 'invalid_arguments',
  /** [`AsunaToolDefinition.timeoutMs`] doldu; is bitmis olabilir de olmayabilir de. */
  timeout: 'timeout',
  /** Cagiran vazgecti (oturum kapandi, kullanici kesti). */
  aborted: 'aborted',
  /**
   * Onay alinamadi — tool **calistirilmadi** (ASU-048).
   *
   * Reddedilen, zaman asimina ugrayan ve hic sorulamayan onaylarin hepsi bu
   * tek `errorKind` altinda: modelin acisindan uc durum da ayni sey demek
   * ("yapmadim"). Ayrimi tasiyan yer audit defteri: `approval_state`
   * `denied` / `timeout` / `not_requested` olarak ayri ayri yazilir.
   */
  denied: 'denied',
  /** Tool implementasyonu hata firlatti (kendi `ok: false`'unu uretemedi). */
  failed: 'tool_failed',
} as const;

export type ToolRegistryErrorCode =
  | 'duplicate_tool'
  | 'invalid_name'
  | 'invalid_description'
  | 'invalid_timeout'
  | 'approval_required';

/**
 * Registry sozlesmesi ihlali. Bu bir **programci hatasidir**, calisma zamani
 * belirsizligi degil: yakalanip "yok sayilmaz", acilista patlar.
 */
export class ToolRegistryError extends Error {
  public override readonly name = 'ToolRegistryError';

  public constructor(
    public readonly code: ToolRegistryErrorCode,
    message: string,
  ) {
    super(message);
  }
}

/**
 * Modele acilan tool'larin kaydi.
 *
 * Global bir singleton **degil**: varsayilan ornek `index.ts`'te kurulur, ama
 * testler ve ileride farkli oturum profilleri kendi ornegini kurabilsin diye
 * sinif olarak birakildi.
 */
export class ToolRegistry {
  /** Ekleme sirasi korunur — modele giden liste deterministik olsun. */
  private readonly tools = new Map<string, AsunaToolDefinition>();

  /**
   * Tool'u kaydeder.
   *
   * @throws {ToolRegistryError} sozlesme ihlalinde. Ozellikle:
   * - ayni isim ikinci kez (sessizce ustune yazmak, hangi implementasyonun
   *   calistigini belirsiz birakirdi);
   * - risk 2/3 olup onay istemeyen tanim (`conventions.md` pazarliksiz kurali).
   */
  public register(definition: AsunaToolDefinition): void {
    this.assertValid(definition);
    this.tools.set(definition.name, definition);
  }

  /** Kayit sirasinda, modele verilebilir liste. */
  public list(): readonly AsunaToolDefinition[] {
    return [...this.tools.values()];
  }

  /** Isimle cozer. Bilinmeyen ad `null` doner — uydurma bir tool calistirilmaz. */
  public resolve(name: string): AsunaToolDefinition | null {
    return this.tools.get(name) ?? null;
  }

  public has(name: string): boolean {
    return this.tools.has(name);
  }

  public get size(): number {
    return this.tools.size;
  }

  private assertValid(definition: AsunaToolDefinition): void {
    const { name, description, risk, requiresApproval, timeoutMs } = definition;

    if (!TOOL_NAME_PATTERN.test(name)) {
      throw new ToolRegistryError(
        'invalid_name',
        `Tool adi snake_case olmali (fiil_nesne): \`${name}\` kabul edilmedi.`,
      );
    }

    if (this.tools.has(name)) {
      throw new ToolRegistryError(
        'duplicate_tool',
        `\`${name}\` zaten kayitli. Ayni isimle ikinci bir tool kaydedilemez.`,
      );
    }

    if (description.trim().length < MIN_DESCRIPTION_CHARS) {
      throw new ToolRegistryError(
        'invalid_description',
        `\`${name}\` aciklamasi cok kisa; model tool'un ne yaptigini ve ne yapmadigini ` +
          'aciklamadan dogru secim yapamaz.',
      );
    }

    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || timeoutMs > MAX_TOOL_TIMEOUT_MS) {
      throw new ToolRegistryError(
        'invalid_timeout',
        `\`${name}\` timeout'u 1..${MAX_TOOL_TIMEOUT_MS.toString()} ms araliginda olmali; ` +
          `\`${String(timeoutMs)}\` kabul edilmedi.`,
      );
    }

    if (risk >= 2 && !requiresApproval) {
      throw new ToolRegistryError(
        'approval_required',
        `\`${name}\` risk ${risk.toString()} ama onay istemiyor. Risk 2/3 icin onay ` +
          'zorunludur ve konfigurasyonla gevsetilemez.',
      );
    }
  }
}

/**
 * Onay kapisi (ASU-048).
 *
 * Politika "onay lazim" dediginde [`executeTool`] bunu cagirir ve cevabi
 * bekler. Cagri **kanit** ister, izin degil: onay akisini kim yurutuyorsa
 * (Realtime oturumunda SDK'nin `tool_approval_requested` -> `approve/reject`
 * dongusu) sonucu buraya bildirir.
 *
 * Sozlesme: firlatirsa **reddedilmis** sayilir, cozulmezse
 * [`APPROVAL_TIMEOUT_MS`] sonunda `timeout` (yine reddetme) kabul edilir.
 * "Onay kapisi bozuldu" durumu hicbir kosulda "calistir" anlamina gelmez.
 */
export type ToolApprovalGate = (
  definition: AsunaToolDefinition,
  args: unknown,
) => Promise<ApprovalOutcome>;

/** [`executeTool`] ek ayarlari. */
export interface ToolExecutionOptions {
  /**
   * Cagiranin iptal sinyali (oturum kapanisi, SDK'nin kendi iptali). Timeout
   * ile birlesir: hangisi once gelirse tool'a giden `context.signal` abort olur.
   */
  readonly signal?: AbortSignal;
  /**
   * `ASUNA_TOOL_APPROVAL_MODE`. Verilmezse **en siki** mod varsayilir
   * (`always`): modu okuyamayan bir cagiran, en gevsek davranisi miras
   * almamali (phase-5.md ASU-048 — "belirsizlik onay lehine").
   */
  readonly approvalMode?: ToolApprovalMode;
  /**
   * Onay kanali. **Verilmezse onay gerektiren tool calismaz**: gate'i unutmak
   * sessizce "onaysiz calistir"a donusmez, `not_requested` ile reddedilir.
   */
  readonly approvalGate?: ToolApprovalGate;
  /**
   * Audit defteri kancasi (ASU-050). Cagri **hangi yoldan biterse bitsin**
   * tam bir kez cagrilir: sema reddi, onay reddi, timeout, hata, basari.
   *
   * Duz bir callback (Promise degil): audit yazimi tool sonucunu bekletmemeli
   * ve bir audit arizasi tool cagrisini dusurmemeli. Yazma hatalarinin gorunur
   * kalmasi `audit.ts`'in isi (`recordToolEvent` asla firlatmaz, `failed`
   * doner ve log'lar).
   */
  readonly onAudit?: (input: ToolAuditInput) => void;
}

/** Sema hatalarindan modele giden ozetin karakter tavani. */
const MAX_ISSUE_SUMMARY_CHARS = 240;

/** Ozete girecek en fazla sema hatasi — model icin ilk birkac tanesi yeterli. */
const MAX_REPORTED_ISSUES = 3;

/**
 * Sema hatalarini modele okunur tek satira cevirir.
 *
 * **Deger yazilmaz**, yalnizca alan yolu ve hatanin turu: reddedilen argumanlar
 * (dosya yolu, sorgu metni) audit'e ve modele geri dokulmemeli.
 */
function describeIssues(
  issues: readonly { readonly path: readonly PropertyKey[]; readonly message: string }[],
): string {
  const described = issues.slice(0, MAX_REPORTED_ISSUES).map((issue) => {
    const field = issue.path.map((part) => String(part)).join('.');
    return field.length === 0 ? issue.message : `${field}: ${issue.message}`;
  });

  const rest = issues.length - described.length;
  const suffix = rest > 0 ? ` (+${rest.toString()} hata daha)` : '';
  const text = `${described.join('; ')}${suffix}`;

  return text.length <= MAX_ISSUE_SUMMARY_CHARS
    ? text
    : `${text.slice(0, MAX_ISSUE_SUMMARY_CHARS - 1)}…`;
}

function failure(summary: string, errorKind: string): ToolResult {
  return { ok: false, summary, errorKind };
}

/** Sinyalin **o andaki** durumu. Bkz. `runTool` — deger zamanla degisir. */
function isAborted(signal: AbortSignal | undefined): boolean {
  return signal?.aborted === true;
}

function messageOf(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return typeof error === 'string' ? error : 'bilinmeyen hata';
}

/**
 * Onay cevabini bekler ve **her** belirsizligi reddetmeye cevirir (ASU-048).
 *
 * Uc kacis yolu da kapali:
 * - kapi firlatirsa `denied`;
 * - kapi hic cozulmezse [`APPROVAL_TIMEOUT_MS`] sonunda `timeout`;
 * - kapi gecersiz bir deger dondurse bile tip sistemi disinda kalir, `approved`
 *   olmayan her sey calistirmamaya gider.
 */
async function awaitApproval(
  gate: ToolApprovalGate,
  definition: AsunaToolDefinition,
  args: unknown,
): Promise<ApprovalOutcome> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const expiry = new Promise<ApprovalOutcome>((resolve) => {
    timer = setTimeout(() => {
      resolve('timeout');
    }, APPROVAL_TIMEOUT_MS);
  });

  try {
    return await Promise.race([
      gate(definition, args).catch((): ApprovalOutcome => 'denied'),
      expiry,
    ]);
  } finally {
    clearTimeout(timer);
  }
}

/** [`executeTool`]'un ic sonucu: modele giden cevap + audit'e giden etiket. */
interface ToolRun {
  readonly result: ToolResult;
  readonly approvalState: ToolApprovalState;
}

/**
 * Bir tool'u calistirmanin tek mesru yolu.
 *
 * Sirasiyla:
 * 1. **Sema dogrulamasi.** Gecersiz argumanda `execute` **hic cagrilmaz** ve
 *    yapisal bir hata doner — "once dogrula, sonra calistir" (PROJECT.md
 *    Bolum 17/18).
 * 2. **Onay kapisi (ASU-048).** [`resolveApproval`] "onay lazim" derse
 *    `options.approvalGate` sorulur. Onaylanmayan cagrida `execute` **hic
 *    cagrilmaz**; sonuc `denied` errorKind'i ile doner ve audit'e gercek
 *    onay durumu (`denied` / `timeout` / `not_requested`) yazilir.
 * 3. **Timeout.** [`AsunaToolDefinition.timeoutMs`] dolunca `context.signal`
 *    abort edilir ve cagri yapisal bir `timeout` sonucuyla **doner**. Asili
 *    kalan bir tool oturumu kilitlemez; arkadaki is kendiliginden durmaz, bu
 *    yuzden sonuc "bitmedi" der, "basarisiz oldu" demez. Sayac **onaydan
 *    sonra** baslar: kullanicinin dusunme suresi tool'un calisma butcesini
 *    yemez.
 * 4. **Yapisal sonuc.** `execute` patlarsa bile serbest metin degil,
 *    [`ToolResult`] doner. Hata yutulmaz: `ok: false` + `errorKind`.
 * 5. **Audit (ASU-050).** Hangi yoldan cikilirsa cikilsin `options.onAudit`
 *    tam bir kez cagrilir — calisan da, reddedilen de deftere yazilir.
 *
 * Hicbir kosulda `throw` etmez — cagiran (SDK adaptoru) her zaman modele
 * cevrilebilir bir sonuc alir.
 */
export async function executeTool(
  definition: AsunaToolDefinition,
  args: unknown,
  context: ToolContext,
  options: ToolExecutionOptions = {},
): Promise<ToolResult> {
  const run = await runTool(definition, args, context, options);
  reportAudit(definition, args, context, options, run);
  return run.result;
}

/** Audit satirini uretir. Cagrilmasi zorunlu; kanca yoksa sessizce gecilir. */
function reportAudit(
  definition: AsunaToolDefinition,
  args: unknown,
  context: ToolContext,
  options: ToolExecutionOptions,
  run: ToolRun,
): void {
  const onAudit = options.onAudit;
  if (onAudit === undefined) {
    return;
  }

  // `arguments` **ham** gonderilir: ozetleme ve redaksiyon host tarafinda
  // yapilir (`shared/tool-event.ts` — renderer hazir ozet gonderemez).
  onAudit({
    toolName: definition.name,
    riskLevel: definition.risk satisfies ToolRiskLevel,
    ...(context.sessionId === null ? {} : { sessionId: context.sessionId }),
    ...(args === undefined ? {} : { arguments: args }),
    approvalState: run.approvalState,
    resultSummary: run.result.summary,
  });
}

async function runTool(
  definition: AsunaToolDefinition,
  args: unknown,
  context: ToolContext,
  options: ToolExecutionOptions,
): Promise<ToolRun> {
  const parsed = definition.parameters.safeParse(args);
  if (!parsed.success) {
    return {
      // Onay asamasina hic gelinmedi: cagri daha once dustu.
      approvalState: 'not_requested',
      result: failure(
        `\`${definition.name}\` cagrisi gecersiz argumanlar yuzunden calistirilmadi — ` +
          `${describeIssues(parsed.error.issues)}. Dogru parametrelerle tekrar dene; ` +
          'sonucu varmis gibi konusma.',
        TOOL_ERROR_KINDS.invalidArguments,
      ),
    };
  }

  const external = options.signal;
  if (external?.aborted === true) {
    return {
      approvalState: 'not_requested',
      result: failure(
        `\`${definition.name}\` calistirilmadi: cagri baslamadan iptal edildi.`,
        TOOL_ERROR_KINDS.aborted,
      ),
    };
  }

  // Mod okunamadiginda **en siki** varsayilan: gevsek olan miras alinmaz.
  const mode = options.approvalMode ?? 'always';
  const decision = resolveApproval(definition.risk, definition.requiresApproval, mode);

  let approvalState = approvalStateFor(definition.risk, decision, null);
  if (decision === 'needs_approval') {
    // Yalnizca onay gerektiginde `await` var: onaysiz calisan bir tool'un
    // baslangici microtask'lara yayilmaz (iptal sinyali de gecikmez).
    const approval = await gateApproval(definition, parsed.data, options, decision);
    if (approval.result !== null) {
      return { approvalState: approval.approvalState, result: approval.result };
    }
    approvalState = approval.approvalState;

    // `isAborted` fonksiyon: sinyalin durumu onay beklenirken **degisebilir**,
    // yukaridaki kontrolun daralttigi tip burada gecerli degil.
    if (isAborted(external)) {
      // Onay beklenirken oturum kapandi: onay alinmis olsa bile calistirmiyoruz.
      return {
        approvalState,
        result: failure(
          `\`${definition.name}\` onaylandi ama cagri bu arada iptal edildi; calistirmadim.`,
          TOOL_ERROR_KINDS.aborted,
        ),
      };
    }
  }

  const controller = new AbortController();

  // Timeout ve iptal, `race`'in **cozulen** tarafi: reject edilirse cagiranin
  // her cagriyi try/catch'e sarmasi gerekirdi; sozlesme "her zaman ToolResult".
  // Executor senkron kosar, `settle` `Promise.race`'ten once atanmis olur;
  // yer tutucu yalnizca "atanmadan kullanildi" analizini susturmak icin.
  let settle: (result: ToolResult) => void = () => undefined;
  const interrupted = new Promise<ToolResult>((resolve) => {
    settle = resolve;
  });

  const timer = setTimeout(() => {
    controller.abort();
    settle(
      failure(
        `\`${definition.name}\` ${definition.timeoutMs.toString()} ms icinde bitmedi ve ` +
          'kesildi. Islemin tamamlanip tamamlanmadigi bilinmiyor — kullaniciya ' +
          'yapildi deme.',
        TOOL_ERROR_KINDS.timeout,
      ),
    );
  }, definition.timeoutMs);

  const onExternalAbort = (): void => {
    controller.abort();
    settle(
      failure(
        `\`${definition.name}\` cagrisi iptal edildi; sonuc alinmadi.`,
        TOOL_ERROR_KINDS.aborted,
      ),
    );
  };
  external?.addEventListener('abort', onExternalAbort, { once: true });

  const run = async (): Promise<ToolResult> => {
    try {
      return await definition.execute(parsed.data, { ...context, signal: controller.signal });
    } catch (error) {
      // Tool kendi hatasini ele almadi. Yutulmuyor: model reddi oldugu gibi gorur.
      return failure(
        `\`${definition.name}\` calistirilamadi: ${messageOf(error)}`,
        TOOL_ERROR_KINDS.failed,
      );
    }
  };

  try {
    return { approvalState, result: await Promise.race([run(), interrupted]) };
  } finally {
    clearTimeout(timer);
    external?.removeEventListener('abort', onExternalAbort);
  }
}

/**
 * Onay kapisi (ASU-048) — `execute` cagrisindan **once** ki tek karar noktasi.
 *
 * `result: null` = yol acik, tool calistirilabilir. Aksi halde donen sonuc
 * dogrudan cagirana gider ve `execute` hic cagrilmaz.
 */
async function gateApproval(
  definition: AsunaToolDefinition,
  args: unknown,
  options: ToolExecutionOptions,
  decision: 'needs_approval',
): Promise<{ readonly approvalState: ToolApprovalState; readonly result: ToolResult | null }> {
  const gate = options.approvalGate;
  if (gate === undefined) {
    // Onay gerekiyor ama soracak kimse yok. Varsayilan calistirmamak: kapiyi
    // baglamayi unutmak, sessizce "onaysiz calistir"a donusmemeli.
    return {
      approvalState: approvalStateFor(definition.risk, decision, null),
      result: failure(
        `\`${definition.name}\` onay gerektiriyor ama onay istegi iletilemedi; ` +
          'calistirmadim. Kullaniciya yapilmis gibi anlatma.',
        TOOL_ERROR_KINDS.denied,
      ),
    };
  }

  const outcome = await awaitApproval(gate, definition, args);
  const approvalState = approvalStateFor(definition.risk, decision, outcome);

  if (outcome === 'approved') {
    return { approvalState, result: null };
  }

  return {
    approvalState,
    result: failure(
      outcome === 'timeout'
        ? `\`${definition.name}\` icin onay beklenirken sure doldu; calistirmadim. ` +
            'Kullanici hala isterse tekrar sorabilirsin.'
        : `\`${definition.name}\` kullanici tarafindan onaylanmadi; calistirmadim. ` +
            'Bunu durustce soyle, yapilmis gibi anlatma.',
      TOOL_ERROR_KINDS.denied,
    ),
  };
}

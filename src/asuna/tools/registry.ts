/**
 * Tool registry + calistirma sarmalayicisi (ASU-047, PROJECT.md Bolum 17).
 *
 * Iki is yapar:
 *
 * 1. [`ToolRegistry`] — modele hangi yeteneklerin acildiginin **tek** kaydi.
 *    Kayit aninda sozlesme zorlanir: gecersiz bir tanim modele hic acilmaz
 *    (`conventions.md` — "Tool Tanimi").
 * 2. [`executeTool`] — bir tool'u calistirmanin **tek** mesru yolu. Sema
 *    dogrulamasi, timeout ve yapisal sonuc uretimi burada; tool'lar bunlari
 *    kendi iclerinde tekrarlamak zorunda kalmasin diye.
 *
 * # Neden calistirma tool'un kendisinde degil
 *
 * "Her tool kendi semasini dogrular, kendi timeout'unu kurar" demek, guvenlik
 * kurallarinin N kez kopyalanmasi ve ilk unutuldugunda sessizce delinmesi
 * demekti. Tek kapi olunca ASU-048 (onay) ve ASU-050 (audit) da tek yere
 * baglanir.
 *
 * # SDK'dan bagimsiz
 *
 * Bu modul `@openai/agents-realtime` bilmez. Realtime oturumu bir tuketicidir,
 * sahip degil: `realtime-service.ts` registry'nin listesini SDK tool'una cevirir
 * ve `execute` govdesinde yine buradaki [`executeTool`]'u cagirir.
 */

import type { AsunaToolDefinition, ToolContext, ToolResult } from './types';

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

/** [`executeTool`] ek ayarlari. */
export interface ToolExecutionOptions {
  /**
   * Cagiranin iptal sinyali (oturum kapanisi, SDK'nin kendi iptali). Timeout
   * ile birlesir: hangisi once gelirse tool'a giden `context.signal` abort olur.
   */
  readonly signal?: AbortSignal;
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

function messageOf(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return typeof error === 'string' ? error : 'bilinmeyen hata';
}

/**
 * Bir tool'u calistirmanin tek mesru yolu.
 *
 * Sirasiyla:
 * 1. **Sema dogrulamasi.** Gecersiz argumanda `execute` **hic cagrilmaz** ve
 *    yapisal bir hata doner — "once dogrula, sonra calistir" (PROJECT.md
 *    Bolum 17/18).
 * 2. **Timeout.** [`AsunaToolDefinition.timeoutMs`] dolunca `context.signal`
 *    abort edilir ve cagri yapisal bir `timeout` sonucuyla **doner**. Asili
 *    kalan bir tool oturumu kilitlemez; arkadaki is kendiliginden durmaz, bu
 *    yuzden sonuc "bitmedi" der, "basarisiz oldu" demez.
 * 3. **Yapisal sonuc.** `execute` patlarsa bile serbest metin degil,
 *    [`ToolResult`] doner. Hata yutulmaz: `ok: false` + `errorKind`.
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
  const parsed = definition.parameters.safeParse(args);
  if (!parsed.success) {
    return failure(
      `\`${definition.name}\` cagrisi gecersiz argumanlar yuzunden calistirilmadi — ` +
        `${describeIssues(parsed.error.issues)}. Dogru parametrelerle tekrar dene; ` +
        'sonucu varmis gibi konusma.',
      TOOL_ERROR_KINDS.invalidArguments,
    );
  }

  const external = options.signal;
  if (external?.aborted === true) {
    return failure(
      `\`${definition.name}\` calistirilmadi: cagri baslamadan iptal edildi.`,
      TOOL_ERROR_KINDS.aborted,
    );
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
    return await Promise.race([run(), interrupted]);
  } finally {
    clearTimeout(timer);
    external?.removeEventListener('abort', onExternalAbort);
  }
}

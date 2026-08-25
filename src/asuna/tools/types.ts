/**
 * Modele acilan yeteneklerin sozlesmesi (PROJECT.md Bolum 17, `conventions.md` — "Tool Tanimi").
 *
 * **Kapsam notu:** burada yalnizca *tip* var. Registry, permission gate ve audit yazimi
 * Phase 5'e (ASU-05x) ait. Phase 1'de servise bos dizi geciliyordu; ASU-044 ile ilk gercek
 * tool (`get_current_project`, risk 0) bu sozlesmeyi doldurdu ve `use-asuna-session`
 * varsayilan listesinden Realtime oturumuna giriyor.
 *
 * Erken soyutlama yapilmadi (PROJECT.md Bolum 39/16): sema tipi, timeout, audit alanlari
 * gercek tool'lar yazilirken eklenecek.
 */

/**
 * Risk seviyesi (PROJECT.md Bolum 5.4):
 * - `0` read-only
 * - `1` geri alinabilir dusuk risk
 * - `2` mutation
 * - `3` destructive / harici etki
 */
export type ToolRisk = 0 | 1 | 2 | 3;

/** Tool calisirken erisebilecegi oturum/proje context'i. */
export interface ToolContext {
  /**
   * Tool cagrisini ureten Realtime oturumunun kalici kaydindaki kimlik
   * (audit korelasyonu icin).
   *
   * `null` = kimlik bu cagri icin **bilinmiyor** (hafiza kapali ya da oturum
   * kaydi henuz acilmadi). Uydurulmus bir korelasyon kimligi, audit kaydini
   * dogru gorunen ama yanlis bir zincire baglardi (ASU-044). Gercek baglama
   * ASU-047'de registry + audit yazimi ile yapilacak.
   */
  readonly sessionId: string | null;
  /** Aktif projenin sandbox koku; proje secili degilse `null`. */
  readonly projectRoot: string | null;
}

/**
 * Yapisal tool sonucu. **Basari taklit edilmez** (`conventions.md` — "Hata Yonetimi"):
 * tool hata verdiyse `ok: false` doner ve model bunu oldugu gibi gorur.
 *
 * `summary` modele/kullaniciya gidebilecek kisa metindir ve **secret degeri tasimaz**.
 */
export type ToolResult =
  | { readonly ok: true; readonly summary: string; readonly data?: unknown }
  | { readonly ok: false; readonly summary: string; readonly errorKind: string };

/** Registry'ye kaydedilen tool tanimi. */
export interface AsunaToolDefinition {
  /** `snake_case`, fiil_nesne: `get_current_project`. */
  readonly name: string;
  /** Model icin net ve dar kapsam aciklamasi. */
  readonly description: string;
  readonly risk: ToolRisk;
  /**
   * Risk 2/3 icin **her zaman** `true`; bu deger `ASUNA_TOOL_APPROVAL_MODE` ile
   * gevsetilemez (`conventions.md`). Zorlama Phase 5'te registry'de yapilir.
   */
  readonly requiresApproval: boolean;
  /**
   * Tek cagri icin ust sinir (ms). Asili kalan bir tool, sesli oturumda
   * cevapsiz bir sessizlik demektir; SDK'ya `timeoutMs` olarak gecirilir
   * (`realtime-service.ts` adaptoru).
   */
  readonly timeoutMs: number;
  /** `args` bilerek `unknown`: implementasyonun ilk isi sema dogrulamasidir. */
  execute(args: unknown, context: ToolContext): Promise<ToolResult>;
}

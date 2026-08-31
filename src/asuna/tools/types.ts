/**
 * Modele acilan yeteneklerin sozlesmesi (PROJECT.md Bolum 17, `conventions.md` — "Tool Tanimi").
 *
 * **Kapsam notu:** burada sozlesme var, davranis yok. Kayit ve calistirma
 * [`ToolRegistry`](./registry.ts) / `executeTool` isidir; `tool_events` audit
 * yazimi ASU-050'ye, onay politikasi ASU-048'e ait.
 *
 * Bir tool tanimi SDK'siz **duz veri**dir: `@openai/agents-realtime` tipleri bu
 * klasore girmez, cevrim `realtime-service.ts` adaptorunde kalir
 * (`sdk-import-boundary.spec.ts` bunu tarar).
 */

import { z } from 'zod';

/**
 * Risk seviyesi (PROJECT.md Bolum 5.4):
 * - `0` read-only
 * - `1` geri alinabilir dusuk risk
 * - `2` mutation
 * - `3` destructive / harici etki
 */
export type ToolRisk = 0 | 1 | 2 | 3;

/**
 * Tool argumanlarinin semasi.
 *
 * **Object** olmasi zorunlu: function calling protokolu argumanlari adlandirilmis
 * bir nesne olarak tasir, dolayisiyla `z.string()` gibi bir sema modele
 * cevrilemez. Sema **tek kaynaktir** — hem `executeTool` dogrulamasi hem SDK'ya
 * giden JSON Schema ayni tanimdan uretilir; iki yerde tutulan bir sema er ya da
 * gec birbirinden ayrilirdi.
 */
export type ToolInputSchema = z.ZodObject;

/**
 * Parametresiz tool'larin semasi.
 *
 * `strictObject`: beklenmeyen bir alan **sessizce atilmaz**, reddedilir. Model
 * hayali bir parametre uydurdugunda (ornegin `get_current_project({ path: ... })`)
 * bunu gormek istiyoruz — sessizce yok saymak, modelin yanlis bir zihinsel
 * modelle devam etmesi demekti.
 */
export const NO_TOOL_ARGUMENTS: ToolInputSchema = z.strictObject({});

/** Tool calisirken erisebilecegi oturum/proje context'i. */
export interface ToolContext {
  /**
   * Tool cagrisini ureten Realtime oturumunun kalici kaydindaki kimlik
   * (`sessions.id`) — `tool_events.session_id` korelasyonu icin.
   *
   * `null` = kimlik bu cagri icin **bilinmiyor** (hafiza kapali ya da oturum
   * kaydi henuz acilmadi). Uydurulmus bir korelasyon kimligi, audit kaydini
   * dogru gorunen ama yanlis bir zincire baglardi (ASU-044). ASU-048'den beri
   * gercek deger `SessionRecorder`'dan gelir; hafiza kapaliyken `null` kalir ve
   * audit satiri "hangi konusmada oldugunu bilmiyoruz" der.
   */
  readonly sessionId: number | null;
  /** Aktif projenin sandbox koku; proje secili degilse `null`. */
  readonly projectRoot: string | null;
  /**
   * Iptal sinyali: timeout doldugunda ya da cagiran vazgectiginde `abort` olur.
   *
   * `executeTool` bunu **her zaman** doldurur; opsiyonel olmasinin tek sebebi
   * sarmalayicisiz (dogrudan) cagrilabilen test yollari. Uzun suren bir tool
   * (IPC, alt process) bunu dinleyip isi birakmali — timeout sonucu doner ama
   * arkada calisan is kendiliginden durmaz.
   */
  readonly signal?: AbortSignal;
}

/**
 * Yapisal tool sonucu. **Basari taklit edilmez** (`conventions.md` — "Hata Yonetimi"):
 * tool hata verdiyse `ok: false` doner ve model bunu oldugu gibi gorur.
 *
 * `summary` modele/kullaniciya gidebilecek kisa metindir ve **secret degeri tasimaz**.
 *
 * # `summary` ile `auditSummary` neden ayri (ASU-051)
 *
 * `summary` **modele** gider. Cogu tool icin bu zaten kisa bir cumledir ve
 * denetim defterine de aynen yazilabilir — bu yuzden `auditSummary`
 * opsiyoneldir ve verilmezse `summary` kullanilir.
 *
 * Ama **icerik donduren** bir tool (`read_project_file`) modele dosyanin
 * kendisini vermek zorunda. O metnin `tool_events.result_summary` alanina
 * dusmesi, migration 004'un acikca yasakladigi seydir ("dosya icerigi audit'e
 * girmez"): host tarafi 512 karakterde kirptigi icin *sizinti* kucuk olurdu ama
 * yine de bir sizintiydi. `auditSummary` bu ayrimi tip duzeyinde acar —
 * deftere ne yazilacagi tool'un bilincli karari olur, uzunluk tavaninin yan
 * etkisi degil.
 *
 * Ayni deger transcript satirinda da kullanilir (ASU-054): kullanici "README.md
 * okundu (2.1 KB)" gorur, dosyanin tamamini degil.
 */
export type ToolResult =
  | {
      readonly ok: true;
      readonly summary: string;
      readonly auditSummary?: string;
      readonly data?: unknown;
    }
  | {
      readonly ok: false;
      readonly summary: string;
      readonly errorKind: string;
      readonly auditSummary?: string;
    };

/** Registry'ye kaydedilen tool tanimi. */
export interface AsunaToolDefinition {
  /** `snake_case`, fiil_nesne: `get_current_project`. */
  readonly name: string;
  /** Model icin net ve dar kapsam aciklamasi. */
  readonly description: string;
  readonly risk: ToolRisk;
  /**
   * Risk 2/3 icin **her zaman** `true`; bu deger `ASUNA_TOOL_APPROVAL_MODE` ile
   * gevsetilemez (`conventions.md`). Zorlama kayit aninda yapilir
   * ([`ToolRegistry.register`]) — yanlis tanim modele hic acilmaz.
   */
  readonly requiresApproval: boolean;
  /**
   * Tek cagri icin ust sinir (ms). Asili kalan bir tool, sesli oturumda
   * cevapsiz bir sessizlik demektir; `executeTool` bu sureyi zorlar ve
   * `realtime-service.ts` adaptoru ayni degeri SDK'ya da verir.
   */
  readonly timeoutMs: number;
  /** Argumanlarin **tek** kaynagi. Parametresiz tool: [`NO_TOOL_ARGUMENTS`]. */
  readonly parameters: ToolInputSchema;
  /**
   * `args` bilerek `unknown`: sema disi bir deger buraya gelmemeli ama tip
   * seviyesinde "dogrulanmis" diye bir garanti verilmiyor. Tek mesru cagri
   * yolu `executeTool`'dur; o da `execute`'u yalnizca sema gectikten sonra
   * cagirir ve **parse edilmis** degeri gecirir.
   */
  execute(args: unknown, context: ToolContext): Promise<ToolResult>;
}

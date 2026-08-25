/**
 * Tool audit defterinin renderer tarafindaki **tek** erisim noktasi (ASU-050).
 *
 * # Neden bu sarmalayici digerlerinden farkli
 *
 * `memory-service.ts` ve `session-service.ts` hatayi cagirana **firlatir**.
 * Burada firlatmak yanlis olurdu: audit yazimi bir tool cagrisinin *yan* isidir
 * ve basarisiz olmasi tool sonucunu degistirmemeli. Bir `throw` ise cagiran
 * tarafi `try/catch` ile sarmaya iter — ve `catch {}` ile gecistirmek, ASU-050'nin
 * tam olarak yasakladigi seyi (sessiz kayip) yapmanin en kolay yolu olurdu.
 *
 * Bunun yerine [`recordToolEvent`] **her zaman** yapisal bir sonuc doner:
 *
 * | Sonuc | Anlami |
 * |---|---|
 * | `recorded` | Satir yazildi. |
 * | `skipped`  | Kalici hafiza kapali — satir olusmadi, bu kullanicinin karari. |
 * | `failed`   | Yazma denendi ve **basarisiz oldu**; hata sonucun icinde. |
 *
 * `failed` sessiz degil: ayni anda `error` seviyesinde log'lanir (`logger`
 * zaten redaksiyon uygular) ve `error` alani cagirana verilir. ASU-047'nin tool
 * runner'i bunu tool sonucuna karistirmadan UI'a tasiyabilir.
 *
 * # Redaksiyon burada yapilmaz
 *
 * Bilerek: ham `arguments` oldugu gibi host'a gonderilir, ozetleme ve
 * redaksiyon Rust tarafindadir (`db/tool_event_repository.rs`). Renderer'in
 * urettigi bir ozete guvenmek, redaksiyonu modelin ciktisiyla ayni process'e
 * devretmek olurdu. Rust sozlesmesi `argumentsRedacted` alanini kabul **etmez**.
 */

import { invoke } from '@tauri-apps/api/core';

import {
  parseToolEventPage,
  parseToolEventWriteResult,
  type ToolAuditInput,
  type ToolEventListQuery,
  type ToolEventPage,
  type ToolEventWriteResult,
} from '../../shared/tool-event';
import { toStoreError, type AsunaStoreError } from '../../shared/store-error';
import { logger } from '../observability';

/**
 * Rust tarafindaki komut adlari. `src-tauri/build.rs` (ACL manifest) ve
 * `src-tauri/capabilities/asuna-tool-audit-{read,write}.json` ile birebir ayni
 * olmali.
 *
 * Bu nesnede bir `delete` ya da `update` alani **yok** ve olmayacak: audit
 * defteri MVP'de salt yazilir (PROJECT.md Bolum 19).
 */
export const TOOL_AUDIT_COMMANDS = {
  record: 'record_tool_event',
  list: 'tool_event_list',
} as const;

/** Log kayitlarinda kullanilan kapsam etiketi. */
export const TOOL_AUDIT_LOG_SCOPE = 'tools.audit';

const log = logger.child(TOOL_AUDIT_LOG_SCOPE);

/**
 * [`recordToolEvent`] sonucu. `failed` bir istisna degil, **gorunur bir durum**:
 * cagiran taraf tool sonucunu degistirmeden bunu UI'a tasiyabilir.
 */
export type ToolAuditOutcome =
  ToolEventWriteResult | { readonly status: 'failed'; readonly error: AsunaStoreError };

/**
 * Bir tool cagrisini audit defterine yazar.
 *
 * **Her** cagri icin cagrilir: onaylanan, reddedilen, hata veren, zaman asimina
 * ugrayan. Yalnizca calisanlari kaydeden bir defter denetim defteri degil, bir
 * basari vitrinidir.
 *
 * Asla firlatmaz (modul dokumantasyonu). Yazma basarisiz olursa hata
 * `error` seviyesinde log'lanir **ve** sonucta doner — ikisi birden, cunku
 * log'a yazip donmemek de bir tur sessiz kayiptir (cagiran taraf durumu
 * kullaniciya gosteremez).
 */
export async function recordToolEvent(input: ToolAuditInput): Promise<ToolAuditOutcome> {
  try {
    const raw = await invoke<unknown>(TOOL_AUDIT_COMMANDS.record, { input });
    // Dogrulama bilerek `try` ICINDE (okuma tarafinin tersine): sozlesmeye
    // uymayan bir yanit, kaydin yazildigina dair guvenilir bir kanit degildir —
    // "yazildi" demek yerine `failed` demek durust olani.
    return parseToolEventWriteResult(raw);
  } catch (error) {
    const storeError = toStoreError(error);
    log.error(
      'Tool audit kaydi yazilamadi; tool sonucu degismedi ama bu cagri deftere islenmedi.',
      {
        toolName: input.toolName,
        approvalState: input.approvalState,
        code: storeError.code,
        reason: storeError.message,
      },
    );
    return { status: 'failed', error: storeError };
  }
}

/**
 * Audit defterini listeler (Tools sekmesi + oturum detayi).
 *
 * Okuma tarafi **firlatir**: burada bir ekran veri bekliyor ve "audit'e
 * bakamadim" ile "audit bos" ayni cevaplar degil (PROJECT.md Bolum 30).
 * Hafiza kapaliyken bos sayfa doner — o bir hata degil.
 */
export async function listToolEvents(query?: ToolEventListQuery): Promise<ToolEventPage> {
  let raw: unknown;
  try {
    raw = await invoke<unknown>(TOOL_AUDIT_COMMANDS.list, { query: query ?? null });
  } catch (error) {
    throw toStoreError(error);
  }
  // Dogrulama `try` DISINDA: bir sozlesme ihlali (`ToolEventContractError`) bir
  // depolama arizasi degildir ve `AsunaStoreError` kiligina sokulmamali —
  // `memory-service.ts` ile ayni ayrim.
  return parseToolEventPage(raw);
}

/**
 * "Araçlar" sekmesi (ASU-054).
 *
 * # Neden var
 *
 * PROJECT.md Bolum 19: *"The user should never wonder whether the agent is
 * silently modifying the computer."* Bu ekran o cumlenin karsiligi ve iki
 * soruya cevap verir:
 *
 * 1. **Ne yapabilir?** — modele acik tool listesi, risk seviyeleri, onay
 *    politikasi ve tek tek kapatma anahtari.
 * 2. **Ne yapti?** — `tool_events` denetim defteri: onaylanan, reddedilen,
 *    zaman asimina ugrayan, hata veren her cagri.
 *
 * # Sinirlar
 *
 * - Defter **salt okunur**. Bu bilesende silme/duzenleme yolu yok cunku
 *   servis katmaninda da yok (ASU-050): `TOOL_AUDIT_COMMANDS` yalnizca
 *   `record` ve `list` icerir.
 * - Bilesen SQL gormez, `invoke` cagirmaz: her sey [`ToolsViewPort`] uzerinden
 *   servis katmanina gider (ADR-005), `memory-view.tsx` ile ayni desen.
 * - Kapatma **oturum-yereldir** (bellekte), kalici bir ayar degil — kullanici
 *   bunu ekranda okur, tahmin etmez.
 * - Metin duz render edilir; `dangerouslySetInnerHTML` yok (arguman ozeti ve
 *   sonuc ozeti model/tool ciktisindan turer).
 */

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';

import { loadFrontendConfig } from '../asuna/config/config.service';
import type { ToolApprovalMode } from '../asuna/config/frontend-config';
import { logger } from '../asuna/observability';
import { buildToolSummaries, type ToolToggleStore } from '../asuna/tools';
import type { AsunaToolDefinition } from '../asuna/tools/types';
import { listToolEvents } from '../asuna/tools/audit';
import type { ToolEventListQuery, ToolEventPage, ToolEventRecord } from '../shared/tool-event';
import type { ToolSummary } from '../shared/tools';

import { formatMemoryTimestamp } from './memory-text';
import {
  TOOL_APPROVAL_POLICY_LABELS,
  TOOL_APPROVAL_STATE_LABELS,
  TOOL_OUTCOME_LABELS,
  describeToolError,
  describeToolRisk,
  riskAttribute,
} from './tool-text';

/** Bir sayfada istenen audit kaydi sayisi. */
export const TOOL_EVENT_PAGE_SIZE = 25;

/** "Tum oturumlar" secimi — `<select>` degeri bos olamayacagi icin sabit. */
const ALL_SESSIONS = 'all';

/**
 * Bilesenin **audit** servis yuzeyi. Testler gercek IPC'ye dokunmadan sahte
 * port verir; uretimde [`DEFAULT_TOOLS_PORT`] kullanilir.
 *
 * Tool listesi bu portta **yok** ve olmayacak: liste ile anahtarlar oturumla
 * ayni kaynaktan gelmeli (bkz. [`ToolsViewProps.definitions`]), ekranin kendi
 * basina turettigi bir liste oturumdakinden ayrisabilirdi.
 */
export interface ToolsViewPort {
  readonly listEvents: (query: ToolEventListQuery) => Promise<ToolEventPage>;
}

export interface ToolsViewProps {
  /**
   * Oturuma verilen tool tanimlarinin **ayni** listesi (ASU-054).
   *
   * Zorunlu prop: `App` kompozisyon kokunde listeyi bir kez kurar ve hem
   * `useAsunaSession` secenegi (`options.tools`) hem buraya verir. Ekranin
   * `asunaToolRegistry`'yi kendi basina okumasi, oturuma daraltilmis bir liste
   * verildigi anda modele **acik olmayan** tool'lari "Açık" gostermek olurdu.
   */
  readonly definitions: readonly AsunaToolDefinition[];
  /**
   * Canli oturumun **paylasilan** tool anahtarlari (ASU-054).
   *
   * `App` bu store'u kurar ve ayni ornegi hem `useAsunaSession` secenegi olarak
   * hem buraya verir. Paylasilmasi sart: `executeTool` kapisi bu store'a sorar,
   * ayri bir ornek uzerinden yapilan kapatma ekranda "Kapalı" gorunur ama
   * cagriyi durdurmazdi — yani ekran yalan soylerdi.
   */
  readonly toggles: ToolToggleStore;
  readonly port?: ToolsViewPort;
  readonly pageSize?: number;
}

export function ToolsView({
  definitions,
  toggles,
  port = DEFAULT_TOOLS_PORT,
  pageSize = TOOL_EVENT_PAGE_SIZE,
}: ToolsViewProps): React.JSX.Element {
  const [limit, setLimit] = useState(pageSize);
  const [sessionFilter, setSessionFilter] = useState<string>(ALL_SESSIONS);
  const [page, setPage] = useState<ToolEventPage | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  // Liste her render'da kaynagindan turer; kopyasi tutulmaz. Kapatilan bir
  // tool'un ekranda hala "Açık" gorunmesi, bu ekranin varlik sebebine aykiri
  // olurdu.
  const visibleTools = useLiveToolSummaries(definitions, toggles);

  useEffect(() => {
    let cancelled = false;
    const query: ToolEventListQuery =
      sessionFilter === ALL_SESSIONS
        ? { limit }
        : { limit, sessionId: Number.parseInt(sessionFilter, 10) };

    port.listEvents(query).then(
      (result) => {
        if (!cancelled) {
          setPage(result);
          setLoadError(null);
        }
      },
      (error: unknown) => {
        if (cancelled) {
          return;
        }
        // "Deftere bakamadim" ile "defter bos" ayni cevap degil: hata varken
        // bayat liste gosterilmez (PROJECT.md Bolum 30).
        setPage(null);
        setLoadError(describeToolError(error));
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [port, limit, sessionFilter]);

  // Paylasilan store: `executeTool` kapisi da bunu okur, yani anahtarin
  // ekrandaki hali ile calisma zamanindaki hali ayni kaynaktan gelir. Yeniden
  // render'i store'un abonelik zinciri tetikler (`useLiveToolSummaries`).
  const handleToggle = useCallback(
    (name: string, enabled: boolean): void => {
      toggles.setEnabled(name, enabled);
    },
    [toggles],
  );

  const events = page?.events ?? [];
  const total = page?.total ?? 0;
  // Filtre secenekleri **gorunen** kayitlardan turer: ayri bir "oturumlari
  // listele" cagrisi yapilmaz, cunku burada anlamli kume defterde gercekten
  // cagrisi olan oturumlardir. Secili oturum her zaman listede kalir; "Tüm
  // oturumlar" secenegi de her zaman durur, yani geri donus yolu kapanmaz.
  const sessionIds = sessionOptions(events, sessionFilter);

  return (
    <section className="asuna-tools" aria-label="Araçlar">
      <p className="asuna-tools__note">
        Gizli araç çalıştırma yolu yok — Asuna’nın yaptığı her çağrı burada görünür.
      </p>

      <h3 className="asuna-tools__heading">Etkin araçlar</h3>
      {visibleTools.length === 0 ? (
        <p className="asuna-tools__note">Modele açık bir araç yok.</p>
      ) : (
        <ul className="asuna-tools__list">
          {visibleTools.map((tool) => (
            <li
              key={tool.name}
              className="asuna-tools__item"
              data-risk={riskAttribute(tool.risk)}
              data-enabled={tool.enabled ? 'true' : 'false'}
            >
              <div className="asuna-tools__item-head">
                <span className="asuna-tools__name">{tool.name}</span>
                <span className="asuna-tools__badge">{describeToolRisk(tool.risk)}</span>
                <span className="asuna-tools__badge">
                  {TOOL_APPROVAL_POLICY_LABELS[tool.approval]}
                </span>
              </div>

              <p className="asuna-tools__description">{tool.description}</p>

              <label className="asuna-tools__switch">
                <input
                  type="checkbox"
                  role="switch"
                  checked={tool.enabled}
                  aria-label={`${tool.name} aracı`}
                  onChange={(event): void => {
                    handleToggle(tool.name, event.target.checked);
                  }}
                />
                <span>{tool.enabled ? 'Açık' : 'Kapalı — model bu aracı görmez'}</span>
              </label>
            </li>
          ))}
        </ul>
      )}
      {visibleTools.length > 0 && (
        <p className="asuna-tools__note">
          Kapatma bu oturum için geçerlidir; uygulama yeniden başlayınca araçlar yine açılır.
        </p>
      )}

      <h3 className="asuna-tools__heading">Çağrı geçmişi</h3>
      <p className="asuna-tools__note">
        Denetim defteri salt okunurdur: kayıtlar uygulamadan silinemez, düzenlenemez.
      </p>

      <label className="asuna-tools__filter">
        <span>Oturum</span>
        <select
          value={sessionFilter}
          onChange={(event): void => {
            setSessionFilter(event.target.value);
            setLimit(pageSize);
          }}
        >
          <option value={ALL_SESSIONS}>Tüm oturumlar</option>
          {sessionIds.map((id) => (
            <option key={id} value={id.toString()}>
              Oturum #{id.toString()}
            </option>
          ))}
        </select>
      </label>

      {loadError !== null && (
        <p className="asuna-tools__notice" role="alert">
          {loadError}
        </p>
      )}

      {loadError === null &&
        (events.length === 0 ? (
          <p className="asuna-tools__note">Bu filtreye uyan araç çağrısı yok.</p>
        ) : (
          <>
            <ul className="asuna-tools__events">
              {events.map((event) => (
                <ToolEventRow key={event.id} event={event} />
              ))}
            </ul>
            <p className="asuna-tools__note">
              {events.length.toString()} / {total.toString()} kayıt gösteriliyor.
            </p>
            {events.length < total && (
              <button
                type="button"
                className="asuna-tools__more"
                onClick={(): void => {
                  setLimit((current) => current + pageSize);
                }}
              >
                Daha fazla göster
              </button>
            )}
          </>
        ))}
    </section>
  );
}

interface ToolEventRowProps {
  readonly event: ToolEventRecord;
}

function ToolEventRow({ event }: ToolEventRowProps): React.JSX.Element {
  // `outcome` migration 005 ile geldi; `null` = bu satir kolon eklenmeden
  // yazilmis, yani sonuc bilinmiyor. "Basarili" varsayilmaz.
  const outcome = event.outcome;

  return (
    <li
      className="asuna-tools__event"
      data-risk={riskAttribute(event.riskLevel)}
      data-outcome={outcome ?? 'unknown'}
    >
      <p className="asuna-tools__event-head">
        <span className="asuna-tools__name">{event.toolName}</span>
        <span className="asuna-tools__badge">{formatMemoryTimestamp(event.createdAt)}</span>
        <span className="asuna-tools__badge">{describeToolRisk(event.riskLevel)}</span>
        <span className="asuna-tools__badge">
          {TOOL_APPROVAL_STATE_LABELS[event.approvalState]}
        </span>
        {outcome !== null && (
          <span className="asuna-tools__badge">{TOOL_OUTCOME_LABELS[outcome]}</span>
        )}
      </p>
      <p className="asuna-tools__event-args">{event.argumentsRedacted ?? 'Argümansız çağrı'}</p>
      {/* Sonuc ozeti yoksa bu **gizlenmez**: "sonuc yazilmadi" da bir bilgidir. */}
      <p className="asuna-tools__event-result">
        {event.resultSummary ?? 'Sonuç özeti yok (çağrı çalışmamış olabilir).'}
      </p>
      <p className="asuna-tools__event-meta">
        {event.sessionId === null
          ? 'Oturum bilinmiyor'
          : `Oturum #${event.sessionId.toString()}`}
      </p>
    </li>
  );
}

const toolsLog = logger.child('ui.tools');

/**
 * Verilen tanim listesinden ve paylasilan anahtar setinden tool ozetlerini
 * turetir.
 *
 * Turetme `buildToolSummaries` ile yapilir — `useAsunaSession().tools` ile
 * **ayni** fonksiyon ve **ayni** girdiler (tanimlar + anahtarlar). Ekran ile
 * oturum boylece tek kaynaktan konusur: modele verilmeyen bir tool burada
 * "Açık" gorunemez.
 *
 * Onay modu okunana kadar **en siki** mod (`always`) varsayilir: config
 * gelmeden "bu onaysiz calisir" diye bir soz verilmez (`executeTool` de ayni
 * varsayilani kullanir).
 */
function useLiveToolSummaries(
  definitions: readonly AsunaToolDefinition[],
  toggles: ToolToggleStore,
): readonly ToolSummary[] {
  const [mode, setMode] = useState<ToolApprovalMode>('always');

  useEffect(() => {
    let cancelled = false;

    loadFrontendConfig().then(
      (config) => {
        if (!cancelled) {
          setMode(config.toolApprovalMode);
        }
      },
      (error: unknown) => {
        // Yutulmaz ama ekrani da dusurmez: liste yine gorunur, yalnizca politika
        // sutunu en siki degerde kalir.
        toolsLog.warn('Tool onay modu okunamadi; en siki mod varsayiliyor.', {
          reason: error instanceof Error ? error.message : String(error),
        });
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, []);

  // Snapshot'in degeri kullanilmaz; amaci anahtar seti degisince yeniden render
  // etmek. Liste asagida her render'da store'dan tazeden okunur.
  useSyncExternalStore(
    useCallback((onStoreChange: () => void) => toggles.subscribe(onStoreChange), [toggles]),
    useCallback(() => toggles.disabledNames, [toggles]),
    useCallback(() => toggles.disabledNames, [toggles]),
  );

  return buildToolSummaries(definitions, mode, (toolName) => toggles.isEnabled(toolName));
}

/**
 * Filtre seceneklerini gorunen kayitlardan turetir.
 *
 * Secili oturum listeye zorla eklenir: sunucu o oturuma ait kayit dondurmese
 * bile `<select>` bos bir degere dusmemeli.
 */
function sessionOptions(
  events: readonly ToolEventRecord[],
  selected: string,
): readonly number[] {
  const ids = new Set(
    events.map((event) => event.sessionId).filter((id): id is number => id !== null),
  );

  if (selected !== ALL_SESSIONS) {
    ids.add(Number.parseInt(selected, 10));
  }

  return [...ids].sort((left, right) => right - left);
}

/**
 * Uretim portu.
 *
 * Yalnizca audit okumasi: dogrudan servis katmanina gider. Tool listesi ve
 * anahtarlar porttan **gelmez**, prop olarak gelir (bkz. `ToolsViewProps`).
 */
const DEFAULT_TOOLS_PORT: ToolsViewPort = {
  listEvents: listToolEvents,
};

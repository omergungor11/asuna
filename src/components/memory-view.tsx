/**
 * Hafiza sekmesi (ASU-036) — listele / ara / sil / arsivle.
 *
 * # Neden var
 *
 * PROJECT.md Bolum 20: "Memory storage is inspectable. User can delete memories."
 * Bu MVP kabul maddesi; hafizasi gorulemeyen bir asistan guvenilmezdir. Ekran
 * dashboard degil, **denetim yuzeyidir**: her kaydin nereden geldigi (kaynak
 * oturum) ve ne zaman olustugu gorunur, her kayit tek tikla arsivlenip
 * silinebilir.
 *
 * # Sinirlar
 *
 * - Bu bilesen SQL gormez, `invoke` cagirmaz: her sey [`MemoryViewPort`]
 *   uzerinden `src/asuna/memory/*` servislerine gider (ADR-005).
 * - Servis katmani `limit` disinda sayfalama sunmaz (offset yok). Bu yuzden
 *   "daha fazla yukle" tavani buyutur ve listeyi tazeler — sanal liste
 *   kutuphanesi eklenmez, DOM'daki kayit sayisini kullanici belirler.
 * - Liste goruntulemek **erisim degildir**: `markAccessed` gonderilmez, aksi
 *   halde Stage A siralamasi (ASU-035) UI'da gezinmekten etkilenirdi.
 * - Hafiza `disabled` ile `unavailable` ayri gosterilir; "kapali" ile "bozuk"
 *   ayni ekrana dusmez (PROJECT.md Bolum 30).
 */

import { useCallback, useEffect, useState } from 'react';

import { fetchDbStatus } from '../asuna/memory/db-status-service';
import {
  archiveMemory,
  deleteMemory,
  listMemories,
  updateMemory,
} from '../asuna/memory/memory-service';
import type { DbStatus } from '../shared/db-status';
import {
  wasMemoryStored,
  type MemoryArchiveFilter,
  type MemoryFilter,
  type MemoryPatch,
  type MemoryRecord,
  type MemoryWriteResult,
} from '../shared/memory';

import { ALL_KINDS, MemoryFilters, type KindFilterValue } from './memory-filters';
import { MemoryItem } from './memory-item';
import { describeMemoryError } from './memory-text';
import { PendingApprovals } from './pending-approvals';

/** Bir sayfada istenen kayit sayisi. */
export const MEMORY_PAGE_SIZE = 25;

/** Arama kutusunda yazma durduktan sonra sorgunun gitmesi icin beklenen sure. */
export const SEARCH_DEBOUNCE_MS = 250;

/**
 * Bilesenin servis yuzeyi. Testler gercek IPC'ye dokunmadan sahte port verir;
 * uretimde [`DEFAULT_MEMORY_PORT`] kullanilir.
 */
export interface MemoryViewPort {
  readonly fetchStatus: () => Promise<DbStatus>;
  readonly list: (filter: MemoryFilter) => Promise<readonly MemoryRecord[]>;
  readonly archive: (id: number, archived: boolean) => Promise<MemoryWriteResult>;
  readonly remove: (id: number) => Promise<MemoryWriteResult>;
  /** Onay bekleyen kaydin bayragini kaldirmak icin (ASU-037). */
  readonly update: (id: number, patch: MemoryPatch) => Promise<MemoryWriteResult>;
}

/** Uretim portu: dogrudan servis katmani. */
const DEFAULT_MEMORY_PORT: MemoryViewPort = {
  fetchStatus: fetchDbStatus,
  list: listMemories,
  archive: archiveMemory,
  remove: deleteMemory,
  update: updateMemory,
};

type StatusState =
  | { readonly phase: 'checking' }
  | { readonly phase: 'known'; readonly status: DbStatus }
  | { readonly phase: 'error'; readonly message: string };

interface ActionMessage {
  readonly tone: 'info' | 'error';
  readonly text: string;
}

export interface MemoryViewProps {
  readonly port?: MemoryViewPort;
  readonly pageSize?: number;
  readonly searchDebounceMs?: number;
}

export function MemoryView({
  port = DEFAULT_MEMORY_PORT,
  pageSize = MEMORY_PAGE_SIZE,
  searchDebounceMs = SEARCH_DEBOUNCE_MS,
}: MemoryViewProps): React.JSX.Element {
  const [status, setStatus] = useState<StatusState>({ phase: 'checking' });

  const [searchDraft, setSearchDraft] = useState('');
  const [search, setSearch] = useState('');
  const [kind, setKind] = useState<KindFilterValue>(ALL_KINDS);
  const [archived, setArchived] = useState<MemoryArchiveFilter>('active');
  const [limit, setLimit] = useState(pageSize);

  const [records, setRecords] = useState<readonly MemoryRecord[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadedKey, setLoadedKey] = useState<string | null>(null);
  const [reloadToken, setReloadToken] = useState(0);

  const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [actionMessage, setActionMessage] = useState<ActionMessage | null>(null);

  const availability = status.phase === 'known' ? status.status.availability : null;

  /**
   * Su an ekranda olmasi gereken sorgunun kimligi.
   *
   * "Yukleniyor" bir state degil, **turetilmis** bir gercek: en son tamamlanan
   * sorgu ile istenen sorgu ayni degilse liste bayattir. Effect icinde
   * `setLoading(true)` cagirmak zincirleme render uretirdi (react-hooks
   * `set-state-in-effect`).
   */
  const requestKey = [archived, kind, search, limit.toString(), reloadToken.toString()].join(
    '\u0000',
  );
  const loading = loadedKey !== requestKey;

  // 1) Hafiza durumu. Onbelleklenmez (bkz. db-status-service): sekme her
  //    acildiginda yeniden sorulur, "hatirliyorum" iddiasi tazedir.
  useEffect(() => {
    let cancelled = false;
    port.fetchStatus().then(
      (value) => {
        if (!cancelled) {
          setStatus({ phase: 'known', status: value });
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          setStatus({ phase: 'error', message: describeMemoryError(error) });
        }
      },
    );
    return (): void => {
      cancelled = true;
    };
  }, [port]);

  // 2) Arama debounce'u: her tusa basista IPC cagrisi gitmez.
  useEffect(() => {
    if (searchDraft === search) {
      return undefined;
    }
    const timer = setTimeout(() => {
      setSearch(searchDraft);
      setLimit(pageSize);
      setPendingDeleteId(null);
    }, searchDebounceMs);
    return (): void => {
      clearTimeout(timer);
    };
  }, [searchDraft, search, pageSize, searchDebounceMs]);

  // 3) Liste. Yalnizca hafiza gercekten hazirken sorgu atilir — kapaliyken
  //    bos liste cizmek yerine durum acikca yazilir.
  useEffect(() => {
    if (availability !== 'ready') {
      return undefined;
    }

    let cancelled = false;

    // `exactOptionalPropertyTypes`: verilmeyen filtre alani `undefined` olarak
    // degil, hic gonderilmez.
    const filter: MemoryFilter = {
      archived,
      sort: 'recent',
      limit,
      ...(kind === ALL_KINDS ? {} : { kinds: [kind] }),
      ...(search === '' ? {} : { search }),
    };

    port.list(filter).then(
      (result) => {
        if (cancelled) {
          return;
        }
        setRecords(result);
        setLoadError(null);
        setLoadedKey(requestKey);
      },
      (error: unknown) => {
        if (cancelled) {
          return;
        }
        // Hata varken bayat liste gosterilmez: ekranda ya dogru veri olur ya da
        // neden olmadigi yazar.
        setRecords([]);
        setLoadError(describeMemoryError(error));
        setLoadedKey(requestKey);
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [port, availability, archived, kind, search, limit, requestKey]);

  /**
   * Yazma islemlerinin ortak yolu.
   *
   * `skipped` sonucu **basari sayilmaz**: hafiza kapaliyken kullaniciya
   * "sildim" denmez (PROJECT.md Bolum 20).
   */
  const runWrite = useCallback(
    (id: number, operation: () => Promise<MemoryWriteResult>): void => {
      setBusyId(id);
      setActionMessage(null);

      operation().then(
        (result) => {
          setBusyId(null);
          setPendingDeleteId(null);
          if (!wasMemoryStored(result)) {
            setActionMessage({
              tone: 'info',
              text: 'Hafıza kapalı olduğu için işlem uygulanmadı.',
            });
            return;
          }
          // Listeyi tazele: silinen kayit ekranda kalmasin, sayim dogru olsun.
          setReloadToken((token) => token + 1);
        },
        (error: unknown) => {
          setBusyId(null);
          setPendingDeleteId(null);
          setActionMessage({ tone: 'error', text: describeMemoryError(error) });
        },
      );
    },
    [],
  );

  const handleConfirmDelete = useCallback(
    (id: number): void => {
      runWrite(id, () => port.remove(id));
    },
    [port, runWrite],
  );

  const handleToggleArchive = useCallback(
    (record: MemoryRecord): void => {
      runWrite(record.id, () => port.archive(record.id, !record.isArchived));
    },
    [port, runWrite],
  );

  /** Onay kuyrugu degistiginde ana liste de tazelenir. */
  const handleApprovalsChanged = useCallback((): void => {
    setReloadToken((token) => token + 1);
  }, []);

  if (status.phase === 'checking') {
    return (
      <section className="asuna-memory" aria-label="Hafıza">
        <p className="asuna-memory__notice">Hafıza durumu kontrol ediliyor…</p>
      </section>
    );
  }

  if (status.phase === 'error') {
    return (
      <section className="asuna-memory" aria-label="Hafıza">
        <p className="asuna-memory__notice" role="alert">
          Hafıza durumu okunamadı: {status.message}
        </p>
      </section>
    );
  }

  if (status.status.availability === 'disabled') {
    return (
      <section className="asuna-memory" aria-label="Hafıza">
        <p className="asuna-memory__notice">
          Hafıza kapalı (ASUNA_MEMORY_ENABLED=false). Hiçbir şey kaydedilmiyor, bu yüzden
          listelenecek kayıt da yok.
        </p>
      </section>
    );
  }

  if (status.status.availability === 'unavailable') {
    return (
      <section className="asuna-memory" aria-label="Hafıza">
        {/* Kapali degil, BOZUK — ikisi ayni gorunmemeli. */}
        <p className="asuna-memory__notice" role="alert">
          Hafıza kullanılamıyor: {status.status.reason ?? 'neden bildirilmedi'}
        </p>
      </section>
    );
  }

  const showLoadMore = !loading && loadError === null && records.length >= limit;

  return (
    <section className="asuna-memory" aria-label="Hafıza">
      {/* Onay kuyrugu filtrelerin ustunde: bekleyen bir karar, arama yapmadan
          once gorulmeli. Kuyruk bossa bolum hic cizilmez. */}
      <PendingApprovals
        list={port.list}
        update={port.update}
        remove={port.remove}
        onChanged={handleApprovalsChanged}
      />

      <MemoryFilters
        search={searchDraft}
        kind={kind}
        archived={archived}
        onSearchChange={setSearchDraft}
        onKindChange={(value): void => {
          setKind(value);
          setLimit(pageSize);
          setPendingDeleteId(null);
        }}
        onArchivedChange={(value): void => {
          setArchived(value);
          setLimit(pageSize);
          setPendingDeleteId(null);
        }}
      />

      {actionMessage !== null && (
        <p
          className="asuna-memory__notice"
          role={actionMessage.tone === 'error' ? 'alert' : 'status'}
        >
          {actionMessage.text}
        </p>
      )}

      {loadError !== null && (
        <p className="asuna-memory__notice" role="alert">
          {loadError}
        </p>
      )}

      {loading && records.length === 0 && (
        <p className="asuna-memory__notice">Hafıza yükleniyor…</p>
      )}

      {!loading && loadError === null && records.length === 0 && (
        <p className="asuna-memory__notice">
          {search === '' && kind === ALL_KINDS
            ? 'Henüz kayıtlı hafıza yok.'
            : 'Bu filtreye uyan hafıza yok.'}
        </p>
      )}

      {records.length > 0 && (
        <ul className="asuna-memory__list" aria-label="Hafıza kayıtları">
          {records.map((record) => (
            <MemoryItem
              key={record.id}
              record={record}
              confirmingDelete={pendingDeleteId === record.id}
              busy={busyId === record.id}
              onRequestDelete={(id): void => {
                setPendingDeleteId(id);
                setActionMessage(null);
              }}
              onCancelDelete={(): void => {
                setPendingDeleteId(null);
              }}
              onConfirmDelete={handleConfirmDelete}
              onToggleArchive={handleToggleArchive}
            />
          ))}
        </ul>
      )}

      {showLoadMore && (
        <button
          type="button"
          className="asuna-memory__more"
          onClick={(): void => {
            setLimit(limit + pageSize);
          }}
        >
          Daha fazla yükle
        </button>
      )}
    </section>
  );
}

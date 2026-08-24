/**
 * Hafiza listesinin filtre serisi (ASU-036): metin aramasi, `kind` ve arsiv gorunumu.
 *
 * Saf sunum. Arama kutusu **denetimli**: yazilan deger ust bilesende debounce
 * edilir, her tusa basista IPC cagrisi gitmez.
 */

import { MEMORY_KINDS, type MemoryArchiveFilter, type MemoryKind } from '../shared/memory';

import { MEMORY_KIND_LABELS } from './memory-text';

/** `kind` filtresinin "hepsi" degeri — `MemoryKind` ile karismasin diye ayri sabit. */
export const ALL_KINDS = 'all';

export type KindFilterValue = MemoryKind | typeof ALL_KINDS;

const ARCHIVE_OPTIONS: readonly {
  readonly value: MemoryArchiveFilter;
  readonly label: string;
}[] = [
  { value: 'active', label: 'Arşivde olmayanlar' },
  { value: 'archived', label: 'Yalnızca arşiv' },
  { value: 'all', label: 'Hepsi' },
];

export interface MemoryFiltersProps {
  readonly search: string;
  readonly kind: KindFilterValue;
  readonly archived: MemoryArchiveFilter;
  readonly onSearchChange: (value: string) => void;
  readonly onKindChange: (value: KindFilterValue) => void;
  readonly onArchivedChange: (value: MemoryArchiveFilter) => void;
}

function isKindFilterValue(value: string): value is KindFilterValue {
  return value === ALL_KINDS || (MEMORY_KINDS as readonly string[]).includes(value);
}

function isArchiveFilter(value: string): value is MemoryArchiveFilter {
  return value === 'active' || value === 'archived' || value === 'all';
}

export function MemoryFilters({
  search,
  kind,
  archived,
  onSearchChange,
  onKindChange,
  onArchivedChange,
}: MemoryFiltersProps): React.JSX.Element {
  return (
    <div className="asuna-memory-filters">
      <label className="asuna-memory-filters__field">
        <span>Ara</span>
        <input
          type="search"
          value={search}
          placeholder="başlık, içerik veya özet"
          onChange={(event): void => {
            onSearchChange(event.currentTarget.value);
          }}
        />
      </label>

      <label className="asuna-memory-filters__field">
        <span>Tür</span>
        <select
          value={kind}
          onChange={(event): void => {
            const next = event.currentTarget.value;
            // `select` degeri string doner; union'a *iddia* etmeden dogrulanir.
            if (isKindFilterValue(next)) {
              onKindChange(next);
            }
          }}
        >
          <option value={ALL_KINDS}>Hepsi</option>
          {MEMORY_KINDS.map((value) => (
            <option key={value} value={value}>
              {MEMORY_KIND_LABELS[value]}
            </option>
          ))}
        </select>
      </label>

      <label className="asuna-memory-filters__field">
        <span>Arşiv</span>
        <select
          value={archived}
          onChange={(event): void => {
            const next = event.currentTarget.value;
            if (isArchiveFilter(next)) {
              onArchivedChange(next);
            }
          }}
        >
          {ARCHIVE_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

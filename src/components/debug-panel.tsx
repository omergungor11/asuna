/**
 * Gelistirme modundaki log paneli (ASU-019).
 *
 * Kaynak: `PROJECT.md` Bolum 29 — "In development, provide a debug console."
 *
 * Kapsam sinirlari:
 * - **Sadece dev.** `src/app/app.tsx` bu bileseni yalnizca `import.meta.env.DEV`
 *   dogruyken `lazy()` ile yukler; uretim build'inde dal olu koda dusup elenir.
 * - **Guzellestirme yok.** UI ana urun degil (PROJECT.md Bolum 21); panel
 *   gorunurluk icin var. Stiller bilerek satir ici ve minimal — global tema
 *   dosyalarina (`app.css`) dokunmaz.
 * - **Durum uretmez.** Tampondan okur, `useSyncExternalStore` ile abone olur;
 *   paralel bir log kopyasi tutmaz (conventions.md "Frontend").
 */

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';

import {
  LOG_LEVELS,
  formatLogEntry,
  isLevelEnabledFor,
  logBuffer,
  type LogEntry,
  type LogLevel,
  type LogRingBuffer,
} from '../asuna/observability/logger';

const LEVEL_COLORS: Readonly<Record<LogLevel, string>> = {
  error: '#ff6b6b',
  warn: '#ffd166',
  info: '#d0d0d0',
  debug: '#8a8a8a',
};

const styles = {
  panel: {
    position: 'fixed',
    right: 0,
    bottom: 0,
    width: 'min(560px, 100vw)',
    maxHeight: '45vh',
    display: 'flex',
    flexDirection: 'column',
    background: '#101014',
    color: '#d0d0d0',
    border: '1px solid #2a2a30',
    borderRadius: '6px 0 0 0',
    font: '11px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace',
    zIndex: 9999,
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '4px 8px',
    borderBottom: '1px solid #2a2a30',
  },
  title: { fontWeight: 600, marginRight: 'auto' },
  list: {
    overflowY: 'auto',
    padding: '4px 8px',
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
  },
  empty: { color: '#6a6a6a', padding: '4px 0' },
} as const satisfies Record<string, React.CSSProperties>;

export interface DebugPanelProps {
  /** Varsayilan uygulama tamponu; testte/izole kullanimda degistirilebilir. */
  readonly buffer?: LogRingBuffer;
}

export function DebugPanel({ buffer = logBuffer }: DebugPanelProps): React.JSX.Element {
  const subscribe = useCallback(
    (onStoreChange: () => void) => buffer.subscribe(onStoreChange),
    [buffer],
  );
  const getSnapshot = useCallback((): readonly LogEntry[] => buffer.getSnapshot(), [buffer]);
  const entries = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const [threshold, setThreshold] = useState<LogLevel>('debug');
  const [autoScroll, setAutoScroll] = useState(true);
  const [collapsed, setCollapsed] = useState(false);
  const listRef = useRef<HTMLDivElement | null>(null);

  const visible = entries.filter((entry) => isLevelEnabledFor(entry.level, threshold));

  useEffect(() => {
    if (!autoScroll || collapsed) {
      return;
    }
    const node = listRef.current;
    if (node !== null) {
      node.scrollTop = node.scrollHeight;
    }
  }, [visible.length, autoScroll, collapsed]);

  return (
    <aside style={styles.panel} aria-label="Asuna log paneli">
      <div style={styles.header}>
        <span style={styles.title}>
          Asuna log ({visible.length.toString()}/{buffer.capacity.toString()})
        </span>

        <label htmlFor="asuna-log-level">seviye</label>
        <select
          id="asuna-log-level"
          value={threshold}
          onChange={(event): void => {
            setThreshold(event.target.value as LogLevel);
          }}
        >
          {LOG_LEVELS.map((level) => (
            <option key={level} value={level}>
              {level}
            </option>
          ))}
        </select>

        <label htmlFor="asuna-log-autoscroll">otomatik kaydir</label>
        <input
          id="asuna-log-autoscroll"
          type="checkbox"
          checked={autoScroll}
          onChange={(event): void => {
            setAutoScroll(event.target.checked);
          }}
        />

        <button
          type="button"
          onClick={(): void => {
            buffer.clear();
          }}
        >
          temizle
        </button>
        <button
          type="button"
          onClick={(): void => {
            setCollapsed((previous) => !previous);
          }}
        >
          {collapsed ? 'ac' : 'kapat'}
        </button>
      </div>

      {!collapsed && (
        <div style={styles.list} ref={listRef} role="log" aria-live="off">
          {visible.length === 0 ? (
            <div style={styles.empty}>Bu seviyede log yok.</div>
          ) : (
            visible.map((entry, index) => (
              <div
                key={`${entry.at}-${index.toString()}`}
                style={{ color: LEVEL_COLORS[entry.level] }}
              >
                {formatLogEntry(entry)}
              </div>
            ))
          )}
        </div>
      )}
    </aside>
  );
}

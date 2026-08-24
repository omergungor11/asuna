/**
 * Mikrofon gostergesi (ASU-015).
 *
 * PROJECT.md Bolum 19: kullanici mikrofonun ne zaman acik oldugunu **her an**
 * gorebilmeli. Gosterge oturumun gercek durumundan turer; ayri bir "acik sanilan"
 * bayrak tutulmaz.
 */

export interface MicIndicatorProps {
  readonly active: boolean;
}

export function MicIndicator({ active }: MicIndicatorProps): React.JSX.Element {
  return (
    <span className="asuna-mic" data-active={active ? 'true' : 'false'}>
      <span className="asuna-mic__dot" aria-hidden="true" />
      {active ? 'Mikrofon açık' : 'Mikrofon kapalı'}
    </span>
  );
}

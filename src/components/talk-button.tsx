/**
 * Tek buton: "Talk to Asuna" / "Stop" (ASU-015).
 *
 * TEMPORARY: ASU-023 wake word ile degistirilecek. Bu buton wake word'un yerini
 * gecici olarak tutar (TRANSCRIPT.md Bolum 20); Phase 2'de birincil aktivasyon
 * "Hey Asuna" olur ve buton ikincil/yedek hale gelir.
 *
 * Guzellestirme yok: UI Phase 1'de guven ve gorunurluk icin var, urun degil
 * (PROJECT.md Bolum 21).
 */

export interface TalkButtonProps {
  readonly connected: boolean;
  /** Aktivasyon suruyor — buton kilitli, cift tiklama yaris kosulu uretemez. */
  readonly busy: boolean;
  /** Kurtarilamaz hatada tekrar denemek anlamsiz; buton kapali kalir. */
  readonly disabled?: boolean;
  readonly onStart: () => void;
  readonly onStop: () => void;
}

function label(connected: boolean, busy: boolean): string {
  if (busy) {
    return 'Bağlanıyor…';
  }
  return connected ? 'Stop' : 'Talk to Asuna';
}

export function TalkButton({
  connected,
  busy,
  disabled = false,
  onStart,
  onStop,
}: TalkButtonProps): React.JSX.Element {
  return (
    <button
      type="button"
      className="asuna-talk-button"
      aria-busy={busy}
      disabled={busy || disabled}
      onClick={(): void => {
        if (connected) {
          onStop();
          return;
        }
        onStart();
      }}
    >
      {label(connected, busy)}
    </button>
  );
}

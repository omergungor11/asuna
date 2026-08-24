/**
 * Durum rozeti (ASU-015).
 *
 * Kullanicinin sisteme guvenmesi bu gorunurluge bagli (PROJECT.md Bolum 19/21):
 * dinliyor mu, bagli mi, konusuyor mu — her an ekranda.
 *
 * Bilesen durum **uretmez**, `VoiceState`'i gosterir (`conventions.md` — Frontend).
 */

import type { VoiceState } from '../asuna/state/voice-state-machine';

/**
 * `Record` bilincli: yeni bir durum eklendiginde etiketi yazmayi unutmak
 * derleme hatasi olur, ekranda bos rozet degil.
 */
const STATE_LABELS: Readonly<Record<VoiceState, string>> = {
  BOOTING: 'Bağlı değil',
  IDLE_WAKE_WORD: 'Bekliyor (wake word)',
  WAKING: 'Uyanıyor',
  CONNECTING: 'Bağlanıyor',
  LISTENING: 'Dinliyor',
  USER_SPEAKING: 'Sen konuşuyorsun',
  ASSISTANT_THINKING: 'Düşünüyor',
  ASSISTANT_SPEAKING: 'Konuşuyor',
  TOOL_PENDING: 'Araç çalışıyor',
  AWAITING_APPROVAL: 'Onay bekliyor',
  ERROR: 'Hata',
};

export interface VoiceStatusBadgeProps {
  readonly state: VoiceState;
}

export function VoiceStatusBadge({ state }: VoiceStatusBadgeProps): React.JSX.Element {
  return (
    <span className="asuna-status" data-state={state} role="status">
      <span className="asuna-status__dot" aria-hidden="true" />
      {STATE_LABELS[state]}
    </span>
  );
}

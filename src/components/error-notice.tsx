/**
 * Hata bildirimi (ASU-015, ASU-019).
 *
 * PROJECT.md Bolum 30: basari taklidi yok. Ne oldugu ve kullanicinin atabilecegi
 * somut adim ayri ayri gosterilir — mikrofon izni reddedildiginde macOS ayar yolu
 * bu `action` metninden gelir (`error-messages.ts`).
 *
 * Metin duz olarak render edilir; `dangerouslySetInnerHTML` yok.
 */

import type { UserFacingError } from '../asuna/observability';

export interface ErrorNoticeProps {
  readonly error: UserFacingError;
}

export function ErrorNotice({ error }: ErrorNoticeProps): React.JSX.Element {
  return (
    <div className="asuna-error" role="alert" data-kind={error.kind}>
      <p className="asuna-error__message">{error.message}</p>
      {error.action !== null && <p className="asuna-error__action">{error.action}</p>}
    </div>
  );
}

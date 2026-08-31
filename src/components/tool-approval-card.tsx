/**
 * Tool onay karti (ASU-053).
 *
 * # Neden var
 *
 * PROJECT.md Bolum 19: kullanici, Asuna'nin bilgisayarda sessizce bir sey
 * yapip yapmadigini **hic** merak etmemeli. Onay gerektiren bir tool cagrisi
 * (ASU-048 matrisi) calismadan once bu kart cikar ve kullanici karari verene
 * kadar SDK `execute`'u cagirmaz — yani kart "bilgi" degil, gercek bir kapi.
 *
 * # Sinirlar
 *
 * - Karar **`requestId` ile** verilir. "Sonuncuyu onayla" yok: kartin gosterdigi
 *   istek ile onaylanan istegin ayni oldugu kanitli olmali.
 * - Geri sayim yalnizca **gosterilir**. Sure dolunca bu bilesen hicbir sey
 *   cagirmaz; otomatik reddetme serviste (ASU-048). UI'in zaman asimini
 *   tetiklemesi, iki tarafin ayri saatlerle ayni karari vermesi demekti.
 * - Arguman ozeti host tarafinda redakte edilmis tek satirlik metindir ve
 *   **duz** basilir; `dangerouslySetInnerHTML` yok (arguman icerigi modelden gelir).
 * - Bilesen saf sunumdur: servis cagirmaz, `invoke` etmez. Props in, event out.
 *
 * # Klavye: onaylayan kisayol YOK
 *
 * Kart acilinca odak **"Reddet"** butonuna gelir ve tek kisayol `Esc` = reddet.
 * Onaylayan bir kisayol bilerek yok: kart konusmanin ortasinda, kullanici baska
 * bir sey icin Enter'a basarken acilabilir ve refleksle onaylanan bir risk 1+
 * aksiyon, onay katmaninin varlik sebebini yok eder. Kaza ile basilan tusun
 * sonucu her zaman "calistirma" yonunde (ASU-048 varsayilani da reddetmek).
 * Onay yalnizca kasitli bir eylemle verilir: butona tiklamak ya da butona
 * Tab'layip Enter'a basmak.
 *
 * # Gorunurluk
 *
 * Ayri bir overlay penceresi henuz yok (`tauri.conf.json` tek pencere: `main`).
 * Bu yuzden kart `voice-panel.tsx` icinden `document.body`'ye portal edilir ve
 * sabit konumda durur: baska bir sekme acikken Konusma paneli `hidden` olsa
 * bile onay istegi ekrandan kaybolmaz.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import type { PendingToolApproval } from '../asuna/agent/use-asuna-session';

import { describeToolRisk, formatApprovalCountdown, riskAttribute } from './tool-text';

/** Geri sayimin tazelenme araligi. Saniyeden sik tazelemek gereksiz render'dir. */
export const APPROVAL_TICK_MS = 1000;

/** Karar sonrasi kisa bildirimin ekranda kalma suresi. */
export const DECISION_NOTICE_MS = 6000;

const NOTICE_APPROVED = 'Onayladın — araç çalıştırılıyor.';
const NOTICE_REJECTED = 'Reddettin — araç çalıştırılmadı.';
const NOTICE_TIMEOUT = 'Onay süresi doldu — istek reddedildi, araç çalıştırılmadı.';
const NOTICE_CLOSED = 'Onay isteği kapandı — araç çalıştırılmadı.';

/** Ekranda duran istegin kimligi + son karar ani. */
interface Tracked {
  readonly requestId: string;
  readonly deadlineMs: number;
}

/** Kullanicinin **bu** istek icin verdigi karar. */
interface Decision {
  readonly requestId: string;
  readonly text: string;
}

interface Notice {
  readonly text: string;
  /** Ayni metin iki kez gosterildiginde TTL sayaci yeniden bassin diye. */
  readonly seq: number;
}

export interface ToolApprovalCardProps {
  /** Bekleyen istek; `null` = kart cizilmez (varsa yalnizca karar bildirimi kalir). */
  readonly approval: PendingToolApproval | null;
  readonly onApprove: (requestId: string) => void;
  readonly onReject: (requestId: string) => void;
  /** Geri sayim saati; testler sahte saat verir. Varsayilan `Date.now`. */
  readonly now?: () => number;
}

export function ToolApprovalCard({
  approval,
  onApprove,
  onReject,
  now = Date.now,
}: ToolApprovalCardProps): React.JSX.Element | null {
  const rejectRef = useRef<HTMLButtonElement | null>(null);

  const [tracked, setTracked] = useState<Tracked | null>(null);
  const [decision, setDecision] = useState<Decision | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [nowMs, setNowMs] = useState<number>(now);

  // Prop degisimine gore state ayarlama **render sirasinda** yapilir (React'in
  // "adjusting state when a prop changes" deseni). Effect icinde yapilsaydi
  // kullanici bir kare boyunca eski karti gorurdu ve zincirleme render olurdu.
  if (approval !== null && tracked?.requestId !== approval.requestId) {
    setTracked({
      requestId: approval.requestId,
      deadlineMs: approval.requestedAtMs + approval.timeoutMs,
    });
    setDecision(null);
    setNotice(null);
    // Yeni istek icin saat de tazelenir: onceki kartin son tikinden kalan
    // bayat bir "simdi" degeri, yeni istegin kalan suresini yanlis gosterirdi.
    setNowMs(now());
  }

  // Istek cozuldu (onay / red / zaman asimi / oturum kapandi). Kart kapanir,
  // yerinde **kisa** bir bildirim kalir. Bu bildirim transcript'e girmez:
  // transcript'teki tool satiri backend'in isi, burasi anlik geri bildirim.
  if (approval === null && tracked !== null) {
    const resolved = decision !== null && decision.requestId === tracked.requestId;
    const text = resolved
      ? decision.text
      : // Karar bizden gelmediyse: ya onay penceresi doldu (servis otomatik
        // reddetti) ya da oturum kapandi. Ikisi de "calismadi" demek — burada
        // iyimser bir cumle kurmak basari taklidi olurdu.
        now() >= tracked.deadlineMs
        ? NOTICE_TIMEOUT
        : NOTICE_CLOSED;

    setTracked(null);
    setDecision(null);
    setNotice({ text, seq: (notice?.seq ?? 0) + 1 });
  }

  const requestId = approval?.requestId ?? null;

  // Geri sayim saati yalnizca acik kartta doner: kapali kartta saniyede bir
  // render etmenin kimseye faydasi yok.
  useEffect(() => {
    if (requestId === null) {
      return;
    }
    const timer = setInterval(() => {
      setNowMs(now());
    }, APPROVAL_TICK_MS);

    return (): void => {
      clearInterval(timer);
    };
  }, [requestId, now]);

  // Kart acilinca odak **"Reddet"** butonuna gelir.
  //
  // Bilincli olarak guvenli yon: kart konusmanin ortasinda, kullanici baska bir
  // sey icin Enter'a basarken acilabilir. Odak "Onayla"da olsaydi (ya da kart
  // seviyesinde bir Enter kisayolu bulunsaydi) risk 1+ bir aksiyon refleksle
  // onaylanirdi. Onay yalnizca **kasitli** bir eylemle verilir: butona tiklamak
  // ya da butona Tab'layip Enter'a basmak.
  useEffect(() => {
    if (requestId !== null) {
      rejectRef.current?.focus();
    }
  }, [requestId]);

  useEffect(() => {
    if (notice === null) {
      return;
    }
    const timer = setTimeout(() => {
      setNotice(null);
    }, DECISION_NOTICE_MS);

    return (): void => {
      clearTimeout(timer);
    };
  }, [notice]);

  const decide = useCallback(
    (approve: boolean): void => {
      if (approval === null || decision?.requestId === approval.requestId) {
        return;
      }
      setDecision({
        requestId: approval.requestId,
        text: approve ? NOTICE_APPROVED : NOTICE_REJECTED,
      });
      if (approve) {
        onApprove(approval.requestId);
      } else {
        onReject(approval.requestId);
      }
    },
    [approval, decision, onApprove, onReject],
  );

  // Kartta **tek** klavye kisayoli var: Esc = reddet. Onaylayan bir kisayol
  // yok — kaza ile basilan bir tusun sonucu her zaman "calistirma" yonunde
  // olmali (ASU-048 varsayilani da reddetmek).
  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLElement>): void => {
      if (event.key === 'Escape') {
        event.preventDefault();
        decide(false);
      }
    },
    [decide],
  );

  if (approval === null) {
    return notice === null ? null : (
      <p className="asuna-tool-approval__notice" role="status">
        {notice.text}
      </p>
    );
  }

  const remainingMs = approval.requestedAtMs + approval.timeoutMs - nowMs;
  const busy = decision?.requestId === approval.requestId;

  return (
    <section
      className="asuna-tool-approval"
      role="dialog"
      aria-label="Araç onayı"
      data-risk={riskAttribute(approval.risk)}
      onKeyDown={handleKeyDown}
    >
      {/* `role="alert"` bir kez duyurulur; kartin tamamina `aria-live` vermek
          geri sayim her tikladiginda ekran okuyucuyu bastan konusturur. */}
      <p className="asuna-tool-approval__lead" role="alert">
        Asuna bir araç çalıştırmak için izin istiyor.
      </p>

      <h3 className="asuna-tool-approval__tool">{approval.toolName}</h3>
      <p className="asuna-tool-approval__description">
        {approval.description === '' ? 'Bu aracın açıklaması yok.' : approval.description}
      </p>

      <dl className="asuna-tool-approval__facts">
        <div className="asuna-tool-approval__fact">
          <dt>Risk</dt>
          <dd>{describeToolRisk(approval.risk)}</dd>
        </div>
        <div className="asuna-tool-approval__fact">
          <dt>Argümanlar</dt>
          <dd>{approval.argumentsPreview ?? 'Argümansız çağrı'}</dd>
        </div>
      </dl>

      <p className="asuna-tool-approval__countdown" aria-live="off">
        Kalan süre: {formatApprovalCountdown(remainingMs)}
      </p>
      <p className="asuna-tool-approval__hint">
        Süre dolarsa istek otomatik reddedilir. Onaylanmayan araç çalışmaz.
      </p>

      <div className="asuna-tool-approval__actions">
        <button
          type="button"
          disabled={busy}
          aria-label={`Onayla: ${approval.toolName}`}
          onClick={(): void => {
            decide(true);
          }}
        >
          Onayla
        </button>
        <button
          type="button"
          ref={rejectRef}
          disabled={busy}
          aria-label={`Reddet: ${approval.toolName}`}
          onClick={(): void => {
            decide(false);
          }}
        >
          Reddet (Esc)
        </button>
      </div>
    </section>
  );
}

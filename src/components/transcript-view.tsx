/**
 * Canli transcript (ASU-017).
 *
 * # Bu bir sohbet gecmisi degil, bir KAYIT
 *
 * Mesaj balonu, avatar, zaman damgasi, metin girisi yok (CLAUDE.md prime directive).
 * Amac kullanicinin "beni dogru mu anladi?" sorusuna anlik cevap verebilmesi.
 *
 * Kurallar:
 * - Metin **duz** render edilir; `dangerouslySetInnerHTML` yok (model/tool ciktisi
 *   HTML olarak yorumlanmaz).
 * - Kismi (partial) satirlar gorunur ama kesinlesmemis olduklari belli edilir.
 * - Kesilen Asuna cevabi kesildigi yerde "— kesildi" ile isaretlenir.
 * - Otomatik en alta kaydirma **yalnizca** kullanici zaten alttayken; yukari
 *   kaydirdiysa okudugu yerden koparilmaz.
 * - Uzun oturumda DOM sismesin diye yalnizca son [`VISIBLE_LINE_COUNT`] satir basilir;
 *   gizlenen satir sayisi durustce yazilir (sessizce kirpilmaz).
 */

import { useEffect, useRef, useState } from 'react';

import type { TranscriptLine } from '../asuna/agent/use-asuna-session';

/** DOM'a basilan azami satir. Bellekteki sinir `MAX_TRANSCRIPT_LINES`. */
export const VISIBLE_LINE_COUNT = 60;

/** Bu mesafeden yakinsa kullanici "altta" sayilir (px). */
const STICK_THRESHOLD_PX = 24;

const ROLE_LABELS: Readonly<Record<TranscriptLine['role'], string>> = {
  user: 'Sen',
  assistant: 'Asuna',
};

export interface TranscriptViewProps {
  readonly lines: readonly TranscriptLine[];
}

export function TranscriptView({ lines }: TranscriptViewProps): React.JSX.Element {
  const listRef = useRef<HTMLDivElement | null>(null);
  const [stickToBottom, setStickToBottom] = useState(true);

  const hiddenCount = Math.max(0, lines.length - VISIBLE_LINE_COUNT);
  const visible = hiddenCount > 0 ? lines.slice(hiddenCount) : lines;

  useEffect(() => {
    if (!stickToBottom) {
      return;
    }
    const node = listRef.current;
    if (node !== null) {
      node.scrollTop = node.scrollHeight;
    }
  }, [lines, stickToBottom]);

  return (
    <div
      className="asuna-transcript"
      ref={listRef}
      role="log"
      aria-label="Konuşma dökümü"
      aria-live="polite"
      onScroll={(event): void => {
        const node = event.currentTarget;
        const distance = node.scrollHeight - node.scrollTop - node.clientHeight;
        setStickToBottom(distance <= STICK_THRESHOLD_PX);
      }}
    >
      {hiddenCount > 0 && (
        <p className="asuna-transcript__hidden">
          Önceki {hiddenCount.toString()} satır gizlendi.
        </p>
      )}

      {visible.length === 0 ? (
        <p className="asuna-transcript__empty">Henüz konuşma yok.</p>
      ) : (
        visible.map((line) => (
          <p
            key={line.itemId}
            className="asuna-transcript__line"
            data-role={line.role}
            data-status={line.status}
          >
            <span className="asuna-transcript__role">{ROLE_LABELS[line.role]}:</span>{' '}
            <span className="asuna-transcript__text">{line.text}</span>
            {line.status === 'in_progress' && (
              <span className="asuna-transcript__partial"> …</span>
            )}
            {line.interrupted && <span className="asuna-transcript__cut"> — kesildi</span>}
          </p>
        ))
      )}
    </div>
  );
}

/**
 * Ses paneli — Phase 1'in tum gorunur yuzeyi (ASU-015).
 *
 * # Bu bir sohbet penceresi DEGILDIR
 *
 * Mesaj balonu, "send" butonu, metin girisi yok (CLAUDE.md prime directive).
 * Ses birincil; bu panelin isi sistemi **gorunur** kilmak: durum, mikrofon, aktif
 * tool, hata ve her an erisilebilir bir durdurma yolu (PROJECT.md Bolum 19/21).
 *
 * Panel tek "container" bilesendir: servis erisimi yalnizca `useAsunaSession`
 * uzerinden olur, alt bilesenler saf sunumdur (props in, event out).
 */

import { useAsunaSession, type UseAsunaSessionOptions } from '../asuna/agent/use-asuna-session';
import { describeSessionOutcome } from '../asuna/memory/session-service';
import {
  useCurrentProject,
  type CurrentProjectPort,
} from '../asuna/projects/use-current-project';

import { ErrorNotice } from './error-notice';
import { MicIndicator } from './mic-indicator';
import { describeCurrentProject } from './project-text';
import { TalkButton } from './talk-button';
import { TranscriptView } from './transcript-view';
import { VoiceStatusBadge } from './voice-status-badge';

export interface VoicePanelProps {
  /** Testlerin sahte servis/mikrofon/config enjekte etmesi icin. */
  readonly options?: UseAsunaSessionOptions;
  /** Guncel proje kaynagi (ASU-045); testler sahte port verir. */
  readonly projectPort?: CurrentProjectPort;
}

export function VoicePanel({ options, projectPort }: VoicePanelProps): React.JSX.Element {
  const session = useAsunaSession(options);
  // "Mevcut proje" panelin guven yuzeyinin parcasi (PROJECT.md Bolum 19):
  // Asuna hangi projede oldugunu sanip yanlis yere bakiyorsa kullanici bunu
  // konusmadan once gormeli.
  const project = useCurrentProject(projectPort);
  const blocked = session.error !== null && !session.error.retryable;

  return (
    <section className="asuna-panel" aria-label="Asuna ses oturumu">
      <div className="asuna-panel__row">
        <VoiceStatusBadge state={session.state} />
        <MicIndicator active={session.micActive} />
      </div>

      <TalkButton
        connected={session.connected}
        busy={session.busy}
        disabled={blocked}
        onStart={session.start}
        onStop={session.stop}
      />

      {session.error !== null && <ErrorNotice error={session.error} />}

      {/* Barge-in gorsel tepkisi (ASU-016): kullanici "duydu mu?" diye tekrarlamasin. */}
      {session.bargeIn && (
        <p className="asuna-panel__bargein">Sözünü kestin — Asuna sustu, seni dinliyor.</p>
      )}

      <TranscriptView lines={session.transcript} />

      <dl className="asuna-panel__facts">
        <div className="asuna-panel__fact">
          <dt>Proje</dt>
          <dd>{describeCurrentProject(project)}</dd>
        </div>
        <div className="asuna-panel__fact">
          <dt>Model</dt>
          <dd>{session.model ?? '—'}</dd>
        </div>
        <div className="asuna-panel__fact">
          <dt>Aktif araç</dt>
          <dd>{session.activeTool ?? '—'}</dd>
        </div>
        <div className="asuna-panel__fact">
          <dt>Gecikme</dt>
          <dd>
            {session.lastLatencyMs === null ? '—' : `${session.lastLatencyMs.toString()} ms`}
          </dd>
        </div>
        {/* Kapanan oturumun suresi ve token kullanimi (ASU-032 / R1 takibi). */}
        <div className="asuna-panel__fact">
          <dt>Oturum</dt>
          <dd>{describeSessionOutcome(session.sessionOutcome)}</dd>
        </div>
      </dl>
    </section>
  );
}

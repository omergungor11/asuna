/**
 * Guncel projenin detayi (ASU-045) — ozet + git durumu + son oturum ozeti.
 *
 * # Neden "guncel proje"nin detayi
 *
 * Detay `project_context` komutundan gelir (ASU-044) ve o komut argument
 * **almaz**: baglam tanim geregi guncel projeye aittir. Renderer'in "su projeyi
 * anlat" diyebilmesi, ekrandaki bir tiklamanin dosya okutmasi demek olurdu.
 * Bu yuzden detay bir satir secimiyle degil, kullanicinin acik "güncel proje
 * yap" eylemiyle degisir — ve ekranda gorunen sey Asuna'nin sesli soyleyecegi
 * seyle ayni kaynaktan besleniyor olur.
 *
 * Saf sunum: servis cagirmaz, kendi durumu yok. Butun metinler duz render
 * edilir (`dangerouslySetInnerHTML` yok) — icerik proje dosyalarindan ve model
 * ciktisindan geliyor olabilir.
 */

import type { ProjectContextResult } from '../asuna/projects/project-context';
import type { SessionListItem } from '../shared/session';

import {
  LAST_SESSION_SCOPE_NOTE,
  describeGitStatus,
  describeLastSession,
} from './project-text';

export interface ProjectDetailProps {
  /** Guncel proje yoksa `null` — o zaman komut hic cagrilmaz. */
  readonly result: ProjectContextResult | null;
  readonly loading: boolean;
  readonly lastSession: SessionListItem | null;
  /** Oturum ozeti okunamadiysa nedeni; gizlenmez. */
  readonly lastSessionError: string | null;
}

export function ProjectDetail({
  result,
  loading,
  lastSession,
  lastSessionError,
}: ProjectDetailProps): React.JSX.Element {
  return (
    <section className="asuna-project-detail" aria-label="Güncel proje detayı">
      <h3 className="asuna-project-detail__title">Güncel proje detayı</h3>

      {loading && <p className="asuna-project-detail__notice">Detay yükleniyor…</p>}

      {!loading && result === null && (
        <p className="asuna-project-detail__notice">
          Güncel proje seçilmedi. Bir projeyi “Güncel proje yap” ile seçin; Asuna “şu an hangi
          projedeyim?” sorusuna ancak o zaman cevap verebilir.
        </p>
      )}

      {!loading && result !== null && result.status === 'unknown' && (
        <p className="asuna-project-detail__notice">{result.message}</p>
      )}

      {!loading && result !== null && result.status === 'unavailable' && (
        // Sessiz bos ekran yok: detay yuklenemediyse nedeni yazar, liste calisir.
        <p className="asuna-project-detail__notice" role="alert">
          Detay yüklenemedi: {result.message}
        </p>
      )}

      {!loading && result !== null && result.status === 'known' && (
        <>
          <dl className="asuna-project-detail__facts">
            <div className="asuna-project-detail__fact">
              <dt>Git</dt>
              <dd>{describeGitStatus(result.detail.git)}</dd>
            </div>
            <div className="asuna-project-detail__fact">
              <dt>Son oturum özeti</dt>
              <dd>{lastSessionError ?? describeLastSession(lastSession)}</dd>
            </div>
          </dl>

          <p className="asuna-project-detail__note">{LAST_SESSION_SCOPE_NOTE}</p>

          {result.detail.sources.length === 0 ? (
            <p className="asuna-project-detail__notice">
              Bu projede okunabilir bir özet kaynağı (README/PROJECT.md/manifest) bulunamadı.
            </p>
          ) : (
            <ul className="asuna-project-detail__sources" aria-label="Proje özeti kaynakları">
              {result.detail.sources.map((source) => (
                <li key={source.name} className="asuna-project-detail__source">
                  <h4 className="asuna-project-detail__source-name">
                    {source.name}
                    {source.truncated && (
                      <span className="asuna-project-detail__badge">kırpıldı</span>
                    )}
                  </h4>
                  <p className="asuna-project-detail__excerpt">{source.excerpt}</p>
                </li>
              ))}
            </ul>
          )}

          {/* `.asuna/context.json` — DB ile celisirse DB kazanir (ASU-043).
              Bu yuzden ayri bir blok: proje ozetiyle ayni sey gibi durmasin. */}
          {result.detail.handoff.ignoredMessage !== null && (
            <p className="asuna-project-detail__notice" role="alert">
              {result.detail.handoff.ignoredMessage}
            </p>
          )}

          {result.detail.handoff.activeTask !== null && (
            <p className="asuna-project-detail__handoff">
              Aktif iş: {result.detail.handoff.activeTask}
            </p>
          )}

          {result.detail.handoff.objective !== null && (
            <p className="asuna-project-detail__handoff">
              Hedef: {result.detail.handoff.objective}
            </p>
          )}

          {result.detail.truncated && (
            <p className="asuna-project-detail__note">
              Bağlam tavana takıldı; en az bir liste kısaltıldı.
            </p>
          )}
        </>
      )}
    </section>
  );
}

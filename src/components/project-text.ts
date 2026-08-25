/**
 * Projeler sekmesinin metin katmani (ASU-045).
 *
 * `memory-text.ts` ile ayni gerekce: etiketler ve durum cumleleri tek yerde
 * durur ve saf fonksiyon olduklari icin bilesen render etmeden test edilir.
 *
 * Kural: hicbir sey guzellestirilmez. Yolu kaybolmus proje "kayıtlı" gibi
 * gorunmez, kaldirilamayan kayit "kaldırıldı" demez (PROJECT.md Bolum 30).
 */

import type { ProjectGitView } from '../asuna/projects/project-context';
import type { CurrentProjectState } from '../asuna/projects/use-current-project';
import {
  AsunaRegistryError,
  type ProjectAddOutcome,
  type ProjectRecord,
  type ProjectRemoveOutcome,
  type ProjectStatus,
} from '../shared/project';
import type { SessionListItem } from '../shared/session';

import { formatMemoryTimestamp } from './memory-text';

/**
 * Durum rozetleri.
 *
 * `Record<ProjectStatus, string>`: semaya yeni bir durum eklenirse
 * (`src/shared/project.ts` aynasi) burasi derleme hatasi verir — rozet sessizce
 * eksik kalmaz.
 */
export const PROJECT_STATUS_LABELS: Readonly<Record<ProjectStatus, string>> = {
  active: 'kayıtlı',
  missing: 'yolu bulunamıyor',
  archived: 'arşiv',
  unlinked: 'yalnızca hafıza etiketi',
};

/**
 * Durumun ne anlama geldigi — rozet tek basina yeterli degil.
 *
 * `missing` icin ozellikle onemli: kayit **silinmedi**, kullanici harici diski
 * takmayi unutmus olabilir (ASU-040).
 */
export const PROJECT_STATUS_HINTS: Readonly<Record<ProjectStatus, string | null>> = {
  active: null,
  missing: 'Kayıtlı kök şu an diskte yok. Kayıt silinmedi; disk takılınca geri döner.',
  archived: null,
  unlinked:
    'Bu projenin kayıtlı kökü yok; yalnızca hafızada geçen bir etiket. Hiçbir dosya ' +
    'sistemi yetkisi taşımaz.',
};

/** Dil/cati satiri. Tespit edilememisse bu **gizlenmez**. */
export function describeProjectStack(project: ProjectRecord): string {
  const parts = [project.primaryLanguage, project.framework].filter(
    (part): part is string => part !== null,
  );
  return parts.length === 0 ? 'dil/çatı bilinmiyor' : parts.join(' · ');
}

/** Kayitli kok. `unlinked` bir etiketin yolu yoktur ve bu acikca yazilir. */
export function describeProjectPath(project: ProjectRecord): string {
  return project.path ?? 'kayıtlı kök yok';
}

/** Son acilma. `null` tahmin edilmez: "hiç açılmadı" yazar. */
export function describeLastOpened(lastOpenedAt: string | null): string {
  return lastOpenedAt === null ? 'hiç açılmadı' : formatMemoryTimestamp(lastOpenedAt);
}

/**
 * Servis hatasini kullaniciya gosterilecek cumleye cevirir.
 *
 * Orijinal mesaj her zaman korunur — kod yalnizca baglam ekler.
 */
export function describeRegistryError(error: unknown): string {
  if (error instanceof AsunaRegistryError) {
    switch (error.code) {
      case 'invalid':
        return `İstek reddedildi: ${error.message}`;
      case 'path-refused':
        return `Bu yol kabul edilmedi: ${error.message}`;
      case 'path-not-found':
        return `Yol bulunamadı: ${error.message}`;
      case 'not-a-directory':
        return `Seçilen yol bir dizin değil: ${error.message}`;
      case 'not-found':
        return `Proje kaydı bulunamadı — liste güncel olmayabilir. (${error.message})`;
      case 'refused':
        return `Bu işlem bu proje için geçerli değil: ${error.message}`;
      case 'disabled':
        return `Hafıza kapalı olduğu için proje kaydı tutulamıyor. (${error.message})`;
      case 'unavailable':
        return `Proje kaydı kullanılamıyor: ${error.message}`;
      case 'storage':
        return `Depolama hatası: ${error.message}`;
      case 'unknown':
        return error.message;
    }
  }

  if (error instanceof Error && error.message.length > 0) {
    return error.message;
  }

  return 'Proje işlemi bilinmeyen bir nedenle başarısız oldu.';
}

/** Ekleme sonucu. Cift kayit bir hata degil, ama "eklendi" de denmez. */
export function describeAddOutcome(outcome: ProjectAddOutcome): string {
  return outcome.status === 'registered'
    ? `Proje eklendi: ${outcome.project.name}`
    : `Bu dizin zaten kayıtlı: ${outcome.project.name}`;
}

/**
 * Kaldirma sonucu.
 *
 * `unlinked` durumunda kullaniciya **dogrusu** soylenir: kayitli kok kaldirildi
 * ama satir silinmedi, cunku bu etikete bagli hafiza var. "Sildim" demek yalan
 * olurdu (ASU-040).
 */
export function describeRemoveOutcome(outcome: ProjectRemoveOutcome): string {
  if (outcome.status === 'deleted') {
    return 'Proje kaydı kaldırıldı.';
  }
  return (
    `Kayıt kaldırıldı, hafıza etiketi korundu: “${outcome.project.name}” adına bağlı ` +
    `${outcome.references.toString()} kayıt olduğu için proje hafızada etiket olarak duruyor. ` +
    'Hafıza silinmedi.'
  );
}

/**
 * Git durumu tek satirda.
 *
 * "Depo degil" bir hata degil, bir gercek — ve gizlenmez. `degraded` de
 * gizlenmez: eksik bilgi tam bilgi gibi sunulmaz (PROJECT.md Bolum 30).
 */
export function describeGitStatus(git: ProjectGitView): string {
  if (!git.isRepository) {
    return 'git deposu değil';
  }

  const head = git.detached ? 'dal yok (detached HEAD)' : (git.branch ?? 'dal adı okunamadı');
  const worktree = git.dirty
    ? `${git.changedTrackedFiles.toString()} izlenen dosya değişmiş`
    : 'çalışma ağacı temiz';
  const degraded = git.degraded ? ' · git bilgisi eksik okundu' : '';

  return `${head} · ${worktree}${degraded}`;
}

/**
 * Son oturum ozeti.
 *
 * **Kapsam uyarisi**: oturum listesi (ASU-065 sozlesmesi) proje kimligi
 * tasimaz, bu yuzden burada gorunen sey "bu projenin son oturumu" degil, en son
 * konusmanin ozetidir. Bunu "proje ozeti" gibi sunmak yanlis olurdu; etiketi ve
 * altindaki not bunu acikca soyler.
 */
export const LAST_SESSION_SCOPE_NOTE =
  'En son konuşmanın özeti — oturum kayıtları henüz projeye göre filtrelenemiyor.';

export function describeLastSession(session: SessionListItem | null): string {
  if (session === null) {
    return 'kayıtlı oturum yok';
  }
  if (session.summaryPreview === null) {
    return 'son oturumun özeti yok';
  }
  return session.summaryTruncated ? `${session.summaryPreview}…` : session.summaryPreview;
}

/** Ses panelindeki "mevcut proje" satiri (PROJECT.md Bolum 19). */
export function describeCurrentProject(state: CurrentProjectState): string {
  switch (state.phase) {
    case 'loading':
      return 'okunuyor…';
    case 'error':
      // "Proje yok" ile "proje okunamadi" ayni gorunmemeli.
      return `okunamadı: ${state.message}`;
    case 'known':
      if (state.project === null) {
        return 'seçilmedi';
      }
      return state.project.status === 'missing'
        ? `${state.project.name} (yolu bulunamıyor)`
        : state.project.name;
  }
}

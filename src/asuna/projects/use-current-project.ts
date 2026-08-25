/**
 * "Su an hangi projedeyiz?" — guncel projeyi okuyan hook (ASU-045).
 *
 * Ses paneli bunu overlay'de gostermek zorunda (PROJECT.md Bolum 19). Panel
 * canli oturumu tasidigi icin hicbir zaman unmount edilmez; bu yuzden secimi
 * `project-events` sinyali ile ogrenir ve **yeniden okur**. Ekranda gosterilen
 * deger her zaman servisin dondugu deger; UI'da tutulan bir kopya degil.
 *
 * Guncel proje ayri bir bayrak degil, `currentProjectOf` ile listeden turetilir
 * (ASU-040 sozlesmesi). Secim yoksa `null` doner ve arayuz "proje seçilmedi"
 * yazar — uydurmaz.
 */

import { useEffect, useState } from 'react';

import { toRegistryError, type ProjectRecord } from '../../shared/project';

import { subscribeProjectsChanged } from './project-events';
import { currentProjectOf, listProjects } from './project-registry';

/** Hook'un servis yuzeyi; testler sahte port verir. */
export interface CurrentProjectPort {
  readonly list: () => Promise<readonly ProjectRecord[]>;
}

const DEFAULT_CURRENT_PROJECT_PORT: CurrentProjectPort = { list: listProjects };

export type CurrentProjectState =
  | { readonly phase: 'loading' }
  | { readonly phase: 'known'; readonly project: ProjectRecord | null }
  | { readonly phase: 'error'; readonly message: string };

export function useCurrentProject(
  port: CurrentProjectPort = DEFAULT_CURRENT_PROJECT_PORT,
): CurrentProjectState {
  const [state, setState] = useState<CurrentProjectState>({ phase: 'loading' });
  const [token, setToken] = useState(0);

  // Baska bir sekmede yapilan secim de burayi tazeler.
  useEffect(
    () =>
      subscribeProjectsChanged(() => {
        setToken((value) => value + 1);
      }),
    [],
  );

  useEffect(() => {
    let cancelled = false;

    port.list().then(
      (projects) => {
        if (!cancelled) {
          setState({ phase: 'known', project: currentProjectOf(projects) });
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          // Hata yutulmaz: "proje yok" ile "proje okunamadi" ayni sey degil.
          setState({ phase: 'error', message: toRegistryError(error).message });
        }
      },
    );

    return (): void => {
      cancelled = true;
    };
  }, [port, token]);

  return state;
}

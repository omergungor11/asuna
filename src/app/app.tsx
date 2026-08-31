import { Suspense, lazy, useMemo, useState } from 'react';

import type { UseAsunaSessionOptions } from '../asuna/agent/use-asuna-session';
import { asunaToolRegistry, ToolToggleStore } from '../asuna/tools';
import { MemoryView } from '../components/memory-view';
import { ProjectsView } from '../components/projects-view';
import { SettingsView } from '../components/settings-view';
import { ToolsView } from '../components/tools-view';
import { VoicePanel } from '../components/voice-panel';

/**
 * Debug log paneli (ASU-019) yalnizca gelistirmede yuklenir.
 *
 * `import.meta.env.DEV` uretim build'inde `false` sabitine indirgenir; ternary
 * olu koda duser ve dinamik `import()` bundle'a hic girmez.
 */
const DebugPanel = import.meta.env.DEV
  ? lazy(async () => {
      const module = await import('../components/debug-panel');
      return { default: module.DebugPanel };
    })
  : null;

const TABS = [
  { id: 'conversation', label: 'Konuşma' },
  { id: 'projects', label: 'Projeler' },
  { id: 'memory', label: 'Hafıza' },
  { id: 'tools', label: 'Araçlar' },
  { id: 'settings', label: 'Ayarlar' },
] as const;

type TabId = (typeof TABS)[number]['id'];

/**
 * Uygulama kabugu.
 *
 * Kabuk bilerek ince: burada servis **cagrisi** yok, yalnizca hangi panelin
 * gorunur oldugu ve paylasilan tool anahtar seti tutulur (kompozisyon koku —
 * ayni store hem ses oturumuna hem Araclar sekmesine gider).
 *
 * # Sekme kurali: ses paneli ASLA unmount edilmez
 *
 * `VoicePanel` canli bir Realtime oturumu tasir. Hafiza sekmesine gecince
 * unmount edilseydi oturum kopar, kullanici konusurken Asuna susardi. Bu yuzden
 * panel her zaman monte kalir, yalnizca `hidden` ile gizlenir. `MemoryView`
 * tersine yalnizca acikken monte olur — kapali sekme IPC sorgusu atmasin.
 */
export function App(): React.JSX.Element {
  const [tab, setTab] = useState<TabId>('conversation');

  /**
   * Tool tanimlari + anahtarlari (ASU-054) — kabuk **kompozisyon kokudur**.
   *
   * Ikisi de burada bir kez kurulur ve **ayni ornek** iki yere verilir: ses
   * oturumuna (modele giden liste ve `executeTool` kapisi bunlari okur) ve
   * Araclar sekmesine.
   *
   * - Ayri **store** olsaydi ekranda "Kapalı" gorunen bir tool calismaya devam
   *   ederdi.
   * - Ayri **liste** olsaydi (sekme registry'yi kendi okusaydi) oturuma
   *   daraltilmis bir liste verildigi anda sekme, modele acik olmayan
   *   tool'lari "Açık" gosterirdi.
   *
   * Ikisi de `useState` ile dondurulur: her render'da yeni bir store,
   * kullanicinin kapattigi tool'u sessizce geri acardi.
   */
  const [toolDefinitions] = useState(() => asunaToolRegistry.list());
  const [toolToggles] = useState(() => new ToolToggleStore());
  const sessionOptions = useMemo<UseAsunaSessionOptions>(
    () => ({ toolToggles, tools: toolDefinitions }),
    [toolToggles, toolDefinitions],
  );

  return (
    <main className="asuna-shell">
      <h1 className="asuna-shell__title">Asuna</h1>

      <div className="asuna-tabs" role="tablist" aria-label="Asuna panelleri">
        {TABS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="tab"
            id={`asuna-tab-${entry.id}`}
            className="asuna-tabs__tab"
            aria-selected={tab === entry.id}
            aria-controls={`asuna-panel-${entry.id}`}
            onClick={(): void => {
              setTab(entry.id);
            }}
          >
            {entry.label}
          </button>
        ))}
      </div>

      <div
        id="asuna-panel-conversation"
        role="tabpanel"
        aria-labelledby="asuna-tab-conversation"
        className="asuna-shell__panel"
        hidden={tab !== 'conversation'}
      >
        <VoicePanel options={sessionOptions} />
      </div>

      {/* Projeler de yalnizca acikken monte olur: kapali sekme proje listesi
          sormaz. Guncel proje secimi ses panelinde ayrica gorunur, o yuzden bu
          panelin acik kalmasi gerekmez (ASU-045). */}
      {tab === 'projects' && (
        <div
          id="asuna-panel-projects"
          role="tabpanel"
          aria-labelledby="asuna-tab-projects"
          className="asuna-shell__panel"
        >
          <ProjectsView />
        </div>
      )}

      {tab === 'memory' && (
        <div
          id="asuna-panel-memory"
          role="tabpanel"
          aria-labelledby="asuna-tab-memory"
          className="asuna-shell__panel"
        >
          <MemoryView />
        </div>
      )}

      {/* Araclar sekmesi de yalnizca acikken monte olur: kapali sekme denetim
          defterini sorgulamaz (ASU-054). Onay karti bu sekmeden bagimsizdir —
          ses panelinden `document.body`'ye portal edilir, yani buradayken de
          gorunur (ASU-053). */}
      {tab === 'tools' && (
        <div
          id="asuna-panel-tools"
          role="tabpanel"
          aria-labelledby="asuna-tab-tools"
          className="asuna-shell__panel"
        >
          <ToolsView definitions={toolDefinitions} toggles={toolToggles} />
        </div>
      )}

      {/* Ayarlar da yalnizca acikken monte olur: kapali sekme gizlilik
          ayarlarini sormaz, ekranda bayat bir "acik/kapali" durmaz. */}
      {tab === 'settings' && (
        <div
          id="asuna-panel-settings"
          role="tabpanel"
          aria-labelledby="asuna-tab-settings"
          className="asuna-shell__panel"
        >
          <SettingsView />
        </div>
      )}

      {DebugPanel !== null && (
        <Suspense fallback={null}>
          <DebugPanel />
        </Suspense>
      )}
    </main>
  );
}

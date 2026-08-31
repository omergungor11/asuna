/**
 * `ToolsView` testleri (ASU-054).
 *
 * Kanitlanan seyler:
 * 1. Etkin tool listesi risk + onay politikasi ile gorunur.
 * 2. Tool tek tek kapatilabilir; karar **paylasilan** anahtar setine yazilir.
 * 3. Denetim defteri listelenir: **reddedilen** cagri da gorunur.
 * 4. Oturum filtresi sunucuya `sessionId` olarak gider (istemcide kirpilmaz).
 * 5. Defter salt okunur: silme/duzenleme yolu yok.
 * 6. Defter okunamazsa "kayit yok" denmez.
 */

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi, type Mock } from 'vitest';

import type * as ConfigServiceModule from '../asuna/config/config.service';
import type { FrontendConfig } from '../asuna/config/frontend-config';
import { ToolToggleStore } from '../asuna/tools';
import { NO_TOOL_ARGUMENTS, type AsunaToolDefinition } from '../asuna/tools/types';
import { AsunaStoreError } from '../shared/store-error';
import type { ToolEventListQuery, ToolEventPage, ToolEventRecord } from '../shared/tool-event';

import { TOOL_EVENT_PAGE_SIZE, ToolsView, type ToolsViewPort } from './tools-view';

/**
 * Onay politikasi sutunu `ASUNA_TOOL_APPROVAL_MODE`'a bagli; ekran bu degeri
 * config servisinden okur. Testte gercek IPC'ye gidilmez.
 */
const CONFIG: FrontendConfig = {
  realtimeModel: 'gpt-realtime-2.1-mini',
  realtimeVoice: null,
  wakeWord: 'Hey Asuna',
  wakeWordProvider: 'sherpa-kws',
  idleTimeoutSeconds: 45,
  logLevel: 'info',
  memoryEnabled: true,
  transcriptStorage: true,
  toolApprovalMode: 'safe',
  turnDetection: 'semantic_vad',
  vadEagerness: 'high',
  vadSilenceMs: 400,
};

vi.mock('../asuna/config/config.service', async (importOriginal) => ({
  ...(await importOriginal<typeof ConfigServiceModule>()),
  loadFrontendConfig: (): Promise<FrontendConfig> => Promise.resolve(CONFIG),
}));

/**
 * Tanim listesi ekrana **prop** olarak gelir (oturumla ayni kaynak); ekran
 * registry'yi kendi basina okumaz. Ozetler (risk + onay politikasi) gercek
 * `buildToolSummaries` ile bu tanimlardan turer.
 */
function definition(
  overrides: Partial<AsunaToolDefinition> & { readonly name: string },
): AsunaToolDefinition {
  return {
    description: 'Test aracı.',
    risk: 0,
    requiresApproval: false,
    timeoutMs: 5_000,
    parameters: NO_TOOL_ARGUMENTS,
    execute: () => Promise.resolve({ ok: true as const, summary: 'oldu' }),
    ...overrides,
  };
}

const DEFINITIONS: readonly AsunaToolDefinition[] = [
  definition({
    name: 'get_current_project',
    description: 'Kullanıcının şu an hangi projede olduğunu söyler.',
    risk: 0,
  }),
  definition({
    name: 'open_project',
    description: 'Kayıtlı bir projeyi yapılandırılmış editörde açar.',
    risk: 1,
    requiresApproval: true,
  }),
];

function event(overrides: Partial<ToolEventRecord> = {}): ToolEventRecord {
  return {
    id: 1,
    sessionId: 7,
    toolName: 'get_current_project',
    riskLevel: 0,
    argumentsRedacted: null,
    approvalState: 'not_required',
    resultSummary: 'Asuna projesi bildirildi.',
    outcome: 'succeeded',
    createdAt: '2026-08-25T09:30:00Z',
    ...overrides,
  };
}

const DENIED = event({
  id: 2,
  sessionId: 8,
  toolName: 'open_project',
  riskLevel: 1,
  argumentsRedacted: 'projectId=asuna',
  approvalState: 'denied',
  resultSummary: 'Kullanıcı reddetti; proje açılmadı.',
  outcome: 'not_run',
  createdAt: '2026-08-25T10:15:00Z',
});

function page(events: readonly ToolEventRecord[], total = events.length): ToolEventPage {
  return { events, limit: TOOL_EVENT_PAGE_SIZE, limitMax: 200, total };
}

interface Harness {
  readonly port: ToolsViewPort;
  readonly listEvents: Mock<(query: ToolEventListQuery) => Promise<ToolEventPage>>;
  readonly toggles: ToolToggleStore;
}

function createHarness(result: ToolEventPage = page([event(), DENIED])): Harness {
  const listEvents = vi.fn<(query: ToolEventListQuery) => Promise<ToolEventPage>>(() =>
    Promise.resolve(result),
  );

  return { port: { listEvents }, listEvents, toggles: new ToolToggleStore() };
}

/** Ekran her zaman tanim listesi + paylasilan anahtar seti ile kurulur. */
function renderView(
  harness: Harness,
  definitions: readonly AsunaToolDefinition[] = DEFINITIONS,
): ReturnType<typeof render> {
  return render(
    <ToolsView definitions={definitions} toggles={harness.toggles} port={harness.port} />,
  );
}

describe('ToolsView', () => {
  it('etkin tool listesini risk ve onay politikasiyla gosterir', async () => {
    const harness = createHarness();
    renderView(harness);

    await screen.findByText('Kullanıcının şu an hangi projede olduğunu söyler.');

    // Liste bolumune kapsamlanir: ayni etiketler asagidaki defterde de gecer.
    const list = within(document.querySelector<HTMLElement>('.asuna-tools__list')!);
    expect(list.getByText('get_current_project')).toBeInTheDocument();
    expect(list.getByText('Risk 0 · salt okuma')).toBeInTheDocument();
    expect(list.getByText('Onaysız çalışır')).toBeInTheDocument();
    expect(list.getByText('Risk 1 · geri alınabilir')).toBeInTheDocument();
    expect(list.getByText('Her seferinde onay')).toBeInTheDocument();
  });

  /**
   * Kapatma **paylasilan** store'a yazilir: `executeTool` kapisi ve modele
   * giden liste ayni ornegi okur. Ayri bir ornek olsaydi ekran "Kapalı"
   * gorunurken tool calismaya devam ederdi.
   */
  it('tool"u tek tek kapatir ve karari paylasilan anahtar setine yazar', async () => {
    const harness = createHarness();
    renderView(harness);

    const toggle = await screen.findByRole('switch', { name: 'get_current_project aracı' });
    expect(toggle).toBeChecked();

    fireEvent.click(toggle);

    expect(harness.toggles.isEnabled('get_current_project')).toBe(false);
    expect(screen.getByRole('switch', { name: 'get_current_project aracı' })).not.toBeChecked();
    expect(screen.getByText('Kapalı — model bu aracı görmez')).toBeInTheDocument();
  });

  /**
   * Liste **prop**'tan gelir: oturuma daraltilmis bir tanim listesi verilirse
   * sekme de yalnizca onu gosterir. Registry'yi kendi okusaydi, modele acik
   * olmayan bir tool burada "Açık" gorunurdu.
   */
  it('yalnizca verilen tanim listesini gosterir', async () => {
    const harness = createHarness();
    renderView(harness, [DEFINITIONS[0]!]);

    await screen.findByText('Kullanıcının şu an hangi projede olduğunu söyler.');

    const list = within(document.querySelector<HTMLElement>('.asuna-tools__list')!);
    expect(list.getByText('get_current_project')).toBeInTheDocument();
    expect(list.queryByText('open_project')).toBeNull();
  });

  it('tanim listesi bossa durustce "acik arac yok" der', async () => {
    const harness = createHarness();
    renderView(harness, []);

    expect(await screen.findByText('Modele açık bir araç yok.')).toBeInTheDocument();
    // Defter yine de dolu: gorunurluk tool listesine bagli degil.
    expect(screen.getByText('Kullanıcı reddetti; proje açılmadı.')).toBeInTheDocument();
  });

  it('denetim defterini listeler; reddedilen cagri da gorunur', async () => {
    const harness = createHarness();
    renderView(harness);

    expect(await screen.findByText('Kullanıcı reddetti; proje açılmadı.')).toBeInTheDocument();
    expect(screen.getByText('Reddedildi')).toBeInTheDocument();
    expect(screen.getByText('Onay gerekmedi')).toBeInTheDocument();
    expect(screen.getByText('projectId=asuna')).toBeInTheDocument();
    expect(screen.getByText('2026-08-25 13:15')).toBeInTheDocument();
    expect(screen.getByText('2 / 2 kayıt gösteriliyor.')).toBeInTheDocument();
  });

  it('sonuc alanini gosterir; bilinmiyorsa basari uydurmaz', async () => {
    const harness = createHarness(page([DENIED, event({ id: 3, outcome: null })]));
    renderView(harness);

    // Migration 005 oncesi yazilmis satirda `outcome` yok: etiket de yok.
    expect(await screen.findByText('çalışmadı')).toBeInTheDocument();
    expect(screen.queryByText('başarılı')).toBeNull();
  });

  it('oturum filtresini sunucuya gonderir', async () => {
    const harness = createHarness();
    renderView(harness);

    await screen.findByText('Kullanıcı reddetti; proje açılmadı.');
    expect(harness.listEvents).toHaveBeenCalledWith({ limit: TOOL_EVENT_PAGE_SIZE });

    fireEvent.change(screen.getByLabelText('Oturum'), { target: { value: '8' } });

    await waitFor(() => {
      expect(harness.listEvents).toHaveBeenCalledWith({
        limit: TOOL_EVENT_PAGE_SIZE,
        sessionId: 8,
      });
    });
  });

  it('defter salt okunur: silme ya da duzenleme yolu yok', async () => {
    const harness = createHarness();
    renderView(harness);

    await screen.findByText('Kullanıcı reddetti; proje açılmadı.');

    const labels = screen.queryAllByRole('button').map((button) => button.textContent);
    expect(labels.some((label) => /sil|düzenle|temizle/i.test(label))).toBe(false);
    expect(
      screen.getByText(
        'Denetim defteri salt okunurdur: kayıtlar uygulamadan silinemez, düzenlenemez.',
      ),
    ).toBeInTheDocument();
  });

  it('gizli calistirma yolu olmadigini yazar', async () => {
    const harness = createHarness();
    renderView(harness);

    expect(
      await screen.findByText(
        'Gizli araç çalıştırma yolu yok — Asuna’nın yaptığı her çağrı burada görünür.',
      ),
    ).toBeInTheDocument();
  });

  it('defter okunamazsa "kayit yok" demez', async () => {
    const harness = createHarness();
    harness.listEvents.mockRejectedValueOnce(
      new AsunaStoreError('unavailable', 'veritabani kapali'),
    );
    renderView(harness);

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Araç geçmişi kullanılamıyor: veritabani kapali',
    );
    expect(screen.queryByText('Bu filtreye uyan araç çağrısı yok.')).toBeNull();
  });

  it('tavana carpan listede daha fazlasini ister', async () => {
    const harness = createHarness(page([event()], 40));
    renderView(harness);

    fireEvent.click(await screen.findByRole('button', { name: 'Daha fazla göster' }));

    await waitFor(() => {
      expect(harness.listEvents).toHaveBeenCalledWith({ limit: TOOL_EVENT_PAGE_SIZE * 2 });
    });
  });

  it('tool ciktisi duz metin olarak basilir', async () => {
    const harness = createHarness(page([event({ resultSummary: '<b>enjekte</b>' })]));
    const { container } = renderView(harness);

    expect(await screen.findByText('<b>enjekte</b>')).toBeInTheDocument();
    expect(container.querySelector('b')).toBeNull();
  });
});

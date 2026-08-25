/**
 * `PendingApprovals` testleri (ASU-037).
 *
 * Kanitlanan seyler:
 * 1. Yalnizca `metadata_json.pendingApproval === true` kayitlar listelenir.
 * 2. Onaylamak bayragi `false` yapar; diger metadata alanlari korunur.
 * 3. Reddetmek kaydi **gercekten** siler.
 * 4. Kuyruk bosken bolum hic cizilmez.
 * 5. Hafiza kapaliyken "onayladim" denmez.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, type Mock } from 'vitest';

import type {
  MemoryFilter,
  MemoryPatch,
  MemoryRecord,
  MemoryWriteResult,
} from '../shared/memory';

import { PENDING_SCAN_LIMIT, PendingApprovals } from './pending-approvals';

function record(overrides: Partial<MemoryRecord> = {}): MemoryRecord {
  return {
    id: 1,
    kind: 'profile',
    title: 'Omer sabahlari calisiyor',
    content: 'Kullanici gunun ilk saatlerinde odaklaniyor.',
    summary: null,
    projectId: null,
    importance: 0.7,
    confidence: 0.8,
    sourceSessionId: 7,
    createdAt: '2026-08-20T09:30:00Z',
    updatedAt: '2026-08-20T09:30:00Z',
    lastAccessedAt: null,
    expiresAt: null,
    isArchived: false,
    metadataJson: '{"pendingApproval":true,"extraction":{"promptVersion":"v1"}}',
    ...overrides,
  };
}

const PENDING = record();
const NORMAL = record({
  id: 2,
  kind: 'decision',
  title: 'Wake word yerel kalir',
  metadataJson: '{"pendingApproval":false}',
});

interface Harness {
  readonly list: Mock<(filter: MemoryFilter) => Promise<readonly MemoryRecord[]>>;
  readonly update: Mock<(id: number, patch: MemoryPatch) => Promise<MemoryWriteResult>>;
  readonly remove: Mock<(id: number) => Promise<MemoryWriteResult>>;
  readonly onChanged: Mock<() => void>;
}

function createHarness(rows: readonly MemoryRecord[]): Harness {
  const store = [...rows];

  return {
    list: vi.fn(() => Promise.resolve([...store])),
    update: vi.fn((id: number, patch: MemoryPatch): Promise<MemoryWriteResult> => {
      const index = store.findIndex((row) => row.id === id);
      const updated = {
        ...store[index]!,
        ...(patch.metadataJson === undefined ? {} : { metadataJson: patch.metadataJson }),
      };
      store[index] = updated;
      return Promise.resolve({ status: 'stored', record: updated });
    }),
    remove: vi.fn((id: number): Promise<MemoryWriteResult> => {
      store.splice(
        store.findIndex((row) => row.id === id),
        1,
      );
      return Promise.resolve({ status: 'deleted', id });
    }),
    onChanged: vi.fn(),
  };
}

const renderSection = (harness: Harness): ReturnType<typeof render> =>
  render(
    <PendingApprovals
      list={harness.list}
      update={harness.update}
      remove={harness.remove}
      onChanged={harness.onChanged}
    />,
  );

describe('PendingApprovals', () => {
  it('yalnizca onay bekleyen kayitlari listeler', async () => {
    const harness = createHarness([PENDING, NORMAL]);
    renderSection(harness);

    expect(await screen.findByText('Omer sabahlari calisiyor')).toBeInTheDocument();
    expect(screen.queryByText('Wake word yerel kalir')).not.toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Onay bekleyen hafızalar' })).toHaveTextContent(
      'Onay bekleyen hafızalar (1)',
    );
  });

  /** Kuyruk arsivli/suresi dolmus kayitlari da taramali. */
  it('taramayi filtresiz ve tavan limitle yapar', async () => {
    const harness = createHarness([PENDING]);
    renderSection(harness);
    await screen.findByText('Omer sabahlari calisiyor');

    expect(harness.list).toHaveBeenCalledExactlyOnceWith({
      archived: 'all',
      includeExpired: true,
      sort: 'recent',
      limit: PENDING_SCAN_LIMIT,
    });
    // Goruntulemek erisim degildir: `markAccessed` gonderilmez.
    expect(harness.list.mock.calls[0]?.[0].markAccessed).toBeUndefined();
  });

  it('kuyruk bossa hicbir sey cizmez', async () => {
    const harness = createHarness([NORMAL]);
    const { container } = renderSection(harness);

    await waitFor(() => {
      expect(harness.list).toHaveBeenCalled();
    });
    expect(container).toBeEmptyDOMElement();
  });

  it('onaylayinca bayragi kaldirir, diger metadata"yi korur ve listeden cikarir', async () => {
    const harness = createHarness([PENDING]);
    renderSection(harness);

    fireEvent.click(
      await screen.findByRole('button', { name: 'Onayla: Omer sabahlari calisiyor' }),
    );

    await waitFor(() => {
      expect(screen.queryByText('Omer sabahlari calisiyor')).not.toBeInTheDocument();
    });
    expect(harness.update).toHaveBeenCalledExactlyOnceWith(1, {
      metadataJson: '{"pendingApproval":false,"extraction":{"promptVersion":"v1"}}',
    });
    // Ana liste de tazelensin: onaylanan kayit orada gorunur olmali.
    expect(harness.onChanged).toHaveBeenCalled();
  });

  it('reddedince kaydi gercekten siler', async () => {
    const harness = createHarness([PENDING]);
    renderSection(harness);

    fireEvent.click(
      await screen.findByRole('button', { name: 'Reddet: Omer sabahlari calisiyor' }),
    );

    await waitFor(() => {
      expect(harness.remove).toHaveBeenCalledWith(1);
    });
    expect(harness.update).not.toHaveBeenCalled();
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Hafıza reddedildi ve silindi.',
    );
  });

  /** `skipped` basari sayilmaz: hafiza kapaliyken "onayladim" denmez. */
  it('hafiza kapaliyken onayladim demez', async () => {
    const harness = createHarness([PENDING]);
    harness.update.mockResolvedValueOnce({ status: 'skipped', reason: 'memory-disabled' });
    renderSection(harness);

    fireEvent.click(
      await screen.findByRole('button', { name: 'Onayla: Omer sabahlari calisiyor' }),
    );

    expect(await screen.findByRole('status')).toHaveTextContent(
      'Hafıza kapalı olduğu için işlem uygulanmadı.',
    );
    expect(screen.getByText('Omer sabahlari calisiyor')).toBeInTheDocument();
    expect(harness.onChanged).not.toHaveBeenCalled();
  });

  it('yazma hatasini yutmaz', async () => {
    const harness = createHarness([PENDING]);
    harness.remove.mockRejectedValueOnce(new Error('disk dolu'));
    renderSection(harness);

    fireEvent.click(
      await screen.findByRole('button', { name: 'Reddet: Omer sabahlari calisiyor' }),
    );

    expect(await screen.findByRole('alert')).toHaveTextContent('disk dolu');
    expect(screen.getByText('Omer sabahlari calisiyor')).toBeInTheDocument();
  });

  it('model ciktisi duz metin olarak basilir', async () => {
    const harness = createHarness([record({ content: '<b>enjekte</b>' })]);
    renderSection(harness);

    expect(await screen.findByText('<b>enjekte</b>')).toBeInTheDocument();
  });
});

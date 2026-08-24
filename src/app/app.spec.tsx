import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { DbStatus } from '../shared/db-status';

import { App } from './app';

// Kabuk testi IPC'ye dokunmaz: hafiza sekmesi acildiginda gercek `db_status`
// komutu cagrilmasin diye servis katmani sahtelenir (ASU-036).
const DISABLED: DbStatus = {
  availability: 'disabled',
  schemaVersion: null,
  sqliteVersion: '3.46.0',
  reason: null,
};

vi.mock('../asuna/memory/db-status-service', () => ({
  DB_STATUS_COMMAND: 'db_status',
  fetchDbStatus: (): Promise<DbStatus> => Promise.resolve(DISABLED),
}));

// Phase 0 smoke testi: test zincirinin (Vitest + jsdom + JSX + strict TS)
// uctan uca calistigini kanitlar. Gercek davranis testleri Phase 1'de gelir.
describe('App', () => {
  it('uygulama kabugunu Asuna basligiyla render eder', () => {
    render(<App />);

    expect(screen.getByRole('heading', { name: 'Asuna' })).toBeInTheDocument();
  });

  it('konusma sekmesiyle acilir, hafiza paneli monte degildir', () => {
    render(<App />);

    expect(screen.getByRole('tab', { name: 'Konuşma' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(document.getElementById('asuna-panel-memory')).toBeNull();
  });

  it('hafiza sekmesine gecince ses paneli monte kalir (oturum kopmaz)', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('tab', { name: 'Hafıza' }));

    expect(await screen.findByText(/Hafıza kapalı/)).toBeInTheDocument();

    const conversation = document.getElementById('asuna-panel-conversation');
    expect(conversation).toHaveAttribute('hidden');
    // Gizli ama YIKILMAMIS: canli Realtime oturumu sekme degisiminde kopmaz.
    expect(conversation?.querySelector('.asuna-panel')).not.toBeNull();
  });
});

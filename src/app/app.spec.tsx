import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { App } from './app';

// Phase 0 smoke testi: test zincirinin (Vitest + jsdom + JSX + strict TS)
// uctan uca calistigini kanitlar. Gercek davranis testleri Phase 1'de gelir.
describe('App', () => {
  it('uygulama kabugunu Asuna basligiyla render eder', () => {
    render(<App />);

    expect(screen.getByRole('heading', { name: 'Asuna' })).toBeInTheDocument();
  });
});

// jest-dom matcher'larini Vitest'in `expect`ine bagla (toBeInTheDocument vb.).
// Bu import ayni zamanda matcher tiplerini de getirir — tsconfig'de ayri
// `types` girdisine gerek kalmaz.
import '@testing-library/jest-dom/vitest';

import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';

// Her testten sonra DOM'u temizle: testler birbirinin durumunu gormesin.
afterEach(() => {
  cleanup();
});

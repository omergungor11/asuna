import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './app/app';
import './app/app.css';

const rootElement = document.getElementById('root');

if (!rootElement) {
  // Sessiz yutma yok: mount noktasi yoksa bu bir konfigurasyon hatasidir.
  throw new Error('Root element (#root) bulunamadi — index.html bozuk.');
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

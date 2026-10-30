import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles.css';
import './styles-light.css';
import './theme';
import { loadFonts } from './engine/fonts';
import { installNumberWheel } from './ui/fields';
import { installTooltips } from './ui/tooltips';
import { installEscape } from './ui/escape';

// Server fonts for every page (designer, operator, HTML5 outputs, plugins).
void loadFonts();
// Wheel on number fields and designed tooltips, everywhere.
installNumberWheel();
installTooltips();
// Esc closes the dialog on top.
installEscape();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

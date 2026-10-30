// Fonts available on the server (bundled, uploaded, system), loaded into the
// browser from the very files the Rust renderer uses — so a font picked in
// the designer looks the same in HTML5 and on SDI / ST 2110.
import { create } from 'zustand';

export interface FontFamily {
  family: string;
  weights: number[];
  source: 'bundled' | 'uploaded' | 'system';
  /** Uploaded fonts: their files (to remove them in Assets). */
  files?: string[];
}

export const useFonts = create<{ families: FontFamily[]; /** Bumped when a font finishes loading (text measurements change). */ ready: number }>(() => ({ families: [], ready: 0 }));

if (typeof document !== 'undefined' && document.fonts) {
  document.fonts.addEventListener('loadingdone', () => useFonts.setState((s) => ({ ready: s.ready + 1 })));
}

const cssEsc = (s: string) => s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');

export async function loadFonts(): Promise<void> {
  try {
    const res = await fetch('/api/fonts');
    if (!res.ok) return;
    const families = (await res.json()) as FontFamily[];
    const rules = families
      // Inter is declared statically in styles.css.
      .filter((f) => f.family !== 'Inter')
      .flatMap((f) => f.weights.map((w) => `@font-face{font-family:"${cssEsc(f.family)}";font-weight:${w};font-style:normal;font-display:block;src:url("/api/fonts/file/${encodeURIComponent(f.family)}/${w}")}`))
      .join('\n');
    let style = document.getElementById('pcg-fonts') as HTMLStyleElement | null;
    if (!style) {
      style = document.createElement('style');
      style.id = 'pcg-fonts';
      document.head.appendChild(style);
    }
    style.textContent = rules;
    useFonts.setState({ families });
  } catch {
    /* offline: Inter only */
  }
}

/** Closest available weight of a family (what both renderers will use). */
export function nearestWeight(f: FontFamily | undefined, w: number): number {
  if (!f?.weights.length) return w;
  return f.weights.reduce((a, b) => (Math.abs(b - w) < Math.abs(a - w) ? b : a));
}

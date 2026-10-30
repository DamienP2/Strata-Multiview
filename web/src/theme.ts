// The colour theme of the interface: dark (the default), light, or the
// computer's ("system", followed live). Kept for the user on the server and
// in this browser, so the first paint already has it (index.html reads the
// same key before the styles load).
//
// Light sets `data-theme="light"` on <html>; the light tokens live under
// `:root[data-theme='light']` in styles-light.css. Pictures (stage,
// monitors, previews) stay dark in both.
import { create } from 'zustand';

export type ThemePref = 'dark' | 'light' | 'system';
export const THEMES: ThemePref[] = ['dark', 'light', 'system'];

const KEY = 'strata.theme';
const isTheme = (v: unknown): v is ThemePref => v === 'dark' || v === 'light' || v === 'system';

function first(): ThemePref {
  try {
    const saved = localStorage.getItem(KEY);
    if (isTheme(saved)) return saved;
  } catch {
    /* private mode */
  }
  return 'dark';
}

const media = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: light)') : null;
let pref: ThemePref = first();

/** Output pages (on air, embedded previews) keep the dark tokens. */
const onAir = () => /^\/(output|embed)(\/|$)/.test(location.pathname);

function apply() {
  const light = !onAir() && (pref === 'light' || (pref === 'system' && !!media?.matches));
  const root = document.documentElement;
  if (light) root.dataset.theme = 'light';
  else delete root.dataset.theme;
}

apply();
media?.addEventListener('change', () => pref === 'system' && apply());

/** The chosen theme; the user menu re-renders when it changes. */
export const useTheme = create<{ theme: ThemePref }>(() => ({ theme: pref }));

/** Shows a theme (and remembers it in this browser); empty or unknown values are ignored. */
export function setTheme(t: string | null | undefined) {
  if (!isTheme(t)) return;
  try {
    localStorage.setItem(KEY, t);
  } catch {
    /* private mode */
  }
  if (t === pref) return;
  pref = t;
  apply();
  useTheme.setState({ theme: t });
}

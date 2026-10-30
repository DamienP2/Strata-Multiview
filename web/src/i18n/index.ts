// The interface in English (the source texts) or French. A text is looked
// up by its English wording, so an untranslated text stays in English:
//
//   t('New project')                       → "Nouveau projet"
//   t('Delete “{name}”?', { name })        → "Supprimer « … » ?"
//   tn(n, '{n} project', '{n} projects')   → "3 projets"
//
// Call t() when drawing, not when a module loads (lists of labels keep
// their English texts and are translated where they are shown).
import { create } from 'zustand';
import { fr } from './fr';

export type Lang = 'en' | 'fr';
export const LANGS: { id: Lang; label: string; name: string }[] = [
  { id: 'en', label: 'EN', name: 'English' },
  { id: 'fr', label: 'FR', name: 'Français' },
];

const KEY = 'strata.lang';
const isLang = (l: unknown): l is Lang => l === 'en' || l === 'fr';

function first(): Lang {
  try {
    const saved = localStorage.getItem(KEY);
    if (isLang(saved)) return saved;
  } catch {
    /* private mode */
  }
  return navigator.language?.toLowerCase().startsWith('fr') ? 'fr' : 'en';
}

let current: Lang = first();
document.documentElement.lang = current;

/** The language shown; components that hold texts re-render when it changes. */
export const useLang = create<{ lang: Lang }>(() => ({ lang: current }));

export function setLang(l: string | null | undefined) {
  if (!isLang(l) || l === current) return;
  current = l;
  document.documentElement.lang = l;
  try {
    localStorage.setItem(KEY, l);
  } catch {
    /* private mode */
  }
  useLang.setState({ lang: l });
}

export const lang = () => current;

let picked = false;
/** A language picked by hand (login page): it wins over the one saved for the user. */
export function pickLang(l: Lang) {
  picked = true;
  setLang(l);
}
export const pickedByHand = () => picked;

/** Locale for dates and numbers. */
export const locale = () => (current === 'fr' ? 'fr-FR' : undefined);

const DICT: Record<Lang, Record<string, string>> = { en: {}, fr };

/** The text in the interface's language, with `{name}` replaced by vars. */
export function t(s: string, vars?: Record<string, string | number>): string {
  const out = DICT[current][s] ?? s;
  return vars ? out.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : out;
}

/**
 * The text in another language than the interface's: the project's, for the
 * texts a widget draws on air (`tl(lang, 'No data')`). "fr…" gives the French.
 */
export function tl(language: string | undefined, s: string, vars?: Record<string, string | number>): string {
  const out = ((language ?? 'fr').toLowerCase().startsWith('fr') ? DICT.fr[s] : undefined) ?? s;
  return vars ? out.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : out;
}

/**
 * A short text whose French depends on what it is about: `tc('theme',
 * 'Light')` looks up 'theme|Light' first ("Clair", not the font weight).
 */
export function tc(context: string, s: string, vars?: Record<string, string | number>): string {
  const key = `${context}|${s}`;
  return key in DICT[current] ? t(key, vars) : t(s, vars);
}

/** Singular or plural by `n` (French: 0 and 1 are singular), `{n}` replaced. */
export function tn(n: number, one: string, other: string, vars?: Record<string, string | number>): string {
  const single = current === 'fr' ? Math.abs(n) < 2 : n === 1;
  return t(single ? one : other, { n, ...vars });
}

// Settings of a layer the Data canvas can wire from the data (a colour, a size,
// a switch…): each one is a path in the layer, set through Layer.settingBindings.
import type { Layer } from './types';

/** Not settings: identity, animation, bindings, and content wired elsewhere. */
const SKIP = new Set(['id', 'name', 'parent', 'type', 'visible', 'locked', 'props', 'tracks', 'bindings', 'settingBindings', 'overrides', 'timeline', 'manualStart', 'scaleLock', 'lockRatio', 'grid', 'entries', 'merges', 'link', 'csv', 'items', 'text', 'src']);

/** Settings a widget has that its saved form may leave out (unset = inherited). */
const EXTRA: Record<string, string[]> = {
  table: ['txt.title.color', 'txt.header.color', 'txt.rows.color', 'txt.rank.color', 'txt.title.size', 'txt.header.size', 'txt.rows.size'],
};

const WORDS: Record<string, string> = {
  bg: 'background',
  st: '',
  cf: '',
  txt: 'text',
  hl: 'highlighted row',
  odd: 'odd rows',
  even: 'even rows',
  top3: 'top rows',
  hcol: 'first column',
  tableBg: 'table background',
  innerH: 'lines between rows',
  innerV: 'lines between columns',
  outer: 'outline',
  cell: 'one cell',
  rank: 'rank',
  kw: 'weight',
  img: 'picture',
  on: 'shown',
  w: 'width',
  h: 'height',
  pct: 'width %',
  fmt: 'format',
  neg: 'negatives',
};
const human = (k: string) => (k in WORDS ? WORDS[k] : k.replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase());

export interface SettingPort {
  path: string;
  /** "Title · from" */
  label: string;
  /** Its group in the picker ("Title", "Column team"…). */
  group: string;
  value: unknown;
}

/** Translates a word of a label (the interface's t(); English by default). */
export type Tr = (text: string, vars?: Record<string, string | number>) => string;
const same: Tr = (s) => s;

/** A readable name for a setting path; `tr` translates its words when it is shown. */
export function settingLabel(l: Layer, path: string, tr: Tr = same): { label: string; group: string } {
  const ks = path.split('.');
  const parts: string[] = [];
  for (let i = 0; i < ks.length; i++) {
    const k = ks[i];
    if (/^\d+$/.test(k) && i > 0) {
      // An item of a list: its own name (a column's label).
      const item = ks.slice(0, i + 1).reduce<unknown>((a, x) => (a as Record<string, unknown>)?.[x], l) as Record<string, unknown> | undefined;
      const name = (item?.label || item?.src || item?.name || `#${+k + 1}`) as string;
      parts[parts.length - 1] = `${ks[i - 1] === 'cols' ? tr('column') : parts[parts.length - 1]} ${name}`;
      continue;
    }
    const w = human(k);
    if (w) parts.push(tr(w));
  }
  const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
  // A setting of the widget itself (titleFrom, radius…): in General.
  if (parts.length < 2) return { group: tr('General'), label: `${tr('General')} · ${parts[0] ?? path}` };
  const group = cap(parts[0]);
  return { group, label: `${group} · ${parts.slice(1).join(' ')}` };
}

/** Every setting of the layer that takes a single value (`tr` as for settingLabel). */
export function settingPorts(l: Layer, tr: Tr = same): SettingPort[] {
  const out: SettingPort[] = [];
  const seen = new Set<string>();
  const add = (path: string, value: unknown) => {
    if (seen.has(path)) return;
    seen.add(path);
    out.push({ path, value, ...settingLabel(l, path, tr) });
  };
  const walk = (o: unknown, path: string, depth: number) => {
    if (o == null || depth > 5) return;
    if (Array.isArray(o)) return o.forEach((x, i) => typeof x === 'object' && walk(x, `${path}.${i}`, depth + 1));
    if (typeof o === 'object') {
      for (const [k, v] of Object.entries(o as Record<string, unknown>)) if (!(depth === 0 && SKIP.has(k))) walk(v, path ? `${path}.${k}` : k, depth + 1);
      return;
    }
    // Long or templated texts are content, not settings.
    if (typeof o === 'string' && (o.length > 120 || o.includes('{{'))) return;
    add(path, o);
  };
  walk(l, '', 0);
  for (const p of EXTRA[l.type] ?? []) add(p, undefined);
  return out;
}

export const isColorValue = (v: unknown) => typeof v === 'string' && /^#[0-9a-f]{3,8}$/i.test(v);

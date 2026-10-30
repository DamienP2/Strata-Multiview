import { create } from 'zustand';
import { parseHex, toHex } from './engine/evaluate';
import type { Color, NamedColor, Palette } from './engine/types';
import { api, changeEvents, useApp } from './store';

/*
 * Library › Colours: the shared palettes and the open project's own. A layer
 * keeps the value it took from a palette; when the palette changes a colour,
 * every value equal to the old one takes the new (the server rewrites the
 * saved projects, the open Designer its working copy: `colors-replaced`).
 */

/** 8-bit RGBA: one colour however it is written (#fff, #FFFFFFFF, [1, 1, 1, 1]). */
const key = (c: Color) => c.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)).join(',');
export const colorKey = (v: string | Color | undefined) => {
  const c = typeof v === 'string' ? parseHex(v) : v;
  return c ? key(c) : '';
};
export const sameColor = (a: string | Color | undefined, b: string | Color | undefined) => !!a && !!b && colorKey(a) !== '' && colorKey(a) === colorKey(b);

const RECENT = 'strata.recentColors';
const readRecent = (): string[] => {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT) ?? '[]');
    return Array.isArray(v) ? v.filter((x) => typeof x === 'string').slice(0, 9) : [];
  } catch {
    return [];
  }
};

interface Colors {
  palettes: Palette[];
  loaded: boolean;
  recent: string[];
  load: () => Promise<void>;
  /** Saves the shared palettes (the projects follow the changed colours). */
  save: (palettes: Palette[]) => Promise<void>;
  /** Saves the open project's palette. */
  saveProject: (projectId: string, colors: NamedColor[]) => Promise<void>;
  pushRecent: (hex: string) => void;
}

/** (old, new) of the colours whose value changed (same id). */
function changes(old: NamedColor[], next: NamedColor[]): [string, string][] {
  return next.flatMap((n) => {
    const o = old.find((x) => x.id === n.id);
    return o && !sameColor(o.value, n.value) && parseHex(n.value) ? [[o.value, n.value] as [string, string]] : [];
  });
}
const tell = (ch: [string, string][]) => ch.length && window.dispatchEvent(new CustomEvent('colors-replaced', { detail: ch }));

export const useColors = create<Colors>((set, get) => ({
  palettes: [],
  loaded: false,
  recent: readRecent(),
  async load() {
    try {
      set({ palettes: await api<Palette[]>('GET', '/api/colors'), loaded: true });
    } catch {
      set({ loaded: true });
    }
  },
  async save(palettes) {
    const ch = changes(
      get().palettes.flatMap((p) => p.colors),
      palettes.flatMap((p) => p.colors),
    );
    const before = get().palettes;
    set({ palettes });
    try {
      await api('PUT', '/api/colors', palettes);
      tell(ch);
    } catch (e) {
      set({ palettes: before });
      useApp.getState().toast((e as Error).message, 'error');
    }
  },
  async saveProject(projectId, colors) {
    const old = useApp.getState().full[projectId]?.colors ?? [];
    try {
      await api('PUT', `/api/projects/${projectId}/colors`, { colors });
      tell(changes(old, colors));
    } catch (e) {
      useApp.getState().toast((e as Error).message, 'error');
    }
  },
  pushRecent(hex) {
    const recent = [hex, ...get().recent.filter((x) => !sameColor(x, hex))].slice(0, 9);
    set({ recent });
    try {
      localStorage.setItem(RECENT, JSON.stringify(recent));
    } catch {
      /* not kept */
    }
  },
}));

/** The palette colour a value is (the project's first), if any. */
export function namedOf(value: string | Color | undefined, project = useApp.getState().project): NamedColor | undefined {
  if (!value || !colorKey(value)) return undefined;
  return (project?.colors ?? []).find((c) => sameColor(c.value, value)) ?? useColors.getState().palettes.flatMap((p) => p.colors).find((c) => sameColor(c.value, value));
}

/**
 * Replaces `from` by `to` in an object (a template): hex strings anywhere,
 * `[r, g, b, a]` property and keyframe values. Mirror of cg_core::palette.
 */
export function replaceColor(v: unknown, from: string, to: string): number {
  const f = colorKey(from);
  const c = parseHex(to);
  if (!f || !c) return 0;
  const hexTo = toHex(c, c[3] < 1);
  const isRgba = (x: unknown): x is number[] => Array.isArray(x) && x.length === 4 && x.every((n) => typeof n === 'number' && n >= 0 && n <= 1);
  let n = 0;
  const walk = (x: unknown, prop: boolean): unknown => {
    if (prop && isRgba(x) && key(x as Color) === f) {
      n++;
      return [...c];
    }
    if (typeof x === 'string') {
      if (x.trim().startsWith('#') && colorKey(x) === f) {
        n++;
        return hexTo;
      }
      return x;
    }
    if (Array.isArray(x)) {
      for (let i = 0; i < x.length; i++) x[i] = walk(x[i], false);
      return x;
    }
    if (x && typeof x === 'object') {
      const o = x as Record<string, unknown>;
      for (const k of Object.keys(o)) {
        if (k === 'props' && o[k] && typeof o[k] === 'object' && !Array.isArray(o[k])) {
          const props = o[k] as Record<string, unknown>;
          for (const p of Object.keys(props)) props[p] = walk(props[p], true);
        } else o[k] = walk(o[k], k === 'value');
      }
    }
    return x;
  };
  walk(v, false);
  return n;
}

// Changed elsewhere (another page, another user): read again.
changeEvents.addEventListener('colors', () => void useColors.getState().load());

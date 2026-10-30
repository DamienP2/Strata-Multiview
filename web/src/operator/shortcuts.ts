import { useEffect, useRef } from 'react';
import { create } from 'zustand';
import { t } from '../i18n';
import { api, useApp } from '../store';

/*
 * Each user's keyboard shortcuts in the Operator (saved on the server,
 * GET / PUT /api/me/shortcuts). An action is a fixed id ("next", "take:3")
 * or one named after the project ("outZone:Bug", "trigger:reveal"). A key
 * is written "Ctrl+Alt+Shift+Meta+Key" ("F1", "Shift+B", "Space", "1").
 */

export interface Binding {
  action: string;
  keys: string[];
}

export interface ActionDef {
  id: string;
  group: 'air' | 'list';
  label: () => string;
}

/** The actions every project has, in the order of the settings page. */
export const ACTIONS: ActionDef[] = [
  { id: 'next', group: 'air', label: () => t('NEXT · take the next graphic') },
  { id: 'take', group: 'air', label: () => t('TAKE · the selected graphic') },
  { id: 'continue', group: 'air', label: () => t('CONTINUE · front-most graphic') },
  { id: 'continueAll', group: 'air', label: () => t('CONTINUE · every graphic on air') },
  { id: 'out', group: 'air', label: () => t('OUT · front-most graphic') },
  { id: 'outSelected', group: 'air', label: () => t('OUT · the selected graphic') },
  { id: 'clear', group: 'air', label: () => t('Clear all') },
  { id: 'undo', group: 'air', label: () => t('Undo the last action') },
  { id: 'update', group: 'air', label: () => t('UPDATE · send the values') },
  { id: 'prev', group: 'list', label: () => t('Previous graphic') },
  { id: 'nextItem', group: 'list', label: () => t('Next graphic') },
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n): ActionDef => ({ id: `take:${n}`, group: 'list', label: () => t('Take graphic no. {n}', { n }) })),
  { id: 'schedule', group: 'list', label: () => t('Scheduled takes') },
];

/** The label of any action, the project's own ones too. */
export function actionLabel(id: string): string {
  const def = ACTIONS.find((a) => a.id === id);
  if (def) return def.label();
  if (id.startsWith('outZone:')) return t('OUT zone {name}', { name: id.slice(8) });
  if (id.startsWith('trigger:')) return t('Trigger “{name}”', { name: id.slice(8) });
  return id;
}

/** The Broadcast profile: the keys of the Operator's buttons. */
export const BROADCAST: Binding[] = [
  { action: 'next', keys: ['F1', 'Enter'] },
  { action: 'take', keys: [] },
  { action: 'continue', keys: ['F2', 'Space'] },
  { action: 'continueAll', keys: [] },
  { action: 'out', keys: ['F3'] },
  { action: 'outSelected', keys: [] },
  { action: 'clear', keys: ['F12'] },
  { action: 'undo', keys: ['Ctrl+Z', 'Meta+Z'] },
  { action: 'update', keys: ['F4'] },
  { action: 'prev', keys: ['ArrowUp'] },
  { action: 'nextItem', keys: ['ArrowDown'] },
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => ({ action: `take:${n}`, keys: [String(n)] })),
  { action: 'schedule', keys: [] },
];

/** Keys that also reload the page: Strata takes them while the Operator is open. */
export const BROWSER_KEYS = ['F5', 'Ctrl+F5', 'Ctrl+R', 'Meta+R', 'Ctrl+Shift+R', 'Shift+Meta+R'];

const MODIFIERS = new Set(['Control', 'Shift', 'Alt', 'Meta', 'AltGraph', 'CapsLock', 'OS', 'Fn', 'Hyper', 'Super']);

/** The key of a key press as written in the bindings (`null`: a modifier alone). */
export function keyOf(e: KeyboardEvent): string | null {
  if (MODIFIERS.has(e.key)) return null;
  let k: string;
  const digit = /^(?:Digit|Numpad)(\d)$/.exec(e.code);
  const letter = /^Key([A-Z])$/.exec(e.code);
  if (digit) k = digit[1];
  else if (e.key === ' ' || e.code === 'Space') k = 'Space';
  // Alt changes the character (macOS): the letter of the key then.
  else if (e.altKey && letter) k = letter[1];
  else if (e.key.length === 1) k = e.key.toUpperCase();
  else k = e.key;
  if (!k || k === 'Unidentified' || k === 'Dead') return null;
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Meta'].filter(Boolean) as string[];
  return [...mods, k].join('+');
}

/** "Shift+B" → ["Shift", "B"] ("Ctrl++" → ["Ctrl", "+"]). */
export function keyParts(key: string): string[] {
  if (key === '+') return ['+'];
  if (key.endsWith('++')) return [...key.slice(0, -2).split('+').filter(Boolean), '+'];
  return key.split('+');
}

const mac = typeof navigator !== 'undefined' && /Mac|iP(hone|ad)/.test(navigator.platform || navigator.userAgent);

/** One part of a key as shown on a key cap. */
export function capLabel(part: string): string {
  switch (part) {
    case 'Ctrl':
      return mac ? '⌃' : 'Ctrl';
    case 'Meta':
      return mac ? '⌘' : 'Win';
    case 'Alt':
      return mac ? '⌥' : 'Alt';
    case 'Shift':
      return '⇧';
    case 'ArrowUp':
      return '↑';
    case 'ArrowDown':
      return '↓';
    case 'ArrowLeft':
      return '←';
    case 'ArrowRight':
      return '→';
    case 'Enter':
      return t('Enter');
    case 'Space':
      return t('Space');
    case 'Escape':
      return t('Esc');
    case 'Backspace':
      return '⌫';
    case 'Delete':
      return t('Del');
    case 'PageUp':
      return t('Page up');
    case 'PageDown':
      return t('Page down');
    default:
      return part;
  }
}

/** The bindings in force: the user's, else the Broadcast profile. */
export const effective = (b: Binding[] | null): Binding[] => {
  if (!b) return BROADCAST;
  // Update was on F5 (which also reloads the page): saved before it moved to F4, it follows.
  const upd = b.find((x) => x.action === 'update');
  if (upd?.keys.length !== 1 || upd.keys[0] !== 'F5' || b.some((x) => x.keys.includes('F4'))) return b;
  return b.map((x) => (x === upd ? { ...x, keys: ['F4'] } : x));
};

/** The actions a key runs. */
export const actionsOf = (bindings: Binding[], key: string) => bindings.filter((b) => b.keys.includes(key)).map((b) => b.action);

/** Keys bound to more than one action. */
export function conflicts(bindings: Binding[]): Set<string> {
  const seen = new Map<string, number>();
  for (const b of bindings) for (const k of new Set(b.keys)) seen.set(k, (seen.get(k) ?? 0) + 1);
  return new Set([...seen].filter(([, n]) => n > 1).map(([k]) => k));
}

interface ShortcutStore {
  /** The user's bindings (`null`: never set, the Broadcast profile). */
  bindings: Binding[] | null;
  loaded: boolean;
  load: () => Promise<void>;
  save: (b: Binding[] | null) => Promise<void>;
}

export const useShortcutPrefs = create<ShortcutStore>((set, get) => ({
  bindings: null,
  loaded: false,
  async load() {
    try {
      const r = await api<{ bindings: Binding[] | null }>('GET', '/api/me/shortcuts');
      set({ bindings: r.bindings ?? null, loaded: true });
    } catch {
      set({ loaded: true });
    }
  },
  async save(b) {
    const before = get().bindings;
    set({ bindings: b });
    try {
      await api('PUT', '/api/me/shortcuts', { bindings: b });
    } catch (e) {
      set({ bindings: before });
      useApp.getState().toast((e as Error).message, 'error');
    }
  },
}));

/** What the Operator does for each action (the same functions as its buttons). */
export interface ShortcutHandlers {
  next: () => void;
  take: () => void;
  continue: () => void;
  continueAll: () => void;
  out: () => void;
  outSelected: () => void;
  clear: () => void;
  undo: () => void;
  update: () => void;
  /** Selects the previous (-1) or next (+1) graphic. */
  select: (delta: number) => void;
  /** Takes the n-th graphic of the playlist (1 = first). */
  takeAt: (n: number) => void;
  outZone: (name: string) => void;
  trigger: (name: string) => void;
  schedule: () => void;
}

function run(h: ShortcutHandlers, action: string) {
  const take = /^take:(\d+)$/.exec(action);
  if (take) return h.takeAt(+take[1]);
  if (action.startsWith('outZone:')) return h.outZone(action.slice(8));
  if (action.startsWith('trigger:')) return h.trigger(action.slice(8));
  switch (action) {
    case 'prev':
      return h.select(-1);
    case 'nextItem':
      return h.select(1);
    case 'next':
    case 'take':
    case 'continue':
    case 'continueAll':
    case 'out':
    case 'outSelected':
    case 'clear':
    case 'undo':
    case 'update':
    case 'schedule':
      return h[action]();
  }
}

/** Typing in a field: its keys are not the Operator's (but F1…F12 are). */
function inField(e: KeyboardEvent): boolean {
  const el = e.target as HTMLElement | null;
  return !!(el?.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"])') || el?.isContentEditable);
}
/** A dialog open: no key is the Operator's. */
const inDialog = () => !!document.querySelector('.modal-back, .sch-pop');
const FUNCTION_KEY = /^F\d{1,2}$/;

/** The user's keyboard shortcuts in the Operator. */
export function useShortcuts(handlers: ShortcutHandlers) {
  const ref = useRef(handlers);
  ref.current = handlers;
  const loaded = useShortcutPrefs((s) => s.loaded);
  useEffect(() => {
    if (!loaded) void useShortcutPrefs.getState().load();
  }, [loaded]);
  useEffect(() => {
    // Typing in a field, F1…F12 are taken before the field (some fields keep their keys to themselves);
    // every other key once nothing else took it.
    const onKey = (capture: boolean) => (e: KeyboardEvent) => {
      // A key the selected widget on air took (its own shortcuts) is not the Operator's.
      if (e.defaultPrevented || inDialog()) return;
      const key = keyOf(e);
      if (!key) return;
      const field = inField(e);
      if (capture !== (field && FUNCTION_KEY.test(key))) return;
      const bindings = effective(useShortcutPrefs.getState().bindings);
      const action = actionsOf(bindings, key)[0];
      if (!action) return;
      e.preventDefault();
      // Held down: only the selection moves on.
      if (e.repeat && action !== 'prev' && action !== 'nextItem') return;
      // A focused button (a widget's, a row's) would also "click" on Space / Enter.
      if (key === 'Space' || key === 'Enter') (e.target as HTMLElement).closest?.('button')?.blur();
      // F4 right after typing a value: the field hands its value over first (some do on leaving).
      if (field) (e.target as HTMLElement).blur?.();
      run(ref.current, action);
    };
    const inFields = onKey(true);
    const elsewhere = onKey(false);
    window.addEventListener('keydown', inFields, true);
    window.addEventListener('keydown', elsewhere);
    return () => {
      window.removeEventListener('keydown', inFields, true);
      window.removeEventListener('keydown', elsewhere);
    };
  }, []);
}

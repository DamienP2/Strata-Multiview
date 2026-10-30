import { create } from 'zustand';
import { displayPref, setDisplayPref, useDisplayPrefs } from '../displayPrefs';

/*
 * The other templates shown faint under the open one (Display › Other
 * templates), to arrange it against them: all of them, or the ones picked.
 */

interface Ghosts {
  on: boolean;
  /** The templates shown; null: all the others. */
  ids: string[] | null;
  /** Their opacity, 0..1. */
  opacity: number;
}

const KEY = 'designer.ghosts';
const load = (): Ghosts => ({ on: false, ids: null, opacity: 0.35, ...displayPref<Partial<Ghosts>>(KEY, {}) });

export const useGhosts = create<Ghosts & { set: (v: Partial<Ghosts>) => void }>((set, get) => ({
  ...load(),
  set(v) {
    set(v);
    const { on, ids, opacity } = get();
    setDisplayPref(KEY, { on, ids, opacity });
  },
}));
// Another user logs in: their choices.
useDisplayPrefs.subscribe((s, before) => s.user !== before.user && useGhosts.setState(load()));

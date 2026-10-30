import { create } from 'zustand';
import { outFrame, resolveLayer } from '../engine/evaluate';
import type { Easing, Keyframe, Layer, MotionKind, Prop, Template } from '../engine/types';
import { findLayer, useDesigner } from './designerStore';

/*
 * Designer › Animations: ready-made in, out and loop animations for layers.
 * They fit the layer: its size (how far it slides), its type (a stroke is
 * drawn, a text spreads its letters, a picture zooms slowly) and its time
 * (the in ends before the On air point, the out after it, both as long as
 * the layer is shown allows).
 */

export type Cat = 'in' | 'out' | 'loop';
/** Kinds of layer a preset is for (none: all). */
export type Kind = 'text' | 'shape' | 'image' | 'pen' | 'other';
type Ease = 'linear' | 'out' | 'in' | 'inOut' | 'back' | 'elastic' | 'bounce';

/**
 * What moves, from → to:
 * x, y: in slide lengths (from the layer's size); scale, scaleX, scaleY,
 * opacity: times the layer's own; rotation, letterSpacing: added; crop*,
 * draw*: as they are. An in goes from → the layer as set; an out the other way.
 */
type Move = Partial<Record<'x' | 'y' | 'scale' | 'scaleX' | 'scaleY' | 'opacity' | 'rotation' | 'letterSpacing' | 'cropLeft' | 'cropRight' | 'cropTop' | 'cropBottom' | 'drawStart' | 'drawEnd', [number, number]>>;

export interface Preset {
  id: string;
  cat: Cat;
  label: string;
  move?: Move;
  ease: Ease;
  only?: Kind[];
  /** Loop: the movement repeated on air, and one cycle (s). */
  motion?: MotionKind;
  period?: number;
}

const p = (cat: Cat, id: string, label: string, move: Move, ease: Ease, only?: Kind[]): Preset => ({ id, cat, label, move, ease, only });
const loop = (id: string, label: string, motion: MotionKind, period: number, preview: Move, only?: Kind[]): Preset => ({ id, cat: 'loop', label, move: preview, ease: 'inOut', motion, period, only });

export const PRESETS: Preset[] = [
  // In
  p('in', 'fade', 'Fade', { opacity: [0, 1] }, 'out'),
  p('in', 'slide-l', 'Slide from the left', { x: [-1, 0], opacity: [0, 1] }, 'out'),
  p('in', 'slide-r', 'Slide from the right', { x: [1, 0], opacity: [0, 1] }, 'out'),
  p('in', 'rise', 'Rise', { y: [1, 0], opacity: [0, 1] }, 'out'),
  p('in', 'drop', 'Drop', { y: [-1, 0], opacity: [0, 1] }, 'out'),
  p('in', 'zoom-in', 'Zoom in', { scale: [0.6, 1], opacity: [0, 1] }, 'out'),
  p('in', 'zoom-out', 'Zoom out', { scale: [1.4, 1], opacity: [0, 1] }, 'out'),
  p('in', 'pop', 'Pop', { scale: [0.4, 1], opacity: [0, 1] }, 'back'),
  p('in', 'bounce', 'Bounce', { y: [-1.5, 0] }, 'bounce'),
  p('in', 'elastic', 'Elastic', { scaleX: [0.2, 1], opacity: [0, 1] }, 'elastic'),
  p('in', 'wipe-l', 'Wipe from the left', { cropRight: [1, 0] }, 'inOut'),
  p('in', 'wipe-u', 'Wipe up', { cropTop: [1, 0] }, 'inOut'),
  p('in', 'open-c', 'Open from the centre', { cropLeft: [0.5, 0], cropRight: [0.5, 0] }, 'inOut'),
  p('in', 'spin', 'Spin', { rotation: [-90, 0], scale: [0.5, 1], opacity: [0, 1] }, 'out'),
  p('in', 'flip', 'Flip', { scaleY: [0, 1] }, 'back'),
  p('in', 'slide-wipe', 'Slide and wipe', { x: [-0.35, 0], cropRight: [1, 0] }, 'out'),
  p('in', 'letters', 'Letters closing up', { letterSpacing: [30, 0], opacity: [0, 1] }, 'out', ['text']),
  p('in', 'draw', 'Draw the stroke', { drawEnd: [0, 100] }, 'inOut', ['pen']),
  p('in', 'ken', 'Slow zoom', { scale: [1.15, 1], opacity: [0, 1] }, 'out', ['image']),
  // Out
  p('out', 'fade', 'Fade', { opacity: [1, 0] }, 'in'),
  p('out', 'slide-l', 'Slide to the left', { x: [0, -1], opacity: [1, 0] }, 'in'),
  p('out', 'slide-r', 'Slide to the right', { x: [0, 1], opacity: [1, 0] }, 'in'),
  p('out', 'rise', 'Rise', { y: [0, -1], opacity: [1, 0] }, 'in'),
  p('out', 'drop', 'Drop', { y: [0, 1], opacity: [1, 0] }, 'in'),
  p('out', 'shrink', 'Shrink', { scale: [1, 0.6], opacity: [1, 0] }, 'in'),
  p('out', 'grow', 'Grow', { scale: [1, 1.4], opacity: [1, 0] }, 'in'),
  p('out', 'wipe-r', 'Wipe to the right', { cropLeft: [0, 1] }, 'inOut'),
  p('out', 'wipe-d', 'Wipe down', { cropBottom: [0, 1] }, 'inOut'),
  p('out', 'close-c', 'Close to the centre', { cropLeft: [0, 0.5], cropRight: [0, 0.5] }, 'inOut'),
  p('out', 'spin', 'Spin', { rotation: [0, 90], scale: [1, 0.5], opacity: [1, 0] }, 'in'),
  p('out', 'undraw', 'Erase the stroke', { drawStart: [0, 100] }, 'inOut', ['pen']),
  // Loop (on air)
  loop('pulse', 'Pulse', 'pulse', 1.6, { scale: [1, 1.06] }),
  loop('blink', 'Blink', 'blink', 1.2, { opacity: [1, 0.25] }),
  loop('swing', 'Swing', 'swing', 2, { rotation: [-3, 3] }),
  loop('float', 'Float', 'float', 2.4, { y: [-0.12, 0.12] }),
  loop('shake', 'Shake', 'shake', 0.25, { x: [-0.04, 0.04] }),
  loop('kenburns', 'Ken Burns', 'kenBurns', 10, { scale: [1, 1.08] }, ['image']),
];

export function kindOf(l: Layer): Kind {
  if (l.type === 'text') return 'text';
  if (l.type === 'image') return 'image';
  if (l.type === 'pen') return 'pen';
  if (l.type === 'rect' || l.type === 'ellipse' || l.type === 'polygon') return 'shape';
  return 'other';
}

/** The presets for these layers: the general ones, and those of their kind when all are of it. */
export function presetsFor(cat: Cat, layers: Layer[]): Preset[] {
  const kinds = new Set(layers.map(kindOf));
  return PRESETS.filter((x) => x.cat === cat && (!x.only || (kinds.size === 1 && x.only.includes([...kinds][0]))));
}

const EASE: Record<Ease, Easing> = {
  linear: { type: 'linear' },
  out: { type: 'easeOut' },
  in: { type: 'easeIn' },
  inOut: { type: 'easeInOut' },
  back: { type: 'back' },
  elastic: { type: 'elastic' },
  bounce: { type: 'bounce' },
};

/** Easing functions of the previews (0..1). */
export const EASE_FN: Record<Ease, (t: number) => number> = {
  linear: (t) => t,
  out: (t) => 1 - Math.pow(1 - t, 3),
  in: (t) => t * t * t,
  inOut: (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2),
  back: (t) => 1 + 2.70158 * Math.pow(t - 1, 3) + 1.70158 * Math.pow(t - 1, 2),
  elastic: (t) => (t === 0 || t === 1 ? t : Math.pow(2, -10 * t) * Math.sin((t * 10 - 0.75) * ((2 * Math.PI) / 3)) + 1),
  bounce: (t) => {
    const n = 7.5625;
    const d = 2.75;
    if (t < 1 / d) return n * t * t;
    if (t < 2 / d) return n * (t -= 1.5 / d) * t + 0.75;
    if (t < 2.5 / d) return n * (t -= 2.25 / d) * t + 0.9375;
    return n * (t -= 2.625 / d) * t + 0.984375;
  },
};

/** Where a layer is shown: its span, and the template's On air point. */
function shown(t: Template, l: Layer) {
  const from = l.span?.from ?? 0;
  const to = Math.min(t.duration, l.span?.to ?? t.duration);
  const hold = outFrame(t) ?? t.duration;
  return { from, to, hold };
}

/** The length of an in or out (s) that fits a layer shown `secs`: 1.5 s, less on a short layer (half of it, at least 0.3 s). */
export const autoSecs = (secs: number) => Math.min(1.5, Math.max(0.3, secs * 0.5));

/** The automatic length for these layers (the first one's). */
export function autoFor(t: Template, layers: Layer[], fps: number): number {
  const l = layers[0];
  if (!l) return 1.5;
  const w = shown(t, l);
  return autoSecs((w.to - w.from) / fps);
}

export interface Choice {
  in: string | null;
  out: string | null;
  /** A loop preset, 'none' to take the loop off, null to leave it as it is. */
  loop: string | null;
  /** Seconds, or null: automatic. */
  secs: number | null;
}

/** The numbers a preset moves between for a layer: [from, to] in the layer's own values. */
function values(key: keyof Move, k: [number, number], base: (p: Prop) => number, w: number, h: number): [Prop[], [number, number]] {
  const slideX = Math.min(300, Math.max(40, w * 0.35));
  const slideY = Math.min(200, Math.max(20, h * 0.6));
  const at = (v: number, p: Prop): number => {
    switch (key) {
      case 'x':
        return base('x') + v * slideX;
      case 'y':
        return base('y') + v * slideY;
      case 'scale':
      case 'scaleX':
      case 'scaleY':
      case 'opacity':
        return base(p) * v;
      case 'rotation':
      case 'letterSpacing':
        return base(p) + v;
      default:
        // Crop and draw: the preset's own values, back to the layer's at rest.
        return v;
    }
  };
  const props: Prop[] = key === 'scale' ? ['scaleX', 'scaleY'] : [key as Prop];
  return [props, [at(k[0], props[0]), at(k[1], props[0])]];
}

/**
 * Puts the chosen animations on the layers (in their order in the template,
 * one after the other when there are several). The keyframes of the
 * properties they move are replaced; the others stay.
 */
export function applyAnimations(t: Template, ids: string[], c: Choice, fps: number) {
  const order = t.layers.filter((l) => ids.includes(l.id));
  const step = order.length > 1 ? Math.round(0.08 * fps) : 0;
  const n = order.length;
  const pin = c.in ? PRESETS.find((x) => x.cat === 'in' && x.id === c.in) : undefined;
  const pout = c.out ? PRESETS.find((x) => x.cat === 'out' && x.id === c.out) : undefined;
  const ploop = c.loop && c.loop !== 'none' ? PRESETS.find((x) => x.cat === 'loop' && x.id === c.loop) : undefined;
  order.forEach((l0, i) => {
    const l = findLayer(t, l0.id)!;
    const { from, to, hold } = shown(t, l);
    const secs = c.secs ?? autoSecs((to - from) / fps);
    let d = Math.max(1, Math.round(secs * fps));
    // The in ends before the On air point, the out starts after it (when the layer is there then).
    const a = from + i * step;
    const inLen = hold > a ? Math.max(1, Math.min(d, hold - a)) : d;
    const e = to - (n - 1 - i) * step;
    const outLen = to > hold ? Math.max(1, Math.min(d, e - Math.max(hold, a + inLen))) : d;
    d = inLen;
    const r = resolveLayer(l, hold, {}, hold);
    const base = (p: Prop) => (typeof r[p] === 'number' ? (r[p] as number) : 0);
    const w = base('width');
    const h = base('height');
    const keys = new Map<Prop, Keyframe[]>();
    const add = (p: Prop, k: Keyframe) => keys.set(p, [...(keys.get(p) ?? []).filter((x) => x.frame !== k.frame), k]);
    if (pin?.move)
      for (const [key, k] of Object.entries(pin.move) as [keyof Move, [number, number]][]) {
        const [props, [v0, v1]] = values(key, k, base, w, h);
        for (const p of props) {
          add(p, { frame: a, value: v0, easing: EASE[pin.ease] });
          add(p, { frame: a + d, value: key.startsWith('crop') || key.startsWith('draw') ? base(p) : v1, easing: { type: 'linear' } });
        }
      }
    if (pout?.move) {
      const c0 = e - outLen;
      for (const [key, k] of Object.entries(pout.move) as [keyof Move, [number, number]][]) {
        const [props, [v0, v1]] = values(key, k, base, w, h);
        for (const p of props) {
          add(p, { frame: c0, value: key.startsWith('crop') || key.startsWith('draw') ? base(p) : v0, easing: EASE[pout.ease] });
          add(p, { frame: e, value: v1, easing: { type: 'linear' } });
        }
      }
    }
    for (const [p, ks] of keys) l.tracks[p] = ks.sort((x, y) => x.frame - y.frame);
    if (ploop?.motion) l.motion = { kind: ploop.motion, period: ploop.period ?? 2, from: pin ? a + d : from };
    else if (c.loop === 'none') delete l.motion;
  });
}

/** The Animations window: the layers it is open for. */
export const useAnimations = create<{ ids: string[] | null }>(() => ({ ids: null }));
export const openAnimations = (ids: string[]) => useAnimations.setState({ ids });

/** Applies from the window (one step of the undo history). */
export function apply(ids: string[], c: Choice, fps: number) {
  useDesigner.getState().update((t) => applyAnimations(t, ids, c, fps));
}

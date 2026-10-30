// Multi-format projects (16:9 + 9:16…): mirror of Template::in_format and
// Project::format_for in crates/cg-core/src/model.rs, plus the designer's
// edit-in-format helpers and the smart adaptation.
import { holdFrame, num, propValue, sortKeys } from './evaluate';
import { MAIN_FORMAT, PROPS, type Format, type Keyframe, type Layer, type LayerOverride, type Project, type Prop, type Template, type Value } from './types';
import { t } from '../i18n';

export function projectFormats(p: Project): Format[] {
  // The main format's name is only shown (formats are found by id).
  return [{ id: MAIN_FORMAT, name: t('Main'), width: p.width, height: p.height }, ...(p.formats ?? [])];
}

/** Format closest in aspect ratio to a width × height output. */
export function formatFor(p: Project, width: number, height: number): Format {
  const target = Math.log(Math.max(1, width) / Math.max(1, height));
  let best = projectFormats(p)[0];
  let bestD = Infinity;
  for (const f of projectFormats(p)) {
    const d = Math.abs(Math.log(Math.max(1, f.width) / Math.max(1, f.height)) - target);
    if (d < bestD - 1e-9) {
      best = f;
      bestD = d;
    }
  }
  return best;
}

export function formatById(p: Project, id: string | null | undefined): Format {
  return projectFormats(p).find((f) => f.id === id) ?? projectFormats(p)[0];
}

/** The template as laid out in `format` (overrides applied). */
export function inFormat(t: Template, format: string | null | undefined): Template {
  if (!format || format === MAIN_FORMAT || !t.layers.some((l) => l.overrides?.[format])) return t;
  return {
    ...t,
    layers: t.layers.map((l) => {
      const o = l.overrides?.[format];
      if (!o) return l;
      const props = { ...l.props };
      const tracks = { ...l.tracks };
      for (const [p, v] of Object.entries(o.props ?? {}) as [Prop, Value][]) {
        props[p] = v;
        delete tracks[p];
      }
      for (const [p, k] of Object.entries(o.tracks ?? {}) as [Prop, Keyframe[]][]) tracks[p] = k;
      return { ...l, props, tracks, visible: o.visible ?? l.visible };
    }),
  };
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Writes an edited format view back into the base template: properties
 * that differ from the main layout become overrides of `format`, everything
 * else (names, texts, structure, markers, fields) is shared.
 */
export function fromFormatView(base: Template, view: Template, format: string): Template {
  const baseById = new Map(base.layers.map((l) => [l.id, l]));
  const layers = view.layers.map((v) => {
    const b = baseById.get(v.id);
    // New layer: belongs to every format.
    if (!b) return v;
    const o: LayerOverride = {};
    for (const p of PROPS) {
      const vTrack = v.tracks[p];
      const bTrack = b.tracks[p];
      if (vTrack?.length) {
        if (!same(vTrack, bTrack)) (o.tracks ??= {})[p] = vTrack;
      } else if (v.props[p] !== undefined && (bTrack?.length || !same(v.props[p], b.props[p]))) {
        (o.props ??= {})[p] = v.props[p];
      }
    }
    if (v.visible !== b.visible) o.visible = v.visible;
    const overrides = { ...(b.overrides ?? {}) };
    if (o.props || o.tracks || o.visible !== undefined) overrides[format] = o;
    else delete overrides[format];
    // Shared fields from the view, layout from the base.
    return { ...v, props: b.props, tracks: b.tracks, visible: b.visible, overrides: Object.keys(overrides).length ? overrides : undefined };
  });
  return { ...view, layers };
}

/** Removes a property (or all) from a layer's override in `format`. */
export function resetOverride(l: Layer, format: string, prop?: Prop) {
  const o = l.overrides?.[format];
  if (!o) return;
  if (!prop) delete l.overrides![format];
  else {
    delete o.props?.[prop];
    delete o.tracks?.[prop];
    if (!Object.keys(o.props ?? {}).length && !Object.keys(o.tracks ?? {}).length && o.visible === undefined) delete l.overrides![format];
  }
  if (l.overrides && !Object.keys(l.overrides).length) delete l.overrides;
}

export function isOverridden(l: Layer, format: string, prop: Prop): boolean {
  const o = l.overrides?.[format];
  return !!o && (o.props?.[prop] !== undefined || o.tracks?.[prop] !== undefined);
}

// ---------------------------------------------------------------------------
// Smart adaptation: a first layout of every element in a new format.
//
// - The short side sets the scale of the graphics (a 1080-wide vertical
//   keeps the text size of a 1080-high landscape), capped by the safe area.
// - Each element keeps its anchoring: corners, edges or centre, with its
//   distance to the edges scaled the same way.
// - Full-width bars (tickers, straps, backgrounds) stretch to the new width
//   and their content follows (left / centre / right of the bar).
// - Animations are kept: tracks are moved (and scaled) with the element.

type Box = { x: number; y: number; w: number; h: number; sx: number; sy: number; ax: number; ay: number };

const at = (l: Layer, p: Prop, f: number) => num(propValue(l, p, f));

function box(l: Layer, f: number): Box {
  return { x: at(l, 'x', f), y: at(l, 'y', f), w: at(l, 'width', f), h: at(l, 'height', f), sx: at(l, 'scaleX', f), sy: at(l, 'scaleY', f), ax: at(l, 'anchorX', f), ay: at(l, 'anchorY', f) };
}

/** Shifts (and optionally scales around 0) a property: static value or every keyframe. */
function moveProp(l: Layer, o: LayerOverride, p: Prop, map: (v: number) => number) {
  const track = l.tracks[p];
  if (track?.length) (o.tracks ??= {})[p] = sortKeys(track).map((k) => ({ ...k, value: map(num(k.value)) }));
  else (o.props ??= {})[p] = map(num(propValue(l, p, 0)));
}

/** 0 = start, 1 = centre, 2 = end: the side with the smaller gap, centre
 *  when both gaps are about the same. */
function anchorOf(start: number, size: number, total: number): 0 | 1 | 2 {
  const before = start;
  const after = total - (start + size);
  if (Math.abs(before - after) < total * 0.08) return 1;
  return before < after ? 0 : 2;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), Math.max(lo, hi));

/** Stretches a full-width element (and its content) from oldW to newW. */
function stretch(t: Template, l: Layer, f: number, oldW: number, newW: number, out: Map<string, LayerOverride>) {
  const o = out.get(l.id) ?? {};
  const k = newW / Math.max(1, oldW);
  moveProp(l, o, 'width', (v) => v * k);
  out.set(l.id, o);
  for (const c of t.layers.filter((x) => x.parent === l.id)) {
    const b = box(c, f);
    if (b.w * b.sx >= 0.8 * oldW) {
      // Full-width child: stretches too, its left margin scales.
      const co = out.get(c.id) ?? {};
      moveProp(c, co, 'x', (v) => v * k);
      out.set(c.id, co);
      stretch(t, c, f, b.w, b.w * k, out);
    } else {
      // Stays at its place relative to the left, centre or right of the bar.
      const pos = anchorOf(b.x, b.w * b.sx, oldW);
      const dx = pos === 0 ? 0 : pos === 1 ? (newW - oldW) / 2 : newW - oldW;
      if (dx) {
        const co = out.get(c.id) ?? {};
        moveProp(c, co, 'x', (v) => v + dx);
        out.set(c.id, co);
      }
    }
  }
}

/** Generates the overrides of `to` for every layer of a template. */
export function adaptTemplate(t: Template, from: { width: number; height: number }, to: Format): Template {
  const f = holdFrame(t);
  const W1 = from.width;
  const H1 = from.height;
  const W2 = to.width;
  const H2 = to.height;
  const m = Math.min(W2, H2) / Math.min(W1, H1);
  const out = new Map<string, LayerOverride>();
  for (const l of t.layers.filter((x) => !x.parent)) {
    const b = box(l, f);
    // Visual box (scale applies around the anchor).
    const vw = b.w * b.sx;
    const vh = b.h * b.sy;
    const vx = b.x + b.ax * b.w * (1 - b.sx);
    const vy = b.y + b.ay * b.h * (1 - b.sy);
    const o: LayerOverride = {};
    out.set(l.id, o);
    const fullW = vw >= 0.8 * W1;
    const fullH = vh >= 0.8 * H1;
    if (fullW && fullH) {
      // Backgrounds / full frames: cover the new frame.
      moveProp(l, o, 'x', (v) => (v * W2) / W1);
      moveProp(l, o, 'y', (v) => (v * H2) / H1);
      moveProp(l, o, 'height', (v) => (v * H2) / H1);
      stretch(t, l, f, b.w, (b.w * W2) / W1, out);
      continue;
    }
    // Uniform scale: short side, then fit the safe area (90 %).
    let s = m;
    if (!fullW) s = Math.min(s, (0.9 * W2) / Math.max(1, vw));
    s = Math.min(s, (0.9 * H2) / Math.max(1, vh));
    let newVw = vw * s;
    const newVh = vh * s;
    let newVx: number;
    if (fullW) {
      // Bars: full new width, same side margins (scaled).
      newVx = vx * m;
      stretch(t, l, f, b.w, (W2 - 2 * newVx) / Math.max(1e-6, s * b.sx), out);
      newVw = W2 - 2 * newVx;
    } else {
      const h = anchorOf(vx, vw, W1);
      newVx = h === 0 ? vx * m : h === 2 ? W2 - (W1 - (vx + vw)) * m - newVw : W2 / 2 + ((vx + vw / 2 - W1 / 2) * W2) / W1 - newVw / 2;
      // Stay inside the frame.
      newVx = clamp(newVx, 0, W2 - newVw);
    }
    const v = anchorOf(vy, vh, H1);
    let newVy = v === 0 ? vy * m : v === 2 ? H2 - (H1 - (vy + vh)) * m - newVh : H2 / 2 + ((vy + vh / 2 - H1 / 2) * H2) / H1 - newVh / 2;
    newVy = clamp(newVy, 0, H2 - newVh);
    // Back to the layer's own x / y (anchor-relative scaling).
    const newW = fullW ? newVw / Math.max(1e-6, s * b.sx) : b.w;
    const x = newVx - b.ax * newW * (1 - b.sx * s);
    const y = newVy - b.ay * b.h * (1 - b.sy * s);
    const dx = x - b.x;
    const dy = y - b.y;
    moveProp(l, o, 'x', (val) => val + dx);
    moveProp(l, o, 'y', (val) => val + dy);
    if (Math.abs(s - 1) > 1e-3) {
      moveProp(l, o, 'scaleX', (val) => val * s);
      moveProp(l, o, 'scaleY', (val) => val * s);
    }
  }
  return {
    ...t,
    layers: t.layers.map((l) => {
      const o = out.get(l.id);
      const overrides = { ...(l.overrides ?? {}) };
      if (o && (o.props || o.tracks)) overrides[to.id] = o;
      else delete overrides[to.id];
      return { ...l, overrides: Object.keys(overrides).length ? overrides : undefined };
    }),
  };
}

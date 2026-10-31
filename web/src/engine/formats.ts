// Multi-format projects (16:9 + 9:16…): mirror of Template::in_format and
// Project::format_for in crates/strata-mv-core/src/model.rs, plus the designer's
// edit-in-format helpers and the smart adaptation.
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


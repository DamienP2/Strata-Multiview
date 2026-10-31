// A multiview's background (mirror of strata_mv_core::model::Backdrop): a locked rectangle under
// everything, of the size of the picture, filled with a colour, a gradient or a picture.
import type { Layer, Template } from './types';
import type { FillStyle } from '../widgets/shape';

export interface Backdrop {
  /** `#rrggbb`: the colour, the first of a gradient. */
  color?: string;
  fillStyle?: FillStyle;
}

export const BACKDROP_ID = '__backdrop';
/** None set: a very dark grey. */
export const BACKDROP_COLOR = '#141416';

const channel = (hex: string, i: number) => {
  const v = parseInt(hex.slice(i, i + 2), 16);
  return Number.isFinite(v) ? v / 255 : 0x14 / 255;
};

/** The layer drawing a background over `width` × `height`. */
export function backdropLayer(b: Backdrop | undefined, width: number, height: number): Layer {
  const hex = (b?.color || BACKDROP_COLOR).trim().replace(/^#/, '');
  return {
    id: BACKDROP_ID,
    name: 'Background',
    parent: null,
    type: 'rect',
    visible: true,
    locked: true,
    tracks: {},
    bindings: {},
    fillStyle: { ...(b?.fillStyle ?? {}) },
    props: { x: 0, y: 0, width, height, fill: [channel(hex, 0), channel(hex, 2), channel(hex, 4), 1] },
  } as unknown as Layer;
}

/** The template with its background as its first layer (drawn `width` × `height`). */
export function withBackdrop(t: Template, width: number, height: number): Template {
  return { ...t, layers: [backdropLayer(t.background, width, height), ...t.layers.filter((l) => l.id !== BACKDROP_ID)] };
}

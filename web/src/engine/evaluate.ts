// Animation evaluation — mirror of crates/cg-core/src/{easing,anim}.rs.
import { COLOR_PROPS, PROPS, type Color, type Data, type Easing, type Keyframe, type Layer, type Prop, type Template, type TextTransform, type Value } from './types';

export function cubicBezier(x1: number, y1: number, x2: number, y2: number, x: number): number {
  x1 = Math.min(1, Math.max(0, x1));
  x2 = Math.min(1, Math.max(0, x2));
  const bez = (a: number, b: number, t: number) => {
    const mt = 1 - t;
    return 3 * mt * mt * t * a + 3 * mt * t * t * b + t * t * t;
  };
  const dbez = (a: number, b: number, t: number) => {
    const mt = 1 - t;
    return 3 * mt * mt * a + 6 * mt * t * (b - a) + 3 * t * t * (1 - b);
  };
  let t = x;
  for (let i = 0; i < 8; i++) {
    const err = bez(x1, x2, t) - x;
    if (Math.abs(err) < 1e-7) return bez(y1, y2, t);
    const d = dbez(x1, x2, t);
    if (Math.abs(d) < 1e-6) break;
    t -= err / d;
  }
  let lo = 0;
  let hi = 1;
  t = x;
  for (let i = 0; i < 40; i++) {
    const v = bez(x1, x2, t);
    if (Math.abs(v - x) < 1e-7) break;
    if (v < x) lo = t;
    else hi = t;
    t = (lo + hi) / 2;
  }
  return bez(y1, y2, t);
}

function bounceOut(t: number): number {
  const n1 = 7.5625;
  const d1 = 2.75;
  if (t < 1 / d1) return n1 * t * t;
  if (t < 2 / d1) return n1 * (t -= 1.5 / d1) * t + 0.75;
  if (t < 2.5 / d1) return n1 * (t -= 2.25 / d1) * t + 0.9375;
  return n1 * (t -= 2.625 / d1) * t + 0.984375;
}

export function applyEasing(e: Easing, t: number): number {
  t = Math.min(1, Math.max(0, t));
  switch (e.type) {
    case 'linear':
      return t;
    case 'hold':
      return 0;
    case 'easeIn':
      return cubicBezier(0.42, 0, 1, 1, t);
    case 'easeOut':
      return cubicBezier(0, 0, 0.58, 1, t);
    case 'easeInOut':
      return cubicBezier(0.42, 0, 0.58, 1, t);
    case 'bezier':
      return cubicBezier(e.x1, e.y1, e.x2, e.y2, t);
    case 'back': {
      const c1 = 1.70158;
      const c3 = c1 + 1;
      return 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2);
    }
    case 'elastic': {
      if (t === 0 || t === 1) return t;
      const c4 = (2 * Math.PI) / 3;
      return Math.pow(2, -10 * t) * Math.sin((t * 10 - 0.75) * c4) + 1;
    }
    case 'bounce':
      return bounceOut(t);
  }
}

export const DEFAULTS: Record<Prop, Value> = {
  x: 0,
  y: 0,
  width: 400,
  height: 100,
  anchorX: 0.5,
  anchorY: 0.5,
  rotation: 0,
  scaleX: 1,
  scaleY: 1,
  opacity: 1,
  fill: [1, 1, 1, 1],
  stroke: [0, 0, 0, 0],
  strokeWidth: 0,
  cornerRadius: 0,
  fontSize: 48,
  letterSpacing: 0,
  cropLeft: 0,
  cropRight: 0,
  cropTop: 0,
  cropBottom: 0,
  arcStart: 0,
  arcEnd: 360,
  innerRadius: 0,
  // Drop shadow (Layer.dropShadow), as the Text widget's own: #000000aa, 4 px down, 12 px blur.
  shadowColor: [0, 0, 0, 170 / 255],
  shadowX: 0,
  shadowY: 4,
  shadowBlur: 12,
  // Pen: visible portion of the path, % of its length.
  drawStart: 0,
  drawEnd: 100,
  // Rectangle « Per corner »: top left, top right, bottom right, bottom left radius.
  cornerTl: 0,
  cornerTr: 0,
  cornerBr: 0,
  cornerBl: 0,
  // Second colour of a gradient (shapes, Text).
  fill2: [1, 59 / 255, 92 / 255, 1],
};

/** Mirror of anim::layer_default: a property neither set nor animated; an ellipse's arc is its settings (older layers have them only there). */
export function layerDefault(layer: Layer, p: Prop): Value {
  if (layer.type === 'ellipse') {
    if (p === 'arcStart') return layer.start ?? DEFAULTS.arcStart;
    if (p === 'arcEnd') return layer.end ?? DEFAULTS.arcEnd;
    if (p === 'innerRadius') return layer.inner ?? DEFAULTS.innerRadius;
  }
  // A pen's visible portion: its settings (older layers have it only there).
  if (layer.type === 'pen') {
    if (p === 'drawStart') return layer.vStart ?? DEFAULTS.drawStart;
    if (p === 'drawEnd') return layer.vEnd ?? DEFAULTS.drawEnd;
  }
  // A rectangle's corners and a gradient's second colour: their settings (older layers have them only there).
  const ci = ['cornerTl', 'cornerTr', 'cornerBr', 'cornerBl'].indexOf(p);
  if (ci >= 0 && layer.type === 'rect') return layer.corners?.[ci] ?? 0;
  if (p === 'fill2') {
    const c2 = layer.type === 'text' ? layer.color2 ?? '#a594ff' : ['rect', 'ellipse', 'polygon', 'pen'].includes(layer.type) ? ((layer as { fillStyle?: { color2?: string } }).fillStyle?.color2 ?? '#ff3b5c') : undefined;
    if (c2 !== undefined) return parseHex(c2) ?? DEFAULTS.fill2;
  }
  return DEFAULTS[p];
}

function lerp(a: Value, b: Value, t: number): Value {
  if (typeof a === 'number' && typeof b === 'number') return a + (b - a) * t;
  if (Array.isArray(a) && Array.isArray(b)) return a.map((v, i) => v + (b[i] - v) * t) as Color;
  return t < 1 ? a : b;
}

export function sortKeys(track: Keyframe[]): Keyframe[] {
  return [...track].sort((a, b) => a.frame - b.frame);
}

export function sampleTrack(track: Keyframe[] | undefined, frame: number): Value | undefined {
  if (!track || track.length === 0) return undefined;
  const sorted = sortKeys(track);
  if (frame <= sorted[0].frame) return sorted[0].value;
  for (let i = 0; i < sorted.length - 1; i++) {
    const k0 = sorted[i];
    const k1 = sorted[i + 1];
    if (frame >= k0.frame && frame < k1.frame) {
      const span = Math.max(1e-9, k1.frame - k0.frame);
      return lerp(k0.value, k1.value, applyEasing(k0.easing, (frame - k0.frame) / span));
    }
  }
  return sorted[sorted.length - 1].value;
}

/** Mirror of TextTransform::apply. */
export function transformText(t: string, transform?: TextTransform): string {
  switch (transform) {
    case 'upper':
      return t.toUpperCase();
    case 'lower':
      return t.toLowerCase();
    case 'capitalize': {
      let start = true;
      let out = '';
      for (const c of t) {
        if (start && c.toLowerCase() !== c.toUpperCase()) {
          out += c.toUpperCase();
          start = false;
        } else {
          out += c;
          if (/\s/.test(c) || c === '-') start = true;
        }
      }
      return out;
    }
    default:
      return t;
  }
}

export function interpolateText(text: string, data: Data): string {
  return text.replace(/\{\{([^}]*)\}\}/g, (_, key: string) => data[key.trim()] ?? '');
}

export function parseHex(s: string): Color | undefined {
  const h = s.trim().replace(/^#/, '');
  const p = (i: number, l: number) => parseInt(h.slice(i, i + l), 16);
  let rgba: number[];
  if (h.length === 3) rgba = [p(0, 1) * 17, p(1, 1) * 17, p(2, 1) * 17, 255];
  else if (h.length === 6) rgba = [p(0, 2), p(2, 2), p(4, 2), 255];
  else if (h.length === 8) rgba = [p(0, 2), p(2, 2), p(4, 2), p(6, 2)];
  else return undefined;
  if (rgba.some((v) => Number.isNaN(v))) return undefined;
  return rgba.map((v) => v / 255) as Color;
}

export function toHex(c: Color, withAlpha = false): string {
  const h = (v: number) =>
    Math.round(Math.min(1, Math.max(0, v)) * 255)
      .toString(16)
      .padStart(2, '0');
  return `#${h(c[0])}${h(c[1])}${h(c[2])}${withAlpha ? h(c[3]) : ''}`;
}

export function cssColor(c: Color): string {
  const v = (x: number) => Math.round(Math.min(1, Math.max(0, x)) * 255);
  return `rgba(${v(c[0])},${v(c[1])},${v(c[2])},${Math.min(1, Math.max(0, c[3]))})`;
}

export type Resolved = Record<Prop, Value>;

/** Mirror of anim::resolve_layer: a value bound to an animated number sets
 * what is seen at the hold frame; the animation moves relative to it. */
export function resolveLayer(layer: Layer, frame: number, data: Data, hold = 0): Resolved {
  const out = {} as Resolved;
  for (const p of PROPS) {
    const key = layer.bindings?.[p];
    let bound: Value | undefined;
    if (key && data[key] !== undefined) {
      const raw = data[key].trim();
      bound = COLOR_PROPS.has(p) ? parseHex(raw) : raw === 'true' ? 1 : raw === 'false' ? 0 : Number.isFinite(parseFloat(raw)) ? parseFloat(raw) : undefined;
    }
    const track = layer.tracks?.[p];
    const animated = sampleTrack(track, frame);
    if (typeof bound === 'number' && typeof animated === 'number') {
      const atHold = sampleTrack(track, hold);
      out[p] = animated + bound - (typeof atHold === 'number' ? atHold : animated);
    } else out[p] = bound ?? animated ?? layer.props?.[p] ?? layerDefault(layer, p);
  }
  return out;
}

/** Static or animated value of a property, without bindings (for editors). */
export function propValue(layer: Layer, prop: Prop, frame: number): Value {
  return sampleTrack(layer.tracks?.[prop], frame) ?? layer.props?.[prop] ?? layerDefault(layer, prop);
}

export const num = (v: Value) => (typeof v === 'number' ? v : 0);
export const col = (v: Value): Color => (Array.isArray(v) ? v : [0, 0, 0, 0]);

export function defaultData(t: Template): Data {
  return Object.fromEntries(t.fields.map((f) => [f.key, f.default]));
}

export function outFrame(t: Template): number | undefined {
  return t.markers.find((m) => m.kind === 'out')?.frame;
}

/** Where playback stops next after `frame`: a pause, the On air point, else the end. */
export function nextStopAfter(t: Template, frame: number): number {
  const stops = t.markers.filter((m) => (m.kind === 'pause' || m.kind === 'out') && m.frame > frame + 1e-6).map((m) => m.frame);
  return Math.min(t.duration, ...stops);
}

/** The On air frame (what the graphic looks like on air), for previews and thumbnails. */
export function holdFrame(t: Template): number {
  return outFrame(t) ?? t.duration;
}

export function newId(): string {
  return Math.random().toString(36).slice(2, 10) + Date.now().toString(36).slice(-4);
}

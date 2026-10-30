// Ellipse: portion (sector / arc / chord) between a start and an end angle, ring / donut (a circle: the Dimensions lock).
// Drawn as a path in layer pixels (never a scaled circle), so a flat ellipse keeps an even stroke.
// Same picture as crates/cg-render/src/wid/ellipse.rs; geometry mirrors crates/cg-core/src/wid/ellipse.rs.
// Start, End and Inner radius are animatable properties (arcStart, arcEnd, innerRadius); unset, the
// settings below give them (older layers).
import { Circle } from 'lucide-react';
import { col, cssColor, num, propValue, type Resolved } from '../engine/evaluate';
import type { Layer } from '../engine/types';
import { GroupRow } from '../designer/Inspector';
import { setProp, useDesigner } from '../designer/designerStore';
import { Bound, Btns, clamp, defineWidget, Handle, Section, Seg, useBound, type Warning, withDefaults } from './kit';
import { FillStrokeSections, ShapeSvg, StrokeRows, type FillStyle, type ShapeEdit, type StrokeStyle } from './shape';
import { t, tc } from '../i18n';

export interface Config {
  /** Older ellipses: width = height. Unused: the server makes it the layer's Dimensions lock (lockRatio). */
  circle?: boolean;
  /** Degrees, 0 = 12 o'clock, clockwise. */
  start?: number;
  /** Past 360 wraps round; 360° of sweep = full ellipse. */
  end?: number;
  portion?: 'sector' | 'arc' | 'chord';
  /** Sector: inner radius, % (ring / donut). */
  inner?: number;
  fillStyle?: FillStyle;
  /** Older ellipses (none set) have a centred stroke. */
  strokeStyle?: StrokeStyle;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface State {}

const D = { start: 0, end: 360, portion: 'sector' as const, inner: 0 };
const cfg = (c: Config) => withDefaults(D, c);
/** Older ellipses: a centred stroke (also when only some of its style is set, as Rust reads it). */
const strokeOf = (c: Config): StrokeStyle => ({ position: 'center', ...c.strokeStyle });
const DEG = Math.PI / 180;
const r3 = (v: number) => Math.round(v * 1000) / 1000;

/** The portion's angles and inner radius (mirrors Span). */
export interface Span {
  start: number;
  end: number;
  inner: number;
}

/** The span from the settings. */
export const spanOf = (c: Config): Span => {
  const k = cfg(c);
  return { start: k.start, end: k.end, inner: k.inner };
};

/** The span at a frame: the animated properties (resolved values). */
export const spanAt = (r: Resolved): Span => ({ start: num(r.arcStart), end: num(r.arcEnd), inner: num(r.innerRadius) });

/** The span of a layer at a frame of the Designer (no data). */
const spanOfLayer = (l: Layer, frame: number): Span => ({ start: num(propValue(l, 'arcStart', frame)), end: num(propValue(l, 'arcEnd', frame)), inner: num(propValue(l, 'innerRadius', frame)) });

/** Sweep in degrees: 0 = nothing, 360 = full ellipse. */
export function sweep(s: Span) {
  let d = s.end - s.start;
  if (d < 0) d += 360;
  return clamp(d, 0, 360);
}

const pt = (cx: number, cy: number, rx: number, ry: number, a: number): [number, number] => [cx + Math.sin(a * DEG) * rx, cy - Math.cos(a * DEG) * ry];

/** Cubic Béziers of an elliptical arc (mirrors arc_to). */
function arcTo(cx: number, cy: number, rx: number, ry: number, a0: number, a1: number) {
  const n = Math.max(1, Math.ceil(Math.abs(a1 - a0) / 90));
  const step = (a1 - a0) / n;
  const k = (4 / 3) * Math.tan((step * DEG) / 4);
  let s = '';
  for (let i = 0; i < n; i++) {
    const a = (a0 + step * i) * DEG;
    const b = (a0 + step * (i + 1)) * DEG;
    const p0 = [cx + Math.sin(a) * rx, cy - Math.cos(a) * ry];
    const p1 = [cx + Math.sin(b) * rx, cy - Math.cos(b) * ry];
    const d0 = [Math.cos(a) * rx, Math.sin(a) * ry];
    const d1 = [Math.cos(b) * rx, Math.sin(b) * ry];
    s += `C${r3(p0[0] + d0[0] * k)} ${r3(p0[1] + d0[1] * k)} ${r3(p1[0] - d1[0] * k)} ${r3(p1[1] - d1[1] * k)} ${r3(p1[0])} ${r3(p1[1])}`;
  }
  return s;
}

/** The outline and whether it is closed; null = nothing to draw (mirrors outline_span). */
export function outline(c: Config, w: number, h: number, span: Span = spanOf(c)): { d: string; closed: boolean } | null {
  const k = cfg(c);
  const a0 = span.start;
  const [cx, cy, rx, ry] = [w / 2, h / 2, w / 2, h / 2];
  const sw = sweep(span);
  if (!Number.isFinite(sw) || sw <= 0.01 || rx <= 0 || ry <= 0) return null;
  const q = clamp(span.inner, 0, 100) / 100;
  const ring = k.portion === 'sector' && q > 0;
  if (ring && q >= 1) return null;
  const M = (p: [number, number]) => `M${r3(p[0])} ${r3(p[1])}`;
  const a = pt(cx, cy, rx, ry, a0);
  if (sw >= 360) {
    let d = M(a) + arcTo(cx, cy, rx, ry, a0, a0 + 360) + 'Z';
    if (ring) d += M(pt(cx, cy, rx * q, ry * q, a0)) + arcTo(cx, cy, rx * q, ry * q, a0, a0 - 360) + 'Z';
    return { d, closed: true };
  }
  const end = a0 + sw;
  const arc = arcTo(cx, cy, rx, ry, a0, end);
  if (k.portion === 'arc') return { d: M(a) + arc, closed: false };
  if (k.portion === 'chord') return { d: M(a) + arc + 'Z', closed: true };
  if (ring) {
    const ib = pt(cx, cy, rx * q, ry * q, end);
    return { d: M(a) + arc + `L${r3(ib[0])} ${r3(ib[1])}` + arcTo(cx, cy, rx * q, ry * q, end, a0) + 'Z', closed: true };
  }
  return { d: `M${cx} ${cy}L${r3(a[0])} ${r3(a[1])}` + arc + 'Z', closed: true };
}

/** Stroke group of an arc: colour, width, style and ends (the stroke is centred, always on). */
function ArcStroke({ layer }: { layer: Parameters<typeof FillStrokeSections>[0]['layer'] }) {
  return (
    <Section id="ellipse-arc-stroke" title={t('Stroke')}>
      <StrokeRows layer={layer} caps joins={false} />
    </Section>
  );
}

/** Fill / Stroke groups showing the centred stroke of older ellipses; Joins only when the outline has corners (a part, not the full ellipse or ring). */
function Look({ layer, edit, corners }: { layer: Parameters<typeof FillStrokeSections>[0]['layer'] & Config; edit: ShapeEdit; corners: boolean }) {
  const b = useBound();
  return (
    <Bound obj={{ ...layer, strokeStyle: strokeOf(layer) }} set={b.set}>
      <FillStrokeSections layer={layer} id="ellipse" edit={edit} joins={corners} />
    </Bound>
  );
}

export default defineWidget({
  id: 'ellipse',
  type: 'ellipse',
  label: 'Ellipse',
  group: 'Shapes & tools',
  icon: Circle,
  create: () => ({ lockRatio: true, strokeStyle: { position: 'inside' }, props: { width: 600, height: 600 } }),
  ownsLook: true,
  Render: ({ layer, r, w, h, data }) => {
    const p = outline(layer, w, h, spanAt(r));
    if (!p) return null;
    const st = { ...strokeOf(layer) };
    if (!p.closed) {
      const sw = num(r.strokeWidth);
      if (sw <= 0) return null;
      const dash = st.dash === 'dashed' ? `${sw * 3} ${sw * 2}` : st.dash === 'dotted' ? `0.001 ${sw * 2}` : undefined;
      return (
        <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} style={{ position: 'absolute', inset: 0, overflow: 'visible' }}>
          <path d={p.d} fill="none" stroke={cssColor(col(r.stroke))} strokeWidth={sw} strokeLinecap={st.dash === 'dotted' ? 'round' : (st.cap ?? 'butt')} strokeLinejoin={st.join ?? 'miter'} strokeDasharray={dash} />
        </svg>
      );
    }
    return <ShapeSvg d={p.d} w={w} h={h} r={r} fill={layer.fillStyle} stroke={st} rule="evenodd" data={data} />;
  },
  Inspector: ({ layer, edit }) => {
    const k = cfg(layer);
    const frame = useDesigner((s) => s.frame);
    const quick = (start: number, end: number) =>
      edit((l) => {
        setProp(l as Layer, 'arcStart', start, frame);
        setProp(l as Layer, 'arcEnd', end, frame);
      });
    return (
      <>
        <Section id="ellipse-shape" title={t('Shape')}>
          {/* Animatable (◆ puts them on the timeline). */}
          <GroupRow layer={layer as Layer} spec={{ label: t('Start'), props: ['arcStart'], min: 0, max: 360, suffix: '°' }} />
          <GroupRow layer={layer as Layer} spec={{ label: t('End'), props: ['arcEnd'], min: 0, max: 720, suffix: '°' }} />
          <Btns
            label={tc('ellipse', 'Quick')}
            list={[
              { label: t('Full'), cls: 'ghost', onClick: () => quick(0, 360) },
              { label: t('Half'), cls: 'ghost', onClick: () => quick(270, 450) },
              { label: t('Quarter'), cls: 'ghost', onClick: () => quick(0, 90) },
            ]}
          />
          <Seg
            expose="portion"
            exposeValue={k.portion}
            label={t('Portion')}
            value={k.portion}
            onChange={(v) =>
              edit((l) => {
                l.portion = v;
                if (v === 'arc' && !num(l.props.strokeWidth ?? 0)) {
                  l.props.strokeWidth = 28;
                  if (col(l.props.stroke ?? [0, 0, 0, 0])[3] === 0) l.props.stroke = [1, 1, 1, 1];
                }
              })
            }
            options={[
              ['sector', t('Sector')],
              ['arc', t('Arc')],
              ['chord', t('Chord')],
            ]}
          />
          {k.portion === 'sector' && <GroupRow layer={layer as Layer} spec={{ label: t('Inner radius'), props: ['innerRadius'], min: 0, max: 100, suffix: '%' }} />}
        </Section>
        {k.portion === 'arc' ? <ArcStroke layer={layer} /> : <Look layer={layer} edit={edit} corners={sweep(spanOfLayer(layer as Layer, frame)) < 360 || !!(layer.tracks.arcStart?.length || layer.tracks.arcEnd?.length)} />}
      </>
    );
  },
  Overlay: ({ layer, w, h, scale, edit }) => {
    const k = cfg(layer);
    // The handles move the values at the playhead (a keyframe when animated).
    const frame = useDesigner((s) => s.frame);
    const sp = spanOfLayer(layer as Layer, frame);
    const sw = sweep(sp);
    const q = clamp(sp.inner, 0, 100) / 100;
    const ang = (x: number, y: number) => Math.round((Math.atan2((x - w / 2) / (w / 2), -(y - h / 2) / (h / 2)) / DEG + 360) % 360);
    const a = pt(w / 2, h / 2, w / 2, h / 2, sp.start);
    const b = pt(w / 2, h / 2, w / 2, h / 2, sp.start + sw);
    const m = pt(w / 2, h / 2, (w / 2) * q, (h / 2) * q, sp.start + sw / 2);
    const apart = sw >= 360 || sw <= 0.01 ? 22 / scale : 0;
    return (
      <>
        <Handle x={a[0]} y={a[1]} scale={scale} label={t('Start {angle}°', { angle: Math.round(sp.start) })} onDrag={(x, y) => edit((l) => setProp(l as Layer, 'arcStart', ang(x, y), frame), `estart-${layer.id}`)} />
        <Handle
          x={b[0] + apart}
          y={b[1]}
          scale={scale}
          label={t('End {angle}° · sweep {sweep}°', { angle: Math.round(sp.end), sweep: Math.round(sw) })}
          onDrag={(x, y) =>
            edit((l) => {
              let e = ang(x, y);
              const s = num(propValue(l as Layer, 'arcStart', frame));
              if (e <= s) e += 360; // the end always sweeps clockwise from the start
              setProp(l as Layer, 'arcEnd', e, frame);
            }, `eend-${layer.id}`)
          }
        />
        {k.portion === 'sector' && (
          <Handle
            x={m[0]}
            y={m[1]}
            scale={scale}
            label={t('Inner radius {pct} %', { pct: Math.round(sp.inner) })}
            onDrag={(x, y) => {
              const dx = (x - w / 2) / (w / 2);
              const dy = (y - h / 2) / (h / 2);
              edit((l) => setProp(l as Layer, 'innerRadius', Math.round(clamp(Math.hypot(dx, dy) * 100, 0, 100)), frame), `einner-${layer.id}`);
            }}
          />
        )}
      </>
    );
  },
  warnings: (layer, { r }) => {
    const k = cfg(layer);
    const out: Warning[] = [];
    const sp = r ? spanAt(r) : spanOf(layer);
    const sw = sweep(sp);
    const w = num(r?.width ?? layer.props.width ?? 0);
    const h = num(r?.height ?? layer.props.height ?? 0);
    const width = num(r?.strokeWidth ?? layer.props.strokeWidth ?? 0);
    if (sw <= 0.01) out.push({ level: 'info', text: t('Start and end are equal (0° of sweep): nothing is drawn.') });
    if (k.portion === 'sector' && sp.inner >= 100 && sw > 0.01) out.push({ level: 'warn', text: t('Inner radius at 100 %: the ring has no thickness, the shape is invisible.') });
    if (k.portion !== 'arc' && layer.fillStyle?.kind === 'none' && !width) out.push({ level: 'warn', text: t('No fill and no stroke: the shape is invisible.') });
    const flat = Math.min(w, h) / Math.max(w, h, 1);
    if (flat < 0.12 && (width || k.portion === 'arc')) out.push({ level: 'ok', text: t('Very flat ellipse: the outline is computed in stage pixels, the stroke keeps the same thickness all round.') });
    return out;
  },
});

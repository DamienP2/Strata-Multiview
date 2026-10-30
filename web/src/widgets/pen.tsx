// Pen: a Bézier path edited on the stage; points and handles are fractions of the box.
// The visible portion (start / end %, the animatable drawStart / drawEnd) is what the In draws,
// over the pen's own duration after its delay; arrows follow the visible tip.
// Same picture as crates/cg-render/src/wid/pen.rs; geometry mirrors crates/cg-core/src/wid/pen.rs.
import { useEffect, useState, useSyncExternalStore } from 'react';
import { PenTool } from 'lucide-react';
import { col, cssColor, holdFrame, num, propValue } from '../engine/evaluate';
import { AssetPicker, ColorGroupRow, GroupRow } from '../designer/Inspector';
import type { Layer } from '../engine/types';
import { useApp } from '../store';
import { Btns, Catcher, clamp, defineWidget, getPath, Handle, Num, Row, Section, Seg, Toggle, useBound, type Warning, withDefaults } from './kit';
import { FILL, GradientTo, ShapeSvg, STROKE, StrokeRows, type FillRule, type FillStyle, type StrokeStyle } from './shape';
import { t } from '../i18n';

export interface PenPoint {
  x: number;
  y: number;
  ix?: number;
  iy?: number;
  ox?: number;
  oy?: number;
  type?: 'corner' | 'curve';
  mode?: 'symmetric' | 'asymmetric' | 'broken';
}
export type Arrow = 'none' | 'triangle' | 'open' | 'diamond' | 'circle' | 'bar';

export interface Config {
  points?: PenPoint[];
  closed?: boolean;
  strokeStyle?: StrokeStyle;
  fixedWidth?: boolean;
  refW?: number;
  refH?: number;
  arrowStart?: Arrow;
  arrowEnd?: Arrow;
  arrowSize?: number;
  fillOn?: boolean;
  fillStyle?: FillStyle;
  fillRule?: FillRule;
  /** Visible portion, %: the defaults of the animatable drawStart / drawEnd (older layers have only these). */
  vStart?: number;
  vEnd?: number;
  drawIn?: boolean;
  /** How long the drawing lasts, s (unset = the template's In, 1 s without one). */
  drawDuration?: number;
  /** Time on air before it starts drawing, s. */
  drawDelay?: number;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface State {}

type Pt = [number, number];
const P = (x: number, y: number, o: Partial<PenPoint> = {}): Required<PenPoint> => ({ x, y, ix: 0, iy: 0, ox: 0, oy: 0, type: 'corner', mode: 'symmetric', ...o });
const DEFAULT_PTS = () => [P(0.05, 0.85, { type: 'curve', ox: 0.1, oy: -0.55 }), P(0.5, 0.5, { type: 'curve', ix: -0.14, iy: -0.2, ox: 0.14, oy: 0.2 }), P(0.94, 0.14, { type: 'curve', ix: -0.1, iy: 0.55 })];
const D = { closed: false, fixedWidth: true, refW: 1100, refH: 560, arrowStart: 'none' as Arrow, arrowEnd: 'triangle' as Arrow, arrowSize: 3.5, fillOn: false, fillRule: 'nonzero' as FillRule, vStart: 0, vEnd: 100, drawIn: true, drawDelay: 0 };
const cfg = (c: Config) => ({ ...withDefaults(D, c), points: (c.points ?? DEFAULT_PTS()).map((p) => ({ ...P(0, 0), ...p })) });
const strokeOf = (c: Config) => ({ ...STROKE, cap: 'round' as const, join: 'round' as const, ...c.strokeStyle });
const f4 = (v: number) => +v.toFixed(4);
const r2 = (v: number) => Math.round(v * 100) / 100;

/** Cubic segments in layer px (mirrors segments). */
function segments(c: Config, w: number, h: number): [Pt, Pt, Pt, Pt][] {
  const k = cfg(c);
  const ps = k.points;
  if (ps.length < 2) return [];
  const seg = (a: Required<PenPoint>, b: Required<PenPoint>): [Pt, Pt, Pt, Pt] => [
    [a.x * w, a.y * h],
    [(a.x + a.ox) * w, (a.y + a.oy) * h],
    [(b.x + b.ix) * w, (b.y + b.iy) * h],
    [b.x * w, b.y * h],
  ];
  const out = ps.slice(1).map((b, i) => seg(ps[i], b));
  if (k.closed) out.push(seg(ps[ps.length - 1], ps[0]));
  return out;
}

function pathD(c: Config, w: number, h: number) {
  const s = segments(c, w, h);
  if (!s.length) return '';
  return `M${r2(s[0][0][0])} ${r2(s[0][0][1])}` + s.map((q) => `C${r2(q[1][0])} ${r2(q[1][1])} ${r2(q[2][0])} ${r2(q[2][1])} ${r2(q[3][0])} ${r2(q[3][1])}`).join('') + (cfg(c).closed ? 'Z' : '');
}

const STEPS = 48;
/** The path as a polyline with lengths (mirrors flatten / Poly). */
function flatten(segs: [Pt, Pt, Pt, Pt][]) {
  const pts: Pt[] = [];
  const len: number[] = [];
  segs.forEach((s, si) => {
    for (let j = si > 0 ? 1 : 0; j <= STEPS; j++) {
      const t = j / STEPS;
      const u = 1 - t;
      const [a, b, c, d] = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
      const q: Pt = [a * s[0][0] + b * s[1][0] + c * s[2][0] + d * s[3][0], a * s[0][1] + b * s[1][1] + c * s[2][1] + d * s[3][1]];
      const last = pts[pts.length - 1];
      len.push(last ? len[len.length - 1] + Math.hypot(q[0] - last[0], q[1] - last[1]) : 0);
      pts.push(q);
    }
  });
  const total = len[len.length - 1] ?? 0;
  const pointAt = (l: number): Pt => {
    const n = pts.length;
    if (!n) return [0, 0];
    l = clamp(l, 0, total);
    let i = len.findIndex((x) => x >= l);
    i = clamp(i < 0 ? n - 1 : i, 1, n - 1);
    const [l0, l1] = [len[i - 1], len[i]];
    const k = l1 > l0 ? (l - l0) / (l1 - l0) : 0;
    return [pts[i - 1][0] + (pts[i][0] - pts[i - 1][0]) * k, pts[i - 1][1] + (pts[i][1] - pts[i - 1][1]) * k];
  };
  const dirAt = (l: number): Pt => {
    const e = Math.min(1, total / 4);
    const a = pointAt(l - e);
    const b = pointAt(l + e);
    const n = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return n < 1e-6 ? [1, 0] : [(b[0] - a[0]) / n, (b[1] - a[1]) / n];
  };
  const cut = (a: number, b: number): Pt[] => [pointAt(a), ...pts.filter((_, i) => len[i] > a && len[i] < b), pointAt(b)];
  return { total, pointAt, dirAt, cut };
}

export function strokeWidth(c: Config, base: number, w: number, h: number) {
  const k = cfg(c);
  if (k.fixedWidth || k.refW <= 0 || k.refH <= 0) return base;
  return base * Math.sqrt(Math.max(0, w * h) / (k.refW * k.refH));
}

const ease = (k: number) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
const easeOut = (k: number) => 1 - Math.pow(1 - k, 3);

/** How long the drawing lasts, s: its own duration, else the template's In (1 s without one) (mirrors draw_secs). */
function drawSecs(c: Config, hold: number, fps: number) {
  const d = c.drawDuration;
  if (d !== undefined && Number.isFinite(d) && d > 0) return d;
  return hold > 0 ? hold / Math.max(1, fps) : 1;
}

/** In / Out progress (mirrors progress): the In draws over the pen's duration once its delay has passed. */
function progress(c: Config, frame: number, hold: number, fps: number, age: number): [number, number] {
  fps = Math.max(1, fps);
  const secs = drawSecs(c, hold, fps);
  const kIn = clamp((age - Math.max(0, cfg(c).drawDelay)) / secs, 0, 1);
  const kOut = frame > hold + 1e-6 ? clamp((frame - hold) / (secs * fps), 0, 1) : 0;
  return [kIn, kOut];
}

/** The visible portion (start / end %: drawStart / drawEnd at this frame) with the In / Out, and the fill opacity (mirrors portion). */
function portion(c: Config, start: number, end: number, [kIn, kOut]: [number, number]): [number, number, number] {
  const k = cfg(c);
  let a = clamp(start, 0, 100);
  let b = clamp(end, 0, 100);
  if (!k.drawIn) return [a, b, 1];
  b = a + (b - a) * easeOut(kIn);
  a += (b - a) * ease(kOut);
  return [a, b, easeOut(kIn) * (1 - ease(kOut))];
}

type Head = { kind: 'fill'; pts: Pt[]; w: number } | { kind: 'line'; pts: Pt[]; w: number; butt: boolean } | { kind: 'circle'; c: Pt; r: number };

/** Arrow head at a tip (mirrors arrow): [head, inset]. */
function arrow(style: Arrow, [x, y]: Pt, [dx, dy]: Pt, size: number, sw: number): [Head, number] | null {
  if (style === 'none' || size <= 0) return null;
  const [nx, ny] = [-dy, dx];
  const at = (along: number, side: number): Pt => [x + dx * along + nx * side, y + dy * along + ny * side];
  const [L, Wd] = [size, size * 0.9];
  switch (style) {
    case 'triangle':
      return [{ kind: 'fill', pts: [at(0, 0), at(-L, Wd / 2), at(-L, -Wd / 2)], w: sw * 0.15 }, L * 0.8];
    case 'open':
      return [{ kind: 'line', pts: [at(-L * 0.9, Wd / 2), at(0, 0), at(-L * 0.9, -Wd / 2)], w: sw, butt: false }, sw * 0.5];
    case 'diamond':
      return [{ kind: 'fill', pts: [at(0, 0), at(-L / 2, Wd / 3), at(-L, 0), at(-L / 2, -Wd / 3)], w: 0 }, L * 0.5];
    case 'circle':
      return [{ kind: 'circle', c: [x - dx * L * 0.3, y - dy * L * 0.3], r: L * 0.32 }, L * 0.3];
    case 'bar':
      return [{ kind: 'line', pts: [at(0, Wd / 2), at(0, -Wd / 2)], w: Math.max(sw, 4), butt: true }, 0];
  }
  return null;
}

const poly = (pts: Pt[]) => pts.map((p, i) => `${i ? 'L' : 'M'}${r2(p[0])} ${r2(p[1])}`).join('');

// Selected point and tool: Designer UI state, per layer (not saved).
const ui = { sel: new Map<string, number>(), tool: new Map<string, 'edit' | 'add'>() };
const subs = new Set<() => void>();
let version = 0;
const bump = () => {
  version++;
  subs.forEach((f) => f());
};
const useUi = () =>
  useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => version,
  );
const selOf = (id: string, n: number) => clamp(ui.sel.get(id) ?? 0, 0, Math.max(0, n - 1));
const setSel = (id: string, i: number) => (ui.sel.set(id, i), bump());
const toolOf = (id: string) => ui.tool.get(id) ?? 'edit';
const setTool = (id: string, t: 'edit' | 'add') => (ui.tool.set(id, t), bump());

type L = Config & { id: string; props: Record<string, unknown> };
type Edit = (fn: (l: L) => void, key?: string) => void;

/** Smooth handles for point i (curve). */
function smooth(l: L, i: number, w: number, h: number) {
  const k = cfg(l);
  const ps = k.points;
  const n = ps.length;
  const cur = ps[i];
  const prev = i > 0 ? ps[i - 1] : k.closed ? ps[n - 1] : null;
  const next = i < n - 1 ? ps[i + 1] : k.closed ? ps[0] : null;
  let tx: number, ty: number;
  if (prev && next) [tx, ty] = [((next.x - prev.x) * w) / 4, ((next.y - prev.y) * h) / 4];
  else if (next) [tx, ty] = [((next.x - cur.x) * w) / 3, ((next.y - cur.y) * h) / 3];
  else if (prev) [tx, ty] = [((cur.x - prev.x) * w) / 3, ((cur.y - prev.y) * h) / 3];
  else [tx, ty] = [80, 0];
  Object.assign(cur, { type: 'curve', ox: f4(tx / w), oy: f4(ty / h), ix: f4(-tx / w), iy: f4(-ty / h) });
  l.points = ps;
}

function setType(l: L, i: number, type: 'corner' | 'curve', w: number, h: number) {
  if (type === 'curve') return smooth(l, i, w, h);
  const ps = cfg(l).points;
  Object.assign(ps[i], { type: 'corner', ix: 0, iy: 0, ox: 0, oy: 0 });
  l.points = ps;
}

/** Drag of a handle ('i' / 'o') to (hx, hy) layer px, honouring the point's handle mode. */
function dragHandle(l: L, i: number, which: 'i' | 'o', hx: number, hy: number, w: number, h: number) {
  const ps = cfg(l).points;
  const p = ps[i];
  const [dx, dy] = [hx - p.x * w, hy - p.y * h];
  const other = which === 'o' ? 'i' : 'o';
  p[`${which}x`] = f4(dx / w);
  p[`${which}y`] = f4(dy / h);
  if (p.mode === 'symmetric') [p[`${other}x`], p[`${other}y`]] = [f4(-dx / w), f4(-dy / h)];
  else if (p.mode === 'asymmetric') {
    const lo = Math.hypot(p[`${other}x`] * w, p[`${other}y`] * h) || Math.hypot(dx, dy);
    const len = Math.hypot(dx, dy) || 1;
    [p[`${other}x`], p[`${other}y`]] = [f4(((-dx / len) * lo) / w), f4(((-dy / len) * lo) / h)];
  }
  l.points = ps;
}

function deletePoint(layer: L, edit: Edit, i: number) {
  const n = cfg(layer).points.length;
  if (n <= 1) return useApp.getState().toast(t('This is the last point: use “New path” to start again'), 'info');
  edit((l) => {
    const ps = cfg(l).points;
    ps.splice(i, 1);
    l.points = ps;
  });
  setSel(layer.id, clamp(i, 0, n - 2));
}

/** Keys of the stage editing: Delete removes the selected point (after a click on a handle), Esc leaves Add. */
function useStageKeys(layer: L, edit: Edit) {
  const [focus, setFocus] = useState(false);
  useEffect(() => {
    const down = (e: PointerEvent) => setFocus(!!(e.target as HTMLElement).closest?.('.wk-handle, .wk-catch'));
    const key = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest?.('input, textarea, select')) return;
      if (e.key === 'Escape' && toolOf(layer.id) === 'add') {
        e.preventDefault();
        e.stopPropagation();
        setTool(layer.id, 'edit');
      } else if (focus && (e.key === 'Delete' || e.key === 'Backspace')) {
        e.preventDefault();
        e.stopPropagation();
        deletePoint(layer, edit, selOf(layer.id, cfg(layer).points.length));
      }
    };
    addEventListener('pointerdown', down, true);
    addEventListener('keydown', key, true);
    return () => {
      removeEventListener('pointerdown', down, true);
      removeEventListener('keydown', key, true);
    };
  }, [focus, layer, edit]);
}

/** Inspector list of points: select, coordinates in px of the box. */
function PointList({ layer, edit }: { layer: L; edit: Edit }) {
  const k = cfg(layer);
  const w = num(layer.props.width as number) || 1;
  const h = num(layer.props.height as number) || 1;
  const sel = selOf(layer.id, k.points.length);
  const setV = (i: number, ax: 'x' | 'y', v: number) =>
    edit((l) => {
      const ps = cfg(l).points;
      const dim = ax === 'x' ? w : h;
      ps[i][ax] = f4(clamp(v, 0, dim) / dim);
      l.points = ps;
    }, `pt-${layer.id}-${i}-${ax}`);
  return (
    <Row expose={false} label={t('Points · {n}', { n: k.points.length })}>
      <div className="wk-list">
        {k.points.map((p, i) => (
          <div key={i} className="wk-item" style={i === sel ? { background: 'rgba(124,92,255,.18)', borderRadius: 6 } : undefined}>
            <button className="wk-x" title={t('Select')} style={{ flex: '0 0 30px', fontWeight: 700 }} onClick={() => setSel(layer.id, i)}>
              {i + 1}
            </button>
            {(['x', 'y'] as const).map((ax) => (
              <span key={ax} style={{ display: 'flex', alignItems: 'center', gap: 4, flex: '1 1 0', minWidth: 0 }}>
                <span className="hint">{ax}</span>
                <input type="number" step={1} style={{ width: '100%', minWidth: 0 }} title={t('{axis} in px from the box', { axis: ax.toUpperCase() })} value={Math.round(p[ax] * (ax === 'x' ? w : h))} onKeyDown={(e) => e.stopPropagation()} onChange={(e) => e.target.value !== '' && setV(i, ax, +e.target.value)} />
              </span>
            ))}
            <span className="hint" style={{ flex: '0 0 58px', textAlign: 'right' }}>
              {p.type === 'curve' ? t('◠ curve') : t('◇ corner')}
            </span>
          </div>
        ))}
      </div>
    </Row>
  );
}

/** Fill group of a closed path. */
function FillSection({ layer }: { layer: L }) {
  const k = cfg(layer);
  const f = { ...FILL, ...k.fillStyle };
  const b = useBound();
  if (!k.closed)
    return (
      <Section id="pen-fill" title={t('Fill')}>
        <Toggle label={t('Fill')} value={false} disabled={t('Close the path to fill it')} />
      </Section>
    );
  return (
    <Section id="pen-fill" title={t('Fill')}>
      <Toggle label={t('Fill')} path="fillOn" value={false} />
      {k.fillOn && (
        <>
          <Seg
            label={t('Type')}
            path="fillStyle.kind"
            value={f.kind}
            options={[
              ['solid', t('Solid')],
              ['linear', t('Linear')],
              ['radial', t('Radial')],
              ['image', t('Image')],
            ]}
          />
          {f.kind !== 'image' && f.kind !== 'none' && <ColorGroupRow layer={layer as never} prop="fill" label={f.kind === 'solid' ? t('Color') : t('From')} />}
          {(f.kind === 'linear' || f.kind === 'radial') && <GradientTo layer={layer as never} />}
          {f.kind === 'linear' && <Num label={t('Angle')} path="fillStyle.angle" value={FILL.angle} min={0} max={360} unit="°" />}
          {f.kind === 'image' && (
            <>
              <Row expose="fillStyle.image" exposeValue={String(getPath(b.obj, 'fillStyle.image') ?? '')} label={t('Picture')}>
                <AssetPicker value={String(getPath(b.obj, 'fillStyle.image') ?? '')} onChange={(v) => b.set('fillStyle.image', v)} />
              </Row>
              <Seg
                label={t('Fit')}
                path="fillStyle.fit"
                value={FILL.fit}
                options={[
                  ['contain', t('Contain')],
                  ['cover', t('Cover')],
                  ['stretch', t('Stretch')],
                  ['tile', t('Tile')],
                ]}
              />
            </>
          )}
          <Seg
            label={t('Fill rule')}
            path="fillRule"
            value={k.fillRule}
            options={[
              ['nonzero', t('Non-zero')],
              ['evenodd', t('Even-odd')],
            ]}
          />
        </>
      )}
    </Section>
  );
}

const ARROWS: [Arrow, string][] = [
  ['none', 'None'],
  ['triangle', '▶'],
  ['open', '›'],
  ['diamond', '◆'],
  ['circle', '●'],
  ['bar', '|'],
];

export default defineWidget({
  id: 'pen',
  type: 'pen',
  label: 'Pen',
  group: 'Shapes & tools',
  icon: PenTool,
  create: () => ({ points: DEFAULT_PTS(), arrowEnd: 'none', refW: 1100, refH: 560, props: { width: 1100, height: 560, stroke: [1, 1, 1, 1], strokeWidth: 16 } }),
  ownsLook: true,
  live: (l) => l.drawIn !== false,
  Render: ({ layer, r, w, h, data, time, frame, hold, fps, designer }) => {
    const k = cfg(layer);
    const segs = segments(layer, w, h);
    if (!segs.length) return null;
    const [pa, pb, fillOp] = portion(layer, num(r.drawStart), num(r.drawEnd), progress(layer, frame, hold, fps, time.ageSecs));
    const d = pathD(layer, w, h);
    const st = strokeOf(layer);
    const sw = strokeWidth(layer, num(r.strokeWidth), w, h);
    const color = cssColor(col(r.stroke));
    const P2 = flatten(segs);
    const T = P2.total;
    const [a, b] = [(T * pa) / 100, (T * pb) / 100];
    let body = null;
    if (b - a > 0.5 && sw > 0) {
      const size = sw * k.arrowSize;
      const head = arrow(k.arrowEnd, P2.pointAt(b), P2.dirAt(b), size, sw);
      const da = P2.dirAt(a);
      const tail = arrow(k.arrowStart, P2.pointAt(a), [-da[0], -da[1]], size, sw);
      const room = (b - a) * 0.45;
      const [hi, ti] = [head?.[1] ?? 0, tail?.[1] ?? 0];
      const [sa, sb] = [a + Math.min(ti, room), b - Math.min(hi, room)];
      const full = a <= 0.01 && b >= T - 0.01 && !hi && !ti;
      const dash = st.dash === 'dashed' ? `${sw * 3} ${sw * 2}` : st.dash === 'dotted' ? `0.001 ${sw * 2}` : undefined;
      body = (
        <>
          <path d={full ? d : poly(P2.cut(sa, sb))} fill="none" stroke={color} strokeWidth={sw} strokeLinecap={st.dash === 'dotted' ? 'round' : st.cap} strokeLinejoin={st.join} strokeDasharray={dash} strokeDashoffset={dash ? sa : undefined} />
          {[head?.[0], tail?.[0]].map((hd, i) =>
            !hd ? null : hd.kind === 'fill' ? (
              <path key={i} d={poly(hd.pts) + 'Z'} fill={color} stroke={hd.w > 0 ? color : 'none'} strokeWidth={hd.w} strokeLinejoin="round" />
            ) : hd.kind === 'line' ? (
              <path key={i} d={poly(hd.pts)} fill="none" stroke={color} strokeWidth={hd.w} strokeLinecap={hd.butt ? 'butt' : 'round'} strokeLinejoin="round" />
            ) : (
              <circle key={i} cx={hd.c[0]} cy={hd.c[1]} r={hd.r} fill={color} />
            ),
          )}
        </>
      );
    }
    return (
      <>
        {k.closed && k.fillOn && fillOp > 0 && (
          <div style={{ position: 'absolute', inset: 0, opacity: fillOp }}>
            <ShapeSvg d={d} w={w} h={h} r={{ ...r, strokeWidth: 0 }} fill={k.fillStyle} rule={k.fillRule} data={data} />
          </div>
        )}
        <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} style={{ position: 'absolute', inset: 0, overflow: 'visible' }}>
          {/* Designer only: the whole path, faint, while the In has not drawn it all. */}
          {designer && (pa > 0.01 || pb < 99.99) && <path d={d} fill="none" stroke={color} strokeOpacity={0.22} strokeWidth={Math.max(2, sw)} strokeLinecap="round" strokeLinejoin="round" strokeDasharray={`${Math.max(4, sw)} ${Math.max(4, sw)}`} />}
          {body}
        </svg>
      </>
    );
  },
  Inspector: ({ layer, template, edit }) => {
    useUi();
    const fps = useApp((s) => (s.project ? s.project.frameRate.num / s.project.frameRate.den : 50));
    const k = cfg(layer);
    const ed = edit as unknown as Edit;
    const lay = layer as unknown as L;
    const w = num(layer.props.width ?? 0) || 1;
    const h = num(layer.props.height ?? 0) || 1;
    const tool = toolOf(layer.id);
    const sel = selOf(layer.id, k.points.length);
    const p = k.points[sel];
    return (
      <>
        <Section id="pen-path" title={t('Path')}>
          <Seg
            label={t('Tool')}
            value={tool}
            onChange={(v) => setTool(layer.id, v)}
            options={[
              ['edit', t('✥ Edit'), t('Move points and handles (Esc)')],
              ['add', t('✚ Add'), t('Click on the stage to add a point')],
            ]}
          />
          <Toggle label={t('Closed')} path="closed" value={k.closed} />
          <PointList layer={lay} edit={ed} />
          {p && (
            <>
              <Seg
                expose={false}
                label={t('Point {n}', { n: sel + 1 })}
                value={p.type}
                onChange={(v) => ed((l) => setType(l, sel, v, w, h))}
                options={[
                  ['corner', t('Corner')],
                  ['curve', t('Curve')],
                ]}
              />
              {p.type === 'curve' && (
                <Seg
                  expose={false}
                  label={t('Handles')}
                  value={p.mode}
                  onChange={(v) =>
                    ed((l) => {
                      const ps = cfg(l).points;
                      ps[sel].mode = v;
                      if (v === 'symmetric') [ps[sel].ix, ps[sel].iy] = [-ps[sel].ox, -ps[sel].oy];
                      l.points = ps;
                    })
                  }
                  options={[
                    ['symmetric', t('Symmetric')],
                    ['asymmetric', t('Asymmetric')],
                    ['broken', t('Broken')],
                  ]}
                />
              )}
            </>
          )}
          <Btns
            list={[
              p && { label: t('✕ Delete point'), cls: 'ghost', keyHint: 'Del', onClick: () => deletePoint(lay, ed, sel) },
              p && {
                label: t('⇄ Reverse'),
                cls: 'ghost',
                title: t('Swap start and end'),
                onClick: () =>
                  ed(
                    (l) =>
                      void (l.points = cfg(l)
                        .points.reverse()
                        .map((q) => ({ ...q, ix: q.ox, iy: q.oy, ox: q.ix, oy: q.iy }))),
                  ),
              },
              {
                label: t('New path'),
                cls: 'ghost',
                confirm: t('Start a new empty path?'),
                onClick: () => {
                  ed((l) => void (l.points = []));
                  setSel(layer.id, 0);
                  setTool(layer.id, 'add');
                },
              },
            ]}
          />
        </Section>
        <Section id="pen-stroke" title={t('Stroke')}>
          <StrokeRows layer={layer} caps capDefault="round" joinDefault="round" />
          <Toggle label={t('Fixed width')} path="fixedWidth" value={k.fixedWidth} />
          {!k.fixedWidth && <Btns list={[{ label: t('Use current size'), cls: 'ghost', title: t('Reference size = the box now'), onClick: () => ed((l) => void ((l.refW = Math.round(w)), (l.refH = Math.round(h)))) }]} />}
        </Section>
        <Section id="pen-arrows" title={t('Arrows')}>
          <Seg label={t('Start')} path="arrowStart" value={k.arrowStart} options={ARROWS.map(([v, l]) => [v, t(l)] as [Arrow, string])} />
          <Seg label={t('End')} path="arrowEnd" value={k.arrowEnd} options={ARROWS.map(([v, l]) => [v, t(l)] as [Arrow, string])} />
          {(k.arrowStart !== 'none' || k.arrowEnd !== 'none') && <Num label={t('Size')} path="arrowSize" value={k.arrowSize} min={1.5} max={8} step={0.5} precision={1} unit="×" />}
        </Section>
        <FillSection layer={lay} />
        <Section id="pen-portion" title={t('Drawing')}>
          {/* Animatable (◆ puts them on the timeline). */}
          <GroupRow layer={layer as Layer} spec={{ label: t('Start'), props: ['drawStart'], min: 0, max: 100, suffix: '%' }} />
          <GroupRow layer={layer as Layer} spec={{ label: t('End'), props: ['drawEnd'], min: 0, max: 100, suffix: '%' }} />
          <Toggle label={t('Draw at In')} path="drawIn" value={k.drawIn} />
          {k.drawIn && (
            <>
              <Num label={t('Duration')} path="drawDuration" value={+drawSecs(layer, holdFrame(template), fps).toFixed(2)} min={0.05} max={600} step={0.1} precision={1} unit="s" />
              <Num label={t('Delay')} path="drawDelay" value={k.drawDelay} min={0} max={600} step={0.1} precision={1} unit="s" />
            </>
          )}
        </Section>
      </>
    );
  },
  Overlay: ({ layer, w, h, scale, edit }) => {
    useUi();
    const lay = layer as unknown as L;
    const ed = edit as unknown as Edit;
    useStageKeys(lay, ed);
    const k = cfg(layer);
    const n = k.points.length;
    const sel = selOf(layer.id, n);
    const tool = toolOf(layer.id);
    const sp = k.points[sel];
    const S = (v: number) => v * scale;
    const first = sel === 0 && !k.closed;
    const last = sel === n - 1 && !k.closed;
    const curve = sp && sp.type === 'curve';
    const hi: Pt | null = curve && !first ? [(sp.x + sp.ix) * w, (sp.y + sp.iy) * h] : null;
    const ho: Pt | null = curve && !last ? [(sp.x + sp.ox) * w, (sp.y + sp.oy) * h] : null;
    return (
      <>
        {tool === 'add' && (
          <Catcher
            scale={scale}
            onDown={(x, y, e) => {
              const host = (e.currentTarget as HTMLElement).getBoundingClientRect();
              const fx = f4(clamp(x / w, 0, 1));
              const fy = f4(clamp(y / h, 0, 1));
              const at = n ? sel + 1 : 0;
              ed((l) => {
                const ps = cfg(l).points;
                ps.splice(at, 0, P(fx, fy));
                l.points = ps;
              });
              setSel(layer.id, at);
              // Drag right after the click pulls out symmetric curve handles.
              const move = (ev: PointerEvent) => {
                const dx = (ev.clientX - host.left) / scale - x;
                const dy = (ev.clientY - host.top) / scale - y;
                if (Math.hypot(dx, dy) * scale < 6) return;
                ed((l) => {
                  const ps = cfg(l).points;
                  if (ps[at]) Object.assign(ps[at], { type: 'curve', mode: 'symmetric', ox: f4(dx / w), oy: f4(dy / h), ix: f4(-dx / w), iy: f4(-dy / h) });
                  l.points = ps;
                }, `penadd-${layer.id}`);
              };
              const up = () => (removeEventListener('pointermove', move), removeEventListener('pointerup', up));
              addEventListener('pointermove', move);
              addEventListener('pointerup', up);
            }}
          />
        )}
        <svg style={{ position: 'absolute', left: 0, top: 0, overflow: 'visible', pointerEvents: 'none' }} width={1} height={1}>
          {n === 1 && <circle cx={S(k.points[0].x * w)} cy={S(k.points[0].y * h)} r={22} fill="none" stroke="var(--warn)" strokeWidth={3} strokeDasharray="6 6" />}
          {hi && <line x1={S(sp.x * w)} y1={S(sp.y * h)} x2={S(hi[0])} y2={S(hi[1])} stroke="var(--warn)" strokeWidth={2} />}
          {ho && <line x1={S(sp.x * w)} y1={S(sp.y * h)} x2={S(ho[0])} y2={S(ho[1])} stroke="var(--warn)" strokeWidth={2} />}
          {sp && <circle cx={S(sp.x * w)} cy={S(sp.y * h)} r={13} fill="none" stroke="var(--warn)" strokeWidth={2} />}
        </svg>
        {k.points.map((p, i) => (
          <Handle
            key={i}
            x={p.x * w}
            y={p.y * h}
            scale={scale}
            label={i === sel ? t('Point {n} · {type} (selected)', { n: i + 1, type: t(p.type) }) : t('Point {n} · {type}', { n: i + 1, type: t(p.type) })}
            onDrag={(x, y) => {
              if (selOf(layer.id, n) !== i) setSel(layer.id, i);
              ed((l) => {
                const ps = cfg(l).points;
                ps[i].x = f4(clamp(x / w, 0, 1));
                ps[i].y = f4(clamp(y / h, 0, 1));
                l.points = ps;
              }, `penpt-${layer.id}-${i}`);
            }}
          />
        ))}
        {hi && <Handle x={hi[0]} y={hi[1]} scale={scale} label={t('In handle · {mode}', { mode: t(sp.mode) })} onDrag={(x, y) => ed((l) => dragHandle(l, sel, 'i', x, y, w, h), `penh-${layer.id}`)} />}
        {ho && <Handle x={ho[0]} y={ho[1]} scale={scale} label={t('Out handle · {mode}', { mode: t(sp.mode) })} onDrag={(x, y) => ed((l) => dragHandle(l, sel, 'o', x, y, w, h), `penh-${layer.id}`)} />}
      </>
    );
  },
  warnings: (layer, { r }) => {
    const k = cfg(layer);
    const out: Warning[] = [];
    const n = k.points.length;
    const [vs, ve] = [num(r?.drawStart ?? propValue(layer as Layer, 'drawStart', 0)), num(r?.drawEnd ?? propValue(layer as Layer, 'drawEnd', 0))];
    const width = num(r?.strokeWidth ?? layer.props.strokeWidth ?? 0);
    if (n === 0) out.push({ level: 'info', text: t('Empty path: choose Add points and click on the stage.') });
    if (n === 1) out.push({ level: 'warn', text: t('Path with a single point: nothing is drawn. Add at least one more point.') });
    if (!k.closed && k.fillOn) out.push({ level: 'info', text: t('Fill is ignored while the path is open.') });
    if (ve <= vs && n > 1) out.push({ level: 'warn', text: t('Visible portion: End ≤ Start, the stroke is hidden.') });
    if (!width && !(k.closed && k.fillOn) && n > 1) out.push({ level: 'warn', text: t('Width 0 and no fill: the path is invisible.') });
    return out;
  },
});

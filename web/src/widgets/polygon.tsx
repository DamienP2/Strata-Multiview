// Polygon: regular (with a star option), triangle presets, or a free polygon edited on the stage.
// Free vertices are fractions of the box (they scale with it).
// Same picture as crates/cg-render/src/wid/polygon.rs; geometry mirrors crates/cg-core/src/wid/polygon.rs.
import { useEffect, useState, useSyncExternalStore } from 'react';
import { Pentagon } from 'lucide-react';
import { useApp } from '../store';
import { num } from '../engine/evaluate';
import { Btns, clamp, defineWidget, Handle, Num, Row, Section, Seg, Toggle, type Warning, withDefaults } from './kit';
import { FillStrokeSections, roundedPolygon, ShapeSvg, type FillRule, type FillStyle, type Pt, type StrokeStyle } from './shape';
import { t, tc, tn } from '../i18n';

export interface Config {
  mode?: 'regular' | 'triangle' | 'free';
  sides?: number;
  startAngle?: number;
  star?: boolean;
  inner?: number;
  tri?: 'equilateral' | 'isosceles' | 'right';
  triDir?: 'up' | 'down' | 'left' | 'right';
  /** Free polygon: vertices as fractions of the box. */
  verts?: Pt[];
  clickAdd?: boolean;
  round?: number;
  fillRule?: FillRule;
  fillStyle?: FillStyle;
  strokeStyle?: StrokeStyle;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface State {}

const FREE_DEFAULT = (): Pt[] => [
  [0.04, 0.12],
  [0.68, 0.02],
  [0.98, 0.5],
  [0.68, 0.98],
  [0.04, 0.88],
  [0.24, 0.5],
];
const D = { mode: 'regular' as const, sides: 6, startAngle: 0, star: false, inner: 50, tri: 'equilateral' as const, triDir: 'up' as const, clickAdd: true, round: 0, fillRule: 'nonzero' as FillRule };
const cfg = (c: Config) => ({ ...withDefaults(D, c), verts: c.verts ?? FREE_DEFAULT() });
const DEG = Math.PI / 180;
const f4 = (v: number) => +v.toFixed(4);

/** Regular polygon or star on the unit circle, and how it is fitted to the box: its edges touch the four sides (mirrors regular_frame). */
function regularFrame(c: Config, w: number, h: number) {
  const k = cfg(c);
  const n = clamp(Math.round(k.sides), 3, 64);
  const count = k.star ? n * 2 : n;
  const inner = clamp(k.inner, 1, 100) / 100;
  const unit: Pt[] = [];
  for (let i = 0; i < count; i++) {
    const a = (k.startAngle + (i * 360) / count) * DEG;
    const r = k.star && i % 2 ? inner : 1;
    unit.push([Math.sin(a) * r, -Math.cos(a) * r]);
  }
  const xs = unit.map((p) => p[0]);
  const ys = unit.map((p) => p[1]);
  const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
  // Scale of the unit circle and the place of its centre in the box.
  const sx = x1 - x0 > 1e-6 ? w / (x1 - x0) : 0;
  const sy = y1 - y0 > 1e-6 ? h / (y1 - y0) : 0;
  return { unit, sx, sy, cx: sx ? -x0 * sx : w / 2, cy: sy ? -y0 * sy : h / 2 };
}

/** Regular polygon or star fitted to the box (mirrors regular_points). */
function regularPts(c: Config, w: number, h: number): Pt[] {
  const f = regularFrame(c, w, h);
  return f.unit.map(([x, y]) => [f.cx + x * f.sx, f.cy + y * f.sy]);
}

/** Triangle presets (mirrors triangle_points). */
function trianglePts(c: Config, w: number, h: number): Pt[] {
  const k = cfg(c);
  const dir = k.triDir;
  const side = dir === 'left' || dir === 'right';
  const unit: Pt[] =
    k.tri === 'right'
      ? [
          [0, 0],
          [1, 1],
          [0, 1],
        ]
      : [
          [0.5, 0],
          [1, 1],
          [0, 1],
        ];
  const turn = ([u, v]: Pt): Pt => (dir === 'down' ? [u, 1 - v] : dir === 'left' ? [v, u] : dir === 'right' ? [1 - v, u] : [u, v]);
  let [fw, fh, ox, oy] = [w, h, 0, 0];
  if (k.tri === 'equilateral') {
    const along = side ? h : w;
    const across = side ? w : h;
    const len = Math.min(along, (across * 2) / Math.sqrt(3));
    const th = (len * Math.sqrt(3)) / 2;
    [fw, fh] = side ? [th, len] : [len, th];
    ox = (w - fw) / 2;
    oy = (h - fh) / 2;
  }
  return unit.map(turn).map(([u, v]) => [ox + u * fw, oy + v * fh]);
}

export function points(c: Config, w: number, h: number): Pt[] {
  const k = cfg(c);
  if (k.mode === 'regular') return regularPts(c, w, h);
  if (k.mode === 'triangle') return trianglePts(c, w, h);
  return k.verts.map(([x, y]) => [x * w, y * h]);
}

/** How many vertices have their rounding capped. */
function cappedCount(pts: Pt[], r: number) {
  if (!r) return 0;
  let c = 0;
  const n = pts.length;
  for (let i = 0; i < n; i++) {
    const p0 = pts[(i - 1 + n) % n];
    const p1 = pts[i];
    const p2 = pts[(i + 1) % n];
    const l0 = Math.hypot(p0[0] - p1[0], p0[1] - p1[1]);
    const l2 = Math.hypot(p2[0] - p1[0], p2[1] - p1[1]);
    if (l0 < 1e-6 || l2 < 1e-6) continue;
    const cos = clamp(((p0[0] - p1[0]) * (p2[0] - p1[0]) + (p0[1] - p1[1]) * (p2[1] - p1[1])) / (l0 * l2), -1, 1);
    const tan = Math.tan(Math.acos(cos) / 2);
    if (tan > 1e-6 && r / tan > Math.min(l0, l2) / 2 + 0.5) c++;
  }
  return c;
}

function segCross(a: Pt, b: Pt, c: Pt, d: Pt) {
  const o = (p: Pt, q: Pt, r: Pt) => Math.sign((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]));
  return o(a, b, c) * o(a, b, d) < 0 && o(c, d, a) * o(c, d, b) < 0;
}
function selfCrossing(pts: Pt[]) {
  const n = pts.length;
  for (let i = 0; i < n; i++)
    for (let j = i + 2; j < n; j++) {
      if (i === 0 && j === n - 1) continue;
      if (segCross(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n])) return true;
    }
  return false;
}
function overlapping(pts: Pt[]) {
  let c = 0;
  for (let i = 0; i < pts.length; i++) {
    const j = (i + 1) % pts.length;
    if (Math.hypot(pts[i][0] - pts[j][0], pts[i][1] - pts[j][1]) < 2) c++;
  }
  return c;
}

// Selected vertex and refusals: Designer UI state, per layer (not saved).
const ui = { sel: new Map<string, number>(), refusedAt: new Map<string, number>() };
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

type L = Config & { id: string; props: Record<string, unknown> };
type Edit = (fn: (l: L) => void, key?: string) => void;

/** Removes vertex i, refused under 3 vertices. */
function removeVertex(layer: L, edit: Edit, i: number) {
  const v = cfg(layer).verts;
  if (v.length <= 3) {
    ui.refusedAt.set(layer.id, Date.now());
    bump();
    useApp.getState().toast(t('Refused: a polygon keeps at least 3 vertices'), 'error');
    return;
  }
  edit((l) => {
    const vs = [...cfg(l).verts];
    vs.splice(i, 1);
    l.verts = vs;
  });
  setSel(layer.id, Math.min(i, v.length - 2));
}

/** Inserts a vertex on the side nearest to (px, py). */
function insertNear(layer: L, edit: Edit, w: number, h: number, px: number, py: number) {
  const pts = points(layer, w, h);
  const n = pts.length;
  let best = 0;
  let bd = Infinity;
  for (let i = 0; i < n; i++) {
    const a = pts[i];
    const b = pts[(i + 1) % n];
    const vx = b[0] - a[0];
    const vy = b[1] - a[1];
    const L2 = vx * vx + vy * vy || 1;
    const k = clamp(((px - a[0]) * vx + (py - a[1]) * vy) / L2, 0, 1);
    const d = Math.hypot(a[0] + vx * k - px, a[1] + vy * k - py);
    if (d < bd) [bd, best] = [d, i];
  }
  edit((l) => {
    const vs = [...cfg(l).verts];
    vs.splice(best + 1, 0, [f4(clamp(px / w, 0, 1)), f4(clamp(py / h, 0, 1))]);
    l.verts = vs;
  });
  setSel(layer.id, best + 1);
}

/** Alt held (Alt+click adds a vertex even when Stage click is off). */
function useAlt() {
  const [alt, setAlt] = useState(false);
  useEffect(() => {
    const on = (e: KeyboardEvent) => setAlt(e.altKey);
    const off = () => setAlt(false);
    addEventListener('keydown', on);
    addEventListener('keyup', on);
    addEventListener('blur', off);
    return () => {
      removeEventListener('keydown', on);
      removeEventListener('keyup', on);
      removeEventListener('blur', off);
    };
  }, []);
  return alt;
}

/** Delete / Backspace remove the selected vertex while a vertex was the last thing clicked (else the Designer deletes the layer). */
function useVertexDelete(active: boolean, onDelete: () => void) {
  const [focus, setFocus] = useState(false);
  useEffect(() => {
    if (!active) return;
    const down = (e: PointerEvent) => setFocus(!!(e.target as HTMLElement).closest?.('.wk-handle'));
    const key = (e: KeyboardEvent) => {
      if (!focus || (e.key !== 'Delete' && e.key !== 'Backspace')) return;
      if ((e.target as HTMLElement).closest?.('input, textarea, select')) return;
      e.preventDefault();
      e.stopPropagation();
      onDelete();
    };
    addEventListener('pointerdown', down, true);
    addEventListener('keydown', key, true);
    return () => {
      removeEventListener('pointerdown', down, true);
      removeEventListener('keydown', key, true);
    };
  }, [active, focus, onDelete]);
}

/** Inspector list of free vertices: x / y in px of the box, select, remove, add. */
function VertexList({ layer, edit }: { layer: L; edit: Edit }) {
  useUi();
  const k = cfg(layer);
  const w = num(layer.props.width as number) || 1;
  const h = num(layer.props.height as number) || 1;
  const sel = selOf(layer.id, k.verts.length);
  const setV = (i: number, ax: 0 | 1, v: number) =>
    edit((l) => {
      const vs = cfg(l).verts.map((p) => [...p] as Pt);
      const dim = ax ? h : w;
      vs[i][ax] = f4(clamp(v, 0, dim) / dim);
      l.verts = vs;
    }, `vert-${layer.id}-${i}-${ax}`);
  return (
    <Row expose={false} label={t('Vertices')}>
      <div className="wk-list">
        {k.verts.map((v, i) => (
          <div key={i} className="wk-item" style={i === sel ? { background: 'rgba(124,92,255,.18)', borderRadius: 6 } : undefined}>
            <button className="wk-x" title={t('Select')} style={{ flex: '0 0 30px', fontWeight: 700 }} onClick={() => setSel(layer.id, i)}>
              {i + 1}
            </button>
            {([0, 1] as const).map((ax) => (
              <span key={ax} style={{ display: 'flex', alignItems: 'center', gap: 4, flex: '1 1 0', minWidth: 0 }}>
                <span className="hint" style={{ flex: '0 0 auto' }}>
                  {ax ? 'y' : 'x'}
                </span>
                <input
                  type="number"
                  step={1}
                  style={{ width: '100%', minWidth: 0 }}
                  title={ax ? t('Y in px from the box top') : t('X in px from the box left')}
                  value={Math.round(v[ax] * (ax ? h : w))}
                  onKeyDown={(e) => e.stopPropagation()}
                  onChange={(e) => e.target.value !== '' && setV(i, ax, +e.target.value)}
                />
              </span>
            ))}
            <button className="wk-x" title={t('Remove this vertex')} onClick={() => removeVertex(layer, edit, i)}>
              ✕
            </button>
          </div>
        ))}
        <button
          className="opx-btn small wk-add"
          onClick={() => {
            const n = k.verts.length;
            const a = k.verts[sel];
            const b = k.verts[(sel + 1) % n];
            edit((l) => {
              const vs = [...cfg(l).verts];
              vs.splice(sel + 1, 0, [f4((a[0] + b[0]) / 2), f4((a[1] + b[1]) / 2)]);
              l.verts = vs;
            });
            setSel(layer.id, sel + 1);
          }}
        >
          {t('+ Vertex after {n}', { n: sel + 1 })}
        </button>
      </div>
    </Row>
  );
}

export default defineWidget({
  id: 'polygon',
  type: 'polygon',
  label: 'Polygon',
  group: 'Shapes & tools',
  icon: Pentagon,
  create: () => ({ sides: 5, props: { width: 600, height: 600 } }),
  ownsLook: true,
  Render: ({ layer, r, w, h, data }) => {
    const k = cfg(layer);
    const pts = points(layer, w, h);
    if (pts.length < 3) return null;
    return <ShapeSvg d={roundedPolygon(pts, [k.round])} w={w} h={h} r={r} fill={layer.fillStyle} stroke={layer.strokeStyle} rule={k.mode === 'free' ? k.fillRule : 'nonzero'} data={data} />;
  },
  Inspector: ({ layer, edit }) => {
    const k = cfg(layer);
    const w = num(layer.props.width ?? 0);
    const h = num(layer.props.height ?? 0);
    const toFree = () =>
      edit((l) => {
        l.verts = points(l, w || 1, h || 1).map(([x, y]) => [f4(x / (w || 1)), f4(y / (h || 1))]);
        l.mode = 'free';
      });
    return (
      <>
        <Section id="polygon-shape" title={t('Shape')}>
          <Seg
            label={t('Mode')}
            path="mode"
            value={k.mode}
            options={[
              ['regular', tc('polygon', 'Regular')],
              ['triangle', t('Triangle')],
              ['free', t('Free')],
            ]}
          />
          {k.mode === 'regular' && (
            <>
              <Num label={t('Sides')} path="sides" value={k.sides} min={3} max={64} />
              <Num label={t('Orientation')} path="startAngle" value={k.startAngle} min={0} max={360} unit="°" />
              <Toggle label={t('Star')} path="star" value={k.star} />
              {k.star && <Num label={t('Inner radius')} path="inner" value={k.inner} min={1} max={100} unit="%" />}
            </>
          )}
          {k.mode === 'triangle' && (
            <>
              <Seg
                label={t('Preset')}
                path="tri"
                value={k.tri}
                options={[
                  ['equilateral', t('Equilateral')],
                  ['isosceles', t('Isosceles')],
                  ['right', t('Right-angled')],
                ]}
              />
              <Seg
                label={t('Direction')}
                path="triDir"
                value={k.triDir}
                options={[
                  ['up', '▲', t('Up')],
                  ['down', '▼', t('Down')],
                  ['left', '◀', t('Left')],
                  ['right', '▶', t('Right')],
                ]}
              />
            </>
          )}
          {k.mode !== 'free' && <Btns list={[{ label: t('Edit as free'), cls: 'ghost', title: t('Copy these vertices into a free polygon'), onClick: toFree }]} />}
          {k.mode === 'free' && (
            <>
              <VertexList layer={layer as L} edit={edit as unknown as Edit} />
              <Toggle label={t('Click to add')} path="clickAdd" value={k.clickAdd} expose={false} />
            </>
          )}
          <Num label={t('Rounding')} path="round" value={k.round} min={0} max={400} unit="px" />
        </Section>
        <FillStrokeSections layer={layer} id="polygon" edit={edit} rule={k.mode === 'free'} />
      </>
    );
  },
  Overlay: ({ layer, w, h, scale, edit }) => {
    useUi();
    const alt = useAlt();
    const k = cfg(layer);
    const ed = edit as unknown as Edit;
    const free = k.mode === 'free';
    const sel = selOf(layer.id, k.verts.length);
    useVertexDelete(free, () => removeVertex(layer as L, ed, selOf(layer.id, cfg(layer).verts.length)));
    if (free)
      return (
        <>
          {(k.clickAdd || alt) && (
            <div
              className="wk-catch"
              style={{ cursor: 'copy' }}
              title={t('Click: add a vertex on the nearest side')}
              onPointerDown={(e) => {
                e.stopPropagation();
                const r = e.currentTarget.getBoundingClientRect();
                insertNear(layer as L, ed, w, h, (e.clientX - r.left) / scale, (e.clientY - r.top) / scale);
              }}
            />
          )}
          {k.verts.map(([fx, fy], i) => (
            <Handle
              key={i}
              x={fx * w}
              y={fy * h}
              scale={scale}
              label={i === sel ? t('Vertex {n} (selected) · {x}, {y}', { n: i + 1, x: Math.round(fx * w), y: Math.round(fy * h) }) : t('Vertex {n} · {x}, {y}', { n: i + 1, x: Math.round(fx * w), y: Math.round(fy * h) })}
              onDrag={(x, y) => {
                if (selOf(layer.id, k.verts.length) !== i) setSel(layer.id, i);
                ed((l) => {
                  const vs = cfg(l).verts.map((p) => [...p] as Pt);
                  vs[i] = [f4(clamp(x / w, 0, 1)), f4(clamp(y / h, 0, 1))];
                  l.verts = vs;
                }, `vert-${layer.id}-${i}`);
              }}
            />
          ))}
          <SelRing x={k.verts[sel][0] * w * scale} y={k.verts[sel][1] * h * scale} />
        </>
      );
    if (k.mode === 'regular') {
      const pts = regularPts(layer, w, h);
      // Handles in the shape's own frame (its centre and scale once fitted to the box).
      const f = regularFrame(layer, w, h);
      const local = (x: number, y: number): Pt => [f.sx ? (x - f.cx) / f.sx : 0, f.sy ? (y - f.cy) / f.sy : 0];
      return (
        <>
          <Handle
            x={pts[0][0]}
            y={pts[0][1]}
            scale={scale}
            label={t('Orientation {angle}°', { angle: Math.round(k.startAngle) })}
            onDrag={(x, y) => {
              const [u, v] = local(x, y);
              const a = Math.atan2(u, -v) / DEG;
              ed((l) => void (l.startAngle = Math.round((a + 360) % 360)), `pangle-${layer.id}`);
            }}
          />
          {k.star && (
            <Handle
              x={pts[1][0]}
              y={pts[1][1]}
              scale={scale}
              label={t('Inner radius {pct} %', { pct: Math.round(k.inner) })}
              onDrag={(x, y) => ed((l) => void (l.inner = Math.round(clamp(Math.hypot(...local(x, y)) * 100, 1, 100))), `pinner-${layer.id}`)}
            />
          )}
        </>
      );
    }
    return null;
  },
  warnings: (layer, { r }) => {
    const k = cfg(layer);
    const w = num(r?.width ?? layer.props.width ?? 0);
    const h = num(r?.height ?? layer.props.height ?? 0);
    const out: Warning[] = [];
    const pts = points(layer, w, h);
    if (Date.now() - (ui.refusedAt.get(layer.id) ?? 0) < 5000) out.push({ level: 'warn', text: t('Removing that vertex was refused: a polygon keeps at least 3 vertices.') });
    if (pts.length < 3) out.push({ level: 'warn', text: t('Fewer than 3 vertices: nothing is drawn.') });
    const c = cappedCount(pts, k.round);
    if (c) out.push({ level: 'info', text: tn(c, 'Rounding {round} px capped on {n} vertex (too sharp an angle or too short a side): each vertex takes the largest radius that fits.', 'Rounding {round} px capped on {n} vertexes (too sharp an angle or too short a side): each vertex takes the largest radius that fits.', { round: k.round }) });
    if (k.mode === 'free') {
      if (selfCrossing(pts)) out.push({ level: 'info', text: t('The polygon crosses itself: fill rule {rule} applies.', { rule: t(k.fillRule === 'evenodd' ? 'Even-odd' : 'Non-zero') }) });
      const o = overlapping(pts);
      if (o) out.push({ level: 'info', text: tn(o, '{n} vertex on top of the next one: kept, drawn without artefact.', '{n} vertexes on top of the next one: kept, drawn without artefact.') });
    }
    return out;
  },
});

/** Ring round the selected vertex (screen px). */
function SelRing({ x, y }: { x: number; y: number }) {
  return <div style={{ position: 'absolute', left: x - 13, top: y - 13, width: 26, height: 26, borderRadius: '50%', border: '2px solid var(--warn)', pointerEvents: 'none' }} />;
}

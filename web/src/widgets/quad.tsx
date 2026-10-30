// Quad: rectangle, diamond, parallelogram or trapezoid.
// Same picture as crates/cg-render/src/wid/quad.rs; geometry mirrors crates/cg-core/src/wid/quad.rs.
import { Square } from 'lucide-react';
import { num } from '../engine/evaluate';
import { GroupRow } from '../designer/Inspector';
import { clamp, defineWidget, Handle, Num, Section, Seg, Toggle, type Warning, withDefaults } from './kit';
import { FillStrokeSections, roundedPolygon, ShapeSvg, type FillStyle, type Pt, type StrokeStyle } from './shape';
import { t } from '../i18n';

export interface Config {
  shape?: 'rect' | 'diamond' | 'parallelogram' | 'trapezoid';
  perCorner?: boolean;
  /** Top left, top right, bottom right, bottom left. */
  corners?: [number, number, number, number];
  skewUnit?: 'px' | 'deg';
  skewPx?: number;
  skewDeg?: number;
  trapMode?: 'percent' | 'offsets';
  trapTop?: number;
  trapLeft?: number;
  trapRight?: number;
  vertexRound?: number;
  fillStyle?: FillStyle;
  strokeStyle?: StrokeStyle;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface State {}

const D: Required<Omit<Config, 'fillStyle' | 'strokeStyle'>> = {
  shape: 'rect',
  perCorner: false,
  corners: [0, 0, 0, 0],
  skewUnit: 'px',
  skewPx: 120,
  skewDeg: 15,
  trapMode: 'percent',
  trapTop: 70,
  trapLeft: 80,
  trapRight: 200,
  vertexRound: 0,
};
const cfg = (c: Config) => withDefaults(D, c);

export const maxSkew = (w: number) => Math.max(0, w - 1);

export function skew(c: Config, w: number, h: number): number {
  const k = cfg(c);
  const off = k.skewUnit === 'deg' ? Math.tan((k.skewDeg * Math.PI) / 180) * h : k.skewPx;
  return clamp(off, -maxSkew(w), maxSkew(w));
}

/** The four corners, parameters capped so the sides never cross (mirrors corners()). */
export function corners(c: Config, w: number, h: number): Pt[] {
  const k = cfg(c);
  switch (k.shape) {
    case 'diamond':
      return [
        [w / 2, 0],
        [w, h / 2],
        [w / 2, h],
        [0, h / 2],
      ];
    case 'parallelogram': {
      const off = skew(c, w, h);
      return off >= 0
        ? [
            [off, 0],
            [w, 0],
            [w - off, h],
            [0, h],
          ]
        : [
            [0, 0],
            [w + off, 0],
            [w, h],
            [-off, h],
          ];
    }
    case 'trapezoid': {
      let l: number, r: number;
      if (k.trapMode === 'percent') {
        const top = (clamp(k.trapTop, 0, 100) / 100) * w;
        l = r = (w - top) / 2;
      } else {
        l = clamp(k.trapLeft, 0, w);
        r = clamp(k.trapRight, 0, w - l);
      }
      return [
        [l, 0],
        [w - r, 0],
        [w, h],
        [0, h],
      ];
    }
    default:
      return [
        [0, 0],
        [w, 0],
        [w, h],
        [0, h],
      ];
  }
}

/** The outline; `radius`: the animated corner radius, `each`: the animated radius of each corner (Per corner; else the settings'). */
export function outline(c: Config, w: number, h: number, radius: number, each?: number[]): string {
  const k = cfg(c);
  const radii = k.shape !== 'rect' ? [k.vertexRound] : k.perCorner ? [...(each ?? k.corners)] : [radius];
  return roundedPolygon(corners(c, w, h), radii);
}

export default defineWidget({
  id: 'quad',
  type: 'rect',
  label: 'Quad',
  group: 'Shapes & tools',
  icon: Square,
  create: () => ({ props: { width: 500, height: 120 } }),
  ownsLook: true,
  Render: ({ layer, r, w, h, data }) => <ShapeSvg d={outline(layer, w, h, num(r.cornerRadius), [num(r.cornerTl), num(r.cornerTr), num(r.cornerBr), num(r.cornerBl)])} w={w} h={h} r={r} fill={layer.fillStyle} stroke={layer.strokeStyle} data={data} />,
  Inspector: ({ layer, edit }) => {
    const k = cfg(layer);
    return (
      <>
        <Section id="quad-shape" title={t('Shape')}>
          <Seg
            label={t('Type')}
            path="shape"
            value={k.shape}
            options={[
              ['rect', t('Rectangle')],
              ['diamond', t('Diamond')],
              ['parallelogram', t('Parallelogram')],
              ['trapezoid', t('Trapezoid')],
            ]}
          />
          {k.shape === 'rect' && (
            <>
              <Toggle label={t('Per corner')} path="perCorner" value={false} />
              {!k.perCorner ? (
                <GroupRow layer={layer} spec={{ label: t('Radius'), props: ['cornerRadius'], min: 0, suffix: 'px' }} />
              ) : (
                // Animatable, as the radius of all corners.
                (['cornerTl', 'cornerTr', 'cornerBr', 'cornerBl'] as const).map((p, i) => <GroupRow key={p} layer={layer} spec={{ label: t(['Top left', 'Top right', 'Bottom right', 'Bottom left'][i]), props: [p], min: 0, suffix: 'px' }} />)
              )}
            </>
          )}
          {k.shape === 'parallelogram' && (
            <>
              <Seg
                label={t('Slant')}
                path="skewUnit"
                value={k.skewUnit}
                options={[
                  ['px', 'px'],
                  ['deg', '°'],
                ]}
              />
              {k.skewUnit === 'px' ? <Num label={t('Offset')} path="skewPx" value={k.skewPx} min={-1000} max={1000} unit="px" /> : <Num label={t('Angle')} path="skewDeg" value={k.skewDeg} min={-80} max={80} unit="°" />}
            </>
          )}
          {k.shape === 'trapezoid' && (
            <>
              <Seg
                label={t('Top side')}
                path="trapMode"
                value={k.trapMode}
                options={[
                  ['percent', t('Width %')],
                  ['offsets', t('Offsets')],
                ]}
              />
              {k.trapMode === 'percent' ? (
                <Num label={t('Top width')} path="trapTop" value={k.trapTop} min={0} max={100} unit="%" />
              ) : (
                <>
                  <Num label={t('Left offset')} path="trapLeft" value={k.trapLeft} min={0} max={2000} unit="px" />
                  <Num label={t('Right offset')} path="trapRight" value={k.trapRight} min={0} max={2000} unit="px" />
                </>
              )}
            </>
          )}
          {k.shape !== 'rect' && <Num label={t('Radius')} path="vertexRound" value={k.vertexRound} min={0} max={300} unit="px" />}
        </Section>
        <FillStrokeSections layer={layer} id="quad" edit={edit} />
      </>
    );
  },
  Overlay: ({ layer, w, h, scale, edit }) => {
    const k = cfg(layer);
    if (k.shape === 'rect' && !k.perCorner) {
      const r = Math.min(num(layer.props.cornerRadius ?? 0), w / 2, h / 2);
      return <Handle x={r} y={0} scale={scale} label={t('Radius')} onDrag={(x) => edit((l) => void (l.props.cornerRadius = Math.max(0, Math.round(x))), `radius-${layer.id}`)} />;
    }
    if (k.shape === 'parallelogram') {
      const off = skew(layer, w, h);
      return (
        <Handle
          x={clamp(off, 0, w)}
          y={0}
          scale={scale}
          label={t('Slant')}
          onDrag={(x) =>
            edit((l) => {
              const v = clamp(Math.round(x), -maxSkew(w), maxSkew(w));
              l.skewPx = v;
              l.skewDeg = Math.round((Math.atan(v / h) * 180) / Math.PI);
            }, `skew-${layer.id}`)
          }
        />
      );
    }
    if (k.shape === 'trapezoid') {
      const p = corners(layer, w, h);
      const set = (side: 'l' | 'r', v: number) =>
        edit((l) => {
          const c = cfg(l);
          l.trapMode = 'offsets';
          if (c.trapMode === 'percent') l.trapLeft = l.trapRight = Math.round((w - (c.trapTop / 100) * w) / 2);
          if (side === 'l') l.trapLeft = clamp(Math.round(v), 0, w - (l.trapRight ?? 0));
          else l.trapRight = clamp(Math.round(v), 0, w - (l.trapLeft ?? 0));
        }, `trap-${layer.id}`);
      return (
        <>
          <Handle x={p[0][0]} y={0} scale={scale} label={t('Left offset')} onDrag={(x) => set('l', x)} />
          <Handle x={p[1][0]} y={0} scale={scale} label={t('Right offset')} onDrag={(x) => set('r', w - x)} />
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
    const radius = num(r?.cornerRadius ?? layer.props.cornerRadius ?? 0);
    const maxR = Math.min(w, h) / 2;
    if (k.shape === 'rect' && !k.perCorner && radius > maxR) out.push({ level: 'info', text: t('Radius {radius} px capped at {max} px (half the smallest side): the shape is a pill.', { radius: Math.round(radius), max: Math.round(maxR) }) });
    if (k.shape === 'parallelogram') {
      const off = k.skewUnit === 'deg' ? Math.tan((k.skewDeg * Math.PI) / 180) * h : k.skewPx;
      if (Math.abs(off) > maxSkew(w)) out.push({ level: 'info', text: t('Slant capped at {max} px: more would make the sides cross.', { max: maxSkew(w) }) });
    }
    if (k.shape === 'trapezoid' && k.trapMode === 'offsets' && k.trapLeft + k.trapRight > w) out.push({ level: 'info', text: t('Offsets capped: the top side cannot be shorter than 0.') });
    return out;
  },
});

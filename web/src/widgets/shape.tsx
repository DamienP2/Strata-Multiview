// What the shape widgets share (quad, polygon, ellipse, pen): fill and
// stroke styles, outlines as path commands, and their inspector sections.
// Mirror of crates/cg-core/src/wid/shape.rs and cg-render/src/wid/shape.rs.
import { col, cssColor, interpolateText, num, propValue, type Resolved } from '../engine/evaluate';
import type { Data, Layer } from '../engine/types';
import { ColorGroupRow, GroupRow, AssetPicker } from '../designer/Inspector';
import { useEffect, useState } from 'react';
import { getPath, hex, Num, Row, Section, Seg, Toggle, useBound, ListEditor } from './kit';
import { t } from '../i18n';

export type FillKind = 'none' | 'solid' | 'linear' | 'radial' | 'image';
export type ImageFill = 'contain' | 'cover' | 'stretch' | 'tile';
export interface FillStyle {
  kind?: FillKind;
  /** Second colour of a gradient. */
  color2?: string;
  angle?: number;
  /** Colours between the fill (0 %) and color2 (100 %). */
  stops?: { at?: number; color?: string }[];
  image?: string;
  fit?: ImageFill;
}
export interface StrokeStyle {
  position?: 'inside' | 'center' | 'outside';
  dash?: 'solid' | 'dashed' | 'dotted';
  join?: 'miter' | 'round' | 'bevel';
  cap?: 'butt' | 'round' | 'square';
}
export type FillRule = 'nonzero' | 'evenodd';
export const FILL: Required<FillStyle> = { kind: 'solid', color2: '#ff3b5c', angle: 90, stops: [], image: '', fit: 'cover' };
/** The gradient's stops in order (mirror of FillStyle::gradient); `c2`: the second colour (the animatable fill2). */
const gradientStops = (f: Required<FillStyle>, c1: string, c2: string) =>
  [
    { at: 0, color: c1 },
    ...f.stops
      .filter((s) => Number.isFinite(s.at ?? 50))
      .map((s) => ({ at: Math.min(1, Math.max(0, (s.at ?? 50) / 100)), color: hex(s.color ?? '#ffffff') }))
      .sort((a, b) => a.at - b.at),
    { at: 1, color: c2 },
  ].map((s, i) => <stop key={i} offset={s.at} stopColor={s.color} />);
export const STROKE: Required<StrokeStyle> = { position: 'inside', dash: 'solid', join: 'miter', cap: 'butt' };

export type Pt = [number, number];

/** SVG path of a closed polygon with rounded vertices, radii capped (mirrors rounded_polygon). */
export function roundedPolygon(pts: Pt[], radii: number[]): string {
  const n = pts.length;
  if (n < 2) return '';
  const seg: { a: Pt; b: Pt; c: Pt; d: number }[] = [];
  for (let i = 0; i < n; i++) {
    const p0 = pts[(i - 1 + n) % n];
    const p1 = pts[i];
    const p2 = pts[(i + 1) % n];
    const v0: Pt = [p0[0] - p1[0], p0[1] - p1[1]];
    const v2: Pt = [p2[0] - p1[0], p2[1] - p1[1]];
    const l0 = Math.hypot(...v0);
    const l2 = Math.hypot(...v2);
    if (l0 < 1e-6 || l2 < 1e-6) {
      seg.push({ a: p1, b: p1, c: p1, d: 0 });
      continue;
    }
    const u0: Pt = [v0[0] / l0, v0[1] / l0];
    const u2: Pt = [v2[0] / l2, v2[1] / l2];
    const cos = Math.min(1, Math.max(-1, u0[0] * u2[0] + u0[1] * u2[1]));
    const tan = Math.tan(Math.acos(cos) / 2);
    const r = Math.max(0, radii[i] ?? radii[radii.length - 1] ?? 0);
    const d = Math.min(tan > 1e-6 ? r / tan : 0, l0 / 2, l2 / 2);
    seg.push({ a: [p1[0] + u0[0] * d, p1[1] + u0[1] * d], b: [p1[0] + u2[0] * d, p1[1] + u2[1] * d], c: p1, d });
  }
  const k = 0.5523;
  let path = `M${seg[0].b[0]} ${seg[0].b[1]}`;
  for (let j = 1; j <= n; j++) {
    const s = seg[j % n];
    path += `L${s.a[0]} ${s.a[1]}`;
    if (s.d > 0.01) {
      const c1 = [s.a[0] + (s.c[0] - s.a[0]) * k, s.a[1] + (s.c[1] - s.a[1]) * k];
      const c2 = [s.b[0] + (s.c[0] - s.b[0]) * k, s.b[1] + (s.c[1] - s.b[1]) * k];
      path += `C${c1[0]} ${c1[1]} ${c2[0]} ${c2[1]} ${s.b[0]} ${s.b[1]}`;
    }
  }
  return path + 'Z';
}

let seq = 0;

/**
 * A filled and stroked outline as SVG (w × h, layer pixels): fill from the
 * `fill` property (+ style), stroke from `stroke` / `strokeWidth` inside,
 * centred on or outside the outline.
 */
export function ShapeSvg({ d, w, h, r, fill, stroke, rule = 'nonzero', data }: { d: string; w: number; h: number; r: Resolved; fill?: FillStyle; stroke?: StrokeStyle; rule?: FillRule; data: Data }) {
  const id = `ws${(seq = (seq + 1) % 1e6)}`;
  const f = { ...FILL, ...fill };
  const st = { ...STROKE, ...stroke };
  const c1 = cssColor(col(r.fill));
  const sw = num(r.strokeWidth);
  let defs = null;
  let paint = c1;
  if (f.kind === 'none') paint = 'none';
  else if (f.kind === 'linear') {
    const a = ((f.angle - 90) * Math.PI) / 180;
    const [dx, dy] = [Math.cos(a) / 2, Math.sin(a) / 2];
    const len = (w * Math.abs(dx) + h * Math.abs(dy)) * 2;
    defs = (
      <linearGradient id={`${id}f`} gradientUnits="userSpaceOnUse" x1={w / 2 - dx * len} y1={h / 2 - dy * len} x2={w / 2 + dx * len} y2={h / 2 + dy * len}>
        {gradientStops(f, c1, cssColor(col(r.fill2)))}
      </linearGradient>
    );
    paint = `url(#${id}f)`;
  } else if (f.kind === 'radial') {
    defs = (
      <radialGradient id={`${id}f`} gradientUnits="userSpaceOnUse" cx={w / 2} cy={h / 2} r={Math.hypot(w, h) / 2}>
        {gradientStops(f, c1, cssColor(col(r.fill2)))}
      </radialGradient>
    );
    paint = `url(#${id}f)`;
  } else if (f.kind === 'image') {
    const src = interpolateText(f.image, data);
    const ar = { contain: 'xMidYMid meet', cover: 'xMidYMid slice', stretch: 'none', tile: 'xMidYMid meet' }[f.fit];
    defs =
      f.fit === 'tile' ? (
        <TilePattern id={`${id}f`} src={src} />
      ) : (
        <pattern id={`${id}f`} patternUnits="userSpaceOnUse" width={w} height={h}>
          <image href={src} width={w} height={h} preserveAspectRatio={ar} />
        </pattern>
      );
    paint = src ? `url(#${id}f)` : 'none';
  }
  const dash = st.dash === 'dashed' ? (k: number) => `${sw * k * 3} ${sw * k * 2}` : st.dash === 'dotted' ? (k: number) => `0.001 ${sw * k * 2}` : () => undefined;
  const strokeEl = (k: number, extra: Record<string, string>) => <path d={d} fill="none" stroke={cssColor(col(r.stroke))} strokeWidth={sw * k} strokeLinejoin={st.join} strokeLinecap={st.dash === 'dotted' ? 'round' : st.cap} strokeDasharray={dash(k)} {...extra} />;
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} style={{ position: 'absolute', inset: 0, overflow: 'visible' }}>
      <defs>
        {defs}
        {sw > 0 && st.position === 'inside' && (
          <clipPath id={`${id}c`}>
            <path d={d} clipRule={rule} />
          </clipPath>
        )}
        {sw > 0 && st.position === 'outside' && (
          <mask id={`${id}m`} maskUnits="userSpaceOnUse" x={-5000} y={-5000} width={10000 + w} height={10000 + h}>
            <rect x={-5000} y={-5000} width={10000 + w} height={10000 + h} fill="#fff" />
            <path d={d} fill="#000" fillRule={rule} />
          </mask>
        )}
      </defs>
      <path d={d} fill={paint} fillRule={rule} />
      {sw > 0 && st.position === 'center' && strokeEl(1, {})}
      {sw > 0 && st.position === 'inside' && strokeEl(2, { clipPath: `url(#${id}c)` })}
      {sw > 0 && st.position === 'outside' && strokeEl(2, { mask: `url(#${id}m)` })}
    </svg>
  );
}

/** A picture repeated at its own size (tile fill). */
function TilePattern({ id, src }: { id: string; src: string }) {
  const [size] = useImageSize(src);
  return (
    <pattern id={id} patternUnits="userSpaceOnUse" width={size[0]} height={size[1]}>
      <image href={src} width={size[0]} height={size[1]} />
    </pattern>
  );
}

const sizes = new Map<string, [number, number]>();
/** Natural size of a picture (100 × 100 until it has loaded). */
export function useImageSize(src: string): [[number, number], (s: [number, number]) => void] {
  const [s, set] = useState<[number, number]>(sizes.get(src) ?? [100, 100]);
  useEffect(() => {
    if (!src || sizes.has(src)) return;
    const img = new Image();
    img.onload = () => {
      sizes.set(src, [img.naturalWidth, img.naturalHeight]);
      set([img.naturalWidth, img.naturalHeight]);
    };
    img.src = src;
  }, [src]);
  return [sizes.get(src) ?? s, set];
}

/** The Stroke toggle of a shape: on gives a visible stroke (4 px, black when its colour is transparent), off sets the width to 0. */
export function StrokeToggle({ layer, edit, width = 4 }: { layer: Layer; edit: ShapeEdit; width?: number }) {
  return (
    <Toggle
      // The width is exposed by its own row (Width): the toggle is not.
      expose={false}
      label={t('Stroke')}
      value={num(propValue(layer, 'strokeWidth', 0)) > 0}
      onChange={(v) =>
        edit((l) => {
          l.props.strokeWidth = v ? width : 0;
          if (v && col(l.props.stroke ?? [0, 0, 0, 0])[3] === 0) l.props.stroke = [0, 0, 0, 1];
        })
      }
    />
  );
}

export type ShapeEdit = (fn: (l: { props: Layer['props'] }) => void) => void;

/**
 * Fill and Stroke groups of the inspector: the colours and the width are
 * the layer's animated properties (keyframes), the rest is `fillStyle` /
 * `strokeStyle` of the layer.
 */
export function FillStrokeSections({ layer, id, edit, rule, caps, noPosition, joins = true }: { layer: Layer; id: string; edit: ShapeEdit; rule?: boolean; caps?: boolean; noPosition?: boolean; joins?: boolean }) {
  const f = { ...FILL, ...(layer as { fillStyle?: FillStyle }).fillStyle };
  const stroke = num(propValue(layer, 'strokeWidth', 0)) > 0;
  return (
    <>
      <Section id={`${id}-fill`} title={t('Fill')}>
        <Seg
          label={t('Type')}
          path="fillStyle.kind"
          value={f.kind}
          options={[
            ['none', t('None')],
            ['solid', t('Solid')],
            ['linear', t('Linear')],
            ['radial', t('Radial')],
            ['image', t('Image')],
          ]}
        />
        {f.kind !== 'none' && f.kind !== 'image' && <ColorGroupRow layer={layer} prop="fill" label={f.kind === 'solid' ? t('Color') : t('From')} />}
        {(f.kind === 'linear' || f.kind === 'radial') && <GradientTo layer={layer} />}
        {f.kind === 'linear' && <Num label={t('Angle')} path="fillStyle.angle" value={FILL.angle} min={0} max={360} unit="°" />}
        {f.kind === 'image' && <ImageSource />}
        {f.kind === 'image' && (
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
        )}
        {rule && (
          <Seg
            label={t('Fill rule')}
            path="fillRule"
            value="nonzero"
            options={[
              ['nonzero', t('Non-zero')],
              ['evenodd', t('Even-odd')],
            ]}
          />
        )}
      </Section>
      <Section id={`${id}-stroke`} title={t('Stroke')}>
        <StrokeToggle layer={layer} edit={edit} />
        {stroke && <StrokeRows layer={layer} caps={caps} joins={joins} position={!noPosition} />}
      </Section>
    </>
  );
}

/** Colour, width and style rows of a stroke. */
export function StrokeRows({ layer, caps, joins = true, position, capDefault = STROKE.cap, joinDefault = STROKE.join }: { layer: Layer; caps?: boolean; joins?: boolean; position?: boolean; capDefault?: StrokeStyle['cap']; joinDefault?: StrokeStyle['join'] }) {
  return (
    <>
      <ColorGroupRow layer={layer} prop="stroke" label={t('Color')} />
      <GroupRow layer={layer} spec={{ label: t('Width'), props: ['strokeWidth'], min: 0, suffix: 'px' }} />
      {position && (
        <Seg
          label={t('Position')}
          path="strokeStyle.position"
          value={STROKE.position}
          options={[
            ['inside', t('Inside')],
            ['center', t('Centre')],
            ['outside', t('Outside')],
          ]}
        />
      )}
      <Seg
        label={t('Style')}
        path="strokeStyle.dash"
        value={STROKE.dash}
        options={[
          ['solid', t('Solid')],
          ['dashed', t('Dashes')],
          ['dotted', t('Dots')],
        ]}
      />
      {joins && (
        <Seg
          label={t('Joins')}
          path="strokeStyle.join"
          value={joinDefault}
          options={[
            ['miter', t('Miter')],
            ['round', t('Round')],
            ['bevel', t('Bevel')],
          ]}
        />
      )}
      {caps && (
        <Seg
          label={t('Ends')}
          path="strokeStyle.cap"
          value={capDefault}
          options={[
            ['butt', t('Flat')],
            ['round', t('Round')],
            ['square', t('Square')],
          ]}
        />
      )}
    </>
  );
}

/** The second colour of a gradient (animatable) and the colours between. */
export function GradientTo({ layer }: { layer: Layer }) {
  return (
    <>
      <ColorGroupRow layer={layer} prop="fill2" label={t('To')} />
      <ListEditor
        label={t('Between')}
        path="fillStyle.stops"
        value={[]}
        fields={[
          { key: 'color', color: true, w: 44 },
          { key: 'at', num: true, placeholder: '%', w: 60 },
        ]}
        add={() => ({ at: 50, color: '#ffffff' })}
        addLabel={t('Colour')}
      />
    </>
  );
}

/** Picture of an image fill (from the media library). */
function ImageSource() {
  const b = useBound();
  return (
    <Row expose="fillStyle.image" exposeValue={String(getPath(b.obj, 'fillStyle.image') ?? '')} label={t('Picture')}>
      <AssetPicker value={String(getPath(b.obj, 'fillStyle.image') ?? '')} onChange={(v) => b.set('fillStyle.image', v)} />
    </Row>
  );
}

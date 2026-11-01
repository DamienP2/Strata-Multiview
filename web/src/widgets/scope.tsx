// The picture measuring tools of a multiview: waveform, RGB and Y'CbCr parades, vectorscope,
// histogram, CIE 1931 chromaticity and false colour, of a live input. The trace is made by the
// server from the input's pictures (crates/strata-mv-render/src/scopes.rs) and sent here
// (engine/scopes.ts, one WebSocket for every scope of the page), screen blended over the background; the graticule and labels are laid out
// as in crates/strata-mv-core/src/wid/scope.rs (mirrored here: keep in sync) and drawn as items, as
// the Rust engine does (crates/strata-mv-render/src/wid/scope.rs).
import { ChartColumn, Columns3, Diamond, Grid3x3, Layers, Palette, Radar, Ruler, ScanEye, Triangle, Waves } from 'lucide-react';
import { useInputs, useSignal } from '../engine/liveInputs';
import { useScope } from '../engine/scopes';
import type { Panel } from '../engine/types';
import { t } from '../i18n';
import { circle, Items, path, rect, text, type Item } from './broadcast';
import { ColorPick, defineWidget, Flags, Num, Section, Seg, Select, withDefaults } from './kit';

type Area = [number, number, number, number];
type Pt = [number, number];

export type ScopeKind = 'waveform' | 'parade' | 'ycbcrParade' | 'vector' | 'histogram' | 'cie' | 'falseColor' | 'goniometer';
export type Transfer = 'sdr' | 'pq' | 'hlg';

export interface ScopeConfig {
  input?: string;
  kind?: ScopeKind;
  graticule?: boolean;
  labels?: boolean;
  intensity?: number;
  color?: string;
  background?: string;
}
const D_SCOPE = { input: '', kind: 'waveform' as ScopeKind, graticule: true, labels: true, intensity: 1, color: '#6dff8f', background: '#05070b' };

const GRID = '#ffffff38';
const GRID_STRONG = '#ffffff70';
const LABEL = '#ffffffb0';
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));

/** From ffmpeg's name of a transfer (mirrors Transfer::of). */
export const transferOf = (name = ''): Transfer => {
  const n = name.toLowerCase();
  return n.includes('2084') || n === 'pq' ? 'pq' : n.includes('b67') || n.includes('hlg') ? 'hlg' : 'sdr';
};

/** PQ (ST 2084): the signal (0–1) of `nits` (mirrors pq_signal). */
export function pqSignal(nits: number): number {
  const [m1, m2, c1, c2, c3] = [0.15930176, 78.84375, 0.8359375, 18.851562, 18.6875];
  const y = Math.pow(clamp(nits / 10000, 0, 1), m1);
  return Math.pow((c1 + c2 * y) / (1 + c3 * y), m2);
}

/** The lines of a level scale and their labels (mirrors levels). */
export function levels(tr: Transfer): [number, string, boolean][] {
  if (tr === 'pq') return [0, 1, 10, 100, 203, 1000, 4000, 10000].map((n) => [pqSignal(n), n >= 1000 ? `${n / 1000}k` : String(n), n === 203 || n === 100]);
  if (tr === 'hlg') return [0, 25, 50, 75, 100].map((p) => [p / 100, String(p), p === 75]);
  return [0, 25, 50, 75, 100].map((p) => [p / 100, String(p), p === 0 || p === 100]);
}
const unit = (tr: Transfer) => (tr === 'pq' ? 'nits' : tr === 'hlg' ? '% HLG' : '%');

/** Where the trace goes (mirrors trace_area). */
export function traceArea(c: Required<ScopeConfig>, w: number, h: number): Area {
  const pad = clamp(Math.min(w, h) * 0.04, 2, 16);
  const left = c.labels && !['vector', 'cie', 'falseColor', 'goniometer'].includes(c.kind) ? clamp(h * 0.09, 18, 48) : 0;
  const top = c.labels && (c.kind === 'parade' || c.kind === 'ycbcrParade') ? clamp(h * 0.07, 10, 28) : 0;
  const bottom = c.labels && c.kind === 'histogram' ? clamp(h * 0.07, 10, 28) : c.kind === 'goniometer' ? correlationHeight(h) : 0;
  const right = c.kind === 'falseColor' && c.labels ? clamp(w * 0.1, 28, 90) : 0;
  const a: Area = [pad + left, pad + top, Math.max(1, w - 2 * pad - left - right), Math.max(1, h - 2 * pad - top - bottom)];
  if (c.kind === 'vector' || c.kind === 'cie' || c.kind === 'goniometer') {
    const s = Math.min(a[2], a[3]);
    return [a[0] + (a[2] - s) / 2, a[1] + (a[3] - s) / 2, s, s];
  }
  return a;
}

/** The goniometer's correlation meter height (mirrors correlation_height). */
const correlationHeight = (h: number) => clamp(h * 0.12, 16, 44);
/** Out of phase red, wide amber, in phase green (mirrors correlation_color). */
export const correlationColor = (v: number) => (v < 0 ? '#ff3b3b' : v < 0.3 ? '#fbbf24' : '#2bd96b');

/** The parades' three panels (mirrors panels). */
export function panels(a: Area): [Area, Area, Area] {
  const gap = Math.max(2, a[2] * 0.02);
  const w = (a[2] - 2 * gap) / 3;
  return [0, 1, 2].map((i) => [a[0] + i * (w + gap), a[1], w, a[3]] as Area) as [Area, Area, Area];
}
const levelY = (v: number, a: Area) => a[1] + a[3] * (1 - v);
function vectorPoint(cb: number, cr: number, a: Area): Pt {
  const r = (Math.min(a[2], a[3]) / 2) * 0.92;
  const [cx, cy] = [a[0] + a[2] / 2, a[1] + a[3] / 2];
  return [cx + cb * 1.85 * r, cy - cr * 1.85 * r];
}
function ycbcr(r: number, g: number, b: number): [number, number, number] {
  const y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return [y, (b - y) / 1.8556, (r - y) / 1.5748];
}
const ciePoint = (x: number, y: number, a: Area): Pt => [a[0] + (x / 0.8) * a[2], a[1] + a[3] - (y / 0.9) * a[3]];

/** The spectral locus, 380 to 700 nm (mirrors LOCUS). */
const LOCUS: Pt[] = [
  [0.1741, 0.005], [0.174, 0.005], [0.1738, 0.0049], [0.1733, 0.0048], [0.1726, 0.0048], [0.1714, 0.0051], [0.1689, 0.0069], [0.1644, 0.0109], [0.1566, 0.0177], [0.144, 0.0297], [0.1241, 0.0578],
  [0.0913, 0.1327], [0.0687, 0.2007], [0.0454, 0.295], [0.0235, 0.4127], [0.0082, 0.5384], [0.0039, 0.6548], [0.0139, 0.7502], [0.0389, 0.812], [0.0743, 0.8338], [0.1142, 0.8262], [0.1547, 0.8059],
  [0.1929, 0.7816], [0.2296, 0.7543], [0.2658, 0.7243], [0.3016, 0.6923], [0.3373, 0.6589], [0.3731, 0.6245], [0.4087, 0.5896], [0.4441, 0.5547], [0.4788, 0.5202], [0.5125, 0.4866], [0.7347, 0.2653],
];
const GAMUTS: [string, Pt[], string][] = [
  ['709', [[0.64, 0.33], [0.3, 0.6], [0.15, 0.06]], '#ffffffc0'],
  ['P3', [[0.68, 0.32], [0.265, 0.69], [0.15, 0.06]], '#ffd34ec0'],
  ['2020', [[0.708, 0.292], [0.17, 0.797], [0.131, 0.046]], '#4ed8ffc0'],
];

/** False colour bands (mirrors false_colors). */
function falseColors(tr: Transfer): [number, string, string][] {
  if (tr === 'pq')
    return [
      [pqSignal(0.5), '#5a2ca0', '<0.5'], [pqSignal(5), '#1f5fd6', '<5'], [pqSignal(50), '#3a3a3a', '<50'], [pqSignal(120), '#7a7a7a', '<120'],
      [pqSignal(250), '#2fb34a', '≈203'], [pqSignal(1000), '#f2d13a', '<1k'], [pqSignal(4000), '#ff8a1f', '<4k'], [1.01, '#ff2a2a', '4k+'],
    ];
  return [
    [0.025, '#5a2ca0', '<2.5'], [0.1, '#1f5fd6', '<10'], [0.4, '#3a3a3a', '<40'], [0.5, '#2fb34a', '40–50'], [0.7, '#7a7a7a', '<70'],
    [0.8, '#ff9fc2', '70–80'], [0.97, '#bdbdbd', '<97'], [0.995, '#f2d13a', '97–100'], [1.01, '#ff2a2a', '100'],
  ];
}

/** The graticule and labels: under the trace, over it (mirrors layout). */
export function layout(c: Required<ScopeConfig>, w: number, h: number, tr: Transfer, correlation?: number): [Item[], Item[]] {
  const under: Item[] = [rect([0, 0, w, h], 0, c.background)];
  const over: Item[] = [];
  const a = traceArea(c, w, h);
  const size = clamp(h * 0.045, 8, 18);
  const lw = clamp(h / 400, 1, 2);
  const label = (box: Area, s: string, align: 'left' | 'center' | 'right', color = LABEL) => over.push(text(box, s, size, 600, true, color, align));
  if (c.kind === 'waveform' || c.kind === 'parade' || c.kind === 'ycbcrParade') {
    const parts = c.kind === 'waveform' ? [a] : panels(a);
    if (c.graticule)
      for (const [v, name, strong] of levels(tr)) {
        const y = levelY(v, a);
        for (const p of parts) over.push(path([[p[0], y], [p[0] + p[2], y]], false, lw, strong ? GRID_STRONG : GRID));
        if (c.labels) label([0, y - size * 0.6, a[0] - 4, size * 1.2], name, 'right');
      }
    if (c.labels && c.kind !== 'waveform') {
      const names: [string, string][] = c.kind === 'parade' ? [['R', '#ff6b6b'], ['G', '#6bff8a'], ['B', '#6ba8ff']] : [["Y'", '#ffffff'], ['Cb', '#6ba8ff'], ['Cr', '#ff6b6b']];
      parts.forEach((p, i) => over.push(text([p[0], a[1] - size * 1.4, p[2], size * 1.3], names[i][0], size, 700, false, names[i][1], 'center')));
    }
    if (c.labels) label([a[0] + a[2] - 80, a[1], 78, size * 1.2], unit(tr), 'right');
  } else if (c.kind === 'histogram') {
    if (c.graticule)
      for (const [v, name, strong] of levels(tr)) {
        const x = a[0] + a[2] * v;
        over.push(path([[x, a[1]], [x, a[1] + a[3]]], false, lw, strong ? GRID_STRONG : GRID));
        if (c.labels) label([x - 30, a[1] + a[3] + 2, 60, size * 1.2], name, 'center');
      }
  } else if (c.kind === 'vector') {
    const center: Pt = [a[0] + a[2] / 2, a[1] + a[3] / 2];
    const r = (Math.min(a[2], a[3]) / 2) * 0.92;
    if (c.graticule) {
      over.push(circle(center, r, lw, GRID_STRONG));
      over.push(circle(center, r * 0.5, lw, GRID));
      over.push(path([[center[0] - r, center[1]], [center[0] + r, center[1]]], false, lw, GRID));
      over.push(path([[center[0], center[1] - r], [center[0], center[1] + r]], false, lw, GRID));
      const ang = (123 * Math.PI) / 180;
      over.push(path([center, [center[0] + r * Math.cos(ang), center[1] - r * Math.sin(ang)]], false, lw, '#ffb07080'));
      const targets: [string, [number, number, number]][] = [['R', [1, 0, 0]], ['MG', [1, 0, 1]], ['B', [0, 0, 1]], ['CY', [0, 1, 1]], ['G', [0, 1, 0]], ['YL', [1, 1, 0]]];
      for (const [name, rgb] of targets)
        for (const [k, s] of [[0.75, 0.06], [1, 0.035]]) {
          const [, cb, cr] = ycbcr(rgb[0] * k, rgb[1] * k, rgb[2] * k);
          const p = vectorPoint(cb, cr, a);
          const d = r * s;
          over.push(path([[p[0] - d, p[1] - d], [p[0] + d, p[1] - d], [p[0] + d, p[1] + d], [p[0] - d, p[1] + d]], true, lw, k < 1 ? GRID_STRONG : GRID));
          if (k < 1 && c.labels) {
            const [, cb2, cr2] = ycbcr(rgb[0] * 1.12, rgb[1] * 1.12, rgb[2] * 1.12);
            const q = vectorPoint(cb2, cr2, a);
            label([q[0] - 20, q[1] - size * 0.6, 40, size * 1.2], name, 'center');
          }
        }
    }
  } else if (c.kind === 'cie') {
    if (c.graticule) {
      const locus = LOCUS.map(([x, y]) => ciePoint(x, y, a)).filter((p, i, all) => i === 0 || p[0] !== all[i - 1][0] || p[1] !== all[i - 1][1]);
      over.push(path(locus, true, lw, GRID_STRONG));
      GAMUTS.forEach(([name, prim, col], i) => {
        over.push(path(prim.map(([x, y]) => ciePoint(x, y, a)), true, lw, col));
        if (c.labels) label([a[0] + a[2] - 70, a[1] + size * 1.3 * i, 68, size * 1.2], name, 'right', col);
      });
      over.push(circle(ciePoint(0.3127, 0.329, a), Math.max(2, a[2] * 0.008), lw, '#ffffff'));
    }
  } else if (c.kind === 'goniometer') {
    const center: Pt = [a[0] + a[2] / 2, a[1] + a[3] / 2];
    const r = (Math.min(a[2], a[3]) / 2) * 0.95;
    if (c.graticule) {
      const k = r / Math.SQRT2;
      const corner = (dx: number, dy: number): Pt => [center[0] + dx * r, center[1] + dy * r];
      over.push(path([corner(0, -1), corner(1, 0), corner(0, 1), corner(-1, 0)], true, lw, GRID_STRONG));
      over.push(path([corner(0, -1), corner(0, 1)], false, lw, GRID));
      over.push(path([corner(-1, 0), corner(1, 0)], false, lw, GRID));
      over.push(path([[center[0] - k, center[1] - k], [center[0] + k, center[1] + k]], false, lw, GRID));
      over.push(path([[center[0] + k, center[1] - k], [center[0] - k, center[1] + k]], false, lw, GRID));
      if (c.labels) {
        const d = k + size * 0.7;
        for (const [name, x, y] of [
          ['L', center[0] - d, center[1] - d],
          ['R', center[0] + d, center[1] - d],
          ['M', center[0], center[1] - r - size * 0.2],
          ['S', center[0] + r + size * 0.6, center[1]],
        ] as const)
          label([x - 20, y - size * 0.6, 40, size * 1.2], name, 'center');
      }
    }
    const mh = correlationHeight(h);
    const pad = clamp(Math.min(w, h) * 0.04, 2, 16);
    const lab = c.labels ? size * 2.2 : 0;
    const bar: Area = [pad + lab, h - pad - mh * 0.62, Math.max(1, w - 2 * pad - 2 * lab), mh * 0.36];
    under.push(rect(bar, bar[3] / 2, '#ffffff14'));
    const mid = bar[0] + bar[2] / 2;
    over.push(rect([mid - 0.5, bar[1] - 2, 1, bar[3] + 4], 0, GRID_STRONG));
    if (correlation !== undefined) {
      const v = clamp(correlation, -1, 1);
      const x = mid + (v * bar[2]) / 2;
      const [x0, x1] = v < 0 ? [x, mid] : [mid, x];
      over.push(rect([x0, bar[1], Math.max(1, x1 - x0), bar[3]], bar[3] / 2, correlationColor(v)));
      over.push(rect([x - 1.5, bar[1] - 3, 3, bar[3] + 6], 1, '#ffffff'));
    }
    if (c.labels) {
      label([pad, bar[1] - size * 0.4, lab - 4, bar[3] + size * 0.8], '−1', 'right');
      label([bar[0] + bar[2] + 4, bar[1] - size * 0.4, lab - 4, bar[3] + size * 0.8], '+1', 'left');
      let value = '—';
      if (correlation !== undefined) {
        const v = Math.round(clamp(correlation, -1, 1) * 100) / 100;
        value = v < 0 ? `−${(-v).toFixed(2)}` : v.toFixed(2);
      }
      over.push(text([w - pad - 90, pad, 88, size * 1.3], value, size * 1.1, 700, true, correlation === undefined ? LABEL : correlationColor(correlation), 'right'));
    }
  } else if (c.labels) {
    const bands = falseColors(tr);
    const x = a[0] + a[2] + 6;
    const bw = w - x - 4;
    const bh = Math.min(a[3] / bands.length, size * 1.6);
    [...bands].reverse().forEach(([, col, name], i) => {
      const y = a[1] + i * bh;
      under.push(rect([x, y + 1, bw * 0.3, bh - 2], 2, col));
      label([x + bw * 0.34, y, bw * 0.66, bh], name, 'left');
    });
  }
  return [under, over];
}

/** Sizes asked of the server in steps (a small change of size keeps the stream). */
const step = (v: number) => clamp(Math.round(v / 16) * 16, 16, 1024);

function ScopeView({ cfg, w, h }: { cfg: ScopeConfig; w: number; h: number }) {
  const c = withDefaults(D_SCOPE, cfg) as Required<ScopeConfig>;
  const id = c.input.trim();
  const signal = useSignal(id);
  const tr = transferOf(signal.video?.transfer);
  const [under, over] = layout(c, w, h, tr, signal.correlation ?? undefined);
  const a = traceArea(c, w, h);
  const url = useScope(id ? { input: id, kind: c.kind, w: step(a[2]), h: step(a[3]), i: Math.round(c.intensity * 100), color: c.color.replace('#', '').slice(0, 6) } : null);
  const look = (cfg as { panel?: Panel }).panel;
  return (
    <>
      <Items items={under} look={look} />
      {url && (
        <img
          src={url}
          alt=""
          draggable={false}
          style={{ position: 'absolute', left: a[0], top: a[1], width: a[2], height: a[3], objectFit: c.kind === 'falseColor' ? 'contain' : 'fill', mixBlendMode: c.kind === 'falseColor' ? 'normal' : 'screen', pointerEvents: 'none' }}
        />
      )}
      <Items items={over} look={look} />
    </>
  );
}

const KINDS: [ScopeKind, string][] = [
  ['waveform', 'Waveform (luma)'],
  ['parade', 'RGB parade'],
  ['ycbcrParade', "Y'CbCr parade"],
  ['vector', 'Vectorscope'],
  ['histogram', 'Histogram'],
  ['cie', 'CIE 1931 chromaticity'],
  ['falseColor', 'False colour'],
  ['goniometer', 'Goniometer and phase'],
];

const KIND_ICONS: Record<ScopeKind, React.ReactNode> = {
  waveform: <Waves size={14} />,
  parade: <Columns3 size={14} />,
  ycbcrParade: <Layers size={14} />,
  vector: <Radar size={14} />,
  histogram: <ChartColumn size={14} />,
  cie: <Triangle size={14} />,
  falseColor: <Palette size={14} />,
  goniometer: <Diamond size={14} />,
};

export const scopeWidget = defineWidget({
  id: 'scope',
  type: 'scope',
  label: 'Scope',
  group: 'Multiview',
  icon: ScanEye,
  create: () => ({ kind: 'waveform', props: { width: 640, height: 360 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h }) => <ScopeView cfg={layer as ScopeConfig} w={w} h={h} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_SCOPE, layer as ScopeConfig);
    const inputs = useInputs();
    return (
      <Section id="scope" title={t('Scope')}>
        <Select label={t('Input')} value={c.input} onChange={(v) => set('input', v)} options={[['', t('None')] as [string, string], ...inputs.map((i) => [i.id, i.name] as [string, string])]} />
        <Seg label={t('Tool')} value={c.kind} onChange={(v) => set('kind', v)} options={KINDS.map(([k, l]) => [k, KIND_ICONS[k], t(l)] as const)} />
        <Flags
          label={t('Show')}
          items={[
            { icon: <Grid3x3 size={14} />, title: t('Graticule'), value: c.graticule, onChange: (v) => set('graticule', v) },
            { icon: <Ruler size={14} />, title: t('Values: the scale and its unit'), value: c.labels, onChange: (v) => set('labels', v) },
          ]}
        />
        <Num label={t('Intensity')} value={c.intensity} min={0.25} max={4} step={0.05} precision={2} onChange={(v) => set('intensity', v)} />
        <ColorPick label={t('Trace')} value={c.color} onChange={(v) => set('color', v)} />
      </Section>
    );
  },
});

export default scopeWidget;

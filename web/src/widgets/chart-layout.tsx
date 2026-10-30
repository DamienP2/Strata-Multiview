// Chart layout: the picture as shapes and texts in layer pixels. Mirror of
// crates/cg-core/src/wid/chart.rs + chart_layout.rs (keep in sync line by line)
// so that the Rust and the web renderers draw the same chart.
import { interpolateText } from '../engine/evaluate';
import type { Data } from '../engine/types';
import type { AdvanceMode } from './centre';
import { parseCsv } from './kit';
import { roundedPolygon } from './shape';
import { parseRows } from './table';

export type ChartType = 'columns' | 'areas' | 'lines' | 'pie';
export type Marker = 'none' | 'circle' | 'square' | 'diamond';
export type RevealMode = 'all' | 'series' | 'categories';
export type Fx = 'auto' | 'grow' | 'draw' | 'sweep' | 'fade' | 'none';
export type HlKind = 'none' | 'series' | 'category';

export interface Axis {
  auto?: boolean;
  min?: number;
  max?: number;
  ticks?: number;
  decimals?: number;
  prefix?: string;
  suffix?: string;
  sep?: 'space' | 'comma' | 'none';
  grid?: boolean;
  vgrid?: boolean;
  zero?: boolean;
  xTitle?: string;
  yTitle?: string;
}
export interface SeriesStyle {
  color?: string;
  width?: number;
  marker?: Marker;
}
export interface Pie {
  series?: number;
  start?: number;
  sort?: 'none' | 'desc' | 'asc';
  donut?: number;
  explode?: number;
  explodeDist?: number;
  others?: boolean;
  othersPct?: number;
  centerTotal?: boolean;
}
export interface Labels {
  value?: boolean;
  pct?: boolean;
  name?: boolean;
  pos?: 'inside' | 'outside';
  overlap?: 'offset' | 'hide';
  minSize?: number;
}
export interface Config {
  chartType?: ChartType;
  colMode?: 'grouped' | 'stacked' | 'percent';
  areaMode?: 'simple' | 'stacked';
  source?: 'grid' | 'csv';
  /** First row: header (categories title, series names); first column: categories. Cells may hold {{key}}. */
  data?: string[][];
  /** CSV text, or a {{key}} whose data value is CSV. */
  csv?: string;
  missing?: 'gap' | 'interp' | 'zero';
  axis?: Axis;
  series?: SeriesStyle[];
  sliceColors?: string[];
  smooth?: boolean;
  areaOpacity?: number;
  pie?: Pie;
  labels?: Labels;
  title?: { text?: string; sub?: string; pos?: 'left' | 'center' | 'none' };
  legend?: 'top' | 'bottom' | 'right' | 'none';
  panel?: { on?: boolean; color?: string; opacity?: number };
  anim?: { fx?: Fx; dur?: number; stagger?: number; upd?: number; curve?: 'ease' | 'out' | 'linear' };
  reveal?: RevealMode;
  /** Columns: standing (v) or lying, names on the left (h). */
  orient?: 'v' | 'h';
  /** New data on air: tweened at once (auto) or kept until the operator's Reload. */
  onAir?: 'auto' | 'reload';
  /** Font family of the texts (weights by role). */
  font?: string;
  /** Colour of the texts (empty = white and lavender). */
  textColor?: string;
  refLine?: { on?: boolean; value?: number; label?: string };
  /** Texts the chart writes itself (empty = in the project's language). */
  texts?: { noData?: string; others?: string; total?: string };
  /** Sparkline: the first series alone filling the box (columns, line or area), without panel, titles, axes or texts. */
  spark?: boolean;
  /** The reveal steps are chained from the take: every `autoSecs` seconds, on Continue, or on a trigger (`autoOn`). */
  autoSteps?: boolean;
  autoSecs?: number;
  autoOn?: AdvanceMode;
  autoTrigger?: string;
  /** Put forward from the take: '' = nothing, `s2` = series 3, `c0` = the first category (column, point or slice). */
  hlDefault?: string;
}
export interface Hl {
  kind: HlKind;
  idx: number;
}
export interface PrevCat {
  name: string;
  slot: number;
  vals: (number | null)[];
}
/** Mirrors `State` of the Rust module. */
export interface State {
  cur?: string[][];
  prev?: PrevCat[];
  prevSeries?: string[];
  prevAxis?: [number, number];
  prevN?: number;
  at?: number;
  mode?: RevealMode;
  step?: number;
  stepAt?: number[];
  hl?: Hl;
  prevHl?: Hl;
  hlAt?: number;
  /** The operator acted (else the Designer's chained steps and highlight apply from the take). */
  ctl?: boolean;
  /** Steps chained: one more every `interval` s from `autoAt` (`on` timed), or on Continue / on the trigger (`interval` 0). */
  auto?: boolean;
  interval?: number;
  autoAt?: number;
  on?: AdvanceMode;
  trigger?: string;
  pausedAt?: number;
}

export const SAMPLE = [
  ['Month', 'Martin', 'Durand', 'Petit'],
  ['Jan', '28', '24', '12'],
  ['Feb', '30', '23', '13'],
  ['Mar', '27', '26', '14'],
  ['Apr', '31', '25', '11'],
  ['May', '33', '24', '12'],
  ['Jun', '32', '27', '10'],
];

const AXIS: Required<Axis> = { auto: true, min: 0, max: 50, ticks: 5, decimals: 0, prefix: '', suffix: ' %', sep: 'space', grid: true, vgrid: false, zero: true, xTitle: '', yTitle: '' };
const PIE: Required<Pie> = { series: 0, start: 0, sort: 'desc', donut: 0, explode: -1, explodeDist: 36, others: false, othersPct: 4, centerTotal: false };
const LABELS: Required<Labels> = { value: true, pct: false, name: false, pos: 'outside', overlap: 'offset', minSize: 3 };

/** Settings with every default (mirrors `Config::default`). */
export function cfg(c: Config) {
  return {
    chartType: c.chartType ?? 'columns',
    colMode: c.colMode ?? 'grouped',
    areaMode: c.areaMode ?? 'simple',
    source: c.source ?? 'grid',
    data: c.data ?? SAMPLE,
    csv: c.csv ?? '',
    missing: c.missing ?? 'gap',
    axis: { ...AXIS, ...c.axis },
    series: c.series ?? [],
    sliceColors: c.sliceColors ?? [],
    smooth: c.smooth ?? false,
    areaOpacity: c.areaOpacity ?? 0.45,
    pie: { ...PIE, ...c.pie },
    labels: { ...LABELS, ...c.labels },
    title: { text: 'Voting intentions', sub: 'Poll · first round · % of voters', pos: 'left' as const, ...c.title },
    legend: c.legend ?? 'top',
    panel: { on: true, color: '#120e24', opacity: 0.88, ...c.panel },
    anim: { fx: 'auto' as Fx, dur: 0.9, stagger: 110, upd: 0.8, curve: 'ease' as const, ...c.anim },
    reveal: c.reveal ?? 'all',
    orient: c.orient ?? 'v',
    onAir: c.onAir ?? 'reload',
    font: c.font ?? 'Inter',
    textColor: c.textColor ?? '',
    refLine: { on: false, value: 0, label: '', ...c.refLine },
    texts: { noData: '', others: '', total: '', ...c.texts },
    spark: c.spark ?? false,
    autoSteps: c.autoSteps ?? false,
    autoSecs: c.autoSecs ?? 5,
    autoOn: c.autoOn ?? 'timed',
    autoTrigger: c.autoTrigger ?? '',
    hlDefault: c.hlDefault ?? '',
  };
}

/** A text the chart writes: the Designer's, else in the project language (mirrors `word`). */
export function word(own: string, en: string, fr: string, lang: string): string {
  return own.trim() ? own : lang.startsWith('fr') ? fr : en;
}
export type Cfg = ReturnType<typeof cfg>;

const NO_HL: Hl = { kind: 'none', idx: -1 };
export function st(s: unknown): Required<State> & { cur?: string[][]; prevAxis?: [number, number]; mode?: RevealMode } {
  const x = (s ?? {}) as State;
  return { prev: [], prevSeries: [], prevN: 0, at: 0, step: 0, stepAt: [], hlAt: 0, ctl: false, auto: false, interval: 0, autoAt: 0, on: 'timed', trigger: '', pausedAt: 0, ...x, hl: x.hl ?? NO_HL, prevHl: x.prevHl ?? NO_HL } as never;
}
export type St = ReturnType<typeof st>;

// ------------------------------------------------------------------- data --

export function resolveGrid(c: Cfg, data: Data): string[][] {
  if (c.source === 'csv') {
    // CSV, or JSON rows (a data set's table).
    const t = interpolateText(c.csv, data);
    return numberList(t) ?? (t.trimStart().startsWith('[') ? parseRows(t) : parseCsv(t));
  }
  return c.data.map((r) => r.map((s) => interpolateText(String(s ?? ''), data)));
}

/** A JSON list of numbers (a count per minute) as one series, its rows numbered from 1 (mirrors `number_list`). */
function numberList(t: string): string[][] | null {
  let v: unknown;
  try {
    v = JSON.parse(t.trim());
  } catch {
    return null;
  }
  if (!Array.isArray(v) || !v.length || !v.every((x) => typeof x === 'number')) return null;
  return [['', 'Value'], ...v.map((x, i) => [String(i + 1), String(x)])];
}

export function withData(c: Cfg, data: Data): Cfg {
  return { ...c, title: { ...c.title, text: interpolateText(c.title.text, data), sub: interpolateText(c.title.sub, data) }, axis: { ...c.axis, xTitle: interpolateText(c.axis.xTitle, data), yTitle: interpolateText(c.axis.yTitle, data) }, refLine: { ...c.refLine, label: interpolateText(c.refLine.label, data) } };
}

export function numOf(v: string): number | null {
  const t = String(v ?? '')
    .trim()
    .replace(/\s/g, '')
    .replace(',', '.');
  const low = t.toLowerCase();
  if (t === '' || t === '-' || low === 'na' || low === 'n/a') return null;
  if (!/^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(t)) return null;
  const n = +t;
  return Number.isFinite(n) ? n : null;
}

export interface Cat {
  name: string;
  raw: (number | null)[];
  vals: (number | null)[];
  interp: boolean[];
  ri: number;
}
export interface Parsed {
  ser: string[];
  cats: Cat[];
  missing: number;
}

export function parse(c: Cfg, rows: string[][]): Parsed {
  const head = rows[0] ?? ['Category'];
  const ser = head.slice(1).map((n, j) => String(n ?? '').trim() || `Series ${j + 1}`);
  const cats: Cat[] = [];
  rows.slice(1).forEach((row, k) => {
    const name = String(row[0] ?? '').trim();
    const raw = ser.map((_, j) => numOf(row[j + 1] ?? ''));
    if (!name && raw.every((v) => v == null)) return;
    cats.push({ name: name || '—', raw, vals: raw.slice(), interp: ser.map(() => false), ri: k });
  });
  let missing = 0;
  for (let j = 0; j < ser.length; j++)
    for (let i = 0; i < cats.length; i++) {
      if (cats[i].raw[j] != null) continue;
      missing++;
      if (c.missing === 'zero') cats[i].vals[j] = 0;
      else if (c.missing === 'interp') {
        let a = i - 1;
        let b = i + 1;
        while (a >= 0 && cats[a].raw[j] == null) a--;
        while (b < cats.length && cats[b].raw[j] == null) b++;
        if (a >= 0 && b < cats.length) {
          const va = cats[a].raw[j]!;
          const vb = cats[b].raw[j]!;
          cats[i].vals[j] = va + ((vb - va) * (i - a)) / (b - a);
          cats[i].interp[j] = true;
        }
      }
    }
  return { ser, cats, missing };
}

// ---------------------------------------------------------------- helpers --

export const PAL = ['#7c5cff', '#ff3b5c', '#22c55e', '#facc15', '#38bdf8', '#f97316', '#e879f9', '#94a3b8', '#14b8a6', '#fb7185'];
export const OTHERS = '#64748b';
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
const lerp = (a: number, b: number, k: number) => a + (b - a) * k;
const ease = (k: number) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
const easeOut = (k: number) => 1 - Math.pow(1 - k, 3);
const curve = (c: string, k: number) => (c === 'linear' ? k : c === 'out' ? easeOut(k) : ease(k));

export const fxOf = (c: Cfg): Fx => (c.anim.fx !== 'auto' ? c.anim.fx : c.chartType === 'pie' ? 'sweep' : c.chartType === 'columns' ? 'grow' : 'draw');
export function revealMode(c: Cfg, s: St): RevealMode {
  const m = s.mode ?? c.reveal;
  return c.chartType === 'pie' && m === 'series' ? 'all' : m;
}
export const steps = (c: Cfg, s: St, p: Parsed) => (revealMode(c, s) === 'series' ? p.ser.length : revealMode(c, s) === 'categories' ? p.cats.length : 0);

export function seriesStyle(c: Cfg, j: number): { color: string; width: number; marker: Marker } {
  const s = c.series[j] ?? {};
  return { color: s.color ?? PAL[j % PAL.length], width: s.width ?? 6, marker: s.marker ?? 'circle' };
}
export const sliceColor = (c: Cfg, i: number) => c.sliceColors[i] || PAL[i % PAL.length];

export function updK(c: Cfg, s: St, now: number): number {
  const dur = c.anim.upd * 1000;
  if (!s.at || dur <= 0) return 1;
  return curve(c.anim.curve, clamp((now - s.at) / dur, 0, 1));
}
export function hlK(c: Cfg, s: St, now: number): number {
  const dur = c.anim.upd * 1000;
  if (!s.hlAt || dur <= 0) return 1;
  return curve(c.anim.curve, clamp((now - s.hlAt) / dur, 0, 1));
}
export function tweenVal(s: St, k: number, cat: string, ser: string, target: number | null): number | null {
  if (target == null) return null;
  if (k >= 1) return target;
  const j = s.prevSeries.indexOf(ser);
  const p = s.prev.find((q) => q.name === cat);
  const from = p && j >= 0 ? (p.vals[j] ?? null) : null;
  return from != null ? lerp(from, target, k) : target;
}
export const isNewCat = (s: St, name: string) => s.at > 0 && s.prev.length > 0 && !s.prev.some((p) => p.name === name);

export interface Scale {
  min: number;
  max: number;
  step: number;
}
export function nice(lo: number, hi: number, n: number): Scale {
  if (!(hi > lo)) hi = lo + 1;
  const raw = (hi - lo) / Math.max(1, n);
  let step = Math.pow(10, Math.floor(Math.log10(raw)));
  const e = raw / step;
  step *= e >= 7.5 ? 10 : e >= 3.5 ? 5 : e >= 1.5 ? 2 : 1;
  return { min: Math.floor(lo / step + 1e-6) * step, max: Math.ceil(hi / step - 1e-6) * step, step };
}
export const isStacked = (c: Cfg) => (c.chartType === 'columns' && c.colMode !== 'grouped') || (c.chartType === 'areas' && c.areaMode === 'stacked');
export const isPct = (c: Cfg) => c.chartType === 'columns' && c.colMode === 'percent';
export function targets(c: Cfg, vals: (number | null)[][]): (number | null)[][] {
  const pct = isPct(c);
  return vals.map((vs) => {
    const tot = vs.reduce<number>((a, v) => a + (v != null && v > 0 ? v : 0), 0);
    return vs.map((v) => (v == null ? null : !pct ? v : tot > 0 ? (Math.max(0, v) / tot) * 100 : 0));
  });
}
export function scale(c: Cfg, tv: (number | null)[][]): Scale {
  const stacked = isStacked(c);
  let lo = Infinity;
  let hi = -Infinity;
  for (const vs of tv) {
    if (stacked) {
      let p = 0;
      let n = 0;
      for (const v of vs) if (v != null) v > 0 ? (p += v) : v < 0 && (n += v);
      hi = Math.max(hi, p);
      lo = Math.min(lo, n);
    } else for (const v of vs) if (v != null) ((hi = Math.max(hi, v)), (lo = Math.min(lo, v)));
  }
  if (!Number.isFinite(lo)) ((lo = 0), (hi = 1));
  if (c.chartType !== 'lines' || c.axis.zero) ((lo = Math.min(lo, 0)), (hi = Math.max(hi, 0)));
  if (isPct(c)) return { min: 0, max: 100, step: nice(0, 100, c.axis.ticks).step };
  if (c.axis.auto) return nice(lo, hi + (hi - lo) * 0.04, c.axis.ticks);
  const mn = Math.min(c.axis.min, c.axis.max - 1e-6);
  const mx = Math.max(c.axis.max, mn + 1e-6);
  return { min: mn, max: mx, step: nice(mn, mx, c.axis.ticks).step };
}
export const jsNum = (v: number) => String(Math.round(v * 1e6) / 1e6);
export function fixed(v: number, d: number): string {
  const p = Math.pow(10, d);
  const r = Math.round(Math.abs(v) * p) / p;
  const s = r.toFixed(d);
  return v < 0 && r !== 0 ? `-${s}` : s;
}
export function fmt(c: Cfg, v: number): string {
  const a = c.axis;
  const dec = Math.min(4, a.decimals);
  let t = fixed(Math.abs(v), dec);
  if (a.sep !== 'none') {
    const [i, d] = t.split('.');
    t = i.replace(/\B(?=(\d{3})+(?!\d))/g, a.sep === 'space' ? ' ' : ',') + (d ? '.' + d : '');
  }
  const neg = v < 0 && /[1-9]/.test(fixed(Math.abs(v), dec));
  return `${neg ? '−' : ''}${a.prefix}${t}${a.suffix}`;
}
export function labelText(c: Cfg, nameOn: boolean, v: number | null, pct: number | null, name: string): string {
  const L = c.labels;
  const parts: string[] = [];
  if (nameOn && name) parts.push(name);
  if (L.value && v != null) parts.push(fmt(c, v));
  if (L.pct && pct != null) parts.push(`${fixed(pct, pct < 10 && c.axis.decimals > 0 ? 1 : 0)} %`);
  return parts.join(' · ');
}
export const tw2 = (s: string, size: number) => [...s].length * size * 0.58;
export function ink(hex: string): string {
  const m = /^#?([0-9a-f]{6})/i.exec(hex || '');
  if (!m) return '#ffffff';
  const n = parseInt(m[1], 16);
  return 0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255) > 165 ? '#14102a' : '#ffffff';
}
export const shownGrid = (c: Cfg, s: St, data: Data) => s.cur ?? resolveGrid(c, data);

// ------------------------------------------------------------- primitives --

export type Anchor = 'start' | 'middle' | 'end';
export interface PShape {
  kind: 'shape';
  d: string;
  fill?: string;
  fillOpacity: number;
  stroke?: string;
  width: number;
  dash?: [number, number];
  round: boolean;
  opacity: number;
  clip?: [number, number, number, number];
}
export interface PText {
  kind: 'text';
  x: number;
  y: number;
  text: string;
  size: number;
  weight: number;
  anchor: Anchor;
  fill: string;
  opacity: number;
  halo?: string;
  rotate: boolean;
  clip?: [number, number, number, number];
}
export type Prim = PShape | PText;
export interface Out {
  prims: Prim[];
  hidden: number;
  pieNeg: [string, number][];
  pieZero: boolean;
  pieOthers: number;
  pieMissing: number;
}
export interface Input {
  cfg: Cfg;
  st: St;
  grid: string[][];
  w: number;
  h: number;
  fs: number;
  ageMs: number;
  now: number;
  /** The project's language (texts the chart writes: « Aucune donnée »…). */
  lang: string;
}

const D2R = Math.PI / 180;
const MUTED = '#b9b2dd';
const WHITE = '#ffffff';
const shape = (d: string): PShape => ({ kind: 'shape', d, fillOpacity: 1, width: 0, round: false, opacity: 1 });
const text = (x: number, y: number, s: string, size: number, weight: number, anchor: Anchor, fill: string, op: number): PText => ({ kind: 'text', x, y, text: s, size, weight, anchor, fill, opacity: op, rotate: false });
const rectD = (x: number, y: number, w: number, h: number, radii: number[]) =>
  roundedPolygon(
    [
      [x, y],
      [x + w, y],
      [x + w, y + h],
      [x, y + h],
    ],
    radii,
  );
const lineD = (a: [number, number], b: [number, number]) => `M${a[0]} ${a[1]}L${b[0]} ${b[1]}`;
const pol = (cx: number, cy: number, r: number, a: number): [number, number] => [cx + r * Math.sin(a * D2R), cy - r * Math.cos(a * D2R)];
function arcTo(cx: number, cy: number, r: number, a0: number, a1: number): string {
  const n = Math.max(1, Math.ceil(Math.abs(a1 - a0) / 90));
  const th = (a1 - a0) / n;
  const k = (4 / 3) * Math.tan((th * D2R) / 4) * r;
  let d = '';
  for (let i = 0; i < n; i++) {
    const a = a0 + th * i;
    const b = a + th;
    const p0 = pol(cx, cy, r, a);
    const p1 = pol(cx, cy, r, b);
    d += `C${p0[0] + k * Math.cos(a * D2R)} ${p0[1] + k * Math.sin(a * D2R)} ${p1[0] - k * Math.cos(b * D2R)} ${p1[1] - k * Math.sin(b * D2R)} ${p1[0]} ${p1[1]}`;
  }
  return d;
}
function circleD(cx: number, cy: number, r: number): string {
  const p = pol(cx, cy, r, 0);
  return `M${p[0]} ${p[1]}${arcTo(cx, cy, r, 0, 360)}Z`;
}
function sliceD(cx: number, cy: number, r0: number, r1: number, a0: number, a1: number): string {
  if (a1 - a0 >= 359.9) {
    let d = circleD(cx, cy, r1);
    if (r0 > 0) {
      const p = pol(cx, cy, r0, 360);
      d += `M${p[0]} ${p[1]}${arcTo(cx, cy, r0, 360, 0)}Z`;
    }
    return d;
  }
  const p = pol(cx, cy, r1, a0);
  let d = `M${p[0]} ${p[1]}${arcTo(cx, cy, r1, a0, a1)}`;
  if (r0 > 0) {
    const q = pol(cx, cy, r0, a1);
    d += `L${q[0]} ${q[1]}${arcTo(cx, cy, r0, a1, a0)}`;
  } else d += `L${cx} ${cy}`;
  return d + 'Z';
}
function pathLine(pts: [number, number][], smooth: boolean): string {
  if (!pts.length) return '';
  let d = `M${pts[0][0]} ${pts[0][1]}`;
  for (let i = 1; i < pts.length; i++) {
    const p0 = i >= 2 ? pts[i - 2] : pts[i - 1];
    const p1 = pts[i - 1];
    const p2 = pts[i];
    const p3 = pts[i + 1] ?? p2;
    if (!smooth) {
      d += `L${p2[0]} ${p2[1]}`;
      continue;
    }
    const lo = Math.min(p1[1], p2[1]);
    const hi = Math.max(p1[1], p2[1]);
    const c1y = clamp(p1[1] + (p2[1] - p0[1]) / 6, lo, hi);
    const c2y = clamp(p2[1] - (p3[1] - p1[1]) / 6, lo, hi);
    d += `C${p1[0] + (p2[0] - p1[0]) / 3} ${c1y} ${p2[0] - (p2[0] - p1[0]) / 3} ${c2y} ${p2[0]} ${p2[1]}`;
  }
  return d;
}
function marker(kind: Marker, x: number, y: number, r: number, fill: string, stroke: string, hollow: boolean): PShape | null {
  const f = hollow ? stroke : fill;
  const s = hollow ? fill : stroke;
  let d: string;
  if (kind === 'none') return null;
  if (kind === 'square') d = rectD(x - r, y - r, 2 * r, 2 * r, [0]);
  else if (kind === 'diamond') d = `M${x} ${y - r * 1.3}L${x + r * 1.3} ${y}L${x} ${y + r * 1.3}L${x - r * 1.3} ${y}Z`;
  else d = circleD(x, y, r);
  return { ...shape(d), fill: f, stroke: s, width: r * 0.55 };
}

interface Lab {
  x: number;
  y: number;
  txt: string;
  fill: string;
  halo?: string;
  prio: number;
  size?: number;
  anchor: Anchor;
  op: number;
  w?: number;
  slot: number;
  j: number;
  inside: boolean;
  col?: number;
  hide: boolean;
}
const lab = (x: number, y: number, txt: string, fill: string, prio: number, op: number): Lab => ({ x, y, txt, fill, prio, anchor: 'middle', op, slot: 0, j: 0, inside: false, hide: false });

function spread(list: Lab[], gap: number, lo: number, hi: number, mode: string): number {
  if (mode === 'hide') {
    const idx = list.map((_, i) => i).sort((a, b) => list[b].prio - list[a].prio);
    const kept: number[] = [];
    let hidden = 0;
    for (const i of idx) {
      if (kept.some((k) => Math.abs(k - list[i].y) < gap)) ((list[i].hide = true), hidden++);
      else kept.push(list[i].y);
    }
    return hidden;
  }
  const arr = list.slice().sort((a, b) => a.y - b.y);
  const n = arr.length;
  for (let k = 1; k < n; k++) if (arr[k].y < arr[k - 1].y + gap) arr[k].y = arr[k - 1].y + gap;
  if (n && arr[n - 1].y > hi) {
    arr[n - 1].y = hi;
    for (let k = n - 2; k >= 0; k--) if (arr[k].y > arr[k + 1].y - gap) arr[k].y = arr[k + 1].y - gap;
  }
  if (n && arr[0].y < lo) {
    arr[0].y = lo;
    for (let k = 1; k < n; k++) if (arr[k].y < arr[k - 1].y + gap) arr[k].y = arr[k - 1].y + gap;
  }
  return 0;
}

class Tm {
  constructor(
    public now: number,
    public age: number,
    public dur: number,
    public stagger: number,
    public fx: Fx,
    public mode: RevealMode,
    public step: number,
  ) {}
  ep(el: number): number {
    if (el === Infinity) return 1;
    if (this.dur <= 0) return el >= 0 ? 1 : 0;
    return easeOut(clamp(el / this.dur, 0, 1));
  }
  cat(s: St, i: number, name: string): [number, boolean] {
    if (this.mode === 'categories' && i >= this.step) return [0, true];
    let el = this.fx === 'grow' ? this.age - i * this.stagger : Infinity;
    if (this.mode === 'categories' && (s.stepAt[i] ?? 0) > 0) el = this.now - s.stepAt[i];
    if (isNewCat(s, name)) el = Math.min(el, this.now - s.at);
    return [this.ep(el), false];
  }
  ser(s: St, j: number): [number, boolean] {
    if (this.mode === 'series' && j >= this.step) return [0, true];
    let el = this.fx === 'draw' ? this.age - j * this.stagger * 2.5 : Infinity;
    if (this.mode === 'series' && (s.stepAt[j] ?? 0) > 0) el = this.now - s.stepAt[j];
    return [this.ep(el), false];
  }
}

interface Ctx {
  c: Cfg;
  st: St;
  p: Parsed;
  tm: Tm;
  k: number;
  kh: number;
  fs: number;
  hidden: number;
  ink: Inks;
  lang: string;
}
/** Colours of the texts: main, secondary (ticks, subtitle), category names, n/a (mirrors Inks). */
interface Inks {
  main: string;
  muted: string;
  cat: string;
  na: string;
}
function inksOf(c: Cfg): Inks {
  const t = c.textColor.trim();
  if (!t) return { main: WHITE, muted: MUTED, cat: '#d6d0f5', na: '#8f88b5' };
  const solid = [...t].slice(0, 7).join('');
  return { main: t, muted: `${solid}bf`, cat: t, na: `${solid}99` };
}
const hlv = (cx: Ctx, f: (hl: Hl) => number) => lerp(f(cx.st.prevHl), f(cx.st.hl), cx.kh);
interface LegendItem {
  name: string;
  color: string;
  dim: boolean;
  op: number;
}

export function layout(inp: Input): Out {
  const c = inp.cfg;
  const s = inp.st;
  const p = parse(c, inp.grid);
  const fxv = fxOf(c);
  const tm = new Tm(inp.now, inp.ageMs, fxv === 'none' ? 0 : c.anim.dur * 1000, c.anim.stagger, fxv, revealMode(c, s), Math.max(1, s.step));
  const cx: Ctx = { c, st: s, p, tm, k: updK(c, s, inp.now), kh: hlK(c, s, inp.now), fs: inp.fs, hidden: 0, ink: inksOf(c), lang: inp.lang };
  const { w, h, fs } = inp;
  const out: Out = { prims: [], hidden: 0, pieNeg: [], pieZero: false, pieOthers: 0, pieMissing: 0 };
  if (c.spark) {
    out.prims = spark(cx, w, h);
    return out;
  }
  if (c.panel.on) out.prims.push({ ...shape(rectD(0, 0, w, h, [28])), fill: c.panel.color, opacity: c.panel.opacity });
  const pad = Math.round(fs * 1.4);
  let top = pad;
  let bottom = h - pad;
  const left = pad;
  let right = w - pad;
  if (c.title.pos !== 'none' && c.title.text) {
    const center = c.title.pos === 'center';
    const tx = center ? w / 2 : left;
    const anc: Anchor = center ? 'middle' : 'start';
    out.prims.push(text(tx, top + fs * 1.45, c.title.text, fs * 1.75, 900, anc, cx.ink.main, 1));
    top += fs * 2.1;
    if (c.title.sub) {
      out.prims.push(text(tx, top + fs * 0.85, c.title.sub, fs, 600, anc, cx.ink.muted, 1));
      top += fs * 1.5;
    }
    top += fs * 0.3;
  }
  const isPie = c.chartType === 'pie';
  const legend: LegendItem[] = [];
  if (!isPie) p.ser.forEach((name, j) => legend.push({ name, color: seriesStyle(c, j).color, dim: s.hl.kind === 'series' && s.hl.idx !== j, op: tm.ser(s, j)[1] ? 0.25 : 1 }));
  const lfs = fs * 0.85;
  let ly = 0;
  let lx = 0;
  if (c.legend === 'top') ((ly = top + lfs * 0.6), (top += lfs * 2));
  else if (c.legend === 'bottom') ((bottom -= lfs * 1.9), (ly = bottom + lfs * 1.1));
  else if (c.legend === 'right') {
    const names = isPie ? [...p.cats.map((x) => x.name), word(c.texts.others, 'Others', 'Autres', cx.lang)] : p.ser;
    const widest = names.reduce((a, n) => Math.max(a, tw2(n, lfs)), 60);
    const lw = Math.min(w * 0.3, widest + lfs * 2.6);
    right -= lw;
    lx = right + lfs * 1.2;
  }
  const fade = fxv === 'fade' ? tm.ep(tm.age) : 1;
  const frame: [number, number, number, number] = [left, right, top, bottom];
  const body = isPie ? pie(cx, frame, legend, out) : xy(cx, frame);
  for (const b of body) b.opacity *= fade;
  out.prims.push(...body);
  if (c.legend !== 'none' && legend.length) {
    const sw = lfs * 0.8;
    const item = (x: number, y: number, it: LegendItem) => {
      const o = it.dim ? 0.3 : it.op;
      out.prims.push({ ...shape(rectD(x, y - sw * 0.85, sw, sw, [sw * 0.25])), fill: it.color, opacity: o });
      out.prims.push(text(x + sw + lfs * 0.5, y, it.name, lfs, 700, 'start', cx.ink.main, o));
    };
    if (c.legend === 'right') {
      let y = top + lfs;
      for (const it of legend) (item(lx, y, it), (y += lfs * 1.55));
    } else {
      const widths = legend.map((it) => sw + lfs * 0.5 + tw2(it.name, lfs) + lfs * 1.3);
      const tot = widths.reduce((a, b) => a + b, 0);
      let x = c.title.pos === 'center' ? (w - tot) / 2 : left;
      legend.forEach((it, i) => {
        if (x + widths[i] > w - pad + lfs) return;
        item(x, ly, it);
        x += widths[i];
      });
    }
  }
  out.hidden = cx.hidden;
  return out;
}

interface P {
  x: number;
  y: number;
  y0: number;
  v: number;
  ci: number;
  interp: boolean;
}

/** Sparkline: the first series filling the box, growing (columns) or drawn (line, area) from the take (mirrors `spark`). */
function spark(cx: Ctx, w: number, h: number): Prim[] {
  const c = cx.c;
  const vals = cx.p.cats.map((k) => k.vals[0] ?? 0);
  const n = vals.length;
  const g: Prim[] = [];
  if (!n) return g;
  const { color, width } = seriesStyle(c, 0);
  const lo = vals.reduce((a, v) => Math.min(a, v), 0);
  const hi = vals.reduce((a, v) => Math.max(a, v), lo);
  const span = Math.max(hi - lo, 1e-6);
  const e = cx.tm.ep(cx.tm.age);
  if (c.chartType === 'columns' || c.chartType === 'pie') {
    const slot = w / n;
    const bw = Math.max(slot * 0.7, 1);
    const r = Math.min(bw / 2, 3);
    vals.forEach((v, i) => {
      const bh = ((v - lo) / span) * h * e;
      if (bh <= 0) return;
      g.push({ ...shape(rectD(i * slot + (slot - bw) / 2, h - bh, bw, bh, [r, r, 0, 0])), fill: color });
    });
    return g;
  }
  const lw = Math.max(Math.min(width, h / 4), 1);
  const step = n > 1 ? w / (n - 1) : 0;
  const pts = vals.map((v, i) => [i * step, clamp(h - ((v - lo) / span) * h, lw / 2, h - lw / 2)] as [number, number]);
  const clip: [number, number, number, number] = [0, 0, w * e, h];
  if (c.chartType === 'areas') g.push({ ...shape(`${pathLine(pts, c.smooth)}L${pts[n - 1][0]} ${h}L${pts[0][0]} ${h}Z`), fill: color, fillOpacity: c.areaOpacity, clip });
  g.push({ ...shape(pathLine(pts, c.smooth)), stroke: color, width: lw, round: true, clip });
  return g;
}

function xy(cx: Ctx, o: [number, number, number, number]): Prim[] {
  const { c, st: s, fs } = cx;
  const g: Prim[] = [];
  const ns = cx.p.ser.length;
  const nc = cx.p.cats.length;
  if (!ns || !nc) return [text((o[0] + o[1]) / 2, (o[2] + o[3]) / 2, word(c.texts.noData, 'No data', 'Aucune donnée', cx.lang), fs * 1.4, 800, 'middle', cx.ink.muted, 1)];
  const ct = c.chartType;
  if (ct === 'columns' && c.orient === 'h') return hbars(cx, o);
  const stacked = isStacked(c);
  const pct = isPct(c);
  const tv = targets(
    c,
    cx.p.cats.map((x) => x.vals),
  );
  const sv = targets(
    c,
    cx.p.cats.map((cat) => cat.vals.map((v, j) => tweenVal(s, cx.k, cat.name, cx.p.ser[j], v))),
  );
  const sc = scale(c, tv);
  const k = cx.k;
  const pa = s.prevAxis ?? [sc.min, sc.max];
  const yMin = lerp(pa[0], sc.min, k);
  const yMax = lerp(pa[1], sc.max, k);
  const ticks: number[] = [];
  for (let v = Math.ceil(sc.min / sc.step - 1e-6) * sc.step; v <= sc.max + 1e-6 * Math.max(1, sc.step) && ticks.length <= 200; v += sc.step) ticks.push(Math.round(v * 1e6) / 1e6);
  const tickLabel = (v: number) => (pct ? `${jsNum(v)} %` : fmt(c, v));
  const tfs = fs * 0.8;
  const tickW = ticks.reduce((a, v) => Math.max(a, tw2(tickLabel(v), tfs)), -Infinity);
  let left = o[0];
  let bottom = o[3];
  if (c.axis.yTitle) left += fs * 1.3;
  left += tickW + fs * 0.6;
  if (c.axis.xTitle) bottom -= fs * 1.4;
  bottom -= fs * 1.5;
  const top = o[2] + fs * 0.8;
  const right = o[1] - (c.labels.name && ct !== 'columns' ? Math.min(220, cx.p.ser.reduce((a, n) => Math.max(a, tw2(n, fs * 0.8)), -Infinity) + 20) : 10);
  const pw = right - left;
  const ph = bottom - top;
  const span = yMax - yMin !== 0 ? yMax - yMin : 1;
  const Y = (v: number) => top + ph - ((v - yMin) / span) * ph;
  const nSlots = s.prev.length ? lerp(s.prevN, nc, k) : nc;
  const band = pw / nSlots;
  const slotX = cx.p.cats.map((cat, i) => left + (lerp(s.prev.find((q) => q.name === cat.name)?.slot ?? i, i, k) + 0.5) * (pw / nSlots));
  for (const v of ticks) {
    const y = Y(v);
    if (y < top - 2 || y > bottom + 2) continue;
    const zero = Math.abs(v) < 1e-6;
    if (c.axis.grid || zero) g.push({ ...shape(lineD([left, y], [right, y])), stroke: WHITE, opacity: zero ? 0.55 : 0.12, width: zero ? 3 : 2 });
    g.push(text(left - fs * 0.5, y + tfs * 0.35, tickLabel(v), tfs, 700, 'end', cx.ink.muted, 1));
  }
  if (c.axis.yTitle) g.push({ ...text(o[0] + fs * 0.7, top + ph / 2, c.axis.yTitle, fs * 0.8, 700, 'middle', cx.ink.muted, 1), rotate: true });
  if (c.axis.xTitle) g.push(text(left + pw / 2, o[3] - fs * 0.2, c.axis.xTitle, fs * 0.8, 700, 'middle', cx.ink.muted, 1));
  const cfs = fs * 0.82;
  const maxCat = cx.p.cats.reduce((a, x) => Math.max(a, tw2(x.name, cfs)), -Infinity);
  const every = Math.max(1, Math.ceil((maxCat + 12) / band));
  const catE = cx.p.cats.map((cat, i) => cx.tm.cat(s, i, cat.name));
  const serE = cx.p.ser.map((_, j) => cx.tm.ser(s, j));
  cx.p.cats.forEach((cat, i) => {
    const x = slotX[i];
    const isHl = s.hl.kind === 'category' && s.hl.idx === cat.ri;
    if (c.axis.vgrid) g.push({ ...shape(lineD([x, top], [x, bottom])), stroke: WHITE, opacity: 0.1, width: 2 });
    if (isHl) g.push({ ...shape(rectD(x - band / 2, top - fs * 0.6, band, bottom - top + fs * 0.6, [10])), fill: WHITE, opacity: 0.08 });
    const [e, hid] = catE[i];
    const ce = Math.max(e, hid ? 0 : 0.0001);
    if (i % every === 0 || isHl) g.push(text(x, bottom + fs * 1.15, cat.name, cfs, isHl ? 900 : 700, 'middle', isHl ? cx.ink.main : cx.ink.cat, hid ? 0.3 : Math.min(1, 0.3 + ce)));
  });
  const clip: [number, number, number, number] = [left - 30, top - 4, pw + 60, ph + 8];
  const dimOf = (j: number, ri: number) => hlv(cx, (hl) => (hl.kind === 'none' || (hl.kind === 'series' && hl.idx === j) || (hl.kind === 'category' && hl.idx === ri) ? 1 : 0.22));
  const L = c.labels;
  const lfs = fs * 0.78;
  const labels: Lab[] = [];
  const data: Prim[] = [];
  let hidden = 0;
  if (ct === 'columns') {
    const grouped = c.colMode === 'grouped';
    const gw = band * 0.74;
    const bw = grouped ? Math.max(2, (gw - (ns - 1) * 6) / ns) : gw;
    cx.p.cats.forEach((cat, i) => {
      const cxp = slotX[i];
      let pos = 0;
      let neg = 0;
      const tot = cat.vals.reduce<number>((a, v) => a + (v != null && v > 0 ? v : 0), 0);
      for (let j = 0; j < ns; j++) {
        const e = Math.min(catE[i][0], serE[j][0]);
        const { color } = seriesStyle(c, j);
        const raw = tv[i][j];
        const op = dimOf(j, cat.ri);
        const x = grouped ? cxp - gw / 2 + j * (bw + 6) : cxp - gw / 2;
        if (raw == null) {
          if (e > 0 && grouped) labels.push({ ...lab(x + bw / 2, Y(0) - 10, word('', 'n/a', 'n.d.', cx.lang), cx.ink.na, -1, 1), size: lfs * 0.8 });
          continue;
        }
        const shown = sv[i][j] ?? raw;
        const v = shown * e;
        let y0: number, y1: number;
        if (grouped) ((y0 = Y(0)), (y1 = Y(v)));
        else if (v >= 0) ((y0 = Y(pos)), (y1 = Y(pos + v)), (pos += v));
        else ((y0 = Y(neg)), (y1 = Y(neg + v)), (neg += v));
        const yt = Math.min(y0, y1);
        const hh = Math.abs(y1 - y0);
        if (hh < 0.2 && e < 1) continue;
        const rad = Math.min(8, bw / 4, hh / 2);
        const bar: PShape = { ...shape(rectD(x, yt, bw, hh, v >= 0 ? [rad, rad, 0, 0] : [0, 0, rad, rad])), fill: color, opacity: op };
        if (cat.interp[j]) Object.assign(bar, { fillOpacity: 0.35, stroke: color, width: 3, dash: [8, 6] });
        data.push(bar);
        if (e < 0.6) continue;
        const vj = cat.vals[j];
        const lp = pct ? raw : vj != null && tot > 0 && vj > 0 ? (vj / tot) * 100 : null;
        const lv = pct ? vj : shown;
        const txt = labelText(c, L.name, lv, lp, cx.p.ser[j]);
        if (!txt) continue;
        const tw = tw2(txt, lfs);
        const fits = hh >= lfs * 1.35;
        let lastPos = -1;
        for (let q = 0; q < ns; q++) if ((tv[i][q] ?? 0) > 0) lastPos = q;
        const isTop = grouped || (v >= 0 && lastPos === j);
        let inside = L.pos === 'inside' || !grouped;
        if (inside && !fits) {
          if (L.overlap === 'offset' && isTop) inside = false;
          else {
            hidden++;
            continue;
          }
        }
        if (!inside && !grouped && !isTop) {
          hidden++;
          continue;
        }
        if (!inside && v >= 0 && yt - lfs * 1.2 < top - lfs * 0.6) inside = true;
        const ylab = inside ? (v >= 0 ? Math.max(yt, top) + lfs * 1.05 : yt + hh - lfs * 0.45) : v >= 0 ? yt - lfs * 0.45 : yt + hh + lfs * 1.05;
        const yc = !grouped && inside ? yt + hh / 2 + lfs * 0.35 : ylab;
        labels.push({ ...lab(x + bw / 2, yc, txt, inside ? ink(color) : cx.ink.main, Math.abs(v), op), w: tw, slot: bw + 6, j, inside });
      }
    });
    if (grouped)
      for (const lb of labels) {
        if (lb.w == null || lb.w <= lb.slot) continue;
        if (L.overlap === 'offset') {
          if (lb.j % 2 === 1) lb.y += lb.inside ? lfs * 1.15 : -lfs * 1.15;
          if (lb.w > lb.slot * 2) ((lb.hide = true), hidden++);
        } else ((lb.hide = true), hidden++);
      }
  } else {
    const base = cx.p.cats.map(() => 0);
    const pts: [P, number, number][] = [];
    const panel = c.panel.color;
    for (let j = 0; j < ns; j++) {
      const { color, width, marker: mk } = seriesStyle(c, j);
      const se = serE[j][0];
      const arr: (P | null)[] = [];
      let prev: P | null = null;
      cx.p.cats.forEach((cat, i) => {
        const v = sv[i][j];
        if (v == null) return void arr.push(null);
        const x = slotX[i];
        const b0 = stacked ? base[i] : 0;
        const vv = stacked ? b0 + v : v;
        if (stacked) base[i] = vv;
        const pt: P = { x, y: Y(vv), y0: Y(b0), v, ci: i, interp: cat.interp[j] };
        const [ce, hid] = catE[i];
        if (hid) return void arr.push(null);
        if (ce < 1) {
          if (prev) ((pt.x = lerp(prev.x, pt.x, ce)), (pt.y = lerp(prev.y, pt.y, ce)), (pt.y0 = lerp(prev.y0, pt.y0, ce)));
          else pt.y = lerp(pt.y0, pt.y, ce);
        }
        arr.push(pt);
        prev = pt;
      });
      const runs: P[][] = [];
      for (const pt of arr) {
        if (pt) runs.length ? runs[runs.length - 1].push(pt) : runs.push([pt]);
        else if (runs.length && runs[runs.length - 1].length) runs.push([]);
      }
      const rs = runs.filter((r) => r.length);
      const op = hlv(cx, (hl) => (hl.kind === 'series' ? (hl.idx === j ? 1 : 0.2) : 1));
      if (se <= 0) continue;
      const sclip: [number, number, number, number] | undefined = se < 1 ? [left - 20, -2000, (pw + 40) * se, 5000] : undefined;
      const gs: PShape[] = [];
      for (const rn of rs) {
        const topl = pathLine(
          rn.map((q) => [q.x, q.y]),
          c.smooth,
        );
        if (ct === 'areas') {
          let d = topl;
          if (stacked)
            d += pathLine(
              rn
                .slice()
                .reverse()
                .map((q) => [q.x, q.y0]),
              c.smooth,
            ).replace(/^M/, 'L');
          else d += `L${rn[rn.length - 1].x} ${Y(0)}L${rn[0].x} ${Y(0)}`;
          gs.push({ ...shape(d + 'Z'), fill: color, opacity: c.areaOpacity });
        }
        if (ct === 'lines' || width > 0) gs.push({ ...shape(topl), stroke: color, width, round: true });
      }
      for (const rn of rs)
        for (const pt of rn) {
          const cat = cx.p.cats[pt.ci];
          const isHlc = s.hl.kind === 'category' && s.hl.idx === cat.ri;
          const mr = Math.max(4, width * 0.9) * (isHlc ? 1.5 : 1);
          if (mk !== 'none' || pt.interp || isHlc) {
            const m = marker(mk === 'none' ? 'circle' : mk, pt.x, pt.y, mr, color, panel, pt.interp);
            if (m) gs.push(m);
          }
          const [ce, hid] = catE[pt.ci];
          if (!hid && ce > 0.7) pts.push([pt, j, op]);
        }
      for (const sh of gs) data.push({ ...sh, opacity: sh.opacity * op, clip: sclip });
      if (L.name && rs.length && se > 0.95) {
        const lastRun = rs[rs.length - 1];
        const last = lastRun[lastRun.length - 1];
        labels.push({ ...lab(last.x + fs * 0.7, last.y + lfs * 0.35, cx.p.ser[j], color, 1000 - j, op), anchor: 'start', col: -1 });
      }
    }
    if (L.value || L.pct)
      for (const [pt, j, op] of pts) {
        if (serE[j][0] < 0.95) continue;
        const cat = cx.p.cats[pt.ci];
        const tot = cat.vals.reduce<number>((a, v) => a + (v != null && v > 0 ? v : 0), 0);
        const pc = tot > 0 && pt.v > 0 ? (pt.v / tot) * 100 : null;
        const txt = labelText(c, false, pt.v, pc, '');
        if (!txt) continue;
        const y = L.pos === 'inside' && ct === 'areas' ? pt.y + lfs * 1.4 : pt.y - lfs * 0.75;
        labels.push({ ...lab(pt.x, y, txt, cx.ink.main, Math.abs(pt.v), op), halo: c.panel.color, col: pt.ci });
      }
    const cols: number[] = [];
    for (const lb of labels) if (lb.col != null && !cols.includes(lb.col)) cols.push(lb.col);
    for (const cl of cols)
      hidden += spread(
        labels.filter((lb) => lb.col === cl),
        lfs * 1.1,
        top + lfs * 0.5,
        bottom - 2,
        L.overlap,
      );
  }
  if (c.refLine.on) {
    const y = Y(c.refLine.value);
    if (y >= top - 2 && y <= bottom + 2) data.push(...refLine(cx, [left, y], [right, y], [right, y - lfs * 0.45], 'end'));
  }
  for (const lb of labels) {
    if (lb.hide) continue;
    data.push({ ...text(lb.x, lb.y, lb.txt, lb.size ?? lfs, 800, lb.anchor, lb.fill, lb.op), halo: lb.halo });
  }
  for (const d of data) {
    if (d.kind === 'shape' && d.clip) {
      const sc2 = d.clip;
      const x0 = Math.max(sc2[0], clip[0]);
      const y0 = Math.max(sc2[1], clip[1]);
      const x1 = Math.min(sc2[0] + sc2[2], clip[0] + clip[2]);
      const y1 = Math.min(sc2[1] + sc2[3], clip[1] + clip[3]);
      d.clip = [x0, y0, Math.max(0, x1 - x0), Math.max(0, y1 - y0)];
    } else d.clip = clip;
  }
  cx.hidden += hidden;
  return g.concat(data);
}

/** The reference line from `a` to `b` (dashed, the text colour) and its label at `at` (mirrors ref_line). */
function refLine(cx: Ctx, a: [number, number], b: [number, number], at: [number, number], anchor: Anchor): Prim[] {
  const out: Prim[] = [{ ...shape(lineD(a, b)), stroke: cx.ink.main, width: 3, dash: [10, 8], opacity: 0.85 }];
  if (cx.c.refLine.label) out.push(text(at[0], at[1], cx.c.refLine.label, cx.fs * 0.78, 800, anchor, cx.ink.main, 1));
  return out;
}

/** Columns lying (`orient: h`): names on the left, values along the bottom, bars growing to the right (mirrors hbars). */
function hbars(cx: Ctx, o: [number, number, number, number]): Prim[] {
  const { c, st: s, fs } = cx;
  const g: Prim[] = [];
  const ns = cx.p.ser.length;
  const nc = cx.p.cats.length;
  const grouped = c.colMode === 'grouped';
  const pct = isPct(c);
  const tv = targets(
    c,
    cx.p.cats.map((x) => x.vals),
  );
  const sv = targets(
    c,
    cx.p.cats.map((cat) => cat.vals.map((v, j) => tweenVal(s, cx.k, cat.name, cx.p.ser[j], v))),
  );
  const sc = scale(c, tv);
  const k = cx.k;
  const pa = s.prevAxis ?? [sc.min, sc.max];
  const xMin = lerp(pa[0], sc.min, k);
  const xMax = lerp(pa[1], sc.max, k);
  const ticks: number[] = [];
  for (let v = Math.ceil(sc.min / sc.step - 1e-6) * sc.step; v <= sc.max + 1e-6 * Math.max(1, sc.step) && ticks.length <= 200; v += sc.step) ticks.push(Math.round(v * 1e6) / 1e6);
  const tickLabel = (v: number) => (pct ? `${jsNum(v)} %` : fmt(c, v));
  const tfs = fs * 0.8;
  const cfs = fs * 0.82;
  const lfs = fs * 0.78;
  const L = c.labels;
  const nameW = Math.min(
    cx.p.cats.reduce((a, x) => Math.max(a, tw2(x.name, cfs)), 0),
    (o[1] - o[0]) * 0.4,
  );
  let left = o[0] + nameW + fs * 0.6;
  if (c.axis.yTitle) left += fs * 1.3;
  // Room for the values written after the bars, and for the last tick.
  const labW = L.value || L.pct ? tv.flat().reduce<number>((a, v) => (v == null ? a : Math.max(a, tw2(labelText(c, L.name, v, 100, ''), lfs))), 0) + lfs * 0.6 : 0;
  const lastTick = ticks.length ? tw2(tickLabel(ticks[ticks.length - 1]), tfs) / 2 : 0;
  const right = o[1] - Math.min(Math.max(labW, lastTick), (o[1] - o[0]) * 0.3);
  const top = o[2] + fs * 0.4;
  let bottom = o[3] - fs * 1.5;
  if (c.axis.xTitle) bottom -= fs * 1.4;
  const pw = right - left;
  const ph = bottom - top;
  const span = xMax - xMin !== 0 ? xMax - xMin : 1;
  const X = (v: number) => left + ((v - xMin) / span) * pw;
  const nSlots = s.prev.length ? lerp(s.prevN, nc, k) : nc;
  const band = ph / nSlots;
  const slotY = cx.p.cats.map((cat, i) => top + (lerp(s.prev.find((q) => q.name === cat.name)?.slot ?? i, i, k) + 0.5) * band);
  for (const v of ticks) {
    const x = X(v);
    if (x < left - 2 || x > right + 2) continue;
    const zero = Math.abs(v) < 1e-6;
    if (c.axis.grid || zero) g.push({ ...shape(lineD([x, top], [x, bottom])), stroke: WHITE, opacity: zero ? 0.55 : 0.12, width: zero ? 3 : 2 });
    g.push(text(x, bottom + fs * 1.15, tickLabel(v), tfs, 700, 'middle', cx.ink.muted, 1));
  }
  if (c.axis.yTitle) g.push({ ...text(o[0] + fs * 0.7, top + ph / 2, c.axis.yTitle, fs * 0.8, 700, 'middle', cx.ink.muted, 1), rotate: true });
  if (c.axis.xTitle) g.push(text(left + pw / 2, o[3] - fs * 0.2, c.axis.xTitle, fs * 0.8, 700, 'middle', cx.ink.muted, 1));
  const every = Math.max(1, Math.ceil((cfs * 1.3) / band));
  const catE = cx.p.cats.map((cat, i) => cx.tm.cat(s, i, cat.name));
  const serE = cx.p.ser.map((_, j) => cx.tm.ser(s, j));
  cx.p.cats.forEach((cat, i) => {
    const y = slotY[i];
    const isHl = s.hl.kind === 'category' && s.hl.idx === cat.ri;
    if (c.axis.vgrid) g.push({ ...shape(lineD([left, y], [right, y])), stroke: WHITE, opacity: 0.1, width: 2 });
    if (isHl) g.push({ ...shape(rectD(o[0], y - band / 2, right - o[0] + fs * 0.6, band, [10])), fill: WHITE, opacity: 0.08 });
    const [e, hid] = catE[i];
    const ce = Math.max(e, hid ? 0 : 0.0001);
    if (i % every === 0 || isHl) g.push(text(left - fs * 0.6, y + cfs * 0.35, cat.name, cfs, isHl ? 900 : 700, 'end', isHl ? cx.ink.main : cx.ink.cat, hid ? 0.3 : Math.min(1, 0.3 + ce)));
  });
  const clip: [number, number, number, number] = [left - 4, top - 30, o[1] - left + 8, ph + 60];
  const dimOf = (j: number, ri: number) => hlv(cx, (hl) => (hl.kind === 'none' || (hl.kind === 'series' && hl.idx === j) || (hl.kind === 'category' && hl.idx === ri) ? 1 : 0.22));
  const data: Prim[] = [];
  const labs: Prim[] = [];
  let hidden = 0;
  const gw = band * 0.74;
  const bh = grouped ? Math.max(2, (gw - (ns - 1) * 6) / ns) : gw;
  cx.p.cats.forEach((cat, i) => {
    const cy = slotY[i];
    let pos = 0;
    let neg = 0;
    const tot = cat.vals.reduce<number>((a, v) => a + (v != null && v > 0 ? v : 0), 0);
    for (let j = 0; j < ns; j++) {
      const e = Math.min(catE[i][0], serE[j][0]);
      const { color } = seriesStyle(c, j);
      const op = dimOf(j, cat.ri);
      const y = grouped ? cy - gw / 2 + j * (bh + 6) : cy - gw / 2;
      const raw = tv[i][j];
      if (raw == null) {
        if (e > 0 && grouped) labs.push(text(X(0) + 10, y + bh / 2 + lfs * 0.3, word('', 'n/a', 'n.d.', cx.lang), lfs * 0.8, 800, 'start', cx.ink.na, 1));
        continue;
      }
      const shown = sv[i][j] ?? raw;
      const v = shown * e;
      let x0: number, x1: number;
      if (grouped) ((x0 = X(0)), (x1 = X(v)));
      else if (v >= 0) ((x0 = X(pos)), (x1 = X(pos + v)), (pos += v));
      else ((x0 = X(neg)), (x1 = X(neg + v)), (neg += v));
      const xl = Math.min(x0, x1);
      const ww = Math.abs(x1 - x0);
      if (ww < 0.2 && e < 1) continue;
      const rad = Math.min(8, bh / 4, ww / 2);
      const bar: PShape = { ...shape(rectD(xl, y, ww, bh, v >= 0 ? [0, rad, rad, 0] : [rad, 0, 0, rad])), fill: color, opacity: op };
      if (cat.interp[j]) Object.assign(bar, { fillOpacity: 0.35, stroke: color, width: 3, dash: [8, 6] });
      data.push(bar);
      if (e < 0.6) continue;
      const vj = cat.vals[j];
      const lp = pct ? raw : vj != null && tot > 0 && vj > 0 ? (vj / tot) * 100 : null;
      const lv = pct ? vj : shown;
      const txt = labelText(c, L.name, lv, lp, cx.p.ser[j]);
      if (!txt) continue;
      if (grouped && bh < lfs * 0.9) {
        hidden++;
        continue;
      }
      const tw = tw2(txt, lfs);
      const fits = ww >= tw + lfs * 0.8;
      let lastPos = -1;
      for (let q = 0; q < ns; q++) if ((tv[i][q] ?? 0) > 0) lastPos = q;
      const isEnd = grouped || (v >= 0 && lastPos === j);
      let inside = L.pos === 'inside' || !grouped;
      if (inside && !fits) {
        if (L.overlap === 'offset' && isEnd) inside = false;
        else {
          hidden++;
          continue;
        }
      }
      if (!inside && !grouped && !isEnd) {
        hidden++;
        continue;
      }
      const ty = y + bh / 2 + lfs * 0.35;
      const [tx, anchor]: [number, Anchor] = inside && !grouped ? [xl + ww / 2, 'middle'] : inside ? (v >= 0 ? [x1 - lfs * 0.5, 'end'] : [x1 + lfs * 0.5, 'start']) : v >= 0 ? [x1 + lfs * 0.45, 'start'] : [x1 - lfs * 0.45, 'end'];
      labs.push(text(tx, ty, txt, lfs, 800, anchor, inside ? ink(color) : cx.ink.main, op));
    }
  });
  if (c.refLine.on) {
    const x = X(c.refLine.value);
    if (x >= left - 2 && x <= right + 2) data.push(...refLine(cx, [x, top - fs * 0.3], [x, bottom], [x + lfs * 0.4, top + lfs * 0.6], 'start'));
  }
  data.push(...labs);
  for (const d of data) d.clip = clip;
  cx.hidden += hidden;
  return g.concat(data);
}

interface Slice {
  name: string;
  ri: number;
  i: number;
  target: number;
  color: string;
  group: number[];
  e: number;
  v: number;
  hidden: boolean;
}
interface PieLab {
  inside: boolean;
  side: number;
  x: number;
  y: number;
  ax: number;
  ay: number;
  ex: number;
  txt: string;
  fill: string;
  op: number;
  color: string;
}
const OTHERS_RI = -2;

function pie(cx: Ctx, o: [number, number, number, number], legend: LegendItem[], out: Out): Prim[] {
  const { c, st: s, fs } = cx;
  const pc = c.pie;
  const g: Prim[] = [];
  const ns = cx.p.ser.length;
  const j = Math.min(Math.max(0, Math.trunc(pc.series)), Math.max(0, ns - 1));
  let slices: Slice[] = [];
  cx.p.cats.forEach((cat, i) => {
    // Values after the « Missing » rule (zero, interpolated, or a gap).
    const raw = cat.vals[j] ?? null;
    if (raw == null) out.pieMissing++;
    else if (raw < 0) out.pieNeg.push([cat.name, raw]);
    else slices.push({ name: cat.name, ri: cat.ri, i, target: raw, color: sliceColor(c, i), group: [], e: 0, v: 0, hidden: false });
  });
  const totalT = slices.reduce((a, x) => a + x.target, 0);
  out.pieZero = !(totalT > 0);
  if (pc.sort === 'desc') slices.sort((a, b) => b.target - a.target);
  if (pc.sort === 'asc') slices.sort((a, b) => a.target - b.target);
  if (pc.others && totalT > 0) {
    const small = slices.filter((x) => (x.target / totalT) * 100 < pc.othersPct);
    if (small.length >= 2) {
      slices = slices.filter((x) => !small.includes(x));
      slices.push({ name: word(c.texts.others, 'Others', 'Autres', cx.lang), ri: OTHERS_RI, i: -1, target: small.reduce((a, x) => a + x.target, 0), color: OTHERS, group: small.map((x) => x.i), e: 0, v: 0, hidden: false });
      out.pieOthers = small.length;
    }
  }
  const serName = cx.p.ser[j] ?? '';
  for (const sl of slices) {
    if (sl.ri === OTHERS_RI) {
      let e = 0;
      let all = true;
      let v = 0;
      for (const i of sl.group) {
        const cat = cx.p.cats[i];
        const [ce, hid] = cx.tm.cat(s, i, cat.name);
        e = Math.max(e, ce);
        all = all && hid;
        v += tweenVal(s, cx.k, cat.name, serName, cat.vals[j]) ?? 0;
      }
      sl.e = e;
      sl.hidden = all;
      sl.v = v * e;
    } else {
      const cat = cx.p.cats[sl.i];
      const [ce, hid] = cx.tm.cat(s, sl.i, cat.name);
      sl.e = ce;
      sl.hidden = hid;
      sl.v = (tweenVal(s, cx.k, cat.name, serName, sl.target) ?? 0) * ce;
    }
  }
  for (const sl of slices) legend.push({ name: sl.name, color: sl.color, dim: s.hl.kind === 'category' && s.hl.idx !== sl.ri, op: sl.hidden ? 0.25 : 1 });
  const total = slices.reduce((a, x) => a + x.v, 0);
  const L = c.labels;
  const outside = L.pos === 'outside';
  const cxp = (o[0] + o[1]) / 2;
  const cyp = (o[2] + o[3]) / 2;
  const lfs = fs * 0.8;
  const maxLab = outside
    ? Math.min(
        260,
        slices.reduce((a, x) => Math.max(a, tw2(labelText(c, L.name, x.target, 0, x.name) + '00', lfs)), 60),
      )
    : 0;
  const R = Math.max(30, Math.min((o[3] - o[2]) / 2 - (outside ? lfs * 0.8 : 6), (o[1] - o[0]) / 2 - (outside ? maxLab + fs * 2.2 : 6)) - pc.explodeDist * 0.6);
  const R0 = (R * clamp(pc.donut, 0, 95)) / 100;
  if (totalT <= 0 || total <= 1e-9) {
    g.push({ ...shape(circleD(cxp, cyp, R - 3)), stroke: WHITE, opacity: 0.35, width: 6, dash: [18, 14] });
    if (totalT <= 0) g.push(text(cxp, cyp + fs * 0.35, word(c.texts.noData, 'No data', 'Aucune donnée', cx.lang), fs * 1.2, 800, 'middle', cx.ink.muted, 1));
    return g;
  }
  const sweep = cx.tm.fx !== 'sweep' ? 360 : 360 * cx.tm.ep(cx.tm.age);
  let a = pc.start;
  const labs: PieLab[] = [];
  const prio: number[] = [];
  const edge = c.panel.on ? c.panel.color : '#0b0d14';
  for (const sl of slices) {
    const span = (sl.v / total) * 360;
    const a0 = a;
    const a1 = a + span;
    a = a1;
    if (span <= 0.01) continue;
    const vis1 = Math.min(a1, pc.start + sweep);
    const mid = (a0 + a1) / 2;
    const exploded = pc.explode >= 0 && (sl.ri === pc.explode || sl.group.some((i) => cx.p.cats[i].ri === pc.explode));
    const expl = (exploded ? pc.explodeDist : 0) + hlv(cx, (hl) => (hl.kind === 'category' && hl.idx === sl.ri ? fs * 0.9 : 0));
    const dx = expl * Math.sin(mid * D2R);
    const dy = -expl * Math.cos(mid * D2R);
    const op = hlv(cx, (hl) => (hl.kind === 'category' && hl.idx !== sl.ri ? 0.3 : 1));
    if (vis1 > a0) g.push({ ...shape(sliceD(cxp + dx, cyp + dy, R0, R, a0, vis1)), fill: sl.color, stroke: edge, width: 3, round: true, opacity: op });
    const pctv = (sl.target / totalT) * 100;
    const name = sl.ri === OTHERS_RI ? `${sl.name} (${sl.group.length})` : sl.name;
    const txt = labelText(c, L.name, sl.target, pctv, name);
    if (!txt || pc.start + sweep < mid || sl.e < 0.6) continue;
    const small = pctv < L.minSize;
    if (small && L.overlap === 'hide') {
      cx.hidden++;
      continue;
    }
    const goOut = outside || (small && L.overlap === 'offset') || tw2(txt, lfs) > (R - R0) * span * D2R * 0.9 + (R - R0) * 0.3;
    if (!outside && goOut && L.overlap === 'hide') {
      cx.hidden++;
      continue;
    }
    const sx = Math.sin(mid * D2R);
    const sy = -Math.cos(mid * D2R);
    if (!goOut) {
      const rr = R0 > 0 ? (R0 + R) / 2 : R * 0.62;
      labs.push({ inside: true, side: 0, x: cxp + dx + sx * rr, y: cyp + dy + sy * rr + lfs * 0.35, ax: 0, ay: 0, ex: 0, txt, fill: ink(sl.color), op, color: sl.color });
    } else labs.push({ inside: false, side: sx >= 0 ? 1 : -1, x: 0, y: cyp + dy + sy * (R + fs * 0.9), ax: cxp + dx + sx * (R + 4), ay: cyp + dy + sy * (R + 4), ex: cxp + dx + sx * (R + fs * 0.9), txt, fill: cx.ink.main, op, color: sl.color });
    prio.push(pctv);
  }
  const hide = labs.map(() => false);
  for (const side of [1, -1]) {
    const ids = labs.map((_, i) => i).filter((i) => !labs[i].inside && labs[i].side === side);
    const tmp = ids.map((i) => lab(0, labs[i].y, '', WHITE, prio[i], 1));
    cx.hidden += spread(tmp, lfs * 1.25, o[2] + lfs * 0.6, o[3] - lfs * 0.2, L.overlap);
    ids.forEach((i, n) => ((labs[i].y = tmp[n].y), (hide[i] = tmp[n].hide)));
  }
  labs.forEach((lb, i) => {
    if (hide[i]) return;
    if (lb.inside) return void g.push(text(lb.x, lb.y, lb.txt, lfs, 800, 'middle', lb.fill, lb.op));
    const tx = cxp + lb.side * (R + fs * 2);
    const ex = lb.side > 0 ? Math.min(lb.ex, tx - 10) : Math.max(lb.ex, tx + 10);
    g.push({ ...shape(`M${lb.ax} ${lb.ay}L${ex} ${lb.y}L${tx - lb.side * 6} ${lb.y}`), stroke: lb.color, width: 3, opacity: lb.op });
    g.push(text(tx, lb.y + lfs * 0.35, lb.txt, lfs, 800, lb.side > 0 ? 'start' : 'end', lb.fill, lb.op));
  });
  if (R0 > fs * 2.5 && pc.centerTotal) {
    g.push(text(cxp, cyp + fs * 0.2, fmt(c, totalT), Math.min(R0 * 0.45, fs * 1.8), 900, 'middle', cx.ink.main, 1));
    g.push(text(cxp, cyp + fs * 1.3, word(c.texts.total, 'Total', 'Total', cx.lang), fs * 0.75, 700, 'middle', cx.ink.muted, 1));
  }
  return g;
}

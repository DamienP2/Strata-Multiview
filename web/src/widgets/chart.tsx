// The Chart widget: a measure of the plant over the last minutes, as a curve — PTP and NTP offsets,
// processors, memory, the time to draw a picture, an input's pictures a second or its loudness.
// Several make a dashboard in a multiview. Laid out as items, mirroring
// crates/strata-mv-core/src/wid/chart.rs (keep in sync); the Rust engine draws the same list.
import { ChartLine } from 'lucide-react';
import { useInputs, useSignal, useSystem, type Curves, type SignalView } from '../engine/liveInputs';
import type { Panel } from '../engine/types';
import { t } from '../i18n';
import { AMBER, Items, path, RED, rect, text, type Item } from './broadcast';
import { ColorPick, defineWidget, Num, Section, Select, Text, Toggle, withDefaults } from './kit';

export type ChartMetric = 'ptpOffset' | 'ntpOffset' | 'cpu' | 'memory' | 'serverCpu' | 'ffmpegCpu' | 'renderTime' | 'inputFps' | 'loudness';

export interface ChartConfig {
  metric?: ChartMetric;
  input?: string;
  channel?: string;
  title?: string;
  window?: number;
  min?: number;
  max?: number;
  warn?: number;
  crit?: number;
  color?: string;
  fill?: boolean;
  stats?: boolean;
  background?: string;
}
const D_CHART = { metric: 'ptpOffset' as ChartMetric, input: '', channel: '', title: '', window: 120, min: 0, max: 0, warn: 0, crit: 0, color: '#4ed8ff', fill: true, stats: true, background: '#0b0d12d9' };
type C = typeof D_CHART;

const DIM = '#9aa3b2';
const WHITE = '#ffffff';
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
const padOf = (w: number, h: number) => Math.round(clamp(Math.min(w, h) * 0.04, 3, 14));
const PERCENT: ChartMetric[] = ['cpu', 'memory', 'serverCpu', 'ffmpegCpu'];
const isOffset = (m: ChartMetric) => m === 'ptpOffset' || m === 'ntpOffset';

/** Its key in the curves (mirrors key_of); null: from the input (loudness). */
function keyOf(c: C, curves: Curves | undefined): string | null {
  switch (c.metric) {
    case 'ptpOffset':
      return 'ptp';
    case 'ntpOffset':
      return 'ntp';
    case 'cpu':
      return 'cpu';
    case 'memory':
      return 'memory';
    case 'serverCpu':
      return 'server';
    case 'ffmpegCpu':
      return 'ffmpeg';
    case 'renderTime': {
      const ch = c.channel.trim();
      if (ch) return `render:${ch}`;
      return Object.keys(curves?.series ?? {}).sort().find((k) => k.startsWith('render:')) ?? 'render:main';
    }
    case 'inputFps':
      return `fps:${c.input.trim()}`;
    default:
      return null;
  }
}

const NAMES: Record<ChartMetric, [string, string]> = {
  ptpOffset: ['Écart PTP', 'PTP offset'],
  ntpOffset: ['Écart NTP', 'NTP offset'],
  cpu: ['Processeurs', 'Processors'],
  memory: ['Mémoire', 'Memory'],
  serverCpu: ['Strata Multiview', 'Strata Multiview'],
  ffmpegCpu: ['Décodeurs (ffmpeg)', 'Decoders (ffmpeg)'],
  renderTime: ['Temps de rendu', 'Render time'],
  inputFps: ['Images par seconde', 'Pictures a second'],
  loudness: ['Loudness court terme', 'Short-term loudness'],
};

const minus = (s: string) => (s.startsWith('-') ? `−${s.slice(1)}` : s);

/** A value with its unit (mirrors value_text). */
export function valueText(m: ChartMetric, v: number): string {
  if (m === 'ptpOffset') {
    const a = Math.abs(v);
    return minus(a < 1000 ? `${v.toFixed(0)} ns` : a < 1e6 ? `${(v / 1000).toFixed(1)} µs` : `${(v / 1e6).toFixed(2)} ms`);
  }
  if (m === 'ntpOffset') return minus(`${v.toFixed(2)} ms`);
  if (PERCENT.includes(m)) return minus(`${v.toFixed(0)} %`);
  if (m === 'renderTime') return minus(`${v.toFixed(1)} ms`);
  if (m === 'inputFps') return minus(`${v.toFixed(1)} fps`);
  return minus(`${v.toFixed(1)} LUFS`);
}

/** The window's measures, the newest last (mirrors values). */
function values(c: C, curves: Curves | undefined, source: SignalView | null): (number | null)[] {
  const win = clamp(c.window, 30, 300);
  const k = keyOf(c, curves);
  const all = k !== null ? (curves?.series[k] ?? []) : (source?.history ?? []).map((v) => (v > -70 ? v : null));
  return all.slice(Math.max(0, all.length - win));
}

/** The scale (mirrors scale). */
function scale(c: C, vals: (number | null)[]): [number, number] {
  if (c.max > c.min) return [c.min, c.max];
  const known = vals.filter((v): v is number => v !== null);
  const lo = Math.min(...known);
  const hi = Math.max(...known);
  if (PERCENT.includes(c.metric)) return [0, 100];
  if (isOffset(c.metric)) {
    const m = known.reduce((a, v) => Math.max(a, Math.abs(v)), c.metric === 'ptpOffset' ? 100 : 1) * 1.2;
    return [-m, m];
  }
  if (c.metric === 'renderTime') return [0, known.length ? Math.max(hi * 1.25, 10) : 40];
  if (c.metric === 'inputFps') return known.length ? [Math.floor(Math.max(lo - 2, 0)), Math.ceil(hi + 2)] : [0, 60];
  return known.length ? [Math.floor(Math.min(lo - 3, -30)), Math.ceil(Math.max(hi + 3, -15))] : [-40, -10];
}

/** The colour of a value against the thresholds (mirrors level_color). */
function levelColor(c: C, v: number): string {
  const size = isOffset(c.metric) ? Math.abs(v) : v;
  return c.crit > 0 && size >= c.crit ? RED : c.warn > 0 && size >= c.warn ? AMBER : c.color;
}

/** The Chart widget (mirrors chart). */
export function chart(cfg: ChartConfig, w: number, h: number, curves: Curves | undefined, source: SignalView | null, sourceName: string, lang: string): Item[] {
  const c = withDefaults(D_CHART, cfg) as C;
  const fr = lang.toLowerCase().startsWith('fr');
  const pad = padOf(w, h);
  const out: Item[] = [rect([0, 0, w, h], pad * 0.6, c.background)];
  const vals = values(c, curves, source);
  const last = [...vals].reverse().find((v): v is number => v !== null);
  const fs = Math.round(clamp(Math.min(h * 0.07, w * 0.045), 8, 24));
  const head = fs * 1.8;
  let title = c.title.trim();
  if (!title) {
    title = NAMES[c.metric][fr ? 0 : 1];
    if ((c.metric === 'inputFps' || c.metric === 'loudness') && sourceName) title = `${title} · ${sourceName}`;
  }
  const vw = (w - 2 * pad) * 0.42;
  out.push(text([pad, pad, w - 2 * pad - vw, head], title, fs, 600, false, WHITE, 'left'));
  out.push(text([w - pad - vw, pad, vw, head], last === undefined ? '—' : valueText(c.metric, last), fs * 1.25, 700, true, last === undefined ? DIM : levelColor(c, last), 'right'));
  const foot = c.stats ? fs * 1.4 : 0;
  const labelW = fs * 3.6;
  const plot = [pad + labelW, pad + head + fs * 0.4, Math.max(1, w - 2 * pad - labelW), Math.max(1, h - 2 * pad - head - fs * 0.4 - foot)];
  const [lo, hi] = scale(c, vals);
  const span = Math.max(hi - lo, 1e-6);
  const yOf = (v: number) => plot[1] + plot[3] * (1 - clamp((v - lo) / span, 0, 1));
  const lw = clamp(h / 300, 1, 2.5);
  [hi, (hi + lo) / 2, lo].forEach((v, i) => {
    const y = yOf(v);
    out.push(path([[plot[0], y], [plot[0] + plot[2], y]], false, 1, i === 1 ? '#ffffff14' : '#ffffff24'));
    out.push(text([pad, y - fs * 0.55, labelW - fs * 0.4, fs * 1.1], valueText(c.metric, v), fs * 0.62, 500, true, DIM, 'right'));
  });
  for (const [th, col] of [
    [c.warn, AMBER],
    [c.crit, RED],
  ] as const) {
    if (th <= 0) continue;
    for (const s of isOffset(c.metric) ? [1, -1] : [1]) {
      const v = th * s;
      if (v > lo && v < hi) out.push(path([[plot[0], yOf(v)], [plot[0] + plot[2], yOf(v)]], false, 1, `${col}99`));
    }
  }
  const win = clamp(c.window, 30, 300);
  const step = plot[2] / Math.max(win - 1, 1);
  const n = vals.length;
  const runs: [number, number][][] = [[]];
  vals.forEach((v, i) => {
    const x = plot[0] + plot[2] - (n - 1 - i) * step;
    if (v !== null) runs[runs.length - 1].push([x, yOf(v)]);
    else if (runs[runs.length - 1].length) runs.push([]);
  });
  // Filled to zero when the scale holds it, else to its bottom.
  const base = yOf(lo < 0 && hi > 0 ? 0 : lo);
  for (const run of runs.filter((r) => r.length)) {
    if (c.fill && run.length > 1) out.push({ path: [...run, [run[run.length - 1][0], base], [run[0][0], base]], closed: true, width: 0, color: c.color, fill: `${c.color.slice(0, 7)}33` });
    out.push(path(run.length === 1 ? [run[0], [run[0][0] + 1, run[0][1]]] : run, false, lw, c.color));
  }
  if (c.stats) {
    const known = vals.filter((v): v is number => v !== null);
    let s = '—';
    if (known.length) {
      const mean = known.reduce((a, v) => a + v, 0) / known.length;
      const [loW, meanW, hiW] = fr ? ['min', 'moy.', 'max'] : ['min', 'mean', 'max'];
      s = `${loW} ${valueText(c.metric, Math.min(...known))} · ${meanW} ${valueText(c.metric, mean)} · ${hiW} ${valueText(c.metric, Math.max(...known))}`;
    }
    out.push(text([plot[0], h - pad - foot, plot[2], foot], s, fs * 0.68, 500, true, DIM, 'right'));
  }
  return out;
}

function ChartView({ cfg, w, h, lang }: { cfg: ChartConfig; w: number; h: number; lang: string }) {
  const sys = useSystem();
  const id = (cfg.input ?? '').trim();
  const signal = useSignal(id);
  const inputs = useInputs();
  const name = inputs.find((i) => i.id === id)?.name ?? '';
  return <Items items={chart(cfg, w, h, sys.curves, id ? signal : null, name, lang)} look={(cfg as { panel?: Panel }).panel} />;
}

const METRICS: [ChartMetric, string][] = [
  ['ptpOffset', 'PTP offset'],
  ['ntpOffset', 'NTP offset'],
  ['cpu', 'Processors'],
  ['memory', 'Memory'],
  ['serverCpu', 'Strata Multiview (processors)'],
  ['ffmpegCpu', 'Decoders (ffmpeg)'],
  ['renderTime', 'Render time'],
  ['inputFps', 'Pictures a second of an input'],
  ['loudness', 'Short-term loudness of an input'],
];

export const chartWidget = defineWidget({
  id: 'chart',
  type: 'chart',
  label: 'Chart',
  group: 'Multiview',
  icon: ChartLine,
  create: () => ({ props: { width: 640, height: 300 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, lang }) => <ChartView cfg={layer as ChartConfig} w={w} h={h} lang={lang ?? 'fr'} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_CHART, layer as ChartConfig) as C;
    const inputs = useInputs();
    const withInput = c.metric === 'inputFps' || c.metric === 'loudness';
    const unit = c.metric === 'ptpOffset' ? ' ns' : c.metric === 'ntpOffset' || c.metric === 'renderTime' ? ' ms' : PERCENT.includes(c.metric) ? ' %' : '';
    return (
      <Section id="chart" title={t('Chart')}>
        <Select label={t('Measure')} value={c.metric} onChange={(v) => set('metric', v)} options={METRICS.map(([k, l]) => [k, t(l)] as [ChartMetric, string])} />
        {withInput && <Select label={t('Input')} value={c.input} onChange={(v) => set('input', v)} options={[['', t('None')] as [string, string], ...inputs.map((i) => [i.id, i.name] as [string, string])]} />}
        {c.metric === 'renderTime' && <Text label={t('Channel')} value={c.channel} placeholder="main" onChange={(v) => set('channel', v)} />}
        <Text label={t('Title')} value={c.title} placeholder={t('The measure’s name')} onChange={(v) => set('title', v)} />
        <Num label={t('Duration')} value={c.window} min={30} max={300} step={10} unit=" s" onChange={(v) => set('window', v)} />
        <Num label={t('Scale from')} value={c.min} slider={false} unit={unit} onChange={(v) => set('min', v)} />
        <Num label={t('Scale to')} value={c.max} slider={false} unit={unit} onChange={(v) => set('max', v)} />
        <Num label={t('Amber from')} value={c.warn} min={0} slider={false} unit={unit} onChange={(v) => set('warn', v)} />
        <Num label={t('Red from')} value={c.crit} min={0} slider={false} unit={unit} onChange={(v) => set('crit', v)} />
        <ColorPick label={t('Curve')} value={c.color} onChange={(v) => set('color', v)} />
        <Toggle label={t('Fill')} text={t('The area under the curve')} value={c.fill} onChange={(v) => set('fill', v)} />
        <Toggle label={t('Figures')} text={t('Lowest, mean and highest')} value={c.stats} onChange={(v) => set('stats', v)} />
      </Section>
    );
  },
});

export default chartWidget;

// Numbers of the performance views: percentages, cores, bytes, frame rates,
// in the interface's language (decimal comma and no-break spaces in French).
import { lang, locale, t, tn } from '../i18n';
import type { ChannelMetrics, InputMetrics, OutputMetrics, PictureMetrics } from '../engine/metrics';

export type Tone = 'ok' | 'warn' | 'bad' | 'idle';

const formats = new Map<string, Intl.NumberFormat>();
/** `v` with at most `digits` decimals. */
export function num(v: number, digits = 0): string {
  const key = `${locale() ?? 'en'}|${digits}`;
  let f = formats.get(key);
  if (!f) formats.set(key, (f = new Intl.NumberFormat(locale(), { maximumFractionDigits: digits })));
  return f.format(Number.isFinite(v) ? v : 0);
}

/** "46%" / "46 %". */
export const pct = (v: number) => `${num(Math.round(v))}${lang() === 'fr' ? ' %' : '%'}`;

/** A frame rate: 50, 47.2, 59.94. */
export const fps = (v: number) => num(v, v >= 100 ? 0 : 2);

/** Cores: 0.39, 1.62, 12.8. */
export const cores = (v: number) => num(v, v >= 10 ? 1 : 2);

/** "0.39 cores", "1 core" (French: singular under 2). */
export const coresLabel = (v: number) => tn(v, '{n} core', '{n} cores', { n: cores(v) });

/*
 * Processor use is shown in % of one core (100 % = one core busy), with the
 * number of cores: "250 % · 4 cores" (at most 400 %). The server gives the
 * machine and its processes in % of the whole machine (0–100) and threads in
 * cores (1.02): these turn either into the same unit.
 */
/** Cores in use (1.02) → "102 %". */
export const corePct = (used: number) => pct(used * 100);
/** A share of the whole machine (0–100) on `n` cores → % of one core: 100 % of 4 cores → "400 %". */
export const machinePct = (v: number, n: number) => pct(v * Math.max(1, n));
/** "400 % · 4 cores": the CPU of the machine (or of a process) and its cores. */
export const cpuLabel = (v: number, n: number) => `${machinePct(v, n)} · ${tn(n, '{n} core', '{n} cores')}`;

/** "12 GB", "850 MB" (never broken across lines). */
export function bytes(n: number): string {
  const s = n >= 1024 ** 3 ? t('{n} GB', { n: num(n / 1024 ** 3, 1) }) : t('{n} MB', { n: num(n / 1024 ** 2, 0) });
  return s.replace(' ', '\u00a0');
}

/** HH:MM:SS.mmm, local time. */
export function clock(ms: number): string {
  const d = new Date(ms);
  const p = (v: number, n = 2) => String(v).padStart(n, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}

/** Frames per second against the target: ok, a little short, far short. */
export function fpsTone(actual: number, target: number): Tone {
  if (!target) return 'idle';
  const r = actual / target;
  return r >= 0.98 ? 'ok' : r >= 0.9 ? 'warn' : 'bad';
}

const worst = (...ts: Tone[]): Tone => (ts.includes('bad') ? 'bad' : ts.includes('warn') ? 'warn' : ts.includes('ok') ? 'ok' : 'idle');
export { worst as worstTone };

/** Memory: amber above 85 %, red above 95 %. */
export const memTone = (v: number): Tone => (v >= 95 ? 'bad' : v >= 85 ? 'warn' : 'ok');

const loadOf = (v: number): Tone => (v >= 85 ? 'bad' : v >= 60 ? 'warn' : 'ok');

export const pictureTone = (p: PictureMetrics, rate: number): Tone => worst(fpsTone(p.fps, rate), loadOf(p.load), p.repeated > 0 ? 'warn' : 'ok');

export function outputTone(o: OutputMetrics, rate: number): Tone {
  if (!o.ok) return 'bad';
  return worst(fpsTone(o.fps, rate), o.dropped > 0 ? 'warn' : 'ok');
}

export function inputTone(i: InputMetrics): Tone {
  if (!i.live) return 'idle';
  return worst(fpsTone(i.fps, i.nominal), i.dropped > 0 ? 'warn' : 'ok');
}

export const channelTone = (c: ChannelMetrics): Tone => worst('ok', ...c.pictures.map((p) => pictureTone(p, c.rate)), ...c.pictures.flatMap((p) => p.outputs.map((o) => outputTone(o, c.rate))));

/** The picture furthest from its rate (what a card shows). */
export function slowest(chans: ChannelMetrics[]): { fps: number; rate: number } | null {
  let best: { fps: number; rate: number; r: number } | null = null;
  for (const c of chans)
    for (const p of c.pictures) {
      const r = c.rate ? p.fps / c.rate : 1;
      if (!best || r < best.r) best = { fps: p.fps, rate: c.rate, r };
    }
  return best && { fps: best.fps, rate: best.rate };
}

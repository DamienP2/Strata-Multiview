// The broadcast widgets of a multiview, besides the Source tile: audio meters (up to 16 channels, an
// input with or without picture), loudness (EBU R128), timecode, the state of the plant (PTP, NTP,
// faults) and the alarm journal. Each one is laid out as a list of rectangles and texts, mirroring
// crates/strata-mv-core/src/wid/broadcast.rs (keep in sync); the Rust engine draws the same list
// (crates/strata-mv-render/src/wid/broadcast.rs).
import { Activity, AudioLines, BellRing, Gauge, Timer } from 'lucide-react';
import { alarmText, smoothLevels, useEveryFrame, useInputs, useSignal, useSystem, type SignalView, type SystemView } from '../engine/liveInputs';
import { zonedFields } from '../engine/widgets';
import type { Panel } from '../engine/types';
import { t } from '../i18n';
import { ColorPick, defineWidget, Num, Section, Select, Text, Toggle, withDefaults } from './kit';
import { FitLine } from './text-fit';

type Area = [number, number, number, number];
type Align = 'left' | 'center' | 'right';

/** What a broadcast widget draws (mirrors broadcast::Item). */
export type Item = { rect: Area; radius: number; color: string } | { text: string; box: Area; size: number; weight: number; mono: boolean; color: string; align: Align };

const rect = (a: Area, radius: number, color: string): Item => ({ rect: a, radius, color });
const text = (box: Area, s: string, size: number, weight: number, mono: boolean, color: string, align: Align): Item => ({ text: s, box, size, weight, mono, color, align });

export const GREEN = '#2bd96b';
export const YELLOW = '#f2c12e';
export const RED = '#ff3b3b';
export const AMBER = '#fbbf24';
const TRACK = '#ffffff14';
const DIM = '#9aa3b2';
const WHITE = '#ffffff';
export const WARN = ['#fbbf24', '#1c1300'];
export const CRIT = ['#dc2626', '#ffffff'];
/** EBU R128 (mirrors signal::TARGET, MAX_TRUE_PEAK). */
export const TARGET = -23;
const MAX_TRUE_PEAK = -1;
const SILENCE = -120;
const DASH = '—';

const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
const round1 = (v: number) => Math.round(v * 10) / 10;
/** One decimal, a real minus sign (mirrors num1). */
export const num1 = (v: number) => {
  const r = round1(v);
  return r < 0 ? `−${(-r).toFixed(1)}` : r.toFixed(1);
};
const padOf = (w: number, h: number) => Math.round(clamp(Math.min(w, h) * 0.04, 3, 14));
const fr = (lang: string) => lang.toLowerCase().startsWith('fr');

/** On target (green), off it (amber), the true peak over the limit (red) (mirrors loud_color). */
export const loudColor = (lufs: number, tp: number, target: number, tol: number) => (tp > MAX_TRUE_PEAK ? RED : Math.abs(lufs - target) <= tol ? GREEN : AMBER);

// ------------------------------------------------------------ meters --

export interface MeterConfig {
  input?: string;
  channels?: number;
  labels?: string;
  scale?: boolean;
  values?: boolean;
  hold?: boolean;
  loudness?: boolean;
  min?: number;
  yellow?: number;
  red?: number;
  background?: string;
}
const D_METER = { input: '', channels: 0, labels: '', scale: true, values: false, hold: true, loudness: true, min: -60, yellow: -18, red: -9, background: '#0b0d12d9' };
const onScale = (min: number, db: number) => {
  const m = Math.min(min, -1);
  return clamp((db - m) / -m, 0, 1);
};

export function meters(cfg: MeterConfig, w: number, h: number, v: SignalView): Item[] {
  const c = withDefaults(D_METER, cfg);
  const pad = padOf(w, h);
  const out: Item[] = [rect([0, 0, w, h], pad * 0.6, c.background)];
  const levels = v.levels ?? [];
  const n = c.channels === 0 ? (levels.length === 0 ? 2 : Math.min(levels.length, 16)) : Math.min(c.channels, 16);
  const fs = Math.round(clamp(h * 0.035, 8, 18));
  const sw = c.scale ? Math.round(fs * 2.2) : 0;
  const top = pad + (c.values ? fs * 1.5 : 0);
  const lufsH = c.loudness ? fs * 1.8 : 0;
  const bottom = h - pad - fs * 1.5 - lufsH;
  const x0 = pad + sw;
  const span = Math.max(1, w - x0 - pad);
  const gap = Math.round(clamp((span / n) * 0.18, 1, 10));
  const bw = Math.max(1, (span - gap * (n - 1)) / n);
  const bh = Math.max(1, bottom - top);
  const labels = c.labels
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  if (c.scale)
    for (const d of [0, -6, -12, -18, -24, -36, -48, -60]) {
      if (d < c.min) continue;
      const y = top + bh * (1 - onScale(c.min, d));
      out.push(text([pad, y - fs * 0.6, sw - fs * 0.3, fs * 1.2], String(d), fs * 0.8, 500, true, DIM, 'right'));
    }
  const [y1, r1] = [onScale(c.min, c.yellow), onScale(c.min, Math.max(c.red, c.yellow))];
  for (let i = 0; i < n; i++) {
    const x = x0 + i * (bw + gap);
    const [peak, , hold] = levels[i] ?? [SILENCE, SILENCE, SILENCE];
    out.push(rect([x, top, bw, bh], 0, TRACK));
    const lit = onScale(c.min, peak);
    for (const [from, to, col] of [
      [0, y1, GREEN],
      [y1, r1, YELLOW],
      [r1, 1, RED],
    ] as const) {
      const tt = Math.min(lit, to);
      if (tt > from) out.push(rect([x, top + bh * (1 - tt), bw, bh * (tt - from)], 0, col));
    }
    if (c.hold && hold > c.min) {
      const k = onScale(c.min, hold);
      const th = clamp(bh * 0.008, 1, 3);
      const y = Math.min(top + bh * (1 - k), top + bh - th);
      out.push(rect([x, y, bw, th], 0, hold >= c.red ? RED : hold >= c.yellow ? YELLOW : GREEN));
    }
    if (c.values) {
      const s = peak <= SILENCE + 1 || i >= levels.length ? DASH : String(Math.round(peak));
      out.push(text([x - gap / 2, pad, bw + gap, fs * 1.3], s, fs * 0.8, 600, true, DIM, 'center'));
    }
    out.push(text([x - gap / 2, bottom + fs * 0.25, bw + gap, fs * 1.2], labels[i] ?? String(i + 1), fs * 0.85, 600, false, WHITE, 'center'));
  }
  if (c.loudness) {
    const y = h - pad - lufsH;
    const l = v.loudness;
    const s = l ? `I ${num1(l[2])}  S ${num1(l[1])}  TP ${num1(l[4])}` : `LUFS ${DASH}`;
    out.push(text([pad, y + fs * 0.3, w - 2 * pad, fs * 1.3], s, fs, 700, true, l ? loudColor(l[2], l[4], TARGET, 1) : DIM, 'center'));
  }
  return out;
}

// ---------------------------------------------------------- loudness --

export interface LoudnessConfig {
  input?: string;
  target?: number;
  tolerance?: number;
  history?: boolean;
  background?: string;
}
const D_LOUD = { input: '', target: TARGET, tolerance: 1, history: true, background: '#0b0d12d9' };

export function loudness(cfg: LoudnessConfig, w: number, h: number, v: SignalView, lang: string): Item[] {
  const c = withDefaults(D_LOUD, cfg);
  const pad = padOf(w, h);
  const out: Item[] = [rect([0, 0, w, h], pad * 0.6, c.background)];
  const l = v.loudness;
  const f = fr(lang);
  const inner = w - 2 * pad;
  const big = Math.round(clamp(h * 0.16, 12, 96));
  out.push(text([pad, pad, inner * 0.62, big * 1.15], l ? num1(l[2]) : DASH, big, 700, true, l ? loudColor(l[2], l[4], c.target, c.tolerance) : DIM, 'left'));
  const small = Math.round(Math.max(big * 0.3, 7));
  out.push(text([pad + inner * 0.62, pad + big * 0.1, inner * 0.38, small * 1.3], f ? 'LUFS INTÉGRÉ' : 'LUFS INTEGRATED', small, 700, false, DIM, 'right'));
  out.push(text([pad + inner * 0.62, pad + big * 0.1 + small * 1.4, inner * 0.38, small * 1.3], `${f ? 'CIBLE' : 'TARGET'} ${num1(c.target)} ±${c.tolerance}`, small, 600, false, DIM, 'right'));
  const y = pad + big * 1.3;
  const cells: [string, string, string][] = [
    ['M', l ? num1(l[0]) : DASH, WHITE],
    ['S', l ? num1(l[1]) : DASH, WHITE],
    ['LRA', l ? `${num1(l[3])} LU` : DASH, WHITE],
    ['TP', l ? `${num1(l[4])} dBTP` : DASH, l && l[4] > MAX_TRUE_PEAK ? RED : WHITE],
  ];
  const cw = (inner - pad * 3) / 4;
  const ch = Math.round(small * 3.6);
  cells.forEach(([k, val, col], i) => {
    const x = pad + i * (cw + pad);
    out.push(rect([x, y, cw, ch], pad * 0.4, '#ffffff0d'));
    out.push(text([x + pad * 0.6, y + small * 0.3, cw - pad * 1.2, small * 1.2], k, small, 700, false, DIM, 'left'));
    out.push(text([x + pad * 0.6, y + small * 1.6, cw - pad * 1.2, small * 1.7], val, small * 1.35, 700, true, col, 'left'));
  });
  const hy = y + ch + pad;
  const hh = h - hy - pad;
  if (c.history && hh > 12) {
    out.push(rect([pad, hy, inner, hh], pad * 0.4, '#ffffff0d'));
    const [lo, hi] = [c.target - 13, c.target + 11];
    const k = (x: number) => clamp((x - lo) / (hi - lo), 0, 1);
    const hist = v.history ?? [];
    const slots = 120;
    const sw = inner / slots;
    const shown = hist.slice(-slots);
    const first = slots - shown.length;
    shown.forEach((s, i) => {
      const tt = k(s);
      if (tt <= 0) return;
      out.push(rect([pad + (first + i) * sw, hy + hh * (1 - tt), Math.max(1, sw - 1), hh * tt], 0, s - c.target > c.tolerance ? AMBER : GREEN));
    });
    out.push(rect([pad, hy + hh * (1 - k(c.target)), inner, 1], 0, '#93c5fd'));
  }
  return out;
}

// ---------------------------------------------------------- timecode --

export interface TimecodeConfig {
  input?: string;
  label?: string;
  color?: string;
  background?: string;
}
const D_TC = { input: '', label: 'TC', color: '#fbbf24', background: '#0b0d12d9' };
const p2 = (n: number) => String(n).padStart(2, '0');

/** The station's timecode: the time of day at `fps` (mirrors station_timecode). */
export function stationTimecode(nowMs: number, tz: string, fps: number): string {
  const f = zonedFields(nowMs, tz);
  const rate = fps > 0 ? fps : 25;
  const ms = ((nowMs % 1000) + 1000) % 1000;
  return `${p2(f.hour)}:${p2(f.minute)}:${p2(f.second)}:${p2(Math.floor((ms * rate) / 1000))}`;
}

/** The time of day, "HH:MM:SS" (mirrors clock). */
const clockText = (ms: number, tz: string) => {
  const f = zonedFields(ms, tz);
  return `${p2(f.hour)}:${p2(f.minute)}:${p2(f.second)}`;
};

export function timecode(cfg: TimecodeConfig, w: number, h: number, tc: string, tag: string): Item[] {
  const c = withDefaults(D_TC, cfg);
  const pad = padOf(w, h);
  const out: Item[] = [rect([0, 0, w, h], pad * 0.6, c.background)];
  const hasLabel = c.label.trim() !== '';
  const small = Math.round(clamp(h * 0.16, 7, 28));
  const top = hasLabel ? pad + small * 1.3 : pad;
  if (hasLabel) out.push(text([pad, pad, (w - 2 * pad) * 0.6, small * 1.2], c.label.trim(), small, 700, false, DIM, 'left'));
  out.push(text([pad + (w - 2 * pad) * 0.6, pad, (w - 2 * pad) * 0.4, small * 1.2], tag, small * 0.9, 700, false, DIM, 'right'));
  const size = Math.max(6, h - top - pad) * 0.82;
  out.push(text([pad, top, w - 2 * pad, h - top - pad], tc || '--:--:--:--', size, 700, true, c.color, 'center'));
  return out;
}

// ------------------------------------------------------------ status --

export interface StatusConfig {
  ptp?: boolean;
  ntp?: boolean;
  alarms?: boolean;
  background?: string;
}
const D_STATUS = { ptp: true, ntp: true, alarms: true, background: '#0b0d12d9' };

/** Its lines: name, what it says, colour (mirrors status_lines). */
export function statusLines(cfg: StatusConfig, s: SystemView, lang: string): [string, string, string][] {
  const c = withDefaults(D_STATUS, cfg);
  const f = fr(lang);
  const out: [string, string, string][] = [];
  if (c.ptp) {
    const p = s.ptp;
    let row: [string, string];
    if (p.locked) row = [`${f ? 'VERROUILLÉ' : 'LOCKED'}${p.offsetNs != null ? ` ±${Math.round(Math.abs(p.offsetNs))} ns` : ''}`, GREEN];
    else if (p.running) row = [p.state || '?', AMBER];
    else row = [f ? 'ABSENT' : 'NOT RUNNING', DIM];
    out.push([`PTP · ${f ? 'domaine' : 'domain'} ${p.domain}`, ...row]);
  }
  if (c.ntp) {
    const n = s.ntp;
    const row: [string, string] =
      n.synced === true ? [`${f ? 'SYNCHRO' : 'SYNCED'}${n.offsetMs != null ? ` ${n.offsetMs >= 0 ? '+' : '-'}${Math.abs(n.offsetMs).toFixed(1)} ms` : ''}`, GREEN] : n.synced === false ? [f ? 'NON SYNCHRO' : 'NOT SYNCED', AMBER] : [f ? 'INCONNU' : 'UNKNOWN', DIM];
    out.push([n.source ? `NTP · ${n.source}` : 'NTP', ...row]);
  }
  if (c.alarms) {
    const on = s.journal.filter((e) => e.ended == null);
    const crit = on.filter((e) => e.severity === 'critical').length;
    const row: [string, string] = crit > 0 ? [`${crit} ${f ? 'CRITIQUE(S)' : 'CRITICAL'}`, RED] : on.length ? [`${on.length} ${f ? 'ALERTE(S)' : 'WARNING(S)'}`, AMBER] : [f ? 'AUCUNE' : 'NONE', GREEN];
    out.push([f ? 'ALARMES' : 'ALARMS', ...row]);
  }
  return out;
}

export function status(cfg: StatusConfig, w: number, h: number, s: SystemView, lang: string): Item[] {
  const c = withDefaults(D_STATUS, cfg);
  const pad = padOf(w, h);
  const out: Item[] = [rect([0, 0, w, h], pad * 0.6, c.background)];
  const lines = statusLines(c, s, lang);
  if (!lines.length) return out;
  const rh = (h - 2 * pad) / lines.length;
  // Narrow: the value under its name.
  const stack = w < rh * 5;
  const fs = Math.round(clamp(stack ? Math.min(rh * 0.3, w * 0.06) : rh * 0.4, 7, 30));
  const lh = stack ? rh * 0.5 : rh;
  lines.forEach(([name, val, col], i) => {
    const y = pad + i * rh;
    const dot = fs * 0.7;
    out.push(rect([pad, y + (lh - dot) / 2, dot, dot], dot / 2, col));
    const x = pad + dot + fs * 0.6;
    const tw = w - x - pad;
    const [nameA, valA]: [Area, Area] = stack ? [[x, y, tw, lh], [x, y + lh, tw, lh]] : [[x, y, tw * 0.5, rh], [x + tw * 0.5, y, tw * 0.5, rh]];
    out.push(text(nameA, name, fs, 600, false, WHITE, 'left'));
    out.push(text(valA, val, fs, 700, false, col, stack ? 'left' : 'right'));
  });
  return out;
}

// ------------------------------------------------------------ alarms --

export interface AlarmLogConfig {
  rows?: number;
  ended?: boolean;
  background?: string;
}
const D_LOG = { rows: 8, ended: true, background: '#0b0d12d9' };

export function alarmLog(cfg: AlarmLogConfig, w: number, h: number, s: SystemView, tz: string, lang: string): Item[] {
  const c = withDefaults(D_LOG, cfg);
  const pad = padOf(w, h);
  const out: Item[] = [rect([0, 0, w, h], pad * 0.6, c.background)];
  const f = fr(lang);
  const rows = clamp(c.rows, 1, 40);
  const rh = (h - 2 * pad) / rows;
  // Narrow: the fault under its time and input. As wide as the line
  // allows too.
  const stack = w < rh * 11;
  const fs = Math.round(clamp(stack ? Math.min(rh * 0.3, w * 0.06) : Math.min(rh * 0.42, w * 0.026), 7, 28));
  const lh = stack ? rh * 0.5 : rh;
  const lines = s.journal.filter((e) => c.ended || e.ended == null).slice(0, rows);
  if (!lines.length) {
    out.push(text([pad, pad, w - 2 * pad, rh], f ? 'Aucune alarme' : 'No alarm', fs, 600, false, DIM, 'left'));
    return out;
  }
  const tw = fs * 4.6;
  const cw = fs * 6.4;
  lines.forEach((e, i) => {
    const y = pad + i * rh;
    const over = e.ended != null;
    const fg = over ? DIM : WHITE;
    out.push(text([pad, y, tw, lh], clockText(e.at, tz), fs, 500, true, DIM, 'left'));
    const [bg, chipFg, word] = over ? ['#ffffff1a', DIM, f ? 'FINIE' : 'OVER'] : e.severity === 'critical' ? [CRIT[0], CRIT[1], f ? 'CRITIQUE' : 'CRITICAL'] : [WARN[0], WARN[1], f ? 'ALERTE' : 'WARNING'];
    const chip: Area = [pad + tw + fs * 0.4, y + lh * 0.18, cw, lh * 0.64];
    out.push(rect(chip, fs * 0.25, bg));
    out.push(text(chip, word, fs * 0.72, 800, false, chipFg, 'center'));
    const x = chip[0] + cw + fs * 0.6;
    const rest = Math.max(1, w - x - pad);
    const [inputA, faultA]: [Area, Area] = stack ? [[x, y, rest, lh], [pad, y + lh, w - 2 * pad, lh]] : [[x, y, rest * 0.4, rh], [x + rest * 0.4, y, rest * 0.6, rh]];
    out.push(text(inputA, e.input, fs, 700, false, fg, 'left'));
    out.push(text(faultA, alarmText(e.kind, lang), fs, 600, false, fg, 'left'));
  });
  return out;
}

// ------------------------------------------------------------- drawing --

const hexCss = (c: string) => {
  // #rrggbbaa → rgba(): every browser reads 8-digit hex, kept as is.
  return c;
};

/** The typography of a widget's texts (mirror of model::Panel::family, text_color, text_size). */
const familyOf = (look: Panel | undefined, mono: boolean) => (mono ? look?.figures?.trim() || 'JetBrains Mono' : look?.font?.trim() || undefined);
const colorOf = (look: Panel | undefined, color: string) => (color.toLowerCase() === '#ffffff' && look?.textColor?.trim() ? look.textColor.trim() : color);
const sizeOf = (look: Panel | undefined, size: number) => ((look?.textScale ?? 0) > 0 ? (size * Math.min(400, Math.max(25, look!.textScale!))) / 100 : size);

/** Draws the items (the same as the Rust engine), in the widget's typography. */
export function Items({ items, look }: { items: Item[]; look?: Panel }) {
  return (
    <>
      {items.map((it, i) => {
        if ('rect' in it) return <div key={i} style={{ position: 'absolute', left: it.rect[0], top: it.rect[1], width: it.rect[2], height: it.rect[3], background: hexCss(it.color), borderRadius: it.radius }} />;
        const size = sizeOf(look, it.size);
        return <FitLine key={i} text={it.text} size={size} weight={it.weight} family={familyOf(look, it.mono)} align={it.align} box={it.box} color={colorOf(look, it.color)} fit={{ overflow: 'shrink', minSize: size * 0.55, minSqueeze: 70 }} />;
      })}
    </>
  );
}

function InputPick({ value, onChange, none }: { value: string; onChange: (v: string) => void; none?: string }) {
  const inputs = useInputs();
  return <Select label={t('Input')} value={value} onChange={onChange} options={[['', none ?? t('None')] as [string, string], ...inputs.map((i) => [i.id, i.name] as [string, string])]} />;
}

function MeterView({ c, w, h }: { c: MeterConfig; w: number; h: number }) {
  const id = (c.input ?? '').trim();
  const signal = useSignal(id);
  // Drawn on each frame: the meters on their sound's time (with the pictures), falling smoothly.
  useEveryFrame(!!id);
  return <Items items={meters(c, w, h, id ? { ...signal, levels: smoothLevels(id) } : signal)} look={(c as { panel?: Panel }).panel} />;
}
function LoudnessView({ c, w, h, lang }: { c: LoudnessConfig; w: number; h: number; lang: string }) {
  return <Items items={loudness(c, w, h, useSignal((c.input ?? '').trim()), lang)} look={(c as { panel?: Panel }).panel} />;
}
function TimecodeView({ c, w, h, nowMs, tz, fps, lang }: { c: TimecodeConfig; w: number; h: number; nowMs: number; tz: string; fps: number; lang: string }) {
  const id = (c.input ?? '').trim();
  const signal = useSignal(id);
  const inputs = useInputs();
  const [tc, tag] = id ? [signal.timecode ?? '', inputs.find((i) => i.id === id)?.name ?? ''] : [stationTimecode(nowMs, tz, fps), fr(lang) ? 'STATION' : 'HOUSE'];
  return <Items items={timecode(c, w, h, tc, tag)} look={(c as { panel?: Panel }).panel} />;
}
function StatusView({ c, w, h, lang }: { c: StatusConfig; w: number; h: number; lang: string }) {
  return <Items items={status(c, w, h, useSystem(), lang)} look={(c as { panel?: Panel }).panel} />;
}
function AlarmLogView({ c, w, h, tz, lang }: { c: AlarmLogConfig; w: number; h: number; tz: string; lang: string }) {
  return <Items items={alarmLog(c, w, h, useSystem(), tz, lang)} look={(c as { panel?: Panel }).panel} />;
}

export const audioMeter = defineWidget({
  id: 'audioMeter',
  type: 'audioMeter',
  label: 'Audio meters',
  group: 'Multiview',
  icon: AudioLines,
  create: () => ({ labels: 'L, R', props: { width: 360, height: 420 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h }) => <MeterView c={layer as MeterConfig} w={w} h={h} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_METER, layer as MeterConfig);
    return (
      <Section id="meter" title={t('Audio meters')}>
        <InputPick value={c.input} onChange={(v) => set('input', v)} />
        <Num label={t('Channels')} value={c.channels} min={0} max={16} onChange={(v) => set('channels', v)} />
        <Text label={t('Names')} value={c.labels} placeholder="L, R, C, LFE, Ls, Rs" onChange={(v) => set('labels', v)} />
        <Toggle label={t('Scale')} value={c.scale} onChange={(v) => set('scale', v)} />
        <Toggle label={t('Values')} text={t('The peak above each bar')} value={c.values} onChange={(v) => set('values', v)} />
        <Toggle label={t('Peak hold')} value={c.hold} onChange={(v) => set('hold', v)} />
        <Toggle label={t('Loudness')} text="I · S · TP" value={c.loudness} onChange={(v) => set('loudness', v)} />
        <Num label={t('Scale from')} value={c.min} min={-120} max={-6} unit=" dBFS" onChange={(v) => set('min', v)} />
        <Num label={t('Yellow from')} value={c.yellow} min={-60} max={0} unit=" dBFS" onChange={(v) => set('yellow', v)} />
        <Num label={t('Red from')} value={c.red} min={-60} max={0} unit=" dBFS" onChange={(v) => set('red', v)} />
      </Section>
    );
  },
});

export const loudnessWidget = defineWidget({
  id: 'loudness',
  type: 'loudness',
  label: 'Loudness',
  group: 'Multiview',
  icon: Gauge,
  create: () => ({ props: { width: 560, height: 360 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, lang }) => <LoudnessView c={layer as LoudnessConfig} w={w} h={h} lang={lang ?? 'fr'} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_LOUD, layer as LoudnessConfig);
    return (
      <Section id="loudness" title={t('Loudness')}>
        <InputPick value={c.input} onChange={(v) => set('input', v)} />
        <Num label={t('Target')} value={c.target} min={-40} max={-10} step={0.5} precision={1} unit=" LUFS" onChange={(v) => set('target', v)} />
        <Num label={t('Tolerance')} value={c.tolerance} min={0} max={5} step={0.5} precision={1} unit=" LU" onChange={(v) => set('tolerance', v)} />
        <Toggle label={t('History')} text={t('The last two minutes')} value={c.history} onChange={(v) => set('history', v)} />
      </Section>
    );
  },
});

export const timecodeWidget = defineWidget({
  id: 'timecode',
  type: 'timecode',
  label: 'Timecode',
  group: 'Multiview',
  icon: Timer,
  create: () => ({ props: { width: 420, height: 110 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, time, fps, lang }) => <TimecodeView c={layer as TimecodeConfig} w={w} h={h} nowMs={time.nowMs} tz={time.defaultTz} fps={time.fps || fps} lang={lang ?? 'fr'} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_TC, layer as TimecodeConfig);
    return (
      <Section id="timecode" title={t('Timecode')}>
        <InputPick value={c.input} onChange={(v) => set('input', v)} none={t('Station (time of day)')} />
        <Text label={t('Label')} value={c.label} onChange={(v) => set('label', v)} />
        <ColorPick label={t('Figures')} value={c.color} onChange={(v) => set('color', v)} />
      </Section>
    );
  },
});

export const statusWidget = defineWidget({
  id: 'status',
  type: 'status',
  label: 'Status',
  group: 'Multiview',
  icon: Activity,
  create: () => ({ props: { width: 520, height: 180 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, lang }) => <StatusView c={layer as StatusConfig} w={w} h={h} lang={lang ?? 'fr'} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_STATUS, layer as StatusConfig);
    return (
      <Section id="status" title={t('Status')}>
        <Toggle label="PTP" value={c.ptp} onChange={(v) => set('ptp', v)} />
        <Toggle label="NTP" value={c.ntp} onChange={(v) => set('ntp', v)} />
        <Toggle label={t('Alarms')} value={c.alarms} onChange={(v) => set('alarms', v)} />
      </Section>
    );
  },
});

export const alarmLogWidget = defineWidget({
  id: 'alarmLog',
  type: 'alarmLog',
  label: 'Alarm log',
  group: 'Multiview',
  icon: BellRing,
  create: () => ({ props: { width: 760, height: 320 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, time, lang }) => <AlarmLogView c={layer as AlarmLogConfig} w={w} h={h} tz={time.defaultTz} lang={lang ?? 'fr'} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_LOG, layer as AlarmLogConfig);
    return (
      <Section id="alarmLog" title={t('Alarm log')}>
        <Num label={t('Lines')} value={c.rows} min={1} max={40} onChange={(v) => set('rows', v)} />
        <Toggle label={t('Ended')} text={t('The faults over too, dimmed')} value={c.ended} onChange={(v) => set('ended', v)} />
      </Section>
    );
  },
});

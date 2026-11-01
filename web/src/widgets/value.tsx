// The Value widget: one live value as text — of an input (name, UMD, format, timecode, loudness,
// peaks, codec, colour space, HDR, levels, phase, worst fault, tally) or of the plant (time, date,
// station timecode, PTP, NTP, processors, memory, faults on). Each one a tool of its own, placed and
// styled freely. Laid out as items, mirroring crates/strata-mv-core/src/wid/value.rs (keep in sync).
import { AlignCenter, AlignLeft, AlignRight, Rows2, Columns2, Tag, Variable } from 'lucide-react';
import { alarmText, useEveryFrame, useInputs, useSignal, useSystem, type SignalView, type SystemView } from '../engine/liveInputs';
import type { Panel } from '../engine/types';
import { t } from '../i18n';
import { AMBER, clockText, Items, num1, RED, rect, stationTimecode, text, type Item } from './broadcast';
import { valueText } from './chart';
import { ColorPick, defineWidget, Flags, Num, Section, Seg, Select, Text, withDefaults } from './kit';
import { correlationColor } from './scope';
import { audioLine, dynamicColors, dynamicOf, levelsText, MATRIX, named, PRIMARIES } from './signalInfo';
import { formatName } from './source';
import { zonedFields } from '../engine/widgets';

export type ValueKind =
  | 'name'
  | 'umd'
  | 'format'
  | 'timecode'
  | 'momentary'
  | 'shortTerm'
  | 'integrated'
  | 'loudnessRange'
  | 'truePeak'
  | 'peakLeft'
  | 'peakRight'
  | 'codec'
  | 'picture'
  | 'colorSpace'
  | 'dynamic'
  | 'levels'
  | 'phase'
  | 'audio'
  | 'alarm'
  | 'tally'
  | 'time'
  | 'date'
  | 'stationTimecode'
  | 'ptp'
  | 'ptpOffset'
  | 'ntpOffset'
  | 'cpu'
  | 'memory'
  | 'alarmsOn';

const PLANT: ValueKind[] = ['time', 'date', 'stationTimecode', 'ptp', 'ptpOffset', 'ntpOffset', 'cpu', 'memory', 'alarmsOn'];
export const isPlant = (k: ValueKind) => PLANT.includes(k);

/** Short names (mirrors ValueKind::label): [French, English]. */
const LABELS: Record<ValueKind, [string, string]> = {
  name: ['Source', 'Source'],
  umd: ['UMD', 'UMD'],
  format: ['Format', 'Format'],
  timecode: ['TC', 'TC'],
  momentary: ['M', 'M'],
  shortTerm: ['S', 'S'],
  integrated: ['I', 'I'],
  loudnessRange: ['LRA', 'LRA'],
  truePeak: ['TP', 'TP'],
  peakLeft: ['G', 'L'],
  peakRight: ['D', 'R'],
  codec: ['Codec', 'Codec'],
  picture: ['Image', 'Picture'],
  colorSpace: ['Colorimétrie', 'Colour space'],
  dynamic: ['Dynamique', 'Dynamic range'],
  levels: ['Niveaux', 'Levels'],
  phase: ['Phase', 'Phase'],
  audio: ['Son', 'Sound'],
  alarm: ['Alarme', 'Alarm'],
  tally: ['Tally', 'Tally'],
  time: ['Heure', 'Time'],
  date: ['Date', 'Date'],
  stationTimecode: ['TC station', 'House TC'],
  ptp: ['PTP', 'PTP'],
  ptpOffset: ['Écart PTP', 'PTP offset'],
  ntpOffset: ['Écart NTP', 'NTP offset'],
  cpu: ['CPU', 'CPU'],
  memory: ['Mémoire', 'Memory'],
  alarmsOn: ['Alarmes', 'Alarms'],
};

export interface ValueConfig {
  input?: string;
  kind?: ValueKind;
  label?: string;
  showLabel?: boolean;
  stack?: boolean;
  align?: 'left' | 'center' | 'right';
  warn?: number;
  crit?: number;
  color?: string;
  background?: string;
}
const D_VALUE = { input: '', kind: 'timecode' as ValueKind, label: '', showLabel: true, stack: false, align: 'center' as const, warn: 0, crit: 0, color: '#ffffff', background: '#0b0d12d9' };
type C = typeof D_VALUE;

const DASH = '—';
const GREEN = '#2bd96b';
const DIM = '#9aa3b2';
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
const padOf = (w: number, h: number) => Math.round(clamp(Math.min(w, h) * 0.04, 3, 14));
const p2 = (n: number) => String(n).padStart(2, '0');
const db = (v: number, unit: string) => (v <= -119 ? DASH : `${num1(v)} ${unit}`);

/** What the value is drawn from (mirrors value::Facts). */
export interface Facts {
  source: (SignalView & { name: string; format: string; live: boolean }) | null;
  system: SystemView;
  nowMs: number;
  tz: string;
  fps: number;
  lang: string;
}

/** "01/11/2026" (French), "2026-11-01" (mirrors date). */
const dateText = (ms: number, tz: string, fr: boolean) => {
  const f = zonedFields(ms, tz);
  return fr ? `${p2(f.day)}/${p2(f.month)}/${f.year}` : `${f.year}-${p2(f.month)}-${p2(f.day)}`;
};

/** The value, its number and its own colour (mirrors value). */
export function valueOf(c: C, f: Facts): [string, number | null, string | null] {
  const fr = f.lang.toLowerCase().startsWith('fr');
  const s = f.source;
  const v = s?.video;
  const loud = s?.loudness;
  const none: [string, null, null] = [DASH, null, null];
  const last = (key: string) => {
    const a = f.system.curves?.series[key] ?? [];
    for (let i = a.length - 1; i >= 0; i--) if (a[i] != null) return a[i] as number;
    return null;
  };
  switch (c.kind) {
    case 'name':
      return s ? [s.name, null, null] : none;
    case 'umd':
      return s ? [s.tally?.umd?.trim() ? s.tally.umd : s.name, null, null] : none;
    case 'format':
      return s?.format ? [s.format, null, null] : none;
    case 'timecode':
      return s?.timecode ? [s.timecode, null, null] : none;
    case 'momentary':
      return loud ? [db(loud[0], 'LUFS'), loud[0], null] : none;
    case 'shortTerm':
      return loud ? [db(loud[1], 'LUFS'), loud[1], null] : none;
    case 'integrated':
      return loud ? [db(loud[2], 'LUFS'), loud[2], null] : none;
    case 'loudnessRange':
      return loud ? [`${num1(loud[3])} LU`, loud[3], null] : none;
    case 'truePeak':
      return loud ? [db(loud[4], 'dBTP'), loud[4], null] : none;
    case 'peakLeft':
    case 'peakRight': {
      const l = s?.levels?.[c.kind === 'peakRight' ? 1 : 0];
      return l ? [db(l[0], 'dBFS'), l[0], null] : none;
    }
    case 'codec':
      return v?.codec ? [v.profile ? `${v.codec} · ${v.profile}` : v.codec, null, null] : none;
    case 'picture':
      return v?.size ? [[v.size.replace(/x/g, '×'), v.bits ? `${v.bits} bits` : '', v.chroma].filter(Boolean).join(' · '), null, null] : none;
    case 'colorSpace':
      if (!v || (!v.matrix && !v.primaries)) return none;
      return [!v.primaries ? named(MATRIX, v.matrix) : !v.matrix ? named(PRIMARIES, v.primaries) : `${named(MATRIX, v.matrix)} · ${named(PRIMARIES, v.primaries)}`, null, null];
    case 'dynamic': {
      if (!s?.live || !v) return none;
      const d = dynamicOf(v);
      return [d, null, dynamicColors(d)[0]];
    }
    case 'levels': {
      if (!v?.luma) return none;
      const [tx, col] = levelsText(v, fr);
      return [tx, v.illegal ?? null, col !== '#ffffff' ? AMBER : null];
    }
    case 'phase': {
      const x = s?.correlation;
      if (x == null) return none;
      const r = Math.round(clamp(x, -1, 1) * 100) / 100;
      return [r < 0 ? `−${(-r).toFixed(2)}` : `+${r.toFixed(2)}`, x, correlationColor(x)];
    }
    case 'audio':
      return v?.audio?.[0] ? [audioLine(v.audio[0]), null, null] : none;
    case 'alarm': {
      if (!s) return none;
      const a = s.alarms?.[0];
      return a ? [alarmText(a.kind, f.lang), null, a.severity === 'critical' ? RED : AMBER] : ['OK', null, GREEN];
    }
    case 'tally':
      if (!s) return none;
      if (s.tally?.program) return [fr ? 'ANTENNE' : 'ON AIR', null, RED];
      if (s.tally?.preview) return [fr ? 'PRÉPARATION' : 'PREVIEW', null, GREEN];
      if (s.tally?.aux) return ['AUX', null, '#f2c12e'];
      return [DASH, null, DIM];
    case 'time':
      return [clockText(f.nowMs, f.tz), null, null];
    case 'date':
      return [dateText(f.nowMs, f.tz, fr), null, null];
    case 'stationTimecode':
      return [stationTimecode(f.nowMs, f.tz, f.fps), null, null];
    case 'ptp': {
      const p = f.system.ptp;
      if (!p.running) return [fr ? 'Absent' : 'Not running', null, DIM];
      if (p.locked) return [`${fr ? 'Verrouillé' : 'Locked'} · ${fr ? 'domaine' : 'domain'} ${p.domain}`, null, GREEN];
      return [fr ? 'Non verrouillé' : 'Unlocked', null, RED];
    }
    case 'ptpOffset': {
      const o = f.system.ptp.running ? f.system.ptp.offsetNs : null;
      return o != null ? [valueText('ptpOffset', o), o, null] : none;
    }
    case 'ntpOffset': {
      const o = f.system.ntp.offsetMs;
      return o != null ? [valueText('ntpOffset', o), o, null] : none;
    }
    case 'cpu': {
      const x = last('cpu');
      return x != null ? [valueText('cpu', x), x, null] : none;
    }
    case 'memory': {
      const x = last('memory');
      return x != null ? [valueText('memory', x), x, null] : none;
    }
    case 'alarmsOn': {
      const on = f.system.journal.filter((e) => e.ended == null);
      return [String(on.length), on.length, on.some((e) => e.severity === 'critical') ? RED : on.length ? AMBER : GREEN];
    }
  }
}

/** Thresholds first, then its own colour, then the set one (mirrors color_of). */
function colorOf(c: C, n: number | null, own: string | null): string {
  if (n != null) {
    const size = c.kind === 'ptpOffset' || c.kind === 'ntpOffset' ? Math.abs(n) : n;
    if (c.crit !== 0 && size >= c.crit) return RED;
    if (c.warn !== 0 && size >= c.warn) return AMBER;
  }
  return own ?? c.color;
}

/** The Value widget (mirrors value_items). */
export function valueItems(cfg: ValueConfig, w: number, h: number, f: Facts): Item[] {
  const c = withDefaults(D_VALUE, cfg) as C;
  const fr = f.lang.toLowerCase().startsWith('fr');
  const pad = padOf(w, h);
  const out: Item[] = [rect([0, 0, w, h], pad * 0.6, c.background)];
  const [v, n, own] = valueOf(c, f);
  const col = colorOf(c, n, own);
  const label = c.label.trim() ? c.label : LABELS[c.kind][fr ? 0 : 1];
  const [iw, ih] = [w - 2 * pad, h - 2 * pad];
  if (c.showLabel && c.stack) {
    const lh = ih * 0.32;
    out.push(text([pad, pad, iw, lh], label, lh * 0.8, 600, false, DIM, c.align));
    out.push(text([pad, pad + lh, iw, ih - lh], v, (ih - lh) * 0.72, 700, true, col, c.align));
  } else if (c.showLabel) {
    const size = Math.min(ih * 0.62, iw * 0.2);
    const lw = Math.min([...label].length * size * 0.62 + size * 0.6, iw * 0.45);
    out.push(text([pad, pad, lw, ih], label, size * 0.8, 600, false, DIM, 'left'));
    out.push(text([pad + lw, pad, iw - lw, ih], v, size, 700, true, col, 'right'));
  } else out.push(text([pad, pad, iw, ih], v, ih * 0.72, 700, true, col, c.align));
  return out;
}

function ValueView({ cfg, w, h, lang, nowMs, tz, fps }: { cfg: ValueConfig; w: number; h: number; lang: string; nowMs: number; tz: string; fps: number }) {
  const c = withDefaults(D_VALUE, cfg) as C;
  const id = isPlant(c.kind) ? '' : c.input.trim();
  const signal = useSignal(id);
  const system = useSystem();
  const inputs = useInputs();
  // The station timecode and the time move by themselves.
  useEveryFrame(c.kind === 'stationTimecode');
  const info = inputs.find((i) => i.id === id);
  const source = id && info ? { ...signal, name: info.name, format: info.live ? formatName(info) : '', live: info.live } : null;
  return <Items items={valueItems(cfg, w, h, { source, system, nowMs, tz, fps, lang })} look={(cfg as { panel?: Panel }).panel} />;
}

/** The values, grouped as in the inspector. */
const GROUPS: [string, ValueKind[]][] = [
  ['Input', ['name', 'umd', 'format', 'timecode', 'tally', 'alarm']],
  ['Loudness and audio', ['momentary', 'shortTerm', 'integrated', 'loudnessRange', 'truePeak', 'peakLeft', 'peakRight', 'phase', 'audio']],
  ['Picture', ['codec', 'picture', 'colorSpace', 'dynamic', 'levels']],
  ['Plant', ['time', 'date', 'stationTimecode', 'ptp', 'ptpOffset', 'ntpOffset', 'cpu', 'memory', 'alarmsOn']],
];
/** Names in the inspector (translated with t()). */
const NAMES: Record<ValueKind, string> = {
  name: 'Name',
  umd: 'UMD',
  format: 'Format',
  timecode: 'Timecode',
  tally: 'Tally',
  alarm: 'Worst fault',
  momentary: 'Momentary loudness (M)',
  shortTerm: 'Short-term loudness (S)',
  integrated: 'Integrated loudness (I)',
  loudnessRange: 'Loudness range (LRA)',
  truePeak: 'True peak',
  peakLeft: 'Peak, left',
  peakRight: 'Peak, right',
  phase: 'Phase correlation',
  audio: 'Sound track',
  codec: 'Codec',
  picture: 'Size, depth, sampling',
  colorSpace: 'Colour space',
  dynamic: 'SDR / HDR',
  levels: 'Levels (EBU R103)',
  time: 'Time of day',
  date: 'Date',
  stationTimecode: 'Station timecode',
  ptp: 'PTP state',
  ptpOffset: 'PTP offset',
  ntpOffset: 'NTP offset',
  cpu: 'Processors',
  memory: 'Memory',
  alarmsOn: 'Faults on now',
};
const NUMERIC: ValueKind[] = ['momentary', 'shortTerm', 'integrated', 'loudnessRange', 'truePeak', 'peakLeft', 'peakRight', 'levels', 'ptpOffset', 'ntpOffset', 'cpu', 'memory', 'alarmsOn'];

export const valueWidget = defineWidget({
  id: 'value',
  type: 'value',
  label: 'Value',
  group: 'Multiview',
  icon: Variable,
  create: () => ({ props: { width: 420, height: 90 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, lang, time, fps }) => <ValueView cfg={layer as ValueConfig} w={w} h={h} lang={lang ?? 'fr'} nowMs={time.nowMs} tz={time.defaultTz} fps={time.fps || fps} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_VALUE, layer as ValueConfig) as C;
    const inputs = useInputs();
    return (
      <>
        <Section id="value" title={t('Value')}>
          <Select
            label={t('Shows')}
            value={c.kind}
            onChange={(v) => set('kind', v)}
            options={GROUPS.flatMap(([g, list]) => list.map((k) => [k, `${t(g)} · ${t(NAMES[k])}`] as [ValueKind, string]))}
          />
          {!isPlant(c.kind) && <Select label={t('Input')} value={c.input} onChange={(v) => set('input', v)} options={[['', t('None')] as [string, string], ...inputs.map((i) => [i.id, i.name] as [string, string])]} />}
        </Section>
        <Section id="value-look" title={t('Layout')}>
          <Flags
            label={t('Label')}
            items={[
              { icon: <Tag size={14} />, title: t('Show the label'), value: c.showLabel, onChange: (v) => set('showLabel', v) },
              c.showLabel && { icon: c.stack ? <Rows2 size={14} /> : <Columns2 size={14} />, title: c.stack ? t('Label over the value') : t('Label beside the value'), value: c.stack, onChange: (v) => set('stack', v) },
            ]}
          />
          {c.showLabel && <Text label={t('Words')} value={c.label} placeholder={LABELS[c.kind][0]} onChange={(v) => set('label', v)} />}
          {(!c.showLabel || c.stack) && (
            <Seg
              label={t('Align')}
              value={c.align}
              onChange={(v) => set('align', v)}
              options={[
                ['left', <AlignLeft size={14} />, t('Left')],
                ['center', <AlignCenter size={14} />, t('Centre')],
                ['right', <AlignRight size={14} />, t('Right')],
              ]}
            />
          )}
          <ColorPick label={t('Value colour')} value={c.color} onChange={(v) => set('color', v)} />
        </Section>
        {NUMERIC.includes(c.kind) && (
          <Section id="value-thresholds" title={t('Thresholds')}>
            <Num label={t('Amber from')} value={c.warn} slider={false} onChange={(v) => set('warn', v)} />
            <Num label={t('Red from')} value={c.crit} slider={false} onChange={(v) => set('crit', v)} />
          </Section>
        )}
      </>
    );
  },
});

export default valueWidget;

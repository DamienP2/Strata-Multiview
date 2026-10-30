// Clock layers, shared by two widgets: the Clock (« Horloge », ./clock.tsx: time of day and
// date) and the Time (« Temps », ./time.tsx: stopwatch and countdown). Both are `clock`
// layers with the same settings; `timerMode` says which widget a layer is (widgetOf).
// Here: the drawing, the inspector rows both use, the control centre and the warnings.
// Same picture as crates/cg-render/src/wid/timer.rs; logic mirrored in ./timer-fmt.ts
// (crates/cg-core/src/wid/timer.rs); actions in crates/cg-core/src/wid/timer.rs.
import { Dropdown } from '../ui/Dropdown';
import { useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { Flag, Minus, Pause, Play, Plus, RotateCcw, RotateCw, type LucideIcon } from 'lucide-react';
import { col, cssColor, num, type Resolved } from '../engine/evaluate';
import { layerAgeFrames, textWidth } from '../engine/widgets';
import type { Data, Layer, FontStyle, PlayoutItem } from '../engine/types';
import { FontControls, GroupRow } from '../designer/Inspector';
import { useApp } from '../store';
import { confirmDialog } from '../ui/dialogs';
import { Bound, ColorPick, Content, defineWidget, getPath, hex, Num, Row, Seg, Text, Toggle, useBound, useCentreMode, useFrame, useKeys, type InspectorProps, type Warning } from './kit';
import { Centre, SetInput, StartRow, StartStatus, type Line2Item } from './centre';
import * as T from './timer-fmt';
import { t, tc } from '../i18n';

export type Config = T.Config;
export type State = T.State;
export type L = T.ClockLayer;

export const ZONES = ['Europe/Paris', 'Europe/London', 'Europe/Berlin', 'Europe/Madrid', 'Europe/Rome', 'America/New_York', 'America/Chicago', 'America/Los_Angeles', 'America/Sao_Paulo', 'Asia/Tokyo', 'Asia/Shanghai', 'Asia/Dubai', 'Asia/Kolkata', 'Australia/Sydney', 'UTC'];
export const PRESETS: Record<T.TimerMode, [string, T.DateCase?][]> = {
  clock: [['HH:mm:ss'], ['HH:mm'], ['H[h]mm'], ['HH:mm:ss:FF'], ['dddd HH:mm']],
  date: [['dddd D MMMM YYYY', 'first'], ['DD/MM/YYYY'], ['D MMM', 'upper'], ['ddd D MMM', 'upper'], ['YYYY-MM-DD']],
  stopwatch: [['mm:ss'], ['HH:mm:ss'], ['mm:ss.S'], ['mm:ss.SS'], ['HH:mm:ss:FF']],
  countdown: [['mm:ss'], ['HH:mm:ss'], ['mm:ss.S'], ['DD[d] HH:mm:ss'], ['[J-]D']],
};
export const LEGACY_MODE: Record<T.TimerMode, string> = { clock: 'time', date: 'time', stopwatch: 'elapsed', countdown: 'timer' };
export const STEPS: Partial<Record<T.TimerMode, T.Step[]>> = {
  countdown: [
    { on: true, at: 60, color: '#f59e0b', bg: '', blink: false },
    { on: true, at: 10, color: '#ff3b5c', bg: '', blink: true },
  ],
  stopwatch: [{ on: true, at: 2700, color: '#facc15', bg: '', blink: false }],
};
const LH = 1.2;

// ------------------------------------------------------------- layout --

interface Piece {
  text: string;
  w: number;
  cell: boolean;
  alpha: number;
}
const cellW = (fam: string, wt: number, size: number) => Math.max(...'0123456789'.split('').map((d) => textWidth(d, fam, wt, size, 0)));

function pieces(segs: T.Seg[], sep: string, fam: string, wt: number, size: number, ls: number, tabular: boolean, sepAlpha: number, upper: boolean): Piece[] {
  const out: Piece[] = [];
  const push = (text: string, cell: boolean, alpha: number) => {
    if (!text) return;
    const last = out[out.length - 1];
    if (last && !cell && !last.cell && last.alpha === alpha) return void (last.text += text);
    out.push({ text, w: 0, cell, alpha });
  };
  for (const g of segs) {
    let t = g.k === 'sep' ? sep : g.t;
    if (upper) t = t.toUpperCase();
    const alpha = g.k === 'sep' ? sepAlpha : 1;
    if (tabular) {
      let run = '';
      for (const ch of t) {
        if (/[0-9]/.test(ch)) {
          push(run, false, alpha);
          run = '';
          push(ch, true, alpha);
        } else run += ch;
      }
      push(run, false, alpha);
    } else push(t, false, alpha);
  }
  const cw = tabular ? cellW(fam, wt, size) : 0;
  for (const p of out) p.w = p.cell ? cw : textWidth(p.text, fam, wt, size, ls);
  return out;
}
const width = (ps: Piece[]) => ps.reduce((a, p) => a + p.w, 0);

interface Plate {
  disp: T.Disp;
  label: string;
  wi: number;
  wp: number;
  ws: number;
}

const fontCss = (fam: string) => `"${fam}", Inter, system-ui, sans-serif`;

function TimerView({ layer, r, w, h, data, time, state, fps, designer, lang }: { layer: L; r: Resolved; w: number; h: number; data: Data; time: { nowMs: number; ageSecs: number; defaultTz: string }; state: unknown; fps: number; designer?: boolean; lang?: string }) {
  const e = T.eff(layer, data, lang);
  const c = e.c;
  const now = time.nowMs;
  const at: T.At = { now, ageMs: time.ageSecs * 1000, fps, defaultTz: time.defaultTz };
  const size = Math.max(1, num(r.fontSize));
  const fam = layer.fontFamily;
  const wt = layer.fontWeight;
  const ls = num(r.letterSpacing);
  const upper = !!layer.uppercase;
  const [prefix, suffix] = T.affixes(c, data);
  const sepAlpha = c.sepBlink && ((now % 1000) + 1000) % 1000 >= 500 ? 0.12 : 1;
  const st = state as State | undefined;
  const zones: [T.ZoneSel | undefined, string][] =
    !T.timed(e) && c.cities && c.cityList.length
      ? c.cityList.map((ct) => {
          const label = ct.label?.trim() ? ct.label : (ct.zone ?? '');
          return [T.zoneNamed(ct.zone), c.affix.upper ? label.toUpperCase() : label];
        })
      : [[undefined, '']];
  if (c.face === 'analog') {
    const fill = cssColor(col(r.fill));
    return <AnalogFaces e={e} st={st} at={at} zones={zones} w={w} h={h} align={layer.align} valign={layer.verticalAlign} ink={c.analog.hands.trim() ? hex(c.analog.hands) : fill} labelCol={c.affix.color.trim() ? hex(c.affix.color) : fill} fam={fam} wt={wt} labelWt={c.affix.weight} />;
  }
  const afam = fam;
  const asz = (k: number) => (size * k * c.affix.size) / 100;
  const aw = (s: string) => (s ? textWidth(s, afam, c.affix.weight, asz(1), 0) : 0);
  const plates: Plate[] = zones.map(([z, label]) => {
    const disp = T.display(e, st, at, z);
    const wv = width(pieces(disp.segs, c.sep, fam, wt, size, ls, c.tabular, 1, upper));
    const wi = c.reserve ? Math.max(wv, ...T.widest(e, disp, c.sep, fps).map((s) => width(pieces([{ k: 'lit', t: s }], c.sep, fam, wt, size, ls, c.tabular, 1, upper)))) : wv;
    return { disp, label, wi, wp: aw(prefix), ws: aw(suffix) };
  });
  if (plates.every((p) => p.disp.hidden)) return null;
  const pad = (k: number): [number, number, number] => {
    const p = c.plate;
    if (!p.on) return [0, 0, 0];
    const py = Math.max(p.padY * k, size * k * 0.08);
    return [(p.padX + (p.accent ? 12 : 0)) * k, p.padX * k, py];
  };
  const gap = (k: number) => size * k * 0.14;
  const contentW = (pl: Plate, k: number) => pl.wi * k + (pl.wp > 0 ? pl.wp * k + gap(k) : 0) + (pl.ws > 0 ? pl.ws * k + gap(k) : 0);
  const plateW = (pl: Plate, k: number) => {
    const [l, rr] = pad(k);
    return l + contentW(pl, k) + rr;
  };
  const plateH = (k: number) => size * k * LH + 2 * pad(k)[2];
  const labelH = (pl: Plate, k: number) => (pl.label ? asz(k) * LH + size * k * 0.12 : 0);
  const n = plates.length;
  const total1 = plates.reduce((a, p) => a + plateW(p, 1), 0) + (n - 1) * 36;
  const k = c.fit && total1 > 0 ? Math.min(1, Math.max(0.01, (w - 8) / total1)) : 1;
  const total = plates.reduce((a, p) => a + plateW(p, k), 0) + (n - 1) * 36 * k;
  const labH = Math.max(...plates.map((p) => labelH(p, k)));
  const groupH = labH + plateH(k);
  const x0 = layer.align === 'left' ? 0 : layer.align === 'right' ? w - total : (w - total) / 2;
  const y0 = layer.verticalAlign === 'top' ? 0 : layer.verticalAlign === 'bottom' ? h - groupH : (h - groupH) / 2;
  const fill = cssColor(col(r.fill));
  const affixCol = c.affix.color.trim() ? hex(c.affix.color) : fill;
  const strokeW = num(r.strokeWidth);
  const strokeCss = strokeW > 0 ? `${strokeW}px ${cssColor(col(r.stroke))}` : undefined;
  const els: ReactNode[] = [];
  const textBox = (key: string, text: string, x: number, cy: number, sz: number, style: CSSProperties) => (
    <div key={key} style={{ position: 'absolute', left: x, top: cy - (sz * LH) / 2, height: sz * LH, lineHeight: LH, fontSize: sz, whiteSpace: 'pre', fontKerning: 'normal', ...style }}>
      {text}
    </div>
  );
  let x = x0;
  plates.forEach((pl, i) => {
    const pw = plateW(pl, k);
    const ph = plateH(k);
    const y = y0 + labH;
    const cy = y + ph / 2;
    const sz = size * k;
    const d = pl.disp;
    if (!d.hidden) {
      const bg = d.bg ? hex(d.bg) : c.plate.on ? hex(c.plate.color) : null;
      if (bg) {
        const px = c.plate.on ? 0 : c.plate.padX * k;
        const pyy = c.plate.on ? 0 : Math.max(c.plate.padY * k, sz * 0.08);
        const rad = c.plate.radius * k;
        els.push(
          <div key={`bg${i}`} style={{ position: 'absolute', left: x - px, top: y - pyy, width: pw + 2 * px, height: ph + 2 * pyy, borderRadius: rad, background: bg, overflow: 'hidden', boxShadow: c.plate.shadow && c.plate.on ? `0 ${18 * k}px ${50 * k}px rgba(0,0,0,.45)` : undefined }}>
            {c.plate.on && c.plate.accent && <i style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: Math.max(8, sz * 0.08), background: hex(c.plate.accentColor) }} />}
          </div>,
        );
      }
      const blinkOff = d.blink && ((now % 1000) + 1000) % 1000 >= 600;
      const valAlpha = blinkOff ? 0.15 : 1;
      const textCol = d.color ? hex(d.color) : fill;
      let ix = x + pad(k)[0];
      const affix = (key: string, s: string, ax: number) => textBox(key, s, ax, cy, asz(k), { color: affixCol, fontFamily: fontCss(afam), fontWeight: c.affix.weight });
      if (pl.wp > 0) {
        els.push(affix(`p${i}`, prefix, ix));
        ix += pl.wp * k + gap(k);
      }
      const ps = pieces(d.segs, c.sep, fam, wt, sz, ls * k, c.tabular, sepAlpha, upper);
      const wv = width(ps);
      const wi = pl.wi * k;
      const off = layer.align === 'left' ? 0 : layer.align === 'right' ? wi - wv : (wi - wv) / 2;
      if (designer && c.reserve && c.guide)
        els.push(<i key={`g${i}`} title={t('Reserved width')} style={{ position: 'absolute', left: ix - 6, top: cy - (sz * LH) / 2 - 4, width: wi + 12, height: sz * LH + 8, border: '2px dashed rgba(255,255,255,.22)', borderRadius: 8, boxSizing: 'border-box', pointerEvents: 'none' }} />);
      let vx = ix + off;
      ps.forEach((p, j) => {
        const st: CSSProperties = { color: textCol, opacity: p.alpha * valAlpha, fontFamily: fontCss(fam), fontWeight: wt, letterSpacing: p.cell ? 0 : ls * k, WebkitTextStroke: strokeCss };
        if (p.cell) Object.assign(st, { width: p.w, textAlign: 'center' });
        els.push(textBox(`v${i}-${j}`, p.text, vx, cy, sz, st));
        vx += p.w;
      });
      ix += wi;
      if (pl.ws > 0) els.push(affix(`s${i}`, suffix, ix + gap(k)));
      if (pl.label) els.push(textBox(`l${i}`, pl.label, x, y - labelH(pl, k) + (asz(k) * LH) / 2, asz(k), { width: pw, textAlign: 'center', color: affixCol, fontFamily: fontCss(afam), fontWeight: c.affix.weight }));
      if (d.split) {
        const bs = Math.max(18, sz * 0.16);
        const bw = textWidth('SPLIT', 'Inter', 900, bs, 0) + 24;
        const bh = bs * LH + 6;
        els.push(
          <div key={`sp${i}`} style={{ position: 'absolute', left: x + pw - 18 - bw, top: y - sz * 0.22, width: bw, height: bh, lineHeight: `${bh}px`, borderRadius: 6, background: '#facc15', color: '#1a1200', font: `900 ${bs}px Inter, system-ui, sans-serif`, textAlign: 'center' }}>
            SPLIT
          </div>,
        );
      }
    }
    x += pw + 36 * k;
  });
  const b = plates[0] && T.banner(e, plates[0].disp, now);
  if (b) {
    const [sc, alpha] = b;
    const bs = 64 * k;
    const bw = textWidth(c.bannerText, 'Inter', 900, bs, 0) + 80 * k;
    const bh = bs * LH + 28 * k;
    els.push(
      <div
        key="banner"
        style={{
          position: 'absolute',
          left: x0 + total / 2 - bw / 2,
          top: y0 + groupH + 22 * k,
          width: bw,
          height: bh,
          lineHeight: `${bh}px`,
          textAlign: 'center',
          borderRadius: 14 * k,
          background: hex(c.bannerColor),
          color: '#fff',
          font: `900 ${bs}px Inter, system-ui, sans-serif`,
          whiteSpace: 'pre',
          opacity: alpha,
          transform: `scale(${sc})`,
        }}
      >
        {c.bannerText}
      </div>,
    );
  }
  return <>{els}</>;
}

/** Analog faces side by side (one per city), labels under them. Same geometry as draw_analog in cg-render. */
function AnalogFaces({ e, st, at, zones, w, h, align, valign, ink, labelCol, fam, wt, labelWt }: { e: T.Eff; st: State | undefined; at: T.At; zones: [T.ZoneSel | undefined, string][]; w: number; h: number; align?: string; valign?: string; ink: string; labelCol: string; fam: string; wt: number; labelWt: number }) {
  const a = e.c.analog;
  const n = Math.max(1, zones.length);
  const lab = zones.some(([, l]) => l) ? 0.16 : 0;
  const diam = Math.max(0, Math.min(h / (1 + lab), w / (n + (n - 1) * 0.15)));
  if (diam < 1) return null;
  const gap = diam * 0.15;
  const total = n * diam + (n - 1) * gap;
  const x0 = align === 'left' ? 0 : align === 'right' ? w - total : (w - total) / 2;
  const gh = diam * (1 + lab);
  const y0 = valign === 'top' ? 0 : valign === 'bottom' ? h - gh : (h - gh) / 2;
  const accent = hex(a.accent);
  // Marks: count, a long one every, long from, short from, widths.
  const marks: [number, number, number, number, number, number] | null =
    a.ticks === 'hours' ? [12, 3, 0.78, 0.84, Math.max(1.5, diam * 0.02), Math.max(1.5, diam * 0.02)] : a.ticks === 'minutes' ? [60, 5, 0.8, 0.88, Math.max(1.5, diam * 0.022), Math.max(1, diam * 0.008)] : null;
  const faces: ReactNode[] = [];
  const texts: ReactNode[] = [];
  zones.forEach(([zone, label], i) => {
    const cx = x0 + i * (diam + gap) + diam / 2;
    const cy = y0 + diam / 2;
    const r = diam / 2;
    const seg = (deg: number, r0: number, r1: number) => {
      const t = (deg * Math.PI) / 180;
      return { x1: cx + Math.sin(t) * r0, y1: cy - Math.cos(t) * r0, x2: cx + Math.sin(t) * r1, y2: cy - Math.cos(t) * r1 };
    };
    const parts: ReactNode[] = [];
    if (a.face.trim()) parts.push(<circle key="f" cx={cx} cy={cy} r={r} fill={hex(a.face)} />);
    if (a.rim.trim()) parts.push(<circle key="r" cx={cx} cy={cy} r={r - diam * 0.015} fill="none" stroke={hex(a.rim)} strokeWidth={diam * 0.03} />);
    if (marks) {
      const [count, every, major, minor, wmaj, wmin] = marks;
      for (let k = 0; k < count; k++) {
        const big = k % every === 0;
        parts.push(<line key={`t${k}`} {...seg((k * 360) / count, r * (big ? major : minor), r * 0.92)} stroke={ink} strokeWidth={big ? wmaj : wmin} />);
      }
    }
    const hd = T.hands(e, st, at, zone);
    if (hd) {
      const [ha, ma, sa] = hd;
      parts.push(<line key="h" {...seg(ha, -r * 0.08, r * 0.5)} stroke={ink} strokeWidth={diam * 0.06} />);
      parts.push(<line key="m" {...seg(ma, -r * 0.08, r * 0.74)} stroke={ink} strokeWidth={diam * 0.04} />);
      if (a.seconds) parts.push(<line key="s" {...seg(sa, -r * 0.15, r * 0.86)} stroke={accent} strokeWidth={Math.max(1, diam * 0.012)} />);
      parts.push(<circle key="d" cx={cx} cy={cy} r={diam * 0.035} fill={a.seconds ? accent : ink} />);
    }
    faces.push(<g key={i}>{parts}</g>);
    if (a.numbers) {
      const rr = r * (a.ticks === 'none' ? 0.8 : 0.64);
      const sz = diam * 0.11;
      for (let k = 1; k <= 12; k++) {
        const t = (k * 30 * Math.PI) / 180;
        const nx = cx + Math.sin(t) * rr;
        const ny = cy - Math.cos(t) * rr;
        texts.push(
          <div key={`n${i}-${k}`} style={{ position: 'absolute', left: nx - sz, top: ny - (sz * LH) / 2, width: sz * 2, height: sz * LH, lineHeight: LH, fontSize: sz, textAlign: 'center', color: ink, fontFamily: fontCss(fam), fontWeight: wt, whiteSpace: 'pre' }}>
            {k}
          </div>,
        );
      }
    }
    if (label) {
      const sz = diam * 0.11;
      texts.push(
        <div key={`l${i}`} style={{ position: 'absolute', left: cx - diam, top: y0 + diam + diam * 0.02 + (diam * lab - sz * LH) / 2, width: diam * 2, height: sz * LH, lineHeight: LH, fontSize: sz, textAlign: 'center', color: labelCol, fontFamily: fontCss(fam), fontWeight: labelWt, whiteSpace: 'pre' }}>
          {label}
        </div>,
      );
    }
  });
  return (
    <>
      <svg style={{ position: 'absolute', inset: 0, overflow: 'visible' }} width={w} height={h} strokeLinecap="round">
        {faces}
      </svg>
      {texts}
    </>
  );
}

// ------------------------------------------------------------ inspector --

export type Edit = (fn: (l: L) => void, key?: string) => void;
export type SetPath = (path: string, v: unknown, coalesce?: boolean) => void;
/** What the inspector of each widget gets. */
export interface InspP {
  layer: L;
  data: Data;
  edit: Edit;
  set: SetPath;
}

/** The project's language: a clock that follows it shows its day and month names in it. */
export const useProjectLang = () => useApp((s) => s.project?.language) || 'fr';

export function DurField({ label, value, onChange, min = 0, expose }: { label: string; value: number; onChange: (ms: number) => void; min?: number; expose?: string }) {
  const toast = useApp((s) => s.toast);
  const [txt, setTxt] = useState<string | null>(null);
  const commit = () => {
    if (txt == null) return;
    const p = T.parseDur(txt);
    if ('err' in p || p.ms < min) toast(t('Refused: {error}', { error: 'err' in p ? p.err : t('value too small') }), 'error');
    else onChange(p.ms);
    setTxt(null);
  };
  return (
    <Row label={label} expose={expose} exposeValue={value}>
      <input style={{ fontFamily: 'var(--mono, monospace)' }} value={txt ?? T.plainDur(value)} placeholder="mm:ss" onChange={(ev) => setTxt(ev.target.value)} onBlur={commit} onKeyDown={(ev) => (ev.stopPropagation(), ev.key === 'Enter' && commit())} />
    </Row>
  );
}

const sampleText = (layer: L, data: Data, fmt: string, lang: string) => {
  const e = T.eff({ ...layer, format: fmt }, data, lang);
  const d = T.display(e, undefined, { now: Date.now(), ageMs: 0, fps: 25, defaultTz: '' }, undefined);
  return T.segText(d.segs, e.c.sep);
};

/** The mode, among the two of the widget (the format, zeros and steps follow it unless the designer's own). */
export function ModeSeg({ mode, modes, edit }: { mode: T.TimerMode; modes: [T.TimerMode, string][]; edit: Edit }) {
  return (
    <Seg
      expose="timerMode"
      exposeValue={mode}
      label={t('Mode')}
      value={mode}
      options={modes.map(([v, l]) => [v, t(l)] as [T.TimerMode, string])}
      onChange={(v) =>
        edit((l) => {
          const x = l as L & Config;
          const was = x.timerMode ?? mode;
          x.timerMode = v;
          x.mode = LEGACY_MODE[v] as L['mode'];
          // A format, zeros and steps of the designer's own stay (only the presets of the old mode change).
          const preset = !x.format?.trim() || PRESETS[was].some((p) => p[0] === x.format) || !T.hasToken(x.format);
          if (preset) {
            x.format = PRESETS[v][0][0];
            if (PRESETS[v][0][1]) x.case = PRESETS[v][0][1];
            x.zeros = 'pad';
          }
          const sameSteps = (a?: T.Step[], b?: T.Step[]) => JSON.stringify(a ?? []) === JSON.stringify(b ?? []);
          if (!x.steps?.length || sameSteps(x.steps, STEPS[was])) x.steps = STEPS[v] ? structuredClone(STEPS[v]) : [];
          if (v !== 'clock') x.cities = false;
        })
      }
    />
  );
}

/** The format presets of the mode, each with what it shows now. */
export function PresetRow({ layer, data, mode, edit }: { layer: L; data: Data; mode: T.TimerMode; edit: Edit }) {
  const lang = useProjectLang();
  const list = PRESETS[mode];
  return (
    <Row expose="format" exposeValue={layer.format} label={t('Preset')}>
      <Dropdown
        value={list.some((p) => p[0] === layer.format) ? layer.format : '__custom'}
        options={[...list.map(([f]) => ({ value: f, label: `${f} → ${sampleText(layer, data, f, lang)}` })), { value: '__custom', label: t('Custom') }]}
        onChange={(v) => {
          const p = list.find((x) => x[0] === v);
          if (!p) return;
          edit((l) => {
            const x = l as L & Config;
            x.format = p[0];
            if (p[1]) x.case = p[1];
            if (mode === 'stopwatch') x.swRound = x.swRound ?? 'trunc';
          });
        }}
      />
    </Row>
  );
}

/** The format typed, the prefix and the suffix. */
export function TextRows() {
  return (
    <>
      <Text label={t('Format')} path="format" placeholder="HH:mm:ss" />
      <Content label={t('Prefix')} path="prefix" value="" />
      <Content label={t('Suffix')} path="suffix" value="" />
    </>
  );
}

/** Zeros, separator and its blink (durations and times of day). */
export function DigitRows({ e }: { e: T.Eff }) {
  return (
    <>
      <Seg
        label={t('Zeros')}
        path="zeros"
        value="pad"
        options={[
          ['pad', '00:01:05'],
          ['trim', '1:05'],
        ]}
      />
      {T.timed(e) && <Toggle label={t('Auto hours')} path="hoursAuto" value={false} />}
      <Seg
        label={t('Separator')}
        path="sep"
        value=":"
        options={[
          [':', ':'],
          ['h', 'h'],
          ["'", "'"],
          [' ', t('space')],
          ['.', '.'],
        ]}
      />
      <Toggle label={t('Blink')} path="sepBlink" value={false} />
    </>
  );
}

/** Digits or an analog face (offered to a timed layer only when it has a face already, to go back to digits). */
export function LookSeg({ c }: { c: T.Cfg }) {
  return (
    <Seg
      label={t('Look')}
      path="face"
      value={c.face}
      options={[
        ['digital', t('Digits')],
        ['analog', t('Analog clock')],
      ]}
    />
  );
}

/** The look of an analog face. */
export function AnalogRows({ c }: { c: T.Cfg }) {
  return (
    <>
      <ColorPick label={t('Face')} path="analog.face" value={T.DEF.analog.face} none={t('None')} first={T.DEF.analog.face} />
      <ColorPick label={t('Rim')} path="analog.rim" value="" none={t('None')} />
      <ColorPick label={t('Hands')} path="analog.hands" value="" none={t('Text colour')} />
      <Toggle label={t('Second hand')} path="analog.seconds" value={true} />
      {c.analog.seconds && (
        <>
          <ColorPick label={t('Second hand colour')} path="analog.accent" value={T.DEF.analog.accent} />
          <Toggle label={t('Sweep')} path="analog.smooth" value={false} text={t('The second hand moves smoothly')} />
        </>
      )}
      <Seg
        label={t('Marks')}
        path="analog.ticks"
        value={c.analog.ticks}
        options={[
          ['minutes', t('Minutes')],
          ['hours', t('Hours')],
          ['none', t('None')],
        ]}
      />
      <Toggle label={tc('clock', 'Numbers')} path="analog.numbers" value={false} text={t('1 to 12 around the face')} />
    </>
  );
}

/** Font and size (no size for a face: it fills its box). */
export function FontRows({ layer, c, set }: { layer: L; c: T.Cfg; set: SetPath }) {
  return (
    <>
      <FontControls layer={layer as Layer & FontStyle} set={(k, v) => set(k, v)} />
      {c.face !== 'analog' && <GroupRow layer={layer} spec={{ label: t('Size'), props: ['fontSize'], min: 1, suffix: 'px' }} />}
    </>
  );
}

/** The look of the prefix, suffix and city names. */
export function AffixRows() {
  return (
    <>
      <Num label={t('Affix size')} path="affix.size" value={30} min={15} max={100} unit="%" />
      <ColorPick label={t('Affix colour')} path="affix.color" value="#a594ff" />
      <Seg
        label={t('Affix weight')}
        path="affix.weight"
        value={800}
        options={[
          [500, t('Medium')],
          [700, t('Bold')],
          [800, t('Extra')],
          [900, t('Black')],
        ]}
      />
      <Toggle label={t('Affix caps')} path="affix.upper" value={true} />
    </>
  );
}

/** The plate behind the value. */
export function PlateRows({ c }: { c: T.Cfg }) {
  return (
    <>
      <Toggle label={t('Plate')} path="plate.on" value={false} />
      {c.plate.on && (
        <>
          <ColorPick label={t('Colour')} path="plate.color" value={T.DEF.plate.color} />
          <Num label={t('Radius')} path="plate.radius" value={22} min={0} max={80} unit="px" />
          <Num label={t('Padding')} path="plate.padX" value={46} min={0} max={120} unit="px" />
          <Num label={t('Padding V')} path="plate.padY" value={14} min={0} max={80} unit="px" />
          <Toggle label={t('Accent')} path="plate.accent" value={true} />
          {c.plate.accent && <ColorPick label={t('Accent colour')} path="plate.accentColor" value="#ff3b5c" />}
          <Toggle label={t('Shadow')} path="plate.shadow" value={true} />
        </>
      )}
    </>
  );
}

/** Digits that do not move (the widest value's width), or shrunk to the box. */
export function WidthRows({ c }: { c: T.Cfg }) {
  return (
    <>
      <Toggle label={t('Reserve width')} path="reserve" value={T.DEF.reserve} />
      {c.reserve && <Toggle label={t('Guide')} path="guide" value={true} />}
      <Toggle label={t('Shrink to fit')} path="fit" value={false} />
    </>
  );
}

// ------------------------------------------------------------- operator --

type OpP = { layer: L; state: unknown; data: Data; act: (op: string, args?: unknown) => void; item?: PlayoutItem; time: { nowMs: number; ageSecs: number; defaultTz: string; fps?: number; held?: boolean } };

/** The layer's clock on air has yet to start (a manual start) or is paused: its time on air stands still. */
const clockStill = (layer: L, item: PlayoutItem | undefined) => !!item && ((!!layer.manualStart && !item.clocks?.[layer.id]) || item.clocks?.[layer.id]?.pausedAt != null);

/** What the controls show now (time on air advancing between two updates). */
function useNow({ layer, state, data, item, time }: OpP) {
  useFrame();
  const serverOffset = useApp((s) => s.serverOffset) ?? 0;
  const got = useRef({ age: time.ageSecs, at: Date.now() });
  if (got.current.age !== time.ageSecs) got.current = { age: time.ageSecs, at: Date.now() };
  const now = Date.now() + (item ? serverOffset : 0);
  const e = T.eff(layer, data);
  const st = state as State | undefined;
  const tz = time.defaultTz;
  const fps = time.fps ?? 25;
  // On air the layer's own time (its clock: 0 until a manual start, still while paused), as the
  // renderer; held by the preview, it stands still too.
  const base = item ? (layerAgeFrames(item, item.age ?? 0, layer) / fps) * 1000 : got.current.age * 1000;
  const still = !!time.held || clockStill(layer, item);
  const ageMs = base + (still ? 0 : Date.now() - got.current.at);
  const r = T.run(e, st, now, ageMs, tz);
  const d = T.display(e, st, { now, ageMs, fps, defaultTz: tz });
  return { e, r, d, now, tz };
}

/** A duration of the template (mm:ss), exposable: the countdown's duration, the stopwatch's start value. */
function DurRow({ label, path, value, min }: { label: string; path: string; value: number; min?: number }) {
  const b = useBound();
  const cur = getPath(b.obj, path) as number | undefined;
  return <DurField label={label} value={cur ?? value} min={min} onChange={(ms) => b.set(path, ms)} expose={path} />;
}

/** "20:00:00" of an instant in the countdown's zone. */
function timeAt(ms: number, zone: T.ZoneSel, tz: string): string {
  const c = T.compAt(ms, zone, tz);
  return [c.h, c.mi, c.s].map((n) => String(n).padStart(2, '0')).join(':');
}

/**
 * The control centre of a stopwatch or countdown (web/src/widgets/centre.tsx; a clock has no
 * controls). Line 1: the time shown and the mode, what it waits for on air. Line 2: Play / Pause,
 * reset, restart, lap (stopwatch), − / + a step; a countdown to a time: reset only. Line 3: how it
 * starts (Designer), a value to set, the start value (exposable), the target time (exposable),
 * the step of − / + (exposable).
 */
function TimerBar(p: OpP) {
  const { layer, data, act, item } = p;
  const mode = useCentreMode();
  const designer = mode === 'designer';
  const b = useBound();
  const { e, r, d, tz } = useNow(p);
  const isTimed = T.timed(e);
  const tgt = T.isTarget(e);
  const paused = r.pausedAt != null;
  // Waiting for Continue / its trigger on air (its clock has yet to start), or held by the preview: not running yet.
  const waiting = !tgt && !r.pausedAt && !(p.state as State | undefined)?.ctl && !!item && !!layer.manualStart && !item.clocks?.[layer.id];
  const running = T.started(e, r) && !paused && !p.time.held && !waiting;
  const stepS = [1, 10, 60].includes(e.c.adjStep) ? e.c.adjStep : 10;
  const stepTxt = stepS >= 60 ? `${stepS / 60} min` : `${stepS} s`;
  const field = useRef<HTMLSpanElement>(null);
  // Confirmations only on air (a key does as its button).
  const resetMsg = t('{name} is on air: reset it to its start value?', { name: layer.name });
  const restartMsg = t('{name} is on air: start it again from its start value?', { name: layer.name });
  const confirmed = (msg: string, go: () => void) => async () => {
    if (item && !(await confirmDialog({ title: msg, confirm: t('Yes') }))) return;
    go();
  };
  const reset = confirmed(resetMsg, () => act('reset'));
  const restart = confirmed(restartMsg, () => act('restart'));
  const adj = (s: number) => act('adjust', { seconds: s });
  const playPause = () => act(paused ? 'resume' : running ? 'pause' : 'start');
  // Space stays the Operator's CONTINUE; the value is typed in the field (V).
  useKeys(
    isTimed
      ? {
          s: () => act('start'),
          p: () => !tgt && playPause(),
          x: () => void reset(),
          t: () => !tgt && void restart(),
          l: () => e.mode === 'stopwatch' && running && act('split'),
          v: () => setTimeout(() => field.current?.querySelector('input')?.focus(), 30),
          '1': () => !tgt && adj(-stepS),
          '2': () => !tgt && adj(stepS),
        }
      : {},
  );
  if (!isTimed) return null;
  const [prefix, suffix] = T.affixes(e.c, data);
  const shown = `${prefix ? `${prefix} ` : ''}${T.segText(d.segs, e.c.sep)}${suffix ? ` ${suffix}` : ''}`;
  // The template's layer (the Designer's host shows a held view of it): how it starts on air.
  const real = ((b.obj as L | undefined)?.id ? b.obj : layer) as L;
  // Older layers still waiting for Play are shown as starting on Continue; a change moves them to the layer's start.
  const startView = e.start === 'manual' ? { ...real, manualStart: true } : real;
  const startSet = (path: string, v: unknown, coalesce?: boolean) => {
    b.set(path, v, coalesce);
    if (e.start === 'manual') b.set('startMode', 'auto');
  };
  const sub = tgt ? t('until {time}', { time: r.targetMs != null ? timeAt(r.targetMs, e.targetZone, tz) : e.target || '--:--:--' }) : e.mode === 'countdown' ? tc('cc', 'countdown') : tc('cc', 'stopwatch');
  const actions: Line2Item[] = tgt
    ? [{ icon: RotateCcw, label: tc('cc', 'Reset'), key: 'X', confirm: item ? resetMsg : undefined, onClick: () => act('reset') }]
    : [
        running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: playPause } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: playPause },
        { icon: RotateCcw, label: tc('cc', 'Reset'), key: 'X', confirm: item ? resetMsg : undefined, onClick: () => act('reset') },
        { icon: RotateCw, label: tc('cc', 'Restart'), key: 'T', confirm: item ? restartMsg : undefined, onClick: () => act('restart') },
        e.mode === 'stopwatch' && { icon: Flag, label: t('Lap'), key: 'L', disabled: !running, onClick: () => act('split') },
        { icon: Minus, text: `−${stepTxt}`, label: `−${stepTxt}`, key: '1', onClick: () => adj(-stepS) },
        { icon: Plus, text: `+${stepTxt}`, label: `+${stepTxt}`, key: '2', onClick: () => adj(stepS) },
      ];
  const setValue = (v: string) => {
    const pr = tgt ? T.parseTarget(v) : T.parseDur(v);
    if ('err' in pr) return pr.err;
    if (!tgt && e.mode === 'stopwatch' && e.c.swLimitOn && 'ms' in pr && pr.ms > e.c.swLimit) return t('{value} is above the limit {limit}', { value: T.plainDur(pr.ms), limit: T.plainDur(e.c.swLimit) });
    act('set', { value: v });
  };
  return (
    <Centre name={layer.name} readout={{ v: <span style={{ color: d.color ? hex(d.color) : undefined, opacity: d.hidden ? 0.4 : 1 }}>{shown}</span>, sub }} status={<StartStatus layer={layer} item={item} />} actions={actions}>
      {!tgt && designer && (
        <Bound obj={real} set={startSet} data={b.data}>
          <StartRow layer={startView} />
        </Bound>
      )}
      {!tgt && designer && <Toggle label={t('At a fixed time')} value={e.start === 'at'} onChange={(v) => b.set('startMode', v ? 'at' : 'auto')} />}
      {!tgt && designer && e.start === 'at' && (
        <>
          <Text label={t('Start at')} path="startAt" value="20:00:00" placeholder="20:00:00" />
          <Toggle label={t('Run off air')} path="offAir" value={true} />
        </>
      )}
      <Row label={t('Value')} title={tgt ? t('New target') : t('Set value')}>
        <span ref={field} style={{ display: 'contents' }}>
          <SetInput placeholder={tgt ? '20:00:00' : 'mm:ss'} title={tgt ? t('New target') : t('Set value')} onSet={setValue} width={tgt ? 92 : 76} />
        </span>
      </Row>
      {!tgt && (e.mode === 'countdown' ? <DurRow label={t('Duration')} path="cdDuration" value={T.DEF.cdDuration} min={1000} /> : <DurRow label={t('Start value')} path="swStart" value={0} />)}
      {tgt && <Text label={t('Target time')} path="tTime" value="20:00:00" placeholder="20:00:00" />}
      {!tgt && (
        <Seg
          label={t('Step of − +')}
          path="adjStep"
          value={10}
          options={[
            [1, '1 s'],
            [10, '10 s'],
            [60, '1 min'],
          ]}
        />
      )}
    </Centre>
  );
}

// ------------------------------------------------------------- warnings --

function warnings(layer: L, data: Data, state: unknown): Warning[] {
  const out: Warning[] = [];
  const e = T.eff(layer, data);
  const now = Date.now();
  const tz = useApp.getState().timezone ?? '';
  if (!T.hasToken(e.format)) out.push({ level: 'error', text: t('The format has no time token: nothing would move on screen.') });
  if (!T.timed(e)) {
    if ('name' in e.zone && e.zone.name && !T.validZone(e.zone.name)) out.push({ level: 'error', text: t('Unknown time zone “{zone}”: the server zone is used.', { zone: e.zone.name }) });
    if (e.c.cities) for (const ct of e.c.cityList) if (ct.zone && !T.validZone(ct.zone)) out.push({ level: 'error', text: t('Unknown time zone “{zone}” ({city}).', { zone: ct.zone, city: ct.label ?? '' }) });
    return out;
  }
  const st = state as State | undefined;
  const r = T.run(e, st, now, 0, tz);
  if (T.isTarget(e)) {
    const tg = T.targetOf(e, r, now, tz);
    if (tg == null) out.push({ level: 'error', op: true, text: t('The target time is empty or invalid: the countdown shows --:--.') });
    else if (tg <= now && now - tg > 3000) {
      const z = { freeze: 'frozen at 0', overrun: 'overrun', restart: 'frozen at 0', out: 'widget hidden' }[e.atZero];
      out.push({ level: 'warn', op: true, text: t('Target already passed ({ago} ago): zero behaviour applied — {behaviour}.', { ago: T.plainDur(Math.floor((now - tg) / 1000) * 1000), behaviour: t(z) }) });
    }
  }
  if (!e.c.reserve) out.push({ level: 'info', text: t('Width reservation off: the plate moves when the number of digits changes.') });
  if (!e.legacy && !T.isTarget(e) && !st?.ctl) {
    if (e.start === 'at') {
      const ts = T.startAtMs(e, now, tz);
      out.push(
        ts == null
          ? { level: 'error', op: true, text: t('Start time invalid: use HH:mm:ss.') }
          : { level: 'info', op: true, text: ts > now ? t('Starts by itself at {time} (in {left}).', { time: e.c.startAt, left: T.plainDur(Math.ceil((ts - now) / 1000) * 1000) }) : now - ts < 6 * 3600000 ? t('Starts by itself at {time} — started.', { time: e.c.startAt }) : t('Starts by itself at {time} — already passed today, start it by hand.', { time: e.c.startAt }) },
      );
    }
    if (e.start === 'ext') out.push({ level: 'info', op: true, text: t('Waiting for the external trigger (API).') });
    if (e.start === 'auto' && !e.manualStart) out.push({ level: 'info', op: 'only', text: t('Starts by itself at the In.') });
  }
  if (e.mode === 'stopwatch' && e.c.swLimitOn && e.c.swLimit <= e.c.swStart) out.push({ level: 'warn', text: t('The limit is not above the start value: it is ignored.') });
  return out;
}

/**
 * The definition of a widget drawing clock layers (the Clock or the Time): the same drawing,
 * Operator controls and warnings, its own name, icon, new layer and inspector.
 */
export const clockWidget = (w: { id: string; label: string; icon: LucideIcon; create: () => Partial<L> & { props?: Record<string, unknown> }; content: string[]; Inspector: (p: InspP) => ReactNode }) =>
  defineWidget({
    id: w.id,
    type: 'clock',
    label: w.label,
    group: 'Time',
    icon: w.icon,
    create: w.create as never,
    live: () => true,
    content: w.content,
    Render: (p) => <TimerView layer={p.layer as L} r={p.r} w={p.w} h={p.h} data={p.data} time={p.time} state={p.state} fps={p.fps} designer={p.designer} lang={p.lang} />,
    Inspector: (p: InspectorProps<'clock'>) => <w.Inspector layer={p.layer as L} data={p.data} edit={p.edit as Edit} set={p.set} />,
    Operator: (p) => <TimerBar layer={p.layer as L} state={p.state} data={p.data} act={p.act} item={p.item} time={p.time} />,
    // A clock has no controls at all: only a stopwatch or countdown has its centre.
    controls: (l, data) => T.timed(T.eff(l as L, data)),
    // The Operator's preview holds it at its start value until its Play (a countdown to a time follows the clock).
    plays: (l, data) => {
      const e = T.eff(l as L, data);
      return T.timed(e) && !T.isTarget(e) && 'start';
    },
    warnings: (l, { data, state }) => warnings(l as L, data, state),
  });

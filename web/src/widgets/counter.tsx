// Counter: settings, drawing, inspector and control centre.
// Same picture as crates/cg-render/src/wid/counter.rs; number format, displayed
// value and actions mirror crates/cg-core/src/wid/counter.rs.
//
// The value is the layer's `to` (a number, or {{key}} from the Data canvas:
// the linked source). On air it counts up from `from` over `duration`
// seconds (as older counters did); every later value rolls from the value
// displayed. The operator's value holds until the source sends another one;
// the freeze holds the display while the source keeps being read.
import { useRef, type CSSProperties, type ReactNode } from 'react';
import { Hash, Minus, Plus, RotateCcw } from 'lucide-react';
import { col, cssColor, interpolateText, num, type Resolved } from '../engine/evaluate';
import type { Data, FontStyle, Layer, PlayoutItem } from '../engine/types';
import { ColorGroupRow, FontControls, GroupRow } from '../designer/Inspector';
import { clamp, ColorPick, Content, defineWidget, hex, ListEditor, Num, Section, Seg, Select, Text, Toggle, useKeys, type LayerOf, type Warning, withDefaults } from './kit';
import { Centre, GAP, type Line2Item } from './centre';
import { fontCss, measure } from './text';
import { useLiveTime } from './table-text';
import { confirmDialog } from '../ui/dialogs';
import { t, tc } from '../i18n';

export interface Config {
  min?: number | null;
  max?: number | null;
  step?: number;
  /** More + buttons of the Operator ("2, 3": +2 and +3). */
  extraSteps?: string;
  /** The Operator's « Freeze the display » row (exposed through its path; the value itself is not read). */
  opFreeze?: boolean;
  locale?: string;
  plus?: boolean;
  compact?: boolean;
  affixSize?: number;
  affixColor?: string;
  affixWeight?: number;
  affixRaise?: 'baseline' | 'top';
  affixGap?: number;
  tabular?: boolean;
  reserve?: 'off' | 'max' | 'chars';
  reserveChars?: number;
  transition?: 'none' | 'roll' | 'odometer' | 'flip';
  transMs?: number;
  easing?: Curve;
  colorMode?: 'fixed' | 'thresholds' | 'trend';
  thresholds?: { from: string; color: string }[];
  upColor?: string;
  downColor?: string;
  trendHold?: number;
  plate?: boolean;
  plateColor?: string;
}

export interface Anim {
  from: number;
  to: number;
  t0: number;
  dur: number;
}
/** Mirrors `State` in crates/cg-core/src/wid/counter.rs. */
export interface State {
  value?: number | null;
  srcSeen?: string | null;
  anim?: Anim | null;
  frozen?: boolean;
  alert?: { level: string; text: string; at: number } | null;
}

type Curve = 'linear' | 'out' | 'inOut' | 'back';
const D: Required<Config> = {
  min: null,
  max: null,
  step: 1,
  extraSteps: '',
  opFreeze: false,
  locale: 'fr-FR',
  plus: false,
  compact: false,
  affixSize: 100,
  affixColor: '',
  affixWeight: 0,
  affixRaise: 'baseline',
  affixGap: 0,
  tabular: false,
  reserve: 'off',
  reserveChars: 9,
  transition: 'roll',
  transMs: 1200,
  easing: 'out',
  colorMode: 'fixed',
  thresholds: [],
  upColor: '#22c55e',
  downColor: '#ff3b5c',
  trendHold: 2,
  plate: false,
  plateColor: '#160f33',
};
type K = Required<Config>;
const cfg = (c: Config): K => withDefaults(D, c) as K;
type CLayer = LayerOf<'counter'>;

// --------------------------------------------------------------- format --

export function curve(c: Curve, k: number): number {
  k = clamp(k, 0, 1);
  if (c === 'linear') return k;
  if (c === 'inOut') return k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2;
  if (c === 'back') return 1 + 2.2 * Math.pow(k - 1, 3) + 1.2 * Math.pow(k - 1, 2);
  return 1 - Math.pow(1 - k, 3);
}
const progress = (a: Anim, now: number) => (a.dur <= 0 ? 1 : clamp((now - a.t0) / a.dur, 0, 1));
const animValue = (a: Anim, now: number, c: Curve) => {
  const k = progress(a, now);
  return k >= 1 ? a.to : a.from + (a.to - a.from) * curve(c, k);
};

/** A number as typed or sent (mirrors parse_num). */
export function parseNum(raw: string): number | null {
  let t = [...String(raw ?? '').trim()]
    .filter((c) => !/\s/u.test(c) && !["'", '’', ' ', ' '].includes(c))
    .map((c) => (c === '−' ? '-' : c))
    .join('');
  const a = t.lastIndexOf(',');
  const b = t.lastIndexOf('.');
  if (a >= 0 && b >= 0) t = a > b ? t.replace(/\./g, '').replace(',', '.') : t.replace(/,/g, '');
  else if (a >= 0) t = (t.match(/,/g)?.length ?? 0) > 1 ? t.replace(/,/g, '') : t.replace(',', '.');
  else if (b >= 0 && (t.match(/\./g)?.length ?? 0) > 1) t = t.replace(/\./g, '');
  if (!/^[+-]?[\d.]*\d[\d.]*$/.test(t) || (t.match(/\./g)?.length ?? 0) > 1) return null;
  const v = parseFloat(t);
  return Number.isFinite(v) ? v : null;
}

interface Loc {
  group: string;
  dec: string;
  min: number;
  indian: boolean;
  compact: [number, string][];
}
const LOCS: Record<string, Loc> = {
  'fr-FR': {
    group: ' ',
    dec: ',',
    min: 4,
    indian: false,
    compact: [
      [1e9, ' Md'],
      [1e6, ' M'],
      [1e3, ' k'],
    ],
  },
  'en-US': {
    group: ',',
    dec: '.',
    min: 4,
    indian: false,
    compact: [
      [1e9, 'B'],
      [1e6, 'M'],
      [1e3, 'K'],
    ],
  },
  'de-DE': {
    group: '.',
    dec: ',',
    min: 4,
    indian: false,
    compact: [
      [1e9, ' Mrd.'],
      [1e6, ' Mio.'],
      [1e3, ' Tsd.'],
    ],
  },
  'de-CH': {
    group: '’',
    dec: '.',
    min: 4,
    indian: false,
    compact: [
      [1e9, ' Mrd.'],
      [1e6, ' Mio.'],
      [1e3, ' Tsd.'],
    ],
  },
  'es-ES': {
    group: '.',
    dec: ',',
    min: 5,
    indian: false,
    compact: [
      [1e9, ' mil M'],
      [1e6, ' M'],
      [1e3, ' mil'],
    ],
  },
  'en-IN': {
    group: ',',
    dec: '.',
    min: 4,
    indian: true,
    compact: [
      [1e7, 'Cr'],
      [1e5, 'L'],
      [1e3, 'K'],
    ],
  },
};
const loc = (l: string) => LOCS[l] ?? LOCS['fr-FR'];
export const LOCALES: [string, string][] = [
  ['fr-FR', 'French · 1 234 567,8'],
  ['en-US', 'English (US) · 1,234,567.8'],
  ['de-DE', 'German · 1.234.567,8'],
  ['de-CH', 'Swiss · 1’234’567.8'],
  ['es-ES', 'Spanish · 1.234.567,8'],
  ['en-IN', 'Indian · 12,34,567.8'],
];

export interface Fmt {
  decimals: number;
  grouping: boolean;
  locale: string;
  plus: boolean;
  compact: boolean;
}
export function groupBefore(f: Fmt, n: number, i: number): boolean {
  const l = loc(f.locale);
  if (!f.grouping || i === 0 || n < l.min) return false;
  const r = n - i;
  return l.indian ? r === 3 || (r > 3 && (r - 3) % 2 === 0) : r % 3 === 0;
}
function formatAbs(f: Fmt, v: number, d: number, intDigits: number): string {
  const s = Math.abs(v).toFixed(d);
  const [int0, frac] = s.split('.');
  const int = int0.padStart(intDigits, '0');
  let out = '';
  for (let i = 0; i < int.length; i++) out += (groupBefore(f, int.length, i) ? loc(f.locale).group : '') + int[i];
  return frac !== undefined ? out + loc(f.locale).dec + frac : out;
}
export function sign(f: Fmt, v: number, d: number): string {
  const tiny = 0.5 * Math.pow(10, -d);
  return v < 0 && Math.abs(v) >= tiny ? '-' : f.plus && v >= tiny ? '+' : '';
}
/** `v` written as set (mirrors format()). */
export function format(f: Fmt, v: number): string {
  const d = f.decimals;
  if (f.compact) {
    const l = loc(f.locale);
    const hit = l.compact.find(([u]) => Math.abs(v) >= u * 0.9995);
    if (hit) {
      let s = (Math.abs(v) / hit[0]).toFixed(Math.max(1, d));
      if (s.includes('.')) s = s.replace(/0+$/, '').replace(/\.$/, '');
      return `${v < 0 ? '-' : f.plus && v > 0 ? '+' : ''}${s.replace('.', l.dec)}${hit[1]}`;
    }
  }
  return sign(f, v, d) + formatAbs(f, v, d, 1);
}
const intDigits = (v: number, d: number) => Math.abs(v).toFixed(d).split('.')[0].length;

export interface Parts {
  c: K;
  from: string;
  to: string;
  duration: number;
  fmt: Fmt;
  prefix: string;
  suffix: string;
}
export const partsOf = (l: CLayer): Parts => {
  const c = cfg(l);
  return { c, from: l.from, to: l.to, duration: l.duration, fmt: { decimals: l.decimals, grouping: l.grouping, locale: c.locale, plus: c.plus, compact: c.compact }, prefix: l.prefix, suffix: l.suffix };
};
const cap = (c: K, v: number): [number, boolean] => {
  let x = v;
  if (c.max != null) x = Math.min(x, c.max);
  if (c.min != null) x = Math.max(x, c.min);
  return [x, x !== v];
};
const roundTo = (v: number, d: number) => Math.round(v * Math.pow(10, d)) / Math.pow(10, d);
function source(p: Parts, data: Data): [string, number | null] {
  const raw = interpolateText(p.to, data);
  const v = parseNum(raw);
  return [raw, v == null ? null : roundTo(cap(p.c, v)[0], p.fmt.decimals)];
}
const manual = (st: State, raw: string) => st.value != null && (!!st.frozen || st.srcSeen === raw);
/** The extra steps ("2, 3; 5" → [2, 3, 5]), positive, at most 4 (mirrors extra_steps). */
export const extraSteps = (c: K) =>
  String(c.extraSteps ?? '')
    .split(/[,; ]/)
    .map((x) => parseNum(x.trim()))
    .filter((v): v is number => v != null && v > 0 && Number.isFinite(v))
    .slice(0, 4);

interface Mem {
  target: number | null;
  anim: Anim | null;
  age: number;
  seen: number;
}

/** The value displayed now and its last transition (mirrors display()). */
export function display(p: Parts, st: State, data: Data, now: number, age: number, mem: Mem | null): [number, Anim | null, boolean] {
  const [raw, src] = source(p, data);
  const from = parseNum(interpolateText(p.from, data)) ?? 0;
  const take = now - Math.trunc(age * 1000);
  const introTarget = src ?? st.value ?? from;
  const intro: Anim = { from, to: introTarget, t0: take, dur: Math.max(0, p.duration) * 1000 };
  const at = (a: Anim) => animValue(a, now, a.t0 === take ? 'out' : p.c.easing);
  if (manual(st, raw)) {
    const v = st.value ?? 0;
    return st.anim ? [at(st.anim), st.anim, false] : [v, null, false];
  }
  if (!mem) {
    const latest = st.anim && st.anim.t0 > take ? st.anim : intro;
    const a = { ...latest, to: src ?? latest.to };
    return [at(a), a, a.t0 === take];
  }
  if (age + 1e-6 < mem.age || Math.abs(now - mem.seen) > 1500) Object.assign(mem, { target: null, anim: null, age: 0 });
  mem.age = age;
  mem.seen = now;
  const target = src ?? mem.target ?? introTarget;
  let latest = intro;
  for (const a of [st.anim, mem.anim]) if (a && a.t0 >= latest.t0) latest = a;
  if (mem.target != null && Math.abs(mem.target - target) > 1e-12) {
    const shown = at({ ...latest, to: mem.target });
    const a: Anim = { from: shown, to: target, t0: now, dur: p.c.transition === 'none' ? 0 : p.c.transMs };
    mem.anim = a;
    latest = a;
  }
  mem.target = target;
  const a = { ...latest, to: target };
  return [at(a), a, a.t0 === take];
}

export function colorFor(c: K, v: number, a: Anim | null, now: number): string | null {
  if (c.colorMode === 'thresholds') {
    const t = c.thresholds
      .map((x) => [parseNum(x.from), x.color] as const)
      .filter((x): x is readonly [number, string] => x[0] != null)
      .sort((x, y) => x[0] - y[0]);
    let out: string | null = null;
    for (const [n, colr] of t) if (v >= n) out = colr;
    return out;
  }
  if (c.colorMode === 'trend') {
    if (!a) return null;
    const fresh = c.trendHold <= 0 || now - a.t0 < c.trendHold * 1000 + a.dur;
    if (!fresh || Math.abs(a.to - a.from) < 1e-12) return null;
    return a.to > a.from ? c.upColor : c.downColor;
  }
  return null;
}

/** Odometer reels: position (0..10) of every place, least significant first (mirrors reels()). */
function reels(a: Anim | null, v: number, d: number, now: number, cv: Curve): number[] {
  const k10 = Math.pow(10, d);
  const nOf = (x: number) => Math.round(Math.abs(x) * k10);
  const digit = (n: number, i: number) => Math.max(0, Math.floor(n / Math.pow(10, i)) % 10);
  const places = (x: number) => Math.max(d + 1, intDigits(x, d) + d);
  const rem = (x: number) => ((x % 10) + 10) % 10;
  if (a && progress(a, now) < 1) {
    const P = Math.max(places(a.from), places(a.to));
    const nf = nOf(a.from);
    const nt = nOf(a.to);
    const up = nt >= nf;
    const gap = Math.abs(nt - nf);
    const e = curve(cv, progress(a, now));
    return Array.from({ length: P }, (_, i) => {
      const start = digit(nf, i);
      const end = digit(nt, i);
      let dd = up ? rem(end - start) : -rem(start - end);
      dd += (up ? 10 : -10) * Math.min(2, Math.floor(gap / Math.pow(10, i + 1)));
      return rem(start + dd * e);
    });
  }
  const n = nOf(v);
  return Array.from({ length: places(v) }, (_, i) => digit(n, i));
}
const placeFrac = (v: number, p: number) => (p <= 0 ? 1 : clamp((Math.log10(Math.max(Math.abs(v), 1e-9)) - p) * 3 + 1, 0, 1));

// --------------------------------------------------------------- render --

let mctx: CanvasRenderingContext2D | null = null;
const asc = new Map<string, [number, number]>();
/** Ascent and descent of a CSS font (as the Rust renderer's font metrics). */
function metrics(font: string, size: number): [number, number] {
  let m = asc.get(font);
  if (!m) {
    mctx ??= document.createElement('canvas').getContext('2d');
    if (!mctx) return [size * 0.97, size * 0.24];
    mctx.font = font;
    const t = mctx.measureText('0');
    m = [t.fontBoundingBoxAscent / size, t.fontBoundingBoxDescent / size];
    if (!(m[0] > 0)) m = [0.96875, 0.2421875];
    asc.set(font, m);
  }
  return [m[0] * size, m[1] * size];
}

/** A piece of text with its baseline at `b` (line box of 1.2, as the Rust renderer). */
function At({ x, b, font, size, color, children, style }: { x: number; b: number; font: string; size: number; color?: string; children: ReactNode; style?: CSSProperties }) {
  const L = size * 1.2;
  const [a, d] = metrics(font, size);
  return <div style={{ position: 'absolute', left: x, top: b - L / 2 - (a - d) / 2, height: L, lineHeight: `${L}px`, font, whiteSpace: 'pre', color, ...style }}>{children}</div>;
}

function Render({ layer, r, w, h, data, time, state }: { layer: CLayer; r: Resolved; w: number; h: number; data: Data; time: { nowMs: number; ageSecs: number }; state: unknown }) {
  const st = (state ?? {}) as State;
  const p = partsOf(layer);
  const c = p.c;
  const now = time.nowMs;
  // Its own memory on each picture (the preview and the on-air monitor show the same layer with other values).
  const memRef = useRef<Mem | null>(null);
  const mem = (memRef.current ??= { target: null, anim: null, age: 0, seen: 0 });
  const [v, anim, isIntro] = display(p, st, data, now, time.ageSecs, mem);
  const cv: Curve = isIntro ? 'out' : c.easing;
  const fill = cssColor(col(r.fill));
  const tc = colorFor(c, v, anim, now);
  const colr = tc ? hex(tc) : fill;
  const size = num(r.fontSize);
  const ls = num(r.letterSpacing);
  const font = fontCss(layer.fontFamily, layer.fontWeight, size);
  const m = (s: string) => measure(font, 0, s);
  const prefix = interpolateText(p.prefix, data);
  const suffix = interpolateText(p.suffix, data);
  const asize = (size * Math.max(1, c.affixSize)) / 100;
  const afont = fontCss(layer.fontFamily, c.affixWeight || layer.fontWeight, asize);
  const dw = Math.max(...'0123456789'.split('').map((x) => m(x)));
  const dec = p.fmt.decimals;
  const mode = p.fmt.compact && c.transition === 'odometer' ? 'roll' : c.transition;
  const active = !!anim && progress(anim, now) < 1;
  const flip = mode === 'flip' && active;
  const text = format(p.fmt, mode === 'none' ? (anim?.to ?? v) : v);
  const inkW = (s: string) => (c.tabular ? [...s].reduce((a, ch) => a + (/\d/.test(ch) ? dw : m(ch)), 0) + ls * Math.max(0, [...s].length - 1) : measure(font, ls, s));
  const cellW = dw + 0.12 * size;
  const flipW = (s: string) => [...s].reduce((a, ch) => a + (/\d/.test(ch) ? cellW + 0.05 * size : m(ch)), 0);
  const [flipFrom, flipTo] = mode === 'flip' && anim ? [format(p.fmt, anim.from), format(p.fmt, anim.to)] : [text, text];
  const odo = reels(anim, v, dec, now, cv);
  const pInt = Math.max(0, odo.length - dec);
  const groupAfter = (pl: number) => pl < pInt && groupBefore(p.fmt, pInt, pInt - pl);
  const L = loc(p.fmt.locale);
  let numW: number;
  if (mode === 'flip') {
    const l = Math.max([...flipFrom].length, [...flipTo].length);
    numW = flip ? Math.max(flipW(flipTo.padStart(l, ' ').replace(/ /g, '0')), flipW(flipTo)) : flipW(text);
  } else if (mode === 'odometer') {
    const sg = sign(p.fmt, v, dec);
    numW = sg ? m(sg) : 0;
    for (let i = odo.length - 1; i >= 0; i--) {
      const pi = i - dec;
      const fr = placeFrac(v, pi);
      numW += dw * fr;
      if (pi === 0 && dec > 0) numW += m(L.dec);
      if (pi > 0 && groupAfter(pi)) numW += m(L.group) * fr;
    }
  } else numW = inkW(text);
  const resW = (x: number | null) => (x == null ? 0 : mode === 'flip' ? flipW(format(p.fmt, x)) : inkW(format(p.fmt, x)));
  const reserve = c.reserve === 'chars' ? c.reserveChars * dw : c.reserve === 'max' ? Math.max(resW(c.min), resW(c.max)) : 0;
  const boxW = Math.max(numW, reserve);
  const preW = prefix ? measure(afont, 0, prefix) + c.affixGap : 0;
  const sufW = suffix ? measure(afont, 0, suffix) + c.affixGap : 0;
  const pad = c.plate ? 44 : 0;
  const avail = w - 2 * pad;
  const total = preW + boxW + sufW;
  const x0 = pad + (layer.align === 'center' ? (avail - total) / 2 : layer.align === 'right' ? avail - total : 0);
  const lineH = size * 1.2;
  const top = layer.verticalAlign === 'top' ? 0 : layer.verticalAlign === 'bottom' ? h - lineH : (h - lineH) / 2;
  const [a, dsc] = metrics(font, size);
  const baseline = top + (lineH - (a + dsc)) / 2 + a;
  const numX = x0 + preW + (layer.align === 'center' ? (boxW - numW) / 2 : layer.align === 'right' ? boxW - numW : 0);
  const affB = c.affixRaise === 'top' ? baseline - 0.727 * (size - asize) : baseline;
  const acol = c.affixColor.trim() ? hex(c.affixColor) : fill;
  const out: ReactNode[] = [];
  let x = numX;
  if (mode === 'odometer') {
    const sg = sign(p.fmt, v, dec);
    if (sg) {
      out.push(
        <At key="sg" x={x} b={baseline} font={font} size={size} color={colr}>
          {sg}
        </At>,
      );
      x += m(sg);
    }
    const cellH = 1.1 * size;
    for (let i = odo.length - 1; i >= 0; i--) {
      const pi = i - dec;
      const fr = placeFrac(v, pi);
      const cw = dw * fr;
      if (fr > 0.001) {
        const d0 = Math.floor(odo[i]);
        const frac = odo[i] - d0;
        const ct = baseline - 0.95 * size;
        out.push(
          <div key={`r${i}`} style={{ position: 'absolute', left: x, top: ct, width: Math.max(0.5, cw), height: cellH, overflow: 'hidden', opacity: fr }}>
            {[
              [d0, -frac * cellH],
              [d0 + 1, (1 - frac) * cellH],
            ].map(([k, dy], j) => {
              const ch = String(((k % 10) + 10) % 10);
              return (
                <At key={j} x={(cw - m(ch)) / 2} b={baseline - ct + dy} font={font} size={size} color={colr}>
                  {ch}
                </At>
              );
            })}
          </div>,
        );
      }
      x += cw;
      if (pi === 0 && dec > 0) {
        out.push(
          <At key={`d${i}`} x={x} b={baseline} font={font} size={size} color={colr}>
            {L.dec}
          </At>,
        );
        x += m(L.dec);
      }
      if (pi > 0 && groupAfter(pi)) {
        if (fr > 0.001)
          out.push(
            <At key={`g${i}`} x={x} b={baseline} font={font} size={size} color={colr}>
              {L.group}
            </At>,
          );
        x += m(L.group) * fr;
      }
    }
  } else if (mode === 'flip') {
    const k = active && anim ? curve(cv, progress(anim, now)) : 1;
    out.push(<Flip key="flip" from={flipFrom} to={flipTo} text={text} k={k} active={active} x={x} baseline={baseline} asc={a} size={size} cellW={cellW} font={font} color={colr} m={m} />);
  } else if (c.tabular) {
    for (const [i, ch] of [...text].entries()) {
      const cw = /\d/.test(ch) ? dw : m(ch);
      out.push(
        <At key={i} x={x + (cw - m(ch)) / 2} b={baseline} font={font} size={size} color={colr}>
          {ch}
        </At>,
      );
      x += cw + ls;
    }
  } else
    out.push(
      <At key="t" x={x} b={baseline} font={font} size={size} color={colr} style={{ letterSpacing: ls }}>
        {text}
      </At>,
    );
  return (
    <>
      {c.plate && <div style={{ position: 'absolute', inset: 0, background: hex(c.plateColor), borderRadius: 6, boxShadow: 'inset 0 -10px #ff3b5c' }} />}
      <div style={{ position: 'absolute', inset: 0, overflow: 'hidden', fontKerning: 'normal' }}>
        {prefix && (
          <At x={x0} b={affB} font={afont} size={asize} color={acol}>
            {prefix}
          </At>
        )}
        {out}
        {suffix && (
          <At x={x0 + preW + boxW + c.affixGap} b={affB} font={afont} size={asize} color={acol}>
            {suffix}
          </At>
        )}
      </div>
    </>
  );
}

/** Split-flap cards (mirrors draw_flip: the flap is scaled about the hinge, no perspective). */
function Flip({ from, to, text, k, active, x, baseline, asc: a, size, cellW, font, color, m }: { from: string; to: string; text: string; k: number; active: boolean; x: number; baseline: number; asc: number; size: number; cellW: number; font: string; color: string; m: (s: string) => number }) {
  let A: string[];
  let B: string[];
  if (active) {
    const l = Math.max([...from].length, [...to].length);
    A = [...from.padStart(l, ' ')];
    B = [...to.padStart(l, ' ')];
  } else A = B = [...text];
  const cardH = 1.15 * size;
  const cardTop = baseline - a - 0.05 * size;
  const hinge = cardTop + cardH / 2;
  const gap = 0.012 * size;
  const r = 0.07 * size;
  const out: ReactNode[] = [];
  const half = (key: string, ch: string, cx: number, topHalf: boolean, sy: number, shade: number) => {
    const hy = topHalf ? cardTop : hinge + gap;
    const hh = cardH / 2 - gap;
    out.push(
      <div key={key} style={{ position: 'absolute', left: cx, top: hy, width: cellW, height: hh, borderRadius: r, overflow: 'hidden', background: topHalf ? '#2c2170' : '#241a5c', transform: sy !== 1 ? `scaleY(${sy})` : undefined, transformOrigin: `50% ${hinge - hy}px` }}>
        {ch !== ' ' && (
          <At x={(cellW - m(ch)) / 2} b={baseline - hy} font={font} size={size} color={color}>
            {ch}
          </At>
        )}
        {shade > 0.001 && <div style={{ position: 'absolute', inset: 0, background: `rgba(0,0,0,${shade})` }} />}
      </div>,
    );
  };
  let xx = x;
  A.forEach((o, i) => {
    const n = B[i];
    if (!/\d/.test(n) && !/\d/.test(o)) {
      if (n !== ' ') {
        out.push(
          <At key={`s${i}`} x={xx} b={baseline} font={font} size={size} color={color}>
            {n}
          </At>,
        );
        xx += m(n);
      }
      return;
    }
    if (n === ' ' && (!active || k >= 1)) return;
    const cx = xx + 0.025 * size;
    xx += cellW + 0.05 * size;
    if (!active || o === n || k >= 1) {
      half(`t${i}`, n, cx, true, 1, 0);
      half(`b${i}`, n, cx, false, 1, 0);
      return;
    }
    half(`t${i}`, n, cx, true, 1, 0);
    half(`b${i}`, o, cx, false, 1, 0);
    if (k < 0.5) half(`f${i}`, o, cx, true, Math.cos(k * Math.PI), k);
    else half(`f${i}`, n, cx, false, Math.abs(Math.cos((1 - k) * Math.PI)), 0.5 - k / 2);
  });
  return <>{out}</>;
}

// ------------------------------------------------------------- operator --

type OpP = { layer: CLayer; state: unknown; data: Data; act: (op: string, args?: unknown) => void; item?: PlayoutItem; time: { nowMs: number; ageSecs: number; held?: boolean } };

/**
 * The control centre of the counter (web/src/widgets/centre.tsx). Line 1: the value shown, on air
 * or frozen. Line 2: − / + the step, the extra steps, a value to set, reset. Line 3: freeze the
 * display (a value from data; the Operator sees it when exposed), the step and the extra steps
 * (exposable).
 */
function CounterBar(p: OpP) {
  const { layer, state, data, act, item, time } = p;
  const { now, age } = useLiveTime(time);
  const st = (state ?? {}) as State;
  const pt = partsOf(layer);
  // The readout: the value the number settles at (not the roll towards it).
  const [shown, anim] = display(pt, st, data, now, age, null);
  const v = anim ? anim.to : shown;
  const plain = { ...pt.fmt, plus: false, compact: false };
  const stepTxt = format(plain, pt.c.step);
  const wired = /\{\{\w+\}\}/.test(layer.to);
  const resetMsg = item ? t('{name} is on air: back to the initial value?', { name: layer.name }) : undefined;
  // ↑ / ↓ stay the rundown's. Confirmations only on air (a key does as its button).
  useKeys({
    '+': () => act('inc'),
    '-': () => act('dec'),
    f: () => wired && act('freeze'),
    x: async () => {
      if (resetMsg && !(await confirmDialog({ title: resetMsg, confirm: t('Yes') }))) return;
      act('reset');
    },
  });
  const value = format(pt.fmt, v);
  const actions: Line2Item[] = [
    { icon: Minus, text: `− ${stepTxt}`, label: `− ${stepTxt}`, key: '−', onClick: () => act('dec') },
    { icon: Plus, text: `+ ${stepTxt}`, label: `+ ${stepTxt}`, key: '+', onClick: () => act('inc') },
    ...extraSteps(pt.c).map((x) => ({ icon: Plus, text: `+ ${format(plain, x)}`, label: `+ ${format(plain, x)}`, onClick: () => act('inc', { by: x }) })),
    {
      field: true,
      placeholder: t('Value'),
      title: t('Set value'),
      width: 64,
      onSet: (x) => {
        if (parseNum(x) == null) return t('“{value}” is not a number', { value: x });
        act('set', { value: x });
      },
    },
    GAP,
    { icon: RotateCcw, label: tc('cc', 'Reset'), key: 'X', confirm: resetMsg, onClick: () => act('reset') },
  ];
  const sub = st.frozen ? tc('cc', 'frozen') : item ? tc('cc', 'on air') : undefined;
  return (
    <Centre name={layer.name} readout={{ v: value, sub }} actions={actions}>
      {wired && <Toggle label={t('Freeze the display')} expose="opFreeze" exposeValue={true} value={!!st.frozen} onChange={() => act('freeze')} />}
      <Num label={tc('cc', 'Step')} path="step" value={1} min={0.001} step={1} precision={3} />
      <Text label={tc('cc', 'Extra steps')} path="extraSteps" value="" placeholder="2, 3" />
    </Centre>
  );
}

// ------------------------------------------------------------ inspector --

function Inspector({ layer, edit }: { layer: CLayer; edit: (fn: (l: CLayer) => void, key?: string) => void }) {
  const c = cfg(layer);
  const set = (key: string, v: unknown) => edit((l) => void ((l as unknown as Record<string, unknown>)[key] = v));
  const limits = c.min != null || c.max != null;
  const affixed = !!(layer.prefix || layer.suffix);
  return (
    <>
      <Section id="ct-value" title={t('Value')}>
        <Content label={t('Value')} path="to" value="" placeholder={t('Number or {{key}}')} />
        <Text label={t('Prefix')} path="prefix" value="" />
        <Text label={t('Suffix')} path="suffix" value="" />
        <Text label={t('Count from')} path="from" value="0" />
        <Num label={t('Intro')} path="duration" value={2} min={0} step={0.1} precision={1} unit="s" />
        <Toggle label={t('Limits')} value={limits} onChange={(on) => edit((l) => void Object.assign(l, on ? { min: 0, max: 10000000 } : { min: undefined, max: undefined }))} />
        {limits && (
          <>
            <Num expose="min" exposeValue={c.min ?? 0} label={t('Minimum')} value={c.min ?? 0} step={1} precision={3} onChange={(v) => set('min', v)} />
            <Num expose="max" exposeValue={c.max ?? 0} label={t('Maximum')} value={c.max ?? 0} step={1} precision={3} onChange={(v) => set('max', v)} />
          </>
        )}
      </Section>
      <Section id="ct-format" title={t('Format')}>
        <Select label={t('Language')} path="locale" value={c.locale} options={LOCALES.map(([v, l]) => [v, t(l)] as [string, string])} />
        <Num expose="decimals" exposeValue={layer.decimals} label={t('Decimals')} value={layer.decimals} min={0} max={3} onChange={(v) => set('decimals', v)} />
        <Toggle expose="grouping" exposeValue={layer.grouping} label={t('Thousands')} value={layer.grouping} onChange={(v) => set('grouping', v)} />
        <Toggle label={t('Plus sign')} path="plus" value={false} />
        <Toggle label={t('Compact')} path="compact" value={false} />
      </Section>
      <Section id="ct-figures" title={t('Style')}>
        <div className="pp-typo">
          <FontControls layer={layer as Layer & FontStyle} set={set} />
          <GroupRow layer={layer as Layer} spec={{ label: t('Size'), props: ['fontSize'], min: 1, suffix: 'px' }} />
          <ColorGroupRow layer={layer as Layer} prop="fill" label={t('Color')} />
        </div>
        <Toggle label={t('Tabular')} path="tabular" value={false} />
        <Seg
          label={t('Reserve width')}
          path="reserve"
          value={c.reserve}
          options={[
            ['off', t('Off')],
            ['max', t('Min / max')],
            ['chars', t('Digits')],
          ]}
        />
        {c.reserve === 'chars' && <Num label={t('Digits')} path="reserveChars" value={c.reserveChars} min={1} max={16} />}
        {affixed && (
          <>
            <Num label={t('Affix size')} path="affixSize" value={c.affixSize} min={20} max={100} unit="%" />
            <ColorPick label={t('Affix color')} path="affixColor" value="" none={t('Text colour')} />
            <Seg
              label={t('Affix weight')}
              path="affixWeight"
              value={c.affixWeight}
              options={[
                [0, t('Same')],
                [400, t('Regular')],
                [700, t('Bold')],
                [900, t('Black')],
              ]}
            />
            <Seg
              label={t('Affix position')}
              path="affixRaise"
              value={c.affixRaise}
              options={[
                ['baseline', t('Baseline')],
                ['top', t('Raised')],
              ]}
            />
            <Num label={t('Affix gap')} path="affixGap" value={c.affixGap} min={0} max={60} unit="px" />
          </>
        )}
        <Toggle label={t('Plate')} path="plate" value={false} />
        {c.plate && <ColorPick label={t('Plate color')} path="plateColor" value={D.plateColor} />}
      </Section>
      <Section id="ct-color" title={t('Color by value')}>
        <Seg
          label={t('Mode')}
          path="colorMode"
          value={c.colorMode}
          options={[
            ['fixed', t('Fixed')],
            ['thresholds', t('Thresholds')],
            ['trend', t('Up / down')],
          ]}
        />
        {c.colorMode === 'thresholds' && (
          <ListEditor
            label={t('From value')}
            path="thresholds"
            value={c.thresholds}
            fields={[
              { key: 'from', placeholder: t('from') },
              { key: 'color', color: true, w: 44 },
            ]}
            add={() => ({ from: '0', color: '#38bdf8' })}
            addLabel={t('Threshold')}
          />
        )}
        {c.colorMode === 'trend' && (
          <>
            <ColorPick label={tc('trend', 'Up')} path="upColor" value={D.upColor} />
            <ColorPick label={tc('trend', 'Down')} path="downColor" value={D.downColor} />
            <Num label={tc('trend', 'Hold')} path="trendHold" value={c.trendHold} min={0} max={10} step={0.5} precision={1} unit="s" />
          </>
        )}
      </Section>
      <Section id="ct-trans" title={t('Transition')}>
        <Seg
          label={t('Type')}
          path="transition"
          value={c.transition}
          options={[
            ['none', t('Cut')],
            ['roll', t('Roll')],
            ['odometer', t('Odometer')],
            ['flip', t('Flip')],
          ]}
        />
        {c.transition !== 'none' && (
          <>
            <Num label={t('Duration')} path="transMs" value={c.transMs} min={100} max={5000} step={50} unit="ms" />
            <Select
              label={t('Curve')}
              path="easing"
              value={c.easing}
              options={[
                ['linear', t('Linear')],
                ['out', t('Ease out')],
                ['inOut', t('Ease in-out')],
                ['back', t('Overshoot')],
              ]}
            />
          </>
        )}
      </Section>
    </>
  );
}

/** An alert of the server (its wording is the Rust module's), in the interface's language. */
function alertText(s: string): string {
  let m = /^(.*) is out of range \((.*) – (.*)\): capped at (.*)\.$/.exec(s);
  if (m) return t('{value} is out of range ({min} – {max}): capped at {capped}.', { value: m[1], min: m[2], max: m[3], capped: m[4] });
  m = /^Value “(.*)”: not a number, ignored — (.*) kept\.$/.exec(s);
  if (m) return t('Value “{value}”: not a number, ignored — {kept} kept.', { value: m[1], kept: m[2] });
  return s;
}

export default defineWidget({
  id: 'counter',
  type: 'counter',
  label: 'Counter',
  group: 'Data',
  icon: Hash,
  create: () => ({
    from: '0',
    to: '1234',
    duration: 2,
    decimals: 0,
    prefix: '',
    suffix: '',
    grouping: true,
    fontFamily: 'Inter',
    fontWeight: 700,
    align: 'center',
    verticalAlign: 'middle',
    uppercase: false,
    tabular: true,
    props: { width: 800, height: 220, fontSize: 140, fill: [1, 1, 1, 1] },
  }),
  ownsLook: true,
  live: () => true,
  content: ['to'],
  Render: ({ layer, r, w, h, data, time, state }) => <Render layer={layer} r={r} w={w} h={h} data={data} time={time} state={state} />,
  Inspector: ({ layer, edit }) => <Inspector layer={layer} edit={edit} />,
  Operator: ({ layer, state, data, act, time, item }) => <CounterBar layer={layer} state={state} data={data} act={act} time={time} item={item} />,
  warnings: (layer, { data, state }) => {
    const out: Warning[] = [];
    const st = (state ?? {}) as State;
    const p = partsOf(layer);
    const c = p.c;
    if (st.alert && Date.now() - st.alert.at < 8000) out.push({ level: st.alert.level === 'error' ? 'error' : 'warn', text: alertText(st.alert.text), op: true });
    if (c.min != null && c.max != null && c.min >= c.max) out.push({ level: 'error', text: t('Minimum must be lower than the maximum.'), op: true });
    const raw = interpolateText(layer.to, data);
    const v = parseNum(raw);
    if (v == null) out.push({ level: 'warn', text: /\{\{\w+\}\}/.test(layer.to) ? t('Source sent “{value}”: not a number, ignored — the last valid value stays on air.', { value: raw }) : t('Value “{value}”: not a number, ignored — the last valid value stays on air.', { value: raw }), op: true });
    else if (cap(c, v)[1]) out.push({ level: 'error', text: t('{value} is out of range ({min} – {max}): capped at {capped}.', { value: format(p.fmt, v), min: c.min == null ? '−∞' : format(p.fmt, c.min), max: c.max == null ? '∞' : format(p.fmt, c.max), capped: format(p.fmt, cap(c, v)[0]) }), op: true });
    if (st.frozen && v != null && st.value != null && Math.abs(roundTo(cap(c, v)[0], p.fmt.decimals) - st.value) > 1e-9) out.push({ level: 'info', text: t('Display frozen at {frozen} — source now at {value}.', { frozen: format(p.fmt, st.value), value: format(p.fmt, v) }), op: true });
    if (!c.tabular && c.transition !== 'flip' && c.transition !== 'none') out.push({ level: 'info', text: t('Proportional figures: the number width changes while it rolls (turn Tabular on).') });
    return out;
  },
});

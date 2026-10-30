// Typewriter: settings, drawing, inspector and Operator controls.
// Same picture as crates/cg-render/src/wid/typewriter.rs; timeline and actions in crates/cg-core/src/wid/typewriter.rs.
//
// Frozen layout: the complete text is laid out (and auto-shrunk) before typing;
// what is not typed yet is not drawn, so no word ever jumps line. Everything
// is a pure function of the elapsed time (time on air, or the operator's
// timestamps): the state never counts.
import { useEffect, useRef, type CSSProperties } from 'react';
import { ChevronsRight, Keyboard, Pause, Play, SkipForward, Trash2 } from 'lucide-react';
import { col, cssColor, interpolateText, num } from '../engine/evaluate';
import type { Data, FontStyle, Layer, PlayoutItem } from '../engine/types';
import { layerAgeFrames } from '../engine/widgets';
import { graphemes } from '../engine/textFit';
import { ColorGroupRow, FontControls, GroupRow } from '../designer/Inspector';
import { clamp, ColorPick, Content, defineWidget, hex, ListEditor, Num, RangeNum, Section, Seg, Text, Toggle, useBound, useCentreMode, useKeys, type LayerOf, type Warning, withDefaults } from './kit';
import { ADVANCE_MODES, Centre, Countdown, GAP, StartRow, WaitsContinue, WaitsTrigger, type AdvanceMode, type Line2Item } from './centre';
import { fontCss, measure } from './text';
import { useLiveTime } from './table-text';
import { layerTime } from './ticker';
import { confirmDialog } from '../ui/dialogs';
import { t, tc } from '../i18n';

export interface Config {
  /** Texts typed after the first one (`text`). */
  more?: { text: string }[];
  unit?: 'char' | 'word' | 'line';
  speedMode?: 'rate' | 'duration';
  duration?: number;
  /** Speed mode `rate`: a text too long for this many seconds is typed faster, to end on time (0 = no limit). */
  maxDuration?: number;
  jitter?: number;
  punctPause?: number;
  appear?: 'instant' | 'fade' | 'slide';
  appearMs?: number;
  cursorCh?: string;
  /** Empty = the text colour. */
  cursorColor?: string;
  cursorBlink?: boolean;
  cursorHide?: boolean;
  erase?: 'backspace' | 'block';
  eraseRate?: number;
  /** The pause between two texts chained by time: a typed text stays that long before it is erased. */
  hold?: number;
  chainLoop?: boolean;
  /** The texts are chained: the next one comes by itself (`nextOn`). */
  autoNext?: boolean;
  /** How the next text comes: after `hold` seconds, on Continue (F2) or on the trigger `nextTrigger`. */
  nextOn?: AdvanceMode;
  nextTrigger?: string;
  /** Older layers: the layout is now always frozen (no word jumps line). */
  frozen?: boolean;
  onEdit?: 'restart' | 'continue';
  autoShrink?: boolean;
  minSize?: number;
  lineHeight?: number;
  plate?: boolean;
  plateColor?: string;
  /** The plate's bar on the left ('' = none). */
  plateAccent?: string;
  shadow?: boolean;
}

/** Mirrors `State` in crates/cg-core/src/wid/typewriter.rs. */
export interface State {
  mode?: 'auto' | 'run' | 'paused' | 'idle';
  idx?: number;
  base?: number;
  since?: number | null;
  allAt?: number | null;
  /** The chaining changed on air (null = the template's). */
  auto?: boolean | null;
  on?: AdvanceMode | null;
  secs?: number | null;
  trigger?: string | null;
}

const D: Required<Config> = {
  more: [],
  unit: 'char',
  speedMode: 'rate',
  duration: 4,
  maxDuration: 0,
  jitter: 0,
  punctPause: 0,
  appear: 'instant',
  appearMs: 220,
  cursorCh: '|',
  cursorColor: '',
  cursorBlink: true,
  cursorHide: false,
  erase: 'backspace',
  eraseRate: 90,
  hold: 1.6,
  chainLoop: false,
  autoNext: true,
  nextOn: 'timed',
  nextTrigger: '',
  frozen: true,
  onEdit: 'restart',
  autoShrink: false,
  minSize: 30,
  lineHeight: 1.2,
  plate: false,
  plateColor: '#160f33',
  plateAccent: '#ff3b5c',
  shadow: false,
};
type K = Required<Config>;
const cfg = (c: Config): K => withDefaults(D, c);
type TwLayer = LayerOf<'typewriter'>;

// ------------------------------------------------------------- timeline --

/** Deterministic noise 0..1 (same integer hash as rnd() in Rust). */
export function rnd(i: number, j: number): number {
  let h = ((Math.imul(i, 374761393) >>> 0) + (Math.imul(j, 668265263) >>> 0)) >>> 0;
  h = (h ^ 0x9e3779b9) >>> 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177) >>> 0;
  h = (h ^ (h >>> 16)) >>> 0;
  return h / 4294967296;
}
const isPunct = (s: string) => /[.,;:!?…—]$/.test(s.trimEnd());
const wordChar = (g: string) => /^[\p{L}\p{N}]/u.test(g);

export interface Timeline {
  g: string[];
  units: [number, number][];
  t: number[];
  unitOf: number[];
  appear: number;
  typeEnd: number;
  eraseDur: number;
}

/** When each unit of one text appears (mirrors timeline()). */
export function timeline(c: K, rate: number, text: string, idx: number, lines?: number[]): Timeline {
  const g = graphemes(text);
  const n = g.length;
  const units: [number, number][] = [];
  if (c.unit === 'char') for (let i = 0; i < n; i++) units.push([i, i + 1]);
  else if (c.unit === 'word') {
    let start = 0;
    let seen = false;
    for (let i = 0; i < n; i++) {
      const w = wordChar(g[i]);
      if (w && seen && (i === 0 || !wordChar(g[i - 1]))) (units.push([start, i]), (start = i));
      seen ||= w;
    }
    if (n > start) units.push([start, n]);
  } else {
    const lineOf = (i: number) => (lines ? (lines[i] ?? 0) : g.slice(0, i).filter((x) => x.includes('\n')).length);
    let start = 0;
    for (let i = 1; i <= n; i++) if (i === n || lineOf(i) !== lineOf(start)) (units.push([start, i]), (start = i));
  }
  const k = units.length;
  const per = c.speedMode === 'rate' ? 1 / Math.max(0.1, rate) : 1;
  const t: number[] = [];
  let acc = 0;
  for (let i = 0; i < k; i++) {
    acc += (1 + (clamp(c.jitter, 0, 100) / 100) * (rnd(i, idx) * 2 - 1)) * per;
    t.push(acc);
    if (i + 1 < k && isPunct(g.slice(units[i][0], units[i][1]).join(''))) acc += Math.max(0, c.punctPause) / 1000;
  }
  // As cg-core: the last unit lands on the duration; at a rate, only past the maximum.
  const fit = c.speedMode === 'duration' ? Math.max(0, c.duration) : c.maxDuration > 0 && k > 0 && t[k - 1] > c.maxDuration ? c.maxDuration : null;
  if (fit !== null && k > 0) {
    const last = Math.max(1e-9, t[k - 1]);
    for (let i = 0; i < k; i++) t[i] *= fit / last;
  }
  const appear = c.appear === 'instant' ? 0 : Math.max(0, c.appearMs) / 1000;
  const unitOf = new Array<number>(n).fill(0);
  units.forEach(([a, b], i) => {
    for (let j = a; j < b; j++) unitOf[j] = i;
  });
  return { g, units, t, unitOf, appear, typeEnd: (t[k - 1] ?? 0) + appear, eraseDur: c.erase === 'block' ? 0.45 : n / Math.max(1, c.eraseRate) };
}

/** How the texts are chained on air: the template's settings, or what the operator changed (mirrors chain()). */
export interface Chain {
  auto: boolean;
  on: AdvanceMode;
  /** The pause between two texts, s. */
  hold: number;
  trigger: string;
  looping: boolean;
}
export const chainOf = (c: K, st: State): Chain => ({ auto: st.auto ?? c.autoNext, on: st.on ?? c.nextOn, hold: Math.max(0, st.secs ?? c.hold), trigger: (st.trigger ?? c.nextTrigger).trim(), looping: c.chainLoop });
/** Text `i` has a next one (Next goes there). */
export const hasNext = (c: K, n: number, i: number) => n > 1 && (c.chainLoop || i + 1 < n);
/** The next text comes by itself after text `i`: chained by time (then it erases itself after its pause). */
export const autoNext = (ch: Chain, n: number, i: number) => n > 1 && (ch.looping || i + 1 < n) && ch.auto && ch.on === 'timed';

export function elapsed(st: State, nowMs: number, age: number): number {
  const mode = st.mode ?? 'auto';
  if (mode === 'auto') return Math.max(0, age);
  if (mode === 'run') return (st.base ?? 0) + Math.max(0, st.since != null ? (nowMs - st.since) / 1000 : 0);
  return st.base ?? 0;
}

function position(ch: Chain, n: number, start: number, e: number, tl: (i: number) => Timeline): [number, number] {
  if (!n) return [0, e];
  const cycle = (t: Timeline) => t.typeEnd + ch.hold + t.eraseDur;
  let i = Math.min(start, n - 1);
  e = Math.max(0, e);
  const timed = ch.auto && ch.on === 'timed';
  if (timed && ch.looping && n > 1) {
    let total = 0;
    for (let j = 0; j < n; j++) total += cycle(tl(j));
    if (total > 1e-6) e %= total;
  }
  for (let guard = 0; guard < 4 * n + 4; guard++) {
    if (!autoNext(ch, n, i)) return [i, e];
    const cy = cycle(tl(i));
    if (e < cy) return [i, e];
    e -= cy;
    i = (i + 1) % n;
  }
  return [i, e];
}

/** The text shown and its elapsed time, "Show all" included (mirrors now_at). */
export function nowAt(c: K, st: State, n: number, e: number, tl: (i: number) => Timeline): [number, number] {
  const ch = chainOf(c, st);
  if (st.allAt != null && e >= st.allAt) {
    const [i, ea] = position(ch, n, st.idx ?? 0, st.allAt, tl);
    e += Math.max(0, tl(i).typeEnd - ea);
  }
  return position(ch, n, st.idx ?? 0, e, tl);
}

// --------------------------------------------------------------- layout --

/** Lines of graphemes (pre-wrap), mirrors wrap_g. */
export function wrapG(g: string[], maxW: number, m: (s: string) => number): [number, number][] {
  const nl = (s: string) => s.includes('\n');
  const sp = (s: string) => /^\s+$/u.test(s);
  const width = (a: number, b: number) => m(g.slice(a, b).join('').trimEnd());
  const n = g.length;
  const lines: [number, number][] = [];
  let start = 0;
  let i = 0;
  while (i < n) {
    if (nl(g[i])) {
      lines.push([start, i + 1]);
      start = i + 1;
      i++;
      continue;
    }
    if (sp(g[i])) {
      i++;
      continue;
    }
    let j = i;
    while (j < n && !sp(g[j]) && !nl(g[j])) j++;
    if (width(start, j) <= maxW) {
      i = j;
      continue;
    }
    if (start < i) {
      lines.push([start, i]);
      start = i;
      if (width(i, j) <= maxW) {
        i = j;
        continue;
      }
    }
    for (let q = i; q < j; q++) {
      if (q > start && width(start, q + 1) > maxW) {
        lines.push([start, q]);
        start = q;
      }
    }
    i = j;
  }
  lines.push([start, n]);
  return lines;
}

const block = (n: number, size: number, lh: number) => (n ? (n - 1) * size * lh + size : 0);

export interface Lay {
  size: number;
  lines: [number, number][];
  over: boolean;
}

/** Size (auto-shrink on the complete text) and lines, mirrors layout(). */
export function layoutTw(c: K, g: string[], size0: number, w: number, h: number, m: (s: string, size: number) => number): Lay {
  const at = (size: number): [boolean, [number, number][]] => {
    const l = wrapG(g, w, (s) => m(s, size));
    return [block(l.length, size, c.lineHeight) <= h + 0.5, l];
  };
  const [ok, l] = at(size0);
  if (ok || !c.autoShrink || !g.length) return { size: size0, lines: l, over: !ok && g.length > 0 };
  const lo0 = Math.max(1, Math.min(c.minSize, size0));
  const [okLo, lLo] = at(lo0);
  if (!okLo) return { size: lo0, lines: lLo, over: true };
  let lo = lo0;
  let hi = size0;
  let best = lLo;
  for (let i = 0; i < 14; i++) {
    const mid = (lo + hi) / 2;
    const [k, ll] = at(mid);
    if (k) {
      lo = mid;
      best = ll;
    } else hi = mid;
  }
  return { size: lo, lines: best, over: false };
}

export interface Vis {
  k: number[];
  shown: number;
  block: number;
  phase: 'typing' | 'hold' | 'erasing' | 'done' | 'idle';
}

/** `hold`: the pause between two texts, s (the text stays typed that long). */
export function visible(c: K, tl: Timeline, e: number, next: boolean, idle: boolean, hold = c.hold): Vis {
  const n = tl.g.length;
  const k = new Array<number>(n).fill(0);
  if (idle) return { k, shown: 0, block: 1, phase: 'idle' };
  const holdEnd = tl.typeEnd + Math.max(0, hold);
  if (e < tl.typeEnd || !next || e < holdEnd) {
    let shown = 0;
    for (let i = 0; i < n; i++) {
      const dt = e - tl.t[tl.unitOf[i]];
      k[i] = tl.appear > 0 ? clamp(dt / tl.appear, 0, 1) : dt >= 0 ? 1 : 0;
      if (k[i] > 0) shown = i + 1;
    }
    return { k, shown, block: 1, phase: e < tl.typeEnd ? 'typing' : next ? 'hold' : 'done' };
  }
  const ee = e - holdEnd;
  if (c.erase === 'block') return { k: k.fill(1), shown: n, block: clamp(1 - ee / Math.max(1e-6, tl.eraseDur), 0, 1), phase: 'erasing' };
  const left = Math.max(0, n - Math.floor(ee * Math.max(1, c.eraseRate)));
  for (let i = 0; i < left; i++) k[i] = 1;
  return { k, shown: left, block: 1, phase: 'erasing' };
}

/** The texts of the chain (fields filled in, case changed). */
export function textsOf(l: TwLayer, data: Data): string[] {
  const list = [l.text, ...(l.more ?? []).map((m) => m.text)].map((t) => interpolateText(t ?? '', data));
  return l.uppercase ? list.map((t) => t.toUpperCase()) : list;
}

/** Everything the picture needs at `nowMs` / `age`. */
export function runOf(l: TwLayer, st: State, data: Data, size0: number, ls: number, w: number, h: number, nowMs: number, age: number) {
  const c = cfg(l);
  const list = textsOf(l, data);
  const n = Math.max(1, list.length);
  const font = (size: number) => fontCss(l.fontFamily, l.fontWeight, size);
  const m = (s: string, size: number) => measure(font(size), ls, s);
  const [px, py] = c.plate ? [48, 20] : [0, 0];
  const bw = Math.max(10, w - 2 * px);
  const bh = Math.max(10, h - 2 * py);
  const lays = new Map<number, [string[], Lay]>();
  const layOf = (i: number): [string[], Lay] => {
    let v = lays.get(i);
    if (!v) {
      const g = graphemes(list[i] ?? '');
      v = [g, layoutTw(c, g, size0, bw, bh, m)];
      lays.set(i, v);
    }
    return v;
  };
  const tls = new Map<number, Timeline>();
  const tlOf = (i: number): Timeline => {
    let t = tls.get(i);
    if (!t) {
      let lines: number[] | undefined;
      if (c.unit === 'line') {
        const [g, lay] = layOf(i);
        lines = new Array<number>(g.length).fill(0);
        lay.lines.forEach(([a, b], li) => {
          for (let j = a; j < b; j++) lines![j] = li;
        });
      }
      t = timeline(c, l.speed, list[i] ?? '', i, lines);
      tls.set(i, t);
    }
    return t;
  };
  const idle = st.mode === 'idle';
  const e = elapsed(st, nowMs, age);
  const [idx, el] = idle ? [Math.min(st.idx ?? 0, n - 1), 0] : nowAt(c, st, n, e, tlOf);
  const tl = tlOf(idx);
  const ch = chainOf(c, st);
  // The text erases itself only when the next one comes by itself (chained by time).
  const next = autoNext(ch, n, idx);
  const vis = visible(c, tl, el, next, idle, ch.hold);
  const [g, lay] = layOf(idx);
  return { c, list, n, idx, el, tl, vis, g, lay, px, py, bw, bh, font, m, next, ch, more: hasNext(c, n, idx) };
}

// --------------------------------------------------------------- render --

function Render({ layer, r, w, h, data, time, state }: { layer: TwLayer; r: import('../engine/evaluate').Resolved; w: number; h: number; data: Data; time: { nowMs: number; ageSecs: number }; state: unknown }) {
  const st = (state ?? {}) as State;
  const size0 = num(r.fontSize);
  const ls = num(r.letterSpacing);
  const R = runOf(layer, st, data, size0, ls, w, h, time.nowMs, time.ageSecs);
  const { c, g, vis, px, py, bw, bh, lay, m } = R;
  // The layout is always the complete text's (frozen): no word jumps while typing.
  const size = lay.size;
  const lineH = size * c.lineHeight;
  const nl = lay.lines.length;
  const top = py + (layer.verticalAlign === 'top' ? 0 : layer.verticalAlign === 'bottom' ? bh - lineH * nl : (bh - lineH * nl) / 2);
  const fill = cssColor(col(r.fill));
  const font = R.font(size);
  let cursor = null as [number, number] | null; // x, line top
  const afterBreak = vis.shown === 0 || g[vis.shown - 1]?.includes('\n');
  const lines = lay.lines.map(([a, b0], li) => {
    const b = Math.min(b0, g.length);
    const idxs: number[] = [];
    for (let i = a; i < b; i++) if (!g[i].includes('\n') && !g[i].includes('\r')) idxs.push(i);
    const text = idxs.map((i) => g[i]).join('');
    const wt = m(text.trimEnd(), size);
    const x0 = px + (layer.align === 'center' ? (bw - wt) / 2 : layer.align === 'right' ? bw - wt : 0);
    const y = top + li * lineH;
    if (afterBreak && (vis.shown === a || (li === 0 && vis.shown === 0))) cursor = [x0, y];
    if (!afterBreak && vis.shown - 1 >= a && vis.shown - 1 < b) cursor = [x0 + m(g.slice(a, vis.shown).join('').replace(/\n/g, ''), size), y];
    // Graphemes in runs of the same opacity (what is not typed yet is not drawn).
    const runs: { k: number; s: string }[] = [];
    for (const i of idxs) {
      const k = vis.k[i] ?? 0;
      if (k <= 0) continue;
      const last = runs[runs.length - 1];
      if (last && last.k === k) last.s += g[i];
      else runs.push({ k, s: g[i] });
    }
    return (
      <div key={li} style={{ position: 'absolute', left: x0, top: y, height: lineH, lineHeight: `${lineH}px`, whiteSpace: 'pre', font, letterSpacing: ls, fontKerning: 'normal' }}>
        {runs.map((run, j) =>
          run.k >= 1 ? (
            run.s
          ) : (
            <span key={j} style={{ opacity: run.k, position: 'relative', top: c.appear === 'slide' ? (1 - run.k) * 0.45 * size : 0 }}>
              {run.s}
            </span>
          ),
        )}
      </div>
    );
  });
  // Cursor.
  const done = vis.phase === 'done';
  let cur: CSSProperties | null = null;
  if (layer.cursor && !(done && c.cursorHide && g.length)) {
    const typing = vis.phase === 'typing' || vis.phase === 'erasing';
    const on = !c.cursorBlink || typing || Math.floor(time.nowMs / 500) % 2 === 0;
    const [cx, ly] = cursor ?? [px, top];
    // Glyph box top inside the line box (as the Rust renderer: baseline − ascent).
    const gt = ly + (lineH - size * 1.21) / 2;
    const [x, y, cw, ch] = c.cursorCh === '█' ? [cx, gt + 0.1 * size, 0.56 * size, 1.02 * size] : c.cursorCh === '_' ? [cx, gt + size, 0.56 * size, 0.11 * size] : [cx + 0.03 * size, gt + 0.08 * size, 0.09 * size, 1.08 * size];
    if (on) cur = { position: 'absolute', left: x, top: y, width: cw, height: ch, background: c.cursorColor.trim() ? hex(c.cursorColor) : fill };
  }
  return (
    <>
      {c.plate && <div style={{ position: 'absolute', inset: 0, background: hex(c.plateColor), boxShadow: c.plateAccent.trim() ? `inset 10px 0 ${hex(c.plateAccent)}` : undefined, borderRadius: 4 }} />}
      <div style={{ position: 'absolute', inset: 0, overflow: 'hidden', opacity: vis.block, color: fill, textShadow: c.shadow ? '0 3px 10px #00000088' : undefined }}>
        {lines}
        {cur && <div style={cur} />}
      </div>
    </>
  );
}

// ------------------------------------------------------------- operator --

type OpP = { layer: TwLayer; state: unknown; data: Data; act: (op: string, args?: unknown) => void; item?: unknown; time: { nowMs: number; ageSecs: number; held?: boolean; fps?: number } };

/** Where the typing is now, for the centre (time on air advancing between two updates). */
function useTw({ layer, state, data, time }: OpP) {
  const { now, age } = useLiveTime(time);
  const st = (state ?? {}) as State;
  const R = runOf(layer, st, data, num(layer.props.fontSize ?? 64), num(layer.props.letterSpacing ?? 0), num(layer.props.width ?? 800), num(layer.props.height ?? 200), now, age);
  return { st, R };
}

/** The next text waits for Continue (F2): chained on it, the text shown fully typed, not the last one (mirrors waits_continue). */
const waitsNext = (R: ReturnType<typeof runOf>, st: State) => st.mode !== 'idle' && R.n > 1 && R.ch.auto && R.ch.on === 'continue' && R.idx + 1 < R.n && R.el >= R.tl.typeEnd - 1e-9;

/**
 * The control centre of the typewriter (web/src/widgets/centre.tsx). Line 1: typed / total
 * characters, the text shown, what it waits for (its start, the next text). Line 2: play /
 * pause, next text, show all, clear. Line 3: how it starts (Designer), how the texts are
 * chained (both modes, on the air only in the Operator), the speed (exposable).
 */
function TwBar(p: OpP) {
  const { layer, act } = p;
  const item = p.item as PlayoutItem | undefined;
  const mode = useCentreMode();
  const b = useBound();
  const designer = mode === 'designer';
  const time = layerTime(layer, item, p.time, ((p.state as State | undefined)?.mode ?? 'auto') !== 'auto');
  const { st, R } = useTw({ ...p, time });
  const md = st.mode ?? 'auto';
  const n = R.n;
  const several = n > 1;
  // Held by the preview (or waiting for its start on air), nothing typed yet: it waits for its Play.
  const running = (md === 'auto' && !time.held) || md === 'run';
  const typedAll = md !== 'idle' && R.el >= R.tl.typeEnd - 1e-9;
  const cur = { cur: R.idx };
  const clearMsg = item ? t('{name} is on air: clear the screen?', { name: layer.name }) : undefined;
  const clear = () => act(several ? 'reset' : 'clear', cur);
  const clearKey = async () => {
    if (clearMsg && !(await confirmDialog({ title: clearMsg, confirm: t('Yes') }))) return;
    clear();
  };
  const play = () => act(md === 'paused' ? 'resume' : 'start', cur);
  // The Designer edits the template (what the take starts with); the Operator the air only.
  const c = R.c;
  const ch: Chain = designer ? { auto: c.autoNext, on: c.nextOn, hold: c.hold, trigger: c.nextTrigger, looping: c.chainLoop } : R.ch;
  const setChained = (v: boolean) => (designer ? b.set('autoNext', v) : act(v ? 'advance' : 'stop', v ? { on: ch.on, seconds: ch.hold, trigger: ch.trigger, ...cur } : cur));
  const setOn = (v: AdvanceMode) => (designer ? b.set('nextOn', v) : act('advance', { on: v, seconds: ch.hold, trigger: ch.trigger, ...cur }));
  const setSecs = (v: number) => (designer ? b.set('hold', v) : act('advance', { seconds: v, ...cur }));
  const setTrigger = (v: string) => (designer ? b.set('nextTrigger', v) : act('advance', { on: 'trigger', trigger: v, ...cur }));
  // Confirmations only on air (a key does as its button). Enter and Space stay the Operator's TAKE and CONTINUE.
  useKeys({
    s: () => act('start', cur),
    p: () => (running ? act('pause', cur) : play()),
    x: () => void clearKey(),
    ArrowRight: () => several && act('next', cur),
  });
  // Line 1: its start on air, else the next text (the countdown of its pause, » F2, ⚡ name).
  const waitsStart = !!layer.manualStart && !!item && !item.clocks?.[layer.id] && md === 'auto';
  const holdLeft = R.tl.typeEnd + ch.hold - R.el;
  const status = waitsStart ? (
    layer.startOn === 'trigger' ? (
      <WaitsTrigger name={layer.startTrigger ?? ''} />
    ) : (
      <WaitsContinue />
    )
  ) : !several || !ch.auto ? undefined : ch.on === 'timed' ? (
    running && typedAll && R.next && holdLeft > 0 ? (
      <Countdown left={holdLeft} total={ch.hold} />
    ) : undefined
  ) : ch.on === 'continue' ? (
    waitsNext(R, st) ? (
      <WaitsContinue />
    ) : undefined
  ) : (
    <WaitsTrigger name={ch.trigger} />
  );
  const actions: Line2Item[] = [
    running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: () => act('pause', cur) } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: play },
    several && { icon: SkipForward, label: t('Next text'), key: '→', onClick: () => act('next', cur) },
    { icon: ChevronsRight, label: t('Show all'), disabled: md !== 'paused' && R.vis.phase !== 'typing' && md !== 'idle', onClick: () => act('showAll', cur) },
    GAP,
    { icon: Trash2, label: tc('cc', 'Clear'), key: 'X', tone: 'red', confirm: clearMsg, onClick: clear },
  ];
  const unitName = c.unit === 'char' ? t('char/s') : c.unit === 'word' ? t('words/s') : t('lines/s');
  return (
    <Centre name={layer.name} readout={{ v: R.vis.shown, of: R.g.length, sub: several ? t('Text {n}/{total}', { n: R.idx + 1, total: n }) : undefined }} status={status} actions={actions}>
      <StartRow layer={designer ? (b.obj as TwLayer) : layer} />
      {several && (
        <>
          <Toggle label={t('Chain the texts')} value={ch.auto} onChange={setChained} />
          {ch.auto && <Seg label={t('Next text')} value={ch.on} onChange={setOn} options={ADVANCE_MODES()} />}
          {ch.auto && ch.on === 'timed' && <RangeNum label={t('Pause between texts')} value={ch.hold} min={0.5} max={20} step={0.5} precision={1} unit="s" onRelease onChange={setSecs} />}
          {ch.auto && ch.on === 'trigger' && <Text label={tc('cc', 'Trigger name')} value={ch.trigger} onChange={setTrigger} />}
        </>
      )}
      {c.speedMode === 'rate' ? <RangeNum label={t('Speed')} path="speed" value={layer.speed} min={1} max={80} unit={unitName} onRelease /> : <RangeNum label={t('Duration')} path="duration" value={c.duration} min={0.5} max={20} step={0.5} precision={1} unit="s" onRelease />}
    </Centre>
  );
}

/** The text shown changed while typing: restart or continue (setting « If edited »).
 *  Mounted by the Operator only, so the action is sent once. */
function EditedWatch(p: OpP) {
  const { act } = p;
  const { st, R } = useTw(p);
  const mode = st.mode ?? 'auto';
  const text = R.list[R.idx] ?? '';
  // Held by the preview (nothing typed yet): an edit changes nothing, and an action would release it.
  const held = !!p.time.held;
  const seen = useRef<{ idx: number; text: string; shown: number } | null>(null);
  useEffect(() => {
    const s = seen.current;
    if (s && !held && s.idx === R.idx && s.text !== text && (mode === 'run' || mode === 'paused' || mode === 'auto') && R.vis.phase === 'typing') act('edited', { cur: R.idx, typed: s.shown });
    seen.current = { idx: R.idx, text, shown: R.vis.shown };
  });
  return null;
}

// ------------------------------------------------------------ inspector --

function Inspector({ layer, edit }: { layer: TwLayer; edit: (fn: (l: TwLayer) => void, key?: string) => void }) {
  const c = cfg(layer);
  const set = (key: string, v: unknown) => edit((l) => void ((l as unknown as Record<string, unknown>)[key] = v));
  const unitName = c.unit === 'char' ? t('char/s') : c.unit === 'word' ? t('words/s') : t('lines/s');
  return (
    <>
      <Section id="tw-texts" title={t('Texts')}>
        <Content path="text" value="" multiline placeholder={t('Text')} />
        <ListEditor label={t('Then')} path="more" value={c.more} fields={[{ key: 'text', placeholder: t('Next text') }]} add={() => ({ text: 'New text' })} addLabel={t('Text')} />
      </Section>
      <Section id="tw-style" title={t('Style')}>
        <div className="pp-typo">
          <FontControls layer={layer as Layer & FontStyle} set={set} />
          <GroupRow layer={layer as Layer} spec={{ label: t('Size'), props: ['fontSize'], min: 1, suffix: 'px' }} />
          <GroupRow layer={layer as Layer} spec={{ label: t('Spacing'), props: ['letterSpacing'], precision: 1, suffix: 'px' }} />
          <ColorGroupRow layer={layer as Layer} prop="fill" label={t('Color')} />
        </div>
        <Num label={t('Line height')} path="lineHeight" value={c.lineHeight} min={0.9} max={2} step={0.05} precision={2} unit="×" />
        <Toggle label={t('Auto-shrink')} path="autoShrink" value={false} />
        {c.autoShrink && <Num label={t('Min size')} path="minSize" value={c.minSize} min={8} max={Math.max(8, num(layer.props.fontSize ?? 64))} unit="px" />}
        <Toggle label={t('Plate')} path="plate" value={false} />
        {c.plate && (
          <>
            <ColorPick label={t('Plate color')} path="plateColor" value={D.plateColor} />
            <ColorPick label={t('Accent colour')} path="plateAccent" value={D.plateAccent} none={t('None')} first={D.plateAccent} />
          </>
        )}
        <Toggle label={t('Shadow')} path="shadow" value={false} />
      </Section>
      <Section id="tw-cursor" title={t('Cursor')}>
        <Toggle label={t('Cursor')} path="cursor" value={true} />
        {layer.cursor && (
          <>
            <Seg
              label={t('Shape')}
              path="cursorCh"
              value={c.cursorCh}
              options={[
                ['|', '|'],
                ['_', '_'],
                ['█', '█'],
              ]}
            />
            <ColorPick label={t('Color')} path="cursorColor" value="" none={t('Text colour')} />
            <Toggle label={t('Blink')} path="cursorBlink" value={true} />
            <Toggle label={t('Hide at end')} path="cursorHide" value={false} />
          </>
        )}
      </Section>
      <Section id="tw-typing" title={t('Typing')}>
        <Seg
          label={t('Unit')}
          path="unit"
          value={c.unit}
          options={[
            ['char', t('Character')],
            ['word', t('Word')],
            ['line', t('Line')],
          ]}
        />
        <Seg
          label={t('Speed in')}
          path="speedMode"
          value={c.speedMode}
          options={[
            ['rate', t('Units / s')],
            ['duration', t('Duration')],
          ]}
        />
        {c.speedMode === 'rate' ? (
          <>
            <Num label={t('Speed')} path="speed" value={layer.speed} min={1} max={80} unit={unitName} />
            <Num label={t('Max duration')} path="maxDuration" value={c.maxDuration} min={0} max={60} step={0.5} precision={1} unit="s" />
          </>
        ) : (
          <Num label={t('Duration')} path="duration" value={c.duration} min={0.5} max={20} step={0.5} precision={1} unit="s" />
        )}
        <Num label={t('Variation')} path="jitter" value={c.jitter} min={0} max={80} unit="%" />
        <Num label={t('Punctuation')} path="punctPause" value={c.punctPause} min={0} max={1200} step={20} unit="ms" />
        <Seg
          label={t('Each unit')}
          path="appear"
          value={c.appear}
          options={[
            ['instant', t('Instant')],
            ['fade', t('Fade')],
            ['slide', t('Slide up')],
          ]}
        />
        {c.appear !== 'instant' && <Num label={c.appear === 'slide' ? t('Slide time') : t('Fade time')} path="appearMs" value={c.appearMs} min={40} max={800} step={10} unit="ms" />}
        <Seg
          label={t('If edited')}
          path="onEdit"
          value={c.onEdit}
          options={[
            ['restart', t('Restart')],
            ['continue', t('Continue')],
          ]}
        />
      </Section>
      {c.more.length > 0 && (
        <Section id="tw-chain" title={t('Chaining')}>
          <Seg
            label={t('Erase')}
            path="erase"
            value={c.erase}
            options={[
              ['backspace', t('Backspace')],
              ['block', t('All at once')],
            ]}
          />
          {c.erase === 'backspace' && <Num label={t('Erase speed')} path="eraseRate" value={c.eraseRate} min={5} max={200} unit={t('char/s')} />}
          <Toggle label={t('Loop')} path="chainLoop" value={false} />
        </Section>
      )}
    </>
  );
}

export default defineWidget({
  id: 'typewriter',
  type: 'typewriter',
  label: 'Typewriter',
  group: 'Data',
  icon: Keyboard,
  create: () => ({
    text: 'Le Parlement adopte la réforme après quatorze heures de débat.',
    speed: 32,
    cursor: true,
    fontFamily: 'Inter',
    fontWeight: 700,
    align: 'left',
    verticalAlign: 'middle',
    uppercase: false,
    props: { width: 1360, height: 200, fontSize: 64, fill: [1, 1, 1, 1] },
  }),
  ownsLook: true,
  live: () => true,
  content: ['text'],
  Render: ({ layer, r, w, h, data, time, state }) => <Render layer={layer} r={r} w={w} h={h} data={data} time={time} state={state} />,
  Inspector: ({ layer, edit }) => <Inspector layer={layer} edit={edit} />,
  Operator: ({ layer, state, data, act, time, item }) => (
    <>
      <EditedWatch layer={layer} state={state} data={data} act={act} time={time} item={item} />
      <TwBar layer={layer} state={state} data={data} act={act} time={time} item={item} />
    </>
  ),
  // Chained on Continue: F2 brings the next text once the one shown is typed (until the last one).
  waits: (l, state, data, item) => {
    const st = (state ?? {}) as State;
    if (textsOf(l, data).length < 2 || !chainOf(cfg(l), st).auto) return false;
    const R = runOf(l, st, data, num(l.props.fontSize ?? 64), num(l.props.letterSpacing ?? 0), num(l.props.width ?? 800), num(l.props.height ?? 200), Date.now(), layerAgeFrames(item, item.age ?? 0, l) / 50);
    return waitsNext(R, st);
  },
  // The Operator's preview holds it until its Start: nothing typed yet.
  plays: () => 'start',
  warnings: (layer, { data, state, r }) => {
    const out: Warning[] = [];
    const list = textsOf(layer, data);
    if (!list.some((s) => s.trim())) out.push({ level: 'info', text: t('Empty text: only the cursor is shown.'), op: true });
    const size0 = num(r?.fontSize ?? layer.props.fontSize ?? 64);
    const R = runOf(layer, (state ?? {}) as State, data, size0, num(r?.letterSpacing ?? layer.props.letterSpacing ?? 0), num(r?.width ?? layer.props.width ?? 800), num(r?.height ?? layer.props.height ?? 200), Date.now(), 0);
    if (R.lay.size < size0 - 0.5) out.push({ level: 'info', text: t('Auto-shrink: {size} px, computed on the complete text before typing (the size never changes while typing).', { size: Math.round(R.lay.size) }) });
    if (R.lay.over) out.push({ level: 'warn', text: t('The complete text does not fit the box even at {size} px.', { size: Math.round(R.lay.size) }), op: true });
    return out;
  },
});

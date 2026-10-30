// Ticker: crawl (horizontal, continuous), roll (vertical, line by line) or flip (one item at a time), separators
// text / symbol / picture, categories, title block, edge fade, end of list. The queue lives in the operator's
// state (seeded from the layer's items); its edits apply at the next pass, without a jump.
// Same picture as crates/cg-render/src/wid/ticker.rs; tape logic mirrors crates/cg-core/src/wid/ticker.rs.
import { memo, useMemo, useState } from 'react';
import { ArrowLeftRight, Pause, Play, RefreshCw, SkipBack, SkipForward } from 'lucide-react';
import { cssColor, col, interpolateText, num } from '../engine/evaluate';
import type { Data, FontStyle, PlayoutItem } from '../engine/types';
import { frameSpeed, layerAgeFrames } from '../engine/widgets';
import { GroupRow, ColorGroupRow, AssetPicker } from '../designer/Inspector';
import { FontPick } from '../ui/FontPick';
import { useFonts } from '../engine/fonts';
import { Btns, clamp, ColorPick, findCol, headerRows, hex as cssBg, Content, defineWidget, ease, ListEditor, Num, RangeNum, Row, Section, Seg, Text, Toggle, useBound, useCentreMode, useFrameValue, useKeys, type Warning } from './kit';
import { Centre, GAP, StartRow, WaitsContinue, WaitsTrigger, type Line2Item } from './centre';
import { FitLine, FONT_CSS, tw, useLiveClock } from './table-text';
import { t, tc } from '../i18n';

export interface Item {
  cat?: string;
  text?: string;
  on?: boolean;
  /** The layer's itemFormat (a list's elements as text). */
  format?: string;
}
export interface Cat {
  name?: string;
  bg?: string;
  color?: string;
  text?: string;
}
export interface Config {
  mode?: 'crawl' | 'roll' | 'flip';
  dir?: 'left' | 'right';
  rollDir?: 'up' | 'down';
  itemDur?: number;
  transDur?: number;
  trans?: 'slide' | 'push' | 'fade' | 'flip' | 'type';
  items?: Item[];
  sepType?: 'text' | 'symbol' | 'image' | 'none';
  sepSymbol?: string;
  sepImage?: string;
  sepColor?: string;
  cats?: Cat[];
  title?: { on?: boolean; text?: string; bg?: string; color?: string; w?: number };
  barBg?: string;
  spacing?: number;
  fade?: boolean;
  fadeW?: number;
  end?: 'loop' | 'passes' | 'stop';
  passes?: number;
  emptyMode?: 'message' | 'hide';
  emptyText?: string;
  startFull?: boolean;
  /** The elements of a linked list (JSON objects, a data set's rows) as text: `{name} · {text}` takes their keys (empty = their text). */
  itemFormat?: string;
}
interface Run {
  at: number;
  items: Item[];
  prio?: Item | null;
  kind?: 'pass' | 'item' | 'cut';
}
/** Mirrors `State` of the Rust module. */
export interface State {
  started?: boolean;
  running?: boolean;
  base?: number;
  anchor?: number;
  speed?: number | null;
  queue?: Item[] | null;
  runs?: Run[];
}

type TL = Config & FontStyle & { id: string; text: string; separator: string; speed: number; props: Record<string, unknown>; manualStart?: boolean; startOn?: 'continue' | 'trigger'; startTrigger?: string; name?: string };
const PAD = 30;

/**
 * The time of a widget's control centre on air: the layer's own time on air (its clock, 0 while
 * a manual start waits for its Continue / trigger), still while it waits or its clock is paused.
 * Once the widget runs on its own state (`started`), the time on air no longer matters.
 */
export function layerTime<T extends { nowMs: number; ageSecs: number; held?: boolean; fps?: number }>(l: { id: string; manualStart?: boolean }, item: PlayoutItem | undefined, time: T, started: boolean): T {
  if (!item || started) return time;
  const c = item.clocks?.[l.id];
  const still = c ? c.pausedAt != null : !!l.manualStart;
  return { ...time, ageSecs: layerAgeFrames(item, item.age ?? 0, l as never) / (time.fps ?? 50), held: time.held || still };
}

export function cfg(c: Config) {
  return {
    mode: c.mode ?? 'crawl',
    dir: c.dir ?? 'left',
    rollDir: c.rollDir ?? 'up',
    itemDur: c.itemDur ?? 5,
    transDur: c.transDur ?? 0.6,
    trans: c.trans ?? 'slide',
    items: c.items ?? [],
    sepType: c.sepType ?? 'text',
    sepSymbol: c.sepSymbol ?? '●',
    sepImage: c.sepImage ?? '',
    sepColor: c.sepColor ?? '',
    cats: (c.cats ?? []).map((x) => ({ name: 'NEW', bg: '#7c5cff', color: '#ffffff', text: '', ...x })),
    title: { on: false, text: 'INFO', bg: '#ff3b5c', color: '#ffffff', w: 190, ...c.title },
    barBg: c.barBg ?? '',
    spacing: c.spacing ?? 0,
    fade: c.fade ?? false,
    fadeW: c.fadeW ?? 90,
    end: c.end ?? 'loop',
    passes: c.passes ?? 2,
    emptyMode: c.emptyMode ?? 'message',
    emptyText: c.emptyText ?? '',
    startFull: c.startFull ?? false,
    itemFormat: c.itemFormat ?? '',
  };
}
type K = ReturnType<typeof cfg>;

// ----------------------------------------------------- tape (mirror) --
const it0 = (i: Item) => ({ cat: i.cat ?? '', text: i.text ?? '', on: i.on !== false, format: i.format ?? '' });
export function layerItems(l: TL): Item[] {
  const fmt = (i: Item) => ({ ...i, format: l.itemFormat ?? '' });
  if (l.items?.length) return l.items.map(it0).map(fmt);
  return String(l.text ?? '')
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean)
    .map((text) => fmt({ cat: '', text, on: true }));
}
/** `{key}` of a format replaced by an element's values (mirrors `formatted`). */
export function formatted(format: string, get: (k: string) => string | undefined): string {
  return format.replace(/\{([^}]*)\}/g, (_, k: string) => get(k.trim()) ?? '').trim();
}
/** A JSON value as text (mirrors datapath::text). */
const jsonText = (v: unknown) => (typeof v === 'string' ? v : v == null ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v));
export function expand(items: Item[], data: Data): Item[] {
  const out: Item[] = [];
  for (const raw of items) {
    const it = it0(raw);
    if (!it.on) continue;
    const t = interpolateText(it.text, data);
    if (t.trim().startsWith('[')) {
      try {
        const a = JSON.parse(t.trim());
        // A data set's table: its text (or first) column, its category column.
        const tb = Array.isArray(a) ? headerRows(a) : null;
        if (tb) {
          const ti = Math.max(0, findCol(tb.head, ['text', 'title', 'headline']));
          const ci = findCol(tb.head, ['cat', 'category']);
          for (const r of tb.rows) {
            const text = it.format ? formatted(it.format, (k) => r[findCol(tb.head, [k.toLowerCase()])]) : (r[ti] ?? '').trim();
            if (text) out.push({ cat: ci >= 0 ? (r[ci] ?? '') : it.cat, text, on: true });
          }
          continue;
        }
        if (Array.isArray(a)) {
          for (const v of a) {
            const text = typeof v === 'string' ? v : v && typeof v === 'object' ? (it.format ? formatted(it.format, (k) => (k in v ? jsonText(v[k]) : undefined)) : String(v.text ?? v.title ?? '')) : JSON.stringify(v);
            const cat = v && typeof v === 'object' && typeof (v.cat ?? v.category) === 'string' ? (v.cat ?? v.category) : it.cat;
            if (text.trim()) out.push({ cat, text: text.trim(), on: true });
          }
          continue;
        }
      } catch {
        /* lines */
      }
    }
    t.split('\n')
      .map((s) => s.trim())
      .filter(Boolean)
      .forEach((text) => out.push({ cat: it.cat, text, on: true }));
  }
  return out;
}
/** Readable speeds, px/s: the same range in the Designer and on air (mirrors SPEED_MIN / SPEED_MAX). */
export const SPEED: [number, number] = [10, 800];
const speedOf = (l: TL, st: State) => clamp(st.speed ?? l.speed ?? 120, SPEED[0], SPEED[1]);
export const periodOf = (l: TL, k: K, st: State, h: number) => (k.mode === 'flip' ? Math.max(0.5, k.itemDur) : Math.max(0, k.itemDur) + h / Math.max(5, speedOf(l, st)));
export function clockOf(l: TL, k: K, st: State, now: number, age: number, fps?: number) {
  const rate = k.mode === 'crawl' ? frameSpeed(speedOf(l, st), fps) : 1;
  if (!st.started) return rate * Math.max(0, age);
  return (st.base ?? 0) + (st.running ? (rate * Math.max(0, now - (st.anchor ?? 0))) / 1000 : 0);
}
export const queueOf = (l: TL, st: State) => (st.queue ?? layerItems(l)).map(it0);
const runsOf = (l: TL, st: State): Run[] => (st.runs?.length ? st.runs : [{ at: 0, items: queueOf(l, st), prio: null, kind: 'pass' }]);
const limitOf = (k: K) => (k.end === 'loop' ? undefined : k.end === 'stop' ? 1 : Math.max(1, k.passes));

interface Placed {
  start: number;
  lead: [Item, number][];
  items: [Item, number][];
  leadW: number;
  cycle: number;
  passesBefore: number;
  leadPass: boolean;
  end: number;
}
const sum = (a: [Item, number][]) => a.reduce((s, x) => s + x[1], 0);
/** Mirrors resolve(). */
export function resolve(runs: Run[], data: Data, wf: (i: Item) => number, z: number, first: number, limit?: number): Placed[] {
  const out: Placed[] = [];
  const place = (items: Item[]) => expand(items, data).map((i) => [i, Math.max(1e-3, wf(i))] as [Item, number]);
  for (const run of runs) {
    const kind = run.kind ?? 'pass';
    const items = place(run.items ?? []);
    const cycle = sum(items);
    let start: number;
    let lead: [Item, number][];
    let passesBefore: number;
    let leadPass: boolean;
    const r = out[out.length - 1];
    if (!r) {
      start = first;
      lead = run.prio ? place([run.prio]) : [];
      passesBefore = 0;
      leadPass = false;
    } else {
      const t = run.at + z;
      const lp = r.leadPass ? 1 : 0;
      let b: number;
      let rest: [Item, number][] = [];
      let done = 0;
      if (t <= r.start) {
        b = r.start;
        if (kind === 'item') rest = r.lead.slice();
      } else if (t <= r.start + r.leadW || r.cycle <= 0) {
        if (kind === 'pass' || (r.cycle <= 0 && t > r.start + r.leadW)) {
          b = r.cycle <= 0 ? Math.max(t, r.start + r.leadW) : r.start + r.leadW;
          done = lp;
        } else {
          let x = r.start;
          let idx = r.lead.length;
          for (let i = 0; i < r.lead.length; i++) {
            if (x >= t - 1e-9) {
              idx = i;
              break;
            }
            x += r.lead[i][1];
          }
          b = x;
          if (kind === 'item') rest = r.lead.slice(idx);
          done = idx === r.lead.length ? lp : 0;
        }
      } else {
        const u = t - r.start - r.leadW;
        const n = Math.floor(u / r.cycle);
        const base = r.start + r.leadW + n * r.cycle;
        if (kind === 'pass') {
          const n2 = base >= t - 1e-9 ? n : n + 1;
          b = r.start + r.leadW + n2 * r.cycle;
          done = lp + n2;
        } else {
          let x = base;
          let idx = r.items.length;
          for (let i = 0; i < r.items.length; i++) {
            if (x >= t - 1e-9) {
              idx = i;
              break;
            }
            x += r.items[i][1];
          }
          b = x;
          if (idx === r.items.length) done = lp + n + 1;
          else {
            if (kind === 'item') rest = r.items.slice(idx);
            done = lp + n;
          }
        }
      }
      lead = run.prio ? place([run.prio]) : [];
      leadPass = rest.length > 0;
      lead.push(...rest);
      if (b > r.end) {
        start = Infinity;
        passesBefore = r.passesBefore + lp;
      } else {
        start = b;
        passesBefore = r.passesBefore + done;
      }
    }
    if (r) r.end = Math.min(r.end, start);
    if (!Number.isFinite(start)) break;
    const leadW = sum(lead);
    let end = Infinity;
    if (limit !== undefined) {
      const dn = passesBefore + (leadPass ? 1 : 0);
      end = dn >= limit || cycle <= 0 ? start + leadW : start + leadW + (limit - dn) * cycle;
    } else if (cycle <= 0) end = start + leadW;
    out.push({ start, lead, items, leadW, cycle, passesBefore, leadPass, end });
  }
  return out;
}
export function segs(placed: Placed[], lo: number, hi: number): { item: Item; x: number; w: number }[] {
  const out: { item: Item; x: number; w: number }[] = [];
  for (const p of placed) {
    if (p.start >= hi || p.end <= lo) continue;
    const stop = Math.min(p.end, hi);
    let x = p.start;
    for (const [item, w] of p.lead) {
      if (x >= stop) break;
      if (x + w > lo) out.push({ item, x, w });
      x += w;
    }
    if (p.cycle <= 0 || x >= stop) continue;
    x += Math.max(0, Math.floor((lo - x) / p.cycle)) * p.cycle;
    let guard = 0;
    outer: while (x < stop && guard < 10000) {
      for (const [item, w] of p.items) {
        if (x >= stop) break outer;
        if (x + w > lo) out.push({ item, x, w });
        x += w;
        guard++;
      }
    }
  }
  return out;
}
const tapeEnd = (p: Placed[]) => {
  const e = p[p.length - 1]?.end;
  return e !== undefined && Number.isFinite(e) ? e : undefined;
};
function passAt(p: Placed[], x: number) {
  for (let i = p.length - 1; i >= 0; i--) {
    const q = p[i];
    if (x >= q.start) {
      const after = x - q.start - q.leadW;
      const n = q.cycle > 0 && after > 0 ? Math.floor(after / q.cycle) : 0;
      return q.passesBefore + (q.leadPass && after > 0 ? 1 : 0) + n + 1;
    }
  }
  return 1;
}

// ------------------------------------------------------------- measure --
interface Look {
  /** CSS align-items of the text (old tickers keep their vertical alignment). */
  ai: string;
  family: string;
  weight: number;
  size: number;
  ls: number;
  upper: boolean;
  color: string;
}
const chip = (size: number) => ({ cs: Math.round(size * 0.6), cp: Math.round(size * 0.36), cg: Math.round(size * 0.45) });
const upc = (s: string, u: boolean) => (u ? s.toUpperCase() : s);
function measure(k: K, l: TL, lk: Look) {
  const catOf = (it: Item) => (it.cat ? k.cats.find((c) => c.name === it.cat) : undefined);
  const chipW = (it: Item) => {
    const c = catOf(it);
    if (!c) return 0;
    const { cs, cp, cg } = chip(lk.size);
    return tw(c.name.toUpperCase(), cs, 900) + 2 * cp + cg;
  };
  const textW = (s: string) => tw(upc(s, lk.upper), lk.size, lk.weight, lk.ls, lk.family);
  const itemW = (it: Item) => textW(it.text ?? '') + chipW(it);
  const sepW = k.sepType === 'none' ? 0 : k.sepType === 'image' ? Math.round(lk.size * 1.05) * 2.4 : k.sepType === 'symbol' ? tw(k.sepSymbol, Math.round(lk.size * 0.7), 900) : textW(l.separator ?? '');
  return { catOf, chipW, itemW, sepW };
}

// Memoised: an item sliding by keeps its props, it is not drawn again on each frame.
const ItemView = memo(function ItemView({ it, x, h, k, lk, ms }: { it: Item; x: number; h: number; k: K; lk: Look; ms: ReturnType<typeof measure> }) {
  const c = ms.catOf(it);
  const { cs, cp, cg } = chip(lk.size);
  const cw = ms.chipW(it);
  const ch = Math.round(cs * 1.7);
  const color = c?.text ? c.text : lk.color;
  return (
    <>
      {c && (
        <div style={{ position: 'absolute', left: x, top: (h - ch) / 2, width: cw - cg, height: ch, borderRadius: 6, background: c.bg, color: c.color, font: `900 ${cs}px ${FONT_CSS}`, lineHeight: `${ch}px`, paddingLeft: cp, boxSizing: 'border-box', whiteSpace: 'pre' }}>{c.name.toUpperCase()}</div>
      )}
      <div style={{ position: 'absolute', left: x + cw, top: 0, height: h, display: 'flex', alignItems: lk.ai, whiteSpace: 'pre', fontFamily: `"${lk.family}", ${FONT_CSS}`, fontWeight: lk.weight, fontSize: lk.size, letterSpacing: lk.ls, lineHeight: 1.2, color }}>{upc(it.text ?? '', lk.upper)}</div>
      {void k}
    </>
  );
});
const SepView = memo(function SepView({ x, w, h, k, l, lk }: { x: number; w: number; h: number; k: K; l: TL; lk: Look }) {
  const color = k.sepColor || lk.color;
  const box = { position: 'absolute' as const, left: x, top: 0, width: w, height: h, display: 'flex', alignItems: 'center', justifyContent: 'center', whiteSpace: 'pre' as const };
  if (k.sepType === 'none') return null;
  if (k.sepType === 'image') {
    const ih = Math.round(lk.size * 1.05);
    return <div style={box}>{k.sepImage ? <img src={k.sepImage} alt="" style={{ height: ih, display: 'block' }} /> : <div style={{ width: ih * 2.4, height: ih, borderRadius: ih * 0.24, background: '#ff3b5c' }} />}</div>;
  }
  if (k.sepType === 'symbol') return <div style={{ ...box, font: `900 ${Math.round(lk.size * 0.7)}px ${FONT_CSS}`, lineHeight: 1.2, color }}>{k.sepSymbol}</div>;
  return <div style={{ ...box, alignItems: lk.ai, fontFamily: `"${lk.family}", ${FONT_CSS}`, fontWeight: lk.weight, fontSize: lk.size, letterSpacing: lk.ls, lineHeight: 1.2, color }}>{upc(l.separator ?? '', lk.upper)}</div>;
});

function lookOf(l: TL, r: Record<string, unknown>): Look {
  return {
    ai: l.items?.length ? 'center' : l.verticalAlign === 'top' ? 'flex-start' : l.verticalAlign === 'bottom' ? 'flex-end' : 'center',
    family: l.fontFamily || 'Inter',
    weight: l.fontWeight || 800,
    size: Math.max(1, num(r.fontSize as number)),
    ls: num(r.letterSpacing as number),
    upper: !!l.uppercase,
    color: cssColor(col(r.fill as never)),
  };
}

/** What the picture is whatever the time: the measures and the tape (kept from one frame to the next). */
function sceneBase(l: TL, k: K, st: State, data: Data, lk: Look, w: number, h: number) {
  const ms = measure(k, l, lk);
  const zx = k.title.on ? Math.max(0, Math.min(k.title.w, w - 40)) : 0;
  const zw = Math.max(40, w - zx);
  const q = expand(queueOf(l, st), data);
  const crawl = k.mode === 'crawl';
  const gap = k.spacing + ms.sepW;
  const placed = crawl ? resolve(runsOf(l, st), data, (i) => ms.itemW(i) + gap, zw, k.startFull ? 0 : zw, limitOf(k)) : resolve(runsOf(l, st), data, () => 1, 0, 0, limitOf(k));
  const P = periodOf(l, k, st, h);
  return { ms, zx, zw, q, crawl, gap, placed, P, end: tapeEnd(placed) };
}

/** The picture at `now`: where the tape is. */
function sceneAt(b: ReturnType<typeof sceneBase>, l: TL, k: K, st: State, now: number, age: number, fps?: number) {
  const e = clockOf(l, k, st, now, age, fps);
  const pos = b.crawl ? e : Math.floor(e / b.P);
  const ended = b.end !== undefined && pos >= b.end;
  return { ...b, e, pos, ended, pass: passAt(b.placed, pos) };
}

/** Everything the picture and the Operator need at `now`. */
function scene(l: TL, k: K, st: State, data: Data, lk: Look, w: number, h: number, now: number, age: number, fps?: number) {
  return sceneAt(sceneBase(l, k, st, data, lk, w, h), l, k, st, now, age, fps);
}

// ------------------------------------------------------------- render --
function TickerRender({ l, r, w, h, data, time, state }: { l: TL; r: Record<string, unknown>; w: number; h: number; data: Data; time: { nowMs: number; ageSecs: number; fps?: number }; state: unknown }) {
  // Redrawn on each frame: what does not depend on the time is kept (measured again when a font loads).
  const fonts = useFonts((s) => s.ready);
  const k = useMemo(() => cfg(l), [l]);
  const st = useMemo(() => (state ?? {}) as State, [state]);
  const look = lookOf(l, r);
  const lk = useMemo(() => look, [look.ai, look.family, look.weight, look.size, look.ls, look.upper, look.color]); // eslint-disable-line react-hooks/exhaustive-deps
  const base = useMemo(() => sceneBase(l, k, st, data, lk, w, h), [l, k, st, data, lk, w, h, fonts]); // eslint-disable-line react-hooks/exhaustive-deps
  const sc = sceneAt(base, l, k, st, time.nowMs, time.ageSecs, time.fps);
  const { ms, zx, zw, e, crawl, gap, placed, P } = sc;
  const ts = Math.round(lk.size * 0.95);
  // The title does not move: the same element from one frame to the next (not drawn again).
  const title = useMemo(
    () =>
      k.title.on &&
      zx > 0 && (
        <>
          <div style={{ position: 'absolute', left: 0, top: 0, width: zx, height: h, background: cssBg(k.title.bg), clipPath: `polygon(0 0,100% 0,calc(100% - ${Math.min(22, zx)}px) 100%,0 100%)` }} />
          <FitLine text={k.title.text} size={ts} weight={900} ls={ts * 0.06} align="center" box={[0, 0, Math.max(1, zx - 16), h]} color={cssBg(k.title.color)} fit={{ overflow: 'shrink', minSize: ts * 0.5, minSqueeze: 70 }} />
        </>
      ),
    [k, zx, h, ts, fonts],
  );
  const empty = !sc.q.length;
  if (empty && k.emptyMode === 'hide') return null;
  const fw = crawl ? Math.min(k.fadeW, zw / 3) : Math.min(k.fadeW, h * 0.2);
  const mask = k.fade ? `linear-gradient(${crawl ? '90deg' : '180deg'},transparent 0,#000 ${fw}px,#000 calc(100% - ${fw}px),transparent 100%)` : undefined;
  const content: React.ReactNode[] = [];
  if (empty) {
    if (k.emptyText)
      content.push(
        <div key="empty" style={{ position: 'absolute', left: PAD, top: 0, height: h, display: 'flex', alignItems: 'center', whiteSpace: 'pre', fontFamily: `"${lk.family}", ${FONT_CSS}`, fontWeight: lk.weight, fontSize: lk.size, letterSpacing: lk.ls, color: lk.color, opacity: 0.6 }}>
          {k.emptyText}
        </div>,
      );
  } else if (crawl) {
    const right = k.dir === 'right';
    segs(placed, e, e + zw).forEach((g) => {
      const iw = ms.itemW(g.item);
      const rel = g.x - e;
      const x0 = right ? zw - rel - g.w : rel;
      content.push(
        // Keyed by its place on the tape and slid as one (a transform): on each frame only its offset changes, nothing is laid out again.
        <div key={g.x} style={{ transform: `translateX(${x0}px)` }}>
          {right ? (
            <>
              <SepView x={0} w={gap} h={h} k={k} l={l} lk={lk} />
              <ItemView it={g.item} x={gap} h={h} k={k} lk={lk} ms={ms} />
            </>
          ) : (
            <>
              <ItemView it={g.item} x={0} h={h} k={k} lk={lk} ms={ms} />
              <SepView x={iw} w={gap} h={h} k={k} l={l} lk={lk} />
            </>
          )}
        </div>,
      );
    });
  } else {
    const kk = Math.floor(e / P);
    const wk = e - kk * P;
    const at = (s: number) => segs(placed, s, s + 1).find((g) => g.x <= s + 1e-9)?.item;
    const cur = at(kk);
    const prev = kk >= 1 ? at(kk - 1) : undefined;
    const next = at(kk + 1);
    const started = !!st.started || time.ageSecs > 0;
    const scrollOf = (it: Item, wt: number) => {
      const over = ms.itemW(it) + 2 * PAD - zw;
      if (over <= 0) return 0;
      const t0 = (k.mode === 'flip' ? k.transDur : 0) + 0.7;
      const t1 = (k.mode === 'flip' ? P : k.itemDur) - 0.7;
      return -over * clamp((wt - t0) / Math.max(0.1, t1 - t0), 0, 1);
    };
    const put = (key: string, it: Item, dx: number, dy: number, op: number, sy: number, wt: number) => {
      if (op <= 0.001) return;
      content.push(
        <div key={key} style={{ position: 'absolute', left: 0, top: 0, width: zw, height: h, transform: `translate(${dx}px,${dy}px) scaleY(${Math.max(0.001, sy)})`, transformOrigin: '50% 50%', opacity: op }}>
          <ItemView it={it} x={PAD + scrollOf(it, wt)} h={h} k={k} lk={lk} ms={ms} />
        </div>,
      );
    };
    if (k.mode === 'flip' && k.trans === 'type') {
      // Typed in: the first item too, once the ticker runs (as cg-core's `typed`).
      if (cur) {
        const chars = Array.from(cur.text ?? '');
        const typing = started && wk < k.transDur && chars.length > 0;
        const n = typing ? Math.min(chars.length, Math.ceil(chars.length * Math.max(0, wk / Math.max(0.01, k.transDur)))) : chars.length;
        put('a', typing ? { ...cur, text: `${chars.slice(0, n).join('')}|` } : cur, 0, 0, 1, 1, wk);
      }
    } else if (k.mode === 'flip') {
      const inTrans = wk < k.transDur && !!prev && !!cur && started;
      const p = inTrans ? ease(wk / Math.max(0.01, k.transDur)) : 1;
      if (!inTrans) {
        if (cur) put('a', cur, 0, 0, 1, 1, wk);
      } else if (k.trans === 'fade') (put('b', prev!, 0, 0, 1 - p, 1, P), put('a', cur!, 0, 0, p, 1, wk));
      else if (k.trans === 'push') (put('b', prev!, -p * zw, 0, 1, 1, P), put('a', cur!, (1 - p) * zw, 0, 1, 1, wk));
      else if (k.trans === 'flip') {
        const c = Math.abs(Math.cos(p * Math.PI));
        if (p < 0.5) put('b', prev!, 0, 0, 1, c, P);
        else put('a', cur!, 0, 0, 1, c, wk);
      } else (put('b', prev!, 0, -p * h, 1, 1, P), put('a', cur!, 0, (1 - p) * h, 1, 1, wk));
    } else {
      const hold = k.itemDur;
      const m = wk > hold && started ? ease((wk - hold) / Math.max(0.01, P - hold)) : 0;
      const u = k.rollDir === 'down' ? -1 : 1;
      if (cur) put('a', cur, 0, -u * m * h, 1, 1, wk);
      if (next && m > 0) put('b', next, 0, u * (1 - m) * h, 1, 1, 0);
    }
  }
  return (
    <div style={{ position: 'absolute', inset: 0, overflow: 'hidden', background: k.barBg ? cssBg(k.barBg) : undefined }}>
      <div style={{ position: 'absolute', left: zx, top: 0, width: zw, height: h, overflow: 'hidden', WebkitMaskImage: mask, maskImage: mask }}>{content}</div>
      {title}
    </div>
  );
}

// ------------------------------------------------------------ operator --
type OpP = { l: TL; state: unknown; data: Data; act: (op: string, args?: unknown) => void; item?: unknown; time: { nowMs: number; ageSecs: number; held?: boolean; fps?: number } };

/** What the centre needs of the scene, followed on each frame but redrawn only when it changes: the item shown (flip / roll), their count, the end of the list. */
function useScene({ l, state, data, time }: OpP) {
  const at = useLiveClock(time);
  const k = cfg(l);
  const st = (state ?? {}) as State;
  const w = num(l.props.width as number) || 1920;
  const h = num(l.props.height as number) || 80;
  const lk = lookOf(l, { fontSize: l.props.fontSize ?? 34, letterSpacing: l.props.letterSpacing ?? 0, fill: l.props.fill ?? [1, 1, 1, 1] });
  // Held by the preview (no state yet, its time standing still): nothing to follow.
  const key = useFrameValue(() => {
    const { now, age: onAir } = at();
    const s = scene(l, k, st, data, lk, w, h, now, st.started ? 0 : onAir);
    let idx = -1;
    if (!s.crawl) {
      const kk = Math.floor(s.e / s.P);
      const cur = segs(s.placed, kk, kk + 1).find((g) => g.x <= kk + 1e-9)?.item;
      if (cur) idx = s.q.findIndex((i) => i.text === cur.text && i.cat === cur.cat);
    }
    return `${s.ended ? 1 : 0}:${idx}:${s.q.length}`;
  }, !time.held);
  const [ended, idx, count] = key.split(':').map(Number);
  return { k, st, sc: { ended: !!ended, idx, count } };
}

/** The category of an urgent item: one named URGENT, if any. */
const urgentCat = (k: K) => k.cats.find((c) => c.name.trim().toUpperCase() === 'URGENT')?.name ?? '';

/** A priority item, typed then inserted now (crawl: the next free slot, nothing on screen jumps); in the Designer (narrow panel, a ⚡ square), in the try-out. */
function Priority({ k, act }: { k: K; act: OpP['act'] }) {
  const designer = useCentreMode() === 'designer';
  const [text, setText] = useState('');
  const insert = () => {
    if (!text.trim()) return;
    act('insert', { text: text.trim(), cat: urgentCat(k), when: 'now' });
    setText('');
  };
  return (
    <Row label={tc('cc', 'Priority item')}>
      <input placeholder={t('Text')} value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => (e.stopPropagation(), e.key === 'Enter' && insert())} />
      <Btns list={[{ label: designer ? '⚡' : t('⚡ Insert'), cls: `violet ${designer ? 'sq' : ''}`, title: designer ? t('⚡ Insert') : undefined, disabled: !text.trim(), onClick: insert }]} />
    </Row>
  );
}

/** The items come from data ({{key}}: a list, a data set): Refresh reads the sources again. */
const fromData = (l: TL, k: K) => /\{\{\w+\}\}/.test([l.text ?? '', ...k.items.map((x) => x.text ?? '')].join(' '));

/**
 * The control centre of the ticker (web/src/widgets/centre.tsx). Line 1: the item shown (flip /
 * roll) or the speed (crawl), the mode, the end of the list, its start on air. Line 2: play /
 * pause, back to the start, next item, refresh (items from data). Line 3: how it starts
 * (Designer), the speed (exposable), a priority item.
 */
function TickerBar(p: OpP) {
  const { l, act } = p;
  const item = p.item as PlayoutItem | undefined;
  const mode = useCentreMode();
  const b = useBound();
  const designer = mode === 'designer';
  const time = layerTime(l, item, p.time, !!(p.state as State | undefined)?.started);
  const { k, st, sc } = useScene({ ...p, time });
  const paused = !!st.started && !st.running;
  // Manual start (or held in the preview), nothing started: it waits for the operator (or its Continue / trigger on air).
  const waiting = !st.started && !!l.manualStart && !item?.clocks?.[l.id];
  const running = !waiting && !paused && !sc.ended;
  const play = () => act(sc.ended || waiting ? 'start' : 'resume');
  // Space stays the Operator's CONTINUE.
  useKeys({
    s: () => act('start'),
    p: () => (running ? act('pause') : play()),
    ArrowRight: () => k.mode !== 'crawl' && act('next'),
    '+': () => k.mode !== 'flip' && act('speed', { delta: 20 }),
    '-': () => k.mode !== 'flip' && act('speed', { delta: -20 }),
    r: () => fromData(l, k) && act('reload'),
  });
  const modeName = k.mode === 'crawl' ? t('Crawl') : k.mode === 'roll' ? t('Roll') : t('Flip');
  const sub = sc.ended ? tc('cc', 'end of the list') : modeName;
  const readout = k.mode !== 'crawl' && sc.idx >= 0 ? { k: t('Item'), v: sc.idx + 1, of: sc.count, sub } : { v: `${Math.round(frameSpeed(speedOf(l, st), time.fps))} px/s`, sub };
  const status = waiting && item ? startChip(l) : undefined;
  const actions: Line2Item[] = [
    running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: () => act('pause') } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: play },
    { icon: SkipBack, label: tc('cc', 'To the start'), key: 'S', onClick: () => act('start') },
    k.mode !== 'crawl' && { icon: SkipForward, label: t('Next item'), key: '→', onClick: () => act('next') },
    GAP,
    fromData(l, k) && { icon: RefreshCw, label: tc('cc', 'Refresh'), key: 'R', onClick: () => act('reload') },
  ];
  return (
    <Centre name={l.name} readout={readout} status={status} actions={actions}>
      <StartRow layer={designer ? (b.obj as TL) : l} />
      {k.mode !== 'flip' && <RangeNum label={t('Speed')} path="speed" value={l.speed ?? 140} min={SPEED[0]} max={SPEED[1]} step={5} unit="px/s" onRelease />}
      <Priority k={k} act={act} />
    </Centre>
  );
}

/** Line 1 of a ticker waiting for its start on air: Continue (F2) or its trigger. */
const startChip = (l: TL) => (l.startOn === 'trigger' ? <WaitsTrigger name={l.startTrigger ?? ''} /> : <WaitsContinue />);

// ----------------------------------------------------------- inspector --
function warningsOf(l: TL, data: Data, state: unknown, W: number, H: number): Warning[] {
  const k = cfg(l);
  const st = (state ?? {}) as State;
  const out: Warning[] = [];
  const q = expand(queueOf(l, st), data);
  if (!q.length) out.push({ level: 'warn', text: k.emptyMode === 'hide' ? t('Queue empty: the ticker is hidden.') : t('Queue empty: the fallback message is shown.'), op: true });
  const v = speedOf(l, st);
  if (k.mode === 'crawl' && v > 400) out.push({ level: 'warn', text: t('{speed} px/s is hard to read (above 400 px/s).', { speed: Math.round(v) }), op: true });
  const lk = lookOf(l, { fontSize: l.props.fontSize ?? 34, letterSpacing: l.props.letterSpacing ?? 0, fill: [1, 1, 1, 1] });
  const sc = scene(l, k, st, data, lk, W, H, Date.now(), 0);
  if (st.started && sc.ended) out.push({ level: 'info', text: t('End of the list reached — Start to play it again.'), op: true });
  if (k.mode !== 'crawl') {
    const long = q.filter((it) => sc.ms.itemW(it) + 2 * PAD > sc.zw);
    if (long.length) out.push({ level: 'info', text: t('{n} item(s) longer than the zone: they scroll within their slot.', { n: long.length }) });
  }
  return out;
}

export default defineWidget({
  id: 'ticker',
  type: 'ticker',
  label: 'Ticker',
  group: 'Data',
  icon: ArrowLeftRight,
  create: () => ({
    text: 'First headline\nSecond headline',
    separator: '—',
    speed: 140,
    fontFamily: 'Inter',
    fontWeight: 700,
    align: 'left',
    verticalAlign: 'middle',
    uppercase: false,
    spacing: 40,
    props: { width: 1920, height: 80, fontSize: 34, fill: [1, 1, 1, 1] },
  }),
  ownsLook: true,
  content: ['items', 'text'],
  live: () => true,
  ageOnly: true,
  Render: ({ layer, r, w, h, data, time, state }) => <TickerRender l={layer as unknown as TL} r={r as never} w={w} h={h} data={data} time={time} state={state} />,
  Inspector: ({ layer, set }) => {
    const l = layer as unknown as TL;
    const k = cfg(l);
    const cats: [string, string][] = [['', '—'], ...k.cats.map((c) => [c.name, c.name] as [string, string])];
    return (
      <>
        <Section id="ticker-items" title={t('Content')}>
          {k.items.length ? (
            <Row expose="items" exposeValue={k.items}>
              <ListEditor
                path="items"
                toggle="on"
                fields={[
                  { key: 'cat', options: cats, w: 92 },
                  { key: 'text', placeholder: t('Text') },
                ]}
                add={() => ({ cat: '', text: 'New item', on: true })}
                addLabel={t('Item')}
              />
            </Row>
          ) : (
            <>
              <Content path="text" multiline placeholder={t('One item per line')} />
              <Btns list={[{ label: t('Items'), title: t('Convert to items (categories, on / off)'), onClick: () => set('items', layerItems(l).map((i) => ({ cat: i.cat, text: i.text, on: i.on }))) }]} />
            </>
          )}
          {/\{\{\w+\}\}/.test([l.text ?? '', ...k.items.map((x) => x.text ?? '')].join(' ')) && <Text label={t('Item format')} path="itemFormat" value={k.itemFormat} placeholder="{name} · {text}" />}
          <Toggle label={t('Title')} path="title.on" value={k.title.on} />
          {k.title.on && (
            <>
              <Content label={t('Text')} path="title.text" value={k.title.text} />
              <ColorPick label={t('Background')} path="title.bg" value={k.title.bg} />
              <ColorPick label={t('Color')} path="title.color" value={k.title.color} />
              <Num label={t('Width')} path="title.w" value={k.title.w} min={80} max={500} unit="px" />
            </>
          )}
        </Section>
        <Section id="ticker-cats" title={t('Categories')} closed>
          <Row expose="cats" exposeValue={k.cats}>
            <ListEditor
              path="cats"
              fields={[
                { key: 'name', placeholder: t('Name') },
                { key: 'bg', color: true, w: 40 },
                { key: 'color', color: true, w: 40 },
                { key: 'text', color: true, w: 40 },
              ]}
              add={() => ({ name: 'NEW', bg: '#7c5cff', color: '#ffffff', text: '' })}
              addLabel={t('Category')}
            />
          </Row>
        </Section>
        <Section id="ticker-look" title={t('Look')}>
          <Row label={t('Font')} expose={['fontFamily', 'fontWeight']} exposeValue={{ fontFamily: l.fontFamily, fontWeight: l.fontWeight }}>
            <FontPick family={l.fontFamily} weight={l.fontWeight} onChange={(f, w) => (set('fontFamily', f), w !== l.fontWeight && set('fontWeight', w))} />
          </Row>
          <Row label={t('Upper case')} expose="uppercase" exposeValue={!!l.uppercase}>
            <button className={`btn small ${l.uppercase ? 'on' : ''}`} onClick={() => set('uppercase', !l.uppercase)} title={t('Upper case')}>
              AA
            </button>
          </Row>
          <GroupRow layer={layer as never} spec={{ label: t('Size'), props: ['fontSize'], min: 1, suffix: 'px' }} />
          <ColorGroupRow layer={layer as never} prop="fill" label={t('Color')} />
          <Num label={t('Spacing')} path="spacing" value={k.spacing} min={0} max={300} unit="px" />
          <Seg
            label={t('Separator')}
            path="sepType"
            value={k.sepType}
            options={[
              ['text', t('Text')],
              ['symbol', t('Symbol')],
              ['image', t('Logo')],
              ['none', t('None')],
            ]}
          />
          {k.sepType === 'text' && <Text label={t('Text')} path="separator" value={l.separator} />}
          {k.sepType === 'symbol' && <Seg label={t('Symbol')} path="sepSymbol" value={k.sepSymbol} options={['●', '◆', '▶', '■', '★', '|', '/']} />}
          {(k.sepType === 'text' || k.sepType === 'symbol') && <ColorPick label={t('Sep. color')} path="sepColor" value="" none={t('Text colour')} />}
          {k.sepType === 'image' && (
            <Row expose="sepImage" exposeValue={k.sepImage} label={t('Logo')}>
              <AssetPicker value={k.sepImage} onChange={(v) => set('sepImage', v)} />
            </Row>
          )}
          <Toggle label={t('Bar')} value={!!k.barBg} onChange={(v) => set('barBg', v ? '#0f1222' : '')} />
          {!!k.barBg && <ColorPick label={t('Color')} path="barBg" value={k.barBg} />}
          <Toggle label={t('Edge fade')} path="fade" value={k.fade} />
          {k.fade && <Num label={t('Width')} path="fadeW" value={k.fadeW} min={10} max={300} unit="px" />}
        </Section>
        <Section id="ticker-mode" title={t('Motion')}>
          <Seg
            label={t('Mode')}
            path="mode"
            value={k.mode}
            options={[
              ['crawl', t('Crawl'), t('Horizontal, continuous')],
              ['roll', t('Roll'), t('Vertical, line by line')],
              ['flip', t('Flip'), t('One item at a time')],
            ]}
          />
          {k.mode === 'crawl' && (
            <Seg
              label={t('Direction')}
              path="dir"
              value={k.dir}
              options={[
                ['left', '←', t('Right to left')],
                ['right', '→', t('Left to right')],
              ]}
            />
          )}
          {k.mode === 'roll' && (
            <Seg
              label={t('Direction')}
              path="rollDir"
              value={k.rollDir}
              options={[
                ['up', '↑', t('Up')],
                ['down', '↓', t('Down')],
              ]}
            />
          )}
          {k.mode !== 'flip' && <RangeNum label={t('Speed')} path="speed" value={l.speed ?? 140} min={SPEED[0]} max={SPEED[1]} step={5} unit="px/s" />}
          {k.mode !== 'crawl' && <Num label={k.mode === 'flip' ? t('Per item') : tc('ticker', 'Hold')} path="itemDur" value={k.itemDur} min={k.mode === 'flip' ? 0.5 : 0} max={60} step={0.5} precision={1} unit="s" />}
          {k.mode === 'flip' && (
            <>
              <Seg
                label={t('Transition')}
                path="trans"
                value={k.trans}
                options={[
                  ['slide', t('Slide')],
                  ['push', t('Push')],
                  ['fade', t('Fade')],
                  ['flip', t('Flip')],
                  ['type', t('Typewriter')],
                ]}
              />
              <Num label={t('Duration')} path="transDur" value={k.transDur} min={0.1} max={k.trans === 'type' ? 10 : 2} step={0.05} precision={2} unit="s" />
            </>
          )}
          {k.mode === 'crawl' && <Toggle label={t('Start full')} path="startFull" value={k.startFull} />}
          <Seg
            label={t('End')}
            path="end"
            value={k.end}
            options={[
              ['loop', t('Loop')],
              ['passes', t('Passes')],
              ['stop', t('Stop')],
            ]}
          />
          {k.end === 'passes' && <Num label={t('Passes')} path="passes" value={k.passes} min={1} max={99} />}
          <Seg
            label={t('If empty')}
            path="emptyMode"
            value={k.emptyMode}
            options={[
              ['message', t('Message')],
              ['hide', t('Hide')],
            ]}
          />
          {k.emptyMode === 'message' && <Text label={t('Message')} path="emptyText" value={k.emptyText} />}
        </Section>
      </>
    );
  },
  Operator: ({ layer, state, data, act, time, item }) => <TickerBar l={layer as unknown as TL} state={state} data={data} act={act} time={time} item={item} />,
  // A manual start on Continue waits for it (the server starts its clock on F2), unless the operator started it himself.
  waits: (l, state, _data, item) => !!l.manualStart && l.startOn !== 'trigger' && !(state as State | undefined)?.started && !item.clocks?.[l.id],
  // The Operator's preview holds it until its Play: nothing has entered yet.
  plays: () => 'start',
  warnings: (layer, { data, state, r }) => {
    const l = layer as unknown as TL;
    return warningsOf(l, data, state, num((r?.width as number) ?? (l.props.width as number) ?? 1920), num((r?.height as number) ?? (l.props.height as number) ?? 80));
  },
});

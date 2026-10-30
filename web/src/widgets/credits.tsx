// Credits: roll (vertical), crawl (horizontal) or pages (cards). Structured entries (sections, role | name lines,
// logos, spaces) with an import from text, 1 / 2 / 3 columns, styles per level, speed from a total duration or
// px/s (optimised: whole px per frame), start off / on screen, end exit / stop on the last element, fades.
// Same picture as crates/cg-render/src/wid/credits.rs; layout and progress mirror crates/cg-core/src/wid/credits.rs.
import { memo, useEffect, useMemo, useState } from 'react';
import { Flag, Minus, Pause, Play, Plus, RotateCcw, ScrollText, SkipForward } from 'lucide-react';
import { interpolateText, num, parseHex, toHex } from '../engine/evaluate';
import { parseDuration } from '../engine/widgets';
import type { Data, PlayoutItem } from '../engine/types';
import { useApp } from '../store';
import { ColorField, NumberField, Segmented } from '../ui/fields';
import { Dropdown } from '../ui/Dropdown';
import { confirmDialog } from '../ui/dialogs';
import { loadFonts, useFonts } from '../engine/fonts';
import { Btns, clamp, findCol, headerRows, ColorPick, Content, defineWidget, hex, ListEditor, Num, Row, Section, Seg, Toggle, useBound, useCentreMode, useKeys, type Warning } from './kit';
import { Centre, StartRow, WaitsContinue, WaitsTrigger, type Line2Item } from './centre';
import { familyCss, FitLine, FONT, OpField, tw, useLiveTime } from './table-text';
import { layerTime } from './ticker';
import { t, tc, tl } from '../i18n';

type Kind = 'section' | 'line' | 'logo' | 'space';
export interface Entry {
  kind?: Kind;
  a?: string;
  b?: string;
}
interface Style {
  size?: number;
  color?: string;
  weight?: number;
  upper?: boolean;
}
export interface Config {
  mode?: 'roll' | 'crawl' | 'pages';
  entries?: Entry[];
  link?: string;
  importText?: string;
  layout?: 'one' | 'two' | 'three';
  align?: 'left' | 'center' | 'right';
  /** One font for the whole credits (the channel's), Inter by default. */
  font?: string;
  gutter?: number;
  sectionGap?: number;
  lineGap?: number;
  spaceH?: number;
  logoH?: number;
  st?: { section?: Style; role?: Style; name?: Style };
  speedMode?: 'duration' | 'pxs' | 'card';
  duration?: number;
  pxs?: number;
  optimised?: boolean;
  /** Frames per second of the optimised speed (the project's). */
  fps?: number;
  maxPxs?: number;
  cardDur?: number;
  start?: 'off' | 'on';
  end?: 'exit' | 'stop';
  shortMode?: 'center' | 'exit';
  fade?: boolean;
  fadeSize?: number;
  panel?: boolean;
  panelColor?: string;
}
/** Mirrors `State` of the Rust module. */
export interface State {
  started?: boolean;
  running?: boolean;
  base?: number;
  anchor?: number;
  rate?: number | null;
}

const S = (size: number, color: string, weight: number, upper: boolean) => ({ size, color, weight, upper });
export function cfg(c: Config) {
  const st = c.st ?? {};
  return {
    mode: c.mode ?? 'roll',
    entries: (c.entries ?? []).map((e) => ({ kind: e.kind ?? 'line', a: e.a ?? '', b: e.b ?? '' })),
    link: c.link ?? '',
    importText: c.importText ?? '',
    layout: c.layout ?? 'two',
    align: c.align ?? 'center',
    font: c.font?.trim() || FONT,
    gutter: c.gutter ?? 40,
    sectionGap: c.sectionGap ?? 70,
    lineGap: c.lineGap ?? 10,
    spaceH: c.spaceH ?? 40,
    logoH: c.logoH ?? 130,
    st: { section: { ...S(30, '#a594ff', 900, true), ...st.section }, role: { ...S(26, '#a3a9bd', 600, true), ...st.role }, name: { ...S(38, '#ffffff', 800, false), ...st.name } },
    speedMode: c.speedMode ?? 'duration',
    duration: c.duration ?? 40,
    pxs: c.pxs ?? 120,
    optimised: c.optimised ?? false,
    fps: c.fps && c.fps >= 1 ? c.fps : 25,
    maxPxs: c.maxPxs ?? 260,
    cardDur: c.cardDur ?? 4,
    start: c.start ?? 'on',
    end: c.end ?? 'stop',
    shortMode: c.shortMode ?? 'center',
    fade: c.fade ?? true,
    fadeSize: c.fadeSize ?? 140,
    panel: c.panel ?? true,
    panelColor: c.panelColor ?? '#060812c7',
  };
}
type K = ReturnType<typeof cfg>;
type St = K['st']['name'];
type E = { kind: Kind; a: string; b: string };

// ------------------------------------------------------------- content --
/** Mirrors parse(). */
export function parse(text: string): E[] {
  const out: E[] = [];
  for (const line of String(text).split(/\r?\n/)) {
    const l = line.trim();
    if (!l) {
      if (out.length && out[out.length - 1].kind !== 'space') out.push({ kind: 'space', a: '', b: '' });
      continue;
    }
    if (l.startsWith('#')) {
      out.push({ kind: 'section', a: l.replace(/^#+\s*/, ''), b: '' });
      continue;
    }
    if (l.toLowerCase().startsWith('[logo') && l.endsWith(']')) {
      const v = l.slice(5, -1).trim();
      out.push({ kind: 'logo', a: v || 'PULSE TV', b: '' });
      continue;
    }
    const parts = line.split(/[|;\t]/);
    if (parts.length > 1)
      out.push({
        kind: 'line',
        a: parts[0].trim(),
        b: parts
          .slice(1)
          .map((s) => s.trim())
          .join(' ')
          .trim(),
      });
    else out.push({ kind: 'line', a: '', b: l });
  }
  return out;
}
export const toText = (es: E[]) => es.map((e) => (e.kind === 'section' ? `# ${e.a}` : e.kind === 'logo' ? `[logo ${e.a}]` : e.kind === 'space' ? '' : e.a ? `${e.a} | ${e.b}` : e.b)).join('\n');
/** Entries from a table (a data set): its section / logo / role / name columns, else the first two columns as role and name. */
function tableEntries(head: string[], rows: string[][]): E[] {
  const section = findCol(head, ['section']);
  const logo = findCol(head, ['logo']);
  let role = findCol(head, ['role', 'job', 'function']);
  let name = findCol(head, ['name', 'person']);
  if (section < 0 && logo < 0 && role < 0 && name < 0) [role, name] = head.length >= 2 ? [0, 1] : [-1, 0];
  const cell = (r: string[], i: number) => (i >= 0 ? (r[i] ?? '').trim() : '');
  const out: E[] = [];
  for (const r of rows) {
    if (cell(r, section)) out.push({ kind: 'section', a: cell(r, section), b: '' });
    if (cell(r, logo)) out.push({ kind: 'logo', a: cell(r, logo), b: '' });
    if (cell(r, role) || cell(r, name)) out.push({ kind: 'line', a: cell(r, role), b: cell(r, name) });
  }
  return out;
}
export function entriesOf(k: K, data: Data): E[] {
  if (k.link.trim()) {
    const v = interpolateText(k.link, data);
    if (v.trim().startsWith('[')) {
      try {
        const a = JSON.parse(v.trim());
        const t = Array.isArray(a) ? headerRows(a) : null;
        if (t) return tableEntries(t.head, t.rows);
        if (Array.isArray(a)) {
          const out: E[] = [];
          for (const r of a) {
            if (typeof r === 'string') out.push(...parse(r));
            else if (r && typeof r === 'object') {
              const s = (x: string) => (typeof r[x] === 'string' ? r[x] : '');
              if (s('section')) out.push({ kind: 'section', a: s('section'), b: '' });
              if (s('logo')) out.push({ kind: 'logo', a: s('logo'), b: '' });
              if (s('name')) out.push({ kind: 'line', a: s('role'), b: s('name') });
            }
          }
          return out;
        }
      } catch {
        /* text */
      }
    }
    return parse(v);
  }
  return k.entries.map((e) => ({ kind: e.kind, a: interpolateText(e.a, data), b: interpolateText(e.b, data) }));
}

// -------------------------------------------------------------- layout --
type Body = { t: 'section'; s: string } | { t: 'logo'; s: string } | { t: 'space' } | { t: 'lines'; row: [string, string][] };
interface Block {
  body: Body;
  gap: number;
  y: number;
  h: number;
  roleH: number;
}
const R = Math.round;
export function blocks(k: K, es: E[]): [Block[], number] {
  const { section: s, role: rs, name: ns } = k.st;
  const out: Block[] = [];
  let i = 0;
  while (i < es.length) {
    const e = es[i];
    const first = !out.length;
    if (e.kind === 'section') (out.push({ body: { t: 'section', s: e.a }, gap: first ? 0 : k.sectionGap, y: 0, h: R(s.size * 1.3) + R(k.lineGap * 1.5), roleH: 0 }), i++);
    else if (e.kind === 'logo') (out.push({ body: { t: 'logo', s: e.a }, gap: first ? 0 : k.sectionGap, y: 0, h: k.logoH, roleH: 0 }), i++);
    else if (e.kind === 'space') (out.push({ body: { t: 'space' }, gap: 0, y: 0, h: k.spaceH, roleH: 0 }), i++);
    else if (k.layout === 'three') {
      const row: [string, string][] = [];
      while (i < es.length && es[i].kind === 'line' && row.length < 3) (row.push([es[i].a, es[i].b]), i++);
      const roleH = row.some((x) => x[0]) ? R(rs.size * 1.3) : 0;
      out.push({ body: { t: 'lines', row }, gap: 0, y: 0, h: roleH + R(ns.size * 1.25) + k.lineGap, roleH });
    } else if (k.layout === 'one') {
      const roleH = e.a ? R(rs.size * 1.3) : 0;
      out.push({ body: { t: 'lines', row: [[e.a, e.b]] }, gap: 0, y: 0, h: roleH + R(ns.size * 1.25) + k.lineGap, roleH });
      i++;
    } else {
      const lh = R(Math.max(rs.size, ns.size) * 1.32);
      out.push({ body: { t: 'lines', row: [[e.a, e.b]] }, gap: 0, y: 0, h: lh + k.lineGap, roleH: lh });
      i++;
    }
  }
  while (out.length && out[out.length - 1].body.t === 'space') out.pop();
  let y = 0;
  out.forEach((b) => ((y += b.gap), (b.y = y), (y += b.h)));
  return [out, y];
}
interface Card {
  blocks: Block[];
  h: number;
}
export function cards(bl: Block[], boxH: number): Card[] {
  const max = boxH - 80;
  const cs: Card[] = [];
  for (const b of bl) {
    const cur = cs[cs.length - 1];
    if (b.body.t === 'space' && (!cur || !cur.blocks.length)) continue;
    if (!cur || b.body.t === 'section' || b.body.t === 'logo' || cur.h + b.h > max) cs.push({ blocks: [], h: 0 });
    const c = cs[cs.length - 1];
    const yy = c.blocks.length ? c.h + b.gap : 0;
    c.blocks.push({ ...b, y: yy });
    c.h = yy + b.h;
  }
  cs.forEach((c) => {
    while (c.blocks.length && c.blocks[c.blocks.length - 1].body.t === 'space') c.h -= c.blocks.pop()!.h;
  });
  return cs.filter((c) => c.blocks.length);
}
const styled = (s: St, t: string) => (s.upper ? t.toUpperCase() : t);
const lsOf = (s: St) => (s.upper ? s.size * 0.06 : 0);
const mw = (t: string, s: St, family: string) => tw(styled(s, t), s.size, s.weight, lsOf(s), family);
const isAsset = (s: string) => s.startsWith('/') || s.includes('.');
const IMG = new Map<string, number>();
/** Width / height of a picture (loaded once; 3 until known). */
function ratioOf(src: string): number {
  if (!isAsset(src)) return 3;
  const v = IMG.get(src);
  if (v !== undefined) return v || 3;
  IMG.set(src, 0);
  const im = new Image();
  im.onload = () => IMG.set(src, im.naturalWidth / Math.max(1, im.naturalHeight));
  im.src = src;
  return 3;
}
const crawlLogoH = (k: K, h: number) => Math.min(k.logoH, h * 0.7);
interface CItem {
  body: Body;
  x: number;
  w: number;
  roleW: number;
}
export function crawlItems(k: K, es: E[], h: number): [CItem[], number] {
  const out: CItem[] = [];
  let x = 0;
  const lh = crawlLogoH(k, h);
  for (const e of es) {
    let gap = out.length ? k.gutter * 2 : 0;
    let it: Omit<CItem, 'x'>;
    if (e.kind === 'section') {
      if (out.length) gap = k.sectionGap * 2;
      it = { body: { t: 'section', s: e.a }, w: mw(e.a, k.st.section, k.font), roleW: 0 };
    } else if (e.kind === 'logo') {
      if (out.length) gap = k.sectionGap * 2;
      it = { body: { t: 'logo', s: e.a }, w: lh * ratioOf(e.a), roleW: 0 };
    } else if (e.kind === 'space') {
      x += k.spaceH;
      continue;
    } else {
      const rw = e.a ? mw(e.a, k.st.role, k.font) + 16 : 0;
      it = { body: { t: 'lines', row: [[e.a, e.b]] }, w: rw + mw(e.b, k.st.name, k.font), roleW: rw };
    }
    x += gap;
    out.push({ ...it, x });
    x += it.w;
  }
  return [out, x];
}
export function travel(k: K, len: number, lastMid: number | undefined, boxl: number) {
  let a = k.start === 'off' ? boxl : k.fade ? k.fadeSize * 0.6 : 0;
  const short = len < boxl;
  const b = short && k.shortMode === 'center' ? (boxl - len) / 2 : k.end === 'stop' && lastMid !== undefined ? boxl / 2 - lastMid : -len;
  a = Math.max(a, b);
  return { a, b, x: a - b, len, short };
}
/** Everything the motion needs (mirrors the renderer). */
function geo(k: K, es: E[], w: number, h: number) {
  if (k.mode === 'pages') {
    const [bl] = blocks(k, es);
    const cs = cards(bl, h);
    return { mode: k.mode, cards: cs, x: cs.length, a: 0, len: 0, short: false, bl, items: [] as CItem[] };
  }
  if (k.mode === 'roll') {
    const [bl, len] = blocks(k, es);
    const t = travel(k, len, bl.length ? bl[bl.length - 1].y + bl[bl.length - 1].h / 2 : undefined, h);
    return { mode: k.mode, cards: [] as Card[], ...t, bl, items: [] as CItem[] };
  }
  const [items, len] = crawlItems(k, es, h);
  const l = items[items.length - 1];
  const t = travel(k, len, l ? l.x + l.w / 2 : undefined, w);
  return { mode: k.mode, cards: [] as Card[], ...t, bl: [] as Block[], items };
}
export function nominal(k: K, x: number) {
  x = Math.max(1e-6, x);
  if (k.speedMode === 'duration') return 1 / Math.max(0.5, k.duration);
  if (k.mode === 'pages') return 1 / (Math.max(0.3, k.cardDur) * x);
  return Math.max(1, k.pxs) / x;
}
const optim = (k: K, v: number) => (k.optimised && k.mode !== 'pages' ? Math.max(1, Math.round(v / k.fps)) * k.fps : v);
export function progress(k: K, st: State, x: number, now: number, age: number) {
  const rate = st.rate ?? nominal(k, x);
  const [base, dt] = !st.started ? [0, Math.max(0, age)] : st.running ? [st.base ?? 0, Math.max(0, now - (st.anchor ?? 0)) / 1000] : [st.base ?? 0, 0];
  const adv = k.optimised && k.mode !== 'pages' && x > 0 ? ((optim(k, rate * x) / k.fps) * Math.floor(dt * k.fps)) / x : rate * dt;
  return clamp(base + adv, 0, 1);
}
function realRate(k: K, st: State, x: number) {
  const rate = st.rate ?? nominal(k, x);
  return k.optimised && k.mode !== 'pages' && x > 0 ? optim(k, rate * x) / x : rate;
}
export const remaining = (k: K, st: State, x: number, now: number, age: number) => (1 - progress(k, st, x, now, age)) / Math.max(1e-9, realRate(k, st, x));

// -------------------------------------------------------------- render --
function Logo({ src, x, y, h }: { src: string; x: number; y: number; h: number }) {
  if (isAsset(src) && (IMG.get(src) ?? 0) > 0) return <img src={src} alt="" draggable={false} style={{ position: 'absolute', left: x, top: y, height: h, display: 'block' }} />;
  const n = src.toUpperCase().slice(0, 18);
  return (
    <svg style={{ position: 'absolute', left: x, top: y }} width={h * 3} height={h} viewBox="0 0 420 140">
      <defs>
        <linearGradient id="cg-cr-g" x1="0" x2="1">
          <stop offset="0" stopColor="#ff3b5c" />
          <stop offset="1" stopColor="#7c5cff" />
        </linearGradient>
      </defs>
      <rect x="4" y="4" width="412" height="132" rx="30" fill="url(#cg-cr-g)" />
      <circle cx="70" cy="70" r="38" fill="#fff" />
      <circle cx="70" cy="70" r="16" fill="#ff3b5c" />
      <text x="255" y="70" dominantBaseline="central" fontFamily="Inter, sans-serif" fontWeight={900} fontSize={n.length > 9 ? 30 : 46} fill="#fff" textAnchor="middle">
        {n}
      </text>
    </svg>
  );
}
function TextBox({ s, t, align, box, cut, family }: { s: St; t: string; align: 'left' | 'center' | 'right'; box: [number, number, number, number]; cut?: boolean; family: string }) {
  if (!t.trim()) return null;
  return <FitLine text={styled(s, t)} size={s.size} weight={s.weight} ls={lsOf(s)} align={align} box={box} color={hex(s.color, '#fff')} family={family} fit={cut ? { overflow: 'ellipsis', minSize: s.size, minSqueeze: 100 } : undefined} />;
}
// Memoised: a block rolling by keeps its props (it is slid by its parent), it is not drawn again on each frame.
const BlockView = memo(function BlockView({ k, b, y0, w }: { k: K; b: Block; y0: number; w: number }) {
  const { section: ss, role: rs, name: ns } = k.st;
  const al = k.align;
  if (b.body.t === 'space') return null;
  if (b.body.t === 'section') return <TextBox family={k.font} s={ss} t={b.body.s} align={al} box={[0, y0, w, R(ss.size * 1.3)]} />;
  if (b.body.t === 'logo') {
    const lw = k.logoH * ratioOf(b.body.s);
    return <Logo src={b.body.s} x={al === 'left' ? 0 : al === 'center' ? (w - lw) / 2 : w - lw} y={y0} h={k.logoH} />;
  }
  const row = b.body.row;
  if (k.layout === 'two') {
    const [role, name] = row[0];
    const split = al === 'left' ? 0.4 : al === 'right' ? 0.6 : 0.5;
    const lw = w * split - k.gutter / 2;
    return (
      <>
        <TextBox family={k.font} s={rs} t={role} align={al === 'left' ? 'left' : 'right'} box={[0, y0, lw, b.roleH]} cut />
        <TextBox family={k.font} s={ns} t={name} align={al === 'right' ? 'right' : 'left'} box={[lw + k.gutter, y0, w - lw - k.gutter, b.roleH]} cut />
      </>
    );
  }
  const three = k.layout === 'three';
  const cw = three ? (w - 2 * k.gutter) / 3 : w;
  const nh = R(ns.size * 1.25);
  return (
    <>
      {row.map(([role, name], j) => (
        <span key={j}>
          {b.roleH > 0 && <TextBox family={k.font} s={rs} t={role} align={al} box={[j * (cw + k.gutter), y0, cw, b.roleH]} cut={three} />}
          <TextBox family={k.font} s={ns} t={name} align={al} box={[j * (cw + k.gutter), y0 + b.roleH, cw, nh]} cut={three} />
        </span>
      ))}
    </>
  );
});

function CreditsRender({ layer, w, h, data, time, state }: { layer: Config; w: number; h: number; data: Data; time: { nowMs: number; ageSecs: number }; state: unknown }) {
  // Redrawn on each frame: the layout is kept (laid out again when a font loads).
  const fonts = useFonts((s) => s.ready);
  const k = useMemo(() => cfg(layer), [layer]);
  const st = (state ?? {}) as State;
  const g = useMemo(() => geo(k, entriesOf(k, data), w, h), [k, data, w, h, fonts]); // eslint-disable-line react-hooks/exhaustive-deps
  const vert = k.mode !== 'crawl';
  const f = k.fadeSize;
  const mask = k.fade ? `linear-gradient(${vert ? '180deg' : '90deg'},transparent 0,#000 ${f}px,#000 calc(100% - ${f}px),transparent 100%)` : undefined;
  const band = Math.min(h, R(Math.max(k.st.name.size, k.st.section.size, Math.min(k.logoH, h * 0.7)) * 1.6));
  const out: React.ReactNode[] = [];
  const p = progress(k, st, g.x, time.nowMs, time.ageSecs);
  if (k.mode === 'roll') {
    const off = g.a - p * g.x;
    g.bl.forEach((b, i) => {
      const y = off + b.y;
      if (y > h || y + b.h < 0) return;
      // Slid as one (a transform): on each frame nothing is laid out again.
      out.push(
        <div key={i} style={{ transform: `translateY(${y}px)` }}>
          <BlockView k={k} b={b} y0={0} w={w} />
        </div>,
      );
    });
  } else if (k.mode === 'crawl') {
    const off = g.a - p * g.x;
    const lh = crawlLogoH(k, h);
    g.items.forEach((it, i) => {
      const x = off + it.x;
      if (x > w || x + it.w < 0) return;
      if (it.body.t === 'section') out.push(<TextBox family={k.font} key={i} s={k.st.section} t={it.body.s} align="left" box={[x, 0, it.w + 4, h]} />);
      else if (it.body.t === 'logo') out.push(<Logo key={i} src={it.body.s} x={x} y={(h - lh) / 2} h={lh} />);
      else if (it.body.t === 'lines') {
        const [role, name] = it.body.row[0];
        out.push(
          <span key={i}>
            {role && <TextBox family={k.font} s={k.st.role} t={role} align="left" box={[x, 0, it.roleW, h]} />}
            <TextBox family={k.font} s={k.st.name} t={name} align="left" box={[x + it.roleW, 0, it.w - it.roleW + 4, h]} />
          </span>,
        );
      }
    });
  } else {
    const n = g.cards.length;
    const pc = p * n;
    const rate = (st.rate ?? nominal(k, n)) * n;
    const fk = Math.min(0.45, 0.6 * Math.max(0.05, rate));
    g.cards.forEach((c, i) => {
      const inn = i === 0 ? (k.start === 'on' ? 1 : clamp(pc / fk, 0, 1)) : clamp((pc - i + fk / 2) / fk, 0, 1);
      const o2 = i < n - 1 ? clamp((i + 1 + fk / 2 - pc) / fk, 0, 1) : k.end === 'exit' ? clamp((n - pc) / fk, 0, 1) : 1;
      const o = Math.min(inn, o2);
      if (o <= 0.001) return;
      const top = (h - c.h) / 2;
      out.push(
        <div key={i} style={{ position: 'absolute', inset: 0, opacity: o, transform: `translateY(${(1 - inn) * 30}px)` }}>
          {c.blocks.map((b, j) => (
            <BlockView key={j} k={k} b={b} y0={top + b.y} w={w} />
          ))}
        </div>,
      );
    });
  }
  return (
    <div style={{ position: 'absolute', inset: 0, fontFamily: familyCss(k.font) }}>
      {k.panel && <div style={{ position: 'absolute', left: 0, width: w, ...(vert ? { top: 0, height: h } : { top: R((h - band) / 2), height: band }), background: hex(k.panelColor), borderRadius: 6 }} />}
      <div style={{ position: 'absolute', inset: 0, overflow: 'hidden', WebkitMaskImage: mask, maskImage: mask }}>{out}</div>
    </div>
  );
}

// ------------------------------------------------------------ inspector --
const pad2 = (v: number) => String(Math.floor(v)).padStart(2, '0');
const fmtDur = (sec: number) => {
  const v = Math.max(0, Number.isFinite(sec) ? sec : 0);
  return `${pad2(v / 60)}:${pad2(v % 60)}.${Math.floor((v * 10) % 10)}`;
};
/** A time of day, hh:mm:ss, in the server's zone (the end time is read there). */
const fmtClock = (ms: number, tz?: string) => {
  try {
    return new Date(ms).toLocaleTimeString('en-GB', { timeZone: tz || undefined });
  } catch {
    return new Date(ms).toLocaleTimeString('en-GB');
  }
};
const mss = (sec: number) => {
  const s = Math.max(0, Math.ceil(sec));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};

const TEXT0 = `# Presented by
Presenter | Claire Martin
Co-host | Julien Moreau

# Editorial
Editor-in-chief | Sophie Laurent
Senior editor | Karim Benali
Reporters | Léa Dubois
 | Thomas Petit
 | Inès Garnier
Fact-checking | Hugo Lefèvre

# Production
Executive producer | Nathalie Roux
Producer | Antoine Girard
Production manager | Camille Bonnet
Production assistant | Yanis Mercier

# Technical crew
Director | Paul Fontaine
Vision mixer | Emma Chevalier
Graphics | Lucas Perrin
Sound engineer | Chloé Robin
Camera operators | Maxime Faure
 | Sarah Blanc
 | Nicolas Henry
Lighting | Manon Gauthier

[logo PULSE TV]`;

type CL = Config & { id: string; name?: string; props: Record<string, unknown>; manualStart?: boolean; startOn?: 'continue' | 'trigger'; startTrigger?: string };
const sizeOf = (l: CL, r?: Record<string, unknown>) => [num((r?.width as number) ?? (l.props.width as number) ?? 1100), num((r?.height as number) ?? (l.props.height as number) ?? 1000)];

function warningsOf(l: CL, data: Data, state: unknown, w: number, h: number): Warning[] {
  const k = cfg(l);
  const st = (state ?? {}) as State;
  const es = entriesOf(k, data);
  const g = geo(k, es, w, h);
  const out: Warning[] = [];
  const live = !!st.started;
  if (k.mode !== 'pages') {
    const v = optim(k, (live && st.rate ? st.rate : nominal(k, g.x)) * g.x);
    if (v > k.maxPxs) out.push({ level: 'error', text: t('Speed {speed} px/s is above the readable threshold ({max} px/s): at least {secs} s needed.', { speed: v.toFixed(0), max: k.maxPxs, secs: Math.ceil(g.x / k.maxPxs) }), op: true });
    if (k.optimised) out.push({ level: 'info', text: t('Optimised speed: {perFrame} px/frame at {fps} fps ({speed} px/s), the duration follows: {duration}.', { perFrame: Math.round(v / k.fps), fps: +k.fps.toFixed(2), speed: v.toFixed(0), duration: fmtDur(g.x / Math.max(1, v)) }) });
  } else {
    const per = 1 / Math.max(1e-6, (live && st.rate ? st.rate : nominal(k, g.x)) * Math.max(1, g.x));
    if (per < 1.5) out.push({ level: 'error', text: t('{secs} s per card is too short to read (min 1.5 s).', { secs: per.toFixed(1) }), op: true });
  }
  if (g.short) out.push({ level: 'info', text: k.shortMode === 'center' ? t('Content shorter than the screen: it stops centred — no empty scroll.') : t('Content shorter than the screen: it passes and exits — no empty scroll.') });
  if (!es.length) out.push({ level: 'warn', text: t('No content.'), op: true });
  return out;
}

/** The font of the whole credits (families of Assets, each shown in itself). */
function FamilyRow({ value }: { value: string }) {
  const b = useBound();
  const families = useFonts((s) => s.families);
  // The list is loaded at start, which fails before logging in.
  useEffect(() => {
    if (!families.length) void loadFonts();
  }, [families.length]);
  const known = families.some((f) => f.family.toLowerCase() === value.toLowerCase());
  const opts = [...(known ? [] : [{ value, label: t('{family} (missing)', { family: value }) }]), ...families.map((f) => ({ value: f.family, label: f.family, style: { fontFamily: `"${f.family}", Inter, sans-serif` } }))];
  return (
    <Row label={t('Font')} expose="font" exposeValue={value}>
      <div className="font-pick">
        <Dropdown value={families.find((f) => f.family.toLowerCase() === value.toLowerCase())?.family ?? value} options={opts} onChange={(v) => b.set('font', v)} />
      </div>
    </Row>
  );
}

/** The text pasted in the import box (several lines). */
function PasteBox({ init, onChange }: { init: string; onChange: (v: string) => void }) {
  const [v, setV] = useState(init);
  return <textarea rows={14} value={v} placeholder={'# Section\nRole | Name'} style={{ width: '100%', minWidth: 360, fontFamily: 'var(--mono, monospace)', fontSize: 12 }} onKeyDown={(e) => e.stopPropagation()} onChange={(e) => (setV(e.target.value), onChange(e.target.value))} />;
}

type OpP = { layer: unknown; state: unknown; data: Data; act: (op: string, args?: unknown) => void; item?: unknown; time: { nowMs: number; ageSecs: number; defaultTz?: string; fps?: number; held?: boolean } };

/** Where the credits are now, for the Operator's controls. */
function useRun({ layer, state, data, time }: OpP) {
  const { now, age } = useLiveTime(time);
  const l = layer as CL;
  const k = cfg(l);
  const st = (state ?? {}) as State;
  const [w, h] = sizeOf(l);
  const g = geo(k, entriesOf(k, data), w, h);
  const ageNow = st.started ? 0 : Math.max(0, age);
  const f = progress(k, st, g.x, now, ageNow);
  const rem = remaining(k, st, g.x, now, ageNow);
  // Manual launch: it waits for the operator's top.
  const status: 'running' | 'paused' | 'done' | 'idle' = f >= 1 - 1e-9 ? 'done' : !st.started ? (l.manualStart ? 'idle' : 'running') : st.running ? 'running' : f > 0 ? 'paused' : 'idle';
  return { l, k, st, g, f, rem, status, now, travel: { travel: g.x } };
}

/**
 * The control centre of the credits (web/src/widgets/centre.tsx). Line 1: the time left, when
 * it ends (or its state), its start on air. Line 2: play / pause, next card (pages), finish,
 * reset, ∓10 s of the time left. Line 3: how it starts (Designer), the duration of the whole
 * roll (the template's in the Designer, the air's in the Operator), the end time (Operator).
 */
function CreditsBar(p: OpP) {
  const { act } = p;
  const item = p.item as PlayoutItem | undefined;
  const mode = useCentreMode();
  const b = useBound();
  const designer = mode === 'designer';
  const l0 = p.layer as CL;
  const time = layerTime(l0, item, p.time, !!(p.state as State | undefined)?.started);
  const { l, k, st, g, rem, status, now, travel } = useRun({ ...p, time });
  const tz = time.defaultTz || undefined;
  const running = status === 'running';
  const play = () => act(status === 'paused' ? 'resume' : 'start', travel);
  // Space stays the Operator's CONTINUE.
  useKeys({
    s: () => act('start', travel),
    p: () => (running ? act('pause', travel) : play()),
    ArrowRight: () => k.mode === 'pages' && act('next', travel),
  });
  const sub = running ? t('ends at {time}', { time: fmtClock(now + rem * 1000, tz) }) : status === 'paused' ? tc('cc', 'paused') : status === 'done' ? tc('cc', 'finished') : tc('cc', 'ready');
  // Waiting for its start on air: Continue (F2) or its trigger (the server starts it, see credits.rs).
  const waiting = !!item && !!l.manualStart && !st.started;
  const status1 = waiting ? l.startOn === 'trigger' ? <WaitsTrigger name={l.startTrigger ?? ''} /> : <WaitsContinue /> : undefined;
  const resetMsg = item ? t('{name} is on air: back to the start of the credits?', { name: l.name ?? '' }) : undefined;
  const actions: Line2Item[] = [
    running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: () => act('pause', travel) } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: play },
    k.mode === 'pages' && { icon: SkipForward, label: tc('credits', 'Next card'), key: '→', onClick: () => act('next', travel) },
    { icon: Flag, label: t('Finish'), disabled: status === 'done', onClick: () => act('finish', travel) },
    { icon: RotateCcw, label: tc('cc', 'Reset'), confirm: resetMsg, onClick: () => act('reset') },
    { icon: Minus, text: '−10 s', label: t('Remaining −10 s'), onClick: () => act('adjust', { seconds: -10, ...travel }) },
    { icon: Plus, text: '+10 s', label: t('Remaining +10 s'), onClick: () => act('adjust', { seconds: 10, ...travel }) },
  ];
  // The time of the whole roll: the template's speed in the Designer, the air's in the Operator (what is left takes its share).
  const total = designer ? 1 / Math.max(1e-9, nominal(k, g.x)) : 1 / Math.max(1e-9, realRate(k, st, g.x));
  const setTotal = (v: string) => {
    const s = parseDuration(v);
    // Not a duration: the field shows the current one again.
    if (s == null || s <= 0) return;
    if (!designer) return act('duration', { seconds: s, ...travel });
    b.set('speedMode', 'duration');
    b.set('duration', Math.round(s));
  };
  return (
    <Centre name={l.name} readout={{ v: mss(rem), sub }} status={status1} actions={actions}>
      <StartRow layer={designer ? (b.obj as CL) : l} />
      <Row label={t('Duration')} title={t('The whole roll, mm:ss')}>
        <OpField width={70} title={t('Duration')} value={mss(Math.round(total))} placeholder="mm:ss" onSubmit={setTotal} />
      </Row>
      {!designer && (
        <Row label={t('End at')}>
          <OpField width={84} title={t('End at')} value={status === 'done' ? '' : fmtClock(now + rem * 1000, tz)} placeholder="hh:mm:ss" onSubmit={(v) => act('endAt', { time: v, ...(tz ? { tz } : {}), ...travel })} />
        </Row>
      )}
    </Centre>
  );
}

/** Colour · size · weight · CAPS of a level. */
function StyleRows({ label, path, s }: { label: string; path: string; s: St }) {
  const b = useBound();
  return (
    <>
      <Row expose={[`${path}.color`, `${path}.size`]} exposeValue={{ [`${path}.color`]: s.color, [`${path}.size`]: s.size }} label={label}>
        <span className="pp-field color" style={{ flex: 1, minWidth: 0 }}>
          <ColorField value={parseHex(s.color) ?? [1, 1, 1, 1]} onChange={(c) => b.set(`${path}.color`, toHex(c, c[3] < 1), true)} />
        </span>
        <span className="pp-field" style={{ flex: 'none' }}>
          <NumberField value={s.size} min={1} max={400} suffix="px" onChange={(x) => b.set(`${path}.size`, x, true)} />
        </span>
      </Row>
      <Row expose={[`${path}.weight`, `${path}.upper`]} exposeValue={{ [`${path}.weight`]: s.weight, [`${path}.upper`]: s.upper }} label="">
        <Segmented
          value={String(s.weight)}
          onChange={(x) => b.set(`${path}.weight`, +x)}
          options={[
            { value: '500', label: t('Med') },
            { value: '700', label: t('Bold') },
            { value: '900', label: t('Black') },
          ]}
        />
        <Segmented value={s.upper ? 'u' : ''} onChange={() => b.set(`${path}.upper`, !s.upper)} options={[{ value: 'u', label: 'AA', title: t('Upper case') }]} />
      </Row>
    </>
  );
}

export default defineWidget({
  id: 'credits',
  type: 'credits',
  label: 'Credits',
  group: 'Data',
  icon: ScrollText,
  create: () => {
    // The sample credits in the project's language.
    const text = tl(useApp.getState().project?.language, TEXT0);
    return { entries: parse(text), importText: text, fade: false, panel: false, props: { width: 1100, height: 1000 } };
  },
  ownsLook: true,
  content: ['entries', 'link'],
  live: () => true,
  ageOnly: true,
  Render: ({ layer, w, h, data, time, state }) => <CreditsRender layer={layer} w={w} h={h} data={data} time={time} state={state} />,
  Inspector: ({ layer, edit }) => {
    const l = layer as unknown as CL;
    const k = cfg(l);
    const set = (fn: (x: Config) => void) => edit((x) => fn(x as never));
    return (
      <>
        <Section id="credits-content" title={t('Content')}>
          <Content label={t('Source')} path="link" placeholder="{{key}}" />
          {!k.link.trim() && (
            <>
              <Row expose="entries" exposeValue={k.entries}>
                <ListEditor
                  path="entries"
                  fields={[
                    {
                      key: 'kind',
                      options: [
                        ['section', t('Section')],
                        ['line', t('Line')],
                        ['logo', t('Logo')],
                        ['space', t('Space')],
                      ],
                      w: 82,
                    },
                    { key: 'a', placeholder: t('Role / title / logo') },
                    { key: 'b', placeholder: t('Name') },
                  ]}
                  add={() => ({ kind: 'line', a: 'Role', b: 'Name' })}
                  addLabel={t('Entry')}
                />
              </Row>
              <Btns
                label={t('Import')}
                list={[
                  {
                    label: t('Paste a text…'),
                    title: t('Paste a text (# Section, Role | Name)'),
                    onClick: async () => {
                      let text = toText(k.entries);
                      const ok = await confirmDialog({ title: t('Paste a text (# Section, Role | Name)'), message: <PasteBox init={text} onChange={(v) => (text = v)} />, confirm: t('Replace') });
                      if (ok && text.trim()) set((x) => void ((x.entries = parse(text)), (x.importText = text)));
                    },
                  },
                ]}
              />
            </>
          )}
        </Section>
        <Section id="credits-layout" title={t('Layout')}>
          {k.mode !== 'crawl' && (
            <>
              <Seg
                label={t('Columns')}
                path="layout"
                value={k.layout}
                options={[
                  ['one', '1'],
                  ['two', '2'],
                  ['three', '3'],
                ]}
              />
              <Seg
                label={t('Align')}
                path="align"
                value={k.align}
                options={[
                  ['left', t('Left')],
                  ['center', t('Centre')],
                  ['right', t('Right')],
                ]}
              />
            </>
          )}
          <Num label={t('Gutter')} path="gutter" value={k.gutter} min={0} max={1000} unit="px" />
          <Num label={t('Section gap')} path="sectionGap" value={k.sectionGap} min={0} max={1000} unit="px" />
          <Num label={t('Line gap')} path="lineGap" value={k.lineGap} min={0} max={1000} unit="px" />
          <Num label={t('Space')} path="spaceH" value={k.spaceH} min={0} max={1000} unit="px" />
          <Num label={t('Logo height')} path="logoH" value={k.logoH} min={10} max={1000} unit="px" />
        </Section>
        <Section id="credits-styles" title={t('Styles')}>
          <FamilyRow value={k.font} />
          <StyleRows label={t('Section')} path="st.section" s={k.st.section} />
          <StyleRows label={t('Role')} path="st.role" s={k.st.role} />
          <StyleRows label={t('Name')} path="st.name" s={k.st.name} />
          <Toggle label={t('Panel')} path="panel" value={k.panel} />
          {k.panel && <ColorPick label={t('Color')} path="panelColor" value={k.panelColor} />}
          <Toggle label={t('Fade')} path="fade" value={k.fade} />
          {k.fade && <Num label={t('Size')} path="fadeSize" value={k.fadeSize} min={1} max={1000} unit="px" />}
        </Section>
        <Section id="credits-motion" title={t('Motion')}>
          <Seg
            label={t('Mode')}
            path="mode"
            value={k.mode}
            options={[
              ['roll', t('Roll'), t('Vertical')],
              ['crawl', t('Crawl'), t('Horizontal')],
              ['pages', t('Pages'), t('Cards')],
            ]}
          />
          {k.mode === 'pages' ? (
            <>
              <Seg
                expose="speedMode"
                exposeValue={k.speedMode}
                exposeOptions={[
                  ['duration', t('Total')],
                  ['card', t('Per card')],
                ]}
                label={t('Speed by')}
                value={k.speedMode === 'pxs' ? 'card' : k.speedMode}
                onChange={(x) => set((c) => void (c.speedMode = x))}
                options={[
                  ['duration', t('Total')],
                  ['card', t('Per card')],
                ]}
              />
              {k.speedMode === 'duration' ? <Num label={t('Total')} path="duration" value={k.duration} min={1} max={3600} unit="s" /> : <Num label={t('Per card')} path="cardDur" value={k.cardDur} min={0.5} max={600} step={0.5} precision={1} unit="s" />}
            </>
          ) : (
            <>
              <Seg
                expose="speedMode"
                exposeValue={k.speedMode}
                exposeOptions={[
                  ['duration', t('Duration')],
                  ['pxs', 'px/s'],
                ]}
                label={t('Speed by')}
                value={k.speedMode === 'card' ? 'pxs' : k.speedMode}
                onChange={(x) => set((c) => void (c.speedMode = x))}
                options={[
                  ['duration', t('Duration')],
                  ['pxs', 'px/s'],
                ]}
              />
              {k.speedMode === 'duration' ? <Num label={t('Duration')} path="duration" value={k.duration} min={1} max={3600} unit="s" /> : <Num label={t('Speed')} path="pxs" value={k.pxs} min={1} max={5000} unit="px/s" />}
              <Num label={t('Max speed')} path="maxPxs" value={k.maxPxs} min={10} max={5000} step={10} unit="px/s" />
              <Toggle
                label={t('Whole px')}
                value={k.optimised}
                text={t('per frame')}
                onChange={(v) =>
                  edit((l) => {
                    (l as Config).optimised = v;
                    // At the project's frame rate.
                    const fr = useApp.getState().project?.frameRate;
                    if (v && fr) (l as Config).fps = fr.num / fr.den;
                  })
                }
              />
            </>
          )}
          <Seg
            label={t('Start')}
            path="start"
            value={k.start}
            options={[
              ['off', t('Off screen')],
              ['on', t('On screen')],
            ]}
          />
          <Seg
            label={t('End')}
            path="end"
            value={k.end}
            options={[
              ['exit', t('Exit')],
              ['stop', t('Stop')],
            ]}
          />
          {k.mode !== 'pages' && (
            <Seg
              label={t('If short')}
              path="shortMode"
              value={k.shortMode}
              options={[
                ['center', t('Centre')],
                ['exit', t('Exit')],
              ]}
            />
          )}
        </Section>
      </>
    );
  },
  // The Operator's preview holds it until its Play: not started yet.
  plays: () => 'start',
  Operator: (p) => <CreditsBar {...p} />,
  // A manual start on Continue waits for it (credits.rs starts it on F2), unless the operator started it himself.
  waits: (l, state) => !!l.manualStart && l.startOn !== 'trigger' && !(state as State | undefined)?.started,
  warnings: (layer, { data, state, r }) => {
    const l = layer as unknown as CL;
    const [w, h] = sizeOf(l, r as never);
    return warningsOf(l, data, state, w, h);
  },
});

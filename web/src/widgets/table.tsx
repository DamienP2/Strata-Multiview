// Table: data grid (manual grid with CSV paste, or a linked source: a {{key}} whose value is CSV / JSON rows),
// mapped columns, header row / column, merges, widths fixed / auto / %, styles per level, borders, conditional
// formatting, sort, pagination (pages or continuous scroll) and a cascade of the rows.
// Same picture as crates/cg-render/src/wid/table.rs; model and actions mirror crates/cg-core/src/wid/table.rs.
import { ChevronsRight, Pause, Pencil, Play, RefreshCw, SkipBack, SkipForward, Square, Table, Zap } from 'lucide-react';
import { Dropdown } from '../ui/Dropdown';
import { useEffect, useState } from 'react';
import { interpolateText, num as vnum, parseHex, toHex } from '../engine/evaluate';
import type { Data } from '../engine/types';
import { t, tc, tl, tn } from '../i18n';
import { useTableEditor } from './table-editor-store';
import { ColorField } from '../ui/fields';
import { clamp, defineWidget, easeOut, Handle, hex, parseCsv, RangeNum, Row, Seg, Text, Toggle, useBound, useCentreMode, useKeys, type Warning } from './kit';
import { ADVANCE_MODES, Centre, Countdown, Dots, GAP, WaitsContinue, WaitsTrigger, type AdvanceMode, type Line2Item } from './centre';
import { cased, TextCell, tw, useLiveTime, type CellText, type Overflow } from './table-text';
import { textWidth } from '../engine/textFit';

export interface Col {
  src?: string;
  label?: string;
  type?: 'text' | 'image' | 'number';
  align?: 'left' | 'center' | 'right';
  w?: number;
  pct?: number;
  fmt?: string;
  on?: boolean;
  /** A colour column of the data drawn as a small swatch before the text ('' = none). */
  mark?: string;
  /** Its own text (unset = the text of all the rows). */
  txt?: Typo;
  prefix?: string;
  suffix?: string;
  img?: Pic;
  /** Data columns whose value (a colour) paints this column's text / background, row by row ('' = none). */
  colorFrom?: string;
  bgFrom?: string;
}
/** Typography of a level of the table; unset = the level's default (mirrors `Typo` of the Rust module). */
export interface Typo {
  family?: string;
  weight?: number;
  size?: number;
  spacing?: number;
  case?: CellText['case'];
  italic?: boolean;
  deco?: CellText['deco'];
  lines?: 'one' | 'multi';
  max?: number;
  lh?: number;
  brk?: 'words' | 'any';
  fit?: CellText['fit'];
  narrow?: number;
  small?: number;
  then?: CellText['then'];
}
interface Pair {
  bg?: string;
  color?: string;
}
interface Line {
  on?: boolean;
  width?: number;
  color?: string;
}
export interface Config {
  grid?: string[][];
  link?: string;
  cols?: Col[];
  title?: { on?: boolean; text?: string };
  headerRow?: boolean;
  headerColOn?: boolean;
  headerCol?: string;
  showRank?: boolean;
  /** `key`: the row's key (its team), followed when the order changes; without it `row` is the rank. */
  merges?: { on?: boolean; key?: string; row?: number; col?: number; span?: number; text?: string }[];
  widthMode?: 'auto' | 'fixed' | 'percent';
  rowH?: number;
  pad?: number;
  gap?: number;
  headSize?: number;
  overflow?: Overflow;
  minSize?: number;
  minSqueeze?: number;
  st?: { header?: Pair; hcol?: Pair; odd?: Pair; even?: Pair; hl?: Pair; top3?: Pair; tableBg?: string };
  cell?: { on?: boolean; key?: string; row?: number; col?: number; bg?: string; color?: string };
  borders?: { outer?: Line; innerH?: Line; innerV?: Line };
  cf?: { neg?: boolean; negColor?: string; top3?: boolean; top3Color?: string; topN?: number; topBar?: boolean; hlBar?: boolean; hlBarColor?: string };
  sortCol?: string;
  sortDir?: 'desc' | 'asc';
  perPage?: number;
  repeatHeader?: boolean;
  pageMode?: 'pages' | 'scroll';
  scrollSpeed?: number;
  indicator?: boolean;
  /** Pages are chained from the take: every `autoSecs` seconds, on Continue, or on a trigger (`autoOn`). */
  autoPages?: boolean;
  autoSecs?: number;
  autoOn?: AdvanceMode;
  autoTrigger?: string;
  /** Key of the row highlighted from the take ('' = none). */
  hlKey?: string;
  cascade?: { on?: boolean; style?: 'slide' | 'fade' | 'flip'; offset?: number; dur?: number };
  emptyMode?: 'message' | 'hide';
  emptyText?: string;
  txt?: { title?: Typo; header?: Typo; rows?: Typo; rank?: Typo };
  /** Weights of the kinds of rows (0 = the rows' weight). */
  kw?: { odd?: number; even?: number; hl?: number; top3?: number };
  titleH?: number;
  /** '' = the default gradient. */
  titleBg?: string;
  titleColor?: string;
  /** 0 = 85 % of a row. */
  headH?: number;
  titleFrom?: string;
  titleTo?: string;
  titleAlign?: 'left' | 'center' | 'right';
  titlePad?: number;
  /** Unset = each column's alignment. */
  headAlign?: 'left' | 'center' | 'right' | null;
  /** Corners of the cells, px. */
  radius?: number;
  indicatorColor?: string;
  /** A data column whose value (a colour) marks each row ('' = none), and how. */
  rowFrom?: string;
  rowPaint?: 'bar' | 'bg' | 'text';
}
/** How a picture column draws its pictures (like the Image widget; mirrors `Pic`). */
export interface Pic {
  fit?: 'contain' | 'cover' | 'stretch';
  size?: number;
  width?: number;
  mask?: 'none' | 'rounded' | 'circle';
  radius?: number;
  opacity?: number;
  fallback?: string;
}

/** Mirrors `State` of the Rust module. */
export interface State {
  /** The operator acted (else the Designer's auto pages and highlighted row apply from the take). */
  ctl?: boolean;
  page?: number;
  pageAt?: number;
  auto?: boolean;
  interval?: number;
  autoAt?: number;
  /** How the pages go on when chained (timed: every `interval` s). */
  on?: AdvanceMode;
  trigger?: string;
  pausedAt?: number;
  hl?: string;
}

// ------------------------------------------------------------ defaults --
const P = (bg: string, color: string) => ({ bg, color });
export const DST = { header: P('#7c5cff', '#ffffff'), hcol: P('#232a4d', '#ffffff'), odd: P('#141a33', '#ffffff'), even: P('#1c2344', '#ffffff'), hl: P('#ff3b5c', '#ffffff'), top3: P('#2c2717', '#fff4cc'), tableBg: '#060812d9' };
const DCOL: Required<Omit<Col, 'txt' | 'img'>> & { txt?: Typo; img?: Pic } = { src: '', label: '', type: 'text', align: 'left', w: 100, pct: 10, fmt: '0', on: true, mark: '', prefix: '', suffix: '', colorFrom: '', bgFrom: '' };
export function cfg(c: Config) {
  const st = c.st ?? {};
  const b = c.borders ?? {};
  return {
    grid: c.grid ?? [],
    link: c.link ?? '',
    cols: (c.cols ?? []).map((x) => ({ ...DCOL, ...x })),
    // '' = « Classement » / "Standings", in the project's language.
    title: { on: false, text: '', ...c.title },
    headerRow: c.headerRow ?? true,
    headerColOn: c.headerColOn ?? false,
    headerCol: c.headerCol ?? '',
    showRank: c.showRank ?? false,
    merges: (c.merges ?? []).map((g) => ({ on: true, key: '', row: 1, col: 1, span: 2, text: '', ...g })),
    widthMode: c.widthMode ?? 'auto',
    rowH: c.rowH ?? 66,
    pad: c.pad ?? 18,
    gap: c.gap ?? 4,
    headSize: c.headSize ?? 20,
    overflow: c.overflow ?? 'shrink',
    minSize: c.minSize ?? 20,
    minSqueeze: c.minSqueeze ?? 70,
    st: { header: { ...DST.header, ...st.header }, hcol: { ...DST.hcol, ...st.hcol }, odd: { ...DST.odd, ...st.odd }, even: { ...DST.even, ...st.even }, hl: { ...DST.hl, ...st.hl }, top3: { ...DST.top3, ...st.top3 }, tableBg: st.tableBg ?? DST.tableBg },
    cell: { on: false, key: '', row: 1, col: 1, bg: '#facc15', color: '#111111', ...c.cell },
    borders: { outer: { on: false, width: 4, color: '#ffffff', ...b.outer }, innerH: { on: true, width: 1, color: '#ffffff14', ...b.innerH }, innerV: { on: false, width: 1, color: '#ffffff1f', ...b.innerV } },
    cf: { neg: true, negColor: '#ff6b81', top3: false, top3Color: '#facc15', topN: 3, topBar: true, hlBar: true, hlBarColor: '#ffffff', ...c.cf },
    sortCol: c.sortCol ?? '',
    sortDir: c.sortDir ?? 'desc',
    perPage: Math.max(1, c.perPage ?? 7),
    repeatHeader: c.repeatHeader ?? true,
    pageMode: c.pageMode ?? 'pages',
    scrollSpeed: c.scrollSpeed ?? 50,
    indicator: c.indicator ?? true,
    autoPages: c.autoPages ?? false,
    autoSecs: c.autoSecs ?? 6,
    autoOn: c.autoOn ?? 'timed',
    autoTrigger: c.autoTrigger ?? '',
    hlKey: c.hlKey ?? '',
    cascade: { on: true, style: 'slide' as const, offset: 70, dur: 450, ...c.cascade },
    emptyMode: c.emptyMode ?? 'message',
    emptyText: c.emptyText ?? '',
    txt: { title: c.txt?.title ?? {}, header: c.txt?.header ?? {}, rows: c.txt?.rows ?? {}, rank: c.txt?.rank },
    kw: { odd: 0, even: 0, hl: 0, top3: 0, ...c.kw },
    titleH: c.titleH && c.titleH > 0 ? c.titleH : 64,
    titleBg: c.titleBg ?? '',
    titleColor: c.titleColor ?? '#ffffff',
    headH: c.headH ?? 0,
    titleFrom: c.titleFrom ?? '#ff3b5c',
    titleTo: c.titleTo ?? '#7c5cff',
    titleAlign: c.titleAlign ?? 'left',
    titlePad: c.titlePad ?? 24,
    headAlign: c.headAlign ?? null,
    radius: c.radius ?? 0,
    indicatorColor: c.indicatorColor ?? '#ff3b5c',
    rowFrom: c.rowFrom ?? '',
    rowPaint: c.rowPaint ?? 'bar',
  };
}

// --------------------------------------------------------------- texts --
/** `t` over `b` (mirrors Text::over). */
export function over(b: CellText, t: Typo | undefined): CellText {
  if (!t) return b;
  const clamp10 = (v: number) => Math.min(100, Math.max(10, v));
  return {
    family: t.family || b.family,
    weight: t.weight ?? b.weight,
    size: t.size && t.size > 0 ? t.size : b.size,
    spacing: t.spacing ?? b.spacing,
    case: t.case ?? b.case,
    italic: t.italic ?? b.italic,
    deco: t.deco ?? b.deco,
    multi: t.lines ? t.lines === 'multi' : b.multi,
    max: Math.max(1, t.max ?? b.max),
    lh: t.lh && t.lh > 0 ? t.lh : b.lh,
    anyBreak: t.brk ? t.brk === 'any' : b.anyBreak,
    fit: t.fit ?? b.fit,
    narrow: clamp10(t.narrow ?? b.narrow),
    small: clamp10(t.small ?? b.small),
    then: t.then ?? b.then,
  };
}
function base(k: K, size: number, weight: number, spacing: number, cs: CellText['case']): CellText {
  const [fit, narrow, small]: [CellText['fit'], number, number] = k.overflow === 'ellipsis' ? ['ellipsis', 100, 100] : k.overflow === 'squeeze' ? ['fit', k.minSqueeze, 100] : ['fit', k.minSqueeze, Math.min(100, (k.minSize / Math.max(1, size)) * 100)];
  return { family: 'Inter', weight, size, spacing, case: cs, italic: false, deco: 'none', multi: false, max: 2, lh: 1.1, anyBreak: false, fit, narrow, small, then: 'ellipsis' };
}
export const titleText = (k: K) => over({ ...base(k, 30, 900, 1.2, 'upper'), fit: 'fit', narrow: 70, small: 60 }, k.txt.title);
export const headerText = (k: K) => over(base(k, k.headSize, 800, k.headSize * 0.05, 'upper'), k.txt.header);
export const rowsText = (k: K, size: number) => over(base(k, size, 700, 0, 'none'), k.txt.rows);
export function colText(k: K, mc: MCol, size: number): CellText {
  const rows = rowsText(k, size);
  if (mc.col.type === 'number' && k.txt.rows.weight === undefined) rows.weight = 800;
  return over(rows, mc.i === undefined ? k.txt.rank : mc.col.txt);
}
const cellW = (t: CellText) => (s: string) => textWidth(cased(t, s), { fontFamily: t.family, fontWeight: t.weight, letterSpacing: t.spacing }, t.size);
type K = ReturnType<typeof cfg>;

// ---------------------------------------------------------------- data --
const jsonCell = (v: unknown) => (v == null ? '' : typeof v === 'string' ? v : JSON.stringify(v));
/** Rows of a linked value: JSON array (of arrays, or of objects: keys = header) or CSV. */
export function parseRows(text: string): string[][] {
  const t = text.trim();
  if (t.startsWith('[') || t.startsWith('{')) {
    try {
      const v = JSON.parse(t);
      const arr: unknown[] = Array.isArray(v) ? v : Array.isArray(v?.rows) ? v.rows : Array.isArray(v?.data) ? v.data : [];
      if (arr.every((r) => Array.isArray(r))) return (arr as unknown[][]).map((r) => r.map(jsonCell));
      const head: string[] = [];
      arr.forEach((r) => r && typeof r === 'object' && Object.keys(r).forEach((k) => !head.includes(k) && head.push(k)));
      return [head, ...arr.filter((r) => r && typeof r === 'object').map((r) => head.map((k) => jsonCell((r as Record<string, unknown>)[k])))];
    } catch {
      /* CSV */
    }
  }
  return parseCsv(text);
}
export const gridOf = (k: K, data: Data) => (k.link.trim() ? parseRows(interpolateText(k.link, data)) : k.grid.map((r) => r.map((s) => interpolateText(String(s ?? ''), data))));
export function num(s: string): number | undefined {
  const t = String(s ?? '')
    .replace(/[\s  ]/g, '')
    .replace('−', '-')
    .replace(',', '.')
    .replace(/%+$/, '');
  if (!t) return undefined;
  const n = Number(t);
  return Number.isFinite(n) ? n : undefined;
}
export function fmtNum(s: string, f: string): string {
  const n = num(s);
  if (n === undefined) return s;
  const minus = n < 0 ? '−' : '';
  if (f === '0.0') return minus + Math.abs(n).toFixed(1);
  if (f === '+0') {
    const r = Math.round(n);
    return (r > 0 ? '+' : r < 0 ? '−' : '') + Math.abs(r);
  }
  if (f === '0 %') return `${minus}${Math.abs(Math.round(n))} %`;
  const r = Math.abs(Math.round(n));
  return minus + (r >= 1000 ? String(r).replace(/\B(?=(\d{3})+(?!\d))/g, ' ') : String(r));
}
export interface MCol {
  col: typeof DCOL;
  /** Index in the columns of the settings (or of the data, when they follow it). */
  ci?: number;
  i?: number;
}
export interface MRow {
  r: string[];
  k: number;
  key: string;
  rank: number;
}
const RANK: typeof DCOL = { ...DCOL, label: '#', type: 'number', align: 'center', w: 80, pct: 7 };
export function model(k: K, data: Data) {
  const g = gridOf(k, data);
  const head = (g[0] ?? []).map((h) => String(h ?? '').trim());
  const body = g.slice(1).filter((r) => r.some((c) => String(c ?? '').trim() !== ''));
  const auto = !k.cols.length;
  const mapped = auto ? head.filter(Boolean).map((h) => ({ ...DCOL, src: h, label: h })) : k.cols;
  const cols: MCol[] = [];
  const missing: string[] = [];
  mapped.forEach((c, ci) => {
    if (!c.on) return;
    const i = head.indexOf(c.src);
    if (i < 0) missing.push(c.src);
    else cols.push({ col: c, i, ci });
  });
  if (k.showRank) cols.unshift({ col: RANK });
  const unmapped = auto ? [] : head.filter((h) => h && h !== k.rowFrom && !k.cols.some((c) => c.src === h || (c.on && [c.mark, c.colorFrom, c.bgFrom].includes(h))));
  const keyCol = cols.find((m) => m.i !== undefined && k.headerColOn && m.col.src === k.headerCol) ?? cols.find((m) => m.col.type === 'text' && m.i !== undefined);
  const rows: MRow[] = body.map((r, i) => ({ r, k: i, key: keyCol ? String(r[keyCol.i!] ?? '') : String(i + 1), rank: 0 }));
  const sc = cols.find((m) => m.i !== undefined && k.sortCol && m.col.src === k.sortCol);
  if (sc) {
    const dir = k.sortDir === 'asc' ? 1 : -1;
    rows.sort((a, b) => {
      const x = a.r[sc.i!] ?? '';
      const y = b.r[sc.i!] ?? '';
      const nx = num(x);
      const ny = num(y);
      const d = nx !== undefined && ny !== undefined ? nx - ny : x.toLowerCase() < y.toLowerCase() ? -1 : x.toLowerCase() > y.toLowerCase() ? 1 : 0;
      return d * dir || a.k - b.k;
    });
  }
  rows.forEach((r, i) => (r.rank = i + 1));
  return { head, cols, rows, unmapped, missing };
}
type M = ReturnType<typeof model>;
export const pageCount = (k: K, m: M) => Math.max(1, Math.ceil(m.rows.length / k.perPage));
export function cellText(c: MCol, row: MRow): string {
  if (c.i === undefined) return String(row.rank);
  const v = c.col.type === 'number' ? fmtNum(row.r[c.i] ?? '', c.col.fmt) : String(row.r[c.i] ?? '');
  return c.col.type === 'image' || !v.trim() ? v : `${c.col.prefix ?? ''}${v}${c.col.suffix ?? ''}`;
}
/** A setting aimed at a row: by its key (it follows the team), else by rank (mirrors aims). */
export const aims = (key: string | undefined, rank: number, row: MRow) => (key?.trim() ? row.key === key.trim() : +rank === row.rank);
/** The single cell styled, when it is in this row: its column, 0-based (mirrors cell_in). */
export const cellIn = (k: K, row: MRow) => (k.cell.on && aims(k.cell.key, k.cell.row, row) ? Math.max(1, +k.cell.col) - 1 : -1);
function mergeOf(k: K, m: M, row: MRow): [number, number, string] | undefined {
  const g = k.merges.find((x) => x.on && aims(x.key, x.row, row));
  if (!g) return;
  const j = Math.max(1, +g.col) - 1;
  if (j >= m.cols.length) return;
  return [j, Math.min(Math.max(1, +g.span), m.cols.length - j), g.text];
}
export const headerH = (k: K) => (k.headH > 0 ? k.headH : Math.round(k.rowH * 0.85));
export const titleH = (k: K) => k.titleH;

/** The state on air: the operator's once he acted, else the Designer's auto pages (from the take) and highlighted row (mirrors live). */
export function live(k: K, st: State, now: number, ageSecs: number): State {
  if (st.ctl) return st;
  const s = { ...st };
  if (!s.hl) s.hl = k.hlKey.trim();
  if (k.autoPages && !s.auto && k.pageMode === 'pages') {
    s.on = k.autoOn;
    s.trigger = k.autoTrigger.trim();
    if (k.autoOn === 'timed') {
      if (k.autoSecs > 0) Object.assign(s, { auto: true, interval: clamp(k.autoSecs, 0.5, 3600), autoAt: Math.max(1, now - Math.trunc(Math.max(0, ageSecs) * 1000)) });
    } else Object.assign(s, { auto: true, interval: 0 });
  }
  return s;
}

/** Page shown at `now` and when it appeared (0 = since on air), mirrors current_page. */
export function currentPage(st: State, pages: number, now: number): [number, number] {
  pages = Math.max(1, pages);
  const page = st.page ?? 0;
  if (st.auto && (st.interval ?? 0) > 0 && (st.autoAt ?? 0) > 0 && pages > 1) {
    const t = st.pausedAt ? st.pausedAt : now;
    const step = Math.trunc(st.interval! * 1000);
    const n = Math.floor(Math.max(0, t - st.autoAt!) / Math.max(1, step));
    return [(page + n) % pages, n > 0 ? st.autoAt! + n * step : (st.pageAt ?? 0)];
  }
  return [Math.min(page, pages - 1), st.pageAt ?? 0];
}
/** Column widths (mirrors widths() of the renderer). */
export function widths(k: K, m: M, W: number, size: number): number[] {
  const n = m.cols.length;
  const avail = W - k.gap * Math.max(0, n - 1);
  if (k.widthMode === 'fixed') return m.cols.map((c) => c.col.w);
  if (k.widthMode === 'percent') {
    // Shares of the width: always 100 % in total, whatever the numbers typed.
    const rk = m.cols.some((c) => c.i === undefined) ? 80 : 0;
    const sum = m.cols.reduce((a, c) => (c.i === undefined ? a : a + Math.max(0, +c.col.pct || 0)), 0) || 1;
    return m.cols.map((c) => (c.i === undefined ? 80 : (Math.max(0, +c.col.pct || 0) / sum) * (avail - rk)));
  }
  const nat = m.cols.map((c) => {
    if (c.col.type === 'image') return k.rowH * 0.75 + 2 * k.pad;
    const hw = cellW(headerText(k));
    const cw = cellW(colText(k, c, size));
    let w = hw(c.col.label);
    for (const r of m.rows) w = Math.max(w, cw(cellText(c, r)));
    return Math.min(w, 600) + 2 * k.pad + 8;
  });
  const isText = m.cols.map((c) => c.col.type === 'text' && c.i !== undefined);
  const fixed = nat.reduce((a, w, i) => a + (isText[i] ? 0 : w), 0);
  const textNat = nat.reduce((a, w, i) => a + (isText[i] ? w : 0), 0);
  const rest = avail - fixed;
  if (textNat <= 0 || rest < textNat * 0.25) {
    const sum = Math.max(
      1,
      nat.reduce((a, b) => a + b, 0),
    );
    return nat.map((w) => (w * avail) / sum);
  }
  return nat.map((w, i) => (isText[i] ? (w * rest) / textNat : w));
}

/** A picture in its cell, set like the Image widget (mirrors cell_picture of cg-render table.rs). */
export function CellPicture({ v, p = {}, align, box: [x, y, w, h] }: { v: string; p?: Pic; align: 'left' | 'center' | 'right'; box: [number, number, number, number] }) {
  const [nat, setNat] = useState<[number, number] | 'missing' | null>(null);
  const s = (v.trim() || p.fallback?.trim() || '').trim();
  // A picture that does not load shows as a chip, like on air.
  const isImg = (s.startsWith('/') || s.includes('.')) && nat !== 'missing';
  useEffect(() => {
    if (!(s.startsWith('/') || s.includes('.'))) return setNat(null);
    const im = new Image();
    im.onload = () => setNat([im.naturalWidth, im.naturalHeight]);
    im.onerror = () => setNat('missing');
    im.src = s;
  }, [s]);
  if (!s || w <= 1) return null;
  const fit = p.fit ?? 'contain';
  const mask = p.mask ?? 'none';
  const bh = Math.round((h * Math.min(100, Math.max(5, p.size ?? 50))) / 100);
  let bw = mask === 'circle' ? bh : fit === 'contain' ? (Array.isArray(nat) && isImg ? (nat[0] * bh) / Math.max(1, nat[1]) : bh * 1.5) : (w * Math.min(100, Math.max(5, p.width ?? 80))) / 100;
  bw = Math.min(bw, w);
  const bx = align === 'left' ? x : align === 'right' ? x + w - bw : x + (w - bw) / 2;
  const r = mask === 'rounded' ? (p.radius ?? 8) : mask === 'circle' ? bh / 2 : 0;
  const style: React.CSSProperties = { position: 'absolute', left: bx, top: y + (h - bh) / 2, width: bw, height: bh, borderRadius: Math.min(r, bw / 2, bh / 2), overflow: 'hidden', opacity: Math.min(1, Math.max(0, (p.opacity ?? 100) / 100)) };
  if (isImg) return <img src={s} alt="" draggable={false} style={{ ...style, objectFit: fit === 'stretch' ? 'fill' : fit, display: 'block' }} />;
  return <div style={{ ...style, borderRadius: Math.max(r, bh * 0.1), background: '#334155', color: '#cbd5e1', display: 'grid', placeItems: 'center', font: `700 ${bh * 0.35}px Inter, sans-serif` }}>{s.slice(0, 3).toUpperCase()}</div>;
}

/** A picture cell: an asset path, else a grey chip with the code. */
export function Picture({ v, cx, cy, h }: { v: string; cx: number; cy: number; h: number }) {
  const s = v.trim();
  if (!s) return null;
  if (s.startsWith('/') || s.includes('.')) return <img src={s} alt="" draggable={false} style={{ position: 'absolute', left: cx, top: cy - h / 2, height: h, transform: 'translateX(-50%)', display: 'block' }} />;
  const w = h * 1.5;
  return <div style={{ position: 'absolute', left: cx - w / 2, top: cy - h / 2, width: w, height: h, borderRadius: h * 0.1, background: '#334155', color: '#cbd5e1', display: 'grid', placeItems: 'center', font: `700 ${h * 0.35}px Inter, sans-serif` }}>{s.slice(0, 3).toUpperCase()}</div>;
}

// ------------------------------------------------------------- render --
/** The texts the table writes itself, in the project's language, unless the Designer typed its own (mirrors title_or / empty_or). */
export const titleOr = (k: K, lang?: string) => k.title.text.trim() || tl(lang, 'Standings');
export const emptyOr = (k: K, lang?: string) => k.emptyText.trim() || tl(lang, 'No data yet');

export function TableRender({ layer, r, w, data, time, state, lang }: { layer: Config; r: Record<string, unknown>; w: number; data: Data; time: { nowMs: number; ageSecs: number }; state: unknown; lang?: string }) {
  const k = cfg(layer);
  const m = model(k, data);
  const st = live(k, (state ?? {}) as State, time.nowMs, time.ageSecs);
  const size = Math.max(1, vnum(r.fontSize as number));
  if (!m.rows.length && k.emptyMode === 'hide') return null;
  const pages = pageCount(k, m);
  const [page, at] = currentPage(st, pages, time.nowMs);
  const since = at > 0 ? (time.nowMs - at) / 1000 : time.ageSecs;
  const scroll = k.pageMode === 'scroll';
  const ws = widths(k, m, w, size);
  const xs: number[] = [];
  ws.reduce((x, ww, i) => ((xs[i] = x), x + ww + k.gap), 0);
  const { rowH: rh, gap, pad } = k;
  const hh = headerH(k);
  const showHead = k.headerRow && (scroll || k.repeatHeader || page === 0);
  const rowsT = rowsText(k, size);
  const colTs = m.cols.map((c) => colText(k, c, size));
  const TH = titleH(k);
  const per = k.perPage;
  const list = scroll ? m.rows : m.rows.slice(page * per, page * per + per);
  const shownN = scroll ? per : list.length;
  const clipH = !m.rows.length ? rh * 2 : Math.max(0, Math.min(per, shownN) * (rh + gap) - gap);
  let y = 0;
  if (k.title.on) y += TH + gap;
  if (showHead) y += hh + gap;
  const y0 = y;
  const totalH = y0 + clipH;
  const bH = k.borders.innerH;
  const bV = k.borders.innerV;
  const lines = (top: boolean, left: boolean, x: number, yy: number, cw: number, ch: number, key: string) => (
    <>
      {top && bH.on && <div key={`${key}h`} style={{ position: 'absolute', left: x, top: yy, width: cw, height: Math.min(bH.width, ch), background: hex(bH.color) }} />}
      {left && bV.on && <div key={`${key}v`} style={{ position: 'absolute', left: x, top: yy, width: Math.min(bV.width, cw), height: ch, background: hex(bV.color) }} />}
    </>
  );
  const out: React.ReactNode[] = [];
  out.push(<div key="bg" style={{ position: 'absolute', left: 0, top: 0, width: w, height: totalH, background: hex(k.st.tableBg) }} />);
  let yy = 0;
  if (k.title.on) {
    out.push(<div key="title" style={{ position: 'absolute', left: 0, top: 0, width: w, height: TH, background: k.titleBg.trim() ? hex(k.titleBg) : `linear-gradient(90deg,${hex(k.titleFrom)},${hex(k.titleTo)})` }} />);
    out.push(<TextCell key="tt" t={titleText(k)} text={titleOr(k, lang)} align={k.titleAlign} box={[k.titlePad, 0, w - 2 * k.titlePad, TH]} color={hex(k.titleColor)} />);
    yy += TH + gap;
  }
  if (showHead) {
    const top = yy;
    const headT = headerText(k);
    m.cols.forEach((c, j) => {
      out.push(<div key={`h${j}`} style={{ position: 'absolute', left: xs[j], top, width: ws[j], height: hh, background: hex(k.st.header.bg), borderRadius: k.radius }} />);
      out.push(<span key={`hl${j}`}>{lines(false, j > 0, xs[j], top, ws[j], hh, `h${j}`)}</span>);
      out.push(<TextCell key={`ht${j}`} t={headT} text={c.col.label} align={k.headAlign ?? c.col.align} box={[xs[j] + pad, top, ws[j] - 2 * pad, hh]} color={hex(k.st.header.color)} />);
    });
    yy += hh + gap;
  }
  if (!m.rows.length) {
    out.push(
      <div key="empty" style={{ position: 'absolute', left: 0, top: y0, width: w, height: rh * 2, background: hex(k.st.odd.bg), opacity: 0.85 }}>
        <TextCell t={rowsT} text={emptyOr(k, lang)} align="center" box={[pad, 0, w - 2 * pad, rh * 2]} color={hex(k.st.odd.color)} />
      </div>,
    );
  } else {
    const step = rh + gap;
    const looped = scroll && m.rows.length > per;
    const off = looped ? (((k.scrollSpeed * time.ageSecs) % (m.rows.length * step)) + m.rows.length * step) % (m.rows.length * step) : 0;
    const shown = looped ? [...list, ...list] : list;
    const rowsOut: React.ReactNode[] = [];
    shown.forEach((row, i) => {
      const ry = i * step - off;
      if (ry > clipH || ry + rh < 0) return;
      let transform: string | undefined;
      let opacity = 1;
      if (!scroll && k.cascade.on) {
        const kk = easeOut(clamp((since * 1000 - i * k.cascade.offset) / Math.max(1, k.cascade.dur), 0, 1));
        if (k.cascade.style === 'slide') ((transform = `translateX(${(1 - kk) * -80}px)`), (opacity = kk));
        else if (k.cascade.style === 'fade') opacity = kk;
        else ((transform = `scaleY(${Math.max(0.001, Math.cos(((1 - kk) * 90 * Math.PI) / 180))})`), (opacity = kk > 0 ? 1 : 0));
        if (opacity <= 0.001) return;
      }
      const hl = !!row.key && row.key === st.hl;
      const top3 = !hl && k.cf.top3 && row.rank <= Math.max(1, k.cf.topN);
      const base = { ...(hl ? k.st.hl : top3 ? k.st.top3 : row.rank % 2 ? k.st.odd : k.st.even) };
      // The row's own colour, from a column of the data (mirrors cell_color).
      const cellOf = (name?: string) => {
        if (!name) return null;
        const i = m.head.indexOf(name);
        const v = i >= 0 ? String(row.r[i] ?? '').trim() : '';
        return /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(v) ? v : null;
      };
      const rowColor = cellOf(k.rowFrom);
      if (rowColor && !hl && k.rowPaint === 'bg') base.bg = rowColor;
      if (rowColor && !hl && k.rowPaint === 'text') base.color = rowColor;
      const merge = mergeOf(k, m, row);
      let skipTo = 0;
      const cells: React.ReactNode[] = [];
      m.cols.forEach((c, j) => {
        if (j < skipTo) return;
        let cw = ws[j];
        let txt = cellText(c, row);
        let align = c.col.align;
        let type = c.col.type;
        if (merge && merge[0] === j) {
          for (let q = 1; q < merge[1]; q++) cw += ws[j + q] + gap;
          skipTo = j + merge[1];
          if (merge[2]) ((txt = merge[2]), (type = 'text'));
          align = 'center';
        }
        let bg = base.bg;
        let color = base.color;
        if (!hl && k.headerColOn && c.i !== undefined && c.col.src === k.headerCol) ((bg = k.st.hcol.bg), (color = k.st.hcol.color));
        if (type === 'number' && c.i !== undefined && k.cf.neg && !hl && (num(row.r[c.i] ?? '') ?? 0) < 0) color = k.cf.negColor;
        if (c.i === undefined && top3) color = k.cf.top3Color;
        if (!hl) ((color = cellOf(c.col.colorFrom) ?? color), (bg = cellOf(c.col.bgFrom) ?? bg));
        if (cellIn(k, row) === j) ((bg = k.cell.bg), (color = k.cell.color));
        const x = xs[j];
        cells.push(<div key={`b${j}`} style={{ position: 'absolute', left: x, top: 0, width: cw, height: rh, background: hex(bg), borderRadius: k.radius }} />);
        cells.push(<span key={`l${j}`}>{lines(i > 0 || showHead || k.title.on, j > 0, x, 0, cw, rh, `c${j}`)}</span>);
        if (j === 0 && top3 && k.cf.topBar) cells.push(<div key="t3" style={{ position: 'absolute', left: x, top: 0, width: 8, height: rh, background: hex(k.cf.top3Color) }} />);
        if (j === 0 && rowColor && !hl && k.rowPaint === 'bar') cells.push(<div key="rc" style={{ position: 'absolute', left: x, top: 0, width: 8, height: rh, background: hex(rowColor) }} />);
        if (j === 0 && hl && k.cf.hlBar) cells.push(<div key="hl" style={{ position: 'absolute', left: x, top: 0, width: 8, height: rh, background: hex(k.cf.hlBarColor) }} />);
        // A colour swatch before the text (a colour column of the data).
        let tx0 = x + pad;
        const mi = type === 'text' && c.col.mark ? m.head.indexOf(c.col.mark) : -1;
        const mv = mi >= 0 ? String(row.r[mi] ?? '').trim() : '';
        if (mv) {
          const s = Math.round(rh * 0.3);
          cells.push(<div key={`m${j}`} style={{ position: 'absolute', left: tx0, top: (rh - s) / 2, width: s, height: s, borderRadius: s * 0.2, background: hex(mv) }} />);
          tx0 += s * 1.6;
        }
        if (type === 'image') cells.push(<CellPicture key={`p${j}`} v={txt} p={c.col.img} align={align} box={[x + pad, 0, cw - 2 * pad, rh]} />);
        else {
          const t = merge && merge[0] === j ? { ...rowsT } : { ...colTs[j] };
          const kw = hl ? k.kw.hl : top3 ? k.kw.top3 : row.rank % 2 ? k.kw.odd : k.kw.even;
          if (kw) t.weight = kw;
          cells.push(<TextCell key={`t${j}`} t={t} text={txt} align={align} box={[tx0, 0, cw - pad - (tx0 - x), rh]} color={hex(color)} style={type === 'number' ? { fontVariantNumeric: 'tabular-nums' } : undefined} />);
        }
      });
      rowsOut.push(
        <div key={`r${i}`} style={{ position: 'absolute', left: 0, top: ry, width: w, height: rh, transform, transformOrigin: 'left top', opacity }}>
          {cells}
        </div>,
      );
    });
    out.push(
      <div key="rows" style={{ position: 'absolute', left: 0, top: y0, width: w, height: clipH, overflow: 'hidden' }}>
        {rowsOut}
      </div>,
    );
  }
  if (k.borders.outer.on) out.push(<div key="outer" style={{ position: 'absolute', left: 0, top: 0, width: w, height: totalH, boxShadow: `inset 0 0 0 ${k.borders.outer.width}px ${hex(k.borders.outer.color)}` }} />);
  if (k.indicator && pages > 1 && !scroll && m.rows.length)
    out.push(
      <div key="ind" style={{ position: 'absolute', right: 0, top: totalH + 14, display: 'flex', alignItems: 'center', gap: 8, font: `800 24px ${'Inter, sans-serif'}`, lineHeight: 1.2, color: '#fff', background: 'rgba(11,13,24,.8)', padding: '8px 16px', borderRadius: 24, whiteSpace: 'nowrap' }}>
        {Array.from({ length: pages }, (_, i) => (
          <i key={i} style={{ width: i === page ? 30 : 12, height: 12, borderRadius: 6, background: i === page ? hex(k.indicatorColor) : 'rgba(255,255,255,.35)', display: 'inline-block' }} />
        ))}
        <span style={{ marginLeft: 6 }}>
          {page + 1} / {pages}
        </span>
      </div>,
    );
  return <div style={{ position: 'absolute', inset: 0, overflow: 'hidden' }}>{out}</div>;
}

// ------------------------------------------------------------ inspector --
/** Background · text colours of a level. */
export function PairRow({ label, path }: { label: string; path: string }) {
  const b = useBound();
  const v = (path.split('.').reduce<unknown>((a, x) => (a as Record<string, unknown> | undefined)?.[x], b.obj) ?? {}) as Pair;
  const d = path.split('.').reduce<unknown>((a, x) => (a as Record<string, unknown> | undefined)?.[x], { st: DST, cell: { bg: '#facc15', color: '#111111' } }) as Pair;
  const pick = (k: 'bg' | 'color') => <ColorField value={parseHex(v[k] ?? d?.[k] ?? '#ffffff') ?? [1, 1, 1, 1]} onChange={(c) => b.set(`${path}.${k}`, toHex(c, c[3] < 1), true)} />;
  return (
    <Row expose={[`${path}.bg`, `${path}.color`]} exposeValue={{ [`${path}.bg`]: v.bg ?? d?.bg, [`${path}.color`]: v.color ?? d?.color }} label={label}>
      <div style={{ display: 'flex', gap: 6, flex: 1, minWidth: 0 }}>
        <div style={{ flex: 1, minWidth: 0, overflow: 'hidden' }}>{pick('bg')}</div>
        <div style={{ flex: 1, minWidth: 0, overflow: 'hidden' }}>{pick('color')}</div>
      </div>
    </Row>
  );
}
export const FMTS: [string, string][] = [
  ['0', '1 234'],
  ['0.0', '12.3'],
  ['+0', '+5 / −3'],
  ['0 %', '45 %'],
];

type TL = Config & { props: Record<string, unknown>; id: string };
const sizeOf = (l: TL) => vnum((l.props.fontSize as number) ?? 30);

function warningsOf(layer: TL, data: Data, state: unknown, W: number, H: number): Warning[] {
  const k = cfg(layer);
  const m = model(k, data);
  const out: Warning[] = [];
  m.unmapped.forEach((h) => out.push({ level: 'warn', text: t("New column '{name}' in the data — not mapped (ignored until mapped in Columns).", { name: h }), op: true }));
  if (m.missing.length) out.push({ level: 'error', text: tn(m.missing.length, 'Mapped column not in the data: {names} — not shown.', 'Mapped columns not in the data: {names} — not shown.', { names: m.missing.join(', ') }) });
  if (!m.rows.length) out.push({ level: 'info', text: k.emptyMode === 'hide' ? t('No data: the table is hidden.') : t('No data: the fallback message is on air.'), op: true });
  const pages = pageCount(k, m);
  const [page] = currentPage(live(k, (state ?? {}) as State, Date.now(), 0), pages, Date.now());
  if (pages > 1 && k.pageMode === 'pages') out.push({ level: 'info', text: t('{rows} rows for {per} per page: automatic pagination, page {page}/{pages}.', { rows: m.rows.length, per: k.perPage, page: page + 1, pages }) });
  const size = sizeOf(layer);
  const ws = widths(k, m, W, size);
  if (k.widthMode === 'fixed') {
    const sum = ws.reduce((a, b) => a + b, 0) + k.gap * Math.max(0, m.cols.length - 1);
    if (sum > W + 0.5) out.push({ level: 'warn', text: t('Fixed widths total {sum} px for a {w} px box: the right columns are cut.', { sum: Math.round(sum), w: Math.round(W) }) });
  }
  const need = (k.title.on ? titleH(k) + k.gap : 0) + (k.headerRow ? headerH(k) + k.gap : 0) + Math.min(k.perPage, Math.max(1, m.rows.length)) * (k.rowH + k.gap) + (k.indicator && pages > 1 && k.pageMode === 'pages' ? 70 : 0);
  if (need > H + 1) out.push({ level: 'warn', text: t('The table ({need} px) is taller than the box ({h} px): fewer rows per page or a smaller row height.', { need: Math.round(need), h: Math.round(H) }) });
  let cut = 0;
  m.rows.forEach((r) => m.cols.forEach((c, j) => c.col.type === 'text' && c.i !== undefined && tw(cellText(c, r), size, 700) > ws[j] - 2 * k.pad && cut++));
  if (cut) {
    const how = k.overflow === 'shrink' ? t('shrunk (then squeezed / ellipsis at the minimum)') : k.overflow === 'squeeze' ? t('squeezed (then ellipsis at the minimum)') : t('cut with an ellipsis');
    out.push({ level: 'info', text: tn(cut, '{n} cell too long: {how}.', '{n} cells too long: {how}.', { how }) });
  }
  return out;
}

export default defineWidget({
  id: 'table',
  type: 'table',
  label: 'Table',
  group: 'Data',
  icon: Table,
  create: () => ({
    grid: [
      ['Name', 'Value'],
      ['Alpha', '1'],
      ['Beta', '2'],
      ['Gamma', '3'],
    ],
    st: { header: { bg: '#00000000', color: '#ffffffb3' }, even: { ...DST.odd } },
    borders: { innerH: { on: false, width: 1, color: '#ffffff14' } },
    cf: { neg: false },
    cascade: { on: false },
    // A standings table turns its pages by itself from the In.
    autoPages: true,
    autoSecs: 6,
    props: { width: 1200, height: 400, fontSize: 30 },
  }),
  ownsLook: true,
  content: ['link', 'grid'],
  live: (l) => {
    const k = cfg(l);
    return k.cascade.on || k.pageMode === 'scroll';
  },
  ageOnly: true,
  Render: ({ layer, r, w, data, time, state, lang }) => <TableRender layer={layer} r={r as never} w={w} data={data} time={time} state={state} lang={lang} />,
  Inspector: ({ layer }) => <TableCard layer={layer as unknown as TL} />,
  quiet: true,
  Operator: (p) => <TableBar {...p} />,
  // Chained on Continue: F2 turns the page (until the last one).
  waits: (l, state, data, item) => {
    const k = cfg(l);
    if (k.pageMode !== 'pages') return false;
    const m = model(k, data);
    const now = Date.now();
    return waitsContinue(live(k, (state ?? {}) as State, now, (item.age ?? 0) / 50), pageCount(k, m), now);
  },
  // Pages to turn, a row to highlight or data to reload.
  controls: (l, data) => {
    const k = cfg(l);
    return !!k.link.trim() || model(k, data).rows.length > 0;
  },
  // Timed pages: the Operator's preview holds them on the page shown until their Play.
  plays: (l, data) => {
    const k = cfg(l);
    return k.pageMode === 'pages' && k.autoPages && k.autoOn === 'timed' && k.autoSecs > 0 && pageCount(k, model(k, data)) > 1 && 'onAir';
  },
  Overlay: ({ layer, scale, edit }) => {
    const k = cfg(layer);
    const top = (k.title.on ? titleH(k) + k.gap : 0) + (k.headerRow ? headerH(k) + k.gap : 0);
    return <Handle x={-14 / scale} y={top + k.rowH} scale={scale} label={t('Row height {n} px', { n: k.rowH })} onDrag={(_, y) => edit((l) => void (l.rowH = clamp(Math.round(y - top), 30, 160)), `rowh-${layer.id}`)} />;
  },
  warnings: (layer, { data, state, r }) => warningsOf(layer as unknown as TL, data, state, vnum(r?.width ?? (layer.props.width as number) ?? 0), vnum(r?.height ?? (layer.props.height as number) ?? 0)),
});

type OpP = { layer: Config; state: unknown; data: Data; act: (op: string, args?: unknown) => void; time: { nowMs: number; ageSecs: number; held?: boolean } };

/** The table on air now (the Designer's auto pages and highlight until the operator acts). */
function useTable({ layer, state, data, time }: OpP) {
  const { now, age } = useLiveTime(time);
  const k = cfg(layer);
  const m = model(k, data);
  const st = live(k, (state ?? {}) as State, now, age);
  const pages = pageCount(k, m);
  const [page] = currentPage(st, pages, now);
  return { k, m, st, pages, page, now, scroll: k.pageMode === 'scroll' };
}

/** The pages wait for Continue (F2): chained on it, not on the last page (mirrors waits_continue). */
export function waitsContinue(st: State, pages: number, now: number): boolean {
  return !!st.auto && st.on === 'continue' && currentPage(st, pages, now)[0] + 1 < pages;
}

/** Seconds left before the next timed page (undefined = not running), mirrors next_in. */
function nextIn(st: State, now: number): number | undefined {
  if (!st.auto || !(st.interval ?? 0) || !(st.autoAt ?? 0)) return undefined;
  const t = st.pausedAt ? st.pausedAt : now;
  const step = st.interval! * 1000;
  const e = Math.max(0, t - st.autoAt!) % step;
  return (step - e) / 1000;
}

/** The highlighted row, chosen among the rows (their key); in the Designer the template's (hlKey, exposable), in the Operator the one on air. */
function HlRow({ m, value, onChange }: { m: M; value: string; onChange: (v: string) => void }) {
  const options = [{ value: '', label: '—' }, ...m.rows.map((r) => ({ value: r.key, label: `${r.rank}. ${r.key}` }))];
  return (
    <Row label={tc('cc', 'Highlighted row')} expose="hlKey" exposeValue={value} exposeOptions={options.map((o) => [o.value, o.label] as [string, string])}>
      <Dropdown value={value} options={options} onChange={onChange} />
    </Row>
  );
}

/**
 * The control centre of the table (web/src/widgets/centre.tsx). Line 1: the page and its dots,
 * the countdown to the next page or what it waits for. Line 2: manual → previous / next;
 * timed → play / pause, stop; on Continue → Continue, stop; on a trigger → fire it, stop; then
 * Refresh (linked data). Line 3: chain the pages, how the next page comes, the time per page or
 * the trigger (always in the Operator, on the air only), the highlighted row (exposable).
 */
function TableBar(p: OpP) {
  const { act, layer } = p;
  const mode = useCentreMode();
  const b = useBound();
  const { k, m, st, pages, page, now, scroll } = useTable(p);
  const turns = !scroll && pages > 1;
  // Held by the preview: its auto pages wait for their Play.
  const held = !!p.time.held;
  const chained = !!st.auto;
  const on: AdvanceMode = chained ? (st.on ?? 'timed') : 'timed';
  const running = chained && on === 'timed' && !st.pausedAt && !held;
  const trigger = st.trigger ?? k.autoTrigger;
  const secs = st.interval || k.autoSecs || 6;
  const left = nextIn(st, now);
  // The Designer edits the template (what the take starts with); the Operator the air only.
  const designer = mode === 'designer';
  const setChained = (v: boolean) => (designer ? b.set('autoPages', v) : act(v ? 'advance' : 'stop', v ? { on: k.autoOn, seconds: k.autoSecs, trigger: k.autoTrigger } : undefined));
  const setOn = (v: AdvanceMode) => (designer ? b.set('autoOn', v) : act('advance', { on: v, seconds: secs, trigger }));
  const setSecs = (v: number) => (designer ? b.set('autoSecs', v) : act('auto', { seconds: v }));
  const setTrigger = (v: string) => (designer ? b.set('autoTrigger', v) : act('advance', { on: 'trigger', trigger: v }));
  const chainedShown = designer ? k.autoPages : chained;
  const onShown = designer ? k.autoOn : on;
  const secsShown = designer ? k.autoSecs : secs;
  const triggerShown = designer ? k.autoTrigger : trigger;
  useKeys({
    ArrowLeft: () => !scroll && act('prev'),
    ArrowRight: () => !scroll && act('next'),
    p: () => turns && on === 'timed' && act(running ? 'pause' : chained ? 'resume' : 'auto', chained ? undefined : { seconds: secs }),
    r: () => k.link.trim() && act('reload'),
  });
  const status = !turns || !chained ? undefined : on === 'timed' ? (running && left !== undefined ? <Countdown left={left} total={st.interval ?? secs} /> : undefined) : on === 'continue' ? (waitsContinue(st, pages, now) ? <WaitsContinue /> : undefined) : <WaitsTrigger name={trigger} />;
  const actions: Line2Item[] = !scroll
    ? [
        ...(!chained
          ? [
              { icon: SkipBack, label: tc('cc', 'Previous'), key: '←', disabled: pages < 2, onClick: () => act('prev') },
              { icon: SkipForward, label: tc('cc', 'Next'), key: '→', disabled: pages < 2, onClick: () => act('next') },
            ]
          : on === 'timed'
            ? [running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: () => act('pause') } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: () => act(st.pausedAt ? 'resume' : 'auto', st.pausedAt ? undefined : { seconds: secs }) }]
            : on === 'continue'
              ? [{ icon: ChevronsRight, label: tc('cc', 'Continue'), key: 'F2', main: true, disabled: !waitsContinue(st, pages, now), onClick: () => act('next') }]
              : [{ icon: Zap, label: trigger || tc('cc', 'Fire the trigger'), main: true, onClick: () => act('next') }]),
        chained && { icon: Square, label: tc('cc', 'Stop'), onClick: () => act('stop') },
        GAP,
        !!k.link.trim() && { icon: RefreshCw, label: tc('cc', 'Refresh'), key: 'R', onClick: () => act('reload') },
      ]
    : [!!k.link.trim() && { icon: RefreshCw, label: tc('cc', 'Refresh'), key: 'R', onClick: () => act('reload') }];
  return (
    <Centre name={(layer as { name?: string }).name} readout={!scroll && m.rows.length > 0 ? { k: tc('cc', 'Page'), v: page + 1, of: pages, after: <Dots at={page} count={pages} /> } : undefined} status={status} actions={actions}>
      {!scroll && (
        <>
          <Toggle label={t('Chain the pages')} value={chainedShown} onChange={setChained} />
          {chainedShown && <Seg label={t('Next page')} value={onShown} onChange={setOn} options={ADVANCE_MODES()} />}
          {chainedShown && onShown === 'timed' && <RangeNum label={t('Time per page')} value={secsShown} min={2} max={30} step={1} unit="s" onRelease onChange={setSecs} />}
          {chainedShown && onShown === 'trigger' && <Text label={tc('cc', 'Trigger name')} value={triggerShown} onChange={setTrigger} />}
        </>
      )}
      {m.rows.length > 0 && <HlRow m={m} value={designer ? k.hlKey : (st.hl ?? '')} onChange={(v) => (designer ? b.set('hlKey', v) : act('highlight', { row: v }))} />}
    </Centre>
  );
}

export function ColorPickRow({ label, path, value }: { label: string; path: string; value: string }) {
  const b = useBound();
  const v = (path.split('.').reduce<unknown>((a, x) => (a as Record<string, unknown> | undefined)?.[x], b.obj) as string | undefined) ?? value;
  return (
    <Row expose={path} exposeValue={v} label={label}>
      <ColorField value={parseHex(v) ?? [1, 1, 1, 1]} onChange={(c) => b.set(path, toHex(c, c[3] < 1), true)} />
    </Row>
  );
}

/** The right panel: the button to the table's editor (every setting is there). */
function TableCard({ layer }: { layer: TL }) {
  const open = useTableEditor((st) => st.open);
  return (
    <div className="te-card">
      <button className="btn primary te-edit" onClick={() => open(layer.id)}>
        <Pencil size={14} /> {t('Edit the table')}
      </button>
    </div>
  );
}

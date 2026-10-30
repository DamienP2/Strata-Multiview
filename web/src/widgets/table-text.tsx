// One line of text fitted to its cell for the table, ticker and credits widgets: shrink → squeeze → ellipsis,
// the chain of crates/cg-render/src/wid/table.rs `fitted_path` (engine/textFit.ts fitText = cg_render text::fit_text).
import { useRef, useState, type CSSProperties, type ReactNode, type Ref } from 'react';
import { fitText, textWidth } from '../engine/textFit';
import { MoreButton, OpRow, useFrame, type OpBtn } from './kit';

export const FONT = 'Inter';
export const FONT_CSS = `"Inter", system-ui, sans-serif`;
export type Overflow = 'shrink' | 'squeeze' | 'ellipsis';
export interface FitCfg {
  overflow: Overflow;
  /** px. */
  minSize: number;
  /** %. */
  minSqueeze: number;
}

/** Width of one line (a family, Inter by default, at a weight), px. */
export const tw = (text: string, size: number, weight: number, ls = 0, family = FONT) => textWidth(text, { fontFamily: family || FONT, fontWeight: weight, letterSpacing: ls }, size);
/** CSS font family of a text drawn in `family` (Inter when it is missing). */
export const familyCss = (family?: string) => (family && family !== FONT ? `"${family}", ${FONT_CSS}` : FONT_CSS);

/**
 * A field of a compact Operator row (a value, a time): it shows `value` until
 * it is typed in; Enter (or leaving it changed) sends the text, Escape gives up.
 */
export function OpField({ value, title, width, placeholder, onSubmit, inputRef }: { value: string; title: string; width: number; placeholder?: string; onSubmit: (v: string) => void; inputRef?: Ref<HTMLInputElement> }) {
  const [draft, setDraft] = useState<string | null>(null);
  const send = () => {
    const v = (draft ?? '').trim();
    setDraft(null);
    if (v && v !== value.trim()) onSubmit(v);
  };
  return (
    <input
      ref={inputRef}
      className="mono"
      title={title}
      placeholder={placeholder}
      style={{ width, height: 28, textAlign: 'center' }}
      value={draft ?? value}
      onFocus={() => setDraft(value)}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={send}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === 'Enter') e.currentTarget.blur();
        if (e.key === 'Escape') (setDraft(null), requestAnimationFrame(() => (e.target as HTMLInputElement).blur()));
      }}
    />
  );
}

/**
 * The wall-clock time and the time on air now, for an Operator row redrawn every frame: the
 * Operator gives them when it draws (its updates), they advance from there.
 */
export function useLiveTime(time: { nowMs: number; ageSecs: number; held?: boolean }) {
  // Held by the preview (no state yet, its time standing still): nothing moves, no redraw on each frame.
  const frame = useFrame(!time.held);
  const ref = useRef({ nowMs: time.nowMs, off: time.nowMs - Date.now(), age: time.ageSecs, at: Date.now() });
  const r = ref.current;
  if (r.nowMs !== time.nowMs) Object.assign(r, { nowMs: time.nowMs, off: time.nowMs - Date.now() });
  if (r.age !== time.ageSecs) Object.assign(r, { age: time.ageSecs, at: Date.now() });
  // Held by the preview: its time on air stands still.
  return { now: frame + r.off, age: time.held ? r.age : r.age + Math.max(0, frame - r.at) / 1000 };
}

/** useLiveTime without a redraw on each frame: reads the time now and on air when called (see useFrameValue). */
export function useLiveClock(time: { nowMs: number; ageSecs: number; held?: boolean }): () => { now: number; age: number } {
  const ref = useRef({ nowMs: time.nowMs, off: time.nowMs - Date.now(), age: time.ageSecs, at: Date.now() });
  const r = ref.current;
  if (r.nowMs !== time.nowMs) Object.assign(r, { nowMs: time.nowMs, off: time.nowMs - Date.now() });
  if (r.age !== time.ageSecs) Object.assign(r, { age: time.ageSecs, at: Date.now() });
  const held = !!time.held;
  return () => {
    const frame = Date.now();
    return { now: frame + r.off, age: held ? r.age : r.age + Math.max(0, frame - r.at) / 1000 };
  };
}

type Btn = OpBtn | false | null | undefined;
/** A compact Operator row with a field among its buttons: readout, buttons, the field, more buttons (the ⋯ last). */
export function OpLine({ value, list, field, after = [] }: { value?: ReactNode; list: Btn[]; field?: ReactNode; after?: Btn[] }) {
  if (!field) return <OpRow value={value} list={[...list, ...after]} />;
  const has = (l: Btn[]) => l.some(Boolean);
  return (
    <div className="wk-oprow">
      {(has(list) || (value != null && value !== '')) && <OpRow value={value} list={list} noMore />}
      {field}
      {has(after) ? <OpRow list={after} /> : <MoreButton />}
    </div>
  );
}

export interface Fitted1 {
  text: string;
  size: number;
  sx: number;
  changed: boolean;
}

/** The fitted text of a cell `w` px wide (mirrors fitted_path). */
export function fit1(text: string, size: number, weight: number, ls: number, w: number, fit?: FitCfg, family = FONT): Fitted1 {
  if (!fit) return { text, size, sx: 1, changed: false };
  const base = { fontFamily: family || FONT, fontWeight: weight, letterSpacing: ls, lineHeight: 1.2, boxW: Math.max(1, w), boxH: size, wrap: false };
  const r = fit.overflow === 'ellipsis' ? fitText(text, { ...base, size, overflow: 'ellipsis', squeeze: 1, minSize: 1 }) : fitText(text, { ...base, size, overflow: 'fit', squeeze: fit.minSqueeze / 100, minSize: fit.overflow === 'shrink' ? Math.min(1, fit.minSize / size) : 1 });
  let t = r.text;
  if (r.clip && fit.overflow !== 'ellipsis') t = fitText(t, { ...base, boxW: Math.max(1, w / r.squeeze), size: r.size, overflow: 'ellipsis', squeeze: 1, minSize: 1 }).text;
  return { text: t, size: r.size, sx: r.squeeze, changed: r.size !== size || r.squeeze !== 1 || t !== text };
}

/** A fitted line in the box (x, y, w, h), vertically centred. */
export function FitLine({ text, size, weight, ls = 0, align, box: [x, y, w, h], color, fit, style, family = FONT }: { text: string; size: number; weight: number; ls?: number; align: 'left' | 'center' | 'right'; box: [number, number, number, number]; color: string; fit?: FitCfg; style?: CSSProperties; family?: string }) {
  const f = fit1(text, size, weight, ls, w, fit, family);
  return (
    <div style={{ position: 'absolute', left: x, top: y, width: w, height: h, display: 'flex', alignItems: 'center', ...style }}>
      <span
        style={{
          display: 'block',
          width: w / f.sx,
          flex: 'none',
          transform: f.sx < 1 ? `scaleX(${f.sx})` : undefined,
          transformOrigin: '0 50%',
          fontFamily: familyCss(family),
          fontWeight: weight,
          fontSize: f.size,
          letterSpacing: ls,
          lineHeight: 1.2,
          whiteSpace: 'pre',
          overflow: 'hidden',
          textAlign: align,
          color,
        }}
      >
        {f.text}
      </span>
    </div>
  );
}

// -------------------------------------------------------- table levels --
/** A table level's text with every setting known (mirrors `Text` of crates/cg-core/src/wid/table.rs). */
export interface CellText {
  family: string;
  weight: number;
  size: number;
  spacing: number;
  case: 'none' | 'upper' | 'lower' | 'title';
  italic: boolean;
  deco: 'none' | 'underline' | 'strike';
  multi: boolean;
  max: number;
  lh: number;
  anyBreak: boolean;
  fit: 'fit' | 'ellipsis' | 'clip';
  narrow: number;
  small: number;
  then: 'ellipsis' | 'clip';
}

export function cased(t: CellText, s: string): string {
  if (t.case === 'upper') return s.toUpperCase();
  if (t.case === 'lower') return s.toLowerCase();
  if (t.case === 'title') {
    let start = true;
    let out = '';
    for (const ch of s) {
      if (start && /[\p{L}\p{N}]/u.test(ch)) {
        out += ch.toUpperCase();
        start = false;
      } else {
        if (/\s|-/.test(ch)) start = true;
        out += ch;
      }
    }
    return out;
  }
  return s;
}

type Width = (s: string, size: number) => number;
function cutTo(width: Width, s: string, size: number, maxW: number, dots: boolean): string {
  if (width(s, size) <= maxW + 0.5) return s;
  const ch = [...s];
  for (;;) {
    let t = ch.join('');
    if (dots) t = t.trimEnd() + '…';
    if (!ch.length || width(t, size) <= maxW + 0.5) return t;
    ch.pop();
  }
}
function wrapCell(width: Width, s: string, size: number, maxW: number, any: boolean): string[] {
  const out: string[] = [];
  for (const para of s.split('\n')) {
    let cur = '';
    if (any) {
      for (const ch of para) {
        const next = cur + ch;
        if (cur && width(next, size) > maxW + 0.5) {
          out.push(cur.trimEnd());
          cur = ch.trimStart();
        } else cur = next;
      }
    } else {
      for (const word of para.split(' ')) {
        const next = cur ? `${cur} ${word}` : word;
        if (cur && width(next, size) > maxW + 0.5) {
          out.push(cur);
          cur = word;
        } else cur = next;
      }
    }
    out.push(cur);
  }
  return out;
}
const blockH = (n: number, size: number, lh: number) => (n ? (n - 1) * size * lh + size : 0);

/** How a text fits its cell: [lines, size, horizontal scale] (mirrors fit_cell of cg-render table.rs). */
export function fitCell(width: Width, t: CellText, s: string, w: number, h: number): [string, number, number] {
  const size0 = Math.max(1, t.size);
  w = Math.max(1, w);
  h = Math.max(1, h);
  const dotsAfterFit = t.then === 'ellipsis';
  if (!t.multi) {
    const w0 = width(s, size0);
    if (w0 <= w + 0.5) return [s, size0, 1];
    if (t.fit === 'ellipsis') return [cutTo(width, s, size0, w, true), size0, 1];
    if (t.fit === 'clip') return [cutTo(width, s, size0, w, false), size0, 1];
    const narrow = t.narrow / 100;
    if (w / w0 >= narrow) return [s, size0, w / w0];
    const min = (size0 * t.small) / 100;
    const fits = (sz: number) => width(s, sz) * narrow <= w + 0.5;
    if (!fits(min)) return [cutTo(width, s, min, w / narrow, dotsAfterFit), min, narrow];
    let lo = min;
    let hi = size0;
    for (let i = 0; i < 14; i++) {
      const mid = (lo + hi) / 2;
      if (fits(mid)) lo = mid;
      else hi = mid;
    }
    return [s, lo, Math.min(1, Math.max(narrow, w / width(s, lo)))];
  }
  const max = Math.max(1, t.max);
  const lines = (sz: number) => wrapCell(width, s, sz, w, t.anyBreak);
  const fits = (sz: number) => {
    const l = lines(sz);
    return l.length <= max && blockH(l.length, sz, t.lh) <= h + 0.5 && l.every((x) => width(x, sz) <= w + 0.5);
  };
  if (fits(size0)) return [lines(size0).join('\n'), size0, 1];
  let size = size0;
  if (t.fit === 'fit') {
    const min = (size0 * t.small) / 100;
    if (fits(min)) {
      let lo = min;
      let hi = size0;
      for (let i = 0; i < 14; i++) {
        const mid = (lo + hi) / 2;
        if (fits(mid)) lo = mid;
        else hi = mid;
      }
      return [lines(lo).join('\n'), lo, 1];
    }
    size = min;
  }
  const dots = t.fit === 'ellipsis' ? true : t.fit === 'clip' ? false : dotsAfterFit;
  const room = Math.max(1, Math.min(max, Math.floor(Math.max(0, (h - size) / (size * t.lh))) + 1));
  const l = lines(size);
  const more = l.length > room;
  l.length = Math.min(l.length, room);
  const out = l.map((line, i) => {
    if (i + 1 === l.length && more && dots) {
      const c = cutTo(width, `${line}…`, size, w, true);
      return c.endsWith('…') ? c : `${c}…`;
    }
    return cutTo(width, line, size, w, dots);
  });
  return [out.join('\n'), size, 1];
}

/** A text in the box (x, y, w, h) with its level's typography, vertically centred. */
export function TextCell({ t, text, align, box: [x, y, w, h], color, style }: { t: CellText; text: string; align: 'left' | 'center' | 'right'; box: [number, number, number, number]; color: string; style?: CSSProperties }) {
  const s = cased(t, text);
  const o = { fontFamily: t.family, fontWeight: t.weight, letterSpacing: t.spacing };
  const [lines, size, sx] = fitCell((str, sz) => textWidth(str, o, sz), t, s, w, h);
  return (
    <div style={{ position: 'absolute', left: x, top: y, width: w, height: h, display: 'flex', alignItems: 'center', ...style }}>
      <span
        style={{
          display: 'block',
          width: w / sx,
          flex: 'none',
          transform: sx < 1 ? `scaleX(${sx})` : undefined,
          transformOrigin: '0 50%',
          fontFamily: `"${t.family}", ${FONT_CSS}`,
          fontWeight: t.weight,
          fontStyle: t.italic ? 'italic' : undefined,
          textDecoration: t.deco === 'underline' ? 'underline' : t.deco === 'strike' ? 'line-through' : undefined,
          fontSize: size,
          letterSpacing: t.spacing,
          lineHeight: t.multi ? t.lh : 1.2,
          whiteSpace: 'pre',
          overflow: 'hidden',
          textAlign: align,
          color,
        }}
      >
        {lines}
      </span>
    </div>
  );
}

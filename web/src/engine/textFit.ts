import type { TextOverflow } from './types';

/*
 * A text made to fit its box, with the same rules as the Rust renderer
 * (crates/cg-render/src/text.rs, fit_text): narrower first (one line of
 * text), then smaller down to the minimum, then cut at the box; or "…",
 * or cut, or drawn outside.
 */

export interface Fitted {
  /** The lines to draw (wrapped, or shortened with "…"). */
  text: string;
  size: number;
  /** Horizontal scale (1 = as designed). */
  squeeze: number;
  /** Cut at the box. */
  clip: boolean;
  /** Size and width kept, 1 = as designed. */
  ratio: number;
  /** Not all of it shows (cut, "…", or outside its box). */
  overflows: boolean;
}

export interface FitOptions {
  fontFamily: string;
  fontWeight: number;
  size: number;
  letterSpacing: number;
  lineHeight: number;
  boxW: number;
  boxH: number;
  wrap: boolean;
  overflow: TextOverflow;
  squeeze: number;
  minSize: number;
}

let ctx: CanvasRenderingContext2D | null = null;

/*
 * Widths measured, by font and line: a text moving or redrawn on each frame
 * (a ticker, a readout) is measured once, not on every frame. Forgotten when
 * a font finishes loading (the widths change) and when it grows too big.
 */
const widths = new Map<string, number>();
if (typeof document !== 'undefined' && document.fonts) document.fonts.addEventListener('loadingdone', () => widths.clear());

/** The width of one line of text in a CSS `font` (canvas measure, without letter spacing); 0 without a canvas. */
export function lineWidth(font: string, line: string): number {
  const key = `${font}\u0000${line}`;
  let w = widths.get(key);
  if (w === undefined) {
    ctx ??= document.createElement('canvas').getContext('2d');
    if (!ctx) return 0;
    ctx.font = font;
    w = ctx.measureText(line).width;
    if (widths.size >= 4000) widths.clear();
    widths.set(key, w);
  }
  return w;
}

export function textWidth(text: string, o: Pick<FitOptions, 'fontFamily' | 'fontWeight' | 'letterSpacing'>, size: number): number {
  const font = `${o.fontWeight} ${size}px "${o.fontFamily}", Inter, system-ui, sans-serif`;
  return Math.max(0, ...text.split('\n').map((line) => lineWidth(font, line) + Math.max(0, [...line].length - 1) * o.letterSpacing));
}

/** Breaks lines at spaces so that none is wider than `maxW` (like the Rust wrap_text). */
export function wrapText(text: string, o: Pick<FitOptions, 'fontFamily' | 'fontWeight' | 'letterSpacing'>, size: number, maxW: number): string {
  const out: string[] = [];
  for (const line of text.split('\n')) {
    let cur = '';
    for (const word of line.split(' ')) {
      const candidate = cur ? `${cur} ${word}` : word;
      if (cur && textWidth(candidate, o, size) > maxW) {
        out.push(cur);
        cur = word;
      } else cur = candidate;
    }
    out.push(cur);
  }
  return out.join('\n');
}

const blockHeight = (lines: number, size: number, lh: number) => (lines ? (lines - 1) * size * lh + size : 0);

export function fitText(content: string, o: FitOptions): Fitted {
  const bw = Math.max(1, o.boxW);
  const bh = Math.max(1, o.boxH);
  const eps = 0.5;
  const layout = (size: number, width: number) => {
    const t = o.wrap ? wrapText(content, o, size, width) : content;
    return { t, w: textWidth(t, o, size), h: blockHeight(t.split('\n').length, size, o.lineHeight) };
  };
  const size0 = o.size;
  const first = layout(size0, bw);
  const fits = first.w <= bw + eps && first.h <= bh + eps;
  if (fits) return { text: first.t, size: size0, squeeze: 1, clip: false, ratio: 1, overflows: false };
  if (o.overflow === 'visible') return { text: first.t, size: size0, squeeze: 1, clip: false, ratio: 1, overflows: true };
  if (o.overflow === 'clip') return { text: first.t, size: size0, squeeze: 1, clip: true, ratio: 1, overflows: true };
  if (o.overflow === 'ellipsis') {
    const maxLines = Math.max(0, Math.floor((bh - size0) / (size0 * o.lineHeight))) + 1;
    const all = first.t.split('\n');
    const cut = all.length > maxLines;
    const lines = all.slice(0, maxLines).map((line, i, arr) => {
      const last = i === arr.length - 1;
      if (textWidth(line, o, size0) <= bw + eps && !(last && cut)) return line;
      const chars = [...line];
      for (;;) {
        const t = chars.join('').trimEnd() + '…';
        if (!chars.length || textWidth(t, o, size0) <= bw + eps) return t;
        chars.pop();
      }
    });
    return { text: lines.join('\n'), size: size0, squeeze: 1, clip: true, ratio: 1, overflows: true };
  }
  // Fit: narrower first (a paragraph wraps instead), then smaller.
  const sx = o.wrap || first.w <= bw ? 1 : Math.max(bw / first.w, Math.min(1, Math.max(0.1, o.squeeze)));
  const test = (size: number) => {
    const l = layout(size, bw / sx);
    return { ok: l.w * sx <= bw + eps && l.h <= bh + eps, t: l.t };
  };
  const at0 = test(size0);
  if (at0.ok) return { text: at0.t, size: size0, squeeze: sx, clip: false, ratio: sx, overflows: false };
  const loSize = size0 * Math.min(1, Math.max(0.1, o.minSize));
  const atLo = test(loSize);
  if (!atLo.ok) return { text: atLo.t, size: loSize, squeeze: sx, clip: true, ratio: (loSize / size0) * sx, overflows: true };
  let lo = loSize;
  let hi = size0;
  let best = atLo.t;
  for (let i = 0; i < 14; i++) {
    const mid = (lo + hi) / 2;
    const r = test(mid);
    if (r.ok) {
      lo = mid;
      best = r.t;
    } else hi = mid;
  }
  return { text: best, size: lo, squeeze: sx, clip: false, ratio: (lo / size0) * sx, overflows: false };
}

/* ------------------------------------------------------------ rich text --
 * The fit chain of the Text widget, same as `fit_lines` in
 * crates/cg-render/src/text.rs, and the language rules of
 * crates/cg-core/src/wid/text.rs (graphemes, hyphenation, French spaces).
 */

const GS = typeof Intl !== 'undefined' && 'Segmenter' in Intl ? new Intl.Segmenter(undefined, { granularity: 'grapheme' }) : null;
/** User-perceived characters (an emoji, a flag, a letter and its accents count as one). */
export const graphemes = (s: string): string[] => (GS ? Array.from(GS.segment(s), (x) => x.segment) : [...s]);

const LANGS: Record<string, { v: string; keep: string[]; l: number; r: number }> = {
  fr: { v: 'aeiouyàâäéèêëîïôöùûüœæ', keep: ['ch', 'ph', 'th', 'gn', 'qu', 'bl', 'br', 'cl', 'cr', 'dr', 'fl', 'fr', 'gl', 'gr', 'pl', 'pr', 'tr', 'vr'], l: 2, r: 3 },
  en: { v: 'aeiouy', keep: ['ch', 'sh', 'th', 'ph', 'wh', 'ck', 'ng', 'qu'], l: 2, r: 3 },
  de: { v: 'aeiouyäöü', keep: ['ch', 'ck', 'ph', 'th', 'qu', 'sch'], l: 2, r: 2 },
  es: { v: 'aeiouáéíóúü', keep: ['ch', 'll', 'rr', 'bl', 'br', 'cl', 'cr', 'dr', 'fl', 'fr', 'gl', 'gr', 'pl', 'pr', 'tr', 'qu'], l: 2, r: 2 },
  it: { v: 'aeiouàèéìòù', keep: ['ch', 'gh', 'gn', 'gl', 'sc', 'bl', 'br', 'cr', 'dr', 'fr', 'gr', 'pr', 'tr', 'qu'], l: 2, r: 2 },
};
export const HYPHEN_LANGS: [string, string][] = [
  ['fr', 'French'],
  ['en', 'English'],
  ['de', 'German'],
  ['es', 'Spanish'],
  ['it', 'Italian'],
];

/** Where a word may be hyphenated (V-CV, VC-CV; simple rules, no dictionary), shortest first. */
export function hyphenPoints(word: string, lang: string): [string, string][] {
  const L = LANGS[lang] ?? LANGS.fr;
  const c = [...word];
  const low = c.map((x) => x.toLowerCase()[0] ?? x);
  const isL = (i: number) => i >= 0 && i < low.length && /\p{L}/u.test(low[i]);
  const isV = (i: number) => i >= 0 && i < low.length && L.v.includes(low[i]);
  const out: [string, string][] = [];
  if (c.length < L.l + L.r) return out;
  for (let i = L.l; i <= c.length - L.r; i++) {
    if (!isL(i - 1) || !isL(i) || !isL(i + 1)) continue;
    const ok = (isV(i - 1) && !isV(i) && isV(i + 1)) || (i >= 2 && !isV(i - 1) && !isV(i) && isV(i + 1) && isV(i - 2) && !L.keep.includes(low[i - 1] + low[i]));
    if (ok) out.push([c.slice(0, i).join(''), c.slice(i).join('')]);
  }
  return out;
}

/** French typography: the space before : ; ! ? » and after « never breaks. */
export const glue = (t: string, lang: string) => (lang === 'fr' ? t.replace(/ ([:;!?»])/g, ' $1').replace(/« /g, '« ') : t);

export interface FitLine {
  text: string;
  /** Last line of its paragraph (not justified). */
  end: boolean;
}
export interface FitLines {
  lines: FitLine[];
  size: number;
  squeeze: number;
  clip: boolean;
  ratio: number;
  overflows: boolean;
  /** fit | squeeze | shrink | end (minimums reached) | clip | ellipsis | visible. */
  stage: 'fit' | 'squeeze' | 'shrink' | 'end' | 'clip' | 'ellipsis' | 'visible';
  forced: boolean;
  hyphenated: boolean;
}
export interface FitLinesOpts {
  size: number;
  lineHeight: number;
  boxW: number;
  boxH: number;
  wrap: boolean;
  overflow: TextOverflow;
  squeeze: number;
  minSize: number;
  /** 0 = no limit. */
  maxLines: number;
  hyphen: boolean;
  lang: string;
  ellipsisEnd: boolean;
}
type Measure = (line: string, size: number) => number;
interface Wrapped {
  lines: FitLine[];
  forced: boolean;
  hyph: boolean;
}

function wrapLines(content: string, o: FitLinesOpts, size: number, maxW: number, forced: boolean, m: Measure): Wrapped {
  const w: Wrapped = { lines: [], forced: false, hyph: false };
  if (!o.wrap) {
    w.lines = content.split('\n').map((text) => ({ text, end: true }));
    return w;
  }
  for (const para of glue(content, o.lang).split('\n')) {
    let cur = '';
    for (const word of para.split(' ')) {
      const cand = cur === '' ? word : `${cur} ${word}`;
      if (cur === '' || m(cand, size) <= maxW) cur = cand;
      else {
        let rest = word;
        if (o.hyphen) {
          const p = hyphenPoints(word, o.lang)
            .reverse()
            .find(([a]) => m(`${cur} ${a}-`, size) <= maxW);
          if (p) {
            w.lines.push({ text: `${cur} ${p[0]}-`, end: false });
            w.hyph = true;
            rest = p[1];
            cur = '';
          }
        }
        if (cur !== '') w.lines.push({ text: cur, end: false });
        cur = rest;
      }
      let guard = 0;
      while (forced && guard < 200 && m(cur, size) > maxW) {
        guard++;
        let p = hyphenPoints(cur, o.lang)
          .reverse()
          .find(([a]) => m(`${a}-`, size) <= maxW);
        if (!p) {
          const g = graphemes(cur);
          let k = Math.max(0, g.length - 1);
          while (k > 1 && m(`${g.slice(0, k).join('')}-`, size) > maxW) k--;
          p = [g.slice(0, k).join(''), g.slice(k).join('')];
        }
        if (!p[0] || !p[1]) break;
        w.lines.push({ text: `${p[0]}-`, end: false });
        w.forced = true;
        cur = p[1];
      }
    }
    w.lines.push({ text: cur, end: true });
  }
  return w;
}

function ellipsize(lines: FitLine[], o: FitLinesOpts, size: number, sx: number, m: Measure): FitLine[] {
  const bw = Math.max(1, o.boxW) / sx;
  let max = Math.max(0, Math.floor((Math.max(1, o.boxH) - size) / (size * o.lineHeight))) + 1;
  if (o.maxLines > 0) max = Math.min(max, o.maxLines);
  const cut = lines.length > max;
  const kept = lines.slice(0, max);
  return kept.map((l, i) => {
    const last = i === kept.length - 1;
    if (m(l.text, size) <= bw + 0.5 && !(last && cut)) return l;
    const g = graphemes(l.text);
    for (;;) {
      const t = g.join('').trimEnd() + '…';
      if (!g.length || m(t, size) <= bw + 0.5) return { text: t, end: true };
      g.pop();
    }
  });
}

/** The Text widget's fit chain (mirrors fit_lines): as fitText, plus a line limit, hyphenation, forced breaks of too-long words and "…" at the minimums. */
export function fitLines(content: string, o: FitLinesOpts, m: Measure): FitLines {
  const bw = Math.max(1, o.boxW);
  const bh = Math.max(1, o.boxH);
  const eps = 0.5;
  const size0 = o.size;
  const widest = (w: Wrapped, size: number) => Math.max(0, ...w.lines.map((l) => m(l.text, size)));
  const fits = (w: Wrapped, size: number, sx: number) => widest(w, size) * sx <= bw + eps && blockHeight(w.lines.length, size, o.lineHeight) <= bh + eps && (o.maxLines === 0 || w.lines.length <= o.maxLines);
  const done = (w: Wrapped, size: number, sx: number, clip: boolean, overflows: boolean, stage: FitLines['stage']): FitLines => ({ lines: w.lines, size, squeeze: sx, clip, ratio: (size / Math.max(1e-6, size0)) * sx, overflows, stage, forced: w.forced, hyphenated: w.hyph });
  const first = wrapLines(content, o, size0, bw, false, m);
  const w0 = widest(first, size0);
  if (fits(first, size0, 1)) return done(first, size0, 1, false, false, 'fit');
  if (o.overflow === 'visible') return done(first, size0, 1, false, true, 'visible');
  if (o.overflow === 'clip' || o.overflow === 'ellipsis') {
    const f = o.wrap ? wrapLines(content, o, size0, bw, true, m) : first;
    if (fits(f, size0, 1)) return done(f, size0, 1, false, false, 'fit');
    if (o.overflow === 'clip') return done(f, size0, 1, true, true, 'clip');
    return done({ ...f, lines: ellipsize(f.lines, o, size0, 1, m) }, size0, 1, true, true, 'ellipsis');
  }
  // Fit: narrower first (one line of text only: a paragraph wraps), then smaller.
  const sx = o.wrap || w0 <= bw ? 1 : Math.max(bw / w0, Math.min(1, Math.max(0.1, o.squeeze)));
  const test = (size: number, forced: boolean): [boolean, Wrapped] => {
    const w = wrapLines(content, o, size, bw / sx, forced, m);
    return [fits(w, size, sx), w];
  };
  const [ok0, w00] = test(size0, false);
  if (ok0) return done(w00, size0, sx, false, false, sx < 1 ? 'squeeze' : 'fit');
  const loSize = size0 * Math.min(1, Math.max(0.1, o.minSize));
  const [ok, t0] = test(loSize, false);
  let t = t0;
  let forced = false;
  if (!ok && o.wrap) {
    const [okF, wF] = test(loSize, true);
    if (okF) {
      forced = true;
      t = wF;
    }
  }
  if (!ok && !forced) {
    if (o.ellipsisEnd) return done({ ...t, lines: ellipsize(t.lines, o, loSize, sx, m) }, loSize, sx, true, true, 'end');
    return done(t, loSize, sx, true, true, 'end');
  }
  if (forced) {
    const [okF0, wF0] = test(size0, true);
    if (okF0) return done(wF0, size0, sx, false, false, sx < 1 ? 'squeeze' : 'fit');
  }
  let lo = loSize;
  let hi = size0;
  let best = t;
  for (let i = 0; i < 14; i++) {
    const mid = (lo + hi) / 2;
    const [k, w] = test(mid, forced);
    if (k) {
      lo = mid;
      best = w;
    } else hi = mid;
  }
  return done(best, lo, sx, false, false, 'shrink');
}

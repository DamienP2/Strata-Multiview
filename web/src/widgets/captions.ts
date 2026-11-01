// Closed captions over a tile's picture. Mirrors crates/strata-mv-core/src/captions.rs
// (CaptionScreen, pick, caption_boxes): keep in sync.

export interface CaptionSpan {
  text: string;
  color: string;
  italic?: boolean;
  underline?: boolean;
}

export interface CaptionLine {
  row: number;
  col: number;
  spans: CaptionSpan[];
}

/** What a caption channel shows (mirrors captions::CaptionScreen). */
export interface CaptionScreen {
  cols: number;
  /** The CEA-608 grid, in the middle 4:3 of a wide picture. */
  narrow?: boolean;
  lines: CaptionLine[];
}

type Area = [number, number, number, number];

/** The channels, in the order "auto" picks them. */
export const CHANNELS = ['s1', 'cc1', 's2', 'cc3', 'cc2', 'cc4', 's3', 's4', 's5', 's6'];

/** The channel a tile shows. Mirrors captions::pick. */
export function pickCaptions(screens: Record<string, CaptionScreen> | undefined, want: string): CaptionScreen | null {
  if (!want || !screens) return null;
  if (want === 'auto') return CHANNELS.map((c) => screens[c]).find((s) => s && s.lines.length > 0) ?? null;
  return screens[want] ?? null;
}

/** JetBrains Mono's advance, per pixel of size. */
export const ADVANCE = 0.6;

export interface CaptionBox {
  a: Area;
  t: Area;
  text: string;
  color: string;
  size: number;
  italic: boolean;
  underline: Area | null;
  /** The cells of its ♪ (spaces in `text`), drawn by `note`. */
  notes: Area[];
}

/** A ♪ in cell `c` of a caption `size` px: head [cx, cy, rx, ry], stem box, flag triangle. Mirrors captions::note. */
export function note(c: Area, size: number): { head: Area; stem: Area; flag: [number, number][] } {
  const [x, y, w, h] = c;
  const rx = size * 0.19;
  const ry = size * 0.14;
  const cx = x + w * 0.42;
  const cy = y + h * 0.68;
  const sw = Math.max(1, size * 0.07);
  const sx = cx + rx - sw;
  const top = y + h * 0.18;
  return {
    head: [cx, cy, rx, ry],
    stem: [sx, top, sw, cy - top],
    flag: [
      [sx + sw, top],
      [sx + sw + size * 0.2, top + size * 0.3],
      [sx + sw, top + size * 0.24],
    ],
  };
}

const chars = (s: string) => [...s].length;

/** Where `s` is drawn over picture `p`, `scale` times the usual size. Mirrors captions::caption_boxes. */
export function captionBoxes(s: CaptionScreen, p: Area, scale: number): CaptionBox[] {
  const [px, py, pw, ph] = p;
  if (pw <= 0 || ph <= 0 || !s.lines.length) return [];
  const cols = Math.max(1, s.cols);
  const ah = ph * 0.8;
  const aw = s.narrow ? Math.min((ph * 4) / 3, pw) * 0.8 : pw * 0.8;
  const ax = px + (pw - aw) / 2;
  const ay = py + (ph - ah) / 2;
  const rowH = ah / 15;
  const colW = aw / cols;
  const k = Math.min(4, Math.max(0.25, scale));
  const size = Math.min(rowH * 0.82, colW / ADVANCE) * k;
  const adv = size * ADVANCE;
  const bh = size * 1.2;
  const pad = size * 0.25;
  const out: CaptionBox[] = [];
  for (const l of s.lines) {
    const y = ay + (l.row + 0.5) * rowH - bh / 2;
    const width = l.spans.reduce((n, sp) => n + chars(sp.text), 0) * adv;
    let x = ax + l.col * colW;
    x = Math.max(px + pad, Math.min(x, px + pw - width - pad));
    l.spans.forEach((sp, i) => {
      const n = chars(sp.text);
      if (!n) return;
      const w = n * adv;
      const notes: Area[] = [];
      [...sp.text].forEach((ch, j) => ch === '♪' && notes.push([x + j * adv, y, adv, bh]));
      const lp = i === 0 ? pad : 0;
      const rp = i === l.spans.length - 1 ? pad : 0;
      out.push({
        a: [x - lp, y, w + lp + rp, bh],
        t: [x, y, w, bh],
        text: sp.text.replace(/♪/g, ' '),
        color: sp.color,
        size,
        italic: !!sp.italic,
        underline: sp.underline ? [x, y + bh - size * 0.18, w, Math.max(1, size * 0.07)] : null,
        notes,
      });
      x += w;
    });
  }
  return out;
}

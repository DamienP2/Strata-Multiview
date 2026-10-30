// A small painter drawing SVG (rectangles, paths, circles, text by centre
// line, clips, group opacity) used by the scoreboard and the map. Mirror of
// crates/cg-render/src/wid/scoreboard_paint.rs: the widgets draw with the
// same calls on both sides, so the pictures match.
import type { ReactNode } from 'react';
import { textWidth } from '../engine/widgets';

export type RGBA = [number, number, number, number];
export type Cmd = ['M', number, number] | ['L', number, number] | ['C', number, number, number, number, number, number] | ['Z'];

/** `#rgb`, `#rrggbb[aa]` or `rgba(r, g, b, a)` → colour (transparent when invalid). */
export function col(s: string | undefined): RGBA {
  const t = String(s ?? '').trim();
  const m = t.match(/^rgba?\(([^)]*)\)$/);
  if (m) {
    const v = m[1].split(',').map((x) => +x.trim());
    return v.length >= 3 && v.every(Number.isFinite) ? [v[0] / 255, v[1] / 255, v[2] / 255, v[3] ?? 1] : [0, 0, 0, 0];
  }
  let h = t.replace(/^#/, '');
  if (h.length === 3) h = [...h].map((c) => c + c).join('');
  if (!/^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(h)) return [0, 0, 0, 0];
  const n = (i: number) => parseInt(h.slice(i, i + 2), 16) / 255;
  return [n(0), n(2), n(4), h.length === 8 ? n(6) : 1];
}
export const css = (c: RGBA) => `rgb(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)})`;
export const lum = (s: string) => {
  const c = col(s);
  return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
};
/** Readable ink on a background. */
export const ink = (bg: string) => (lum(bg) > 0.62 ? '#0b0d18' : '#ffffff');
export const mix = (a: RGBA, b: RGBA, k: number): RGBA => [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, a[3] + (b[3] - a[3]) * k];

export function rrect(x: number, y: number, w: number, h: number, r: number): Cmd[] {
  r = Math.min(Math.max(0, r), w / 2, h / 2);
  if (r <= 0.01) return [['M', x, y], ['L', x + w, y], ['L', x + w, y + h], ['L', x, y + h], ['Z']];
  const k = 0.5523 * r;
  return [
    ['M', x + r, y],
    ['L', x + w - r, y],
    ['C', x + w - r + k, y, x + w, y + r - k, x + w, y + r],
    ['L', x + w, y + h - r],
    ['C', x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h],
    ['L', x + r, y + h],
    ['C', x + r - k, y + h, x, y + h - r + k, x, y + h - r],
    ['L', x, y + r],
    ['C', x, y + r - k, x + r - k, y, x + r, y],
    ['Z'],
  ];
}
export function ellipse(cx: number, cy: number, rx: number, ry: number): Cmd[] {
  const kx = 0.5523 * rx;
  const ky = 0.5523 * ry;
  return [['M', cx + rx, cy], ['C', cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry], ['C', cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy], ['C', cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry], ['C', cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy], ['Z']];
}
export function poly(pts: [number, number][], close: boolean): Cmd[] {
  const out: Cmd[] = pts.map(([x, y], i) => [i ? 'L' : 'M', x, y] as Cmd);
  if (close && out.length) out.push(['Z']);
  return out;
}
/** Moved by (dx, dy) and scaled by s around the origin. */
export function tf(cmds: Cmd[], dx: number, dy: number, s: number): Cmd[] {
  return cmds.map((c) => (c[0] === 'Z' ? c : ([c[0], ...c.slice(1).map((v, i) => (i % 2 ? dy : dx) + (v as number) * s)] as Cmd)));
}
const r2 = (v: number) => Math.round(v * 100) / 100;
export const toD = (cmds: Cmd[]) =>
  cmds
    .map(
      (c) =>
        c[0] +
        c
          .slice(1)
          .map((v) => r2(v as number))
          .join(' '),
    )
    .join('');

/** Text options. */
export interface T {
  size: number;
  weight: number;
  /** -1 start, 0 middle, 1 end. */
  anchor?: -1 | 0 | 1;
  ls?: number;
  sx?: number;
  sy?: number;
  skew?: number;
}

let seq = 0;

/** The painter: collects SVG elements; `dry` measures only. */
export class Pt {
  dry: boolean;
  alpha = 1;
  /** Font family of the texts. */
  font = 'Inter';
  private stack: { kids: ReactNode[]; clip?: string }[] = [{ kids: [] }];
  defs: ReactNode[] = [];
  private id = `sbp${(seq = (seq + 1) % 1e6)}`;
  private n = 0;
  constructor(dry = false) {
    this.dry = dry;
  }
  private add(e: ReactNode) {
    const top = this.stack[this.stack.length - 1];
    top.kids.push(e);
  }
  private key() {
    return this.n++;
  }
  clip(cmds: Cmd[]) {
    const id = `${this.id}c${this.key()}`;
    if (!this.dry)
      this.defs.push(
        <clipPath key={id} id={id}>
          <path d={toD(cmds)} />
        </clipPath>,
      );
    this.stack.push({ kids: [], clip: id });
  }
  pop() {
    if (this.stack.length < 2) return;
    const g = this.stack.pop()!;
    if (!this.dry && g.kids.length)
      this.add(
        <g key={this.key()} clipPath={`url(#${g.clip})`}>
          {g.kids}
        </g>,
      );
  }
  fillC(cmds: Cmd[], c: RGBA) {
    if (this.dry || c[3] <= 0) return;
    this.add(<path key={this.key()} d={toD(cmds)} fill={css(c)} fillOpacity={c[3] * this.alpha} />);
  }
  fill(cmds: Cmd[], c: string) {
    this.fillC(cmds, col(c));
  }
  fillLin(cmds: Cmd[], p1: [number, number], p2: [number, number], stops: [number, RGBA][]) {
    if (this.dry) return;
    const id = `${this.id}g${this.key()}`;
    this.defs.push(
      <linearGradient key={id} id={id} gradientUnits="userSpaceOnUse" x1={p1[0]} y1={p1[1]} x2={p2[0]} y2={p2[1]}>
        {stops.map(([o, c], i) => (
          <stop key={i} offset={o} stopColor={css(c)} stopOpacity={c[3] * this.alpha} />
        ))}
      </linearGradient>,
    );
    this.add(<path key={this.key()} d={toD(cmds)} fill={`url(#${id})`} />);
  }
  strokeC(cmds: Cmd[], c: RGBA, width: number, round: boolean, dash?: [number, number]) {
    if (this.dry || width <= 0 || c[3] <= 0) return;
    this.add(<path key={this.key()} d={toD(cmds)} fill="none" stroke={css(c)} strokeOpacity={c[3] * this.alpha} strokeWidth={width} strokeLinecap={round ? 'round' : 'butt'} strokeLinejoin={round ? 'round' : 'miter'} strokeDasharray={dash ? dash.join(' ') : undefined} />);
  }
  stroke(cmds: Cmd[], c: string, width: number, round: boolean) {
    this.strokeC(cmds, col(c), width, round);
  }
  rect(x: number, y: number, w: number, h: number, r: number, c: string) {
    if (w > 0 && h > 0) this.fill(rrect(x, y, w, h, r), c);
  }
  rectC(x: number, y: number, w: number, h: number, r: number, c: RGBA) {
    if (w > 0 && h > 0) this.fillC(rrect(x, y, w, h, r), c);
  }
  circle(cx: number, cy: number, r: number, c: string) {
    if (r > 0) this.fill(ellipse(cx, cy, r, r), c);
  }
  tw(s: string, size: number, weight: number, ls: number) {
    return textWidth(s, this.font, weight, size, ls);
  }
  private textEl(s: string, x: number, cy: number, o: T, paint: Record<string, unknown>) {
    const w = this.tw(s, o.size, o.weight, o.ls ?? 0);
    const x0 = x - (w * ((o.anchor ?? -1) + 1)) / 2;
    const sx = o.sx ?? 1;
    const sy = o.sy ?? 1;
    const sk = o.skew ?? 0;
    const transform = sx !== 1 || sy !== 1 || sk ? `translate(${r2(x)} ${r2(cy)}) matrix(${sx} 0 ${-sk * sx} ${sy} 0 0) translate(${r2(-x)} ${r2(-cy)})` : undefined;
    return (
      <text key={this.key()} x={r2(x0)} y={r2(cy + 0.3633 * o.size)} fontFamily={this.font === 'Inter' ? 'Inter' : `"${this.font}", Inter`} fontWeight={o.weight} fontSize={o.size} letterSpacing={o.ls || undefined} transform={transform} style={{ whiteSpace: 'pre' }} {...paint}>
        {s}
      </text>
    );
  }
  text(s: string, x: number, cy: number, c: string, o: T) {
    this.textC(s, x, cy, col(c), o);
  }
  textC(s: string, x: number, cy: number, c: RGBA, o: T) {
    if (this.dry || !s || c[3] <= 0 || Math.abs(o.sy ?? 1) < 0.01) return;
    this.add(this.textEl(s, x, cy, o, { fill: css(c), fillOpacity: c[3] * this.alpha }));
  }
  textHalo(s: string, x: number, cy: number, c: RGBA, halo: RGBA, hw: number, o: T) {
    if (this.dry || !s) return;
    this.add(this.textEl(s, x, cy, o, { fill: css(c), fillOpacity: c[3] * this.alpha, stroke: css(halo), strokeOpacity: halo[3] * this.alpha, strokeWidth: hw, strokeLinejoin: 'round', paintOrder: 'stroke' }));
  }
  image(src: string, x: number, y: number, w: number, h: number) {
    if (this.dry) return true;
    this.add(<image key={this.key()} href={src} x={x} y={y} width={w} height={h} opacity={this.alpha} preserveAspectRatio="xMidYMid meet" />);
    return true;
  }
  /** Raw element (map widgets). */
  raw(e: ReactNode) {
    if (!this.dry) this.add(e);
  }
  /** The SVG of what was drawn (w × h). */
  svg(w: number, h: number) {
    while (this.stack.length > 1) this.pop();
    return (
      <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} style={{ position: 'absolute', inset: 0, overflow: 'hidden' }}>
        <defs>{this.defs}</defs>
        {this.stack[0].kids}
      </svg>
    );
  }
}

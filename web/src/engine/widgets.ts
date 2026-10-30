// Prefab widgets — mirror of crates/cg-core/src/widgets.rs (keep in sync):
// everything is a pure function of (wall-clock time, time on air) so the
// HTML5 output matches the Rust renderer.
import { interpolateText } from './evaluate';
import type { ClockMode, Data, Layer, PlayoutItem } from './types';
import { lineWidth } from './textFit';

export interface TimeCtx {
  /** Wall-clock time (server clock), Unix ms. */
  nowMs: number;
  /** Seconds since the graphic was taken on air (or the designer playhead). */
  ageSecs: number;
  /** Server zone, used when a widget does not set one. */
  defaultTz: string;
  /** Time on air of one layer when the operator controls its widget. */
  layerAge?: (layer: Layer) => number;
  /** The operator controls this layer's clock (paused, played, restarted, or waiting for a manual start). */
  clocked?: (layer: Layer) => boolean;
  /** The operator's state of a spec widget, by layer id (web/src/widgets). */
  widgetState?: (layerId: string) => unknown;
  /** The Operator's preview holds this widget until its Play (none of its controls used yet): its time stands still. */
  held?: boolean;
  /** Project frame rate. */
  fps?: number;
  /** Project language (texts' hyphenation and spaces). */
  lang?: string;
}

/** A scrolling text, as the ticker or roll it behaves like (mirrors LayerKind::scrolling_as). */
export function scrollingAs(l: Layer): Layer | null {
  if (l.type !== 'text' || !l.scroll || l.scroll === 'none') return null;
  const font = { fontFamily: l.fontFamily, fontWeight: l.fontWeight, align: l.align, verticalAlign: l.verticalAlign, uppercase: l.uppercase || l.transform === 'upper' };
  const base = { ...l, ...font };
  return l.scroll === 'horizontal' ? ({ ...base, type: 'ticker', text: l.text, separator: l.separator ?? '   •   ', speed: l.speed ?? 120 } as Layer) : ({ ...base, type: 'roll', text: l.text, speed: l.speed ?? 120, loop: !!l.looping } as Layer);
}

/** Widgets whose animation follows the time on air (mirrors playout::is_timed). */
export function isTimed(l: Layer): boolean {
  switch (l.type) {
    case 'text':
      return !!l.scroll && l.scroll !== 'none';
    case 'clock':
      return l.mode === 'elapsed' || l.mode === 'timer';
    case 'ticker':
    case 'roll':
    case 'counter':
    case 'progress':
    case 'rotator':
    case 'typewriter':
    case 'slideshow':
    case 'barChart':
      return true;
    // A Lottie animation: the operator pauses, plays and restarts it.
    case 'group':
      return l.lottie !== undefined;
    default:
      return false;
  }
}

/** The frame the layers of a Lottie group are drawn at (mirrors PlayoutItem::lottie_frame): the template's, unless the operator controls the group's clock, up to the on-air point. */
export function lottieFrame(group: Layer, frame: number, hold: number, time: TimeCtx): number {
  if (group.type !== 'group' || group.lottie === undefined || !time.layerAge || !time.clocked?.(group) || frame > hold + 1e-6) return frame;
  return Math.min(hold, time.layerAge(group) * (time.fps ?? 50));
}

/** Mirrors PlayoutItem::layer_age (frames). */
export function layerAgeFrames(item: PlayoutItem, age: number, layer: Layer): number {
  const c = item.clocks?.[layer.id];
  if (c) return Math.max(0, (c.pausedAt ?? age) - c.offset);
  return layer.manualStart ? 0 : age;
}

interface Fields {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
  tenth: number;
  duration: boolean;
}

const formatters = new Map<string, Intl.DateTimeFormat | null>();

function formatter(tz: string): Intl.DateTimeFormat | null {
  if (!formatters.has(tz)) {
    try {
      formatters.set(
        tz,
        new Intl.DateTimeFormat('en-US', {
          timeZone: tz || undefined,
          hourCycle: 'h23',
          year: 'numeric',
          month: 'numeric',
          day: 'numeric',
          hour: 'numeric',
          minute: 'numeric',
          second: 'numeric',
        }),
      );
    } catch {
      formatters.set(tz, null);
    }
  }
  return formatters.get(tz)!;
}

function zoneName(tz: string, fallback: string): string {
  const name = tz.trim() || fallback.trim();
  return formatter(name) ? name : fallback.trim();
}

function zonedFields(ms: number, tz: string): Fields {
  const f = formatter(tz) ?? formatter('')!;
  const parts: Record<string, number> = {};
  for (const p of f.formatToParts(new Date(ms))) if (p.type !== 'literal') parts[p.type] = parseInt(p.value, 10);
  return {
    year: parts.year,
    month: parts.month,
    day: parts.day,
    hour: parts.hour % 24,
    minute: parts.minute,
    second: parts.second,
    tenth: Math.floor((((ms % 1000) + 1000) % 1000) / 100),
    duration: false,
  };
}

const TOKENS = ['yyyy', 'yy', 'MM', 'M', 'dd', 'd', 'HH', 'H', 'hh', 'h', 'mm', 'm', 'ss', 's', 'f', 'A'];
const pad = (n: number, w = 2) => String(n).padStart(w, '0');

export function formatTokens(fmt: string, f: Fields): string {
  const h12 = f.hour % 12 === 0 ? 12 : f.hour % 12;
  let out = '';
  let i = 0;
  outer: while (i < fmt.length) {
    for (const t of TOKENS) {
      if (fmt.startsWith(t, i)) {
        switch (t) {
          case 'yyyy':
            out += pad(f.year, 4);
            break;
          case 'yy':
            out += pad(((f.year % 100) + 100) % 100);
            break;
          case 'MM':
            out += pad(f.month);
            break;
          case 'M':
            out += f.month;
            break;
          case 'dd':
            out += pad(f.day);
            break;
          case 'd':
            out += f.day;
            break;
          case 'HH':
            out += pad(f.hour);
            break;
          case 'H':
            out += f.hour;
            break;
          case 'hh':
            out += pad(f.duration ? f.hour : h12);
            break;
          case 'h':
            out += f.duration ? f.hour : h12;
            break;
          case 'mm':
            out += pad(f.minute);
            break;
          case 'm':
            out += f.minute;
            break;
          case 'ss':
            out += pad(f.second);
            break;
          case 's':
            out += f.second;
            break;
          case 'f':
            out += f.tenth;
            break;
          case 'A':
            out += f.duration ? '' : f.hour < 12 ? 'AM' : 'PM';
            break;
        }
        i += t.length;
        continue outer;
      }
    }
    const c = String.fromCodePoint(fmt.codePointAt(i)!);
    out += c;
    i += c.length;
  }
  return out;
}

function durationFields(secs: number): Fields {
  secs = Math.max(0, secs);
  const whole = Math.floor(secs);
  return {
    year: 0,
    month: 0,
    day: 0,
    hour: Math.floor(whole / 3600),
    minute: Math.floor(whole / 60) % 60,
    second: whole % 60,
    tenth: Math.floor((secs - whole) * 10),
    duration: true,
  };
}

function parseNumbers(s: string): number[] | undefined {
  const parts = s
    .trim()
    .split(':')
    .map((p) => p.trim());
  if (parts.some((p) => p === '' || !/^[-+]?\d*\.?\d+$/.test(p))) return undefined;
  return parts.map(Number);
}

/** "[HH:]mm:ss" or plain seconds. */
export function parseDuration(s: string): number | undefined {
  const p = parseNumbers(s);
  if (!p) return undefined;
  if (p.length === 1) return p[0];
  if (p.length === 2) return p[0] * 60 + p[1];
  if (p.length === 3) return p[0] * 3600 + p[1] * 60 + p[2];
  return undefined;
}

/** Offset of `tz` at `ms` (zone time − UTC), in ms. */
function tzOffset(ms: number, tz: string): number {
  const f = zonedFields(ms, tz);
  return Date.UTC(f.year, f.month - 1, f.day, f.hour, f.minute, f.second) - (ms - (((ms % 1000) + 1000) % 1000));
}

function targetMs(target: string, tz: string, nowMs: number): number | undefined {
  const t = target.trim();
  if (t.includes('T') || (t.match(/-/g)?.length ?? 0) >= 2) {
    const m = t.replace(' ', 'T').match(/^(\d{4})-(\d{1,2})-(\d{1,2})T(\d{1,2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?$/);
    if (!m) return undefined;
    const guess = Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +(m[6] ?? 0));
    const first = guess - tzOffset(guess, tz);
    return guess - tzOffset(first, tz);
  }
  const p = parseNumbers(t);
  if (!p || (p.length !== 2 && p.length !== 3)) return undefined;
  const secs = p[0] * 3600 + p[1] * 60 + (p[2] ?? 0);
  const f = zonedFields(nowMs, tz);
  const midnight = nowMs - (((nowMs % 1000) + 1000) % 1000) - (f.hour * 3600 + f.minute * 60 + f.second) * 1000;
  return midnight + secs * 1000;
}

export function clockText(mode: ClockMode, format: string, timezone: string, target: string, data: Data, ctx: TimeCtx): string {
  const fmt = format.trim() ? format : 'HH:mm:ss';
  const tz = zoneName(timezone, ctx.defaultTz);
  switch (mode) {
    case 'time':
      return formatTokens(fmt, zonedFields(ctx.nowMs, tz));
    case 'elapsed':
      return formatTokens(fmt, durationFields(ctx.ageSecs));
    case 'timer': {
      const total = parseDuration(interpolateText(target, data)) ?? 0;
      return formatTokens(fmt, durationFields(total - ctx.ageSecs));
    }
    case 'countdown': {
      const t = targetMs(interpolateText(target, data), tz, ctx.nowMs);
      const remaining = t === undefined ? 0 : (t - ctx.nowMs) / 1000;
      return formatTokens(fmt, durationFields(Math.ceil(Math.max(0, remaining))));
    }
  }
}

const remEuclid = (a: number, b: number) => ((a % b) + b) % b;

/** A crawl's speed on air: the nearest whole number of pixels per frame (at least one), so every frame moves the text by the same whole step (mirrors frame_speed). */
export function frameSpeed(pxs: number, fps?: number): number {
  if (!fps || fps < 1 || !Number.isFinite(pxs)) return pxs;
  return Math.max(1, Math.round(pxs / fps)) * fps;
}

export function scrollPositions(len: number, cycle: number, travelled: number, looping: boolean): number[] {
  if (cycle <= 0 || len <= 0) return [];
  const first = len - travelled;
  if (first >= 0 || !looping) return first + cycle > 0 ? [first] : [];
  const out: number[] = [];
  for (let x = -remEuclid(travelled - len, cycle); x < len; x += cycle) out.push(x);
  return out;
}

export function grow(duration: number, age: number): number {
  if (duration <= 0) return 1;
  const t = Math.min(1, Math.max(0, age / duration));
  return 1 - Math.pow(1 - t, 3);
}

function number(raw: string, data: Data): number {
  const v = parseFloat(interpolateText(raw, data).trim().replace(',', '.').replace(/ /g, ''));
  return Number.isFinite(v) ? v : 0;
}

export function formatNumber(v: number, decimals: number, grouping: boolean): string {
  const s = Math.abs(v).toFixed(decimals);
  let [int, frac] = s.split('.');
  if (grouping) int = int.replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
  const sign = v < 0 && Math.abs(v) >= 0.5 * Math.pow(10, -decimals) ? '-' : '';
  return frac !== undefined ? `${sign}${int},${frac}` : `${sign}${int}`;
}

export function counterText(l: { from: string; to: string; duration: number; decimals: number; prefix: string; suffix: string; grouping: boolean }, data: Data, age: number): string {
  const a = number(l.from, data);
  const b = number(l.to, data);
  const v = a + (b - a) * grow(l.duration, age);
  return `${interpolateText(l.prefix, data)}${formatNumber(v, l.decimals, l.grouping)}${interpolateText(l.suffix, data)}`;
}

export function progressRatio(value: string, max: string, duration: number, data: Data, age: number): number {
  const m = number(max, data);
  const ratio = m > 0 ? Math.min(1, Math.max(0, number(value, data) / m)) : 0;
  return ratio * grow(duration, age);
}

export function lines(text: string, data: Data): string[] {
  return interpolateText(text, data)
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l !== '');
}

export function rotatorState(n: number, interval: number, fade: number, age: number): [number, number] {
  if (n <= 1 || interval <= 0) return [0, 1];
  const idx = Math.floor(Math.max(0, Math.floor(age / interval))) % n;
  const t = remEuclid(age, interval);
  fade = Math.min(interval / 2, Math.max(0, fade));
  let opacity = fade <= 0 ? 1 : Math.min(1, Math.max(0, Math.min(t / fade, (interval - t) / fade)));
  if (age < interval && t < fade) opacity = 1;
  return [idx, opacity];
}

export function slideshowState(n: number, interval: number, fade: number, age: number): [number, number, number] {
  if (n <= 1 || interval <= 0) return [0, 0, 0];
  const idx = Math.max(0, Math.floor(age / interval)) % n;
  const t = remEuclid(age, interval);
  fade = Math.min(interval, Math.max(0, fade));
  const mix = fade > 0 && t > interval - fade ? (t - (interval - fade)) / fade : 0;
  return [idx, (idx + 1) % n, mix];
}

export function typewriterText(text: string, speed: number, cursor: boolean, data: Data, age: number): string {
  const full = [...interpolateText(text, data)];
  const shown = Math.min(full.length, Math.max(0, Math.floor(age * speed)));
  let out = full.slice(0, shown).join('');
  if (cursor && (shown < full.length || Math.floor(age * 2) % 2 === 0)) out += '|';
  return out;
}

export function chartRows(text: string, data: Data): [string, number][] {
  return lines(text, data).map((l) => {
    const i = l.lastIndexOf(':');
    if (i < 0) return [l, 0];
    const v = parseFloat(
      l
        .slice(i + 1)
        .trim()
        .replace(',', '.'),
    );
    return [l.slice(0, i).trim(), Number.isFinite(v) ? v : 0];
  });
}

export function clockHands(timezone: string, ctx: TimeCtx): [number, number, number] {
  const f = zonedFields(ctx.nowMs, zoneName(timezone, ctx.defaultTz));
  return [((f.hour % 12) + f.minute / 60) * 30, (f.minute + f.second / 60) * 6, f.second * 6];
}

// --- text measurement (ticker strip length) ---------------------------------

export function textWidth(text: string, family: string, weight: number, size: number, letterSpacing: number): number {
  if (typeof document === 'undefined' || !hasCanvas()) return text.length * size * 0.5;
  const chars = [...text].length;
  // Measured once per font and text (see lineWidth).
  return lineWidth(`${weight} ${size}px "${family}", Inter, system-ui, sans-serif`, text) + Math.max(0, chars - 1) * letterSpacing;
}

let canvasOk: boolean | undefined;
const hasCanvas = () => (canvasOk ??= !!document.createElement('canvas').getContext('2d'));

// --- QR codes (modules computed by the server so both renderers match) -----

type Qr = { size: number; modules: string };
const qrCache = new Map<string, Qr | null | Promise<void>>();
const qrListeners = new Set<() => void>();

export function onQrLoaded(fn: () => void): () => void {
  qrListeners.add(fn);
  return () => void qrListeners.delete(fn);
}

export function qrModules(content: string): Qr | null {
  const hit = qrCache.get(content);
  if (hit === undefined) {
    qrCache.set(
      content,
      fetch(`/api/qr?data=${encodeURIComponent(content)}`)
        .then((r) => (r.ok ? (r.json() as Promise<Qr>) : null))
        .catch(() => null)
        .then((q) => {
          qrCache.set(content, q);
          qrListeners.forEach((f) => f());
        }),
    );
    return null;
  }
  return hit instanceof Promise ? null : hit;
}


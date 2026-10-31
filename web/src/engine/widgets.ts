// Prefab widgets — mirror of crates/strata-mv-core/src/widgets.rs (keep in sync):
// everything is a pure function of (wall-clock time, time on air) so the
// HTML5 output matches the Rust renderer.
import { interpolateText } from './evaluate';
import type { ClockMode, Data, Layer, PlayoutItem } from './types';
import { lineWidth } from './textFit';

export interface TimeCtx {
  /** Wall-clock time (server clock), Unix ms. */
  nowMs: number;
  /** Seconds since the multiview was taken on air (or the designer playhead). */
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

/** Widgets whose animation follows the time on air (mirrors playout::is_timed). */
export function isTimed(l: Layer): boolean {
  switch (l.type) {
    case 'text':
      return !!l.scroll && l.scroll !== 'none';
    case 'clock':
      return l.mode === 'elapsed' || l.mode === 'timer';
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

export function zonedFields(ms: number, tz: string): Fields {
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

export function lines(text: string, data: Data): string[] {
  return interpolateText(text, data)
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l !== '');
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


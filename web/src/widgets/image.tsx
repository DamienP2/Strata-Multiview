// Image: fits (incl. original size and tile), focal point, crop, rounded / circle mask, stroke,
// colorize, fallback picture when the source is missing.
// Same picture as crates/cg-render/src/wid/image.rs; geometry and actions mirror crates/cg-core/src/wid/image.rs.
import { useEffect, useRef, useState, useSyncExternalStore, type CSSProperties, type ReactNode } from 'react';
import { api, uploadAsset, useApp } from '../store';
import { Film, Image, Pause, Play, Radio, RefreshCw, RotateCcw } from 'lucide-react';
import { AssetLibrary } from '../designer/AssetPicker';
import { inputMessage, useInputs, useLiveFrame } from '../engine/liveInputs';
import { col, cssColor, interpolateText, num } from '../engine/evaluate';
import type { Data, PlayoutItem } from '../engine/types';
import type { TimeCtx } from '../engine/widgets';
import { AssetPicker, ColorGroupRow, GroupRow } from '../designer/Inspector';
import { Bound, Btns, clamp, ColorPick, DataValue, defineWidget, ExposeToggle, Handle, hex, Note, Num, Row, Section, Seg, Toggle, useBound, useCentreMode, useExposable, useKeys, type Warning, withDefaults } from './kit';
import { Centre, StartRow, WaitsContinue, WaitsTrigger } from './centre';
import { confirmDialog } from '../ui/dialogs';
import { lang, t, tc, tn } from '../i18n';

export interface Crop {
  l: number;
  t: number;
  r: number;
  b: number;
}
export interface Config {
  /** Original / Tile on top of the layer's `fit` (contain / cover / stretch), or any of the five when the Operator chooses it (an exposed Fit). */
  fitMode?: 'original' | 'tile' | 'contain' | 'cover' | 'stretch';
  tileScale?: number;
  /** Kept in view when Cover crops (fractions of the picture). */
  focal?: { x: number; y: number };
  crop?: Crop;
  mask?: 'none' | 'rounded' | 'circle';
  radius?: number;
  colorize?: 'none' | 'replace' | 'tint';
  tint?: string;
  /** Shown when the source cannot be found (empty = the built-in slate). */
  fallback?: string;
  /** Animated sequence: pictures played instead of `src`. */
  frames?: string[];
  fps?: number;
  playMode?: 'loop' | 'once' | 'count';
  loops?: number;
  start?: 'in' | 'manual';
  /** A Video layer (the Video widget): a video file played as frames, or a live input. */
  video?: boolean;
  /** The video file the frames come from. */
  clip?: string;
  /** A live video input shown instead ('' = none). */
  input?: string;
  /** In and out points of the sequence (a video file), s at its speed: only the frames between are played (out 0 = to the end). */
  clipIn?: number;
  clipOut?: number;
}

/** Operator state of a sequence (wall-clock ms): started by hand, or taken over (paused, played, restarted) when it starts at the In. */
export interface State {
  playing?: boolean;
  atMs?: number;
  pausedEl?: number;
  /** The operator took over a sequence starting at the In: these times replace its time on air. */
  own?: boolean;
  /** Times the operator reloaded the picture. */
  reload?: number;
}

type Fit = 'contain' | 'cover' | 'stretch' | 'original' | 'tile';
type Rect = [number, number, number, number];
type ImgLayer = Config & { id: string; src: string; fit: 'contain' | 'cover' | 'stretch'; props: Record<string, unknown> };

const D = { tileScale: 100, focal: { x: 0.5, y: 0.5 }, crop: { l: 0, t: 0, r: 0, b: 0 }, mask: 'none' as const, radius: 28, colorize: 'none' as const, tint: '#ff3b5c', fallback: '', frames: [] as string[], fps: 12, playMode: 'loop' as const, loops: 3, start: 'in' as const, video: false, clip: '', input: '', clipIn: 0, clipOut: 0 };
const cfg = (c: Config) => withDefaults(D, c);
const fitOf = (l: ImgLayer): Fit => l.fitMode ?? l.fit ?? 'contain';

/** The frame in the box and its radius (mirrors frame_rect). */
function frameRect(c: Config, w: number, h: number): [Rect, number] {
  const k = cfg(c);
  if (k.mask === 'circle') {
    const d = Math.min(w, h);
    return [[(w - d) / 2, (h - d) / 2, d, d], d / 2];
  }
  if (k.mask === 'rounded') return [[0, 0, w, h], Math.min(Math.max(0, k.radius), w / 2, h / 2)];
  return [[0, 0, w, h], 0];
}

/** The same crop with the picture centred in it (as much cut on each side). */
const centred = (c: Crop): Crop => {
  const h = +((c.l + c.r) / 2).toFixed(3);
  const v = +((c.t + c.b) / 2).toFixed(3);
  return { l: h, r: h, t: v, b: v };
};

/** Crop capped so 5 % of the picture is always left (mirrors crop). */
function cropOf(c: Config): Crop {
  const k = cfg(c).crop;
  const l = clamp(k.l, 0, 0.95);
  const t = clamp(k.t, 0, 0.95);
  return { l, t, r: clamp(k.r, 0, Math.max(0, 0.95 - l)), b: clamp(k.b, 0, Math.max(0, 0.95 - t)) };
}

/** Visible window and whole picture in the frame (mirrors place). */
function place(c: Config, [iw, ih]: [number, number], [fw, fh]: [number, number], fit: Fit, cr: Crop): [Rect, Rect] {
  const focal = cfg(c).focal;
  const cw = Math.max(1e-3, iw * (1 - cr.l - cr.r));
  const ch = Math.max(1e-3, ih * (1 - cr.t - cr.b));
  let sx: number, sy: number, ox: number, oy: number;
  if (fit === 'stretch') [sx, sy, ox, oy] = [fw / cw, fh / ch, 0, 0];
  else {
    const k = fit === 'contain' ? Math.min(fw / cw, fh / ch) : fit === 'cover' ? Math.max(fw / cw, fh / ch) : 1;
    let [x, y] = [(fw - cw * k) / 2, (fh - ch * k) / 2];
    if (fit === 'cover') {
      x = clamp(fw / 2 - (focal.x - cr.l) * iw * k, fw - cw * k, 0);
      y = clamp(fh / 2 - (focal.y - cr.t) * ih * k, fh - ch * k, 0);
    }
    [sx, sy, ox, oy] = [k, k, x, y];
  }
  return [
    [ox, oy, cw * sx, ch * sy],
    [ox - cr.l * iw * sx, oy - cr.t * ih * sy, iw * sx, ih * sy],
  ];
}

/** The frames played, between the in and out points: [first, count] (mirrors clip_range). */
export function clipRange(c: Config): [number, number] {
  const k = cfg(c);
  const total = k.frames.length;
  if (!total) return [0, 0];
  const fps = clamp(k.fps, 0.1, 120);
  const first = Math.min(Math.round(Math.max(0, k.clipIn) * fps), total - 1);
  const end = k.clipOut > 0 ? clamp(Math.round(k.clipOut * fps), first + 1, total) : total;
  return [first, end - first];
}

/** Where a sequence is (mirrors seq): its frame, whether it runs, the seconds left (to the end, or of this loop when it loops forever). */
export function seqInfo(c: Config, st: State | undefined, nowMs: number, ageSecs: number) {
  const k = cfg(c);
  const [first, count] = clipRange(c);
  const n = Math.max(1, count);
  const driven = k.start === 'manual' || !!(c as { manualStart?: boolean }).manualStart || !!st?.own;
  const el = Math.max(0, !driven ? ageSecs : st?.playing ? (nowMs - (st.atMs ?? 0)) / 1000 : (st?.pausedEl ?? 0));
  const loops = k.playMode === 'loop' ? Infinity : k.playMode === 'once' ? 1 : Math.max(1, k.loops);
  const fps = clamp(k.fps, 0.1, 120);
  let f = Math.floor(el * fps);
  let ended = false;
  if (f >= n * loops) ((f = n * loops - 1), (ended = true));
  const lap = n / fps;
  const remaining = Number.isFinite(loops) ? Math.max(0, lap * loops - el) : lap - (el % lap);
  return { idx: first + (f % n), lap: Math.floor(f / n) + 1, loops, ended, el, n, running: !ended && (!driven || !!st?.playing), remaining };
}

/**
 * The time of the controls now. On air, the time on air runs on from the
 * state last received (the server sends none while the timeline holds),
 * re-rendered while `active`; elsewhere (preview, Designer) the time given.
 */
function useNow(time: TimeCtx, item: PlayoutItem | undefined, active: boolean) {
  // When the state holding this item arrived: only that channel's news draw it again.
  const liveAt = useApp((s) => {
    if (!item) return undefined;
    for (const l of Object.values(s.live)) if (Object.values(l.state.layers).includes(item)) return l.at;
    return undefined;
  });
  const [, tick] = useState(0);
  const onAir = !!item && active;
  useEffect(() => {
    if (!onAir) return;
    const h = setInterval(() => document.hidden || tick((n) => n + 1), 250);
    return () => clearInterval(h);
  }, [onAir]);
  const anchor = useRef({ time, at: Date.now() });
  if (anchor.current.time !== time) anchor.current = { time, at: Date.now() };
  if (!item || liveAt === undefined) return { nowMs: time.nowMs, ageSecs: time.ageSecs };
  return { nowMs: time.nowMs + Date.now() - anchor.current.at, ageSecs: (item.age ?? 0) / (time.fps ?? 50) + Math.max(0, Date.now() - liveAt) / 1000 };
}

// --------------------------------------------------------------- pictures --
// Natural sizes and failures, per source (module level: warnings read them too).
const sizes = new Map<string, [number, number]>();
/** Sources that could not be read, and when: tried again after a while (a file imported since shows). */
const failed = new Map<string, number>();
const RETRY_MS = 3000;
const hasFailed = (src: string) => {
  const at = failed.get(src);
  if (at === undefined) return false;
  if (Date.now() - at < RETRY_MS) return true;
  failed.delete(src);
  return false;
};
const loading = new Set<string>();
const subs = new Set<() => void>();
let version = 0;
const bump = () => {
  version++;
  subs.forEach((f) => f());
};
const useStore = () =>
  useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => version,
  );

function load(src: string) {
  if (!src || sizes.has(src) || hasFailed(src) || loading.has(src)) return;
  loading.add(src);
  const img = new window.Image();
  img.onload = () => {
    loading.delete(src);
    sizes.set(src, [img.naturalWidth || 1, img.naturalHeight || 1]);
    bump();
  };
  img.onerror = () => {
    loading.delete(src);
    failed.set(src, Date.now());
    bump();
  };
  img.src = src;
}

/** 'ok' with its size, 'missing', or 'loading'. */
function status(src: string): { s: 'ok'; size: [number, number] } | { s: 'missing' } | { s: 'loading' } {
  src = src.trim();
  if (!src || hasFailed(src)) return { s: 'missing' };
  const z = sizes.get(src);
  if (z) return { s: 'ok', size: z };
  load(src);
  return { s: 'loading' };
}

/** Natural size of a picture, null until it is read (or when missing): loads it. */
export function pictureSize(src: string): [number, number] | null {
  const st = status(src);
  return st.s === 'ok' ? st.size : null;
}
/** Draws again when a picture asked for is read. */
export const usePictures = useStore;

/** The source in effect: frame of the sequence or the picture. */
function wantedSrc(l: ImgLayer, data: Data, idx = 0) {
  const k = cfg(l);
  return interpolateText(k.frames.length ? k.frames[Math.min(idx, k.frames.length - 1)] : l.src, data).trim();
}

// Designer-only UI: crop editing, per layer (not saved).
const ui = { crop: new Set<string>() };
const setUi = (fn: () => void) => (fn(), bump());

// --------------------------------------------------------------- drawing --

const px = (r: Rect): CSSProperties => ({ position: 'absolute', left: r[0], top: r[1], width: r[2], height: r[3] });

/** One picture at rectangle P, colorized. */
function Pic({ src, P, colorize, tint }: { src: string; P: Rect; colorize: string; tint: string }) {
  const q = `url("${src.replace(/"/g, '%22')}")`;
  const filter = colorize === 'tint' ? 'grayscale(1)' : undefined;
  const masked = (bg: string): CSSProperties => ({ ...px(P), background: bg, WebkitMaskImage: q, maskImage: q, WebkitMaskSize: '100% 100%', maskSize: '100% 100%', WebkitMaskRepeat: 'no-repeat', maskRepeat: 'no-repeat' });
  if (colorize === 'replace') return <div style={masked(hex(tint))} />;
  return (
    <>
      <img src={src} alt="" draggable={false} onError={() => (failed.set(src, Date.now()), bump())} style={{ ...px(P), maxWidth: 'none', display: 'block', filter }} />
      {colorize === 'tint' && <div style={{ ...masked(hex(tint)), mixBlendMode: 'multiply' }} />}
    </>
  );
}

/** The pulse line of the slate, in ring radii (mirrors SLATE_PULSE). */
const SLATE_PULSE: [number, number][] = [
  [-0.73, 0.08],
  [-0.36, 0.08],
  [-0.19, -0.4],
  [0.02, 0.52],
  [0.23, -0.6],
  [0.42, 0.08],
  [0.75, 0.08],
];

/** The built-in slate: neutral, branded, never an error on air (same as the Rust one). */
function Slate({ F }: { F: Rect }) {
  const [x, y, w, h] = F;
  const s = Math.min(w, h);
  const [cx, cy, rr] = [x + w / 2, y + h / 2, s * 0.2];
  const id = `slate${Math.round(x)}_${Math.round(w)}`;
  return (
    <svg style={{ position: 'absolute', inset: 0, overflow: 'visible' }} width="100%" height="100%">
      <defs>
        <linearGradient id={id} gradientUnits="userSpaceOnUse" x1={x} y1={y} x2={x + w} y2={y + h}>
          <stop offset="0" stopColor="#1b1442" />
          <stop offset="1" stopColor="#3b0d2a" />
        </linearGradient>
      </defs>
      <rect x={x} y={y} width={w} height={h} fill={`url(#${id})`} />
      <circle cx={cx} cy={cy} r={rr} fill="none" stroke="#fff" strokeWidth={s * 0.045} />
      <path d={SLATE_PULSE.map(([u, v], i) => `${i ? 'L' : 'M'}${cx + u * rr} ${cy + v * rr}`).join('')} fill="none" stroke="#ff3b5c" strokeWidth={s * 0.038} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** The picture read again after `reload` operator reloads (another URL: never the cached copy). */
/** The picture is at a web address (reloading it fetches it again). */
const webPicture = (l: ImgLayer, data: Record<string, string> = {}) => /^https?:\/\//i.test(interpolateText(l.src, data).trim());
const reloaded = (src: string, n: number) => (n > 0 && src ? `${src}${src.includes('?') ? '&' : '?'}reload=${n}` : src);

/** What is shown: the picture, the fallback (missing), or the slate. `reload`: the operator's reloads. */
function resolve(l: ImgLayer, data: Data, idx: number, reload = 0) {
  const k = cfg(l);
  let src = wantedSrc(l, data, idx);
  let st = status(src);
  if (reload > 0 && st.s !== 'missing') {
    // The new copy once it has loaded; the old one meanwhile.
    const fresh = reloaded(src, reload);
    const sf = status(fresh);
    if (sf.s === 'ok') [src, st] = [fresh, sf];
  }
  if (st.s !== 'missing') return { src, st, missing: false };
  const fb = interpolateText(k.fallback, data).trim();
  return { src: fb, st: status(fb), missing: true };
}

/** What the Designer shows in a Video with nothing to play (nothing goes on air). */
function VideoPlaceholder({ icon, title, sub }: { icon: ReactNode; title: string; sub?: string }) {
  return (
    <div style={{ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center', alignContent: 'center', gap: 10, border: '4px dashed rgba(255,255,255,.35)', borderRadius: 8, background: 'rgba(12,14,22,.45)', color: 'rgba(255,255,255,.7)', font: '600 26px Inter, sans-serif', textAlign: 'center', padding: 16 }}>
      {icon}
      <div>{title}</div>
      {sub && <div style={{ font: '500 18px Inter, sans-serif', opacity: 0.7 }}>{sub}</div>}
    </div>
  );
}

function ImageView({ layer, r, w, h, data, time, state, designer }: { layer: ImgLayer; r: Record<string, unknown>; w: number; h: number; data: Data; time: { nowMs: number; ageSecs: number }; state: unknown; designer?: boolean }) {
  useStore();
  const k = cfg(layer);
  // On air every picture; in the Designer a lighter preview.
  const live = useLiveFrame(k.input.trim(), designer ? 'preview' : 'stream');
  const inputs = useInputs();
  // Every frame of a sequence decoded once, up front: cycling never stutters.
  useEffect(() => k.frames.forEach((f) => load(interpolateText(f, data).trim())), [k.frames, data]);
  const sq = seqInfo(layer, state as State, time.nowMs, time.ageSecs);
  const reload = (state as State | undefined)?.reload ?? 0;
  if (k.input.trim() && !(live.url && live.size)) {
    const info = inputs.find((i) => i.id === k.input.trim());
    return designer ? <VideoPlaceholder icon={<Radio size={44} />} title={info ? info.name : t('Live input')} sub={info ? (inputMessage(info.message) || t('No signal')) : t('This input does not exist any more')} /> : null;
  }
  if (k.video && !k.input.trim() && !k.frames.length && !layer.src.trim()) return designer ? <VideoPlaceholder icon={<Film size={44} />} title={t('Video')} sub={t('Choose a file or a live input on the right')} /> : null;
  const isLive = !!(k.input.trim() && live.url && live.size);
  const { src, st, missing } = isLive ? { src: live.url!, st: { s: 'ok' as const, size: live.size! }, missing: false } : resolve(layer, data, sq.idx, reload);
  const [F, radius] = frameRect(layer, w, h);
  const sw = num(r.strokeWidth as never);
  const fit: Fit = missing ? 'cover' : fitOf(layer);
  // Neither the fallback nor a live input is colorized (as the engine).
  const colorize = missing || isLive ? 'none' : k.colorize;
  let inner: ReactNode = null;
  if (st.s === 'missing') inner = <Slate F={[0, 0, F[2], F[3]]} />;
  else if (st.s === 'ok') {
    const [iw, ih] = st.size;
    if (designer && ui.crop.has(layer.id) && fit !== 'tile' && !missing) {
      // Crop editing: the whole picture (contain), outside the crop dimmed.
      const [R] = place(layer, [iw, ih], [w, h], 'contain', { l: 0, t: 0, r: 0, b: 0 });
      const c = cropOf(layer);
      const [cx, cy, cw, ch] = [R[0] + c.l * R[2], R[1] + c.t * R[3], R[2] * (1 - c.l - c.r), R[3] * (1 - c.t - c.b)];
      return (
        <div style={{ position: 'absolute', inset: 0 }}>
          <Pic src={src} P={R} colorize="none" tint="" />
          <svg width={w} height={h} style={{ position: 'absolute', inset: 0 }}>
            <path fillRule="evenodd" fill="#000" opacity={0.6} d={`M${R[0]} ${R[1]}h${R[2]}v${R[3]}h${-R[2]}Z M${cx} ${cy}h${cw}v${ch}h${-cw}Z`} />
            <rect x={cx} y={cy} width={cw} height={ch} fill="none" stroke="#f59e0b" strokeWidth={4} strokeDasharray="14 10" />
          </svg>
        </div>
      );
    }
    if (fit === 'tile') {
      const t = clamp(k.tileScale, 1, 1000) / 100;
      const q = `url("${src.replace(/"/g, '%22')}")`;
      const size = `${iw * t}px ${ih * t}px`;
      inner =
        colorize === 'replace' ? (
          <div style={{ position: 'absolute', inset: 0, background: hex(k.tint), WebkitMaskImage: q, maskImage: q, WebkitMaskSize: size, maskSize: size, WebkitMaskRepeat: 'repeat', maskRepeat: 'repeat' }} />
        ) : (
          <>
            <div style={{ position: 'absolute', inset: 0, backgroundImage: q, backgroundSize: size, backgroundRepeat: 'repeat', filter: colorize === 'tint' ? 'grayscale(1)' : undefined }} />
            {colorize === 'tint' && <div style={{ position: 'absolute', inset: 0, background: hex(k.tint), mixBlendMode: 'multiply', WebkitMaskImage: q, maskImage: q, WebkitMaskSize: size, maskSize: size }} />}
          </>
        );
    } else {
      const [win, pic] = place(layer, [iw, ih], [F[2], F[3]], fit, missing ? { l: 0, t: 0, r: 0, b: 0 } : cropOf(layer));
      inner = (
        <div style={{ ...px(win), overflow: 'hidden', isolation: 'isolate' }}>
          <Pic src={src} P={[pic[0] - win[0], pic[1] - win[1], pic[2], pic[3]]} colorize={colorize} tint={k.tint} />
        </div>
      );
    }
  }
  return (
    <div style={{ position: 'absolute', inset: 0 }}>
      <div style={{ ...px(F), borderRadius: radius, overflow: 'hidden', isolation: 'isolate' }}>{inner}</div>
      {sw > 0 && <div style={{ ...px(F), borderRadius: radius, boxShadow: `inset 0 0 0 ${sw}px ${cssColor(col(r.stroke as never))}`, pointerEvents: 'none' }} />}
    </div>
  );
}

// ------------------------------------------------------------- inspector --

/** Source thumbnail with the focal point and the crop; a click sets the focal point. */
function Thumb({ src, size, layer, edit }: { src: string; size: [number, number]; layer: ImgLayer; edit: (fn: (l: ImgLayer) => void, key?: string) => void }) {
  const k = cfg(layer);
  const c = cropOf(layer);
  return (
    <div
      style={{ position: 'relative', width: `min(100%, ${Math.round((170 * size[0]) / size[1])}px)`, aspectRatio: `${size[0]} / ${size[1]}`, background: `url("${src}") 0 0 / 100% 100%`, borderRadius: 6, margin: '4px auto', cursor: 'crosshair' }}
      onClick={(e) => {
        const b = e.currentTarget.getBoundingClientRect();
        edit((l) => void (l.focal = { x: +clamp((e.clientX - b.left) / b.width, 0, 1).toFixed(3), y: +clamp((e.clientY - b.top) / b.height, 0, 1).toFixed(3) }));
      }}
    >
      <div style={{ position: 'absolute', left: `${c.l * 100}%`, top: `${c.t * 100}%`, right: `${c.r * 100}%`, bottom: `${c.b * 100}%`, outline: '2px dashed #f59e0b' }} />
      <div style={{ position: 'absolute', left: `${k.focal.x * 100}%`, top: `${k.focal.y * 100}%`, width: 16, height: 16, margin: -8, borderRadius: '50%', border: '3px solid #fff', background: '#f59e0b', boxShadow: '0 1px 4px #000' }} />
    </div>
  );
}

/** The picture: chosen in the library, or exposed to the Operator (who then picks it); from data it is read only. */
function PictureRow() {
  const x = useExposable('src', '', undefined, 'image');
  const b = useBound();
  if (x.wired) return <DataValue label={t('Picture')} value={interpolateText(x.raw, x.data)} />;
  return (
    <Row label={t('Picture')} tail={x.canExpose ? <ExposeToggle on={x.exposed} onClick={() => x.toggleExpose()} /> : undefined}>
      <AssetPicker
        value={x.shown}
        clearable={false}
        onChange={(v) => {
          x.change(v);
          // A new picture: the old crop means nothing for it.
          if (!x.exposed) b.set('crop', undefined);
        }}
      />
    </Row>
  );
}

type StartLayer = { id: string; name?: string; manualStart?: boolean; startOn?: 'continue' | 'trigger'; startTrigger?: string };

/** The sequence waits for the operator: the layer's manual start, or the older `start: manual`. */
const manualOf = (l: Config & { manualStart?: boolean }) => !!l.manualStart || cfg(l).start === 'manual';

/** Seconds left as mm:ss. */
const leftText = (s: number) => {
  const v = Math.ceil(Math.max(0, s) - 1e-6);
  return `${String(Math.floor(v / 60)).padStart(2, '0')}:${String(v % 60).padStart(2, '0')}`;
};

/**
 * The control centre (web/src/widgets/centre.tsx). A sequence (animation, video file): the time
 * left, how it waits for its start; Play / Pause, Restart; how it starts, how it plays. A still
 * picture at a web address: Refresh. A live input: nothing (`controls`).
 */
function MediaBar({ layer, state, data, act, time, item }: { layer: unknown; state: unknown; data: Data; act: (op: string, args?: unknown) => void; time: TimeCtx; item?: PlayoutItem }) {
  const l = layer as ImgLayer & StartLayer;
  const k = cfg(l);
  const b = useBound();
  const designer = useCentreMode() === 'designer';
  const st = (state ?? {}) as State;
  const now = useNow(time, item, k.frames.length > 0);
  const s = k.frames.length ? seqInfo(l, st, now.nowMs, now.ageSecs) : null;
  // The preview and the Designer's try-out hold it on its first frame until its Play.
  const held = time.held !== undefined ? !!time.held : !item && state == null;
  const manual = manualOf(l);
  // Started by hand: waits on its first frame until the operator (Continue, its trigger, Play) starts it.
  const waiting = !!s && manual && !st.own && !held;
  const running = !!s && s.running && !held && !waiting;
  const paused = !!s && !s.ended && !running && !waiting && (st.playing === false || held);
  const pstate: 'stop' | 'run' | 'pause' | 'ended' = !s ? 'stop' : running ? 'run' : s.ended ? 'ended' : paused ? 'pause' : 'stop';
  // Confirmations only on air (the T key does as the ⟲ button).
  const restart = async () => {
    if (item && !(await confirmDialog({ title: t('{name} is on air: start it again from its start value?', { name: l.name ?? '' }), confirm: t('Yes') }))) return;
    act('restart');
  };
  const play = () => act(pstate === 'run' ? 'pause' : pstate === 'ended' || held ? 'restart' : 'play');
  const web = !k.video && webPicture(l, data);
  useKeys(
    s
      ? {
          p: play,
          t: () => void restart(),
        }
      : web
        ? { r: () => act('reload') }
        : {},
  );
  if (k.input.trim()) return null;
  if (!s) return web ? <Centre name={l.name} actions={[{ icon: RefreshCw, label: tc('cc', 'Refresh'), key: 'R', onClick: () => act('reload') }]} /> : null;
  // The settings of line 3 as the template (or the take) has them.
  const kb = cfg((b.obj ?? l) as Config);
  const status = item && waiting ? l.startOn === 'trigger' ? <WaitsTrigger name={l.startTrigger ?? ''} /> : <WaitsContinue /> : undefined;
  const sub = s.ended ? tc('cc', 'finished') : s.loops === 1 ? tc('cc', 'left') : `${tc('cc', 'left')} · ${tc('cc', 'loop {n}', { n: s.lap })}`;
  // The start row writes the layer's manual start; the older `start: manual` then goes.
  const setStart = (path: string, v: unknown, coalesce?: boolean) => {
    b.set(path, v, coalesce);
    if (path === 'manualStart' && k.start === 'manual') b.set('start', undefined);
  };
  return (
    <Centre
      name={l.name}
      readout={{ v: leftText(s.remaining), sub }}
      status={status}
      actions={[
        pstate === 'run' ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: play } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: play },
        { icon: RotateCcw, label: tc('cc', 'Restart'), key: 'T', confirm: item ? t('{name} is on air: start it again from its start value?', { name: l.name ?? '' }) : undefined, onClick: () => act('restart') },
      ]}
    >
      {designer && (
        <Bound obj={b.obj} set={setStart} data={b.data}>
          <StartRow layer={{ ...l, manualStart: manual || undefined }} />
        </Bound>
      )}
      <Seg
        label={tc('cc', 'Playback')}
        path="playMode"
        value={k.playMode}
        options={[
          ['loop', tc('cc', 'Loop')],
          ['once', tc('cc', 'Once')],
          ['count', tc('cc', 'N times')],
        ]}
      />
      {kb.playMode === 'count' && <Num label={t('Times')} path="loops" value={k.loops} min={1} max={999} />}
    </Centre>
  );
}

export default defineWidget({
  id: 'image',
  type: 'image',
  label: 'Image',
  group: 'Shapes & tools',
  icon: Image,
  create: () => ({ src: '', fit: 'contain', props: { width: 800, height: 560, stroke: [1, 1, 1, 1], strokeWidth: 0 } }),
  ownsLook: true,
  content: ['src'],
  live: (l) => (l.frames?.length ?? 0) > 0 || !!l.input,
  Render: ({ layer, r, w, h, data, time, state, designer }) => <ImageView layer={layer as unknown as ImgLayer} r={r as never} w={w} h={h} data={data} time={time} state={state} designer={designer} />,
  Inspector: ({ layer, data, edit }) => {
    useStore();
    const l = layer as unknown as ImgLayer;
    const ed = edit as unknown as (fn: (l: ImgLayer) => void, key?: string) => void;
    const k = cfg(l);
    const fit = fitOf(l);
    const { src, st, missing } = resolve(l, data, 0);
    const cropping = ui.crop.has(layer.id);
    const stroke = num(l.props.strokeWidth as never) > 0;
    return (
      <>
        {k.video ? (
          <VideoSource k={k} ed={ed} />
        ) : (
          <>
            <Section id="image-source" title={t('Picture')}>
              <PictureRow />
              <Row expose="fallback" exposeValue={k.fallback} label={t('Fallback')}>
                <AssetPicker value={k.fallback} title={t('Choose the fallback picture')} placeholder={t('None')} onChange={(v) => ed((x) => void (x.fallback = v || undefined))} />
              </Row>
            </Section>
            <Sequence k={k} ed={ed} />
          </>
        )}
        <Section id="image-fit" title={t('Fit & crop')}>
          <Seg
            // Exposed: the Operator chooses among the five (fitMode wins over fit).
            expose="fitMode"
            exposeValue={fit}
            label={t('Fit')}
            value={fit}
            onChange={(v) =>
              ed((x) => {
                if (v === 'original' || v === 'tile') x.fitMode = v;
                else ((x.fitMode = undefined), (x.fit = v));
              })
            }
            options={[
              ['contain', t('Contain')],
              ['cover', t('Cover')],
              ['stretch', t('Stretch')],
              ['original', t('Original')],
              ['tile', t('Tile')],
            ]}
          />
          {fit === 'tile' && <Num label={t('Tile size')} path="tileScale" value={k.tileScale} min={5} max={400} unit="%" />}
          {fit === 'cover' && !missing && st.s === 'ok' && (
            <Row expose={['focal.x', 'focal.y']} exposeValue={{ 'focal.x': k.focal.x, 'focal.y': k.focal.y }} label={t('Focal point')}>
              <Thumb src={src} size={st.size} layer={l} edit={ed} />
            </Row>
          )}
          {fit !== 'tile' && (
            <>
              <Toggle label={t('Crop')} value={cropping} onChange={(v) => setUi(() => (v ? ui.crop.add(layer.id) : ui.crop.delete(layer.id)))} />
              {cropping && (
                <>
                  {(['l', 'r', 't', 'b'] as const).map((s) => (
                    <Num
                      expose={`crop.${s}`}
                      exposeValue={cropOf(l)[s]}
                      key={s}
                      label={t({ l: 'Left', t: 'Top', r: 'Right', b: 'Bottom' }[s])}
                      value={Math.round(cropOf(l)[s] * 100)}
                      min={0}
                      max={95}
                      unit="%"
                      onChange={(v) => ed((x) => void (x.crop = { ...cropOf(x), [s]: v / 100 }), `crop-${layer.id}`)}
                    />
                  ))}
                  <Btns
                    list={[
                      { label: t('Centre the picture'), cls: 'ghost', onClick: () => ed((x) => void (x.crop = centred(cropOf(x)))) },
                      { label: t('Reset crop'), cls: 'ghost', onClick: () => ed((x) => void delete x.crop) },
                    ]}
                  />
                </>
              )}
            </>
          )}
        </Section>
        <Section id="image-shape" title={t('Shape')}>
          <Seg
            label={t('Mask')}
            path="mask"
            value={k.mask}
            options={[
              ['none', t('Square')],
              ['rounded', t('Rounded')],
              ['circle', t('Circle')],
            ]}
          />
          {k.mask === 'rounded' && <Num label={t('Radius')} path="radius" value={k.radius} min={0} max={2000} unit="px" />}
          <Toggle
            // The width is exposed by its own row (Width): the toggle is not.
            expose={false}
            label={t('Stroke')}
            value={stroke}
            onChange={(v) =>
              ed((x) => {
                x.props.strokeWidth = v ? 4 : 0;
                if (v && col((x.props.stroke as never) ?? [0, 0, 0, 0])[3] === 0) x.props.stroke = [1, 1, 1, 1];
              })
            }
          />
          {stroke && (
            <>
              <ColorGroupRow layer={layer} prop="stroke" label={t('Color')} />
              <GroupRow layer={layer} spec={{ label: t('Width'), props: ['strokeWidth'], min: 0, suffix: 'px' }} />
            </>
          )}
        </Section>
        {/* A live input is never colorized. */}
        {!k.input.trim() && <Section id="image-colorize" title={t('Colorize')}>
          <Seg
            label={t('Mode')}
            path="colorize"
            value={k.colorize}
            options={[
              ['none', t('Off')],
              ['replace', t('Replace'), t('Every opaque pixel takes the colour (one-colour logos)')],
              ['tint', t('Tint'), t('Greys take the colour')],
            ]}
          />
          {k.colorize !== 'none' && <ColorPick label={t('Colour')} path="tint" value={D.tint} />}
        </Section>}
      </>
    );
  },

  // A sequence (animation, video file): time left, play / pause (it freezes), restart; a still picture: reload. A live input: none.
  // A sequence: play / pause; a picture at a web address: reload.
  controls: (layer, data) => {
    const k = cfg(layer as unknown as ImgLayer);
    return !k.input.trim() && (k.frames.length > 0 || (!k.video && webPicture(layer as unknown as ImgLayer, data)));
  },
  // A sequence: the Operator's preview holds it on its first frame until its Play.
  plays: (layer) => {
    const k = cfg(layer as unknown as ImgLayer);
    return !k.input.trim() && k.frames.length > 0 && 'start';
  },
  Operator: ({ layer, state, data, act, time, item }) => <MediaBar layer={layer} state={state} data={data} act={act} time={time} item={item} />,
  // A sequence started by hand waits for Continue (F2) until the operator starts it (mirrors waits_start).
  waits: (layer, state) => {
    const l = layer as unknown as ImgLayer & { manualStart?: boolean; startOn?: string };
    const k = cfg(l);
    return k.frames.length > 0 && !k.input.trim() && manualOf(l) && l.startOn !== 'trigger' && !(state as State | undefined)?.own;
  },

  Overlay: ({ layer, w, h, scale, edit }) => {
    useStore();
    const l = layer as unknown as ImgLayer;
    const ed = edit as unknown as (fn: (l: ImgLayer) => void, key?: string) => void;
    const fit = fitOf(l);
    const { st, missing } = resolve(l, {}, 0);
    if (st.s !== 'ok' || missing) return null;
    const c = cropOf(l);
    if (ui.crop.has(layer.id) && fit !== 'tile') {
      const [R] = place(l, st.size, [w, h], 'contain', { l: 0, t: 0, r: 0, b: 0 });
      const fx = (x: number) => (x - R[0]) / R[2];
      const fy = (y: number) => (y - R[1]) / R[3];
      const r3 = (v: number) => +v.toFixed(3);
      // The kept part moved over the picture (its size kept): the picture recentred in it.
      const move = (e: React.PointerEvent) => {
        e.stopPropagation();
        e.preventDefault();
        const c0 = c;
        const [x0, y0] = [e.clientX, e.clientY];
        const shift = (ev: PointerEvent) => {
          const dx = (ev.clientX - x0) / scale / R[2];
          const dy = (ev.clientY - y0) / scale / R[3];
          const l2 = r3(clamp(c0.l + dx, 0, c0.l + c0.r));
          const t2 = r3(clamp(c0.t + dy, 0, c0.t + c0.b));
          ed((q) => void (q.crop = { l: l2, r: r3(c0.r - (l2 - c0.l)), t: t2, b: r3(c0.b - (t2 - c0.t)) }), `crop-${layer.id}`);
        };
        const up = () => {
          removeEventListener('pointermove', shift);
          removeEventListener('pointerup', up);
        };
        addEventListener('pointermove', shift);
        addEventListener('pointerup', up);
      };
      return (
        <>
          <div
            className="img-crop-move"
            title={t('Drag to move the picture in the crop · double-click to centre it')}
            style={{ left: (R[0] + c.l * R[2]) * scale, top: (R[1] + c.t * R[3]) * scale, width: R[2] * (1 - c.l - c.r) * scale, height: R[3] * (1 - c.t - c.b) * scale }}
            onPointerDown={move}
            onDoubleClick={(e) => {
              e.stopPropagation();
              ed((q) => void (q.crop = centred(cropOf(q))), `crop-${layer.id}`);
            }}
          />
          <Handle x={R[0] + c.l * R[2]} y={R[1] + c.t * R[3]} scale={scale} label={t('Crop: top left')} onDrag={(x, y) => ed((q) => void (q.crop = { ...cropOf(q), l: r3(clamp(fx(x), 0, 0.95 - c.r)), t: r3(clamp(fy(y), 0, 0.95 - c.b)) }), `crop-${layer.id}`)} />
          <Handle x={R[0] + (1 - c.r) * R[2]} y={R[1] + (1 - c.b) * R[3]} scale={scale} label={t('Crop: bottom right')} onDrag={(x, y) => ed((q) => void (q.crop = { ...cropOf(q), r: r3(clamp(1 - fx(x), 0, 0.95 - c.l)), b: r3(clamp(1 - fy(y), 0, 0.95 - c.t)) }), `crop-${layer.id}`)} />
        </>
      );
    }
    if (fit === 'cover') {
      const k = cfg(l);
      const [F] = frameRect(l, w, h);
      const ux = (k.focal.x - c.l) / (1 - c.l - c.r);
      const uy = (k.focal.y - c.t) / (1 - c.t - c.b);
      return (
        <Handle
          x={F[0] + clamp(ux, 0, 1) * F[2]}
          y={F[1] + clamp(uy, 0, 1) * F[3]}
          scale={scale}
          label={t('Focal point: kept in view when Cover crops')}
          onDrag={(x, y) =>
            ed((q) => {
              const cc = cropOf(q);
              const [G] = frameRect(q, w, h);
              q.focal = { x: +(cc.l + clamp((x - G[0]) / G[2], 0, 1) * (1 - cc.l - cc.r)).toFixed(3), y: +(cc.t + clamp((y - G[1]) / G[3], 0, 1) * (1 - cc.t - cc.b)).toFixed(3) };
            }, `focal-${layer.id}`)
          }
        />
      );
    }
    return null;
  },
  warnings: (layer, { data, state }) => {
    const l = layer as unknown as ImgLayer;
    const k = cfg(l);
    const out: Warning[] = [];
    const wanted = wantedSrc(l, data, 0);
    const r = resolve(l, data, 0);
    // A video shows its own placeholder until a file or an input is chosen.
    if (r.missing && !k.input && !(k.video && !wanted)) {
      const slate = r.st.s === 'missing';
      const also = !!k.fallback && slate;
      const v = { file: wanted, fallback: slate ? k.fallback : r.src };
      const text = wanted
        ? slate
          ? also
            ? t('File not found: {file}. On air: the channel slate instead (the fallback {fallback} cannot be found either).', v)
            : t('File not found: {file}. On air: the channel slate instead.', v)
          : t('File not found: {file}. On air: the fallback {fallback} instead.', v)
        : slate
          ? also
            ? t('No picture chosen: the channel slate is shown (the fallback {fallback} cannot be found either).', v)
            : t('No picture chosen: the channel slate is shown.', v)
          : t('No picture chosen: the fallback {fallback} is shown.', v);
      out.push({ level: 'error', text, op: true });
    }
    const missingFrames = k.frames.filter((f) => status(interpolateText(f, data)).s === 'missing').length;
    if (k.frames.length && missingFrames) out.push({ level: 'warn', text: tn(missingFrames, '{n} frame of the sequence cannot be found: the fallback shows in their place.', '{n} frames of the sequence cannot be found: the fallback shows in their place.'), op: true });
    if (k.colorize !== 'none' && r.missing) out.push({ level: 'info', text: t('Colorize does not apply to the fallback picture.') });
    if (k.frames.length && !r.missing && (manualOf(l as Config & { manualStart?: boolean }) || (state as State | undefined)?.own)) {
      const s = seqInfo(l, state as State, Date.now(), 0);
      if (!s.running) out.push({ level: 'info', text: s.ended ? t('Sequence finished on frame {n}/{total}.', { n: s.idx + 1, total: s.n }) : t('Sequence paused on frame {n}/{total}.', { n: s.idx + 1, total: s.n }), op: 'only' });
    }
    return out;
  },
});

/** Animated sequence: frames played instead of the picture (an animated GIF / APNG is split into frames). */
function Sequence({ k, ed }: { k: ReturnType<typeof cfg>; ed: (fn: (l: ImgLayer) => void, key?: string) => void }) {
  const [busy, setBusy] = useState(false);
  const pid = useApp((s) => s.project?.id);
  const toast = useApp((s) => s.toast);
  const importFile = async (f: File) => {
    if (!pid) return;
    setBusy(true);
    try {
      const r = await api<{ frames: string[]; fps: number }>('POST', `/api/projects/${encodeURIComponent(pid)}/sequence/${encodeURIComponent(f.name)}`, f);
      ed((x) => {
        x.frames = r.frames;
        x.fps = Math.round(r.fps * 100) / 100;
      });
      toast(tn(r.frames.length, '{n} frame at {fps} fps', '{n} frames at {fps} fps', { fps: Math.round(r.fps * 10) / 10 }), 'info');
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  // A PNG sequence: a zip (split by the server), or the pictures of a folder (sent one by one), in the order of their names.
  const fps0 = useApp((s) => (s.project ? s.project.frameRate.num / s.project.frameRate.den : 25));
  const importPngs = async (files: File[]) => {
    if (!pid || !files.length) return;
    const zip = files.find((f) => /\.zip$/i.test(f.name));
    if (zip) return importFile(zip);
    const pngs = files.filter((f) => /\.png$/i.test(f.name) || f.type === 'image/png').sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
    if (!pngs.length) return toast(t('No PNG picture in this folder'), 'error');
    setBusy(true);
    try {
      const stem = (pngs[0].webkitRelativePath.split('/').slice(-2, -1)[0] || pngs[0].name.replace(/[-_ ]*\d*\.png$/i, '') || 'sequence').replace(/[^\w-]+/g, '-');
      const frames: string[] = [];
      for (const [i, f] of pngs.entries()) frames.push(await uploadAsset(new File([f], `${stem}-${String(i).padStart(4, '0')}.png`, { type: 'image/png' }), pid));
      ed((x) => {
        x.frames = frames;
        x.fps = Math.round(fps0 * 100) / 100;
      });
      toast(tn(frames.length, '{n} frame at {fps} fps', '{n} frames at {fps} fps', { fps: Math.round(fps0 * 10) / 10 }), 'info');
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  // A dropped folder: its pictures.
  const dropped = async (items: DataTransferItemList) => {
    const out: File[] = [];
    const walk = async (e: FileSystemEntry): Promise<void> => {
      if (e.isFile) out.push(await new Promise<File>((res, rej) => (e as FileSystemFileEntry).file(res, rej)));
      else if (e.isDirectory) {
        const r = (e as FileSystemDirectoryEntry).createReader();
        for (let batch: FileSystemEntry[] = []; (batch = await new Promise((res, rej) => r.readEntries(res, rej))).length; ) for (const x of batch) await walk(x);
      }
    };
    for (const e of [...items].map((i) => i.webkitGetAsEntry()).filter((x): x is FileSystemEntry => !!x)) await walk(e);
    return importPngs(out);
  };
  const n = k.frames.length;
  return (
    <Section id="image-anim" title={n ? tn(n, 'Animation · {n} frame', 'Animation · {n} frames') : t('Animation')} closed={!n}>
      <Row label={t('Import')}>
        <label className={`btn small ${busy ? 'disabled' : ''}`} title={t("An animated GIF or PNG (APNG): split into frames in the project's media")}>
          {busy ? t('Splitting…') : t('Animated GIF / PNG…')}
          <input type="file" accept="image/gif,image/png,image/apng" hidden disabled={busy} onChange={(e) => (e.target.files?.[0] && void importFile(e.target.files[0]), (e.target.value = ''))} />
        </label>
      </Row>
      <Row label={t('PNG sequence')}>
        <label
          className={`btn small ${busy ? 'disabled' : ''}`}
          title={t('A folder or a zip of numbered PNG pictures (with transparency): choose the pictures of the folder, or drop it here')}
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => (e.preventDefault(), void dropped(e.dataTransfer.items))}
        >
          {t('Folder or zip…')}
          <input type="file" accept=".zip,image/png" multiple hidden disabled={busy} onChange={(e) => (void importPngs([...(e.target.files ?? [])]), (e.target.value = ''))} />
        </label>
      </Row>
      {n > 0 && (
        <div className="seq-frames">
          {k.frames.map((f, i) => (
            <span key={i} className="seq-frame" title={f}>
              <img src={f} alt="" loading="lazy" />
              <i>{i + 1}</i>
              <button title={t('Remove this frame')} onClick={() => ed((x) => void (x.frames = cfg(x).frames.filter((_, j) => j !== i)))}>
                ×
              </button>
            </span>
          ))}
        </div>
      )}
      <Row label={t('Add a frame')}>
        <AssetPicker value="" placeholder={t('Choose a picture…')} title={t('Add a frame at the end')} onChange={(v) => v && ed((x) => void (x.frames = [...cfg(x).frames, v]))} />
      </Row>
      {n > 0 && (
        <>
          <Playback k={k} />
          <Btns list={[{ label: t('Remove the animation'), cls: 'ghost', onClick: () => ed((x) => void (x.frames = [])) }]} />
        </>
      )}
    </Section>
  );
}

/** How a sequence or a video file plays. */
function Playback({ k }: { k: ReturnType<typeof cfg> }) {
  return (
    <>
      <Num label={t('Speed')} path="fps" value={k.fps} min={0.1} max={120} step={1} unit="fps" />
      <Seg
        label={t('Play')}
        path="playMode"
        value={k.playMode}
        options={[
          ['loop', t('Loop')],
          ['once', t('Once')],
          ['count', t('Times')],
        ]}
      />
      {k.playMode === 'count' && <Num label={t('Times')} path="loops" value={k.loops} min={1} max={999} />}
    </>
  );
}

/** Seconds as mm:ss,d. */
const clipTime = (s: number) => {
  const d = Math.round(Math.max(0, s) * 10);
  const out = `${String(Math.floor(d / 600)).padStart(2, '0')}:${((d % 600) / 10).toFixed(1).padStart(4, '0')}`;
  return lang() === 'fr' ? out.replace('.', ',') : out;
};
/** 3 · 3,5 · 0:03,5 · 1:02.5 → seconds. */
const parseClipTime = (s: string) => {
  const m = /^(?:(\d+):)?(\d+(?:\.\d*)?)$/.exec(s.trim().replace(',', '.'));
  return m ? Number(m[1] ?? 0) * 60 + Number(m[2]) : null;
};

/** A time of the clip (mm:ss,d): typed, or ↑ ↓ by a tenth (Shift: a second). */
function ClipTime({ label, value, max, onChange }: { label: string; value: number; max: number; onChange: (v: number) => void }) {
  const [text, setText] = useState<string | null>(null);
  const put = (v: number) => onChange(Math.round(clamp(v, 0, max) * 100) / 100);
  const commit = () => {
    const v = text === null ? null : parseClipTime(text);
    if (v !== null) put(v);
    setText(null);
  };
  return (
    <Row label={label}>
      <span className="pp-field">
        <label className="num-field">
          <input
            value={text ?? clipTime(value)}
            title={t('Minutes:seconds — or type 3, 3.5, 0:03.5')}
            onFocus={(e) => (setText(clipTime(value)), e.currentTarget.select())}
            onChange={(e) => setText(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === 'Enter') e.currentTarget.blur();
              if (e.key === 'Escape') (setText(null), e.currentTarget.blur());
              if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
                e.preventDefault();
                put(value + (e.key === 'ArrowUp' ? 1 : -1) * (e.shiftKey ? 1 : 0.1));
                setText(null);
              }
            }}
          />
        </label>
      </span>
    </Row>
  );
}

/** In and out points of a video file: only the frames between are played. */
function ClipPoints({ k, ed }: { k: ReturnType<typeof cfg>; ed: (fn: (l: ImgLayer) => void, key?: string) => void }) {
  const fps = clamp(k.fps, 0.1, 120);
  const total = k.frames.length / fps;
  const [first, count] = clipRange(k);
  const out = (first + count) / fps;
  return (
    <>
      <ClipTime label={t('In point')} value={first / fps} max={Math.max(0, out - 1 / fps)} onChange={(v) => ed((x) => void (x.clipIn = v || undefined))} />
      <ClipTime label={t('Out point')} value={out} max={total} onChange={(v) => ed((x) => void (x.clipOut = v >= total - 1e-6 ? undefined : Math.max(v, first / fps + 1 / fps)))} />
    </>
  );
}

const fileName = (src: string) => decodeURIComponent(src.split('?')[0].split('/').pop() || src);

/** The Video widget's source: a file of the assets (played as frames) or a live input. */
function VideoSource({ k, ed }: { k: ReturnType<typeof cfg>; ed: (fn: (l: ImgLayer) => void, key?: string) => void }) {
  const inputs = useInputs();
  const pid = useApp((s) => s.project?.id);
  const toast = useApp((s) => s.toast);
  const [lib, setLib] = useState(false);
  const [busy, setBusy] = useState(false);
  const input = k.input.trim();
  const live = useLiveFrame(input);
  const info = inputs.find((i) => i.id === input);
  const choose = async (src: string) => {
    setLib(false);
    if (!pid) return;
    setBusy(true);
    try {
      const r = await api<{ frames: string[]; fps: number }>('POST', `/api/projects/${encodeURIComponent(pid)}/sequence-from`, { src });
      ed((x) => {
        x.frames = r.frames;
        x.fps = Math.round(r.fps * 100) / 100;
        x.clip = src;
        x.src = '';
        x.input = undefined;
        // Another file: its own in and out points.
        x.clipIn = x.clipOut = undefined;
        if (!x.playMode) x.playMode = 'once';
      });
      toast(tn(r.frames.length, '{n} frame at {fps} fps', '{n} frames at {fps} fps', { fps: Math.round(r.fps * 10) / 10 }), 'info');
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <Section id="video-source" title={t('Video')}>
      <Seg
        label={t('Source')}
        value={input ? 'input' : 'file'}
        onChange={(v) => {
          if (v === 'file') return ed((x) => void (x.input = undefined));
          if (!inputs.length) return toast(t('No live input yet: add one in Broadcast › Video inputs.'), 'error');
          ed((x) => void (x.input = inputs[0].id));
        }}
        options={[
          ['file', t('File')],
          ['input', t('Live input')],
        ]}
      />
      {input ? (
        <>
          <Row label={t('Input')}>
            <select value={input} onChange={(e) => ed((x) => void (x.input = e.target.value))}>
              {!info && <option value={input}>{t('(missing: {id})', { id: input })}</option>}
              {inputs.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name} · {i.kind.toUpperCase()}
                </option>
              ))}
            </select>
          </Row>
          <div className="video-live">
            {live.url ? <img src={live.url} alt="" /> : <span>{t('No signal')}</span>}
            <em className={info?.live ? 'ok' : ''}>
              <i /> {info ? inputMessage(info.message) : t('This input does not exist any more')}
            </em>
          </div>
          <Note>{t('Live inputs are set in Broadcast › Video inputs. The engine draws their latest picture on every output.')}</Note>
        </>
      ) : (
        <>
          <Row label={t('File')}>
            <button type="button" className="pick video-pick" disabled={busy} onClick={() => setLib(true)} title={k.clip || undefined}>
              <Film size={14} />
              <span className={`pick-nm ${k.clip ? '' : 'ph'}`}>{busy ? t('Converting…') : k.clip ? fileName(k.clip) : t('Choose a video…')}</span>
            </button>
          </Row>
          {k.frames.length > 0 && (
            <>
              <Note>{tn(k.frames.length, '{n} frame at {fps} fps', '{n} frames at {fps} fps', { fps: k.fps })} · {t('the video is played as pictures (alpha kept), at most a minute')}</Note>
              <ClipPoints k={k} ed={ed} />
              <Playback k={k} />
            </>
          )}
        </>
      )}
      {lib && <AssetLibrary kind="video" title={t('Choose a video')} value={k.clip} onClose={() => setLib(false)} onChoose={(src) => void choose(src)} />}
    </Section>
  );
}

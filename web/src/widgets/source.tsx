// Source: one tile of a multiview. The picture of a live input, framed by its tally (red on air, green
// next, yellow aux), with its UMD (the name bar, its tally lamps at each end) and the meters of its audio
// channels. The lamps, the UMD text and the levels come with the input (engine/liveInputs.ts useSignal).
// Same picture as crates/strata-mv-render/src/wid/source.rs; settings and geometry mirror
// crates/strata-mv-core/src/wid/source.rs (keep in sync).
import { MonitorPlay } from 'lucide-react';
import { interpolateText, num } from '../engine/evaluate';
import { LiveCanvas } from '../engine/LiveCanvas';
import { alarmText, streamWidth, useInputs, useLiveFrame, useLiveStream, useSignal, type Alarm, type InputInfo, type SignalView, type Tally } from '../engine/liveInputs';
import { CRIT, loudColor, num1, stationTimecode, TARGET, WARN } from './broadcast';
import type { Data } from '../engine/types';
import { FontPick } from '../ui/FontPick';
import { t, tc, tl } from '../i18n';
import { ColorPick, defineWidget, hex, Num, Row, Section, Seg, Select, Text, Toggle, withDefaults } from './kit';
import { FitLine } from './text-fit';
import { SmoothMeters } from './SmoothMeters';

export type Fit = 'contain' | 'cover' | 'stretch';

export interface TallyLook {
  border: boolean;
  program: string;
  preview: string;
  aux: string;
  off: string;
}

export interface Umd {
  position: 'bottom' | 'top' | 'off';
  /** Height, % of the tile's. */
  size: number;
  /** Beside the picture (smaller) instead of over it. */
  outside: boolean;
  /** '' = the switcher's UMD, else the input's name; {{key}} replaced. */
  text: string;
  font: string;
  weight: number;
  align: 'left' | 'center' | 'right';
  color: string;
  background: string;
  /** Tally lamps at each end: on air left, next right. */
  lamps: boolean;
}

export interface Vu {
  position: 'right' | 'left' | 'off';
  width: number;
  gap: number;
  margin: number;
  /** dBFS: bottom of the scale, yellow from, red from. */
  min: number;
  yellow: number;
  red: number;
  hold: boolean;
  /** At most this many channels (0: all). */
  max: number;
}

export interface Config {
  input?: string;
  fit?: Fit;
  background?: string;
  border?: number;
  tally?: Partial<TallyLook>;
  umd?: Partial<Umd>;
  vu?: Partial<Vu>;
  badge?: boolean;
  noSignal?: string;
  /** No picture: SMPTE bars and the bouncing card, or the background and the words. */
  noSignalLook?: 'bars' | 'plain';
  /** The timecode, top right: the input's, the station's, or none. */
  timecode?: 'off' | 'input' | 'station';
  /** The short-term loudness, bottom right. */
  loudness?: boolean;
  /** The faults the server sees: a banner, the frame in their colour. */
  alarms?: boolean;
}

const D_TALLY: TallyLook = { border: true, program: '#e5212b', preview: '#17b14a', aux: '#f2b705', off: '#262a33' };
const D_UMD: Umd = { position: 'bottom', size: 11, outside: false, text: '', font: 'Inter', weight: 600, align: 'center', color: '#ffffff', background: '#000000b3', lamps: true };
const D_VU: Vu = { position: 'right', width: 6, gap: 2, margin: 6, min: -60, yellow: -18, red: -9, hold: true, max: 0 };
const D = { input: '', fit: 'contain' as Fit, background: '#0b0d12', border: 4, badge: false, noSignal: '', noSignalLook: 'bars' as 'bars' | 'plain', timecode: 'off' as 'off' | 'input' | 'station', loudness: false, alarms: true };

export type Full = typeof D & { tally: TallyLook; umd: Umd; vu: Vu };

export const cfg = (c: Config): Full => ({ ...withDefaults(D, c), tally: withDefaults(D_TALLY, c.tally ?? {}), umd: withDefaults(D_UMD, c.umd ?? {}), vu: withDefaults(D_VU, c.vu ?? {}) }) as Full;

/** The meters' colours (green, yellow, red), their track, a lamp that is off (mirrors METER, TRACK, LAMP_OFF). */
export const METER = ['#2bd96b', '#f2c12e', '#ff3b3b'];
/** Meters shown at rest when the input sends no audio (mirrors SILENT_CHANNELS of wid/source.rs). */
const SILENT_CHANNELS = 2;
const LAMP_OFF = '#ffffff26';

type Area = [number, number, number, number];

export interface Layout {
  inner: Area;
  video: Area;
  umd: Area | null;
  lamps: [Area, Area] | null;
  text: Area | null;
  meters: Area[];
}

const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));

/** Where everything of a `w` × `h` tile goes (mirrors wid::source::layout). */
export function layout(c: Full, w: number, h: number, channels: number): Layout {
  const b = clamp(c.border, 0, Math.max(0, Math.min(w, h) / 4));
  const inner: Area = [b, b, Math.max(0, w - 2 * b), Math.max(0, h - 2 * b)];
  const video: Area = [...inner];
  let umd: Area | null = null;
  let lamps: [Area, Area] | null = null;
  let text: Area | null = null;
  let top = 0;
  let bottom = 0;
  if (c.umd.position !== 'off' && inner[2] > 0 && inner[3] > 0) {
    const bh = Math.min(Math.max((clamp(c.umd.size, 0, 50) / 100) * h, 8), inner[3] / 2);
    const y = c.umd.position === 'top' ? inner[1] : inner[1] + inner[3] - bh;
    umd = [inner[0], y, inner[2], bh];
    if (c.umd.outside) {
      video[3] -= bh;
      if (c.umd.position === 'top') video[1] += bh;
    } else if (c.umd.position === 'top') top = bh;
    else bottom = bh;
    const pad = Math.round(bh * 0.2);
    const tx: Area = [umd[0] + pad, umd[1], Math.max(0, umd[2] - 2 * pad), bh];
    if (c.umd.lamps) {
      const s = Math.max(4, Math.round(bh * 0.56));
      const ly = umd[1] + (bh - s) / 2;
      lamps = [
        [umd[0] + pad, ly, s, s],
        [umd[0] + umd[2] - pad - s, ly, s, s],
      ];
      tx[0] += s + pad;
      tx[2] = Math.max(0, tx[2] - 2 * (s + pad));
    }
    text = tx;
  }
  const meters: Area[] = [];
  if (c.vu.position !== 'off' && channels > 0) {
    const n = c.vu.max > 0 ? Math.min(channels, c.vu.max) : channels;
    const [bw, gap, m] = [Math.max(1, c.vu.width), Math.max(0, c.vu.gap), Math.max(0, c.vu.margin)];
    const total = n * bw + Math.max(0, n - 1) * gap;
    const y0 = video[1] + top + m;
    const y1 = video[1] + video[3] - bottom - m;
    if (y1 > y0 && total + 2 * m <= video[2]) {
      const x0 = c.vu.position === 'left' ? video[0] + m : video[0] + video[2] - m - total;
      for (let i = 0; i < n; i++) meters.push([x0 + i * (bw + gap), y0, bw, y1 - y0]);
    }
  }
  return { inner, video, umd, lamps, text, meters };
}

/** The frame's colour by the tally: on air first, then next, then aux (mirrors border_color). */
export function borderColor(c: Full, tally: Tally): string {
  if (!c.tally.border) return c.tally.off;
  if (tally.program) return c.tally.program;
  if (tally.preview) return c.tally.preview;
  if (tally.aux) return c.tally.aux;
  return c.tally.off;
}

/** Where the overlays go: timecode, loudness, the fault's banner, and their text size (mirrors overlay). */
export function overlay(c: Full, l: Layout, h: number) {
  const size = Math.round(clamp(h * 0.05, 9, 30));
  const m = Math.max(c.vu.margin, 4);
  const v = l.video;
  const right = c.vu.position === 'right' && l.meters.length ? l.meters[0][0] - m : v[0] + v[2] - m;
  let [top, bottom] = [v[1] + m, v[1] + v[3] - m];
  if (l.umd && !c.umd.outside) {
    if (c.umd.position === 'top') top = l.umd[1] + l.umd[3] + m;
    else bottom = l.umd[1] - m;
  }
  const [tw, lw] = [Math.round(11 * 0.6 * size + size), Math.round(5 * 0.6 * size + size)];
  const bh = Math.round(size * 1.5);
  const [bannerH, bannerW] = [Math.round(size * 2), Math.round(v[2] * 0.7)];
  return {
    tc: [right - tw, top, tw, bh] as Area,
    lufs: [right - lw, bottom - bh, lw, bh] as Area,
    banner: [v[0] + (v[2] - bannerW) / 2, v[1] + (v[3] - bannerH) / 2, bannerW, bannerH] as Area,
    size,
  };
}

/** The frame with the faults: a critical one blinks, a warning stays (mirrors alarm_border). */
export function alarmBorder(c: Full, tally: Tally, alarms: Alarm[], nowMs: number): string {
  if (c.alarms && alarms.length) {
    if (alarms[0].severity === 'critical' && ((Math.floor(nowMs / 500) % 2) + 2) % 2 === 0) return '#dc2626';
    if (alarms[0].severity === 'warning') return '#fbbf24';
  }
  return borderColor(c, tally);
}

/** The worst fault and how many others (mirrors alarm_banner). */
export const alarmBanner = (alarms: Alarm[], lang?: string) => (!alarms.length ? '' : alarms.length === 1 ? alarmText(alarms[0].kind, lang) : `${alarmText(alarms[0].kind, lang)} +${alarms.length - 1}`);

/** The UMD text: the one set, else the switcher's, else the input's name (mirrors umd_text). */
export function umdText(c: Full, info: InputInfo | undefined, signal: SignalView, data: Data): string {
  const own = interpolateText(c.umd.text, data).trim();
  if (own) return own;
  if (!info) return '';
  return signal.tally?.umd?.trim() || info.name;
}

/** A level on the scale: 0 at `min`, 1 at full scale (mirrors scale). */
export const scale = (v: Vu, db: number) => {
  const min = Math.min(v.min, -1);
  return clamp((db - min) / -min, 0, 1);
};

/** The parts of a meter lit at `peak`, bottom up: [from, to, colour index] (mirrors segments). */
export function segments(v: Vu, peak: number): [number, number, number][] {
  const lv = scale(v, peak);
  const y = scale(v, v.yellow);
  const r = scale(v, Math.max(v.red, v.yellow));
  const out: [number, number, number][] = [];
  for (const [from, to, k] of [
    [0, y, 0],
    [y, r, 1],
    [r, 1, 2],
  ] as const) {
    const top = Math.min(lv, to);
    if (top > from) out.push([from, top, k]);
  }
  return out;
}

/** "1080i50", "720p59.94" (mirrors inputs.rs format_name). */
export function formatName(i: InputInfo | undefined): string {
  if (!i || !i.live || !i.height || !i.fps) return '';
  const rate = Math.abs(i.fps - Math.round(i.fps)) < 0.005 ? String(Math.round(i.fps)) : i.fps.toFixed(2);
  return `${i.height}${i.interlaced ? 'i' : 'p'}${rate}`;
}

/** SMPTE colour bars (mirrors wid::source::BARS): x, y, width, height in fractions of the picture, colour. */
const [S, T, M, B, P] = [1 / 7, 2 / 3, 0.75, 5 / 28, 1 / 21];
export const BARS: [number, number, number, number, string][] = [
  [0, 0, S, T, '#c0c0c0'],
  [S, 0, S, T, '#c0c000'],
  [2 * S, 0, S, T, '#00c0c0'],
  [3 * S, 0, S, T, '#00c000'],
  [4 * S, 0, S, T, '#c000c0'],
  [5 * S, 0, S, T, '#c00000'],
  [6 * S, 0, S, T, '#0000c0'],
  [0, T, S, M - T, '#0000c0'],
  [S, T, S, M - T, '#131313'],
  [2 * S, T, S, M - T, '#c000c0'],
  [3 * S, T, S, M - T, '#131313'],
  [4 * S, T, S, M - T, '#00c0c0'],
  [5 * S, T, S, M - T, '#131313'],
  [6 * S, T, S, M - T, '#c0c0c0'],
  [0, M, B, 1 - M, '#00214c'],
  [B, M, B, 1 - M, '#ffffff'],
  [2 * B, M, B, 1 - M, '#32006a'],
  [3 * B, M, B + P, 1 - M, '#131313'],
  [4 * B + P, M, P, 1 - M, '#090909'],
  [4 * B + 2 * P, M, P, 1 - M, '#1d1d1d'],
  [6 * S, M, S, 1 - M, '#131313'],
];

/** Width over height of the F AudioVisuel signature, quill included (mirrors LOGO_RATIO); same file as crates/strata-mv-render/src/slate_signature.svg. */
const LOGO_RATIO = 910 / 135.2;
const SIGNATURE = new URL('./signature.svg', import.meta.url).href;

const bounce = (travel: number, range: number) => {
  if (range <= 0) return 0;
  const p = (((travel / range) % 2) + 2) % 2;
  return range * (p > 1 ? 2 - p : p);
};

/** The card bouncing over the bars at `nowMs` (mirrors no_signal_card). */
export function noSignalCard(v: Area, nowMs: number, seed: number): { plate: Area; logo: Area; text: Area; radius: number } | null {
  const [vx, vy, vw, vh] = v;
  let lh = clamp(vh * 0.1, 8, 110);
  let lw = lh * LOGO_RATIO;
  if (lw > vw * 0.55) {
    const k = (vw * 0.55) / lw;
    lw *= k;
    lh *= k;
  }
  const pad = Math.round(lh * 0.4);
  const [gap, th] = [lh * 0.25, lh * 0.55];
  const [pw, ph] = [lw + 2 * pad, pad + lh + gap + th + pad];
  if (lh < 6 || pw > vw || ph > vh) return null;
  const t = nowMs / 1000;
  const x = vx + bounce(t * vw * 0.1 + seed * 3, vw - pw);
  const y = vy + bounce(t * vh * 0.13 + seed * 5, vh - ph);
  return { plate: [x, y, pw, ph], logo: [x + pad, y + pad, lw, lh], text: [x + pad, y + pad + lh + gap, lw, th], radius: pad * 0.6 };
}

const box = (a: Area, extra?: React.CSSProperties): React.CSSProperties => ({ position: 'absolute', left: a[0], top: a[1], width: a[2], height: a[3], ...extra });

function SourceView({ layer, w, h, data, designer, lang, nowMs, tz, fps, seed }: { layer: Config & { name: string }; w: number; h: number; data: Data; designer?: boolean; lang?: string; nowMs: number; tz: string; fps: number; seed: number }) {
  const c = cfg(layer);
  const id = c.input.trim();
  // The Designer: a preview, ten a second. The outputs: every picture, played out evenly.
  const live = useLiveFrame(designer ? id : '', 'preview');
  const inputs = useInputs();
  const signal = useSignal(id);
  const info = id ? inputs.find((i) => i.id === id) : undefined;
  const levels = info ? (signal.levels ?? []) : [];
  // The meters always show (two channels at rest without audio): the tile looks the same with or without sound.
  const l = layout(c, w, h, Math.max(levels.length, SILENT_CHANNELS));
  const tally: Tally = (info && signal.tally) || {};
  const v = l.video;
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const stream = useLiveStream(designer ? '' : id, v[2] > 0 ? streamWidth(v[2] * dpr) : 0);
  const size = designer ? live.size : (stream?.size ?? null);
  // Lost (no picture for 2 s): the bars, not its last picture frozen.
  const lost = !!signal.alarms?.some((a) => a.kind === 'loss');
  const picture = !!(id && size && !lost && (designer ? live.url : stream));
  let pic: React.ReactNode = null;
  if (picture && v[2] > 0 && v[3] > 0) {
    const [iw, ih] = size!;
    let [sx, sy] = [v[2] / iw, v[3] / ih];
    if (c.fit === 'contain') sx = sy = Math.min(sx, sy);
    else if (c.fit === 'cover') sx = sy = Math.max(sx, sy);
    const [pw, ph] = [iw * sx, ih * sy];
    pic = (
      <div style={box(v, { overflow: 'hidden' })}>
        {designer ? (
          <img src={live.url!} alt="" draggable={false} style={{ position: 'absolute', left: (v[2] - pw) / 2, top: (v[3] - ph) / 2, width: pw, height: ph, maxWidth: 'none' }} />
        ) : (
          <LiveCanvas stream={stream!} style={{ position: 'absolute', left: (v[2] - pw) / 2, top: (v[3] - ph) / 2, width: pw, height: ph }} />
        )}
      </div>
    );
  } else if (c.noSignalLook === 'bars' && v[2] > 0 && v[3] > 0) {
    const s = c.noSignal.trim() || tl(lang, 'NO SIGNAL');
    const card = noSignalCard(v, nowMs, seed);
    pic = (
      <>
        {BARS.map(([x, y, bw, bh, col], i) => (
          <div key={i} style={box([v[0] + x * v[2], v[1] + y * v[3], bw * v[2], bh * v[3]], { background: col })} />
        ))}
        {card && (
          <>
            <div style={box(card.plate, { background: hex('#000000c7'), borderRadius: card.radius })} />
            <img src={SIGNATURE} alt="" draggable={false} style={box(card.logo, { maxWidth: 'none' })} />
            <FitLine text={s} size={card.text[3] * 0.82} weight={700} ls={card.text[3] * 0.82 * 0.06} align="center" box={card.text} color="#ffffff" fit={{ overflow: 'shrink', minSize: card.text[3] * 0.41, minSqueeze: 70 }} />
          </>
        )}
      </>
    );
  } else {
    const s = c.noSignal.trim() || tl(lang, 'NO SIGNAL');
    const size = clamp(v[3] * 0.09, 10, 64);
    pic = <FitLine text={s} size={size} weight={700} ls={size * 0.08} align="center" box={[v[0] + 8, v[1], Math.max(1, v[2] - 16), v[3]]} color={hex('#ffffff66')} fit={{ overflow: 'shrink', minSize: size * 0.5, minSqueeze: 70 }} />;
  }
  const fmt = c.badge ? formatName(info) : '';
  const bsize = clamp(h * 0.045, 9, 28);
  const bpad = Math.round(bsize * 0.4);
  const name = umdText(c, info, signal, data) || (designer && !id ? layer.name : '');
  const lamp = (a: Area, lit: boolean | undefined, col: string) => <div style={box(a, { background: hex(lit ? col : LAMP_OFF), borderRadius: a[2] * 0.2 })} />;
  const alarms = info ? (signal.alarms ?? []) : [];
  const o = overlay(c, l, h);
  const tc = c.timecode === 'input' ? (info ? (signal.timecode ?? '') : '') : c.timecode === 'station' ? stationTimecode(nowMs, tz, fps) : '';
  const lo = info ? signal.loudness : undefined;
  const tag = (a: Area, s: string, size: number, bg: string, fg: string, mono: boolean) => (
    <>
      <div style={box(a, { background: hex(bg), borderRadius: size * 0.25 })} />
      <FitLine text={s} size={size} weight={mono ? 700 : 800} family={mono ? 'JetBrains Mono' : undefined} align="center" box={a} color={hex(fg)} fit={{ overflow: 'shrink', minSize: size * 0.55, minSqueeze: 70 }} />
    </>
  );
  return (
    <div style={{ position: 'absolute', inset: 0, overflow: 'hidden', background: hex(alarmBorder(c, tally, alarms, nowMs)) }}>
      <div style={box(l.inner, { background: hex(c.background) })} />
      {pic}
      {fmt && (
        <div style={box([v[0] + bpad, v[1] + bpad, 0, bsize * 1.5], { width: 'auto', padding: `0 ${bpad}px`, background: hex('#000000b3'), borderRadius: bsize * 0.25, font: `700 ${bsize}px Inter, sans-serif`, color: '#fff', display: 'flex', alignItems: 'center', whiteSpace: 'pre' })}>{fmt}</div>
      )}
      {/* At the display's rate, the peaks falling smoothly (on the outputs' timeline, with the picture). */}
      {l.meters.length > 0 && <SmoothMeters id={info ? id : ''} areas={l.meters} vu={c.vu} />}
      {tc && tag(o.tc, tc, o.size, '#000000b3', '#fbbf24', true)}
      {c.loudness && lo && tag(o.lufs, num1(lo[1]), o.size, '#000000b3', loudColor(lo[1], lo[4], TARGET, 1), true)}
      {c.alarms && alarms.length > 0 && picture && tag(o.banner, alarmBanner(alarms, lang), o.size * 1.1, ...((alarms[0].severity === 'critical' ? CRIT : WARN) as [string, string]), false)}
      {l.umd && (
        <>
          <div style={box(l.umd, { background: hex(c.umd.background) })} />
          {l.lamps && (
            <>
              {lamp(l.lamps[0], tally.program, c.tally.program)}
              {lamp(l.lamps[1], tally.preview, c.tally.preview)}
            </>
          )}
          {l.text && name && (
            <div style={box(l.text, { overflow: 'hidden' })}>
              <FitLine
                text={name}
                size={Math.max(6, l.umd[3] * 0.56)}
                weight={c.umd.weight}
                family={c.umd.font}
                align={c.umd.align}
                box={[0, 0, Math.max(1, l.text[2]), l.text[3]]}
                color={hex(c.umd.color)}
                fit={{ overflow: 'shrink', minSize: Math.max(6, l.umd[3] * 0.56) * 0.6, minSqueeze: 75 }}
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}

function InputPick({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const inputs = useInputs();
  const known = !value || inputs.some((i) => i.id === value);
  return (
    <Select
      label={t('Input')}
      value={value}
      onChange={onChange}
      options={[['', t('None')] as [string, string], ...inputs.map((i) => [i.id, i.name] as [string, string]), ...(known ? [] : [[value, t('(removed input)')] as [string, string]])]}
    />
  );
}

export default defineWidget({
  id: 'source',
  type: 'source',
  label: 'Source',
  group: 'Multiview',
  icon: MonitorPlay,
  create: () => ({ props: { width: 960, height: 540 } }),
  ownsLook: true,
  // Its picture, tally and meters change by themselves.
  live: () => true,
  Render: ({ layer, w, h, r, data, designer, lang, time, fps }) => <SourceView layer={layer as Config & { name: string }} w={w} h={h} data={data} designer={designer} lang={lang} nowMs={time.nowMs} tz={time.defaultTz} fps={time.fps || fps} seed={num(r.x) + num(r.y)} />,
  Inspector: ({ layer, set }) => {
    const c = cfg(layer as Config);
    // The tally shows on the frame and in the UMD's lamps: both on or off.
    const tally = c.tally.border || (c.umd.position !== 'off' && c.umd.lamps);
    return (
      <>
        <Section id="source-video" title={t('Video')}>
          <InputPick value={c.input} onChange={(v) => set('input', v)} />
          <Seg
            label={t('Fit')}
            value={c.fit}
            onChange={(v) => set('fit', v)}
            options={[
              ['contain', t('Contain')],
              ['cover', t('Cover')],
              ['stretch', t('Stretch')],
            ]}
          />
          <Toggle label={t('Format')} text={t('The signal format, top left')} value={c.badge} onChange={(v) => set('badge', v)} />
          <Seg
            label={t('No signal')}
            value={c.noSignalLook}
            onChange={(v) => set('noSignalLook', v)}
            options={[
              ['bars', t('Colour bars')],
              ['plain', t('Plain')],
            ]}
          />
          <Text label={t('Words')} value={c.noSignal} placeholder={t('NO SIGNAL')} onChange={(v) => set('noSignal', v)} />
        </Section>
        <Section id="source-audio" title={t('Audio')}>
          <Seg
            label={t('Meters')}
            value={c.vu.position}
            onChange={(v) => set('vu.position', v)}
            options={[
              ['left', t('Left')],
              ['right', t('Right')],
              ['off', t('None')],
            ]}
          />
          {c.vu.position !== 'off' && (
            <>
              <Num label={t('Channels')} value={c.vu.max} min={0} max={16} onChange={(v) => set('vu.max', v)} />
              <Num label={t('Bar width')} value={c.vu.width} min={1} max={60} unit=" px" onChange={(v) => set('vu.width', v)} />
              <Num label={t('Gap')} value={c.vu.gap} min={0} max={40} unit=" px" onChange={(v) => set('vu.gap', v)} />
              <Num label={t('Margin')} value={c.vu.margin} min={0} max={100} unit=" px" onChange={(v) => set('vu.margin', v)} />
              <Num label={t('Scale from')} value={c.vu.min} min={-120} max={-6} unit=" dBFS" onChange={(v) => set('vu.min', v)} />
              <Num label={t('Yellow from')} value={c.vu.yellow} min={-60} max={0} unit=" dBFS" onChange={(v) => set('vu.yellow', v)} />
              <Num label={t('Red from')} value={c.vu.red} min={-60} max={0} unit=" dBFS" onChange={(v) => set('vu.red', v)} />
              <Toggle label={t('Peak hold')} value={c.vu.hold} onChange={(v) => set('vu.hold', v)} />
            </>
          )}
          <Toggle label={t('Loudness')} text={t('Short-term, bottom right')} value={c.loudness} onChange={(v) => set('loudness', v)} />
        </Section>
        <Section id="source-data" title={t('Data (ANC)')}>
          <Seg
            label={t('Timecode')}
            value={c.timecode}
            onChange={(v) => set('timecode', v)}
            options={[
              ['off', t('None')],
              ['input', t('The input’s')],
              ['station', t('Station')],
            ]}
          />
        </Section>
        <Section id="source-tally" title={t('Tally')}>
          <Toggle
            label={t('Show the tally')}
            text={t('The frame and the UMD lamps')}
            value={tally}
            onChange={(v) => {
              set('tally.border', v);
              set('umd.lamps', v);
            }}
          />
          <Num label={t('Frame')} value={c.border} min={0} max={60} unit=" px" onChange={(v) => set('border', v)} />
          {tally && (
            <>
              <ColorPick label={t('On air')} value={c.tally.program} onChange={(v) => set('tally.program', v)} />
              <ColorPick label={tc('tally', 'Next')} value={c.tally.preview} onChange={(v) => set('tally.preview', v)} />
              <ColorPick label={t('Aux')} value={c.tally.aux} onChange={(v) => set('tally.aux', v)} />
            </>
          )}
          <ColorPick label={tally ? t('No tally') : t('Frame colour')} value={c.tally.off} onChange={(v) => set('tally.off', v)} />
        </Section>
        <Section id="source-umd" title="UMD">
          <Seg
            label={t('Position')}
            value={c.umd.position}
            onChange={(v) => set('umd.position', v)}
            options={[
              ['bottom', t('Bottom')],
              ['top', t('Top')],
              ['off', t('None')],
            ]}
          />
          {c.umd.position !== 'off' && (
            <>
              <Text label={t('Text')} value={c.umd.text} placeholder={t('From the switcher, else the input name')} onChange={(v) => set('umd.text', v)} />
              <Num label={t('Height')} value={c.umd.size} min={2} max={50} step={0.5} precision={1} unit=" %" onChange={(v) => set('umd.size', v)} />
              <Toggle label={t('Outside')} text={t('Beside the picture, not over it')} value={c.umd.outside} onChange={(v) => set('umd.outside', v)} />
              <Row label={t('Font')}>
                <FontPick family={c.umd.font} weight={c.umd.weight} onChange={(f, wt) => (set('umd.font', f), set('umd.weight', wt))} />
              </Row>
              <Seg
                label={t('Align')}
                value={c.umd.align}
                onChange={(v) => set('umd.align', v)}
                options={[
                  ['left', t('Left')],
                  ['center', t('Centre')],
                  ['right', t('Right')],
                ]}
              />
              <ColorPick label={t('Text colour')} value={c.umd.color} onChange={(v) => set('umd.color', v)} />
              <ColorPick label={t('Bar')} value={c.umd.background} onChange={(v) => set('umd.background', v)} />
            </>
          )}
        </Section>
        <Section id="source-alarms" title={t('Alarms')}>
          <Toggle label={t('Alarms')} text={t('Black, frozen, silence, loudness…')} value={c.alarms} onChange={(v) => set('alarms', v)} />
        </Section>
      </>
    );
  },
});

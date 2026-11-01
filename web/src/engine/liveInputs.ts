// Live video inputs (crates/strata-mv-server/src/inputs.rs) for the Video widget:
// their list and status, and their pictures, shared by everything on the
// page showing the same input, and stopped when nothing shows it any more.
// - HTML5 outputs (and the Operator's program) read a Motion JPEG stream:
//   every picture, smoothly;
// - the Designer and the Video streams page ask for the newest picture a
//   few times a second: short requests that get through any proxy and never
//   hold one of the few connections a browser opens to a server.
import { useEffect, useState, useSyncExternalStore } from 'react';
import { t } from '../i18n';

export interface InputInfo {
  id: string;
  name: string;
  kind: string;
  /** A picture came in the last two seconds. */
  live: boolean;
  message: string;
  width: number;
  height: number;
  /** Pictures a second (fields when interlaced). */
  fps?: number;
  interlaced?: boolean;
}

/** Server messages, in the interface language. */
export function inputMessage(m: string): string {
  let x = /^Receiving (\d+)×(\d+)(?: \((.+)\))?$/.exec(m);
  if (x) return t('Receiving {size}', { size: `${x[1]}×${x[2]}${x[3] ? ` · ${x[3]}` : ''}` });
  x = /^Waiting for a signal \((.+)\)…$/.exec(m);
  if (x) return t('Waiting for a signal ({format})…', { format: x[1] });
  x = /^Connecting to (.+)…$/.exec(m);
  if (x) return t('Connecting to {source}…', { source: x[1] });
  x = /^Stopped: (.*) — trying again$/.exec(m);
  if (x) return t('Stopped: {reason} — trying again', { reason: x[1] });
  x = /^This address is a list of (\d+) channels: choose one$/.exec(m);
  if (x) return t('This address is a list of {n} channels: choose one', { n: x[1] });
  x = /^yt-dlp could not be fetched \((.*)\)$/.exec(m);
  if (x) return t('yt-dlp could not be downloaded ({reason})', { reason: x[1] });
  if (/will begin|not currently live|is offline|not live|has ended|premieres in|is not live/i.test(m)) return t('Not live right now — {reason}', { reason: m });
  switch (m) {
    case 'Looking for the stream on the page…':
      return t('Looking for the stream on the page…');
    case 'Starting…':
      return t('Starting…');
    case 'Waiting for the sender…':
      return t('Waiting for the sender…');
    case 'Connecting…':
      return t('Connecting…');
    case 'Not started (save the settings)':
      return t('Not applied yet');
    case 'No signal':
      return t('No signal');
    case 'no NDI source chosen':
      return t('No NDI source chosen');
    default:
      return m;
  }
}

// ----------------------------------------------------------------- list --

let list: InputInfo[] = [];
/** The answer the list was read from: the same one again changes nothing. */
let listText = '';
let listUsers = 0;
let listTimer: ReturnType<typeof setInterval> | null = null;
const listSubs = new Set<() => void>();

async function fetchList() {
  try {
    const r = await fetch('/api/inputs', { credentials: 'same-origin' });
    if (!r.ok) return;
    const text = await r.text();
    if (text === listText) return;
    listText = text;
    list = JSON.parse(text) as InputInfo[];
    listSubs.forEach((f) => f());
  } catch {
    /* offline */
  }
}

/** The inputs and how they are doing (refreshed every 3 s while used). */
export function useInputs(): InputInfo[] {
  useEffect(() => {
    if (listUsers++ === 0) {
      void fetchList();
      listTimer = setInterval(() => document.hidden || void fetchList(), 3000);
    }
    return () => {
      if (--listUsers === 0 && listTimer) {
        clearInterval(listTimer);
        listTimer = null;
      }
    };
  }, []);
  return useSyncExternalStore(
    (f) => (listSubs.add(f), () => listSubs.delete(f)),
    () => list,
  );
}

// -------------------------------------------------------------- pictures --

interface Feed {
  users: number;
  snap: { url: string | null; size: [number, number] | null };
  abort: AbortController;
  subs: Set<() => void>;
}
const feeds = new Map<string, Feed>();

/** stream: every picture (on air); preview: 10 a second, large; tile: 5 a second, small. */
export type FeedKind = 'stream' | 'preview' | 'tile';

/** Stops a feed nothing shows (after a moment: a layer moving in the tree mounts again at once). */
function release(key: string, feed: Feed, delay: number) {
  setTimeout(() => {
    if (feed.users > 0 || feeds.get(key) !== feed) return;
    feed.abort.abort();
    if (feed.snap.url) URL.revokeObjectURL(feed.snap.url);
    feeds.delete(key);
  }, delay);
}

function feedFor(id: string, kind: FeedKind): Feed {
  const key = `${kind}:${id}`;
  let f = feeds.get(key);
  if (!f) {
    f = { users: 0, snap: { url: null, size: null }, abort: new AbortController(), subs: new Set() };
    feeds.set(key, f);
    void (kind === 'stream' ? run(id, f) : poll(id, f, kind === 'tile' ? 480 : 1280, kind === 'tile' ? 200 : 100));
    release(key, f, 5000);
  }
  return f;
}

const sleep = (ms: number) => new Promise((res) => setTimeout(res, ms));

/** Asks for the newest picture every `period` ms (the server answers 204 when there is none newer). */
async function poll(id: string, feed: Feed, width: number, period: number) {
  let after: string | null = null;
  while (!feed.abort.signal.aborted) {
    const t0 = performance.now();
    try {
      const r: Response = await fetch(`/api/inputs/${encodeURIComponent(id)}/snapshot.jpg?w=${width}${after ? `&after=${after}` : ''}`, { credentials: 'same-origin', signal: feed.abort.signal, cache: 'no-store' });
      if (r.status === 200) {
        after = r.headers.get('x-frame');
        show(feed, await r.blob());
      } else if (!r.ok) {
        await sleep(2000);
      }
    } catch {
      if (feed.abort.signal.aborted) return;
      await sleep(1500);
    }
    // A hidden tab asks once a second.
    const wait = (document.hidden ? 1000 : period) - (performance.now() - t0);
    if (wait > 0) await sleep(wait);
  }
}

const enc = new TextEncoder();
const find = (hay: Uint8Array, needle: Uint8Array, from = 0) => {
  outer: for (let i = from; i <= hay.length - needle.length; i++) {
    for (let j = 0; j < needle.length; j++) if (hay[i + j] !== needle[j]) continue outer;
    return i;
  }
  return -1;
};

/** Reads the multipart stream: each JPEG becomes the feed's picture. */
async function run(id: string, feed: Feed) {
  const sep = enc.encode('\r\n\r\n');
  while (!feed.abort.signal.aborted) {
    try {
      const r = await fetch(`/api/inputs/${encodeURIComponent(id)}/live.mjpeg?w=1280`, { credentials: 'same-origin', signal: feed.abort.signal });
      if (!r.ok || !r.body) throw new Error(String(r.status));
      const reader = r.body.getReader();
      let buf = new Uint8Array(0);
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        const next = new Uint8Array(buf.length + value.length);
        next.set(buf);
        next.set(value, buf.length);
        buf = next;
        // --frame\r\nContent-Type: image/jpeg\r\nContent-Length: N\r\n\r\n<N bytes>\r\n
        for (;;) {
          const h = find(buf, sep);
          if (h < 0) break;
          const head = new TextDecoder().decode(buf.subarray(0, h));
          const m = head.match(/Content-Length:\s*(\d+)/i);
          if (!m) {
            buf = buf.subarray(h + 4);
            continue;
          }
          const n = +m[1];
          if (buf.length < h + 4 + n) break;
          const jpeg = buf.slice(h + 4, h + 4 + n);
          buf = buf.subarray(h + 4 + n);
          show(feed, new Blob([jpeg], { type: 'image/jpeg' }));
        }
      }
    } catch {
      if (feed.abort.signal.aborted) return;
    }
    // Dropped: again in a moment.
    await new Promise((res) => setTimeout(res, 1500));
  }
}

function show(feed: Feed, blob: Blob) {
  const url = URL.createObjectURL(blob);
  const old = feed.snap.url;
  const img = new Image();
  img.onload = () => {
    if (feed.abort.signal.aborted) return URL.revokeObjectURL(url);
    feed.snap = { url, size: [img.naturalWidth || 1, img.naturalHeight || 1] };
    feed.subs.forEach((f) => f());
    // The previous picture, once the new one is on screen.
    if (old) setTimeout(() => URL.revokeObjectURL(old), 500);
  };
  img.onerror = () => URL.revokeObjectURL(url);
  img.src = url;
}

// ----------------------------------------------------------- live streams --
// The HTML5 output: every picture of an input, at about the size of its tile, decoded off the main
// thread (createImageBitmap) and played out evenly on a canvas, without React. Each part of the
// server's stream (inputs.rs, mjpeg) says when its picture came: shown that long after, plus a
// short delay that absorbs the irregular arrival over the network.

interface Timed {
  bmp: ImageBitmap;
  /** When to show it (performance.now()). */
  at: number;
}

export interface LiveStream {
  size: [number, number] | null;
  queue: Timed[];
  /** The picture on show. */
  current: ImageBitmap | null;
  users: number;
  abort: AbortController;
  subs: Set<() => void>;
}

const streams = new Map<string, LiveStream>();
const WIDTHS = [320, 480, 640, 960, 1280, 1920, 3840];
/** The width asked of the server for a picture drawn `px` wide (a few sizes only: shared by the tiles alike). */
export const streamWidth = (px: number) => WIDTHS.find((w) => w >= px) ?? 3840;
// ------------------------------------------------------------ the clock --
// One timeline for everything an input brings: its pictures, its sound and its meters each carry
// when they came to the server (ms since 1970); each is played that long after, plus the smallest
// delay seen between the server and here and a playout delay that absorbs the network: so they
// stay together.

/** How long after its time anything is played at least: room for the network's irregularity. */
const PLAYOUT_MS = 80;
/** The smallest delay seen from the server to here (ms, both clocks), rising slowly (the clocks drift apart). */
let lag: number | null = null;
let lagAt = 0;
/** How much later than that things came lately (ms; a source sending its sound in bursts): played as late. */
let late = 0;

/** Something the server timed `serverMs` has just come (`dur`: what it holds lasts that long after). */
export function observe(serverMs: number, dur = 0) {
  const now = performance.timeOrigin + performance.now();
  const seen = now - serverMs - dur;
  const dt = lagAt ? now - lagAt : 0;
  // Rises by 1 ms a second at most: a delay grown for good is followed slowly, a shorter one at once.
  if (lag == null) lag = seen;
  else lag = Math.min(seen, lag + dt / 1000);
  // Later than that: the playout waits as long at once, and comes back 10 ms a second.
  late = Math.max(seen - lag, late - dt / 100, 0);
  lagAt = now;
}

/** How long after its time anything plays now (ms): never under 80 ms, as long as things came late lately. */
export const playout = () => Math.min(1200, Math.max(PLAYOUT_MS, late + 30));

/** When (performance.now()) something timed `serverMs` by the server plays here. */
export function playAt(serverMs: number): number {
  return serverMs + (lag ?? 0) + playout() - performance.timeOrigin;
}

function streamFor(id: string, width: number): LiveStream {
  const key = `${id}@${width}`;
  let s = streams.get(key);
  if (!s) {
    s = { size: null, queue: [], current: null, users: 0, abort: new AbortController(), subs: new Set() };
    streams.set(key, s);
    void runStream(id, width, s);
  }
  return s;
}

async function runStream(id: string, width: number, s: LiveStream) {
  const sep = enc.encode('\r\n\r\n');
  while (!s.abort.signal.aborted) {
    try {
      const r = await fetch(`/api/inputs/${encodeURIComponent(id)}/live.mjpeg?w=${width}`, { credentials: 'same-origin', signal: s.abort.signal });
      if (!r.ok || !r.body) throw new Error(String(r.status));
      const reader = r.body.getReader();
      let buf = new Uint8Array(0);
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        const next = new Uint8Array(buf.length + value.length);
        next.set(buf);
        next.set(value, buf.length);
        buf = next;
        for (;;) {
          const h = find(buf, sep);
          if (h < 0) break;
          const head = new TextDecoder().decode(buf.subarray(0, h));
          const m = head.match(/Content-Length:\s*(\d+)/i);
          if (!m) {
            buf = buf.subarray(h + 4);
            continue;
          }
          const n = +m[1];
          if (buf.length < h + 4 + n) break;
          const jpeg = buf.slice(h + 4, h + 4 + n);
          buf = buf.subarray(h + 4 + n);
          const came = Number(head.match(/X-Time:\s*(\d+)/i)?.[1] ?? 0);
          await take(s, new Blob([jpeg], { type: 'image/jpeg' }), came);
        }
      }
    } catch {
      if (s.abort.signal.aborted) return;
    }
    await sleep(1500);
  }
}

/** Decodes a picture and queues it for its time. */
async function take(s: LiveStream, blob: Blob, came: number) {
  // Seen as it comes, before the decoding (which is not the network's delay).
  if (came > 0) observe(came);
  let bmp: ImageBitmap;
  try {
    bmp = await createImageBitmap(blob);
  } catch {
    return;
  }
  if (s.abort.signal.aborted) return bmp.close();
  let at = performance.now();
  // On the inputs' common timeline (the sound and the meters too).
  if (came > 0) at = playAt(came);
  s.queue.push({ bmp, at });
  // Too far behind (a tab in the background): only the newest ones (the playout holds a second at most).
  while (s.queue.length > 60) s.queue.shift()!.bmp.close();
  if (!s.size || s.size[0] !== bmp.width || s.size[1] !== bmp.height) {
    s.size = [bmp.width, bmp.height];
    s.subs.forEach((f) => f());
  }
  startTicks();
}

/** The canvases drawing the streams, redrawn at the display's rate. */
const painters = new Set<() => void>();
let ticking = 0;

function startTicks() {
  if (!ticking && painters.size) ticking = requestAnimationFrame(tick);
}

function tick() {
  ticking = 0;
  const now = performance.now();
  const done: ImageBitmap[] = [];
  for (const s of streams.values()) {
    let next: Timed | undefined;
    while (s.queue.length && s.queue[0].at <= now) {
      if (next) done.push(next.bmp);
      next = s.queue.shift();
    }
    if (next) {
      if (s.current) done.push(s.current);
      s.current = next.bmp;
    }
  }
  painters.forEach((p) => p());
  done.forEach((b) => b.close());
  if (painters.size) ticking = requestAnimationFrame(tick);
}

/** Every picture of an input at `width` (0 or "": none), played out evenly; its size follows. */
export function useLiveStream(id: string, width: number): LiveStream | null {
  const key = id && width ? `${id}@${width}` : '';
  useEffect(() => {
    if (!key) return;
    const s = streamFor(id, width);
    s.users++;
    return () => {
      if (--s.users > 0) return;
      setTimeout(() => {
        if (s.users > 0 || streams.get(key) !== s) return;
        s.abort.abort();
        s.queue.forEach((f) => f.bmp.close());
        s.current?.close();
        streams.delete(key);
      }, 2000);
    };
  }, [key]);
  useSyncExternalStore(
    (cb) => {
      if (!key) return noop;
      const s = streamFor(id, width);
      s.subs.add(cb);
      return () => s.subs.delete(cb);
    },
    () => (key ? streamFor(id, width).size : null),
  );
  return key ? streamFor(id, width) : null;
}

/** Draws the component again on each frame of the display while `on` (meters falling smoothly). */
export function useEveryFrame(on: boolean) {
  const [, setFrame] = useState(0);
  useEffect(() => (on ? onPaint(() => setFrame((n) => (n + 1) % 1e6)) : undefined), [on]);
}

/** Calls `paint` at the display's rate while the streams play (until the returned function). */
export function onPaint(paint: () => void): () => void {
  painters.add(paint);
  startTicks();
  return () => void painters.delete(paint);
}

const none = { url: null as string | null, size: null as [number, number] | null };
const noop = () => {};

/** The latest picture of an input (none for "") and its size. */
export function useLiveFrame(id: string, kind: FeedKind = 'preview'): { url: string | null; size: [number, number] | null } {
  useEffect(() => {
    if (!id) return;
    const feed = feedFor(id, kind);
    feed.users++;
    return () => {
      if (--feed.users === 0) release(`${kind}:${id}`, feed, 2000);
    };
  }, [id, kind]);
  return useSyncExternalStore(
    (cb) => {
      if (!id) return noop;
      const f = feedFor(id, kind);
      f.subs.add(cb);
      return () => f.subs.delete(cb);
    },
    () => (id ? feedFor(id, kind).snap : none),
  );
}

// -------------------------------------------------------------- signals --
// The tally, UMD, audio levels, loudness, timecode and faults of the inputs, and the state of the
// plant (crates/strata-mv-server/src/signals.rs), pushed by the server a dozen times a second while
// they change.

/** Tally lamps and the switcher's UMD text (mirrors strata_mv_core::signal::Tally). */
export interface Tally {
  program?: boolean;
  preview?: boolean;
  aux?: boolean;
  umd?: string;
}

/** A fault the server sees on an input (mirrors signal::AlarmKind). */
export type AlarmKind =
  | 'loss'
  | 'black'
  | 'freeze'
  | 'silence'
  | 'loud'
  | 'peak'
  | 'channelSilent'
  | 'quiet'
  | 'clipping'
  | 'phase'
  | 'fps'
  | 'reconnecting'
  | 'onBackup'
  | 'noTimecode'
  | 'noCaptions'
  | 'tsPriority1'
  | 'tsPriority2'
  | 'ptpUnlocked'
  | 'ntpUnsynced'
  | 'cpuHigh'
  | 'memoryHigh'
  | 'outputDropped'
  | 'colorSpace'
  | 'illegalLevels'
  | 'fullRange'
  | 'hdrMetadata'
  | 'noColorInfo';
export type Severity = 'warning' | 'critical';
export interface Alarm {
  kind: AlarmKind;
  severity: Severity;
  /** Since when, ms since 1970. */
  since: number;
  /** Someone has seen it (the tile stops flashing). */
  acked?: boolean;
}

/** One input's signals. */
export interface SignalView {
  tally?: Tally;
  /** Per audio channel: [peak, mean power, hold], dBFS. */
  levels?: [number, number, number][];
  /** The time of the sound they measure (ms since 1970). */
  levelsAt?: number;
  /** Every measure since the last message: its time (ms since 1970) and the meters then. */
  levelsSeq?: [number, [number, number, number][]][];
  /** [momentary, short-term, integrated] LUFS, range LU, true peak dBTP. */
  loudness?: [number, number, number, number, number];
  /** Short-term loudness, one a second, oldest first (sent when it changes; kept here). */
  history?: number[];
  /** "HH:MM:SS:FF". */
  timecode?: string;
  /** The worst first. */
  alarms?: Alarm[];
  /** What its picture and sound are (sent when it changes; kept here). */
  video?: VideoInfo;
  /** The phase correlation of its left and right (−1 to 1). */
  correlation?: number;
  /** Lost its signal: the id of the backup its tiles show. */
  backup?: string;
}

/** What an input's picture and sound are (mirrors strata_mv_core::signal::VideoInfo). */
export interface VideoInfo {
  codec: string;
  profile: string;
  size: string;
  pixFmt: string;
  bits: number;
  chroma: string;
  range: string;
  matrix: string;
  primaries: string;
  transfer: string;
  hdr: string;
  dolbyVision: boolean;
  hdr10Plus: boolean;
  mastering: string;
  maxCll: number;
  maxFall: number;
  audio: { codec: string; layout: string; rate: number; language: string; dolby: boolean; atmos: boolean }[];
  /** Lowest and highest luma lately, % (0 black, 100 white). */
  luma?: [number, number] | null;
  /** Pixels out of the broadcast range lately, %. */
  illegal?: number | null;
  /** Active Format Description carried by the pictures. */
  afd?: number;
}

/** A line of the alarm journal (mirrors signal::JournalEntry). */
export interface JournalEntry {
  at: number;
  input: string;
  /** The input's id ("" for the plant). */
  inputId?: string;
  kind: AlarmKind;
  severity: Severity;
  ended?: number | null;
  /** When it was acknowledged, and by whom. */
  acked?: number | null;
  by?: string;
}

/** PTP, NTP and the alarm journal (mirrors signal::SystemView). */
export interface SystemView {
  ptp: { running: boolean; locked: boolean; state: string; grandmaster: string; domain: number; offsetNs?: number | null };
  ntp: { synced?: boolean | null; source: string; offsetMs?: number | null };
  journal: JournalEntry[];
  /** The plant's measures, one a second, the newest last (sent when they change; kept here). */
  curves?: Curves;
  /** The pages sound for a new fault this grave at least (null: silence). */
  sound?: Severity | null;
}

/** Measures of the plant (mirrors signal::Curves): "cpu", "memory", "server", "ffmpeg", "ptp", "ntp", "render:<channel>", "fps:<input>". */
export interface Curves {
  endMs: number;
  series: Record<string, (number | null)[]>;
}

let signals: Record<string, SignalView> = {};
let system: SystemView = { ptp: { running: false, locked: false, state: '', grandmaster: '', domain: 0 }, ntp: { source: '' }, journal: [] };
const signalSubs = new Set<() => void>();

/** From the WebSocket (store.ts): a history not sent again is the one kept. */
/** Signals waiting for their time on the timeline (with the pictures they go with). */
const waiting: { t: number; all: Record<string, SignalView>; sys?: SystemView }[] = [];
let waitTimer: ReturnType<typeof setTimeout> | null = null;

function flushSignals() {
  waitTimer = null;
  const now = performance.now();
  while (waiting.length && waiting[0].t <= now + 2) {
    const w = waiting.shift()!;
    applySignals(w.all, w.sys);
  }
  if (waiting.length) waitTimer = setTimeout(flushSignals, Math.max(0, waiting[0].t - performance.now()));
}

/** From the WebSocket (store.ts), sent at `at` (ms since 1970): shown as late as the pictures of that moment. */
export function setSignals(all: Record<string, SignalView>, sys?: SystemView, at?: number) {
  if (!at) return applySignals(all, sys);
  observe(at);
  // The meters on their sound's own time (with its pictures), the rest when sent.
  for (const [id, v] of Object.entries(all)) {
    if (!v.levels?.length) continue;
    const q = levelQueue.get(id) ?? [];
    // Each measure at its time (they come in bursts), else the last one.
    if (v.levelsSeq?.length) for (const [t, levels] of v.levelsSeq) q.push({ t: playAt(t), levels });
    else q.push({ t: playAt(v.levelsAt ?? at), levels: v.levels });
    while (q.length > 100) q.shift();
    levelQueue.set(id, q);
  }
  // Inputs gone silent: their meters go when the message's time comes.
  for (const id of levelQueue.keys()) if (!all[id]?.levels?.length) levelQueue.get(id)!.push({ t: playAt(at), levels: [] });
  waiting.push({ t: playAt(at), all, sys });
  // A tab long in the background: only the newest ones.
  while (waiting.length > 50) waiting.shift();
  if (!waitTimer) waitTimer = setTimeout(flushSignals, Math.max(0, waiting[0].t - performance.now()));
}

type Levels = [number, number, number][];
/** Each input's meters waiting for their time, and the ones shown. */
const levelQueue = new Map<string, { t: number; levels: Levels }[]>();
const levelShown = new Map<string, Levels>();
/** dB a second a peak falls (mirrors signals.rs FALL). */
const FALL = 24;
const smooth = new Map<string, { at: number; peaks: number[]; out: Levels }>();

/** An input's meters at this moment of the timeline. */
function levelsNow(id: string): Levels {
  const q = levelQueue.get(id);
  const now = performance.now();
  if (q) while (q.length && q[0].t <= now) levelShown.set(id, q.shift()!.levels);
  return levelShown.get(id) ?? signals[id]?.levels ?? [];
}

/**
 * An input's meters drawn at the display's rate: on their sound's time (with the pictures), a peak
 * rising at once and falling smoothly between two measures. The same for every meter of the input
 * in a frame.
 */
export function smoothLevels(id: string): Levels {
  const now = performance.now();
  let s = smooth.get(id);
  if (s && now - s.at < 4) return s.out;
  const lv = levelsNow(id);
  const dt = s ? Math.min(0.25, (now - s.at) / 1000) : 0;
  const peaks = lv.map(([p], i) => {
    const was = s?.peaks[i] ?? -200;
    return p >= was ? p : Math.max(p, was - FALL * dt);
  });
  s = { at: now, peaks, out: lv.map(([, rms, hold], i) => [peaks[i], rms, hold]) };
  smooth.set(id, s);
  return s.out;
}

function applySignals(all: Record<string, SignalView>, sys?: SystemView) {
  const next: Record<string, SignalView> = {};
  for (const [id, v] of Object.entries(all)) {
    const was = signals[id];
    next[id] = { ...v, history: v.history ?? was?.history, video: v.video ?? was?.video };
  }
  // What an input is stays known while it says nothing else.
  for (const [id, v] of Object.entries(signals)) if (!next[id] && v.video) next[id] = { video: v.video };
  signals = next;
  // The curves come once a second: those of before stay until then.
  if (sys) system = sys.curves?.endMs ? sys : { ...sys, curves: system.curves };
  signalSubs.forEach((f) => f());
}

const NONE: SignalView = {};

const subscribe = (f: () => void) => {
  signalSubs.add(f);
  return () => void signalSubs.delete(f);
};

/** The signals of an input, redrawn when they change. */
export function useSignal(id: string): SignalView {
  return useSyncExternalStore(subscribe, () => (id ? (signals[id] ?? NONE) : NONE));
}

/** PTP, NTP and the alarm journal, redrawn when they change. */
export function useSystem(): SystemView {
  return useSyncExternalStore(subscribe, () => system);
}

/** The words of a fault, in the project's language (mirrors AlarmKind::text). */
export function alarmText(kind: AlarmKind, lang = 'fr'): string {
  const fr = lang.toLowerCase().startsWith('fr');
  const words: Record<AlarmKind, [string, string]> = {
    loss: ['PERTE DU SIGNAL', 'SIGNAL LOSS'],
    black: ['NOIR', 'BLACK'],
    freeze: ['IMAGE FIGÉE', 'FROZEN PICTURE'],
    silence: ['SILENCE', 'SILENCE'],
    loud: ['TROP FORT', 'TOO LOUD'],
    peak: ['TRUE PEAK', 'TRUE PEAK'],
    channelSilent: ['CANAL MUET', 'CHANNEL SILENT'],
    quiet: ['TROP FAIBLE', 'TOO QUIET'],
    clipping: ['SATURATION', 'CLIPPING'],
    phase: ['OPPOSITION DE PHASE', 'OUT OF PHASE'],
    fps: ['CADENCE ANORMALE', 'ABNORMAL FRAME RATE'],
    reconnecting: ['RECONNEXIONS', 'RECONNECTING'],
    onBackup: ['SUR SECOURS', 'ON BACKUP'],
    noTimecode: ['SANS TIMECODE', 'NO TIMECODE'],
    noCaptions: ['SANS SOUS-TITRES', 'NO CAPTIONS'],
    tsPriority1: ['TR 101 290 P1', 'TR 101 290 P1'],
    tsPriority2: ['TR 101 290 P2', 'TR 101 290 P2'],
    ptpUnlocked: ['PTP NON VERROUILLÉ', 'PTP UNLOCKED'],
    ntpUnsynced: ['NTP NON SYNCHRONISÉ', 'NTP NOT SYNCED'],
    cpuHigh: ['PROCESSEURS SATURÉS', 'PROCESSORS BUSY'],
    memoryHigh: ['MÉMOIRE PLEINE', 'MEMORY FULL'],
    outputDropped: ['IMAGES PERDUES EN SORTIE', 'OUTPUT DROPS FRAMES'],
    colorSpace: ['ESPACE COULEUR', 'COLOUR SPACE'],
    illegalLevels: ['NIVEAUX HORS NORMES', 'ILLEGAL LEVELS'],
    fullRange: ['PLAGE COMPLÈTE', 'FULL RANGE'],
    hdrMetadata: ['MÉTADONNÉES HDR', 'HDR METADATA'],
    noColorInfo: ['COLORIMÉTRIE NON SIGNALÉE', 'NO COLOUR INFO'],
  };
  return words[kind][fr ? 0 : 1];
}

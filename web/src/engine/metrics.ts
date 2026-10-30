// Load of the server (crates/cg-server/src/perf.rs): the machine, each
// channel's pictures and outputs, the live inputs, what more the machine can
// take, and what goes wrong. One poll per page, shared by every meter.
import { useEffect, useSyncExternalStore } from 'react';

export interface Metrics {
  /** Server clock, ms. */
  at: number;
  machine: {
    /** Whole machine, % of all cores. */
    cpu: number;
    cores: number;
    cpuName: string;
    memUsed: number;
    memTotal: number;
    hostname: string;
  };
  /** Strata itself, and the ffmpeg it runs (streams in and out). */
  process: { cpu: number; mem: number; ffmpegCpu: number; ffmpeg: number };
  channels: ChannelMetrics[];
  inputs: InputMetrics[];
  capacity: Capacity;
  warnings: Warning[];
}

export interface ChannelMetrics {
  id: string;
  name: string;
  projectId: string | null;
  /** Graphics on air. */
  onAir: number;
  /** Channel frame rate. */
  rate: number;
  /** How late the clock ticks, ms (largest in the last 10 s). */
  jitterMs: number;
  /** Highest load of its pictures, % of the frame time. */
  load: number;
  pictures: PictureMetrics[];
}

export interface PictureMetrics {
  id: string;
  name: string;
  width: number;
  height: number;
  /** Frames rendered per second (the channel rate when all is well). */
  fps: number;
  renderMs: number;
  renderMaxMs: number;
  /** Render time, % of the frame time. */
  load: number;
  /** Frames sent again because the next one was not ready, last 10 s. */
  repeated: number;
  outputs: OutputMetrics[];
}

export interface OutputMetrics {
  id: string;
  /** ndi | st2110 | sdi | stream */
  kind: string;
  name: string;
  fps: number;
  /** Frames the output could not take, last 10 s. */
  dropped: number;
  ok: boolean;
  message: string;
  /** Cores it uses (its ffmpeg, or the time spent sending). */
  cpu: number;
}

export interface InputMetrics {
  id: string;
  name: string;
  kind: string;
  live: boolean;
  /** Pictures received per second. */
  fps: number;
  /** The rate it is decoded at. */
  nominal: number;
  /** Pictures thrown away (too many waiting), last 10 s. */
  dropped: number;
  /** Smoothing buffer, ms. */
  bufferMs: number;
  /** Cores its decoding uses. */
  cpu: number;
}

export interface Capacity {
  /** Cores Strata should use at most (80 % of the machine). */
  budget: number;
  /** Cores in use now (whole machine). */
  used: number;
  free: number;
  items: CapacityItem[];
}

export interface CapacityItem {
  /** picture | stream | ndi | st2110 | sdi | inputStream | inputNdi */
  kind: string;
  /** What one of them is, e.g. "1920×1080 50p". */
  format: string;
  /** Cores one of them uses. */
  unit: number;
  /** Measured on this server (else a typical figure). */
  measured: boolean;
  /** How many more fit now. */
  more: number;
  /** How many this machine takes on its own. */
  max: number;
  /** How many run now. */
  now: number;
}

export interface Warning {
  level: 'info' | 'warn' | 'error';
  /** English text with {vars}, translated with t(). */
  text: string;
  vars: Record<string, string | number>;
}

let current: Metrics | null = null;
let timer: ReturnType<typeof setInterval> | null = null;
let timerEvery = 0;
/** How often each user wants them (ms): polled at the shortest. */
const wanted: number[] = [];
const subs = new Set<() => void>();

/** Polls at the shortest interval asked, not while the page is hidden. */
function schedule() {
  const every = wanted.length ? Math.min(...wanted) : 0;
  if (every === timerEvery) return;
  if (timer) clearInterval(timer);
  timer = null;
  timerEvery = every;
  if (!every) return;
  // Not while the page is hidden: nothing shows them.
  timer = setInterval(() => document.hidden || void poll(), every);
}

if (typeof document !== 'undefined') {
  // Shown again: fresh figures at once.
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden && wanted.length) void poll();
  });
}

async function poll() {
  try {
    const r = await fetch('/api/system/metrics', { credentials: 'same-origin' });
    if (!r.ok) return;
    current = (await r.json()) as Metrics;
    subs.forEach((f) => f());
  } catch {
    /* offline */
  }
}

/** The latest metrics, every `every` ms while something shows them (3 s for
 * the status in the top bar; the Performance page asks every second). */
export function useMetrics(every = 3000): Metrics | null {
  useEffect(() => {
    if (!wanted.length) void poll();
    wanted.push(every);
    schedule();
    return () => {
      wanted.splice(wanted.indexOf(every), 1);
      schedule();
    };
  }, [every]);
  return useSyncExternalStore(
    (f) => (subs.add(f), () => subs.delete(f)),
    () => current,
  );
}

/** green under 60 %, amber under 85 %, red above. */
export const loadTone = (pct: number) => (pct >= 85 ? 'bad' : pct >= 60 ? 'warn' : 'ok');

export interface LogEntry {
  seq: number;
  /** ms */
  at: number;
  level: 'ERROR' | 'WARN' | 'INFO' | 'DEBUG' | 'TRACE';
  target: string;
  message: string;
  /** Other fields, "key=value key=value". */
  fields: string;
}

/** `GET /api/system/logs?after=<seq>&limit=<n>&level=<ERROR|WARN|INFO|DEBUG>` (admins). */
export interface LogsResponse {
  /** Oldest first, only those with seq > after. */
  entries: LogEntry[];
  /** The last seq the server has. */
  last: number;
  /** The log file on the server, if it writes one. */
  file: string | null;
}

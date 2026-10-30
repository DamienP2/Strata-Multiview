// Live video inputs (crates/cg-server/src/inputs.rs) for the Video widget:
// their list and status, and their pictures, shared by everything on the
// page showing the same input, and stopped when nothing shows it any more.
// - HTML5 outputs (and the Operator's program) read a Motion JPEG stream:
//   every picture, smoothly;
// - the Designer and the Video streams page ask for the newest picture a
//   few times a second: short requests that get through any proxy and never
//   hold one of the few connections a browser opens to a server.
import { useEffect, useSyncExternalStore } from 'react';
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
  switch (m) {
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

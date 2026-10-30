import { useEffect, useRef, useState } from 'react';
import { clientNow } from '../store';

/*
 * One animation-frame loop for the whole page. What moves subscribes to it
 * while it moves (a ticker on air, a timeline playing…): with nothing
 * subscribed, or the page hidden, no frame is requested at all. Every
 * subscriber of a frame is called from the same callback, so React draws
 * them in one pass. A subscriber can ask for fewer frames (a monitor at
 * 30 fps): it is then called on every other frame or so, on the frames' own
 * time stamps.
 */

interface Sub {
  fn: (now: number) => void;
  /** Least time between two calls (ms); 0: every frame. */
  every: number;
  last: number;
}

const subs = new Set<Sub>();
let raf = 0;

const hidden = () => typeof document !== 'undefined' && document.visibilityState === 'hidden';

function frame(t: number) {
  raf = 0;
  if (hidden()) return;
  // On the clock the frames are stamped with: what moves steps evenly.
  const now = performance.timeOrigin + t;
  for (const s of [...subs]) {
    // A quarter of a 60 Hz frame of slack: a 30 fps subscriber takes every other frame, whatever the jitter.
    if (s.every && t - s.last < s.every - 4) continue;
    s.last = t;
    s.fn(now);
  }
  schedule();
}

function schedule() {
  if (!raf && subs.size && !hidden()) raf = requestAnimationFrame(frame);
}

if (typeof document !== 'undefined')
  document.addEventListener('visibilitychange', () => {
    if (hidden()) {
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
    } else schedule();
  });

/** Calls `fn` with the frame's time (client clock, ms) on each animation frame, at most `maxFps` times a second. */
export function onFrames(fn: (now: number) => void, maxFps?: number): () => void {
  const s: Sub = { fn, every: maxFps && maxFps > 0 ? 1000 / maxFps : 0, last: -Infinity };
  subs.add(s);
  schedule();
  return () => {
    subs.delete(s);
    if (!subs.size && raf) {
      cancelAnimationFrame(raf);
      raf = 0;
    }
  };
}

/**
 * Client clock that ticks every animation frame while `active` (at most
 * `maxFps` a second): the frame's own time stamp (not when its callback
 * happens to run), so what moves by the clock steps evenly from frame to frame.
 */
export function useFrameNow(active: boolean, maxFps?: number): number {
  const [, setNow] = useState(0);
  // The last frame's time while active (none before the first frame: now).
  const last = useRef<number | null>(null);
  useEffect(() => {
    if (!active) return;
    const stop = onFrames((t) => {
      last.current = t;
      setNow(t);
    }, maxFps);
    return () => {
      stop();
      last.current = null;
    };
  }, [active, maxFps]);
  return active && last.current !== null ? last.current : clientNow();
}

// Listening to an input: its sound as the server sends it (inputs.rs `audio`: chunks of 16-bit
// stereo PCM at 48 kHz, each with when its first sample came), played through Web Audio on the
// inputs' common timeline (engine/liveInputs): with its pictures and its meters. One input at a
// time in a page.
import { useSyncExternalStore } from 'react';
import { observe, playAt } from './liveInputs';

const RATE = 48000;
/** Played on from the last chunk when its time is this close (s): the sound stays continuous. */
const SLACK = 0.03;

let ctx: AudioContext | null = null;
let gain: GainNode | null = null;
let current: { id: string; abort: AbortController } | null = null;
let volume = 1;
const subs = new Set<() => void>();
const changed = () => subs.forEach((f) => f());

const sleep = (ms: number) => new Promise((res) => setTimeout(res, ms));

function audio(): { ctx: AudioContext; gain: GainNode } {
  if (!ctx) {
    ctx = new AudioContext({ sampleRate: RATE, latencyHint: 'interactive' });
    gain = ctx.createGain();
    gain.gain.value = volume;
    gain.connect(ctx.destination);
  }
  // Started by a click: allowed to sound.
  if (ctx.state === 'suspended') void ctx.resume();
  return { ctx, gain: gain! };
}

async function play(id: string, abort: AbortController) {
  const { ctx, gain } = audio();
  while (!abort.signal.aborted) {
    try {
      const r = await fetch(`/api/inputs/${encodeURIComponent(id)}/audio`, { credentials: 'same-origin', signal: abort.signal, cache: 'no-store' });
      if (!r.ok || !r.body) throw new Error(String(r.status));
      const reader = r.body.getReader();
      let rest = new Uint8Array(0);
      let next = 0;
      for (;;) {
        const { value, done } = await reader.read();
        if (done || abort.signal.aborted) break;
        const joined = new Uint8Array(rest.length + value.length);
        joined.set(rest);
        joined.set(value, rest.length);
        let off = 0;
        // Each chunk: its length (u32), when its first sample came (f64, ms since 1970), its samples.
        while (joined.length - off >= 12) {
          const head = new DataView(joined.buffer, joined.byteOffset + off, 12);
          const len = head.getUint32(0, true);
          const at = head.getFloat64(4, true);
          if (joined.length - off < 12 + len) break;
          const frames = Math.floor(len / 4);
          const view = new DataView(joined.buffer, joined.byteOffset + off + 12, frames * 4);
          off += 12 + len;
          if (!frames) continue;
          observe(at, (frames / RATE) * 1000);
          const buf = ctx.createBuffer(2, frames, RATE);
          const l = buf.getChannelData(0);
          const rr = buf.getChannelData(1);
          for (let i = 0; i < frames; i++) {
            l[i] = view.getInt16(i * 4, true) / 32768;
            rr[i] = view.getInt16(i * 4 + 2, true) / 32768;
          }
          // When it plays: on the timeline, heard when it comes out of the speakers.
          const latency = ctx.outputLatency || ctx.baseLatency || 0;
          const when = ctx.currentTime + (playAt(at) - performance.now()) / 1000 - latency;
          // Too late: dropped. Close to where the last one ends: right after it (no click).
          if (when + buf.duration < ctx.currentTime) continue;
          if (Math.abs(when - next) > SLACK) next = Math.max(when, ctx.currentTime);
          const src = ctx.createBufferSource();
          src.buffer = buf;
          src.connect(gain);
          src.start(next);
          next += buf.duration;
        }
        rest = joined.slice(off);
      }
    } catch {
      if (abort.signal.aborted) return;
    }
    // The input restarted or has no sound yet: again in a moment.
    await sleep(1000);
  }
}

/** Listens to an input (null: silence). */
export function listen(id: string | null) {
  if ((current?.id ?? null) === id) return;
  current?.abort.abort();
  current = null;
  if (id) {
    const abort = new AbortController();
    current = { id, abort };
    void play(id, abort);
  }
  changed();
}

/** The volume, 0–1. */
export function setVolume(v: number) {
  volume = Math.min(1, Math.max(0, v));
  if (gain) gain.gain.value = volume;
  changed();
}

/** The input listened to, and the volume. */
export function useListening(): { id: string | null; volume: number } {
  const id = useSyncExternalStore(
    (f) => (subs.add(f), () => void subs.delete(f)),
    () => current?.id ?? null,
  );
  const v = useSyncExternalStore(
    (f) => (subs.add(f), () => void subs.delete(f)),
    () => volume,
  );
  return { id, volume: v };
}

// Listening to an input: its sound as the server sends it (inputs.rs `audio`: 16-bit stereo PCM at
// 48 kHz, endless), played through Web Audio with a short buffer that absorbs the network. One input
// at a time in a page.
import { useSyncExternalStore } from 'react';

const RATE = 48000;
/** How far ahead of the clock the sound is played (absorbs the network's irregularity). */
const AHEAD = 0.15;

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
        const bytes = new Uint8Array(rest.length + value.length);
        bytes.set(rest);
        bytes.set(value, rest.length);
        const frames = Math.floor(bytes.length / 4);
        rest = bytes.slice(frames * 4);
        if (!frames) continue;
        const buf = ctx.createBuffer(2, frames, RATE);
        const l = buf.getChannelData(0);
        const rr = buf.getChannelData(1);
        const view = new DataView(bytes.buffer, bytes.byteOffset, frames * 4);
        for (let i = 0; i < frames; i++) {
          l[i] = view.getInt16(i * 4, true) / 32768;
          rr[i] = view.getInt16(i * 4 + 2, true) / 32768;
        }
        // Too late (a pause) or too far ahead (a burst): from now again.
        const now = ctx.currentTime;
        if (next < now + 0.02 || next > now + 1) next = now + AHEAD;
        const src = ctx.createBufferSource();
        src.buffer = buf;
        src.connect(gain);
        src.start(next);
        next += buf.duration;
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

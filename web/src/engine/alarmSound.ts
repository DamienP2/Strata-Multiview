// A sound on the page when a fault is raised (Alarms › Actions › Sound): two tones, a critical fault
// three times. Faults already on when the page opened, or acknowledged, do not sound.
import { useEffect, useRef } from 'react';
import { useSystem, type JournalEntry } from './liveInputs';

let ctx: AudioContext | null = null;

function beep(times: number) {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === 'suspended') void ctx.resume();
    const t0 = ctx.currentTime + 0.02;
    for (let i = 0; i < times; i++) {
      for (const [k, f] of [
        [0, 880],
        [1, 660],
      ] as const) {
        const at = t0 + i * 0.5 + k * 0.18;
        const o = ctx.createOscillator();
        const g = ctx.createGain();
        o.frequency.value = f;
        o.type = 'sine';
        g.gain.setValueAtTime(0, at);
        g.gain.linearRampToValueAtTime(0.25, at + 0.01);
        g.gain.exponentialRampToValueAtTime(0.0001, at + 0.16);
        o.connect(g).connect(ctx.destination);
        o.start(at);
        o.stop(at + 0.17);
      }
    }
  } catch {
    // No sound here (no audio device, not allowed yet).
  }
}

const key = (e: JournalEntry) => `${e.inputId ?? e.input}|${e.kind}|${e.at}`;

/** Sounds for the new faults as the settings say. */
export function useAlarmSound(on = true) {
  const sys = useSystem();
  const known = useRef<Set<string> | null>(null);
  useEffect(() => {
    const open = sys.journal.filter((e) => e.ended == null);
    // The first look: what is on already is known.
    if (!known.current) {
      if (sys.journal.length || sys.ptp.state || sys.curves) known.current = new Set(open.map(key));
      return;
    }
    const fresh = open.filter((e) => !known.current!.has(key(e)));
    for (const e of open) known.current.add(key(e));
    if (!on || !sys.sound) return;
    const loud = fresh.filter((e) => !e.acked && (sys.sound === 'warning' || e.severity === 'critical'));
    if (loud.length) beep(loud.some((e) => e.severity === 'critical') ? 3 : 1);
  }, [sys, on]);
}

import { useEffect, useState } from 'react';
import { clockStats } from '../store';

/*
 * `?debug` on an HTML5 output: what makes motion uneven, measured where the
 * page runs (vMix, OBS, a browser). Pictures per second and their spacing
 * (the browser source's rate, a page too slow to draw), and the server's
 * messages (the network).
 */

interface Stats {
  fps: number;
  avg: number;
  max: number;
  late: number;
  long: number;
  msgs: number;
  spread: number;
  offset: number;
  steps: number;
}

const pct = (n: number, d: number) => (d ? Math.round((n / d) * 100) : 0);

export function OutputDebug({ fps: target }: { fps: number }) {
  const [s, setS] = useState<Stats | null>(null);
  useEffect(() => {
    let raf = 0;
    let frames: number[] = [];
    // Work long enough to make the page miss pictures (over 50 ms).
    let long: number[] = [];
    let watch: PerformanceObserver | null = null;
    try {
      watch = new PerformanceObserver((l) => l.getEntries().forEach((e) => long.push(e.startTime)));
      watch.observe({ type: 'longtask', buffered: false });
    } catch {
      /* not measured by this browser */
    }
    const tick = (t: number) => {
      frames.push(t);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    const every = setInterval(() => {
      const now = performance.now();
      frames = frames.filter((t) => now - t < 2000);
      long = long.filter((t) => now - t < 2000);
      const gaps = frames.slice(1).map((t, i) => t - frames[i]);
      const avg = gaps.length ? gaps.reduce((a, b) => a + b, 0) / gaps.length : 0;
      const wall = performance.timeOrigin + now;
      const recent = clockStats.arrivals.filter((a) => wall - a.at < 2000);
      const delays = recent.map((a) => a.delay);
      setS({
        fps: gaps.length / 2,
        avg,
        max: gaps.length ? Math.max(...gaps) : 0,
        late: pct(gaps.filter((g) => g > avg * 1.5).length, gaps.length),
        long: long.length,
        msgs: recent.length / 2,
        spread: delays.length ? Math.max(...delays) - Math.min(...delays) : 0,
        offset: clockStats.offset,
        steps: clockStats.steps,
      });
    }, 500);
    return () => {
      watch?.disconnect();
      cancelAnimationFrame(raf);
      clearInterval(every);
    };
  }, []);
  if (!s) return null;
  const ok = (good: boolean) => ({ color: good ? '#4ade80' : '#f87171' });
  // The page's rate against the channel's: equal (or double) moves evenly; any other ratio drops or repeats pictures.
  const ratio = s.fps / target;
  const sameRate = Math.abs(ratio - Math.round(ratio)) < 0.05 && Math.round(ratio) >= 1;
  return (
    <div style={{ position: 'fixed', left: 12, top: 12, zIndex: 9, padding: '10px 14px', borderRadius: 8, background: 'rgba(0,0,0,.78)', color: '#fff', font: '600 15px/1.55 ui-monospace, Menlo, monospace', whiteSpace: 'pre' }}>
      <div style={{ color: sameRate ? '#4ade80' : '#fb923c' }}>{`Images / s     ${s.fps.toFixed(1)}  ${sameRate ? '=' : '≠'} canal ${target.toFixed(2).replace(/\.00$/, '')}`}</div>
      <div style={ok(s.late < 3)}>{`Écart images   ${s.avg.toFixed(1)} ms moy · ${s.max.toFixed(0)} ms max · ${s.late} % en retard`}</div>
      <div style={ok(s.long === 0)}>{`Calcul lent    ${s.long} fois (plus de 50 ms)`}</div>
      <div style={ok(s.spread < 40)}>{`Réseau         ${s.msgs.toFixed(0)} msg/s · délai variable ${s.spread.toFixed(0)} ms`}</div>
      <div style={ok(s.steps === 0)}>{`Horloge        ${s.offset.toFixed(0)} ms · ${s.steps} recalage(s)`}</div>
      <div style={{ color: '#9ca3af' }}>{`Fenêtre        ${innerWidth}×${innerHeight} @${devicePixelRatio}x`}</div>
    </div>
  );
}

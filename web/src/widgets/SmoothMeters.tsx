// The meters of a Source tile drawn at the display's rate, on their sound's time (with the
// pictures): a peak rises at once and falls smoothly between two measures (engine/liveInputs
// smoothLevels), instead of by steps. Drawn without React: the bars move on each frame.
import { useEffect, useRef } from 'react';
import { onPaint, smoothLevels } from '../engine/liveInputs';
import { scale, METER, type Vu } from './source';

type Area = [number, number, number, number];
/** The track of a meter (mirrors source.rs TRACK). */
const TRACK = '#000000a6';

/** The meters of input `id` in their `areas`, the channels in order. */
export function SmoothMeters({ id, areas, vu }: { id: string; areas: Area[]; vu: Vu }) {
  const bars = useRef<(HTMLDivElement | null)[]>([]);
  const holds = useRef<(HTMLDivElement | null)[]>([]);
  // The tile's layout, read on each frame (a new layout keeps the peaks shown).
  const geo = useRef({ areas, vu });
  geo.current = { areas, vu };
  useEffect(() => {
    return onPaint(() => {
      const { areas, vu } = geo.current;
      const levels = smoothLevels(id).slice(Math.max(1, vu.first ?? 1) - 1);
      const y = scale(vu, vu.yellow);
      const r = scale(vu, Math.max(vu.red, vu.yellow));
      areas.forEach((a, i) => {
        const lv = levels[i];
        const k = scale(vu, lv ? lv[0] : -200);
        // Three segments (green, yellow, red), each up to the peak.
        [
          [0, y],
          [y, r],
          [r, 1],
        ].forEach(([from, to], z) => {
          const el = bars.current[i * 3 + z];
          if (!el) return;
          const top = Math.min(k, to);
          if (top > from) {
            el.style.display = '';
            el.style.top = `${a[3] * (1 - top)}px`;
            el.style.height = `${a[3] * (top - from)}px`;
          } else el.style.display = 'none';
        });
        const h = holds.current[i];
        if (h) {
          const held = lv ? lv[2] : -200;
          const hk = scale(vu, held);
          const th = Math.min(3, Math.max(1, a[3] * 0.012));
          if (vu.hold && lv && held > vu.min) {
            h.style.display = '';
            h.style.top = `${Math.min(a[3] * (1 - hk), a[3] - th)}px`;
            h.style.height = `${th}px`;
            h.style.background = METER[held >= vu.red ? 2 : held >= vu.yellow ? 1 : 0];
          } else h.style.display = 'none';
        }
      });
    });
  }, [id]);
  return (
    <>
      {areas.map((a, i) => (
        <div key={i} style={{ position: 'absolute', left: a[0], top: a[1], width: a[2], height: a[3], background: TRACK, overflow: 'hidden' }}>
          {[0, 1, 2].map((z) => (
            <div key={z} ref={(el) => void (bars.current[i * 3 + z] = el)} style={{ position: 'absolute', left: 0, width: a[2], display: 'none', background: METER[z] }} />
          ))}
          <div ref={(el) => void (holds.current[i] = el)} style={{ position: 'absolute', left: 0, width: a[2], display: 'none' }} />
        </div>
      ))}
    </>
  );
}

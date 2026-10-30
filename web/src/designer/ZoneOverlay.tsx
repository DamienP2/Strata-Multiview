import { memo, useEffect, useRef, useState } from 'react';
import { zoneArea } from '../engine/zones';
import type { Area, Zone } from '../engine/types';
import { useApp } from '../store';
import { dragArea, useAreaSnap, type Handle } from './Guides';
import { t } from '../i18n';

const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];
const MIN = 0.02;

/**
 * Screen zones drawn over the stage, in the format being edited. The zone
 * of the open template is highlighted; drag a zone by its name tab, resize
 * it with its handles. Layers stay clickable through the zones.
 */
// Memoised: not drawn again on each frame of the playback (its props stay).
export const ZoneOverlay = memo(function ZoneOverlay({ zones, format, width, height, scale, current, only }: { zones: Zone[]; format: string; width: number; height: number; scale: number; current: string; /** Show only this zone (all are kept when saving). */ only?: string }) {
  const saveZones = useApp((s) => s.saveZones);
  const [active, setActive] = useState<string | null>(null);
  const [draft, setDraft] = useState<{ id: string; area: Area } | null>(null);
  const draftRef = useRef<{ id: string; area: Area } | null>(null);
  const drag = useRef<{ id: string; mode: Handle; x: number; y: number; start: Area } | null>(null);
  // Edges on the guides and areas (Alt: free).
  const snap = useAreaSnap();
  const snapRef = useRef(snap);
  snapRef.current = snap;
  const portrait = height > width;
  const W = width * scale;
  const H = height * scale;

  useEffect(() => {
    const move = (e: PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      const area = dragArea(d.start, d.mode, (e.clientX - d.x) / W, (e.clientY - d.y) / H, {
        step: e.shiftKey ? 0.01 : 0.001,
        min: MIN,
        snap: e.altKey ? null : snapRef.current,
        near: { x: 6 / W, y: 6 / H },
      });
      draftRef.current = { id: d.id, area };
      setDraft(draftRef.current);
    };
    const up = () => {
      const d = drag.current;
      drag.current = null;
      const cur = draftRef.current;
      draftRef.current = null;
      if (d && cur && cur.id === d.id) {
        const next = zones.map((z) => (z.id === d.id ? { ...z, areas: { ...(z.areas ?? {}), [format]: round(cur.area) } } : z));
        void saveZones(next);
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
  }, [W, H, zones, format, saveZones]);

  // The saved project replaces the draft.
  useEffect(() => setDraft(null), [zones]);

  const begin = (e: React.PointerEvent, z: Zone, mode: Handle, area: Area) => {
    e.stopPropagation();
    e.preventDefault();
    setActive(z.id);
    drag.current = { id: z.id, mode, x: e.clientX, y: e.clientY, start: area };
  };

  return (
    <div className="zone-overlay" onPointerDown={() => setActive(null)}>
      {[...zones]
        .filter((z) => !only || z.id === only)
        .sort((a, b) => a.layer - b.layer)
        .map((z) => {
          const area = draft?.id === z.id ? draft.area : zoneArea(z, format, portrait);
          const mine = z.id === current;
          const whole = area.width >= 0.999 && area.height >= 0.999;
          // A full-screen zone would only frame the picture: show it for its own templates.
          if (whole && !mine && active !== z.id) return null;
          const editing = active === z.id || mine;
          return (
            <div key={z.id} className={`zone-box ${mine ? 'mine' : ''} ${active === z.id ? 'active' : ''}`} style={{ left: area.x * W, top: area.y * H, width: area.width * W, height: area.height * H, ['--zc' as string]: z.color }}>
              {mine && !whole && <span className="zone-veil" />}
              <span className="zone-tab" title={t('Drag to move the zone (Shift: 1% steps)')} onPointerDown={(e) => begin(e, z, 'move', area)}>
                {z.name}
                {draft?.id === z.id && <em>{`${Math.round(area.width * width)}×${Math.round(area.height * height)}`}</em>}
              </span>
              {editing && HANDLES.map((h) => <span key={h} className={`zone-handle h-${h}`} onPointerDown={(e) => begin(e, z, h, area)} />)}
            </div>
          );
        })}
    </div>
  );
});

function round(a: Area): Area {
  const r = (v: number) => Math.round(v * 10000) / 10000;
  return { x: r(a.x), y: r(a.y), width: r(a.width), height: r(a.height) };
}

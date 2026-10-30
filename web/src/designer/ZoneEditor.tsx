import { create } from 'zustand';
import { HexColorField } from '../ui/fields';
import { GripVertical, Plus, Trash2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { designTemplates } from '../engine/drafts';
import { zoneArea } from '../engine/zones';
import type { Area, Project, Zone, ZoneTransition } from '../engine/types';
import { useApp } from '../store';
import { dragArea, useAreaSnap, type Handle } from './Guides';
import { t, tn } from '../i18n';

/*
 * The zones edited on the stage: every zone of the project shown at once,
 * moved and resized there; their name, colour, place in the stacking order
 * (drag in the list) and the way a graphic replaces another set in the
 * right panel. Saved with « Done ».
 */

const COLORS = ['#8fa3ff', '#ff3b5c', '#22c55e', '#f5a524', '#38bdf8', '#e879f9', '#14b8a6', '#f97316'];
const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];
const MIN = 0.02;

interface ZoneEdit {
  on: boolean;
  /** Front-most first. */
  zones: Zone[];
  selected: string | null;
  start: (p: Project, select?: string) => void;
  stop: () => void;
  select: (id: string | null) => void;
  edit: (id: string, patch: Partial<Zone>) => void;
  /** Moves a zone to another place in the list (index in the list without it). */
  move: (id: string, to: number) => void;
  add: () => void;
  remove: (id: string) => void;
}

export const useZoneEdit = create<ZoneEdit>((set, get) => ({
  on: false,
  zones: [],
  selected: null,
  start: (p, select) => set({ on: true, zones: [...(p.zones ?? [])].sort((a, b) => b.layer - a.layer), selected: select ?? null }),
  stop: () => set({ on: false, zones: [], selected: null }),
  select: (selected) => set({ selected }),
  edit: (id, patch) => set({ zones: get().zones.map((z) => (z.id === id ? { ...z, ...patch } : z)) }),
  move: (id, to) => {
    const zones = get().zones;
    const z = zones.find((x) => x.id === id);
    if (!z) return;
    const rest = zones.filter((x) => x.id !== id);
    rest.splice(Math.max(0, Math.min(rest.length, to)), 0, z);
    set({ zones: rest });
  },
  add: () => {
    const zones = get().zones;
    let n = zones.length + 1;
    while (zones.some((z) => z.id === `zone-${n}`)) n++;
    const z: Zone = { id: `zone-${n}`, name: t('Zone {n}', { n }), layer: 0, color: COLORS[zones.length % COLORS.length], transition: 'sequential', areas: {} };
    set({ zones: [z, ...zones], selected: z.id });
  },
  remove: (id) => set({ zones: get().zones.filter((z) => z.id !== id), selected: get().selected === id ? null : get().selected }),
}));

/** The playout layers follow the list: the front-most highest (bottom 10, then 20…). */
const layerAt = (zones: Zone[], i: number) => (zones.length - i) * 10;

/** Templates and rundown items in a zone: it cannot be removed while it has any. */
function usesOf(project: Project, id: string) {
  return designTemplates(project).filter((t) => t.zone === id).length + project.rundown.filter((r) => r.zone === id).length;
}

/** Every zone on the stage, in the format being edited: drag to move, handles to resize. */
export function ZonesEditOverlay({ format, width, height, scale }: { format: string; width: number; height: number; scale: number }) {
  const { zones, selected, select, edit } = useZoneEdit();
  const drag = useRef<{ id: string; mode: Handle; x: number; y: number; start: Area } | null>(null);
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
      const z = useZoneEdit.getState().zones.find((x) => x.id === d.id);
      if (z) edit(d.id, { areas: { ...(z.areas ?? {}), [format]: round(area) } });
    };
    const up = () => (drag.current = null);
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
  }, [W, H, format, edit]);

  const begin = (e: React.PointerEvent, z: Zone, mode: Handle, area: Area) => {
    e.stopPropagation();
    e.preventDefault();
    select(z.id);
    drag.current = { id: z.id, mode, x: e.clientX, y: e.clientY, start: area };
  };

  return (
    <div className="zone-edit" onPointerDown={(e) => (e.stopPropagation(), select(null))}>
      {zones
        .map((z, i) => ({ z, i }))
        .reverse()
        .map(({ z, i }) => {
          const area = zoneArea(z, format, portrait);
          const on = z.id === selected;
          const top = area.y * H < 18;
          return (
            <div
              key={z.id}
              className={`ze-box ${on ? 'on' : ''} ${top ? 'inside' : ''}`}
              style={{ left: area.x * W, top: area.y * H, width: area.width * W, height: area.height * H, ['--zc' as string]: z.color }}
              onPointerDown={(e) => begin(e, z, 'move', area)}
            >
              <span className="ze-tab">
                {z.name || '—'} <em>L{layerAt(zones, i)}</em>
              </span>
              {on && HANDLES.map((h) => <span key={h} className={`zone-handle h-${h}`} onPointerDown={(e) => begin(e, z, h, area)} />)}
            </div>
          );
        })}
    </div>
  );
}

const TRANSITIONS: { id: ZoneTransition; label: string; icon: React.ReactNode }[] = [
  {
    id: 'sequential',
    label: 'Out, then in',
    icon: (
      <svg viewBox="0 0 44 16">
        <rect x="1" y="3" width="18" height="10" rx="2" fill="currentColor" opacity=".45" />
        <rect x="25" y="3" width="18" height="10" rx="2" fill="currentColor" />
      </svg>
    ),
  },
  {
    id: 'cut',
    label: 'Cut',
    icon: (
      <svg viewBox="0 0 44 16">
        <rect x="1" y="3" width="21" height="10" rx="2" fill="currentColor" opacity=".45" />
        <rect x="22" y="3" width="21" height="10" rx="2" fill="currentColor" />
      </svg>
    ),
  },
  {
    id: 'fade',
    label: 'Fade',
    icon: (
      <svg viewBox="0 0 44 16">
        <rect x="1" y="3" width="28" height="10" rx="2" fill="currentColor" opacity=".45" />
        <rect x="15" y="3" width="28" height="10" rx="2" fill="currentColor" opacity=".8" />
      </svg>
    ),
  },
];

/** The right panel while the zones are edited: the list (drag to stack), the selected zone's settings. */
export function ZonesPanel({ project }: { project: Project }) {
  const { zones, selected, select, edit, move, add, remove, stop } = useZoneEdit();
  const saveZones = useApp((s) => s.saveZones);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState<string | null>(null);
  const [dropAt, setDropAt] = useState<{ id: string; after: boolean } | null>(null);
  const done = async () => {
    setBusy(true);
    const ordered = zones.map((z, i) => ({ ...z, name: z.name.trim(), layer: layerAt(zones, i) }));
    if (await saveZones(ordered)) stop();
    setBusy(false);
  };
  return (
    <div className="zp">
      <header className="zp-head">
        <span>{t('Zones')}</span>
        <em>{zones.length}</em>
        <span className="spacer" />
        <button className="icon-btn ghost" title={t('Add zone')} onClick={add}>
          <Plus size={14} />
        </button>
      </header>
      <div className="zp-list">
        {zones.map((z, i) => {
          const used = usesOf(project, z.id);
          return (
            <div
              key={z.id}
              className={`zp-zone ${z.id === selected ? 'on' : ''} ${dragging === z.id ? 'dragging' : ''} ${dropAt?.id === z.id && dragging !== z.id ? (dropAt.after ? 'drop-after' : 'drop-before') : ''}`}
              style={{ ['--zc' as string]: z.color }}
              onDragOver={(e) => {
                if (!dragging) return;
                e.preventDefault();
                const r = e.currentTarget.getBoundingClientRect();
                const after = e.clientY > r.top + Math.min(r.height, 40) / 2;
                if (dropAt?.id !== z.id || dropAt.after !== after) setDropAt({ id: z.id, after });
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (dragging && dropAt && dragging !== dropAt.id) {
                  const rest = zones.filter((x) => x.id !== dragging);
                  move(dragging, rest.findIndex((x) => x.id === dropAt.id) + (dropAt.after ? 1 : 0));
                }
                setDragging(null);
                setDropAt(null);
              }}
            >
              <div
                className="zp-row"
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.effectAllowed = 'move';
                  e.dataTransfer.setData('text/zone', z.id);
                  setDragging(z.id);
                }}
                onDragEnd={() => (setDragging(null), setDropAt(null))}
                onClick={() => select(z.id === selected ? null : z.id)}
              >
                <GripVertical size={13} className="zp-grip" />
                <span className="zp-sw" />
                <span className="zp-name">{z.name || '—'}</span>
                <span className="zp-l">L{layerAt(zones, i)}</span>
              </div>
              {z.id === selected && (
                <div className="zp-body">
                  <label>
                    <span>{t('Name')}</span>
                    <input value={z.name} onChange={(e) => edit(z.id, { name: e.target.value })} onKeyDown={(e) => e.stopPropagation()} />
                  </label>
                  <label>
                    <span>{t('Colour')}</span>
                    <span className="zp-colors">
                      {COLORS.map((c) => (
                        <button key={c} className={z.color.toLowerCase() === c ? 'on' : ''} style={{ background: c }} onClick={() => edit(z.id, { color: c })} />
                      ))}
                      <HexColorField compact title={t('Another colour')} value={z.color} onChange={(c) => edit(z.id, { color: c })} />
                    </span>
                  </label>
                  <label>
                    <span>{t('From one graphic to the next')}</span>
                    <span className="zp-trans">
                      {TRANSITIONS.map((x) => (
                        <button key={x.id} className={z.transition === x.id ? 'on' : ''} onClick={() => edit(z.id, { transition: x.id })}>
                          {x.icon}
                          {t(x.label)}
                        </button>
                      ))}
                    </span>
                  </label>
                  <button className="btn small zp-del" disabled={!!used || zones.length === 1} title={used ? tn(used, '{n} use', '{n} uses') : undefined} onClick={() => remove(z.id)}>
                    <Trash2 size={13} /> {t('Remove the zone')}
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>
      <footer className="zp-foot">
        <button className="btn" onClick={stop}>
          {t('Cancel')}
        </button>
        <button className="btn primary" disabled={busy || zones.some((z) => !z.name.trim())} onClick={done}>
          {t('Done')}
        </button>
      </footer>
    </div>
  );
}

function round(a: Area): Area {
  const r = (v: number) => Math.round(v * 10000) / 10000;
  return { x: r(a.x), y: r(a.y), width: r(a.width), height: r(a.height) };
}

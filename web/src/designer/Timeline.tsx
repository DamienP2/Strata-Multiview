import {
  ChevronDown,
  Zap,
  ChevronLeft,
  ChevronRight,
  Circle,
  ClipboardCopy,
  ClipboardPaste,
  Code,
  Copy,
  Diamond,
  DiamondMinus,
  Eye,
  EyeOff,
  Flag,
  FolderPlus,
  PenTool,
  Pentagon,
  Image,
  Lock,
  Minus,
  Pause,
  Play,
  Plus,
  Repeat,
  SkipBack,
  SkipForward,
  Sparkles,
  Square,
  StepBack,
  StepForward,
  Trash2,
  Unlink,
  Link as LinkIcon,
  X,
  Pilcrow,
  Type,
  Unlock,
  Video,
  MoreHorizontal,
} from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useShallow } from 'zustand/react/shallow';
import { childrenMap } from '../engine/Scene';
import { newId, num, propValue } from '../engine/evaluate';
import { COLOR_PROPS, PROPS, type Keyframe, type Layer, type LayerType, type Marker, type Prop, type Template } from '../engine/types';
import { NumberField } from '../ui/fields';
import { WIDGET_LIST } from '../widgets';
import { onTimeline, setOnTimeline } from '../engine/timelineProps';
import { descendants, duplicateLayers, findLayer, markerRole, setProp, toggleKeyAt, useDesigner, WIDGET_PRESETS, type KeyRef } from './designerStore';
import { PROP_LABELS } from './Inspector';
import { layerIcon, moveLayer, type DropPos } from './LayersPanel';
import { confirmDialog } from '../ui/dialogs';
import { clearKeyframes, clearSelectedKeys, copyAnimation, pasteAnimationOnRow, rowMapping, useClipboard } from './clipboard';
import { ClipboardMenu, type ClipMenuAt } from './ClipboardMenu';
import { t, tc, tn } from '../i18n';

/** Rows shown under an expanded layer, like After Effects' Transform group. */
const PROP_ROWS: {
  label: string;
  props: Prop[];
  display?: number;
  suffix?: string;
  labels?: string[];
  /** The two values keep their ratio when this setting of the layer is on. */
  lock?: 'scaleLock' | 'lockRatio';
}[] = [
  { label: 'Position', props: ['x', 'y'], labels: ['X', 'Y'] },
  { label: 'Size', props: ['width', 'height'], labels: ['W', 'H'], lock: 'lockRatio' },
  {
    label: 'Scale',
    props: ['scaleX', 'scaleY'],
    display: 100,
    suffix: '%',
    labels: ['X', 'Y'],
    lock: 'scaleLock',
  },
  { label: 'Rotation', props: ['rotation'], suffix: '°' },
  { label: 'Opacity', props: ['opacity'], display: 100, suffix: '%' },
];
const IN_ROWS = new Set(PROP_ROWS.flatMap((r) => r.props));

export type AddLayer = (type: LayerType, patch?: Record<string, unknown>, name?: string) => void;

export function formatTime(frame: number, fps: number): string {
  const f = Math.round(frame);
  const r = Math.round(fps);
  const s = Math.floor(f / r);
  const ff = f % r;
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}:${String(ff).padStart(2, '0')}`;
}

function flatten(t: Template): { layer: Layer; depth: number }[] {
  const tree = childrenMap(t.layers, true);
  const out: { layer: Layer; depth: number }[] = [];
  const walk = (pid: string | null, depth: number) => {
    for (const l of [...(tree.get(pid) ?? [])].reverse()) {
      out.push({ layer: l, depth });
      walk(l.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

/** Room after the end of the template, for the end marker and its handle. */
const TAIL = 24;

function tickStep(zoom: number): number {
  for (const s of [1, 2, 5, 10, 25, 50, 100, 250, 500, 1000]) if (s * zoom >= 56) return s;
  return 1000;
}

const sameKey = (a: KeyRef, b: KeyRef) => a.layerId === b.layerId && a.prop === b.prop && a.frame === b.frame;

export function Timeline({ fps, leftW, onAdd }: { fps: number; leftW: number; onAdd: AddLayer }) {
  const LEFT_W = leftW;
  const [renaming, setRenaming] = useState<string | null>(null);
  const [dragging, setDragging] = useState<{ id: string; frame: number } | null>(null);
  const [editing, setEditing] = useState<{ id: string; left: number; bottom: number } | null>(null);
  const [drop, setDrop] = useState<{ id: string; pos: DropPos } | null>(null);
  // Right-click menu of a row: copy / paste layers and animations.
  const [ctx, setCtx] = useState<ClipMenuAt | null>(null);
  const animClip = useClipboard((c) => c.animation);
  const st = useDesigner();
  const { template, frame, zoom, expanded, selectedKeys, playing, loop, selected } = st;
  const scrollRef = useRef<HTMLDivElement>(null);
  const rows = useMemo(() => (template ? flatten(template) : []), [template]);

  // Keep the playhead visible while playing.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !playing) return;
    const x = frame * zoom;
    if (x < el.scrollLeft || x > el.scrollLeft + el.clientWidth - LEFT_W - 40) el.scrollLeft = Math.max(0, x - 40);
  }, [frame, zoom, playing]);

  // Default zoom: the whole template fits the visible width. It follows the
  // panel size until the zoom is changed by hand, and again for each template.
  const fitted = useRef(true);
  const duration = template?.duration ?? 0;
  useEffect(() => {
    fitted.current = true;
  }, [template?.id]);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !duration) return;
    const fit = () => {
      if (!fitted.current) return;
      const z = (el.clientWidth - LEFT_W - TAIL) / duration;
      if (z > 0) useDesigner.setState({ zoom: Math.round(z * 100) / 100 });
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [template?.id, duration, LEFT_W]);

  if (!template)
    return (
      <div className="timeline tl-empty">
        <p>{t('No template open')}</p>
      </div>
    );
  const width = template.duration * zoom + TAIL;
  const step = tickStep(zoom);
  const outRaw = template.markers.find((m) => m.kind === 'out')?.frame;
  // A marker past the end (the duration was shortened) stays reachable at the end.
  const outF = outRaw === undefined ? undefined : Math.min(outRaw, template.duration);

  const frameAt = (clientX: number) => {
    const el = scrollRef.current!;
    const r = el.getBoundingClientRect();
    return (clientX - r.left - LEFT_W + el.scrollLeft) / zoom;
  };

  const scrub = (e: React.PointerEvent) => {
    e.preventDefault(); // no text selection while scrubbing
    st.setPlaying(false);
    st.setFrame(Math.round(frameAt(e.clientX)));
    const move = (ev: PointerEvent) => useDesigner.getState().setFrame(Math.round(frameAt(ev.clientX)));
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  const dragKeys = (e: React.PointerEvent, ref: KeyRef) => {
    e.stopPropagation();
    const s = useDesigner.getState();
    let sel = s.selectedKeys;
    const isSel = sel.some((k) => sameKey(k, ref));
    if (e.shiftKey) sel = isSel ? sel.filter((k) => !sameKey(k, ref)) : [...sel, ref];
    else if (!isSel) sel = [ref];
    s.selectKeys(sel);
    if (!sel.some((k) => sameKey(k, ref))) return;
    const base = s.template!;
    const startX = e.clientX;
    let lastDelta = 0;
    useDesigner.setState({ lastCoalesce: null });
    // The keys move together: the group stops at the start or the end, keys never pile up there.
    const lo = Math.min(...sel.map((k) => k.frame));
    const hi = Math.max(...sel.map((k) => k.frame));
    const move = (ev: PointerEvent) => {
      const delta = Math.max(-lo, Math.min(base.duration - hi, Math.round((ev.clientX - startX) / zoom)));
      if (delta === lastDelta) return;
      lastDelta = delta;
      const moved: KeyRef[] = [];
      useDesigner.getState().update((t) => {
        const groups = new Map<string, KeyRef[]>();
        for (const k of sel) groups.set(`${k.layerId}|${k.prop}`, [...(groups.get(`${k.layerId}|${k.prop}`) ?? []), k]);
        for (const [gk, refs] of groups) {
          const [layerId, prop] = gk.split('|') as [string, Prop];
          const baseTrack = findLayer(base, layerId)?.tracks[prop] ?? [];
          const frames = new Set(refs.map((r) => r.frame));
          const movedKeys: Keyframe[] = baseTrack
            .filter((k) => frames.has(k.frame))
            .map((k) => ({
              ...k,
              frame: k.frame + delta,
            }));
          const newFrames = new Set(movedKeys.map((k) => k.frame));
          const kept = baseTrack.filter((k) => !frames.has(k.frame) && !newFrames.has(k.frame));
          findLayer(t, layerId)!.tracks[prop] = [...kept, ...movedKeys].sort((a, b) => a.frame - b.frame);
          moved.push(...movedKeys.map((k) => ({ layerId, prop, frame: k.frame })));
        }
      }, 'key-drag');
      useDesigner.getState().selectKeys(moved);
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  /** A layer's bar: its ends set when it is shown (`from`, `to`), the bar itself moves it in time, its keyframes and its children's with it. */
  const dragSpan = (e: React.PointerEvent, layerId: string, mode: 'move' | 'from' | 'to') => {
    e.stopPropagation();
    if (e.button !== 0) return;
    const s = useDesigner.getState();
    const base = s.template!;
    const layer = findLayer(base, layerId);
    if (!layer || layer.locked) return;
    if (!s.selected.includes(layerId)) s.select([layerId]);
    const full = { from: 0, to: base.duration };
    const span0 = layer.span ?? full;
    const ids = [layerId, ...(mode === 'move' ? descendants(base, layerId) : [])];
    // Moving: every keyframe stays within the template (the bar is cut at its ends).
    const keys = ids.flatMap((id) => Object.values(findLayer(base, id)?.tracks ?? {}).flatMap((tr) => (tr ?? []).map((k) => k.frame)));
    const lo = Math.min(span0.to - 1, ...keys);
    const hi = Math.max(span0.from + 1, ...keys);
    const startX = e.clientX;
    let last = 0;
    useDesigner.setState({ lastCoalesce: null });
    const move = (ev: PointerEvent) => {
      let delta = Math.round((ev.clientX - startX) / zoom);
      if (mode === 'move') delta = Math.max(-lo, Math.min(base.duration - hi, delta));
      if (delta === last) return;
      last = delta;
      useDesigner.getState().update((t) => {
        const set = (id: string, span: { from: number; to: number } | undefined) => {
          const l = findLayer(t, id)!;
          if (!span || (span.from <= 0 && span.to >= t.duration)) delete l.span;
          else l.span = span;
        };
        if (mode === 'from') return set(layerId, { from: Math.max(0, Math.min(span0.to - 1, span0.from + delta)), to: span0.to });
        if (mode === 'to') return set(layerId, { from: span0.from, to: Math.max(span0.from + 1, Math.min(base.duration, span0.to + delta)) });
        for (const id of ids) {
          const was = findLayer(base, id)!;
          const l = findLayer(t, id)!;
          const sp = was.span ?? (id === layerId ? span0 : undefined);
          if (sp) set(id, { from: Math.max(0, sp.from + delta), to: Math.min(t.duration, sp.to + delta) });
          for (const [p, tr] of Object.entries(was.tracks)) if (tr) l.tracks[p as Prop] = tr.map((k) => ({ ...k, frame: k.frame + delta }));
        }
      }, 'span-drag');
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  /** The template's end, dragged: its length (never before a marker, a keyframe or a layer's start). */
  const dragDuration = (e: React.PointerEvent) => {
    e.stopPropagation();
    if (e.button !== 0) return;
    const base = useDesigner.getState().template!;
    const used = [
      ...base.markers.map((m) => m.frame),
      ...base.layers.flatMap((l) => [...Object.values(l.tracks).flatMap((tr) => (tr ?? []).map((k) => k.frame)), ...(l.span ? [l.span.from] : [])]),
    ];
    const min = Math.max(1, ...used) + 1;
    const startX = e.clientX;
    let last = base.duration;
    useDesigner.setState({ lastCoalesce: null });
    const move = (ev: PointerEvent) => {
      const d = Math.max(min, Math.min(600 * fps, Math.round(base.duration + (ev.clientX - startX) / zoom)));
      if (d === last) return;
      last = d;
      useDesigner.getState().update((t) => {
        t.duration = d;
        // A layer's bar stays within the template.
        for (const l of t.layers) if (l.span) l.span = { from: l.span.from, to: Math.min(l.span.to, d) };
      }, 'duration-drag');
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  // A marker moves by dragging (its time shows meanwhile); a click edits it.
  const dragMarker = (e: React.PointerEvent, id: string) => {
    e.stopPropagation();
    e.preventDefault();
    useDesigner.setState({ lastCoalesce: null });
    const target = e.currentTarget as HTMLElement;
    const startX = e.clientX;
    let moved = false;
    const move = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 3) return;
      moved = true;
      const f = Math.max(0, Math.min(template.duration, Math.round(frameAt(ev.clientX))));
      setDragging({ id, frame: f });
      useDesigner.getState().update((t) => {
        const m = t.markers.find((x) => x.id === id);
        if (m) m.frame = f;
      }, `marker-${id}`);
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      setDragging(null);
      if (!moved) {
        const r = target.getBoundingClientRect();
        setEditing({ id, left: Math.min(r.left, window.innerWidth - 300), bottom: window.innerHeight - r.top + 6 });
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  const addPause = () =>
    st.update((t) => {
      const f = Math.round(frame);
      if (t.markers.some((m) => m.kind !== 'label' && Math.abs(m.frame - f) < 0.5)) return;
      t.markers.push({ id: newId(), frame: f, name: 'Pause', kind: 'pause' });
    });

  /** ◀ ◆ ▶ of a timeline row: previous key, key at the playhead, next key. */
  const keyNav = (layer: Layer, props: Prop[], canToggle: boolean) => {
    const frames = [...new Set(props.flatMap((p) => (layer.tracks[p] ?? []).map((k) => k.frame)))].sort((a, b) => a - b);
    const prev = [...frames].reverse().find((f) => f < frame - 0.5);
    const next = frames.find((f) => f > frame + 0.5);
    const here = frames.some((f) => Math.abs(f - frame) < 0.5);
    return (
      <span className="tl-keynav" onClick={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
        <button title={t('Previous keyframe')} disabled={prev === undefined} onClick={() => prev !== undefined && st.setFrame(prev)}>
          <ChevronLeft size={12} />
        </button>
        {canToggle && (
          <button
            className={here ? 'on' : ''}
            title={here ? t('Remove the keyframe at the playhead') : t('Add a keyframe at the playhead')}
            onClick={() =>
              st.update((t) => {
                const l = findLayer(t, layer.id)!;
                // A row (Position…) keys all its properties together.
                for (const p of props) if (here === (l.tracks[p] ?? []).some((k) => Math.abs(k.frame - frame) < 0.5)) toggleKeyAt(l, p, frame);
              })
            }
          >
            <Diamond size={11} fill={here ? 'currentColor' : 'none'} />
          </button>
        )}
        <button title={t('Next keyframe')} disabled={next === undefined} onClick={() => next !== undefined && st.setFrame(next)}>
          <ChevronRight size={12} />
        </button>
      </span>
    );
  };

  const markers = [...template.markers].sort((a, b) => a.frame - b.frame);
  const jumpMarker = (dir: 1 | -1) => {
    const stops = [0, ...markers.map((m) => m.frame), template.duration];
    const target = dir > 0 ? stops.find((f) => f > frame + 0.5) : [...stops].reverse().find((f) => f < frame - 0.5);
    if (target !== undefined) st.setFrame(target);
  };

  const diamond = (ref: KeyRef, k: Keyframe, summary = false) => {
    const isSel = selectedKeys.some((s) => sameKey(s, ref));
    return (
      <div
        key={`${ref.prop}-${k.frame}`}
        className={`kf ${isSel ? 'sel' : ''} ${summary ? 'summary' : ''} e-${k.easing.type}`}
        style={{ left: k.frame * zoom }}
        title={`${t(PROP_LABELS[ref.prop])} @ ${k.frame} · ${k.easing.type}`}
        onPointerDown={summary ? undefined : (e) => dragKeys(e, ref)}
        onDoubleClick={() => st.setFrame(k.frame)}
      />
    );
  };

  /** The stretches between two keyframes: a click edits their easing. */
  const segments = (layer: Layer, props: Prop[]) => {
    const starts = new Map<number, number>();
    for (const p of props) {
      const ks = [...(layer.tracks[p] ?? [])].sort((x, y) => x.frame - y.frame);
      for (let i = 0; i + 1 < ks.length; i++) starts.set(ks[i].frame, Math.max(starts.get(ks[i].frame) ?? 0, ks[i + 1].frame));
    }
    const seg = st.selectedSegment;
    return [...starts].map(([from, to]) => {
      const on = !!seg && seg.layerId === layer.id && seg.frame === from && seg.props.some((p) => props.includes(p));
      return (
        <div
          key={`seg-${from}`}
          className={`kf-seg ${on ? 'sel' : ''}`}
          style={{ left: from * zoom + 6, width: Math.max(0, (to - from) * zoom - 12) }}
          title={t('Click: easing of this stretch')}
          onPointerDown={(e) => {
            e.stopPropagation();
            st.selectSegment({ layerId: layer.id, props: props.filter((p) => layer.tracks[p]?.some((k) => k.frame === from)), frame: from });
          }}
        />
      );
    });
  };

  const body: React.ReactNode[] = [];
  const labels: React.ReactNode[] = [];
  const track = (layer: Layer, props: Prop[]) => {
    const keys = props.flatMap((p) => (layer.tracks[p] ?? []).map((k) => ({ p, k })));
    if (!keys.length) return null;
    const frames = keys.map((x) => x.k.frame);
    const a = Math.min(...frames);
    const b = Math.max(...frames);
    return (
      <>
        {b > a && <div className="tl-span" style={{ left: a * zoom, width: (b - a) * zoom }} />}
        {segments(layer, props)}
        {keys.map(({ p, k }) => diamond({ layerId: layer.id, prop: p, frame: k.frame }, k))}
      </>
    );
  };
  for (const { layer, depth } of rows) {
    const animated = PROPS.filter((p) => layer.tracks[p]?.length);
    const open = expanded[layer.id];
    const isSel = selected.includes(layer.id);
    const Icon = layerIcon(layer);
    const dropCls = drop?.id === layer.id ? `drop-${drop.pos}` : '';
    labels.push(
      <div
        key={layer.id}
        className={`tl-label layer ${isSel ? 'selected' : ''} ${layer.visible ? '' : 'dim'} ${dropCls}`}
        style={{ paddingLeft: 4 + depth * 14 }}
        draggable={renaming !== layer.id}
        onDragStart={(e) => e.dataTransfer.setData('text/layer', layer.id)}
        onDragOver={(e) => {
          e.preventDefault();
          const r = e.currentTarget.getBoundingClientRect();
          const y = (e.clientY - r.top) / r.height;
          setDrop({
            id: layer.id,
            pos: layer.type === 'group' && y > 0.3 && y < 0.7 ? 'into' : y < 0.5 ? 'above' : 'below',
          });
        }}
        onDragLeave={() => setDrop(null)}
        onDrop={(e) => {
          const id = e.dataTransfer.getData('text/layer');
          const d = drop;
          setDrop(null);
          if (id && d) st.update((t) => moveLayer(t, id, d.id, d.pos, st.frame));
        }}
        onClick={(e) => {
          if (e.shiftKey || e.metaKey || e.ctrlKey) st.select(isSel ? selected.filter((x) => x !== layer.id) : [...selected, layer.id]);
          else st.select([layer.id]);
        }}
        onDoubleClick={() => setRenaming(layer.id)}
        onContextMenu={(e) => {
          e.preventDefault();
          if (!isSel) st.select([layer.id]);
          setCtx({ x: e.clientX, y: e.clientY, layerId: layer.id });
        }}
      >
        <span
          className="twirl"
          title={t('Show the properties')}
          onClick={(e) => {
            e.stopPropagation();
            st.toggleExpanded(layer.id);
          }}
        >
          {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </span>
        <Icon size={13} className="layer-icon" />
        {renaming === layer.id ? (
          <input
            autoFocus
            className="rename"
            defaultValue={layer.name}
            onClick={(e) => e.stopPropagation()}
            onBlur={(e) => {
              const v = e.target.value.trim();
              setRenaming(null);
              if (v && v !== layer.name) st.update((t) => void (findLayer(t, layer.id)!.name = v));
            }}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
              if (e.key === 'Escape') setRenaming(null);
            }}
          />
        ) : (
          <span className="layer-name">{layer.name}</span>
        )}
        {animated.length > 0 && !open && <span className="badge">{animated.length}</span>}
        <span className="row-actions">
          <button title={t('Duplicate (Ctrl+D)')} onClick={(e) => (e.stopPropagation(), duplicateLayers([layer.id]))}>
            <Copy size={12} />
          </button>
        </span>
        <button className={`row-btn ${layer.visible ? '' : 'off'}`} title={layer.visible ? t('Hide') : t('Show')} onClick={(e) => (e.stopPropagation(), st.update((t) => void (findLayer(t, layer.id)!.visible = !layer.visible)))}>
          {layer.visible ? <Eye size={12} /> : <EyeOff size={12} />}
        </button>
        <button className={`row-btn ${layer.locked ? 'on' : ''}`} title={layer.locked ? t('Unlock') : t('Lock')} onClick={(e) => (e.stopPropagation(), st.update((t) => void (findLayer(t, layer.id)!.locked = !layer.locked)))}>
          {layer.locked ? <Lock size={12} /> : <Unlock size={12} />}
        </button>
        <button
          className="row-btn"
          title={t('More')}
          onClick={(e) => {
            e.stopPropagation();
            if (!isSel) st.select([layer.id]);
            const r = e.currentTarget.getBoundingClientRect();
            setCtx({ x: r.left, y: r.bottom + 4, layerId: layer.id });
          }}
        >
          <MoreHorizontal size={12} />
        </button>
      </div>,
    );
    const allFrames = new Map<number, Keyframe>();
    for (const p of animated) for (const k of layer.tracks[p]!) allFrames.set(k.frame, k);
    body.push(
      <div key={layer.id} className={`tl-row layer ${isSel ? 'selected' : ''}`}>
        {(() => {
          const sp = layer.span ?? { from: 0, to: template.duration };
          return (
            <div className={`tl-bar ${layer.locked ? 'locked' : ''}`} style={{ left: sp.from * zoom, width: (sp.to - sp.from) * zoom }} title={t('Drag to start it later; its ends set when it is shown')} onPointerDown={(e) => dragSpan(e, layer.id, 'move')}>
              <span className="tl-bar-end from" onPointerDown={(e) => dragSpan(e, layer.id, 'from')} />
              <span className="tl-bar-end to" onPointerDown={(e) => dragSpan(e, layer.id, 'to')} />
            </div>
          );
        })()}
        {!open && [...allFrames.values()].map((k) => diamond({ layerId: layer.id, prop: 'x', frame: k.frame }, k, true))}
      </div>,
    );
    if (!open) continue;
    // Only what is on the timeline (position, size, opacity by default; the ◆ of the properties adds more).
    const tl = onTimeline(layer);
    const extra = PROPS.filter((p) => tl.has(p) && !IN_ROWS.has(p));
    const groups = [
      ...PROP_ROWS.filter((g) => g.props.some((p) => tl.has(p))),
      ...extra.map((p) => ({
        label: PROP_LABELS[p],
        props: [p] as Prop[],
        display: undefined,
        suffix: undefined,
        labels: undefined,
        lock: undefined,
      })),
    ];
    labels.push(
      <div key={`${layer.id}-tr`} className="tl-label group" style={{ paddingLeft: 22 + depth * 14 }}>
        {tc('section', 'Transform')}
      </div>,
    );
    body.push(<div key={`${layer.id}-tr`} className="tl-row prop" />);
    for (const g of groups) {
      const isAnim = g.props.some((p) => layer.tracks[p]?.length);
      labels.push(
        <div
          key={`${layer.id}-${g.label}`}
          className={`tl-label prop ${isAnim ? 'animated' : ''}`}
          style={{ paddingLeft: 14 + depth * 14 }}
          onContextMenu={(e) => {
            e.preventDefault();
            setCtx({ x: e.clientX, y: e.clientY, layerId: layer.id, row: { props: g.props, label: g.label } });
          }}
        >
          {keyNav(layer, g.props, true)}
          <span className="prop-name">{t(g.label)}</span>
          {isAnim && (
            <button className="tl-off tl-anim" title={t('Copy the animation of {prop}', { prop: t(g.label) })} onClick={() => copyAnimation(layer.id, g.props, g.label)}>
              <ClipboardCopy size={11} />
            </button>
          )}
          {animClip && rowMapping(animClip, g.props).length > 0 && (
            <button className="tl-off tl-anim" title={t('Paste the animation of {what}', { what: animClip.label })} onClick={() => pasteAnimationOnRow(layer.id, g.props)}>
              <ClipboardPaste size={11} />
            </button>
          )}
          {isAnim && (
            <button className="tl-off tl-anim tl-erase" title={t('Erase the keyframes of {prop}', { prop: t(g.label) })} onClick={() => clearKeyframes([layer.id], g.props, t('Erase the keyframes of {prop} (“{name}”)?', { prop: t(g.label), name: layer.name }))}>
              <DiamondMinus size={11} />
            </button>
          )}
          <button
            className="tl-off"
            title={t('Remove from the timeline')}
            onClick={async () => {
              const keyed = g.props.some((p) => layer.tracks[p]?.length);
              if (keyed && !(await confirmDialog({ title: t('Remove {prop} from the timeline?', { prop: t(g.label) }), message: t('Its keyframes are deleted; the value at the playhead stays.'), confirm: t('Remove'), danger: true }))) return;
              st.update((t) => {
                const l = findLayer(t, layer.id)!;
                for (const p of g.props) if (l.tracks[p]?.length) l.props[p] = propValue(l, p, frame);
                setOnTimeline(l, g.props, false);
              });
            }}
          >
            <X size={10} />
          </button>
          {g.lock && (
            <button className={`tl-lock ${layer[g.lock] ? 'on' : ''}`} title={layer[g.lock] ? t('Proportions locked (click to unlock)') : t('Lock the proportions')} onClick={() => st.update((t) => void (findLayer(t, layer.id)![g.lock!] = !layer[g.lock!] || undefined))}>
              {layer[g.lock] ? <LinkIcon size={11} /> : <Unlink size={11} />}
            </button>
          )}
          <span className="prop-values">
            {g.props.map((p, i) =>
              COLOR_PROPS.has(p) ? (
                <span
                  key={p}
                  className="prop-swatch"
                  style={{
                    background: `rgba(${(propValue(layer, p, frame) as number[])
                      .slice(0, 3)
                      .map((c) => Math.round(c * 255))
                      .join(',')},${(propValue(layer, p, frame) as number[])[3]})`,
                  }}
                />
              ) : (
                <NumberField
                  key={p}
                  label={g.labels?.[i] && t(g.labels[i])}
                  value={num(propValue(layer, p, frame))}
                  display={g.display}
                  suffix={g.suffix}
                  onChange={(v) =>
                    st.update((t) => {
                      const l = findLayer(t, layer.id)!;
                      // Locked scale or size: the two keep their ratio.
                      if (g.lock && l[g.lock] && g.props.length === 2) {
                        const other: Prop = p === g.props[0] ? g.props[1] : g.props[0];
                        const cur = num(propValue(l, p, frame));
                        setProp(l, other, cur ? (num(propValue(l, other, frame)) * v) / cur : v, frame);
                      }
                      setProp(l, p, v, frame);
                    }, `tl-${layer.id}-${g.label}`)
                  }
                />
              ),
            )}
          </span>
        </div>,
      );
      body.push(
        <div key={`${layer.id}-${g.label}`} className="tl-row prop">
          {track(layer, g.props)}
        </div>,
      );
    }
  }

  return (
    <div className="timeline">
      <div className="tl-toolbar">
        <InsertBar width={LEFT_W} onAdd={onAdd} />
        <div className="transport">
          <button className="icon-btn" title={t('Go to start (Home)')} onClick={() => st.setFrame(0)}>
            <SkipBack size={15} />
          </button>
          <button className="icon-btn" title={t('Previous marker')} onClick={() => jumpMarker(-1)}>
            <StepBack size={15} />
          </button>
          <button className="icon-btn" title={t('Previous frame')} onClick={() => st.setFrame(Math.max(0, Math.ceil(frame - 1e-6) - 1))}>
            <ChevronLeft size={15} />
          </button>
          <button className="icon-btn play" title={t('Play / pause (K)')} onClick={() => st.setPlaying(!playing)}>
            {playing ? <Pause size={16} /> : <Play size={16} />}
          </button>
          <button className="icon-btn" title={t('Next frame')} onClick={() => st.setFrame(Math.min(template.duration, Math.floor(frame + 1e-6) + 1))}>
            <ChevronRight size={15} />
          </button>
          <button className="icon-btn" title={t('Next marker')} onClick={() => jumpMarker(1)}>
            <StepForward size={15} />
          </button>
          <button className="icon-btn" title={t('Go to end (End)')} onClick={() => st.setFrame(template.duration)}>
            <SkipForward size={15} />
          </button>
        </div>
        <button className={`btn small tl-loop ${loop ? 'on' : ''}`} title={t('Loop: play through the markers and start over at the end. Off: playback stops on On air, Pause and Out, like on air (K resumes).')} aria-label={t('Loop')} onClick={() => st.set({ loop: !loop })}>
          <Repeat size={13} />
        </button>
        <KeysMenu />
        <button className="btn small mk-pause" title={t('Add a pause at the playhead: playback stops there and resumes on Continue, after a delay or on a trigger (click the marker to set it)')} onClick={addPause}>
          <Flag size={13} /> {t('Pause')}
        </button>
        {/* The playhead's time, in the middle of the room between Pause and Zoom. */}
        <div className="spacer" />
        <div className="timecode">
          <strong>{formatTime(frame, fps)}</strong>
          <span>
            {Math.round(frame)} / {template.duration}
          </span>
        </div>
        <div className="spacer" />
        <label className="zoom">
          {t('Zoom')}
          <input
            type="range"
            min={0.2}
            max={40}
            step={0.1}
            value={zoom}
            onChange={(e) => {
              fitted.current = false;
              st.set({ zoom: +e.target.value });
            }}
          />
        </label>
        <button
          className="btn small"
          title={t('Fit the whole template in the timeline')}
          onClick={() => {
            const el = scrollRef.current;
            fitted.current = true;
            if (el)
              st.set({
                zoom: Math.max(0.2, Math.round(((el.clientWidth - LEFT_W - TAIL) / template.duration) * 100) / 100),
              });
          }}
        >
          {t('Fit')}
        </button>
      </div>
      <div className="tl-scroll" ref={scrollRef}>
        <div className="tl-grid" style={{ width: LEFT_W + width }}>
          <div className="tl-left" style={{ width: LEFT_W }}>
            <div className="tl-corner">
              {t('Layers')}
              <AddMenu onAdd={onAdd} />
            </div>
            {labels}
          </div>
          <div className="tl-right" style={{ width }}>
            <div className="tl-ruler" onPointerDown={scrub}>
              {/* IN: from the start to the hold (on air); OUT: from the hold to the end. */}
              <div className="tl-zone in" style={{ left: 0, width: (outF ?? template.duration) * zoom }}>
                <small>{((outF ?? template.duration) / fps).toFixed(2)} s</small>IN
              </div>
              {outF !== undefined && (
                <div className="tl-zone out" style={{ left: outF * zoom, width: (template.duration - outF) * zoom }}>
                  OUT<small>{((template.duration - outF) / fps).toFixed(2)} s</small>
                </div>
              )}
              {Array.from({ length: Math.floor(template.duration / step) + 1 }, (_, i) => (
                <div key={i} className="tick" style={{ left: i * step * zoom }}>
                  <span>{i * step}</span>
                </div>
              ))}
              <div className="tl-dur" style={{ left: template.duration * zoom }} title={t('Length of the template: drag')} onPointerDown={dragDuration}>
                <span>{(template.duration / fps).toFixed(2)} s</span>
              </div>
              {markers.map((m) => {
                const role = markerRole(template, m);
                const moving = dragging?.id === m.id;
                return (
                  <div
                    key={m.id}
                    className={`tl-marker ${role} ${moving ? 'moving' : ''} ${editing?.id === m.id ? 'editing' : ''}`}
                    style={{ left: Math.min(m.frame, template.duration) * zoom }}
                    title={t('{name} · {time} — drag to move, click to edit, double-click to go there', { name: role === 'onair' ? t('End of IN, start of OUT: the take holds here on air') : m.name, time: formatTime(m.frame, fps) })}
                    onPointerDown={(e) => dragMarker(e, m.id)}
                    onDoubleClick={() => st.setFrame(m.frame)}
                  >
                    {moving ? (
                      <span className="mk-time">{formatTime(dragging.frame, fps)}</span>
                    ) : role === 'onair' ? (
                      <span className="mk-hold" title={m.resume?.type === 'after' ? t('Goes out by itself after {s} s', { s: m.resume.seconds }) : m.resume?.type === 'trigger' ? t('Goes out on the trigger {name}', { name: m.resume.name }) : m.resume?.type === 'triggers' ? t('Goes out when its Triggers say so') : t('Holds until Take out')}>
                        {m.resume?.type === 'after' ? `${m.resume.seconds} s` : m.resume?.type === 'trigger' ? '⚡' : m.resume?.type === 'triggers' ? <Zap size={10} /> : '∞'}
                      </span>
                    ) : (
                      <span>
                        {m.name}
                        {m.resume?.type === 'after' ? ` · ${m.resume.seconds}s` : m.resume?.type === 'trigger' ? ' · ⚡' : m.resume?.type === 'triggers' ? ' · ⚡' : ''}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
            <div className="tl-rows" onPointerDown={() => st.selectKeys([])}>
              {outF !== undefined && (
                <div
                  className="out-zone"
                  style={{
                    left: outF * zoom,
                    width: (template.duration - outF) * zoom,
                  }}
                />
              )}
              {markers.map((m) => (
                <div key={m.id} className={`marker-line ${markerRole(template, m)}`} style={{ left: Math.min(m.frame, template.duration) * zoom }} />
              ))}
              {body}
              <div className="tl-end" style={{ left: template.duration * zoom }} />
            </div>
            <div className="playhead" style={{ left: frame * zoom }} />
          </div>
        </div>
      </div>
      {ctx && <ClipboardMenu at={ctx} onClose={() => setCtx(null)} />}
      {editing && template.markers.some((m) => m.id === editing.id) && <MarkerEditor id={editing.id} fps={fps} pos={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}

/** Popover of a marker: how it plays on (Operator, after a delay, on a trigger); for a pause also its name, time and delete. */
function MarkerEditor({ id, fps, pos, onClose }: { id: string; fps: number; pos: { left: number; bottom: number }; onClose: () => void }) {
  const st = useDesigner();
  const tpl = st.template!;
  const m = tpl.markers.find((x) => x.id === id)!;
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const down = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onClose();
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('pointerdown', down);
    window.addEventListener('keydown', key);
    return () => {
      window.removeEventListener('pointerdown', down);
      window.removeEventListener('keydown', key);
    };
  }, [onClose]);
  const edit = (fn: (x: Marker) => void, coalesce?: string) =>
    st.update((tt) => {
      const x = tt.markers.find((y) => y.id === id);
      if (x) fn(x);
    }, coalesce);
  const onAir = m.kind === 'out';
  const resume = m.resume ?? { type: 'manual' };
  // Its node in the Triggers: « Sortir (OUT) » for the IN/OUT point, « Reprendre la pause » for a pause.
  const ownNode = (l: Template['triggers']) => l?.nodes.find((n) => (onAir ? n.type === 'out' : n.type === 'resume' && n.marker === id));
  const events = (tpl.triggers?.nodes ?? []).filter((n) => n.type === 'event').length;
  const toLogic = () =>
    st.update((tt) => {
      const x = tt.markers.find((y) => y.id === id);
      if (x) x.resume = { type: 'triggers' };
      tt.triggers ??= { nodes: [], edges: [] };
      if (!ownNode(tt.triggers)) {
        const right = Math.max(400, ...tt.triggers.nodes.map((n) => n.x + 280));
        tt.triggers.nodes.push(onAir ? { id: newId(), type: 'out', x: right, y: 60 } : { id: newId(), type: 'resume', marker: id, x: right, y: 200 });
      }
    });
  return createPortal(
    <div ref={ref} className="menu marker-editor" style={{ position: 'fixed', left: Math.max(8, pos.left), bottom: pos.bottom, top: 'auto' }} onKeyDown={(e) => e.stopPropagation()}>
      <header>
        <span className={`marker-dot ${onAir ? 'onair' : 'pause'}`} />
        <strong>{onAir ? 'IN → OUT' : t('Pause')}</strong>
        <div className="spacer" />
        <button className="icon-btn ghost" title={t('Close')} onClick={onClose}>
          <X size={14} />
        </button>
      </header>
      {!onAir && (
        <label className="me-field">
          <span>{t('Name')}</span>
          <input value={m.name} onChange={(e) => edit((x) => void (x.name = e.target.value), `marker-name-${id}`)} />
        </label>
      )}
      {!onAir && (
        <label className="me-field">
          <span>{tc('position', 'Time')}</span>
          <NumberField value={m.frame} min={0} max={tpl.duration} suffix="fr" onChange={(v) => edit((x) => void (x.frame = Math.round(v)), `marker-${id}`)} />
          <em>{formatTime(m.frame, fps)}</em>
        </label>
      )}
      <>
          <div className="me-field">
            <span>{onAir ? tc('marker', 'Out') : t('Resume')}</span>
            <div className="seg">
              {(
                [
                  ['manual', 'Operator'],
                  ['after', 'After'],
                  ['trigger', 'Trigger'],
                ] as const
              ).map(([k, label]) => (
                <button
                  key={k}
                  className={resume.type === k || (k === 'trigger' && resume.type === 'triggers') ? 'on' : ''}
                  onClick={() =>
                    edit((x) => {
                      if (k === 'manual') delete x.resume;
                      else if (k === 'after') x.resume = { type: 'after', seconds: resume.type === 'after' ? resume.seconds : onAir ? 5 : 2 };
                      else if (resume.type !== 'trigger' && resume.type !== 'triggers') x.resume = { type: 'trigger', name: 'next' };
                    })
                  }
                >
                  {t(label)}
                </button>
              ))}
            </div>
          </div>
          {resume.type === 'after' && (
            <label className="me-field">
              <span>{t('Delay')}</span>
              <NumberField value={resume.seconds} min={0} step={0.5} suffix="s" onChange={(v) => edit((x) => void (x.resume = { type: 'after', seconds: Math.max(0, v) }), `marker-delay-${id}`)} />
            </label>
          )}
          {(resume.type === 'trigger' || resume.type === 'triggers') && (
            <div className="me-field">
              <span>{t('By')}</span>
              <div className="seg">
                <button className={resume.type === 'trigger' ? 'on' : ''} onClick={() => edit((x) => void (x.resume = { type: 'trigger', name: 'next' }))}>
                  {tc('trigger', 'Name')}
                </button>
                <button className={resume.type === 'triggers' ? 'on' : ''} onClick={() => resume.type !== 'triggers' && toLogic()}>
                  {t('Triggers')}
                </button>
              </div>
            </div>
          )}
          {resume.type === 'trigger' && (
            <label className="me-field">
              <span>{tc('trigger', 'Name')}</span>
              <input value={resume.name} placeholder={t('goal, next, line2…')} onChange={(e) => edit((x) => void (x.resume = { type: 'trigger', name: e.target.value }), `marker-trigger-${id}`)} />
            </label>
          )}
          {resume.type === 'triggers' && (
            <button
              className="me-logic"
              onClick={() => {
                if (!ownNode(tpl.triggers)) toLogic();
                window.dispatchEvent(new Event('strata:open-triggers'));
                onClose();
              }}
            >
              <Zap size={14} /> {t('Open the Triggers')}
              <small>{tn(events, '{n} event', '{n} events')}</small>
            </button>
          )}
          {resume.type !== 'triggers' && (
          <p className="hint">
            {onAir
              ? resume.type === 'manual'
                ? t('Stays on air until Take out.')
                : resume.type === 'after'
                  ? t('Goes out by itself after the delay.')
                  : t('Stays on air until this trigger is fired, then goes out: by the operator, the API (POST /api/channels/{id}/command {"action":"trigger","name":…}) or Companion.')
              : resume.type === 'manual'
                ? t('Holds until the operator presses Continue.')
                : resume.type === 'after'
                  ? t('Holds, then plays on by itself after the delay.')
                  : t('Holds until this trigger is fired: by the operator, the API (POST /api/channels/{id}/command {"action":"trigger","name":…}) or Companion.')}
          </p>
          )}
          {!onAir && (
          <div className="row end">
            <button
              className="btn small danger"
              onClick={() => {
                st.update((tt) => void (tt.markers = tt.markers.filter((x) => x.id !== id)));
                onClose();
              }}
            >
              <Trash2 size={13} /> {t('Delete pause')}
            </button>
          </div>
          )}
      </>
    </div>,
    document.body,
  );
}

/** A Paragraph's text to begin with. */
const LOREM_PARAGRAPH = 'Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.';

/** Shapes on hand above the layers; the rest behind "+". */
function InsertBar({ width, onAdd }: { width: number; onAdd: AddLayer }) {
  return (
    <div className="tl-insert" style={{ width }}>
      <button className="add-btn folder" title={t('New folder (group)')} onClick={() => onAdd('group')}>
        <FolderPlus size={14} />
      </button>
      <span className="tl-pipe" />
      <button className="tool" title={t('Rectangle')} onClick={() => onAdd('rect')}>
        <Square size={15} />
      </button>
      <button className="tool" title={t('Ellipse')} onClick={() => onAdd('ellipse')}>
        <Circle size={15} />
      </button>
      <button className="tool" title={t('Polygon')} onClick={() => onAdd('polygon', { sides: 5 })}>
        <Pentagon size={15} />
      </button>
      <button
        className="tool"
        title={tc('stroke', 'Line')}
        // A straight Pen stroke: round ends, dashes along the line (a thin rectangle had neither).
        onClick={() =>
          onAdd(
            'pen',
            {
              points: [
                { x: 0, y: 0.5 },
                { x: 1, y: 0.5 },
              ],
              arrowEnd: 'none',
              drawIn: false,
              props: {
                x: 160,
                y: 520,
                width: 600,
                height: 40,
                stroke: [1, 1, 1, 1],
                strokeWidth: 4,
              },
            },
            tc('stroke', 'Line'),
          )
        }
      >
        <Minus size={15} />
      </button>
      <button className="tool" title={tc('text', 'Line')} onClick={() => onAdd('text', undefined, tc('text', 'Line'))}>
        <Type size={15} />
      </button>
      <button
        className="tool"
        title={t('Paragraph')}
        onClick={() => onAdd('text', { wrap: true, text: LOREM_PARAGRAPH, props: { width: 900, height: 300, fontSize: 40 } }, t('Paragraph'))}
      >
        <Pilcrow size={15} />
      </button>
      <button className="tool" title={t('Image')} onClick={() => onAdd('image')}>
        <Image size={15} />
      </button>
      <button className="tool" title={t('Pen')} onClick={() => onAdd('pen')}>
        <PenTool size={15} />
      </button>
    </div>
  );
}

type MenuItem = {
  label: string;
  icon: typeof Square;
  /** A spec widget (type + settings), or a preset of the older widgets. */
  type?: LayerType;
  patch?: Record<string, unknown>;
  preset?: string;
  hint?: string;
  soon?: boolean;
};

/** The + menu: one list of widgets, in this order. */
const MENU: { type?: LayerType; widget?: string; label: string; soon?: typeof Video; patch?: Record<string, unknown>; action?: 'video' | 'lottie'; hint?: string; preset?: string }[] = [
  { type: 'ticker', label: 'Ticker' },
  { type: 'credits', label: 'Credits' },
  { widget: 'clock', label: 'Clock' },
  { widget: 'time', label: 'Time' },
  { type: 'table', label: 'Table' },
  { type: 'counter', label: 'Counter' },
  { type: 'chart', label: 'Chart' },
  { type: 'typewriter', label: 'Typewriter' },
  { type: 'progress', label: 'Progress bar' },
  { type: 'map', label: 'Map' },
  { type: 'qrCode', label: 'QR code' },
  { label: 'Video', soon: Video, action: 'video', hint: 'A video file (mp4, mov, webm…; alpha kept) played as frames, at most a minute' },
  { type: 'web', label: 'Iframe', soon: Code, hint: 'A web page or your HTML code (HTML5 outputs)' },
  { label: 'Animation (Lottie)', soon: Sparkles, action: 'lottie', hint: 'A Lottie / Bodymovin file (.json, .zip, .lottie) added as layers' },
  { type: 'scoreboard', label: 'Scoreboard' },
  { type: 'socialWall', label: 'Social wall' },
  { type: 'socialBadge', label: 'Social badge' },
];
type Menu = MenuItem & { action?: 'video' | 'lottie' };
const menu = (): Menu[] =>
  MENU.map((m) => {
    // A widget by its id (a type drawn by two widgets: the Clock and the Time) comes with its own settings.
    const w = m.widget ? WIDGET_LIST.find((x) => x.id === m.widget) : m.type ? WIDGET_LIST.find((x) => x.type === m.type) : undefined;
    const patch = m.widget && w ? (w.create() as Record<string, unknown>) : m.patch;
    return { label: m.label, icon: m.soon && !w ? m.soon : (w?.icon ?? m.soon ?? Square), type: w?.type, patch, hint: m.hint, preset: m.preset, action: m.action, soon: !w && !m.action && !m.preset };
  });

function AddMenu({ onAdd }: { onAdd: AddLayer }) {
  const [open, setOpen] = useState<{ left: number; bottom: number } | null>(null);
  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(null);
    window.addEventListener('pointerdown', close);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('resize', close);
    };
  }, [open]);
  const pick = (it: Menu) => {
    // Video and Lottie are placed at once; their file is chosen on the right.
    if (it.action === 'video') {
      setOpen(null);
      onAdd('image', { video: true, fit: 'contain', src: '' }, t('Video'));
      return;
    }
    if (it.action === 'lottie') {
      setOpen(null);
      onAdd('group', { lottie: '', clip: true }, t('Animation (Lottie)'));
      return;
    }
    if (it.type) {
      setOpen(null);
      onAdd(it.type, it.patch, tc('widget', it.label));
      return;
    }
    const w = WIDGET_PRESETS.find((x) => x.key === it.preset);
    if (!w) return;
    setOpen(null);
    onAdd(w.type, w.patch as Record<string, unknown> | undefined, t(w.label));
  };
  return (
    <div className="menu-wrap" onPointerDown={(e) => e.stopPropagation()}>
      <button
        className={`opx-add ${open ? 'on' : ''}`}
        title={t('Widgets')}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setOpen(
            open
              ? null
              : {
                  left: Math.max(8, r.left),
                  bottom: window.innerHeight - r.top + 6,
                },
          );
        }}
      >
        <Plus size={14} />
      </button>
      {open && (
        <div
          className="menu add-menu"
          style={{
            position: 'fixed',
            left: open.left,
            bottom: open.bottom,
            top: 'auto',
          }}
        >
          <div className="add-grid">
            {menu().map((it) => (
              <button key={it.label} disabled={it.soon} title={it.soon ? t('Coming soon') : it.hint && t(it.hint)} onClick={() => pick(it)}>
                <span className="add-icon">
                  <it.icon size={15} />
                </span>
                {tc('widget', it.label)}
                {it.soon && <em>{t('soon')}</em>}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}


/** The timeline's « Keyframes » menu: erase the selected ones, those of the selected layers, or all. */
function KeysMenu() {
  const [open, setOpen] = useState(false);
  const { template, selected, selectedKeys, readOnly } = useDesigner(useShallow((s) => ({ template: s.template, selected: s.selected, selectedKeys: s.selectedKeys, readOnly: s.readOnly })));
  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.stopPropagation(), setOpen(false));
    window.addEventListener('pointerdown', close);
    window.addEventListener('keydown', esc, true);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('keydown', esc, true);
    };
  }, [open]);
  if (!template) return null;
  const keyed = (ids: string[]) => ids.some((id) => PROPS.some((p) => findLayer(template, id)?.tracks[p]?.length));
  const all = template.layers.map((l) => l.id);
  const run = (fn: () => void) => () => (setOpen(false), fn());
  return (
    <div className="menu-wrap tl-keys" onPointerDown={(e) => e.stopPropagation()}>
      <button className={`btn small ${open ? 'on' : ''}`} disabled={readOnly || !keyed(all)} onClick={() => setOpen(!open)}>
        <Diamond size={13} /> {t('Keyframes')} <ChevronDown size={12} />
      </button>
      {open && (
        <div className="menu project-menu tl-keys-menu" role="menu">
          <button disabled={!selectedKeys.length} onClick={run(() => clearSelectedKeys(true))}>
            <DiamondMinus size={13} /> {t('Erase the selected keyframes')}
            <kbd>{t('Del')}</kbd>
          </button>
          <button disabled={!keyed(selected)} onClick={run(() => clearKeyframes(selected, undefined, selected.length > 1 ? t('Erase the keyframes of {n} layers?', { n: selected.length }) : t('Erase the keyframes of “{name}”?', { name: findLayer(template, selected[0])?.name ?? '' })))}>
            <DiamondMinus size={13} /> {t('Erase the keyframes of the selected layers')}
          </button>
          <div className="menu-sep" />
          <button className="danger" onClick={run(() => clearKeyframes(all, undefined, t('Erase every keyframe of the template?')))}>
            <Trash2 size={13} /> {t('Erase every keyframe of the template')}
          </button>
        </div>
      )}
    </div>
  );
}

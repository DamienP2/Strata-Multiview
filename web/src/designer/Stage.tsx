import { useTableEditor } from '../widgets/table-editor-store';
import { clientNow } from '../store';
import { useShallow } from 'zustand/react/shallow';
import { settingDefaults } from '../engine/settings';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { LiveTime, type Ticking } from '../engine/LiveTime';
import { Scene, Viewport } from '../engine/Scene';
import { defaultData, holdFrame, num, propValue } from '../engine/evaluate';
import { designTemplates } from '../engine/drafts';
import { inFormat } from '../engine/formats';
import { areaClip, zoneArea } from '../engine/zones';
import type { Layer, Project, Template } from '../engine/types';
import { findLayer, runAge, setProp, useDesigner } from './designerStore';
import { ZoneOverlay } from './ZoneOverlay';
import { useZoneEdit, ZonesEditOverlay } from './ZoneEditor';
import { ClipboardMenu, type ClipMenuAt } from './ClipboardMenu';
import { widgetOf } from '../widgets';
import { useBoundData } from '../data/dataStore';
import { snapTargets, StageGuides, stageViewStyle, useGuidesView, useProjectGuides } from './Guides';
import { t, tn } from '../i18n';
import { useLiveFrame } from '../engine/liveInputs';
import { useGhosts } from './ghosts';

interface Box {
  id: string;
  left: number;
  top: number;
  width: number;
  height: number;
}

/** The boxes as they were when they did not change (no new state, no redraw). */
const same = (was: Box[], now: Box[]) => (was.length === now.length && was.every((b, i) => b.id === now[i].id && b.left === now[i].left && b.top === now[i].top && b.width === now[i].width && b.height === now[i].height) ? was : now);

type Handle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';

/** A line a dragged box snaps to: a guide or the picture's edge (no box: across the picture), or another layer's edge or centre. */
interface SnapLine {
  v: number;
  /** The other layer's extent along the line (stage pixels). */
  from?: number;
  to?: number;
}

/** The shift that puts one of `edges` on the nearest line within `th`, and that line. */
function nearest(edges: number[], lines: SnapLine[], th: number): number {
  let best = 0;
  let bestD = th;
  for (const e of edges)
    for (const l of lines) {
      const d = Math.abs(l.v - e);
      if (d < bestD) {
        bestD = d;
        best = l.v - e;
      }
    }
  return best;
}

/** A smart guide shown while dragging: along x (vertical line) or y, from / to on the other axis. */
interface Shown {
  axis: 'x' | 'y';
  v: number;
  from: number;
  to: number;
}
const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];

/** Zoom of the canvas, set by hand: from 5 % to 400 %. */
const ZOOM_MIN = 0.05;
const ZOOM_MAX = 4;
const clampZoom = (z: number) => Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, z));

/** The canvas's zoom, in the canvas toolbar: a slider, Fit (once) and Auto (fits and follows the panels' size). */
export function StageZoom() {
  const { stageZoom, stageFit } = useDesigner(useShallow((s) => ({ stageZoom: s.stageZoom, stageFit: s.stageFit })));
  const zoom = stageZoom ?? stageFit;
  return (
    <>
      <label className="zoom" title={`${Math.round(zoom * 100)} %`}>
        {t('Zoom')}
        <input type="range" min={Math.log2(ZOOM_MIN)} max={Math.log2(ZOOM_MAX)} step={0.01} value={Math.log2(zoom)} onChange={(e) => useDesigner.setState({ stageZoom: clampZoom(2 ** +e.target.value) })} />
      </label>
      <button className="btn small" title={t('Fit the picture in the canvas')} onClick={() => useDesigner.setState({ stageZoom: stageFit })}>
        {t('Fit')}
      </button>
      <button className={`btn small ${stageZoom === null ? 'on' : ''}`} title={t('Fit the picture in the canvas, and again when the panels are resized')} onClick={() => useDesigner.setState({ stageZoom: stageZoom === null ? stageFit : null })}>
        {t('Auto')}
      </button>
    </>
  );
}

/** The canvas, at the size of the format being edited. */
export function Stage({ project: _project, size, showZones, background, bgInput }: { project: Project; size: { width: number; height: number }; showZones: boolean; background: string; /** The live input shown behind (« video » background). */ bgInput: string }) {
  const project = { ..._project, width: size.width, height: size.height };
  const { template, frame, previewData, selected, format } = useDesigner(useShallow((s) => ({ template: s.template, frame: s.frame, previewData: s.previewData, selected: s.selected, format: s.format })));
  // Bound fields show their live value, like on air.
  const bound = useBoundData(template, previewData);
  const data = useMemo(() => ({ ...bound, ...settingDefaults(template) }), [bound, template]);
  const wrapRef = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<HTMLDivElement>(null);
  // Fitted to the canvas area (Auto), or as set by hand (StageZoom).
  const { stageZoom, stageFit } = useDesigner(useShallow((s) => ({ stageZoom: s.stageZoom, stageFit: s.stageFit })));
  const scale = stageZoom ?? stageFit;
  // Opening the Designer: Auto, the picture fitted.
  useEffect(() => useDesigner.setState({ stageZoom: null }), []);
  const [boxes, setBoxes] = useState<Box[]>([]);
  // The other layers: a faint frame, so the template's layout reads at a glance.
  const [ghosts, setGhosts] = useState<Box[]>([]);
  // Right-click menu of the layer under the pointer (the timeline's, ClipboardMenu).
  const [ctx, setCtx] = useState<ClipMenuAt | null>(null);
  const widgetRun = useDesigner((s) => s.widgetRun);
  const widgetState = useDesigner((s) => s.widgetState);
  const running = Object.values(widgetRun).some((r) => r.pausedAge === undefined);
  const fps = project.frameRate.num / project.frameRate.den || 1;
  // Only the widgets that move redraw on each frame: all of them while a try-out runs, else those moving with the wall clock (a clock…).
  const ticking: Ticking = running ? 'all' : 'wall';
  // Resting on a pause or the On air point, the time on air runs on, as on air: loop animations keep moving.
  const playing = useDesigner((s) => s.playing);
  const atHold = !playing && !!template?.markers.some((m) => (m.kind === 'pause' || m.kind === 'out') && Math.abs(m.frame - frame) < 1e-3);
  const holdSince = useMemo(() => (atHold ? clientNow() : undefined), [atHold, frame]);
  const layerAge = useCallback((l: Layer, age: number, now: number) => (widgetRun[l.id] ? runAge(widgetRun[l.id], now) : age), [widgetRun]);
  const clocked = useCallback((l: Layer) => !!widgetRun[l.id], [widgetRun]);
  const stateOf = useCallback((id: string) => widgetState[id], [widgetState]);
  // Snapping targets, read by the drag handler.
  const [guides] = useProjectGuides(_project);
  const view = useGuidesView();
  const zonesEdit = useZoneEdit((s) => s.on);
  // The other templates shown faint under this one.
  const ghostPick = useGhosts();
  const zoneById = new Map(_project.zones.map((z) => [z.id, z]));
  const ghostTemplates = template && ghostPick.on ? designTemplates(_project).filter((tpl) => tpl.id !== template.id && (ghostPick.ids === null || ghostPick.ids.includes(tpl.id))) : [];
  const ghostOn = ghostTemplates.length > 0;
  const ghostOpacity = ghostPick.opacity;
  const snapRef = useRef({ on: view.snap, targets: snapTargets(guides, project.width, project.height, view.safe) });
  snapRef.current = { on: view.snap, targets: snapTargets(guides, project.width, project.height, view.safe) };

  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      // Its padding and room for the rulers.
      const pad = 72;
      useDesigner.setState({ stageFit: Math.max(0.05, Math.min((el.clientWidth - pad) / project.width, (el.clientHeight - pad) / project.height)) });
    });
    ro.observe(el);
    return () => ro.disconnect();
    // The overview and a template have their own stage element.
  }, [project.width, project.height, !!template]);

  // Ctrl / ⌘ + wheel (a pinch on a trackpad): zoom around the pointer.
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const st = useDesigner.getState();
      const was = st.stageZoom ?? st.stageFit;
      const next = clampZoom(was * Math.exp(-e.deltaY * 0.0025));
      const r = el.getBoundingClientRect();
      const px = e.clientX - r.left;
      const py = e.clientY - r.top;
      const ax = (el.scrollLeft + px) / was;
      const ay = (el.scrollTop + py) / was;
      useDesigner.setState({ stageZoom: next });
      requestAnimationFrame(() => {
        el.scrollLeft = ax * next - px;
        el.scrollTop = ay * next - py;
      });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [!!template]);

  // Space held: the hand. A drag moves the zoomed canvas (no layer is picked).
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    let held = false;
    const typing = (e: Event) => !!(e.target as HTMLElement | null)?.closest?.('input, textarea, select, [contenteditable="true"]');
    const set = (on: boolean) => {
      held = on;
      el.classList.toggle('hand', on);
    };
    const down = (e: KeyboardEvent) => e.key === ' ' && !typing(e) && !document.querySelector('.logic-editor, .modal-back') && set(true);
    const up = (e: KeyboardEvent) => e.key === ' ' && set(false);
    const blur = () => set(false);
    const grab = (e: PointerEvent) => {
      if (!held || e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      const from = { x: e.clientX, y: e.clientY, l: el.scrollLeft, t: el.scrollTop };
      el.classList.add('grabbing');
      const move = (ev: PointerEvent) => {
        el.scrollLeft = from.l - (ev.clientX - from.x);
        el.scrollTop = from.t - (ev.clientY - from.y);
      };
      const stop = () => {
        el.classList.remove('grabbing');
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', stop);
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', stop);
    };
    window.addEventListener('keydown', down);
    window.addEventListener('keyup', up);
    window.addEventListener('blur', blur);
    el.addEventListener('pointerdown', grab, { capture: true });
    return () => {
      window.removeEventListener('keydown', down);
      window.removeEventListener('keyup', up);
      window.removeEventListener('blur', blur);
      el.removeEventListener('pointerdown', grab, { capture: true });
    };
  }, [!!template]);

  // Selection boxes come from the DOM so they follow nested transforms.
  useLayoutEffect(() => {
    const root = sceneRef.current;
    if (!root) return;
    const origin = root.getBoundingClientRect();
    const box = (id: string, el: Element): Box => {
      const r = el.getBoundingClientRect();
      return { id, left: r.left - origin.left, top: r.top - origin.top, width: r.width, height: r.height };
    };
    // Kept when nothing moved (a playback where these layers stand still): the stage is not drawn twice per frame.
    setGhosts((was) =>
      same(
        was,
        [...root.querySelectorAll<HTMLElement>('[data-layer-id]')]
          .filter((el) => !selected.includes(el.dataset.layerId!))
          .map((el) => box(el.dataset.layerId!, el))
          .filter((b) => b.width > 0 && b.height > 0),
      ),
    );
    setBoxes((was) =>
      same(
        was,
        selected.flatMap((id) => {
          const el = root.querySelector<HTMLElement>(`[data-layer-id="${CSS.escape(id)}"]`);
          if (!el) return [];
          const r = el.getBoundingClientRect();
          return [{ id, left: r.left - origin.left, top: r.top - origin.top, width: r.width, height: r.height }];
        }),
      ),
    );
  }, [selected, template, frame, scale, previewData]);

  // Zoomed by hand, the canvas scrolls over the picture and the widgets out
  // of it, no further (Auto fits the picture: no scroll).
  const W = project.width * scale;
  const H = project.height * scale;
  const all = [...ghosts, ...boxes];
  const out = (v: number) => (v > 0.5 ? Math.ceil(v) + 24 : 0);
  const room =
    stageZoom === null
      ? { l: 0, t: 0, r: 0, b: 0 }
      : {
          l: out(-Math.min(0, ...all.map((b) => b.left))),
          t: out(-Math.min(0, ...all.map((b) => b.top))),
          r: out(Math.max(W, ...all.map((b) => b.left + b.width)) - W),
          b: out(Math.max(H, ...all.map((b) => b.top + b.height)) - H),
        };
  // More room above or left: the picture stays where it was on screen.
  const roomWas = useRef(room);
  useLayoutEffect(() => {
    const el = wrapRef.current;
    const was = roomWas.current;
    roomWas.current = room;
    if (!el) return;
    if (room.l !== was.l) el.scrollLeft += room.l - was.l;
    if (room.t !== was.t) el.scrollTop += room.t - was.t;
  }, [room.l, room.t]); // eslint-disable-line react-hooks/exhaustive-deps
  // The part of the canvas in view (picture pixels from its top left): the
  // grey outside the picture covers it, no more (else it would scroll).
  const [vis, setVis] = useState({ x0: 0, y0: 0, x1: 0, y1: 0 });
  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    const sc = sceneRef.current;
    if (!wrap || !sc) return;
    const update = () => {
      const w = wrap.getBoundingClientRect();
      const o = sc.getBoundingClientRect();
      const x0 = Math.ceil(w.left + wrap.clientLeft - o.left);
      const y0 = Math.ceil(w.top + wrap.clientTop - o.top);
      let next = { x0, y0, x1: x0 + Math.floor(wrap.clientWidth) - 1, y1: y0 + Math.floor(wrap.clientHeight) - 1 };
      // Zoomed by hand: no further than the picture and its widgets (the
      // canvas would scroll over the grey itself, kept as wide as it was).
      if (stageZoom !== null) {
        const m = 28;
        next = { x0: Math.max(next.x0, -room.l - m), y0: Math.max(next.y0, -room.t - m), x1: Math.min(next.x1, W + room.r + m), y1: Math.min(next.y1, H + room.b + m) };
      }
      setVis((v) => (v.x0 === next.x0 && v.y0 === next.y0 && v.x1 === next.x1 && v.y1 === next.y1 ? v : next));
    };
    update();
    wrap.addEventListener('scroll', update, { passive: true });
    const ro = new ResizeObserver(update);
    ro.observe(wrap);
    return () => {
      wrap.removeEventListener('scroll', update);
      ro.disconnect();
    };
  }, [scale, stageZoom === null, room.l, room.t, room.r, room.b, !!template]); // eslint-disable-line react-hooks/exhaustive-deps

  const drag = useRef<{
    mode: 'move' | Handle;
    startX: number;
    startY: number;
    start: Record<string, { x: number; y: number; w: number; h: number; ox: number; oy: number }>;
    /** The other layers' edges and centres (stage pixels), read when the drag began. */
    objs: { x: SnapLine[]; y: SnapLine[] };
  } | null>(null);
  // The smart guides of the drag going on.
  const [smart, setSmart] = useState<Shown[]>([]);

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const d = drag.current;
      const st = useDesigner.getState();
      if (!d || !st.template) return;
      let dx = (e.clientX - d.startX) / scale;
      let dy = (e.clientY - d.startY) / scale;
      // Shift while moving: one axis only, the one moved the most.
      const axis = d.mode === 'move' && e.shiftKey ? (Math.abs(dx) >= Math.abs(dy) ? 'x' : 'y') : null;
      const snap = e.shiftKey && d.mode !== 'move' ? 10 : 1;
      const r = (v: number) => Math.round(v / snap) * snap;
      // Snap the box's edges (and centre when moving) to the guides, the picture's edges, the areas and the
      // other layers' edges and centres (side by side, aligned, centred), showing the lines it snaps to.
      const starts = Object.values(d.start);
      const lines: Shown[] = [];
      if (starts.length && snapRef.current.on && !e.altKey) {
        const { x: gx, y: gy } = snapRef.current.targets;
        const tx: SnapLine[] = [...gx.map((v) => ({ v })), ...d.objs.x];
        const ty: SnapLine[] = [...gy.map((v) => ({ v })), ...d.objs.y];
        const th = 6 / scale;
        // The selection's box on the stage (in a group, x / y are from the group).
        const bx = Math.min(...starts.map((s) => s.ox + s.x));
        const by = Math.min(...starts.map((s) => s.oy + s.y));
        const bw = Math.max(...starts.map((s) => s.ox + s.x + s.w)) - bx;
        const bh = Math.max(...starts.map((s) => s.oy + s.y + s.h)) - by;
        const nx = bx + dx;
        const ny = by + dy;
        let ex: number[];
        let ey: number[];
        if (d.mode === 'move') {
          ex = [nx, nx + bw / 2, nx + bw];
          ey = [ny, ny + bh / 2, ny + bh];
        } else {
          ex = d.mode.includes('e') ? [nx + bw] : d.mode.includes('w') ? [nx] : [];
          ey = d.mode.includes('s') ? [ny + bh] : d.mode.includes('n') ? [ny] : [];
        }
        const sx = nearest(ex, tx, th);
        const sy = nearest(ey, ty, th);
        dx += sx;
        dy += sy;
        // The lines now met, drawn from the box to the layer it lines up with (or across the picture).
        const box = { l: bx + dx, t: by + dy, r: bx + dx + bw, b: by + dy + bh };
        if (d.mode !== 'move') {
          if (d.mode.includes('e')) box.r = bx + bw + dx;
          if (d.mode.includes('w')) box.l = bx + dx;
          if (d.mode.includes('s')) box.b = by + bh + dy;
          if (d.mode.includes('n')) box.t = by + dy;
          if (!/[ew]/.test(d.mode)) (box.l = bx), (box.r = bx + bw);
          if (!/[ns]/.test(d.mode)) (box.t = by), (box.b = by + bh);
        }
        const meet = (axis: 'x' | 'y', edges: number[], targets: SnapLine[], lo: number, hi: number, size: number) => {
          for (const tl of targets) {
            if (!edges.some((e) => Math.abs(e - tl.v) < 0.75)) continue;
            const from = tl.from === undefined ? 0 : Math.min(lo, tl.from);
            const to = tl.to === undefined ? size : Math.max(hi, tl.to);
            const same = lines.find((l) => l.axis === axis && Math.abs(l.v - tl.v) < 0.75);
            if (same) (same.from = Math.min(same.from, from)), (same.to = Math.max(same.to, to));
            else lines.push({ axis, v: tl.v, from, to });
          }
        };
        if (sx !== 0 || ex.length) meet('x', d.mode === 'move' ? [box.l, (box.l + box.r) / 2, box.r] : ex.map((v) => v + sx), tx, box.t, box.b, project.height);
        if (sy !== 0 || ey.length) meet('y', d.mode === 'move' ? [box.t, (box.t + box.b) / 2, box.b] : ey.map((v) => v + sy), ty, box.l, box.r, project.width);
      }
      setSmart((was) => (was.length || lines.length ? lines : was));
      if (axis === 'x') dy = 0;
      if (axis === 'y') dx = 0;
      st.update((t) => {
        for (const [id, s] of Object.entries(d.start)) {
          const l = findLayer(t, id);
          if (!l || l.locked) continue;
          if (d.mode === 'move') {
            setProp(l, 'x', r(s.x + dx), st.frame);
            setProp(l, 'y', r(s.y + dy), st.frame);
            continue;
          }
          let { x, y, w, h } = s;
          if (d.mode.includes('e')) w = Math.max(1, r(s.w + dx));
          if (d.mode.includes('s')) h = Math.max(1, r(s.h + dy));
          if (d.mode.includes('w')) {
            w = Math.max(1, r(s.w - dx));
            x = s.x + (s.w - w);
          }
          if (d.mode.includes('n')) {
            h = Math.max(1, r(s.h - dy));
            y = s.y + (s.h - h);
          }
          // Width / height locked: the ratio stays (a corner follows the side moved the most).
          if (l.lockRatio && s.w > 0 && s.h > 0) {
            const k = s.w / s.h;
            const horiz = /[ew]/.test(d.mode);
            const vert = /[ns]/.test(d.mode);
            if (horiz && (!vert || Math.abs(w - s.w) / s.w >= Math.abs(h - s.h) / s.h)) h = Math.max(1, Math.round(w / k));
            else w = Math.max(1, Math.round(h * k));
            if (d.mode.includes('w')) x = s.x + (s.w - w);
            if (d.mode.includes('n')) y = s.y + (s.h - h);
          }
          if (x !== s.x) setProp(l, 'x', x, st.frame);
          if (y !== s.y) setProp(l, 'y', y, st.frame);
          if (w !== s.w) setProp(l, 'width', w, st.frame);
          if (h !== s.h) setProp(l, 'height', h, st.frame);
        }
      }, 'stage-drag');
    };
    const onUp = () => {
      drag.current = null;
      setSmart([]);
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
  }, [scale]);

  const beginDrag = (e: React.PointerEvent, mode: 'move' | Handle, ids: string[]) => {
    const st = useDesigner.getState();
    if (!st.template) return;
    const start: Record<string, { x: number; y: number; w: number; h: number; ox: number; oy: number }> = {};
    for (const id of ids) {
      const l = findLayer(st.template, id);
      if (!l) continue;
      // Where its group is on the stage.
      let ox = 0;
      let oy = 0;
      const seen = new Set<string>();
      for (let p = l.parent ? findLayer(st.template, l.parent) : undefined; p && !seen.has(p.id); p = p.parent ? findLayer(st.template, p.parent) : undefined) {
        seen.add(p.id);
        ox += num(propValue(p, 'x', st.frame));
        oy += num(propValue(p, 'y', st.frame));
      }
      start[id] = {
        ox,
        oy,
        x: num(propValue(l, 'x', st.frame)),
        y: num(propValue(l, 'y', st.frame)),
        w: num(propValue(l, 'width', st.frame)),
        h: num(propValue(l, 'height', st.frame)),
      };
    }
    // The other layers (not the ones dragged, nor inside them): their edges and centres.
    const objs: { x: SnapLine[]; y: SnapLine[] } = { x: [], y: [] };
    const root = sceneRef.current;
    if (root) {
      const origin = root.getBoundingClientRect();
      const moving = ids.map((id) => root.querySelector<HTMLElement>(`[data-layer-id="${CSS.escape(id)}"]`)).filter((el): el is HTMLElement => !!el);
      for (const el of root.querySelectorAll<HTMLElement>('[data-layer-id]')) {
        if (moving.some((m) => m === el || m.contains(el) || el.contains(m))) continue;
        const r = el.getBoundingClientRect();
        if (r.width <= 0 || r.height <= 0) continue;
        const l = (r.left - origin.left) / scale;
        const t = (r.top - origin.top) / scale;
        const w = r.width / scale;
        const h = r.height / scale;
        for (const v of [l, l + w / 2, l + w]) objs.x.push({ v, from: t, to: t + h });
        for (const v of [t, t + h / 2, t + h]) objs.y.push({ v, from: l, to: l + w });
      }
    }
    drag.current = { mode, startX: e.clientX, startY: e.clientY, start, objs };
    useDesigner.setState({ lastCoalesce: null });
    e.stopPropagation();
  };

  // The layer under the pointer: the one drawn there, else one whose box is there but which its group's
  // mask cuts (not drawn outside it, so not under the pointer for the page).
  const layerAt = (e: { target: EventTarget; clientX: number; clientY: number }) => {
    const hit = (e.target as HTMLElement).closest<HTMLElement>('[data-layer-id]');
    if (hit) return hit.dataset.layerId;
    const els = [...(sceneRef.current?.querySelectorAll<HTMLElement>('[data-layer-id]') ?? [])];
    for (let i = els.length - 1; i >= 0; i--) {
      const r = els[i].getBoundingClientRect();
      if (r.width > 0 && r.height > 0 && e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom) return els[i].dataset.layerId;
    }
    return undefined;
  };

  const onPointerDown = (e: React.PointerEvent) => {
    const st = useDesigner.getState();
    const id = layerAt(e);
    if (!id) {
      st.select([]);
      return;
    }
    const l = st.template && findLayer(st.template, id);
    // Right button: the layer is selected for its menu (onContextMenu), not dragged.
    if (e.button === 2) {
      if (!st.selected.includes(id)) st.select([id]);
      return;
    }
    if (l?.locked) return;
    // Pressed beside the picture: no text of the page gets selected while dragging.
    e.preventDefault();
    let sel = st.selected;
    if (e.shiftKey) sel = sel.includes(id) ? sel.filter((s) => s !== id) : [...sel, id];
    else if (!sel.includes(id)) sel = [id];
    st.select(sel);
    beginDrag(e, 'move', sel);
  };

  // A double-click on a Table opens its editor.
  const onDoubleClick = (e: React.MouseEvent) => {
    const id = layerAt(e);
    const st = useDesigner.getState();
    const l = id && st.template ? findLayer(st.template, id) : undefined;
    if (l?.type === 'table' && !l.locked) useTableEditor.getState().open(l.id);
  };

  // Program, fill or key, for the whole stage (canvas toolbar).
  const shown = stageViewStyle(view.stageView, background);
  if (!template) return <Overview project={_project} size={project} format={format} scale={scale} background={shown.background} bgInput={bgInput} filter={shown.filter} showZones={showZones} wrapRef={wrapRef} ticking={running ? 'all' : false} fps={fps} />;

  return (
    // Around the picture too: a layer dragged out of it is still picked there (or nothing: deselects).
    <div className="stage-wrap" ref={wrapRef} onPointerDown={onPointerDown}>
      <div className={`stage-room ${stageZoom === null ? 'fit' : ''}`} style={{ padding: `${room.t}px ${room.r}px ${room.b}px ${room.l}px` }}>
        <div className={`stage-frame bg-${shown.background}`} onPointerDown={(e) => e.stopPropagation()}>
          {shown.background === 'video' && <StageVideo input={bgInput} />}
          <div
            ref={sceneRef}
            onPointerDown={onPointerDown}
            onDoubleClick={onDoubleClick}
            onContextMenu={(e) => {
              const id = layerAt(e);
              if (!id) return;
              e.preventDefault();
              const st = useDesigner.getState();
              if (!st.selected.includes(id)) st.select([id]);
              setCtx({ x: e.clientX, y: e.clientY, layerId: id });
            }}
            style={{ position: 'relative' }}
          >
            <Viewport width={project.width} height={project.height} scale={scale} style={{ overflow: 'visible' }}>
              {/* Widgets run on the playhead: scrub to see a ticker or a counter move. */}
              <LiveTime ageSecs={frame / fps} since={holdSince} ticking={ticking} layerAge={layerAge} clocked={clocked} widgetState={stateOf} fps={fps} lang={_project.language || 'fr'}>
                {/* The other templates picked (Display › Other templates), faint, under it: to arrange it against them. */}
                {!zonesEdit && ghostOn && (
                  <div className="stage-ghosts" style={{ position: 'absolute', inset: 0, opacity: ghostOpacity, pointerEvents: 'none', filter: shown.filter }} aria-hidden>
                    {ghostTemplates.map((tpl) => {
                      const z = zoneById.get(tpl.zone);
                      return (
                        <div key={tpl.id} style={{ position: 'absolute', inset: 0, ...areaClip(z && zoneArea(z, format, project.height > project.width)) }}>
                          <OverviewScene template={tpl} format={format} ticking={false} fps={fps} lang={_project.language || 'fr'} />
                        </div>
                      );
                    })}
                  </div>
                )}
                {/* While the zones are edited: the zones alone, no graphic. */}
                {!zonesEdit && (
                  <div style={{ position: 'absolute', inset: 0, filter: shown.filter }}>
                    <Scene template={template} frame={frame} data={data} interactive />
                  </div>
                )}
              </LiveTime>
            </Viewport>
            {/* What is outside the picture still shows, greyed (it is cut on air). */}
            <div className="stage-outside" aria-hidden>
              <i style={{ left: vis.x0, top: vis.y0, width: Math.max(0, vis.x1 - vis.x0), height: Math.max(0, -vis.y0) }} />
              <i style={{ left: vis.x0, top: H, width: Math.max(0, vis.x1 - vis.x0), height: Math.max(0, vis.y1 - H) }} />
              <i style={{ left: vis.x0, top: Math.max(0, vis.y0), width: Math.max(0, -vis.x0), height: Math.max(0, Math.min(H, vis.y1) - Math.max(0, vis.y0)) }} />
              <i style={{ left: W, top: Math.max(0, vis.y0), width: Math.max(0, vis.x1 - W), height: Math.max(0, Math.min(H, vis.y1) - Math.max(0, vis.y0)) }} />
            </div>
            <StageGuides project={_project} width={project.width} height={project.height} scale={scale} frameRef={sceneRef} />
            {zonesEdit ? <ZonesEditOverlay format={format} width={project.width} height={project.height} scale={scale} /> : showZones && <ZoneOverlay zones={_project.zones ?? []} only={template.zone} format={format} width={project.width} height={project.height} scale={scale} current={template.zone} />}
            {view.frames && !zonesEdit && ghosts.map((b) => (
              <div key={`g${b.id}`} className="ghost-box" style={{ left: b.left, top: b.top, width: b.width, height: b.height }} />
            ))}
            {smart.map((g) => (
              <div key={`${g.axis}${g.v}`} className={`smart-guide ${g.axis}`} style={g.axis === 'x' ? { left: g.v * scale, top: g.from * scale, height: (g.to - g.from) * scale } : { top: g.v * scale, left: g.from * scale, width: (g.to - g.from) * scale }} />
            ))}
            {!zonesEdit && boxes.map((b) => (
              <div key={b.id} className="sel-box" style={{ left: b.left, top: b.top, width: b.width, height: b.height }}>
                {selected.length === 1 && HANDLES.map((h) => <div key={h} className={`handle h-${h}`} onPointerDown={(e) => beginDrag(e, h, [b.id])} />)}
                {selected.length === 1 && <WidgetOverlay layerId={b.id} box={b} frame={frame} />}
              </div>
            ))}
          </div>
        </div>
      </div>
      <div className="stage-info">
        {project.width}×{project.height} · {Math.round(scale * 100)}%
      </div>
      {/* Its clicks do not reach the stage (which would drop the selection). */}
      {ctx && (
        <div onPointerDown={(e) => e.stopPropagation()}>
          <ClipboardMenu at={ctx} onClose={() => setCtx(null)} />
        </div>
      )}
    </div>
  );
}

/** Handles of a spec widget (web/src/widgets) over its selection box. */
function WidgetOverlay({ layerId, box, frame }: { layerId: string; box: Box; frame: number }) {
  const { template, update } = useDesigner(useShallow((s) => ({ template: s.template, update: s.update })));
  const layer = template && findLayer(template, layerId);
  const Ov = layer && widgetOf(layer)?.Overlay;
  if (!layer || !Ov) return null;
  const w = num(propValue(layer, 'width', frame));
  const h = num(propValue(layer, 'height', frame));
  const edit = (fn: (l: never) => void, key?: string) => update((t) => fn(findLayer(t, layerId)! as never), key);
  return (
    <div className="wk-overlay">
      <Ov layer={layer as never} w={w} h={h} scale={w > 0 ? box.width / w : 1} edit={edit} />
    </div>
  );
}

/**
 * No template open: every template of the project at its on-air point,
 * each cut to its zone, over the zones. Clicking a graphic opens its
 * template; Esc in a template comes back here.
 */
function Overview({
  project,
  size,
  format,
  scale,
  background,
  bgInput,
  filter,
  showZones,
  wrapRef,
  ticking,
  fps,
}: {
  project: Project;
  size: { width: number; height: number };
  format: string;
  scale: number;
  background: string;
  bgInput: string;
  filter?: string;
  showZones: boolean;
  wrapRef: React.RefObject<HTMLDivElement | null>;
  ticking: Ticking;
  fps: number;
}) {
  const load = useDesigner((s) => s.load);
  const zonesEdit = useZoneEdit((s) => s.on);
  const sceneRef = useRef<HTMLDivElement>(null);
  const portrait = size.height > size.width;
  const zones = new Map(project.zones.map((z) => [z.id, z]));
  const templates = [...designTemplates(project)].sort((a, b) => (zones.get(a.zone)?.layer ?? 0) - (zones.get(b.zone)?.layer ?? 0));
  return (
    <div className="stage-wrap" ref={wrapRef}>
      <div className={`stage-frame bg-${background}`}>
        {background === 'video' && <StageVideo input={bgInput} />}
        <div
          ref={sceneRef}
          className="overview"
          style={{ position: 'relative' }}
          onPointerDown={(e) => {
            const id = (e.target as HTMLElement).closest<HTMLElement>('[data-template-id]')?.dataset.templateId;
            const t = id && templates.find((x) => x.id === id);
            if (t) load(t);
          }}
        >
          <Viewport width={size.width} height={size.height} scale={scale} style={{ filter }}>
            {(zonesEdit ? [] : templates).map((tpl) => {
              const z = zones.get(tpl.zone);
              return (
                <div key={tpl.id} className="overview-tpl" data-template-id={tpl.id} title={t('{name} — click to open', { name: tpl.name })} style={{ position: 'absolute', inset: 0, ...areaClip(z && zoneArea(z, format, portrait)) }}>
                  <OverviewScene template={tpl} format={format} ticking={ticking} fps={fps} lang={project.language || 'fr'} />
                </div>
              );
            })}
          </Viewport>
          <StageGuides project={project} width={size.width} height={size.height} scale={scale} frameRef={sceneRef} />
          {zonesEdit ? <ZonesEditOverlay format={format} width={size.width} height={size.height} scale={scale} /> : showZones && <ZoneOverlay zones={project.zones ?? []} format={format} width={size.width} height={size.height} scale={scale} current="" />}
        </div>
      </div>
      <div className="stage-info">
        {tn(templates.length, '{n} template', '{n} templates')} · {size.width}×{size.height} · {Math.round(scale * 100)}%
      </div>
    </div>
  );
}

/**
 * One template of the overview, drawn as the open template draws it: its
 * default values, bound fields and data sets read like on air, its settings.
 */
function OverviewScene({ template, format, ticking, fps, lang }: { template: Template; format: string; ticking: Ticking; fps: number; lang?: string }) {
  const defaults = useMemo(() => defaultData(template), [template]);
  const bound = useBoundData(template, defaults);
  const data = useMemo(() => ({ ...bound, ...settingDefaults(template) }), [bound, template]);
  const shown = useMemo(() => inFormat(template, format), [template, format]);
  const hold = holdFrame(template);
  return (
    <LiveTime ageSecs={hold / fps} ticking={ticking} fps={fps} lang={lang}>
      <Scene template={shown} frame={hold} data={data} interactive />
    </LiveTime>
  );
}

/** A live video input behind the stage, filling it. */
function StageVideo({ input }: { input: string }) {
  const { url } = useLiveFrame(input, 'preview');
  return url ? <img className="stage-video" src={url} alt="" draggable={false} /> : null;
}

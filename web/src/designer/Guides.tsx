import { Eye, Move, Plus, Trash2 } from 'lucide-react';
import { memo, useEffect, useRef, useState } from 'react';
import { create } from 'zustand';
import { newId } from '../engine/evaluate';
import type { Area, Guides, Project } from '../engine/types';
import { api, useApp } from '../store';
import { t } from '../i18n';
import { displayPref, setDisplayPref, useDisplayPrefs } from '../displayPrefs';

/*
 * Designer aids: rulers, guides (dragged out of the rulers, back onto them
 * to remove), snapping, safe areas and areas to keep clear (a channel bug,
 * a sign language box). Guides and areas belong to the project, as
 * fractions of the picture so they hold in every format; what is shown
 * is a per-browser choice.
 */

export const DEFAULT_GUIDES: Guides = { vertical: [], horizontal: [], actionSafe: 0.05, titleSafe: 0.1, exclusions: [] };

const VIEW_KEY = 'designer.view';
type View = {
  safe: boolean;
  guides: boolean;
  snap: boolean;
  /** Safe areas and areas to keep clear moved on the stage (not kept). */
  editSafe?: boolean;
  /** The area selected while editing ("action", "title" or an exclusion id). */
  area?: string | null;
  /** What the stage shows: the picture (program), the fill over black, or the key (alpha, white on black). */
  stageView?: StageView;
  /** The faint frame of each widget not selected. */
  frames: boolean;
};

export type StageView = 'program' | 'fill' | 'key';
export const STAGE_VIEWS: { id: StageView; label: string }[] = [
  { id: 'program', label: 'Program' },
  { id: 'fill', label: 'Fill' },
  { id: 'key', label: 'Key' },
];

/** The stage's background and the filter of its scene for a view (fill: over black; key: the alpha in white on black). */
export function stageViewStyle(view: StageView | undefined, background: string): { background: string; filter?: string } {
  if (!view || view === 'program') return { background };
  return { background: 'black', filter: view === 'key' ? 'brightness(0) invert(1)' : undefined };
}
const loadView = (): View => ({ safe: true, guides: true, snap: true, stageView: 'program', frames: true, ...displayPref<Partial<View>>(VIEW_KEY, {}), editSafe: false });
export const useGuidesView = create<View & { set: (v: Partial<View>) => void }>((set, get) => ({
  ...loadView(),
  set(v) {
    set(v);
    if (v.editSafe && !get().safe) set({ safe: true });
    const { safe, guides, snap, stageView, frames } = get();
    setDisplayPref(VIEW_KEY, { safe, guides, snap, stageView, frames });
  },
}));
// Another user logs in: their choices.
useDisplayPrefs.subscribe((s, before) => s.user !== before.user && useGuidesView.setState(loadView()));

/** Guides being edited, shared by the stage and the menu until saved. */
const useDraft = create<{ id: string | null; g: Guides | null }>(() => ({ id: null, g: null }));
let pending: ReturnType<typeof setTimeout> | undefined;

/** The project's guides, edited locally and saved a moment later. */
export function useProjectGuides(project: Project): [Guides, (g: Guides) => void] {
  const toast = useApp((s) => s.toast);
  const draft = useDraft();
  const saved = JSON.stringify(project.guides ?? DEFAULT_GUIDES);
  useEffect(() => {
    if (!pending) useDraft.setState({ id: project.id, g: JSON.parse(saved) });
  }, [saved, project.id]);
  const g = draft.id === project.id && draft.g ? draft.g : (JSON.parse(saved) as Guides);
  const update = (next: Guides) => {
    useDraft.setState({ id: project.id, g: next });
    clearTimeout(pending);
    pending = setTimeout(() => {
      pending = undefined;
      api('PUT', `/api/projects/${encodeURIComponent(project.id)}/guides`, next).catch((e) => toast((e as Error).message, 'error'));
    }, 400);
  };
  return [g, update];
}

/**
 * Snapping: the smallest shift that brings one of `edges` (px) onto one
 * of `targets` (px) within `threshold`, or 0.
 */
export function snapShift(edges: number[], targets: number[], threshold: number): number {
  let best = 0;
  let bestD = threshold;
  for (const e of edges)
    for (const t of targets) {
      const d = Math.abs(t - e);
      if (d < bestD) {
        bestD = d;
        best = t - e;
      }
    }
  return best;
}

/** What zones and areas snap to, in fractions of the frame; null when snapping is off. */
export function useAreaSnap(): { x: number[]; y: number[] } | null {
  const view = useGuidesView();
  const project = useApp((s) => s.project);
  const draft = useDraft();
  if (!view.snap || !project) return null;
  const g = draft.id === project.id && draft.g ? draft.g : (project.guides ?? DEFAULT_GUIDES);
  return snapTargets(g, 1, 1, view.safe);
}

export type Handle = 'move' | 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';

/**
 * An area moved or resized from `start` by (dx, dy), in fractions: in
 * `step`s, at least `min` wide, inside the frame, and with its edges (or
 * centre, when moved) on the nearest snap target within `near`.
 */
export function dragArea(start: Area, mode: Handle, dx: number, dy: number, o: { step: number; min: number; snap: { x: number[]; y: number[] } | null; near: { x: number; y: number } }): Area {
  const r = (v: number) => Math.round(v / o.step) * o.step;
  // Not to its own edges where it was.
  const own = (v: number[], a: number, b: number) => v.filter((t) => Math.abs(t - a) > 1e-6 && Math.abs(t - b) > 1e-6);
  const tx = o.snap ? own(o.snap.x, start.x, start.x + start.width) : [];
  const ty = o.snap ? own(o.snap.y, start.y, start.y + start.height) : [];
  const sx = (edges: number[]) => snapShift(edges, tx, o.near.x);
  const sy = (edges: number[]) => snapShift(edges, ty, o.near.y);
  let { x, y, width: w, height: h } = start;
  if (mode === 'move') {
    x = r(x + dx);
    y = r(y + dy);
    x += sx([x, x + w / 2, x + w]);
    y += sy([y, y + h / 2, y + h]);
    return { x: Math.min(1 - w, Math.max(0, x)), y: Math.min(1 - h, Math.max(0, y)), width: w, height: h };
  }
  if (mode.includes('e')) {
    let right = r(x + w + dx);
    right += sx([right]);
    w = Math.min(1 - x, Math.max(o.min, right - x));
  }
  if (mode.includes('s')) {
    let bottom = r(y + h + dy);
    bottom += sy([bottom]);
    h = Math.min(1 - y, Math.max(o.min, bottom - y));
  }
  if (mode.includes('w')) {
    let nx = r(x + dx);
    nx += sx([nx]);
    nx = Math.max(0, Math.min(x + w - o.min, nx));
    w += x - nx;
    x = nx;
  }
  if (mode.includes('n')) {
    let ny = r(y + dy);
    ny += sy([ny]);
    ny = Math.max(0, Math.min(y + h - o.min, ny));
    h += y - ny;
    y = ny;
  }
  return { x, y, width: w, height: h };
}

/** What layers snap to, in px: the frame, its centre, the guides, safe areas and areas to keep clear. */
export function snapTargets(g: Guides, w: number, h: number, safe: boolean): { x: number[]; y: number[] } {
  const x = [0, w / 2, w, ...g.vertical.map((v) => v * w)];
  const y = [0, h / 2, h, ...g.horizontal.map((v) => v * h)];
  if (safe)
    for (const a of safeRects(g)) {
      x.push(a.x * w, (a.x + a.width) * w);
      y.push(a.y * h, (a.y + a.height) * h);
    }
  for (const e of g.exclusions) {
    x.push(e.x * w, (e.x + e.width) * w);
    y.push(e.y * h, (e.y + e.height) * h);
  }
  return { x, y };
}

const margin = (m: number): Area => ({ x: m, y: m, width: 1 - 2 * m, height: 1 - 2 * m });

/** Action safe and title safe: placed by hand, or from the margins. */
export function safeRects(g: Guides): [Area, Area] {
  return [g.actionArea ?? margin(g.actionSafe), g.titleArea ?? margin(g.titleSafe)];
}

const STEPS = [10, 20, 25, 50, 100, 200, 250, 500, 1000];

/** A ruler along the top (x) or the left (y) of the stage. */
function RulerBar({ axis, length, scale, onGuide }: { axis: 'x' | 'y'; length: number; scale: number; onGuide: (e: React.PointerEvent) => void }) {
  const step = STEPS.find((s) => s * scale >= 60) ?? 1000;
  const ticks = [];
  for (let v = 0; v <= length; v += step) ticks.push(v);
  return (
    <div className={`ruler ruler-${axis}`} style={axis === 'x' ? { width: length * scale } : { height: length * scale }} onPointerDown={onGuide} title={t('Drag out a guide')}>
      {ticks.map((v) => (
        <span key={v} className="ruler-tick" style={axis === 'x' ? { left: v * scale } : { top: v * scale }}>
          {v}
        </span>
      ))}
    </div>
  );
}

/**
 * Rulers, guides, safe areas and areas to keep clear over the stage.
 * `frameRef` is the stage frame the positions are relative to.
 */
// Memoised: not drawn again on each frame of the playback (its props stay).
export const StageGuides = memo(function StageGuides({ project, width, height, scale, frameRef }: { project: Project; width: number; height: number; scale: number; frameRef: React.RefObject<HTMLDivElement | null> }) {
  const view = useGuidesView();
  const [g, setG] = useProjectGuides(project);

  /** Drags guide `i` of `axis` (a new one when i = -1); out of the picture removes it. */
  const dragGuide = (e: React.PointerEvent, axis: 'vertical' | 'horizontal', index: number) => {
    e.preventDefault();
    e.stopPropagation();
    const frame = frameRef.current;
    if (!frame) return;
    const list = [...g[axis]];
    let i = index;
    if (i < 0) {
      list.push(-1);
      i = list.length - 1;
    }
    const at = (ev: PointerEvent | React.PointerEvent) => {
      const r = frame.getBoundingClientRect();
      return axis === 'vertical' ? (ev.clientX - r.left) / r.width : (ev.clientY - r.top) / r.height;
    };
    let current = { ...g, [axis]: list };
    const move = (ev: PointerEvent) => {
      const f = at(ev);
      const px = axis === 'vertical' ? width : height;
      // Whole pixels; Shift snaps to 10 px.
      const snap = ev.shiftKey ? 10 : 1;
      const v = Math.round((f * px) / snap) * snap;
      const next = [...current[axis]];
      next[i] = v / px;
      current = { ...current, [axis]: next };
      setG(current);
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      const f = at(ev);
      if (f < 0 || f > 1) setG({ ...current, [axis]: current[axis].filter((_, k) => k !== i) });
      document.body.classList.remove('dragging-guide');
    };
    document.body.classList.add('dragging-guide');
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  return (
    <>
      {view.safe && !view.editSafe && (
        <>
          {safeRects(g).map((a, i) => (
            <div key={i} className={`safe-area ${i ? 'title' : ''}`} style={pct(a)} title={i ? t('Title safe') : t('Action safe')} />
          ))}
          {g.exclusions.map((x) => (
            <div key={x.id} className="exclusion" style={pct(x)}>
              <span>{x.name}</span>
            </div>
          ))}
        </>
      )}
      {view.editSafe && (
        <RectEditor
          width={width * scale}
          height={height * scale}
          px={{ width, height }}
          rects={[
            { id: 'action', label: t('Action safe'), kind: 'safe', area: safeRects(g)[0] },
            { id: 'title', label: t('Title safe'), kind: 'title', area: safeRects(g)[1] },
            ...g.exclusions.map((x) => ({ id: x.id, label: x.name, kind: 'exclusion' as const, area: { x: x.x, y: x.y, width: x.width, height: x.height } })),
          ]}
          onChange={(id, a) => setG(id === 'action' ? { ...g, actionArea: a } : id === 'title' ? { ...g, titleArea: a } : { ...g, exclusions: g.exclusions.map((x) => (x.id === id ? { ...x, ...a } : x)) })}
          onDone={() => view.set({ editSafe: false })}
        />
      )}
      {view.guides && (
        <>
          {/* As in Photoshop: the top ruler gives horizontal guides, the left one vertical guides. */}
          <RulerBar axis="x" length={width} scale={scale} onGuide={(e) => dragGuide(e, 'horizontal', -1)} />
          <RulerBar axis="y" length={height} scale={scale} onGuide={(e) => dragGuide(e, 'vertical', -1)} />
          {g.vertical.map((v, i) =>
            v >= 0 && v <= 1 ? (
              <div key={`v${i}`} className="guide v" style={{ left: `${v * 100}%` }} onPointerDown={(e) => dragGuide(e, 'vertical', i)} onDoubleClick={() => setG({ ...g, vertical: g.vertical.filter((_, k) => k !== i) })} title={t('Drag to move · double-click to remove')}>
                <span>{Math.round(v * width)}</span>
              </div>
            ) : null,
          )}
          {g.horizontal.map((v, i) =>
            v >= 0 && v <= 1 ? (
              <div key={`h${i}`} className="guide h" style={{ top: `${v * 100}%` }} onPointerDown={(e) => dragGuide(e, 'horizontal', i)} onDoubleClick={() => setG({ ...g, horizontal: g.horizontal.filter((_, k) => k !== i) })} title={t('Drag to move · double-click to remove')}>
                <span>{Math.round(v * height)}</span>
              </div>
            ) : null,
          )}
        </>
      )}
    </>
  );
});

const pct = (a: Area) => ({ left: `${a.x * 100}%`, top: `${a.y * 100}%`, width: `${a.width * 100}%`, height: `${a.height * 100}%` });

const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];
const round = (v: number) => Math.round(v * 10000) / 10000;

/** Rectangles moved and resized on the stage (safe areas, areas to keep clear), like the zones. */
function RectEditor({ width, height, px, rects, onChange, onDone }: { width: number; height: number; px: { width: number; height: number }; rects: { id: string; label: string; kind: 'safe' | 'title' | 'exclusion'; area: Area }[]; onChange: (id: string, a: Area) => void; onDone: () => void }) {
  const drag = useRef<{ id: string; mode: Handle; x: number; y: number; start: Area } | null>(null);
  const snap = useAreaSnap();
  const snapRef = useRef(snap);
  snapRef.current = snap;
  const [draft, setDraft] = useState<{ id: string; area: Area } | null>(null);
  const draftRef = useRef(draft);
  useEffect(() => {
    const move = (e: PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      const area = dragArea(d.start, d.mode, (e.clientX - d.x) / width, (e.clientY - d.y) / height, {
        step: e.shiftKey ? 0.01 : 0.001,
        min: 0.01,
        snap: e.altKey ? null : snapRef.current,
        near: { x: 6 / width, y: 6 / height },
      });
      draftRef.current = { id: d.id, area };
      setDraft(draftRef.current);
    };
    const up = () => {
      const d = drag.current;
      const cur = draftRef.current;
      drag.current = null;
      draftRef.current = null;
      setDraft(null);
      if (d && cur) onChange(cur.id, { x: round(cur.area.x), y: round(cur.area.y), width: round(cur.area.width), height: round(cur.area.height) });
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
  }, [width, height, onChange, onDone]);
  const selected = useGuidesView((s) => s.area);
  const begin = (e: React.PointerEvent, id: string, mode: Handle, area: Area) => {
    e.stopPropagation();
    e.preventDefault();
    useGuidesView.getState().set({ area: id });
    drag.current = { id, mode, x: e.clientX, y: e.clientY, start: area };
  };
  return (
    <div className="rect-editor" onPointerDown={(e) => (e.stopPropagation(), useGuidesView.getState().set({ area: null }))}>
      {rects.map((r) => {
        const a = draft?.id === r.id ? draft.area : r.area;
        return (
          <div key={r.id} className={`re-box ${r.kind} ${selected === r.id ? 'sel' : ''}`} style={pct(a)}>
            <span className="re-tab" title={t('Drag to move (Shift: 1 % steps)')} onPointerDown={(e) => begin(e, r.id, 'move', a)}>
              <Move size={10} /> {r.label}
              {draft?.id === r.id && <em>{`${Math.round(a.x * px.width)}, ${Math.round(a.y * px.height)} · ${Math.round(a.width * px.width)}×${Math.round(a.height * px.height)}`}</em>}
            </span>
            {HANDLES.map((h) => (
              <span key={h} className={`zone-handle h-${h}`} onPointerDown={(e) => begin(e, r.id, h, a)} />
            ))}
          </div>
        );
      })}
      <button className="btn small primary re-done" onClick={onDone}>
        {t('Done (Esc)')}
      </button>
    </div>
  );
}

/** Display menu (DisplayMenu): settings of the rulers and guides, snapping. */
export function GuidesSettings() {
  const view = useGuidesView();
  return (
    <label className="switch gm-switch">
      <input type="checkbox" checked={view.snap} onChange={(e) => view.set({ snap: e.target.checked })} />
      {t('Snap to guides')}
    </label>
  );
}

/** Display menu (DisplayMenu): the safe areas and areas to keep clear, edited on the stage, margins, areas. */
export function SafeSettings({ project }: { project: Project }) {
  const view = useGuidesView();
  const [g, setG] = useProjectGuides(project);
  const W = project.width;
  const H = project.height;
  const pctv = (v: number) => Math.round(v * 1000) / 10;
  return (
    <>
      <button className={`btn small ${view.editSafe ? 'primary' : ''}`} data-close onClick={() => view.set({ editSafe: !view.editSafe })}>
        <Move size={13} /> {view.editSafe ? t('Done editing') : t('Edit on the stage')}
      </button>
      <div className="gm-row">
        <label title={t('Margin on every side; resets a safe area placed by hand')}>
          {t('Action safe')}
          <input type="number" min={0} max={40} step={0.5} value={pctv(g.actionSafe)} onChange={(e) => setG({ ...g, actionSafe: Math.max(0, Math.min(40, +e.target.value)) / 100, actionArea: undefined })} />%
        </label>
        <label title={t('Margin on every side; resets a safe area placed by hand')}>
          {t('Title safe')}
          <input type="number" min={0} max={40} step={0.5} value={pctv(g.titleSafe)} onChange={(e) => setG({ ...g, titleSafe: Math.max(0, Math.min(40, +e.target.value)) / 100, titleArea: undefined })} />%
        </label>
      </div>
      {(g.actionArea || g.titleArea) && <p className="hint">{t('Placed by hand on the stage; a margin above puts it back in the centre.')}</p>}
      <div className="gm-title">
        {t('Areas to keep clear')}
        <button className="icon-btn ghost" title={t('Add an area (a channel bug top right)')} onClick={() => setG({ ...g, exclusions: [...g.exclusions, { id: newId(), name: t('Channel bug'), x: 0.85, y: 0.05, width: 0.1, height: 0.09 }] })}>
          <Plus size={13} />
        </button>
      </div>
      {!g.exclusions.length && <p className="hint">{t('None. Add the channel bug, a clock or a sign language box: graphics snap around them.')}</p>}
      {g.exclusions.map((x) => (
        <div key={x.id} className="gm-excl">
          <div className="row">
            <input value={x.name} onChange={(e) => setG({ ...g, exclusions: g.exclusions.map((y) => (y.id === x.id ? { ...y, name: e.target.value } : y)) })} />
            <button className="icon-btn ghost" title={t('Remove')} onClick={() => setG({ ...g, exclusions: g.exclusions.filter((y) => y.id !== x.id) })}>
              <Trash2 size={12} />
            </button>
          </div>
          <span className="muted small">
            {Math.round(x.x * W)}, {Math.round(x.y * H)} · {Math.round(x.width * W)}×{Math.round(x.height * H)} px
          </span>
        </div>
      ))}
      <p className="hint">
        <Eye size={11} /> {t('In px of the main format ({w}×{h}). Edit on the stage to move and resize them.', { w: W, h: H })}
      </p>
    </>
  );
}

/** Right panel while editing the safe areas: the values of the selected area, like a widget's. */
export function AreaInspector({ project }: { project: Project }) {
  const view = useGuidesView();
  const [g, setG] = useProjectGuides(project);
  const W = project.width;
  const H = project.height;
  const id = view.area;
  const [action, title] = safeRects(g);
  const x = g.exclusions.find((e) => e.id === id);
  const area = id === 'action' ? action : id === 'title' ? title : x;
  const set = (a: Area) => {
    if (id === 'action') setG({ ...g, actionArea: a });
    else if (id === 'title') setG({ ...g, titleArea: a });
    else if (x) setG({ ...g, exclusions: g.exclusions.map((e) => (e.id === x.id ? { ...e, ...a } : e)) });
  };
  if (!area)
    return (
      <div className="area-inspector">
        <div className="ai-title">{t('Safe areas')}</div>
        <p className="hint">{t('Click an area on the stage (its label) to see and type its position and size. Esc or Done to leave.')}</p>
      </div>
    );
  const box = { x: area.x, y: area.y, width: area.width, height: area.height };
  const field = (label: string, key: keyof Area, size: number) => (
    <label className="ai-field">
      <span>{label}</span>
      <input type="number" value={Math.round(box[key] * size)} onChange={(e) => set({ ...box, [key]: Math.max(0, Math.min(size, +e.target.value)) / size })} />
      <em>px</em>
    </label>
  );
  return (
    <div className="area-inspector">
      <div className="ai-title">{id === 'action' ? t('Action safe') : id === 'title' ? t('Title safe') : t('Area to keep clear')}</div>
      {x && (
        <label className="ai-field wide">
          <span>{t('Name')}</span>
          <input value={x.name} onChange={(e) => setG({ ...g, exclusions: g.exclusions.map((y) => (y.id === x.id ? { ...y, name: e.target.value } : y)) })} />
        </label>
      )}
      <div className="ai-grid">
        {field('X', 'x', W)}
        {field('Y', 'y', H)}
        {field(t('W'), 'width', W)}
        {field('H', 'height', H)}
      </div>
      <p className="hint">
        {t('In px of the main format ({w}×{h}). {pw} % × {ph} % of the picture.', { w: W, h: H, pw: Math.round(box.width * 100), ph: Math.round(box.height * 100) })}
      </p>
      {x ? (
        <button
          className="btn small"
          onClick={() => {
            setG({ ...g, exclusions: g.exclusions.filter((y) => y.id !== x.id) });
            view.set({ area: null });
          }}
        >
          <Trash2 size={13} /> {t('Remove the area')}
        </button>
      ) : (
        <button className="btn small" onClick={() => setG(id === 'action' ? { ...g, actionArea: undefined } : { ...g, titleArea: undefined })}>
          {t('Back to a {n} % margin', { n: pctv(id === 'action' ? g.actionSafe : g.titleSafe) })}
        </button>
      )}
    </div>
  );
}

const pctv = (v: number) => Math.round(v * 1000) / 10;

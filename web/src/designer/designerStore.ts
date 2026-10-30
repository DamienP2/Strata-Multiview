import { syncSettingDefaults } from '../engine/settings';
import { widgetOf } from '../widgets';
import { api, useApp } from '../store';
import { replaceColor } from '../colors';
import { create } from 'zustand';
import { DEFAULTS, defaultData, newId, propValue } from '../engine/evaluate';
import { fromFormatView, inFormat } from '../engine/formats';
import { MAIN_FORMAT, MARKER_MODEL, type Data, type FontStyle, type Keyframe, type Layer, type LayerType, type Marker, type Prop, type Template, type Value } from '../engine/types';
import { t, tc } from '../i18n';

export interface KeyRef {
  layerId: string;
  prop: Prop;
  frame: number;
}

interface DesignerStore {
  /** The template being edited, as saved (all formats). */
  base: Template | null;
  /** Format being edited ("main", "vertical"…). */
  format: string;
  /** What the editors show and edit: `base` laid out in `format`. Edits in
   *  another format than main are stored as that format's overrides. */
  template: Template | null;
  past: Template[];
  future: Template[];
  lastCoalesce: { key: string; at: number } | null;
  frame: number;
  playing: boolean;
  loop: boolean;
  selected: string[];
  selectedKeys: KeyRef[];
  /** The segment between two keyframes whose easing is edited: its start keyframe, in each property of the row. */
  selectedSegment: { layerId: string; props: Prop[]; frame: number } | null;
  expanded: Record<string, boolean>;
  zoom: number;
  /** The canvas's zoom set by hand; null: it fits the canvas area and follows its size (Auto). */
  stageZoom: number | null;
  /** The zoom at which the picture fits the canvas area. */
  stageFit: number;
  previewData: Data;
  /** Someone else holds the template's edit lock: edits are ignored. */
  readOnly: boolean;
  /** Counts user edits (autosave saves on change, not on load). */
  edits: number;
  /** Widgets run by hand in the designer (▶ ❚❚ ⟲), apart from the playhead. */
  widgetRun: Record<string, WidgetRun>;
  /** Try-out state of the spec widgets (the Operator controls in the inspector), by layer id. */
  widgetState: Record<string, unknown>;

  load: (t: Template | null) => void;
  /** Shows a newer version of the open template (read only) without resetting the view. */
  replace: (t: Template) => void;
  update: (recipe: (t: Template) => void, coalesceKey?: string) => void;
  /** Edits the saved template directly (all formats at once). */
  updateBase: (recipe: (t: Template) => void) => void;
  setFormat: (format: string) => void;
  undo: () => void;
  redo: () => void;
  setFrame: (f: number) => void;
  setPlaying: (p: boolean) => void;
  select: (ids: string[]) => void;
  selectKeys: (keys: KeyRef[]) => void;
  selectSegment: (s: DesignerStore['selectedSegment']) => void;
  toggleExpanded: (id: string) => void;
  set: (partial: Partial<DesignerStore>) => void;
  controlWidget: (layerId: string, c: 'start' | 'pause' | 'reset' | 'playhead') => void;
  /** Applies an Operator action to a widget's try-out state (same logic as on air, run by the server). */
  actWidget: (layerId: string, op: string, args?: unknown) => Promise<void>;
}

/** A widget's own clock: running since `startedAt` (ms), or stopped at `pausedAge` seconds. */
export interface WidgetRun {
  startedAt: number;
  pausedAge?: number;
}

/** Seconds a hand-run widget has been running. */
export const runAge = (r: WidgetRun, now: number) => r.pausedAge ?? Math.max(0, (now - r.startedAt) / 1000);

const HISTORY = 200;

const view = (base: Template | null, format: string) => (base ? structuredClone(inFormat(base, format)) : null);

/** The On air point (the IN/OUT marker), where a template opens. */
const onAirFrame = (t: Template | null) => t?.markers.find((m) => m.kind === 'out')?.frame ?? 0;

export const useDesigner = create<DesignerStore>((set, get) => ({
  base: null,
  format: MAIN_FORMAT,
  template: null,
  past: [],
  future: [],
  lastCoalesce: null,
  frame: 0,
  playing: false,
  loop: false,
  selected: [],
  selectedKeys: [],
  selectedSegment: null,
  expanded: {},
  zoom: 8,
  stageZoom: null,
  stageFit: 0.5,
  previewData: {},
  readOnly: false,
  edits: 0,
  widgetRun: {},
  widgetState: {},

  load(t) {
    const base = t ? structuredClone(t) : null;
    set({
      base,
      template: view(base, get().format),
      past: [],
      future: [],
      frame: onAirFrame(base),
      playing: false,
      selected: [],
      selectedKeys: [],
      selectedSegment: null,
      previewData: t ? defaultData(t) : {},
      lastCoalesce: null,
      widgetRun: {},
      widgetState: {},
    });
  },

  update(recipe, coalesceKey) {
    const { base, format, past, lastCoalesce, readOnly } = get();
    if (!base || readOnly) return;
    const edited = view(base, format)!;
    recipe(edited);
    const next = format === MAIN_FORMAT ? edited : fromFormatView(base, edited, format);
    syncSettingDefaults(next);
    const now = performance.now();
    const coalesce = coalesceKey && lastCoalesce?.key === coalesceKey && now - lastCoalesce.at < 800;
    set({
      base: next,
      template: view(next, format),
      past: coalesce ? past : [...past.slice(-HISTORY), base],
      future: [],
      lastCoalesce: coalesceKey ? { key: coalesceKey, at: now } : null,
      edits: get().edits + 1,
    });
  },

  replace(t) {
    const { format } = get();
    const base = structuredClone(t);
    set({ base, template: view(base, format), past: [], future: [], lastCoalesce: null });
  },

  updateBase(recipe) {
    const { base, format, past, readOnly } = get();
    if (!base || readOnly) return;
    const next = structuredClone(base);
    recipe(next);
    syncSettingDefaults(next);
    set({ base: next, template: view(next, format), past: [...past.slice(-HISTORY), base], future: [], lastCoalesce: null, edits: get().edits + 1 });
  },

  setFormat(format) {
    set({ format, template: view(get().base, format), selectedKeys: [] });
  },

  undo() {
    const { past, base, future, format } = get();
    if (!past.length || !base) return;
    const prev = past[past.length - 1];
    set({ base: prev, template: view(prev, format), past: past.slice(0, -1), future: [base, ...future], lastCoalesce: null, edits: get().edits + 1 });
  },

  redo() {
    const { past, base, future, format } = get();
    if (!future.length || !base) return;
    set({ base: future[0], template: view(future[0], format), future: future.slice(1), past: [...past, base], lastCoalesce: null, edits: get().edits + 1 });
  },

  setFrame(f) {
    const t = get().template;
    const max = t?.duration ?? 0;
    set({ frame: Math.max(0, Math.min(max, f)) });
  },
  setPlaying(playing) {
    const t = get().template;
    // Playing again from the end starts over.
    if (playing && t && get().frame >= t.duration - 1e-3) set({ frame: 0 });
    set({ playing });
  },
  select: (selected) => set({ selected, selectedSegment: null }),
  selectKeys: (selectedKeys) => set({ selectedKeys, selectedSegment: null }),
  selectSegment: (selectedSegment) => set({ selectedSegment, selectedKeys: [] }),
  toggleExpanded: (id) => set({ expanded: { ...get().expanded, [id]: !get().expanded[id] } }),
  set: (partial) => set(partial),

  async actWidget(id, op, args) {
    const s = get();
    const layer = s.template && findLayer(s.template, id);
    if (!layer) return;
    try {
      const state = await api('POST', '/api/widgets/act', { layer, state: s.widgetState[id] ?? null, op, args: args ?? null, data: s.previewData, ageSecs: s.frame / 50, fps: 50 });
      set({ widgetState: { ...get().widgetState, [id]: state } });
    } catch (e) {
      useApp.getState().toast(String((e as Error).message ?? e), 'error');
    }
  },
  controlWidget(id, c) {
    const run = { ...get().widgetRun };
    const now = Date.now();
    const cur = run[id];
    if (c === 'playhead') delete run[id];
    else if (c === 'reset') run[id] = { startedAt: now, pausedAge: 0 };
    else if (c === 'pause') run[id] = { startedAt: now, pausedAge: cur ? runAge(cur, now) : get().frame / 50 };
    else run[id] = { startedAt: now - (cur ? runAge(cur, now) : 0) * 1000 };
    set({ widgetRun: run });
  },
}));

// The values shown follow the variables: one exposed since, or a new default, shows its default.
useDesigner.subscribe((s, prev) => {
  if (!s.base || s.base === prev.base) return;
  const defaults = defaultData(s.base);
  const was = prev.base ? defaultData(prev.base) : {};
  const next = { ...s.previewData };
  let changed = false;
  for (const [k, v] of Object.entries(defaults))
    if (!(k in next) || (k in was && next[k] === was[k] && v !== was[k])) {
      next[k] = v;
      changed = true;
    }
  if (changed) useDesigner.setState({ previewData: next });
});

// ---------------------------------------------------------------------------
// Template mutations (applied on a draft inside `update`).

export function findLayer(t: Template, id: string): Layer | undefined {
  return t.layers.find((l) => l.id === id);
}

export const isAnimated = (l: Layer, p: Prop) => (l.tracks[p]?.length ?? 0) > 0;

/** Sets a property: writes a keyframe when the property is animated (auto-key). */
export function setProp(l: Layer, p: Prop, v: Value, frame: number) {
  const track = l.tracks[p];
  if (track && track.length) {
    const f = Math.round(frame);
    const existing = track.find((k) => k.frame === f);
    if (existing) existing.value = v;
    else {
      track.push({ frame: f, value: v, easing: nearestEasing(track, f) });
      track.sort((a, b) => a.frame - b.frame);
    }
  } else {
    l.props[p] = v;
  }
}

function nearestEasing(track: Keyframe[], f: number) {
  const before = [...track].reverse().find((k) => k.frame < f);
  return before?.easing ?? { type: 'easeInOut' as const };
}

/** The AE stopwatch: enable/disable animation of a property. */
export function toggleStopwatch(l: Layer, p: Prop, frame: number) {
  if (isAnimated(l, p)) {
    l.props[p] = propValue(l, p, frame);
    delete l.tracks[p];
  } else {
    l.tracks[p] = [{ frame: Math.round(frame), value: propValue(l, p, frame), easing: { type: 'easeInOut' } }];
  }
}

export function toggleKeyAt(l: Layer, p: Prop, frame: number) {
  const f = Math.round(frame);
  const track = l.tracks[p] ?? [];
  const idx = track.findIndex((k) => k.frame === f);
  if (idx >= 0) {
    track.splice(idx, 1);
    if (!track.length) {
      l.props[p] = propValue({ ...l, tracks: {} }, p, f);
      delete l.tracks[p];
    }
  } else {
    track.push({ frame: f, value: propValue(l, p, f), easing: nearestEasing(track, f) });
    track.sort((a, b) => a.frame - b.frame);
    l.tracks[p] = track;
  }
}

export function descendants(t: Template, id: string): string[] {
  const out: string[] = [];
  const walk = (pid: string) => {
    for (const l of t.layers) if (l.parent === pid) (out.push(l.id), walk(l.id));
  };
  walk(id);
  return out;
}

/** Layer type names, in English: translated where shown. */
export const LABELS: Record<LayerType, string> = {
  group: 'Group',
  rect: 'Rectangle',
  ellipse: 'Ellipse',
  text: 'Text',
  image: 'Image',
  clock: 'Clock',
  ticker: 'Ticker',
  roll: 'Credits roll',
  counter: 'Counter',
  progress: 'Progress bar',
  rotator: 'Rotator',
  typewriter: 'Typewriter',
  slideshow: 'Slideshow',
  qrCode: 'QR code',
  barChart: 'Bar chart / poll',
  polygon: 'Polygon',
  pen: 'Pen',
  table: 'Table',
  credits: 'Credits',
  chart: 'Chart',
  map: 'Map',
  scoreboard: 'Scoreboard',
  web: 'Iframe',
  socialWall: 'Social wall',
  socialBadge: 'Social badge',
};

/** Widget presets offered in the toolbar menu (a type plus overrides). */
export const WIDGET_PRESETS: { key: string; label: string; hint: string; type: LayerType; patch?: Partial<Record<string, unknown>> }[] = [
  { key: 'clock', label: 'Clock', hint: 'Time of day, any time zone', type: 'clock' },
  { key: 'date', label: 'Date', hint: 'dd/MM/yyyy', type: 'clock', patch: { format: 'dd/MM/yyyy' } },
  { key: 'countdown', label: 'Countdown', hint: 'To a time of day or a date', type: 'clock', patch: { mode: 'countdown', target: '20:00', format: 'HH:mm:ss' } },
  { key: 'timer', label: 'Timer', hint: 'Counts down a duration once on air', type: 'clock', patch: { mode: 'timer', target: '5:00', format: 'mm:ss' } },
  { key: 'elapsed', label: 'Stopwatch', hint: 'Time since the take', type: 'clock', patch: { mode: 'elapsed', format: 'mm:ss.f' } },
  { key: 'typewriter', label: 'Typewriter', hint: 'Letter-by-letter reveal', type: 'typewriter' },
  { key: 'counter', label: 'Counter', hint: 'Animated number', type: 'counter' },
  { key: 'progress', label: 'Progress bar', hint: 'Value / max, animated', type: 'progress' },
  { key: 'barChart', label: 'Bar chart / poll', hint: 'Label: value lines', type: 'barChart' },
  { key: 'slideshow', label: 'Slideshow', hint: 'Images with cross-fade', type: 'slideshow' },
  { key: 'qrCode', label: 'QR code', hint: 'URL or text', type: 'qrCode' },
];

const font = (align: FontStyle['align'] = 'left', weight = 700): FontStyle => ({ fontFamily: 'Inter', fontWeight: weight, align, verticalAlign: 'middle', uppercase: false });

export function makeLayer(type: LayerType, width: number, height: number, parent?: string | null): Layer {
  const base = {
    id: newId(),
    // A default name, in the interface's language (layers are found by id).
    name: t(LABELS[type]),
    parent: parent ?? null,
    visible: true,
    locked: false,
    tracks: {},
    bindings: {},
  };
  const centered = (w: number, h: number) => ({ x: parent ? 0 : Math.round((width - w) / 2), y: parent ? 0 : Math.round((height - h) / 2), width: w, height: h });
  // The spec widgets make their own layers (web/src/widgets).
  const made = widgetOf(type)?.create();
  if (made && Object.keys(made).length) {
    const { props = {}, ...rest } = made as { props?: Record<string, unknown> };
    const w = Number(props.width ?? 600);
    const h = Number(props.height ?? 400);
    // A rectangle is dark, like a panel: a text added on it reads at once (texts are white).
    const own = type === 'rect' ? { fill: [0.071, 0.078, 0.114, 1] } : {};
    return { ...base, type, ...rest, props: { ...own, ...props, ...centered(w, h) } } as Layer;
  }
  switch (type) {
    case 'group':
      return { ...base, type, clip: false, props: centered(800, 200) };
    case 'rect':
      return { ...base, type, props: { ...centered(500, 120), fill: [0.486, 0.361, 1, 1] } };
    case 'ellipse':
      return { ...base, type, props: { ...centered(200, 200), fill: [1, 0.231, 0.361, 1] } };
    case 'text':
      return {
        ...base,
        name: tc('text', 'Line'),
        type,
        text: 'Lorem ipsum dolor sit amet',
        fontFamily: 'Inter',
        fontWeight: 700,
        align: 'left',
        verticalAlign: 'middle',
        uppercase: false,
        props: { ...centered(700, 100), fontSize: 64, fill: DEFAULTS.fill },
      };
    case 'image':
      return { ...base, type, src: '', fit: 'contain', props: centered(400, 300) };
    case 'clock':
      return { ...base, type, mode: 'time', format: 'HH:mm:ss', timezone: '', target: '', ...font('center'), props: { ...centered(300, 90), fontSize: 64, fill: DEFAULTS.fill } };
    case 'ticker':
      return {
        ...base,
        type,
        text: 'First headline of the ticker\nSecond headline\nThird headline',
        separator: '   •   ',
        speed: 180,
        ...font('left', 600),
        props: { x: 0, y: parent ? 0 : height - 140, width: parent ? 1000 : width, height: 64, fontSize: 36, fill: DEFAULTS.fill },
      };
    case 'roll':
      return {
        ...base,
        type,
        text: 'PRESENTED BY\nJane Doe\n\nDIRECTED BY\nJohn Smith\n\nPRODUCED BY\nStrata',
        speed: 60,
        loop: true,
        ...font('center', 600),
        props: { ...centered(900, 800), fontSize: 44, fill: DEFAULTS.fill },
      };
    case 'counter':
      return {
        ...base,
        type,
        from: '0',
        to: '12500',
        duration: 2,
        decimals: 0,
        prefix: '',
        suffix: '',
        grouping: true,
        ...font('center', 800),
        props: { ...centered(500, 140), fontSize: 110, fill: DEFAULTS.fill },
      };
    case 'progress':
      return { ...base, type, value: '65', max: '100', duration: 1, track: '#ffffff33', props: { ...centered(800, 24), cornerRadius: 12, fill: [0.486, 0.361, 1, 1] } };
    case 'rotator':
      return { ...base, type, text: 'First message\nSecond message\nThird message', interval: 4, fade: 0.5, ...font('left', 600), props: { ...centered(900, 80), fontSize: 48, fill: DEFAULTS.fill } };
    case 'typewriter':
      return { ...base, type, text: 'Breaking news: type your text here', speed: 20, cursor: true, ...font('left', 600), props: { ...centered(1100, 90), fontSize: 52, fill: DEFAULTS.fill } };
    case 'slideshow':
      return { ...base, type, images: '', interval: 5, fade: 0.8, fit: 'cover', props: centered(640, 360) };
    case 'qrCode':
      return { ...base, type, data: 'https://example.com', background: '#ffffff', props: { ...centered(260, 260), fill: [0.05, 0.05, 0.08, 1] } };
    case 'barChart':
      return {
        ...base,
        type,
        text: 'Yes: 58\nNo: 34\nNo opinion: 8',
        percent: true,
        duration: 1.2,
        track: '#ffffff22',
        textColor: '#ffffff',
        ...font('left', 600),
        props: { ...centered(900, 260), fontSize: 36, fill: [0.486, 0.361, 1, 1] },
      };
    default:
      return { ...base, type, props: centered(600, 400) } as Layer;
  }
}

/** A new template: 1 s long (`fps` frames), on air at half of it. */
export function newTemplate(name: string, zone = '', fps = 25, secs = 1): Template {
  const len = Math.max(2, Math.round(fps * Math.min(600, Math.max(0.1, secs))));
  return {
    id: newId(),
    name,
    zone,
    duration: len,
    // Animate in up to the On air point, out from there to the end.
    markers: [{ id: newId(), frame: Math.round(len / 2), name: 'On air', kind: 'out' }],
    fields: [],
    layers: [],
    markerModel: MARKER_MODEL,
  };
}

/** How a marker reads: the On air point (green; the out animation starts
 *  there too), pauses (orange) and labels. */
export function markerRole(_t: Template, m: Marker): 'onair' | 'pause' | 'label' {
  return m.kind === 'out' ? 'onair' : m.kind;
}

/** Duplicates layers (with their children) just above the originals and selects the copies. */
export function duplicateLayers(ids: string[]) {
  const st = useDesigner.getState();
  if (!st.template || !ids.length) return;
  const newIds: string[] = [];
  st.update((tpl) => {
    for (const id of ids) {
      if (!tpl.layers.some((l) => l.id === id)) continue;
      const sub = [id, ...descendants(tpl, id)];
      const map = new Map(sub.map((x) => [x, newId()]));
      const clones = tpl.layers.filter((l) => sub.includes(l.id)).map((l) => ({ ...structuredClone(l), id: map.get(l.id)!, parent: l.id === id ? l.parent : (map.get(l.parent!) ?? l.parent), name: l.id === id ? t('{name} copy', { name: l.name }) : l.name }));
      // Paint order: right above the original.
      const last = Math.max(...sub.map((x) => tpl.layers.findIndex((l) => l.id === x)));
      tpl.layers.splice(last + 1, 0, ...clones);
      newIds.push(map.get(id)!);
    }
  });
  st.select(newIds);
}

// A palette colour changed: the open template follows it (the server has
// rewritten the saved ones).
window.addEventListener('colors-replaced', (e) => {
  const changes = (e as CustomEvent<[string, string][]>).detail;
  const { base, updateBase } = useDesigner.getState();
  if (!base) return;
  const probe = structuredClone(base);
  if (changes.reduce((n, [from, to]) => n + replaceColor(probe, from, to), 0)) updateBase((t) => void changes.forEach(([from, to]) => replaceColor(t, from, to)));
});

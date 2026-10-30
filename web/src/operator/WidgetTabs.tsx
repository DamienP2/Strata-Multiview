import { useEffect, useRef, useState } from 'react';
import { api, useApp } from '../store';
import { defaultData } from '../engine/evaluate';
import { isTimed } from '../engine/widgets';
import { widgetOf } from '../widgets';
import { Pause, Play, RotateCcw } from 'lucide-react';
import { CentreModeContext, KeyScope, type CentreMode } from '../widgets/kit';
import { Centre, OperatorBound } from '../widgets/centre';
import { oneKey, rowKey, usedDatasets, type DataSet, type Field, type Layer, type LayerClock, type PlayoutItem, type Template } from '../engine/types';
import { datasetUse, readsAllRows } from './DataSets';
import { exposedFields, type ExposedLayer } from './Exposed';
import { tc } from '../i18n';

/*
 * The graphic selected in the Operator, widget by widget (one tab each): its
 * values (Content), what feeds it (Data), and its internal controls, which
 * drive the preview only (the ones on air are in the On air panel).
 */

/** What one widget (layer) of a template gives the operator. */
export interface WidgetPart {
  layer: Layer;
  /** Values only it uses (texts, pictures, a table's rows…). */
  fields: Field[];
  /** Its properties exposed in the Designer. */
  exposed?: ExposedLayer;
  /** Data sets it reads. */
  datasets: DataSet[];
  /** It shows a data set feeding Texts (a Text reading it). */
  fed: DataSet[];
  /** Fields of it read from a data source. */
  bound: Field[];
  /** Internal controls (start / pause, pages, scores…). */
  controls: boolean;
}

export interface Parts {
  /** Values used by several widgets, or by the Logic only. */
  shared: Field[];
  widgets: WidgetPart[];
}

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** A widget of the graphic on air waits for Continue (F2): a layer waiting for its start, a widget's next page… */
export function widgetWaits(layers: Layer[], item: PlayoutItem): boolean {
  return layers.some((l) => {
    if (!l.visible) return false;
    if (isTimed(l) && l.manualStart && l.startOn !== 'trigger' && !item.clocks?.[l.id]) return true;
    const def = widgetOf(l);
    return !!def?.waits?.(l as never, item.widgets?.[l.id], item.data, item);
  });
}

/** The widget has controls with this data: its own (spec widgets) or start / pause (timed layers). */
export function hasControls(l: Layer, data: Record<string, string> = {}): boolean {
  const def = widgetOf(l);
  if (def?.Operator) return def.controls ? def.controls(l as never, data) : true;
  return isTimed(l) && l.type !== 'text';
}

const partsCache = new WeakMap<Template, Parts>();
/** Splits a template's values, data and controls by widget. */
export function widgetParts(tpl: Template): Parts {
  const hit = partsCache.get(tpl);
  if (hit) return hit;
  const layers = tpl.layers.filter((l) => l.visible !== false);
  const defaults = defaultData(tpl);
  const { content, layers: exposed } = exposedFields(tpl.fields, tpl.layers);
  const json = new Map(layers.map((l) => [l.id, JSON.stringify(l)]));
  const users = (f: Field) =>
    layers.filter((l) => {
      if (Object.values(l.bindings ?? {}).includes(f.key) || Object.values(l.settingBindings ?? {}).includes(f.key)) return true;
      return new RegExp(`\\{\\{\\s*${esc(f.key)}\\s*\\}\\}`).test(json.get(l.id)!);
    });
  const parts = new Map<string, WidgetPart>();
  const part = (l: Layer) => {
    let p = parts.get(l.id);
    if (!p) parts.set(l.id, (p = { layer: l, fields: [], datasets: [], fed: [], bound: [], controls: hasControls(l, defaults) }));
    return p;
  };
  const shared: Field[] = [];
  const logic = JSON.stringify(tpl.logic ?? null);
  for (const f of content) {
    const u = users(f);
    // A value nothing shows any more (its widget was deleted in the Designer): not asked for.
    if (!u.length && !logic.includes(`"${f.key}"`)) continue;
    if (u.length === 1) {
      const p = part(u[0]);
      p.fields.push(f);
      if (f.binding?.source) p.bound.push(f);
    } else shared.push(f);
  }
  for (const e of exposed) {
    const p = part(e.layer);
    p.exposed = e;
    for (const f of e.at.values()) if (f.binding?.source && !p.bound.includes(f)) p.bound.push(f);
  }
  for (const ds of usedDatasets(tpl)) {
    const use = datasetUse(tpl, ds);
    for (const l of layers) {
      if (!json.get(l.id)!.includes(`${ds.key}_`)) continue;
      // A ticker goes through all the rows by itself: nothing to choose in its tab.
      if (readsAllRows(l, ds)) continue;
      const p = part(l);
      p.datasets.push(ds);
      if (use.source && use.texts.some((x) => x.id === l.id)) p.fed.push(ds);
    }
  }
  for (const l of layers) if (hasControls(l, defaults)) part(l);
  // In the order of the layers.
  const widgets = layers.map((l) => parts.get(l.id)).filter((p): p is WidgetPart => !!p);
  const out = { shared, widgets };
  partsCache.set(tpl, out);
  return out;
}

/** The keys Update sends that belong to a widget: its values, and the row it shows of a data set feeding Texts. */
export function partKeys(tpl: Template, p: Pick<WidgetPart, 'fields' | 'exposed' | 'fed'>): string[] {
  return [...p.fields.map((f) => f.key), ...[...(p.exposed?.at.values() ?? [])].map((f) => f.key), ...p.fed.flatMap((d) => (datasetUse(tpl, d).source ? [rowKey(d.key), oneKey(d.key)] : []))];
}

/** Keys whose value Update would change on air. */
export function changedKeys(sent: Record<string, string>, air: Record<string, string> | undefined): Set<string> {
  const out = new Set<string>();
  if (!air) return out;
  for (const [k, v] of Object.entries(sent)) if ((air[k] ?? '') !== (v ?? '')) out.add(k);
  return out;
}

// ------------------------------------------------------- preview controls --

/** Widget states and clocks of the preview: what its controls changed, nothing on air. */
export interface PreviewRun {
  /** A control was used: the preview runs on from `since`. */
  active: boolean;
  /** Client time (ms) the preview runs from. */
  since: number;
  widgets: Record<string, unknown>;
  /** Clocks of the timed layers, in frames of preview age (as LayerClock on air). */
  clocks: Record<string, LayerClock>;
}

export const newPreviewRun = (): PreviewRun => ({ active: false, since: Date.now(), widgets: {}, clocks: {} });

/**
 * Where the preview holds a layer, still: the time on air (s) it shows until its Play, else
 * undefined. A widget moving by itself (a crawl, credits, a countdown, a Lottie…) waits there for
 * its Play, as a manual start would, until one of its controls is used; on air it runs from the take.
 */
export function heldAt(run: PreviewRun, l: Layer, data: Record<string, string>, holdSecs: number): number | undefined {
  if (l.id in run.widgets || l.id in run.clocks) return undefined;
  const def = widgetOf(l);
  const from = def?.Operator ? def.plays?.(l as never, data) : isTimed(l) && l.type !== 'text' && 'start';
  return from === 'start' ? 0 : from === 'onAir' ? holdSecs : undefined;
}

/** start / pause… of a timed layer, as the server does it on air (PlayoutItem::control); held at its start until then. */
export function controlClock(run: PreviewRun, l: Layer, control: 'toggle' | 'reset' | 'restart', ageFrames: number): PreviewRun {
  const c = { ...(run.clocks[l.id] ?? { offset: ageFrames, pausedAt: ageFrames }) };
  const running = c.pausedAt == null;
  if (control === 'toggle') {
    if (running) c.pausedAt = ageFrames;
    else {
      c.offset += ageFrames - (c.pausedAt ?? ageFrames);
      c.pausedAt = null;
    }
  } else if (control === 'reset') {
    c.offset = ageFrames;
    if (!running) c.pausedAt = ageFrames;
  } else {
    c.offset = ageFrames;
    c.pausedAt = null;
  }
  return { ...run, clocks: { ...run.clocks, [l.id]: c } };
}

/**
 * The control centre of one widget, driving the preview only (and, for its exposed settings,
 * the take's draft values through `onData`).
 */
export function PreviewControls({ layer, run, setRun, data, fps, ageSecs, holdSecs, lang, onData, mode = 'operator' }: { layer: Layer; run: PreviewRun; setRun: (f: (r: PreviewRun) => PreviewRun) => void; data: Record<string, string>; fps: number; ageSecs: () => number; holdSecs: number; lang?: string; onData?: (patch: Record<string, string>) => void; /** `air`: its state and actions only (over the preview). */ mode?: CentreMode }) {
  const def = widgetOf(layer);
  const offset = useApp((s) => s.serverOffset);
  const tz = useApp((s) => s.timezone);
  const toast = useApp((s) => s.toast);
  // The widget's own controls (they re-render with time, like on air).
  const [, tick] = useState(0);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    const h = setInterval(() => document.hidden || tick((n) => n + 1), 500);
    return () => {
      live.current = false;
      clearInterval(h);
    };
  }, []);
  const Op = def?.Operator;
  if (Op) {
    // Held (heldAt): it waits for its Play as a manual start would, its time standing still.
    const at = heldAt(run, layer, data, holdSecs);
    const held = at !== undefined;
    const lay = held ? { ...layer, manualStart: true } : layer;
    const act = async (op: string, args?: unknown) => {
      try {
        const state = await api('POST', '/api/widgets/act', { layer: lay, state: run.widgets[layer.id] ?? null, op, args: args ?? null, data, ageSecs: at ?? ageSecs(), fps });
        if (live.current) setRun((r) => ({ ...r, widgets: { ...r.widgets, [layer.id]: state } }));
      } catch (e) {
        toast((e as Error).message, 'error');
      }
    };
    const state = run.widgets[layer.id];
    const time = { nowMs: Date.now() + offset, ageSecs: at ?? ageSecs(), defaultTz: tz, fps, lang, held };
    // The preview's controls never take the keyboard (only the selected widget on air does).
    return (
      <KeyScope.Provider value={false}>
        <div className="opx-pvctl cc-host">
          <CentreModeContext.Provider value={mode}>
            <OperatorBound layer={layer} data={data} setData={onData}>
              <Op layer={lay as never} state={state} data={data} act={act} time={time} lang={lang} setData={onData} />
            </OperatorBound>
          </CentreModeContext.Provider>
        </div>
      </KeyScope.Provider>
    );
  }
  const c = run.clocks[layer.id];
  // Held at its start until its first Play (controlClock).
  const running = !!c && c.pausedAt == null;
  const age = ageSecs() * fps;
  const toggle = () => setRun((r) => controlClock(r, layer, 'toggle', age));
  return (
    <div className="opx-pvctl cc-host">
      <CentreModeContext.Provider value={mode}>
        <Centre
          name={layer.name}
          actions={[
            running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: toggle } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: toggle },
            { icon: RotateCcw, label: tc('cc', 'Restart'), key: 'T', onClick: () => setRun((r) => controlClock(r, layer, running ? 'restart' : 'reset', age)) },
          ]}
        />
      </CentreModeContext.Provider>
    </div>
  );
}

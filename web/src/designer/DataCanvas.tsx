import { widgetOf } from '../widgets';
import { HexColorField } from '../ui/fields';
import { useShallow } from 'zustand/react/shallow';
import { ArrowDown, ArrowDownUp, ArrowLeft, ArrowRight, ArrowUp, Database, Filter, Hash, Image as ImageIcon, Palette, Plus, Radio, Repeat, Rows3, Timer, ToggleLeft, Trash2, Type, Variable, Wand2, X, Zap, Blend, Pause, ChevronsUp, FlipVertical2 } from 'lucide-react';
import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { create } from 'zustand';
import { isTable, pipeline, useBoundData, useData, useProjectSources, useSources, type Stage } from '../data/dataStore';
import { col, holdFrame, interpolateText, newId, num, parseHex, resolveLayer, toHex } from '../engine/evaluate';
import { applyLogic, measureLayer } from '../engine/logic';
import { api, useApp } from '../store';
import { columnKey, linesKey, rowKey, tableKey, TEXT_LIKE, type CondOp, type DataSet, type Data, type DataFilter, type DataFlow, type DataStep, type Field, type FieldKind, type FlowNode, type Layer, type LiveChange, type Logic, type LogicNode, type Prop, type Template, type ValueChange } from '../engine/types';
import { isTimed } from '../engine/widgets';
import { navigate, projectPath } from '../router';
import { autoLayout, measureNodes, type LayoutEdge } from '../ui/autoLayout';
import { freeSpot, ResizeCorner, useCanvasView } from '../ui/canvasView';
import { findLayer, useDesigner } from './designerStore';
import { AssetPicker, fieldKey, PROP_LABELS } from './Inspector';
import { layerIcon } from './LayersPanel';
import { WidgetRunButtons } from './WidgetRun';
import { BuildCard, TableNodeInfo } from './TableBuild';
import { unexposeRows } from '../widgets/table-rows';
import { CanvasHeader, CanvasVoid } from '../ui/CanvasHeader';
import { Dropdown } from '../ui/Dropdown';
import { fedKey } from '../engine/valueMotion';
import { getPath, setPath } from '../widgets/kit';
import { isColorValue, settingLabel, settingPorts } from '../engine/settingPorts';
import { fieldUsed, forgetKeys, formatSetting, settingKind } from '../engine/settings';
import { lang, t, tc, tn } from '../i18n';

/*
 * Data canvas. The data goes through as tables:
 *   source (a list of its value, as rows and columns)
 *     → filter (conditions, rows, columns) → sort (keys) → transform (text)
 *     → variable (the text of the last table: a line per row)
 *     → layers' values, and the operations (math, conditions…).
 * The source → … → variable part is the template's `dataflow`, compiled into
 * each variable's binding (list + steps) at each change. Variables → layers
 * are {{key}} in a text or property bindings; the operations are the logic
 * graph, whose "field" and "set property" nodes are the variables and the
 * layers' ports here. Positions of variables and layers, and the sizes of
 * the nodes, are a per-browser layout.
 */

export const useDataCanvas = create<{ open: boolean; set: (open: boolean) => void }>((set) => ({ open: false, set: (open) => set({ open }) }));

/** Content edited by hand only (a data set's table goes into the widget's source next to it). */
const HAND_ONLY = new Set(['grid', 'entries']);
/** What a layer shows, wired by default: its text, or its picture. */
function contentOf(l: Layer): string[] {
  const spec = widgetOf(l)?.content;
  if (spec) return spec.filter((p) => !HAND_ONLY.has(p));
  if (l.type === 'image') return ['src'];
  return typeof (l as { text?: unknown }).text === 'string' ? ['text'] : [];
}
/** Content ports taking a whole table (a data set's All rows). */
const TABLE_PORTS: Record<string, string> = { table: 'link', credits: 'link', chart: 'csv', ticker: 'items', socialWall: 'items' };
const takesTable = (l: Layer, port: string) => TABLE_PORTS[l.type] === port;
/** The content port of a layer taking a data set's All rows, if it has one. */
export const tablePortOf = (l: Layer): string | undefined => TABLE_PORTS[l.type];
/** The layer's items are one text (the social wall's JSON), not a list (the ticker's). */
const textItems = (l: Layer) => l.type === 'socialWall';
const CONTENT_LABEL: Record<string, string> = { src: 'Picture', text: 'Text', link: 'Rows', csv: 'Rows', items: 'Items', 'title.text': 'Title' };
type Item = { cat?: string; text?: string; on?: boolean };
const contentValue = (l: Layer, port: string) => {
  // The ticker's items are a list (the social wall's are a text).
  const items = (l as { items?: unknown }).items;
  if (port === 'items' && Array.isArray(items)) return (items as Item[]).map((i) => i.text ?? '').join('\n');
  return String(getPath(l, port) ?? '');
};
/** "12 rows · Team, Pts…" for a data set's whole table. */
function tableSummary(v: string): string | null {
  if (!v.startsWith('[[')) return null;
  try {
    const a = JSON.parse(v) as string[][];
    const head = a[0] ?? [];
    return tn(a.length - 1, '{n} row · {columns}', '{n} rows · {columns}', { columns: `${head.slice(0, 3).join(', ')}${head.length > 3 ? '…' : ''}` });
  } catch {
    return null;
  }
}
/** Wires `{{key}}` into a layer's content port (`x` is the draft layer). */
function wireContent(x: Layer, port: string, key: string) {
  // A data set's rows into a text or a typewriter: a line per row (or only the current one, in the Operator).
  if ((x.type === 'text' || x.type === 'typewriter') && port === 'text' && key.endsWith('__table')) key = linesKey(key.slice(0, -'__table'.length));
  const token = `{{${key}}}`;
  const l = x as unknown as Record<string, unknown>;
  if (port === 'text') {
    const cur = String(l.text ?? '');
    if (!cur.includes(token)) l.text = cur.trim() && /\{\{\w+\}\}/.test(cur) ? `${cur} ${token}` : token;
  } else if (port === 'items' && !textItems(x)) l.items = [{ cat: '', text: token, on: true }];
  else {
    setPath(l, port, token);
    if (x.type === 'chart' && port === 'csv') l.source = 'csv';
  }
}
/** Takes `{{key}}` out of a layer's content port. */
function unwireContent(x: Layer, port: string, key: string) {
  const token = `{{${key}}}`;
  const l = x as unknown as Record<string, unknown>;
  if (port === 'items' && !textItems(x)) {
    l.items = ((l.items as Item[] | undefined) ?? []).filter((i) => !(i.text ?? '').includes(token));
    return;
  }
  setPath(
    l,
    port,
    String(getPath(l, port) ?? '')
      .split(token)
      .join('')
      .replace(/ {2,}/g, ' ')
      .trim(),
  );
}
/** A {{key}} in a layer's text or picture. */
const tokensIn = (l: Layer, port: string, key: string) => contentValue(l, port).includes(`{{${key}}}`);
/** The data set whose All rows the layer's table port reads (`{{key__table}}` alone there): its key. */
export function tableKeyAt(l: Layer): string | null {
  const port = TABLE_PORTS[l.type];
  return port ? (/^\{\{(\w+)__table\}\}$/.exec(contentValue(l, port).trim())?.[1] ?? null) : null;
}

/** Setting ports of a layer: the settings set from the data (or waiting for a wire). */
const settingPortsOf = (l: Layer) =>
  Object.keys(l.settingBindings ?? {})
    .filter((p) => !HAND_ONLY.has(p) && !contentOf(l).includes(p))
    .map((p) => `s:${p}`);
/** Sets a setting of `l` from `key` (a variable or a data set column); a plain text variable takes the setting's type. */
function bindSetting(t: Template, l: Layer, path: string, key: string) {
  (l.settingBindings ??= {})[path] = key;
  const f = t.fields.find((x) => x.key === key);
  const cur = getPath(l, path);
  if (f && f.kind === 'text' && !f.binding) {
    const kind = settingKind(cur ?? (/color$/i.test(path) ? '#ffffff' : ''), path);
    if (kind !== 'text') f.kind = kind;
    if (!f.default) f.default = formatSetting(cur ?? '');
  }
}
/** The setting goes back to its value in Properties. */
function unbindSetting(l: Layer, path: string) {
  if (!l.settingBindings) return;
  delete l.settingBindings[path];
  if (!Object.keys(l.settingBindings).length) delete l.settingBindings;
}

/** Where a layer takes an exposed value: a content port, `s:` + a setting, or a property. */
type Exposure = { layer: string; port: string };
/**
 * Fields exposed to the Operator for one value of one layer (the (•) of the
 * Inspector): typed by the Operator, used by that port only, fed by no data and
 * read by no operation. They show in their layer's node, not as variables.
 */
function exposuresOf(t: Template, flow: DataFlow): Map<string, Exposure> {
  const uses = new Map<string, (Exposure | null)[]>();
  const use = (k: string, e: Exposure | null) => void (k && uses.set(k, [...(uses.get(k) ?? []), e]));
  for (const l of t.layers) {
    for (const [p, k] of Object.entries(l.bindings ?? {})) if (k) use(k, { layer: l.id, port: p });
    for (const [p, k] of Object.entries(l.settingBindings ?? {})) if (k) use(k, HAND_ONLY.has(p) || contentOf(l).includes(p) ? null : { layer: l.id, port: `s:${p}` });
    // {{key}}: a content that is exactly one key is its exposure, any other mention another use.
    const count = new Map<string, number>();
    for (const m of JSON.stringify(l).matchAll(/\{\{(\w+)\}\}/g)) count.set(m[1], (count.get(m[1]) ?? 0) + 1);
    for (const c of contentOf(l)) {
      const k = /^\{\{(\w+)\}\}$/.exec(contentValue(l, c).trim())?.[1];
      if (!k || !count.get(k)) continue;
      use(k, { layer: l.id, port: c });
      count.set(k, count.get(k)! - 1);
    }
    for (const [k, n] of count) if (n > 0) use(k, null);
  }
  const fed = new Set(flow.edges.flatMap((e) => (e.to.startsWith('field:') ? [e.to.slice(6)] : [])));
  const flowJson = JSON.stringify(flow);
  const out = new Map<string, Exposure>();
  for (const f of t.fields) {
    const u = uses.get(f.key);
    if (u?.length !== 1 || !u[0] || f.binding || fed.has(f.key) || flowJson.includes(`{${f.key}}`)) continue;
    if (t.logic?.nodes.some((n) => n.type === 'field' && n.key === f.key)) continue;
    out.set(f.key, u[0]);
  }
  return out;
}
/** A field exposed to the Operator goes once nothing uses it any more (as the Inspector's (•) does). */
function dropField(t: Template, key: string) {
  if (fieldUsed(t, key) || t.logic?.nodes.some((n) => n.type === 'field' && n.key === key) || t.dataflow?.edges.some((e) => e.to === `field:${key}`)) return;
  t.fields = t.fields.filter((f) => f.key !== key);
}

/** Properties a layer exposes as ports. */
function portsOf(l: Layer): string[] {
  const props: Prop[] = ['x', 'y', 'width', 'height', 'scaleX', 'scaleY', 'rotation', 'opacity'];
  if (l.type !== 'image' && l.type !== 'slideshow') props.push('fill');
  if (l.type !== 'image' && l.type !== 'slideshow' && l.type !== 'progress' && l.type !== 'qrCode') props.push('stroke', 'strokeWidth');
  if (l.type === 'rect' || l.type === 'group' || l.type === 'progress') props.push('cornerRadius');
  if (TEXT_LIKE.has(l.type)) props.push('fontSize', 'letterSpacing');
  if (l.type === 'image') props.push('cropLeft', 'cropRight', 'cropTop', 'cropBottom');
  if (l.type === 'ellipse') props.push('arcStart', 'arcEnd', 'innerRadius');
  if (l.type === 'pen') props.push('drawStart', 'drawEnd');
  if (l.type === 'rect') props.push('cornerTl', 'cornerTr', 'cornerBr', 'cornerBl');
  if (['rect', 'ellipse', 'polygon', 'pen', 'text'].includes(l.type)) props.push('fill2');
  return props;
}
const isColor = (p: string) => p === 'fill' || p === 'stroke' || p === 'fill2';

/** Text operations of a Transform node. */
const TRANSFORMS: { make: () => DataFilter; label: string }[] = [
  { make: () => ({ type: 'upper' }), label: 'Uppercase' },
  { make: () => ({ type: 'lower' }), label: 'Lowercase' },
  { make: () => ({ type: 'capitalize' }), label: 'Capitalize' },
  { make: () => ({ type: 'trim' }), label: 'Trim spaces' },
  { make: () => ({ type: 'prefix', text: '' }), label: 'Prefix' },
  { make: () => ({ type: 'suffix', text: '' }), label: 'Suffix' },
  { make: () => ({ type: 'replace', find: '', with: '' }), label: 'Replace' },
  { make: () => ({ type: 'truncate', max: 30 }), label: 'Truncate' },
  { make: () => ({ type: 'number', decimals: 0, grouping: true }), label: 'Number format' },
  { make: () => ({ type: 'math', op: 'add', value: 0 }), label: 'Calculate' },
  { make: () => ({ type: 'map', when: '', then: '' }), label: 'If equals' },
  { make: () => ({ type: 'default', text: '' }), label: 'If empty' },
  { make: () => ({ type: 'join', separator: ' · ' }), label: 'Join rows' },
];

const FIELD_KINDS: { value: FieldKind; label: string }[] = [
  { value: 'text', label: 'Text' },
  { value: 'multiline', label: 'Paragraph' },
  { value: 'number', label: 'Number' },
  { value: 'color', label: 'Color' },
  { value: 'image', label: 'Picture' },
  { value: 'boolean', label: 'On / off' },
  { value: 'table', label: 'Table rows' },
];

const CONDITIONS: { op: CondOp; label: string; value: boolean }[] = [
  { op: 'eq', label: '=', value: true },
  { op: 'ne', label: '≠', value: true },
  { op: 'contains', label: 'contains', value: true },
  { op: 'notContains', label: 'does not contain', value: true },
  { op: 'startsWith', label: 'starts with', value: true },
  { op: 'gt', label: '>', value: true },
  { op: 'ge', label: '≥', value: true },
  { op: 'lt', label: '<', value: true },
  { op: 'le', label: '≤', value: true },
  { op: 'empty', label: 'is empty', value: false },
  { op: 'notEmpty', label: 'is not empty', value: false },
];

const W = { source: 360, step: 320, field: 250, logic: 210, layer: 260 };
const HEAD = 34;
const ROW = 24;
const CROW = 52;
const COLS = { source: 40, field: 1640, logic: 1940, layer: 2220 };

interface Layout {
  pos: Record<string, { x: number; y: number }>;
  size: Record<string, { w: number; h?: number }>;
  collapsed: Record<string, boolean>;
}

function loadLayout(id: string): Layout {
  const empty: Layout = { pos: {}, size: {}, collapsed: {} };
  try {
    const l = JSON.parse(localStorage.getItem(`pulse.dataCanvas.${id}`) ?? '{}');
    return { pos: l.pos ?? {}, size: l.size ?? {}, collapsed: l.collapsed ?? {} };
  } catch {
    return empty;
  }
}

type Pt = { x: number; y: number };
/** A port's dot: its node and where it is in it (canvas units). */
type PortSpot = { node: string; x: number; y: number };
const sameSpots = (a: Record<string, PortSpot>, b: Record<string, PortSpot>) => {
  const ka = Object.keys(a);
  return ka.length === Object.keys(b).length && ka.every((k) => b[k] && a[k].node === b[k].node && a[k].x === b[k].x && a[k].y === b[k].y);
};
function curve(a: Pt, b: Pt) {
  const dx = Math.max(40, Math.abs(b.x - a.x) / 2);
  return `M ${a.x} ${a.y} C ${a.x + dx} ${a.y}, ${b.x - dx} ${b.y}, ${b.x} ${b.y}`;
}

type Wire = { kind: 'flow'; id: string } | { kind: 'field'; key: string } | { kind: 'logic'; id: string };
type StepType = DataStep['type'];

const STEP: Record<StepType, { label: string; icon: typeof Filter; hint: string; make: () => DataStep }> = {
  filter: { label: 'Filter', icon: Filter, hint: 'Rows matching conditions, some rows, some columns', make: () => ({ type: 'filter', where: [], from: 0, count: 0, columns: [] }) },
  sort: { label: 'Sort', icon: ArrowDownUp, hint: 'Order the rows by one or more columns', make: () => ({ type: 'sort', keys: [] }) },
  transform: { label: 'Transform', icon: Wand2, hint: 'Uppercase, prefix, replace, number format…', make: () => ({ type: 'transform', columns: [], ops: [] }) },
};

/** The step part of a flow node (what the binding keeps). */
function stepOf(n: FlowNode): DataStep {
  const { id: _id, x: _x, y: _y, ...step } = n as FlowNode & { type: StepType };
  return step as DataStep;
}

/** Height of a data set's column row (its ports follow it). */
const DS_ROW = 28;
/** A column's dot in the middle of its row (`.port` already centres it: no extra margin). */
const DS_DOT = { marginTop: 0 };

/** The source node and the steps leading to node `id` (included). */
function chainOf(flow: DataFlow, id: string): { source: FlowNode & { type: 'source' }; steps: DataStep[] } | null {
  const byId = new Map(flow.nodes.map((n) => [n.id, n]));
  const steps: DataStep[] = [];
  const seen = new Set<string>();
  let cur = byId.get(id);
  while (cur && cur.type !== 'source') {
    if (seen.has(cur.id)) return null;
    seen.add(cur.id);
    steps.unshift(stepOf(cur));
    const from = flow.edges.find((e) => e.to === cur!.id)?.from;
    cur = from ? byId.get(from) : undefined;
  }
  return cur?.type === 'source' ? { source: cur, steps } : null;
}

/**
 * The graph of a template made before the Data canvas worked on tables:
 * the simple paths (`a.b`, `items[0].x`, `items[*].x`), their sort and
 * filters become nodes. Others (`rows[{row}].x`…) stay as they are.
 */
function migrate(t: Template): DataFlow {
  const flow: DataFlow = { nodes: [], edges: [] };
  let y = 40;
  for (const f of t.fields) {
    const b = f.binding;
    if (!b?.source || isTable(b)) continue;
    const m = b.path.match(/^([\w.]*?)(?:\[(-?\d+|\*)\])?(?:\.?([\w.]+))?$/);
    if (!m || /length$/.test(b.path)) continue;
    const [, list = '', idx, column] = m;
    const chain: FlowNode[] = [{ id: newId(), x: COLS.source, y, type: 'source', source: b.source, list: idx === undefined ? '' : list }];
    const col = idx === undefined ? (b.path ? b.path : '') : (column ?? '');
    if (b.sort?.by && b.sort.list === list) chain.push({ id: newId(), x: 0, y, type: 'sort', keys: [{ column: b.sort.by, descending: b.sort.descending, numeric: b.sort.numeric }] });
    if (idx !== undefined || col) {
      const i = idx === undefined || idx === '*' ? 0 : +idx;
      chain.push({ id: newId(), x: 0, y, type: 'filter', where: [], from: i >= 0 && idx !== undefined && idx !== '*' ? i + 1 : i, count: idx === undefined || idx === '*' ? 0 : 1, columns: col ? [col] : [] });
    }
    if (b.filters?.length) chain.push({ id: newId(), x: 0, y, type: 'transform', columns: [], ops: b.filters });
    chain.forEach((n, i) => {
      n.x = COLS.source + i * (W.step + 50) + (i ? 40 : 0);
      flow.nodes.push(n);
      flow.edges.push({ from: n.id, to: chain[i + 1]?.id ?? `field:${f.key}` });
    });
    y += 380;
  }
  return flow;
}

/** How a new value shows on air (Layer.liveChanges). */
const VALUE_CHANGES: { value: ValueChange; label: string; Icon: typeof Zap }[] = [
  { value: 'hold', label: 'No automatic update', Icon: Pause },
  { value: 'cut', label: 'Cut', Icon: Zap },
  { value: 'fade', label: 'Fade', Icon: Blend },
  { value: 'slide', label: 'Slide', Icon: ChevronsUp },
  { value: 'flip', label: 'Flip', Icon: FlipVertical2 },
  { value: 'count', label: 'Count up', Icon: Hash },
  { value: 'outIn', label: 'Out, then in again', Icon: Repeat },
];

/** Height of the transition rows under a port (LiveRows). */
const LIVE_H = 50;

/** Under a layer's port fed by a source or a data set: how a new value of the keys wired there shows on air (icons), and its length (slider). */
function LiveRows({ template, layer, keys }: { template: Template; layer: Layer; keys: string[] }) {
  const update = useDesigner((s) => s.update);
  // Set on the variable before it was the layers'.
  const onVariable = (k: string): LiveChange | undefined => {
    const b = template.fields.find((f) => f.key === k)?.binding;
    return b?.change ? { kind: b.change, secs: b.changeSecs } : undefined;
  };
  const cur = layer.liveChanges?.[keys[0]] ?? onVariable(keys[0]);
  const kind = cur?.kind ?? 'cut';
  const secs = cur?.secs ?? 0.6;
  const set = (v: LiveChange, coalesce?: string) =>
    update((t) => {
      const l = findLayer(t, layer.id)!;
      const all = (l.liveChanges ??= {});
      for (const k of keys) {
        if (v.kind !== 'cut' || onVariable(k)) all[k] = v;
        else delete all[k];
      }
      if (!Object.keys(all).length) delete l.liveChanges;
    }, coalesce);
  return (
    <div className="dc-live" style={{ height: LIVE_H }} title={t('When the value changes on air')} onPointerDown={(e) => e.stopPropagation()}>
      <div className="dc-live-icons">
        {VALUE_CHANGES.map((o) => (
          <button key={o.value} className={o.value === kind ? 'on' : ''} title={tc('value change', o.label)} onClick={() => set({ kind: o.value, secs: cur?.secs })}>
            <o.Icon size={12} />
          </button>
        ))}
      </div>
      <div className={`dc-live-secs ${kind === 'cut' || kind === 'hold' ? 'off' : ''}`} title={t('Duration')}>
        <Timer size={11} />
        <input type="range" min={0.1} max={3} step={0.1} value={secs} disabled={kind === 'cut' || kind === 'hold'} onChange={(e) => set({ kind, secs: +e.target.value }, `live-${layer.id}-${keys[0]}`)} />
        <span>{secs.toFixed(1).replace('.', lang() === 'fr' ? ',' : '.')} s</span>
      </div>
    </div>
  );
}

/** The fields' bindings from the graph. */
function compile(t: Template, flow: DataFlow) {
  for (const f of t.fields) {
    const from = flow.edges.find((e) => e.to === `field:${f.key}`)?.from;
    if (!from) {
      if (f.binding && isTable(f.binding)) delete f.binding;
      continue;
    }
    const c = chainOf(flow, from);
    // How a new value shows on air (made before it was the layers'): kept.
    if (c) f.binding = { source: c.source.source, path: '', list: c.source.list, steps: c.steps, change: f.binding?.change, changeSecs: f.binding?.changeSecs };
    else delete f.binding;
  }
  // Data sets: the rows of the chain wired into them.
  const sets: DataSet[] = [];
  for (const n of flow.nodes) {
    if (n.type !== 'dataset') continue;
    const from = flow.edges.find((e) => e.to === n.id)?.from;
    const c = from ? chainOf(flow, from) : null;
    if (c) sets.push({ key: n.key, name: n.name, source: c.source.source, list: c.source.list, steps: c.steps, change: n.change });
  }
  if (sets.length) t.datasets = sets;
  else delete t.datasets;
  t.dataflow = flow.nodes.length ? flow : undefined;
  if (!t.dataflow) delete t.dataflow;
}

/** Changes the data graph of `t` (the one saved, else `base`: its old bindings migrated, as the canvas draws them), then the bindings it gives. */
export function editDataflow(t: Template, fn: (f: DataFlow) => void, base?: DataFlow | null) {
  const f: DataFlow = structuredClone(t.dataflow ?? base ?? migrate(t));
  fn(f);
  compile(t, f);
}
/** Wires `from` into `to` (its only input), unless that makes a loop. */
export function linkFlow(f: DataFlow, from: string, to: string) {
  let cur: string | undefined = from;
  const seen = new Set<string>();
  while (cur && !seen.has(cur)) {
    if (cur === to) return;
    seen.add(cur);
    cur = f.edges.find((e) => e.to === cur)?.from;
  }
  f.edges = f.edges.filter((e) => e.to !== to);
  f.edges.push({ from, to });
}
/** A new data set node named `name` at `at`, in `f`; its key comes from its name, not a variable's nor another set's. */
export function newDataset(t: Template, f: DataFlow, name: string, at: Pt): FlowNode & { type: 'dataset' } {
  const taken = new Set([...t.fields.map((x) => x.key), ...f.nodes.flatMap((n) => (n.type === 'dataset' ? [n.key] : []))]);
  const n = { id: newId(), ...at, type: 'dataset' as const, key: fieldKey(name, taken), name, change: 'outIn' as const };
  f.nodes.push(n);
  return n;
}
/**
 * Feeds a layer's table port from a source's list through a new data set
 * (source → data set → the layer), as wiring the data set's All rows by hand
 * does. The source node is shared with the chains already reading that list.
 * Gives the data set's key (null: the layer takes no table).
 */
export function feedTable(t: Template, l: Layer, source: string, list: string): string | null {
  const port = TABLE_PORTS[l.type];
  if (!port) return null;
  let key = '';
  editDataflow(t, (f) => {
    let src = f.nodes.find((n): n is FlowNode & { type: 'source' } => n.type === 'source' && n.source === source && n.list === list);
    if (!src) {
      // Under the chains already there.
      const y = f.nodes.reduce((m, n) => Math.max(m, n.y + 380), 40);
      src = { id: newId(), x: COLS.source, y, type: 'source', source, list };
      f.nodes.push(src);
    }
    const ds = newDataset(t, f, l.name, { x: src.x + W.source + 90, y: src.y });
    linkFlow(f, src.id, ds.id);
    key = ds.key;
  });
  wireContent(l, port, tableKey(key));
  return key;
}
/**
 * Takes the data set out of a layer's table port; when nothing else reads it
 * (no layer, variable or operation), the data set goes with the nodes that
 * fed only it. The port keeps what else it held.
 */
export function unfeedTable(t: Template, l: Layer) {
  const port = TABLE_PORTS[l.type];
  const key = tableKeyAt(l);
  if (!port || !key) return;
  unwireContent(l, port, tableKey(key));
  if (JSON.stringify([t.layers, t.fields, t.logic ?? null]).includes(`${key}_`)) return;
  editDataflow(t, (f) => {
    let cur = f.nodes.find((n) => n.type === 'dataset' && n.key === key)?.id;
    while (cur) {
      const into = f.edges.find((e) => e.to === cur)?.from;
      f.nodes = f.nodes.filter((n) => n.id !== cur);
      f.edges = f.edges.filter((e) => e.from !== cur && e.to !== cur);
      cur = into && !f.edges.some((e) => e.from === into) ? into : undefined;
    }
  });
}

export function DataCanvas() {
  const { open, set: setOpen } = useDataCanvas();
  const template = useDesigner((s) => s.template);
  if (!open || !template) return null;
  return <Canvas key={template.id} template={template} onClose={() => setOpen(false)} />;
}

function Canvas({ template, onClose }: { template: Template; onClose: () => void }) {
  const { update, previewData, frame } = useDesigner(useShallow((s) => ({ update: s.update, previewData: s.previewData, frame: s.frame })));
  const projectId = useApp((s) => s.project?.id);
  // Lookups by id: every source; the palette: this project's and the shared ones.
  const sources = useSources();
  const offered = useProjectSources(projectId);
  const bound = useBoundData(template, previewData);
  const values = useMemo(() => ({ ...previewData, ...bound }), [previewData, bound]);
  const cv = useCanvasView(`data.${template.id}`, '.logic-node', { onArrange: () => arrange() });
  const [layout, setLayoutState] = useState<Layout>(() => loadLayout(template.id));
  const [drag, setDrag] = useState<{ wire: Wire; x: number; y: number } | null>(null);
  const [ghost, setGhost] = useState<{ label: string; x: number; y: number } | null>(null);
  // The columns each data set node shows (from its preview), by node id.
  const [dsColumns, setDsColumns] = useState<Record<string, string[]>>({});
  // The Table whose "Build the table" card is open.
  const [building, setBuilding] = useState<string | null>(null);
  // Where each port's dot is in its node, measured once drawn: the wires start and end on the dots,
  // whatever the rows, sizes and fonts of the node.
  const spaceRef = useRef<HTMLDivElement>(null);
  const [portAt, setPortAt] = useState<Record<string, PortSpot>>({});
  useLayoutEffect(() => {
    const space = spaceRef.current;
    if (!space) return;
    const measure = () => {
      const zoom = space.getBoundingClientRect().width / space.offsetWidth || 1;
      const next: Record<string, PortSpot> = {};
      for (const el of space.querySelectorAll<HTMLElement>('.port[data-wire]')) {
        const node = el.closest<HTMLElement>('[data-node-id]');
        if (!node) continue;
        const n = node.getBoundingClientRect();
        const r = el.getBoundingClientRect();
        const half = (v: number) => Math.round((v / zoom) * 2) / 2;
        next[el.dataset.wire!] = { node: node.dataset.nodeId!, x: half(r.left + r.width / 2 - n.left), y: half(r.top + r.height / 2 - n.top) };
      }
      setPortAt((cur) => (sameSpots(cur, next) ? cur : next));
    };
    measure();
    // Nodes whose content changes size by itself (a preview loaded, a picture).
    const ro = new ResizeObserver(measure);
    for (const n of space.querySelectorAll('.logic-node')) ro.observe(n);
    return () => ro.disconnect();
  });
  const logic: Logic = template.logic ?? { nodes: [], edges: [] };
  const migrated = useMemo(() => (template.dataflow ? null : migrate(template)), [template]);
  const flow: DataFlow = template.dataflow ?? migrated!;

  const setLayout = (fn: (l: Layout) => Layout) =>
    setLayoutState((cur) => {
      const next = fn(cur);
      try {
        localStorage.setItem(`pulse.dataCanvas.${template.id}`, JSON.stringify(next));
      } catch {
        /* private mode */
      }
      return next;
    });

  // Values at the playhead: layer properties and logic nodes.
  const resolved = useMemo(() => {
    const hold = holdFrame(template);
    const m = new Map(template.layers.map((l) => [l.id, resolveLayer(l, frame, values, hold)]));
    applyLogic(template.logic, m, values, measureLayer(template.layers, values, interpolateText));
    return m;
  }, [template, frame, values]);

  // ---- What the canvas shows and where.
  /** The keys from a source or a data set wired at a port of `l` (its new values can move on air). */
  const liveKeys = (l: Layer, port: string): string[] => {
    const keys = contentOf(l).includes(port) ? [...contentValue(l, port).matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]) : port.startsWith('s:') ? [l.settingBindings?.[port.slice(2)] ?? ''] : [l.bindings[port as Prop] ?? ''];
    return [...new Set(keys.filter((k) => k && fedKey(template, k)))];
  };
  const wiredPorts = (l: Layer) => new Set<string>([...(Object.entries(l.bindings) as [Prop, string][]).filter(([, k]) => k).map(([p]) => p), ...contentOf(l).filter((c) => /\{\{\w+\}\}/.test(contentValue(l, c))), ...settingPortsOf(l)]);
  /** Properties with a port: the usual ones, and any other one set from the data (anchor, crop… exposed in the Inspector). */
  const propPortsOf = (l: Layer) => [...portsOf(l), ...(Object.entries(l.bindings) as [Prop, string][]).filter(([p, k]) => k && !portsOf(l).includes(p)).map(([p]) => p)];
  /** Content ports always; the settings set from the data; properties when unfolded (or wired). */
  const ports = (l: Layer) => {
    const wired = wiredPorts(l);
    return [...contentOf(l), ...settingPortsOf(l), ...propPortsOf(l).filter((p) => layout.collapsed[l.id] === false || wired.has(p))];
  };
  /** The keys from a source or a data set wired at a port (not typed by the Operator): its transition rows under it. */
  const liveAt = (l: Layer, port: string): string[] => (wiredPorts(l).has(port) && !exposureAt(l, port) ? liveKeys(l, port) : []);
  const portHeight = (l: Layer, port: string) => (contentOf(l).includes(port) ? CROW : ROW) + (liveAt(l, port).length ? LIVE_H : 0);
  const portTop = (l: Layer, port: string) => {
    let y = HEAD;
    for (const p of ports(l)) {
      if (p === port) return y;
      y += portHeight(l, p);
    }
    return -1;
  };
  const layerHeight = (l: Layer) => HEAD + ports(l).reduce((h, p) => h + portHeight(l, p), 0) + 30 + 26 + (isTimed(l) ? 40 : 0);

  const widthOf = (id: string, def: number) => layout.size[id]?.w ?? def;
  const nodePos = new Map<string, Pt>();
  const stack = (x: number, items: [string, number][]) => {
    let y = 40;
    for (const [id, h] of items) {
      nodePos.set(id, layout.pos[id] ?? { x, y });
      y += h + 24;
    }
  };
  // Rows the Operator types into a table are not variables of the data (other exposed settings are, wired to their setting).
  const exposed = new Set(template.layers.flatMap((l) => Object.entries(l.settingBindings ?? {}).flatMap(([p, k]) => (HAND_ONLY.has(p) ? [k] : []))));
  // Values exposed to the Operator for one port of a layer: in that layer's node.
  const folded = exposuresOf(template, flow);
  /** The field exposed to the Operator at a port of `l`, if any. */
  const exposureAt = (l: Layer, port: string) => {
    for (const [k, e] of folded) if (e.layer === l.id && e.port === port) return template.fields.find((f) => f.key === k);
    return undefined;
  };
  const shownFields = template.fields.filter((f) => !exposed.has(f.key) && !folded.has(f.key));
  const fieldHeight = (f: Field) => HEAD + (f.binding ? 90 : f.kind === 'number' ? 130 : f.kind === 'image' ? 150 : 96);
  stack(
    COLS.field,
    shownFields.map((f) => [`f:${f.key}`, fieldHeight(f)]),
  );
  stack(
    COLS.layer,
    template.layers.map((l) => [`l:${l.id}`, layerHeight(l)]),
  );
  for (const n of flow.nodes) nodePos.set(n.id, { x: n.x, y: n.y });
  // A data set is drawn as wide as a source.
  const flowWidth = (n: FlowNode) => widthOf(n.id, n.type === 'source' || n.type === 'dataset' ? W.source : W.step);

  // Port positions: the dot measured in its node, else where the rows should put it (first drawing).
  const spot = (wire: string, fallback: Pt): Pt => {
    const m = portAt[wire];
    const p = m && nodePos.get(m.node);
    return p ? { x: p.x + m.x, y: p.y + m.y } : fallback;
  };
  const outOf = (id: string, w: number) => {
    const p = nodePos.get(id)!;
    return spot(`o:${id}`, { x: p.x + w, y: p.y + HEAD / 2 });
  };
  const inOf = (id: string) => {
    const p = nodePos.get(id)!;
    return spot(`i:${id}`, { x: p.x, y: p.y + HEAD / 2 });
  };
  const layerIn = (l: Layer, port: string): Pt | null => {
    const p = nodePos.get(`l:${l.id}`);
    const top = portTop(l, port);
    if (!p || top < 0) return null;
    return spot(`i:l:${l.id}:${port}`, { x: p.x, y: p.y + top + ROW / 2 });
  };
  const logicOut = (n: LogicNode) => ({ x: n.x + widthOf(n.id, W.logic), y: n.y + HEAD / 2 });

  // ---- Editing the template.
  const editLogic = (fn: (l: Logic) => void, key?: string) =>
    update((t) => {
      t.logic ??= { nodes: [], edges: [] };
      fn(t.logic);
      // Field and set-property nodes only exist through their wires.
      t.logic.nodes = t.logic.nodes.filter((n) => (n.type === 'field' ? t.logic!.edges.some((e) => e.from === n.id) : n.type === 'target' ? t.logic!.edges.some((e) => e.to === n.id) : true));
      if (!t.logic.nodes.length) delete t.logic;
    }, key);
  const setNode = (id: string, patch: Partial<LogicNode>, key?: string) =>
    editLogic((l) => {
      const n = l.nodes.find((x) => x.id === id);
      if (n) Object.assign(n, patch);
    }, key);
  /** Changes the data graph, then the bindings it gives. */
  const editFlow = (fn: (f: DataFlow) => void, key?: string) => update((t) => editDataflow(t, fn, migrated), key);
  const setFlowNode = (id: string, fn: (n: FlowNode) => void, key?: string) =>
    editFlow((f) => {
      const n = f.nodes.find((x) => x.id === id);
      if (n) fn(n);
    }, key);
  const editField = (key: string, fn: (f: Field) => void, coalesce?: string) => update((t) => fn(t.fields.find((f) => f.key === key)!), coalesce);
  /** Stops exposing to the Operator the value at `port` of `l`, as the Inspector's (•) does: the layer keeps the value set so far, the field goes. */
  const unexpose = (l: Layer, port: string, key: string) =>
    update((t) => {
      const x = findLayer(t, l.id)!;
      const def = t.fields.find((f) => f.key === key)?.default ?? '';
      if (port.startsWith('s:')) unbindSetting(x, port.slice(2));
      else if (contentOf(x).includes(port)) {
        if (port === 'items' && !textItems(x)) (x as unknown as { items: Item[] }).items = def.split('\n').filter(Boolean).map((text) => ({ cat: '', text, on: true }));
        else setPath(x, port, def);
      } else delete x.bindings[port as Prop];
      dropField(t, key);
    });
  const newField = (t: Template, label: string, kind: FieldKind, def: string): string => {
    const key = fieldKey(label, new Set(t.fields.map((f) => f.key)));
    t.fields.push({ key, label, kind, default: def });
    return key;
  };

  // Data set columns: where their ports are (the node reports its columns).
  const columnPorts = new Map<string, Pt>();
  // The data set node each of those keys comes from.
  const columnNode = new Map<string, string>();
  for (const n of flow.nodes)
    if (n.type === 'dataset') {
      // All the rows first (for tables, credits…), then each column of the current row.
      const at = (i: number, key: string) => spot(`o:k:${key}`, { x: n.x + flowWidth(n), y: n.y + HEAD + i * DS_ROW + DS_ROW / 2 });
      columnPorts.set(tableKey(n.key), at(0, tableKey(n.key)));
      columnPorts.set(linesKey(n.key), at(0, tableKey(n.key)));
      (dsColumns[n.id] ?? []).forEach((c, i) => columnPorts.set(columnKey(n.key, c), at(i + 1, columnKey(n.key, c))));
      for (const k of [tableKey(n.key), linesKey(n.key), ...(dsColumns[n.id] ?? []).map((c) => columnKey(n.key, c))]) columnNode.set(k, n.id);
    }
  /** Where a value wired by key starts: a variable, or a data set column. */
  const keyOut = (key: string): Pt | null => (nodePos.has(`f:${key}`) ? outOf(`f:${key}`, widthOf(`f:${key}`, W.field)) : (columnPorts.get(key) ?? null));

  // ---- Wires of the template.
  const wires: { d: string; title: string; onClick: () => void; data?: boolean; edge?: number }[] = [];
  // The same wires node to node, for Arrange.
  const links: LayoutEdge[] = [];
  flow.edges.forEach((e, i) => {
    const a = flow.nodes.find((n) => n.id === e.from);
    if (!a) return;
    const to = e.to.startsWith('field:') ? (nodePos.has(`f:${e.to.slice(6)}`) ? inOf(`f:${e.to.slice(6)}`) : null) : nodePos.has(e.to) ? inOf(e.to) : null;
    if (!to) return;
    links.push({ from: a.id, to: e.to.startsWith('field:') ? `f:${e.to.slice(6)}` : e.to });
    wires.push({ d: curve(outOf(a.id, flowWidth(a)), to), data: true, edge: i, title: t('Data (click to remove · drop a Filter, Sort or Transform on it)'), onClick: () => editFlow((f) => void f.edges.splice(i, 1)) });
  });
  for (const l of template.layers)
    for (const port of [...contentOf(l), ...settingPortsOf(l), ...propPortsOf(l)]) {
      const to = layerIn(l, port);
      if (!to) continue;
      const content = contentOf(l).includes(port);
      const setting = port.startsWith('s:') ? port.slice(2) : null;
      // From a variable or a data set column (an exposed value is in the node itself: no wire).
      const keys = (content ? [...template.fields.map((f) => f.key), ...columnPorts.keys()].filter((k) => tokensIn(l, port, k)) : [setting ? l.settingBindings?.[setting] : l.bindings[port as Prop]]).filter((k): k is string => !!k && !!keyOut(k));
      for (const key of keys) {
        links.push({ from: nodePos.has(`f:${key}`) ? `f:${key}` : (columnNode.get(key) ?? ''), to: `l:${l.id}` });
        wires.push({
          d: curve(keyOut(key)!, to),
          data: columnPorts.has(key),
          title: t('{key} → {layer} {port} (click to remove)', { key, layer: l.name, port: content ? port : setting ? settingLabel(l, setting, t).label : t(PROP_LABELS[port as Prop]) }),
          onClick: () =>
            update((t) => {
              const x = findLayer(t, l.id)!;
              if (content) unwireContent(x, port, key);
              else if (setting) unbindSetting(x, setting);
              else delete x.bindings[port as Prop];
            }),
        });
      }
    }
  const nodes = new Map(logic.nodes.map((n) => [n.id, n]));

  /**
   * Lays the nodes out from left to right: sources first, the layers last,
   * the steps, data sets and variables in between where their wires go.
   */
  const arrange = () => {
    const size = measureNodes(cv.ref.current);
    const all = [...flow.nodes.map((n) => n.id), ...shownFields.map((f) => `f:${f.key}`), ...template.layers.map((l) => `l:${l.id}`)];
    const at = autoLayout(
      all.map((id) => ({ id, ...(size[id] ?? { w: W.step, h: 160 }), side: id.startsWith('l:') ? ('end' as const) : flow.nodes.some((n) => n.id === id && n.type === 'source') ? ('start' as const) : undefined })),
      links,
      { tightSources: true },
    );
    useDesigner.setState({ lastCoalesce: null });
    if (flow.nodes.some((n) => at[n.id]))
      editFlow((f) => {
        for (const n of f.nodes) if (at[n.id]) Object.assign(n, at[n.id]);
      });
    setLayout((l) => ({ ...l, pos: { ...l.pos, ...Object.fromEntries(all.filter((id) => /^[fl]:/.test(id) && at[id]).map((id) => [id, at[id]])) } }));
  };

  // ---- Wiring with the mouse.
  const point = cv.toWorld;
  const wireStart = (w: Wire): Pt | null => {
    if (w.kind === 'flow') {
      const n = flow.nodes.find((x) => x.id === w.id);
      return n ? outOf(n.id, flowWidth(n)) : null;
    }
    if (w.kind === 'field') return keyOut(w.key);
    const n = nodes.get(w.id);
    return n ? logicOut(n) : null;
  };

  const connect = (w: Wire, target: HTMLElement | null, at: Pt) => {
    const port = target?.closest<HTMLElement>('.port.in');
    const d = port?.dataset ?? {};
    if (w.kind === 'flow') {
      const kind = kindOf(flow, w.id);
      if (d.flow) editFlow((f) => linkFlow(f, w.id, d.flow!));
      else if (d.field)
        update((t) => {
          const f: DataFlow = structuredClone(t.dataflow ?? migrated ?? { nodes: [], edges: [] });
          linkFlow(f, w.id, `field:${d.field}`);
          compile(t, f);
          // A text variable fed with pictures, numbers… takes their type.
          const x = t.fields.find((y) => y.key === d.field);
          if (x && kind && x.kind === 'text' && kind !== 'text') x.kind = kind;
        });
      else if (d.layer && d.port) {
        // On a value exposed to the Operator: the data feeds it from now on (a variable, before the layer).
        const l = findLayer(template, d.layer);
        const x = l && exposureAt(l, d.port);
        if (!l || !x) return;
        update((t) => {
          const f: DataFlow = structuredClone(t.dataflow ?? migrated ?? { nodes: [], edges: [] });
          linkFlow(f, w.id, `field:${x.key}`);
          compile(t, f);
          const y = t.fields.find((z) => z.key === x.key);
          if (y && kind && y.kind === 'text' && kind !== 'text') y.kind = kind;
        });
        const lp = nodePos.get(`l:${l.id}`)!;
        const place = freeSpot(document.querySelector('.data-canvas .logic-canvas'), { x: lp.x - W.field - 80, y: at.y - HEAD / 2 }, W.field);
        setLayout((s) => ({ ...s, pos: { ...s.pos, [`f:${x.key}`]: place } }));
      } else if (!target?.closest('.dc-node')) {
        // Dropped on empty space: a new variable fed from here.
        let made = '';
        update((t) => {
          made = newField(t, `Field ${t.fields.length + 1}`, kind ?? 'text', '');
          const f: DataFlow = structuredClone(t.dataflow ?? migrated ?? { nodes: [], edges: [] });
          linkFlow(f, w.id, `field:${made}`);
          compile(t, f);
        });
        const spot = freeSpot(document.querySelector('.data-canvas .logic-canvas'), { x: at.x, y: at.y - HEAD / 2 });
        setLayout((l) => ({ ...l, pos: { ...l.pos, [`f:${made}`]: spot } }));
      }
      return;
    }
    // Into an operation's input.
    if (d.logic && d.port) {
      editLogic((l) => {
        let from = w.kind === 'logic' ? w.id : '';
        if (w.kind === 'field') {
          const fp = keyOut(w.key) ?? { x: 0, y: 0 };
          from = l.nodes.find((n) => n.type === 'field' && n.key === w.key)?.id ?? '';
          if (!from) {
            from = newId();
            l.nodes.push({ id: from, type: 'field', key: w.key, x: fp.x, y: fp.y });
          }
        }
        if (!from || from === d.logic) return;
        l.edges = l.edges.filter((e) => !(e.to === d.logic && e.port === d.port));
        l.edges.push({ from, to: d.logic!, port: d.port! });
      });
      return;
    }
    // Into a layer's port.
    const layerId = d.layer;
    const p = d.port;
    if (!layerId || !p) return;
    // A value exposed to the Operator there: the data takes its place, the exposed field goes.
    const before = findLayer(template, layerId);
    const old = before && exposureAt(before, p);
    const replaced = old && w.kind === 'field' && old.key !== w.key ? old.key : null;
    if (w.kind === 'field' && p.startsWith('s:')) {
      update((t) => {
        bindSetting(t, findLayer(t, layerId)!, p.slice(2), w.key);
        if (replaced) dropField(t, replaced);
      });
      return;
    }
    if (p.startsWith('s:')) return;
    if (w.kind === 'field') {
      update((t) => {
        const l = findLayer(t, layerId)!;
        if (contentOf(l).includes(p)) {
          if (replaced) unwireContent(l, p, replaced);
          wireContent(l, p, w.key);
        } else l.bindings[p as Prop] = w.key;
        if (replaced) dropField(t, replaced);
        // Its rows now come from the data: the operator no longer types them.
        if (l.type === 'table' && takesTable(l, p)) unexposeRows(t, l);
      });
      // All rows into a table with no columns yet: offer to build it.
      const l = findLayer(template, layerId);
      if (l?.type === 'table' && takesTable(l, p) && w.key.endsWith('__table') && !(l as { cols?: unknown[] }).cols?.length) setBuilding(layerId);
    } else if (w.kind === 'logic' && !contentOf(findLayer(template, layerId)!).includes(p)) {
      editLogic((l) => {
        let to = l.nodes.find((n) => n.type === 'target' && n.layer === layerId && n.prop === p)?.id;
        if (!to) {
          to = newId();
          const lp = nodePos.get(`l:${layerId}`)!;
          l.nodes.push({ id: to, type: 'target', layer: layerId, prop: p as Prop, mode: 'set', x: lp.x, y: lp.y });
        }
        l.edges = l.edges.filter((e) => !(e.to === to && e.port === 'in'));
        l.edges.push({ from: w.id, to, port: 'in' });
      });
    }
  };

  const startWire = (e: React.PointerEvent, wire: Wire) => {
    e.stopPropagation();
    e.preventDefault();
    setDrag({ wire, ...point(e.clientX, e.clientY) });
    const move = (ev: PointerEvent) => setDrag({ wire, ...point(ev.clientX, ev.clientY) });
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      setDrag(null);
      connect(wire, document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null, point(ev.clientX, ev.clientY));
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  /** Drags a node: of the layout, of the data graph, or of the logic graph. */
  const dragNode = (e: React.PointerEvent, id: string, kind: 'layout' | 'flow' | 'logic') => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('select, input, button, .port, textarea, label, [data-scroll], .cv-resize')) return;
    e.preventDefault();
    const start = point(e.clientX, e.clientY);
    const base = nodePos.get(id) ?? { x: nodes.get(id)!.x, y: nodes.get(id)!.y };
    useDesigner.setState({ lastCoalesce: null });
    document.body.classList.add('cv-moving');
    const move = (ev: PointerEvent) => {
      const p = point(ev.clientX, ev.clientY);
      const at = { x: Math.round(base.x + p.x - start.x), y: Math.round(base.y + p.y - start.y) };
      if (kind === 'logic') setNode(id, at, `data-move-${id}`);
      else if (kind === 'flow') setFlowNode(id, (n) => void Object.assign(n, at), `flow-move-${id}`);
      else setLayout((l) => ({ ...l, pos: { ...l.pos, [id]: at } }));
    };
    const up = () => {
      document.body.classList.remove('cv-moving');
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };
  const resizer = (id: string, height = true) => <ResizeCorner onPointerDown={(e) => cv.resize(e, (w, h) => setLayout((l) => ({ ...l, size: { ...l.size, [id]: height ? { w, h } : { w } } })))} />;
  const sizeStyle = (id: string, def: number) => ({ width: widthOf(id, def), height: layout.size[id]?.h });

  /** Somewhere free in view, for a node added with a click. */
  const inView = () => {
    const v = cv.visible();
    return { x: Math.round(v.x + v.w / 2 - 160), y: Math.round(v.y + 80 + ((flow.nodes.length * 30) % 200)) };
  };
  /** The first free place in view (from the top left) for a node `w` wide; below the others when the view is full. */
  const freeInView = (w: number, h = 120): Pt => {
    const world = document.querySelector('.data-canvas .logic-canvas');
    const rects = [...(world?.querySelectorAll<HTMLElement>('.logic-node') ?? [])].map((el) => ({ x: el.offsetLeft, y: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight }));
    const free = (x: number, y: number) => !rects.some((r) => x < r.x + r.w + 24 && x + w + 24 > r.x && y < r.y + r.h + 24 && y + h + 24 > r.y);
    const v = cv.visible();
    const pad = 32;
    for (let y = v.y + pad; y + h <= v.y + v.h - pad; y += 24) for (let x = v.x + pad; x + w <= v.x + v.w - pad; x += 24) if (free(x, y)) return { x: Math.round(x), y: Math.round(y) };
    return freeSpot(world, inView(), w, h);
  };
  /** A node already on the canvas: brought into view and flashed. */
  const focusNode = (id: string) => {
    const el = document.querySelector<HTMLElement>(`.data-canvas [data-node-id="${CSS.escape(id)}"]`);
    const vp = cv.ref.current;
    if (!el || !vp) return;
    const v = cv.visible();
    const inside = el.offsetLeft >= v.x && el.offsetTop >= v.y && el.offsetLeft + el.offsetWidth <= v.x + v.w && el.offsetTop + Math.min(el.offsetHeight, 160) <= v.y + v.h;
    if (!inside) {
      const zoom = cv.view.zoom;
      cv.setView({ zoom, x: vp.clientWidth / 2 - (el.offsetLeft + el.offsetWidth / 2) * zoom, y: vp.clientHeight / 3 - el.offsetTop * zoom });
    }
    el.classList.remove('ux2-flash');
    void el.offsetWidth;
    el.classList.add('ux2-flash');
  };
  /**
   * Adds a node from the palette: a click puts it in view, a drag where it
   * is dropped — on a data wire, it goes in between.
   */
  const fromPalette = (e: React.PointerEvent, label: string, add: (at: Pt, edge?: number) => void, width: number = W.step, onClick?: () => void) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const start = { x: e.clientX, y: e.clientY };
    let moved = false;
    const move = (ev: PointerEvent) => {
      moved ||= Math.hypot(ev.clientX - start.x, ev.clientY - start.y) > 5;
      if (moved) setGhost({ label, x: ev.clientX, y: ev.clientY });
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      setGhost(null);
      const world = document.querySelector('.data-canvas .logic-canvas');
      if (!moved) return onClick ? onClick() : add(freeInView(width));
      const el = document.elementFromPoint(ev.clientX, ev.clientY) as Element | null;
      if (!el?.closest('.logic-canvas')) return;
      const edge = el.closest('[data-edge]')?.getAttribute('data-edge');
      const p = point(ev.clientX, ev.clientY);
      const drop = { x: Math.round(p.x - 30), y: Math.round(p.y - HEAD / 2) };
      add(edge != null ? drop : freeSpot(world, drop), edge != null ? +edge : undefined);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };
  const addStep = (type: StepType) => (at: Pt, edge?: number) => {
    if (edge === undefined) return editFlow((f) => void f.nodes.push({ id: newId(), ...at, ...STEP[type].make() } as FlowNode));
    // On a wire: in between, and what is on its right moves aside to make room.
    const shift = W.step + 60;
    const x0 = at.x - 20;
    update((t) => {
      const f: DataFlow = structuredClone(t.dataflow ?? migrated ?? { nodes: [], edges: [] });
      for (const n of f.nodes) if (n.x >= x0) n.x += shift;
      for (const n of t.logic?.nodes ?? []) if (n.x >= x0) n.x += shift;
      const n = { id: newId(), ...at, ...STEP[type].make() } as FlowNode;
      f.nodes.push(n);
      const e = f.edges[edge];
      if (e) f.edges.splice(edge, 1, { from: e.from, to: n.id }, { from: n.id, to: e.to });
      compile(t, f);
    });
    const moved: Layout['pos'] = {};
    for (const [id, p] of nodePos) if ((id.startsWith('f:') || id.startsWith('l:')) && p.x >= x0) moved[id] = { x: p.x + shift, y: p.y };
    setLayout((l) => ({ ...l, pos: { ...l.pos, ...moved } }));
  };
  const addDataset = (at: Pt, edge?: number) => {
    const name = t('Data set {n}', { n: flow.nodes.filter((n) => n.type === 'dataset').length + 1 });
    editFlow((f) => {
      const n = newDataset(template, f, name, at);
      // Dropped on a wire: fed from where it starts.
      const e = edge !== undefined ? f.edges[edge] : undefined;
      if (e) f.edges.push({ from: e.from, to: n.id });
    });
  };
  const addSource = (source: string) => (at: Pt) => editFlow((f) => void f.nodes.push({ id: newId(), ...at, type: 'source', source, list: '' }));

  const dragFrom = drag && wireStart(drag.wire);
  const removeFlowNode = (id: string) => {
    // A data set: its rows and columns are no longer anywhere.
    const ds = flow.nodes.find((n) => n.id === id && n.type === 'dataset') as { key: string } | undefined;
    if (ds) update((t) => forgetKeys(t, (k) => k.startsWith(`${ds.key}_`)));
    removeFlow(id);
  };
  const removeFlow = (id: string) =>
    editFlow((f) => {
      // The wires through it are kept: what fed it feeds what it fed.
      const into = f.edges.find((e) => e.to === id)?.from;
      const outs = f.edges.filter((e) => e.from === id).map((e) => e.to);
      f.nodes = f.nodes.filter((n) => n.id !== id);
      f.edges = f.edges.filter((e) => e.from !== id && e.to !== id);
      if (into) for (const to of outs) f.edges.push({ from: into, to });
    });

  return (
    <div className="logic-editor data-canvas" onKeyDown={(e) => e.stopPropagation()}>
      <CanvasHeader icon={<Database size={15} />} title={t('Data')} subject={template.name} onClose={onClose} />
      <div className="logic-body">
        <aside className="logic-palette">
          <button
            className="kind-field"
            onPointerDown={(e) =>
              fromPalette(
                e,
                t('Variable'),
                (at) => {
                  let made = '';
                  update((tpl) => void (made = newField(tpl, t('Field {n}', { n: tpl.fields.length + 1 }), 'text', '')));
                  setLayout((l) => ({ ...l, pos: { ...l.pos, [`f:${made}`]: at } }));
                },
                W.field,
              )
            }
          >
            <Variable size={14} />
            <span>
              <strong>{t('Variable')}</strong>
            </span>
          </button>

          <div className="dc-pal-title">{t('Data sources')}</div>
          {offered.map((s, i) => (
            <span key={s.id} style={{ display: 'contents' }}>
              {!s.project && (i === 0 || offered[i - 1].project) && <div className="dc-pal-sub">{t('Shared')}</div>}
              <button
                className="kind-source"
                onPointerDown={(e) => {
                  // Already on the canvas: a click shows it (a drag still adds another).
                  const there = flow.nodes.find((n) => n.type === 'source' && n.source === s.id);
                  fromPalette(e, s.name, addSource(s.id), W.source, there && (() => focusNode(there.id)));
                }}
              >
                <Database size={14} />
                <span>
                  <strong>{s.name}</strong>
                  <em>{s.type.toUpperCase()}</em>
                </span>
              </button>
            </span>
          ))}
          {!offered.length && (
            <p className="hint">
              {t('No source yet:')}{' '}
              <a href="#" onClick={(e) => (e.preventDefault(), projectId && navigate(projectPath(projectId, 'data')))}>
                {t('add one in Connectors')}
              </a>{' '}
              {t('(this project) or in the main menu, Data (shared).')}
            </p>
          )}

          <div className="dc-pal-title">{t('On the way')}</div>
          {(['filter', 'sort', 'transform'] as StepType[]).map((type) => {
            const k = STEP[type];
            return (
              <button key={type} className={`kind-${type}`} onPointerDown={(e) => fromPalette(e, t(k.label), addStep(type))}>
                <k.icon size={14} />
                <span>
                  <strong>{t(k.label)}</strong>
                </span>
              </button>
            );
          })}

          <div className="dc-pal-title">{t('Into the graphic')}</div>
          <button className="kind-dataset" onPointerDown={(e) => fromPalette(e, t('Data set'), addDataset, W.source)}>
            <Rows3 size={14} />
            <span>
              <strong>{t('Data set')}</strong>
            </span>
          </button>
        </aside>

        <div {...cv.viewport} className={`logic-canvas ${cv.viewport.className}`}>
          <div className="logic-space" ref={spaceRef} data-canvas-void style={cv.spaceStyle}>
            <svg className="logic-wires">
              {wires.map((w, i) => (
                <g key={i} className="wire-g">
                  <path d={w.d} className={`wire ${w.data ? 'data' : ''}`} />
                  {/* A wider line to click, or to drop a step on. */}
                  <path d={w.d} data-edge={w.edge} className="wire-hit" onPointerDown={(e) => (e.stopPropagation(), w.onClick())}>
                    <title>{w.title}</title>
                  </path>
                </g>
              ))}
              {drag && dragFrom && <path d={curve(dragFrom, drag)} className="wire live" />}
            </svg>

            {/* The data graph: sources and steps */}
            {flow.nodes.map((n) => {
              const chain = chainOf(flow, n.id);
              if (n.type === 'source') {
                const s = sources.find((x) => x.id === n.source);
                const used = flow.edges.some((e) => e.from === n.id);
                return (
                  <div key={n.id} data-node-id={n.id} className={`logic-node dc-node kind-source ${layout.size[n.id]?.h ? 'sized' : ''}`} style={{ left: n.x, top: n.y, ...sizeStyle(n.id, W.source) }} onPointerDown={(e) => dragNode(e, n.id, 'flow')}>
                    <div className="ln-head" style={{ height: HEAD }}>
                      <Database size={12} />
                      <span>{s?.name ?? t('Missing: {source}', { source: n.source })}</span>
                      <button className="ln-del" title={used ? t('Remove (and its wires)') : t('Remove')} onClick={() => removeFlowNode(n.id)}>
                        <Trash2 size={11} />
                      </button>
                      <span className="port out" data-wire={`o:${n.id}`} title={t('Drag to a filter, a sort, a transform or a variable')} onPointerDown={(e) => startWire(e, { kind: 'flow', id: n.id })} />
                    </div>
                    <div className="ln-body">
                      <span className="dc-meta">
                        <span className={`live-dot ${s?.error ? 'err' : ''}`} /> {s ? t('{type} · every {n} s', { type: s.type.toUpperCase(), n: s.interval }) : t('not found')}
                      </span>
                      <SourceBody node={n} version={s?.updatedAt ?? 0} data={values} onList={(list) => setFlowNode(n.id, (x) => void ((x as { list: string }).list = list))} />
                    </div>
                    {resizer(n.id)}
                  </div>
                );
              }
              if (n.type === 'dataset') {
                const into = flow.edges.find((e) => e.to === n.id)?.from;
                return (
                  <div key={n.id} data-node-id={n.id} className="logic-node dc-node kind-dataset" style={{ left: n.x, top: n.y, width: widthOf(n.id, W.source) }} onPointerDown={(e) => dragNode(e, n.id, 'flow')}>
                    <div className="ln-head" style={{ height: HEAD }}>
                      <span className="port in" data-flow={n.id} data-wire={`i:${n.id}`} title={t('Drop a source or a step here')} />
                      <Rows3 size={12} />
                      <input className="dc-title" value={n.name} onChange={(e) => setFlowNode(n.id, (x) => void ((x as { name: string }).name = e.target.value), `ds-name-${n.id}`)} title={t('Key: {key}', { key: n.key })} />
                      <button className="ln-del" title={t('Remove')} onClick={() => removeFlowNode(n.id)}>
                        <Trash2 size={11} />
                      </button>
                    </div>
                    <DataSetBody
                      node={n}
                      chain={into ? chainOf(flow, into) : null}
                      data={values}
                      onColumns={(cols) => setDsColumns((m) => (m[n.id]?.join('\n') === cols.join('\n') ? m : { ...m, [n.id]: cols }))}
                      onRow={(row) => useDesigner.setState((st) => ({ previewData: { ...st.previewData, [rowKey(n.key)]: String(row) } }))}
                      startWire={(e, key) => startWire(e, { kind: 'field', key })}
                    />
                    {resizer(n.id, false)}
                  </div>
                );
              }
              const k = STEP[n.type];
              return (
                <div key={n.id} data-node-id={n.id} className={`logic-node dc-node kind-${n.type} ${layout.size[n.id]?.h ? 'sized' : ''}`} style={{ left: n.x, top: n.y, ...sizeStyle(n.id, W.step) }} onPointerDown={(e) => dragNode(e, n.id, 'flow')}>
                  <div className="ln-head" style={{ height: HEAD }}>
                    <span className="port in" data-flow={n.id} data-wire={`i:${n.id}`} title={t('Drop a source or a step here')} />
                    <k.icon size={12} />
                    <span>{t(k.label)}</span>
                    <button className="ln-del" title={t('Remove (the wires go through)')} onClick={() => removeFlowNode(n.id)}>
                      <Trash2 size={11} />
                    </button>
                    <span className="port out" data-wire={`o:${n.id}`} title={t('Drag to a step or a variable')} onPointerDown={(e) => startWire(e, { kind: 'flow', id: n.id })} />
                  </div>
                  <div className="ln-body">
                    {chain ? <StepBody source={chain.source} steps={chain.steps} data={values} node={n as FlowNode & DataStep} onChange={(fn) => setFlowNode(n.id, fn as (x: FlowNode) => void, `flow-${n.id}`)} /> : <span className="dc-meta">{t('Wire a source (or a step) into it.')}</span>}
                  </div>
                  {resizer(n.id)}
                </div>
              );
            })}

            {/* Variables */}
            {shownFields.map((f) => {
              const p = nodePos.get(`f:${f.key}`)!;
              const v = values[f.key] ?? '';
              const fed = flow.edges.some((e) => e.to === `field:${f.key}`);
              const legacy = f.binding && !isTable(f.binding) && !fed;
              return (
                <div key={f.key} data-node-id={`f:${f.key}`} className="logic-node dc-node kind-field" style={{ left: p.x, top: p.y, ...sizeStyle(`f:${f.key}`, W.field) }} onPointerDown={(e) => dragNode(e, `f:${f.key}`, 'layout')}>
                  <div className="ln-head" style={{ height: HEAD }}>
                    <span className="port in" data-field={f.key} data-wire={`i:f:${f.key}`} title={t('Drop a source or a step here')} />
                    <Variable size={12} />
                    <input className="dc-title" value={f.label} onChange={(e) => editField(f.key, (x) => void (x.label = e.target.value), `label-${f.key}`)} title={t('Key: {key}', { key: f.key })} />
                    <button
                      className="ln-del"
                      title={t('Delete the variable')}
                      onClick={() =>
                        update((t) => {
                          t.fields = t.fields.filter((x) => x.key !== f.key);
                          forgetKeys(t, (k) => k === f.key);
                          if (t.dataflow) {
                            t.dataflow.edges = t.dataflow.edges.filter((e) => e.to !== `field:${f.key}`);
                          }
                        })
                      }
                    >
                      <Trash2 size={11} />
                    </button>
                    <span className="port out" data-wire={`o:f:${f.key}`} title={t("Drag to a layer's value or to an operation")} onPointerDown={(e) => startWire(e, { kind: 'field', key: f.key })} />
                  </div>
                  <div className="ln-body">
                    <div className="row">
                      <Dropdown value={f.kind} options={FIELD_KINDS.map((o) => ({ ...o, label: t(o.label) }))} onChange={(k) => editField(f.key, (x) => void (x.kind = k))} />
                      <code className="dc-key" title={t('Key (API, MOS)')}>
                        {f.key}
                      </code>
                    </div>
                    {f.binding ? (
                      <>
                        {cellType(v) === 'image' ? (
                          <span className="dc-result img" title={v}>
                            <img src={v} alt="" />
                          </span>
                        ) : (
                          <span className="dc-result" title={v}>
                            {v === '' ? <em>{t('(empty)')}</em> : v}
                          </span>
                        )}
                        {legacy && (
                          <span className="dc-meta" title={t('Made before the tables: unlink it and wire the data again to change it')}>
                            {t('Path {path}', { path: f.binding.path })}
                            <button className="dc-addfilter" onClick={() => update((t) => void delete t.fields.find((x) => x.key === f.key)!.binding)}>
                              {t('Unlink')}
                            </button>
                          </span>
                        )}
                      </>
                    ) : (
                      <DefaultEditor field={f} onChange={(fn, key) => editField(f.key, fn, key)} />
                    )}
                  </div>
                  {resizer(`f:${f.key}`, false)}
                </div>
              );
            })}

            {/* Layers: what they show (text, picture); their properties folded */}
            {template.layers.map((l) => {
              const p = nodePos.get(`l:${l.id}`)!;
              const Icon = layerIcon(l);
              const wired = wiredPorts(l);
              const r = resolved.get(l.id);
              const hidden = portsOf(l).filter((x) => !ports(l).includes(x)).length;
              return (
                <div key={l.id} data-node={`l:${l.id}`} data-node-id={`l:${l.id}`} className="logic-node dc-node kind-layer" style={{ left: p.x, top: p.y, width: widthOf(`l:${l.id}`, W.layer) }} onPointerDown={(e) => dragNode(e, `l:${l.id}`, 'layout')}>
                  <div className="ln-head" style={{ height: HEAD }}>
                    <Icon size={12} />
                    <span>{l.name}</span>
                  </div>
                  {ports(l).map((port) => {
                    const content = contentOf(l).includes(port);
                    // A value the Operator types (exposed in the Inspector): here, with its (•) to stop.
                    const op = exposureAt(l, port);
                    const opMark = op && <OperatorMark field={op} onStop={() => unexpose(l, port, op.key)} />;
                    const live = liveAt(l, port);
                    const liveRows = live.length > 0 && <LiveRows template={template} layer={l} keys={live} />;
                    if (content) {
                      const raw = contentValue(l, port);
                      const isWired = wired.has(port) && !op;
                      const result = interpolateText(raw, values);
                      return (
                        <div key={port} className={`ln-content ${wired.has(port) ? 'wired' : ''}`} style={{ height: CROW + (live.length ? LIVE_H : 0) }}>
                          <div className="ln-input" style={{ height: ROW }}>
                            <span className="port in" data-layer={l.id} data-port={port} data-wire={`i:l:${l.id}:${port}`} title={takesTable(l, port) ? t("Drop a data set's All rows here (or a variable)") : t('Drop a variable here')} />
                            {port === 'src' ? <ImageIcon size={11} /> : takesTable(l, port) ? <Rows3 size={11} /> : <Type size={11} />} {CONTENT_LABEL[port] ? t(CONTENT_LABEL[port]) : port}
                            {isWired && <span className="dc-from">{t('from data')}</span>}
                            {op && (
                              <span className="dc-from" style={{ marginRight: 4, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={t('Key: {key}', { key: op.key })}>
                                {op.label}
                              </span>
                            )}
                            {opMark}
                          </div>
                          {op ? (
                            op.kind === 'multiline' || op.kind === 'table' || op.default.includes('\n') ? (
                              <span className="dc-value live" title={op.default}>
                                {tableSummary(op.default) ?? (op.default.length > 60 ? `${op.default.slice(0, 60)}…` : op.default.replace(/\n/g, ' ⏎ '))}
                              </span>
                            ) : (
                              <input className="dc-content-input" value={op.default} title={t('Value until the operator types another one')} onChange={(e) => editField(op.key, (x) => void (x.default = e.target.value), `default-${op.key}`)} />
                            )
                          ) : isWired && port === 'src' && cellType(result) === 'image' ? (
                            <span className="dc-value live dc-pic" title={result}>
                              <img src={result} alt="" /> {result.split('/').pop()}
                            </span>
                          ) : isWired ? (
                            <span className="dc-value live" title={t('Result: {value}', { value: result })}>
                              {result === '' ? t('(empty)') : (tableSummary(result) ?? (result.length > 60 ? `${result.slice(0, 60)}…` : result.replace(/\n/g, ' ⏎ ')))}
                            </span>
                          ) : port === 'items' && !textItems(l) ? (
                            <span className="dc-value">{raw ? tn(raw.split('\n').length, '{n} item (Inspector)', '{n} items (Inspector)') : t('No items')}</span>
                          ) : (
                            <input
                              className="dc-content-input"
                              value={raw}
                              placeholder={takesTable(l, port) ? t("A data set's All rows") : undefined}
                              title={t('Default value (until a variable is wired)')}
                              onChange={(e) => update((t) => setPath(findLayer(t, l.id), port, e.target.value), `content-${l.id}-${port}`)}
                            />
                          )}
                          {liveRows}
                        </div>
                      );
                    }
                    if (port.startsWith('s:')) {
                      const path = port.slice(2);
                      const key = l.settingBindings?.[path];
                      const sv = key !== undefined && values[key] !== undefined ? values[key] : getPath(l, path);
                      return (
                        <Fragment key={port}>
                        <div className="ln-input wired dc-setting" style={{ height: ROW }}>
                          <span className="port in" data-layer={l.id} data-port={port} data-wire={`i:l:${l.id}:${port}`} title={t('Drop a variable or a data set column here')} />
                          <span title={path}>{settingLabel(l, path, t).label.replace(`${t('General')} · `, '')}</span>
                          <span className="dc-port-value live" title={op ? t('Value until the operator types another one') : t('Value it gets from the data')}>
                            {isColorValue(sv) ? <span className="dc-swatch" style={{ background: String(sv) }} /> : String(sv ?? '').slice(0, 14)}
                          </span>
                          {opMark || (
                            <button className="dc-unset" title={t('Stop setting it from the data')} onClick={() => update((t) => unbindSetting(findLayer(t, l.id)!, path))}>
                              ×
                            </button>
                          )}
                        </div>
                        {liveRows}
                        </Fragment>
                      );
                    }
                    const v = r ? r[port as Prop] : undefined;
                    return (
                      <Fragment key={port}>
                      <div className={`ln-input ${wired.has(port) ? 'wired' : ''}`} style={{ height: ROW }}>
                        <span className="port in" data-layer={l.id} data-port={port} data-wire={`i:l:${l.id}:${port}`} title={t('Drop a variable or an operation here')} />
                        <span>{PROP_LABELS[port as Prop] ? t(PROP_LABELS[port as Prop]) : port}</span>
                        <span className={`dc-port-value ${wired.has(port) ? 'live' : ''}`} title={op ? t('Value until the operator types another one') : wired.has(port) ? t('Value it gets from the data') : t('Value in Properties')}>
                          {v === undefined ? '' : isColor(port) ? <span className="dc-swatch" style={{ background: toHex(col(v), true) }} /> : +num(v).toFixed(2)}
                        </span>
                        {opMark}
                      </div>
                      {liveRows}
                      </Fragment>
                    );
                  })}
                  <SettingAdd layer={l} onPick={(path) => update((t) => void ((findLayer(t, l.id)!.settingBindings ??= {})[path] ??= ''))} />
                  <button className="dc-props-toggle" onClick={() => setLayout((x) => ({ ...x, collapsed: { ...x.collapsed, [l.id]: x.collapsed[l.id] === false ? true : false } }))} title={t('Position, size, colours… (Properties)')}>
                    {hidden ? tn(hidden, '+ {n} property', '+ {n} properties') : t('− Fold the properties')}
                  </button>
                  {isTimed(l) && (
                    <div className="ln-body">
                      <WidgetRunButtons layer={l} compact />
                    </div>
                  )}
                  {l.type === 'table' && <TableNodeInfo layer={l} values={values} onBuild={() => setBuilding(l.id)} />}
                  {resizer(`l:${l.id}`, false)}
                </div>
              );
            })}
            {(() => {
              const l = building ? template.layers.find((x) => x.id === building) : undefined;
              if (!l) return null;
              return <BuildCard key={l.id} layer={l} template={template} values={values} onClose={() => setBuilding(null)} />;
            })()}
          </div>
          {/* Nothing yet: a small icon and a word, centred (as in the Operator). */}
          {!flow.nodes.length && !shownFields.length && !template.layers.length && <CanvasVoid icon={Database} text={t('No data')} />}
          {cv.controls}
        </div>
      </div>
      {ghost && (
        <div className="cv-ghost" style={{ left: ghost.x, top: ghost.y }}>
          {ghost.label}
        </div>
      )}
    </div>
  );
}

/** Latest value of a data source, for the tables and the sort's lists. */
const sourceValues = new Map<string, { version: number; value: unknown }>();
function useSourceValue(id: string, version: number): unknown {
  const [value, setValue] = useState<unknown>(sourceValues.get(id)?.value);
  useEffect(() => {
    const cached = sourceValues.get(id);
    if (cached && cached.version === version) return setValue(cached.value);
    let live = true;
    api('GET', `/api/data/sources/${encodeURIComponent(id)}/value`)
      .then((v) => {
        sourceValues.set(id, { version, value: v });
        if (live) setValue(v);
      })
      .catch(() => live && setValue(null));
    return () => {
      live = false;
    };
  }, [id, version]);
  return value;
}

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };
const isObj = (v: unknown): v is Record<string, Json> => !!v && typeof v === 'object' && !Array.isArray(v);

/** Dotted paths of the lists in a value (the rows a source node can read), the root first. */
function listsIn(v: unknown, path = '', depth = 0, out: string[] = []): string[] {
  if (Array.isArray(v)) {
    if (v.length) out.push(path);
  } else if (isObj(v) && depth < 4) {
    for (const [k, x] of Object.entries(v)) listsIn(x, path ? `${path}.${k}` : k, depth + 1, out);
  }
  return out;
}

/** The tables of a pipeline (each stage), refreshed when the source changes. */
const stageCache = new Map<string, { stages: Stage[]; text: string } | null>();
function useStages(source: string, list: string, steps: DataStep[], data: Data) {
  const version = useData((s) => s.versions[source] ?? 0);
  // Only the values its conditions read count.
  const used =
    JSON.stringify(steps)
      .match(/\{(\w+)\}/g)
      ?.map((m) => m.slice(1, -1)) ?? [];
  const sub = Object.fromEntries(used.map((k) => [k, data[k] ?? '']));
  const key = JSON.stringify([source, list, steps, sub, version]);
  const [res, setRes] = useState(() => stageCache.get(key));
  useEffect(() => {
    if (stageCache.has(key)) return setRes(stageCache.get(key));
    let live = true;
    const h = setTimeout(
      () =>
        pipeline(source, list, steps, sub).then((r) => {
          stageCache.set(key, r);
          if (live) setRes(r);
        }),
      120,
    );
    return () => {
      live = false;
      clearTimeout(h);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return res;
}

/** What a cell holds: a picture (uploaded or on the web), a colour, a number… */
export type CellType = 'number' | 'image' | 'color' | 'boolean' | 'text';
const IMAGE = /^(data:image\/|(https?:\/\/|\/)\S+\.(png|jpe?g|gif|webp|svg|avif|bmp)([?#]\S*)?$)/i;
export function cellType(v: string): CellType | null {
  const t = v.trim();
  if (!t) return null;
  if (IMAGE.test(t)) return 'image';
  if (/^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(t) || /^rgba?\(/i.test(t)) return 'color';
  if (/^-?\d+([.,]\d+)?$/.test(t)) return 'number';
  if (/^(true|false)$/i.test(t)) return 'boolean';
  return 'text';
}
/** The type of each column: the one most of its cells have. */
function columnTypes(stage: Stage): CellType[] {
  return stage.columns.map((_, i) => {
    const n: Partial<Record<CellType, number>> = {};
    for (const r of stage.rows.slice(0, 50)) {
      const t = cellType(r[i] ?? '');
      if (t) n[t] = (n[t] ?? 0) + 1;
    }
    return ((Object.entries(n) as [CellType, number][]).sort((a, b) => b[1] - a[1])[0]?.[0] ?? 'text') as CellType;
  });
}
const TYPE_ICON: Record<CellType, typeof Type> = { text: Type, number: Hash, image: ImageIcon, color: Palette, boolean: ToggleLeft };

/** A cell as it reads: the picture, the colour, or the text. */
export function Cell({ v }: { v: string }) {
  const t = cellType(v);
  if (t === 'image') return <img className="dc-thumb" src={v} alt="" loading="lazy" />;
  if (t === 'color')
    return (
      <>
        <span className="dc-swatch" style={{ background: v }} /> {v}
      </>
    );
  return <>{v}</>;
}

/** The variable kind fitting the result of node `id`, from its last preview. */
function kindOf(flow: DataFlow, id: string): FieldKind | null {
  const c = chainOf(flow, id);
  if (!c) return null;
  const prefix = JSON.stringify([c.source.source, c.source.list, c.steps]).slice(0, -1) + ',';
  const hit = [...stageCache.entries()].find(([k, v]) => k.startsWith(prefix) && v)?.[1];
  const last = hit?.stages[hit.stages.length - 1];
  if (!last || last.columns.length !== 1) return null;
  const t = columnTypes(last)[0];
  return t === 'image' ? 'image' : t === 'number' ? 'number' : t === 'color' ? 'color' : t === 'boolean' ? 'boolean' : last.total > 1 ? 'multiline' : 'text';
}

/** A table, 10 rows in view (more when the node is taller), scrolling. */
function StageTable({ stage, title }: { stage: Stage | undefined; title?: string }) {
  if (!stage) return <span className="dc-meta">{t('Loading the data…')}</span>;
  const types = columnTypes(stage);
  return (
    <div className="dc-table-wrap">
      <div className="dc-table-title">
        {title && <strong>{title}</strong>}
        {tn(stage.total, '{n} row', '{n} rows')} · {tn(stage.columns.length, '{n} column', '{n} columns')}
      </div>
      <div className="dc-table-scroll" data-scroll>
        <table className="dc-table">
          <thead>
            <tr>
              <th>#</th>
              {stage.columns.map((c, i) => {
                const Icon = TYPE_ICON[types[i]];
                return (
                  <th key={c} title={`${c} · ${t(types[i])}`}>
                    <Icon size={10} className={`dc-type t-${types[i]}`} /> {c}
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {stage.rows.map((r, i) => (
              <tr key={i}>
                <td>{i + 1}</td>
                {r.map((c, j) => (
                  <td key={j} title={c} className={types[j] === 'image' ? 'img' : ''}>
                    <Cell v={c} />
                  </td>
                ))}
              </tr>
            ))}
            {!stage.rows.length && (
              <tr>
                <td colSpan={stage.columns.length + 1} className="dc-none">
                  {t('No row')}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/** A source: which list of its value is the table, and the table. */
function SourceBody({ node, version, data, onList }: { node: FlowNode & { type: 'source' }; version: number; data: Data; onList: (list: string) => void }) {
  const value = useSourceValue(node.source, version);
  const res = useStages(node.source, node.list, [], data);
  const lists = value ? ['', ...listsIn(value).filter(Boolean)] : [''];
  return (
    <>
      <label className="dc-inline">
        {t('Rows')}
        <Dropdown value={node.list} options={[...(lists.includes(node.list) ? [] : [node.list]), ...lists].map((l) => ({ value: l, label: l === '' ? (Array.isArray(value) ? t('The list') : t('The whole value')) : l }))} onChange={onList} />
      </label>
      {res === null ? <span className="dc-meta">{t('No data at this place.')}</span> : <StageTable stage={res?.stages[0]} />}
    </>
  );
}

/** A filter, a sort or a transform: its settings on the columns it gets, then its result. */
function StepBody({ source, steps, data, node, onChange }: { source: FlowNode & { type: 'source' }; steps: DataStep[]; data: Data; node: FlowNode & DataStep; onChange: (fn: (n: DataStep) => void) => void }) {
  const res = useStages(source.source, source.list, steps, data);
  const input = res?.stages[res.stages.length - 2];
  const out = res?.stages[res.stages.length - 1];
  const columns = input?.columns ?? [];
  const colSelect = (value: string, set: (c: string) => void) => <Dropdown value={value} placeholder={t('Column')} options={[...(columns.includes(value) || !value ? [] : [value]), ...columns].map((c) => ({ value: c, label: c }))} onChange={set} />;
  let settings: React.ReactNode = null;
  if (node.type === 'filter') {
    settings = (
      <>
        <div className="dc-sec">
          <span className="dc-sec-title">
            {t('Rows where')}
            {node.where.length > 1 && (
              <span className="seg tiny">
                <button className={node.any ? '' : 'on'} onClick={() => onChange((n) => void ((n as typeof node).any = false))}>
                  {t('all')}
                </button>
                <button className={node.any ? 'on' : ''} onClick={() => onChange((n) => void ((n as typeof node).any = true))}>
                  {t('any')}
                </button>
              </span>
            )}
          </span>
          {node.where.map((c, i) => {
            const op = CONDITIONS.find((x) => x.op === c.op)!;
            const set = (patch: Partial<typeof c>) => onChange((n) => void Object.assign((n as typeof node).where[i], patch));
            return (
              <div key={i} className="dc-cond">
                {colSelect(c.column, (column) => set({ column }))}
                <Dropdown value={c.op} options={CONDITIONS.map((x) => ({ value: x.op, label: t(x.label) }))} onChange={(op) => set({ op })} />
                {op.value && <input value={c.value} placeholder={t('value or {key}')} onChange={(e) => set({ value: e.target.value })} />}
                <button className="icon-btn ghost" title={t('Remove')} onClick={() => onChange((n) => void (n as typeof node).where.splice(i, 1))}>
                  <X size={11} />
                </button>
              </div>
            );
          })}
          <button className="dc-addfilter" onClick={() => onChange((n) => void (n as typeof node).where.push({ column: columns[0] ?? '', op: 'eq', value: '' }))}>
            <Plus size={11} /> {t('Condition')}
          </button>
        </div>
        <div className="dc-sec">
          <span className="dc-sec-title">{t('Rows')}</span>
          <div className="row">
            <label className="dc-inline" title={t('1 = the first, −1 = the last; 0 = from the first')}>
              {t('From')} <input type="number" value={node.from} onChange={(e) => onChange((n) => void ((n as typeof node).from = +e.target.value))} />
            </label>
            <label className="dc-inline" title={t('0 = all')}>
              {t('Count')} <input type="number" min={0} value={node.count} onChange={(e) => onChange((n) => void ((n as typeof node).count = Math.max(0, +e.target.value)))} />
            </label>
          </div>
        </div>
        <div className="dc-sec">
          <span className="dc-sec-title">{t('Columns')} {node.columns.length ? '' : <em>{t('all')}</em>}</span>
          <ColumnChips columns={columns} selected={node.columns} onChange={(cols) => onChange((n) => void ((n as typeof node).columns = cols))} />
        </div>
      </>
    );
  } else if (node.type === 'sort') {
    settings = (
      <div className="dc-sec">
        <span className="dc-sec-title">{t('By')}</span>
        {node.keys.map((k, i) => {
          const set = (patch: Partial<typeof k>) => onChange((n) => void Object.assign((n as typeof node).keys[i], patch));
          return (
            <div key={i} className="dc-cond">
              <span className="dc-step-n">{i + 1}</span>
              {colSelect(k.column, (column) => set({ column }))}
              <button className={`btn small ${k.descending ? '' : 'on'}`} title={t('Smallest first')} onClick={() => set({ descending: false })}>
                <ArrowUp size={12} />
              </button>
              <button className={`btn small ${k.descending ? 'on' : ''}`} title={t('Largest first')} onClick={() => set({ descending: true })}>
                <ArrowDown size={12} />
              </button>
              <button className={`btn small ${k.numeric ? 'on' : ''}`} title={t('Compare as numbers (else as text)')} onClick={() => set({ numeric: !k.numeric })}>
                123
              </button>
              <button className="icon-btn ghost" title={t('Remove')} onClick={() => onChange((n) => void (n as typeof node).keys.splice(i, 1))}>
                <X size={11} />
              </button>
            </div>
          );
        })}
        <button className="dc-addfilter" onClick={() => onChange((n) => void (n as typeof node).keys.push({ column: columns[0] ?? '', descending: true, numeric: true }))}>
          <Plus size={11} /> {node.keys.length ? t('Then by') : t('Column')}
        </button>
      </div>
    );
  } else {
    settings = (
      <>
        <div className="dc-sec">
          <span className="dc-sec-title">{t('On the columns')} {node.columns.length ? '' : <em>{t('all')}</em>}</span>
          <ColumnChips columns={columns} selected={node.columns} onChange={(cols) => onChange((n) => void ((n as typeof node).columns = cols))} />
        </div>
        <div className="dc-sec">
          <span className="dc-sec-title">{t('Operations, in order')}</span>
          <TransformSteps steps={node.ops} onChange={(ops) => onChange((n) => void ((n as typeof node).ops = ops))} />
        </div>
      </>
    );
  }
  return (
    <>
      {settings}
      {res === null ? <span className="dc-meta">{t('No data at this place.')}</span> : <StageTable stage={out} title={t('Result')} />}
    </>
  );
}

/** Columns to keep (or work on), in the order picked; none = all. */
function ColumnChips({ columns, selected, onChange }: { columns: string[]; selected: string[]; onChange: (c: string[]) => void }) {
  const all = [...columns, ...selected.filter((c) => !columns.includes(c))];
  return (
    <div className="dc-chips">
      {all.map((c) => {
        const i = selected.indexOf(c);
        return (
          <button key={c} className={`dc-chip ${i >= 0 ? 'on' : ''} ${columns.includes(c) ? '' : 'missing'}`} onClick={() => onChange(i >= 0 ? selected.filter((x) => x !== c) : [...selected, c])}>
            {i >= 0 && <span className="dc-chip-n">{i + 1}</span>}
            {c}
          </button>
        );
      })}
      {selected.length > 0 && (
        <button className="dc-chip clear" onClick={() => onChange([])} title={t('Every column')}>
          {t('All')}
        </button>
      )}
    </div>
  );
}

/**
 * A data set: its columns (a port each, with the value of the row shown),
 * the row previewed in the Designer, and how the graphic on air goes from
 * one row to the next.
 */
function DataSetBody({ node, chain, data, onColumns, onRow, startWire }: { node: FlowNode & { type: 'dataset' }; chain: ReturnType<typeof chainOf>; data: Data; onColumns: (cols: string[]) => void; onRow: (row: number) => void; startWire: (e: React.PointerEvent, key: string) => void }) {
  const res = useStages(chain?.source.source ?? '', chain?.source.list ?? '', chain?.steps ?? [], data);
  const tbl = chain ? res?.stages[res.stages.length - 1] : undefined;
  const cols = tbl?.columns ?? [];
  useEffect(() => onColumns(cols), [cols.join('\n')]); // eslint-disable-line react-hooks/exhaustive-deps
  const types = tbl ? columnTypes(tbl) : [];
  const total = tbl?.total ?? 0;
  const row = Math.min(Math.max(1, parseInt(data[rowKey(node.key)] ?? '1', 10) || 1), Math.max(1, total));
  return (
    <>
      <div className="ds-col ds-all" style={{ height: DS_ROW }}>
        <Rows3 size={11} className="dc-type" />
        <span className="ds-name">{t('All rows')}</span>
        <span className="ds-val">{tbl ? t('{rows} × {columns} table', { rows: total, columns: cols.length }) : ''}</span>
        <span className="port out" style={DS_DOT} data-wire={`o:k:${tableKey(node.key)}`} title={t('Drag to a Table, Credits, Ticker or Chart: the whole table (key {key})', { key: tableKey(node.key) })} onPointerDown={(e) => startWire(e, tableKey(node.key))} />
      </div>
      {cols.map((c, i) => {
        const Icon = TYPE_ICON[types[i]];
        const v = tbl?.rows[row - 1]?.[i] ?? '';
        return (
          <div key={c} className="ds-col" style={{ height: DS_ROW }}>
            <Icon size={11} className={`dc-type t-${types[i]}`} />
            <span className="ds-name">{c}</span>
            <span className="ds-val" title={v}>
              <Cell v={v} />
            </span>
            <span className="port out" style={DS_DOT} data-wire={`o:k:${columnKey(node.key, c)}`} title={t("Drag {column} to a layer's value (key {key})", { column: c, key: columnKey(node.key, c) })} onPointerDown={(e) => startWire(e, columnKey(node.key, c))} />
          </div>
        );
      })}
      <div className="ln-body">
        {!chain && <span className="dc-meta">{t('Wire a source (or a step) into it.')}</span>}
        {chain && res === null && <span className="dc-meta">{t('No data at this place.')}</span>}
        {chain && tbl && (
          <div className="ds-cursor">
            <span className="dc-meta">{t('Row')}</span>
            <b>
              {total ? row : 0} / {total}
            </b>
            <span className="dc-meta ds-first">{tbl.rows[row - 1]?.find((x) => x && cellType(x) === 'text') ?? ''}</span>
            <button className="icon-btn ghost" title={t('Previous row (preview)')} disabled={row <= 1} onClick={() => onRow(row - 1)}>
              <ArrowLeft size={12} />
            </button>
            <button className="icon-btn ghost" title={t('Next row (preview)')} disabled={row >= total} onClick={() => onRow(row + 1)}>
              <ArrowRight size={12} />
            </button>
          </div>
        )}
      </div>
    </>
  );
}

/** The operations of a transform, applied one after the other. */
function TransformSteps({ steps, onChange }: { steps: DataFilter[]; onChange: (s: DataFilter[]) => void }) {
  const set = (i: number, f: DataFilter) => onChange(steps.map((x, k) => (k === i ? f : x)));
  const move = (i: number, d: -1 | 1) => {
    const next = [...steps];
    [next[i], next[i + d]] = [next[i + d], next[i]];
    onChange(next);
  };
  return (
    <div className="dc-steps">
      {steps.map((f, i) => (
        <div key={i} className="dc-step">
          <div className="row">
            <span className="dc-step-n">{i + 1}</span>
            <Dropdown
              value={f.type as string}
              options={[...TRANSFORMS.map((x) => ({ value: x.make().type as string, label: t(x.label) })), ...(TRANSFORMS.some((x) => x.make().type === f.type) ? [] : [{ value: f.type as string, label: f.type as string }])]}
              onChange={(ty) => set(i, TRANSFORMS.find((x) => x.make().type === ty)!.make())}
            />
            <button className="icon-btn ghost" title={t('Earlier')} disabled={i === 0} onClick={() => move(i, -1)}>
              <ArrowUp size={11} />
            </button>
            <button className="icon-btn ghost" title={t('Later')} disabled={i === steps.length - 1} onClick={() => move(i, 1)}>
              <ArrowDown size={11} />
            </button>
            <button className="icon-btn ghost" title={t('Remove')} onClick={() => onChange(steps.filter((_, k) => k !== i))}>
              <X size={11} />
            </button>
          </div>
          <FilterBody filter={f} onChange={(nf) => set(i, nf)} />
        </div>
      ))}
      <button className="dc-addfilter" onClick={() => onChange([...steps, { type: 'upper' }])}>
        <Plus size={11} /> {t('Operation')}
      </button>
    </div>
  );
}

/** The settings of a filter. */
function FilterBody({ filter: f, onChange }: { filter: DataFilter; onChange: (f: DataFilter) => void }) {
  const text = (key: string, placeholder: string) => <input placeholder={placeholder} value={String((f as Record<string, unknown>)[key] ?? '')} onChange={(e) => onChange({ ...f, [key]: e.target.value } as DataFilter)} />;
  const number = (key: string, title: string, min?: number) => <input type="number" title={title} min={min} value={Number((f as Record<string, unknown>)[key] ?? 0)} onChange={(e) => onChange({ ...f, [key]: +e.target.value } as DataFilter)} />;
  let body: React.ReactNode = null;
  switch (f.type) {
    case 'prefix':
    case 'suffix':
      body = text('text', f.type === 'prefix' ? t('Text before') : t('Text after'));
      break;
    case 'default':
      body = text('text', t('Text when empty'));
      break;
    case 'replace':
      body = (
        <>
          {text('find', t('Find'))}
          {text('with', t('Replace with'))}
        </>
      );
      break;
    case 'map':
      body = (
        <>
          {text('when', t('When the value is'))}
          {text('then', t('Show'))}
        </>
      );
      break;
    case 'truncate':
      body = <label className="dc-inline">{t('Max')} {number('max', t('Characters'), 1)}</label>;
      break;
    case 'lines':
      body = (
        <div className="row">
          <label className="dc-inline">{t('From')} {number('from', t('First line (1 = first, −1 = last)'))}</label>
          <label className="dc-inline">{t('Count')} {number('count', t('0 = all'), 0)}</label>
        </div>
      );
      break;
    case 'join':
      body = text('separator', t('Separator'));
      break;
    case 'number':
      body = (
        <div className="row">
          <label className="dc-inline">{t('Decimals')} {number('decimals', t('Decimals'), 0)}</label>
          <label className="dc-inline">
            <input type="checkbox" checked={f.grouping} onChange={(e) => onChange({ ...f, grouping: e.target.checked })} /> 1 000
          </label>
        </div>
      );
      break;
    case 'math':
      body = (
        <div className="row">
          <Dropdown
            value={f.op}
            options={[
              { value: 'add', label: t('+ add') },
              { value: 'sub', label: t('− subtract') },
              { value: 'mul', label: t('× multiply') },
              { value: 'div', label: t('÷ divide') },
            ]}
            onChange={(op) => onChange({ ...f, op })}
          />
          {number('value', t('Value'))}
        </div>
      );
      break;
  }
  return body ? <div className="dc-step-body">{body}</div> : null;
}

/** The default value of a variable, with an editor fitting its kind. */
function DefaultEditor({ field: f, onChange }: { field: Field; onChange: (fn: (f: Field) => void, key?: string) => void }) {
  const set = (v: string) => onChange((x) => void (x.default = v), `default-${f.key}`);
  switch (f.kind) {
    case 'multiline':
      return <textarea rows={3} placeholder={t('Default value')} value={f.default} onChange={(e) => set(e.target.value)} />;
    case 'number': {
      const min = f.min ?? 0;
      const max = f.max ?? 100;
      return (
        <>
          <div className="row">
            <input type="range" min={min} max={max} step={(max - min) / 200 || 1} value={parseFloat(f.default) || 0} onChange={(e) => set(e.target.value)} />
            <input type="number" className="dc-num" value={f.default} onChange={(e) => set(e.target.value)} />
          </div>
          <div className="row dc-range">
            <label className="dc-inline">
              {t('Min')} <input type="number" value={min} onChange={(e) => onChange((x) => void (x.min = +e.target.value), `min-${f.key}`)} />
            </label>
            <label className="dc-inline">
              {t('Max')} <input type="number" value={max} onChange={(e) => onChange((x) => void (x.max = +e.target.value), `max-${f.key}`)} />
            </label>
          </div>
        </>
      );
    }
    case 'color': {
      const c = parseHex(f.default) ?? [1, 1, 1, 1];
      return (
        <div className="row">
          <HexColorField value={toHex(c, c[3] < 1)} onChange={set} />
        </div>
      );
    }
    case 'image':
      return <AssetPicker value={f.default} onChange={set} />;
    case 'boolean':
      return (
        <label className="switch">
          <input type="checkbox" checked={f.default === 'true'} onChange={(e) => set(e.target.checked ? 'true' : 'false')} />
          <span>{f.default === 'true' ? t('On') : t('Off')}</span>
        </label>
      );
    default:
      return <input placeholder={t('Default value')} title={t('Value until the operator types another one')} value={f.default} onChange={(e) => set(e.target.value)} />;
  }
}

/** The (•) of a value exposed to the Operator, as in the Inspector: one click stops. */
function OperatorMark({ field, onStop }: { field: Field; onStop: () => void }) {
  return (
    <button className="pp-op on" style={{ flex: 'none', width: 20, height: 20, marginRight: 4 }} title={t('Exposed to the Operator ({fields}) · click to stop', { fields: field.label })} onClick={onStop}>
      <Radio size={12} />
    </button>
  );
}

export function DataCanvasButton({ className = 'btn small' }: { className?: string }) {
  const open = useDataCanvas((s) => s.set);
  // What the canvas holds: its nodes and the fields fed by a source.
  const count = useDesigner((s) => (s.template?.dataflow?.nodes.length ?? 0) + (s.template?.fields.filter((f) => f.binding?.source).length ?? 0));
  return (
    <button className={`${className} ${count ? 'has' : ''}`} onClick={() => open(true)} title={t("Sources, filters, variables, operations and the layers' values, on a canvas")}>
      <Database size={13} /> {t('Data')}
    </button>
  );
}

/** "+ Setting from data": picks a setting of the layer to give a port (then wire a variable or a column into it). */
function SettingAdd({ layer, onPick }: { layer: Layer; onPick: (path: string) => void }) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState('');
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    window.addEventListener('pointerdown', close);
    return () => window.removeEventListener('pointerdown', close);
  }, [open]);
  const taken = new Set(Object.keys(layer.settingBindings ?? {}));
  const all = open ? settingPorts(layer, t).filter((s) => !taken.has(s.path) && !contentOf(layer).includes(s.path)) : [];
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const list = all.filter((s) => words.every((w) => s.label.toLowerCase().includes(w)));
  const groups = [...new Set(list.map((s) => s.group))];
  return (
    <div className="dc-setadd" ref={ref}>
      <button className="dc-props-toggle" onClick={() => setOpen(!open)} title={t('A colour, a size, a switch… of this widget set from the data')}>
        <Plus size={11} /> {t('Setting from data')}
      </button>
      {open && (
        <div className="dc-setmenu" data-scroll onWheel={(e) => e.stopPropagation()}>
          <input autoFocus placeholder={t('Search a setting (colour, title…)')} value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.key === 'Escape' && setOpen(false)} />
          <div className="dc-setlist">
            {groups.map((g) => (
              <div key={g}>
                <div className="dc-setgroup">{g}</div>
                {list
                  .filter((s) => s.group === g)
                  .map((s) => (
                    <button
                      key={s.path}
                      onClick={() => {
                        onPick(s.path);
                        setOpen(false);
                        setQ('');
                      }}
                    >
                      {isColorValue(s.value) ? <span className="dc-swatch" style={{ background: String(s.value) }} /> : <span className="dc-setv">{s.value === undefined ? '' : String(s.value).slice(0, 10)}</span>}
                      <span>{s.label.slice(g.length + 3) || s.label}</span>
                    </button>
                  ))}
              </div>
            ))}
            {!list.length && <em className="dc-setnone">{t('No setting')}</em>}
          </div>
        </div>
      )}
    </div>
  );
}

import { Ampersand, ArrowDown, ArrowLeftRight, Calculator, Crosshair, Diamond, Hash, Link2, Pause, Play, Rows3, Split, Square, SquareStack, Target, TextCursorInput, Timer, Trash2, Type, Zap } from 'lucide-react';
import { useShallow } from 'zustand/react/shallow';
import { useMemo, useState } from 'react';
import { holdFrame, interpolateText, newId, resolveLayer } from '../engine/evaluate';
import { INPUTS, measureLayer, nodeValues, type LVal } from '../engine/logic';
import { COLOR_PROPS, PROPS, TEXT_LIKE, type Cmp, type Layer, type Logic, type LogicNode, type MathOp, type Prop, type Template, type TextMetric, type TriggerEvent } from '../engine/types';
import { CanvasHeader, CanvasVoid } from '../ui/CanvasHeader';
import { Dropdown } from '../ui/Dropdown';
import { NumberField, Segmented, TextInput } from '../ui/fields';
import { useApp } from '../store';
import { autoLayout, measureNodes } from '../ui/autoLayout';
import { freeSpot, ResizeCorner, useCanvasView } from '../ui/canvasView';
import { PROP_LABELS } from './Inspector';
import { useDesigner } from './designerStore';
import { t } from '../i18n';
import { displayPref, setDisplayPref } from '../displayPrefs';

/*
 * Relations editor (the Logic canvas): sources → operations → targets,
 * wired on a canvas. Values update live at the playhead with the preview
 * data, and the same graph runs in the Rust renderer.
 */

const NODE_W = 210;
const HEAD = 30;
const ROW = 24;

/** Node kinds, by category (the palette's sections and the nodes' colors). An event node per event (`ev`). */
type Cat = 'input' | 'event' | 'op' | 'output' | 'air';
/** The Logic (what is drawn) or the Triggers (what happens on air). */
export type Graph = 'logic' | 'triggers';
export const KINDS: { type: LogicNode['type']; ev?: TriggerEvent['kind']; label: string; cat: Cat; icon: typeof Hash; in: Graph[] }[] = [
  { type: 'event', ev: 'scrollEnd', label: 'End of the scroll', cat: 'event', icon: ArrowLeftRight, in: ['triggers'] },
  { type: 'event', ev: 'timerZero', label: 'Timer', cat: 'event', icon: Timer, in: ['triggers'] },
  { type: 'event', ev: 'videoEnd', label: 'End of the video', cat: 'event', icon: Play, in: ['triggers'] },
  { type: 'event', ev: 'lastRow', label: 'Last row', cat: 'event', icon: Rows3, in: ['triggers'] },
  { type: 'event', ev: 'dataChange', label: 'Value changes', cat: 'event', icon: Diamond, in: ['triggers'] },
  { type: 'event', ev: 'graphic', label: 'Another graphic', cat: 'event', icon: SquareStack, in: ['triggers'] },
  { type: 'layerProp', label: 'Layer property', cat: 'input', icon: Crosshair, in: ['logic'] },
  { type: 'field', label: 'Data field', cat: 'input', icon: TextCursorInput, in: ['logic', 'triggers'] },
  { type: 'text', label: 'Text', cat: 'input', icon: Type, in: ['logic', 'triggers'] },
  { type: 'constant', label: 'Number', cat: 'input', icon: Hash, in: ['logic', 'triggers'] },
  { type: 'math', label: 'Math', cat: 'op', icon: Calculator, in: ['logic'] },
  { type: 'clamp', label: 'Clamp', cat: 'op', icon: Split, in: ['logic'] },
  { type: 'condition', label: 'Condition', cat: 'op', icon: Split, in: ['logic', 'triggers'] },
  { type: 'bool', label: 'And / Or', cat: 'op', icon: Ampersand, in: ['logic', 'triggers'] },
  { type: 'target', label: 'Set property', cat: 'output', icon: Target, in: ['logic'] },
  { type: 'out', label: 'Take out (OUT)', cat: 'air', icon: Square, in: ['triggers'] },
  { type: 'resume', label: 'Resume the pause', cat: 'air', icon: Pause, in: ['triggers'] },
  { type: 'fire', label: 'Fire a trigger', cat: 'air', icon: Zap, in: ['triggers'] },
  { type: 'nextRow', label: 'Next row', cat: 'air', icon: ArrowDown, in: ['triggers'] },
];
/** The palette's sections, by graph. */
const CATS: Record<Graph, { id: Cat; label: string }[]> = {
  logic: [
    { id: 'input', label: 'Inputs' },
    { id: 'op', label: 'Operations' },
    { id: 'output', label: 'Output' },
  ],
  triggers: [
    { id: 'event', label: 'Events' },
    { id: 'input', label: 'Values' },
    { id: 'op', label: 'Conditions' },
    { id: 'air', label: 'On air' },
  ],
};
/** The event an event node's kind stands for (a timer at a time is the Timer node). */
const evKind = (on: TriggerEvent) => (on.kind === 'timerAt' ? 'timerZero' : on.kind === 'dataIs' ? 'dataChange' : on.kind);
/** A node's kind (a text measurement is a Layer property). */
export const kindOf = (n: LogicNode) => KINDS.find((k) => k.type === (n.type === 'textSize' ? 'layerProp' : n.type) && (n.type !== 'event' || k.ev === evKind(n.on)))!;

/** Layers an event can watch. */
const timerLayer = (l: Layer) => l.type === 'clock' && !!((l as { timerMode?: string }).timerMode || ['countdown', 'stopwatch'].includes((l as { mode?: string }).mode ?? ''));
const videoLayer = (l: Layer) => l.type === 'image' && !!((l as { video?: boolean }).video || ((l as { frames?: string[] }).frames?.length ?? 0) > 0);
const scrollLayer = (l: Layer) => l.type === 'ticker' || l.type === 'credits';

const MATH: { value: MathOp; label: string }[] = [
  { value: 'add', label: 'A + B' },
  { value: 'sub', label: 'A − B' },
  { value: 'mul', label: 'A × B' },
  { value: 'div', label: 'A ÷ B' },
  { value: 'min', label: 'min(A, B)' },
  { value: 'max', label: 'max(A, B)' },
  { value: 'abs', label: '|A|' },
  { value: 'round', label: 'round(A)' },
  { value: 'floor', label: 'floor(A)' },
  { value: 'ceil', label: 'ceil(A)' },
];

const CMP: { value: Cmp; label: string }[] = [
  { value: 'gt', label: 'A > B' },
  { value: 'ge', label: 'A ≥ B' },
  { value: 'lt', label: 'A < B' },
  { value: 'le', label: 'A ≤ B' },
  { value: 'eq', label: 'A = B' },
  { value: 'ne', label: 'A ≠ B' },
  { value: 'empty', label: 'A is empty' },
  { value: 'notEmpty', label: 'A is not empty' },
];

/** Properties a layer of this type has (what a Layer property or Set property node offers). */
export function propsOf(l: Layer | undefined, numeric: boolean): Prop[] {
  if (!l) return [];
  const has = (p: Prop) => {
    if (p === 'fill') return l.type !== 'image';
    if (p === 'stroke' || p === 'strokeWidth') return l.type !== 'image' && l.type !== 'qrCode';
    if (p === 'cornerRadius') return l.type === 'rect' || l.type === 'group' || l.type === 'progress';
    if (p === 'fontSize' || p === 'letterSpacing') return TEXT_LIKE.has(l.type);
    if (p.startsWith('crop')) return l.type === 'image';
    if (p === 'arcStart' || p === 'arcEnd' || p === 'innerRadius') return l.type === 'ellipse';
    if (p === 'drawStart' || p === 'drawEnd') return l.type === 'pen';
    if (p === 'cornerTl' || p === 'cornerTr' || p === 'cornerBr' || p === 'cornerBl') return l.type === 'rect';
    if (p === 'fill2') return ['rect', 'ellipse', 'polygon', 'pen', 'text'].includes(l.type);
    // The layer's drop shadow (a Text has its own).
    if (p === 'shadowColor' || p === 'shadowX' || p === 'shadowY' || p === 'shadowBlur') return l.type !== 'text';
    return true;
  };
  return PROPS.filter((p) => has(p) && (!numeric || !COLOR_PROPS.has(p)));
}
/** Property names in full, for the lists (the Inspector's labels are short: W, H). */
const PROP_NAMES: Partial<Record<Prop, string>> = { x: 'X position', y: 'Y position', width: 'Width', height: 'Height', anchorX: 'Anchor X', anchorY: 'Anchor Y', scaleX: 'Scale X', scaleY: 'Scale Y', rotation: 'Rotation', opacity: 'Opacity', strokeWidth: 'Stroke width', cornerRadius: 'Corner radius', fontSize: 'Text size', letterSpacing: 'Letter spacing' };
const propName = (p: Prop) => t(PROP_NAMES[p] ?? PROP_LABELS[p] ?? p);
const METRICS: { value: TextMetric; label: string }[] = [
  { value: 'width', label: 'Text width' },
  { value: 'height', label: 'Text height' },
  { value: 'lines', label: 'Lines' },
  { value: 'chars', label: 'Characters' },
];

export function makeNode(type: LogicNode['type'], t: Template, x: number, y: number, ev?: TriggerEvent['kind']): LogicNode {
  const first = t.layers[0]?.id ?? '';
  const text = t.layers.find((l) => l.type === 'text')?.id ?? first;
  const base = { id: newId(), x, y };
  switch (type) {
    case 'layerProp':
      return { ...base, type, layer: first, prop: 'width' };
    case 'textSize':
      return { ...base, type, layer: text, metric: 'width' };
    case 'field':
      return { ...base, type, key: t.fields[0]?.key ?? '' };
    case 'constant':
      return { ...base, type, value: 0 };
    case 'math':
      return { ...base, type, op: 'add' };
    case 'clamp':
      return { ...base, type, min: 0, max: 1920 };
    case 'condition':
      return { ...base, type, cmp: 'gt' };
    case 'target':
      return { ...base, type, layer: first, prop: 'width', mode: 'set' };
    case 'text':
      return { ...base, type, value: '' };
    case 'bool':
      return { ...base, type, op: 'or' };
    case 'out':
      return { ...base, type };
    case 'resume':
      return { ...base, type, marker: t.markers.find((m) => m.kind === 'pause')?.id ?? '' };
    case 'fire':
      return { ...base, type, name: '' };
    case 'nextRow':
      return { ...base, type, dataset: t.datasets?.[0]?.key ?? '' };
    case 'event': {
      const of = (f: (l: Layer) => boolean) => t.layers.find(f)?.id ?? '';
      const on: TriggerEvent =
        ev === 'timerZero'
          ? { kind: 'timerZero', layer: of(timerLayer) }
          : ev === 'videoEnd'
            ? { kind: 'videoEnd', layer: of(videoLayer) }
            : ev === 'lastRow'
              ? { kind: 'lastRow', dataset: t.datasets?.[0]?.key ?? '' }
              : ev === 'dataChange'
                ? { kind: 'dataChange', key: t.fields[0]?.key ?? '' }
                : ev === 'graphic'
                  ? { kind: 'graphic', template: '' }
                  : { kind: 'scrollEnd', layer: of(scrollLayer), passes: 1 };
      return { ...base, type, on };
    }
  }
}

const portIn = (n: LogicNode, port: string) => ({ x: n.x, y: n.y + HEAD + INPUTS[n.type].indexOf(port) * ROW + ROW / 2 });
const portOut = (n: LogicNode, w = NODE_W) => ({ x: n.x + w, y: n.y + HEAD / 2 });

function curve(a: { x: number; y: number }, b: { x: number; y: number }) {
  const dx = Math.max(40, Math.abs(b.x - a.x) / 2);
  return `M ${a.x} ${a.y} C ${a.x + dx} ${a.y}, ${b.x - dx} ${b.y}, ${b.x} ${b.y}`;
}

export const fmt = (v: LVal | undefined) => (v === undefined ? '' : typeof v === 'number' ? String(+v.toFixed(2)) : `“${v.length > 18 ? `${v.slice(0, 18)}…` : v}”`);

/** Settings of a logic node (layer, property, operation…), shared by the Logic and Data editors. */
export function LogicNodeBody({ n, template, setNode }: { n: LogicNode; template: Template; setNode: (id: string, patch: Partial<LogicNode>, key?: string) => void }) {
  const layerOpts = template.layers.map((l) => ({ value: l.id, label: l.name, note: l.type }));
  const layerOf = (id: string) => template.layers.find((l) => l.id === id);
  const propOpts = (l: Layer | undefined, numeric: boolean) => propsOf(l, numeric).map((p) => ({ value: p as string, label: propName(p), group: t('Property') }));
  const replace = (patch: Partial<LogicNode> & { type: LogicNode['type'] }) => setNode(n.id, patch as Partial<LogicNode>);

  switch (n.type) {
    // One node reads a layer: its properties, and for a text its measured size.
    case 'layerProp':
    case 'textSize': {
      const l = layerOf(n.layer);
      const measured = l?.type === 'text';
      const value = n.type === 'textSize' ? `m:${n.metric}` : n.prop;
      const opts = [...propOpts(l, true), ...(measured ? METRICS.map((m) => ({ value: `m:${m.value}`, label: t(m.label), group: t('Measured text') })) : [])];
      return (
        <>
          <Dropdown
            value={n.layer}
            options={layerOpts}
            onChange={(id) => {
              const nl = layerOf(id);
              // A measurement needs a text: back to a property otherwise.
              if (n.type === 'textSize' && nl?.type !== 'text') replace({ type: 'layerProp', layer: id, prop: 'width' } as never);
              else setNode(n.id, { layer: id } as Partial<LogicNode>);
            }}
          />
          <Dropdown
            value={value}
            options={opts}
            onChange={(v) => (v.startsWith('m:') ? replace({ type: 'textSize', metric: v.slice(2) as TextMetric } as never) : replace({ type: 'layerProp', prop: v as Prop } as never))}
          />
        </>
      );
    }
    case 'field':
      return (
        <Dropdown
          value={n.key}
          placeholder={t('Choose a field')}
          options={[...(template.fields.some((f) => f.key === n.key) || !n.key ? [] : [{ value: n.key, label: n.key }]), ...template.fields.map((f) => ({ value: f.key, label: f.label, note: t(f.kind) }))]}
          onChange={(key) => setNode(n.id, { key })}
        />
      );
    case 'constant':
      return <NumberField value={n.value} onChange={(v) => setNode(n.id, { value: v }, `logic-v-${n.id}`)} />;
    case 'math':
      return <Dropdown value={n.op} options={MATH.map((o) => ({ ...o, label: t(o.label) }))} onChange={(op) => setNode(n.id, { op })} />;
    case 'clamp':
      return (
        <div className="ln-pair">
          <label>
            <span>{t('Min')}</span>
            <NumberField value={n.min} onChange={(v) => setNode(n.id, { min: v }, `logic-v-${n.id}`)} />
          </label>
          <label>
            <span>{t('Max')}</span>
            <NumberField value={n.max} onChange={(v) => setNode(n.id, { max: v }, `logic-v-${n.id}`)} />
          </label>
        </div>
      );
    case 'condition':
      return <Dropdown value={n.cmp} options={CMP.map((o) => ({ ...o, label: t(o.label) }))} onChange={(cmp) => setNode(n.id, { cmp })} />;
    case 'target': {
      const l = layerOf(n.layer);
      return (
        <>
          <Dropdown value={n.layer} options={layerOpts} onChange={(id) => setNode(n.id, { layer: id } as Partial<LogicNode>)} />
          <Dropdown value={n.prop} options={propOpts(l, false)} onChange={(p) => setNode(n.id, { prop: p as Prop } as Partial<LogicNode>)} />
          <Dropdown
            value={n.mode}
            options={[
              { value: 'set', label: t('Set to') },
              { value: 'add', label: t('Add to the animation') },
              { value: 'multiply', label: t('Multiply the animation') },
            ]}
            onChange={(mode) => setNode(n.id, { mode: mode as 'set' })}
          />
        </>
      );
    }
    case 'text':
      return <TextInput value={n.value} onChange={(value) => setNode(n.id, { value }, `logic-v-${n.id}`)} />;
    case 'bool':
      return (
        <Segmented
          value={n.op}
          onChange={(op) => setNode(n.id, { op })}
          options={[
            { value: 'and', label: t('And') },
            { value: 'or', label: t('Or') },
          ]}
        />
      );
    case 'out':
      return null;
    case 'resume':
      return <Dropdown value={n.marker} placeholder={t('Choose a pause')} options={template.markers.filter((m) => m.kind === 'pause').map((m) => ({ value: m.id, label: m.name || t('Pause') }))} onChange={(marker) => setNode(n.id, { marker })} />;
    case 'event':
      return <EventBody n={n} template={template} set={(on, key) => setNode(n.id, { on } as Partial<LogicNode>, key)} />;
    case 'fire':
      return <TextInput value={n.name} placeholder={t('goal, next, line2…')} onChange={(name) => setNode(n.id, { name }, `logic-v-${n.id}`)} />;
    case 'nextRow':
      return <Dropdown value={n.dataset} placeholder={t('Choose a data set')} options={(template.datasets ?? []).map((d) => ({ value: d.key, label: d.name }))} onChange={(dataset) => setNode(n.id, { dataset })} />;
  }
}

/** What an event node watches: its layer, data set, value or graphic, and its settings. */
function EventBody({ n, template, set }: { n: Extract<LogicNode, { type: 'event' }>; template: Template; set: (on: TriggerEvent, key?: string) => void }) {
  const project = useApp((s) => s.project);
  const on = n.on;
  const layers = (f: (l: Layer) => boolean) => template.layers.filter(f).map((l) => ({ value: l.id, label: l.name }));
  const fields = template.fields.map((f) => ({ value: f.key, label: f.label || f.key }));
  switch (on.kind) {
    case 'scrollEnd': {
      const ticker = template.layers.find((l) => l.id === on.layer)?.type === 'ticker';
      return (
        <>
          <Dropdown value={on.layer} placeholder={t('Choose a layer')} options={layers(scrollLayer)} onChange={(layer) => set({ ...on, layer, passes: template.layers.find((l) => l.id === layer)?.type === 'ticker' ? (on.passes ?? 1) : undefined })} />
          {ticker && (
            <label className="ln-row">
              <span>{t('Passes')}</span>
              <NumberField value={on.passes ?? 1} min={1} step={1} onChange={(v) => set({ ...on, passes: Math.max(1, Math.round(v)) }, `logic-v-${n.id}`)} />
            </label>
          )}
        </>
      );
    }
    case 'timerZero':
    case 'timerAt':
      return (
        <>
          <Dropdown value={on.layer} placeholder={t('Choose a layer')} options={layers(timerLayer)} onChange={(layer) => set({ ...on, layer })} />
          <Segmented
            value={on.kind}
            onChange={(k) => set(k === 'timerAt' ? { kind: 'timerAt', layer: on.layer, seconds: 10 } : { kind: 'timerZero', layer: on.layer })}
            options={[
              { value: 'timerZero', label: t('At zero') },
              { value: 'timerAt', label: t('At a time') },
            ]}
          />
          {on.kind === 'timerAt' && <NumberField value={on.seconds} min={0} step={1} suffix="s" onChange={(v) => set({ ...on, seconds: Math.max(0, v) }, `logic-v-${n.id}`)} />}
        </>
      );
    case 'videoEnd':
      return <Dropdown value={on.layer} placeholder={t('Choose a layer')} options={layers(videoLayer)} onChange={(layer) => set({ ...on, layer })} />;
    case 'lastRow':
      return <Dropdown value={on.dataset} placeholder={t('Choose a data set')} options={(template.datasets ?? []).map((d) => ({ value: d.key, label: d.name }))} onChange={(dataset) => set({ ...on, dataset })} />;
    case 'dataChange':
    case 'dataIs':
      return <Dropdown value={on.key} placeholder={t('Choose a field')} options={fields} onChange={(key) => set({ kind: 'dataChange', key })} />;
    case 'graphic':
      return (
        <>
          <Dropdown value={on.template} placeholder={t('Choose a graphic')} options={(project?.templates ?? []).filter((x) => x.id !== template.id).map((x) => ({ value: x.id, label: x.name }))} onChange={(tpl) => set({ ...on, template: tpl })} />
          <Segmented
            value={on.out ? 'out' : 'in'}
            onChange={(v) => set({ ...on, out: v === 'out' || undefined })}
            options={[
              { value: 'in', label: t('Comes in') },
              { value: 'out', label: t('Goes out') },
            ]}
          />
        </>
      );
  }
}

export const inputLabel = (n: LogicNode, port: string) => t((n.type === 'condition' ? { a: 'A', b: 'B', then: 'then', else: 'else' }[port] : n.type === 'math' || n.type === 'bool' ? port.toUpperCase() : port === 'when' ? 'When' : port === 'in' ? (n.type === 'target' ? 'value' : 'in') : port) ?? port);

export function LogicEditor({ onClose, graph = 'logic' }: { onClose: () => void; /** The Logic (what is drawn) or the Triggers (what happens on air). */ graph?: Graph }) {
  const { template, update, frame, previewData } = useDesigner(useShallow((s) => ({ template: s.template, update: s.update, frame: s.frame, previewData: s.previewData })));
  const cv = useCanvasView(`${graph}.${template?.id ?? ''}`, '.logic-node', { onArrange: () => arrange() });
  // Node widths, kept for the user.
  const sizesKey = `${graph}.sizes.${template?.id ?? ''}`;
  const [sizes, setSizesState] = useState<Record<string, number>>(() => displayPref(sizesKey, {}));
  const setSize = (id: string, w: number) =>
    setSizesState((cur) => {
      const next = { ...cur, [id]: w };
      setDisplayPref(sizesKey, next);
      return next;
    });
  const widthOf = (n: LogicNode) => sizes[n.id] ?? NODE_W;
  const [selected, setSelected] = useState<string | null>(null);
  const [wire, setWire] = useState<{ from: string; x: number; y: number } | null>(null);
  const logic: Logic = template?.[graph] ?? { nodes: [], edges: [] };

  // Live values at the playhead.
  const values = useMemo(() => {
    if (!template) return new Map<string, LVal>();
    const hold = holdFrame(template);
    const resolved = new Map(template.layers.map((l) => [l.id, resolveLayer(l, frame, previewData, hold)]));
    return nodeValues(logic, resolved, previewData, measureLayer(template.layers, previewData, interpolateText));
  }, [template, frame, previewData]);

  if (!template) return null;
  const nodes = new Map(logic.nodes.map((n) => [n.id, n]));
  const edit = (fn: (l: Logic) => void, key?: string) =>
    update((t) => {
      const g = (t[graph] ??= { nodes: [], edges: [] });
      fn(g);
      if (!g.nodes.length) delete t[graph];
    }, key);
  const setNode = (id: string, patch: Partial<LogicNode>, key?: string) =>
    edit((l) => {
      const n = l.nodes.find((x) => x.id === id);
      if (n) Object.assign(n, patch);
    }, key);

  const canvasPoint = cv.toWorld;

  /** Lays the nodes out from left to right (one step to undo): inputs left, the properties they set right. */
  const arrange = () => {
    const size = measureNodes(cv.ref.current);
    const at = autoLayout(
      logic.nodes.map((n) => ({ id: n.id, ...(size[n.id] ?? { w: widthOf(n), h: 120 }), side: ['target', 'out', 'resume', 'fire', 'nextRow'].includes(n.type) ? ('end' as const) : undefined })),
      logic.edges,
      { tightSources: true },
    );
    useDesigner.setState({ lastCoalesce: null });
    edit((l) => {
      for (const n of l.nodes) if (at[n.id]) Object.assign(n, at[n.id]);
    });
  };

  const add = (type: LogicNode['type'], ev?: TriggerEvent['kind']) => {
    const v = cv.visible();
    const x = Math.round(v.x + 60 + ((logic.nodes.length * 30) % 300));
    const y = Math.round(v.y + 60 + ((logic.nodes.length * 40) % 360));
    const at = freeSpot(document.querySelector('.logic-editor:not(.data-canvas) .logic-canvas'), { x, y });
    const n = makeNode(type, template, at.x, at.y, ev);
    edit((l) => void l.nodes.push(n));
    setSelected(n.id);
  };

  const dragNode = (e: React.PointerEvent, n: LogicNode) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('select, input, button, label, .port, .cv-resize')) return;
    e.preventDefault();
    setSelected(n.id);
    const start = canvasPoint(e.clientX, e.clientY);
    const base = { x: n.x, y: n.y };
    useDesigner.setState({ lastCoalesce: null });
    const move = (ev: PointerEvent) => {
      const p = canvasPoint(ev.clientX, ev.clientY);
      setNode(n.id, { x: Math.round(base.x + p.x - start.x), y: Math.round(base.y + p.y - start.y) }, `logic-move-${n.id}`);
    };
    document.body.classList.add('cv-moving');
    const up = () => {
      document.body.classList.remove('cv-moving');
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  const startWire = (e: React.PointerEvent, from: string) => {
    e.stopPropagation();
    e.preventDefault();
    const p = canvasPoint(e.clientX, e.clientY);
    setWire({ from, ...p });
    const move = (ev: PointerEvent) => setWire({ from, ...canvasPoint(ev.clientX, ev.clientY) });
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      setWire(null);
      const target = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)?.closest<HTMLElement>('.port.in');
      const to = target?.dataset.node;
      const port = target?.dataset.port;
      if (to && port && to !== from) {
        edit((l) => {
          l.edges = l.edges.filter((x) => !(x.to === to && x.port === port));
          l.edges.push({ from, to, port });
        });
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  return (
    <div className="logic-editor" onKeyDown={(e) => e.stopPropagation()}>
      <CanvasHeader icon={graph === 'logic' ? <Link2 size={15} /> : <Zap size={15} />} title={graph === 'logic' ? t('Logic') : t('Triggers')} subject={template.name} onClose={onClose} />
      <div className="logic-body">
        <aside className="logic-palette">
          {CATS[graph].map((c) => (
            <div key={c.id} className="lp-group">
              <div className="dc-pal-title">{t(c.label)}</div>
              {KINDS.filter((k) => k.cat === c.id && k.in.includes(graph)).map((k) => (
                <button key={`${k.type}-${k.ev ?? ''}`} onClick={() => add(k.type, k.ev)} className={`cat-${k.cat}`}>
                  <k.icon size={14} />
                  <strong>{t(k.label)}</strong>
                </button>
              ))}
            </div>
          ))}
        </aside>
        <div
          {...cv.viewport}
          className={`logic-canvas ${cv.viewport.className}`}
          tabIndex={0}
          onPointerDown={(e) => {
            setSelected(null);
            cv.viewport.onPointerDown(e);
          }}
          onKeyDown={(e) => {
            if ((e.key === 'Delete' || e.key === 'Backspace') && selected && !(e.target as HTMLElement).closest('input, select')) {
              edit((l) => {
                l.nodes = l.nodes.filter((n) => n.id !== selected);
                l.edges = l.edges.filter((x) => x.from !== selected && x.to !== selected);
              });
              setSelected(null);
            }
          }}
        >
          <div className="logic-space" data-canvas-void style={cv.spaceStyle}>
            <svg className="logic-wires">
              {logic.edges.map((e, i) => {
                const a = nodes.get(e.from);
                const b = nodes.get(e.to);
                if (!a || !b) return null;
                return (
                  <path
                    key={i}
                    d={curve(portOut(a, widthOf(a)), portIn(b, e.port))}
                    className="wire"
                    onPointerDown={(ev) => {
                      ev.stopPropagation();
                      edit((l) => void (l.edges = l.edges.filter((x) => x !== l.edges[i])));
                    }}
                  >
                    <title>{t('Click to remove')}</title>
                  </path>
                );
              })}
              {wire && nodes.get(wire.from) && <path d={curve(portOut(nodes.get(wire.from)!, widthOf(nodes.get(wire.from)!)), wire)} className="wire live" />}
            </svg>
            {logic.nodes.map((n) => {
              const k = kindOf(n);
              return (
                <div key={n.id} data-node-id={n.id} className={`logic-node kind-${n.type} cat-${k.cat} ${selected === n.id ? 'sel' : ''}`} style={{ left: n.x, top: n.y, width: widthOf(n) }} onPointerDown={(e) => (e.stopPropagation(), dragNode(e, n))}>
                  <div className="ln-head" style={{ height: HEAD }}>
                    <k.icon size={12} />
                    <span>{t(k.label)}</span>
                    <span className="ln-value">{fmt(values.get(n.id))}</span>
                    <button
                      className="ln-del"
                      title={t('Delete (Del)')}
                      onClick={() =>
                        edit((l) => {
                          l.nodes = l.nodes.filter((x) => x.id !== n.id);
                          l.edges = l.edges.filter((x) => x.from !== n.id && x.to !== n.id);
                        })
                      }
                    >
                      <Trash2 size={11} />
                    </button>
                    {!['target', 'out', 'resume', 'fire', 'nextRow'].includes(n.type) && <span className="port out" title={t('Drag to an input')} onPointerDown={(e) => startWire(e, n.id)} />}
                  </div>
                  {INPUTS[n.type].map((port) => (
                    <div key={port} className="ln-input" style={{ height: ROW }}>
                      <span className="port in" data-node={n.id} data-port={port} />
                      <span>{inputLabel(n, port)}</span>
                    </div>
                  ))}
                  <div className="ln-body">
                    <LogicNodeBody n={n} template={template} setNode={setNode} />
                  </div>
                  <ResizeCorner onPointerDown={(e) => cv.resize(e, (w) => setSize(n.id, w))} />
                </div>
              );
            })}
          </div>
          {/* Nothing yet: a small icon and a word, centred (as in the Operator). */}
          {!logic.nodes.length && <CanvasVoid icon={graph === 'logic' ? Link2 : Zap} text={graph === 'logic' ? t('No relation') : t('No trigger')} />}
          {cv.controls}
        </div>
      </div>
    </div>
  );
}

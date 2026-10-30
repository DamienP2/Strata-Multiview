import { RichInput } from '../ui/RichText';
import { useTextTokens } from './textVars';
import { plain as plainText } from '../engine/rich';
// The widgets of the spec, one file each (web/src/widgets/<id>.tsx), with
// their Rust twins in crates/cg-core/src/wid/<id>.rs (settings, operator
// state and actions) and crates/cg-render/src/wid/<id>.rs (drawing). This
// file is what they share: the definition of a widget and the controls of
// the inspector and of the Operator, styled like the rest of the app.
import { Dropdown } from '../ui/Dropdown';
import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { ChevronDown, Database, Ellipsis, Radio, WandSparkles, type LucideIcon } from 'lucide-react';
import { useDesigner } from '../designer/designerStore';
import { useShallow } from 'zustand/react/shallow';
import { fieldUsed, formatSetting, newFieldKey, settingKind } from '../engine/settings';
import { interpolateText, parseHex, toHex, type Resolved } from '../engine/evaluate';
import type { Data, FieldKind, Layer, LayerType, PlayoutItem, Prop, Template, Value } from '../engine/types';
import type { TimeCtx } from '../engine/widgets';
import { onFrames } from '../engine/frameClock';
import { ColorField, NumberField, Segmented, TextInput, HexColorField } from '../ui/fields';
import { alertDialog, confirmDialog, promptDialog } from '../ui/dialogs';
import { t } from '../i18n';

export type WidgetGroup = 'Shapes & tools' | 'Time' | 'Data' | 'Sport & interaction';
export type LayerOf<T extends LayerType> = Extract<Layer, { type: T }>;

export interface Warning {
  level: 'info' | 'warn' | 'error' | 'ok';
  text: string;
  /** Also shown to the Operator (true), or only there ('only'). */
  op?: boolean | 'only';
}

export interface RenderProps<T extends LayerType = LayerType> {
  layer: LayerOf<T>;
  /** Animated properties at this frame (fill, stroke, strokeWidth, cornerRadius, fontSize…: use num()/col()). */
  r: Resolved;
  w: number;
  h: number;
  data: Data;
  time: TimeCtx;
  /** The operator's state of this widget (undefined = default). */
  state: unknown;
  /** Template frame and its On air frame (the In runs from 0 to hold). */
  frame: number;
  hold: number;
  fps: number;
  /** Drawn in the Designer (guides such as a reserved width may show). */
  designer?: boolean;
  /** The project's language ("fr" by default), for the texts a widget writes itself (« Aucune donnée », periods…): `tl(lang, 'No data')`. */
  lang?: string;
}

export interface InspectorProps<T extends LayerType = LayerType> {
  layer: LayerOf<T>;
  template: Template;
  /** Values the Designer previews with (bound fields included). */
  data: Data;
  /** Changes the layer (coalesce: one undo step while typing/dragging). */
  edit: (fn: (l: LayerOf<T>) => void, coalesceKey?: string) => void;
  /** Sets a dot path of the layer (`stroke.width`, `items.2.text`). */
  set: (path: string, v: unknown, coalesce?: boolean) => void;
  /** Try-out state of the Designer and its actions (same logic as on air). */
  state: unknown;
  act: (op: string, args?: unknown) => void;
}

export interface OperatorProps<T extends LayerType = LayerType> {
  layer: LayerOf<T>;
  /** The operator's state of this widget (undefined = default). */
  state: unknown;
  data: Data;
  /** Sends an action (Command `act`) to the graphic on air (or to the Designer's try-out). */
  act: (op: string, args?: unknown) => void;
  /** The graphic on air (undefined in the Designer). */
  item?: PlayoutItem;
  time: TimeCtx;
  /** The project's language ("fr" by default). */
  lang?: string;
  /** Writes values of the take (an exposed setting changed on air); undefined in the Designer. */
  setData?: (patch: Record<string, string>) => void;
}

export interface OverlayProps<T extends LayerType = LayerType> {
  layer: LayerOf<T>;
  /** Layer size and screen pixels per layer pixel. */
  w: number;
  h: number;
  scale: number;
  edit: (fn: (l: LayerOf<T>) => void, coalesceKey?: string) => void;
}

export interface WidgetDef<T extends LayerType = LayerType> {
  /** Module name (quad, timer…) and the layer type it draws. */
  id: string;
  type: T;
  label: string;
  group: WidgetGroup;
  icon: LucideIcon;
  /** Entries of the + menu (default: one, the widget itself). */
  presets?: { label: string; hint?: string; patch?: Record<string, unknown> }[];
  /** Settings and properties of a new layer (x / y are centred by the caller from width / height). */
  create: () => Partial<LayerOf<T>> & { props?: Partial<Record<Prop, Value>> };
  /** Its Fill / Stroke are in its own groups: Properties > Contents leaves them out. */
  ownsLook?: boolean;
  /** Its look changes with time: the widget is redrawn on every frame while its time runs. */
  live?: (l: LayerOf<T>) => boolean;
  /**
   * A live widget whose look changes only with its time on air (or its operator state): with that
   * time standing still and no state (the Designer at rest), it is not redrawn on every frame.
   * Without it, it is (a clock, a cursor blinking, a value easing to a new one…).
   */
  ageOnly?: boolean;
  /** Layer keys of its content (wired by the Data canvas with {{key}}). */
  content?: string[];
  /** Draws the layer inside its box (absolute position, w × h). */
  Render?: (p: RenderProps<T>) => ReactNode;
  /** Its warnings are not shown in the inspector (its editor or the Operator shows them). */
  quiet?: boolean;
  /** Groups of the inspector, above Properties (replaces General). */
  Inspector?: (p: InspectorProps<T>) => ReactNode;
  /** Its control centre (web/src/widgets/centre.tsx): in the Designer's panel, the Operator's preview bar and its On air card. */
  Operator?: (p: OperatorProps<T>) => ReactNode;
  /** It waits for Continue (F2) on air: its next page, step, view, its start… (the Operator's Continue then serves it first). */
  waits?: (l: LayerOf<T>, state: unknown, data: Data, item: PlayoutItem) => boolean;
  /** It has controls with these settings (default: when it has Operator controls). */
  controls?: (l: LayerOf<T>, data: Data) => boolean;
  /**
   * It moves by itself (a crawl, credits, a countdown…): the Operator's preview holds it, still,
   * until its Play — at its start ('start'), or as at the On air point ('onAir': only its own
   * motion waits, its In shown played). Else not held (default).
   */
  plays?: (l: LayerOf<T>, data: Data) => 'start' | 'onAir' | false;
  /** Orange handles over the selected layer in the Designer. */
  Overlay?: (p: OverlayProps<T>) => ReactNode;
  warnings?: (l: LayerOf<T>, ctx: { data: Data; state: unknown; r?: Resolved }) => Warning[];
}

export const defineWidget = <T extends LayerType>(d: WidgetDef<T>) => d;

// ------------------------------------------------------------------ paths --

export function getPath(o: unknown, path: string): unknown {
  return path.split('.').reduce<unknown>((a, k) => (a == null ? a : (a as Record<string, unknown>)[k]), o);
}

export function setPath(o: unknown, path: string, v: unknown) {
  const ks = path.split('.');
  let a = o as Record<string, unknown>;
  for (let i = 0; i < ks.length - 1; i++) {
    if (a[ks[i]] == null) a[ks[i]] = /^\d+$/.test(ks[i + 1]) ? [] : {};
    a = a[ks[i]] as Record<string, unknown>;
  }
  if (v === undefined) delete a[ks[ks.length - 1]];
  else a[ks[ks.length - 1]] = v;
}

export const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
export const lerp = (a: number, b: number, k: number) => a + (b - a) * k;
export const ease = (k: number) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
export const easeOut = (k: number) => 1 - Math.pow(1 - k, 3);
/** `#rrggbb[aa]` → CSS colour. */
export const hex = (s: string | undefined, fallback = 'transparent') => {
  const c = s ? parseHex(s) : undefined;
  return c ? `rgba(${Math.round(c[0] * 255)}, ${Math.round(c[1] * 255)}, ${Math.round(c[2] * 255)}, ${c[3]})` : fallback;
};

// ------------------------------------------------------------- inspector --

/** The object the path-based controls read and write (the layer, or a list item). */
interface Bind {
  obj: unknown;
  set: (path: string, v: unknown, coalesce?: boolean) => void;
  data: Data;
}
const BindContext = createContext<Bind>({ obj: {}, set: () => {}, data: {} });

/** The object the path-based controls edit, its setter and the preview data. */
export const useBound = () => useContext(BindContext);

/** Path-based controls inside read / write `obj` (usually the layer). */
export function Bound({ obj, set, data, children }: { obj: unknown; set: Bind['set']; data?: Data; children: ReactNode }) {
  const parent = useContext(BindContext);
  return <BindContext.Provider value={{ obj, set, data: data ?? parent.data }}>{children}</BindContext.Provider>;
}

function useBind<V>(path: string | undefined, value: V | undefined, onChange: ((v: V) => void) | undefined): [V, (v: V, coalesce?: boolean) => void] {
  const b = useContext(BindContext);
  if (path) {
    // `value` is the default shown while the path is not set.
    const got = getPath(b.obj, path);
    return [(got == null ? value : got) as V, (v, coalesce) => b.set(path, v, coalesce)];
  }
  return [value as V, (v) => onChange?.(v)];
}

/** The settings over their defaults; a setting cleared to `undefined` keeps its default. */
export function withDefaults<D extends object, C extends object>(d: D, c: C): Omit<D, keyof C> & { [K in keyof C]-?: K extends keyof D ? Exclude<C[K], undefined> | D[K] : C[K] } {
  const out: Record<string, unknown> = { ...(d as Record<string, unknown>) };
  for (const [k, v] of Object.entries(c)) if (v !== undefined) out[k] = v;
  return out as never;
}

/** Collapsible group of the inspector (open state kept per browser), like General / Properties. */
/** Which section of a widget's inspector opens by default: the first one (its content); the others start closed. */
export const FirstSection = createContext<((id: string) => boolean) | null>(null);

export function Section({ id, title, children, actions, level = 1, closed }: { id: string; title: string; children: ReactNode; actions?: ReactNode; level?: 1 | 2; closed?: boolean }) {
  const key = `pulse.insp2.${id}`;
  const first = useContext(FirstSection);
  const [open, setOpen] = useState(() => {
    const byDefault = !closed && (first ? first(id) : true);
    try {
      const v = localStorage.getItem(key);
      return v === null ? byDefault : v !== '0';
    } catch {
      return byDefault;
    }
  });
  const toggle = () => {
    setOpen(!open);
    try {
      localStorage.setItem(key, open ? '0' : '1');
    } catch {
      /* private mode */
    }
  };
  return (
    <section className={`pp-group l${level} ${open ? 'open' : ''}`}>
      <header>
        <button className="pp-twirl" onClick={toggle} aria-expanded={open}>
          <ChevronDown size={level === 1 ? 14 : 12} />
          <span>{title}</span>
        </button>
        {actions}
      </header>
      {open && <div className="pp-body wk-body">{children}</div>}
    </section>
  );
}

/** Settings a row exposes to the Operator: a path in the layer, several, or none. */
export type Expose = string | readonly string[] | false | undefined;

/**
 * Where a widget's control centre is drawn (web/src/widgets/centre.tsx): the Designer's panel, the
 * Operator (the preview's bar), or a tile of a graphic on air (its state and actions only: no field,
 * no setting).
 */
export type CentreMode = 'designer' | 'operator' | 'air';
export const CentreModeContext = createContext<CentreMode>('designer');
export const useCentreMode = () => useContext(CentreModeContext);
/** The Operator's filter of a centre's settings: a row with `expose` paths shows only when every path is exposed. */
export const ExposedFilter = createContext<((paths: readonly string[]) => boolean) | null>(null);

/** A labelled row of the inspector; `expose` adds its expose-to-Operator button. In the Operator
 *  (ExposedFilter) a row with `expose` paths shows only when they are exposed, without the button. */
export function Row({ label, children, title, lead, tail, expose, exposeValue, exposeOptions, exposeRange, stack }: { label?: ReactNode; children: ReactNode; title?: string; lead?: ReactNode; tail?: ReactNode; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; /** A slider's bounds, kept in the exposed field (the Operator's slider). */ exposeRange?: [number, number]; /** The control under its label (a wide choice), in a control centre. */ stack?: boolean }) {
  const filter = useContext(ExposedFilter);
  const paths = expose ? (typeof expose === 'string' ? [expose] : expose) : undefined;
  if (filter) {
    if (paths && !filter(paths)) return null;
    return (
      <div className={`row wk-row op ${label === undefined ? 'full' : ''} ${stack ? 'stack' : ''}`} title={title}>
        {label !== undefined && <span className="prop-label">{label}</span>}
        <div className="wk-ctl">{children}</div>
      </div>
    );
  }
  return (
    <div className={`row wk-row ${label === undefined ? 'full' : ''} ${stack ? 'stack' : ''}`} title={title}>
      {lead ?? <span className="wk-gut" />}
      {label !== undefined && <span className="prop-label">{label}</span>}
      <div className="wk-ctl">{children}</div>
      {tail ?? (paths ? <ExposeSetting paths={paths} label={typeof label === 'string' ? label : undefined} value={exposeValue} options={exposeOptions} range={exposeRange} /> : <span className="wk-gut" />)}
    </div>
  );
}

/** The radio button exposing settings of the layer to the Operator: one click exposes, one click stops. */
export function ExposeSetting({ paths, label, value, options, range }: { paths: readonly string[]; label?: string; value?: unknown; options?: [string, string][]; range?: [number, number] }) {
  const { obj } = useContext(BindContext);
  const { template, update } = useDesigner(useShallow((s) => ({ template: s.template, update: s.update })));
  const layer = obj as Layer | undefined;
  if (!template || !layer?.id || !template.layers.some((l) => l.id === layer.id)) return <span className="wk-gut" />;
  const sb = layer.settingBindings ?? {};
  const fields = paths.map((p) => template.fields.find((f) => f.key === sb[p]));
  const on = fields.every(Boolean);
  const toggle = () =>
    update((t) => {
      const l = t.layers.find((x) => x.id === layer.id)!;
      l.settingBindings ??= {};
      if (on) {
        for (const p of paths) {
          const key = l.settingBindings[p];
          delete l.settingBindings[p];
          if (key && !fieldUsed(t, key)) t.fields = t.fields.filter((f) => f.key !== key);
        }
        if (!Object.keys(l.settingBindings).length) delete l.settingBindings;
        return;
      }
      for (const p of paths) {
        if (l.settingBindings[p] && t.fields.some((f) => f.key === l.settingBindings![p])) continue;
        // The value the Designer shows (its default when the layer does not set it).
        let cur = getPath(l, p);
        if (cur === undefined) cur = paths.length === 1 ? value : (value as Record<string, unknown> | undefined)?.[p];
        const last = p.split('.').pop()!;
        const name = paths.length > 1 || !label ? last.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/^./, (x) => x.toUpperCase()) : label;
        const key = newFieldKey(t, `${l.name} ${name}`);
        t.fields.push({ key, label: `${l.name} · ${name}`, kind: settingKind(cur, p), default: formatSetting(cur), ...(options?.length && paths.length === 1 ? { options } : {}), ...(range && paths.length === 1 ? { min: range[0], max: range[1] } : {}) });
        l.settingBindings[p] = key;
      }
    });
  const names = fields.filter(Boolean).map((f) => f!.label);
  return (
    <button className={`pp-op ${on ? 'on' : ''}`} title={on ? t('Exposed to the Operator ({fields}) · click to stop', { fields: names.join(', ') }) : t('Expose to the Operator')} onClick={toggle}>
      <Radio size={13} />
    </button>
  );
}

type Opt<V> = readonly [V, ReactNode, string?] | V;
const opts = <V,>(o: readonly Opt<V>[]) => o.map((x) => (Array.isArray(x) ? (x as readonly [V, ReactNode, string?]) : ([x as V, String(x)] as const)));

/** Segmented choice (like Lines / Scroll of the text). */
export function Seg<V extends string | number | boolean>(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: V; onChange?: (v: V) => void; options: readonly Opt<V>[]; title?: string }) {
  const [v, set] = useBind<V>(p.path, p.value, p.onChange);
  const o = opts(p.options);
  // Three wide choices (Dès le IN · Continuer F2 · Déclencheur) go under their label in a control centre.
  const wide = o.reduce((n, [, lab]) => n + (typeof lab === 'string' ? lab.length : 8), 0) > 24;
  return (
    <Row
      label={p.label}
      expose={p.expose ?? p.path}
      exposeValue={p.exposeValue ?? (p.path ? v : undefined)}
      exposeOptions={p.exposeOptions ?? (p.path || typeof p.expose === 'string' ? o.map(([val, lab]) => [String(val), typeof lab === 'string' ? lab : String(val)] as [string, string]) : undefined)}
      title={p.title}
      stack={wide}
    >
      <Segmented value={String(v)} onChange={(s) => set(o.find((x) => String(x[0]) === s)![0])} options={o.map(([val, lab, tip]) => ({ value: String(val), label: lab, title: tip }))} />
    </Row>
  );
}

export function Select<V extends string | number>(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: V; onChange?: (v: V) => void; options: readonly Opt<V>[] }) {
  const [v, set] = useBind<V>(p.path, p.value, p.onChange);
  const o = opts(p.options);
  return (
    <Row
      label={p.label}
      expose={p.expose ?? p.path}
      exposeValue={p.exposeValue ?? (p.path ? v : undefined)}
      exposeOptions={p.exposeOptions ?? (p.path || typeof p.expose === 'string' ? o.map(([val, lab]) => [String(val), typeof lab === 'string' ? lab : String(val)] as [string, string]) : undefined)}
    >
      <Dropdown value={String(v ?? '')} options={o.map(([val, lab]) => ({ value: String(val), label: typeof lab === 'string' ? lab : String(val) }))} onChange={(s) => set(o.find((x) => String(x[0]) === s)![0])} />
    </Row>
  );
}

/** On / off switch with its text. `disabled`: the reason it cannot change. */
export function Toggle(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: boolean; onChange?: (v: boolean) => void; text?: ReactNode; disabled?: string }) {
  const [v, set] = useBind<boolean>(p.path, p.value, p.onChange);
  return (
    <Row label={p.label} expose={p.expose ?? p.path} exposeValue={p.exposeValue ?? (p.path ? v : undefined)} exposeOptions={p.exposeOptions}>
      <button className={`wk-tog ${v ? 'on' : ''}`} disabled={!!p.disabled} title={p.disabled} onClick={() => set(!v)}>
        <i />
        {p.text && <span>{p.text}</span>}
      </button>
    </Row>
  );
}

/** Slider with its value. */
export function Range(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: number; onChange?: (v: number) => void; min: number; max: number; step?: number; unit?: string; fmt?: (v: number) => string }) {
  const [v, set] = useBind<number>(p.path, p.value, p.onChange);
  const step = p.step ?? 1;
  const val = Number(v ?? 0);
  return (
    <Row label={p.label} expose={p.expose ?? p.path} exposeValue={p.exposeValue ?? (p.path ? v : undefined)} exposeOptions={p.exposeOptions}>
      <input type="range" min={p.min} max={p.max} step={step} value={val} onChange={(e) => set(+e.target.value, true)} />
      <span className="wk-val">{p.fmt ? p.fmt(val) : `${+val.toFixed(step < 1 ? 2 : 0)}${p.unit ?? ''}`}</span>
    </Row>
  );
}

/** Number field (drag the label, wheel, arrows), like the properties. */
export function Num(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: number; onChange?: (v: number) => void; min?: number; max?: number; step?: number; precision?: number; unit?: string }) {
  const [v, set] = useBind<number>(p.path, p.value, p.onChange);
  return (
    <Row label={p.label} expose={p.expose ?? p.path} exposeValue={p.exposeValue ?? (p.path ? v : undefined)} exposeOptions={p.exposeOptions}>
      <span className="pp-field">
        <NumberField value={Number(v ?? 0)} min={p.min} max={p.max} step={p.step ?? 1} precision={p.precision ?? 0} suffix={p.unit?.trim()} onChange={(x) => set(x, true)} />
      </span>
    </Row>
  );
}

/**
 * A slider and its number field on one row. `onRelease`: the slider gives its value when let go
 * (a live control: one action per move, not one per pixel); the field gives it when typed.
 */
export function SliderNum({ value, onChange, min, max, step = 1, precision = 0, unit, title, onRelease }: { value: number; onChange: (v: number) => void; min: number; max: number; step?: number; precision?: number; unit?: string; title?: string; onRelease?: boolean }) {
  const [drag, setDrag] = useState<number | null>(null);
  const shown = drag ?? value;
  const done = () => {
    if (drag !== null && drag !== value) onChange(drag);
    setDrag(null);
  };
  return (
    <span className="wk-slnum" title={title}>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={Math.min(max, Math.max(min, shown))}
        onChange={(e) => (onRelease ? setDrag(+e.target.value) : onChange(+e.target.value))}
        onPointerUp={done}
        onKeyUp={done}
        onBlur={done}
        onKeyDown={(e) => e.stopPropagation()}
      />
      <span className="pp-field">
        <NumberField value={shown} min={min} max={max} step={step} precision={precision} onChange={(v) => (setDrag(null), onChange(v))} />
      </span>
      {unit && <span className="wk-unit">{unit}</span>}
    </span>
  );
}

/** Slider with its number field (typed, wheel, arrows), on a row of the properties. */
export function RangeNum(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: number; onChange?: (v: number) => void; min: number; max: number; step?: number; precision?: number; unit?: string; onRelease?: boolean }) {
  const [v, set] = useBind<number>(p.path, p.value, p.onChange);
  return (
    <Row label={p.label} expose={p.expose ?? p.path} exposeValue={p.exposeValue ?? (p.path ? v : undefined)} exposeOptions={p.exposeOptions} exposeRange={[p.min, p.max]}>
      <SliderNum value={Number(v ?? 0)} min={p.min} max={p.max} step={p.step} precision={p.precision} unit={p.unit} onRelease={p.onRelease} onChange={(x) => set(x, true)} />
    </Row>
  );
}

/** `#rrggbb[aa]` colour. */
/**
 * A colour. With `none` (what an empty colour means: "None", "Text colour"…)
 * the colour can be left empty: a button sets one, × empties it again.
 */
export function ColorPick(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: string; onChange?: (v: string) => void; none?: string; first?: string }) {
  const [v, set] = useBind<string>(p.path, p.value, p.onChange);
  const empty = p.none !== undefined && !(v ?? '').trim();
  const c = parseHex(v ?? '') ?? [1, 1, 1, 1];
  return (
    <Row label={p.label} expose={p.expose ?? p.path} exposeValue={p.exposeValue ?? (p.path ? v : undefined)} exposeOptions={p.exposeOptions}>
      {empty ? (
        <button type="button" className="pp-none" title={t('Choose a colour')} onClick={() => set(p.first ?? '#ffffff', true)}>
          {p.none}
        </button>
      ) : (
        <span className="pp-field color">
          <ColorField value={c} onChange={(x) => set(toHex(x, x[3] < 1), true)} />
          {p.none !== undefined && (
            <button type="button" className="pp-clear" title={p.none} onClick={() => set('', true)}>
              ×
            </button>
          )}
        </span>
      )}
    </Row>
  );
}

/** One-line text. */
export function Text(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: string; onChange?: (v: string) => void; placeholder?: string }) {
  const [v, set] = useBind<string>(p.path, p.value, p.onChange);
  return (
    <Row label={p.label} expose={p.expose ?? p.path} exposeValue={p.exposeValue ?? (p.path ? v : undefined)} exposeOptions={p.exposeOptions}>
      <TextInput value={String(v ?? '')} placeholder={p.placeholder} onChange={(x) => set(x, true)} />
    </Row>
  );
}

/** Several lines of text. */
export function Area(p: { label?: ReactNode; path?: string; expose?: Expose; exposeValue?: unknown; exposeOptions?: [string, string][]; value?: string; onChange?: (v: string) => void; placeholder?: string }) {
  const [v, set] = useBind<string>(p.path, p.value, p.onChange);
  return (
    <Row label={p.label} expose={p.expose ?? p.path} exposeValue={p.exposeValue ?? (p.path ? v : undefined)} exposeOptions={p.exposeOptions}>
      <TextInput multiline value={String(v ?? '')} placeholder={p.placeholder} onChange={(x) => set(x, true)} />
    </Row>
  );
}

/** {{key}} of the Data canvas in a content value. */
export const wiredKeys = (s: string) => [...String(s ?? '').matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]);

/**
 * The content of a widget (its text, value, source…). Wired in the Data
 * canvas ({{key}}), it shows the data's value, greyed and read only: it is
 * changed in the Data canvas.
 */
/** A content the Designer can expose to the Operator: exposed, it is exactly `{{key}}` with a field of its own (no data source), and editing it edits the field's default. */
export function useExposable(path: string | undefined, value: string | undefined, onChange: ((v: string) => void) | undefined, kind: FieldKind, expose?: boolean) {
  const [v, set] = useBind<string>(path, value, onChange);
  const { data, obj } = useContext(BindContext);
  const { template, update } = useDesigner(useShallow((s) => ({ template: s.template, update: s.update })));
  const raw = String(v ?? '');
  const layer = obj as { id?: string; name?: string };
  const single = /^\{\{(\w+)\}\}$/.exec(raw.trim());
  const field = single ? template?.fields.find((f) => f.key === single[1]) : undefined;
  const exposed = !!field && !field.binding?.source;
  const wired = wiredKeys(raw).length > 0 && !exposed;
  const shown = exposed ? field!.default : raw;
  const change = (x: string) => (exposed ? update((t) => void (t.fields.find((f) => f.key === field!.key)!.default = x), `field-${field!.key}`) : set(x, true));
  const canExpose = expose !== false && !!path && !!layer.id;
  /** `value`: the exposed field's value (else the content as it is); `also`: more changes in the same step. */
  const toggleExpose = (opts?: { value?: string; also?: (t: Template, l: Layer) => void }) =>
    update((t) => {
      const l = t.layers.find((x) => x.id === layer.id)!;
      opts?.also?.(t, l);
      if (exposed) {
        // Back to a plain content: the value set so far.
        setPath(l, path!, field!.default);
        const still = t.layers.some((x) => x.id !== l.id && JSON.stringify(x).includes(`{{${field!.key}}}`));
        if (!still) t.fields = t.fields.filter((f) => f.key !== field!.key);
        return;
      }
      const taken = new Set(t.fields.map((f) => f.key));
      let key =
        String(l.name || kind)
          .normalize('NFD')
          .replace(/[^A-Za-z0-9]+(.)?/g, (_, ch: string | undefined) => (ch ? ch.toUpperCase() : ''))
          .replace(/^./, (ch) => ch.toLowerCase()) || kind;
      if (!/^[a-z]/i.test(key)) key = `t${key}`;
      for (let i = 2; taken.has(key); i++) key = key.replace(/\d*$/, String(i));
      t.fields.push({ key, label: l.name || key, kind, default: opts?.value ?? raw });
      setPath(l, path!, `{{${key}}}`);
    });
  return { raw, data, field, exposed, wired, shown, change, canExpose, toggleExpose };
}

/** The radio button of a row: exposes the content to the Operator. */
export function ExposeToggle({ on, onClick }: { on: boolean; onClick: () => void }) {
  return (
    <button className={`pp-op ${on ? 'on' : ''}`} title={on ? t('Exposed to the Operator · click to stop') : t('Expose to the Operator')} onClick={onClick}>
      <Radio size={13} />
    </button>
  );
}

export function Content(p: {
  label?: ReactNode;
  path?: string;
  value?: string;
  onChange?: (v: string) => void;
  multiline?: boolean;
  placeholder?: string;
  lorem?: boolean;
  expose?: boolean;
  rich?: boolean;
  /** Parts of it made variables, pictures in it (a Text). */
  vars?: boolean;
  /** No looks of its own (the text's typography applies): no style bar, a menu on right-click. */
  plain?: boolean;
}) {
  const { raw, data, field, exposed, wired: anyKey, shown: value, change, canExpose, toggleExpose } = useExposable(p.path, p.value, p.onChange, p.multiline ? 'multiline' : 'text', p.expose);
  const tpl = useDesigner((s) => s.template);
  const { obj } = useContext(BindContext);
  const vars = useTextTokens(p.vars && !exposed ? (obj as { id?: string }).id : undefined, raw, change);
  // Its variables are typed by the Operator: edited here; read from data: shown, read only.
  const wired = anyKey && (!p.vars || wiredKeys(raw).some((k) => !tpl?.fields.some((f) => f.key === k && !f.binding?.source)));
  if (wired) return <DataValue label={p.label} value={plainText(interpolateText(raw, data))} multiline={p.multiline} />;
  const inline = p.label === undefined;
  // The whole text exposed: its variables go back to their text (asked first).
  const expose = async () => {
    if (exposed || !p.vars || !tpl) return toggleExpose();
    const keys = [...new Set([...raw.matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((m) => m[1]))].filter((k) => tpl.fields.some((f) => f.key === k && !f.binding?.source));
    if (keys.length) {
      const ok = await confirmDialog({
        title: t('Expose the whole text?'),
        message: keys.length === 1 ? t('The variable set in the text will be removed (its text stays).') : t('The {n} variables set in the text will be removed (their text stays).', { n: keys.length }),
        confirm: t('Expose the whole text'),
        danger: true,
      });
      if (!ok) return;
    }
    const text = keys.reduce((s, k) => s.split(`{{${k}}}`).join(tpl.fields.find((f) => f.key === k)?.default ?? ''), raw);
    toggleExpose({
      value: text,
      also: (t, l) => {
        const own = (l as { varStyles?: Record<string, unknown> }).varStyles;
        for (const k of keys) {
          if (own) delete own[k];
          if (!t.layers.some((x) => x.id !== l.id && JSON.stringify(x).includes(`{{${k}}}`))) t.fields = t.fields.filter((f) => f.key !== k);
        }
      },
    });
  };
  return (
    <Row label={p.label} tail={canExpose && !inline ? <ExposeToggle on={exposed} onClick={() => void expose()} /> : undefined}>
      <div className="wk-content">
        {p.rich && !exposed ? <RichInput plain={p.plain} multiline={p.multiline} value={value} placeholder={p.placeholder} onChange={change} tokens={vars.tokens} /> : <TextInput multiline={p.multiline} value={value} placeholder={p.placeholder} onChange={change} />}
        {(p.lorem || (canExpose && inline) || exposed) && (
          <div className="wk-content-tools">
            {p.lorem && (
              <button className="wk-chip" title={t('Fill with Lorem ipsum (click again for a longer one)')} onClick={() => change(nextLorem(value))}>
                <WandSparkles size={12} /> Lorem
              </button>
            )}
            {canExpose && inline ? (
              <button className={`wk-chip op ${exposed ? 'on' : ''}`} title={exposed ? t('Exposed to the Operator as “{name}” · click to stop', { name: field!.label }) : t('Expose to the Operator')} onClick={() => void expose()}>
                <Radio size={12} /> {exposed ? t('Exposed to the Operator') : p.vars ? t('Expose the whole text') : t('Expose to the Operator')}
              </button>
            ) : (
              exposed && (
                <span className="wk-exposed">
                  <Radio size={12} /> {t('Exposed to the Operator')}
                </span>
              )
            )}
          </div>
        )}
        {vars.panel}
      </div>
    </Row>
  );
}

const LOREMS = [
  'Lorem ipsum dolor sit amet',
  'Lorem ipsum dolor sit amet, consectetur adipiscing elit',
  'Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.',
  'Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.',
];
const nextLorem = (cur: string) => LOREMS[(LOREMS.indexOf(cur) + 1) % LOREMS.length];

/** A value coming from data: greyed, read only. */
export function DataValue({ label, value, multiline }: { label?: ReactNode; value: string; multiline?: boolean }) {
  return (
    <Row
      label={label}
      title={t('Comes from data: change it in the Data canvas')}
      tail={
        <span className="wk-from" title={t('From data: set in the Data canvas')}>
          <Database size={13} />
        </span>
      }
    >
      <div className="wired-value">{multiline ? <textarea className="wk-data" rows={3} readOnly value={value} /> : <input className="wk-data" readOnly value={value} />}</div>
    </Row>
  );
}

export function Note({ children }: { children: ReactNode }) {
  return <p className="hint wk-note">{children}</p>;
}

export function Msg({ level = 'info', children }: { level?: Warning['level']; children: ReactNode }) {
  return <div className={`wk-msg ${level}`}>{children}</div>;
}

export function Warnings({ list }: { list: Warning[] }) {
  if (!list.length) return null;
  return (
    <div className="wk-warnings">
      {list.map((w, i) => (
        <Msg key={i} level={w.level}>
          {w.text}
        </Msg>
      ))}
    </div>
  );
}

export interface BtnSpec {
  label: ReactNode;
  onClick: () => void;
  /** red | green | violet | amber | ghost, plus big | huge. */
  cls?: string;
  title?: string;
  disabled?: boolean;
  /** Keyboard hint shown in the button. */
  keyHint?: string;
  /** Asks before doing it. */
  confirm?: string;
}

const LEFT_KEYS = new Set(['←', 'Left']);
const KEY_NAMES: Record<string, string> = { Left: '←', Right: '→', Up: '↑', Down: '↓' };
const keyName = (k: string) => KEY_NAMES[k] ?? k;

/** Buttons of the Operator (and of the inspector); a keyboard shortcut shows as a key in the button. */
export function Btns({ list, label }: { list: (BtnSpec | false | null | undefined)[]; label?: ReactNode }) {
  const row = (
    <div className="wk-btns">
      {list
        .filter((b): b is BtnSpec => !!b)
        .map((b, i) => (
          <button
            key={i}
            className={`opx-btn wk-btn ${b.cls ?? ''}`}
            title={b.keyHint ? t('{text} · key {key}', { text: b.title ?? (typeof b.label === 'string' ? b.label : ''), key: keyName(b.keyHint) }).replace(/^ · /, '') : b.title}
            disabled={b.disabled}
            onClick={async () => {
              if (b.confirm) {
                if (!(await confirmDialog({ title: b.confirm, confirm: t('Yes') }))) return;
              }
              b.onClick();
            }}
          >
            {b.keyHint && LEFT_KEYS.has(b.keyHint) && <kbd className="kc">{keyName(b.keyHint)}</kbd>}
            <span>{b.label}</span>
            {b.keyHint && !LEFT_KEYS.has(b.keyHint) && <kbd className="kc">{keyName(b.keyHint)}</kbd>}
          </button>
        ))}
    </div>
  );
  return label === undefined ? row : <Row label={label}>{row}</Row>;
}

/** A thin vertical rule between the control groups of a bar. */
export const Sep = () => <span className="wk-sep" aria-hidden="true" />;

export interface ListField {
  key: string;
  placeholder?: string;
  /** Width in px (else flexible). */
  w?: number;
  num?: boolean;
  color?: boolean;
  options?: readonly Opt<string>[];
}

/** Editable list of objects (reorder, remove, add, on / off). */
export function ListEditor(p: { label?: ReactNode; path?: string; value?: Record<string, unknown>[]; onChange?: (v: Record<string, unknown>[]) => void; fields: ListField[]; add?: () => Record<string, unknown>; addLabel?: string; toggle?: string; min?: number; minMsg?: string }) {
  const [v, set] = useBind<Record<string, unknown>[]>(p.path, p.value, p.onChange);
  const items = v ?? [];
  const put = (next: Record<string, unknown>[], coalesce?: boolean) => set(next, coalesce);
  const box = (
    <div className="wk-list">
      {items.map((it, i) => (
        <div key={i} className={`wk-item ${p.toggle && it[p.toggle] === false ? 'off' : ''}`}>
          {p.toggle && <input type="checkbox" checked={it[p.toggle] !== false} title={t('On / off')} onChange={(e) => put(items.map((x, j) => (j === i ? { ...x, [p.toggle!]: e.target.checked } : x)))} />}
          {p.fields.map((f) => {
            const val = it[f.key];
            const change = (nv: unknown, coalesce?: boolean) =>
              put(
                items.map((x, j) => (j === i ? { ...x, [f.key]: nv } : x)),
                coalesce,
              );
            const style = f.w ? { flex: `0 0 ${f.w}px`, width: f.w } : undefined;
            if (f.options) {
              const o = opts(f.options).map(([v, l]) => ({ value: String(v), label: typeof l === 'string' ? l : String(v) }));
              // An on / off kept as a boolean: false is the option '' (unless 'false' is one).
              const cur = typeof val === 'boolean' && !o.some((x) => x.value === String(val)) ? (val ? 'true' : '') : String(val ?? '');
              return (
                <span key={f.key} className="dd-cell" style={style}>
                  <Dropdown value={cur} options={o} onChange={(nv) => change(nv)} />
                </span>
              );
            }
            if (f.color) return <HexColorField key={f.key} compact value={String(val ?? '#ffffff')} onChange={(v) => change(v, true)} />;
            return <input key={f.key} style={style} placeholder={f.placeholder ?? f.key} value={String(val ?? '')} onKeyDown={(e) => e.stopPropagation()} onChange={(e) => change(f.num ? +e.target.value : e.target.value, true)} />;
          })}
          <button className="wk-x" title={t('Up')} disabled={!i} onClick={() => put(items.map((x, j) => (j === i - 1 ? items[i] : j === i ? items[i - 1] : x)))}>
            ↑
          </button>
          <button
            className="wk-x"
            title={t('Remove')}
            onClick={async () => {
              if (p.min && items.length <= p.min) {
                return alertDialog({ title: p.minMsg ?? t('At least {n}', { n: p.min }) });
              }
              put(items.filter((_, j) => j !== i));
            }}
          >
            ✕
          </button>
        </div>
      ))}
      {p.add && (
        <button className="opx-btn small wk-add" onClick={() => put([...items, p.add!()])}>
          + {p.addLabel ?? t('Add')}
        </button>
      )}
    </div>
  );
  return p.label === undefined ? (
    box
  ) : (
    <Row label={p.label} expose={p.path} exposeValue={items}>
      {box}
    </Row>
  );
}

/** Grid of strings (first row = header when `header`), with CSV paste. */
export function GridEditor(p: { path?: string; value?: string[][]; onChange?: (v: string[][]) => void; header?: boolean; csv?: boolean }) {
  const [v, set] = useBind<string[][]>(p.path, p.value, p.onChange);
  const rows = v?.length ? v : [['']];
  const put = (next: string[][], coalesce?: boolean) => set(next, coalesce);
  const cell = (i: number, j: number, s: string) =>
    put(
      rows.map((r, a) => (a === i ? r.map((c, b) => (b === j ? s : c)) : r)),
      true,
    );
  return (
    <div className="wk-grid">
      <div className="wk-grid-scroll">
        <table>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i} className={i === 0 && p.header ? 'head' : ''}>
                {r.map((c, j) => (
                  <td key={j}>
                    <input value={c} onKeyDown={(e) => e.stopPropagation()} onChange={(e) => cell(i, j, e.target.value)} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="wk-btns">
        <button className="opx-btn small" onClick={() => put([...rows, rows[0].map(() => '')])}>
          {t('+ Row')}
        </button>
        <button className="opx-btn small" onClick={() => rows.length > (p.header ? 2 : 1) && put(rows.slice(0, -1))}>
          {t('− Row')}
        </button>
        <button className="opx-btn small" onClick={() => put(rows.map((r, i) => [...r, i === 0 && p.header ? `Col ${r.length + 1}` : '']))}>
          {t('+ Column')}
        </button>
        <button className="opx-btn small" onClick={() => rows[0].length > 1 && put(rows.map((r) => r.slice(0, -1)))}>
          {t('− Column')}
        </button>
        {p.csv && (
          <button
            className="opx-btn small"
            onClick={async () => {
              const csv = await promptDialog({ title: t('Paste CSV'), message: t('Comma, semicolon or tab separated; one row per line.'), value: rows.map((r) => r.join(';')).join('\n') });
              if (csv) put(parseCsv(csv));
            }}
          >
            {t('Import CSV…')}
          </button>
        )}
      </div>
    </div>
  );
}

/** Rows of a CSV text (`,` `;` or tab; quotes allowed). */
const cellOf = (v: unknown) => (v == null ? '' : typeof v === 'string' ? v : JSON.stringify(v));
/** A data set's whole table (`[["col", …], [cell, …], …]`: the header, then the rows). */
export function headerRows(a: unknown[]): { head: string[]; rows: string[][] } | null {
  if (!a.length || !a.every((r) => Array.isArray(r))) return null;
  const [h, ...rows] = a as unknown[][];
  return { head: h.map(cellOf), rows: rows.map((r) => r.map(cellOf)) };
}
/** The column named one of `names` (any case), or -1. */
export const findCol = (head: string[], names: string[]) => head.findIndex((h) => names.includes(h.trim().toLowerCase()));

export function parseCsv(text: string): string[][] {
  const lines = text
    .replace(/\r/g, '')
    .split('\n')
    .filter((l) => l.trim());
  const sep = [';', '\t', ','].find((s) => lines[0]?.includes(s)) ?? ',';
  return lines.map((l) => {
    const out: string[] = [];
    let cur = '';
    let q = false;
    for (let i = 0; i < l.length; i++) {
      const ch = l[i];
      if (q) {
        if (ch === '"' && l[i + 1] === '"') ((cur += '"'), i++);
        else if (ch === '"') q = false;
        else cur += ch;
      } else if (ch === '"') q = true;
      else if (ch === sep) (out.push(cur), (cur = ''));
      else cur += ch;
    }
    out.push(cur);
    return out.map((s) => s.trim());
  });
}

// ------------------------------------------------------------- operator --

/**
 * How many times a second the readouts of the Operator's controls redraw (useFrame): 30 is as
 * smooth as a readout needs; controls mounted hidden (for their keys) need far fewer.
 */
export const ReadoutFps = createContext(30);

/** Re-renders the component every animation frame while `active`, at most ReadoutFps a second (live readouts): the time now (Date.now()). */
export function useFrame(active = true): number {
  const fps = useContext(ReadoutFps);
  const [now, setNow] = useState(Date.now);
  useEffect(() => (active ? onFrames(() => setNow(Date.now()), fps) : undefined), [active, fps]);
  return active ? now : Date.now();
}

/**
 * A value read from the time on each frame (at most ReadoutFps a second, while `active`) that
 * redraws the component only when it changes (Object.is): a readout that changes once a
 * second, a state that flips at the end of a list…, without drawing the whole row each frame.
 */
export function useFrameValue<T>(compute: () => T, active = true): T {
  const fps = useContext(ReadoutFps);
  const value = compute();
  const fn = useRef(compute);
  fn.current = compute;
  const last = useRef(value);
  last.current = value;
  const [, redraw] = useState(0);
  useEffect(
    () =>
      active
        ? onFrames(() => {
            const v = fn.current();
            if (!Object.is(v, last.current)) {
              last.current = v;
              redraw((n) => n + 1);
            }
          }, fps)
        : undefined,
    [active, fps],
  );
  return value;
}

/**
 * Where a widget's keys apply: on air, only while its row is selected in the On air panel (true);
 * never from the Operator's preview bar (false); no scope (null, the Designer's try-out): always.
 */
export const KeyScope = createContext<boolean | null>(null);

/**
 * Keyboard shortcuts of an Operator panel while it is shown (not in fields). A key handled here is
 * not passed on (the Operator's Space / Enter / arrows); one handler per key and event.
 */
export function useKeys(map: Record<string, () => void>) {
  const ref = useRef(map);
  ref.current = map;
  const scope = useContext(KeyScope);
  const active = useRef(scope);
  active.current = scope;
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (active.current === false || e.defaultPrevented) return;
      if ((e.target as HTMLElement).closest('input, textarea, select, [contenteditable]')) return;
      const fn = ref.current[e.key] ?? ref.current[e.code];
      if (!fn) return;
      e.preventDefault();
      // A focused button would also "click" on Space / Enter.
      (e.target as HTMLElement).closest?.('button')?.blur();
      fn();
    };
    // Before the Operator's own keys (bubbling on window).
    addEventListener('keydown', on, true);
    return () => removeEventListener('keydown', on, true);
  }, []);
}

/** The ⋯ of a compact row (the table editor's rows), given by its host. */
export const MoreToggle = createContext<{ open: boolean; toggle: () => void } | null>(null);

/** The ⋯ button (OpRow adds it by itself). */
export function MoreButton() {
  const more = useContext(MoreToggle);
  if (!more) return null;
  return (
    <button className={`opx-sq ${more.open ? 'on' : ''}`} title={t('More')} aria-expanded={more.open} onClick={more.toggle}>
      <Ellipsis size={14} />
    </button>
  );
}

/** A button of a compact Operator row: an icon, its text in the tooltip. */
export interface OpBtn {
  icon: LucideIcon;
  title: string;
  onClick: () => void;
  disabled?: boolean;
  tone?: 'green' | 'amber' | 'violet';
  /** Asks first (on air). */
  confirm?: string;
  /** A few letters next to the icon (a team's short name…). */
  tag?: string;
}

/** The compact controls of a widget: a small readout, then icon buttons (and the ⋯ of its More controls, unless `noMore`). */
export function OpRow({ value, list, noMore }: { value?: ReactNode; list: (OpBtn | false | null | undefined)[]; noMore?: boolean }) {
  return (
    <div className="wk-oprow">
      {value != null && value !== '' && <b className="wk-oprow-v">{value}</b>}
      {list
        .filter((b): b is OpBtn => !!b)
        .map((b, i) => (
          <button
            key={i}
            className={`opx-sq ${b.tone ?? ''} ${b.tag ? 'tagged' : ''}`}
            title={b.title}
            disabled={b.disabled}
            onClick={async () => {
              if (b.confirm && !(await confirmDialog({ title: b.confirm, confirm: t('Yes') }))) return;
              b.onClick();
            }}
          >
            <b.icon size={13} />
            {b.tag && <span>{b.tag}</span>}
          </button>
        ))}
      {!noMore && <MoreButton />}
    </div>
  );
}

// -------------------------------------------------------------- overlay --

/** An orange handle on the stage at layer coordinates (x, y); drag gives layer coordinates. */
export function Handle({ x, y, scale, label, onDrag, onDone }: { x: number; y: number; scale: number; label?: string; onDrag: (x: number, y: number, e: PointerEvent) => void; onDone?: () => void }) {
  return (
    <div
      className="wk-handle"
      title={label}
      style={{ left: x * scale, top: y * scale }}
      onPointerDown={(e) => {
        e.stopPropagation();
        e.preventDefault();
        const host = (e.currentTarget.parentElement as HTMLElement).getBoundingClientRect();
        const move = (ev: PointerEvent) => onDrag((ev.clientX - host.left) / scale, (ev.clientY - host.top) / scale, ev);
        const up = () => {
          removeEventListener('pointermove', move);
          removeEventListener('pointerup', up);
          onDone?.();
        };
        addEventListener('pointermove', move);
        addEventListener('pointerup', up);
      }}
    />
  );
}

/** A layer-sized surface of the overlay catching clicks (adding points…), in layer coordinates. */
export function Catcher({ scale, cursor = 'crosshair', onDown }: { scale: number; cursor?: string; onDown: (x: number, y: number, e: React.PointerEvent) => void }) {
  return (
    <div
      className="wk-catch"
      style={{ cursor }}
      onPointerDown={(e) => {
        e.stopPropagation();
        const r = e.currentTarget.getBoundingClientRect();
        onDown((e.clientX - r.left) / scale, (e.clientY - r.top) / scale, e);
      }}
    />
  );
}

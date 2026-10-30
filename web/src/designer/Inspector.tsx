import { Boundary } from '../ui/Boundary';
import { useShallow } from 'zustand/react/shallow';
import { Dropdown } from '../ui/Dropdown';
import { FontPick } from '../ui/FontPick';
import { AssetLibrary, AssetPicker } from './AssetPicker';
import { loadLottieAsset } from './lottie';
import { fieldUsed } from '../engine/settings';
import { Check, Database, Radio, RotateCcw, AlignCenter, AlignLeft, AlignRight, ChevronDown, ChevronLeft, ChevronRight, Diamond, Link as LinkIcon, Link2, LayoutTemplate, Settings2, Sparkles, Unlink } from 'lucide-react';
import { createContext, Fragment, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { applyEasing, col, holdFrame, interpolateText, num, parseHex, propValue, resolveLayer, toHex } from '../engine/evaluate';
import { applyLogic, measureLayer } from '../engine/logic';
import { useBoundData } from '../data/dataStore';
import { MAIN_FORMAT, rowKey, TEXT_LIKE, usedDatasets, type DataSet, type TextOverflow, type Easing, type FontStyle, type Layer, type LayerType, type Prop, type Template, type Value } from '../engine/types';
import { useApp } from '../store';
import { ColorField, NumberField, Segmented, TextInput } from '../ui/fields';
import { useZoneEdit } from './ZoneEditor';
import { loadFonts, useFonts } from '../engine/fonts';
import { isOverridden, resetOverride } from '../engine/formats';
import { findLayer, isAnimated, LABELS, setProp, toggleKeyAt, useDesigner } from './designerStore';
import { isTimed } from '../engine/widgets';
import { onTimeline, setOnTimeline } from '../engine/timelineProps';
import { WidgetRunButtons } from './WidgetRun';
import { AreaInspector, useGuidesView } from './Guides';
import { fitLayer, layerText } from '../engine/Scene';
import { TemplatePublish } from './Collab';
import { layerIcon } from './LayersPanel';
import { widgetOf } from '../widgets';
import { Bound, CentreModeContext, FirstSection, Row as KitRow, Section as SpecSection, setPath, Toggle, Warnings, wiredKeys } from '../widgets/kit';
import { t, tc, tn } from '../i18n';
import { useDisplayPref } from '../displayPrefs';
import { START_MODES, startMode } from '../widgets/centre';
import { datasetUse, useDatasetRows } from '../operator/DataSets';

interface PropSpec {
  prop: Prop;
  label: string;
  display?: number;
  suffix?: string;
  step?: number;
  precision?: number;
  min?: number;
  max?: number;
}

const TRANSFORM: PropSpec[] = [
  { prop: 'x', label: 'X' },
  { prop: 'y', label: 'Y' },
  { prop: 'width', label: 'W', min: 0 },
  { prop: 'height', label: 'H', min: 0 },
  { prop: 'anchorX', label: 'Anchor X', display: 100, suffix: '%' },
  { prop: 'anchorY', label: 'Anchor Y', display: 100, suffix: '%' },
  { prop: 'rotation', label: 'Rotation', suffix: '°', precision: 1 },
  { prop: 'scaleX', label: 'Scale X', display: 100, suffix: '%' },
  { prop: 'scaleY', label: 'Scale Y', display: 100, suffix: '%' },
  {
    prop: 'opacity',
    label: 'Opacity',
    display: 100,
    suffix: '%',
    min: 0,
    max: 1,
  },
];
const STYLE: PropSpec[] = [
  { prop: 'strokeWidth', label: 'Stroke W', min: 0, precision: 1 },
  { prop: 'cornerRadius', label: 'Radius', min: 0 },
];
const TEXT: PropSpec[] = [
  { prop: 'fontSize', label: 'Size', min: 1 },
  { prop: 'letterSpacing', label: 'Tracking', precision: 1 },
];
const CROP: PropSpec[] = [
  {
    prop: 'cropLeft',
    label: 'Left',
    display: 100,
    suffix: '%',
    min: 0,
    max: 1,
  },
  {
    prop: 'cropRight',
    label: 'Right',
    display: 100,
    suffix: '%',
    min: 0,
    max: 1,
  },
  { prop: 'cropTop', label: 'Top', display: 100, suffix: '%', min: 0, max: 1 },
  {
    prop: 'cropBottom',
    label: 'Bottom',
    display: 100,
    suffix: '%',
    min: 0,
    max: 1,
  },
];

export const PROP_LABELS: Record<Prop, string> = Object.fromEntries([...TRANSFORM, ...STYLE, ...TEXT, ...CROP.map((c) => ({ ...c, label: `Crop ${c.label}` })), { prop: 'fill', label: 'Fill' }, { prop: 'stroke', label: 'Stroke' }, { prop: 'arcStart', label: 'Start' }, { prop: 'arcEnd', label: 'End' }, { prop: 'innerRadius', label: 'Inner radius' }, { prop: 'shadowColor', label: 'Shadow color' }, { prop: 'shadowX', label: 'Shadow X' }, { prop: 'shadowY', label: 'Shadow Y' }, { prop: 'shadowBlur', label: 'Shadow blur' }, { prop: 'drawStart', label: 'Start' }, { prop: 'drawEnd', label: 'End' }, { prop: 'cornerTl', label: 'Top left' }, { prop: 'cornerTr', label: 'Top right' }, { prop: 'cornerBr', label: 'Bottom right' }, { prop: 'cornerBl', label: 'Bottom left' }, { prop: 'fill2', label: 'To' }].map((s) => [s.prop, s.label])) as Record<Prop, string>;

export function Section({ title, children, actions }: { title: string; children: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <section className="insp-section">
      <header>
        <span>{title}</span>
        {actions}
      </header>
      <div className="insp-body">{children}</div>
    </section>
  );
}

/** Collapsible group of the properties panel (kept for the user). */
function Group({ id, title, level = 1, children, actions }: { id: string; title: string; level?: 1 | 2; children: React.ReactNode; actions?: React.ReactNode }) {
  const [open, setOpen] = useDisplayPref(`designer.group.${id}`, false);
  const toggle = () => setOpen(!open);
  return (
    <section className={`pp-group l${level} ${open ? 'open' : ''}`}>
      <header>
        <button className="pp-twirl" onClick={toggle} aria-expanded={open}>
          <ChevronDown size={level === 1 ? 14 : 12} />
          <span>{title}</span>
        </button>
        {actions}
      </header>
      {open && <div className="pp-body">{children}</div>}
    </section>
  );
}

export interface RowSpec {
  label: string;
  props: Prop[];
  /** Field labels (X / Y, W / H…). */
  fields?: string[];
  display?: number;
  suffix?: string;
  precision?: number;
  min?: number;
  max?: number;
  /** The two values keep their ratio when this setting of the layer is on (Scale: scaleLock, Size: lockRatio). */
  lock?: 'scaleLock' | 'lockRatio';
}

const TRANSFORM_ROWS: RowSpec[] = [
  { label: 'Position', props: ['x', 'y'], fields: ['X', 'Y'], suffix: 'px' },
  {
    label: 'Scale',
    props: ['scaleX', 'scaleY'],
    fields: ['X', 'Y'],
    display: 100,
    suffix: '%',
    lock: 'scaleLock',
  },
  {
    // "Size", as in the timeline.
    label: 'Size',
    props: ['width', 'height'],
    fields: ['W', 'H'],
    suffix: 'px',
    min: 0,
    lock: 'lockRatio',
  },
  {
    label: 'Anchor',
    props: ['anchorX', 'anchorY'],
    fields: ['X', 'Y'],
    display: 100,
    suffix: '%',
  },
  { label: 'Rotation', props: ['rotation'], suffix: '°', precision: 1 },
  {
    label: 'Opacity',
    props: ['opacity'],
    display: 100,
    suffix: '%',
    min: 0,
    max: 1,
  },
];

/**
 * ◆ of a property row. Off the timeline (dim): click puts the property on
 * the timeline. On it (yellow): click adds / removes the keyframe at the
 * playhead (filled when there is one).
 */
export function TimelineButton({ layer, props }: { layer: Layer; props: Prop[] }) {
  const { frame, update } = useDesigner(useShallow((s) => ({ frame: s.frame, update: s.update })));
  const on = props.every((p) => onTimeline(layer).has(p));
  const here = props.some((p) => (layer.tracks[p] ?? []).some((k) => Math.abs(k.frame - frame) < 0.5));
  return (
    <button
      className={`pp-tl ${on ? 'on' : ''} ${here ? 'key' : ''}`}
      title={!on ? t('Not on the timeline: click to animate it') : here ? t('Remove the keyframe at the playhead') : t('On the timeline: click to add a keyframe at the playhead')}
      onClick={() => {
        update((t) => {
          const l = findLayer(t, layer.id)!;
          if (!on) return setOnTimeline(l, props, true);
          for (const p of props) if (here === (l.tracks[p] ?? []).some((k) => Math.abs(k.frame - frame) < 0.5)) toggleKeyAt(l, p, frame);
        });
        if (on && !here) useDesigner.setState((st) => ({ expanded: { ...st.expanded, [layer.id]: true } }));
      }}
    >
      <Diamond size={12} fill={here ? 'currentColor' : 'none'} />
    </button>
  );
}

/** Lit when a Logic relation drives one of the row's properties. */
function LogicBadge({ layer, props }: { layer: Layer; props: Prop[] }) {
  const driven = useDesigner((s) => s.template?.logic?.nodes.some((n) => n.type === 'target' && n.layer === layer.id && props.includes(n.prop as Prop)));
  if (!driven) return null;
  return (
    <span className="logic-badge" title={t('Driven by a Logic relation (Logic button in the toolbar)')}>
      <Link2 size={11} />
    </span>
  );
}

/** (•) at the end of a row: exposes the property to the Operator (a variable he sets), or stops. */
export function ExposeButton({ layer, props }: { layer: Layer; props: Prop[] }) {
  const { update, template } = useDesigner(useShallow((s) => ({ update: s.update, template: s.template })));
  if (!template) return <span className="pp-op-space" />;
  const bound = props.map((p) => layer.bindings[p]).filter((k): k is string => !!k);
  const on = bound.length > 0;
  const fields = template.fields.filter((f) => bound.includes(f.key));
  const expose = () =>
    update((tpl) => {
      const l = findLayer(tpl, layer.id)!;
      for (const prop of props) {
        const isColor = prop === 'fill' || prop === 'stroke';
        const name = PROP_LABELS[prop] ?? prop;
        // The key from the English name (an identifier), the label in the interface's language.
        const key = fieldKey(`${l.name} ${name}`, new Set(tpl.fields.map((f) => f.key)));
        // Default: what is seen on air.
        const v = propValue(l, prop, holdFrame(tpl));
        tpl.fields.push({ key, label: `${l.name} · ${t(name)}`, kind: isColor ? 'color' : 'number', default: isColor ? toHex(col(v), col(v)[3] < 1) : String(+num(v).toFixed(3)) });
        l.bindings[prop] = key;
      }
    });
  const unbind = () =>
    update((t) => {
      const l = findLayer(t, layer.id)!;
      for (const prop of props) {
        const key = l.bindings[prop];
        delete l.bindings[prop];
        // A field made for this binding and used nowhere else goes too.
        if (key && !fieldUsed(t, key)) t.fields = t.fields.filter((f) => f.key !== key);
      }
    });
  const fromSource = fields.some((f) => f.binding?.source);
  return (
    <button
      className={`pp-op ${on ? 'on' : ''}`}
      title={on ? (fromSource ? t('Exposed to the Operator ({fields}) · fed by a data source · click to stop', { fields: fields.map((f) => f.label).join(', ') || bound.join(', ') }) : t('Exposed to the Operator ({fields}) · click to stop', { fields: fields.map((f) => f.label).join(', ') || bound.join(', ') })) : t('Expose to the Operator')}
      onClick={() => (on ? unbind() : expose())}
    >
      <Radio size={13} />
    </button>
  );
}

/** Values of the selected layer set by data (a bound variable or a relation of the Data canvas), at the playhead. */
const DrivenContext = createContext<Map<string, Value>>(new Map());

function useDrivenValues(layer: Layer): Map<string, Value> {
  const { template, frame, previewData } = useDesigner(useShallow((s) => ({ template: s.template, frame: s.frame, previewData: s.previewData })));
  const bound = useBoundData(template, previewData);
  return useMemo(() => {
    const out = new Map<string, Value>();
    if (!template) return out;
    const targets = template.logic?.nodes.filter((n): n is typeof n & { type: 'target' } => n.type === 'target' && n.layer === layer.id && !!template.logic?.edges.some((e) => e.to === n.id)) ?? [];
    const driven = new Set<Prop>([...(Object.entries(layer.bindings) as [Prop, string][]).filter(([, k]) => k).map(([p]) => p), ...targets.map((t) => t.prop)]);
    if (!driven.size) return out;
    const data = { ...previewData, ...bound };
    const hold = holdFrame(template);
    const resolved = new Map(template.layers.map((l) => [l.id, resolveLayer(l, frame, data, hold)]));
    applyLogic(template.logic, resolved, data, measureLayer(template.layers, data, interpolateText));
    const r = resolved.get(layer.id);
    if (r) for (const p of driven) out.set(p, r[p]);
    return out;
  }, [template, frame, previewData, bound, layer]);
}

/** A value set by data: shown, not edited (change it in the Data canvas or the Operator). */
function DrivenValue({ value, display = 1, suffix, precision = 0, label }: { value: Value; display?: number; suffix?: string; precision?: number; label?: string }) {
  return (
    <span className="pp-driven" title={t('Set by data (Data canvas): read only')}>
      {label && <span className="scrub">{t(label)}</span>}
      <span className="pp-driven-v">{+(num(value) * display).toFixed(precision)}</span>
      {suffix && <span className="suffix">{suffix}</span>}
    </span>
  );
}

/** A property row: ◀ ◆ ▶, name, animate, its fields (X / Y…). */
export function GroupRow({ layer, spec }: { layer: Layer; spec: RowSpec }) {
  const { frame, update, format, updateBase } = useDesigner(useShallow((s) => ({ frame: s.frame, update: s.update, format: s.format, updateBase: s.updateBase })));
  const driven = useContext(DrivenContext);
  const locked = !!spec.lock && !!layer[spec.lock];
  const overridden = format !== MAIN_FORMAT && spec.props.some((p) => isOverridden(layer, format, p));
  const setValue = (prop: Prop, nv: number) =>
    update((t) => {
      const l = findLayer(t, layer.id)!;
      if (locked && spec.props.length === 2) {
        // Keep the X / Y ratio.
        const [a, b] = spec.props;
        const other = prop === a ? b : a;
        const cur = num(propValue(l, prop, frame));
        const ov = num(propValue(l, other, frame));
        setProp(l, other, cur ? (ov * nv) / cur : nv, frame);
      }
      setProp(l, prop, nv, frame);
    }, `prop-${layer.id}-${spec.label}`);
  return (
    <div className={`pp-row ${spec.props.some((p) => isAnimated(layer, p)) ? 'animated' : ''}`}>
      <TimelineButton layer={layer} props={spec.props} />
      <span className="pp-label">
        {t(spec.label)}
        <LogicBadge layer={layer} props={spec.props} />
      </span>
      <div className="pp-fields">
        {spec.props.map((p, i) => (
          <span key={p} className="pp-field">
            {driven.has(p) ? (
              <DrivenValue value={driven.get(p)!} label={spec.fields?.[i]} display={spec.display} suffix={spec.suffix} precision={spec.precision ?? (spec.display ? 0 : 1)} />
            ) : (
              <NumberField label={spec.fields?.[i] && t(spec.fields[i])} value={num(propValue(layer, p, frame))} display={spec.display} suffix={spec.suffix} precision={spec.precision} min={spec.min} max={spec.max} onChange={(nv) => setValue(p, nv)} />
            )}
          </span>
        ))}
        {spec.lock && (
          <button className={`pp-lock ${locked ? 'on' : ''}`} title={locked ? t('Proportions locked (click to unlock)') : t('Lock the proportions')} onClick={() => update((t) => void (findLayer(t, layer.id)![spec.lock!] = !locked || undefined))}>
            {locked ? <LinkIcon size={12} /> : <Unlink size={12} />}
          </button>
        )}
        {overridden && (
          <button className="override-mark" title={t('Changed in this format — click to use the main layout again')} onClick={() => updateBase((tpl) => spec.props.forEach((p) => resetOverride(findLayer(tpl, layer.id)!, format, p)))}>
            <RotateCcw size={10} />
          </button>
        )}
      </div>
      <ExposeButton layer={layer} props={spec.props} />
    </div>
  );
}

/** Colour row in the same layout. */
export function ColorGroupRow({ layer, prop, label }: { layer: Layer; prop: 'fill' | 'stroke' | 'shadowColor' | 'fill2'; label: string }) {
  const { frame, update } = useDesigner(useShallow((s) => ({ frame: s.frame, update: s.update })));
  const driven = useContext(DrivenContext).get(prop);
  return (
    <div className={`pp-row ${isAnimated(layer, prop) ? 'animated' : ''}`}>
      <TimelineButton layer={layer} props={[prop]} />
      <span className="pp-label">
        {t(label)}
        <LogicBadge layer={layer} props={[prop]} />
      </span>
      <div className="pp-fields">
        <span className="pp-field color">
          {driven ? (
            <span className="pp-driven" title={t('Set by data (Data canvas): read only')}>
              <span className="dc-swatch" style={{ background: toHex(col(driven), true) }} />
              <span className="pp-driven-v">{toHex(col(driven), true)}</span>
            </span>
          ) : (
            <ColorField value={col(propValue(layer, prop, frame))} onChange={(c) => update((t) => setProp(findLayer(t, layer.id)!, prop, c as Value, frame), `prop-${layer.id}-${prop}`)} />
          )}
        </span>
        <OverrideMark layer={layer} prop={prop} />
      </div>
      <ExposeButton layer={layer} props={[prop]} />
    </div>
  );
}

/** Field key from a label: "Name plate X" -> "namePlateX". */
export function fieldKey(label: string, taken: Set<string>): string {
  const words = label
    .normalize('NFD')
    .replace(/[^A-Za-z0-9 ]/g, ' ')
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  let key = words.map((w, i) => (i ? w[0].toUpperCase() + w.slice(1).toLowerCase() : w.toLowerCase())).join('') || 'field';
  if (/^\d/.test(key)) key = `f${key}`;
  let k = key;
  for (let n = 2; taken.has(k); n++) k = `${key}${n}`;
  return k;
}

/** Dot on a property changed in the current format; click to inherit again. */
function OverrideMark({ layer, prop }: { layer: Layer; prop: Prop }) {
  const { format, updateBase } = useDesigner(useShallow((s) => ({ format: s.format, updateBase: s.updateBase })));
  if (format === MAIN_FORMAT || !isOverridden(layer, format, prop)) return null;
  return (
    <button className="override-mark" title={t('Changed in this format — click to use the main layout again')} onClick={() => updateBase((tpl) => resetOverride(findLayer(tpl, layer.id)!, format, prop))}>
      <RotateCcw size={10} />
    </button>
  );
}

export { AssetPicker };

export type Attrs = Record<string, unknown>;

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="row">
      <span className="prop-label">{label}</span>
      {children}
    </div>
  );
}

function Num({ value, onChange, min, max, step, precision, suffix }: { value: number; onChange: (v: number) => void; min?: number; max?: number; step?: number; precision?: number; suffix?: string }) {
  return <NumberField value={value} onChange={onChange} min={min} max={max} step={step ?? 1} precision={precision ?? 0} suffix={suffix} />;
}

const CLOCK_FORMATS = ['HH:mm:ss', 'HH:mm', 'h:mm A', 'mm:ss', 'mm:ss.f', 'H:mm:ss', 'dd/MM/yyyy', 'dd/MM/yy HH:mm', 'yyyy-MM-dd'];
const ZONES: string[] = (() => {
  try {
    return (Intl as unknown as { supportedValuesOf(k: string): string[] }).supportedValuesOf('timeZone');
  } catch {
    return ['UTC', 'Europe/Paris', 'Europe/London', 'America/New_York', 'Asia/Tokyo'];
  }
})();

function ZoneInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <>
      <input list="insp-zones" value={value} placeholder={t('Server zone')} onChange={(e) => onChange(e.target.value)} onKeyDown={(e) => e.stopPropagation()} />
      <datalist id="insp-zones">
        {ZONES.map((z) => (
          <option key={z} value={z} />
        ))}
      </datalist>
    </>
  );
}

function HexInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const c = parseHex(value) ?? [0, 0, 0, 0];
  return <ColorField value={c} onChange={(v) => onChange(toHex(v, v[3] < 1))} />;
}

/** Settings of prefab widgets. */
export function WidgetSettings({ layer, set, data = {} }: { layer: Layer; set: (key: string, v: unknown, coalesce?: boolean) => void; data?: Record<string, string> }) {
  const a = layer as unknown as Attrs;
  // Content wired in the Data canvas: the data's value, greyed and read only.
  const text = (key: string, multiline = false, placeholder?: string) =>
    wiredKeys(String(a[key] ?? '')).length ? <WiredValue value={interpolateText(String(a[key]), data)} multiline={multiline} /> : <TextInput multiline={multiline} value={String(a[key] ?? '')} placeholder={placeholder} onChange={(v) => set(key, v, true)} />;
  const check = (key: string, label: string) => (
    <label className="check">
      <input type="checkbox" checked={!!a[key]} onChange={(e) => set(key, e.target.checked)} />
      {label}
    </label>
  );
  switch (layer.type) {
    case 'clock':
      return (
        <>
          <Row label={t('Mode')}>
            <Dropdown
              value={layer.mode}
              onChange={(v) => set('mode', v)}
              options={[
                { value: 'time', label: t('Time of day') },
                { value: 'countdown', label: t('Countdown to…') },
                { value: 'timer', label: t('Timer (from take)') },
                { value: 'elapsed', label: t('Stopwatch (since take)') },
              ]}
            />
          </Row>
          <Row label={t('Format')}>
            <input list="clock-formats" value={layer.format} onChange={(e) => set('format', e.target.value, true)} onKeyDown={(e) => e.stopPropagation()} />
            <datalist id="clock-formats">
              {CLOCK_FORMATS.map((f) => (
                <option key={f} value={f} />
              ))}
            </datalist>
          </Row>
          {(layer.mode === 'time' || layer.mode === 'countdown') && (
            <Row label={t('Time zone')}>
              <ZoneInput value={layer.timezone} onChange={(v) => set('timezone', v, true)} />
            </Row>
          )}
          {layer.mode === 'countdown' && (
            <>
              <span className="prop-label">{t('Target — “20:00” today or “2026-12-31T23:59:59”')}</span>
              {text('target', false, '20:00')}
            </>
          )}
          {layer.mode === 'timer' && (
            <>
              <span className="prop-label">{t('Duration — “5:00” or “1:30:00”')}</span>
              {text('target', false, '5:00')}
            </>
          )}
          <p className="hint">{t('Tokens: yyyy yy MM M dd d HH H hh h mm m ss s f (tenths) A (AM/PM).')}</p>
        </>
      );
    case 'ticker':
      return (
        <>
          <span className="prop-label">{t('Items (one per line)')}</span>
          {text('text', true)}
          <Row label={t('Separator')}>
            <input value={layer.separator} onChange={(e) => set('separator', e.target.value, true)} onKeyDown={(e) => e.stopPropagation()} />
          </Row>
          <Row label={t('Speed')}>
            <Num value={layer.speed} onChange={(v) => set('speed', v, true)} min={1} suffix=" px/s" />
          </Row>
        </>
      );
    case 'roll':
      return (
        <>
          <span className="prop-label">{t('Text (scrolls upwards)')}</span>
          {text('text', true)}
          <Row label={t('Speed')}>
            <Num value={layer.speed} onChange={(v) => set('speed', v, true)} min={1} suffix=" px/s" />
          </Row>
          {check('loop', t('Loop'))}
        </>
      );
    case 'counter':
      return (
        <>
          <Row label={t('From')}>
            <TextInput value={layer.from} onChange={(v) => set('from', v, true)} />
          </Row>
          <span className="prop-label">{t('To')}</span>
          {text('to')}
          <Row label={t('Duration')}>
            <Num value={layer.duration} onChange={(v) => set('duration', v, true)} min={0} step={0.1} precision={1} suffix=" s" />
          </Row>
          <Row label={t('Decimals')}>
            <Num value={layer.decimals} onChange={(v) => set('decimals', Math.round(v), true)} min={0} max={6} />
          </Row>
          <Row label={t('Prefix')}>
            <TextInput value={layer.prefix} onChange={(v) => set('prefix', v, true)} />
          </Row>
          <Row label={t('Suffix')}>
            <TextInput value={layer.suffix} onChange={(v) => set('suffix', v, true)} />
          </Row>
          {check('grouping', t('Thousands separator'))}
        </>
      );
    case 'progress':
      return (
        <>
          <span className="prop-label">{t('Value')}</span>
          {text('value')}
          <Row label={t('Max')}>
            <TextInput value={layer.max} onChange={(v) => set('max', v, true)} />
          </Row>
          <Row label={t('Animation')}>
            <Num value={layer.duration} onChange={(v) => set('duration', v, true)} min={0} step={0.1} precision={1} suffix=" s" />
          </Row>
          <Row label={t('Track')}>
            <HexInput value={layer.track} onChange={(v) => set('track', v, true)} />
          </Row>
        </>
      );
    case 'rotator':
      return (
        <>
          <span className="prop-label">{t('Lines (shown one after another)')}</span>
          {text('text', true)}
          <Row label={t('Each')}>
            <Num value={layer.interval} onChange={(v) => set('interval', v, true)} min={0.5} step={0.1} precision={1} suffix=" s" />
          </Row>
          <Row label={t('Fade')}>
            <Num value={layer.fade} onChange={(v) => set('fade', v, true)} min={0} step={0.1} precision={1} suffix=" s" />
          </Row>
        </>
      );
    case 'typewriter':
      return (
        <>
          {text('text', true)}
          <Row label={t('Speed')}>
            <Num value={layer.speed} onChange={(v) => set('speed', v, true)} min={1} suffix={` ${t('char/s')}`} />
          </Row>
          {check('cursor', t('Blinking cursor'))}
        </>
      );
    case 'slideshow':
      return (
        <>
          <span className="prop-label">{t('Images (one asset per line)')}</span>
          {text('images', true, '/assets/photo1.jpg')}
          <AssetPicker value="" onChange={(v) => v && set('images', `${layer.images.trim() ? layer.images.trim() + '\n' : ''}${v}`)} />
          <Row label={t('Each')}>
            <Num value={layer.interval} onChange={(v) => set('interval', v, true)} min={0.5} step={0.1} precision={1} suffix=" s" />
          </Row>
          <Row label={t('Fade')}>
            <Num value={layer.fade} onChange={(v) => set('fade', v, true)} min={0} step={0.1} precision={1} suffix=" s" />
          </Row>
          <Row label={t('Fit')}>
            <Segmented
              value={layer.fit}
              onChange={(v) => set('fit', v)}
              options={[
                { value: 'contain', label: t('Contain') },
                { value: 'cover', label: t('Cover') },
                { value: 'stretch', label: t('Stretch') },
              ]}
            />
          </Row>
        </>
      );
    case 'qrCode':
      return (
        <>
          <span className="prop-label">{t('Content (URL, text…)')}</span>
          {text('data')}
          <Row label={t('Background')}>
            <HexInput value={layer.background} onChange={(v) => set('background', v, true)} />
          </Row>
        </>
      );
    case 'barChart':
      return (
        <>
          <span className="prop-label">{t('Rows — “Label: value”, one per line')}</span>
          {text('text', true)}
          {check('percent', t('Show as % of total'))}
          <Row label={t('Animation')}>
            <Num value={layer.duration} onChange={(v) => set('duration', v, true)} min={0} step={0.1} precision={1} suffix=" s" />
          </Row>
          <Row label={t('Track')}>
            <HexInput value={layer.track} onChange={(v) => set('track', v, true)} />
          </Row>
          <Row label={t('Labels')}>
            <HexInput value={layer.textColor} onChange={(v) => set('textColor', v, true)} />
          </Row>
        </>
      );
    default:
      return null;
  }
}

/** A new duration: markers past the end come back to it (they would be out of reach). */
function setDuration(t: Template, v: number) {
  t.duration = Math.max(1, v);
  for (const m of t.markers) if (m.frame > t.duration) m.frame = t.duration;
}

/** Font picker: the fonts the server really has (bundled, uploaded, system). */
export function FontControls({ layer, set }: { layer: Layer & FontStyle; set: (key: string, v: unknown) => void }) {
  const families = useFonts((s) => s.families);
  // The list is loaded at start, which fails before logging in.
  useEffect(() => {
    if (!families.length) void loadFonts();
  }, [families.length]);

  return (
    <>
      <KitRow label={t('Font')} expose={['fontFamily', 'fontWeight']} exposeValue={{ fontFamily: layer.fontFamily, fontWeight: layer.fontWeight }}>
        <FontPick family={layer.fontFamily} weight={layer.fontWeight} onChange={(f, w) => (set('fontFamily', f), w !== layer.fontWeight && set('fontWeight', w))} />
      </KitRow>
      <KitRow label={t('Align')} expose={['align', 'verticalAlign', 'uppercase']} exposeValue={{ align: layer.align, verticalAlign: layer.verticalAlign, uppercase: !!layer.uppercase }}>
        <Segmented
          value={layer.align}
          onChange={(v) => set('align', v)}
          options={[
            { value: 'left', label: <AlignLeft size={13} /> },
            { value: 'center', label: <AlignCenter size={13} /> },
            { value: 'right', label: <AlignRight size={13} /> },
          ]}
        />
        <Segmented
          value={layer.verticalAlign}
          onChange={(v) => set('verticalAlign', v)}
          options={[
            { value: 'top', label: t('Top') },
            { value: 'middle', label: t('Mid') },
            { value: 'bottom', label: t('Bot') },
          ]}
        />
        {layer.type !== 'text' && (
          <button className={`btn small ${layer.uppercase ? 'on' : ''}`} onClick={() => set('uppercase', !layer.uppercase)} title={t('Upper case')}>
            AA
          </button>
        )}
      </KitRow>
      {layer.type === 'text' && (
        <KitRow label={t('Case')} expose={['transform', 'decoration']} exposeValue={{ transform: layer.transform ?? 'none', decoration: layer.decoration ?? 'none' }}>
          <Segmented
            value={layer.uppercase ? 'upper' : (layer.transform ?? 'none')}
            onChange={(v) => {
              set('uppercase', false);
              set('transform', v === 'none' ? undefined : v);
            }}
            options={[
              { value: 'none', label: <span title={t('As typed')}>—</span> },
              { value: 'upper', label: <span title={t('UPPER CASE')}>AA</span> },
              { value: 'lower', label: <span title={t('lower case')}>aa</span> },
              { value: 'capitalize', label: <span title={t('Each Word')}>Aa</span> },
            ]}
          />
          <Segmented
            value={layer.decoration ?? 'none'}
            onChange={(v) => set('decoration', v === 'none' ? undefined : v)}
            options={[
              { value: 'none', label: <span title={t('No decoration')}>—</span> },
              {
                value: 'underline',
                label: (
                  <span title={t('Underline')} style={{ textDecoration: 'underline' }}>
                    U
                  </span>
                ),
              },
              {
                value: 'lineThrough',
                label: (
                  <span title={t('Strike through')} style={{ textDecoration: 'line-through' }}>
                    S
                  </span>
                ),
              },
            ]}
          />
        </KitRow>
      )}
    </>
  );
}

const FILL_LABEL: Partial<Record<LayerType, string>> = {
  progress: 'Bar',
  barChart: 'Bars',
  qrCode: 'Modules',
};

/** In another format than main: what is specific to it, and a reset. */
function FormatNote({ layer }: { layer: Layer }) {
  const { format, updateBase } = useDesigner(useShallow((s) => ({ format: s.format, updateBase: s.updateBase })));
  const project = useApp((s) => s.project);
  if (format === MAIN_FORMAT || !project) return null;
  const name = project.formats.find((f) => f.id === format)?.name ?? format;
  const o = layer.overrides?.[format];
  const n = Object.keys(o?.props ?? {}).length + Object.keys(o?.tracks ?? {}).length + (o?.visible !== undefined ? 1 : 0);
  return (
    <div className="format-note">
      <span>{rich(t('Editing the {format} layout: changes apply to {name} only. Texts, data and new layers are shared.', { name }), { format: <strong>{name}</strong> })}</span>
      {n > 0 && (
        <button className="btn small" onClick={() => updateBase((tpl) => resetOverride(findLayer(tpl, layer.id)!, format))}>
          <RotateCcw size={12} /> {tn(n, 'Reset {n} change', 'Reset {n} changes')}
        </button>
      )}
    </div>
  );
}

/** A translated sentence with elements in place of its {placeholders}. */
function rich(text: string, parts: Record<string, React.ReactNode>): React.ReactNode[] {
  return text.split(/\{(\w+)\}/).map((s, i) => (i % 2 ? <Fragment key={i}>{parts[s] ?? `{${s}}`}</Fragment> : s));
}

/** What a layer shows: its text, picture, widget settings (and ▶ ❚❚ ⟲ to try a widget). */
/** A content value that comes from data (wired in the Data canvas): greyed, read only. */
function WiredValue({ value, multiline }: { value: string; multiline?: boolean }) {
  return (
    <div className="wired-value" title={t('Comes from data: change it in the Data canvas')}>
      {multiline ? <textarea className="wk-data" rows={3} readOnly value={value} /> : <input className="wk-data" readOnly value={value} />}
      <span className="wk-from in" title={t('From data: set in the Data canvas')}>
        <Database size={12} />
      </span>
    </div>
  );
}

function LayerGeneral({ layer }: { layer: Layer }) {
  const { update, template, previewData } = useDesigner(useShallow((s) => ({ update: s.update, template: s.template, previewData: s.previewData })));
  const data = useBoundData(template, previewData);
  if (!template) return null;
  const edit = (fn: (l: Layer) => void, key?: string) => update((t) => fn(findLayer(t, layer.id)!), key);
  const set = (key: string, v: unknown, coalesce?: boolean) => edit((l) => void ((l as unknown as Attrs)[key] = v), coalesce ? `${key}-${layer.id}` : undefined);
  return (
    <div className="pp-general">
      {layer.type === 'text' && (
        <>
          {wiredKeys(layer.text).length ? <WiredValue value={interpolateText(layer.text, data)} multiline /> : <TextInput multiline value={layer.text} onChange={(v) => set('text', v, true)} />}
          <div className="row">
            <span className="prop-label">{t('Lines')}</span>
            <Segmented
              value={layer.wrap ? 'paragraph' : 'single'}
              onChange={(v) => edit((l) => void (l.type === 'text' && (l.wrap = v === 'paragraph' || undefined)))}
              options={[
                {
                  value: 'single',
                  label: <span title={t('Line breaks only where you type them')}>{t('As typed')}</span>,
                },
                {
                  value: 'paragraph',
                  label: <span title={t('Lines wrap at the width of the box')}>{t('Paragraph')}</span>,
                },
              ]}
            />
          </div>
          <div className="row">
            <span className="prop-label">{t('Scroll')}</span>
            <Segmented
              value={layer.scroll ?? 'none'}
              onChange={(v) => edit((l) => void (l.type === 'text' && (l.scroll = v === 'none' ? undefined : v)))}
              options={[
                {
                  value: 'none',
                  label: <span title={t('The text stays in place')}>{t('Fixed')}</span>,
                },
                {
                  value: 'horizontal',
                  label: <span title={t('The lines one after the other, right to left (ticker)')}>{t('Horizontal')}</span>,
                },
                {
                  value: 'vertical',
                  label: <span title={t('Bottom to top (credit roll)')}>{t('Vertical')}</span>,
                },
              ]}
            />
          </div>
          {layer.scroll && layer.scroll !== 'none' && (
            <>
              <div className="row">
                <span className="prop-label">{t('Speed')}</span>
                <NumberField value={layer.speed ?? 120} min={1} max={2000} suffix="px/s" onChange={(v) => edit((l) => void (l.type === 'text' && (l.speed = v)), `speed-${layer.id}`)} />
              </div>
              {layer.scroll === 'horizontal' ? (
                <div className="row">
                  <span className="prop-label">{t('Separator')}</span>
                  <TextInput value={layer.separator ?? '   •   '} onChange={(v) => set('separator', v, true)} />
                </div>
              ) : (
                <label className="check">
                  <input type="checkbox" checked={!!layer.looping} onChange={(e) => edit((l) => void (l.type === 'text' && (l.looping = e.target.checked || undefined)))} />
                  {t('Loop (comes round again)')}
                </label>
              )}
            </>
          )}
          {(!layer.scroll || layer.scroll === 'none') && <TextFitControls layer={layer} edit={edit} />}
        </>
      )}
      {TEXT_LIKE.has(layer.type) && (
        <div className="pp-typo">
          <FontControls layer={layer as Layer & FontStyle} set={set} />
          <GroupRow layer={layer} spec={{ label: 'Size', props: ['fontSize'], min: 1, suffix: 'px' }} />
          <GroupRow
            layer={layer}
            spec={{
              label: 'Spacing',
              props: ['letterSpacing'],
              precision: 1,
              suffix: 'px',
            }}
          />
          {layer.type === 'text' && (
            <div className="row">
              <span className="prop-label">{t('Line height')}</span>
              <NumberField value={layer.lineHeight ?? 1.2} min={0.5} max={4} step={0.05} precision={2} suffix="×" onChange={(v) => edit((l) => void (l.type === 'text' && (l.lineHeight = Math.abs(v - 1.2) < 1e-6 ? undefined : v)), `lh-${layer.id}`)} />
            </div>
          )}
        </div>
      )}
      {layer.type === 'image' && (wiredKeys(layer.src).length ? <WiredValue value={interpolateText(layer.src, data)} /> : <AssetPicker value={layer.src} onChange={(v) => edit((l) => void (l.type === 'image' && (l.src = v)))} />)}
      <WidgetSettings layer={layer} set={set} data={data} />
      {isTimed(layer) && !widgetOf(layer)?.Operator && (
        <>
          <div className="row" title={t('How it starts on air: at the In, on Continue (F2), or on a trigger (also the API action widget)')}>
            <span className="prop-label">{tc('cc', 'Start')}</span>
            <Segmented
              value={startMode(layer)}
              onChange={(v) =>
                edit((l) => {
                  l.manualStart = v !== 'in' ? true : undefined;
                  l.startOn = v === 'trigger' ? 'trigger' : undefined;
                })
              }
              options={START_MODES().map(([value, label]) => ({ value, label }))}
            />
          </div>
          {startMode(layer) === 'trigger' && (
            <div className="row">
              <span className="prop-label">{tc('cc', 'Trigger name')}</span>
              <TextInput value={layer.startTrigger ?? ''} commitOnBlur onChange={(v) => edit((l) => void (l.startTrigger = v.trim() || undefined))} />
            </div>
          )}
        </>
      )}
      {layer.type === 'group' && layer.lottie !== undefined && <LottieSource layer={layer} />}
      {layer.type !== 'text' && layer.type !== 'image' && layer.type !== 'group' && !isTimed(layer) && <p className="hint">{t(LABELS[layer.type])}</p>}
    </div>
  );
}

/** The file of a Lottie layer: chosen in the assets (or uploaded there), converted into its layers. */
function LottieSource({ layer }: { layer: Layer }) {
  const [lib, setLib] = useState(false);
  const [busy, setBusy] = useState(false);
  const toast = useApp((s) => s.toast);
  const file = layer.type === 'group' ? (layer.lottie ?? '') : '';
  const choose = async (src: string) => {
    setLib(false);
    setBusy(true);
    try {
      await loadLottieAsset(layer.id, src);
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <div className="row">
        <span className="prop-label">{t('File')}</span>
        <button type="button" className="pick video-pick" disabled={busy} onClick={() => setLib(true)} title={file || undefined}>
          <Sparkles size={14} />
          <span className={`pick-nm ${file ? '' : 'ph'}`}>{busy ? t('Converting…') : file ? decodeURIComponent(file.split('/').pop() || file) : t('Choose a Lottie file…')}</span>
        </button>
      </div>
      {lib && <AssetLibrary kind="lottie" title={t('Choose a Lottie animation')} value={file} onClose={() => setLib(false)} onChoose={(src) => void choose(src)} />}
    </>
  );
}

/** Settings of a spec widget (web/src/widgets) and its warnings. */
function SpecInspector({ layer }: { layer: Layer }) {
  const { update, template, previewData } = useDesigner(useShallow((s) => ({ update: s.update, template: s.template, previewData: s.previewData })));
  const data = useBoundData(template, previewData);
  const state = useDesigner((s) => s.widgetState[layer.id]);
  const actWidget = useDesigner((s) => s.actWidget);
  if (!template) return null;
  const def = widgetOf(layer)!;
  const edit = (fn: (l: Layer) => void, key?: string) => update((t) => fn(findLayer(t, layer.id)!), key);
  const set = (path: string, v: unknown, coalesce?: boolean) => edit((l) => setPath(l, path, v), coalesce ? `${path}-${layer.id}` : undefined);
  const act = (op: string, args?: unknown) => void actWidget(layer.id, op, args);
  const Insp = def.Inspector!;
  // The first section drawn (the widget's content) opens; the others start closed.
  const firstRef = useRef<{ type: string; id: string | null }>({ type: layer.type, id: null });
  if (firstRef.current.type !== layer.type) firstRef.current = { type: layer.type, id: null };
  const claimFirst = (id: string) => {
    if (firstRef.current.id === null) firstRef.current.id = id;
    return firstRef.current.id === id;
  };
  const r = resolveLayer(layer, useDesigner.getState().frame, data, holdFrame(template));
  const warnings = def.quiet ? [] : (def.warnings?.(layer as never, { data, state, r }) ?? []).filter((w) => w.op !== 'only');
  return (
    <Bound obj={layer} set={set} data={data}>
      <FirstSection.Provider value={claimFirst}>
        <Warnings list={warnings} />
        <Insp layer={layer as never} template={template} data={data} edit={edit as never} set={set} state={state} act={act} />
      </FirstSection.Provider>
    </Bound>
  );
}

/** The control centre of a widget (web/src/widgets/centre.tsx), to try in the Designer: under its name, always shown. */
function OperatorPreview({ layer }: { layer: Layer }) {
  const { template, previewData, update } = useDesigner(useShallow((s) => ({ template: s.template, previewData: s.previewData, update: s.update })));
  const data = useBoundData(template, previewData);
  const state = useDesigner((s) => s.widgetState[layer.id]);
  const actWidget = useDesigner((s) => s.actWidget);
  // Its settings (line 3) edit the layer, as the inspector's rows do.
  const set = (path: string, v: unknown, coalesce?: boolean) => update((t) => setPath(findLayer(t, layer.id)!, path, v), coalesce ? `${path}-${layer.id}` : undefined);
  if (!template) return null;
  const def = widgetOf(layer);
  const Op = def?.Operator && (def.controls ? def.controls(layer as never, data) : true) ? def.Operator : undefined;
  // A Text fed by data sets: the row the Designer shows, one to the next, as the Operator does.
  const sets = usedDatasets(template).filter((ds) => datasetUse(template, ds).texts.some((l) => l.id === layer.id));
  const timed = !def?.Inspector && isTimed(layer);
  if (!Op && !sets.length && !timed) return null;
  const act = (op: string, args?: unknown) => void actWidget(layer.id, op, args);
  // Until its first Play here, a widget that moves by itself waits, as in the Operator's preview.
  const held = state === undefined && !!(def?.plays ? def.plays(layer as never, data) : isTimed(layer));
  const shown = held ? { ...layer, manualStart: true } : layer;
  return (
    <div className="pp-try">
      {timed && <WidgetRunButtons layer={layer} />}
      {sets.map((ds) => (
        <RowStep key={ds.key} ds={ds} />
      ))}
      {Op && (
        <CentreModeContext.Provider value="designer">
          <Bound obj={layer} set={set} data={data}>
            <Op layer={shown as never} state={state} data={data} act={act} time={{ nowMs: Date.now(), ageSecs: 0, defaultTz: '', held }} />
          </Bound>
        </CentreModeContext.Provider>
      )}
    </div>
  );
}

function RowStep({ ds }: { ds: DataSet }) {
  const previewData = useDesigner((s) => s.previewData);
  const stage = useDatasetRows(ds, previewData);
  const total = stage?.rows.length ?? 0;
  const row = Math.min(Math.max(1, parseInt(previewData[rowKey(ds.key)] ?? '1', 10) || 1), Math.max(1, total));
  const go = (n: number) => useDesigner.setState((st) => ({ previewData: { ...st.previewData, [rowKey(ds.key)]: String(n) } }));
  return (
    <KitRow label={ds.name}>
      <div className="wk-rowstep">
        <button className="icon-btn" title={t('Previous row')} disabled={row <= 1} onClick={() => go(row - 1)}>
          <ChevronLeft size={14} />
        </button>
        <span className="wk-rowstep-n">{total ? `${row} / ${total}` : '—'}</span>
        <button className="icon-btn" title={t('Next row')} disabled={row >= total} onClick={() => go(row + 1)}>
          <ChevronRight size={14} />
        </button>
      </div>
    </KitRow>
  );
}

function LayerInspector({ layer }: { layer: Layer }) {
  const { update } = useDesigner(useShallow((s) => ({ update: s.update })));
  const edit = (fn: (l: Layer) => void, key?: string) => update((t) => fn(findLayer(t, layer.id)!), key);
  const textLike = TEXT_LIKE.has(layer.type);
  const radius = layer.type === 'rect' || layer.type === 'group' || layer.type === 'progress';
  const Icon = layerIcon(layer);
  const hasLook = layer.type !== 'image' && layer.type !== 'slideshow' && !widgetOf(layer)?.ownsLook;
  const driven = useDrivenValues(layer);
  const spec = widgetOf(layer);
  return (
    <DrivenContext.Provider value={driven}>
      <FormatNote layer={layer} />
      <div className="pp-head">
        <span className="pp-type">
          <Icon size={14} />
        </span>
        <TextInput value={layer.name} commitOnBlur onChange={(v) => edit((l) => void (l.name = v))} />
      </div>
      <OperatorPreview layer={layer} />
      {spec?.Inspector ? (
        <Boundary resetKey={layer} fallback={<div className="hint wk-note">{t("This widget's settings cannot be shown.")}</div>}>
          <SpecInspector layer={layer} />
        </Boundary>
      ) : (
        <Group id="general" title={t('General')}>
          <LayerGeneral layer={layer} />
        </Group>
      )}
      {layer.type !== 'text' && <EffectsGroup layer={layer} />}
      <Group id="properties" title={t('Properties')}>
        <Group id="transform" title={tc('section', 'Transform')} level={2}>
          {TRANSFORM_ROWS.map((r) => (
            <GroupRow key={r.label} layer={layer} spec={r} />
          ))}
        </Group>
        {(hasLook || layer.type === 'group' || (layer.type === 'image' && !spec?.Inspector)) && (
          <Group id="contents" title={t('Contents')} level={2}>
            {hasLook && <ColorGroupRow layer={layer} prop="fill" label={FILL_LABEL[layer.type] ?? (textLike ? 'Color' : 'Fill')} />}
            {hasLook && layer.type !== 'progress' && layer.type !== 'qrCode' && <ColorGroupRow layer={layer} prop="stroke" label="Stroke" />}
            {hasLook &&
              STYLE.filter((st) => (st.prop === 'cornerRadius' ? radius : !['progress', 'qrCode'].includes(layer.type))).map((st) => (
                <GroupRow
                  key={st.prop}
                  layer={layer}
                  spec={{
                    label: st.label === 'Stroke W' ? 'Stroke width' : st.label,
                    props: [st.prop],
                    min: st.min,
                    precision: st.precision,
                    suffix: 'px',
                  }}
                />
              ))}
            {layer.type === 'image' && !spec?.Inspector && (
              <div className="row">
                <span className="prop-label">{t('Fit')}</span>
                <Segmented
                  value={layer.fit}
                  onChange={(v) => edit((l) => void (l.type === 'image' && (l.fit = v)))}
                  options={[
                    { value: 'contain', label: t('Contain') },
                    { value: 'cover', label: t('Cover') },
                    { value: 'stretch', label: t('Stretch') },
                  ]}
                />
              </div>
            )}
            {layer.type === 'group' && (
              <label className="check">
                <input type="checkbox" checked={!!layer.clip} onChange={(e) => edit((l) => void (l.type === 'group' && (l.clip = e.target.checked)))} />
                {t('Clip children to group bounds')}
              </label>
            )}
          </Group>
        )}
        <Group id="reveal" title={t('Reveal mask')} level={2}>
          {[
            { label: 'Left', props: ['cropLeft'] as Prop[] },
            { label: 'Right', props: ['cropRight'] as Prop[] },
            { label: 'Top', props: ['cropTop'] as Prop[] },
            { label: 'Bottom', props: ['cropBottom'] as Prop[] },
          ].map((r) => (
            <GroupRow key={r.label} layer={layer} spec={{ ...r, display: 100, suffix: '%', min: 0, max: 1 }} />
          ))}
        </Group>
      </Group>
    </DrivenContext.Provider>
  );
}

/**
 * Effects › Shadow of a layer (every widget but the Text, which has its own): on / off, then its
 * colour, offset and blur, animatable like the other properties. Drawn by both renderers.
 */
function EffectsGroup({ layer }: { layer: Layer }) {
  const { update } = useDesigner(useShallow((s) => ({ update: s.update })));
  const set = (path: string, v: unknown, coalesce?: boolean) => update((t) => setPath(findLayer(t, layer.id)!, path, v), coalesce ? `${path}-${layer.id}` : undefined);
  return (
    <Bound obj={layer} set={set}>
      <SpecSection id="layer-effects" title={t('Effects')} closed>
        <Toggle label={t('Shadow')} expose="dropShadow" exposeValue={!!layer.dropShadow} value={!!layer.dropShadow} onChange={(v) => set('dropShadow', v || undefined)} />
        {layer.dropShadow && (
          <>
            <ColorGroupRow layer={layer} prop="shadowColor" label="Color" />
            <GroupRow layer={layer} spec={{ label: 'Offset X', props: ['shadowX'], suffix: 'px', min: -200, max: 200 }} />
            <GroupRow layer={layer} spec={{ label: 'Offset Y', props: ['shadowY'], suffix: 'px', min: -200, max: 200 }} />
            <GroupRow layer={layer} spec={{ label: 'Blur', props: ['shadowBlur'], suffix: 'px', min: 0, max: 200 }} />
          </>
        )}
      </SpecSection>
    </Bound>
  );
}

const EASINGS: { value: Easing['type']; label: string }[] = [
  { value: 'linear', label: 'Linear' },
  { value: 'easeIn', label: 'Ease in' },
  { value: 'easeOut', label: 'Ease out' },
  { value: 'easeInOut', label: 'Easy ease' },
  { value: 'back', label: 'Back (overshoot)' },
  { value: 'elastic', label: 'Elastic' },
  { value: 'bounce', label: 'Bounce' },
  { value: 'bezier', label: 'Custom bezier' },
  { value: 'hold', label: 'Hold' },
];

export function EasingCurve({ easing, size = 120 }: { easing: Easing; size?: number }) {
  const pad = 14;
  const s = size - pad * 2;
  const pts = Array.from({ length: 61 }, (_, i) => {
    const t = i / 60;
    const y = easing.type === 'hold' ? (t < 1 ? 0 : 1) : applyEasing(easing, t);
    return `${pad + t * s},${pad + (1 - y) * s * 0.75 + s * 0.125}`;
  }).join(' ');
  return (
    <svg className="easing-curve" width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      <rect x={pad} y={pad} width={s} height={s} rx={4} />
      <polyline points={pts} />
    </svg>
  );
}

/** The easing of the stretch between two keyframes (clicked in the timeline): nothing else. */
function SegmentInspector() {
  const { template, selectedSegment: seg, update } = useDesigner(useShallow((s) => ({ template: s.template, selectedSegment: s.selectedSegment, update: s.update })));
  if (!template || !seg) return null;
  const layer = findLayer(template, seg.layerId);
  const k = seg.props.map((p) => layer?.tracks[p]?.find((x) => x.frame === seg.frame)).find(Boolean);
  if (!layer || !k) return null;
  const next = Math.min(...seg.props.flatMap((p) => (layer.tracks[p] ?? []).filter((x) => x.frame > seg.frame).map((x) => x.frame)));
  const setEasing = (e: Easing) =>
    update((t) => {
      for (const p of seg.props) {
        const kk = findLayer(t, seg.layerId)?.tracks[p]?.find((x) => x.frame === seg.frame);
        if (kk) kk.easing = e;
      }
    });
  const bez = k.easing.type === 'bezier' ? k.easing : { x1: 0.42, y1: 0, x2: 0.58, y2: 1 };
  return (
    <Section title={t('Easing · {layer} · {props}', { layer: layer.name, props: seg.props.map((p) => t(PROP_LABELS[p])).join(', ') })}>
      <p className="hint">{t('From frame {from} to {to}', { from: seg.frame, to: Number.isFinite(next) ? next : '…' })}</p>
      <div className="easing-picks">
        {EASINGS.map((e) => (
          <button key={e.value} className={k.easing.type === e.value ? 'on' : ''} title={t(e.label)} onClick={() => setEasing(e.value === 'bezier' ? { type: 'bezier', ...bez } : ({ type: e.value } as Easing))}>
            <EasingCurve easing={e.value === 'bezier' ? { type: 'bezier', ...bez } : ({ type: e.value } as Easing)} size={44} />
            <span>{t(e.label)}</span>
          </button>
        ))}
      </div>
      <div className="easing-box">
        <EasingCurve easing={k.easing} />
        {k.easing.type === 'bezier' && (
          <div className="bez-grid">
            {(['x1', 'y1', 'x2', 'y2'] as const).map((c) => (
              <NumberField key={c} label={c} value={bez[c]} step={0.01} precision={2} display={1} onChange={(v) => setEasing({ type: 'bezier', ...bez, [c]: c.startsWith('x') ? Math.min(1, Math.max(0, v)) : v })} />
            ))}
          </div>
        )}
      </div>
    </Section>
  );
}

/** "5" or "5s" seconds, "2:10" seconds and frames, "1:02:10" minutes too → frames. */
function parseDuration(text: string, rate: number): number | null {
  const t = text.trim().replace(/s$/i, '');
  if (/^\d+([.,]\d+)?$/.test(t)) return Math.round(parseFloat(t.replace(',', '.')) * rate);
  const parts = t.split(':').map((x) => parseInt(x, 10));
  if (parts.length < 2 || parts.length > 3 || parts.some((x) => Number.isNaN(x) || x < 0)) return null;
  const [m, sec, fr] = parts.length === 3 ? parts : [0, ...parts];
  return Math.round((m * 60 + sec) * rate) + fr;
}
const pad2 = (n: number) => String(n).padStart(2, '0');
/** Frames as mm:ss:ff. */
const timecode = (frames: number, rate: number) => {
  const r = Math.round(rate);
  const sec = Math.floor(frames / r);
  return `${pad2(Math.floor(sec / 60))}:${pad2(sec % 60)}:${pad2(frames % r)}`;
};

/** A template's length: a timecode to type in, steps of one frame or one second, and usual lengths. */
function DurationField({ frames, rate, onChange }: { frames: number; rate: number; onChange: (frames: number) => void }) {
  const [text, setText] = useState<string | null>(null);
  const r = Math.round(rate);
  const commit = () => {
    if (text !== null) {
      const v = parseDuration(text, rate);
      if (v !== null) onChange(Math.max(1, v));
    }
    setText(null);
  };
  const step = (d: number) => onChange(Math.max(1, frames + d));
  return (
    <div className="dur-field">
      <div className="dur-main">
        <button className="dur-step" title={t('− 1 second (Shift: − 1 frame)')} onClick={(e) => step(e.shiftKey ? -1 : -r)}>
          <ChevronLeft size={14} />
        </button>
        <input
          className="dur-tc"
          value={text ?? timecode(frames, rate)}
          title={t('Minutes:seconds:frames — or type 5, 5s, 2:10')}
          onFocus={(e) => (setText(timecode(frames, rate)), e.currentTarget.select())}
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur();
            if (e.key === 'Escape') (setText(null), e.currentTarget.blur());
            if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
              e.preventDefault();
              step((e.key === 'ArrowUp' ? 1 : -1) * (e.shiftKey ? 1 : r));
              setText(null);
            }
            e.stopPropagation();
          }}
        />
        <button className="dur-step" title={t('+ 1 second (Shift: + 1 frame)')} onClick={(e) => step(e.shiftKey ? 1 : r)}>
          <ChevronRight size={14} />
        </button>
      </div>
      <div className="dur-units">
        <span>{t('min')}</span>
        <span>{t('sec')}</span>
        <span>{t('frames')}</span>
      </div>
      <div className="dur-presets">
        {[3, 5, 8, 10, 15, 30].map((sec) => (
          <button key={sec} className={frames === sec * r ? 'on' : ''} onClick={() => onChange(sec * r)}>
            {sec}s
          </button>
        ))}
      </div>
    </div>
  );
}

const OVERFLOWS: { value: TextOverflow; label: string; hint: string }[] = [
  { value: 'fit', label: 'Fit', hint: 'Narrower, then smaller, down to the minimums below; then cut at the box' },
  { value: 'ellipsis', label: '…', hint: 'Shortened with “…”' },
  { value: 'clip', label: 'Cut', hint: 'Cut at the box' },
  { value: 'visible', label: 'Overflow', hint: 'Drawn outside its box' },
];

/** What a text does when it is too long for its box, and how it does now with the preview values. */
function TextFitControls({ layer, edit }: { layer: Layer & { type: 'text' }; edit: (fn: (l: Layer) => void, key?: string) => void }) {
  const { frame, previewData, template } = useDesigner(useShallow((s) => ({ frame: s.frame, previewData: s.previewData, template: s.template })));
  const data = useBoundData(template, previewData);
  const mode = layer.overflow ?? 'fit';
  const fit = fitLayer(layer, resolveLayer(layer, frame, data, template ? holdFrame(template) : 0), layerText(layer, data));
  const pct = (v: number) => `${Math.round(v * 100)} %`;
  const setT = (fn: (l: Layer & { type: 'text' }) => void, key?: string) => edit((l) => void (l.type === 'text' && fn(l)), key);
  return (
    <div className="fit-controls">
      <div className="row">
        <span className="prop-label" title={t('When the text is too long for its box')}>
          {t('Too long')}
        </span>
        <Segmented value={mode} onChange={(v) => setT((l) => (l.overflow = v === 'fit' ? undefined : v))} options={OVERFLOWS.map((o) => ({ value: o.value, label: <span title={t(o.hint)}>{t(o.label)}</span> }))} />
      </div>
      {mode === 'fit' && (
        <>
          <div className="row">
            <span className="prop-label" title={t('Narrowest the text gets, before getting smaller')}>
              {t('Narrower to')}
            </span>
            <input type="range" min={50} max={100} value={Math.round((layer.squeeze ?? 0.8) * 100)} onChange={(e) => setT((l) => (l.squeeze = +e.target.value / 100), `squeeze-${layer.id}`)} />
            <span className="fit-val">{pct(layer.squeeze ?? 0.8)}</span>
          </div>
          <div className="row">
            <span className="prop-label" title={t('Smallest font size, as a part of the size set')}>
              {t('Smaller to')}
            </span>
            <input type="range" min={20} max={100} value={Math.round((layer.minSize ?? 0.6) * 100)} onChange={(e) => setT((l) => (l.minSize = +e.target.value / 100), `minsize-${layer.id}`)} />
            <span className="fit-val">{pct(layer.minSize ?? 0.6)}</span>
          </div>
          <div className="row">
            <span className="prop-label" title={t('The Operator warns when the text gets smaller than this')}>
              {t('Warn under')}
            </span>
            <input type="range" min={20} max={100} value={Math.round((layer.warnBelow ?? 0.8) * 100)} onChange={(e) => setT((l) => (l.warnBelow = +e.target.value / 100), `warn-${layer.id}`)} />
            <span className="fit-val">{pct(layer.warnBelow ?? 0.8)}</span>
          </div>
        </>
      )}
      {(fit.ratio < 0.999 || fit.overflows) && (
        <div className={`fit-state ${fit.overflows ? 'bad' : fit.ratio < (layer.warnBelow ?? 0.8) ? 'warn' : ''}`}>
          {fit.overflows ? (mode === 'fit' ? t('Too long even at the minimums: cut') : mode === 'ellipsis' ? t('Too long: shortened with “…”') : mode === 'clip' ? t('Too long: cut at the box') : t('Too long: goes out of its box')) : t('With the preview values: {pct}', { pct: pct(fit.ratio) })}
        </div>
      )}
    </div>
  );
}

function TemplateInspector({ template }: { template: Template }) {
  const { update } = useDesigner(useShallow((s) => ({ update: s.update })));
  const fps = useApp((s) => s.project?.frameRate);
  const project = useApp((s) => s.project);
  const zones = project?.zones ?? [];
  const rate = fps ? fps.num / fps.den : 50;
  const sorted = [...zones].sort((a, b) => b.layer - a.layer);
  return (
    <>
      <div className="pp-head">
        <span className="pp-type">
          <LayoutTemplate size={14} />
        </span>
        <TextInput value={template.name} commitOnBlur onChange={(v) => update((t) => void (t.name = v))} />
      </div>
      {project && (
        <div className="tpl-publish-row">
          <TemplatePublish project={project} id={template.id} />
        </div>
      )}
      <div className="tpl-props">
        <div className="tpl-field">
          <span className="tpl-label">{t('Duration')}</span>
          <DurationField frames={template.duration} rate={rate} onChange={(v) => (update((t) => setDuration(t, v), 'duration'), useDesigner.getState().setFrame(useDesigner.getState().frame))} />
        </div>
        <div className="tpl-field">
          <span className="tpl-label">
            {t('Zone')}
            <button className="icon-btn ghost tpl-zones-edit" title={t('Edit the zones')} onClick={() => project && useZoneEdit.getState().start(project, template.zone)}>
              <Settings2 size={13} />
            </button>
          </span>
          <div className="tpl-zones" role="radiogroup">
            {sorted.map((z) => (
              <button key={z.id} role="radio" aria-checked={template.zone === z.id} className={template.zone === z.id ? 'on' : ''} style={{ ['--z' as string]: z.color }} onClick={() => update((t) => void (t.zone = z.id))}>
                <span className="zone-dot" />
                <span className="tpl-zone-name">{z.name}</span>
                <span className="tpl-zone-layer">L{z.layer}</span>
                {template.zone === z.id && <Check size={13} />}
              </button>
            ))}
            {!zones.length && <span className="muted small">{t('No zone yet')}</span>}
          </div>
        </div>
      </div>
    </>
  );
}

export function Inspector() {
  const { template, selected, selectedSegment } = useDesigner(useShallow((s) => ({ template: s.template, selected: s.selected, selectedSegment: s.selectedSegment })));
  const editSafe = useGuidesView((s) => s.editSafe);
  const project = useApp((s) => s.project);
  if (editSafe && project)
    return (
      <div className="inspector">
        <AreaInspector project={project} />
      </div>
    );
  if (!template)
    return (
      <div className="inspector">
        <Section title={t('All templates')}>
          <p className="muted small">{tn(project?.templates.length ?? 0, '{n} template', '{n} templates')}</p>
        </Section>
      </div>
    );
  const layer = selected.length === 1 ? findLayer(template, selected[0]) : undefined;
  return (
    <div className="inspector">
      {selectedSegment ? (
        <SegmentInspector />
      ) : layer ? (
        <LayerInspector key={layer.id} layer={layer} />
      ) : selected.length > 1 ? (
        <Section title={t('Selection')}>
          <p className="hint">{t('{n} layers selected. Drag on the stage to move them together.', { n: selected.length })}</p>
        </Section>
      ) : (
        <TemplateInspector template={template} />
      )}
    </div>
  );
}

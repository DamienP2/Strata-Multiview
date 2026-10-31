import { Boundary } from '../ui/Boundary';
import { useShallow } from 'zustand/react/shallow';
import { Dropdown } from '../ui/Dropdown';
import { FontPick } from '../ui/FontPick';
import { AssetPicker } from './AssetPicker';
import { Database, RotateCcw, AlignCenter, AlignLeft, AlignRight, ChevronDown, Link as LinkIcon, LayoutTemplate, Settings2, Unlink } from 'lucide-react';
import { createContext, Fragment, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { col, holdFrame, interpolateText, num, parseHex, propValue, resolveLayer, toHex } from '../engine/evaluate';
import { BACKDROP_COLOR, type Backdrop } from '../engine/backdrop';
import type { FillStyle } from '../widgets/shape';
import { MAIN_FORMAT, TEXT_LIKE, type TextOverflow, type FontStyle, type Layer, type Prop, type Template, type Value } from '../engine/types';
import { useApp } from '../store';
import { ColorField, NumberField, Segmented, TextInput } from '../ui/fields';
import { loadFonts, useFonts } from '../engine/fonts';
import { isOverridden, resetOverride } from '../engine/formats';
import { findLayer, isAnimated, LABELS, setProp, useDesigner } from './designerStore';
import { isTimed } from '../engine/widgets';
import { WidgetRunButtons } from './WidgetRun';
import { AreaInspector, useGuidesView } from './Guides';
import { fitLayer, layerText } from '../engine/Scene';
import { MultiviewSettings } from './MultiviewSettings';
import { layerIcon } from './LayersPanel';
import { widgetOf } from '../widgets';
import { Bound, CentreModeContext, ColorPick, FirstSection, Num as KitNum, Row as KitRow, Section as KitSection, setPath, SliderNum, Warnings, wiredKeys } from '../widgets/kit';
import { t, tn } from '../i18n';
import { useDisplayPref } from '../displayPrefs';

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


/** Values of the selected layer set by a bound variable, at the playhead. */
const DrivenContext = createContext<Map<string, Value>>(new Map());

function useDrivenValues(layer: Layer): Map<string, Value> {
  const { template, frame, previewData } = useDesigner(useShallow((s) => ({ template: s.template, frame: s.frame, previewData: s.previewData })));
  const bound = previewData;
  return useMemo(() => {
    const out = new Map<string, Value>();
    if (!template) return out;
    const driven = new Set<Prop>((Object.entries(layer.bindings) as [Prop, string][]).filter(([, k]) => k).map(([p]) => p));
    if (!driven.size) return out;
    const data = { ...previewData, ...bound };
    const hold = holdFrame(template);
    const resolved = new Map(template.layers.map((l) => [l.id, resolveLayer(l, frame, data, hold)]));
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
      <span className="pp-label">
        {t(spec.label)}
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
    </div>
  );
}

/** Colour row in the same layout. */
export function ColorGroupRow({ layer, prop, label }: { layer: Layer; prop: 'fill' | 'stroke' | 'shadowColor' | 'fill2'; label: string }) {
  const { frame, update } = useDesigner(useShallow((s) => ({ frame: s.frame, update: s.update })));
  const driven = useContext(DrivenContext).get(prop);
  return (
    <div className={`pp-row ${isAnimated(layer, prop) ? 'animated' : ''}`}>
      <span className="pp-label">
        {t(label)}
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
    </div>
  );
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

/** Settings of prefab widgets. */
export function WidgetSettings({ layer, set, data = {} }: { layer: Layer; set: (key: string, v: unknown, coalesce?: boolean) => void; data?: Record<string, string> }) {
  const a = layer as unknown as Attrs;
  // Content wired in the Data canvas: the data's value, greyed and read only.
  const text = (key: string, multiline = false, placeholder?: string) =>
    wiredKeys(String(a[key] ?? '')).length ? <WiredValue value={interpolateText(String(a[key]), data)} multiline={multiline} /> : <TextInput multiline={multiline} value={String(a[key] ?? '')} placeholder={placeholder} onChange={(v) => set(key, v, true)} />;
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
    default:
      return null;
  }
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
  const data = previewData;
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
      {layer.type !== 'text' && layer.type !== 'image' && layer.type !== 'group' && !isTimed(layer) && <p className="hint">{t(LABELS[layer.type])}</p>}
    </div>
  );
}

/** Settings of a spec widget (web/src/widgets) and its warnings. */
function SpecInspector({ layer }: { layer: Layer }) {
  const { update, template, previewData } = useDesigner(useShallow((s) => ({ update: s.update, template: s.template, previewData: s.previewData })));
  const data = previewData;
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
        <LookSections layer={layer} set={set} />
      </FirstSection.Provider>
    </Bound>
  );
}

/** Widgets with a frame (fill, border, corners); those with their own background colour; those with texts of their own. */
const FRAMED = new Set(['source', 'audioMeter', 'loudness', 'timecode', 'status', 'alarmLog', 'clock', 'text', 'image']);
const OWN_BACKGROUND = new Set(['source', 'audioMeter', 'loudness', 'timecode', 'status', 'alarmLog']);
const TYPED = new Set(['audioMeter', 'loudness', 'timecode', 'status', 'alarmLog']);

/** The frame of a widget and the typography of its texts (model::Panel), the same for every widget. */
function LookSections({ layer, set }: { layer: Layer; set: (path: string, v: unknown, coalesce?: boolean) => void }) {
  if (!FRAMED.has(layer.type)) return null;
  const p = layer.panel ?? {};
  const own = OWN_BACKGROUND.has(layer.type);
  const bg = own ? String((layer as unknown as { background?: string }).background ?? '') : (p.fill ?? '');
  return (
    <>
      <KitSection id="look-frame" title={t('Frame')}>
        <ColorPick label={t('Background')} value={bg} none={own ? undefined : t('None')} first="#0b0d12" onChange={(v) => set(own ? 'background' : 'panel.fill', v, true)} />
        <KitNum
          label={t('Border')}
          value={p.border ?? 0}
          min={0}
          max={40}
          unit=" px"
          onChange={(v) => {
            set('panel.border', v, true);
            if (v > 0 && !p.borderColor) set('panel.borderColor', '#ffffff');
          }}
        />
        {(p.border ?? 0) > 0 && <ColorPick label={t('Border colour')} value={p.borderColor ?? '#ffffff'} onChange={(v) => set('panel.borderColor', v, true)} />}
        <KitNum label={t('Corners')} value={p.radius ?? 0} min={0} max={200} unit=" px" onChange={(v) => set('panel.radius', v, true)} />
      </KitSection>
      {TYPED.has(layer.type) && (
        <KitSection id="look-type" title={t('Typography')}>
          <KitRow label={t('Font')}>
            <FontPick family={p.font || 'Inter'} weight={400} onChange={(f) => set('panel.font', f === 'Inter' ? undefined : f)} />
          </KitRow>
          <KitRow label={t('Figures')}>
            <FontPick family={p.figures || 'JetBrains Mono'} weight={400} onChange={(f) => set('panel.figures', f === 'JetBrains Mono' ? undefined : f)} />
          </KitRow>
          <ColorPick label={t('Text colour')} value={p.textColor ?? ''} none={t('White')} first="#ffffff" onChange={(v) => set('panel.textColor', v || undefined, true)} />
          <KitNum label={t('Text size')} value={p.textScale || 100} min={50} max={200} unit=" %" onChange={(v) => set('panel.textScale', v === 100 ? undefined : v, true)} />
        </KitSection>
      )}
    </>
  );
}

/** The control centre of a widget (web/src/widgets/centre.tsx), to try in the Designer: under its name, always shown. */
function OperatorPreview({ layer }: { layer: Layer }) {
  const { template, previewData, update } = useDesigner(useShallow((s) => ({ template: s.template, previewData: s.previewData, update: s.update })));
  const data = previewData;
  const state = useDesigner((s) => s.widgetState[layer.id]);
  const actWidget = useDesigner((s) => s.actWidget);
  // Its settings (line 3) edit the layer, as the inspector's rows do.
  const set = (path: string, v: unknown, coalesce?: boolean) => update((t) => setPath(findLayer(t, layer.id)!, path, v), coalesce ? `${path}-${layer.id}` : undefined);
  if (!template) return null;
  const def = widgetOf(layer);
  const Op = def?.Operator && (def.controls ? def.controls(layer as never, data) : true) ? def.Operator : undefined;
  const timed = !def?.Inspector && isTimed(layer);
  if (!Op && !timed) return null;
  const act = (op: string, args?: unknown) => void actWidget(layer.id, op, args);
  // Until its first Play here, a widget that moves by itself waits, as in the Operator's preview.
  const held = state === undefined && !!(def?.plays ? def.plays(layer as never, data) : isTimed(layer));
  const shown = held ? { ...layer, manualStart: true } : layer;
  return (
    <div className="pp-try">
      {timed && <WidgetRunButtons layer={layer} />}
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

function LayerInspector({ layer }: { layer: Layer }) {
  const { update } = useDesigner(useShallow((s) => ({ update: s.update })));
  const edit = (fn: (l: Layer) => void, key?: string) => update((t) => fn(findLayer(t, layer.id)!), key);
  const Icon = layerIcon(layer);
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
    </DrivenContext.Provider>
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
  const data = previewData;
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
            <span className="prop-label" title={t('A warning shows when the text gets smaller than this')}>
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

/** Nothing selected: the multiview itself and its background. */
function MultiviewInspector({ template }: { template: Template }) {
  const project = useApp((s) => s.project);
  const [settings, setSettings] = useState(false);
  if (!project) return null;
  return (
    <>
      <div className="pp-head">
        <span className="pp-type">
          <LayoutTemplate size={14} />
        </span>
        <span className="mv-insp-name">{project.name}</span>
        <button className="icon-btn ghost" title={t('Multiview settings')} onClick={() => setSettings(true)}>
          <Settings2 size={14} />
        </button>
      </div>
      <BackdropSection template={template} />
      {settings && <MultiviewSettings project={project} readOnly={false} onClose={() => setSettings(false)} />}
    </>
  );
}

type BackdropKind = 'solid' | 'gradient' | 'image';
const IMAGE_FITS: { value: NonNullable<FillStyle['fit']>; label: string }[] = [
  { value: 'cover', label: 'Cover the picture' },
  { value: 'contain', label: 'Whole picture' },
  { value: 'stretch', label: 'Stretch' },
  { value: 'tile', label: 'Tile' },
];

/** The multiview's background: a colour (a very dark grey unless set), a gradient or a picture, under everything. */
function BackdropSection({ template }: { template: Template }) {
  const update = useDesigner((s) => s.update);
  const b = template.background ?? {};
  const f = b.fillStyle ?? {};
  const kind: BackdropKind = f.kind === 'linear' || f.kind === 'radial' ? 'gradient' : f.kind === 'image' ? 'image' : 'solid';
  const color = parseHex(b.color || BACKDROP_COLOR) ?? [0.08, 0.08, 0.086, 1];
  const color2 = parseHex(f.color2 || '#000000') ?? [0, 0, 0, 1];
  const set = (fn: (b: Backdrop) => void, key = 'backdrop') =>
    update((tp) => {
      const next: Backdrop = structuredClone(tp.background ?? {});
      next.fillStyle ??= {};
      fn(next);
      tp.background = next;
    }, key);
  const setKind = (k: BackdropKind) =>
    set((x) => {
      const fs = x.fillStyle!;
      if (k === 'solid') fs.kind = 'solid';
      else if (k === 'image') fs.kind = 'image';
      else {
        fs.kind = fs.kind === 'radial' ? 'radial' : 'linear';
        fs.color2 ??= '#000000';
        fs.angle ??= 180;
      }
    });
  return (
    <Section
      title={t('Background')}
      actions={
        template.background ? (
          <button className="icon-btn ghost" title={t('Back to the default background (very dark grey)')} onClick={() => update((tp) => void delete tp.background)}>
            <RotateCcw size={13} />
          </button>
        ) : undefined
      }
    >
      <Segmented<BackdropKind>
        value={kind}
        onChange={setKind}
        options={[
          { value: 'solid', label: t('Colour') },
          { value: 'gradient', label: t('Gradient') },
          { value: 'image', label: t('Image') },
        ]}
      />
      <Row label={kind === 'gradient' ? t('Colour 1') : kind === 'image' ? t('Under the picture') : t('Colour')}>
        <ColorField value={color} onChange={(c) => set((x) => void (x.color = toHex(c)), 'backdrop-color')} />
      </Row>
      {kind === 'gradient' && (
        <>
          <Row label={t('Colour 2')}>
            <ColorField value={color2} onChange={(c) => set((x) => void (x.fillStyle!.color2 = toHex(c)), 'backdrop-color2')} />
          </Row>
          <Row label={t('Shape')}>
            <Segmented<'linear' | 'radial'>
              value={f.kind === 'radial' ? 'radial' : 'linear'}
              onChange={(v) => set((x) => void (x.fillStyle!.kind = v))}
              options={[
                { value: 'linear', label: t('Linear') },
                { value: 'radial', label: t('Radial') },
              ]}
            />
          </Row>
          {f.kind !== 'radial' && (
            <Row label={t('Angle')}>
              <SliderNum value={f.angle ?? 180} min={0} max={360} unit="°" onChange={(v) => set((x) => void (x.fillStyle!.angle = v), 'backdrop-angle')} />
            </Row>
          )}
        </>
      )}
      {kind === 'image' && (
        <>
          <Row label={t('Picture')}>
            <AssetPicker value={f.image ?? ''} onChange={(v) => set((x) => void (x.fillStyle!.image = v))} />
          </Row>
          <Row label={t('Fit')}>
            <Segmented<NonNullable<FillStyle['fit']>> value={f.fit ?? 'cover'} onChange={(v) => set((x) => void (x.fillStyle!.fit = v))} options={IMAGE_FITS.map((o) => ({ value: o.value, label: t(o.label) }))} />
          </Row>
        </>
      )}
    </Section>
  );
}

export function Inspector() {
  const { template, selected } = useDesigner(useShallow((s) => ({ template: s.template, selected: s.selected })));
  const editSafe = useGuidesView((s) => s.editSafe);
  const project = useApp((s) => s.project);
  if (editSafe && project)
    return (
      <div className="inspector">
        <AreaInspector project={project} />
      </div>
    );
  if (!template) return <div className="inspector" />;
  const layer = selected.length === 1 ? findLayer(template, selected[0]) : undefined;
  return (
    <div className="inspector">
      {layer ? (
        <LayerInspector key={layer.id} layer={layer} />
      ) : selected.length > 1 ? (
        <Section title={t('Selection')}>
          <p className="hint">{t('{n} layers selected. Drag on the stage to move them together.', { n: selected.length })}</p>
        </Section>
      ) : (
        <MultiviewInspector template={template} />
      )}
    </div>
  );
}

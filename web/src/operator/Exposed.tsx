import type { ReactNode } from 'react';
import { AlignCenter, AlignJustify, AlignLeft, AlignRight, AlignVerticalJustifyCenter, AlignVerticalJustifyEnd, AlignVerticalJustifyStart, Bold, Italic, Rows3, Strikethrough, Underline } from 'lucide-react';
import { ColorField, NumberField, Segmented, TextInput } from '../ui/fields';
import { Dropdown } from '../ui/Dropdown';
import { FontPick } from '../ui/FontPick';
import { Row, SliderNum } from '../widgets/kit';
import { widgetOf } from '../widgets';
import { ICONS } from '../designer/LayersPanel';
import { AssetPicker } from '../designer/AssetPicker';
import { parseHex, toHex } from '../engine/evaluate';
import { readPath } from '../engine/settings';
import { HYPHEN_LANGS } from '../engine/textFit';
import type { Field, Layer, Prop } from '../engine/types';
import { t, tc } from '../i18n';

/*
 * The properties the Designer exposed to the Operator (Layer.bindings and
 * Layer.settingBindings), shown with the Designer's own controls and in its
 * order (Typography, Colour, Effects, When the text is too long, Transform),
 * as plain sections: no accordion. The contents (texts, pictures…) come first,
 * in the Operator's Content part.
 */

/** A property path of a layer: `@prop` for an animatable property (Layer.bindings), else a setting path. */
type Path = string;

export interface ExposedLayer {
  layer: Layer;
  /** The field each exposed path writes. */
  at: Map<Path, Field>;
}

/** Settings that are contents (a table's rows…): with the contents, not with the look. */
const CONTENT_SETTINGS = new Set(['grid', 'entries']);

/**
 * Splits the fields: contents (typed values, pictures, a table's rows, the
 * variables of the logic) and, per layer, the properties exposed from it.
 */
export function exposedFields(fields: Field[], layers: Layer[]): { content: Field[]; layers: ExposedLayer[] } {
  const byKey = new Map(fields.map((f) => [f.key, f]));
  const owned = new Set<string>();
  const out: ExposedLayer[] = [];
  for (const l of layers) {
    const at = new Map<Path, Field>();
    for (const [p, k] of Object.entries(l.bindings ?? {})) {
      const f = k ? byKey.get(k) : undefined;
      if (f) at.set(`@${p}`, f);
    }
    for (const [p, k] of Object.entries(l.settingBindings ?? {})) {
      const f = byKey.get(k);
      if (f && !CONTENT_SETTINGS.has(p) && f.kind !== 'table') at.set(p, f);
    }
    if (!at.size) continue;
    for (const f of at.values()) owned.add(f.key);
    out.push({ layer: l, at });
  }
  // In the order the Designer exposed them (the fields' order).
  const first = (e: ExposedLayer) => Math.min(...[...e.at.values()].map((f) => fields.indexOf(f)));
  out.sort((a, b) => first(a) - first(b));
  return { content: fields.filter((f) => !owned.has(f.key)), layers: out };
}

/** How the values are read and written (the item's draft, or the source's value when it feeds the field). */
export interface FieldIO {
  value: (f: Field) => string;
  set: (f: Field, v: string) => void;
  /** Fed by a data source: shown, not edited. */
  fed: (f: Field) => string | null;
  /** Its value differs from the one on air (its label gets a dot). */
  changed?: (f: Field) => boolean;
}

const TRUE = new Set(['true', '1', 'yes', 'on']);

/** A section of the properties: a plain title, no accordion. */
export function OpSection({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="opx-sec">
      <header>{title}</header>
      <div className="opx-sec-body">{children}</div>
    </section>
  );
}

/** Property row in the Designer's layout (label · fields), like its Position / Size rows. */
function PRow({ label, children }: { label: ReactNode; children: ReactNode }) {
  return (
    <div className="pp-row">
      <span className="wk-gut" />
      <span className="pp-label">{label}</span>
      <div className="pp-fields">{children}</div>
      <span className="wk-gut" />
    </div>
  );
}

/** Animatable properties, as the Designer's rows (Transform, Reveal mask). */
interface PropRow {
  label: string;
  props: Prop[];
  fields?: string[];
  display?: number;
  suffix?: string;
  precision?: number;
  min?: number;
  max?: number;
}
const TRANSFORM_ROWS: PropRow[] = [
  { label: 'Position', props: ['x', 'y'], fields: ['X', 'Y'], suffix: 'px' },
  { label: 'Scale', props: ['scaleX', 'scaleY'], fields: ['X', 'Y'], display: 100, suffix: '%' },
  { label: 'Dimensions', props: ['width', 'height'], fields: ['W', 'H'], suffix: 'px', min: 0 },
  { label: 'Anchor', props: ['anchorX', 'anchorY'], fields: ['X', 'Y'], display: 100, suffix: '%' },
  { label: 'Rotation', props: ['rotation'], suffix: '°', precision: 1 },
  { label: 'Opacity', props: ['opacity'], display: 100, suffix: '%', min: 0, max: 1 },
];
const MASK_ROWS: PropRow[] = [
  { label: 'Left', props: ['cropLeft'] },
  { label: 'Right', props: ['cropRight'] },
  { label: 'Top', props: ['cropTop'] },
  { label: 'Bottom', props: ['cropBottom'] },
].map((r) => ({ ...r, props: r.props as Prop[], display: 100, suffix: '%', min: 0, max: 1 }));
/** Translated labels of the rows above (the i18n check reads literal texts). */
const ROW_LABEL: Record<string, () => string> = {
  Position: () => t('Position'),
  Scale: () => t('Scale'),
  Dimensions: () => t('Dimensions'),
  Anchor: () => t('Anchor'),
  Rotation: () => t('Rotation'),
  Opacity: () => t('Opacity'),
  Left: () => t('Left'),
  Right: () => t('Right'),
  Top: () => t('Top'),
  Bottom: () => t('Bottom'),
};

/** The exposed properties of one layer: its header, then the Designer's sections. */
export function LayerProperties({ entry, io, flat }: { entry: ExposedLayer; io: FieldIO; /** Rows only, as the Operator's values: no header, no section titles. */ flat?: boolean }) {
  const { layer, at } = entry;
  const Icon = widgetOf(layer)?.icon ?? ICONS[layer.type];
  const kind = widgetOf(layer)?.label;
  const has = (...paths: Path[]) => paths.some((p) => at.has(p));
  const shown = new Set<Path>();
  const use = (...paths: Path[]) => (paths.forEach((p) => shown.add(p)), has(...paths));

  /** The control of one path, or the value read from its source. */
  const ctl = (p: Path, node: (v: string, set: (v: string) => void) => ReactNode) => {
    const f = at.get(p);
    if (!f) return null;
    const fed = io.fed(f);
    if (fed !== null)
      return (
        <span key={p} className="opx-bound" title={t('Fed by a data source')}>
          {fed}
        </span>
      );
    const el = node(io.value(f), (v) => io.set(f, v));
    // Changed from on air: the row's label gets a dot (styles.css, :has).
    return io.changed?.(f) ? (
      <span key={p} className="opx-chg-mark">
        {el}
      </span>
    ) : (
      el
    );
  };
  /** A path's value: the exposed field's, else the layer's own (shown, not changed). */
  const val = (p: Path): string => {
    const f = at.get(p);
    if (f) return io.fed(f) ?? io.value(f);
    const v = p.startsWith('@') ? undefined : readPath(layer, p);
    return v == null ? '' : String(v);
  };
  const num = (v: string, d = 0) => (Number.isFinite(parseFloat(v)) ? parseFloat(v) : d);

  const numField = (p: Path, o: { display?: number; suffix?: string; precision?: number; min?: number; max?: number; step?: number; label?: string } = {}) =>
    ctl(p, (v, set) => (
      <span key={p} className="pp-field">
        <NumberField label={o.label} value={num(v)} display={o.display} suffix={o.suffix} precision={o.precision ?? (v.includes('.') && !o.display ? Math.min(3, v.split('.')[1].length) : 0)} min={o.min} max={o.max} step={o.step ?? 1} onChange={(x) => set(String(+x.toFixed(4)))} />
      </span>
    ));
  const colorField = (p: Path) =>
    ctl(p, (v, set) => (
      <span key={p} className="pp-field color">
        <ColorField value={parseHex(v) ?? [1, 1, 1, 1]} onChange={(x) => set(toHex(x, x[3] < 1))} />
      </span>
    ));
  const toggle = (p: Path, on?: (v: string) => boolean, flip?: (now: boolean) => string) =>
    ctl(p, (v, set) => {
      const isOn = on ? on(v) : TRUE.has(v.trim().toLowerCase());
      return (
        <button key={p} className={`wk-tog ${isOn ? 'on' : ''}`} onClick={() => set(flip ? flip(isOn) : String(!isOn))}>
          <i />
        </button>
      );
    });
  const seg = (p: Path, options: { value: string; label: ReactNode; title?: string }[]) =>
    ctl(p, (v, set) => <Segmented key={p} value={v} onChange={set} options={options} />);

  const propRow = (r: PropRow) =>
    use(...r.props.map((p) => `@${p}`)) && (
      <PRow key={r.label} label={ROW_LABEL[r.label]?.() ?? r.label}>
        {r.props.map((p, i) => numField(`@${p}`, { label: r.fields?.[i], display: r.display, suffix: r.suffix, precision: r.precision, min: r.min, max: r.max }))}
      </PRow>
    );

  const sections: ReactNode[] = [];
  if (layer.type === 'text') sections.push(...textSections());
  else {
    // Any other layer: its colours and line, then its settings.
    const look = [
      use('@fill') && (
        <PRow key="fill" label={t('Fill')}>
          {colorField('@fill')}
        </PRow>
      ),
      use('@stroke') && (
        <PRow key="stroke" label={t('Stroke')}>
          {colorField('@stroke')}
        </PRow>
      ),
      use('@strokeWidth') && (
        <PRow key="sw" label={t('Stroke width')}>
          {numField('@strokeWidth', { suffix: 'px', min: 0, precision: 1 })}
        </PRow>
      ),
      use('@cornerRadius') && (
        <PRow key="radius" label={t('Radius')}>
          {numField('@cornerRadius', { suffix: 'px', min: 0 })}
        </PRow>
      ),
    ].filter(Boolean);
    if (look.length)
      sections.push(
        <OpSection key="look" title={t('Colour')}>
          {look}
        </OpSection>,
      );
  }
  // The settings this page does not lay out itself: as the Operator always showed them.
  const others = [...at.entries()].filter(([p]) => !shown.has(p) && !/^@(x|y|width|height|scaleX|scaleY|anchorX|anchorY|rotation|opacity|crop\w+)$/.test(p));
  if (others.length)
    sections.push(
      <OpSection key="settings" title={t('Settings')}>
        {others.map(([p, f]) => {
          const at = f.label.indexOf(' · ');
          return (
            <Row key={p} label={at > 0 ? f.label.slice(at + 3) : f.label}>
              {ctl(p, (v, set) => (
                <SettingInput field={f} value={v} onChange={set} range={f.min != null && f.max != null ? [f.min, f.max] : SETTING_RANGES[p.split('.').pop()!]} />
              ))}
            </Row>
          );
        })}
      </OpSection>,
    );
  const transform = TRANSFORM_ROWS.map(propRow).filter(Boolean);
  if (transform.length)
    sections.push(
      <OpSection key="transform" title={tc('section', 'Transform')}>
        {transform}
      </OpSection>,
    );
  const mask = MASK_ROWS.map(propRow).filter(Boolean);
  if (mask.length)
    sections.push(
      <OpSection key="mask" title={t('Reveal mask')}>
        {mask}
      </OpSection>,
    );

  return (
    <div className={`opx-layer ${flat ? 'flat' : ''}`}>
      {!flat && (
        <div className="opx-layer-head">
          <span className="pp-type">
            <Icon size={13} />
          </span>
          <strong>{layer.name}</strong>
          {kind && <span className="muted small">{tc('widget', kind)}</span>}
        </div>
      )}
      {sections}
    </div>
  );

  /** A Text's sections, row by row as in its Designer inspector (web/src/widgets/text.tsx). */
  function textSections(): ReactNode[] {
    const out: ReactNode[] = [];
    const justify = TRUE.has(val('justify'));
    const typo = [
      use('fontFamily', 'fontWeight') && (
        <Row key="font" label={t('Font')}>
          {(() => {
            const fam = at.get('fontFamily');
            const wf = at.get('fontWeight');
            if ((fam && io.fed(fam) !== null) || (wf && io.fed(wf) !== null)) return <span className="opx-bound">{`${val('fontFamily')} ${val('fontWeight')}`.trim()}</span>;
            const family = val('fontFamily') || 'Inter';
            const weight = num(val('fontWeight'), 400);
            return (
              <FontPick
                family={family}
                weight={weight}
                onChange={(f, w) => {
                  if (fam && f !== family) io.set(fam, f);
                  if (wf && w !== weight) io.set(wf, String(w));
                }}
              />
            );
          })()}
        </Row>
      ),
      use('@fontSize') && (
        <PRow key="size" label={t('Size')}>
          {numField('@fontSize', { min: 1, suffix: 'px' })}
        </PRow>
      ),
      // As in the Designer: bold, italic, underline, strike in one row.
      use('italic', 'decoration') && (
        <Row key="deco" label={t('Decoration')}>
          {(() => {
            const free = (p: Path) => {
              const f = at.get(p);
              return f && io.fed(f) === null ? f : undefined;
            };
            const wf = free('fontWeight');
            const it = free('italic');
            const dec = free('decoration');
            const d = val('decoration');
            const items: [string, ReactNode, string, boolean][] = [
              ...(wf ? [['bold', <Bold key="b" size={13} />, t('Bold'), num(val('fontWeight'), 400) >= 600] as [string, ReactNode, string, boolean]] : []),
              ...(it ? [['italic', <Italic key="i" size={13} />, t('Italic'), TRUE.has(val('italic'))] as [string, ReactNode, string, boolean]] : []),
              ...(dec
                ? ([
                    ['underline', <Underline key="u" size={13} />, t('Underline'), d === 'underline'],
                    ['strike', <Strikethrough key="s" size={13} />, t('Strike through'), d === 'lineThrough'],
                  ] as [string, ReactNode, string, boolean][])
                : []),
            ];
            if (!items.length) return null;
            return (
              <div className="segmented">
                {items.map(([k, icon, title, on]) => (
                  <button
                    key={k}
                    type="button"
                    title={title}
                    className={on ? 'on' : ''}
                    onClick={() => {
                      if (k === 'bold' && wf) io.set(wf, on ? '400' : '700');
                      else if (k === 'italic' && it) io.set(it, String(!on));
                      else if (dec) io.set(dec, on ? 'none' : k === 'underline' ? 'underline' : 'lineThrough');
                    }}
                  >
                    {icon}
                  </button>
                ))}
              </div>
            );
          })()}
        </Row>
      ),
      use('lineHeight') && (
        <Row key="lh" label={t('Line height')}>
          {numField('lineHeight', { min: 0.5, max: 4, step: 0.05, precision: 2, suffix: '×' })}
        </Row>
      ),
      use('@letterSpacing') && (
        <PRow key="ls" label={tc('text', 'Spacing')}>
          {numField('@letterSpacing', { precision: 1, suffix: 'px' })}
        </PRow>
      ),
      use('align', 'justify') && (
        <Row key="align" label={t('Align')}>
          {ctl(at.has('align') ? 'align' : 'justify', () => (
            <Segmented
              key="al"
              value={justify ? 'justify' : val('align') || 'left'}
              onChange={(v) => {
                const a = at.get('align');
                const j = at.get('justify');
                if (v === 'justify') return j && io.set(j, 'true');
                if (j && justify) io.set(j, 'false');
                if (a) io.set(a, v);
              }}
              options={[
                ...(has('align')
                  ? [
                      { value: 'left', label: <AlignLeft size={13} />, title: t('Left') },
                      { value: 'center', label: <AlignCenter size={13} />, title: t('Centre') },
                      { value: 'right', label: <AlignRight size={13} />, title: t('Right') },
                    ]
                  : []),
                ...(has('justify') ? [{ value: 'justify', label: <AlignJustify size={13} />, title: t('Justify (not the last line)') }] : []),
              ]}
            />
          ))}
        </Row>
      ),
      use('verticalAlign') && (
        <Row key="va" label={t('Vertical')}>
          {seg('verticalAlign', [
            { value: 'top', label: <AlignVerticalJustifyStart size={13} />, title: t('Top') },
            { value: 'middle', label: <AlignVerticalJustifyCenter size={13} />, title: t('Middle') },
            { value: 'bottom', label: <AlignVerticalJustifyEnd size={13} />, title: t('Bottom') },
          ])}
        </Row>
      ),
      use('transform') && (
        <Row key="case" label={t('Case')}>
          {ctl('transform', (v, set) => (
            <Segmented
              key="tr"
              value={v || 'none'}
              onChange={set}
              options={[
                { value: 'none', label: '—', title: t('As typed') },
                { value: 'upper', label: 'AA', title: t('UPPER CASE') },
                { value: 'lower', label: 'aa', title: t('lower case') },
                { value: 'capitalize', label: 'Aa', title: t('Each Word') },
              ]}
            />
          ))}
        </Row>
      ),
    ].filter(Boolean);
    if (typo.length)
      out.push(
        <OpSection key="typo" title={t('Typography')}>
          {typo}
        </OpSection>,
      );

    const gradient = val('fillType') === 'gradient';
    const colour = [
      use('fillType') && (
        <Row key="ft" label={t('Fill')}>
          {seg('fillType', [
            { value: 'solid', label: t('Solid') },
            { value: 'gradient', label: t('Gradient') },
          ])}
        </Row>
      ),
      use('@fill') && (
        <PRow key="fill" label={gradient ? t('From') : t('Color')}>
          {colorField('@fill')}
        </PRow>
      ),
      use('color2') && (
        <Row key="c2" label={t('To')}>
          {colorField('color2')}
        </Row>
      ),
      use('gradAngle') && (
        <Row key="ga" label={t('Angle')}>
          {numField('gradAngle', { min: 0, max: 360, suffix: '°' })}
        </Row>
      ),
    ].filter(Boolean);
    if (colour.length)
      out.push(
        <OpSection key="colour" title={t('Colour')}>
          {colour}
        </OpSection>,
      );

    const strokeOn = (v: string) => num(v) > 0;
    const effects = [
      use('props.strokeWidth') && (
        <Row key="st" label={t('Stroke')}>
          {toggle('props.strokeWidth', strokeOn, (now) => (now ? '0' : '2'))}
        </Row>
      ),
      use('@stroke') && (
        <PRow key="sc" label={t('Color')}>
          {colorField('@stroke')}
        </PRow>
      ),
      use('@strokeWidth') && (
        <PRow key="sw" label={t('Width')}>
          {numField('@strokeWidth', { min: 0, precision: 1, suffix: 'px' })}
        </PRow>
      ),
      use('strokeOutside') && (
        <Row key="so" label={t('Placement')}>
          {seg('strokeOutside', [
            { value: 'false', label: t('Centred') },
            { value: 'true', label: t('Outside') },
          ])}
        </Row>
      ),
      use('shadow') && (
        <Row key="sh" label={t('Shadow')}>
          {toggle('shadow')}
        </Row>
      ),
      use('shadowColor') && (
        <Row key="shc" label={t('Color')}>
          {colorField('shadowColor')}
        </Row>
      ),
      use('shadowX') && (
        <Row key="shx" label={t('Offset X')}>
          {numField('shadowX', { min: -200, max: 200, suffix: 'px' })}
        </Row>
      ),
      use('shadowY') && (
        <Row key="shy" label={t('Offset Y')}>
          {numField('shadowY', { min: -200, max: 200, suffix: 'px' })}
        </Row>
      ),
      use('shadowBlur') && (
        <Row key="shb" label={t('Blur')}>
          {numField('shadowBlur', { min: 0, max: 200, suffix: 'px' })}
        </Row>
      ),
      use('band') && (
        <Row key="bd" label={t('Band')}>
          {toggle('band')}
        </Row>
      ),
      use('bandColor') && (
        <Row key="bdc" label={t('Color')}>
          {colorField('bandColor')}
        </Row>
      ),
      use('bandPadX') && (
        <Row key="bdp" label={t('Padding')}>
          {numField('bandPadX', { min: 0, max: 200, suffix: 'px' })}
        </Row>
      ),
      use('bandGap') && (
        <Row key="bdg" label={t('Gap')}>
          {numField('bandGap', { min: 0, max: 200, suffix: 'px' })}
        </Row>
      ),
      use('bandRadius') && (
        <Row key="bdr" label={t('Corners')}>
          {numField('bandRadius', { min: 0, max: 200, suffix: 'px' })}
        </Row>
      ),
      use('bandAccent') && (
        <Row key="bda" label={t('Accent')}>
          {toggle('bandAccent')}
        </Row>
      ),
      use('bandAccentColor') && (
        <Row key="bdac" label={t('Accent color')}>
          {colorField('bandAccentColor')}
        </Row>
      ),
    ].filter(Boolean);
    if (effects.length)
      out.push(
        <OpSection key="effects" title={t('Effects')}>
          {effects}
        </OpSection>,
      );

    const fit = [
      use('wrap') && (
        <Row key="wrap" label={t('Lines')}>
          {seg('wrap', [
            { value: 'false', label: t('As typed'), title: t('Line breaks only where you type them') },
            { value: 'true', label: t('Paragraph'), title: t('Lines wrap at the width of the box') },
          ])}
        </Row>
      ),
      use('maxLines') && (
        <Row key="ml" label={t('Max lines')}>
          {numField('maxLines', { min: 0, max: 99 })}
        </Row>
      ),
      use('breakMode') && (
        <Row key="bm" label={t('Line break')}>
          {seg('breakMode', [
            { value: 'word', label: t('Whole word') },
            { value: 'hyphen', label: t('Hyphenation') },
          ])}
        </Row>
      ),
      use('lang') && (
        <Row key="lang" label={t('Language')}>
          {ctl('lang', (v, set) => (
            <Dropdown value={v} onChange={set} options={[{ value: '', label: t('Project language') }, ...HYPHEN_LANGS.map(([value, label]) => ({ value, label: t(label) }))]} />
          ))}
        </Row>
      ),
      use('dir') && (
        <Row key="dir" label={t('Direction')}>
          {seg('dir', [
            { value: 'ltr', label: t('Left to right') },
            { value: 'rtl', label: t('Right to left'), title: t('Arabic, Hebrew…') },
          ])}
        </Row>
      ),
      use('overflow') && (
        <Row key="ov" label={t('Behaviour')}>
          {seg('overflow', [
            { value: 'fit', label: t('Fit'), title: t('Narrower, then smaller, down to the minimums below; then cut (or “…”)') },
            { value: 'ellipsis', label: '…', title: t('Shortened with “…”') },
            { value: 'clip', label: t('Cut'), title: t('Cut at the box') },
            { value: 'visible', label: t('Overflow'), title: t('Drawn outside its box') },
          ])}
        </Row>
      ),
      use('squeeze') && (
        <Row key="sq" label={t('Narrower to')}>
          {numField('squeeze', { display: 100, min: 0.5, max: 1, suffix: '%' })}
        </Row>
      ),
      use('minSize') && (
        <Row key="ms" label={t('Smaller to')}>
          {numField('minSize', { display: 100, min: 0.2, max: 1, suffix: '%' })}
        </Row>
      ),
      use('fitEnd') && (
        <Row key="fe" label={t('Then')}>
          {seg('fitEnd', [
            { value: 'clip', label: t('Cut') },
            { value: 'ellipsis', label: '…' },
          ])}
        </Row>
      ),
    ].filter(Boolean);
    if (fit.length)
      out.push(
        <OpSection key="fit" title={t('When the text is too long')}>
          {fit}
        </OpSection>,
      );
    return out;
  }
}

/** A setting this page has no layout of its own for: the Designer's kind of control for its value. */
/** Bounds of the settings exposed before their fields kept them (a slider for them). */
const SETTING_RANGES: Record<string, [number, number]> = { speed: [10, 800] };

function SettingInput({ field, value, onChange, range }: { field: Field; value: string; onChange: (v: string) => void; /** Between these bounds: a slider. */ range?: [number, number] }) {
  if (field.options?.length) {
    const short = field.options.length <= 4 && field.options.every(([, l]) => l.length <= 12);
    const options = [...(field.options.some(([v]) => v === value) ? [] : [[value, value] as [string, string]]), ...field.options];
    return short ? <Segmented value={value} onChange={onChange} options={field.options.map(([v, l]) => ({ value: v, label: l }))} /> : <Dropdown value={value} onChange={onChange} options={options.map(([v, l]) => ({ value: v, label: l }))} />;
  }
  switch (field.kind) {
    case 'number':
      if (range) return <SliderNum value={parseFloat(value) || 0} min={range[0]} max={range[1]} step={range[1] - range[0] > 20 ? 1 : 0.1} precision={range[1] - range[0] > 20 ? 0 : 1} onRelease onChange={(v) => onChange(String(v))} />;
      return (
        <span className="pp-field">
          <NumberField value={parseFloat(value) || 0} min={field.min} max={field.max} step={1} precision={value.includes('.') ? Math.min(3, value.split('.')[1].length) : 0} onChange={(v) => onChange(String(v))} />
        </span>
      );
    case 'boolean':
      return (
        <button className={`wk-tog ${TRUE.has(value) ? 'on' : ''}`} onClick={() => onChange(TRUE.has(value) ? 'false' : 'true')}>
          <i />
        </button>
      );
    case 'color':
      return (
        <span className="pp-field color">
          <ColorField value={parseHex(value) ?? [1, 1, 1, 1]} onChange={(x) => onChange(toHex(x, x[3] < 1))} />
        </span>
      );
    case 'image':
      return <AssetPicker value={value} onChange={onChange} />;
    default:
      return <TextInput autoGrow value={value} onChange={onChange} multiline={field.kind === 'multiline'} />;
  }
}

/** A text fed by a data set: what it shows, read only (the data set is chosen on the Data side). */
export function FedText({ label, value, title, tail, children }: { label: ReactNode; value: string; title: string; tail?: ReactNode; children?: ReactNode }) {
  const lines = value.split('\n');
  return (
    <div>
      <Row label={label} tail={tail}>
        <span className={`opx-bound opx-fed ${lines.length > 1 ? 'multi' : ''}`} title={title}>
          <Rows3 size={12} className="opx-fed-icon" />
          <span className="opx-fed-v">{value || '—'}</span>
        </span>
      </Row>
      {children}
    </div>
  );
}

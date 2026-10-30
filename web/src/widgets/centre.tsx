// The control centre of a widget (validated design, Oct. 2026): three lines,
// the same in the Designer's right panel and in the Operator (preview bar and
// On air card).
//   1. The state: a readout (page 2 / 3, 04:32, 64 %…), page dots, and what
//      it waits for (a countdown ring, « » F2 », « ⚡ name »).
//   2. The actions, on ONE row. Designer: the squares of the timeline transport
//      (icon, key in the tooltip). Operator: the main action keeps its label
//      and key as the bottom bar (Lire P, Continuer F2…), the others are
//      squares as the left panel.
//   3. The settings: the kit's rows (Toggle, Seg, RangeNum… with a `path`).
//      In the Designer each row carries its expose-to-Operator radio; in the
//      Operator only the exposed ones show (and the rows without `expose`,
//      always there), reading and writing the take's data.
import { ArrowRight, Bolt, ChevronsRight, type LucideIcon } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { confirmDialog } from '../ui/dialogs';
import { t, tc } from '../i18n';
import { useApp } from '../store';
import type { Data, Layer } from '../engine/types';
import { formatSetting, parseSetting, readPath, withSettings } from '../engine/settings';
import { Bound, ExposedFilter, Seg, Text, useBound, useCentreMode } from './kit';

export { CentreModeContext, ExposedFilter, useCentreMode, type CentreMode } from './kit';

// -------------------------------------------------------------- line 2 --

export interface Action {
  icon: LucideIcon;
  /** Its label (the Operator's main action shows it; the others keep it in the tooltip). */
  label: string;
  /** Its key, shown in the Operator's main action and in the tooltips. */
  key?: string;
  onClick: () => void;
  /** The main action: Lire / Pause, Continuer, the trigger (violet in the Designer, labelled in the Operator). */
  main?: boolean;
  /** red: a destructive action (Effacer). */
  tone?: 'red' | 'violet';
  disabled?: boolean;
  /** Asks first (set on air only). */
  confirm?: string;
  /** A short text instead of the icon (−10 s, + 1…). */
  text?: string;
  title?: string;
}

/** A gap pushing the next actions to the right (Actualiser). */
export const GAP = 'gap' as const;
/** A field with its Régler button (the counter's value, a time…). */
export interface ActionField {
  field: true;
  placeholder: string;
  title: string;
  /** Returns an error text to refuse the value. */
  onSet: (v: string) => string | void;
  width?: number;
}
export type Line2Item = Action | ActionField | typeof GAP | false | null | undefined;

function ActionButton({ a }: { a: Action }) {
  const mode = useCentreMode();
  const press = async () => {
    if (a.confirm && !(await confirmDialog({ title: a.confirm, confirm: t('Yes') }))) return;
    a.onClick();
  };
  const title = a.title ?? (a.key ? `${a.label} · ${a.key}` : a.label);
  const Icon = a.icon;
  if (mode !== 'designer' && a.main)
    return (
      <button className={`cc-ob ${a.tone ?? ''}`} disabled={a.disabled} title={title} onClick={press}>
        {a.text ? <span className="cc-txt">{a.text}</span> : <Icon size={14} />}
        <span>{a.label}</span>
        {a.key && <kbd className="kc">{a.key}</kbd>}
      </button>
    );
  return (
    <button className={`cc-sq ${a.main ? 'main' : ''} ${a.tone ?? ''} ${a.text ? 'txt' : ''}`} disabled={a.disabled} title={title} onClick={press}>
      {a.text ? <span className="cc-txt">{a.text}</span> : <Icon size={15} />}
    </button>
  );
}

function FieldAction({ f }: { f: ActionField }) {
  return <SetInput placeholder={f.placeholder} title={f.title} onSet={f.onSet} width={f.width} />;
}

/** An EMPTY field (placeholder only) and its Régler square. */
export function SetInput({ placeholder, title, onSet, width = 76 }: { placeholder: string; title: string; onSet: (v: string) => string | void; width?: number }) {
  const toast = useApp((s) => s.toast);
  const [v, setV] = useState('');
  const send = () => {
    const s = v.trim();
    if (!s) return;
    const err = onSet(s);
    if (typeof err === 'string') return void toast(t('{error} — nothing changed on air.', { error: err }), 'error');
    setV('');
  };
  return (
    <>
      <input
        className="cc-fld mono"
        style={{ width }}
        title={title}
        placeholder={placeholder}
        value={v}
        onChange={(e) => setV(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') send();
          if (e.key === 'Escape') setV('');
        }}
      />
      <button className="cc-sq" disabled={!v.trim()} title={tc('cc', 'Set')} onClick={send}>
        <ArrowRight size={15} />
      </button>
    </>
  );
}

// -------------------------------------------------------------- line 1 --

/** The countdown ring: `left` seconds of `total`. */
export function Countdown({ left, total, title }: { left: number; total: number; title?: string }) {
  const n = Math.max(0, Math.ceil(left));
  return (
    <span className="cc-cd" style={{ '--p': total > 0 ? Math.max(0, Math.min(1, left / total)) : 0 } as React.CSSProperties} title={title ?? t('Next in {n} s', { n })}>
      <span>{n}</span>
    </span>
  );
}

/** It waits for Continuer (F2). */
export function WaitsContinue({ title }: { title?: string }) {
  return (
    <span className="cc-st" title={title ?? t('Waits for Continue')}>
      <ChevronsRight size={13} /> <kbd className="kc">F2</kbd>
    </span>
  );
}

/** It waits for a trigger. */
export function WaitsTrigger({ name }: { name: string }) {
  return (
    <span className="cc-st" title={t('Waits for the trigger “{name}”', { name })}>
      <Bolt size={13} /> {name || tc('cc', 'trigger')}
    </span>
  );
}

/** Page dots. */
export function Dots({ at, count }: { at: number; count: number }) {
  if (count < 2 || count > 12) return null;
  return (
    <span className="cc-dots">
      {Array.from({ length: count }, (_, i) => (
        <i key={i} className={i === at ? 'on' : ''} />
      ))}
    </span>
  );
}

// ------------------------------------------------------------ the centre --

export interface Readout {
  /** A small label before the number (Page, Étape, Vue…). */
  k?: string;
  /** The value (big, monospace). */
  v: ReactNode;
  /** "/ 3". */
  of?: ReactNode;
  /** A sub text after it. */
  sub?: ReactNode;
  /** After the text: the page dots. */
  after?: ReactNode;
}

/**
 * The three lines. `readout` and `status` make line 1; `actions` line 2 (and `rows` the extra
 * rows under it: the scoreboard's teams); `children` line 3 (kit rows). In the Operator the
 * widget's name opens line 1.
 */
export function Centre({ name, readout, status, actions, rows, children }: { name?: string; readout?: Readout; status?: ReactNode; actions?: Line2Item[]; rows?: ReactNode; children?: ReactNode }) {
  const mode = useCentreMode();
  // A tile on air: the actions only (a value is typed in the preview, then Update).
  const list = (actions ?? []).filter((a) => a && !(mode === 'air' && a !== GAP && 'field' in a)) as Exclude<Line2Item, false | null | undefined>[];
  return (
    <div className={`cc ${mode}`}>
      {/* On air and in the preview: the controls only (no readout, no state). */}
      {mode !== 'air' && (readout || status || (mode !== 'designer' && name)) && (
        <div className="cc-l cc-l1">
          {mode !== 'designer' && name && <span className="cc-name">{name}</span>}
          {readout?.k && <span className="cc-k">{readout.k}</span>}
          {readout && (
            <span className="cc-num">
              {readout.v}
              {readout.of != null && <small> / {readout.of}</small>}
            </span>
          )}
          {readout?.sub && <span className="cc-sub">{readout.sub}</span>}
          {readout?.after}
          {status && <span className="cc-stw">{status}</span>}
        </div>
      )}
      {list.length > 0 && (
        <div className="cc-l cc-l2">
          {list.map((a, i) => (a === GAP ? <span key={i} className="cc-gap" /> : 'field' in a ? <FieldAction key={i} f={a} /> : <ActionButton key={i} a={a} />))}
        </div>
      )}
      {rows && <div className="cc-l cc-rows">{rows}</div>}
      {children && mode !== 'air' && <div className="cc-l cc-l3">{children}</div>}
    </div>
  );
}

/**
 * The Operator's binding of line 3: the layer with the take's settings applied is read, and a
 * change writes the exposed field of the path into the take (`setData`). Rows whose paths are
 * not exposed are hidden (ExposedFilter).
 */
export function OperatorBound({ layer, data, setData, children }: { layer: Layer; data: Data; setData?: (patch: Record<string, string>) => void; children: ReactNode }) {
  const sb = layer.settingBindings ?? {};
  const shown = withSettings({ layers: [layer], fields: [] } as never, data).layers[0] as Layer;
  const set = (path: string, v: unknown) => {
    const key = sb[path];
    if (key && setData) setData({ [key]: formatSetting(v) });
  };
  const exposed = (paths: readonly string[]) => paths.every((p) => !!sb[p]);
  return (
    <ExposedFilter.Provider value={exposed}>
      <Bound obj={shown} set={set} data={data}>
        {children}
      </Bound>
    </ExposedFilter.Provider>
  );
}

/** The value of an exposed setting as the take has it (else the layer's). */
export function exposedValue<T>(layer: Layer, data: Data, path: string, fallback: T): T {
  const key = layer.settingBindings?.[path];
  const raw = key ? data[key] : undefined;
  if (raw === undefined) return (readPath(layer, path) as T) ?? fallback;
  const v = parseSetting(raw, readPath(layer, path) ?? fallback);
  return (v === undefined ? fallback : v) as T;
}

// ------------------------------------------------------------- start --

/** How a widget that moves by itself starts on air: at the In, on Continuer (F2) or on a trigger. */
export type StartMode = 'in' | 'continue' | 'trigger';
export const START_MODES = (): [StartMode, string][] => [
  ['in', tc('cc', 'At the In')],
  ['continue', tc('cc', 'Continue F2')],
  ['trigger', tc('cc', 'Trigger')],
];
export const startMode = (l: { manualStart?: boolean; startOn?: 'continue' | 'trigger' }): StartMode => (!l.manualStart ? 'in' : l.startOn === 'trigger' ? 'trigger' : 'continue');
export const setStartMode = (l: { manualStart?: boolean; startOn?: 'continue' | 'trigger' }, m: StartMode) => {
  l.manualStart = m !== 'in' || undefined;
  l.startOn = m === 'trigger' ? 'trigger' : undefined;
};

/** Line 3 of a widget that moves by itself, in the Designer: how it starts on air (the template's setting; the Operator sees its state on line 1). */
export function StartRow({ layer }: { layer: { manualStart?: boolean; startOn?: 'continue' | 'trigger'; startTrigger?: string } }) {
  const mode = useCentreMode();
  const b = useBound();
  if (mode !== 'designer') return null;
  const m = startMode(layer);
  return (
    <>
      <Seg
        label={tc('cc', 'Start')}
        value={m}
        options={START_MODES()}
        onChange={(v) => {
          b.set('manualStart', v !== 'in' ? true : undefined);
          b.set('startOn', v === 'trigger' ? 'trigger' : undefined);
        }}
      />
      {m === 'trigger' && <Text label={tc('cc', 'Trigger name')} path="startTrigger" value="" />}
    </>
  );
}

/** Line 1 of a widget waiting for its start on air (manual start, no clock yet): what it waits for; else nothing. */
export function StartStatus({ layer, item }: { layer: { id: string; manualStart?: boolean; startOn?: 'continue' | 'trigger'; startTrigger?: string }; item?: { clocks?: Record<string, unknown> } }) {
  if (!layer.manualStart || !item || item.clocks?.[layer.id]) return null;
  return layer.startOn === 'trigger' ? <WaitsTrigger name={layer.startTrigger ?? ''} /> : <WaitsContinue />;
}

/** How pages / steps / views go on: by themselves after a while, on Continuer (F2) or on a trigger. */
export type AdvanceMode = 'timed' | 'continue' | 'trigger';
export const ADVANCE_MODES = (): [AdvanceMode, string][] => [
  ['timed', tc('cc', 'Timed')],
  ['continue', tc('cc', 'Continue F2')],
  ['trigger', tc('cc', 'Trigger')],
];

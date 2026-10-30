import { FileUp, Play, Pause, RotateCcw, SkipBack, X } from 'lucide-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { defaultData, holdFrame, newId } from '../engine/evaluate';
import { LiveTime } from '../engine/LiveTime';
import { Scene, Viewport } from '../engine/Scene';
import type { Easing, FieldKind, Keyframe, Layer, Project, Prop, ProjectSummary, Template, Value } from '../engine/types';
import { api, useApp } from '../store';
import { t, tn } from '../i18n';
import { widgetOf } from '../widgets';

// ------------------------------------------------- the server's analysis --
// Mirror of crates/cg-server/src/import/mod.rs.

type Status = 'ok' | 'partial' | 'ignored';
type Format = 'aep' | 'mogrt' | 'psd' | 'svg';
interface Note {
  label: string;
  params?: Record<string, string>;
  status: Status;
  anim?: boolean;
}
interface LayerReport {
  layer: string;
  name: string;
  source: string;
  depth: number;
  status: Status;
  notes: Note[];
  hidden: boolean;
}
interface Ignored {
  name: string;
  source: string;
  reason: string;
  params?: Record<string, string>;
}
interface VariableReport {
  name: string;
  key: string;
  kind: FieldKind;
  status: Status;
  note?: Note;
}
interface Comp {
  id: string;
  name: string;
  duration: number;
  selected: boolean;
  template: Template;
  layers: LayerReport[];
  ignored: Ignored[];
  variables: VariableReport[];
  animated: boolean;
  flat?: string;
}
interface Analysis {
  token: string;
  format: Format;
  name: string;
  width: number;
  height: number;
  fps: number;
  comps: Comp[];
  warnings: Note[];
}

/** Files the modal takes (a Strata project .json goes to its own import). */
export const IMPORT_ACCEPT = '.aep,.zip,.mogrt,.psd,.psb,.svg,.json';

const BADGE: Record<Format, [string, string]> = {
  aep: ['AE', 'b-ae'],
  mogrt: ['MOGRT', 'b-ae'],
  psd: ['PS', 'b-ps'],
  svg: ['SVG', 'b-svg'],
};

/** A reader's note in the person's language (its values too: effect and blend mode names…). */
const noteText = (n: { label: string; params?: Record<string, string> }) => {
  const params = Object.fromEntries(Object.entries(n.params ?? {}).map(([k, v]) => [k, t(v)]));
  // Values the label has no place for follow it (« Expression · Position »).
  const extra = Object.entries(params)
    .filter(([k]) => !n.label.includes(`{${k}}`))
    .map(([, v]) => v);
  return [t(n.label, params), ...extra].join(' · ');
};

// ------------------------------------------------------------- options --

type Preset = 'none' | 'fade' | 'slide' | 'scale';
type Placement = 'center' | 'corner' | 'full';
interface Options {
  /** Layers switched on, by comp then layer id. */
  on: Record<string, Record<string, boolean>>;
  preset: Preset;
  /** Each top-level layer one after the other (else all together). */
  stagger: boolean;
  /** PSD: groups kept (else the layers only). */
  groups: boolean;
  /** SVG: where the drawing goes, and as layers or one picture. */
  placement: Placement;
  asPicture: boolean;
}

const PRESET_SECS = 0.4;
const STAGGER_SECS = 0.1;

/** Frames of a template retimed from one frame rate to another. */
function retime(tpl: Template, k: number): Template {
  if (Math.abs(k - 1) < 1e-9) return tpl;
  const f = (x: number) => Math.round(x * k);
  return {
    ...tpl,
    duration: Math.max(1, f(tpl.duration)),
    markers: tpl.markers.map((m) => ({ ...m, frame: f(m.frame) })),
    layers: tpl.layers.map((l) => ({
      ...l,
      tracks: Object.fromEntries(Object.entries(l.tracks ?? {}).map(([p, ks]) => [p, (ks as Keyframe[]).map((kf) => ({ ...kf, frame: f(kf.frame) }))])) as Layer['tracks'],
    })),
  };
}

/** The layers kept: those switched on, and their parents; their children go with a group switched off. */
function keptLayers(layers: Layer[], on: Record<string, boolean> | undefined): Layer[] {
  const off = new Set(layers.filter((l) => on && on[l.id] === false).map((l) => l.id));
  const byId = new Map(layers.map((l) => [l.id, l]));
  const gone = (l: Layer): boolean => off.has(l.id) || (!!l.parent && !!byId.get(l.parent) && gone(byId.get(l.parent)!));
  return layers.filter((l) => !gone(l));
}

/** A still template given an in and out animation: each top-level layer (or all at once) fades, slides or scales in, holds, and goes out the other way. */
function withPreset(tpl: Template, preset: Preset, stagger: boolean, fps: number): Template {
  if (preset === 'none') return tpl;
  const top = tpl.layers.filter((l) => !l.parent);
  const d = Math.max(1, Math.round(PRESET_SECS * fps));
  const step = stagger ? Math.round(STAGGER_SECS * fps) : 0;
  const inEnd = d + step * Math.max(0, top.length - 1);
  const outEnd = inEnd + inEnd;
  const easeOut: Easing = { type: 'easeOut' };
  const easeIn: Easing = { type: 'easeIn' };
  const num = (v: Value | undefined, or: number) => (typeof v === 'number' ? v : or);
  const tracks = (l: Layer, i: number): Layer['tracks'] => {
    const a = i * step;
    const b = inEnd + (top.length - 1 - i) * step;
    const out: Partial<Record<Prop, Keyframe[]>> = { ...(l.tracks ?? {}) };
    const span = (prop: Prop, from: number, to: number) => {
      out[prop] = [
        { frame: a, value: from, easing: easeOut },
        { frame: a + d, value: to, easing: easeIn },
        { frame: b, value: to, easing: easeIn },
        { frame: b + d, value: from, easing: easeIn },
      ];
    };
    const opacity = num(l.props.opacity, 1);
    span('opacity', 0, opacity);
    if (preset === 'slide') span('x', num(l.props.x, 0) - 80, num(l.props.x, 0));
    if (preset === 'scale') {
      span('scaleX', num(l.props.scaleX, 1) * 0.85, num(l.props.scaleX, 1));
      span('scaleY', num(l.props.scaleY, 1) * 0.85, num(l.props.scaleY, 1));
    }
    return out;
  };
  const index = new Map(top.map((l, i) => [l.id, i]));
  return {
    ...tpl,
    duration: outEnd + 1,
    markers: [...tpl.markers.filter((m) => m.kind !== 'out'), { id: newId(), frame: inEnd, name: 'On air', kind: 'out' }],
    layers: tpl.layers.map((l) => (index.has(l.id) ? { ...l, tracks: tracks(l, index.get(l.id)!) } : l)),
  };
}

/** The template a composition becomes with the options of the modal. */
function build(a: Analysis, comp: Comp, o: Options, size: { width: number; height: number }, fps: number): Template {
  let tpl: Template = { ...comp.template, name: comp.name };
  let layers = keptLayers(tpl.layers, o.on[comp.id]);
  if (a.format === 'svg' && o.asPicture && comp.flat) {
    layers = [
      {
        id: newId(),
        name: comp.name,
        type: 'image',
        src: comp.flat,
        fit: 'contain',
        visible: true,
        locked: false,
        props: { x: 0, y: 0, width: a.width, height: a.height, opacity: 1 },
        tracks: {},
        bindings: {},
      } as unknown as Layer,
    ];
  }
  if (a.format === 'psd' && !o.groups) {
    // The layers only: each one placed where its groups put it (a PSD group only moves and fades its children).
    const byId = new Map(layers.map((l) => [l.id, l]));
    const num = (v: Value | undefined, or: number) => (typeof v === 'number' ? v : or);
    layers = layers
      .filter((l) => l.type !== 'group')
      .map((l) => {
        let x = num(l.props.x, 0);
        let y = num(l.props.y, 0);
        let opacity = num(l.props.opacity, 1);
        for (let g = l.parent ? byId.get(l.parent) : undefined; g; g = g.parent ? byId.get(g.parent) : undefined) {
          x += num(g.props.x, 0);
          y += num(g.props.y, 0);
          opacity *= num(g.props.opacity, 1);
        }
        return { ...l, parent: null, props: { ...l.props, x, y, opacity } };
      });
  }
  if (a.format === 'svg') {
    // The drawing (its root layers) placed on the frame.
    const sx = o.placement === 'full' ? size.width / a.width : 1;
    const sy = o.placement === 'full' ? size.height / a.height : 1;
    const s = Math.min(sx, sy);
    const w = a.width * s;
    const h = a.height * s;
    const dx = o.placement === 'center' || o.placement === 'full' ? (size.width - w) / 2 : size.width * 0.05;
    const dy = o.placement === 'center' || o.placement === 'full' ? (size.height - h) / 2 : size.height * 0.05;
    layers = layers.map((l) => {
      if (l.parent) return l;
      const p = { ...l.props };
      p.x = (typeof p.x === 'number' ? p.x : 0) * s + dx;
      p.y = (typeof p.y === 'number' ? p.y : 0) * s + dy;
      if (s !== 1) (p.scaleX = (typeof p.scaleX === 'number' ? p.scaleX : 1) * s), (p.scaleY = (typeof p.scaleY === 'number' ? p.scaleY : 1) * s);
      return { ...l, props: p };
    });
  }
  tpl = { ...tpl, layers };
  tpl = retime(tpl, fps / (a.fps || fps));
  if (!comp.animated) tpl = withPreset(tpl, o.preset, o.stagger, fps);
  return tpl;
}

// --------------------------------------------------------------- modal --

const FORMATS: [number, number][] = [
  [1920, 1080],
  [3840, 2160],
  [1280, 720],
  [1080, 1920],
];
const RATES: [number, number][] = [
  [24000, 1001],
  [24, 1],
  [25, 1],
  [50, 1],
  [30000, 1001],
  [30, 1],
  [60000, 1001],
  [60, 1],
];
const rateName = ([n, d]: [number, number]) => (d === 1 ? `${n}` : (n / d).toFixed(2).replace('.', ','));

/** Import of an After Effects, MOGRT, Photoshop or SVG file: what each layer becomes, the animations, what is left out; then a new project. */
export function ImportModal({ file: first, onClose, onCreated, onProject, into, onImported }: { file?: File; onClose: () => void; onCreated?: (p: ProjectSummary) => void; /** A Strata project (.strata.json). */ onProject?: (f: File) => void; /** Templates added to this project (from the Designer), laid out for its format. */ into?: Project; onImported?: (ids: string[]) => void }) {
  const toast = useApp((s) => s.toast);
  const [file, setFile] = useState<File | undefined>(first);
  const [busy, setBusy] = useState(false);
  const [a, setA] = useState<Analysis | null>(null);
  const [error, setError] = useState('');
  const [active, setActive] = useState('');
  const [picked, setPicked] = useState<Record<string, boolean>>({});
  const [name, setName] = useState('');
  const [size, setSize] = useState<[number, number]>([1920, 1080]);
  const [rate, setRate] = useState<[number, number]>([25, 1]);
  const [o, setO] = useState<Options>({ on: {}, preset: 'fade', stagger: true, groups: true, placement: 'corner', asPicture: false });
  const [sel, setSel] = useState<string | null>(null);
  const [drag, setDrag] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!file) return;
    let gone = false;
    setBusy(true);
    setError('');
    setA(null);
    fetch(`/api/import/analyze?name=${encodeURIComponent(file.name)}`, { method: 'POST', credentials: 'same-origin', body: file })
      .then(async (r) => {
        if (!r.ok) throw new Error(await r.text());
        return (await r.json()) as Analysis;
      })
      .then((res) => {
        if (gone) return;
        setA(res);
        setActive(res.comps.find((c) => c.selected)?.id ?? res.comps[0]?.id ?? '');
        setPicked(Object.fromEntries(res.comps.map((c) => [c.id, c.selected])));
        setName(res.name);
        // The document's size and rate when they are a broadcast format, else HD.
        const fmt = FORMATS.find(([w, h]) => w === res.width && h === res.height);
        setSize(into ? [into.width, into.height] : (fmt ?? (res.format === 'svg' || res.width < 1280 ? [1920, 1080] : [res.width, res.height])));
        const r = RATES.find(([n, d]) => Math.abs(n / d - res.fps) < 0.01);
        setRate(into ? [into.frameRate.num, into.frameRate.den] : (r ?? [25, 1]));
        setO((x) => ({ ...x, on: Object.fromEntries(res.comps.map((c) => [c.id, Object.fromEntries(c.layers.map((l) => [l.layer, !l.hidden]))])) }));
      })
      .catch((e) => !gone && setError((e as Error).message))
      .finally(() => !gone && setBusy(false));
    return () => {
      gone = true;
    };
  }, [file]);

  const fps = rate[0] / rate[1];
  const comp = a?.comps.find((c) => c.id === active);
  const chosen = a ? a.comps.filter((c) => picked[c.id]) : [];
  const preview = useMemo(() => (a && comp ? build(a, comp, o, { width: size[0], height: size[1] }, fps) : null), [a, comp, o, size, fps]);

  // Counts over the compositions kept.
  const counts = useMemo(() => {
    let ok = 0;
    let partial = 0;
    let ignored = 0;
    for (const c of chosen) {
      for (const l of c.layers) {
        if (o.on[c.id]?.[l.layer] === false) ignored++;
        else if (l.status === 'partial') partial++;
        else ok++;
      }
      ignored += c.ignored.length;
    }
    return { ok, partial, ignored, vars: chosen.reduce((n, c) => n + c.variables.length, 0) };
  }, [chosen, o.on]);

  const create = async () => {
    if (!a || !chosen.length) return;
    setBusy(true);
    try {
      const templates = chosen.map((c) => build(a, c, o, { width: size[0], height: size[1] }, fps));
      if (into) {
        // One composition (an SVG, a Photoshop file without artboards): the template takes the file's name.
        if (a.comps.length === 1 && file) templates[0].name = file.name.replace(/\.[^.]+$/, '');
        onImported?.(await api<string[]>('POST', `/api/import/${a.token}/into/${encodeURIComponent(into.id)}`, { templates }));
        return;
      }
      const p = await api<ProjectSummary>('POST', `/api/import/${a.token}/create`, { name: name.trim() || a.name, width: size[0], height: size[1], frameRate: { num: rate[0], den: rate[1] }, templates });
      onCreated?.(p);
    } catch (e) {
      toast(t('Import failed: {error}', { error: (e as Error).message }), 'error');
      setBusy(false);
    }
  };

  const setOn = (layer: string, v: boolean) => comp && setO({ ...o, on: { ...o.on, [comp.id]: { ...o.on[comp.id], [layer]: v } } });
  const pick = (f?: File) => {
    if (!f) return;
    if (f.name.toLowerCase().endsWith('.json')) return into ? undefined : onProject?.(f);
    setFile(f);
  };

  return createPortal(
    <div className="modal-back" onPointerDown={onClose}>
      <div className="modal imp" onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === 'Escape' && onClose()} tabIndex={-1}>
        <header className="imp-head">
          <h2>{t('Import')}</h2>
          {file && (
            <span className="imp-file">
              {a && <span className={`imp-badge ${BADGE[a.format][1]}`}>{BADGE[a.format][0]}</span>}
              <span className="imp-fname">{file.name}</span>
              {a && (
                <span className="muted small">
                  · {a.format === 'psd' ? tn(a.comps.length, '{n} artboard', '{n} artboards') : tn(a.comps.length, '{n} composition', '{n} compositions')} · {a.width}×{a.height}
                  {a.comps.some((c) => c.animated) ? ` · ${String(Number(a.fps.toFixed(3))).replace('.', ',')} ${t('fps')}` : ''}
                </span>
              )}
            </span>
          )}
          <span className="spacer" />
          <button className="icon-btn ghost" onClick={onClose} title={t('Close')}>
            <X size={15} />
          </button>
        </header>

        {!a ? (
          <div
            className={`imp-drop ${drag ? 'over' : ''}`}
            onDragOver={(e) => (e.preventDefault(), setDrag(true))}
            onDragLeave={() => setDrag(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDrag(false);
              pick(e.dataTransfer.files[0]);
            }}
          >
            {busy ? (
              <span className="imp-busy">{t('Reading {name}…', { name: file?.name ?? '' })}</span>
            ) : (
              <div className="imp-dz" onClick={() => input.current?.click()}>
                <FileUp size={22} />
                <strong>{t('Drop a file')}</strong>
                <div className="imp-fmts">
                  {(
                    [
                      ['AE', 'b-ae', 'After Effects', '.aep · .zip'],
                      ['MOGRT', 'b-ae', 'Motion Graphics', '.mogrt'],
                      ['PS', 'b-ps', 'Photoshop', '.psd'],
                      ['SVG', 'b-svg', 'SVG', '.svg'],
                    ] as const
                  ).map(([b, c, n, e]) => (
                    <span key={b} className="imp-fmt">
                      <span className={`imp-badge ${c}`}>{b}</span>
                      <b>{n}</b>
                      <small>{e}</small>
                    </span>
                  ))}
                </div>
                <span className="btn">{t('Browse…')}</span>
                {error && <span className="imp-error">{error}</span>}
              </div>
            )}
            <input ref={input} type="file" accept={into ? IMPORT_ACCEPT.replace(',.json', '') : IMPORT_ACCEPT} hidden onChange={(e) => pick(e.target.files?.[0])} />
          </div>
        ) : (
          <>
            <div className="imp-body">
              {/* Left: compositions and the project. */}
              <div className="imp-col">
                {a.comps.length > 1 || a.format !== 'svg' ? (
                  <>
                    <div className="imp-sec">
                      {a.format === 'psd' ? t('Artboards') : t('Compositions')}
                      <em>
                        {chosen.length} / {a.comps.length}
                      </em>
                    </div>
                    <div className="imp-comps">
                      {a.comps.map((c) => (
                        <div key={c.id} className={`imp-comp ${c.id === active ? 'on' : ''}`} onClick={() => setActive(c.id)}>
                          <Switch on={!!picked[c.id]} onChange={(v) => setPicked({ ...picked, [c.id]: v })} />
                          {c.flat ? <img className="imp-thumb" src={c.flat} alt="" /> : <span className="imp-thumb" />}
                          <span className={`imp-cname ${picked[c.id] ? '' : 'muted'}`}>{c.name}</span>
                          <span className="imp-meta">{c.duration > 0 ? secs(c.duration) : ''}</span>
                        </div>
                      ))}
                    </div>
                  </>
                ) : null}
                {!into && (
                  <>
                <div className="imp-sec">{t('Project')}</div>
                <label className="imp-field">
                  <span>{t('Name')}</span>
                  <input value={name} onChange={(e) => setName(e.target.value)} />
                </label>
                <label className="imp-field">
                  <span>{t('Format')}</span>
                  <div className="imp-row">
                    <select value={`${size[0]}x${size[1]}`} onChange={(e) => setSize(e.target.value.split('x').map(Number) as [number, number])}>
                      {[...FORMATS, ...(FORMATS.some(([w, h]) => w === size[0] && h === size[1]) ? [] : [size])].map(([w, h]) => (
                        <option key={`${w}x${h}`} value={`${w}x${h}`}>
                          {w}×{h}
                        </option>
                      ))}
                    </select>
                    <select value={`${rate[0]}/${rate[1]}`} onChange={(e) => setRate(e.target.value.split('/').map(Number) as [number, number])}>
                      {RATES.map((r) => (
                        <option key={`${r[0]}/${r[1]}`} value={`${r[0]}/${r[1]}`}>
                          {rateName(r)} {t('fps')}
                        </option>
                      ))}
                    </select>
                  </div>
                </label>
                  </>
                )}
                {a.format === 'psd' && (
                  <div className="imp-field">
                    <span>{t('Groups')}</span>
                    <Seg value={o.groups ? 'g' : 'f'} onChange={(v) => setO({ ...o, groups: v === 'g' })} options={[['g', t('Groups')], ['f', t('Flat')]]} />
                  </div>
                )}
                {a.format === 'svg' && (
                  <>
                    <div className="imp-field">
                      <span>{t('Placement')}</span>
                      <Seg value={o.placement} onChange={(v) => setO({ ...o, placement: v as Placement })} options={[['center', t('Centre')], ['corner', t('Corner')], ['full', t('Full')]]} />
                    </div>
                    {comp?.flat && (
                      <div className="imp-field">
                        <span>{t('Import')}</span>
                        <Seg value={o.asPicture ? 'p' : 'l'} onChange={(v) => setO({ ...o, asPicture: v === 'p' })} options={[['l', t('Layers')], ['p', t('One picture')]]} />
                      </div>
                    )}
                  </>
                )}
                {!!a.warnings.length && (
                  <div className="imp-warns">
                    {a.warnings.map((w, i) => (
                      <span key={i} className={`imp-chip ${w.status}`}>
                        {noteText(w)}
                      </span>
                    ))}
                  </div>
                )}
              </div>

              {/* Centre: preview and timeline. */}
              <div className="imp-col">
                {comp && preview && <Preview key={comp.id} tpl={preview} width={size[0]} height={size[1]} fps={fps} comp={comp} on={o.on[comp.id]} sel={sel} onSel={setSel} />}
                {comp && !comp.animated && !(a.format === 'svg' && o.asPicture) && (
                  <div className="imp-preset">
                    <span className="imp-plabel">{t('In and out animation')}</span>
                    <Seg value={o.preset} onChange={(v) => setO({ ...o, preset: v as Preset })} options={[['none', t('None')], ['fade', t('Fade')], ['slide', t('Slide')], ['scale', t('Scale')]]} />
                    {o.preset !== 'none' && <Seg value={o.stagger ? 's' : 'a'} onChange={(v) => setO({ ...o, stagger: v === 's' })} options={[['s', t('One by one')], ['a', t('Together')]]} />}
                  </div>
                )}
              </div>

              {/* Right: variables, layers and what is left out. */}
              <div className="imp-col">
                {comp && !!comp.variables.length && (
                  <>
                    <div className="imp-sec">{t('Essential Graphics controls → variables')}</div>
                    <div className="imp-vars">
                      {comp.variables.map((v) => (
                        <div key={v.key} className="imp-var">
                          <span>{v.name}</span>
                          <span className="muted">→</span>
                          <span>
                            <span className={`imp-chip ${v.status}`}>{v.note ? noteText(v.note) : t(KIND[v.kind] ?? v.kind)}</span> {v.key}
                          </span>
                        </div>
                      ))}
                    </div>
                  </>
                )}
                {comp && (
                  <>
                    <div className="imp-sec">
                      {a.format === 'svg' ? t('Elements') : t('Layers')}
                      {a.comps.length > 1 ? ` · ${comp.name}` : ''}
                      <em>
                        {comp.layers.filter((l) => o.on[comp.id]?.[l.layer] !== false).length} / {comp.layers.length + comp.ignored.length}
                      </em>
                    </div>
                    <div className="imp-layers">
                      {comp.layers.map((l) => {
                        const on = o.on[comp.id]?.[l.layer] !== false;
                        const layer = comp.template.layers.find((x) => x.id === l.layer);
                        const def = layer ? widgetOf(layer) : undefined;
                        const Icon = def?.icon;
                        return (
                          <div key={l.layer} className={`imp-layer ${on ? '' : 'off'} ${sel === l.layer ? 'sel' : ''}`} style={{ paddingLeft: 6 + l.depth * 18 }} onClick={() => setSel(l.layer)}>
                            <Switch on={on} onChange={(v) => setOn(l.layer, v)} />
                            <span className="imp-ic">{Icon ? <Icon size={12} /> : null}</span>
                            <div className="imp-lname">
                              <b>{l.name}</b>
                              {(!!l.notes.length || l.hidden) && (
                                <div className="imp-notes">
                                  {l.hidden && <span className="imp-chip">{t('Hidden in the file')}</span>}
                                  {l.notes.map((n, i) => (
                                    <span key={i} className={`imp-chip ${n.status}`}>
                                      {noteText(n)}
                                    </span>
                                  ))}
                                </div>
                              )}
                            </div>
                            <span className="imp-to">
                              → <b>{layer?.type === 'group' ? t('Group') : def ? t(def.label) : layer?.type}</b>
                              {on && <i className={`imp-dot ${l.status}`} />}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                    {!!comp.ignored.length && (
                      <div className="imp-ign">
                        <div className="imp-sec">
                          {t('Left out')}
                          <em>{comp.ignored.length}</em>
                        </div>
                        {comp.ignored.map((g, i) => (
                          <div key={i} className="imp-ignrow">
                            <span>{g.name}</span>
                            <span>{noteText({ label: g.reason, params: g.params })}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </>
                )}
              </div>
            </div>
            <footer className="imp-foot">
              <span className="imp-sum">
                <span>
                  <i className="imp-dot ok" />
                  {tn(counts.ok, '{n} layer imported', '{n} layers imported')}
                </span>
                {!!counts.partial && (
                  <span>
                    <i className="imp-dot partial" />
                    {tn(counts.partial, '{n} layer partial', '{n} layers partial')}
                  </span>
                )}
                {!!counts.ignored && (
                  <span>
                    <i className="imp-dot ignored" />
                    {tn(counts.ignored, '{n} item left out', '{n} items left out')}
                  </span>
                )}
                {!!counts.vars && (
                  <span>
                    <i className="imp-dot var" />
                    {tn(counts.vars, '{n} variable', '{n} variables')}
                  </span>
                )}
              </span>
              <span className="spacer" />
              <button className="btn" onClick={onClose}>
                {t('Cancel')}
              </button>
              <button className="btn primary" disabled={busy || !chosen.length} onClick={create}>
                {into ? tn(chosen.length, 'Import the template', 'Import the {n} templates') : chosen.length > 1 ? t('Create the project · {n} templates', { n: chosen.length }) : t('Create the project')}
              </button>
            </footer>
          </>
        )}
      </div>
    </div>,
    document.body,
  );
}

const KIND: Partial<Record<FieldKind, string>> = { text: 'Text', multiline: 'Text', number: 'Number', color: 'Colour', image: 'Picture', boolean: 'Boolean', table: 'Table' };

const secs = (s: number) => (s < 60 ? `${s.toFixed(1).replace('.', ',')} s` : `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, '0')}`);

function Switch({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      className={`imp-sw ${on ? 'on' : ''}`}
      onClick={(e) => {
        e.stopPropagation();
        onChange(!on);
      }}
    />
  );
}

function Seg({ value, options, onChange }: { value: string; options: [string, string][]; onChange: (v: string) => void }) {
  return (
    <div className="imp-seg">
      {options.map(([k, label]) => (
        <button key={k} type="button" className={k === value ? 'on' : ''} onClick={() => onChange(k)}>
          {label}
        </button>
      ))}
    </div>
  );
}

/** The composition as it will be: played, scrubbed, with its markers and each layer's keyframes. */
function Preview({ tpl, width, height, fps, comp, on, sel, onSel }: { tpl: Template; width: number; height: number; fps: number; comp: Comp; on?: Record<string, boolean>; sel: string | null; onSel: (id: string) => void }) {
  const d = Math.max(1, tpl.duration);
  const data = useMemo(() => defaultData(tpl), [tpl]);
  const [frame, setFrame] = useState(() => Math.min(d - 1, holdFrame(tpl)));
  const [playing, setPlaying] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0.3);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setScale(Math.max(0.01, Math.min(el.clientWidth / width, el.clientHeight / height))));
    ro.observe(el);
    return () => ro.disconnect();
  }, [width, height]);
  // Playing: through the whole template, then from the start again.
  useEffect(() => {
    if (!playing) return;
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      setFrame((f) => {
        const n = f + ((now - last) / 1000) * fps;
        return n >= d ? 0 : n;
      });
      last = now;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing, fps, d]);
  useEffect(() => setFrame((f) => Math.min(f, d - 1)), [d]);
  const lanes = comp.layers.filter((l) => tpl.layers.some((x) => x.id === l.layer) || on?.[l.layer] === false);
  const pct = (f: number) => `${(f / d) * 100}%`;
  // Seconds between the ruler's labels: about ten of them.
  const tick = [1, 2, 5, 10, 15, 30, 60].find((s) => d / fps / s <= 10) ?? 120;
  const ruler = useRef<HTMLDivElement>(null);
  const scrub = (e: React.PointerEvent) => {
    const r = ruler.current?.getBoundingClientRect();
    if (r) setFrame(Math.max(0, Math.min(d - 1, ((e.clientX - r.left) / r.width) * d)));
  };
  const timeAt = (f: number) => {
    const s = f / fps;
    return `${String(Math.floor(s)).padStart(2, '0')}:${String(Math.floor((s % 1) * fps)).padStart(2, '0')}`;
  };
  return (
    <>
      <div className="imp-stage" ref={box}>
        <Viewport width={width} height={height} scale={scale}>
          <LiveTime ageSecs={frame / fps} fps={fps}>
            <Scene template={tpl} frame={frame} data={data} />
          </LiveTime>
        </Viewport>
        <span className="imp-tc">{timeAt(frame)}</span>
      </div>
      <div className="imp-ctrl">
        <button className="imp-pb" onClick={() => (setPlaying(false), setFrame(0))} title={t('Start')}>
          <SkipBack size={13} />
        </button>
        <button className="imp-pb p" onClick={() => setPlaying(!playing)} title={playing ? t('Pause') : t('Play')}>
          {playing ? <Pause size={13} /> : <Play size={13} />}
        </button>
        <button className="imp-pb" onClick={() => (setPlaying(false), setFrame(Math.min(d - 1, holdFrame(tpl))))} title={t('On air')}>
          <RotateCcw size={13} />
        </button>
        <span className="imp-time">
          {timeAt(frame)} / {timeAt(d)}
        </span>
        <span className="spacer" />
        <span className="imp-mk in">IN 0:00</span>
        {tpl.markers
          .filter((m) => m.kind === 'pause')
          .map((m) => (
            <span key={m.id} className="imp-mk pause">
              PAUSE {(m.frame / fps).toFixed(1).replace('.', ',')}
            </span>
          ))}
        <span className="imp-mk out">OUT {(holdFrame(tpl) / fps).toFixed(1).replace('.', ',')}</span>
      </div>
      <div
        className="imp-tl"
        onPointerDown={(e) => {
          // Scrubbed on the ruler and the lanes (the names select).
          const r = ruler.current?.getBoundingClientRect();
          if (!r || e.clientX < r.left) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          setPlaying(false);
          scrub(e);
        }}
        onPointerMove={(e) => e.currentTarget.hasPointerCapture(e.pointerId) && scrub(e)}
      >
        <div className="imp-ruler" ref={ruler}>
          {Array.from({ length: Math.floor(d / fps / tick) + 1 }, (_, i) => (
            <i key={i} style={{ left: pct(i * tick * fps) }}>
              {i * tick}s
            </i>
          ))}
          <span className="imp-ph" style={{ left: pct(frame) }} />
        </div>
        <div className="imp-lanes">
          {lanes.map((l) => {
            const layer = tpl.layers.find((x) => x.id === l.layer);
            const frames = layer ? [...new Set(Object.values(layer.tracks ?? {}).flatMap((ks) => (ks as Keyframe[]).map((k) => k.frame)))] : [];
            const partial = l.notes.some((n) => n.anim && n.status === 'partial');
            return (
              <div key={l.layer} className={`imp-lane ${layer ? '' : 'off'} ${sel === l.layer ? 'sel' : ''}`} onClick={() => onSel(l.layer)}>
                <span className="imp-ln">{l.name}</span>
                <span className="imp-lt">
                  <span className="imp-bar" />
                  {frames.map((f) => (
                    <span key={f} className={`imp-kf ${partial ? 'partial' : ''}`} style={{ left: pct(f) }} />
                  ))}
                </span>
              </div>
            );
          })}
          <span className="imp-ph lane" style={{ left: `calc(150px + (100% - 150px) * ${frame / d})` }} />
        </div>
      </div>
    </>
  );
}

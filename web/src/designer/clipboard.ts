// The Designer's clipboard: layers (Ctrl+C / Ctrl+X / Ctrl+V) and animations
// (the keyframes of a layer, or of one property row, pasted on other layers).
// Kept in memory: it outlives the template and the project open in this tab.
import { create } from 'zustand';
import { newId, num, propValue } from '../engine/evaluate';
import { forgetKeys } from '../engine/settings';
import { setOnTimeline } from '../engine/timelineProps';
import { COLOR_PROPS, PROPS, TEXT_LIKE, type Field, type Keyframe, type Layer, type Prop, type Template, type Value } from '../engine/types';
import { useData } from '../data/dataStore';
import { useApp } from '../store';
import { widgetOf } from '../widgets';
import { t, tn } from '../i18n';
import { PROP_LABELS } from './Inspector';
import { descendants, findLayer, setProp, useDesigner } from './designerStore';
import { confirmDialog } from '../ui/dialogs';

/** Copied layers, with their children, as saved (all formats). */
export interface LayersClip {
  layers: Layer[];
  roots: string[];
  /** The variables the layers read (bound properties, settings, {{key}} in texts). */
  fields: Field[];
  /** Where each root's group put it on the stage: added when pasted outside a group. */
  offsets: Record<string, { x: number; y: number }>;
  templateId: string;
  projectId: string;
}

/** Copied keyframes, by property, at their frames in the template. */
export interface AnimClip {
  tracks: Partial<Record<Prop, Keyframe[]>>;
  /** The properties in order (a row: Position = x, y). */
  props: Prop[];
  /** A whole layer's animation, or one property row's. */
  scope: 'layer' | 'row';
  /** What was copied, for the buttons' tips: "Title · Position". */
  label: string;
}

/** Everything of a layer (« Copy all »): its values at the playhead, its keyframes, its span; pasted as chosen (PasteDialog). */
export interface AllClip {
  values: Partial<Record<Prop, Value>>;
  tracks: Partial<Record<Prop, Keyframe[]>>;
  span?: { from: number; to: number };
  label: string;
}

interface ClipboardState {
  layers: LayersClip | null;
  animation: AnimClip | null;
  all: AllClip | null;
  /** The layers « Paste… » is choosing what to paste on (PasteDialog open). */
  pasting: string[] | null;
  /** What Ctrl+V pastes. */
  last: 'layers' | 'animation' | 'all' | null;
}

export const useClipboard = create<ClipboardState>(() => ({ layers: null, animation: null, all: null, pasting: null, last: null }));

const toast = (m: string, kind: 'info' | 'error' = 'info') => useApp.getState().toast(m, kind);
const propName = (p: Prop) => t(PROP_LABELS[p] ?? p);

// ---------------------------------------------------------------------------
// Animation

/** The animated properties of a layer, in timeline order. */
export const animatedProps = (l: Layer) => PROPS.filter((p) => l.tracks[p]?.length);

/** Copies the keyframes of a layer (all its animated properties) or of some of its properties. */
export function copyAnimation(layerId: string, props?: Prop[], rowLabel?: string) {
  const tpl = useDesigner.getState().template;
  const l = tpl && findLayer(tpl, layerId);
  if (!l) return;
  const list = (props ?? PROPS).filter((p) => l.tracks[p]?.length);
  if (!list.length) return toast(t('Nothing to copy: no keyframes here'), 'error');
  const tracks: AnimClip['tracks'] = {};
  for (const p of list) tracks[p] = structuredClone(l.tracks[p]!);
  const label = rowLabel ? `${l.name} · ${t(rowLabel)}` : l.name;
  useClipboard.setState({ animation: { tracks, props: list, scope: props ? 'row' : 'layer', label }, last: 'animation' });
  toast(props ? t('Animation of {what} copied: paste it on another property or layer', { what: label }) : tn(list.length, 'Animation of {name} copied ({n} property): paste it on another layer (Ctrl+V)', 'Animation of {name} copied ({n} properties): paste it on another layer (Ctrl+V)', { name: l.name }));
}

const ALWAYS = new Set<Prop>(['x', 'y', 'width', 'height', 'anchorX', 'anchorY', 'rotation', 'scaleX', 'scaleY', 'opacity', 'cropLeft', 'cropRight', 'cropTop', 'cropBottom']);

/** Whether a layer has a property (as its properties panel shows them). */
export function layerHasProp(l: Layer, p: Prop): boolean {
  if (ALWAYS.has(p) || l.props[p] !== undefined || l.tracks[p]?.length || l.timeline?.includes(p)) return true;
  if (p === 'fontSize' || p === 'letterSpacing') return TEXT_LIKE.has(l.type) || 'fontFamily' in l;
  const look = l.type !== 'image' && l.type !== 'slideshow' && !widgetOf(l)?.ownsLook;
  if (p === 'fill') return look;
  if (p === 'cornerRadius') return look && ['rect', 'group', 'progress'].includes(l.type);
  if (p === 'stroke' || p === 'strokeWidth') return look && !['progress', 'qrCode'].includes(l.type);
  // The arcs of a circle (Ellipse widget).
  if (p === 'arcStart' || p === 'arcEnd' || p === 'innerRadius') return l.type === 'ellipse';
  // The drawn portion of a pen.
  if (p === 'drawStart' || p === 'drawEnd') return l.type === 'pen';
  // A rectangle's corners, the second colour of a gradient.
  if (p === 'cornerTl' || p === 'cornerTr' || p === 'cornerBr' || p === 'cornerBl') return l.type === 'rect';
  if (p === 'fill2') return ['rect', 'ellipse', 'polygon', 'pen', 'text'].includes(l.type);
  return false;
}

/** Which copied property goes to which property of a row: the same ones, or a row of the same kind (Fill → Stroke). */
export function rowMapping(clip: AnimClip, target: Prop[]): [Prop, Prop][] {
  const common = target.filter((p) => clip.props.includes(p));
  if (common.length) return common.map((p) => [p, p]);
  if (clip.props.length === target.length && clip.props.every((p, i) => COLOR_PROPS.has(p) === COLOR_PROPS.has(target[i]))) return clip.props.map((p, i) => [p, target[i]]);
  return [];
}

function applyTracks(l: Layer, pairs: [Prop, Prop][], clip: AnimClip) {
  for (const [from, to] of pairs) l.tracks[to] = structuredClone(clip.tracks[from]!);
  setOnTimeline(
    l,
    pairs.map(([, to]) => to),
    true,
  );
}

/** Pastes the copied animation on layers: on the properties they have (the others are skipped, and said). */
export function pasteAnimation(layerIds: string[]) {
  const st = useDesigner.getState();
  const clip = useClipboard.getState().animation;
  if (!clip || !st.template || st.readOnly) return;
  const ids = layerIds.filter((id) => findLayer(st.template!, id));
  if (!ids.length) return toast(t('Select the layers to paste the animation on'), 'error');
  const skipped = new Set<Prop>();
  let done = 0;
  st.update((tpl) => {
    for (const id of ids) {
      const l = findLayer(tpl, id)!;
      const pairs = clip.props.filter((p) => (layerHasProp(l, p) ? true : (skipped.add(p), false))).map((p) => [p, p] as [Prop, Prop]);
      applyTracks(l, pairs, clip);
      done += pairs.length ? 1 : 0;
    }
  });
  const names = [...skipped].map(propName).join(', ');
  if (!done) return toast(t('Nothing pasted: these layers do not have {props}', { props: names }), 'error');
  toast(skipped.size ? tn(done, 'Animation pasted on {n} layer — skipped (not on this widget): {props}', 'Animation pasted on {n} layers — skipped (not on these widgets): {props}', { props: names }) : tn(done, 'Animation pasted on {n} layer', 'Animation pasted on {n} layers'));
}

/** Pastes the copied animation on one property row of a layer. */
export function pasteAnimationOnRow(layerId: string, props: Prop[]) {
  const st = useDesigner.getState();
  const clip = useClipboard.getState().animation;
  if (!clip || !st.template || st.readOnly) return;
  const pairs = rowMapping(clip, props);
  if (!pairs.length) return toast(t('This animation does not fit this property'), 'error');
  st.update((tpl) => {
    const l = findLayer(tpl, layerId);
    if (l) applyTracks(l, pairs, clip);
  });
}

// ---------------------------------------------------------------------------
// Copy all, paste as chosen

/** The values « Paste… » offers, by group. */
export const PASTE_GROUPS: { id: string; label: string; props: Prop[] }[] = [
  { id: 'position', label: 'Position', props: ['x', 'y'] },
  { id: 'size', label: 'Size', props: ['width', 'height'] },
  { id: 'scale', label: 'Scale', props: ['scaleX', 'scaleY'] },
  { id: 'rotation', label: 'Rotation', props: ['rotation'] },
  { id: 'opacity', label: 'Opacity', props: ['opacity'] },
  { id: 'anchor', label: 'Anchor', props: ['anchorX', 'anchorY'] },
  { id: 'crop', label: 'Reveal mask', props: ['cropLeft', 'cropRight', 'cropTop', 'cropBottom'] },
  { id: 'colours', label: 'Colours', props: ['fill', 'fill2', 'stroke', 'strokeWidth'] },
  { id: 'corners', label: 'Corners', props: ['cornerRadius', 'cornerTl', 'cornerTr', 'cornerBr', 'cornerBl'] },
  { id: 'text', label: 'Text size and spacing', props: ['fontSize', 'letterSpacing'] },
];

/** Copies everything of a layer: its values at the playhead, its keyframes and its span. */
export function copyAll(layerId: string) {
  const st = useDesigner.getState();
  const l = st.template && findLayer(st.template, layerId);
  if (!l) return;
  const values: AllClip['values'] = {};
  for (const g of PASTE_GROUPS) for (const p of g.props) if (layerHasProp(l, p)) values[p] = structuredClone(propValue(l, p, st.frame));
  const tracks: AllClip['tracks'] = {};
  for (const p of animatedProps(l)) tracks[p] = structuredClone(l.tracks[p]!);
  useClipboard.setState({ all: { values, tracks, span: l.span ? { ...l.span } : undefined, label: l.name }, last: 'all' });
  toast(t('“{name}” copied: paste on another layer to choose what to take', { name: l.name }));
}

/** What « Paste… » can paste on these layers: the value groups both sides have, the animation, the span. */
export function pasteChoices(clip: AllClip, targets: Layer[]) {
  const has = (p: Prop) => clip.values[p] !== undefined && targets.some((l) => layerHasProp(l, p));
  const animated = (Object.keys(clip.tracks) as Prop[]).filter((p) => targets.some((l) => layerHasProp(l, p)));
  return { groups: PASTE_GROUPS.filter((g) => g.props.some(has)), animated, span: !!clip.span };
}

/** Pastes on layers what was chosen: value groups, then the animation (its keyframes win), then the span. */
export function pasteAll(layerIds: string[], chosen: { groups: string[]; animation: boolean; span: boolean }) {
  const st = useDesigner.getState();
  const clip = useClipboard.getState().all;
  if (!clip || !st.template || st.readOnly) return;
  const props = PASTE_GROUPS.filter((g) => chosen.groups.includes(g.id)).flatMap((g) => g.props);
  st.update((tpl) => {
    for (const id of layerIds) {
      const l = findLayer(tpl, id);
      if (!l || l.locked) continue;
      for (const p of props) if (clip.values[p] !== undefined && layerHasProp(l, p)) setProp(l, p, structuredClone(clip.values[p]!), st.frame);
      if (chosen.animation) {
        const ps = (Object.keys(clip.tracks) as Prop[]).filter((p) => layerHasProp(l, p));
        applyTracks(
          l,
          ps.map((p) => [p, p]),
          { tracks: clip.tracks, props: ps, scope: 'layer', label: clip.label },
        );
      }
      if (chosen.span) {
        if (clip.span) l.span = { ...clip.span };
        else delete l.span;
      }
    }
  });
  toast(tn(layerIds.length, 'Pasted on {n} layer', 'Pasted on {n} layers'));
}

// ---------------------------------------------------------------------------
// Layers

const KEY_RE = /\{\{\s*(\w+)\s*\}\}/g;

/** The data keys layers read: bound properties, exposed settings, {{key}} in their texts. */
function keysOf(layers: Layer[]): Set<string> {
  const out = new Set<string>();
  for (const l of layers) {
    for (const k of Object.values(l.bindings ?? {})) if (k) out.add(k);
    for (const k of Object.values(l.settingBindings ?? {})) if (k) out.add(k);
    for (const m of JSON.stringify(l).matchAll(KEY_RE)) out.add(m[1]);
  }
  return out;
}

/** Where the groups around a layer put it on the stage. */
function groupOffset(tpl: Template, parent: string | null | undefined, frame: number) {
  let x = 0;
  let y = 0;
  const seen = new Set<string>();
  for (let p = parent ? findLayer(tpl, parent) : undefined; p && !seen.has(p.id); p = p.parent ? findLayer(tpl, p.parent) : undefined) {
    seen.add(p.id);
    x += num(propValue(p, 'x', frame));
    y += num(propValue(p, 'y', frame));
  }
  return { x, y };
}

/** Moves a layer on the stage, in every format (keyframes included). */
function shift(l: Layer, dx: number, dy: number) {
  const move = (props: Layer['props'] | undefined, tracks: Layer['tracks'] | undefined) => {
    for (const [p, d] of [
      ['x', dx],
      ['y', dy],
    ] as const) {
      if (!d) continue;
      if (props && typeof props[p] === 'number') props[p] = (props[p] as number) + d;
      for (const k of tracks?.[p] ?? []) if (typeof k.value === 'number') k.value += d;
    }
  };
  move(l.props, l.tracks);
  for (const o of Object.values(l.overrides ?? {})) move(o.props, o.tracks);
}

/** The roots among layers: those not inside another one of them. */
function rootsOf(tpl: Template, ids: string[]) {
  const set = ids.filter((id) => findLayer(tpl, id));
  return set.filter((id) => !set.some((o) => o !== id && descendants(tpl, o).includes(id)));
}

/** Copies layers (with their children, animations, settings and bindings). */
export function copyLayers(ids: string[], quiet = false): number {
  const st = useDesigner.getState();
  const base = st.base;
  if (!base) return 0;
  const roots = rootsOf(base, ids);
  if (!roots.length) return 0;
  const all = new Set(roots.flatMap((id) => [id, ...descendants(base, id)]));
  const layers = structuredClone(base.layers.filter((l) => all.has(l.id)));
  const keys = keysOf(layers);
  useClipboard.setState({
    layers: {
      layers,
      roots,
      fields: structuredClone(base.fields.filter((f) => keys.has(f.key))),
      offsets: Object.fromEntries(roots.map((id) => [id, groupOffset(base, findLayer(base, id)!.parent, st.frame)])),
      templateId: base.id,
      projectId: useApp.getState().project?.id ?? '',
    },
    last: 'layers',
  });
  if (!quiet) toast(tn(roots.length, '{n} layer copied (Ctrl+V to paste, here or in another template)', '{n} layers copied (Ctrl+V to paste, here or in another template)'));
  return roots.length;
}

/** Cuts layers: copied, then deleted. */
export function cutLayers(ids: string[]) {
  const st = useDesigner.getState();
  if (!st.base || st.readOnly) return;
  const n = copyLayers(ids, true);
  if (!n) return;
  st.update((d) => {
    const gone = new Set(ids.flatMap((id) => [id, ...descendants(d, id)]));
    d.layers = d.layers.filter((l) => !gone.has(l.id));
  });
  st.select([]);
  toast(tn(n, '{n} layer cut (Ctrl+V to paste)', '{n} layers cut (Ctrl+V to paste)'));
}

const STEP = 20;

/** Pastes the copied layers in the open template: new ids, a little apart when they would cover a layer at the same place. */
export function pasteLayers() {
  const st = useDesigner.getState();
  const clip = useClipboard.getState().layers;
  if (!clip || !st.base || st.readOnly) return;
  const projectId = useApp.getState().project?.id ?? '';
  const sameProject = clip.projectId === projectId;
  const sameTemplate = sameProject && clip.templateId === st.base.id;
  const sources = useData.getState().sources;
  const frame = st.frame;
  const newRoots: string[] = [];
  const added: string[] = [];
  const dropped = new Set<string>();
  st.updateBase((tpl) => {
    const map = new Map(clip.layers.map((l) => [l.id, newId()]));
    const roots = new Set(clip.roots);
    const clones = structuredClone(clip.layers);
    // Inside its group again when it is still here (same template), else at the top level, at the same place on the stage.
    for (const c of clones) {
      if (roots.has(c.id)) {
        const keepParent = sameTemplate && !!c.parent && !!findLayer(tpl, c.parent);
        if (!keepParent) {
          const o = clip.offsets[c.id] ?? { x: 0, y: 0 };
          shift(c, o.x, o.y);
          c.parent = null;
        }
      } else c.parent = map.get(c.parent!) ?? null;
    }
    // A little apart from a layer at the same place (pasting twice: further each time).
    for (const c of clones) {
      if (!roots.has(c.id)) continue;
      const at = (l: Layer) => [num(propValue(l, 'x', frame)), num(propValue(l, 'y', frame))];
      for (let i = 0; i < 50; i++) {
        const [x, y] = at(c);
        if (!tpl.layers.some((l) => (l.parent ?? null) === (c.parent ?? null) && Math.abs(num(propValue(l, 'x', frame)) - x) < 0.5 && Math.abs(num(propValue(l, 'y', frame)) - y) < 0.5)) break;
        shift(c, STEP, STEP);
      }
      if (sameTemplate && tpl.layers.some((l) => l.name === c.name)) c.name = t('{name} copy', { name: c.name });
    }
    // Data: the variables of another template come along when this one lacks them; keys of data sets it does not have are let go.
    if (!sameTemplate) {
      const known = (k: string) => tpl.fields.some((f) => f.key === k) || (tpl.datasets ?? []).some((d) => k.startsWith(`${d.key}_`));
      for (const k of keysOf(clones)) {
        if (known(k)) continue;
        const f = clip.fields.find((x) => x.key === k);
        if (!f) {
          dropped.add(k);
          continue;
        }
        const copy = structuredClone(f);
        // A table read through the Data canvas is not copied with it; a source of another project stays there.
        const src = copy.binding?.source;
        if (copy.binding && (copy.binding.list !== undefined || (!sameProject && !sources.some((s) => s.id === src && (!s.project || s.project === projectId))))) delete copy.binding;
        tpl.fields.push(copy);
        added.push(copy.label || copy.key);
      }
      if (dropped.size) forgetKeys({ layers: clones } as Template, (k) => dropped.has(k));
    }
    for (const c of clones) c.id = map.get(c.id)!;
    // Paint order: right above the original when it is here, else on top.
    for (const r of clip.roots) {
      const sub = new Set([r, ...clip.layers.filter((l) => isUnder(clip.layers, l, r)).map((l) => l.id)]);
      const part = clones.filter((c) => [...sub].some((id) => map.get(id) === c.id));
      const origin = sameTemplate ? [r, ...descendants(tpl, r)].map((id) => tpl.layers.findIndex((l) => l.id === id)).filter((i) => i >= 0) : [];
      if (origin.length) tpl.layers.splice(Math.max(...origin) + 1, 0, ...part);
      else tpl.layers.push(...part);
      newRoots.push(map.get(r)!);
    }
  });
  if (!newRoots.length) return;
  st.select(newRoots);
  const notes = [tn(newRoots.length, '{n} layer pasted', '{n} layers pasted')];
  if (added.length) notes.push(tn(added.length, 'variable added: {names}', 'variables added: {names}', { names: added.join(', ') }));
  if (dropped.size) notes.push(t('data not in this template, unbound: {names}', { names: [...dropped].join(', ') }));
  toast(notes.join(' · '));
}

function isUnder(layers: Layer[], l: Layer, root: string) {
  const seen = new Set<string>();
  for (let p = l.parent; p && !seen.has(p); p = layers.find((x) => x.id === p)?.parent) {
    if (p === root) return true;
    seen.add(p);
  }
  return false;
}

/** Ctrl+V: the last thing copied — layers, or an animation onto the selected layers. */
export function pasteClipboard() {
  const c = useClipboard.getState();
  const st = useDesigner.getState();
  if (c.last === 'animation' && c.animation) {
    if (st.selected.length) return pasteAnimation(st.selected);
    if (!c.layers) return toast(t('Select the layers to paste the animation on'), 'error');
  }
  // « Copy all »: what to paste is chosen (PasteDialog).
  if (c.last === 'all' && c.all) {
    if (st.selected.length) return useClipboard.setState({ pasting: [...st.selected] });
    if (!c.layers) return toast(t('Select the layers to paste on'), 'error');
  }
  if (c.layers) pasteLayers();
}

/**
 * Erases the keyframes of layers (all their properties, or some), once confirmed. A property
 * left without keyframes keeps its value at the playhead (and stays on the timeline).
 */
export async function clearKeyframes(layerIds: string[], props: Prop[] | undefined, title: string) {
  const st = useDesigner.getState();
  const tpl = st.template;
  if (!tpl) return;
  const list: readonly Prop[] = props ?? PROPS;
  let n = 0;
  for (const id of layerIds) for (const p of list) n += findLayer(tpl, id)?.tracks[p]?.length ?? 0;
  if (!n || !(await confirmDialog({ title, message: tn(n, '{n} keyframe is erased.', '{n} keyframes are erased.'), confirm: t('Erase keyframes'), danger: true }))) return;
  const frame = useDesigner.getState().frame;
  st.update((d) => {
    for (const id of layerIds) {
      const l = findLayer(d, id);
      if (!l) continue;
      for (const p of list)
        if (l.tracks[p]?.length) {
          l.props[p] = propValue(l, p, frame);
          delete l.tracks[p];
        }
    }
  });
  st.selectKeys(useDesigner.getState().selectedKeys.filter((k) => !layerIds.includes(k.layerId) || !list.includes(k.prop)));
}

/** Erases the selected keyframes (Del, or once confirmed from the menu). */
export async function clearSelectedKeys(ask: boolean) {
  const st = useDesigner.getState();
  const keys = st.selectedKeys;
  if (!keys.length) return;
  if (ask && !(await confirmDialog({ title: tn(keys.length, 'Erase the selected keyframe?', 'Erase the {n} selected keyframes?'), confirm: t('Erase keyframes'), danger: true }))) return;
  const frame = st.frame;
  st.update((d) => {
    for (const k of keys) {
      const l = findLayer(d, k.layerId);
      const tr = l?.tracks[k.prop];
      if (!l || !tr) continue;
      const remaining = tr.filter((x) => x.frame !== k.frame);
      if (remaining.length) l.tracks[k.prop] = remaining;
      else {
        l.props[k.prop] = propValue(l, k.prop, frame);
        delete l.tracks[k.prop];
      }
    }
  });
  st.selectKeys([]);
}

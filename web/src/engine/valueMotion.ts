// Values a data source changed on air, moving in the layers showing them
// (mirror of cg-core playout::Change, count_value, shown_data and of the
// renderer's value_tf): a count, a slide, a turn over.
import { createContext } from 'react';
import type { Data, Layer, PlayoutItem, Template, ValueChange, ValueMove } from './types';

/** How far it is at `frame` frames since the change, 0 → 1, eased out. */
export const eased = (m: ValueMove, frame = m.frame) => {
  const k = Math.min(1, Math.max(0, frame / Math.max(1, m.length)));
  return 1 - (1 - k) ** 3;
};

function number(s: string): [number, number, boolean] | null {
  const t = s.trim().replace(/[\s  ]/g, '');
  if (!t || !/^[-+0-9.,]+$/.test(t)) return null;
  const comma = t.includes(',') && !t.includes('.');
  const v = Number(t.replace(',', '.'));
  if (!Number.isFinite(v)) return null;
  const at = Math.max(t.lastIndexOf('.'), t.lastIndexOf(','));
  return [v, at >= 0 ? t.length - at - 1 : 0, comma];
}

/** What a value counting from `from` to `to` shows `k` of the way, written as `to` is. */
export function countValue(from: string, to: string, k: number): string | null {
  const a = number(from);
  const b = number(to);
  if (!a || !b) return null;
  if (k >= 1) return to;
  const p = 10 ** b[1];
  const v = (Math.round((a[0] + (b[0] - a[0]) * Math.min(1, Math.max(0, k))) * p) / p).toFixed(b[1]);
  return b[2] ? v.replace('.', ',') : v;
}

/** The moves of an item `since` frames after its state was sent. */
export const movesAt = (item: PlayoutItem, since: number): ValueMove[] => (item.changes ?? []).map((m) => ({ ...m, frame: m.frame + since })).filter((m) => m.frame < m.length);

/** The data shown: the counting values on their way. */
export function shownData(data: Data, moves: ValueMove[]): Data {
  const counts = moves.filter((m) => m.kind === 'count');
  if (!counts.length) return data;
  const out = { ...data };
  for (const m of counts) {
    const v = data[m.key] !== undefined ? countValue(m.from, data[m.key], eased(m)) : null;
    if (v !== null) out[m.key] = v;
  }
  return out;
}

/** The layers showing a value sliding or turning over, by id: how and how far. */
export type ValueMotion = { layers: Map<string, [ValueChange, number]>; old: boolean } | null;
export const ValueMotionContext = createContext<ValueMotion>(null);

/** Whether a layer (as JSON) shows the data value `key`: in a text ({{key}}) or bound to it (mirror of playout::layer_shows). */
export const layerShows = (json: string, key: string) => json.includes(`{{${key}}}`) || json.includes(`"${key}"`);

/** A layer as JSON, to look for the data keys it shows: without its own settings by key (mirror of playout::layer_json). */
export const layerJson = (l: Layer) => JSON.stringify(l.liveChanges ? { ...l, liveChanges: undefined } : l);

/** Whether a data key comes from a source or a data set (its value can change on air). */
export const fedKey = (t: Template, key: string) => t.fields.some((f) => f.key === key && f.binding) || (t.datasets ?? []).some((d) => key.startsWith(`${d.key}_`));

/** The layers (not groups) showing the sliding or turning values: how and how far. */
export function movingLayers(t: Template, moves: ValueMove[]): Map<string, [ValueChange, number]> {
  const out = new Map<string, [ValueChange, number]>();
  const turning = moves.filter((m) => m.kind === 'slide' || m.kind === 'flip');
  if (!turning.length) return out;
  for (const l of t.layers as Layer[]) {
    if (l.type === 'group') continue;
    const json = layerJson(l);
    const m = turning.find((c) => (c.layer ? c.layer === l.id : layerShows(json, c.key)));
    if (m) out.set(l.id, [m.kind, eased(m)]);
  }
  return out;
}

/** The data with the old values of the sliding or turning ones. */
export function oldData(data: Data, moves: ValueMove[]): Data {
  const out = { ...data };
  for (const m of moves) if (m.kind === 'slide' || m.kind === 'flip') out[m.key] = m.from;
  return out;
}

/** The CSS transform of a layer showing a moving value (`h` its height), null when edge on. */
export function valueTransform(kind: ValueChange, e: number, old: boolean, h: number): string | null {
  if (kind === 'slide') return `translateY(${old ? -e * h : (1 - e) * h}px)`;
  if (kind === 'flip') {
    const s = old ? 1 - 2 * e : 2 * e - 1;
    return s > 0.001 ? `scaleY(${s})` : null;
  }
  return '';
}

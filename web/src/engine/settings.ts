// Exposed settings (Layer.settingBindings): a setting of a layer set from a
// data field the Operator fills. Mirror of Template::with_settings.
import type { Data, Field, FieldKind, Layer, Template } from './types';

/** `path` in `o` (dotted, numbers index lists). */
export function readPath(o: unknown, path: string): unknown {
  return path.split('.').reduce<unknown>((a, k) => (a == null ? a : (a as Record<string, unknown>)[k]), o);
}

/** `raw` read as the type of `current` (number, true/false, text, JSON); undefined when it does not fit. */
export function parseSetting(raw: string, current: unknown): unknown {
  const t = raw.trim();
  if (typeof current === 'number') return Number.isFinite(parseFloat(t)) ? parseFloat(t) : undefined;
  if (typeof current === 'boolean') return ['true', '1', 'yes', 'on'].includes(t) ? true : ['false', '0', 'no', 'off', ''].includes(t) ? false : undefined;
  if (typeof current === 'string') return raw;
  if (current && typeof current === 'object') {
    try {
      return JSON.parse(t);
    } catch {
      return undefined;
    }
  }
  if (t !== '' && Number.isFinite(Number(t))) return Number(t);
  if (t === 'true' || t === 'false') return t === 'true';
  return raw;
}

/** The value written to a field for a setting (what the Operator sees and edits). */
export function formatSetting(v: unknown): string {
  if (v == null) return '';
  if (typeof v === 'object') return JSON.stringify(v);
  return String(v);
}

/** Field kind for a setting value. */
export function settingKind(v: unknown, path: string): FieldKind {
  if (typeof v === 'number') return 'number';
  if (typeof v === 'boolean') return 'boolean';
  if (typeof v === 'string' && /^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(v)) return 'color';
  if (/(^|\.)(src|logo|image|fallback|sepImage)$/i.test(path)) return 'image';
  if (typeof v === 'string' && v.includes('\n')) return 'multiline';
  return 'text';
}

function writePath(o: Record<string, unknown>, path: string, v: unknown) {
  const ks = path.split('.');
  let a = o;
  for (let i = 0; i < ks.length - 1; i++) {
    if (a[ks[i]] == null || typeof a[ks[i]] !== 'object') a[ks[i]] = /^\d+$/.test(ks[i + 1]) ? [] : {};
    a = a[ks[i]] as Record<string, unknown>;
  }
  a[ks[ks.length - 1]] = v;
}

const cache = new WeakMap<Template, { data: Data; out: Template }>();

/** The template with its exposed settings set from `data`; the same object when none applies. */
export function withSettings(t: Template, data: Data): Template {
  const bound = (l: Layer) => Object.values(l.settingBindings ?? {}).some((k) => data[k] !== undefined);
  if (!t.layers.some(bound)) return t;
  const hit = cache.get(t);
  if (hit && hit.data === data) return hit.out;
  const out: Template = {
    ...t,
    layers: t.layers.map((l) => {
      if (!bound(l)) return l;
      const c = structuredClone(l) as unknown as Record<string, unknown>;
      for (const [path, key] of Object.entries(l.settingBindings!)) {
        if (data[key] === undefined) continue;
        const v = parseSetting(data[key], readPath(c, path));
        if (v !== undefined) writePath(c, path, v);
      }
      return c as unknown as Layer;
    }),
  };
  cache.set(t, { data, out });
  return out;
}

/** Keeps every exposed setting's field default equal to the Designer's value (the layer is the source; the Operator overrides it). */
export function syncSettingDefaults(t: Template) {
  for (const l of t.layers)
    for (const [path, key] of Object.entries(l.settingBindings ?? {})) {
      const f: Field | undefined = t.fields.find((x) => x.key === key);
      if (f && !f.binding?.source) {
        const v = formatSetting(readPath(l, path));
        if (f.default !== v) f.default = v;
      }
    }
}

/** Whether a field is still used by a layer (property, setting or a {{key}} in a text). */
export function fieldUsed(t: Template, key: string): boolean {
  return t.layers.some((l) => Object.values(l.bindings ?? {}).includes(key) || Object.values(l.settingBindings ?? {}).includes(key) || JSON.stringify(l).includes(`{{${key}}}`));
}

/** A field key not taken yet, from a label ("Name · Mask" → nameMask). */
export function newFieldKey(t: Template, base: string): string {
  let key =
    base
      .normalize('NFD')
      .replace(/[^A-Za-z0-9]+(.)?/g, (_, ch: string | undefined) => (ch ? ch.toUpperCase() : ''))
      .replace(/^./, (ch) => ch.toLowerCase()) || 'value';
  if (!/^[a-z]/i.test(key)) key = `v${key}`;
  const taken = new Set(t.fields.map((f) => f.key));
  for (let i = 2; taken.has(key); i++) key = key.replace(/\d*$/, String(i));
  return key;
}

/** The exposed settings' current values (field defaults, kept equal to the layers): the Designer's preview shows its own edits, not a value saved earlier. */
export function settingDefaults(t: Template | null | undefined): Data {
  const out: Data = {};
  if (!t) return out;
  for (const l of t.layers)
    for (const key of Object.values(l.settingBindings ?? {})) {
      const f = t.fields.find((x) => x.key === key);
      if (f && !f.binding?.source) out[key] = f.default;
    }
  return out;
}

/**
 * Takes every use of the keys `gone` matches out of the layers (bound properties,
 * settings, `{{key}}` in texts) and the logic: after a variable or a data set is
 * deleted, nothing still waits for a value that never comes.
 */
export function forgetKeys(t: Template, gone: (key: string) => boolean) {
  const strip = (o: unknown): unknown => {
    if (typeof o === 'string') {
      if (!o.includes('{{')) return o;
      const out = o.replace(/\{\{(\w+)\}\}/g, (m, k: string) => (gone(k) ? '' : m));
      return out === o ? o : out.replace(/ {2,}/g, ' ').trim();
    }
    if (Array.isArray(o)) return o.map(strip);
    if (o && typeof o === 'object') {
      const r = o as Record<string, unknown>;
      for (const k of Object.keys(r)) r[k] = strip(r[k]);
      return r;
    }
    return o;
  };
  for (const l of t.layers) {
    for (const [p, k] of Object.entries(l.bindings ?? {})) if (k && gone(k)) delete (l.bindings as Record<string, string>)[p];
    if (l.settingBindings) {
      for (const [p, k] of Object.entries(l.settingBindings)) if (gone(k)) delete l.settingBindings[p];
      if (!Object.keys(l.settingBindings).length) delete l.settingBindings;
    }
    const r = l as unknown as Record<string, unknown>;
    for (const k of Object.keys(r)) if (!['id', 'name', 'props', 'tracks', 'bindings', 'settingBindings'].includes(k)) r[k] = strip(r[k]);
  }
  if (t.logic) {
    const dead = new Set(t.logic.nodes.filter((n) => n.type === 'field' && gone(n.key)).map((n) => n.id));
    t.logic.nodes = t.logic.nodes.filter((n) => !dead.has(n.id));
    t.logic.edges = t.logic.edges.filter((e) => !dead.has(e.from));
  }
}

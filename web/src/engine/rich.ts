// Rich text: parts of a typed text in bold, italic, underlined, struck
// through or in another colour. Mirror of crates/cg-core/src/rich.rs.
//
//   <b>bold</b> <i>italic</i> <u>underlined</u> <s>struck</s> <c=#ff3b5c>red</c>
//
// A picture in the line: <img=media/flag.png|cap> (height of the capitals,
// `size` the font size, `line` the line's; width in proportion), its source
// maybe a {{key}}. In the shown text it is one character, IMG_BASE + its rank.
//
// Tags nest; anything else that looks like a tag stays text. A {{key}} takes
// the style around it. Only what people type is styled: the Designer's text
// and the values the operator types (`rich` keys); the values of the data
// sources never add styles.
import { createContext } from 'react';
import { interpolateText } from './evaluate';
import type { Data, Template, TextTransform } from './types';

export interface RunStyle {
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  /** #rrggbb[aa]; unset = the colour of the text. */
  color?: string;
  /** A variable's own looks (see VarStyle): font, weight, size and letter spacing (px, at the text's fontSize), above / below the line, case. */
  fontFamily?: string;
  fontWeight?: number;
  fontSize?: number;
  letterSpacing?: number;
  sup?: boolean;
  sub?: boolean;
  transform?: TextTransform;
}

/** The looks of a variable set in a text (`{{key}}`); unset: as the rest of the text. Mirror of wid::text::VarStyle. */
export interface VarStyle {
  fontFamily?: string;
  fontWeight?: number;
  fontSize?: number;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  sup?: boolean;
  sub?: boolean;
  letterSpacing?: number;
  transform?: TextTransform;
  color?: string;
}
export type VarStyles = Record<string, VarStyle>;

/** Measured otherwise than the rest of the text (another font, weight, size or spacing). */
export const metric = (s: RunStyle) => !!(s.bold || s.italic || s.sup || s.sub || s.fontFamily || s.fontWeight || s.fontSize != null || s.letterSpacing != null);

function withVar(s: RunStyle, v: VarStyle) {
  for (const k of ['bold', 'italic', 'underline', 'strike', 'sup', 'sub'] as const) if (v[k] != null) s[k] = v[k];
  if (v.color) s.color = v.color;
  if (v.fontFamily) s.fontFamily = v.fontFamily;
  if (v.fontWeight != null) s.fontWeight = v.fontWeight;
  if (v.fontSize != null) s.fontSize = v.fontSize;
  if (v.letterSpacing != null) s.letterSpacing = v.letterSpacing;
  if (v.transform) s.transform = v.transform;
}
export interface Run {
  text: string;
  style: RunStyle;
}

type Tag = { open: boolean; t: string; v?: string; len: number };

/** First character standing for a picture (the n-th is IMG_BASE + n), mirror of rich::IMG_BASE. */
export const IMG_BASE = 0x10f000;
/** The rank of the picture a character stands for, -1 for any other. */
export const imageIndex = (c: string) => {
  const v = c.codePointAt(0) ?? 0;
  return v >= IMG_BASE && v < IMG_BASE + 0xffd ? v - IMG_BASE : -1;
};
export type ImgHeight = 'cap' | 'size' | 'line';
/** A picture of the text: its source (data filled in) and its height. */
export interface InlineImage {
  src: string;
  height: ImgHeight;
}
/** Height in px of a picture of the text (mirror of ImgHeight::px). */
export const imgHeightPx = (h: ImgHeight, size: number, lineHeight: number) => (h === 'cap' ? size * 0.7 : h === 'line' ? size * lineHeight : size);
/** Middle of a picture of the text above the baseline (the capitals' middle). */
export const imgMiddle = (size: number) => size * 0.35;
/** `src|cap` → its source and height. */
export function imgValue(v: string): { src: string; height: ImgHeight } {
  const at = v.lastIndexOf('|');
  const h = at >= 0 ? v.slice(at + 1) : '';
  return h === 'cap' || h === 'size' || h === 'line' ? { src: v.slice(0, at), height: h } : { src: v, height: 'size' };
}
/** The markup of a picture of the text. */
export const imgTag = (src: string, height: ImgHeight) => `<img=${src}${height === 'size' ? '' : `|${height}`}>`;
const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

function tagAt(s: string, vars: VarStyles = {}): Tag | null {
  for (const t of ['b', 'i', 'u', 's']) if (s.startsWith(`<${t}>`)) return { open: true, t, len: 3 };
  const end = s.indexOf('>');
  if (end < 0) return null;
  const inner = s.slice(1, end);
  if (inner.startsWith('img=')) return inner.slice(4).trim() ? { open: true, t: 'img', v: inner.slice(4), len: end + 1 } : null;
  if (inner.startsWith('v=')) return vars[inner.slice(2)] ? { open: true, t: 'v', v: inner.slice(2), len: end + 1 } : null;
  if (inner === '/v' && Object.keys(vars).length) return { open: false, t: 'v', len: end + 1 };
  if (/^\/[biusc]$/.test(inner)) return { open: false, t: inner[1], len: end + 1 };
  if (inner.startsWith('c=') && HEX.test(inner.slice(2))) return { open: true, t: 'c', v: inner.slice(2), len: end + 1 };
  return null;
}

export const hasMarkup = (s: string) => {
  for (let i = s.indexOf('<'); i >= 0; i = s.indexOf('<', i + 1)) if (tagAt(s.slice(i))) return true;
  return false;
};

const same = (a: RunStyle, b: RunStyle) => JSON.stringify(a) === JSON.stringify(b);

/** `markup` with every variable of `vars` wrapped in `<v=key>…</v>` (mirror of rich::with_vars). */
function withVars(markup: string, vars: VarStyles): string {
  if (!Object.keys(vars).length) return markup;
  return markup.replace(/\{\{([^}]*)\}\}/g, (m, k: string) => (vars[k.trim()] ? `<v=${k.trim()}>${m}</v>` : m));
}

/** The parts of `markup` with their style. */
export function parse(markup: string, vars: VarStyles = {}): Run[] {
  const runs: Run[] = [];
  const stack: { t: string; v?: string }[] = [];
  const style = (): RunStyle => {
    const s: RunStyle = {};
    for (const { t, v } of stack) {
      if (t === 'b') s.bold = true;
      else if (t === 'i') s.italic = true;
      else if (t === 'u') s.underline = true;
      else if (t === 's') s.strike = true;
      else if (t === 'v') {
        if (v && vars[v]) withVar(s, vars[v]);
      } else s.color = v;
    }
    return s;
  };
  let cur = '';
  const flush = () => {
    if (!cur) return;
    const st = style();
    const last = runs[runs.length - 1];
    if (last && same(last.style, st)) last.text += cur;
    else runs.push({ text: cur, style: st });
    cur = '';
  };
  let rest = markup;
  let pictures = 0;
  for (let i = rest.indexOf('<'); i >= 0; i = rest.indexOf('<')) {
    cur += rest.slice(0, i);
    const at = rest.slice(i);
    const tag = tagAt(at, vars);
    if (!tag) {
      cur += '<';
      rest = at.slice(1);
      continue;
    }
    if (tag.t === 'img') {
      // One character as wide as the picture, in the style around it.
      cur += String.fromCodePoint(IMG_BASE + pictures++);
      rest = at.slice(tag.len);
      continue;
    }
    flush();
    if (tag.open) stack.push({ t: tag.t, v: tag.v });
    else {
      const p = stack.map((x) => x.t).lastIndexOf(tag.t);
      if (p >= 0) stack.splice(p, 1);
    }
    rest = at.slice(tag.len);
  }
  cur += rest;
  flush();
  return runs;
}

/** The text without its tags (nor its pictures). */
export const plain = (markup: string) => [...parse(markup).map((r) => r.text).join('')].filter((c) => imageIndex(c) < 0).join('');

/** `markup` with the values of the `rich` keys put in, as if typed in the text. */
const expand = (markup: string, data: Data, rich: (key: string) => boolean) => markup.replace(/\{\{([^}]*)\}\}/g, (m, k: string) => (rich(k.trim()) && data[k.trim()] !== undefined ? data[k.trim()] : m));

/** The picture tags of `markup` as written, in their order: where each is and its value (`src|height`). */
export function imageTags(markup: string): { start: number; end: number; value: string }[] {
  const out: { start: number; end: number; value: string }[] = [];
  for (let i = markup.indexOf('<'); i >= 0; i = markup.indexOf('<', i + 1)) {
    const tag = tagAt(markup.slice(i));
    if (tag?.t === 'img') out.push({ start: i, end: i + tag.len, value: tag.v! });
  }
  return out;
}

/** The pictures of `markup` in their order (mirror of rich::inline_images). */
export function inlineImages(markup: string, data: Data, rich: (key: string) => boolean): InlineImage[] {
  if (!markup.includes('<img=') && !markup.includes('{{')) return [];
  const s = expand(markup, data, rich);
  const out: InlineImage[] = [];
  for (let i = s.indexOf('<'); i >= 0; i = s.indexOf('<', i + 1)) {
    const tag = tagAt(s.slice(i));
    if (tag?.t !== 'img') continue;
    const { src, height } = imgValue(tag.v!);
    out.push({ src: interpolateText(src, data).trim(), height });
  }
  return out;
}

/** The parts as shown (mirror of rich::runs_with); null without styles. */
export function runs(markup: string, data: Data, rich: (key: string) => boolean, kase: (s: string) => string, vars: VarStyles = {}, ownCase: (t: TextTransform, s: string) => string = (_, s) => s): Run[] | null {
  // The variables in their own looks, then the operator's styled values, as if typed in the text.
  const expanded = expand(withVars(markup, vars), data, rich);
  if (!hasMarkup(expanded) && !(Object.keys(vars).length && expanded.includes('<v='))) return null;
  const out = parse(expanded, vars).map((r) => ({ text: interpolateText(r.text, data), style: r.style }));
  const typed = out.map((r) => r.text);
  const whole = typed.join('');
  const cased = kase(whole);
  if ([...cased].length === [...whole].length) {
    const chars = [...cased];
    let at = 0;
    for (const r of out) {
      const n = [...r.text].length;
      r.text = chars.slice(at, at + n).join('');
      at += n;
    }
  } else for (const r of out) r.text = kase(r.text);
  // A variable's own case, on what was typed.
  out.forEach((r, i) => {
    if (r.style.transform) r.text = ownCase(r.style.transform, typed[i]);
  });
  return out.filter((r) => r.text);
}

/** First private character standing for a character of a part measured in its own style (mirror of rich::RUN_BASE). */
export const RUN_BASE = 0xf0000;

/** The shown text for the line breaking: the characters of the parts measured in their own style stand as private characters (mirror of rich::encode). */
export function encodeRuns(rs: Run[]): { text: string; table: [string, number][] } {
  let text = '';
  const table: [string, number][] = [];
  rs.forEach((r, i) => {
    const own = metric(r.style);
    for (const c of r.text) {
      if (own && c !== ' ' && c !== '\n' && imageIndex(c) < 0) {
        text += String.fromCodePoint(RUN_BASE + table.length);
        table.push([c, i]);
      } else text += c;
    }
  });
  return { text, table };
}

/** The character `c` of an encoded text stands for, and its run. */
export function decodeChar(c: string, table: [string, number][]): [string, number] | null {
  const v = (c.codePointAt(0) ?? 0) - RUN_BASE;
  return v >= 0 && v < table.length ? table[v] : null;
}

/** An encoded line back to the shown characters. */
export const decodeLine = (line: string, table: [string, number][]) => (table.length ? [...line].map((c) => decodeChar(c, table)?.[0] ?? c).join('') : line);

const space = (c: string | undefined) => c === ' ' || c === ' ' || c === ' ' || c === '\n' || c === '\t';

/** The parts of each shown line, as [text, index of the run] (mirror of rich::align). */
export function align(rs: Run[], lines: string[]): [string, number][][] {
  const chars: [string, number][] = rs.flatMap((r, i) => [...r.text].map((c) => [c, i] as [string, number]));
  let at = 0;
  return lines.map((line) => {
    const parts: [string, number][] = [];
    for (const ch of line) {
      if (!space(ch)) {
        let j = at;
        while (j < chars.length && chars[j][0] !== ch && space(chars[j][0])) j++;
        if (j < chars.length && chars[j][0] === ch) at = j;
      }
      let run: number;
      if (at < chars.length && (chars[at][0] === ch || (space(ch) && space(chars[at][0])))) run = chars[at++][1];
      else run = chars[Math.max(0, at - 1)]?.[1] ?? 0;
      const last = parts[parts.length - 1];
      if (last && last[1] === run) last[0] += ch;
      else parts.push([ch, run]);
    }
    return parts;
  });
}

/** Keys whose values the operator types (they may have styles). */
export const typedKeys = (t: Pick<Template, 'fields'> | null | undefined) => {
  const keys = new Set((t?.fields ?? []).filter((f) => !f.binding).map((f) => f.key));
  return (k: string) => keys.has(k);
};

/** The template's typed keys, for the texts drawn inside it (set by Scene). */
export const RichKeys = createContext<(key: string) => boolean>(() => false);

/** Whether the operator can style the value of `key` (typed, and only shown in Texts). */
export function richField(t: Pick<Template, 'fields' | 'layers'>, key: string): boolean {
  const f = t.fields.find((x) => x.key === key);
  if (!f || f.binding || (f.kind !== 'text' && f.kind !== 'multiline')) return false;
  const token = `{{${key}}}`;
  const users = t.layers.filter((l) => JSON.stringify(l).includes(token));
  return users.length > 0 && users.every((l) => l.type === 'text' && l.text.includes(token) && !JSON.stringify({ ...l, text: '' }).includes(token));
}

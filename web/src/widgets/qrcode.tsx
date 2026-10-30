// QR code: settings, drawing, inspector and the readability check.
// Same picture as crates/cg-render/src/wid/qrcode.rs; structure / payload / geometry mirror crates/cg-core/src/wid/qrcode.rs.
// The matrix comes from the server (`/api/qr?data=…&ec=…`, the Rust `qrcode` crate): one encoder for both renderers.
import { useEffect, useReducer, useRef } from 'react';
import { ChevronDown, QrCode, SkipBack, SkipForward } from 'lucide-react';
import { Dropdown } from '../ui/Dropdown';
import { col, interpolateText, num, toHex, type Resolved } from '../engine/evaluate';
import type { Data } from '../engine/types';
import { AssetPicker, ColorGroupRow } from '../designer/Inspector';
import { FontPick } from '../ui/FontPick';
import { Area, ColorPick, Content, defineWidget, ListEditor, Num, Row, Section, Seg, Select, Toggle, useKeys, type Warning } from './kit';
import { Centre } from './centre';
import { t, tc, tn } from '../i18n';

type Ecl = 'L' | 'M' | 'Q' | 'H';
export interface Config {
  kind?: 'url' | 'text' | 'vcard' | 'wifi' | 'email' | 'sms';
  vcard?: { first?: string; last?: string; org?: string; title?: string; phone?: string; email?: string; url?: string };
  wifi?: { ssid?: string; password?: string; security?: 'WPA' | 'WEP' | 'nopass'; hidden?: boolean };
  email?: { to?: string; subject?: string; body?: string };
  sms?: { number?: string; message?: string };
  ecl?: Ecl;
  transparent?: boolean;
  backdrop?: string;
  radius?: number;
  modStyle?: 'square' | 'rounded' | 'dots';
  finderStyle?: 'square' | 'rounded' | 'circle';
  finderSame?: boolean;
  finderColor?: string;
  logo?: { on?: boolean; src?: string; size?: number };
  quiet?: number;
  minPx?: number;
  label?: { on?: boolean; text?: string; size?: number; color?: string; bg?: string; pos?: 'below' | 'beside' | 'above'; font?: string; weight?: number; upper?: boolean };
  /** URL type: addresses prepared for the Operator (none = the layer's `data`). */
  addresses?: Address[];
  /** The Operator gets the list of the prepared addresses (the centre's row, when exposed; the arrows stay). */
  opAddress?: boolean;
}
export interface Address {
  name?: string;
  url?: string;
  /** Its own caption (empty = the caption's text). */
  caption?: string;
}
/** The prepared address on air (mirrors `State` in Rust). */
export interface State {
  addr?: number;
}

type QrLayer = Config & { id?: string; data: string; background: string; props: Record<string, unknown> };

function cfg(c: Config) {
  return {
    kind: c.kind ?? 'url',
    vcard: { first: '', last: '', org: '', title: '', phone: '', email: '', url: '', ...c.vcard },
    wifi: { ssid: '', password: '', security: 'WPA' as const, hidden: false, ...c.wifi },
    email: { to: '', subject: '', body: '', ...c.email },
    sms: { number: '', message: '', ...c.sms },
    ecl: (c.ecl ?? 'M') as Ecl,
    transparent: c.transparent ?? false,
    backdrop: c.backdrop ?? '#12151e',
    radius: c.radius ?? 0,
    modStyle: c.modStyle ?? 'square',
    finderStyle: c.finderStyle ?? 'square',
    finderSame: c.finderSame ?? true,
    finderColor: c.finderColor ?? '#7c5cff',
    logo: { on: false, src: '', size: 22, ...c.logo },
    quiet: c.quiet ?? 4,
    minPx: c.minPx ?? 6,
    label: { on: false, text: 'Scan to vote', size: 34, color: '#ffffff', bg: '#7c5cff', pos: 'below' as 'below' | 'beside' | 'above', font: 'Inter', weight: 800, upper: false, ...c.label },
    addresses: (c.addresses ?? []).map((a) => ({ name: '', url: '', caption: '', ...a })),
    opAddress: c.opAddress ?? true,
  };
}
type Cfg = ReturnType<typeof cfg>;

/** The URL on air and its caption: the prepared address chosen, else the layer's `data` (mirrors `address`). */
export function address(c: Cfg, dataField: string, st: State | undefined): [string, string] {
  if (c.kind === 'url' && c.addresses.length) {
    const a = c.addresses[Math.min(st?.addr ?? 0, c.addresses.length - 1)];
    return [a.url, a.caption.trim() ? a.caption : c.label.text];
  }
  return [dataField, c.label.text];
}
const capText = (c: Cfg, s: string) => (c.label.upper ? s.toUpperCase() : s);

const ORD: Record<Ecl, number> = { L: 0, M: 1, Q: 2, H: 3 };
const LOGO_MAX: Record<Ecl, number> = { L: 12, M: 18, Q: 24, H: 30 };
const EC_BITS: Record<Ecl, number> = { L: 1, M: 0, Q: 3, H: 2 };

// ---------------------------------------------------------------- payload --

const wifiEsc = (v: string) => v.replace(/([\\;,:"])/g, '\\$1');
export function payload(c: Cfg, dataField: string, data: Data): string {
  const i = (s: string) => interpolateText(s ?? '', data);
  switch (c.kind) {
    case 'vcard': {
      const v = c.vcard;
      const [first, last] = [i(v.first), i(v.last)];
      const L = ['BEGIN:VCARD', 'VERSION:3.0', `N:${last};${first}`, `FN:${[first, last].filter(Boolean).join(' ')}`];
      for (const [k, val] of [
        ['ORG', v.org],
        ['TITLE', v.title],
        ['TEL;TYPE=CELL', v.phone],
        ['EMAIL', v.email],
        ['URL', v.url],
      ] as const)
        if (i(val)) L.push(`${k}:${i(val)}`);
      L.push('END:VCARD');
      return L.join('\n');
    }
    case 'wifi': {
      const w = c.wifi;
      return `WIFI:T:${w.security};S:${wifiEsc(i(w.ssid))};${w.security === 'nopass' ? '' : `P:${wifiEsc(i(w.password))};`}${w.hidden ? 'H:true;' : ''};`;
    }
    case 'email': {
      const m = c.email;
      const q = [i(m.subject) && `subject=${encodeURIComponent(i(m.subject))}`, i(m.body) && `body=${encodeURIComponent(i(m.body))}`].filter(Boolean).join('&');
      return `mailto:${i(m.to)}${q ? '?' + q : ''}`;
    }
    case 'sms':
      return `SMSTO:${i(c.sms.number)}:${i(c.sms.message)}`;
    default:
      return i(dataField);
  }
}

// ----------------------------------------------------------------- matrix --

type Matrix = { size: number; mods: boolean[] };
const cache = new Map<string, Matrix | null | Promise<void>>();
const listeners = new Set<() => void>();
function matrix(content: string, ec: Ecl): Matrix | null | undefined {
  const key = `${ec}|${content}`;
  const hit = cache.get(key);
  if (hit === undefined) {
    cache.set(
      key,
      fetch(`/api/qr?data=${encodeURIComponent(content)}&ec=${ec}`)
        .then((r) => (r.ok ? (r.json() as Promise<{ size: number; modules: string }>) : null))
        .catch(() => null)
        .then((q) => {
          cache.set(key, q ? { size: q.size, mods: [...q.modules].map((ch) => ch === '1') } : null);
          listeners.forEach((f) => f());
        }),
    );
    return undefined;
  }
  return hit instanceof Promise ? undefined : hit;
}
function useMatrix(content: string, ec: Ecl) {
  const [, rerender] = useReducer((x: number) => x + 1, 0);
  useEffect(() => {
    listeners.add(rerender);
    return () => void listeners.delete(rerender);
  }, []);
  return content ? matrix(content, ec) : null;
}

// ----------------------------------------------------- symbol structure --

const ECC_PER_BLOCK = [
  [-1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  [-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28],
  [-1, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  [-1, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
];
const NUM_BLOCKS = [
  [-1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
  [-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49],
  [-1, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29, 34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68],
  [-1, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81],
];
function rawModules(v: number): number {
  let r = (16 * v + 128) * v + 64;
  if (v >= 2) {
    const na = Math.floor(v / 7) + 2;
    r -= (25 * na - 10) * na - 55;
    if (v >= 7) r -= 36;
  }
  return r;
}
function alignPos(v: number): number[] {
  if (v === 1) return [];
  const n = Math.floor(v / 7) + 2;
  const step = v === 32 ? 26 : Math.ceil((v * 4 + 4) / (n * 2 - 2)) * 2;
  const r = [6];
  for (let p = v * 4 + 17 - 7; r.length < n; p -= step) r.splice(1, 0, p);
  return r;
}
interface Structure {
  size: number;
  version: number;
  func: boolean[];
  cw: number[];
  blockOf: number[];
  blocks: number;
  eccLen: number;
}
/** Mirrors `structure()` of the Rust module. */
function structure(size: number, ecl: Ecl): Structure | null {
  if (size < 21 || (size - 17) % 4) return null;
  const v = (size - 17) / 4;
  if (v > 40) return null;
  const e = ORD[ecl];
  const func = new Array(size * size).fill(false);
  const set = (x: number, y: number) => (func[y * size + x] = true);
  for (let i = 0; i < size; i++) (set(6, i), set(i, 6));
  for (const [cx, cy] of [
    [3, 3],
    [size - 4, 3],
    [3, size - 4],
  ])
    for (let dy = -4; dy <= 4; dy++)
      for (let dx = -4; dx <= 4; dx++) {
        const x = cx + dx;
        const y = cy + dy;
        if (x >= 0 && y >= 0 && x < size && y < size) set(x, y);
      }
  const al = alignPos(v);
  const k = al.length;
  al.forEach((ay, i) =>
    al.forEach((ax, j) => {
      if ((i === 0 && j === 0) || (i === 0 && j === k - 1) || (i === k - 1 && j === 0)) return;
      for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) set(ax + dx, ay + dy);
    }),
  );
  for (let i = 0; i < 9; i++) (set(8, i), set(i, 8));
  for (let i = 0; i < 8; i++) (set(size - 1 - i, 8), set(8, size - 1 - i));
  if (v >= 7)
    for (let i = 0; i < 18; i++) {
      const a = size - 11 + (i % 3);
      const b = Math.floor(i / 3);
      (set(a, b), set(b, a));
    }
  const raw = Math.floor(rawModules(v) / 8);
  const nb = NUM_BLOCKS[e][v];
  const eccLen = ECC_PER_BLOCK[e][v];
  const nShort = nb - (raw % nb);
  const shortLen = Math.floor(raw / nb);
  const blockOf: number[] = [];
  for (let i = 0; i <= shortLen; i++) for (let j = 0; j < nb; j++) if (i !== shortLen - eccLen || j >= nShort) blockOf.push(j);
  const cw = new Array(size * size).fill(-1);
  let bit = 0;
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vert = 0; vert < size; vert++)
      for (let j = 0; j < 2; j++) {
        const x = right - j;
        const up = ((right + 1) & 2) === 0;
        const y = up ? size - 1 - vert : vert;
        if (!func[y * size + x] && bit < raw * 8) ((cw[y * size + x] = bit >> 3), bit++);
      }
  }
  return { size, version: v, func, cw, blockOf, blocks: nb, eccLen };
}
function logoRect(size: number, pct: number): [number, number, number, number] | null {
  if (!pct) return null;
  const side = Math.ceil((size * pct) / 100);
  const plate = Math.min(size, side + 2);
  const a = Math.floor((size - plate) / 2);
  return [a, a, plate, side];
}
function damage(s: Structure, pct: number) {
  const d = { fnHit: false, worst: 0, cap: Math.floor(s.eccLen / 2), modules: 0, cws: 0 };
  const r = logoRect(s.size, pct);
  if (!r) return d;
  const hit = new Set<number>();
  for (let y = r[1]; y < Math.min(s.size, r[1] + r[2]); y++)
    for (let x = r[0]; x < Math.min(s.size, r[0] + r[2]); x++) {
      d.modules++;
      const k = y * s.size + x;
      if (s.func[k]) d.fnHit = true;
      else if (s.cw[k] >= 0) hit.add(s.cw[k]);
    }
  const per = new Array(s.blocks).fill(0);
  hit.forEach((i) => s.blockOf[i] != null && per[s.blockOf[i]]++);
  d.worst = Math.max(0, ...per);
  d.cws = hit.size;
  return d;
}
function logoSize(c: Cfg, s: Structure | null): [number, string] {
  // No picture: no logo (never a placeholder on air).
  if (!s || !c.logo.on || !c.logo.src.trim()) return [0, ''];
  let pct = Math.min(c.logo.size, LOGO_MAX[c.ecl]);
  let by = c.logo.size > LOGO_MAX[c.ecl] ? 'level' : '';
  while (pct > 0) {
    const d = damage(s, pct);
    if (!d.fnHit && d.worst <= Math.floor((d.cap * 85) / 100)) break;
    pct--;
    by = 'budget';
  }
  return [pct, by];
}
const roundJs = (v: number) => Math.floor(v + 0.5);
/** Approximate width of a caption (layout only, the same in both renderers). */
const capW = (s: string, size: number, up: boolean) => [...s].length * size * (up ? 0.68 : 0.6);
function capLines(s: string, max: number, size: number, up: boolean): string[] {
  const words = s.split(' ').filter(Boolean);
  if (capW(s, size, up) <= max || words.length < 2) return [s];
  let best: [number, number] = [Infinity, 1];
  for (let i = 1; i < words.length; i++) {
    const m = Math.max(capW(words.slice(0, i).join(' '), size, up), capW(words.slice(i).join(' '), size, up));
    if (m < best[0]) best = [m, i];
  }
  return [words.slice(0, best[1]).join(' '), words.slice(best[1]).join(' ')];
}
/** Code and caption pill in the box (mirrors `geo`); `label`: the caption as drawn. */
function geo(c: Cfg, label: string, n: number, w: number, h: number) {
  const on = c.label.on && !!label;
  const size = c.label.size;
  const up = c.label.upper;
  const total = n + 2 * c.quiet;
  if (on && c.label.pos === 'beside') {
    const maxW = roundJs(w * 0.4);
    const gap = 18;
    const avail = Math.min(w - maxW - gap, h);
    const m = Math.max(1, Math.floor(avail / total));
    const px = m * total;
    const lines = capLines(label, maxW - size * 1.6, size, up);
    const widest = Math.max(0, ...lines.map((l) => capW(l, size, up)));
    const labW = Math.round(Math.min(maxW, widest + size * 1.6));
    const lab = roundJs(lines.length * size * 1.25 + size * 0.75);
    const x0 = Math.max(0, roundJs((w - px - gap - labW) / 2));
    const y0 = Math.max(0, roundJs((h - px) / 2));
    return { m, px, x0, y0, lab, labX: x0 + px + gap, labY: roundJs(y0 + px / 2 - lab / 2), labW, labR: Math.min(lab / 2, size * 0.7), lines };
  }
  const lab = on ? roundJs(size * 1.9) : 0;
  const gap = lab ? 18 : 0;
  const avail = Math.min(w, h - lab - gap);
  const m = Math.max(1, Math.floor(avail / total));
  const px = m * total;
  const x0 = roundJs((w - px) / 2);
  const top = Math.max(0, roundJs((h - lab - gap - px) / 2));
  const above = c.label.pos === 'above';
  const labW = Math.round(Math.min(w, Math.max(px * 0.7, capW(label, size, up) + 60)));
  return { m, px, x0, y0: above ? top + lab + gap : top, lab, labX: Math.floor((w - labW) / 2 + 0.5), labY: above ? top : top + px + gap, labW, labR: lab / 2, lines: on ? [label] : [] };
}

// --------------------------------------------------------- self-decode --

const gfMul = (x: number, y: number) => {
  let z = 0;
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ ((z >>> 7) * 0x11d);
    z ^= ((y >>> i) & 1) * x;
  }
  return z & 255;
};
const MASKS = [
  (x: number, y: number) => (x + y) % 2 === 0,
  (_: number, y: number) => y % 2 === 0,
  (x: number) => x % 3 === 0,
  (x: number, y: number) => (x + y) % 3 === 0,
  (x: number, y: number) => (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0,
  (x: number, y: number) => ((x * y) % 2) + ((x * y) % 3) === 0,
  (x: number, y: number) => (((x * y) % 2) + ((x * y) % 3)) % 2 === 0,
  (x: number, y: number) => (((x + y) % 2) + ((x * y) % 3)) % 2 === 0,
];
/** Reads the matrix back: format bits (level, mask), unmasking, Reed-Solomon check of every block. */
function selfDecode(q: Matrix, s: Structure | null): { ok: boolean; why?: string; ecl?: Ecl; mask?: number; blocks?: number } {
  const n = q.size;
  const rd = (x: number, y: number) => (q.mods[y * n + x] ? 1 : 0);
  let fb = 0;
  for (let i = 0; i <= 5; i++) fb |= rd(8, i) << i;
  fb |= (rd(8, 7) << 6) | (rd(8, 8) << 7) | (rd(7, 8) << 8);
  for (let i = 9; i < 15; i++) fb |= rd(14 - i, 8) << i;
  fb ^= 0x5412;
  const mask = (fb >>> 10) & 7;
  const ecl = (Object.keys(EC_BITS) as Ecl[]).find((k) => EC_BITS[k] === fb >>> 13);
  if (!ecl) return { ok: false, why: 'format bits' };
  const S = s && s.size === n && s.blocks === NUM_BLOCKS[ORD[ecl]][s.version] && s.eccLen === ECC_PER_BLOCK[ORD[ecl]][s.version] ? s : structure(n, ecl);
  if (!S) return { ok: false, why: 'size' };
  const total = S.blockOf.length;
  const read = new Array(total).fill(0);
  const pos = new Array(total).fill(0);
  for (let right = n - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vert = 0; vert < n; vert++)
      for (let j = 0; j < 2; j++) {
        const x = right - j;
        const up = ((right + 1) & 2) === 0;
        const y = up ? n - 1 - vert : vert;
        const k = S.cw[y * n + x];
        if (k < 0) continue;
        if (q.mods[y * n + x] !== MASKS[mask](x, y)) read[k] |= 1 << (7 - pos[k]);
        pos[k]++;
      }
  }
  const deg = S.eccLen;
  const div = new Array(deg).fill(0);
  div[deg - 1] = 1;
  let root = 1;
  for (let i = 0; i < deg; i++) {
    for (let j = 0; j < deg; j++) {
      div[j] = gfMul(div[j], root);
      if (j + 1 < deg) div[j] ^= div[j + 1];
    }
    root = gfMul(root, 2);
  }
  const blocks: number[][] = Array.from({ length: S.blocks }, () => []);
  read.forEach((b, i) => blocks[S.blockOf[i]].push(b));
  for (const b of blocks) {
    const r = new Array(deg).fill(0);
    for (const x of b.slice(0, b.length - deg)) {
      const f = x ^ r.shift();
      r.push(0);
      div.forEach((c, i) => (r[i] ^= gfMul(c, f)));
    }
    const ecc = b.slice(b.length - deg);
    if (r.some((v, i) => v !== ecc[i])) return { ok: false, why: 'Reed-Solomon check', ecl, mask };
  }
  return { ok: true, ecl, mask, blocks: S.blocks };
}

// ------------------------------------------------------------ the check --

function lum(hex: string): number | null {
  const m = /^#?([0-9a-f]{6})/i.exec(String(hex || '').trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  const ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
    v /= 255;
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
}
function contrast(a: string, b: string): number | null {
  const la = lum(a);
  const lb = lum(b);
  if (la == null || lb == null) return null;
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

interface Check {
  ready: boolean;
  ok: boolean;
  issues: Warning[];
  payload: string;
  q?: Matrix | null;
  m?: number;
  screenPx?: number;
  px?: number;
  logo?: [number, string];
  dmg?: ReturnType<typeof damage>;
  cr?: number | null;
  decode?: ReturnType<typeof selfDecode>;
}
function check(layer: QrLayer, data: Data, st: State | undefined, r: Resolved | undefined, q: Matrix | null | undefined): Check {
  const c = cfg(layer);
  const [url, caption] = address(c, layer.data, st);
  const text = payload(c, url, data);
  const issues: Warning[] = [];
  if (!text) return { ready: true, ok: false, issues: [{ level: 'error', text: t('Empty content: nothing to encode (nothing on air).'), op: true }], payload: text };
  if (q === undefined) return { ready: false, ok: true, issues, payload: text };
  if (q === null) {
    const n = new TextEncoder().encode(text).length;
    // The server takes 2 048 bytes at most (crates/cg-server/src/api.rs).
    const msg = n > 2048 ? t('Too long: {n} bytes, the server encodes 2,048 at most.', { n }) : t('Too long: {n} bytes do not fit even in version 40 at level {level}.', { n, level: c.ecl });
    return { ready: true, ok: false, issues: [{ level: 'error', text: msg, op: true }], payload: text };
  }
  const w = num((r?.width ?? layer.props.width ?? 520) as number);
  const h = num((r?.height ?? layer.props.height ?? 640) as number);
  const sc = Math.min(Math.abs(num((r?.scaleX ?? layer.props.scaleX ?? 1) as number)), Math.abs(num((r?.scaleY ?? layer.props.scaleY ?? 1) as number)));
  const g = geo(c, capText(c, interpolateText(caption, data)), q.size, w, h);
  const s = structure(q.size, c.ecl);
  const logo = logoSize(c, s);
  const dmg = s ? damage(s, logo[0]) : undefined;
  const fill = toHex(r ? col(r.fill) : ((layer.props.fill as [number, number, number, number]) ?? [1, 1, 1, 1]));
  const back = c.transparent ? c.backdrop : layer.background;
  const cr = contrast(fill, back);
  const lm = lum(fill);
  const lb = lum(back);
  const dec = selfDecode(q, s);
  const screenPx = Math.floor(g.m * sc * 10) / 10;
  if (!dec.ok) issues.push({ level: 'error', text: t('Self-decode failed ({why}).', { why: t(dec.why ?? '') }), op: true });
  if (screenPx < c.minPx) issues.push({ level: 'error', text: t('Module {px} px on screen, under the {min} px minimum: shorten the content, lower the correction or enlarge the box.', { px: screenPx, min: c.minPx }), op: true });
  if (cr == null) issues.push({ level: 'warn', text: t('A colour is not a #rrggbb value: contrast not checked.') });
  else if (cr < 3) issues.push({ level: 'error', text: t('Contrast {ratio}:1 is too low (needs 3:1, 7:1 recommended).', { ratio: cr.toFixed(1) }), op: true });
  else if (cr < 7) issues.push({ level: 'warn', text: t('Contrast {ratio}:1 is weak: 7:1 or more is safer on a TV picture.', { ratio: cr.toFixed(1) }) });
  if (lm != null && lb != null && lm > lb) issues.push({ level: 'warn', text: t('Light code on a dark background (inverted): many phone readers do not decode it.'), op: true });
  if (c.quiet < 2) issues.push({ level: 'error', text: tn(c.quiet, 'Quiet zone {n} module: readers cannot find the code edge (4 recommended, 2 minimum).', 'Quiet zone {n} modules: readers cannot find the code edge (4 recommended, 2 minimum).'), op: true });
  else if (c.quiet < 4) issues.push({ level: 'warn', text: t('Quiet zone reduced to {n} modules (4 is the standard): keep the area around the code plain.', { n: c.quiet }) });
  if (c.transparent && c.quiet > 0) issues.push({ level: 'info', text: t('Transparent background: the check assumes the backdrop colour {color} behind the code.', { color: c.backdrop }) });
  if (c.logo.on && !c.logo.src.trim()) issues.push({ level: 'info', text: t('Logo on without a picture: nothing is drawn.') });
  else if (c.logo.on && logo[0] < c.logo.size) issues.push({ level: 'info', text: logo[1] === 'level' ? t('Logo capped at {pct} % (asked {asked} %): level {level} allows {max} % at most.', { pct: logo[0], asked: c.logo.size, level: c.ecl, max: LOGO_MAX[c.ecl] }) : t('Logo capped at {pct} % (asked {asked} %): more would hide more codewords than the correction can repair.', { pct: logo[0], asked: c.logo.size }) });
  if (c.logo.on && c.logo.src.trim() && c.ecl !== 'H') issues.push({ level: 'info', text: t('A logo hides modules: correction H is recommended (now {level}).', { level: c.ecl }) });
  return { ready: true, ok: !issues.some((i) => i.level === 'error'), issues, payload: text, q, m: g.m, screenPx, px: g.px, logo, dmg, cr, decode: dec };
}

// ----------------------------------------------------------------- render --

function Svg({ layer, r, w, h, data, q, caption }: { layer: QrLayer; r: Resolved; w: number; h: number; data: Data; q: Matrix; caption: string }) {
  const c = cfg(layer);
  const n = q.size;
  const label = capText(c, interpolateText(caption, data));
  const g = geo(c, label, n, w, h);
  const m = g.m;
  const s = structure(n, c.ecl);
  const [pct] = logoSize(c, s);
  const rect = logoRect(n, pct);
  const ox = g.x0 + c.quiet * m;
  const oy = g.y0 + c.quiet * m;
  const inFinder = (x: number, y: number) => (x < 7 && y < 7) || (x >= n - 7 && y < 7) || (x < 7 && y >= n - 7);
  const hidden = (x: number, y: number) => !!rect && x >= rect[0] && x < rect[0] + rect[2] && y >= rect[1] && y < rect[1] + rect[2];
  const dark = (x: number, y: number) => x >= 0 && y >= 0 && x < n && y < n && q.mods[y * n + x] && !hidden(x, y);
  const k = 0.5523;
  let d = '';
  for (let y = 0; y < n; y++)
    for (let x = 0; x < n; x++) {
      if (!dark(x, y) || inFinder(x, y)) continue;
      const X = ox + x * m;
      const Y = oy + y * m;
      if (c.modStyle === 'dots') {
        const rr = m * 0.45;
        d += `M${X + m / 2 - rr} ${Y + m / 2}a${rr} ${rr} 0 1 0 ${2 * rr} 0a${rr} ${rr} 0 1 0 ${-2 * rr} 0Z`;
      } else if (c.modStyle === 'rounded') {
        const rr = m / 2;
        const up = dark(x, y - 1);
        const dn = dark(x, y + 1);
        const lf = dark(x - 1, y);
        const rt = dark(x + 1, y);
        const tl = !up && !lf ? rr : 0;
        const tr = !up && !rt ? rr : 0;
        const br = !dn && !rt ? rr : 0;
        const bl = !dn && !lf ? rr : 0;
        d += `M${X + tl} ${Y}L${X + m - tr} ${Y}${tr ? `C${X + m - tr + tr * k} ${Y} ${X + m} ${Y + tr - tr * k} ${X + m} ${Y + tr}` : ''}L${X + m} ${Y + m - br}${br ? `C${X + m} ${Y + m - br + br * k} ${X + m - br + br * k} ${Y + m} ${X + m - br} ${Y + m}` : ''}L${X + bl} ${Y + m}${bl ? `C${X + bl - bl * k} ${Y + m} ${X} ${Y + m - bl + bl * k} ${X} ${Y + m - bl}` : ''}L${X} ${Y + tl}${tl ? `C${X} ${Y + tl - tl * k} ${X + tl - tl * k} ${Y} ${X + tl} ${Y}` : ''}Z`;
      } else d += `M${X} ${Y}h${m}v${m}h${-m}Z`;
    }
  const fill = toHex(col(r.fill), true);
  const fcol = c.finderSame ? fill : c.finderColor;
  const sq = (x: number, y: number, s2: number, rr: number) => {
    rr = Math.min(rr, s2 / 2);
    const kk = k * rr;
    return rr <= 0
      ? `M${x} ${y}h${s2}v${s2}h${-s2}Z`
      : `M${x + rr} ${y}L${x + s2 - rr} ${y}C${x + s2 - rr + kk} ${y} ${x + s2} ${y + rr - kk} ${x + s2} ${y + rr}L${x + s2} ${y + s2 - rr}C${x + s2} ${y + s2 - rr + kk} ${x + s2 - rr + kk} ${y + s2} ${x + s2 - rr} ${y + s2}L${x + rr} ${y + s2}C${x + rr - kk} ${y + s2} ${x} ${y + s2 - rr + kk} ${x} ${y + s2 - rr}L${x} ${y + rr}C${x} ${y + rr - kk} ${x + rr - kk} ${y} ${x + rr} ${y}Z`;
  };
  const circ = (cx: number, cy: number, rr: number) => `M${cx - rr} ${cy}a${rr} ${rr} 0 1 0 ${2 * rr} 0a${rr} ${rr} 0 1 0 ${-2 * rr} 0Z`;
  let fin = '';
  for (const [fx, fy] of [
    [0, 0],
    [n - 7, 0],
    [0, n - 7],
  ]) {
    const X = ox + fx * m;
    const Y = oy + fy * m;
    if (c.finderStyle === 'circle') fin += circ(X + 3.5 * m, Y + 3.5 * m, 3.5 * m) + circ(X + 3.5 * m, Y + 3.5 * m, 2.5 * m) + circ(X + 3.5 * m, Y + 3.5 * m, 1.5 * m);
    else {
      const rd = c.finderStyle === 'rounded';
      fin += sq(X, Y, 7 * m, rd ? 2.2 * m : 0) + sq(X + m, Y + m, 5 * m, rd ? 1.2 * m : 0) + sq(X + 2 * m, Y + 2 * m, 3 * m, rd ? 0.9 * m : 0);
    }
  }
  const crisp = c.modStyle === 'square' && c.finderStyle === 'square';
  let logo = null;
  if (rect) {
    // A picture only: no logo is drawn without one.
    const lw = Math.floor(rect[3] * m + 0.5);
    const lx = Math.floor(ox + (rect[0] + rect[2] / 2) * m - lw / 2 + 0.5);
    const ly = Math.floor(oy + (rect[0] + rect[2] / 2) * m - lw / 2 + 0.5);
    logo = <image href={c.logo.src.trim()} x={lx} y={ly} width={lw} height={lw} preserveAspectRatio="xMidYMid meet" />;
  }
  let lab = null;
  if (g.lab) {
    // Caption pill below, above or beside the code, in its font and case (same in qrcode.rs).
    const lh = c.label.size * 1.25;
    const top = g.labY + (g.lab - lh * g.lines.length) / 2;
    lab = (
      <>
        <rect x={g.labX} y={g.labY} width={g.labW} height={g.lab} rx={g.labR} fill={c.label.bg} />
        {g.lines.map((line, i) => (
          <text key={i} x={g.labX + g.labW / 2} y={top + i * lh + lh / 2 + c.label.size * 0.36} textAnchor="middle" fontFamily={`"${c.label.font}", Inter, system-ui, sans-serif`} fontWeight={c.label.weight} fontSize={c.label.size} fill={c.label.color} style={{ whiteSpace: 'pre' }}>
            {line}
          </text>
        ))}
      </>
    );
  }
  const rad = Math.max(0, Math.min(c.radius, g.px / 2));
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} style={{ position: 'absolute', inset: 0, overflow: 'visible' }}>
      {!c.transparent && <rect x={g.x0} y={g.y0} width={g.px} height={g.px} rx={rad} fill={layer.background} />}
      <g shapeRendering={crisp ? 'crispEdges' : 'geometricPrecision'}>
        <path d={d} fill={fill} />
        <path d={fin} fill={fcol} fillRule="evenodd" />
      </g>
      {logo}
      {lab}
    </svg>
  );
}

function Render({ layer, r, w, h, data, state }: { layer: QrLayer; r: Resolved; w: number; h: number; data: Data; state: unknown }) {
  const c = cfg(layer);
  const [url, caption] = address(c, layer.data, state as State | undefined);
  const text = payload(c, url, data);
  const q = useMatrix(text, c.ecl);
  // The prepared addresses are fetched ahead: switching on air is instant.
  for (const a of c.kind === 'url' ? c.addresses : []) {
    const p = payload(c, a.url, data);
    if (p) matrix(p, c.ecl);
  }
  // While a new code loads, the last one stays (never an empty frame on air).
  const last = useRef<{ q: Matrix; caption: string } | null>(null);
  if (q) last.current = { q, caption };
  else if (q === null || !text) last.current = null;
  const shown = q ? { q, caption } : last.current;
  if (!shown) return null;
  return <Svg layer={layer} r={r} w={w} h={h} data={data} q={shown.q} caption={shown.caption} />;
}

/** The rare types, under « More types ». */
const RARE: [NonNullable<Config['kind']>, string][] = [
  ['vcard', 'vCard'],
  ['wifi', 'Wi-Fi'],
  ['email', 'E-mail'],
];

/** The URL type's addresses: name, address, own caption; the first one is the layer's `data` (wired or exposed like before). */
function Addresses({ layer, edit }: { layer: QrLayer; edit: (fn: (l: QrLayer) => void, key?: string) => void }) {
  const c = cfg(layer);
  const items = c.addresses.length ? c.addresses : [{ name: t('Address'), url: layer.data ?? '', caption: '' }];
  return (
    <Row expose="addresses" exposeValue={items}>
      <ListEditor
        value={items}
        onChange={(v) =>
          edit((l) => {
            const list = v.map((a) => ({ name: String(a.name ?? ''), url: String(a.url ?? ''), caption: String(a.caption ?? '') }));
            l.addresses = list;
            if (list[0]) l.data = list[0].url;
          }, `qr-addr-${layer.id}`)
        }
        fields={[
          { key: 'name', placeholder: t('Name'), w: 54 },
          { key: 'url', placeholder: 'https://…' },
          { key: 'caption', placeholder: t('Caption') },
        ]}
        add={() => ({ name: `${t('Address')} ${items.length + 1}`, url: 'https://', caption: '' })}
        addLabel={t('Address')}
      />
    </Row>
  );
}

/** The address without its scheme, cut to `max` characters. */
const shortUrl = (u: string, max = 40) => {
  const s = u.replace(/^https?:\/\//i, '').replace(/\/$/, '');
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
};

/**
 * The control centre (web/src/widgets/centre.tsx), when several addresses are prepared. Line 1:
 * the address on air (its number, its caption or address). Line 2: previous / next. Line 3: the
 * list of the prepared addresses (the Operator gets it when the Designer exposes `opAddress`).
 */
function QrBar({ layer, state, act }: { layer: QrLayer; state: unknown; act: (op: string, args?: unknown) => void }) {
  const c = cfg(layer);
  const n = c.addresses.length;
  const cur = Math.min((state as State | undefined)?.addr ?? 0, Math.max(0, n - 1));
  const a = c.addresses[cur];
  const go = (i: number) => n > 0 && act('address', { i: ((i % n) + n) % n });
  useKeys({ ArrowLeft: () => go(cur - 1), ArrowRight: () => go(cur + 1) });
  return (
    <Centre
      name={(layer as { name?: string }).name}
      readout={{ k: t('Address'), v: cur + 1, of: n, sub: a ? <span title={a.url}>{a.caption.trim() ? a.caption : shortUrl(a.url)}</span> : undefined }}
      actions={[
        { icon: SkipBack, label: tc('cc', 'Previous'), key: '←', disabled: n < 2, onClick: () => go(cur - 1) },
        { icon: SkipForward, label: tc('cc', 'Next'), key: '→', disabled: n < 2, onClick: () => go(cur + 1) },
      ]}
    >
      <Row label={t('Address')} expose="opAddress" exposeValue={c.opAddress}>
        <Dropdown value={String(cur)} options={c.addresses.map((x, i) => ({ value: String(i), label: x.name || x.url || String(i + 1) }))} onChange={(v) => go(+v)} />
      </Row>
    </Centre>
  );
}

export default defineWidget({
  id: 'qrcode',
  type: 'qrCode',
  label: 'QR code',
  group: 'Sport & interaction',
  icon: QrCode,
  create: () => ({ data: 'https://example.com', background: '#ffffff', props: { width: 520, height: 520, fill: [0, 0, 0, 1] } }),
  ownsLook: true,
  content: ['data'],
  Render: ({ layer, r, w, h, data, state }) => <Render layer={layer as QrLayer} r={r} w={w} h={h} data={data} state={state} />,
  Inspector: ({ layer, set, edit }) => {
    const L = layer as QrLayer;
    const c = cfg(L);
    const rare = RARE.find(([k]) => k === c.kind);
    return (
      <>
        <Section id="qr-content" title={t('Content')}>
          <Seg
            expose="kind"
            exposeValue={c.kind}
            label={t('Type')}
            value={rare ? 'more' : c.kind}
            onChange={(v) => set('kind', v === 'more' ? (rare?.[0] ?? 'vcard') : v)}
            options={[
              ['url', 'URL'],
              ['text', t('Text')],
              ['sms', 'SMS'],
              [
                'more',
                <span key="m" style={{ display: 'inline-flex', alignItems: 'center', gap: 3, whiteSpace: 'nowrap' }}>
                  {rare ? rare[1] : t('More')} <ChevronDown size={12} />
                </span>,
                t('More types'),
              ],
            ]}
          />
          {rare && <Select label={t('More types')} value={c.kind} onChange={(v) => set('kind', v)} options={RARE} />}
          {c.kind === 'url' && <Addresses layer={L} edit={edit as never} />}
          {c.kind === 'text' && <Content label={t('Text')} path="data" value="" multiline />}
          {c.kind === 'vcard' && (
            <>
              <Content label={t('First name')} path="vcard.first" value="" />
              <Content label={t('Last name')} path="vcard.last" value="" />
              <Content label={t('Company')} path="vcard.org" value="" />
              <Content label={t('Job title')} path="vcard.title" value="" />
              <Content label={t('Phone')} path="vcard.phone" value="" />
              <Content label={t('E-mail')} path="vcard.email" value="" />
              <Content label={t('Website')} path="vcard.url" value="" />
            </>
          )}
          {c.kind === 'wifi' && (
            <>
              <Content label={t('Network')} path="wifi.ssid" value="" />
              <Seg
                label={t('Security')}
                path="wifi.security"
                value={c.wifi.security}
                options={[
                  ['WPA', 'WPA/WPA2'],
                  ['WEP', 'WEP'],
                  ['nopass', t('None')],
                ]}
              />
              {c.wifi.security !== 'nopass' && <Content label={t('Password')} path="wifi.password" value="" />}
              <Toggle label={t('Hidden')} path="wifi.hidden" value={c.wifi.hidden} />
            </>
          )}
          {c.kind === 'email' && (
            <>
              <Content label={t('To')} path="email.to" value="" placeholder={t('name@domain')} />
              <Content label={t('Subject')} path="email.subject" value="" />
              <Area label={t('Body')} path="email.body" value="" />
            </>
          )}
          {c.kind === 'sms' && (
            <>
              <Content label={tc('phone', 'Number')} path="sms.number" value="" />
              <Content label={t('Message')} path="sms.message" value="" />
            </>
          )}
        </Section>
        <Section id="qr-colours" title={t('Colour')}>
          <ColorGroupRow layer={layer} prop="fill" label={t('Modules')} />
          <Seg
            label={t('Background')}
            path="transparent"
            value={c.transparent}
            options={[
              [false, t('Solid')],
              [true, t('None')],
            ]}
          />
          {!c.transparent ? (
            <>
              <ColorPick label={t('Color')} path="background" value="#ffffff" />
              <Num label={t('Corners')} path="radius" value={c.radius} min={0} max={60} unit="px" />
            </>
          ) : (
            <ColorPick label={t('Backdrop')} path="backdrop" value={c.backdrop} />
          )}
        </Section>
        <Section id="qr-style" title={t('Style')}>
          <Seg
            label={t('Modules')}
            path="modStyle"
            value={c.modStyle}
            options={[
              ['square', t('Squares')],
              ['rounded', t('Rounded')],
              ['dots', t('Dots')],
            ]}
          />
          <Seg
            label={t('Finders')}
            path="finderStyle"
            value={c.finderStyle}
            options={[
              ['square', t('Square')],
              ['rounded', t('Rounded')],
              ['circle', t('Circle')],
            ]}
          />
          <Seg
            label={t('Finder color')}
            path="finderSame"
            value={c.finderSame}
            options={[
              [true, t('Same')],
              [false, t('Own')],
            ]}
          />
          {!c.finderSame && <ColorPick label={t('Color')} path="finderColor" value={c.finderColor} />}
        </Section>
        <Section id="qr-extras" title={t('Extras')}>
          <Toggle label={t('Logo')} path="logo.on" value={c.logo.on} />
          {c.logo.on && (
            <>
              <Row expose="logo.src" exposeValue={c.logo.src} label={t('Picture')}>
                <AssetPicker value={c.logo.src} onChange={(v) => set('logo.src', v)} />
              </Row>
              <Num label={t('Max size')} path="logo.size" value={c.logo.size} min={5} max={40} unit="%" />
            </>
          )}
          <Toggle label={t('Caption')} path="label.on" value={c.label.on} />
          {c.label.on && (
            <>
              <Content label={t('Text')} path="label.text" value={c.label.text} placeholder={t('Scan to vote')} />
              <Num label={t('Size')} path="label.size" value={c.label.size} min={16} max={72} unit="px" />
              <ColorPick label={t('Color')} path="label.color" value={c.label.color} />
              <ColorPick label={t('Pill')} path="label.bg" value={c.label.bg} />
              <Seg
                label={t('Position')}
                path="label.pos"
                value={c.label.pos}
                options={[
                  ['below', t('Below')],
                  ['beside', t('Beside')],
                  ['above', t('Above')],
                ]}
              />
              <Row label={t('Font')}>
                <FontPick family={c.label.font} weight={c.label.weight} onChange={(f, w) => edit((l) => void ((l as QrLayer).label = { ...(l as QrLayer).label, font: f, weight: w }))} />
              </Row>
              <Toggle label={t('Capitals')} path="label.upper" value={c.label.upper} />
            </>
          )}
        </Section>
        <Section id="qr-code" title={t('Code')}>
          <Seg
            label={t('Correction')}
            path="ecl"
            value={c.ecl}
            options={[
              ['L', 'L · 7 %', t('Recovers ~7 % damage')],
              ['M', 'M · 15 %', t('Recovers ~15 % damage')],
              ['Q', 'Q · 25 %', t('Recovers ~25 % damage')],
              ['H', 'H · 30 %', t('Recovers ~30 % damage (logo)')],
            ]}
          />
          <Num label={t('Quiet zone')} path="quiet" value={c.quiet} min={0} max={10} unit="mod" />
          <Num label={t('Min module')} path="minPx" value={c.minPx} min={2} max={20} unit="px" />
        </Section>
      </>
    );
  },
  // The control centre: the prepared address on air, the arrows, the list (exposable).
  Operator: ({ layer, state, act }) => <QrBar layer={layer as QrLayer} state={state} act={act} />,
  controls: (l) => cfg(l as QrLayer).kind === 'url' && cfg(l as QrLayer).addresses.length > 1,
  warnings: (layer, { data, state, r }) => {
    const L = layer as QrLayer;
    const c = cfg(L);
    const [url] = address(c, L.data, state as State | undefined);
    const text = payload(c, url, data);
    const ck = check(L, data, state as State | undefined, r, text ? matrix(text, c.ecl) : null);
    if (!ck.ready) return [];
    const out = ck.issues.slice();
    if (c.kind === 'url' && url && !/\{\{/.test(url) && !/^[a-z][a-z0-9+.-]*:/i.test(url)) out.push({ level: 'warn', text: t('No https:// — phones may read it as plain text.') });
    if (ck.ok && !out.some((i) => i.level === 'warn')) out.unshift({ level: 'ok', text: t('Readable ✓') });
    return out;
  },
});

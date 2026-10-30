// Scoreboard: sport presets and their counting rules, teams, match clock,
// score animations, event banners, feed vs manual, anti-double press, Undo.
// Same picture as crates/cg-render/src/wid/scoreboard.rs (line for line);
// rules and actions in crates/cg-core/src/wid/scoreboard.rs.
import { useState } from 'react';
import { ArrowLeftRight, ArrowRight, Minus, Pause, Play, RotateCcw, SkipBack, SkipForward, Trophy, Undo2 } from 'lucide-react';
import { interpolateText, num } from '../engine/evaluate';
import type { Data } from '../engine/types';
import { useFonts } from '../engine/fonts';
import { Dropdown } from '../ui/Dropdown';
import { useApp } from '../store';
import { t, tc, tl } from '../i18n';
import { hexMix } from './map-geo';
import { AssetPicker } from '../designer/Inspector';
import { Btns, ColorPick, Content, defineWidget, getPath, Num, Row, Section, Seg, Select, Text, Toggle, type Warning, useBound, useFrame, useKeys } from './kit';
import { Centre, GAP, SetInput } from './centre';
import { col, ink, mix, poly, rrect, tf, ellipse, Pt, type Cmd } from './scoreboard-paint';

export type Sport = 'football' | 'rugby' | 'basketball' | 'handball' | 'volleyball' | 'tennis' | 'generic';
type Kind = 'points' | 'sets' | 'tennis';
export interface Team {
  name?: string;
  short?: string;
  crest?: 'shield' | 'round' | 'diamond';
  logo?: string;
  c1?: string;
  c2?: string;
  /** Score from data ({{key}}); empty = manual. */
  score?: string;
}
export interface Fields {
  period?: boolean;
  clock?: boolean;
  fouls?: boolean;
  timeouts?: boolean;
  cards?: boolean;
  poss?: boolean;
  sets?: boolean;
  /** Shot clock (basketball: 24 s, reset to 24 or 14). */
  shot?: boolean;
  /** Penalty shoot-out kicks (the « Penalties » period). */
  shootout?: boolean;
  /** Timed suspensions counting down. */
  susp?: boolean;
}
export interface Config {
  preset?: Sport;
  teams?: Team[];
  fields?: Fields;
  digits?: number;
  clock?: { dir?: 'up' | 'down' | 'none'; len?: number; extra?: number; tenths?: boolean };
  layout?: 'horizontal' | 'stacked';
  order?: 'clockFirst' | 'clockMid' | 'clockLast';
  version?: 'compact' | 'full';
  nameW?: number;
  radius?: number;
  showCrest?: boolean;
  anim?: { score?: 'flip' | 'highlight' | 'none'; banner?: boolean; evDur?: number };
  debounce?: number;
  source?: { rule?: 'override' | 'manual' | 'feed'; overS?: number };
  bestOf?: number;
  shot?: { len?: number; reset?: number };
  /** Minutes of a suspension (0 = the sport's). */
  suspLen?: number;
  /** Background of the board. */
  bg?: string;
  font?: string;
  /** The texts the board writes, by English wording (empty = in the project's language). */
  texts?: Record<string, string>;
  /** The rows of the control centre the Operator gets (when the Designer exposes them). */
  opClock?: boolean;
  opFouls?: boolean;
  opTimeouts?: boolean;
  opSusp?: boolean;
  opShot?: boolean;
  opShootout?: boolean;
  opSwap?: boolean;
  opScore?: boolean;
  opAdded?: boolean;
}

interface TeamState {
  score?: number;
  fouls?: number;
  toUsed?: number;
  yellow?: number;
  red?: number;
  susp?: number;
  added?: number[];
  pens?: boolean[];
  susps?: [number, number][];
}
export interface State {
  teams?: TeamState[];
  period?: number;
  clock?: { running?: boolean; startAt?: number; base?: number };
  added?: number;
  poss?: number;
  swapped?: boolean;
  version?: 'compact' | 'full' | null;
  tennis?: { pts?: [number, number]; games?: [number, number]; sets?: [number, number][]; tb?: boolean; done?: boolean; free?: boolean; freePts?: [string, string] };
  volley?: { sets?: [number, number][]; won?: [number, number]; done?: boolean };
  fx?: { sc?: Record<string, { at: number; from: string }>; ev?: { text: string; sub: string; color: string; at: number; vars?: Record<string, string> } | null; undoAt?: number; ignAt?: number };
  src?: { manualAt?: number; dirty?: boolean };
  last?: { key: string; at: number };
  ignored?: number;
  hist?: { label: string }[];
  shotBase?: number;
  shotLen?: number;
}

// ---------------------------------------------------------------- presets --

interface Pts {
  label: string;
  v: number;
  ev: string;
}
interface Preset {
  name: string;
  periods: string[];
  len: number;
  extra: number;
  dir: 'up' | 'down' | 'none';
  cumulative: boolean;
  kind: Kind;
  pts: Pts[];
  fields: Required<Fields>;
  digits: number;
  cards: string[];
  timeouts: number;
  bonus: number;
  poss: 'Possession' | 'Serve';
  multi: boolean;
  rules: string;
}
const F = (period: boolean, clock: boolean, fouls: boolean, timeouts: boolean, cards: boolean, poss: boolean, sets: boolean) => ({ period, clock, fouls, timeouts, cards, poss, sets, shot: false, shootout: false, susp: false });
export const PRESETS: Record<Sport, Preset> = {
  football: {
    name: 'Football',
    periods: ['1st half', '2nd half', 'Extra time 1', 'Extra time 2', 'Penalties'],
    len: 45,
    extra: 15,
    dir: 'up',
    cumulative: true,
    kind: 'points',
    pts: [{ label: 'GOAL', v: 1, ev: 'GOAL!' }],
    fields: { ...F(true, true, false, false, true, false, false), shootout: true },
    digits: 2,
    cards: ['yellow', 'red'],
    timeouts: 0,
    bonus: 0,
    poss: 'Possession',
    multi: false,
    rules: 'Goal = 1 · 2 halves of 45 min, counting up · added time.',
  },
  rugby: {
    name: 'Rugby',
    periods: ['1st half', '2nd half', 'Extra time'],
    len: 40,
    extra: 10,
    dir: 'up',
    cumulative: true,
    kind: 'points',
    pts: [
      { label: 'TRY +5', v: 5, ev: 'TRY!' },
      { label: 'CONV +2', v: 2, ev: 'CONVERSION' },
      { label: 'PEN +3', v: 3, ev: 'PENALTY GOAL' },
      { label: 'DROP +3', v: 3, ev: 'DROP GOAL' },
    ],
    fields: F(true, true, false, false, true, false, false),
    digits: 2,
    cards: ['yellow', 'red'],
    timeouts: 0,
    bonus: 0,
    poss: 'Possession',
    multi: false,
    rules: 'Try 5 · conversion 2 · penalty 3 · drop 3 · 2 × 40 min.',
  },
  basketball: {
    name: 'Basketball',
    periods: ['Q1', 'Q2', 'Q3', 'Q4', 'OT'],
    len: 10,
    extra: 5,
    dir: 'down',
    cumulative: false,
    kind: 'points',
    pts: [
      { label: '+1', v: 1, ev: '' },
      { label: '+2', v: 2, ev: '' },
      { label: '+3', v: 3, ev: '3 POINTS!' },
    ],
    fields: { ...F(true, true, true, true, false, true, false), shot: true },
    digits: 3,
    cards: [],
    timeouts: 5,
    bonus: 5,
    poss: 'Possession',
    multi: false,
    rules: 'Free throw 1 · field goal 2 · 3-pointer 3 · 4 × 10 min counting down · team fouls (bonus at 5) · 5 timeouts.',
  },
  handball: {
    name: 'Handball',
    periods: ['1st half', '2nd half', 'Extra 1', 'Extra 2'],
    len: 30,
    extra: 5,
    dir: 'up',
    cumulative: true,
    kind: 'points',
    pts: [{ label: 'GOAL', v: 1, ev: 'GOAL!' }],
    fields: { ...F(true, true, false, true, true, false, false), susp: true },
    digits: 2,
    cards: ['yellow', '2 min', 'red'],
    timeouts: 3,
    bonus: 0,
    poss: 'Possession',
    multi: false,
    rules: 'Goal = 1 · 2 × 30 min · 3 timeouts · 2-minute suspensions.',
  },
  volleyball: {
    name: 'Volleyball',
    periods: ['Set 1', 'Set 2', 'Set 3', 'Set 4', 'Set 5'],
    len: 0,
    extra: 0,
    dir: 'none',
    cumulative: false,
    kind: 'sets',
    pts: [{ label: 'POINT', v: 1, ev: '' }],
    fields: F(true, false, false, true, false, true, true),
    digits: 2,
    cards: [],
    timeouts: 2,
    bonus: 0,
    poss: 'Serve',
    multi: false,
    rules: 'Rally point · set to 25 (5th to 15), 2-point lead · best of 5 · serve to the side that scored.',
  },
  tennis: {
    name: 'Tennis',
    periods: ['Set 1', 'Set 2', 'Set 3', 'Set 4', 'Set 5'],
    len: 0,
    extra: 0,
    dir: 'none',
    cumulative: false,
    kind: 'tennis',
    pts: [{ label: 'POINT', v: 1, ev: '' }],
    fields: F(true, false, false, false, false, true, true),
    digits: 2,
    cards: [],
    timeouts: 0,
    bonus: 0,
    poss: 'Serve',
    multi: false,
    rules: '15 / 30 / 40 / deuce / advantage · game · set at 6 with 2 games lead · tie-break at 6–6 (7 points, 2 lead).',
  },
  generic: {
    name: 'Generic',
    periods: ['P1', 'P2', 'P3', 'P4'],
    len: 20,
    extra: 5,
    dir: 'up',
    cumulative: false,
    kind: 'points',
    pts: [{ label: '+1', v: 1, ev: 'SCORE!' }],
    fields: F(true, true, false, false, false, false, false),
    digits: 3,
    cards: [],
    timeouts: 0,
    bonus: 0,
    poss: 'Possession',
    multi: true,
    rules: '+1 per point · 4 periods · 2 or more teams.',
  },
};
const CARD_COL: Record<string, string> = { yellow: '#facc15', red: '#ef4444', '2 min': '#38bdf8' };
/** Names of the cards on the Operator's buttons (translated where shown). */
const CARD_NAMES: Record<string, string> = { yellow: 'Yellow', red: 'Red', '2 min': '2 min' };
const SAMPLE: Required<Team>[] = [
  { name: 'Paris Saint-Germain', short: 'PSG', c1: '#1e3a8a', c2: '#e11d48', crest: 'shield', logo: '', score: '' },
  { name: 'Olympique de Marseille', short: 'OM', c1: '#0ea5e9', c2: '#ffffff', crest: 'round', logo: '', score: '' },
  { name: 'Lyon Dragons', short: 'LYO', c1: '#b91c1c', c2: '#facc15', crest: 'diamond', logo: '', score: '' },
  { name: 'Nantes Canaris', short: 'NAN', c1: '#eab308', c2: '#15803d', crest: 'shield', logo: '', score: '' },
];
const TEAM_DEF: Required<Team> = { name: 'Team', short: '', crest: 'shield', logo: '', c1: '#334155', c2: '#ffffff', score: '' };

/** Settings with their defaults (mirrors Config::default()). */
function cfg(c: Config) {
  return {
    preset: c.preset ?? 'football',
    teams: c.teams ?? [SAMPLE[0], SAMPLE[1]],
    fields: { ...PRESETS.football.fields, ...c.fields },
    digits: c.digits ?? 2,
    clock: { dir: 'up' as const, len: 45, extra: 15, tenths: true, ...c.clock },
    layout: c.layout ?? 'horizontal',
    order: c.order ?? 'clockFirst',
    version: c.version ?? 'full',
    nameW: c.nameW ?? 5.4,
    radius: c.radius ?? 10,
    showCrest: c.showCrest ?? true,
    anim: { score: 'flip' as const, banner: true, evDur: 3.2, ...c.anim },
    debounce: c.debounce ?? 400,
    source: { rule: 'override' as const, overS: 20, ...c.source },
    bestOf: c.bestOf ?? 3,
    shot: { len: 24, reset: 14, ...c.shot },
    suspLen: c.suspLen ?? 0,
    bg: c.bg ?? '#05060c',
    font: c.font ?? 'Inter',
    texts: c.texts ?? {},
    opClock: c.opClock ?? true,
    opFouls: c.opFouls ?? true,
    opTimeouts: c.opTimeouts ?? true,
    opSusp: c.opSusp ?? true,
    opShot: c.opShot ?? true,
    opShootout: c.opShootout ?? false,
    opSwap: c.opSwap ?? false,
    opScore: c.opScore ?? true,
    opAdded: c.opAdded ?? true,
  };
}
type K = ReturnType<typeof cfg>;
const P = (k: K) => PRESETS[k.preset] ?? PRESETS.football;
const teamCount = (k: K) => (P(k).multi ? Math.max(2, k.teams.length) : 2);
const team = (k: K, i: number): Required<Team> => (k.teams[i] ? { ...TEAM_DEF, ...k.teams[i] } : SAMPLE[i % 4]);
const ts = (s: State, i: number): Required<TeamState> => ({ score: 0, fouls: 0, toUsed: 0, yellow: 0, red: 0, susp: 0, added: [], pens: [], susps: [], ...s.teams?.[i] });

const STOP = new Set(['de', 'du', 'des', 'la', 'le', 'les', 'of', 'the', 'fc', 'and', 'et', 'd']);
/** 3-letter short name from a long name (mirrors auto_short). */
export function autoShort(name: string): string {
  const words = String(name || '')
    .replace(/[’']/g, ' ')
    .split(/[\s-]+/)
    .filter((w) => w && !STOP.has(w.toLowerCase()));
  const first = (w: string, n: number) => [...w].slice(0, n).join('');
  if (!words.length) return '???';
  if (words.length >= 3)
    return words
      .slice(0, 3)
      .map((w) => first(w, 1))
      .join('')
      .toUpperCase();
  if (words.length === 2) return (first(words[0], 1) + first(words[1], 2)).toUpperCase();
  return first(words[0], 3).toUpperCase();
}
export const shortOf = (t: Team) => (String(t.short ?? '').trim() ? [...String(t.short).trim()].slice(0, 3).join('').toUpperCase() : autoShort(t.name ?? ''));

// ------------------------------------------------------------------ clock --

const periodLen = (k: K, i: number) => {
  const n = (P(k).periods[i] ?? '').toLowerCase();
  return Math.max(0, n.includes('extra') || n === 'ot' ? k.clock.extra : k.clock.len) * 60000;
};
const elapsed = (s: State, now: number) => Math.max(0, (s.clock?.base ?? 0) + (s.clock?.running ? now - (s.clock?.startAt ?? 0) : 0));
const clockOn = (k: K, s: State) => k.clock.dir !== 'none' && k.fields.clock && P(k).kind === 'points' && P(k).periods[s.period ?? 0] !== 'Penalties';
const offsetMs = (k: K, s: State) => {
  if (!P(k).cumulative || k.clock.dir !== 'up') return 0;
  let o = 0;
  for (let i = 0; i < (s.period ?? 0); i++) o += periodLen(k, i);
  return o;
};
const pad = (n: number) => String(n).padStart(2, '0');
function fmtMs(ms: number, tenths: boolean) {
  ms = Math.max(0, Math.trunc(ms));
  if (tenths && ms < 60000) return `${Math.floor(ms / 1000)}.${Math.floor((ms % 1000) / 100)}`;
  const t = Math.floor(ms / 1000);
  return `${pad(Math.floor(t / 60))}:${pad(t % 60)}`;
}
/** [text, over, ended] (mirrors clock_text). */
function clockText(k: K, s: State, now: number): [string, boolean, boolean] {
  const e = elapsed(s, now);
  const len = periodLen(k, s.period ?? 0);
  if (k.clock.dir === 'down') return [fmtMs(len - e, k.clock.tenths), false, e >= len];
  return [fmtMs(offsetMs(k, s) + e, false), len > 0 && e > len, false];
}

// ----------------------------------------------------------------- source --

type Active = { id: 'manual' | 'lost' | 'priority' | 'feed'; left?: number };
function feedValues(k: K, data: Data): (number | null | undefined)[] {
  return Array.from({ length: teamCount(k) }, (_, i) => {
    const src = team(k, i).score;
    if (!src.includes('{{')) return undefined;
    const v = interpolateText(src, data).trim();
    return /^-?\d+$/.test(v) ? +v : null;
  });
}
function activeSource(k: K, s: State, data: Data, now: number): Active {
  const fv = feedValues(k, data);
  if (P(k).kind !== 'points' || fv.every((v) => v === undefined)) return { id: 'manual' };
  if (fv.some((v) => v === null)) return { id: 'lost' };
  const dirty = !!s.src?.dirty;
  if (k.source.rule === 'feed') return { id: 'feed' };
  if (k.source.rule === 'manual') return dirty ? { id: 'priority' } : { id: 'feed' };
  const left = k.source.overS * 1000 - (now - (s.src?.manualAt ?? 0));
  return dirty && left > 0 ? { id: 'priority', left: Math.ceil(left / 1000) } : { id: 'feed' };
}
/** The source in control, for the operator (never drawn on air). */
function activeText(a: Active, rule: string) {
  if (a.id === 'manual') return t('MANUAL');
  if (a.id === 'lost') return t('MANUAL · feed lost');
  if (a.id === 'priority') return a.left != null ? t('MANUAL override · feed back in {n} s', { n: a.left }) : t('MANUAL (manual has priority)');
  return rule === 'feed' ? t('FEED (feed has priority)') : rule === 'manual' ? t('FEED (until a manual change)') : t('FEED');
}
function scores(k: K, s: State, data: Data, now: number): number[] {
  const feed = activeSource(k, s, data, now).id === 'feed';
  const fv = feedValues(k, data);
  return Array.from({ length: teamCount(k) }, (_, i) => (feed && typeof fv[i] === 'number' ? (fv[i] as number) : ts(s, i).score));
}

// ----------------------------------------------------------------- tennis --

type Tn = Required<NonNullable<State['tennis']>>;
const tennisOf = (s: State): Tn => ({ pts: [0, 0], games: [0, 0], sets: [], tb: false, done: false, free: false, freePts: ['', ''], ...s.tennis });
function tennisPts(t: Tn, i: number) {
  const p = t.pts[i];
  const o = t.pts[1 - i];
  if (t.free) return t.freePts[i] ?? '';
  if (t.tb) return String(p);
  if (p >= 3 && o >= 3) return p > o ? 'AD' : '40';
  return ['0', '15', '30', '40'][Math.min(p, 3)];
}
function tennisTag(t: Tn) {
  if (t.done) return 'FINAL';
  if (t.free) return 'MANUAL ENTRY';
  if (t.tb) return 'TIE-BREAK';
  if (t.pts[0] >= 3 && t.pts[0] === t.pts[1]) return 'DEUCE';
  if (t.pts[0] >= 3 && t.pts[1] >= 3) return 'ADVANTAGE';
  return '';
}
const periodText = (k: K, s: State) => (P(k).kind === 'tennis' ? `SET ${tennisOf(s).sets.length + (tennisOf(s).done ? 0 : 1)}` : (P(k).periods[s.period ?? 0] ?? ''));
const won = (s: State): [number, number] => s.volley?.won ?? [0, 0];

// ------------------------------------------------------------------ texts --

/** A text the board writes: the Designer's own, else in the project's language (mirrors tr). */
export const tr = (k: K, lang: string, en: string) => (k.texts[en]?.trim() ? k.texts[en] : tl(lang, en));
/** `{name}` replaced by its value (mirrors fill). */
const fill = (s: string, vars: Record<string, string> | undefined) => Object.entries(vars ?? {}).reduce((a, [key, v]) => a.split(`{${key}}`).join(v), s);
/** The texts of a sport the Designer may rename, in order (mirrors texts_of). */
function textsOf(k: K): string[] {
  const pr = P(k);
  const out: string[] = [];
  const add = (x: string) => x && !out.includes(x) && out.push(x);
  pr.periods.forEach(add);
  pr.pts.forEach((pt) => add(pt.ev));
  if (pr.kind === 'tennis') ['FINAL', 'MANUAL ENTRY', 'TIE-BREAK', 'DEUCE', 'ADVANTAGE', 'GAME, SET & MATCH'].forEach(add);
  if (pr.kind === 'sets') ['MATCH!', '{team} wins {a}–{b}'].forEach(add);
  if (pr.bonus > 0) ['FOULS', 'BONUS'].forEach(add);
  if (pr.timeouts > 0) ['TO', 'TIMEOUT', '{team} · {n} left'].forEach(add);
  for (const c of pr.cards) add(c === 'yellow' ? 'YELLOW CARD' : c === 'red' ? 'RED CARD' : '2-MIN SUSPENSION');
  if (k.fields.susp) add('SUSPENSION');
  return out;
}

// -------------------------------------------- shot clock, suspensions, shoot-out --

const shotOn = (k: K, s: State) => k.fields.shot && clockOn(k, s);
/** Whole seconds left of the shot clock, ended (mirrors shot_text). */
function shotText(k: K, s: State, now: number): [string, boolean] {
  const len = (s.shotLen ?? 0) > 0 ? s.shotLen! : Math.trunc(Math.max(1, k.shot.len) * 1000);
  const left = Math.max(0, len - (elapsed(s, now) - (s.shotBase ?? 0)));
  return [String(Math.floor((left + 999) / 1000)), left === 0];
}
const matchMs = (k: K, s: State, now: number) => offsetMs(k, s) + elapsed(s, now);
const suspMin = (k: K) => (k.suspLen > 0 ? k.suspLen : k.preset === 'rugby' ? 10 : 2);
/** Time left of the running suspensions of a team, soonest first (mirrors susps_left). */
function suspsLeft(k: K, s: State, i: number, now: number): number[] {
  const m = matchMs(k, s, now);
  return (ts(s, i).susps ?? [])
    .map(([a, l]) => a + l - m)
    .filter((l) => l > 0)
    .sort((a, b) => a - b);
}
const fmtLeft = (ms: number) => {
  const t = Math.floor((Math.max(0, ms) + 999) / 1000);
  return `${Math.floor(t / 60)}:${pad(t % 60)}`;
};
const shootoutOn = (k: K, s: State) => k.fields.shootout && P(k).periods[s.period ?? 0] === 'Penalties';

/** Colours of the board from its background (mirrors pal). */
function pal(k: K) {
  const bg = k.bg.trim();
  if (!bg || bg.toLowerCase() === '#05060c') return { bg: '#05060c', cell: CELL_BG, tag: TAG_BG };
  return { bg, cell: hexMix(bg, '#ffffff', 0.04), tag: hexMix(bg, '#ffffff', 0.08) };
}

// ---------------------------------------------------------------- drawing --

const TAG_BG = '#11142a';
const CELL_BG = '#0b0d18';
const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
const easeIO = (k: number) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
const easeOut = (k: number) => 1 - Math.pow(1 - k, 3);

function crest(p: Pt, t: Required<Team>, x: number, y: number, size: number, data: Data) {
  const logo = interpolateText(t.logo, data);
  if (logo.trim() && p.image(logo, x, y, size, size)) return;
  const s = size / 100;
  const c1 = col(t.c1)[3] > 0 ? t.c1 : '#334155';
  const c2 = col(t.c2)[3] > 0 ? t.c2 : '#ffffff';
  const letter = [...shortOf(t)][0] ?? '?';
  let shape: Cmd[], band: Cmd[] | null, ty: number, fs: number;
  if (t.crest === 'round') [shape, band, ty, fs] = [ellipse(50, 50, 44, 44), null, 50, 44];
  else if (t.crest === 'diamond')
    [shape, band, ty, fs] = [
      poly(
        [
          [50, 4],
          [95, 50],
          [50, 96],
          [5, 50],
        ],
        true,
      ),
      poly(
        [
          [27, 47.5],
          [73, 47.5],
          [73, 52.5],
          [27, 52.5],
        ],
        true,
      ),
      50,
      44,
    ];
  else
    [shape, band, ty, fs] = [
      [['M', 50, 4], ['L', 92, 16], ['L', 92, 48], ['C', 92, 74, 72, 90, 50, 97], ['C', 28, 90, 8, 74, 8, 48], ['L', 8, 16], ['Z']],
      poly(
        [
          [8, 30],
          [92, 62],
          [92, 48],
          [8, 16],
        ],
        true,
      ),
      58,
      40,
    ];
  const sh = tf(shape, x, y, s);
  p.fill(sh, c1);
  if (band) {
    const a = p.alpha;
    p.alpha = a * 0.9;
    p.fill(tf(band, x, y, s), c2);
    p.alpha = a;
  }
  if (t.crest === 'round') {
    const a = p.alpha;
    p.alpha = a * 0.7;
    p.stroke(tf(ellipse(50, 50, 31, 31), x, y, s), c2, 3 * s, false);
    p.alpha = a;
  }
  p.stroke(sh, c2, 7 * s, true);
  p.textHalo(letter, x + 50 * s, y + ty * s, col(ink(c1)), col(c1), 5 * s, { size: fs * s, weight: 800, anchor: 0 });
}

function possMark(p: Pt, serve: boolean, cx: number, cy: number, h: number, mirror: boolean) {
  if (serve) {
    p.circle(cx, cy, h * 0.24, '#84cc16');
    p.circle(cx - h * 0.03, cy - h * 0.03, h * 0.17, '#d9f99d');
  } else {
    const w = h * 0.32;
    const hh = h * 0.22;
    const d = mirror ? -1 : 1;
    p.fill(
      poly(
        [
          [cx - (d * w) / 2, cy - hh],
          [cx + (d * w) / 2, cy],
          [cx - (d * w) / 2, cy + hh],
        ],
        true,
      ),
      '#facc15',
    );
  }
}

type Item = ['txt', string, string, number] | ['dot', boolean] | ['card', string] | ['badge', string, string] | ['pen', boolean | null] | ['sep'];
function stats(k: K, s: State, i: number, lang: string, now: number): Item[] {
  const pr = P(k);
  const t = ts(s, i);
  const groups: Item[][] = [];
  if (shootoutOn(k, s)) {
    const g: Item[] = Array.from({ length: Math.max(5, t.pens.length) }, (_, j): Item => ['pen', t.pens[j] ?? null]);
    g.push(['txt', ` ${t.pens.filter(Boolean).length}`, '#ffffff', 800]);
    groups.push(g);
  }
  if (k.fields.fouls) {
    const g: Item[] = [
      ['txt', `${tr(k, lang, 'FOULS')} `, '#ffffffbf', 700],
      ['txt', String(t.fouls), '#ffffff', 800],
    ];
    if (pr.bonus > 0 && t.fouls >= pr.bonus) g.push(['badge', tr(k, lang, 'BONUS'), '#ff3b5c']);
    groups.push(g);
  }
  if (k.fields.timeouts && pr.timeouts > 0) {
    const g: Item[] = [['txt', `${tr(k, lang, 'TO')} `, '#ffffffbf', 700]];
    const left = Math.max(0, pr.timeouts - t.toUsed);
    for (let j = 0; j < pr.timeouts; j++) g.push(['dot', j < left]);
    groups.push(g);
  }
  if (k.fields.cards && pr.cards.length) {
    const g: Item[] = [];
    for (const c of pr.cards) {
      const n = c === 'yellow' ? t.yellow : c === 'red' ? t.red : t.susp;
      if (n > 0) {
        g.push(['card', CARD_COL[c]]);
        if (n > 1) g.push(['txt', `×${n} `, '#ffffff', 700]);
      }
    }
    if (g.length) groups.push(g);
  }
  if (k.fields.susp) {
    const g = suspsLeft(k, s, i, now).map((ms): Item => ['badge', fmtLeft(ms), '#0284c7']);
    if (g.length) groups.push(g);
  }
  const out: Item[] = [];
  groups.forEach((g, j) => {
    if (j) out.push(['sep']);
    out.push(...g);
  });
  return out;
}

function drawStats(p: Pt, items: Item[], x: number, w: number, cy: number, fs: number, color: string, right: boolean) {
  const widths = items.map((it) => (it[0] === 'txt' ? p.tw(it[1], fs, it[3], 0) : it[0] === 'dot' ? fs * 0.55 + 3 : it[0] === 'card' ? fs * 0.6 + 3 : it[0] === 'badge' ? p.tw(it[1], fs * 0.8, 800, 0) + 18 : it[0] === 'pen' ? fs * 0.62 + 3 : fs * 0.9));
  const total = widths.reduce((a, b) => a + b, 0);
  let cx = right ? x + w - total : x;
  const white = color === '#ffffff';
  items.forEach((it, j) => {
    const iw = widths[j];
    if (it[0] === 'txt') p.text(it[1], cx, cy, white ? it[2] : color, { size: fs, weight: it[3] });
    else if (it[0] === 'dot') p.circle(cx + fs * 0.275, cy, fs * 0.275, it[1] ? '#facc15' : '#ffffff33');
    else if (it[0] === 'card') p.rect(cx, cy - fs * 0.425, fs * 0.6, fs * 0.85, 2, it[1]);
    else if (it[0] === 'pen') p.circle(cx + fs * 0.31, cy, fs * 0.31, it[1] === true ? '#22c55e' : it[1] === false ? '#ef4444' : '#ffffff33');
    else if (it[0] === 'badge') {
      p.rect(cx + 6, cy - fs * 0.5, iw - 6, fs, 4, it[2]);
      p.text(it[1], cx + 6 + (iw - 6) / 2, cy, '#ffffff', { size: fs * 0.8, weight: 800, anchor: 0 });
    } else p.text('|', cx + iw / 2, cy, '#ffffff4d', { size: fs, weight: 700, anchor: 0 });
    cx += iw;
  });
}

/** Lays out (and unless dry, draws) the board at row height h; returns its size (mirrors board()). */
function board(p: Pt, k: K, s: State, data: Data, now: number, h: number, lang: string): [number, number] {
  const pr = P(k);
  const pl = pal(k);
  const shot = shotOn(k, s) ? shotText(k, s, now) : null;
  const n = teamCount(k);
  const order = Array.from({ length: n }, (_, i) => (s.swapped ? n - 1 - i : i));
  const full = (s.version ?? k.version) === 'full';
  const sc = scores(k, s, data, now);
  const sfs = h * 0.62;
  const nfs = h * 0.4;
  const digits = Math.min(4, Math.max(1, k.digits));
  const reserve = digits * p.tw('0', sfs, 800, 0);
  const scoreW = reserve + h * 0.5;
  const nameW = full ? h * Math.min(20, Math.max(1, k.nameW)) : p.tw('WWW', nfs, 800, 0) + h * 0.3;
  const crestW = k.showCrest ? h : 0;
  const barW = h * 0.12;
  const possW = k.fields.poss ? h * 0.42 : 0;
  const setsW = k.fields.sets && pr.kind === 'sets' ? h * 0.7 : 0;
  const ck = clockOn(k, s) ? clockText(k, s, now) : null;
  const per = k.fields.period ? tr(k, lang, periodText(k, s)) : '';
  const serve = pr.poss === 'Serve';
  const radius = k.radius;
  const gap = h * 0.0375;
  const poss = s.poss ?? 0;

  const nameOf = (t: Required<Team>, w: number) => {
    const long = t.name.toUpperCase();
    return !full || p.tw(long, nfs, 800, nfs * 0.02) > w ? shortOf(t) : long;
  };
  const nameBlock = (t: Required<Team>, x: number, y: number, w: number, right: boolean) => {
    const str = nameOf(t, w - h * 0.3);
    p.clip(rrect(x, y, w, h, 0));
    if (right) p.text(str, x + w - h * 0.15, y + h / 2, ink(t.c1), { size: nfs, weight: 800, ls: nfs * 0.02, anchor: 1 });
    else p.text(str, x + h * 0.15, y + h / 2, ink(t.c1), { size: nfs, weight: 800, ls: nfs * 0.02 });
    p.pop();
  };
  const scoreCell = (i: number, t: Required<Team>, x: number, y: number, w: number) => {
    const txt = String(sc[i] ?? 0);
    const real = p.tw(txt, sfs, 800, 0);
    const sx = real > reserve ? reserve / real : 1;
    let bg = col(pl.cell);
    let fg = col('#ffffff');
    let scale = 1;
    const fx = s.fx?.sc?.[i];
    const kk = fx ? (now - fx.at) / 700 : 1;
    p.clip(rrect(x, y, w, h, 0));
    const cx = x + w / 2;
    const cy = y + h / 2;
    if (fx && kk >= 0 && kk < 1 && k.anim.score === 'flip') {
      p.rectC(x, y, w, h, 0, bg);
      const [str, sy] = kk < 0.5 ? [fx.from, Math.cos(kk * Math.PI)] : [txt, Math.cos((kk - 1) * Math.PI)];
      const r2 = p.tw(str, sfs, 800, 0);
      const sx2 = r2 > reserve ? reserve / r2 : 1;
      p.textC(str, cx, cy, fg, { size: sfs, weight: 800, anchor: 0, sx: sx2, sy });
      p.rect(x, cy - h * 0.005, w, h * 0.01, 0, '#ffffff2e');
      p.pop();
      return;
    }
    if (fx && kk >= 0 && kk < 1 && k.anim.score === 'highlight') {
      const e = Math.sin(Math.PI * Math.min(1, kk * 1.2));
      bg = mix(bg, col(t.c1), e);
      scale = 1 + 0.18 * e;
      if (e > 0.4) fg = col(ink(t.c1));
    }
    p.rectC(x, y, w, h, 0, bg);
    p.textC(txt, cx, cy, fg, { size: sfs * scale, weight: 800, anchor: 0, sx });
    p.pop();
  };

  let w0: number, h0: number;
  if (pr.kind === 'tennis') {
    const tn = tennisOf(s);
    const cellW = h * 0.72;
    const gW = h * 0.85;
    const pW = h * 1.25;
    const slots = Math.min(5, Math.max(3, k.bestOf)) - 1;
    const rowW = barW + possW + crestW + nameW + slots * cellW + gW + pW;
    const hd = h * 0.56;
    const tag = tennisTag(tn) && tr(k, lang, tennisTag(tn));
    w0 = rowW;
    h0 = hd + 2 * h + 2 * gap;
    p.clip(rrect(0, 0, w0, h0, radius));
    p.rect(0, 0, w0, hd, 0, pl.tag);
    let x = h * 0.25;
    const pfs = h * 0.3;
    p.text(per, x, hd / 2, '#ffffff', { size: pfs, weight: 800, ls: pfs * 0.08 });
    x += p.tw(per, pfs, 800, pfs * 0.08) + h * 0.3;
    if (tag) {
      const tw = p.tw(tag, pfs, 800, pfs * 0.08);
      p.rect(x, hd / 2 - pfs * 0.75, tw + h * 0.3, pfs * 1.5, 5, tn.tb ? '#ff3b5c' : tn.free ? '#f59e0b' : '#7c5cff');
      p.text(tag, x + h * 0.15, hd / 2, '#ffffff', { size: pfs, weight: 800, ls: pfs * 0.08 });
    }
    let y = hd;
    for (const i of order.slice(0, 2)) {
      const t = team(k, i);
      y += gap;
      p.rect(0, y - gap, w0, gap, 0, '#1a1d30');
      p.rect(0, y, w0, h, 0, pl.cell);
      p.rect(0, y, barW, h * 0.7, 0, t.c1);
      p.rect(0, y + h * 0.7, barW, h * 0.3, 0, t.c2);
      let xx = barW;
      p.rect(xx, y, possW + crestW + nameW, h, 0, t.c1);
      if (possW > 0 && poss === i) possMark(p, serve, xx + possW / 2, y + h / 2, h, false);
      xx += possW;
      if (crestW > 0) crest(p, t, xx + (crestW - h * 0.74) / 2, y + h * 0.13, h * 0.74, data);
      xx += crestW;
      nameBlock(t, xx, y, nameW, false);
      xx += nameW;
      for (let j = 0; j < slots; j++) {
        p.rect(xx, y, cellW, h, 0, '#ffffff0a');
        p.rect(xx, y, 1, h, 0, '#ffffff14');
        const st = tn.sets[j];
        if (st) {
          const w = st[i] > st[1 - i];
          p.text(String(st[i]), xx + cellW / 2, y + h / 2, w ? '#ffffff' : '#ffffff8c', { size: h * 0.46, weight: w ? 800 : 600, anchor: 0 });
        }
        xx += cellW;
      }
      p.rect(xx, y, gW, h, 0, '#1b1f38');
      p.text(String(tn.games[i]), xx + gW / 2, y + h / 2, '#ffffff', { size: h * 0.56, weight: 800, anchor: 0 });
      xx += gW;
      const pts = tennisPts(tn, i);
      p.rect(xx, y, pW, h, 0, pts === 'AD' ? '#facc15' : '#ffffff');
      p.text(pts, xx + pW / 2, y + h / 2, pl.cell, { size: h * 0.56, weight: 800, anchor: 0 });
      y += h;
    }
    p.pop();
  } else if (k.layout === 'stacked') {
    const statW = full && (k.fields.fouls || k.fields.timeouts || k.fields.cards || k.fields.susp || shootoutOn(k, s)) ? h * 3.6 : 0;
    const rowW = barW + crestW + nameW + possW + statW + setsW + scoreW;
    const hd = k.fields.period || ck ? h * 0.6 : 0;
    w0 = rowW;
    h0 = hd + n * (h + gap);
    const top = k.order !== 'clockLast';
    p.clip(rrect(0, 0, w0, h0, radius));
    const headY = top ? 0 : h0 - hd;
    if (hd > 0) {
      p.rect(0, headY, w0, hd, 0, pl.tag);
      const pfs = h * 0.3;
      p.text(per, h * 0.25, headY + hd / 2, '#c7cbe0', { size: pfs, weight: 800, ls: pfs * 0.08 });
      if (ck) {
        let rx = w0 - h * 0.25;
        if (shot) {
          const sfs = h * 0.42;
          p.text(shot[0], rx, headY + hd / 2, shot[1] ? '#ff3b5c' : '#ffd200', { size: sfs, weight: 800, anchor: 1 });
          rx -= p.tw('88', sfs, 800, 0) + h * 0.3;
        }
        if ((s.added ?? 0) > 0) {
          const a = `+${s.added}`;
          const afs = h * 0.34;
          const aw = p.tw(a, afs, 800, 0) + h * 0.2;
          p.rect(rx - aw, headY + hd / 2 - afs * 0.7, aw, afs * 1.4, 5, '#ff3b5c');
          p.text(a, rx - aw / 2, headY + hd / 2, '#ffffff', { size: afs, weight: 800, anchor: 0 });
          rx -= aw + h * 0.15;
        }
        p.text(ck[0], rx, headY + hd / 2, ck[2] ? '#ff3b5c' : '#ffffff', { size: h * 0.42, weight: 800, anchor: 1 });
      }
    }
    let y = top ? hd : 0;
    for (const i of order) {
      const t = team(k, i);
      p.rect(0, y, w0, gap, 0, pl.bg);
      y += gap;
      p.rect(0, y, w0, h, 0, t.c1);
      p.rect(0, y, barW, h, 0, t.c2);
      let x = barW;
      if (crestW > 0) crest(p, t, x + (crestW - h * 0.74) / 2, y + h * 0.13, h * 0.74, data);
      x += crestW;
      nameBlock(t, x, y, nameW, false);
      x += nameW;
      if (possW > 0 && poss === i) possMark(p, serve, x + possW / 2, y + h / 2, h, false);
      x += possW;
      if (statW > 0) {
        p.clip(rrect(x, y, statW, h, 0));
        drawStats(p, stats(k, s, i, lang, now), x, statW, y + h / 2, h * 0.24, ink(t.c1), false);
        p.pop();
      }
      x += statW;
      if (setsW > 0) {
        p.rect(x, y, setsW, h, 0, '#00000059');
        p.text(String(won(s)[Math.min(i, 1)]), x + setsW / 2, y + h / 2, '#facc15', { size: h * 0.46, weight: 800, anchor: 0 });
      }
      x += setsW;
      scoreCell(i, t, x, y, scoreW);
      y += h;
    }
    p.pop();
  } else {
    const showClk = k.fields.period || !!ck;
    const shotW = shot ? h * 0.95 : 0;
    const clockW = showClk ? (ck ? h * 2.9 + shotW : h * 2.1) : 0;
    const teamW = barW + crestW + nameW + possW + setsW + scoreW;
    const seq: ([number, boolean] | null)[] = order.map((i, j) => [i, n === 2 && j === 1]);
    if (showClk) {
      if (k.order === 'clockLast') seq.push(null);
      else if (k.order === 'clockMid' && n === 2) seq.splice(1, 0, null);
      else seq.unshift(null);
    }
    let x = 0;
    let clockX = 0;
    const pos: [number, number, boolean][] = [];
    seq.forEach((b, j) => {
      if (j > 0) x += gap;
      if (!b) {
        clockX = x;
        x += clockW;
      } else {
        pos.push([b[0], x, b[1]]);
        x += teamW;
      }
    });
    w0 = x;
    const exH = h * 0.56;
    const hasStats = full && order.some((i) => stats(k, s, i, lang, now).length > 0);
    const tab = !!ck && (s.added ?? 0) > 0;
    h0 = h + (hasStats ? exH : tab ? h * 0.42 : 0);
    p.clip(rrect(0, 0, w0, h, radius));
    p.rect(0, 0, w0, h, 0, pl.bg);
    if (showClk) {
      p.rect(clockX, 0, clockW, h, 0, pl.tag);
      const pf = h * 0.25;
      const cf = h * 0.46;
      const lp = k.fields.period ? pf * 1.1 : 0;
      const lc = ck ? cf * 1.05 : 0;
      const top = (h - lp - lc) / 2;
      const cx = clockX + (clockW - shotW) / 2;
      if (shot) {
        // The shot clock right of the match clock.
        const sx = clockX + clockW - shotW;
        p.rect(sx, h * 0.2, 1, h * 0.6, 0, '#ffffff26');
        p.text(shot[0], sx + shotW / 2, h / 2, shot[1] ? '#ff3b5c' : '#ffd200', { size: h * 0.5, weight: 800, anchor: 0 });
      }
      if (k.fields.period) p.text(per, cx, top + lp / 2, '#c7cbe0', { size: pf, weight: 800, ls: pf * 0.08, anchor: 0 });
      if (ck) p.text(ck[0], cx, top + lp + lc / 2, ck[2] ? '#ff3b5c' : '#ffffff', { size: cf, weight: 800, anchor: 0 });
    }
    for (const [i, bx, mirror] of pos) {
      const t = team(k, i);
      p.rect(bx, 0, teamW, h, 0, t.c1);
      const parts: [number, number][] = [
        [0, barW],
        [1, crestW],
        [2, nameW],
        [3, possW],
        [4, setsW],
        [5, scoreW],
      ];
      if (mirror) parts.reverse();
      let xx = bx;
      for (const [kind, w] of parts) {
        if (w <= 0) continue;
        if (kind === 0) p.rect(xx, 0, w, h, 0, t.c2);
        else if (kind === 1) crest(p, t, xx + (w - h * 0.74) / 2, h * 0.13, h * 0.74, data);
        else if (kind === 2) nameBlock(t, xx, 0, w, mirror);
        else if (kind === 3) {
          if (poss === i) possMark(p, serve, xx + w / 2, h / 2, h, mirror);
        } else if (kind === 4) {
          p.rect(xx, 0, w, h, 0, '#00000059');
          p.text(String(won(s)[Math.min(i, 1)]), xx + w / 2, h / 2, '#facc15', { size: h * 0.46, weight: 800, anchor: 0 });
        } else scoreCell(i, t, xx, 0, w);
        xx += w;
      }
    }
    p.pop();
    if (hasStats)
      for (const [i, bx, mirror] of pos) {
        const t = team(k, i);
        p.rect(bx, h, teamW, exH, 0, '#0b0d18eb');
        p.rect(bx, h, teamW, gap, 0, t.c2);
        p.clip(rrect(bx, h, teamW, exH, 0));
        drawStats(p, stats(k, s, i, lang, now), bx + h * 0.2, teamW - h * 0.4, h + exH / 2, h * 0.3, '#ffffff', mirror);
        p.pop();
      }
    if (tab) {
      const tx = clockX + (clockW - shotW) / 2 - h * 0.45;
      p.rect(tx, h, h * 0.9, h * 0.42, h * 0.1, '#ff3b5c');
      p.rect(tx, h, h * 0.9, h * 0.1, 0, '#ff3b5c');
      p.text(`+${s.added}`, tx + h * 0.45, h + h * 0.21, '#ffffff', { size: h * 0.32, weight: 800, anchor: 0 });
    }
  }

  let totalH = h0;
  if (k.anim.banner) {
    const by = h0 + h * 0.18;
    const bh = h * 0.95;
    totalH = by + bh;
    const ev = s.fx?.ev;
    if (ev) {
      const dur = Math.max(0.5, k.anim.evDur) * 1000;
      const kk = (now - ev.at) / dur;
      if (kk >= 0 && kk < 1) {
        const kIn = easeOut(clamp01(kk / 0.12));
        const kOut = easeIO(clamp01((kk - 0.86) / 0.14));
        const l = kOut * w0;
        const r = w0 - (1 - kIn) * w0;
        if (r > l) {
          p.clip(rrect(l, by, r - l, bh, 0));
          const ec = col(ev.color);
          const ec2: typeof ec = [ec[0], ec[1], ec[2], ec[3] * 0.8];
          p.fillLin(
            rrect(0, by, w0, bh, radius),
            [0, by],
            [w0, by],
            [
              [0, ec],
              [0.7, ec],
              [1, ec2],
            ],
          );
          const fg = ink(ev.color);
          const pulse = 1 + 0.04 * Math.sin(kk * 40) * (1 - clamp01(kk * 4));
          const tfs = h * 0.6;
          const tx = h * 0.35;
          // In the project's language (or the Designer's words).
          const text = fill(tr(k, lang, ev.text), ev.vars);
          const sub = fill(tr(k, lang, ev.sub), ev.vars);
          const tw = p.tw(text, tfs, 800, tfs * 0.02);
          p.text(text, tx + tw / 2, by + bh / 2, fg, { size: tfs, weight: 800, ls: tfs * 0.02, anchor: 0, sx: pulse, sy: pulse, skew: 0.2 });
          const sx = tx + tw + h * 0.35;
          p.clip(rrect(sx, by, Math.max(0, w0 - sx - h * 0.2), bh, 0));
          const a = p.alpha;
          p.alpha = a * 0.9;
          p.text(sub, sx, by + bh / 2, fg, { size: h * 0.32, weight: 800 });
          p.alpha = a;
          p.pop();
          const shx = -0.1 * w0 + 1.2 * w0 * clamp01(kk * 2.2);
          const sw = h * 0.5;
          const sk = bh * 0.36;
          p.alpha = a * 0.3;
          p.fill(
            poly(
              [
                [shx + sk, by],
                [shx + sk + sw, by],
                [shx + sw - sk, by + bh],
                [shx - sk, by + bh],
              ],
              true,
            ),
            '#ffffff',
          );
          p.alpha = a;
          p.pop();
        }
      }
    }
  }
  return [w0, totalH];
}

/** A painter in the board's font. */
const ptOf = (k: K, dry = false) => Object.assign(new Pt(dry), { font: k.font || 'Inter' });
/** Natural size of the board at row height h. */
const sizeAt = (k: K, s: State, data: Data, now: number, h: number, lang = 'fr') => board(ptOf(k, true), k, s, data, now, h, lang);

// --------------------------------------------------------------- operator --

type OpP = { layer: Config; state: unknown; data: Data; act: (op: string, args?: unknown) => void; item?: unknown; lang?: string };

/** The point buttons of a sport: one per value (highest first), with the first preset button of that value. */
function pointTypes(k: K): [number, number][] {
  const pr = P(k);
  if (pr.kind !== 'points') return [[1, 0]];
  const seen = new Map<number, number>();
  pr.pts.forEach((pt, j) => !seen.has(pt.v) && seen.set(pt.v, j));
  return [...seen.entries()].sort((a, b) => b[0] - a[0]);
}

/** A small text pad of the centre (a team's foul, a shot clock value…). */
function Pad({ text, title, disabled, color, onClick }: { text: string; title: string; disabled?: boolean; color?: string; onClick: () => void }) {
  return (
    <button className="cc-sq txt" title={title} disabled={disabled} style={color ? { color } : undefined} onClick={onClick}>
      <span className="cc-txt">{text}</span>
    </button>
  );
}

/** The score typed by hand: one small field per team (placeholder: its score now) and the Régler square → `set`. */
function ScoreSet({ n, names, scores, onSet }: { n: number; names: string[]; scores: number[]; onSet: (scores: string[]) => void }) {
  const [v, setV] = useState<string[]>([]);
  const dirty = v.some((x) => x?.trim());
  const send = () => {
    if (!dirty) return;
    onSet(Array.from({ length: n }, (_, i) => (v[i]?.trim() ? v[i].trim() : String(scores[i]))));
    setV([]);
  };
  return (
    <>
      {Array.from({ length: n }, (_, i) => (
        <input
          key={i}
          className="cc-fld mono"
          style={{ width: 56 }}
          inputMode="numeric"
          title={names[i]}
          placeholder={String(scores[i])}
          value={v[i] ?? ''}
          onChange={(e) => setV(Object.assign([...v], { [i]: e.target.value.replace(/[^\d]/g, '') }))}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') send();
            if (e.key === 'Escape') setV([]);
          }}
        />
      ))}
      <button className="cc-sq" disabled={!dirty} title={tc('cc', 'Set')} onClick={send}>
        <ArrowRight size={15} />
      </button>
    </>
  );
}

/**
 * The control centre (web/src/widgets/centre.tsx). Line 1: the score, the period and the clock,
 * the shot clock. Line 2: the clock's Play / Pause, Undo, the next period, the possession, the
 * reset of the match. Then one row per team: − and the + of each point type. Line 3: the clock
 * field, the fouls, timeouts, cards / suspensions, shot clock and shoot-out pads, the swap of the
 * teams — each exposable (the Designer chooses what the Operator gets).
 */
function Board({ layer, state, data, act, lang: lg, item }: OpP) {
  const now = useFrame();
  const lang = lg ?? 'fr';
  const k = cfg(layer);
  const s = (state ?? {}) as State;
  const pr = P(k);
  const n = teamCount(k);
  const order = Array.from({ length: n }, (_, i) => (s.swapped ? n - 1 - i : i));
  const sc = scores(k, s, data, now);
  const tn = tennisOf(s);
  const tennis = pr.kind === 'tennis';
  const ck = clockOn(k, s) ? clockText(k, s, now) : null;
  const hasClock = pr.kind === 'points' && k.clock.dir !== 'none';
  const running = !!s.clock?.running;
  const score = tennis ? `${tn.games.join('–')} · ${[0, 1].map((i) => tennisPts(tn, i)).join('–')}` : sc.join(' – ');
  const period = tennis ? tennisTag(tn) && tr(k, lang, tennisTag(tn)) : tr(k, lang, pr.periods[s.period ?? 0] ?? '');
  const a = activeSource(k, s, data, now);
  // The source in control, when a feed gives the score: ● flux / flux perdu / manuel.
  const src = a.id === 'feed' ? ['#22c55e', t('feed')] : a.id === 'lost' ? ['#f59e0b', t('feed lost')] : a.id === 'priority' ? ['#a594ff', tc('cc', 'manual')] : null;
  const subText = [period, ck ? `${ck[0]}${s.added ? ` +${s.added}` : ''}` : ''].filter(Boolean).join(' · ');
  const sub = src ? (
    <>
      {subText && `${subText} · `}
      <span style={{ color: src[0] }} title={activeText(a, k.source.rule)}>
        ●
      </span>{' '}
      {src[1]}
    </>
  ) : (
    subText || undefined
  );
  const types = pointTypes(k);
  const sh = (i: number) => shortOf(team(k, i));
  const per = s.period ?? 0;
  const next = pr.periods[per + 1];
  const hist = s.hist?.length ?? 0;
  const first = order[0];
  const second = order[1];
  const toggleClock = () => hasClock && act(running ? 'clockPause' : 'clockStart');
  useKeys({
    q: () => act('add', { team: first }),
    m: () => act('add', { team: second }),
    p: toggleClock,
    z: () => act('undo'),
  });
  const shot = shotOn(k, s);
  const shootout = shootoutOn(k, s);
  const cards = k.fields.cards && pr.cards.length > 0;
  const susp = k.fields.susp && !pr.cards.includes('2 min');
  const rows = order.map((i) => (
    <div key={i} className="cc-team">
      <span className="cc-tn" title={team(k, i).name}>
        {sh(i)}
      </span>
      <button className="cc-sq" title={tennis ? t('Remove a point in this game') : t('{team}: cancel its last point', { team: sh(i) })} onClick={() => act('sub', { team: i })}>
        <Minus size={15} />
      </button>
      {types.map(([v, j], x) => {
        const key = x === 0 && n === 2 ? (i === first ? 'Q' : i === second ? 'M' : undefined) : undefined;
        const label = pr.kind === 'points' ? `${sh(i)} +${v}` : t('POINT {team}', { team: sh(i) });
        return (
          <button key={v} className="cc-sq txt" title={key ? `${label} · ${key}` : label} onClick={() => act('add', { team: i, pts: j })}>
            <span className="cc-txt">+{v}</span>
          </button>
        );
      })}
    </div>
  ));
  return (
    <Centre
      name={(layer as { name?: string }).name}
      readout={{ v: score, sub }}
      status={
        shot && running ? (
          <span className="cc-st" title={t('Shot clock')}>
            {shotText(k, s, now)[0]}
          </span>
        ) : undefined
      }
      actions={[
        hasClock && (running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: toggleClock } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: toggleClock }),
        { icon: Undo2, label: t('Undo'), key: 'Z', disabled: !hist, onClick: () => act('undo') },
        k.fields.period && !tennis && { icon: SkipBack, label: t('Prev period'), disabled: per <= 0, onClick: () => act('prevPeriod') },
        k.fields.period && !tennis && { icon: SkipForward, label: next ? `${t('Next period')} · ${tr(k, lang, next)}` : t('Next period'), disabled: !next, onClick: () => act('nextPeriod') },
        k.fields.poss && { icon: ArrowLeftRight, label: `${t(pr.poss)} → ${sh(((s.poss ?? 0) + 1) % n)}`, onClick: () => act('possession') },
        GAP,
        { icon: RotateCcw, label: t('Reset match'), tone: 'red', confirm: item ? t('Reset the whole match (scores, clock, cards)? Undo stays available.') : undefined, onClick: () => act('reset') },
      ]}
      rows={rows}
    >
      <Row label={t('Score')} expose="opScore" exposeValue={k.opScore}>
        <ScoreSet n={n} names={order.map((i) => team(k, i).name)} scores={order.map((i) => sc[i])} onSet={(v) => act('set', { scores: order.reduce<string[]>((acc, i, x) => Object.assign(acc, { [i]: v[x] }), []) })} />
      </Row>
      {hasClock && (
        <Row label={tc('cc', 'Clock')} expose="opClock" exposeValue={k.opClock}>
          <SetInput placeholder="mm:ss" title={t('Set clock')} onSet={(v) => (/^\d{1,3}:\d{2}$/.test(v.trim()) ? void act('clockSet', { time: v.trim() }) : t('Clock format: mm:ss'))} />
        </Row>
      )}
      {hasClock && (
        <Row label={t('Added time')} expose="opAdded" exposeValue={k.opAdded}>
          {[1, 2, 3, 5].map((m) => (
            <Pad key={m} text={`+${m}`} title={s.added === m ? t('Added time: {n} min (click again to clear)', { n: m }) : t('Added time: {n} min', { n: m })} color={s.added === m ? 'var(--accent)' : undefined} onClick={() => act('added', { min: s.added === m ? 0 : m })} />
          ))}
        </Row>
      )}
      {k.fields.fouls && !tennis && (
        <Row label={t('Fouls')} expose="opFouls" exposeValue={k.opFouls}>
          {order.map((i) => (
            <Pad key={i} text={sh(i)} title={`${t('Foul {team}', { team: sh(i) })} · ${ts(s, i).fouls}`} onClick={() => act('foul', { team: i })} />
          ))}
        </Row>
      )}
      {k.fields.timeouts && pr.timeouts > 0 && (
        <Row label={t('Timeouts')} expose="opTimeouts" exposeValue={k.opTimeouts}>
          {order.map((i) => (
            <Pad key={i} text={sh(i)} title={t('Timeout ({n} left)', { n: Math.max(0, pr.timeouts - ts(s, i).toUsed) })} disabled={ts(s, i).toUsed >= pr.timeouts} onClick={() => act('timeout', { team: i })} />
          ))}
        </Row>
      )}
      {(cards || susp) && (
        <Row label={cards ? t('Cards') : t('Suspension')} expose="opSusp" exposeValue={k.opSusp}>
          {order.flatMap((i) =>
            cards
              ? pr.cards.map((c) => <Pad key={`${i}${c}`} text={sh(i)} color={CARD_COL[c]} title={`${t(CARD_NAMES[c] ?? c)} · ${sh(i)}`} onClick={() => act('card', { team: i, color: c })} />)
              : [<Pad key={i} text={sh(i)} title={t('Suspension {team}', { team: sh(i) })} onClick={() => act('suspension', { team: i })} />],
          )}
        </Row>
      )}
      {shot && (
        <Row label={t('Shot clock')} expose="opShot" exposeValue={k.opShot}>
          {[k.shot.len, k.shot.reset].map((v) => (
            <Pad key={v} text={String(v)} title={t('Shot clock: {n} s', { n: v })} onClick={() => act('shot', { s: v })} />
          ))}
        </Row>
      )}
      {shootout && (
        <Row label={tr(k, lang, 'Penalties')} expose="opShootout" exposeValue={k.opShootout}>
          {order.flatMap((i) => [
            <Pad key={`${i}y`} text={`${sh(i)} ✓`} color="#22c55e" title={t('Kick scored · {team}', { team: sh(i) })} onClick={() => act('penalty', { team: i, scored: true })} />,
            <Pad key={`${i}n`} text={`${sh(i)} ✗`} color="#ef4444" title={t('Kick missed · {team}', { team: sh(i) })} onClick={() => act('penalty', { team: i, scored: false })} />,
          ])}
        </Row>
      )}
      <Toggle label={t('Teams swapped')} expose="opSwap" exposeValue={k.opSwap} value={!!s.swapped} onChange={() => act('swap')} />
    </Centre>
  );
}

// -------------------------------------------------------------- inspector --

/** The font families of the project (Assets). */
function FamilyPick({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const families = useFonts((s) => s.families);
  const has = families.some((f) => f.family.toLowerCase() === value.toLowerCase());
  return <Dropdown value={value} options={[...(has ? [] : [{ value, label: value }]), ...families.map((f) => ({ value: f.family, label: f.family, style: { fontFamily: `"${f.family}", Inter, sans-serif` } }))]} onChange={onChange} />;
}

function TeamLogo({ i, label }: { i: number; label: string }) {
  const b = useBound();
  return (
    <Row expose={`teams.${i}.logo`} exposeValue={String(getPath(b.obj, `teams.${i}.logo`) ?? '')} label={label}>
      <AssetPicker value={String(getPath(b.obj, `teams.${i}.logo`) ?? '')} onChange={(v) => b.set(`teams.${i}.logo`, v)} />
    </Row>
  );
}

/** The plainest board: compact, score and clock only. */
const PLAIN: Config = {
  version: 'compact',
  showCrest: false,
  fields: { period: false, clock: true, fouls: false, timeouts: false, cards: false, poss: false, sets: false },
  anim: { banner: false },
};

function applyPreset(l: Config & { props: Record<string, unknown> }, sp: Sport) {
  const pr = PRESETS[sp];
  l.preset = sp;
  l.fields = { ...pr.fields };
  l.digits = pr.digits;
  l.clock = { ...cfg(l).clock, dir: pr.dir, len: pr.len || 45, extra: pr.extra };
  if (!pr.multi && (l.teams?.length ?? 2) > 2) l.teams = l.teams!.slice(0, 2);
  if (pr.kind === 'tennis') l.layout = 'stacked';
}

export default defineWidget({
  id: 'scoreboard',
  type: 'scoreboard',
  label: 'Scoreboard',
  group: 'Sport & interaction',
  icon: Trophy,
  create: () => {
    const [bw, bh] = sizeAt(cfg(PLAIN), {}, {}, 0, 100);
    return { ...PLAIN, props: { width: Math.ceil((bw * 120) / bh), height: 120 } };
  },
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, data, time, state, lang }) => {
    const k = cfg(layer);
    const s = (state ?? {}) as State;
    const now = time.nowMs;
    const [bw, bh] = sizeAt(k, s, data, now, 100, lang ?? 'fr');
    const row = Math.max(1, 100 * Math.min(w / Math.max(1, bw), h / Math.max(1, bh)));
    const p = ptOf(k);
    board(p, k, s, data, now, row, lang ?? 'fr');
    return p.svg(w, h);
  },
  Inspector: ({ layer, edit, data }) => {
    const k = cfg(layer);
    const pr = P(k);
    // eslint-disable-next-line react-hooks/rules-of-hooks
    const lang = useApp((s) => s.project?.language) || 'fr';
    const n = teamCount(k);
    const fit = () =>
      edit((l) => {
        const kk = cfg(l);
        const w = Number(l.props.width ?? 1500);
        const h = Number(l.props.height ?? 170);
        const [bw, bh] = sizeAt(kk, {}, data, Date.now(), 100);
        const row = 100 * Math.min(w / bw, h / bh);
        l.props.width = Math.ceil((bw * row) / 100);
        l.props.height = Math.ceil((bh * row) / 100);
      });
    // Teams not stored yet (defaults) are written out before the first edit of one of them.
    const ensure = (i: number) => () =>
      !layer.teams?.[i] &&
      edit((l) => {
        const ks = cfg(l);
        l.teams = Array.from({ length: Math.max(n, ks.teams.length) }, (_, j) => ({ ...team(ks, j) }));
      });
    const points = pr.kind === 'points';
    return (
      <>
        <Section id="sb-sport" title={t('Sport')}>
          <Select expose="preset" exposeValue={k.preset} label={t('Preset')} value={k.preset} onChange={(v) => edit((l) => applyPreset(l as never, v as Sport))} options={(Object.keys(PRESETS) as Sport[]).map((sp) => [sp, t(PRESETS[sp].name)] as const)} />
          {pr.kind === 'tennis' && (
            <Seg
              label={t('Best of')}
              path="bestOf"
              value={3}
              options={[
                [3, '3 sets'],
                [5, '5 sets'],
              ]}
            />
          )}
        </Section>
        <Section id="sb-teams" title={t('Teams')}>
          {Array.from({ length: n }, (_, i) => {
            const tm = team(k, i);
            return (
              <Section key={i} id={`sb-team-${i}`} title={t('Team {n}', { n: i + 1 })} level={2}>
                <div onFocusCapture={ensure(i)} onPointerDownCapture={ensure(i)}>
                  <Text label={t('Name')} path={`teams.${i}.name`} value={tm.name} placeholder={t('Full name')} />
                  <Text label={t('Short')} path={`teams.${i}.short`} value={tm.short} placeholder={autoShort(tm.name)} />
                  <ColorPick label={t('Primary')} path={`teams.${i}.c1`} value={tm.c1} />
                  <ColorPick label={t('Secondary')} path={`teams.${i}.c2`} value={tm.c2} />
                  <Content label={t('Score feed')} path={`teams.${i}.score`} value={tm.score} placeholder="{{key}}" />
                </div>
              </Section>
            );
          })}
          {(pr.multi || n > 2) && (
            <Btns
              list={[
                pr.multi && { label: t('+ Add team'), onClick: () => edit((l) => void (l.teams = [...Array.from({ length: n }, (_, j) => ({ ...team(cfg(l), j) })), { ...SAMPLE[n % 4] }])) },
                n > 2 && { label: t('− Last team'), cls: 'ghost', onClick: () => edit((l) => void (l.teams = (l.teams ?? []).slice(0, n - 1))) },
              ]}
            />
          )}
        </Section>
        <Section id="sb-layout" title={t('Layout')}>
          <Seg
            label={t('Version')}
            path="version"
            value={k.version}
            options={[
              ['compact', t('Compact')],
              ['full', t('Full')],
            ]}
          />
          {pr.kind !== 'tennis' && (
            <Seg
              label={t('Layout')}
              path="layout"
              value={k.layout}
              options={[
                ['horizontal', t('Horizontal')],
                ['stacked', t('Stacked')],
              ]}
            />
          )}
          {pr.kind !== 'tennis' && (
            <Seg
              label={t('Order')}
              path="order"
              value={k.layout === 'stacked' && k.order === 'clockMid' ? 'clockFirst' : k.order}
              options={[['clockFirst', t('Clock first')], ...(k.layout === 'stacked' ? [] : [['clockMid', t('Between')] as const]), ['clockLast', t('Clock last')]]}
            />
          )}
          {k.version === 'full' && <Num label={t('Name space')} path="nameW" value={k.nameW} min={2.5} max={9} step={0.1} precision={1} unit="×h" />}
          {pr.kind !== 'tennis' && (
            <Seg
              label={t('Score width')}
              path="digits"
              value={k.digits}
              options={[
                [2, t('2 digits')],
                [3, t('3 digits')],
              ]}
            />
          )}
          <Num label={t('Corners')} path="radius" value={k.radius} min={0} max={30} unit="px" />
          <ColorPick label={t('Background')} path="bg" value={k.bg} />
          <Row label={t('Font')}>
            <FamilyPick value={k.font} onChange={(v) => edit((l) => void (l.font = v))} />
          </Row>
          <Btns list={[{ label: t('Fit box'), title: t('Fit the box to the board'), onClick: fit }]} />
        </Section>
        <Section id="sb-elements" title={t('Elements')}>
          <Toggle label={t('Period')} path="fields.period" value={k.fields.period} />
          {pr.kind === 'sets' && <Toggle label={t('Sets')} path="fields.sets" value={k.fields.sets} />}
          <Toggle label={t(pr.poss)} path="fields.poss" value={k.fields.poss} />
          {pr.kind !== 'tennis' && <Toggle label={t('Fouls')} path="fields.fouls" value={k.fields.fouls} />}
          {pr.timeouts > 0 && <Toggle label={t('Timeouts')} path="fields.timeouts" value={k.fields.timeouts} />}
          {points && <Toggle label={t('Shot clock')} path="fields.shot" value={k.fields.shot} />}
          {pr.cards.length > 0 && <Toggle label={t('Cards')} path="fields.cards" value={k.fields.cards} />}
          {pr.periods.includes('Penalties') && <Toggle label={t('Shoot-out')} path="fields.shootout" value={k.fields.shootout} />}
          {(k.preset === 'handball' || k.preset === 'rugby' || k.preset === 'generic') && <Toggle label={t('Suspensions (countdown)')} path="fields.susp" value={k.fields.susp} />}
          <Toggle label={t('Crests')} path="showCrest" value={k.showCrest} />
          {k.showCrest &&
            Array.from({ length: n }, (_, i) => {
              const tm = team(k, i);
              return (
                <div key={i} onFocusCapture={ensure(i)} onPointerDownCapture={ensure(i)}>
                  <Seg
                    label={shortOf(tm)}
                    path={`teams.${i}.crest`}
                    value={tm.crest}
                    options={[
                      ['shield', t('Shield')],
                      ['round', t('Round')],
                      ['diamond', t('Diamond')],
                    ]}
                  />
                  <TeamLogo i={i} label={t('Picture')} />
                </div>
              );
            })}
        </Section>
        {points && (
          <Section id="sb-clock" title={t('Clock')}>
            <Toggle label={t('Clock')} path="fields.clock" value={k.fields.clock} />
            {k.fields.clock && (
              <>
                <Seg
                  label={t('Direction')}
                  path="clock.dir"
                  value={k.clock.dir}
                  options={[
                    ['up', t('Up ↑')],
                    ['down', t('Down ↓')],
                    ['none', t('None')],
                  ]}
                />
                <Num label={t('Period')} path="clock.len" value={k.clock.len} min={1} max={90} unit="min" />
                <Num label={t('Extra period')} path="clock.extra" value={k.clock.extra} min={0} max={30} unit="min" />
                {k.clock.dir === 'down' && <Toggle label={t('Tenths')} path="clock.tenths" value={k.clock.tenths} />}
                {k.fields.shot && (
                  <>
                    <Num label={t('Shot clock')} path="shot.len" value={k.shot.len} min={5} max={60} unit="s" />
                    <Num label={t('Reset')} path="shot.reset" value={k.shot.reset} min={1} max={60} unit="s" />
                  </>
                )}
                {k.fields.susp && <Num label={t('Suspension')} path="suspLen" value={suspMin(k)} min={0.5} max={20} step={0.5} precision={1} unit="min" />}
              </>
            )}
          </Section>
        )}
        <Section id="sb-texts" title={t('Texts')} closed>
          {textsOf(k).map((en) => (
            <Text key={en} label={tl(lang, en)} value={k.texts[en] ?? ''} placeholder={tl(lang, en)} onChange={(v) => edit((l) => void (l.texts = { ...(l.texts ?? {}), [en]: v }), `sb-text-${en}`)} />
          ))}
        </Section>
        <Section id="sb-anim" title={t('Animation')}>
          <Seg
            label={t('Score')}
            path="anim.score"
            value={k.anim.score}
            options={[
              ['flip', t('Flip')],
              ['highlight', t('Highlight')],
              ['none', t('None')],
            ]}
          />
          <Toggle label={t('Banners')} path="anim.banner" value={k.anim.banner} />
          {k.anim.banner && <Num label={t('Duration')} path="anim.evDur" value={k.anim.evDur} min={1} max={8} step={0.1} precision={1} unit="s" />}
        </Section>
        <Section id="sb-control" title={t('Control')}>
          {points && (
            <>
              <Seg
                label={t('Priority')}
                path="source.rule"
                value={k.source.rule}
                options={[
                  ['override', t('Override'), t('Manual override, then feed')],
                  ['manual', t('Manual'), t('Manual wins')],
                  ['feed', t('Feed'), t('Feed wins')],
                ]}
              />
              {k.source.rule === 'override' && <Num label={t('Override')} path="source.overS" value={k.source.overS} min={5} max={120} unit="s" />}
            </>
          )}
          <Num label={t('Debounce')} path="debounce" value={k.debounce} min={0} max={1000} step={50} unit="ms" />
        </Section>
      </>
    );
  },
  Operator: ({ layer, state, data, act, lang, item }) => <Board layer={layer} state={state} data={data} act={act} lang={lang} item={item} />,
  warnings: (layer, { data, state, r }) => {
    const k = cfg(layer);
    const s = (state ?? {}) as State;
    const now = Date.now();
    const out: Warning[] = [];
    const n = teamCount(k);
    if (s.fx?.ignAt && now - s.fx.ignAt < 4000) out.push({ level: 'warn', op: true, text: t('Double press ignored ({n} so far) — the second press within {ms} ms counted once. Undo is available.', { n: s.ignored ?? 0, ms: k.debounce }) });
    if ((s.version ?? k.version) === 'full') {
      const pad = new Pt(true);
      const long: string[] = [];
      for (let i = 0; i < n; i++) {
        const tm = team(k, i);
        // Scale-free: the name space is nameW × h, the text 0.4 h.
        if (pad.tw(tm.name.toUpperCase(), 40, 800, 0.8) > 100 * k.nameW - 30) long.push(tm.name);
      }
      if (long.length) out.push({ level: 'info', text: t('Name too long for its space: short name shown for {names}.', { names: long.join(', ') }) });
    }
    if (k.teams.some((t) => String(t.short ?? '').trim().length > 3)) out.push({ level: 'info', text: t('Short names are cut to 3 letters.') });
    const sc = scores(k, s, data, now);
    const big = sc.find((v) => String(v).length > k.digits);
    if (big != null) out.push({ level: 'warn', op: true, text: t('Score with {n} digits in a {cell}-digit cell: compressed to keep the layout. Set “Score width” to 3 digits.', { n: String(big).length, cell: k.digits }) });
    const fv = feedValues(k, data);
    if (fv.some((v) => v !== undefined) && P(k).kind === 'points') {
      const a = activeSource(k, s, data, now);
      const manual = Array.from({ length: n }, (_, i) => ts(s, i).score).join('–');
      const feed = fv.map((v, i) => (typeof v === 'number' ? v : ts(s, i).score)).join('–');
      if (a.id === 'lost') out.push({ level: 'error', op: true, text: t('Feed lost (data not a number): manual control (automatic fallback).') });
      else if (feed !== manual && s.src?.dirty) out.push({ level: 'warn', op: true, text: t('Conflict: feed {feed} · manual {manual} → {source} is on air.', { feed, manual, source: activeText(a, k.source.rule) }) });
    }
    if (fv.some((v) => v !== undefined) && P(k).kind !== 'points') out.push({ level: 'info', op: true, text: t('The score feed covers point sports only: manual here.') });
    if (k.clock.dir === 'down' && clockOn(k, s) && clockText(k, s, now)[2]) out.push({ level: 'info', op: true, text: t('Period over: clock at 0. Next period to continue.') });
    if (P(k).kind === 'tennis' && tennisOf(s).free) out.push({ level: 'warn', op: true, text: t('Tennis free entry: automatic counting is off, the typed points show as they are.') });
    if (r) {
      const w = num(r.width);
      const h = num(r.height);
      const [bw, bh] = sizeAt(k, s, data, now, 100);
      const row = (100 * Math.min(w / bw, h / bh)) | 0;
      if (row < 30) out.push({ level: 'warn', text: t('The box is small for the board (row height {n} px): use “Fit box” or enlarge it.', { n: row }) });
    }
    return out;
  },
});

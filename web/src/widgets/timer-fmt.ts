// Timer logic and formatting — mirror of crates/cg-core/src/wid/timer.rs and
// timer_fmt.rs (keep in sync): tokens, duration and wall-clock segments,
// day / month names (same tables as Rust), zones (Intl), timestamp-based
// values. Everything is a pure function of (settings, state, time).
import { interpolateText } from '../engine/evaluate';
import type { Data, Layer } from '../engine/types';
import { t } from '../i18n';

export type TimerMode = 'clock' | 'date' | 'stopwatch' | 'countdown';
export type StartMode = 'manual' | 'auto' | 'at' | 'ext';
export type AtZero = 'freeze' | 'overrun' | 'restart' | 'out';
export type DateCase = 'none' | 'first' | 'words' | 'upper' | 'lower';

export interface Step {
  on?: boolean;
  at?: number;
  color?: string;
  bg?: string;
  blink?: boolean;
}
export interface City {
  label?: string;
  zone?: string;
}

/** Settings of the layer (flattened into it; every field optional: older projects lack them). */
export type Face = 'digital' | 'analog';
export type Ticks = 'hours' | 'minutes' | 'none';
/** The look of an analog face (colours: '' = none for face and rim, the text colour for the hands). */
export interface Analog {
  seconds?: boolean;
  smooth?: boolean;
  ticks?: Ticks;
  numbers?: boolean;
  face?: string;
  rim?: string;
  hands?: string;
  accent?: string;
}

export interface Config {
  timerMode?: TimerMode;
  /** Digits or an analog face (its look in `analog`). */
  face?: Face;
  analog?: Analog;
  tabular?: boolean;
  zeros?: 'pad' | 'trim';
  hoursAuto?: boolean;
  sep?: string;
  sepBlink?: boolean;
  prefix?: string;
  suffix?: string;
  affix?: { size?: number; color?: string; weight?: number; upper?: boolean };
  reserve?: boolean;
  guide?: boolean;
  fit?: boolean;
  plate?: { on?: boolean; color?: string; radius?: number; padX?: number; padY?: number; accent?: boolean; accentColor?: string; shadow?: boolean };
  steps?: Step[];
  offAir?: boolean;
  tzOffset?: number;
  h12?: boolean;
  am?: string;
  pm?: string;
  source?: string;
  /** A countdown at zero fires this trigger ('' = none). */
  zeroTrigger?: string;
  fine?: number;
  fineUnit?: 's' | 'f';
  cities?: boolean;
  cityList?: City[];
  /** Language of the day and month names ('fr-FR', 'de'…; '' = the project's). Older layers without it keep English. */
  lang?: string;
  case?: DateCase;
  swStart?: number;
  swLimitOn?: boolean;
  swLimit?: number;
  swLimitMode?: 'stop' | 'continue' | 'restart';
  swRound?: 'trunc' | 'near';
  splitHold?: number;
  cdTarget?: 'duration' | 'time';
  cdDuration?: number;
  tDate?: string;
  tTime?: string;
  tZone?: string;
  roundUp?: boolean;
  atZero?: AtZero;
  overColor?: string;
  overBg?: string;
  banner?: boolean;
  bannerText?: string;
  bannerColor?: string;
  startMode?: StartMode;
  startAt?: string;
  /** Seconds the Operator's − / + squares add (1, 10 or 60). */
  adjStep?: number;
}

/** What the operator changed (mirrors `State` in Rust): timestamps only. */
export interface State {
  ctl?: boolean;
  startedAt?: number | null;
  pausedAt?: number | null;
  pausedTotal?: number;
  adj?: number;
  split?: { at: number; v: number } | null;
  targetMs?: number | null;
}

export const DEF = {
  face: 'digital' as Face,
  analog: { seconds: true, smooth: false, ticks: 'minutes' as Ticks, numbers: false, face: '#0f1222e6', rim: '', hands: '', accent: '#ff3b5c' },
  // Digits that do not move: fixed-width figures, width of the widest value.
  tabular: true,
  zeros: 'pad' as 'pad' | 'trim',
  hoursAuto: false,
  sep: ':',
  sepBlink: false,
  prefix: '',
  suffix: '',
  affix: { size: 30, color: '#a594ff', weight: 800, upper: true },
  reserve: true,
  guide: true,
  fit: false,
  plate: { on: false, color: '#140c2e', radius: 22, padX: 46, padY: 14, accent: true, accentColor: '#ff3b5c', shadow: true },
  steps: [] as Step[],
  offAir: true,
  h12: false,
  am: 'AM',
  pm: 'PM',
  source: 'system',
  zeroTrigger: '',
  fine: 0,
  fineUnit: 's' as 's' | 'f',
  cities: false,
  cityList: [] as City[],
  lang: 'en-GB',
  case: 'none' as DateCase,
  swStart: 0,
  swLimitOn: false,
  swLimit: 5400000,
  swLimitMode: 'stop' as 'stop' | 'continue' | 'restart',
  swRound: 'trunc' as 'trunc' | 'near',
  splitHold: 5,
  cdTarget: 'duration' as 'duration' | 'time',
  cdDuration: 300000,
  tDate: '',
  tTime: '20:00:00',
  tZone: '',
  atZero: 'freeze' as AtZero,
  overColor: '#ff3b5c',
  overBg: '',
  banner: false,
  bannerText: "C'est parti !",
  bannerColor: '#ff3b5c',
  startMode: 'manual' as StartMode,
  startAt: '20:00:00',
  adjStep: 10,
};
export type Cfg = typeof DEF & { timerMode?: TimerMode; tzOffset?: number; roundUp?: boolean };
export const cfg = (c: Config): Cfg => ({ ...DEF, ...c, affix: { ...DEF.affix, ...c.affix }, plate: { ...DEF.plate, ...c.plate }, analog: { ...DEF.analog, ...c.analog } }) as Cfg;

// ------------------------------------------------------------- segments --

export type SegKind = 'num' | 'name' | 'lit' | 'sep';
export interface Seg {
  k: SegKind;
  t: string;
}
export const segText = (segs: Seg[], sep: string) => segs.map((g) => (g.k === 'sep' ? sep : g.t)).join('');

type Tok = { lit: string; raw: boolean } | { tok: string };
const TOKENS = ['YYYY', 'yyyy', 'MMMM', 'dddd', 'MMM', 'ddd', 'YY', 'yy', 'MM', 'DD', 'dd', 'HH', 'hh', 'mm', 'ss', 'SS', 'FF', 'M', 'D', 'd', 'H', 'h', 'm', 's', 'S', 'f', 'A'];
const tokCache = new Map<string, Tok[]>();
export function tokenize(fmt: string): Tok[] {
  const hit = tokCache.get(fmt);
  if (hit) return hit;
  const out: Tok[] = [];
  let lit = '';
  let i = 0;
  const flush = () => {
    if (lit) out.push({ lit, raw: false });
    lit = '';
  };
  outer: while (i < fmt.length) {
    if (fmt[i] === '[') {
      const end = fmt.indexOf(']', i + 1);
      if (end >= 0) {
        flush();
        out.push({ lit: fmt.slice(i + 1, end), raw: true });
        i = end + 1;
        continue;
      }
    }
    for (const t of TOKENS) {
      if (fmt.startsWith(t, i)) {
        flush();
        out.push({ tok: t });
        i += t.length;
        continue outer;
      }
    }
    const ch = String.fromCodePoint(fmt.codePointAt(i)!);
    lit += ch;
    i += ch.length;
  }
  flush();
  if (tokCache.size > 500) tokCache.clear();
  tokCache.set(fmt, out);
  return out;
}
export const hasToken = (fmt: string) => tokenize(fmt).some((t) => 'tok' in t);

function pushLit(segs: Seg[], t: string, raw: boolean) {
  if (raw) return void segs.push({ k: 'lit', t });
  let cur = '';
  for (const ch of t) {
    if (ch === ':') {
      if (cur) segs.push({ k: 'lit', t: cur });
      cur = '';
      segs.push({ k: 'sep', t: ':' });
    } else cur += ch;
  }
  if (cur) segs.push({ k: 'lit', t: cur });
}

const pad2 = (n: number) => String(n).padStart(2, '0');
const isDay = (t: string) => t === 'DD' || t === 'D' || t === 'dd' || t === 'd';
const isHour = (t: string) => t === 'HH' || t === 'H' || t === 'hh' || t === 'h';
export type Rounding = 'up' | 'trunc' | 'near';

export function durSegs(ms: number, fmt: string, fps: number, round: Rounding, o: { trim: boolean; hoursAuto: boolean }): Seg[] {
  const toks = tokenize(fmt);
  const has = (f: (t: string) => boolean) => toks.some((t) => 'tok' in t && f(t.tok));
  const hD = has(isDay);
  const hH = has(isHour);
  const hM = has((t) => t === 'mm' || t === 'm');
  const hS = has((t) => t === 'ss' || t === 's');
  const frame = 1000 / Math.max(1, fps);
  const p = has((t) => t === 'FF') ? frame : has((t) => t === 'SS') ? 10 : has((t) => t === 'S' || t === 'f') ? 100 : hS ? 1000 : hM ? 60000 : hH ? 3600000 : 86400000;
  let q = Math.abs(ms) / p;
  q = round === 'up' ? Math.ceil(q - 1e-7) : round === 'near' ? Math.round(q) : Math.floor(q + 1e-7);
  const a = Math.max(0, q * p);
  let rem = a;
  const dd = hD ? Math.floor(rem / 86400000) : 0;
  rem -= dd * 86400000;
  const hh = hH ? Math.floor(rem / 3600000) : 0;
  rem -= hh * 3600000;
  const mm = hM ? Math.floor(rem / 60000) : 0;
  rem -= mm * 60000;
  const ss = hS ? Math.floor(rem / 1000) : 0;
  const f = Math.min(999, Math.round(a % 1000));
  const val = (t: string) => (isDay(t) ? dd : isHour(t) ? hh : t === 'mm' || t === 'm' ? mm : t === 'ss' || t === 's' ? ss : t === 'S' || t === 'f' ? Math.floor(f / 100) : t === 'SS' ? Math.floor(f / 10) : t === 'FF' ? Math.floor((f + 0.01) / frame) : 0);
  const parts: { t: Tok; n: number }[] = toks.map((t) => ({ t, n: 'tok' in t ? val(t.tok) : 0 }));
  if (o.trim || o.hoursAuto) {
    for (;;) {
      const i = parts.findIndex((p) => 'tok' in p.t);
      if (i < 0) break;
      const x = (parts[i].t as { tok: string }).tok;
      if (!(isDay(x) || isHour(x)) || parts[i].n !== 0) break;
      let j = i + 1;
      while (j < parts.length && !('tok' in parts[j].t)) j++;
      if (j >= parts.length) break;
      const y = (parts[j].t as { tok: string }).tok;
      if (!(isHour(y) || y === 'mm' || y === 'm' || y === 'ss' || y === 's')) break;
      parts.splice(i, j - i);
    }
  }
  const segs: Seg[] = [];
  let first = true;
  for (const { t, n } of parts) {
    if (!('tok' in t)) {
      pushLit(segs, t.lit, t.raw);
      continue;
    }
    const two = ['DD', 'dd', 'HH', 'hh', 'mm', 'ss', 'SS', 'FF'].includes(t.tok);
    let txt = two ? pad2(n) : String(n);
    if (first && o.trim && ['DD', 'dd', 'HH', 'hh', 'mm'].includes(t.tok)) txt = String(n);
    first = false;
    segs.push({ k: 'num', t: txt });
  }
  return segs;
}

// ---------------------------------------------------------------- zones --

export interface Comp {
  y: number;
  mo: number;
  d: number;
  h: number;
  mi: number;
  s: number;
  ms: number;
  wd: number;
}
/** '' or 'machine' = the server zone; a name; or { offset } hours. */
export type ZoneSel = { name: string } | { offset: number };
export const zoneNamed = (n: string | undefined): ZoneSel => ({ name: !n || n.trim() === 'machine' ? '' : n.trim() });

const fmts = new Map<string, Intl.DateTimeFormat | null>();
function formatter(tz: string): Intl.DateTimeFormat | null {
  if (!fmts.has(tz)) {
    try {
      fmts.set(tz, new Intl.DateTimeFormat('en-US', { timeZone: tz || undefined, hourCycle: 'h23', year: 'numeric', month: 'numeric', day: 'numeric', hour: 'numeric', minute: 'numeric', second: 'numeric' }));
    } catch {
      fmts.set(tz, null);
    }
  }
  return fmts.get(tz)!;
}
export const validZone = (tz: string) => !tz || !!formatter(tz);

export function compAt(ms: number, z: ZoneSel, defaultTz: string): Comp {
  if ('offset' in z) {
    const d = new Date(ms + z.offset * 3600000);
    return { y: d.getUTCFullYear(), mo: d.getUTCMonth() + 1, d: d.getUTCDate(), h: d.getUTCHours(), mi: d.getUTCMinutes(), s: d.getUTCSeconds(), ms: d.getUTCMilliseconds(), wd: d.getUTCDay() };
  }
  const name = z.name || defaultTz;
  const f = formatter(name) ?? formatter(defaultTz) ?? formatter('')!;
  const o: Record<string, number> = {};
  for (const p of f.formatToParts(new Date(ms))) if (p.type !== 'literal') o[p.type] = parseInt(p.value, 10);
  const c = { y: o.year, mo: o.month, d: o.day, h: o.hour % 24, mi: o.minute, s: o.second, ms: ((ms % 1000) + 1000) % 1000, wd: 0 };
  c.wd = new Date(Date.UTC(c.y, c.mo - 1, c.d)).getUTCDay();
  return c;
}
function tzOffset(ms: number, z: ZoneSel, defaultTz: string) {
  const c = compAt(ms, z, defaultTz);
  return Date.UTC(c.y, c.mo - 1, c.d, c.h, c.mi, c.s) - (ms - (((ms % 1000) + 1000) % 1000));
}
/** Wall-clock date and time in a zone → Unix ms (summer / winter time safe). */
export function zonedMs(y: number, mo: number, d: number, h: number, mi: number, s: number, z: ZoneSel, defaultTz: string): number {
  const g = Date.UTC(y, mo - 1, d, h, mi, s);
  return g - tzOffset(g - tzOffset(g, z, defaultTz), z, defaultTz);
}

// ---------------------------------------------------------------- names --

const MONTHS: Record<string, [string[], string[]]> = {
  en: [
    ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'],
    ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'],
  ],
  fr: [
    ['janvier', 'février', 'mars', 'avril', 'mai', 'juin', 'juillet', 'août', 'septembre', 'octobre', 'novembre', 'décembre'],
    ['janv', 'févr', 'mars', 'avr', 'mai', 'juin', 'juil', 'août', 'sept', 'oct', 'nov', 'déc'],
  ],
  de: [
    ['Januar', 'Februar', 'März', 'April', 'Mai', 'Juni', 'Juli', 'August', 'September', 'Oktober', 'November', 'Dezember'],
    ['Jan', 'Feb', 'Mär', 'Apr', 'Mai', 'Jun', 'Jul', 'Aug', 'Sep', 'Okt', 'Nov', 'Dez'],
  ],
  es: [
    ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre'],
    ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'sept', 'oct', 'nov', 'dic'],
  ],
  it: [
    ['gennaio', 'febbraio', 'marzo', 'aprile', 'maggio', 'giugno', 'luglio', 'agosto', 'settembre', 'ottobre', 'novembre', 'dicembre'],
    ['gen', 'feb', 'mar', 'apr', 'mag', 'giu', 'lug', 'ago', 'set', 'ott', 'nov', 'dic'],
  ],
  pt: [
    ['janeiro', 'fevereiro', 'março', 'abril', 'maio', 'junho', 'julho', 'agosto', 'setembro', 'outubro', 'novembro', 'dezembro'],
    ['jan', 'fev', 'mar', 'abr', 'mai', 'jun', 'jul', 'ago', 'set', 'out', 'nov', 'dez'],
  ],
  nl: [
    ['januari', 'februari', 'maart', 'april', 'mei', 'juni', 'juli', 'augustus', 'september', 'oktober', 'november', 'december'],
    ['jan', 'feb', 'mrt', 'apr', 'mei', 'jun', 'jul', 'aug', 'sep', 'okt', 'nov', 'dec'],
  ],
};
const DAYS: Record<string, [string[], string[]]> = {
  en: [
    ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'],
    ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'],
  ],
  fr: [
    ['dimanche', 'lundi', 'mardi', 'mercredi', 'jeudi', 'vendredi', 'samedi'],
    ['dim', 'lun', 'mar', 'mer', 'jeu', 'ven', 'sam'],
  ],
  de: [
    ['Sonntag', 'Montag', 'Dienstag', 'Mittwoch', 'Donnerstag', 'Freitag', 'Samstag'],
    ['So', 'Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa'],
  ],
  es: [
    ['domingo', 'lunes', 'martes', 'miércoles', 'jueves', 'viernes', 'sábado'],
    ['dom', 'lun', 'mar', 'mié', 'jue', 'vie', 'sáb'],
  ],
  it: [
    ['domenica', 'lunedì', 'martedì', 'mercoledì', 'giovedì', 'venerdì', 'sabato'],
    ['dom', 'lun', 'mar', 'mer', 'gio', 'ven', 'sab'],
  ],
  pt: [
    ['domingo', 'segunda-feira', 'terça-feira', 'quarta-feira', 'quinta-feira', 'sexta-feira', 'sábado'],
    ['dom', 'seg', 'ter', 'qua', 'qui', 'sex', 'sáb'],
  ],
  nl: [
    ['zondag', 'maandag', 'dinsdag', 'woensdag', 'donderdag', 'vrijdag', 'zaterdag'],
    ['zo', 'ma', 'di', 'wo', 'do', 'vr', 'za'],
  ],
};
export const LANGS: [string, string][] = [
  ['fr-FR', 'Français'],
  ['en-GB', 'English (UK)'],
  ['en-US', 'English (US)'],
  ['de-DE', 'Deutsch'],
  ['es-ES', 'Español'],
  ['it-IT', 'Italiano'],
  ['pt-BR', 'Português'],
  ['nl-NL', 'Nederlands'],
];
/** Languages of the day and month names (mirror of NAME_LANGS: never the browser's locale, so every output shows the same names). */
export const NAME_LANGS = ['fr', 'en', 'de', 'es', 'it', 'pt', 'nl'];
const known = (l: string | undefined) => {
  const k = String(l ?? '')
    .trim()
    .split(/[-_]/)[0]
    .toLowerCase();
  return NAME_LANGS.includes(k) ? k : null;
};
/** The names' language of a clock: its own ('fr-FR', 'de'…), else the project's ('' = the project's language), else French (mirror of name_lang). */
export const nameLang = (lang: string | undefined, project?: string) => known(lang) ?? known(project) ?? 'fr';
const langKey = (l: string) => nameLang(l);
export const monthName = (l: string, mo: number, long: boolean) => MONTHS[langKey(l)][long ? 0 : 1][Math.min(12, Math.max(1, mo)) - 1];
export const dayName = (l: string, wd: number, long: boolean) => DAYS[langKey(l)][long ? 0 : 1][wd % 7];

export function applyCase(segs: Seg[], mode: DateCase): Seg[] {
  if (mode === 'upper' || mode === 'lower') return segs.map((g) => ({ ...g, t: mode === 'upper' ? g.t.toUpperCase() : g.t.toLowerCase() }));
  if (mode === 'words')
    return segs.map((g) => {
      let out = '';
      let start = true;
      for (const ch of g.t) {
        out += start && /\p{L}/u.test(ch) ? ch.toUpperCase() : ch;
        start = /\s/.test(ch) || ch === '-';
      }
      return { ...g, t: out };
    });
  if (mode === 'first') {
    let done = false;
    return segs.map((g) => {
      if (done || !/\p{L}/u.test(g.t)) return g;
      done = true;
      return { ...g, t: g.t.replace(/\p{L}/u, (c) => c.toUpperCase()) };
    });
  }
  return segs;
}

export function wallSegs(c: Comp, fmt: string, o: { lang: string; h12: boolean; am: string; pm: string; trim: boolean; fps: number }): Seg[] {
  let toks = tokenize(fmt);
  const has = (f: (t: string) => boolean) => toks.some((t) => 'tok' in t && f(t.tok));
  if (o.h12 && has((t) => t === 'HH' || t === 'H') && !has((t) => t === 'A')) toks = [...toks, { lit: ' ', raw: true }, { tok: 'A' }];
  const h12 = ((c.h + 11) % 12) + 1;
  const hh = o.h12 ? h12 : c.h;
  const frame = 1000 / Math.max(1, o.fps);
  const segs: Seg[] = [];
  let first = true;
  for (const t of toks) {
    if (!('tok' in t)) {
      pushLit(segs, t.lit, t.raw);
      continue;
    }
    let s = '';
    switch (t.tok) {
      case 'YYYY':
      case 'yyyy':
        s = String(c.y).padStart(4, '0');
        break;
      case 'YY':
      case 'yy':
        s = pad2(((c.y % 100) + 100) % 100);
        break;
      case 'MMMM':
        s = monthName(o.lang, c.mo, true);
        break;
      case 'MMM':
        s = monthName(o.lang, c.mo, false);
        break;
      case 'MM':
        s = pad2(c.mo);
        break;
      case 'M':
        s = String(c.mo);
        break;
      case 'DD':
      case 'dd':
        s = pad2(c.d);
        break;
      case 'D':
      case 'd':
        s = String(c.d);
        break;
      case 'dddd':
        s = dayName(o.lang, c.wd, true);
        break;
      case 'ddd':
        s = dayName(o.lang, c.wd, false);
        break;
      case 'HH':
        s = first && o.trim ? String(hh) : pad2(hh);
        break;
      case 'H':
        s = String(hh);
        break;
      case 'hh':
        s = pad2(h12);
        break;
      case 'h':
        s = String(h12);
        break;
      case 'mm':
        s = pad2(c.mi);
        break;
      case 'm':
        s = String(c.mi);
        break;
      case 'ss':
        s = pad2(c.s);
        break;
      case 's':
        s = String(c.s);
        break;
      case 'S':
      case 'f':
        s = String(Math.floor(c.ms / 100));
        break;
      case 'SS':
        s = pad2(Math.floor(c.ms / 10));
        break;
      case 'FF':
        s = pad2(Math.floor((c.ms + 0.01) / frame));
        break;
      case 'A':
        s = c.h < 12 ? o.am : o.pm;
        break;
    }
    first = false;
    segs.push({ k: ['MMMM', 'MMM', 'dddd', 'ddd', 'A'].includes(t.tok) ? 'name' : 'num', t: s });
  }
  return segs;
}

// -------------------------------------------------------------- parsing --

const digits = (s: string, min: number, max: number) => (s.length >= min && s.length <= max && /^\d+$/.test(s) ? parseInt(s, 10) : null);

export function parseHms(s: string): [number, number, number] | null {
  const p = String(s ?? '')
    .trim()
    .split(':');
  if (p.length < 2 || p.length > 3) return null;
  const h = digits(p[0], 1, 2);
  const m = digits(p[1], 2, 2);
  const sec = p.length === 3 ? digits(p[2], 2, 2) : 0;
  if (h == null || m == null || sec == null || h > 23 || m > 59 || sec > 59) return null;
  return [h, m, sec];
}

export function parseDur(s: string): { ms: number } | { err: string } {
  const x = String(s ?? '').trim();
  if (!x) return { err: t('Empty value') };
  if (x.startsWith('-')) return { err: t('Negative values are refused') };
  const bad = { err: t('“{value}” is not a time: use ss, mm:ss or h:mm:ss', { value: x }) };
  const i = x.search(/[.,]/);
  const main = i >= 0 ? x.slice(0, i) : x;
  const frac = i >= 0 ? x.slice(i + 1) : null;
  const parts = main.split(':');
  if (parts.length > 3) return bad;
  let total = 0;
  for (let j = 0; j < parts.length; j++) {
    const n = j === 0 ? digits(parts[j], 1, 9) : digits(parts[j], 1, 2);
    if (n == null) return bad;
    if (j > 0 && n > 59) return { err: t('“{value}”: minutes and seconds go up to 59', { value: x }) };
    total = total * 60 + n;
  }
  let ms = 0;
  if (frac != null) {
    const n = digits(frac, 1, 3);
    if (n == null) return bad;
    ms = Math.round(n * Math.pow(10, 3 - frac.length));
  }
  return { ms: total * 1000 + ms };
}

export type Target = { date: [number, number, number] | null; time: [number, number, number] };
export function parseTarget(s: string): Target | { err: string } {
  const x = String(s ?? '').trim();
  if (!x) return { err: t('The target time is empty') };
  const bad = { err: t('“{value}” is not a target: use 20:00, 20:00:00 or 2026-10-18 20:00', { value: x }) };
  const i = x.search(/[ T]/);
  const date = i >= 0 ? x.slice(0, i) : null;
  const time = parseHms(i >= 0 ? x.slice(i + 1).trim() : x);
  if (!time) return bad;
  if (date == null) return { date: null, time };
  const p = date.split('-');
  if (p.length !== 3) return bad;
  const [y, m, d] = [digits(p[0], 4, 4), digits(p[1], 1, 2), digits(p[2], 1, 2)];
  if (y == null || m == null || d == null) return bad;
  const day = new Date(Date.UTC(y, m - 1, d));
  if (m < 1 || m > 12 || day.getUTCDate() !== d) return { err: t('“{date}” is not a date', { date }) };
  return { date: [y, m, d], time };
}

export function targetMs(t: Target, z: ZoneSel, now: number, defaultTz: string): number {
  let [y, mo, d] = t.date ?? [0, 0, 0];
  if (!t.date) {
    const c = compAt(now, z, defaultTz);
    [y, mo, d] = [c.y, c.mo, c.d];
  }
  return zonedMs(y, mo, d, t.time[0], t.time[1], t.time[2], z, defaultTz);
}

export function plainDur(ms: number): string {
  ms = Math.max(0, Math.round(ms));
  const sec = Math.floor(ms / 1000);
  const f = ms % 1000;
  const h = Math.floor(sec / 3600);
  const t = h > 0 ? `${h}:${pad2(Math.floor(sec / 60) % 60)}:${pad2(sec % 60)}` : `${pad2(Math.floor(sec / 60))}:${pad2(sec % 60)}`;
  return f ? `${t}.${String(f).padStart(3, '0').replace(/0+$/, '')}` : t;
}

// ------------------------------------------------------------- resolved --

export type ClockLayer = Extract<Layer, { type: 'clock' }>;

export interface Eff {
  c: Cfg;
  mode: TimerMode;
  legacy: boolean;
  manualStart: boolean;
  format: string;
  zone: ZoneSel;
  start: StartMode;
  roundUp: boolean;
  atZero: AtZero;
  cdTime: boolean;
  duration: number;
  target: string;
  targetZone: ZoneSel;
  /** Language of the day and month names (one of NAME_LANGS). */
  lang: string;
}

/** "[HH:]mm:ss" or plain seconds (legacy durations, like widgets.rs parse_duration). */
function legacyDuration(s: string): number | null {
  const p = s.trim().split(':');
  if (p.some((x) => !/^\s*[-+]?\d*\.?\d+\s*$/.test(x))) return null;
  const n = p.map(Number);
  if (n.length === 1) return n[0];
  if (n.length === 2) return n[0] * 60 + n[1];
  if (n.length === 3) return n[0] * 3600 + n[1] * 60 + n[2];
  return null;
}

/** The settings that apply (older layers mapped); `project`: the project's language, for a clock that follows it. */
export function eff(l: ClockLayer, data: Data, project = ''): Eff {
  const c = cfg(l);
  const lang = nameLang(c.lang, project);
  const zone: ZoneSel = c.tzOffset != null ? { offset: c.tzOffset } : zoneNamed(l.timezone);
  const format = l.format?.trim() ? l.format : 'HH:mm:ss';
  if (c.timerMode) {
    return {
      c,
      mode: c.timerMode,
      legacy: false,
      manualStart: !!l.manualStart,
      format,
      zone,
      // The layer's manual start (Continue / trigger) carries the wait: its clock starts the
      // timer. Older layers still waiting for Play keep « manual »; « External » behaved as it.
      start: c.startMode === 'at' ? 'at' : l.manualStart ? 'auto' : c.startMode === 'ext' ? 'manual' : c.startMode,
      roundUp: c.roundUp ?? true,
      atZero: c.atZero,
      cdTime: c.cdTarget === 'time',
      duration: Math.max(0, c.cdDuration),
      target: c.tDate.trim() ? `${c.tDate.trim()} ${c.tTime.trim()}` : c.tTime.trim(),
      targetZone: zoneNamed(c.tZone),
      lang,
    };
  }
  const tgt = interpolateText(l.target ?? '', data);
  const map: Record<string, [TimerMode, boolean]> = { time: ['clock', false], elapsed: ['stopwatch', false], timer: ['countdown', false], countdown: ['countdown', true] };
  const [mode, cdTime] = map[l.mode] ?? ['clock', false];
  const d = legacyDuration(tgt);
  return {
    c,
    mode,
    legacy: true,
    manualStart: !!l.manualStart,
    format,
    zone,
    start: 'auto',
    roundUp: c.roundUp ?? cdTime,
    atZero: 'freeze',
    cdTime,
    duration: Math.max(0, d == null ? 0 : Math.round(d * 1000)),
    target: tgt.trim(),
    targetZone: zone,
    lang,
  };
}

export const timed = (e: Eff) => e.mode === 'stopwatch' || e.mode === 'countdown';
export const isTarget = (e: Eff) => e.mode === 'countdown' && e.cdTime;

export interface Run {
  startedAt: number | null;
  pausedAt: number | null;
  pausedTotal: number;
  adj: number;
  split: { at: number; v: number } | null;
  targetMs: number | null;
}

export function startAtMs(e: Eff, now: number, defaultTz: string): number | null {
  const t = parseHms(e.c.startAt);
  if (!t) return null;
  const z = zoneNamed('');
  const c = compAt(now, z, defaultTz);
  return zonedMs(c.y, c.mo, c.d, t[0], t[1], t[2], z, defaultTz);
}

export function run(e: Eff, st: State | undefined, now: number, ageMs: number, defaultTz: string): Run {
  if (st?.ctl) return { startedAt: st.startedAt ?? null, pausedAt: st.pausedAt ?? null, pausedTotal: st.pausedTotal ?? 0, adj: st.adj ?? 0, split: st.split ?? null, targetMs: st.targetMs ?? null };
  const intoAir = now - Math.max(0, ageMs);
  let startedAt: number | null = null;
  if (e.legacy || e.start === 'auto') startedAt = intoAir;
  else if (e.start === 'at') {
    const ts = startAtMs(e, now, defaultTz);
    if (ts != null && now >= ts && now - ts < 6 * 3600000) startedAt = e.c.offAir ? ts : Math.max(ts, intoAir);
  }
  return { startedAt, pausedAt: null, pausedTotal: 0, adj: 0, split: null, targetMs: null };
}
export const started = (e: Eff, r: Run) => isTarget(e) || r.startedAt != null;
const elapsed = (r: Run, now: number) => (r.startedAt == null ? 0 : (r.pausedAt ?? now) - r.startedAt - r.pausedTotal);

export function targetOf(e: Eff, r: Run, now: number, defaultTz: string): number | null {
  if (r.targetMs != null) return r.targetMs;
  const s = e.legacy ? e.target.replace('T', ' ').split('.')[0] : e.target;
  const p = parseTarget(s);
  return 'err' in p ? null : targetMs(p, e.targetZone, now, defaultTz);
}

export function rawValue(e: Eff, r: Run, now: number, defaultTz: string): number | null {
  if (e.mode === 'stopwatch') return e.c.swStart + elapsed(r, now) + r.adj;
  if (e.mode === 'countdown' && e.cdTime) {
    const t = targetOf(e, r, now, defaultTz);
    return t == null ? null : t - ((r.pausedAt ?? now) - r.pausedTotal) + r.adj;
  }
  if (e.mode === 'countdown') return e.duration - elapsed(r, now) + r.adj;
  return 0;
}

export interface Value {
  v: number;
  over: boolean;
  zero: boolean;
  limit: boolean;
  zeroAt: number | null;
}
export function value(e: Eff, r: Run, now: number, defaultTz: string): Value | null {
  let v = rawValue(e, r, now, defaultTz);
  if (v == null) return null;
  const c = e.c;
  const out: Value = { v: 0, over: false, zero: false, limit: false, zeroAt: null };
  if (e.mode === 'stopwatch') {
    if (c.swLimitOn && c.swLimit > c.swStart && v >= c.swLimit) {
      if (c.swLimitMode === 'stop') ((v = c.swLimit), (out.limit = true));
      else if (c.swLimitMode === 'restart') v = c.swStart + ((v - c.swStart) % (c.swLimit - c.swStart));
    }
    out.v = Math.max(0, v);
    return out;
  }
  if (v > 0) return { ...out, v };
  const base = { ...out, zero: true, zeroAt: (r.pausedAt ?? now) + v };
  if (e.atZero === 'overrun') return { ...base, v, over: v < 0 };
  if (e.atZero === 'restart' && !e.cdTime) {
    const p = e.duration + r.adj;
    if (p > 0) {
      const m = ((v % p) + p) % p;
      return { ...base, v: m === 0 ? p : m, zero: false };
    }
  }
  return base;
}

// -------------------------------------------------------------- display --

export interface Disp {
  segs: Seg[];
  color: string | null;
  bg: string | null;
  blink: boolean;
  split: boolean;
  hidden: boolean;
  v: number | null;
  over: boolean;
  zeroAt: number | null;
}
export interface At {
  now: number;
  ageMs: number;
  fps: number;
  defaultTz: string;
}

const wallOpts = (e: Eff, fps: number) => ({ lang: e.lang, h12: e.c.h12, am: e.c.am, pm: e.c.pm, trim: e.c.zeros === 'trim', fps });
export const clockMs = (e: Eff, now: number, fps: number) => now + Math.round(e.c.fineUnit === 'f' ? (e.c.fine * 1000) / Math.max(1, fps) : e.c.fine * 1000);

export function wall(e: Eff, ms: number, zone: ZoneSel, fmt: string, fps: number, defaultTz: string): Seg[] {
  const segs = wallSegs(compAt(ms, zone, defaultTz), fmt, wallOpts(e, fps));
  return e.mode === 'date' ? applyCase(segs, e.c.case) : segs;
}

function stepFor(e: Eff, v: number): Step | null {
  let best: Step | null = null;
  for (const s of e.c.steps) {
    if (s.on === false || !Number.isFinite(+(s.at ?? NaN))) continue;
    const at = +s.at! * 1000;
    const ok = e.mode === 'countdown' ? v <= at && (!best || +s.at! < +best.at!) : v >= at && (!best || +s.at! > +best.at!);
    if (ok) best = s;
  }
  return best;
}
const rounding = (e: Eff, over: boolean): Rounding => (e.mode === 'countdown' ? (over || !e.roundUp ? 'trunc' : 'up') : e.c.swRound === 'near' ? 'near' : 'trunc');
export const dur = (e: Eff, ms: number, fps: number, round: Rounding) => durSegs(ms, e.format, fps, round, { trim: e.c.zeros === 'trim', hoursAuto: e.c.hoursAuto });

export function display(e: Eff, st: State | undefined, at: At, zone?: ZoneSel): Disp {
  const out: Disp = { segs: [], color: null, bg: null, blink: false, split: false, hidden: false, v: null, over: false, zeroAt: null };
  if (!timed(e)) {
    out.segs = wall(e, clockMs(e, at.now, at.fps), zone ?? e.zone, e.format, at.fps, at.defaultTz);
    return out;
  }
  const r = run(e, st, at.now, at.ageMs, at.defaultTz);
  const val = value(e, r, at.now, at.defaultTz);
  if (!val) {
    out.segs = [
      { k: 'lit', t: '--' },
      { k: 'sep', t: ':' },
      { k: 'lit', t: '--' },
    ];
    return out;
  }
  let v = val.v;
  if (e.mode === 'stopwatch' && r.split && at.now >= r.split.at && at.now - r.split.at < e.c.splitHold * 1000) {
    v = r.split.v;
    out.split = true;
  }
  out.v = v;
  out.over = val.over;
  out.zeroAt = val.zeroAt;
  if (e.mode === 'countdown' && e.atZero === 'out' && val.zero) out.hidden = true;
  out.segs = dur(e, v, at.fps, rounding(e, val.over));
  if (val.over) {
    out.segs.unshift({ k: 'lit', t: '+' });
    out.color = e.c.overColor || null;
    out.bg = e.c.overBg || null;
    return out;
  }
  const s = stepFor(e, v);
  if (s) {
    out.color = s.color || null;
    out.bg = s.bg || null;
    out.blink = !!s.blink;
  }
  return out;
}

/** Angles of the hour, minute and second hands, degrees from 12 (mirror of timer::hands). */
export function hands(e: Eff, st: State | undefined, at: At, zone?: ZoneSel): [number, number, number] | null {
  let secs: number;
  if (timed(e)) {
    const d = display(e, st, at, zone);
    if (d.hidden || d.v == null) return null;
    secs = Math.abs(d.v) / 1000;
  } else {
    const c = compAt(clockMs(e, at.now, at.fps), zone ?? e.zone, at.defaultTz);
    secs = c.h * 3600 + c.mi * 60 + c.s + c.ms / 1000;
  }
  const s = e.c.analog.smooth ? secs : Math.floor(secs);
  const mod = (x: number) => ((x % 360) + 360) % 360;
  return [mod(s / 120), mod(s / 10), mod(s * 6)];
}

export function widest(e: Eff, d: Disp, sep: string, fps: number): string[] {
  const dig = (s: string) => s.replace(/\d/g, '8');
  const out = [dig(segText(d.segs, sep))];
  if (!timed(e)) {
    for (let i = 0; i < 12; i++) {
      const segs = wallSegs({ y: 2026, mo: i + 1, d: 28, h: i % 2 ? 20 : 8, mi: 28, s: 28, ms: 888, wd: i % 7 }, e.format, wallOpts(e, fps));
      out.push(dig(segText(e.mode === 'date' ? applyCase(segs, e.c.case) : segs, sep)));
    }
    return out;
  }
  const hasH = tokenize(e.format).some((t) => 'tok' in t && isHour(t.tok));
  const maxV = e.mode === 'stopwatch' ? (e.c.swLimitOn ? Math.max(e.c.swLimit, e.c.swStart) : Math.max(e.c.swStart, hasH ? 36e6 - 1 : 36e5 - 1)) : e.cdTime ? Math.max(0, d.v ?? 0) : Math.max(0, e.duration);
  const m = dig(segText(dur(e, maxV, fps, 'trunc'), sep));
  if (e.mode === 'countdown' && e.atZero === 'overrun') out.push('+' + m);
  out.push(m);
  return out;
}

/** The zero banner: [scale, opacity] while it shows. */
export function banner(e: Eff, d: Disp, now: number): [number, number] | null {
  if (e.mode !== 'countdown' || !e.c.banner || !e.c.bannerText.trim() || d.zeroAt == null) return null;
  const dt = now - d.zeroAt;
  if (dt < 0 || dt > 4500) return null;
  let k = Math.min(1, Math.max(0, dt / 350));
  k = 1 - Math.pow(1 - k, 3);
  if (dt > 4000) k *= 1 - (dt - 4000) / 500;
  return [0.6 + 0.4 * k, k];
}

export function affixes(c: Cfg, data: Data): [string, string] {
  const f = (s: string) => {
    const t = interpolateText(s, data);
    return c.affix.upper ? t.toUpperCase() : t;
  };
  return [f(c.prefix), f(c.suffix)];
}

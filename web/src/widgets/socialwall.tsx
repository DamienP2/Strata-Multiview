// Social wall: the approved messages of a social source as a stack of cards (avatar or initials, name, platform
// badge, city, text on a few lines, photo); a card coming in slides and fades in, the others move, a card leaving
// fades out (the change is in the state, played from the wall clock).
// Same picture as crates/cg-render/src/wid/socialwall.rs; layout mirrors crates/cg-core/src/wid/socialwall.rs.
import { MessagesSquare } from 'lucide-react';
import { interpolateText } from '../engine/evaluate';
import type { Data } from '../engine/types';
import { useFonts } from '../engine/fonts';
import { Dropdown } from '../ui/Dropdown';
import { ColorPick, Content, defineWidget, headerRows, Num, RangeNum, Row, Section, Seg, Toggle, type Warning } from './kit';
import { ellipse, poly, rrect, tf, Pt, type Cmd } from './scoreboard-paint';
import { t } from '../i18n';

export interface Config {
  /** The messages: `{{key}}` of a field holding a JSON list of messages (or a data set's table). */
  items?: string;
  /** The messages typed by hand, kept while a data set feeds the wall (back by hand, they return). */
  handItems?: string;
  /** Cards shown at most (the first ones of the list). */
  max?: number;
  anchor?: 'top' | 'bottom';
  gap?: number;
  pad?: number;
  radius?: number;
  bg?: string;
  nameColor?: string;
  textColor?: string;
  /** The city. */
  muted?: string;
  font?: string;
  nameSize?: number;
  textSize?: number;
  /** Lines of text at most ("…" after). */
  lines?: number;
  avatar?: boolean;
  avatarSize?: number;
  badge?: boolean;
  city?: boolean;
  photo?: boolean;
  photoH?: number;
  /** A left edge in the platform's colour. */
  edge?: boolean;
  edgeW?: number;
  /** A card coming in or leaving, seconds (0 = cut). */
  anim?: number;
}
/** A message of the list (mirrors `Msg`). */
export interface Msg {
  id: string;
  platform: string;
  platformName: string;
  name: string;
  handle: string;
  city: string;
  text: string;
  avatar: string;
  photo: string;
  amount: string;
  time: string;
}
/** The last change of the cards shown (mirrors `State`). */
export interface State {
  prev?: string[];
  gone?: Partial<Msg>[];
  at?: number;
}
type WallLayer = Config & { id?: string };

export function cfg(c: Config) {
  return {
    items: c.items ?? '',
    max: c.max ?? 3,
    anchor: c.anchor ?? 'top',
    gap: c.gap ?? 12,
    pad: c.pad ?? 16,
    radius: c.radius ?? 14,
    bg: c.bg ?? '#f2f3f5',
    nameColor: c.nameColor ?? '#0b0d18',
    textColor: c.textColor ?? '#1f2433',
    muted: c.muted ?? '#6b7280',
    font: c.font ?? 'Inter',
    nameSize: c.nameSize ?? 20,
    textSize: c.textSize ?? 20,
    lines: c.lines ?? 3,
    avatar: c.avatar ?? true,
    avatarSize: c.avatarSize ?? 40,
    badge: c.badge ?? true,
    city: c.city ?? true,
    photo: c.photo ?? true,
    photoH: c.photoH ?? 130,
    edge: c.edge ?? false,
    edgeW: c.edgeW ?? 6,
    anim: c.anim ?? 0.6,
  };
}
type Cfg = ReturnType<typeof cfg>;

// --------------------------------------------------------------- messages --

const cell = (v: unknown) => (typeof v === 'string' ? v.trim() : typeof v === 'number' ? String(v) : '');
/** A message from its keys (a table's columns), matched by name whatever their case (mirrors `Msg::of`). */
export function msgOf(o: Record<string, unknown>): Msg {
  const keys = Object.keys(o);
  const g = (k: string) => {
    const hit = k in o ? k : keys.find((x) => x.toLowerCase() === k.toLowerCase());
    return hit === undefined ? '' : cell(o[hit]);
  };
  const m: Msg = { id: g('id'), platform: g('platform').toLowerCase(), platformName: g('platformName'), name: g('name'), handle: g('handle'), city: g('city'), text: g('text'), avatar: g('avatar'), photo: g('photo'), amount: g('amount'), time: g('time') };
  if (!m.name) m.name = m.handle;
  if (!m.id) m.id = `${m.name}|${m.text}`;
  return m;
}
const fullMsg = (m: Partial<Msg>): Msg => msgOf(m as Record<string, unknown>);
const hasText = (m: Msg) => !!(m.text || m.name);

/**
 * The messages of a value: a JSON list of objects, a data set's table
 * (`[[header…], [cells…]…]`: its columns by name, a row's id its index when
 * there is no id column) or an object with one under `approved` (mirrors `parse`).
 */
export function parse(text: string): Msg[] {
  const t0 = text.trim();
  if (!t0.startsWith('[') && !t0.startsWith('{')) return [];
  let v: unknown;
  try {
    v = JSON.parse(t0);
  } catch {
    return [];
  }
  const list: unknown[] = Array.isArray(v) ? v : v && typeof v === 'object' && Array.isArray((v as { approved?: unknown }).approved) ? (v as { approved: unknown[] }).approved : [];
  const tb = headerRows(list);
  if (tb) {
    const head = tb.head.map((h) => h.trim());
    const ids = head.some((h) => h.toLowerCase() === 'id');
    return tb.rows
      .map((r, i) => {
        const m = msgOf(Object.fromEntries(head.map((k, j) => [k, r[j] ?? ''])));
        if (!ids) m.id = String(i);
        return m;
      })
      .filter(hasText);
  }
  return list
    .filter((x): x is Record<string, unknown> => !!x && typeof x === 'object' && !Array.isArray(x))
    .map(msgOf)
    .filter(hasText);
}

export const messages = (c: Cfg, data: Data) => parse(interpolateText(c.items, data)).slice(0, Math.max(1, c.max));

// -------------------------------------------------------------- platforms --

export type Glyph = 'play' | 'bubble' | 'f' | 'camera' | 'x' | 'note' | 'phone' | 'dots' | 'letter';
/** Colour and glyph of a platform (mirrors `platform`). */
export function platform(p: string): [string, Glyph] {
  switch (p.trim().toLowerCase()) {
    case 'youtube':
      return ['#ff0033', 'play'];
    case 'twitch':
      return ['#9146ff', 'bubble'];
    case 'facebook':
      return ['#1877f2', 'f'];
    case 'instagram':
      return ['#e1306c', 'camera'];
    case 'x':
    case 'twitter':
      return ['#000000', 'x'];
    case 'tiktok':
      return ['#000000', 'note'];
    case 'whatsapp':
      return ['#25d366', 'phone'];
    case 'sms':
      return ['#64748b', 'dots'];
    default:
      return ['#64748b', 'letter'];
  }
}
const TIKTOK_EDGES = ['#25f4ee', '#fe2c55'];
const AVATARS = ['#93c5fd', '#f9a8d4', '#86efac', '#fcd34d', '#c4b5fd', '#fdba74', '#67e8f9', '#fca5a5'];
const AVATAR_INK = '#0b0d18';
export function avatarColor(name: string): string {
  let h = 0;
  for (const ch of name.trim()) h = (Math.imul(h, 31) + ch.codePointAt(0)!) >>> 0;
  return AVATARS[h % AVATARS.length];
}
export const initials = (name: string) =>
  name
    .split(/\s+/)
    .map((w) => [...w.replace(/^@+/, '')].find((ch) => /[\p{L}\p{N}]/u.test(ch)))
    .filter((ch): ch is string => !!ch)
    .slice(0, 2)
    .join('')
    .toUpperCase();

// ----------------------------------------------------------------- layout --

type Measure = (s: string, size: number, weight: number) => number;
const NAME_WEIGHT = 700;
const TEXT_WEIGHT = 500;
const CITY_WEIGHT = 500;

function ellipsize(s: string, maxW: number, size: number, weight: number, m: Measure): string {
  if (m(s, size, weight) <= maxW) return s;
  const cs = [...s];
  while (cs.length) {
    cs.pop();
    const t1 = `${cs.join('').trimEnd()}…`;
    if (m(t1, size, weight) <= maxW) return t1;
  }
  return '';
}

/** The text in lines of at most maxW, k lines at most (mirrors `wrap`). */
export function wrap(text: string, maxW: number, k: number, size: number, weight: number, m: Measure): string[] {
  let lines: string[] = [];
  for (const para of text.split('\n')) {
    let cur = '';
    // Not at a no-break space (French « ? », « : »…).
    for (const word of para.split(/[^\S\u00a0\u202f]+/).filter(Boolean)) {
      const cand = cur ? `${cur} ${word}` : word;
      if (cur && m(cand, size, weight) > maxW) {
        lines.push(cur);
        cur = word;
      } else cur = cand;
    }
    lines.push(cur);
  }
  while (lines.length && !lines[lines.length - 1]) lines.pop();
  k = Math.max(1, k);
  const cut = lines.length > k;
  lines = lines.slice(0, k);
  if (cut && lines.length) {
    let last = ellipsize(`${lines[lines.length - 1]}…`, maxW, size, weight, m);
    if (!last.endsWith('…')) last += '…';
    lines[lines.length - 1] = last;
  }
  return lines.map((l) => ellipsize(l, maxW, size, weight, m));
}

interface Card {
  h: number;
  left: number;
  x0: number;
  name: string;
  nameW: number;
  city: string;
  lines: string[];
  /** Top, width, height. */
  photo: [number, number, number] | null;
}
const headerH = (c: Cfg) => c.nameSize * 1.3;
const lineH = (c: Cfg) => c.textSize * 1.3;
const badgeSize = (c: Cfg) => Math.round(c.nameSize * 0.9);

function card(c: Cfg, msg: Msg, w: number, m: Measure): Card {
  const pad = Math.max(0, c.pad);
  const a = c.avatar ? Math.max(0, c.avatarSize) : 0;
  const left = (c.edge ? Math.max(0, c.edgeW) : 0) + pad;
  const x0 = left + (a > 0 ? a + pad * 0.75 : 0);
  const cw = Math.max(10, w - x0 - pad);
  const gap = c.nameSize * 0.3;
  const bs = c.badge ? badgeSize(c) + gap : 0;
  const name = ellipsize(msg.name, Math.max(0, cw - bs), c.nameSize, NAME_WEIGHT, m);
  const nameW = m(name, c.nameSize, NAME_WEIGHT);
  let city = '';
  if (c.city && msg.city) {
    const s = `· ${msg.city}`;
    if (m(s, c.nameSize * 0.85, CITY_WEIGHT) <= cw - nameW - bs - gap) city = s;
  }
  const lines = msg.text.trim() ? wrap(msg.text, cw, c.lines, c.textSize, TEXT_WEIGHT, m) : [];
  let top = headerH(c);
  if (lines.length) top += c.textSize * 0.15 + lines.length * lineH(c);
  let photo: Card['photo'] = null;
  if (c.photo && msg.photo && c.photoH > 0) {
    const ph = c.photoH;
    const pw = Math.min(cw, (ph * 16) / 9);
    const py = top + pad * 0.6;
    photo = [pad + py, pw, ph];
    top = py + ph;
  }
  return { h: Math.round(pad * 2 + Math.max(top, a)), left, x0, name, nameW, city, lines, photo };
}

interface Placed {
  msg: Msg;
  card: Card;
  x: number;
  y: number;
  alpha: number;
}

function stack(c: Cfg, hs: number[], h: number): number[] {
  const total = hs.reduce((a, b) => a + b, 0) + c.gap * Math.max(0, hs.length - 1);
  let y = c.anchor === 'bottom' ? h - total : 0;
  return hs.map((ch) => {
    const top = y;
    y += ch + c.gap;
    return top;
  });
}

function fitting(c: Cfg, cards: [Msg, Card][], h: number): [Msg, Card][] {
  let used = 0;
  const out: [Msg, Card][] = [];
  for (let i = 0; i < cards.length; i++) {
    const next = used + cards[i][1].h + (i > 0 ? c.gap : 0);
    if (i > 0 && next > h + 0.5) break;
    used = next;
    out.push(cards[i]);
  }
  return out;
}

/** Progress of the last change at `now` (1 = done). */
export function progress(c: Cfg, st: State, now: number): number {
  const at = st.at ?? 0;
  if (at <= 0 || c.anim <= 0) return 1;
  return Math.min(1, Math.max(0, (now - at) / (c.anim * 1000)));
}
const easeOut = (k: number) => 1 - Math.pow(1 - k, 3);
/** Share of the change after which a new card comes in (the others have made room for it by then). */
const IN_FROM = 0.4;
const SLIDE = 40;

/** The cards of the wall at `now` (mirrors `layout`). */
export function layout(c: Cfg, st: State, data: Data, w: number, h: number, now: number, m: Measure): Placed[] {
  const cur = fitting(
    c,
    messages(c, data).map((msg) => [msg, card(c, msg, w, m)] as [Msg, Card]),
    h,
  );
  const ys = stack(
    c,
    cur.map((x) => x[1].h),
    h,
  );
  const t1 = progress(c, st, now);
  const k = easeOut(t1);
  const kin = easeOut(Math.min(1, Math.max(0, (t1 - IN_FROM) / (1 - IN_FROM))));
  if (t1 >= 1) return cur.map(([msg, cd], i) => ({ msg, card: cd, x: 0, y: ys[i], alpha: 1 }));
  const gone = (st.gone ?? []).map(fullMsg);
  const before: [Msg, Card][] = [];
  for (const id of st.prev ?? []) {
    const here = cur.find((x) => x[0].id === id);
    if (here) before.push(here);
    else {
      const g = gone.find((x) => x.id === id);
      if (g) before.push([g, card(c, g, w, m)]);
    }
  }
  const ys0 = stack(
    c,
    before.map((x) => x[1].h),
    h,
  );
  const out: Placed[] = [];
  before.forEach(([msg, cd], i) => {
    if (!cur.some((x) => x[0].id === msg.id)) out.push({ msg, card: cd, x: -SLIDE * k, y: ys0[i], alpha: 1 - k });
  });
  cur.forEach(([msg, cd], i) => {
    const was = before.findIndex((x) => x[0].id === msg.id);
    out.push(was >= 0 ? { msg, card: cd, x: 0, y: ys0[was] + (ys[i] - ys0[was]) * k, alpha: 1 } : { msg, card: cd, x: SLIDE * (1 - kin), y: ys[i], alpha: kin });
  });
  return out;
}

// --------------------------------------------------------------- painting --

let pics = 0;
/** A picture covering the box (cut to the clip in effect). */
function cover(p: Pt, src: string, x: number, y: number, w: number, h: number) {
  p.raw(<image key={`swp${(pics = (pics + 1) % 1e6)}`} href={src} x={x} y={y} width={w} height={h} opacity={p.alpha} preserveAspectRatio="xMidYMid slice" />);
}

/** An avatar in the circle (cx, cy, r): the initials on the name's colour, under the picture when there is one (mirrors `avatar`). */
export function avatar(p: Pt, src: string, name: string, cx: number, cy: number, r: number) {
  p.circle(cx, cy, r, avatarColor(name));
  p.text(initials(name), cx, cy, AVATAR_INK, { size: Math.round(r * 0.8), weight: 800, anchor: 0 });
  if (src.trim()) {
    p.clip(ellipse(cx, cy, r, r));
    cover(p, src.trim(), cx - r, cy - r, r * 2, r * 2);
    p.pop();
  }
}

function glyph(p: Pt, g: Glyph, letter: string, x: number, y: number, s: number, c: string, bg: string) {
  const u = (cmds: Cmd[]) => tf(cmds, x, y, s);
  switch (g) {
    case 'play':
      p.fill(
        u(
          poly(
            [
              [0.38, 0.29],
              [0.38, 0.71],
              [0.73, 0.5],
            ],
            true,
          ),
        ),
        c,
      );
      break;
    case 'bubble':
      p.fill(u(rrect(0.24, 0.22, 0.52, 0.42, 0.06)), c);
      p.fill(
        u(
          poly(
            [
              [0.34, 0.62],
              [0.34, 0.8],
              [0.5, 0.62],
            ],
            true,
          ),
        ),
        c,
      );
      p.fill(u(rrect(0.4, 0.32, 0.07, 0.17, 0)), bg);
      p.fill(u(rrect(0.55, 0.32, 0.07, 0.17, 0)), bg);
      break;
    case 'f':
      p.text('f', x + s * 0.54, y + s * 0.53, c, { size: s * 0.78, weight: 800, anchor: 0 });
      break;
    case 'x':
      p.text('X', x + s * 0.5, y + s * 0.5, c, { size: s * 0.6, weight: 700, anchor: 0 });
      break;
    case 'camera':
      p.stroke(u(rrect(0.24, 0.24, 0.52, 0.52, 0.16)), c, s * 0.08, true);
      p.stroke(u(ellipse(0.5, 0.5, 0.12, 0.12)), c, s * 0.08, true);
      p.fill(u(ellipse(0.65, 0.35, 0.04, 0.04)), c);
      break;
    case 'note':
      p.fill(u(ellipse(0.42, 0.66, 0.13, 0.13)), c);
      p.fill(u(rrect(0.49, 0.2, 0.08, 0.47, 0)), c);
      p.fill(
        u(
          poly(
            [
              [0.57, 0.2],
              [0.75, 0.32],
              [0.75, 0.42],
              [0.57, 0.31],
            ],
            true,
          ),
        ),
        c,
      );
      break;
    case 'phone':
      p.stroke(u(ellipse(0.52, 0.48, 0.23, 0.23)), c, s * 0.08, true);
      p.fill(
        u(
          poly(
            [
              [0.22, 0.8],
              [0.29, 0.6],
              [0.4, 0.72],
            ],
            true,
          ),
        ),
        c,
      );
      break;
    case 'dots':
      p.fill(u(rrect(0.2, 0.24, 0.6, 0.42, 0.1)), c);
      p.fill(
        u(
          poly(
            [
              [0.3, 0.64],
              [0.3, 0.8],
              [0.46, 0.64],
            ],
            true,
          ),
        ),
        c,
      );
      for (let k = 0; k < 3; k++) p.fill(u(ellipse(0.36 + k * 0.14, 0.45, 0.045, 0.045)), bg);
      break;
    default:
      p.text(letter, x + s * 0.5, y + s * 0.5, c, { size: s * 0.6, weight: 800, anchor: 0 });
  }
}

/** A platform's badge: a rounded square of side s at (x, y) (mirrors `badge`). */
export function badge(p: Pt, id: string, name: string, x: number, y: number, s: number) {
  const [bg, g] = platform(id);
  p.fill(rrect(x, y, s, s, s * 0.28), bg);
  const letter = ([...name, ...id].find((ch) => /[\p{L}\p{N}]/u.test(ch)) ?? '').toUpperCase();
  if (g === 'note') {
    // TikTok: the glyph doubled in cyan and pink at its edges.
    const o = s * 0.05;
    glyph(p, g, letter, x - o, y - o, s, TIKTOK_EDGES[0], bg);
    glyph(p, g, letter, x + o, y + o, s, TIKTOK_EDGES[1], bg);
  }
  glyph(p, g, letter, x, y, s, '#ffffff', bg);
}

function paintCard(p: Pt, c: Cfg, pl: Placed, w: number) {
  const k = pl.card;
  const m = pl.msg;
  const { x, y } = pl;
  p.alpha = Math.min(1, Math.max(0, pl.alpha));
  const shape = rrect(x, y, w, k.h, c.radius);
  p.fill(shape, c.bg);
  if (c.edge) {
    p.clip(shape);
    p.rect(x, y, c.edgeW, k.h, 0, platform(m.platform)[0]);
    p.pop();
  }
  const top = y + c.pad;
  if (c.avatar) {
    const r = c.avatarSize / 2;
    avatar(p, m.avatar, m.name, x + k.left + r, top + r, r);
  }
  const cy = top + headerH(c) / 2;
  const x0 = x + k.x0;
  p.text(k.name, x0, cy, c.nameColor, { size: c.nameSize, weight: NAME_WEIGHT });
  const gap = c.nameSize * 0.3;
  let after = x0 + k.nameW + gap;
  if (c.badge) {
    const bs = badgeSize(c);
    badge(p, m.platform, m.platformName, after, cy - bs / 2, bs);
    after += bs + gap;
  }
  if (k.city) p.text(k.city, after, cy, c.muted, { size: c.nameSize * 0.85, weight: CITY_WEIGHT });
  const lh = lineH(c);
  const t0 = top + headerH(c) + c.textSize * 0.15;
  k.lines.forEach((l, i) => p.text(l, x0, t0 + lh * (i + 0.5), c.textColor, { size: c.textSize, weight: TEXT_WEIGHT }));
  if (k.photo) {
    const [py, pw, ph] = k.photo;
    const r = c.radius * 0.6;
    p.rect(x0, y + py, pw, ph, r, '#00000014');
    p.clip(rrect(x0, y + py, pw, ph, r));
    cover(p, m.photo, x0, y + py, pw, ph);
    p.pop();
  }
  p.alpha = 1;
}

function WallView({ layer, w, h, data, state, nowMs }: { layer: WallLayer; w: number; h: number; data: Data; state: unknown; nowMs: number }) {
  const c = cfg(layer);
  const p = new Pt();
  p.font = c.font;
  const cards = layout(c, (state ?? {}) as State, data, w, h, nowMs, (s, size, weight) => p.tw(s, size, weight, 0));
  p.clip(rrect(0, 0, w, h, 0));
  cards.forEach((pl) => paintCard(p, c, pl, w));
  p.pop();
  return p.svg(w, h);
}

// ------------------------------------------------------------- inspector --

/** A font family (the weights are the widget's). */
export function FamilyPick({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const families = useFonts((s) => s.families);
  const has = families.some((f) => f.family.toLowerCase() === value.toLowerCase());
  return <Dropdown value={value} options={[...(has ? [] : [{ value, label: value }]), ...families.map((f) => ({ value: f.family, label: f.family, style: { fontFamily: `"${f.family}", Inter, sans-serif` } }))]} onChange={onChange} />;
}

const SAMPLE = JSON.stringify([
  { id: 's1', platform: 'youtube', platformName: 'YouTube', name: 'Sophie L.', city: 'Lyon', text: 'Pourquoi la réforme ne concerne pas les indépendants ?' },
  { id: 's2', platform: 'x', platformName: 'X', name: '@claire.v', text: 'Et les retraités agricoles ?' },
]);

export default defineWidget({
  id: 'socialwall',
  type: 'socialWall',
  label: 'Social wall',
  group: 'Data',
  icon: MessagesSquare,
  create: () => ({ items: SAMPLE, props: { width: 640, height: 520 } }),
  ownsLook: true,
  live: () => true,
  ageOnly: true,
  content: ['items'],
  Render: ({ layer, w, h, data, time, state }) => <WallView layer={layer as unknown as WallLayer} w={w} h={h} data={data} state={state} nowMs={time.nowMs} />,
  warnings: (l, { data }) => {
    const out: Warning[] = [];
    if (!messages(cfg(l as unknown as Config), data).length) out.push({ level: 'info', text: t('No message to show.'), op: true });
    return out;
  },
  Inspector: ({ layer, set }) => {
    const c = cfg(layer as unknown as Config);
    return (
      <>
        <Section id="sw-content" title={t('Content')}>
          <Content label={t('Messages')} path="items" value={c.items} multiline placeholder="{{messages}}" />
          <Num label={t('Cards shown')} path="max" value={c.max} min={1} max={12} />
          <Seg
            label={t('Stack')}
            path="anchor"
            value={c.anchor}
            options={[
              ['top', t('Top')],
              ['bottom', t('Bottom')],
            ]}
          />
          <Num label={t('Lines of text')} path="lines" value={c.lines} min={1} max={8} />
        </Section>
        <Section id="sw-card" title={t('Card')}>
          <Toggle label={t('Avatar')} path="avatar" value={c.avatar} />
          {c.avatar && <Num label={t('Size')} path="avatarSize" value={c.avatarSize} min={16} max={160} unit="px" />}
          <Toggle label={t('Platform badge')} path="badge" value={c.badge} />
          <Toggle label={t('City')} path="city" value={c.city} />
          <Toggle label={t('Photo')} path="photo" value={c.photo} />
          {c.photo && <Num label={t('Height')} path="photoH" value={c.photoH} min={40} max={600} unit="px" />}
          <Toggle label={t('Platform edge')} path="edge" value={c.edge} />
          {c.edge && <Num label={t('Width')} path="edgeW" value={c.edgeW} min={1} max={40} unit="px" />}
        </Section>
        <Section id="sw-look" title={t('Look')}>
          <Row label={t('Font')}>
            <FamilyPick value={c.font} onChange={(f) => set('font', f)} />
          </Row>
          <Num label={t('Name size')} path="nameSize" value={c.nameSize} min={8} max={120} unit="px" />
          <Num label={t('Text size')} path="textSize" value={c.textSize} min={8} max={120} unit="px" />
          <ColorPick label={t('Background')} path="bg" value={c.bg} />
          <ColorPick label={t('Name')} path="nameColor" value={c.nameColor} />
          <ColorPick label={t('Text')} path="textColor" value={c.textColor} />
          <ColorPick label={t('City')} path="muted" value={c.muted} />
          <Num label={t('Padding')} path="pad" value={c.pad} min={0} max={80} unit="px" />
          <Num label={t('Gap')} path="gap" value={c.gap} min={0} max={120} unit="px" />
          <Num label={t('Corner radius')} path="radius" value={c.radius} min={0} max={80} unit="px" />
        </Section>
        <Section id="sw-motion" title={t('Motion')}>
          <RangeNum label={t('New card')} path="anim" value={c.anim} min={0} max={2} step={0.05} precision={2} unit="s" />
        </Section>
      </>
    );
  },
});

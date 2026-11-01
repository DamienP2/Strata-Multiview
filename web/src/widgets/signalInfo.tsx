// The Signal widget: what an input's picture and sound are — codec, sampling, colour space,
// transfer, range, HDR (HDR10, HLG, Dolby Vision, HDR10+) and its metadata (ST 2086, MaxCLL /
// MaxFALL), its sound tracks (Dolby Digital, Digital Plus, Atmos…). Laid out as items, mirroring
// crates/strata-mv-core/src/wid/signal_info.rs (keep in sync); the Rust engine draws the same list.
import { AudioLines, FileVideo, Film, Palette, Sun } from 'lucide-react';
import { useInputs, useSignal, type VideoInfo } from '../engine/liveInputs';
import type { Panel } from '../engine/types';
import { t } from '../i18n';
import { Items, rect, text, type Item } from './broadcast';
import { defineWidget, Flags, Section, Select, withDefaults } from './kit';
import { formatName } from './source';

export interface SignalConfig {
  input?: string;
  video?: boolean;
  color?: boolean;
  hdr?: boolean;
  audio?: boolean;
  background?: string;
}
const D_SIGNAL = { input: '', video: true, color: true, hdr: true, audio: true, background: '#0b0d12d9' };

const DASH = '—';
const DIM = '#9aa3b2';
const WHITE = '#ffffff';
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
const padOf = (w: number, h: number) => Math.round(clamp(Math.min(w, h) * 0.04, 3, 14));

export const MATRIX: Record<string, string> = { bt709: 'BT.709', bt2020nc: 'BT.2020 NCL', bt2020c: 'BT.2020 CL', smpte170m: 'BT.601 (525)', bt470bg: 'BT.601 (625)', ictcp: 'ICtCp', rgb: 'RGB', gbr: 'RGB', '': DASH };
export const PRIMARIES: Record<string, string> = { bt709: 'BT.709', bt2020: 'BT.2020', smpte432: 'P3 D65', smpte431: 'DCI-P3', smpte170m: 'BT.601 (525)', bt470bg: 'BT.601 (625)', '': DASH };
const TRANSFER: Record<string, string> = {
  bt709: 'BT.709 (SDR)',
  bt1361e: 'BT.709 (SDR)',
  smpte2084: 'PQ (ST 2084)',
  'arib-std-b67': 'HLG (ARIB STD-B67)',
  'iec61966-2-1': 'sRGB',
  smpte170m: 'BT.601 (SDR)',
  'bt2020-10': 'BT.2020 (SDR)',
  'bt2020-12': 'BT.2020 (SDR)',
  linear: 'Linéaire',
  '': DASH,
};
export const named = (map: Record<string, string>, k: string) => map[k] ?? k;

/** SDR, HDR10, HLG… (mirrors VideoInfo::dynamic). */
export const dynamicOf = (v: Partial<VideoInfo>) =>
  v.dolbyVision ? 'Dolby Vision' : v.hdr10Plus ? 'HDR10+' : (v.transfer ?? '').includes('2084') ? 'HDR10' : (v.transfer ?? '').includes('b67') ? 'HLG' : v.hdr || 'SDR';

/** A dynamic's badge colours (mirrors dynamic_colors). */
export const dynamicColors = (d: string): [string, string] =>
  d === 'Dolby Vision' ? ['#8b5cf6', '#ffffff'] : d === 'HDR10+' ? ['#f97316', '#1c0d00'] : d === 'HDR10' ? ['#f59e0b', '#1c1300'] : d === 'HLG' ? ['#22c55e', '#04130a'] : ['#4b5563', '#ffffff'];

/** ST 2086 → "P3 · 0.0001–1000 nits" (mirrors mastering_text). */
export function masteringText(m: string): string {
  if (!m.trim()) return DASH;
  const num = (key: string) => {
    const r = m.split(key)[1];
    if (r === undefined) return undefined;
    const s = /^[0-9.]*/.exec(r)?.[0] ?? '';
    const n = parseFloat(s);
    return Number.isFinite(n) ? n : undefined;
  };
  const x = num('r(');
  const gamut = x === undefined ? '' : Math.abs(x - 0.708) < 0.01 ? 'BT.2020' : Math.abs(x - 0.68) < 0.01 ? 'P3' : Math.abs(x - 0.64) < 0.01 ? 'BT.709' : '';
  const [lo, hi] = [num('min_luminance='), num('max_luminance=')];
  const lum = lo !== undefined && hi !== undefined ? `${Math.round(lo * 10000) / 10000}–${Math.round(hi)} nits` : '';
  if (!gamut && !lum) return m.slice(0, 40);
  return gamut && lum ? `${gamut} · ${lum}` : gamut || lum;
}

/** The levels measured, amber out of EBU R103 (mirrors levels_text). */
export function levelsText(v: Partial<VideoInfo>, fr: boolean): [string, string] {
  const pct = (x: number) => {
    const s = x.toFixed(0);
    return s.startsWith('-') ? (s === '-0' ? '0 %' : `−${s.slice(1)} %`) : `${s} %`;
  };
  if (!v.luma) return [DASH, WHITE];
  const [lo, hi] = v.luma;
  let s = `${pct(lo)} – ${pct(hi)}`;
  const illegal = v.illegal ?? 0;
  if (illegal >= 0.05) s += ` · ${illegal.toFixed(1)} % ${fr ? 'hors normes' : 'out of range'}`;
  return [s, lo < -1 || hi > 103 || illegal > 1 ? '#fbbf24' : WHITE];
}

export const audioLine = (a: VideoInfo['audio'][number]) =>
  [a.codec, a.layout, a.rate > 0 ? `${a.rate / 1000} kHz` : '', a.language && a.language !== 'und' ? a.language : ''].filter(Boolean).join(' · ');

/** The lines shown (mirrors lines). */
export function lines(c: Required<SignalConfig>, v: Partial<VideoInfo>, fr: boolean): [string, string, string][] {
  const l = (f: string, e: string) => (fr ? f : e);
  const orDash = (s: string) => (s.trim() ? s : DASH);
  const out: [string, string, string][] = [];
  if (c.video) {
    const codec = !v.codec ? '' : v.profile ? `${v.codec} · ${v.profile}` : v.codec;
    out.push(['Codec', orDash(codec), WHITE]);
    const pic = [v.size ? v.size.replace(/x/g, '×') : '', v.bits ? `${v.bits} bits` : '', v.chroma ?? ''].filter(Boolean);
    out.push([l('Image', 'Picture'), orDash(pic.join(' · ')), WHITE]);
  }
  if (c.color) {
    const [m, p] = [v.matrix ?? '', v.primaries ?? ''];
    const space = !m && !p ? '' : !m ? named(PRIMARIES, p) : !p ? named(MATRIX, m) : `${named(MATRIX, m)} · ${named(PRIMARIES, p)}`;
    out.push([l('Colorimétrie', 'Colour space'), orDash(space), WHITE]);
    out.push([l('Transfert', 'Transfer'), named(TRANSFER, v.transfer ?? ''), WHITE]);
    out.push([l('Plage', 'Range'), v.range === 'tv' ? l('Limitée (TV)', 'Limited (TV)') : v.range === 'pc' ? l('Complète (PC)', 'Full (PC)') : DASH, WHITE]);
    const [levels, col] = levelsText(v, fr);
    out.push([l('Niveaux (R103)', 'Levels (R103)'), levels, col]);
  }
  if (c.hdr) {
    out.push(['Mastering (ST 2086)', masteringText(v.mastering ?? ''), WHITE]);
    out.push(['MaxCLL / MaxFALL', (v.maxCll ?? 0) > 0 || (v.maxFall ?? 0) > 0 ? `${v.maxCll ?? 0} / ${v.maxFall ?? 0} nits` : DASH, WHITE]);
  }
  if (c.audio) {
    const tracks = v.audio ?? [];
    if (!tracks.length) out.push([l('Son', 'Sound'), DASH, WHITE]);
    tracks.forEach((a, i) => out.push([tracks.length === 1 ? l('Son', 'Sound') : `${l('Son', 'Sound')} ${i + 1}`, audioLine(a), WHITE]));
  }
  return out;
}

/** The Signal widget of an input (mirrors signal_info). */
export function signalInfo(cfg: SignalConfig, w: number, h: number, view: { name: string; format: string; live: boolean; video: Partial<VideoInfo> } | null, lang: string): Item[] {
  const c = withDefaults(D_SIGNAL, cfg) as Required<SignalConfig>;
  const fr = lang.toLowerCase().startsWith('fr');
  const pad = padOf(w, h);
  const out: Item[] = [rect([0, 0, w, h], pad * 0.6, c.background)];
  const info = view?.video ?? {};
  const rows = lines(c, info, fr);
  const n = rows.length + 1.4;
  const rh = Math.max(1, (h - 2 * pad) / n);
  const fs = Math.round(clamp(Math.min(rh * 0.52, w * 0.035), 7, 26));
  const head = rh * 1.4;
  const live = !!view?.live;
  const name = view?.name ?? '';
  const dynamic = live ? dynamicOf(info) : '';
  const badgeW = dynamic ? Math.min(fs * 0.62 * [...dynamic].length + fs * 1.4, w * 0.45) : 0;
  const tw = w - 2 * pad - badgeW - fs * 0.5;
  out.push(text([pad, pad, tw * 0.62, head], name || DASH, fs * 1.15, 700, false, WHITE, 'left'));
  out.push(text([pad + tw * 0.62, pad, tw * 0.38, head], view?.format ?? '', fs, 600, true, DIM, 'left'));
  if (badgeW > 0) {
    const [bg, fg] = dynamicColors(dynamic);
    const bh = fs * 1.5;
    const bx: [number, number, number, number] = [w - pad - badgeW, pad + (head - bh) / 2, badgeW, bh];
    out.push(rect(bx, bh * 0.25, bg));
    out.push(text(bx, dynamic, fs * 0.9, 800, false, fg, 'center'));
  }
  out.push(rect([pad, pad + head - 1, w - 2 * pad, 1], 0, '#ffffff1f'));
  const tracks = (info.audio ?? []).map((a) => [a.dolby, a.atmos] as const);
  const firstTrack = rows.length - (c.audio ? Math.max(1, tracks.length) : 0);
  rows.forEach(([label, value, col], i) => {
    const y = pad + head + i * rh;
    const nw = (w - 2 * pad) * 0.36;
    out.push(text([pad, y, nw, rh], label, fs * 0.85, 600, false, DIM, 'left'));
    let vx = pad + nw;
    let vw = w - pad - vx;
    const tr = c.audio && i >= firstTrack ? tracks[i - firstTrack] : undefined;
    if (tr) {
      const [dolby, atmos] = tr;
      let x = w - pad;
      for (const [on, mark, bg] of [
        [atmos, 'ATMOS', '#0ea5e9'],
        [dolby, 'DOLBY', '#e5e7eb'],
      ] as const) {
        if (!on) continue;
        const bw = fs * 3.6;
        const bh = rh * 0.7;
        x -= bw;
        out.push(rect([x, y + (rh - bh) / 2, bw, bh], bh * 0.2, bg));
        out.push(text([x, y + (rh - bh) / 2, bw, bh], mark, fs * 0.7, 800, false, '#0b0d12', 'center'));
        x -= fs * 0.3;
      }
      vw = x - vx - fs * 0.3;
    }
    vx = Math.min(vx, w - pad);
    out.push(text([vx, y, Math.max(1, vw), rh], value, fs, 600, false, col, 'left'));
  });
  return out;
}

function SignalView({ cfg, w, h, lang }: { cfg: SignalConfig; w: number; h: number; lang: string }) {
  const id = (cfg.input ?? '').trim();
  const signal = useSignal(id);
  const inputs = useInputs();
  const info = inputs.find((i) => i.id === id);
  const view = id && info ? { name: info.name, format: info.live ? formatName(info) : '', live: info.live, video: signal.video ?? {} } : null;
  return <Items items={signalInfo(cfg, w, h, view, lang)} look={(cfg as { panel?: Panel }).panel} />;
}

export const signalInfoWidget = defineWidget({
  id: 'signalInfo',
  type: 'signalInfo',
  label: 'Signal',
  group: 'Multiview',
  icon: FileVideo,
  create: () => ({ props: { width: 560, height: 380 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, lang }) => <SignalView cfg={layer as SignalConfig} w={w} h={h} lang={lang ?? 'fr'} />,
  Inspector: ({ layer, set }) => {
    const c = withDefaults(D_SIGNAL, layer as SignalConfig);
    const inputs = useInputs();
    return (
      <Section id="signalInfo" title={t('Signal')}>
        <Select label={t('Input')} value={c.input} onChange={(v) => set('input', v)} options={[['', t('None')] as [string, string], ...inputs.map((i) => [i.id, i.name] as [string, string])]} />
        <Flags
          label={t('Show')}
          items={[
            { icon: <Film size={14} />, title: t('Video: codec, size, sampling'), value: c.video, onChange: (v) => set('video', v) },
            { icon: <Palette size={14} />, title: t('Colour: colour space, transfer, range, levels'), value: c.color, onChange: (v) => set('color', v) },
            { icon: <Sun size={14} />, title: t('HDR: mastering display, MaxCLL / MaxFALL'), value: c.hdr, onChange: (v) => set('hdr', v) },
            { icon: <AudioLines size={14} />, title: t('Sound: tracks, Dolby, Atmos'), value: c.audio, onChange: (v) => set('audio', v) },
          ]}
        />
      </Section>
    );
  },
});

export default signalInfoWidget;

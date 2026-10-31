// Ready-made multiviews of a playout control room, offered by New multiview: the final control of a
// channel, sixteen channels, main and backup, off-air returns, audio, IP and PTP, contributions,
// the night shift, branding and subtitles, UHD HDR. Drawn on 1920 × 1080 and scaled to the
// multiview's picture; their tiles take the inputs in order. Everything stays editable afterwards.
import type { InputInfo } from '../engine/liveInputs';
import type { Layer, LayerType } from '../engine/types';
import { t } from '../i18n';
import { makeLayer } from './designerStore';

type Box = [number, number, number, number];

export interface Model {
  id: string;
  name: string;
  hint: string;
  build: (b: Builder) => void;
}

/** What a model places, on 1920 × 1080. */
interface Builder {
  /** A Source tile of the n-th input. */
  tile: (n: number, box: Box, opts?: Record<string, unknown>) => void;
  /** A widget, on the `input`-th input when it shows one. */
  widget: (type: LayerType, box: Box, cfg?: Record<string, unknown>, input?: number) => void;
  label: (box: Box, text: string, size?: number) => void;
  band: (box: Box, color?: string) => void;
}

const LABELS_16 = 'L, R, C, LFE, Ls, Rs, L, R, AD, AD, VO, VO, 13, 14, 15, 16';

/** The bar at the top: its name, the station's timecode, the state of the plant. */
function header(b: Builder, title: string) {
  b.band([0, 0, 1920, 60]);
  b.label([20, 6, 1000, 48], title, 30);
  b.widget('status', [1060, 6, 420, 48], { ptp: false, ntp: false, alarms: true, background: '#00000000' });
  b.widget('timecode', [1496, 6, 408, 48], { label: '', background: '#00000000' });
}

/** A grid of tiles from input `first`, in `area`. */
function grid(b: Builder, area: Box, cols: number, rows: number, gap: number, first: number, opts?: Record<string, unknown>) {
  const [x0, y0, w, h] = area;
  const cw = (w - (cols - 1) * gap) / cols;
  const ch = (h - (rows - 1) * gap) / rows;
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) b.tile(first + r * cols + c, [Math.round(x0 + c * (cw + gap)), Math.round(y0 + r * (ch + gap)), Math.round(cw), Math.round(ch)], opts);
}

export const MODELS: Model[] = [
  {
    id: 'final',
    name: 'Final control of a channel',
    hint: 'PGM, PVW, playout A and B, four returns, 16 audio channels, loudness, alarms',
    build: (b) => {
      header(b, t('Final control · Channel 1'));
      b.tile(0, [16, 72, 960, 540], { timecode: 'station', loudness: true, alarms: true, umd: { text: 'PGM' } });
      b.tile(1, [988, 72, 448, 252], { umd: { text: 'PVW' } });
      b.tile(2, [988, 336, 448, 252], { umd: { text: 'PLAYOUT A' } });
      b.tile(3, [988, 600, 448, 252], { umd: { text: 'PLAYOUT B' } });
      for (let i = 0; i < 4; i++) b.tile(4 + i, [16 + i * 242, 624, 230, 130], { vu: { max: 2 } });
      b.widget('audioMeter', [1448, 72, 456, 700], { channels: 16, labels: LABELS_16 }, 0);
      b.widget('loudness', [1448, 784, 456, 288], {}, 0);
      b.widget('alarmLog', [16, 766, 960, 306], { rows: 6 });
      b.widget('status', [988, 864, 448, 208]);
    },
  },
  {
    id: 'sixteen',
    name: 'Sixteen channels',
    hint: '4 × 4 channels with their loudness and alarms, the state of the plant and the journal',
    build: (b) => {
      header(b, t('Playout · 16 channels'));
      grid(b, [16, 72, 1592, 1000], 4, 4, 8, 0, { loudness: true, alarms: true, vu: { max: 2 } });
      b.widget('status', [1620, 72, 284, 260]);
      b.widget('alarmLog', [1620, 344, 284, 728], { rows: 12 });
    },
  },
  {
    id: 'redundancy',
    name: 'Main and backup',
    hint: 'Four channels, their main and backup side by side, with their audio',
    build: (b) => {
      header(b, t('Redundancy · main / backup'));
      for (let i = 0; i < 4; i++) {
        const y = 72 + i * 252;
        b.band([16, y, 200, 242], '#10141b');
        b.label([28, y + 90, 180, 60], t('Channel {n}', { n: i + 1 }), 30);
        b.tile(2 * i, [226, y, 430, 242], { umd: { text: 'MAIN' }, alarms: true });
        b.tile(2 * i + 1, [666, y, 430, 242], { umd: { text: 'BACKUP' }, alarms: true });
        b.widget('audioMeter', [1106, y, 394, 242], { channels: 8, scale: false, loudness: true }, 2 * i);
        b.widget('audioMeter', [1510, y, 394, 242], { channels: 8, scale: false, loudness: true }, 2 * i + 1);
      }
    },
  },
  {
    id: 'returns',
    name: 'Off-air returns',
    hint: 'The programme against its returns (DTT, satellite, IPTV, OTT), loudness and journal',
    build: (b) => {
      header(b, t('Off-air control · what viewers see'));
      b.tile(0, [16, 72, 1120, 630], { timecode: 'station', loudness: true, alarms: true, umd: { text: 'PGM' } });
      const names = ['DTT', 'SATELLITE', 'IPTV', 'OTT'];
      names.forEach((n, i) => b.tile(1 + i, [1148 + (i % 2) * 382, 72 + Math.floor(i / 2) * 220, 374, 210], { umd: { text: n }, loudness: true, alarms: true }));
      b.widget('loudness', [1148, 512, 756, 190], { history: false }, 0);
      b.widget('alarmLog', [16, 714, 1120, 358], { rows: 7 });
      b.widget('status', [1148, 714, 756, 170]);
      b.widget('timecode', [1148, 896, 756, 176]);
    },
  },
  {
    id: 'audio',
    name: 'Audio, 16 channels',
    hint: 'An input’s 16 channels and loudness, four more inputs, with or without picture',
    build: (b) => {
      header(b, t('Audio control · 16 channels'));
      b.widget('audioMeter', [16, 72, 1200, 640], { channels: 16, labels: LABELS_16, values: true }, 0);
      b.widget('loudness', [1228, 72, 676, 640], {}, 0);
      for (let i = 0; i < 4; i++) b.widget('audioMeter', [16 + i * 474, 724, 462, 348], { values: true }, 1 + i);
    },
  },
  {
    id: 'ip',
    name: 'IP, PTP and sync',
    hint: 'Nine ST 2110 sources, PTP and NTP, the station timecode and the journal',
    build: (b) => {
      header(b, t('Infrastructure · ST 2110, PTP and sync'));
      grid(b, [16, 72, 1200, 680], 3, 3, 8, 0, { badge: true, alarms: true, vu: { max: 2 } });
      b.widget('status', [1228, 72, 676, 260]);
      b.widget('timecode', [1228, 344, 676, 140]);
      b.widget('alarmLog', [1228, 496, 676, 576], { rows: 9 });
      b.widget('loudness', [16, 764, 600, 308], { history: false }, 0);
      b.widget('audioMeter', [628, 764, 588, 308], {}, 0);
    },
  },
  {
    id: 'contributions',
    name: 'Contributions',
    hint: 'Six incoming feeds with their timecode and loudness, the journal',
    build: (b) => {
      header(b, t('Contributions and booked links'));
      for (let i = 0; i < 6; i++) {
        const [c, r] = [i % 3, Math.floor(i / 3)];
        const [x, y] = [16 + c * 459, 72 + r * 490];
        b.tile(i, [x, y, 450, 300], { timecode: 'input', alarms: true, badge: true });
        b.widget('loudness', [x, y + 308, 450, 170], {}, i);
      }
      b.widget('status', [1396, 72, 508, 200]);
      b.widget('timecode', [1396, 284, 508, 130]);
      b.widget('alarmLog', [1396, 426, 508, 646], { rows: 10 });
    },
  },
  {
    id: 'night',
    name: 'Night shift, 25 sources',
    hint: '5 × 5 sources where only the faults stand out, the journal beside them',
    build: (b) => {
      header(b, t('Night shift · 25 sources'));
      grid(b, [16, 72, 1440, 1000], 5, 5, 6, 0, { alarms: true, vu: { max: 2, width: 4 } });
      b.widget('timecode', [1468, 72, 436, 120]);
      b.widget('status', [1468, 204, 436, 200]);
      b.widget('alarmLog', [1468, 416, 436, 656], { rows: 12, ended: false });
    },
  },
  {
    id: 'branding',
    name: 'Branding and subtitles',
    hint: 'The programme before and after branding, its loudness, its audio and the journal',
    build: (b) => {
      header(b, t('Editorial control · branding, subtitles'));
      b.tile(0, [16, 72, 936, 527], { umd: { text: 'CLEAN' }, alarms: true });
      b.tile(1, [968, 72, 936, 527], { umd: { text: 'DIRTY' }, timecode: 'station', loudness: true, alarms: true });
      b.widget('loudness', [16, 611, 620, 461], {}, 1);
      b.widget('audioMeter', [648, 611, 400, 461], { labels: 'L, R, C, LFE, Ls, Rs, AD, AD' }, 1);
      b.widget('alarmLog', [1060, 611, 844, 330], { rows: 6 });
      b.widget('timecode', [1060, 953, 844, 119]);
    },
  },
  {
    id: 'uhd',
    name: 'UHD HDR and SDR',
    hint: 'The UHD programme and its SDR conversion, loudness, 16 channels, the state of the plant',
    build: (b) => {
      header(b, t('UHD channel · HDR and SDR'));
      b.tile(0, [16, 72, 1264, 711], { timecode: 'station', loudness: true, alarms: true, badge: true, umd: { text: 'PGM UHD' } });
      b.tile(1, [1292, 72, 612, 344], { umd: { text: 'SDR' }, badge: true, alarms: true });
      b.widget('status', [1292, 428, 612, 180]);
      b.widget('loudness', [1292, 620, 612, 452], {}, 0);
      b.widget('audioMeter', [16, 795, 1264, 277], { channels: 16, labels: LABELS_16, scale: false }, 0);
    },
  },
];

/** The rgba of a `#rrggbb` (the layers' fill). */
const rgba = (hex: string): [number, number, number, number] => {
  const n = parseInt(hex.slice(1, 7), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, 1];
};

/** The layers of a model in a `width` × `height` multiview, its tiles on the inputs in order. */
export function buildModel(m: Model, size: { width: number; height: number }, inputs: InputInfo[]): Layer[] {
  const [sx, sy] = [size.width / 1920, size.height / 1080];
  const out: Layer[] = [];
  const place = (l: Layer, [x, y, w, h]: Box) => {
    l.props = { ...l.props, x: Math.round(x * sx), y: Math.round(y * sy), width: Math.round(w * sx), height: Math.round(h * sy) };
    out.push(l);
  };
  const inputOf = (n: number) => inputs[n]?.id ?? '';
  const b: Builder = {
    tile: (n, box, opts) => {
      const l = makeLayer('source', size.width, size.height);
      l.name = inputs[n]?.name ?? t('Source {n}', { n: n + 1 });
      Object.assign(l, opts ?? {}, { input: inputOf(n) });
      place(l, box);
    },
    widget: (type, box, cfg, input) => {
      const l = makeLayer(type, size.width, size.height);
      Object.assign(l, cfg ?? {});
      if (input !== undefined) Object.assign(l, { input: inputOf(input) });
      place(l, box);
    },
    label: (box, text, fontSize = 28) => {
      const l = makeLayer('text', size.width, size.height);
      if (l.type === 'text') l.text = text;
      l.name = text;
      place(l, box);
      l.props = { ...l.props, fontSize: Math.round(fontSize * Math.min(sx, sy)), fill: rgba('#f5f7fa') };
    },
    band: (box, color = '#10141b') => {
      const l = makeLayer('rect', size.width, size.height);
      l.name = t('Band');
      place(l, box);
      l.props = { ...l.props, fill: rgba(color) };
    },
  };
  m.build(b);
  return out;
}

/** A small drawing of a model: tiles grey, widgets in colour. */
export function ModelIcon({ m, w = 96 }: { m: Model; w?: number }) {
  const h = Math.round((w * 9) / 16);
  const boxes: { box: Box; color: string }[] = [];
  const k = w / 1920;
  const add = (box: Box, color: string) => boxes.push({ box: [box[0] * k, box[1] * k, box[2] * k, box[3] * k], color });
  const colors: Partial<Record<LayerType, string>> = { audioMeter: '#2bd96b', loudness: '#93c5fd', timecode: '#fbbf24', status: '#c4b5fd', alarmLog: '#f87171' };
  m.build({
    tile: (_n, box) => add(box, 'currentColor'),
    widget: (type, box) => add(box, colors[type] ?? 'currentColor'),
    label: () => {},
    band: (box) => add(box, '#ffffff22'),
  });
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} aria-hidden>
      <rect width={w} height={h} fill="#00000040" rx={2} />
      {boxes.map((b, i) => (
        <rect key={i} x={b.box[0]} y={b.box[1]} width={Math.max(0.5, b.box[2] - 0.6)} height={Math.max(0.5, b.box[3] - 0.6)} rx={0.8} fill={b.color} opacity={b.color === 'currentColor' ? 0.5 : 0.85} />
      ))}
    </svg>
  );
}

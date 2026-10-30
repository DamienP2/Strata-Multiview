import { ArrowLeftRight, ArrowRightToLine, Check, Copy, Cpu, ExternalLink, Frame, ImageOff, MapPin, MoreHorizontal, Network, PanelRightClose, PanelRightOpen, Pencil, Play, Plus, Radio, RadioTower, RefreshCw, Save, Trash2, Tv, Globe } from 'lucide-react';
import { HexColorField } from '../ui/fields';
import { useEffect, useMemo, useRef, useState } from 'react';
import { api, can, useApp, useChanged } from '../store';
import { VIDEO_FORMATS, slateStyleOf, videoFormatIndex, type ChannelConfig, type InputConfig, type InputKind, type HardwareConfig, type Html5Output, type NdiOutput, type OutputState, type OutputsConfig, type SdiOutput, type ServerConfig, type SlateStyle, type St2110Output, type StreamOutput, type StreamProtocol, type SystemInfo } from './types';
import { CLOCKS, ENCODINGS, clockLabel, fillSdi, fillSt2110, hardwareOf, modeLabel, sdiOutputLabel, sdiPorts, usedHardware } from '../hardware/model';
import { AssetLibrary } from '../designer/AssetPicker';
import { navigate } from '../router';
import { ResizeCorner, useCanvasView } from '../ui/canvasView';
import { autoLayout, measureNodes } from '../ui/autoLayout';
import { confirmDialog, promptDialog } from '../ui/dialogs';
import { t, tc, tn } from '../i18n';
import { friendlyError } from '../ui/serverError';
import { InfoTip } from '../ui/InfoTip';
import { InputsMode } from './Inputs';
import { useInputs } from '../engine/liveInputs';
import { OnSwitch } from './OnSwitch';
import { useMetrics, type OutputMetrics } from '../engine/metrics';
import { corePct, fps as fpsText, fpsTone, num, outputTone, pct, pictureTone } from '../perf/format';
import './streams.css';
import '../perf/perf.css';

/*
 * Video outputs as a node canvas: each channel (an independent playout) feeds
 * its pictures (the main format and extra renditions such as 9:16), which
 * feed the destinations: web pages, SDI, ST 2110, NDI and streams (SRT, UDP,
 * RTMP, HLS). Channels are sorted by location (a studio, a truck, the web
 * desk…). The INPUTS mode of the page lists the live video inputs (Inputs.tsx).
 */

type Kind = 'program' | 'picture' | 'source' | 'html5' | 'sdi' | 'st2110' | 'ndi' | 'stream';
type DestKind = Exclude<Kind, 'program' | 'picture' | 'source'>;

/** Labels translated where shown. */
const KIND: Record<Kind, { label: string; color: string; icon: typeof Play }> = {
  program: { label: 'Program', color: '#bfcaff', icon: Play },
  picture: { label: 'Format', color: '#38bdf8', icon: Frame },
  source: { label: 'Source', color: '#94a3b8', icon: ArrowRightToLine },
  html5: { label: 'HTML5', color: '#22c55e', icon: Globe },
  sdi: { label: 'SDI', color: '#f5a524', icon: Tv },
  st2110: { label: 'ST 2110', color: '#e879f9', icon: Network },
  ndi: { label: 'NDI', color: '#14b8a6', icon: Radio },
  stream: { label: 'Stream', color: '#fb7185', icon: RadioTower },
};

const DEST_KINDS: DestKind[] = ['html5', 'ndi', 'stream', 'sdi', 'st2110'];

/** Live inputs placed as sources, by family (a stream is SRT, UDP, RTMP, HLS, RTSP or a URL). */
type SrcFamily = 'sdi' | 'ndi' | 'st2110' | 'stream';
const SRC_FAMILIES: SrcFamily[] = ['sdi', 'ndi', 'st2110', 'stream'];
const familyOf = (k: InputKind): SrcFamily => (k === 'sdi' || k === 'ndi' || k === 'st2110' ? k : 'stream');
/** Destinations that send to something Hardware declares. */
const needsHardware = (k: DestKind) => k === 'sdi' || k === 'st2110';

const PROTOCOLS: { value: StreamProtocol; label: string; placeholder: string }[] = [
  { value: 'srt', label: 'SRT', placeholder: 'srt://host:9000' },
  { value: 'udp', label: 'UDP', placeholder: 'udp://239.1.1.1:5000' },
  { value: 'rtmp', label: 'RTMP', placeholder: 'rtmp://a.rtmp.youtube.com/live2/<key>' },
  { value: 'hls', label: 'HLS', placeholder: '' },
];
const protocolLabel = (p: StreamProtocol) => PROTOCOLS.find((x) => x.value === p)?.label ?? p.toUpperCase();
/** "SRT · Listener". */
const streamBadge = (o: StreamOutput) => (o.protocol === 'srt' ? `SRT · ${tc('short', o.listen ? 'Listener' : 'Caller')}` : protocolLabel(o.protocol));
/** SRT outputs: Strata calls the receiver, or waits for the receivers. Labels translated where shown. */
const SRT_MODES = [
  { listen: false, label: 'Caller', hint: 'Strata connects to the receiver, which must be listening.' },
  { listen: true, label: 'Listener', hint: 'Strata waits for the receivers to connect.' },
];

/** Where Strata serves an HLS stream output. */
const hlsUrl = (channel: string, picture: string, output: string) => `${location.origin}/hls/${encodeURIComponent(channel)}/${encodeURIComponent(picture)}/${encodeURIComponent(output)}/index.m3u8`;

/** A `#rrggbb` for the colour picker. */
const hexColor = (c: string) => (/^#[0-9a-f]{6}$/i.test(c) ? c : /^#[0-9a-f]{3}$/i.test(c) ? `#${[...c.slice(1)].map((x) => x + x).join('')}` : c === 'white' ? '#ffffff' : '#000000');

const NODE_W = 232;
const COL = { program: 40, picture: 330, dest: 690 };
/** Format nodes are wider: their title carries the size and rate. */
const FORMAT_W = 290;

/** Extra formats (renditions); the first ones have their own button in the toolbar. */
const PICTURE_PRESETS = [
  { id: 'vertical', name: '9:16', width: 1080, height: 1920 },
  { id: 'square', name: '1:1', width: 1080, height: 1080 },
  { id: 'portrait', name: '4:5', width: 1080, height: 1350 },
  { id: 'hd', name: '720p', width: 1280, height: 720 },
  { id: 'uhd', name: 'UHD', width: 3840, height: 2160 },
];
const TOOLBAR_PRESETS = 3;

/** Frames rendered ahead (Program inspector). */
const RENDER_AHEAD = [1, 2, 3, 4];

const BACKGROUNDS = [
  { value: '', label: 'Transparent' },
  { value: 'black', label: 'Black' },
  { value: '#00b140', label: 'Green (chroma key)' },
];

/** A format of the channel (the main one, or a rendition): internally a "picture". */
interface Picture {
  id: string;
  name: string;
  width: number;
  height: number;
  outputs: OutputsConfig;
  slate?: string;
  slateStyle?: SlateStyle;
}

const pictures = (ch: ChannelConfig): Picture[] => [{ id: 'main', name: t('Main format'), width: ch.width, height: ch.height, outputs: ch.outputs, slate: ch.slate, slateStyle: ch.slateStyle }, ...(ch.renditions ?? [])];

/** What a format can show without project: the server's designs, then its own picture. */
const SLATES: { id: SlateStyle; label: () => string }[] = [
  { id: 'aurora', label: () => tc('slate', 'Aurora') },
  { id: 'bento', label: () => tc('slate', 'Bento') },
  { id: 'gradient', label: () => tc('slate', 'Vivid gradient') },
  { id: 'frame', label: () => tc('slate', 'Frame') },
  { id: 'lines', label: () => tc('slate', 'Lines') },
  { id: 'split', label: () => tc('slate', 'Split') },
  { id: 'transparent', label: () => tc('slate', 'Transparent') },
  { id: 'custom', label: () => t('Custom picture') },
];

/** The slate's name ("" = nothing shown). */
const slateName = (p: Picture) => {
  const s = slateStyleOf(p);
  return s === 'custom' ? (p.slate?.split('/').pop() ?? '') : SLATES.find((x) => x.id === s)!.label();
};

/** Size of a slate thumbnail (CSS pixels). */
const thumbSize = (p: Picture) => {
  const w = Math.min(84, Math.round((64 * p.width) / p.height));
  return [w, Math.round((w * p.height) / p.width)] as const;
};

/** A slate drawn by the server for the settings being edited (not applied yet), `w` CSS pixels wide. */
const slateThumb = (style: SlateStyle, ch: ChannelConfig, p: Picture, w: number) =>
  `/api/slate.png?${new URLSearchParams({ style, channel: ch.name, width: `${p.width}`, height: `${p.height}`, num: `${ch.frameRate.num}`, den: `${ch.frameRate.den}`, w: `${w * 2}`, ...(style === 'custom' ? { image: p.slate ?? '' } : {}) })}`;

/** `picture` is "" for a destination wired to no format. */
type Dest = { kind: DestKind; picture: string; value: St2110Output | SdiOutput | NdiOutput | Html5Output | StreamOutput };

/** The destinations whose wire was removed (kept, not started). */
const UNWIRED: Picture = { id: '', name: '', width: 0, height: 0, outputs: { st2110: [], sdi: [] } };
const unwiredOf = (ch: ChannelConfig): Picture => ({ ...UNWIRED, outputs: ch.unwired ?? UNWIRED.outputs });

/** The destination list of a format, or of the unwired destinations (""). */
const outputsOf = (c: ChannelConfig, picture: string): OutputsConfig => {
  if (!picture) return (c.unwired ??= { st2110: [], sdi: [], ndi: [], html5: [], streams: [] });
  return picture === 'main' ? c.outputs : (c.renditions ?? []).find((r) => r.id === picture)!.outputs;
};

/** "50p", "59.94p". */
const rateLabel = (ch: ChannelConfig) => `${+(ch.frameRate.num / ch.frameRate.den).toFixed(2)}${ch.interlaced ? 'i' : 'p'}`;
/** Node title of a format: "Main format · 1920×1080 50p", "9:16 · 1080×1920". */
const formatTitle = (p: Picture, ch: ChannelConfig) => `${p.name} · ${p.width}×${p.height}${p.id === 'main' ? ` ${rateLabel(ch)}` : ''}`;
const isOn = (d: Dest) => (d.value as { enabled?: boolean }).enabled !== false;

function dests(p: Picture): Dest[] {
  const o = p.outputs;
  return [
    ...(o.html5 ?? []).map((value) => ({ kind: 'html5' as const, picture: p.id, value })),
    ...o.sdi.map((value) => ({ kind: 'sdi' as const, picture: p.id, value })),
    ...o.st2110.map((value) => ({ kind: 'st2110' as const, picture: p.id, value })),
    ...(o.ndi ?? []).map((value) => ({ kind: 'ndi' as const, picture: p.id, value })),
    ...(o.streams ?? []).map((value) => ({ kind: 'stream' as const, picture: p.id, value })),
  ];
}

const listOf = (o: OutputsConfig, kind: DestKind) => {
  if (kind === 'ndi') return (o.ndi ??= []);
  if (kind === 'html5') return (o.html5 ??= []);
  if (kind === 'stream') return (o.streams ??= []);
  return (o[kind] ??= []) as (St2110Output | SdiOutput)[];
};

const formatLabel = (ch: ChannelConfig) => {
  const f = VIDEO_FORMATS[videoFormatIndex(ch)];
  return f?.label ?? `${ch.width}×${ch.height} ${+(ch.frameRate.num / ch.frameRate.den).toFixed(2)}${ch.interlaced ? 'i' : 'p'}`;
};

/** "1080i 50", "720p 59.94": a source's format, named like the video formats. */
const sourceFormatName = (f: { h: number; fps: number; interlaced: boolean }) => `${f.h}${f.interlaced ? 'i' : 'p'} ${+f.fps.toFixed(2)}`;

const outputUrl = (channel: string, picture: string, bg: string) => `${location.origin}/output/${encodeURIComponent(channel)}${picture === 'main' ? '' : `/${encodeURIComponent(picture)}`}${bg ? `?bg=${encodeURIComponent(bg)}` : ''}`;

const slug = (s: string) =>
  s
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '') || 'item';

const unique = (base: string, taken: Set<string>) => {
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base}-${n}`)) n++;
  return `${base}-${n}`;
};

/** Node positions, per browser (moving a node does not restart the outputs). */
const LAYOUT_KEY = 'pulse.outputsLayout';
type Layout = Record<string, Record<string, { x: number; y: number }>>;
export type Mode = 'outputs' | 'inputs';

/** The config without its inputs (what restarts the outputs). */
const withoutInputs = (c: ServerConfig) => JSON.stringify({ ...c, inputs: undefined });

const readLayout = (): Layout => {
  try {
    return JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? '{}');
  } catch {
    return {};
  }
};

export function Outputs({ mode }: { mode: Mode }) {
  const { toast, projects, channels: live, live: liveState, loadOnChannel, auth } = useApp();
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [cfg, setCfg] = useState<ServerConfig | null>(null);
  const [status, setStatus] = useState<Record<string, Record<string, OutputState>>>({});
  const [current, setCurrent] = useState<string | null>(null);
  const [sel, setSel] = useState<string>('program');
  // Narrow screens: the inspector is a drawer over the graph.
  const [panel, setPanel] = useState(false);
  const [layout, setLayout] = useState<Layout>(readLayout);
  const [saving, setSaving] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const [wire, setWire] = useState<{ from: string; moving?: string; x: number; y: number } | null>(null);
  const [addMenu, setAddMenu] = useState<{ picture: string; x: number; y: number } | null>(null);
  const [moreFormats, setMoreFormats] = useState(false);
  const [slatePick, setSlatePick] = useState<string | null>(null);
  const metrics = useMetrics();
  const inputLive = useInputs();
  const planeRef = useRef<HTMLDivElement>(null);
  // The menu of other formats closes on a click elsewhere (or Esc).
  useEffect(() => {
    if (!moreFormats) return;
    const close = () => setMoreFormats(false);
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.preventDefault(), close());
    window.addEventListener('pointerdown', close);
    window.addEventListener('keydown', esc);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('keydown', esc);
    };
  }, [moreFormats]);
  // Esc: the add menu of the canvas, then the drawer of the inspector.
  useEffect(() => {
    const esc = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented || document.querySelector('.modal-back')) return;
      if (addMenu) (e.preventDefault(), setAddMenu(null));
      else if (panel) (e.preventDefault(), setPanel(false));
    };
    window.addEventListener('keydown', esc);
    return () => window.removeEventListener('keydown', esc);
  });
  // Arrange lays the nodes out from left to right (the graph is only known further down).
  const arrangeRef = useRef<() => void>(() => {});
  const cv = useCanvasView('outputs', '.oc-node', { onArrange: () => arrangeRef.current() });
  // Node widths, per browser.
  const [widths, setWidths] = useState<Record<string, number>>(() => {
    try {
      return JSON.parse(localStorage.getItem('pulse.outputs.widths') ?? '{}');
    } catch {
      return {};
    }
  });
  const setWidth = (id: string, w: number) =>
    setWidths((cur) => {
      const next = { ...cur, [id]: w };
      try {
        localStorage.setItem('pulse.outputs.widths', JSON.stringify(next));
      } catch {
        /* private mode */
      }
      return next;
    });
  const widthOf = (id: string) => widths[id] ?? (id.startsWith('pic:') ? FORMAT_W : NODE_W);

  const load = async () => {
    const i = await api<SystemInfo>('GET', '/api/system');
    setInfo(i);
    setCfg(structuredClone(i.config));
  };
  useEffect(() => {
    load().catch((e) => toast(String(e), 'error'));
    const poll = () =>
      api<Record<string, Record<string, OutputState>>>('GET', '/api/outputs/status')
        .then(setStatus)
        .catch(() => {});
    poll();
    const h = setInterval(() => document.hidden || void poll(), 3000);
    return () => clearInterval(h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    try {
      localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
    } catch {
      /* private mode */
    }
  }, [layout]);

  // Another admin applied changes: follow them, unless we are editing.
  const editing = useRef(false);
  const appliedAt = useRef(0);
  useChanged('config', () => {
    if (Date.now() - appliedAt.current < 5000) return; // our own Apply
    if (editing.current) toast(t('Another user changed the outputs. Discard your changes to see theirs.'), 'info');
    else load().catch(() => {});
  });
  const dirty = useMemo(() => !!info && !!cfg && JSON.stringify(info.config) !== JSON.stringify(cfg), [info, cfg]);
  const outputsDirty = useMemo(() => !!info && !!cfg && withoutInputs(info.config) !== withoutInputs(cfg), [info, cfg]);
  editing.current = dirty;
  if (!can.admin(auth?.user)) return <div className="loading">{t('Only administrators set up the outputs.')}</div>;
  if (!info || !cfg) return <div className="loading">{t('Loading outputs…')}</div>;

  const locations = cfg.locations?.length ? cfg.locations : [{ id: 'main', name: t('Main') }];
  const chIndex = Math.max(0, cfg.channels.findIndex((c) => c.id === current));
  const ch = cfg.channels[chIndex];
  const saved = info.config.channels.find((c) => c.id === ch?.id);

  const edit = (fn: (c: ServerConfig) => void) => {
    const next = structuredClone(cfg);
    fn(next);
    setCfg(next);
  };
  const editCh = (fn: (c: ChannelConfig, all: ServerConfig) => void) => edit((c) => fn(c.channels[chIndex], c));

  // ---- graph ----
  const pics = ch ? pictures(ch) : [];
  const allDests = [...pics.flatMap(dests), ...(ch ? dests(unwiredOf(ch)) : [])];
  const hw = hardwareOf(cfg);
  const usedHw = usedHardware(cfg);
  // Live inputs placed as sources (a column between the program and the formats).
  const inputs = cfg.inputs ?? [];
  const srcIds = ch ? (ch.sources ?? []).filter((id) => inputs.some((i) => i.id === id)) : [];
  const inputOf = (id: string) => inputs.find((i) => i.id === id);
  const shift = srcIds.length ? NODE_W + 80 : 0;
  const graphicsOnly = (pic: string) => (ch?.graphicsOnly ?? []).includes(pic);
  const pos: Record<string, { x: number; y: number }> = {};
  allDests.forEach((d, i) => (pos[d.value.id] = { x: COL.dest + shift, y: 50 + i * 128 }));
  pics.forEach((p, i) => {
    const ys = allDests.filter((d) => d.picture === p.id).map((d) => pos[d.value.id].y);
    pos[`pic:${p.id}`] = { x: COL.picture + shift, y: ys.length ? (Math.min(...ys) + Math.max(...ys)) / 2 : 50 + i * 170 };
  });
  const picYs = pics.map((p) => pos[`pic:${p.id}`].y);
  pos.program = { x: COL.program, y: picYs.length ? (Math.min(...picYs) + Math.max(...picYs)) / 2 : 50 };
  srcIds.forEach((id, i) => (pos[`src:${id}`] = { x: COL.program + shift, y: 50 + i * 128 }));
  const own = (ch && layout[ch.id]) ?? {};
  // Places saved when the sources stood before the program are not kept.
  const stale = srcIds.some((id) => own[`src:${id}`] && own.program && own[`src:${id}`].x < own.program.x);
  for (const k of Object.keys(pos)) if (own[k] && !(stale && (k === 'program' || k.startsWith('src:')))) pos[k] = own[k];

  const savedDest = (id: string) => !!saved && pictures(saved).some((p) => dests(p).some((d) => d.value.id === id));
  const destStatus = (d: Dest): { tone: 'ok' | 'warn' | 'idle'; text: string; raw?: string } => {
    if (!d.picture) return { tone: 'idle', text: t('Not connected: drag a wire from a format to it') };
    if (d.kind === 'html5') return { tone: 'ok', text: t('Web link') };
    if (!isOn(d)) return { tone: 'idle', text: tc('output', 'Off') };
    if (!savedDest(d.value.id)) return { tone: 'idle', text: t('Not applied yet') };
    const s = status[ch.id]?.[d.value.id];
    if (!s) return { tone: 'idle', text: t('Starting…') };
    if (s.message === 'Off') return { tone: 'idle', text: tc('output', 'Off') };
    return s.ok ? { tone: 'ok', text: !s.message || s.message === 'Running' ? t('Running') : friendlyError(s.message), raw: s.message } : { tone: 'warn', text: friendlyError(s.message), raw: s.message };
  };
  const projectName = (id?: string | null) => projects.find((p) => p.id === id)?.name;
  const liveCh = live.find((c) => c.id === ch?.id);
  // Live rates of this channel (what the server measures).
  const chMetrics = metrics?.channels.find((c) => c.id === ch?.id);
  const outMetrics = (id: string): OutputMetrics | undefined => chMetrics?.pictures.flatMap((p) => p.outputs).find((o) => o.id === id);
  const rateChip = (id: string): GNode['chip'] => {
    if (!chMetrics) return undefined;
    if (id.startsWith('pic:')) {
      const p = chMetrics.pictures.find((x) => x.id === id.slice(4));
      return p && { text: `${fpsText(p.fps)} fps · ${pct(p.load)}`, tone: pictureTone(p, chMetrics.rate), title: t('Frames rendered per second · render time, % of the frame time') };
    }
    const o = outMetrics(id);
    return o && { text: `${fpsText(o.fps)} fps`, tone: outputTone(o, chMetrics.rate), title: o.dropped ? t('{n} frames dropped in the last 10 s', { n: o.dropped }) : t('Frames sent per second') };
  };

  type GNode = { id: string; kind: Kind; title: string; sub: string; tone: 'ok' | 'warn' | 'idle'; status: string; raw?: string; badge?: string; chip?: { text: string; tone: string; title: string }; on?: boolean; loose?: boolean; label?: string; color?: string; icon?: typeof Play; extra?: React.ReactNode };
  const famLook = (f: SrcFamily) => (f === 'stream' ? KIND.stream : KIND[f]);
  const bgInput = ch?.background && srcIds.includes(ch.background) ? inputOf(ch.background) : undefined;
  /** What a source sends and how it sits with the program's format. */
  const srcCheck = (id: string) => {
    const i = inputLive.find((x) => x.id === id);
    const f = i && i.live && i.height && i.fps ? { w: i.width, h: i.height, fps: i.fps, interlaced: !!i.interlaced } : null;
    if (!ch || !f) return { state: 'none' as const, f: null, name: '' };
    const name = sourceFormatName(f);
    const chFps = ch.frameRate.num / Math.max(1, ch.frameRate.den);
    const same = f.w === ch.width && f.h === ch.height && Math.abs(f.fps - chFps) < 0.05 && f.interlaced === !!ch.interlaced;
    return { state: same ? ('same' as const) : ch.sourceConvert === name ? ('convert' as const) : ('differs' as const), f, name };
  };
  /** The video format matching a source's (to switch the program to it), if any. */
  const formatOfSource = (f: { w: number; h: number; fps: number; interlaced: boolean }) => VIDEO_FORMATS.find((v) => v.width === f.w && v.height === f.h && !!v.interlaced === f.interlaced && Math.abs(v.rate.num / v.rate.den - f.fps) < 0.05);
  const convertSource = (name: string) => editCh((c) => void (c.sourceConvert = name));
  const programToSource = (f: { w: number; h: number; fps: number; interlaced: boolean }) => {
    const v = formatOfSource(f);
    if (!v) return;
    editCh((c) => {
      Object.assign(c, { width: v.width, height: v.height, frameRate: { ...v.rate } });
      if (v.interlaced) c.interlaced = true;
      else delete c.interlaced;
      delete c.sourceConvert;
    });
  };
  /** The three ways out of a source whose format differs from the program's. */
  const sourceChoices = (id: string, c: ReturnType<typeof srcCheck>, big = false) => {
    if (c.state !== 'differs' || !c.f) return null;
    const f = c.f;
    const v = formatOfSource(f);
    return (
      <div className={`oc-src-choices ${big ? 'big' : ''}`} onPointerDown={(e) => e.stopPropagation()}>
        <button type="button" className="on" onClick={() => convertSource(c.name)}>
          <RefreshCw size={13} /> {t('Convert live')}
        </button>
        <button type="button" disabled={!v} title={v ? undefined : t('No channel format matches this source')} onClick={() => programToSource(f)}>
          <Play size={13} /> {t('Program in {format}', { format: v?.label ?? c.name })}
        </button>
        <button
          type="button"
          onClick={() => {
            setSel(`src:${id}`);
            setPanel(true);
          }}
        >
          <ArrowLeftRight size={13} /> {t('Change the source')}
        </button>
      </div>
    );
  };
  const nodes: GNode[] = ch
    ? [
        {
          id: 'program',
          kind: 'program',
          title: ch.name,
          sub: `${formatLabel(ch)}\n${projectName(liveCh?.projectId) ? t('plays “{name}”', { name: projectName(liveCh?.projectId)! }) : t('no project loaded')}${bgInput ? `\n${t('over {name}', { name: bgInput.name })}` : ''}`,
          tone: liveCh ? 'ok' : 'idle',
          status: liveCh ? t('Running') : t('Not applied yet'),
        },
        ...srcIds.map((id): GNode => {
          const i = inputOf(id)!;
          const f = famLook(familyOf(i.kind));
          const wired = ch.background === id;
          const c = srcCheck(id);
          const st: [GNode['tone'], string] = !wired
            ? ['idle', t('Not connected: drag a wire from the program')]
            : c.state === 'none'
              ? ['idle', t('No signal')]
              : c.state === 'same'
                ? ['ok', t('Under the graphics')]
                : c.state === 'convert'
                  ? ['ok', t('Live conversion')]
                  : ['warn', t('Different format')];
          return {
            id: `src:${id}`,
            kind: 'source',
            label: `${t('Source')} · ${t(f.label)}`,
            color: f.color,
            icon: f.icon,
            title: i.name,
            sub: i.enabled === false ? tc('output', 'Off') : c.f ? (c.state === 'differs' ? `${c.name} ≠ ${formatLabel(ch)}` : c.name) : '',
            tone: st[0],
            status: st[1],
            loose: !wired,
            extra: wired ? sourceChoices(id, c) : null,
          };
        }),
        ...pics.map((p) => ({ id: `pic:${p.id}`, kind: 'picture' as const, title: formatTitle(p, ch), sub: `${ratioOf(p.width, p.height)}${slateName(p) ? `\n${t('Without project: {name}', { name: slateName(p) })}` : ''}`, tone: 'ok' as const, status: tn(dests(p).length, '{n} destination', '{n} destinations'), chip: rateChip(`pic:${p.id}`) })),
        ...allDests.map((d) => {
          const st = destStatus(d);
          return { id: d.value.id, kind: d.kind, title: destTitle(d), sub: destSub(d, ch, hw), tone: st.tone, status: st.text, raw: st.raw, badge: d.kind === 'stream' ? streamBadge(d.value as StreamOutput) : undefined, chip: d.kind === 'html5' || !d.picture ? undefined : rateChip(d.value.id), on: d.kind === 'html5' ? undefined : isOn(d), loose: !d.picture };
        }),
      ]
    : [];
  // Format → destination wires can be removed (the destination stays, unwired).
  const edges: { from: string; to: string; color: string; dest?: string; src?: boolean; via?: string }[] = ch
    ? [
        ...(bgInput ? [{ from: 'program', to: `src:${bgInput.id}`, color: famLook(familyOf(bgInput.kind)).color, src: true }] : []),
        ...pics.map((p) => (bgInput && !graphicsOnly(p.id) ? { from: `src:${bgInput.id}`, to: `pic:${p.id}`, color: famLook(familyOf(bgInput.kind)).color, via: p.id } : { from: 'program', to: `pic:${p.id}`, color: KIND.picture.color })),
        ...allDests.filter((d) => d.picture).map((d) => ({ from: `pic:${d.picture}`, to: d.value.id, color: KIND[d.kind].color, dest: d.value.id })),
      ]
    : [];
  // Program first, the destinations last (wired or not); saved like a drag of the nodes.
  arrangeRef.current = () => {
    if (!ch) return;
    const size = measureNodes(cv.ref.current, 'data-node');
    const at = autoLayout(
      nodes.map((n) => ({ id: n.id, ...(size[n.id] ?? { w: widthOf(n.id), h: 110 }), side: n.kind === 'program' ? ('start' as const) : n.kind === 'picture' || n.kind === 'source' ? undefined : ('end' as const) })),
      edges,
      { origin: { x: COL.program, y: 40 }, anchorY: 27 },
    );
    setLayout((l) => ({ ...l, [ch.id]: { ...(l[ch.id] ?? {}), ...at } }));
  };
  /** Column titles follow the nodes of the column. */
  const colX = (k: 'source' | 'program' | 'picture' | 'dest') => {
    const xs = nodes.filter((n) => (k === 'dest' ? n.kind !== 'program' && n.kind !== 'picture' && n.kind !== 'source' : n.kind === k)).map((n) => pos[n.id].x);
    return xs.length ? Math.min(...xs) : k === 'source' ? COL.program : COL[k] + shift;
  };
  const width = Math.max(980, ...Object.values(pos).map((p) => p.x + NODE_W + 60));
  const height = Math.max(560, ...Object.values(pos).map((p) => p.y + 160));

  // ---- actions ----
  const selDest = allDests.find((d) => d.value.id === sel);
  const selPic = sel.startsWith('pic:') ? pics.find((p) => `pic:${p.id}` === sel) : undefined;
  const takenIds = () => new Set(allDests.map((d) => d.value.id));
  // SDI and ST 2110 destinations send to what Hardware declares.
  const freeSdi = hw.sdi.outputs.filter((o) => !usedHw.has(o.id));
  const outFlows = hw.st2110.flows.filter((f) => f.direction === 'output');
  const freeFlows = outFlows.filter((f) => !usedHw.has(f.id));
  const hwMissing = (k: DestKind) => (k === 'sdi' ? !hw.sdi.outputs.length : k === 'st2110' ? !outFlows.length : false);
  const hwHint = (k: DestKind) => (k === 'sdi' ? t('Define SDI outputs in Hardware') : t('Define ST 2110 output flows in Hardware'));

  const addDest = (kind: DestKind, picture: string, at?: { x: number; y: number }) => {
    if (needsHardware(kind)) {
      if (hwMissing(kind)) return toast(hwHint(kind), 'info');
      if (!(kind === 'sdi' ? freeSdi : freeFlows).length) return toast(kind === 'sdi' ? t('Every SDI output of Hardware is already used') : t('Every ST 2110 output flow of Hardware is already used'), 'info');
    }
    const taken = takenIds();
    let k = 1;
    while (taken.has(`${kind}-${k}`)) k++;
    const id = `${kind}-${k}`;
    const n = allDests.filter((d) => d.kind === kind).length + 1;
    editCh((c, all) => {
      const p = pictures(c).find((x) => x.id === picture) ?? pictures(c)[0];
      // "": not wired to any format (the user draws the wire).
      const picOutputs = outputsOf(c, picture ? p.id : '');
      if (kind === 'st2110') {
        const f = freeFlows[0];
        const o: St2110Output = { id, name: f.name, hardware: f.id, encoding: f.encoding, fill: f.fill, key: null, interface: null, ttl: 32, payloadType: 96 };
        fillSt2110(o, hw);
        (picOutputs.st2110 ??= []).push(o);
      } else if (kind === 'sdi') {
        const d = freeSdi[0];
        const o: SdiOutput = { id, name: d.name, hardware: d.id, device: 0, externalKey: true, fillOnly: false, genlock: false };
        fillSdi(o, hw);
        (picOutputs.sdi ??= []).push(o);
      } else if (kind === 'ndi') {
        const names = new Set(all.channels.flatMap((x) => pictures(x).flatMap((pp) => (pp.outputs.ndi ?? []).map((o) => o.name.toLowerCase()))));
        let name = `${c.name}${p.id === 'main' ? '' : ` ${p.name}`}`;
        for (let k = 2; names.has(name.toLowerCase()); k++) name = `${c.name}${p.id === 'main' ? '' : ` ${p.name}`} ${k}`;
        (picOutputs.ndi ??= []).push({ id, name, groups: '', alpha: true });
      } else if (kind === 'stream') {
        // An SRT listener on a port no other stream (output or input) uses.
        const urls = new Set([...all.channels.flatMap((x) => pictures(x).flatMap((pp) => (pp.outputs.streams ?? []).map((o) => o.url))), ...(all.inputs ?? []).map((i) => i.url)]);
        let port = 9000;
        while ([...urls].some((u) => u.endsWith(`:${port}`) || u.includes(`:${port}/`) || u.includes(`:${port}?`))) port++;
        (picOutputs.streams ??= []).push({ id, name: t('Stream {n}', { n }), protocol: 'srt', url: `srt://0.0.0.0:${port}`, listen: true, bitrate: 8000, codec: 'h264', background: '#000000', audio: true });
      } else {
        (picOutputs.html5 ??= []).push({ id, name: t('Web page'), background: '' });
      }
    });
    if (at) setLayout((l) => ({ ...l, [ch.id]: { ...(l[ch.id] ?? {}), [id]: at } }));
    setSel(id);
  };

  const addPicture = (preset: (typeof PICTURE_PRESETS)[number]) => {
    const taken = new Set(pics.map((p) => p.id));
    const id = unique(preset.id, taken);
    editCh((c) => void (c.renditions ??= []).push({ id, name: preset.name, width: preset.width, height: preset.height, outputs: { st2110: [], sdi: [], ndi: [], html5: [], streams: [] } }));
    setSel(`pic:${id}`);
    setMoreFormats(false);
  };

  /** Places a live input as a source (the first of that family not placed yet; a new NDI or stream input when there is none). */
  const addSource = (fam: SrcFamily) => {
    const placed = new Set(ch.sources ?? []);
    const existing = inputs.find((i) => familyOf(i.kind) === fam && !placed.has(i.id));
    if (existing) {
      editCh((c) => void (c.sources = [...(c.sources ?? []), existing.id]));
      setSel(`src:${existing.id}`);
      return;
    }
    if (fam === 'sdi' || fam === 'st2110') {
      if (inputs.some((i) => familyOf(i.kind) === fam)) return toast(t('Every input of this kind is already a source of this channel'), 'info');
      return toast(fam === 'sdi' ? t('Define SDI inputs in Hardware') : t('Define ST 2110 input flows in Hardware'), 'info');
    }
    const ids = new Set(inputs.map((i) => i.id));
    let n = 1;
    while (ids.has(`input-${n}`)) n++;
    const id = `input-${n}`;
    let port = 10000;
    const used = new Set(inputs.map((i) => i.port).filter(Boolean));
    while (used.has(port)) port++;
    const input: InputConfig =
      fam === 'ndi'
        ? { id, name: t('Input {n}', { n }), kind: 'ndi', url: '', listen: false, ndiSource: '', device: 0, width: 1920, height: 1080, fps: 25, buffer: 2, hwDecode: false }
        : { id, name: t('Input {n}', { n }), kind: 'srt', url: '', port, latency: 120, listen: true, ndiSource: '', device: 0, width: 1920, height: 1080, fps: 25, buffer: 2, hwDecode: false };
    edit((c) => {
      (c.inputs ??= []).push(input);
      const cc = c.channels[chIndex];
      cc.sources = [...(cc.sources ?? []), id];
    });
    setSel(`src:${id}`);
  };
  const selSrc = sel.startsWith('src:') ? inputOf(sel.slice(4)) : undefined;

  const removeSelected = async () => {
    if (selSrc) {
      editCh((c) => {
        c.sources = (c.sources ?? []).filter((x) => x !== selSrc.id);
        if (!c.sources.length) delete c.sources;
        if (c.background === selSrc.id) delete c.background;
      });
      setSel('program');
      return;
    }
    if (selDest) {
      editCh((c) => {
        const list = listOf(outputsOf(c, selDest.picture), selDest.kind) as { id: string }[];
        list.splice(
          list.findIndex((x) => x.id === selDest.value.id),
          1,
        );
        if (c.unwired && !dests(unwiredOf(c)).length) delete c.unwired;
      });
    } else if (selPic && selPic.id !== 'main') {
      if (dests(selPic).length && !(await confirmDialog({ title: t('Remove the format “{name}”?', { name: selPic.name }), message: tn(dests(selPic).length, 'Its {n} destination is removed too.', 'Its {n} destinations are removed too.'), confirm: t('Remove'), danger: true }))) return;
      editCh((c) => void (c.renditions = (c.renditions ?? []).filter((r) => r.id !== selPic.id)));
    }
    setSel('program');
  };

  /** Feeds a destination from another format ("": unwires it). */
  const moveDest = (destId: string, to: string) =>
    editCh((c) => {
      const d = [...pictures(c).flatMap(dests), ...dests(unwiredOf(c))].find((x) => x.value.id === destId);
      if (!d || d.picture === to) return;
      if (to && !pictures(c).some((x) => x.id === to)) return;
      const list = listOf(outputsOf(c, d.picture), d.kind) as { id: string }[];
      const [item] = list.splice(
        list.findIndex((x) => x.id === destId),
        1,
      );
      (listOf(outputsOf(c, to), d.kind) as { id: string }[]).push(item);
      if (c.unwired && !dests(unwiredOf(c)).length) delete c.unwired;
    });

  const editDestById = <T,>(id: string, fn: (d: T) => void) =>
    editCh((c) => {
      const d = [...pictures(c).flatMap(dests), ...dests(unwiredOf(c))].find((x) => x.value.id === id);
      if (d) fn(d.value as T);
    });
  const editDest = <T,>(fn: (d: T) => void) => selDest && editDestById<T>(selDest.value.id, fn);
  const setOn = (id: string, on: boolean) => editDestById<{ enabled?: boolean }>(id, (d) => void (d.enabled = on));
  const setSlate = (picture: string, src: string) =>
    editCh((c) => {
      const target = picture === 'main' ? c : (c.renditions ?? []).find((r) => r.id === picture);
      if (!target) return;
      if (src) Object.assign(target, { slate: src, slateStyle: 'custom' });
      else delete target.slate;
    });
  const setSlateStyle = (picture: string, style: SlateStyle) =>
    editCh((c) => {
      const target = picture === 'main' ? c : (c.renditions ?? []).find((r) => r.id === picture);
      if (target) target.slateStyle = style;
    });

  const addLocation = async () => {
    const name = await promptDialog({ title: t('New location'), label: t('Name'), placeholder: t('A studio, a truck, the web desk…'), confirm: t('Add') });
    if (!name) return;
    edit((c) => {
      c.locations = c.locations?.length ? c.locations : [...locations];
      c.locations.push({ id: unique(slug(name), new Set(c.locations.map((l) => l.id))), name });
    });
  };
  const renameLocation = async (id: string, old: string) => {
    const name = await promptDialog({ title: t('Rename the location'), label: t('Name'), value: old, confirm: t('Rename') });
    if (!name || name === old) return;
    edit((c) => {
      c.locations = c.locations?.length ? c.locations : [...locations];
      c.locations.find((l) => l.id === id)!.name = name;
    });
  };
  const removeLocation = async (id: string, name: string) => {
    if (locations.length < 2) return toast(t('Keep at least one location'), 'error');
    const n = cfg.channels.filter((c) => c.location === id).length;
    if (!(await confirmDialog({ title: t('Remove the location “{name}”?', { name }), message: n ? tn(n, 'Its {n} channel moves to “{name}”.', 'Its {n} channels move to “{name}”.', { name: locations.find((l) => l.id !== id)!.name }) : undefined, confirm: t('Remove'), danger: true }))) return;
    edit((c) => {
      c.locations = (c.locations?.length ? c.locations : [...locations]).filter((l) => l.id !== id);
      for (const x of c.channels) if (x.location === id) x.location = c.locations[0].id;
    });
  };
  const addChannel = (loc: string) => {
    const ids = new Set(cfg.channels.map((c) => c.id));
    let n = cfg.channels.length + 1;
    while (ids.has(`channel-${n}`)) n++;
    const first = cfg.channels[0];
    const id = `channel-${n}`;
    edit((c) => {
      c.locations = c.locations?.length ? c.locations : [...locations];
      c.channels.push({ id, name: t('Channel {n}', { n }), width: first?.width ?? 1920, height: first?.height ?? 1080, frameRate: first?.frameRate ?? { num: 50, den: 1 }, projectId: null, location: loc, outputs: { st2110: [], sdi: [], ndi: [], html5: [{ id: 'html5-1', name: t('Web page'), background: '' }], streams: [] }, renditions: [] });
    });
    setCurrent(id);
    setSel('program');
  };
  const removeChannel = async () => {
    if (cfg.channels.length < 2) return;
    if (!(await confirmDialog({ title: t('Remove the channel “{name}”?', { name: ch.name }), message: t('All its outputs are removed too.'), confirm: t('Remove'), danger: true }))) return;
    edit((c) => void c.channels.splice(chIndex, 1));
    setCurrent(null);
    setSel('program');
  };

  const apply = async () => {
    setSaving(true);
    try {
      appliedAt.current = Date.now();
      await api('PUT', '/api/system/config', cfg);
      toast(outputsDirty ? t('Outputs applied — the engine is restarting') : t('Inputs applied'), 'info');
      // The server may still be restarting: try again until it answers.
      const reload = (tries: number) => void load().catch(() => tries > 0 && setTimeout(() => reload(tries - 1), 1000));
      setTimeout(() => reload(15), 1500);
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setSaving(false);
    }
  };

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(text);
      setTimeout(() => setCopied((c) => (c === text ? null : c)), 1500);
    } catch {
      toast(text);
    }
  };

  // ---- drag ----
  const grab = (e: React.PointerEvent, id: string) => {
    if (e.button !== 0) return;
    e.preventDefault();
    setSel(id);
    if ((e.target as HTMLElement).closest('.cv-resize')) return;
    const start = { mx: e.clientX, my: e.clientY, ...pos[id] };
    const chId = ch.id;
    const z = cv.view.zoom;
    const move = (ev: PointerEvent) =>
      setLayout((l) => ({ ...l, [chId]: { ...(l[chId] ?? {}), [id]: { x: Math.round(start.x + (ev.clientX - start.mx) / z), y: Math.round(start.y + (ev.clientY - start.my) / z) } } }));
    document.body.classList.add('cv-moving');
    const up = () => {
      document.body.classList.remove('cv-moving');
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  // ---- wiring with the mouse ----
  // From a picture's output: drop on a destination to feed it, or in the
  // void to create one there. From a destination's input: drop on another
  // picture to feed it from there.
  const planePoint = (ev: { clientX: number; clientY: number }) => {
    return cv.toWorld(ev.clientX, ev.clientY);
  };
  const startWire = (e: React.PointerEvent, from: string, moving?: string) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    setAddMenu(null);
    setWire({ from, moving, ...planePoint(e) });
    const move = (ev: PointerEvent) => setWire((w) => w && { ...w, ...planePoint(ev) });
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      setWire(null);
      const target = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)?.closest<HTMLElement>('[data-node]')?.dataset.node;
      // From the program: onto a source, its video goes under the graphics;
      // onto a format, that format gets the graphics only.
      if (from === 'program') {
        if (target?.startsWith('src:')) editCh((c) => void (c.background = target.slice(4)));
        else if (target?.startsWith('pic:')) editCh((c) => void (c.background && (c.graphicsOnly = [...new Set([...(c.graphicsOnly ?? []), target.slice(4)])])));
        return;
      }
      // From a source: onto a format, it gets the graphics over this video.
      if (from.startsWith('src:')) {
        if (target?.startsWith('pic:'))
          editCh((c) => {
            c.background = from.slice(4);
            c.graphicsOnly = (c.graphicsOnly ?? []).filter((x) => x !== target.slice(4));
            if (!c.graphicsOnly.length) delete c.graphicsOnly;
          });
        return;
      }
      const targetPic = target?.startsWith('pic:') ? target.slice(4) : undefined;
      if (moving) {
        if (targetPic) moveDest(moving, targetPic);
        return;
      }
      if (target && !target.startsWith('pic:') && target !== 'program') moveDest(target, from);
      else if (!target) {
        const at = planePoint(ev);
        setAddMenu({ picture: from, x: at.x, y: at.y });
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  const selNode = nodes.find((n) => n.id === sel) ?? nodes[0];
  const destCount = (c: ChannelConfig) => pictures(c).reduce((n, p) => n + dests(p).length, 0);
  const warns = (c: ChannelConfig) => Object.values(status[c.id] ?? {}).some((s) => !s.ok);

  // The inspector's notes, gathered while it renders (shown as one "i").
  const notes: string[] = [];
  const note = (text: string) => void notes.push(text);
  return (
    <div className="vs-page">
      {mode === 'inputs' ? (
        <InputsMode cfg={cfg} saved={info.config} edit={edit} />
      ) : (
        <div className="outputs-page">
          <aside className="lib-nav">
            <div className="lib-nav-section">
              <span className="grow">
                {t('Locations')}
                <InfoTip text={t('A location groups the channels of one place: a studio, a truck, the web desk… Each channel is an independent playout.')} />
              </span>
              <button className="icon-btn ghost" title={t('Add a location')} onClick={addLocation}>
                <Plus size={14} />
              </button>
            </div>
            {locations.map((l) => {
              const list = cfg.channels.filter((c) => (c.location || locations[0].id) === l.id);
              return (
                <div key={l.id} className="oc-location">
                  <div className="oc-loc-head">
                    <MapPin size={14} />
                    <span className="grow" onDoubleClick={() => renameLocation(l.id, l.name)} title={t('Double-click to rename')}>
                      {l.name}
                    </span>
                    <span className="lib-nav-actions">
                      <button className="icon-btn ghost" title={t('Rename')} onClick={() => renameLocation(l.id, l.name)}>
                        <Pencil size={12} />
                      </button>
                      <button className="icon-btn ghost" title={t('Remove the location')} onClick={() => removeLocation(l.id, l.name)}>
                        <Trash2 size={12} />
                      </button>
                    </span>
                    <span className="count">{list.length}</span>
                  </div>
                  {list.map((c) => (
                    <button
                      key={c.id}
                      className={`oc-channel ${c.id === ch?.id ? 'on' : ''}`}
                      onClick={() => {
                        setCurrent(c.id);
                        setSel('program');
                      }}
                    >
                      <span className={`oc-dot ${warns(c) ? 'warn' : 'ok'}`} />
                      <span className="grow">
                        <span>{c.name}</span>
                        <small>
                          {formatLabel(c)} · {tn(destCount(c), '{n} destination', '{n} destinations')}
                        </small>
                      </span>
                    </button>
                  ))}
                  <button className="oc-channel add" onClick={() => addChannel(l.id)}>
                    <Plus size={13} /> {t('Add a channel')}
                  </button>
                </div>
              );
            })}
          </aside>

          {ch && (
            <main className={`oc-main ${panel ? 'ux3-panel-open' : ''}`}>
              <header className="oc-head">
                <div>
                  <div className="row">
                    <h1>{ch.name}</h1>
                    <span className="chip">{locations.find((l) => l.id === (ch.location || locations[0].id))?.name}</span>
                  </div>
                  <span className="muted small">
                    {formatLabel(ch)} · {projectName(liveCh?.projectId) ? t('plays “{name}”', { name: projectName(liveCh?.projectId)! }) : t('no project loaded')} · {tn(allDests.length, '{n} destination', '{n} destinations')}
                  </span>
                </div>
                <div className="spacer" />
                <div className="oc-tools">
                  <div className="oc-tool-group" role="group" aria-label={t('Formats')}>
                    <span className="oc-tool-label">{t('Formats')}</span>
                    {PICTURE_PRESETS.slice(0, TOOLBAR_PRESETS).map((p) => (
                      <button key={p.id} className="oc-tool" onClick={() => addPicture(p)} title={t('Add a {ratio} format ({size}): the same program in another shape', { ratio: p.name, size: `${p.width}×${p.height}` })}>
                        <Plus size={13} color={KIND.picture.color} /> {p.name}
                      </button>
                    ))}
                    <span className="oc-more" onPointerDown={(e) => e.stopPropagation()}>
                      <button className="oc-tool" onClick={() => setMoreFormats((v) => !v)} title={t('Other formats')} aria-label={t('Other formats')}>
                        <MoreHorizontal size={15} />
                      </button>
                      {moreFormats && (
                        <div className="menu oc-more-menu">
                          {PICTURE_PRESETS.slice(TOOLBAR_PRESETS).map((p) => (
                            <button key={p.id} onClick={() => addPicture(p)}>
                              <Frame size={14} color={KIND.picture.color} /> {p.name} · {p.width}×{p.height}
                            </button>
                          ))}
                        </div>
                      )}
                    </span>
                  </div>
                  <span className="oc-tool-sep" aria-hidden="true" />
                  <div className="oc-tool-group" role="group" aria-label={t('Sources')}>
                    <span className="oc-tool-label">{t('Sources')}</span>
                    {SRC_FAMILIES.map((f) => {
                      const look = famLook(f);
                      const Icon = look.icon;
                      const missing = (f === 'sdi' || f === 'st2110') && !inputs.some((i) => familyOf(i.kind) === f);
                      return (
                        <button key={f} className={`oc-tool ${missing ? 'unavailable' : ''}`} aria-disabled={missing} onClick={() => addSource(f)} title={missing ? (f === 'sdi' ? t('Define SDI inputs in Hardware') : t('Define ST 2110 input flows in Hardware')) : t('Add a {kind} source', { kind: t(look.label) })}>
                          <Icon size={15} color={look.color} /> {t(look.label)}
                        </button>
                      );
                    })}
                  </div>
                  <span className="oc-tool-sep" aria-hidden="true" />
                  <div className="oc-tool-group" role="group" aria-label={t('Destinations')}>
                    <span className="oc-tool-label">{t('Destinations')}</span>
                    {DEST_KINDS.map((k) => {
                      const Icon = KIND[k].icon;
                      const missing = hwMissing(k);
                      return (
                        <button key={k} className={`oc-tool ${missing ? 'unavailable' : ''}`} aria-disabled={missing} onClick={() => addDest(k, '')} title={missing ? hwHint(k) : t('Add a {kind} destination', { kind: t(KIND[k].label) })}>
                          <Icon size={15} color={KIND[k].color} /> {t(KIND[k].label)}
                        </button>
                      );
                    })}
                  </div>
                </div>
              </header>

              <div className="oc-body">
                <div {...cv.viewport} className={`oc-canvas ${cv.viewport.className}`} onPointerDown={(e) => (setAddMenu(null), cv.viewport.onPointerDown(e))}>
                  <div ref={planeRef} data-canvas-void className={`oc-plane ${wire ? (wire.moving ? 'wiring-in' : 'wiring-out') : ''}`} style={{ width, height, ...cv.spaceStyle }} onPointerDown={() => setAddMenu(null)}>
                    {srcIds.length > 0 && (
                      <span className="oc-col" style={{ left: colX('source') }}>
                        {t('Sources')}
                      </span>
                    )}
                    <span className="oc-col" style={{ left: colX('program') }}>
                      {t('Program')}
                    </span>
                    <span className="oc-col" style={{ left: colX('picture') }}>
                      {t('Formats')}
                    </span>
                    <span className="oc-col" style={{ left: colX('dest') }}>
                      {t('Destinations')}
                    </span>
                    <svg className="oc-wires" width={width} height={height} aria-hidden="true">
                      {edges.map((e) => {
                        const a = pos[e.from];
                        const b = pos[e.to];
                        if (!a || !b || e.to === wire?.moving) return null;
                        const x1 = a.x + widthOf(e.from);
                        const y1 = a.y + 27;
                        const x2 = b.x;
                        const y2 = b.y + 27;
                        const c = Math.max(40, (x2 - x1) / 2);
                        const d = `M${x1},${y1} C${x1 + c},${y1} ${x2 - c},${y2} ${x2},${y2}`;
                        if (e.src)
                          return (
                            <g key={`${e.from}-${e.to}`} className="oc-wire-g">
                              <path d={d} stroke={e.color} />
                              <path
                                d={d}
                                className="oc-wire-hit"
                                onPointerDown={(ev) => {
                                  ev.stopPropagation();
                                  if (ev.button !== 0) return;
                                  editCh((c) => void delete c.background);
                                  setSel(e.from);
                                }}
                              >
                                <title>{t('Click to remove (the source stays, not connected)')}</title>
                              </path>
                            </g>
                          );
                        if (e.via) {
                          const via = e.via;
                          return (
                            <g key={`${e.from}-${e.to}`} className="oc-wire-g">
                              <path d={d} stroke={e.color} />
                              <path
                                d={d}
                                className="oc-wire-hit"
                                onPointerDown={(ev) => {
                                  ev.stopPropagation();
                                  if (ev.button !== 0) return;
                                  editCh((c) => void (c.graphicsOnly = [...new Set([...(c.graphicsOnly ?? []), via])]));
                                  setSel(`pic:${via}`);
                                }}
                              >
                                <title>{t('Click to feed this format the graphics only')}</title>
                              </path>
                            </g>
                          );
                        }
                        if (!e.dest) return <path key={`${e.from}-${e.to}`} d={d} stroke={e.color} />;
                        const dest = e.dest;
                        return (
                          <g key={`${e.from}-${e.to}`} className={`oc-wire-g ${allDests.find((x) => x.value.id === dest)?.value && !isOn(allDests.find((x) => x.value.id === dest)!) ? 'off' : ''}`}>
                            <path d={d} stroke={e.color} />
                            {/* A wider line to click. */}
                            <path
                              d={d}
                              className="oc-wire-hit"
                              onPointerDown={(ev) => {
                                ev.stopPropagation();
                                if (ev.button !== 0) return;
                                moveDest(dest, '');
                                setSel(dest);
                              }}
                            >
                              <title>{t('Click to remove (the destination stays, not connected)')}</title>
                            </path>
                          </g>
                        );
                      })}
                      {wire &&
                        pos[`pic:${wire.from}`] &&
                        (() => {
                          const a = pos[`pic:${wire.from}`];
                          const x1 = a.x + widthOf(`pic:${wire.from}`);
                          const y1 = a.y + 27;
                          const c = Math.max(40, Math.abs(wire.x - x1) / 2);
                          return <path className="oc-wire-live" d={`M${x1},${y1} C${x1 + c},${y1} ${wire.x - c},${wire.y} ${wire.x},${wire.y}`} stroke={KIND.picture.color} />;
                        })()}
                      {wire &&
                        (wire.from === 'program' || wire.from.startsWith('src:')) &&
                        pos[wire.from] &&
                        (() => {
                          const a = pos[wire.from];
                          const x1 = a.x + widthOf(wire.from);
                          const y1 = a.y + 27;
                          const c = Math.max(40, Math.abs(wire.x - x1) / 2);
                          return <path className="oc-wire-live" d={`M${x1},${y1} C${x1 + c},${y1} ${wire.x - c},${wire.y} ${wire.x},${wire.y}`} stroke={KIND.source.color} />;
                        })()}
                      {wire &&
                        !wire.from &&
                        wire.moving &&
                        pos[wire.moving] &&
                        (() => {
                          // An unwired destination looking for its format: from the mouse to its input.
                          const b = pos[wire.moving];
                          const y2 = b.y + 27;
                          const c = Math.max(40, Math.abs(b.x - wire.x) / 2);
                          return <path className="oc-wire-live" d={`M${wire.x},${wire.y} C${wire.x + c},${wire.y} ${b.x - c},${y2} ${b.x},${y2}`} stroke={KIND.picture.color} />;
                        })()}
                    </svg>
                    {nodes.map((n) => {
                      const k = { ...KIND[n.kind], ...(n.color ? { color: n.color } : {}) };
                      const Icon = n.icon ?? k.icon;
                      return (
                        <div key={n.id} data-node={n.id} className={`oc-node ${n.kind} ${n.id === selNode?.id ? 'sel' : ''} ${n.on === false ? 'off' : ''} ${n.loose ? 'loose' : ''}`} style={{ left: pos[n.id].x, top: pos[n.id].y, width: widthOf(n.id), ['--k' as string]: k.color }} onPointerDown={(e) => grab(e, n.id)} onDoubleClick={() => setPanel(true)}>
                          <div className="oc-node-head">
                            <span className="oc-icon">
                              <Icon size={15} />
                            </span>
                            <span className="grow">
                              <small>
                                {n.label ?? t(k.label)}
                                {n.badge && <span className="vs-badge">{n.badge}</span>}
                              </small>
                              <strong title={n.title}>{n.title}</strong>
                            </span>
                            {n.on !== undefined && <OnSwitch on={n.on} mini onChange={(v) => setOn(n.id, v)} />}
                          </div>
                          <div className="oc-node-sub">{n.sub}</div>
                          {n.extra}
                          <div className="oc-node-status">
                            <span className={`oc-dot ${n.tone}`} />
                            <span className="oc-status-text" title={n.raw ?? n.status}>
                              {n.status}
                            </span>
                            {n.chip && (
                              <span className={`vs-fps tone-${n.chip.tone}`} title={n.chip.title}>
                                {n.chip.text}
                              </span>
                            )}
                          </div>
                          {n.kind === 'picture' && <span className="oc-port in" />}
                          {n.kind !== 'program' && n.kind !== 'picture' && n.kind !== 'source' && (
                            <span className="oc-port in grab" title={t('Drag to a format to feed this output from it')} onPointerDown={(e) => startWire(e, allDests.find((d) => d.value.id === n.id)!.picture, n.id)} />
                          )}
                          {n.kind === 'program' && (srcIds.length > 0 ? <span className="oc-port out grab" title={t('Drag to a source to put its video under the graphics, or to a format for the graphics only')} onPointerDown={(e) => startWire(e, 'program')} /> : <span className="oc-port out" />)}
                          {n.kind === 'source' && <span className="oc-port in" />}
                          {n.kind === 'source' && <span className="oc-port out grab" title={t('Drag to a format to send it the graphics over this video')} onPointerDown={(e) => startWire(e, n.id)} />}
                          {n.kind === 'picture' && <span className="oc-port out grab" title={t('Drag to an output to feed it, or into the empty canvas to add one')} onPointerDown={(e) => startWire(e, n.id.slice(4))} />}
                          <ResizeCorner onPointerDown={(e) => cv.resize(e, (w) => setWidth(n.id, w))} />
                        </div>
                      );
                    })}
                    {addMenu && (
                      <div className="menu oc-add-menu" style={{ left: addMenu.x, top: addMenu.y }} onPointerDown={(e) => e.stopPropagation()}>
                        <div className="menu-label">{t('Add an output')}</div>
                        {DEST_KINDS.map((k) => {
                          const Icon = KIND[k].icon;
                          const missing = hwMissing(k);
                          return (
                            <button
                              key={k}
                              className={missing ? 'unavailable' : ''}
                              title={missing ? hwHint(k) : undefined}
                              onClick={() => {
                                addDest(k, addMenu.picture, { x: Math.max(0, Math.round(addMenu.x)), y: Math.max(30, Math.round(addMenu.y - 27)) });
                                setAddMenu(null);
                              }}
                            >
                              <Icon size={14} color={KIND[k].color} /> {t(KIND[k].label)}
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                  {cv.controls}
                </div>
                <button className="btn small ux3-drawer-btn" onClick={() => setPanel(true)}>
                  <PanelRightOpen size={14} /> {t('Properties')}
                </button>

                <aside className="oc-inspector">
                  {selNode && (
                    <div className="oc-ins-head" style={{ ['--k' as string]: selNode.color ?? KIND[selNode.kind].color }}>
                      <span className="oc-swatch" />
                      <div className="grow">
                        <small>{selNode.label ?? t(KIND[selNode.kind].label)}</small>
                        <strong>
                          {selNode.title}
                          {/* The notes of the panel (filled while it renders, below). */}
                          <Notes list={notes} />
                        </strong>
                      </div>
                      {selDest && selDest.kind !== 'html5' && <OnSwitch on={isOn(selDest)} onChange={(v) => setOn(selDest.value.id, v)} label={isOn(selDest) ? tc('output', 'On') : tc('output', 'Off')} />}
                      {(selDest || selSrc || (selPic && selPic.id !== 'main')) && (
                        <button className="icon-btn ghost" title={t('Remove')} onClick={removeSelected}>
                          <Trash2 size={15} />
                        </button>
                      )}
                      <button className="icon-btn ghost ux3-drawer-close" title={t('Close the panel')} onClick={() => setPanel(false)}>
                        <PanelRightClose size={15} />
                      </button>
                    </div>
                  )}
                  {selDest && !selDest.picture && note(t('Not connected: this destination is kept but not started. Drag a wire from a format to its input to send it again.'))}

                  {selNode?.kind === 'source' &&
                    selSrc &&
                    (() => {
                      const fam = familyOf(selSrc.kind);
                      const placed = new Set(ch.sources ?? []);
                      const choices = inputs.filter((i) => familyOf(i.kind) === fam && (i.id === selSrc.id || !placed.has(i.id)));
                      const wired = ch.background === selSrc.id;
                      return (
                        <>
                          <Field label={t('Input')}>
                            <select
                              value={selSrc.id}
                              onChange={(e) => {
                                const next = e.target.value;
                                editCh((c) => {
                                  c.sources = (c.sources ?? []).map((x) => (x === selSrc.id ? next : x));
                                  if (c.background === selSrc.id) c.background = next;
                                });
                                setSel(`src:${next}`);
                              }}
                            >
                              {choices.map((i) => (
                                <option key={i.id} value={i.id}>
                                  {i.name}
                                </option>
                              ))}
                            </select>
                          </Field>
                          <Field label={t('Program')}>
                            <select value={wired ? 'under' : ''} onChange={(e) => editCh((c) => void (e.target.value ? (c.background = selSrc.id) : delete c.background))}>
                              <option value="under">{t('Under the graphics')}</option>
                              <option value="">{t('Not connected')}</option>
                            </select>
                          </Field>
                          {(() => {
                            const c = srcCheck(selSrc.id);
                            if (!c.f) return null;
                            const f = c.f;
                            const chFps = ch.frameRate.num / Math.max(1, ch.frameRate.den);
                            const rate = (fps: number, fields: boolean) => (fields ? tn(Math.round(fps * 100) / 100, '{n} field/s', '{n} fields/s') : tn(Math.round(fps * 100) / 100, '{n} frame/s', '{n} frames/s'));
                            const rows: [string, string, string, boolean][] = [
                              [t('Size'), `${f.w}×${f.h}`, `${ch.width}×${ch.height}`, f.w === ch.width && f.h === ch.height],
                              [t('Rate'), rate(f.fps, f.interlaced), rate(chFps, !!ch.interlaced), Math.abs(f.fps - chFps) < 0.05],
                              [t('Scan'), f.interlaced ? t('interlaced') : t('progressive'), ch.interlaced ? t('interlaced') : t('progressive'), f.interlaced === !!ch.interlaced],
                            ];
                            return (
                              <div className="oc-src-check">
                                {rows.map(([k, a, b, same]) => (
                                  <div key={k} className={same ? '' : 'bad'}>
                                    <span>{k}</span>
                                    <span>{same ? a : `${a} ≠ ${b}`}</span>
                                  </div>
                                ))}
                                <div>
                                  <span>{t('Processing')}</span>
                                  <span>{c.state === 'same' ? t('Direct') : c.state === 'convert' ? t('Live conversion') : t('To choose')}</span>
                                </div>
                                <div>
                                  <span>{t('Added delay')}</span>
                                  <span>{selSrc.kind === 'sdi' ? tn(0, '{n} frame', '{n} frames') : tn(selSrc.buffer ?? 2, '{n} frame', '{n} frames')}</span>
                                </div>
                                {wired && sourceChoices(selSrc.id, c, true)}
                              </div>
                            );
                          })()}
                          <button type="button" className="btn small oc-hw-edit" onClick={() => navigate('/inputs')}>
                            <Pencil size={13} /> {t('Edit in Inputs')}
                          </button>
                          {note(t('The server keys the graphics over this video for the formats wired after the source; a format wired straight to the program gets the graphics only.'))}
                        </>
                      );
                    })()}

                  {selNode?.kind === 'program' && (
                    <>
                      <Field label={t('Name')}>
                        <input value={ch.name} onChange={(e) => editCh((c) => void (c.name = e.target.value))} />
                      </Field>
                      <Field label={t('Location')}>
                        <select value={ch.location || locations[0].id} onChange={(e) => edit((c) => void ((c.locations = c.locations?.length ? c.locations : [...locations]), (c.channels[chIndex].location = e.target.value)))}>
                          {locations.map((l) => (
                            <option key={l.id} value={l.id}>
                              {l.name}
                            </option>
                          ))}
                        </select>
                      </Field>
                      <Field label={t('Video format')} hint={t('Every format and destination of the channel runs at this rate.')}>
                        <select
                          value={videoFormatIndex(ch)}
                          onChange={(e) =>
                            editCh((c) => {
                              const f = VIDEO_FORMATS[+e.target.value];
                              if (!f) return;
                              c.width = f.width;
                              c.height = f.height;
                              c.frameRate = f.rate;
                              if (f.interlaced) c.interlaced = true;
                              else delete c.interlaced;
                            })
                          }
                        >
                          <option value={-1} disabled>
                            {formatLabel(ch)}
                          </option>
                          {VIDEO_FORMATS.map((f, i) => (
                            <option key={f.label} value={i}>
                              {f.label}
                            </option>
                          ))}
                        </select>
                      </Field>
                      {liveCh && (
                        <Field label={t('Project on air')} hint={t('Takes effect at once (not with Apply).')}>
                          <select
                            value={liveCh.projectId ?? ''}
                            onChange={async (e) => {
                              const onAir = Object.keys(liveState[ch.id]?.state.layers ?? {}).length;
                              if (onAir && !(await confirmDialog({ title: t('Load another project?'), message: tn(onAir, 'The {n} graphic on air on {channel} is cleared.', 'The {n} graphics on air on {channel} are cleared.', { channel: ch.name }), confirm: t('Load'), danger: true }))) return;
                              await loadOnChannel(ch.id, e.target.value || null);
                            }}
                          >
                            <option value="">{t('— no project —')}</option>
                            {projects.map((p) => (
                              <option key={p.id} value={p.id}>
                                {p.name}
                              </option>
                            ))}
                          </select>
                        </Field>
                      )}
                      <Field label={t('Render ahead')} hint={t('Frames rendered ahead, each on its own thread: absorbs slow frames; adds that many frames of delay.')}>
                        <select value={ch.renderAhead ?? 2} onChange={(e) => editCh((c) => void (c.renderAhead = +e.target.value))}>
                          {RENDER_AHEAD.map((n) => (
                            <option key={n} value={n}>
                              {tn(n, '{n} frame', '{n} frames')}
                              {` · ${Math.round((n * 1000 * ch.frameRate.den) / ch.frameRate.num)} ms`}
                            </option>
                          ))}
                        </select>
                      </Field>
                      <Field label={t('Clock reference')} hint={t('What the channel clock follows. PTP: frames on the ST 2059 grid of the PTP-locked system clock. DeckLink: the genlock of its first SDI destination.')}>
                        <select value={ch.clock ?? ''} onChange={(e) => editCh((c) => void (c.clock = (e.target.value || null) as ChannelConfig['clock']))}>
                          <option value="">{t('Default ({reference})', { reference: clockLabel(hardwareOf(cfg).sync?.reference ?? 'auto') })}</option>
                          {CLOCKS.map((r) => (
                            <option key={r} value={r}>
                              {clockLabel(r)}
                            </option>
                          ))}
                        </select>
                      </Field>
                      {note(t('The playout of this channel: what the operator takes goes to every format and destination wired to it.'))}
                      {cfg.channels.length > 1 && (
                        <button className="btn small danger" onClick={removeChannel}>
                          <Trash2 size={13} /> {t('Remove the channel')}
                        </button>
                      )}
                    </>
                  )}

                  {selPic && (
                    <>
                      {selPic.id === 'main' ? (
                        note(t("The channel's main format, {size} (its video format, set on the Program).", { size: `${selPic.width}×${selPic.height}` }))
                      ) : (
                        <>
                          <Field label={t('Name')}>
                            <input value={selPic.name} onChange={(e) => editCh((c) => void (c.renditions!.find((r) => r.id === selPic.id)!.name = e.target.value))} />
                          </Field>
                          <Field label={t('Size')}>
                            <select
                              value={PICTURE_PRESETS.findIndex((p) => p.width === selPic.width && p.height === selPic.height)}
                              aria-label={t('Size')}
                              onChange={(e) =>
                                editCh((c) => {
                                  const p = PICTURE_PRESETS[+e.target.value];
                                  const r = c.renditions!.find((x) => x.id === selPic.id)!;
                                  if (p) Object.assign(r, { width: p.width, height: p.height });
                                })
                              }
                            >
                              <option value={-1}>{t('Custom')}</option>
                              {PICTURE_PRESETS.map((p, i) => (
                                <option key={p.id} value={i}>
                                  {p.name} · {p.width}×{p.height}
                                </option>
                              ))}
                            </select>
                          </Field>
                          <div className="oc-pair">
                            <Field label={t('Width')}>
                              <input type="number" min={16} max={7680} value={selPic.width} onChange={(e) => editCh((c) => void (c.renditions!.find((x) => x.id === selPic.id)!.width = +e.target.value))} />
                            </Field>
                            <Field label={t('Height')}>
                              <input type="number" min={16} max={7680} value={selPic.height} onChange={(e) => editCh((c) => void (c.renditions!.find((x) => x.id === selPic.id)!.height = +e.target.value))} />
                            </Field>
                          </div>
                        </>
                      )}
                      <div className="set-field">
                        <span className="set-label">
                          {t('Still picture without project')}
                          <InfoTip text={t("Shown full frame on this format's outputs while no project is loaded.")} />
                        </span>
                        <div className="easing-picks">
                          {SLATES.map((sl) => {
                            const on = slateStyleOf(selPic) === sl.id;
                            const [tw, th] = thumbSize(selPic);
                            return (
                              <button
                                key={sl.id}
                                type="button"
                                className={on ? 'on' : ''}
                                title={sl.label()}
                                aria-pressed={on}
                                onClick={() => {
                                  setSlateStyle(selPic.id, sl.id);
                                  // No picture yet: choose one.
                                  if (sl.id === 'custom' && !selPic.slate) setSlatePick(selPic.id);
                                }}
                              >
                                {sl.id !== 'custom' || selPic.slate ? (
                                  <img src={slateThumb(sl.id, ch, selPic, tw)} width={tw} height={th} alt="" />
                                ) : (
                                  <svg width={tw} height={th} aria-hidden="true">
                                    <ImageOff x={(tw - 18) / 2} y={(th - 18) / 2} size={18} />
                                  </svg>
                                )}
                                <span>{sl.label()}</span>
                              </button>
                            );
                          })}
                        </div>
                        {slateStyleOf(selPic) === 'custom' && (
                          <div className="oc-slate">
                            <div className="oc-slate-info">
                              <span className="oc-slate-name" title={selPic.slate || undefined}>
                                {selPic.slate ? selPic.slate.split('/').pop() : <span className="muted">{t('None: black')}</span>}
                              </span>
                              <span className="row">
                                <button type="button" className="btn small" onClick={() => setSlatePick(selPic.id)}>
                                  {selPic.slate ? t('Change…') : t('Choose…')}
                                </button>
                                {selPic.slate && (
                                  <button type="button" className="btn small ghost" onClick={() => setSlate(selPic.id, '')}>
                                    {t('Clear')}
                                  </button>
                                )}
                              </span>
                            </div>
                          </div>
                        )}
                      </div>
                      {note(t("One take drives every format. Each one uses the project format closest in aspect ratio (a template's 9:16 layout, if it has one), letterboxed to fit."))}
                    </>
                  )}

                  {selDest?.kind === 'html5' &&
                    (() => {
                      const o = selDest.value as Html5Output;
                      const url = outputUrl(ch.id, selDest.picture, o.background);
                      return (
                        <>
                          <Field label={t('Name')}>
                            <input value={o.name} onChange={(e) => editDest<Html5Output>((d) => void (d.name = e.target.value))} />
                          </Field>
                          <Field label={t('Background')}>
                            <select value={o.background} onChange={(e) => editDest<Html5Output>((d) => void (d.background = e.target.value))}>
                              {BACKGROUNDS.map((b) => (
                                <option key={b.value} value={b.value}>
                                  {t(b.label)}
                                </option>
                              ))}
                            </select>
                          </Field>
                          <div className="set-field">
                            <span className="set-label">{t('Web link')}</span>
                            <div className="oc-link">
                              <code>{url}</code>
                              <button className="icon-btn" title={t('Copy')} onClick={() => void copy(url)}>
                                {copied === url ? <Check size={14} /> : <Copy size={14} />}
                              </button>
                              <a className="icon-btn" title={t('Open')} href={url} target="_blank" rel="noreferrer">
                                <ExternalLink size={14} />
                              </a>
                            </div>
                          </div>
                          {note(cfg.publicOutputs ? t('A web page playing the graphics live: browser source in OBS, vMix, CasparCG, a web player or a screen. No login needed.') : t('A web page playing the graphics live. Output pages need a login (Settings › Public output pages).'))}
                        </>
                      );
                    })()}

                  {selDest?.kind === 'sdi' &&
                    (() => {
                      const o = selDest.value as SdiOutput;
                      const def = hw.sdi.outputs.find((d) => d.id === o.hardware);
                      return (
                        <>
                          <Field label={t('Name')}>
                            <input value={o.name} onChange={(e) => editDest<SdiOutput>((d) => void (d.name = e.target.value))} />
                          </Field>
                          {(() => {
                            // A card, then one of its outputs still free (a keyer pair is one output).
                            const pick = (d: SdiOutput, next: (typeof hw.sdi.outputs)[number]) => {
                              const prev = hw.sdi.outputs.find((x) => x.id === d.hardware);
                              if (!prev || d.name === prev.name) d.name = next.name;
                              d.hardware = next.id;
                              fillSdi(d, hw);
                            };
                            const free = (cardId: string) => hw.sdi.outputs.filter((d) => d.card === cardId && (d.id === o.hardware || !usedHw.has(d.id)));
                            const cardId = def?.card ?? '';
                            return (
                              <>
                                <Field label={t('Card')}>
                                  <select
                                    value={cardId}
                                    onChange={(e) => {
                                      const next = free(e.target.value)[0];
                                      if (next) editDest<SdiOutput>((d) => pick(d, next));
                                    }}
                                  >
                                    {!def && <option value="">{t('— choose —')}</option>}
                                    {hw.sdi.cards.map((c) => {
                                      const n = free(c.id).length;
                                      return (
                                        <option key={c.id} value={c.id} disabled={!n && c.id !== cardId}>
                                          {c.name}
                                          {!n && c.id !== cardId ? ` (${t('no free output')})` : ''}
                                        </option>
                                      );
                                    })}
                                  </select>
                                </Field>
                                <Field label={t('Output')} hint={t('The outputs set on the card in Hardware.')}>
                                  <select
                                    value={o.hardware ?? ''}
                                    disabled={!cardId}
                                    onChange={(e) => {
                                      const next = hw.sdi.outputs.find((x) => x.id === e.target.value);
                                      if (next) editDest<SdiOutput>((d) => pick(d, next));
                                    }}
                                  >
                                    {!def && <option value="">{t('— choose —')}</option>}
                                    {free(cardId).map((d) => (
                                      <option key={d.id} value={d.id}>
                                        {sdiPorts(d)} · {modeLabel(d.mode)}
                                      </option>
                                    ))}
                                  </select>
                                </Field>
                              </>
                            );
                          })()}
                          {def && (
                            <dl className="oc-hw-sum">
                              <dt>Desktop Video</dt>
                              <dd>{t('device {n}', { n: o.device })}</dd>
                              <dt>Genlock</dt>
                              <dd>{def.genlock ? t('Yes') : t('No')}</dd>
                            </dl>
                          )}
                          <button type="button" className="btn small oc-hw-edit" onClick={() => navigate('/hardware')}>
                            <Cpu size={13} /> {t('Edit in Hardware')}
                          </button>
                          {note(t('Needs Blackmagic Desktop Video on this server. The status shows whether the card answered.'))}
                        </>
                      );
                    })()}

                  {selDest?.kind === 'st2110' &&
                    (() => {
                      const o = selDest.value as St2110Output;
                      const enc = o.encoding ?? 'fillKey';
                      const two = enc === 'fillKey';
                      const flow = outFlows.find((f) => f.id === o.hardware);
                      const sdp = (k: 'fill' | 'key') => `/api/channels/${encodeURIComponent(ch.id)}/outputs/${encodeURIComponent(o.id)}/${k}.sdp`;
                      return (
                        <>
                          <Field label={t('Name')}>
                            <input value={o.name} onChange={(e) => editDest<St2110Output>((d) => void (d.name = e.target.value))} />
                          </Field>
                          <Field label={t('ST 2110 flow')} hint={t('The multicast addresses are set in Hardware.')}>
                            <select
                              value={o.hardware ?? ''}
                              onChange={(e) =>
                                editDest<St2110Output>((d) => {
                                  const prev = outFlows.find((x) => x.id === d.hardware);
                                  const next = outFlows.find((x) => x.id === e.target.value);
                                  if (prev && d.name === prev.name && next) d.name = next.name;
                                  d.hardware = e.target.value;
                                  fillSt2110(d, hw);
                                })
                              }
                            >
                              {!flow && <option value="">{t('— choose —')}</option>}
                              {outFlows.map((f) => (
                                <option key={f.id} value={f.id}>
                                  {f.name} — {f.fill}
                                  {f.id !== o.hardware && usedHw.has(f.id) ? ` (${t('in use')})` : ''}
                                </option>
                              ))}
                            </select>
                          </Field>
                          {flow && (
                            <dl className="oc-hw-sum">
                              <dt>{t('Encoding')}</dt>
                              <dd>{t(ENCODINGS.find((x) => x.value === enc)?.label ?? enc)}</dd>
                              <dt>{two ? 'Fill' : t('Stream')}</dt>
                              <dd>
                                <code>{o.fill}</code>
                              </dd>
                              {two && (
                                <>
                                  <dt>Key</dt>
                                  <dd>{o.key ? <code>{o.key}</code> : t('no key')}</dd>
                                </>
                              )}
                              <dt>{t('Network interface')}</dt>
                              <dd>{o.interface ?? t('System default')}</dd>
                              <dt>TTL · PT</dt>
                              <dd>
                                {o.ttl} · {o.payloadType}
                              </dd>
                            </dl>
                          )}
                          <button type="button" className="btn small oc-hw-edit" onClick={() => navigate('/hardware#st2110')}>
                            <Cpu size={13} /> {t('Edit in Hardware')}
                          </button>
                          <div className="set-field">
                            <span className="set-label">SDP</span>
                            {savedDest(o.id) ? (
                              <span className="oc-sdp">
                                <a href={sdp('fill')}>{two ? 'fill.sdp' : `${enc}.sdp`}</a>
                                {two && o.key && <a href={sdp('key')}>key.sdp</a>}
                              </span>
                            ) : (
                              <span className="muted small">{t('Apply to generate the SDP files.')}</span>
                            )}
                          </div>
                          {note(two ? t('Two ST 2110-20 streams, what most vision mixers and keyers expect. RTP timing follows the system clock: keep this server PTP-disciplined.') : t('One stream carrying the alpha, for receivers that read this sampling (check your mixer accepts it). RTP timing follows the system clock: keep this server PTP-disciplined.'))}
                        </>
                      );
                    })()}

                  {selDest?.kind === 'ndi' &&
                    (() => {
                      const o = selDest.value as NdiOutput;
                      return (
                        <>
                          <Field label={t('Source name')} hint={t('Receivers see it as “SERVER (name)”.')}>
                            <input value={o.name} onChange={(e) => editDest<NdiOutput>((d) => void (d.name = e.target.value))} />
                          </Field>
                          <Field label={t('Groups')} hint={t('Comma-separated; empty = public.')}>
                            <input value={o.groups} placeholder="public" onChange={(e) => editDest<NdiOutput>((d) => void (d.groups = e.target.value))} />
                          </Field>
                          <label className="check">
                            <input type="checkbox" checked={o.alpha} onChange={(e) => editDest<NdiOutput>((d) => void (d.alpha = e.target.checked))} />
                            {t('Send the alpha channel')}
                          </label>
                          <Field label={t('NDI runtime (all NDI outputs)')} hint={t('Leave empty to use the NDI runtime installed with Strata. Only to force another libndi.')}>
                            <input value={cfg.ndiLibrary ?? ''} placeholder={info.os === 'macos' ? '/usr/local/lib/libndi.dylib' : info.os === 'windows' ? 'C:\\Program Files\\NDI\\…\\Processing.NDI.Lib.x64.dll' : '/usr/lib/libndi.so.6'} onChange={(e) => edit((c) => void (c.ndiLibrary = e.target.value))} />
                          </Field>
                          {note(t('Seen by every NDI receiver on the network (TriCaster, vMix, OBS, Studio Monitor…). The NDI runtime comes with the Strata installers (macOS, Windows).'))}
                        </>
                      );
                    })()}

                  {selDest?.kind === 'stream' &&
                    (() => {
                      const o = selDest.value as StreamOutput;
                      const proto = PROTOCOLS.find((p) => p.value === o.protocol) ?? PROTOCOLS[0];
                      const hls = hlsUrl(ch.id, selDest.picture, o.id);
                      const setProtocol = (p: StreamProtocol) =>
                        editDest<StreamOutput>((d) => {
                          if (d.protocol === p) return;
                          // Keep the port when going from SRT to UDP and back; else start again.
                          const port = /:(\d+)(?:[/?]|$)/.exec(d.url)?.[1];
                          d.protocol = p;
                          d.url = p === 'srt' ? `srt://0.0.0.0:${port ?? 9000}` : p === 'udp' ? `udp://239.1.1.1:${port ?? 5000}` : '';
                          d.listen = p === 'srt';
                          if (p === 'rtmp') d.audio = true;
                        });
                      return (
                        <>
                          <Field label={t('Name')}>
                            <input value={o.name} onChange={(e) => editDest<StreamOutput>((d) => void (d.name = e.target.value))} />
                          </Field>
                          <Group label={t('Protocol')}>
                            <div className="seg vs-seg">
                              {PROTOCOLS.map((p) => (
                                <button key={p.value} type="button" className={o.protocol === p.value ? 'on' : ''} onClick={() => setProtocol(p.value)}>
                                  {p.label}
                                </button>
                              ))}
                            </div>
                          </Group>
                          {o.protocol === 'srt' && (
                            <Group label={t('Mode')} hint={t(SRT_MODES.find((m) => m.listen === !!o.listen)!.hint)}>
                              <div className="seg vs-seg">
                                {SRT_MODES.map((m) => (
                                  <button
                                    key={m.label}
                                    type="button"
                                    className={!!o.listen === m.listen ? 'on' : ''}
                                    onClick={() =>
                                      editDest<StreamOutput>((d) => {
                                        if (!!d.listen === m.listen) return;
                                        // An address still the default of the other mode follows.
                                        const port = /:(\d+)(?:[/?]|$)/.exec(d.url)?.[1] ?? '9000';
                                        const def = (listen: boolean) => (listen ? `srt://0.0.0.0:${port}` : `srt://host:${port}`);
                                        const was = !d.url.trim() || d.url === def(!!d.listen);
                                        d.listen = m.listen;
                                        if (was) d.url = def(m.listen);
                                      })
                                    }
                                  >
                                    {t(m.label)}
                                  </button>
                                ))}
                              </div>
                            </Group>
                          )}
                          {o.protocol === 'hls' ? (
                            <div className="set-field">
                              <span className="set-label">
                                {t('Playback address')}
                                {savedDest(o.id) ? <InfoTip text={t('Served by this server, for players and CDNs that pull HLS.')} /> : <InfoTip text={t('Apply to start the stream.')} />}
                              </span>
                              <div className="oc-link">
                                <code title={hls}>{hls}</code>
                                <button className="icon-btn" title={t('Copy')} onClick={() => void copy(hls)}>
                                  {copied === hls ? <Check size={14} /> : <Copy size={14} />}
                                </button>
                              </div>
                            </div>
                          ) : (
                            <Field
                              label={t('Address')}
                              hint={
                                o.protocol === 'srt'
                                  ? o.listen
                                    ? t('srt://0.0.0.0:9000 listens on every interface: the receivers call srt://{host}:{port}.', { host: location.hostname, port: /:(\d+)(?:[/?]|$)/.exec(o.url)?.[1] ?? '9000' })
                                    : t("The receiver's address, e.g. srt://host:9000 (it must be listening).")
                                  : o.protocol === 'udp'
                                    ? t('A multicast group (udp://239.1.1.1:5000) or a host (udp://10.0.0.20:5000).')
                                    : t('The server and stream key given by the platform, e.g. rtmp://a.rtmp.youtube.com/live2/<key>.')
                              }
                            >
                              <input value={o.url} placeholder={o.protocol === 'srt' && o.listen ? 'srt://0.0.0.0:9000' : proto.placeholder} spellCheck={false} onChange={(e) => editDest<StreamOutput>((d) => void (d.url = e.target.value))} />
                            </Field>
                          )}
                          <div className="oc-pair">
                            <Field label={t('Bit rate (kbit/s)')}>
                              <input type="number" min={100} max={100000} step={500} value={o.bitrate} onChange={(e) => editDest<StreamOutput>((d) => void (+e.target.value > 0 && (d.bitrate = Math.round(+e.target.value))))} />
                            </Field>
                            <Field label={t('Codec')}>
                              <select value={o.codec} onChange={(e) => editDest<StreamOutput>((d) => void (d.codec = e.target.value as StreamOutput['codec']))}>
                                <option value="h264">H.264</option>
                                <option value="hevc">HEVC</option>
                              </select>
                            </Field>
                          </div>
                          <Group label={t('Background colour')}>
                            <span className="vs-color">
                              <HexColorField value={hexColor(o.background)} onChange={(c) => editDest<StreamOutput>((d) => void (d.background = c))} />
                            </span>
                          </Group>
                          <label className="check">
                            <input type="checkbox" checked={o.audio} onChange={(e) => editDest<StreamOutput>((d) => void (d.audio = e.target.checked))} />
                            {t('Audio track (silent stereo)')}
                          </label>
                          {o.protocol === 'rtmp' && !o.audio && note(t('Most RTMP platforms (YouTube, Facebook, Twitch) refuse a stream without audio.'))}
                          <div className="set-field">
                            <span className="ux3-row">
                              <OnSwitch on={!!o.hwEncode} onChange={(v) => editDest<StreamOutput>((d) => void (d.hwEncode = v))} label={t('Hardware encoding (GPU)')} />
                              <InfoTip text={t('VideoToolbox on a Mac, NVENC / Quick Sync / AMF elsewhere; falls back to the processor when none works.')} />
                            </span>
                          </div>
                          {note(`${t('Streams carry no transparency: the graphics are laid over the background colour.')} ${t('Encoded by ffmpeg, which must be installed on the server.')}`)}
                        </>
                      );
                    })()}

                  {selDest && (selDest.kind !== 'html5' || !selDest.picture) && (
                    <div className={`oc-state ${destStatus(selDest).tone}`}>
                      <span className={`oc-dot ${destStatus(selDest).tone}`} />
                      {destStatus(selDest).text}
                    </div>
                  )}
                  {selDest &&
                    selDest.kind !== 'html5' &&
                    chMetrics &&
                    (() => {
                      const o = outMetrics(selDest.value.id);
                      if (!o) return null;
                      return (
                        <div className="vs-perf">
                          <div>
                            <small>FPS</small>
                            <strong className={`tone-${o.ok ? fpsTone(o.fps, chMetrics.rate) : 'bad'}`}>
                              {fpsText(o.fps)} / {fpsText(chMetrics.rate)}
                            </strong>
                          </div>
                          <div>
                            <small>{t('Dropped (10 s)')}</small>
                            <strong className={o.dropped ? 'tone-warn' : ''}>{num(o.dropped)}</strong>
                          </div>
                          <div>
                            <small>CPU</small>
                            <strong>{corePct(o.cpu)}</strong>
                          </div>
                        </div>
                      );
                    })()}
                  {selDest && isOn(selDest) && selDest.picture && status[ch.id]?.[selDest.value.id]?.detail && status[ch.id][selDest.value.id].detail !== destStatus(selDest).text && (
                    <details className="oc-detail">
                      <summary>{t('Details')}</summary>
                      <code>{status[ch.id][selDest.value.id].detail}</code>
                    </details>
                  )}
                </aside>
              </div>
            </main>
          )}
        </div>
      )}

      {slatePick !== null && ch && (
        <AssetLibrary
          kind="image"
          title={t('Still picture without project')}
          value={pics.find((p) => p.id === slatePick)?.slate ?? ''}
          onClose={() => setSlatePick(null)}
          onChoose={(src) => {
            setSlate(slatePick, src);
            setSlatePick(null);
          }}
        />
      )}

      <div className={`save-bar ${dirty ? 'show' : ''}`}>
        <span>{outputsDirty ? t('Unapplied output changes. Applying restarts the outputs of every channel.') : t('Unapplied input changes. Applying restarts only the inputs that changed.')}</span>
        <div className="spacer" />
        <button className="btn" onClick={() => setCfg(structuredClone(info.config))}>
          {t('Discard')}
        </button>
        <button className="btn primary" disabled={saving} onClick={apply}>
          <Save size={14} /> {saving ? t('Applying…') : t('Apply')}
        </button>
      </div>
    </div>
  );
}

function destTitle(d: Dest): string {
  switch (d.kind) {
    case 'html5':
      return (d.value as Html5Output).name || t('Web page');
    case 'st2110':
      return d.value.name;
    default:
      return d.value.name;
  }
}

function destSub(d: Dest, ch: ChannelConfig, hw: HardwareConfig): string {
  switch (d.kind) {
    case 'html5': {
      const o = d.value as Html5Output;
      const bg = BACKGROUNDS.find((b) => b.value === o.background);
      return `/output/${ch.id}${d.picture === 'main' ? '' : `/${d.picture}`}\n${bg ? t(bg.label) : o.background}`;
    }
    case 'sdi': {
      const o = d.value as SdiOutput;
      const def = hw.sdi.outputs.find((x) => x.id === o.hardware);
      const where = def ? sdiOutputLabel(hw, def) : `DeckLink #${o.device}`;
      return `${where}\n${o.fillOnly ? t('Fill only') : o.externalKey ? 'Fill + Key' : t('Internal keyer')}${o.genlock ? ' · genlock' : ''}`;
    }
    case 'st2110': {
      const o = d.value as St2110Output;
      const enc = o.encoding ?? 'fillKey';
      return enc === 'fillKey' ? `${o.fill}\n${o.key ? `key ${o.key}` : t('no key')}` : `${enc === 'rgba' ? 'RGBA' : 'YCbCrA'}\n${o.fill}`;
    }
    case 'ndi': {
      const o = d.value as NdiOutput;
      return `${o.groups || 'public'}\n${o.alpha ? t('with alpha') : t('no alpha')}`;
    }
    case 'stream': {
      const o = d.value as StreamOutput;
      const where = o.protocol === 'hls' ? `/hls/${ch.id}/${d.picture}/${o.id}/` : o.url || '—';
      return `${where}\n${o.codec === 'hevc' ? 'HEVC' : 'H.264'}${o.hwEncode ? ' GPU' : ''} · ${bitrateLabel(o.bitrate)}${o.audio ? '' : ` · ${t('no audio')}`}`;
    }
  }
}

/** "16:9", "9:16"; "1.85:1" for the others. */
function ratioOf(w: number, h: number): string {
  const gcd = (a: number, b: number): number => (b ? gcd(b, a % b) : a);
  const g = gcd(w, h) || 1;
  const [a, b] = [w / g, h / g];
  return a <= 32 && b <= 32 ? `${a}:${b}` : `${(w / h).toFixed(2)}:1`;
}

const bitrateLabel = (kbps: number) => (kbps >= 1000 ? `${+(kbps / 1000).toFixed(1)} Mbit/s` : `${kbps} kbit/s`);

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="set-field">
      <span className="set-label">
        {label}
        {hint && <InfoTip text={hint} />}
      </span>
      {children}
    </label>
  );
}

/** A field holding buttons (a `<label>` would click the first one). */
function Group({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="set-field">
      <span className="set-label">
        {label}
        {hint && <InfoTip text={hint} />}
      </span>
      {children}
    </div>
  );
}

/** The notes of the inspector, as one "i" after its title. */
function Notes({ list }: { list: string[] }) {
  return list.length ? <InfoTip text={list.join('\n\n')} /> : null;
}

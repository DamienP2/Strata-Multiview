import type { FrameRate } from '../engine/types';
import type { MosConfig } from '../mos/MosPage';

/** Mirrors crates/cg-server/src/config.rs and crates/cg-output. */

export type St2110Encoding = 'fillKey' | 'rgba' | 'ycbcra';

export interface St2110Output {
  id: string;
  /** Off: kept in the settings, not started (true when missing). */
  enabled?: boolean;
  name: string;
  /** The ST 2110 output flow of Hardware it sends: the server copies the fields below from it. */
  hardware?: string;
  encoding?: St2110Encoding;
  fill: string;
  key?: string | null;
  interface?: string | null;
  ttl: number;
  payloadType: number;
}

export interface SdiOutput {
  id: string;
  enabled?: boolean;
  name: string;
  /** The SDI output of Hardware it sends to: the server copies the fields below from it. */
  hardware?: string;
  device: number;
  externalKey: boolean;
  fillOnly?: boolean;
  genlock?: boolean;
  /** Genlock offset in lines (filled by the server from the card). */
  genlockOffset?: number;
}

export interface NdiOutput {
  id: string;
  enabled?: boolean;
  name: string;
  groups: string;
  alpha: boolean;
}

export interface Html5Output {
  id: string;
  name: string;
  background: string;
}

export type StreamProtocol = 'srt' | 'udp' | 'rtmp' | 'hls';

/** An SRT, UDP, RTMP or HLS stream, encoded by ffmpeg (crates/cg-output/src/stream.rs). */
export interface StreamOutput {
  id: string;
  enabled?: boolean;
  name: string;
  protocol: StreamProtocol;
  /** `srt://host:port`, `udp://239.1.1.1:5000`, `rtmp://server/app/key` (none for HLS). */
  url: string;
  /** SRT: wait for the receivers to call (listener). */
  listen: boolean;
  /** Video bit rate, kbit/s (8000 by default). */
  bitrate: number;
  codec: 'h264' | 'hevc';
  /** Streams carry no alpha: the graphics are laid over this colour. */
  background: string;
  /** A silent stereo track (RTMP platforms usually need one). */
  audio: boolean;
  /** Hardware encoder (VideoToolbox, NVENC, Quick Sync, AMF); falls back to the processor. */
  hwEncode?: boolean;
}

export interface OutputsConfig {
  st2110: St2110Output[];
  sdi: SdiOutput[];
  ndi?: NdiOutput[];
  html5?: Html5Output[];
  streams?: StreamOutput[];
}

export type InputKind = 'srt' | 'udp' | 'rtmp' | 'hls' | 'rtsp' | 'url' | 'ndi' | 'sdi' | 'st2110';

/** A live video input (crates/cg-server/src/inputs.rs). */
export interface InputConfig {
  id: string;
  /** Off: kept in the settings, not started (true when missing). */
  enabled?: boolean;
  name: string;
  kind: InputKind;
  url: string;
  /** SRT / RTMP: wait for the sender to connect. */
  listen: boolean;
  /** NDI: the source name ("HOST (name)"). */
  ndiSource: string;
  /** SDI: DeckLink device index (filled by the server from `hardware`). */
  device: number;
  /** SDI / ST 2110: the input of Hardware it receives. */
  hardware?: string;
  /** SRT listener: the port the server listens on (srt://0.0.0.0:<port>). */
  port?: number;
  /** SRT latency, ms (120 by default). */
  latency?: number;
  /** Pre-detection decoded size and rate (no longer shown: the server detects them). */
  width: number;
  height: number;
  fps: number;
  /** Smoothing buffer, in frames (2 by default; 0 = lowest latency). */
  buffer: number;
  /** Streams: decode on the GPU when the machine has a decoder (ffmpeg falls back to the processor). */
  hwDecode: boolean;
}

/** `GET /api/inputs`. */
export interface InputInfo {
  id: string;
  name: string;
  kind: InputKind;
  /** A picture came in the last 2 s. */
  live: boolean;
  message: string;
  width: number;
  height: number;
  frames: number;
  /** Frames a second of the source (streams: as detected). */
  fps?: number;
}

/** What a format shows while no project is loaded: a built-in design (drawn by the server) or its still picture. */
export type SlateStyle = 'aurora' | 'bento' | 'gradient' | 'frame' | 'lines' | 'split' | 'transparent' | 'custom';

/** Not set (older settings): the still picture when there is one, else the first design (as the server). */
export const slateStyleOf = (p: { slate?: string; slateStyle?: SlateStyle }): SlateStyle => p.slateStyle ?? (p.slate?.trim() ? 'custom' : 'aurora');

export interface Rendition {
  id: string;
  name: string;
  width: number;
  height: number;
  outputs: OutputsConfig;
  /** Still picture (`/assets/…`) its outputs show while no project is loaded ("" = none). */
  slate?: string;
  slateStyle?: SlateStyle;
}

export interface ChannelConfig {
  id: string;
  name: string;
  width: number;
  height: number;
  frameRate: FrameRate;
  /** Interlaced SDI (1080i50): the frame rate counts fields; the SDI outputs weave two images into a frame. */
  interlaced?: boolean;
  /** The live input under the graphics (the source wired after the program). */
  background?: string | null;
  /** Pictures ("main", rendition ids) wired straight to the program: graphics only. */
  graphicsOnly?: string[];
  /** The source format chosen to be converted live ("1080p 25"). */
  sourceConvert?: string;
  /** The live inputs placed as sources in the canvas. */
  sources?: string[];
  projectId?: string | null;
  outputs: OutputsConfig;
  renditions?: Rendition[];
  location?: string;
  /** Still picture (`/assets/…`) the main format shows while no project is loaded ("" = none). */
  slate?: string;
  slateStyle?: SlateStyle;
  /** Frames rendered ahead, each on its own thread (1–4, 2 by default). */
  renderAhead?: number;
  /** Destinations wired to no format: kept, not started. */
  unwired?: OutputsConfig;
  /** What the channel clock follows (none: the Hardware default). */
  clock?: ClockReference | null;
}

export interface Location {
  id: string;
  name: string;
}

export interface ServerConfig {
  name: string;
  bind?: string | null;
  publicOutputs: boolean;
  timezone: string;
  /** How the machine keeps its time (NTP), see crates/cg-server/src/timectl.rs. */
  time?: TimeConfig;
  /** Path of the NDI runtime library ("" = search for it). */
  ndiLibrary?: string;
  locations?: Location[];
  channels: ChannelConfig[];
  mos: MosConfig;
  /** Live video inputs. */
  inputs?: InputConfig[];
  /** DeckLink cards and ST 2110 flows (Hardware page). */
  hardware?: HardwareConfig;
  /** OSC and TCP control (Bitfocus Buttons), see crates/cg-server/src/apiv1. */
  control?: { oscEnabled: boolean; oscPort: number; tcpEnabled: boolean; tcpPort: number; allow: string[] };
}

// ---- Hardware (crates/cg-server/src/hardware.rs) ----

export type SdiModel = 'miniMonitor' | 'miniRecorder' | 'sdi4k' | 'duo2' | 'quad2' | '8kPro' | 'other';

export interface SdiCard {
  id: string;
  name: string;
  model: SdiModel;
  /** Index of the card's first device in Desktop Video (0 = first). */
  device: number;
  /** What each connector is wired to; its role comes from the outputs and inputs using it. */
  connectors: { name: string }[];
  /** The model as Desktop Video names it; none: declared by hand. */
  modelName?: string;
  /** The identity of the detected card (`DetectedCard.id`). */
  persistentId?: string;
  /** The Desktop Video device of each connector (index = connector). */
  devices?: number[];
  /** A mode of its model (`DeckLinkModel.modes`). */
  mode?: string;
  /** Its outputs follow the reference input. */
  genlock?: boolean;
  /** Genlock offset, lines. */
  genlockOffset?: number;
}

export type SdiMode = 'externalKey' | 'internalKey' | 'fillOnly';

export interface SdiOutputDef {
  id: string;
  name: string;
  card: string;
  /** Connector of the fill (0 = first). */
  fill: number;
  /** Connector of the key (external keyer). */
  key?: number | null;
  mode: SdiMode;
  genlock: boolean;
}

export interface SdiInputDef {
  id: string;
  name: string;
  card: string;
  connector: number;
}

export interface St2110Flow {
  id: string;
  name: string;
  direction: 'output' | 'input';
  encoding: St2110Encoding;
  fill: string;
  key?: string | null;
  interface?: string | null;
  /** SMPTE 2022-7: the same packets again on the blue network, to these destinations. */
  blue?: string | null;
  blueKey?: string | null;
  /** Blue interface; none = the Media blue role. */
  blueInterface?: string | null;
  ttl: number;
  payloadType: number;
}

/** Which network interface (by name) does what. "" = no particular one. */
export interface NetworkRoles {
  control: string;
  ndi: string;
  ndiDiscovery: string;
  /** The machine name of the NDI sources (« NAME (source) »); '' = the computer's. */
  ndiMachine?: string;
  mediaRed: string;
  mediaBlue: string;
  ptp: string;
  /** The MOS ports listen on this one only ("" = every interface). */
  mos?: string;
}

export type ClockReference = 'auto' | 'free' | 'ptp' | 'decklink';

export interface SyncConfig {
  reference: ClockReference;
  ptpDomain: number;
  taiOffset: number;
  version?: number;
}

export interface TimeConfig {
  ntp: boolean;
  ntpServers: string[];
}

export const emptyTime = (): TimeConfig => ({ ntp: true, ntpServers: [] });

/** GET /api/system/time. */
export interface TimeState {
  supported: boolean;
  last?: { ok: boolean; message: string; at: number } | null;
  /** The server's time (ms). */
  now: number;
}

export interface HardwareConfig {
  sdi: { cards: SdiCard[]; outputs: SdiOutputDef[]; inputs: SdiInputDef[]; ignored?: string[] };
  st2110: { flows: St2110Flow[] };
  network?: NetworkRoles;
  sync?: SyncConfig;
}

export const emptyNetwork = (): NetworkRoles => ({ control: '', ndi: '', ndiDiscovery: '', mediaRed: '', mediaBlue: '', ptp: '', mos: '' });
export const emptySync = (): SyncConfig => ({ reference: 'auto', ptpDomain: 127, taiOffset: 37, version: 1 });

/** NTP / PTP status (crates/cg-server/src/sync.rs). */
export interface SyncStatus {
  ntp: { tool?: string | null; synced?: boolean | null; enabled?: boolean | null; source?: string | null; offsetMs?: number | null };
  ptp: { running: boolean; message?: string | null; domain: number; portState?: string | null; grandmaster?: string | null; offsetNs?: number | null; jitterNs?: number | null; locked: boolean; setsClock?: boolean };
  checkedAt: number;
}

export const emptyHardware = (): HardwareConfig => ({ sdi: { cards: [], outputs: [], inputs: [] }, st2110: { flows: [] } });

/** `GET /api/hardware`. */
export interface HardwareInfo {
  desktopVideo: boolean;
  desktopVideoPath?: string | null;
  decklinkSupport: boolean;
  st2110Receive: boolean;
  interfaces: { name: string; ip: string; loopback: boolean }[];
  os: string;
  decklinkDevices?: string[];
  decklinkError?: string | null;
  sync?: SyncStatus | null;
  /** The NDI network settings changed since the NDI runtime started. */
  ndiRestart?: boolean;
  decklinkCards?: DetectedCard[];
  decklinkModels?: DeckLinkModel[];
  /** Made-up cards (STRATA_FAKE_DECKLINK). */
  decklinkFake?: boolean;
  desktopVideoVersion?: string | null;
  nics?: Nic[];
}

export type PortCap = 'in' | 'out' | 'both';

/** A DeckLink card the driver lists (crates/cg-server/src/decklink.rs). */
export interface DetectedCard {
  id: string;
  modelName: string;
  legacyModel: SdiModel;
  connectors: number;
  /** The Desktop Video device of each connector (index = connector). */
  devices: number[];
  ports: PortCap[];
  hasReference: boolean;
  referenceLocked?: boolean | null;
  referenceKind?: 'blackBurst' | 'triLevel' | null;
  referenceFormat?: string | null;
  interface?: 'pcie' | 'usb' | 'thunderbolt' | null;
  topologicalId?: string | null;
  persistentId?: string | null;
  profile?: string | null;
  profiles: string[];
  /** The driver says the card has an internal / external keyer. */
  internalKeyer?: boolean;
  externalKeyer?: boolean;
}

/** What a card can key: on pairs of connectors (numbers as printed, the first one first). */
export interface Keying {
  internal: boolean;
  external: boolean;
  pairs: [number, number][];
  /** One Desktop Video profile for the whole card (8K Pro). */
  cardProfile: boolean;
}

/** The bracket of a model: connector numbers top to bottom (empty: numeric order). */
export interface DeckLinkModel {
  name: string;
  legacy: SdiModel;
  jack: 'din' | 'bnc';
  connectors: number;
  order: number[];
  reference?: boolean | null;
  ports: PortCap[];
  keying: Keying;
}

/** A network interface and its link. */
export interface Nic {
  name: string;
  ipv4: string[];
  mac?: string | null;
  mtu?: number | null;
  /** Mb/s. */
  speed?: number | null;
  up?: boolean | null;
  kind?: 'ethernet' | 'wifi' | 'virtual' | null;
  model?: string | null;
}

export interface SystemInfo {
  version: string;
  os: string;
  arch: string;
  hostname: string;
  cpus: number;
  dataDir: string;
  bind: string;
  uptimeSecs: number;
  timezone: string;
  config: ServerConfig;
  interfaces: { name: string; ip: string }[];
}

export interface OutputState {
  ok: boolean;
  message: string;
  /** The full error, when it says more. */
  detail?: string;
}

export const VIDEO_FORMATS: { label: string; width: number; height: number; rate: FrameRate; interlaced?: boolean }[] = [
  { label: '1080p 50', width: 1920, height: 1080, rate: { num: 50, den: 1 } },
  { label: '1080i 50', width: 1920, height: 1080, rate: { num: 50, den: 1 }, interlaced: true },
  { label: '1080i 59.94', width: 1920, height: 1080, rate: { num: 60000, den: 1001 }, interlaced: true },
  { label: '1080p 25', width: 1920, height: 1080, rate: { num: 25, den: 1 } },
  { label: '1080p 59.94', width: 1920, height: 1080, rate: { num: 60000, den: 1001 } },
  { label: '1080p 29.97', width: 1920, height: 1080, rate: { num: 30000, den: 1001 } },
  { label: '1080p 60', width: 1920, height: 1080, rate: { num: 60, den: 1 } },
  { label: '720p 50', width: 1280, height: 720, rate: { num: 50, den: 1 } },
  { label: '720p 59.94', width: 1280, height: 720, rate: { num: 60000, den: 1001 } },
  { label: '2160p 50 (UHD)', width: 3840, height: 2160, rate: { num: 50, den: 1 } },
  { label: '2160p 59.94 (UHD)', width: 3840, height: 2160, rate: { num: 60000, den: 1001 } },
];

/** The entry of VIDEO_FORMATS a channel uses (-1: none). */
export const videoFormatIndex = (ch: { width: number; height: number; frameRate: FrameRate; interlaced?: boolean }) =>
  VIDEO_FORMATS.findIndex((f) => f.width === ch.width && f.height === ch.height && f.rate.num * ch.frameRate.den === ch.frameRate.num * f.rate.den && !!f.interlaced === !!ch.interlaced);

export const splitAddr = (a?: string | null): [string, string] => {
  if (!a) return ['', ''];
  const i = a.lastIndexOf(':');
  return i < 0 ? [a, ''] : [a.slice(0, i), a.slice(i + 1)];
};
export const joinAddr = (ip: string, port: string) => `${ip.trim()}:${port.trim()}`;

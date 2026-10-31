// What an input carries, whatever its source (GET /api/inputs/{id}/analysis, every 2 s): its
// container and programs with their EPG, every stream (codec, profile and level, picture,
// colours, HDR, scan, audio, languages, subtitles, captions, data), its pictures (GOP, I/P/B,
// timecode, side data), and by transport the MPEG-TS tables and TR 101 290 checks, the HLS
// playlists, NDI and SDI details (server: crates/strata-mv-server/src/probe).
import { Activity, AudioLines, Captions, Clock, Database, FileVideo, Film, Layers, ListTree, Network, RefreshCw, Radio, ShieldAlert, Tv } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api } from '../store';
import { locale, t } from '../i18n';
import './analysis.css';

type KV = [string, string];

interface Event {
  id: number;
  start: string;
  durationS: number;
  name: string;
  text: string;
  language?: string;
  genre?: string;
  rating?: number;
  running?: string;
}

interface Program {
  number: number;
  name: string;
  provider: string;
  kind?: string;
  metadata?: KV[];
  streams: number[];
  now?: Event;
  next?: Event;
  scrambled: boolean;
  caSystems?: string[];
}

interface Stream {
  index: number;
  pid?: number;
  kind: string;
  codec: string;
  codecName: string;
  profile?: string;
  language?: string;
  title?: string;
  dispositions?: string[];
  bitrateKbps?: number;
  width?: number;
  height?: number;
  sar?: string;
  dar?: string;
  fps?: number;
  scan?: string;
  pixFmt?: string;
  depth?: string;
  range?: string;
  matrix?: string;
  primaries?: string;
  transfer?: string;
  hdr?: string;
  captions: boolean;
  sampleRate?: number;
  layout?: string;
  sampleFmt?: string;
  metadata?: KV[];
  details?: string[];
}

interface SideData {
  kind: string;
  count: number;
  last?: string;
  agoS: number;
}

interface Frames {
  frames: number;
  window: number;
  i: number;
  p: number;
  b: number;
  other: number;
  gop?: number;
  gopMin?: number;
  gopMax?: number;
  scan: string;
  progressive: number;
  tff: number;
  bff: number;
  pixFmt: string;
  size: string;
  sar: string;
  range?: string;
  matrix?: string;
  primaries?: string;
  transfer?: string;
  timecode?: string;
  timecodeKind?: string;
  captions: boolean;
  afd?: number;
  mastering?: string;
  light?: string;
  side: SideData[];
}

interface Es {
  pid: number;
  streamType: number;
  kind: string;
  class: string;
  language?: string;
  details?: string[];
  profile?: string;
}

interface Service {
  number: number;
  pmtPid: number;
  pcrPid?: number;
  name: string;
  provider: string;
  kind: string;
  scrambled: boolean;
  caSystems?: string[];
  streams: Es[];
}

interface Pid {
  pid: number;
  kind: string;
  packets: number;
  kbps: number;
  ccErrors: number;
  scrambled: boolean;
  pcr: boolean;
}

interface Splice {
  pid: number;
  command: string;
  eventId?: number;
  out?: boolean;
  immediate?: boolean;
  durationS?: number;
  ptsS?: number;
  segmentation?: string[];
  cancel: boolean;
  atS: number;
}

interface Check {
  priority: number;
  name: string;
  count: number;
  agoS?: number;
}

interface Ts {
  live: boolean;
  packets: number;
  seconds: number;
  kbps: number;
  tsId?: number;
  networkId?: number;
  network?: string;
  utc?: string;
  tables: string[];
  eitSchedule: boolean;
  services: Service[];
  pids: Pid[];
  splices: Splice[];
  spliceNulls: number;
  checks: Check[];
}

interface Variant {
  bandwidth: number;
  averageBandwidth?: number;
  resolution?: string;
  frameRate?: number;
  codecs?: string;
  videoRange?: string;
  audio?: string;
  subtitles?: string;
  closedCaptions?: string;
  hdcp?: string;
  uri: string;
}

interface Rendition {
  kind: string;
  group: string;
  name: string;
  language?: string;
  default: boolean;
  autoselect: boolean;
  forced: boolean;
  channels?: string;
  instreamId?: string;
  characteristics?: string;
  uri?: string;
}

interface Media {
  uri: string;
  version: number;
  targetDuration: number;
  mediaSequence: number;
  discontinuitySequence: number;
  kind: string;
  segments: number;
  totalS: number;
  segmentMinS: number;
  segmentMaxS: number;
  format: string;
  programDateTime?: string;
  discontinuities: number;
  encryption: string[];
  cues: string[];
  partTargetS?: number;
  byteRanges: boolean;
  gaps: number;
}

interface Hls {
  url: string;
  master: boolean;
  version: number;
  independentSegments: boolean;
  variants: Variant[];
  renditions: Rendition[];
  iframeStreams: number;
  session: string[];
  media?: Media;
  error?: string;
}

interface Ndi {
  source: string;
  width: number;
  height: number;
  rate: [number, number];
  aspect: number;
  format: string;
  timecode?: string;
  audioRate: number;
  audioChannels: number;
  audioSamples: number;
  frameMetadata?: string;
  metadata?: string;
  videoFrames: number;
  audioFrames: number;
}

interface Anc {
  did: number;
  sdid: number;
  line: number;
  name: string;
  count: number;
}

interface Sdi {
  device: number;
  mode: string;
  signal?: string;
  field?: string;
  timecode?: string;
  timecodeKind?: string;
  colorspace?: string;
  hdr?: string;
  audioChannels: number;
  anc: Anc[];
}

export interface Report {
  id: string;
  name: string;
  kind: string;
  source: string;
  live: boolean;
  status: string;
  how: string[];
  notes: string[];
  container?: { format: string; formatName: string; duration?: string; start?: number; bitrateKbps?: number; metadata?: KV[] };
  programs: Program[];
  streams: Stream[];
  frames?: Frames;
  ts?: Ts;
  hls?: Hls;
  ndi?: Ndi;
  sdi?: Sdi;
}

type Tab = 'summary' | 'programs' | 'streams' | 'pictures' | 'transport' | 'hls' | 'ndi' | 'sdi';

const hex = (n: number, w = 4) => `0x${n.toString(16).toUpperCase().padStart(w, '0')}`;
const kbps = (v?: number) => (v == null ? '—' : v >= 1000 ? `${(v / 1000).toFixed(2)} Mb/s` : `${v.toFixed(0)} kb/s`);
const fpsText = (v?: number) => (v == null ? '' : `${+v.toFixed(3)} i/s`);
const duration = (s: number) => {
  const h = Math.floor(s / 3600);
  const m = Math.round((s % 3600) / 60);
  return h ? `${h} h ${String(m).padStart(2, '0')}` : `${m} min`;
};
const clock = (iso: string) => (iso ? new Date(iso).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' }) : '');
const ago = (s?: number) => (s == null ? '' : s < 60 ? t('{n} s ago', { n: Math.round(s) }) : t('{n} min ago', { n: Math.round(s / 60) }));
const KIND_ICON: Record<string, typeof Film> = { video: Film, audio: AudioLines, subtitle: Captions, data: Database, attachment: FileVideo };

/** The analysis of an input, refreshed every 2 s while shown. */
export function InputAnalysis({ id, name, preview, off }: { id: string; name: string; preview: React.ReactNode; off?: boolean }) {
  const [r, setR] = useState<Report | null>(null);
  const [error, setError] = useState('');
  const [tab, setTab] = useState<Tab>('summary');
  const [busy, setBusy] = useState(false);
  const load = (refresh = false) =>
    api<Report>('GET', `/api/inputs/${encodeURIComponent(id)}/analysis${refresh ? '?refresh=true' : ''}`)
      .then((x) => {
        setR(x);
        setError('');
      })
      .catch((e) => setError((e as Error).message));
  useEffect(() => {
    setR(null);
    if (off) return;
    let alive = true;
    void load();
    const h = setInterval(() => alive && !document.hidden && void load(), 2000);
    return () => {
      alive = false;
      clearInterval(h);
    };
  }, [id, off]);
  const tabs: { id: Tab; label: string; icon: typeof Film; shown: boolean; alert?: boolean }[] = r
    ? [
        { id: 'summary', label: t('Overview'), icon: Activity, shown: true },
        { id: 'programs', label: t('Programs and EPG'), icon: Tv, shown: r.programs.length > 0 },
        { id: 'streams', label: t('Streams'), icon: Layers, shown: r.streams.length > 0 },
        { id: 'pictures', label: t('Pictures'), icon: Film, shown: !!r.frames },
        { id: 'transport', label: t('Transport (TS)'), icon: Network, shown: !!r.ts, alert: !!r.ts?.checks.some((c) => c.priority <= 2 && c.count > 0 && (c.agoS ?? 1e9) < 10) },
        { id: 'hls', label: 'HLS', icon: ListTree, shown: !!r.hls },
        { id: 'ndi', label: 'NDI', icon: Radio, shown: !!r.ndi },
        { id: 'sdi', label: 'SDI', icon: Radio, shown: !!r.sdi },
      ]
    : [];
  const shown = tabs.find((x) => x.id === tab && x.shown) ? tab : 'summary';
  return (
    <section className="ia">
      <header className="ia-head">
        <div>
          <small>{t('Signal contents')}</small>
          <h2>{name}</h2>
        </div>
        {r && (
          <span className="ia-src">
            <span className={`oc-dot ${r.live ? 'ok' : 'warn'}`} /> {r.kind} · <code>{r.source}</code>
          </span>
        )}
        <div className="spacer" />
        <button
          className="btn small"
          disabled={busy}
          title={t('Read the playlists again (HLS) and refresh')}
          onClick={async () => {
            setBusy(true);
            await load(true);
            setBusy(false);
          }}
        >
          <RefreshCw size={13} className={busy ? 'spin' : ''} /> {t('Refresh')}
        </button>
      </header>
      <div className="ia-layout">
        {/* The picture, always in sight on the left. */}
        <div className="ia-preview">{preview}</div>
        <div className="ia-main">
      {off && <p className="muted ia-off">{t('This input is off: switch it on to receive and analyse it.')}</p>}
      {!off && error && <p className="ia-error">{error}</p>}
      {!off && !r && !error && <p className="muted ia-off">{t('Analysing…')}</p>}
      {!off && r && (
        <>
          <nav className="ia-tabs" role="tablist">
            {tabs
              .filter((x) => x.shown)
              .map((x) => (
                <button key={x.id} role="tab" aria-selected={shown === x.id} className={shown === x.id ? 'on' : ''} onClick={() => setTab(x.id)}>
                  <x.icon size={13} /> {x.label}
                  {x.alert && <span className="ia-alert" />}
                </button>
              ))}
          </nav>
          <div className="ia-body">
            {shown === 'summary' && <Summary r={r} />}
            {shown === 'programs' && <Programs r={r} />}
            {shown === 'streams' && <Streams r={r} />}
            {shown === 'pictures' && r.frames && <Pictures f={r.frames} />}
            {shown === 'transport' && r.ts && <Transport ts={r.ts} />}
            {shown === 'hls' && r.hls && <HlsView h={r.hls} />}
            {shown === 'ndi' && r.ndi && <NdiView n={r.ndi} />}
            {shown === 'sdi' && r.sdi && <SdiView s={r.sdi} />}
          </div>
        </>
      )}
        </div>
      </div>
    </section>
  );
}

function Facts({ items }: { items: [string, React.ReactNode][] }) {
  const rows = items.filter(([, v]) => v !== '' && v != null && v !== false);
  if (!rows.length) return null;
  return (
    <dl className="ia-facts">
      {rows.map(([k, v]) => (
        <div key={k}>
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function Card({ title, icon: Icon, children, wide }: { title: string; icon?: typeof Film; children: React.ReactNode; wide?: boolean }) {
  return (
    <div className={`ia-card ${wide ? 'wide' : ''}`}>
      <h3>
        {Icon && <Icon size={13} />} {title}
      </h3>
      {children}
    </div>
  );
}

/** "1920×1080 · 25 i/s · progressif". */
function videoLine(s: Stream) {
  return [s.width ? `${s.width}×${s.height}` : '', fpsText(s.fps), s.scan, s.depth, s.hdr].filter(Boolean).join(' · ');
}

function Summary({ r }: { r: Report }) {
  const video = r.streams.filter((s) => s.kind === 'video');
  const audio = r.streams.filter((s) => s.kind === 'audio');
  const subs = r.streams.filter((s) => s.kind === 'subtitle');
  const data = r.streams.filter((s) => s.kind === 'data');
  const p1 = r.ts?.checks.filter((c) => c.priority === 1).reduce((n, c) => n + c.count, 0) ?? 0;
  const p2 = r.ts?.checks.filter((c) => c.priority === 2).reduce((n, c) => n + c.count, 0) ?? 0;
  const now = r.programs.find((p) => p.now)?.now;
  const tc = r.frames?.timecode || r.ndi?.timecode || r.sdi?.timecode;
  const captions = video.some((v) => v.captions) || r.frames?.captions || r.sdi?.anc.some((a) => a.did === 0x61);
  return (
    <>
      <div className="ia-tiles">
        <Tile label={t('Container')} value={r.container?.formatName || r.container?.format || r.kind} sub={r.ts ? kbps(r.ts.kbps) : r.container?.bitrateKbps ? kbps(r.container.bitrateKbps) : ''} />
        {video.map((v) => (
          <Tile key={v.index} label={t('Video')} value={`${v.codecName || v.codec}${v.profile ? ` ${v.profile}` : ''}`} sub={videoLine(v)} />
        ))}
        {r.ndi && <Tile label="NDI" value={`${r.ndi.width}×${r.ndi.height}`} sub={`${r.ndi.rate[1] ? +(r.ndi.rate[0] / r.ndi.rate[1]).toFixed(3) : '?'} i/s · ${r.ndi.format}`} />}
        {r.sdi && <Tile label="SDI" value={r.sdi.mode} sub={[r.sdi.signal, r.sdi.hdr].filter(Boolean).join(' · ')} />}
        <Tile label={t('Audio')} value={audio.length ? t('{n} tracks', { n: audio.length }) : r.ndi ? t('{n} channels', { n: r.ndi.audioChannels }) : r.sdi ? t('{n} channels', { n: r.sdi.audioChannels }) : t('None')} sub={audio.map((a) => `${a.codecName || a.codec}${a.language ? ` (${a.language})` : ''}`).join(', ')} />
        <Tile label={t('Subtitles and captions')} value={[subs.length ? t('{n} tracks', { n: subs.length }) : '', captions ? 'CC' : ''].filter(Boolean).join(' + ') || t('None')} sub={subs.map((s) => `${s.codecName || s.codec}${s.language ? ` (${s.language})` : ''}`).join(', ')} />
        <Tile label={t('Data')} value={data.length ? t('{n} streams', { n: data.length }) : t('None')} sub={data.map((d) => d.codecName || d.codec).join(', ')} />
        <Tile label="Timecode" value={tc || '—'} sub={r.frames?.timecodeKind || r.sdi?.timecodeKind || (r.ndi?.timecode ? 'NDI' : '')} mono />
        {r.frames?.gop != null && <Tile label="GOP" value={`${+r.frames.gop.toFixed(1)}`} sub={`I ${r.frames.i} · P ${r.frames.p} · B ${r.frames.b}`} />}
        {r.ts && <Tile label="TR 101 290" value={p1 + p2 === 0 ? 'OK' : `P1 ${p1} · P2 ${p2}`} sub={t('{n} PIDs', { n: r.ts.pids.length })} tone={p1 ? 'bad' : p2 ? 'warn' : 'ok'} />}
        {now && <Tile label={t('On air now (EPG)')} value={now.name} sub={`${clock(now.start)} · ${duration(now.durationS)}`} />}
        {r.ts && r.ts.splices.length > 0 && <Tile label="SCTE-35" value={t('{n} splices', { n: r.ts.splices.length })} sub={r.ts.splices[0].command} />}
        {r.hls && <Tile label="HLS" value={r.hls.master ? t('{n} variants', { n: r.hls.variants.length }) : t('Media playlist')} sub={r.hls.media ? `${r.hls.media.format} · ${r.hls.media.kind}` : ''} />}
      </div>
      {r.notes.length > 0 && (
        <ul className="ia-notes">
          {r.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
      <p className="ia-how">
        {t('Read from')}: {r.how.join(' · ') || t('nothing yet (no signal)')}
      </p>
    </>
  );
}

function Tile({ label, value, sub, mono, tone }: { label: string; value: string; sub?: string; mono?: boolean; tone?: 'ok' | 'warn' | 'bad' }) {
  return (
    <div className={`ia-tile ${tone ?? ''}`}>
      <small>{label}</small>
      <strong className={mono ? 'mono' : ''} title={value}>
        {value}
      </strong>
      {sub && <span title={sub}>{sub}</span>}
    </div>
  );
}

function EventView({ e, label, now }: { e: Event; label: string; now?: boolean }) {
  const start = e.start ? new Date(e.start).getTime() : 0;
  const pct = now && start && e.durationS ? Math.min(100, Math.max(0, ((Date.now() - start) / (e.durationS * 1000)) * 100)) : null;
  return (
    <div className={`ia-event ${now ? 'now' : ''}`}>
      <div className="ia-event-head">
        <small>{label}</small>
        <span className="mono">
          {clock(e.start)} · {duration(e.durationS)}
        </span>
        {e.genre && <span className="ia-chip">{e.genre}</span>}
        {!!e.rating && <span className="ia-chip warn">-{e.rating}</span>}
        {e.running && <span className="ia-chip">{e.running}</span>}
      </div>
      <strong>{e.name || t('(no title)')}</strong>
      {e.text && <p>{e.text}</p>}
      {pct != null && (
        <div className="ia-progress">
          <span style={{ width: `${pct}%` }} />
        </div>
      )}
    </div>
  );
}

function Programs({ r }: { r: Report }) {
  return (
    <div className="ia-grid">
      {r.programs.map((p) => (
        <Card key={p.number} title={`${t('Program')} ${p.number}${p.name ? ` · ${p.name}` : ''}`} icon={Tv} wide={r.programs.length === 1}>
          <Facts
            items={[
              [t('Name'), p.name],
              [t('Provider'), p.provider],
              [t('Type'), p.kind],
              [t('Access'), p.scrambled ? `${t('Scrambled')}${p.caSystems?.length ? ` · ${p.caSystems.join(', ')}` : ''}` : t('Free to air')],
              ...((p.metadata ?? []).map(([k, v]) => [k === 'variant_bitrate' ? t('Variant bit rate') : k, k === 'variant_bitrate' ? kbps(+v / 1000) : v]) as [string, string][]),
            ]}
          />
          {(p.now || p.next) && (
            <div className="ia-epg">
              {p.now && <EventView e={p.now} label={t('Now')} now />}
              {p.next && <EventView e={p.next} label={t('Next')} />}
            </div>
          )}
          {!p.now && !p.next && r.ts && <p className="muted small">{t('No EPG (EIT present / following) in this stream.')}</p>}
          <div className="ia-chips">
            {p.streams.map((i) => {
              const s = r.streams.find((x) => x.index === i);
              if (!s) return null;
              const Icon = KIND_ICON[s.kind] ?? Database;
              return (
                <span key={i} className="ia-chip">
                  <Icon size={11} /> {s.codecName || s.codec}
                  {s.language ? ` · ${s.language}` : ''}
                  {s.pid != null ? ` · ${hex(s.pid)}` : ''}
                </span>
              );
            })}
          </div>
        </Card>
      ))}
    </div>
  );
}

function Streams({ r }: { r: Report }) {
  return (
    <div className="ia-grid">
      {r.streams.map((s) => {
        const Icon = KIND_ICON[s.kind] ?? Database;
        const colours = [s.matrix, s.primaries, s.transfer].filter(Boolean);
        return (
          <Card key={s.index} title={`${s.pid != null ? `PID ${hex(s.pid)} (${s.pid})` : `#${s.index}`} · ${s.codecName || s.codec || s.kind}`} icon={Icon}>
            <Facts
              items={[
                [t('Codec'), [s.codec, s.profile].filter(Boolean).join(' · ')],
                [t('Picture'), s.width ? `${s.width}×${s.height}${s.dar ? ` (${s.dar})` : ''}${s.sar && s.sar !== '1:1' ? `, SAR ${s.sar}` : ''}` : ''],
                [t('Rate'), fpsText(s.fps)],
                [t('Scan'), s.scan ?? ''],
                [t('Sampling'), [s.pixFmt, s.depth].filter(Boolean).join(' · ')],
                [t('Range'), s.range ?? ''],
                [t('Colours'), colours.length ? colours.join(' / ') : ''],
                ['HDR', s.hdr ?? ''],
                ['CC (A/53)', s.captions ? t('Present') : ''],
                [t('Sampling rate'), s.sampleRate ? `${s.sampleRate} Hz` : ''],
                [t('Channels'), s.layout ?? ''],
                [t('Sample format'), s.sampleFmt ?? ''],
                [t('Language'), s.language ?? ''],
                [t('Title'), s.title ?? ''],
                [t('Bit rate'), s.bitrateKbps ? kbps(s.bitrateKbps) : ''],
                [t('Flags'), (s.dispositions ?? []).join(', ')],
              ]}
            />
            {!!s.details?.length && (
              <ul className="ia-list">
                {s.details.map((d) => (
                  <li key={d}>{d}</li>
                ))}
              </ul>
            )}
            {!!s.metadata?.length && <Facts items={s.metadata.map(([k, v]) => [k, v])} />}
          </Card>
        );
      })}
    </div>
  );
}

function Pictures({ f }: { f: Frames }) {
  const total = Math.max(1, f.i + f.p + f.b + f.other);
  return (
    <div className="ia-grid">
      <Card title={t('Picture types (last {n})', { n: f.window })} icon={Film}>
        <div className="ia-ipb">
          {f.i > 0 && <span className="i" style={{ flex: f.i }} title={`I ${f.i}`} />}
          {f.p > 0 && <span className="p" style={{ flex: f.p }} title={`P ${f.p}`} />}
          {f.b > 0 && <span className="b" style={{ flex: f.b }} title={`B ${f.b}`} />}
          {f.other > 0 && <span className="o" style={{ flex: f.other }} />}
        </div>
        <Facts
          items={[
            ['I', `${f.i} (${Math.round((f.i / total) * 100)} %)`],
            ['P', `${f.p} (${Math.round((f.p / total) * 100)} %)`],
            ['B', `${f.b} (${Math.round((f.b / total) * 100)} %)`],
            ['GOP', f.gop != null ? `${+f.gop.toFixed(1)}${f.gopMin !== f.gopMax ? ` (${f.gopMin}–${f.gopMax})` : ''} ${t('pictures')}` : ''],
            [t('Scan'), f.scan],
            [t('Decoded'), t('{n} pictures', { n: f.frames })],
          ]}
        />
      </Card>
      <Card title={t('Format as decoded')} icon={FileVideo}>
        <Facts
          items={[
            [t('Size'), f.size],
            ['SAR', f.sar],
            [t('Pixel format'), f.pixFmt],
            [t('Range'), f.range ?? ''],
            [t('Matrix'), f.matrix ?? ''],
            [t('Primaries'), f.primaries ?? ''],
            [t('Transfer'), f.transfer ?? ''],
            ['AFD', f.afd != null ? String(f.afd) : ''],
          ]}
        />
      </Card>
      <Card title="Timecode" icon={Clock}>
        <div className="ia-tc">{f.timecode || '--:--:--:--'}</div>
        <p className="muted small">{f.timecodeKind || t('No timecode in the pictures (SEI, GOP).')}</p>
      </Card>
      <Card title={t('Captions and HDR')} icon={Captions}>
        <Facts
          items={[
            ['CC A/53 (608/708)', f.captions ? t('Present') : t('Absent')],
            ['Mastering display', f.mastering ?? ''],
            ['MaxCLL / MaxFALL', f.light ?? ''],
          ]}
        />
      </Card>
      <Card title={t('Side data of the pictures')} icon={Layers} wide>
        {f.side.length ? (
          <table className="ia-table">
            <thead>
              <tr>
                <th>{t('Kind')}</th>
                <th>{t('Count')}</th>
                <th>{t('Last value')}</th>
                <th>{t('Seen')}</th>
              </tr>
            </thead>
            <tbody>
              {f.side.map((s) => (
                <tr key={s.kind}>
                  <td>{s.kind}</td>
                  <td className="num">{s.count}</td>
                  <td className="mono ellipsis" title={s.last}>
                    {s.last}
                  </td>
                  <td>{ago(s.agoS)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="muted small">{t('No side data (timecode, captions, HDR, AFD, SEI) in the pictures.')}</p>
        )}
      </Card>
    </div>
  );
}

function Transport({ ts }: { ts: Ts }) {
  const max = Math.max(1, ...ts.pids.map((p) => p.kbps));
  return (
    <div className="ia-grid">
      <Card title={ts.live ? t('Transport stream (continuous)') : t('Transport stream (last segment)')} icon={Network}>
        <Facts
          items={[
            [t('Bit rate'), kbps(ts.kbps)],
            [t('Packets'), ts.packets.toLocaleString(locale())],
            [ts.live ? t('Read for') : t('Duration'), `${ts.seconds.toFixed(1)} s`],
            ['TS id', ts.tsId != null ? `${ts.tsId} (${hex(ts.tsId)})` : ''],
            [t('Network'), [ts.network, ts.networkId != null ? `ONID ${ts.networkId}` : ''].filter(Boolean).join(' · ')],
            [t('Stream time (UTC)'), ts.utc ? new Date(ts.utc).toLocaleString(locale()) : ''],
            [t('Full EPG (schedule)'), ts.eitSchedule ? t('Present') : ''],
          ]}
        />
        <div className="ia-chips">
          {ts.tables.map((x) => (
            <span key={x} className="ia-chip">
              {x}
            </span>
          ))}
        </div>
      </Card>
      <Card title="TR 101 290" icon={ShieldAlert}>
        <table className="ia-table">
          <tbody>
            {ts.checks.map((c) => {
              const recent = c.count > 0 && (c.agoS ?? 1e9) < 10;
              return (
                <tr key={c.name}>
                  <td>
                    <span className={`oc-dot ${c.count === 0 ? 'ok' : recent ? (c.priority === 1 ? 'bad' : 'warn') : 'idle'}`} />
                  </td>
                  <td className="mono">{c.name}</td>
                  <td className="num">{c.count}</td>
                  <td className="muted">{c.count ? ago(c.agoS) : ''}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </Card>
      <Card title={t('PIDs')} icon={ListTree} wide>
        <table className="ia-table">
          <thead>
            <tr>
              <th>PID</th>
              <th>{t('Contents')}</th>
              <th>{t('Bit rate')}</th>
              <th />
              <th>CC</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {ts.pids.map((p) => (
              <tr key={p.pid}>
                <td className="mono">
                  {hex(p.pid)} <span className="muted">({p.pid})</span>
                </td>
                <td>{p.kind}</td>
                <td className="num">{kbps(p.kbps)}</td>
                <td className="ia-barcell">
                  <span className="ia-bar" style={{ width: `${(p.kbps / max) * 100}%` }} />
                </td>
                <td className={`num ${p.ccErrors ? 'tone-warn' : ''}`}>{p.ccErrors}</td>
                <td>
                  {p.pcr && <span className="ia-chip">PCR</span>}
                  {p.scrambled && <span className="ia-chip warn">{t('Scrambled')}</span>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
      <Card title={`SCTE-35${ts.spliceNulls ? ` · ${t('{n} heartbeats', { n: ts.spliceNulls })}` : ''}`} icon={Radio} wide>
        {ts.splices.length ? (
          <table className="ia-table">
            <thead>
              <tr>
                <th>{t('Seen')}</th>
                <th>{t('Command')}</th>
                <th>{t('Event')}</th>
                <th>{t('Direction')}</th>
                <th>{t('Duration')}</th>
                <th>PTS</th>
                <th>{t('Segmentation')}</th>
              </tr>
            </thead>
            <tbody>
              {ts.splices.map((s, i) => (
                <tr key={i}>
                  <td>{ts.live ? ago(s.atS) : ''}</td>
                  <td className="mono">{s.command}</td>
                  <td className="num">{s.cancel ? t('cancelled') : (s.eventId ?? '')}</td>
                  <td>{s.out == null ? '' : s.out ? t('Out (break)') : t('In (back)')}</td>
                  <td>{s.durationS != null ? `${s.durationS.toFixed(1)} s` : ''}</td>
                  <td className="mono">{s.immediate ? t('immediate') : s.ptsS != null ? s.ptsS.toFixed(3) : ''}</td>
                  <td>{(s.segmentation ?? []).join(' · ')}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="muted small">{t('No splice point (SCTE-35) seen.')}</p>
        )}
      </Card>
    </div>
  );
}

function HlsView({ h }: { h: Hls }) {
  const m = h.media;
  return (
    <div className="ia-grid">
      {h.error && <p className="ia-error">{h.error}</p>}
      <Card title={h.master ? t('Master playlist') : t('Media playlist')} icon={ListTree}>
        <Facts
          items={[
            [t('Address'), <code key="u">{h.url}</code>],
            [t('Version'), h.version ? String(h.version) : ''],
            [t('Independent segments'), h.independentSegments ? t('Yes') : ''],
            [t('Trick play (I-frames)'), h.iframeStreams ? String(h.iframeStreams) : ''],
          ]}
        />
        {h.session.length > 0 && (
          <ul className="ia-list">
            {h.session.map((x) => (
              <li key={x}>{x}</li>
            ))}
          </ul>
        )}
      </Card>
      {m && (
        <Card title={t('Media playlist (first variant)')} icon={FileVideo}>
          <Facts
            items={[
              [t('Kind'), m.kind],
              [t('Segments'), `${m.segments} × ${m.segmentMinS.toFixed(2)}–${m.segmentMaxS.toFixed(2)} s (${t('target')} ${m.targetDuration} s)`],
              [t('Window'), `${m.totalS.toFixed(1)} s`],
              [t('Format'), m.format],
              [t('Sequence'), `${m.mediaSequence}${m.discontinuitySequence ? ` · ${t('discontinuity')} ${m.discontinuitySequence}` : ''}`],
              ['Program date time', m.programDateTime ? new Date(m.programDateTime).toLocaleString(locale()) : ''],
              [t('Discontinuities'), m.discontinuities ? String(m.discontinuities) : ''],
              [t('Encryption'), m.encryption.join(', ')],
              ['Low-latency HLS', m.partTargetS ? `${t('parts of')} ${m.partTargetS} s` : ''],
              [t('Gaps'), m.gaps ? String(m.gaps) : ''],
              ['Byte ranges', m.byteRanges ? t('Yes') : ''],
            ]}
          />
          {m.cues.length > 0 && (
            <>
              <h4>{t('Ad markers')}</h4>
              <ul className="ia-list mono">
                {m.cues.map((c, i) => (
                  <li key={i}>{c}</li>
                ))}
              </ul>
            </>
          )}
        </Card>
      )}
      {h.variants.length > 0 && (
        <Card title={t('Variants (multi-bitrate)')} icon={Layers} wide>
          <table className="ia-table">
            <thead>
              <tr>
                <th>{t('Bit rate')}</th>
                <th>{t('Resolution')}</th>
                <th>{t('Rate')}</th>
                <th>Codecs</th>
                <th>{t('Range')}</th>
                <th>{t('Groups')}</th>
              </tr>
            </thead>
            <tbody>
              {h.variants.map((v, i) => (
                <tr key={i} title={v.uri}>
                  <td className="num">
                    {kbps(v.bandwidth / 1000)}
                    {v.averageBandwidth ? <span className="muted"> ({kbps(v.averageBandwidth / 1000)})</span> : null}
                  </td>
                  <td>{v.resolution}</td>
                  <td>{v.frameRate ? fpsText(v.frameRate) : ''}</td>
                  <td className="mono">{v.codecs}</td>
                  <td>{v.videoRange}</td>
                  <td>{[v.audio && `${t('audio')} ${v.audio}`, v.subtitles && `${t('subtitles')} ${v.subtitles}`, v.closedCaptions && `CC ${v.closedCaptions}`, v.hdcp && `HDCP ${v.hdcp}`].filter(Boolean).join(' · ')}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
      {h.renditions.length > 0 && (
        <Card title={t('Renditions (audio, subtitles, captions)')} icon={AudioLines} wide>
          <table className="ia-table">
            <thead>
              <tr>
                <th>{t('Type')}</th>
                <th>{t('Group')}</th>
                <th>{t('Name')}</th>
                <th>{t('Language')}</th>
                <th>{t('Channels')}</th>
                <th>{t('Flags')}</th>
              </tr>
            </thead>
            <tbody>
              {h.renditions.map((x, i) => (
                <tr key={i}>
                  <td>{x.kind}</td>
                  <td>{x.group}</td>
                  <td>{x.name}</td>
                  <td>{x.language}</td>
                  <td>{x.channels ?? x.instreamId}</td>
                  <td>{[x.default && 'DEFAULT', x.autoselect && 'AUTOSELECT', x.forced && 'FORCED', x.characteristics].filter(Boolean).join(' · ')}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  );
}

function NdiView({ n }: { n: Ndi }) {
  return (
    <div className="ia-grid">
      <Card title={t('NDI source')} icon={Radio}>
        <Facts
          items={[
            [t('Source'), n.source],
            [t('Picture'), `${n.width}×${n.height}${n.aspect ? ` (${n.aspect.toFixed(3)}:1)` : ''}`],
            [t('Rate'), n.rate[1] ? `${n.rate[0]}/${n.rate[1]} (${+(n.rate[0] / n.rate[1]).toFixed(3)} i/s)` : ''],
            [t('Scan'), n.format],
            ['Timecode', n.timecode ?? ''],
            [t('Audio'), n.audioChannels ? `${n.audioChannels} ${t('channels')} · ${n.audioRate} Hz · ${n.audioSamples} ${t('samples per frame')}` : ''],
            [t('Frames received'), `${n.videoFrames} ${t('video')} · ${n.audioFrames} ${t('audio')}`],
          ]}
        />
      </Card>
      {(n.metadata || n.frameMetadata) && (
        <Card title={t('Metadata (XML)')} icon={Database} wide>
          {n.metadata && <pre className="ia-pre">{n.metadata}</pre>}
          {n.frameMetadata && <pre className="ia-pre">{n.frameMetadata}</pre>}
        </Card>
      )}
    </div>
  );
}

function SdiView({ s }: { s: Sdi }) {
  return (
    <div className="ia-grid">
      <Card title={t('SDI signal')} icon={Radio}>
        <Facts
          items={[
            [t('Card'), `DeckLink ${s.device + 1}`],
            [t('Format'), s.mode],
            [t('Signal'), s.signal ?? ''],
            [t('Scan'), s.field ?? ''],
            [t('Colours'), s.colorspace ?? ''],
            ['HDR', s.hdr ?? ''],
            ['Timecode', s.timecode ? `${s.timecode} (${s.timecodeKind})` : ''],
            [t('Embedded audio'), t('{n} channels', { n: s.audioChannels })],
          ]}
        />
      </Card>
      <Card title={t('Ancillary data (ANC / VANC)')} icon={Database} wide>
        {s.anc.length ? (
          <table className="ia-table">
            <thead>
              <tr>
                <th>DID</th>
                <th>SDID</th>
                <th>{t('Line')}</th>
                <th>{t('Contents')}</th>
                <th>{t('Packets')}</th>
              </tr>
            </thead>
            <tbody>
              {s.anc.map((a) => (
                <tr key={`${a.did}-${a.sdid}-${a.line}`}>
                  <td className="mono">{hex(a.did, 2)}</td>
                  <td className="mono">{hex(a.sdid, 2)}</td>
                  <td className="num">{a.line}</td>
                  <td>{a.name || t('Unknown')}</td>
                  <td className="num">{a.count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="muted small">{t('No ancillary packet seen (or the card does not hand them over).')}</p>
        )}
      </Card>
    </div>
  );
}

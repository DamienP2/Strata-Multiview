import { Cpu, Plus, Search, Trash2, Video, VideoOff } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api } from '../store';
import { t, tc, tn } from '../i18n';
import { InfoTip } from '../ui/InfoTip';
import { inputMessage as messageText, useLiveFrame } from '../engine/liveInputs';
import type { InputConfig, InputInfo, InputKind, ServerConfig } from './types';
import { useMetrics, type InputMetrics } from '../engine/metrics';
import { corePct, fps as fpsText, fpsTone, inputTone, num } from '../perf/format';
import '../perf/perf.css';
import { deviceOf, hardwareOf } from '../hardware/model';
import { navigate } from '../router';
import { OnSwitch } from './OnSwitch';
import { NdiPicker } from './NdiPicker';

/*
 * The INPUTS mode of the Video streams page: live video coming into the
 * server (SRT, UDP, RTMP, HLS, RTSP, any address ffmpeg reads, NDI, SDI),
 * shown by the Video widget. Saved with the rest of the settings (the save
 * bar of the page); the server restarts only the inputs that changed.
 */

const COLOR = '#a78bfa';

/** Labels translated where shown. */
const KINDS: { value: InputKind; label: string; badge: string }[] = [
  { value: 'srt', label: 'SRT', badge: 'SRT' },
  { value: 'udp', label: 'UDP', badge: 'UDP' },
  { value: 'rtmp', label: 'RTMP', badge: 'RTMP' },
  { value: 'hls', label: 'HLS', badge: 'HLS' },
  { value: 'rtsp', label: 'RTSP', badge: 'RTSP' },
  { value: 'url', label: 'Address / file', badge: 'URL' },
  { value: 'ndi', label: 'NDI', badge: 'NDI' },
  { value: 'sdi', label: 'SDI', badge: 'SDI' },
  { value: 'st2110', label: 'ST 2110', badge: 'ST 2110' },
];
const badgeOf = (k: InputKind) => KINDS.find((x) => x.value === k)?.badge ?? k.toUpperCase();
const SCHEMES: Partial<Record<InputKind, RegExp>> = { srt: /^srt:/i, udp: /^udp:/i, rtmp: /^rtmps?:/i, hls: /^https?:/i, rtsp: /^rtsps?:/i };
const isStream = (k: InputKind) => k !== 'ndi' && k !== 'sdi' && k !== 'st2110';
/** Inputs that receive something Hardware declares. */
const needsHardware = (k: InputKind) => k === 'sdi' || k === 'st2110';
/** An SRT listener waits on a port; the others have an address. */
const byPort = (c: Pick<InputConfig, 'kind' | 'listen'>) => c.kind === 'srt' && !!c.listen;
const portOf = (c: Pick<InputConfig, 'url' | 'port'>) => c.port || +(/:(\d+)(?:[/?]|$)/.exec(c.url)?.[1] ?? 0) || 10000;
/** SRT latencies offered, ms. */
const LATENCIES = [20, 40, 80, 120, 200, 300, 500, 1000, 2000];
const canListen = (k: InputKind) => k === 'srt' || k === 'rtmp';

/** SRT: Strata calls the sender, or waits for it; RTMP: Strata pulls, or the encoder pushes. Labels translated where shown. */
const MODES: Partial<Record<InputKind, { listen: boolean; label: string; hint: string }[]>> = {
  srt: [
    { listen: false, label: 'Caller', hint: 'Strata connects to the sender, which must be listening.' },
    { listen: true, label: 'Listener', hint: 'Strata waits for the sender to connect.' },
  ],
  rtmp: [
    { listen: false, label: 'Pull', hint: 'Strata pulls the stream from a server.' },
    { listen: true, label: 'Listen', hint: 'The encoder pushes the stream to Strata.' },
  ],
};
const modeOf = (c: Pick<InputConfig, 'kind' | 'listen'>) => MODES[c.kind]?.find((m) => m.listen === !!c.listen);
/** "SRT · Listener". */
const badgeText = (c: Pick<InputConfig, 'kind' | 'listen'>) => {
  const m = modeOf(c);
  return m ? `${badgeOf(c.kind)} · ${tc('short', m.label)}` : badgeOf(c.kind);
};

/** Jitter buffer, in frames. */
const BUFFERS = [0, 1, 2, 3, 5];


function placeholder(c: Pick<InputConfig, 'kind' | 'listen'>): string {
  switch (c.kind) {
    case 'srt':
      return c.listen ? 'srt://0.0.0.0:9000' : 'srt://host:9000';
    case 'udp':
      return 'udp://@239.1.1.1:5000';
    case 'rtmp':
      return c.listen ? 'rtmp://0.0.0.0:1935/live/cam1' : 'rtmp://server/app/key';
    case 'hls':
      return 'https://…/index.m3u8';
    case 'rtsp':
      return 'rtsp://camera/stream';
    default:
      return 'https://… · /path/to/clip.mp4';
  }
}

/** The address a mode starts with (SRT keeps the port). */
function modeDefault(c: Pick<InputConfig, 'kind' | 'listen' | 'url'>): string {
  const port = /:(\d+)(?:[/?]|$)/.exec(c.url)?.[1] ?? '9000';
  if (c.kind === 'srt') return c.listen ? `srt://0.0.0.0:${port}` : `srt://host:${port}`;
  return placeholder(c);
}

function addressHint(c: InputConfig): string {
  const host = location.hostname;
  switch (c.kind) {
    case 'srt':
      return t("The sender's address, e.g. srt://host:9000 (it must be listening).");
    case 'udp':
      return t('udp://@239.1.1.1:5000 to join a multicast group, or udp://0.0.0.0:5000 for packets sent to this server.');
    case 'rtmp':
      return c.listen ? t('Senders push to rtmp://{host}:1935/live/cam1: set the address to rtmp://0.0.0.0:1935/live/cam1.', { host }) : t('The stream to pull, e.g. rtmp://server/app/key.');
    case 'hls':
      return t('The playlist, e.g. https://…/index.m3u8.');
    case 'rtsp':
      return t("The camera's stream, e.g. rtsp://camera/stream.");
    default:
      return t('Anything ffmpeg reads: an address (http, rtp, srt…) or a file on the server.');
  }
}


/** Waiting is not failing. */
const WAITING = /^(Starting…|Waiting for the sender…|Connecting…|Connecting to .*…|Not started.*|Waiting for a signal .*…|Off)$/;

type Tone = 'ok' | 'warn' | 'idle';

export function InputsMode({ cfg, saved, edit }: { cfg: ServerConfig; saved: ServerConfig; edit: (fn: (c: ServerConfig) => void) => void }) {
  const inputs = cfg.inputs ?? [];
  const [sel, setSel] = useState<string | null>(null);
  const [status, setStatus] = useState<InputInfo[]>([]);
  const metrics = useMetrics();
  const rates = (id: string) => metrics?.inputs.find((x) => x.id === id);

  useEffect(() => {
    let alive = true;
    const poll = () =>
      api<InputInfo[]>('GET', '/api/inputs')
        .then((s) => alive && setStatus(s))
        .catch(() => {});
    poll();
    const h = setInterval(() => document.hidden || void poll(), 2000);
    return () => {
      alive = false;
      clearInterval(h);
    };
  }, []);

  const current = inputs.find((i) => i.id === sel) ?? inputs[0];
  const savedOf = (id: string) => (saved.inputs ?? []).find((i) => i.id === id);
  const stateOf = (c: InputConfig): { tone: Tone; text: string; live: boolean; info?: InputInfo } => {
    if (c.enabled === false) return { tone: 'idle', text: tc('output', 'Off'), live: false };
    const s = savedOf(c.id);
    const info = status.find((x) => x.id === c.id);
    if (info?.message === 'Off') return { tone: 'idle', text: s && s.enabled !== false ? t('Not applied yet') : tc('output', 'Off'), live: false };
    if (!s || !info) return { tone: 'idle', text: s ? t('Starting…') : t('Not applied yet'), live: false };
    if (info.live) return { tone: 'ok', text: messageText(info.message) || t('Receiving'), live: true, info };
    return { tone: WAITING.test(info.message) ? 'idle' : 'warn', text: messageText(info.message), live: false, info };
  };

  const editInput = (fn: (i: InputConfig) => void, id = current?.id) =>
    edit((c) => {
      const i = (c.inputs ?? []).find((x) => x.id === id);
      if (i) fn(i);
    });
  const setOn = (id: string, on: boolean) => editInput((i) => void (i.enabled = on), id);
  const hw = hardwareOf(cfg);
  const inFlows = hw.st2110.flows.filter((f) => f.direction === 'input');
  /** What Hardware declares for an input kind ([] for the other kinds). */
  const hwChoices = (k: InputKind) => (k === 'sdi' ? hw.sdi.inputs.map((d) => ({ id: d.id, name: d.name })) : k === 'st2110' ? inFlows.map((f) => ({ id: f.id, name: f.name })) : []);

  const add = () => {
    const ids = new Set(inputs.map((i) => i.id));
    let n = 1;
    while (ids.has(`input-${n}`)) n++;
    const id = `input-${n}`;
    // An SRT listener on a port nothing else uses.
    const urls = [...inputs.map((i) => i.url), ...cfg.channels.flatMap((ch) => [ch.outputs, ...(ch.renditions ?? []).map((r) => r.outputs)].flatMap((o) => (o.streams ?? []).map((s) => s.url)))];
    let port = 10000;
    while (urls.some((u) => new RegExp(`:${port}(?:[/?]|$)`).test(u))) port++;
    for (const i of inputs) if (byPort(i)) urls.push(`:${portOf(i)}`);
    while (urls.some((u) => new RegExp(`:${port}(?:[/?]|$)`).test(u))) port++;
    edit((c) => void (c.inputs ??= []).push({ id, name: t('Input {n}', { n }), kind: 'srt', url: '', port, latency: 120, listen: true, ndiSource: '', device: 0, width: 1920, height: 1080, fps: 25, buffer: 2, hwDecode: false }));
    setSel(id);
  };

  const remove = () => {
    if (!current) return;
    const at = inputs.indexOf(current);
    edit((c) => void (c.inputs = (c.inputs ?? []).filter((i) => i.id !== current.id)));
    setSel(inputs[at + 1]?.id ?? inputs[at - 1]?.id ?? null);
  };

  const setKind = (k: InputKind) =>
    editInput((i) => {
      if (i.kind === k) return;
      i.kind = k;
      if (!canListen(k)) i.listen = false;
      // An address of another protocol goes.
      const scheme = SCHEMES[k];
      if (scheme && !scheme.test(i.url.trim())) i.url = i.listen ? placeholder(i) : '';
      if (byPort(i)) {
        i.port = portOf(i);
        i.url = '';
      }
      // SDI and ST 2110: the first input Hardware declares.
      if (needsHardware(k)) {
        i.hardware = hwChoices(k)[0]?.id ?? '';
        const d = hw.sdi.inputs.find((x) => x.id === i.hardware);
        const card = d && hw.sdi.cards.find((c) => c.id === d.card);
        if (k === 'sdi' && d && card) i.device = deviceOf(card, d.connector);
      } else delete i.hardware;
    });

  const liveCount = inputs.filter((i) => stateOf(i).live).length;

  return (
    <div className="outputs-page">
      <aside className="lib-nav">
        <div className="lib-nav-section">
          <span className="grow">
            {t('Inputs')}
            <InfoTip text={t('Cameras, encoders, NDI or SDI sources coming into the server. The Video widget shows them in the graphics.')} />
          </span>
          <button className="icon-btn ghost" title={t('Add an input')} onClick={add}>
            <Plus size={14} />
          </button>
        </div>
        {inputs.map((i) => {
          const st = stateOf(i);
          return (
            <div key={i.id} role="button" tabIndex={0} className={`oc-channel vs-item ${i.id === current?.id ? 'on' : ''} ${i.enabled === false ? 'off' : ''}`} onClick={() => setSel(i.id)} onKeyDown={(e) => e.key === 'Enter' && setSel(i.id)}>
              <OnSwitch on={i.enabled !== false} mini onChange={(v) => setOn(i.id, v)} />
              <span className={`oc-dot ${st.tone}`} />
              <span className="grow">
                <span>{i.name || i.id}</span>
                <small title={st.text}>{st.text}</small>
              </span>
              <Rate m={rates(i.id)} />
              <span className="vs-badge" style={{ ['--k' as string]: COLOR }}>
                {badgeText(i)}
              </span>
            </div>
          );
        })}
        <button className="oc-channel vs-item add" onClick={add}>
          <Plus size={13} /> {t('Add an input')}
        </button>
      </aside>

      <main className="oc-main">
        <header className="oc-head">
          <div>
            <h1>{t('Inputs')}</h1>
            <span className="muted small">
              {tn(inputs.length, '{n} input', '{n} inputs')} · {t('{n} receiving', { n: liveCount })}
            </span>
          </div>
          <div className="spacer" />
          <div className="oc-tools">
            <button className="oc-tool" onClick={add}>
              <Video size={15} color={COLOR} /> {t('Add an input')}
            </button>
          </div>
        </header>

        <div className="oc-body">
          <div className="vs-grid-wrap">
            {inputs.length === 0 ? (
              <div className="vs-empty">
                <VideoOff size={28} />
                <p>{t('No input yet. Add an SRT, RTMP or NDI source, a camera… and show it with the Video widget.')}</p>
                <button className="btn primary" onClick={add}>
                  <Plus size={14} /> {t('Add an input')}
                </button>
              </div>
            ) : (
              <div className="vs-grid">
                {inputs.map((i) => {
                  const st = stateOf(i);
                  return (
                    <div key={i.id} role="button" tabIndex={0} className={`vs-tile ${i.id === current?.id ? 'sel' : ''} ${i.enabled === false ? 'off' : ''}`} onClick={() => setSel(i.id)} onKeyDown={(e) => e.key === 'Enter' && setSel(i.id)}>
                      <Preview id={i.id} live={st.live} />
                      <span className="vs-tile-head">
                        <OnSwitch on={i.enabled !== false} mini onChange={(v) => setOn(i.id, v)} />
                        <strong>{i.name || i.id}</strong>
                        <span className="vs-badge" style={{ ['--k' as string]: COLOR }}>
                          {badgeText(i)}
                        </span>
                      </span>
                      <span className="vs-tile-status" title={st.text}>
                        <span className={`oc-dot ${st.tone}`} />
                        <span className="grow">{st.text}</span>
                        <Rate m={rates(i.id)} full />
                        <span className="vs-res">{detected(st.info) || '—'}</span>
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          <aside className="oc-inspector">
            {current ? (
              <Inspector
                input={current}
                state={stateOf(current)}
                rates={rates(current.id)}
                editInput={editInput}
                setOn={(v) => setOn(current.id, v)}
                hwChoices={hwChoices}
                setKind={setKind}
                remove={remove}
              />
            ) : (
              <p className="oc-hint">{t('Add an input to set it up here.')}</p>
            )}
          </aside>
        </div>
      </main>
    </div>
  );
}

function Inspector({
  input: i,
  state,
  rates,
  editInput,
  setOn,
  hwChoices,
  setKind,
  remove,
}: {
  input: InputConfig;
  state: { tone: Tone; text: string; live: boolean; info?: InputInfo };
  rates?: InputMetrics;
  editInput: (fn: (i: InputConfig) => void) => void;
  setOn: (on: boolean) => void;
  hwChoices: (k: InputKind) => { id: string; name: string }[];
  setKind: (k: InputKind) => void;
  remove: () => void;
}) {
  const listId = `vs-ndi-${i.id}`;
  const [picking, setPicking] = useState(false);
  const choices = hwChoices(i.kind);
  const port = portOf(i);
  return (
    <>
      <div className="oc-ins-head" style={{ ['--k' as string]: COLOR }}>
        <span className="oc-swatch" />
        <div className="grow">
          <small>{t('Input')}</small>
          <strong>{i.name || i.id}</strong>
        </div>
        <OnSwitch on={i.enabled !== false} onChange={setOn} label={i.enabled !== false ? tc('output', 'On') : tc('output', 'Off')} />
        <button className="icon-btn ghost" title={t('Remove')} onClick={remove}>
          <Trash2 size={15} />
        </button>
      </div>

      <div className={`oc-state ${state.tone}`}>
        <span className={`oc-dot ${state.tone}`} />
        <span className="grow">{state.text}</span>
        {!!state.info?.frames && <span className="vs-res">{tn(state.info.frames, '{n} frame', '{n} frames')}</span>}
      </div>
      {state.live && detected(state.info) && (
        <div className="vs-detected">
          <small>{t('Detected')}</small>
          <strong>{detected(state.info)}</strong>
        </div>
      )}
      {rates && (
        <div className="vs-perf">
          <div>
            <small>{t('Received')}</small>
            <strong className={`tone-${rates.live ? fpsTone(rates.fps, rates.nominal) : 'idle'}`}>
              {fpsText(rates.fps)} / {fpsText(rates.nominal)} fps
            </strong>
          </div>
          <div>
            <small>{t('Dropped (10 s)')}</small>
            <strong className={rates.dropped ? 'tone-warn' : ''}>{num(rates.dropped)}</strong>
          </div>
          <div>
            <small>CPU</small>
            <strong>{corePct(rates.cpu)}</strong>
          </div>
        </div>
      )}

      <Field label={t('Name')} hint={t('How the Video widget lists it.')}>
        <input value={i.name} onChange={(e) => editInput((x) => void (x.name = e.target.value))} />
      </Field>
      <Field label={t('Type')}>
        <select value={i.kind} onChange={(e) => setKind(e.target.value as InputKind)}>
          {KINDS.map((k) => {
            const missing = needsHardware(k.value) && !hwChoices(k.value).length && k.value !== i.kind;
            return (
              <option key={k.value} value={k.value} disabled={missing}>
                {t(k.label)}
                {missing ? ` — ${k.value === 'sdi' ? t('Define SDI inputs in Hardware') : t('Define ST 2110 input flows in Hardware')}` : ''}
              </option>
            );
          })}
        </select>
      </Field>

      {isStream(i.kind) && (
        <>
          {canListen(i.kind) && (
            <Group label={t('Mode')} hint={t(modeOf(i)?.hint ?? '')}>
              <div className="seg vs-seg">
                {MODES[i.kind]!.map((m) => (
                  <button
                    key={m.label}
                    type="button"
                    className={!!i.listen === m.listen ? 'on' : ''}
                    onClick={() =>
                      editInput((x) => {
                        if (!!x.listen === m.listen) return;
                        // An address still the default of the other mode follows.
                        const was = !x.url.trim() || x.url === placeholder(x) || x.url === modeDefault(x);
                        const port = portOf(x);
                        x.listen = m.listen;
                        if (byPort(x)) {
                          // An SRT listener: a port, no address.
                          x.port = port;
                          x.url = '';
                        } else if (x.kind === 'srt' && was) x.url = `srt://host:${port}`;
                        else if (was) x.url = modeDefault(x);
                      })
                    }
                  >
                    {t(m.label)}
                  </button>
                ))}
              </div>
            </Group>
          )}
          {byPort(i) ? (
            <Field label={t('Port')} hint={t('The sender calls srt://{host}:{port}.', { host: location.hostname, port })}>
              <input type="number" min={1} max={65535} value={port} onChange={(e) => editInput((x) => void ((x.port = Math.max(1, Math.min(65535, Math.round(+e.target.value) || 0))), (x.url = '')))} />
            </Field>
          ) : (
            <Field label={t('Address')} hint={addressHint(i)}>
              <input value={i.url} placeholder={placeholder(i)} spellCheck={false} onChange={(e) => editInput((x) => void (x.url = e.target.value))} />
            </Field>
          )}
          {i.kind === 'srt' && (
            <Field label={t('Latency')} hint={t('SRT latency: the time allowed to recover lost packets (about 3–4× the network round trip).')}>
              <select value={i.latency ?? 120} onChange={(e) => editInput((x) => void (x.latency = +e.target.value))}>
                {!LATENCIES.includes(i.latency ?? 120) && <option value={i.latency}>{i.latency} ms</option>}
                {LATENCIES.map((ms) => (
                  <option key={ms} value={ms}>
                    {ms} ms
                  </option>
                ))}
              </select>
            </Field>
          )}
        </>
      )}

      {i.kind === 'ndi' && (
        <div className="set-field">
          <label className="set-label" htmlFor={`${listId}-in`}>
            {t('NDI source')}
          </label>
          <div className="vs-row">
            <input id={`${listId}-in`} value={i.ndiSource} placeholder={t('HOST (name)')} spellCheck={false} onChange={(e) => editInput((x) => void (x.ndiSource = e.target.value))} />
            <button className="btn small" onClick={() => setPicking(true)}>
              <Search size={13} /> {t('Browse…')}
            </button>
          </div>
          {picking && (
            <NdiPicker
              current={i.ndiSource}
              onClose={() => setPicking(false)}
              onChoose={(s) => {
                editInput((x) => void (x.ndiSource = s));
                setPicking(false);
              }}
            />
          )}
        </div>
      )}

      {needsHardware(i.kind) && (
        <>
          <Field label={i.kind === 'sdi' ? t('SDI input') : t('ST 2110 flow')} hint={i.kind === 'sdi' ? t('The connectors declared in Hardware.') : t('The multicast addresses are set in Hardware.')}>
            <select value={i.hardware ?? ''} onChange={(e) => editInput((x) => void (x.hardware = e.target.value))}>
              {!choices.some((c) => c.id === i.hardware) && <option value="">{t('— choose —')}</option>}
              {choices.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </Field>
          <button type="button" className="btn small oc-hw-edit" onClick={() => navigate(i.kind === 'st2110' ? '/hardware#st2110' : '/hardware')}>
            <Cpu size={13} /> {t('Edit in Hardware')}
          </button>
          <Hint>{i.kind === 'sdi' ? t('SDI inputs need a server built with DeckLink support.') : t('Receiving ST 2110 is not available yet.')}</Hint>
        </>
      )}

      <Field label={t('Jitter buffer')} hint={t('Absorbs network jitter: each frame adds one frame of delay.')}>
        <select value={i.buffer ?? 2} onChange={(e) => editInput((x) => void (x.buffer = +e.target.value))}>
          {!BUFFERS.includes(i.buffer ?? 2) && <option value={i.buffer}>{tn(i.buffer, '{n} frame', '{n} frames')}</option>}
          {BUFFERS.map((b) => (
            <option key={b} value={b}>
              {b === 0 ? t('0 (lowest latency)') : tn(b, '{n} frame', '{n} frames')}
              {b > 0 && isStream(i.kind) && (state.info?.fps ?? 0) > 0 ? ` · ${Math.round((b * 1000) / state.info!.fps!)} ms` : ''}
            </option>
          ))}
        </select>
      </Field>

      {isStream(i.kind) && (
        <Group label={t('Decoding: GPU / CPU')} hint={i.hwDecode ? t('Frees the processor. When the machine has no GPU decoder for this stream, ffmpeg falls back to the processor.') : t('Decoded by the processor.')}>
          <div className="seg vs-seg">
            {[true, false].map((gpu) => (
              <button key={String(gpu)} type="button" className={!!i.hwDecode === gpu ? 'on' : ''} onClick={() => editInput((x) => void (x.hwDecode = gpu))}>
                {gpu ? 'GPU' : 'CPU'}
              </button>
            ))}
          </div>
        </Group>
      )}

      <button className="btn small danger" onClick={remove}>
        <Trash2 size={13} /> {t('Remove the input')}
      </button>
    </>
  );
}

/** "1920×1080 · 25 fps", as the server detects it. */
const detected = (info?: InputInfo) => (info?.width ? `${info.width}×${info.height}${info.fps ? ` · ${fpsText(info.fps)} fps` : ''}` : '');

/** The newest picture, five times a second; the last one stays, dimmed, when the signal goes. */
function Preview({ id, live }: { id: string; live: boolean }) {
  const pic = useLiveFrame(id, 'tile');
  return (
    <span className={`vs-preview ${live ? '' : 'stale'}`}>
      {pic.url && <img src={pic.url} alt="" draggable={false} />}
      {(!live || !pic.url) && (
        <span className="vs-nosignal">
          <VideoOff size={20} />
          {live ? t('Waiting for a picture…') : t('No signal')}
        </span>
      )}
    </span>
  );
}

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

function Hint({ children }: { children: React.ReactNode }) {
  return <p className="oc-hint">{children}</p>;
}

/** Pictures received per second against the decoded rate, and those thrown away. */
function Rate({ m, full }: { m?: InputMetrics; full?: boolean }) {
  if (!m || !m.live) return null;
  const tone = inputTone(m);
  const title = m.dropped ? `${t('{fps} of {nominal} fps received', { fps: fpsText(m.fps), nominal: fpsText(m.nominal) })} · ${tn(m.dropped, '{n} picture thrown away (10 s)', '{n} pictures thrown away (10 s)')}` : t('{fps} of {nominal} fps received', { fps: fpsText(m.fps), nominal: fpsText(m.nominal) });
  return (
    <span className={`vs-fps tone-${tone}`} title={title}>
      {fpsText(m.fps)}/{fpsText(m.nominal)}
      {full && ' fps'}
      {full && m.dropped > 0 && ` · −${num(m.dropped)}`}
    </span>
  );
}

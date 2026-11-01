// A channel's test generator (Broadcast › Output streams, the channel): colour bars, test patterns,
// a still picture, a lip sync test, with or without sound, sent by every destination of the channel
// instead of its multiview. Applied at once (PUT /api/channels/{id}/generator), the outputs going
// on; drawn by the server (crates/strata-mv-render/src/generator.rs).
import { useEffect, useRef, useState } from 'react';
import { AssetLibrary } from '../designer/AssetPicker';
import type { GeneratorConfig } from '../engine/types';
import { t } from '../i18n';
import { useInputs } from '../engine/liveInputs';
import { api, useApp } from '../store';
import { HexColorField } from '../ui/fields';
import { InfoTip } from '../ui/InfoTip';
import { friendlyError } from '../ui/serverError';
import { OnSwitch } from './OnSwitch';

const DEFAULT: GeneratorConfig = { enabled: false, pattern: 'smpteHd', color: '#000000', image: '', ident: '', clock: false, motion: false, tone: 'tone', level: -18, frequency: 1000 };

const PATTERNS: [GeneratorConfig['pattern'], string][] = [
  ['smpteHd', 'SMPTE HD colour bars'],
  ['ebu75', 'EBU 75 % colour bars'],
  ['bars100', '100 % colour bars'],
  ['greyScale', 'Grey scale and ramp'],
  ['pluge', 'PLUGE (black level)'],
  ['grid', 'Grid and safe areas'],
  ['zonePlate', 'Zone plate'],
  ['lipsync', 'Lip sync (flash and beep)'],
  ['solid', 'Plain colour'],
  ['image', 'Still picture'],
];

const TONES: [GeneratorConfig['tone'], string][] = [
  ['tone', 'Tone'],
  ['ebuIdent', 'EBU stereo ident (left cut every 3 s)'],
  ['beep', 'A beep each second (with the lip sync flash)'],
  ['silence', 'Silence'],
  ['none', 'No sound'],
];

export function GeneratorPanel({ channelId }: { channelId: string }) {
  const toast = useApp((s) => s.toast);
  const live = useApp((s) => s.channels.find((c) => c.id === channelId)?.generator);
  const [g, setG] = useState<GeneratorConfig>(live ?? DEFAULT);
  const [picking, setPicking] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  // The server's when it changes elsewhere (another page, the API).
  useEffect(() => {
    if (live) setG(live);
  }, [live]);

  const send = (next: GeneratorConfig) => api('PUT', `/api/channels/${encodeURIComponent(channelId)}/generator`, next).catch((e) => toast(friendlyError((e as Error).message), 'error'));
  /** Applied at once; words and numbers a moment after the last key. */
  const set = (patch: Partial<GeneratorConfig>, typing = false) => {
    const next = { ...g, ...patch };
    setG(next);
    clearTimeout(timer.current);
    if (typing) timer.current = setTimeout(() => void send(next), 500);
    else void send(next);
  };

  return (
    <div className={`gen ${g.enabled ? 'on' : ''}`}>
      <div className="gen-head">
        <span className="set-label">
          {t('Test generator')}
          <InfoTip text={t('Sent at once by every destination of the channel instead of its multiview (SDI and NDI with its sound).')} />
        </span>
        <OnSwitch on={g.enabled} onChange={(on) => set({ enabled: on })} label={g.enabled ? t('On air') : t('Off')} />
      </div>
      <img className="gen-preview" src={`/api/channels/${encodeURIComponent(channelId)}/generator.mjpeg`} alt="" />
      <label className="set-field">
        <span className="set-label">{t('Pattern')}</span>
        <select value={g.pattern} onChange={(e) => set({ pattern: e.target.value as GeneratorConfig['pattern'] })}>
          {PATTERNS.map(([k, l]) => (
            <option key={k} value={k}>
              {t(l)}
            </option>
          ))}
        </select>
      </label>
      {g.pattern === 'solid' && (
        <label className="set-field">
          <span className="set-label">{t('Colour')}</span>
          <HexColorField value={g.color} fallback="#000000" onChange={(color) => set({ color })} />
        </label>
      )}
      {g.pattern === 'image' && (
        <div className="set-field">
          <span className="set-label">{t('Picture')}</span>
          <span className="row">
            <span className="gen-file" title={g.image}>
              {g.image ? g.image.split('/').pop() : <span className="muted">{t('None: black')}</span>}
            </span>
            <button type="button" className="btn small" onClick={() => setPicking(true)}>
              {g.image ? t('Change…') : t('Choose…')}
            </button>
          </span>
        </div>
      )}
      <label className="set-field">
        <span className="set-label">{t('Ident')}</span>
        <input value={g.ident} placeholder={t('Words over the picture (channel, place…)')} onChange={(e) => set({ ident: e.target.value }, true)} />
      </label>
      <div className="gen-checks">
        <label>
          <input type="checkbox" checked={g.clock} onChange={(e) => set({ clock: e.target.checked })} /> {t('Time and frame burnt in')}
        </label>
        <label>
          <input type="checkbox" checked={g.motion} onChange={(e) => set({ motion: e.target.checked })} /> {t('A moving box (live, not frozen)')}
        </label>
      </div>
      <label className="set-field">
        <span className="set-label">{t('Sound')}</span>
        <select value={g.tone} onChange={(e) => set({ tone: e.target.value as GeneratorConfig['tone'] })}>
          {TONES.map(([k, l]) => (
            <option key={k} value={k}>
              {t(l)}
            </option>
          ))}
        </select>
      </label>
      {g.tone !== 'none' && g.tone !== 'silence' && (
        <div className="gen-row">
          <label className="set-field">
            <span className="set-label">{t('Level')}</span>
            <select value={g.level} onChange={(e) => set({ level: +e.target.value })}>
              {[-12, -18, -20, -24].map((l) => (
                <option key={l} value={l}>
                  {`${l} dBFS`}
                  {l === -18 ? ' · EBU R68' : l === -20 ? ' · SMPTE RP 155' : ''}
                </option>
              ))}
            </select>
          </label>
          <label className="set-field">
            <span className="set-label">{t('Frequency')}</span>
            <select value={g.frequency} onChange={(e) => set({ frequency: +e.target.value })}>
              {[400, 440, 1000].map((f) => (
                <option key={f} value={f}>{`${f} Hz`}</option>
              ))}
            </select>
          </label>
        </div>
      )}
      {g.enabled && g.pattern === 'lipsync' && <LatencyList channelId={channelId} />}
      {picking && (
        <AssetLibrary
          kind="image"
          title={t('Still picture')}
          value={g.image}
          onClose={() => setPicking(false)}
          onChoose={(src) => {
            set({ image: src });
            setPicking(false);
          }}
        />
      )}
    </div>
  );
}

interface Latency {
  input: string;
  channel: string;
  lastMs: number;
  avgMs: number;
  minMs: number;
  maxMs: number;
  count: number;
  at: number;
}

/** The latency of the inputs that receive this channel's flash back (an output looped to an input). */
function LatencyList({ channelId }: { channelId: string }) {
  const [list, setList] = useState<Latency[]>([]);
  const inputs = useInputs();
  useEffect(() => {
    const read = () =>
      api<Latency[]>('GET', '/api/latency')
        .then((l) => setList(l.filter((x) => x.channel === channelId && Date.now() - x.at < 10_000)))
        .catch(() => {});
    void read();
    const id = setInterval(read, 1000);
    return () => clearInterval(id);
  }, [channelId]);
  return (
    <div className="set-field">
      <span className="set-label">
        {t('Latency measured')}
        <InfoTip
          text={t(
            'Loop an output of this channel back to an input (through the router, the encoder, the network…): each flash is timed from when it leaves to when the input receives it, decoding included.',
          )}
        />
      </span>
      {list.length ? (
        list.map((l) => (
          <div key={l.input} className="gen-latency">
            <b>{inputs.find((i) => i.id === l.input)?.name ?? l.input}</b>
            <span className="mono">{Math.round(l.lastMs)} ms</span>
            <span className="muted">{t('average {avg} ms · {min}–{max} ms · {n} flashes', { avg: Math.round(l.avgMs), min: Math.round(l.minMs), max: Math.round(l.maxMs), n: l.count })}</span>
          </div>
        ))
      ) : (
        <span className="muted">{t('No input receives the flash back yet.')}</span>
      )}
    </div>
  );
}

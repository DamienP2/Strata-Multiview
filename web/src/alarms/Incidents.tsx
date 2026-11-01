// The pictures recorded around a fault (crates/strata-mv-server/src/incidents.rs): a thumbnail in
// the journal, and a player (twice a second, from 15 s before to 20 s after).
import { Pause, Play, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { JournalEntry } from '../engine/liveInputs';
import { t } from '../i18n';
import { api } from '../store';

export interface Incident {
  id: string;
  inputId: string;
  input: string;
  at: number;
  kinds: string[];
  frames: number[];
  recording: boolean;
}

/** The recordings, read again every 10 s while the journal shows. */
export function useIncidents(): Incident[] {
  const [list, setList] = useState<Incident[]>([]);
  useEffect(() => {
    const read = () =>
      api<Incident[]>('GET', '/api/incidents')
        .then(setList)
        .catch(() => {});
    void read();
    const id = setInterval(read, 10_000);
    return () => clearInterval(id);
  }, []);
  return list;
}

/** The recording of a line of the journal: same input, begun within 5 s. */
export const incidentOf = (list: Incident[], e: JournalEntry) => (e.inputId ? list.find((i) => i.inputId === e.inputId && Math.abs(i.at - e.at) <= 5000) : undefined);

export function Thumb({ inc, onOpen }: { inc: Incident; onOpen: () => void }) {
  if (!inc.frames.length) return null;
  return (
    <button type="button" className="inc-thumb" title={t('Play the pictures around the fault')} onClick={onOpen}>
      <img src={`/api/incidents/${encodeURIComponent(inc.id)}/thumb.jpg`} alt="" loading="lazy" />
      <Play size={12} />
    </button>
  );
}

export function Player({ inc, onClose }: { inc: Incident; onClose: () => void }) {
  const start = Math.max(
    0,
    inc.frames.findIndex((f) => f >= inc.at),
  );
  const [n, setN] = useState(0);
  const [playing, setPlaying] = useState(true);
  const count = inc.frames.length;
  useEffect(() => {
    if (!playing || count < 2) return;
    const id = setInterval(() => setN((v) => (v + 1) % count), 500);
    return () => clearInterval(id);
  }, [playing, count]);
  // Every picture loaded ahead: smooth playback.
  useEffect(() => {
    for (let i = 0; i < count; i++) new Image().src = `/api/incidents/${encodeURIComponent(inc.id)}/${String(i).padStart(3, '0')}.jpg`;
  }, [inc.id, count]);
  const rel = ((inc.frames[n] ?? inc.at) - inc.at) / 1000;
  return (
    <div className="inc-modal" onClick={onClose}>
      <div className="inc-box" onClick={(e) => e.stopPropagation()}>
        <div className="inc-head">
          <b>{inc.input}</b>
          <span className="muted">
            {new Date(inc.at).toLocaleString()} · {inc.kinds.join(', ')}
          </span>
          <button className="icon-btn ghost" onClick={onClose} title={t('Close')}>
            <X size={16} />
          </button>
        </div>
        <div className="inc-pic">
          <img src={`/api/incidents/${encodeURIComponent(inc.id)}/${String(n).padStart(3, '0')}.jpg`} alt="" />
          <span className={`inc-time ${rel >= 0 ? 'after' : ''}`}>{rel >= 0 ? `+${rel.toFixed(1)} s` : `${rel.toFixed(1)} s`}</span>
        </div>
        <div className="inc-bar">
          <button className="icon-btn" onClick={() => setPlaying(!playing)} title={playing ? t('Pause') : t('Play')}>
            {playing ? <Pause size={15} /> : <Play size={15} />}
          </button>
          <input
            type="range"
            min={0}
            max={Math.max(0, count - 1)}
            value={n}
            onChange={(e) => {
              setPlaying(false);
              setN(+e.target.value);
            }}
          />
          <button className="btn small" onClick={() => (setPlaying(false), setN(start))}>
            {t('The fault')}
          </button>
        </div>
        {inc.recording && <p className="muted">{t('Still recording: the following pictures come in.')}</p>}
      </div>
    </div>
  );
}

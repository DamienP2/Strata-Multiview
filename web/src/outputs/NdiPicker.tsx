import { RefreshCw, Search, VideoOff, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../store';
import { t, tn } from '../i18n';
import { friendlyError } from '../ui/serverError';

/** A picture of a source and its format, as the server grabbed it. */
type Shot = { url: string; width: number; height: number; fps: number; interlaced: boolean } | 'none' | 'loading';

/** "HOST (name)" → its machine and its name. */
const split = (s: string) => {
  const m = /^(.*?)\s*\((.*)\)\s*$/.exec(s);
  return m ? { host: m[1], name: m[2] } : { host: s, name: s };
};

const formatOf = (s: { height: number; fps: number; interlaced: boolean }) => `${s.height}${s.interlaced ? 'i' : 'p'} ${+(s.interlaced ? s.fps * 2 : s.fps).toFixed(2)}`;

/** The NDI sources on the network, each with a picture, grouped by machine: one is chosen. */
export function NdiPicker({ current, onChoose, onClose }: { current: string; onChoose: (source: string) => void; onClose: () => void }) {
  const [sources, setSources] = useState<string[] | null>(null);
  const [error, setError] = useState<string>();
  const [shots, setShots] = useState<Record<string, Shot>>({});
  const [filter, setFilter] = useState('');
  const [chosen, setChosen] = useState(current);
  const urls = useRef<string[]>([]);
  const run = useRef(0);

  /** Looks for the sources, then grabs a picture of each (four at a time). */
  const search = async () => {
    const n = ++run.current;
    setSources(null);
    setError(undefined);
    setShots({});
    let list: string[];
    try {
      list = await api<string[]>('GET', '/api/inputs/ndi-sources');
    } catch (e) {
      if (n === run.current) {
        setSources([]);
        setError((e as Error).message);
      }
      return;
    }
    if (n !== run.current) return;
    setSources(list);
    setShots(Object.fromEntries(list.map((s) => [s, 'loading' as Shot])));
    const queue = [...list];
    const grab = async () => {
      for (let s = queue.shift(); s !== undefined && n === run.current; s = queue.shift()) {
        let shot: Shot = 'none';
        try {
          const r = await fetch(`/api/inputs/ndi-preview?source=${encodeURIComponent(s)}&w=400`, { credentials: 'same-origin' });
          if (r.status === 200) {
            const url = URL.createObjectURL(await r.blob());
            urls.current.push(url);
            const num = (k: string) => Number(r.headers.get(k) ?? 0);
            shot = { url, width: num('x-width'), height: num('x-height'), fps: num('x-fps'), interlaced: r.headers.get('x-interlaced') === 'true' };
          }
        } catch {
          /* no picture */
        }
        if (n === run.current) setShots((m) => ({ ...m, [s]: shot }));
      }
    };
    await Promise.all([grab(), grab(), grab(), grab()]);
  };

  useEffect(() => {
    void search();
    const keep = urls.current;
    return () => {
      run.current++;
      keep.forEach((u) => URL.revokeObjectURL(u));
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [onClose]);

  const groups = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const map = new Map<string, string[]>();
    for (const s of sources ?? []) {
      if (q && !s.toLowerCase().includes(q)) continue;
      const { host } = split(s);
      map.set(host, [...(map.get(host) ?? []), s]);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [sources, filter]);

  const pick = split(chosen);
  return (
    <div className="modal-back" onPointerDown={onClose}>
      <div className="modal ndi-pick" role="dialog" aria-label={t('NDI sources')} onPointerDown={(e) => e.stopPropagation()}>
        <header>
          <h2>{t('NDI sources')}</h2>
          {sources && <span className="ndi-count">{tn(sources.length, '{n} on the network', '{n} on the network')}</span>}
          <span className="grow" />
          <label className="ndi-filter">
            <Search size={14} />
            <input value={filter} placeholder={t('Filter…')} spellCheck={false} onChange={(e) => setFilter(e.target.value)} />
          </label>
          <button type="button" className="btn small" disabled={!sources} onClick={() => void search()}>
            <RefreshCw size={13} /> {t('Refresh')}
          </button>
          <button type="button" className="icon-btn ghost" title={t('Close')} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="ndi-body">
          {!sources && <p className="ndi-empty">{t('Searching…')}</p>}
          {sources && !sources.length && <p className="ndi-empty">{error ? friendlyError(error) : t('No NDI source found on the network.')}</p>}
          {groups.map(([host, list]) => (
            <section key={host}>
              <div className="ndi-host">{host}</div>
              <div className="ndi-grid">
                {list.map((s) => {
                  const shot = shots[s];
                  return (
                    <button key={s} type="button" className={`ndi-card ${s === chosen ? 'on' : ''}`} onClick={() => setChosen(s)} onDoubleClick={() => onChoose(s)}>
                      <span className={`ndi-shot ${shot === 'none' ? 'none' : ''}`}>
                        {shot === 'loading' && <span className="ndi-wait" />}
                        {shot === 'none' && (
                          <>
                            <VideoOff size={18} /> {t('No picture')}
                          </>
                        )}
                        {shot && typeof shot === 'object' && (
                          <>
                            <img src={shot.url} alt="" />
                            <span className="ndi-fmt">{formatOf(shot)}</span>
                          </>
                        )}
                      </span>
                      <span className="ndi-meta">
                        <b>{split(s).name}</b>
                        <span>{s}</span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
        <footer>
          <span className="ndi-sel">{chosen ? <>{t('Chosen:')} <b>{pick.name}</b> · {pick.host}</> : t('No source chosen')}</span>
          <span className="grow" />
          <button type="button" className="btn" onClick={onClose}>
            {t('Cancel')}
          </button>
          <button type="button" className="btn primary" disabled={!chosen} onClick={() => onChoose(chosen)}>
            {t('Choose')}
          </button>
        </footer>
      </div>
    </div>
  );
}

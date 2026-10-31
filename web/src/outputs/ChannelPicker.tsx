import { Search, Tv, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { api } from '../store';
import { t, tn } from '../i18n';
import { friendlyError } from '../ui/serverError';

/** A channel of a list (mirrors weblive::Channel). */
export interface Channel {
  name: string;
  url: string;
  group?: string;
  logo?: string;
}

/** The channels of a list (IPTV M3U, read by the server), grouped: one is chosen. */
export function ChannelPicker({ url, onChoose, onClose }: { url: string; onChoose: (c: Channel) => void; onClose: () => void }) {
  const [list, setList] = useState<Channel[] | null>(null);
  const [error, setError] = useState<string>();
  const [filter, setFilter] = useState('');
  const [chosen, setChosen] = useState<Channel | null>(null);

  useEffect(() => {
    let alive = true;
    api<Channel[]>('GET', `/api/inputs/playlist?url=${encodeURIComponent(url)}`)
      .then((l) => alive && setList(l))
      .catch((e) => {
        if (!alive) return;
        setList([]);
        setError((e as Error).message);
      });
    return () => void (alive = false);
  }, [url]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [onClose]);

  const groups = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const map = new Map<string, Channel[]>();
    for (const c of list ?? []) {
      if (q && !`${c.name} ${c.group ?? ''}`.toLowerCase().includes(q)) continue;
      const g = c.group || t('TV channels');
      map.set(g, [...(map.get(g) ?? []), c]);
    }
    return [...map.entries()];
  }, [list, filter]);

  return (
    <div className="modal-back" onPointerDown={onClose}>
      <div className="modal ndi-pick ch-pick" role="dialog" aria-label={t('Channels of the list')} onPointerDown={(e) => e.stopPropagation()}>
        <header>
          <h2>{t('Channels of the list')}</h2>
          {list && list.length > 0 && <span className="ndi-count">{tn(list.length, '{n} TV channel', '{n} TV channels')}</span>}
          <span className="grow" />
          <label className="ndi-filter">
            <Search size={14} />
            <input value={filter} placeholder={t('Filter…')} spellCheck={false} autoFocus onChange={(e) => setFilter(e.target.value)} />
          </label>
          <button type="button" className="icon-btn ghost" title={t('Close')} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="ndi-body">
          {!list && <p className="ndi-empty">{t('Reading the list…')}</p>}
          {list && !list.length && <p className="ndi-empty">{error ? friendlyError(error) : t('This address is not a list of channels: it is read as it is.')}</p>}
          {groups.map(([group, items]) => (
            <section key={group}>
              <div className="ndi-host">{group}</div>
              <div className="ch-list">
                {items.map((c, i) => (
                  <button key={`${c.url}-${i}`} type="button" className={`ch-item ${chosen === c ? 'on' : ''}`} title={c.url} onClick={() => setChosen(c)} onDoubleClick={() => onChoose(c)}>
                    <span className="ch-logo">{c.logo ? <img src={c.logo} alt="" loading="lazy" referrerPolicy="no-referrer" onError={(e) => void (e.currentTarget.style.display = 'none')} /> : <Tv size={14} />}</span>
                    <b>{c.name}</b>
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
        <footer>
          <span className="ndi-sel">{chosen ? <>{t('Chosen:')} <b>{chosen.name}</b></> : t('No channel chosen')}</span>
          <span className="grow" />
          <button type="button" className="btn" onClick={onClose}>
            {t('Cancel')}
          </button>
          <button type="button" className="btn primary" disabled={!chosen} onClick={() => chosen && onChoose(chosen)}>
            {t('Choose')}
          </button>
        </footer>
      </div>
    </div>
  );
}

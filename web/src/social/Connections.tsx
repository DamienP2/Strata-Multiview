import { Plus } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { connectionState, SocialEditor, useSocialSources } from '../data/SocialSources';
import { t, tn } from '../i18n';
import { can, useApp } from '../store';
import { PLATFORM_IDS, PLATFORMS, PlatformBadge, type Platform } from './PlatformBadge';
import { save, type Desk } from './socialStore';

/*
 * Project › Social › Connections: every shared connection of the server as
 * a card with its state, a switch to follow it in this project, and the
 * connection itself (settings and live messages) on the right. A new
 * connection made here is followed at once.
 */

type Pick = { id: string } | { platform: Platform };

export function Connections({ projectId, desk }: { projectId: string; desk: Desk }) {
  const list = useSocialSources();
  const user = useApp((s) => s.auth?.user);
  const edit = can.design(user);
  const [pick, setPick] = useState<Pick | null>(null);
  const picked = pick && 'id' in pick ? pick.id : null;
  // The one picked is gone (deleted): nothing shown.
  useEffect(() => {
    if (picked && !list.some((s) => s.id === picked)) setPick(null);
  }, [list, picked]);
  const follow = (id: string, on: boolean) => void save(projectId, 'follow', { sources: on ? [...desk.sources, id] : desk.sources.filter((x) => x !== id) });
  const followed = desk.sources.filter((id) => list.some((s) => s.id === id)).length;
  return (
    <div className="soc-conns">
      <section className="soc-conns-list">
        <header className="mod-head">
          <b>{t('Shared connections')}</b>
          <span className="mod-count">{list.length}</span>
          <span className="spacer" />
          {list.length > 0 && <span className="muted small">{tn(followed, '{n} followed by this project', '{n} followed by this project (plural)')}</span>}
        </header>
        {list.length ? (
          <div className="soc-grid">
            {list.map((s) => {
              const st = connectionState(s);
              const on = desk.sources.includes(s.id);
              return (
                <div key={s.id} className={`soc-card ${on ? 'on' : ''} ${picked === s.id ? 'sel' : ''}`} role="button" tabIndex={0} onClick={() => setPick({ id: s.id })} onKeyDown={(e) => e.key === 'Enter' && setPick({ id: s.id })}>
                  <div className="soc-card-head">
                    <PlatformBadge platform={s.platform} />
                    <strong>{s.name}</strong>
                    <label className="switch" title={on ? t('Followed by this project') : t('Follow in this project')} onClick={(e) => e.stopPropagation()}>
                      <input type="checkbox" checked={on} onChange={(e) => follow(s.id, e.target.checked)} />
                    </label>
                  </div>
                  <div className="soc-card-state" title={st.raw ?? st.text}>
                    <i className={`status-dot ${st.dot}`} />
                    {st.text}
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="empty big ux3-no-msg">{edit && <NewConnection big onPick={(p) => setPick({ platform: p })} />}</div>
        )}
      </section>
      <section className="soc-conns-side">
        {pick ? (
          <SocialEditor
            key={'id' in pick ? pick.id : `new-${pick.platform}`}
            pick={pick}
            editable={edit}
            onSaved={(id) => {
              // Made here: followed here.
              if (!('id' in pick) && !desk.sources.includes(id)) follow(id, true);
              setPick({ id });
            }}
            onClose={() => setPick(null)}
          />
        ) : (
          <div className="empty big ux3-no-msg">{edit && list.length > 0 && <NewConnection big onPick={(p) => setPick({ platform: p })} />}</div>
        )}
      </section>
    </div>
  );
}

/** « + Connection »: the platform of the new one. */
export function NewConnection({ onPick, big }: { onPick: (p: Platform) => void; big?: boolean }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    addEventListener('mousedown', close);
    addEventListener('keydown', esc);
    return () => {
      removeEventListener('mousedown', close);
      removeEventListener('keydown', esc);
    };
  }, [open]);
  return (
    <div className={`mod-follow ${big ? 'big' : ''}`} ref={ref}>
      <button className={`btn primary ${big ? 'big' : ''}`} onClick={() => setOpen(!open)}>
        <Plus size={14} /> {t('Connection')}
      </button>
      {open && (
        <div className="mod-menu">
          {PLATFORM_IDS.map((p) => (
            <button
              key={p}
              onClick={() => {
                setOpen(false);
                onPick(p);
              }}
            >
              <PlatformBadge platform={p} size={16} /> {PLATFORMS[p].label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

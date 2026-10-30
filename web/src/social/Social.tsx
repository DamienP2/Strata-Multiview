import { Plus } from 'lucide-react';
import { useState } from 'react';
import { t } from '../i18n';
import { promptDialog } from '../ui/dialogs';
import { PlatformBadge } from './PlatformBadge';
import { Connections } from './Connections';
import { Hashtags } from './Hashtags';
import { Moderation } from './Moderation';
import { RulesPage } from './Rules';
import { save, useDesk, useSocial, type Desk } from './socialStore';

/*
 * Project › Social: the shared connections the project follows, what it
 * does with their messages (moderation: incoming, approved, on air), its
 * rules, and the hashtags it counts.
 */

type Sub = 'connections' | 'moderation' | 'rules' | 'hashtags';
const LAST_SUB = 'strata.socialTab';

function recall(): Sub {
  try {
    const v = localStorage.getItem(LAST_SUB);
    if (v === 'rules' || v === 'hashtags' || v === 'connections') return v;
  } catch {
    /* private mode */
  }
  return 'moderation';
}

export function Social({ projectId }: { projectId: string }) {
  const desk = useDesk(projectId);
  const [chosen, setSub] = useState<Sub | null>(null);
  // Nothing followed yet: the connections first.
  const sub: Sub = chosen ?? (desk && !desk.sources.length ? 'connections' : recall());
  const pick = (s: Sub) => {
    setSub(s);
    try {
      localStorage.setItem(LAST_SUB, s);
    } catch {
      /* private mode */
    }
  };
  const addTag = async () => {
    if (!desk) return;
    const tag = await promptDialog({ title: t('New hashtag'), placeholder: '#JT20H', confirm: t('Add') });
    if (!tag?.trim()) return;
    void save(projectId, 'hashtags', [...desk.hashtags.map((h) => ({ tag: h.tag, running: h.running })), { tag, running: true }]);
  };
  return (
    <div className="mod-page">
      <div className="mod-subtabs">
        <div className="tabs">
          <button className={sub === 'connections' ? 'on' : ''} onClick={() => pick('connections')}>
            {t('Connections')}
          </button>
          <button className={sub === 'moderation' ? 'on' : ''} onClick={() => pick('moderation')}>
            {t('Moderation')}
          </button>
          <button className={sub === 'rules' ? 'on' : ''} onClick={() => pick('rules')}>
            {t('Rules')}
          </button>
          <button className={sub === 'hashtags' ? 'on' : ''} onClick={() => pick('hashtags')}>
            {t('Hashtags')}
          </button>
        </div>
        <span className="spacer" />
        {desk && sub === 'moderation' && <Followed desk={desk} onConnections={() => pick('connections')} />}
        {desk && sub === 'hashtags' && (
          <button className="btn" onClick={addTag}>
            <Plus size={14} /> {t('Hashtag')}
          </button>
        )}
      </div>
      {!desk ? (
        <div className="loading">{t('Loading…')}</div>
      ) : sub === 'connections' ? (
        <Connections projectId={projectId} desk={desk} />
      ) : sub === 'moderation' ? (
        <Moderation projectId={projectId} desk={desk} onConnections={() => pick('connections')} />
      ) : sub === 'rules' ? (
        <RulesPage projectId={projectId} desk={desk} />
      ) : (
        <Hashtags projectId={projectId} desk={desk} />
      )}
    </div>
  );
}

/** The connections the project follows (chips), and the way to the Connections tab. */
function Followed({ desk, onConnections }: { desk: Desk; onConnections: () => void }) {
  const connections = useSocial((s) => s.connections);
  return (
    <div className="mod-follow">
      {desk.sources.map((id) => {
        const c = connections.find((x) => x.id === id);
        return (
          <span key={id} className={`mod-conn ${c && !c.connected ? 'off' : ''}`} title={c && !c.connected ? t('Not connected') : undefined}>
            {c && <PlatformBadge platform={c.platform} size={15} />}
            {c?.name ?? id}
          </span>
        );
      })}
      <button className="mod-conn add" onClick={onConnections}>
        <Plus size={12} /> {t('Connection')}
      </button>
    </div>
  );
}

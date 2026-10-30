import { InfoTip } from '../ui/InfoTip';
import { friendlyError } from '../ui/serverError';
import { Cable, Copy, KeyRound, ListOrdered, Newspaper, Plug } from 'lucide-react';
import { useEffect, useState } from 'react';
import { t, tn } from '../i18n';
import { NavItem, NavSection, PageHead, SidePage } from '../pages/SidePage';
import { navigate } from '../router';
import { Card, Field, SaveBar } from '../settings/ui';
import { useServerConfig } from '../settings/serverConfig';
import { api, useApp } from '../store';

export interface MosConfig {
  enabled: boolean;
  mosId: string;
  ncsId: string;
  lowerPort: number;
  upperPort: number;
  projectId: string;
}

interface MosStatus {
  listening: boolean;
  error: string | null;
  connections: number;
  lastMessage: string | null;
  lastMessageAt: number | null;
  runningOrders: { id: string; slug: string; projectId: string; stories: number; items: number }[];
}

type Section = 'connection' | 'orders' | 'plugin';

/** Newsroom systems (MOS 2.8: Saga, iNews, ENPS, Octopus…). */
export function MosPage() {
  const { projects, toast } = useApp();
  const { cfg, edit, dirty, saving, save, discard } = useServerConfig('mos');
  const [status, setStatus] = useState<MosStatus | null>(null);
  const [section, setSection] = useState<Section>('connection');
  useEffect(() => {
    const load = () => api<MosStatus>('GET', '/api/mos/status').then(setStatus).catch(() => {});
    load();
    const h = setInterval(() => document.hidden || void load(), 3000);
    return () => clearInterval(h);
  }, []);
  if (!cfg) return <div className="loading">{t('Loading…')}</div>;

  const mos = cfg.mos;
  const set = (patch: Partial<MosConfig>) => edit((c) => void (c.mos = { ...c.mos, ...patch }));
  const base = `${location.protocol}//${location.host}`;
  const plugin = `${base}/mos/plugin?key=pcg_…`;
  const state: 'ok' | 'warn' | 'off' = !mos.enabled ? 'off' : status?.listening ? 'ok' : 'warn';
  const stateText = !mos.enabled ? t('Off') : status?.listening ? tn(status.connections, 'Listening · {n} connection', 'Listening · {n} connections') : (status?.error ? friendlyError(status.error) : t('Not listening'));
  const orders = status?.runningOrders ?? [];

  const item = (s: Section, icon: React.ReactNode, label: string, extra?: Partial<React.ComponentProps<typeof NavItem>>) => <NavItem on={section === s} icon={icon} label={label} onClick={() => setSection(s)} {...extra} />;
  const nav = (
    <>
      <NavSection>
        {t('Newsroom')}
        <InfoTip text={t('Saga (Fonn Group), iNews, ENPS, Octopus… send their running orders over MOS 2.8. Each one becomes a playlist of the project.')} />
      </NavSection>
      {item('connection', <Cable size={15} />, t('Connection'), { status: state })}
      {item('orders', <ListOrdered size={15} />, t('Running orders'), { count: orders.length })}
      {item('plugin', <Plug size={15} />, t('Journalist plugin'))}
    </>
  );

  return (
    <SidePage nav={nav}>
      <PageHead title={t('Newsroom (MOS)')} sub={stateText}>
        <span className={`mos-state ${state}`}>
          <span className={`nav-dot ${state}`} /> {!mos.enabled ? t('Off') : !status?.listening ? t('Not listening') : status.connections ? t('Connected') : t('Listening')}
        </span>
      </PageHead>

      {section === 'connection' && (
        <>
          <Card icon={<Newspaper size={16} />} title={t('MOS device')}>
            <div className="set-grid">
              <Field label={t('Connections')}>
                <label className="check">
                  <input type="checkbox" checked={mos.enabled} onChange={(e) => set({ enabled: e.target.checked })} />
                  {t('Accept newsroom connections')}
                </label>
              </Field>
              <Field label={t('MOS ID')} hint={t('Declare the same ID in the newsroom system.')}>
                <input value={mos.mosId} onChange={(e) => set({ mosId: e.target.value })} />
              </Field>
              <Field label={t('NCS ID')} hint={t('Newsroom system ID (informational).')}>
                <input value={mos.ncsId} placeholder="SAGA" onChange={(e) => set({ ncsId: e.target.value })} />
              </Field>
              <Field label={t('Lower port')} hint={t('Profiles 0–1 (objects).')}>
                <input type="number" value={mos.lowerPort} onChange={(e) => set({ lowerPort: +e.target.value })} />
              </Field>
              <Field label={t('Upper port')} hint={t('Profile 2 (running orders).')}>
                <input type="number" value={mos.upperPort} onChange={(e) => set({ upperPort: +e.target.value })} />
              </Field>
              <Field label={t('Project')} hint={t('Receives the running orders. Automatic: the project of the graphics.')}>
                <select value={mos.projectId} onChange={(e) => set({ projectId: e.target.value })}>
                  <option value="">{t('Automatic')}</option>
                  {projects.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </Field>
            </div>
          </Card>
          {status?.lastMessage && (
            <p className="hint">
              {t('Last message:')} <code>{status.lastMessage}</code> {status.lastMessageAt && new Date(status.lastMessageAt).toLocaleTimeString()}
            </p>
          )}
        </>
      )}

      {section === 'orders' &&
        (orders.length ? (
          <table className="user-table">
            <thead>
              <tr>
                <th>{t('Running order')}</th>
                <th>{t('Project')}</th>
                <th>{t('Stories')}</th>
                <th>{t('Items')}</th>
              </tr>
            </thead>
            <tbody>
              {orders.map((r) => (
                <tr key={r.id}>
                  <td>
                    <strong>{r.slug || r.id}</strong> <span className="muted">{r.id}</span>
                  </td>
                  <td>{projects.find((p) => p.id === r.projectId)?.name ?? r.projectId}</td>
                  <td>{r.stories}</td>
                  <td>{r.items}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <div className="empty big">
            <ListOrdered size={22} />
            <p>{mos.enabled ? t('No running order received yet.') : t('Turn on the newsroom connections to receive running orders.')}</p>
          </div>
        ))}

      {section === 'plugin' && (
        <Card icon={<Plug size={16} />} title={t('HTML5 plugin for the journalists')}>
          <div className="mos-plugin-url">
            <div className="row">
              <code className="grow">{plugin}</code>
              <button
                className="btn small"
                onClick={() =>
                  navigator.clipboard
                    ?.writeText(plugin)
                    .then(() => toast(t('Copied'), 'info'))
                    .catch(() => toast(plugin))
                }
              >
                <Copy size={13} /> {t('Copy')}
              </button>
            </div>
            <span className="set-hint">{t('Register this URL as an HTML5 MOS plugin in the newsroom system, with the API key of an operator account restricted to the right projects.')}</span>
          </div>
          <div>
            <button className="btn" onClick={() => navigate('/api-docs')}>
              <KeyRound size={14} /> {t('Create an API key')}
            </button>
          </div>
        </Card>
      )}

      <SaveBar dirty={dirty} saving={saving} onSave={save} onDiscard={discard} />
    </SidePage>
  );
}

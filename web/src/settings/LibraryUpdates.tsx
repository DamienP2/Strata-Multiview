import { CloudDownload, Images, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api, useApp } from '../store';
import { t, tn } from '../i18n';
import { ago } from './Updates';

/** A picture library shipped with the app (cg-server iconlib.rs). */
interface Library {
  id: string;
  kind: 'icons' | 'emoji' | 'flags' | 'illustrations';
  title: string;
  license: string;
  total: number;
  /** npm package: it can be updated. */
  package: string | null;
  inUse: string;
  update: string | null;
}

interface Libraries {
  libraries: Library[];
  lastCheck: number | null;
  error: string | null;
}

const KIND: Record<Library['kind'], string> = { icons: 'Icons', emoji: 'Emojis', flags: 'Flags', illustrations: 'Illustrations' };

/** The picture libraries: their versions, updates looked for once a week and installed on a click. */
export function LibraryUpdates({ Card }: { Card: (p: { icon: React.ReactNode; title: string; children: React.ReactNode; actions?: React.ReactNode }) => React.ReactElement }) {
  const toast = useApp((s) => s.toast);
  const [libs, setLibs] = useState<Libraries | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  useEffect(() => void api<Libraries>('GET', '/api/iconlib/libraries').then(setLibs).catch(() => {}), []);
  if (!libs) return null;
  const run = async (what: string, path: string) => {
    setBusy(what);
    try {
      setLibs(await api<Libraries>('POST', path));
    } catch (e) {
      toast((e as Error).message, 'error');
    }
    setBusy(null);
  };
  const updates = libs.libraries.filter((l) => l.update);
  return (
    <Card
      icon={<Images size={16} />}
      title={t('Picture libraries')}
      actions={
        <button className="btn small" disabled={!!busy} onClick={() => void run('check', '/api/iconlib/libraries/check')}>
          <RefreshCw size={13} className={busy === 'check' ? 'spin' : ''} /> {t('Check now')}
        </button>
      }
    >
      <div className="update-rows">
        {libs.libraries.map((l) => (
          <div key={l.id} className="update-row">
            <strong>{l.title}</strong>
            <span className="muted small">{t(KIND[l.kind])}</span>
            <span className="muted small">{l.inUse}</span>
            <span className="muted small">{t('{n} pictures', { n: l.total.toLocaleString() })}</span>
            <span className="muted small">{l.license}</span>
            <span className="spacer" />
            {l.update ? (
              <button className="btn small primary" disabled={!!busy} onClick={() => void run(l.id, `/api/iconlib/libraries/${encodeURIComponent(l.id)}/update`)}>
                {busy === l.id ? <RefreshCw size={13} className="spin" /> : <CloudDownload size={13} />} {t('Update to {version}', { version: l.update })}
              </button>
            ) : (
              <span className="badge">{l.package ? t('Up to date') : t('With the app')}</span>
            )}
          </div>
        ))}
      </div>
      <p className="hint">
        {updates.length ? tn(updates.length, '{n} update available', '{n} updates available') : t('Up to date')}
        {libs.lastCheck ? ` · ${t('checked {when}', { when: ago(libs.lastCheck) })}` : ''}
      </p>
      {libs.error && <p className="update-error">{libs.error}</p>}
    </Card>
  );
}

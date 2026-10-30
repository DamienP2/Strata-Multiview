import { CloudDownload, History, KeyRound, RefreshCw, RotateCcw, Upload } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { api, useApp } from '../store';
import { confirmDialog } from '../ui/dialogs';
import { locale, t } from '../i18n';

interface Release {
  version: string;
  name: string;
  notes: string;
  publishedAt: string;
  url: string;
  assetName: string;
  size: number;
}

interface UpdateInfo {
  current: string;
  platform: string;
  /** The release file to upload (`Strata-<version>-…`). */
  package?: string;
  repo: string;
  hasToken: boolean;
  autoCheck: boolean;
  previous: string | null;
  history: { from: string; to: string; at: number; outcome: string } | null;
  supervised: boolean;
  status: {
    phase: 'idle' | 'checking' | 'downloading' | 'installing' | 'restarting' | 'error';
    progress: number;
    message: string;
    latest: Release | null;
    /** Every release with a package for this system, newest first. */
    releases: Release[];
    lastCheck: number | null;
  };
}

const key = (v: string) => v.split(/[.+-]/).map((p) => parseInt(p, 10) || 0);
export const isNewer = (a: string, b: string) => {
  const x = key(a);
  const y = key(b);
  for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  return false;
};

export const ago = (ms: number) => {
  const s = Math.round((Date.now() - ms) / 1000);
  return s < 60 ? t('just now') : s < 3600 ? t('{n} min ago', { n: Math.round(s / 60) }) : s < 86400 ? t('{n} h ago', { n: Math.round(s / 3600) }) : new Date(ms).toLocaleDateString(locale());
};

/** Over-the-air updates: GitHub releases or an uploaded package, with rollback. */
export function Updates({ Card }: { Card: (p: { icon: React.ReactNode; title: string; children: React.ReactNode; actions?: React.ReactNode }) => React.ReactElement }) {
  const toast = useApp((s) => s.toast);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [repo, setRepo] = useState('');
  const [token, setToken] = useState('');
  const [allowOlder, setAllowOlder] = useState(false);
  const [waiting, setWaiting] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const load = () =>
    api<UpdateInfo>('GET', '/api/system/update')
      .then((i) => {
        setInfo(i);
        setRepo((r) => r || i.repo);
      })
      .catch(() => {});
  useEffect(() => void load(), []);

  // Poll while something is going on.
  const busy = info && ['checking', 'downloading', 'installing', 'restarting'].includes(info.status.phase);
  useEffect(() => {
    if (!busy || waiting) return;
    const h = setInterval(load, 700);
    return () => clearInterval(h);
  }, [busy, waiting]);

  // After an install or a rollback: wait for the server to come back, then
  // reload so the web app matches the new version.
  useEffect(() => {
    if (!waiting) return;
    const started = Date.now();
    const h = setInterval(async () => {
      try {
        const r = await fetch('/api/health');
        const v = ((await r.json()) as { version: string }).version;
        if (v !== waiting) {
          clearInterval(h);
          toast(t('Strata {version} is running', { version: v }), 'info');
          setTimeout(() => location.reload(), 600);
        }
      } catch {
        /* restarting */
      }
      if (Date.now() - started > 120_000) {
        clearInterval(h);
        setWaiting(null);
        toast(t('The server did not come back within 2 minutes: check its logs.'), 'error');
      }
    }, 1000);
    return () => clearInterval(h);
  }, [waiting]);

  if (!info) return null;
  const { status } = info;
  const latest = status.latest;
  const newer = latest && isNewer(latest.version, info.current);
  const restartNote = t('The engine restarts: graphics on air are cleared and outputs stop for a few seconds.');

  const check = async () => {
    try {
      await api('POST', '/api/system/update/check');
    } catch (e) {
      toast((e as Error).message, 'error');
    }
    load();
  };
  /** Installs a release: the latest, or one of the list (older: going back). */
  const install = async (r: Release | null = latest, older = false) => {
    if (!r || !(await confirmDialog({ title: older ? t('Go back to version {version}?', { version: r.version }) : t('Install Strata {version}?', { version: r.version }), message: restartNote, confirm: older ? t('Roll back') : t('Install'), danger: older }))) return;
    await api('POST', `/api/system/update/install${r === latest ? '' : `?version=${encodeURIComponent(r.version)}`}`).catch((e) => toast((e as Error).message, 'error'));
    load();
    // The status poll turns into the "waiting for restart" poll.
    const h = setInterval(async () => {
      const i = await api<UpdateInfo>('GET', '/api/system/update').catch(() => null);
      if (!i || i.status.phase === 'restarting') {
        clearInterval(h);
        setWaiting(info.current);
      } else if (i.status.phase === 'error') {
        clearInterval(h);
        setInfo(i);
      } else setInfo(i);
    }, 700);
  };
  const upload = async (file: File) => {
    if (!(await confirmDialog({ title: t('Install the package “{name}”?', { name: file.name }), message: restartNote, confirm: t('Install') }))) return;
    try {
      const res = await fetch(`/api/system/update/upload?allowOlder=${allowOlder}`, { method: 'POST', body: file, credentials: 'same-origin' });
      if (!res.ok) throw new Error(await res.text());
      const { version } = (await res.json()) as { version: string };
      toast(t('Installing {version}…', { version }), 'info');
      setWaiting(info.current);
    } catch (e) {
      toast((e as Error).message, 'error');
      load();
    }
  };
  const rollback = async () => {
    if (!info.previous || !(await confirmDialog({ title: t('Go back to version {version}?', { version: info.previous }), message: restartNote, confirm: t('Roll back'), danger: true }))) return;
    try {
      await api('POST', '/api/system/update/rollback');
      setWaiting(info.current);
    } catch (e) {
      toast((e as Error).message, 'error');
    }
  };
  const saveSettings = async (patch: { token?: string | null; autoCheck?: boolean }) => {
    try {
      await api('PUT', '/api/system/update/settings', { repo, autoCheck: patch.autoCheck ?? info.autoCheck, token: patch.token ?? null });
      setToken('');
      toast(t('Update settings saved'), 'info');
      load();
    } catch (e) {
      toast((e as Error).message, 'error');
    }
  };

  return (
    <Card
      icon={<CloudDownload size={16} />}
      title={t('Updates')}
      actions={
        <button className="btn small" disabled={!!busy || !!waiting} onClick={check}>
          <RefreshCw size={13} /> {t('Check now')}
        </button>
      }
    >
      <div className="update-head">
        <div>
          <span className="set-label">{t('Installed')}</span>
          <strong className="update-version">{info.current}</strong>
          <span className="muted small">{info.platform}</span>
        </div>
        <div className="update-latest">
          {waiting ? (
            <span className="update-busy">
              <RefreshCw size={14} className="spin" /> {t('Restarting on the new version…')}
            </span>
          ) : busy ? (
            <span className="update-busy">
              <RefreshCw size={14} className="spin" /> {status.message || status.phase}
              {status.phase === 'downloading' && <progress max={1} value={status.progress} />}
            </span>
          ) : status.phase === 'error' ? (
            <span className="update-error">{status.message}</span>
          ) : latest ? (
            newer ? (
              <>
                <span>
                  {t('Version')} <strong>{latest.version}</strong> {t('is available')}
                  {latest.publishedAt && ` (${new Date(latest.publishedAt).toLocaleDateString(locale())})`}.
                </span>
                <button className="btn primary" onClick={() => install()}>
                  <CloudDownload size={14} /> {t('Install {version}', { version: latest.version })}
                </button>
              </>
            ) : (
              <span className="muted">
                {t('Up to date — latest release {version}', { version: latest.version })}
                {status.lastCheck ? `, ${t('checked {when}', { when: ago(status.lastCheck) })}` : ''}.
              </span>
            )
          ) : (
            <span className="muted">{t('Not checked yet.')}</span>
          )}
        </div>
      </div>
      {newer && latest?.notes && !waiting && (
        <details className="update-notes">
          <summary>{t('What’s new in {version}', { version: latest.version })}</summary>
          <pre>{latest.notes}</pre>
        </details>
      )}
      {status.releases.length > 0 && !waiting && (
        <div className="update-list">
          <span className="set-label">{t('Available versions')}</span>
          <div className="update-rows">
            {status.releases.map((r) => {
              const here = r.version === info.current;
              const up = isNewer(r.version, info.current);
              return (
                <div key={r.version} className={`update-row ${here ? 'here' : ''}`}>
                  <strong>{r.version}</strong>
                  <span className="muted small">{r.publishedAt ? new Date(r.publishedAt).toLocaleDateString(locale()) : ''}</span>
                  <span className="muted small">{t('{n} MB', { n: (r.size / 1048576).toFixed(0) })}</span>
                  <span className="spacer" />
                  {here ? (
                    <span className="badge">{t('Installed')}</span>
                  ) : (
                    <button className={`btn small ${up ? 'primary' : ''}`} disabled={!!busy} onClick={() => install(r, !up)}>
                      {up ? <CloudDownload size={13} /> : <RotateCcw size={13} />} {up ? t('Install') : t('Roll back')}
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
      {info.history && (
        <p className="hint">
          <History size={12} /> {info.history.outcome === 'restored automatically' ? t('Version {from} failed to start: {to} was restored automatically', { from: info.history.from, to: info.history.to }) : `${info.history.from} → ${info.history.to} (${t(info.history.outcome)})`}, {ago(info.history.at)}.
        </p>
      )}

      <div className="set-grid">
        <label className="set-field">
          <span className="set-label">{t('GitHub repository')}</span>
          <input value={repo} onChange={(e) => setRepo(e.target.value)} onBlur={() => repo !== info.repo && saveSettings({})} placeholder="owner/name" />
          <span className="set-hint">{t('Releases are downloaded from here.')}</span>
        </label>
        <label className="set-field">
          <span className="set-label">
            {t('Access token')} {info.hasToken && <span className="badge">{t('saved')}</span>}
          </span>
          <div className="row">
            <input type="password" value={token} placeholder={info.hasToken ? `••••••••  (${t('leave empty to keep')})` : 'github_pat_…'} autoComplete="off" onChange={(e) => setToken(e.target.value)} />
            <button className="btn small" disabled={!token} onClick={() => saveSettings({ token })} title={t('Save the token')}>
              <KeyRound size={13} />
            </button>
            {info.hasToken && (
              <button className="btn small" onClick={() => saveSettings({ token: '' })} title={t('Remove the token')}>
                ✕
              </button>
            )}
          </div>
          <span className="set-hint">{t('Private repository: a fine-grained token with read-only “Contents” on it. Kept on the server, never shown again.')}</span>
        </label>
        <label className="set-field">
          <span className="set-label">{t('Automatic check')}</span>
          <label className="check">
            <input type="checkbox" checked={info.autoCheck} onChange={(e) => saveSettings({ autoCheck: e.target.checked })} />
            {t('Look for new versions every 6 hours')}
          </label>
          <span className="set-hint">{t('Installing always stays a manual decision.')}</span>
        </label>
      </div>

      <div className="update-offline">
        <div>
          <strong>{t('Offline update')}</strong>
          <span className="muted small">
            {t('Upload the file from the release page — for servers without internet access:')} <code>{info.package ?? `Strata-<version>-${info.platform}`}</code>
          </span>
        </div>
        <label className="check small">
          <input type="checkbox" checked={allowOlder} onChange={(e) => setAllowOlder(e.target.checked)} />
          {t('Allow older versions')}
        </label>
        <button className="btn" disabled={!!busy || !!waiting} onClick={() => fileRef.current?.click()}>
          <Upload size={14} /> {t('Upload package')}
        </button>
        <input
          ref={fileRef}
          type="file"
          accept=".gz,.tgz,.zip,.pkg,.msi,application/gzip,application/zip"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            e.target.value = '';
            if (f) upload(f);
          }}
        />
        {info.previous && (
          <button className="btn" disabled={!!busy || !!waiting} onClick={rollback} title={t('Put the previously installed version back')}>
            <RotateCcw size={14} /> {t('Back to {version}', { version: info.previous })}
          </button>
        )}
      </div>
      {!info.supervised && <p className="hint">{t('This server runs in the foreground: it relaunches itself after an update.')}</p>}
    </Card>
  );
}

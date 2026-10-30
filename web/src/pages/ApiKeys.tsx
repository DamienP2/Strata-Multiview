import { Copy, Eye, KeyRound, Plus, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { User } from '../engine/types';
import { locale, t } from '../i18n';
import { api, can, useApp } from '../store';
import { confirmDialog, promptDialog } from '../ui/dialogs';

export interface ApiKey {
  id: string;
  name: string;
  userId: string;
  prefix: string;
  createdAt: number;
  lastUsed: number | null;
}

const when = (ms: number | null) => (ms ? new Date(ms).toLocaleString(locale()) : t('never'));

/** Permanent keys for automation; the secret is shown once. */
export function ApiKeys({ keys, reload }: { keys: ApiKey[]; reload: () => void }) {
  const { auth, toast } = useApp();
  const me = auth?.user;
  const admin = can.admin(me);
  const [users, setUsers] = useState<User[]>([]);
  const [name, setName] = useState('');
  const [owner, setOwner] = useState(me?.id ?? '');
  const [secret, setSecret] = useState<{ name: string; value: string; again?: boolean } | null>(null);
  useEffect(() => {
    if (admin) api<User[]>('GET', '/api/users').then(setUsers).catch(() => {});
  }, [admin]);
  const userName = (id: string) => (id === me?.id ? me.username : (users.find((u) => u.id === id)?.username ?? id));

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      const r = await api<{ key: ApiKey; secret: string }>('POST', '/api/keys', { name, userId: owner || undefined });
      setSecret({ name: r.key.name, value: r.secret });
      setName('');
      reload();
    } catch (err) {
      toast((err as Error).message, 'error');
    }
  };
  const canCreate = !!me && me.role !== 'viewer';
  // An admin sees a key again after typing their password.
  const reveal = async (k: ApiKey) => {
    const password = await promptDialog({ title: t('Show the key “{name}”', { name: k.name }), label: t('Your password'), password: true, confirm: t('Show') });
    if (!password) return;
    try {
      const r = await api<{ secret: string }>('POST', `/api/keys/${encodeURIComponent(k.id)}/reveal`, { password });
      setSecret({ name: k.name, value: r.secret, again: true });
    } catch (err) {
      toast((err as Error).message, 'error');
    }
  };

  return (
    <>
      {secret && (
        <div className="key-secret">
          <strong>{secret.again ? t('Key “{name}”:', { name: secret.name }) : t('Key “{name}” created — copy it now:', { name: secret.name })}</strong>
          <div className="row">
            <code>{secret.value}</code>
            <button
              className="btn small"
              onClick={() =>
                navigator.clipboard
                  ?.writeText(secret.value)
                  .then(() => toast(t('Copied'), 'info'))
                  .catch(() => toast(secret.value))
              }
            >
              <Copy size={13} /> {t('Copy')}
            </button>
            <button className="btn small" onClick={() => setSecret(null)}>
              {t('Done')}
            </button>
          </div>
        </div>
      )}
      {canCreate && (
        <form className="key-form" onSubmit={create}>
          <KeyRound size={16} className="key-form-icon" />
          <input value={name} placeholder={t('What uses it — e.g. Companion control room A')} onChange={(e) => setName(e.target.value)} />
          {admin && (
            <select value={owner} onChange={(e) => setOwner(e.target.value)} title={t('The key acts as this user')}>
              {users.map((u) => (
                <option key={u.id} value={u.id}>
                  {t('as {user} ({role})', { user: u.username, role: t(u.role) })}
                </option>
              ))}
            </select>
          )}
          <button className="btn primary" disabled={!name.trim()}>
            <Plus size={14} /> {t('Create key')}
          </button>
        </form>
      )}
      {keys.length ? (
        <div className="key-grid">
          {keys.map((k) => (
            <div key={k.id} className="key-card">
              <div className="key-card-head">
                <span className="key-icon">
                  <KeyRound size={15} />
                </span>
                <strong>{k.name}</strong>
                {admin && (
                  <button className="icon-btn ghost" title={t('Show the key')} onClick={() => void reveal(k)}>
                    <Eye size={14} />
                  </button>
                )}
                <button
                  className="icon-btn ghost"
                  title={t('Revoke')}
                  onClick={async () => {
                    if (!(await confirmDialog({ title: t('Revoke the key “{name}”?', { name: k.name }), message: t('Systems using it stop working at once.'), confirm: t('Revoke'), danger: true }))) return;
                    await api('DELETE', `/api/keys/${encodeURIComponent(k.id)}`).catch((e) => toast((e as Error).message, 'error'));
                    reload();
                  }}
                >
                  <Trash2 size={14} />
                </button>
              </div>
              <code className="key-prefix">{k.prefix}…</code>
              <dl>
                <dt>{t('Acts as')}</dt>
                <dd>{userName(k.userId)}</dd>
                <dt>{t('Created')}</dt>
                <dd>{when(k.createdAt)}</dd>
                <dt>{t('Last used')}</dt>
                <dd className={k.lastUsed ? '' : 'muted'}>{when(k.lastUsed)}</dd>
              </dl>
            </div>
          ))}
        </div>
      ) : (
        <div className="empty big">
          <KeyRound size={22} />
          <p>{t('No API key yet.')}</p>
        </div>
      )}
    </>
  );
}

import { Ban, Eye, FolderOpen, KeyRound, MonitorPlay, Palette, Pencil, Plus, Radio, Search, ShieldCheck, Trash2, Users as UsersIcon, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { Role, User } from '../engine/types';
import { api, useApp } from '../store';
import { t, tn } from '../i18n';
import { NavItem, NavSection, PageHead, SidePage } from './SidePage';
import { confirmDialog } from '../ui/dialogs';

const ROLES: { value: Role; label: string; plural: string; hint: string; icon: typeof ShieldCheck }[] = [
  { value: 'admin', label: 'Admin', plural: 'Admins', hint: 'Everything: users, channels, server settings.', icon: ShieldCheck },
  { value: 'designer', label: 'Designer', plural: 'Designers', hint: 'Creates projects and templates; can also operate.', icon: Palette },
  { value: 'operator', label: 'Operator', plural: 'Operators', hint: 'Loads projects on channels, plays graphics, edits rundown data.', icon: Radio },
  { value: 'viewer', label: 'Viewer', plural: 'Viewers', hint: 'Read-only: sees projects and program monitors.', icon: Eye },
];

type Draft = Omit<User, 'id'> & { id?: string; password: string };

const blank = (role: Role = 'operator'): Draft => ({ username: '', displayName: '', role, projects: null, channels: null, disabled: false, lang: '', password: '' });

/** "All" or a subset of ids. */
function Scope({ label, all, value, onChange }: { label: string; all: { id: string; name: string }[]; value: string[] | null; onChange: (v: string[] | null) => void }) {
  return (
    <div className="set-field">
      <span className="set-label">{label}</span>
      <label className="check">
        <input type="checkbox" checked={value === null} onChange={(e) => onChange(e.target.checked ? null : [])} />
        {t('All, including future ones')}
      </label>
      {value !== null && (
        <div className="scope-list">
          {all.map((x) => (
            <label key={x.id} className="check">
              <input type="checkbox" checked={value.includes(x.id)} onChange={(e) => onChange(e.target.checked ? [...value, x.id] : value.filter((v) => v !== x.id))} />
              {x.name}
            </label>
          ))}
          {all.length === 0 && <span className="muted">{t('None yet.')}</span>}
        </div>
      )}
    </div>
  );
}

function UserDialog({ initial, onClose, onSaved }: { initial: Draft; onClose: () => void; onSaved: () => void }) {
  const { projects, channels, toast } = useApp();
  const [d, setD] = useState(initial);
  const [busy, setBusy] = useState(false);
  const isNew = !initial.id;
  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      if (isNew) await api('POST', '/api/users', d);
      else await api('PUT', `/api/users/${encodeURIComponent(initial.id!)}`, d);
      onSaved();
      onClose();
    } catch (err) {
      toast((err as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  const scoped = d.role !== 'admin';
  return (
    <div className="modal-back" onPointerDown={onClose}>
      <form className="modal wide" onSubmit={save} onPointerDown={(e) => e.stopPropagation()}>
        <header>
          <h2>{isNew ? t('New user') : t('Edit {name}', { name: initial.username })}</h2>
          <button type="button" className="icon-btn ghost" onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="set-grid two">
          <label className="set-field">
            <span className="set-label">{t('User name')}</span>
            <input autoFocus value={d.username} autoComplete="off" onChange={(e) => setD({ ...d, username: e.target.value })} />
          </label>
          <label className="set-field">
            <span className="set-label">{t('Display name')}</span>
            <input value={d.displayName} onChange={(e) => setD({ ...d, displayName: e.target.value })} />
          </label>
          <label className="set-field">
            <span className="set-label">{isNew ? t('Password') : t('New password')}</span>
            <input type="password" value={d.password} autoComplete="new-password" placeholder={isNew ? t('At least 8 characters') : t('Leave empty to keep it')} onChange={(e) => setD({ ...d, password: e.target.value })} />
          </label>
          <label className="set-field">
            <span className="set-label">{t('Status')}</span>
            <label className="check">
              <input type="checkbox" checked={d.disabled} onChange={(e) => setD({ ...d, disabled: e.target.checked })} />
              {t('Account disabled')}
            </label>
          </label>
        </div>
        <div className="set-field">
          <span className="set-label">{t('Role')}</span>
          <div className="role-pick">
            {ROLES.map((r) => (
              <button type="button" key={r.value} className={d.role === r.value ? 'on' : ''} onClick={() => setD({ ...d, role: r.value })}>
                <strong>{t(r.label)}</strong>
                <span>{t(r.hint)}</span>
              </button>
            ))}
          </div>
        </div>
        {scoped && (
          <div className="set-grid two">
            <Scope label={t('Projects')} all={projects} value={d.projects} onChange={(v) => setD({ ...d, projects: v })} />
            <Scope label={t('Channels')} all={channels} value={d.channels} onChange={(v) => setD({ ...d, channels: v })} />
          </div>
        )}
        <footer>
          <button type="button" className="btn" onClick={onClose}>
            {t('Cancel')}
          </button>
          <button className="btn primary" disabled={busy || !d.username.trim() || (isNew && d.password.length < 8)}>
            {isNew ? <Plus size={14} /> : <KeyRound size={14} />} {isNew ? t('Create user') : t('Save')}
          </button>
        </footer>
      </form>
    </div>
  );
}

type View = { kind: 'all' } | { kind: 'role'; role: Role } | { kind: 'disabled' };

/** People: their role and the projects and channels they reach. */
export function Users() {
  const { auth, projects, channels, toast } = useApp();
  const [users, setUsers] = useState<User[]>([]);
  const [editing, setEditing] = useState<Draft | null>(null);
  const [view, setView] = useState<View>({ kind: 'all' });
  const [query, setQuery] = useState('');
  const load = () => api<User[]>('GET', '/api/users').then(setUsers).catch((e) => toast((e as Error).message, 'error'));
  useEffect(() => void load(), []);

  const scope = (ids: string[] | null, all: { id: string; name: string }[]) => (ids === null ? null : ids.map((id) => all.find((x) => x.id === id)?.name ?? id));
  const inView = (u: User) => (view.kind === 'all' ? true : view.kind === 'disabled' ? u.disabled : u.role === view.role);
  const q = query.trim().toLowerCase();
  const shown = users.filter((u) => inView(u) && (!q || u.username.toLowerCase().includes(q) || u.displayName.toLowerCase().includes(q))).sort((a, b) => a.displayName.localeCompare(b.displayName));
  const disabled = users.filter((u) => u.disabled).length;
  const remove = async (u: User) => {
    if (!(await confirmDialog({ title: t('Delete the user “{name}”?', { name: u.username }), confirm: t('Delete'), danger: true }))) return;
    await api('DELETE', `/api/users/${encodeURIComponent(u.id)}`).catch((e) => toast((e as Error).message, 'error'));
    load();
  };

  const role = (r: Role) => ROLES.find((x) => x.value === r)!;
  const title = view.kind === 'all' ? t('All users') : view.kind === 'disabled' ? t('Disabled accounts') : t(role(view.role).plural);
  const nav = (
    <>
      <NavSection>{t('People')}</NavSection>
      <NavItem on={view.kind === 'all'} icon={<UsersIcon size={15} />} label={t('All users')} count={users.length} onClick={() => setView({ kind: 'all' })} />
      <NavSection>{t('Roles')}</NavSection>
      {ROLES.map((r) => (
        <NavItem key={r.value} on={view.kind === 'role' && view.role === r.value} icon={<r.icon size={15} />} label={t(r.plural)} count={users.filter((u) => u.role === r.value).length} onClick={() => setView({ kind: 'role', role: r.value })} />
      ))}
      {disabled > 0 && <NavItem on={view.kind === 'disabled'} icon={<Ban size={15} />} label={t('Disabled')} count={disabled} onClick={() => setView({ kind: 'disabled' })} />}
      {view.kind === 'role' && <p className="lib-nav-hint">{t(role(view.role).hint)}</p>}
    </>
  );

  const Scopes = ({ icon, ids, all }: { icon: React.ReactNode; ids: string[] | null; all: { id: string; name: string }[] }) => {
    const names = scope(ids, all);
    return (
      <div className="user-scope">
        {icon}
        {names === null ? (
          <span className="scope-chip all">{t('All')}</span>
        ) : names.length === 0 ? (
          <span className="muted">{t('None')}</span>
        ) : (
          <>
            {names.slice(0, 3).map((n) => (
              <span key={n} className="scope-chip">
                {n}
              </span>
            ))}
            {names.length > 3 && <span className="muted">+{names.length - 3}</span>}
          </>
        )}
      </div>
    );
  };

  return (
    <SidePage nav={nav}>
      <PageHead title={title} sub={tn(shown.length, '{n} user', '{n} users')}>
        <button className="btn primary" onClick={() => setEditing(blank(view.kind === 'role' ? view.role : 'operator'))}>
          <Plus size={14} /> {t('New user')}
        </button>
      </PageHead>
      <div className="media-toolbar">
        <label className="search">
          <Search size={14} />
          <input value={query} placeholder={t('Search users')} onChange={(e) => setQuery(e.target.value)} />
        </label>
      </div>
      <div className="user-grid">
        {shown.map((u) => {
          const R = role(u.role);
          const me = u.id === auth?.user?.id;
          return (
            <div key={u.id} className={`user-card role-${u.role} ${u.disabled ? 'dim' : ''}`} onClick={() => setEditing({ ...u, password: '' })}>
              <div className="user-card-head">
                <span className="user-avatar">{(u.displayName || u.username).slice(0, 1).toUpperCase()}</span>
                <div className="user-card-name">
                  <strong>{u.displayName || u.username}</strong>
                  <span className="muted">
                    @{u.username}
                    {me ? ` · ${t('you')}` : ''}
                  </span>
                </div>
                <span className={`badge role-${u.role}`}>
                  <R.icon size={11} /> {t(R.label)}
                </span>
              </div>
              {u.disabled && (
                <span className="user-disabled">
                  <Ban size={12} /> {t('Account disabled')}
                </span>
              )}
              {u.role === 'admin' ? (
                <p className="user-all muted">{t('Every project and channel, users and server settings.')}</p>
              ) : (
                <div className="user-scopes">
                  <Scopes icon={<FolderOpen size={13} />} ids={u.projects} all={projects} />
                  <Scopes icon={<MonitorPlay size={13} />} ids={u.channels} all={channels} />
                </div>
              )}
              <div className="user-card-actions" onClick={(e) => e.stopPropagation()}>
                <button className="btn small" onClick={() => setEditing({ ...u, password: '' })}>
                  <Pencil size={13} /> {t('Edit')}
                </button>
                <button className="btn small" onClick={() => setEditing({ ...u, password: '' })} title={t('Set a new password')}>
                  <KeyRound size={13} /> {t('Password')}
                </button>
                <div className="spacer" />
                {!me && (
                  <button className="icon-btn ghost" title={t('Delete')} onClick={() => void remove(u)}>
                    <Trash2 size={14} />
                  </button>
                )}
              </div>
            </div>
          );
        })}
        {view.kind !== 'disabled' && !q && (
          <button className="project-new user-new" onClick={() => setEditing(blank(view.kind === 'role' ? view.role : 'operator'))}>
            <Plus size={22} />
            <span>{t('New user')}</span>
          </button>
        )}
        {shown.length === 0 && q && (
          <div className="empty big">
            <UsersIcon size={22} />
            <p>{t('No user matches.')}</p>
          </div>
        )}
      </div>
      {editing && <UserDialog initial={editing} onClose={() => setEditing(null)} onSaved={load} />}
    </SidePage>
  );
}

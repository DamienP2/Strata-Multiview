// Settings › Security: HTTPS of the web interface, signing in with the company's directory (LDAP /
// Active Directory, or an OpenID Connect provider), and the audit journal of the changes.
// Mirrors crates/strata-mv-server/src/security.rs.
import { Download, KeyRound, Lock, ShieldCheck, Users } from 'lucide-react';
import { useEffect, useState } from 'react';
import { t } from '../i18n';
import type { LdapConfig, OidcConfig, ServerConfig } from '../outputs/types';
import { api } from '../store';
import { Card, Field } from './ui';

type Edit = (fn: (c: ServerConfig) => void) => void;

const LDAP: LdapConfig = {
  enabled: false,
  url: '',
  starttls: false,
  insecure: false,
  bindDn: '',
  bindPassword: '',
  baseDn: '',
  userFilter: '(sAMAccountName={user})',
  nameAttribute: 'displayName',
  adminGroups: [],
  designerGroups: [],
  operatorGroups: [],
  defaultRole: 'viewer',
};
const OIDC: OidcConfig = {
  enabled: false,
  label: 'SSO',
  issuer: '',
  clientId: '',
  clientSecret: '',
  usernameClaim: 'preferred_username',
  groupsClaim: 'groups',
  adminGroups: [],
  designerGroups: [],
  operatorGroups: [],
  defaultRole: 'viewer',
};

const ROLES: [string, string][] = [
  ['', 'Refused'],
  ['viewer', 'Viewer'],
  ['operator', 'Operator'],
  ['designer', 'Designer'],
  ['admin', 'Administrator'],
];

/** A list typed one per line. */
function Lines({ value, onChange, placeholder }: { value: string[]; onChange: (v: string[]) => void; placeholder?: string }) {
  return (
    <textarea
      rows={2}
      value={value.join('\n')}
      placeholder={placeholder}
      onChange={(e) =>
        onChange(
          e.target.value
            .split('\n')
            .map((x) => x.trim())
            .filter(Boolean),
        )
      }
    />
  );
}

function Groups({ c, set }: { c: { adminGroups: string[]; designerGroups: string[]; operatorGroups: string[]; defaultRole: string }; set: (patch: Partial<LdapConfig & OidcConfig>) => void }) {
  return (
    <>
      <Field label={t('Administrator groups')} hint={t('One a line: the group’s name, or its full DN (LDAP).')}>
        <Lines value={c.adminGroups} placeholder="mv-admins" onChange={(v) => set({ adminGroups: v })} />
      </Field>
      <Field label={t('Designer groups')}>
        <Lines value={c.designerGroups} placeholder="mv-designers" onChange={(v) => set({ designerGroups: v })} />
      </Field>
      <Field label={t('Operator groups')}>
        <Lines value={c.operatorGroups} placeholder="mv-operateurs" onChange={(v) => set({ operatorGroups: v })} />
      </Field>
      <Field label={t('The others')} hint={t('The role of someone in none of these groups.')}>
        <select value={c.defaultRole} onChange={(e) => set({ defaultRole: e.target.value })}>
          {ROLES.map(([k, l]) => (
            <option key={k} value={k}>
              {t(l)}
            </option>
          ))}
        </select>
      </Field>
    </>
  );
}

export function Security({ cfg, edit }: { cfg: ServerConfig; edit: Edit }) {
  const tls = cfg.tls ?? { enabled: false, cert: '', key: '' };
  const ldap = { ...LDAP, ...cfg.ldap };
  const oidc = { ...OIDC, ...cfg.oidc };
  const setTls = (p: Partial<typeof tls>) => edit((c) => void (c.tls = { ...tls, ...p }));
  const setLdap = (p: Partial<LdapConfig>) => edit((c) => void (c.ldap = { ...ldap, ...p }));
  const setOidc = (p: Partial<OidcConfig>) => edit((c) => void (c.oidc = { ...oidc, ...p }));
  const callback = `${location.origin.replace(/^http:/, tls.enabled ? 'https:' : 'http:')}/api/auth/oidc/callback`;
  return (
    <>
      <Card icon={<Lock size={16} />} title="HTTPS">
        <div className="set-grid">
          <Field label={t('Encrypted interface')} hint={t('The web interface, the API and the output pages over HTTPS on the same port. Applied when the settings are saved (the engine restarts).')}>
            <label className="check">
              <input type="checkbox" checked={tls.enabled} onChange={(e) => setTls({ enabled: e.target.checked })} />
              {t('HTTPS')}
            </label>
          </Field>
          <Field label={t('Certificate (PEM)')} hint={t('Path of the certificate (chain) on the server. Both empty: a certificate made for this machine, which browsers ask to trust once.')}>
            <input value={tls.cert} placeholder="/etc/ssl/strata/fullchain.pem" spellCheck={false} onChange={(e) => setTls({ cert: e.target.value })} />
          </Field>
          <Field label={t('Private key (PEM)')}>
            <input value={tls.key} placeholder="/etc/ssl/strata/privkey.pem" spellCheck={false} onChange={(e) => setTls({ key: e.target.value })} />
          </Field>
        </div>
        <p className="muted">{t('Plain HTTP from another machine is sent to HTTPS; the machine itself (127.0.0.1) stays reachable over HTTP for the menu bar app.')}</p>
      </Card>

      <Card icon={<Users size={16} />} title={t('Directory (LDAP / Active Directory)')}>
        <div className="set-grid">
          <Field label={t('Sign in with the directory')}>
            <label className="check">
              <input type="checkbox" checked={ldap.enabled} onChange={(e) => setLdap({ enabled: e.target.checked })} />
              {t('On')}
            </label>
          </Field>
          <Field label={t('Server')} hint="ldap://dc.exemple.fr · ldaps://dc.exemple.fr:636">
            <input value={ldap.url} placeholder="ldaps://dc.exemple.fr" spellCheck={false} onChange={(e) => setLdap({ url: e.target.value })} />
          </Field>
          <Field label={t('Security')}>
            <label className="check">
              <input type="checkbox" checked={ldap.starttls} onChange={(e) => setLdap({ starttls: e.target.checked })} />
              StartTLS
            </label>
            <label className="check">
              <input type="checkbox" checked={ldap.insecure} onChange={(e) => setLdap({ insecure: e.target.checked })} />
              {t('Do not check its certificate')}
            </label>
          </Field>
          <Field label={t('Search account (DN)')} hint={t('Empty: anonymous search.')}>
            <input value={ldap.bindDn} placeholder="CN=svc-strata,OU=Services,DC=exemple,DC=fr" spellCheck={false} onChange={(e) => setLdap({ bindDn: e.target.value })} />
          </Field>
          <Field label={t('Its password')} hint={t('Kept on the server; leave empty to keep the saved one.')}>
            <input type="password" value={ldap.bindPassword} placeholder="••••••••" autoComplete="new-password" onChange={(e) => setLdap({ bindPassword: e.target.value })} />
          </Field>
          <Field label={t('Base of the users (DN)')}>
            <input value={ldap.baseDn} placeholder="DC=exemple,DC=fr" spellCheck={false} onChange={(e) => setLdap({ baseDn: e.target.value })} />
          </Field>
          <Field label={t('User filter')} hint={t('{user}: the name typed. Active Directory: (sAMAccountName={user}); OpenLDAP: (uid={user}).')}>
            <input value={ldap.userFilter} spellCheck={false} onChange={(e) => setLdap({ userFilter: e.target.value })} />
          </Field>
          <Field label={t('Name attribute')}>
            <input value={ldap.nameAttribute} spellCheck={false} onChange={(e) => setLdap({ nameAttribute: e.target.value })} />
          </Field>
          <Groups c={ldap} set={setLdap} />
        </div>
        <p className="muted">
          {t(
            'An account of this server signs in first; the others are checked against the directory, their account made on first sign-in with the role of their groups (brought up to date at each sign-in).',
          )}
        </p>
      </Card>

      <Card icon={<KeyRound size={16} />} title={t('Single sign-on (OpenID Connect)')}>
        <div className="set-grid">
          <Field label={t('Sign-in button')}>
            <label className="check">
              <input type="checkbox" checked={oidc.enabled} onChange={(e) => setOidc({ enabled: e.target.checked })} />
              {t('On')}
            </label>
          </Field>
          <Field label={t('Words of the button')}>
            <input value={oidc.label} placeholder="Microsoft" onChange={(e) => setOidc({ label: e.target.value })} />
          </Field>
          <Field label={t('Issuer')} hint="https://login.microsoftonline.com/<tenant>/v2.0 · https://accounts.google.com · https://keycloak/realms/<realm>">
            <input value={oidc.issuer} spellCheck={false} onChange={(e) => setOidc({ issuer: e.target.value })} />
          </Field>
          <Field label={t('Client ID')}>
            <input value={oidc.clientId} spellCheck={false} onChange={(e) => setOidc({ clientId: e.target.value })} />
          </Field>
          <Field label={t('Client secret')} hint={t('Kept on the server; leave empty to keep the saved one.')}>
            <input type="password" value={oidc.clientSecret} placeholder="••••••••" autoComplete="new-password" onChange={(e) => setOidc({ clientSecret: e.target.value })} />
          </Field>
          <Field label={t('Name claim')}>
            <input value={oidc.usernameClaim} spellCheck={false} onChange={(e) => setOidc({ usernameClaim: e.target.value })} />
          </Field>
          <Field label={t('Groups claim')}>
            <input value={oidc.groupsClaim} spellCheck={false} onChange={(e) => setOidc({ groupsClaim: e.target.value })} />
          </Field>
          <Groups c={oidc} set={setOidc} />
        </div>
        <p className="muted">
          {t('Address to give the provider (redirect URI):')} <code>{callback}</code>
        </p>
      </Card>
    </>
  );
}

interface AuditEntry {
  at: number;
  user: string;
  ip: string;
  method: string;
  path: string;
  status: number;
  action: string;
}

/** Settings › Audit: who changed what, when, from where. */
export function AuditJournal() {
  const [list, setList] = useState<AuditEntry[] | null>(null);
  const [q, setQ] = useState('');
  useEffect(() => {
    const read = () =>
      api<AuditEntry[]>('GET', '/api/audit?limit=2000')
        .then(setList)
        .catch(() => setList([]));
    void read();
    const id = setInterval(read, 5000);
    return () => clearInterval(id);
  }, []);
  const shown = (list ?? []).filter((e) => !q || `${e.user} ${e.ip} ${e.action} ${e.path}`.toLowerCase().includes(q.toLowerCase()));
  return (
    <Card
      icon={<ShieldCheck size={16} />}
      title={t('Audit journal')}
      actions={
        <a className="btn small" href="/api/audit?csv=true&limit=50000" download>
          <Download size={13} /> CSV
        </a>
      }
    >
      <p className="muted">{t('Every change made through the interface or the API: sign-ins, settings, multiviews, alarms, users…')}</p>
      <div className="al-filters">
        <input placeholder={t('Filter…')} value={q} onChange={(e) => setQ(e.target.value)} />
        <span className="muted">{t('{n} lines', { n: shown.length })}</span>
      </div>
      <table className="al-table">
        <thead>
          <tr>
            <th>{t('Date')}</th>
            <th>{t('User')}</th>
            <th>{t('Address')}</th>
            <th>{t('Action')}</th>
            <th>{t('Result')}</th>
          </tr>
        </thead>
        <tbody>
          {shown.slice(0, 500).map((e, i) => (
            <tr key={`${e.at}-${i}`}>
              <td className="mono">{new Date(e.at).toLocaleString()}</td>
              <td>{e.user}</td>
              <td className="mono">{e.ip}</td>
              <td title={`${e.method} ${e.path}`}>{e.action}</td>
              <td className={e.status >= 400 ? 'al-on' : 'muted'}>{e.status}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

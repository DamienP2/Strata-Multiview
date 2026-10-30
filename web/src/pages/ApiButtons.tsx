import { Check, Copy, Download, Eye, KeyRound, Plug, Radio, Save, Settings2, Terminal, Webhook } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { t, tn } from '../i18n';
import type { ServerConfig, SystemInfo } from '../outputs/types';
import { api, can, useApp } from '../store';
import { promptDialog } from '../ui/dialogs';
import type { ApiKey } from './ApiKeys';
import { PageHead } from './SidePage';

/*
 * API › Bitfocus Buttons: the control API for surfaces (crates/cg-server/src/apiv1):
 * HTTP actions, the feedback WebSocket, OSC and TCP, with this server's
 * address and a key filled in, ready to copy.
 */

type Named = { id: string; name: string };
interface V1Channel {
  id: string;
  name: string;
  project: Named | null;
  next: Named | null;
  items: (Named & { index: number; zone: string; tally: string })[];
  zones: (Named & { layer: number })[];
}
interface V1Status {
  surfaces: number;
  control: { osc: { enabled: boolean; port: number }; tcp: { enabled: boolean; port: number } };
  channels: V1Channel[];
}
interface Control {
  oscEnabled: boolean;
  oscPort: number;
  tcpEnabled: boolean;
  tcpPort: number;
  allow: string[];
}
const DEFAULT_CONTROL: Control = { oscEnabled: false, oscPort: 9000, tcpEnabled: false, tcpPort: 9001, allow: [] };

/** A copy button. */
function CopyBtn({ text, label }: { text: string; label?: string }) {
  const toast = useApp((s) => s.toast);
  const [done, setDone] = useState(false);
  return (
    <button
      className="icon-btn ghost btn1-copy"
      title={label ?? t('Copy')}
      aria-label={label ?? t('Copy')}
      onClick={() =>
        navigator.clipboard
          ?.writeText(text)
          .then(() => {
            setDone(true);
            setTimeout(() => setDone(false), 1200);
          })
          .catch(() => toast(text))
      }
    >
      {done ? <Check size={13} /> : <Copy size={13} />}
    </button>
  );
}

/** A label and a value to copy. */
function Line({ label, value }: { label: string; value: string }) {
  return (
    <div className="btn1-line">
      <span className="btn1-label">{label}</span>
      <code className="btn1-value">{value}</code>
      <CopyBtn text={value} />
    </div>
  );
}

function Panel({ icon, title, extra, children }: { icon: React.ReactNode; title: string; extra?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="btn1-card">
      <header>
        {icon}
        <h2>{title}</h2>
        <span className="spacer" />
        {extra}
      </header>
      {children}
    </section>
  );
}

function OnOff({ on }: { on: boolean }) {
  return <span className={`btn1-state ${on ? 'on' : ''}`}>{on ? t('Enabled') : t('Disabled')}</span>;
}

export function ApiButtons({ keys, onCreateKey }: { keys: ApiKey[]; onCreateKey: () => void }) {
  const { auth, channels, toast } = useApp();
  const admin = can.admin(auth?.user);
  const [status, setStatus] = useState<V1Status | null>(null);
  const [channelId, setChannelId] = useState('');
  const [keyId, setKeyId] = useState('');
  const [secrets, setSecrets] = useState<Record<string, string>>({});

  const load = () =>
    api<V1Status>('GET', '/api/v1/status')
      .then(setStatus)
      .catch(() => {});
  useEffect(() => {
    void load();
    const h = setInterval(() => !document.hidden && void load(), 3000);
    return () => clearInterval(h);
  }, []);

  const channel = channels.find((c) => c.id === channelId) ?? channels[0];
  const ch = status?.channels.find((c) => c.id === channel?.id);
  const key = keys.find((k) => k.id === keyId) ?? keys[0];
  const token = key ? (secrets[key.id] ?? `${key.prefix}…`) : 'pcg_…';
  const host = location.hostname;
  const origin = location.origin;
  const base = `${origin}/api/v1/${encodeURIComponent(channel?.id ?? 'main')}`;
  const wsBase = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/v1/feedback`;
  const cid = channel?.id ?? 'main';
  // Real names for the examples: the loaded project's first zone and graphic.
  const zone = ch?.zones[0]?.id ?? 'lower';
  const item = ch?.items[0];

  const reveal = async () => {
    if (!key) return;
    const password = await promptDialog({ title: t('Show the key “{name}”', { name: key.name }), label: t('Your password'), password: true, confirm: t('Show') });
    if (!password) return;
    try {
      const r = await api<{ secret: string }>('POST', `/api/keys/${encodeURIComponent(key.id)}/reveal`, { password });
      setSecrets((s) => ({ ...s, [key.id]: r.secret }));
    } catch (e) {
      toast((e as Error).message, 'error');
    }
  };

  const actions: { path: string; params: string }[] = [
    { path: '/take/{item}', params: t('item: id, name or number · template: id or name') },
    { path: '/next', params: '?playlist=' },
    { path: '/cue/{item}', params: t('marks the graphic NEXT takes') },
    { path: '/continue/{zone}', params: t('zone: id or name') },
    { path: '/out/{zone}', params: t('zone: id or name') },
    { path: '/out', params: t('every zone') },
    { path: '/clear/{zone}', params: t('cut, without the out animation') },
    { path: '/update/{zone}', params: '{"key": "value", …}' },
    { path: '/value/{zone}/{key}', params: t('body: the value (text)') },
    { path: '/undo', params: t('the last action of the channel') },
    { path: '/trigger/{name}', params: t('pauses waiting for this trigger') },
    { path: '/widget/{zone}/{widget}/{action}', params: 'start · pause · toggle · reset · restart' },
    { path: '/row/{zone}/{row}', params: 'next · prev · first · last · 1…n' },
  ];
  const feedback: { key: string; what: string }[] = [
    { key: 'items[].tally', what: 'off · cue · pvw · pgm' },
    { key: 'zones[].item', what: t('graphic on air, phase, pause') },
    { key: 'zones[].countdown', what: t('seconds before resume / out / next') },
    { key: 'next', what: t('graphic in CUE') },
    { key: 'zones[].widgets', what: t('timed widgets running') },
    { key: 'locks', what: t('zone → operator') },
    { key: 'health', what: t('outputs, frames per second') },
    { key: 'surfaces', what: t('connected surfaces') },
  ];
  const osc = status?.control.osc ?? { enabled: false, port: 9000 };
  const tcp = status?.control.tcp ?? { enabled: false, port: 9001 };
  const itemRef = item ? String(item.index) : '1';
  const oscExamples = [`/strata/${cid}/take/${itemRef}`, `/strata/${cid}/next`, `/strata/${cid}/out/${zone}`, `/strata/${cid}/value/${zone}/name "Marie Curie"`, `/strata/${cid}/undo`];
  const tcpExamples = [`${cid} take ${itemRef}`, `${cid} next`, `${cid} out ${zone}`, `${cid} value ${zone} name "Marie Curie"`, 'status', `watch ${cid}`];
  const curl = `curl -X POST -H "Authorization: Bearer ${token}" ${base}/take/${itemRef}`;

  /** A Buttons "Generic HTTP" connection, with an action per graphic and zone. */
  const connection = useMemo(
    () => ({
      module: 'Generic HTTP',
      label: `Strata ${channel?.name ?? cid}`,
      baseUrl: base,
      headers: { Authorization: `Bearer ${token}` },
      feedback: `${wsBase}?key=${token}&channel=${encodeURIComponent(cid)}`,
      actions: [
        { name: 'NEXT', method: 'POST', url: `${base}/next` },
        { name: 'UNDO', method: 'POST', url: `${base}/undo` },
        { name: 'OUT ALL', method: 'POST', url: `${base}/out` },
        ...(ch?.items ?? []).map((i) => ({ name: `TAKE ${i.name}`, method: 'POST', url: `${base}/take/${i.index}` })),
        ...(ch?.zones ?? []).flatMap((z) => [
          { name: `CONTINUE ${z.name}`, method: 'POST', url: `${base}/continue/${encodeURIComponent(z.id)}` },
          { name: `OUT ${z.name}`, method: 'POST', url: `${base}/out/${encodeURIComponent(z.id)}` },
        ]),
      ],
    }),
    [base, token, ch, cid, channel?.name, wsBase],
  );
  const connectionJson = JSON.stringify(connection, null, 2);
  const download = () => {
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([connectionJson], { type: 'application/json' }));
    a.download = `strata-buttons-${cid}.json`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  };

  return (
    <div className="btn1-page">
      <PageHead
        title={
          <span className="btn1-title">
            Bitfocus Buttons <span className={`btn1-chip ${status?.surfaces ? 'on' : ''}`}>● {tn(status?.surfaces ?? 0, '{n} surface connected', '{n} surfaces connected')}</span>
          </span>
        }
      >
        <button className="btn" onClick={onCreateKey}>
          <KeyRound size={14} /> {t('Create key')}
        </button>
      </PageHead>

      <div className="btn1-bar">
        <label>
          <span className="btn1-label">{t('Channel')}</span>
          <select value={channel?.id ?? ''} onChange={(e) => setChannelId(e.target.value)} aria-label={t('Channel')}>
            {channels.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span className="btn1-label">{t('API key')}</span>
          <select value={key?.id ?? ''} onChange={(e) => setKeyId(e.target.value)} aria-label={t('API key')}>
            {!keys.length && <option value="">{t('No API key yet.')}</option>}
            {keys.map((k) => (
              <option key={k.id} value={k.id}>
                {k.name} · {k.prefix}…
              </option>
            ))}
          </select>
        </label>
        {admin && key && !secrets[key.id] && (
          <button className="btn small" onClick={() => void reveal()} title={t('Show the key')}>
            <Eye size={13} /> {t('Show the key')}
          </button>
        )}
        <span className="spacer" />
        {ch?.project && (
          <span className="btn1-project">
            {ch.project.name}
            {ch.next && (
              <>
                {' '}
                · <b>CUE</b> {ch.next.name}
              </>
            )}
          </span>
        )}
      </div>

      <div className="btn1-grid">
        <Panel icon={<Webhook size={15} />} title={t('Actions (HTTP)')}>
          <Line label={t('Base URL')} value={base} />
          <Line label={t('Header')} value={`Authorization: Bearer ${token}`} />
          <table className="btn1-table">
            <tbody>
              {actions.map((a) => (
                <tr key={a.path}>
                  <td className="btn1-method">POST</td>
                  <td>
                    <code>{a.path}</code>
                  </td>
                  <td className="btn1-muted">{a.params}</td>
                  <td className="btn1-c">
                    <CopyBtn text={base + a.path} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <Line label={t('Example')} value={curl} />
        </Panel>

        <Panel icon={<Radio size={15} />} title={t('Feedback (WebSocket / variables)')}>
          <Line label="WebSocket" value={`${wsBase}?key=${token}`} />
          <Line label="GET" value={`${origin}/api/v1/status`} />
          <Line label="GET" value={`${base}/status`} />
          <table className="btn1-table">
            <tbody>
              {feedback.map((f) => (
                <tr key={f.key}>
                  <td>
                    <code className="btn1-key">{f.key}</code>
                  </td>
                  <td className="btn1-muted">{f.what}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Panel>

        <Panel icon={<Plug size={15} />} title={t('OSC (UDP)')} extra={<OnOff on={osc.enabled} />}>
          <Line label={t('Address')} value={`${host}:${osc.port}`} />
          <table className="btn1-table">
            <tbody>
              {oscExamples.map((x) => (
                <tr key={x}>
                  <td>
                    <code>{x}</code>
                  </td>
                  <td className="btn1-c">
                    <CopyBtn text={x} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Panel>

        <Panel icon={<Terminal size={15} />} title={t('TCP (text)')} extra={<OnOff on={tcp.enabled} />}>
          <Line label={t('Address')} value={`${host}:${tcp.port}`} />
          <table className="btn1-table">
            <tbody>
              {tcpExamples.map((x) => (
                <tr key={x}>
                  <td>
                    <code>{x}</code>
                  </td>
                  <td className="btn1-muted">{x === 'status' ? t('one JSON line') : x.startsWith('watch') ? t('a JSON line at each change') : 'OK … / ERR …'}</td>
                  <td className="btn1-c">
                    <CopyBtn text={x} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Panel>

        <Panel
          icon={<Download size={15} />}
          title={t('Buttons connection')}
          extra={
            <>
              <CopyBtn text={connectionJson} />
              <button className="btn small" onClick={download}>
                <Download size={13} /> JSON
              </button>
            </>
          }
        >
          <div className="btn1-kv">
            <span className="btn1-label">{t('Module')}</span>
            <code>Generic HTTP</code>
            <span className="btn1-label">{t('Method')}</span>
            <code>POST</code>
            <span className="btn1-label">{t('Actions')}</span>
            <code>{connection.actions.length}</code>
          </div>
          <pre className="btn1-pre">{connectionJson}</pre>
        </Panel>

        {admin && <ControlSettings onSaved={() => setTimeout(() => void load(), 3000)} />}
      </div>
    </div>
  );
}

/** OSC and TCP on or off, their ports and the machines allowed (admins). */
function ControlSettings({ onSaved }: { onSaved: () => void }) {
  const toast = useApp((s) => s.toast);
  const [cfg, setCfg] = useState<Control | null>(null);
  const [initial, setInitial] = useState('');
  const [allow, setAllow] = useState('');
  const [saving, setSaving] = useState(false);
  const read = () =>
    api<SystemInfo>('GET', '/api/system').then((i) => {
      const c = { ...DEFAULT_CONTROL, ...(i.config as ServerConfig).control };
      setCfg(c);
      setAllow(c.allow.join(', '));
      setInitial(JSON.stringify(c));
    });
  useEffect(() => void read().catch(() => {}), []);
  if (!cfg) return null;
  const next: Control = { ...cfg, allow: allow.split(/[\s,;]+/).filter(Boolean) };
  const dirty = JSON.stringify(next) !== initial;
  const set = (patch: Partial<Control>) => setCfg({ ...cfg, ...patch });
  const save = async () => {
    setSaving(true);
    try {
      const i = await api<SystemInfo>('GET', '/api/system');
      await api('PUT', '/api/system/config', { ...i.config, control: next });
      toast(t('Settings saved — the engine is restarting'), 'info');
      setInitial(JSON.stringify(next));
      onSaved();
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setSaving(false);
    }
  };
  const port = (v: number, f: (n: number) => void, label: string) => <input type="number" min={1} max={65535} value={v} aria-label={label} onChange={(e) => f(+e.target.value)} />;
  return (
    <Panel
      icon={<Settings2 size={15} />}
      title={t('OSC and TCP')}
      extra={
        <button className="btn small primary" disabled={!dirty || saving} onClick={() => void save()}>
          <Save size={13} /> {saving ? t('Applying…') : t('Save & apply')}
        </button>
      }
    >
      <table className="btn1-table btn1-settings">
        <tbody>
          <tr>
            <td>OSC</td>
            <td>
              <label className="switch">
                <input type="checkbox" checked={cfg.oscEnabled} onChange={(e) => set({ oscEnabled: e.target.checked })} aria-label={t('OSC (UDP)')} />
              </label>
            </td>
            <td className="btn1-label">{t('Port')}</td>
            <td>{port(cfg.oscPort, (n) => set({ oscPort: n }), t('OSC port'))}</td>
          </tr>
          <tr>
            <td>TCP</td>
            <td>
              <label className="switch">
                <input type="checkbox" checked={cfg.tcpEnabled} onChange={(e) => set({ tcpEnabled: e.target.checked })} aria-label={t('TCP (text)')} />
              </label>
            </td>
            <td className="btn1-label">{t('Port')}</td>
            <td>{port(cfg.tcpPort, (n) => set({ tcpPort: n }), t('TCP port'))}</td>
          </tr>
          <tr>
            <td colSpan={2} className="btn1-label">
              {t('Allowed addresses')}
            </td>
            <td colSpan={2}>
              <input className="btn1-allow" value={allow} placeholder={t('Any · e.g. 10.0.0.0/24, 192.168.1.20')} spellCheck={false} aria-label={t('Allowed addresses')} onChange={(e) => setAllow(e.target.value)} />
            </td>
          </tr>
        </tbody>
      </table>
    </Panel>
  );
}

// Broadcast › Tally & UMD: the switchers and tally controllers (TSL UMD 3.1 / 5.0, ATEM, GPI), what
// their displays say, and the routes that give each input its lamps and its UMD text.
// Mirrors crates/strata-mv-server/src/tally (settings in tally.json, applied at once).
import { Cable, Plus, Radio, Route as RouteIcon, Trash2, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useInputs } from '../engine/liveInputs';
import { t, tc } from '../i18n';
import { NavItem, NavSection, PageHead, SidePage } from '../pages/SidePage';
import { Card, Field, SaveBar } from '../settings/ui';
import { api, useApp } from '../store';
import { friendlyError } from '../ui/serverError';
import './tally.css';

type Lamp = 'program' | 'preview' | 'aux';
type Protocol = 'tsl31' | 'tsl5' | 'atem' | 'gpi';
type Transport = 'udp' | 'tcpListen' | 'tcpConnect';

export interface SourceConfig {
  id: string;
  name: string;
  enabled: boolean;
  protocol: Protocol;
  transport: Transport;
  port: number;
  host: string;
  screen?: number | null;
  lamps: [Lamp, Lamp, Lamp, Lamp];
}

export interface Route {
  input: string;
  source: string;
  index: number;
  lamp?: Lamp | null;
}

export interface TallyConfig {
  sources: SourceConfig[];
  routes: Route[];
}

interface Lamps {
  program: boolean;
  preview: boolean;
  aux: boolean;
}

interface SourceState {
  status: string;
  connected: boolean;
  lastAt?: number | null;
  displays: Record<string, Lamps & { text?: string }>;
}

interface InputTally {
  program?: boolean;
  preview?: boolean;
  aux?: boolean;
  umd?: string;
}

interface TallyView {
  config: TallyConfig;
  sources: Record<string, SourceState>;
  inputs: Record<string, InputTally>;
  overrides: Record<string, unknown>;
}

const PROTOCOLS: [Protocol, string][] = [
  ['tsl31', 'TSL UMD 3.1'],
  ['tsl5', 'TSL UMD 5.0'],
  ['atem', 'Blackmagic ATEM'],
  ['gpi', 'GPI'],
];
const protocolName = (p: Protocol) => PROTOCOLS.find(([k]) => k === p)?.[1] ?? p;
const LAMPS: Lamp[] = ['program', 'preview', 'aux'];
const lampName = (l: Lamp) => (l === 'program' ? t('On air') : l === 'preview' ? tc('tally', 'Next') : t('Aux'));

/** Server messages, in the interface language. */
function statusText(m: string): string {
  let x = /^Listening on (UDP|TCP) (\d+)$/.exec(m);
  if (x) return t('Listening on {proto} {port}', { proto: x[1], port: x[2] });
  x = /^Receiving from (.+)$/.exec(m);
  if (x) return t('Receiving from {host}', { host: x[1] });
  x = /^Connected (?:from|to) (.+)$/.exec(m);
  if (x) return t('Connected: {host}', { host: x[1] });
  x = /^Connecting to (.+)…$/.exec(m);
  if (x) return t('Connecting to {source}…', { source: x[1] });
  x = /^No answer from (.+) — trying again$/.exec(m);
  if (x) return t('No answer from {host} — trying again', { host: x[1] });
  if (m === 'Waiting for the GPI box') return t('Waiting for the GPI box');
  if (m === 'Starting…') return t('Starting…');
  if (m === 'Receiving') return t('Receiving');
  return m;
}

function Dots({ l }: { l: Partial<Lamps> }) {
  return (
    <span className="tl-dots" aria-hidden>
      <i className={`pgm ${l.program ? 'on' : ''}`} />
      <i className={`pvw ${l.preview ? 'on' : ''}`} />
      <i className={`aux ${l.aux ? 'on' : ''}`} />
    </span>
  );
}

const newId = (c: TallyConfig, base: string) => {
  let n = 1;
  while (c.sources.some((s) => s.id === `${base}-${n}`)) n++;
  return `${base}-${n}`;
};

export function TallyPage() {
  const toast = useApp((s) => s.toast);
  const inputs = useInputs();
  const [view, setView] = useState<TallyView | null>(null);
  const [cfg, setCfg] = useState<TallyConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [page, setPage] = useState<string>('routes');
  const [adding, setAdding] = useState(false);

  useEffect(() => {
    let gone = false;
    const read = () =>
      api<TallyView>('GET', '/api/tally')
        .then((v) => {
          if (gone) return;
          setView(v);
          setCfg((c) => c ?? structuredClone(v.config));
        })
        .catch(() => {});
    void read();
    const id = setInterval(read, 1000);
    return () => {
      gone = true;
      clearInterval(id);
    };
  }, []);

  const dirty = useMemo(() => !!(cfg && view && JSON.stringify(cfg) !== JSON.stringify(view.config)), [cfg, view]);
  if (!view || !cfg) return <div className="loading">{t('Loading…')}</div>;

  const edit = (fn: (c: TallyConfig) => void) =>
    setCfg((c) => {
      const n = structuredClone(c!);
      fn(n);
      return n;
    });
  const save = async () => {
    setSaving(true);
    try {
      await api('PUT', '/api/tally/config', cfg);
      const v = await api<TallyView>('GET', '/api/tally');
      setView(v);
      setCfg(structuredClone(v.config));
      toast(t('Tally settings applied'), 'info');
    } catch (e) {
      toast(friendlyError((e as Error).message), 'error');
    } finally {
      setSaving(false);
    }
  };
  const add = (p: Protocol) => {
    const id = newId(cfg, p);
    edit((c) =>
      c.sources.push({
        id,
        name: `${protocolName(p)} ${c.sources.filter((s) => s.protocol === p).length + 1}`,
        enabled: true,
        protocol: p,
        transport: 'udp',
        port: p === 'tsl31' ? 8900 : p === 'tsl5' ? 8901 : p === 'atem' ? 9910 : 0,
        host: '',
        lamps: ['program', 'preview', 'aux', 'aux'],
      }),
    );
    setAdding(false);
    setPage(id);
  };

  const source = cfg.sources.find((s) => s.id === page);
  const nav = (
    <>
      <NavSection
        actions={
          <button className="icon-btn ghost" title={t('Add a tally source')} onClick={() => setAdding((a) => !a)}>
            <Plus size={14} />
          </button>
        }
      >
        {t('Sources')}
      </NavSection>
      {adding && (
        <div className="tl-add">
          {PROTOCOLS.map(([p, label]) => (
            <button key={p} className="btn small" onClick={() => add(p)}>
              {label}
            </button>
          ))}
        </div>
      )}
      {cfg.sources.map((s) => {
        const st = view.sources[s.id];
        return <NavItem key={s.id} on={page === s.id} icon={<Cable size={15} />} label={s.name} status={!s.enabled ? 'off' : st?.connected ? 'ok' : 'warn'} onClick={() => setPage(s.id)} />;
      })}
      {!cfg.sources.length && !adding && <p className="muted tl-empty">{t('No tally source yet.')}</p>}
      <NavSection>{t('Inputs')}</NavSection>
      <NavItem on={page === 'routes'} icon={<RouteIcon size={15} />} label={t('Routing')} count={cfg.routes.length} onClick={() => setPage('routes')} />
    </>
  );

  let body: React.ReactNode;
  if (source) {
    const st = view.sources[source.id];
    const setS = (fn: (s: SourceConfig) => void) => edit((c) => fn(c.sources.find((x) => x.id === source.id)!));
    const tsl = source.protocol === 'tsl31' || source.protocol === 'tsl5';
    const displays = Object.entries(st?.displays ?? {}).sort(([a], [b]) => +a - +b);
    const routeTo = (index: number, input: string) =>
      edit((c) => {
        c.routes = c.routes.filter((r) => !(r.source === source.id && r.index === index));
        if (input) c.routes.push({ input, source: source.id, index });
      });
    const routedTo = (index: number) => cfg.routes.find((r) => r.source === source.id && r.index === index)?.input ?? '';
    const origin = location.origin;
    body = (
      <>
        <PageHead title={source.name} sub={`${protocolName(source.protocol)} · ${st ? statusText(st.status) : t('Not applied yet')}`}>
          <button
            className="btn danger"
            onClick={() => {
              edit((c) => {
                c.sources = c.sources.filter((s) => s.id !== source.id);
                c.routes = c.routes.filter((r) => r.source !== source.id);
              });
              setPage('routes');
            }}
          >
            <Trash2 size={14} /> {t('Delete')}
          </button>
        </PageHead>
        <Card icon={<Cable size={16} />} title={t('Connection')}>
          <div className="tl-grid">
            <Field label={t('Name')}>
              <input value={source.name} onChange={(e) => setS((s) => void (s.name = e.target.value))} />
            </Field>
            <Field label={t('Active')}>
              <select value={source.enabled ? 'on' : 'off'} onChange={(e) => setS((s) => void (s.enabled = e.target.value === 'on'))}>
                <option value="on">{t('On')}</option>
                <option value="off">{t('Off')}</option>
              </select>
            </Field>
            {tsl && (
              <Field label={t('Transport')} hint={t('UDP: the switcher sends to this server. TCP: it connects to this server (listen), or this server connects to it.')}>
                <select value={source.transport} onChange={(e) => setS((s) => void (s.transport = e.target.value as Transport))}>
                  <option value="udp">UDP</option>
                  <option value="tcpListen">{t('TCP · this server listens')}</option>
                  <option value="tcpConnect">{t('TCP · connect to the switcher')}</option>
                </select>
              </Field>
            )}
            {(source.protocol === 'atem' || (tsl && source.transport === 'tcpConnect')) && (
              <Field label={t('Switcher address')}>
                <input value={source.host} placeholder="192.168.10.240" onChange={(e) => setS((s) => void (s.host = e.target.value))} />
              </Field>
            )}
            {(tsl || source.protocol === 'atem') && (
              <Field label={t('Port')}>
                <input type="number" min={1} max={65535} value={source.port || ''} onChange={(e) => setS((s) => void (s.port = +e.target.value || 0))} />
              </Field>
            )}
            {source.protocol === 'tsl5' && (
              <Field label={t('Screen')} hint={t('Only the displays of this screen (empty: every screen).')}>
                <input type="number" min={0} max={65534} value={source.screen ?? ''} onChange={(e) => setS((s) => void (s.screen = e.target.value === '' ? null : +e.target.value))} />
              </Field>
            )}
          </div>
          {source.protocol === 'tsl31' && (
            <div className="tl-grid">
              {[0, 1, 2, 3].map((i) => (
                <Field key={i} label={t('Tally {n}', { n: i + 1 })}>
                  <select value={source.lamps[i]} onChange={(e) => setS((s) => void (s.lamps[i] = e.target.value as Lamp))}>
                    {LAMPS.map((l) => (
                      <option key={l} value={l}>
                        {lampName(l)}
                      </option>
                    ))}
                  </select>
                </Field>
              ))}
            </div>
          )}
          {source.protocol === 'tsl5' && <p className="muted">{t('Red lights the on-air lamp, green the next one, amber the aux one.')}</p>}
          {source.protocol === 'atem' && <p className="muted">{t('Inputs are numbered as on the switcher (1 = Camera 1); their long names become the UMD texts.')}</p>}
        </Card>
        {source.protocol === 'gpi' && (
          <Card icon={<Radio size={16} />} title={t('GPI box')}>
            <p className="muted">{t('Set the GPI box to call these addresses when a contact closes or opens (an API key of Users › API keys, operator rights):')}</p>
            <code className="tl-url">{`${origin}/api/tally/gpi/${source.id}/<pin>/on?key=smv_…`}</code>
            <code className="tl-url">{`${origin}/api/tally/gpi/${source.id}/<pin>/off?key=smv_…`}</code>
          </Card>
        )}
        <Card icon={<Radio size={16} />} title={t('Displays received')}>
          {!displays.length ? (
            <p className="muted">{source.protocol === 'gpi' ? t('No contact reported yet.') : t('Nothing received yet.')}</p>
          ) : (
            <table className="tl-table">
              <thead>
                <tr>
                  <th>{source.protocol === 'gpi' ? t('Pin') : source.protocol === 'atem' ? t('Input') : t('Index')}</th>
                  <th>{t('Tally')}</th>
                  <th>UMD</th>
                  <th>{t('Shown by')}</th>
                </tr>
              </thead>
              <tbody>
                {displays.map(([i, d]) => (
                  <tr key={i}>
                    <td className="mono">{i}</td>
                    <td>
                      <Dots l={d} />
                    </td>
                    <td>{d.text || <span className="muted">—</span>}</td>
                    <td>
                      <select value={routedTo(+i)} onChange={(e) => routeTo(+i, e.target.value)}>
                        <option value="">{t('— none —')}</option>
                        {inputs.map((x) => (
                          <option key={x.id} value={x.id}>
                            {x.name}
                          </option>
                        ))}
                      </select>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </>
    );
  } else {
    body = (
      <>
        <PageHead title={t('Tally & UMD')} sub={t('Each input takes the lamps and the name of the switcher displays routed to it. The API can also set them: PUT /api/tally/inputs/<input>.')} />
        <Card icon={<RouteIcon size={16} />} title={t('Routing')}>
          {!inputs.length && <p className="muted">{t('No live input yet: add one in Broadcast › Video inputs.')}</p>}
          {inputs.length > 0 && (
            <table className="tl-table">
              <thead>
                <tr>
                  <th>{t('Input')}</th>
                  <th>{t('Now')}</th>
                  <th>{t('Routes')}</th>
                </tr>
              </thead>
              <tbody>
                {inputs.map((x) => {
                  const now = view.inputs[x.id] ?? {};
                  const routes = cfg.routes.map((r, i) => [r, i] as const).filter(([r]) => r.input === x.id);
                  return (
                    <tr key={x.id}>
                      <td>
                        <strong>{x.name}</strong>
                      </td>
                      <td>
                        <Dots l={now} /> <span className="tl-umd">{now.umd || ''}</span>
                      </td>
                      <td>
                        <div className="tl-routes">
                          {routes.map(([r, i]) => {
                            const src = cfg.sources.find((s) => s.id === r.source);
                            return (
                              <span key={i} className="tl-route">
                                <select value={r.source} onChange={(e) => edit((c) => void (c.routes[i].source = e.target.value))}>
                                  {cfg.sources.map((s) => (
                                    <option key={s.id} value={s.id}>
                                      {s.name}
                                    </option>
                                  ))}
                                </select>
                                <input type="number" min={0} title={src?.protocol === 'gpi' ? t('Pin') : src?.protocol === 'atem' ? t('Input') : t('Index')} value={r.index} onChange={(e) => edit((c) => void (c.routes[i].index = Math.max(0, +e.target.value || 0)))} />
                                {src?.protocol === 'gpi' && (
                                  <select value={r.lamp ?? 'program'} onChange={(e) => edit((c) => void (c.routes[i].lamp = e.target.value as Lamp))}>
                                    {LAMPS.map((l) => (
                                      <option key={l} value={l}>
                                        {lampName(l)}
                                      </option>
                                    ))}
                                  </select>
                                )}
                                <button className="icon-btn ghost" title={t('Remove')} onClick={() => edit((c) => void c.routes.splice(i, 1))}>
                                  <X size={13} />
                                </button>
                              </span>
                            );
                          })}
                          {cfg.sources.length > 0 && (
                            <button className="btn small ghost" onClick={() => edit((c) => void c.routes.push({ input: x.id, source: c.sources[0].id, index: 1 }))}>
                              <Plus size={13} /> {t('Route')}
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </Card>
      </>
    );
  }

  return (
    <SidePage nav={nav} after={<SaveBar dirty={dirty} saving={saving} onSave={() => void save()} onDiscard={() => setCfg(structuredClone(view.config))} />}>
      {body}
    </SidePage>
  );
}

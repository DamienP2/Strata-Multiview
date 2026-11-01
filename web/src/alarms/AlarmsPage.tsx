// Broadcast › Alarms: the faults on now (acknowledged there), their journal (filtered, exported),
// statistics, and the settings — profiles of faults the inputs share (on or off, after how long, how
// grave, from what threshold), the profile of each input, the plant's faults, and what happens when
// a fault is raised (a sound on the pages, calls to other systems). Mirrors
// crates/strata-mv-server/src/alarms.rs (settings in alarms.json, applied at once).
import { BellOff, BellRing, Check, CheckCheck, Download, History, ListChecks, Plus, Server, SlidersHorizontal, Trash2, Webhook as WebhookIcon, BarChart3, Copy, RotateCcw, Send } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useAlarmSound } from '../engine/alarmSound';
import { alarmText, useInputs, useSystem, type AlarmKind, type JournalEntry, type Severity } from '../engine/liveInputs';
import { t } from '../i18n';
import { NavItem, NavSection, PageHead, SidePage } from '../pages/SidePage';
import { Card, Field, SaveBar } from '../settings/ui';
import { api, can, useApp } from '../store';
import { friendlyError } from '../ui/serverError';
import { COLOR_SPACES, DEFAULT_PROFILE, INPUT_ALARMS, KIND_INFO, SYSTEM_ALARMS, defaultRule, ruleOf, thresholdUnit, type AlarmSettings, type Profile, type Rule, type Webhook } from './model';
import './alarms.css';

interface AlarmsView {
  settings: AlarmSettings;
  journal: JournalEntry[];
}

const kindName = (k: AlarmKind) => t(KIND_INFO[k][0]);
const sevName = (s: Severity) => (s === 'critical' ? t('Critical') : t('Warning'));

/** "4 s", "2 min 05 s", "1 h 03 min". */
export function duration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${String(s % 60).padStart(2, '0')} s`;
  const h = Math.floor(m / 60);
  return `${h} h ${String(m % 60).padStart(2, '0')} min`;
}

const when = (ms: number) =>
  new Date(ms).toLocaleString(undefined, {
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });

function Sev({ s }: { s: Severity }) {
  return <span className={`al-sev ${s}`}>{sevName(s)}</span>;
}

/** Re-drawn every second (the durations of the faults on now). */
function useSecond() {
  const [, set] = useState(0);
  useEffect(() => {
    const id = setInterval(() => set((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);
}

const newId = (base: string, taken: string[]) => {
  let n = 1;
  while (taken.includes(`${base}${n}`)) n++;
  return `${base}${n}`;
};

export function AlarmsPage() {
  const toast = useApp((s) => s.toast);
  const user = useApp((s) => s.auth?.user);
  const admin = can.admin(user);
  const operator = can.operate(user);
  const inputs = useInputs();
  const live = useSystem();
  const [view, setView] = useState<AlarmsView | null>(null);
  const [cfg, setCfg] = useState<AlarmSettings | null>(null);
  const [saving, setSaving] = useState(false);
  const [page, setPage] = useState('now');
  useAlarmSound();

  useEffect(() => {
    api<AlarmsView>('GET', '/api/alarms')
      .then((v) => {
        setView(v);
        setCfg(structuredClone(v.settings));
      })
      .catch((e) => toast(friendlyError((e as Error).message), 'error'));
  }, [toast]);

  // The journal as it changes (signals), else as read.
  const journal = live.journal.length || !view ? live.journal : view.journal;
  const open = journal.filter((e) => e.ended == null);
  const dirty = useMemo(() => !!(cfg && view && JSON.stringify(cfg) !== JSON.stringify(view.settings)), [cfg, view]);
  if (!view || !cfg) return <div className="loading">{t('Loading…')}</div>;

  const edit = (fn: (c: AlarmSettings) => void) =>
    setCfg((c) => {
      const n = structuredClone(c!);
      fn(n);
      return n;
    });
  const save = async () => {
    setSaving(true);
    try {
      await api('PUT', '/api/alarms/settings', cfg);
      const v = await api<AlarmsView>('GET', '/api/alarms');
      setView(v);
      setCfg(structuredClone(v.settings));
      toast(t('Alarm settings applied'), 'info');
    } catch (e) {
      toast(friendlyError((e as Error).message), 'error');
    } finally {
      setSaving(false);
    }
  };
  const ack = async (inputId?: string, kind?: AlarmKind) => {
    try {
      await api('POST', '/api/alarms/ack', { inputId, kind });
    } catch (e) {
      toast(friendlyError((e as Error).message), 'error');
    }
  };

  const unacked = open.filter((e) => !e.acked).length;
  const nav = (
    <>
      <NavSection>{t('Supervision')}</NavSection>
      <NavItem
        on={page === 'now'}
        icon={<BellRing size={15} />}
        label={t('On now')}
        count={open.length}
        status={open.some((e) => e.severity === 'critical' && !e.acked) ? 'warn' : open.length ? 'off' : 'ok'}
        onClick={() => setPage('now')}
      />
      <NavItem on={page === 'journal'} icon={<History size={15} />} label={t('Journal')} count={journal.length} onClick={() => setPage('journal')} />
      <NavItem on={page === 'stats'} icon={<BarChart3 size={15} />} label={t('Statistics')} onClick={() => setPage('stats')} />
      {admin && (
        <>
          <NavSection
            actions={
              <button
                className="icon-btn ghost"
                title={t('Add a profile')}
                onClick={() => {
                  const id = newId(
                    'profile',
                    cfg.profiles.map((p) => p.id),
                  );
                  edit(
                    (c) =>
                      void c.profiles.push({
                        id,
                        name: t('New profile'),
                        rules: {},
                      }),
                  );
                  setPage(`profile:${id}`);
                }}
              >
                <Plus size={14} />
              </button>
            }
          >
            {t('Profiles')}
          </NavSection>
          {cfg.profiles.map((p) => (
            <NavItem
              key={p.id}
              on={page === `profile:${p.id}`}
              icon={<ListChecks size={15} />}
              label={p.name}
              count={inputs.filter((i) => (cfg.inputs[i.id] ?? DEFAULT_PROFILE) === p.id).length}
              onClick={() => setPage(`profile:${p.id}`)}
            />
          ))}
          <NavSection>{t('Settings')}</NavSection>
          <NavItem on={page === 'sources'} icon={<SlidersHorizontal size={15} />} label={t('Profile of each input')} onClick={() => setPage('sources')} />
          <NavItem on={page === 'system'} icon={<Server size={15} />} label={t('Plant')} onClick={() => setPage('system')} />
          <NavItem on={page === 'actions'} icon={<WebhookIcon size={15} />} label={t('Actions')} count={cfg.webhooks.length || undefined} onClick={() => setPage('actions')} />
        </>
      )}
    </>
  );

  let body: React.ReactNode;
  const profile = page.startsWith('profile:') ? cfg.profiles.find((p) => `profile:${p.id}` === page) : undefined;
  if (page === 'now') {
    body = (
      <>
        <PageHead title={t('Alarms on now')} sub={unacked ? t('{n} not acknowledged', { n: unacked }) : open.length ? t('All acknowledged') : t('No fault.')}>
          {operator && unacked > 0 && (
            <button className="btn primary" onClick={() => void ack()}>
              <CheckCheck size={14} /> {t('Acknowledge all')}
            </button>
          )}
        </PageHead>
        <NowList open={open} onAck={operator ? ack : undefined} />
      </>
    );
  } else if (page === 'journal') {
    body = <JournalView journal={journal} />;
  } else if (page === 'stats') {
    body = <StatsView journal={journal} />;
  } else if (profile) {
    const setP = (fn: (p: Profile) => void) => edit((c) => fn(c.profiles.find((x) => x.id === profile.id)!));
    const used = inputs.filter((i) => (cfg.inputs[i.id] ?? DEFAULT_PROFILE) === profile.id);
    body = (
      <>
        <PageHead
          title={profile.name}
          sub={
            used.length
              ? t('Used by: {list}', {
                  list: used.map((i) => i.name).join(', '),
                })
              : t('Used by no input.')
          }
        >
          <button
            className="btn"
            title={t('A copy of this profile')}
            onClick={() => {
              const id = newId(
                'profile',
                cfg.profiles.map((p) => p.id),
              );
              edit(
                (c) =>
                  void c.profiles.push({
                    ...structuredClone(profile),
                    id,
                    name: `${profile.name} (2)`,
                  }),
              );
              setPage(`profile:${id}`);
            }}
          >
            <Copy size={14} /> {t('Duplicate')}
          </button>
          {profile.id !== DEFAULT_PROFILE && (
            <button
              className="btn danger"
              onClick={() => {
                edit((c) => {
                  c.profiles = c.profiles.filter((p) => p.id !== profile.id);
                  for (const [k, v] of Object.entries(c.inputs)) if (v === profile.id) delete c.inputs[k];
                });
                setPage('now');
              }}
            >
              <Trash2 size={14} /> {t('Delete')}
            </button>
          )}
        </PageHead>
        <Card icon={<ListChecks size={16} />} title={t('Profile')}>
          <div className="al-grid">
            <Field label={t('Name')}>
              <input value={profile.name} onChange={(e) => setP((p) => void (p.name = e.target.value))} />
            </Field>
          </div>
          {profile.id === DEFAULT_PROFILE && <p className="muted">{t('The profile of every input not given another.')}</p>}
        </Card>
        <Card
          icon={<BellRing size={16} />}
          title={t('Faults of the inputs')}
          actions={
            <button className="btn small ghost" onClick={() => setP((p) => void (p.rules = {}))}>
              <RotateCcw size={13} /> {t('Defaults')}
            </button>
          }
        >
          <RulesTable kinds={INPUT_ALARMS} get={(k) => profile.rules[k] ?? defaultRule(k)} set={(k, r) => setP((p) => void (p.rules[k] = r))} />
        </Card>
      </>
    );
  } else if (page === 'sources') {
    body = (
      <>
        <PageHead title={t('Profile of each input')} sub={t('Inputs share a profile: cameras, playout servers, a feed showing colour bars between programmes…')} />
        <Card icon={<SlidersHorizontal size={16} />} title={t('Inputs')}>
          {!inputs.length && <p className="muted">{t('No live input yet: add one in Broadcast › Video inputs.')}</p>}
          {inputs.length > 0 && (
            <table className="al-table">
              <thead>
                <tr>
                  <th>{t('Input')}</th>
                  <th>{t('Profile')}</th>
                  <th>{t('Faults watched')}</th>
                </tr>
              </thead>
              <tbody>
                {inputs.map((i) => (
                  <tr key={i.id}>
                    <td>
                      <strong>{i.name}</strong>
                    </td>
                    <td>
                      <select
                        value={cfg.inputs[i.id] ?? DEFAULT_PROFILE}
                        onChange={(e) =>
                          edit((c) => {
                            if (e.target.value === DEFAULT_PROFILE) delete c.inputs[i.id];
                            else c.inputs[i.id] = e.target.value;
                          })
                        }
                      >
                        {cfg.profiles.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.name}
                          </option>
                        ))}
                      </select>
                    </td>
                    <td className="muted">
                      {INPUT_ALARMS.filter((k) => ruleOf(cfg, i.id, k).enabled).length} / {INPUT_ALARMS.length}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </>
    );
  } else if (page === 'system') {
    body = (
      <>
        <PageHead title={t('Faults of the plant')} sub={t('PTP, NTP, processors, memory, outputs.')} />
        <Card icon={<Server size={16} />} title={t('Plant')}>
          <RulesTable kinds={SYSTEM_ALARMS} get={(k) => cfg.system[k] ?? defaultRule(k)} set={(k, r) => edit((c) => void (c.system[k] = r))} />
        </Card>
      </>
    );
  } else {
    body = <ActionsView cfg={cfg} edit={edit} />;
  }

  return (
    <SidePage nav={nav} after={admin ? <SaveBar dirty={dirty} saving={saving} onSave={() => void save()} onDiscard={() => setCfg(structuredClone(view.settings))} /> : undefined}>
      {body}
    </SidePage>
  );
}

/** The faults on now, the worst and newest first. */
function NowList({ open, onAck }: { open: JournalEntry[]; onAck?: (inputId?: string, kind?: AlarmKind) => void }) {
  useSecond();
  const now = Date.now();
  const list = [...open].sort((a, b) => (a.severity === b.severity ? b.at - a.at : a.severity === 'critical' ? -1 : 1));
  if (!list.length)
    return (
      <div className="al-calm">
        <BellOff size={28} />
        <p>{t('No fault on the inputs or the plant.')}</p>
      </div>
    );
  return (
    <div className="al-now">
      {list.map((e) => (
        <div key={`${e.inputId}-${e.kind}-${e.at}`} className={`al-item ${e.severity} ${e.acked ? 'acked' : ''}`}>
          <span className="al-bar" />
          <div className="al-what">
            <strong>{alarmText(e.kind, 'fr')}</strong>
            <span>{e.input || '—'}</span>
          </div>
          <Sev s={e.severity} />
          <div className="al-time">
            <span className="mono">{duration(now - e.at)}</span>
            <small className="muted">{t('since {time}', { time: when(e.at) })}</small>
          </div>
          <div className="al-ack">
            {e.acked ? (
              <span className="muted" title={when(e.acked)}>
                <Check size={13} /> {e.by || t('Acknowledged')}
              </span>
            ) : (
              onAck && (
                <button className="btn small" onClick={() => onAck(e.inputId, e.kind)}>
                  <Check size={13} /> {t('Acknowledge')}
                </button>
              )
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

const PERIODS: [number, string][] = [
  [3600e3, 'Last hour'],
  [86400e3, 'Last 24 hours'],
  [7 * 86400e3, 'Last 7 days'],
  [0, 'All'],
];

function JournalView({ journal }: { journal: JournalEntry[] }) {
  const [text, setText] = useState('');
  const [kind, setKind] = useState<AlarmKind | ''>('');
  const [sev, setSev] = useState<Severity | ''>('');
  const [period, setPeriod] = useState(86400e3);
  const now = Date.now();
  const kinds = [...INPUT_ALARMS, ...SYSTEM_ALARMS];
  const list = journal.filter((e) => (!period || now - e.at <= period) && (!kind || e.kind === kind) && (!sev || e.severity === sev) && (!text || e.input.toLowerCase().includes(text.toLowerCase())));
  const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
  return (
    <>
      <PageHead title={t('Journal of the alarms')} sub={t('The last thousand faults, kept when the server restarts.')}>
        <a className="btn" href={`/api/alarms/journal.csv?tz=${encodeURIComponent(tz)}`} download>
          <Download size={14} /> {t('Export (CSV)')}
        </a>
      </PageHead>
      <div className="al-filters">
        <input placeholder={t('Source…')} value={text} onChange={(e) => setText(e.target.value)} />
        <select value={kind} onChange={(e) => setKind(e.target.value as AlarmKind | '')}>
          <option value="">{t('Every fault')}</option>
          {kinds.map((k) => (
            <option key={k} value={k}>
              {kindName(k)}
            </option>
          ))}
        </select>
        <select value={sev} onChange={(e) => setSev(e.target.value as Severity | '')}>
          <option value="">{t('Every severity')}</option>
          <option value="critical">{t('Critical')}</option>
          <option value="warning">{t('Warning')}</option>
        </select>
        <select value={period} onChange={(e) => setPeriod(+e.target.value)}>
          {PERIODS.map(([ms, l]) => (
            <option key={ms} value={ms}>
              {t(l)}
            </option>
          ))}
        </select>
        <span className="muted">{t('{n} lines', { n: list.length })}</span>
      </div>
      <table className="al-table">
        <thead>
          <tr>
            <th>{t('Began')}</th>
            <th>{t('Lasted')}</th>
            <th>{t('Source')}</th>
            <th>{t('Fault')}</th>
            <th>{t('Severity')}</th>
            <th>{t('Acknowledged')}</th>
          </tr>
        </thead>
        <tbody>
          {list.slice(0, 500).map((e) => (
            <tr key={`${e.inputId}-${e.kind}-${e.at}`} className={e.ended == null ? 'al-open' : ''}>
              <td className="mono">{when(e.at)}</td>
              <td className="mono">{e.ended == null ? <span className="al-on">{t('on now')}</span> : duration(e.ended - e.at)}</td>
              <td>{e.input || '—'}</td>
              <td>{kindName(e.kind)}</td>
              <td>
                <Sev s={e.severity} />
              </td>
              <td className="muted">{e.acked ? `${e.by || '—'} · ${when(e.acked)}` : '—'}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {!list.length && <p className="muted al-empty">{t('Nothing in this period.')}</p>}
    </>
  );
}

function StatsView({ journal }: { journal: JournalEntry[] }) {
  const [period, setPeriod] = useState(86400e3);
  const now = Date.now();
  const list = journal.filter((e) => !period || now - e.at <= period);
  const by = <K extends string>(key: (e: JournalEntry) => K) => {
    const m = new Map<K, { n: number; ms: number; critical: number }>();
    for (const e of list) {
      const k = key(e);
      const v = m.get(k) ?? { n: 0, ms: 0, critical: 0 };
      v.n++;
      v.ms += (e.ended ?? now) - e.at;
      if (e.severity === 'critical') v.critical++;
      m.set(k, v);
    }
    return [...m.entries()].sort((a, b) => b[1].n - a[1].n);
  };
  const kinds = by((e) => e.kind);
  const sources = by((e) => e.input || '—');
  const top = Math.max(1, ...kinds.map(([, v]) => v.n), ...sources.map(([, v]) => v.n));
  const total = list.reduce((a, e) => a + ((e.ended ?? now) - e.at), 0);
  const Bars = ({ rows, name }: { rows: [string, { n: number; ms: number; critical: number }][]; name: (k: string) => string }) => (
    <table className="al-table al-stats">
      <tbody>
        {rows.map(([k, v]) => (
          <tr key={k}>
            <td className="al-stat-name">{name(k)}</td>
            <td className="al-stat-bar">
              <span style={{ width: `${(v.n / top) * 100}%` }} className={v.critical ? 'critical' : 'warning'} />
            </td>
            <td className="mono">{v.n}</td>
            <td className="mono muted">{duration(v.ms)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
  return (
    <>
      <PageHead
        title={t('Statistics')}
        sub={t('{n} faults, {time} in all', {
          n: list.length,
          time: duration(total),
        })}
      >
        <select value={period} onChange={(e) => setPeriod(+e.target.value)}>
          {PERIODS.map(([ms, l]) => (
            <option key={ms} value={ms}>
              {t(l)}
            </option>
          ))}
        </select>
      </PageHead>
      <Card icon={<BellRing size={16} />} title={t('By fault')}>
        {kinds.length ? <Bars rows={kinds} name={(k) => kindName(k as AlarmKind)} /> : <p className="muted">{t('Nothing in this period.')}</p>}
      </Card>
      <Card icon={<SlidersHorizontal size={16} />} title={t('By source')}>
        {sources.length ? <Bars rows={sources} name={(k) => k} /> : <p className="muted">{t('Nothing in this period.')}</p>}
      </Card>
    </>
  );
}

/** Each fault: on or off, after how long, how grave, from what threshold. */
function RulesTable({ kinds, get, set }: { kinds: AlarmKind[]; get: (k: AlarmKind) => Rule; set: (k: AlarmKind, r: Rule) => void }) {
  return (
    <table className="al-table al-rules">
      <thead>
        <tr>
          <th>{t('Fault')}</th>
          <th>{t('On')}</th>
          <th>{t('After')}</th>
          <th>{t('Severity')}</th>
          <th>{t('Threshold / expected')}</th>
        </tr>
      </thead>
      <tbody>
        {kinds.map((k) => {
          const r = get(k);
          const unit = thresholdUnit(k);
          const put = (p: Partial<Rule>) => set(k, { ...r, ...p });
          return (
            <tr key={k} className={r.enabled ? '' : 'al-off'}>
              <td>
                <strong>{kindName(k)}</strong>
                <small className="muted">{t(KIND_INFO[k][1])}</small>
              </td>
              <td>
                <input type="checkbox" checked={r.enabled} onChange={(e) => put({ enabled: e.target.checked })} aria-label={t('On')} />
              </td>
              <td>
                <span className="al-num">
                  <input type="number" min={0} max={3600} step={1} value={r.after} disabled={!r.enabled} onChange={(e) => put({ after: Math.max(0, +e.target.value || 0) })} /> s
                </span>
              </td>
              <td>
                <select value={r.severity} disabled={!r.enabled} onChange={(e) => put({ severity: e.target.value as Severity })}>
                  <option value="critical">{t('Critical')}</option>
                  <option value="warning">{t('Warning')}</option>
                </select>
              </td>
              <td>
                {k === 'colorSpace' ? (
                  <select value={r.expect || 'bt709'} disabled={!r.enabled} onChange={(e) => put({ expect: e.target.value })} title={t('Expected')}>
                    {COLOR_SPACES.map(([id, name]) => (
                      <option key={id} value={id}>
                        {t(name)}
                      </option>
                    ))}
                  </select>
                ) : unit ? (
                  <span className="al-num">
                    <input type="number" step={k === 'phase' || k === 'clipping' ? 0.1 : 1} value={r.threshold} disabled={!r.enabled} onChange={(e) => put({ threshold: +e.target.value || 0 })} />{' '}
                    {unit}
                  </span>
                ) : (
                  <span className="muted">—</span>
                )}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function ActionsView({ cfg, edit }: { cfg: AlarmSettings; edit: (fn: (c: AlarmSettings) => void) => void }) {
  const toast = useApp((s) => s.toast);
  const setW = (id: string, fn: (w: Webhook) => void) => edit((c) => fn(c.webhooks.find((w) => w.id === id)!));
  const test = async (w: Webhook) => {
    try {
      const r = await api<{ ok: boolean; status?: number; error?: string }>('POST', '/api/alarms/test', { url: w.url, method: w.method });
      toast(r.ok ? t('Answered: {status}', { status: r.status ?? '' }) : friendlyError(r.error ?? ''), r.ok ? 'info' : 'error');
    } catch (e) {
      toast(friendlyError((e as Error).message), 'error');
    }
  };
  return (
    <>
      <PageHead title={t('Actions')} sub={t('What happens when a fault is raised, ends or is acknowledged.')} />
      <Card icon={<BellRing size={16} />} title={t('Sound')}>
        <div className="al-grid">
          <Field label={t('A sound on the pages')} hint={t('The output pages and this page sound when a fault this grave is raised.')}>
            <select value={cfg.sound ?? ''} onChange={(e) => edit((c) => void (c.sound = (e.target.value || null) as Severity | null))}>
              <option value="">{t('None')}</option>
              <option value="critical">{t('Critical faults')}</option>
              <option value="warning">{t('Every fault')}</option>
            </select>
          </Field>
        </div>
      </Card>
      <Card
        icon={<WebhookIcon size={16} />}
        title={t('Calls to other systems')}
        actions={
          <button
            className="btn small"
            onClick={() =>
              edit(
                (c) =>
                  void c.webhooks.push({
                    id: newId(
                      'hook',
                      c.webhooks.map((w) => w.id),
                    ),
                    name: t('Call {n}', { n: c.webhooks.length + 1 }),
                    enabled: true,
                    method: 'POST',
                    url: '',
                    minSeverity: 'warning',
                    onEnd: true,
                  }),
              )
            }
          >
            <Plus size={13} /> {t('Add')}
          </button>
        }
      >
        <p className="muted">
          {t(
            'An automation, a messenger, or a GPO box (most take a GET address: one per contact). In the address, {input}, {kind}, {severity}, {state} (raised, ended, acked) and {text} are replaced; POST sends them as JSON.',
          )}
        </p>
        {cfg.webhooks.map((w) => (
          <div key={w.id} className="al-hook">
            <div className="al-grid">
              <Field label={t('Name')}>
                <input value={w.name} onChange={(e) => setW(w.id, (x) => void (x.name = e.target.value))} />
              </Field>
              <Field label={t('Active')}>
                <select value={w.enabled ? 'on' : 'off'} onChange={(e) => setW(w.id, (x) => void (x.enabled = e.target.value === 'on'))}>
                  <option value="on">{t('On')}</option>
                  <option value="off">{t('Off')}</option>
                </select>
              </Field>
              <Field label={t('Method')}>
                <select value={w.method} onChange={(e) => setW(w.id, (x) => void (x.method = e.target.value as 'GET' | 'POST'))}>
                  <option value="POST">POST (JSON)</option>
                  <option value="GET">GET</option>
                </select>
              </Field>
              <Field label={t('Faults')}>
                <select value={w.minSeverity} onChange={(e) => setW(w.id, (x) => void (x.minSeverity = e.target.value as Severity))}>
                  <option value="critical">{t('Critical faults')}</option>
                  <option value="warning">{t('Every fault')}</option>
                </select>
              </Field>
              <Field label={t('When over too')}>
                <select value={w.onEnd ? 'on' : 'off'} onChange={(e) => setW(w.id, (x) => void (x.onEnd = e.target.value === 'on'))}>
                  <option value="on">{t('Yes')}</option>
                  <option value="off">{t('No')}</option>
                </select>
              </Field>
            </div>
            <div className="al-url">
              <input value={w.url} placeholder="http://192.168.10.50/relay/1?state={state}" spellCheck={false} onChange={(e) => setW(w.id, (x) => void (x.url = e.target.value))} />
              <button className="btn small" disabled={!w.url.trim()} onClick={() => void test(w)}>
                <Send size={13} /> {t('Try')}
              </button>
              <button className="icon-btn ghost" title={t('Remove')} onClick={() => edit((c) => void (c.webhooks = c.webhooks.filter((x) => x.id !== w.id)))}>
                <Trash2 size={14} />
              </button>
            </div>
          </div>
        ))}
        {!cfg.webhooks.length && <p className="muted">{t('No call yet.')}</p>}
      </Card>
    </>
  );
}

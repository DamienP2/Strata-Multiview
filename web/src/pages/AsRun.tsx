import { CalendarDays, CalendarRange, ChevronDown, ChevronRight, Clock, Cog, Database, Download, FileText, History, KeyRound, Layers, Newspaper, Radio, Search, Tv, X } from 'lucide-react';
import { Fragment, useEffect, useMemo, useState } from 'react';
import type { Zone } from '../engine/types';
import { locale, t, tc, tn } from '../i18n';
import { api, serverEvents, useApp } from '../store';
import { Dropdown } from '../ui/Dropdown';
import { NavItem, NavSection, PageHead, SidePage } from './SidePage';

/*
 * The as-run log ("Journal d'antenne"): every action on air, who did it and
 * with which values, read from the server per period and filtered here;
 * new entries come live over the WebSocket.
 */

type Kind = 'user' | 'api' | 'mos' | 'auto' | 'schedule' | 'data';

export interface AsRunEntry {
  at: number;
  channelId: string;
  channelName: string;
  projectId: string;
  projectName: string;
  action: string;
  label: string;
  zone: string;
  actor: { kind: Kind; id: string; name: string };
  values?: Record<string, [string | null, string]>;
  detail?: string;
}

type Period = 'hour' | 'today' | 'yesterday' | 'week' | 'custom';

/** Groups of actions, as filtered and shown (labels translated where shown). */
const GROUPS = ['take', 'out', 'update', 'continue', 'auto', 'undo', 'clear', 'trigger', 'row', 'widget'] as const;
type Group = (typeof GROUPS)[number];
const ALWAYS: Group[] = ['take', 'out', 'update', 'continue', 'auto', 'undo'];
const groupOf = (action: string): Group => (action.startsWith('auto-') ? 'auto' : (GROUPS as readonly string[]).includes(action) ? (action as Group) : 'widget');

function badge(g: Group): string {
  switch (g) {
    case 'take':
      return 'TAKE';
    case 'out':
      return 'OUT';
    case 'update':
      return 'UPDATE';
    case 'continue':
      return 'CONTINUE';
    case 'auto':
      return 'AUTO';
    case 'undo':
      return t('UNDONE');
    case 'clear':
      return tc('asrun', 'CLEAR');
    case 'trigger':
      return 'TRIGGER';
    case 'row':
      return 'ROW';
    case 'widget':
      return 'WIDGET';
  }
}

function chipLabel(g: Group): string {
  return g === 'undo' ? t('Undos') : badge(g);
}

function kindLabel(k: Kind): string {
  switch (k) {
    case 'user':
      return t('Operators');
    case 'api':
      return 'API';
    case 'mos':
      return 'MOS';
    case 'auto':
      return t('Engine');
    case 'schedule':
      return t('Scheduled');
    case 'data':
      return t('Data');
  }
}
const KINDS: Kind[] = ['user', 'api', 'mos', 'auto', 'schedule', 'data'];

/** The source of one entry. */
function kindOne(k: Kind): string {
  return k === 'user' ? t('Operator') : kindLabel(k);
}

function autoLabel(action: string): string {
  switch (action) {
    case 'auto-next':
      return t('auto next');
    case 'auto-out':
      return t('automatic out');
    case 'auto-continue':
      return t('automatic continue');
    default:
      return action;
  }
}

// ---------- time in the server's zone ----------

const DAY = 86_400_000;

function parts(tz: string, ms: number) {
  let f: Intl.DateTimeFormat;
  try {
    f = new Intl.DateTimeFormat('en-US', { timeZone: tz || undefined, hourCycle: 'h23', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    f = new Intl.DateTimeFormat('en-US', { hourCycle: 'h23', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }
  const p: Record<string, number> = {};
  for (const x of f.formatToParts(new Date(ms))) if (x.type !== 'literal') p[x.type] = Number(x.value);
  return { y: p.year, mo: p.month, d: p.day, h: p.hour % 24, mi: p.minute, s: p.second };
}

/** Wall clock of `tz` − UTC, ms, at `ms`. */
function offset(tz: string, ms: number): number {
  const p = parts(tz, ms);
  return Date.UTC(p.y, p.mo - 1, p.d, p.h, p.mi, p.s) - Math.floor(ms / 1000) * 1000;
}

/** A wall time of `tz` (ms as if UTC) → server time. */
const fromWall = (tz: string, wall: number) => wall - offset(tz, wall - offset(tz, wall));

/** Midnight of the day of `ms` in `tz`. */
function dayStart(tz: string, ms: number): number {
  const wall = ms + offset(tz, ms);
  return fromWall(tz, wall - (((wall % DAY) + DAY) % DAY));
}

const pad = (n: number, w = 2) => String(n).padStart(w, '0');

function clockText(tz: string, ms: number, withDate: boolean): string {
  const p = parts(tz, ms);
  const time = `${pad(p.h)}:${pad(p.mi)}:${pad(p.s)}.${pad(Math.floor((((ms % 1000) + 1000) % 1000) / 10))}`;
  return withDate ? `${pad(p.d)}/${pad(p.mo)} ${time}` : time;
}

function dateText(tz: string, ms: number): string {
  try {
    return new Intl.DateTimeFormat(locale(), { timeZone: tz || undefined, day: '2-digit', month: '2-digit', year: 'numeric' }).format(ms);
  } catch {
    return new Date(ms).toLocaleDateString(locale());
  }
}

/** "2026-10-24T20:00" (datetime-local) of `ms` in `tz`. */
function toInput(tz: string, ms: number): string {
  const p = parts(tz, ms);
  return `${p.y}-${pad(p.mo)}-${pad(p.d)}T${pad(p.h)}:${pad(p.mi)}`;
}
function fromInput(tz: string, v: string): number | null {
  const m = /^(\d{4})-(\d\d)-(\d\d)T(\d\d):(\d\d)/.exec(v);
  if (!m) return null;
  return fromWall(tz, Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5]));
}

/** [from, to] of a period (`to` null: up to now, live). */
function range(p: Period, tz: string, now: number, custom: { from: string; to: string }): [number, number | null] {
  switch (p) {
    case 'hour':
      return [now - 3_600_000, null];
    case 'today':
      return [dayStart(tz, now), null];
    case 'yesterday': {
      const today = dayStart(tz, now);
      return [dayStart(tz, today - DAY / 2), today - 1];
    }
    case 'week':
      return [dayStart(tz, now - 6 * DAY), null];
    case 'custom': {
      const from = fromInput(tz, custom.from) ?? dayStart(tz, now);
      const to = fromInput(tz, custom.to);
      // Up to now: live.
      return [from, to !== null && to + 59_999 < now ? to + 59_999 : null];
    }
  }
}

// ---------- page ----------

const actorKey = (e: AsRunEntry) => e.actor.id;

function matches(e: AsRunEntry, needle: string): boolean {
  if (!needle) return true;
  const hay = [e.label, e.action, e.zone, e.channelName, e.projectName, e.actor.name, e.detail ?? ''];
  if (hay.some((h) => h.toLowerCase().includes(needle))) return true;
  return Object.entries(e.values ?? {}).some(([k, [b, a]]) => k.toLowerCase().includes(needle) || a.toLowerCase().includes(needle) || (b ?? '').toLowerCase().includes(needle));
}

export function AsRun() {
  const { channels, timezone: tz, serverOffset, toast } = useApp();
  const [period, setPeriod] = useState<Period>('today');
  const [custom, setCustom] = useState(() => {
    const now = Date.now();
    const start = dayStart(tz, now);
    return { from: toInput(tz, start), to: toInput(tz, now) };
  });
  const [channel, setChannel] = useState<string | null>(null);
  const [groups, setGroups] = useState<Group[]>([]);
  const [actor, setActor] = useState('');
  const [kind, setKind] = useState<'' | Kind>('');
  const [query, setQuery] = useState('');
  const [rows, setRows] = useState<AsRunEntry[] | null>(null);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [zones, setZones] = useState<Record<string, Zone[]>>({});

  const now = () => Date.now() + serverOffset;
  const [from, to] = useMemo(() => range(period, tz, Date.now() + serverOffset, custom), [period, tz, custom]); // eslint-disable-line react-hooks/exhaustive-deps
  const live = to === null;

  // Entries of the period, from the server.
  useEffect(() => {
    let alive = true;
    setRows(null);
    const q = new URLSearchParams({ from: String(from), limit: '5000' });
    if (to !== null) q.set('to', String(to));
    api<AsRunEntry[]>('GET', `/api/asrun?${q}`)
      .then((r) => alive && setRows(r))
      .catch((e) => {
        if (!alive) return;
        setRows([]);
        toast((e as Error).message, 'error');
      });
    return () => {
      alive = false;
    };
  }, [from, to]); // eslint-disable-line react-hooks/exhaustive-deps

  // New entries, live.
  useEffect(() => {
    if (!live) return;
    const on = (ev: Event) => {
      const e = (ev as CustomEvent<{ entry: AsRunEntry }>).detail.entry;
      if (e.at >= from) setRows((rs) => (rs ? [e, ...rs] : rs));
    };
    serverEvents.addEventListener('asRun', on);
    return () => serverEvents.removeEventListener('asRun', on);
  }, [live, from]);

  // Zone names and colours of the projects seen.
  useEffect(() => {
    const ids = new Set((rows ?? []).map((e) => e.projectId).filter((id) => id && !(id in zones)));
    if (!ids.size) return;
    const full = useApp.getState().full;
    for (const id of ids) {
      if (full[id]) {
        setZones((z) => ({ ...z, [id]: full[id].zones ?? [] }));
        continue;
      }
      setZones((z) => ({ ...z, [id]: [] }));
      api<{ zones?: Zone[] }>('GET', `/api/projects/${encodeURIComponent(id)}`)
        .then((p) => setZones((z) => ({ ...z, [id]: p.zones ?? [] })))
        .catch(() => {});
    }
  }, [rows]); // eslint-disable-line react-hooks/exhaustive-deps

  const all = rows ?? [];
  const needle = query.trim().toLowerCase();
  const inChannel = channel ? all.filter((e) => e.channelId === channel) : all;
  const shown = inChannel.filter((e) => (!groups.length || groups.includes(groupOf(e.action))) && (!actor || actorKey(e) === actor) && (!kind || e.actor.kind === kind) && matches(e, needle));

  // Channels: the server's and those seen in the log.
  const chans = useMemo(() => {
    const m = new Map<string, string>(channels.map((c) => [c.id, c.name]));
    for (const e of all) if (!m.has(e.channelId)) m.set(e.channelId, e.channelName || e.channelId);
    return [...m].map(([id, name]) => ({ id, name, n: all.filter((e) => e.channelId === id).length }));
  }, [channels, all]);
  const actors = useMemo(() => {
    const m = new Map<string, string>();
    for (const e of all) if (e.actor.id && !m.has(e.actor.id)) m.set(e.actor.id, e.actor.name || e.actor.id);
    return [...m].sort((a, b) => a[1].localeCompare(b[1]));
  }, [all]);
  const seenGroups = useMemo(() => {
    const seen = new Set(all.map((e) => groupOf(e.action)));
    return GROUPS.filter((g) => ALWAYS.includes(g) || seen.has(g) || groups.includes(g));
  }, [all, groups]);

  const multiDay = to === null ? now() - from > DAY || dayStart(tz, from) !== dayStart(tz, now()) : to - from > DAY;
  const zoneOf = (e: AsRunEntry) => zones[e.projectId]?.find((z) => z.id === e.zone);
  const channelName = (id: string) => chans.find((c) => c.id === id)?.name ?? id;

  const toggle = (g: Group) => setGroups((gs) => (gs.includes(g) ? gs.filter((x) => x !== g) : [...gs, g]));
  const flip = (k: string) =>
    setOpen((o) => {
      const n = new Set(o);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });

  const csvHref = () => {
    const q = new URLSearchParams({ from: String(from) });
    if (to !== null) q.set('to', String(to));
    if (channel) q.set('channel', channel);
    if (groups.length) q.set('actions', groups.join(','));
    if (actor) q.set('actors', actor);
    if (kind) q.set('kinds', kind);
    if (needle) q.set('q', needle);
    return `/api/asrun.csv?${q}`;
  };

  const periodText = () => {
    const a = dateText(tz, from);
    const b = dateText(tz, to ?? now());
    return a === b ? a : `${a} – ${b}`;
  };

  const printPdf = () => {
    const w = window.open('', '_blank');
    if (!w) return toast(t('The browser blocked the print window.'), 'error');
    const filters = [
      t('Period: {value}', { value: periodText() }),
      t('Channel: {value}', { value: channel ? channelName(channel) : tc('asrun', 'All') }),
      groups.length ? t('Actions: {value}', { value: groups.map(chipLabel).join(', ') }) : '',
      actor ? t('Operator: {value}', { value: actors.find((a) => a[0] === actor)?.[1] ?? actor }) : '',
      kind ? t('Source: {value}', { value: kindLabel(kind) }) : '',
      needle ? t('Search: “{value}”', { value: query.trim() }) : '',
    ].filter(Boolean);
    w.document.write(printHtml(shown, filters, tz, multiDay, zoneOf));
    w.document.close();
    w.focus();
    setTimeout(() => w.print(), 200);
  };

  const periodItem = (p: Period, icon: React.ReactNode, label: string) => <NavItem on={period === p} icon={icon} label={label} onClick={() => setPeriod(p)} />;

  const nav = (
    <>
      <NavSection>{t('Channels')}</NavSection>
      <NavItem on={channel === null} icon={<Layers size={15} />} label={tc('asrun', 'All')} count={all.length} onClick={() => setChannel(null)} />
      {chans.map((c) => (
        <NavItem key={c.id} on={channel === c.id} icon={<Tv size={15} />} label={c.name} count={c.n} onClick={() => setChannel(c.id)} />
      ))}
      <NavSection>{t('Period')}</NavSection>
      {periodItem('hour', <History size={15} />, t('Last hour'))}
      {periodItem('today', <CalendarDays size={15} />, t('Today'))}
      {periodItem('yesterday', <CalendarDays size={15} />, t('Yesterday'))}
      {periodItem('week', <CalendarRange size={15} />, t('Last 7 days'))}
      {periodItem('custom', <CalendarRange size={15} />, t('Dates…'))}
      {period === 'custom' && (
        <div className="asr-dates">
          <label>
            <span>{tc('asrun', 'From')}</span>
            <input type="datetime-local" value={custom.from} onChange={(e) => e.target.value && setCustom({ ...custom, from: e.target.value })} />
          </label>
          <label>
            <span>{tc('asrun', 'To')}</span>
            <input type="datetime-local" value={custom.to} onChange={(e) => e.target.value && setCustom({ ...custom, to: e.target.value })} />
          </label>
        </div>
      )}
    </>
  );

  return (
    <SidePage nav={nav} className="asr-page">
      <PageHead title={t('As-run log')} sub={`${periodText()} · ${tn(shown.length, '{n} action', '{n} actions')}`}>
        <div className="asr-head-actions">
          {live && <span className="asr-live" title={t('Live')} />}
          <a className="btn" href={csvHref()} download>
            <Download size={14} /> {t('Export CSV')}
          </a>
          <button className="btn" onClick={printPdf} disabled={!shown.length}>
            <FileText size={14} /> {t('Export PDF')}
          </button>
        </div>
      </PageHead>

      <div className="asr-toolbar ux3-asr-row">
        <label className="search">
          <Search size={14} />
          <input value={query} placeholder={t('Search…')} title={t('Graphic, value, operator…')} onChange={(e) => setQuery(e.target.value)} />
          {query && (
            <button className="icon-btn ghost" title={t('Clear the search')} onClick={() => setQuery('')}>
              <X size={13} />
            </button>
          )}
        </label>
        <div className="asr-chips">
          <button className={`asr-chip ${groups.length ? '' : 'on'}`} onClick={() => setGroups([])}>
            {t('All actions')}
          </button>
          {seenGroups.map((g) => (
            <button key={g} className={`asr-chip ${groups.includes(g) ? 'on' : ''}`} onClick={() => toggle(g)}>
              {chipLabel(g)}
            </button>
          ))}
        </div>
        <div className="spacer" />
        <Dropdown className="asr-drop" value={actor} title={t('Operators')} onChange={setActor} options={[{ value: '', label: t('Operators · all') }, ...actors.map(([id, name]) => ({ value: id, label: name }))]} />
        <Dropdown className="asr-drop" value={kind} title={t('Sources')} onChange={(v) => setKind(v as '' | Kind)} options={[{ value: '', label: t('Sources · all') }, ...KINDS.map((k) => ({ value: k, label: kindLabel(k) }))]} />
      </div>

      {rows === null ? (
        <div className="loading">{t('Loading…')}</div>
      ) : (
        <table className="asr-table">
          <thead>
            <tr>
              <th className="asr-c-time">{t('Time')}</th>
              <th className="asr-c-act">{t('Action')}</th>
              <th>{t('Graphic')}</th>
              <th className="asr-c-zone">{t('Zone')}</th>
              <th className="asr-c-chan">{t('Channel')}</th>
              <th className="asr-c-by">{t('By')}</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((e, i) => {
              const k = `${e.at}-${e.channelId}-${e.action}-${i}`;
              const vals = Object.entries(e.values ?? {});
              const can = vals.length > 0;
              const isOpen = can && open.has(`${e.at}-${e.channelId}-${e.action}`);
              const g = groupOf(e.action);
              const z = zoneOf(e);
              return (
                <Fragment key={k}>
                  <tr className={`${can ? 'asr-can' : ''} ${isOpen ? 'asr-open' : ''}`} onClick={can ? () => flip(`${e.at}-${e.channelId}-${e.action}`) : undefined}>
                    <td className="asr-time">{clockText(tz, e.at, multiDay)}</td>
                    <td>
                      <span className={`asr-act asr-a-${g}`}>{badge(g)}</span>
                    </td>
                    <td className="asr-graphic">
                      {can && (isOpen ? <ChevronDown size={13} className="asr-chev" /> : <ChevronRight size={13} className="asr-chev" />)}
                      {g === 'undo' && e.detail ? <span className="asr-undone">{e.detail.toUpperCase()}</span> : null}
                      <span>{e.label || '—'}</span>
                      {g === 'auto' && <span className="muted"> · {autoLabel(e.action)}</span>}
                      {e.actor.kind === 'schedule' && e.detail ? (
                        <span className="asr-sched">
                          <Clock size={11} /> {e.detail}
                        </span>
                      ) : g !== 'undo' && e.detail ? (
                        <span className="muted"> · {e.detail}</span>
                      ) : null}
                      {isOpen && (
                        <div className="asr-vals">
                          {vals.map(([key, [before, after]]) => (
                            <Fragment key={key}>
                              <span className="asr-key">{key}</span>
                              <span>
                                {before !== null && before !== after ? (
                                  <>
                                    <span className="muted">{before || '∅'}</span> → <b className="asr-diff">{after || '∅'}</b>
                                  </>
                                ) : (
                                  after
                                )}
                              </span>
                            </Fragment>
                          ))}
                        </div>
                      )}
                    </td>
                    <td>
                      {e.zone ? (
                        <span className="asr-zone">
                          <span className="asr-zd" style={{ background: z?.color || 'var(--text-3)' }} />
                          {z?.name || e.zone}
                        </span>
                      ) : (
                        <span className="muted">—</span>
                      )}
                    </td>
                    <td>{e.channelName || e.channelId}</td>
                    <td>
                      <Who e={e} />
                    </td>
                  </tr>
                </Fragment>
              );
            })}
            {shown.length === 0 && (
              <tr>
                <td colSpan={6} className="asr-empty">
                  {t('No action')}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      )}
    </SidePage>
  );
}

const initials = (name: string) =>
  name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join('') || '?';

function Who({ e }: { e: AsRunEntry }) {
  const k = e.actor.kind;
  const icon = k === 'api' ? <KeyRound size={11} /> : k === 'auto' ? <Cog size={11} /> : k === 'schedule' ? <Clock size={11} /> : k === 'mos' ? <Newspaper size={11} /> : k === 'data' ? <Database size={11} /> : k === 'user' ? initials(e.actor.name) : <Radio size={11} />;
  return (
    <span className="asr-who" title={kindOne(k)}>
      <span className={`asr-av asr-k-${k}`}>{icon}</span>
      {e.actor.name || kindOne(k)}
    </span>
  );
}

// ---------- print ----------

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

function printHtml(rows: AsRunEntry[], filters: string[], tz: string, multiDay: boolean, zoneOf: (e: AsRunEntry) => Zone | undefined): string {
  const body = rows
    .map((e) => {
      const g = groupOf(e.action);
      const vals = Object.entries(e.values ?? {})
        .map(([k, [b, a]]) => `${esc(k)}: ${b !== null && b !== a ? `${esc(b)} → ` : ''}${esc(a)}`)
        .join('<br>');
      const label = `${g === 'undo' && e.detail ? `${esc(e.detail.toUpperCase())} ` : ''}${esc(e.label || '—')}${g === 'auto' ? ` · ${esc(autoLabel(e.action))}` : g !== 'undo' && e.detail ? ` · ${esc(e.detail)}` : ''}`;
      return `<tr><td class="m">${clockText(tz, e.at, multiDay)}</td><td><b>${esc(badge(g))}</b></td><td>${label}${vals ? `<div class="v">${vals}</div>` : ''}</td><td>${esc(zoneOf(e)?.name || e.zone)}</td><td>${esc(e.channelName || e.channelId)}</td><td>${esc(e.actor.name || kindOne(e.actor.kind))}<div class="v">${esc(kindOne(e.actor.kind))}</div></td></tr>`;
    })
    .join('');
  return `<!doctype html><html lang="${document.documentElement.lang}"><head><meta charset="utf-8"><title>${esc(t('As-run log'))}</title><style>
@page{size:A4 landscape;margin:12mm}
body{font:11px/1.35 Inter,system-ui,sans-serif;color:#111;margin:0}
h1{font-size:18px;margin:0 0 4px}
.f{color:#555;margin:0 0 12px}
.f span+span:before{content:" · "}
table{width:100%;border-collapse:collapse}
th{text-align:left;font-size:9.5px;text-transform:uppercase;letter-spacing:.06em;color:#666;border-bottom:1.5px solid #222;padding:5px 6px}
td{border-bottom:1px solid #ddd;padding:5px 6px;vertical-align:top}
tr{break-inside:avoid}
.m{font-family:ui-monospace,Consolas,monospace;white-space:nowrap}
.v{color:#666;font-size:10px;margin-top:2px}
</style></head><body>
<h1>${esc(t('As-run log'))}</h1>
<p class="f">${filters.map((f) => `<span>${esc(f)}</span>`).join('')}<span>${esc(tn(rows.length, '{n} action', '{n} actions'))}</span></p>
<table><thead><tr><th>${esc(t('Time'))}</th><th>${esc(t('Action'))}</th><th>${esc(t('Graphic'))}</th><th>${esc(t('Zone'))}</th><th>${esc(t('Channel'))}</th><th>${esc(t('By'))}</th></tr></thead><tbody>${body}</tbody></table>
</body></html>`;
}

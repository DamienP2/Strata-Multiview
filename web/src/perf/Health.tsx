import { Activity, ChevronRight, Download, HardDrive, HeartPulse, RefreshCw, ScrollText, Search } from 'lucide-react';
import { Fragment, useEffect, useRef, useState } from 'react';
import { t, tc, tn, useLang } from '../i18n';
import { NavItem, NavSection, PageHead, SidePage } from '../pages/SidePage';
import { api } from '../store';
import { bytes, clock } from './format';
import { useHealth, useHealthStore, type Incident, type JournalLine } from './health';
import { Logs } from './Logs';

/*
 * System › Health (admins): how the server has been doing — its uptime, the
 * crashes of the month, the errors and warnings of the day grouped as
 * incidents, the journal kept on disk (30 days), the live log, and an
 * archive of all of it for the support (cg-server health.rs).
 */

type Section = 'overview' | 'journal' | 'live';
type Level = 'crash' | 'error' | 'warn' | 'info';

const CATS: [string, string][] = [
  ['air', 'On air'],
  ['outputs', 'Outputs'],
  ['inputs', 'Inputs'],
  ['data', 'Data'],
  ['render', 'Rendering'],
  ['users', 'Users'],
  ['server', 'Server'],
];
const catLabel = (c: string) => t(CATS.find(([k]) => k === c)?.[1] ?? 'Server');
const LEVEL_LABEL: Record<string, string> = { CRASH: 'Crash', ERROR: 'Error', WARN: 'Warning', INFO: 'Info' };

/** 3 d 4 h, 2 h 10 min, 12 min. */
function uptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return d ? t('{d} d {h} h', { d, h }) : h ? t('{h} h {m} min', { h, m }) : t('{m} min', { m });
}
/** 27/10 13:40, or 13:40 today. */
function when(ms: number): string {
  const d = new Date(ms);
  const p = (v: number) => String(v).padStart(2, '0');
  const today = new Date().toDateString() === d.toDateString();
  return today ? `${p(d.getHours())}:${p(d.getMinutes())}` : `${p(d.getDate())}/${p(d.getMonth() + 1)} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
const dayLabel = (ms: number) => {
  const d = new Date(ms);
  const today = new Date();
  const yesterday = new Date(Date.now() - 86_400_000);
  if (d.toDateString() === today.toDateString()) return t('Today');
  if (d.toDateString() === yesterday.toDateString()) return t('Yesterday');
  return d.toLocaleDateString(useLang.getState().lang, { weekday: 'long', day: 'numeric', month: 'long' });
};

export function Health() {
  const health = useHealth(true);
  const [section, setSection] = useState<Section>('overview');
  const item = (s: Section, icon: React.ReactNode, label: string, extra?: Partial<React.ComponentProps<typeof NavItem>>) => <NavItem on={section === s} icon={icon} label={label} onClick={() => setSection(s)} {...extra} />;
  const incidents = health?.incidents.length ?? 0;
  const nav = (
    <>
      <NavSection>{t('Health')}</NavSection>
      {item('overview', <HeartPulse size={15} />, tc('perf', 'Overview'), { status: health ? (health.unseenCrash || health.lastHourErrors ? 'warn' : 'ok') : undefined, count: incidents || undefined })}
      {item('journal', <ScrollText size={15} />, t('Journal'))}
      {item('live', <Activity size={15} />, t('Live log'))}
    </>
  );
  return (
    <SidePage nav={nav} className="perf-page health-page">
      <PageHead title={t('Health')} sub={health ? t('Strata {version} · up {up}', { version: health.version, up: uptime(health.uptimeSecs) }) : undefined}>
        <a className="btn" href="/api/system/health/export.zip" download>
          <Download size={14} /> {t('Export for the support')}
        </a>
      </PageHead>
      {section === 'overview' ? <Overview /> : section === 'journal' ? <Journal /> : <Logs />}
    </SidePage>
  );
}

function Overview() {
  const health = useHealthStore((s) => s.health);
  const seen = useHealthStore((s) => s.seen);
  const [open, setOpen] = useState<number | null>(null);
  // Looked at: the crashes are seen (the banner and the menu's dot go).
  useEffect(() => {
    if (health?.unseenCrash) void seen();
  }, [health?.unseenCrash, seen]);
  if (!health) return <div className="loading">{t('Loading…')}</div>;
  const used = health.disk.total ? 1 - health.disk.free / health.disk.total : 0;
  return (
    <>
      <div className="hl-cards">
        <div className="hl-card ok">
          <span>{t('In service')}</span>
          <b>{uptime(health.uptimeSecs)}</b>
          <em>{health.startedAt ? t('since {when}', { when: when(health.startedAt) }) : ''}</em>
        </div>
        <div className={`hl-card ${health.crashes ? 'crash' : 'ok'}`}>
          <span>{t('Crashes · 30 days')}</span>
          <b>{health.crashes}</b>
          <em>{health.lastCrash ? t('last {when}', { when: when(health.lastCrash) }) : t('None')}</em>
        </div>
        <div className={`hl-card ${health.errors ? 'err' : 'ok'}`}>
          <span>{t('Errors · 24 h')}</span>
          <b>{health.errors}</b>
          <em>{health.lastHourErrors ? tn(health.lastHourErrors, '{n} in the last hour', '{n} in the last hour') : ' '}</em>
        </div>
        <div className={`hl-card ${health.warnings ? 'warn' : 'ok'}`}>
          <span>{t('Warnings · 24 h')}</span>
          <b>{health.warnings}</b>
          <em> </em>
        </div>
        {health.disk.total > 0 && (
          <div className={`hl-card ${used > 0.9 ? 'err' : used > 0.8 ? 'warn' : 'ok'}`}>
            <span>
              <HardDrive size={11} /> {t('Disk')}
            </span>
            <b>{Math.round(used * 100)} %</b>
            <em>{t('{free} free', { free: bytes(health.disk.free) })}</em>
          </div>
        )}
      </div>
      <h2 className="hl-h">{t('Incidents')}</h2>
      {health.incidents.length ? (
        <div className="hl-inc">
          {health.incidents.map((i, n) => (
            <IncidentRow key={n} i={i} open={open === n} onToggle={() => setOpen(open === n ? null : n)} />
          ))}
        </div>
      ) : (
        <div className="hl-empty">{t('No incident: no crash in 30 days, no error or warning in 24 hours.')}</div>
      )}
    </>
  );
}

function IncidentRow({ i, open, onToggle }: { i: Incident; open: boolean; onToggle: () => void }) {
  const level = i.kind === 'stop' ? t('Shutdown') : t(LEVEL_LABEL[i.level] ?? i.level);
  const title = i.kind === 'stop' ? t('Unexpected stop (power cut, forced stop or crash)') : i.crash ? t('The server crashed: {message}', { message: i.title }) : i.title;
  const detail = i.crash
    ? [i.crash.location, i.crash.thread && t('thread {name}', { name: i.crash.thread }), i.crash.uptimeSecs ? t('Up for {up} before', { up: uptime(i.crash.uptimeSecs) }) : ''].filter(Boolean).join(' · ')
    : i.detail;
  const report = i.crash ? [i.crash.version && t('Version {v}', { v: i.crash.version }), i.crash.backtrace].filter(Boolean).join('\n\n').trim() : '';
  const more = !!report || (!i.crash && !!i.detail);
  return (
    <div className={`hl-row ${open ? 'open' : ''}`} onClick={more ? onToggle : undefined}>
      <span className={`hl-lv ${i.level.toLowerCase()}`}>{level}</span>
      <div className="hl-main">
        <div className="hl-t">
          {more && <ChevronRight size={12} className="hl-chev" />}
          {title}
        </div>
        <div className="hl-d">
          {catLabel(i.cat)}
          {detail ? ` · ${detail}` : ''}
        </div>
        {open && report && <pre className="hl-pre">{report}</pre>}
        {open && !i.crash && i.detail && <pre className="hl-pre">{i.detail}</pre>}
      </div>
      <div className="hl-n">
        <b>{when(i.last)}</b>
        {i.count > 1 ? t('×{n} since {when}', { n: i.count, when: when(i.first) }) : ''}
      </div>
    </div>
  );
}

const LEVELS: { value: Level; label: string }[] = [
  { value: 'crash', label: 'Crashes' },
  { value: 'error', label: 'Errors' },
  { value: 'warn', label: '+ Warnings' },
  { value: 'info', label: '+ Info' },
];
const DAYS: [number, string][] = [
  [1, '24 h'],
  [7, '7 days'],
  [30, '30 days'],
];

function Journal() {
  const [level, setLevel] = useState<Level>('warn');
  const [days, setDays] = useState(1);
  const [cat, setCat] = useState('all');
  const [q, setQ] = useState('');
  const [query, setQuery] = useState('');
  const [lines, setLines] = useState<JournalLine[]>([]);
  const [more, setMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const ask = useRef(0);
  useEffect(() => {
    const h = setTimeout(() => setQuery(q), 300);
    return () => clearTimeout(h);
  }, [q]);
  const url = (before?: number) => `/api/system/health/journal?level=${level}&days=${days}&cat=${cat}&q=${encodeURIComponent(query)}&limit=300${before ? `&before=${before}` : ''}`;
  const load = async () => {
    const n = ++ask.current;
    setBusy(true);
    try {
      const r = await api<{ lines: JournalLine[]; more: boolean }>('GET', url());
      if (n !== ask.current) return;
      setLines(r.lines);
      setMore(r.more);
    } catch {
      /* the server is away */
    }
    setBusy(false);
  };
  useEffect(() => {
    void load();
    const h = setInterval(() => void load(), 15_000);
    return () => clearInterval(h);
  }, [level, days, cat, query]); // eslint-disable-line react-hooks/exhaustive-deps
  const older = async () => {
    const last = lines[lines.length - 1];
    if (!last) return;
    const r = await api<{ lines: JournalLine[]; more: boolean }>('GET', url(last.at));
    setLines((x) => [...x, ...r.lines]);
    setMore(r.more);
  };
  let day = '';
  return (
    <>
      <div className="hl-bar">
        <div className="segmented">
          {LEVELS.map((l) => (
            <button key={l.value} className={level === l.value ? 'on' : ''} onClick={() => setLevel(l.value)}>
              {t(l.label)}
            </button>
          ))}
        </div>
        <div className="segmented">
          {DAYS.map(([d, l]) => (
            <button key={d} className={days === d ? 'on' : ''} onClick={() => setDays(d)}>
              {t(l)}
            </button>
          ))}
        </div>
        <label className="lib-search hl-search">
          <Search size={13} />
          <input value={q} placeholder={t('Search')} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.stopPropagation()} />
        </label>
        <button className={`btn ghost icon ${busy ? 'spin' : ''}`} title={t('Refresh')} onClick={() => void load()}>
          <RefreshCw size={14} />
        </button>
      </div>
      <div className="hl-chips">
        {[['all', 'All'] as [string, string], ...CATS].map(([k, l]) => (
          <button key={k} className={cat === k ? 'on' : ''} onClick={() => setCat(k)}>
            {t(l)}
          </button>
        ))}
      </div>
      {lines.length ? (
        <div className="hl-log">
          {lines.map((l, n) => {
            const d = new Date(l.at).toDateString();
            const head = d !== day;
            day = d;
            return (
              <Fragment key={n}>
                {head && <div className="hl-day">{dayLabel(l.at)}</div>}
                <div className={`hl-line ${l.level}`}>
                  <span className="hl-at">{clock(l.at)}</span>
                  <span className="hl-lvl">{t(LEVEL_LABEL[l.level] ?? l.level)}</span>
                  <span className="hl-cat">{catLabel(l.cat)}</span>
                  <span className="hl-msg" title={l.target}>
                    {l.target === 'crash' && l.msg.startsWith('unexpected stop') ? t('Unexpected stop (power cut, forced stop or crash)') : l.msg}
                    {l.fields ? <em> {l.fields}</em> : null}
                  </span>
                </div>
              </Fragment>
            );
          })}
          {more && (
            <button className="btn ghost hl-more" onClick={() => void older()}>
              {t('Older')}
            </button>
          )}
        </div>
      ) : (
        <div className="hl-empty">{busy ? t('Loading…') : t('Nothing in the journal for these filters.')}</div>
      )}
    </>
  );
}

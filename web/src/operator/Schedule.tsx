import { AlarmClock, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { create } from 'zustand';
import { t } from '../i18n';
import { useTicker } from '../engine/LiveTime';
import type { FrameRate, Project, RundownItem } from '../engine/types';
import { api, clientNow, serverEvents, useApp } from '../store';

/*
 * Takes and outs programmed in advance (crates/cg-server/src/schedule.rs):
 * the clock button of a graphic and its popover, the badge of a graphic
 * programmed, the list in the top bar, and the time-of-day timecode.
 */

export type When = { kind: 'at'; time: string } | { kind: 'in'; at: number } | { kind: 'timecode'; tc: string };

export interface Scheduled {
  id: string;
  channelId: string;
  projectId: string;
  action: 'take' | 'out';
  item?: string;
  zone?: string;
  when: When;
  repeat: 'once' | 'daily';
  createdBy: string;
  enabled: boolean;
  label: string;
  /** Next time it fires (server clock, ms). */
  next: number;
  missed: boolean;
  /** Its project is not the one on the channel. */
  waiting: boolean;
}

const useSchedule = create<{ byChannel: Record<string, Scheduled[]>; listOpen: boolean }>(() => ({ byChannel: {}, listOpen: false }));

/** Opens (or closes) the list of the scheduled takes. */
export const toggleScheduleList = () => useSchedule.setState((s) => ({ listOpen: !s.listOpen }));

const NONE: Scheduled[] = [];
const useEntries = (channelId: string | null) => useSchedule((s) => (channelId ? (s.byChannel[channelId] ?? NONE) : NONE));

/** Keeps the channel's entries (fetched, then pushed by the server). */
function useScheduleFeed(channelId: string | null) {
  const connected = useApp((s) => s.connected);
  useEffect(() => {
    if (!channelId) return;
    const set = (items: Scheduled[]) => useSchedule.setState((s) => ({ byChannel: { ...s.byChannel, [channelId]: items } }));
    api<Scheduled[]>('GET', `/api/channels/${encodeURIComponent(channelId)}/schedule`).then(set, () => {});
    const on = (e: Event) => {
      const msg = (e as CustomEvent<{ channelId: string; items: Scheduled[] }>).detail;
      if (msg.channelId === channelId) set(msg.items);
    };
    serverEvents.addEventListener('schedule', on);
    return () => serverEvents.removeEventListener('schedule', on);
  }, [channelId, connected]);
}

// ------------------------------------------------------------- time ----

const DAY = 86_400_000;
const fmts = new Map<string, Intl.DateTimeFormat>();
function fmtOf(tz: string): Intl.DateTimeFormat {
  let f = fmts.get(tz);
  if (!f) {
    const opts: Intl.DateTimeFormatOptions = { hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23' };
    try {
      f = new Intl.DateTimeFormat('en-GB', { ...opts, timeZone: tz || undefined });
    } catch {
      f = new Intl.DateTimeFormat('en-GB', opts);
    }
    fmts.set(tz, f);
  }
  return f;
}

/** ms since local midnight, in the server's time zone, of the server time `ms`. */
export function msOfDay(tz: string, ms: number): number {
  const p = fmtOf(tz).formatToParts(new Date(ms));
  const n = (type: string) => +(p.find((x) => x.type === type)?.value ?? 0);
  return (((n('hour') % 24) * 60 + n('minute')) * 60 + n('second')) * 1000 + (((ms % 1000) + 1000) % 1000);
}

const two = (n: number) => String(Math.floor(n)).padStart(2, '0');
const hms = (msDay: number) => {
  const s = Math.floor(msDay / 1000);
  return `${two(s / 3600)}:${two((s / 60) % 60)}:${two(s % 60)}`;
};

/** Nominal frames a second, and frames dropped each minute (drop frame at 29.97 / 59.94). */
function tcRate(r: FrameRate): [number, number] {
  const fps = r.num / (r.den || 1);
  const n = Math.max(1, Math.round(fps));
  return [n, Math.abs(fps - n) > 1e-3 && n % 30 === 0 ? n / 15 : 0];
}

/** The time-of-day timecode, as the server counts it (schedule.rs `tc_of_day`). */
export function tcOfDay(msDay: number, r: FrameRate): string {
  const [n, d] = tcRate(r);
  const ms = ((msDay % DAY) + DAY) % DAY;
  if (!d) return `${hms(ms)}:${two(((ms % 1000) * n) / 1000)}`;
  const per10 = n * 600 - d * 9;
  const perMin = n * 60 - d;
  let f = Math.floor((ms * r.num) / (1000 * (r.den || 1))) % (per10 * 144);
  const tens = Math.floor(f / per10);
  const rem = f % per10;
  f += d * 9 * tens + (rem > d ? d * Math.floor((rem - d) / perMin) : 0);
  const s = Math.floor(f / n);
  return `${two((s / 3600) % 24)}:${two((s / 60) % 60)}:${two(s % 60)};${two(f % n)}`;
}

/** "1:48", "31:48", "2:05:10". */
function left(ms: number): string {
  const s = Math.max(0, Math.ceil(ms / 1000));
  return s >= 3600 ? `${Math.floor(s / 3600)}:${two((s / 60) % 60)}:${two(s % 60)}` : `${Math.floor(s / 60)}:${two(s % 60)}`;
}

/** The server's clock, now. */
function useServerNow(fps: number) {
  const offset = useApp((s) => s.serverOffset);
  return useTicker(true, fps) + offset;
}

/** "20:00:00", "TC 20:41:30:00": when an entry fires. */
function whenText(e: Scheduled, tz: string): string {
  if (e.when.kind === 'at') return e.when.time;
  if (e.when.kind === 'timecode') return `TC ${e.when.tc}`;
  return hms(msOfDay(tz, e.next || e.when.at));
}

// ---------------------------------------------------------- top bar ----

/** In the app's top bar: the running timecode and the scheduled takes. */
export function ScheduleTop({ channelId, project }: { channelId: string | null; project: Project }) {
  useScheduleFeed(channelId);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  useEffect(() => setSlot(document.getElementById('topbar-slot')), []);
  const open = useSchedule((s) => s.listOpen);
  const entries = useEntries(channelId);
  const btn = useRef<HTMLButtonElement>(null);
  const active = entries.filter((e) => e.enabled && !e.missed);
  if (!slot || !channelId) return null;
  return createPortal(
    <>
      <button ref={btn} className={`sch-topbtn ${open ? 'on' : ''} ${active.length ? 'some' : ''}`} title={t('Scheduled takes')} onClick={toggleScheduleList}>
        <AlarmClock size={13} />
        {active.length > 0 && <b>{active.length}</b>}
      </button>
      {open && <ScheduleList channelId={channelId} project={project} entries={entries} anchor={btn.current} />}
    </>,
    slot,
  );
}

/** The time of day in timecode (in the transport bar). */
export function TimecodeChip({ channelId }: { channelId: string }) {
  const tz = useApp((s) => s.timezone);
  const rate = useApp((s) => s.channels.find((c) => c.id === channelId)?.frameRate) ?? { num: 25, den: 1 };
  const now = useServerNow(25);
  // Server lost: no time of its own to show.
  const connected = useApp((s) => s.connected);
  return (
    <span className="sch-tc" title={t('Time of day, in frames of the channel')}>
      <span className="sch-tc-l">TC</span> <b>{connected ? tcOfDay(msOfDay(tz, now), rate) : '--:--:--:--'}</b>
    </span>
  );
}

/** The scheduled takes of the channel: countdowns, on / off, remove. */
function ScheduleList({ channelId, project, entries, anchor }: { channelId: string; project: Project; entries: Scheduled[]; anchor: HTMLElement | null }) {
  const ref = useRef<HTMLDivElement>(null);
  const tz = useApp((s) => s.timezone);
  const toast = useApp((s) => s.toast);
  const loaded = useApp((s) => s.channels.find((c) => c.id === channelId)?.projectId);
  const now = useServerNow(4);
  useEffect(() => {
    const down = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && !anchor?.contains(e.target as Node) && useSchedule.setState({ listOpen: false });
    const key = (e: KeyboardEvent) => e.key === 'Escape' && useSchedule.setState({ listOpen: false });
    window.addEventListener('pointerdown', down);
    window.addEventListener('keydown', key);
    return () => {
      window.removeEventListener('pointerdown', down);
      window.removeEventListener('keydown', key);
    };
  }, [anchor]);
  const url = (id: string) => `/api/channels/${encodeURIComponent(channelId)}/schedule/${encodeURIComponent(id)}`;
  const fail = (e: unknown) => toast((e as Error).message, 'error');
  const box = anchor?.getBoundingClientRect();
  return createPortal(
    <div ref={ref} className="sch-list" style={{ top: (box?.bottom ?? 46) + 8, right: Math.max(8, window.innerWidth - (box?.right ?? window.innerWidth - 8)) }}>
      <header>{t('Scheduled takes')}</header>
      {entries.length === 0 && <p className="sch-empty">{t('Nothing scheduled.')}</p>}
      {entries.map((e) => {
        const waiting = e.projectId !== loaded;
        const off = !e.enabled || e.missed;
        const what = e.action === 'take' ? 'TAKE' : 'OUT';
        const when = e.when.kind === 'timecode' ? t('{action} at TC {tc}', { action: what, tc: e.when.tc }) : t('{action} at {time}', { action: what, time: whenText(e, tz) });
        const name = (e.item && project.id === e.projectId && project.rundown.find((r) => r.id === e.item)?.name) || e.label;
        const ms = e.next - now;
        return (
          <div key={e.id} className={`sch-row ${off || waiting ? 'off' : ''}`}>
            <span className="sch-badge">
              <AlarmClock size={11} />
            </span>
            <span className="sch-row-main">
              <b>{name}</b>
              <span className="muted small">
                {when}
                {e.repeat === 'daily' && ` · ${t('every day')}`}
                {e.missed ? ` · ${t('missed')}` : waiting ? ` · ${t('waiting for its project')}` : !e.enabled ? ` · ${t('off')}` : ''}
              </span>
            </span>
            <span className={`sch-left ${!off && !waiting && ms < 5 * 60_000 ? 'soon' : ''}`}>{off ? '—' : left(ms)}</span>
            <button className={`wk-tog ${e.enabled && !e.missed ? 'on' : ''}`} title={e.enabled && !e.missed ? t('Turn off') : t('Turn on')} onClick={() => api('PATCH', url(e.id), { enabled: !(e.enabled && !e.missed) }).catch(fail)}>
              <i />
            </button>
            <button className="sch-x" title={t('Remove')} onClick={() => api('DELETE', url(e.id)).catch(fail)}>
              <X size={13} />
            </button>
          </div>
        );
      })}
    </div>,
    document.body,
  );
}

// ------------------------------------------------------------ cards ----

/** A graphic's next scheduled take (a badge), and the editor to schedule one, opened from its row's menu at `editAt`. */
export function ScheduleCell({ channelId, project, item, editAt, onClose }: { channelId: string | null; project: Project; item: RundownItem; editAt: DOMRect | null; onClose: () => void }) {
  const entries = useEntries(channelId);
  const mine = useMemo(() => entries.filter((e) => e.item === item.id && e.projectId === project.id && e.enabled && !e.missed).sort((a, b) => a.next - b.next), [entries, item.id, project.id]);
  if (!mine[0] && !editAt) return null;
  return (
    <span className="sch-cell" onClick={(e) => e.stopPropagation()}>
      {mine[0] && <Badge entry={mine[0]} more={mine.length - 1} />}
      {editAt && channelId && <ScheduleEditor channelId={channelId} item={item} at={editAt} onClose={onClose} />}
    </span>
  );
}

function Badge({ entry, more }: { entry: Scheduled; more: number }) {
  const tz = useApp((s) => s.timezone);
  const now = useServerNow(4);
  const ms = entry.next - now;
  const full = whenText(entry, tz);
  const action = entry.action === 'take' ? 'TAKE' : 'OUT';
  const title = entry.when.kind === 'timecode' ? t('{action} at TC {tc}', { action, tc: entry.when.tc }) : t('{action} at {time}', { action, time: full });
  // Short on the card: the time (without ":00" seconds), the countdown in the last minutes.
  const short = ms < 600_000 ? left(ms) : full.replace(/^(TC )?(\d\d:\d\d):00(?:[:;]00)?$/, '$1$2').replace(/^(TC \d\d:\d\d:\d\d)[:;]\d\d$/, '$1');
  return (
    <span className={`sch-badge text ${ms < 600_000 ? 'soon' : ''}`} title={`${title} · ${entry.createdBy}${more > 0 ? ` (+${more})` : ''}`} onClick={toggleScheduleList}>
      ⏱ {entry.action === 'out' && 'OUT '}
      {short}
    </span>
  );
}

type Mode = 'at' | 'in' | 'timecode';

/** "HH:MM:SS" (or "HH:MM"). */
const AT = /^([01]?\d|2[0-3]):([0-5]\d)(?::([0-5]\d))?$/;
/** "mm:ss", or seconds. */
const IN = /^(?:(\d{1,4}):([0-5]\d)|(\d+))$/;
/** "HH:MM:SS:FF" (";" or "." before the frames too). */
const TC = /^([01]?\d|2[0-3]):([0-5]\d):([0-5]\d)[:;.](\d{1,2})$/;

/** Programs a take or an out of one graphic. */
function ScheduleEditor({ channelId, item, at, onClose }: { channelId: string; item: RundownItem; at: DOMRect; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const tz = useApp((s) => s.timezone);
  const offset = useApp((s) => s.serverOffset);
  const toast = useApp((s) => s.toast);
  const rate = useApp((s) => s.channels.find((c) => c.id === channelId)?.frameRate) ?? { num: 25, den: 1 };
  const [n, drop] = tcRate(rate);
  // The next whole minute, to start from.
  const [start] = useState(() => {
    const d = msOfDay(tz, clientNow() + offset);
    return Math.ceil((d + 1) / 60_000) * 60_000;
  });
  const [action, setAction] = useState<'take' | 'out'>('take');
  const [mode, setMode] = useState<Mode>('at');
  const [repeat, setRepeat] = useState<'once' | 'daily'>('once');
  const [values, setValues] = useState<Record<Mode, string>>(() => ({ at: hms(start), in: '00:30', timecode: `${hms(start)}${drop ? ';' : ':'}00` }));
  const [busy, setBusy] = useState(false);
  const value = values[mode].trim();
  const when = ((): ({ kind: 'at'; time: string } | { kind: 'in'; secs: number } | { kind: 'timecode'; tc: string }) | null => {
    if (mode === 'at') {
      const m = AT.exec(value);
      return m ? { kind: 'at', time: `${two(+m[1])}:${m[2]}:${m[3] ?? '00'}` } : null;
    }
    if (mode === 'in') {
      const m = IN.exec(value);
      const secs = m ? (m[3] !== undefined ? +m[3] : +m[1] * 60 + +m[2]) : 0;
      return secs > 0 ? { kind: 'in', secs } : null;
    }
    const m = TC.exec(value);
    return m && +m[4] < n ? { kind: 'timecode', tc: `${two(+m[1])}:${m[2]}:${m[3]}${drop ? ';' : ':'}${two(+m[4])}` } : null;
  })();

  useEffect(() => {
    const down = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onClose();
    const key = (e: KeyboardEvent) => e.key === 'Escape' && (e.stopPropagation(), onClose());
    window.addEventListener('pointerdown', down);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', down);
      window.removeEventListener('keydown', key, true);
    };
  }, [onClose]);

  const submit = async () => {
    if (!when || busy) return;
    setBusy(true);
    try {
      await api('POST', `/api/channels/${encodeURIComponent(channelId)}/schedule`, { action, item: item.id, when, repeat });
      onClose();
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };

  const seg = <T extends string>(cur: T, set: (v: T) => void, opts: [T, string][]) => (
    <div className="sch-seg">
      {opts.map(([v, label]) => (
        <button key={v} className={cur === v ? 'on' : ''} onClick={() => set(v)}>
          {label}
        </button>
      ))}
    </div>
  );
  const top = Math.min(at.top - 8, window.innerHeight - 330);
  return createPortal(
    <div ref={ref} className="sch-pop" style={{ left: Math.min(at.right + 8, window.innerWidth - 312), top: Math.max(8, top) }} onClick={(e) => e.stopPropagation()}>
      <b className="sch-pop-title">{t('Schedule · {name}', { name: item.name })}</b>
      {seg(action, setAction, [
        ['take', 'TAKE'],
        ['out', 'OUT'],
      ])}
      {seg(mode, setMode, [
        ['at', t('At the time')],
        ['in', t('In…')],
        ['timecode', t('Timecode')],
      ])}
      <input
        className={`sch-time ${when ? '' : 'bad'}`}
        autoFocus
        spellCheck={false}
        value={values[mode]}
        placeholder={mode === 'at' ? 'HH:MM:SS' : mode === 'in' ? 'mm:ss' : 'HH:MM:SS:FF'}
        onChange={(e) => setValues({ ...values, [mode]: e.target.value })}
        onKeyDown={(e) => e.key === 'Enter' && void submit()}
      />
      {seg(repeat, setRepeat, [
        ['once', t('Once')],
        ['daily', t('Every day')],
      ])}
      <div className="sch-pop-foot">
        <button className="btn" onClick={onClose}>
          {t('Cancel')}
        </button>
        <button className="btn primary" disabled={!when || busy} onClick={() => void submit()}>
          {t('Schedule')}
        </button>
      </div>
    </div>,
    document.body,
  );
}

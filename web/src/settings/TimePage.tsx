import { useEffect, useRef, useState } from 'react';
import { CircleCheck, CircleX, Clock, Globe, Info, RefreshCw, Server, X } from 'lucide-react';
import { t } from '../i18n';
import { api, useApp } from '../store';
import { emptyTime, type HardwareInfo, type ServerConfig, type TimeConfig, type TimeState } from '../outputs/types';
import { Card, Field } from './ui';
import '../hardware/hardware.css';

/*
 * Settings › Time: the server's time of day (clocks, scheduled takes, logs)
 * and how the machine keeps it (NTP, or PTP when phc2sys sets the clock).
 * The frames' timing is Settings › Video sync.
 */
export function TimePage({
  cfg,
  edit,
  det,
  zone,
  timezones,
  onRefresh,
}: {
  cfg: ServerConfig;
  edit: (fn: (c: ServerConfig) => void) => void;
  det: HardwareInfo | null;
  /** The server's own time zone. */
  zone: string;
  timezones: string[];
  onRefresh: () => void;
}) {
  const toast = useApp((s) => s.toast);
  const time = { ...emptyTime(), ...cfg.time };
  const st = det?.sync;
  const [state, setState] = useState<TimeState | null>(null);
  // Server time − this computer's time, from GET /api/system/time.
  const skew = useRef(0);
  const [, tick] = useState(0);
  const [syncing, setSyncing] = useState(false);
  const [draft, setDraft] = useState('');

  const load = () => {
    const sent = Date.now();
    return api<TimeState>('GET', '/api/system/time')
      .then((s) => {
        skew.current = s.now - (sent + Date.now()) / 2;
        setState(s);
      })
      .catch(() => {});
  };
  useEffect(() => {
    void load();
    const poll = setInterval(() => void load(), 5000);
    const clock = setInterval(() => tick((n) => n + 1), 250);
    return () => {
      clearInterval(poll);
      clearInterval(clock);
    };
  }, []);

  const setTime = (fn: (t: TimeConfig) => void) =>
    edit((c) => {
      c.time = { ...emptyTime(), ...c.time };
      fn(c.time);
    });
  const addServer = () => {
    const s = draft.trim();
    if (!s || time.ntpServers.some((x) => x.toLowerCase() === s.toLowerCase())) return setDraft('');
    setTime((tc) => void tc.ntpServers.push(s));
    setDraft('');
  };
  const syncNow = async () => {
    setSyncing(true);
    try {
      await api('POST', '/api/system/time/sync');
      setTimeout(() => {
        void load();
        onRefresh();
      }, 6000);
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setTimeout(() => setSyncing(false), 6000);
    }
  };

  const tz = cfg.timezone.trim() || zone;
  const now = new Date(Date.now() + skew.current);
  const fmt = (o: Intl.DateTimeFormatOptions) => {
    try {
      return new Intl.DateTimeFormat(undefined, { ...o, timeZone: tz }).format(now);
    } catch {
      return new Intl.DateTimeFormat(undefined, o).format(now);
    }
  };
  const ptpTime = !!st?.ptp.setsClock;
  const ntpState = !st || st.ntp.synced == null ? 'unknown' : st.ntp.synced ? 'ok' : 'bad';
  const supported = state?.supported ?? false;
  const last = state?.last;

  return (
    <div className="net-page">
      <Card
        icon={<Clock size={16} />}
        title={t("Server's time")}
        actions={
          <button className="btn small" onClick={onRefresh}>
            <RefreshCw size={13} /> {t('Refresh')}
          </button>
        }
      >
        <div className="time-now">
          <span className="time-clock">{fmt({ hour: '2-digit', minute: '2-digit', second: '2-digit' })}</span>
          <span className="muted">
            {fmt({ weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })} · {tz}
          </span>
        </div>
        <div className="hw-detect">
          {ptpTime ? (
            <div className={`hw-det-item ${st?.ptp.locked ? 'ok' : 'warn'}`}>
              {st?.ptp.locked ? <CircleCheck size={16} /> : <CircleX size={16} />}
              <div>
                <strong>{st?.ptp.locked ? t('Set by PTP') : t('Set by PTP, not locked')}</strong>
                <span>{[st?.ptp.grandmaster && t('grandmaster {id}', { id: st.ptp.grandmaster }), st?.ptp.offsetNs != null && t('offset {ns} ns', { ns: Math.round(st.ptp.offsetNs) })].filter(Boolean).join(' · ') || '—'}</span>
              </div>
            </div>
          ) : (
            <div className={`hw-det-item ${ntpState === 'ok' ? 'ok' : 'warn'}`}>
              {ntpState === 'ok' ? <CircleCheck size={16} /> : ntpState === 'bad' ? <CircleX size={16} /> : <Info size={16} />}
              <div>
                <strong>{st?.ntp.enabled === false ? t('NTP off') : ntpState === 'ok' ? t('Synchronised (NTP)') : ntpState === 'bad' ? t('Not synchronised (NTP)') : t('NTP status unknown')}</strong>
                <span>{[st?.ntp.source && t('source {source}', { source: st.ntp.source }), st?.ntp.offsetMs != null && t('offset {ms} ms', { ms: st.ntp.offsetMs.toFixed(3) }), st?.ntp.tool && t('read with {tool}', { tool: st.ntp.tool })].filter(Boolean).join(' · ') || '—'}</span>
              </div>
            </div>
          )}
        </div>
      </Card>

      <Card
        icon={<Server size={16} />}
        title={t('NTP servers')}
        actions={
          <>
            <button className="btn small" disabled={!supported || !time.ntp || syncing} onClick={() => void syncNow()}>
              <RefreshCw size={13} className={syncing ? 'spin' : undefined} /> {t('Synchronise now')}
            </button>
            <label className="switch" title={t('NTP')}>
              <input type="checkbox" checked={time.ntp} disabled={!supported} onChange={(e) => setTime((tc) => void (tc.ntp = e.target.checked))} />
            </label>
          </>
        }
      >
        <div className={`ntp-servers ${!supported || !time.ntp ? 'off' : ''}`}>
          {time.ntpServers.map((s, i) => (
            <div key={i} className="ntp-server">
              <input value={s} disabled={!supported} aria-label={t('NTP server')} onChange={(e) => setTime((tc) => void (tc.ntpServers[i] = e.target.value))} />
              <button type="button" className="icon-btn ghost" disabled={!supported} title={t('Remove')} onClick={() => setTime((tc) => void tc.ntpServers.splice(i, 1))}>
                <X size={13} />
              </button>
            </div>
          ))}
          <div className="ntp-server">
            <input
              value={draft}
              disabled={!supported}
              placeholder={time.ntpServers.length ? t('Add a server') : t("The system's servers · add one")}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={addServer}
              onKeyDown={(e) => e.key === 'Enter' && addServer()}
            />
          </div>
          {!supported && state && <span className="hw-note warn">{t('The time cannot be set on this machine')}</span>}
          {supported && last && (
            <span className={`hw-note ${last.ok ? '' : 'warn'}`}>
              {last.ok ? <CircleCheck size={13} /> : <CircleX size={13} />} {new Date(last.at).toLocaleTimeString()} · {last.message}
            </span>
          )}
        </div>
      </Card>

      <Card icon={<Globe size={16} />} title={t('Time zone')}>
        <div className="set-grid">
          <Field label={t('Clocks and schedule')}>
            <input list="tz-list" value={cfg.timezone} placeholder={zone} onChange={(e) => edit((c) => void (c.timezone = e.target.value))} />
            <datalist id="tz-list">
              {timezones.map((z) => (
                <option key={z} value={z} />
              ))}
            </datalist>
          </Field>
        </div>
      </Card>
    </div>
  );
}

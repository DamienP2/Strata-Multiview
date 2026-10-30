import { TriangleAlert, WifiOff } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useMetrics } from '../engine/metrics';
import { t, tn } from '../i18n';
import { bytes, pct } from '../perf/format';
import { navigate } from '../router';
import { api, can, useApp, type AuthState } from '../store';

/** Above this machine load (%), the pill shows the CPU instead of « Connected ». */
const HOT = 85;
/** …and it goes back under this one. */
const COOL = 80;

/** Lost: the socket closed after it was open (or never opened within a few seconds). */
function useLost(connected: boolean): boolean {
  const [lost, setLost] = useState(false);
  const ever = useRef(false);
  useEffect(() => {
    if (connected) {
      ever.current = true;
      setLost(false);
      return;
    }
    const h = setTimeout(() => setLost(true), ever.current ? 800 : 4000);
    // Back up but restarted (sessions are not kept): the login page, not « lost » for ever.
    const ask = setInterval(() => {
      api<AuthState>('GET', '/api/auth/state')
        .then((auth) => !auth.user && useApp.setState({ auth }))
        .catch(() => {});
    }, 3000);
    return () => (clearTimeout(h), clearInterval(ask));
  }, [connected]);
  return lost && !connected;
}

/**
 * The server in a top bar, in one pill: « Connected » with a green dot; the
 * CPU when the machine is loaded (orange); « Server lost » when the socket
 * is gone (red). Hover: the machine; a click opens Performance (admins).
 */
export function StatusPill({ newTab }: { newTab?: boolean }) {
  const connected = useApp((s) => s.connected);
  const serverName = useApp((s) => s.auth?.serverName);
  const admin = useApp((s) => can.admin(s.auth?.user));
  const m = useMetrics();
  const lost = useLost(connected);
  const cpu = m?.machine.cpu;
  // Held a little: it does not blink around the threshold.
  const [hot, setHot] = useState(false);
  if (cpu !== undefined) {
    const next = hot ? cpu >= COOL : cpu > HOT;
    if (next !== hot) setHot(next);
  }

  const state = lost ? 'lost' : !connected ? 'wait' : hot ? 'hot' : 'ok';
  let label: React.ReactNode;
  if (state === 'lost')
    label = (
      <>
        <WifiOff size={13} />
        <span className="ux1-lost-txt">
          <b>{t('Server lost')}</b>
          <span className="ux1-lost-sep"> · </span>
          <span>{t('reconnecting…')}</span>
        </span>
      </>
    );
  else if (state === 'hot')
    label = (
      <>
        <TriangleAlert size={13} /> CPU {pct(cpu ?? 0)}
      </>
    );
  else
    label = (
      <>
        <i className="ux1-dot" /> {state === 'ok' ? t('Connected') : t('Connecting…')}
      </>
    );

  const lines = [serverName || 'Strata', state === 'lost' ? t('Server lost · reconnecting…') : connected ? t('Connected to engine') : t('Connecting…')];
  if (m && !lost) {
    const { machine } = m;
    lines.push(`CPU ${pct(machine.cpu)} · ${tn(machine.cores, '{n} core', '{n} cores')}`);
    lines.push(`${t('Memory')} ${t('{used} of {total}', { used: bytes(machine.memUsed), total: bytes(machine.memTotal) })}`);
    if (machine.hostname && machine.hostname !== serverName) lines.push(machine.hostname);
  }
  const title = lines.join('\n');
  const cls = `ux1-pill ${state}`;

  if (!admin)
    return (
      <span className={cls} role="status" aria-live="polite" title={title}>
        {label}
      </span>
    );
  return (
    <button
      className={cls}
      aria-live="polite"
      title={title}
      onClick={(e) => {
        if (newTab || e.metaKey || e.ctrlKey) window.open('/performance', '_blank', 'noopener');
        else navigate('/performance');
      }}
    >
      {label}
    </button>
  );
}

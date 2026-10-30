import { ArrowUpRight, TriangleAlert } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { loadTone, useMetrics, type ChannelMetrics, type Metrics } from '../engine/metrics';
import { t, tn } from '../i18n';
import { navigate } from '../router';
import { can, useApp } from '../store';
import { Bar, Dot } from '../perf/bits';

/** Just the usage of the whole machine (inside a project): no details, no link. */
export function UsageMeter() {
  const m = useMetrics();
  const cpu = m?.machine.cpu ?? 0;
  const tone = m ? loadTone(cpu) : 'idle';
  return (
    <div className="load-meter" role="meter" aria-label={t('Machine usage')} aria-valuenow={Math.round(cpu)} aria-valuemin={0} aria-valuemax={100}>
      <span className="lm-trigger lm-static">
        <span className="lm-label">CPU</span>
        <Bar value={cpu} tone={tone} className="lm-bar" />
        <span className={`lm-val tone-${tone}`}>{m ? pct(cpu) : '—'}</span>
      </span>
    </div>
  );
}
import { bytes, channelTone, fps, fpsTone, memTone, pct, pictureTone, worstTone } from '../perf/format';

/*
 * The usage of the machine in a top bar: a thin bar filled to the CPU % of
 * the whole machine. Hover or click shows the details: memory, Strata and
 * its ffmpeg, and the channels (those of one project when `projectId` or
 * `channelIds` is given) with the frame rate and render load of their pictures.
 */
export function LoadMeter({ projectId, channelIds, newTab }: { projectId?: string; channelIds?: string[]; newTab?: boolean }) {
  const m = useMetrics();
  const admin = useApp((s) => can.admin(s.auth?.user));
  const [open, setOpen] = useState<'hover' | 'pin' | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const hover = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (open !== 'pin') return;
    const down = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && setOpen(null);
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(null);
    addEventListener('pointerdown', down);
    addEventListener('keydown', key);
    return () => {
      removeEventListener('pointerdown', down);
      removeEventListener('keydown', key);
    };
  }, [open]);
  useEffect(() => () => void (hover.current && clearTimeout(hover.current)), []);

  const later = (fn: () => void, ms: number) => {
    if (hover.current) clearTimeout(hover.current);
    hover.current = setTimeout(fn, ms);
  };

  const scoped = projectId !== undefined || channelIds !== undefined;
  const chans = m ? m.channels.filter((c) => (channelIds ? channelIds.includes(c.id) : projectId ? c.projectId === projectId : true)) : [];
  const cpu = m?.machine.cpu ?? 0;
  const tone = m ? loadTone(cpu) : 'idle';
  // Something wrong here: a struggling channel of this project, or any warning.
  const trouble = !m ? null : scoped ? worstTone('ok', ...chans.map(channelTone)) : m.warnings.some((w) => w.level === 'error') ? 'bad' : m.warnings.some((w) => w.level === 'warn') ? 'warn' : null;

  return (
    <div
      ref={ref}
      className="load-meter"
      onMouseEnter={() => later(() => setOpen((o) => o ?? 'hover'), 120)}
      onMouseLeave={() => later(() => setOpen((o) => (o === 'hover' ? null : o)), 180)}
    >
      <button
        className={`lm-trigger ${open ? 'on' : ''}`}
        aria-expanded={!!open}
        aria-label={t('Machine usage')}
        onClick={() => setOpen((o) => (o === 'pin' ? null : 'pin'))}
      >
        <span className="lm-label">CPU</span>
        <Bar value={cpu} tone={tone} className="lm-bar" />
        <span className={`lm-val tone-${tone}`}>{m ? pct(cpu) : '—'}</span>
        {trouble && trouble !== 'ok' && <span className={`lm-alert tone-${trouble}`} />}
      </button>
      {open && m && <MeterDetails m={m} chans={chans} scoped={scoped} admin={admin} newTab={newTab} />}
    </div>
  );
}

function MeterDetails({ m, chans, scoped, admin, newTab }: { m: Metrics; chans: ChannelMetrics[]; scoped: boolean; admin: boolean; newTab?: boolean }) {
  const { machine, process: p } = m;
  const memPct = machine.memTotal ? (machine.memUsed / machine.memTotal) * 100 : 0;
  const warns = m.warnings.filter((w) => w.level !== 'info').length;
  return (
    <div className="lm-pop" role="dialog" aria-label={t('Machine usage')}>
      <div className="lm-head">
        <strong>{machine.hostname}</strong>
        <span className="muted">
          {machine.cpuName} · {tn(machine.cores, '{n} core', '{n} cores')}
        </span>
      </div>
      <div className="lm-rows">
        <span className="lm-k">{t('Processors')}</span>
        <Bar value={machine.cpu} tone={loadTone(machine.cpu)} />
        <span className="lm-v">{pct(machine.cpu)}</span>
        <span className="lm-k">{t('Memory')}</span>
        <Bar value={memPct} tone={memTone(memPct)} />
        <span className="lm-v">{t('{used} of {total}', { used: bytes(machine.memUsed), total: bytes(machine.memTotal) })}</span>
      </div>
      <dl className="lm-list">
        <dt>Strata</dt>
        <dd>
          {pct(p.cpu)} CPU · {bytes(p.mem)}
        </dd>
        <dt>ffmpeg</dt>
        <dd>{p.ffmpeg ? `${pct(p.ffmpegCpu)} CPU · ${tn(p.ffmpeg, '{n} process', '{n} processes')}` : t('not running')}</dd>
      </dl>

      <div className="lm-section">{scoped ? t('Channels of this project') : t('Channels')}</div>
      {chans.length === 0 ? (
        <p className="lm-empty">{scoped ? t('No channel plays this project.') : t('No channel.')}</p>
      ) : (
        chans.map((c) => (
          <div key={c.id} className="lm-channel">
            <div className="lm-ch-head">
              <Dot tone={channelTone(c)} />
              <strong>{c.name}</strong>
              <span className="muted">{fps(c.rate)} fps</span>
              <span className="spacer" />
              {c.onAir > 0 ? <span className="lm-air">{t('{n} on air', { n: c.onAir })}</span> : <span className="muted">{t('Off air')}</span>}
            </div>
            {c.pictures.map((pic) => {
              const tone = pictureTone(pic, c.rate);
              return (
                <div key={pic.id} className="lm-pic" title={`${pic.width}×${pic.height}`}>
                  <span className="lm-pic-name">{pic.name}</span>
                  <span className={`lm-fps tone-${fpsTone(pic.fps, c.rate)}`}>
                    {fps(pic.fps)}/{fps(c.rate)}
                  </span>
                  <Bar value={pic.load} tone={tone === 'idle' ? 'ok' : loadTone(pic.load)} />
                  <span className="lm-v">{pct(pic.load)}</span>
                </div>
              );
            })}
          </div>
        ))
      )}

      {(admin || warns > 0) && (
        <div className="lm-foot">
          {warns > 0 && (
            <span className="lm-warns">
              <TriangleAlert size={13} /> {tn(warns, '{n} warning', '{n} warnings')}
            </span>
          )}
          <span className="spacer" />
          {admin && (
            <a
              href="/performance"
              target={newTab ? '_blank' : undefined}
              rel="noreferrer"
              onClick={(e) => {
                if (newTab || e.metaKey || e.ctrlKey) return;
                e.preventDefault();
                navigate('/performance');
              }}
            >
              {t('Performance')} <ArrowUpRight size={13} />
            </a>
          )}
        </div>
      )}
    </div>
  );
}

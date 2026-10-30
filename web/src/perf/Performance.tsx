import { Activity, CircleCheck, CircleX, Cpu, Film, Gauge, Image as ImageIcon, Info, Lightbulb, MemoryStick, Network, Radio, RadioTower, TriangleAlert, Tv, Video } from 'lucide-react';
import { useState } from 'react';
import { loadTone, useMetrics, type CapacityItem, type ChannelMetrics, type InputMetrics, type Metrics, type PictureMetrics, type Warning } from '../engine/metrics';
import { t, tc, tn } from '../i18n';
import { NavItem, NavSection, PageHead, SidePage } from '../pages/SidePage';
import { Card } from '../settings/ui';
import { useApp } from '../store';
import { Logo } from '../ui/Logo';
import { Bar, Dot } from './bits';
import { bytes, channelTone, corePct, cores, coresLabel, fps, fpsTone, inputTone, machinePct, memTone, num, outputTone, pct, pictureTone, type Tone } from './format';
// The badges of the Video streams page (its stylesheet may not be loaded yet).
import '../outputs/streams.css';

/*
 * The Performance page (admins): what the machine, each channel's pictures
 * and outputs and the live inputs use; how many more outputs and inputs the
 * server can take (the server's log is in System › Health).
 */

type Section = 'overview' | 'capacity';

/** Colours of the Video streams page. */
const KIND: Record<string, { label: string; badge: string; color: string; icon: typeof Tv }> = {
  picture: { label: 'Rendered picture', badge: 'IMG', color: '#38bdf8', icon: ImageIcon },
  stream: { label: 'Stream output (SRT, UDP, RTMP, HLS)', badge: 'STREAM', color: '#fb7185', icon: RadioTower },
  ndi: { label: 'NDI output', badge: 'NDI', color: '#14b8a6', icon: Radio },
  st2110: { label: 'ST 2110 output', badge: 'ST 2110', color: '#e879f9', icon: Network },
  sdi: { label: 'SDI output', badge: 'SDI', color: '#f5a524', icon: Tv },
  inputStream: { label: 'Stream input (SRT, UDP, RTMP, HLS…)', badge: 'IN', color: '#a78bfa', icon: Video },
  inputNdi: { label: 'NDI input', badge: 'NDI', color: '#a78bfa', icon: Radio },
};
const INPUT_COLOR = '#a78bfa';
const kindOf = (k: string) => KIND[k] ?? { label: k, badge: k.toUpperCase(), color: '#8b90aa', icon: Activity };

export function Performance() {
  const m = useMetrics(1000);
  const [section, setSection] = useState<Section>('overview');
  const item = (s: Section, icon: React.ReactNode, label: string, extra?: Partial<React.ComponentProps<typeof NavItem>>) => <NavItem on={section === s} icon={icon} label={label} onClick={() => setSection(s)} {...extra} />;
  const warns = m?.warnings.filter((w) => w.level !== 'info').length ?? 0;

  const nav = (
    <>
      <NavSection>{t('Performance')}</NavSection>
      {item('overview', <Gauge size={15} />, tc('perf', 'Overview'), { status: m ? (warns ? 'warn' : 'ok') : undefined, count: warns || undefined })}
      {item('capacity', <Lightbulb size={15} />, t('Capacity'))}
      {m && (
        <div className="lib-nav-foot">
          <Dot tone={loadTone(m.machine.cpu)} /> {m.machine.hostname}
          <span className="muted">
            {m.machine.cpuName} · {tn(m.machine.cores, '{n} core', '{n} cores')} · {bytes(m.machine.memTotal)}
          </span>
        </div>
      )}
    </>
  );

  return (
    <SidePage nav={nav} className="perf-page">
      {/* The CPU shows once, in the tiles below. */}
      <PageHead title={t('Performance')} sub={m ? `${m.machine.hostname} · ${m.machine.cpuName} · ${tn(m.machine.cores, '{n} core', '{n} cores')}` : t('Measuring…')} />
      {!m ? <div className="loading">{t('Measuring…')}</div> : section === 'overview' ? <Overview m={m} /> : <CapacityView m={m} />}
    </SidePage>
  );
}

// ---------- overview ----------

function Overview({ m }: { m: Metrics }) {
  const { machine, process: p } = m;
  const memPct = machine.memTotal ? (machine.memUsed / machine.memTotal) * 100 : 0;
  // CPU in % of one core, with the number of cores.
  const n = Math.max(1, machine.cores);
  const coreCount = tn(n, '{n} core', '{n} cores');
  return (
    <>
      <div className="perf-tiles">
        <Tile icon={<Cpu size={15} />} label={t('Processors')} value={machinePct(machine.cpu, n)} unit={coreCount} tone={loadTone(machine.cpu)} bar={machine.cpu} sub={tn((machine.cpu * n) / 100, 'of {max} ({n} core in use)', 'of {max} ({n} cores in use)', { max: pct(n * 100), n: cores((machine.cpu * n) / 100) })} />
        <Tile icon={<MemoryStick size={15} />} label={t('Memory')} value={bytes(machine.memUsed)} tone={memTone(memPct)} bar={memPct} sub={t('of {total} · {pct}', { total: bytes(machine.memTotal), pct: pct(memPct) })} />
        <Tile icon={<Logo size={15} />} label="Strata" value={machinePct(p.cpu, n)} unit={coreCount} sub={t('{mem} of memory', { mem: bytes(p.mem) })} />
        <Tile icon={<Film size={15} />} label="ffmpeg" value={p.ffmpeg ? machinePct(p.ffmpegCpu, n) : '—'} unit={p.ffmpeg ? coreCount : undefined} sub={p.ffmpeg ? tn(p.ffmpeg, '{n} process (streams in and out)', '{n} processes (streams in and out)') : t('not running')} />
      </div>

      <Warnings list={m.warnings} cores={n} />

      {m.channels.map((c) => (
        <ChannelCard key={c.id} c={c} />
      ))}

      <InputsCard inputs={m.inputs} />
    </>
  );
}

function Tile({ icon, label, value, unit, sub, tone, bar }: { icon: React.ReactNode; label: string; value: string; unit?: string; sub: string; tone?: Tone; bar?: number }) {
  return (
    <div className="perf-tile">
      <span className="perf-tile-k">
        {icon} {label}
      </span>
      <span className={`perf-tile-v ${tone ? `tone-${tone}` : ''}`}>
        {value}
        {unit && <small className="ux3-cores">· {unit}</small>}
      </span>
      {bar !== undefined && <Bar value={bar} tone={tone ?? 'ok'} />}
      <span className="perf-tile-sub">{sub}</span>
    </div>
  );
}

const WARN_ICON = { error: CircleX, warn: TriangleAlert, info: Info };

/** A warning's text; the machine's CPU in % of one core, like the tiles. */
function warningText(w: Warning, n: number): string {
  const cpu = Number(w.vars?.cpu);
  if (w.text === 'The machine is at {cpu} % of its processors' && Number.isFinite(cpu)) return t('The machine uses {cpu} of {max} ({cores})', { cpu: machinePct(cpu, n), max: pct(n * 100), cores: tn(n, '{n} core', '{n} cores') });
  return t(w.text, translatedVars(w.vars));
}

function Warnings({ list, cores: n }: { list: Warning[]; cores: number }) {
  return (
    <Card icon={<TriangleAlert size={16} />} title={t('Warnings')} actions={list.length > 0 && <span className="count">{list.length}</span>}>
      {list.length === 0 ? (
        <p className="perf-allgood">
          <CircleCheck size={15} /> {t('Everything runs smoothly.')}
        </p>
      ) : (
        <ul className="perf-warnings">
          {list.map((w, i) => {
            const Icon = WARN_ICON[w.level] ?? Info;
            return (
              // The same problem keeps its place from one second to the next.
              <li key={`${w.text}|${JSON.stringify(w.vars)}|${i}`} className={`lv-${w.level}`}>
                <Icon size={14} />
                <span>{warningText(w, n)}</span>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}

function ChannelCard({ c }: { c: ChannelMetrics }) {
  const project = useApp((s) => s.projects.find((p) => p.id === c.projectId)?.name);
  const frame = c.rate ? 1000 / c.rate : 40;
  const jitterTone: Tone = c.jitterMs > frame / 2 ? 'bad' : c.jitterMs > frame / 5 ? 'warn' : 'ok';
  return (
    <section className="set-card perf-channel">
      <header>
        <Tv size={16} />
        <div className="perf-ch-title">
          <h2>{c.name}</h2>
          <span className="muted">{project ? t('plays “{name}”', { name: project }) : t('no project loaded')}</span>
        </div>
        <div className="spacer" />
        <span className="perf-chip">{fps(c.rate)} fps</span>
        <span className={`perf-chip tone-${jitterTone}`} title={t('How late the clock ticked at most (last 10 s)')}>
          {t('Clock: {ms} ms late at most', { ms: num(c.jitterMs, 1) })}
        </span>
        <span className={`perf-chip ${c.onAir ? 'live' : ''}`}>{c.onAir ? t('{n} on air', { n: c.onAir }) : t('Off air')}</span>
        <Dot tone={channelTone(c)} />
      </header>
      <div className="set-body">
        {c.pictures.length === 0 ? <p className="muted perf-none">{t('Nothing is rendered on this channel.')}</p> : c.pictures.map((p) => <PictureBlock key={p.id} p={p} rate={c.rate} />)}
      </div>
    </section>
  );
}

function PictureBlock({ p, rate }: { p: PictureMetrics; rate: number }) {
  const k = KIND.picture;
  return (
    <div className="perf-pic">
      <div className="perf-pic-head">
        <span className="perf-icon" style={{ ['--k' as string]: k.color }}>
          <ImageIcon size={14} />
        </span>
        <strong>{p.name}</strong>
        <span className="muted">
          {p.width}×{p.height}
        </span>
        <span className="spacer" />
        <Dot tone={pictureTone(p, rate)} />
      </div>
      <div className="perf-stats">
        <Stat label={t('Frame rate')} tone={fpsTone(p.fps, rate)}>
          {fps(p.fps)} <small>/ {fps(rate)} fps</small>
        </Stat>
        <Stat label={t('Render time')}>
          {num(p.renderMs, 1)} <small>{t('ms avg · {max} max', { max: num(p.renderMaxMs, 1) })}</small>
        </Stat>
        <Stat label={t('Render load')} tone={loadTone(p.load)} bar={p.load}>
          {pct(p.load)} <small>{t('of the frame time')}</small>
        </Stat>
        <Stat label={t('Frames shown twice (10 s)')} tone={p.repeated ? 'warn' : 'ok'}>
          {num(p.repeated)}
        </Stat>
      </div>
      {p.outputs.length > 0 && (
        <div className="perf-table-wrap">
          <table className="perf-table">
            <thead>
              <tr>
                <th>{t('Output')}</th>
                <th>{t('Status')}</th>
                <th className="num">FPS</th>
                <th className="num">{t('Dropped (10 s)')}</th>
                <th className="num">CPU</th>
              </tr>
            </thead>
            <tbody>
              {p.outputs.map((o) => {
                const kd = kindOf(o.kind);
                const tone = outputTone(o, rate);
                return (
                  <tr key={o.id}>
                    <td>
                      <span className="perf-name">
                        <span className="vs-badge" style={{ ['--k' as string]: kd.color }}>
                          {kd.badge}
                        </span>
                        <span>{o.name}</span>
                      </span>
                    </td>
                    <td>
                      <span className="perf-status" title={o.message}>
                        <Dot tone={o.ok ? (o.dropped ? 'warn' : 'ok') : 'bad'} />
                        <span>{o.ok && (!o.message || o.message === 'Running') ? t('Running') : o.message}</span>
                      </span>
                    </td>
                    <td className={`num tone-${o.ok ? fpsTone(o.fps, rate) : 'idle'}`}>{fps(o.fps)}</td>
                    <td className={`num ${o.dropped ? `tone-${tone}` : 'dim0'}`}>{num(o.dropped)}</td>
                    <td className="num">{corePct(o.cpu)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function Stat({ label, tone, bar, children }: { label: string; tone?: Tone; bar?: number; children: React.ReactNode }) {
  return (
    <div className="perf-stat">
      <span className="perf-stat-k">{label}</span>
      <span className={`perf-stat-v ${tone ? `tone-${tone}` : ''}`}>{children}</span>
      {bar !== undefined && <Bar value={bar} tone={tone ?? 'ok'} />}
    </div>
  );
}

function InputsCard({ inputs }: { inputs: InputMetrics[] }) {
  return (
    <Card icon={<Video size={16} />} title={t('Inputs')} actions={<span className="count">{inputs.length}</span>}>
      {inputs.length === 0 ? (
        <p className="muted perf-none">{t('No live input.')}</p>
      ) : (
        <div className="perf-table-wrap">
          <table className="perf-table">
            <thead>
              <tr>
                <th>{t('Input')}</th>
                <th>{t('Status')}</th>
                <th className="num">{t('FPS received')}</th>
                <th className="num">{t('Buffer')}</th>
                <th className="num">{t('Dropped (10 s)')}</th>
                <th className="num">CPU</th>
              </tr>
            </thead>
            <tbody>
              {inputs.map((i) => (
                <tr key={i.id}>
                  <td>
                    <span className="perf-name">
                      <span className="vs-badge" style={{ ['--k' as string]: INPUT_COLOR }}>
                        {i.kind === 'url' ? 'URL' : i.kind.toUpperCase()}
                      </span>
                      <span>{i.name}</span>
                    </span>
                  </td>
                  <td>
                    <span className="perf-status">
                      <Dot tone={inputTone(i)} />
                      <span>{i.live ? t('Receiving') : t('No signal')}</span>
                    </span>
                  </td>
                  <td className={`num tone-${i.live ? fpsTone(i.fps, i.nominal) : 'idle'}`}>
                    {fps(i.fps)} <small>/ {fps(i.nominal)}</small>
                  </td>
                  <td className="num">{num(i.bufferMs)} ms</td>
                  <td className={`num ${i.dropped ? 'tone-warn' : 'dim0'}`}>{num(i.dropped)}</td>
                  <td className="num">{corePct(i.cpu)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

// ---------- capacity ----------

function CapacityView({ m }: { m: Metrics }) {
  const c = m.capacity;
  const tone: Tone = c.used >= c.budget ? 'bad' : c.used >= c.budget * 0.75 ? 'warn' : 'ok';
  return (
    <>
      <p className="perf-intro">{t('Strata should use at most 80 % of the machine. Each cost is measured on this server when something of that kind runs, else a typical figure.')}</p>
      <section className="set-card perf-budget">
        <div className="perf-budget-head">
          <div>
            <span className="perf-tile-k">
              <Cpu size={15} /> {t('Processor budget')}
            </span>
            <span className={`perf-budget-v tone-${tone}`}>
              {cores(c.used)} <small>/ {coresLabel(c.budget)}</small>
            </span>
          </div>
          <div className="spacer" />
          <div className="perf-budget-free">
            <strong>{cores(Math.max(0, c.free))}</strong>
            <span className="muted">{t('cores free')}</span>
          </div>
        </div>
        <Bar value={c.used} max={m.machine.cores} mark={c.budget} tone={tone} className="big" />
        <div className="perf-budget-scale muted">
          <span>0</span>
          <span style={{ left: `${Math.min(100, (c.budget / (m.machine.cores || 1)) * 100)}%` }}>{t('budget {n}', { n: cores(c.budget) })}</span>
          <span>{tn(m.machine.cores, '{n} core', '{n} cores')}</span>
        </div>
      </section>

      <div className="perf-cap-grid">
        {c.items.map((it) => (
          <CapacityCard key={`${it.kind}|${it.format}`} it={it} />
        ))}
      </div>

      <div className="perf-note">
        <Info size={15} />
        <div>
          <p>{t('HTML5 outputs are drawn by the browser that shows them: they cost almost nothing on the server.')}</p>
          <p>{t('ST 2110 also needs network bandwidth: about 2.6 Gb/s per 1080p50 fill stream.')}</p>
        </div>
      </div>
    </>
  );
}

function CapacityCard({ it }: { it: CapacityItem }) {
  const k = kindOf(it.kind);
  const Icon = k.icon;
  const tone: Tone = it.more <= 0 ? 'bad' : it.more <= 1 ? 'warn' : 'ok';
  return (
    <div className="perf-cap" style={{ ['--k' as string]: k.color }}>
      <div className="perf-cap-head">
        <span className="perf-icon">
          <Icon size={15} />
        </span>
        <div className="grow">
          <strong>{t(k.label)}</strong>
          <span className="muted">{it.format}</span>
        </div>
      </div>
      <div className={`perf-cap-more tone-${tone}`}>{it.more > 0 ? withNumber(t('You can add about {n} more', { n: '\u0000' }), num(it.more)) : <span className="perf-cap-none">{t('No room for another one')}</span>}</div>
      <Bar value={it.now} max={Math.max(it.max, it.now + it.more, 1)} tone={tone} />
      <dl className="perf-cap-list">
        <dt>{tc('capacity', 'Now')}</dt>
        <dd>{num(it.now)}</dd>
        <dt>
          {t('Cost of one')} <span className={`perf-src ${it.measured ? 'measured' : ''}`}>{it.measured ? t('measured') : t('typical')}</span>
        </dt>
        <dd>{coresLabel(it.unit)}</dd>
        <dt>{t('This machine alone')}</dt>
        <dd>{t('up to {n}', { n: num(it.max) })}</dd>
      </dl>
    </div>
  );
}

/** A sentence with its number set large ("You can add about **13** more"). */
function withNumber(text: string, n: string) {
  const [a, b = ''] = text.split('\u0000');
  return (
    <>
      {a}
      <strong className="perf-cap-n">{n}</strong>
      {b}
    </>
  );
}

/** A reason given in a warning (`why`) is an English sentence too; names stay as they are. */
function translatedVars(vars: Record<string, string | number>): Record<string, string | number> {
  return typeof vars.why === 'string' ? { ...vars, why: t(vars.why) } : vars;
}

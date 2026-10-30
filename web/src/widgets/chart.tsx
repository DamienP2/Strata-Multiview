// Chart: settings, drawing, inspector and Operator controls.
// Same picture as crates/cg-render/src/wid/chart.rs; layout mirrored in chart-layout.tsx; actions in crates/cg-core/src/wid/chart.rs.
import { Dropdown } from '../ui/Dropdown';
import { HexColorField } from '../ui/fields';
import { ChartColumn, ChevronsRight, Pause, Play, RefreshCw, SkipBack, SkipForward, Square, Zap } from 'lucide-react';
import { num, type Resolved } from '../engine/evaluate';
import type { Data } from '../engine/types';
import { useFonts } from '../engine/fonts';
import { useApp } from '../store';
import { t, tc, tl, tn } from '../i18n';
import { GroupRow } from '../designer/Inspector';
import { ColorPick, Content, defineWidget, GridEditor, Num, RangeNum, Row, Section, Seg, Select, Text, Toggle, useBound, useCentreMode, useKeys, type Warning } from './kit';
import { ADVANCE_MODES, Centre, Countdown, Dots, GAP, WaitsContinue, WaitsTrigger, type AdvanceMode, type Line2Item } from './centre';
import { useLiveTime } from './table-text';
import { cfg, fmt, layout, parse, PAL, resolveGrid, revealMode, SAMPLE, seriesStyle, shownGrid, sliceColor, st, steps, withData, type Cfg, type Config as LConfig, type Hl, type Out, type Prim, type St, type State as LState } from './chart-layout';

/** Settings of the layer (flattened into it; every field optional). */
export type Config = LConfig;
/** What the operator changes on air (mirrors `State` in the Rust module). */
export type State = LState;

const SAMPLES: Record<string, { data: string[][]; axis: { suffix: string; decimals: number } }> = {
  poll: { data: SAMPLE, axis: { suffix: ' %', decimals: 0 } },
  budget: {
    data: [
      ['Year', 'Revenue', 'Costs', 'Balance'],
      ['2021', '42', '-38', '4'],
      ['2022', '45', '-47', '-2'],
      ['2023', '51', '-49', '2'],
      ['2024', '48', '-53', '-5'],
      ['2025', '56', '-50', '6'],
    ],
    axis: { suffix: ' M€', decimals: 0 },
  },
  share: {
    data: [
      ['Channel', 'Share'],
      ['Strata TV', '24.5'],
      ['TF Nord', '18.2'],
      ['Canal Est', '14.9'],
      ['Info 24', '9.1'],
      ['Sport+', '5.4'],
      ['Kids', '3.2'],
      ['Docs', '2.1'],
      ['Music', '1.4'],
      ['Local', '0.9'],
      ['Weather', '0.6'],
    ],
    axis: { suffix: ' %', decimals: 1 },
  },
};

/** Layout of the layer at a moment (both the stage and the warnings use it). */
function run(layer: Config, data: Data, state: unknown, w: number, h: number, fs: number, ageMs: number, now: number, lang: string): Out {
  const c0 = cfg(layer);
  const s = shownState(c0, st(state), now, ageMs / 1000, data);
  const grid = shownGrid(c0, s, data);
  return layout({ cfg: withData(c0, data), st: s, grid, w, h, fs, ageMs, now, lang });
}

/** The font families of the project (Assets), for the texts of the chart. */
function FamilyPick({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const families = useFonts((s) => s.families);
  const has = families.some((f) => f.family.toLowerCase() === value.toLowerCase());
  return <Dropdown value={value} options={[...(has ? [] : [{ value, label: value }]), ...families.map((f) => ({ value: f.family, label: f.family, style: { fontFamily: `"${f.family}", Inter, sans-serif` } }))]} onChange={onChange} />;
}

/** Highlight choices: none, each series (several), each column / point / slice. */
function hlOptions(c: ReturnType<typeof cfg>, P: ReturnType<typeof parse>): [string, string][] {
  return [['', t('None')], ...(c.chartType !== 'pie' && P.ser.length > 1 ? P.ser.map((x, j) => [`s${j}`, x] as [string, string]) : []), ...P.cats.map((x) => [`c${x.ri}`, x.name] as [string, string])];
}
const hlValue = (s: St) => (s.hl.kind === 'series' ? `s${s.hl.idx}` : s.hl.kind === 'category' ? `c${s.hl.idx}` : '');
const hlArgs = (v: string) => (!v || v === 'none' ? { kind: 'none' } : { kind: v[0] === 's' ? 'series' : 'category', idx: +v.slice(1) });

let seq = 0;
function Svg({ prims, w, h, font }: { prims: Prim[]; w: number; h: number; font: string }) {
  const id = `wch${(seq = (seq + 1) % 1e6)}`;
  const clips = new Map<string, number>();
  const clipOf = (c?: [number, number, number, number]) => {
    if (!c) return undefined;
    const k = c.join(',');
    if (!clips.has(k)) clips.set(k, clips.size);
    return `url(#${id}c${clips.get(k)})`;
  };
  const body = prims.map((p, i) => {
    if (p.kind === 'shape')
      return (
        <path
          key={i}
          d={p.d}
          clipPath={clipOf(p.clip)}
          fill={p.fill ?? 'none'}
          fillOpacity={p.fillOpacity}
          stroke={p.stroke ?? 'none'}
          strokeWidth={p.stroke ? p.width : undefined}
          strokeDasharray={p.dash?.join(' ')}
          strokeLinejoin={p.round ? 'round' : 'miter'}
          strokeLinecap={p.round ? 'round' : 'butt'}
          opacity={p.opacity}
        />
      );
    if (!p.text || p.opacity <= 0) return null;
    return (
      <text
        key={i}
        x={p.x}
        y={p.y}
        clipPath={clipOf(p.clip)}
        transform={p.rotate ? `rotate(-90 ${p.x} ${p.y})` : undefined}
        textAnchor={p.anchor}
        fontSize={p.size}
        fontWeight={p.weight}
        fill={p.fill}
        opacity={p.opacity}
        stroke={p.halo}
        strokeWidth={p.halo ? 5 : undefined}
        strokeLinejoin="round"
        paintOrder="stroke"
        style={{ whiteSpace: 'pre' }}
      >
        {p.text}
      </text>
    );
  });
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} style={{ position: 'absolute', inset: 0, overflow: 'visible', fontFamily: `"${font}", Inter, system-ui, sans-serif` }}>
      <defs>
        {[...clips].map(([k, n]) => {
          const [x, y, cw, ch] = k.split(',').map(Number);
          return (
            <clipPath key={n} id={`${id}c${n}`} clipPathUnits="userSpaceOnUse">
              <rect x={x} y={y} width={cw} height={ch} />
            </clipPath>
          );
        })}
      </defs>
      {body}
    </svg>
  );
}

const fsOf = (r: Resolved | undefined, layer: { props: Record<string, unknown> }) => num((r?.fontSize ?? layer.props.fontSize ?? 26) as number);

// ------------------------------------------------------------ chained steps --

const clampN = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));

/** The highlight written in the template (`hlDefault`): `s2` = series 3, `c0` = the first category; anything else = none (mirrors hl_of). */
export function hlOf(v: string): Hl {
  const s = v.trim();
  const idx = +s.slice(1);
  if (!/^[sc]\d+$/.test(s) || idx < 0) return { kind: 'none', idx: -1 };
  return { kind: s[0] === 's' ? 'series' : 'category', idx };
}

/** The state on air: the operator's once he acted, else the Designer's chained steps (from the take) and highlight (mirrors live). */
export function live(c: Cfg, s: St, now: number, ageSecs: number): St {
  if (s.ctl) return s;
  const o = { ...s };
  if (o.hl.kind === 'none' && !o.hlAt) {
    o.hl = hlOf(c.hlDefault);
    o.prevHl = o.hl;
  }
  if (c.autoSteps && !o.auto) {
    o.on = c.autoOn;
    o.trigger = c.autoTrigger.trim();
    if (c.autoOn === 'timed') {
      if (c.autoSecs > 0) Object.assign(o, { auto: true, interval: clampN(c.autoSecs, 0.5, 3600), autoAt: Math.max(1, now - Math.trunc(Math.max(0, ageSecs) * 1000)) });
    } else Object.assign(o, { auto: true, interval: 0 });
  }
  return o;
}

/** The step shown at `now` (1 to `n`): the step set, plus the intervals elapsed when the steps are timed (mirrors shown_step). */
export function shownStep(s: St, n: number, now: number): number {
  const base = Math.min(Math.max(1, s.step), Math.max(1, n));
  if (s.auto && s.interval > 0 && s.autoAt > 0 && base < n) {
    const t = s.pausedAt ? s.pausedAt : now;
    const step = Math.max(1, Math.trunc(s.interval * 1000));
    const k = Math.floor(Math.max(0, t - s.autoAt) / step);
    return Math.min(n, base + k);
  }
  return base;
}
const autoStepAt = (s: St, t: number) => s.autoAt + (t + 1 - Math.max(1, s.step)) * Math.max(1, Math.trunc(s.interval * 1000));

/** The state as drawn at `now`: the take's chain and highlight applied, the timed steps revealed so far, with their times (mirrors shown). */
export function shownState(c: Cfg, s: St, now: number, ageSecs: number, data: Data): St {
  const o = live(c, s, now, ageSecs);
  const n = steps(c, o, parse(c, shownGrid(c, o, data)));
  if (!n) return o;
  const base = Math.max(1, o.step);
  const sh = shownStep(o, n, now);
  if (sh <= base) return { ...o, step: sh };
  const stepAt = o.stepAt.slice();
  while (stepAt.length < n) stepAt.push(0);
  for (let t = base; t < sh; t++) stepAt[t] = autoStepAt(o, t);
  return { ...o, step: sh, stepAt, autoAt: autoStepAt(o, sh - 1) };
}

/** Seconds left before the next timed step (undefined = not running, or at the last step), mirrors next_in. */
export function nextIn(s: St, n: number, now: number): number | undefined {
  if (!s.auto || !(s.interval > 0) || !(s.autoAt > 0) || shownStep(s, n, now) >= n) return undefined;
  const t = s.pausedAt ? s.pausedAt : now;
  const step = s.interval * 1000;
  const e = Math.max(0, t - s.autoAt) % step;
  return (step - e) / 1000;
}

/** The steps wait for Continue (F2): chained on it, not at the last step (mirrors waits_continue). */
export const waitsContinue = (s: St, n: number, now: number) => !!s.auto && s.on === 'continue' && n > 0 && shownStep(s, n, now) < n;

/** The steps and highlight of the chart on air now (the take's chain until the operator acts). */
function useChart(layer: Config, state: unknown, data: Data, time: { nowMs: number; ageSecs: number; held?: boolean }) {
  const { now, age } = useLiveTime(time);
  const c = cfg(layer);
  const s = live(c, st(state), now, age);
  const P = parse(c, shownGrid(c, s, data));
  const n = steps(c, s, P);
  return { c, s, P, n, step: n ? shownStep(s, n, now) : 0, now };
}

/** The series / column / slice put forward: in the Designer the template's (hlDefault, exposable), in the Operator the one on air. */
function HlRow({ c, P, value, onChange }: { c: Cfg; P: ReturnType<typeof parse>; value: string; onChange: (v: string) => void }) {
  const options = hlOptions(c, P);
  return (
    <Row label={t('Put forward')} expose="hlDefault" exposeValue={value} exposeOptions={options}>
      <Dropdown value={options.some(([v]) => v === value) ? value : ''} options={options.map(([v, label]) => ({ value: v, label }))} onChange={onChange} />
    </Row>
  );
}

type OpP = { layer: Config; state: unknown; data: Data; act: (op: string, args?: unknown) => void; item?: unknown; time: { nowMs: number; ageSecs: number; held?: boolean } };

/**
 * The control centre of the chart (web/src/widgets/centre.tsx). Line 1: the reveal step and how
 * the steps are revealed (or new data waiting for a Refresh), the countdown to the next step or
 * what it waits for. Line 2: by hand → previous / next / show all; timed → play / pause, stop;
 * on Continue → Continue, stop; on a trigger → fire it, stop; then Refresh. Line 3: chain the
 * steps, how the next one comes, the time per step or the trigger (in the Operator, on the air
 * only), the reveal (exposable), the series / column / slice put forward (exposable).
 */
function ChartBar(p: OpP) {
  const { act, layer, data, item } = p;
  const mode = useCentreMode();
  const b = useBound();
  const { c, s, P, n, step, now } = useChart(layer, p.state, data, p.time);
  const pie = c.chartType === 'pie';
  // The data on air waits for a Refresh (kept since the take, by the server).
  const pending = c.onAir === 'reload' && !!s.cur && JSON.stringify(s.cur) !== JSON.stringify(resolveGrid(c, data));
  // Held by the preview: its timed steps wait for their Play.
  const held = !!p.time.held;
  const chained = !!s.auto;
  const on: AdvanceMode = chained ? s.on : 'timed';
  const running = chained && on === 'timed' && !s.pausedAt && !held;
  const trigger = s.trigger || c.autoTrigger;
  const secs = s.interval || c.autoSecs || 5;
  const left = nextIn(s, n, now);
  // The Designer edits the template (what the take starts with); the Operator the air only.
  const designer = mode === 'designer';
  const setChained = (v: boolean) => (designer ? b.set('autoSteps', v) : act(v ? 'advance' : 'stop', v ? { on: c.autoOn, seconds: c.autoSecs, trigger: c.autoTrigger } : undefined));
  const setOn = (v: AdvanceMode) => (designer ? b.set('autoOn', v) : act('advance', { on: v, seconds: secs, trigger }));
  const setSecs = (v: number) => (designer ? b.set('autoSecs', v) : act('auto', { seconds: v }));
  const setTrigger = (v: string) => (designer ? b.set('autoTrigger', v) : act('advance', { on: 'trigger', trigger: v }));
  const chainedShown = designer ? c.autoSteps : chained;
  const onShown = designer ? c.autoOn : on;
  const secsShown = designer ? c.autoSecs : secs;
  const triggerShown = designer ? c.autoTrigger : trigger;
  useKeys({
    ArrowLeft: () => n && act('prev'),
    ArrowRight: () => n && act('next'),
    p: () => n > 1 && on === 'timed' && act(running ? 'pause' : chained ? 'resume' : 'auto', chained ? undefined : { seconds: secs }),
    r: () => act('reload'),
  });
  const mode3 = revealMode(c, s);
  const words = mode3 === 'series' ? t('Series by series') : pie ? t('Slice by slice') : t('Category by category');
  const status = n < 2 || !chained ? undefined : on === 'timed' ? (running && left !== undefined ? <Countdown left={left} total={s.interval || secs} /> : undefined) : on === 'continue' ? (waitsContinue(s, n, now) ? <WaitsContinue /> : undefined) : <WaitsTrigger name={trigger} />;
  const actions: Line2Item[] = [
    ...(n > 0
      ? !chained
        ? [
            { icon: SkipBack, label: tc('cc', 'Previous'), key: '←', disabled: step <= 1, onClick: () => act('prev') },
            { icon: SkipForward, label: tc('cc', 'Next'), key: '→', disabled: step >= n, onClick: () => act('next') },
            { icon: ChevronsRight, label: t('Show all'), disabled: step >= n, onClick: () => act('showAll') },
          ]
        : on === 'timed'
          ? [running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: () => act('pause') } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: () => act(s.pausedAt ? 'resume' : 'auto', s.pausedAt ? undefined : { seconds: secs }) }]
          : on === 'continue'
            ? [{ icon: ChevronsRight, label: tc('cc', 'Continue'), key: 'F2', main: true, disabled: !waitsContinue(s, n, now), onClick: () => act('next') }]
            : [{ icon: Zap, label: trigger || tc('cc', 'Fire the trigger'), main: true, onClick: () => act(step >= n ? 'first' : 'next') }]
      : []),
    n > 0 && chained && { icon: Square, label: tc('cc', 'Stop'), onClick: () => act('stop') },
    GAP,
    { icon: RefreshCw, label: tc('cc', 'Refresh'), key: 'R', onClick: () => act('reload') },
  ];
  const readout = n > 0 || pending ? { k: t('Step'), v: n ? step : '—', of: n || undefined, sub: pending ? <span style={{ color: '#facc15' }}>● {t('New data')}</span> : n ? words : undefined, after: n ? <Dots at={step - 1} count={n} /> : undefined } : undefined;
  return (
    <Centre name={(layer as { name?: string }).name} readout={readout} status={status} actions={actions}>
      <Toggle label={t('Chain the steps')} value={chainedShown} onChange={setChained} />
      {chainedShown && <Seg label={t('Next step')} value={onShown} onChange={setOn} options={ADVANCE_MODES()} />}
      {chainedShown && onShown === 'timed' && <RangeNum label={t('Time per step')} value={secsShown} min={2} max={30} step={1} unit="s" onRelease onChange={setSecs} />}
      {chainedShown && onShown === 'trigger' && <Text label={tc('cc', 'Trigger name')} value={triggerShown} onChange={setTrigger} />}
      <Seg
        label={t('Reveal')}
        path="reveal"
        value={pie && c.reveal === 'series' ? 'all' : c.reveal}
        options={[
          ['all', t('All at once')],
          ...(pie ? [] : [['series', t('By series')] as const]),
          ['categories', pie ? t('By slice') : t('By category')],
        ]}
      />
      <HlRow c={c} P={P} value={designer ? c.hlDefault : hlValue(s)} onChange={(v) => (designer ? b.set('hlDefault', v) : act('highlight', hlArgs(v)))} />
      {!!s.cur && !item && designer && (
        <Row label="">
          <button className="opx-btn wk-btn ghost" title={t('The try-out holds the data of its last Reload.')} onClick={() => act('reset')}>
            <span>{t('Back to the live data')}</span>
          </button>
        </Row>
      )}
    </Centre>
  );
}

export default defineWidget({
  id: 'chart',
  type: 'chart',
  label: 'Chart',
  group: 'Data',
  icon: ChartColumn,
  create: () => ({ chartType: 'columns', data: SAMPLE.map((r) => r.slice()), title: { pos: 'none' }, labels: { value: false }, panel: { on: false }, props: { width: 1460, height: 820, fontSize: 26 } }),
  ownsLook: true,
  live: () => true,
  content: ['csv', 'title.text'],
  Render: ({ layer, r, w, h, data, time, state, lang }) => {
    const out = run(layer, data, state, w, h, num(r.fontSize), time.ageSecs * 1000, time.nowMs, lang ?? 'fr');
    return <Svg prims={out.prims} w={w} h={h} font={cfg(layer).font} />;
  },
  Inspector: ({ layer, data, edit }) => {
    const c = cfg(layer);
    const P = parse(c, resolveGrid(c, data));
    const type = c.chartType;
    const pct = type === 'columns' && c.colMode === 'percent';
    const labelsOn = c.labels.value || c.labels.pct || c.labels.name;
    // eslint-disable-next-line react-hooks/rules-of-hooks
    const lang = useApp((s) => s.project?.language) || 'fr';
    return (
      <>
        <Section id="chart-data" title={t('Data')}>
          <Seg
            label={t('Source')}
            path="source"
            value={c.source}
            options={[
              ['grid', t('Table')],
              ['csv', 'CSV'],
            ]}
          />
          {c.source === 'csv' ? (
            <Content label="CSV" path="csv" value="" multiline placeholder={t('Month;Martin;Durand\nJan;28;24')} />
          ) : (
            <>
              <Select
                label={t('Sample')}
                value=""
                options={[
                  ['', t('Load…')],
                  ['poll', t('Poll')],
                  ['budget', t('Budget')],
                  ['share', t('Audience share')],
                ]}
                onChange={(v) =>
                  SAMPLES[v] &&
                  edit((l) => {
                    l.data = SAMPLES[v].data.map((r) => r.slice());
                    l.axis = { ...l.axis, ...SAMPLES[v].axis };
                  })
                }
              />
              <Row expose="data" exposeValue={c.data}>
                <GridEditor path="data" value={c.data} header csv />
              </Row>
            </>
          )}
          <Seg
            label={t('Missing')}
            path="missing"
            value={c.missing}
            options={[
              ['gap', t('Gap')],
              ['interp', t('Interpolate')],
              ['zero', t('Zero')],
            ]}
          />
          <Seg
            label={t('On air')}
            path="onAir"
            value={c.onAir}
            options={[
              ['auto', t('Auto')],
              ['reload', t('On Reload')],
            ]}
          />
        </Section>
        <Section id="chart-chart" title={t('Chart')}>
          <Seg
            label={t('Type')}
            path="chartType"
            value={c.chartType}
            options={[
              ['columns', t('Columns')],
              ['areas', t('Areas')],
              ['lines', t('Lines')],
              ['pie', t('Pie')],
            ]}
          />
          <Toggle label={t('Sparkline')} path="spark" value={c.spark} />
          {type === 'columns' && (
            <>
              <Seg
                label={t('Columns')}
                path="colMode"
                value={c.colMode}
                options={[
                  ['grouped', t('Grouped')],
                  ['stacked', t('Stacked')],
                  ['percent', '100 %'],
                ]}
              />
              <Seg
                label={t('Orientation')}
                path="orient"
                value={c.orient}
                options={[
                  ['v', t('Vertical')],
                  ['h', t('Horizontal')],
                ]}
              />
            </>
          )}
          {type === 'areas' && (
            <Seg
              label={t('Areas')}
              path="areaMode"
              value={c.areaMode}
              options={[
                ['simple', t('Simple')],
                ['stacked', t('Stacked')],
              ]}
            />
          )}
          {type === 'pie' && (
            <>
              <Select label={t('Series')} path="pie.series" value={c.pie.series} options={P.ser.map((x, j) => [j, x] as const)} />
              <Seg
                label={t('Sort')}
                path="pie.sort"
                value={c.pie.sort}
                options={[
                  ['none', t('As typed')],
                  ['desc', t('Largest')],
                  ['asc', t('Smallest')],
                ]}
              />
              <Num label={t('Start angle')} path="pie.start" value={c.pie.start} min={0} max={359} unit="°" />
              <Num label={t('Donut')} path="pie.donut" value={c.pie.donut} min={0} max={85} unit="%" />
              {c.pie.donut > 30 && <Toggle label={t('Total')} path="pie.centerTotal" value={c.pie.centerTotal} />}
              <Select label={t('Exploded')} path="pie.explode" value={c.pie.explode} options={[[-1, t('None')] as const, ...P.cats.map((x) => [x.ri, x.name] as const)]} />
              {c.pie.explode >= 0 && <Num label={t('Distance')} path="pie.explodeDist" value={c.pie.explodeDist} min={0} max={400} unit="px" />}
              <Toggle label={t('Others')} path="pie.others" value={c.pie.others} />
              {c.pie.others && <Num label={t('Under')} path="pie.othersPct" value={c.pie.othersPct} min={0} max={50} step={0.5} precision={1} unit="%" />}
            </>
          )}
        </Section>
        {type !== 'pie' && (
          <Section id="chart-axes" title={t('Axes')}>
            {!pct && (
              <>
                <Seg
                  label={t('Range')}
                  path="axis.auto"
                  value={c.axis.auto}
                  options={[
                    [true, t('Auto')],
                    [false, t('Fixed')],
                  ]}
                />
                {!c.axis.auto && (
                  <>
                    <Num label="Min" path="axis.min" value={c.axis.min} precision={2} />
                    <Num label="Max" path="axis.max" value={c.axis.max} precision={2} />
                  </>
                )}
                {type === 'lines' && c.axis.auto && <Toggle label={t('From zero')} path="axis.zero" value={c.axis.zero} />}
              </>
            )}
            <Num label={t('Ticks')} path="axis.ticks" value={c.axis.ticks} min={2} max={12} />
            <Toggle label={t('Grid')} path="axis.grid" value={c.axis.grid} />
            <Toggle label={type === 'columns' && c.orient === 'h' ? t('Grid between rows') : t('Vertical grid')} path="axis.vgrid" value={c.axis.vgrid} />
            <Toggle label={t('Reference line')} path="refLine.on" value={c.refLine.on} />
            {c.refLine.on && (
              <>
                <Num label={t('Value')} path="refLine.value" value={c.refLine.value} step={1} precision={2} />
                <Text label={t('Label')} path="refLine.label" value={c.refLine.label} />
              </>
            )}
            <Text label={t('X title')} path="axis.xTitle" value={c.axis.xTitle} />
            <Text label={t('Y title')} path="axis.yTitle" value={c.axis.yTitle} />
          </Section>
        )}
        <Section id="chart-format" title={t('Numbers')}>
          <Seg
            label={t('Decimals')}
            path="axis.decimals"
            value={c.axis.decimals}
            options={[
              [0, '0'],
              [1, '0.0'],
              [2, '0.00'],
              [3, '0.000'],
              [4, '0.0000'],
            ]}
          />
          <Text label={t('Prefix')} path="axis.prefix" value={c.axis.prefix} placeholder="$" />
          <Text label={t('Suffix')} path="axis.suffix" value={c.axis.suffix} placeholder=" %" />
          <Seg
            label={t('Thousands')}
            path="axis.sep"
            value={c.axis.sep}
            options={[
              ['space', '1 000'],
              ['comma', '1,000'],
              ['none', '1000'],
            ]}
          />
        </Section>
        {type !== 'pie' ? (
          <Section id="chart-series" title={t('Series')}>
            {P.ser.map((name, j) => {
              const s = seriesStyle(c, j);
              return (
                <Row expose={[`series.${j}.color`, `series.${j}.width`, `series.${j}.marker`]} exposeValue={{ [`series.${j}.color`]: s.color, [`series.${j}.width`]: s.width, [`series.${j}.marker`]: s.marker }} key={j} label={name}>
                  <HexColorField compact value={s.color} onChange={(v) => edit((l) => void ((l.series ??= [])[j] = { ...(l.series?.[j] ?? {}), color: v }), `chs-${layer.id}-${j}`)} />
                  {type !== 'columns' && (
                    <>
                      <span className="dd-cell" title={t('Line width')}>
                        <Dropdown value={String(s.width)} options={[0, 2, 4, 6, 8, 12].map((v) => ({ value: String(v), label: `${v} px` }))} onChange={(v) => edit((l) => void ((l.series ??= [])[j] = { ...(l.series?.[j] ?? {}), width: +v }))} />
                      </span>
                      <span className="dd-cell" title={t('Marker')}>
                        <Dropdown
                          value={s.marker}
                          options={[
                            { value: 'none', label: '—' },
                            { value: 'circle', label: t('● Circle') },
                            { value: 'square', label: t('■ Square') },
                            { value: 'diamond', label: t('◆ Diamond') },
                          ]}
                          onChange={(v) => edit((l) => void ((l.series ??= [])[j] = { ...(l.series?.[j] ?? {}), marker: v as never }))}
                        />
                      </span>
                    </>
                  )}
                </Row>
              );
            })}
            {type !== 'columns' && <Toggle label={t('Smooth')} path="smooth" value={c.smooth} />}
            {type === 'areas' && <Num label={t('Area opacity')} expose="areaOpacity" exposeValue={c.areaOpacity} value={Math.round(c.areaOpacity * 100)} min={10} max={100} unit="%" onChange={(v) => edit((l) => void (l.areaOpacity = v / 100), `chao-${layer.id}`)} />}
          </Section>
        ) : (
          <Section id="chart-slices" title={t('Slices')}>
            {P.cats.slice(0, 12).map((x, i) => (
              <ColorPick
                expose={`sliceColors.${i}`}
                exposeValue={sliceColor(c, i)}
                key={i}
                label={x.name}
                value={sliceColor(c, i)}
                onChange={(v) => edit((l) => void ((l.sliceColors ??= PAL.slice(0, Math.max(i + 1, P.cats.length)).map((_, k) => sliceColor(c, k)))[i] = v), `chsl-${layer.id}-${i}`)}
              />
            ))}
          </Section>
        )}
        <Section id="chart-labels" title={t('Labels')}>
          <Toggle
            expose={['labels.value', 'labels.pct', 'labels.name']}
            exposeValue={{ 'labels.value': c.labels.value, 'labels.pct': c.labels.pct, 'labels.name': c.labels.name }}
            label={t('Labels')}
            value={labelsOn}
            onChange={(v) => edit((l) => void (l.labels = { ...l.labels, value: v, pct: false, name: false }))}
          />
          {labelsOn && (
            <>
              <Toggle label={t('Value')} path="labels.value" value={c.labels.value} />
              <Toggle label={t('Percentage')} path="labels.pct" value={c.labels.pct} />
              <Toggle label={t('Name')} path="labels.name" value={c.labels.name} />
              {type !== 'lines' && (
                <Seg
                  label={t('Position')}
                  path="labels.pos"
                  value={c.labels.pos}
                  options={[
                    ['inside', t('Inside')],
                    ['outside', t('Outside')],
                  ]}
                />
              )}
              <Seg
                label={t('Overlap')}
                path="labels.overlap"
                value={c.labels.overlap}
                options={[
                  ['offset', t('Offset')],
                  ['hide', t('Hide')],
                ]}
              />
              {type === 'pie' && <Num label={t('Min slice')} path="labels.minSize" value={c.labels.minSize} min={0} max={10} step={0.5} precision={1} unit="%" />}
            </>
          )}
        </Section>
        <Section id="chart-layout" title={t('Layout')}>
          <GroupRow layer={layer} spec={{ label: t('Text size'), props: ['fontSize'], min: 8, suffix: 'px' }} />
          <Row label={t('Font')}>
            <FamilyPick value={c.font} onChange={(v) => edit((l) => void (l.font = v))} />
          </Row>
          <ColorPick label={t('Colour of the texts')} path="textColor" value={c.textColor || '#ffffff'} />
          <Text label={tl(lang, 'No data')} path="texts.noData" value={c.texts.noData} placeholder={tl(lang, 'No data')} />
          {type === 'pie' && (
            <>
              <Text label={tl(lang, 'Others')} path="texts.others" value={c.texts.others} placeholder={tl(lang, 'Others')} />
              {c.pie.donut > 30 && c.pie.centerTotal && <Text label={tl(lang, 'Total')} path="texts.total" value={c.texts.total} placeholder={tl(lang, 'Total')} />}
            </>
          )}
          <Toggle
            expose="title.pos"
            exposeValue={c.title.pos}
            exposeOptions={[
              ['left', t('Left')],
              ['center', t('Centre')],
              ['none', t('Hidden')],
            ]}
            label={t('Title')}
            value={c.title.pos !== 'none'}
            onChange={(v) => edit((l) => void (l.title = { ...l.title, pos: v ? 'left' : 'none' }))}
          />
          {c.title.pos !== 'none' && (
            <>
              <Content label={t('Title')} path="title.text" value={c.title.text} />
              <Content label={t('Subtitle')} path="title.sub" value={c.title.sub} />
              <Seg
                expose={false}
                label={t('Align')}
                path="title.pos"
                value={c.title.pos}
                options={[
                  ['left', t('Left')],
                  ['center', t('Centre')],
                ]}
              />
            </>
          )}
          <Toggle
            expose="legend"
            exposeValue={c.legend}
            exposeOptions={[
              ['top', t('Top')],
              ['bottom', t('Bottom')],
              ['right', t('Right')],
              ['none', t('None')],
            ]}
            label={t('Legend')}
            value={c.legend !== 'none'}
            onChange={(v) => edit((l) => void (l.legend = v ? 'top' : 'none'))}
          />
          {c.legend !== 'none' && (
            <Seg
              expose={false}
              label={t('Position')}
              path="legend"
              value={c.legend}
              options={[
                ['top', t('Top')],
                ['bottom', t('Bottom')],
                ['right', t('Right')],
              ]}
            />
          )}
          <Toggle label={t('Panel')} path="panel.on" value={c.panel.on} />
          {c.panel.on && (
            <>
              <ColorPick label={t('Color')} path="panel.color" value={c.panel.color} />
              <Num label={t('Opacity')} expose="panel.opacity" exposeValue={c.panel.opacity} value={Math.round(c.panel.opacity * 100)} min={0} max={100} unit="%" onChange={(v) => edit((l) => void (l.panel = { ...l.panel, opacity: v / 100 }), `chpo-${layer.id}`)} />
            </>
          )}
        </Section>
        <Section id="chart-anim" title={t('Animation')}>
          <Seg
            label={t('Entrance')}
            path="anim.fx"
            value={c.anim.fx}
            options={[
              ['auto', t('Auto')],
              ['grow', t('Grow')],
              ...(type === 'pie' ? [] : [['draw', t('Draw')] as const]),
              ...(type === 'pie' ? [['sweep', t('Sweep')] as const] : []),
              ['fade', t('Fade')],
              ['none', t('Cut')],
            ]}
          />
          <Num label={t('Duration')} path="anim.dur" value={c.anim.dur} min={0} max={30} step={0.1} precision={1} unit="s" />
          <Num label={t('Stagger')} path="anim.stagger" value={c.anim.stagger} min={0} max={5000} step={10} unit="ms" />
          <Num label={t('Update')} path="anim.upd" value={c.anim.upd} min={0} max={30} step={0.1} precision={1} unit="s" />
          <Seg
            label={t('Curve')}
            path="anim.curve"
            value={c.anim.curve}
            options={[
              ['ease', t('Ease in-out')],
              ['out', t('Ease out')],
              ['linear', t('Linear')],
            ]}
          />
          <Seg
            label={t('Reveal')}
            path="reveal"
            value={type === 'pie' && c.reveal === 'series' ? 'all' : c.reveal}
            options={[
              ['all', t('All')],
              ...(type === 'pie' ? [] : [['series', t('By series')] as const]),
              ['categories', type === 'pie' ? t('By slice') : t('By category')],
            ]}
          />
        </Section>
      </>
    );
  },
  Operator: (p) => <ChartBar {...p} />,
  // Chained on Continue: F2 reveals the next step (until the last one).
  waits: (l, state, data, item) => {
    const c = cfg(l);
    const now = Date.now();
    const s = live(c, st(state), now, (item.age ?? 0) / 50);
    return waitsContinue(s, steps(c, s, parse(c, shownGrid(c, s, data))), now);
  },
  // Timed steps: the Operator's preview holds them on the step shown until their Play.
  plays: (l, data) => {
    const c = cfg(l);
    return c.autoSteps && c.autoOn === 'timed' && c.autoSecs > 0 && steps(c, st(undefined), parse(c, resolveGrid(c, data))) > 1 && 'onAir';
  },
  warnings: (layer, { data, state, r }) => {
    const c = cfg(layer);
    const s = st(state);
    const P = parse(c, shownGrid(c, s, data));
    const w = num((r?.width ?? layer.props.width ?? 1460) as number);
    const h = num((r?.height ?? layer.props.height ?? 820) as number);
    const o = run(layer, data, state, w, h, fsOf(r, layer), 1e9, Date.now() + 1e7, useApp.getState().project?.language || 'fr');
    const out: Warning[] = [];
    if (c.chartType === 'pie') {
      if (o.pieNeg.length) out.push({ level: 'error', text: t('Negative value refused in a pie: {list} not drawn.', { list: o.pieNeg.map(([n, v]) => `${n} (${fmt(c, v)})`).join(', ') }), op: true });
      if (o.pieZero) out.push({ level: 'error', text: t('Pie total is zero: empty circle shown, nothing to share out.'), op: true });
      if (o.pieOthers) out.push({ level: 'info', text: t('{n} slices under {pct} % grouped in "Others".', { n: o.pieOthers, pct: c.pie.othersPct }) });
      if (P.ser.length > 1) out.push({ level: 'info', text: t('A pie shows one series: "{name}".', { name: P.ser[Math.min(Math.max(0, c.pie.series), P.ser.length - 1)] }) });
      if (o.pieMissing) out.push({ level: 'info', text: tn(o.pieMissing, '{n} missing value: slice not drawn.', '{n} missing values: slice not drawn.') });
    } else {
      if (P.missing) {
        const how = c.missing === 'gap' ? t('shown as a gap') : c.missing === 'interp' ? t('interpolated (hollow marker / dashed column), gap at the ends') : t('counted as zero');
        out.push({ level: 'info', text: tn(P.missing, '{n} missing value: {how}.', '{n} missing values: {how}.', { how }), op: true });
      }
      const hasNeg = P.cats.some((x) => x.vals.some((v) => v != null && v < 0));
      if (hasNeg && c.chartType === 'columns' && c.colMode === 'percent') out.push({ level: 'warn', text: t('100 % columns with negative values: negatives are left out of the shares.') });
      if (hasNeg && c.chartType === 'areas' && c.areaMode === 'stacked') out.push({ level: 'warn', text: t('Stacked areas with negative values are hard to read: prefer lines or columns.') });
      if (!c.axis.auto && !(c.chartType === 'columns' && c.colMode === 'percent')) {
        const all = P.cats.flatMap((x) => x.vals).filter((v): v is number => v != null);
        if (all.some((v) => v > c.axis.max || v < c.axis.min)) out.push({ level: 'warn', text: t('Fixed axis {min} – {max}: some values are outside and cut at the edge.', { min: c.axis.min, max: c.axis.max }) });
      }
    }
    if (!P.cats.length || !P.ser.length) out.push({ level: 'warn', text: t('No data: the chart shows "No data".'), op: true });
    if (o.hidden)
      out.push({
        level: 'info',
        text: c.chartType === 'pie' && c.labels.overlap === 'hide' ? tn(o.hidden, '{n} label hidden to avoid overlaps (or under {pct} %).', '{n} labels hidden to avoid overlaps (or under {pct} %).', { pct: c.labels.minSize }) : tn(o.hidden, '{n} label hidden to avoid overlaps.', '{n} labels hidden to avoid overlaps.'),
      });
    return out;
  },
});

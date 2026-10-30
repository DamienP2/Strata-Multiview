// Time (« Temps »): the stopwatch or countdown of a `clock` layer (timerMode stopwatch /
// countdown). Its inspector puts the value first, the look next, the rest under More
// settings; how it starts on air is in its control centre (line 3). Drawing, control
// centre and the shared rows in ./timer.tsx; logic in ./timer-fmt.ts.
import { Timer } from 'lucide-react';
import { ColorPick, Content, ListEditor, Num, Section, Seg, Select, Text, Toggle } from './kit';
import * as T from './timer-fmt';
import { AffixRows, AnalogRows, clockWidget, DigitRows, DurField, FontRows, LookSeg, ModeSeg, PlateRows, PresetRow, STEPS, TextRows, WidthRows, ZONES, type InspP } from './timer';
import { t, tc } from '../i18n';

const MODES: [T.TimerMode, string][] = [
  ['countdown', 'Countdown'],
  ['stopwatch', 'Stopwatch'],
];

function Inspector({ layer, data, edit, set }: InspP) {
  const e = T.eff(layer, data);
  const c = e.c;
  // An older layer in another mode is shown as a countdown here (the Clock widget draws it).
  const mode: T.TimerMode = e.mode === 'stopwatch' ? 'stopwatch' : 'countdown';
  const cd = mode === 'countdown';
  const digital = c.face !== 'analog';
  return (
    <>
      <Section id="time-main" title={tc('widget', 'Time')}>
        <ModeSeg mode={mode} modes={MODES} edit={edit} />
        {cd ? (
          <>
            <Seg
              label={t('Target')}
              path="cdTarget"
              value="duration"
              options={[
                ['duration', t('Duration')],
                ['time', t('Time')],
              ]}
            />
            {c.cdTarget === 'duration' ? (
              <DurField label={t('Duration')} value={c.cdDuration} min={1000} onChange={(v) => set('cdDuration', v)} expose="cdDuration" />
            ) : (
              <>
                <Text label={t('Date')} path="tDate" value="" placeholder="YYYY-MM-DD" />
                <Text label={t('Time')} path="tTime" value="20:00:00" placeholder="20:00:00" />
                <Select label={t('Zone')} path="tZone" value="" options={[['', t('Server')] as [string, string], ...ZONES.map((z) => [z, z] as [string, string])]} />
              </>
            )}
          </>
        ) : (
          <DurField label={t('Start value')} value={c.swStart} onChange={(v) => set('swStart', v)} expose="swStart" />
        )}
        {cd && (
          <Seg
            label={t('At zero')}
            path="atZero"
            value="freeze"
            options={[
              ['freeze', t('Freeze')],
              ['overrun', t('Overrun +')],
              ['restart', t('Restart')],
              ['out', t('Hide')],
            ]}
          />
        )}
        <PresetRow layer={layer} data={data} mode={mode} edit={edit} />
      </Section>

      <Section id="time-style" title={t('Style')}>
        {!digital && (
          <>
            <LookSeg c={c} />
            <AnalogRows c={c} />
          </>
        )}
        <FontRows layer={layer} c={c} set={set} />
        {digital && <PlateRows c={c} />}
      </Section>

      <Section id="time-more" title={t('More settings')}>
        <Section id="time-text" title={t('Text')} level={2}>
          <TextRows />
          {!!(c.prefix || c.suffix) && <AffixRows />}
        </Section>
        {digital && (
          <Section id="time-digits" title={t('Digits')} level={2}>
            <Toggle label={t('Tabular')} path="tabular" value={T.DEF.tabular} />
            <DigitRows e={e} />
            <WidthRows c={c} />
          </Section>
        )}
        {cd ? (
          <Section id="time-cd" title={t('Countdown')} level={2}>
            <Toggle label={t('Round up')} path="roundUp" value={true} />
            <Text label={t('Then trigger')} path="zeroTrigger" value="" placeholder={t('e.g. next (none)')} />
            {c.atZero === 'overrun' && (
              <>
                <ColorPick label={t('Overrun text')} path="overColor" value="#ff3b5c" />
                <ColorPick label={t('Overrun bg')} path="overBg" value="" none={t('None')} first="#3a0d18" />
              </>
            )}
            <Toggle label={t('Banner')} path="banner" value={false} />
            {c.banner && (
              <>
                <Content label={t('Banner text')} path="bannerText" value="" />
                <ColorPick label={t('Banner colour')} path="bannerColor" value="#ff3b5c" />
              </>
            )}
          </Section>
        ) : (
          <Section id="time-sw" title={t('Stopwatch')} level={2}>
            <Toggle label={t('Limit')} path="swLimitOn" value={false} />
            {c.swLimitOn && (
              <>
                <DurField label={t('Limit at')} value={c.swLimit} min={1000} onChange={(v) => set('swLimit', v)} expose="swLimit" />
                <Seg
                  label={t('At the limit')}
                  path="swLimitMode"
                  value="stop"
                  options={[
                    ['stop', t('Stop')],
                    ['continue', t('Continue')],
                    ['restart', t('Restart')],
                  ]}
                />
              </>
            )}
            <Num label={t('Split hold')} path="splitHold" value={5} min={1} max={15} unit="s" />
          </Section>
        )}
        <Section id="time-steps" title={t('Steps')} level={2}>
          <ListEditor
            label={t('Steps')}
            path="steps"
            value={[]}
            toggle="on"
            fields={[
              { key: 'at', num: true, w: 56, placeholder: 's' },
              { key: 'color', color: true, w: 36 },
              { key: 'bg', placeholder: t('bg #hex') },
              {
                key: 'blink',
                w: 76,
                options: [
                  ['', t('Steady')],
                  ['true', t('Blink')],
                ],
              },
            ]}
            add={() => ({ on: true, at: 30, color: '#facc15', bg: '', blink: false })}
            addLabel={t('Step')}
          />
        </Section>
      </Section>
    </>
  );
}

export default clockWidget({
  id: 'time',
  label: 'Time',
  icon: Timer,
  create: () => ({
    mode: 'timer' as const,
    format: 'mm:ss',
    timezone: '',
    target: '',
    fontFamily: 'Inter',
    fontWeight: 800,
    align: 'center' as const,
    verticalAlign: 'middle' as const,
    uppercase: false,
    timerMode: 'countdown' as const,
    cdDuration: 300000,
    // Starts on Continue (F2) or Play, as the layer's start says (startMode « manual » is the older wait).
    manualStart: true,
    startMode: 'auto' as const,
    steps: structuredClone(STEPS.countdown),
    props: { width: 900, height: 300, fontSize: 150, fill: [1, 1, 1, 1] },
  }),
  content: ['prefix', 'suffix', 'bannerText'],
  Inspector,
});

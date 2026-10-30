// Clock (« Horloge »): the time of day or the date of a `clock` layer (timerMode clock / date),
// digits or an analog face, one zone or several cities. Drawing, Operator controls and the
// rows shared with the Time widget in ./timer.tsx; logic in ./timer-fmt.ts.
import { Clock } from 'lucide-react';
import { ListEditor, Num, Section, Seg, Select, Text, Toggle } from './kit';
import * as T from './timer-fmt';
import { AffixRows, AnalogRows, clockWidget, DigitRows, FontRows, LookSeg, ModeSeg, PlateRows, PresetRow, TextRows, useProjectLang, WidthRows, ZONES, type Config, type InspP, type L } from './timer';
import { t } from '../i18n';
import { HYPHEN_LANGS } from '../engine/textFit';

const MODES: [T.TimerMode, string][] = [
  ['clock', 'Clock'],
  ['date', 'Date'],
];

/** The format writes day or month names. */
const named = (fmt: string) => /dddd|ddd|MMMM|MMM/.test(fmt.replace(/\[[^\]]*\]/g, ''));

/** The language of the names: the project's (by default), or one of the tables. */
function LangSelect() {
  const project = useProjectLang();
  const own = HYPHEN_LANGS.find(([v]) => v === T.nameLang('', project));
  return <Select label={t('Language')} path="lang" value="en-GB" options={[['', own ? `${t('Project language')} · ${t(own[1])}` : t('Project language')] as [string, string], ...T.LANGS]} />;
}

function Inspector({ layer, data, edit, set }: InspP) {
  const e = T.eff(layer, data);
  const c = e.c;
  // An older layer in another mode is shown as a clock here (the Time widget draws it).
  const mode: T.TimerMode = e.mode === 'date' ? 'date' : 'clock';
  const zoneMode = c.tzOffset != null ? 'offset' : layer.timezone?.trim() ? 'named' : 'machine';
  const digital = c.face !== 'analog';
  const names = digital && (mode === 'date' || named(e.format));
  return (
    <>
      <Section id="clock-main" title={t('Clock')}>
        <ModeSeg mode={mode} modes={MODES} edit={edit} />
        <PresetRow layer={layer} data={data} mode={mode} edit={edit} />
        {names && <LangSelect />}
        <TextRows />
        {mode === 'clock' && (
          <>
            <Toggle
              expose="cities"
              exposeValue={c.cities}
              label={t('Cities')}
              value={c.cities}
              onChange={(v) =>
                edit((l) => {
                  const x = l as L & Config;
                  x.cities = v;
                  if (v && !x.cityList?.length)
                    x.cityList = [
                      { label: 'Paris', zone: 'Europe/Paris' },
                      { label: 'New York', zone: 'America/New_York' },
                      { label: 'Tokyo', zone: 'Asia/Tokyo' },
                    ];
                  if (v) x.format = 'HH:mm';
                })
              }
            />
            {c.cities && (
              <ListEditor
                path="cityList"
                value={[]}
                fields={[
                  { key: 'label', placeholder: t('City') },
                  { key: 'zone', w: 150, options: [['', t('Server')], ...ZONES.map((z) => [z, z] as [string, string])] },
                ]}
                add={() => ({ label: 'London', zone: 'Europe/London' })}
                addLabel={t('City')}
                min={1}
              />
            )}
          </>
        )}
      </Section>

      {digital && (
        <Section id="clock-digits" title={t('Digits')}>
          <Toggle label={t('Tabular')} path="tabular" value={T.DEF.tabular} />
          {mode === 'clock' && (
            <>
              <DigitRows e={e} />
              <Seg
                label={t('Hours')}
                path="h12"
                value={false}
                options={[
                  [false, '24 h'],
                  [true, '12 h'],
                ]}
              />
              {c.h12 && (
                <>
                  <Text label="AM" path="am" value="AM" />
                  <Text label="PM" path="pm" value="PM" />
                </>
              )}
            </>
          )}
          {mode === 'date' && (
            <Seg
              label={t('Case')}
              path="case"
              value="none"
              options={[
                ['none', t('As is')],
                ['first', t('First')],
                ['words', t('Words')],
                ['upper', t('UPPER')],
                ['lower', t('lower')],
              ]}
            />
          )}
        </Section>
      )}

      <Section id="clock-style" title={t('Style')}>
        {(mode === 'clock' || !digital) && <LookSeg c={c} />}
        {!digital && <AnalogRows c={c} />}
        <FontRows layer={layer} c={c} set={set} />
        {!!(c.prefix || c.suffix || (mode === 'clock' && c.cities)) && <AffixRows />}
        {digital && (
          <>
            <PlateRows c={c} />
            <WidthRows c={c} />
          </>
        )}
      </Section>

      <Section id="clock-zone" title={t('Time zone')}>
        <Seg
          expose="timezone"
          exposeValue={(layer as L & Config).timezone ?? ''}
          exposeOptions={[]}
          label={t('Zone')}
          value={zoneMode}
          options={[
            ['machine', t('Server')],
            ['named', t('Named')],
            ['offset', t('Offset')],
          ]}
          onChange={(v) =>
            edit((l) => {
              const x = l as L & Config;
              if (v === 'offset') x.tzOffset = x.tzOffset ?? 0;
              else {
                delete x.tzOffset;
                x.timezone = v === 'named' ? x.timezone?.trim() || 'Europe/Paris' : '';
              }
            })
          }
        />
        {zoneMode === 'named' && <Select label={t('Zone name')} path="timezone" value="Europe/Paris" options={ZONES.includes(layer.timezone) ? ZONES : [layer.timezone, ...ZONES]} />}
        {zoneMode === 'offset' && <Num label={t('UTC offset')} path="tzOffset" value={0} min={-12} max={14} step={0.25} precision={2} unit="h" />}
        {mode === 'clock' && (
          <>
            <Seg
              label={t('Fine in')}
              path="fineUnit"
              value="s"
              options={[
                ['s', t('Seconds')],
                ['f', t('Frames')],
              ]}
            />
            {c.fineUnit === 's' ? <Num label={t('Fine offset')} path="fine" value={0} min={-10} max={10} step={0.1} precision={1} unit="s" /> : <Num label={t('Fine offset')} path="fine" value={0} min={-50} max={50} step={1} unit="fr" />}
          </>
        )}
      </Section>
    </>
  );
}

export default clockWidget({
  id: 'clock',
  label: 'Clock',
  icon: Clock,
  create: () => ({
    mode: 'time' as const,
    format: 'HH:mm',
    timezone: '',
    target: '',
    fontFamily: 'Inter',
    fontWeight: 800,
    align: 'center' as const,
    verticalAlign: 'middle' as const,
    uppercase: false,
    timerMode: 'clock' as const,
    // The day and month names in the project's language.
    lang: '',
    props: { width: 900, height: 300, fontSize: 150, fill: [1, 1, 1, 1] },
  }),
  content: ['prefix', 'suffix'],
  Inspector,
});

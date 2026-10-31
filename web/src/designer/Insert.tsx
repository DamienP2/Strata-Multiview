// The tools of the Designer's toolbar that add to the multiview, all at hand in two groups: the
// broadcast widgets (sources, meters, timecode, alarms, time…), then the shapes, text and images.
import { Circle, Image, Minus, Square, Type } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import type { LayerType } from '../engine/types';
import { WIDGET_LIST } from '../widgets';
import { t, tc } from '../i18n';

export type AddLayer = (type: LayerType, patch?: Record<string, unknown>, name?: string) => void;

/** Group 1, in this order: a widget by its id, its name and what it is. */
const BROADCAST: { widget: string; label: string; hint: string }[] = [
  { widget: 'source', label: 'Source', hint: 'A live input with its tally, UMD and audio meters' },
  { widget: 'audioMeter', label: 'Audio meters', hint: 'Up to 16 channels of an input, with or without picture' },
  { widget: 'timecode', label: 'Timecode', hint: 'The station’s or an input’s' },
  { widget: 'alarmLog', label: 'Alarm log', hint: 'The faults of the inputs, the newest first' },
  { widget: 'time', label: 'Time', hint: 'A stopwatch or a countdown' },
  { widget: 'loudness', label: 'Loudness', hint: 'EBU R128: integrated, momentary, short-term, range, true peak' },
  { widget: 'status', label: 'Status', hint: 'PTP, NTP and the faults on now' },
  { widget: 'clock', label: 'Clock', hint: 'The time of day, analogue or digital, of any time zone' },
];

/** Group 2: shapes (rectangle, circle, line), text, images. */
const DRAWING: { key: string; icon: LucideIcon; label: string; ctx?: string; type: LayerType; patch?: Record<string, unknown>; name?: () => string }[] = [
  { key: 'rect', icon: Square, label: 'Rectangle', type: 'rect' },
  { key: 'ellipse', icon: Circle, label: 'Circle', type: 'ellipse', patch: { props: { width: 300, height: 300 } } },
  { key: 'line', icon: Minus, label: 'Line', ctx: 'shape', type: 'rect', patch: { props: { width: 600, height: 4 } }, name: () => tc('shape', 'Line') },
  { key: 'text', icon: Type, label: 'Text', type: 'text', name: () => t('Text') },
  { key: 'image', icon: Image, label: 'Image', type: 'image' },
];

/** The insert tools of the toolbar. */
export function InsertTools({ onAdd }: { onAdd: AddLayer }) {
  return (
    <div className="mv-insert">
      <div className="tl-insert" role="group" aria-label={t('Broadcast widgets')}>
        {BROADCAST.map((b) => {
          const w = WIDGET_LIST.find((x) => x.id === b.widget);
          if (!w) return null;
          const Icon = w.icon;
          const label = tc('widget', b.label);
          return (
            <button
              key={b.widget}
              className="tool"
              title={`${label} — ${t(b.hint)}`}
              onClick={() => onAdd(w.type, b.widget === 'source' ? undefined : (w.create() as Record<string, unknown>), label)}
            >
              <Icon size={15} />
            </button>
          );
        })}
      </div>
      <span className="tl-sep" />
      <div className="tl-insert" role="group" aria-label={t('Shapes, text and images')}>
        {DRAWING.map((d) => (
          <button key={d.key} className="tool" title={d.ctx ? tc(d.ctx, d.label) : t(d.label)} onClick={() => onAdd(d.type, d.patch, d.name?.())}>
            <d.icon size={15} />
          </button>
        ))}
      </div>
    </div>
  );
}

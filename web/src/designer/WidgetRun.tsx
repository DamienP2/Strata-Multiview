import { Pause, Play, RotateCcw, Timer } from 'lucide-react';
import type { Layer } from '../engine/types';
import { isTimed } from '../engine/widgets';
import { useDesigner } from './designerStore';
import { t } from '../i18n';

/** ▶ ❚❚ ⟲ of a widget that has internal controls, to try them in the designer. */
export function WidgetRunButtons({ layer, compact }: { layer: Layer; compact?: boolean }) {
  const run = useDesigner((s) => s.widgetRun[layer.id]);
  const control = useDesigner((s) => s.controlWidget);
  if (!isTimed(layer)) return null;
  const running = !!run && run.pausedAge === undefined;
  return (
    <div className={`widget-run ${compact ? 'compact' : ''}`} onPointerDown={(e) => e.stopPropagation()}>
      <button className={running ? 'on' : ''} title={t('Start (as the operator would)')} onClick={() => control(layer.id, 'start')}>
        <Play size={12} />
      </button>
      <button className={run && !running ? 'on' : ''} title={t('Pause')} onClick={() => control(layer.id, 'pause')}>
        <Pause size={12} />
      </button>
      <button title={t('Back to the start')} onClick={() => control(layer.id, 'reset')}>
        <RotateCcw size={12} />
      </button>
      {!compact && (
        <button className={!run ? 'on' : ''} title={t('Follow the timeline playhead again')} onClick={() => control(layer.id, 'playhead')}>
          <Timer size={12} /> {t('Playhead')}
        </button>
      )}
    </div>
  );
}

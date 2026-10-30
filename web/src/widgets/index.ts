// Registry of the spec widgets (one file each; see kit.tsx).
import type { Layer, LayerType } from '../engine/types';
import type { WidgetDef } from './kit';
import quad from './quad';
import polygon from './polygon';
import ellipse from './ellipse';
import text from './text';
import image from './image';
import pen from './pen';
import clock from './clock';
import time from './time';
import { eff, timed } from './timer-fmt';
import table from './table';
import ticker from './ticker';
import credits from './credits';
import typewriter from './typewriter';
import counter from './counter';
import progress from './progress';
import chart from './chart';
import map from './map';
import qrcode from './qrcode';
import web from './web';
import scoreboard from './scoreboard';
import socialwall from './socialwall';
import socialbadge from './socialbadge';

/** In the order of the spec. */
export const WIDGET_LIST: WidgetDef[] = [quad, polygon, ellipse, text, image, pen, clock, time, table, ticker, credits, typewriter, counter, progress, chart, map, qrcode, scoreboard, socialwall, socialbadge, web] as unknown as WidgetDef[];

/** By layer type (a `clock` layer: the Clock; widgetOf(layer) tells the Clock from the Time). */
export const WIDGETS: Partial<Record<LayerType, WidgetDef>> = Object.fromEntries([...WIDGET_LIST].reverse().map((w) => [w.type, w]));

/**
 * The widget drawing a layer. A `clock` layer is a Clock (time of day, date) or a Time
 * (stopwatch, countdown) by its mode; given only a type, the type's first widget.
 */
export const widgetOf = (of: LayerType | Layer): WidgetDef | undefined => {
  if (typeof of === 'string') return WIDGETS[of];
  if (of.type === 'clock') return (timed(eff(of, {})) ? time : clock) as unknown as WidgetDef;
  return WIDGETS[of.type];
};

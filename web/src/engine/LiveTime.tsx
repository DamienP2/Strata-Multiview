import { createContext, use, useContext, useMemo } from 'react';
import { clientNow, useApp } from '../store';
import { useFrameNow } from './frameClock';
import type { Layer } from './types';
import type { TimeCtx } from './widgets';

/**
 * Client clock that ticks every animation frame while `active` (at most
 * `maxFps` a second): the frame's own time stamp (not when its callback
 * happens to run), so what moves by the clock steps evenly from frame to frame.
 */
export const useTicker = useFrameNow;

/**
 * Which widgets of a scene are redrawn on each frame: all those that move
 * ('all': its time on air runs: on air, a preview or a try-out running), only
 * those that also move with the wall clock ('wall': its time stands still, as
 * in the Designer at rest), or none.
 */
export type Ticking = 'all' | 'wall' | false;

/** How a widget moves: with its time on air only ('age'), also with the wall clock ('wall'), or not at all. */
export type Motion = 'age' | 'wall' | false;

/** How a scene's time runs, for the widgets that redraw on each frame (see useLayerTime). */
export interface Clock {
  /** The time of a frame (client clock, ms). */
  at: (now: number) => TimeCtx;
  /** When the scene was drawn (client clock, ms). */
  drawnAt: number;
  ticking: Ticking;
  maxFps?: number;
}

/** Time seen by widgets; provided per playout layer (or by the designer). */
export const TimeContext = createContext<TimeCtx>({ nowMs: Date.now(), ageSecs: 0, defaultTz: '' });

const ClockContext = createContext<Clock>({ at: () => ({ nowMs: Date.now(), ageSecs: 0, defaultTz: '' }), drawnAt: 0, ticking: false });

/**
 * Provides widget time (server clock + time on air) to a scene. The time on
 * air is `ageSecs` at `since` (client clock, ms) and runs on from there; with
 * no `since` it stands still. `clientNow`: the time the scene is drawn at.
 * Only the widgets that move redraw on each frame (they subscribe to the
 * clock); the rest of the scene is drawn again only when these props change.
 */
export function LiveTime({
  ageSecs,
  since,
  clientNow: drawnAt = clientNow(),
  ticking = false,
  maxFps,
  layerAge,
  clocked,
  widgetState,
  fps,
  lang,
  children,
}: {
  ageSecs: number;
  since?: number;
  clientNow?: number;
  ticking?: Ticking;
  maxFps?: number;
  /** A layer's time on air, given the scene's time on air and the client clock. */
  layerAge?: (l: Layer, age: number, now: number) => number;
  clocked?: (l: Layer) => boolean;
  widgetState?: (layerId: string) => unknown;
  fps?: number;
  lang?: string;
  children: React.ReactNode;
}) {
  const offset = useApp((s) => s.serverOffset);
  const tz = useApp((s) => s.timezone);
  const at = (now: number): TimeCtx => {
    const age = since === undefined ? ageSecs : ageSecs + Math.max(0, now - since) / 1000;
    return { nowMs: now + offset, ageSecs: age, defaultTz: tz, layerAge: layerAge && ((l) => layerAge(l, age, now)), clocked, widgetState, fps, lang };
  };
  const clock = useMemo<Clock>(() => ({ at, drawnAt, ticking, maxFps }), [ageSecs, since, drawnAt, ticking, maxFps, layerAge, clocked, widgetState, fps, lang, offset, tz]); // eslint-disable-line react-hooks/exhaustive-deps
  // What does not move reads this one: kept while the time alone changes, so it is not drawn again.
  const time = useMemo(() => clock.at(drawnAt), [layerAge, clocked, widgetState, fps, lang, offset, tz]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <ClockContext.Provider value={clock}>
      <TimeContext.Provider value={time}>{children}</TimeContext.Provider>
    </ClockContext.Provider>
  );
}

/**
 * The time a layer's widget is drawn at: the scene's, or, for a widget that
 * moves (see Motion) while its scene ticks, the time of each frame.
 */
export function useLayerTime(moves: Motion): TimeCtx {
  const time = useContext(TimeContext);
  // Only what moves follows the time (read conditionally: the rest is not drawn again when it changes).
  const clock = moves ? use(ClockContext) : null;
  const on = !!clock && (clock.ticking === 'all' || (clock.ticking === 'wall' && moves === 'wall'));
  const now = useFrameNow(on, clock?.maxFps);
  return useMemo(() => (!clock ? time : clock.at(on ? now : clock.drawnAt)), [clock, on, now, time]);
}

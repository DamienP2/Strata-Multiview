// Which properties of a layer are on the Designer's timeline (animatable).
// Mirror of `Layer::timeline` (crates/strata-mv-core/src/model.rs).
import { PROPS, type Layer, type Prop } from './types';

/** A new layer: position, size and opacity. */
export const DEFAULT_TIMELINE: Prop[] = ['x', 'y', 'width', 'height', 'opacity'];

/** Puts properties on the timeline, or takes them off (their keyframes go). */
export function setOnTimeline(l: Layer, props: Prop[], on: boolean) {
  const s = new Set<Prop>(l.timeline ?? DEFAULT_TIMELINE);
  for (const p of props) {
    if (on) s.add(p);
    else {
      s.delete(p);
      delete l.tracks[p];
    }
  }
  l.timeline = PROPS.filter((p) => s.has(p));
}

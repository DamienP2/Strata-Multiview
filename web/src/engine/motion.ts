import type { Resolved } from './evaluate';
import type { Motion, Prop } from './types';

/**
 * Adds a layer's repeated movement (Layer.motion) to its values, at `ageSecs`
 * of its time on air (mirror of cg_core::anim::apply_motion).
 */
export function applyMotion(r: Resolved, m: Motion, frame: number, fps: number, ageSecs: number): Resolved {
  if (frame < m.from || m.period <= 0) return r;
  // From 0, where the in animation ends (the layer as set), smoothly.
  const t = Math.max(0, ageSecs - m.from / Math.max(1, fps));
  const phase = (t / m.period) * Math.PI * 2;
  const wave = Math.sin(phase);
  const swell = (1 - Math.cos(phase)) / 2;
  const out = { ...r };
  const num = (p: Prop) => (typeof out[p] === 'number' ? (out[p] as number) : 0);
  const w = num('width');
  const h = num('height');
  switch (m.kind) {
    case 'pulse':
      out.scaleX = num('scaleX') * (1 + 0.06 * swell);
      out.scaleY = num('scaleY') * (1 + 0.06 * swell);
      break;
    case 'blink':
      out.opacity = num('opacity') * (1 - 0.75 * swell);
      break;
    case 'swing':
      out.rotation = num('rotation') + 3 * wave;
      break;
    case 'float':
      out.y = num('y') + Math.max(4, h * 0.05) * wave;
      break;
    case 'shake':
      out.x = num('x') + Math.max(3, w * 0.01) * wave;
      break;
    case 'kenBurns':
      out.scaleX = num('scaleX') * (1 + 0.08 * swell);
      out.scaleY = num('scaleY') * (1 + 0.08 * swell);
      break;
  }
  return out;
}

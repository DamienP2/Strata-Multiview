import type { CSSProperties } from 'react';
import type { Area, Zone } from './types';

/** Starting areas of the default zones, landscape and portrait. */
const DEFAULTS: Record<string, [Area, Area]> = {
  'lower-third': [
    { x: 0.06, y: 0.7, width: 0.6, height: 0.19 },
    { x: 0.05, y: 0.66, width: 0.9, height: 0.16 },
  ],
  'top-left': [
    { x: 0.04, y: 0.05, width: 0.24, height: 0.14 },
    { x: 0.05, y: 0.04, width: 0.42, height: 0.08 },
  ],
  'top-right': [
    { x: 0.72, y: 0.05, width: 0.24, height: 0.14 },
    { x: 0.53, y: 0.04, width: 0.42, height: 0.08 },
  ],
  ticker: [
    { x: 0, y: 0.89, width: 1, height: 0.07 },
    { x: 0, y: 0.9, width: 1, height: 0.05 },
  ],
  'full-screen': [
    { x: 0, y: 0, width: 1, height: 1 },
    { x: 0, y: 0, width: 1, height: 1 },
  ],
};

/** Where a zone sits in a format: its own area, the main one, or a default. */
export function zoneArea(zone: Zone, format: string, portrait: boolean): Area {
  const own = zone.areas?.[format] ?? zone.areas?.main;
  if (own) return own;
  const d = DEFAULTS[zone.id];
  if (d) return d[portrait ? 1 : 0];
  return portrait ? { x: 0.1, y: 0.3, width: 0.8, height: 0.3 } : { x: 0.25, y: 0.25, width: 0.5, height: 0.5 };
}

/** CSS that keeps only what is inside `area` (none when it is the whole frame). */
export function areaClip(area: Area | undefined): CSSProperties | undefined {
  if (!area || (area.x <= 1e-6 && area.y <= 1e-6 && area.x + area.width >= 1 - 1e-6 && area.y + area.height >= 1 - 1e-6)) return undefined;
  const pc = (v: number) => `${(v * 100).toFixed(3)}%`;
  return { clipPath: `inset(${pc(area.y)} ${pc(1 - area.x - area.width)} ${pc(1 - area.y - area.height)} ${pc(area.x)})` };
}

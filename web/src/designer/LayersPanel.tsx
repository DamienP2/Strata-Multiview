import { BadgeCheck, MessagesSquare, Globe, ChartColumn, Map as MapIcon, Pentagon, PenTool, Pilcrow, ScrollText, Table, Trophy, BarChart3, Circle, Clock, Folder, GalleryHorizontal, Hash, Image, Keyboard, MoveHorizontal, MoveVertical, QrCode, Repeat, SlidersHorizontal, Sparkles, Square, Timer, Type, Video } from 'lucide-react';
import type { Layer, LayerType, Template } from '../engine/types';
import { descendants, findLayer } from './designerStore';
import { num, propValue } from '../engine/evaluate';
import { eff, timed } from '../widgets/timer-fmt';

/** Layer type icons (timeline, menus). */
export const ICONS: Record<LayerType, typeof Square> = {
  group: Folder,
  rect: Square,
  ellipse: Circle,
  text: Type,
  image: Image,
  clock: Clock,
  ticker: MoveHorizontal,
  roll: MoveVertical,
  counter: Hash,
  progress: SlidersHorizontal,
  rotator: Repeat,
  typewriter: Keyboard,
  slideshow: GalleryHorizontal,
  qrCode: QrCode,
  barChart: BarChart3,
  polygon: Pentagon,
  pen: PenTool,
  table: Table,
  credits: ScrollText,
  chart: ChartColumn,
  map: MapIcon,
  scoreboard: Trophy,
  web: Globe,
  socialWall: MessagesSquare,
  socialBadge: BadgeCheck,
};

/** A layer's icon: Video and Lottie layers are an image and a group underneath; a clock layer is a Clock or, stopwatch or countdown, a Time. */
export const layerIcon = (l: Layer) => (l.type === 'text' && l.wrap ? Pilcrow : l.type === 'image' && l.video ? Video : l.type === 'group' && l.lottie !== undefined ? Sparkles : l.type === 'clock' && timed(eff(l, {})) ? Timer : ICONS[l.type]);

export type DropPos = 'above' | 'below' | 'into';

/** Moves a layer above / below another (as listed, top-most first) or into a group. */
export function moveLayer(t: Template, id: string, targetId: string, pos: DropPos, frame = 0) {
  if (id === targetId || descendants(t, id).includes(targetId)) return;
  const idx = t.layers.findIndex((l) => l.id === id);
  const [layer] = t.layers.splice(idx, 1);
  const target = findLayer(t, targetId)!;
  const tIdx = t.layers.indexOf(target);
  const from = groupOffset(t, layer.parent, frame);
  if (pos === 'into') {
    layer.parent = target.id;
    t.layers.push(layer);
  } else {
    layer.parent = target.parent ?? null;
    // Listed top-most first, so "above" means later in paint order.
    t.layers.splice(pos === 'above' ? tIdx + 1 : tIdx, 0, layer);
  }
  // It stays where it is on the stage: its x / y are now from its new group.
  const to = groupOffset(t, layer.parent, frame);
  const shift = (p: 'x' | 'y', d: number) => {
    if (!d) return;
    if (typeof layer.props[p] === 'number') layer.props[p] = (layer.props[p] as number) + d;
    for (const k of layer.tracks[p] ?? []) if (typeof k.value === 'number') k.value += d;
  };
  shift('x', from.x - to.x);
  shift('y', from.y - to.y);
}

/** Where a group (and its own groups) puts its layers on the stage. */
function groupOffset(t: Template, parent: string | null | undefined, frame: number) {
  let x = 0;
  let y = 0;
  const seen = new Set<string>();
  for (let p = parent ? findLayer(t, parent) : undefined; p && !seen.has(p.id); p = p.parent ? findLayer(t, p.parent) : undefined) {
    seen.add(p.id);
    x += num(propValue(p, 'x', frame));
    y += num(propValue(p, 'y', frame));
  }
  return { x, y };
}

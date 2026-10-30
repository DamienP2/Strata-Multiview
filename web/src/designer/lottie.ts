// The Lottie widget: a group (`lottie` set) that receives the layers of a
// Lottie / Bodymovin file, converted by the server (cg-core lottie.rs). The
// animation is laid out in the project's frame; the group scales that frame
// into the box the placeholder had.
import type { Layer } from '../engine/types';
import { newId } from '../engine/evaluate';
import { t, tn } from '../i18n';
import { importAfterEffects, useApp } from '../store';
import { descendants, findLayer, useDesigner } from './designerStore';

const num = (v: unknown, d = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : d);

/** Puts the animation of `file` into the Lottie group `groupId` (its former content goes). */
export async function loadLottie(groupId: string, file: File, src: string) {
  const app = useApp.getState();
  const p = app.project;
  if (!p) return;
  const { template: imp, warnings } = await importAfterEffects(p.id, file, true);
  const [W, H] = [p.width, p.height];
  useDesigner.getState().update((tpl) => {
    const g = findLayer(tpl, groupId);
    if (!g || g.type !== 'group') return;
    const old = new Set(descendants(tpl, groupId));
    tpl.layers = tpl.layers.filter((l) => !old.has(l.id));
    const ids = new Map(imp.layers.map((l) => [l.id, newId()] as [string, string]));
    for (const l of imp.layers) tpl.layers.push({ ...structuredClone(l), id: ids.get(l.id)!, parent: l.parent && ids.has(l.parent) ? ids.get(l.parent)! : groupId } as Layer);
    // The project frame (where the animation is laid out) fitted into the box, centred.
    const scaled = num(g.props.width, W) * num(g.props.scaleX, 1);
    const scaledH = num(g.props.height, H) * num(g.props.scaleY, 1);
    const [bx, by] = [num(g.props.x), num(g.props.y)];
    const k = Math.min(scaled / W, scaledH / H);
    g.props = { ...g.props, x: Math.round(bx + (scaled - W * k) / 2), y: Math.round(by + (scaledH - H * k) / 2), width: W, height: H, anchorX: 0, anchorY: 0, scaleX: k, scaleY: k };
    g.lottie = src;
    if (imp.duration > tpl.duration) tpl.duration = imp.duration;
    for (const fl of imp.fields) if (!tpl.fields.some((x) => x.key === fl.key)) tpl.fields.push(fl);
  });
  app.toast(
    warnings.length ? t('“{name}” added — not converted: {warnings}', { name: imp.name, warnings: warnings.join('; ') }) : tn(imp.layers.length, '“{name}” added ({n} layer)', '“{name}” added ({n} layers)', { name: imp.name }),
    'info',
  );
}

/** The same, from a file of the assets. */
export async function loadLottieAsset(groupId: string, src: string) {
  const r = await fetch(src, { credentials: 'same-origin' });
  if (!r.ok) throw new Error(t('This file cannot be read'));
  const name = decodeURIComponent(src.split('/').pop() || 'animation.json');
  await loadLottie(groupId, new File([await r.blob()], name), src);
}

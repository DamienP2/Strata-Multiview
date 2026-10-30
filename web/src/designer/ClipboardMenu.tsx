// Right-click menu of the timeline's layer and property rows, and of the
// layers on the stage: copy / paste layers and animations (clipboard.ts),
// the layer's animation, its order and delete.
import { ArrowDown, ArrowDownToLine, ArrowUp, ArrowUpToLine, BringToFront, ChevronRight, ClipboardCopy, ClipboardPaste, Copy, Scissors, DiamondMinus, Sparkles, Trash2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { Prop } from '../engine/types';
import { t, tn } from '../i18n';
import { confirmDialog } from '../ui/dialogs';
import { animatedProps, clearKeyframes, copyAll, copyAnimation, copyLayers, cutLayers, pasteAnimationOnRow, pasteLayers, rowMapping, useClipboard } from './clipboard';
import { descendants, duplicateLayers, findLayer, useDesigner } from './designerStore';
import { moveLayer } from './LayersPanel';
import { openAnimations } from './animPresets';

export interface ClipMenuAt {
  x: number;
  y: number;
  layerId: string;
  /** A property row (Position…): its properties and label. */
  row?: { props: Prop[]; label: string };
}

type Order = 'front' | 'forward' | 'backward' | 'back';

/** Moves layers among the layers of their group (paint order: last is in front). */
function reorder(ids: string[], how: Order) {
  const st = useDesigner.getState();
  st.update((tpl) => {
    // The front-most first when bringing forward, the back-most first when sending back: they keep their order.
    const list = tpl.layers.filter((l) => ids.includes(l.id)).map((l) => l.id);
    if (how === 'front' || how === 'forward') list.reverse();
    for (const id of list) {
      const l = findLayer(tpl, id);
      if (!l) continue;
      const sibs = tpl.layers.filter((x) => (x.parent ?? null) === (l.parent ?? null));
      const i = sibs.indexOf(l);
      const target = how === 'front' ? sibs[sibs.length - 1] : how === 'back' ? sibs[0] : how === 'forward' ? sibs[i + 1] : sibs[i - 1];
      if (!target || target.id === id) continue;
      moveLayer(tpl, id, target.id, how === 'front' || how === 'forward' ? 'above' : 'below', st.frame);
    }
  });
}

const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
const key = (k: string) => (mac ? `⌘${k}` : `Ctrl+${k}`);

export function ClipboardMenu({ at, onClose }: { at: ClipMenuAt; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [orderOpen, setOrderOpen] = useState(false);
  const clip = useClipboard();
  const st = useDesigner();
  useEffect(() => {
    const down = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onClose();
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.stopPropagation(), onClose());
    window.addEventListener('pointerdown', down);
    window.addEventListener('keydown', esc, true);
    window.addEventListener('resize', onClose);
    return () => {
      window.removeEventListener('pointerdown', down);
      window.removeEventListener('keydown', esc, true);
      window.removeEventListener('resize', onClose);
    };
  }, [onClose]);
  const layer = st.template && findLayer(st.template, at.layerId);
  if (!layer) return null;
  // The selection when the row is part of it, else this layer.
  const targets = st.selected.includes(layer.id) ? st.selected : [layer.id];
  const run = (fn: () => unknown) => () => (onClose(), void fn());
  const anim = clip.animation;
  const rowAnimated = at.row ? at.row.props.some((p) => layer.tracks[p]?.length) : animatedProps(layer).length > 0;
  const fits = !!anim && (!at.row || rowMapping(anim, at.row.props).length > 0);
  const ro = st.readOnly;
  return createPortal(
    <div ref={ref} className="menu project-menu clip-menu" style={{ position: 'fixed', left: Math.min(at.x, window.innerWidth - 250), top: Math.max(8, Math.min(at.y, window.innerHeight - (at.row ? 150 : 440))), right: 'auto' }} onContextMenu={(e) => e.preventDefault()}>
      {!at.row && (
        <>
          <button onClick={run(() => copyLayers(targets))}>
            <Copy size={13} /> {t('Copy')}
            <kbd>{key('C')}</kbd>
          </button>
          <button disabled={ro} onClick={run(() => cutLayers(targets))}>
            <Scissors size={13} /> {t('Cut')}
            <kbd>{key('X')}</kbd>
          </button>
          <button disabled={ro || !clip.layers} onClick={run(pasteLayers)}>
            <ClipboardPaste size={13} /> {t('Paste layers')}
            <kbd>{key('V')}</kbd>
          </button>
          <button disabled={ro} onClick={run(() => duplicateLayers(targets))}>
            <Copy size={13} /> {t('Duplicate')}
            <kbd>{key('D')}</kbd>
          </button>
          <div className="menu-sep" />
        </>
      )}
      {at.row ? (
        <>
          <button disabled={!rowAnimated} title={rowAnimated ? undefined : t('No keyframes here')} onClick={run(() => copyAnimation(layer.id, at.row?.props, at.row?.label))}>
            <ClipboardCopy size={13} /> {t('Copy the animation of {prop}', { prop: t(at.row.label) })}
          </button>
          <button disabled={ro || !fits} title={anim && !fits ? t('This animation does not fit this property') : undefined} onClick={run(() => pasteAnimationOnRow(layer.id, at.row!.props))}>
            <ClipboardPaste size={13} /> {t('Paste the animation')}
            {anim && <em>{anim.label}</em>}
          </button>
        </>
      ) : (
        <>
          <button onClick={run(() => copyAll(layer.id))}>
            <ClipboardCopy size={13} /> {t('Copy all (values and animation)')}
          </button>
          <button disabled={ro || !clip.all} onClick={run(() => useClipboard.setState({ pasting: targets }))}>
            <ClipboardPaste size={13} /> {t('Paste…')}
            {clip.all && <em>{clip.all.label}</em>}
          </button>
        </>
      )}
      <button
        disabled={ro || (at.row ? !rowAnimated : !targets.some((id) => st.template && animatedProps(findLayer(st.template, id)!).length))}
        onClick={run(() =>
          at.row
            ? clearKeyframes([layer.id], at.row.props, t('Erase the keyframes of {prop} (“{name}”)?', { prop: t(at.row.label), name: layer.name }))
            : clearKeyframes(targets, undefined, targets.length > 1 ? t('Erase the keyframes of {n} layers?', { n: targets.length }) : t('Erase the keyframes of “{name}”?', { name: layer.name })),
        )}
      >
        <DiamondMinus size={13} /> {at.row ? t('Erase the keyframes of {prop}', { prop: t(at.row.label) }) : targets.length > 1 ? t('Erase the keyframes of the layers') : t('Erase the keyframes of the layer')}
      </button>
      {!at.row && (
        <>
          <div className="menu-sep" />
          <button disabled={ro} onClick={run(() => openAnimations(targets))}>
            <Sparkles size={13} /> {t('Animation…')}
          </button>
          <div className="ux2-sub-wrap" onMouseEnter={() => setOrderOpen(true)} onMouseLeave={() => setOrderOpen(false)}>
            <button disabled={ro} aria-haspopup="menu" aria-expanded={orderOpen} onClick={() => setOrderOpen(!orderOpen)}>
              <BringToFront size={13} /> {t('Order')}
              <ChevronRight size={12} className="ux2-chev" />
            </button>
            {orderOpen && !ro && (
              <div className={`menu project-menu clip-menu ux2-submenu ${at.x > window.innerWidth - 480 ? 'left' : ''}`}>
                <button onClick={run(() => reorder(targets, 'front'))}>
                  <ArrowUpToLine size={13} /> {t('Bring to front')}
                </button>
                <button onClick={run(() => reorder(targets, 'forward'))}>
                  <ArrowUp size={13} /> {t('Bring forward')}
                </button>
                <button onClick={run(() => reorder(targets, 'backward'))}>
                  <ArrowDown size={13} /> {t('Send backward')}
                </button>
                <button onClick={run(() => reorder(targets, 'back'))}>
                  <ArrowDownToLine size={13} /> {t('Send to back')}
                </button>
              </div>
            )}
          </div>
          <div className="menu-sep" />
          <button
            className="danger"
            disabled={ro}
            onClick={run(async () => {
              const s = useDesigner.getState();
              // A folder: the layers inside it go too, said first.
              const kids = s.template ? targets.reduce((n, id) => n + descendants(s.template!, id).length, 0) : 0;
              if (kids && !(await confirmDialog({ title: targets.length > 1 ? t('Delete the {n} layers?', { n: targets.length }) : t('Delete “{name}”?', { name: layer.name }), message: tn(kids, 'The {n} layer inside it is deleted too.', 'The {n} layers inside it are deleted too.'), confirm: t('Delete'), danger: true }))) return;
              s.update((tpl) => {
                const ids = new Set(targets.flatMap((id) => [id, ...descendants(tpl, id)]));
                tpl.layers = tpl.layers.filter((l) => !ids.has(l.id));
              });
              s.select([]);
            })}
          >
            <Trash2 size={13} /> {t('Delete')}
            <kbd>{t('Del')}</kbd>
          </button>
        </>
      )}
    </div>,
    document.body,
  );
}

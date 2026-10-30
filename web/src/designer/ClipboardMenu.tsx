// Right-click menu of the layers (the Layers list and the stage): copy, cut,
// paste and duplicate (clipboard.ts), their order, delete.
import { ArrowDown, ArrowDownToLine, ArrowUp, ArrowUpToLine, BringToFront, ChevronRight, ClipboardPaste, Copy, Scissors, Trash2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type {} from '../engine/types';
import { t, tn } from '../i18n';
import { confirmDialog } from '../ui/dialogs';
import { copyLayers, cutLayers, pasteLayers, useClipboard } from './clipboard';
import { descendants, duplicateLayers, findLayer, useDesigner } from './designerStore';
import { moveLayer } from './LayersPanel';

export interface ClipMenuAt {
  x: number;
  y: number;
  layerId: string;
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
  // The selection when the layer is part of it, else this layer.
  const targets = st.selected.includes(layer.id) ? st.selected : [layer.id];
  const run = (fn: () => unknown) => () => (onClose(), void fn());
  const ro = st.readOnly;
  return createPortal(
    <div ref={ref} className="menu project-menu clip-menu" style={{ position: 'fixed', left: Math.min(at.x, window.innerWidth - 250), top: Math.max(8, Math.min(at.y, window.innerHeight - 340)), right: 'auto' }} onContextMenu={(e) => e.preventDefault()}>
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
    </div>,
    document.body,
  );
}

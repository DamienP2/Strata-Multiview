// The layers of the open multiview, beside the stage: the shapes and widgets to add (the « + »
// menu), then the layers, front-most first, in their folders. A click selects, a double-click
// renames, a drag moves a layer (into a folder too); right-click: copy, order, delete.
import { Circle, Code, Copy, Eye, EyeOff, FolderPlus, Image, Lock, Minus, MoreHorizontal, PenTool, Pentagon, Pilcrow, Plus, Square, Type, Unlock, Video } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import type { Layer, LayerType, Template } from '../engine/types';
import { childrenMap } from '../engine/Scene';
import { WIDGET_LIST } from '../widgets';
import { duplicateLayers, findLayer, useDesigner, WIDGET_PRESETS } from './designerStore';
import { layerIcon, moveLayer, type DropPos } from './LayersPanel';
import { ClipboardMenu, type ClipMenuAt } from './ClipboardMenu';
import { t, tc } from '../i18n';

export type AddLayer = (type: LayerType, patch?: Record<string, unknown>, name?: string) => void;

/** The layers in the order of the list: front-most first, each folder's layers under it. */
function flatten(t: Template): { layer: Layer; depth: number }[] {
  const tree = childrenMap(t.layers, true);
  const out: { layer: Layer; depth: number }[] = [];
  const walk = (pid: string | null, depth: number) => {
    for (const l of [...(tree.get(pid) ?? [])].reverse()) {
      out.push({ layer: l, depth });
      walk(l.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

export function LayerList({ onAdd }: { onAdd: AddLayer }) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const [drop, setDrop] = useState<{ id: string; pos: DropPos } | null>(null);
  const [ctx, setCtx] = useState<ClipMenuAt | null>(null);
  const st = useDesigner();
  const { template, selected } = st;
  const rows = useMemo(() => (template ? flatten(template) : []), [template]);
  if (!template) return null;
  return (
    <section className="mv-layers">
      <header>
        <span>{t('Layers')}</span>
        <AddMenu onAdd={onAdd} />
      </header>
      <InsertBar onAdd={onAdd} />
      <div className="mv-layer-rows">
        {rows.map(({ layer, depth }) => {
          const isSel = selected.includes(layer.id);
          const Icon = layerIcon(layer);
          return (
            <div
              key={layer.id}
              className={`tl-label layer ${isSel ? 'selected' : ''} ${layer.visible ? '' : 'dim'} ${drop?.id === layer.id ? `drop-${drop.pos}` : ''}`}
              style={{ paddingLeft: 8 + depth * 14 }}
              draggable={renaming !== layer.id}
              onDragStart={(e) => e.dataTransfer.setData('text/layer', layer.id)}
              onDragOver={(e) => {
                e.preventDefault();
                const r = e.currentTarget.getBoundingClientRect();
                const y = (e.clientY - r.top) / r.height;
                setDrop({ id: layer.id, pos: layer.type === 'group' && y > 0.3 && y < 0.7 ? 'into' : y < 0.5 ? 'above' : 'below' });
              }}
              onDragLeave={() => setDrop(null)}
              onDrop={(e) => {
                const id = e.dataTransfer.getData('text/layer');
                const d = drop;
                setDrop(null);
                if (id && d) st.update((tpl) => moveLayer(tpl, id, d.id, d.pos, st.frame));
              }}
              onClick={(e) => {
                if (e.shiftKey || e.metaKey || e.ctrlKey) st.select(isSel ? selected.filter((x) => x !== layer.id) : [...selected, layer.id]);
                else st.select([layer.id]);
              }}
              onDoubleClick={() => setRenaming(layer.id)}
              onContextMenu={(e) => {
                e.preventDefault();
                if (!isSel) st.select([layer.id]);
                setCtx({ x: e.clientX, y: e.clientY, layerId: layer.id });
              }}
            >
              <Icon size={13} className="layer-icon" />
              {renaming === layer.id ? (
                <input
                  autoFocus
                  className="rename"
                  defaultValue={layer.name}
                  onClick={(e) => e.stopPropagation()}
                  onBlur={(e) => {
                    const v = e.target.value.trim();
                    setRenaming(null);
                    if (v && v !== layer.name) st.update((tpl) => void (findLayer(tpl, layer.id)!.name = v));
                  }}
                  onKeyDown={(e) => {
                    e.stopPropagation();
                    if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
                    if (e.key === 'Escape') setRenaming(null);
                  }}
                />
              ) : (
                <span className="layer-name">{layer.name}</span>
              )}
              <span className="row-actions">
                <button title={t('Duplicate (Ctrl+D)')} onClick={(e) => (e.stopPropagation(), duplicateLayers([layer.id]))}>
                  <Copy size={12} />
                </button>
              </span>
              <button className={`row-btn ${layer.visible ? '' : 'off'}`} title={layer.visible ? t('Hide') : t('Show')} onClick={(e) => (e.stopPropagation(), st.update((tpl) => void (findLayer(tpl, layer.id)!.visible = !layer.visible)))}>
                {layer.visible ? <Eye size={12} /> : <EyeOff size={12} />}
              </button>
              <button className={`row-btn ${layer.locked ? 'on' : ''}`} title={layer.locked ? t('Unlock') : t('Lock')} onClick={(e) => (e.stopPropagation(), st.update((tpl) => void (findLayer(tpl, layer.id)!.locked = !layer.locked)))}>
                {layer.locked ? <Lock size={12} /> : <Unlock size={12} />}
              </button>
              <button
                className="row-btn"
                title={t('More')}
                onClick={(e) => {
                  e.stopPropagation();
                  if (!isSel) st.select([layer.id]);
                  const r = e.currentTarget.getBoundingClientRect();
                  setCtx({ x: r.left, y: r.bottom + 4, layerId: layer.id });
                }}
              >
                <MoreHorizontal size={12} />
              </button>
            </div>
          );
        })}
        {!rows.length && <p className="muted mv-layers-empty">{t('Add a tile, a clock, a logo or a text with the buttons above.')}</p>}
      </div>
      {ctx && <ClipboardMenu at={ctx} onClose={() => setCtx(null)} />}
    </section>
  );
}

/** A Paragraph's text to begin with. */
const LOREM_PARAGRAPH = 'Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.';

/** Shapes on hand above the layers; the rest behind "+". */
function InsertBar({ onAdd }: { onAdd: AddLayer }) {
  return (
    <div className="tl-insert mv-insert">
      <button className="add-btn folder" title={t('New folder (group)')} onClick={() => onAdd('group')}>
        <FolderPlus size={14} />
      </button>
      <span className="tl-pipe" />
      <button className="tool" title={t('Rectangle')} onClick={() => onAdd('rect')}>
        <Square size={15} />
      </button>
      <button className="tool" title={t('Ellipse')} onClick={() => onAdd('ellipse')}>
        <Circle size={15} />
      </button>
      <button className="tool" title={t('Polygon')} onClick={() => onAdd('polygon', { sides: 5 })}>
        <Pentagon size={15} />
      </button>
      <button
        className="tool"
        title={tc('stroke', 'Line')}
        // A straight Pen stroke: round ends, dashes along the line (a thin rectangle had neither).
        onClick={() =>
          onAdd(
            'pen',
            {
              points: [
                { x: 0, y: 0.5 },
                { x: 1, y: 0.5 },
              ],
              arrowEnd: 'none',
              drawIn: false,
              props: {
                x: 160,
                y: 520,
                width: 600,
                height: 40,
                stroke: [1, 1, 1, 1],
                strokeWidth: 4,
              },
            },
            tc('stroke', 'Line'),
          )
        }
      >
        <Minus size={15} />
      </button>
      <button className="tool" title={tc('text', 'Line')} onClick={() => onAdd('text', undefined, tc('text', 'Line'))}>
        <Type size={15} />
      </button>
      <button
        className="tool"
        title={t('Paragraph')}
        onClick={() => onAdd('text', { wrap: true, text: LOREM_PARAGRAPH, props: { width: 900, height: 300, fontSize: 40 } }, t('Paragraph'))}
      >
        <Pilcrow size={15} />
      </button>
      <button className="tool" title={t('Image')} onClick={() => onAdd('image')}>
        <Image size={15} />
      </button>
      <button className="tool" title={t('Pen')} onClick={() => onAdd('pen')}>
        <PenTool size={15} />
      </button>
    </div>
  );
}

type MenuItem = {
  label: string;
  icon: typeof Square;
  /** A spec widget (type + settings), or a preset of the older widgets. */
  type?: LayerType;
  patch?: Record<string, unknown>;
  preset?: string;
  hint?: string;
  soon?: boolean;
};

/** The + menu: one list of widgets, in this order. */
const MENU: { type?: LayerType; widget?: string; label: string; soon?: typeof Video; patch?: Record<string, unknown>; action?: 'video'; hint?: string; preset?: string }[] = [
  { type: 'source', label: 'Source', hint: 'A live input with its tally, UMD and audio meters' },
  { type: 'ticker', label: 'Ticker' },
  { widget: 'clock', label: 'Clock' },
  { widget: 'time', label: 'Time' },
  { type: 'qrCode', label: 'QR code' },
  { label: 'Video', soon: Video, action: 'video', hint: 'A video file (mp4, mov, webm…; alpha kept) played as frames, at most a minute' },
  { type: 'web', label: 'Iframe', soon: Code, hint: 'A web page or your HTML code (HTML5 outputs)' },
];
type Menu = MenuItem & { action?: 'video' };
const menu = (): Menu[] =>
  MENU.map((m) => {
    // A widget by its id (a type drawn by two widgets: the Clock and the Time) comes with its own settings.
    const w = m.widget ? WIDGET_LIST.find((x) => x.id === m.widget) : m.type ? WIDGET_LIST.find((x) => x.type === m.type) : undefined;
    const patch = m.widget && w ? (w.create() as Record<string, unknown>) : m.patch;
    return { label: m.label, icon: m.soon && !w ? m.soon : (w?.icon ?? m.soon ?? Square), type: w?.type, patch, hint: m.hint, preset: m.preset, action: m.action, soon: !w && !m.action && !m.preset };
  });

function AddMenu({ onAdd }: { onAdd: AddLayer }) {
  const [open, setOpen] = useState<{ left: number; top: number } | null>(null);
  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(null);
    window.addEventListener('pointerdown', close);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('resize', close);
    };
  }, [open]);
  const pick = (it: Menu) => {
    // A video is placed at once; its file or input is chosen on the right.
    if (it.action === 'video') {
      setOpen(null);
      onAdd('image', { video: true, fit: 'contain', src: '' }, t('Video'));
      return;
    }
    if (it.type) {
      setOpen(null);
      onAdd(it.type, it.patch, tc('widget', it.label));
      return;
    }
    const w = WIDGET_PRESETS.find((x) => x.key === it.preset);
    if (!w) return;
    setOpen(null);
    onAdd(w.type, w.patch as Record<string, unknown> | undefined, t(w.label));
  };
  return (
    <div className="menu-wrap" onPointerDown={(e) => e.stopPropagation()}>
      <button
        className={`opx-add ${open ? 'on' : ''}`}
        title={t('Widgets')}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setOpen(
            open
              ? null
              : {
                  left: Math.max(8, r.left),
                  top: r.bottom + 6,
                },
          );
        }}
      >
        <Plus size={14} />
      </button>
      {open && (
        <div
          className="menu add-menu"
          style={{
            position: 'fixed',
            left: open.left,
            top: open.top,
          }}
        >
          <div className="add-grid">
            {menu().map((it) => (
              <button key={it.label} disabled={it.soon} title={it.soon ? t('Coming soon') : it.hint && t(it.hint)} onClick={() => pick(it)}>
                <span className="add-icon">
                  <it.icon size={15} />
                </span>
                {tc('widget', it.label)}
                {it.soon && <em>{t('soon')}</em>}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

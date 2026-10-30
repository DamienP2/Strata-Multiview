// The tools of the Designer's toolbar that add to the multiview: a Source tile first, then the
// shapes and texts on hand, the other widgets behind « + ».
import { Circle, Code, Image, Minus, MonitorPlay, PenTool, Pentagon, Pilcrow, Plus, Square, Type, Video } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { LayerType } from '../engine/types';
import { WIDGET_LIST } from '../widgets';
import { WIDGET_PRESETS } from './designerStore';
import { t, tc } from '../i18n';

export type AddLayer = (type: LayerType, patch?: Record<string, unknown>, name?: string) => void;

/** The insert tools of the toolbar. */
export function InsertTools({ onAdd }: { onAdd: AddLayer }) {
  return (
    <div className="mv-insert">
      <button className="btn small primary" title={t('A live input with its tally, UMD and audio meters')} onClick={() => onAdd('source', undefined, tc('widget', 'Source'))}>
        <MonitorPlay size={13} /> {tc('widget', 'Source')}
      </button>
      <InsertBar onAdd={onAdd} />
      <AddMenu onAdd={onAdd} />
    </div>
  );
}

/** A Paragraph's text to begin with. */
const LOREM_PARAGRAPH = 'Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.';

/** Shapes and texts on hand; the rest behind "+". */
function InsertBar({ onAdd }: { onAdd: AddLayer }) {
  return (
    <div className="tl-insert">
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
  const [open, setOpen] = useState<{ right: number; top: number } | null>(null);
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
                  right: Math.max(8, window.innerWidth - r.right),
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
            left: 'auto',
            right: open.right,
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

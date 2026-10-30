// Layouts of a multiview: the Source tiles placed on a grid (2×2, 3×3, 1+5, 2+8…), with a gap
// between them and a margin around. Applied to the open template (its tiles moved in reading order,
// the missing ones added with the next inputs) or to a new one.
import { LayoutGrid, X } from 'lucide-react';
import { useState } from 'react';
import { createPortal } from 'react-dom';
import type { InputInfo } from '../engine/liveInputs';
import { useInputs } from '../engine/liveInputs';
import { num, propValue } from '../engine/evaluate';
import type { Layer, Project, Template } from '../engine/types';
import { formatById } from '../engine/formats';
import { t } from '../i18n';
import { makeLayer, useDesigner } from './designerStore';

/** A cell of the grid: column, row, columns spanned, rows spanned. */
type Cell = [number, number, number, number];

export interface LayoutPreset {
  id: string;
  label: string;
  cols: number;
  rows: number;
  cells: Cell[];
}

/** Every cell of a `cols` × `rows` grid, row by row, but those `taken` by a big one. */
function grid(cols: number, rows: number, big: Cell[] = []): Cell[] {
  const covered = (c: number, r: number) => big.some(([bc, br, cs, rs]) => c >= bc && c < bc + cs && r >= br && r < br + rs);
  const out: Cell[] = [...big];
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) if (!covered(c, r)) out.push([c, r, 1, 1]);
  return out;
}

export const PRESETS: LayoutPreset[] = [
  { id: '1', label: '1', cols: 1, rows: 1, cells: grid(1, 1) },
  { id: '2', label: '2', cols: 4, rows: 4, cells: [[0, 1, 2, 2], [2, 1, 2, 2]] },
  { id: '2x2', label: '2 × 2', cols: 2, rows: 2, cells: grid(2, 2) },
  { id: '1+5', label: '1 + 5', cols: 3, rows: 3, cells: grid(3, 3, [[0, 0, 2, 2]]) },
  { id: '3x3', label: '3 × 3', cols: 3, rows: 3, cells: grid(3, 3) },
  { id: '1+7', label: '1 + 7', cols: 4, rows: 4, cells: grid(4, 4, [[0, 0, 3, 3]]) },
  { id: '2+8', label: '2 + 8', cols: 4, rows: 4, cells: grid(4, 4, [[0, 0, 2, 2], [2, 0, 2, 2]]) },
  { id: '1+12', label: '1 + 12', cols: 4, rows: 4, cells: grid(4, 4, [[1, 1, 2, 2]]) },
  { id: '4x4', label: '4 × 4', cols: 4, rows: 4, cells: grid(4, 4) },
  { id: '5x5', label: '5 × 5', cols: 5, rows: 5, cells: grid(5, 5) },
];

/** The boxes of a preset in a `width` × `height` picture, in whole pixels. */
export function cellBoxes(p: LayoutPreset, width: number, height: number, gap: number, margin: number) {
  const cw = (width - 2 * margin - (p.cols - 1) * gap) / p.cols;
  const ch = (height - 2 * margin - (p.rows - 1) * gap) / p.rows;
  return p.cells.map(([c, r, cs, rs]) => {
    const x = Math.round(margin + c * (cw + gap));
    const y = Math.round(margin + r * (ch + gap));
    const x2 = Math.round(margin + (c + cs) * (cw + gap) - gap);
    const y2 = Math.round(margin + (r + rs) * (ch + gap) - gap);
    return { x, y, width: Math.max(1, x2 - x), height: Math.max(1, y2 - y) };
  });
}

/** The tiles of a template in reading order (top to bottom, left to right). */
function tilesOf(tpl: Template): Layer[] {
  const at = (l: Layer, p: 'x' | 'y') => num(propValue(l, p, 0));
  return tpl.layers.filter((l) => l.type === 'source' && !l.parent).sort((a, b) => at(a, 'y') - at(b, 'y') || at(a, 'x') - at(b, 'x'));
}

/**
 * Places the template's tiles on the preset: the first ones moved (their position and size no
 * longer animated), the missing ones added with the inputs no tile shows yet.
 */
export function applyLayout(tpl: Template, p: LayoutPreset, size: { width: number; height: number }, gap: number, margin: number, inputs: InputInfo[]) {
  const boxes = cellBoxes(p, size.width, size.height, gap, margin);
  const tiles = tilesOf(tpl);
  const used = new Set(tpl.layers.map((l) => (l.type === 'source' ? (l.input ?? '') : '')).filter(Boolean));
  const free = inputs.filter((i) => !used.has(i.id));
  boxes.forEach((b, i) => {
    let l = tiles[i];
    if (!l) {
      l = makeLayer('source', size.width, size.height);
      l.name = t('Source {n}', { n: i + 1 });
      const input = free.shift();
      if (input && l.type === 'source') l.input = input.id;
      tpl.layers.push(l);
    }
    for (const k of ['x', 'y', 'width', 'height'] as const) delete l.tracks[k];
    l.props = { ...l.props, ...b, scaleX: 1, scaleY: 1, rotation: 0 };
  });
}

/** A small drawing of a preset. */
export function PresetIcon({ p, w = 64 }: { p: LayoutPreset; w?: number }) {
  const h = Math.round((w * 9) / 16);
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} aria-hidden>
      {cellBoxes(p, w, h, 2, 2).map((b, i) => (
        <rect key={i} x={b.x} y={b.y} width={b.width} height={b.height} rx={1.5} fill="currentColor" opacity={i === 0 && p.cells[0][2] > 1 ? 0.9 : 0.55} />
      ))}
    </svg>
  );
}

/** The preset list, gap and margin: shared by the Layout window and the New template one. */
export function LayoutPicker({ value, onChange, gap, margin, onGap, onMargin, none }: { value: string; onChange: (id: string) => void; gap: number; margin: number; onGap: (v: number) => void; onMargin: (v: number) => void; none?: boolean }) {
  return (
    <div className="mv-layouts">
      <div className="mv-layout-grid">
        {none && (
          <button type="button" className={`mv-layout ${value === '' ? 'on' : ''}`} onClick={() => onChange('')}>
            <span className="mv-layout-empty" />
            <span>{t('Empty')}</span>
          </button>
        )}
        {PRESETS.map((p) => (
          <button type="button" key={p.id} className={`mv-layout ${value === p.id ? 'on' : ''}`} onClick={() => onChange(p.id)}>
            <PresetIcon p={p} />
            <span>{p.label}</span>
          </button>
        ))}
      </div>
      <div className="mv-layout-fields">
        <label className="set-field">
          <span className="set-label">{t('Gap')}</span>
          <input type="number" min={0} max={200} value={gap} onChange={(e) => onGap(Math.max(0, +e.target.value || 0))} />
        </label>
        <label className="set-field">
          <span className="set-label">{t('Margin')}</span>
          <input type="number" min={0} max={400} value={margin} onChange={(e) => onMargin(Math.max(0, +e.target.value || 0))} />
        </label>
      </div>
    </div>
  );
}

/** The Layout button of the Designer's bar and its window. */
export function LayoutButton({ project }: { project: Project }) {
  const [open, setOpen] = useState(false);
  const [preset, setPreset] = useState('2x2');
  const [gap, setGap] = useState(4);
  const [margin, setMargin] = useState(0);
  const inputs = useInputs();
  const apply = () => {
    const p = PRESETS.find((x) => x.id === preset);
    if (!p) return;
    const st = useDesigner.getState();
    const size = formatById(project, st.format);
    st.update((tpl) => applyLayout(tpl, p, size, gap, margin, inputs));
    setOpen(false);
  };
  return (
    <>
      <button className="btn small" onClick={() => setOpen(true)} title={t('Place the tiles on a grid: 2 × 2, 3 × 3, 1 + 5…')}>
        <LayoutGrid size={13} /> {t('Layout')}
      </button>
      {open &&
        createPortal(
          <div className="modal-back" onPointerDown={() => setOpen(false)}>
            <div className="modal mv-layout-modal" role="dialog" aria-label={t('Layout')} onPointerDown={(e) => e.stopPropagation()}>
              <header>
                <h2>{t('Layout')}</h2>
                <button type="button" className="icon-btn ghost" title={t('Close')} onClick={() => setOpen(false)}>
                  <X size={16} />
                </button>
              </header>
              <p className="muted">{t('The tiles are placed in reading order; the missing ones are added with the next inputs.')}</p>
              <LayoutPicker value={preset} onChange={setPreset} gap={gap} margin={margin} onGap={setGap} onMargin={setMargin} />
              <footer>
                <button className="btn" onClick={() => setOpen(false)}>
                  {t('Cancel')}
                </button>
                <button className="btn primary" onClick={apply}>
                  {t('Apply')}
                </button>
              </footer>
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}

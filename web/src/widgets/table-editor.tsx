// The Table editor, full screen like the Data and Logic canvases: the real table in the middle (click a column,
// the rows or the title to edit it; quick handles for column widths, row height, rows per page and text size),
// its structure on the left sorted by what is styled (table, title & header, rows, columns), and the settings of
// the selection on the right, in the inspector's groups.
import { Eye, EyeOff, GripVertical, Plus, Redo2, Table as TableIcon, Trash2, TriangleAlert, Undo2 } from 'lucide-react';
import { useShallow } from 'zustand/react/shallow';
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useBoundData } from '../data/dataStore';
import { t, tn } from '../i18n';
import { findLayer, useDesigner } from '../designer/designerStore';
import { holdFrame, num as vnum, resolveLayer } from '../engine/evaluate';
import type { Data, Layer, Template } from '../engine/types';
import { CanvasHeader } from '../ui/CanvasHeader';
import { ColorField } from '../ui/fields';
import { parseHex, toHex } from '../engine/evaluate';
import { Dropdown } from '../ui/Dropdown';
import { FontPick, WEIGHT_NAMES } from '../ui/FontPick';
import { Btns, Bound, Content, getPath, GridEditor, Note, Num, Row, Section, Seg, Select, setPath, Text, Toggle, useBound } from './kit';
import { cfg, colText, ColorPickRow, DST, emptyOr, FMTS, headerH, headerText, model, rowsText, TableRender, titleH, titleOr, titleText, widths, type Col, type Config } from './table';
import { useApp } from '../store';
import { colFor } from './table-build';
import { useTableEditor } from './table-editor-store';
import { exposeRows, unexposeRows } from './table-rows';
import type { CellText } from './table-text';

type TL = Layer & Config;
type Sel = { k: 'table' } | { k: 'title' } | { k: 'header' } | { k: 'rows' } | { k: 'rank' } | { k: 'col'; i: number };
const TYPE_ABBR: Record<string, string> = { text: 'txt', number: 'num', image: 'pic' };
type Edit = (fn: (l: TL) => void, key?: string) => void;
type Set = (path: string, v: unknown, coalesce?: boolean) => void;
type K = ReturnType<typeof cfg>;
type Live = ReturnType<typeof model>;

/** The editor of the Table being edited, if any (mounted once by the Designer). */
export function TableEditor() {
  const id = useTableEditor((s) => s.id);
  const close = useTableEditor((s) => s.close);
  const template = useDesigner((s) => s.template);
  const previewData = useDesigner((s) => s.previewData);
  const data = useBoundData(template, previewData);
  const layer = id && template ? findLayer(template, id) : undefined;
  useEffect(() => {
    if (id && (!layer || layer.type !== 'table')) close();
  }, [id, layer, close]);
  if (!template || !layer || layer.type !== 'table') return null;
  return createPortal(<Editor key={layer.id} layer={layer as TL} template={template} data={data} onClose={close} />, document.body);
}

/** Sample rows from the columns, to preview a table with no data yet. */
function sampleGrid(cols: Col[]): string[][] {
  const head = [...new Set(cols.flatMap((c) => [c.src ?? '', c.mark ?? '']).filter(Boolean))];
  const NAMES = ['Alpha', 'Bravo', 'Charlie', 'Delta', 'Echo', 'Foxtrot', 'Golf', 'Hotel', 'India'];
  const COLORS = ['#5c6fd6', '#22c55e', '#f5a524', '#ff3b5c', '#38bdf8', '#a78bfa', '#facc15', '#f472b6', '#94a3b8'];
  const rows = NAMES.map((n, r) =>
    head.map((h) => {
      if (cols.some((c) => c.mark === h)) return COLORS[r];
      const c = cols.find((x) => x.src === h);
      if (c?.type === 'number') return c.fmt === '+0' ? String(12 - r * 4) : String(40 - r * 4);
      if (c?.type === 'image') return '';
      return n;
    }),
  );
  return [head, ...rows];
}

function Editor({ layer, template, data, onClose }: { layer: TL; template: Template; data: Data; onClose: () => void }) {
  const { update, undo, redo, past, future } = useDesigner(useShallow((s) => ({ update: s.update, undo: s.undo, redo: s.redo, past: s.past, future: s.future })));
  const frame = useDesigner((s) => s.frame);
  const [sel, setSel] = useState<Sel>({ k: 'table' });
  const k = cfg(layer);
  const live = model(k, data);
  const [sample, setSample] = useState(() => !live.rows.length && k.cols.length > 0);
  const [page, setPage] = useState(0);
  // The rows' size: the animated fontSize at the playhead.
  const size = Math.max(1, vnum(resolveLayer(layer, frame, data, holdFrame(template)).fontSize as number));
  const edit: Edit = (fn, key) => update((t) => fn(findLayer(t, layer.id) as TL), key);
  const set: Set = (path, v, coalesce) => edit((l) => setPath(l, path, v), coalesce ? `${path}-${layer.id}` : undefined);
  // Columns follow the data until they are set: the first change writes them.
  const ensureCols = (l: TL) => {
    if (!l.cols?.length) l.cols = live.head.filter(Boolean).map((h) => ({ src: h, label: h, type: 'text', align: 'left', w: 100, pct: 10, fmt: '0', on: true }));
  };
  const cols = k.cols.length ? k.cols : live.head.filter(Boolean).map((h) => cfg({ cols: [{ src: h, label: h }] }).cols[0]);
  const selectCol = (i: number) => {
    if (!k.cols.length) edit(ensureCols);
    setSel({ k: 'col', i });
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (e.key === 'Escape' && !t.closest('input, textarea, select, .dd-list')) onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const shown: TL = sample ? { ...layer, link: '', grid: sampleGrid(cols) } : layer;
  const dsKey = /\{\{(\w+)__table\}\}/.exec(k.link)?.[1];
  const ds = dsKey ? template.datasets?.find((d) => d.key === dsKey) : undefined;
  const source = ds ? tn(live.rows.length, '{name} · {n} row', '{name} · {n} rows', { name: ds.name }) : k.link.trim() ? tn(live.rows.length, 'Linked data · {n} row', 'Linked data · {n} rows') : t('Rows by hand · {n}', { n: live.rows.length });
  const add = () => addCol(edit, ensureCols, live, setSel);
  // Width modes keep what is shown: switching writes the current widths in the new unit.
  const W = Math.max(50, vnum(resolveLayer(layer, frame, data, holdFrame(template)).width));
  const widthsAs = (mode: 'auto' | 'percent' | 'fixed') =>
    edit((l) => {
      ensureCols(l);
      const ws = widths(k, live, W, size);
      const sum = live.cols.reduce((a, c, j) => (c.i === undefined ? a : a + ws[j]), 0) || 1;
      live.cols.forEach((c, j) => {
        if (c.ci === undefined || !l.cols![c.ci]) return;
        if (mode === 'percent') l.cols![c.ci].pct = Math.round((ws[j] / sum) * 1000) / 10;
        if (mode === 'fixed') l.cols![c.ci].w = Math.round(ws[j]);
      });
      l.widthMode = mode;
    });
  // One column's share: the others share the rest, so the total stays 100 %.
  const setShare = (i: number, v: number) =>
    edit((l) => {
      ensureCols(l);
      const shown = live.cols.filter((c) => c.ci !== undefined).map((c) => c.ci!);
      const share = Math.min(95, Math.max(1, v));
      const others = shown.filter((x) => x !== i);
      const rest = others.reduce((a, x) => a + Math.max(0, +(l.cols![x].pct ?? 10)), 0);
      others.forEach((x) => (l.cols![x].pct = Math.round((rest > 0 ? (Math.max(0, +(l.cols![x].pct ?? 10)) / rest) * (100 - share) : (100 - share) / others.length) * 10) / 10));
      l.cols![i].pct = share;
    }, `te-share-${layer.id}`);
  const shareOf = (i: number) => {
    const shown = live.cols.filter((c) => c.ci !== undefined).map((c) => c.ci!);
    const sum = shown.reduce((a, x) => a + Math.max(0, +(k.cols[x]?.pct ?? 0)), 0) || 1;
    return Math.round((Math.max(0, +(k.cols[i]?.pct ?? 0)) / sum) * 100);
  };
  // Rows typed by the operator (a table without a data set).
  const linked = /\{\{\w+\}\}/.test(k.link);
  const opRows = { show: !linked, on: !!layer.settingBindings?.grid, toggle: () => update((t) => (layer.settingBindings?.grid ? unexposeRows : exposeRows)(t, findLayer(t, layer.id)!)) };
  // The rows typed here are also where the Operator starts from.
  const setGrid = (g: string[][]) =>
    update((t) => {
      const l = findLayer(t, layer.id) as TL;
      l.grid = g;
      const key = l.settingBindings?.grid;
      const f = key ? t.fields.find((x) => x.key === key) : undefined;
      if (f) f.default = JSON.stringify(g);
    }, `grid-${layer.id}`);

  return (
    <div className="logic-editor te-full" role="dialog" aria-label={t('Table editor: {name}', { name: layer.name })}>
      <CanvasHeader icon={<TableIcon size={15} />} title={layer.name} subject={template.name} onClose={onClose}>
        <span className={`te-chip ${ds || k.link.trim() ? 'data' : ''}`}>{source}</span>
        <div className="segmented te-seg" title={t('Preview the table with its data, or with sample rows')}>
          <button className={!sample ? 'on' : ''} onClick={() => setSample(false)}>
            {t('Live data')}
          </button>
          <button className={sample ? 'on' : ''} onClick={() => setSample(true)} disabled={!cols.length}>
            {t('Sample')}
          </button>
        </div>
        <button className="icon-btn ghost" title={t('Undo')} disabled={!past.length} onClick={undo}>
          <Undo2 size={15} />
        </button>
        <button className="icon-btn ghost" title={t('Redo')} disabled={!future.length} onClick={redo}>
          <Redo2 size={15} />
        </button>
        <button className="btn primary small" onClick={onClose}>
          {t('Done')}
        </button>
      </CanvasHeader>
      <div className="te-body">
        <Bound obj={layer} set={set} data={data}>
          <aside className="te-left">
            <Structure k={k} cols={cols} live={live} opRows={opRows} sel={sel} setSel={setSel} selectCol={selectCol} edit={edit} ensureCols={ensureCols} add={add} />
          </aside>
          <TableCanvas layer={shown} template={template} data={data} frame={frame} sel={sel} setSel={setSel} selectCol={selectCol} page={page} setPage={setPage} edit={edit} ensureCols={ensureCols} add={add} />
          <aside className="te-right">
            <Settings layer={layer} k={k} live={live} size={size} setGrid={setGrid} widthsAs={widthsAs} setShare={setShare} shareOf={shareOf} sel={sel} setSel={setSel} edit={edit} set={set} dsName={ds?.name} />
          </aside>
        </Bound>
      </div>
    </div>
  );
}

/** Adds the first column of the data not shown yet (else a new one) and selects it. */
function addCol(edit: Edit, ensureCols: (l: TL) => void, live: Live, setSel: (s: Sel) => void) {
  let at = 0;
  edit((l) => {
    ensureCols(l);
    const free = live.head.find((h) => h && !l.cols!.some((c) => c.src === h || c.mark === h));
    const i = free ? live.head.indexOf(free) : -1;
    l.cols!.push(free ? colFor(free, live.rows.map((r) => r.r[i] ?? '')) : { src: live.head[0] ?? '', label: 'New', type: 'text', align: 'left', w: 100, pct: 10, fmt: '0', on: true });
    at = l.cols!.length - 1;
  });
  setSel({ k: 'col', i: at });
}

// ------------------------------------------------------------ left --
function Structure({ k, cols, live, opRows, sel, setSel, selectCol, edit, ensureCols, add }: { k: K; cols: K['cols']; live: Live; opRows: { show: boolean; on: boolean; toggle: () => void }; sel: Sel; setSel: (s: Sel) => void; selectCol: (i: number) => void; edit: Edit; ensureCols: (l: TL) => void; add: () => void }) {
  const [drag, setDrag] = useState<{ from: number; over: number } | null>(null);
  const zebra = k.st.even.bg !== k.st.odd.bg || k.st.even.color !== k.st.odd.color;
  const tog = (label: string, on: boolean, fn: (l: TL, v: boolean) => void, title?: string) => (
    <button className={`te-tog ${on ? 'on' : ''}`} title={title} onClick={() => edit((l) => fn(l, !on))}>
      <span>{label}</span>
      <i />
    </button>
  );
  const move = (from: number, to: number) => {
    if (from === to) return;
    edit((l) => {
      ensureCols(l);
      const [c] = l.cols!.splice(from, 1);
      l.cols!.splice(to, 0, c);
    });
    if (sel.k === 'col' && sel.i === from) setSel({ k: 'col', i: to });
  };
  const unmapped = k.cols.length ? live.unmapped : [];
  const firstNum = cols.find((c) => c.on && c.type === 'number')?.src ?? cols[0]?.src ?? '';
  return (
    <>
      <div className="te-sec">
        <span className="te-lbl">{t('Columns')}</span>
        {k.showRank && (
          <button className={`te-item ${sel.k === 'rank' ? 'on' : ''}`} onClick={() => setSel({ k: 'rank' })}>
            <span className="te-grip" />
            <span className="te-nm"># {t('Rank')}</span>
            <span className={`te-aa ${k.txt.rank ? 'own' : ''}`} title={k.txt.rank ? t('Its own text') : t('Text of the rows')}>
              Aa
            </span>
            <span className="te-ty">auto</span>
          </button>
        )}
        {cols.map((c, i) => {
          const missing = live.missing.includes(c.src) && c.on;
          return (
            <div
              key={i}
              role="button"
              tabIndex={0}
              className={`te-item ${sel.k === 'col' && sel.i === i ? 'on' : ''} ${c.on ? '' : 'off'} ${drag?.over === i && drag.from !== i ? (drag.from < i ? 'drop-after' : 'drop-before') : ''}`}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.effectAllowed = 'move';
                e.dataTransfer.setData('text/plain', String(i));
                setDrag({ from: i, over: i });
              }}
              onDragOver={(e) => {
                if (!drag) return;
                e.preventDefault();
                if (drag.over !== i) setDrag({ ...drag, over: i });
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (drag) move(drag.from, i);
                setDrag(null);
              }}
              onDragEnd={() => setDrag(null)}
              onClick={() => selectCol(i)}
              onKeyDown={(e) => e.key === 'Enter' && selectCol(i)}
            >
              <span className="te-grip" title={t('Drag to reorder')}>
                <GripVertical size={12} />
              </span>
              <span className="te-nm">{c.label || c.src || '—'}</span>
              {c.type !== 'image' && (
                <span className={`te-aa ${c.txt ? 'own' : ''}`} title={c.txt ? t('Its own text') : t('Text of the rows')}>
                  Aa
                </span>
              )}
              {missing ? (
                <span className="te-warn" title={t("'{name}' is not in the data", { name: c.src })}>
                  <TriangleAlert size={12} />
                </span>
              ) : (
                <span className="te-ty">{t(TYPE_ABBR[c.type])}</span>
              )}
              <button
                className="te-eye"
                title={c.on ? t('Hide') : t('Show')}
                onClick={(e) => {
                  e.stopPropagation();
                  edit((l) => {
                    ensureCols(l);
                    l.cols![i] = { ...l.cols![i], on: !c.on };
                  });
                }}
              >
                {c.on ? <Eye size={13} /> : <EyeOff size={13} />}
              </button>
            </div>
          );
        })}
        {unmapped.map((h) => (
          <div key={h} className="te-new">
            <span>
              {t('New in the data:')} <b>{h}</b>
            </span>
            <button
              className="btn small"
              onClick={() =>
                edit((l) => {
                  const i = live.head.indexOf(h);
                  l.cols = [...(l.cols ?? []), colFor(h, live.rows.map((r) => r.r[i] ?? ''))];
                })
              }
            >
              {t('Add')}
            </button>
          </div>
        ))}
        <button className="te-add" onClick={add}>
          <Plus size={12} /> {t('Column')}
        </button>
      </div>
      <div className="te-sec">
        <span className="te-lbl">{t('Display')}</span>
        {tog(t('Title bar'), k.title.on, (l, v) => void (l.title = { ...l.title, on: v }))}
        {tog(t('Header row'), k.headerRow, (l, v) => void (l.headerRow = v))}
        {tog(t('Rank column'), k.showRank, (l, v) => void (l.showRank = v))}
        {tog(t('Header column'), k.headerColOn, (l, v) => {
          l.headerColOn = v;
          if (v && !l.headerCol) l.headerCol = cols.find((c) => c.type === 'text')?.src ?? '';
        })}
        {tog(t('Page dots'), k.indicator, (l, v) => void (l.indicator = v), t('Under the table, when there are several pages'))}
      </div>
      <div className="te-sec">
        <span className="te-lbl">{t('Options')}</span>
        {opRows.show && (
          <button className={`te-tog ${opRows.on ? 'on' : ''}`} title={t('The operator types the rows or imports a CSV in the Operator')} onClick={opRows.toggle}>
            <span>{t('Rows typed in the Operator')}</span>
            <i />
          </button>
        )}
        {tog(t('Odd / even rows differ'), zebra, (l, v) => void (l.st = { ...l.st, even: v ? { ...(k.st.odd.bg === DST.even.bg ? DST.odd : DST.even), color: k.st.odd.color } : { ...k.st.odd } }))}
        {tog(t('Top rows'), k.cf.top3, (l, v) => void (l.cf = { ...l.cf, top3: v }), t('The first rows marked; how many in the rows settings'))}
        {tog(t('Highlight marker'), k.cf.hlBar, (l, v) => void (l.cf = { ...l.cf, hlBar: v }), t('A bar on the row the operator highlights'))}
        {tog(t('Negatives in colour'), k.cf.neg, (l, v) => void (l.cf = { ...l.cf, neg: v }))}
        {tog(t('Sort'), !!k.sortCol, (l, v) => void (l.sortCol = v ? firstNum : ''))}
        {tog(t('Rows come in animated'), k.cascade.on, (l, v) => void (l.cascade = { ...l.cascade, on: v }))}
        {tog(t('Lines between rows'), k.borders.innerH.on, (l, v) => void (l.borders = { ...l.borders, innerH: { ...k.borders.innerH, on: v } }))}
        {tog(t('Lines between columns'), k.borders.innerV.on, (l, v) => void (l.borders = { ...l.borders, innerV: { ...k.borders.innerV, on: v } }))}
        {tog(t('Outer border'), k.borders.outer.on, (l, v) => void (l.borders = { ...l.borders, outer: { ...k.borders.outer, on: v } }))}
        {tog(t('Scroll instead of pages'), k.pageMode === 'scroll', (l, v) => void (l.pageMode = v ? 'scroll' : 'pages'))}
      </div>
    </>
  );
}

// ------------------------------------------------------------ canvas --
/** The level the text-size handle changes: its typography path and resolved text. */
function sizeTarget(k: K, sel: Sel, size: number): { path: string; t: CellText } {
  if (sel.k === 'title' && k.title.on) return { path: 'txt.title', t: titleText(k) };
  if (sel.k === 'header' && k.headerRow) return { path: 'txt.header', t: headerText(k) };
  if (sel.k === 'col' && k.cols[sel.i]?.txt) return { path: `cols.${sel.i}.txt`, t: colText(k, { col: k.cols[sel.i] as never, i: 0 }, size) };
  if (sel.k === 'rank' && k.txt.rank) return { path: 'txt.rank', t: colText(k, { col: { type: 'number' } as never }, size) };
  return { path: 'txt.rows', t: rowsText(k, size) };
}

function TableCanvas({ layer, template, data, frame, sel, setSel, selectCol, page, setPage, edit, ensureCols, add }: { layer: TL; template: Template; data: Data; frame: number; sel: Sel; setSel: (s: Sel) => void; selectCol: (i: number) => void; page: number; setPage: (p: number) => void; edit: Edit; ensureCols: (l: TL) => void; add: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  const [avail, setAvail] = useState(800);
  const [tip, setTip] = useState<{ x: number; y: number; text: string } | null>(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    // Small changes (a scrollbar showing up) do not rescale: no feedback loop.
    const ro = new ResizeObserver(() => setAvail((a) => (Math.abs(a - el.clientWidth) > 24 ? el.clientWidth : a)));
    ro.observe(el);
    setAvail(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const r = resolveLayer(layer, frame, data, holdFrame(template));
  const W = Math.max(50, vnum(r.width));
  const H = Math.max(50, vnum(r.height));
  const size = Math.max(1, vnum(r.fontSize as number));
  const k = cfg(layer);
  const m = model(k, data);
  const pages = Math.max(1, Math.ceil(m.rows.length / k.perPage));
  const pg = Math.min(page, pages - 1);
  const scroll = k.pageMode === 'scroll';
  // Rounded down: the scale never makes the canvas overflow by a pixel (no scrollbar flip-flop).
  const s = Math.max(0.15, Math.min(1, Math.floor(((avail - 180) / W) * 100) / 100));
  const ws = widths(k, m, W, size);
  const xs: number[] = [];
  ws.reduce((x, w, i) => ((xs[i] = x), x + w + k.gap), 0);
  const hh = headerH(k);
  const TH = titleH(k);
  const step = k.rowH + k.gap;
  const showHead = k.headerRow && (scroll || k.repeatHeader || pg === 0);
  const headY = k.title.on ? TH + k.gap : 0;
  const bodyY = headY + (showHead ? hh + k.gap : 0);
  const list = scroll ? m.rows.slice(0, k.perPage) : m.rows.slice(pg * k.perPage, pg * k.perPage + k.perPage);
  const bodyH = !m.rows.length ? k.rowH * 2 : Math.max(0, list.length * step - k.gap);
  const totalH = bodyY + bodyH;
  const viewH = Math.max(H, totalH);
  const colIndex = (j: number) => {
    const mc = m.cols[j];
    if (!mc || mc.i === undefined) return -1;
    return mc.ci ?? -1;
  };
  const selJ = sel.k === 'col' ? m.cols.findIndex((_, j) => colIndex(j) === sel.i) : sel.k === 'rank' ? m.cols.findIndex((c) => c.i === undefined) : -1;
  const pickCol = (j: number) => {
    if (m.cols[j]?.i === undefined) setSel({ k: 'rank' });
    else if (colIndex(j) >= 0) selectCol(colIndex(j));
  };

  /** Drags a handle: `on(dx, dy)` changes the layer and returns the value shown by the pointer. */
  const drag = (e: React.PointerEvent, on: (dx: number, dy: number) => string, done?: () => void) => {
    e.preventDefault();
    e.stopPropagation();
    const x0 = e.clientX;
    const y0 = e.clientY;
    const mv = (ev: PointerEvent) => setTip({ x: ev.clientX, y: ev.clientY, text: on((ev.clientX - x0) / s, (ev.clientY - y0) / s) });
    const up = () => {
      window.removeEventListener('pointermove', mv);
      window.removeEventListener('pointerup', up);
      setTip(null);
      useDesigner.setState({ lastCoalesce: null });
      done?.();
    };
    window.addEventListener('pointermove', mv);
    window.addEventListener('pointerup', up);
  };
  // Column edge: the two columns share their width (switches to % widths).
  const resize = (e: React.PointerEvent, j: number) => {
    const a = colIndex(j);
    const b = colIndex(j + 1);
    if (a < 0 || b < 0) return;
    const n = m.cols.length;
    const rk = m.cols.some((c) => c.i === undefined) ? 80 : 0;
    const space = W - k.gap * Math.max(0, n - 1) - rk;
    const fixed = k.widthMode === 'fixed';
    const base = m.cols.map((_, x) => (fixed ? ws[x] : (ws[x] / space) * 100));
    const key = `te-rz-${layer.id}`;
    drag(
      e,
      (dx) => {
        const d = fixed ? dx : (dx / space) * 100;
        const min = fixed ? 30 : 3;
        const total = base[j] + base[j + 1];
        const wa = Math.min(total - min, Math.max(min, base[j] + d));
        edit((l) => {
          ensureCols(l);
          if (!fixed) l.widthMode = 'percent';
          m.cols.forEach((_, x) => {
            const i = colIndex(x);
            if (i < 0) return;
            const v = x === j ? wa : x === j + 1 ? total - wa : base[x];
            l.cols![i] = { ...l.cols![i], [fixed ? 'w' : 'pct']: Math.round(v * 10) / 10 };
          });
        }, key);
        return fixed ? `${Math.round(wa)} px · ${Math.round(total - wa)} px` : `${Math.round(wa)} % · ${Math.round(total - wa)} %`;
      },
      () => selectCol(a),
    );
  };
  const rowHeight = (e: React.PointerEvent) => {
    const r0 = k.rowH;
    drag(e, (_, dy) => {
      const v = Math.round(Math.min(160, Math.max(30, r0 + dy)));
      edit((l) => void (l.rowH = v), `te-rowh-${layer.id}`);
      return t('Row {v} px', { v });
    });
  };
  const perPage = (e: React.PointerEvent) => {
    const p0 = k.perPage;
    drag(e, (_, dy) => {
      const v = Math.min(99, Math.max(1, p0 + Math.round(dy / step)));
      edit((l) => void (l.perPage = v), `te-per-${layer.id}`);
      setPage(0);
      return tn(v, '{n} row per page', '{n} rows per page');
    });
  };
  const target = sizeTarget(k, sel, size);
  const textSize = (e: React.PointerEvent) => {
    const t0 = target.t.size;
    drag(e, (dx) => {
      const v = Math.round(Math.min(200, Math.max(6, t0 + dx / 4)));
      edit((l) => setPath(l, `${target.path}.size`, v), `te-size-${layer.id}`);
      return t('Text {v} px', { v });
    });
  };
  const bandY = sel.k === 'title' && k.title.on ? TH / 2 : sel.k === 'header' && showHead ? headY + hh / 2 : bodyY + k.rowH / 2;

  const Z = ({ x, y, w, h, cls, title, onClick }: { x: number; y: number; w: number; h: number; cls: string; title?: string; onClick?: () => void }) => (
    <div
      className={`te-zone ${cls}`}
      title={title}
      style={{ left: x * s, top: y * s, width: w * s, height: h * s }}
      onClick={(e) => {
        e.stopPropagation();
        onClick?.();
      }}
    />
  );

  return (
    <div className="te-center">
      <div className="te-canvas" ref={box} onClick={() => setSel({ k: 'table' })}>
        <div className="te-stage" style={{ width: W * s, height: viewH * s }}>
          <div className="te-box" style={{ width: W * s, height: H * s }} />
          <div style={{ position: 'absolute', left: 0, top: 0, width: W, height: viewH, transform: `scale(${s})`, transformOrigin: '0 0', pointerEvents: 'none' }}>
            <TableRender layer={layer} r={r as never} w={W} data={data} time={{ nowMs: Date.now(), ageSecs: k.pageMode === 'scroll' ? 0 : 60 }} state={{ page: pg, ctl: true, hl: k.hlKey }} lang={useApp.getState().project?.language} />
          </div>
          {k.title.on && Z({ x: 0, y: 0, w: W, h: TH, cls: `title ${sel.k === 'title' ? 'on' : ''}`, title: t('Title bar'), onClick: () => setSel({ k: 'title' }) })}
          {list.map((_, i) => (
            <div key={i}>{Z({ x: 0, y: bodyY + i * step, w: W, h: k.rowH, cls: `row ${sel.k === 'rows' ? 'on' : ''}`, title: t('Rows'), onClick: () => setSel({ k: 'rows' }) })}</div>
          ))}
          {selJ >= 0 && Z({ x: xs[selJ], y: 0, w: ws[selJ], h: totalH, cls: 'colsel' })}
          {showHead && Z({ x: 0, y: headY, w: W, h: hh, cls: `head ${sel.k === 'header' ? 'on' : ''}`, title: t('Header row'), onClick: () => setSel({ k: 'header' }) })}
          {/* Column tabs above the table: a click selects the column. */}
          {m.cols.map((c, j) => (
            <div key={`c${j}`} className={`te-zone chead ${j === selJ ? 'on' : ''}`} title={c.i === undefined ? t('Rank column') : t('Column {name}', { name: c.col.label || c.col.src })} style={{ left: xs[j] * s, top: -28, width: ws[j] * s, height: 20 }} onClick={(e) => (e.stopPropagation(), pickCol(j))}>
              <span>{c.i === undefined ? '#' : c.col.label || c.col.src}</span>
            </div>
          ))}
          {/* Quick-edit handles */}
          {m.cols.map((c, j) =>
            j < m.cols.length - 1 && c.i !== undefined && m.cols[j + 1].i !== undefined ? <div key={`rz${j}`} className="te-cw" title={t('Column width')} style={{ left: (xs[j] + ws[j] + k.gap / 2) * s - 3, top: (showHead ? headY + hh / 2 : bodyY + k.rowH / 2) * s - 14 }} onPointerDown={(e) => resize(e, j)} onClick={(e) => e.stopPropagation()} /> : null,
          )}
          {list.length > 0 && (
            <div className="te-hd ns" title={t('Row height')} style={{ left: -30, top: (bodyY + k.rowH) * s - 9 }} onPointerDown={rowHeight} onClick={(e) => e.stopPropagation()}>
              ↕
            </div>
          )}
          {list.length > 0 && !scroll && (
            <div className="te-hd ns pill" title={t('Rows per page')} style={{ left: (W * s) / 2, top: totalH * s + 8, transform: 'translateX(-50%)' }} onPointerDown={perPage} onClick={(e) => e.stopPropagation()}>
              ⇕ {tn(k.perPage, '{n} row', '{n} rows')}
            </div>
          )}
          <div className="te-hd ew pill" title={t('Text size of the selection')} style={{ left: W * s + 12, top: bandY * s - 11 }} onPointerDown={textSize} onClick={(e) => e.stopPropagation()}>
            Aa {Math.round(target.t.size)}
          </div>
          <button
            className="te-plus"
            style={{ left: W * s + 8, top: -31 }}
            title={t('Add a column')}
            onClick={(e) => {
              e.stopPropagation();
              add();
            }}
          >
            <Plus size={14} />
          </button>
        </div>
        {!scroll && pages > 1 && (
          <div className="te-dots" onClick={(e) => e.stopPropagation()}>
            {Array.from({ length: pages }, (_, i) => (
              <button key={i} className={i === pg ? 'on' : ''} title={t('Page {n}', { n: i + 1 })} onClick={() => setPage(i)} />
            ))}
          </div>
        )}
        {!m.rows.length && !m.cols.length && <p className="te-empty">{t("No columns yet: wire a data set's All rows to the table in the Data canvas, or type rows in Pages, animation, sort › Source.")}</p>}
      </div>
      {tip && createPortal(<div className="te-readv" style={{ left: tip.x + 14, top: tip.y + 16 }}>{tip.text}</div>, document.body)}
    </div>
  );
}

// ------------------------------------------------------------ right --
/** Font, weight, size, spacing, case, line, style of a level (`path` holds its Typo). */
function TypoRows({ path, t: tx }: { path: string; t: CellText }) {
  const b = useBound();
  return (
    <>
      <Row label={t('Font')} expose={[`${path}.family`, `${path}.weight`]} exposeValue={{ [`${path}.family`]: tx.family, [`${path}.weight`]: tx.weight }}>
        <FontPick
          family={tx.family}
          weight={tx.weight}
          onChange={(f, w) => {
            b.set(`${path}.family`, f);
            b.set(`${path}.weight`, w);
          }}
        />
      </Row>
      <Num label={t('Size')} path={`${path}.size`} value={Math.round(tx.size)} min={4} max={300} unit="px" />
      <Num label={t('Spacing')} path={`${path}.spacing`} value={tx.spacing} min={-20} max={100} step={0.5} precision={1} unit="px" />
      <Seg
        label={t('Case')}
        path={`${path}.case`}
        value={tx.case}
        options={[
          ['none', '—', t('As typed')],
          ['upper', 'AA', t('Uppercase')],
          ['lower', 'aa', t('Lowercase')],
          ['title', 'Aa', t('Capitalized')],
        ]}
      />
      <Seg
        label={t('Style')}
        path={`${path}.italic`}
        value={tx.italic}
        options={[
          [false, t('Upright')],
          [true, <i key="i">{t('Italic')}</i>],
        ]}
      />
    </>
  );
}

/** One line or several, and what a too-long text does. */
function LinesRows({ path, t: tx }: { path: string; t: CellText }) {
  return (
    <>
      <Seg
        label={t('Lines')}
        path={`${path}.lines`}
        value={tx.multi ? 'multi' : 'one'}
        options={[
          ['one', t('One line')],
          ['multi', t('Several lines')],
        ]}
      />
      {tx.multi && (
        <>
          <Num label={t('Max lines')} path={`${path}.max`} value={tx.max} min={2} max={9} />
          <Num label={t('Line height')} path={`${path}.lh`} value={tx.lh} min={0.7} max={3} step={0.05} precision={2} unit="×" />
          <Seg
            label={t('Line break')}
            path={`${path}.brk`}
            value={tx.anyBreak ? 'any' : 'words'}
            options={[
              ['words', t('Between words')],
              ['any', t('Anywhere')],
            ]}
          />
        </>
      )}
      <Seg
        label={t('Too long')}
        path={`${path}.fit`}
        value={tx.fit}
        options={[
          ['fit', t('Fit')],
          ['ellipsis', t('Ellipsis …')],
          ['clip', t('Cut')],
        ]}
      />
      {tx.fit === 'fit' && (
        <>
          {!tx.multi && <Num label={t('Narrower to')} path={`${path}.narrow`} value={Math.round(tx.narrow)} min={10} max={100} unit="%" />}
          <Num label={t('Smaller to')} path={`${path}.small`} value={Math.round(tx.small)} min={10} max={100} unit="%" />
          <Seg
            label={t('Then')}
            path={`${path}.then`}
            value={tx.then}
            options={[
              ['ellipsis', t('Ellipsis …')],
              ['clip', t('Cut')],
            ]}
          />
          <Note>{tx.multi ? t('The text wraps up to the max lines, then gets smaller; what still does not fit ends with the choice above.') : t('The text first gets narrower, then smaller; what still does not fit ends with the choice above.')}</Note>
        </>
      )}
    </>
  );
}

/** Text and Lines sections of a level. */
function TextSections({ id, path, t: tx }: { id: string; path: string; t: CellText }) {
  return (
    <>
      <Section id={`te-${id}-text`} title={t('Text')}>
        <TypoRows path={path} t={tx} />
      </Section>
      <Section id={`te-${id}-lines`} title={t('Lines & too long text')}>
        <LinesRows path={path} t={tx} />
      </Section>
    </>
  );
}

/** Background and text colours of a level, one per row. Both are always written together, so the
 * renderer on air never fills a missing one with another level's default. */
function Colours({ path }: { path: string }) {
  const b = useBound();
  const v = (getPath(b.obj, path) ?? {}) as { bg?: string; color?: string };
  const d = (getPath({ st: DST, cell: { bg: '#facc15', color: '#111111' } }, path) ?? {}) as { bg?: string; color?: string };
  const pair = { bg: v.bg ?? d.bg ?? '#000000', color: v.color ?? d.color ?? '#ffffff' };
  const one = (key: 'bg' | 'color', label: string) => (
    <Row expose={[`${path}.bg`, `${path}.color`]} exposeValue={{ [`${path}.bg`]: pair.bg, [`${path}.color`]: pair.color }} label={label}>
      <ColorField value={parseHex(pair[key]) ?? [1, 1, 1, 1]} onChange={(c) => b.set(path, { ...(getPath(b.obj, path) as object), ...pair, [key]: toHex(c, c[3] < 1) }, true)} />
    </Row>
  );
  return (
    <>
      {one('bg', t('Background'))}
      {one('color', t('Text'))}
    </>
  );
}

/** The row a merge or the single cell is on: its key (it follows the team), or its rank for older settings. */
function RowKey({ label, path, live, k }: { label: string; path: string; live: Live; k: { key?: string; row?: number } }) {
  const b = useBound();
  const key = k.key?.trim() ?? '';
  // Older settings: the row at that rank now.
  const value = key || live.rows.find((r) => r.rank === +(k.row ?? 1))?.key || '';
  const opts = [...(key && !live.rows.some((r) => r.key === key) ? [{ value: key, label: key }] : []), ...live.rows.map((r) => ({ value: r.key, label: `${r.rank}. ${r.key}` }))];
  return (
    <Row label={label}>
      <Dropdown
        value={value}
        options={opts}
        onChange={(v) => {
          b.set(`${path}.key`, v);
          const r = live.rows.find((x) => x.key === v);
          if (r) b.set(`${path}.row`, r.rank);
        }}
      />
    </Row>
  );
}

function Settings({ layer, k, live, size, setGrid, widthsAs, setShare, shareOf, sel, setSel, edit, set, dsName }: { layer: TL; k: K; live: Live; size: number; setGrid: (g: string[][]) => void; widthsAs: (m: 'auto' | 'percent' | 'fixed') => void; setShare: (i: number, v: number) => void; shareOf: (i: number) => number; sel: Sel; setSel: (s: Sel) => void; edit: Edit; set: Set; dsName?: string }) {
  const head = live.head.filter(Boolean);
  const colOpts: [string, string][] = k.cols.map((c) => [c.src, c.label || c.src]);
  const zebra = k.st.even.bg !== k.st.odd.bg || k.st.even.color !== k.st.odd.color;
  const wOpts: [number, string][] = [[0, t('Like the rows')], ...[400, 500, 600, 700, 800, 900].map((w) => [w, t(WEIGHT_NAMES[w])] as [number, string])];
  // The rows by their key (their team…): what follows them when the order changes.
  const keyOpts: [string, string][] = [['', t('— none —')], ...(k.hlKey && !live.rows.some((r) => r.key === k.hlKey) ? [[k.hlKey, k.hlKey] as [string, string]] : []), ...live.rows.map((r) => [r.key, `${r.rank}. ${r.key}`] as [string, string])];
  const ALIGN: [string, string][] = [
    ['left', t('Left')],
    ['center', t('Centre')],
    ['right', t('Right')],
  ];
  const title = (badge: string, name: string, extra?: ReactNode) => (
    <div className="te-selname">
      <span className="te-badge">{badge}</span>
      <b>{name}</b>
      {extra}
    </div>
  );
  const toRows = (
    <button className="te-link" onClick={() => setSel({ k: 'rows' })}>
      {t("Edit the rows' text →")}
    </button>
  );

  if (sel.k === 'title') {
    const grad = !k.titleBg.trim();
    return (
      <div className="te-form">
        {title(t('Title'), t('Title bar'))}
        <Section id="te-ti-content" title={t('Content')}>
          <Content label={t('Text')} path="title.text" value={k.title.text} placeholder={titleOr({ ...k, title: { ...k.title, text: '' } }, useApp.getState().project?.language)} />
        </Section>
        <TextSections id="ti" path="txt.title" t={titleText(k)} />
        <Section id="te-ti-col" title={t('Colours')}>
          <Seg
            label={t('Background')}
            value={grad ? 'grad' : 'solid'}
            onChange={(v) => set('titleBg', v === 'solid' ? k.titleTo : '')}
            options={[
              ['grad', t('Gradient')],
              ['solid', t('Solid')],
            ]}
          />
          {grad ? (
            <>
              <ColorPickRow label={t('From')} path="titleFrom" value={k.titleFrom} />
              <ColorPickRow label={t('To')} path="titleTo" value={k.titleTo} />
            </>
          ) : (
            <ColorPickRow label={t('Colour')} path="titleBg" value={k.titleBg} />
          )}
          <ColorPickRow label={t('Text')} path="titleColor" value={k.titleColor} />
        </Section>
        <Section id="te-ti-layout" title={t('Layout')}>
          <Num label={t('Height')} path="titleH" value={k.titleH} min={20} max={300} unit="px" />
          <Seg label={t('Align')} path="titleAlign" value={k.titleAlign} options={ALIGN} />
          <Num label={t('Side margin')} path="titlePad" value={k.titlePad} min={0} max={300} unit="px" />
        </Section>
      </div>
    );
  }
  if (sel.k === 'header') {
    return (
      <div className="te-form">
        {title(t('Header'), t('Header row'))}
        <TextSections id="hd" path="txt.header" t={headerText(k)} />
        <Section id="te-hd-col" title={t('Colours')}>
          <Colours path="st.header" />
        </Section>
        <Section id="te-hd-layout" title={t('Layout')}>
          <Num label={t('Height')} expose="headH" exposeValue={headerH(k)} value={headerH(k)} min={16} max={300} unit="px" onChange={(v) => set('headH', v, true)} />
          <Seg label={t('Align')} value={k.headAlign ?? 'cols'} onChange={(v) => set('headAlign', v === 'cols' ? null : v)} options={[['cols', t('Columns')], ...ALIGN]} />
          {k.pageMode === 'pages' && <Toggle label={t('Each page')} path="repeatHeader" value={k.repeatHeader} text={t('Repeat on every page')} />}
        </Section>
      </div>
    );
  }
  if (sel.k === 'rows') {
    return (
      <div className="te-form">
        {title(t('Rows'), t('All the rows'))}
        <TextSections id="rw" path="txt.rows" t={rowsText(k, size)} />
        <Section id="te-rw-col" title={zebra ? t('Odd rows') : t('Colours')}>
          <Colours path="st.odd" />
        </Section>
        {zebra && (
          <Section id="te-rw-even" title={t('Even rows')}>
            <Colours path="st.even" />
          </Section>
        )}
        {k.cf.top3 && (
          <Section id="te-rw-top" title={t('Top rows')}>
            <Num label={t('How many')} path="cf.topN" value={k.cf.topN} min={1} max={50} />
            <Colours path="st.top3" />
            <Toggle label={t('Accent bar')} path="cf.topBar" value={k.cf.topBar} text={t('On the left')} />
            <ColorPickRow label={t('Accent')} path="cf.top3Color" value={k.cf.top3Color} />
            <Select label={t('Weight')} path="kw.top3" value={k.kw.top3 ?? 0} options={wOpts} />
          </Section>
        )}
        <Section id="te-rw-hl" title={t('Highlighted row')} closed>
          <Colours path="st.hl" />
          {k.cf.hlBar && <ColorPickRow label={t('Marker')} path="cf.hlBarColor" value={k.cf.hlBarColor} />}
          <Select label={t('Weight')} path="kw.hl" value={k.kw.hl ?? 0} options={wOpts} />
          <Select label={t('From the In')} path="hlKey" value={k.hlKey} options={keyOpts} />
        </Section>
        <Section id="te-rw-data" title={t('Colour from the data')}>
          <Row label={t('Column')}>
            <Dropdown value={k.rowFrom} options={[{ value: '', label: t('None') }, ...head.map((h) => ({ value: h, label: h }))]} onChange={(v) => set('rowFrom', v)} />
          </Row>
          {k.rowFrom && (
            <Seg
              label={t('Shows as')}
              path="rowPaint"
              value={k.rowPaint}
              options={[
                ['bar', t('Bar')],
                ['bg', t('Background')],
                ['text', t('Text')],
              ]}
            />
          )}
          <Note>{t("Each row takes the colour written in this column (#rrggbb), e.g. the team's colour.")}</Note>
        </Section>
        {k.headerColOn && (
          <Section id="te-rw-hcol" title={t('Header column')}>
            <Select label={t('Column')} path="headerCol" value={k.headerCol} options={colOpts.length ? colOpts : [['', '—']]} />
            <Colours path="st.hcol" />
          </Section>
        )}
        <Section id="te-rw-size" title={t('Size & spacing')}>
          <Num label={t('Row height')} path="rowH" value={k.rowH} min={20} max={300} unit="px" />
          <Num label={t('Padding')} path="pad" value={k.pad} min={0} max={200} unit="px" />
          <Num label={t('Gap')} path="gap" value={k.gap} min={0} max={100} unit="px" />
          <Num label={t('Corners')} path="radius" value={k.radius} min={0} max={100} unit="px" />
        </Section>
        {k.cf.neg && (
          <Section id="te-rw-num" title={t('Negative numbers')} closed>
            <ColorPickRow label={t('Colour')} path="cf.negColor" value={k.cf.negColor} />
          </Section>
        )}
        {k.borders.innerH.on && (
          <Section id="te-rw-lines" title={t('Lines between rows')} closed>
            <Num label={t('Width')} path="borders.innerH.width" value={k.borders.innerH.width} min={1} max={50} unit="px" />
            <ColorPickRow label={t('Colour')} path="borders.innerH.color" value={k.borders.innerH.color} />
          </Section>
        )}
      </div>
    );
  }
  if (sel.k === 'rank' || sel.k === 'col') {
    const rank = sel.k === 'rank';
    const i = rank ? -1 : sel.i;
    const c = rank ? undefined : k.cols[i];
    if (!rank && !c) return <Note>{t('Select a column.')}</Note>;
    const p = rank ? 'txt.rank' : `cols.${i}.txt`;
    const own = rank ? k.txt.rank : c!.txt;
    const ct = colText(k, rank ? { col: { type: 'number' } as never } : { col: c as never, i: 0 }, size);
    const cp = `cols.${i}`;
    const srcs = [...new Set([...head, c?.src ?? ''])].filter(Boolean);
    const sorted = !!c && k.sortCol === c.src;
    const pic = c?.type === 'image';
    const img = { fit: 'contain', size: 50, width: 80, mask: 'none', radius: 8, opacity: 100, fallback: '', ...c?.img };
    return (
      <div className="te-form">
        {title(
          t('Column'),
          rank ? t('Rank') : c!.label || c!.src,
          !rank && (
            <span className="te-acts">
              <button className="icon-btn" title={c!.on ? t('Hide') : t('Show')} onClick={() => edit((l) => void (l.cols![i] = { ...l.cols![i], on: !c!.on }))}>
                {c!.on ? <Eye size={14} /> : <EyeOff size={14} />}
              </button>
              <button
                className="icon-btn"
                title={t('Remove')}
                onClick={() => {
                  edit((l) => void l.cols!.splice(i, 1));
                  setSel({ k: 'table' });
                }}
              >
                <Trash2 size={14} />
              </button>
            </span>
          ),
        )}
        {rank ? (
          <Section id="te-rank" title={t('Column')}>
            <Note>{t('The position of each row once sorted (1, 2, 3…). Top rows show it in their accent colour.')}</Note>
          </Section>
        ) : (
          <Section id="te-col" title={t('Column')}>
            <Row label={t('Data')}>
              <Dropdown value={c!.src} options={srcs.length ? srcs.map((v) => ({ value: v, label: v })) : [{ value: '', label: '—' }]} onChange={(v) => set(`${cp}.src`, v)} />
            </Row>
            <Text label={t('Header')} path={`${cp}.label`} value={c!.label} />
            <Seg
              label={t('Shows')}
              path={`${cp}.type`}
              value={c!.type}
              expose={false}
              options={[
                ['text', t('Text')],
                ['number', t('Number')],
                ['image', t('Picture')],
              ]}
            />
            <Seg label={t('Align')} path={`${cp}.align`} value={c!.align} options={ALIGN} />
            {k.widthMode === 'percent' && <Num label={t('Width')} value={shareOf(i)} min={1} max={95} unit="%" onChange={(v) => setShare(i, v)} />}
            {k.widthMode === 'fixed' && <Num label={t('Width')} path={`${cp}.w`} value={c!.w} min={10} max={4000} unit="px" />}
            {c!.type === 'number' && <Seg label={t('Format')} path={`${cp}.fmt`} value={c!.fmt} options={FMTS} />}
            {!pic && (
              <>
                <Text label={t('Before')} path={`${cp}.prefix`} value={c!.prefix ?? ''} placeholder={t('e.g. €')} />
                <Text label={t('After')} path={`${cp}.suffix`} value={c!.suffix ?? ''} placeholder={t('e.g. pts')} />
              </>
            )}
            {!pic && (
              <>
                <Row label={t('Text colour')}>
                  <Dropdown value={c!.colorFrom ?? ''} options={[{ value: '', label: t('Rows colour') }, ...head.filter((h) => h !== c!.src).map((h) => ({ value: h, label: t('From {name}', { name: h }) }))]} onChange={(v) => set(`${cp}.colorFrom`, v)} />
                </Row>
                <Row label={t('Background')}>
                  <Dropdown value={c!.bgFrom ?? ''} options={[{ value: '', label: t('Rows colour') }, ...head.filter((h) => h !== c!.src).map((h) => ({ value: h, label: t('From {name}', { name: h }) }))]} onChange={(v) => set(`${cp}.bgFrom`, v)} />
                </Row>
              </>
            )}
            {c!.type === 'text' && (
              <Row label={t('Colour mark')}>
                <Dropdown value={c!.mark} options={[{ value: '', label: t('None') }, ...head.filter((h) => h !== c!.src).map((h) => ({ value: h, label: t('From {name}', { name: h }) }))]} onChange={(v) => set(`${cp}.mark`, v)} />
              </Row>
            )}
            <Toggle label={t('Sort')} value={sorted} text={t('Sort on this column')} onChange={(v) => set('sortCol', v ? c!.src : '')} />
            {sorted && (
              <Seg
                label={t('Order')}
                path="sortDir"
                value={k.sortDir}
                options={[
                  ['desc', t('Biggest first')],
                  ['asc', t('Smallest first')],
                ]}
              />
            )}
          </Section>
        )}
        {pic ? (
          <>
            <Section id="te-pic-fit" title={t('Picture')}>
              <Seg
                label={t('Fit')}
                path={`${cp}.img.fit`}
                value={img.fit}
                options={[
                  ['contain', t('Contain')],
                  ['cover', t('Cover')],
                  ['stretch', t('Stretch')],
                ]}
              />
              <Num label={t('Height')} path={`${cp}.img.size`} value={img.size} min={5} max={100} unit="%" />
              {img.fit !== 'contain' && img.mask !== 'circle' && <Num label={t('Width')} path={`${cp}.img.width`} value={img.width} min={5} max={100} unit="%" />}
              <Num label={t('Opacity')} path={`${cp}.img.opacity`} value={img.opacity} min={0} max={100} unit="%" />
              <Content label={t('If empty')} path={`${cp}.img.fallback`} value={img.fallback} placeholder={t('/assets/… picture')} />
            </Section>
            <Section id="te-pic-shape" title={t('Shape')}>
              <Seg
                label={t('Mask')}
                path={`${cp}.img.mask`}
                value={img.mask}
                options={[
                  ['none', t('None')],
                  ['rounded', t('Rounded')],
                  ['circle', t('Circle')],
                ]}
              />
              {img.mask === 'rounded' && <Num label={t('Radius')} path={`${cp}.img.radius`} value={img.radius} min={0} max={500} unit="px" />}
            </Section>
          </>
        ) : (
          <>
            <Section id="te-col-text" title={t('Text')}>
              <Seg
                label={t('Uses')}
                value={own ? 'own' : 'rows'}
                onChange={(v) => edit((l) => (v === 'own' ? setPath(l, p, {}) : rank ? void (l.txt = { ...l.txt, rank: undefined }) : void delete l.cols![i].txt))}
                options={[
                  ['rows', t('The rows’ text')],
                  ['own', t('Its own text')],
                ]}
              />
              {own ? (
                <TypoRows path={p} t={ct} />
              ) : (
                <div className="te-inherit">
                  <span>
                    <b>{ct.family}</b> {WEIGHT_NAMES[ct.weight] ? t(WEIGHT_NAMES[ct.weight]) : ct.weight} · {Math.round(ct.size)} px
                  </span>
                  {toRows}
                </div>
              )}
            </Section>
            {own && (
              <Section id="te-col-lines" title={t('Lines & too long text')}>
                <LinesRows path={p} t={ct} />
              </Section>
            )}
          </>
        )}
      </div>
    );
  }
  // The table itself.
  return (
    <div className="te-form">
      {title(t('Table'), layer.name)}
      <Section id="te-pages" title={t('Pages')}>
        <Num label={t('Rows per page')} path="perPage" value={k.perPage} min={1} max={99} />
        {k.pageMode === 'scroll' && <Num label={t('Scroll speed')} path="scrollSpeed" value={k.scrollSpeed} min={1} max={2000} unit="px/s" />}
        {k.pageMode === 'pages' && k.indicator && <ColorPickRow label={t('Page dots colour')} path="indicatorColor" value={k.indicatorColor} />}
        <Select label={t('Highlighted row')} path="hlKey" value={k.hlKey} options={keyOpts} />
      </Section>
      {k.cascade.on && (
        <Section id="te-cascade" title={t('Rows come in')}>
          <Seg
            label={t('Style')}
            path="cascade.style"
            value={k.cascade.style}
            options={[
              ['slide', t('Slide')],
              ['fade', t('Fade')],
              ['flip', t('Flip')],
            ]}
          />
          <Num label={t('Offset')} path="cascade.offset" value={k.cascade.offset} min={0} max={5000} step={5} unit="ms" />
          <Num label={t('Duration')} path="cascade.dur" value={k.cascade.dur} min={10} max={5000} step={10} unit="ms" />
        </Section>
      )}
      {!!k.sortCol && (
        <Section id="te-sort" title={t('Sort')}>
          <Select label={t('Column')} path="sortCol" value={k.sortCol} options={colOpts.length ? colOpts : [[k.sortCol, k.sortCol]]} />
          <Seg
            label={t('Order')}
            path="sortDir"
            value={k.sortDir}
            options={[
              ['desc', t('Biggest first')],
              ['asc', t('Smallest first')],
            ]}
          />
        </Section>
      )}
      <Section id="te-source" title={t('Source')} closed>
        {/\{\{\w+\}\}/.test(k.link) ? (
          <>
            <Row label={t('Rows')}>
              <span className="te-chip data">{dsName ? t('{name} · All rows', { name: dsName }) : t('Linked data')}</span>
            </Row>
            <Note>{t('{rows} · {cols}. Wired in the Data canvas.', { rows: tn(live.rows.length, '{n} row', '{n} rows'), cols: tn(head.length, '{n} column', '{n} columns') })}</Note>
          </>
        ) : (
          <>
            <Content label={t('Rows')} path="link" placeholder="{{key}}" />
            {!k.link.trim() && (
              <>
                <div className="te-grid">
                  <GridEditor value={k.grid} onChange={setGrid} header csv />
                </div>
                {layer.settingBindings?.grid && <Note>{t('The Operator starts from these rows; the operator can type others or import a CSV.')}</Note>}
              </>
            )}
          </>
        )}
      </Section>
      <Section id="te-widths" title={t('Column widths')} closed>
        <Seg
          label={t('Widths')}
          value={k.widthMode}
          onChange={(v) => widthsAs(v as 'auto')}
          options={[
            ['auto', t('Auto')],
            ['percent', '%'],
            ['fixed', 'px'],
          ]}
        />
      </Section>
      <Section id="te-bg" title={t('Background')} closed>
        <ColorPickRow label={t('Colour')} path="st.tableBg" value={k.st.tableBg} />
      </Section>
      {(k.borders.outer.on || k.borders.innerV.on) && (
        <Section id="te-borders" title={t('Borders')} closed>
          {(
            [
              ['outer', 'Outer width', 'Outer colour'],
              ['innerV', 'Between columns width', 'Between columns colour'],
            ] as const
          )
            .filter(([b]) => k.borders[b].on)
            .map(([b, wLab, cLab]) => (
              <div key={b}>
                <Num label={t(wLab)} path={`borders.${b}.width`} value={k.borders[b].width} min={1} max={50} unit="px" />
                <ColorPickRow label={t(cLab)} path={`borders.${b}.color`} value={k.borders[b].color} />
              </div>
            ))}
        </Section>
      )}
      <Section id="te-merges" title={t('Merged cells')} closed>
        <Btns list={[{ label: k.merges.length ? t('+ Merge') : t('Merge cells'), cls: 'ghost', onClick: () => set('merges', [...k.merges, { on: true, key: live.rows[1]?.key ?? '', row: 2, col: 1, span: 2, text: '' }]) }]} />
        {k.merges.map((g, x) => (
          <div key={x} className="te-merge">
            <RowKey label={t('Row')} path={`merges.${x}`} live={live} k={g} />
            <Num label={t('From column')} path={`merges.${x}.col`} value={g.col} min={1} />
            <Num label={t('Cells')} path={`merges.${x}.span`} value={g.span} min={2} />
            <Text label={t('Text')} path={`merges.${x}.text`} value={g.text} placeholder={t('Keep the first cell')} />
            <Btns list={[{ label: t('Remove'), cls: 'ghost', onClick: () => set('merges', k.merges.filter((_, q) => q !== x)) }]} />
          </div>
        ))}
      </Section>
      <Section id="te-cell" title={t('One cell')} closed>
        <Toggle label={t('Shown')} path="cell.on" value={k.cell.on} />
        {k.cell.on && (
          <>
            <RowKey label={t('Row')} path="cell" live={live} k={k.cell} />
            <Num label={t('Column')} path="cell.col" value={k.cell.col} min={1} />
            <Colours path="cell" />
          </>
        )}
      </Section>
      <Section id="te-empty" title={t('No rows')} closed>
        <Seg
          label={t('Shows')}
          path="emptyMode"
          value={k.emptyMode}
          options={[
            ['message', t('Message')],
            ['hide', t('Nothing')],
          ]}
        />
        {k.emptyMode === 'message' && <Text label={t('Message')} path="emptyText" value={k.emptyText} placeholder={emptyOr({ ...k, emptyText: '' }, useApp.getState().project?.language)} />}
      </Section>
    </div>
  );
}

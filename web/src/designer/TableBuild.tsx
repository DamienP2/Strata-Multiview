import { Check, Pencil, Sparkles, X } from 'lucide-react';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { interpolateText } from '../engine/evaluate';
import type { Data, Layer, Template } from '../engine/types';
import { Dropdown } from '../ui/Dropdown';
import { NumberField } from '../ui/fields';
import { cfg, model, parseRows, type Config } from '../widgets/table';
import { applyBuild, colFor, colsOf, defaultOpts, detect, ROLES, type BuildOpts, type Detected, type Role } from '../widgets/table-build';
import { useTableEditor } from '../widgets/table-editor-store';
import { findLayer, useDesigner } from './designerStore';
import { t, tn } from '../i18n';

/*
 * A Table in the Data canvas: once a data set's All rows is wired into it, Strata reads the columns
 * (type, guessed role) and builds the table in one go; afterwards a new column of the data is offered,
 * never added silently.
 */

type TL = Layer & Config;
const KIND: Record<Detected['kind'], string> = {
  text: 'text',
  num: 'num',
  color: 'color',
  pic: 'pic',
};

/** The data set a table shows (its `{{key__table}}`). */
export function tableDataset(l: Layer, template: Template) {
  const key = /\{\{(\w+)__table\}\}/.exec(String((l as TL).link ?? ''))?.[1];
  return key ? { key, ds: template.datasets?.find((d) => d.key === key) } : null;
}

/** Under a Table node: its columns, a new column of the data, Build / Edit. */
export function TableNodeInfo({ layer, values, onBuild }: { layer: Layer; values: Data; onBuild: () => void }) {
  const update = useDesigner((s) => s.update);
  const l = layer as TL;
  const k = cfg(l);
  const wired = /\{\{\w+\}\}/.test(k.link);
  const m = model(k, values);
  const shown = k.cols.filter((c) => c.on);
  return (
    <div className="ln-body tb-info">
      {wired && !k.cols.length ? (
        <button className="tb-build" onClick={onBuild} title={t('Make the columns from the data')}>
          <Sparkles size={12} /> {t('Build the table')}
        </button>
      ) : (
        shown.length > 0 && (
          <span className="tb-cols" title={shown.map((c) => c.label || c.src).join(', ')}>
            {tn(shown.length, '{n} column · {names}', '{n} columns · {names}', { names: shown.map((c) => c.label || c.src).join(', ') })}
          </span>
        )
      )}
      {k.cols.length > 0 &&
        m.unmapped.map((h) => (
          <div key={h} className="tb-new">
            <span>
              {t('New column')} <b>{h}</b>
            </span>
            <button
              className="btn small"
              onClick={() =>
                update((t) => {
                  const x = findLayer(t, l.id) as TL;
                  const i = m.head.indexOf(h);
                  x.cols = [
                    ...(x.cols ?? []),
                    colFor(
                      h,
                      m.rows.map((r) => r.r[i] ?? ''),
                    ),
                  ];
                })
              }
            >
              {t('Add')}
            </button>
          </div>
        ))}
      <button className="tb-edit" onClick={() => useTableEditor.getState().open(l.id)}>
        <Pencil size={11} /> {t('Edit the table')}
      </button>
    </div>
  );
}

/** The card next to a Table node: the columns found in the data, then Build. */
export function BuildCard({ layer, template, values, onClose }: { layer: Layer; template: Template; values: Data; onClose: () => void }) {
  const update = useDesigner((s) => s.update);
  const l = layer as TL;
  const grid = useMemo(() => parseRows(interpolateText(String(l.link ?? ''), values)), [l.link, values]);
  const head = (grid[0] ?? []).map((h) => String(h ?? '').trim());
  const rows = grid.slice(1);
  const [det, setDet] = useState<Detected[]>(() => detect(head, rows));
  const [opts, setOpts] = useState<BuildOpts>(() => defaultOpts(det));
  const [built, setBuilt] = useState(false);
  const card = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<{
    left: number;
    top: number;
    flip: boolean;
  } | null>(null);
  // Next to its node on screen (left of it when there is no room on the right), following the canvas.
  useLayoutEffect(() => {
    let raf = 0;
    const place = () => {
      const node = document.querySelector<HTMLElement>(`[data-node="l:${l.id}"]`);
      const el = card.current;
      if (node && el) {
        const r = node.getBoundingClientRect();
        const w = el.offsetWidth;
        const h = el.offsetHeight;
        const flip = r.right + 18 + w > window.innerWidth - 8;
        const left = flip ? Math.max(8, r.left - 18 - w) : r.right + 18;
        const top = Math.min(Math.max(60, r.top), Math.max(60, window.innerHeight - h - 8));
        setAt((a) => (a && a.left === left && a.top === top && a.flip === flip ? a : { left, top, flip }));
      }
      raf = requestAnimationFrame(place);
    };
    place();
    return () => cancelAnimationFrame(raf);
  }, [l.id]);
  const name = tableDataset(l, template)?.ds?.name ?? t('the data');
  const cols = colsOf(det);
  const mark = det.find((d) => d.on && d.role === 'mark')?.src;
  const colAt = (src: string) => head.indexOf(src);
  const sortOpts = [
    { value: '', label: t('As received') },
    ...det
      .filter((d) => d.on && d.role !== 'skip' && d.role !== 'mark')
      .flatMap((d) => [
        { value: `${d.src}|desc`, label: `${d.src} ↓` },
        { value: `${d.src}|asc`, label: `${d.src} ↑` },
      ]),
  ];
  const setRole = (src: string, role: Role) => setDet((ds) => ds.map((d) => (d.src === src ? { ...d, role } : d)));
  const build = () => {
    update((t) => applyBuild(findLayer(t, l.id) as TL, det, opts));
    setBuilt(true);
  };

  return createPortal(
    <div ref={card} className={`tb-card ${at?.flip ? 'flip' : ''}`} style={{ left: at?.left ?? -9999, top: at?.top ?? 0 }} onPointerDown={(e) => e.stopPropagation()} onWheel={(e) => e.stopPropagation()}>
      {built ? (
        <>
          <h4>
            <span className="tb-ok">
              <Check size={12} /> {t('In sync')}
            </span>
            {t('Built from {name}', { name })}
            <button className="icon-btn" title={t('Close')} onClick={onClose}>
              <X size={13} />
            </button>
          </h4>
          <p className="tb-note">{t('Columns, labels, types and the sort are set. A new column in the data will be offered here, never added on its own.')}</p>
          <div className="tb-foot">
            <button className="btn small" onClick={() => setBuilt(false)}>
              {t('Back')}
            </button>
            <button
              className="btn primary small"
              onClick={() => {
                onClose();
                useTableEditor.getState().open(l.id);
              }}
            >
              {t('Open the editor')}
            </button>
          </div>
        </>
      ) : (
        <>
          <h4>
            <Sparkles size={13} /> {t('Build the table from {name}', { name })}
            <button className="icon-btn" title={t('Not now')} onClick={onClose}>
              <X size={13} />
            </button>
          </h4>
          {!det.length ? (
            <p className="tb-note">{t('No columns in the data yet.')}</p>
          ) : (
            <div className="tb-detect">
              {det.map((d) => (
                <label key={d.src} className={d.on ? '' : 'off'}>
                  <input type="checkbox" checked={d.on} onChange={(e) => setDet((ds) => ds.map((x) => (x.src === d.src ? { ...x, on: e.target.checked } : x)))} />
                  <span className="tb-kind">{t(KIND[d.kind])}</span>
                  <b>{d.src}</b>
                  <Dropdown value={d.role} options={ROLES.map((r) => ({ ...r, label: t(r.label) }))} onChange={(r) => setRole(d.src, r)} />
                </label>
              ))}
            </div>
          )}
          <button className={`te-tog ${opts.rank ? 'on' : ''}`} onClick={() => setOpts({ ...opts, rank: !opts.rank })}>
            <span>{t('Rank column')}</span>
            <i />
          </button>
          <div className="tb-row2">
            <label>
              <span>{t('Sort by')}</span>
              <Dropdown
                value={opts.sort ? `${opts.sort}|${opts.dir}` : ''}
                options={sortOpts}
                onChange={(v) =>
                  setOpts({
                    ...opts,
                    sort: v.split('|')[0],
                    dir: (v.split('|')[1] as 'asc' | 'desc') || 'desc',
                  })
                }
              />
            </label>
            <label>
              <span>{t('Rows per page')}</span>
              <span className="pp-field">
                <NumberField value={opts.perPage} min={1} max={99} onChange={(v) => setOpts({ ...opts, perPage: v })} />
              </span>
            </label>
          </div>
          {cols.length > 0 && (
            <div className="tb-preview">
              <table>
                <thead>
                  <tr>
                    {opts.rank && <th>#</th>}
                    {cols.map((c) => (
                      <th key={c.src} style={{ textAlign: c.align }}>
                        {c.label}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview(rows, head, opts).map((r, i) => (
                    <tr key={i}>
                      {opts.rank && <td>{i + 1}</td>}
                      {cols.map((c) => (
                        <td key={c.src} style={{ textAlign: c.align }}>
                          {c.mark && mark && <i className="tb-mark" style={{ background: r[colAt(mark)] }} />}
                          {c.type === 'image' ? '▣' : r[colAt(c.src ?? '')]}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <div className="tb-foot">
            <button className="btn small" onClick={onClose}>
              {t('Not now')}
            </button>
            <button className="btn primary small" disabled={!cols.length} onClick={build}>
              {t('Build the table')}
            </button>
          </div>
        </>
      )}
    </div>,
    document.body,
  );
}

/** The first 3 rows, sorted like the table will be. */
function preview(rows: string[][], head: string[], o: BuildOpts): string[][] {
  const i = head.indexOf(o.sort);
  if (i < 0) return rows.slice(0, 3);
  const n = (s: string) =>
    Number(
      String(s ?? '')
        .replace('−', '-')
        .replace(',', '.'),
    );
  return [...rows]
    .sort((a, b) => {
      const x = n(a[i]);
      const y = n(b[i]);
      const d = Number.isFinite(x) && Number.isFinite(y) ? x - y : String(a[i]).localeCompare(String(b[i]));
      return o.dir === 'asc' ? d : -d;
    })
    .slice(0, 3);
}

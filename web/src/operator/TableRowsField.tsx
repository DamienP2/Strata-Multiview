import { Plus, X } from 'lucide-react';
import { useRef, useState } from 'react';
import { rowsOf } from '../widgets/table-rows';
import { parseCsv } from '../widgets/kit';
import { t, tn } from '../i18n';
import { Pager, usePaged } from './Paged';

const csvCell = (s: string) => (/[;"\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
const toCsv = (rows: string[][]) => rows.map((r) => r.map(csvCell).join(';')).join('\n');
const rowText = (r: string[]) => r.join(' ');

/**
 * The rows of a Table typed by the operator (a "table" field, JSON rows: the
 * header, then the rows): 8 rows at a time with a search, a CSV file to
 * import and an example CSV.
 */
export function TableRowsField({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const file = useRef<HTMLInputElement>(null);
  const all = rowsOf(value);
  const head = all[0] ?? ['Name', 'Value'];
  const rows = all.slice(1);
  const put = (h: string[], r: string[][]) => onChange(JSON.stringify([h, ...r]));
  // A row just added: its page.
  const [added, setAdded] = useState<number | null>(null);
  const paged = usePaged(rows, rowText, added, head, (r, i) => r[i] ?? '');
  const width = Math.max(head.length, ...rows.map((r) => r.length));
  const cols = Array.from({ length: width }, (_, j) => j);
  const example = () => {
    // The table's own columns, with two sample rows to fill.
    const sample = [head, ...[1, 2].map((n) => head.map((h, j) => (j === 0 ? `${h} ${n}` : String(n * 10))))];
    const blob = new Blob([`﻿${toCsv(sample)}\n`], { type: 'text/csv;charset=utf-8' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'table-example.csv';
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  };
  return (
    <div className="op-rows opt">
      <Pager paged={paged} />
      <div className="opt-scroll">
        <table>
          <thead>
            <tr>
              <th className="opt-n">#</th>
              {cols.map((j) => (
                <th key={j}>
                  <input value={head[j] ?? ''} title={t('Column name')} onKeyDown={(e) => e.stopPropagation()} onChange={(e) => put(cols.map((k) => (k === j ? e.target.value : (head[k] ?? ''))), rows)} />
                </th>
              ))}
              <th />
            </tr>
          </thead>
          <tbody>
            {paged.shown.map(({ item: r, index: i }) => (
              <tr key={i}>
                <td className="opt-n">{i + 1}</td>
                {cols.map((j) => (
                  <td key={j}>
                    <input value={r[j] ?? ''} onKeyDown={(e) => e.stopPropagation()} onChange={(e) => put(head, rows.map((x, a) => (a === i ? cols.map((k) => (k === j ? e.target.value : (x[k] ?? ''))) : x)))} />
                  </td>
                ))}
                <td className="opt-x">
                  <button title={t('Remove this row')} onClick={() => put(head, rows.filter((_, a) => a !== i))}>
                    <X size={12} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="wk-btns">
        <button
          className="opx-btn small"
          onClick={() => {
            put(head, [...rows, cols.map(() => '')]);
            paged.setQuery('');
            setAdded(rows.length);
          }}
        >
          <Plus size={12} /> {t('Row')}
        </button>
        <button className="opx-btn small" onClick={() => put([...cols.map((j) => head[j] ?? ''), t('Column {n}', { n: width + 1 })], rows.map((r) => [...cols.map((j) => r[j] ?? ''), '']))}>
          <Plus size={12} /> {t('Column')}
        </button>
        <button className="opx-btn small" disabled={width <= 1} title={t('Remove the last column')} onClick={() => put(head.slice(0, width - 1), rows.map((r) => r.slice(0, width - 1)))}>
          − {t('Column')}
        </button>
        <button className="opx-btn small" onClick={() => file.current?.click()} title={t('A CSV file: the first line holds the column names')}>
          {t('Import CSV file…')}
        </button>
        <button className="opx-btn small" onClick={example} title={t("A CSV with the table's columns, to fill in a spreadsheet")}>
          {t('Example CSV')}
        </button>
        <span className="op-rows-n">{tn(rows.length, '{n} row', '{n} rows')}</span>
      </div>
      <input
        ref={file}
        type="file"
        accept=".csv,text/csv,text/plain"
        hidden
        onChange={async (e) => {
          const f = e.target.files?.[0];
          e.target.value = '';
          if (!f) return;
          const r = parseCsv((await f.text()).replace(/^﻿/, ''));
          if (r.length) put(r[0], r.slice(1));
        }}
      />
    </div>
  );
}

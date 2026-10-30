// The rows of a Table without a data set, typed by the operator: the table's grid exposed to the Operator
// as a "table" field (JSON rows), with a grid to type in, a CSV file to import and an example CSV.
import { useRef } from 'react';
import { fieldUsed, newFieldKey } from '../engine/settings';
import type { Layer, Template } from '../engine/types';
import { t, tn } from '../i18n';
import { GridEditor, parseCsv } from './kit';

type TL = Layer & { grid?: string[][] };

/** Exposes the grid of a table to the Operator (a field the operator fills). */
export function exposeRows(tpl: Template, l: Layer) {
  if (l.settingBindings?.grid) return;
  const key = newFieldKey(tpl, `${l.name} rows`);
  tpl.fields.push({ key, label: t('{name} · Rows', { name: l.name }), kind: 'table', default: JSON.stringify((l as TL).grid ?? []) });
  l.settingBindings = { ...l.settingBindings, grid: key };
}

/** The grid back to the Designer only. */
export function unexposeRows(t: Template, l: Layer) {
  const key = l.settingBindings?.grid;
  if (!key) return;
  delete l.settingBindings!.grid;
  if (!Object.keys(l.settingBindings!).length) delete l.settingBindings;
  if (!fieldUsed(t, key)) t.fields = t.fields.filter((f) => f.key !== key);
}

/** Rows from a field value: JSON rows, else CSV. */
export function rowsOf(value: string): string[][] {
  const v = value.trim();
  if (!v) return [];
  try {
    const a = JSON.parse(v);
    if (Array.isArray(a) && a.every((r) => Array.isArray(r))) return a.map((r: unknown[]) => r.map((c) => (c == null ? '' : String(c))));
  } catch {
    /* CSV */
  }
  return parseCsv(v);
}

const csvCell = (s: string) => (/[;"\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
const toCsv = (rows: string[][]) => rows.map((r) => r.map(csvCell).join(';')).join('\n');

/** The Operator's editor of a table's rows: type them, import a CSV file, or start from an example. */
export function TableRowsInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const file = useRef<HTMLInputElement>(null);
  const rows = rowsOf(value);
  const head = rows[0] ?? ['Name', 'Value'];
  const put = (r: string[][]) => onChange(JSON.stringify(r));
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
    <div className="op-rows">
      <GridEditor value={rows.length ? rows : [head, head.map(() => '')]} onChange={put} header />
      <div className="wk-btns">
        <button className="opx-btn small" onClick={() => file.current?.click()} title={t('A CSV file: the first line holds the column names')}>
          {t('Import CSV file…')}
        </button>
        <button className="opx-btn small" onClick={example} title={t("A CSV with the table's columns, to fill in a spreadsheet")}>
          {t('Example CSV')}
        </button>
        <span className="op-rows-n">{tn(Math.max(0, rows.length - 1), '{n} row', '{n} rows')}</span>
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
          const text = (await f.text()).replace(/^﻿/, '');
          const r = parseCsv(text);
          if (r.length) put(r);
        }}
      />
    </div>
  );
}

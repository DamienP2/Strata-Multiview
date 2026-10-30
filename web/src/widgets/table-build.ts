// Builds a Table's columns from the rows it is wired to (a data set's All rows): each column of the data
// with its guessed type and role, then the columns, labels, alignment, formats, sort and rank in one go.
import type { Col, Config } from './table';
import { num } from './table';

export type Role = 'label' | 'text' | 'number' | 'signed' | 'mark' | 'picture' | 'skip';
export const ROLES: { value: Role; label: string }[] = [
  { value: 'label', label: 'Label' },
  { value: 'text', label: 'Text' },
  { value: 'number', label: 'Number' },
  { value: 'signed', label: 'Number (+/−)' },
  { value: 'mark', label: 'Colour mark' },
  { value: 'picture', label: 'Picture' },
  { value: 'skip', label: 'Skip' },
];
export type Kind = 'text' | 'num' | 'color' | 'pic';
export interface Detected {
  src: string;
  kind: Kind;
  role: Role;
  on: boolean;
}

const COLOR = /^(#[0-9a-f]{3,8}|rgba?\(.+\))$/i;
const PIC = /^(https?:\/\/|\/).+\.(png|jpe?g|gif|webp|svg|avif)(\?.*)?$/i;

/** The type of a column from its values (empty cells ignored). */
export function kindOf(values: string[]): Kind {
  const v = values.map((x) => String(x ?? '').trim()).filter(Boolean);
  if (!v.length) return 'text';
  if (v.every((x) => COLOR.test(x))) return 'color';
  if (v.every((x) => PIC.test(x) || /^\/assets\//.test(x))) return 'pic';
  if (v.every((x) => num(x) !== undefined)) return 'num';
  return 'text';
}

/** Each column of the data with its type and a guessed role. */
export function detect(head: string[], rows: string[][]): Detected[] {
  let label = false;
  return head
    .map((src, i) => ({ src, i }))
    .filter((c) => c.src.trim())
    .map(({ src, i }) => {
      const values = rows.map((r) => String(r[i] ?? ''));
      const kind = kindOf(values);
      let role: Role = 'text';
      if (kind === 'color') role = 'mark';
      else if (kind === 'pic') role = 'picture';
      else if (kind === 'num') role = values.some((x) => /^\s*[+−-]/.test(x)) ? 'signed' : 'number';
      else if (!label) ((role = 'label'), (label = true));
      return { src, kind, role, on: true };
    });
}

const title = (s: string) => s.replace(/[_-]+/g, ' ').replace(/^./, (x) => x.toUpperCase());

/** The columns made from the detected ones (a colour mark goes before the first label / text). */
export function colsOf(det: Detected[]): Col[] {
  const used = det.filter((d) => d.on && d.role !== 'skip');
  const mark = used.find((d) => d.role === 'mark')?.src ?? '';
  let marked = false;
  return used
    .filter((d) => d.role !== 'mark')
    .map((d): Col => {
      const isNum = d.role === 'number' || d.role === 'signed';
      const col: Col = {
        src: d.src,
        label: title(d.src),
        type: d.role === 'picture' ? 'image' : isNum ? 'number' : 'text',
        align: isNum ? 'right' : d.role === 'picture' ? 'center' : 'left',
        w: 100,
        pct: 10,
        fmt: d.role === 'signed' ? '+0' : '0',
        on: true,
      };
      if (mark && !marked && (d.role === 'label' || d.role === 'text')) ((col.mark = mark), (marked = true));
      return col;
    });
}

export interface BuildOpts {
  rank: boolean;
  /** Column to sort on ('' = as received) and its direction. */
  sort: string;
  dir: 'desc' | 'asc';
  perPage: number;
}

/** The first number column, sorted from the biggest (a standings table). */
export function defaultOpts(det: Detected[]): BuildOpts {
  const n = det.find((d) => d.on && (d.role === 'number' || d.role === 'signed'));
  return { rank: !!n, sort: n?.src ?? '', dir: 'desc', perPage: 7 };
}

/** Applies a build to the layer. */
export function applyBuild(l: Config, det: Detected[], o: BuildOpts) {
  l.cols = colsOf(det);
  l.showRank = o.rank;
  l.sortCol = o.sort;
  l.sortDir = o.dir;
  l.perPage = Math.max(1, Math.round(o.perPage) || 7);
  l.widthMode = 'auto';
  l.headerRow = true;
}

/** A Col for a column found later in the data. */
export function colFor(src: string, values: string[]): Col {
  const k = kindOf(values);
  return (
    colsOf([
      {
        src,
        kind: k,
        role: k === 'num' ? 'number' : k === 'pic' ? 'picture' : 'text',
        on: true,
      },
    ])[0] ?? { src, label: title(src), on: true }
  );
}

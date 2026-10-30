import { ChevronLeft, ChevronRight, Search, X } from 'lucide-react';
import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { t } from '../i18n';

/*
 * Every list of data rows in the Operator (data set rows, row pickers, a
 * table's rows): 5, 10 or 15 entries at a time (the user's choice, kept in
 * this browser), with a search field and pages.
 */

export const PAGE_SIZES = [5, 10, 15] as const;
const SIZE_KEY = 'strata.pageSize';
const readSize = (): number => {
  try {
    const v = Number(localStorage.getItem(SIZE_KEY));
    return (PAGE_SIZES as readonly number[]).includes(v) ? v : 10;
  } catch {
    return 10;
  }
};
let pageSize = readSize();
const listeners = new Set<() => void>();
const setPageSize = (n: number) => {
  pageSize = n;
  try {
    localStorage.setItem(SIZE_KEY, String(n));
  } catch {
    /* private mode */
  }
  listeners.forEach((f) => f());
};
/** Rows per page, the same for every list. */
export const usePageSize = () =>
  useSyncExternalStore(
    (f) => (listeners.add(f), () => listeners.delete(f)),
    () => pageSize,
  );

export interface Paged<T> {
  /** Entries per page. */
  size: number;
  /** The columns the search can be narrowed to (several), and the one chosen (-1: all). */
  columns: string[];
  column: number;
  setColumn: (c: number) => void;
  query: string;
  setQuery: (q: string) => void;
  page: number;
  setPage: (p: number) => void;
  pages: number;
  /** Entries matching the search. */
  matching: number;
  /** All the entries. */
  total: number;
  /** The entries of this page, with their index in the whole list. */
  shown: { item: T; index: number }[];
}

/**
 * Pages of `items` filtered by `query` (any of `text(item)`, any case).
 * `focus`: the index of an entry to keep in view (the NEXT row…): its page
 * is shown when it changes, unless a search hides it.
 */
export function usePaged<T>(items: T[], text: (item: T) => string, focus?: number | null, columns: string[] = [], cell?: (item: T, column: number) => string): Paged<T> {
  const size = usePageSize();
  const [query, setQuery] = useState('');
  const [column, setColumn] = useState(-1);
  const col = cell && column >= 0 && column < columns.length ? column : -1;
  const [page, setPage] = useState(0);
  const q = query.trim().toLowerCase();
  const matches = useMemo(() => {
    const all = items.map((item, index) => ({ item, index }));
    const of = (item: T) => (col >= 0 ? cell!(item, col) : text(item)) ?? '';
    return q ? all.filter((x) => of(x.item).toLowerCase().includes(q)) : all;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, q, col]);
  const pages = Math.max(1, Math.ceil(matches.length / size));
  const at = Math.min(page, pages - 1);
  // The focused entry's page when it changes.
  useEffect(() => {
    if (focus == null || focus < 0) return;
    const i = matches.findIndex((x) => x.index === focus);
    if (i >= 0) setPage(Math.floor(i / size));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus, items.length, size]);
  return {
    size,
    columns: cell ? columns : [],
    column: col,
    setColumn: (c) => (setColumn(c), setPage(0)),
    query,
    setQuery: (v) => (setQuery(v), setPage(0)),
    page: at,
    setPage: (p) => setPage(Math.max(0, Math.min(pages - 1, p))),
    pages,
    matching: matches.length,
    total: items.length,
    shown: matches.slice(at * size, at * size + size),
  };
}

/** The search field and the pages of a list: "1–8 of 42", previous / next. */
export function Pager<T>({ paged, placeholder }: { paged: Paged<T>; placeholder?: string }) {
  const { size, columns, column, setColumn, query, setQuery, page, setPage, pages, matching, total } = paged;
  // A short list fits at any size: no search, no pages.
  if (total <= PAGE_SIZES[0] && !query) return null;
  const a = matching ? page * size + 1 : 0;
  const b = Math.min(matching, (page + 1) * size);
  return (
    <div className="opg">
      {columns.length > 1 && (
        <select className="opg-col" value={column} title={t('Search in')} onChange={(e) => setColumn(+e.target.value)}>
          <option value={-1}>{t('All columns')}</option>
          {columns.map((c, i) => (
            <option key={i} value={i}>
              {c}
            </option>
          ))}
        </select>
      )}
      <label className="opg-search">
        <Search size={12} />
        <input
          value={query}
          placeholder={placeholder ?? t('Search')}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === 'Escape') setQuery('');
          }}
        />
        {query && (
          <button title={t('Clear the search')} onClick={() => setQuery('')}>
            <X size={11} />
          </button>
        )}
      </label>
      <span className="opg-count">
        {matching ? t('{a}–{b} of {n}', { a, b, n: matching }) : t('No match')}
        {query.trim() && matching !== total && <span className="muted"> {t('({n} in all)', { n: total })}</span>}
      </span>
      <div className="opx-seg tiny" title={t('Rows per page')}>
        {PAGE_SIZES.map((n) => (
          <button key={n} className={n === size ? 'on' : ''} onClick={() => setPageSize(n)}>
            {n}
          </button>
        ))}
      </div>
      <button className="opx-sq" title={t('Previous page')} disabled={page <= 0} onClick={() => setPage(page - 1)}>
        <ChevronLeft size={13} />
      </button>
      <button className="opx-sq" title={t('Next page')} disabled={page >= pages - 1} onClick={() => setPage(page + 1)}>
        <ChevronRight size={13} />
      </button>
    </div>
  );
}

import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { t } from '../i18n';

const SIMPLE = /^[A-Za-z_@#$][\w@#$-]*$/;

/** Path of a child: `a.b`, `a[0]`, `a[odd key]`. */
export function childPath(parent: string, key: string | number): string {
  if (typeof key === 'number') return `${parent}[${key}]`;
  if (SIMPLE.test(key)) return parent ? `${parent}.${key}` : key;
  return `${parent}[${key}]`;
}

/**
 * A JSON value as a tree. Clicking a value (or a list, to take all its
 * items) gives its binding path.
 */
export function JsonTree({ value, onPick, picked }: { value: unknown; onPick?: (path: string) => void; picked?: string }) {
  return (
    <div className="json-tree">
      <Node name={null} value={value} path="" depth={0} onPick={onPick} picked={picked} />
    </div>
  );
}

function Node({ name, value, path, depth, onPick, picked }: { name: string | null; value: unknown; path: string; depth: number; onPick?: (p: string) => void; picked?: string }) {
  const [open, setOpen] = useState(depth < 3);
  const [more, setMore] = useState(false);
  const isArr = Array.isArray(value);
  const isObj = value !== null && typeof value === 'object';
  const label = name !== null && <span className="jt-key">{name}</span>;
  if (!isObj) {
    const text = value === null ? 'null' : typeof value === 'string' ? `“${value}”` : String(value);
    return (
      <div className={`jt-row jt-leaf ${picked === path ? 'picked' : ''}`} onClick={() => onPick?.(path)} title={onPick ? t('Use {path}', { path: path || t('(root)') }) : path}>
        {label}
        <span className={`jt-val t-${value === null ? 'null' : typeof value}`}>{text.length > 120 ? `${text.slice(0, 120)}…` : text}</span>
      </div>
    );
  }
  const entries: [string | number, unknown][] = isArr ? (value as unknown[]).map((v, i) => [i, v]) : Object.entries(value as Record<string, unknown>);
  const shown = more ? entries : entries.slice(0, 30);
  return (
    <div className="jt-node">
      <div className={`jt-row ${picked === path ? 'picked' : ''}`}>
        <span className="jt-twirl" onClick={() => setOpen(!open)}>
          {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
        </span>
        {label}
        <span className="jt-meta" onClick={() => setOpen(!open)}>
          {isArr ? `[${entries.length}]` : `{${entries.length}}`}
        </span>
        {onPick && isArr && entries.length > 0 && (
          <button className="jt-all" title={t('Every item, one per line (tickers, lists)')} onClick={() => onPick(`${path}[*]`)}>
            {t('all items')}
          </button>
        )}
      </div>
      {open && (
        <div className="jt-children">
          {shown.map(([k, v]) => (
            <Node key={String(k)} name={String(k)} value={v} path={childPath(path, k)} depth={depth + 1} onPick={onPick} picked={picked} />
          ))}
          {!more && entries.length > shown.length && (
            <button className="jt-more" onClick={() => setMore(true)}>
              {t('{n} more…', { n: entries.length - shown.length })}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

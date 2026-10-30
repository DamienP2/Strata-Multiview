import { Check, ChevronDown } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { t } from '../i18n';

/*
 * A designed drop-down list (instead of the browser's <select>): a button
 * with the current choice, and a list in a layer over the page, so it stays
 * readable at any zoom of the node canvases. Options can be grouped.
 */

export interface DropOption<V extends string> {
  value: V;
  label: string;
  /** Options with the same group are listed under its title. */
  group?: string;
  /** Muted text after the label (a unit, a hint). */
  note?: string;
  /** Style of the label (a font shown in itself). */
  style?: React.CSSProperties;
  /** Second line under the label (what the choice does, or why it cannot be taken). */
  sub?: string;
  /** A mark before the label (a colour swatch, a dot), in the list and on the button. */
  icon?: React.ReactNode;
  /** Muted text after the label on the button only (the connector it goes with…). */
  detail?: string;
  /** Shown, not taken (its `sub` says why). */
  disabled?: boolean;
}

export function Dropdown<V extends string>({ value, options, onChange, title, placeholder = '—', className = '' }: { value: V; options: readonly DropOption<V>[]; onChange: (v: V) => void; title?: string; placeholder?: string; className?: string }) {
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState<{ left: number; top: number; width: number; up: boolean } | null>(null);
  const [hover, setHover] = useState(-1);
  const btn = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const current = options.find((o) => o.value === value);
  // Options with a mark or a second line: the check goes to the end of the row.
  const rich = options.some((o) => o.icon || o.sub);
  /** The next option that can be taken from `i` going `step`, or `i`. */
  const next = (i: number, step: number) => {
    for (let j = i + step; j >= 0 && j < options.length; j += step) if (!options[j].disabled) return j;
    return i;
  };

  useLayoutEffect(() => {
    if (!open || !btn.current) return;
    const r = btn.current.getBoundingClientRect();
    const below = window.innerHeight - r.bottom;
    setAt({ left: r.left, top: below < 240 && r.top > below ? r.top : r.bottom, width: r.width, up: below < 240 && r.top > below });
    setHover(Math.max(0, options.findIndex((o) => o.value === value)));
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!open) return;
    const close = (e: PointerEvent) => {
      if (!list.current?.contains(e.target as Node) && !btn.current?.contains(e.target as Node)) {
        setOpen(false);
        // A click elsewhere leaves the list for good (no focused look left behind).
        btn.current?.blur();
      }
    };
    // Scrolling the list keeps it open; scrolling anything else closes it.
    const away = (e: Event) => {
      if (!list.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('pointerdown', close, true);
    window.addEventListener('wheel', away, { passive: true });
    window.addEventListener('resize', away);
    return () => {
      window.removeEventListener('pointerdown', close, true);
      window.removeEventListener('wheel', away);
      window.removeEventListener('resize', away);
    };
  }, [open]);

  useEffect(() => {
    if (open) list.current?.querySelector('.dd-opt.hover')?.scrollIntoView({ block: 'nearest' });
  }, [open, hover]);

  // A list wider than the room right of its button opens leftwards, inside the window.
  useLayoutEffect(() => {
    const el = list.current;
    if (!open || !at || !el) return;
    const over = el.getBoundingClientRect().right - (window.innerWidth - 8);
    if (over > 0) el.style.left = `${Math.max(8, at.left - over)}px`;
  }, [open, at]);

  const pick = (v: V, keyboard = false) => {
    setOpen(false);
    if (v !== value) onChange(v);
    if (keyboard) btn.current?.focus();
    else btn.current?.blur();
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (!open && (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ')) {
      e.preventDefault();
      setOpen(true);
      return;
    }
    if (!open) return;
    e.stopPropagation();
    if (e.key === 'Escape') {
      setOpen(false);
      btn.current?.focus();
    }
    else if (e.key === 'ArrowDown') setHover((h) => next(h, 1));
    else if (e.key === 'ArrowUp') setHover((h) => next(h, -1));
    else if (e.key === 'Enter' && options[hover] && !options[hover].disabled) pick(options[hover].value, true);
    else return;
    e.preventDefault();
  };

  let lastGroup: string | undefined;
  return (
    <>
      <button
        ref={btn}
        type="button"
        className={`dd-btn ${open ? 'open' : ''} ${className}`}
        title={title}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => setOpen((o) => !o)}
        onKeyDown={onKey}
      >
        {current?.icon && <span className="dd-icon">{current.icon}</span>}
        <span className="dd-label" style={current?.style}>
          {current?.label ?? (value || placeholder)}
          {current?.detail && <span className="dd-detail">{current.detail}</span>}
        </span>
        <ChevronDown size={12} />
      </button>
      {open &&
        at &&
        createPortal(
          <div
            ref={list}
            className={`dd-list ${at.up ? 'up' : ''} ${rich ? 'rich' : ''}`}
            style={{ left: at.left, top: at.up ? undefined : at.top + 4, bottom: at.up ? window.innerHeight - at.top + 4 : undefined, minWidth: Math.max(rich ? 260 : 160, at.width) }}
            role="listbox"
            onPointerDown={(e) => e.stopPropagation()}
          >
            {options.map((o, i) => {
              const head = o.group && o.group !== lastGroup ? o.group : null;
              lastGroup = o.group;
              return (
                <div key={o.value}>
                  {head && <div className="dd-group">{head}</div>}
                  <div
                    role="option"
                    aria-selected={o.value === value}
                    aria-disabled={o.disabled || undefined}
                    className={`dd-opt ${o.value === value ? 'on' : ''} ${i === hover && !o.disabled ? 'hover' : ''} ${o.disabled ? 'off' : ''}`}
                    onPointerEnter={() => !o.disabled && setHover(i)}
                    onClick={() => !o.disabled && pick(o.value)}
                  >
                    {!rich && <span className="dd-check">{o.value === value && <Check size={12} />}</span>}
                    {rich && <span className="dd-icon">{o.icon}</span>}
                    {rich ? (
                      <span className="dd-text">
                        <span className="dd-main" style={o.style}>{o.label}</span>
                        {o.sub && <span className="dd-sub">{o.sub}</span>}
                      </span>
                    ) : (
                      <span className="dd-text" style={o.style}>{o.label}</span>
                    )}
                    {o.note && <span className="dd-note">{o.note}</span>}
                    {rich && <span className="dd-check">{o.value === value && <Check size={13} />}</span>}
                  </div>
                </div>
              );
            })}
            {!options.length && <div className="dd-empty">{t('Nothing to choose')}</div>}
          </div>,
          document.body,
        )}
    </>
  );
}

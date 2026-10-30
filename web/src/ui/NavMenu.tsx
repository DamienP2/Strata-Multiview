import { ChevronDown } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { navigate } from '../router';

export interface NavEntry {
  id: string;
  path: string;
  icon: React.ReactNode;
  label: string;
}

/**
 * An entry of the main menu that opens a list of pages (Broadcast ▾,
 * System ▾). It shows as open (`on`) when one of its pages is. Keyboard:
 * ↓ / Enter opens, ↑ ↓ Home End move, Esc closes.
 */
export function NavGroup({ icon, label, entries, current, dot }: { icon: React.ReactNode; label: string; entries: NavEntry[]; current: string; /** Something to look at in its pages (System: a crash, errors). */ dot?: boolean }) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const focusAt = useRef(0);

  useEffect(() => {
    if (!open) return;
    list.current?.querySelectorAll<HTMLElement>('[role=menuitem]')[focusAt.current]?.focus();
    const down = (e: PointerEvent) => !wrap.current?.contains(e.target as Node) && setOpen(false);
    window.addEventListener('pointerdown', down);
    return () => window.removeEventListener('pointerdown', down);
  }, [open]);

  if (!entries.length) return null;
  const on = entries.some((e) => e.id === current);
  const show = (at: number) => {
    focusAt.current = at < 0 ? entries.length - 1 : at;
    setOpen(true);
  };
  const onKey = (e: React.KeyboardEvent) => {
    const items = [...(list.current?.querySelectorAll<HTMLElement>('[role=menuitem]') ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'Escape') (e.preventDefault(), setOpen(false), btn.current?.focus());
    else if (e.key === 'ArrowDown') (e.preventDefault(), items[(at + 1) % items.length]?.focus());
    else if (e.key === 'ArrowUp') (e.preventDefault(), items[(at - 1 + items.length) % items.length]?.focus());
    else if (e.key === 'Home') (e.preventDefault(), items[0]?.focus());
    else if (e.key === 'End') (e.preventDefault(), items[items.length - 1]?.focus());
    else if (e.key === 'Tab') setOpen(false);
  };

  return (
    <div className="ux1-navgroup" ref={wrap}>
      <button
        ref={btn}
        className={`${on ? 'on' : ''} ${open ? 'open' : ''}`}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => (open ? setOpen(false) : show(Math.max(0, entries.findIndex((e) => e.id === current))))}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown') (e.preventDefault(), show(0));
          if (e.key === 'ArrowUp') (e.preventDefault(), show(-1));
        }}
      >
        {icon} <span className="tab-label">{label}</span>
        {dot && <i className="nav-dot" />}
        <ChevronDown size={13} className="ux1-chev" />
      </button>
      {open && (
        <div ref={list} className="menu ux1-navmenu" role="menu" aria-label={label} onKeyDown={onKey}>
          {entries.map((e) => (
            <button
              key={e.id}
              role="menuitem"
              className={`ux1-mi ${e.id === current ? 'on' : ''}`}
              aria-current={e.id === current ? 'page' : undefined}
              onClick={() => {
                setOpen(false);
                navigate(e.path);
              }}
            >
              {e.icon} {e.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

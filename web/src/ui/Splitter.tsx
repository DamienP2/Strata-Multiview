import { useEffect, useState } from 'react';
import { t } from '../i18n';
import { displayPref, setDisplayPref } from '../displayPrefs';

/** Panel sizes of a page, remembered in this browser (`user`: for the user, on every browser). */
export function useLayout<T extends Record<string, number>>(key: string, defaults: T, user = false): [T, (k: keyof T, v: number) => void] {
  const [sizes, setSizes] = useState<T>(() => {
    if (user) return { ...defaults, ...displayPref<Partial<T>>(key, {}) };
    try {
      return { ...defaults, ...JSON.parse(localStorage.getItem(key) ?? '{}') };
    } catch {
      return defaults;
    }
  });
  useEffect(() => {
    if (user) return setDisplayPref(key, sizes);
    try {
      localStorage.setItem(key, JSON.stringify(sizes));
    } catch {
      /* private mode */
    }
  }, [key, sizes, user]);
  return [sizes, (k, v) => setSizes((s) => ({ ...s, [k]: v }))];
}

/**
 * Drag handle between two areas. `axis` x resizes a width, y a height;
 * `sign` -1 when dragging towards the start makes the area bigger (right
 * panel, bottom timeline). Double-click restores the default.
 */
export function Splitter({
  axis,
  value,
  onChange,
  min,
  max,
  sign = 1,
  reset,
  className = '',
}: {
  axis: 'x' | 'y';
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  sign?: 1 | -1;
  reset?: number;
  className?: string;
}) {
  const start = (e: React.PointerEvent) => {
    e.preventDefault();
    const from = axis === 'x' ? e.clientX : e.clientY;
    const base = value;
    document.body.classList.add(axis === 'x' ? 'resizing-x' : 'resizing-y');
    const move = (ev: PointerEvent) => {
      const d = ((axis === 'x' ? ev.clientX : ev.clientY) - from) * sign;
      onChange(Math.round(Math.min(max, Math.max(min, base + d))));
    };
    const up = () => {
      document.body.classList.remove('resizing-x', 'resizing-y');
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };
  return <div className={`splitter ${axis} ${className}`} onPointerDown={start} onMouseDown={(e) => e.preventDefault()} onDoubleClick={() => reset !== undefined && onChange(reset)} title={t('Drag to resize — double-click to reset')} />;
}

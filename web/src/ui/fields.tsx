import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { parseHex, toHex } from '../engine/evaluate';
import type { Color } from '../engine/types';
import { t } from '../i18n';
import { namedOf, useColors } from '../colors';
import { useApp } from '../store';
import { ColorPicker, swatchCss } from './ColorPicker';

/*
 * Mouse wheel / trackpad on number fields, without clicking first: resting
 * the pointer on a field arms it (a short pause, so scrolling a panel past
 * the field keeps scrolling the panel). A wheel notch is one step; trackpad
 * movement adds up. Hold Shift for ×10, Alt for ×0.1.
 */

/** When the page last scrolled outside a number field, and when the pointer entered one. */
const wheelState = { outside: 0, entered: 0, field: null as Element | null };
let wheelWatch = false;
function watchWheel() {
  if (wheelWatch) return;
  wheelWatch = true;
  window.addEventListener(
    'wheel',
    (e) => {
      const f = (e.target as Element | null)?.closest?.('[data-wheel-field], input[type="number"]');
      if (!f) wheelState.outside = performance.now();
    },
    { capture: true, passive: true },
  );
  window.addEventListener(
    'pointerover',
    (e) => {
      const f = (e.target as Element | null)?.closest?.('[data-wheel-field], input[type="number"]') ?? null;
      if (f !== wheelState.field) {
        wheelState.field = f;
        wheelState.entered = performance.now();
      }
    },
    { capture: true, passive: true },
  );
}

/** Whether a wheel event over field `el` adjusts it (else the page scrolls). */
function wheelArmed(el: Element): boolean {
  if (document.activeElement === el) return true;
  const now = performance.now();
  // Still scrolling the panel, or just arrived on the field while scrolling: let the panel scroll.
  return now - wheelState.outside > 300 && now - wheelState.entered > 120;
}

/** Steps of a wheel event: ±1 per notch, trackpad pixels added up (Shift turns the wheel sideways on macOS). */
function wheelSteps(e: WheelEvent, acc: { v: number }): number {
  const raw = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
  const d = e.deltaMode === 1 ? raw * 40 : e.deltaMode === 2 ? raw * 400 : raw;
  // A mouse notch: big, round deltas.
  if (Math.abs(d) >= 50 && (e.deltaMode !== 0 || Math.abs(d) % 1 === 0)) {
    acc.v = 0;
    return -Math.sign(d);
  }
  acc.v += d;
  let steps = 0;
  while (Math.abs(acc.v) >= 16) {
    steps -= Math.sign(acc.v);
    acc.v -= Math.sign(acc.v) * 16;
  }
  return steps;
}

/** Wheel / trackpad steps on a number field (see above). */
export function useWheelSteps(ref: React.RefObject<HTMLInputElement | null>, onSteps: (steps: number, e: WheelEvent) => void) {
  const cb = useRef(onSteps);
  cb.current = onSteps;
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    watchWheel();
    el.dataset.wheelField = '';
    const acc = { v: 0 };
    const onWheel = (e: WheelEvent) => {
      if (!wheelArmed(el) || e.ctrlKey) return;
      e.preventDefault();
      const steps = wheelSteps(e, acc);
      if (steps) cb.current(steps, e);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [ref]);
}

/** The same for every plain <input type="number"> of the app (installed once). */
export function installNumberWheel() {
  watchWheel();
  const acc = { v: 0 };
  window.addEventListener(
    'wheel',
    (e) => {
      const el = e.target as HTMLInputElement | null;
      if (!el || el.tagName !== 'INPUT' || el.type !== 'number' || el.disabled || el.readOnly || e.ctrlKey) return;
      if (!wheelArmed(el)) return;
      e.preventDefault();
      const steps = wheelSteps(e, acc);
      if (!steps) return;
      const step = Number(el.step) || 1;
      const mult = e.shiftKey ? 10 : e.altKey ? 0.1 : 1;
      let v = (Number(el.value) || 0) + steps * step * mult;
      if (el.min !== '') v = Math.max(Number(el.min), v);
      if (el.max !== '') v = Math.min(Number(el.max), v);
      const digits = Math.max(0, (String(step * mult).split('.')[1] ?? '').length);
      // React listens to the native setter + an input event.
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(el, String(+v.toFixed(digits)));
      el.dispatchEvent(new Event('input', { bubbles: true }));
    },
    { passive: false },
  );
}

/** Number input with AE-style scrubbing on its label. */
export function NumberField({
  label,
  value,
  onChange,
  step = 1,
  display = 1,
  precision = 0,
  suffix,
  min,
  max,
  title,
}: {
  label?: React.ReactNode;
  value: number;
  onChange: (v: number) => void;
  step?: number;
  /** Display multiplier (e.g. 100 to show a 0..1 value as a percentage). */
  display?: number;
  precision?: number;
  suffix?: string;
  min?: number;
  max?: number;
  title?: string;
}) {
  const shown = +(value * display).toFixed(precision);
  const [text, setText] = useState(String(shown));
  const focused = useRef(false);
  useEffect(() => {
    if (!focused.current) setText(String(shown));
  }, [shown]);

  const clamp = (v: number) => Math.min(max ?? Infinity, Math.max(min ?? -Infinity, v));

  // Mouse wheel / trackpad on a focused field: ±step (Shift ×10, Alt ×0.1).
  const inputRef = useRef<HTMLInputElement>(null);
  const live = useRef({ value, onChange, clamp, step, display, precision });
  live.current = { value, onChange, clamp, step, display, precision };
  useWheelSteps(inputRef, (steps, e) => {
    const l = live.current;
    const mult = e.shiftKey ? 10 : e.altKey ? 0.1 : 1;
    const digits = Math.max(l.precision, mult < 1 ? 1 : 0);
    const next = l.clamp(l.value + (steps * l.step * mult) / l.display);
    const rounded = +(next * l.display).toFixed(digits) / l.display;
    l.value = rounded;
    setText(String(+(rounded * l.display).toFixed(digits)));
    l.onChange(rounded);
  });
  const commit = (s: string) => {
    // Untouched: nothing to write (no keyframe added, no rounding, no undo step).
    if (s.trim() === String(shown)) return;
    // Allow simple expressions: "100+20", "1920/2".
    const expr = s.replace(/[^-+*/().\d\s]/g, '');
    let v: number;
    try {
      v = Function(`"use strict";return (${expr || 0})`)() as number;
    } catch {
      v = NaN;
    }
    if (Number.isFinite(v)) {
      const c = clamp(v / display);
      onChange(c);
      // Clamped to the same value: show it again (the field would keep the typed number).
      setText(String(+(c * display).toFixed(precision)));
    } else setText(String(shown));
  };

  const scrub = (e: React.PointerEvent) => {
    const startX = e.clientX;
    const start = value;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const mult = ev.shiftKey ? 10 : ev.altKey ? 0.1 : 1;
      onChange(clamp(start + ((ev.clientX - startX) * step * mult) / display));
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  };

  return (
    <label className="num-field" title={title}>
      {label !== undefined && (
        <span className="scrub" onPointerDown={scrub}>
          {label}
        </span>
      )}
      <input
        ref={inputRef}
        value={text}
        title={t('Wheel or trackpad to adjust · Shift ×10 · Alt ×0.1')}
        onFocus={(e) => {
          focused.current = true;
          e.target.select();
        }}
        onBlur={(e) => {
          focused.current = false;
          commit(e.target.value);
        }}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
          if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault();
            const d = (e.key === 'ArrowUp' ? 1 : -1) * step * (e.shiftKey ? 10 : 1);
            const next = clamp(value + d / display);
            setText(String(+(next * display).toFixed(precision)));
            onChange(next);
          }
        }}
      />
      {suffix && <span className="suffix">{suffix}</span>}
    </label>
  );
}

export function ColorField({ value, onChange, compact, title }: { value: Color; onChange: (c: Color) => void; /** The swatch alone (tight places). */ compact?: boolean; title?: string }) {
  const [text, setText] = useState(toHex(value).slice(1).toUpperCase());
  useEffect(() => setText(toHex(value).slice(1).toUpperCase()), [value]);
  const [open, setOpen] = useState<DOMRect | null>(null);
  const [typing, setTyping] = useState(false);
  const project = useApp((s) => s.project);
  useColors((s) => s.palettes);
  const named = namedOf(value, project);
  const sw = (
    <button type="button" className="cf-sw" title={title ?? named?.name} onClick={(e) => setOpen(open ? null : e.currentTarget.closest('.color-field')!.getBoundingClientRect())}>
      <i style={{ background: swatchCss(value) }} />
    </button>
  );
  const picker = open && <ColorPicker value={value} anchor={open} onChange={onChange} onClose={() => setOpen(null)} />;
  if (compact)
    return (
      <span className="color-field compact">
        {sw}
        {picker}
      </span>
    );
  return (
    <div className={`color-field ${open ? 'open' : ''}`}>
      {sw}
      {named && !typing ? (
        <button type="button" className="cf-name" title={toHex(value, value[3] < 1).toUpperCase()} onClick={() => setTyping(true)}>
          <b />
          {named.name}
        </button>
      ) : (
        <input
          className="hex"
          value={text}
          spellCheck={false}
          autoFocus={typing}
          aria-label="HEX"
          onChange={(e) => setText(e.target.value.replace(/[^0-9a-f#]/gi, '').toUpperCase())}
          onBlur={() => {
            setTyping(false);
            const c = parseHex(text);
            if (c && text.replace('#', '').length >= 6) onChange(text.replace('#', '').length === 8 ? c : [c[0], c[1], c[2], value[3]]);
            else setText(toHex(value).slice(1).toUpperCase());
          }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
          }}
        />
      )}
      <NumberField value={value[3]} display={100} min={0} max={1} suffix="%" onChange={(a) => onChange([value[0], value[1], value[2], a])} />
      {picker}
    </div>
  );
}

/** A colour field on a `#rrggbb[aa]` value. */
export function HexColorField({ value, onChange, compact, title, fallback = '#ffffff' }: { value: string | undefined; onChange: (hex: string) => void; compact?: boolean; title?: string; fallback?: string }) {
  return <ColorField compact={compact} title={title} value={parseHex(value ?? '') ?? parseHex(fallback) ?? [1, 1, 1, 1]} onChange={(c) => onChange(toHex(c, c[3] < 1))} />;
}

export function TextInput({
  value,
  onChange,
  multiline,
  placeholder,
  commitOnBlur,
  maxLength,
  autoGrow,
}: {
  value: string;
  onChange: (v: string) => void;
  multiline?: boolean;
  placeholder?: string;
  commitOnBlur?: boolean;
  /** Most characters that can be typed. */
  maxLength?: number;
  /** As tall as its text (a line wraps, without a return). */
  autoGrow?: boolean;
}) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const area = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const el = area.current;
    if (!autoGrow || !el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight + 2}px`;
  }, [autoGrow, text]);
  const props = {
    value: text,
    placeholder,
    maxLength: maxLength || undefined,
    onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      setText(e.target.value);
      if (!commitOnBlur) onChange(e.target.value);
    },
    onBlur: () => commitOnBlur && text !== value && onChange(text),
    onKeyDown: (e: React.KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === 'Enter' && !multiline) {
        e.preventDefault();
        (e.target as HTMLInputElement).blur();
      }
    },
  };
  if (autoGrow) return <textarea ref={area} rows={1} className={`tx-grow ${multiline ? '' : 'line'}`} {...props} />;
  return multiline ? <textarea rows={3} {...props} /> : <input {...props} />;
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: React.ReactNode; title?: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented">
      {options.map((o) => (
        <button key={o.value} type="button" title={o.title} className={o.value === value ? 'on' : ''} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

import { Pipette, Plus } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { namedOf, sameColor, useColors } from '../colors';
import { newId, parseHex, toHex } from '../engine/evaluate';
import type { Color, NamedColor } from '../engine/types';
import { t } from '../i18n';
import { useApp } from '../store';
import { Dropdown } from './Dropdown';

/*
 * The colour picker of every colour field: saturation × value square, hue
 * and transparency bars, the value in HEX, RGB or HSL, and the palettes of
 * the Library (the project's, the shared ones, the recent colours).
 */

type Hsv = [number, number, number];

function hsvToRgb([h, s, v]: Hsv): [number, number, number] {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return [f(5), f(3), f(1)];
}
function rgbToHsv(r: number, g: number, b: number, hue = 0): Hsv {
  const mx = Math.max(r, g, b);
  const d = mx - Math.min(r, g, b);
  let h = hue;
  if (d) h = (((mx === r ? (g - b) / d : mx === g ? (b - r) / d + 2 : (r - g) / d + 4) * 60) % 360 + 360) % 360;
  return [h, mx ? d / mx : 0, mx];
}
function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const l = (mx + mn) / 2;
  const d = mx - mn;
  return [Math.round(rgbToHsv(r, g, b)[0]), Math.round((d ? d / (1 - Math.abs(2 * l - 1)) : 0) * 100), Math.round(l * 100)];
}
function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const v = l + c / 2;
  return hsvToRgb([h, v ? (2 * (v - l)) / v : 0, v]);
}

type Mode = 'hex' | 'rgb' | 'hsl';
const MODE = 'strata.colorMode';
const readMode = (): Mode => {
  try {
    const m = localStorage.getItem(MODE);
    return m === 'rgb' || m === 'hsl' ? m : 'hex';
  } catch {
    return 'hex';
  }
};

/** CSS of a colour over the checkerboard. */
export const swatchCss = (c: Color) => `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${c[3]})`;

/** Drags a pointer over `el`: (x, y) in 0..1. */
function useDrag(onMove: (x: number, y: number) => void) {
  return (e: React.PointerEvent<HTMLElement>) => {
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent | React.PointerEvent) => {
      const r = el.getBoundingClientRect();
      onMove(Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width)), Math.min(1, Math.max(0, (ev.clientY - r.top) / r.height)));
    };
    move(e);
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  };
}

/** One number of the RGB / HSL values, committed as it is typed. */
function Part({ value, max, onChange }: { value: number; max: number; onChange: (v: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  return (
    <input
      value={text}
      inputMode="numeric"
      onChange={(e) => {
        setText(e.target.value);
        const v = Number(e.target.value);
        if (e.target.value.trim() !== '' && v >= 0 && v <= max) onChange(v);
      }}
      onBlur={() => setText(String(value))}
      onKeyDown={(e) => e.stopPropagation()}
    />
  );
}

type Source = 'project' | 'recent' | string;

export function ColorPicker({ value, anchor, onChange, onClose }: { value: Color; anchor: DOMRect; onChange: (c: Color) => void; onClose: () => void }) {
  const [hsv, setHsv] = useState<Hsv>(() => rgbToHsv(value[0], value[1], value[2]));
  const [alpha, setAlpha] = useState(value[3]);
  const [mode, setMode] = useState<Mode>(readMode);
  const [hexText, setHexText] = useState('');
  const [at, setAt] = useState<{ left: number; top: number } | null>(null);
  const pop = useRef<HTMLDivElement>(null);
  const { palettes, loaded, load, save, saveProject, recent, pushRecent } = useColors();
  const project = useApp((s) => s.project);
  // The first palette with colours: the project's, a shared one, the recent ones.
  const [source, setSource] = useState<Source>(() => (project?.colors?.length ? 'project' : (useColors.getState().palettes.find((p) => p.colors.length)?.id ?? (project ? 'project' : 'recent'))));
  const changed = useRef(false);
  // The value set from here: an outside change (undo…) is shown, ours not re-read.
  const mine = useRef(value);

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);
  useEffect(() => {
    if (sameColor(mine.current, value) && Math.abs(mine.current[3] - value[3]) < 1 / 512) return;
    mine.current = value;
    setHsv((h) => rgbToHsv(value[0], value[1], value[2], h[0]));
    setAlpha(value[3]);
  }, [value]);

  const rgb = hsvToRgb(hsv);
  const color: Color = [rgb[0], rgb[1], rgb[2], alpha];
  const hex6 = toHex(color).slice(1).toUpperCase();
  useEffect(() => setHexText(hex6), [hex6]);

  const emit = (h: Hsv, a: number) => {
    setHsv(h);
    setAlpha(a);
    const [r, g, b] = hsvToRgb(h);
    const c: Color = [r, g, b, a];
    mine.current = c;
    changed.current = true;
    onChange(c);
  };
  const setRgb = (r: number, g: number, b: number, a = alpha) => emit(rgbToHsv(r, g, b, hsv[0]), a);

  // Placed under the field (over it when there is no room), inside the window.
  useLayoutEffect(() => {
    const el = pop.current;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    const below = anchor.bottom + 4 + h <= window.innerHeight - 8;
    setAt({ left: Math.max(8, Math.min(anchor.left, window.innerWidth - w - 8)), top: below ? anchor.bottom + 4 : Math.max(8, anchor.top - h - 4) });
  }, [anchor]);

  useEffect(() => {
    const away = (e: PointerEvent) => {
      if (!pop.current?.contains(e.target as Node) && !(e.target as Element).closest?.('.dd-list')) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('keydown', key, true);
    };
  }, [onClose]);
  // The colour chosen goes to the recent ones.
  useEffect(
    () => () => {
      if (changed.current) pushRecent(toHex(mine.current, mine.current[3] < 1));
    },
    [pushRecent],
  );

  const sv = useDrag((x, y) => emit([hsv[0], x, 1 - y], alpha));
  const hue = useDrag((x) => emit([Math.min(359.9, x * 360), hsv[1], hsv[2]], alpha));
  const alp = useDrag((x) => emit(hsv, Math.round(x * 100) / 100));

  const shared = palettes.map((p) => ({ value: p.id, label: p.name || t('Palette') }));
  const sources = [...(project ? [{ value: 'project', label: t('Project') }] : []), ...shared, { value: 'recent', label: t('Recent colours') }];
  const list: NamedColor[] =
    source === 'project'
      ? (project?.colors ?? [])
      : source === 'recent'
        ? recent.map((v, i) => ({ id: `r${i}`, name: '', value: v }))
        : (palettes.find((p) => p.id === source)?.colors ?? []);
  const named = namedOf(color, project);
  const hexNow = toHex(color, alpha < 1);
  const add = () => {
    const c: NamedColor = { id: newId(), name: t('Colour {n}', { n: list.length + 1 }), value: hexNow };
    if (source === 'project' && project) void saveProject(project.id, [...(project.colors ?? []), c]);
    else void save(palettes.map((p) => (p.id === source ? { ...p, colors: [...p.colors, c] } : p)));
  };
  const eyedropper = typeof window !== 'undefined' && 'EyeDropper' in window;
  const pickScreen = async () => {
    try {
      const Ed = (window as unknown as { EyeDropper: new () => { open: () => Promise<{ sRGBHex: string }> } }).EyeDropper;
      const r = await new Ed().open();
      const c = parseHex(r.sRGBHex);
      if (c) setRgb(c[0], c[1], c[2]);
    } catch {
      /* cancelled */
    }
  };
  const [r8, g8, b8] = rgb.map((v) => Math.round(v * 255));
  const hsl = rgbToHsl(rgb[0], rgb[1], rgb[2]);

  return createPortal(
    <div ref={pop} className="cp-pop" style={at ? { left: at.left, top: at.top } : { left: -9999, top: 0 }} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
      <div className="cp-sv" style={{ background: `linear-gradient(transparent, #000), linear-gradient(90deg, #fff, hsl(${hsv[0]}, 100%, 50%))` }} onPointerDown={sv}>
        <span className="cp-knob" style={{ left: `${hsv[1] * 100}%`, top: `${(1 - hsv[2]) * 100}%`, background: `rgb(${r8},${g8},${b8})` }} />
      </div>
      <div className="cp-bars">
        <span className="cp-prev">
          <i style={{ background: swatchCss(color) }} />
        </span>
        <div className="cp-bar cp-hue" onPointerDown={hue}>
          <span className="cp-knob" style={{ left: `${(hsv[0] / 360) * 100}%`, background: `hsl(${hsv[0]}, 100%, 50%)` }} />
        </div>
        <div className="cp-bar cp-alpha" onPointerDown={alp}>
          <i style={{ background: `linear-gradient(90deg, rgba(${r8},${g8},${b8},0), rgb(${r8},${g8},${b8}))` }} />
          <span className="cp-knob" style={{ left: `${alpha * 100}%`, background: swatchCss(color) }} />
        </div>
      </div>
      <div className="cp-vals">
        <Dropdown
          className="cp-mode"
          value={mode}
          options={[
            { value: 'hex', label: 'HEX' },
            { value: 'rgb', label: 'RGB' },
            { value: 'hsl', label: 'HSL' },
          ]}
          onChange={(m) => {
            setMode(m);
            try {
              localStorage.setItem(MODE, m);
            } catch {
              /* not kept */
            }
          }}
        />
        <div className="cp-main">
          {mode === 'hex' ? (
            <input
              className="cp-hex"
              value={hexText}
              spellCheck={false}
              aria-label="HEX"
              onChange={(e) => {
                const s = e.target.value.replace(/[^0-9a-f#]/gi, '');
                setHexText(s.replace('#', '').toUpperCase());
                const c = parseHex(s);
                if (c && s.replace('#', '').length >= 6) setRgb(c[0], c[1], c[2], s.replace('#', '').length === 8 ? c[3] : alpha);
              }}
              onBlur={() => setHexText(hex6)}
              onKeyDown={(e) => e.stopPropagation()}
            />
          ) : mode === 'rgb' ? (
            <>
              <Part value={r8} max={255} onChange={(v) => setRgb(v / 255, g8 / 255, b8 / 255)} />
              <Part value={g8} max={255} onChange={(v) => setRgb(r8 / 255, v / 255, b8 / 255)} />
              <Part value={b8} max={255} onChange={(v) => setRgb(r8 / 255, g8 / 255, v / 255)} />
            </>
          ) : (
            <>
              <Part value={hsl[0]} max={360} onChange={(v) => emit(rgbToHsv(...hslToRgb(v, hsl[1] / 100, hsl[2] / 100), v), alpha)} />
              <Part value={hsl[1]} max={100} onChange={(v) => setRgb(...hslToRgb(hsl[0], v / 100, hsl[2] / 100))} />
              <Part value={hsl[2]} max={100} onChange={(v) => setRgb(...hslToRgb(hsl[0], hsl[1] / 100, v / 100))} />
            </>
          )}
        </div>
        <label className="cp-a" title={t('Opacity')}>
          <Part value={Math.round(alpha * 100)} max={100} onChange={(v) => emit(hsv, v / 100)} />
          <span>%</span>
        </label>
        {eyedropper && (
          <button type="button" className="icon-btn cp-pick" title={t('Pick a colour on the screen')} onClick={() => void pickScreen()}>
            <Pipette size={14} />
          </button>
        )}
      </div>
      <div className="cp-src">
        <span>{t('Colours')}</span>
        <Dropdown className="cp-source" value={source} options={sources} onChange={setSource} />
      </div>
      <div className="cp-swatches">
        {list.map((c) => (
          <button key={c.id} type="button" className={`cp-sw ${sameColor(c.value, color) && Math.abs((parseHex(c.value)?.[3] ?? 1) - alpha) < 0.01 ? 'on' : ''}`} title={c.name ? `${c.name} · ${c.value.toUpperCase()}` : c.value.toUpperCase()} onClick={() => {
            const v = parseHex(c.value);
            if (v) setRgb(v[0], v[1], v[2], v[3]);
          }}>
            <i style={{ background: c.value }} />
          </button>
        ))}
        {source !== 'recent' && (
          <button type="button" className="cp-sw cp-add" title={t('Add this colour')} onClick={add}>
            <Plus size={13} />
          </button>
        )}
      </div>
      {named && (
        <div className="cp-named">
          <b style={{ background: named.value }} />
          {named.name}
        </div>
      )}
    </div>,
    document.body,
  );
}

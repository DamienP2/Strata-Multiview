import { LayoutGrid, Maximize, Minus, Plus } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { t } from '../i18n';
import { displayPref, setDisplayPref } from '../displayPrefs';

/*
 * Pan and zoom of the node canvases (Data, Logic, Outputs): the viewport
 * keeps its size and the space inside is moved and scaled.
 *   Shift + wheel, or pinch (trackpad)              zoom around the pointer
 *   wheel, two-finger scroll (trackpad)             pan
 *   Space + drag, middle button, drag on the void   pan (hand)
 *   Fit                                             every node in view
 *   Arrange (with opts.onArrange)                   the nodes laid out again, then Fit
 *                                                   (see autoLayout.ts)
 * Positions given to the canvas (toWorld) are in the space's coordinates,
 * whatever the zoom.
 */

export interface View {
  x: number;
  y: number;
  zoom: number;
}

const MIN = 0.2;
const MAX = 2.5;
const clamp = (z: number) => Math.min(MAX, Math.max(MIN, z));

/** Kept for the user. */
function loadView(key: string): View | null {
  const v = displayPref<View | null>(`view.${key}`, null);
  return v && typeof v.zoom === 'number' ? v : null;
}

/** True for a mouse wheel notch (zoom), false for a trackpad scroll (pan). */
const isMouseWheel = (e: WheelEvent) => e.deltaMode !== 0 || (e.deltaX === 0 && Math.abs(e.deltaY) >= 40 && Number.isInteger(e.deltaY));

export interface CanvasViewOptions {
  /** Shows an Arrange button next to Fit: moves the nodes (autoLayout), the view then fits them. */
  onArrange?: () => void;
}

export function useCanvasView(key: string, nodeSelector = '.logic-node', opts: CanvasViewOptions = {}) {
  const onArrangeRef = useRef(opts.onArrange);
  onArrangeRef.current = opts.onArrange;
  const ref = useRef<HTMLDivElement | null>(null);
  // The viewport can appear after the first render (a page still loading).
  const [el, setEl] = useState<HTMLDivElement | null>(null);
  const setRef = useCallback((e: HTMLDivElement | null) => {
    ref.current = e;
    setEl(e);
  }, []);
  const saved = useRef(loadView(key));
  const [view, setViewState] = useState<View>(saved.current ?? { x: 0, y: 0, zoom: 1 });
  const viewRef = useRef(view);
  viewRef.current = view;
  const [space, setSpace] = useState(false);
  const [panning, setPanning] = useState(false);

  const setView = useCallback(
    (v: View) => {
      viewRef.current = v;
      setViewState(v);
      setDisplayPref(`view.${key}`, v);
    },
    [key],
  );

  /** A screen point in the space's coordinates. */
  const toWorld = useCallback((clientX: number, clientY: number) => {
    const r = ref.current!.getBoundingClientRect();
    const v = viewRef.current;
    return { x: (clientX - r.left - v.x) / v.zoom, y: (clientY - r.top - v.y) / v.zoom };
  }, []);

  /** The part of the space in view. */
  const visible = useCallback(() => {
    const el = ref.current;
    const v = viewRef.current;
    const w = el?.clientWidth ?? 800;
    const h = el?.clientHeight ?? 600;
    return { x: -v.x / v.zoom, y: -v.y / v.zoom, w: w / v.zoom, h: h / v.zoom };
  }, []);

  const zoomAt = useCallback(
    (factor: number, cx?: number, cy?: number) => {
      const el = ref.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const px = cx === undefined ? r.width / 2 : cx - r.left;
      const py = cy === undefined ? r.height / 2 : cy - r.top;
      const v = viewRef.current;
      const zoom = clamp(v.zoom * factor);
      const k = zoom / v.zoom;
      setView({ zoom, x: px - (px - v.x) * k, y: py - (py - v.y) * k });
    },
    [setView],
  );

  /** Every node in view (at most 100 %). */
  const fit = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const nodes = [...el.querySelectorAll<HTMLElement>(nodeSelector)];
    if (!nodes.length) return setView({ x: 0, y: 0, zoom: 1 });
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const n of nodes) {
      x0 = Math.min(x0, n.offsetLeft);
      y0 = Math.min(y0, n.offsetTop);
      x1 = Math.max(x1, n.offsetLeft + n.offsetWidth);
      y1 = Math.max(y1, n.offsetTop + n.offsetHeight);
    }
    const pad = 40;
    const zoom = clamp(Math.min(1, (el.clientWidth - pad * 2) / (x1 - x0), (el.clientHeight - pad * 2) / (y1 - y0)));
    setView({ zoom, x: (el.clientWidth - (x1 - x0) * zoom) / 2 - x0 * zoom, y: Math.max(pad, (el.clientHeight - (y1 - y0) * zoom) / 2) - y0 * zoom });
  }, [nodeSelector, setView]);

  /** Arranges the nodes, then fits them once they are drawn where they went. */
  const arrange = useCallback(() => {
    onArrangeRef.current?.();
    requestAnimationFrame(() => requestAnimationFrame(fit));
  }, [fit]);

  // First opening: everything in view.
  useEffect(() => {
    if (saved.current || !el) return;
    const h = requestAnimationFrame(fit);
    return () => cancelAnimationFrame(h);
  }, [fit, el]);

  // Wheel: zoom (mouse wheel, pinch) or pan (trackpad scroll).
  useEffect(() => {
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const zoom = e.ctrlKey || e.metaKey || e.shiftKey;
      // A list or a table inside a node scrolls by itself.
      const inner = (e.target as HTMLElement).closest?.('[data-scroll], textarea, select');
      if (inner && !(e.ctrlKey || e.metaKey) && inner.scrollHeight > inner.clientHeight) return;
      e.preventDefault();
      // Shift turns the wheel sideways on macOS: either axis zooms.
      const dz = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
      if (zoom) zoomAt(Math.exp(-dz * (e.ctrlKey && !isMouseWheel(e) ? 0.01 : 0.0015)), e.clientX, e.clientY);
      else {
        const v = viewRef.current;
        setView({ ...v, x: v.x - e.deltaX, y: v.y - e.deltaY });
      }
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [zoomAt, setView, el]);

  // Space held: the hand.
  useEffect(() => {
    const typing = (t: EventTarget | null) => t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
    const down = (e: KeyboardEvent) => {
      if (e.code === 'Space' && !typing(e.target)) {
        e.preventDefault();
        setSpace(true);
      }
    };
    const up = (e: KeyboardEvent) => e.code === 'Space' && setSpace(false);
    const blur = () => setSpace(false);
    // Capture: the canvases stop the keys from reaching the page.
    window.addEventListener('keydown', down, true);
    window.addEventListener('keyup', up, true);
    window.addEventListener('blur', blur);
    return () => {
      window.removeEventListener('keydown', down, true);
      window.removeEventListener('keyup', up, true);
      window.removeEventListener('blur', blur);
    };
  }, []);

  const pan = (e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const start = { mx: e.clientX, my: e.clientY, ...viewRef.current };
    setPanning(true);
    const move = (ev: PointerEvent) => setView({ zoom: start.zoom, x: start.x + ev.clientX - start.mx, y: start.y + ev.clientY - start.my });
    const up = () => {
      setPanning(false);
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  /** On the viewport: Space or the middle button pans from anywhere. */
  const onPointerDownCapture = (e: React.PointerEvent) => {
    if (space || e.button === 1) pan(e);
  };
  /** On the viewport: a drag on the void pans too. */
  const onPointerDown = (e: React.PointerEvent) => {
    const t = e.target as Element;
    if (e.button === 0 && (t === ref.current || t.hasAttribute('data-canvas-void') || t instanceof SVGSVGElement)) pan(e);
  };

  /** Resizes a node from its corner; `apply` gets the new size in space units. */
  const resize = (e: React.PointerEvent, apply: (w: number, h: number) => void, min = { w: 160, h: 60 }) => {
    e.preventDefault();
    e.stopPropagation();
    const node = (e.currentTarget as HTMLElement).parentElement!;
    const start = { mx: e.clientX, my: e.clientY, w: node.offsetWidth, h: node.offsetHeight };
    const move = (ev: PointerEvent) => {
      const z = viewRef.current.zoom;
      apply(Math.max(min.w, Math.round(start.w + (ev.clientX - start.mx) / z)), Math.max(min.h, Math.round(start.h + (ev.clientY - start.my) / z)));
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  const grid = 20 * view.zoom;
  return {
    ref,
    view,
    toWorld,
    visible,
    setView,
    fit,
    arrange,
    resize,
    /** Props of the viewport element. */
    viewport: {
      ref: setRef,
      onPointerDownCapture,
      onPointerDown,
      className: `cv-viewport ${space ? 'hand' : ''} ${panning ? 'panning' : ''}`,
      style: { backgroundSize: `${grid}px ${grid}px`, backgroundPosition: `${view.x}px ${view.y}px` },
    },
    /** Style of the space inside it (the nodes and the wires). */
    spaceStyle: { transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})`, transformOrigin: '0 0' } as React.CSSProperties,
    controls: <CanvasControls zoom={view.zoom} onZoom={(f) => zoomAt(f)} onFit={fit} onArrange={opts.onArrange ? arrange : undefined} />,
  };
}

function CanvasControls({ zoom, onZoom, onFit, onArrange }: { zoom: number; onZoom: (factor: number) => void; onFit: () => void; onArrange?: () => void }) {
  return (
    <div className="cv-controls" onPointerDown={(e) => e.stopPropagation()}>
      <button title={t('Zoom out')} onClick={() => onZoom(1 / 1.2)}>
        <Minus size={13} />
      </button>
      <span title={t('Shift + wheel or pinch to zoom · wheel or Space + drag to move')}>{Math.round(zoom * 100)}%</span>
      <button title={t('Zoom in')} onClick={() => onZoom(1.2)}>
        <Plus size={13} />
      </button>
      <button title={t('Fit: every node in view')} onClick={onFit}>
        <Maximize size={13} /> {t('Fit')}
      </button>
      {onArrange && (
        <button title={t('Auto-arrange: lay the nodes out from left to right')} onClick={onArrange}>
          <LayoutGrid size={13} /> {t('Arrange')}
        </button>
      )}
    </div>
  );
}

/** The corner a node is resized from. */
export function ResizeCorner({ onPointerDown }: { onPointerDown: (e: React.PointerEvent) => void }) {
  return <span className="cv-resize" title={t('Resize')} onPointerDown={onPointerDown} />;
}

/**
 * A free place for a new node near `at` (canvas units): below the nodes it would cover,
 * so nodes never land on each other. `root` holds the canvas's `.logic-node` elements.
 */
export function freeSpot(root: Element | null, at: { x: number; y: number }, w = 260, h = 150): { x: number; y: number } {
  const rects = [...(root?.querySelectorAll<HTMLElement>('.logic-node') ?? [])].map((el) => ({ x: el.offsetLeft, y: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight }));
  let p = { ...at };
  for (let i = 0; i < 80; i++) {
    const hit = rects.find((r) => p.x < r.x + r.w + 16 && p.x + w + 16 > r.x && p.y < r.y + r.h + 16 && p.y + h + 16 > r.y);
    if (!hit) return p;
    p = { x: p.x, y: hit.y + hit.h + 24 };
  }
  return p;
}

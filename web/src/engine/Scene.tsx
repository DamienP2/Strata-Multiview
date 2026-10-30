import { SceneLayers } from './sceneLayers';
import { RichKeys, runs as richRuns, typedKeys } from './rich';
// HTML5 renderer: same conventions as crates/cg-render (box at x/y in parent
// space, rotate/scale around the anchor, crop as a reveal mask).
import { Boundary } from '../ui/Boundary';
import { Sparkles } from 'lucide-react';
import { t } from '../i18n';
import { withSettings } from './settings';
import { ValueMotionContext, valueTransform } from './valueMotion';
import { createContext, memo, useContext, useEffect, useMemo, useReducer, type CSSProperties } from 'react';
import { TimeContext, useLayerTime, type Motion } from './LiveTime';
import { col, cssColor, holdFrame, interpolateText, num, transformText, parseHex, resolveLayer, type Resolved } from './evaluate';
import type { Data, FontStyle, Layer, LayerType, Template, TextAlign, VerticalAlign } from './types';
import * as W from './widgets';
import { applyLogic, measureLayer } from './logic';
import { fitText, type Fitted } from './textFit';
import { applyMotion } from './motion';
import { useFonts } from './fonts';
import { widgetOf } from '../widgets';

export { TimeContext } from './LiveTime';

/** Layers whose look depends on the clock: they alone are redrawn on each frame (useLayerTime). */
export const LIVE_TYPES: ReadonlySet<LayerType> = new Set(['clock', 'ticker', 'roll', 'counter', 'progress', 'rotator', 'typewriter', 'slideshow', 'barChart']);

interface SceneProps {
  template: Template;
  frame: number;
  data: Data;
  /** Render hidden layers' ids for hit-testing in the designer. */
  interactive?: boolean;
}

export const LINE_HEIGHT = 1.2;

/** Layers changed by the template's logic graph (resolved values). */
const NO_LOGIC = new Map<string, Resolved>();
const LogicContext = createContext<Map<string, Resolved>>(NO_LOGIC);

/** How a legacy widget moves (see Motion): a clock with the wall clock, the others with their time on air. */
const legacyMotion = (l: Layer): Motion => (l.type === 'clock' ? 'wall' : LIVE_TYPES.has(l.type) ? 'age' : false);

/** How a text layer fits its box with these values (also for the Designer's and the Operator's warnings). */
export function fitLayer(layer: Layer & { type: 'text' }, r: Resolved, text: string): Fitted {
  return fitText(text, {
    fontFamily: layer.fontFamily,
    fontWeight: layer.fontWeight,
    size: num(r.fontSize),
    letterSpacing: num(r.letterSpacing),
    lineHeight: layer.lineHeight ?? LINE_HEIGHT,
    boxW: num(r.width),
    boxH: num(r.height),
    wrap: !!layer.wrap,
    overflow: layer.overflow ?? 'fit',
    squeeze: layer.squeeze ?? 0.8,
    minSize: layer.minSize ?? 0.6,
  });
}

export interface TextWarning {
  layer: Layer & { type: 'text' };
  ratio: number;
  overflows: boolean;
  /** Fields and data keys the text shows. */
  keys: string[];
}

/** Texts too long for their box with these values, at the on-air point: reduced under their warning, or not all shown. */
export function textWarnings(template: Template, data: Data): TextWarning[] {
  const hold = holdFrame(template);
  const out: TextWarning[] = [];
  for (const l of template.layers) {
    if (l.type !== 'text' || !l.visible || (l.scroll && l.scroll !== 'none') || (l.overflow ?? 'fit') === 'visible') continue;
    const fit = fitLayer(l, resolveLayer(l, hold, data, hold), layerText(l, data, typedKeys(template)));
    if (fit.overflows || fit.ratio < (l.warnBelow ?? 0.8)) out.push({ layer: l, ratio: fit.ratio, overflows: fit.overflows, keys: [...l.text.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]) });
  }
  return out;
}

/** A text layer's text as shown (fields filled in, case changed). */
export const layerText = (layer: Layer & { type: 'text' }, data: Data, rich: (key: string) => boolean = () => false) => {
  const kase = (t: string) => (layer.uppercase ? t.toUpperCase() : transformText(t, layer.transform));
  // Styled parts (<b>…): the text without its tags.
  const rs = richRuns(layer.text, data, rich, kase);
  return rs ? rs.map((r) => r.text).join('') : kase(interpolateText(layer.text, data));
};

function layerStyle(layer: Layer, frame: number, data: Data, hold: number, logic?: Resolved): { style: CSSProperties; text?: string; src?: string; squeeze?: number; fit?: string } {
  const r = logic ?? resolveLayer(layer, frame, data, hold);
  const w = num(r.width);
  const h = num(r.height);
  const style: CSSProperties = {
    position: 'absolute',
    left: num(r.x),
    top: num(r.y),
    width: w,
    height: h,
    opacity: num(r.opacity),
    transformOrigin: `${num(r.anchorX) * 100}% ${num(r.anchorY) * 100}%`,
    transform: `rotate(${num(r.rotation)}deg) scale(${num(r.scaleX)}, ${num(r.scaleY)})`,
    boxSizing: 'border-box',
  };
  const cl = num(r.cropLeft);
  const cr = num(r.cropRight);
  const ct = num(r.cropTop);
  const cb = num(r.cropBottom);
  if (cl > 0 || cr > 0 || ct > 0 || cb > 0) {
    const p = (v: number) => `${Math.min(1, Math.max(0, v)) * 100}%`;
    style.clipPath = `inset(${p(ct)} ${p(cr)} ${p(cb)} ${p(cl)})`;
  }
  // Drop shadow of the layer and its children, in its own space (cg-render draws the same); a Text has its own.
  const shadow = layerShadow(layer, r);
  if (shadow) style.filter = shadow;
  // The spec widgets draw everything inside their box (SpecWidget).
  if (widgetOf(layer)?.Render) return { style };
  const fill = cssColor(col(r.fill));
  const strokeW = num(r.strokeWidth);
  const stroke = cssColor(col(r.stroke));
  const strokeCss = (s: CSSProperties) => {
    if (strokeW > 0) {
      s.outline = `${strokeW}px solid ${stroke}`;
      s.outlineOffset = -strokeW / 2;
    }
  };
  switch (layer.type) {
    case 'group':
      if (layer.props.fill !== undefined || layer.tracks.fill) style.background = fill;
      if (layer.clip) style.overflow = 'hidden';
      style.borderRadius = num(r.cornerRadius);
      return { style };
    case 'rect':
      style.background = fill;
      style.borderRadius = num(r.cornerRadius);
      strokeCss(style);
      return { style };
    case 'ellipse':
      style.background = fill;
      style.borderRadius = '50%';
      strokeCss(style);
      return { style };
    case 'text': {
      // Too long for its box: narrower and smaller, "…", or cut.
      const fit = fitLayer(layer, r, layerText(layer, data));
      const text = fit.text;
      Object.assign(style, {
        display: 'flex',
        flexDirection: 'column',
        justifyContent: layer.verticalAlign === 'top' ? 'flex-start' : layer.verticalAlign === 'bottom' ? 'flex-end' : 'center',
        textAlign: layer.align,
        color: fill,
        fontFamily: `"${layer.fontFamily}", Inter, system-ui, sans-serif`,
        fontWeight: layer.fontWeight,
        fontSize: fit.size,
        letterSpacing: num(r.letterSpacing),
        lineHeight: layer.lineHeight ?? LINE_HEIGHT,
        textDecoration: layer.decoration === 'underline' ? 'underline' : layer.decoration === 'lineThrough' ? 'line-through' : undefined,
        textDecorationThickness: layer.decoration && layer.decoration !== 'none' ? `${Math.max(1, num(r.fontSize) * 0.06)}px` : undefined,
        textUnderlineOffset: layer.decoration === 'underline' ? `${num(r.fontSize) * 0.1}px` : undefined,
        // Wrapped already, as the Rust renderer does.
        whiteSpace: 'pre',
        overflow: fit.clip ? 'hidden' : 'visible',
        fontKerning: 'normal',
      } satisfies CSSProperties);
      if (strokeW > 0) (style as Record<string, unknown>).WebkitTextStroke = `${strokeW}px ${stroke}`;
      const warn = fit.overflows ? (layer.overflow === 'visible' ? 'overflows' : 'too long') : fit.ratio < (layer.warnBelow ?? 0.8) ? `${Math.round(fit.ratio * 100)} %` : undefined;
      return { style, text, squeeze: fit.squeeze, fit: warn };
    }
    case 'image':
      return { style, src: interpolateText(layer.src, data) };
    default:
      // Widgets draw their own content (see WidgetContent), inside their box.
      if (layer.type !== 'progress' && layer.type !== 'qrCode') style.overflow = 'hidden';
      return { style };
  }
}

/** The CSS filter of a layer's drop shadow (Layer.dropShadow), if it has one. */
export function layerShadow(layer: Layer, r: Resolved): string | undefined {
  if (!layer.dropShadow || layer.type === 'text') return undefined;
  const c = col(r.shadowColor);
  if (c[3] <= 0) return undefined;
  return `drop-shadow(${num(r.shadowX)}px ${num(r.shadowY)}px ${Math.max(0, num(r.shadowBlur))}px ${cssColor(c)})`;
}

const hexColor = (s: string, fallback = 'transparent') => {
  const c = parseHex(s);
  return c ? cssColor(c) : fallback;
};

const justify = (v: VerticalAlign) => (v === 'top' ? 'flex-start' : v === 'bottom' ? 'flex-end' : 'center');

/** A block of text in a box, laid out like the Rust text engine. */
function TextBox({ text, font, r, align, valign, box, color, stroke = true, opacity = 1 }: { text: string; font: FontStyle; r: Resolved; align: TextAlign; valign: VerticalAlign; box: [number, number, number, number]; color?: string; stroke?: boolean; opacity?: number }) {
  if (!text) return null;
  const strokeW = num(r.strokeWidth);
  const style: CSSProperties = {
    position: 'absolute',
    left: box[0],
    top: box[1],
    width: box[2],
    height: box[3],
    opacity,
    display: 'flex',
    flexDirection: 'column',
    justifyContent: justify(valign),
    textAlign: align,
    color: color ?? cssColor(col(r.fill)),
    fontFamily: `"${font.fontFamily}", Inter, system-ui, sans-serif`,
    fontWeight: font.fontWeight,
    fontSize: num(r.fontSize),
    letterSpacing: num(r.letterSpacing),
    lineHeight: LINE_HEIGHT,
    whiteSpace: 'pre',
    fontKerning: 'normal',
  };
  if (stroke && strokeW > 0) (style as Record<string, unknown>).WebkitTextStroke = `${strokeW}px ${cssColor(col(r.stroke))}`;
  return (
    <div style={style}>
      {text.split('\n').map((line, i) => (
        <div key={i} style={{ width: '100%' }}>
          {line || ' '}
        </div>
      ))}
    </div>
  );
}

function QrCode({ content, w, h, fill, background }: { content: string; w: number; h: number; fill: string; background: string }) {
  const [, rerender] = useReducer((x: number) => x + 1, 0);
  useEffect(() => W.onQrLoaded(rerender), []);
  const side = Math.min(w, h);
  const qr = content ? W.qrModules(content) : null;
  const rects: React.ReactNode[] = [];
  if (qr) {
    for (let i = 0; i < qr.modules.length; i++) {
      if (qr.modules[i] === '1') rects.push(<rect key={i} x={(i % qr.size) + 4} y={Math.floor(i / qr.size) + 4} width={1.02} height={1.02} />);
    }
  }
  const n = (qr?.size ?? 21) + 8;
  return (
    <svg style={{ position: 'absolute', left: (w - side) / 2, top: (h - side) / 2, width: side, height: side }} viewBox={`0 0 ${n} ${n}`} shapeRendering="crispEdges">
      <rect width={n} height={n} fill={background} />
      <g fill={fill}>{rects}</g>
    </svg>
  );
}

/** Content of a widget layer; reads the time context so only it re-renders. */
function WidgetContent({ layer, frame, data, hold }: { layer: Layer; frame: number; data: Data; hold: number }) {
  const base = useLayerTime(legacyMotion(layer));
  const time = base.layerAge ? { ...base, ageSecs: base.layerAge(layer) } : base;
  // Resolved once per frame of the timeline, not on each redraw of a moving widget.
  const resolved = useMemo(() => resolveLayer(layer, frame, data, hold), [layer, frame, data, hold]);
  const r = useContext(LogicContext).get(layer.id) ?? resolved;
  const w = num(r.width);
  const h = num(r.height);
  const age = time.ageSecs;
  const up = (s: string, font: FontStyle) => (font.uppercase ? s.toUpperCase() : s);
  const clip: CSSProperties = { position: 'absolute', inset: 0, overflow: 'hidden' };
  switch (layer.type) {
    case 'clock':
      return <TextBox text={up(W.clockText(layer.mode, layer.format, layer.timezone, layer.target, data, time), layer)} font={layer} r={r} align={layer.align} valign={layer.verticalAlign} box={[0, 0, w, h]} />;
    case 'counter':
      return <TextBox text={up(W.counterText(layer, data, age), layer)} font={layer} r={r} align={layer.align} valign={layer.verticalAlign} box={[0, 0, w, h]} />;
    case 'typewriter':
      return <TextBox text={up(W.typewriterText(layer.text, layer.speed, layer.cursor, data, age), layer)} font={layer} r={r} align={layer.align} valign={layer.verticalAlign} box={[0, 0, w, h]} />;
    case 'rotator': {
      const items = W.lines(layer.text, data);
      if (!items.length) return null;
      const [idx, alpha] = W.rotatorState(items.length, layer.interval, layer.fade, age);
      return <TextBox text={up(items[idx], layer)} font={layer} r={r} align={layer.align} valign={layer.verticalAlign} box={[0, 0, w, h]} opacity={alpha} />;
    }
    case 'ticker': {
      const items = W.lines(layer.text, data);
      if (!items.length) return null;
      const strip = up(items.join(layer.separator) + layer.separator, layer);
      const ls = num(r.letterSpacing);
      const cycle = W.textWidth(strip, layer.fontFamily, layer.fontWeight, num(r.fontSize), ls) + ls;
      return (
        <div style={clip}>
          {W.scrollPositions(w, cycle, layer.speed * age, true).map((x, i) => (
            <TextBox key={i} text={strip} font={layer} r={r} align="left" valign={layer.verticalAlign} box={[x, 0, cycle, h]} />
          ))}
        </div>
      );
    }
    case 'roll': {
      const body = up(interpolateText(layer.text, data), layer);
      const block = body.split('\n').length * num(r.fontSize) * LINE_HEIGHT;
      const cycle = layer.loop ? block + h : block;
      return (
        <div style={clip}>
          {W.scrollPositions(h, cycle, layer.speed * age, layer.loop).map((y, i) => (
            <TextBox key={i} text={body} font={layer} r={r} align={layer.align} valign="top" box={[0, y, w, block]} />
          ))}
        </div>
      );
    }
    case 'progress': {
      const ratio = W.progressRatio(layer.value, layer.max, layer.duration, data, age);
      const radius = num(r.cornerRadius);
      return (
        <>
          <div style={{ position: 'absolute', inset: 0, background: hexColor(layer.track), borderRadius: radius }} />
          <div style={{ position: 'absolute', left: 0, top: 0, height: h, width: w * ratio, background: cssColor(col(r.fill)), borderRadius: Math.min(radius, (w * ratio) / 2) }} />
        </>
      );
    }
    case 'slideshow': {
      const list = W.lines(layer.images, data);
      if (!list.length) return null;
      const [cur, next, mix] = W.slideshowState(list.length, layer.interval, layer.fade, age);
      const img = (src: string, opacity: number) => <img key={src + opacity} src={src} alt="" draggable={false} style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', objectFit: layer.fit === 'stretch' ? 'fill' : layer.fit, opacity }} />;
      return (
        <>
          {img(list[cur], 1 - mix)}
          {mix > 0 && img(list[next], mix)}
        </>
      );
    }
    case 'qrCode':
      return <QrCode content={interpolateText(layer.data, data)} w={w} h={h} fill={cssColor(col(r.fill))} background={hexColor(layer.background, 'white')} />;
    case 'barChart': {
      const rows = W.chartRows(layer.text, data);
      if (!rows.length) return null;
      const total = rows.reduce((a, [, v]) => a + Math.max(0, v), 0);
      const max = rows.reduce((a, [, v]) => Math.max(a, v), 0);
      const g = W.grow(layer.duration, age);
      const rowH = h / rows.length;
      const [labelW, valueW] = [w * 0.28, w * 0.16];
      const barX = labelW + w * 0.02;
      const barW = Math.max(0, w - labelW - valueW - w * 0.04);
      const color = hexColor(layer.textColor, 'white');
      return (
        <>
          {rows.map(([label, v], i) => {
            const y = i * rowH;
            const bh = rowH * 0.56;
            const by = y + (rowH - bh) / 2;
            const frac = max > 0 ? Math.min(1, Math.max(0, v / max)) : 0;
            const shown = layer.percent ? `${W.formatNumber((total > 0 ? (v / total) * 100 : 0) * g, 0, false)}%` : W.formatNumber(v * g, Number.isInteger(v) ? 0 : 1, true);
            return (
              <div key={i}>
                <div style={{ position: 'absolute', left: barX, top: by, width: barW, height: bh, background: hexColor(layer.track), borderRadius: bh / 4 }} />
                <div style={{ position: 'absolute', left: barX, top: by, width: barW * frac * g, height: bh, background: cssColor(col(r.fill)), borderRadius: bh / 4 }} />
                <TextBox text={up(label, layer)} font={layer} r={r} align="left" valign="middle" box={[0, y, labelW, rowH]} color={color} stroke={false} />
                <TextBox text={shown} font={layer} r={r} align="right" valign="middle" box={[w - valueW, y, valueW, rowH]} color={color} stroke={false} />
              </div>
            );
          })}
        </>
      );
    }
    default:
      return null;
  }
}

/** A spec widget (web/src/widgets) drawing its layer; reads the time context so only it re-renders. */
function SpecWidget({ layer, frame, data, hold, interactive }: { layer: Layer; frame: number; data: Data; hold: number; interactive?: boolean }) {
  const def = widgetOf(layer)!;
  // Only a widget that moves is redrawn on each frame; one moving with its time only, without an operator state, stands still with it.
  const still = useContext(TimeContext);
  const base = useLayerTime(!def.live?.(layer as never) ? false : def.ageOnly && still.widgetState?.(layer.id) == null ? 'age' : 'wall');
  const time = base.layerAge ? { ...base, ageSecs: base.layerAge(layer) } : base;
  // Resolved once per frame of the timeline, not on each redraw of a moving widget.
  const resolved = useMemo(() => resolveLayer(layer, frame, data, hold), [layer, frame, data, hold]);
  const r = useContext(LogicContext).get(layer.id) ?? resolved;
  const Render = def.Render!;
  return (
    <Boundary resetKey={layer}>
      <Render layer={layer as never} r={r} w={num(r.width)} h={num(r.height)} data={data} time={time} state={time.widgetState?.(layer.id)} frame={frame} hold={hold} fps={time.fps ?? 50} designer={interactive} lang={time.lang || 'fr'} />
    </Boundary>
  );
}

const LayerNode = memo(function LayerNode({ layer, childrenOf, frame, data, hold, interactive }: { layer: Layer; childrenOf: Map<string | null, Layer[]>; frame: number; data: Data; hold: number; interactive?: boolean }) {
  const logic0 = useContext(LogicContext).get(layer.id);
  // A repeated movement (Layer.motion): follows the layer's age, also while it holds on air (the rest stands still).
  const lt = useLayerTime(layer.motion ? 'wall' : false);
  const logic = layer.motion ? applyMotion(logic0 ?? resolveLayer(layer, frame, data, hold), layer.motion, frame, lt.fps ?? 50, lt.layerAge ? lt.layerAge(layer) : lt.ageSecs) : logic0;
  const framed = useContext(FramedContext);
  const motion = useContext(ValueMotionContext);
  const moving = motion?.layers.get(layer.id);
  // The old values' pass of a value sliding or turning over: only the layers showing it.
  if (motion?.old && !moving && layer.type !== 'group') return null;
  // Outside its span on the timeline: not shown (mirror of model::Layer::shown_at).
  if (layer.span && !(frame >= layer.span.from && frame < layer.span.to)) return null;
  const self = layer;
  const { style, text, src, squeeze = 1, fit } = layerStyle(layer, frame, data, hold, logic);
  const tf = moving ? valueTransform(moving[0], moving[1], motion!.old, typeof style.height === 'number' ? style.height : 0) : '';
  if (tf === null) return null;
  const kids = childrenOf.get(self.id) ?? [];
  const lines = text?.split('\n').map((line, i) => (
    <div key={i} style={{ width: '100%' }}>
      {line || ' '}
    </div>
  ));
  const body = (
    <>
      {lines && (squeeze < 1 ? <div style={{ width: `${100 / squeeze}%`, transform: `scaleX(${squeeze})`, transformOrigin: '0 0' }}>{lines}</div> : lines)}
      {src && layer.type === 'image' && <img src={src} alt="" draggable={false} style={{ width: '100%', height: '100%', objectFit: layer.fit === 'stretch' ? 'fill' : layer.fit, display: 'block' }} />}
      {widgetOf(layer)?.Render ? <SpecWidget layer={layer} frame={frame} data={data} hold={hold} interactive={interactive} /> : LIVE_TYPES.has(layer.type) || layer.type === 'qrCode' ? <WidgetContent layer={layer} frame={frame} data={data} hold={hold} /> : null}
      {layer.type === 'group' && layer.lottie !== undefined ? (
        <LottieKids group={layer} kids={kids} childrenOf={childrenOf} frame={frame} data={data} hold={hold} interactive={interactive} />
      ) : (
        kids.map((k) => <LayerNode key={k.id} layer={k} childrenOf={childrenOf} frame={framed.has(k.id) ? frame : hold} data={data} hold={hold} interactive={interactive} />)
      )}
      {interactive && layer.type === 'group' && layer.lottie !== undefined && !kids.length && <LottiePlaceholder />}
    </>
  );
  return (
    <div style={moving ? { ...style, overflow: 'hidden' } : style} data-layer-id={interactive ? layer.id : undefined} data-fit={interactive ? fit : undefined}>
      {moving ? (
        // Moved in its box, cut by it (mirror of the renderer's value_tf).
        <div style={{ position: 'absolute', inset: 0, display: style.display, flexDirection: style.flexDirection, justifyContent: style.justifyContent, alignItems: style.alignItems, transform: tf || undefined }}>{body}</div>
      ) : (
        body
      )}
    </div>
  );
});

/** The layers of a Lottie group: at the group's own clock when the operator controls it (reads the time context so only it re-renders). */
function LottieKids({ group, kids, childrenOf, frame, data, hold, interactive }: { group: Layer; kids: Layer[]; childrenOf: Map<string | null, Layer[]>; frame: number; data: Data; hold: number; interactive?: boolean }) {
  // Played by its own clock (the operator controls it): redrawn on each frame while that clock runs.
  const f = W.lottieFrame(group, frame, hold, useLayerTime(useContext(TimeContext).clocked?.(group) ? 'age' : false));
  const framed = useContext(FramedContext);
  return (
    <>
      {kids.map((k) => (
        <LayerNode key={k.id} layer={k} childrenOf={childrenOf} frame={framed.has(k.id) ? f : hold} data={data} hold={hold} interactive={interactive} />
      ))}
    </>
  );
}

/** A Lottie layer with no file yet, in the Designer only (nothing goes on air). */
function LottiePlaceholder() {
  return (
    <div style={{ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center', alignContent: 'center', gap: 10, border: '4px dashed rgba(255,255,255,.35)', borderRadius: 8, background: 'rgba(12,14,22,.45)', color: 'rgba(255,255,255,.7)', font: '600 26px Inter, sans-serif', textAlign: 'center', padding: 16 }}>
      <Sparkles size={44} />
      <div>{t('Animation (Lottie)')}</div>
      <div style={{ font: '500 18px Inter, sans-serif', opacity: 0.7 }}>{t('Choose a .json, .lottie or .zip file on the right')}</div>
    </div>
  );
}

/** The layers whose look changes with the playhead: animated properties, a pen drawing in, a Lottie, a logic target, or one of these inside. */
const FramedContext = createContext<ReadonlySet<string>>(new Set());

function framedLayers(layers: Layer[], childrenOf: Map<string | null, Layer[]>, logic: Template['logic']): Set<string> {
  const targets = new Set(logic?.nodes.flatMap((n) => (n.type === 'target' ? [n.layer] : [])) ?? []);
  const out = new Set<string>();
  const seen = new Set<string>();
  const visit = (l: Layer): boolean => {
    if (seen.has(l.id)) return out.has(l.id);
    seen.add(l.id);
    let on = l.type === 'pen' || (l.type === 'group' && l.lottie !== undefined) || targets.has(l.id) || !!l.span || Object.values(l.tracks ?? {}).some((t) => (t?.length ?? 0) > 0);
    for (const k of childrenOf.get(l.id) ?? []) on = visit(k) || on;
    if (on) out.add(l.id);
    return on;
  };
  layers.forEach(visit);
  return out;
}

export function childrenMap(layers: Layer[], includeHidden = false): Map<string | null, Layer[]> {
  const ids = new Set(layers.map((l) => l.id));
  const m = new Map<string | null, Layer[]>();
  for (const l of layers) {
    if (!includeHidden && !l.visible) continue;
    const parent = l.parent && ids.has(l.parent) ? l.parent : null;
    const arr = m.get(parent) ?? [];
    arr.push(l);
    m.set(parent, arr);
  }
  return m;
}

/** Renders a template at native resolution (the caller scales it). */
export const Scene = memo(function Scene({ template, frame, data, interactive }: SceneProps) {
  // Settings exposed to the Operator, set from the data.
  template = withSettings(template, data);
  // Kept from one frame to the next: the layers that do not change are not drawn again.
  const childrenOf = useMemo(() => childrenMap(template.layers), [template.layers]);
  const framed = useMemo(() => framedLayers(template.layers, childrenOf, template.logic), [template.layers, childrenOf, template.logic]);
  const hold = holdFrame(template);
  // Text measured by the logic changes when fonts finish loading.
  const fontsReady = useFonts((s) => s.ready);
  // Relations: resolve every layer, run the graph, keep the changed ones.
  const logic = useMemo(() => {
    if (!template.logic?.nodes.length) return NO_LOGIC;
    const out = new Map<string, Resolved>();
    const values = new Map(template.layers.map((l) => [l.id, resolveLayer(l, frame, data, hold)]));
    applyLogic(template.logic, values, data, measureLayer(template.layers, data, interpolateText));
    for (const n of template.logic.nodes) if (n.type === 'target' && values.has(n.layer)) out.set(n.layer, values.get(n.layer)!);
    return out;
  }, [template, frame, data, hold, fontsReady]); // eslint-disable-line react-hooks/exhaustive-deps
  const rich = useMemo(() => typedKeys(template), [template.fields]);
  return (
    <RichKeys.Provider value={rich}>
      <SceneLayers.Provider value={template.layers}>
      <LogicContext.Provider value={logic}>
      <FramedContext.Provider value={framed}>
        {(childrenOf.get(null) ?? []).map((l) => (
          // A layer that does not change with the playhead (nor anything inside it) is drawn once, not on each frame of the timeline.
          <LayerNode key={l.id} layer={l} childrenOf={childrenOf} frame={framed.has(l.id) ? frame : hold} data={data} hold={hold} interactive={interactive} />
        ))}
      </FramedContext.Provider>
      </LogicContext.Provider>
      </SceneLayers.Provider>
    </RichKeys.Provider>
  );
});

/** Fixed-size canvas scaled to fit its container. */
export function Viewport({ width, height, scale, children, className, style }: { width: number; height: number; scale: number; children: React.ReactNode; className?: string; style?: CSSProperties }) {
  return (
    <div className={className} style={{ width: width * scale, height: height * scale, position: 'relative', overflow: 'hidden', ...style }}>
      <div style={{ width, height, transform: `scale(${scale})`, transformOrigin: '0 0', position: 'absolute', left: 0, top: 0 }}>{children}</div>
    </div>
  );
}

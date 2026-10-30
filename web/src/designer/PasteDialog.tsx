import { Anchor, Blend, Check, ClipboardPaste, Clock, Crop, Diamond, Maximize2, Move, Palette, RotateCw, Scaling, SquareRoundCorner, Type, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { t, tn } from '../i18n';
import { cssColor } from '../engine/evaluate';
import type { Color, Prop, Value } from '../engine/types';
import { PROP_LABELS } from './Inspector';
import { findLayer, useDesigner } from './designerStore';
import { pasteAll, pasteChoices, useClipboard } from './clipboard';

const ICONS: Record<string, typeof Move> = { position: Move, size: Maximize2, scale: Scaling, rotation: RotateCw, opacity: Blend, anchor: Anchor, crop: Crop, colours: Palette, corners: SquareRoundCorner, text: Type };

const n = (v: Value | undefined, digits = 0) => (typeof v === 'number' ? (+v.toFixed(digits)).toLocaleString() : '—');
const pct = (v: Value | undefined) => (typeof v === 'number' ? `${Math.round(v * 100)} %` : '—');

/** What the copied layer has for a group: its values, as the Inspector shows them. */
function preview(id: string, v: Partial<Record<Prop, Value>>): React.ReactNode {
  switch (id) {
    case 'position':
      return `${n(v.x)}, ${n(v.y)}`;
    case 'size':
      return `${n(v.width)} × ${n(v.height)}`;
    case 'scale':
      return v.scaleX === v.scaleY ? pct(v.scaleX) : `${pct(v.scaleX)} × ${pct(v.scaleY)}`;
    case 'rotation':
      return `${n(v.rotation, 1)}°`;
    case 'opacity':
      return pct(v.opacity);
    case 'anchor':
      return `${pct(v.anchorX)}, ${pct(v.anchorY)}`;
    case 'corners':
      return `${n(v.cornerRadius)} px`;
    case 'text':
      return `${n(v.fontSize)} px · ${n(v.letterSpacing, 1)}`;
    case 'colours':
      return (
        <span className="pd-swatches">
          {(['fill', 'fill2', 'stroke'] as Prop[])
            .map((p) => v[p])
            .filter((c): c is Color => Array.isArray(c))
            .map((c, i) => (
              <i key={i} style={{ background: cssColor(c) }} />
            ))}
        </span>
      );
    default:
      return null;
  }
}

function Tile({ on, icon: Icon, label, detail, onClick }: { on: boolean; icon: typeof Move; label: string; detail?: React.ReactNode; onClick: () => void }) {
  return (
    <button type="button" className={`pd-tile ${on ? 'on' : ''}`} aria-pressed={on} onClick={onClick}>
      <Icon size={15} />
      <span className="pd-text">
        <strong>{label}</strong>
        {detail != null && <span className="pd-detail">{detail}</span>}
      </span>
      <span className="pd-check">{on && <Check size={12} />}</span>
    </button>
  );
}

/** « Paste… » after « Copy all »: what to take from the copied layer — values by group, its animation, its span. */
export function PasteDialog() {
  const targets = useClipboard((c) => c.pasting);
  const clip = useClipboard((c) => c.all);
  const template = useDesigner((s) => s.template);
  const [groups, setGroups] = useState<string[]>([]);
  const [animation, setAnimation] = useState(true);
  const [span, setSpan] = useState(false);
  const layers = (targets ?? []).map((id) => template && findLayer(template, id)).filter((l) => !!l);
  const choices = clip && layers.length ? pasteChoices(clip, layers) : null;
  // The animation ticked to begin with (values would move the layer onto the copied one: chosen by hand).
  useEffect(() => {
    if (!choices) return;
    setGroups([]);
    setAnimation(choices.animated.length > 0);
    setSpan(false);
  }, [targets]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!targets || !clip || !choices) return null;
  const close = () => useClipboard.setState({ pasting: null });
  const toggle = (id: string) => setGroups((g) => (g.includes(id) ? g.filter((x) => x !== id) : [...g, id]));
  const hasAnim = choices.animated.length > 0;
  const total = choices.groups.length + (hasAnim ? 1 : 0) + (choices.span ? 1 : 0);
  const count = groups.length + (animation && hasAnim ? 1 : 0) + (span ? 1 : 0);
  const all = (on: boolean) => {
    setGroups(on ? choices.groups.map((g) => g.id) : []);
    setAnimation(on && hasAnim);
    setSpan(on && choices.span);
  };
  return createPortal(
    <div className="modal-back" onPointerDown={close}>
      <form
        className="modal paste-dialog"
        onPointerDown={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          pasteAll(targets, { groups, animation: animation && hasAnim, span });
          close();
        }}
      >
        <header>
          <h2>{t('Paste from “{name}”', { name: clip.label })}</h2>
          <button type="button" className="icon-btn ghost" title={t('Close')} onClick={close}>
            <X size={16} />
          </button>
        </header>
        <div className="pd-bar">
          <span className="muted small">{layers.length === 1 ? t('On “{name}”', { name: layers[0]!.name }) : tn(layers.length, 'On {n} layer', 'On {n} layers')}</span>
          <span className="spacer" />
          <button type="button" className="btn small" disabled={count === total} onClick={() => all(true)}>
            {t('All')}
          </button>
          <button type="button" className="btn small" disabled={!count} onClick={() => all(false)}>
            {t('None')}
          </button>
        </div>
        {choices.groups.length > 0 && (
          <section className="pd-section">
            <span className="set-label">{t('Values')}</span>
            <div className="pd-grid">
              {choices.groups.map((g) => (
                <Tile key={g.id} on={groups.includes(g.id)} icon={ICONS[g.id] ?? Move} label={t(g.label)} detail={preview(g.id, clip.values)} onClick={() => toggle(g.id)} />
              ))}
            </div>
          </section>
        )}
        {(hasAnim || choices.span) && (
          <section className="pd-section">
            <span className="set-label">{t('Animation')}</span>
            <div className="pd-grid">
              {hasAnim && <Tile on={animation} icon={Diamond} label={t('Keyframes')} detail={choices.animated.map((p) => t(PROP_LABELS[p] ?? p)).join(', ')} onClick={() => setAnimation((a) => !a)} />}
              {choices.span && <Tile on={span} icon={Clock} label={t('When it is shown (timeline bar)')} onClick={() => setSpan((s) => !s)} />}
            </div>
          </section>
        )}
        <footer>
          <button type="button" className="btn" onClick={close}>
            {t('Cancel')}
          </button>
          <button className="btn primary" disabled={!count}>
            <ClipboardPaste size={14} /> {t('Paste')}
          </button>
        </footer>
      </form>
    </div>,
    document.body,
  );
}

import { Check, Sparkles, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { t, tn } from '../i18n';
import { useApp } from '../store';
import { apply, autoFor, EASE_FN, kindOf, presetsFor, useAnimations, type Cat, type Choice, type Kind, type Preset } from './animPresets';
import { findLayer, useDesigner } from './designerStore';

const CATS: [Cat, string][] = [
  ['in', 'In'],
  ['out', 'Out'],
  ['loop', 'Loop'],
];

/** The sample a preview moves: a name strap, a shape, a picture or a stroke. */
function Sample({ kind }: { kind: Kind }) {
  if (kind === 'pen') return <i className="an-obj pen" />;
  if (kind === 'image') return <i className="an-obj image" />;
  if (kind === 'shape') return <i className="an-obj shape" />;
  return <i className="an-obj text">Jean Dupont</i>;
}

/** The previews move together, on one clock: an in plays then rests, an out rests then plays, a loop swings. */
function usePreviews(root: React.RefObject<HTMLDivElement | null>, list: Preset[], secs: number) {
  useEffect(() => {
    let raf = 0;
    const t0 = performance.now();
    const tick = (now: number) => {
      const el = root.current;
      if (el) {
        const d = secs * 1000;
        for (const node of el.querySelectorAll<HTMLElement>('[data-preset]')) {
          const p = list.find((x) => `${x.cat}:${x.id}` === node.dataset.preset);
          const obj = node.firstElementChild as HTMLElement | null;
          if (!p || !obj) continue;
          let k: number;
          if (p.cat === 'loop') {
            const ph = ((now - t0) % 1600) / 1600;
            k = EASE_FN.inOut(ph < 0.5 ? ph * 2 : 2 - ph * 2);
          } else {
            const cycle = d + 900;
            const tt = (now - t0) % cycle;
            // An out holds first, then plays.
            const raw = p.cat === 'out' ? Math.max(0, Math.min(1, (tt - 900) / d)) : Math.min(1, tt / d);
            k = EASE_FN[p.ease](raw);
          }
          const g = p.move ?? {};
          const v = (r?: [number, number], or = 0) => (r ? r[0] + (r[1] - r[0]) * k : or);
          const s = v(g.scale, 1);
          const crop = [v(g.cropTop), v(g.cropRight), v(g.cropBottom), v(g.cropLeft)];
          const draw = g.drawEnd || g.drawStart ? [v(g.drawStart, 0), v(g.drawEnd, 100)] : null;
          obj.style.transform = `translate(${v(g.x) * 70}px, ${v(g.y) * 26}px) rotate(${v(g.rotation)}deg) scale(${v(g.scaleX, 1) * s}, ${v(g.scaleY, 1) * s})`;
          obj.style.opacity = String(v(g.opacity, 1));
          obj.style.letterSpacing = `${v(g.letterSpacing) / 3}px`;
          obj.style.clipPath = draw ? `inset(0 ${100 - draw[1]}% 0 ${draw[0]}%)` : `inset(${crop.map((c) => `${c * 100}%`).join(' ')})`;
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [root, list, secs]);
}

/** Designer › right-click › Animations…: ready-made in, out and loop animations, previewed, fitted to the layers. */
export function AnimationsDialog() {
  const ids = useAnimations((s) => s.ids);
  const template = useDesigner((s) => s.template);
  const project = useApp((s) => s.project);
  const fps = project ? project.frameRate.num / project.frameRate.den || 50 : 50;
  const [cat, setCat] = useState<Cat>('in');
  const [choice, setChoice] = useState<Choice>({ in: null, out: null, loop: null, secs: null });
  const grid = useRef<HTMLDivElement>(null);
  const layers = (ids ?? []).map((id) => template && findLayer(template, id)).filter((l) => !!l);
  const list = presetsFor(cat, layers);
  const auto = template ? autoFor(template, layers, fps) : 1.5;
  const secs = choice.secs ?? auto;
  usePreviews(grid, list, secs);
  useEffect(() => {
    setCat('in');
    setChoice({ in: null, out: null, loop: null, secs: null });
  }, [ids]);
  if (!ids || !template || !layers.length) return null;
  const close = () => useAnimations.setState({ ids: null });
  const kinds = new Set(layers.map(kindOf));
  const kind: Kind = kinds.size === 1 ? [...kinds][0] : 'other';
  const name = (c: Cat) => {
    const id = choice[c];
    if (id === 'none') return t('None');
    const p = id ? presetsFor(c, layers).find((x) => x.id === id) : undefined;
    return p ? t(p.label) : '—';
  };
  const pick = (id: string) => setChoice((c) => ({ ...c, [cat]: c[cat] === id ? null : id }));
  const any = !!(choice.in || choice.out || choice.loop);
  return createPortal(
    <div className="modal-back" onPointerDown={close}>
      <div className="modal an-dialog" role="dialog" aria-label={t('Animations')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
        <header>
          <h2>{t('Animations')}</h2>
          <span className="muted small">{layers.length === 1 ? t('On “{name}”', { name: layers[0]!.name }) : tn(layers.length, 'On {n} layer', 'On {n} layers')}</span>
          <span className="grow" />
          <button type="button" className="icon-btn ghost" title={t('Close')} onClick={close}>
            <X size={16} />
          </button>
        </header>
        <div className="seg an-tabs">
          {CATS.map(([c, label]) => (
            <button key={c} type="button" className={cat === c ? 'on' : ''} onClick={() => setCat(c)}>
              {t(label)}
              {choice[c] && <i className="an-dot" />}
            </button>
          ))}
        </div>
        <div className="an-grid" ref={grid}>
          {cat === 'loop' && (
            <button type="button" className={`an-card ${choice.loop === 'none' ? 'on' : ''}`} onClick={() => pick('none')}>
              <span className="an-pv none">
                <X size={18} />
              </span>
              <span className="an-name">{t('None')}</span>
            </button>
          )}
          {list.map((p) => (
            <button key={p.id} type="button" className={`an-card ${choice[cat] === p.id ? 'on' : ''}`} onClick={() => pick(p.id)}>
              <span className="an-pv" data-preset={`${p.cat}:${p.id}`}>
                <Sample kind={p.only?.[0] ?? kind} />
              </span>
              <span className="an-name">
                {t(p.label)}
                {choice[cat] === p.id && <Check size={13} />}
              </span>
            </button>
          ))}
        </div>
        <footer>
          <span className="an-chosen">
            {CATS.map(([c, label]) => (
              <span key={c}>
                {t(label)} <b>{name(c)}</b>
              </span>
            ))}
          </span>
          <span className="grow" />
          <label className="check an-auto">
            <input type="checkbox" checked={choice.secs === null} onChange={(e) => setChoice((c) => ({ ...c, secs: e.target.checked ? null : auto }))} />
            {t('Auto')}
          </label>
          <input className="an-range" type="range" min={0.2} max={3} step={0.1} value={secs} disabled={choice.secs === null} onChange={(e) => setChoice((c) => ({ ...c, secs: +e.target.value }))} />
          <span className="an-secs">{(+secs.toFixed(1)).toLocaleString()} s</span>
          <button type="button" className="btn" onClick={close}>
            {t('Cancel')}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={!any}
            onClick={() => {
              apply(ids, choice, fps);
              close();
            }}
          >
            <Sparkles size={14} /> {t('Apply')}
          </button>
        </footer>
      </div>
    </div>,
    document.body,
  );
}

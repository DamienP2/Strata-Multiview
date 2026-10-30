import { Check, ChevronDown, ChevronRight, Contrast, Eye, Ruler, ShieldCheck, SquareDashed } from 'lucide-react';
import { inputMessage, useInputs, useLiveFrame, type InputInfo } from '../engine/liveInputs';
import { useEffect, useState } from 'react';
import type { Project } from '../engine/types';
import { Segmented } from '../ui/fields';
import { t } from '../i18n';
import { GuidesSettings, SafeSettings, STAGE_VIEWS, useGuidesView } from './Guides';

/*
 * The Designer toolbar's "Display" menu: guides, safe areas (each
 * shown / hidden, with its settings), the output view, the stage background
 * and the layout of the panels.
 */

export type StageBackground = 'checker' | 'black' | 'studio' | 'video';
const BACKGROUNDS: { id: StageBackground; label: string }[] = [
  { id: 'checker', label: 'transparent' },
  { id: 'black', label: 'black' },
  { id: 'studio', label: 'studio' },
  { id: 'video', label: 'video' },
];

type Sub = 'guides' | 'safe' | null;

export function DisplayMenu({
  project,
  background,
  onBackground,
  bgInput,
  onBgInput,
}: {
  project: Project;
  background: StageBackground;
  onBackground: (b: StageBackground) => void;
  /** The live input behind the stage (« video »). */
  bgInput: string;
  onBgInput: (id: string) => void;
}) {
  const view = useGuidesView();
  const [open, setOpen] = useState<{ left: number; top: number } | null>(null);
  const [sub, setSub] = useState<Sub>(null);
  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(null);
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.stopPropagation(), setOpen(null));
    window.addEventListener('pointerdown', close);
    window.addEventListener('keydown', esc, true);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('keydown', esc, true);
      window.removeEventListener('resize', close);
    };
  }, [open]);
  const mode = view.stageView ?? 'program';
  const row = (id: Exclude<Sub, null>, on: boolean, toggle: () => void, icon: React.ReactNode, label: string, body: React.ReactNode) => (
    <>
      <div className={`ux2-drow ${sub === id ? 'open' : ''}`}>
        <button className="ux2-dcheck" role="menuitemcheckbox" aria-checked={on} onClick={toggle}>
          <span className={`ux2-ck ${on ? 'on' : ''}`}>{on && <Check size={11} />}</span>
          {icon}
          <span>{label}</span>
        </button>
        <button className="ux2-adj" aria-expanded={sub === id} onClick={() => setSub(sub === id ? null : id)}>
          {t('Tune')} {sub === id ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>
      </div>
      {sub === id && <div className="ux2-dsub">{body}</div>}
    </>
  );
  return (
    <div className="menu-wrap ux2-display" onPointerDown={(e) => e.stopPropagation()}>
      <button
        className={`btn small ${open ? 'on' : ''}`}
        title={t('Stage display')}
        aria-haspopup="menu"
        aria-expanded={!!open}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setOpen(open ? null : { left: Math.max(8, Math.min(r.left, window.innerWidth - 330)), top: r.bottom + 6 });
        }}
      >
        <Eye size={13} /> {t('Display')} <ChevronDown size={12} />
      </button>
      {open && (
        <div className="menu ux2-dmenu" role="menu" onClick={(e) => (e.target as HTMLElement).closest('[data-close]') && setOpen(null)} style={{ position: 'fixed', left: open.left, top: open.top, maxHeight: `calc(100vh - ${open.top + 12}px)` }}>
          <div className="ux2-drow">
            <button className="ux2-dcheck" role="menuitemcheckbox" aria-checked={view.frames} onClick={() => view.set({ frames: !view.frames })}>
              <span className={`ux2-ck ${view.frames ? 'on' : ''}`}>{view.frames && <Check size={11} />}</span>
              <SquareDashed size={13} />
              <span>{t('Widget frames')}</span>
            </button>
          </div>
          {row('guides', view.guides, () => view.set({ guides: !view.guides }), <Ruler size={13} />, t('Guides'), <GuidesSettings />)}
          {row('safe', view.safe, () => view.set({ safe: !view.safe, editSafe: false }), <ShieldCheck size={13} />, t('Safe area'), <SafeSettings project={project} />)}
          <div className="menu-sep" />
          <div className="ux2-dlabel">
            <Contrast size={12} /> {t('Output view')}
          </div>
          <Segmented value={mode} onChange={(v) => view.set({ stageView: v })} options={STAGE_VIEWS.map((v) => ({ value: v.id, label: t(v.label) }))} />
          <div className="menu-sep" />
          <div className="ux2-dline ux2-dbg">
            <span className={`bg-swatch stage-frame bg-${background}`} />
            <span>{t('Stage background')}</span>
            <Segmented value={background} onChange={onBackground} options={BACKGROUNDS.map((b) => ({ value: b.id, label: t(b.label) }))} />
          </div>
          {background === 'video' && <InputPick value={bgInput} onChange={onBgInput} />}
        </div>
      )}
    </div>
  );
}

/** The live inputs to show behind the stage. */
function InputPick({ value, onChange }: { value: string; onChange: (id: string) => void }) {
  const inputs = useInputs();
  if (!inputs.length) return <div className="ux2-dinputs empty">{t('No video input')}</div>;
  return (
    <div className="ux2-dinputs">
      {inputs.map((i) => (
        <InputRow key={i.id} input={i} on={i.id === value} onPick={() => onChange(i.id)} />
      ))}
    </div>
  );
}

function InputRow({ input, on, onPick }: { input: InputInfo; on: boolean; onPick: () => void }) {
  const { url } = useLiveFrame(input.id, 'tile');
  const format = input.width ? `${input.width}×${input.height}${input.fps ? ` · ${Math.round(input.fps * 100) / 100}${input.interlaced ? 'i' : 'p'}` : ''}` : inputMessage(input.message);
  return (
    <button className={`ux2-dinput ${on ? 'on' : ''}`} onClick={onPick}>
      <span className="th">{url && <img src={url} alt="" />}</span>
      <span className="tx">
        <b>{input.name}</b>
        <em>
          {input.kind.toUpperCase()} · {format}
        </em>
      </span>
    </button>
  );
}

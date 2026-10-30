import { Check, ChevronDown, ChevronRight, Contrast, Eye, Layers, LayoutGrid, PanelLeft, PanelRight, Ruler, Settings2, ShieldCheck, SquareDashed } from 'lucide-react';
import { inputMessage, useInputs, useLiveFrame, type InputInfo } from '../engine/liveInputs';
import { useEffect, useState } from 'react';
import type { Project } from '../engine/types';
import { Segmented } from '../ui/fields';
import { t } from '../i18n';
import { useDesigner } from './designerStore';
import { GuidesSettings, SafeSettings, STAGE_VIEWS, useGuidesView } from './Guides';
import { useGhosts } from './ghosts';
import { designTemplates } from '../engine/drafts';

/*
 * The Designer toolbar's "Display" menu: zones, guides, safe areas (each
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

type Sub = 'zones' | 'guides' | 'safe' | 'ghosts' | null;

export function DisplayMenu({
  project,
  showZones,
  onShowZones,
  onManageZones,
  background,
  onBackground,
  bgInput,
  onBgInput,
  leftFull,
  rightFull,
  onLayout,
}: {
  project: Project;
  showZones: boolean;
  onShowZones: (v: boolean) => void;
  onManageZones: () => void;
  background: StageBackground;
  onBackground: (b: StageBackground) => void;
  /** The live input behind the stage (« video »). */
  bgInput: string;
  onBgInput: (id: string) => void;
  leftFull: boolean;
  rightFull: boolean;
  onLayout: (side: 'leftFull' | 'rightFull', on: boolean) => void;
}) {
  const view = useGuidesView();
  const ghosts = useGhosts();
  const current = useDesigner((s) => s.template?.zone);
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
          {row(
            'zones',
            showZones,
            () => onShowZones(!showZones),
            <LayoutGrid size={13} />,
            t('Zones'),
            <>
              <div className="zm-list">
                {[...project.zones]
                  .sort((a, b) => b.layer - a.layer)
                  .map((z) => (
                    <div key={z.id} className={`zm-zone ${z.id === current ? 'mine' : ''}`} style={{ ['--zc' as string]: z.color }}>
                      <span className="zm-dot" />
                      <span>{z.name}</span>
                      <em>{z.id === current ? t('this template') : t('layer {n}', { n: z.layer })}</em>
                    </div>
                  ))}
              </div>
              <button
                className="btn small"
                onClick={() => {
                  setOpen(null);
                  onManageZones();
                }}
              >
                <Settings2 size={13} /> {t('Manage zones…')}
              </button>
            </>,
          )}
          <div className="ux2-drow">
            <button className="ux2-dcheck" role="menuitemcheckbox" aria-checked={view.frames} onClick={() => view.set({ frames: !view.frames })}>
              <span className={`ux2-ck ${view.frames ? 'on' : ''}`}>{view.frames && <Check size={11} />}</span>
              <SquareDashed size={13} />
              <span>{t('Widget frames')}</span>
            </button>
          </div>
          {row('ghosts', ghosts.on, () => ghosts.set({ on: !ghosts.on }), <Layers size={13} />, t('Other templates'), <GhostSettings project={project} />)}
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
          <div className="menu-sep" />
          <div className="ux2-dline">
            <span>{t('Panel layout')}</span>
            <span className="ux2-layout">
              <button className={leftFull ? 'on' : ''} aria-pressed={leftFull} title={t('Left panel full height')} onClick={() => onLayout('leftFull', !leftFull)}>
                <PanelLeft size={15} />
              </button>
              <button className={rightFull ? 'on' : ''} aria-pressed={rightFull} title={t('Right panel full height')} onClick={() => onLayout('rightFull', !rightFull)}>
                <PanelRight size={15} />
              </button>
            </span>
          </div>
        </div>
      )}
    </div>
  );
}

/** Which other templates show faint under the open one (all, or some), and how faint. */
function GhostSettings({ project }: { project: Project }) {
  const ghosts = useGhosts();
  const open = useDesigner((s) => s.template?.id);
  const others = designTemplates(project).filter((tpl) => tpl.id !== open);
  const all = ghosts.ids === null;
  const shown = (id: string) => all || ghosts.ids!.includes(id);
  const toggle = (id: string) => {
    const ids = all ? others.map((o) => o.id) : ghosts.ids!;
    ghosts.set({ on: true, ids: ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id] });
  };
  return (
    <>
      <div className="ux2-ghosts">
        <button className="ux2-dcheck" role="menuitemcheckbox" aria-checked={all} onClick={() => ghosts.set({ on: true, ids: all ? [] : null })}>
          <span className={`ux2-ck ${all ? 'on' : ''}`}>{all && <Check size={11} />}</span>
          <span>{t('All')}</span>
        </button>
        {others.map((o) => (
          <button key={o.id} className="ux2-dcheck" role="menuitemcheckbox" aria-checked={shown(o.id)} onClick={() => toggle(o.id)}>
            <span className={`ux2-ck ${shown(o.id) ? 'on' : ''}`}>{shown(o.id) && <Check size={11} />}</span>
            <span>{o.name}</span>
          </button>
        ))}
      </div>
      <label className="ux2-dline ux2-ghost-op">
        <span>{t('Opacity')}</span>
        <input type="range" min={0.1} max={0.8} step={0.05} value={ghosts.opacity} onChange={(e) => ghosts.set({ opacity: +e.target.value })} />
        <span className="fit-val">{Math.round(ghosts.opacity * 100)} %</span>
      </label>
    </>
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

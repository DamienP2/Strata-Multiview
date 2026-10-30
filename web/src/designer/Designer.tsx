import { LayoutButton } from './Layouts';
import { Redo2, Undo2 } from 'lucide-react';
import { memo, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { num, propValue } from '../engine/evaluate';
import type { Layer, LayerType, Template } from '../engine/types';
import { useApp } from '../store';
import { descendants, duplicateLayers, findLayer, makeLayer, setProp, useDesigner } from './designerStore';
import { useShallow } from 'zustand/react/shallow';
import { Inspector } from './Inspector';
import { LockOverlay, TopPublish, useEditLock } from './Collab';
import { Stage, StageZoom } from './Stage';
import { InsertTools } from './Insert';
import { copyLayers, cutLayers, pasteClipboard } from './clipboard';
import { useGuidesView } from './Guides';
import { DisplayMenu, type StageBackground } from './DisplayMenu';
import { ProjectHistory } from './MultiviewSettings';
import { Splitter, useLayout } from '../ui/Splitter';
import { designTemplates } from '../engine/drafts';
import { t } from '../i18n';
import { useDisplayPref } from '../displayPrefs';

/*
 * The Designer edits one multiview: a project is a multiview, its page (the
 * project's single template) open on the stage, the inspector on the right.
 */

/** The multiview the Designer has open (the store outlives the Designer). */
let designedProject: string | null = null;

function useAutosave() {
  // Saves the draft after the user's edits (not when a template is opened).
  const edits = useDesigner((s) => s.edits);
  const saveTemplate = useApp((s) => s.saveTemplate);
  const [state, setState] = useState<'saved' | 'pending'>('saved');
  // Edits waiting to be saved, per project and template: switching template, project or leaving never drops one,
  // and an edit is saved in the project it was made in, whatever project is open by then.
  const pending = useRef(new Map<string, { project: string; template: Template }>());
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const flush = useRef(async () => {});
  flush.current = async () => {
    clearTimeout(timer.current);
    const list = [...pending.current.values()];
    pending.current.clear();
    for (const p of list) await saveTemplate(p.template, p.project);
    setState('saved');
  };
  useEffect(() => {
    if (!edits) return;
    const { base, readOnly } = useDesigner.getState();
    const project = designedProject ?? useApp.getState().projectId;
    if (!base || readOnly || !project) return;
    setState('pending');
    pending.current.set(`${project}/${base.id}`, { project, template: base });
    clearTimeout(timer.current);
    timer.current = setTimeout(() => void flush.current(), 500);
  }, [edits, saveTemplate]);
  // Leaving the Designer: saved now.
  useEffect(() => () => void flush.current(), []);
  return { state, flush: () => flush.current() };
}

function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.closest('input, textarea, select, [contenteditable="true"]')) return;
      // A dialog is open: its keys are its own (Delete would remove the layer behind it).
      if (e.defaultPrevented || document.querySelector('.modal-back')) return;
      const st = useDesigner.getState();
      const t = st.template;
      // Esc: out of the safe area editing, then off the selection.
      if (e.key === 'Escape' && useGuidesView.getState().editSafe) return useGuidesView.getState().set({ editSafe: false });
      if (e.key === 'Escape' && t) {
        st.set({ selected: [], selectedKeys: [] });
        return;
      }
      if (!t) return;
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === 'z') {
        e.preventDefault();
        if (e.shiftKey) st.redo();
        else st.undo();
      } else if (mod && e.key.toLowerCase() === 'y') {
        e.preventDefault();
        st.redo();
      } else if (mod && e.key.toLowerCase() === 'd') {
        e.preventDefault();
        duplicateSelection();
      } else if (mod && !e.shiftKey && !e.altKey && ['c', 'x', 'v'].includes(e.key.toLowerCase())) {
        // Copy / cut / paste layers (clipboard.ts); a text selected on the page is copied as usual.
        const k = e.key.toLowerCase();
        if (k !== 'v' && (!st.selected.length || window.getSelection()?.toString())) return;
        e.preventDefault();
        if (k === 'c') copyLayers(st.selected);
        else if (k === 'x') cutLayers(st.selected);
        else pasteClipboard();
      } else if (e.key === ' ') {
        // Space held: the hand, to move the canvas (Stage). A focused button would also "click" on it.
        e.preventDefault();
        if (target.closest('button')) (target as HTMLElement).blur();
      } else if (e.key === 'Delete' || e.key === 'Backspace') {
        if (st.selected.length) {
          st.update((d) => {
            const ids = new Set(st.selected.flatMap((id) => [id, ...descendants(d, id)]));
            d.layers = d.layers.filter((l) => !ids.has(l.id));
          });
          st.select([]);
        }
      } else if (e.key.startsWith('Arrow') && st.selected.length) {
        e.preventDefault();
        const d = e.shiftKey ? 10 : 1;
        const [dx, dy] = {
          ArrowLeft: [-d, 0],
          ArrowRight: [d, 0],
          ArrowUp: [0, -d],
          ArrowDown: [0, d],
        }[e.key] ?? [0, 0];
        st.update((draft) => {
          for (const id of st.selected) {
            const l = findLayer(draft, id);
            if (!l || l.locked) continue;
            if (dx) setProp(l, 'x', num(propValue(l, 'x', st.frame)) + dx, st.frame);
            if (dy) setProp(l, 'y', num(propValue(l, 'y', st.frame)) + dy, st.frame);
          }
        }, 'nudge');
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}

function duplicateSelection() {
  duplicateLayers(useDesigner.getState().selected);
}

// Memoised: the page around it follows the channels on air; the Designer does not show them.
export const Designer = memo(function Designer() {
  const project = useApp((s) => s.project);
  // Not the playhead: the Designer is not drawn again on each frame of the playback (the stage and the timeline follow it).
  const { template, load, update, undo, redo, past, future, selected, select } = useDesigner(
    useShallow((s) => ({ template: s.template, load: s.load, update: s.update, undo: s.undo, redo: s.redo, past: s.past, future: s.future, selected: s.selected, select: s.select })),
  );
  // The top bar's slot for undo, redo and the history.
  const [topSlot, setTopSlot] = useState<HTMLElement | null>(null);
  useEffect(() => setTopSlot(document.getElementById('topbar-slot')), []);
  // The inspector's width, adjustable with the splitter.
  const [layout, setLayout] = useLayout('designer.layout', { right: 380 }, true);
  const [background, setBackground] = useDisplayPref<StageBackground>('designer.background', 'checker');
  // The live video input behind the stage (« video » background).
  const [bgInput, setBgInput] = useDisplayPref<string>('designer.backgroundInput', '');
  useShortcuts();
  const { state: saveState, flush: flushSaves } = useAutosave();
  const lock = useEditLock(project, template?.id);

  // Opening another multiview: the edits of the one before are saved (in it), then its page opens —
  // never on the layers of the one before, even when their ids match (copied multiviews).
  // Back to the same one: what was selected stays.
  const openedFor = useRef<string | null>(null);
  useEffect(() => {
    if (!project) return;
    const page = designTemplates(project)[0] ?? null;
    if (openedFor.current !== project.id) {
      openedFor.current = project.id;
      const same = designedProject === project.id;
      if (!same) void flushSaves();
      designedProject = project.id;
      if (!same || template?.id !== page?.id) load(page);
    } else if (!template && page) load(page);
  }, [project, template, load]);

  if (!project) return <div className="loading">{t('Connecting…')}</div>;

  const add = (type: LayerType, patch?: Record<string, unknown>, name?: string) => {
    const parent = selected.length === 1 && template && findLayer(template, selected[0])?.type === 'group' ? selected[0] : null;
    const size = { width: project.width, height: project.height };
    const made = makeLayer(type, size.width, size.height, parent);
    const l = { ...made, ...patch } as Layer;
    // A widget's own size (the second widget of a type: the Time): centred like the first.
    if (patch?.props) {
      const p = { ...made.props, ...(patch.props as Layer['props']) } as Record<string, unknown>;
      if (!parent) {
        p.x = Math.round((size.width - Number(p.width ?? 0)) / 2);
        p.y = Math.round((size.height - Number(p.height ?? 0)) / 2);
      }
      l.props = p as Layer['props'];
    }
    if (name) l.name = name;
    // In the middle of the picture.
    if (!parent && typeof l.props.width === 'number' && typeof l.props.height === 'number') {
      const w = l.props.width as number;
      const h = l.props.height as number;
      l.props = { ...l.props, x: Math.max(0, Math.round((size.width - w) / 2)), y: Math.max(0, Math.round((size.height - h) / 2)) };
    }
    // Not exactly on a layer already there (the second rectangle on the first): shifted down and right.
    if (template && typeof l.props.x === 'number' && typeof l.props.y === 'number') {
      const taken = (x: number, y: number) => template.layers.some((o) => (o.parent ?? null) === (l.parent ?? null) && num(propValue(o, 'x', 0)) === x && num(propValue(o, 'y', 0)) === y);
      let { x, y } = l.props as { x: number; y: number };
      for (let i = 0; i < 20 && taken(x, y); i++) {
        x += 40;
        y += 40;
      }
      l.props = { ...l.props, x, y };
    }
    // A video starts as a 16:9 box in the middle, until its file is chosen.
    if (l.type === 'image' && l.video) {
      const w = Math.round(size.width * 0.5);
      const h = Math.round((w * 9) / 16);
      l.props = { ...l.props, x: Math.round((size.width - w) / 2), y: Math.round((size.height - h) / 2), width: w, height: h };
    }
    update((t) => {
      t.layers.push(l);
    });
    select([l.id]);
  };

  return (
    <div
      className={`designer ${lock.state.status === 'held' ? 'locked' : ''}`}
      style={{ ['--right-w' as string]: `${layout.right}px` }}
    >
      <main className="center">
        <div className="toolbar">
          {topSlot &&
            createPortal(
              <>
                <button className="top-tool" title={t('Undo (Ctrl+Z)')} disabled={!past.length} onClick={undo}>
                  <Undo2 size={15} />
                </button>
                <button className="top-tool" title={t('Redo (Ctrl+Shift+Z)')} disabled={!future.length} onClick={redo}>
                  <Redo2 size={15} />
                </button>
                <ProjectHistory project={project} readOnly={lock.state.status === 'held'} />
                <TopPublish project={project} saving={saveState === 'pending'} readOnly={lock.state.status === 'held'} />
              </>,
              topSlot,
            )}
          {template && (
            <>
              <LayoutButton project={project} />
              <InsertTools onAdd={add} />
            </>
          )}
          <div className="spacer" />
          <DisplayMenu
            project={project}
            background={background}
            onBackground={setBackground}
            bgInput={bgInput}
            onBgInput={setBgInput}
          />
          <StageZoom />
        </div>
        <div className="stage-holder">
          <Stage project={project} size={{ width: project.width, height: project.height }} background={background} bgInput={bgInput} />
        </div>
      </main>
      <aside className="panel right">
        <Splitter axis="x" className="edge-left" sign={-1} value={layout.right} min={300} max={620} reset={380} onChange={(v) => setLayout('right', v)} />
        <Inspector />
      </aside>
      {lock.state.status === 'held' && <LockOverlay holder={lock.state.holder} template={template?.name ?? ''} onTakeOver={lock.takeOver} />}
    </div>
  );
});

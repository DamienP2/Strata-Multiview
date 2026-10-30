import { applyLayout, LayoutButton, LayoutPicker, PRESETS, type LayoutPreset } from './Layouts';
import { useInputs } from '../engine/liveInputs';
import { Copy, GripVertical, Link2, Plus, Redo2, Trash2, Undo2, X } from 'lucide-react';
import { memo, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Dropdown } from '../ui/Dropdown';
import { newId, num, propValue } from '../engine/evaluate';
import type { Layer, LayerType, Project, Template } from '../engine/types';
import { api, thumbnailUrl, useApp } from '../store';
import { descendants, duplicateLayers, findLayer, makeLayer, newTemplate, setProp, useDesigner } from './designerStore';
import { useShallow } from 'zustand/react/shallow';
import { Inspector } from './Inspector';
import { LockOverlay, TopPublish, useEditLock } from './Collab';
import { Stage, StageZoom } from './Stage';
import { LayerList } from './LayerList';
import { copyLayers, cutLayers, pasteClipboard } from './clipboard';
import { useZoneEdit, ZonesPanel } from './ZoneEditor';
import { LogicEditor } from './LogicEditor';
import { useGuidesView } from './Guides';
import { DisplayMenu, type StageBackground } from './DisplayMenu';
import { ProjectHistory, ProjectPanel } from './ProjectPanel';
import { Splitter, useLayout } from '../ui/Splitter';
import { FormatBar } from './Formats';
import { formatById } from '../engine/formats';
import { zoneArea } from '../engine/zones';
import { designTemplates, publishState } from '../engine/drafts';
import { confirmDialog } from '../ui/dialogs';
import { t } from '../i18n';
import { useDisplayPref } from '../displayPrefs';

/** The project the Designer's open template belongs to (the store outlives the Designer). */
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
      // A full-screen editor (Logic, Data, table) or a dialog is open: its keys are its own (Delete would remove the layer behind it).
      if (e.defaultPrevented || document.querySelector('.logic-editor, .modal-back')) return;
      const st = useDesigner.getState();
      const t = st.template;
      // Esc: out of the safe area editing, off the selected layers, then off the template (overview).
      if (e.key === 'Escape' && useGuidesView.getState().editSafe) return useGuidesView.getState().set({ editSafe: false });
      if (e.key === 'Escape' && t) {
        if (st.selected.length || st.selectedKeys.length) st.set({ selected: [], selectedKeys: [] });
        else st.load(null);
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

function TemplateList({ project }: { project: Project }) {
  // Not the playhead: the list is not drawn again on each frame of the playback.
  const { template, load, format } = useDesigner(useShallow((s) => ({ template: s.template, load: s.load, format: s.format })));
  const saveTemplate = useApp((s) => s.saveTemplate);
  const deleteTemplate = useApp((s) => s.deleteTemplate);
  const toast = useApp((s) => s.toast);
  // « + »: a blank template, or one from a file dropped or picked.
  const [newOpen, setNewOpen] = useState(false);
  const [newName, setNewName] = useState('');
  const [newZone, setNewZone] = useState('');
  const [newLayout, setNewLayout] = useState('2x2');
  const [newGap, setNewGap] = useState(4);
  const [newMargin, setNewMargin] = useState(0);
  const inputs = useInputs();
  const openNew = () => {
    setNewName(t('Multiview {n}', { n: designTemplates(project).length + 1 }));
    setNewZone(defaultZone ?? '');
    setNewOpen(true);
  };
  const createNew = () => {
    setNewOpen(false);
    void create(undefined, newName, newZone || undefined, PRESETS.find((p) => p.id === newLayout));
  };
  // Dragging a template to another place in the list: the one moved, and where it goes.
  const [dragging, setDragging] = useState<string | null>(null);
  const [dropAt, setDropAt] = useState<{ id: string; after: boolean } | null>(null);
  const reorder = (id: string, to: { id: string; after: boolean }) => {
    const ids = designTemplates(project).map((x) => x.id);
    if (id === to.id) return;
    const rest = ids.filter((x) => x !== id);
    rest.splice(rest.indexOf(to.id) + (to.after ? 1 : 0), 0, id);
    if (rest.join() === ids.join()) return;
    // At once here; the server sends the project back.
    useApp.setState((s) => (s.project?.id === project.id ? { project: { ...s.project, designOrder: rest } } : {}));
    api('PUT', `/api/projects/${encodeURIComponent(project.id)}/design-order`, rest).catch((e) => toast((e as Error).message, 'error'));
  };
  const defaultZone = (project.zones.find((z) => z.id === 'full-screen' || /full/i.test(z.name)) ?? project.zones[0])?.id;
  const create = async (base?: Template, name?: string, zone?: string, layout?: LayoutPreset) => {
    const tpl = base ? { ...structuredClone(base), id: newId(), name: t('{name} copy', { name: base.name }) } : newTemplate(name?.trim() || t('Multiview {n}', { n: designTemplates(project).length + 1 }), zone ?? defaultZone, project.frameRate.num / project.frameRate.den, project.templateSecs ?? 1);
    // Its tiles placed on the layout chosen, with the inputs in order.
    if (layout && !base) applyLayout(tpl, layout, { width: project.width, height: project.height }, newGap, newMargin, inputs);
    // Open it at once; the save follows.
    load(tpl);
    await saveTemplate(tpl);
  };
  return (
    // A click beside the templates leaves the open one, like Esc.
    <div className="template-list" onClick={() => template && load(null)}>
      <header onClick={(e) => e.stopPropagation()}>
        <span>{t('Multiviews')}</span>
        <div className="header-actions">
          <button className="opx-add" title={t('New multiview')} onClick={openNew}>
            <Plus size={15} />
          </button>
        </div>
        {newOpen &&
          createPortal(
          <div className="modal-back" onPointerDown={() => setNewOpen(false)}>
            <div className="modal new-tpl" role="dialog" aria-label={t('New multiview')} onPointerDown={(e) => e.stopPropagation()}>
              <header>
                <h2>{t('New multiview')}</h2>
                <button type="button" className="icon-btn ghost" title={t('Close')} onClick={() => setNewOpen(false)}>
                  <X size={16} />
                </button>
              </header>
              <div className="new-tpl-cols">
                <form
                  className="new-tpl-new"
                  onSubmit={(e) => {
                    e.preventDefault();
                    createNew();
                  }}
                >
                  <label className="set-field">
                    <span className="set-label">{t('Name')}</span>
                    <input autoFocus value={newName} onChange={(e) => setNewName(e.target.value)} onFocus={(e) => e.target.select()} />
                  </label>
                  <div className="set-field">
                    <span className="set-label">{t('Zone')}</span>
                    <Dropdown
                      value={newZone}
                      options={project.zones.map((z) => ({ value: z.id, label: z.name || '—', note: `L${z.layer}`, icon: <span className="ntpl-dot" style={{ background: z.color }} /> }))}
                      onChange={setNewZone}
                    />
                  </div>
                  <div className="set-field">
                    <span className="set-label">{t('Layout')}</span>
                    <LayoutPicker value={newLayout} onChange={setNewLayout} gap={newGap} margin={newMargin} onGap={setNewGap} onMargin={setNewMargin} none />
                  </div>
                  <button className="btn primary">
                    <Plus size={14} /> {t('Create')}
                  </button>
                </form>
              </div>
            </div>
          </div>,
            document.body,
          )}
      </header>
      {designTemplates(project).map((tpl) => {
        const { published, draft } = publishState(project, tpl.id);
        const unpublished = draft || !published;
        return (
          <div
            key={tpl.id}
            className={`template-item ${template?.id === tpl.id ? 'selected' : ''} ${dragging === tpl.id ? 'dragging' : ''} ${dropAt?.id === tpl.id && dragging !== tpl.id ? (dropAt.after ? 'drop-after' : 'drop-before') : ''}`}
            draggable
            onDragStart={(e) => {
              e.dataTransfer.effectAllowed = 'move';
              e.dataTransfer.setData('text/template', tpl.id);
              setDragging(tpl.id);
            }}
            onDragOver={(e) => {
              if (!dragging) return;
              e.preventDefault();
              const r = e.currentTarget.getBoundingClientRect();
              const after = e.clientY > r.top + r.height / 2;
              if (dropAt?.id !== tpl.id || dropAt.after !== after) setDropAt({ id: tpl.id, after });
            }}
            onDrop={(e) => {
              e.preventDefault();
              if (dragging && dropAt) reorder(dragging, dropAt);
              setDragging(null);
              setDropAt(null);
            }}
            onDragEnd={() => (setDragging(null), setDropAt(null))}
            onClick={(e) => (e.stopPropagation(), template?.id !== tpl.id && load(tpl))}
          >
            <span className="opx-grip" title={t('Drag to reorder')}>
              <GripVertical size={13} />
            </span>
            <img src={thumbnailUrl(project.id, tpl, format, true)} alt="" loading="lazy" />
            <span className="tpl-text">
              <span className="tpl-name">{tpl.name}</span>
              {(() => {
                const z = project.zones.find((z) => z.id === tpl.zone);
                return z ? (
                  <em className="tpl-zone" style={{ ['--zc' as string]: z.color }}>
                    {z.name}
                  </em>
                ) : null;
              })()}
            </span>
            {unpublished && <span className="tpl-draft" title={published ? t('Changes not on the outputs yet: click Publish at the top') : t('New, not on the outputs yet: click Publish at the top')} />}
            <div className="item-actions">
              <button className="icon-btn ghost" title={t('Duplicate')} onClick={(e) => (e.stopPropagation(), create(tpl))}>
                <Copy size={12} />
              </button>
              <button
                className="icon-btn ghost"
                title={designTemplates(project).length < 2 ? t('A project keeps at least one template') : t('Delete')}
                disabled={designTemplates(project).length < 2}
                onClick={async (e) => {
                  e.stopPropagation();
                  if (!(await confirmDialog({ title: t('Delete the template “{name}”?', { name: tpl.name }), message: t('Rundown items using it are removed too.'), confirm: t('Delete'), danger: true }))) return;
                  await deleteTemplate(tpl.id).catch((err) => toast(String(err), 'error'));
                  if (template?.id === tpl.id) load(designTemplates(project).find((x) => x.id !== tpl.id) ?? null);
                }}
              >
                <Trash2 size={12} />
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
}

const MIN_LEFT = 320;

// Memoised: the page around it follows the channels on air; the Designer does not show them.
export const Designer = memo(function Designer() {
  const project = useApp((s) => s.project);
  // Not the playhead: the Designer is not drawn again on each frame of the playback (the stage and the timeline follow it).
  const { template, load, update, undo, redo, past, future, selected, select, format } = useDesigner(
    useShallow((s) => ({ template: s.template, load: s.load, update: s.update, undo: s.undo, redo: s.redo, past: s.past, future: s.future, selected: s.selected, select: s.select, format: s.format })),
  );
  // The zones edited on the stage (the right panel shows them instead of the inspector).
  const zonesEdit = useZoneEdit((s) => s.on);
  useEffect(() => () => useZoneEdit.getState().stop(), []);
  // The top bar's slot for undo, redo and the history.
  const [topSlot, setTopSlot] = useState<HTMLElement | null>(null);
  useEffect(() => setTopSlot(document.getElementById('topbar-slot')), []);
  const [showZones, setShowZones] = useDisplayPref('designer.zones', true);
  const [logicOpen, setLogicOpen] = useState(false);
  const logicCount = useDesigner((s) => s.template?.logic?.nodes.length ?? 0);
  // Panel sizes, adjustable with the splitters.
  const [layout, setLayout] = useLayout('designer.layout', {
    left: 320,
    right: 380,
  }, true);
  const left = Math.max(MIN_LEFT, layout.left);
  const [background, setBackground] = useDisplayPref<StageBackground>('designer.background', 'checker');
  // The live video input behind the stage (« video » background).
  const [bgInput, setBgInput] = useDisplayPref<string>('designer.backgroundInput', '');
  useShortcuts();
  const { state: saveState, flush: flushSaves } = useAutosave();
  const lock = useEditLock(project, template?.id);

  // Opening another project: the edits of the one before are saved (in it), then it starts afresh —
  // never on the template and layers of the one before, even when their ids match (copied projects).
  // Back to the same project (from the Operator…): the open template stays.
  const openedFor = useRef<string | null>(null);
  useEffect(() => {
    if (!project) return;
    if (openedFor.current !== project.id) {
      openedFor.current = project.id;
      const same = designedProject === project.id;
      if (!same) void flushSaves();
      designedProject = project.id;
      // A project opens on the overview: every zone and template; with a
      // single template (a new project), on that template.
      const all = designTemplates(project);
      if (!same || !template || !all.some((t) => t.id === template.id)) load(all.length === 1 ? all[0] : null);
    }
  }, [project, template, load]);

  // The empty template of a new project, removed by the server at a publish: if it was the open one,
  // the Designer opens another (an open template that is still there keeps its selection).
  const starter = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (!project) return;
    const was = starter.current;
    starter.current = project.starter;
    if (!was || project.starter === was) return;
    const all = designTemplates(project);
    if (useDesigner.getState().base?.id === was && !all.some((x) => x.id === was)) load(all[0] ?? null);
  }, [project, load]);

  if (!project) return <div className="loading">{t('Connecting…')}</div>;

  const add = (type: LayerType, patch?: Record<string, unknown>, name?: string) => {
    const parent = selected.length === 1 && template && findLayer(template, selected[0])?.type === 'group' ? selected[0] : null;
    const size = formatById(project, useDesigner.getState().format);
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
    // In the middle of the template's zone (outside it, the stage greys it out).
    const zone = !parent && template ? project.zones.find((z) => z.id === template.zone) : undefined;
    if (zone && typeof l.props.width === 'number' && typeof l.props.height === 'number') {
      const a = zoneArea(zone, useDesigner.getState().format, size.height > size.width);
      const w = l.props.width as number;
      const h = l.props.height as number;
      const x = Math.round(a.x * size.width + (a.width * size.width - w) / 2);
      const y = Math.round(a.y * size.height + (a.height * size.height - h) / 2);
      l.props = { ...l.props, x: Math.max(0, Math.min(size.width - w, x)), y: Math.max(0, Math.min(size.height - h, y)) };
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
    // Video and Lottie start as a 16:9 box in the middle, until their file is chosen.
    if ((l.type === 'image' && l.video) || (l.type === 'group' && l.lottie !== undefined)) {
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
      style={{
        ['--left-w' as string]: `${left}px`,
        ['--right-w' as string]: `${layout.right}px`,
      }}
    >
      <aside className="panel left">
        <Splitter axis="x" className="edge-right" value={left} min={MIN_LEFT} max={520} reset={320} onChange={(v) => setLayout('left', v)} />
        <ProjectPanel project={project} readOnly={lock.state.status === 'held'} />
        <TemplateList project={project} />
        <LayerList onAdd={add} />
      </aside>
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
          <FormatBar project={project} />
          <div className="spacer" />
          {template && (
            <>
              <LayoutButton project={project} />
              <button className={`btn small ${logicCount ? 'has' : ''}`} onClick={() => setLogicOpen(true)} title={t('Relations between values: a box that follows a text, conditions…')}>
                <Link2 size={13} /> {t('Logic')}
              </button>
            </>
          )}
          <DisplayMenu
            project={project}
            showZones={showZones}
            onShowZones={setShowZones}
            onManageZones={() => project && useZoneEdit.getState().start(project)}
            background={background}
            onBackground={setBackground}
            bgInput={bgInput}
            onBgInput={setBgInput}
          />
          <StageZoom />
        </div>
        <div className="stage-holder">
          <Stage project={project} size={formatById(project, format)} showZones={showZones} background={background} bgInput={bgInput} />
        </div>
      </main>
      <aside className="panel right">
        <Splitter axis="x" className="edge-left" sign={-1} value={layout.right} min={300} max={620} reset={380} onChange={(v) => setLayout('right', v)} />
        {zonesEdit && project ? <ZonesPanel project={project} /> : <Inspector />}
      </aside>
      {lock.state.status === 'held' && <LockOverlay holder={lock.state.holder} template={template?.name ?? ''} onTakeOver={lock.takeOver} />}
      {logicOpen && <LogicEditor onClose={() => setLogicOpen(false)} />}
    </div>
  );
});

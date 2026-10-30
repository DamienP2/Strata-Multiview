import { FormatButtons } from '../designer/MultiviewSettings';
import { ArrowLeft, Check, CheckSquare, ChevronRight, Clock, Copy, Download, Folder, FolderInput, FolderOpen, FolderPlus, Inbox, LayoutGrid, MonitorPlay, MoreHorizontal, PenTool, Pencil, Plus, Radio, Search, Trash2, Upload, X } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { FrameRate, Project, ProjectSummary } from '../engine/types';
import { applyLayout, LayoutPicker, PRESETS } from '../designer/Layouts';
import { useInputs } from '../engine/liveInputs';
import { api, can, useApp } from '../store';
import { useShallow } from 'zustand/react/shallow';
import { navigate, projectPath } from '../router';
import { confirmDialog, promptDialog } from '../ui/dialogs';
import { locale, t, tn } from '../i18n';
import { NavItem, NavSection } from './SidePage';
import { useMetrics, type ChannelMetrics } from '../engine/metrics';
import { Bar } from '../perf/bits';
import { fps as fpsText, fpsTone, pct, slowest, worstTone } from '../perf/format';

const fps = (r: FrameRate) => +(r.num / r.den).toFixed(2);

function NewProject({ from, folder, onClose }: { from?: ProjectSummary; folder?: string; onClose: () => void }) {
  const toast = useApp((s) => s.toast);
  const [name, setName] = useState(from ? t('{name} copy', { name: from.name }) : '');
  const [description, setDescription] = useState(from?.description ?? '');
  const [size, setSize] = useState({ width: 1920, height: 1080 });
  const [rate, setRate] = useState<FrameRate>({ num: 50, den: 1 });
  const [layout, setLayout] = useState('2x2');
  const [gap, setGap] = useState(4);
  const [margin, setMargin] = useState(0);
  const inputs = useInputs();
  const [busy, setBusy] = useState(false);
  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const p = await api<ProjectSummary>(
        'POST',
        '/api/projects',
        from
          ? { name, description, from: from.id }
          : {
              name,
              description,
              width: size.width,
              height: size.height,
              frameRate: rate,
            },
      );
      // Its tiles placed on the layout chosen, with the inputs in order, published at once.
      const preset = PRESETS.find((x) => x.id === layout);
      if (!from && preset) {
        const full = await api<Project>('GET', `/api/projects/${encodeURIComponent(p.id)}`);
        const page = full.templates[0];
        if (page) {
          applyLayout(page, preset, size, gap, margin, inputs);
          await api('PUT', `/api/projects/${encodeURIComponent(p.id)}/templates/${encodeURIComponent(page.id)}?publish=true`, page);
        }
      }
      onClose();
      if (folder) {
        const f = useApp.getState().folders;
        await useApp.getState().saveFolders({ ...f, assign: { ...f.assign, [p.id]: folder } });
      }
      // A new multiview opens in its own tab (a copy stays in the list).
      if (!from) openInTab(projectPath(p.id));
    } catch (err) {
      toast((err as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="modal-back" onPointerDown={onClose}>
      <form className={`modal ${from ? '' : 'new-mv'}`} onSubmit={create} onPointerDown={(e) => e.stopPropagation()}>
        <header>
          <h2>{from ? t('Duplicate “{name}”', { name: from.name }) : t('New multiview')}</h2>
          <button type="button" className="icon-btn ghost" onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <label className="set-field">
          <span className="set-label">{t('Name')}</span>
          <input autoFocus value={name} placeholder={t('Control room A, Studio 2…')} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="set-field">
          <span className="set-label">{t('Description')}</span>
          <input value={description} onChange={(e) => setDescription(e.target.value)} />
        </label>
        {!from && (
          <div className="set-field">
            <span className="set-label">{t('Resolution')}</span>
            <FormatButtons width={size.width} height={size.height} rate={rate} onSize={(width, height) => setSize({ width, height })} onRate={setRate} />
          </div>
        )}
        {!from && (
          <div className="set-field">
            <span className="set-label">{t('Layout')}</span>
            <LayoutPicker value={layout} onChange={setLayout} gap={gap} margin={margin} onGap={setGap} onMargin={setMargin} none />
          </div>
        )}
        <footer>
          <button type="button" className="btn" onClick={onClose}>
            {t('Cancel')}
          </button>
          <button className="btn primary" disabled={busy || !name.trim()}>
            <Plus size={14} /> {from ? t('Duplicate') : t('Create')}
          </button>
        </footer>
      </form>
    </div>
  );
}

type View = { kind: 'all' } | { kind: 'unfiled' } | { kind: 'folder'; name: string };
type Sort = 'recent' | 'name' | 'tiles';

/** "Modified 5 min ago", "Modified yesterday"… */
function modifiedAgo(ms: number): string {
  const s = Math.max(0, (Date.now() - ms) / 1000);
  if (s < 60) return t('Modified just now');
  if (s < 3600) return t('Modified {n} min ago', { n: Math.floor(s / 60) });
  if (s < 86400) return t('Modified {n} h ago', { n: Math.floor(s / 3600) });
  const days = Math.floor(s / 86400);
  if (days < 2) return t('Modified yesterday');
  if (days < 14) return t('Modified {n} days ago', { n: days });
  if (days < 60) return t('Modified {n} weeks ago', { n: Math.floor(days / 7) });
  return tn(Math.floor(days / 30), 'Modified {n} month ago', 'Modified {n} months ago');
}

/** A thumbnail, with a skeleton until it has loaded. */
function Thumb({ src, style }: { src: string; style?: React.CSSProperties }) {
  // The address loaded (a new one shows the skeleton again).
  const [loaded, setLoaded] = useState<string | null>(null);
  const ok = loaded === src;
  return (
    <>
      <img src={src} alt="" loading="lazy" draggable={false} style={style} ref={(el) => void (el?.complete && el.naturalWidth && !ok && setLoaded(src))} onLoad={() => setLoaded(src)} onError={() => setLoaded(src)} />
      {!ok && <span className="ux3-skel" aria-hidden />}
    </>
  );
}

/** A project opens in its own tab: the list stays open in this one. */
function openInTab(path: string) {
  if (!window.open(path, '_blank')) navigate(path);
}

/** A card's "…" menu: opens upward when there is no room below. */
function CardMenu({ children, page }: { children: React.ReactNode; page: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const [up, setUp] = useState(false);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const below = el.getBoundingClientRect();
    const room = (el.closest('.library')?.getBoundingClientRect().bottom ?? window.innerHeight) - 8;
    if (!up && below.bottom > room) setUp(true);
  }, [page, up]);
  return (
    <div ref={ref} className={`menu project-menu ux3-card-menu ${up ? 'ux3-up' : ''}`}>
      {children}
    </div>
  );
}

export function Projects() {
  // Not the channels' live state: the list is not drawn again on each frame on air.
  const { projects, channels, auth, toast, folders, saveFolders } = useApp(useShallow((s) => ({ projects: s.projects, channels: s.channels, auth: s.auth, toast: s.toast, folders: s.folders, saveFolders: s.saveFolders })));
  const metrics = useMetrics();
  const user = auth?.user;
  const design = can.design(user);
  const [dialog, setDialog] = useState<{ from?: ProjectSummary } | null>(null);
  const [view, setView] = useState<View>({ kind: 'all' });
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<Sort>('recent');
  const [dropOn, setDropOn] = useState<string | null>(null);
  const [menu, setMenu] = useState<string | null>(null);
  // The card menu shows the folders to move the project to.
  const [menuMove, setMenuMove] = useState(false);
  // Projects picked for an action on all of them (delete, move); picking
  // starts with none (the Select button), or with the ones clicked.
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [picking, setPicking] = useState(false);
  const [anchor, setAnchor] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setMenuMove(false);
    if (!menu) return;
    const close = () => setMenu(null);
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.preventDefault(), setMenu(null));
    window.addEventListener('pointerdown', close);
    window.addEventListener('keydown', esc);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('keydown', esc);
    };
  }, [menu]);
  // A folder deleted elsewhere.
  useEffect(() => {
    if (view.kind === 'folder' && !folders.folders.includes(view.name)) setView({ kind: 'all' });
  }, [folders, view]);
  // Projects deleted meanwhile leave the selection.
  useEffect(() => {
    setSelected((sel) => {
      const ids = new Set(projects.map((p) => p.id));
      return [...sel].every((id) => ids.has(id)) ? sel : new Set([...sel].filter((id) => ids.has(id)));
    });
  }, [projects]);

  const folderOf = (id: string) => (folders.folders.includes(folders.assign[id]) ? folders.assign[id] : null);
  const inView = (p: ProjectSummary) => (view.kind === 'all' ? true : view.kind === 'unfiled' ? !folderOf(p.id) : folderOf(p.id) === view.name);
  const q = query.trim().toLowerCase();
  const shown = projects
    .filter((p) => inView(p) && (!q || p.name.toLowerCase().includes(q) || p.description.toLowerCase().includes(q)))
    .sort((a, b) => (sort === 'tiles' ? b.tiles - a.tiles : sort === 'recent' ? (b.modifiedAt ?? 0) - (a.modifiedAt ?? 0) : 0) || a.name.localeCompare(b.name));
  const count = (f: string | null) => projects.filter((p) => folderOf(p.id) === f).length;
  const picked = projects.filter((p) => selected.has(p.id));
  const selecting = picking || selected.size > 0;
  /** Leaves the selection mode. */
  const endPick = () => {
    setSelected(new Set());
    setPicking(false);
  };

  // Esc empties the selection, Ctrl/Cmd+A takes every project shown.
  useEffect(() => {
    if (!design) return;
    const key = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && e.target.closest('input, textarea, select, [contenteditable]');
      if (typing || document.querySelector('.modal-back')) return;
      if (e.key === 'Escape' && selecting && !e.defaultPrevented) endPick();
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        e.preventDefault();
        setPicking(true);
        setSelected(new Set(shown.map((p) => p.id)));
      }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  });

  /** Click on a project's box: one more, one less, or (Shift) all the ones between. */
  const toggle = (id: string, range: boolean) => {
    const next = new Set(selected);
    if (range && anchor && anchor !== id) {
      const ids = shown.map((p) => p.id);
      const [a, b] = [ids.indexOf(anchor), ids.indexOf(id)].sort((x, y) => x - y);
      if (a >= 0) for (const x of ids.slice(a, b + 1)) next.add(x);
      else next.add(id);
    } else if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
    setAnchor(id);
  };

  const moveAll = (ids: string[], folder: string | null, list = folders.folders) => {
    const assign = { ...folders.assign };
    for (const id of ids) {
      if (folder) assign[id] = folder;
      else delete assign[id];
    }
    void saveFolders({ folders: list, assign });
  };
  const moveTo = (id: string, folder: string | null) => moveAll([id], folder);
  const askFolder = () => promptDialog({ title: t('New folder'), label: t('Name'), placeholder: t('Sport, News, Elections…'), confirm: t('Create') });
  const newFolder = async () => {
    const name = await askFolder();
    if (!name) return;
    if (folders.folders.includes(name)) return setView({ kind: 'folder', name });
    void saveFolders({
      ...folders,
      folders: [...folders.folders, name].sort((a, b) => a.localeCompare(b)),
    });
    setView({ kind: 'folder', name });
  };
  /** A new folder with the selected projects in it. */
  const newFolderWithSelection = async () => {
    const name = await promptDialog({
      title: tn(selected.size, 'New folder for {n} project', 'New folder for {n} projects'),
      label: t('Name'),
      placeholder: t('Sport, News, Elections…'),
      confirm: t('Create and move'),
    });
    if (!name) return;
    const list = folders.folders.includes(name) ? folders.folders : [...folders.folders, name].sort((a, b) => a.localeCompare(b));
    moveAll([...selected], name, list);
    toast(tn(selected.size, '{n} project moved to “{folder}”', '{n} projects moved to “{folder}”', { folder: name }), 'info');
    endPick();
    setView({ kind: 'folder', name });
  };
  const moveSelection = (folder: string | null) => {
    moveAll([...selected], folder);
    toast(folder ? tn(selected.size, '{n} project moved to “{folder}”', '{n} projects moved to “{folder}”', { folder }) : tn(selected.size, '{n} project moved to the top level', '{n} projects moved to the top level'), 'info');
    endPick();
    setMenu(null);
  };
  const renameFolder = async (old: string) => {
    const name = await promptDialog({ title: t('Rename the folder'), label: t('Name'), value: old, confirm: t('Rename') });
    if (!name || name === old) return;
    if (folders.folders.includes(name)) return toast(t('A folder “{name}” already exists', { name }), 'error');
    const assign = Object.fromEntries(Object.entries(folders.assign).map(([p, f]) => [p, f === old ? name : f]));
    void saveFolders({
      folders: folders.folders.map((f) => (f === old ? name : f)).sort((a, b) => a.localeCompare(b)),
      assign,
    });
    setView({ kind: 'folder', name });
  };
  const deleteFolder = async (name: string) => {
    const n = count(name);
    if (
      !(await confirmDialog({
        title: t('Delete the folder “{name}”?', { name }),
        message: n ? tn(n, 'Its project goes back to the top level (it is not deleted).', 'Its {n} projects go back to the top level (they are not deleted).') : undefined,
        confirm: t('Delete'),
        danger: true,
      }))
    )
      return;
    const assign = Object.fromEntries(Object.entries(folders.assign).filter(([, f]) => f !== name));
    void saveFolders({
      folders: folders.folders.filter((f) => f !== name),
      assign,
    });
    setView({ kind: 'all' });
  };

  const importRef = useRef<HTMLInputElement>(null);
  const importFile = async (file: File) => {
    try {
      const p = await api<ProjectSummary>('POST', '/api/projects/import', JSON.parse(await file.text()));
      if (view.kind === 'folder') moveTo(p.id, view.name);
      toast(t('Imported “{name}”', { name: p.name }));
    } catch (e) {
      toast(t('Import failed: {error}', { error: (e as Error).message }), 'error');
    }
  };
  /** Deletes projects after one question (with the channels they are loaded on). */
  const removeAll = async (list: ProjectSummary[]) => {
    if (!list.length) return;
    const used = channels.filter((c) => list.some((p) => p.id === c.projectId)).map((c) => c.name);
    const warn = used.length ? ` ${t('Loaded on {channels}: they will be unloaded.', { channels: used.join(', ') })}` : '';
    if (
      !(await confirmDialog({
        title: list.length === 1 ? t('Delete the multiview “{name}”?', { name: list[0].name }) : tn(list.length, 'Delete {n} multiview?', 'Delete {n} multiviews?'),
        message: (
          <>
            {list.length > 1 && (
              <span className="dialog-list">
                {list.slice(0, 8).map((p) => (
                  <span key={p.id}>{p.name}</span>
                ))}
                {list.length > 8 && <span className="muted">{t('and {n} more', { n: list.length - 8 })}</span>}
              </span>
            )}
            {list.length === 1 ? t('Its media are deleted too.') : t('Their media are deleted too.')}{' '}
            {t('This cannot be undone.')}
            {warn}
          </>
        ),
        confirm: t('Delete'),
        danger: true,
      }))
    )
      return;
    setBusy(true);
    let failed = 0;
    for (const p of list) {
      await api('DELETE', `/api/projects/${encodeURIComponent(p.id)}`).catch((e) => {
        failed++;
        toast(`${p.name}: ${(e as Error).message}`, 'error');
      });
    }
    setBusy(false);
    endPick();
    if (list.length > 1) toast(tn(list.length - failed, '{n} project deleted', '{n} projects deleted'), 'info');
  };

  // Drop target: a folder (name), or the top level (null). A selected project carries the whole selection.
  const drop = (target: string | null) => ({
    onDragOver: (e: React.DragEvent) => {
      if (!design || !e.dataTransfer.types.includes('application/x-strata-project')) return;
      e.preventDefault();
      setDropOn(target ?? '');
    },
    onDragLeave: () => setDropOn(null),
    onDrop: (e: React.DragEvent) => {
      const ids = e.dataTransfer.getData('application/x-strata-project').split('\n').filter(Boolean);
      setDropOn(null);
      if (!ids.length) return;
      moveAll(ids, target);
      if (ids.length > 1) endPick();
    },
  });

  const title = view.kind === 'all' ? t('All multiviews') : view.kind === 'unfiled' ? t('Not in a folder') : view.name;
  const navItem = (key: string, v: View, icon: React.ReactNode, label: string, n: number, target?: string | null, actions?: React.ReactNode) => {
    const on = view.kind === v.kind && (v.kind !== 'folder' || (view.kind === 'folder' && view.name === v.name));
    return (
      <NavItem
        key={key}
        on={on}
        className={target !== undefined && dropOn === (target ?? '') ? 'drop' : ''}
        icon={icon}
        label={label}
        count={n}
        actions={actions}
        onClick={() => setView(v)}
        {...(target !== undefined ? drop(target) : {})}
      />
    );
  };

  return (
    <div className="projects-page">
      <aside className="lib-nav">
        <NavSection>{t('Library')}</NavSection>
        {navItem('all', { kind: 'all' }, <LayoutGrid size={15} />, t('All multiviews'), projects.length)}
        {navItem('unfiled', { kind: 'unfiled' }, <Inbox size={15} />, t('Not in a folder'), count(null), null)}
        <NavSection
          actions={
            design && (
              <button className="icon-btn ghost" title={t('New folder')} onClick={newFolder}>
                <FolderPlus size={14} />
              </button>
            )
          }
        >
          {t('Folders')}
        </NavSection>
        {folders.folders.map((f) =>
          navItem(
            `f-${f}`,
            { kind: 'folder', name: f },
            view.kind === 'folder' && view.name === f ? <FolderOpen size={15} /> : <Folder size={15} />,
            f,
            count(f),
            f,
            design && (
              <span className="lib-nav-actions">
                <button className="icon-btn ghost" title={t('Rename')} onClick={(e) => (e.stopPropagation(), renameFolder(f))}>
                  <Pencil size={12} />
                </button>
                <button className="icon-btn ghost" title={t('Delete the folder')} onClick={(e) => (e.stopPropagation(), deleteFolder(f))}>
                  <Trash2 size={12} />
                </button>
              </span>
            ),
          ),
        )}
        {!folders.folders.length && <p className="lib-nav-hint">{design ? t('Create folders to sort your multiviews, then drag them onto the folders.') : t('No folder.')}</p>}
      </aside>

      <main className="library">
        <div className="library-inner wide">
          <div className="settings-head">
            <div>
              <h1>{title}</h1>
              <p className="muted">{tn(shown.length, '{n} multiview', '{n} multiviews')}</p>
            </div>
            <div className="spacer" />
            {design && (
              <>
                <button className="btn" title={t('A multiview exported from Strata Multiview (.json)')} onClick={() => importRef.current?.click()}>
                  <Upload size={14} /> {t('Import')}
                </button>
                <input
                  ref={importRef}
                  type="file"
                  accept=".json"
                  hidden
                  onChange={(e) => {
                    const f = e.target.files?.[0];
                    e.target.value = '';
                    if (f) void importFile(f);
                  }}
                />
                <button className="btn primary" onClick={() => setDialog({})}>
                  <Plus size={14} /> {t('New multiview')}
                </button>
              </>
            )}
          </div>

          {selecting ? (
            <div className="bulk-bar" onPointerDown={(e) => e.stopPropagation()}>
              <button className="icon-btn ghost" title={t('Clear the selection (Esc)')} onClick={endPick}>
                <X size={15} />
              </button>
              <strong>{tn(selected.size, '{n} project selected', '{n} projects selected')}</strong>
              {selected.size < shown.length && (
                <button className="btn small ghost" onClick={() => setSelected(new Set(shown.map((p) => p.id)))}>
                  <CheckSquare size={13} /> {t('Select all ({n})', { n: shown.length })}
                </button>
              )}
              {selected.size === 0 && <span className="ux3-pick-hint">{t('Click the multiviews to select')}</span>}
              <div className="spacer" />
              <div className="menu-wrap">
                <button className="btn small" disabled={busy || !selected.size} onClick={() => setMenu(menu === '@bulk' ? null : '@bulk')}>
                  <FolderInput size={13} /> {t('Move to')}
                </button>
                {menu === '@bulk' && (
                  <div className="menu project-menu right">
                    <button onClick={() => moveSelection(null)}>
                      <Inbox size={13} /> {t('Top level')}
                    </button>
                    {folders.folders.map((f) => (
                      <button key={f} onClick={() => moveSelection(f)}>
                        <Folder size={13} /> {f}
                      </button>
                    ))}
                    <div className="menu-sep" />
                    <button onClick={() => (setMenu(null), void newFolderWithSelection())}>
                      <FolderPlus size={13} /> {t('New folder…')}
                    </button>
                  </div>
                )}
              </div>
              {selected.size > 0 && (
                <button className="btn small" disabled={busy} onClick={() => void newFolderWithSelection()}>
                  <FolderPlus size={13} /> {t('New folder with them')}
                </button>
              )}
              <button className="btn small danger" disabled={busy || !selected.size} onClick={() => void removeAll(picked)}>
                <Trash2 size={13} /> {busy ? t('Deleting…') : t('Delete')}
              </button>
            </div>
          ) : (
            <div className="media-toolbar">
              <label className="search">
                <Search size={14} />
                <input value={query} placeholder={t('Search multiviews')} onChange={(e) => setQuery(e.target.value)} />
              </label>
              <div className="spacer" />
              {design && shown.length > 0 && (
                <button className="btn small ghost" title={t('Pick multiviews to delete or move them together (Ctrl+A: all)')} onClick={() => setPicking(true)}>
                  <CheckSquare size={13} /> {t('Select')}
                </button>
              )}
              <span className="sort">
                {t('Sort')}
                <span className="seg ux3-sort" role="radiogroup" aria-label={t('Sort')}>
                  <button role="radio" aria-checked={sort === 'recent'} className={sort === 'recent' ? 'on' : ''} onClick={() => setSort('recent')}>
                    <Clock size={13} /> {t('Recent')}
                  </button>
                  <button role="radio" aria-checked={sort === 'name'} className={sort === 'name' ? 'on' : ''} onClick={() => setSort('name')}>
                    {t('Name')}
                  </button>
                  <button role="radio" aria-checked={sort === 'tiles'} className={sort === 'tiles' ? 'on' : ''} onClick={() => setSort('tiles')}>
                    {t('Tiles')}
                  </button>
                </span>
              </span>
            </div>
          )}

          <div className={`project-grid ${selecting ? 'selecting' : ''}`}>
            {shown.map((p) => {
              const on = channels.filter((c) => c.projectId === p.id);
              const folder = folderOf(p.id);
              const isOn = selected.has(p.id);
              const ratio = p.width / p.height;
              return (
                <div
                  key={p.id}
                  className={`project-card ${isOn ? 'picked' : ''} ${menu === p.id ? 'ux3-menu-open' : ''}`}
                  draggable={design}
                  onDragStart={(e) => {
                    const ids = isOn ? [...selected] : [p.id];
                    e.dataTransfer.setData('application/x-strata-project', ids.join('\n'));
                    e.dataTransfer.effectAllowed = 'move';
                  }}
                >
                  <a
                    className="project-cover"
                    href={projectPath(p.id)}
                    target="_blank"
                    rel="noopener"
                    draggable={false}
                    style={{ aspectRatio: String(Math.max(16 / 9, ratio)) }}
                    onClick={(e) => {
                      // While picking, a click on a project picks it.
                      if (!selecting) return;
                      e.preventDefault();
                      toggle(p.id, e.shiftKey);
                    }}
                  >
                    {p.cover ? (
                      <Thumb src={`/api/projects/${encodeURIComponent(p.id)}/templates/${encodeURIComponent(p.cover)}/thumbnail.png?w=640`} style={{ aspectRatio: String(ratio) }} />
                    ) : (
                      <span className="project-cover-empty">{t('Empty')}</span>
                    )}
                    <span className="project-badge">
                      <span className="ratio" style={ratio >= 1 ? { width: 14, height: 14 / ratio } : { width: 14 * ratio, height: 14 }} />
                      {p.width}×{p.height} · {fps(p.frameRate)}p
                    </span>
                    {metrics && metrics.channels.some((c) => c.projectId === p.id) ? (
                      <ProjectAir chans={metrics.channels.filter((c) => c.projectId === p.id)} />
                    ) : (
                      on.length > 0 && (
                        <span className="project-channel" title={t('Loaded on')}>
                          <MonitorPlay size={11} /> {on.map((c) => c.name).join(', ')}
                        </span>
                      )
                    )}
                  </a>
                  {design && (
                    <button
                      className={`project-check ${isOn ? 'on' : ''}`}
                      title={isOn ? t('Unselect') : t('Select (Shift: all the multiviews between)')}
                      aria-pressed={isOn}
                      onClick={(e) => toggle(p.id, e.shiftKey)}
                    >
                      {isOn && <Check size={13} strokeWidth={3} />}
                    </button>
                  )}
                  <div className="project-body">
                    <div className="project-head">
                      <strong>{p.name}</strong>
                      <div className="menu-wrap" onPointerDown={(e) => e.stopPropagation()}>
                        <button className={`icon-btn ghost ${menu === p.id ? 'on' : ''}`} title={t('More')} aria-expanded={menu === p.id} onClick={() => setMenu(menu === p.id ? null : p.id)}>
                          <MoreHorizontal size={16} />
                        </button>
                        {menu === p.id &&
                          (menuMove ? (
                            <CardMenu page={1}>
                              <button onClick={() => setMenuMove(false)}>
                                <ArrowLeft size={13} /> {t('Move to')}
                              </button>
                              <div className="menu-sep" />
                              <button disabled={!folder} onClick={() => (moveTo(p.id, null), setMenu(null))}>
                                <Inbox size={13} /> {t('Top level')}
                              </button>
                              {folders.folders.map((f) => (
                                <button key={f} disabled={folder === f} onClick={() => (moveTo(p.id, f), setMenu(null))}>
                                  <Folder size={13} /> {f}
                                </button>
                              ))}
                            </CardMenu>
                          ) : (
                            <CardMenu page={0}>
                              <a href={`/api/projects/${encodeURIComponent(p.id)}/export`} onClick={() => setMenu(null)}>
                                <Download size={13} /> {t('Export')}
                              </a>
                              {design && (
                                <>
                                  <button onClick={() => (setDialog({ from: p }), setMenu(null))}>
                                    <Copy size={13} /> {t('Duplicate')}
                                  </button>
                                  <button className="ux3-sub" onClick={() => setMenuMove(true)}>
                                    <FolderInput size={13} /> {t('Move…')}
                                    <ChevronRight size={13} className="ux3-sub-chev" />
                                  </button>
                                  <div className="menu-sep" />
                                  <button className="danger" onClick={() => (void removeAll([p]), setMenu(null))}>
                                    <Trash2 size={13} /> {t('Delete')}
                                  </button>
                                </>
                              )}
                            </CardMenu>
                          ))}
                      </div>
                    </div>
                    <p className="project-desc">{p.description || <span className="muted">{t('No description')}</span>}</p>
                    <div className="project-meta">
                      <span>
                        <MonitorPlay size={12} /> {tn(p.tiles, '{n} tile', '{n} tiles')}
                      </span>
                      {p.modifiedAt !== undefined && (
                        <span title={new Date(p.modifiedAt).toLocaleString(locale())}>
                          <Clock size={12} /> {modifiedAgo(p.modifiedAt)}
                        </span>
                      )}
                      {folder && view.kind !== 'folder' && (
                        <span>
                          <Folder size={12} /> {folder}
                        </span>
                      )}
                    </div>
                  </div>
                  {design && (
                    <a className="project-links project-link" href={projectPath(p.id)} target="_blank" rel="noopener">
                      <PenTool size={15} />
                      <span>{t('Edit')}</span>
                    </a>
                  )}
                </div>
              );
            })}
            {shown.length === 0 && (
              <div className="empty big">
                <Radio size={22} />
                <p>{q ? t('No multiview matches.') : t('No multiview here.')}</p>
              </div>
            )}
          </div>
        </div>
      </main>
      {dialog && <NewProject from={dialog.from} folder={view.kind === 'folder' ? view.name : undefined} onClose={() => setDialog(null)} />}
    </div>
  );
}

/**
 * The channels playing a project, over its cover: on air (or only loaded),
 * the frame rate of its slowest picture and the highest render load.
 */
function ProjectAir({ chans }: { chans: ChannelMetrics[] }) {
  const onAir = chans.reduce((n, c) => n + c.onAir, 0);
  const load = Math.max(0, ...chans.map((c) => c.load));
  const slow = slowest(chans);
  const loadTone = load >= 85 ? 'bad' : load >= 60 ? 'warn' : 'ok';
  const rateTone = slow ? fpsTone(slow.fps, slow.rate) : 'idle';
  const tone = worstTone(loadTone, rateTone);
  return (
    <span
      className={`project-air tone-${tone} ${onAir ? 'live' : ''}`}
      style={{ ['--load-k' as string]: Math.min(100, load) / 100 }}
      title={t('Frame rate of the slowest picture · render time, % of the frame time')}
    >
      <span className="project-air-state">{onAir ? t('On air') : t('Loaded')}</span>
      <span className="project-air-names">
        <MonitorPlay size={11} />
        <span>{chans.map((c) => c.name).join(', ')}</span>
      </span>
      {slow && (
        <span className={`project-air-fps tone-${rateTone}`}>
          {fpsText(slow.fps)}/{fpsText(slow.rate)} fps
        </span>
      )}
      <span className={`project-air-load tone-${loadTone}`}>
        <Bar value={load} tone={loadTone} />
        {pct(load)}
      </span>
    </span>
  );
}

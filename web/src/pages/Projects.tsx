import { FormatButtons } from '../designer/ProjectPanel';
import { ArrowLeft, Check, CheckSquare, ChevronRight, Clock, Copy, Database, GraduationCap, Download, Folder, FolderInput, FolderOpen, FolderPlus, Library as LibraryIcon, Inbox, Layers, LayoutGrid, MonitorPlay, MoreHorizontal, PenTool, Pencil, Plus, Radio, Search, Trash2, Upload, X } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { FrameRate, ProjectSummary } from '../engine/types';
import { api, can, useApp } from '../store';
import { useShallow } from 'zustand/react/shallow';
import { navigate, projectPath, type ProjectPage } from '../router';
import { confirmDialog, promptDialog } from '../ui/dialogs';
import { lang, locale, t, tn } from '../i18n';
import { NavItem, NavSection } from './SidePage';
import { ImportModal } from './ImportModal';
import { useMetrics, type ChannelMetrics } from '../engine/metrics';
import { Bar } from '../perf/bits';
import { fps as fpsText, fpsTone, pct, slowest, worstTone } from '../perf/format';

export const FORMATS: {
  label: string;
  width: number;
  height: number;
  rate: FrameRate;
}[] = [
  { label: '1080p 50', width: 1920, height: 1080, rate: { num: 50, den: 1 } },
  { label: '1080p 25', width: 1920, height: 1080, rate: { num: 25, den: 1 } },
  {
    label: '1080p 59.94',
    width: 1920,
    height: 1080,
    rate: { num: 60000, den: 1001 },
  },
  {
    label: '1080p 29.97',
    width: 1920,
    height: 1080,
    rate: { num: 30000, den: 1001 },
  },
  { label: '720p 50', width: 1280, height: 720, rate: { num: 50, den: 1 } },
  {
    label: '2160p 50 (UHD)',
    width: 3840,
    height: 2160,
    rate: { num: 50, den: 1 },
  },
  {
    label: 'Vertical 1080×1920 30',
    width: 1080,
    height: 1920,
    rate: { num: 30, den: 1 },
  },
  {
    label: 'Square 1080×1080 30',
    width: 1080,
    height: 1080,
    rate: { num: 30, den: 1 },
  },
];

const fps = (r: FrameRate) => +(r.num / r.den).toFixed(2);

function NewProject({ from, folder, onClose }: { from?: ProjectSummary; folder?: string; onClose: () => void }) {
  const toast = useApp((s) => s.toast);
  const [name, setName] = useState(from ? t('{name} copy', { name: from.name }) : '');
  const [description, setDescription] = useState(from?.description ?? '');
  const [size, setSize] = useState({ width: 1920, height: 1080 });
  const [rate, setRate] = useState<FrameRate>({ num: 50, den: 1 });
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
      onClose();
      if (folder) {
        const f = useApp.getState().folders;
        await useApp.getState().saveFolders({ ...f, assign: { ...f.assign, [p.id]: folder } });
      }
      // A new project opens in its own tab (a copy stays in the list).
      if (!from) openInTab(projectPath(p.id));
    } catch (err) {
      toast((err as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="modal-back" onPointerDown={onClose}>
      <form className="modal" onSubmit={create} onPointerDown={(e) => e.stopPropagation()}>
        <header>
          <h2>{from ? t('Duplicate “{name}”', { name: from.name }) : t('New project')}</h2>
          <button type="button" className="icon-btn ghost" onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <label className="set-field">
          <span className="set-label">{t('Name')}</span>
          <input autoFocus value={name} placeholder={t('Evening news, Match day…')} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="set-field">
          <span className="set-label">{t('Description')}</span>
          <input value={description} onChange={(e) => setDescription(e.target.value)} />
        </label>
        {!from && (
          <div className="set-field">
            <span className="set-label">{t('Design format')}</span>
            <FormatButtons width={size.width} height={size.height} rate={rate} onSize={(width, height) => setSize({ width, height })} onRate={setRate} />
            <span className="set-hint">{t('Channels in another format scale the graphics to fit.')}</span>
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
type Sort = 'recent' | 'name' | 'templates';

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

interface ExampleInfo {
  id: string;
  name: string;
  category: string;
  tags: string[];
  description: string;
  summary?: string;
  en?: { name: string; description: string; summary?: string };
  templates: string[];
  formats: string[];
}

/** The example projects: a copy of one becomes a project of the user. */
function ExampleBank({ onClose }: { onClose: () => void }) {
  const toast = useApp((s) => s.toast);
  const [examples, setExamples] = useState<ExampleInfo[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const french = lang().startsWith('fr');
  useEffect(() => {
    api<ExampleInfo[]>('GET', '/api/examples')
      .then(setExamples)
      .catch((e) => toast((e as Error).message, 'error'));
  }, []);
  const create = async (d: ExampleInfo) => {
    setBusy(d.id);
    try {
      const p = await api<ProjectSummary>('POST', `/api/examples/${encodeURIComponent(d.id)}`);
      toast(t('“{name}” added to your projects', { name: p.name }), 'info');
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(null);
    }
  };
  return (
    <div className="modal-back" onPointerDown={onClose}>
      <div className="modal wide example-bank" onPointerDown={(e) => e.stopPropagation()}>
        <header>
          <GraduationCap size={18} />
          <h2>{t('Examples')}</h2>
          <div className="spacer" />
          <kbd className="ux3-esc">{t('Esc')}</kbd>
          <button className="icon-btn ghost" title={t('Close (Esc)')} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="example-grid ux3-examples">
          {examples === null && <p className="muted">{t('Loading…')}</p>}
          {examples?.map((d) => {
            const name = (!french && d.en?.name) || d.name;
            const description = (!french && d.en?.description) || d.description;
            const summary = (!french && d.en?.summary) || d.summary || description;
            return (
              <div key={d.id} className="example-card">
                <div className="example-cover">
                  <Thumb src={`/api/examples/${encodeURIComponent(d.id)}/cover.png?w=640`} />
                  <span className="example-num">{d.id.match(/^ex-(\d+)/)?.[1] ?? ''}</span>
                </div>
                <div className="example-body">
                  <div className="example-top">
                    <strong>{name.replace(/^\d+ · /, '')}</strong>
                    {d.category && <span className="example-cat">{d.category}</span>}
                  </div>
                  <p title={description}>{summary}</p>
                  <div className="example-tags" title={[...d.formats, ...d.tags].join(' · ')}>
                    <span>{tn(d.templates.length, '{n} template', '{n} templates')}</span>
                    {[...d.formats, ...d.tags].slice(0, 1).map((x) => (
                      <span key={x}>{x}</span>
                    ))}
                  </div>
                  <button className="btn primary small" disabled={busy !== null} onClick={() => create(d)}>
                    <Plus size={13} /> {busy === d.id ? t('Adding…') : t('Add to my projects')}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>
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
  const [examples, setExamples] = useState(false);
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
    .sort((a, b) => (sort === 'templates' ? b.templates - a.templates : sort === 'recent' ? (b.modifiedAt ?? 0) - (a.modifiedAt ?? 0) : 0) || a.name.localeCompare(b.name));
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

  const [importing, setImporting] = useState(false);
  const imported = (p: ProjectSummary) => {
    setImporting(false);
    if (view.kind === 'folder') moveTo(p.id, view.name);
    toast(t('Imported “{name}”', { name: p.name }));
  };
  const importFile = async (file: File) => {
    setImporting(false);
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
    const templates = list.reduce((n, p) => n + p.templates, 0);
    if (
      !(await confirmDialog({
        title: list.length === 1 ? t('Delete the project “{name}”?', { name: list[0].name }) : tn(list.length, 'Delete {n} project?', 'Delete {n} projects?'),
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
            {list.length === 1
              ? tn(templates, 'Its template and its media are deleted.', 'Its {n} templates and its media are deleted.')
              : tn(templates, 'Their template and their media are deleted.', 'Their {n} templates and their media are deleted.')}{' '}
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

  const title = view.kind === 'all' ? t('All projects') : view.kind === 'unfiled' ? t('Not in a folder') : view.name;
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
        {navItem('all', { kind: 'all' }, <LayoutGrid size={15} />, t('All projects'), projects.length)}
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
        {!folders.folders.length && <p className="lib-nav-hint">{design ? t('Create folders to sort your shows, then drag projects onto them.') : t('No folder.')}</p>}
      </aside>

      <main className="library">
        <div className="library-inner wide">
          <div className="settings-head">
            <div>
              <h1>{title}</h1>
              <p className="muted">{tn(shown.length, '{n} project', '{n} projects')}</p>
            </div>
            <div className="spacer" />
            {design && (
              <>
                <button className="btn" title={t('Ready-to-air graphics packages to start from')} onClick={() => setExamples(true)}>
                  <GraduationCap size={14} /> {t('Examples')}
                </button>
                <button className="btn" title={t('After Effects, Motion Graphics template, Photoshop, SVG or Strata project')} onClick={() => setImporting(true)}>
                  <Upload size={14} /> {t('Import')}
                </button>
                <button className="btn primary" onClick={() => setDialog({})}>
                  <Plus size={14} /> {t('New project')}
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
              {selected.size === 0 && <span className="ux3-pick-hint">{t('Click the projects to select')}</span>}
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
                <input value={query} placeholder={t('Search projects')} onChange={(e) => setQuery(e.target.value)} />
              </label>
              <div className="spacer" />
              {design && shown.length > 0 && (
                <button className="btn small ghost" title={t('Pick projects to delete or move them together (Ctrl+A: all)')} onClick={() => setPicking(true)}>
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
                  <button role="radio" aria-checked={sort === 'templates'} className={sort === 'templates' ? 'on' : ''} onClick={() => setSort('templates')}>
                    {t('Templates')}
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
              const link = (sub: ProjectPage, icon: React.ReactNode, label: string, show = true) =>
                show && (
                  <a className="project-link" href={projectPath(p.id, sub)} target="_blank" rel="noopener">
                    {icon}
                    <span>{label}</span>
                  </a>
                );
              /** A page of the project, in the "…" menu. */
              const page = (sub: ProjectPage, icon: React.ReactNode, label: string, show = true) =>
                show && (
                  <a href={projectPath(p.id, sub)} target="_blank" rel="noopener" onClick={() => setMenu(null)}>
                    {icon} {label}
                  </a>
                );
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
                    href={projectPath(p.id, design ? 'designer' : 'operator')}
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
                      <span className="project-cover-empty">{t('No template yet')}</span>
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
                      title={isOn ? t('Unselect') : t('Select (Shift: all the projects between)')}
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
                              {page('data', <Database size={13} />, t('Data'), design)}
                              {page('media', <LibraryIcon size={13} />, t('Library'), can.operate(user))}
                              <div className="menu-sep" />
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
                        <Layers size={12} /> {tn(p.templates, '{n} template', '{n} templates')}
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
                  <div className="project-links ux3-two">
                    {link('designer', <PenTool size={15} />, t('Designer'), design)}
                    {link('operator', <Radio size={15} />, t('Operator'), can.operate(user))}
                  </div>
                </div>
              );
            })}
            {shown.length === 0 && (
              <div className="empty big">
                <Radio size={22} />
                <p>{q ? t('No project matches.') : t('No project here.')}</p>
              </div>
            )}
          </div>
        </div>
      </main>
      {examples && <ExampleBank onClose={() => setExamples(false)} />}
      {importing && <ImportModal onClose={() => setImporting(false)} onCreated={imported} onProject={importFile} />}
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

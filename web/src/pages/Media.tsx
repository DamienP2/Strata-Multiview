import { Check, Copy, Download, File, FileAudio, Film, HardDrive, Image as ImageIcon, LayoutGrid, Palette as PaletteIcon, Plus, Search, Trash2, Type, Upload, UploadCloud } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { api, can, mediaName, mediaUrl, uploadAsset, useApp, useChanged, type MediaFile } from '../store';
import { confirmDialog } from '../ui/dialogs';
import { loadFonts, useFonts } from '../engine/fonts';
import { locale, t, tn } from '../i18n';
import { NavItem, NavSection, PageHead, SidePage } from './SidePage';
import { addPalette, ColorsPanel } from './ColorsPanel';
import { useColors } from '../colors';

type Kind = MediaFile['kind'];

const KINDS: { kind: Kind | 'all'; label: string; icon: typeof File }[] = [
  { kind: 'all', label: 'All files', icon: LayoutGrid },
  { kind: 'image', label: 'Images', icon: ImageIcon },
  { kind: 'video', label: 'Videos', icon: Film },
  { kind: 'audio', label: 'Audio', icon: FileAudio },
  { kind: 'font', label: 'Fonts', icon: Type },
  { kind: 'other', label: 'Other', icon: File },
];

const FONT_FILE = /\.(ttf|otf|woff2?|ttc)$/i;

/** Adds fonts to the server (shared by every project). */
async function uploadFonts(files: File[], toast: (m: string, k?: 'info' | 'error') => void) {
  for (const file of files) {
    const name = file.name.replace(/[^A-Za-z0-9._-]/g, '_');
    try {
      const res = await fetch(`/api/fonts/${encodeURIComponent(name)}`, { method: 'PUT', body: file, credentials: 'same-origin' });
      if (!res.ok) throw new Error(await res.text());
      toast(t('Font {name} added', { name: file.name }), 'info');
    } catch (e) {
      toast(`${file.name}: ${(e as Error).message}`, 'error');
    }
  }
  await loadFonts();
}

/** The fonts of the server: uploaded ones (removable), built in, installed on the machine. */
function FontsPanel({ canEdit }: { canEdit: boolean }) {
  const families = useFonts((s) => s.families);
  const toast = useApp((s) => s.toast);
  const groups: [string, string, typeof families][] = [
    [t('Uploaded'), t('Added here, for every project'), families.filter((f) => f.source === 'uploaded')],
    [t('Built in'), t('Shipped with Strata'), families.filter((f) => f.source === 'bundled')],
    [t('Server'), t('Installed on the machine'), families.filter((f) => f.source === 'system')],
  ];
  const remove = async (file: string, family: string) => {
    if (!(await confirmDialog({ title: t('Remove {family} ({file})?', { family, file }), message: t('Texts using it fall back to Inter.'), confirm: t('Remove'), danger: true }))) return;
    try {
      await api('DELETE', `/api/fonts/${encodeURIComponent(file)}`);
      await loadFonts();
    } catch (e) {
      toast((e as Error).message, 'error');
    }
  };
  return (
    <div className="fonts-panel">
      {groups
        .filter(([, , l]) => l.length)
        .map(([title, hint, list]) => (
          <section key={title} className="fonts-group">
            <h3>
              {title} <span className="muted small">{hint}</span>
            </h3>
            <div className="fonts-list">
              {list.map((f) => (
                <div key={f.family} className="font-row">
                  <span className="font-sample" style={{ fontFamily: `"${f.family}"` }}>
                    {f.family}
                  </span>
                  <span className="muted small">{f.weights.join(' · ')}</span>
                  <span className="spacer" />
                  {canEdit &&
                    (f.files ?? []).map((file) => (
                      <button key={file} className="icon-btn danger" title={t('Remove {file}', { file })} onClick={() => void remove(file, f.family)}>
                        <Trash2 size={13} />
                      </button>
                    ))}
                </div>
              ))}
            </div>
          </section>
        ))}
    </div>
  );
}

const ICON: Record<Kind, typeof File> = { image: ImageIcon, video: Film, audio: FileAudio, font: Type, other: File };

/** "70 o", "49 Ko", "6,7 Mo" in French. */
export function formatSize(n: number) {
  const f = (v: number, digits: number) => v.toLocaleString(locale(), { maximumFractionDigits: digits });
  if (n < 1024) return t('{n} B', { n: f(n, 0) });
  if (n < 1024 ** 2) return t('{n} KB', { n: f(n / 1024, 0) });
  if (n < 1024 ** 3) return t('{n} MB', { n: f(n / 1024 ** 2, 1) });
  return t('{n} GB', { n: f(n / 1024 ** 3, 2) });
}

const formatDate = (ms: number) => new Date(ms).toLocaleDateString(locale(), { day: 'numeric', month: 'short', year: 'numeric' });

type Upload = { name: string; progress: number; error?: string };

/**
 * A media library: the files of one project (`projectId`), or the files
 * shared by every project. Upload by button or by dropping files anywhere.
 */
export function MediaLibrary({ projectId }: { projectId?: string }) {
  const { auth, toast } = useApp();
  const user = auth?.user;
  const canUpload = !!user && user.role !== 'viewer';
  const canDelete = can.design(user);
  const [files, setFiles] = useState<MediaFile[] | null>(null);
  const families = useFonts((s) => s.families);
  useEffect(() => {
    if (!families.length) void loadFonts();
  }, [families.length]);
  const [kind, setKind] = useState<Kind | 'all' | 'colors'>('all');
  const palettes = useColors((s) => s.palettes);
  const loadColors = useColors((s) => s.load);
  const colorsLoaded = useColors((s) => s.loaded);
  useEffect(() => {
    if (!colorsLoaded) void loadColors();
  }, [colorsLoaded, loadColors]);
  const projectColors = useApp((s) => (projectId ? (s.full[projectId]?.colors?.length ?? 0) : 0));
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<'name' | 'date' | 'size'>('name');
  const [uploads, setUploads] = useState<Upload[]>([]);
  const [dragging, setDragging] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const depth = useRef(0);
  const url = mediaUrl(projectId);

  const refresh = () =>
    api<MediaFile[]>('GET', url)
      .then(setFiles)
      .catch((e) => toast((e as Error).message, 'error'));
  // Uploads and deletions by other users.
  useChanged(projectId ? `media:${projectId}` : 'media', () => void refresh());
  useEffect(() => {
    setFiles(null);
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url]);

  const upload = async (list: FileList | File[]) => {
    // Fonts are global: they go to the server's fonts, never to a project.
    const fonts = [...list].filter((f) => FONT_FILE.test(f.name));
    if (fonts.length) await uploadFonts(fonts, toast);
    const todo = [...list].filter((f) => !FONT_FILE.test(f.name));
    if (!todo.length) return;
    const existing = new Set(files?.map((f) => f.name));
    const replaced = todo.filter((f) => existing.has(mediaName(f.name))).map((f) => mediaName(f.name));
    if (replaced.length && !(await confirmDialog({ title: t('Replace {names}?', { names: replaced.join(', ') }), message: tn(replaced.length, 'Templates using it show the new file.', 'Templates using them show the new files.'), confirm: t('Replace') }))) return;
    setUploads((u) => [...u, ...todo.map((f) => ({ name: f.name, progress: 0 }))]);
    await Promise.all(
      todo.map(async (f) => {
        const set = (patch: Partial<Upload>) => setUploads((u) => u.map((x) => (x.name === f.name ? { ...x, ...patch } : x)));
        try {
          await uploadAsset(f, projectId, (progress) => set({ progress }));
          setUploads((u) => u.filter((x) => x.name !== f.name));
        } catch (e) {
          set({ error: (e as Error).message });
        }
      }),
    );
    void refresh();
  };

  const remove = async (m: MediaFile) => {
    if (!(await confirmDialog({ title: t('Delete “{name}”?', { name: m.name }), message: t('Templates that use it will show nothing in its place.'), confirm: t('Delete'), danger: true }))) return;
    try {
      await api('DELETE', `${url}/${encodeURIComponent(m.name)}`);
      setFiles((l) => l?.filter((x) => x.src !== m.src) ?? null);
    } catch (e) {
      toast((e as Error).message, 'error');
    }
  };

  const copy = async (m: MediaFile) => {
    try {
      await navigator.clipboard.writeText(m.src);
      setCopied(m.src);
      setTimeout(() => setCopied((c) => (c === m.src ? null : c)), 1500);
    } catch {
      toast(m.src);
    }
  };

  const counts = useMemo(() => {
    const c: Record<string, number> = { all: files?.length ?? 0 };
    for (const f of files ?? []) c[f.kind] = (c[f.kind] ?? 0) + 1;
    return c;
  }, [files]);
  const total = useMemo(() => (files ?? []).reduce((n, f) => n + f.size, 0), [files]);
  const q = query.trim().toLowerCase();
  const shown = (files ?? [])
    .filter((f) => (kind === 'all' || f.kind === kind) && f.name.toLowerCase().includes(q))
    .sort((a, b) => (sort === 'date' ? b.modified - a.modified : sort === 'size' ? b.size - a.size : 0) || a.name.localeCompare(b.name));
  const fonts = kind === 'font';
  const colors = kind === 'colors';
  const label = KINDS.find((k) => k.kind === kind)?.label ?? 'All files';

  const nav = (
    <>
      <NavSection>{projectId ? t('Media of the project') : t('Media')}</NavSection>
      {KINDS.filter((k) => k.kind !== 'font').map(({ kind: k, label: l, icon: Icon }) => (
        <NavItem key={k} on={kind === k} icon={<Icon size={15} />} label={t(l)} count={counts[k] ?? 0} className={k !== 'all' && !counts[k] ? 'dim' : ''} onClick={() => setKind(k)} />
      ))}
      <NavSection>{t('Typography')}</NavSection>
      <NavItem on={kind === 'font'} icon={<Type size={15} />} label={t('Fonts')} count={families.length} onClick={() => setKind('font')} />
      <NavSection>{t('Colours')}</NavSection>
      <NavItem on={colors} icon={<PaletteIcon size={15} />} label={t('Colours')} count={palettes.reduce((n, p) => n + p.colors.length, projectColors)} onClick={() => setKind('colors')} />
      <div className="lib-nav-foot">
        <HardDrive size={13} /> {files ? `${tn(files.length, '{n} file', '{n} files')} · ${formatSize(total)}` : '…'}
      </div>
    </>
  );

  return (
    <SidePage
      nav={nav}
      className={`media-page ${dragging ? 'dragging' : ''}`}
      onDragEnter={(e) => {
        if (!canUpload || !e.dataTransfer.types.includes('Files')) return;
        depth.current++;
        setDragging(true);
      }}
      onDragLeave={() => {
        if (--depth.current <= 0) {
          depth.current = 0;
          setDragging(false);
        }
      }}
      onDragOver={(e) => canUpload && e.dataTransfer.types.includes('Files') && e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        depth.current = 0;
        setDragging(false);
        if (canUpload) void upload(e.dataTransfer.files);
      }}
      after={
        dragging && (
          <div className="drop-overlay">
            <UploadCloud size={42} />
            <strong>{projectId ? t('Drop to upload to this project') : t('Drop to upload to the shared assets')}</strong>
          </div>
        )
      }
    >
      <PageHead
        title={colors ? t('Colours') : fonts ? t('Fonts') : kind === 'all' ? (projectId ? t('Media of the project') : t('Shared media')) : t(label)}
        sub={colors ? undefined : fonts ? t('Fonts are shared by every project. Built-in ones work offline and look the same on every output.') : tn(shown.length, '{n} file', '{n} files')}
      >
        {colors && canDelete && (
          <button className="btn primary" onClick={addPalette}>
            <Plus size={14} /> {t('Palette')}
          </button>
        )}
        {!colors && canUpload && (
          <label className="btn primary">
            <Upload size={14} /> {fonts ? t('Add fonts') : t('Upload')}
            <input type="file" multiple hidden accept={fonts ? '.ttf,.otf,.woff,.woff2,.ttc' : undefined} onChange={(e) => e.target.files && void upload(e.target.files)} />
          </label>
        )}
      </PageHead>

      {!fonts && !colors && (
        <div className="media-toolbar">
          <label className="search">
            <Search size={14} />
            <input value={query} placeholder={t('Search files')} onChange={(e) => setQuery(e.target.value)} />
          </label>
          <div className="spacer" />
          <label className="sort">
            {t('Sort')}
            <select value={sort} onChange={(e) => setSort(e.target.value as typeof sort)}>
              <option value="name">{t('Name')}</option>
              <option value="date">{t('Newest')}</option>
              <option value="size">{t('Size')}</option>
            </select>
          </label>
        </div>
      )}

      {uploads.length > 0 && (
        <div className="uploads">
          {uploads.map((u) => (
            <div key={u.name} className={`upload-row ${u.error ? 'error' : ''}`}>
              <span className="grow">{u.name}</span>
              {u.error ? (
                <>
                  <span className="small">{u.error}</span>
                  <button className="btn small" onClick={() => setUploads((l) => l.filter((x) => x.name !== u.name))}>
                    {t('Dismiss')}
                  </button>
                </>
              ) : (
                <div className="bar">
                  <div style={{ width: `${Math.round(u.progress * 100)}%` }} />
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {colors ? (
        <ColorsPanel projectId={projectId} canEdit={canDelete} />
      ) : fonts ? (
        <FontsPanel canEdit={canDelete} />
      ) : files && files.length === 0 ? (
        <label className={`media-empty ${canUpload ? '' : 'static'}`}>
          <UploadCloud size={36} />
          <strong>{t('No media yet')}</strong>
          <span className="muted">{canUpload ? t('Drop files here or click to upload: images, videos, audio, fonts…') : t('Nothing has been uploaded.')}</span>
          {canUpload && <input type="file" multiple hidden onChange={(e) => e.target.files && void upload(e.target.files)} />}
        </label>
      ) : (
        <div className="media-grid">
          {shown.map((m) => {
            const Icon = ICON[m.kind];
            return (
              <div key={m.src} className="media-tile" title={m.src}>
                <div className="media-preview">
                  {m.kind === 'image' ? (
                    <img src={`${m.src}?v=${m.modified}`} alt="" loading="lazy" />
                  ) : m.kind === 'video' ? (
                    <>
                      <Icon size={30} />
                      <video src={`${m.src}?v=${m.modified}#t=0.5`} muted preload="metadata" onError={(e) => (e.currentTarget.style.display = 'none')} onMouseEnter={(e) => void e.currentTarget.play().catch(() => {})} onMouseLeave={(e) => e.currentTarget.pause()} />
                    </>
                  ) : (
                    <Icon size={30} />
                  )}
                  <span className={`media-kind ${m.kind}`}>
                    <Icon size={11} /> {m.name.split('.').pop()?.toUpperCase()}
                  </span>
                  <div className="media-actions">
                    <button className="icon-btn" title={t('Copy the URL')} onClick={() => void copy(m)}>
                      {copied === m.src ? <Check size={14} /> : <Copy size={14} />}
                    </button>
                    <a className="icon-btn" title={t('Download')} href={m.src} download={m.name}>
                      <Download size={14} />
                    </a>
                    {canDelete && (
                      <button className="icon-btn danger" title={t('Delete')} onClick={() => void remove(m)}>
                        <Trash2 size={14} />
                      </button>
                    )}
                  </div>
                </div>
                <div className="media-info">
                  <strong>{m.name}</strong>
                  <span className="muted small">
                    {formatSize(m.size)} · {formatDate(m.modified)}
                  </span>
                </div>
              </div>
            );
          })}
          {files && !shown.length && <p className="muted">{t('No file matches.')}</p>}
        </div>
      )}
    </SidePage>
  );
}

// Picture choice: the row shows the thumbnail and the name; a click opens the
// library (project and shared pictures, search, upload, drop) as a grid.
import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Link2, Search, Sparkles, Upload, X } from 'lucide-react';
import { api, mediaUrl, uploadAsset, useApp, type MediaFile } from '../store';
import { PictureLibrary } from './PictureLibrary';
import { t } from '../i18n';

const fileName = (src: string) => decodeURIComponent(src.split('?')[0].split('/').pop() || src);

export function AssetPicker({ value, onChange, placeholder = t('Choose a picture…'), title, clearable = true }: { value: string; onChange: (v: string) => void; placeholder?: string; title?: string; clearable?: boolean }) {
  const [open, setOpen] = useState(false);
  const [broken, setBroken] = useState(false);
  useEffect(() => setBroken(false), [value]);
  return (
    <>
      <div className="pick" role="button" tabIndex={0} title={value || undefined} onClick={() => setOpen(true)} onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && setOpen(true)}>
        <span className={`pick-th ${value && !broken ? '' : 'none'}`}>{value && !broken && <img src={value} alt="" onError={() => setBroken(true)} />}</span>
        <span className={`pick-nm ${value ? '' : 'ph'}`}>{value ? fileName(value) : placeholder}</span>
        {clearable && value && (
          <button
            className="pick-x"
            title={t('Remove')}
            onClick={(e) => {
              e.stopPropagation();
              onChange('');
            }}
          >
            <X size={12} />
          </button>
        )}
      </div>
      {open && (
        <AssetLibrary
          title={title ?? t('Choose a picture')}
          value={value}
          onClose={() => setOpen(false)}
          onChoose={(src) => {
            setOpen(false);
            onChange(src);
          }}
        />
      )}
    </>
  );
}

type Tab = 'project' | 'shared' | 'library';

/** What a library lists: pictures, videos (and animated GIFs), or Lottie files. */
export type LibraryKind = 'image' | 'video' | 'lottie';

const KIND: Record<LibraryKind, { keep: (m: MediaFile) => boolean; accept: string; upload: (f: File) => boolean }> = {
  // Illustrator files (.ai) come back as SVG pictures, one per artboard.
  image: { keep: (m) => m.kind === 'image', accept: 'image/*,.ai', upload: (f) => f.type.startsWith('image/') || /\.ai$/i.test(f.name) },
  video: {
    keep: (m) => m.kind === 'video' || /\.(gif|apng)$/i.test(m.name),
    accept: 'video/*,.mov,.mxf,.mkv,.gif,.apng',
    upload: (f) => f.type.startsWith('video/') || /\.(mov|mxf|mkv|mp4|webm|gif|apng)$/i.test(f.name),
  },
  lottie: { keep: (m) => /\.(json|lottie|zip)$/i.test(m.name), accept: '.json,.lottie,.zip', upload: (f) => /\.(json|lottie|zip)$/i.test(f.name) },
};

export function AssetLibrary({ title, value, onClose, onChoose, kind = 'image' }: { title: string; value: string; onClose: () => void; onChoose: (src: string) => void; kind?: LibraryKind }) {
  const projectId = useApp((s) => s.projectId);
  const toast = useApp((s) => s.toast);
  const [lists, setLists] = useState<Record<Tab, MediaFile[]>>({ project: [], shared: [], library: [] });
  const [tab, setTab] = useState<Tab>(projectId ? 'project' : 'shared');
  const [q, setQ] = useState('');
  const [sel, setSel] = useState(value);
  const [drag, setDrag] = useState(false);
  // A picture at a web address, fetched on the fly.
  const [url, setUrl] = useState(() => (/^https?:\/\//i.test(value) ? value : ''));
  const isUrl = (v: string) => /^https?:\/\/\S+$/i.test(v.trim());
  const [busy, setBusy] = useState(0);
  const sizes = useRef(new Map<string, string>());
  const [, bump] = useState(0);

  const refresh = () => {
    const images = (l: MediaFile[]) => l.filter(KIND[kind].keep).sort((a, b) => b.modified - a.modified);
    if (projectId)
      api<MediaFile[]>('GET', mediaUrl(projectId))
        .then((l) => setLists((x) => ({ ...x, project: images(l) })))
        .catch(() => {});
    api<MediaFile[]>('GET', mediaUrl(null))
      .then((l) => setLists((x) => ({ ...x, shared: images(l) })))
      .catch(() => {});
  };
  useEffect(refresh, [projectId, kind]);
  // Open on the tab that holds the current picture.
  useEffect(() => {
    if (value && lists.shared.some((m) => m.src === value) && !lists.project.some((m) => m.src === value)) setTab('shared');
  }, [lists, value]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      } else if (e.key === 'Enter' && sel && !(e.target instanceof HTMLInputElement)) onChoose(sel);
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [sel, onClose, onChoose]);

  const upload = async (files: FileList | File[]) => {
    const imgs = [...files].filter(KIND[kind].upload);
    if (!imgs.length) return;
    setBusy((n) => n + imgs.length);
    let last = '';
    for (const f of imgs) {
      try {
        last = await uploadAsset(f, projectId);
      } catch (err) {
        toast(t('Upload failed: {error}', { error: (err as Error).message }), 'error');
      }
      setBusy((n) => n - 1);
    }
    if (last) {
      setTab(projectId ? 'project' : 'shared');
      setQ('');
      setSel(last);
      refresh();
    }
  };

  const list = useMemo(() => lists[tab].filter((m) => m.name.toLowerCase().includes(q.trim().toLowerCase())), [lists, tab, q]);
  const selected = [...lists.project, ...lists.shared].find((m) => m.src === sel);

  return createPortal(
    <div className="lib-scrim" onPointerDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="lib">
        <div className="lib-h">
          <h2>{title}</h2>
          <div className="lib-tabs">
            {projectId && (
              <button className={tab === 'project' ? 'on' : ''} onClick={() => setTab('project')}>
                {t('Project')}
                <b>{lists.project.length}</b>
              </button>
            )}
            <button className={tab === 'shared' ? 'on' : ''} onClick={() => setTab('shared')}>
              {t('Shared')}
              <b>{lists.shared.length}</b>
            </button>
            {/* Icons, emojis, flags, illustrations: copied into the project when used. */}
            {kind === 'image' && projectId && (
              <button className={tab === 'library' ? 'on' : ''} onClick={() => setTab('library')}>
                {t('Library')}
              </button>
            )}
          </div>
          <span className="grow" />
          <label className="lib-search">
            <Search size={13} />
            <input autoFocus value={q} placeholder={t('Search')} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.stopPropagation()} />
          </label>
          {kind === 'image' && (
            <label className={`lib-search lib-url ${url && !isUrl(url) ? 'bad' : ''}`} title={t('A picture at a web address, fetched when shown')}>
              <Link2 size={13} />
              <input
                value={url}
                placeholder={t('Paste a URL')}
                onChange={(e) => setUrl(e.target.value)}
                onPaste={(e) => {
                  const v = e.clipboardData.getData('text').trim();
                  if (isUrl(v)) {
                    e.preventDefault();
                    onChoose(v);
                  }
                }}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === 'Enter' && isUrl(url)) onChoose(url.trim());
                }}
              />
            </label>
          )}
          <label className="btn">
            <Upload size={13} /> {busy ? t('Uploading…') : t('Upload')}
            <input
              type="file"
              accept={KIND[kind].accept}
              multiple
              hidden
              onChange={(e) => {
                if (e.target.files) void upload(e.target.files);
                e.target.value = '';
              }}
            />
          </label>
          <button className="btn ghost icon" title={t('Close')} onClick={onClose}>
            <X size={14} />
          </button>
        </div>
        {tab === 'library' ? (
          <div className="lib-b">
            <PictureLibrary q={q} onChoose={onChoose} />
          </div>
        ) : (
        <div
          className={`lib-b ${drag ? 'dragging' : ''}`}
          onDragOver={(e) => {
            e.preventDefault();
            setDrag(true);
          }}
          onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && setDrag(false)}
          onDrop={(e) => {
            e.preventDefault();
            setDrag(false);
            void upload(e.dataTransfer.files);
          }}
        >
          {list.length ? (
            <div className="lib-grid">
              {list.map((m) => (
                <button key={m.src} className={`lib-card ${sel === m.src ? 'on' : ''}`} onClick={() => setSel(m.src)} onDoubleClick={() => onChoose(m.src)} title={m.name}>
                  <div className="lib-im">
                    {kind === 'lottie' ? (
                      <Sparkles size={28} className="lib-icon" />
                    ) : m.kind === 'video' ? (
                      <video
                        src={`${m.src}#t=0.5`}
                        muted
                        preload="metadata"
                        onLoadedMetadata={(e) => {
                          const v = e.currentTarget;
                          if (!sizes.current.has(m.src)) {
                            sizes.current.set(m.src, `${v.videoWidth}×${v.videoHeight} · ${Math.round(v.duration)} s`);
                            bump((n) => n + 1);
                          }
                        }}
                      />
                    ) : (
                      <img
                        src={m.src}
                        alt=""
                        loading="lazy"
                        onLoad={(e) => {
                          const i = e.currentTarget;
                          if (!sizes.current.has(m.src)) {
                            sizes.current.set(m.src, `${i.naturalWidth}×${i.naturalHeight}`);
                            bump((n) => n + 1);
                          }
                        }}
                      />
                    )}
                  </div>
                  <div className="lib-meta">
                    <b>{m.name}</b>
                    <span>{sizes.current.get(m.src) ?? ' '}</span>
                  </div>
                </button>
              ))}
            </div>
          ) : (
            <div className="lib-empty">
              {kind === 'video'
                ? q
                  ? t('No video matches.')
                  : tab === 'project'
                    ? t('No videos in this project yet: upload or drop some here.')
                    : t('No shared videos yet.')
                : kind === 'lottie'
                  ? q
                    ? t('No file matches.')
                    : t('No Lottie file here yet: upload or drop a .json, .lottie or .zip.')
                  : q
                    ? t('No picture matches.')
                    : tab === 'project'
                      ? t('No pictures in this project yet: upload or drop some here.')
                      : t('No shared pictures yet.')}
            </div>
          )}
          <div className="lib-drop">{kind !== 'image' ? (projectId ? t('Drop files here: they go to the project') : t('Drop files here: they go to the shared library')) : projectId ? t('Drop pictures here: they go to the project') : t('Drop pictures here: they go to the shared library')}</div>
        </div>
        )}
        {tab !== 'library' && (
        <div className="lib-f">
          <span className="lib-sel">{selected ? `${selected.name}${sizes.current.get(selected.src) ? ` · ${sizes.current.get(selected.src)}` : ''}` : kind === 'image' ? t('Double-click a picture to choose it') : t('Double-click a file to choose it')}</span>
          <button className="btn ghost" onClick={onClose}>
            {t('Cancel')}
          </button>
          <button className="btn violet" disabled={!sel} onClick={() => onChoose(sel)}>
            {t('Choose')}
          </button>
        </div>
        )}
      </div>
    </div>,
    document.body,
  );
}

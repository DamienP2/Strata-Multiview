import { designTemplates } from '../engine/drafts';
import { useShallow } from 'zustand/react/shallow';
import { Plus, RectangleHorizontal, RectangleVertical, Square, Wand2, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { adaptTemplate, projectFormats } from '../engine/formats';
import { MAIN_FORMAT, type Format, type Project } from '../engine/types';
import { api, useApp } from '../store';
import { useDesigner } from './designerStore';
import { confirmDialog } from '../ui/dialogs';
import { t, tn } from '../i18n';

const PRESETS = [
  { id: 'vertical', name: '9:16', width: 1080, height: 1920, hint: 'Stories, Reels, TikTok, Shorts' },
  { id: 'square', name: '1:1', width: 1080, height: 1080, hint: 'Square feeds' },
  { id: 'portrait', name: '4:5', width: 1080, height: 1350, hint: 'Instagram / Facebook feed' },
  { id: 'landscape', name: '16:9', width: 1920, height: 1080, hint: 'TV, YouTube, web players' },
  { id: 'custom', name: 'Custom', width: 1280, height: 720, hint: 'Any size' },
];

/** A frame drawn to the format's proportions. */
function Shape({ width, height, size = 44 }: { width: number; height: number; size?: number }) {
  const r = width / height;
  return <span className="fmt-shape" style={r >= 1 ? { width: size, height: size / r } : { width: size * r, height: size }} />;
}

const shapeIcon = (f: { width: number; height: number }) => (f.width > f.height * 1.1 ? RectangleHorizontal : f.height > f.width * 1.1 ? RectangleVertical : Square);

/** Lays every template of the project out in `format` (smart adaptation). */
/** Returns the templates left as they were (someone else is editing them). */
async function adaptProject(project: Project, format: Format, only?: string): Promise<string[]> {
  const skipped: string[] = [];
  for (const t of designTemplates(project)) {
    if (only && t.id !== only) continue;
    const adapted = adaptTemplate(t, project, format);
    await api('PUT', `/api/projects/${encodeURIComponent(project.id)}/templates/${encodeURIComponent(t.id)}`, adapted).catch(() => skipped.push(t.name));
  }
  return skipped;
}

function AddFormat({ project, onClose }: { project: Project; onClose: () => void }) {
  const toast = useApp((s) => s.toast);
  const existing = projectFormats(project);
  const free = PRESETS.filter((p) => !existing.some((f) => f.width * p.height === f.height * p.width));
  const [pick, setPick] = useState(free.find((p) => p.id !== 'custom') ?? PRESETS[PRESETS.length - 1]);
  const [name, setName] = useState(pick.name);
  const [w, setW] = useState(pick.width);
  const [h, setH] = useState(pick.height);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setName(pick.id === 'custom' ? `${w}×${h}` : pick.name);
    if (pick.id !== 'custom') {
      setW(pick.width);
      setH(pick.height);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pick]);
  const create = async () => {
    setBusy(true);
    let id = pick.id;
    for (let n = 2; existing.some((f) => f.id === id); n++) id = `${pick.id}-${n}`;
    const format: Format = { id, name: name.trim() || (pick.id === 'custom' ? t('Custom') : pick.name), width: w, height: h };
    try {
      // Save the designer's pending edits first, so the adaptation sees them.
      const st = useDesigner.getState();
      if (st.base) await api('PUT', `/api/projects/${encodeURIComponent(project.id)}/templates/${encodeURIComponent(st.base.id)}`, st.base);
      await api('PUT', `/api/projects/${encodeURIComponent(project.id)}/formats`, [...(project.formats ?? []), format]);
      const fresh = await api<Project>('GET', `/api/projects/${encodeURIComponent(project.id)}`);
      const skipped = await adaptProject(fresh, format);
      const reloaded = await api<Project>('GET', `/api/projects/${encodeURIComponent(project.id)}`);
      const cur = designTemplates(reloaded).find((t) => t.id === st.base?.id);
      st.setFormat(format.id);
      if (cur) st.load(cur);
      toast(skipped.length ? t('{name} added. Not adapted (being edited by someone else): {list}', { name: format.name, list: skipped.join(', ') }) : t('{name} added: every template has a first layout to refine', { name: format.name }), skipped.length ? 'error' : 'info');
      onClose();
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  const taken = (p: { width: number; height: number }) => existing.find((f) => f.width * p.height === f.height * p.width);
  return (
    <div className="modal-back" onPointerDown={onClose}>
      <div className="modal wide add-format" onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
        <header>
          <h2>{t('Add a format to “{name}”', { name: project.name })}</h2>
          <button className="icon-btn ghost" title={t('Close')} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <p className="hint">{t('Each template gets a first layout in the new format, made from the main one. Refine it afterwards: changes made in a format stay in that format.')}</p>

        <div className="fmt-grid">
          {PRESETS.map((p) => {
            const already = p.id !== 'custom' && taken(p);
            return (
              <button type="button" key={p.id} className={`fmt-card ${pick.id === p.id ? 'on' : ''}`} disabled={!!already} onClick={() => setPick(p)}>
                <span className="fmt-preview">{p.id === 'custom' ? <Shape width={w} height={h} /> : <Shape width={p.width} height={p.height} />}</span>
                <strong>{p.id === 'custom' ? t('Custom') : p.name}</strong>
                <span className="fmt-size">{p.id === 'custom' ? `${w}×${h}` : `${p.width}×${p.height}`}</span>
                {already && <span className="fmt-hint">{t('Already in the project ({name})', { name: already.name })}</span>}
              </button>
            );
          })}
        </div>

        <div className="fmt-fields">
          <label className="set-field">
            <span className="set-label">{t('Name')}</span>
            <input value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="set-field">
            <span className="set-label">{t('Width')}</span>
            <input type="number" min={16} max={7680} value={w} onChange={(e) => (setW(+e.target.value), pick.id !== 'custom' && setPick(PRESETS[PRESETS.length - 1]))} />
          </label>
          <span className="fmt-x">×</span>
          <label className="set-field">
            <span className="set-label">{t('Height')}</span>
            <input type="number" min={16} max={7680} value={h} onChange={(e) => (setH(+e.target.value), pick.id !== 'custom' && setPick(PRESETS[PRESETS.length - 1]))} />
          </label>
        </div>

        <div className="fmt-compare">
          <span>
            <Shape width={project.width} height={project.height} size={34} /> {t('Main {format}', { format: `${project.width}×${project.height}` })}
          </span>
          <span className="arrow">→</span>
          <span>
            <Shape width={w || 1} height={h || 1} size={34} /> {name.trim() || (pick.id === 'custom' ? t('Custom') : pick.name)} {w}×{h}
          </span>
          <span className="muted small">{tn(designTemplates(project).length, '{n} template to adapt', '{n} templates to adapt')}</span>
        </div>

        <footer>
          <button className="btn" onClick={onClose}>
            {t('Cancel')}
          </button>
          <button className="btn primary" disabled={busy || w < 16 || h < 16} onClick={create}>
            <Wand2 size={14} /> {busy ? t('Adapting the templates…') : t('Add and adapt')}
          </button>
        </footer>
      </div>
    </div>
  );
}

/** Format tabs of the designer (Main 16:9 | 9:16 | +). */
export function FormatBar({ project }: { project: Project }) {
  const { format, setFormat, base, readOnly } = useDesigner(useShallow((s) => ({ format: s.format, setFormat: s.setFormat, base: s.base, readOnly: s.readOnly })));
  const toast = useApp((s) => s.toast);
  const [adding, setAdding] = useState(false);
  const formats = projectFormats(project);
  const current = formats.find((f) => f.id === format) ?? formats[0];
  useEffect(() => {
    if (!formats.some((f) => f.id === format)) setFormat(MAIN_FORMAT);
  }, [formats.length, format]);
  const removeFormat = async (f: Format) => {
    const n = designTemplates(project).length;
    if (!(await confirmDialog({ title: t('Remove the {format} format?', { format: f.name }), message: tn(n, 'The template loses its layout in this format.', 'The {n} templates lose their layout in this format.'), confirm: t('Remove'), danger: true }))) return;
    await api(
      'PUT',
      `/api/projects/${encodeURIComponent(project.id)}/formats`,
      (project.formats ?? []).filter((x) => x.id !== f.id),
    ).catch((e) => toast((e as Error).message, 'error'));
    if (format === f.id) setFormat(MAIN_FORMAT);
  };
  const ratio = (f: Format) => {
    const g = (a: number, b: number): number => (b ? g(b, a % b) : a);
    const d = g(f.width, f.height);
    return `${f.width / d}:${f.height / d}`;
  };
  return (
    <div className="format-bar">
      {formats.map((f) => {
        const Icon = shapeIcon(f);
        return (
          <button key={f.id} className={f.id === current.id ? 'on' : ''} onClick={() => setFormat(f.id)} title={`${f.width}×${f.height}`}>
            <Icon size={13} /> {f.id === MAIN_FORMAT ? t('Main {format}', { format: ratio(f) }) : f.name}
            {f.id !== MAIN_FORMAT && !readOnly && (
              <span
                className="format-x"
                role="button"
                title={t('Remove the {format} format from the project', { format: f.name })}
                onClick={(e) => {
                  e.stopPropagation();
                  void removeFormat(f);
                }}
              >
                <X size={11} />
              </span>
            )}
          </button>
        );
      })}
      <button className="add" title={t('Add a format (9:16, 1:1…)')} onClick={() => setAdding(true)}>
        <Plus size={13} />
      </button>
      {current.id !== MAIN_FORMAT && base && (
        <>
          <button
            className="tool-link"
            disabled={readOnly}
            title={t("Generate this template's layout again from the main format (replaces its changes in this format)")}
            onClick={async () => {
              if (!(await confirmDialog({ title: t('Re-adapt “{name}” to {format}?', { name: base.name, format: current.name }), message: t('Its changes in {format} are replaced by a new layout made from the main format.', { format: current.name }), confirm: t('Re-adapt'), danger: true }))) return;
              const adapted = adaptTemplate(base, project, current);
              // An edit like the others: undoable, saved by the autosave (with the edit lock).
              useDesigner.getState().updateBase((t) => {
                for (const k of Object.keys(t)) delete (t as unknown as Record<string, unknown>)[k];
                Object.assign(t, structuredClone(adapted));
              });
            }}
          >
            <Wand2 size={13} /> {t('Re-adapt')}
          </button>
        </>
      )}
      {adding && <AddFormat project={project} onClose={() => setAdding(false)} />}
    </div>
  );
}

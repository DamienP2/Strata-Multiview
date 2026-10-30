import { Plus, Trash2, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useColors } from '../colors';
import { newId, parseHex, toHex } from '../engine/evaluate';
import type { Color, NamedColor, Palette } from '../engine/types';
import { t } from '../i18n';
import { useApp } from '../store';
import { ColorPicker, swatchCss } from '../ui/ColorPicker';
import { confirmDialog } from '../ui/dialogs';

/*
 * Library › Colours: the shared palettes and, in a project, its own. A
 * colour's new value is saved when its picker closes (the layers using the
 * old value follow it).
 */

/** A name typed in place, saved when left. */
function NameInput({ value, onChange, disabled, className }: { value: string; onChange: (v: string) => void; disabled?: boolean; className?: string }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <input
      className={className}
      value={text}
      disabled={disabled}
      spellCheck={false}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => text.trim() !== value && onChange(text.trim())}
      onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
    />
  );
}

function Chip({ c, canEdit, onChange, onRemove }: { c: NamedColor; canEdit: boolean; onChange: (c: NamedColor) => void; onRemove: () => void }) {
  const [open, setOpen] = useState<DOMRect | null>(null);
  const [live, setLive] = useState<Color | null>(null);
  const live$ = useRef<Color | null>(null);
  const value = live ?? parseHex(c.value) ?? [1, 1, 1, 1];
  const close = () => {
    setOpen(null);
    const v = live$.current;
    live$.current = null;
    setLive(null);
    if (v) onChange({ ...c, value: toHex(v, v[3] < 1) });
  };
  return (
    <div className="lc-chip">
      <button type="button" className="lc-swatch" disabled={!canEdit} onClick={(e) => setOpen(e.currentTarget.getBoundingClientRect())}>
        <i style={{ background: swatchCss(value) }} />
      </button>
      <div className="lc-text">
        <NameInput className="lc-name" value={c.name} disabled={!canEdit} onChange={(name) => onChange({ ...c, name })} />
        <code>{toHex(value, value[3] < 1).toUpperCase()}</code>
      </div>
      {canEdit && (
        <button type="button" className="lc-x" title={t('Remove')} onClick={onRemove}>
          <X size={12} />
        </button>
      )}
      {open && (
        <ColorPicker
          value={value}
          anchor={open}
          onChange={(v) => {
            live$.current = v;
            setLive(v);
          }}
          onClose={close}
        />
      )}
    </div>
  );
}

function PaletteView({ title, sub, colors, canEdit, onColors, onName, onRemove }: { title: string; sub: string; colors: NamedColor[]; canEdit: boolean; onColors: (c: NamedColor[]) => void; onName?: (n: string) => void; onRemove?: () => void }) {
  return (
    <section className="lc-pal">
      <header>
        {onName ? <NameInput className="lc-pal-name" value={title} disabled={!canEdit} onChange={onName} /> : <strong className="lc-pal-name">{title}</strong>}
        <span className="muted small">{sub}</span>
        <span className="spacer" />
        {canEdit && onRemove && (
          <button type="button" className="icon-btn danger" title={t('Delete the palette')} onClick={onRemove}>
            <Trash2 size={13} />
          </button>
        )}
      </header>
      <div className="lc-grid">
        {colors.map((c) => (
          <Chip key={c.id} c={c} canEdit={canEdit} onChange={(n) => onColors(colors.map((x) => (x.id === c.id ? n : x)))} onRemove={() => onColors(colors.filter((x) => x.id !== c.id))} />
        ))}
        {canEdit && (
          <button type="button" className="lc-chip lc-new" onClick={() => onColors([...colors, { id: newId(), name: t('Colour {n}', { n: colors.length + 1 }), value: '#ffffff' }])}>
            <Plus size={14} /> {t('Colour')}
          </button>
        )}
      </div>
    </section>
  );
}

export function ColorsPanel({ projectId, canEdit }: { projectId?: string; canEdit: boolean }) {
  const { palettes, loaded, load, save, saveProject } = useColors();
  const project = useApp((s) => (projectId ? s.full[projectId] : undefined));
  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);
  const setPalette = (id: string, fn: (p: Palette) => Palette) => void save(palettes.map((p) => (p.id === id ? fn(p) : p)));
  const removePalette = async (p: Palette) => {
    if (!(await confirmDialog({ title: t('Delete the palette “{name}”?', { name: p.name }), message: t('The layers keep their colours.'), confirm: t('Delete'), danger: true }))) return;
    void save(palettes.filter((x) => x.id !== p.id));
  };
  return (
    <div className="lc-panel">
      {projectId && project && <PaletteView title={t('Project')} sub={t('this project only')} colors={project.colors ?? []} canEdit={canEdit} onColors={(c) => void saveProject(projectId, c)} />}
      {palettes.map((p) => (
        <PaletteView key={p.id} title={p.name} sub={t('shared · every project')} colors={p.colors} canEdit={canEdit} onName={(name) => setPalette(p.id, (x) => ({ ...x, name }))} onColors={(colors) => setPalette(p.id, (x) => ({ ...x, colors }))} onRemove={() => void removePalette(p)} />
      ))}
      {loaded && !palettes.length && !projectId && <p className="muted">{t('No palette yet.')}</p>}
    </div>
  );
}

/** Adds a shared palette. */
export function addPalette() {
  const { palettes, save } = useColors.getState();
  void save([...palettes, { id: newId(), name: t('Palette {n}', { n: palettes.length + 1 }), colors: [] }]);
}

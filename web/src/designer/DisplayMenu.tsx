import { Check, ChevronDown, Eye, Ruler } from 'lucide-react';
import { useEffect, useState } from 'react';
import { t } from '../i18n';
import { GuidesSettings, useGuidesView } from './Guides';

/*
 * The Designer toolbar's "Display" menu: the guides, shown or hidden, and
 * their settings right under them.
 */

export function DisplayMenu() {
  const view = useGuidesView();
  const [open, setOpen] = useState<{ left: number; top: number } | null>(null);
  // What the menu offered before (widget frames, safe areas, output view):
  // off, so a choice kept from then cannot stay with no way to undo it.
  useEffect(() => {
    if (view.frames || view.safe || (view.stageView && view.stageView !== 'program')) view.set({ frames: false, safe: false, editSafe: false, stageView: 'program' });
  }, []);
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
        <div className="menu ux2-dmenu" role="menu" style={{ position: 'fixed', left: open.left, top: open.top, maxHeight: `calc(100vh - ${open.top + 12}px)` }}>
          <div className="ux2-drow">
            <button className="ux2-dcheck" role="menuitemcheckbox" aria-checked={view.guides} onClick={() => view.set({ guides: !view.guides })}>
              <span className={`ux2-ck ${view.guides ? 'on' : ''}`}>{view.guides && <Check size={11} />}</span>
              <Ruler size={13} />
              <span>{t('Guides')}</span>
            </button>
          </div>
          <div className="ux2-dsub">
            <GuidesSettings />
          </div>
        </div>
      )}
    </div>
  );
}

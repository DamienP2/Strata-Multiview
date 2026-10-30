import { t } from '../i18n';

/** An on/off switch (outputs and inputs). */
export function OnSwitch({ on, onChange, label, mini }: { on: boolean; onChange: (on: boolean) => void; label?: string; mini?: boolean }) {
  return (
    <label className={`switch oc-switch ${mini ? 'mini' : ''}`} title={on ? t('On: click to switch off (kept in the settings, not started)') : t('Off: click to switch on')} onPointerDown={(e) => e.stopPropagation()}>
      <input type="checkbox" checked={on} onChange={(e) => onChange(e.target.checked)} />
      {label && <span>{label}</span>}
    </label>
  );
}

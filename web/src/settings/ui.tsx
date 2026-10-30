import { Save } from 'lucide-react';
import { t } from '../i18n';
import { InfoTip } from '../ui/InfoTip';

/** A titled block of settings. */
export function Card({ icon, title, children, actions }: { icon: React.ReactNode; title: string; children: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <section className="set-card">
      <header>
        {icon}
        <h2>{title}</h2>
        <div className="spacer" />
        {actions}
      </header>
      <div className="set-body">{children}</div>
    </section>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="set-field">
      <span className="set-label">
        {label}
        {hint && <InfoTip text={hint} />}
      </span>
      {children}
    </label>
  );
}

/** Appears at the bottom while there are unsaved changes. */
export function SaveBar({ dirty, saving, onSave, onDiscard }: { dirty: boolean; saving: boolean; onSave: () => void; onDiscard: () => void }) {
  return (
    <div className={`save-bar ${dirty ? 'show' : ''}`}>
      <span>{t('You have unsaved changes.')}</span>
      <div className="spacer" />
      <button className="btn" onClick={onDiscard}>
        {t('Discard')}
      </button>
      <button className="btn primary" disabled={saving} onClick={onSave}>
        <Save size={14} /> {saving ? t('Applying…') : t('Save & apply')}
      </button>
    </div>
  );
}

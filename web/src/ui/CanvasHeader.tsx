import { LayoutTemplate, X } from 'lucide-react';
import { t } from '../i18n';

/** Top of a node canvas (Logic, Data): what it is, for which template, and Close. */
export function CanvasHeader({ icon, title, subject, onClose, children }: { icon: React.ReactNode; title: string; subject?: string; onClose: () => void; children?: React.ReactNode }) {
  return (
    <header className="cv-head">
      <span className="cv-head-icon">{icon}</span>
      <h2>{title}</h2>
      {subject && (
        <span className="cv-head-subject">
          <LayoutTemplate size={12} />
          {subject}
        </span>
      )}
      <div className="spacer" />
      {children}
      <button className="icon-btn ghost" onClick={onClose} title={t('Close (Esc)')}>
        <X size={16} />
      </button>
    </header>
  );
}

/** An empty canvas: a small icon and a word, centred over it (as the Operator's empty areas). */
export function CanvasVoid({ icon: Icon, text }: { icon: React.ComponentType<{ size?: number }>; text: string }) {
  return (
    <div className="cv-void">
      <span className="cv-void-ic">
        <Icon size={20} />
      </span>
      <span>{text}</span>
    </div>
  );
}

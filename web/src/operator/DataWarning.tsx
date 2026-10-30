import { TriangleAlert } from 'lucide-react';
import { useState } from 'react';
import { createPortal } from 'react-dom';
import type { Template } from '../engine/types';
import { t } from '../i18n';
import { useSources } from '../data/dataStore';
import { issueLine, issuesOf, templateSources, warningCount } from '../data/dataRules';

/** Lines shown per source in the card. */
const MAX_LINES = 6;

/**
 * A graphic whose sources break their rules: an orange badge, and on hover
 * what is wrong. Only a warning: the graphic can still be taken.
 */
export function DataWarning({ template }: { template?: Template | null }) {
  const sources = useSources();
  const ids = templateSources(template);
  const bad = sources.filter((s) => ids.includes(s.id) && warningCount(issuesOf(s)) > 0);
  const [at, setAt] = useState<DOMRect | null>(null);
  if (!bad.length) return null;
  const n = bad.reduce((sum, s) => sum + warningCount(issuesOf(s)), 0);
  return (
    <span className="dr-badge" onMouseEnter={(e) => setAt(e.currentTarget.getBoundingClientRect())} onMouseLeave={() => setAt(null)} onClick={(e) => e.stopPropagation()}>
      <TriangleAlert size={11} /> {n}
      {at &&
        createPortal(
          // Above the badge, below it near the top of the screen.
          <div
            className="dr-tip"
            style={{
              right: Math.max(8, window.innerWidth - at.right),
              ...(at.top > 220 ? { bottom: window.innerHeight - at.top + 8 } : { top: at.bottom + 8 }),
            }}
          >
            {bad.map((s) => {
              const issues = issuesOf(s);
              return (
                <div key={s.id}>
                  <b>
                    <TriangleAlert size={12} /> {s.name}
                  </b>
                  {issues.slice(0, MAX_LINES).map((i, k) => (
                    <span key={k} className={i.row === null && i.message === 'sum' ? 'muted' : ''}>
                      {issueLine(i)}
                    </span>
                  ))}
                  {issues.length > MAX_LINES && <span className="muted">{t('{n} more', { n: issues.length - MAX_LINES })}</span>}
                </div>
              );
            })}
          </div>,
          document.body,
        )}
    </span>
  );
}

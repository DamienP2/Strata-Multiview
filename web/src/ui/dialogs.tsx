import { AlertTriangle, Info, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { create } from 'zustand';
import { t } from '../i18n';

/*
 * In-app confirmation, question and information boxes (instead of the
 * browser's confirm / prompt / alert), in the design of the other dialogs:
 * `if (!(await confirmDialog({ … }))) return;`.
 */

type Request =
  | { kind: 'confirm'; title: string; message?: React.ReactNode; confirm?: string; cancel?: string; danger?: boolean; resolve: (v: boolean) => void }
  | { kind: 'prompt'; title: string; message?: React.ReactNode; label?: string; value?: string; placeholder?: string; confirm?: string; password?: boolean; resolve: (v: string | null) => void }
  | { kind: 'alert'; title: string; message?: React.ReactNode; resolve: (v: void) => void };

const useDialogs = create<{ queue: Request[] }>(() => ({ queue: [] }));

const push = (r: Request) => useDialogs.setState((s) => ({ queue: [...s.queue, r] }));
const shift = () => useDialogs.setState((s) => ({ queue: s.queue.slice(1) }));

/** Asks to confirm; resolves true on the confirm button. */
export const confirmDialog = (o: { title: string; message?: React.ReactNode; confirm?: string; cancel?: string; danger?: boolean }) =>
  new Promise<boolean>((resolve) => push({ kind: 'confirm', ...o, resolve }));

/** Asks for a text; resolves null when cancelled (or empty). */
export const promptDialog = (o: { title: string; message?: React.ReactNode; label?: string; value?: string; placeholder?: string; confirm?: string; password?: boolean }) =>
  new Promise<string | null>((resolve) => push({ kind: 'prompt', ...o, resolve }));

/** Shows an information. */
export const alertDialog = (o: { title: string; message?: React.ReactNode }) => new Promise<void>((resolve) => push({ kind: 'alert', ...o, resolve }));

/** Mounted once by the app: shows the dialogs one after the other. */
export function DialogHost() {
  const r = useDialogs((s) => s.queue[0]);
  const [text, setText] = useState('');
  const input = useRef<HTMLInputElement>(null);
  const ok = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!r) return;
    if (r.kind === 'prompt') {
      setText(r.value ?? '');
      setTimeout(() => input.current?.select(), 0);
    } else setTimeout(() => ok.current?.focus(), 0);
  }, [r]);
  if (!r) return null;

  const close = (accept: boolean) => {
    shift();
    if (r.kind === 'confirm') r.resolve(accept);
    else if (r.kind === 'prompt') r.resolve(accept && text.trim() ? text.trim() : null);
    else r.resolve();
  };
  const Icon = r.kind === 'confirm' && r.danger ? AlertTriangle : Info;

  return (
    <div className="modal-back dialog-back" onPointerDown={() => close(false)}>
      <form
        className={`modal dialog ${r.kind === 'confirm' && r.danger ? 'danger' : ''}`}
        role={r.kind === 'alert' ? 'alertdialog' : 'dialog'}
        aria-labelledby="dialog-title"
        onPointerDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Escape') close(false);
        }}
        onSubmit={(e) => {
          e.preventDefault();
          close(true);
        }}
      >
        <header>
          <span className="dialog-icon">
            <Icon size={16} />
          </span>
          <h2 id="dialog-title">{r.title}</h2>
          <button type="button" className="icon-btn ghost" title={t('Close')} onClick={() => close(false)}>
            <X size={16} />
          </button>
        </header>
        {r.message && <div className="dialog-message">{r.message}</div>}
        {r.kind === 'prompt' && (
          <label className="set-field">
            {r.label && <span className="set-label">{r.label}</span>}
            <input ref={input} type={r.password ? 'password' : 'text'} autoComplete={r.password ? 'current-password' : undefined} value={text} placeholder={r.placeholder} onChange={(e) => setText(e.target.value)} />
          </label>
        )}
        <footer>
          {r.kind !== 'alert' && (
            <button type="button" className="btn" onClick={() => close(false)}>
              {(r.kind === 'confirm' && r.cancel) || t('Cancel')}
            </button>
          )}
          <button ref={ok} type="submit" className={`btn ${r.kind === 'confirm' && r.danger ? 'danger-solid' : 'primary'}`} disabled={r.kind === 'prompt' && !text.trim()}>
            {r.kind === 'alert' ? t('OK') : r.confirm || (r.kind === 'prompt' ? t('OK') : t('Confirm'))}
          </button>
        </footer>
      </form>
    </div>
  );
}

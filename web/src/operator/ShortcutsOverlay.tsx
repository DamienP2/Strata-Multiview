import { X } from 'lucide-react';
import { Fragment, memo, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { create } from 'zustand';
import { t } from '../i18n';
import { navigate } from '../router';
import { ACTIONS, actionLabel, BROADCAST, capLabel, effective, keyParts, useShortcutPrefs, type Binding } from './shortcuts';

/*
 * The user's keyboard shortcuts in a layer over the page (the Operator's
 * « ? » button and key, the account menu). Esc closes it.
 */

const useOverlay = create<{ open: boolean }>(() => ({ open: false }));
export const openShortcuts = () => useOverlay.setState({ open: true });
const close = () => useOverlay.setState({ open: false });

const mac = typeof navigator !== 'undefined' && /Mac|iP(hone|ad)/.test(navigator.platform || navigator.userAgent);

/** The keys worth showing: Ctrl or ⌘ variants of the same key only once (this computer's). */
function shown(keys: string[]): string[] {
  const other = mac ? 'Ctrl+' : 'Meta+';
  const mine = keys.filter((k) => !k.startsWith(other));
  return mine.length ? mine : keys;
}

/** The first key of an action in the user's bindings (`null`: none). */
export function useActionKey(action: string): string | null {
  const bindings = useShortcutPrefs((s) => s.bindings);
  const keys = effective(bindings).find((b) => b.action === action)?.keys ?? [];
  return shown(keys)[0] ?? null;
}

/** A key on a button: « F1 », « Ctrl Z ». */
export const KeyCap = memo(function KeyCap({ action }: { action: string }) {
  const key = useActionKey(action);
  if (!key) return null;
  return <kbd className="ux1-cap">{keyParts(key).map(capLabel).join(' ')}</kbd>;
});

function Keys({ k }: { k: string }) {
  const parts = keyParts(k);
  return (
    <span className="ux1-sc-key">
      {parts.map((p, i) => (
        <Fragment key={i}>
          {i > 0 && <span className="ux1-sc-plus">+</span>}
          <kbd>{capLabel(p)}</kbd>
        </Fragment>
      ))}
    </span>
  );
}

function Row({ label, keys }: { label: string; keys: React.ReactNode }) {
  return (
    <div className="ux1-sc-row">
      <span className="ux1-sc-label">{label}</span>
      <span className="ux1-sc-keys">{keys}</span>
    </div>
  );
}

/** Same bindings as the Broadcast profile (whatever the order). */
const broadcast = (b: Binding[]) => {
  const norm = (l: Binding[]) =>
    JSON.stringify(
      l
        .filter((x) => x.keys.length)
        .map((x) => [x.action, [...x.keys].sort()])
        .sort(),
    );
  return norm(b) === norm(BROADCAST);
};

/** Mounted once for the app: shows the layer when asked. */
export function ShortcutsOverlay() {
  const open = useOverlay((s) => s.open);
  return open ? <Overlay /> : null;
}

function Overlay() {
  const saved = useShortcutPrefs((s) => s.bindings);
  const loaded = useShortcutPrefs((s) => s.loaded);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!loaded) void useShortcutPrefs.getState().load();
  }, [loaded]);
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    box.current?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      close();
    };
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('keydown', key, true);
      before?.focus?.();
    };
  }, []);

  const bindings = effective(saved);
  const bound = bindings.filter((b) => b.keys.length);
  const group = (id: string) => ACTIONS.find((a) => a.id === id)?.group ?? 'list';
  // Take 1…9 on their digits: one row.
  const takes = bound.filter((b) => /^take:\d+$/.test(b.action));
  const digits = takes.length > 1 && takes.every((b) => b.keys.length === 1 && b.keys[0] === b.action.slice(5));
  const row = (b: Binding) => (
    <Row
      key={b.action}
      label={actionLabel(b.action)}
      keys={shown(b.keys).map((k) => (
        <Keys key={k} k={k} />
      ))}
    />
  );
  const inProject = location.pathname.startsWith('/project/');
  const edit = () => {
    close();
    if (inProject) window.open('/settings#shortcuts', '_blank', 'noopener');
    else navigate('/settings#shortcuts');
  };

  return createPortal(
    <div className="modal-back ux1-sc-back" onPointerDown={close}>
      <div ref={box} tabIndex={-1} className="modal ux1-sc" role="dialog" aria-modal="true" aria-label={t('Keyboard shortcuts')} onPointerDown={(e) => e.stopPropagation()}>
        <header>
          <h2>{t('Keyboard shortcuts')}</h2>
          <span className="ux1-sc-prof">{broadcast(bindings) ? t('Profile · Broadcast (F1-F12)') : t('Profile · Custom')}</span>
          <button className="btn small" onClick={edit}>
            {t('Edit')}
          </button>
          <button className="icon-btn ghost" title={t('Close (Esc)')} onClick={close}>
            <X size={15} />
          </button>
        </header>
        <div className="ux1-sc-cols">
          <section className="ux1-sc-card">
            <header>{t('On air')}</header>
            {bound.filter((b) => group(b.action) === 'air').map(row)}
          </section>
          <section className="ux1-sc-card">
            <header>{t('Playlist and zones')}</header>
            {bound.filter((b) => group(b.action) === 'list' && !(digits && takes.includes(b))).map(row)}
            {digits && (
              <Row
                label={t('Take graphic no. 1 to {n}', { n: takes.length })}
                keys={
                  <>
                    <Keys k={takes[0].keys[0]} />
                    <span className="ux1-sc-plus">…</span>
                    <Keys k={takes[takes.length - 1].keys[0]} />
                  </>
                }
              />
            )}
            <Row label={t('Show the shortcuts')} keys={<Keys k="?" />} />
          </section>
        </div>
      </div>
    </div>,
    document.body,
  );
}

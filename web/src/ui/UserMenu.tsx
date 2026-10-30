import { ChevronDown, Keyboard, Languages, LogOut, SunMoon } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { LANGS, setLang, t, tc, useLang, type Lang } from '../i18n';
import { openShortcuts } from '../operator/ShortcutsOverlay';
import { api, useApp } from '../store';
import { setTheme, useTheme, type ThemePref } from '../theme';

/** FR | EN: kept for the user on the server. */
function LangSwitch() {
  const lang = useLang((s) => s.lang);
  const pick = (l: Lang) => {
    if (l === lang) return;
    setLang(l);
    const { auth, toast } = useApp.getState();
    if (!auth?.user) return;
    useApp.setState({ auth: { ...auth, user: { ...auth.user, lang: l } } });
    api('PUT', '/api/me/lang', { lang: l }).catch((e) => toast((e as Error).message, 'error'));
  };
  return (
    <div className="lang-switch" role="group" aria-label={t('Language')}>
      {LANGS.map((l) => (
        <button key={l.id} className={lang === l.id ? 'on' : ''} aria-pressed={lang === l.id} title={l.name} onClick={() => pick(l.id)}>
          {l.label}
        </button>
      ))}
    </div>
  );
}

const THEME_LABELS: { id: ThemePref; label: string }[] = [
  { id: 'dark', label: 'Dark' },
  { id: 'light', label: 'Light' },
  { id: 'system', label: 'System' },
];

/** Dark | Light | System: kept for the user on the server. */
function ThemeSwitch() {
  const theme = useTheme((s) => s.theme);
  const pick = (th: ThemePref) => {
    if (th === theme) return;
    setTheme(th);
    const { auth, toast } = useApp.getState();
    if (!auth?.user) return;
    useApp.setState({ auth: { ...auth, user: { ...auth.user, theme: th } } });
    api('PUT', '/api/me/theme', { theme: th }).catch((e) => toast((e as Error).message, 'error'));
  };
  return (
    <div className="lang-switch theme-switch" role="group" aria-label={t('Theme')}>
      {THEME_LABELS.map((o) => (
        <button key={o.id} className={theme === o.id ? 'on' : ''} aria-pressed={theme === o.id} onClick={() => pick(o.id)}>
          {tc('theme', o.label)}
        </button>
      ))}
    </div>
  );
}

/**
 * The account in a top bar: the avatar (with the name when `named`), and its
 * menu — language, theme, keyboard shortcuts, sign out.
 */
export function UserMenu({ named }: { named?: boolean }) {
  const user = useApp((s) => s.auth?.user);
  const logout = useApp((s) => s.logout);
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    // The first entry, for the keyboard.
    menu.current?.querySelector<HTMLElement>('button')?.focus();
    const down = (e: PointerEvent) => !wrap.current?.contains(e.target as Node) && setOpen(false);
    window.addEventListener('pointerdown', down);
    return () => window.removeEventListener('pointerdown', down);
  }, [open]);

  if (!user) return null;
  const name = user.displayName || user.username;
  const initial = name.slice(0, 1).toUpperCase();
  const shut = (focus = true) => {
    setOpen(false);
    if (focus) btn.current?.focus();
  };
  const onKey = (e: React.KeyboardEvent) => {
    const items = [...(menu.current?.querySelectorAll<HTMLElement>('button') ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'Escape') (e.preventDefault(), e.stopPropagation(), shut());
    else if (e.key === 'ArrowDown') (e.preventDefault(), items[(at + 1) % items.length]?.focus());
    else if (e.key === 'ArrowUp') (e.preventDefault(), items[(at - 1 + items.length) % items.length]?.focus());
    else if (e.key === 'Home') (e.preventDefault(), items[0]?.focus());
    else if (e.key === 'End') (e.preventDefault(), items[items.length - 1]?.focus());
    else if (e.key === 'Tab') shut(false);
  };

  return (
    <div className="ux1-user" ref={wrap}>
      <button
        ref={btn}
        className={`ux1-user-btn ${named ? 'named' : ''} ${open ? 'on' : ''}`}
        aria-haspopup="menu"
        aria-expanded={open}
        title={`${name} · ${tc('role', user.role)}`}
        onClick={() => setOpen(!open)}
        onKeyDown={(e) => e.key === 'ArrowDown' && (e.preventDefault(), setOpen(true))}
      >
        <span className="avatar">{initial}</span>
        {named && (
          <>
            <span className="user-name">{name}</span>
            <ChevronDown size={13} />
          </>
        )}
      </button>
      {open && (
        <div ref={menu} className="menu ux1-umenu" role="menu" aria-label={name} onKeyDown={onKey}>
          <div className="ux1-who">
            <span className="avatar">{initial}</span>
            <span>
              <b>{name}</b>
              <small>{tc('role', user.role)}</small>
            </span>
          </div>
          <div className="menu-sep" />
          <div className="ux1-mrow">
            <Languages size={14} /> {t('Language')}
            <LangSwitch />
          </div>
          <div className="ux1-mrow">
            <SunMoon size={14} /> {t('Theme')}
            <ThemeSwitch />
          </div>
          <button
            className="ux1-mi"
            role="menuitem"
            onClick={() => {
              shut(false);
              openShortcuts();
            }}
          >
            <Keyboard size={14} /> {t('Keyboard shortcuts')}
            <kbd className="ux1-cap">?</kbd>
          </button>
          <div className="menu-sep" />
          <button className="ux1-mi danger" role="menuitem" onClick={() => (shut(false), void logout())}>
            <LogOut size={14} /> {t('Sign out')}
          </button>
        </div>
      )}
    </div>
  );
}

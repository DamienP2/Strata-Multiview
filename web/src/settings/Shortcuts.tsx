import { Plus, RotateCcw, Trash2, TriangleAlert, X } from 'lucide-react';
import { Fragment, useEffect, useMemo, useState } from 'react';
import { t } from '../i18n';
import { PageHead } from '../pages/SidePage';
import { useApp } from '../store';
import { ACTIONS, actionLabel, BROADCAST, BROWSER_KEYS, capLabel, conflicts, effective, keyOf, keyParts, useShortcutPrefs, type Binding } from '../operator/shortcuts';

/** The keys of one binding, as key caps. */
function Keys({ k }: { k: string }) {
  return (
    <>
      {keyParts(k).map((p, i) => (
        <kbd key={i}>{capLabel(p)}</kbd>
      ))}
    </>
  );
}

/** Same bindings, whatever the order of the actions and keys. */
const same = (a: Binding[], b: Binding[]) => {
  const norm = (l: Binding[]) =>
    JSON.stringify(
      l
        .filter((x) => x.keys.length)
        .map((x) => [x.action, [...x.keys].sort()])
        .sort(),
    );
  return norm(a) === norm(b);
};

/** Settings › You › Shortcuts: the user's keys in the Operator. */
export function ShortcutsPage() {
  const { bindings: saved, loaded, load, save } = useShortcutPrefs();
  const full = useApp((s) => s.full);
  const [rec, setRec] = useState<string | null>(null);
  const [adding, setAdding] = useState<'outZone' | 'trigger' | null>(null);
  const [name, setName] = useState('');
  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const current = effective(saved);
  // Every action of the app, then the ones named after a project.
  const list: Binding[] = useMemo(() => [...ACTIONS.map((a) => current.find((b) => b.action === a.id) ?? { action: a.id, keys: [] }), ...current.filter((b) => !ACTIONS.some((a) => a.id === b.action))], [current]);
  const twice = conflicts(list);
  const change = (fn: (l: Binding[]) => Binding[]) => void save(fn(list.map((b) => ({ ...b, keys: [...b.keys] }))));

  // Recording: the next key pressed (Esc cancels).
  useEffect(() => {
    if (!rec) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === 'Escape' && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) return setRec(null);
      const k = keyOf(e);
      if (!k) return;
      change((l) => l.map((b) => (b.action === rec && !b.keys.includes(k) ? { ...b, keys: [...b.keys, k] } : b)));
      setRec(null);
    };
    const cancel = () => setRec(null);
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('pointerdown', cancel);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('pointerdown', cancel);
    };
  });

  // Zone and trigger names of the projects opened in this session, to pick from.
  const names = useMemo(() => {
    const zones = new Set<string>();
    const triggers = new Set<string>();
    for (const p of Object.values(full)) {
      p.zones.forEach((z) => zones.add(z.name));
      for (const tpl of p.templates) for (const m of tpl.markers) if (m.resume?.type === 'trigger') triggers.add(m.resume.name);
    }
    return { outZone: [...zones].sort(), trigger: [...triggers].sort() };
  }, [full]);

  const addAction = () => {
    const n = name.trim();
    if (!adding || !n) return setAdding(null);
    const id = `${adding}:${n}`;
    if (!list.some((b) => b.action === id)) change((l) => [...l, { action: id, keys: [] }]);
    setAdding(null);
    setName('');
    setTimeout(() => setRec(id), 0);
  };

  const row = (b: Binding) => {
    const recording = rec === b.action;
    const taken = b.keys.filter((k) => twice.has(k));
    const browser = b.keys.filter((k) => BROWSER_KEYS.includes(k));
    const own = !ACTIONS.some((a) => a.id === b.action);
    return (
      <div key={b.action} className={`sch-sc-row ${recording ? 'rec' : ''}`}>
        <span className="sch-sc-label">{actionLabel(b.action)}</span>
        <span className="sch-sc-keys">
          {b.keys.map((k) => (
            <span key={k} className={`sch-sc-key ${twice.has(k) ? 'twice' : ''}`}>
              <Keys k={k} />
              <button className="sch-sc-del" title={t('Remove')} onClick={() => change((l) => l.map((x) => (x.action === b.action ? { ...x, keys: x.keys.filter((y) => y !== k) } : x)))}>
                <X size={11} />
              </button>
            </span>
          ))}
          {recording && <span className="sch-sc-rec">{t('Press a key…')}</span>}
        </span>
        <span className="sch-sc-acts">
          {own && !recording && (
            <button className="sch-sc-btn" title={t('Remove')} onClick={() => change((l) => l.filter((x) => x.action !== b.action))}>
              <Trash2 size={13} />
            </button>
          )}
          <button
            className="sch-sc-btn"
            title={recording ? t('Cancel') : t('Add a key')}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => setRec(recording ? null : b.action)}
          >
            {recording ? <X size={14} /> : <Plus size={14} />}
          </button>
        </span>
        {taken.map((k) => (
          <span key={`c-${k}`} className="sch-sc-warn">
            <TriangleAlert size={12} />{' '}
            {t('{key} is also bound to “{action}”', {
              key: keyParts(k).map(capLabel).join(' '),
              action: list
                .filter((x) => x.action !== b.action && x.keys.includes(k))
                .map((x) => actionLabel(x.action))
                .join(', '),
            })}
          </span>
        ))}
        {browser.map((k) => (
          <span key={`b-${k}`} className="sch-sc-warn">
            <TriangleAlert size={12} /> {t('{key} also reloads the page in the browser: Strata intercepts it while the Operator is open.', { key: keyParts(k).map(capLabel).join(' ') })}
          </span>
        ))}
      </div>
    );
  };

  const custom = !same(list, BROADCAST);
  return (
    <>
      <PageHead title={t('Keyboard shortcuts')}>
        <span className="sch-sc-profile">{custom ? t('Profile · Custom') : t('Profile · Broadcast (F1-F12)')}</span>
        <button className="btn" disabled={!custom && saved === null} title={t('Back to the Broadcast profile')} onClick={() => void save(null)}>
          <RotateCcw size={14} /> {t('Reset')}
        </button>
      </PageHead>
      {!loaded ? (
        <div className="loading">{t('Loading…')}</div>
      ) : (
        <div className="sch-sc-page">
          {(['air', 'list'] as const).map((g) => (
            <section key={g} className="sch-sc-card">
              <header>{g === 'air' ? t('On air') : t('Playlist and zones')}</header>
              {list.filter((b) => ACTIONS.find((a) => a.id === b.action)?.group === g).map(row)}
              {g === 'list' && (
                <>
                  {list.filter((b) => !ACTIONS.some((a) => a.id === b.action)).map(row)}
                  <div className="sch-sc-row sch-sc-add">
                    {adding ? (
                      <Fragment>
                        <input
                          autoFocus
                          list="sch-sc-names"
                          value={name}
                          placeholder={adding === 'outZone' ? t('Name of the zone') : t('Trigger name')}
                          onChange={(e) => setName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') addAction();
                            if (e.key === 'Escape') setAdding(null);
                          }}
                        />
                        <datalist id="sch-sc-names">
                          {names[adding].map((n) => (
                            <option key={n} value={n} />
                          ))}
                        </datalist>
                        <span className="sch-sc-acts">
                          <button className="btn small primary" disabled={!name.trim()} onClick={addAction}>
                            {t('Add')}
                          </button>
                          <button className="sch-sc-btn" title={t('Cancel')} onClick={() => setAdding(null)}>
                            <X size={14} />
                          </button>
                        </span>
                      </Fragment>
                    ) : (
                      <span className="sch-sc-addbtns">
                        <button className="btn small" onClick={() => (setAdding('outZone'), setName(''))}>
                          <Plus size={13} /> {t('OUT of a zone')}
                        </button>
                        <button className="btn small" onClick={() => (setAdding('trigger'), setName(''))}>
                          <Plus size={13} /> {t('Trigger')}
                        </button>
                      </span>
                    )}
                  </div>
                </>
              )}
            </section>
          ))}
        </div>
      )}
    </>
  );
}

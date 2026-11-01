// Salvos: several actions at once, run from big buttons, a keyboard key, an address (a Stream Deck,
// Bitfocus Companion) or at set times. Mirrors crates/strata-mv-server/src/salvos.rs.
import { Clock, Copy, Keyboard, Pencil, Play, Plus, Save, Trash2, Zap } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useInputs } from '../engine/liveInputs';
import { t } from '../i18n';
import { PageHead } from '../pages/SidePage';
import { Card, Field } from '../settings/ui';
import { api, can, useApp } from '../store';
import { friendlyError } from '../ui/serverError';
import './salvos.css';

type Action =
  | { type: 'multiview'; channel: string; projectId: string | null }
  | { type: 'generator'; channel: string; enabled: boolean }
  | { type: 'backup'; input: string; backup: boolean }
  | { type: 'ackAll' };

interface Schedule {
  time: string;
  days: number[];
}

export interface Salvo {
  id: string;
  name: string;
  color: string;
  key: string;
  actions: Action[];
  schedules: Schedule[];
}

const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
const COLORS = ['#3b82f6', '#22c55e', '#f59e0b', '#ef4444', '#a855f7', '#14b8a6', '#64748b'];

/** "Ctrl+Shift+F1" of a key press. */
function keyName(e: KeyboardEvent): string {
  const k = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (['Control', 'Shift', 'Alt', 'Meta'].includes(k)) return '';
  return [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Meta', k].filter(Boolean).join('+');
}

export function SalvosPage() {
  const user = useApp((s) => s.auth?.user);
  const channels = useApp((s) => s.channels);
  const projects = useApp((s) => s.projects);
  const toast = useApp((s) => s.toast);
  const inputs = useInputs();
  const admin = can.admin(user);
  const [list, setList] = useState<Salvo[] | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<Salvo[]>([]);
  const [ran, setRan] = useState<string>('');
  const load = useCallback(
    () =>
      api<Salvo[]>('GET', '/api/salvos')
        .then(setList)
        .catch(() => setList([])),
    [],
  );
  useEffect(() => void load(), [load]);

  const run = useCallback(
    async (s: Salvo) => {
      try {
        const r = await api<{ ok: boolean; errors: string[] }>('POST', `/api/salvos/${encodeURIComponent(s.id)}/run`);
        setRan(s.id);
        setTimeout(() => setRan(''), 800);
        if (r.ok) toast(t('« {name} » run', { name: s.name }), 'info');
        else toast(r.errors.join(' · '), 'error');
      } catch (e) {
        toast(friendlyError((e as Error).message), 'error');
      }
    },
    [toast],
  );

  // The keys of the salvos, while this page shows (not while typing).
  useEffect(() => {
    if (editing || !list) return;
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)) return;
      const k = keyName(e);
      const s = k && list.find((x) => x.key && x.key.toLowerCase() === k.toLowerCase());
      if (s) {
        e.preventDefault();
        void run(s);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [list, editing, run]);

  const save = async () => {
    try {
      await api('PUT', '/api/salvos', draft);
      setEditing(false);
      await load();
      toast(t('Salvos saved'), 'info');
    } catch (e) {
      toast(friendlyError((e as Error).message), 'error');
    }
  };
  const upd = (i: number, fn: (s: Salvo) => void) => setDraft((d) => d.map((s, j) => (j === i ? (fn((s = structuredClone(s))), s) : s)));
  const describe = (a: Action) => {
    const ch = (id: string) => channels.find((c) => c.id === id)?.name ?? id;
    switch (a.type) {
      case 'multiview':
        return `${ch(a.channel)} → ${a.projectId ? (projects.find((p) => p.id === a.projectId)?.name ?? a.projectId) : t('slate')}`;
      case 'generator':
        return `${ch(a.channel)} · ${a.enabled ? t('generator on') : t('generator off')}`;
      case 'backup':
        return `${inputs.find((i) => i.id === a.input)?.name ?? a.input} · ${a.backup ? t('its backup') : t('itself')}`;
      case 'ackAll':
        return t('acknowledge every fault');
    }
  };

  if (!list) return <div className="loading">{t('Loading…')}</div>;
  return (
    <div className="salvos-page">
      <PageHead title={t('Salvos')} sub={t('Several actions at once: from these buttons, a key of the keyboard, a Stream Deck (an address) or at set times.')}>
        {admin && !editing && (
          <button className="btn" onClick={() => (setDraft(structuredClone(list)), setEditing(true))}>
            <Pencil size={14} /> {t('Edit')}
          </button>
        )}
        {editing && (
          <>
            <button className="btn" onClick={() => setEditing(false)}>
              {t('Cancel')}
            </button>
            <button className="btn primary" onClick={() => void save()}>
              <Save size={14} /> {t('Save')}
            </button>
          </>
        )}
      </PageHead>

      {!editing && (
        <>
          <div className="salvo-grid">
            {list.map((s) => (
              <button key={s.id} className={`salvo-btn ${ran === s.id ? 'ran' : ''}`} style={{ ['--c' as string]: s.color || COLORS[0] }} onClick={() => void run(s)} disabled={!can.operate(user)}>
                <Zap size={18} />
                <b>{s.name}</b>
                <span>
                  {s.key && (
                    <kbd>
                      <Keyboard size={11} /> {s.key}
                    </kbd>
                  )}
                  {s.schedules.map((sc, i) => (
                    <kbd key={i}>
                      <Clock size={11} /> {sc.time}
                    </kbd>
                  ))}
                </span>
                <small>{s.actions.map(describe).join(' · ')}</small>
              </button>
            ))}
          </div>
          {!list.length && <p className="muted">{admin ? t('No salvo yet: Edit to make one.') : t('No salvo yet.')}</p>}
          {list.length > 0 && (
            <Card icon={<Copy size={16} />} title={t('Stream Deck, Companion, automation')}>
              <p className="muted">{t('Each salvo runs from this address (GET or POST), with an API key (System › API) of an operator:')}</p>
              {list.map((s) => (
                <p key={s.id}>
                  <code>
                    {location.origin}/api/salvos/{encodeURIComponent(s.id)}/run?key=smv_…
                  </code>
                </p>
              ))}
            </Card>
          )}
        </>
      )}

      {editing && (
        <>
          {draft.map((s, i) => (
            <Card
              key={i}
              icon={<Zap size={16} />}
              title={s.name || t('Salvo')}
              actions={
                <button className="icon-btn ghost" title={t('Remove')} onClick={() => setDraft((d) => d.filter((_, j) => j !== i))}>
                  <Trash2 size={14} />
                </button>
              }
            >
              <div className="set-grid">
                <Field label={t('Name')}>
                  <input value={s.name} onChange={(e) => upd(i, (x) => void (x.name = e.target.value))} />
                </Field>
                <Field label={t('Key')} hint={t('Press the key (F1, 1, Ctrl+Shift+A…): it runs the salvo while this page shows.')}>
                  <input
                    value={s.key}
                    placeholder={t('Press a key')}
                    readOnly
                    onKeyDown={(e) => {
                      e.preventDefault();
                      if (e.key === 'Backspace' || e.key === 'Delete') return upd(i, (x) => void (x.key = ''));
                      const k = keyName(e.nativeEvent);
                      if (k) upd(i, (x) => void (x.key = k));
                    }}
                  />
                </Field>
                <Field label={t('Colour')}>
                  <div className="salvo-colors">
                    {COLORS.map((c) => (
                      <button key={c} type="button" className={s.color === c ? 'on' : ''} style={{ background: c }} onClick={() => upd(i, (x) => void (x.color = c))} />
                    ))}
                  </div>
                </Field>
              </div>
              <h4 className="salvo-sub">{t('Actions, in order')}</h4>
              {s.actions.map((a, k) => (
                <div key={k} className="salvo-row">
                  <select
                    value={a.type}
                    onChange={(e) =>
                      upd(i, (x) => {
                        const ty = e.target.value as Action['type'];
                        const c = channels[0]?.id ?? 'main';
                        x.actions[k] =
                          ty === 'multiview'
                            ? { type: ty, channel: c, projectId: projects[0]?.id ?? null }
                            : ty === 'generator'
                              ? { type: ty, channel: c, enabled: true }
                              : ty === 'backup'
                                ? { type: ty, input: inputs[0]?.id ?? '', backup: true }
                                : { type: 'ackAll' };
                      })
                    }
                  >
                    <option value="multiview">{t('Multiview of a channel')}</option>
                    <option value="generator">{t('Test generator')}</option>
                    <option value="backup">{t('Input backup')}</option>
                    <option value="ackAll">{t('Acknowledge every fault')}</option>
                  </select>
                  {(a.type === 'multiview' || a.type === 'generator') && (
                    <select value={a.channel} onChange={(e) => upd(i, (x) => void ((x.actions[k] as { channel: string }).channel = e.target.value))}>
                      {channels.map((c) => (
                        <option key={c.id} value={c.id}>
                          {c.name}
                        </option>
                      ))}
                    </select>
                  )}
                  {a.type === 'multiview' && (
                    <select value={a.projectId ?? ''} onChange={(e) => upd(i, (x) => void ((x.actions[k] as { projectId: string | null }).projectId = e.target.value || null))}>
                      <option value="">{t('— slate —')}</option>
                      {projects.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  )}
                  {a.type === 'generator' && (
                    <select value={a.enabled ? 'on' : 'off'} onChange={(e) => upd(i, (x) => void ((x.actions[k] as { enabled: boolean }).enabled = e.target.value === 'on'))}>
                      <option value="on">{t('On')}</option>
                      <option value="off">{t('Off')}</option>
                    </select>
                  )}
                  {a.type === 'backup' && (
                    <>
                      <select value={a.input} onChange={(e) => upd(i, (x) => void ((x.actions[k] as { input: string }).input = e.target.value))}>
                        {inputs.map((n) => (
                          <option key={n.id} value={n.id}>
                            {n.name}
                          </option>
                        ))}
                      </select>
                      <select value={a.backup ? 'b' : 's'} onChange={(e) => upd(i, (x) => void ((x.actions[k] as { backup: boolean }).backup = e.target.value === 'b'))}>
                        <option value="b">{t('Its backup')}</option>
                        <option value="s">{t('This input')}</option>
                      </select>
                    </>
                  )}
                  <button className="icon-btn ghost" title={t('Remove')} onClick={() => upd(i, (x) => void x.actions.splice(k, 1))}>
                    <Trash2 size={14} />
                  </button>
                </div>
              ))}
              <button className="btn small" onClick={() => upd(i, (x) => void x.actions.push({ type: 'multiview', channel: channels[0]?.id ?? 'main', projectId: projects[0]?.id ?? null }))}>
                <Plus size={13} /> {t('Add an action')}
              </button>
              <h4 className="salvo-sub">{t('At set times')}</h4>
              {s.schedules.map((sc, k) => (
                <div key={k} className="salvo-row">
                  <input type="time" value={sc.time} onChange={(e) => upd(i, (x) => void (x.schedules[k].time = e.target.value))} />
                  <div className="salvo-days">
                    {DAYS.map((d, n) => (
                      <button
                        key={d}
                        type="button"
                        className={sc.days.includes(n + 1) ? 'on' : ''}
                        onClick={() =>
                          upd(i, (x) => {
                            const ds = x.schedules[k].days;
                            x.schedules[k].days = ds.includes(n + 1) ? ds.filter((v) => v !== n + 1) : [...ds, n + 1].sort();
                          })
                        }
                      >
                        {t(d)}
                      </button>
                    ))}
                  </div>
                  <span className="muted">{sc.days.length ? '' : t('every day')}</span>
                  <button className="icon-btn ghost" title={t('Remove')} onClick={() => upd(i, (x) => void x.schedules.splice(k, 1))}>
                    <Trash2 size={14} />
                  </button>
                </div>
              ))}
              <button className="btn small" onClick={() => upd(i, (x) => void x.schedules.push({ time: '08:00', days: [] }))}>
                <Clock size={13} /> {t('Add a time')}
              </button>
            </Card>
          ))}
          <button
            className="btn"
            onClick={() => setDraft((d) => [...d, { id: '', name: t('Salvo {n}', { n: d.length + 1 }), color: COLORS[d.length % COLORS.length], key: '', actions: [], schedules: [] }])}
          >
            <Plus size={14} /> {t('Add a salvo')}
          </button>
        </>
      )}
      {!editing && list.length > 0 && (
        <p className="muted salvo-hint">
          <Play size={12} /> {t('Keys work while this page shows; the times, on the server, in its time zone.')}
        </p>
      )}
    </div>
  );
}

import { Plus, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { locale, t, tc, tn } from '../i18n';
import { promptDialog } from '../ui/dialogs';
import { Dropdown } from '../ui/Dropdown';
import { save, type Action, type Desk, type Rules } from './socialStore';

/*
 * Rules of the project: the toxicity score and its two thresholds, the
 * filters (each with its action), trusted and blocked accounts.
 */

type FilterKey = 'banned' | 'links' | 'caps' | 'length' | 'emojis' | 'duplicates' | 'personal';

/** The actions a filter can take. */
const ACTIONS: Record<FilterKey, Action[]> = {
  banned: ['reject', 'check', 'mask'],
  links: ['reject', 'check', 'mask'],
  caps: ['fix', 'check', 'reject'],
  length: ['check', 'reject', 'fix'],
  emojis: ['fix', 'check', 'reject'],
  duplicates: ['reject', 'check'],
  personal: ['mask', 'check', 'reject'],
};

function actionLabel(a: Action): string {
  switch (a) {
    case 'reject':
      return t('Reject');
    case 'check':
      return t('To check');
    case 'fix':
      return t('Fix');
    case 'mask':
      return tc('action', 'Mask');
  }
}

const fmt = (n: number) => n.toLocaleString(locale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export function RulesPage({ projectId, desk }: { projectId: string; desk: Desk }) {
  const r = desk.rules;
  const set = (next: Rules) => void save(projectId, 'rules', next);
  const patch = <K extends keyof Rules>(k: K, v: Partial<Rules[K]>) => set({ ...r, [k]: { ...r[k], ...v } });
  const ask = async (title: string) => (await promptDialog({ title, confirm: t('Add') }))?.trim() || null;

  const filter = (k: FilterKey, name: string, detail: React.ReactNode) => (
    <div className="mod-rule">
      <Toggle on={r[k].enabled} onChange={(enabled) => patch(k, { enabled })} />
      <b>{name}</b>
      <div className="mod-rule-detail">{detail}</div>
      <Dropdown className="mod-action" value={r[k].action} options={ACTIONS[k].map((a) => ({ value: a, label: actionLabel(a) }))} onChange={(action) => patch(k, { action })} />
    </div>
  );

  return (
    <div className="mod-rules">
      <div className="mod-card">
        <div className="mod-card-head">{t('Toxicity')}</div>
        <div className="mod-rule">
          <Toggle on={r.toxicity.enabled} onChange={(enabled) => patch('toxicity', { enabled })} />
          <b>{t('Toxicity score')}</b>
          <div className="mod-rule-detail mod-tox">
            <div className="mod-tox-text">
              {t('Auto reject above')} <b>{fmt(r.toxicity.high)}</b> · {t('to check above')} <b>{fmt(r.toxicity.low)}</b>
            </div>
            <Range low={r.toxicity.low} high={r.toxicity.high} onChange={(low, high) => patch('toxicity', { low, high })} />
          </div>
          <span className="mod-chip static">{tn(desk.toxRejects, '{n} reject', '{n} rejects')}</span>
        </div>
      </div>

      <div className="mod-card">
        <div className="mod-card-head">{t('Filters')}</div>
        {filter(
          'banned',
          t('Banned words'),
          <Words
            items={r.banned.words}
            onChange={(words) => patch('banned', { words })}
            onAdd={() => ask(t('Banned word'))}
            first={
              <button className={`mod-word ${r.banned.builtin ? '' : 'off'}`} onClick={() => patch('banned', { builtin: !r.banned.builtin })}>
                {t('built-in insults ({n})', { n: desk.builtinWords })}
              </button>
            }
          />,
        )}
        {filter('links', t('Links'), <Words items={r.links.allow} label={(d) => t('except {domain}', { domain: d })} onChange={(allow) => patch('links', { allow })} onAdd={() => ask(t('Allowed domain'))} />)}
        {filter(
          'caps',
          t('Capitals'),
          <span>
            {t('more than')} <Num value={r.caps.percent} min={10} max={100} onChange={(percent) => patch('caps', { percent })} /> {t('% of the text')}
          </span>,
        )}
        {filter(
          'length',
          t('Length'),
          <span>
            <Num value={r.length.min} min={0} max={10000} onChange={(min) => patch('length', { min })} /> {t('to')} <Num value={r.length.max} min={1} max={10000} onChange={(max) => patch('length', { max })} /> {t('characters')}
          </span>,
        )}
        {filter(
          'emojis',
          t('Emojis'),
          <span>
            <Num value={r.emojis.max} min={0} max={100} onChange={(max) => patch('emojis', { max })} /> {t('at most')}
          </span>,
        )}
        {filter(
          'duplicates',
          t('Duplicates'),
          <span>
            {t('same text within')} <Num value={r.duplicates.minutes} min={1} max={1440} onChange={(minutes) => patch('duplicates', { minutes })} /> {t('min')}
          </span>,
        )}
        {filter('personal', t('Personal data'), <span>{t('phones · e-mails · addresses')}</span>)}
      </div>

      <div className="mod-card">
        <div className="mod-card-head">{t('Accounts')}</div>
        <div className="mod-rule">
          <Toggle on={r.trusted.enabled} onChange={(enabled) => patch('trusted', { enabled })} />
          <b>{t('Trusted accounts')}</b>
          <div className="mod-rule-detail">
            <Words items={r.trusted.accounts} onChange={(accounts) => patch('trusted', { accounts })} onAdd={() => ask(t('Trusted account'))} />
          </div>
          <span className="mod-chip static">{t('Auto approve')}</span>
        </div>
        <div className="mod-rule">
          <Toggle on={r.blocked.enabled} onChange={(enabled) => patch('blocked', { enabled })} />
          <b>{t('Blocked accounts')}</b>
          <div className="mod-rule-detail">
            <Words items={r.blocked.accounts} onChange={(accounts) => patch('blocked', { accounts })} onAdd={() => ask(t('Blocked account'))} />
          </div>
          <span className="mod-chip static">{t('Reject')}</span>
        </div>
      </div>
    </div>
  );
}

function Toggle({ on, onChange }: { on: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="switch mod-toggle">
      <input type="checkbox" checked={on} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

/** Words as chips (removed with ×), and "+ add". */
function Words({ items, label = (s) => s, onChange, onAdd, first }: { items: string[]; label?: (s: string) => string; onChange: (items: string[]) => void; onAdd: () => Promise<string | null>; first?: React.ReactNode }) {
  return (
    <div className="mod-words">
      {first}
      {items.map((w) => (
        <span key={w} className="mod-word">
          {label(w)}
          <button title={t('Remove')} onClick={() => onChange(items.filter((x) => x !== w))}>
            <X size={10} />
          </button>
        </span>
      ))}
      <button
        className="mod-word add"
        onClick={async () => {
          const w = await onAdd();
          if (w && !items.includes(w)) onChange([...items, w]);
        }}
      >
        <Plus size={10} /> {t('add')}
      </button>
    </div>
  );
}

/** A small number, saved when left. */
function Num({ value, min, max, onChange }: { value: number; min: number; max: number; onChange: (n: number) => void }) {
  const [v, setV] = useState(String(value));
  useEffect(() => setV(String(value)), [value]);
  const done = () => {
    const n = Math.round(Number(v));
    if (!Number.isFinite(n)) return setV(String(value));
    const c = Math.min(max, Math.max(min, n));
    setV(String(c));
    if (c !== value) onChange(c);
  };
  return <input className="mod-num" type="number" min={min} max={max} value={v} onChange={(e) => setV(e.target.value)} onBlur={done} onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()} />;
}

/** Two handles on 0..1: "to check" from `low`, rejected from `high`. */
function Range({ low, high, onChange }: { low: number; high: number; onChange: (low: number, high: number) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<{ which: 'low' | 'high'; low: number; high: number } | null>(null);
  const shown = drag ?? { low, high };
  const at = (x: number) => {
    const r = ref.current!.getBoundingClientRect();
    return Math.round(Math.min(1, Math.max(0, (x - r.left) / r.width)) * 100) / 100;
  };
  const move = (e: React.PointerEvent) => {
    if (!drag) return;
    const v = at(e.clientX);
    setDrag(drag.which === 'low' ? { ...drag, low: Math.min(v, drag.high) } : { ...drag, high: Math.max(v, drag.low) });
  };
  const start = (which: 'low' | 'high') => (e: React.PointerEvent) => {
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setDrag({ which, low, high });
  };
  const end = () => {
    if (drag && (drag.low !== low || drag.high !== high)) onChange(drag.low, drag.high);
    setDrag(null);
  };
  const key = (which: 'low' | 'high') => (e: React.KeyboardEvent) => {
    const d = e.key === 'ArrowRight' || e.key === 'ArrowUp' ? 0.01 : e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -0.01 : 0;
    if (!d) return;
    e.preventDefault();
    const r = (n: number) => Math.round(Math.min(1, Math.max(0, n)) * 100) / 100;
    if (which === 'low') onChange(Math.min(r(low + d), high), high);
    else onChange(low, Math.max(r(high + d), low));
  };
  return (
    <div className="mod-range" ref={ref} onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
      <div className="mod-range-track" style={{ ['--lo' as string]: `${shown.low * 100}%`, ['--hi' as string]: `${shown.high * 100}%` }} />
      <span className="mod-range-knob" role="slider" tabIndex={0} aria-valuenow={shown.low} title={t('To check')} style={{ left: `${shown.low * 100}%` }} onPointerDown={start('low')} onKeyDown={key('low')} />
      <span className="mod-range-knob" role="slider" tabIndex={0} aria-valuenow={shown.high} title={t('Reject')} style={{ left: `${shown.high * 100}%` }} onPointerDown={start('high')} onKeyDown={key('high')} />
    </div>
  );
}

import { Check, Plus, Trash2, TriangleAlert } from 'lucide-react';
import { useState } from 'react';
import type { DataSourceInfo } from '../engine/types';
import { t, tn } from '../i18n';
import { type DataIssue, type DataRule, invalidRows, issuesOf, num, warningCount } from './dataRules';

const TYPES: { type: DataRule['type']; label: string }[] = [
  { type: 'text', label: 'Text' },
  { type: 'number', label: 'Number' },
  { type: 'list', label: 'List' },
  { type: 'image', label: 'Image' },
  { type: 'date', label: 'Date' },
  { type: 'calc', label: 'Calc' },
];

/** The Source | Rules tabs of a source's editor. */
export function RulesTabs({ tab, onTab, count }: { tab: 'source' | 'rules'; onTab: (t: 'source' | 'rules') => void; count: number }) {
  return (
    <div className="dr-tabs">
      <button className={tab === 'source' ? 'on' : ''} onClick={() => onTab('source')}>
        {t('Source')}
      </button>
      <button className={tab === 'rules' ? 'on' : ''} onClick={() => onTab('rules')}>
        {t('Rules')}
        {count > 0 && <em>{count}</em>}
      </button>
    </div>
  );
}

/** Under a source's name in the list: its warnings, if any. */
export function WarnLine({ source }: { source: DataSourceInfo }) {
  const issues = issuesOf(source);
  const rows = invalidRows(issues);
  return (
    <span className="dr-warn-line">
      <TriangleAlert size={11} /> {rows ? tn(rows, '{n} invalid row', '{n} invalid rows') : tn(issues.length, '{n} problem', '{n} problems')}
    </span>
  );
}

export const hasWarnings = (s: DataSourceInfo) => warningCount(issuesOf(s)) > 0;

/** The sums off target, under the Value table (`Σ score = 191,7`). */
export function SumIssues({ issues }: { issues: DataIssue[] }) {
  const sums = issues.filter((i) => i.message === 'sum');
  if (!sums.length) return null;
  return (
    <div className="dr-sums">
      {sums.map((i, n) => (
        <span
          key={n}
          title={t('Expected {n}', {
            n: `${num(i.params?.expected)} ± ${num(i.params?.tolerance)}`,
          })}
        >
          Σ {i.field} = {num(i.params?.sum)}
        </span>
      ))}
    </div>
  );
}

const isObj = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);

/** Names to pick: the columns of the value's main list, and its other values. */
function fieldsOf(value: unknown): string[] {
  const out = new Set<string>();
  const flat = (o: Record<string, unknown>, prefix: string, depth: number, rows: boolean) => {
    for (const [k, x] of Object.entries(o)) {
      const name = prefix ? `${prefix}.${k}` : k;
      if (isObj(x) && depth < 3) flat(x, name, depth + 1, rows);
      else if (Array.isArray(x)) {
        if (!rows) x.slice(0, 50).forEach((r) => isObj(r) && flat(r, '', 0, true));
      } else out.add(name);
    }
  };
  if (Array.isArray(value)) value.slice(0, 50).forEach((r) => isObj(r) && flat(r, '', 0, true));
  else if (isObj(value)) flat(value, '', 0, false);
  return [...out];
}

/** The rule in a few words. */
function ruleText(r: DataRule): string {
  const has = (n: number | undefined): n is number => n !== undefined && n !== null;
  const parts: string[] = [];
  switch (r.type) {
    case 'text':
      if (has(r.minLength) && has(r.maxLength))
        parts.push(
          t('{min} to {max} characters', {
            min: r.minLength,
            max: r.maxLength,
          }),
        );
      else if (has(r.minLength)) parts.push(t('at least {n} characters', { n: r.minLength }));
      else if (has(r.maxLength)) parts.push(t('at most {n} characters', { n: r.maxLength }));
      if (r.pattern) parts.push(`/${r.pattern}/`);
      break;
    case 'number':
      if (has(r.min) && has(r.max)) parts.push(t('{min} to {max}', { min: num(r.min), max: num(r.max) }));
      else if (has(r.min)) parts.push(`≥ ${num(r.min)}`);
      else if (has(r.max)) parts.push(`≤ ${num(r.max)}`);
      if (has(r.decimals)) parts.push(tn(r.decimals, '{n} decimal', '{n} decimals'));
      break;
    case 'list':
      parts.push((r.values ?? []).join(' · '));
      break;
    case 'image':
      parts.push(t('exists in the Library'));
      break;
    case 'date':
      parts.push(r.format || 'ISO 8601');
      break;
    case 'calc':
      if (has(r.equals)) parts.push(`Σ ${r.field} = ${num(r.equals)}${r.tolerance ? ` ± ${num(r.tolerance)}` : ''}`);
      break;
  }
  return parts.filter(Boolean).join(' · ');
}

/** A number input; empty = unset. */
function NumIn({ value, onChange, disabled, placeholder }: { value?: number; onChange: (v: number | undefined) => void; disabled: boolean; placeholder?: string }) {
  return <input type="number" className="dr-num" value={value ?? ''} placeholder={placeholder} disabled={disabled} onChange={(e) => onChange(e.target.value === '' ? undefined : +e.target.value)} />;
}

/** A rule's own settings, under its row. */
function RuleParams({ r, set, disabled }: { r: DataRule; set: (p: Partial<DataRule>) => void; disabled: boolean }) {
  switch (r.type) {
    case 'text':
      return (
        <div className="dr-params">
          <NumIn value={r.minLength} disabled={disabled} placeholder={t('min')} onChange={(v) => set({ minLength: v })} />
          <span>{t('to')}</span>
          <NumIn value={r.maxLength} disabled={disabled} placeholder={t('max')} onChange={(v) => set({ maxLength: v })} />
          <span>{t('characters')}</span>
          <input className="dr-wide mono" placeholder={t('Pattern (regular expression)')} value={r.pattern ?? ''} disabled={disabled} onChange={(e) => set({ pattern: e.target.value || undefined })} />
        </div>
      );
    case 'number':
      return (
        <div className="dr-params">
          <NumIn value={r.min} disabled={disabled} placeholder={t('min')} onChange={(v) => set({ min: v })} />
          <span>{t('to')}</span>
          <NumIn value={r.max} disabled={disabled} placeholder={t('max')} onChange={(v) => set({ max: v })} />
          <span>·</span>
          <NumIn
            value={r.decimals}
            disabled={disabled}
            placeholder="0"
            onChange={(v) =>
              set({
                decimals: v === undefined ? undefined : Math.max(0, Math.round(v)),
              })
            }
          />
          <span>{t('decimals at most')}</span>
        </div>
      );
    case 'list':
      return (
        <div className="dr-params">
          <input
            className="dr-wide"
            placeholder={t('Allowed values, separated by commas')}
            defaultValue={(r.values ?? []).join(', ')}
            disabled={disabled}
            onBlur={(e) =>
              set({
                values: e.target.value
                  .split(',')
                  .map((v) => v.trim())
                  .filter(Boolean),
              })
            }
          />
        </div>
      );
    case 'date':
      return (
        <div className="dr-params">
          <input className="dr-wide mono" placeholder="DD/MM/YYYY HH:mm · ISO 8601" value={r.format ?? ''} disabled={disabled} onChange={(e) => set({ format: e.target.value || undefined })} />
        </div>
      );
    case 'calc':
      return (
        <div className="dr-params">
          <span>Σ {r.field || '…'} =</span>
          <NumIn value={r.equals} disabled={disabled} placeholder="100" onChange={(v) => set({ equals: v })} />
          <span>±</span>
          <NumIn value={r.tolerance} disabled={disabled} placeholder="0" onChange={(v) => set({ tolerance: v })} />
        </div>
      );
    default:
      return null;
  }
}

/** The rules of a source: a table to edit, saved with the source. */
export function SourceRules({ rules, value, editable, onChange }: { rules: DataRule[]; value: unknown; editable: boolean; onChange: (rules: DataRule[]) => void }) {
  const [open, setOpen] = useState<number | null>(null);
  const fields = fieldsOf(value);
  const set = (i: number, p: Partial<DataRule>) => onChange(rules.map((r, n) => (n === i ? { ...r, ...p } : r)));
  return (
    <div className="dr-rules">
      <datalist id="dr-fields">
        {fields.map((f) => (
          <option key={f} value={f} />
        ))}
      </datalist>
      <table className="dr-table">
        <thead>
          <tr>
            <th>{t('Field')}</th>
            <th>{t('Type')}</th>
            <th>{t('Required')}</th>
            <th>{t('Rule')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {rules.map((r, i) => (
            <RuleRow key={i} r={r} open={open === i} editable={editable} onOpen={() => setOpen(open === i ? null : i)} set={(p) => set(i, p)} onDelete={() => (setOpen(null), onChange(rules.filter((_, n) => n !== i)))} />
          ))}
          {editable && (
            <tr className="dr-add">
              <td colSpan={5}>
                <button
                  className="link-btn"
                  onClick={() => {
                    onChange([
                      ...rules,
                      {
                        field: fields.find((f) => !rules.some((r) => r.field === f)) ?? '',
                        type: 'text',
                        required: false,
                      },
                    ]);
                    setOpen(rules.length);
                  }}
                >
                  <Plus size={12} /> {t('Rule')}
                </button>
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

function RuleRow({ r, open, editable, onOpen, set, onDelete }: { r: DataRule; open: boolean; editable: boolean; onOpen: () => void; set: (p: Partial<DataRule>) => void; onDelete: () => void }) {
  const disabled = !editable;
  return (
    <>
      <tr className={open ? 'on' : ''}>
        <td>
          <input className="mono" list="dr-fields" value={r.field} disabled={disabled} placeholder="score" onChange={(e) => set({ field: e.target.value })} />
        </td>
        <td>
          <select value={r.type} disabled={disabled} onChange={(e) => set({ type: e.target.value as DataRule['type'] })}>
            {TYPES.map((k) => (
              <option key={k.type} value={k.type}>
                {t(k.label)}
              </option>
            ))}
          </select>
        </td>
        <td className="dr-req">{r.type !== 'calc' && (editable ? <input type="checkbox" checked={!!r.required} onChange={(e) => set({ required: e.target.checked })} /> : r.required && <Check size={13} />)}</td>
        <td className="dr-sum" onClick={r.type === 'image' ? undefined : onOpen}>
          {ruleText(r) || <span className="muted">{r.type === 'image' ? '' : t('Set…')}</span>}
        </td>
        <td>
          {editable && (
            <button className="icon-btn" title={t('Delete')} onClick={onDelete}>
              <Trash2 size={13} />
            </button>
          )}
        </td>
      </tr>
      {open && r.type !== 'image' && (
        <tr className="dr-edit">
          <td colSpan={5}>
            <RuleParams r={r} set={set} disabled={disabled} />
          </td>
        </tr>
      )}
    </>
  );
}

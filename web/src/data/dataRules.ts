import type { DataSourceInfo, Template } from '../engine/types';
import { locale, t } from '../i18n';

/** A validation rule of a source (mirror of datarules::Rule). */
export interface DataRule {
  /** `score`: that value, else that column of every row of the main list; `rows[].score`: of that list. */
  field: string;
  type: 'text' | 'number' | 'list' | 'image' | 'date' | 'calc';
  required?: boolean;
  minLength?: number;
  maxLength?: number;
  pattern?: string;
  min?: number;
  max?: number;
  decimals?: number;
  values?: string[];
  format?: string;
  equals?: number;
  tolerance?: number;
}

/** A broken rule, from the server (mirror of datarules::Issue). */
export interface DataIssue {
  /** The list of the row ("" = the value itself). */
  list?: string;
  row: number | null;
  field: string;
  value: string;
  message: string;
  params?: Record<string, number | string>;
}

/** A source with its rules and the issues of its value. */
export type RuledSource = DataSourceInfo & {
  rules?: DataRule[];
  issues?: DataIssue[];
};

export const issuesOf = (s: DataSourceInfo | null | undefined): DataIssue[] => (s as RuledSource | null | undefined)?.issues ?? [];

/** Rows with issues, else issues (as the server counts them). */
export function warningCount(issues: DataIssue[]): number {
  const rows = new Set(issues.filter((i) => i.row !== null).map((i) => `${i.list ?? ''}#${i.row}`));
  return rows.size || issues.length;
}

export const invalidRows = (issues: DataIssue[]) => new Set(issues.filter((i) => i.row !== null).map((i) => `${i.list ?? ''}#${i.row}`)).size;

/** A number as people read it here (`191,7`). */
export const num = (v: number | string | undefined) => (typeof v === 'number' ? v.toLocaleString(locale(), { maximumFractionDigits: 6 }) : (v ?? ''));

/** What is wrong, in a few words (without the row). */
export function issueText(i: DataIssue): string {
  const p = { field: i.field, value: i.value, limit: num(i.params?.limit) };
  switch (i.message) {
    case 'required':
      return t('{field} missing', p);
    case 'minLength':
      return t('{field} “{value}” under {limit} characters', p);
    case 'maxLength':
      return t('{field} “{value}” over {limit} characters', p);
    case 'pattern':
      return t('{field} “{value}” does not match the pattern', p);
    case 'number':
      return t('{field} “{value}” is not a number', p);
    case 'min':
      return t('{field} {value} < {limit}', p);
    case 'max':
      return t('{field} {value} > {limit}', p);
    case 'decimals':
      return t('{field} {value}: more than {limit} decimals', p);
    case 'list':
      return t('{field} “{value}” not in the list', p);
    case 'image':
      return t('{field} “{value}” not found', p);
    case 'date':
      return t('{field} “{value}” is not a date', p);
    case 'sum':
      return `Σ ${i.field} = ${num(i.params?.sum)}`;
    default:
      return `${i.field} “${i.value}”`;
  }
}

/** One line of a list of issues: `Row 2 · parti “RN ?” not in the list`. */
export const issueLine = (i: DataIssue) => (i.row === null ? issueText(i) : t('Row {n} · {issue}', { n: i.row + 1, issue: issueText(i) }));

/** Is this cell of the table (`list`, row, column) or this value (row null) wrong? */
export const isBad = (issues: DataIssue[], list: string | null, row: number | null, field: string) => issues.some((i) => i.message !== 'sum' && i.row === row && i.field === field && (row === null || (i.list ?? '') === list));

/** The sources a template reads (mirror of datapath::sources). */
export function templateSources(tpl: Template | null | undefined): string[] {
  if (!tpl) return [];
  return [...new Set([...tpl.fields.flatMap((f) => (f.binding?.source ? [f.binding.source] : [])), ...(tpl.datasets ?? []).map((d) => d.source)])];
}

import { useEffect, useState } from 'react';
import { create } from 'zustand';
import { columnKey, countKey, tableKey, linesKey, rowKey, type Data, type DataFilter, type DataSet, type DataSort, type DataSourceInfo, type DataStatus, type DataStep, type FieldBinding, type Template } from '../engine/types';
import { api } from '../store';
import { t, tn } from '../i18n';

/** Data sources, their status (pushed live) and a version per source. */
interface DataState {
  sources: DataSourceInfo[];
  loaded: boolean;
  /** Bumped when a source's value changes, to refresh lookups. */
  versions: Record<string, number>;
  load: () => Promise<void>;
  onStatus: (id: string, status: DataStatus) => void;
}

export const useData = create<DataState>((set, get) => ({
  sources: [],
  loaded: false,
  versions: {},
  async load() {
    const sources = await api<DataSourceInfo[]>('GET', '/api/data/sources');
    set({ sources, loaded: true });
  },
  onStatus(id, status) {
    const prev = get().sources.find((s) => s.id === id);
    const last = lastUpdate.has(id) ? lastUpdate.get(id) : prev?.updatedAt;
    lastUpdate.set(id, status.updatedAt);
    if (last !== status.updatedAt) set({ versions: { ...get().versions, [id]: (get().versions[id] ?? 0) + 1 } });
    // The list is fetched when first used: it will have this status.
    if (!get().loaded) return;
    if (!prev) {
      // A source the list does not have (new, or of a project not shown): the list again, once.
      if (!reloaded.has(id)) {
        reloaded.add(id);
        void get().load().catch(() => {});
      }
      return;
    }
    // The same status again (a source still failing): nothing to show.
    const same = (Object.keys(status) as (keyof DataSourceInfo)[]).every((k) => JSON.stringify(prev[k]) === JSON.stringify(status[k as keyof DataStatus]));
    if (same) return;
    set({ sources: get().sources.map((s) => (s.id === id ? { ...s, ...status } : s)) });
  },
}));

/** Last `updatedAt` seen of each source (its value changed when it moves). */
const lastUpdate = new Map<string, number | null | undefined>();
/** Sources the list was fetched again for. */
const reloaded = new Set<string>();

/** Sources, loaded on first use: the shared ones and those of the projects the user sees. */
export function useSources() {
  const { sources, loaded, load } = useData();
  useEffect(() => {
    if (!loaded) void load().catch(() => {});
  }, [loaded, load]);
  return sources;
}

/** The sources a project can use: its own and the shared ones (shared last). */
export function useProjectSources(projectId: string | null | undefined) {
  const all = useSources();
  return [...all.filter((s) => projectId && s.project === projectId), ...all.filter((s) => !s.project)];
}

export async function lookup(source: string, path: string, data: Data, filters?: DataFilter[], sort?: DataSort): Promise<string | null> {
  try {
    const r = await api<{ value: string | null }>('POST', '/api/data/lookup', { source, path, data, filters: filters ?? [], sort: sort?.by ? sort : undefined });
    return r.value;
  } catch {
    return null;
  }
}

/** A table: the source's rows, or the rows after a step. */
export interface Stage {
  columns: string[];
  rows: string[][];
  /** Rows before the cut to 200. */
  total: number;
}

/** Runs a table pipeline on a source's value: the table at each stage, and the text it gives. */
export async function pipeline(source: string, list: string, steps: DataStep[], data: Data): Promise<{ stages: Stage[]; text: string } | null> {
  try {
    return await api('POST', '/api/data/pipeline', { source, list, steps, data });
  } catch {
    return null;
  }
}

/** A table binding (list and steps), else a path. */
export const isTable = (b: FieldBinding) => b.list !== undefined || !!b.steps?.length;

/** A short description of where a binding reads (shown: called when drawing). */
export const bindingLabel = (b: FieldBinding) => (isTable(b) ? `${b.list || t('root')}${b.steps?.length ? ` · ${tn(b.steps.length, '{n} step', '{n} steps')}` : ''}` : b.path || t('(no path)'));

/** What a binding gives now. */
export async function readBinding(b: FieldBinding, data: Data): Promise<string | null> {
  if (isTable(b)) return (await pipeline(b.source, b.list ?? '', b.steps ?? [], data))?.text ?? null;
  return lookup(b.source, b.path, data, b.filters, b.sort);
}

/** A row as one line of text: its text / title / headline column, else its cells joined (as the server). */
export function rowLine(columns: string[], row: string[]): string {
  const i = columns.findIndex((c) => ['text', 'title', 'headline'].includes(c.trim().toLowerCase()));
  return i >= 0 ? (row[i] ?? '') : row.map((c) => c.trim()).filter(Boolean).join(' · ');
}

/** The columns of the current row of a data set (and its number of rows), as the server fills them. */
export async function readDataset(ds: DataSet, data: Data): Promise<Data | null> {
  const r = await pipeline(ds.source, ds.list, ds.steps ?? [], data);
  const t = r?.stages[r.stages.length - 1];
  if (!t) return null;
  const n = t.total;
  const row = Math.min(Math.max(1, parseInt(data[rowKey(ds.key)] ?? '1', 10) || 1), Math.max(1, n));
  const line = (r: string[]) => rowLine(t.columns, r);
  // The chosen row only.
  const lines = t.rows[row - 1] ? line(t.rows[row - 1]) : '';
  const out: Data = { [countKey(ds.key)]: String(n), [tableKey(ds.key)]: JSON.stringify([t.columns, ...t.rows]), [linesKey(ds.key)]: lines };
  t.columns.forEach((c, i) => (out[columnKey(ds.key, c)] = t.rows[row - 1]?.[i] ?? ''));
  return out;
}

/**
 * `data` with the template's bound fields filled from their sources, like the
 * server does on air (previews in the designer and the operator).
 */
export function useBoundData(template: Template | null | undefined, data: Data): Data {
  const versions = useData((s) => s.versions);
  const bound = template?.fields.filter((f) => f.binding?.source && (f.binding.path || isTable(f.binding))) ?? [];
  const sets = template?.datasets ?? [];
  const key = JSON.stringify([bound.map((f) => [f.key, f.binding]), sets, data, bound.map((f) => versions[f.binding!.source] ?? 0), sets.map((d) => versions[d.source] ?? 0)]);
  const [values, setValues] = useState<Record<string, string>>({});
  useEffect(() => {
    if (!bound.length && !sets.length) {
      setValues({});
      return;
    }
    let live = true;
    const h = setTimeout(async () => {
      const out: Record<string, string> = {};
      await Promise.all(
        bound.map(async (f) => {
          const v = await readBinding(f.binding!, data);
          if (v !== null) out[f.key] = v;
        }),
      );
      for (const r of await Promise.all(sets.map((d) => readDataset(d, data)))) if (r) Object.assign(out, r);
      // A value typed by hand over the data stays.
      for (const k of Object.keys(out)) if (data[`${k}__manual`]?.trim()) delete out[k];
      if (live) setValues(out);
    }, 150);
    return () => {
      live = false;
      clearTimeout(h);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return Object.keys(values).length ? { ...data, ...values } : data;
}

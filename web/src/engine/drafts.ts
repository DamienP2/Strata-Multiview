import type { Project, Template } from './types';

/*
 * Templates exist twice: `project.templates` is what the operator, the
 * outputs and automation use (published); `project.drafts` holds the
 * designer's unpublished changes and new templates.
 */

/** The template as the designer sees it: its draft, else the published one. */
export function draftOf(p: Project, id: string): Template | undefined {
  return p.drafts?.find((t) => t.id === id) ?? p.templates.find((t) => t.id === id);
}

/** Every template the designer lists, in order: published (or their draft), then new ones. */
export function designTemplates(p: Project): Template[] {
  const drafts = new Map((p.drafts ?? []).map((t) => [t.id, t]));
  const out = p.templates.map((t) => drafts.get(t.id) ?? t);
  for (const d of p.drafts ?? []) if (!p.templates.some((t) => t.id === d.id)) out.push(d);
  // In the Designer's order (a template missing from it keeps its place after the others).
  const at = new Map((p.designOrder ?? []).map((id, i) => [id, i]));
  return at.size ? out.map((t, i) => ({ t, i: at.get(t.id) ?? at.size + i })).sort((a, b) => a.i - b.i).map((x) => x.t) : out;
}

/** Publication state of a template. */
export function publishState(p: Project, id: string): { published?: Template; draft: boolean } {
  return { published: p.templates.find((t) => t.id === id), draft: !!p.drafts?.some((t) => t.id === id) };
}

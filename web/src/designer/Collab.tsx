import { Check, Lock, RotateCcw, Upload } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { draftOf, publishState } from '../engine/drafts';
import type { ChannelState, Project } from '../engine/types';
import { api, clientId, lockEvents, useApp, type EditLock } from '../store';
import { useDesigner } from './designerStore';
import { confirmDialog } from '../ui/dialogs';
import { t, tn } from '../i18n';

/*
 * One designer at a time per template (edit lock, renewed while the template
 * is open), and drafts that reach the operator only when published.
 */

const RENEW_MS = 15_000;

type LockState = { status: 'pending' } | { status: 'mine' } | { status: 'held'; holder: EditLock };

const lockUrl = (p: string, t: string) => `/api/projects/${encodeURIComponent(p)}/templates/${encodeURIComponent(t)}/lock`;

function release(projectId: string, templateId: string) {
  // keepalive: also sent while the tab closes.
  void fetch(lockUrl(projectId, templateId), { method: 'DELETE', credentials: 'same-origin', keepalive: true, headers: { 'X-Strata-Client': clientId } }).catch(() => {});
}

/** Takes the open template's edit lock, keeps it, and follows take overs. */
export function useEditLock(project: Project | null, templateId: string | undefined) {
  const [state, setState] = useState<LockState>({ status: 'pending' });
  const toast = useApp((s) => s.toast);
  const pid = project?.id;
  const live = useRef(state);
  live.current = state;

  const acquire = async (force = false, opening = false): Promise<void> => {
    if (!pid || !templateId) return;
    try {
      const r = await api<{ granted: boolean; lock: EditLock }>('POST', lockUrl(pid, templateId), { force });
      // Opening a template still held by oneself (the Designer left a moment ago, a reloaded tab): taken back silently.
      if (!r.granted && opening && r.lock.userId === useApp.getState().auth?.user?.id) return acquire(true);
      setState(r.granted ? { status: 'mine' } : { status: 'held', holder: r.lock });
    } catch {
      // Offline or no rights: let the save errors speak.
      setState({ status: 'mine' });
    }
  };

  useEffect(() => {
    if (!pid || !templateId) return;
    setState({ status: 'pending' });
    void acquire(false, true);
    const renew = setInterval(() => void acquire(), RENEW_MS);
    const onLock = (e: Event) => {
      const ev = (e as CustomEvent).detail as { projectId: string; templateId: string; lock: EditLock | null; takenFrom: string | null };
      if (ev.projectId !== pid || ev.templateId !== templateId) return;
      if (ev.lock && ev.lock.client !== clientId) {
        if (ev.takenFrom === clientId) toast(t('{user} took over this template — it is read only for you now', { user: ev.lock.userName }), 'info');
        setState({ status: 'held', holder: ev.lock });
      } else if (!ev.lock && live.current.status === 'held') {
        // Released: take it.
        void acquire();
      }
    };
    lockEvents.addEventListener('lock', onLock);
    const onUnload = () => release(pid, templateId);
    window.addEventListener('pagehide', onUnload);
    return () => {
      clearInterval(renew);
      lockEvents.removeEventListener('lock', onLock);
      window.removeEventListener('pagehide', onUnload);
      release(pid, templateId);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pid, templateId]);

  // Read only while someone else edits; follow their saves.
  const readOnly = state.status === 'held';
  useEffect(() => {
    useDesigner.setState({ readOnly });
  }, [readOnly]);
  const draft = project && templateId ? draftOf(project, templateId) : undefined;
  useEffect(() => {
    if (readOnly && draft) useDesigner.getState().replace(draft);
  }, [readOnly, draft]);

  return { state, takeOver: () => acquire(true) };
}

/**
 * Veil over the designer while another designer holds the template: their
 * changes show live underneath; it cannot be dismissed, only taken over. The
 * top menus and the template list stay usable.
 */
export function LockOverlay({ holder, template, onTakeOver }: { holder: EditLock; template: string; onTakeOver: () => void }) {
  const [confirm, setConfirm] = useState(false);
  // Held by oneself (another tab): taken over without a second question.
  const own = useApp((s) => s.auth?.user?.id === holder.userId);
  return (
    <div className="lock-overlay">
      <div className="lock-card" role="alertdialog" aria-labelledby="lock-title">
        <span className="lock-icon">
          <Lock size={20} />
        </span>
        <h2 id="lock-title">
          {template ? t('{user} is editing “{template}”', { user: holder.userName, template }) : t('{user} is editing this template', { user: holder.userName })}
        </h2>
        {confirm ? (
          <>
            <p>{t('{user} switches to read only and is told that you took over. A change they made in the last second may be lost.', { user: holder.userName })}</p>
            <div className="row end">
              <button className="btn" onClick={() => setConfirm(false)}>
                {t('Cancel')}
              </button>
              <button className="btn warn" onClick={onTakeOver}>
                {t('Take over')}
              </button>
            </div>
          </>
        ) : (
          <>
            <p>{t('You see their changes live. Take over to edit it yourself, or open another template.')}</p>
            <div className="row end">
              <button className="btn warn" onClick={() => (own ? onTakeOver() : setConfirm(true))}>
                {own ? t('Take over') : t('Take over…')}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/** Templates of `ids` on air now in a channel of the project: "name · channel". */
async function onAir(project: Project, ids: string[]): Promise<string[]> {
  const channels = useApp.getState().channels.filter((c) => c.projectId === project.id);
  const name = (id: string) => project.templates.find((t) => t.id === id)?.name ?? id;
  const out: string[] = [];
  await Promise.all(
    channels.map(async (c) => {
      try {
        const st = await api<ChannelState>('GET', `/api/channels/${encodeURIComponent(c.id)}/state`);
        for (const p of Object.values(st.layers ?? {})) if (ids.includes(p.templateId)) out.push(`${name(p.templateId)} · ${c.name}`);
      } catch {
        /* channel not visible to this user */
      }
    }),
  );
  return out;
}

/** Asks before publishing what is on air (it changes on screen at once); the rest is published directly. */
async function confirmPublish(project: Project, ids: string[]): Promise<boolean> {
  const live = await onAir(project, ids);
  if (!live.length) return true;
  const names = ids.map((id) => project.drafts?.find((t) => t.id === id)?.name ?? project.templates.find((t) => t.id === id)?.name ?? id);
  return confirmDialog({
    title: ids.length > 1 ? t('Publish {n} templates?', { n: ids.length }) : t('Publish “{name}”?', { name: names[0] }),
    message: (
      <div className="publish-confirm">
        <p>{t('The Operator uses the new version immediately.')}</p>
        {ids.length > 1 && (
          <ul className="publish-list">
            {names.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        )}
        {live.length ? (
          <div className="publish-onair">
            <b>{live.length > 1 ? t('On air now: they change on screen at once') : t('On air now: it changes on screen at once')}</b>
            <ul>
              {live.map((n) => (
                <li key={n}>
                  <span className="live-dot" /> {n}
                </li>
              ))}
            </ul>
          </div>
        ) : (
          <p className="muted">{ids.length > 1 ? t('None of them is on air.') : t('It is not on air.')}</p>
        )}
      </div>
    ),
    confirm: t('Publish'),
    danger: live.length > 0,
  });
}

/** Publish state of one template, in its properties (it is published with the top bar's Publish). */
export function TemplatePublish({ project, id }: { project: Project; id: string }) {
  const { published, draft } = publishState(project, id);
  if (published && !draft) return null;
  return (
    <div className="publish publish-line">
      <span className="publish-draft">{published ? t('Changes not in the Operator') : t('New · not in the Operator')}</span>
    </div>
  );
}

/**
 * The Designer's part of the top bar: one state (saving, changes not in the
 * Operator, all published) and one Publish, of every template with changes.
 */
export function TopPublish({ project, saving, readOnly }: { project: Project; saving: boolean; readOnly: boolean }) {
  const saveTemplate = useApp((s) => s.saveTemplate);
  const toast = useApp((s) => s.toast);
  const [busy, setBusy] = useState(false);
  const changed = project.drafts ?? [];
  const isNew = (id: string) => !project.templates.some((t) => t.id === id);
  const publish = async () => {
    setBusy(true);
    try {
      if (!(await confirmPublish(project, changed.map((t) => t.id)))) return;
      const st = useDesigner.getState();
      if (st.base && !(await saveTemplate(st.base))) return;
      const r = await api<{ name: string; revision: number }[]>('POST', `/api/projects/${encodeURIComponent(project.id)}/publish`);
      if (r.length === 1) toast(t('Published · {name}', { name: r[0].name }), 'info');
      else toast(tn(r.length, 'Published · {n} template', 'Published · {n} templates'), 'info');
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  let state: React.ReactNode;
  if (readOnly) state = <span className="ux2-pub ro">{t('Read only')}</span>;
  else if (saving)
    state = (
      <span className="ux2-pub saving" title={t('Changes are saved automatically')}>
        {t('Saving…')}
      </span>
    );
  return (
    <>
      {state}
      {/* Changes waiting: Publish pulses lightly. */}
      <button className={`btn small primary ux2-publish ${changed.length && !readOnly && !busy ? 'pending' : ''}`} disabled={busy || readOnly || !changed.length} title={changed.length ? t('Not yet in the Operator: {list}', { list: changed.map((d) => (isNew(d.id) ? t('{name} (new)', { name: d.name }) : d.name)).join(', ') }) : t('Publish to the Operator')} onClick={publish}>
        <Upload size={13} /> {busy ? t('Publishing…') : t('Publish')}
      </button>
    </>
  );
}

/** Draft / published state of the open template, and the Publish button. */
export function PublishButton({ project, readOnly }: { project: Project; readOnly: boolean }) {
  const base = useDesigner((s) => s.base);
  const saveTemplate = useApp((s) => s.saveTemplate);
  const toast = useApp((s) => s.toast);
  const [busy, setBusy] = useState(false);
  if (!base) return null;
  const { published, draft } = publishState(project, base.id);
  const changed = draft || !published;
  const url = `/api/projects/${encodeURIComponent(project.id)}/templates/${encodeURIComponent(base.id)}`;
  const publish = async () => {
    setBusy(true);
    try {
      // The latest edits first (autosave may still be pending).
      if (!(await saveTemplate(useDesigner.getState().base!))) return;
      const r = await api<{ revision: number }>('POST', `${url}/publish`);
      toast(t('“{name}” v{rev} is live in the Operator', { name: base.name, rev: r.revision }), 'info');
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setBusy(false);
    }
  };
  const discard = async () => {
    if (!published || !(await confirmDialog({ title: t('Discard the unpublished changes?'), message: t('“{name}” goes back to v{rev}, the version the Operator uses.', { name: base.name, rev: published.revision ?? 1 }), confirm: t('Discard'), danger: true }))) return;
    await api('DELETE', `${url}/draft`).catch((e) => toast((e as Error).message, 'error'));
    useDesigner.getState().load(published);
  };
  if (!changed)
    return (
      <span className="publish-state" title={t('The Operator uses this version')}>
        <Check size={13} /> {t('Live in Operator')}
        {published?.revision ? ` · v${published.revision}` : ''}
      </span>
    );
  return (
    <div className="publish">
      {published && draft && !readOnly && (
        <button className="icon-btn ghost" title={t('Discard unpublished changes (back to v{rev})', { rev: published.revision || 1 })} onClick={discard}>
          <RotateCcw size={13} />
        </button>
      )}
      <button className="btn small primary" disabled={busy || readOnly} title={published ? t('The Operator uses v{rev} until you publish', { rev: published.revision || 1 }) : t('New template: the Operator does not see it until you publish')} onClick={publish}>
        <Upload size={13} /> {published ? t('Publish to Operator') : t('Publish (new)')}
      </button>
    </div>
  );
}

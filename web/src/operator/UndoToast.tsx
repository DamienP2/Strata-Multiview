import { useEffect, useRef } from 'react';
import { api, useApp } from '../store';
import { t } from '../i18n';

/*
 * Undo on air: ⌘Z / Ctrl+Z (or Undo in the transport bar) undoes the last
 * action of the channel.
 */

export interface UndoStep {
  /** Server time (ms). */
  at: number;
  actor: { kind: string; id: string; name: string };
  action: string;
  label: string;
  /** Graphics on air before it (Clear all). */
  count?: number;
}

const url = (channelId: string) => `/api/channels/${encodeURIComponent(channelId)}/undo`;
const actions = new EventTarget();
/** Undo asked (the Operator's shortcut). */
export const requestUndo = () => actions.dispatchEvent(new Event('undo'));

/** Undoes the last action of the channel; false when nothing was undone (already told). */
async function undoLast(channelId: string): Promise<boolean> {
  try {
    await api('POST', url(channelId));
    return true;
  } catch (e) {
    const msg = (e as Error).message;
    useApp.getState().toast(/nothing to undo/i.test(msg) ? t('Nothing to undo') : msg, 'error');
    return false;
  }
}

/** ⌘Z / Ctrl+Z undoes the last action on the channel. */
export function UndoShortcut({ channelId, operate }: { channelId: string | null; operate: boolean }) {
  const undo = async () => {
    if (!channelId || !operate) return;
    const list = await api<UndoStep[]>('GET', url(channelId)).catch(() => [] as UndoStep[]);
    if (!list.length) return useApp.getState().toast(t('Nothing to undo'), 'error');
    await undoLast(channelId);
  };
  const undoRef = useRef(undo);
  undoRef.current = undo;

  useEffect(() => {
    const onUndo = () => void undoRef.current();
    actions.addEventListener('undo', onUndo);
    return () => actions.removeEventListener('undo', onUndo);
  }, []);
  return null;
}

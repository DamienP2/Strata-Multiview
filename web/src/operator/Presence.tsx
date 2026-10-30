import { useEffect, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import { sendWs, useApp } from '../store';
import { t } from '../i18n';

/*
 * Who has the Operator of a channel open: the Operator says where it is on
 * the socket (again after a reconnection), and shows the others in the top bar.
 */

const COLORS = ['#8fa3ff', '#a78bfa', '#22c55e', '#f5a524', '#f472b6', '#38bdf8', '#fb7185', '#34d399'];

/** A person's colour, the same on every screen. */
export function personColor(id: string): string {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0;
  return COLORS[Math.abs(h) % COLORS.length];
}

/** "Thomas Bernard" → TB, "thomas" → T. */
export function initials(name: string): string {
  const words = name.trim().split(/[\s._-]+/).filter(Boolean);
  if (!words.length) return '?';
  return (words.length > 1 ? words[0][0] + words[1][0] : words[0][0]).toUpperCase();
}

export function Avatar({ id, name, small }: { id: string; name: string; small?: boolean }) {
  return (
    <span className={`opl-av ${small ? 'small' : ''}`} style={{ background: personColor(id) }} aria-hidden>
      {initials(name)}
    </span>
  );
}

/** Tells the server this Operator is open on `channelId` (none: closed), again after a reconnection. */
export function usePresence(channelId: string | null) {
  const connected = useApp((s) => s.connected);
  useEffect(() => {
    if (!connected) return;
    sendWs({ type: 'presence', channelId });
  }, [channelId, connected]);
  useEffect(() => () => sendWs({ type: 'presence', channelId: null }), []);
}

/** The people on the channel, me first. */
function People({ channelId }: { channelId: string }) {
  const presence = useApp((s) => s.presence);
  const me = useApp((s) => s.auth?.user);
  const people = useMemo(() => {
    const here = presence.filter((p) => p.channelId === channelId);
    const mine = here.find((p) => p.userId === me?.id) ?? (me ? { userId: me.id, userName: me.displayName.trim() || me.username, channelId } : null);
    return [...(mine ? [mine] : []), ...here.filter((p) => p.userId !== me?.id)];
  }, [presence, channelId, me]);
  if (!people.length) return null;
  return (
    <div className="opl-people">
      {people.map((p) => (
        <span key={p.userId} className={`opl-person ${p.userId === me?.id ? 'me' : ''}`} title={p.userId === me?.id ? t('{name} (you)', { name: p.userName }) : p.userName}>
          <Avatar id={p.userId} name={p.userName} />
        </span>
      ))}
    </div>
  );
}

/** In the app's top bar: the people on the channel (the history of the actions is in the transport bar). */
export function OperatorTop({ channelId }: { channelId: string | null; operate?: boolean }) {
  usePresence(channelId);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  useEffect(() => setSlot(document.getElementById('topbar-slot')), []);
  if (!slot || !channelId) return null;
  return createPortal(<People channelId={channelId} />, slot);
}

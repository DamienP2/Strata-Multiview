import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronDown, Hand } from 'lucide-react';
import { create } from 'zustand';
import { api, serverEvents, useApp, type ZoneLock } from '../store';
import type { Project } from '../engine/types';
import { t, tc } from '../i18n';
import { Avatar } from './Presence';

/*
 * Zones of a channel held by operators: taking a graphic on a free zone gives
 * it to you; the others see it held (on its rundown rows and its on-air tile)
 * until they take it over (one click, the holder is told and can take it back).
 */

const NONE: ZoneLock[] = [];
const url = (channelId: string) => `/api/channels/${encodeURIComponent(channelId)}`;

/** Takes a zone (from whoever holds it), or frees it. */
export function setZoneLock(channelId: string, zone: string, take: boolean) {
  return api(take ? 'PUT' : 'DELETE', `${url(channelId)}/zones/${encodeURIComponent(zone)}/lock`).catch((e) => useApp.getState().toast((e as Error).message, 'error'));
}

/** A zone of mine someone just took: told for a few seconds, with « Take it back ». */
interface Taken {
  channelId: string;
  zone: string;
  zoneName: string;
  userId: string;
  userName: string;
}
const useTaken = create<{ taken: Taken | null }>(() => ({ taken: null }));
let takenTimer: ReturnType<typeof setTimeout> | undefined;

/** The zones held on the channel; tells me when someone takes one of mine. */
export function useZoneLocks(channelId: string | null, project: Project | null) {
  const locks = useApp((s) => (channelId ? (s.zoneLocks[channelId] ?? NONE) : NONE));
  const me = useApp((s) => s.auth?.user?.id);
  const connected = useApp((s) => s.connected);
  // The locks as last seen here: which of mine were taken.
  const seen = useRef<ZoneLock[]>(locks);
  const zones = useRef(project?.zones ?? []);
  zones.current = project?.zones ?? [];

  useEffect(() => {
    if (!channelId || !connected) return;
    api<ZoneLock[]>('GET', `${url(channelId)}/locks`)
      .then((l) => {
        seen.current = l;
        useApp.setState({ zoneLocks: { ...useApp.getState().zoneLocks, [channelId]: l } });
      })
      .catch(() => {});
  }, [channelId, connected]);

  useEffect(() => {
    if (!channelId || !me) return;
    const on = (e: Event) => {
      const ev = (e as CustomEvent<{ channelId: string; locks: ZoneLock[]; takenFrom: string | null }>).detail;
      if (ev.channelId !== channelId) return;
      const before = seen.current;
      seen.current = ev.locks;
      if (ev.takenFrom !== me) return;
      for (const l of ev.locks) {
        if (l.userId === me || !before.some((b) => b.zone === l.zone && b.userId === me)) continue;
        const zone = zones.current.find((z) => z.id === l.zone)?.name ?? l.zone;
        useTaken.setState({ taken: { channelId, zone: l.zone, zoneName: zone, userId: l.userId, userName: l.userName } });
        clearTimeout(takenTimer);
        takenTimer = setTimeout(() => useTaken.setState({ taken: null }), 8000);
      }
    };
    serverEvents.addEventListener('zoneLocks', on);
    return () => serverEvents.removeEventListener('zoneLocks', on);
  }, [channelId, me]);

  return useMemo(() => {
    const byZone = new Map(locks.map((l) => [l.zone, l]));
    return {
      locks,
      /** The lock of a zone someone else holds. */
      other: (zone?: string | null) => {
        const l = zone ? byZone.get(zone) : undefined;
        return l && l.userId !== me ? l : undefined;
      },
    };
  }, [locks, me]);
}

/** In the top bar: the zones and who holds them (a square per zone, the holders' faces); a click lists them, to take or free. */
export function ZonesButton({ project, channelId, locks, operate }: { project: Project; channelId: string; locks: ZoneLock[]; operate: boolean }) {
  const me = useApp((s) => s.auth?.user?.id);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const [open, setOpen] = useState<{ right: number; top: number } | null>(null);
  const pop = useRef<HTMLDivElement>(null);
  useEffect(() => setSlot(document.getElementById('topbar-slot')), []);
  useEffect(() => {
    if (!open) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.preventDefault(), setOpen(null));
    const away = (e: PointerEvent) => !pop.current?.contains(e.target as Node) && !(e.target as Element).closest?.('.opl-zbtn') && setOpen(null);
    window.addEventListener('keydown', esc);
    window.addEventListener('pointerdown', away, true);
    return () => (window.removeEventListener('keydown', esc), window.removeEventListener('pointerdown', away, true));
  }, [open]);
  const zones = [...project.zones].sort((a, b) => b.layer - a.layer);
  if (!slot || !zones.length) return null;
  const lockOf = (zone: string) => locks.find((l) => l.zone === zone);
  const holders = [...new Map(locks.map((l) => [l.userId, l])).values()];
  const mine = locks.filter((l) => l.userId === me);
  return createPortal(
    <>
      <button
        className={`opl-zbtn ${open ? 'open' : ''}`}
        title={t('Zones')}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setOpen(open ? null : { right: Math.max(8, window.innerWidth - r.right), top: r.bottom + 6 });
        }}
      >
        <span className="opl-zdots">
          {zones.map((z) => (
            <i key={z.id} style={lockOf(z.id) ? { background: z.color } : undefined} />
          ))}
        </span>
        {t('Zones')}
        {holders.length > 0 && (
          <span className="opl-zstack">
            {holders.map((l) => (
              <Avatar key={l.userId} id={l.userId} name={l.userName} small />
            ))}
          </span>
        )}
        <ChevronDown size={14} />
      </button>
      {open &&
        createPortal(
          <div ref={pop} className="menu project-menu opl-zpop" style={{ position: 'fixed', right: open.right, top: open.top, left: 'auto' }}>
            {zones.map((z) => {
              const l = lockOf(z.id);
              return (
                <div key={z.id} className="opl-zrow">
                  <i className="opl-zc" style={{ background: z.color }} />
                  <span className="opl-zn">{z.name}</span>
                  {l ? (
                    <span className="opl-zwho">
                      <Avatar id={l.userId} name={l.userName} small />
                      {l.userId === me ? t('you') : l.userName}
                    </span>
                  ) : (
                    <span className="opl-free">{t('free')}</span>
                  )}
                  {l?.userId === me ? (
                    <button className="opl-mini" disabled={!operate} onClick={() => setZoneLock(channelId, z.id, false)}>
                      {tc('zone', 'Free')}
                    </button>
                  ) : l ? (
                    <button className="opl-mini take" disabled={!operate} onClick={() => setZoneLock(channelId, z.id, true)}>
                      {tc('zone', 'Take')}
                    </button>
                  ) : (
                    <span />
                  )}
                </div>
              );
            })}
            {mine.length > 0 && (
              <>
                <div className="menu-sep" />
                <button className="opl-zall" disabled={!operate} onClick={() => mine.forEach((l) => setZoneLock(channelId, l.zone, false))}>
                  {t('Free my zones')}
                </button>
              </>
            )}
          </div>,
          document.body,
        )}
    </>,
    slot,
  );
}

/** « Take over » a zone someone else holds (a rundown row, an on-air tile). */
export function TakeOver({ channelId, lock, operate }: { channelId: string; lock: ZoneLock; operate: boolean }) {
  return (
    <button className="opl-mini take opl-takeover" disabled={!operate} title={t('{name} holds this zone', { name: lock.userName })} onClick={(e) => (e.stopPropagation(), void setZoneLock(channelId, lock.zone, true))}>
      <Hand size={12} /> {t('Take over')}
    </button>
  );
}

/** Told when someone takes one of my zones: who, which, and « Take it back » for a few seconds. */
export function TakenNotice() {
  const taken = useTaken((s) => s.taken);
  if (!taken) return null;
  return (
    <div className="opl-taken" role="status">
      <Avatar id={taken.userId} name={taken.userName} />
      <span>{t('{name} took over {zone}', { name: taken.userName, zone: taken.zoneName })}</span>
      <button
        className="opl-mini"
        onClick={() => {
          void setZoneLock(taken.channelId, taken.zone, true);
          useTaken.setState({ taken: null });
        }}
      >
        {t('Take it back')}
      </button>
    </div>
  );
}

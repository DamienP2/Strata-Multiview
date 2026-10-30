import { useEffect } from 'react';
import { create } from 'zustand';
import { api } from '../store';

/*
 * The server's health (cg-server health.rs), for admins: the System menu's
 * dot, the banner after a crash and the Health page. Asked every minute.
 */

export interface Crash {
  at: number;
  kind: 'panic' | 'stop';
  version: string;
  thread: string;
  message: string;
  location: string;
  backtrace: string;
  uptimeSecs: number;
}

export interface Incident {
  kind: string;
  level: 'CRASH' | 'ERROR' | 'WARN';
  cat: string;
  title: string;
  detail: string;
  first: number;
  last: number;
  count: number;
  crash?: Crash;
}

export interface Health {
  version: string;
  startedAt: number | null;
  uptimeSecs: number;
  crashes: number;
  lastCrash: number | null;
  unseenCrash: boolean;
  errors: number;
  warnings: number;
  lastHourErrors: number;
  disk: { total: number; free: number };
  incidents: Incident[];
}

export interface JournalLine {
  at: number;
  level: 'CRASH' | 'ERROR' | 'WARN' | 'INFO';
  cat: string;
  target: string;
  msg: string;
  fields?: string;
}

interface Store {
  health: Health | null;
  load: () => Promise<void>;
  seen: () => Promise<void>;
}

export const useHealthStore = create<Store>((set, get) => ({
  health: null,
  load: async () => {
    try {
      set({ health: await api<Health>('GET', '/api/system/health') });
    } catch {
      /* not an admin, or the server is away */
    }
  },
  seen: async () => {
    await api('POST', '/api/system/health/seen').catch(() => {});
    const h = get().health;
    if (h) set({ health: { ...h, unseenCrash: false } });
  },
}));

/** The health, asked now and every minute while `on`. */
export function useHealth(on: boolean): Health | null {
  const load = useHealthStore((s) => s.load);
  useEffect(() => {
    if (!on) return;
    void load();
    const h = setInterval(() => void load(), 60_000);
    return () => clearInterval(h);
  }, [on, load]);
  return useHealthStore((s) => s.health);
}

/** Something to look at: a crash not seen yet, or errors in the last hour. */
export const needsLook = (h: Health | null) => !!h && (h.unseenCrash || h.lastHourErrors > 0);

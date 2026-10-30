import { useState } from 'react';
import { create } from 'zustand';
import { api, clientId } from './store';

/*
 * What a user chose for the display (panel sizes, what the Designer shows…),
 * kept on the server for them (GET / PATCH /api/me/display): the same on
 * every browser they log in from. Loaded before the pages are drawn; a
 * change is sent a moment later, with the ones made meanwhile.
 */

type Values = Record<string, unknown>;

export const useDisplayPrefs = create<{ user: string | null; values: Values }>(() => ({ user: null, values: {} }));

let pending: Values = {};
let timer: ReturnType<typeof setTimeout> | undefined;

/** The choices of `user` (a new user: theirs, not the one before's). */
export async function loadDisplayPrefs(user: string): Promise<void> {
  if (useDisplayPrefs.getState().user === user) return;
  flush();
  const values = await api<Values>('GET', '/api/me/display').catch(() => ({}));
  useDisplayPrefs.setState({ user, values: values ?? {} });
}

export function displayPref<T>(key: string, fallback: T): T {
  const v = useDisplayPrefs.getState().values[key];
  return v === undefined ? fallback : (v as T);
}

export function setDisplayPref(key: string, value: unknown): void {
  const { values } = useDisplayPrefs.getState();
  if (JSON.stringify(values[key]) === JSON.stringify(value)) return;
  useDisplayPrefs.setState({ values: { ...values, [key]: value } });
  pending[key] = value ?? null;
  clearTimeout(timer);
  timer = setTimeout(flush, 600);
}

function flush(keepalive = false) {
  clearTimeout(timer);
  if (!Object.keys(pending).length) return;
  const body = JSON.stringify(pending);
  pending = {};
  if (keepalive) {
    // The page closes: a plain request that outlives it.
    void fetch('/api/me/display', { method: 'PATCH', credentials: 'same-origin', keepalive: true, headers: { 'Content-Type': 'application/json', 'X-Strata-Client': clientId }, body }).catch(() => {});
  } else void api('PATCH', '/api/me/display', JSON.parse(body)).catch(() => {});
}

if (typeof window !== 'undefined') window.addEventListener('pagehide', () => flush(true));

/** A display choice, as `useState`, kept for the user. */
export function useDisplayPref<T>(key: string, fallback: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => displayPref(key, fallback));
  return [
    v,
    (x: T) => {
      setV(x);
      setDisplayPref(key, x);
    },
  ];
}

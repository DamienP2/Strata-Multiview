// The traces of the page's scopes (widgets/scope.tsx), all on one WebSocket to the server
// (inputs.rs `scopes_ws`): a browser opens six connections at most to a server, a multiview may
// show many scopes. The page sends the list of its scopes when it changes; each trace comes as a
// JPEG when a new one is made, shown as an object URL (the one before is freed once replaced).
import { useEffect, useState } from 'react';

/** A scope's trace, as asked of the server (mirrors inputs.rs ScopeWanted). */
export interface ScopeWanted {
  input: string;
  kind: string;
  w: number;
  h: number;
  /** Intensity in hundredths. */
  i: number;
  /** rrggbb. */
  color: string;
}

const keyOf = (s: ScopeWanted) => `${s.input}|${s.kind}|${s.w}|${s.h}|${s.i}|${s.color}`;

/** By key: what is asked, who shows it, its latest trace (null: the input has no picture). */
const wanted = new Map<string, { s: ScopeWanted; subs: Set<(url: string | null) => void>; url: string | null }>();
let socket: WebSocket | null = null;
let opening: ReturnType<typeof setTimeout> | null = null;
let pending = false;

function send() {
  pending = false;
  if (socket?.readyState !== WebSocket.OPEN) return;
  socket.send(JSON.stringify([...wanted.entries()].map(([key, w]) => ({ key, ...w.s }))));
}

/** Sends the list once, after the changes of this moment. */
function changed() {
  if (!wanted.size) {
    // Nothing shown: the connection goes.
    socket?.close();
    socket = null;
    return;
  }
  if (!socket) return connect();
  if (!pending) {
    pending = true;
    queueMicrotask(send);
  }
}

function connect() {
  if (socket || opening || !wanted.size) return;
  const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/scopes`);
  ws.binaryType = 'arraybuffer';
  socket = ws;
  ws.onopen = send;
  ws.onmessage = (e) => {
    if (!(e.data instanceof ArrayBuffer)) return;
    const b = new Uint8Array(e.data);
    const n = b[0];
    const key = new TextDecoder().decode(b.subarray(1, 1 + n));
    const w = wanted.get(key);
    if (!w) return;
    const old = w.url;
    w.url = b.length > 1 + n ? URL.createObjectURL(new Blob([b.subarray(1 + n)], { type: 'image/jpeg' })) : null;
    w.subs.forEach((f) => f(w.url));
    // Freed once the new one is drawn.
    if (old) setTimeout(() => URL.revokeObjectURL(old), 1000);
  };
  ws.onclose = () => {
    if (socket !== ws) return;
    socket = null;
    // Again in a moment while scopes are shown (the server restarted…).
    opening = setTimeout(() => {
      opening = null;
      connect();
    }, 1000);
  };
}

/** The latest trace of a scope (an object URL, null while there is none). */
export function useScope(s: ScopeWanted | null): string | null {
  const key = s ? keyOf(s) : '';
  const [url, setUrl] = useState<string | null>(() => (key ? (wanted.get(key)?.url ?? null) : null));
  useEffect(() => {
    if (!s) return setUrl(null);
    let w = wanted.get(key);
    if (!w) {
      w = { s, subs: new Set(), url: null };
      wanted.set(key, w);
      changed();
    }
    setUrl(w.url);
    w.subs.add(setUrl);
    return () => {
      const x = wanted.get(key);
      if (!x) return;
      x.subs.delete(setUrl);
      if (!x.subs.size) {
        wanted.delete(key);
        if (x.url) URL.revokeObjectURL(x.url);
        changed();
      }
    };
    // The key says everything of `s`.
  }, [key]);
  return url;
}

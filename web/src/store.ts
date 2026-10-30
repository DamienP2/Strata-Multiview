import { useEffect, useRef } from 'react';
import { lang, pickedByHand, setLang, t } from './i18n';
import { setTheme } from './theme';
import { create } from 'zustand';
import { useData } from './data/dataStore';
import { designTemplates } from './engine/drafts';
import type { ChannelInfo, ChannelState, Command, DataStatus, Project, ProjectSummary, RundownItem, Template, User, Zone } from './engine/types';

type ServerEvent =
  | { type: 'hello'; user: User | null; serverName: string; serverTime: number; timezone: string; channels: ChannelInfo[]; projects: ProjectSummary[] }
  | { type: 'project'; project: Project }
  /** A project saved with only this template (or draft) changed or added. */
  | { type: 'projectTemplate'; projectId: string; draft: boolean; template: Template }
  | { type: 'projectDeleted'; id: string }
  | { type: 'channels'; channels: ChannelInfo[] }
  | { type: 'channel'; channelId: string; state: ChannelState; serverTime: number }
  | { type: 'data'; source: string; status: DataStatus }
  | { type: 'lock'; projectId: string; templateId: string; lock: EditLock | null; takenFrom: string | null }
  | { type: 'folders'; folders: Folders }
  | { type: 'changed'; what: string }
  | { type: 'presence'; present: Present[] }
  | { type: 'zoneLocks'; channelId: string; locks: ZoneLock[]; takenFrom: string | null }
  | { type: 'error'; message: string };

/** Someone with the Operator open on a channel. */
export interface Present {
  userId: string;
  userName: string;
  channelId: string;
}

/** A zone of a channel held by one operator. */
export interface ZoneLock {
  zone: string;
  userId: string;
  userName: string;
  since: number;
}

/** Folders of the project list (one level) and the folder of each project. */
export interface Folders {
  folders: string[];
  assign: Record<string, string>;
}

interface Toast {
  id: number;
  message: string;
  kind: 'error' | 'info';
}

export interface AuthState {
  needsSetup: boolean;
  user: User | null;
  serverName: string;
  publicOutputs: boolean;
}

/** Channel state plus when it arrived, to extrapolate widget time. */
export interface Live {
  state: ChannelState;
  /** Client clock (ms) when received. */
  at: number;
}

interface AppStore {
  auth: AuthState | null;
  connected: boolean;
  /** Server clock − client clock, ms. */
  serverOffset: number;
  timezone: string;
  channels: ChannelInfo[];
  projects: ProjectSummary[];
  folders: Folders;
  /** Full projects received so far. */
  full: Record<string, Project>;
  live: Record<string, Live>;
  /** Project being designed, or loaded on the operated channel. */
  projectId: string | null;
  project: Project | null;
  channelId: string | null;
  toasts: Toast[];
  /** Who has the Operator open, on which channel. */
  presence: Present[];
  /** Zones held by operators, per channel. */
  zoneLocks: Record<string, ZoneLock[]>;

  init: () => Promise<void>;
  login: (username: string, password: string) => Promise<void>;
  setup: (username: string, password: string, displayName: string) => Promise<void>;
  logout: () => Promise<void>;
  connect: (outputChannel?: string) => void;
  openProject: (id: string | null) => Promise<void>;
  selectChannel: (id: string | null) => void;
  command: (cmd: Command, channelId?: string) => void;
  loadOnChannel: (channelId: string, projectId: string | null) => Promise<void>;
  /** False when not saved (already toasted). */
  /** Saved in `projectId` (default: the open project). */
  saveTemplate: (t: Template, projectId?: string) => Promise<boolean>;
  deleteTemplate: (id: string) => Promise<void>;
  saveRundown: (r: RundownItem[]) => Promise<void>;
  saveZones: (z: Zone[]) => Promise<boolean>;
  saveFolders: (f: Folders) => Promise<void>;
  toast: (message: string, kind?: Toast['kind']) => void;
}

let socket: WebSocket | null = null;
/** The WebSocket was connected before (a hello after it is a reconnection). */
let reconnected = false;

/** `p` with a template (or draft) replaced, or added when new: as the server
 * changed it (crates/cg-server/src/state.rs, apply_template). */
function withTemplate(p: Project | undefined, draft: boolean, t: Template): Project | undefined {
  if (!p) return undefined;
  const list = (draft ? p.drafts : p.templates) ?? [];
  const at = list.findIndex((x) => x.id === t.id);
  const next = at < 0 ? [...list, t] : list.map((x, i) => (i === at ? t : x));
  return draft ? { ...p, drafts: next } : { ...p, templates: next };
}

/** The client's clock, on the same steady scale as the animation frames. */
export const clientNow = () => performance.timeOrigin + performance.now();

/*
 * Server clock − client clock. Each message gives it minus its trip over the
 * network, which varies by a few ms from one to the next: the least delayed
 * of the last ones is kept, and it changes only by a real step, so what moves
 * by the clock (a ticker) never jumps back and forth.
 */
const offsetSamples: number[] = [];
let steadyOffset: number | null = null;
/** For the HTML5 output's diagnostics (`?debug`): messages received and how late each one came. */
export const clockStats = { arrivals: [] as { at: number; delay: number }[], offset: 0, steps: 0 };
function clockOffset(serverTime: number, now: number): number {
  const before = steadyOffset;
  clockStats.arrivals.push({ at: now, delay: now - serverTime });
  if (clockStats.arrivals.length > 400) clockStats.arrivals.splice(0, 100);
  offsetSamples.push(serverTime - now);
  if (offsetSamples.length > 64) offsetSamples.shift();
  const best = Math.max(...offsetSamples);
  if (steadyOffset === null || Math.abs(best - steadyOffset) > 30) steadyOffset = best;
  if (before !== null && before !== steadyOffset) clockStats.steps++;
  clockStats.offset = steadyOffset;
  return steadyOffset;
}
let toastId = 0;

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

/** API key used instead of the session cookie (pages embedded in other
 *  applications: newsroom MOS plugin). */
let apiKey: string | null = null;
export const setApiKey = (k: string | null) => void (apiKey = k);

/** A designer's edit lock on a template. */
export interface EditLock {
  userId: string;
  userName: string;
  client: string;
  since: number;
}

/** Lock changes pushed by the server (`detail`: the event). */
export const lockEvents = new EventTarget();

/** Other server events, by their `type` (`detail`: the event). */
export const serverEvents = new EventTarget();

/** Sends a message on the socket (dropped while disconnected). */
export function sendWs(msg: object) {
  if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(msg));
}

/** Lists changed by another client (`media`, `media:<project>`, `sources`, `config`). */
export const changeEvents = new EventTarget();

/** Calls `fn` whenever another client changes `what`. */
export function useChanged(what: string, fn: () => void) {
  const ref = useRef(fn);
  ref.current = fn;
  useEffect(() => {
    const h = () => ref.current();
    changeEvents.addEventListener(what, h);
    return () => changeEvents.removeEventListener(what, h);
  }, [what]);
}

/** This browser tab, for designer edit locks. */
export const clientId = typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : String(Math.random()).slice(2);

export async function api<T = unknown>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = { 'X-Strata-Client': clientId };
  // A file goes as it is; anything else as JSON.
  const raw = typeof Blob !== 'undefined' && body instanceof Blob;
  if (body !== undefined) headers['Content-Type'] = raw ? (body as Blob).type || 'application/octet-stream' : 'application/json';
  if (apiKey) headers.Authorization = `Bearer ${apiKey}`;
  const res = await fetch(path, {
    method,
    credentials: 'same-origin',
    headers,
    body: body === undefined ? undefined : raw ? (body as Blob) : JSON.stringify(body),
  });
  if (res.status === 401 && !path.startsWith('/api/auth/')) {
    // Session expired: back to the login page.
    const a = useApp.getState().auth;
    if (a?.user) useApp.setState({ auth: { ...a, user: null } });
  }
  if (!res.ok) throw new ApiError(res.status, (await res.text()) || res.statusText);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const useApp = create<AppStore>((set, get) => {
  /** Keeps `project` in sync with the cache and the selection. */
  const select = (projectId: string | null, full = get().full) => ({ projectId, project: (projectId && full[projectId]) || null });

  return {
    auth: null,
    connected: false,
    serverOffset: 0,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    channels: [],
    projects: [],
    folders: { folders: [], assign: {} },
    full: {},
    live: {},
    projectId: null,
    project: null,
    channelId: null,
    toasts: [],
    presence: [],
    zoneLocks: {},

    async init() {
      try {
        const auth = await api<AuthState>('GET', '/api/auth/state');
        // The user's language before anything is drawn.
        setLang(auth.user?.lang);
        // …and their colour theme (the browser's copy only paints the first frame).
        if (auth.user) setTheme(auth.user.theme || 'dark');
        set({ auth });
      } catch (e) {
        get().toast(t('Cannot reach the server: {error}', { error: errText(e) }), 'error');
        setTimeout(() => get().init(), 2000);
      }
    },

    async login(username, password) {
      const r = await api<{ user: User }>('POST', '/api/auth/login', { username, password });
      // A language picked on the login page becomes the user's.
      if (pickedByHand() && r.user.lang !== lang()) {
        r.user.lang = lang();
        void api('PUT', '/api/me/lang', { lang: r.user.lang }).catch(() => {});
      } else setLang(r.user.lang);
      setTheme(r.user.theme || 'dark');
      set({ auth: { ...get().auth!, user: r.user } });
      socket?.close();
    },

    async setup(username, password, displayName) {
      const r = await api<{ user: User }>('POST', '/api/auth/setup', { username, password, displayName });
      set({ auth: { ...get().auth!, needsSetup: false, user: r.user } });
    },

    async logout() {
      await api('POST', '/api/auth/logout').catch(() => {});
      socket?.close();
      set({ auth: { ...get().auth!, user: null }, full: {}, projects: [], channels: [], live: {}, ...select(null, {}) });
    },

    connect(outputChannel) {
      if (socket) return;
      const q = outputChannel ? `?channel=${encodeURIComponent(outputChannel)}` : '';
      const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws${q}`;
      let delay = 500;
      const open = () => {
        const ws = new WebSocket(url);
        socket = ws;
        ws.onopen = () => {
          delay = 500;
          set({ connected: true });
        };
        ws.onclose = () => {
          set({ connected: false });
          if (socket === ws) socket = null;
          // Logged out: do not hammer the server.
          if (!outputChannel && !get().auth?.user) return;
          setTimeout(() => !socket && open(), delay);
          delay = Math.min(delay * 2, 5000);
        };
        ws.onmessage = (ev) => {
          const msg = JSON.parse(ev.data) as ServerEvent;
          const now = clientNow();
          switch (msg.type) {
            case 'hello': {
              set({ channels: msg.channels, timezone: msg.timezone, serverOffset: clockOffset(msg.serverTime, now) });
              if (!outputChannel) {
                set({ projects: msg.projects });
                api<Folders>('GET', '/api/folders').then((folders) => set({ folders }), () => {});
              }
              // Refresh projects edited while we were away (not on the first
              // connection: the page has just opened the project itself).
              const { projectId } = get();
              if (projectId && !outputChannel && reconnected) get().openProject(projectId);
              reconnected = true;
              break;
            }
            case 'projectTemplate':
            case 'project': {
              const project = msg.type === 'project' ? msg.project : withTemplate(get().full[msg.projectId], msg.draft, msg.template);
              // A template of a project not open here: nothing to update.
              if (!project) break;
              const full = { ...get().full, [project.id]: project };
              const { id, name, description, width, height, frameRate, rundown } = project;
              const design = designTemplates(project);
              const summary: ProjectSummary = { id, name, description: description ?? '', width, height, frameRate, templates: design.length, rundown: rundown.length, cover: design[0]?.id, modifiedAt: Date.now() };
              const projects = get().projects.some((p) => p.id === id) ? get().projects.map((p) => (p.id === id ? summary : p)) : [...get().projects, summary];
              set({ full, projects: outputChannel ? get().projects : projects, ...select(get().projectId, full) });
              break;
            }
            case 'projectDeleted': {
              const full = { ...get().full };
              delete full[msg.id];
              set({ full, projects: get().projects.filter((p) => p.id !== msg.id), ...select(get().projectId === msg.id ? null : get().projectId, full) });
              break;
            }
            case 'folders':
              set({ folders: msg.folders });
              break;
            case 'changed':
              changeEvents.dispatchEvent(new CustomEvent(msg.what));
              if (msg.what === 'sources') void useData.getState().load();
              break;
            case 'channels':
              set({ channels: msg.channels });
              break;
            case 'channel':
              set({ live: { ...get().live, [msg.channelId]: { state: msg.state, at: now } }, serverOffset: clockOffset(msg.serverTime, now) });
              break;
            case 'lock':
              lockEvents.dispatchEvent(new CustomEvent('lock', { detail: msg }));
              break;
            case 'data':
              useData.getState().onStatus(msg.source, msg.status);
              break;
            case 'presence':
              set({ presence: msg.present });
              break;
            case 'zoneLocks':
              set({ zoneLocks: { ...get().zoneLocks, [msg.channelId]: msg.locks } });
              serverEvents.dispatchEvent(new CustomEvent('zoneLocks', { detail: msg }));
              break;
            case 'error':
              get().toast(msg.message, 'error');
              break;
            default:
              // Events other parts of the app listen to (`asRun`, `schedule`…).
              serverEvents.dispatchEvent(new CustomEvent((msg as { type: string }).type, { detail: msg }));
          }
        };
      };
      open();
    },

    async openProject(id) {
      set(select(id));
      if (!id) return;
      try {
        const p = await api<Project>('GET', `/api/projects/${encodeURIComponent(id)}`);
        const full = { ...get().full, [id]: p };
        set({ full, ...select(get().projectId, full) });
      } catch (e) {
        get().toast(t('Cannot open the project: {error}', { error: errText(e) }), 'error');
      }
    },

    selectChannel(id) {
      set({ channelId: id });
    },

    command(cmd, channelId = get().channelId ?? undefined) {
      if (!channelId) return;
      if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'command', channelId, command: cmd }));
      else api('POST', `/api/channels/${encodeURIComponent(channelId)}/command`, cmd).catch((e) => get().toast(errText(e), 'error'));
    },

    async loadOnChannel(channelId, projectId) {
      try {
        await api('PUT', `/api/channels/${encodeURIComponent(channelId)}/project`, { projectId });
      } catch (e) {
        get().toast(errText(e), 'error');
      }
    },

    async saveTemplate(tpl, projectId) {
      const pid = projectId ?? get().projectId;
      if (!pid) return false;
      try {
        await api('PUT', `/api/projects/${encodeURIComponent(pid)}/templates/${encodeURIComponent(tpl.id)}`, tpl);
        return true;
      } catch (e) {
        get().toast(t('Save failed: {error}', { error: errText(e) }), 'error');
        return false;
      }
    },

    async deleteTemplate(id) {
      const pid = get().projectId;
      if (!pid) return;
      await api('DELETE', `/api/projects/${encodeURIComponent(pid)}/templates/${encodeURIComponent(id)}`);
    },

    async saveRundown(r) {
      const p = get().project;
      if (!p) return;
      const before = p.rundown;
      const full = { ...get().full, [p.id]: { ...p, rundown: r } };
      set({ full, ...select(p.id, full) });
      try {
        // Only this change: another operator's edits made meanwhile are kept.
        await api('PUT', `/api/projects/${encodeURIComponent(p.id)}/rundown`, { base: before, rundown: r });
      } catch (e) {
        // Not saved: back to what the server has, so Take does not use it.
        const cur = get().full[p.id];
        if (cur && cur.rundown === r) {
          const back = { ...get().full, [p.id]: { ...cur, rundown: before } };
          set({ full: back, ...select(p.id, back) });
        }
        get().toast(t('Save failed: {error}', { error: errText(e) }), 'error');
      }
    },

    async saveZones(zones) {
      const p = get().project;
      if (!p) return false;
      try {
        await api('PUT', `/api/projects/${encodeURIComponent(p.id)}/zones`, zones);
        return true;
      } catch (e) {
        get().toast(errText(e), 'error');
        return false;
      }
    },

    async saveFolders(f) {
      set({ folders: f });
      try {
        set({ folders: await api<Folders>('PUT', '/api/folders', f) });
      } catch (e) {
        get().toast(errText(e), 'error');
      }
    },

    toast(message, kind = 'info') {
      // The browser's own words for a request that got no answer (« Failed to fetch »…).
      const offline = /Failed to fetch|NetworkError when attempting to fetch resource\.?|Load failed|Network request failed/g;
      message = message.replace(offline, (m) => (m === message.trim() ? t('The server does not answer') : t('the server does not answer')));
      const id = ++toastId;
      set({ toasts: [...get().toasts, { id, message, kind }] });
      setTimeout(() => set({ toasts: get().toasts.filter((t) => t.id !== id) }), 4000);
    },
  };
});

/** A file of a media library. */
export interface MediaFile {
  name: string;
  src: string;
  size: number;
  modified: number;
  kind: 'image' | 'video' | 'audio' | 'font' | 'other';
}

/** Media library URL: a project's, or the one shared by every project. */
export const mediaUrl = (projectId?: string | null) => (projectId ? `/api/projects/${encodeURIComponent(projectId)}/media` : '/api/media');

export const mediaName = (fileName: string) => fileName.replace(/[^A-Za-z0-9._-]/g, '_').replace(/^\.+/, '') || 'file';

/** Uploads a file to a media library (the project's when given) and returns its URL. */
export async function uploadAsset(file: File, projectId?: string | null, onProgress?: (fraction: number) => void): Promise<string> {
  const url = `${mediaUrl(projectId)}/${encodeURIComponent(mediaName(file.name))}`;
  // XMLHttpRequest: fetch has no upload progress.
  const xhr = new XMLHttpRequest();
  const done = new Promise<string>((resolve, reject) => {
    xhr.onload = () => (xhr.status < 300 ? resolve((JSON.parse(xhr.responseText) as MediaFile).src) : reject(new Error(xhr.responseText || xhr.statusText)));
    xhr.onerror = () => reject(new Error(t('Upload failed')));
  });
  xhr.upload.onprogress = (e) => e.lengthComputable && onProgress?.(e.loaded / e.total);
  xhr.open('PUT', url);
  xhr.withCredentials = true;
  if (apiKey) xhr.setRequestHeader('Authorization', `Bearer ${apiKey}`);
  xhr.send(file);
  return done;
}

/** Imports an After Effects composition (Bodymovin / LottieFiles export) as a new template. */
export async function importAfterEffects(projectId: string, file: File, layersOnly = false): Promise<{ template: Template; warnings: string[] }> {
  const name = file.name.replace(/\.(json|zip|lottie|mogrt)$/i, '');
  const res = await fetch(`/api/projects/${encodeURIComponent(projectId)}/import/after-effects?name=${encodeURIComponent(name)}${layersOnly ? '&layersOnly=true' : ''}`, {
    method: 'POST',
    body: file,
    credentials: 'same-origin',
    headers: apiKey ? { Authorization: `Bearer ${apiKey}` } : undefined,
  });
  if (!res.ok) throw new Error((await res.text()) || res.statusText);
  return res.json();
}

/** Rights, mirroring crates/cg-server/src/auth.rs. */
export const can = {
  design: (u: User | null | undefined) => u?.role === 'admin' || u?.role === 'designer',
  operate: (u: User | null | undefined) => u?.role === 'admin' || u?.role === 'designer' || u?.role === 'operator',
  admin: (u: User | null | undefined) => u?.role === 'admin',
};

/** A short hash of a text (FNV-1a), for versions in addresses. */
const hashText = (s: string) => {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193);
  return (h >>> 0).toString(36) + s.length.toString(36);
};

/** A template's thumbnail, `width` pixels wide at most (the size shown × 2 for
 * HiDPI screens). The version follows the template: browsers keep each one. */
export const thumbnailUrl = (projectId: string, t: Template, format = 'main', draft = false, width = 480) =>
  `/api/projects/${encodeURIComponent(projectId)}/templates/${encodeURIComponent(t.id)}/thumbnail.png?format=${encodeURIComponent(format)}${draft ? '&draft=true' : ''}&w=${width}&v=${hashText(JSON.stringify(t))}`;

import { useEffect } from 'react';
import { create } from 'zustand';
import { api, changeEvents, serverEvents, useApp } from '../store';
import type { Platform } from './PlatformBadge';

/*
 * A project's social desk (crates/cg-server/src/social/moderation.rs): read
 * once, then kept up to date by the `social` events of the socket.
 */

export type Action = 'reject' | 'check' | 'fix' | 'mask';
export type Status = 'pending' | 'check' | 'rejected' | 'approved';
export type NameMode = 'initial' | 'full' | 'anonymous';
export type TagKind = 'question' | 'photo' | 'link' | 'tox' | 'banned' | 'personal' | 'caps' | 'length' | 'emojis' | 'duplicate' | 'fixed' | 'check' | 'autoReject' | 'blocked' | 'trusted' | 'msgs';

export interface Tag {
  kind: TagKind;
  level: 'ok' | 'info' | 'warn' | 'bad';
  value?: string;
}

export interface Message {
  id: string;
  sourceId: string;
  platform: Platform;
  author: { id: string; name: string; handle?: string; avatar?: string; city?: string };
  text: string;
  photos?: string[];
  at: number;
  amount?: string;
  url?: string;
}

export interface Card {
  message: Message;
  status: Status;
  toxicity: number;
  tags: Tag[];
  /** For moderators (phone numbers masked). */
  author: string;
  /** Banned words masked. */
  shown: string;
  words?: string[];
  /** On air after the rules. */
  fixed: string;
  /** A moderator's text. */
  edit?: string;
  nameMode?: NameMode;
  starred: boolean;
  by?: string;
  /** Passes on air. */
  airs?: number;
  lastAir?: number;
  approvedAt?: number;
  /** Its own rule to leave the pool (undefined: the pool's). */
  leave?: Leave;
  retiredAt?: number;
}

export type Order = 'arrival' | 'starred' | 'random';
/** When a message leaves the pool: after `airs` passes, `minutes` after approval (0: never); both 0: by hand. */
export interface Leave {
  airs: number;
  minutes: number;
}

interface Filter {
  enabled: boolean;
  action: Action;
}

export interface Rules {
  toxicity: { enabled: boolean; high: number; low: number };
  banned: Filter & { words: string[]; builtin: boolean };
  links: Filter & { allow: string[] };
  caps: Filter & { percent: number };
  length: Filter & { min: number; max: number };
  emojis: Filter & { max: number };
  duplicates: Filter & { minutes: number };
  personal: Filter;
  trusted: { enabled: boolean; accounts: string[] };
  blocked: { enabled: boolean; accounts: string[] };
}

/** The pool on air: rotation, order, how messages leave it, how authors are named. */
export interface Display {
  rotation: boolean;
  rotationSecs: number;
  nameMode: NameMode;
  order: Order;
  leave: Leave;
  /** Most messages in the pool (0: no limit). */
  max: number;
}

export interface Hashtag {
  tag: string;
  count: number;
  perMin: number;
  series: number[];
  platforms: Record<string, number>;
  running: boolean;
  since: number;
  /** Per minute since `since`, oldest first. */
  history: number[];
}

export interface Desk {
  sourceId: string;
  sources: string[];
  rules: Rules;
  display: Display;
  hashtags: Hashtag[];
  incoming: Card[];
  approved: Card[];
  onAir: Card | null;
  /** Left the pool, oldest first. */
  retired: Card[];
  rotationAt: number | null;
  toxRejects: number;
  builtinWords: number;
}

/** A shared connection (main menu › Data). */
export interface Connection {
  id: string;
  name: string;
  platform: Platform;
  enabled: boolean;
  connected: boolean;
  perMin: number;
}

type Change =
  | { kind: 'incoming'; cards: Card[] }
  | { kind: 'removed'; ids: string[] }
  | { kind: 'queue'; approved: Card[]; onAir: Card | null; retired: Card[]; rotationAt: number | null }
  | { kind: 'settings'; settings: Pick<Desk, 'sources' | 'rules' | 'display'> }
  | { kind: 'hashtags'; hashtags: Hashtag[] }
  | { kind: 'reload' };

interface SocialState {
  projectId: string | null;
  desk: Desk | null;
  connections: Connection[];
  load: (projectId: string) => Promise<void>;
  loadConnections: () => Promise<void>;
  apply: (projectId: string, c: Change) => void;
}

const MAX = 1000;

export const useSocial = create<SocialState>((set, get) => ({
  projectId: null,
  desk: null,
  connections: [],
  async load(projectId) {
    if (get().projectId !== projectId) set({ projectId, desk: null });
    const desk = await api<Desk>('GET', `/api/projects/${encodeURIComponent(projectId)}/social`);
    if (get().projectId === projectId) set({ desk });
  },
  async loadConnections() {
    set({ connections: await api<Connection[]>('GET', '/api/social/sources') });
  },
  apply(projectId, c) {
    const d = get().desk;
    if (projectId !== get().projectId || !d) return;
    switch (c.kind) {
      case 'incoming': {
        const byId = new Map(c.cards.map((x) => [x.message.id, x]));
        const kept = d.incoming.map((x) => byId.get(x.message.id) ?? x);
        const known = new Set(d.incoming.map((x) => x.message.id));
        const incoming = [...kept, ...c.cards.filter((x) => !known.has(x.message.id))].slice(-MAX);
        set({ desk: { ...d, incoming } });
        break;
      }
      case 'removed': {
        const gone = new Set(c.ids);
        set({ desk: { ...d, incoming: d.incoming.filter((x) => !gone.has(x.message.id)) } });
        break;
      }
      case 'queue':
        set({ desk: { ...d, approved: c.approved, onAir: c.onAir, retired: c.retired, rotationAt: c.rotationAt } });
        break;
      case 'settings':
        set({ desk: { ...d, ...c.settings } });
        break;
      case 'hashtags':
        set({ desk: { ...d, hashtags: c.hashtags } });
        break;
      case 'reload':
        void get().load(projectId).catch(() => {});
        break;
    }
  },
}));

/** The project's desk, kept up to date (read again after a reconnection). */
export function useDesk(projectId: string) {
  const connected = useApp((s) => s.connected);
  const { load, loadConnections, apply } = useSocial();
  useEffect(() => {
    if (!connected) return;
    void load(projectId).catch((e) => useApp.getState().toast((e as Error).message, 'error'));
    void loadConnections().catch(() => {});
  }, [projectId, connected, load, loadConnections]);
  useEffect(() => {
    const on = (e: Event) => {
      const m = (e as CustomEvent<{ projectId: string; change: Change }>).detail;
      apply(m.projectId, m.change);
    };
    const conns = () => void loadConnections().catch(() => {});
    serverEvents.addEventListener('social', on);
    changeEvents.addEventListener('social', conns);
    return () => {
      serverEvents.removeEventListener('social', on);
      changeEvents.removeEventListener('social', conns);
    };
  }, [apply, loadConnections]);
  return useSocial((s) => (s.projectId === projectId ? s.desk : null));
}

const base = (projectId: string) => `/api/projects/${encodeURIComponent(projectId)}/social`;

/** A moderation or pool action (errors toasted). */
export async function act(projectId: string, action: string, body: object = {}) {
  try {
    await api('POST', `${base(projectId)}/${action}`, body);
  } catch (e) {
    useApp.getState().toast((e as Error).message, 'error');
  }
}

/** Replaces part of the settings (`follow`, `rules`, `hashtags`, `display`). */
export async function save(projectId: string, what: 'follow' | 'rules' | 'hashtags' | 'display', body: unknown) {
  try {
    await api('PUT', `${base(projectId)}/${what}`, body);
  } catch (e) {
    useApp.getState().toast((e as Error).message, 'error');
  }
}

/** The text on air of a card. */
export const airText = (c: Card) => c.edit ?? c.fixed;

/** "Sophie Lefebvre" → "Sophie L." (as the server names authors on air). */
export function airName(c: Card, mode: NameMode): string {
  const name = (c.message.author.name || c.message.author.handle || '').trim();
  const digits = name.replace(/\D/g, '').length;
  if (mode === 'anonymous' || (digits >= 4 && /^[\d+ .()•-]+$/.test(name) && name.replace(/[^\d•]/g, '').length >= 8)) return '';
  const words = name.split(/\s+/).filter(Boolean);
  if (mode === 'full' || name.startsWith('@') || words.length < 2) return name;
  return `${words[0]} ${words[words.length - 1][0].toUpperCase()}.`;
}

import { Check, GripVertical, Inbox, Pause, Pencil, RotateCcw, Search, Star, Undo2, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { connectionState, useSocialSources, type SocialInfo } from '../data/SocialSources';
import { locale, t, tc, tn } from '../i18n';
import { Dropdown } from '../ui/Dropdown';
import { Segmented } from '../ui/fields';
import { useApp } from '../store';
import { PlatformBadge } from './PlatformBadge';
import { act, airName, airText, save, useSocial, type Card, type Desk, type Leave, type NameMode, type Order, type Tag } from './socialStore';

/*
 * Moderation: the messages coming in (filtered), the pool on air (its
 * rotation, order and how messages leave it), and the message selected
 * (edited before air, its own rule). The desk never touches the antenna:
 * the Operator takes the graphics reading the pool on air.
 * Keys: A approve, R reject, E edit, S star, ↑↓ move through the messages.
 */

/** The label of a tag, as shown on a message. */
export function tagLabel(tag: Tag): string {
  switch (tag.kind) {
    case 'question':
      return t('QUESTION');
    case 'photo':
      return t('PHOTO');
    case 'link':
      return t('LINK');
    case 'tox':
      return t('TOX {n}', { n: Number(tag.value ?? 0).toLocaleString(locale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }) });
    case 'banned':
      return t('BANNED WORD');
    case 'personal':
      return t('PERSONAL DATA');
    case 'caps':
      return t('CAPITALS');
    case 'length':
      return t('LENGTH');
    case 'emojis':
      return t('EMOJIS');
    case 'duplicate':
      return t('DUPLICATE');
    case 'fixed':
      return t('FIXED');
    case 'check':
      return t('TO CHECK');
    case 'autoReject':
      return t('AUTO REJECT');
    case 'blocked':
      return t('BLOCKED ACCOUNT');
    case 'trusted':
      return t('TRUSTED ACCOUNT');
    case 'msgs':
      return t('{n} MSG', { n: tag.value ?? '' });
  }
}

/** Banned words, masked by the server, drawn as marks. */
function Shown({ text }: { text: string }) {
  const parts = text.split(/(•{4,})/);
  return (
    <>
      {parts.map((p, i) =>
        i % 2 ? (
          <mark key={i} className="mod-mask">
            {p}
          </mark>
        ) : (
          p
        ),
      )}
    </>
  );
}

const time = (at: number, seconds = true) => new Date(at).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit', second: seconds ? '2-digit' : undefined });

/** Server clock, every second (ages and times left). */
function useNow() {
  const offset = useApp((s) => s.serverOffset);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(h);
  }, []);
  return now + offset;
}

const ago = (ms: number) => (ms < 60_000 ? t('{n} s ago', { n: Math.max(0, Math.round(ms / 1000)) }) : ms < 3_600_000 ? t('{n} min ago', { n: Math.floor(ms / 60_000) }) : t('{n} h ago', { n: Math.floor(ms / 3_600_000) }));

/** The kinds a message can be filtered by. */
type Kind = 'question' | 'photo' | 'link' | 'check';
const KINDS: [Kind, () => string][] = [
  ['question', () => t('Questions')],
  ['photo', () => t('Photos')],
  ['link', () => t('Links')],
  ['check', () => t('To check')],
];
const has = (c: Card, kind: Kind) => (kind === 'photo' ? !!c.message.photos?.length : c.tags.some((g) => g.kind === kind));

export function Moderation({ projectId, desk, onConnections }: { projectId: string; desk: Desk; onConnections: () => void }) {
  const connections = useSocial((s) => s.connections);
  const [sel, setSel] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  // Filters: connections (none: all), kinds (none: all), hidden statuses.
  const [sources, setSources] = useState<Set<string>>(new Set());
  const [kinds, setKinds] = useState<Set<Kind>>(new Set());
  const [hideAuto, setHideAuto] = useState(true);
  const [hideDup, setHideDup] = useState(false);
  // Frozen: the list stays as it is while new messages wait.
  const [frozen, setFrozen] = useState<Set<string> | null>(null);
  const [editing, setEditing] = useState(0);

  const incoming = useMemo(() => {
    const q = query.trim().toLowerCase();
    return desk.incoming
      .filter((c) => !frozen || frozen.has(c.message.id))
      .filter((c) => !sources.size || sources.has(c.message.sourceId))
      .filter((c) => !kinds.size || [...kinds].some((k) => has(c, k)))
      .filter((c) => !hideAuto || !c.tags.some((g) => g.kind === 'autoReject'))
      .filter((c) => !hideDup || !c.tags.some((g) => g.kind === 'duplicate'))
      .filter((c) => !q || `${c.author} ${c.shown} ${c.message.author.city ?? ''}`.toLowerCase().includes(q))
      .slice()
      .reverse();
  }, [desk.incoming, query, sources, kinds, hideAuto, hideDup, frozen]);
  const waiting = frozen ? desk.incoming.filter((c) => !frozen.has(c.message.id)).length : 0;
  const pool = useMemo(() => [...(desk.onAir ? [desk.onAir] : []), ...desk.approved], [desk.onAir, desk.approved]);
  const all = useMemo(() => [...incoming, ...pool, ...desk.retired], [incoming, pool, desk.retired]);
  const selected = all.find((c) => c.message.id === sel) ?? null;
  const isIncoming = (id: string) => incoming.some((c) => c.message.id === id);
  const toggle = <T,>(set: Set<T>, v: T) => {
    const next = new Set(set);
    if (next.has(v)) next.delete(v);
    else next.add(v);
    return next;
  };

  // Keys: on the selected message.
  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e: KeyboardEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest('input, textarea, select, [contenteditable="true"]') || e.metaKey || e.ctrlKey || e.altKey) return;
    const list = sel && !isIncoming(sel) ? pool : incoming;
    const i = list.findIndex((c) => c.message.id === sel);
    const move = (d: number) => {
      const next = list[Math.min(list.length - 1, Math.max(0, i + d))];
      if (next) setSel(next.message.id);
    };
    // After an action on an incoming message: the next one.
    const after = () => {
      const next = list[i + 1] ?? list[i - 1];
      setSel(next ? next.message.id : null);
    };
    const k = e.key.toLowerCase();
    if (e.key === 'ArrowDown') move(1);
    else if (e.key === 'ArrowUp') move(-1);
    else if (!sel) return;
    else if (k === 'a' && isIncoming(sel)) {
      void act(projectId, 'approve', { id: sel });
      after();
    } else if (k === 'r') {
      void act(projectId, 'reject', { id: sel });
      after();
    } else if (k === 'e') setEditing((n) => n + 1);
    else if (k === 's') void act(projectId, 'star', { id: sel });
    else return;
    e.preventDefault();
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keys.current(e);
    addEventListener('keydown', h);
    return () => removeEventListener('keydown', h);
  }, []);

  const followed = desk.sources.map((id) => connections.find((c) => c.id === id)).filter((c): c is NonNullable<typeof c> => !!c);
  return (
    <div className={`mod-cols ${selected ? '' : 'two'}`}>
      <section className="mod-col">
        <header className="mod-head">
          <b>{t('Incoming')}</b>
          <span className="mod-count live">{desk.incoming.length}</span>
          <span className="spacer" />
          <label className="mod-search">
            <Search size={13} />
            <input value={query} placeholder={t('Search')} onChange={(e) => setQuery(e.target.value)} />
          </label>
          <button className={`mod-chip ${frozen ? 'on' : ''}`} onClick={() => setFrozen(frozen ? null : new Set(desk.incoming.map((c) => c.message.id)))}>
            <Pause size={12} /> {t('Freeze')}
            {waiting > 0 && <span className="mod-waiting">+{waiting}</span>}
          </button>
        </header>
        {desk.sources.length > 0 && (
          <div className="mod-filters">
            {followed.length > 1 &&
              followed.map((c) => (
                <button key={c.id} className={`mod-chip ${sources.has(c.id) ? 'on' : ''}`} title={c.name} onClick={() => setSources(toggle(sources, c.id))}>
                  <PlatformBadge platform={c.platform} size={14} />
                </button>
              ))}
            {KINDS.map(([k, label]) => (
              <button key={k} className={`mod-chip ${kinds.has(k) ? 'on' : ''}`} onClick={() => setKinds(toggle(kinds, k))}>
                {label()}
              </button>
            ))}
            <span className="spacer" />
            <button className={`mod-chip ${hideAuto ? 'on' : ''}`} title={t('Hide the automatic rejects')} onClick={() => setHideAuto(!hideAuto)}>
              {t('Auto rejects')}
            </button>
            <button className={`mod-chip ${hideDup ? 'on' : ''}`} title={t('Hide the duplicates')} onClick={() => setHideDup(!hideDup)}>
              {t('Duplicates')}
            </button>
          </div>
        )}
        <div className="mod-list">
          {!incoming.length && (desk.sources.length ? <Waiting desk={desk} /> : <NoConnection onConnections={onConnections} />)}
          {incoming.map((c) => (
            <IncomingCard
              key={c.message.id}
              card={c}
              selected={sel === c.message.id}
              onSelect={() => setSel(c.message.id)}
              projectId={projectId}
              onEdit={() => {
                setSel(c.message.id);
                setEditing((n) => n + 1);
              }}
            />
          ))}
        </div>
      </section>
      <Pool projectId={projectId} desk={desk} sel={sel} onSelect={setSel} />
      {selected && (
        <section className="mod-col">
          <EditPanel key={`${selected.message.id}-${editing}`} projectId={projectId} card={selected} desk={desk} focus={editing > 0} onClose={() => setSel(null)} />
        </section>
      )}
    </div>
  );
}

function Tags({ tags, words }: { tags: Tag[]; words?: string[] }) {
  return (
    <div className="mod-tags">
      {tags.map((g, i) => (
        <span key={i} className={`mod-tag ${g.level}`} title={g.kind === 'link' ? g.value : g.kind === 'banned' ? words?.join(', ') : undefined}>
          {tagLabel(g)}
        </span>
      ))}
    </div>
  );
}

function IncomingCard({ card: c, selected, onSelect, onEdit, projectId }: { card: Card; selected: boolean; onSelect: () => void; onEdit: () => void; projectId: string }) {
  const m = c.message;
  const n = m.photos?.length ?? 0;
  return (
    <div className={`mod-msg ${selected ? 'sel' : ''} ${c.status}`} onClick={onSelect}>
      <PlatformBadge platform={m.platform} size={20} />
      <div className="mod-msg-body">
        <div className="mod-who">
          <b>{c.author}</b>
          <span className="muted">
            {m.author.city ? ` · ${m.author.city}` : ''} · {time(m.at)}
          </span>
          {m.amount && <span className="mod-amount">{m.amount}</span>}
        </div>
        <div className="mod-text" title={c.words?.length ? c.words.join(', ') : undefined}>
          <Shown text={c.shown} />
          {n > 0 && <span className="mod-chip small">{tn(n, '{n} photo', '{n} photos')}</span>}
        </div>
        <Tags tags={c.tags} words={c.words} />
      </div>
      <div className="mod-qa" onClick={(e) => e.stopPropagation()}>
        <button className="yes" title={t('Approve (A)')} onClick={() => act(projectId, 'approve', { id: m.id })}>
          <Check size={13} />
        </button>
        <button className="no" title={t('Reject (R)')} onClick={() => act(projectId, 'reject', { id: m.id })}>
          <X size={13} />
        </button>
        <button title={t('Edit (E)')} onClick={onEdit}>
          <Pencil size={12} />
        </button>
        <button className={c.starred ? 'star' : ''} title={t('Star (S)')} onClick={() => act(projectId, 'star', { id: m.id })}>
          <Star size={12} fill={c.starred ? 'currentColor' : 'none'} />
        </button>
      </div>
    </div>
  );
}

/** How messages leave the pool, as a choice. */
type LeaveKind = 'hand' | 'airs' | 'minutes';
const leaveKind = (l: Leave): LeaveKind => (l.airs > 0 ? 'airs' : l.minutes > 0 ? 'minutes' : 'hand');
const leaveOf = (kind: LeaveKind, n: number): Leave => ({ airs: kind === 'airs' ? Math.max(1, n) : 0, minutes: kind === 'minutes' ? Math.max(1, n) : 0 });
const leaveText = (l: Leave) => (l.airs > 0 ? tn(l.airs, 'after {n} pass', 'after {n} passes') : l.minutes > 0 ? t('after {n} min', { n: l.minutes }) : t('by hand'));

/** A number in a chip, sent when left or on Enter. */
function NumChip({ label, value, unit, min, onChange, title }: { label?: string; value: number; unit?: string; min: number; onChange: (n: number) => void; title?: string }) {
  const [v, setV] = useState(String(value));
  useEffect(() => setV(String(value)), [value]);
  const send = () => {
    const n = Math.max(min, Math.round(Number(v)) || min);
    setV(String(n));
    if (n !== value) onChange(n);
  };
  return (
    <label className="mod-chip mod-secs static" title={title}>
      {label}
      <input type="number" min={min} value={v} onChange={(e) => setV(e.target.value)} onBlur={send} onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()} />
      {unit}
    </label>
  );
}

/** The pool on air: its settings, the message on air, the next ones (dragged to reorder), those that left. */
function Pool({ projectId, desk, sel, onSelect }: { projectId: string; desk: Desk; sel: string | null; onSelect: (id: string) => void }) {
  const [drag, setDrag] = useState<string | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const [gone, setGone] = useState(false);
  const d = desk.display;
  const now = useNow();
  const drop = (at: number) => {
    if (!drag) return;
    const ids = desk.approved.map((c) => c.message.id).filter((id) => id !== drag);
    const from = desk.approved.findIndex((c) => c.message.id === drag);
    ids.splice(from < at ? at - 1 : at, 0, drag);
    // Shown at once; the server sends the order to everyone.
    const byId = new Map(desk.approved.map((c) => [c.message.id, c]));
    useSocial.setState((s) => (s.desk ? { desk: { ...s.desk, approved: ids.map((id) => byId.get(id)!) } } : s));
    void act(projectId, 'reorder', { ids });
    setDrag(null);
    setOver(null);
  };
  const display = (patch: Partial<typeof d>) => save(projectId, 'display', { ...d, ...patch });
  const kind = leaveKind(d.leave);
  const left = d.rotation && desk.rotationAt !== null ? Math.max(0, Math.ceil((desk.rotationAt - now) / 1000)) : null;
  return (
    <section className="mod-col">
      <header className="mod-head">
        <b>{t('Pool on air')}</b>
        <span className="mod-count">{desk.approved.length + (desk.onAir ? 1 : 0)}</span>
        <span className="spacer" />
        <NumChip label={t('Rotation')} value={d.rotationSecs} unit={t('s')} min={2} title={t('One message every N seconds')} onChange={(n) => act(projectId, 'rotation', { on: d.rotation, secs: n })} />
        <label className="switch" title={t('Rotation')}>
          <input type="checkbox" checked={d.rotation} onChange={(e) => act(projectId, 'rotation', { on: e.target.checked, secs: d.rotationSecs })} />
        </label>
      </header>
      <div className="mod-filters">
        <Dropdown
          className="mod-set"
          title={t('Order')}
          value={d.order}
          options={[
            { value: 'arrival' as Order, label: t('As approved') },
            { value: 'starred' as Order, label: t('Starred first') },
            { value: 'random' as Order, label: t('Random') },
          ]}
          onChange={(order) => display({ order })}
        />
        <Dropdown
          className="mod-set"
          title={t('Leaves the pool')}
          value={kind}
          options={[
            { value: 'hand' as LeaveKind, label: t('Leaves by hand') },
            { value: 'airs' as LeaveKind, label: t('Leaves after passes') },
            { value: 'minutes' as LeaveKind, label: t('Leaves after minutes') },
          ]}
          onChange={(k) => display({ leave: leaveOf(k, k === 'airs' ? d.leave.airs || 3 : d.leave.minutes || 10) })}
        />
        {kind === 'airs' && <NumChip value={d.leave.airs} unit={t('passes')} min={1} onChange={(n) => display({ leave: leaveOf('airs', n) })} />}
        {kind === 'minutes' && <NumChip value={d.leave.minutes} unit={t('min')} min={1} onChange={(n) => display({ leave: leaveOf('minutes', n) })} />}
        <span className="spacer" />
        <NumChip label={t('At most')} value={d.max} unit={t('messages')} min={0} title={t('Most messages in the pool (0: no limit)')} onChange={(n) => display({ max: n })} />
      </div>
      <div className="mod-list mod-queue" onDragOver={(e) => e.preventDefault()} onDrop={() => drop(desk.approved.length)}>
        {desk.onAir && <QueueCard card={desk.onAir} badge="air" left={left} now={now} selected={sel === desk.onAir.message.id} onSelect={() => onSelect(desk.onAir!.message.id)} projectId={projectId} desk={desk} />}
        {desk.approved.map((c, i) => (
          <div
            key={c.message.id}
            draggable
            className={`mod-drag ${drag === c.message.id ? 'dragging' : ''} ${over === i && drag ? 'over' : ''}`}
            onDragStart={(e) => {
              e.dataTransfer.effectAllowed = 'move';
              setDrag(c.message.id);
            }}
            onDragEnd={() => {
              setDrag(null);
              setOver(null);
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setOver(i);
            }}
            onDrop={(e) => {
              e.stopPropagation();
              drop(i);
            }}
          >
            <QueueCard card={c} badge={i === 0 && d.order === 'arrival' ? 'next' : undefined} now={now} selected={sel === c.message.id} onSelect={() => onSelect(c.message.id)} projectId={projectId} desk={desk} />
          </div>
        ))}
        {!desk.onAir && !desk.approved.length && <NoMessage icon={<Inbox size={22} />} />}
        {desk.retired.length > 0 && (
          <>
            <button className="mod-gone-head" onClick={() => setGone(!gone)}>
              {t('Left the pool')} <span className="mod-count">{desk.retired.length}</span>
            </button>
            {gone &&
              desk.retired
                .slice()
                .reverse()
                .map((c) => <QueueCard key={c.message.id} card={c} badge="gone" now={now} selected={sel === c.message.id} onSelect={() => onSelect(c.message.id)} projectId={projectId} desk={desk} />)}
          </>
        )}
      </div>
    </section>
  );
}

/** Nothing followed: the way to the connections. */
function NoConnection({ onConnections }: { onConnections: () => void }) {
  return (
    <div className="empty big ux3-no-msg">
      <Inbox size={22} />
      <button className="btn primary big" onClick={onConnections}>
        {t('Follow a connection')}
      </button>
    </div>
  );
}

/** Connections followed, no message yet: their state. */
function Waiting({ desk }: { desk: Desk }) {
  const list = useSocialSources();
  const mine = desk.sources.map((id) => list.find((s) => s.id === id)).filter((s): s is SocialInfo => !!s);
  return (
    <div className="empty big ux3-no-msg">
      <Inbox size={22} />
      {mine.map((s) => {
        const st = connectionState(s);
        return (
          <span key={s.id} className="mod-waiting-conn" title={st.raw ?? st.text}>
            <i className={`status-dot ${st.dot}`} />
            <PlatformBadge platform={s.platform} size={15} />
            {s.name} · {st.text}
          </span>
        );
      })}
    </div>
  );
}

/** An empty column. */
function NoMessage({ icon }: { icon: React.ReactNode }) {
  return (
    <div className="empty big ux3-no-msg">
      {icon}
      <p>{t('No message')}</p>
    </div>
  );
}

/** A message of the pool: on air, waiting, or gone; its passes and its rule. */
function QueueCard({ card: c, badge, left, now, selected, onSelect, projectId, desk }: { card: Card; badge?: 'air' | 'next' | 'gone'; left?: number | null; now: number; selected: boolean; onSelect: () => void; projectId: string; desk: Desk }) {
  const m = c.message;
  const name = airName(c, c.nameMode ?? desk.display.nameMode);
  const rule = c.leave ?? desk.display.leave;
  const own = !!c.leave;
  const passes = rule.airs > 0 ? `${c.airs ?? 0} / ${rule.airs}` : String(c.airs ?? 0);
  return (
    <div className={`mod-qcard ${badge === 'air' ? 'air' : ''} ${badge === 'gone' ? 'gone' : ''} ${selected ? 'sel' : ''}`} onClick={onSelect}>
      {badge === 'gone' ? <span /> : <GripVertical size={14} className="mod-grip" />}
      <PlatformBadge platform={m.platform} size={20} />
      <div className="mod-msg-body">
        <div className="mod-who">
          {name ? <b>{name}</b> : <b className="mod-anon">{c.author}</b>}
          {m.author.city && <span className="muted"> {m.author.city}</span>}
        </div>
        <div className="mod-text">{airText(c)}</div>
        {c.edit !== undefined && (
          <div className="mod-edited">
            <Pencil size={11} /> {t('edited')}
          </div>
        )}
        {m.photos?.[0] && <img className="mod-photo" src={m.photos[0]} alt="" loading="lazy" />}
        <div className="mod-meta">
          <span title={own ? t('Its own rule: {rule}', { rule: leaveText(rule) }) : t('The pool’s rule: {rule}', { rule: leaveText(rule) })}>
            {t('Passes')} <b>{passes}</b>
            {own && ' ★'}
          </span>
          {c.lastAir && badge !== 'air' && (
            <span>
              {t('Last')} <b>{ago(now - c.lastAir)}</b>
            </span>
          )}
          {c.approvedAt && (
            <span>
              {t('Approved')} <b>{time(c.approvedAt, false)}</b>
              {c.by ? ` ${t('by {name}', { name: c.by })}` : ''}
            </span>
          )}
          {badge === 'gone' && c.retiredAt && (
            <span>
              {t('Left')} <b>{ago(now - c.retiredAt)}</b>
            </span>
          )}
          {badge === 'air' && left !== null && left !== undefined && (
            <span>
              {t('Left on air')} <b>{t('{n} s', { n: left })}</b>
            </span>
          )}
        </div>
      </div>
      <div className="mod-qside" onClick={(e) => e.stopPropagation()}>
        {badge === 'air' && <span className="mod-badge air">{tc('queue', 'ON AIR')}</span>}
        {badge === 'next' && <span className="mod-badge next">{t('NEXT')}</span>}
        <div className="mod-qa">
          {badge === 'gone' ? (
            <button className="take" title={t('Back in the pool')} onClick={() => act(projectId, 'restore', { id: m.id })}>
              <RotateCcw size={12} />
            </button>
          ) : (
            <>
              {badge === 'air' && (
                <button className="take" title={t('Next (skip)')} disabled={!desk.approved.length} onClick={() => act(projectId, 'next', { from: m.id })}>
                  <Undo2 size={12} style={{ transform: 'scaleX(-1)' }} />
                </button>
              )}
              <button title={t('Back to incoming')} onClick={() => act(projectId, 'remove', { id: m.id })}>
                <Undo2 size={12} />
              </button>
              <button className={c.starred ? 'star' : ''} title={t('Star (S)')} onClick={() => act(projectId, 'star', { id: m.id })}>
                <Star size={12} fill={c.starred ? 'currentColor' : 'none'} />
              </button>
              <button className="no" title={t('Out of the pool')} onClick={() => act(projectId, 'retire', { id: m.id })}>
                <X size={13} />
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

/** The message: its text before air, how the author is named, its own rule to leave the pool. */
function EditPanel({ projectId, card, desk, focus, onClose }: { projectId: string; card: Card; desk: Desk; focus: boolean; onClose: () => void }) {
  const mode = desk.display.nameMode;
  const [text, setText] = useState(airText(card));
  const [name, setName] = useState<NameMode>(card.nameMode ?? mode);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (focus) ref.current?.focus();
  }, [focus]);
  const original = card.message.text.trim();
  const changed = text.trim() !== airText(card) || name !== (card.nameMode ?? mode);
  const valid = async () => {
    await act(projectId, 'edit', { id: card.message.id, text, nameMode: name });
    ref.current?.blur();
  };
  const inPool = card.status === 'approved';
  const own = card.leave;
  const kind: 'common' | LeaveKind = own ? leaveKind(own) : 'common';
  const rule = (k: 'common' | LeaveKind, n?: number) => (k === 'common' ? act(projectId, 'rule', { id: card.message.id, common: true }) : act(projectId, 'rule', { id: card.message.id, ...leaveOf(k, n ?? (k === 'airs' ? own?.airs || 3 : own?.minutes || 10)) }));
  return (
    <div className="mod-edit">
      <div className="mod-edit-head">
        <b>{t('Edit before air')}</b>
        <span className="spacer" />
        <span className="muted">{airName(card, name) || card.author}</span>
      </div>
      <textarea
        ref={ref}
        value={text}
        rows={3}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) void valid();
          if (e.key === 'Escape') (e.target as HTMLTextAreaElement).blur();
        }}
      />
      {text.trim() !== original && <div className="mod-original">{original}</div>}
      <Segmented
        value={name}
        onChange={setName}
        options={[
          { value: 'initial', label: t('First name + initial') },
          { value: 'full', label: t('Full name') },
          { value: 'anonymous', label: t('Anonymous') },
        ]}
      />
      <div className="mod-edit-actions">
        <button className="btn" onClick={onClose}>
          {t('Cancel')}
        </button>
        <button className="btn primary" disabled={!changed} onClick={valid}>
          {tc('edit', 'Confirm')}
        </button>
      </div>
      {inPool && (
        <>
          <div className="mod-edit-head">
            <b>{t('Leaves the pool')}</b>
            <span className="spacer" />
            <span className="muted">{tn(card.airs ?? 0, '{n} pass', '{n} passes')}</span>
          </div>
          <Segmented
            value={kind}
            onChange={(k) => void rule(k)}
            options={[
              { value: 'common', label: t('Pool’s rule') },
              { value: 'hand', label: t('By hand') },
              { value: 'airs', label: t('Passes') },
              { value: 'minutes', label: t('Minutes') },
            ]}
          />
          {kind === 'airs' && <NumChip label={t('After')} value={own!.airs} unit={t('passes')} min={1} onChange={(n) => rule('airs', n)} />}
          {kind === 'minutes' && <NumChip label={t('After')} value={own!.minutes} unit={t('min')} min={1} onChange={(n) => rule('minutes', n)} />}
          {kind === 'common' && <span className="muted">{leaveText(desk.display.leave)}</span>}
        </>
      )}
    </div>
  );
}

import { RichInput } from '../ui/RichText';
import { plain, richField } from '../engine/rich';
import { AssetPicker } from '../designer/AssetPicker';
import { ChevronsRight, CircleStop, Clock, Copy, Eye, EyeOff, Radio, Database, Layers, MoreHorizontal, Pause, PenLine, Pencil, Play, Plus, RefreshCw, Repeat, RotateCcw, Rows3, SkipForward, Square, Timer, Trash2, Type, Undo2, X, Zap, GripVertical, ChevronDown, ChevronLeft, ChevronRight, ListPlus, MousePointerClick, Unplug, SlidersHorizontal, type LucideIcon } from 'lucide-react';
import { useShallow } from 'zustand/react/shallow';
import { createPortal } from 'react-dom';
import { Fragment, memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Scene, Viewport } from '../engine/Scene';
import { defaultData, holdFrame, interpolateText, newId, parseHex, toHex } from '../engine/evaluate';
import { oneKey, rowKey, usedDatasets, type Command, type DataSet, type Field, type Layer, type Marker, type Phase, type PlayoutItem, type Playlist, type Project, type ReadMode, type RundownItem, type Template, type Zone, type ChannelInfo } from '../engine/types';
import { isTimed, layerAgeFrames } from '../engine/widgets';
import { widgetOf } from '../widgets';
import { CentreModeContext } from '../widgets/kit';
import { Centre, OperatorBound, StartStatus } from '../widgets/centre';
import { LiveTime, useTicker } from '../engine/LiveTime';
import { formatFor, inFormat, projectFormats } from '../engine/formats';
import { api, can, thumbnailUrl, useApp, type Live } from '../store';
import { Program } from '../output/Program';
import { ColorField, NumberField, Segmented, TextInput } from '../ui/fields';
import { KeyScope, SliderNum } from '../widgets/kit';
import { AirRows, DataSetHand, DataSetPanel, DataSetSource, datasetUse, manualKey, shownData, sourcePatch, steppedDatasets } from './DataSets';
import { changedKeys, hasControls, heldAt, newPreviewRun, partKeys, PreviewControls, widgetParts, widgetWaits, type PreviewRun, type WidgetPart } from './WidgetTabs';
import { FedText, LayerProperties, type ExposedLayer, type FieldIO } from './Exposed';
import { shownText } from '../widgets/text';
import { bindingLabel, readBinding, useBoundData, useSources } from '../data/dataStore';
import { DataWarning } from './DataWarning';
import { navigate, projectPath } from '../router';
import { areaClip, zoneArea } from '../engine/zones';
import { confirmDialog, promptDialog } from '../ui/dialogs';
import { TableRowsField } from './TableRowsField';
import { Dropdown, type DropOption } from '../ui/Dropdown';
import { Splitter, useLayout } from '../ui/Splitter';
import { lang, t, tc, tn } from '../i18n';
import { TakenNotice, TakeOver, useZoneLocks, ZonesButton } from './ZoneBar';
import { Avatar, OperatorTop } from './Presence';
import { requestUndo, UndoShortcut } from './UndoToast';
import { KeyCap, openShortcuts } from './ShortcutsOverlay';
import { ScheduleCell, ScheduleTop, TimecodeChip, toggleScheduleList } from './Schedule';
import { actionLabel, useShortcuts } from './shortcuts';
import type { ZoneLock } from '../store';

/*
 * Operator page, four areas (each resizable):
 *   left    playlists, and the graphics of the playlist with take / out
 *   center  the preview of the selected graphic, and its variables and data
 *   right   what is on air (the output's pictures), and the internal
 *           controls of the graphics on air (widgets, values, triggers)
 */

/** An empty area: a small icon and a word, in its middle. */
function Empty({ icon: Icon, text, children }: { icon: LucideIcon; text: string; children?: React.ReactNode }) {
  return (
    <div className="opx-void">
      <span className="opx-void-ic">
        <Icon size={18} />
      </span>
      <span className="opx-void-t">{text}</span>
      {children}
    </div>
  );
}

/** The control of an exposed value, as in the Designer (text, choices, number, colour, picture, on / off). */
export function FieldInput({ field, value, onChange, rich, maxLength }: { field: Field; value: string; onChange: (v: string) => void; rich?: boolean; /** Most characters (a Text's « Max characters »). */ maxLength?: number }) {
  // A text with styles set in the Designer (bold, colour…): kept, shown, without a style bar.
  if (rich) return <RichInput bare multiline={field.kind === 'multiline'} value={value} onChange={onChange} maxLength={maxLength} />;
  if (field.options?.length) {
    const short = field.options.length <= 4 && field.options.every(([, l]) => l.length <= 12);
    return short ? (
      <Segmented value={value} onChange={onChange} options={field.options.map(([v, l]) => ({ value: v, label: l }))} />
    ) : (
      <select value={value} onChange={(e) => onChange(e.target.value)}>
        {!field.options.some(([v]) => v === value) && <option value={value}>{value}</option>}
        {field.options.map(([v, l]) => (
          <option key={v} value={v}>
            {l}
          </option>
        ))}
      </select>
    );
  }
  switch (field.kind) {
    case 'multiline':
      return <TextInput multiline autoGrow value={value} onChange={onChange} maxLength={maxLength} />;
    case 'number':
      // Between bounds: a slider.
      if (field.min != null && field.max != null) return <SliderNum value={parseFloat(value) || 0} min={field.min} max={field.max} step={field.max - field.min > 20 ? 1 : 0.1} precision={field.max - field.min > 20 ? 0 : 1} onChange={(v) => onChange(String(v))} />;
      return (
        <span className="pp-field">
          <NumberField value={parseFloat(value) || 0} min={field.min} max={field.max} step={1} precision={value.includes('.') ? Math.min(3, value.split('.')[1].length) : 0} onChange={(v) => onChange(String(v))} />
        </span>
      );
    case 'boolean':
      return (
        <button className={`wk-tog ${value === 'true' ? 'on' : ''}`} onClick={() => onChange(value === 'true' ? 'false' : 'true')}>
          <i />
        </button>
      );
    case 'color':
      return (
        <span className="pp-field color">
          <ColorField value={parseHex(value) ?? [1, 1, 1, 1]} onChange={(x) => onChange(toHex(x, x[3] < 1))} />
        </span>
      );
    case 'image':
      return <AssetPicker value={value} onChange={onChange} />;
    case 'table':
      return <TableRowsField value={value} onChange={onChange} />;
    default:
      return <TextInput autoGrow value={value} onChange={onChange} maxLength={maxLength} />;
  }
}

/**
 * What Update sends: the values the operator set, without the rows of the data sets (NEXT is not
 * the row on air) and without the values read from a source (they are read again on air).
 */
function updateData(t: Template, draft: Record<string, string>, read: ReadMode): Record<string, string> {
  const ds = usedDatasets(t).map((d) => d.key);
  // A data set only feeding Texts: its chosen row (and all rows / one row) go with the other values.
  const sent = new Set(usedDatasets(t).flatMap((d) => (datasetUse(t, d).source ? [rowKey(d.key), oneKey(d.key)] : [])));
  // Typed by hand over the data (`<key>__manual`): sent, and its mark too (empty: back to the data).
  const byHand = (k: string) => !!draft[manualKey(k)]?.trim() || k.endsWith('__manual');
  const bound = new Set(read === 'manual' ? [] : t.fields.filter((f) => f.binding?.source && !byHand(f.key)).map((f) => f.key));
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries({ ...defaultData(t), ...draft })) {
    if (bound.has(k) || (!sent.has(k) && !byHand(k) && ds.some((d) => k.startsWith(`${d}__`)))) continue;
    out[k] = v;
  }
  return { ...out, ...sourcePatch(t) };
}

/** Fields grouped by layer ("Name · Vertical": group Name, label Vertical); a layer's only value named after the layer. */
function fieldGroups(fields: Field[], names: Map<string, string>): { title: string; fields: { f: Field; label: string }[] }[] {
  const out: { title: string; fields: { f: Field; label: string }[] }[] = [];
  for (const f of fields) {
    const at = f.label.indexOf(' · ');
    const title = at > 0 ? f.label.slice(0, at) : '';
    const label = names.get(f.key) || (at > 0 ? f.label.slice(at + 3) : f.label);
    let g = out.find((x) => x.title === title);
    if (!g) out.push((g = { title, fields: [] }));
    g.fields.push({ f, label });
  }
  return out;
}

const rundownName = (project: Project, id?: string | null) => (id ? project.rundown.find((r) => r.id === id)?.name : undefined);

/** The first text typed for a graphic (a person's name…): the item's name follows it. */
const firstText = (t: Template) => t.fields.find((f) => (f.kind === 'text' || f.kind === 'multiline') && !f.options?.length && !f.binding?.source);
const firstLine = (v: string | undefined) => plain(v ?? '').split('\n')[0].trim();

/**
 * The item's new name when its first text changes, as long as the name was
 * that text (or the template's name): renamed by hand, it stays. `pendingName`:
 * a new name not saved yet.
 */
function followName(t: Template, name: string, pendingName: string | undefined, before: Record<string, string>, next: Record<string, string>): string | undefined {
  const f = firstText(t);
  if (!f || before[f.key] === next[f.key]) return undefined;
  const current = (pendingName ?? name).trim();
  if (current && current !== firstLine(before[f.key]) && current !== t.name) return undefined;
  const v = firstLine(next[f.key]);
  return v || t.name;
}

/** The tab id of the fields that belong to no widget layer (texts, exposed shape settings). */
const SHARED_TAB = '__texts__';

/** The pause marker the graphic holds on, if it holds on a pause (not on the On air point). */
function pauseOf(p: PlayoutItem, project: Project): Marker | undefined {
  if (p.phase !== 'holding' && p.phase !== 'outHold') return undefined;
  const t = project.templates.find((x) => x.id === p.templateId);
  return t?.markers.find((m) => m.kind === 'pause' && Math.abs(m.frame - p.frame) < 0.5);
}

/** Continue would move it on: a widget waits for it (its next page, its start), or the graphic is held on a pause, playing towards one, or paused while going out (as the server's `resume`, crates/cg-core/src/playout.rs). */
function canContinue(p: PlayoutItem, t: Template | undefined): boolean {
  if (!t || p.phase === 'out') return false;
  if (widgetWaits(t.layers, p)) return true;
  if (p.phase === 'outHold') return true;
  return Math.max(p.target, p.frame) < holdFrame(t) - 1e-6;
}

/** What a graphic on air does by itself, the soonest: a timed pause plays on, it goes out (IN/OUT marker after a delay), or auto next takes the next graphic. `at`: client time (ms); `secs`: the whole delay. As the server, the time counts from the stop it holds at. */
interface AutoStep {
  kind: 'resume' | 'out' | 'next';
  at: number;
  secs: number;
}
function autoStep(project: Project, p: PlayoutItem & { advanced?: boolean }, liveAt: number): AutoStep | null {
  if (p.phase !== 'holding') return null;
  const t = project.templates.find((x) => x.id === p.templateId);
  if (!t) return null;
  const fps = project.frameRate.num / project.frameRate.den || 1;
  const step = (kind: AutoStep['kind'], secs: number): AutoStep => ({ kind, secs: Math.max(0, secs), at: liveAt + (Math.max(0, secs) - (p.held ?? 0) / fps) * 1000 });
  const pause = pauseOf(p, project);
  if (pause) return pause.resume?.type === 'after' ? step('resume', pause.resume.seconds) : null;
  if (p.frame < holdFrame(t) - 1e-6) return null;
  const steps: AutoStep[] = [];
  const out = t.markers.find((m) => m.kind === 'out');
  if (out?.resume?.type === 'after') steps.push(step('out', out.resume.seconds));
  const r = p.rundownId ? project.rundown.find((x) => x.id === p.rundownId) : undefined;
  if (!p.advanced && r?.autoSecs !== undefined && project.playlists.some((x) => x.id === r.playlist && x.autoNext)) steps.push(step('next', r.autoSecs));
  return steps.sort((a, b) => a.at - b.at)[0] ?? null;
}

/** Seconds left before an automatic step, ticking. */
function useLeft(step: AutoStep | null): number {
  const now = useTicker(!!step, 10);
  return step ? Math.max(0, (step.at - now) / 1000) : 0;
}

/** Content scaled to fit its box (both ways), on a checkerboard. */
function Screen({ width, height, children }: { width: number; height: number; children: React.ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0.2);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setScale(Math.max(0.01, Math.min(el.clientWidth / width, el.clientHeight / height))));
    ro.observe(el);
    return () => ro.disconnect();
  }, [width, height]);
  return (
    <div className="opx-screen" ref={ref}>
      <Viewport width={width} height={height} scale={scale} className="op-viewport">
        {children}
      </Viewport>
    </div>
  );
}

const READ_MODES: { id: ReadMode; label: string; hint: string }[] = [
  { id: 'manual', label: 'Manually', hint: 'Only when you press Read now: the values are saved in the item' },
  { id: 'take', label: 'On take', hint: 'Once, when the graphic is taken' },
  { id: 'live', label: 'Live', hint: 'On take, then every time the source changes, also on air' },
];
const INTERVALS = [2, 5, 10, 30, 60, 300];
/** Auto next of a new graphic: the next one after this time on air. */
const AUTO_SECS = 5;

/** Tooltip of the mini timeline of a graphic on air, by phase (translated where shown). */
const PHASE_TITLES: Record<Phase, string> = {
  playing: 'On air · playing',
  holding: 'On air · holding',
  out: 'On air · out',
  outHold: 'On air · outHold',
};

export function Operator({ channelId }: { channelId: string | null }) {
  const { project, command: send, saveRundown, auth, channels, toast } = useApp(useShallow((s) => ({ project: s.project, command: s.command, saveRundown: s.saveRundown, auth: s.auth, channels: s.channels, toast: s.toast })));
  const allSources = useSources();
  const sourceName = (id: string) => allSources.find((s) => s.id === id)?.name ?? id;
  const liveState = useApp((s) => (channelId ? s.live[channelId] : undefined));
  const channel = liveState?.state ?? { layers: {} };
  // Editing the rundown needs the rights; going on air also needs an output.
  const canEdit = can.operate(auth?.user);
  const operate = canEdit && !!channelId;
  const command: typeof send = (cmd) => {
    if (!operate) return;
    const go = () => send(cmd, channelId!);
    // A new name on its way: saved first, so the action (and the as-run) has it.
    if (pending.current?.name !== undefined) void flush().then(go);
    else go();
  };
  const [layout, setLayout] = useLayout('pulse.operatorLayout', { left: 340, right: 620, preview: 400, air: 330, lists: 200, fields: 50 });
  const [cued, setCued] = useState<string | null>(null);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [updateFade, setUpdateFade] = useUpdateFade();
  // The widget tab shown, by rundown item; what the preview's controls changed.
  const [tabOf, setTabOf] = useState<Record<string, string>>({});
  // Every widget's content and data together, or a tab each (default).
  const [merged, setMerged] = useState(() => {
    try {
      return localStorage.getItem('strata.opMerged') === '1';
    } catch {
      return false;
    }
  });
  const toggleMerged = () =>
    setMerged((m) => {
      try {
        localStorage.setItem('strata.opMerged', m ? '' : '1');
      } catch {
        /* private mode */
      }
      return !m;
    });
  const [pv, setPv] = useState<PreviewRun>(newPreviewRun);
  const [adding, setAdding] = useState(false);
  const [playlistId, setPlaylistId] = useState<string | null>(null);
  const [shown, setShown] = useState<string[]>(['main']);
  // The monitors shown: the preview, the on-air picture or both (never none).
  const [mon, setMon] = useMonitors();
  const tilesRef = useRef<HTMLDivElement>(null);
  // The widget of a graphic on air whose keys are on (`zone:layer`, a click on its row), and the ⋯ unfolded.
  const [airWidget, setAirWidget] = useState<string | null>(null);
  const [airMore, setAirMore] = useState<Record<string, boolean>>({});
  // Reordering the graphics by dragging their grip.
  const [dragging, setDragging] = useState<string | null>(null);
  const [dropAt, setDropAt] = useState<{ id: string; where: 'before' | 'after' } | null>(null);
  const original = useRef<{ id: string; data: Record<string, string> } | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const channelInfo = channels.find((c) => c.id === channelId);
  // Zones held by other operators: their graphics are locked here.
  const zoneLocks = useZoneLocks(channelId, project);

  const rundown = project?.rundown ?? [];
  const playlists = project?.playlists ?? [];
  const activeList = playlists.find((p) => p.id === playlistId)?.id ?? playlists[0]?.id ?? '';
  // An empty template (or one that became empty) is not part of the rundown.
  const playable = (id: string) => !!project?.templates.find((t) => t.id === id)?.layers.some((l) => l.visible !== false);
  const items = rundown.filter((r) => (r.playlist === activeList || !playlists.some((p) => p.id === r.playlist)) && playable(r.templateId));
  const activePl = playlists.find((p) => p.id === activeList);
  const setPlaylist = (patch: Partial<Playlist>) =>
    project &&
    api(
      'PUT',
      `/api/projects/${encodeURIComponent(project.id)}/playlists`,
      playlists.map((p) => (p.id === activeList ? { ...p, ...patch } : p)),
    ).catch((e) => toast((e as Error).message, 'error'));
  const item = rundown.find((r) => r.id === cued && playable(r.templateId)) ?? null;
  const template = item ? project?.templates.find((t) => t.id === item.templateId) : undefined;
  const readMode: ReadMode = item?.read ?? 'live';

  useEffect(() => {
    if (!cued || !items.some((r) => r.id === cued)) setCued(items.length ? items[0].id : null);
  }, [cued, activeList, items.length]);

  // The draft follows the selected item; its data on selection is what Revert restores.
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const pending = useRef<{ id: string; data: Record<string, string>; name?: string } | null>(null);
  useEffect(() => {
    if (!item || !template) return;
    const fresh = { ...defaultData(template), ...item.data };
    draftRef.current = fresh;
    setDraft(fresh);
    original.current = { id: item.id, data: structuredClone(item.data) };
    setPv(newPreviewRun());
  }, [cued, template?.id]);
  // Changed elsewhere (another operator, MOS, the API) or new fields after a publish: follow, unless an edit is on its way.
  useEffect(() => {
    if (!item || !template || pending.current?.id === item.id) return;
    const fresh = { ...defaultData(template), ...item.data };
    if (JSON.stringify(fresh) === JSON.stringify(draftRef.current)) return;
    draftRef.current = fresh;
    setDraft(fresh);
  }, [item?.data, template?.fields]);

  // Bound fields read live show the source's value (manual ones show what was read).
  const boundDraft = useBoundData(readMode === 'manual' ? undefined : template, draft);
  // Kept while nothing changes: the preview is not drawn again for nothing.
  const previewData = useMemo(() => ({ ...(template ? defaultData(template) : {}), ...draft, ...boundDraft }), [template, draft, boundDraft]);

  /** Saves the item's data a moment after the last edit. */
  const flush = async () => {
    clearTimeout(saveTimer.current);
    const p = pending.current;
    pending.current = null;
    if (!p) return;
    const cur = useApp.getState().project?.rundown ?? [];
    await saveRundown(cur.map((r) => (r.id === p.id ? { ...r, data: p.data, ...(p.name !== undefined ? { name: p.name } : {}) } : r)));
  };
  const saveData = (id: string, data: Record<string, string>, name?: string) => {
    // Another item's edit still on its way: saved now, not dropped.
    if (pending.current && pending.current.id !== id) void flush();
    pending.current = { id, data, name: name ?? (pending.current?.id === id ? pending.current.name : undefined) };
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => void flush(), 500);
  };
  const editDraft = (patch: Record<string, string>) => {
    if (!item) return;
    // From the latest draft: two edits in the same moment both count.
    const before = draftRef.current;
    const next = { ...before, ...patch };
    draftRef.current = next;
    setDraft(next);
    saveData(item.id, next, template ? followName(template, item.name, pending.current?.id === item.id ? pending.current.name : undefined, before, next) : undefined);
  };
  const setItem = (id: string, patch: Partial<RundownItem>) => {
    // Renamed by hand: the name no longer follows the first text.
    if (patch.name !== undefined && pending.current?.id === id) pending.current.name = undefined;
    return saveRundown(rundown.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  };

  const onAirByItem = useMemo(() => {
    const m = new Map<string, PlayoutItem>();
    for (const p of Object.values(channel.layers)) if (p.rundownId) m.set(p.rundownId, p);
    return m;
  }, [channel]);

  const zoneOf = (r: RundownItem) => project?.zones.find((z) => z.id === r.zone) ?? project?.zones.find((z) => z.id === project.templates.find((t) => t.id === r.templateId)?.zone);
  const dataOf = (r: RundownItem) => {
    const t = project?.templates.find((x) => x.id === r.templateId);
    return { ...(t ? defaultData(t) : {}), ...(r.id === item?.id ? draft : r.data) };
  };
  const takeItem = (r: RundownItem, extra?: Record<string, string>) => {
    const tpl = project?.templates.find((x) => x.id === r.templateId);
    command({ action: 'take', zone: zoneOf(r)?.id, templateId: r.templateId, data: { ...dataOf(r), ...(tpl ? sourcePatch(tpl) : {}), ...extra }, rundownId: r.id });
  };
  /** The NEXT row the operator chose for a data set of the graphic on air (in the cued item), if it is that one. */
  const nextRowOf = (onAir: PlayoutItem, key: string) => (item && onAirByItem.get(item.id) === onAir ? parseInt(draft[rowKey(key)] ?? '', 10) || undefined : undefined);
  /** The graphic after `r` in the playlist (the first again when it loops). */
  const after = (r: RundownItem) => {
    const i = items.findIndex((x) => x.id === r.id);
    const n = i >= 0 && i < items.length - 1 ? items[i + 1] : activePl?.loop ? items[0] : undefined;
    return n && n.id !== r.id ? n : undefined;
  };
  /** NEXT: takes the cued graphic (the one after it when it is already on air), then cues the following one. */
  const nextTarget = item && onAirByItem.get(item.id) ? after(item) : (item ?? undefined);
  const take = () => {
    if (!nextTarget) return;
    takeItem(nextTarget);
    const n = after(nextTarget);
    if (n) setCued(n.id);
  };
  const outZone = (z: Zone | undefined) => z && command({ action: 'takeOut', zone: z.id });
  const continueZone = (z: Zone | undefined) => z && command({ action: 'continue', zone: z.id });

  // On air, front-most zone first; the internal controls show one of them.
  const zones = useMemo(() => [...(project?.zones ?? [])].sort((a, b) => b.layer - a.layer), [project?.zones]);
  const busy = zones.filter((z) => channel.layers[String(z.layer)]);
  const airList = useRef<HTMLDivElement>(null);
  useAirBento(airList, busy.length > 0);
  // CONTINUE / OUT drive the front-most graphic on air.
  const selZone = busy[0];

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // A key the selected widget on air took (its own shortcuts) is not the Operator's.
      if (e.defaultPrevented || (e.target as HTMLElement).closest('input, textarea, select')) return;
      if (e.key === 'Escape' && airWidget) (e.preventDefault(), setAirWidget(null));
      // « ? »: the keyboard shortcuts (not over a dialog).
      if (e.key === '?' && !e.ctrlKey && !e.metaKey && !e.altKey && !document.querySelector('.modal-back')) (e.preventDefault(), openShortcuts());
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
  // The user's keyboard shortcuts (Settings › Shortcuts), through the same functions as the buttons.
  useShortcuts({
    next: take,
    take: () => item && takeItem(item),
    continue: () => continueZone(selZone),
    continueAll: () => busy.forEach(continueZone),
    out: () => outZone(selZone),
    outSelected: () => item && outZone(zoneOf(item)),
    clear: () => command({ action: 'clear' }),
    undo: requestUndo,
    update: () => item && template && onAirByItem.get(item.id) && operate && !lockOf(item) && command({ action: 'update', zone: zoneOf(item)?.id, data: updateData(template, draftRef.current, readMode), fade: updateFade }),
    select: (d) => {
      const i = items.findIndex((r) => r.id === cued) + d;
      if (i >= 0 && i < items.length) setCued(items[i].id);
    },
    takeAt: (n) => items[n - 1] && takeItem(items[n - 1]),
    outZone: (name) => outZone(project?.zones.find((z) => z.name === name) ?? project?.zones.find((z) => z.id === name)),
    trigger: (name) => command({ action: 'trigger', name }),
    schedule: toggleScheduleList,
  });

  if (!project) return <div className="opx-empty">{t('No project loaded.')}</div>;

  // Pictures of the output (main + renditions); the ones without a format in the project are off.
  const formats = projectFormats(project);
  const designed = (w: number, h: number) => formats.some((f) => Math.abs(Math.log(f.width / f.height) - Math.log(w / h)) < 0.01);
  const pictures = channelInfo ? [{ id: 'main', name: `${channelInfo.width}×${channelInfo.height}`, width: channelInfo.width, height: channelInfo.height }, ...channelInfo.renditions] : formats.map((f) => ({ id: f.id, name: f.id === 'main' ? t('Main') : f.name, width: f.width, height: f.height }));
  const visible = pictures.filter((p) => shown.includes(p.id) && designed(p.width, p.height));
  const programs = visible.length ? visible : pictures.slice(0, 1);
  const main = pictures[0];
  const previewFormat = formatFor(project, main.width, main.height);
  const itemZone = item ? zoneOf(item) : undefined;
  const fps = project.frameRate.num / project.frameRate.den || 1;
  const lang = project.language || 'fr';
  // The selected graphic widget by widget (a tab each when there are several).
  const parts = template ? widgetParts(template) : { shared: [], widgets: [] };
  // The fields outside the widget layers live in their own tab (Texts) once widgets have tabs.
  const sharedSel = !!item && tabOf[item.id] === SHARED_TAB && parts.shared.length > 0 && parts.widgets.length > 0;
  const part = sharedSel ? undefined : ((item && parts.widgets.find((p) => p.layer.id === tabOf[item.id])) ?? parts.widgets[0]);
  // The widgets shown: the tab's, or all of them together.
  const together = merged && parts.widgets.length > 1;
  const shownParts = together ? parts.widgets : part ? [part] : [];
  const bound = [...new Set(shownParts.flatMap((p) => p.bound))];
  const datasets = [...new Map(shownParts.flatMap((p) => p.datasets).map((d) => [d.key, d])).values()];
  // The data part only when the widgets read data (a source or a data set).
  const hasData = datasets.length > 0 || bound.length > 0;
  // The widgets' own transport in the preview, at the top of their values.
  const controls = mon.preview ? shownParts.filter((p) => p.controls) : [];
  // Nothing to type for them (a table fed by a data set…): the data takes the width.
  const showShared = together || sharedSel || parts.widgets.length === 0;
  const hasContent = (showShared && parts.shared.length > 0) || (!part && !sharedSel) || shownParts.some((p) => p.fields.length > 0 || !!p.exposed || p.fed.length > 0);
  // The values by section: one per layer with the layers together (its name, its values, its data in front of them); one in a tab.
  const sections: { key: string; title?: string; parts: WidgetPart[]; shared: boolean; data: boolean }[] = together
    ? [...(parts.shared.length ? [{ key: SHARED_TAB, title: t('Texts'), parts: [], shared: true, data: false }] : []), ...parts.widgets.map((p, i) => ({ key: p.layer.id, title: tabName(parts.widgets, i), parts: [p], shared: false, data: true }))]
    : [{ key: 'tab', parts: shownParts, shared: showShared, data: part !== undefined }];
  // What Update would change on air, by widget.
  const onAirNow = item ? onAirByItem.get(item.id) : undefined;
  const sent = template ? updateData(template, draft, readMode) : {};
  const changed = changedKeys(sent, onAirNow ? shownData(onAirNow) : undefined);
  const partChanged = (p: WidgetPart) => !!template && partKeys(template, p).some((k) => changed.has(k));
  const updates = parts.widgets.filter(partChanged).length + (parts.shared.some((f) => changed.has(f.key)) ? 1 : 0) || (changed.size ? 1 : 0);
  const pvHold = template ? holdFrame(template) / fps : 0;
  const pvAge = () => pvHold + (pv.active ? Math.max(0, Date.now() - pv.since) / 1000 : 0);
  const setRun = (f: (r: PreviewRun) => PreviewRun) => setPv((r) => f(r.active ? r : { ...r, active: true, since: Date.now() }));

  // A graphic of a zone another operator holds: no take, out or update here.
  const lockOf = (r: RundownItem | null | undefined) => (r && channelId ? zoneLocks.other(zoneOf(r)?.id) : undefined);
  const operateItem = operate && !lockOf(item);

  return (
    <div className="opl-page">
    {channelId && <ZonesButton project={project} channelId={channelId} locks={zoneLocks.locks} operate={operate} />}
    <OperatorTop channelId={channelId} operate={operate} />
    <TopControls channelId={channelId} channelInfo={channelInfo} />
    <ScheduleTop channelId={channelId} project={project} />
    <TakenNotice />
    <div className="opx" style={{ gridTemplateColumns: `${layout.left}px 1px minmax(0, 1fr) 1px ${layout.right}px` }}>
      {/* Left: playlists and graphics. */}
      <div className="opx-col">
        <section className="opx-pane grow">
          <div className="opx-pltabs">
            <PlaylistTabs project={project} active={activeList} onSelect={setPlaylistId}>
              {playlists.map((p) => (
                <PlaylistRow key={p.id} project={project} id={p.id} active={p.id === activeList} editable={canEdit} onSelect={setPlaylistId} />
              ))}
            </PlaylistTabs>
            {canEdit && (
              <button
                className="opx-add"
                title={t('New playlist')}
                onClick={async () => {
                  const name = await promptDialog({ title: t('New playlist'), label: t('Name'), value: t('Playlist {n}', { n: playlists.length + 1 }), confirm: t('Create') });
                  if (!name) return;
                  const id = newId();
                  await api('PUT', `/api/projects/${encodeURIComponent(project.id)}/playlists`, [...playlists, { id, name }]).catch((e) => toast((e as Error).message, 'error'));
                  setPlaylistId(id);
                }}
              >
                <Plus size={15} />
              </button>
            )}
          </div>
          <header className="opx-head">
            <span className="opx-title">{t('Rundown')}</span>
            {activePl && canEdit && (
              <>
                <button className={`opx-btn small ${activePl.autoNext ? 'violet' : ''}`} title={t('Auto next: each graphic takes the next one after its time (set on each graphic)')} onClick={() => setPlaylist({ autoNext: !activePl.autoNext || undefined })}>
                  <Timer size={13} /> {t('Auto next')}
                </button>
                <button className={`opx-btn small ${activePl.loop ? 'violet' : ''}`} title={t('Loop: after the last graphic, the first again')} onClick={() => setPlaylist({ loop: !activePl.loop || undefined })}>
                  <Repeat size={13} />
                </button>
              </>
            )}
            {canEdit && (
              <button className="opx-add" title={t('Add a graphic from a template')} onClick={() => setAdding(!adding)}>
                <Plus size={15} />
              </button>
            )}
          </header>
          {adding && (
            <AddGraphic
              project={project}
              templates={project.templates.filter((t) => playable(t.id))}
              used={(id) => items.filter((r) => r.templateId === id).length}
              onClose={() => setAdding(false)}
              onPick={(t) => {
                const z = project.zones.find((x) => x.id === t.zone);
                const it: RundownItem = { id: newId(), name: t.name, templateId: t.id, zone: t.zone, playlist: activeList, layer: z?.layer ?? 0, data: defaultData(t), autoSecs: AUTO_SECS };
                saveRundown([...rundown, it]);
                setCued(it.id);
                setAdding(false);
              }}
            />
          )}
          <div className="opx-scroll">
            {items.map((r) => (
              <GraphicRow
                key={r.id}
                project={project}
                item={r}
                zone={zoneOf(r)}
                air={onAirByItem.get(r.id)}
                liveAt={liveState?.at ?? 0}
                selected={r.id === cued}
                cue={r.id === nextTarget?.id}
                operate={operate && !lockOf(r)}
                lockedBy={lockOf(r)}
                takeOver={lockOf(r) && channelId ? <TakeOver channelId={channelId} lock={lockOf(r)!} operate={operate} /> : undefined}
                schedule={(editAt, onClose) => <ScheduleCell channelId={channelId} project={project} item={r} editAt={editAt} onClose={onClose} />}
                canSchedule={!!channelId && operate && !lockOf(r)}
                editable={canEdit}
                onSelect={() => setCued(r.id)}
                onTake={() => takeItem(r)}
                onOut={() => outZone(zoneOf(r))}
                onContinue={() => continueZone(zoneOf(r))}
                onRename={(name) => setItem(r.id, { name })}
                autoNext={!!activePl?.autoNext}
                onAuto={(patch) => setItem(r.id, patch)}
                onDuplicate={() => {
                  const next = [...rundown];
                  next.splice(rundown.indexOf(r) + 1, 0, { ...structuredClone(r), id: newId(), name: t('{name} copy', { name: r.name }) });
                  saveRundown(next);
                }}
                onDelete={async () => {
                  if (!(await confirmDialog({ title: t('Remove “{name}”?', { name: r.name }), message: t('It leaves the playlist (the template stays).'), confirm: t('Remove'), danger: true }))) return;
                  saveRundown(rundown.filter((x) => x.id !== r.id));
                  if (cued === r.id) setCued(null);
                }}
                drag={{
                  over: dropAt?.id === r.id ? dropAt.where : null,
                  onStart: (e) => {
                    setDragging(r.id);
                    e.dataTransfer.effectAllowed = 'move';
                    e.dataTransfer.setData('text/plain', r.id);
                    const row = (e.currentTarget as HTMLElement).closest('.opx-gr');
                    if (row) e.dataTransfer.setDragImage(row, 20, 20);
                  },
                  onOver: (e) => {
                    if (!dragging) return;
                    e.preventDefault();
                    const b = e.currentTarget.getBoundingClientRect();
                    const where = e.clientY < b.top + b.height / 2 ? 'before' : 'after';
                    if (dragging !== r.id && (dropAt?.id !== r.id || dropAt.where !== where)) setDropAt({ id: r.id, where });
                  },
                  onDrop: (e) => {
                    e.preventDefault();
                    if (dragging && dropAt && dragging !== r.id) {
                      const moved = rundown.find((x) => x.id === dragging)!;
                      const rest = rundown.filter((x) => x.id !== dragging);
                      const at = rest.findIndex((x) => x.id === r.id) + (dropAt.where === 'after' ? 1 : 0);
                      rest.splice(at, 0, moved);
                      saveRundown(rest);
                    }
                    setDragging(null);
                    setDropAt(null);
                  },
                  onEnd: () => (setDragging(null), setDropAt(null)),
                }}
              />
            ))}
            {!items.length && (
              <Empty icon={ListPlus} text={t('Empty playlist')}>
                {canEdit && (
                  <button className="opx-btn small" onClick={() => setAdding(true)}>
                    <Plus size={13} /> {t('Add a graphic')}
                  </button>
                )}
              </Empty>
            )}
          </div>
        </section>
      </div>

      <Splitter axis="x" className="opx-gap" value={layout.left} min={260} max={560} reset={340} onChange={(v) => setLayout('left', v)} />

      {/* Center: preview, then the graphic's values as tiles. */}
      <div className="opx-col">
        <>
            <section className="opx-pane" style={mon.preview ? { height: layout.preview, flex: 'none' } : { flex: 'none' }}>
              <header className="opx-head opx-mon-head">
                <span className="opx-title">{t('Preview')}</span>
                {item && template && (
                  <>
                    <span className="opx-mon-name" title={t('{template} · zone {zone}', { template: template.name, zone: itemZone?.name ?? '—' })}>
                      <Pencil size={12} className="muted" />
                      <input className="opx-name" value={item.name} disabled={!canEdit} onChange={(e) => setItem(item.id, { name: e.target.value })} onKeyDown={(e) => e.stopPropagation()} aria-label={t('Name in the playlist')} />
                    </span>
                    {itemZone && (
                      <span className="opx-zpill" style={{ ['--zc' as string]: itemZone.color }}>
                        {itemZone.name}
                      </span>
                    )}
                  </>
                )}
                <span className="spacer" />
                <MonitorToggle on={mon.preview} onChange={(preview) => setMon({ preview })} />
              </header>
              {mon.preview && (
              <div className="opx-monitor">
                <Screen width={previewFormat.width} height={previewFormat.height}>
                  {template && (
                    <div style={{ position: 'absolute', inset: 0, ...areaClip(itemZone && zoneArea(itemZone, previewFormat.id, previewFormat.height > previewFormat.width)) }}>
                      <PreviewScene template={template} format={previewFormat.id} data={previewData} fps={fps} run={pv} lang={lang} />
                    </div>
                  )}
                </Screen>
              </div>
              )}
            </section>
            {mon.preview && <Splitter axis="y" className="opx-gap" value={layout.preview} min={160} max={900} reset={400} onChange={(v) => setLayout('preview', v)} />}
        </>
        <section className="opx-pane grow">
          {item && template ? (
            <>
              {(parts.widgets.length > 1 || (parts.widgets.length > 0 && parts.shared.length > 0)) && (
                <div className="opx-head opx-tabs opx-wtabs">
                  {!together && (
                    <>
                  {parts.widgets.map((p, i) => {
                    const Icon = widgetOf(p.layer)?.icon;
                    const ws = pv.widgets[p.layer.id] as { running?: boolean } | undefined;
                    return (
                      <button key={p.layer.id} className={`opx-wtab ${!sharedSel && p === part ? 'on' : ''}`} onClick={() => setTabOf({ ...tabOf, [item.id]: p.layer.id })}>
                        {Icon && <Icon size={13} />}
                        <span>{tabName(parts.widgets, i)}</span>
                        {ws?.running && <i className="opx-run" title={t('Animation running')} />}
                        {partChanged(p) && <i className="opx-chg" />}
                      </button>
                    );
                  })}
                  {parts.shared.length > 0 && (
                    <button className={`opx-wtab ${sharedSel ? 'on' : ''}`} onClick={() => setTabOf({ ...tabOf, [item.id]: SHARED_TAB })}>
                      <Type size={13} />
                      <span>{t('Texts')}</span>
                      {parts.shared.some((f) => changed.has(f.key)) && <i className="opx-chg" />}
                    </button>
                  )}
                    </>
                  )}
                  <span className="spacer" />
                  {parts.widgets.length > 1 && (
                    <button className={`opx-btn small opx-together ${together ? 'violet' : ''}`} aria-pressed={together} title={together ? t('One tab per layer') : t('All the layers together')} onClick={toggleMerged}>
                      <Layers size={13} /> {t('Layers together')}
                    </button>
                  )}
                </div>
              )}
              {hasContent || hasData || controls.length > 0 ? (
                <div className="opx-tiles" ref={tilesRef}>
                  {sections.map((sec) => {
                    const sBound = [...new Set(sec.parts.flatMap((p) => p.bound))];
                    const sSets = sec.data ? [...new Map(sec.parts.flatMap((p) => p.datasets).map((d) => [d.key, d])).values()] : [];
                    const sData = sSets.length > 0 || (sec.data && sBound.length > 0);
                    const sControls = mon.preview ? sec.parts.filter((p) => p.controls) : [];
                    const sFields = [...(sec.shared ? parts.shared : []), ...sec.parts.flatMap((p) => p.fields)];
                    const sContent = sFields.length > 0 || sec.parts.some((p) => !!p.exposed || p.fed.length > 0);
                    if (!sContent && !sData && !sControls.length) return null;
                    // The columns its layers read (its values and their settings): only those are shown.
                    const only = readColumns(sec.parts, sSets);
                    return (
                      <section key={sec.key} className="opx-sect">
                        {sec.title && <div className="opx-sect-h">{sec.title}</div>}
                        <div className={`opx-sect-b ${sData ? 'split' : ''}`}>
                          {(sContent || sControls.length > 0) && (
                            <div className="opx-tcol">
                              {/* The preview's own transport, in its layer (it hides none of the picture; nothing goes on air). */}
                              {sControls.map((p) => (
                                <div key={`${item.id}-${p.layer.id}`} className="opx-pvrow">
                                  <PreviewControls layer={p.layer} run={pv} setRun={setRun} data={previewData} fps={fps} ageSecs={pvAge} holdSecs={pvHold} lang={lang} onData={editDraft} mode="air" />
                                </div>
                              ))}
                              {sContent && <ValuesPane template={template} fields={sFields} names={new Map(sec.parts.flatMap((p) => (p.fields.length === 1 && (p.layer.type !== 'text' || p.layer.text.trim() === `{{${p.fields[0].key}}}`) ? [[p.fields[0].key, p.layer.name.trim()]] : [])))} exposed={sec.parts.flatMap((p) => (p.exposed ? [p.exposed] : []))} fedLayers={sec.parts.filter((p) => p.fed.length).map((p) => p.layer)} changed={changed} draft={draft} boundDraft={boundDraft} readMode={readMode} sourceName={sourceName} previewData={previewData} onDraft={editDraft} />}
                            </div>
                          )}
                          {sData && (
                            <div className="opx-tcol data">
                              {sSets.map((ds) =>
                                datasetUse(template, ds).source ? (
                                  <div key={ds.key} className="opx-ds">
                                    <DataSetSource ds={ds} template={template} draft={draft} onAir={onAirNow} sourceName={sourceName(ds.source)} onDraft={editDraft} only={only.get(ds.key)} />
                                  </div>
                                ) : (
                                  <div key={ds.key} className="opx-ds opx-ds-stack">
                                    <DataSetPanel ds={ds} template={template} draft={draft} onAir={onAirNow} operate={operateItem} zone={itemZone?.id} fps={fps} onDraft={editDraft} command={command} keys={ds === steppedDatasets(template)[0]} only={only.get(ds.key)} />
                                    <DataSetHand ds={ds} template={template} draft={draft} onAir={onAirNow} operate={operateItem} zone={itemZone?.id} onDraft={editDraft} command={command} />
                                  </div>
                                ),
                              )}
                              {sBound.length > 0 && <DataBlock project={project} template={template} fields={sBound} item={item} draft={draft} canEdit={canEdit} onRead={(patch) => editDraft(patch)} onMode={(m) => setItem(item.id, { read: m === 'live' ? undefined : m })} show={together ? 'cards' : 'all'} />}
                            </div>
                          )}
                        </div>
                      </section>
                    );
                  })}
                  {/* The layers together: how the sources are read, once, under them. */}
                  {together && bound.length > 0 && (
                    <section className="opx-sect">
                      <div className="opx-sect-b split">
                        <div />
                        <div className="opx-tcol data">
                          <DataBlock project={project} template={template} fields={bound} item={item} draft={draft} canEdit={canEdit} onRead={(patch) => editDraft(patch)} onMode={(m) => setItem(item.id, { read: m === 'live' ? undefined : m })} show="controls" />
                        </div>
                      </div>
                    </section>
                  )}
                  {/* In a tab: a cable from each value read from the data to its column or source. */}
                  {!together && hasData && <Wires root={tilesRef} />}
                </div>
              ) : (
                <Empty icon={SlidersHorizontal} text={t('No value to type')} />
              )}
            </>
          ) : (
            <Empty icon={MousePointerClick} text={t('No graphic selected')} />
          )}
        </section>
      </div>

      <Splitter axis="x" className="opx-gap" sign={-1} value={layout.right} min={320} max={1100} reset={620} onChange={(v) => setLayout('right', v)} />

      {/* Right: on air (the same size as the preview), then the controls of the graphics on air. */}
      <div className="opx-col">
        <>
            <section className="opx-pane" style={mon.air ? { height: layout.preview, flex: 'none' } : { flex: 'none' }}>
              <header className="opx-head opx-mon-head">
                <span className={`opx-title ${busy.length ? 'live' : ''}`}>● {t('On air')}</span>
                {pictures.length > 1 && (
                  <div className="opx-pics">
                    {pictures.map((p) => {
                      const ok = designed(p.width, p.height);
                      const on = ok && (shown.includes(p.id) || (!visible.length && p === pictures[0]));
                      return (
                        <button key={p.id} className={on ? 'on' : ''} disabled={!ok} title={ok ? `${p.width}×${p.height}` : t('{size}: not designed in this project (add the format in the Designer)', { size: `${p.width}×${p.height}` })} onClick={() => setShown(on ? shown.filter((x) => x !== p.id) : [...shown, p.id])}>
                          {p.name}
                        </button>
                      );
                    })}
                  </div>
                )}
                <span className="spacer" />
                <MonitorToggle on={mon.air} onChange={(air) => setMon({ air })} />
              </header>
              {mon.air && (
              <div className="opx-programs">
                {channelId ? (
                  programs.map((p) => (
                    <div key={p.id} className="opx-monitor" style={{ flex: `${p.width / p.height} 1 0` }}>
                      <Screen width={p.width} height={p.height}>
                        {/* A monitor: 30 frames a second are enough (the HTML5 output runs at the display rate). */}
                        <Program project={project} live={liveState} width={p.width} height={p.height} maxFps={MONITOR_FPS} />
                      </Screen>
                    </div>
                  ))
                ) : (
                  <Empty icon={Unplug} text={t('No output')} />
                )}
              </div>
              )}
            </section>
            {mon.air && <Splitter axis="y" className="opx-gap" value={layout.preview} min={160} max={900} reset={400} onChange={(v) => setLayout('preview', v)} />}
        </>
        <section className="opx-pane grow">
          {busy.length ? (
            <div className="opx-airlist" ref={airList}>
              {busy.map((z) => {
                const p = channel.layers[String(z.layer)];
                return (
                  <AirCard
                    key={z.id}
                    project={project}
                    zone={z}
                    item={p}
                    liveAt={liveState?.at ?? 0}
                    operate={operate && !zoneLocks.other(z.id)}
                    lockedBy={channelId ? zoneLocks.other(z.id) : undefined}
                    takeOver={channelId && zoneLocks.other(z.id) ? <TakeOver channelId={channelId} lock={zoneLocks.other(z.id)!} operate={operate} /> : undefined}
                    command={command}
                    nextRowOf={(key) => nextRowOf(p, key)}
                    widget={{ selected: airWidget, onSelect: setAirWidget, more: airMore, onMore: (k, open) => setAirMore({ ...airMore, [k]: open }) }}
                  />
                );
              })}
            </div>
          ) : (
            <Empty icon={Radio} text={t('Nothing on air')} />
          )}
        </section>
      </div>
    </div>
    {/* The transport, the whole width: NEXT, CONTINUE, OUT · timecode, undo, clear, shortcuts. */}
    <footer className="ux1-transport">
      <NextButton project={project} live={liveState} target={nextTarget} disabled={!nextTarget || !operate || !!lockOf(nextTarget)} onTake={take} cap={<KeyCap action="next" />} />
      <button className={`opx-btn ${selZone && pauseOf(channel.layers[String(selZone.layer)], project) ? 'amber' : ''}`} disabled={!operate || !selZone || !!zoneLocks.other(selZone.id)} onClick={() => continueZone(selZone)} title={actionLabel('continue')}>
        <ChevronsRight size={16} /> Continue <KeyCap action="continue" />
      </button>
      <button className="opx-btn" disabled={!operate || !selZone || !!zoneLocks.other(selZone.id)} onClick={() => outZone(selZone)} title={actionLabel('out')}>
        <CircleStop size={16} /> Out <KeyCap action="out" />
      </button>
      <span className="opx-tsep" />
      <div className={`opx-upd ${updates && onAirNow && operateItem ? 'ready' : ''}`}>
        <button className="opx-upd-go" disabled={!onAirNow || !operateItem || !updates} title={t(updateFade ? 'Send these values to the graphic on air (cross-fade)' : 'Send these values to the graphic on air (cut)')} onClick={() => command({ action: 'update', zone: itemZone?.id, data: sent, fade: updateFade })}>
          <RefreshCw size={16} /> Update{updates && onAirNow ? <span className="opx-upd-n">{updates}</span> : null} <KeyCap action="update" />
        </button>
        <UpdateMode value={updateFade} onChange={setUpdateFade} disabled={false} />
      </div>
      <span className="spacer" />
      {channelId && <TimecodeChip channelId={channelId} />}
      <button className="opx-btn" disabled={!operate} onClick={requestUndo} title={actionLabel('undo')}>
        <Undo2 size={16} /> {t('Undo')} <KeyCap action="undo" />
      </button>
      <button className="opx-btn ux1-clear" disabled={!operate || !Object.keys(channel.layers).length} onClick={() => command({ action: 'clear' })} title={actionLabel('clear')}>
        {t('Clear all')} <KeyCap action="clear" />
      </button>
      <button className="opx-btn ux1-help" title={t('Keyboard shortcuts')} aria-label={t('Keyboard shortcuts')} onClick={openShortcuts}>
        ?
      </button>
    </footer>
    <UndoShortcut channelId={channelId} operate={operate} />
    </div>
  );
}

/** The Operator's monitors and countdowns redraw at most this many times a second. */
const MONITOR_FPS = 30;

/** An auto next on its way: the graphic on air, the one it takes, and when (client clock, ms). */
function pendingAutoNext(project: Project, live: Live | undefined): { from: RundownItem; to: RundownItem | undefined; at: number; secs: number } | null {
  if (!live) return null;
  const fps = project.frameRate.num / project.frameRate.den || 1;
  let best: { from: RundownItem; to: RundownItem | undefined; at: number; secs: number } | null = null;
  for (const p of Object.values(live.state.layers) as (PlayoutItem & { advanced?: boolean })[]) {
    if (p.advanced || p.phase === 'out' || p.phase === 'outHold' || !p.rundownId) continue;
    const r = project.rundown.find((x) => x.id === p.rundownId);
    if (!r || r.autoSecs === undefined) continue;
    // Counts from the on-air point (after the in animation and the pauses before it).
    const tpl = project.templates.find((x) => x.id === p.templateId);
    if (!tpl || p.phase !== 'holding' || p.frame < holdFrame(tpl) - 1e-6) continue;
    const list = project.playlists.find((x) => x.id === r.playlist && x.autoNext);
    if (!list) continue;
    // As the server: the graphics of the playlist (an empty template is not part of it).
    const items = project.rundown.filter((x) => x.playlist === r.playlist && (x.id === r.id || !!project.templates.find((t) => t.id === x.templateId)?.layers.some((l) => l.visible !== false)));
    const i = items.findIndex((x) => x.id === r.id);
    const to = items[i + 1] ?? (list.loop ? items[0] : undefined);
    const at = live.at + (Math.max(0, r.autoSecs) - (p.held ?? 0) / fps) * 1000;
    if (!best || at < best.at) best = { from: r, to, at, secs: r.autoSecs };
  }
  return best;
}

/** NEXT: takes the cued graphic; with an auto next on its way, its countdown (and what it takes). */
function NextButton({ project, live, target, disabled, onTake, cap }: { project: Project; live: Live | undefined; target: RundownItem | undefined; disabled: boolean; onTake: () => void; /** Its key. */ cap?: React.ReactNode }) {
  const auto = pendingAutoNext(project, live);
  const now = useTicker(!!auto, MONITOR_FPS);
  const left = auto ? Math.max(0, (auto.at - now) / 1000) : 0;
  if (auto) {
    const done = auto.secs > 0 ? Math.min(1, Math.max(0, 1 - left / auto.secs)) : 1;
    return (
      <button className="opx-btn big red opx-next-auto" disabled={disabled} onClick={onTake} title={t('F1 / Enter: takes {name}', { name: target?.name ?? t('the next graphic') })}>
        <span className="opx-next-fill" style={{ width: `${done * 100}%` }} />
        <span className="opx-next-top">
          <b className="opx-next-n">{Math.ceil(left - 1e-3)}</b>
          {/* NEXT keeps its broadcast name. */}
          <span className="opx-next-unit">s</span>
          <span>· Next</span>
        </span>
        <span className="opx-next-sub">{auto.to ? t('Auto: {name}', { name: auto.to.name }) : t('Auto: out')}</span>
      </button>
    );
  }
  return (
    <button className="opx-btn big red" disabled={disabled} onClick={onTake} title={t('F1 / Enter: takes {name}', { name: target?.name ?? t('the next graphic') })}>
      <SkipForward size={15} fill="currentColor" /> Next
      {target && <span className="opx-next-name">{target.name}</span>}
      {cap}
    </button>
  );
}

/** The playlist tabs on one line; when they do not fit: < > to scroll a tab at a time, and ▾ listing them all. */
function PlaylistTabs({ project, active, onSelect, children }: { project: Project; active: string; onSelect: (id: string) => void; children: React.ReactNode }) {
  const box = useRef<HTMLDivElement>(null);
  const [edge, setEdge] = useState({ over: false, left: false, right: false });
  const [list, setList] = useState<{ left: number; top: number } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const read = () => {
      const over = el.scrollWidth > el.clientWidth + 1;
      const next = { over, left: over && el.scrollLeft > 1, right: over && el.scrollLeft < el.scrollWidth - el.clientWidth - 1 };
      setEdge((e) => (e.over === next.over && e.left === next.left && e.right === next.right ? e : next));
    };
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    for (const c of el.children) ro.observe(c);
    el.addEventListener('scroll', read, { passive: true });
    return () => (ro.disconnect(), el.removeEventListener('scroll', read));
  }, [project.playlists.length]);
  // The open playlist stays in sight.
  useEffect(() => {
    box.current?.querySelector<HTMLElement>('.opx-pl.on')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [active]);
  useEffect(() => {
    if (!list) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.preventDefault(), setList(null));
    const away = (e: PointerEvent) => !listRef.current?.contains(e.target as Node) && setList(null);
    window.addEventListener('keydown', esc);
    window.addEventListener('pointerdown', away, true);
    return () => (window.removeEventListener('keydown', esc), window.removeEventListener('pointerdown', away, true));
  }, [list]);
  // A tab at a time, to the left or the right.
  const step = (dir: -1 | 1) => {
    const el = box.current;
    if (!el) return;
    const tabs = [...el.children] as HTMLElement[];
    const x = el.scrollLeft;
    const to = dir > 0 ? tabs.find((c) => c.offsetLeft + c.offsetWidth > x + el.clientWidth + 1) : [...tabs].reverse().find((c) => c.offsetLeft < x - 1);
    if (to) el.scrollTo({ left: dir > 0 ? to.offsetLeft + to.offsetWidth - el.clientWidth : to.offsetLeft, behavior: 'smooth' });
  };
  return (
    <>
      {edge.over && (
        <button className="opx-pl-arrow" disabled={!edge.left} title={t('Previous')} onClick={() => step(-1)}>
          <ChevronLeft size={15} />
        </button>
      )}
      <div className="opx-pltabs-scroll" ref={box} role="tablist" aria-label={t('Playlists')}>
        {children}
      </div>
      {edge.over && (
        <>
          <button className="opx-pl-arrow" disabled={!edge.right} title={t('Next')} onClick={() => step(1)}>
            <ChevronRight size={15} />
          </button>
          <button
            className={`opx-pl-arrow ${list ? 'open' : ''}`}
            title={t('Playlists')}
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              setList(list ? null : { left: Math.max(8, Math.min(r.right - 220, window.innerWidth - 228)), top: r.bottom + 4 });
            }}
          >
            <ChevronDown size={15} />
          </button>
        </>
      )}
      {list &&
        createPortal(
          <div ref={listRef} className="menu project-menu clip-menu opx-pl-menu" style={{ position: 'fixed', left: list.left, top: list.top, right: 'auto', width: 220 }}>
            {project.playlists.map((p) => (
              <button key={p.id} className={p.id === active ? 'on' : ''} onClick={() => (setList(null), onSelect(p.id))}>
                <span className="opx-pl-menu-n">{p.name}</span>
                <em>{project.rundown.filter((r) => r.playlist === p.id).length}</em>
              </button>
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}

// Not drawn again on each frame of the channel (only its props matter).
const PlaylistRow = memo(function PlaylistRow({ project, id, active, editable, onSelect }: { project: Project; id: string; active: boolean; editable: boolean; onSelect: (id: string) => void }) {
  const toast = useApp((s) => s.toast);
  // The "…" menu: where it opens (under the button, kept in the window).
  const [menu, setMenu] = useState<{ left: number; top: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  // Esc, a click elsewhere or a resize closes it.
  useEffect(() => {
    if (!menu) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.preventDefault(), setMenu(null));
    const away = (e: PointerEvent) => !menuRef.current?.contains(e.target as Node) && setMenu(null);
    const close = () => setMenu(null);
    window.addEventListener('keydown', esc);
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('keydown', esc);
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('resize', close);
    };
  }, [menu]);
  const p = project.playlists.find((x) => x.id === id)!;
  const count = project.rundown.filter((r) => r.playlist === id).length;
  const save = (lists: Project['playlists']) => api('PUT', `/api/projects/${encodeURIComponent(project.id)}/playlists`, lists).catch((e) => toast((e as Error).message, 'error'));
  const run = (fn: () => unknown) => () => (setMenu(null), void fn());
  const duplicate = async () => {
    const copy = { ...structuredClone(p), id: newId(), name: t('{name} copy', { name: p.name }) };
    const lists = [...project.playlists];
    lists.splice(lists.indexOf(p) + 1, 0, copy);
    await save(lists);
    const items = project.rundown.filter((r) => r.playlist === id).map((r) => ({ ...structuredClone(r), id: newId(), playlist: copy.id }));
    if (items.length) await useApp.getState().saveRundown([...project.rundown, ...items]);
    onSelect(copy.id);
  };
  return (
    <div className={`opx-pl ${active ? 'on' : ''}`} role="tab" aria-selected={active} tabIndex={0} title={tn(count, '{n} item', '{n} items')} onClick={() => onSelect(id)} onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), onSelect(id))}>
      <span className="opx-name-cell">{p.name}</span>
      <span className="opx-pl-n">{count}</span>
      {editable && (
        <button
          className={`opx-x opx-pl-more ${menu ? 'open' : ''}`}
          title={t('More')}
          onClick={(e) => {
            e.stopPropagation();
            const r = e.currentTarget.getBoundingClientRect();
            setMenu(menu ? null : { left: Math.max(8, Math.min(r.left, window.innerWidth - 228)), top: r.bottom + 4 });
          }}
        >
          <MoreHorizontal size={14} />
        </button>
      )}
      {menu &&
        createPortal(
          <div ref={menuRef} className="menu project-menu clip-menu opx-pl-menu" style={{ position: 'fixed', left: menu.left, top: menu.top, right: 'auto' }} onClick={(e) => e.stopPropagation()}>
            <button
              onClick={run(async () => {
                const name = await promptDialog({ title: t('Rename the playlist'), label: t('Name'), value: p.name, confirm: t('Rename') });
                if (name) save(project.playlists.map((x) => (x.id === id ? { ...x, name } : x)));
              })}
            >
              <Pencil size={13} /> {t('Rename…')}
            </button>
            <button onClick={run(duplicate)}>
              <Copy size={13} /> {t('Duplicate')}
            </button>
            {project.playlists.length > 1 && (
              <>
                <div className="menu-sep" />
                <button
                  className="danger"
                  onClick={run(async () => {
                    const other = project.playlists.find((x) => x.id !== id)!;
                    if (await confirmDialog({ title: t('Remove the playlist “{name}”?', { name: p.name }), message: tn(count, 'Its {n} graphic moves to “{name}”.', 'Its {n} graphics move to “{name}”.', { name: other.name }), confirm: t('Remove'), danger: true })) save(project.playlists.filter((x) => x.id !== id));
                  })}
                >
                  <Trash2 size={13} /> {t('Remove…')}
                </button>
              </>
            )}
          </div>,
          document.body,
        )}
    </div>
  );
});

/**
 * The left part of a graphic: its values (texts, pictures… and what the Texts
 * fed by a data set show), then the properties exposed in the Designer, layer
 * by layer, with the Designer's controls and sections.
 */
function ValuesPane({ template, fields, names, exposed, fedLayers, changed, draft, boundDraft, readMode, sourceName, previewData, onDraft }: { template: Template; fields: Field[]; /** A widget's only value: its layer's name, not the label set at import (often the text itself). */ names: Map<string, string>; exposed: ExposedLayer[]; fedLayers: Layer[]; changed: Set<string>; draft: Record<string, string>; boundDraft: Record<string, string>; readMode: ReadMode; sourceName: (id: string) => string; previewData: Record<string, string>; onDraft: (patch: Record<string, string>) => void }) {
  // Typed by hand over the data, until the operator goes back to it.
  const byHand = (k: string) => !!draft[manualKey(k)]?.trim();
  const fromData = (f: Field) => !!f.binding?.source && readMode !== 'manual';
  const live = (f: Field) => fromData(f) && !byHand(f.key);
  /** The pen (type over the data, from what it shows) or the data (back to it). */
  const handToggle = (keys: string[], shown: (k: string) => string) =>
    keys.some(byHand) ? (
      <button className="icon-btn ghost" title={t('Back to the data')} onClick={() => onDraft(Object.fromEntries(keys.map((k) => [manualKey(k), ''])))}>
        <Database size={13} />
      </button>
    ) : (
      <button className="icon-btn ghost" title={t('Type it by hand')} onClick={() => onDraft(Object.fromEntries(keys.flatMap((k) => [[k, shown(k)], [manualKey(k), 'removed']])))}>
        <PenLine size={13} />
      </button>
    );
  const io: FieldIO = {
    value: (f) => draft[f.key] ?? '',
    set: (f, v) => onDraft({ [f.key]: v }),
    fed: (f) => (live(f) ? (boundDraft[f.key] ?? draft[f.key] ?? '…') : null),
    changed: (f) => changed.has(f.key),
  };
  // A Text a data set feeds (it is only its source): what it shows, read only.
  const fed: { layer: Layer; title: string; cols: string[]; at: number }[] = [];
  for (const ds of usedDatasets(template)) {
    const use = datasetUse(template, ds);
    if (!use.source || !fedLayers.length) continue;
    const row = Math.max(1, parseInt(draft[rowKey(ds.key)] ?? '1', 10) || 1);
    for (const l of use.texts) {
      if (!fedLayers.some((x) => x.id === l.id) || fed.some((x) => x.layer.id === l.id)) continue;
      // The columns it shows (typed by hand over the row, one by one).
      const at = new RegExp(`\\{\\{\\s*(${ds.key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}_[A-Za-z0-9][A-Za-z0-9_]*)\\s*\\}\\}`, 'g');
      const cols = l.type !== 'text' && l.type !== 'typewriter' ? [] : [...new Set([...l.text.matchAll(at)].map((m) => m[1]))];
      fed.push({ layer: l, title: t('From {dataset}: row {row}', { dataset: ds.name, row }), cols, at: ds.key.length + 1 });
    }
  }
  const fedChanged = fedLayers.length > 0 && usedDatasets(template).some((d) => changed.has(rowKey(d.key)) || changed.has(oneKey(d.key)));
  const dot = (on: boolean, label: React.ReactNode) =>
    on ? (
      <>
        <i className="opx-chg" />
        {label}
      </>
    ) : (
      label
    );
  // A value a data set gives (its column), unless typed by hand.
  const fieldKeys = new Set(fields.map((f) => f.key));
  const columnOf = (k: string) => usedDatasets(template).find((d) => k.startsWith(`${d.key}_`) && k[d.key.length + 1] !== '_');
  /** A value read from the data: read only, in the data's colour, the column or path it comes from, the pen to type it by hand. */
  const linkedRow = (k: string, label: string, value: string, chip: string, kind: 'ds' | 'live', wire: string, toggle: React.ReactNode) => (
    <div key={k} className={`opx-var linked ${kind} ${changed.has(k) ? 'chg' : ''}`} data-wire={wire}>
      <i className="opx-var-dot" />
      <span className="opx-var-name" title={label}>
        {label}
      </span>
      <div className="opx-var-val">
        <span className="opx-linked" title={value}>
          {value || '…'}
        </span>
      </div>
      <span className="opx-var-tail">
        <span className="opx-link">
          {kind === 'ds' ? <Rows3 size={11} /> : <Database size={11} />} {chip}
        </span>
        {toggle}
      </span>
    </div>
  );
  /** Typed by hand over the data: its field, and the way back to the data. */
  const handRow = (k: string, label: string, input: React.ReactNode, toggle: React.ReactNode) => (
    <div key={k} className={`opx-var ${changed.has(k) ? 'chg' : ''}`}>
      <i className="opx-var-dot" />
      <span className="opx-var-name" title={label}>
        {label}
      </span>
      <div className="opx-var-val">{input}</div>
      <span className="opx-var-tail">
        <span className="opx-vt-src hand">
          <PenLine size={11} /> {t('Typed by hand')}
        </span>
        {toggle}
      </span>
    </div>
  );
  // A text with a style toolbar, several lines or a table takes the whole row of tiles.
  const wide = (f: Field) => richField(template, f.key) || f.kind === 'multiline' || f.kind === 'table';
  // Bento: a number, a colour or a switch takes one cell, a text, an image or a read value two.
  const size = (f: Field) => (wide(f) ? 'wide' : live(f) || f.kind === 'text' || f.kind === 'image' ? 'w2' : '');
  const groups = fieldGroups(fields, names);
  // The Texts fed by a data set come with the plain values.
  if (fed.length && !groups.some((g) => !g.title)) groups.unshift({ title: '', fields: [] });
  return (
    <>
      {groups.map((g) => (
        <Fragment key={g.title}>
          {g.title && <div className="opx-vt-group">{g.title}</div>}
          {g.fields.map(({ f, label }) =>
            f.kind !== 'table' && fromData(f) && !byHand(f.key) ? (
              linkedRow(f.key, label, boundDraft[f.key] ?? draft[f.key] ?? '', (f.binding!.path || bindingLabel(f.binding!)).split('.').pop()!.replace(/\[.*$/, ''), 'live', `src:${f.binding!.source}`, handToggle([f.key], (k) => boundDraft[k] ?? draft[k] ?? ''))
            ) : f.kind !== 'table' && fromData(f) ? (
              handRow(f.key, label, <FieldInput field={f} value={draft[f.key] ?? ''} onChange={(v) => onDraft({ [f.key]: v })} />, handToggle([f.key], (k) => draft[k] ?? ''))
            ) : f.kind !== 'table' && columnOf(f.key) && !byHand(f.key) ? (
              linkedRow(f.key, label, previewData[f.key] ?? '', f.key.slice(columnOf(f.key)!.key.length + 1), 'ds', f.key, handToggle([f.key], (k) => previewData[k] ?? ''))
            ) : f.kind !== 'table' && columnOf(f.key) ? (
              handRow(f.key, label, <FieldInput field={f} value={draft[f.key] ?? ''} onChange={(v) => onDraft({ [f.key]: v })} />, handToggle([f.key], (k) => previewData[k] ?? ''))
            ) : f.kind !== 'table' ? (
              // A variable as in the Designer: its dot, its name (set in the Designer), its value.
              <div key={f.key} className={`opx-var ${changed.has(f.key) ? 'chg' : ''}`}>
                <i className="opx-var-dot" />
                <span className="opx-var-name" title={label}>
                  {label}
                </span>
                <div className="opx-var-val">
                  <FieldInput field={f} value={draft[f.key] ?? ''} rich={richField(template, f.key) && /<\/?[bisuc]\b|<c=|<img=/.test(draft[f.key] ?? '')} onChange={(v) => onDraft({ [f.key]: v })} />
                </div>
              </div>
            ) : (
            <div key={f.key} className={`opx-vt ${size(f)} ${changed.has(f.key) ? 'chg' : ''}`}>
              <div className="opx-vt-h">
                {changed.has(f.key) && <i className="opx-chg" />}
                <span className="opx-vt-l">{label}</span>
                {fromData(f) && (
                  <span className={`opx-vt-src ${byHand(f.key) ? 'hand' : ''}`} title={t(readMode === 'live' ? '{binding} · {source}, read live' : '{binding} · {source}, read on take', { binding: bindingLabel(f.binding!), source: sourceName(f.binding!.source) })}>
                    {byHand(f.key) ? <PenLine size={11} /> : <Database size={11} />} {byHand(f.key) ? t('Typed by hand') : sourceName(f.binding!.source)}
                  </span>
                )}
                {fromData(f) && f.kind !== 'table' && handToggle([f.key], (k) => boundDraft[k] ?? draft[k] ?? '')}
              </div>
              {live(f) ? <span className="opx-bound">{boundDraft[f.key] ?? draft[f.key] ?? '…'}</span> : <FieldInput field={f} value={draft[f.key] ?? ''} onChange={(v) => onDraft({ [f.key]: v })} />}
            </div>
            ),
          )}
          {!g.title &&
            fed.map(({ layer, title, cols, at }) => {
              // Its columns as values read from the data (the ones not already a variable above).
              const own = cols.filter((k) => !fieldKeys.has(k));
              if (!cols.length)
                return (
                  <div key={layer.id} className="opx-vt wide">
                    <FedText label={dot(fedChanged, layer.name)} value={layer.type === 'text' ? shownText(layer, previewData) : layer.type === 'typewriter' ? interpolateText(layer.text, previewData) : ''} title={title} />
                  </div>
                );
              return own.map((k) => {
                const label = cols.length > 1 ? `${layer.name} · ${k.slice(at)}` : layer.name;
                const toggle = handToggle([k], (x) => previewData[x] ?? '');
                return byHand(k) ? handRow(k, label, <TextInput value={draft[k] ?? ''} onChange={(v) => onDraft({ [k]: v })} />, toggle) : linkedRow(k, label, previewData[k] ?? '', k.slice(at), 'ds', k, toggle);
              });
            })}
        </Fragment>
      ))}
      {/* Its exposed properties: rows like its values, without a box or its name again. */}
      {exposed.map((e) => (
        <LayerProperties key={e.layer.id} entry={e} io={io} flat />
      ))}
    </>
  );
}

/** The keys of the columns of each data set the layers read (their values and settings); null: all of them (a whole table). */
function readColumns(parts: WidgetPart[], sets: DataSet[]): Map<string, Set<string> | null> {
  const json = JSON.stringify(parts.map((p) => [p.layer, p.fields.map((f) => f.key)]));
  return new Map(
    sets.map((ds) => {
      const keys = new Set(json.match(new RegExp(`${ds.key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}_[A-Za-z0-9]\\w*`, 'g')) ?? []);
      return [ds.key, keys.size ? keys : null];
    }),
  );
}

/**
 * Cables from the values read from the data (`data-wire`) to their column
 * or source (`data-wire-to`), drawn over `root` (the values beside the data;
 * none once the data goes under them).
 */
function Wires({ root }: { root: React.RefObject<HTMLDivElement | null> }) {
  const [wires, setWires] = useState<{ d: string; kind: string; x1: number; y1: number; x2: number; y2: number }[]>([]);
  const [size, setSize] = useState({ w: 0, h: 0 });
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const draw = () => {
      const box = el.getBoundingClientRect();
      const out: typeof wires = [];
      el.querySelectorAll<HTMLElement>('[data-wire]').forEach((a) => {
        const b = el.querySelector<HTMLElement>(`[data-wire-to="${CSS.escape(a.dataset.wire ?? '')}"]`);
        if (!b) return;
        const ra = a.getBoundingClientRect();
        const rb = b.getBoundingClientRect();
        // It plugs into the card's edge, level with its column.
        const card = (b.closest('.ods') ?? b).getBoundingClientRect();
        if (!rb.width || card.left < ra.right) return;
        const x1 = ra.right - box.left + el.scrollLeft + 4;
        const y1 = ra.top + ra.height / 2 - box.top + el.scrollTop;
        const x2 = card.left - box.left + el.scrollLeft;
        const y2 = rb.top + rb.height / 2 - box.top + el.scrollTop;
        const dx = Math.max(16, (x2 - x1) / 2);
        out.push({ d: `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`, kind: a.classList.contains('live') ? 'live' : 'ds', x1, y1, x2, y2 });
      });
      setWires((w) => (JSON.stringify(w) === JSON.stringify(out) ? w : out));
      setSize((z) => (z.w === el.scrollWidth && z.h === el.scrollHeight ? z : { w: el.scrollWidth, h: el.scrollHeight }));
    };
    draw();
    const ro = new ResizeObserver(draw);
    ro.observe(el);
    // Rows load, pages turn, values change: drawn again now and then.
    const h = setInterval(draw, 400);
    return () => {
      ro.disconnect();
      clearInterval(h);
    };
  }, [root]);
  if (!wires.length) return null;
  return (
    <svg className="opx-wires" width={size.w} height={size.h} aria-hidden>
      {wires.map((w, i) => (
        <g key={i} className={w.kind}>
          <path d={w.d} />
          <circle cx={w.x1} cy={w.y1} r={3} />
          <circle cx={w.x2} cy={w.y2} r={3} />
        </g>
      ))}
    </svg>
  );
}

/** A widget's tab: its layer's name, numbered when several widgets have that name (Image, Image 2…). */
function tabName(widgets: WidgetPart[], i: number) {
  const name = widgets[i].layer.name;
  const same = widgets.filter((w) => w.layer.name === name);
  const n = same.indexOf(widgets[i]);
  return same.length > 1 && n > 0 ? `${name} ${n + 1}` : name;
}

/**
 * The preview of the selected graphic, held at its On air point; its widgets run once the preview's
 * controls were used. A widget moving by itself waits for its Play, still at its start (heldAt).
 */
const PreviewScene = memo(function PreviewScene({ template: base, format, data, fps, run, lang }: { template: Template; format: string; data: Record<string, string>; fps: number; run: PreviewRun; lang: string }) {
  const template = useMemo(() => inFormat(base, format), [base, format]);
  const hold = holdFrame(template);
  const holdSecs = hold / fps;
  // Kept while the preview's run and values stay: only the widgets that move redraw on each frame (while it runs).
  const layerAge = useCallback(
    (l: Layer, age: number) => heldAt(run, l, data, holdSecs) ?? (run.active ? layerAgeFrames({ clocks: run.clocks } as PlayoutItem, age * fps, l) / fps : age),
    [run, data, holdSecs, fps],
  );
  const clocked = useCallback((l: Layer) => heldAt(run, l, data, holdSecs) !== undefined || (run.active && (!!run.clocks[l.id] || !!l.manualStart)), [run, data, holdSecs]);
  const widgetState = useMemo(() => (run.active ? (id: string) => run.widgets[id] : undefined), [run]);
  return (
    <LiveTime ageSecs={holdSecs} since={run.active ? run.since : undefined} ticking={run.active ? 'all' : false} layerAge={layerAge} clocked={clocked} widgetState={widgetState} fps={fps} lang={lang}>
      <Scene template={template} frame={hold} data={data} />
    </LiveTime>
  );
});

/** In, hold and out of a template, and where the graphic on air is in it. */
function MiniTimeline({ template, air }: { template?: Template; air?: PlayoutItem }) {
  if (!template) return null;
  const d = Math.max(1, template.duration);
  const hold = Math.min(d, holdFrame(template));
  const pauses = template.markers.filter((m) => m.kind === 'pause');
  return (
    <span className={`opx-mtl ${air ? 'air' : ''}`} title={air ? t(PHASE_TITLES[air.phase]) : t('In · on air · out')}>
      <span className="in" style={{ width: `${(hold / d) * 100}%` }} />
      <span className="out" style={{ left: `${(hold / d) * 100}%` }} />
      {pauses.map((m) => (
        <span key={m.id} className="pause" style={{ left: `${(m.frame / d) * 100}%` }} />
      ))}
      {air && <span className="ph" style={{ left: `${(Math.min(air.frame, d) / d) * 100}%` }} />}
    </span>
  );
}

function GraphicRow(props: {
  project: Project;
  item: RundownItem;
  zone?: Zone;
  air?: PlayoutItem;
  /** When the channel state was received (client ms). */
  liveAt: number;
  selected: boolean;
  /** NEXT takes this one. */
  cue: boolean;
  operate: boolean;
  /** Its zone is held by another operator. */
  lockedBy?: ZoneLock;
  /** « Take over » its zone (shown on hover while someone else holds it). */
  takeOver?: React.ReactNode;
  /** Its next scheduled take, and the editor to schedule one (opened from its menu). */
  schedule: (editAt: DOMRect | null, onClose: () => void) => React.ReactNode;
  /** A take can be scheduled (the project plays on an output, the zone is free). */
  canSchedule: boolean;
  editable: boolean;
  onSelect: () => void;
  onTake: () => void;
  onOut: () => void;
  onContinue: () => void;
  onRename: (name: string) => void;
  /** The playlist goes on by itself: show and set this graphic's time. */
  autoNext: boolean;
  onAuto: (patch: Partial<RundownItem>) => void;
  onDuplicate: () => void;
  onDelete: () => void;
  drag: { over: 'before' | 'after' | null; onStart: (e: React.DragEvent) => void; onOver: (e: React.DragEvent) => void; onDrop: (e: React.DragEvent) => void; onEnd: () => void };
}) {
  const { project, item, zone, air } = props;
  const tpl = project.templates.find((x) => x.id === item.templateId);
  const pause = air ? pauseOf(air, project) : undefined;
  const step = air ? autoStep(project, air, props.liveAt) : null;
  const left = useLeft(step);
  const [renaming, setRenaming] = useState(false);
  const [autoAt, setAutoAt] = useState<DOMRect | null>(null);
  const [schedAt, setSchedAt] = useState<DOMRect | null>(null);
  // The row's menu (« … » or a right-click): where it opens.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  return (
    <div
      className={`opx-gr ${props.selected ? 'on' : ''} ${props.cue ? 'cue' : ''} ${air ? 'air' : ''} ${props.lockedBy ? 'opl-locked' : ''} ${props.drag.over ? `drop-${props.drag.over}` : ''}`}
      onClick={props.onSelect}
      onContextMenu={(e) => (e.preventDefault(), props.onSelect(), setMenu({ x: e.clientX, y: e.clientY }))}
      onDragOver={props.drag.onOver}
      onDrop={props.drag.onDrop}
    >
      {props.editable && (
        <span className="opx-grip" draggable onDragStart={props.drag.onStart} onDragEnd={props.drag.onEnd} title={t('Drag to reorder')} onClick={(e) => e.stopPropagation()}>
          <GripVertical size={14} />
        </span>
      )}
      {props.lockedBy ? (
        <button className={`opx-sq big opl-sq ${air ? 'red' : ''}`} disabled title={t('{name} holds this zone', { name: props.lockedBy.userName })} onClick={(e) => e.stopPropagation()}>
          {air ? <Square size={12} fill="currentColor" /> : <Play size={14} fill="currentColor" />}
          <span className="opl-sq-av">
            <Avatar id={props.lockedBy.userId} name={props.lockedBy.userName} small />
          </span>
        </button>
      ) : air && pause ? (
        <PauseButton pause={pause} step={step} left={left} operate={props.operate} onContinue={props.onContinue} />
      ) : (
        <button
          className={`opx-sq big ${air ? 'red' : props.cue ? 'cue' : ''} ${step ? 'opx-pause wait-out' : ''}`}
          style={step ? ({ '--p': step.secs ? 1 - left / step.secs : 1 } as React.CSSProperties) : undefined}
          disabled={!props.operate}
          title={air ? (step ? `${(step.kind === 'next' ? t('Next graphic in {n} s', { n: Math.ceil(left) }) : t('Goes out in {n} s', { n: Math.ceil(left) }))} · ${t('Take out')}` : t('Take out')) : props.cue ? `${t('Take')} · ${t('NEXT takes this graphic')}` : t('Take')}
          onClick={(e) => {
            e.stopPropagation();
            if (air) props.onOut();
            else props.onTake();
          }}
        >
          {step ? <b>{Math.ceil(left)}</b> : air ? <Square size={12} fill="currentColor" /> : props.cue ? <span className="opx-sq-cue">CUE</span> : <Play size={14} fill="currentColor" />}
        </button>
      )}
      <span className="opx-gr-main">
        {renaming ? (
          <input
            className="opx-rename"
            autoFocus
            defaultValue={item.name}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === 'Enter') e.currentTarget.blur();
              if (e.key === 'Escape') setRenaming(false);
            }}
            onBlur={(e) => {
              const v = e.target.value.trim();
              if (v && v !== item.name) props.onRename(v);
              setRenaming(false);
            }}
          />
        ) : (
          <strong title={props.editable ? t('Double-click to rename') : undefined} onDoubleClick={(e) => (e.stopPropagation(), props.editable && setRenaming(true))}>
            {item.name}
          </strong>
        )}
        <span className="muted small opx-gr-line">
          <span className="opx-zpill" style={{ ['--zc' as string]: zone?.color ?? '#6b7090' }}>
            {zone?.name ?? t('no zone')}
          </span>
          <span className="opx-gr-tpl">{tpl?.name ?? t('missing template')}</span>
          {props.autoNext && item.autoSecs !== undefined && <span className="opx-auto"> · {t('next {n} s', { n: item.autoSecs })}</span>}
        </span>
        <MiniTimeline template={tpl} air={air} />
      </span>
      <DataWarning template={tpl} />
      {props.takeOver && <span className="opl-row-take">{props.takeOver}</span>}
      {props.schedule(schedAt, () => setSchedAt(null))}
      <span className="opx-acts" onClick={(e) => e.stopPropagation()}>
        {props.editable && (
          <>
            <button title={t('Duplicate')} onClick={props.onDuplicate}>
              <Copy size={13} />
            </button>
            <button className="red" title={t('Remove')} onClick={props.onDelete}>
              <Trash2 size={13} />
            </button>
          </>
        )}
        <button className={menu ? 'on' : ''} title={t('More')} onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setMenu(menu ? null : { x: r.left, y: r.bottom + 4 });
        }}>
          <MoreHorizontal size={14} />
        </button>
      </span>
      {menu && (
        <RowMenu
          at={menu}
          onClose={() => setMenu(null)}
          items={[
            { icon: <Clock size={13} />, label: t('Schedule a take…'), disabled: !props.canSchedule, run: () => setSchedAt(new DOMRect(menu.x, menu.y, 0, 0)) },
            { icon: <Timer size={13} />, label: t('Auto next…'), disabled: !props.editable, run: () => setAutoAt(new DOMRect(menu.x, menu.y, 0, 0)), on: item.autoSecs !== undefined },
            null,
            { icon: <Pencil size={13} />, label: t('Rename…'), disabled: !props.editable, run: () => setRenaming(true) },
            { icon: <Copy size={13} />, label: t('Duplicate'), disabled: !props.editable, run: props.onDuplicate },
            null,
            { icon: <Trash2 size={13} />, label: t('Remove…'), disabled: !props.editable, run: props.onDelete, danger: true },
          ]}
        />
      )}
      {autoAt && <AutoNextEditor item={item} at={autoAt} onChange={props.onAuto} onClose={() => setAutoAt(null)} />}
    </div>
  );
}

interface RowMenuItem {
  icon: React.ReactNode;
  label: string;
  run: () => unknown;
  disabled?: boolean;
  danger?: boolean;
  /** Set on this graphic (auto next). */
  on?: boolean;
}

/** A rundown row's menu (« … » or right-click), kept in the window; Esc or a click elsewhere closes it. */
function RowMenu({ at, items, onClose }: { at: { x: number; y: number }; items: (RowMenuItem | null)[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (e.preventDefault(), onClose());
    const away = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onClose();
    window.addEventListener('keydown', esc);
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('resize', onClose);
    return () => {
      window.removeEventListener('keydown', esc);
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('resize', onClose);
    };
  }, [onClose]);
  return createPortal(
    <div ref={ref} className="menu project-menu clip-menu opx-row-menu" style={{ position: 'fixed', left: Math.max(8, Math.min(at.x, window.innerWidth - 240)), top: Math.max(8, Math.min(at.y, window.innerHeight - 240)), right: 'auto' }} onClick={(e) => e.stopPropagation()} onContextMenu={(e) => e.preventDefault()}>
      {items.map((it, i) =>
        it ? (
          <button key={i} className={it.danger ? 'danger' : ''} disabled={it.disabled} onClick={() => (onClose(), void it.run())}>
            {it.icon} {it.label}
            {it.on && <i className="opx-row-menu-on" />}
          </button>
        ) : (
          <div key={i} className="menu-sep" />
        ),
      )}
    </div>,
    document.body,
  );
}

/** A graphic held on a pause: Continue when the operator resumes it, else what it waits for (seconds left, a trigger). */
function PauseButton({ pause, step, left, operate, onContinue }: { pause: Marker; step: AutoStep | null; left: number; operate: boolean; onContinue: () => void }) {
  const resume = pause.resume ?? { type: 'manual' };
  if (step) {
    return (
      <span className="opx-sq big amber opx-pause wait" style={{ '--p': step.secs ? 1 - left / step.secs : 1 } as React.CSSProperties} title={t('Pause · plays on in {n} s', { n: Math.ceil(left) })} onClick={(e) => e.stopPropagation()}>
        <b>{Math.ceil(left)}</b>
      </span>
    );
  }
  if (resume.type === 'trigger' || resume.type === 'triggers')
    return (
      <span className="opx-sq big amber opx-pause wait" title={resume.type === 'trigger' ? t('Pause · waits for the trigger {name}', { name: resume.name }) : t('Pause · waits for its Triggers')} onClick={(e) => e.stopPropagation()}>
        <Zap size={14} fill="currentColor" />
      </span>
    );
  return (
    <button
      className="opx-sq big amber opx-pause"
      disabled={!operate}
      title={`${t('Pause · Continue')} · F2`}
      onClick={(e) => {
        e.stopPropagation();
        onContinue();
      }}
    >
      <ChevronsRight size={18} />
    </button>
  );
}

/** Auto next settings of one graphic: its time on air before the next one, or the operator takes it. */
function AutoNextEditor({ item, at, onChange, onClose }: { item: RundownItem; at: DOMRect; onChange: (patch: Partial<RundownItem>) => void; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const down = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onClose();
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('pointerdown', down);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', down);
      window.removeEventListener('keydown', key, true);
    };
  }, [onClose]);
  const auto = item.autoSecs !== undefined;
  return createPortal(
    <div ref={ref} className="menu marker-editor" style={{ position: 'fixed', left: Math.max(8, at.left - 240), top: at.bottom + 6 }} onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
      <header>
        <Timer size={13} />
        <strong>{item.name}</strong>
        <div className="spacer" />
        <button className="icon-btn ghost" title={t('Close')} onClick={onClose}>
          <X size={14} />
        </button>
      </header>
      <div className="me-field">
        <span>{t('Next')}</span>
        <div className="seg">
          <button className={!auto ? 'on' : ''} title={t('The operator takes the next graphic')} onClick={() => onChange({ autoSecs: undefined, hold: undefined })}>
            {t('Operator')}
          </button>
          <button className={auto ? 'on' : ''} title={t('The next graphic is taken after this time on air')} onClick={() => onChange({ autoSecs: item.autoSecs ?? AUTO_SECS, hold: undefined })}>
            {t('After')}
          </button>
        </div>
      </div>
      {auto && (
        <label className="me-field">
          <span>{t('On air for')}</span>
          <NumberField value={item.autoSecs ?? AUTO_SECS} min={0} step={1} suffix="s" onChange={(v) => onChange({ autoSecs: Math.max(0, v), hold: undefined })} />
        </label>
      )}
    </div>,
    document.body,
  );
}

/** Sources the graphic reads, when it reads them, and Read now. */
function DataBlock({ template, fields, item, draft, canEdit, onRead, onMode, children, show = 'all' }: { children?: React.ReactNode; project: Project; template: Template; /** The fields of the widget read from a source. */ fields: Field[]; item: RundownItem; draft: Record<string, string>; canEdit: boolean; onRead: (patch: Record<string, string>) => void; onMode: (m: ReadMode) => void; /** The sources only (in front of their variables), or how they are read only. */ show?: 'all' | 'cards' | 'controls' }) {
  const all = useSources();
  const auth = useApp((s) => s.auth);
  const toast = useApp((s) => s.toast);
  const bound = template.fields.filter((f) => f.binding?.source);
  const used = [...new Set(fields.filter((f) => f.binding?.source).map((f) => f.binding!.source))].map((id) => all.find((s) => s.id === id) ?? { id, name: id, interval: 0, updatedAt: null, error: t('not found') });
  const mode: ReadMode = item.read ?? 'live';
  const [reading, setReading] = useState(false);
  const readNow = async () => {
    setReading(true);
    const patch: Record<string, string> = {};
    await Promise.all(
      bound.map(async (f) => {
        const v = await readBinding(f.binding!, draft);
        if (v !== null) patch[f.key] = v;
      }),
    );
    setReading(false);
    onRead(patch);
    toast(tn(Object.keys(patch).length, '{n} value read', '{n} values read'), 'info');
  };
  const ago = (at?: number | null) => {
    if (!at) return t('never read');
    const s = Math.max(0, Math.round((Date.now() - at) / 1000));
    return s < 60 ? t('read {n} s ago', { n: s }) : t('read {n} min ago', { n: Math.round(s / 60) });
  };
  return (
    <div className="opx-data">
      {children}
      {used.length > 0 && (
        <>
          {show !== 'controls' &&
            used.map((s) => (
              <div key={s.id} className="opx-source" data-wire-to={`src:${s.id}`}>
                <span className={`live-dot ${s.error ? 'err' : ''}`} />
                <strong>{s.name}</strong>
                <span className="muted small">{s.error ? s.error : ago(s.updatedAt)}</span>
              </div>
            ))}
          {show !== 'cards' && (
          <>
          <div>
            <span className="opx-label">{t('Read the data')}</span>
            <div className="opx-seg">
              {READ_MODES.map((m) => (
                <button key={m.id} className={mode === m.id ? 'on' : ''} disabled={!canEdit} title={t(m.hint)} onClick={() => onMode(m.id)}>
                  {t(m.label)}
                </button>
              ))}
            </div>
          </div>
          {mode === 'live' && (
            <div className="opx-every">
              {used.map((s) => (
                <label key={s.id}>
                  <span className="muted small">{t('{name} checked every', { name: s.name })}</span>
                  <select
                    value={s.interval}
                    disabled={!can.design(auth?.user) || !('type' in s)}
                    title={can.design(auth?.user) ? t('How often the server reads the source (for every graphic using it)') : t('Set by a designer in Data')}
                    onChange={async (e) => {
                      const { updatedAt, checkedAt, error, bytes, ...src } = s as typeof s & { checkedAt?: number; bytes?: number };
                      (void updatedAt, checkedAt, error, bytes);
                      await api('PUT', `/api/data/sources/${encodeURIComponent(s.id)}`, { ...src, interval: +e.target.value }).catch((err) => toast((err as Error).message, 'error'));
                    }}
                  >
                    {[...new Set([...INTERVALS, s.interval])]
                      .filter((v) => v > 0)
                      .sort((a, b) => a - b)
                      .map((v) => (
                        <option key={v} value={v}>
                          {v < 60 ? `${v} s` : `${v / 60} min`}
                        </option>
                      ))}
                  </select>
                </label>
              ))}
            </div>
          )}
          {mode !== 'live' && (
            <button className="opx-btn" disabled={reading || !canEdit} onClick={readNow} title={mode === 'manual' ? t('Read the sources now and keep the values in this item') : t('Read now to check; the take reads again')}>
              <RefreshCw size={13} /> {reading ? t('Reading…') : t('Read now')}
            </button>
          )}
          </>
          )}
        </>
      )}
    </div>
  );
}

/** Which widget row of the graphics on air is selected (its keys are on) and which ones are unfolded (⋯). */
interface AirWidgets {
  selected: string | null;
  onSelect: (key: string) => void;
  more: Record<string, boolean>;
  onMore: (key: string, open: boolean) => void;
}

/**
 * The control centre of a widget of the graphic on air (web/src/widgets/centre.tsx): its state,
 * its actions, its exposed settings (written into the take). A click on it selects it: only then
 * its keyboard shortcuts work.
 */
function SpecControls({ layer, item, act, setData, fps, lang, selected, onSelect }: { layer: Layer; item: PlayoutItem; act: (op: string, args?: unknown) => void; setData: (patch: Record<string, string>) => void; fps: number; lang: string; selected: boolean; onSelect: () => void }) {
  const def = widgetOf(layer)!;
  const offset = useApp((s) => s.serverOffset);
  const tz = useApp((s) => s.timezone);
  const state = item.widgets?.[layer.id];
  const Op = def.Operator;
  if (!Op) return null;
  // The layer's own time on air: still while it waits for its start, or paused.
  const time = { nowMs: Date.now() + offset, ageSecs: layerAgeFrames(item, item.age ?? 0, layer) / fps, defaultTz: tz, fps, lang };
  return (
    <KeyScope.Provider value={selected}>
      <div className={`opx-widget cc-host ${selected ? 'sel' : ''}`} onClick={onSelect}>
        <CentreModeContext.Provider value="air">
          <OperatorBound layer={layer} data={item.data} setData={setData}>
            <Op layer={layer as never} state={state} data={item.data} act={act} item={item} time={time} lang={lang} setData={setData} />
          </OperatorBound>
        </CentreModeContext.Provider>
      </div>
    </KeyScope.Provider>
  );
}

/** The templates the operator can add to the rundown: searchable cards with their picture, by zone. */
function AddGraphic({ project, templates, used, onPick, onClose }: { project: Project; templates: Template[]; used: (id: string) => number; onPick: (t: Template) => void; onClose: () => void }) {
  const [q, setQ] = useState('');
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const zoneOf = (t: Template) => project.zones.find((z) => z.id === t.zone);
  const list = templates.filter((t) => words.every((w) => `${t.name} ${zoneOf(t)?.name ?? ''}`.toLowerCase().includes(w)));
  // By zone, in the zones' order.
  const groups = [...project.zones.map((z) => ({ zone: z as Zone | undefined, list: list.filter((t) => t.zone === z.id) })), { zone: undefined, list: list.filter((t) => !zoneOf(t)) }].filter((g) => g.list.length);
  const fps = project.frameRate.num / project.frameRate.den || 25;
  const secs = (f: number) => `${+(f / fps).toFixed(1)} s`;
  return createPortal(
    <div className="modal-back" onPointerDown={onClose}>
      <div className="modal opx-addmodal" onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === 'Escape' && onClose()}>
        <header>
          <h2>{t('Add a graphic from a template')}</h2>
          <button className="icon-btn ghost" title={t('Close (Esc)')} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="opx-addsearch">
          <input
            autoFocus
            placeholder={templates.length > 1 ? t('Search {n} graphics…', { n: templates.length }) : t('Search a graphic…')}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && list[0]) onPick(list[0]);
            }}
          />
        </div>
        <div className="opx-addbody">
          {groups.map((g) => (
            <section key={g.zone?.id ?? '-'}>
              <h4>
                <span className="opx-dot" style={{ background: g.zone?.color ?? '#6b7090' }} /> {g.zone?.name ?? t('No zone')}
                <em>{g.list.length}</em>
              </h4>
              <div className="opx-addgrid">
                {g.list.map((tpl) => {
                  const n = used(tpl.id);
                  return (
                    <button key={tpl.id} className="opx-addcard" title={t('Add “{name}” to the rundown', { name: tpl.name })} onClick={() => onPick(tpl)}>
                      <span className="opx-addpic" style={{ aspectRatio: `${project.width} / ${project.height}` }}>
                        <img src={thumbnailUrl(project.id, tpl)} alt="" loading="lazy" draggable={false} />
                        {n > 0 && <i title={t('Already {n}× in this playlist', { n })}>×{n}</i>}
                      </span>
                      <strong>{tpl.name}</strong>
                      <span className="muted small">
                        {secs(tpl.duration)}
                        {tpl.fields.length ? ` · ${tn(tpl.fields.length, '{n} field', '{n} fields')}` : ''}
                        {usedDatasets(tpl).length ? ` · ${t('data set')}` : ''}
                      </span>
                    </button>
                  );
                })}
              </div>
            </section>
          ))}
          {!groups.length && <p className="opx-hint pad">{templates.length ? t('No graphic matches.') : t('No graphic published yet: publish templates from the Designer.')}</p>}
        </div>
      </div>
    </div>,
    document.body,
  );
}

/** How Update changes the values on air: a cross-fade or a cut (kept in this browser). */
function UpdateMode({ value, onChange, disabled }: { value: boolean; onChange: (fade: boolean) => void; disabled: boolean }) {
  return (
    <div className="opx-seg opx-upd-mode" title={t('How the values change on air')}>
      <button className={value ? 'on' : ''} disabled={disabled} title={t('The new values cross-fade in (½ s)')} onClick={() => onChange(true)}>
        {t('Fade')}
      </button>
      {/* CUT keeps its broadcast name. */}
      <button className={value ? '' : 'on'} disabled={disabled} title={t('The new values replace the old ones at once')} onClick={() => onChange(false)}>
        Cut
      </button>
    </div>
  );
}

function useUpdateFade(): [boolean, (v: boolean) => void] {
  const [v, setV] = useState(() => {
    try {
      return localStorage.getItem('pulse.operator.updateFade') === '1';
    } catch {
      return false;
    }
  });
  const set = (x: boolean) => {
    setV(x);
    try {
      localStorage.setItem('pulse.operator.updateFade', x ? '1' : '0');
    } catch {
      /* private mode */
    }
  };
  return [v, set];
}

/** One graphic on air: a card — the on-air family in its header, the animation pills below. */
/** How far an automatic step went (%), for the tile filling up to it. */
const fillOf = (step: AutoStep, left: number) => (step.secs ? Math.min(100, Math.max(0, (1 - left / step.secs) * 100)) : 100);

const secsText = (s: number) => (s < 10 ? s.toLocaleString(lang(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }) : String(Math.ceil(s)));
const clockText = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;

/**
 * Out, on air: what counts in its corner (what is left of the in, the time on air, an automatism's
 * countdown, what is left of the out), and the tile filling up to an automatism (out, next graphic).
 */
/** A tile sweeping from the left up to an automatic step, as NEXT does with an auto next. */
function Sweep({ step }: { step: AutoStep }) {
  const now = useTicker(true, MONITOR_FPS);
  const left = Math.max(0, (step.at - now) / 1000);
  return <i className="opx-next-fill" style={{ width: `${fillOf(step, left)}%` }} />;
}

function OutTile({ project, item, template, liveAt, step, disabled, onOut }: { project: Project; item: PlayoutItem; template?: Template; liveAt: number; step: AutoStep | null; disabled: boolean; onOut: () => void }) {
  const now = useTicker(true, MONITOR_FPS);
  const left = step ? Math.max(0, (step.at - now) / 1000) : 0;
  const fps = project.frameRate.num / project.frameRate.den || 50;
  const since = Math.max(0, (now - liveAt) / 1000);
  // Where it is now: the frame moves on between two channel states while it plays.
  const moving = item.phase === 'playing' || item.phase === 'out';
  const frame = moving ? Math.min(Math.max(item.frame, item.target), item.frame + since * fps * Math.sign(item.target - item.frame || 1)) : item.frame;
  const end = template ? template.duration : item.target;
  let time: React.ReactNode;
  let title: string | undefined;
  // The sweep: up to the automatism, through the in, through the out.
  let sweep: number | null = null;
  const auto = step && (step.kind === 'out' || step.kind === 'next') ? step : null;
  if (auto) {
    time = (
      <b className="opx-tile-time auto">
        {auto.kind === 'next' && <SkipForward size={11} />}
        {Math.ceil(left)} s
      </b>
    );
    title = auto.kind === 'next' ? t('Next graphic in {n} s', { n: Math.ceil(left) }) : t('Goes out in {n} s', { n: Math.ceil(left) });
    sweep = fillOf(auto, left);
  } else if (item.phase === 'playing' && template && frame < holdFrame(template)) {
    const to = Math.min(item.target, holdFrame(template));
    time = <b className="opx-tile-time">IN {secsText(Math.max(0, (to - frame) / fps))} s</b>;
    sweep = to > 0 ? Math.min(100, (frame / to) * 100) : 100;
  } else if (item.phase === 'out') {
    const from = template ? holdFrame(template) : 0;
    time = <b className="opx-tile-time">OUT {secsText(Math.max(0, (end - frame) / fps))} s</b>;
    sweep = end > from ? Math.min(100, Math.max(0, ((frame - from) / (end - from)) * 100)) : 100;
  } else {
    time = <b className="opx-tile-time">{clockText((item.age ?? 0) / fps + since)}</b>;
    title = t('On air for {time}', { time: clockText((item.age ?? 0) / fps + since) });
  }
  return (
    <button className="opx-tile out" disabled={disabled} onClick={onOut} title={title}>
      {sweep !== null && <i className="opx-next-fill" style={{ width: `${sweep}%` }} />}
      <span className="opx-tile-top">
        <CircleStop size={18} />
        {time}
      </span>
      <span>Out</span>
    </button>
  );
}

/** On air, bento: the cells a control takes (Continue, Out: two, for their label and time; a widget: two to four, by its buttons; anything else: the whole row, -1). */
function cellsOf(el: Element): number {
  if (el.classList.contains('opx-tile')) return 2;
  if (el.classList.contains('opx-widget') || el.classList.contains('opx-trig')) {
    const n = el.querySelectorAll('button').length;
    return n <= 2 ? 2 : n <= 4 ? 3 : 4;
  }
  return -1;
}

const AIR_CELL = 88;
const AIR_GAP = 8;

/**
 * Lays the on-air cards out as a bento: each as wide as its controls need (two cells at least),
 * never wider than the column, the holes filled (grid-auto-flow: dense).
 */
function useAirBento(ref: React.RefObject<HTMLDivElement | null>, shown: boolean) {
  useLayoutEffect(() => {
    const list = ref.current;
    if (!list) return;
    const lay = () => {
      const cs = getComputedStyle(list);
      const inner = list.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
      const cols = Math.max(2, Math.floor((inner + AIR_GAP) / (AIR_CELL + AIR_GAP)));
      list.style.setProperty('--cols', String(cols));
      for (const card of list.querySelectorAll<HTMLElement>(':scope > .opx-aircard')) {
        const kids = [...(card.querySelector('.opx-controls')?.children ?? [])] as HTMLElement[];
        const cells = kids.map(cellsOf);
        const used = cells.reduce((a, c) => a + Math.max(0, c), 0);
        const span = cells.includes(-1) ? cols : Math.min(cols, Math.max(2, used));
        card.style.gridColumn = `span ${span}`;
        card.style.setProperty('--s', String(span));
        // The last control takes what is left of its row (a lone Out fills its card).
        kids.forEach((k, i) => {
          const c = cells[i];
          const want = c < 0 ? span : Math.min(span, c + (i === kids.length - 1 && !cells.includes(-1) ? Math.max(0, span - used) : 0));
          k.style.gridColumn = `span ${want}`;
        });
      }
    };
    lay();
    const ro = new ResizeObserver(lay);
    ro.observe(list);
    const mo = new MutationObserver(lay);
    mo.observe(list, { childList: true, subtree: true });
    return () => {
      ro.disconnect();
      mo.disconnect();
    };
  }, [ref, shown]);
}

function AirCard({ project, zone, item, liveAt, operate, lockedBy, takeOver, command, nextRowOf, widget }: { project: Project; zone: Zone; item: PlayoutItem; liveAt: number; operate: boolean; /** Its zone is held by another operator. */ lockedBy?: ZoneLock; /** « Take over », over its controls while someone else holds it. */ takeOver?: React.ReactNode; command: (c: Command) => void; nextRowOf?: (dataset: string) => number | undefined; widget: AirWidgets }) {
  const tpl = project.templates.find((x) => x.id === item.templateId);
  const waiting = tpl ? pauseOf(item, project) : undefined;
  const step = autoStep(project, item, liveAt);
  const left = useLeft(step);
  const name = rundownName(project, item.rundownId) ?? tpl?.name ?? t('Deleted graphic');
  return (
    <div className={`opx-aircard ${lockedBy ? 'opl-locked' : ''}`} title={lockedBy ? t('{name} holds this zone', { name: lockedBy.userName }) : undefined}>
      <header>
        <strong>{name}</strong>
        {lockedBy && <Avatar id={lockedBy.userId} name={lockedBy.userName} small />}
        <span className="spacer" />
        <DataWarning template={tpl} />
      </header>
      <AirControls
        project={project}
        zone={zone}
        item={item}
        command={command}
        nextRowOf={nextRowOf}
        widget={widget}
        bare
        lead={
          <>
            {canContinue(item, tpl) && (
              <button className={`opx-tile cont ${waiting ? 'waits' : ''}`} disabled={!operate} onClick={() => command({ action: 'continue', zone: zone.id })} title={step?.kind === 'resume' ? t('Pause · plays on in {n} s', { n: Math.ceil(left) }) : undefined}>
                {step?.kind === 'resume' && <Sweep step={step} />}
                <span className="opx-tile-top">
                  <ChevronsRight size={18} />
                  {step?.kind === 'resume' && <b className="opx-tile-time">{Math.ceil(left)} s</b>}
                </span>
                <span>Continue</span>
              </button>
            )}
            <OutTile project={project} item={item} template={tpl} liveAt={liveAt} step={step} disabled={!operate} onOut={() => command({ action: 'takeOut', zone: zone.id })} />
          </>
        }
      />
      {lockedBy && takeOver && (
        <div className="opl-veil">
          <span className="opl-veil-who">
            <Avatar id={lockedBy.userId} name={lockedBy.userName} small />
            {lockedBy.userName}
          </span>
          {takeOver}
        </div>
      )}
    </div>
  );
}

function AirControls({ project, zone, item, command, nextRowOf, widget, bare, lead }: { project: Project; zone: Zone; item: PlayoutItem; command: (c: Command) => void; nextRowOf?: (dataset: string) => number | undefined; widget: AirWidgets; bare?: boolean; /** Tiles before the controls: Continue, Out. */ lead?: React.ReactNode }) {
  const tpl = project.templates.find((x) => x.id === item.templateId);
  const target = { zone: zone.id };
  // Deleted (or unpublished) while on air: it can still be taken out.
  if (!tpl)
    return (
      <div className="opx-controls">
        {lead}
        {!bare && (
          <div className="opx-actions sticky">
            <button className="opx-btn" onClick={() => command({ action: 'takeOut', ...target })} title="F3">
              <CircleStop size={14} /> Out
            </button>
          </div>
        )}
        <p className="opx-hint">{t('This graphic is no longer in the project: take it out.')}</p>
      </div>
    );
  // A Text scrolls by itself: no start / pause for it in the Operator.
  const timed = tpl.layers.filter((l) => isTimed(l) && l.type !== 'text');
  // Pauses waiting for a trigger, and values set by hand until one.
  // …and the widgets' triggers (a start, the next page): every setting named `…Trigger` of a layer.
  const widgetTriggers = tpl.layers.flatMap((l) => Object.entries(l as unknown as Record<string, unknown>).flatMap(([k, v]) => (/Trigger$/.test(k) && typeof v === 'string' && v.trim() ? [v.trim()] : [])));
  const triggers = [...new Set([...tpl.markers.flatMap((m) => (m.kind === 'pause' && m.resume?.type === 'trigger' ? [m.resume.name] : [])), ...Object.entries(item.data).flatMap(([k, v]) => (k.endsWith('__manual') && v.startsWith('trigger:') ? [v.slice(8)] : [])), ...widgetTriggers])];
  const waiting = pauseOf(item, project);
  return (
    <div className="opx-controls">
      {lead}
      {!bare && (
        <div className="opx-actions sticky">
          {canContinue(item, tpl) && (
            <button className={`opx-btn ${waiting ? 'amber' : ''}`} onClick={() => command({ action: 'continue', ...target })} title={t('F2 / Space')}>
              <ChevronsRight size={14} /> Continue
            </button>
          )}
          <button className="opx-btn" onClick={() => command({ action: 'takeOut', ...target })} title="F3">
            <CircleStop size={14} /> Out
          </button>
        </div>
      )}
      {!!steppedDatasets(tpl).length && <AirRows sets={steppedDatasets(tpl)} item={item} command={command} zone={zone.id} nextRowOf={nextRowOf} />}
      {tpl.layers
        .filter((l) => l.visible && !!widgetOf(l)?.Operator && hasControls(l, item.data))
        .map((l) => {
          const key = `${zone.id}:${l.id}`;
          return (
            <SpecControls
              key={l.id}
              layer={l}
              item={item}
              fps={project.frameRate.num / project.frameRate.den || 50}
              lang={project.language || 'fr'}
              act={(op, args) => command({ action: 'act', target: l.id, op, args, ...target })}
              setData={(patch) => command({ action: 'update', data: patch, fade: false, ...target })}
              selected={widget.selected === key}
              onSelect={() => widget.onSelect(key)}
            />
          );
        })}
      {timed
        .filter((l) => !widgetOf(l)?.Operator)
        .map((l) => {
          const c = item.clocks?.[l.id];
          const running = c ? c.pausedAt == null : !l.manualStart;
          const toggle = () => command({ action: 'widget', target: l.id, control: 'toggle', ...target });
          return (
            <div key={l.id} className="opx-widget cc-host">
              <CentreModeContext.Provider value="air">
                <Centre
                  name={l.name}
                  status={<StartStatus layer={l} item={item} />}
                  actions={[
                    running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: toggle } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: toggle },
                    { icon: RotateCcw, label: tc('cc', 'Restart'), key: 'T', onClick: () => command({ action: 'widget', target: l.id, control: running ? 'restart' : 'reset', ...target }) },
                  ]}
                />
              </CentreModeContext.Provider>
            </div>
          );
        })}
      {triggers.map((n) => (
        <button key={n} className={`opx-trig ${waiting?.resume?.type === 'trigger' && waiting.resume.name === n ? 'waits' : ''}`} onClick={() => command({ action: 'trigger', name: n })} title={t('Every graphic waiting for this trigger plays on')}>
          <span className="opx-trig-k">{t('Trigger')}</span>
          <span className="opx-trig-n">
            <Zap size={14} /> {n}
          </span>
        </button>
      ))}
    </div>
  );
}

interface Monitors {
  preview: boolean;
  air: boolean;
}

/** Which monitors show their picture: the preview, the on-air one (kept per browser; their headers always show). */
function useMonitors(): [Monitors, (patch: Partial<Monitors>) => void] {
  const [v, setV] = useState<Monitors>(() => {
    try {
      const s = localStorage.getItem('strata.opScreens');
      return { preview: s !== 'air', air: s !== 'preview' };
    } catch {
      return { preview: true, air: true };
    }
  });
  const set = (patch: Partial<Monitors>) => {
    const next = { ...v, ...patch };
    setV(next);
    try {
      localStorage.setItem('strata.opScreens', next.preview && next.air ? 'both' : next.preview ? 'preview' : next.air ? 'air' : 'none');
    } catch {
      /* private mode */
    }
  };
  return [v, set];
}

/** Shows or hides a monitor's picture, at the right of its header. */
function MonitorToggle({ on, onChange }: { on: boolean; onChange: (on: boolean) => void }) {
  return (
    <button className="icon-btn ghost opx-mon-eye" aria-pressed={on} title={on ? t('Hide the picture') : t('Show the picture')} onClick={() => onChange(!on)}>
      {on ? <Eye size={15} /> : <EyeOff size={15} />}
    </button>
  );
}

/** In the project's top bar: the output and its web page. */
function TopControls({ channelId, channelInfo }: { channelId: string | null; channelInfo?: ChannelInfo }) {
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  useEffect(() => setSlot(document.getElementById('topbar-slot')), []);
  if (!slot) return null;
  return createPortal(
    <div className="opx-top">
      <OutputSelect channelId={channelId} />
      {channelInfo && (
        <a className="opx-btn small opx-html5" href={`/output/${encodeURIComponent(channelInfo.id)}`} target="_blank" rel="noreferrer">
          HTML5 ↗
        </a>
      )}
    </div>,
    slot,
  );
}

/** The output the operator takes to; another project's output is taken over after a confirmation. */
const OutputSelect = memo(function OutputSelect({ channelId }: { channelId: string | null }) {
  const { channels, projects, project, loadOnChannel } = useApp(useShallow((s) => ({ channels: s.channels, projects: s.projects, project: s.project, loadOnChannel: s.loadOnChannel })));
  // Read when asked, not followed: this list is not drawn again on each frame.
  const live = () => useApp.getState().live;
  const projectName = (id?: string | null) => projects.find((p) => p.id === id)?.name ?? t('another project');
  const pick = async (id: string) => {
    if (!project) return;
    if (!id) {
      // No output: the channel it had is unloaded (it shows its picture without project).
      const cur = channels.find((x) => x.id === channelId && x.projectId === project.id);
      if (cur) {
        const onAir = Object.keys(live()[cur.id]?.state.layers ?? {}).length;
        if (onAir && !(await confirmDialog({ title: t('Unload {channel}?', { channel: cur.name }), message: tn(onAir, 'The {n} graphic on air on {channel} is cleared.', 'The {n} graphics on air on {channel} are cleared.', { channel: cur.name }), confirm: t('Unload'), danger: true }))) return;
        await loadOnChannel(cur.id, null);
      }
      return navigate(projectPath(project.id, 'operator', 'none'));
    }
    const c = channels.find((x) => x.id === id);
    if (!c) return;
    if (c.projectId !== project.id) {
      const onAir = Object.keys(live()[c.id]?.state.layers ?? {}).length;
      if (
        c.projectId &&
        !(await confirmDialog({
          title: t('{channel} is used by “{project}”', { channel: c.name, project: projectName(c.projectId) }),
          message: onAir ? tn(onAir, '{n} graphic is on air there and will be cleared. Load “{name}” on it instead?', '{n} graphics are on air there and will be cleared. Load “{name}” on it instead?', { name: project.name }) : t('Load “{name}” on it instead?', { name: project.name }),
          confirm: t('Load here'),
          danger: onAir > 0,
        }))
      )
        return;
      await loadOnChannel(c.id, project.id);
    }
    navigate(projectPath(project.id, 'operator', c.id));
  };
  // Graphics on air per output, as one string: the list is drawn again only when a count changes.
  const onAir = useApp((s) => channels.map((c) => Object.keys(s.live[c.id]?.state.layers ?? {}).length).join(','))
    .split(',')
    .map(Number);
  const airOf = (c: ChannelInfo) => onAir[channels.indexOf(c)] || 0;
  const dot = (c?: ChannelInfo) => <span className={`opx-dot ${!c ? 'none' : airOf(c) ? 'live' : ''}`} />;
  const format = (c: ChannelInfo) => `${c.height}${c.interlaced ? 'i' : 'p'}${+(c.frameRate.num / Math.max(1, c.frameRate.den)).toFixed(2)}`;
  const kinds = (c: ChannelInfo) => [...new Set(c.outputs.map((o) => o.split(' · ')[0]))].join(' · ') || t('No destination');
  const item = (c: ChannelInfo, group: string, sub: string): DropOption<string> => ({ value: c.id, label: c.name, sub, icon: dot(c), note: format(c), detail: format(c), group });
  const options: DropOption<string>[] = [
    { value: '', label: t('No output'), sub: t('Preview only'), icon: dot() },
    ...channels.filter((c) => c.projectId === project?.id).map((c) => item(c, t('This project'), kinds(c))),
    ...channels.filter((c) => !c.projectId).map((c) => item(c, tc('channels', 'Free'), kinds(c))),
    ...channels
      .filter((c) => c.projectId && c.projectId !== project?.id)
      .map((c) => item(c, t('Used by another project'), [`« ${projectName(c.projectId)} »`, ...(airOf(c) ? [tn(airOf(c), '{n} on air', '{n} on air')] : [])].join(' · '))),
  ];
  return <Dropdown value={channelId ?? ''} options={options} title={t('Output')} className="opx-output" onChange={(v) => void pick(v)} />;
});

import { t, tc } from '../i18n';
import { emptyHardware, emptyNetwork, emptySync, type ClockReference, type DeckLinkModel, type DetectedCard, type HardwareConfig, type PortCap, type SdiCard, type SdiModel, type SdiMode, type SdiOutput, type SdiOutputDef, type ServerConfig, type St2110Flow, type St2110Output } from '../outputs/types';

/*
 * The video hardware of the server (Hardware page), shared with the Video
 * streams page: DeckLink card models, what each connector does, and how a
 * destination takes its settings from what Hardware declares (the server
 * does the same when it loads the settings: crates/cg-server/src/hardware.rs).
 */

export type { PortCap };

export const SDI_MODELS: { id: SdiModel; label: string; ports: PortCap[]; oneDevice?: boolean }[] = [
  { id: 'miniMonitor', label: 'DeckLink Mini Monitor', ports: ['out'], oneDevice: true },
  { id: 'miniRecorder', label: 'DeckLink Mini Recorder', ports: ['in'], oneDevice: true },
  { id: 'sdi4k', label: 'DeckLink SDI 4K', ports: ['in', 'out'], oneDevice: true },
  { id: 'duo2', label: 'DeckLink Duo 2', ports: ['both', 'both', 'both', 'both'] },
  { id: 'quad2', label: 'DeckLink Quad 2', ports: Array(8).fill('both') },
  { id: '8kPro', label: 'DeckLink 8K Pro', ports: ['both', 'both', 'both', 'both'] },
  { id: 'other', label: 'Other DeckLink card', ports: [] },
];

export const modelOf = (id: string) => SDI_MODELS.find((m) => m.id === id) ?? SDI_MODELS[SDI_MODELS.length - 1];

/** What each connector of a card can do. */
export const portsOf = (c: SdiCard): PortCap[] => {
  const m = modelOf(c.model);
  return m.ports.length ? m.ports : c.connectors.map(() => 'both');
};

/** The Desktop Video device of a connector: as detected, else one per connector (but on single-device cards). */
export const deviceOf = (c: SdiCard, connector: number) => c.devices?.[connector] ?? (modelOf(c.model).oneDevice ? c.device : c.device + connector);

/** How a card looks from the back of the machine, and what it can key. */
export interface Bracket {
  jack: 'din' | 'bnc';
  /** Connector numbers (1 = SDI 1) from the top of the bracket down; the REF comes after. */
  order: number[];
  reference: boolean;
  ports: PortCap[];
  internalKey: boolean;
  externalKey: boolean;
  /** Keyer pairs, connectors from 0 (the first one first). */
  pairs: [number, number][];
  /** One Desktop Video profile for the whole card (8K Pro): a keyed pair puts every pair in full duplex. */
  cardProfile: boolean;
}

/** The table entry of a card's model (crates/cg-server/src/decklink.rs): by name, else by the hand-declared model. */
export function modelEntry(c: { modelName?: string; model?: string }, models: DeckLinkModel[]): DeckLinkModel | undefined {
  const name = c.modelName?.trim().toLowerCase();
  return (name ? models.find((m) => m.name.toLowerCase() === name) : undefined) ?? (c.model && c.model !== 'other' && !name ? models.find((m) => m.legacy === c.model) : undefined);
}

/** The bracket of a card; models not in the table: connectors in numeric order, the REF when the driver reports one. */
export function bracketOf(c: SdiCard, models: DeckLinkModel[], det?: DetectedCard): Bracket {
  const n = c.connectors.length;
  const m = modelEntry(c, models);
  const numeric = Array.from({ length: n }, (_, i) => i + 1);
  // Not in the table: keyer pairs 1+2, 3+4… when the driver says the card keys.
  const keys = { internal: !!det?.internalKeyer, external: !!det?.externalKeyer };
  const pairs: [number, number][] = m ? m.keying.pairs.map(([a, b]) => [a - 1, b - 1]) : keys.internal || keys.external ? Array.from({ length: Math.floor(n / 2) }, (_, i) => [2 * i, 2 * i + 1]) : [];
  return {
    jack: m?.jack ?? (n > 4 ? 'din' : 'bnc'),
    order: m && m.order.length === n ? m.order : numeric,
    reference: m?.reference ?? det?.hasReference ?? false,
    ports: m && m.ports.length === n ? m.ports : det && det.ports.length === n ? det.ports : portsOf(c),
    internalKey: m ? m.keying.internal : keys.internal,
    externalKey: m ? m.keying.external : keys.external,
    pairs,
    cardProfile: !!m?.keying.cardProfile,
  };
}

/**
 * What a connector does: an output (fill only), the fill or the key of a
 * Fill + Key pair, an input, or the input / output of an internal keyer.
 */
export type Role = 'unused' | 'output' | 'fill' | 'key' | 'input' | 'kin' | 'kout';

/** What a connector does, and for which output or input. */
export function roleOf(hw: HardwareConfig, card: string, connector: number): { role: Role; output?: SdiOutputDef; input?: HardwareConfig['sdi']['inputs'][number] } {
  for (const o of hw.sdi.outputs) {
    if (o.card !== card) continue;
    if (o.fill === connector) return { role: o.mode === 'fillOnly' ? 'output' : o.mode === 'internalKey' ? 'kin' : 'fill', output: o };
    if (o.mode !== 'fillOnly' && o.key === connector) return { role: o.mode === 'internalKey' ? 'kout' : 'key', output: o };
  }
  const input = hw.sdi.inputs.find((i) => i.card === card && i.connector === connector);
  return input ? { role: 'input', input } : { role: 'unused' };
}

export const ENCODINGS: { value: St2110Flow['encoding']; label: string }[] = [
  { value: 'fillKey', label: 'Fill + Key · 2 streams (YCbCr 4:2:2 10-bit + KEY)' },
  { value: 'rgba', label: 'RGBA · 1 stream (4:4:4:4 8-bit)' },
  { value: 'ycbcra', label: 'YCbCrA · 1 stream (4:2:2:4 10-bit)' },
];

export const hardwareOf = (c: ServerConfig): HardwareConfig => {
  const hw = c.hardware ?? emptyHardware();
  hw.sdi ??= { cards: [], outputs: [], inputs: [] };
  hw.sdi.cards ??= [];
  hw.sdi.outputs ??= [];
  hw.sdi.inputs ??= [];
  hw.st2110 ??= { flows: [] };
  hw.st2110.flows ??= [];
  // Always sent by the server; older ones lack them.
  hw.network ??= emptyNetwork();
  hw.sync ??= emptySync();
  return hw;
};

export const CLOCKS: ClockReference[] = ['auto', 'free', 'ptp', 'decklink'];

export const clockLabel = (r: ClockReference) =>
  r === 'ptp' ? 'PTP' : r === 'decklink' ? 'DeckLink (genlock)' : r === 'auto' ? tc('clock reference', 'Automatic') : t("Server's time");

/** "SDI 1 + 2" (a keyer's pair) or "SDI 4". */
export const sdiPorts = (d: SdiOutputDef) => (d.mode !== 'fillOnly' && d.key != null ? `SDI ${d.fill + 1} + ${d.key + 1}` : `SDI ${d.fill + 1}`);

/** "Duo 2 · SDI 1 + 2". */
export function sdiOutputLabel(hw: HardwareConfig, d: SdiOutputDef): string {
  const card = hw.sdi.cards.find((c) => c.id === d.card);
  return `${card?.name ?? '?'} · ${sdiPorts(d)}`;
}

export const modeLabel = (m: SdiMode) => (m === 'fillOnly' ? t('Output') : m === 'internalKey' ? t('Internal keyer') : 'Fill + Key');

/** An SDI destination takes its settings from its Hardware output. */
export function fillSdi(o: SdiOutput, hw: HardwareConfig) {
  const d = hw.sdi.outputs.find((x) => x.id === o.hardware);
  if (!d) return;
  const card = hw.sdi.cards.find((c) => c.id === d.card);
  if (card) o.device = deviceOf(card, d.fill);
  o.externalKey = d.mode !== 'internalKey';
  o.fillOnly = d.mode === 'fillOnly';
  o.genlock = d.genlock || !!card?.genlock;
  // The server leaves out a zero offset.
  if (card?.genlockOffset) o.genlockOffset = card.genlockOffset;
  else delete o.genlockOffset;
}

/** An ST 2110 destination takes its addresses from its Hardware flow. */
export function fillSt2110(o: St2110Output, hw: HardwareConfig) {
  const f = hw.st2110.flows.find((x) => x.id === o.hardware);
  if (!f) return;
  o.encoding = f.encoding;
  o.fill = f.fill;
  o.key = f.encoding === 'fillKey' ? (f.key ?? null) : null;
  o.interface = f.interface ?? null;
  o.ttl = f.ttl;
  o.payloadType = f.payloadType;
}

/** Ids of the Hardware outputs and flows the channels' destinations use (unwired ones aside). */
export function usedHardware(c: ServerConfig): Set<string> {
  const used = new Set<string>();
  for (const ch of c.channels)
    for (const o of [ch.outputs, ...(ch.renditions ?? []).map((r) => r.outputs)]) {
      for (const d of o.sdi ?? []) if (d.hardware && d.enabled !== false) used.add(d.hardware);
      for (const d of o.st2110 ?? []) if (d.hardware && d.enabled !== false) used.add(d.hardware);
    }
  return used;
}

/** The Video streams destinations and inputs naming a Hardware id. */
export function usersOf(c: ServerConfig, id: string): string[] {
  const out: string[] = [];
  for (const ch of c.channels)
    for (const o of [ch.outputs, ...(ch.renditions ?? []).map((r) => r.outputs), ...(ch.unwired ? [ch.unwired] : [])]) {
      for (const d of [...(o.sdi ?? []), ...(o.st2110 ?? [])]) if (d.hardware === id) out.push(`${ch.name} › ${d.name}`);
    }
  for (const i of c.inputs ?? []) if (i.hardware === id && (i.kind === 'sdi' || i.kind === 'st2110')) out.push(i.name);
  return out;
}

/** Removes the destinations and inputs naming a Hardware id (it is being deleted). */
export function dropUsers(c: ServerConfig, id: string) {
  for (const ch of c.channels)
    for (const o of [ch.outputs, ...(ch.renditions ?? []).map((r) => r.outputs), ...(ch.unwired ? [ch.unwired] : [])]) {
      o.sdi = (o.sdi ?? []).filter((d) => d.hardware !== id);
      o.st2110 = (o.st2110 ?? []).filter((d) => d.hardware !== id);
    }
  c.inputs = (c.inputs ?? []).filter((i) => !(i.hardware === id && (i.kind === 'sdi' || i.kind === 'st2110')));
}

/** Copies the Hardware settings into every destination and input using them (what the server does on load). */
export function syncUsers(c: ServerConfig) {
  const hw = hardwareOf(c);
  for (const ch of c.channels)
    for (const o of [ch.outputs, ...(ch.renditions ?? []).map((r) => r.outputs), ...(ch.unwired ? [ch.unwired] : [])]) {
      for (const d of o.sdi ?? []) fillSdi(d, hw);
      for (const d of o.st2110 ?? []) fillSt2110(d, hw);
    }
  for (const i of c.inputs ?? []) {
    const d = i.kind === 'sdi' ? hw.sdi.inputs.find((x) => x.id === i.hardware) : undefined;
    const card = d && hw.sdi.cards.find((x) => x.id === d.card);
    if (d && card) i.device = deviceOf(card, d.connector);
  }
}

/** "25 Gb/s" from Mb/s. */
export const speedLabel = (mbps?: number | null) => (mbps == null ? '' : mbps >= 1000 ? `${+(mbps / 1000).toFixed(1)} Gb/s` : `${mbps} Mb/s`);

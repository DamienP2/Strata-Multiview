/*
 * Automatic arrangement of a node canvas, from left to right (the data flows
 * that way in the Data, Logic and Video streams canvases).
 *
 *   autoLayout(nodes, edges, opts?) → { [id]: { x, y } }   top-left corner of each node
 *
 *   nodes   { id, w, h, side? }[]   size in canvas units (measureNodes gives it);
 *                                   side 'start' pins a node to the first column,
 *                                   'end' to the last one (sources, outputs…)
 *   edges   { from, to }[]          from an output to an input; unknown ids,
 *                                   repeats and loops are ignored
 *   opts    gapX (80)               between two columns
 *           gapY (28)               between two nodes of a column
 *           origin ({ x: 40, y: 40 }) top-left corner of the whole
 *           alignSinks (true)       nodes that feed nothing go to the last column
 *           tightSources (false)    nodes fed by nothing sit just left of what they
 *                                   feed instead of in the first column (unless pinned)
 *           anchorY (20)            where the wires meet a node, from its top (title bar):
 *                                   linked nodes are lined up on it
 *
 * Layers by longest path (cycles are broken first), crossings reduced by
 * barycenter sweeps (long wires get a lane of their own through the columns
 * they cross), columns stacked then straightened towards their neighbours.
 * Nodes without any wire (and not pinned) go in a grid below. Same input, same output.
 *
 * In a canvas:
 *   const cv = useCanvasView(key, '.logic-node', { onArrange: () => {
 *     const size = measureNodes(cv.ref.current);           // nodes carry data-node-id
 *     const pos = autoLayout(nodes.map((n) => ({ id: n.id, ...(size[n.id] ?? { w: 260, h: 120 }) })), edges);
 *     save(pos);                                            // as a drag of the nodes would
 *   } });
 * The Arrange button then appears next to Fit, and the view fits once the nodes have moved.
 */

export interface LayoutNode {
  id: string;
  w: number;
  h: number;
  /** Pins the node to the first or the last column. */
  side?: 'start' | 'end';
}
export interface LayoutEdge {
  from: string;
  to: string;
}
export interface LayoutOptions {
  gapX?: number;
  gapY?: number;
  origin?: { x: number; y: number };
  alignSinks?: boolean;
  tightSources?: boolean;
  anchorY?: number;
}

/** Sizes of the nodes in `root` (canvas units, whatever the zoom), by their `data-node-id`. */
export function measureNodes(root: Element | null, attr = 'data-node-id'): Record<string, { w: number; h: number }> {
  const out: Record<string, { w: number; h: number }> = {};
  for (const el of root?.querySelectorAll<HTMLElement>(`[${attr}]`) ?? []) out[el.getAttribute(attr)!] = { w: el.offsetWidth, h: el.offsetHeight };
  return out;
}

export function autoLayout(nodes: LayoutNode[], edges: LayoutEdge[], opts: LayoutOptions = {}): Record<string, { x: number; y: number }> {
  const gapX = opts.gapX ?? 80;
  const gapY = opts.gapY ?? 28;
  const origin = opts.origin ?? { x: 40, y: 40 };
  const alignSinks = opts.alignSinks ?? true;
  const n = nodes.length;
  const index = new Map<string, number>();
  nodes.forEach((x, i) => index.has(x.id) || index.set(x.id, i));

  // ---- The graph: indices, no repeats, no loops on a node.
  const seen = new Set<string>();
  const links: [number, number][] = [];
  for (const e of edges) {
    const a = index.get(e.from);
    const b = index.get(e.to);
    if (a === undefined || b === undefined || a === b || seen.has(`${a}>${b}`)) continue;
    seen.add(`${a}>${b}`);
    links.push([a, b]);
  }
  const outs: number[][] = nodes.map(() => []);
  for (const [a, b] of links) outs[a].push(b);

  // ---- Cycles broken: the edges going back in a depth-first walk are left out.
  const state = new Uint8Array(n); // 0 new, 1 on the path, 2 done
  const back = new Set<string>();
  for (let s = 0; s < n; s++) {
    if (state[s]) continue;
    const stack: [number, number][] = [[s, 0]];
    state[s] = 1;
    while (stack.length) {
      const top = stack[stack.length - 1];
      const [v, k] = top;
      if (k < outs[v].length) {
        top[1]++;
        const w = outs[v][k];
        if (state[w] === 1) back.add(`${v}>${w}`);
        else if (!state[w]) {
          state[w] = 1;
          stack.push([w, 0]);
        }
      } else {
        state[v] = 2;
        stack.pop();
      }
    }
  }
  const dag = links.filter(([a, b]) => !back.has(`${a}>${b}`));
  const preds: number[][] = nodes.map(() => []);
  const succs: number[][] = nodes.map(() => []);
  for (const [a, b] of dag) {
    preds[b].push(a);
    succs[a].push(b);
  }
  const wired = new Array<boolean>(n).fill(false);
  for (const [a, b] of links) wired[a] = wired[b] = true;

  // ---- Layers: longest path from the sources (topological order, stable).
  const layer = new Array<number>(n).fill(0);
  const indeg = preds.map((p) => p.length);
  const queue: number[] = [];
  for (let i = 0; i < n; i++) if (!indeg[i]) queue.push(i);
  for (let q = 0; q < queue.length; q++) {
    const v = queue[q];
    for (const w of succs[v]) {
      layer[w] = Math.max(layer[w], layer[v] + 1);
      if (!--indeg[w]) queue.push(w);
    }
  }
  const main = nodes.map((_, i) => i).filter((i) => wired[i] || nodes[i].side);
  let last = Math.max(0, ...main.map((i) => layer[i]));
  if (main.some((i) => nodes[i].side === 'start') && main.some((i) => nodes[i].side === 'end')) last = Math.max(last, 1);
  if (alignSinks) for (const i of main) if (!succs[i].length && preds[i].length) layer[i] = last;
  if (opts.tightSources)
    for (const i of main) if (!preds[i].length && succs[i].length && !nodes[i].side) layer[i] = Math.max(0, Math.min(...succs[i].map((w) => layer[w])) - 1);
  for (const i of main) if (nodes[i].side) layer[i] = nodes[i].side === 'start' ? 0 : last;
  // No empty column.
  const used = [...new Set(main.map((i) => layer[i]))].sort((a, b) => a - b);
  const col = new Map(used.map((l, k) => [l, k]));
  for (const i of main) layer[i] = col.get(layer[i])!;
  const cols = used.length;

  // ---- Order in each column: barycenter sweeps, with stand-ins on the long edges.
  type Item = { node: number }; // node < 0: a stand-in
  const columns: Item[][] = Array.from({ length: cols }, () => []);
  // Starting order: as given.
  for (const i of main) columns[layer[i]].push({ node: i });
  const up = new Map<Item, Item[]>(); // neighbours in the column on the left
  const down = new Map<Item, Item[]>(); // … on the right
  const itemOf = new Map<number, Item>();
  for (const c of columns) for (const it of c) itemOf.set(it.node, it);
  const join = (a: Item, b: Item) => {
    down.set(a, [...(down.get(a) ?? []), b]);
    up.set(b, [...(up.get(b) ?? []), a]);
  };
  let stand = -1;
  for (const [a, b] of dag) {
    if (!itemOf.has(a) || !itemOf.has(b) || layer[b] <= layer[a]) continue;
    let prev = itemOf.get(a)!;
    for (let l = layer[a] + 1; l < layer[b]; l++) {
      const s: Item = { node: stand-- };
      columns[l].push(s);
      join(prev, s);
      prev = s;
    }
    join(prev, itemOf.get(b)!);
  }
  const pos = new Map<Item, number>();
  const number = () => columns.forEach((c) => c.forEach((it, k) => pos.set(it, k)));
  number();
  const sweep = (c: number, side: Map<Item, Item[]>) => {
    const bary = new Map<Item, number>();
    for (const it of columns[c]) {
      const nb = side.get(it) ?? [];
      bary.set(it, nb.length ? nb.reduce((s, x) => s + pos.get(x)!, 0) / nb.length : pos.get(it)!);
    }
    columns[c] = columns[c].map((it, k) => ({ it, k })).sort((a, b) => bary.get(a.it)! - bary.get(b.it)! || a.k - b.k).map((x) => x.it);
    columns[c].forEach((it, k) => pos.set(it, k));
  };
  const crossings = () => {
    let total = 0;
    for (let c = 0; c + 1 < cols; c++) {
      const segs: [number, number][] = [];
      for (const it of columns[c]) for (const o of down.get(it) ?? []) segs.push([pos.get(it)!, pos.get(o)!]);
      for (let i = 0; i < segs.length; i++) for (let j = i + 1; j < segs.length; j++) if ((segs[i][0] - segs[j][0]) * (segs[i][1] - segs[j][1]) < 0) total++;
    }
    return total;
  };
  let best = columns.map((c) => [...c]);
  let fewest = crossings();
  for (let round = 0; round < 6 && fewest > 0; round++) {
    for (let c = 1; c < cols; c++) sweep(c, up);
    for (let c = cols - 2; c >= 0; c--) sweep(c, down);
    const x = crossings();
    if (x < fewest) {
      fewest = x;
      best = columns.map((c) => [...c]);
    }
  }
  const order = best;

  // ---- Columns: x from the widest node; y stacked, centred, then towards the neighbours.
  // A stand-in keeps a thin lane open, so a long wire never runs through a node.
  const hOf = (it: Item) => (it.node >= 0 ? nodes[it.node].h : 8);
  const x = new Array<number>(n).fill(0);
  const y = new Map<Item, number>();
  let cx = origin.x;
  for (const c of order) {
    const real = c.filter((it) => it.node >= 0);
    for (const it of real) x[it.node] = cx;
    cx += Math.max(0, ...real.map((it) => nodes[it.node].w)) + gapX;
  }
  const height = (c: Item[]) => c.reduce((s, it) => s + hOf(it), 0) + gapY * Math.max(0, c.length - 1);
  const tallest = Math.max(0, ...order.map(height));
  for (const c of order) {
    let cy = (tallest - height(c)) / 2;
    for (const it of c) {
      y.set(it, cy);
      cy += hOf(it) + gapY;
    }
  }
  // Wires leave and reach the nodes near their title bar: that is what lines up.
  const anchorY = opts.anchorY ?? 20;
  const lead = (it: Item) => Math.min(anchorY, hOf(it) / 2);
  const anchor = (it: Item) => y.get(it)! + lead(it);
  // Long wires are drawn straight from end to end: their lanes weigh more than the nodes
  // in the way, and a node lines up with its long wires before its other neighbours.
  const weight = (it: Item) => (it.node >= 0 ? 1 : 8);
  /** Moves a column towards where its neighbours want it, keeping its order and gaps (pool-adjacent blocks). */
  const straighten = (c: Item[], side: Map<Item, Item[]>) => {
    const want = c.map((it) => {
      const all = side.get(it) ?? [];
      const lanes = all.filter((x) => x.node < 0);
      const nb = lanes.length ? lanes : all;
      if (!nb.length) return y.get(it)!;
      const m = nb.map(anchor).sort((a, b) => a - b);
      const mid = (m[(m.length - 1) >> 1] + m[m.length >> 1]) / 2;
      return mid - lead(it);
    });
    // Item k sits at t + offs[k], t its block's start: order and gaps hold while t never decreases.
    const offs: number[] = [];
    let off = 0;
    for (const it of c) {
      offs.push(off);
      off += hOf(it) + gapY;
    }
    type Block = { from: number; to: number; sum: number; w: number; t: number };
    const blocks: Block[] = [];
    c.forEach((it, k) => {
      const w = weight(it);
      let b: Block = { from: k, to: k, sum: (want[k] - offs[k]) * w, w, t: want[k] - offs[k] };
      while (blocks.length && blocks[blocks.length - 1].t >= b.t) {
        const p = blocks.pop()!;
        const sum = p.sum + b.sum;
        b = { from: p.from, to: b.to, sum, w: p.w + b.w, t: sum / (p.w + b.w) };
      }
      blocks.push(b);
    });
    for (const b of blocks) for (let k = b.from; k <= b.to; k++) y.set(c[k], b.t + offs[k]);
  };
  for (const [list, side] of [
    [order, up],
    [[...order].reverse(), down],
    [order, up],
  ] as const)
    for (const c of list) straighten(c, side);
  const yOf = (i: number) => y.get(itemOf.get(i)!)!;

  // ---- Into place: the whole at the origin, then the loose nodes in a grid below.
  const out: Record<string, { x: number; y: number }> = {};
  const top = Math.min(...main.map(yOf));
  let bottom = origin.y;
  let right = origin.x;
  for (const i of main) {
    out[nodes[i].id] = { x: Math.round(x[i]), y: Math.round(yOf(i) - top + origin.y) };
    bottom = Math.max(bottom, out[nodes[i].id].y + nodes[i].h);
    right = Math.max(right, x[i] + nodes[i].w);
  }
  const loose = nodes.map((_, i) => i).filter((i) => !wired[i] && !nodes[i].side);
  const wrap = Math.max(right, origin.x + 900);
  let gx = origin.x;
  let gy = main.length ? bottom + gapY * 2 : origin.y;
  let row = 0;
  for (const i of loose) {
    if (gx > origin.x && gx + nodes[i].w > wrap) {
      gx = origin.x;
      gy += row + gapY;
      row = 0;
    }
    out[nodes[i].id] = { x: Math.round(gx), y: Math.round(gy) };
    gx += nodes[i].w + gapX / 2;
    row = Math.max(row, nodes[i].h);
  }
  return out;
}

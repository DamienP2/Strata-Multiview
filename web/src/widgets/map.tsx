// Map: stylised offline vector map (no network, ever): projections, key
// views with fly transitions, markers, great-circle routes, zones from data,
// labels by priority, « Aller à… », names in the project's language, the
// attribution of the data drawn.
// Same picture as crates/cg-render/src/wid/map.rs (line for line); actions in
// crates/cg-core/src/wid/map.rs; geometry and logic in ./map-geo.ts.
import { useRef, useState } from 'react';
import { ChevronsRight, Map as MapIcon, MapPin, Pause, Play, Route as RouteIcon, RotateCcw, Search, Shapes, SkipBack, SkipForward, Square, Tag, Zap, type LucideIcon } from 'lucide-react';
import { num } from '../engine/evaluate';
import type { Data } from '../engine/types';
import { t, tc, tl, tn } from '../i18n';
import { useApp } from '../store';
import { Dropdown } from '../ui/Dropdown';
import { Btns, ColorPick, defineWidget, GridEditor, ListEditor, Num, RangeNum, Row, Section, Seg, Text, Toggle, useBound, useCentreMode, useKeys, type Warning } from './kit';
import { ADVANCE_MODES, Centre, Countdown, Dots, GAP, WaitsContinue, WaitsTrigger, type AdvanceMode, type Line2Item } from './centre';
import { useLiveTime } from './table-text';
import { col, ellipse, poly, rrect, Pt, type Cmd, type RGBA } from './scoreboard-paint';
import {
  attribution,
  cfg,
  clampView,
  findPlace,
  hexMix,
  keyOf,
  layerOn,
  live,
  markerOn,
  markerPos,
  nextIn,
  normLon,
  placeName,
  placeNames,
  project,
  routeLine,
  routeOn,
  routeProgress,
  sample,
  shownState,
  shownView,
  startIndex,
  STYLES,
  unproject,
  viewAt,
  waitsContinue,
  world,
  wraps,
  zoneFills,
  zonesOf,
  type Config as GeoConfig,
  type Corner,
  type Icon,
  type K,
  type Ll,
  type Projection,
  type State as GeoState,
  type View,
  type Zone,
} from './map-geo';

export type Config = GeoConfig;
export type State = GeoState;

const D2R = Math.PI / 180;

/** The camera: world (X, Y) → layer pixels (mirrors Cam). */
class Cam {
  cx: number;
  cy: number;
  s: number;
  cr: number;
  sr: number;
  tilt: number;
  d: number;
  constructor(
    p: Projection,
    v: View,
    public w: number,
    public h: number,
  ) {
    [this.cx, this.cy] = project(p, v.lon, v.lat);
    this.s = 256 * Math.pow(2, v.zoom);
    this.cr = Math.cos(v.rot * D2R);
    this.sr = Math.sin(v.rot * D2R);
    this.tilt = v.tilt > 0.5 ? v.tilt * D2R : 0;
    this.d = w * 1.1;
  }
  tilted(x: number, y: number): [number, number] {
    if (this.tilt === 0) return [x, y];
    const oy = this.h * 0.67;
    const xx = x - this.w / 2;
    const yy = y - oy;
    const z = yy * Math.sin(this.tilt);
    const k = this.d / Math.max(this.d - z, this.d * 0.1);
    return [this.w / 2 + xx * k, oy + yy * Math.cos(this.tilt) * k];
  }
  flat(x: number, y: number): [number, number] {
    const dx = (x - this.cx) * this.s;
    const dy = (y - this.cy) * this.s;
    return [this.w / 2 + dx * this.cr - dy * this.sr, this.h / 2 + dx * this.sr + dy * this.cr];
  }
  scr(x: number, y: number): [number, number] {
    return this.tilted(...this.flat(x, y));
  }
  /** Path commands of plane points; tilted, first cut to the plane (mirrors emit). */
  emit(pts: [number, number][], close: boolean, out: Cmd[]) {
    const push = (run: [number, number][], cl: boolean) => {
      run.forEach(([x, y], i) => out.push([i ? 'L' : 'M', ...this.tilted(x, y)]));
      if (cl && run.length) out.push(['Z']);
    };
    if (this.tilt === 0) return push(pts, close);
    const [x0, y0, x1, y1] = [-0.35 * this.w, -0.35 * this.h, 1.35 * this.w, 1.35 * this.h];
    if (close) {
      let poly = pts;
      for (let side = 0; side < 4; side++) {
        const inside = (p: [number, number]) => (side === 0 ? p[0] >= x0 : side === 1 ? p[0] <= x1 : side === 2 ? p[1] >= y0 : p[1] <= y1);
        const cut = (a: [number, number], b: [number, number]): [number, number] => {
          const t = side === 0 ? (x0 - a[0]) / (b[0] - a[0]) : side === 1 ? (x1 - a[0]) / (b[0] - a[0]) : side === 2 ? (y0 - a[1]) / (b[1] - a[1]) : (y1 - a[1]) / (b[1] - a[1]);
          return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        };
        const next: [number, number][] = [];
        for (let i = 0; i < poly.length; i++) {
          const a = poly[i];
          const b = poly[(i + 1) % poly.length];
          if (inside(a)) next.push(a);
          if (inside(a) !== inside(b)) next.push(cut(a, b));
        }
        poly = next;
        if (!poly.length) return;
      }
      return push(poly, true);
    }
    let run: [number, number][] = [];
    for (let i = 1; i < pts.length; i++) {
      const a = pts[i - 1];
      const b = pts[i];
      const dx = b[0] - a[0];
      const dy = b[1] - a[1];
      let t0 = 0;
      let t1 = 1;
      let ok = true;
      for (const [pp, q] of [
        [-dx, a[0] - x0],
        [dx, x1 - a[0]],
        [-dy, a[1] - y0],
        [dy, y1 - a[1]],
      ]) {
        if (pp === 0) {
          if (q < 0) ok = false;
        } else {
          const r = q / pp;
          if (pp < 0) t0 = Math.max(t0, r);
          else t1 = Math.min(t1, r);
        }
      }
      if (!ok || t0 > t1) {
        push(run, false);
        run = [];
        continue;
      }
      if (!run.length || t0 > 0) {
        push(run, false);
        run = [[a[0] + dx * t0, a[1] + dy * t0]];
      }
      run.push([a[0] + dx * t1, a[1] + dy * t1]);
      if (t1 < 1) {
        push(run, false);
        run = [];
      }
    }
    push(run, false);
  }
  reach() {
    return Math.hypot(this.w, this.h) * (this.tilt > 0 ? 1.7 : 0.6);
  }
  copies(wrap: boolean) {
    if (!wrap) return [0];
    const r = this.reach();
    return [-2, -1, 0, 1, 2].filter((o) => (o + 1 - this.cx) * this.s > -r && (o - this.cx) * this.s < r);
  }
}

function rings(p: Projection, cam: Cam, list: Ll[][], close: boolean, offs: number[]): Cmd[] {
  const out: Cmd[] = [];
  for (const o of offs)
    for (const r of list) {
      cam.emit(
        r.map(([lo, la]) => {
          const [x, y] = project(p, lo, la);
          return cam.flat(x + o, y);
        }),
        close,
        out,
      );
    }
  return out;
}

const HALO: RGBA = [5 / 255, 8 / 255, 20 / 255, 0.85];
const WHITE: RGBA = [1, 1, 1, 1];
const withA = (c: RGBA, a: number): RGBA => [c[0], c[1], c[2], c[3] * a];

function pin(x: number, y: number, sz: number): Cmd[] {
  const r = sz / 2;
  const top = y - sz * 1.15;
  const c = top + r * 0.9;
  const k = 0.5523 * r;
  return [['M', x, y], ['C', x - r * 0.5, y - r * 0.8, x - r, top + r * 1.25, x - r, c], ['C', x - r, c - k, x - k, c - r, x, c - r], ['C', x + k, c - r, x + r, c - k, x + r, c], ['C', x + r, top + r * 1.25, x + r * 0.5, y - r * 0.8, x, y], ['Z']];
}
const PLANE: [number, number][] = [
  [0, -9],
  [1.6, -2.5],
  [8, 1],
  [8, 3],
  [1.6, 1.2],
  [1, 6],
  [3, 7.5],
  [3, 9],
  [0, 8],
  [-3, 9],
  [-3, 7.5],
  [-1, 6],
  [-1.6, 1.2],
  [-8, 3],
  [-8, 1],
  [-1.6, -2.5],
];
function plane(x: number, y: number, s: number, a: number): Cmd[] {
  const c = Math.cos(a);
  const si = Math.sin(a);
  return poly(
    PLANE.map(([px, py]) => [x + (px * c - py * si) * s, y + (px * si + py * c) * s]),
    true,
  );
}

function marker(p: Pt, icon: Icon, color: string, x: number, y: number, sz: number, now: number) {
  const r = sz / 2;
  if (icon === 'dot') {
    const ph = (((now % 1600) + 1600) % 1600) / 1600;
    const a = p.alpha;
    p.alpha = a * 0.85 * (1 - ph);
    p.circle(x, y, r * 0.55 * (1 + 2.2 * ph), color);
    p.alpha = a;
    const c = ellipse(x, y, r * 0.42, r * 0.42);
    p.fill(c, color);
    p.stroke(c, '#ffffff', 4, false);
  } else if (icon === 'star') {
    const pts: [number, number][] = Array.from({ length: 10 }, (_, i) => {
      const a = -Math.PI / 2 + (i * Math.PI) / 5;
      const rr = i % 2 ? r * 0.42 : r;
      return [x + Math.cos(a) * rr, y + Math.sin(a) * rr];
    });
    const c = poly(pts, true);
    p.fill(c, color);
    p.stroke(c, '#0b1020', 3, true);
  } else if (icon === 'stadium') {
    const c = rrect(x - r, y - r, sz, sz, r * 0.35);
    p.fill(c, color);
    p.stroke(c, '#ffffff', 4, false);
    p.stroke(ellipse(x, y, r * 0.6, r * 0.44), '#ffffff', 4, false);
    p.rect(x - r * 0.22, y - r * 0.14, r * 0.44, r * 0.28, 0, '#ffffff');
  } else if (icon === 'plane') {
    const c = ellipse(x, y, r, r);
    p.fill(c, color);
    p.stroke(c, '#ffffff', 4, false);
    p.fill(plane(x, y, r / 12, 0), '#ffffff');
  } else {
    const c = pin(x, y, sz);
    p.fill(c, color);
    p.stroke(c, '#ffffff', 3.5, false);
    p.circle(x, y - sz * 1.15 + r * 0.9, r * 0.36, '#ffffff');
  }
}

const corner = (pos: Corner, w: number, h: number, bw: number, bh: number, mx: number, my: number): [number, number] => [pos[1] === 'l' ? mx : w - mx - bw, pos[0] === 't' ? my : h - my - bh];

interface Cand {
  pri: number;
  kind: number; // 0 marker, 1 country, 2 continent, 3 city, 4 poi
  name: string;
  color: string;
  x: number;
  y: number;
  w: number;
  h: number;
  fs: number;
  px: number;
  py: number;
}

/** Draws the map; returns the hidden labels and the invalid markers (mirrors paint). */
/** Colour of the place « Aller à… » went to (mirrors GOTO). */
const GOTO = '#ffd200';

function paint(p: Pt, k: K, s: State, data: Data, now: number, w: number, h: number, debug: boolean, lang: string): { hidden: string[] } {
  const wd = world();
  const [v] = viewAt(k, s, now, w);
  const pj = k.projection;
  const cam = new Cam(pj, v, w, h);
  const offs = cam.copies(wraps(pj));
  const cl = k.colors;
  p.clip(rrect(0, 0, w, h, k.radius));

  const tilted = v.tilt > 0.5;
  if (tilted) {
    p.fillLin(
      rrect(0, 0, w, h, 0),
      [0, 0],
      [0, h],
      [
        [0, col('#0a0f22')],
        [0.28, col(hexMix(cl.water, '#9fb4ff', 0.35))],
        [0.6, col(cl.water)],
      ],
    );
    const q = (
      [
        [-0.35, -0.35],
        [1.35, -0.35],
        [1.35, 1.35],
        [-0.35, 1.35],
      ] as const
    ).map(([a, b]) => cam.tilted(a * w, b * h));
    p.clip(poly(q, true));
  }
  p.rect(-10, -10, w + 20, h + 20, 0, pj === 'equalearth' ? '#05070f' : cl.water);
  if (pj === 'equalearth') {
    const ring: Ll[] = [...Array.from({ length: 37 }, (_, i) => [180, -90 + i * 5] as Ll), ...Array.from({ length: 37 }, (_, i) => [-180, 90 - i * 5] as Ll)];
    p.fill(rings(pj, cam, [ring], true, offs), cl.water);
  }
  if (k.graticule) {
    const lines: Ll[][] = [];
    for (let j = 0; j <= 12; j++) {
      const lo = -180 + j * 30;
      lines.push(Array.from({ length: 37 }, (_, i) => [lo, -90 + i * 5] as Ll).filter((ll) => pj !== 'mercator' || Math.abs(ll[1]) <= 85));
    }
    for (const la of [-60, -30, 0, 30, 60]) lines.push(Array.from({ length: 73 }, (_, i) => [-180 + i * 5, la] as Ll));
    p.strokeC(rings(pj, cam, lines, false, offs), withA(col(cl.grid.trim() ? cl.grid : cl.border), 0.22), 1, false);
  }
  const landCmds = rings(pj, cam, wd.land, true, offs);
  p.fill(landCmds, cl.land);
  p.stroke(landCmds, hexMix(cl.land, '#ffffff', 0.18), 1.2, true);
  p.fill(rings(pj, cam, wd.lakes, true, offs), cl.water);

  const zoneOn = layerOn(k, s, 'zones') && v.zoom <= 10;
  const [zf] = zoneOn ? zoneFills(k.zones, data) : [{} as Record<string, { color: string; text: string }>];
  const hl = s.highlight ?? '';
  if (zoneOn) {
    const a0 = p.alpha;
    const pulse = Math.sin(((((now % 1100) + 1100) % 1100) / 1100) * 2 * Math.PI) * 0.5 + 0.5;
    for (const r of zonesOf(k.zones.level)) {
      const cmds = rings(pj, cam, r.rings, true, offs);
      const is = hl === r.id;
      p.alpha = a0 * Math.min(1, Math.max(0, k.zones.opacity)) * (hl && !is ? 0.32 : 1);
      p.fill(cmds, zf[r.id]?.color ?? cl.land);
      if (!is) p.stroke(cmds, cl.water, 1.6, true);
    }
    const r = zonesOf(k.zones.level).find((x) => x.id === hl);
    if (r) {
      p.alpha = a0;
      p.stroke(rings(pj, cam, r.rings, true, offs), '#ffffff', 3 + 7 * pulse, true);
    }
    p.alpha = a0;
  }
  if (k.showBorders) p.strokeC(rings(pj, cam, wd.borders, false, offs), col(cl.border), 1.8, true, [7, 4]);

  // Paris detail (from zoom 9, when Paris is in view).
  const ppm = cam.s / (40075016.7 * Math.cos(48.87 * D2R));
  const [parX, parY] = cam.scr(...project(pj, 2.3522, 48.8566));
  const near = parX > -30000 * ppm && parY > -30000 * ppm && parX < w + 30000 * ppm && parY < h + 30000 * ppm;
  if (v.zoom >= 9 && near) {
    const dp = wd.paris;
    const mw = (m: number, min: number) => Math.max(m * ppm, min);
    const one = (r: Ll[], close: boolean) => rings(pj, cam, [r], close, [0]);
    const urban = hexMix(cl.land, '#ffffff', 0.08);
    const park = k.style === 'mono' ? hexMix(cl.land, '#ffffff', 0.12) : '#2f6b3a';
    const stand = k.style === 'mono' ? '#c7cad3' : '#dfe3ee';
    p.fill(one(dp.urban, true), urban);
    for (const pk of dp.parks) p.fill(one(pk, true), park);
    p.stroke(rings(pj, cam, dp.streets, false, [0]), hexMix(urban, '#ffffff', 0.25), mw(12, 1.2), true);
    p.stroke(one(dp.seine, false), cl.water, mw(130, 3), true);
    p.stroke(one(dp.canal, false), cl.water, mw(32, 1.5), false);
    {
      const cmds = [...rings(pj, cam, [dp.peri], true, [0]), ...rings(pj, cam, dp.motorways, false, [0])];
      const ww = mw(34, 4);
      p.strokeC(cmds, withA(col('#0b0f1d'), 0.5), ww + 3, true);
      p.stroke(cmds, hexMix(cl.land, '#ffffff', 0.3), ww, true);
    }
    const st = one(dp.stands, true);
    p.fill(st, stand);
    p.strokeC(st, withA(col('#0b0f1d'), 0.4), 2, false);
    p.fill(one(dp.bowl, true), '#8f97ad');
    p.fill(one(dp.pitch, true), '#2f9e44');
    p.stroke([...one(dp.lines, true), ...one(dp.halfway, false), ...one(dp.circle, true)], '#ffffff', 1.6, false);
  }

  const inView = (x: number, y: number, m: number) => x > -m && y > -m && x < w + m && y < h + m;
  const wrapX = (x: number) => (wraps(pj) ? x - Math.round(x - cam.cx) : x);
  const rw = k.routeW;
  if (layerOn(k, s, 'routes'))
    for (const rt of k.routes) {
      if (!routeOn(s, rt)) continue;
      const [line] = routeLine(rt.path, k.curve);
      if (line.length < 2) continue;
      const segs: [number, number][][] = [[]];
      let prev: number | null = null;
      for (const [la, lo] of line) {
        const nl = normLon(lo);
        let [x, y] = project(pj, nl, la);
        const copy = Math.round((lo - nl) / 360);
        if (wraps(pj)) x += copy;
        else if (prev != null && prev !== copy) segs.push([]);
        prev = copy;
        segs[segs.length - 1].push([x, y]);
      }
      let total = 0;
      for (const sg of segs) for (let i = 1; i < sg.length; i++) total += Math.hypot(sg[i][0] - sg[i - 1][0], sg[i][1] - sg[i - 1][1]);
      const prog = routeProgress(s, rt, now);
      let left = total * prog;
      let head: [[number, number], [number, number]] | null = null;
      const ds: [number, number][][] = [];
      for (const sg of segs) {
        if (left <= 0) break;
        const part: [number, number][] = [sg[0]];
        for (let i = 1; i < sg.length; i++) {
          if (left <= 0) break;
          const l = Math.hypot(sg[i][0] - sg[i - 1][0], sg[i][1] - sg[i - 1][1]);
          if (l <= left) {
            part.push(sg[i]);
            left -= l;
          } else {
            const f = left / l;
            part.push([sg[i - 1][0] + (sg[i][0] - sg[i - 1][0]) * f, sg[i - 1][1] + (sg[i][1] - sg[i - 1][1]) * f]);
            left = 0;
          }
        }
        if (part.length > 1) head = [part[part.length - 2], part[part.length - 1]];
        ds.push(part);
      }
      const xs = segs.flat().map((q) => q[0]);
      const minX = Math.min(...xs);
      const maxX = Math.max(...xs);
      const R = cam.reach();
      const routeOffs = wraps(pj) ? [-2, -1, 0, 1, 2].filter((o) => (maxX + o - cam.cx) * cam.s > -R && (minX + o - cam.cx) * cam.s < R) : [0];
      for (const o of routeOffs) {
        const cmds: Cmd[] = [];
        for (const part of ds)
          cam.emit(
            part.map((q) => cam.flat(q[0] + o, q[1])),
            false,
            cmds,
          );
        p.strokeC(cmds, withA(col('#0b0f1d'), 0.55), rw + 6, true);
        p.stroke(cmds, rt.color, rw, true);
        const lastSeg = segs[segs.length - 1];
        const ends = [segs[0][0], ...(prog >= 1 ? [lastSeg[lastSeg.length - 1]] : [])];
        for (const q of ends) {
          const [x, y] = cam.scr(q[0] + o, q[1]);
          if (inView(x, y, 200)) {
            const e = ellipse(x, y, rw + 3, rw + 3);
            p.fill(e, '#ffffff');
            p.stroke(e, rt.color, 4, false);
          }
        }
        if (head && prog < 1 && k.planeHead) {
          const [ax, ay] = cam.scr(head[0][0] + o, head[0][1]);
          const [bx, by] = cam.scr(head[1][0] + o, head[1][1]);
          if (inView(bx, by, 200)) {
            const ang = Math.atan2(by - ay, bx - ax) + Math.PI / 2;
            const e = ellipse(bx, by, 13 * 2.2, 13 * 2.2);
            p.fill(e, rt.color);
            p.stroke(e, '#ffffff', 2 * 2.2, false);
            p.fill(plane(bx, by, 2.2, ang), '#ffffff');
          }
        }
      }
    }

  const cands: Cand[] = [];
  const sz = k.mkSize;
  if (layerOn(k, s, 'markers'))
    k.markers.forEach((m, i) => {
      if (!markerOn(s, m)) return;
      const pos = markerPos(m, data);
      if (!pos) return;
      const [X, Y] = project(pj, pos[1], pos[0]);
      const [x, y] = cam.scr(wrapX(X), Y);
      if (!inView(x, y, 200)) return;
      marker(p, m.icon, m.color, x, y, sz, now);
      if (m.name) {
        const fs = Math.round(sz * 0.62);
        const tw = p.tw(m.name, fs, 800, 0) + 26;
        const ly = m.icon === 'pin' ? y - sz * 0.75 : y;
        cands.push({ pri: 9.5 - i * 0.01, kind: 0, name: m.name, color: m.color, x: x + sz * 0.62 + 6, y: ly - fs * 0.8, w: tw, h: fs * 1.6, fs, px: x, py: y });
      }
    });
  // « Aller à… »: the place, a dot and its name.
  if (s.goto) {
    const g = s.goto;
    const [X, Y] = project(pj, g.lon, g.lat);
    const [x, y] = cam.scr(wrapX(X), Y);
    if (inView(x, y, 200)) {
      marker(p, 'dot', GOTO, x, y, sz, now);
      const fs = Math.round(sz * 0.62);
      const name = placeName(k, g.name, g.name, lang);
      const tw = p.tw(name, fs, 800, 0) + 26;
      cands.push({ pri: 10, kind: 0, name, color: GOTO, x: x + sz * 0.62 + 6, y: y - fs * 0.8, w: tw, h: fs * 1.6, fs, px: x, py: y });
    }
  }
  const lb = k.labels;
  if (layerOn(k, s, 'labels'))
    for (const [en, lat, lon, imp, minZ, kind, fr] of wd.places) {
      const name = placeName(k, en, fr, lang);
      const city = kind === 'city' || kind === 'poi';
      if ((city && !lb.cities) || (!city && !lb.countries)) continue;
      const maxZ = kind === 'country' ? 8.5 : kind === 'continent' ? 3.6 : 99;
      if (v.zoom < minZ || v.zoom > maxZ || imp < lb.minImp) continue;
      const [X, Y] = project(pj, lon, lat);
      const [x, y] = cam.scr(wrapX(X), Y);
      if (!inView(x, y, 0)) continue;
      const base = lb.size;
      if (!city) {
        const fs = Math.round(base * (kind === 'continent' ? 1.25 : 0.95));
        const tw = p.tw(name, fs, 800, fs * 0.14);
        cands.push({ pri: imp + (kind === 'continent' ? 0.5 : -0.5), kind: kind === 'continent' ? 2 : 1, name, color: '', x: x - tw / 2, y: y - fs / 2, w: tw, h: fs, fs, px: x, py: y });
      } else {
        const fs = Math.round(base * (0.78 + imp * 0.045));
        const poi = kind === 'poi';
        const tw = p.tw(name, fs, poi ? 700 : 800, 0);
        cands.push({ pri: imp, kind: poi ? 4 : 3, name, color: '', x: x + 12, y: y - fs * 0.62, w: tw, h: fs * 1.15, fs, px: x, py: y });
      }
    }
  cands.sort((a, b) => b.pri - a.pri);
  const placed: Cand[] = [];
  const hidden: string[] = [];
  const pad = lb.pad;
  for (const c of cands) {
    const hit = placed.some((q) => c.x < q.x + q.w + pad && c.x + c.w + pad > q.x && c.y < q.y + q.h + pad && c.y + c.h + pad > q.y);
    if (hit) {
      hidden.push(c.name);
      if (debug && lb.showHidden) {
        const b = rrect(c.x, c.y, c.w, c.h, 0);
        p.fill(b, '#ff3b5c1f');
        p.strokeC(b, col('#ff3b5c'), 2, false, [6, 4]);
        const a = p.alpha;
        p.alpha = a * 0.8;
        p.text(c.name, c.x + 4, c.y + c.h / 2, '#ff8da1', { size: c.fs * 0.8, weight: 700 });
        p.alpha = a;
      }
      continue;
    }
    placed.push(c);
    if (c.kind === 0) {
      p.rect(c.x, c.y, c.w, c.h, c.h / 2, '#080a16e0');
      p.clip(rrect(c.x, c.y, c.w, c.h, c.h / 2));
      p.rect(c.x, c.y, 7, c.h, 0, c.color);
      p.pop();
      p.text(c.name, c.x + 16, c.y + c.h / 2, '#ffffff', { size: c.fs, weight: 800 });
    } else if (c.kind === 1 || c.kind === 2) {
      const fill: RGBA = [1, 1, 1, c.kind === 1 ? 0.62 : 0.45];
      const o = { size: c.fs, weight: 800, ls: c.fs * 0.14 };
      if (c.kind === 1) p.textHalo(c.name, c.x, c.y + c.h / 2, fill, HALO, 6, o);
      else p.textC(c.name, c.x, c.y + c.h / 2, fill, o);
    } else {
      if (c.kind === 4) {
        const b = rrect(c.px - 6, c.py - 6, 12, 12, 3);
        p.fill(b, '#facc15');
        p.stroke(b, '#0b0f1d', 2, false);
      } else {
        const e = ellipse(c.px, c.py, 6, 6);
        p.fill(e, '#ffffff');
        p.stroke(e, '#0b0f1d', 3, false);
      }
      p.textHalo(c.name, c.x, c.y + c.h / 2, WHITE, HALO, 6, { size: c.fs, weight: c.kind === 4 ? 700 : 800 });
    }
  }

  if (zoneOn && hl) {
    const r = zonesOf(k.zones.level).find((x) => x.id === hl);
    if (r) {
      const [X, Y] = project(pj, r.lon, r.lat);
      const [x, y] = cam.scr(wrapX(X), Y);
      const z = zf[r.id];
      const t2 = z && z.text ? z.text : tl(lang, 'No data');
      const zname = placeName(k, r.en || r.name, r.name, lang);
      const zc = z ? z.color : '#ffffff';
      const bw = Math.max(p.tw(zname, 26, 800, 0), p.tw(t2, 22, 700, 0)) + 36;
      const bx = x - bw / 2;
      const by = y - 120;
      const b = rrect(bx, by, bw, 84, 12);
      p.fill(b, '#080a16eb');
      p.stroke(b, zc, 4, false);
      p.text(zname, bx + 18, by + 26.6, '#ffffff', { size: 26, weight: 800 });
      p.text(t2, bx + 18, by + 58, z ? hexMix(zc, '#ffffff', 0.35) : '#cbd5e1', { size: 22, weight: 700 });
      p.fill(
        poly(
          [
            [bx + bw / 2 - 12, by + 84],
            [bx + bw / 2, by + 104],
            [bx + bw / 2 + 12, by + 84],
          ],
          true,
        ),
        zc,
      );
    }
  }
  if (tilted) p.pop();

  const att = k.attribution;
  const [attText, attReq] = attribution(k, s, v.zoom, lang);
  const attVisible = att.show || attReq;
  if (zoneOn && k.zones.legend) {
    const z = k.zones;
    const title = String(z.title ?? '').toUpperCase();
    const tw = p.tw(title, 17, 800, 17 * 0.06);
    let bw: number, bh: number;
    const count = (c: string) => Object.values(zf).filter((f) => f.color === c).length;
    if (z.mode === 'scale') {
      bw = Math.max(250, tw + 36);
      bh = 14 + 20 + 10 + 18 + 8 + 20 + 14;
    } else {
      let mw = tw;
      for (const c of z.categories) mw = Math.max(mw, 32 + p.tw(c.name ?? '', 19, 700, 0) + 20 + p.tw(String(count(c.color ?? '')), 19, 700, 0));
      bw = Math.max(250, mw + 36);
      bh = 14 + 20 + 10 + z.categories.length * 30 + 14;
    }
    const my = z.legendPos === att.pos && attVisible ? 62 : 24;
    const [lx, ly] = corner(z.legendPos, w, h, bw, bh, 24, my);
    p.rect(lx, ly, bw, bh, 12, '#080a16d1');
    p.text(title, lx + 18, ly + 24, '#a594ff', { size: 17, weight: 800, ls: 17 * 0.06 });
    let yy = ly + 44;
    if (z.mode === 'scale') {
      const stops = z.scale;
      if (stops.length) {
        const a = Number(stops[0].v ?? 0);
        const span = Math.max(1e-9, Number(stops[stops.length - 1].v ?? 0) - a);
        const pos = (v: number) => Math.min(1, Math.max(0, (v - a) / span));
        p.fillLin(
          rrect(lx + 18, yy, bw - 36, 18, 9),
          [lx + 18, 0],
          [lx + bw - 18, 0],
          stops.map((st) => [pos(Number(st.v ?? 0)), col(st.color)] as [number, RGBA]),
        );
        const n = stops.length;
        stops.forEach((st, j) => {
          if (n > 4 && j !== 0 && j !== n - 1 && j !== Math.floor(n / 2)) return;
          const x = lx + 18 + (bw - 36) * pos(Number(st.v ?? 0));
          p.text(String(st.v ?? 0), x, yy + 36, '#ffffff', { size: 16, weight: 700, anchor: j === 0 ? -1 : j === n - 1 ? 1 : 0 });
        });
      }
    } else
      for (const c of z.categories) {
        p.rect(lx + 18, yy + 4, 22, 22, 5, c.color ?? '#ffffff');
        p.text(c.name ?? '', lx + 50, yy + 15, '#ffffff', { size: 19, weight: 700 });
        p.text(String(count(c.color ?? '')), lx + bw - 18, yy + 15, '#c9cde0', { size: 19, weight: 700, anchor: 1 });
        yy += 30;
      }
  }
  // Attribution of the data drawn (kept when the licence requires it).
  if (attVisible) {
    const text = attText;
    const tw = p.tw(text, 15, 600, 0) + 20;
    const [ax, ay] = corner(att.pos, w, h, tw, 26, 16, 14);
    if (att.style === 'minimal') p.textHalo(text, ax + 10, ay + 13, WHITE, [0, 0, 0, 0.7], 3, { size: 15, weight: 600 });
    else {
      p.rect(ax, ay, tw, 26, 6, att.style === 'light' ? '#ffffffe0' : '#00000099');
      p.text(text, ax + 10, ay + 13, att.style === 'light' ? '#111111' : '#e5e7eb', { size: 15, weight: 600 });
    }
  }
  p.pop();
  return { hidden };
}

// --------------------------------------------------------------- operator --

/** The layers the operator turns on and off, by name (translated where shown). */
const LAYER_NAMES = { markers: 'Markers', routes: 'Routes', zones: 'Zones', labels: 'Labels' } as const;
const LAYER_ICONS = { markers: MapPin, routes: RouteIcon, zones: Shapes, labels: Tag } as const;

type Act = (op: string, args?: unknown) => void;

/** « Aller à… »: a city, a zone (name or code) or « lat, lon »; Enter goes there. */
function GotoField({ act, lang, width = 120 }: { act: Act; lang: string; width?: number }) {
  const toast = useApp((s) => s.toast);
  const [q, setQ] = useState('');
  const [open, setOpen] = useState(false);
  const id = useRef(`map-gaz-${Math.random().toString(36).slice(2)}`).current;
  const go = () => {
    const x = q.trim();
    if (!x) return;
    if (!findPlace(x, lang)) return toast(t('“{place}” not found: type a city, a region, a departement, a country or “lat, lon”.', { place: x }), 'error');
    act('goto', { place: x, lang });
    setQ('');
  };
  return (
    <>
      <input
        className="cc-fld"
        style={{ width }}
        value={q}
        list={open ? id : undefined}
        placeholder={t('Go to…')}
        title={t('Go to…')}
        onFocus={() => setOpen(true)}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') go();
          if (e.key === 'Escape') setQ('');
        }}
      />
      {open && (
        <datalist id={id}>
          {placeNames(lang).map((n) => (
            <option key={n} value={n} />
          ))}
        </datalist>
      )}
      <button className="cc-sq" title={t('Go to…')} disabled={!q.trim()} onClick={go}>
        <Search size={15} />
      </button>
    </>
  );
}

type OpP = { layer: Config; state: unknown; act: Act; item?: unknown; lang?: string; time: { nowMs: number; ageSecs: number; held?: boolean } };

/** A zone's name in the lists (departements with their code). */
const zoneLabel = (k: K, z: Zone, level: string, lang: string) => `${level === 'deps' ? `${z.id} ` : ''}${placeName(k, z.en || z.name, z.name, lang)}`;
const zoneOptions = (k: K, lang: string) => zonesOf(k.zones.level).map((z) => ({ value: z.id, label: zoneLabel(k, z, k.zones.level, lang) })).sort((a, b) => (k.zones.level === 'deps' ? 0 : a.label.localeCompare(b.label, lang)));

/** A pad of a row: on (violet) or off; an icon, or a short text. */
function Pad({ on, title, icon: Icon, text, onClick }: { on: boolean; title: string; icon?: LucideIcon; text?: string; onClick: () => void }) {
  return (
    <button className={`cc-sq ${on ? 'violet' : ''} ${text ? 'txt' : ''}`} title={title} aria-pressed={on} onClick={onClick}>
      {text ? <span className="cc-txt">{text}</span> : Icon && <Icon size={15} />}
    </button>
  );
}
const PADS: React.CSSProperties = { display: 'flex', flexWrap: 'wrap', gap: 4 };

/**
 * The control centre of the map (web/src/widgets/centre.tsx). Line 1: the key view shown (or the
 * place gone to), the countdown to the next view or what it waits for. Line 2: by hand → previous
 * / next; timed → play / pause, stop; on Continue → Continue, stop; on a trigger → fire it, stop;
 * then Reset. Line 3: chain the views, how the next one comes, the time per view or the trigger
 * (in the Operator, on the air only), « Go to », the zone put forward (exposable), a route to draw,
 * the layers and the markers (pads, exposable).
 */
function MapBar(p: OpP) {
  const { act, layer, item, lang = 'fr' } = p;
  const mode = useCentreMode();
  const b = useBound();
  const designer = mode === 'designer';
  const { now, age } = useLiveTime(p.time);
  const k = cfg(layer);
  const s = live(k, (p.state ?? {}) as State, now, age);
  const n = k.views.length;
  const [idx, kk] = shownView(k, s, now);
  const cur = idx ?? (s.goto ? undefined : startIndex(k));
  const place = s.goto && !kk ? placeName(k, s.goto.name, s.goto.name, lang) : undefined;
  const name = place ?? (cur != null ? k.views[cur]?.name : undefined);
  // Held by the preview: its timed views wait for their Play.
  const held = !!p.time.held;
  const chained = !!s.auto;
  const on: AdvanceMode = chained ? (s.on ?? 'timed') : 'timed';
  const running = chained && on === 'timed' && !s.pausedAt && !held;
  const trigger = s.trigger || k.autoTrigger;
  const secs = s.interval || k.autoSecs || 6;
  const left = nextIn(s, now);
  // The Designer edits the template (what the take starts with); the Operator the air only.
  const setChained = (v: boolean) => (designer ? b.set('autoViews', v) : act(v ? 'advance' : 'stop', v ? { on: k.autoOn, seconds: k.autoSecs, trigger: k.autoTrigger } : undefined));
  const setOn = (v: AdvanceMode) => (designer ? b.set('autoOn', v) : act('advance', { on: v, seconds: secs, trigger }));
  const setSecs = (v: number) => (designer ? b.set('autoSecs', v) : act('auto', { seconds: v }));
  const setTrigger = (v: string) => (designer ? b.set('autoTrigger', v) : act('advance', { on: 'trigger', trigger: v }));
  const chainedShown = designer ? k.autoViews : chained;
  const onShown = designer ? k.autoOn : on;
  const secsShown = designer ? k.autoSecs : secs;
  const triggerShown = designer ? k.autoTrigger : trigger;
  useKeys({
    ArrowLeft: () => n > 0 && act('prev'),
    ArrowRight: () => n > 0 && act('next'),
    p: () => n > 1 && on === 'timed' && act(running ? 'pause' : chained ? 'resume' : 'auto', chained ? undefined : { seconds: secs }),
  });
  const status = n < 2 || !chained ? undefined : on === 'timed' ? (running && left !== undefined ? <Countdown left={left} total={s.interval || secs} /> : undefined) : on === 'continue' ? (waitsContinue(k, s, now) ? <WaitsContinue /> : undefined) : <WaitsTrigger name={trigger} />;
  const actions: Line2Item[] = [
    ...(n > 0
      ? !chained
        ? [
            { icon: SkipBack, label: tc('cc', 'Previous'), key: '←', onClick: () => act('prev') },
            { icon: SkipForward, label: tc('cc', 'Next'), key: '→', onClick: () => act('next') },
          ]
        : on === 'timed'
          ? [running ? { icon: Pause, label: tc('cc', 'Pause'), key: 'P', main: true, onClick: () => act('pause') } : { icon: Play, label: tc('cc', 'Play'), key: 'P', main: true, onClick: () => act(s.pausedAt ? 'resume' : 'auto', s.pausedAt ? undefined : { seconds: secs }) }]
          : on === 'continue'
            ? [{ icon: ChevronsRight, label: tc('cc', 'Continue'), key: 'F2', main: true, disabled: !waitsContinue(k, s, now), onClick: () => act('next') }]
            : [{ icon: Zap, label: trigger || tc('cc', 'Fire the trigger'), main: true, onClick: () => act('next') }]
      : []),
    n > 0 && chained && { icon: Square, label: tc('cc', 'Stop'), onClick: () => act('stop') },
    GAP,
    { icon: RotateCcw, label: tc('cc', 'Reset'), confirm: item ? t('{name} is on air: back to the template view?', { name: (layer as { name?: string }).name ?? t('Map') }) : undefined, onClick: () => act('reset') },
  ];
  const readout = n > 0 ? { k: t('View'), v: cur != null ? cur + 1 : '—', of: n, sub: name, after: <Dots at={cur ?? -1} count={n} /> } : name ? { v: name } : undefined;
  const zones = [{ value: '', label: '—' }, ...zoneOptions(k, lang)];
  const hl = designer ? k.hlZone : (s.highlight ?? '');
  return (
    <Centre name={(layer as { name?: string }).name} readout={readout} status={status} actions={actions}>
      <Toggle label={t('Chain the views')} value={chainedShown} onChange={setChained} />
      {chainedShown && <Seg label={t('Next view')} value={onShown} onChange={setOn} options={ADVANCE_MODES()} />}
      {chainedShown && onShown === 'timed' && <RangeNum label={t('Time per view')} value={secsShown} min={2} max={30} step={1} unit="s" onRelease onChange={setSecs} />}
      {chainedShown && onShown === 'trigger' && <Text label={tc('cc', 'Trigger name')} value={triggerShown} onChange={setTrigger} />}
      <Row label={tc('cc', 'Go to')}>
        <GotoField act={act} lang={lang} />
      </Row>
      <Row label={t('Put forward')} expose="hlZone" exposeValue={hl} exposeOptions={zones.map((z) => [z.value, z.label] as [string, string])}>
        <Dropdown value={zones.some((z) => z.value === hl) ? hl : ''} options={zones} onChange={(v) => (designer ? b.set('hlZone', v) : act('highlight', { zone: v }))} />
      </Row>
      {k.routes.length > 0 && (
        <Row label={t('Route')}>
          <Dropdown value="" options={[{ value: '', label: t('Draw a route') }, ...k.routes.map((r) => ({ value: keyOf(r), label: r.name }))]} onChange={(v) => v && act('route', { id: v })} />
        </Row>
      )}
      <Row label={t('Layers')} expose="opLayers" exposeValue={true}>
        <span style={PADS}>
          {(['markers', 'routes', 'zones', 'labels'] as const).map((l) => {
            const on = designer ? k.layers[l] : layerOn(k, s, l);
            return <Pad key={l} on={on} icon={LAYER_ICONS[l]} title={tc('cc', LAYER_NAMES[l])} onClick={() => (designer ? b.set(`layers.${l}`, !on) : act('layer', { name: l, on: !on }))} />;
          })}
        </span>
      </Row>
      {k.markers.length > 0 && (
        <Row label={tc('cc', 'Markers')} expose="opMarkers" exposeValue={true}>
          <span style={PADS}>
            {k.markers.map((m, i) => {
              const on = designer ? m.on : markerOn(s, m);
              return <Pad key={keyOf(m) || i} on={on} text={m.name || '—'} title={m.name} onClick={() => (designer ? b.set(`markers.${i}.on`, !on) : act('marker', { id: keyOf(m), on: !on }))} />;
            })}
          </span>
        </Row>
      )}
    </Centre>
  );
}

// --------------------------------------------------------------- overlay --

/** Drag pans the map, the wheel zooms (when “Drag on stage” is “Pans the map”). */
function PanOverlay({ layer, w, h, scale, edit }: { layer: Config; w: number; h: number; scale: number; edit: (fn: (l: Config) => void, key?: string) => void }) {
  const ref = useRef<{ x: number; y: number; v: View } | null>(null);
  const k = cfg(layer);
  const id = useRef(`pan-${Math.random()}`).current;
  const move = (dx: number, dy: number, v0: View) => {
    const [X0, Y0] = project(k.projection, v0.lon, v0.lat);
    const S = 256 * Math.pow(2, v0.zoom);
    const r = -v0.rot * D2R;
    const ux = dx * Math.cos(r) - dy * Math.sin(r);
    const uy = dx * Math.sin(r) + dy * Math.cos(r);
    let X = X0 - ux / S;
    if (wraps(k.projection)) X -= Math.floor(X);
    const [lon, lat] = unproject(k.projection, Math.min(1, Math.max(0, X)), Math.min(1, Math.max(0, Y0 - uy / S)));
    edit((l) => void (l.view = { ...cfg(l).view, lat: +Math.min(80, Math.max(-80, lat)).toFixed(4), lon: +normLon(lon).toFixed(4) }), id);
  };
  return (
    <div
      className="wk-catch"
      style={{ cursor: 'grab', width: w * scale, height: h * scale }}
      onPointerDown={(e) => {
        e.stopPropagation();
        (e.target as HTMLElement).setPointerCapture(e.pointerId);
        ref.current = { x: e.clientX, y: e.clientY, v: clampView(k.view) };
      }}
      onPointerMove={(e) => {
        const r = ref.current;
        if (r) move((e.clientX - r.x) / scale, (e.clientY - r.y) / scale, r.v);
      }}
      onPointerUp={() => (ref.current = null)}
      onWheel={(e) => {
        e.stopPropagation();
        edit((l) => {
          const c = cfg(l);
          l.view = { ...c.view, zoom: +Math.min(18, Math.max(1.5, c.view.zoom - e.deltaY * 0.002)).toFixed(2) };
        }, id);
      }}
    />
  );
}

// -------------------------------------------------------------- inspector --

const POS: [Corner, string][] = [
  ['tl', '↖'],
  ['tr', '↗'],
  ['bl', '↙'],
  ['br', '↘'],
];

export default defineWidget({
  id: 'map',
  type: 'map',
  label: 'Map',
  group: 'Sport & interaction',
  icon: MapIcon,
  create: () => ({ markers: [], routes: [], layers: { markers: false, routes: false, zones: false, labels: true }, props: { width: 1600, height: 840 } }),
  ownsLook: true,
  live: () => true,
  Render: ({ layer, w, h, data, time, state, designer, lang }) => {
    const p = new Pt();
    const k = cfg(layer);
    // The take's chained views and highlight, the timed views gone to so far.
    paint(p, k, shownState(k, (state ?? {}) as State, time.nowMs, time.ageSecs), data, time.nowMs, w, h, !!designer, lang ?? 'fr');
    return p.svg(w, h);
  },
  Overlay: ({ layer, w, h, scale, edit }) => (cfg(layer).dragPans ? <PanOverlay layer={layer} w={w} h={h} scale={scale} edit={edit as never} /> : null),
  Inspector: ({ layer, edit, state }) => {
    const k = cfg(layer);
    const lang = useApp((s) => s.project?.language) || 'fr';
    const [v] = viewAt(k, (state ?? {}) as State, Date.now(), num(layer.props.width ?? 1600));
    const [attText, attReq] = attribution(k, (state ?? {}) as State, v.zoom, lang);
    return (
      <>
        <Section id="map-view" title={t('View')}>
          <Num label={t('Latitude')} path="view.lat" value={k.view.lat} min={-85} max={85} step={0.01} precision={3} unit="°" />
          <Num label={t('Longitude')} path="view.lon" value={k.view.lon} min={-180} max={180} step={0.01} precision={3} unit="°" />
          <Num label={t('Zoom')} path="view.zoom" value={k.view.zoom} min={1.5} max={18} step={0.05} precision={2} />
          <Num label={t('Rotation')} path="view.rot" value={k.view.rot} min={-180} max={180} unit="°" />
          <Num label={t('Tilt')} path="view.tilt" value={k.view.tilt} min={0} max={60} unit="°" />
          <Toggle label={t('Drag to pan')} value={k.dragPans} onChange={(x) => edit((l) => void (l.dragPans = x || undefined))} />
          <Btns list={[{ label: t('+ Key view'), title: t('Save this view as a key view'), onClick: () => edit((l) => void (l.views = [...cfg(l).views, { name: `View ${cfg(l).views.length + 1}`, ...clampView(cfg(l).view) }])) }]} />
        </Section>
        <Section id="map-views" title={t('Key views')}>
          <Row expose="views" exposeValue={k.views}>
            <ListEditor
              path="views"
              value={k.views}
              fields={[
                { key: 'name', placeholder: t('Name') },
                { key: 'lat', num: true, w: 58 },
                { key: 'lon', num: true, w: 58 },
                { key: 'zoom', num: true, w: 44 },
                { key: 'rot', num: true, w: 40 },
                { key: 'tilt', num: true, w: 36 },
              ]}
              add={() => ({ name: `View ${k.views.length + 1}`, ...clampView(k.view) })}
              addLabel={t('Key view')}
              min={1}
              minMsg={t('At least one key view')}
            />
          </Row>
          <Btns list={k.views.map((kv) => ({ label: `◎ ${kv.name}`, title: t('Show in the Designer'), onClick: () => edit((l) => void (l.view = clampView(kv as View))) }))} label={t('Preview')} />
        </Section>
        <Section id="map-markers" title={t('Markers')}>
          <Toggle label={t('Markers')} path="layers.markers" value={true} />
          {k.layers.markers && (
            <>
              <Row expose="markers" exposeValue={k.markers}>
                <ListEditor
                  path="markers"
                  value={k.markers}
                  toggle="on"
                  fields={[
                    { key: 'name', placeholder: t('Label') },
                    { key: 'lat', w: 58, placeholder: 'lat' },
                    { key: 'lon', w: 58, placeholder: 'lon' },
                    {
                      key: 'icon',
                      w: 74,
                      options: [
                        ['pin', t('Pin')],
                        ['dot', t('Dot')],
                        ['star', t('Star')],
                        ['stadium', t('Stadium')],
                        ['plane', t('Plane')],
                      ],
                    },
                    { key: 'color', w: 34, color: true },
                  ]}
                  add={() => ({ on: true, name: 'New marker', lat: v.lat.toFixed(3), lon: v.lon.toFixed(3), icon: 'pin', color: '#ff3b5c' })}
                  addLabel={t('Marker')}
                />
              </Row>
              <Num label={t('Size')} path="mkSize" value={k.mkSize} min={1} max={400} unit="px" />
            </>
          )}
        </Section>
        <Section id="map-routes" title={t('Routes')}>
          <Toggle label={t('Routes')} path="layers.routes" value={true} />
          {k.layers.routes && (
            <>
              <Row expose="routes" exposeValue={k.routes}>
                <ListEditor
                  path="routes"
                  value={k.routes}
                  toggle="on"
                  fields={[
                    { key: 'name', placeholder: t('Name'), w: 96 },
                    { key: 'path', placeholder: 'Paris > Tokyo' },
                    { key: 'color', w: 34, color: true },
                    { key: 'dur', num: true, w: 36, placeholder: 's' },
                  ]}
                  add={() => ({ on: true, name: 'Route', path: 'Paris > New York', color: '#38bdf8', dur: 4 })}
                  addLabel={t('Route')}
                />
              </Row>
              <Seg
                label={t('Path')}
                path="curve"
                value={k.curve}
                options={[
                  ['gc', t('Great circle')],
                  ['straight', t('Straight')],
                ]}
              />
              <Num label={t('Width')} path="routeW" value={k.routeW} min={1} max={100} unit="px" />
              <Toggle label={t('Plane')} path="planeHead" value={true} />
            </>
          )}
        </Section>
        <Section id="map-zones" title={t('Zones')}>
          <Toggle label={t('Zones')} path="layers.zones" value={true} />
          {k.layers.zones && (
            <>
              <Seg
                label={t('Division')}
                path="zones.level"
                value={k.zones.level}
                options={[
                  ['regions', t('Regions')],
                  ['deps', t('Departements')],
                  ['countries', t('Countries')],
                ]}
              />
              <Btns
                label={t('Sample')}
                list={[
                  { label: t('Election'), onClick: () => edit((l) => void (l.zones = { ...cfg(l).zones, ...sample('election', cfg(l).zones.level) })) },
                  { label: t('Weather'), onClick: () => edit((l) => void (l.zones = { ...cfg(l).zones, ...sample('weather', cfg(l).zones.level) })) },
                ]}
              />
              <Seg
                label={t('Values')}
                path="zones.mode"
                value={k.zones.mode}
                options={[
                  ['category', t('Categories')],
                  ['scale', t('Scale')],
                ]}
              />
              <Row expose="zones.rows" exposeValue={k.zones.rows}>
                <GridEditor path="zones.rows" value={k.zones.rows} header csv />
              </Row>
              {k.zones.mode === 'category' ? (
                <ListEditor label={t('Categories')} path="zones.categories" value={k.zones.categories as Record<string, unknown>[]} fields={[{ key: 'name' }, { key: 'color', w: 34, color: true }]} add={() => ({ name: 'New', color: '#a594ff' })} addLabel={t('Category')} />
              ) : (
                <>
                  <ListEditor
                    label={t('Scale')}
                    path="zones.scale"
                    value={k.zones.scale as Record<string, unknown>[]}
                    fields={[
                      { key: 'v', num: true, w: 60 },
                      { key: 'color', w: 34, color: true },
                    ]}
                    add={() => ({ v: 0, color: '#ffffff' })}
                    addLabel={tc('scale', 'Stop')}
                  />
                  <Text label={t('Unit')} path="zones.unit" value={k.zones.unit} />
                </>
              )}
              <Num label={t('Opacity')} expose="zones.opacity" exposeValue={k.zones.opacity} value={Math.round(k.zones.opacity * 100)} min={0} max={100} unit="%" onChange={(x) => edit((l) => void (l.zones = { ...l.zones, opacity: x / 100 }), `mzo-${layer.id}`)} />
              <Toggle label={t('Legend')} path="zones.legend" value={true} />
              {k.zones.legend && (
                <>
                  <Text label={t('Title')} path="zones.title" value={k.zones.title} />
                  <Seg label={t('Position')} path="zones.legendPos" value={k.zones.legendPos} options={POS} />
                </>
              )}
            </>
          )}
        </Section>
        <Section id="map-base" title={t('Map')}>
          <Seg
            expose="style"
            exposeValue={k.style}
            label={t('Style')}
            value={k.style === 'mono' ? 'mono' : 'vector'}
            onChange={(sv) =>
              edit((l) => {
                l.style = sv;
                l.colors = { ...STYLES[sv], grid: cfg(l).colors.grid };
              })
            }
            options={[
              ['vector', t('Vector')],
              ['mono', 'Mono'],
            ]}
          />
          <Seg
            label={t('Projection')}
            path="projection"
            value={k.projection}
            options={[
              ['mercator', 'Mercator'],
              ['equalearth', 'Equal Earth'],
              ['equirect', t('Equirect.')],
            ]}
          />
          <ColorPick label={t('Water')} path="colors.water" value={k.colors.water} />
          <ColorPick label={t('Land')} path="colors.land" value={k.colors.land} />
          <Toggle label={t('Borders')} path="showBorders" value={true} />
          {k.showBorders && <ColorPick label={t('Color')} path="colors.border" value={k.colors.border} />}
          <Toggle label={t('Graticule')} path="graticule" value={false} />
          {k.graticule && <ColorPick label={t('Color')} value={k.colors.grid.trim() ? k.colors.grid : k.colors.border} onChange={(x) => edit((l) => void (l.colors = { ...cfg(l).colors, grid: x }), `mgc-${layer.id}`)} />}
          <Num label={t('Corners')} path="radius" value={k.radius} min={0} max={2000} unit="px" />
        </Section>
        <Section id="map-labels" title={t('Labels')}>
          <Toggle label={t('Labels')} path="layers.labels" value={true} />
          {k.layers.labels && (
            <>
              <Toggle label={t('Cities')} path="labels.cities" value={true} />
              <Toggle label={t('Countries')} path="labels.countries" value={true} />
              <Num label={t('Importance')} path="labels.minImp" value={k.labels.minImp} min={1} max={10} />
              <Num label={t('Size')} path="labels.size" value={k.labels.size} min={1} max={400} unit="px" />
              <Num label={t('Spacing')} path="labels.pad" value={k.labels.pad} min={0} max={200} unit="px" />
              <Toggle label={t('Show hidden')} value={k.labels.showHidden} onChange={(x) => edit((l) => void (l.labels = { ...l.labels, showHidden: x || undefined }))} />
            </>
          )}
          <ListEditor
            label={t('Renamed places')}
            path="names"
            value={k.names}
            fields={[
              { key: 'from', placeholder: t('Name on the map') },
              { key: 'to', placeholder: t('Shown name') },
            ]}
            add={() => ({ from: '', to: '' })}
            addLabel={t('Name')}
          />
        </Section>
        <Section id="map-att" title={t('Attribution')}>
          <Row label={t('Credit (auto)')}>
            <input className="wk-data" readOnly value={attText} />
          </Row>
          <Toggle label={t('Show')} path="attribution.show" value={attReq ? true : k.attribution.show} disabled={attReq ? t('Required by the licence') : undefined} />
          {(k.attribution.show || attReq) && (
            <>
              <Seg label={t('Position')} path="attribution.pos" value={k.attribution.pos} options={POS} />
              <Seg
                label={t('Style')}
                path="attribution.style"
                value={k.attribution.style}
                options={[
                  ['dark', t('Dark')],
                  ['light', tc('theme', 'Light')],
                  ['minimal', t('Text')],
                ]}
              />
            </>
          )}
        </Section>
        <Section id="map-trans" title={t('Transition')}>
          <Num label={t('Duration')} path="transition.dur" value={k.transition.dur} min={0} max={60} step={0.1} precision={1} unit="s" />
          <Seg
            label={t('Curve')}
            path="transition.ease"
            value={k.transition.ease}
            options={[
              ['linear', t('Linear')],
              ['inout', t('In-out')],
              ['out', t('Out')],
              ['in', t('In')],
              ['sine', t('Sine')],
            ]}
          />
          <Toggle label={t('Fly')} path="transition.fly" value={true} />
          {k.transition.fly && <Num label={t('Height')} expose="transition.flyAmt" exposeValue={k.transition.flyAmt} value={Math.round(k.transition.flyAmt * 100)} min={0} max={100} unit="%" onChange={(x) => edit((l) => void (l.transition = { ...l.transition, flyAmt: x / 100 }), `mfa-${layer.id}`)} />}
        </Section>
      </>
    );
  },
  Operator: (p) => <MapBar {...p} />,
  // Chained on Continue: F2 goes to the next view (until the last one).
  waits: (l, state, _data, item) => {
    const k = cfg(l);
    const now = Date.now();
    return waitsContinue(k, live(k, (state ?? {}) as State, now, (item.age ?? 0) / 50), now);
  },
  // Timed views: the Operator's preview holds them on the view shown until their Play.
  plays: (l) => {
    const k = cfg(l);
    return k.autoViews && k.autoOn === 'timed' && k.autoSecs > 0 && k.views.length > 1 && 'onAir';
  },
  warnings: (layer, { data, state, r }) => {
    const k = cfg(layer);
    const s = (state ?? {}) as State;
    const out: Warning[] = [];
    if (layerOn(k, s, 'markers')) for (const m of k.markers) if (markerOn(s, m) && !markerPos(m, data)) out.push({ level: 'error', op: true, text: t('Marker “{name}” ignored: invalid coordinates (lat {lat}, lon {lon}).', { name: m.name, lat: m.lat || '—', lon: m.lon || '—' }) });
    if (layerOn(k, s, 'routes'))
      for (const rt of k.routes) {
        if (!routeOn(s, rt)) continue;
        const [, bad] = routeLine(rt.path, k.curve);
        if (bad.length) out.push({ level: 'warn', op: true, text: t('Route “{name}”: waypoint not found — {places}. Use a known city or “lat,lon”.', { name: rt.name, places: bad.join(', ') }) });
      }
    const [, unmatched] = zoneFills(k.zones, data);
    if (unmatched.length) out.push({ level: 'warn', text: t('Zone data: no zone named {names}.', { names: unmatched.join(', ') }) });
    for (const kv of k.views) if (!Number.isFinite(+kv.lat) || !Number.isFinite(+kv.lon) || Math.abs(+kv.lat) > 90 || Math.abs(+kv.lon) > 180) out.push({ level: 'error', op: true, text: t('Key view “{name}” has invalid coordinates: the Operator cannot go to it.', { name: kv.name }) });
    const lang = useApp.getState().project?.language || 'fr';
    const [, req] = attribution(k, s, viewAt(k, s, Date.now(), 1600)[0].zoom, lang);
    if (!k.attribution.show && req) out.push({ level: 'info', text: t('Attribution kept on air: required by the IGN licence (zones).') });
    if (r) {
      const w = num(r.width);
      const h = num(r.height);
      const { hidden } = paint(new Pt(true), k, s, data, Date.now(), w, h, false, lang);
      if (hidden.length) out.push({ level: 'info', text: tn(hidden.length, '{n} label hidden by priority: {names}', '{n} labels hidden by priority: {names}', { names: `${hidden.slice(0, 6).join(', ')}${hidden.length > 6 ? '…' : ''}` }) });
    }
    const [v] = viewAt(k, s, Date.now(), 1600);
    if (k.projection !== 'mercator' && v.zoom > 8) out.push({ level: 'info', text: t('Equal Earth / equirectangular are meant for world maps; Web Mercator suits city views.') });
    return out;
  },
});

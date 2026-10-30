//! Map: drawing (same picture as `web/src/widgets/map.tsx`, line for line).
//! Everything is projected per point in layer pixels (view centre, zoom,
//! rotation, then the 3D tilt as a perspective), so both renderers agree.

use cg_core::wid::map::{
    attribution, hex_mix, place_name, project, route_line, route_progress, shown, view_at, world,
    zone_fills, AttStyle, BaseStyle, Config, Corner, Ll, MarkerIcon, Projection, State, View,
    ZoneMode,
};
use cg_core::wid::shape::Cmd;
use cg_core::{Color, LayerKind};

use super::scoreboard::paint::{col, ellipse, poly, rrect, Pt, T};
use super::Draw;

const PI: f64 = std::f64::consts::PI;
const D2R: f64 = PI / 180.0;
/// Colour of the place « Aller à… » went to.
const GOTO: &str = "#ffd200";

/// The camera: world (X, Y) → layer pixels.
pub(crate) struct Cam {
    pub cx: f64,
    pub cy: f64,
    pub s: f64,
    cr: f64,
    sr: f64,
    w: f64,
    h: f64,
    tilt: f64,
    d: f64,
}

impl Cam {
    pub fn new(p: Projection, v: &View, w: f64, h: f64) -> Cam {
        let (cx, cy) = project(p, v.lon, v.lat);
        Cam {
            cx,
            cy,
            s: 256.0 * 2f64.powf(v.zoom),
            cr: (v.rot * D2R).cos(),
            sr: (v.rot * D2R).sin(),
            w,
            h,
            tilt: if v.tilt > 0.5 { v.tilt * D2R } else { 0.0 },
            d: w * 1.1,
        }
    }
    /// A point of the (untilted) plane → layer pixels, with the tilt.
    pub fn tilted(&self, x: f64, y: f64) -> (f64, f64) {
        if self.tilt == 0.0 {
            return (x, y);
        }
        let oy = self.h * 0.67;
        let (xx, yy) = (x - self.w / 2.0, y - oy);
        let z = yy * self.tilt.sin();
        let k = self.d / (self.d - z).max(self.d * 0.1);
        (self.w / 2.0 + xx * k, oy + yy * self.tilt.cos() * k)
    }
    /// World → plane (before the tilt).
    pub fn flat(&self, x: f64, y: f64) -> (f64, f64) {
        let (dx, dy) = ((x - self.cx) * self.s, (y - self.cy) * self.s);
        (
            self.w / 2.0 + dx * self.cr - dy * self.sr,
            self.h / 2.0 + dx * self.sr + dy * self.cr,
        )
    }
    pub fn scr(&self, x: f64, y: f64) -> (f64, f64) {
        let (a, b) = self.flat(x, y);
        self.tilted(a, b)
    }
    /// Path commands of plane points; tilted, they are first cut to the
    /// plane (nothing goes behind the camera).
    pub fn emit(&self, pts: &[(f64, f64)], close: bool, out: &mut Vec<Cmd>) {
        let push = |out: &mut Vec<Cmd>, run: &[(f64, f64)], close: bool| {
            for (i, &(x, y)) in run.iter().enumerate() {
                let (x, y) = self.tilted(x, y);
                out.push(if i == 0 {
                    Cmd::Move(f(x), f(y))
                } else {
                    Cmd::Line(f(x), f(y))
                });
            }
            if close && !run.is_empty() {
                out.push(Cmd::Close);
            }
        };
        if self.tilt == 0.0 {
            return push(out, pts, close);
        }
        let (x0, y0, x1, y1) = (-0.35 * self.w, -0.35 * self.h, 1.35 * self.w, 1.35 * self.h);
        if close {
            let mut poly = pts.to_vec();
            // Sutherland–Hodgman against the 4 sides.
            for side in 0..4 {
                let inside = |p: &(f64, f64)| match side {
                    0 => p.0 >= x0,
                    1 => p.0 <= x1,
                    2 => p.1 >= y0,
                    _ => p.1 <= y1,
                };
                let cut = |a: &(f64, f64), b: &(f64, f64)| {
                    let t = match side {
                        0 => (x0 - a.0) / (b.0 - a.0),
                        1 => (x1 - a.0) / (b.0 - a.0),
                        2 => (y0 - a.1) / (b.1 - a.1),
                        _ => (y1 - a.1) / (b.1 - a.1),
                    };
                    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
                };
                let mut next = Vec::with_capacity(poly.len() + 4);
                for i in 0..poly.len() {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    if inside(&a) {
                        next.push(a);
                    }
                    if inside(&a) != inside(&b) {
                        next.push(cut(&a, &b));
                    }
                }
                poly = next;
                if poly.is_empty() {
                    return;
                }
            }
            return push(out, &poly, true);
        }
        // Polyline: Liang–Barsky per segment, joined into runs.
        let mut run: Vec<(f64, f64)> = Vec::new();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let (mut t0, mut t1) = (0.0f64, 1.0f64);
            let mut ok = true;
            for (pp, q) in [
                (-dx, a.0 - x0),
                (dx, x1 - a.0),
                (-dy, a.1 - y0),
                (dy, y1 - a.1),
            ] {
                if pp == 0.0 {
                    if q < 0.0 {
                        ok = false;
                    }
                } else {
                    let r = q / pp;
                    if pp < 0.0 {
                        t0 = t0.max(r);
                    } else {
                        t1 = t1.min(r);
                    }
                }
            }
            if !ok || t0 > t1 {
                push(out, &run, false);
                run.clear();
                continue;
            }
            let (p0, p1) = (
                (a.0 + dx * t0, a.1 + dy * t0),
                (a.0 + dx * t1, a.1 + dy * t1),
            );
            if run.is_empty() || t0 > 0.0 {
                push(out, &run, false);
                run.clear();
                run.push(p0);
            }
            run.push(p1);
            if t1 < 1.0 {
                push(out, &run, false);
                run.clear();
            }
        }
        push(out, &run, false);
    }
    /// Radius around the centre that covers the box (for culling world copies).
    pub fn reach(&self) -> f64 {
        self.w.hypot(self.h) * if self.tilt > 0.0 { 1.7 } else { 0.6 }
    }
    /// Offsets of the world copies to draw (wrapping projections).
    pub fn copies(&self, wrap: bool) -> Vec<f64> {
        if !wrap {
            return vec![0.0];
        }
        let r = self.reach();
        (-2..=2)
            .map(f64::from)
            .filter(|o| (o + 1.0 - self.cx) * self.s > -r && (o - self.cx) * self.s < r)
            .collect()
    }
}

fn f(v: f64) -> f32 {
    v as f32
}

/// Rings of lon/lat projected (all copies) as path commands.
fn rings(p: Projection, cam: &Cam, list: &[&[Ll]], close: bool, offs: &[f64]) -> Vec<Cmd> {
    let mut out = Vec::new();
    for &o in offs {
        for r in list {
            let pts: Vec<(f64, f64)> = r
                .iter()
                .map(|ll| {
                    let (x, y) = project(p, ll[0], ll[1]);
                    cam.flat(x + o, y)
                })
                .collect();
            cam.emit(&pts, close, &mut out);
        }
    }
    out
}

fn halo() -> Color {
    Color([5.0 / 255.0, 8.0 / 255.0, 20.0 / 255.0, 0.85])
}

/// A label candidate (layer pixels).
pub(crate) struct Cand {
    pub pri: f64,
    pub kind: u8, // 0 marker, 1 country, 2 continent, 3 city, 4 poi
    pub name: String,
    pub color: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub fs: f64,
    pub px: f64,
    pub py: f64,
}

/// Plane outline of a pin marker of size `sz` with its tip at (x, y).
fn pin(x: f32, y: f32, sz: f32) -> Vec<Cmd> {
    let r = sz / 2.0;
    let top = y - sz * 1.15;
    let c = top + r * 0.9;
    let k = 0.5523 * r;
    vec![
        Cmd::Move(x, y),
        Cmd::Cubic(x - r * 0.5, y - r * 0.8, x - r, top + r * 1.25, x - r, c),
        Cmd::Cubic(x - r, c - k, x - k, c - r, x, c - r),
        Cmd::Cubic(x + k, c - r, x + r, c - k, x + r, c),
        Cmd::Cubic(x + r, top + r * 1.25, x + r * 0.5, y - r * 0.8, x, y),
        Cmd::Close,
    ]
}

const PLANE: [(f32, f32); 16] = [
    (0.0, -9.0),
    (1.6, -2.5),
    (8.0, 1.0),
    (8.0, 3.0),
    (1.6, 1.2),
    (1.0, 6.0),
    (3.0, 7.5),
    (3.0, 9.0),
    (0.0, 8.0),
    (-3.0, 9.0),
    (-3.0, 7.5),
    (-1.0, 6.0),
    (-1.6, 1.2),
    (-8.0, 3.0),
    (-8.0, 1.0),
    (-1.6, -2.5),
];

/// The plane silhouette at (x, y), scaled, rotated (radians).
fn plane(x: f32, y: f32, s: f32, a: f32) -> Vec<Cmd> {
    let (c, si) = (a.cos(), a.sin());
    poly(
        &PLANE.map(|(px, py)| (x + (px * c - py * si) * s, y + (px * si + py * c) * s)),
        true,
    )
}

fn marker(p: &mut Pt, icon: MarkerIcon, color: &str, x: f32, y: f32, sz: f32, now: i64) {
    let r = sz / 2.0;
    match icon {
        MarkerIcon::Dot => {
            let ph = (now.rem_euclid(1600)) as f32 / 1600.0;
            let a = p.alpha;
            p.alpha = a * 0.85 * (1.0 - ph);
            p.circle(x, y, r * 0.55 * (1.0 + 2.2 * ph), color);
            p.alpha = a;
            let c = ellipse(x, y, r * 0.42, r * 0.42);
            p.fill(&c, color);
            p.stroke(&c, "#ffffff", 4.0, false);
        }
        MarkerIcon::Star => {
            let pts: Vec<(f32, f32)> = (0..10)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    let rr = if i % 2 == 1 { r * 0.42 } else { r };
                    (x + a.cos() * rr, y + a.sin() * rr)
                })
                .collect();
            let c = poly(&pts, true);
            p.fill(&c, color);
            p.stroke(&c, "#0b1020", 3.0, true);
        }
        MarkerIcon::Stadium => {
            let c = rrect(x - r, y - r, sz, sz, r * 0.35);
            p.fill(&c, color);
            p.stroke(&c, "#ffffff", 4.0, false);
            p.stroke(&ellipse(x, y, r * 0.6, r * 0.44), "#ffffff", 4.0, false);
            p.rect(
                x - r * 0.22,
                y - r * 0.14,
                r * 0.44,
                r * 0.28,
                0.0,
                "#ffffff",
            );
        }
        MarkerIcon::Plane => {
            let c = ellipse(x, y, r, r);
            p.fill(&c, color);
            p.stroke(&c, "#ffffff", 4.0, false);
            p.fill(&plane(x, y, r / 12.0, 0.0), "#ffffff");
        }
        MarkerIcon::Pin => {
            let c = pin(x, y, sz);
            p.fill(&c, color);
            p.stroke(&c, "#ffffff", 3.5, false);
            p.circle(x, y - sz * 1.15 + r * 0.9, r * 0.36, "#ffffff");
        }
    }
}

/// Places of a corner box (w × h) with margins.
fn corner(pos: Corner, w: f64, h: f64, bw: f64, bh: f64, mx: f64, my: f64) -> (f64, f64) {
    (
        if pos.left() { mx } else { w - mx - bw },
        if pos.top() { my } else { h - my - bh },
    )
}

/// Draws the map; returns the names of the labels hidden by priority.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint(
    p: &mut Pt,
    c: &Config,
    st: &State,
    data: &cg_core::anim::Data,
    now: i64,
    w: f64,
    h: f64,
    debug: bool,
    lang: &str,
) -> Vec<String> {
    let wd = world();
    let (v, _) = view_at(c, st, now, w);
    let pj = c.projection;
    let cam = Cam::new(pj, &v, w, h);
    let offs = cam.copies(pj.wraps());
    let cl = &c.colors;
    let radius = c.radius as f32;
    let (wf, hf) = (f(w), f(h));
    p.clip(&rrect(0.0, 0.0, wf, hf, radius));

    // Sky (tilted) and the plane.
    let tilted = v.tilt > 0.5;
    if tilted {
        let water = col(&cl.water);
        p.fill_lin(
            &rrect(0.0, 0.0, wf, hf, 0.0),
            (0.0, 0.0),
            (0.0, hf),
            &[
                (0.0, col("#0a0f22")),
                (0.28, col(&hex_mix(&cl.water, "#9fb4ff", 0.35))),
                (0.6, water),
            ],
        );
        let q = [(-0.35, -0.35), (1.35, -0.35), (1.35, 1.35), (-0.35, 1.35)].map(|(a, b)| {
            let (x, y) = cam.tilted(a * w, b * h);
            (f(x), f(y))
        });
        p.clip(&poly(&q, true));
    }
    let bg = if pj == Projection::Equalearth {
        "#05070f"
    } else {
        cl.water.as_str()
    };
    p.rect(-10.0, -10.0, wf + 20.0, hf + 20.0, 0.0, bg);
    if pj == Projection::Equalearth {
        let mut ring: Vec<Ll> = (0..=36).map(|i| [180.0, -90.0 + i as f64 * 5.0]).collect();
        ring.extend((0..=36).map(|i| [-180.0, 90.0 - i as f64 * 5.0]));
        p.fill(&rings(pj, &cam, &[&ring], true, &offs), &cl.water);
    }
    if c.graticule && !p.skip {
        let mut lines: Vec<Vec<Ll>> = Vec::new();
        for k in 0..=12 {
            let lo = -180.0 + k as f64 * 30.0;
            lines.push(
                (0..=36)
                    .map(|i| [lo, -90.0 + i as f64 * 5.0])
                    .filter(|ll| pj != Projection::Mercator || ll[1].abs() <= 85.0)
                    .collect(),
            );
        }
        for la in [-60.0, -30.0, 0.0, 30.0, 60.0] {
            lines.push((0..=72).map(|i| [-180.0 + i as f64 * 5.0, la]).collect());
        }
        let refs: Vec<&[Ll]> = lines.iter().map(|l| l.as_slice()).collect();
        let mut bc = col(if cl.grid.trim().is_empty() {
            &cl.border
        } else {
            &cl.grid
        });
        bc.0[3] *= 0.22;
        p.stroke_c(&rings(pj, &cam, &refs, false, &offs), bc, 1.0, false, None);
    }
    // Land (not even projected when the picture has it: `skip`).
    if !p.skip {
        let land: Vec<&[Ll]> = wd.land.iter().map(|r| r.as_slice()).collect();
        let land_cmds = rings(pj, &cam, &land, true, &offs);
        p.fill(&land_cmds, &cl.land);
        p.stroke(&land_cmds, &hex_mix(&cl.land, "#ffffff", 0.18), 1.2, true);
        let lakes: Vec<&[Ll]> = wd.lakes.iter().map(|r| r.as_slice()).collect();
        p.fill(&rings(pj, &cam, &lakes, true, &offs), &cl.water);
    }

    // Zones
    let zone_on = st.layer(c, "zones") && v.zoom <= 10.0;
    let (zf, _) = if zone_on {
        zone_fills(&c.zones, data)
    } else {
        Default::default()
    };
    if zone_on && !p.skip {
        let a0 = p.alpha;
        let hl = st.highlight.as_str();
        let pulse = ((now.rem_euclid(1100)) as f64 / 1100.0 * 2.0 * PI).sin() * 0.5 + 0.5;
        for r in c.zones.level.list() {
            let rr: Vec<&[Ll]> = r.rings.iter().map(|x| x.as_slice()).collect();
            let cmds = rings(pj, &cam, &rr, true, &offs);
            let is = hl == r.id;
            p.alpha = a0
                * c.zones.opacity.clamp(0.0, 1.0) as f32
                * if !hl.is_empty() && !is { 0.32 } else { 1.0 };
            let fill = zf
                .get(&r.id)
                .map(|z| z.color.as_str())
                .unwrap_or(cl.land.as_str());
            p.fill(&cmds, fill);
            if !is {
                p.stroke(&cmds, &cl.water, 1.6, true);
            }
        }
        if let Some(r) = c.zones.level.list().iter().find(|r| r.id == hl) {
            p.alpha = a0;
            let rr: Vec<&[Ll]> = r.rings.iter().map(|x| x.as_slice()).collect();
            let cmds = rings(pj, &cam, &rr, true, &offs);
            p.stroke(&cmds, "#ffffff", f(3.0 + 7.0 * pulse), true);
        }
        p.alpha = a0;
    }
    if c.show_borders && !p.skip {
        let b: Vec<&[Ll]> = wd.borders.iter().map(|r| r.as_slice()).collect();
        p.stroke_c(
            &rings(pj, &cam, &b, false, &offs),
            col(&cl.border),
            1.8,
            true,
            Some((7.0, 4.0)),
        );
    }
    // Paris detail (from zoom 9, when Paris is in view).
    let ppm = cam.s / (40075016.7 * (48.87 * D2R).cos());
    let (px, py) = {
        let (x, y) = project(pj, 2.3522, 48.8566);
        cam.scr(x, y)
    };
    let near = px > -30000.0 * ppm
        && py > -30000.0 * ppm
        && px < w + 30000.0 * ppm
        && py < h + 30000.0 * ppm;
    if v.zoom >= 9.0 && near && !p.skip {
        let dp = &wd.paris;
        let mw = |m: f64, min: f64| f((m * ppm).max(min));
        let one = |r: &[Ll], close: bool| rings(pj, &cam, &[r], close, &[0.0]);
        let urban = hex_mix(&cl.land, "#ffffff", 0.08);
        let park = if c.style == BaseStyle::Mono {
            hex_mix(&cl.land, "#ffffff", 0.12)
        } else {
            "#2f6b3a".into()
        };
        let stand = if c.style == BaseStyle::Mono {
            "#c7cad3"
        } else {
            "#dfe3ee"
        };
        p.fill(&one(&dp.urban, true), &urban);
        for pk in &dp.parks {
            p.fill(&one(pk, true), &park);
        }
        let streets: Vec<&[Ll]> = dp.streets.iter().map(|r| r.as_slice()).collect();
        p.stroke(
            &rings(pj, &cam, &streets, false, &[0.0]),
            &hex_mix(&urban, "#ffffff", 0.25),
            mw(12.0, 1.2),
            true,
        );
        p.stroke(&one(&dp.seine, false), &cl.water, mw(130.0, 3.0), true);
        p.stroke(&one(&dp.canal, false), &cl.water, mw(32.0, 1.5), false);
        {
            let mut m: Vec<&[Ll]> = vec![dp.peri.as_slice()];
            m.extend(dp.motorways.iter().map(|r| r.as_slice()));
            let mut cmds = rings(pj, &cam, &m[..1], true, &[0.0]);
            cmds.extend(rings(pj, &cam, &m[1..], false, &[0.0]));
            let w = mw(34.0, 4.0);
            let mut sh = col("#0b0f1d");
            sh.0[3] = 0.5;
            p.stroke_c(&cmds, sh, w + 3.0, true, None);
            p.stroke(&cmds, &hex_mix(&cl.land, "#ffffff", 0.3), w, true);
        }
        let s = one(&dp.stands, true);
        p.fill(&s, stand);
        let mut sh = col("#0b0f1d");
        sh.0[3] = 0.4;
        p.stroke_c(&s, sh, 2.0, false, None);
        p.fill(&one(&dp.bowl, true), "#8f97ad");
        p.fill(&one(&dp.pitch, true), "#2f9e44");
        let mut lines = one(&dp.lines, true);
        lines.extend(one(&dp.halfway, false));
        lines.extend(one(&dp.circle, true));
        p.stroke(&lines, "#ffffff", 1.6, false);
    }

    // Routes
    let in_view = |x: f64, y: f64, m: f64| x > -m && y > -m && x < w + m && y < h + m;
    let wrap_x = |x: f64| {
        if pj.wraps() {
            x - (x - cam.cx).round()
        } else {
            x
        }
    };
    let rw = c.route_w as f32;
    if st.layer(c, "routes") && !p.skip {
        for rt in &c.routes {
            if !st.route_on(rt) {
                continue;
            }
            let (line, _) = route_line(&rt.path, c.curve);
            if line.len() < 2 {
                continue;
            }
            let mut segs: Vec<Vec<(f64, f64)>> = vec![Vec::new()];
            let mut prev: Option<f64> = None;
            for &(la, lo) in &line {
                let nl = cg_core::wid::map::norm_lon(lo);
                let (mut x, y) = project(pj, nl, la);
                let copy = ((lo - nl) / 360.0).round();
                if pj.wraps() {
                    x += copy;
                } else if prev.is_some_and(|pc| pc != copy) {
                    segs.push(Vec::new());
                }
                prev = Some(copy);
                segs.last_mut().unwrap().push((x, y));
            }
            let total: f64 = segs
                .iter()
                .map(|sg| {
                    sg.windows(2)
                        .map(|q| (q[1].0 - q[0].0).hypot(q[1].1 - q[0].1))
                        .sum::<f64>()
                })
                .sum();
            let prog = route_progress(st, rt, now);
            let mut left = total * prog;
            let mut head: Option<((f64, f64), (f64, f64))> = None;
            let mut ds: Vec<Vec<(f64, f64)>> = Vec::new();
            for sg in &segs {
                if left <= 0.0 {
                    break;
                }
                let mut part = vec![sg[0]];
                for i in 1..sg.len() {
                    if left <= 0.0 {
                        break;
                    }
                    let l = (sg[i].0 - sg[i - 1].0).hypot(sg[i].1 - sg[i - 1].1);
                    if l <= left {
                        part.push(sg[i]);
                        left -= l;
                    } else {
                        let k = left / l;
                        part.push((
                            sg[i - 1].0 + (sg[i].0 - sg[i - 1].0) * k,
                            sg[i - 1].1 + (sg[i].1 - sg[i - 1].1) * k,
                        ));
                        left = 0.0;
                    }
                }
                if part.len() > 1 {
                    head = Some((part[part.len() - 2], part[part.len() - 1]));
                }
                ds.push(part);
            }
            let xs: Vec<f64> = segs.iter().flatten().map(|q| q.0).collect();
            let (minx, maxx) = (
                xs.iter().cloned().fold(f64::MAX, f64::min),
                xs.iter().cloned().fold(f64::MIN, f64::max),
            );
            let r = cam.reach();
            let route_offs: Vec<f64> = if pj.wraps() {
                (-2..=2)
                    .map(f64::from)
                    .filter(|o| (maxx + o - cam.cx) * cam.s > -r && (minx + o - cam.cx) * cam.s < r)
                    .collect()
            } else {
                vec![0.0]
            };
            for o in route_offs {
                let mut cmds = Vec::new();
                for part in &ds {
                    let pts: Vec<(f64, f64)> =
                        part.iter().map(|q| cam.flat(q.0 + o, q.1)).collect();
                    cam.emit(&pts, false, &mut cmds);
                }
                let mut sh = col("#0b0f1d");
                sh.0[3] = 0.55;
                p.stroke_c(&cmds, sh, rw + 6.0, true, None);
                p.stroke(&cmds, &rt.color, rw, true);
                let a0 = segs[0][0];
                let last = segs.last().unwrap();
                let a1 = last[last.len() - 1];
                let mut ends = vec![a0];
                if prog >= 1.0 {
                    ends.push(a1);
                }
                for q in ends {
                    let (x, y) = cam.scr(q.0 + o, q.1);
                    if in_view(x, y, 200.0) {
                        let e = ellipse(f(x), f(y), rw + 3.0, rw + 3.0);
                        p.fill(&e, "#ffffff");
                        p.stroke(&e, &rt.color, 4.0, false);
                    }
                }
                if let (Some((h0, h1)), true, true) = (head, prog < 1.0, c.plane_head) {
                    let (ax, ay) = cam.scr(h0.0 + o, h0.1);
                    let (bx, by) = cam.scr(h1.0 + o, h1.1);
                    if in_view(bx, by, 200.0) {
                        let ang = (by - ay).atan2(bx - ax) + PI / 2.0;
                        let e = ellipse(f(bx), f(by), 13.0 * 2.2, 13.0 * 2.2);
                        p.fill(&e, &rt.color);
                        p.stroke(&e, "#ffffff", 2.0 * 2.2, false);
                        p.fill(&plane(f(bx), f(by), 2.2, f(ang)), "#ffffff");
                    }
                }
            }
        }
    }

    // The second step: what is over the base map.
    p.skip = false;
    p.d.rd.sub_step(p.d.pm);
    if p.d.rd.base_only() {
        return Vec::new();
    }

    // Markers, then label candidates (markers first).
    let mut cands: Vec<Cand> = Vec::new();
    let sz = c.mk_size as f32;
    if st.layer(c, "markers") {
        for (i, m) in c.markers.iter().enumerate() {
            if !st.marker_on(m) {
                continue;
            }
            let Some((lat, lon)) = m.pos(data) else {
                continue;
            };
            let (x, y) = project(pj, lon, lat);
            let (x, y) = cam.scr(wrap_x(x), y);
            if !in_view(x, y, 200.0) {
                continue;
            }
            marker(p, m.icon, &m.color, f(x), f(y), sz, now);
            if !m.name.is_empty() {
                let fs = (c.mk_size * 0.62).round();
                let tw = p.tw(&m.name, f(fs), 800, 0.0) as f64 + 26.0;
                let ly = if m.icon == MarkerIcon::Pin {
                    y - c.mk_size * 0.75
                } else {
                    y
                };
                cands.push(Cand {
                    pri: 9.5 - i as f64 * 0.01,
                    kind: 0,
                    name: m.name.clone(),
                    color: m.color.clone(),
                    x: x + c.mk_size * 0.62 + 6.0,
                    y: ly - fs * 0.8,
                    w: tw,
                    h: fs * 1.6,
                    fs,
                    px: x,
                    py: y,
                });
            }
        }
    }
    // « Aller à… »: the place, a dot and its name.
    if let Some(g) = &st.goto {
        let (x, y) = project(pj, g.lon, g.lat);
        let (x, y) = cam.scr(wrap_x(x), y);
        if in_view(x, y, 200.0) {
            marker(p, MarkerIcon::Dot, GOTO, f(x), f(y), sz, now);
            let fs = (c.mk_size * 0.62).round();
            let name = place_name(c, &g.name, &g.name, lang).to_string();
            let tw = p.tw(&name, f(fs), 800, 0.0) as f64 + 26.0;
            cands.push(Cand {
                pri: 10.0,
                kind: 0,
                name,
                color: GOTO.into(),
                x: x + c.mk_size * 0.62 + 6.0,
                y: y - fs * 0.8,
                w: tw,
                h: fs * 1.6,
                fs,
                px: x,
                py: y,
            });
        }
    }
    let lb = &c.labels;
    if st.layer(c, "labels") {
        for pl in &wd.places {
            let (lat, lon, imp, minz, kind) = (pl.1, pl.2, pl.3, pl.4, pl.5.as_str());
            let name = &place_name(c, &pl.0, &pl.6, lang).to_string();
            let city = kind == "city" || kind == "poi";
            if (city && !lb.cities) || (!city && !lb.countries) {
                continue;
            }
            let maxz = match kind {
                "country" => 8.5,
                "continent" => 3.6,
                _ => 99.0,
            };
            if v.zoom < minz || v.zoom > maxz || imp < lb.min_imp {
                continue;
            }
            let (x, y) = project(pj, lon, lat);
            let (x, y) = cam.scr(wrap_x(x), y);
            if !in_view(x, y, 0.0) {
                continue;
            }
            let base = lb.size;
            if !city {
                let fs = (base * if kind == "continent" { 1.25 } else { 0.95 }).round();
                let tw = p.tw(name, f(fs), 800, f(fs * 0.14)) as f64;
                cands.push(Cand {
                    pri: imp + if kind == "continent" { 0.5 } else { -0.5 },
                    kind: if kind == "continent" { 2 } else { 1 },
                    name: name.clone(),
                    color: String::new(),
                    x: x - tw / 2.0,
                    y: y - fs / 2.0,
                    w: tw,
                    h: fs,
                    fs,
                    px: x,
                    py: y,
                });
            } else {
                let fs = (base * (0.78 + imp * 0.045)).round();
                let poi = kind == "poi";
                let tw = p.tw(name, f(fs), if poi { 700 } else { 800 }, 0.0) as f64;
                cands.push(Cand {
                    pri: imp,
                    kind: if poi { 4 } else { 3 },
                    name: name.clone(),
                    color: String::new(),
                    x: x + 12.0,
                    y: y - fs * 0.62,
                    w: tw,
                    h: fs * 1.15,
                    fs,
                    px: x,
                    py: y,
                });
            }
        }
    }
    cands.sort_by(|a, b| {
        b.pri
            .partial_cmp(&a.pri)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut placed: Vec<usize> = Vec::new();
    let mut hidden = Vec::new();
    let pad = lb.pad;
    for (ci, cd) in cands.iter().enumerate() {
        let hit = placed.iter().any(|&j| {
            let q = &cands[j];
            cd.x < q.x + q.w + pad
                && cd.x + cd.w + pad > q.x
                && cd.y < q.y + q.h + pad
                && cd.y + cd.h + pad > q.y
        });
        let (x, y, cw, ch, fs) = (f(cd.x), f(cd.y), f(cd.w), f(cd.h), f(cd.fs));
        if hit {
            hidden.push(cd.name.clone());
            if debug && lb.show_hidden {
                let b = rrect(x, y, cw, ch, 0.0);
                p.fill(&b, "#ff3b5c1f");
                p.stroke_c(&b, col("#ff3b5c"), 2.0, false, Some((6.0, 4.0)));
                let a = p.alpha;
                p.alpha = a * 0.8;
                p.text(
                    &cd.name,
                    x + 4.0,
                    y + ch / 2.0,
                    "#ff8da1",
                    T::new(fs * 0.8, 700),
                );
                p.alpha = a;
            }
            continue;
        }
        placed.push(ci);
        match cd.kind {
            0 => {
                p.rect(x, y, cw, ch, ch / 2.0, "#080a16e0");
                p.clip(&rrect(x, y, cw, ch, ch / 2.0));
                p.rect(x, y, 7.0, ch, 0.0, &cd.color);
                p.pop();
                p.text(&cd.name, x + 16.0, y + ch / 2.0, "#ffffff", T::new(fs, 800));
            }
            1 | 2 => {
                let mut fill = Color::WHITE;
                fill.0[3] = if cd.kind == 1 { 0.62 } else { 0.45 };
                let o = T::new(fs, 800).ls(fs * 0.14);
                if cd.kind == 1 {
                    p.text_halo(&cd.name, x, y + ch / 2.0, fill, halo(), 6.0, o);
                } else {
                    p.text_c(&cd.name, x, y + ch / 2.0, fill, o);
                }
            }
            _ => {
                let (px, py) = (f(cd.px), f(cd.py));
                if cd.kind == 4 {
                    let b = rrect(px - 6.0, py - 6.0, 12.0, 12.0, 3.0);
                    p.fill(&b, "#facc15");
                    p.stroke(&b, "#0b0f1d", 2.0, false);
                } else {
                    let e = ellipse(px, py, 6.0, 6.0);
                    p.fill(&e, "#ffffff");
                    p.stroke(&e, "#0b0f1d", 3.0, false);
                }
                p.text_halo(
                    &cd.name,
                    x,
                    y + ch / 2.0,
                    Color::WHITE,
                    halo(),
                    6.0,
                    T::new(fs, if cd.kind == 4 { 700 } else { 800 }),
                );
            }
        }
    }

    // Highlighted zone callout.
    if zone_on && !st.highlight.is_empty() {
        if let Some(r) = c.zones.level.list().iter().find(|r| r.id == st.highlight) {
            let (x, y) = project(pj, r.lon, r.lat);
            let (x, y) = cam.scr(wrap_x(x), y);
            let z = zf.get(&r.id);
            let no_data = if lang.starts_with("fr") {
                "Aucune donnée"
            } else {
                "No data"
            };
            let t2 = z
                .map(|z| z.text.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| no_data.into());
            let en = if r.en.is_empty() { &r.name } else { &r.en };
            let zname = place_name(c, en, &r.name, lang).to_string();
            let zc = z
                .map(|z| z.color.clone())
                .unwrap_or_else(|| "#ffffff".into());
            let bw = p.tw(&zname, 26.0, 800, 0.0).max(p.tw(&t2, 22.0, 700, 0.0)) + 36.0;
            let (bx, by) = (f(x) - bw / 2.0, f(y) - 120.0);
            let b = rrect(bx, by, bw, 84.0, 12.0);
            p.fill(&b, "#080a16eb");
            p.stroke(&b, &zc, 4.0, false);
            p.text(&zname, bx + 18.0, by + 26.6, "#ffffff", T::new(26.0, 800));
            let tc = if z.is_some() {
                hex_mix(&zc, "#ffffff", 0.35)
            } else {
                "#cbd5e1".into()
            };
            p.text(&t2, bx + 18.0, by + 58.0, &tc, T::new(22.0, 700));
            p.fill(
                &poly(
                    &[
                        (bx + bw / 2.0 - 12.0, by + 84.0),
                        (bx + bw / 2.0, by + 104.0),
                        (bx + bw / 2.0 + 12.0, by + 84.0),
                    ],
                    true,
                ),
                &zc,
            );
        }
    }
    if tilted {
        p.pop();
    }

    // Legend
    let att = &c.attribution;
    let (att_text, att_req) = attribution(c, st, v.zoom, lang);
    let att_on = att.show || att_req;
    if zone_on && c.zones.legend {
        let z = &c.zones;
        let title = z.title.to_uppercase();
        let tw = p.tw(&title, 17.0, 800, 17.0 * 0.06) as f64;
        let (bw, bh);
        match z.mode {
            ZoneMode::Category => {
                let mut mw = tw;
                for cat in &z.categories {
                    let n = zf.values().filter(|f| f.color == cat.color).count();
                    let rw = 32.0
                        + p.tw(&cat.name, 19.0, 700, 0.0) as f64
                        + 20.0
                        + p.tw(&n.to_string(), 19.0, 700, 0.0) as f64;
                    mw = mw.max(rw);
                }
                bw = (mw + 36.0).max(250.0);
                bh = 14.0 + 20.0 + 10.0 + z.categories.len() as f64 * 30.0 + 14.0;
            }
            ZoneMode::Scale => {
                bw = (tw + 36.0).max(250.0);
                bh = 14.0 + 20.0 + 10.0 + 18.0 + 8.0 + 20.0 + 14.0;
            }
        }
        let my = if z.legend_pos == att.pos && att_on {
            62.0
        } else {
            24.0
        };
        let (lx, ly) = corner(z.legend_pos, w, h, bw, bh, 24.0, my);
        let (lx, ly, bwf) = (f(lx), f(ly), f(bw));
        p.rect(lx, ly, bwf, f(bh), 12.0, "#080a16d1");
        p.text(
            &title,
            lx + 18.0,
            ly + 14.0 + 10.0,
            "#a594ff",
            T::new(17.0, 800).ls(17.0 * 0.06),
        );
        let mut yy = ly + 44.0;
        match z.mode {
            ZoneMode::Category => {
                for cat in &z.categories {
                    let n = zf.values().filter(|f| f.color == cat.color).count();
                    p.rect(lx + 18.0, yy + 4.0, 22.0, 22.0, 5.0, &cat.color);
                    p.text(
                        &cat.name,
                        lx + 50.0,
                        yy + 15.0,
                        "#ffffff",
                        T::new(19.0, 700),
                    );
                    p.text(
                        &n.to_string(),
                        lx + bwf - 18.0,
                        yy + 15.0,
                        "#c9cde0",
                        T::new(19.0, 700).end(),
                    );
                    yy += 30.0;
                }
            }
            ZoneMode::Scale => {
                let stops = &z.scale;
                if let (Some(a), Some(b)) = (stops.first(), stops.last()) {
                    let span = (b.v - a.v).max(1e-9);
                    let gs: Vec<(f32, Color)> = stops
                        .iter()
                        .map(|s| (f(((s.v - a.v) / span).clamp(0.0, 1.0)), col(&s.color)))
                        .collect();
                    let bar = rrect(lx + 18.0, yy, bwf - 36.0, 18.0, 9.0);
                    p.fill_lin(&bar, (lx + 18.0, 0.0), (lx + bwf - 18.0, 0.0), &gs);
                    let n = stops.len();
                    for (k, s) in stops.iter().enumerate() {
                        if n > 4 && k != 0 && k != n - 1 && k != n / 2 {
                            continue;
                        }
                        let x = lx + 18.0 + (bwf - 36.0) * f(((s.v - a.v) / span).clamp(0.0, 1.0));
                        let o = T::new(16.0, 700);
                        let o = if k == 0 {
                            o
                        } else if k == n - 1 {
                            o.end()
                        } else {
                            o.mid()
                        };
                        p.text(&format!("{}", s.v), x, yy + 36.0, "#ffffff", o);
                    }
                }
            }
        }
    }
    // Attribution of the data drawn (kept when the licence requires it).
    if att_on {
        let text = att_text.as_str();
        let tw = p.tw(text, 15.0, 600, 0.0) as f64 + 20.0;
        let (ax, ay) = corner(att.pos, w, h, tw, 26.0, 16.0, 14.0);
        let (ax, ay) = (f(ax), f(ay));
        match att.style {
            AttStyle::Dark => {
                p.rect(ax, ay, f(tw), 26.0, 6.0, "#00000099");
                p.text(text, ax + 10.0, ay + 13.0, "#e5e7eb", T::new(15.0, 600));
            }
            AttStyle::Light => {
                p.rect(ax, ay, f(tw), 26.0, 6.0, "#ffffffe0");
                p.text(text, ax + 10.0, ay + 13.0, "#111111", T::new(15.0, 600));
            }
            AttStyle::Minimal => {
                let mut hc = Color::BLACK;
                hc.0[3] = 0.7;
                p.text_halo(
                    text,
                    ax + 10.0,
                    ay + 13.0,
                    Color::WHITE,
                    hc,
                    3.0,
                    T::new(15.0, 600),
                );
            }
        }
    }
    p.pop();
    hidden
}

/// Still unless something of it is on its way: a flight to a view, the
/// pulse of a highlighted zone, of a dot marker or of the place gone to, a
/// route drawing (the only things `paint` reads the time for).
/// The map is drawn in two steps: what is under the markers (the base map,
/// zones, routes), then the markers, labels and legend. Each moves when
/// something of it is on its way.
pub(crate) fn motions(
    d: &mut Draw,
    st: &State,
) -> Option<(crate::cache::Motion, crate::cache::Motion)> {
    use crate::cache::Motion;
    let LayerKind::Map { x: c } = &d.layer.kind else {
        return None;
    };
    let now = d.time.now_ms;
    // The take's chained views and highlight, the timed views gone to so far.
    let st = &shown(c, st, now, d.time.age_secs);
    let (v, flying) = view_at(c, st, now, d.w as f64);
    let pulse = st.layer(c, "zones")
        && v.zoom <= 10.0
        && !st.highlight.is_empty()
        && c.zones.level.list().iter().any(|r| r.id == st.highlight);
    let dots = st.layer(c, "markers")
        && c.markers
            .iter()
            .any(|m| st.marker_on(m) && m.icon == MarkerIcon::Dot);
    let routes = st.layer(c, "routes")
        && c.routes
            .iter()
            .any(|r| st.route_on(r) && route_progress(st, r, now) < 1.0);
    // At rest *now* (a flight or a route may still be to come: not `Still`).
    let at_rest = |moving: bool| {
        if moving {
            Motion::Moving
        } else {
            Motion::of("at rest")
        }
    };
    let flying = flying.is_some();
    Some((
        at_rest(flying || pulse || routes),
        at_rest(flying || st.goto.is_some() || dots),
    ))
}

pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::Map { x: c } = &d.layer.kind else {
        return false;
    };
    let now = d.time.now_ms;
    let st = &shown(c, st, now, d.time.age_secs);
    let data = d.data;
    let (w, h) = (d.w as f64, d.h as f64);
    let lang = d.lang.to_string();
    // The base map is in the picture already (kept): only what is over it is drawn.
    let under = std::mem::take(&mut d.rd.steps.under);
    let mut p = Pt::new(d, false);
    p.skip = under;
    paint(&mut p, c, st, data, now, w, h, false, &lang);
    true
}

#[cfg(test)]
mod tests {
    use crate::Renderer;

    /// Pixels of a colour in the frame.
    fn count(f: &crate::Frame, hit: impl Fn(&[u8]) -> bool) -> usize {
        f.data.chunks(4).filter(|p| hit(p)).count()
    }

    #[test]
    fn departements_zones_and_the_place_gone_to() {
        let project = cg_core::sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "mp", "name": "T", "duration": 30.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "m", "name": "Map", "type": "map",
                "markers": [], "routes": [],
                "layers": {"markers": false, "routes": false, "zones": true, "labels": false},
                "zones": {"level": "deps", "mode": "category", "rows": [["Dep", "Party"], ["Finistère", "Red"], ["13", "Red"]],
                          "categories": [{"name": "Red", "color": "#ff0000"}], "opacity": 1.0, "legend": false},
                "props": {"x": 0.0, "y": 0.0, "width": 1600.0, "height": 840.0}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_template(&project, &t, "main", 10.0, &Default::default());
        // Two departements in red (Finistère and Bouches-du-Rhône), the rest in land colour.
        let red = count(&f, |p| p[0] > 230 && p[1] < 30 && p[2] < 30 && p[3] == 255);
        assert!(red > 2000, "{red}");
    }
}

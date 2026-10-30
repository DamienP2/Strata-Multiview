//! Layout of a chart: the picture as shapes and texts in layer pixels, so
//! the Rust and the web renderers draw exactly the same thing (mirror:
//! `web/src/widgets/chart-layout.tsx`, keep in sync line by line).

use super::*;
use crate::wid::shape::{rounded_polygon, Cmd};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PShape {
    pub d: Vec<Cmd>,
    pub fill: Option<String>,
    pub fill_opacity: f32,
    pub stroke: Option<String>,
    pub width: f32,
    pub dash: Option<[f32; 2]>,
    /// Round joins and caps (else miter / butt).
    pub round: bool,
    pub opacity: f32,
    /// Clip rectangle (x, y, w, h).
    pub clip: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PText {
    /// Anchor point on the baseline.
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub size: f32,
    pub weight: u16,
    pub anchor: Anchor,
    pub fill: String,
    pub opacity: f32,
    /// Outline behind the glyphs (5 px, round).
    pub halo: Option<String>,
    /// Turned −90° around (x, y).
    pub rotate: bool,
    pub clip: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Shape(PShape),
    Text(PText),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Out {
    pub prims: Vec<Prim>,
    /// Labels hidden to avoid overlaps.
    pub hidden: usize,
    /// Pie: negative values refused (name, value).
    pub pie_neg: Vec<(String, f32)>,
    pub pie_zero: bool,
    pub pie_others: usize,
    pub pie_missing: usize,
}

pub struct Input<'a> {
    pub cfg: &'a Config,
    pub st: &'a State,
    /// Resolved grid shown (`shown_grid`).
    pub grid: &'a [Vec<String>],
    pub w: f32,
    pub h: f32,
    /// Text size (the `fontSize` property).
    pub fs: f32,
    /// Time on air, ms.
    pub age_ms: f32,
    /// Wall clock, Unix ms.
    pub now: i64,
    /// The project's language (texts the chart writes: « Aucune donnée »…).
    pub lang: &'a str,
}

const D2R: f32 = std::f32::consts::PI / 180.0;
const MUTED: &str = "#b9b2dd";
const WHITE: &str = "#ffffff";

fn shape(d: Vec<Cmd>) -> PShape {
    PShape {
        d,
        fill: None,
        fill_opacity: 1.0,
        stroke: None,
        width: 0.0,
        dash: None,
        round: false,
        opacity: 1.0,
        clip: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn text(
    x: f32,
    y: f32,
    s: &str,
    size: f32,
    weight: u16,
    anchor: Anchor,
    fill: &str,
    op: f32,
) -> PText {
    PText {
        x,
        y,
        text: s.to_string(),
        size,
        weight,
        anchor,
        fill: fill.to_string(),
        opacity: op,
        halo: None,
        rotate: false,
        clip: None,
    }
}

fn rect_cmds(x: f32, y: f32, w: f32, h: f32, radii: &[f32]) -> Vec<Cmd> {
    rounded_polygon(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]], radii)
}

fn line_cmds(a: (f32, f32), b: (f32, f32)) -> Vec<Cmd> {
    vec![Cmd::Move(a.0, a.1), Cmd::Line(b.0, b.1)]
}

/// Point at angle `a` (degrees, 0 = top, clockwise).
fn pol(cx: f32, cy: f32, r: f32, a: f32) -> (f32, f32) {
    (cx + r * (a * D2R).sin(), cy - r * (a * D2R).cos())
}

/// Arc from a0 to a1 (cubic pieces of at most 90°), from the current point.
fn arc_to(out: &mut Vec<Cmd>, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
    let n = ((a1 - a0).abs() / 90.0).ceil().max(1.0) as usize;
    let th = (a1 - a0) / n as f32;
    let k = 4.0 / 3.0 * (th * D2R / 4.0).tan() * r;
    for i in 0..n {
        let a = a0 + th * i as f32;
        let b = a + th;
        let p0 = pol(cx, cy, r, a);
        let p1 = pol(cx, cy, r, b);
        let (ca, sa) = ((a * D2R).cos(), (a * D2R).sin());
        let (cb, sb) = ((b * D2R).cos(), (b * D2R).sin());
        out.push(Cmd::Cubic(
            p0.0 + k * ca,
            p0.1 + k * sa,
            p1.0 - k * cb,
            p1.1 - k * sb,
            p1.0,
            p1.1,
        ));
    }
}

fn circle_cmds(cx: f32, cy: f32, r: f32) -> Vec<Cmd> {
    let p = pol(cx, cy, r, 0.0);
    let mut d = vec![Cmd::Move(p.0, p.1)];
    arc_to(&mut d, cx, cy, r, 0.0, 360.0);
    d.push(Cmd::Close);
    d
}

/// A slice (or ring part) between angles a0 and a1.
fn slice_cmds(cx: f32, cy: f32, r0: f32, r1: f32, a0: f32, a1: f32) -> Vec<Cmd> {
    let mut d = Vec::new();
    if a1 - a0 >= 359.9 {
        d.extend(circle_cmds(cx, cy, r1));
        if r0 > 0.0 {
            // Inner circle the other way round: a hole with the nonzero rule.
            let p = pol(cx, cy, r0, 360.0);
            d.push(Cmd::Move(p.0, p.1));
            arc_to(&mut d, cx, cy, r0, 360.0, 0.0);
            d.push(Cmd::Close);
        }
        return d;
    }
    let p = pol(cx, cy, r1, a0);
    d.push(Cmd::Move(p.0, p.1));
    arc_to(&mut d, cx, cy, r1, a0, a1);
    if r0 > 0.0 {
        let q = pol(cx, cy, r0, a1);
        d.push(Cmd::Line(q.0, q.1));
        arc_to(&mut d, cx, cy, r0, a1, a0);
    } else {
        d.push(Cmd::Line(cx, cy));
    }
    d.push(Cmd::Close);
    d
}

#[derive(Clone, Copy)]
struct P {
    x: f32,
    y: f32,
    y0: f32,
    v: f32,
    ci: usize,
    interp: bool,
}

/// Line through the points (smooth: monotone-ish cubic).
fn path_line(pts: &[(f32, f32)], smooth: bool) -> Vec<Cmd> {
    let mut d = Vec::new();
    let Some(first) = pts.first() else {
        return d;
    };
    d.push(Cmd::Move(first.0, first.1));
    for i in 1..pts.len() {
        let p0 = if i >= 2 { pts[i - 2] } else { pts[i - 1] };
        let p1 = pts[i - 1];
        let p2 = pts[i];
        let p3 = pts.get(i + 1).copied().unwrap_or(p2);
        if !smooth {
            d.push(Cmd::Line(p2.0, p2.1));
            continue;
        }
        let (lo, hi) = (p1.1.min(p2.1), p1.1.max(p2.1));
        let c1y = clamp(p1.1 + (p2.1 - p0.1) / 6.0, lo, hi);
        let c2y = clamp(p2.1 - (p3.1 - p1.1) / 6.0, lo, hi);
        d.push(Cmd::Cubic(
            p1.0 + (p2.0 - p1.0) / 3.0,
            c1y,
            p2.0 - (p2.0 - p1.0) / 3.0,
            c2y,
            p2.0,
            p2.1,
        ));
    }
    d
}

fn marker(
    kind: Marker,
    x: f32,
    y: f32,
    r: f32,
    fill: &str,
    stroke: &str,
    hollow: bool,
) -> Option<PShape> {
    let (f, s) = if hollow {
        (stroke, fill)
    } else {
        (fill, stroke)
    };
    let d = match kind {
        Marker::None => return None,
        Marker::Square => rect_cmds(x - r, y - r, 2.0 * r, 2.0 * r, &[0.0]),
        Marker::Diamond => vec![
            Cmd::Move(x, y - r * 1.3),
            Cmd::Line(x + r * 1.3, y),
            Cmd::Line(x, y + r * 1.3),
            Cmd::Line(x - r * 1.3, y),
            Cmd::Close,
        ],
        Marker::Circle => circle_cmds(x, y, r),
    };
    let mut p = shape(d);
    p.fill = Some(f.to_string());
    p.stroke = Some(s.to_string());
    p.width = r * 0.55;
    Some(p)
}

/// A label waiting for the overlap pass.
#[derive(Clone)]
struct Lab {
    x: f32,
    y: f32,
    txt: String,
    fill: String,
    halo: Option<String>,
    prio: f32,
    size: Option<f32>,
    anchor: Anchor,
    op: f32,
    /// Grouped columns: text width, slot width, series, inside.
    w: Option<f32>,
    slot: f32,
    j: usize,
    inside: bool,
    /// Lines: column of the overlap pass (category index, or usize::MAX = line ends).
    col: Option<usize>,
    hide: bool,
}

impl Lab {
    fn new(x: f32, y: f32, txt: String, fill: &str, prio: f32, op: f32) -> Self {
        Self {
            x,
            y,
            txt,
            fill: fill.into(),
            halo: None,
            prio,
            size: None,
            anchor: Anchor::Middle,
            op,
            w: None,
            slot: 0.0,
            j: 0,
            inside: false,
            col: None,
            hide: false,
        }
    }
}

/// Overlap of labels in one column: offset → pushed apart; hide → dropped.
/// Returns the number hidden.
fn spread(list: &mut [&mut Lab], gap: f32, lo: f32, hi: f32, mode: Overlap) -> usize {
    if mode == Overlap::Hide {
        let mut idx: Vec<usize> = (0..list.len()).collect();
        idx.sort_by(|a, b| {
            list[*b]
                .prio
                .partial_cmp(&list[*a].prio)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut kept: Vec<f32> = Vec::new();
        let mut hidden = 0;
        for i in idx {
            if kept.iter().any(|k| (k - list[i].y).abs() < gap) {
                list[i].hide = true;
                hidden += 1;
            } else {
                kept.push(list[i].y);
            }
        }
        return hidden;
    }
    let mut idx: Vec<usize> = (0..list.len()).collect();
    idx.sort_by(|a, b| {
        list[*a]
            .y
            .partial_cmp(&list[*b].y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let n = idx.len();
    for k in 1..n {
        let prev = list[idx[k - 1]].y;
        if list[idx[k]].y < prev + gap {
            list[idx[k]].y = prev + gap;
        }
    }
    if n > 0 && list[idx[n - 1]].y > hi {
        list[idx[n - 1]].y = hi;
        for k in (0..n - 1).rev() {
            let next = list[idx[k + 1]].y;
            if list[idx[k]].y > next - gap {
                list[idx[k]].y = next - gap;
            }
        }
    }
    if n > 0 && list[idx[0]].y < lo {
        list[idx[0]].y = lo;
        for k in 1..n {
            let prev = list[idx[k - 1]].y;
            if list[idx[k]].y < prev + gap {
                list[idx[k]].y = prev + gap;
            }
        }
    }
    0
}

/// Timing of the elements: entrance from the time on air, reveal steps and
/// new categories from their timestamps.
struct Tm {
    now: i64,
    age: f32,
    dur: f32,
    stagger: f32,
    fx: Fx,
    mode: RevealMode,
    step: usize,
}

impl Tm {
    fn ep(&self, elapsed: f32) -> f32 {
        if elapsed == f32::INFINITY {
            return 1.0;
        }
        if self.dur <= 0.0 {
            return if elapsed >= 0.0 { 1.0 } else { 0.0 };
        }
        ease_out(clamp(elapsed / self.dur, 0.0, 1.0))
    }
    fn since(&self, t: i64) -> f32 {
        (self.now - t) as f32
    }
    /// (progress, hidden) of category `i`.
    fn cat(&self, st: &State, i: usize, name: &str) -> (f32, bool) {
        if self.mode == RevealMode::Categories && i >= self.step {
            return (0.0, true);
        }
        let mut el = if self.fx == Fx::Grow {
            self.age - i as f32 * self.stagger
        } else {
            f32::INFINITY
        };
        if self.mode == RevealMode::Categories {
            if let Some(t) = st.step_at.get(i).filter(|t| **t > 0) {
                el = self.since(*t);
            }
        }
        if is_new_cat(st, name) {
            el = el.min(self.since(st.at));
        }
        (self.ep(el), false)
    }
    fn ser(&self, st: &State, j: usize) -> (f32, bool) {
        if self.mode == RevealMode::Series && j >= self.step {
            return (0.0, true);
        }
        let mut el = if self.fx == Fx::Draw {
            self.age - j as f32 * self.stagger * 2.5
        } else {
            f32::INFINITY
        };
        if self.mode == RevealMode::Series {
            if let Some(t) = st.step_at.get(j).filter(|t| **t > 0) {
                el = self.since(*t);
            }
        }
        (self.ep(el), false)
    }
}

struct Ctx<'a> {
    c: &'a Config,
    st: &'a State,
    p: Parsed,
    tm: Tm,
    k: f32,
    kh: f32,
    fs: f32,
    hidden: usize,
    ink: Inks,
    lang: &'a str,
}

/// Colours of the texts: main, secondary (ticks, subtitle), category names.
struct Inks {
    main: String,
    muted: String,
    cat: String,
    na: String,
}

impl Inks {
    fn of(c: &Config) -> Inks {
        let t = c.text_color.trim();
        if t.is_empty() {
            return Inks {
                main: WHITE.into(),
                muted: MUTED.into(),
                cat: "#d6d0f5".into(),
                na: "#8f88b5".into(),
            };
        }
        let solid: String = t.chars().take(7).collect();
        Inks {
            main: t.into(),
            muted: format!("{solid}bf"),
            cat: t.into(),
            na: format!("{solid}99"),
        }
    }
}

impl Ctx<'_> {
    /// Highlight tween of a value depending on the highlight.
    fn hlv(&self, f: impl Fn(Hl) -> f32) -> f32 {
        lerp(f(self.st.prev_hl), f(self.st.hl), self.kh)
    }
    fn tv(&self, cat: &Cat, j: usize, target: Option<f32>) -> Option<f32> {
        tween_val(self.st, self.k, &cat.name, &self.p.ser[j], target)
    }
}

struct LegendItem {
    name: String,
    color: String,
    dim: bool,
    op: f32,
}

/// Lays the chart out.
pub fn layout(inp: &Input) -> Out {
    let c = inp.cfg;
    let st = inp.st;
    let p = parse(c, inp.grid);
    let mode = reveal_mode(c, st);
    let fxv = fx(c);
    let tm = Tm {
        now: inp.now,
        age: inp.age_ms,
        dur: if fxv == Fx::None {
            0.0
        } else {
            c.anim.dur * 1000.0
        },
        stagger: c.anim.stagger,
        fx: fxv,
        mode,
        step: st.step.max(1) as usize,
    };
    let mut cx = Ctx {
        c,
        st,
        p,
        tm,
        k: upd_k(c, st, inp.now),
        kh: hl_k(c, st, inp.now),
        fs: inp.fs,
        hidden: 0,
        ink: Inks::of(c),
        lang: inp.lang,
    };
    let (w, h, fs) = (inp.w, inp.h, inp.fs);
    let mut out = Out::default();
    if c.spark {
        out.prims = spark(&cx, w, h);
        return out;
    }
    if c.panel.on {
        let mut s = shape(rect_cmds(0.0, 0.0, w, h, &[28.0]));
        s.fill = Some(c.panel.color.clone());
        s.opacity = c.panel.opacity;
        out.prims.push(Prim::Shape(s));
    }
    let pad = (fs * 1.4).round();
    let (mut top, mut bottom, left, mut right) = (pad, h - pad, pad, w - pad);
    if c.title.pos != TitlePos::None && !c.title.text.is_empty() {
        let center = c.title.pos == TitlePos::Center;
        let tx = if center { w / 2.0 } else { left };
        let anc = if center {
            Anchor::Middle
        } else {
            Anchor::Start
        };
        out.prims.push(Prim::Text(text(
            tx,
            top + fs * 1.45,
            &c.title.text,
            fs * 1.75,
            900,
            anc,
            &cx.ink.main,
            1.0,
        )));
        top += fs * 2.1;
        if !c.title.sub.is_empty() {
            out.prims.push(Prim::Text(text(
                tx,
                top + fs * 0.85,
                &c.title.sub,
                fs,
                600,
                anc,
                &cx.ink.muted,
                1.0,
            )));
            top += fs * 1.5;
        }
        top += fs * 0.3;
    }
    let is_pie = c.chart_type == ChartType::Pie;
    let mut legend: Vec<LegendItem> = Vec::new();
    if !is_pie {
        for (j, name) in cx.p.ser.iter().enumerate() {
            let (_, hid) = cx.tm.ser(st, j);
            legend.push(LegendItem {
                name: name.clone(),
                color: series_style(c, j).0,
                dim: st.hl.kind == HlKind::Series && st.hl.idx != j as i32,
                op: if hid { 0.25 } else { 1.0 },
            });
        }
    }
    let lfs = fs * 0.85;
    let (mut ly, mut lx) = (0.0, 0.0);
    match c.legend {
        LegendPos::Top => {
            ly = top + lfs * 0.6;
            top += lfs * 2.0;
        }
        LegendPos::Bottom => {
            bottom -= lfs * 1.9;
            ly = bottom + lfs * 1.1;
        }
        LegendPos::Right => {
            let mut names: Vec<&str> = if is_pie {
                cx.p.cats.iter().map(|c| c.name.as_str()).collect()
            } else {
                cx.p.ser.iter().map(|s| s.as_str()).collect()
            };
            let others = word(&c.texts.others, "Others", "Autres", cx.lang);
            if is_pie {
                names.push(&others);
            }
            let widest = names.iter().map(|n| tw2(n, lfs)).fold(60.0f32, f32::max);
            let lw = (w * 0.3).min(widest + lfs * 2.6);
            right -= lw;
            lx = right + lfs * 1.2;
        }
        LegendPos::None => {}
    }
    let fade = if fxv == Fx::Fade {
        cx.tm.ep(cx.tm.age)
    } else {
        1.0
    };
    let frame = (left, right, top, bottom);
    let mut body = if is_pie {
        pie(&mut cx, frame, &mut legend, &mut out)
    } else {
        xy(&mut cx, frame)
    };
    for b in body.iter_mut() {
        match b {
            Prim::Shape(s) => s.opacity *= fade,
            Prim::Text(t) => t.opacity *= fade,
        }
    }
    out.prims.extend(body);
    if c.legend != LegendPos::None && !legend.is_empty() {
        let sw = lfs * 0.8;
        let main = cx.ink.main.clone();
        let item = |out: &mut Out, x: f32, y: f32, it: &LegendItem| {
            let o = if it.dim { 0.3 } else { it.op };
            let mut s = shape(rect_cmds(x, y - sw * 0.85, sw, sw, &[sw * 0.25]));
            s.fill = Some(it.color.clone());
            s.opacity = o;
            out.prims.push(Prim::Shape(s));
            out.prims.push(Prim::Text(text(
                x + sw + lfs * 0.5,
                y,
                &it.name,
                lfs,
                700,
                Anchor::Start,
                &main,
                o,
            )));
        };
        if c.legend == LegendPos::Right {
            let mut y = top + lfs;
            for it in &legend {
                item(&mut out, lx, y, it);
                y += lfs * 1.55;
            }
        } else {
            let widths: Vec<f32> = legend
                .iter()
                .map(|it| sw + lfs * 0.5 + tw2(&it.name, lfs) + lfs * 1.3)
                .collect();
            let tot: f32 = widths.iter().sum();
            let mut x = if c.title.pos == TitlePos::Center {
                (w - tot) / 2.0
            } else {
                left
            };
            for (i, it) in legend.iter().enumerate() {
                if x + widths[i] > w - pad + lfs {
                    continue;
                }
                item(&mut out, x, ly, it);
                x += widths[i];
            }
        }
    }
    out.hidden = cx.hidden;
    out
}

/// Sparkline: the first series filling the box, growing (columns) or drawn
/// (line, area) from the take.
fn spark(cx: &Ctx, w: f32, h: f32) -> Vec<Prim> {
    let c = cx.c;
    let vals: Vec<f32> =
        cx.p.cats
            .iter()
            .map(|k| k.vals.first().copied().flatten().unwrap_or(0.0))
            .collect();
    let n = vals.len();
    let mut g = Vec::new();
    if n == 0 {
        return g;
    }
    let (color, width, _) = series_style(c, 0);
    let lo = vals.iter().copied().fold(0.0f32, f32::min);
    let hi = vals.iter().copied().fold(lo, f32::max);
    let span = (hi - lo).max(1e-6);
    let e = cx.tm.ep(cx.tm.age);
    if c.chart_type == ChartType::Columns || c.chart_type == ChartType::Pie {
        let slot = w / n as f32;
        let bw = (slot * 0.7).max(1.0);
        let r = (bw / 2.0).min(3.0);
        for (i, v) in vals.iter().enumerate() {
            let bh = (v - lo) / span * h * e;
            if bh <= 0.0 {
                continue;
            }
            let x = i as f32 * slot + (slot - bw) / 2.0;
            let mut s = shape(rect_cmds(x, h - bh, bw, bh, &[r, r, 0.0, 0.0]));
            s.fill = Some(color.clone());
            g.push(Prim::Shape(s));
        }
        return g;
    }
    let lw = width.min(h / 4.0).max(1.0);
    let step = if n > 1 { w / (n - 1) as f32 } else { 0.0 };
    let pts: Vec<(f32, f32)> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let y = h - (v - lo) / span * h;
            (i as f32 * step, clamp(y, lw / 2.0, h - lw / 2.0))
        })
        .collect();
    let clip = Some([0.0, 0.0, w * e, h]);
    if c.chart_type == ChartType::Areas {
        let mut d = path_line(&pts, c.smooth);
        d.push(Cmd::Line(pts[n - 1].0, h));
        d.push(Cmd::Line(pts[0].0, h));
        d.push(Cmd::Close);
        let mut s = shape(d);
        s.fill = Some(color.clone());
        s.fill_opacity = c.area_opacity;
        s.clip = clip;
        g.push(Prim::Shape(s));
    }
    let mut s = shape(path_line(&pts, c.smooth));
    s.stroke = Some(color);
    s.width = lw;
    s.round = true;
    s.clip = clip;
    g.push(Prim::Shape(s));
    g
}

fn xy(cx: &mut Ctx, o: (f32, f32, f32, f32)) -> Vec<Prim> {
    let c = cx.c;
    let st = cx.st;
    let fs = cx.fs;
    let mut g: Vec<Prim> = Vec::new();
    let (ns, nc) = (cx.p.ser.len(), cx.p.cats.len());
    if ns == 0 || nc == 0 {
        g.push(Prim::Text(text(
            (o.0 + o.1) / 2.0,
            (o.2 + o.3) / 2.0,
            &word(&c.texts.no_data, "No data", "Aucune donnée", cx.lang),
            fs * 1.4,
            800,
            Anchor::Middle,
            &cx.ink.muted,
            1.0,
        )));
        return g;
    }
    let ct = c.chart_type;
    if ct == ChartType::Columns && c.orient == Orient::H {
        return hbars(cx, o);
    }
    let stacked = is_stacked(c);
    let pct = is_pct(c);
    let vals: Vec<Vec<Option<f32>>> = cx.p.cats.iter().map(|c| c.vals.clone()).collect();
    let tv = targets(c, &vals);
    // Shown (tweened) values, in the drawn space.
    let shown_raw: Vec<Vec<Option<f32>>> =
        cx.p.cats
            .iter()
            .map(|cat| (0..ns).map(|j| cx.tv(cat, j, cat.vals[j])).collect())
            .collect();
    let sv = targets(c, &shown_raw);
    let sc = scale(c, &tv);
    let k = cx.k;
    let pa = st.prev_axis.unwrap_or([sc.min, sc.max]);
    let (y_min, y_max) = (lerp(pa[0], sc.min, k), lerp(pa[1], sc.max, k));
    let mut ticks = Vec::new();
    let mut v = (sc.min / sc.step - 1e-6).ceil() * sc.step;
    while v <= sc.max + 1e-6 * sc.step.max(1.0) {
        ticks.push((v * 1e6).round() / 1e6);
        v += sc.step;
        if ticks.len() > 200 {
            break;
        }
    }
    let tick_label = |v: f32| {
        if pct {
            format!("{} %", js_num(v))
        } else {
            fmt(c, v)
        }
    };
    let tfs = fs * 0.8;
    let tick_w = ticks
        .iter()
        .map(|v| tw2(&tick_label(*v), tfs))
        .fold(f32::NEG_INFINITY, f32::max);
    let (mut left, mut bottom) = (o.0, o.3);
    if !c.axis.y_title.is_empty() {
        left += fs * 1.3;
    }
    left += tick_w + fs * 0.6;
    if !c.axis.x_title.is_empty() {
        bottom -= fs * 1.4;
    }
    bottom -= fs * 1.5;
    let top = o.2 + fs * 0.8;
    let right = o.1
        - if c.labels.name && ct != ChartType::Columns {
            (cx.p
                .ser
                .iter()
                .map(|s| tw2(s, fs * 0.8))
                .fold(f32::NEG_INFINITY, f32::max)
                + 20.0)
                .min(220.0)
        } else {
            10.0
        };
    let (pw, ph) = (right - left, bottom - top);
    let span = if y_max - y_min != 0.0 {
        y_max - y_min
    } else {
        1.0
    };
    let yy = |v: f32| top + ph - (v - y_min) / span * ph;
    let n_slots = if st.prev.is_empty() {
        nc as f32
    } else {
        lerp(st.prev_n, nc as f32, k)
    };
    let band = pw / n_slots;
    let slot_x: Vec<f32> =
        cx.p.cats
            .iter()
            .enumerate()
            .map(|(i, cat)| {
                let from = st
                    .prev
                    .iter()
                    .find(|q| q.name == cat.name)
                    .map(|q| q.slot)
                    .unwrap_or(i as f32);
                left + (lerp(from, i as f32, k) + 0.5) * (pw / n_slots)
            })
            .collect();
    // Grid and axis.
    for &v in &ticks {
        let y = yy(v);
        if y < top - 2.0 || y > bottom + 2.0 {
            continue;
        }
        let zero = v.abs() < 1e-6;
        if c.axis.grid || zero {
            let mut s = shape(line_cmds((left, y), (right, y)));
            s.stroke = Some(WHITE.into());
            s.opacity = if zero { 0.55 } else { 0.12 };
            s.width = if zero { 3.0 } else { 2.0 };
            g.push(Prim::Shape(s));
        }
        g.push(Prim::Text(text(
            left - fs * 0.5,
            y + tfs * 0.35,
            &tick_label(v),
            tfs,
            700,
            Anchor::End,
            &cx.ink.muted,
            1.0,
        )));
    }
    if !c.axis.y_title.is_empty() {
        let mut t = text(
            o.0 + fs * 0.7,
            top + ph / 2.0,
            &c.axis.y_title,
            fs * 0.8,
            700,
            Anchor::Middle,
            &cx.ink.muted,
            1.0,
        );
        t.rotate = true;
        g.push(Prim::Text(t));
    }
    if !c.axis.x_title.is_empty() {
        g.push(Prim::Text(text(
            left + pw / 2.0,
            o.3 - fs * 0.2,
            &c.axis.x_title,
            fs * 0.8,
            700,
            Anchor::Middle,
            &cx.ink.muted,
            1.0,
        )));
    }
    let cfs = fs * 0.82;
    let max_cat =
        cx.p.cats
            .iter()
            .map(|c| tw2(&c.name, cfs))
            .fold(f32::NEG_INFINITY, f32::max);
    let every = ((max_cat + 12.0) / band).ceil().max(1.0) as usize;
    let cat_e: Vec<(f32, bool)> =
        cx.p.cats
            .iter()
            .enumerate()
            .map(|(i, cat)| cx.tm.cat(st, i, &cat.name))
            .collect();
    let ser_e: Vec<(f32, bool)> = (0..ns).map(|j| cx.tm.ser(st, j)).collect();
    for (i, cat) in cx.p.cats.iter().enumerate() {
        let x = slot_x[i];
        let is_hl = st.hl.kind == HlKind::Category && st.hl.idx == cat.ri as i32;
        if c.axis.vgrid {
            let mut s = shape(line_cmds((x, top), (x, bottom)));
            s.stroke = Some(WHITE.into());
            s.opacity = 0.1;
            s.width = 2.0;
            g.push(Prim::Shape(s));
        }
        if is_hl {
            let mut s = shape(rect_cmds(
                x - band / 2.0,
                top - fs * 0.6,
                band,
                bottom - top + fs * 0.6,
                &[10.0],
            ));
            s.fill = Some(WHITE.into());
            s.opacity = 0.08;
            g.push(Prim::Shape(s));
        }
        let (e, hid) = cat_e[i];
        let ce = e.max(if hid { 0.0 } else { 0.0001 });
        if i % every == 0 || is_hl {
            g.push(Prim::Text(text(
                x,
                bottom + fs * 1.15,
                &cat.name,
                cfs,
                if is_hl { 900 } else { 700 },
                Anchor::Middle,
                if is_hl { &cx.ink.main } else { &cx.ink.cat },
                if hid { 0.3 } else { (0.3 + ce).min(1.0) },
            )));
        }
    }
    let clip = [left - 30.0, top - 4.0, pw + 60.0, ph + 8.0];
    let dim_of = |j: usize, ri: usize| {
        cx.hlv(|hl| {
            let on = hl.kind == HlKind::None
                || (hl.kind == HlKind::Series && hl.idx == j as i32)
                || (hl.kind == HlKind::Category && hl.idx == ri as i32);
            if on {
                1.0
            } else {
                0.22
            }
        })
    };
    let l = &c.labels;
    let lfs = fs * 0.78;
    let mut labels: Vec<Lab> = Vec::new();
    let mut data: Vec<Prim> = Vec::new();
    let mut hidden = 0usize;
    if ct == ChartType::Columns {
        let grouped = c.col_mode == ColMode::Grouped;
        let gw = band * 0.74;
        let bw = if grouped {
            ((gw - (ns as f32 - 1.0) * 6.0) / ns as f32).max(2.0)
        } else {
            gw
        };
        for (i, cat) in cx.p.cats.iter().enumerate() {
            let cxp = slot_x[i];
            let (mut pos, mut neg) = (0.0f32, 0.0f32);
            let tot: f32 = cat.vals.iter().flatten().filter(|v| **v > 0.0).sum();
            for j in 0..ns {
                let e = cat_e[i].0.min(ser_e[j].0);
                let (color, _, _) = series_style(c, j);
                let raw = tv[i][j];
                let op = dim_of(j, cat.ri);
                let x = if grouped {
                    cxp - gw / 2.0 + j as f32 * (bw + 6.0)
                } else {
                    cxp - gw / 2.0
                };
                let Some(raw) = raw else {
                    if e > 0.0 && grouped {
                        let mut lb = Lab::new(
                            x + bw / 2.0,
                            yy(0.0) - 10.0,
                            word("", "n/a", "n.d.", cx.lang),
                            &cx.ink.na,
                            -1.0,
                            1.0,
                        );
                        lb.size = Some(lfs * 0.8);
                        labels.push(lb);
                    }
                    continue;
                };
                let shown = sv[i][j].unwrap_or(raw);
                let v = shown * e;
                let (y0, y1) = if grouped {
                    (yy(0.0), yy(v))
                } else if v >= 0.0 {
                    let r = (yy(pos), yy(pos + v));
                    pos += v;
                    r
                } else {
                    let r = (yy(neg), yy(neg + v));
                    neg += v;
                    r
                };
                let yt = y0.min(y1);
                let hh = (y1 - y0).abs();
                if hh < 0.2 && e < 1.0 {
                    continue;
                }
                let rad = 8.0f32.min(bw / 4.0).min(hh / 2.0);
                let radii = if v >= 0.0 {
                    [rad, rad, 0.0, 0.0]
                } else {
                    [0.0, 0.0, rad, rad]
                };
                let mut s = shape(rect_cmds(x, yt, bw, hh, &radii));
                s.fill = Some(color.clone());
                s.opacity = op;
                if cat.interp[j] {
                    s.fill_opacity = 0.35;
                    s.stroke = Some(color.clone());
                    s.width = 3.0;
                    s.dash = Some([8.0, 6.0]);
                }
                data.push(Prim::Shape(s));
                if e < 0.6 {
                    continue;
                }
                let lp = if pct {
                    Some(raw)
                } else {
                    match cat.vals[j] {
                        Some(vj) if tot > 0.0 && vj > 0.0 => Some(vj / tot * 100.0),
                        _ => None,
                    }
                };
                let lv = if pct { cat.vals[j] } else { Some(shown) };
                let txt = label_text(c, l.name, lv, lp, &cx.p.ser[j]);
                if txt.is_empty() {
                    continue;
                }
                let tw = tw2(&txt, lfs);
                let fits = hh >= lfs * 1.35;
                let is_top = grouped
                    || (v >= 0.0 && (0..ns).rfind(|k| tv[i][*k].unwrap_or(0.0) > 0.0) == Some(j));
                let mut inside = l.pos == LabelPos::Inside || !grouped;
                if inside && !fits {
                    if l.overlap == Overlap::Offset && is_top {
                        inside = false;
                    } else {
                        hidden += 1;
                        continue;
                    }
                }
                if !inside && !grouped && !is_top {
                    hidden += 1;
                    continue;
                }
                if !inside && v >= 0.0 && yt - lfs * 1.2 < top - lfs * 0.6 {
                    inside = true;
                }
                let ylab = if inside {
                    if v >= 0.0 {
                        yt.max(top) + lfs * 1.05
                    } else {
                        yt + hh - lfs * 0.45
                    }
                } else if v >= 0.0 {
                    yt - lfs * 0.45
                } else {
                    yt + hh + lfs * 1.05
                };
                let yc = if !grouped && inside {
                    yt + hh / 2.0 + lfs * 0.35
                } else {
                    ylab
                };
                let mut lb = Lab::new(
                    x + bw / 2.0,
                    yc,
                    txt,
                    if inside { ink(&color) } else { &cx.ink.main },
                    v.abs(),
                    op,
                );
                lb.w = Some(tw);
                lb.slot = bw + 6.0;
                lb.j = j;
                lb.inside = inside;
                labels.push(lb);
            }
        }
        if grouped {
            for lb in labels.iter_mut() {
                let Some(w) = lb.w else { continue };
                if w <= lb.slot {
                    continue;
                }
                if l.overlap == Overlap::Offset {
                    if lb.j % 2 == 1 {
                        lb.y += if lb.inside { lfs * 1.15 } else { -lfs * 1.15 };
                    }
                    if w > lb.slot * 2.0 {
                        lb.hide = true;
                        hidden += 1;
                    }
                } else {
                    lb.hide = true;
                    hidden += 1;
                }
            }
        }
    } else {
        let mut base = vec![0.0f32; nc];
        let mut pts: Vec<(P, usize, f32)> = Vec::new();
        let panel = c.panel.color.clone();
        for j in 0..ns {
            let (color, width, mk) = series_style(c, j);
            let se = ser_e[j].0;
            let mut arr: Vec<Option<P>> = Vec::new();
            let mut prev: Option<P> = None;
            for (i, cat) in cx.p.cats.iter().enumerate() {
                let Some(v) = sv[i][j] else {
                    arr.push(None);
                    continue;
                };
                let x = slot_x[i];
                let b0 = if stacked { base[i] } else { 0.0 };
                let vv = if stacked { b0 + v } else { v };
                if stacked {
                    base[i] = vv;
                }
                let mut pt = P {
                    x,
                    y: yy(vv),
                    y0: yy(b0),
                    v,
                    ci: i,
                    interp: cat.interp[j],
                };
                let (ce, hid) = cat_e[i];
                if hid {
                    arr.push(None);
                    continue;
                }
                if ce < 1.0 {
                    if let Some(pr) = prev {
                        pt.x = lerp(pr.x, pt.x, ce);
                        pt.y = lerp(pr.y, pt.y, ce);
                        pt.y0 = lerp(pr.y0, pt.y0, ce);
                    } else {
                        pt.y = lerp(pt.y0, pt.y, ce);
                    }
                }
                arr.push(Some(pt));
                prev = Some(pt);
            }
            let mut runs: Vec<Vec<P>> = Vec::new();
            for pt in &arr {
                match pt {
                    Some(pt) => {
                        if runs.is_empty() {
                            runs.push(vec![*pt]);
                        } else {
                            runs.last_mut().unwrap().push(*pt);
                        }
                    }
                    None => {
                        if runs.last().is_some_and(|r| !r.is_empty()) {
                            runs.push(Vec::new());
                        }
                    }
                }
            }
            runs.retain(|r| !r.is_empty());
            let op = cx.hlv(|hl| {
                if hl.kind == HlKind::Series {
                    if hl.idx == j as i32 {
                        1.0
                    } else {
                        0.2
                    }
                } else {
                    1.0
                }
            });
            if se <= 0.0 {
                continue;
            }
            let sclip = if se < 1.0 {
                Some([left - 20.0, -2000.0, (pw + 40.0) * se, 5000.0])
            } else {
                None
            };
            let mut gs: Vec<PShape> = Vec::new();
            for rn in &runs {
                let tpts: Vec<(f32, f32)> = rn.iter().map(|p| (p.x, p.y)).collect();
                let topl = path_line(&tpts, c.smooth);
                if ct == ChartType::Areas {
                    let mut d = topl.clone();
                    if stacked {
                        let bpts: Vec<(f32, f32)> = rn.iter().rev().map(|p| (p.x, p.y0)).collect();
                        let mut bl = path_line(&bpts, c.smooth);
                        if let Some(Cmd::Move(x, y)) = bl.first().copied() {
                            bl[0] = Cmd::Line(x, y);
                        }
                        d.extend(bl);
                    } else {
                        d.push(Cmd::Line(rn[rn.len() - 1].x, yy(0.0)));
                        d.push(Cmd::Line(rn[0].x, yy(0.0)));
                    }
                    d.push(Cmd::Close);
                    let mut s = shape(d);
                    s.fill = Some(color.clone());
                    s.opacity = c.area_opacity;
                    gs.push(s);
                }
                if ct == ChartType::Lines || width > 0.0 {
                    let mut s = shape(topl);
                    s.stroke = Some(color.clone());
                    s.width = width;
                    s.round = true;
                    gs.push(s);
                }
            }
            for rn in &runs {
                for pt in rn {
                    let cat = &cx.p.cats[pt.ci];
                    let is_hlc = st.hl.kind == HlKind::Category && st.hl.idx == cat.ri as i32;
                    let mr = (width * 0.9).max(4.0) * if is_hlc { 1.5 } else { 1.0 };
                    if mk != Marker::None || pt.interp || is_hlc {
                        let kind = if mk == Marker::None {
                            Marker::Circle
                        } else {
                            mk
                        };
                        if let Some(s) = marker(kind, pt.x, pt.y, mr, &color, &panel, pt.interp) {
                            gs.push(s);
                        }
                    }
                    let (ce, hid) = cat_e[pt.ci];
                    if !hid && ce > 0.7 {
                        pts.push((*pt, j, op));
                    }
                }
            }
            for mut s in gs {
                s.opacity *= op;
                s.clip = sclip;
                data.push(Prim::Shape(s));
            }
            if l.name && !runs.is_empty() && se > 0.95 {
                let last = *runs.last().unwrap().last().unwrap();
                let mut lb = Lab::new(
                    last.x + fs * 0.7,
                    last.y + lfs * 0.35,
                    cx.p.ser[j].clone(),
                    &color,
                    1000.0 - j as f32,
                    op,
                );
                lb.anchor = Anchor::Start;
                lb.col = Some(usize::MAX);
                labels.push(lb);
            }
        }
        if l.value || l.pct {
            for (pt, j, op) in &pts {
                if ser_e[*j].0 < 0.95 {
                    continue;
                }
                let cat = &cx.p.cats[pt.ci];
                let tot: f32 = cat.vals.iter().flatten().filter(|v| **v > 0.0).sum();
                let pc = if tot > 0.0 && pt.v > 0.0 {
                    Some(pt.v / tot * 100.0)
                } else {
                    None
                };
                let txt = label_text(c, false, Some(pt.v), pc, "");
                if txt.is_empty() {
                    continue;
                }
                let y = if l.pos == LabelPos::Inside && ct == ChartType::Areas {
                    pt.y + lfs * 1.4
                } else {
                    pt.y - lfs * 0.75
                };
                let mut lb = Lab::new(pt.x, y, txt, &cx.ink.main, pt.v.abs(), *op);
                lb.halo = Some(c.panel.color.clone());
                lb.col = Some(pt.ci);
                labels.push(lb);
            }
        }
        // Vertical overlaps within each column (in order of first appearance).
        let mut cols: Vec<usize> = Vec::new();
        for lb in &labels {
            if let Some(cl) = lb.col {
                if !cols.contains(&cl) {
                    cols.push(cl);
                }
            }
        }
        for cl in cols {
            let mut list: Vec<&mut Lab> =
                labels.iter_mut().filter(|lb| lb.col == Some(cl)).collect();
            hidden += spread(
                &mut list,
                lfs * 1.1,
                top + lfs * 0.5,
                bottom - 2.0,
                l.overlap,
            );
        }
    }
    if c.ref_line.on {
        let y = yy(c.ref_line.value);
        if y >= top - 2.0 && y <= bottom + 2.0 {
            data.extend(ref_line(
                cx,
                (left, y),
                (right, y),
                (right, y - lfs * 0.45),
                Anchor::End,
            ));
        }
    }
    for lb in &labels {
        if lb.hide {
            continue;
        }
        let mut t = text(
            lb.x,
            lb.y,
            &lb.txt,
            lb.size.unwrap_or(lfs),
            800,
            lb.anchor,
            &lb.fill,
            lb.op,
        );
        t.halo = lb.halo.clone();
        data.push(Prim::Text(t));
    }
    for d in data.iter_mut() {
        match d {
            Prim::Shape(s) => {
                if s.clip.is_none() {
                    s.clip = Some(clip);
                } else if let Some(sc) = s.clip.as_mut() {
                    // Both clips: their intersection.
                    let x0 = sc[0].max(clip[0]);
                    let y0 = sc[1].max(clip[1]);
                    let x1 = (sc[0] + sc[2]).min(clip[0] + clip[2]);
                    let y1 = (sc[1] + sc[3]).min(clip[1] + clip[3]);
                    *sc = [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)];
                }
            }
            Prim::Text(t) => t.clip = Some(clip),
        }
    }
    cx.hidden += hidden;
    g.extend(data);
    g
}

/// The reference line from `a` to `b` (dashed, the text colour) and its label at `at`.
fn ref_line(cx: &Ctx, a: (f32, f32), b: (f32, f32), at: (f32, f32), anchor: Anchor) -> Vec<Prim> {
    let c = cx.c;
    let mut s = shape(line_cmds(a, b));
    s.stroke = Some(cx.ink.main.clone());
    s.width = 3.0;
    s.dash = Some([10.0, 8.0]);
    s.opacity = 0.85;
    let mut out = vec![Prim::Shape(s)];
    if !c.ref_line.label.is_empty() {
        out.push(Prim::Text(text(
            at.0,
            at.1,
            &c.ref_line.label,
            cx.fs * 0.78,
            800,
            anchor,
            &cx.ink.main,
            1.0,
        )));
    }
    out
}

/// Columns lying (`orient: h`): names on the left, values along the bottom,
/// bars growing to the right (mirror of `hbars` in chart-layout.tsx).
fn hbars(cx: &mut Ctx, o: (f32, f32, f32, f32)) -> Vec<Prim> {
    let c = cx.c;
    let st = cx.st;
    let fs = cx.fs;
    let mut g: Vec<Prim> = Vec::new();
    let (ns, nc) = (cx.p.ser.len(), cx.p.cats.len());
    let grouped = c.col_mode == ColMode::Grouped;
    let pct = is_pct(c);
    let vals: Vec<Vec<Option<f32>>> = cx.p.cats.iter().map(|c| c.vals.clone()).collect();
    let tv = targets(c, &vals);
    let shown_raw: Vec<Vec<Option<f32>>> =
        cx.p.cats
            .iter()
            .map(|cat| (0..ns).map(|j| cx.tv(cat, j, cat.vals[j])).collect())
            .collect();
    let sv = targets(c, &shown_raw);
    let sc = scale(c, &tv);
    let k = cx.k;
    let pa = st.prev_axis.unwrap_or([sc.min, sc.max]);
    let (x_min, x_max) = (lerp(pa[0], sc.min, k), lerp(pa[1], sc.max, k));
    let mut ticks = Vec::new();
    let mut v = (sc.min / sc.step - 1e-6).ceil() * sc.step;
    while v <= sc.max + 1e-6 * sc.step.max(1.0) {
        ticks.push((v * 1e6).round() / 1e6);
        v += sc.step;
        if ticks.len() > 200 {
            break;
        }
    }
    let tick_label = |v: f32| {
        if pct {
            format!("{} %", js_num(v))
        } else {
            fmt(c, v)
        }
    };
    let (tfs, cfs, lfs) = (fs * 0.8, fs * 0.82, fs * 0.78);
    let l = &c.labels;
    let name_w =
        cx.p.cats
            .iter()
            .map(|c| tw2(&c.name, cfs))
            .fold(0.0f32, f32::max)
            .min((o.1 - o.0) * 0.4);
    let mut left = o.0 + name_w + fs * 0.6;
    if !c.axis.y_title.is_empty() {
        left += fs * 1.3;
    }
    // Room for the values written after the bars, and for the last tick.
    let lab_w = if l.value || l.pct {
        tv.iter()
            .flatten()
            .flatten()
            .map(|v| tw2(&label_text(c, l.name, Some(*v), Some(100.0), ""), lfs))
            .fold(0.0f32, f32::max)
            + lfs * 0.6
    } else {
        0.0
    };
    let last_tick = ticks
        .last()
        .map(|v| tw2(&tick_label(*v), tfs) / 2.0)
        .unwrap_or(0.0);
    let right = o.1 - lab_w.max(last_tick).min((o.1 - o.0) * 0.3);
    let top = o.2 + fs * 0.4;
    let mut bottom = o.3 - fs * 1.5;
    if !c.axis.x_title.is_empty() {
        bottom -= fs * 1.4;
    }
    let (pw, ph) = (right - left, bottom - top);
    let span = if x_max - x_min != 0.0 {
        x_max - x_min
    } else {
        1.0
    };
    let xx = |v: f32| left + (v - x_min) / span * pw;
    let n_slots = if st.prev.is_empty() {
        nc as f32
    } else {
        lerp(st.prev_n, nc as f32, k)
    };
    let band = ph / n_slots;
    let slot_y: Vec<f32> =
        cx.p.cats
            .iter()
            .enumerate()
            .map(|(i, cat)| {
                let from = st
                    .prev
                    .iter()
                    .find(|q| q.name == cat.name)
                    .map(|q| q.slot)
                    .unwrap_or(i as f32);
                top + (lerp(from, i as f32, k) + 0.5) * band
            })
            .collect();
    for &v in &ticks {
        let x = xx(v);
        if x < left - 2.0 || x > right + 2.0 {
            continue;
        }
        let zero = v.abs() < 1e-6;
        if c.axis.grid || zero {
            let mut s = shape(line_cmds((x, top), (x, bottom)));
            s.stroke = Some(WHITE.into());
            s.opacity = if zero { 0.55 } else { 0.12 };
            s.width = if zero { 3.0 } else { 2.0 };
            g.push(Prim::Shape(s));
        }
        g.push(Prim::Text(text(
            x,
            bottom + fs * 1.15,
            &tick_label(v),
            tfs,
            700,
            Anchor::Middle,
            &cx.ink.muted,
            1.0,
        )));
    }
    if !c.axis.y_title.is_empty() {
        let mut t = text(
            o.0 + fs * 0.7,
            top + ph / 2.0,
            &c.axis.y_title,
            fs * 0.8,
            700,
            Anchor::Middle,
            &cx.ink.muted,
            1.0,
        );
        t.rotate = true;
        g.push(Prim::Text(t));
    }
    if !c.axis.x_title.is_empty() {
        g.push(Prim::Text(text(
            left + pw / 2.0,
            o.3 - fs * 0.2,
            &c.axis.x_title,
            fs * 0.8,
            700,
            Anchor::Middle,
            &cx.ink.muted,
            1.0,
        )));
    }
    let every = ((cfs * 1.3) / band).ceil().max(1.0) as usize;
    let cat_e: Vec<(f32, bool)> =
        cx.p.cats
            .iter()
            .enumerate()
            .map(|(i, cat)| cx.tm.cat(st, i, &cat.name))
            .collect();
    let ser_e: Vec<(f32, bool)> = (0..ns).map(|j| cx.tm.ser(st, j)).collect();
    for (i, cat) in cx.p.cats.iter().enumerate() {
        let y = slot_y[i];
        let is_hl = st.hl.kind == HlKind::Category && st.hl.idx == cat.ri as i32;
        if c.axis.vgrid {
            let mut s = shape(line_cmds((left, y), (right, y)));
            s.stroke = Some(WHITE.into());
            s.opacity = 0.1;
            s.width = 2.0;
            g.push(Prim::Shape(s));
        }
        if is_hl {
            let mut s = shape(rect_cmds(
                o.0,
                y - band / 2.0,
                right - o.0 + fs * 0.6,
                band,
                &[10.0],
            ));
            s.fill = Some(WHITE.into());
            s.opacity = 0.08;
            g.push(Prim::Shape(s));
        }
        let (e, hid) = cat_e[i];
        let ce = e.max(if hid { 0.0 } else { 0.0001 });
        if i % every == 0 || is_hl {
            g.push(Prim::Text(text(
                left - fs * 0.6,
                y + cfs * 0.35,
                &cat.name,
                cfs,
                if is_hl { 900 } else { 700 },
                Anchor::End,
                if is_hl { &cx.ink.main } else { &cx.ink.cat },
                if hid { 0.3 } else { (0.3 + ce).min(1.0) },
            )));
        }
    }
    let clip = [left - 4.0, top - 30.0, o.1 - left + 8.0, ph + 60.0];
    let dim_of = |j: usize, ri: usize| {
        cx.hlv(|hl| {
            let on = hl.kind == HlKind::None
                || (hl.kind == HlKind::Series && hl.idx == j as i32)
                || (hl.kind == HlKind::Category && hl.idx == ri as i32);
            if on {
                1.0
            } else {
                0.22
            }
        })
    };
    let mut data: Vec<Prim> = Vec::new();
    let mut labs: Vec<Prim> = Vec::new();
    let mut hidden = 0usize;
    let gw = band * 0.74;
    let bh = if grouped {
        ((gw - (ns as f32 - 1.0) * 6.0) / ns as f32).max(2.0)
    } else {
        gw
    };
    for (i, cat) in cx.p.cats.iter().enumerate() {
        let cy = slot_y[i];
        let (mut pos, mut neg) = (0.0f32, 0.0f32);
        let tot: f32 = cat.vals.iter().flatten().filter(|v| **v > 0.0).sum();
        for j in 0..ns {
            let e = cat_e[i].0.min(ser_e[j].0);
            let (color, _, _) = series_style(c, j);
            let op = dim_of(j, cat.ri);
            let y = if grouped {
                cy - gw / 2.0 + j as f32 * (bh + 6.0)
            } else {
                cy - gw / 2.0
            };
            let Some(raw) = tv[i][j] else {
                if e > 0.0 && grouped {
                    labs.push(Prim::Text(text(
                        xx(0.0) + 10.0,
                        y + bh / 2.0 + lfs * 0.3,
                        &word("", "n/a", "n.d.", cx.lang),
                        lfs * 0.8,
                        800,
                        Anchor::Start,
                        &cx.ink.na,
                        1.0,
                    )));
                }
                continue;
            };
            let shown = sv[i][j].unwrap_or(raw);
            let v = shown * e;
            let (x0, x1) = if grouped {
                (xx(0.0), xx(v))
            } else if v >= 0.0 {
                let r = (xx(pos), xx(pos + v));
                pos += v;
                r
            } else {
                let r = (xx(neg), xx(neg + v));
                neg += v;
                r
            };
            let xl = x0.min(x1);
            let ww = (x1 - x0).abs();
            if ww < 0.2 && e < 1.0 {
                continue;
            }
            let rad = 8.0f32.min(bh / 4.0).min(ww / 2.0);
            let radii = if v >= 0.0 {
                [0.0, rad, rad, 0.0]
            } else {
                [rad, 0.0, 0.0, rad]
            };
            let mut s = shape(rect_cmds(xl, y, ww, bh, &radii));
            s.fill = Some(color.clone());
            s.opacity = op;
            if cat.interp[j] {
                s.fill_opacity = 0.35;
                s.stroke = Some(color.clone());
                s.width = 3.0;
                s.dash = Some([8.0, 6.0]);
            }
            data.push(Prim::Shape(s));
            if e < 0.6 {
                continue;
            }
            let lp = if pct {
                Some(raw)
            } else {
                match cat.vals[j] {
                    Some(vj) if tot > 0.0 && vj > 0.0 => Some(vj / tot * 100.0),
                    _ => None,
                }
            };
            let lv = if pct { cat.vals[j] } else { Some(shown) };
            let txt = label_text(c, l.name, lv, lp, &cx.p.ser[j]);
            if txt.is_empty() {
                continue;
            }
            if grouped && bh < lfs * 0.9 {
                hidden += 1;
                continue;
            }
            let tw = tw2(&txt, lfs);
            let fits = ww >= tw + lfs * 0.8;
            let is_end = grouped
                || (v >= 0.0 && (0..ns).rfind(|q| tv[i][*q].unwrap_or(0.0) > 0.0) == Some(j));
            let mut inside = l.pos == LabelPos::Inside || !grouped;
            if inside && !fits {
                if l.overlap == Overlap::Offset && is_end {
                    inside = false;
                } else {
                    hidden += 1;
                    continue;
                }
            }
            if !inside && !grouped && !is_end {
                hidden += 1;
                continue;
            }
            let ty = y + bh / 2.0 + lfs * 0.35;
            let (tx, anchor) = if inside && !grouped {
                (xl + ww / 2.0, Anchor::Middle)
            } else if inside {
                if v >= 0.0 {
                    (x1 - lfs * 0.5, Anchor::End)
                } else {
                    (x1 + lfs * 0.5, Anchor::Start)
                }
            } else if v >= 0.0 {
                (x1 + lfs * 0.45, Anchor::Start)
            } else {
                (x1 - lfs * 0.45, Anchor::End)
            };
            labs.push(Prim::Text(text(
                tx,
                ty,
                &txt,
                lfs,
                800,
                anchor,
                if inside { ink(&color) } else { &cx.ink.main },
                op,
            )));
        }
    }
    if c.ref_line.on {
        let x = xx(c.ref_line.value);
        if x >= left - 2.0 && x <= right + 2.0 {
            data.extend(ref_line(
                cx,
                (x, top - fs * 0.3),
                (x, bottom),
                (x + lfs * 0.4, top + lfs * 0.6),
                Anchor::Start,
            ));
        }
    }
    data.extend(labs);
    for d in data.iter_mut() {
        match d {
            Prim::Shape(s) => s.clip = Some(clip),
            Prim::Text(t) => t.clip = Some(clip),
        }
    }
    cx.hidden += hidden;
    g.extend(data);
    g
}

/// Row index as the highlight names it ("Others" = -2).
fn ri_i(ri: usize) -> i32 {
    if ri == usize::MAX {
        -2
    } else {
        ri as i32
    }
}

struct Slice {
    name: String,
    /// Row index, or usize::MAX for "Others".
    ri: usize,
    i: usize,
    target: f32,
    color: String,
    group: Vec<usize>,
    e: f32,
    v: f32,
    hidden: bool,
}

struct PieLab {
    inside: bool,
    side: f32,
    x: f32,
    y: f32,
    ax: f32,
    ay: f32,
    ex: f32,
    txt: String,
    fill: String,
    op: f32,
    color: String,
}

fn pie(
    cx: &mut Ctx,
    o: (f32, f32, f32, f32),
    legend: &mut Vec<LegendItem>,
    out: &mut Out,
) -> Vec<Prim> {
    let c = cx.c;
    let st = cx.st;
    let fs = cx.fs;
    let pc = &c.pie;
    let mut g: Vec<Prim> = Vec::new();
    let ns = cx.p.ser.len();
    let j = (pc.series.max(0) as usize).min(ns.saturating_sub(1));
    let mut slices: Vec<Slice> = Vec::new();
    for (i, cat) in cx.p.cats.iter().enumerate() {
        // Values after the « Missing » rule (zero, interpolated, or a gap).
        let raw = cat.vals.get(j).copied().flatten();
        match raw {
            None => out.pie_missing += 1,
            Some(v) if v < 0.0 => out.pie_neg.push((cat.name.clone(), v)),
            Some(v) => slices.push(Slice {
                name: cat.name.clone(),
                ri: cat.ri,
                i,
                target: v,
                color: slice_color(c, i),
                group: Vec::new(),
                e: 0.0,
                v: 0.0,
                hidden: false,
            }),
        }
    }
    let total_t: f32 = slices.iter().map(|s| s.target).sum();
    // NaN counts as zero.
    out.pie_zero = total_t.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater);
    match pc.sort {
        PieSort::Desc => slices.sort_by(|a, b| {
            b.target
                .partial_cmp(&a.target)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        PieSort::Asc => slices.sort_by(|a, b| {
            a.target
                .partial_cmp(&b.target)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        PieSort::None => {}
    }
    if pc.others && total_t > 0.0 {
        let small: Vec<usize> = slices
            .iter()
            .enumerate()
            .filter(|(_, s)| s.target / total_t * 100.0 < pc.others_pct)
            .map(|(k, _)| k)
            .collect();
        if small.len() >= 2 {
            let group: Vec<usize> = small.iter().map(|k| slices[*k].i).collect();
            let sum: f32 = small.iter().map(|k| slices[*k].target).sum();
            let mut k = 0;
            slices.retain(|_| {
                let keep = !small.contains(&k);
                k += 1;
                keep
            });
            slices.push(Slice {
                name: word(&c.texts.others, "Others", "Autres", cx.lang),
                ri: usize::MAX,
                i: usize::MAX,
                target: sum,
                color: OTHERS.into(),
                group,
                e: 0.0,
                v: 0.0,
                hidden: false,
            });
            out.pie_others = small.len();
        }
    }
    let ser_name = cx.p.ser.get(j).cloned().unwrap_or_default();
    for s in slices.iter_mut() {
        if s.ri == usize::MAX {
            let mut e = 0.0f32;
            let mut all_hidden = true;
            let mut v = 0.0;
            for &i in &s.group {
                let cat = &cx.p.cats[i];
                let (ce, hid) = cx.tm.cat(st, i, &cat.name);
                e = e.max(ce);
                all_hidden &= hid;
                v += tween_val(st, cx.k, &cat.name, &ser_name, cat.vals[j]).unwrap_or(0.0);
            }
            s.e = e;
            s.hidden = all_hidden;
            s.v = v * e;
        } else {
            let cat = &cx.p.cats[s.i];
            let (ce, hid) = cx.tm.cat(st, s.i, &cat.name);
            s.e = ce;
            s.hidden = hid;
            s.v = tween_val(st, cx.k, &cat.name, &ser_name, Some(s.target)).unwrap_or(0.0) * ce;
        }
    }
    for s in &slices {
        legend.push(LegendItem {
            name: s.name.clone(),
            color: s.color.clone(),
            dim: st.hl.kind == HlKind::Category && st.hl.idx != ri_i(s.ri),
            op: if s.hidden { 0.25 } else { 1.0 },
        });
    }
    let total: f32 = slices.iter().map(|s| s.v).sum();
    let l = &c.labels;
    let outside = l.pos == LabelPos::Outside;
    let (cxp, cyp) = ((o.0 + o.1) / 2.0, (o.2 + o.3) / 2.0);
    let lfs = fs * 0.8;
    let max_lab = if outside {
        let widest = slices
            .iter()
            .map(|s| {
                tw2(
                    &(label_text(c, l.name, Some(s.target), Some(0.0), &s.name) + "00"),
                    lfs,
                )
            })
            .fold(60.0f32, f32::max);
        widest.min(260.0)
    } else {
        0.0
    };
    let r = (((o.3 - o.2) / 2.0 - if outside { lfs * 0.8 } else { 6.0 })
        .min((o.1 - o.0) / 2.0 - if outside { max_lab + fs * 2.2 } else { 6.0 })
        - pc.explode_dist * 0.6)
        .max(30.0);
    let r0 = r * clamp(pc.donut, 0.0, 95.0) / 100.0;
    if total_t <= 0.0 || total <= 1e-9 {
        let mut s = shape(circle_cmds(cxp, cyp, r - 3.0));
        s.stroke = Some(WHITE.into());
        s.opacity = 0.35;
        s.width = 6.0;
        s.dash = Some([18.0, 14.0]);
        g.push(Prim::Shape(s));
        if total_t <= 0.0 {
            g.push(Prim::Text(text(
                cxp,
                cyp + fs * 0.35,
                &word(&c.texts.no_data, "No data", "Aucune donnée", cx.lang),
                fs * 1.2,
                800,
                Anchor::Middle,
                &cx.ink.muted,
                1.0,
            )));
        }
        return g;
    }
    let sweep = if cx.tm.fx != Fx::Sweep {
        360.0
    } else {
        360.0 * cx.tm.ep(cx.tm.age)
    };
    let mut a = pc.start;
    let mut labs: Vec<PieLab> = Vec::new();
    let mut prio: Vec<f32> = Vec::new();
    let edge = if c.panel.on {
        c.panel.color.clone()
    } else {
        "#0b0d14".to_string()
    };
    for s in &slices {
        let span = s.v / total * 360.0;
        let (a0, a1) = (a, a + span);
        a = a1;
        if span <= 0.01 {
            continue;
        }
        let vis1 = a1.min(pc.start + sweep);
        let mid = (a0 + a1) / 2.0;
        let exploded = pc.explode >= 0
            && (s.ri == pc.explode as usize
                || s.group
                    .iter()
                    .any(|i| cx.p.cats[*i].ri == pc.explode as usize));
        let ri = ri_i(s.ri);
        let expl = if exploded { pc.explode_dist } else { 0.0 }
            + cx.hlv(|hl| {
                if hl.kind == HlKind::Category && hl.idx == ri {
                    fs * 0.9
                } else {
                    0.0
                }
            });
        let (dx, dy) = (expl * (mid * D2R).sin(), -expl * (mid * D2R).cos());
        let op = cx.hlv(|hl| {
            if hl.kind == HlKind::Category && hl.idx != ri {
                0.3
            } else {
                1.0
            }
        });
        if vis1 > a0 {
            let mut sh = shape(slice_cmds(cxp + dx, cyp + dy, r0, r, a0, vis1));
            sh.fill = Some(s.color.clone());
            sh.stroke = Some(edge.clone());
            sh.width = 3.0;
            sh.round = true;
            sh.opacity = op;
            g.push(Prim::Shape(sh));
        }
        let pctv = s.target / total_t * 100.0;
        let name = if s.ri == usize::MAX {
            format!("{} ({})", s.name, s.group.len())
        } else {
            s.name.clone()
        };
        let txt = label_text(c, l.name, Some(s.target), Some(pctv), &name);
        if txt.is_empty() || pc.start + sweep < mid || s.e < 0.6 {
            continue;
        }
        let small = pctv < l.min_size;
        if small && l.overlap == Overlap::Hide {
            cx.hidden += 1;
            continue;
        }
        let go_out = outside
            || (small && l.overlap == Overlap::Offset)
            || tw2(&txt, lfs) > (r - r0) * span * D2R * 0.9 + (r - r0) * 0.3;
        if !outside && go_out && l.overlap == Overlap::Hide {
            cx.hidden += 1;
            continue;
        }
        let (sx, sy) = ((mid * D2R).sin(), -(mid * D2R).cos());
        if !go_out {
            let rr = if r0 > 0.0 { (r0 + r) / 2.0 } else { r * 0.62 };
            labs.push(PieLab {
                inside: true,
                side: 0.0,
                x: cxp + dx + sx * rr,
                y: cyp + dy + sy * rr + lfs * 0.35,
                ax: 0.0,
                ay: 0.0,
                ex: 0.0,
                txt,
                fill: ink(&s.color).into(),
                op,
                color: s.color.clone(),
            });
        } else {
            labs.push(PieLab {
                inside: false,
                side: if sx >= 0.0 { 1.0 } else { -1.0 },
                x: 0.0,
                y: cyp + dy + sy * (r + fs * 0.9),
                ax: cxp + dx + sx * (r + 4.0),
                ay: cyp + dy + sy * (r + 4.0),
                ex: cxp + dx + sx * (r + fs * 0.9),
                txt,
                fill: cx.ink.main.clone(),
                op,
                color: s.color.clone(),
            });
        }
        prio.push(pctv);
    }
    // Outside labels: spread per side (offset) or drop the overlapping ones (hide).
    let mut hide = vec![false; labs.len()];
    for side in [1.0f32, -1.0] {
        let ids: Vec<usize> = (0..labs.len())
            .filter(|k| !labs[*k].inside && labs[*k].side == side)
            .collect();
        let mut tmp: Vec<Lab> = ids
            .iter()
            .map(|k| Lab::new(0.0, labs[*k].y, String::new(), WHITE, prio[*k], 1.0))
            .collect();
        let mut refs: Vec<&mut Lab> = tmp.iter_mut().collect();
        cx.hidden += spread(
            &mut refs,
            lfs * 1.25,
            o.2 + lfs * 0.6,
            o.3 - lfs * 0.2,
            l.overlap,
        );
        for (n, k) in ids.iter().enumerate() {
            labs[*k].y = tmp[n].y;
            hide[*k] = tmp[n].hide;
        }
    }
    for (k, lb) in labs.iter().enumerate() {
        if hide[k] {
            continue;
        }
        if lb.inside {
            g.push(Prim::Text(text(
                lb.x,
                lb.y,
                &lb.txt,
                lfs,
                800,
                Anchor::Middle,
                &lb.fill,
                lb.op,
            )));
            continue;
        }
        let tx = cxp + lb.side * (r + fs * 2.0);
        let ex = if lb.side > 0.0 {
            lb.ex.min(tx - 10.0)
        } else {
            lb.ex.max(tx + 10.0)
        };
        let mut sh = shape(vec![
            Cmd::Move(lb.ax, lb.ay),
            Cmd::Line(ex, lb.y),
            Cmd::Line(tx - lb.side * 6.0, lb.y),
        ]);
        sh.stroke = Some(lb.color.clone());
        sh.width = 3.0;
        sh.opacity = lb.op;
        g.push(Prim::Shape(sh));
        g.push(Prim::Text(text(
            tx,
            lb.y + lfs * 0.35,
            &lb.txt,
            lfs,
            800,
            if lb.side > 0.0 {
                Anchor::Start
            } else {
                Anchor::End
            },
            &lb.fill,
            lb.op,
        )));
    }
    if r0 > fs * 2.5 && pc.center_total {
        g.push(Prim::Text(text(
            cxp,
            cyp + fs * 0.2,
            &fmt(c, total_t),
            (r0 * 0.45).min(fs * 1.8),
            900,
            Anchor::Middle,
            &cx.ink.main,
            1.0,
        )));
        g.push(Prim::Text(text(
            cxp,
            cyp + fs * 1.3,
            &word(&c.texts.total, "Total", "Total", cx.lang),
            fs * 0.75,
            700,
            Anchor::Middle,
            &cx.ink.muted,
            1.0,
        )));
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(c: &Config, st: &State, grid: &[Vec<String>]) -> Out {
        layout(&Input {
            cfg: c,
            st,
            grid,
            w: 1460.0,
            h: 820.0,
            fs: 26.0,
            age_ms: 10_000.0,
            now: 1_000_000,
            lang: "en",
        })
    }

    fn g(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn pie_refuses_negatives_and_zero_total() {
        let c = Config {
            chart_type: ChartType::Pie,
            ..Default::default()
        };
        let o = run(
            &c,
            &State::default(),
            &g(&[&["P", "V"], &["A", "10"], &["B", "-5"], &["C", ""]]),
        );
        assert_eq!(o.pie_neg, vec![("B".to_string(), -5.0)]);
        assert_eq!(o.pie_missing, 1);
        assert!(!o.pie_zero);
        let o = run(
            &c,
            &State::default(),
            &g(&[&["P", "V"], &["A", "0"], &["B", "0"]]),
        );
        assert!(o.pie_zero);
        assert!(o
            .prims
            .iter()
            .any(|p| matches!(p, Prim::Text(t) if t.text == "No data")));
    }

    #[test]
    fn others_and_all_types_draw() {
        let mut c = Config {
            chart_type: ChartType::Pie,
            ..Default::default()
        };
        c.pie.others = true;
        let share = g(&[
            &["C", "S"],
            &["A", "50"],
            &["B", "40"],
            &["C", "2"],
            &["D", "1"],
            &["E", "7"],
        ]);
        assert_eq!(run(&c, &State::default(), &share).pie_others, 2);
        for t in [ChartType::Columns, ChartType::Areas, ChartType::Lines] {
            for m in [ColMode::Grouped, ColMode::Stacked, ColMode::Percent] {
                let c = Config {
                    chart_type: t,
                    col_mode: m,
                    area_mode: AreaMode::Stacked,
                    ..Default::default()
                };
                let o = run(&c, &State::default(), &sample());
                assert!(o.prims.len() > 20);
            }
        }
        // No data.
        let o = run(&Config::default(), &State::default(), &g(&[&["X"]]));
        assert!(o
            .prims
            .iter()
            .any(|p| matches!(p, Prim::Text(t) if t.text == "No data")));
    }

    #[test]
    fn negative_axis_goes_below_zero() {
        let c = Config::default();
        let o = run(
            &c,
            &State::default(),
            &g(&[&["Y", "Bal"], &["2021", "4"], &["2022", "-5"]]),
        );
        assert!(o
            .prims
            .iter()
            .any(|p| matches!(p, Prim::Text(t) if t.text.starts_with('−'))));
    }

    #[test]
    fn reveal_hides_series() {
        let c = Config {
            reveal: RevealMode::Series,
            ..Default::default()
        };
        let all = run(&Config::default(), &State::default(), &sample());
        let one = run(&c, &State::default(), &sample());
        assert!(one.prims.len() < all.prims.len());
    }
}

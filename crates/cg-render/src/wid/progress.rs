//! Progress bar: drawing (same picture as `web/src/widgets/progress.tsx`).
//!
//! Linear and segmented bars are laid out along their length (0 = start of
//! the track) and then mapped to the box; a ring is drawn with arcs, angles
//! clockwise from the top.

use ab_glyph::FontArc;
use cg_core::wid::progress::{
    eff_with, frac, CircleDir, Dir, LabelPos, Orient, SegPartial, Shape, State,
};
use cg_core::{Color, Prop, TextAlign, VerticalAlign};
use tiny_skia::{
    FillRule, GradientStop, LineCap, LinearGradient, Mask, Paint, Path, PathBuilder, Point, Shader,
    SpreadMode, Stroke, Transform,
};

use super::{rrect, Draw};

const LH: f32 = 1.2;
const BUBBLE: &str = "#0d0a1f";

fn sk(c: Color, o: f32) -> tiny_skia::Color {
    let [r, g, b, a] = c.0;
    tiny_skia::Color::from_rgba(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        (a * o).clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

/// The bar's paint: a step colour, the gradient (from → to), or the fill.
enum Bar {
    Solid(Color),
    Grad(Color, Color, (f32, f32), (f32, f32)),
}

impl Bar {
    fn paint(&self, o: f32) -> Paint<'static> {
        let mut p = Paint {
            anti_alias: true,
            ..Default::default()
        };
        match *self {
            Bar::Solid(c) => p.set_color(sk(c, o)),
            Bar::Grad(a, b, p0, p1) => {
                match LinearGradient::new(
                    Point::from_xy(p0.0, p0.1),
                    Point::from_xy(p1.0, p1.1),
                    vec![
                        GradientStop::new(0.0, sk(a, o)),
                        GradientStop::new(1.0, sk(b, o)),
                    ],
                    SpreadMode::Pad,
                    Transform::identity(),
                ) {
                    Some(s) => p.shader = s,
                    None => p.shader = Shader::SolidColor(sk(a, o)),
                }
            }
        }
        p
    }
}

fn path_mask(d: &Draw, p: &Path) -> Option<Mask> {
    match d.mask {
        Some(m) => Some(crate::mask::intersect(m, p, d.tf)),
        None => {
            let mut m = Mask::new(d.pm.width(), d.pm.height())?;
            m.fill_path(p, FillRule::Winding, true, d.tf);
            Some(m)
        }
    }
}

/// Text centred vertically on `y`, anchored at `x` (start / middle / end).
fn label(
    d: &mut Draw,
    f: &FontArc,
    s: &str,
    x: f32,
    y: f32,
    size: f32,
    anchor: TextAlign,
    c: Color,
) {
    let w = 20000.0;
    let bx = match anchor {
        TextAlign::Left => x,
        TextAlign::Center => x - w / 2.0,
        TextAlign::Right => x - w,
    };
    if let Some(p) = Draw::text_path(
        f,
        s,
        size,
        0.0,
        anchor,
        VerticalAlign::Middle,
        (bx, y - size * LH / 2.0, w, size * LH),
        LH,
    ) {
        d.fill(&p, c, FillRule::Winding, None);
    }
}

/// Rounded pill with the label, above (x, y); `ptr`: a pointer under it.
#[allow(clippy::too_many_arguments)]
fn bubble(
    d: &mut Draw,
    f: &FontArc,
    s: &str,
    tw: f32,
    x: f32,
    y: f32,
    fs: f32,
    ptr: bool,
    c: Color,
) {
    let (bw, bh) = (tw + fs * 0.9, fs * 1.5);
    let bx = (x - bw / 2.0).clamp(0.0, (d.w - bw).max(0.0));
    if let Some(p) = rrect(bx, y - bh, bw, bh, bh / 2.0) {
        d.fill(&p, Draw::hex(BUBBLE), FillRule::Winding, None);
        let st = Stroke {
            width: 1.0,
            ..Default::default()
        };
        d.stroke(
            &p,
            Color([c.0[0], c.0[1], c.0[2], c.0[3] * 0.25]),
            &st,
            None,
        );
    }
    if ptr {
        let mut pb = PathBuilder::new();
        pb.move_to(x - 9.0, y);
        pb.line_to(x + 9.0, y);
        pb.line_to(x, y + 10.0);
        pb.close();
        if let Some(p) = pb.finish() {
            d.fill(&p, Draw::hex(BUBBLE), FillRule::Winding, None);
        }
    }
    label(
        d,
        f,
        s,
        bx + bw / 2.0,
        y - bh / 2.0,
        fs,
        TextAlign::Center,
        c,
    );
}

/// An arc of radius r around (cx, cy) from a0 to a1 (degrees, clockwise from the top).
fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> Option<Path> {
    let pt = |a: f32| {
        let t = a.to_radians();
        (cx + r * t.sin(), cy - r * t.cos())
    };
    let n = ((a1 - a0).abs() / 90.0).ceil().max(1.0) as usize;
    let step = (a1 - a0) / n as f32;
    let mut pb = PathBuilder::new();
    let (x0, y0) = pt(a0);
    pb.move_to(x0, y0);
    for i in 0..n {
        let s = a0 + step * i as f32;
        let e = s + step;
        let k = 4.0 / 3.0 * (step.to_radians() / 4.0).tan() * r;
        let (sx, sy) = pt(s);
        let (ex, ey) = pt(e);
        // Tangent (clockwise from the top): (cos a, sin a).
        let (ts, te) = (s.to_radians(), e.to_radians());
        pb.cubic_to(
            sx + k * ts.cos(),
            sy + k * ts.sin(),
            ex - k * te.cos(),
            ey - k * te.sin(),
            ex,
            ey,
        );
    }
    pb.finish()
}

fn circle(cx: f32, cy: f32, r: f32) -> Option<Path> {
    PathBuilder::from_circle(cx, cy, r.max(0.01))
}

/// The goal marker's look: colour, label size and text.
struct Goal {
    color: Color,
    fs: f32,
    text: String,
}

/// A tick of the goal: 6 px wide from radius `r0` to `r1`, at angle `a` (degrees, clockwise from the top).
fn radial_tick(cx: f32, cy: f32, r0: f32, r1: f32, a: f32) -> Option<Path> {
    let t = a.to_radians();
    let (s, c) = (t.sin(), t.cos());
    // Along the radius (s, -c), across it (c, s).
    let p = |r: f32, k: f32| (cx + r * s + k * c, cy - r * c + k * s);
    let mut pb = PathBuilder::new();
    let pts = [p(r0, -3.0), p(r1, -3.0), p(r1, 3.0), p(r0, 3.0)];
    pb.move_to(pts[0].0, pts[0].1);
    for q in &pts[1..] {
        pb.line_to(q.0, q.1);
    }
    pb.close();
    pb.finish()
}

/// Drawn from the value shown (and the widget it follows).
pub(crate) fn motion(d: &mut Draw, st: &State) -> Option<crate::cache::Motion> {
    let e = eff_with(d.layer, d.data, d.follow)?;
    let v = e.shown(st, d.time.now_ms, d.time.age_secs);
    let follow = d.follow.map(|(a, b)| (a.to_bits(), b.map(f64::to_bits)));
    Some(crate::cache::Motion::of(&(v.to_bits(), follow)))
}

pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let Some(e) = eff_with(d.layer, d.data, d.follow) else {
        return false;
    };
    let c = e.c;
    let (w, h) = (d.w, d.h);
    let now = d.time.now_ms;
    let v = e.shown(st, now, d.time.age_secs);
    let (f, _, _) = frac(&e, v);
    let f = f as f32;
    let fs = c.label_size.max(1.0);
    let text = e.label(v, f as f64);
    let lc = Draw::hex(&c.label_color);
    let font = d.font("Inter", 800);
    let tw = match &font {
        Some(ft) if !text.is_empty() => Draw::text_width(ft, &text, fs, 0.0),
        _ => 0.0,
    };
    let sw = d.num(Prop::StrokeWidth).max(0.0);
    let sc = d.color(Prop::Stroke);
    let stroke = Stroke {
        width: sw,
        ..Default::default()
    };
    let track = Draw::hex(e.track);
    let fill = d.color(Prop::Fill);
    let thr = e.step_color(f as f64).map(Draw::hex);
    let bar = |p0: (f32, f32), p1: (f32, f32)| match thr {
        Some(t) => Bar::Solid(t),
        None if c.gradient => Bar::Grad(fill, Draw::hex(&c.color2), p0, p1),
        None => Bar::Solid(fill),
    };
    let op = d.opacity;
    // Goal marker (same in progress.tsx).
    let goal_f = e
        .goal
        .map(|g| frac(&e, g))
        .filter(|g| !g.1)
        .map(|g| g.0 as f32);
    let goal = Goal {
        color: Color::from_hex(&c.goal_color).unwrap_or(Color([1.0, 0.824, 0.0, 1.0])),
        fs: (c.label_size * 0.8).max(1.0),
        text: if goal_f.is_some() {
            cg_core::anim::interpolate_text(&c.goal_text, d.data)
        } else {
            String::new()
        },
    };

    if c.shape == Shape::Ring {
        let beside_w = if !text.is_empty() && c.label_pos == LabelPos::Beside {
            tw + 30.0
        } else {
            0.0
        };
        let (cx, cy) = ((w - beside_w) / 2.0, h / 2.0);
        let m = (w - beside_w).min(h);
        let t = c.thickness.unwrap_or(m * 0.12).min(m / 2.0 - 4.0).max(1.0);
        let end_room = if c.label_pos == LabelPos::End && !text.is_empty() {
            fs * 0.9
        } else {
            2.0
        };
        let r = (m / 2.0 - t / 2.0 - sw - end_room).max(4.0);
        let paint = bar((cx - r, cy - r), (cx + r, cy + r)).paint(op);
        let ring = Stroke {
            width: t,
            ..Default::default()
        };
        if let Some(p) = circle(cx, cy, r) {
            d.stroke(&p, track, &ring, None);
        }
        let span = f * 360.0;
        let s0 = match c.circle_dir {
            CircleDir::Cw => c.angle,
            CircleDir::Ccw => c.angle - span,
            CircleDir::Center => c.angle - span / 2.0,
        };
        if f >= 0.9995 {
            if let Some(p) = circle(cx, cy, r) {
                d.stroke_paint(&p, &paint, &ring, None);
            }
        } else if f > 0.0 {
            if let Some(p) = arc(cx, cy, r, s0, s0 + span.max(0.05)) {
                let st = Stroke {
                    width: t,
                    line_cap: if c.rounded {
                        LineCap::Round
                    } else {
                        LineCap::Butt
                    },
                    ..Default::default()
                };
                d.stroke_paint(&p, &paint, &st, None);
            }
        }
        if sw > 0.0 {
            for rr in [r + t / 2.0, (r - t / 2.0).max(0.0)] {
                if let Some(p) = circle(cx, cy, rr) {
                    d.stroke(&p, sc, &stroke, None);
                }
            }
        }
        if let Some(g) = goal_f {
            let ext = (t * 0.32).max(4.0);
            let angs = match c.circle_dir {
                CircleDir::Ccw => vec![c.angle - g * 360.0],
                CircleDir::Center => vec![c.angle + g * 180.0, c.angle - g * 180.0],
                CircleDir::Cw => vec![c.angle + g * 360.0],
            };
            let (r0, r1) = (r - t / 2.0 - ext, r + t / 2.0 + ext);
            for (i, a) in angs.into_iter().enumerate() {
                if let Some(p) = radial_tick(cx, cy, r0, r1, a) {
                    d.fill(&p, goal.color, FillRule::Winding, None);
                }
                if i == 0 && !goal.text.is_empty() {
                    if let Some(ft) = &font {
                        let rr = r1 + goal.fs * 0.9;
                        let ta = a.to_radians();
                        let (gx, gy) = (cx + rr * ta.sin(), cy - rr * ta.cos());
                        label(
                            d,
                            ft,
                            &goal.text,
                            gx,
                            gy,
                            goal.fs,
                            TextAlign::Center,
                            goal.color,
                        );
                    }
                }
            }
        }
        if let (Some(ft), false) = (font, text.is_empty()) {
            match c.label_pos {
                LabelPos::Inside => label(
                    d,
                    &ft,
                    &text,
                    cx,
                    cy,
                    (fs * 1.8).min((r - t / 2.0) * 0.62).max(1.0),
                    TextAlign::Center,
                    lc,
                ),
                LabelPos::Beside => label(
                    d,
                    &ft,
                    &text,
                    w - beside_w + 20.0,
                    cy,
                    fs,
                    TextAlign::Left,
                    lc,
                ),
                LabelPos::End => {
                    let end = match c.circle_dir {
                        CircleDir::Ccw => c.angle - f * 360.0,
                        CircleDir::Center => c.angle + f * 180.0,
                        CircleDir::Cw => c.angle + f * 360.0,
                    }
                    .to_radians();
                    let rr = r + t / 2.0 + fs * 0.9;
                    let (qx, qy) = (cx + rr * end.sin(), cy - rr * end.cos());
                    bubble(d, &ft, &text, tw, qx, qy + fs * 0.75, fs, false, lc);
                }
            }
        }
        return true;
    }

    // Linear and segmented: lengths along the track, then mapped.
    let horiz = c.orient == Orient::H;
    let l0 = if horiz { w } else { h };
    let cc = if horiz { h } else { w };
    let beside_l = if !text.is_empty() && c.label_pos == LabelPos::Beside {
        if horiz {
            tw + 24.0
        } else {
            fs * 1.6
        }
    } else {
        0.0
    };
    let l = (l0 - beside_l - sw).max(10.0);
    let t = match c.thickness {
        Some(t) => t.min(cc - 4.0).max(2.0),
        None => cc,
    };
    let c0 = cc / 2.0 - t / 2.0;
    let so = sw / 2.0;
    let rect = |p: f32, q: f32, rx: f32| -> Option<Path> {
        if horiz {
            rrect(so + p, c0, (q - p).max(0.0), t, rx)
        } else {
            rrect(c0, h - so - q, t, (q - p).max(0.0), rx)
        }
    };
    let grad = if horiz {
        bar((so, 0.0), (so + l, 0.0))
    } else {
        bar((0.0, h - so), (0.0, h - so - l))
    };
    let paint = grad.paint(op);
    let (mut p, mut q) = match c.dir {
        Dir::End => (l - f * l, l),
        Dir::Center => (l / 2.0 - f * l / 2.0, l / 2.0 + f * l / 2.0),
        Dir::Start => (0.0, f * l),
    };
    let radius = if c.rounded {
        t / 2.0
    } else {
        d.num(Prop::CornerRadius)
    };

    if c.shape == Shape::Segments {
        let mut n = c.segments.max(1);
        let g = c.seg_gap.max(0.0);
        while n > 1 && (l - g * (n - 1) as f32) / (n as f32) < 3.0 {
            n -= 1;
        }
        let sl = (l - g * (n - 1) as f32) / n as f32;
        let rx = if c.rounded { t.min(sl) * 0.3 } else { 0.0 };
        let mut u = f * n as f32;
        if c.seg_partial == SegPartial::Round {
            u = if c.dir == Dir::Center && n % 2 == 0 {
                2.0 * (u / 2.0).round()
            } else {
                u.round()
            };
        }
        let mid = (n as f32 - 1.0) / 2.0;
        let mut clip = PathBuilder::new();
        for i in 0..n {
            let a = i as f32 * (sl + g);
            if let Some(sp) = rect(a, a + sl, rx) {
                d.fill(&sp, track, FillRule::Winding, None);
                if sw > 0.0 {
                    d.stroke(&sp, sc, &stroke, None);
                }
                clip.push_path(&sp);
            }
        }
        let clip_mask = clip.finish().and_then(|cp| path_mask(d, &cp));
        for i in 0..n {
            let a = i as f32 * (sl + g);
            let fi_from = match c.dir {
                Dir::End => ((u - (n - 1 - i) as f32).clamp(0.0, 1.0), 1u8),
                Dir::Center => {
                    let r = (i as f32 - mid).abs();
                    let fi = if n % 2 == 1 {
                        if r == 0.0 {
                            u.clamp(0.0, 1.0)
                        } else {
                            ((u - 1.0) / 2.0 - (r - 1.0)).clamp(0.0, 1.0)
                        }
                    } else {
                        (u / 2.0 - r.floor()).clamp(0.0, 1.0)
                    };
                    (
                        fi,
                        if (i as f32) < mid {
                            1
                        } else if (i as f32) > mid {
                            0
                        } else {
                            2
                        },
                    )
                }
                Dir::Start => ((u - i as f32).clamp(0.0, 1.0), 0),
            };
            let (fi, from) = fi_from;
            if fi <= 0.0 {
                continue;
            }
            let len = sl * fi;
            let (fp, fq) = match from {
                0 => (a, a + len),
                1 => (a + sl - len, a + sl),
                _ => (a + (sl - len) / 2.0, a + (sl + len) / 2.0),
            };
            if let Some(fp) = rect(fp, fq, 0.0) {
                d.fill_paint(&fp, &paint, FillRule::Winding, clip_mask.as_ref());
            }
        }
    } else {
        // Rounded ends: a value above 0 is never shorter than the thickness
        // (a clean dot / pill, never a squashed sliver).
        if c.rounded && f > 0.0 && q - p < t {
            match c.dir {
                Dir::End => {
                    p = l - t;
                    q = l;
                }
                Dir::Center => {
                    p = l / 2.0 - t / 2.0;
                    q = l / 2.0 + t / 2.0;
                }
                Dir::Start => q = t,
            }
        }
        if let Some(tp) = rect(0.0, l, radius) {
            d.fill(&tp, track, FillRule::Winding, None);
            if sw > 0.0 {
                d.stroke(&tp, sc, &stroke, None);
            }
        }
        if f > 0.0 {
            if let Some(bp) = rect(p, q, radius) {
                d.fill_paint(&bp, &paint, FillRule::Winding, None);
            }
        }
    }
    if let Some(g) = goal_f {
        let ext = (t * 0.32).max(4.0);
        let ats = match c.dir {
            Dir::End => vec![l - g * l],
            Dir::Center => vec![l / 2.0 + g * l / 2.0, l / 2.0 - g * l / 2.0],
            Dir::Start => vec![g * l],
        };
        for (i, a) in ats.into_iter().enumerate() {
            let tick = if horiz {
                rrect(so + a - 3.0, c0 - ext, 6.0, t + 2.0 * ext, 3.0)
            } else {
                rrect(c0 - ext, h - so - a - 3.0, t + 2.0 * ext, 6.0, 3.0)
            };
            if let Some(p) = tick {
                d.fill(&p, goal.color, FillRule::Winding, None);
            }
            if i == 0 && !goal.text.is_empty() {
                if let Some(ft) = &font {
                    if horiz {
                        let y = c0 - ext - goal.fs * LH / 2.0 - 4.0;
                        label(
                            d,
                            ft,
                            &goal.text,
                            so + a,
                            y,
                            goal.fs,
                            TextAlign::Center,
                            goal.color,
                        );
                    } else {
                        label(
                            d,
                            ft,
                            &goal.text,
                            c0 + t + ext + 8.0,
                            h - so - a,
                            goal.fs,
                            TextAlign::Left,
                            goal.color,
                        );
                    }
                }
            }
        }
    }

    let Some(ft) = font else { return true };
    if text.is_empty() {
        return true;
    }
    let pos = |a: f32| if horiz { so + a } else { h - so - a };
    let len = q - p;
    if c.label_pos == LabelPos::Beside {
        if horiz {
            label(
                d,
                &ft,
                &text,
                l + so + 24.0,
                cc / 2.0,
                fs,
                TextAlign::Left,
                lc,
            );
        } else {
            label(
                d,
                &ft,
                &text,
                cc / 2.0,
                h - so - l - fs * 0.9,
                fs,
                TextAlign::Center,
                lc,
            );
        }
    } else if c.label_pos == LabelPos::End {
        let e = if c.dir == Dir::End { p } else { q };
        if horiz {
            bubble(d, &ft, &text, tw, pos(e), c0 - 12.0, fs, true, lc);
        } else {
            label(
                d,
                &ft,
                &text,
                c0 + t + 14.0,
                pos(e),
                fs,
                TextAlign::Left,
                lc,
            );
        }
    } else if horiz {
        let fits = c.shape != Shape::Segments && len > tw + fs;
        if c.shape == Shape::Segments {
            let (bw, bh) = (tw + fs * 1.1, (t + 8.0).min(fs * 1.6));
            if let Some(bp) = rrect(
                so + l / 2.0 - bw / 2.0,
                cc / 2.0 - bh / 2.0,
                bw,
                bh,
                bh / 2.0,
            ) {
                d.fill(
                    &bp,
                    Color([0.051, 0.039, 0.122, 0.92]),
                    FillRule::Winding,
                    None,
                );
            }
            label(
                d,
                &ft,
                &text,
                so + l / 2.0,
                cc / 2.0,
                fs,
                TextAlign::Center,
                lc,
            );
        } else if c.dir == Dir::Center {
            label(
                d,
                &ft,
                &text,
                so + l / 2.0,
                cc / 2.0,
                fs,
                TextAlign::Center,
                lc,
            );
        } else if fits {
            if c.dir == Dir::End {
                label(
                    d,
                    &ft,
                    &text,
                    pos(p) + fs * 0.6,
                    cc / 2.0,
                    fs,
                    TextAlign::Left,
                    lc,
                );
            } else {
                label(
                    d,
                    &ft,
                    &text,
                    pos(q) - fs * 0.6,
                    cc / 2.0,
                    fs,
                    TextAlign::Right,
                    lc,
                );
            }
        } else if c.dir == Dir::End {
            label(
                d,
                &ft,
                &text,
                pos(p) - fs * 0.5,
                cc / 2.0,
                fs,
                TextAlign::Right,
                lc,
            );
        } else {
            label(
                d,
                &ft,
                &text,
                pos(q) + fs * 0.5,
                cc / 2.0,
                fs,
                TextAlign::Left,
                lc,
            );
        }
    } else {
        let ty = if c.dir == Dir::Center || c.shape == Shape::Segments {
            h - so - l / 2.0
        } else if c.dir == Dir::End {
            pos(q) + fs * 0.9
        } else {
            pos(q) - fs * 0.9
        };
        let ty = ty.max(fs * 0.7).min(h - fs * 0.7);
        label(
            d,
            &ft,
            &text,
            cc / 2.0,
            ty,
            fs.min(t * 0.42).max(1.0),
            TextAlign::Center,
            lc,
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::Renderer;
    use cg_core::sample;

    #[test]
    fn the_goal_marker_crosses_the_bar() {
        let project = sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "pg", "name": "T", "duration": 30.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "p", "name": "Bar", "type": "progress", "value": "0", "max": "100",
                "duration": 0.0, "track": "#000000ff", "thickness": 40.0,
                "goal": "50", "goalText": "Objectif", "goalColor": "#00ff00",
                "props": {"x": 0.0, "y": 100.0, "width": 1000.0, "height": 100.0, "fill": [1.0, 0.0, 0.0, 1.0]}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_template(&project, &t, "main", 10.0, &Default::default());
        let px = |x: u32, y: u32| {
            let i = ((y * f.width + x) * 4) as usize;
            [f.data[i], f.data[i + 1], f.data[i + 2], f.data[i + 3]]
        };
        // The tick at 50 %, on the bar and above it (the bar is 130..170).
        assert!(
            px(500, 150)[1] > 200 && px(500, 150)[0] < 50,
            "{:?}",
            px(500, 150)
        );
        assert!(px(500, 120)[1] > 200, "{:?}", px(500, 120));
        // The track elsewhere, nothing above it.
        assert_eq!(px(300, 150)[3], 255);
        assert!(px(300, 150)[1] < 50);
        assert_eq!(px(300, 120)[3], 0);
        // Its label above the tick.
        let label = (450..550).any(|x| (90..108).any(|y| px(x, y)[3] > 0));
        assert!(label);
    }
}

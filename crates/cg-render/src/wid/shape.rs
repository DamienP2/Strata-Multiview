//! Fill and stroke of an outline with the shared styles of
//! `cg_core::wid::shape` (same look as `web/src/widgets/shape.tsx`).

use cg_core::wid::shape::{
    Cap, Cmd, Dash, FillKind, FillRule as Rule, FillStyle, ImageFill, Join, StrokePosition,
    StrokeStyle,
};
use cg_core::{Color, Prop};
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Point, RadialGradient,
    SpreadMode, Stroke, StrokeDash, Transform,
};

use super::Draw;
use crate::paint_for;

/// tiny-skia path of outline commands.
pub(crate) fn path(cmds: &[Cmd]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for c in cmds {
        match *c {
            Cmd::Move(x, y) => pb.move_to(x, y),
            Cmd::Line(x, y) => pb.line_to(x, y),
            Cmd::Cubic(a, b, c, d, e, f) => pb.cubic_to(a, b, c, d, e, f),
            Cmd::Close => pb.close(),
        }
    }
    pb.finish()
}

pub(crate) fn rule(r: Rule) -> FillRule {
    match r {
        Rule::Nonzero => FillRule::Winding,
        Rule::Evenodd => FillRule::EvenOdd,
    }
}

fn sk(c: Color, opacity: f32) -> tiny_skia::Color {
    let [r, g, b, a] = c.0;
    tiny_skia::Color::from_rgba(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        (a * opacity).clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

/// The stroke of a style and a width.
pub(crate) fn stroke_of(st: &StrokeStyle, width: f32) -> Stroke {
    let mut s = Stroke {
        width,
        line_join: match st.join {
            Join::Miter => tiny_skia::LineJoin::Miter,
            Join::Round => tiny_skia::LineJoin::Round,
            Join::Bevel => tiny_skia::LineJoin::Bevel,
        },
        line_cap: match st.cap {
            Cap::Butt => tiny_skia::LineCap::Butt,
            Cap::Round => tiny_skia::LineCap::Round,
            Cap::Square => tiny_skia::LineCap::Square,
        },
        ..Default::default()
    };
    match st.dash {
        Dash::Solid => {}
        Dash::Dashed => s.dash = StrokeDash::new(vec![width * 3.0, width * 2.0], 0.0),
        Dash::Dotted => {
            s.dash = StrokeDash::new(vec![0.001, width * 2.0], 0.0);
            s.line_cap = tiny_skia::LineCap::Round;
        }
    }
    s
}

/// Fills `p` with the fill style (bounding box (0, 0, w, h) for gradients and pictures).
pub(crate) fn fill(d: &mut Draw, p: &Path, fs: &FillStyle, r: FillRule) {
    let (w, h) = (d.w.max(1.0), d.h.max(1.0));
    let c1 = d.color(Prop::Fill);
    // The second colour is animatable (`fill2`; unset, `color2`).
    let c2 = d.color(Prop::Fill2);
    // From `fill` through the middle colours to the second colour.
    let stops = |o: f32| {
        let g = fs.gradient();
        let last = g.len() - 1;
        g.into_iter()
            .enumerate()
            .map(|(i, (at, c))| {
                let c = if i == last {
                    c2
                } else {
                    c.map_or(c1, Draw::hex)
                };
                GradientStop::new(at, sk(c, o))
            })
            .collect::<Vec<_>>()
    };
    let shader = match fs.kind {
        FillKind::None => return,
        FillKind::Solid => {
            d.fill(p, c1, r, None);
            return;
        }
        FillKind::Linear => {
            let a = (fs.angle - 90.0).to_radians();
            let (dx, dy) = (a.cos() / 2.0, a.sin() / 2.0);
            // Like CSS: the gradient line spans the box along the angle.
            let len = (w * dx.abs() + h * dy.abs()) * 2.0;
            let (cx, cy) = (w / 2.0, h / 2.0);
            LinearGradient::new(
                Point::from_xy(cx - dx * len, cy - dy * len),
                Point::from_xy(cx + dx * len, cy + dy * len),
                stops(d.opacity),
                SpreadMode::Pad,
                Transform::identity(),
            )
        }
        FillKind::Radial => {
            let c = Point::from_xy(w / 2.0, h / 2.0);
            RadialGradient::new(
                c,
                c,
                (w * w + h * h).sqrt() / 2.0,
                stops(d.opacity),
                SpreadMode::Pad,
                Transform::identity(),
            )
        }
        FillKind::Image => {
            let src = cg_core::anim::interpolate_text(&fs.image, d.data);
            let Some((iw, ih)) = d.image_size(&src) else {
                return;
            };
            let (iw, ih) = (iw as f32, ih as f32);
            let (sx, sy) = match fs.fit {
                ImageFill::Stretch => (w / iw, h / ih),
                ImageFill::Contain => {
                    let s = (w / iw).min(h / ih);
                    (s, s)
                }
                ImageFill::Cover => {
                    let s = (w / iw).max(h / ih);
                    (s, s)
                }
                ImageFill::Tile => (1.0, 1.0),
            };
            let tf = if fs.fit == ImageFill::Tile {
                Transform::identity()
            } else {
                Transform::from_translate((w - iw * sx) / 2.0, (h - ih * sy) / 2.0)
                    .pre_scale(sx, sy)
            };
            let spread = if fs.fit == ImageFill::Tile {
                SpreadMode::Repeat
            } else {
                SpreadMode::Pad
            };
            // A picture smaller than the box (contain) only paints where it is.
            let clip = if fs.fit == ImageFill::Contain {
                d.clip((w - iw * sx) / 2.0, (h - ih * sy) / 2.0, iw * sx, ih * sy)
            } else {
                None
            };
            let opacity = d.opacity;
            let (dtf, mask) = (d.tf, clip.as_ref().or(d.mask));
            let Some(img) = d.rd.image(&src) else { return };
            let paint = Paint {
                shader: tiny_skia::Pattern::new(
                    img.as_ref(),
                    spread,
                    tiny_skia::FilterQuality::Bicubic,
                    opacity,
                    tf,
                ),
                anti_alias: true,
                ..Default::default()
            };
            d.pm.fill_path(p, &paint, r, dtf, mask);
            return;
        }
    };
    let Some(shader) = shader else {
        d.fill(p, c1, r, None);
        return;
    };
    let paint = Paint {
        shader,
        anti_alias: true,
        ..Default::default()
    };
    d.fill_paint(p, &paint, r, None);
}

/// Strokes `p` inside, centred on or outside its outline (`strokeWidth`, `stroke`).
pub(crate) fn stroke(d: &mut Draw, p: &Path, st: &StrokeStyle, r: FillRule) {
    let width = d.num(Prop::StrokeWidth);
    if width <= 0.0 {
        return;
    }
    let paint: Paint = paint_for(d.color(Prop::Stroke), d.opacity);
    match st.position {
        StrokePosition::Center => d.stroke_paint(p, &paint, &stroke_of(st, width), None),
        StrokePosition::Inside | StrokePosition::Outside => {
            // Twice the width, half of it cut by the outline.
            let inside = st.position == StrokePosition::Inside;
            let m = match d.mask {
                // Inside the parents' clip: multiplied only where the
                // outline is (`mask::intersect`, the same values).
                Some(parent) if inside && r == FillRule::Winding => {
                    crate::mask::intersect(parent, p, d.tf)
                }
                _ => {
                    let mut m = Mask::new(d.pm.width(), d.pm.height()).expect("mask");
                    m.fill_path(p, r, true, d.tf);
                    if !inside {
                        m.invert();
                    }
                    if let Some(parent) = d.mask {
                        for (a, b) in m.data_mut().iter_mut().zip(parent.data()) {
                            *a = ((*a as u16 * *b as u16 + 127) / 255) as u8;
                        }
                    }
                    m
                }
            };
            d.pm.stroke_path(p, &paint, &stroke_of(st, width * 2.0), d.tf, Some(&m));
        }
    }
}

/// Fill then stroke (an outside stroke goes under nothing: it is outside).
pub(crate) fn paint(d: &mut Draw, cmds: &[Cmd], fs: &FillStyle, st: &StrokeStyle, r: Rule) {
    let Some(p) = path(cmds) else { return };
    let r = rule(r);
    fill(d, &p, fs, r);
    stroke(d, &p, st, r);
}

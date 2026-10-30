//! Pen: drawing (same picture as `web/src/widgets/pen.tsx`). tiny-skia has
//! no "visible portion" of a path: the path is flattened to a polyline with
//! lengths (`cg_core::wid::pen::flatten`) and cut there.

use cg_core::wid::pen::{
    flatten, outline, portion, progress, segments, stroke_width, visible, Head, Pt, State,
};
use cg_core::wid::shape::Dash;
use cg_core::{LayerKind, Prop};
use tiny_skia::{LineCap, LineJoin, PathBuilder, Stroke, StrokeDash};

use super::{shape, Draw};

fn poly_path(pts: &[Pt], close: bool) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let (first, rest) = pts.split_first()?;
    pb.move_to(first[0], first[1]);
    for q in rest {
        pb.line_to(q[0], q[1]);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

/// Drawn by its In / Out progress (still once drawn).
pub(crate) fn motion(d: &mut Draw) -> Option<crate::cache::Motion> {
    let LayerKind::Pen { x: c } = &d.layer.kind else {
        return None;
    };
    if segments(c, d.w, d.h).is_empty() {
        return Some(crate::cache::Motion::Still);
    }
    let (a, b) = progress(c, d.frame, d.hold, d.fps, d.time.age_secs);
    Some(crate::cache::Motion::of(&(a.to_bits(), b.to_bits())))
}

pub(crate) fn draw(d: &mut Draw, _st: &State) -> bool {
    let LayerKind::Pen { x: c } = &d.layer.kind else {
        return false;
    };
    let segs = segments(c, d.w, d.h);
    if segs.is_empty() {
        // Empty path or a single point: nothing on air (the inspector warns).
        return true;
    }
    let k = progress(c, d.frame, d.hold, d.fps, d.time.age_secs);
    // The visible portion at this frame (animatable), then the In / Out.
    let (pa, pb, fill_op) = portion(c, d.num(Prop::DrawStart), d.num(Prop::DrawEnd), k);
    let cmds = outline(c, d.w, d.h);
    // Fill: closed paths only, fading in as the stroke draws.
    if c.closed && c.fill_on && fill_op > 0.0 {
        if let Some(p) = shape::path(&cmds) {
            let keep = d.opacity;
            d.opacity *= fill_op;
            shape::fill(d, &p, &c.fill_style, shape::rule(c.fill_rule));
            d.opacity = keep;
        }
    }
    let sw = stroke_width(c, d.num(Prop::StrokeWidth), d.w, d.h);
    let poly = flatten(&segs);
    let Some(v) = visible(c, &poly, pa, pb, sw) else {
        return true;
    };
    let color = d.color(Prop::Stroke);
    let mut s = shape::stroke_of(&c.stroke_style, sw);
    if c.stroke_style.dash != Dash::Solid {
        // The pattern stays anchored to the start of the path.
        let arr = match c.stroke_style.dash {
            Dash::Dashed => vec![sw * 3.0, sw * 2.0],
            _ => vec![0.001, sw * 2.0],
        };
        s.dash = StrokeDash::new(arr, v.dash_offset);
    }
    let path = match &v.line {
        None => shape::path(&cmds),
        Some(line) => poly_path(line, false),
    };
    if let Some(p) = path {
        d.stroke(&p, color, &s, None);
    }
    for h in &v.heads {
        match h {
            Head::Fill(pts, w) => {
                if let Some(p) = poly_path(pts, true) {
                    d.fill(&p, color, tiny_skia::FillRule::Winding, None);
                    if *w > 0.0 {
                        let st = Stroke {
                            width: *w,
                            line_join: LineJoin::Round,
                            ..Default::default()
                        };
                        d.stroke(&p, color, &st, None);
                    }
                }
            }
            Head::Line(pts, w, butt) => {
                if let Some(p) = poly_path(pts, false) {
                    let st = Stroke {
                        width: *w,
                        line_join: LineJoin::Round,
                        line_cap: if *butt { LineCap::Butt } else { LineCap::Round },
                        ..Default::default()
                    };
                    d.stroke(&p, color, &st, None);
                }
            }
            Head::Circle(ctr, r) => {
                if let Some(p) = PathBuilder::from_circle(ctr[0], ctr[1], *r) {
                    d.fill(&p, color, tiny_skia::FillRule::Winding, None);
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::Renderer;

    #[test]
    fn the_drawn_portion_animates_on_the_timeline() {
        let project = cg_core::sample::demo_project();
        // A straight line across a 400 × 100 box, its End going 0 → 100 % over
        // 10 frames (no drawing at the In).
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "pen", "name": "T", "duration": 30.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "p", "name": "Pen", "type": "pen", "drawIn": false, "arrowEnd": "none",
                "points": [{"x": 0.0, "y": 0.5}, {"x": 1.0, "y": 0.5}],
                "props": {"x": 0.0, "y": 0.0, "width": 400.0, "height": 100.0, "stroke": [1.0, 1.0, 1.0, 1.0], "strokeWidth": 10.0},
                "tracks": {
                    "drawEnd": [
                        {"frame": 0.0, "value": 0.0, "easing": {"type": "linear"}},
                        {"frame": 10.0, "value": 100.0, "easing": {"type": "linear"}}
                    ]
                }
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_template(&project, &t, "main", 5.0, &Default::default());
        let a = |x: u32, y: u32| f.data[((y * f.width + x) * 4 + 3) as usize];
        // Half way: the left half only.
        assert_eq!(a(100, 50), 255);
        assert_eq!(a(300, 50), 0);
        let f = r.render_template(&project, &t, "main", 10.0, &Default::default());
        assert_eq!(f.data[((50 * f.width + 300) * 4 + 3) as usize], 255);
    }
}

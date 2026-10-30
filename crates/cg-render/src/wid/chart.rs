//! Chart: drawing (same picture as `web/src/widgets/chart.tsx`). The layout
//! (shapes and texts) comes from `cg_core::wid::chart::layout`.

use ab_glyph::{Font, ScaleFont};
use cg_core::wid::chart::{
    layout, shown, shown_grid, with_data, Anchor, Input, PShape, PText, Prim, State,
};
use cg_core::{Color, LayerKind, Prop, TextAlign, VerticalAlign};
use tiny_skia::{FillRule, LineCap, LineJoin, Stroke, StrokeDash, Transform};

use super::{shape, Draw};
use crate::text as tx;

fn col(s: &str, op: f32) -> Color {
    let mut c = Draw::hex(s);
    c.0[3] *= op.clamp(0.0, 1.0);
    c
}

fn draw_shape(d: &mut Draw, s: &PShape) {
    let Some(path) = shape::path(&s.d) else {
        return;
    };
    let mask = s.clip.and_then(|c| d.clip(c[0], c[1], c[2], c[3]));
    if let Some(f) = &s.fill {
        d.fill(
            &path,
            col(f, s.opacity * s.fill_opacity),
            FillRule::Winding,
            mask.as_ref(),
        );
    }
    if let Some(st) = &s.stroke {
        if s.width > 0.0 {
            let mut k = Stroke {
                width: s.width,
                ..Default::default()
            };
            if s.round {
                k.line_join = LineJoin::Round;
                k.line_cap = LineCap::Round;
            }
            if let Some([a, b]) = s.dash {
                k.dash = StrokeDash::new(vec![a, b], 0.0);
            }
            d.stroke(&path, col(st, s.opacity), &k, mask.as_ref());
        }
    }
}

fn draw_text(d: &mut Draw, family: &str, t: &PText) {
    if t.text.is_empty() || t.opacity <= 0.0 {
        return;
    }
    let Some(font) = d.font(family, t.weight) else {
        return;
    };
    // Baseline of a top-aligned line of the text layout.
    let upem = font.units_per_em().unwrap_or(1000.0);
    let scaled = font.as_scaled(ab_glyph::PxScale::from(
        t.size * font.height_unscaled() / upem,
    ));
    let base =
        (t.size * tx::LINE_HEIGHT - (scaled.ascent() - scaled.descent())) / 2.0 + scaled.ascent();
    let w = Draw::text_width(&font, &t.text, t.size, 0.0);
    let x0 = match t.anchor {
        Anchor::Start => 0.0,
        Anchor::Middle => -w / 2.0,
        Anchor::End => -w,
    };
    let Some(mut p) = Draw::text_path(
        &font,
        &t.text,
        t.size,
        0.0,
        TextAlign::Left,
        VerticalAlign::Top,
        (x0, -base, w + 10.0, t.size * 2.0),
        tx::LINE_HEIGHT,
    ) else {
        return;
    };
    let tf = if t.rotate {
        Transform::from_rotate(-90.0).post_translate(t.x, t.y)
    } else {
        Transform::from_translate(t.x, t.y)
    };
    let Some(pp) = p.clone().transform(tf) else {
        return;
    };
    p = pp;
    let mask = t.clip.and_then(|c| d.clip(c[0], c[1], c[2], c[3]));
    if let Some(h) = &t.halo {
        let k = Stroke {
            width: 5.0,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        d.stroke(&p, col(h, t.opacity), &k, mask.as_ref());
    }
    d.fill(
        &p,
        col(&t.fill, t.opacity),
        FillRule::Winding,
        mask.as_ref(),
    );
}

/// Drawn from its layout (what the time changes in it: the intro, tweens).
pub(crate) fn motion(d: &mut Draw, st: &State) -> Option<crate::cache::Motion> {
    let LayerKind::Chart { x: c } = &d.layer.kind else {
        return None;
    };
    // The take's chained steps and highlight, the timed steps revealed so far.
    let st = &shown(c, st, d.time.now_ms, d.time.age_secs, d.data);
    let grid = shown_grid(c, st, d.data);
    let c = with_data(c, d.data);
    let out = layout(&Input {
        cfg: &c,
        st,
        grid: &grid,
        w: d.w,
        h: d.h,
        fs: d.num(Prop::FontSize),
        age_ms: (d.time.age_secs * 1000.0) as f32,
        now: d.time.now_ms,
        lang: d.lang,
    });
    Some(crate::cache::Motion::of(&format!("{:?}", out.prims)))
}

pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::Chart { x: c } = &d.layer.kind else {
        return false;
    };
    let st = &shown(c, st, d.time.now_ms, d.time.age_secs, d.data);
    let grid = shown_grid(c, st, d.data);
    let c = with_data(c, d.data);
    let out = layout(&Input {
        cfg: &c,
        st,
        grid: &grid,
        w: d.w,
        h: d.h,
        fs: d.num(Prop::FontSize),
        age_ms: (d.time.age_secs * 1000.0) as f32,
        now: d.time.now_ms,
        lang: d.lang,
    });
    for p in &out.prims {
        match p {
            Prim::Shape(s) => draw_shape(d, s),
            Prim::Text(t) => draw_text(d, &c.font, t),
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::Renderer;

    fn render(chart: serde_json::Value) -> impl Fn(u32, u32) -> [u8; 4] {
        let project = cg_core::sample::demo_project();
        let mut layer = serde_json::json!({
            "id": "c", "name": "Chart", "type": "chart",
            "data": [["Candidat", "%"], ["A", "40"], ["B", "20"]],
            "title": {"pos": "none"}, "legend": "none", "panel": {"on": false},
            "anim": {"fx": "none"}, "labels": {"value": false},
            "props": {"x": 0.0, "y": 0.0, "width": 1000.0, "height": 600.0, "fontSize": 26.0}
        });
        for (k, v) in chart.as_object().unwrap() {
            layer[k] = v.clone();
        }
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "ch", "name": "T", "duration": 30.0, "markers": [], "zone": "", "fields": [],
            "layers": [layer]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_template(&project, &t, "main", 10.0, &Default::default());
        move |x: u32, y: u32| {
            let i = ((y * f.width + x) * 4) as usize;
            [f.data[i], f.data[i + 1], f.data[i + 2], f.data[i + 3]]
        }
    }

    /// Pixels of the first series' colour (#7c5cff) on a row / a column.
    fn violet(p: &[u8; 4]) -> bool {
        p[3] > 200 && p[0] > 100 && p[0] < 150 && p[1] < 120 && p[2] > 220
    }

    #[test]
    fn columns_lie_down_and_the_reference_line_crosses() {
        // Standing: A is a tall column on the left half, nothing at its top-right.
        let px = render(serde_json::json!({}));
        let tall = (0..600).filter(|y| violet(&px(300, *y))).count();
        assert!(tall > 200, "{tall}");
        // Lying: A is a long bar across, from the names on the left.
        let px = render(serde_json::json!({"orient": "h"}));
        let row = (0..600).find(|y| (0..1000).filter(|x| violet(&px(*x, *y))).count() > 400);
        assert!(row.is_some(), "a long horizontal bar");
        let y = row.unwrap();
        // B's bar (half as long) is lower.
        let b = (y + 40..600).find(|y| {
            let n = (0..1000).filter(|x| violet(&px(*x, *y))).count();
            n > 150 && n < 400
        });
        assert!(b.is_some(), "a shorter bar below");
        // A yellow reference line (the text colour) at 30: vertical, between the bars' ends.
        let px = render(
            serde_json::json!({"orient": "h", "textColor": "#ffff00", "refLine": {"on": true, "value": 30.0}}),
        );
        let yellow = |p: [u8; 4]| p[3] > 150 && p[0] > 200 && p[1] > 200 && p[2] < 80;
        let x = (0..1000)
            .filter(|x| (100..500).filter(|y| yellow(px(*x, *y))).count() > 100)
            .count();
        assert!(x > 0, "a vertical yellow line");
    }
}

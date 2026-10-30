//! Drawing helpers shared by the widgets: filled boxes, fitted lines of text.

use ab_glyph::FontArc;
use cg_core::{Color, TextAlign, VerticalAlign};
use tiny_skia::{FillRule, Mask, Transform};

use super::{rrect, Draw};
use crate::text as tx;

/// Fills a rectangle (or a rounded one).
#[allow(clippy::too_many_arguments)]
pub(crate) fn rect(
    d: &mut Draw,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    c: Color,
    m: Option<&Mask>,
) {
    if w <= 0.0 || h <= 0.0 || c.0[3] <= 0.0 {
        return;
    }
    if let Some(p) = rrect(x, y, w, h, r) {
        d.fill(&p, c, FillRule::Winding, m);
    }
}

pub(crate) fn with_alpha(c: Color, a: f32) -> Color {
    Color([c.0[0], c.0[1], c.0[2], c.0[3] * a])
}

/// How a too-long text fits its box: shrunk down to `min_size`, squeezed
/// down to `min_squeeze` %, then cut with an ellipsis.
#[derive(Clone, Copy)]
pub(crate) struct Fit {
    pub min_size: f32,
    pub min_squeeze: f32,
}

/// Lays out one line of text in (x, y, w, h), fitted; returns the path and whether it was fitted.
pub(crate) fn fitted_path(
    font: &FontArc,
    s: &str,
    size: f32,
    ls: f32,
    align: TextAlign,
    (x, y, w, h): (f32, f32, f32, f32),
    fit: Option<Fit>,
) -> (Option<tiny_skia::Path>, bool) {
    let mut style = tx::TextStyle {
        size,
        letter_spacing: ls,
        align,
        vertical_align: VerticalAlign::Middle,
        box_w: w.max(1.0),
        box_h: h.max(size),
        line_height: tx::LINE_HEIGHT,
        decoration: cg_core::TextDecoration::None,
    };
    let (mut text, mut sx, mut changed) = (s.to_string(), 1.0f32, false);
    if let Some(f) = fit {
        use cg_core::TextOverflow as O;
        let r = tx::fit_text(
            font,
            s,
            &style,
            false,
            O::Fit,
            f.min_squeeze / 100.0,
            (f.min_size / size).min(1.0),
        );
        changed = r.size != size || r.squeeze != 1.0 || r.text != s;
        style.size = r.size;
        sx = r.squeeze;
        text = r.text;
        if r.clip {
            // At the minimum: cut with an ellipsis.
            let mut st2 = tx::TextStyle {
                box_w: w / sx,
                ..style
            };
            st2.size = r.size;
            text = tx::fit_text(font, &text, &st2, false, O::Ellipsis, 1.0, 1.0).text;
        }
    }
    style.box_w = (w / sx).max(1.0);
    let p = tx::layout_text(font, &text, &style)
        .and_then(|p| p.transform(Transform::from_row(sx, 0.0, 0.0, 1.0, x, y)));
    (p, changed)
}

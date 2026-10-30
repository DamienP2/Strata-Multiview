//! Ellipse: drawing (same picture as `web/src/widgets/ellipse.tsx`).

use cg_core::wid::ellipse::{outline_span, Span, State};
use cg_core::wid::shape::FillRule;
use cg_core::{LayerKind, Prop};

use super::{shape, Draw};

pub(crate) fn draw(d: &mut Draw, _st: &State) -> bool {
    let LayerKind::Ellipse { x: c } = &d.layer.kind else {
        return false;
    };
    // Angles and inner radius as resolved at this frame: animatable, else the settings.
    let span = Span {
        start: d.num(Prop::ArcStart),
        end: d.num(Prop::ArcEnd),
        inner: d.num(Prop::InnerRadius),
    };
    let Some((cmds, closed)) = outline_span(c.portion, span, d.w, d.h) else {
        return true;
    };
    if closed {
        shape::paint(d, &cmds, &c.fill_style, &c.stroke_style, FillRule::Evenodd);
    } else if let Some(p) = shape::path(&cmds) {
        // An arc: its centred stroke only, with its ends (none at 0 px).
        let width = d.num(Prop::StrokeWidth);
        if width <= 0.0 {
            return true;
        }
        let s = shape::stroke_of(&c.stroke_style, width);
        let col = d.color(Prop::Stroke);
        d.stroke(&p, col, &s, None);
    }
    true
}

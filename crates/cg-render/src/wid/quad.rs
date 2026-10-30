//! Quad: drawing (same picture as `web/src/widgets/quad.tsx`).

use cg_core::wid::quad::{outline, State};
use cg_core::wid::shape::FillRule;
use cg_core::{LayerKind, Prop};

use super::{shape, Draw};

pub(crate) fn draw(d: &mut Draw, _st: &State) -> bool {
    let LayerKind::Rect { x: c } = &d.layer.kind else {
        return false;
    };
    // Per corner: the animatable radius of each corner.
    let each = [
        Prop::CornerTl,
        Prop::CornerTr,
        Prop::CornerBr,
        Prop::CornerBl,
    ]
    .map(|p| d.num(p));
    let cmds = outline(c, d.w, d.h, d.num(Prop::CornerRadius), Some(each));
    shape::paint(d, &cmds, &c.fill_style, &c.stroke_style, FillRule::Nonzero);
    true
}

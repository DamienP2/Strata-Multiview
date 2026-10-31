//! Polygon: drawing (same picture as `web/src/widgets/polygon.tsx`).

use strata_mv_core::wid::polygon::{outline, rule, State};
use strata_mv_core::LayerKind;

use super::{shape, Draw};

pub(crate) fn draw(d: &mut Draw, _st: &State) -> bool {
    let LayerKind::Polygon { x: c } = &d.layer.kind else {
        return false;
    };
    let cmds = outline(c, d.w, d.h);
    if !cmds.is_empty() {
        shape::paint(d, &cmds, &c.fill_style, &c.stroke_style, rule(c));
    }
    true
}

//! Polygon: drawing (same picture as `web/src/widgets/polygon.tsx`).

use cg_core::wid::polygon::{outline, rule, State};
use cg_core::LayerKind;

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

#[cfg(test)]
mod tests {
    use crate::Renderer;

    #[test]
    fn a_regular_polygon_touches_its_box() {
        let project = cg_core::sample::demo_project();
        // A pentagon in a 400 × 400 box: its bottom side lies on the box's
        // (it was 38 px above when inscribed in the circle).
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "pg", "name": "T", "duration": 30.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "p", "name": "Polygon", "type": "polygon", "sides": 5.0,
                "props": {"x": 0.0, "y": 0.0, "width": 400.0, "height": 400.0, "fill": [1.0, 0.0, 0.0, 1.0]}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_template(&project, &t, "main", 0.0, &Default::default());
        let a = |x: u32, y: u32| f.data[((y * f.width + x) * 4 + 3) as usize];
        assert_eq!(a(200, 396), 255);
        assert_eq!(a(200, 3), 255);
        assert_eq!(a(3, 200), 0);
    }
}

//! Quad: rectangle, diamond, parallelogram or trapezoid.
//! Drawn by `cg-render/src/wid/quad.rs` and `web/src/widgets/quad.tsx` (keep in sync).

use serde::{Deserialize, Serialize};

use super::shape::{rounded_polygon, Cmd, FillStyle, StrokeStyle};
use super::ActCtx;
use crate::model::Layer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuadShape {
    #[default]
    Rect,
    Diamond,
    Parallelogram,
    Trapezoid,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkewUnit {
    #[default]
    Px,
    Deg,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrapMode {
    #[default]
    Percent,
    Offsets,
}

/// Settings of the widget, flattened into the layer's JSON. The radius of
/// a rectangle is the animated `cornerRadius` property; the colours and the
/// stroke width are `fill`, `stroke` and `strokeWidth`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub shape: QuadShape,
    /// One radius per corner (top left, top right, bottom right, bottom left).
    pub per_corner: bool,
    pub corners: [f32; 4],
    pub skew_unit: SkewUnit,
    /// Parallelogram: horizontal offset of the top side (sign = direction)…
    pub skew_px: f32,
    /// …or the angle of the sides.
    pub skew_deg: f32,
    pub trap_mode: TrapMode,
    /// Trapezoid: top side, % of the bottom one…
    pub trap_top: f32,
    /// …or the left and right offsets of the top side, px.
    pub trap_left: f32,
    pub trap_right: f32,
    /// Rounding of the vertices of the other shapes, px.
    pub vertex_round: f32,
    pub fill_style: FillStyle,
    pub stroke_style: StrokeStyle,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shape: QuadShape::Rect,
            per_corner: false,
            corners: [0.0, 0.0, 0.0, 0.0],
            skew_unit: SkewUnit::Px,
            skew_px: 120.0,
            skew_deg: 15.0,
            trap_mode: TrapMode::Percent,
            trap_top: 70.0,
            trap_left: 80.0,
            trap_right: 200.0,
            vertex_round: 0.0,
            fill_style: FillStyle::default(),
            stroke_style: StrokeStyle::default(),
        }
    }
}

/// Largest slant that keeps the sides from crossing.
pub fn max_skew(w: f32) -> f32 {
    (w - 1.0).max(0.0)
}

/// Offset of the top side of a parallelogram, capped.
pub fn skew(c: &Config, w: f32, h: f32) -> f32 {
    let off = match c.skew_unit {
        SkewUnit::Deg => c.skew_deg.to_radians().tan() * h,
        SkewUnit::Px => c.skew_px,
    };
    off.clamp(-max_skew(w), max_skew(w))
}

/// The four corners in the box, the parameters capped so the sides never cross.
pub fn corners(c: &Config, w: f32, h: f32) -> [[f32; 2]; 4] {
    match c.shape {
        QuadShape::Diamond => [[w / 2.0, 0.0], [w, h / 2.0], [w / 2.0, h], [0.0, h / 2.0]],
        QuadShape::Parallelogram => {
            let off = skew(c, w, h);
            if off >= 0.0 {
                [[off, 0.0], [w, 0.0], [w - off, h], [0.0, h]]
            } else {
                [[0.0, 0.0], [w + off, 0.0], [w, h], [-off, h]]
            }
        }
        QuadShape::Trapezoid => {
            let (l, r) = match c.trap_mode {
                TrapMode::Percent => {
                    let top = c.trap_top.clamp(0.0, 100.0) / 100.0 * w;
                    ((w - top) / 2.0, (w - top) / 2.0)
                }
                TrapMode::Offsets => {
                    let l = c.trap_left.clamp(0.0, w);
                    (l, c.trap_right.clamp(0.0, w - l))
                }
            };
            [[l, 0.0], [w - r, 0.0], [w, h], [0.0, h]]
        }
        QuadShape::Rect => [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
    }
}

/// The outline; `radius` is the animated corner radius of a rectangle,
/// `per_corner` its animated radius of each corner (`cornerTl`…; the
/// settings' `corners` when None).
pub fn outline(c: &Config, w: f32, h: f32, radius: f32, per_corner: Option<[f32; 4]>) -> Vec<Cmd> {
    let pts = corners(c, w, h);
    let radii: Vec<f32> = match c.shape {
        QuadShape::Rect if c.per_corner => per_corner.unwrap_or(c.corners).to_vec(),
        QuadShape::Rect => vec![radius],
        _ => vec![c.vertex_round],
    };
    rounded_polygon(&pts, &radii)
}

/// No operator state: a quad is taken, continued and taken out like any graphic.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {}

pub fn act(
    _layer: &Layer,
    _st: &mut State,
    op: &str,
    _args: &serde_json::Value,
    _ctx: &ActCtx,
) -> Result<(), String> {
    Err(format!("unknown action `{op}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameters_are_capped() {
        let c = Config {
            shape: QuadShape::Parallelogram,
            skew_px: 5000.0,
            ..Default::default()
        };
        assert_eq!(corners(&c, 300.0, 100.0)[0][0], 299.0);
        let c = Config {
            shape: QuadShape::Trapezoid,
            trap_mode: TrapMode::Offsets,
            trap_left: 600.0,
            trap_right: 700.0,
            ..Default::default()
        };
        let p = corners(&c, 400.0, 100.0);
        // The top side stops at 0 px: a triangle.
        assert_eq!(p[0][0], p[1][0]);
    }

    #[test]
    fn old_rectangles_load() {
        let l: Layer =
            serde_json::from_str(r#"{"id":"a","name":"Box","type":"rect","props":{}}"#).unwrap();
        let crate::LayerKind::Rect { x } = l.kind else {
            panic!()
        };
        assert_eq!(x, Config::default());
    }

    #[test]
    fn corners_and_second_colour_animate_from_their_settings() {
        use crate::anim::layer_default;
        use crate::model::{Color, Prop, Value};
        let l: Layer = serde_json::from_str(
            r##"{"id":"a","name":"Box","type":"rect","props":{},"perCorner":true,"corners":[10,20,30,40],"fillStyle":{"kind":"linear","color2":"#0000ff"}}"##,
        )
        .unwrap();
        assert_eq!(layer_default(&l, Prop::CornerBr), Value::Number(30.0));
        assert_eq!(
            layer_default(&l, Prop::Fill2),
            Value::Color(Color::from_hex("#0000ff").unwrap())
        );
        let crate::LayerKind::Rect { x } = &l.kind else {
            panic!()
        };
        // The animated radii win over the settings.
        let o = outline(x, 200.0, 100.0, 0.0, Some([0.0, 0.0, 0.0, 0.0]));
        assert_eq!(o.len(), 6);
        assert!(outline(x, 200.0, 100.0, 0.0, None).len() > 6);
    }

    #[test]
    fn square_is_the_dimensions_lock() {
        // « Square » (`lockRatio`) of older rectangles is the layer's lock.
        let l: Layer = serde_json::from_str(
            r#"{"id":"a","name":"Box","type":"rect","props":{},"lockRatio":true}"#,
        )
        .unwrap();
        assert!(l.lock_ratio);
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(json.matches("lockRatio").count(), 1);
    }
}

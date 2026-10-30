//! What the shape widgets share (quad, polygon, ellipse, pen): fill and
//! stroke styles, and outlines as plain path commands so that both
//! renderers draw the same geometry (`web/src/widgets/shape.tsx`).
//!
//! The colours and the stroke width stay animated properties of the layer
//! (`fill`, `stroke`, `strokeWidth`): the styles here say how they are used.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillKind {
    None,
    /// The `fill` colour.
    #[default]
    Solid,
    /// From `fill` to `color2`, at `angle`.
    Linear,
    /// From `fill` at the centre to `color2`.
    Radial,
    /// A picture (`image`), fitted to the shape's box.
    Image,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageFill {
    Contain,
    #[default]
    Cover,
    Stretch,
    Tile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FillStyle {
    pub kind: FillKind,
    /// Second colour of a gradient, `#rrggbb[aa]`.
    pub color2: String,
    /// Linear gradient direction, degrees (90 = left to right, like CSS).
    pub angle: f32,
    /// Colours between `fill` (0 %) and `color2` (100 %) of a gradient.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stops: Vec<GradientStop>,
    /// Picture of an image fill (asset path).
    pub image: String,
    pub fit: ImageFill,
}

impl Default for FillStyle {
    fn default() -> Self {
        Self {
            kind: FillKind::Solid,
            color2: "#ff3b5c".into(),
            angle: 90.0,
            stops: Vec::new(),
            image: String::new(),
            fit: ImageFill::Cover,
        }
    }
}

/// A colour inside a gradient, at `at` % of its length.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GradientStop {
    pub at: f32,
    pub color: String,
}

impl Default for GradientStop {
    fn default() -> Self {
        Self {
            at: 50.0,
            color: "#ffffff".into(),
        }
    }
}

impl FillStyle {
    /// The colours of the gradient in order: (position 0..1, `#hex` or None
    /// for the `fill` colour at 0).
    pub fn gradient(&self) -> Vec<(f32, Option<&str>)> {
        let mut mid: Vec<_> = self
            .stops
            .iter()
            .filter(|s| s.at.is_finite())
            .map(|s| ((s.at / 100.0).clamp(0.0, 1.0), Some(s.color.as_str())))
            .collect();
        mid.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out = vec![(0.0, None)];
        out.extend(mid);
        out.push((1.0, Some(self.color2.as_str())));
        out
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StrokePosition {
    /// Inside the outline (never grows the shape).
    #[default]
    Inside,
    Center,
    Outside,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StrokeStyle {
    pub position: StrokePosition,
    pub dash: Dash,
    pub join: Join,
    pub cap: Cap,
}

/// A stroke style read over other defaults than [`StrokeStyle::default`]
/// (the centred stroke of an older ellipse, the round ends of a pen): the
/// fields a layer leaves out keep `base`'s, as the web app reads them.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PartialStroke {
    pub position: Option<StrokePosition>,
    pub dash: Option<Dash>,
    pub join: Option<Join>,
    pub cap: Option<Cap>,
}

impl PartialStroke {
    pub fn over(self, base: StrokeStyle) -> StrokeStyle {
        StrokeStyle {
            position: self.position.unwrap_or(base.position),
            dash: self.dash.unwrap_or(base.dash),
            join: self.join.unwrap_or(base.join),
            cap: self.cap.unwrap_or(base.cap),
        }
    }
}

/// Fill rule of self-crossing outlines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillRule {
    #[default]
    Nonzero,
    Evenodd,
}

/// One command of an outline, in layer pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// A closed polygon with rounded vertices. `radii`: one per vertex (the
/// last one repeats); each is capped so that two roundings never overlap,
/// so a too large radius gives a pill or a circle, never a broken shape.
pub fn rounded_polygon(pts: &[[f32; 2]], radii: &[f32]) -> Vec<Cmd> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    struct V {
        a: [f32; 2],
        b: [f32; 2],
        c: [f32; 2],
        d: f32,
    }
    let mut seg = Vec::with_capacity(n);
    for i in 0..n {
        let p0 = pts[(i + n - 1) % n];
        let p1 = pts[i];
        let p2 = pts[(i + 1) % n];
        let v0 = [p0[0] - p1[0], p0[1] - p1[1]];
        let v2 = [p2[0] - p1[0], p2[1] - p1[1]];
        let (l0, l2) = (v0[0].hypot(v0[1]), v2[0].hypot(v2[1]));
        if l0 < 1e-6 || l2 < 1e-6 {
            seg.push(V {
                a: p1,
                b: p1,
                c: p1,
                d: 0.0,
            });
            continue;
        }
        let u0 = [v0[0] / l0, v0[1] / l0];
        let u2 = [v2[0] / l2, v2[1] / l2];
        let cos = (u0[0] * u2[0] + u0[1] * u2[1]).clamp(-1.0, 1.0);
        let tan = (cos.acos() / 2.0).tan();
        let r = radii
            .get(i)
            .or(radii.last())
            .copied()
            .unwrap_or(0.0)
            .max(0.0);
        let d = if tan > 1e-6 { r / tan } else { 0.0 }
            .min(l0 / 2.0)
            .min(l2 / 2.0);
        seg.push(V {
            a: [p1[0] + u0[0] * d, p1[1] + u0[1] * d],
            b: [p1[0] + u2[0] * d, p1[1] + u2[1] * d],
            c: p1,
            d,
        });
    }
    const K: f32 = 0.5523;
    let mut out = vec![Cmd::Move(seg[0].b[0], seg[0].b[1])];
    for j in 1..=n {
        let s = &seg[j % n];
        out.push(Cmd::Line(s.a[0], s.a[1]));
        if s.d > 0.01 {
            let c1 = [
                s.a[0] + (s.c[0] - s.a[0]) * K,
                s.a[1] + (s.c[1] - s.a[1]) * K,
            ];
            let c2 = [
                s.b[0] + (s.c[0] - s.b[0]) * K,
                s.b[1] + (s.c[1] - s.b[1]) * K,
            ];
            out.push(Cmd::Cubic(c1[0], c1[1], c2[0], c2[1], s.b[0], s.b[1]));
        }
    }
    out.push(Cmd::Close);
    out
}

/// SVG path data of commands (the web renderer draws the same string).
pub fn svg_path(cmds: &[Cmd]) -> String {
    let mut s = String::new();
    for c in cmds {
        match c {
            Cmd::Move(x, y) => s += &format!("M{x} {y}"),
            Cmd::Line(x, y) => s += &format!("L{x} {y}"),
            Cmd::Cubic(a, b, c, d, e, f) => s += &format!("C{a} {b} {c} {d} {e} {f}"),
            Cmd::Close => s += "Z",
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_stops_in_order() {
        let f: FillStyle = serde_json::from_str(
            r##"{"kind":"linear","color2":"#0000ff","stops":[{"at":75,"color":"#00ff00"},{"at":25,"color":"#ff0000"},{"at":150,"color":"#ffffff"}]}"##,
        )
        .unwrap();
        assert_eq!(
            f.gradient(),
            vec![
                (0.0, None),
                (0.25, Some("#ff0000")),
                (0.75, Some("#00ff00")),
                (1.0, Some("#ffffff")),
                (1.0, Some("#0000ff"))
            ]
        );
        // Older shapes: two colours.
        assert_eq!(FillStyle::default().gradient().len(), 2);
    }

    #[test]
    fn radius_is_capped() {
        // A 100 × 40 rectangle with a huge radius is a pill: the roundings meet.
        let p = rounded_polygon(
            &[[0.0, 0.0], [100.0, 0.0], [100.0, 40.0], [0.0, 40.0]],
            &[500.0],
        );
        let Cmd::Move(x, y) = p[0] else { panic!() };
        assert!((x - 20.0).abs() < 1e-3 && y.abs() < 1e-3);
    }
}

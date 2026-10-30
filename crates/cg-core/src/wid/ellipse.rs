//! Ellipse: portion (sector / arc / chord) between a start and an end
//! angle, ring / donut (a circle is the Dimensions lock of the layer). The
//! outline is a path in layer pixels (never a scaled circle), so a flat
//! ellipse keeps an even stroke.
//! Drawn by `cg-render/src/wid/ellipse.rs` and `web/src/widgets/ellipse.tsx` (keep in sync).

use serde::{Deserialize, Serialize};

use serde::Deserializer;

use super::shape::{Cmd, FillStyle, PartialStroke, StrokePosition, StrokeStyle};
use super::ActCtx;
use crate::model::Layer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Portion {
    /// Pie slice, closed through the centre (a ring / donut with `inner`).
    #[default]
    Sector,
    /// The outline only (stroke).
    Arc,
    /// Closed by a straight line.
    Chord,
}

/// The stroke of an older ellipse was centred on its outline.
fn centred() -> StrokeStyle {
    StrokeStyle {
        position: StrokePosition::Center,
        ..Default::default()
    }
}

/// A style that leaves its position out is centred too (as the web reads it).
fn centred_partial<'de, D: Deserializer<'de>>(d: D) -> Result<StrokeStyle, D::Error> {
    Ok(PartialStroke::deserialize(d)?.over(centred()))
}

/// Settings of the widget, flattened into the layer's JSON. Without them
/// (older projects) it is the full ellipse it always was.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Older ellipses: width and height stayed equal. Read only: the project
    /// upgrade turns it into the layer's Dimensions lock (`lock_ratio`).
    #[serde(skip_serializing)]
    pub circle: bool,
    /// Degrees, 0 = 12 o'clock, clockwise.
    pub start: f32,
    /// Degrees; past 360 wraps round. 360° of sweep = full ellipse.
    pub end: f32,
    pub portion: Portion,
    /// Sector: inner radius, % (ring / donut).
    pub inner: f32,
    pub fill_style: FillStyle,
    #[serde(default = "centred", deserialize_with = "centred_partial")]
    pub stroke_style: StrokeStyle,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            circle: false,
            start: 0.0,
            end: 360.0,
            portion: Portion::Sector,
            inner: 0.0,
            fill_style: FillStyle::default(),
            stroke_style: centred(),
        }
    }
}

/// The portion's angles and inner radius: the settings, or the values of
/// the animatable properties (`Prop::ArcStart`, `ArcEnd`, `InnerRadius`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    /// Degrees, 0 = 12 o'clock, clockwise.
    pub start: f32,
    /// Degrees; past 360 wraps round.
    pub end: f32,
    /// Inner radius, %.
    pub inner: f32,
}

impl Span {
    pub fn of(c: &Config) -> Self {
        Self {
            start: c.start,
            end: c.end,
            inner: c.inner,
        }
    }

    /// Sweep in degrees: 0 = nothing, 360 = full ellipse.
    pub fn sweep(&self) -> f32 {
        let mut d = self.end - self.start;
        if d < 0.0 {
            d += 360.0;
        }
        d.clamp(0.0, 360.0)
    }
}

/// Sweep in degrees: 0 = nothing, 360 = full ellipse.
pub fn sweep(c: &Config) -> f32 {
    Span::of(c).sweep()
}

/// Point at angle `a` (degrees) of the ellipse (cx, cy, rx, ry).
fn pt(cx: f32, cy: f32, rx: f32, ry: f32, a: f32) -> [f32; 2] {
    let r = a.to_radians();
    [cx + r.sin() * rx, cy - r.cos() * ry]
}

/// Cubic Béziers of the elliptical arc from `a0` to `a1` (degrees, either
/// direction), at most 90° each; the current point must be at `a0`.
pub fn arc_to(out: &mut Vec<Cmd>, (cx, cy, rx, ry): (f32, f32, f32, f32), a0: f32, a1: f32) {
    let n = ((a1 - a0).abs() / 90.0).ceil().max(1.0) as usize;
    let step = (a1 - a0) / n as f32;
    let k = 4.0 / 3.0 * (step.to_radians() / 4.0).tan();
    for i in 0..n {
        let s = (a0 + step * i as f32).to_radians();
        let e = (a0 + step * (i + 1) as f32).to_radians();
        let p0 = [cx + s.sin() * rx, cy - s.cos() * ry];
        let p1 = [cx + e.sin() * rx, cy - e.cos() * ry];
        // Derivative of (sin a · rx, −cos a · ry).
        let d0 = [s.cos() * rx, s.sin() * ry];
        let d1 = [e.cos() * rx, e.sin() * ry];
        out.push(Cmd::Cubic(
            p0[0] + d0[0] * k,
            p0[1] + d0[1] * k,
            p1[0] - d1[0] * k,
            p1[1] - d1[1] * k,
            p1[0],
            p1[1],
        ));
    }
}

/// The outline and whether it is closed; None = nothing to draw.
pub fn outline(c: &Config, w: f32, h: f32) -> Option<(Vec<Cmd>, bool)> {
    outline_span(c.portion, Span::of(c), w, h)
}

/// The outline of `portion` between the angles of `span`.
pub fn outline_span(portion: Portion, span: Span, w: f32, h: f32) -> Option<(Vec<Cmd>, bool)> {
    let (cx, cy, rx, ry) = (w / 2.0, h / 2.0, w / 2.0, h / 2.0);
    let sw = span.sweep();
    if !sw.is_finite() || sw <= 0.01 || rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    let start = span.start;
    let k = span.inner.clamp(0.0, 100.0) / 100.0;
    let ring = portion == Portion::Sector && k > 0.0;
    if ring && k >= 1.0 {
        return None;
    }
    let e = (cx, cy, rx, ry);
    let ei = (cx, cy, rx * k, ry * k);
    let mut out = Vec::new();
    let a = pt(cx, cy, rx, ry, start);
    if sw >= 360.0 {
        // A closed path from the start angle: no seam.
        out.push(Cmd::Move(a[0], a[1]));
        arc_to(&mut out, e, start, start + 360.0);
        out.push(Cmd::Close);
        if ring {
            // The hole the other way round: empty with either fill rule.
            let b = pt(cx, cy, rx * k, ry * k, start);
            out.push(Cmd::Move(b[0], b[1]));
            arc_to(&mut out, ei, start, start - 360.0);
            out.push(Cmd::Close);
        }
        return Some((out, true));
    }
    let end = start + sw;
    match portion {
        Portion::Arc => {
            out.push(Cmd::Move(a[0], a[1]));
            arc_to(&mut out, e, start, end);
            Some((out, false))
        }
        Portion::Chord => {
            out.push(Cmd::Move(a[0], a[1]));
            arc_to(&mut out, e, start, end);
            out.push(Cmd::Close);
            Some((out, true))
        }
        Portion::Sector if ring => {
            out.push(Cmd::Move(a[0], a[1]));
            arc_to(&mut out, e, start, end);
            let ib = pt(cx, cy, rx * k, ry * k, end);
            out.push(Cmd::Line(ib[0], ib[1]));
            arc_to(&mut out, ei, end, start);
            out.push(Cmd::Close);
            Some((out, true))
        }
        Portion::Sector => {
            out.push(Cmd::Move(cx, cy));
            out.push(Cmd::Line(a[0], a[1]));
            arc_to(&mut out, e, start, end);
            out.push(Cmd::Close);
            Some((out, true))
        }
    }
}

/// No operator state.
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
    fn old_ellipses_load_as_full_ellipses() {
        let l: Layer =
            serde_json::from_str(r#"{"id":"a","name":"E","type":"ellipse","props":{}}"#).unwrap();
        let crate::LayerKind::Ellipse { x } = l.kind else {
            panic!()
        };
        assert_eq!(x, Config::default());
        assert_eq!(x.stroke_style.position, StrokePosition::Center);
        let (cmds, closed) = outline(&x, 200.0, 100.0).unwrap();
        assert!(closed);
        // Move + four quarter arcs + close.
        assert_eq!(cmds.len(), 6);
    }

    #[test]
    fn sweeps() {
        let c = |start: f32, end: f32| Config {
            start,
            end,
            ..Default::default()
        };
        assert_eq!(sweep(&c(137.0, 497.0)), 360.0);
        assert_eq!(sweep(&c(120.0, 120.0)), 0.0);
        assert!(outline(&c(120.0, 120.0), 100.0, 100.0).is_none());
        assert_eq!(sweep(&c(270.0, 90.0)), 180.0);
        // Full ellipse from 137°: closed, ends where it starts.
        let (cmds, _) = outline(&c(137.0, 497.0), 100.0, 100.0).unwrap();
        let Cmd::Move(x0, y0) = cmds[0] else { panic!() };
        let Cmd::Cubic(.., x1, y1) = cmds[cmds.len() - 2] else {
            panic!()
        };
        assert!((x0 - x1).abs() < 1e-3 && (y0 - y1).abs() < 1e-3);
    }

    #[test]
    fn partial_stroke_style_stays_centred() {
        let l: Layer = serde_json::from_str(
            r#"{"id":"a","name":"E","type":"ellipse","props":{},"strokeStyle":{"dash":"dashed"},"circle":true}"#,
        )
        .unwrap();
        let crate::LayerKind::Ellipse { x } = &l.kind else {
            panic!()
        };
        assert_eq!(x.stroke_style.position, StrokePosition::Center);
        assert_eq!(x.stroke_style.dash, super::super::shape::Dash::Dashed);
        assert!(x.circle);
        // « Circle » is read, never written again.
        let json = serde_json::to_string(&l).unwrap();
        assert!(!json.contains("circle"));
        // The project upgrade makes it the layer's Dimensions lock.
        let mut t = crate::model::Template::blank("T", "z", 50.0);
        t.layers.push(l);
        assert!(t.upgrade_layers());
        assert!(t.layers[0].lock_ratio);
        assert!(!t.upgrade_layers());
    }

    #[test]
    fn inner_100_is_invisible() {
        let c = Config {
            inner: 100.0,
            ..Default::default()
        };
        assert!(outline(&c, 100.0, 100.0).is_none());
        let c = Config {
            inner: 100.0,
            portion: Portion::Chord,
            end: 200.0,
            ..Default::default()
        };
        assert!(outline(&c, 100.0, 100.0).is_some());
    }
}

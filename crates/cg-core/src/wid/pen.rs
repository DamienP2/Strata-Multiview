//! Pen: a Bézier path whose points and handles are fractions of the box.
//! The visible portion (start / end %, the animatable `drawStart` /
//! `drawEnd`) is what the In draws, over the pen's own duration after its
//! delay; arrows follow the visible tip. Drawn by `cg-render/src/wid/pen.rs` and
//! `web/src/widgets/pen.tsx` (keep in sync).

use serde::{Deserialize, Deserializer, Serialize};

use super::shape::{Cap, Cmd, FillRule, FillStyle, Join, PartialStroke, StrokeStyle};
use super::ActCtx;
use crate::model::Layer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PointType {
    #[default]
    Corner,
    Curve,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HandleMode {
    /// Same length, opposite.
    #[default]
    Symmetric,
    /// Opposite, own lengths.
    Asymmetric,
    /// Independent (a cusp).
    Broken,
}

/// One point; the handles are offsets from it. All fractions of the box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PenPoint {
    pub x: f32,
    pub y: f32,
    pub ix: f32,
    pub iy: f32,
    pub ox: f32,
    pub oy: f32,
    #[serde(rename = "type")]
    pub kind: PointType,
    pub mode: HandleMode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Arrow {
    #[default]
    None,
    Triangle,
    Open,
    Diamond,
    Circle,
    Bar,
}

/// Settings of the widget. The stroke colour and width are the animated
/// `stroke` / `strokeWidth`, the fill colour `fill`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub points: Vec<PenPoint>,
    pub closed: bool,
    /// Dash, ends and joins (the stroke is centred on the path); what a
    /// layer leaves out is round, as the web reads it.
    #[serde(deserialize_with = "round_partial")]
    pub stroke_style: StrokeStyle,
    /// The width stays the same when the box is resized…
    pub fixed_width: bool,
    /// …or scales from the size it was drawn at.
    pub ref_w: f32,
    pub ref_h: f32,
    pub arrow_start: Arrow,
    pub arrow_end: Arrow,
    /// Arrow size, × the stroke width.
    pub arrow_size: f32,
    /// Fill (closed paths only).
    pub fill_on: bool,
    pub fill_style: FillStyle,
    pub fill_rule: FillRule,
    /// Visible portion, % of the path length: the defaults of the
    /// animatable `drawStart` / `drawEnd` (older layers have only these).
    pub v_start: f32,
    pub v_end: f32,
    /// The In draws the stroke from start to end (the Out erases it).
    pub draw_in: bool,
    /// How long the drawing lasts, s (None = the template's In, 1 s without one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draw_duration: Option<f32>,
    /// Time on air before it starts drawing, s.
    pub draw_delay: f32,
}

/// Round ends and joins, the rest as any stroke.
fn round_stroke() -> StrokeStyle {
    StrokeStyle {
        cap: Cap::Round,
        join: Join::Round,
        ..Default::default()
    }
}

fn round_partial<'de, D: Deserializer<'de>>(d: D) -> Result<StrokeStyle, D::Error> {
    Ok(PartialStroke::deserialize(d)?.over(round_stroke()))
}

fn p(x: f32, y: f32, ix: f32, iy: f32, ox: f32, oy: f32) -> PenPoint {
    PenPoint {
        x,
        y,
        ix,
        iy,
        ox,
        oy,
        kind: PointType::Curve,
        mode: HandleMode::Symmetric,
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            points: vec![
                p(0.05, 0.85, 0.0, 0.0, 0.1, -0.55),
                p(0.5, 0.5, -0.14, -0.2, 0.14, 0.2),
                p(0.94, 0.14, -0.1, 0.55, 0.0, 0.0),
            ],
            closed: false,
            stroke_style: round_stroke(),
            fixed_width: true,
            ref_w: 1100.0,
            ref_h: 560.0,
            arrow_start: Arrow::None,
            arrow_end: Arrow::Triangle,
            arrow_size: 3.5,
            fill_on: false,
            fill_style: FillStyle::default(),
            fill_rule: FillRule::Nonzero,
            v_start: 0.0,
            v_end: 100.0,
            draw_in: true,
            draw_duration: None,
            draw_delay: 0.0,
        }
    }
}

pub type Pt = [f32; 2];

/// Cubic segments (p0, c1, c2, p1) in layer pixels.
pub fn segments(c: &Config, w: f32, h: f32) -> Vec<[Pt; 4]> {
    let n = c.points.len();
    if n < 2 {
        return Vec::new();
    }
    let seg = |a: &PenPoint, b: &PenPoint| {
        [
            [a.x * w, a.y * h],
            [(a.x + a.ox) * w, (a.y + a.oy) * h],
            [(b.x + b.ix) * w, (b.y + b.iy) * h],
            [b.x * w, b.y * h],
        ]
    };
    let mut out: Vec<[Pt; 4]> = c.points.windows(2).map(|q| seg(&q[0], &q[1])).collect();
    if c.closed {
        out.push(seg(&c.points[n - 1], &c.points[0]));
    }
    out
}

/// The Bézier outline (Move, Cubic…, Close when closed).
pub fn outline(c: &Config, w: f32, h: f32) -> Vec<Cmd> {
    let segs = segments(c, w, h);
    let Some(first) = segs.first() else {
        return Vec::new();
    };
    let mut out = vec![Cmd::Move(first[0][0], first[0][1])];
    for s in &segs {
        out.push(Cmd::Cubic(
            s[1][0], s[1][1], s[2][0], s[2][1], s[3][0], s[3][1],
        ));
    }
    if c.closed {
        out.push(Cmd::Close);
    }
    out
}

/// Steps of each cubic when flattened.
pub const STEPS: usize = 48;

/// The path as a polyline: points and the length travelled at each.
pub struct Poly {
    pub pts: Vec<Pt>,
    pub len: Vec<f32>,
}

impl Poly {
    pub fn total(&self) -> f32 {
        self.len.last().copied().unwrap_or(0.0)
    }
    /// Point at length `l` (clamped).
    pub fn point_at(&self, l: f32) -> Pt {
        let n = self.pts.len();
        if n == 0 {
            return [0.0, 0.0];
        }
        let l = l.clamp(0.0, self.total());
        let i = self.len.partition_point(|&x| x < l).clamp(1, n - 1);
        let (l0, l1) = (self.len[i - 1], self.len[i]);
        let k = if l1 > l0 { (l - l0) / (l1 - l0) } else { 0.0 };
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]
    }
    /// Unit direction (forward) at length `l`.
    pub fn dir_at(&self, l: f32) -> Pt {
        let e = (self.total() / 4.0).min(1.0);
        let a = self.point_at(l - e);
        let b = self.point_at(l + e);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let n = dx.hypot(dy);
        if n < 1e-6 {
            [1.0, 0.0]
        } else {
            [dx / n, dy / n]
        }
    }
    /// The part between the lengths `a` and `b`.
    pub fn cut(&self, a: f32, b: f32) -> Vec<Pt> {
        let mut out = vec![self.point_at(a)];
        for (i, &l) in self.len.iter().enumerate() {
            if l > a && l < b {
                out.push(self.pts[i]);
            }
        }
        out.push(self.point_at(b));
        out
    }
}

pub fn flatten(segs: &[[Pt; 4]]) -> Poly {
    let mut pts: Vec<Pt> = Vec::with_capacity(segs.len() * STEPS + 1);
    let mut len: Vec<f32> = Vec::with_capacity(segs.len() * STEPS + 1);
    for (si, s) in segs.iter().enumerate() {
        for j in usize::from(si > 0)..=STEPS {
            let t = j as f32 / STEPS as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            let q = [
                a * s[0][0] + b * s[1][0] + c * s[2][0] + d * s[3][0],
                a * s[0][1] + b * s[1][1] + c * s[2][1] + d * s[3][1],
            ];
            let l = match pts.last() {
                Some(&[x, y]) => len.last().copied().unwrap_or(0.0) + (q[0] - x).hypot(q[1] - y),
                None => 0.0,
            };
            pts.push(q);
            len.push(l);
        }
    }
    Poly { pts, len }
}

/// The stroke width: as set, or scaled with the box from the reference size.
pub fn stroke_width(c: &Config, base: f32, w: f32, h: f32) -> f32 {
    if c.fixed_width || c.ref_w <= 0.0 || c.ref_h <= 0.0 {
        base
    } else {
        base * ((w * h).max(0.0) / (c.ref_w * c.ref_h)).sqrt()
    }
}

fn ease(k: f64) -> f64 {
    if k < 0.5 {
        4.0 * k * k * k
    } else {
        1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
    }
}
fn ease_out(k: f64) -> f64 {
    1.0 - (1.0 - k).powi(3)
}

/// How long the drawing lasts, s: the pen's own duration, else the
/// template's In (its hold frame; 1 s without one).
pub fn draw_secs(c: &Config, hold: f64, fps: f64) -> f64 {
    match c.draw_duration {
        Some(d) if d.is_finite() && d > 0.0 => f64::from(d),
        _ if hold > 0.0 => hold / fps.max(1.0),
        _ => 1.0,
    }
}

/// Progress of the drawing: (In 0…1, Out 0…1). The In draws over the
/// pen's duration once its delay has passed on the time on air; the Out
/// erases over the same time after the hold.
pub fn progress(c: &Config, frame: f64, hold: f64, fps: f64, age_secs: f64) -> (f64, f64) {
    let fps = fps.max(1.0);
    let secs = draw_secs(c, hold, fps);
    let delay = f64::from(c.draw_delay.max(0.0));
    let k_in = ((age_secs - delay) / secs).clamp(0.0, 1.0);
    let k_out = if frame > hold + 1e-6 {
        ((frame - hold) / (secs * fps)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (k_in, k_out)
}

/// Visible portion in % (`start` / `end`: the `drawStart` / `drawEnd` at
/// this frame) with the In / Out applied, and the fill opacity.
pub fn portion(c: &Config, start: f32, end: f32, (k_in, k_out): (f64, f64)) -> (f32, f32, f32) {
    let mut a = start.clamp(0.0, 100.0) as f64;
    let mut b = end.clamp(0.0, 100.0) as f64;
    if !c.draw_in {
        return (a as f32, b as f32, 1.0);
    }
    b = a + (b - a) * ease_out(k_in);
    a += (b - a) * ease(k_out);
    let op = ease_out(k_in) * (1.0 - ease(k_out));
    (a as f32, b as f32, op as f32)
}

/// What an arrow head draws, in layer pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum Head {
    /// A filled polygon, also stroked (round joins) with this width (0 = none).
    Fill(Vec<Pt>, f32),
    /// An open polyline stroked with this width, round ends; `butt` = flat ends.
    Line(Vec<Pt>, f32, bool),
    Circle(Pt, f32),
}

/// Arrow head at `tip` pointing along `dir`; `inset` = how much the stroke
/// is shortened under it.
pub fn arrow(style: Arrow, tip: Pt, dir: Pt, size: f32, sw: f32) -> Option<(Head, f32)> {
    if style == Arrow::None || size <= 0.0 {
        return None;
    }
    let [x, y] = tip;
    let [dx, dy] = dir;
    let (nx, ny) = (-dy, dx);
    let at = |along: f32, side: f32| [x + dx * along + nx * side, y + dy * along + ny * side];
    let (l, wd) = (size, size * 0.9);
    Some(match style {
        Arrow::Triangle => (
            Head::Fill(
                vec![at(0.0, 0.0), at(-l, wd / 2.0), at(-l, -wd / 2.0)],
                sw * 0.15,
            ),
            l * 0.8,
        ),
        Arrow::Open => (
            Head::Line(
                vec![
                    at(-l * 0.9, wd / 2.0),
                    at(0.0, 0.0),
                    at(-l * 0.9, -wd / 2.0),
                ],
                sw,
                false,
            ),
            sw * 0.5,
        ),
        Arrow::Diamond => (
            Head::Fill(
                vec![
                    at(0.0, 0.0),
                    at(-l / 2.0, wd / 3.0),
                    at(-l, 0.0),
                    at(-l / 2.0, -wd / 3.0),
                ],
                0.0,
            ),
            l * 0.5,
        ),
        Arrow::Circle => (
            Head::Circle([x - dx * l * 0.3, y - dy * l * 0.3], l * 0.32),
            l * 0.3,
        ),
        Arrow::Bar => (
            Head::Line(
                vec![at(0.0, wd / 2.0), at(0.0, -wd / 2.0)],
                sw.max(4.0),
                true,
            ),
            0.0,
        ),
        Arrow::None => return None,
    })
}

/// The visible stroke: its polyline (None = the whole Bézier path), the dash
/// offset, and the arrow heads.
pub struct Visible {
    pub line: Option<Vec<Pt>>,
    pub dash_offset: f32,
    pub heads: Vec<Head>,
}

/// What is drawn of the stroke between `pa` % and `pb` %; None = nothing.
pub fn visible(c: &Config, poly: &Poly, pa: f32, pb: f32, sw: f32) -> Option<Visible> {
    let t = poly.total();
    let (a, b) = (t * pa / 100.0, t * pb / 100.0);
    if b - a <= 0.5 || sw <= 0.0 {
        return None;
    }
    let size = sw * c.arrow_size;
    let head = arrow(c.arrow_end, poly.point_at(b), poly.dir_at(b), size, sw);
    let d = poly.dir_at(a);
    let tail = arrow(c.arrow_start, poly.point_at(a), [-d[0], -d[1]], size, sw);
    let room = (b - a) * 0.45;
    let hi = head.as_ref().map_or(0.0, |h| h.1);
    let ti = tail.as_ref().map_or(0.0, |h| h.1);
    let (sa, sb) = (a + ti.min(room), b - hi.min(room));
    let full = a <= 0.01 && b >= t - 0.01 && hi == 0.0 && ti == 0.0;
    Some(Visible {
        line: (!full).then(|| poly.cut(sa, sb)),
        dash_offset: sa,
        heads: head.into_iter().chain(tail).map(|h| h.0).collect(),
    })
}

/// No operator state: the stroke draws itself with the In.
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
    fn single_point_draws_nothing() {
        let c = Config {
            points: vec![PenPoint::default()],
            ..Default::default()
        };
        assert!(segments(&c, 100.0, 100.0).is_empty());
        assert!(outline(&c, 100.0, 100.0).is_empty());
    }

    #[test]
    fn length_and_cut() {
        let c = Config {
            points: vec![
                PenPoint::default(),
                PenPoint {
                    x: 1.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let poly = flatten(&segments(&c, 200.0, 100.0));
        assert!((poly.total() - 200.0).abs() < 1e-2);
        assert_eq!(poly.point_at(50.0), [50.0, 0.0]);
        let cut = poly.cut(20.0, 120.0);
        assert_eq!(cut.first(), Some(&[20.0, 0.0]));
        assert_eq!(cut.last(), Some(&[120.0, 0.0]));
    }

    #[test]
    fn arrow_rides_the_tip() {
        let c = Config {
            points: vec![
                PenPoint::default(),
                PenPoint {
                    x: 1.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let poly = flatten(&segments(&c, 1000.0, 100.0));
        // Half way through the In: the head is at the visible tip, not at the end.
        let (a, b, _) = portion(&c, c.v_start, c.v_end, (0.5, 0.0));
        let v = visible(&c, &poly, a, b, 10.0).unwrap();
        let Head::Fill(pts, _) = &v.heads[0] else {
            panic!()
        };
        assert!((pts[0][0] - 1000.0 * b / 100.0).abs() < 1e-2);
        assert!(pts[0][0] < 1000.0);
    }

    #[test]
    fn timing() {
        let c = Config::default();
        // On air for half the In.
        assert_eq!(progress(&c, 12.5, 25.0, 50.0, 0.25).0, 0.5);
        // Out half way.
        assert_eq!(progress(&c, 37.5, 25.0, 50.0, 10.0), (1.0, 0.5));
        // Its own duration and delay: 2 s after 1 s.
        let c = Config {
            draw_duration: Some(2.0),
            draw_delay: 1.0,
            ..Default::default()
        };
        assert_eq!(progress(&c, 25.0, 25.0, 50.0, 0.5).0, 0.0);
        assert_eq!(progress(&c, 25.0, 25.0, 50.0, 2.0).0, 0.5);
        assert_eq!(progress(&c, 75.0, 25.0, 50.0, 9.0), (1.0, 0.5));
    }

    #[test]
    fn partial_stroke_style_stays_round() {
        let l: Layer = serde_json::from_str(
            r#"{"id":"a","name":"P","type":"pen","props":{},"strokeStyle":{"dash":"dashed"}}"#,
        )
        .unwrap();
        let crate::LayerKind::Pen { x } = l.kind else {
            panic!()
        };
        assert_eq!(x.stroke_style.cap, Cap::Round);
        assert_eq!(x.stroke_style.join, Join::Round);
        assert_eq!(x.stroke_style.dash, super::super::shape::Dash::Dashed);
    }

    #[test]
    fn json_loads() {
        let l: Layer = serde_json::from_str(
            r#"{"id":"a","name":"P","type":"pen","props":{},"points":[{"x":0.1,"y":0.2,"type":"curve","mode":"broken"}],"arrowEnd":"diamond"}"#,
        )
        .unwrap();
        let crate::LayerKind::Pen { x } = l.kind else {
            panic!()
        };
        assert_eq!(x.points[0].kind, PointType::Curve);
        assert_eq!(x.points[0].mode, HandleMode::Broken);
        assert_eq!(x.arrow_end, Arrow::Diamond);
    }
}

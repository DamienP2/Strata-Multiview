//! Keyframe interpolation curves (After Effects / CSS-like).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Easing {
    Linear,
    /// Step: keep the value until the next keyframe.
    Hold,
    EaseIn,
    EaseOut,
    #[default]
    EaseInOut,
    /// Free cubic bezier, same convention as CSS `cubic-bezier()`.
    Bezier {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    /// Damped overshoot, popular for broadcast "pop-in" moves.
    Back,
    Elastic,
    Bounce,
}

impl Easing {
    /// Maps linear progress `t` in 0..=1 to eased progress.
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::Hold => 0.0,
            // Same control points as the web renderer (and AE "easy ease").
            Easing::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, t),
            Easing::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, t),
            Easing::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
            Easing::Bezier { x1, y1, x2, y2 } => cubic_bezier(x1, y1, x2, y2, t),
            Easing::Back => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
            Easing::Elastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    let c4 = (2.0 * std::f64::consts::PI) / 3.0;
                    2f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
                }
            }
            Easing::Bounce => bounce_out(t),
        }
    }
}

fn bounce_out(t: f64) -> f64 {
    let n1 = 7.5625;
    let d1 = 2.75;
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

/// Solves a CSS-style cubic bezier for `x` and returns `y`.
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    let x1 = x1.clamp(0.0, 1.0);
    let x2 = x2.clamp(0.0, 1.0);
    let bez = |a: f64, b: f64, t: f64| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * a + 3.0 * mt * t * t * b + t * t * t
    };
    let dbez = |a: f64, b: f64, t: f64| {
        let mt = 1.0 - t;
        3.0 * mt * mt * a + 6.0 * mt * t * (b - a) + 3.0 * t * t * (1.0 - b)
    };
    // Newton-Raphson, falling back to bisection.
    let mut t = x;
    for _ in 0..8 {
        let err = bez(x1, x2, t) - x;
        if err.abs() < 1e-7 {
            return bez(y1, y2, t);
        }
        let d = dbez(x1, x2, t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= err / d;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    t = x;
    for _ in 0..40 {
        let v = bez(x1, x2, t);
        if (v - x).abs() < 1e-7 {
            break;
        }
        if v < x {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) / 2.0;
    }
    bez(y1, y2, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints() {
        for e in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::Back,
            Easing::Bounce,
        ] {
            assert!((e.apply(0.0)).abs() < 1e-6, "{e:?}");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-6, "{e:?}");
        }
    }

    #[test]
    fn ease_in_out_is_symmetric() {
        let a = Easing::EaseInOut.apply(0.25);
        let b = Easing::EaseInOut.apply(0.75);
        assert!((a + b - 1.0).abs() < 1e-5);
    }
}

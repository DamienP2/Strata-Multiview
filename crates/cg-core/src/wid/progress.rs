//! Progress bar: settings, operator state and actions.
//! Drawn by `cg-render/src/wid/progress.rs` and `web/src/widgets/progress.tsx` (keep in sync).
//!
//! The legacy fields of the layer stay: `value` and `max` (text, `{{key}}`
//! allowed), `duration` (the transition, seconds), `track` (colour of the
//! empty part). The bar is the `fill` colour, square ends use `cornerRadius`,
//! the outline is `stroke` / `strokeWidth`. An older layer (no new field)
//! looks as before: a horizontal bar filling its whole box from the left.

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shape {
    #[default]
    Linear,
    Ring,
    Segments,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orient {
    #[default]
    H,
    V,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Input {
    #[default]
    Value,
    Percent,
    /// The value of another widget of the graphic (`follow`): a countdown's
    /// seconds left, a stopwatch's seconds, a counter's value.
    Follow,
}

/// Linear fill: from the start (left / bottom), the end, or the centre.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dir {
    #[default]
    Start,
    End,
    Center,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CircleDir {
    #[default]
    Cw,
    Ccw,
    Center,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SegPartial {
    #[default]
    Partial,
    Round,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Label {
    #[default]
    None,
    Pct,
    Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LabelPos {
    #[default]
    Inside,
    End,
    Beside,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Easing {
    Linear,
    #[default]
    Out,
    Inout,
    Back,
}

/// Under `below` %, the bar takes `color`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ColorStep {
    pub on: bool,
    pub below: f64,
    pub color: String,
}

impl Default for ColorStep {
    fn default() -> Self {
        Self {
            on: true,
            below: 20.0,
            color: "#ff3b5c".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub shape: Shape,
    pub orient: Orient,
    pub input: Input,
    /// Layer id of the widget followed (input Follow).
    pub follow: String,
    /// Text, `{{key}}` allowed ("" = 0).
    pub min: String,
    /// Operator + / − step.
    pub step: f64,
    pub unit: String,
    pub decimals: u8,
    pub dir: Dir,
    /// Ring: start angle, degrees (0 = top).
    pub angle: f32,
    pub circle_dir: CircleDir,
    pub segments: u32,
    pub seg_gap: f32,
    pub seg_partial: SegPartial,
    /// Gradient from `fill` to `color2` (else plain `fill`).
    pub gradient: bool,
    pub color2: String,
    /// Bar thickness, px (None = the whole box across).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thickness: Option<f32>,
    /// Round ends (else `cornerRadius`).
    pub rounded: bool,
    pub steps: Vec<ColorStep>,
    pub label: Label,
    pub label_pos: LabelPos,
    pub label_size: f32,
    pub label_color: String,
    pub easing: Easing,
    /// Goal marker: its value (text, `{{key}}` allowed; empty = none), its label and colour.
    pub goal: String,
    pub goal_text: String,
    pub goal_color: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shape: Shape::Linear,
            orient: Orient::H,
            input: Input::Value,
            follow: String::new(),
            min: String::new(),
            step: 5.0,
            unit: String::new(),
            decimals: 0,
            dir: Dir::Start,
            angle: 0.0,
            circle_dir: CircleDir::Cw,
            segments: 10,
            seg_gap: 10.0,
            seg_partial: SegPartial::Partial,
            gradient: false,
            color2: "#22c55e".into(),
            thickness: None,
            rounded: false,
            steps: Vec::new(),
            label: Label::None,
            label_pos: LabelPos::Inside,
            label_size: 30.0,
            label_color: "#ffffff".into(),
            easing: Easing::Out,
            goal: String::new(),
            goal_text: String::new(),
            goal_color: "#ffd200".into(),
        }
    }
}

/// The value the operator set, and the transition towards it. No value
/// but a time: back to the data, from the value shown then.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// None = the layer's value (data).
    pub value: Option<f64>,
    /// Value shown when it changed, and when (Unix ms).
    pub from: f64,
    pub at: i64,
}

/// A number typed or bound ("1 250,5", "64 %"); None when it is not one.
pub fn parse_num(s: &str) -> Option<f64> {
    let t: String = s
        .trim()
        .trim_end_matches('%')
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{202f}')
        .collect();
    let t = t.replace(',', ".");
    let v: f64 = t.parse().ok()?;
    v.is_finite().then_some(v)
}

fn number(raw: &str, data: &Data) -> f64 {
    parse_num(&interpolate_text(raw, data)).unwrap_or(0.0)
}

pub fn ease(e: Easing, k: f64) -> f64 {
    let k = k.clamp(0.0, 1.0);
    match e {
        Easing::Linear => k,
        Easing::Out => 1.0 - (1.0 - k).powi(3),
        Easing::Inout => {
            if k < 0.5 {
                4.0 * k * k * k
            } else {
                1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
            }
        }
        Easing::Back => 1.0 + 2.70158 * (k - 1.0).powi(3) + 1.70158 * (k - 1.0).powi(2),
    }
}

/// Resolved inputs of a progress layer.
#[derive(Debug, Clone)]
pub struct Eff<'a> {
    pub c: &'a Config,
    pub lo: f64,
    pub hi: f64,
    /// The layer's own value (data).
    pub base: f64,
    pub duration: f64,
    pub track: &'a str,
    /// Goal value (None = no marker).
    pub goal: Option<f64>,
}

/// The value of the widget a progress follows now, and its natural end (a
/// countdown's duration, a stopwatch's limit), from the graphic's layers and
/// the operator's widget states.
pub fn followed(
    layers: &[Layer],
    c: &Config,
    widgets: &std::collections::BTreeMap<String, serde_json::Value>,
    data: &Data,
    now: i64,
    age_secs: f64,
    tz: &str,
) -> Option<(f64, Option<f64>)> {
    use super::{counter, timer};
    if c.input != Input::Follow {
        return None;
    }
    let l = layers.iter().find(|l| l.id == c.follow)?;
    let state = || {
        widgets
            .get(&l.id)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    };
    if let Some(e) = timer::eff(l, data) {
        let st: timer::State = serde_json::from_value(state()).unwrap_or_default();
        let r = timer::run(&e, &st, now, (age_secs * 1000.0) as i64, tz);
        let v = timer::value(&e, &r, now, tz)?;
        let secs = v.v as f64 / 1000.0;
        return match e.mode {
            timer::TimerMode::Countdown => Some((
                if v.over { 0.0 } else { secs.max(0.0) },
                (!e.cd_time).then_some(e.duration as f64 / 1000.0),
            )),
            timer::TimerMode::Stopwatch => Some((
                secs,
                e.c.sw_limit_on.then_some(e.c.sw_limit as f64 / 1000.0),
            )),
            _ => None,
        };
    }
    if let Some(p) = counter::parts(l) {
        let st: counter::State = serde_json::from_value(state()).unwrap_or_default();
        return Some((counter::display(&p, &st, data, now, age_secs, None).0, None));
    }
    None
}

pub fn eff<'a>(layer: &'a Layer, data: &Data) -> Option<Eff<'a>> {
    eff_with(layer, data, None)
}

/// [`eff`] with the followed widget's value (input Follow).
pub fn eff_with<'a>(
    layer: &'a Layer,
    data: &Data,
    follow: Option<(f64, Option<f64>)>,
) -> Option<Eff<'a>> {
    let LayerKind::Progress {
        value,
        max,
        duration,
        track,
        x: c,
    } = &layer.kind
    else {
        return None;
    };
    let (lo, hi) = match c.input {
        Input::Percent => (0.0, 100.0),
        Input::Value => (number(&c.min, data), number(max, data)),
        // The followed widget's own end unless a max is set.
        Input::Follow => (
            number(&c.min, data),
            match follow.and_then(|f| f.1) {
                Some(end) if max.trim().is_empty() => end,
                _ => number(max, data),
            },
        ),
    };
    Some(Eff {
        c,
        lo,
        hi,
        base: match (c.input, follow) {
            (Input::Follow, Some((v, _))) => v,
            (Input::Follow, None) => 0.0,
            _ => number(value, data),
        },
        duration: *duration,
        track,
        goal: if c.goal.trim().is_empty() {
            None
        } else {
            parse_num(&interpolate_text(&c.goal, data))
        },
    })
}

/// Fraction of a value: (0..1 capped, min = max (nothing drawn), raw fraction).
pub fn frac(e: &Eff, v: f64) -> (f64, bool, f64) {
    let span = e.hi - e.lo;
    if span == 0.0 || !span.is_finite() || !v.is_finite() {
        return (0.0, true, 0.0);
    }
    let f = (v - e.lo) / span;
    (f.clamp(0.0, 1.0), false, f)
}

impl Eff<'_> {
    /// The target value now.
    pub fn target(&self, st: &State) -> f64 {
        st.value.unwrap_or(self.base)
    }
    fn capped(&self, v: f64) -> f64 {
        if self.hi > self.lo {
            v.clamp(self.lo, self.hi)
        } else if self.hi < self.lo {
            v.clamp(self.hi, self.lo)
        } else {
            v
        }
    }
    /// The value drawn at `now` (`age`: seconds on air): grows from the
    /// min at the In, then eases from value to value.
    pub fn shown(&self, st: &State, now: i64, age: f64) -> f64 {
        let d = self.duration.max(0.0);
        match st.value {
            // A value set by the operator, or back to the data (from the value shown then).
            v if v.is_some() || st.at != 0 => {
                let k = if d <= 0.0 {
                    1.0
                } else {
                    (now - st.at) as f64 / (d * 1000.0)
                };
                let (a, b) = (self.capped(st.from), self.capped(v.unwrap_or(self.base)));
                a + (b - a) * ease(self.c.easing, k)
            }
            _ => {
                let k = if d <= 0.0 { 1.0 } else { age / d };
                let b = self.capped(self.base);
                self.lo + (b - self.lo) * ease(self.c.easing, k)
            }
        }
    }
    /// Colour step for a fraction (lowest matching wins), if any.
    pub fn step_color(&self, f: f64) -> Option<&str> {
        let mut best: Option<&ColorStep> = None;
        for s in self.c.steps.iter().filter(|s| s.on) {
            if f * 100.0 < s.below && best.is_none_or(|b| s.below < b.below) {
                best = Some(s);
            }
        }
        best.map(|s| s.color.as_str()).filter(|s| !s.is_empty())
    }
    /// The label ("" = none).
    pub fn label(&self, v: f64, f: f64) -> String {
        if self.c.label == Label::None {
            return String::new();
        }
        if frac(self, v).1 {
            return "—".into();
        }
        let dec = self.c.decimals.min(3) as usize;
        match self.c.label {
            Label::Pct => format!("{} %", fmt_num(f * 100.0, dec)),
            _ => {
                let unit = if self.c.unit.is_empty() {
                    String::new()
                } else {
                    format!(" {}", self.c.unit)
                };
                format!(
                    "{} / {}{unit}",
                    fmt_num(v, dec),
                    fmt_num(self.hi, dec_of(self.hi))
                )
            }
        }
    }
}

/// Decimals a number needs to show as typed (0–3): 100 → 0, 2.5 → 1.
pub fn dec_of(v: f64) -> usize {
    (0..3)
        .find(|&d| {
            let x = v * 10f64.powi(d as i32);
            (x - x.round()).abs() < 1e-6
        })
        .unwrap_or(3)
}

/// "5 000.5": thousands with a space, `dec` decimals.
pub fn fmt_num(v: f64, dec: usize) -> String {
    let s = format!("{:.*}", dec, v.abs());
    let (int, frac) = match s.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (s.clone(), None),
    };
    let digits: Vec<char> = int.chars().collect();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(*c);
    }
    let neg = v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0');
    format!(
        "{}{out}{}",
        if neg { "-" } else { "" },
        frac.map(|f| format!(".{f}")).unwrap_or_default()
    )
}

/// Actions: `set {value}` (number or text), `step {by}` (default: the
/// step), `inc`, `dec`, `reset` (back to the min), `data` (back to the
/// data: the layer's value, or the widget followed). Following a widget,
/// `step` / `inc` / `dec` are refused and `reset` is `data`.
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let e = eff(layer, ctx.data).ok_or("not a progress bar")?;
    let follow = e.c.input == Input::Follow;
    let op = if follow && op == "reset" { "data" } else { op };
    if op == "data" {
        // The followed value is not known here: the bar takes it back at once.
        let from = if follow {
            0.0
        } else {
            e.shown(st, ctx.now_ms, ctx.age_secs)
        };
        *st = if follow || st.value.is_none() {
            State::default()
        } else {
            State {
                value: None,
                from,
                at: ctx.now_ms,
            }
        };
        return Ok(());
    }
    if follow && matches!(op, "step" | "inc" | "dec") {
        return Err("The bar follows a timer or a counter: + / − are off".into());
    }
    let cur = e.target(st);
    let next = match op {
        "set" => {
            let v = args.get("value").unwrap_or(args);
            match v {
                serde_json::Value::Number(n) => n.as_f64().ok_or("not a number")?,
                serde_json::Value::String(s) if s.trim().is_empty() => {
                    return Err("Empty value".into())
                }
                serde_json::Value::String(s) => {
                    parse_num(s).ok_or_else(|| format!("“{}” is not a number", s.trim()))?
                }
                _ => return Err("set needs {\"value\": n}".into()),
            }
        }
        "step" | "inc" | "dec" => {
            let by = args
                .get("by")
                .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(parse_num)))
                .unwrap_or(e.c.step);
            cur + if op == "dec" { -by } else { by }
        }
        "reset" => e.lo,
        _ => return Err(format!("unknown action `{op}`")),
    };
    if !next.is_finite() {
        return Err("not a number".into());
    }
    let from = e.shown(st, ctx.now_ms, ctx.age_secs);
    *st = State {
        value: Some((next * 1e6).round() / 1e6),
        from,
        at: ctx.now_ms,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn layer(json: &str) -> Layer {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_progress_follows_a_countdown() {
        let cd = layer(
            r#"{"id":"cd","name":"C","type":"clock","mode":"timer","format":"mm:ss","timerMode":"countdown","cdDuration":10000,"startMode":"auto","props":{}}"#,
        );
        let p = layer(
            r##"{"id":"p","name":"P","type":"progress","value":"","max":"","duration":0,"track":"#ffffff33","input":"follow","follow":"cd","props":{}}"##,
        );
        let LayerKind::Progress { x: c, .. } = &p.kind else {
            panic!()
        };
        let layers = [cd, p.clone()];
        let now = 1_700_000_000_000;
        let f = followed(&layers, c, &BTreeMap::new(), &Data::new(), now, 4.0, "").unwrap();
        assert_eq!(f, (6.0, Some(10.0)));
        let e = eff_with(&p, &Data::new(), Some(f)).unwrap();
        assert_eq!((e.lo, e.hi, e.base), (0.0, 10.0, 6.0));
        assert!((frac(&e, e.base).0 - 0.6).abs() < 1e-9);
    }

    #[test]
    fn old_layers_load() {
        let l = layer(
            r##"{"id":"a","name":"P","type":"progress","value":"{{v}}","max":"200","duration":1,"track":"#ffffff33","props":{}}"##,
        );
        let LayerKind::Progress { x, .. } = &l.kind else {
            panic!()
        };
        assert_eq!(x, &Config::default());
        let mut d = BTreeMap::new();
        d.insert("v".to_string(), "50".to_string());
        let e = eff(&l, &d).unwrap();
        // Grows from empty (ease out) like before.
        assert_eq!(frac(&e, e.shown(&State::default(), 0, 1.0)).0, 0.25);
        assert_eq!(frac(&e, e.shown(&State::default(), 0, 0.0)).0, 0.0);
    }

    #[test]
    fn min_eq_max_and_caps() {
        let l = layer(
            r#"{"id":"a","name":"P","type":"progress","value":"50","max":"50","min":"50","props":{}}"#,
        );
        let d = BTreeMap::new();
        let e = eff(&l, &d).unwrap();
        let (f, bad, _) = frac(&e, 50.0);
        assert!(bad && f == 0.0);
        let l = layer(r#"{"id":"a","name":"P","type":"progress","value":"135","props":{}}"#);
        let e = eff(&l, &d).unwrap();
        assert_eq!(frac(&e, 135.0).0, 1.0);
        assert_eq!(frac(&e, -5.0).0, 0.0);
    }

    #[test]
    fn actions() {
        let l = layer(
            r#"{"id":"a","name":"P","type":"progress","value":"64","duration":0.5,"step":5,"props":{}}"#,
        );
        let d = BTreeMap::new();
        let ctx = ActCtx {
            now_ms: 1000,
            age_secs: 10.0,
            fps: 50.0,
            data: &d,
        };
        let mut st = State::default();
        act(&l, &mut st, "step", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.value, Some(69.0));
        assert_eq!(st.from, 64.0);
        act(&l, &mut st, "dec", &serde_json::json!({"by": 10}), &ctx).unwrap();
        assert_eq!(st.value, Some(59.0));
        assert!(act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": "abc"}),
            &ctx
        )
        .is_err());
        assert!(act(&l, &mut st, "set", &serde_json::json!({"value": ""}), &ctx).is_err());
        act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": "12,5 %"}),
            &ctx,
        )
        .unwrap();
        assert_eq!(st.value, Some(12.5));
        act(&l, &mut st, "reset", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.value, Some(0.0));
        let e = eff(&l, &d).unwrap();
        assert_eq!(e.shown(&st, 1000 + 500, 0.0), 0.0);
        assert_eq!(fmt_num(5000.0, 0), "5 000");
        assert_eq!(fmt_num(12.345, 1), "12.3");
    }

    #[test]
    fn back_to_the_data_and_follow() {
        let l = layer(
            r#"{"id":"a","name":"P","type":"progress","value":"{{v}}","max":"2.5","duration":1,"step":0.5,"label":"value","goal":"2","props":{}}"#,
        );
        let mut d = BTreeMap::new();
        d.insert("v".to_string(), "1".to_string());
        let ctx = ActCtx {
            now_ms: 10_000,
            age_secs: 10.0,
            fps: 50.0,
            data: &d,
        };
        let mut st = State::default();
        act(&l, &mut st, "inc", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.value, Some(1.5));
        // An Update arrives: the bar keeps the operator's value until « Données ».
        d.insert("v".to_string(), "2".to_string());
        let e = eff(&l, &d).unwrap();
        assert_eq!(e.shown(&st, 20_000, 20.0), 1.5);
        let ctx = ActCtx {
            now_ms: 20_000,
            age_secs: 20.0,
            fps: 50.0,
            data: &d,
        };
        act(&l, &mut st, "data", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.value, None);
        // Tweens from 1.5 to the data's 2 over the duration.
        let mid = e.shown(&st, 20_500, 20.5);
        assert!(mid > 1.5 && mid < 2.0, "{mid}");
        assert_eq!(e.shown(&st, 21_000, 21.0), 2.0);
        // The max keeps its decimals; the goal is read.
        assert_eq!(e.label(2.0, 0.8), "2 / 2.5");
        assert_eq!(e.goal, Some(2.0));
        assert_eq!(dec_of(100.0), 0);
        assert_eq!(dec_of(2.25), 2);

        let f = layer(
            r#"{"id":"b","name":"P","type":"progress","value":"","max":"","input":"follow","follow":"cd","props":{}}"#,
        );
        let mut st = State::default();
        assert!(act(&f, &mut st, "inc", &serde_json::Value::Null, &ctx).is_err());
        act(&f, &mut st, "set", &serde_json::json!({"value": 3}), &ctx).unwrap();
        assert_eq!(st.value, Some(3.0));
        // Reset follows again.
        act(&f, &mut st, "reset", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st, State::default());
    }
}

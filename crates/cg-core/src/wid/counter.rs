//! Counter: settings, operator state and actions, number format and the
//! displayed value (transitions) both renderers use.
//! Drawn by `cg-render/src/wid/counter.rs` and `web/src/widgets/counter.tsx` (keep in sync).
//!
//! The value is the layer's `to` (a number, or `{{key}}` wired by the Data
//! canvas: the linked source). On air it counts up from `from` over
//! `duration` seconds (as older counters did), then every new value rolls
//! from the value displayed. The operator's value (+ / − step, set) holds
//! until the source sends another value; the freeze holds the display while
//! the source keeps being read.

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AffixRaise {
    #[default]
    Baseline,
    Top,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reserve {
    #[default]
    Off,
    /// The width of the widest of min and max.
    Max,
    /// `reserveChars` digits.
    Chars,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Transition {
    /// Cut.
    None,
    /// Numeric roll.
    #[default]
    Roll,
    Odometer,
    Flip,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Curve {
    Linear,
    #[default]
    Out,
    InOut,
    Back,
}

impl Curve {
    pub fn apply(self, k: f64) -> f64 {
        let k = k.clamp(0.0, 1.0);
        match self {
            Curve::Linear => k,
            Curve::Out => 1.0 - (1.0 - k).powi(3),
            Curve::InOut => {
                if k < 0.5 {
                    4.0 * k * k * k
                } else {
                    1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
                }
            }
            Curve::Back => 1.0 + 2.2 * (k - 1.0).powi(3) + 1.2 * (k - 1.0).powi(2),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorMode {
    #[default]
    Fixed,
    Thresholds,
    /// Green going up, red going down.
    Trend,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Threshold {
    pub from: String,
    pub color: String,
}

/// Settings of the widget, flattened into the layer's JSON (every field has
/// a default: an older counter counts up as before). The number's colour
/// and size are the animated `fill` and `fontSize`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Limits (none = unlimited).
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Of the + / − buttons.
    pub step: f64,
    /// More + buttons of the Operator ("2, 3": +2 and +3).
    pub extra_steps: String,
    /// The Operator's « Freeze the display » row is exposed through this
    /// path (the value itself is not read).
    pub op_freeze: bool,
    /// fr-FR, en-US, de-DE, de-CH, es-ES, en-IN.
    pub locale: String,
    /// + on positive values.
    pub plus: bool,
    /// Short notation (1,2 M).
    pub compact: bool,
    /// Prefix / suffix size, % of the number's.
    pub affix_size: f32,
    /// Empty = the number's colour.
    pub affix_color: String,
    /// 0 = the number's weight.
    pub affix_weight: u16,
    pub affix_raise: AffixRaise,
    pub affix_gap: f32,
    /// Fixed-width figures.
    pub tabular: bool,
    pub reserve: Reserve,
    pub reserve_chars: u32,
    pub transition: Transition,
    /// Of a change, ms.
    pub trans_ms: f64,
    pub easing: Curve,
    pub color_mode: ColorMode,
    pub thresholds: Vec<Threshold>,
    pub up_color: String,
    pub down_color: String,
    /// Seconds the trend colour stays (0 = until the next change).
    pub trend_hold: f64,
    pub plate: bool,
    pub plate_color: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            step: 1.0,
            extra_steps: String::new(),
            op_freeze: false,
            locale: "fr-FR".into(),
            plus: false,
            compact: false,
            affix_size: 100.0,
            affix_color: String::new(),
            affix_weight: 0,
            affix_raise: AffixRaise::Baseline,
            affix_gap: 0.0,
            tabular: false,
            reserve: Reserve::Off,
            reserve_chars: 9,
            transition: Transition::Roll,
            trans_ms: 1200.0,
            easing: Curve::Out,
            color_mode: ColorMode::Fixed,
            thresholds: Vec::new(),
            up_color: "#22c55e".into(),
            down_color: "#ff3b5c".into(),
            trend_hold: 2.0,
            plate: false,
            plate_color: "#160f33".into(),
        }
    }
}

/// The extra steps of the Operator's row ("2, 3; 5" → [2, 3, 5]), positive, at most 4.
pub fn extra_steps(c: &Config) -> Vec<f64> {
    c.extra_steps
        .split([',', ';', ' '])
        .filter_map(|s| parse_num(s.trim()))
        .filter(|v| *v > 0.0 && v.is_finite())
        .take(4)
        .collect()
}

/// A transition of the displayed value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Anim {
    pub from: f64,
    pub to: f64,
    /// Wall clock, ms.
    pub t0: i64,
    /// ms.
    pub dur: f64,
}

impl Anim {
    pub fn progress(&self, now_ms: i64) -> f64 {
        if self.dur <= 0.0 {
            1.0
        } else {
            ((now_ms - self.t0) as f64 / self.dur).clamp(0.0, 1.0)
        }
    }
    pub fn value(&self, now_ms: i64, curve: Curve) -> f64 {
        let k = self.progress(now_ms);
        if k >= 1.0 {
            self.to
        } else {
            self.from + (self.to - self.from) * curve.apply(k)
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Alert {
    /// warn | error.
    pub level: String,
    pub text: String,
    pub at: i64,
}

/// What the operator did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// The operator's value (None = the source's).
    pub value: Option<f64>,
    /// The source's raw value when the operator set his: a new one wins.
    pub src_seen: Option<String>,
    /// Transition started by the last action.
    pub anim: Option<Anim>,
    /// The display holds `value` (the source is still read).
    pub frozen: bool,
    pub alert: Option<Alert>,
}

/// What the renderer remembers of the source (last valid value, the
/// transition it started when the source changed). Kept by each renderer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mem {
    pub target: Option<f64>,
    pub anim: Option<Anim>,
    pub age: f64,
    /// Wall clock of the last frame drawn (a stale memory is dropped).
    pub seen: i64,
}

/// A number as typed or sent: spaces, ’ and ' ignored, `,` or `.` decimal.
pub fn parse_num(raw: &str) -> Option<f64> {
    let t: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\'' | '’' | '\u{202f}' | '\u{a0}'))
        .map(|c| if c == '−' { '-' } else { c })
        .collect();
    let t = match (t.rfind(','), t.rfind('.')) {
        (Some(a), Some(b)) if a > b => t.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => t.replace(',', ""),
        (Some(_), None) if t.matches(',').count() > 1 => t.replace(',', ""),
        (Some(_), None) => t.replace(',', "."),
        (None, Some(_)) if t.matches('.').count() > 1 => t.replace('.', ""),
        _ => t,
    };
    let ok = !t.is_empty()
        && t.chars()
            .enumerate()
            .all(|(i, c)| c.is_ascii_digit() || c == '.' || (i == 0 && (c == '+' || c == '-')))
        && t.chars().any(|c| c.is_ascii_digit());
    if !ok {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

struct Loc {
    group: &'static str,
    dec: &'static str,
    /// Grouping starts at this many integer digits (es: 5).
    min_digits: usize,
    indian: bool,
    compact: [(f64, &'static str); 3],
}

fn loc(l: &str) -> Loc {
    match l {
        "en-US" => Loc {
            group: ",",
            dec: ".",
            min_digits: 4,
            indian: false,
            compact: [(1e9, "B"), (1e6, "M"), (1e3, "K")],
        },
        "de-DE" => Loc {
            group: ".",
            dec: ",",
            min_digits: 4,
            indian: false,
            compact: [
                (1e9, "\u{a0}Mrd."),
                (1e6, "\u{a0}Mio."),
                (1e3, "\u{a0}Tsd."),
            ],
        },
        "de-CH" => Loc {
            group: "’",
            dec: ".",
            min_digits: 4,
            indian: false,
            compact: [
                (1e9, "\u{a0}Mrd."),
                (1e6, "\u{a0}Mio."),
                (1e3, "\u{a0}Tsd."),
            ],
        },
        "es-ES" => Loc {
            group: ".",
            dec: ",",
            min_digits: 5,
            indian: false,
            compact: [
                (1e9, "\u{a0}mil\u{a0}M"),
                (1e6, "\u{a0}M"),
                (1e3, "\u{a0}mil"),
            ],
        },
        "en-IN" => Loc {
            group: ",",
            dec: ".",
            min_digits: 4,
            indian: true,
            compact: [(1e7, "Cr"), (1e5, "L"), (1e3, "K")],
        },
        _ => Loc {
            group: "\u{202f}",
            dec: ",",
            min_digits: 4,
            indian: false,
            compact: [(1e9, "\u{a0}Md"), (1e6, "\u{a0}M"), (1e3, "\u{a0}k")],
        },
    }
}

/// How numbers are written.
#[derive(Debug, Clone, PartialEq)]
pub struct Fmt {
    pub decimals: u8,
    pub grouping: bool,
    pub locale: String,
    pub plus: bool,
    pub compact: bool,
}

/// Does a separator go before the digit at `i` of `n` integer digits?
pub fn group_before(f: &Fmt, n: usize, i: usize) -> bool {
    let l = loc(&f.locale);
    if !f.grouping || i == 0 || n < l.min_digits {
        return false;
    }
    let from_right = n - i;
    if l.indian {
        from_right == 3 || (from_right > 3 && (from_right - 3).is_multiple_of(2))
    } else {
        from_right.is_multiple_of(3)
    }
}

pub fn group_sep(f: &Fmt) -> &'static str {
    loc(&f.locale).group
}
pub fn dec_sep(f: &Fmt) -> &'static str {
    loc(&f.locale).dec
}

/// `|v|` with its separators, `int_digits` integer digits at least (leading zeros).
pub fn format_abs(f: &Fmt, v: f64, decimals: usize, int_digits: usize) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = s
        .split_once('.')
        .map_or((s.as_str(), None), |(a, b)| (a, Some(b)));
    let int = format!("{int:0>int_digits$}");
    let n = int.len();
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if group_before(f, n, i) {
            out.push_str(group_sep(f));
        }
        out.push(c);
    }
    if let Some(fr) = frac {
        out.push_str(dec_sep(f));
        out.push_str(fr);
    }
    out
}

/// The sign shown before `v` written with `decimals`.
pub fn sign(f: &Fmt, v: f64, decimals: usize) -> &'static str {
    let tiny = 0.5 * 10f64.powi(-(decimals as i32));
    if v < 0.0 && v.abs() >= tiny {
        "-"
    } else if f.plus && v >= tiny {
        "+"
    } else {
        ""
    }
}

/// `v` written as set (same as `format` in counter.tsx).
pub fn format(f: &Fmt, v: f64) -> String {
    let d = f.decimals as usize;
    if f.compact {
        let l = loc(&f.locale);
        if let Some((unit, suffix)) = l.compact.iter().find(|(u, _)| v.abs() >= *u * 0.9995) {
            let digits = d.max(1);
            let x = v.abs() / unit;
            let mut s = format!("{x:.digits$}");
            if s.contains('.') {
                s = s.trim_end_matches('0').trim_end_matches('.').to_string();
            }
            let s = s.replace('.', l.dec);
            let sg = if v < 0.0 {
                "-"
            } else if f.plus && v > 0.0 {
                "+"
            } else {
                ""
            };
            return format!("{sg}{s}{suffix}");
        }
    }
    format!("{}{}", sign(f, v, d), format_abs(f, v, d, 1))
}

/// Integer digits of `|v|` written with `decimals`.
pub fn int_digits(v: f64, decimals: usize) -> usize {
    let s = format!("{:.*}", decimals, v.abs());
    s.split('.').next().map_or(1, str::len)
}

/// Parts of a counter layer.
pub struct Parts<'a> {
    pub c: &'a Config,
    pub from: &'a str,
    pub to: &'a str,
    /// Intro, seconds.
    pub duration: f64,
    pub fmt: Fmt,
    pub prefix: &'a str,
    pub suffix: &'a str,
}

pub fn parts(layer: &Layer) -> Option<Parts<'_>> {
    let LayerKind::Counter {
        x,
        from,
        to,
        duration,
        decimals,
        prefix,
        suffix,
        grouping,
        ..
    } = &layer.kind
    else {
        return None;
    };
    Some(Parts {
        c: x,
        from,
        to,
        duration: *duration,
        fmt: Fmt {
            decimals: *decimals,
            grouping: *grouping,
            locale: x.locale.clone(),
            plus: x.plus,
            compact: x.compact,
        },
        prefix,
        suffix,
    })
}

/// Within the limits (and whether it was capped).
pub fn cap(c: &Config, v: f64) -> (f64, bool) {
    let mut x = v;
    if let Some(m) = c.max {
        x = x.min(m);
    }
    if let Some(m) = c.min {
        x = x.max(m);
    }
    (x, x != v)
}

fn round_to(v: f64, d: u8) -> f64 {
    let k = 10f64.powi(d as i32);
    (v * k).round() / k
}

/// The source's raw value (fields filled in) and its number, capped.
pub fn source(p: &Parts, data: &Data) -> (String, Option<f64>) {
    let raw = interpolate_text(p.to, data);
    let v = parse_num(&raw).map(|v| round_to(cap(p.c, v).0, p.fmt.decimals));
    (raw, v)
}

/// Is the operator's value the one shown (the source has not changed since)?
pub fn manual(st: &State, raw: &str) -> bool {
    st.value.is_some() && (st.frozen || st.src_seen.as_deref() == Some(raw))
}

/// The value displayed now and its last transition (for the trend colour and
/// the odometer / flip). `mem`: the renderer's memory of the source (None:
/// no memory, e.g. an operator action).
pub fn display(
    p: &Parts,
    st: &State,
    data: &Data,
    now_ms: i64,
    age_secs: f64,
    mem: Option<&mut Mem>,
) -> (f64, Option<Anim>) {
    let (raw, src) = source(p, data);
    let from = parse_num(&interpolate_text(p.from, data)).unwrap_or(0.0);
    let take = now_ms - (age_secs * 1000.0) as i64;
    let intro_target = src.or(st.value).unwrap_or(from);
    let intro = Anim {
        from,
        to: intro_target,
        t0: take,
        dur: p.duration.max(0.0) * 1000.0,
    };
    let curve = p.c.easing;
    let at = |a: &Anim| a.value(now_ms, if a.t0 == take { Curve::Out } else { curve });
    if manual(st, &raw) {
        let v = st.value.unwrap_or(0.0);
        return match st.anim {
            Some(a) => (at(&a), Some(a)),
            None => (v, None),
        };
    }
    // The source's value (non-numeric: the last valid one is kept).
    let Some(mem) = mem else {
        let latest = match st.anim {
            Some(a) if a.t0 > take => a,
            _ => intro,
        };
        let a = Anim {
            to: src.unwrap_or(latest.to),
            ..latest
        };
        return (at(&a), Some(a));
    };
    if age_secs + 1e-6 < mem.age || (now_ms - mem.seen).abs() > 1500 {
        *mem = Mem::default(); // taken again, or not drawn for a while: no transition from old values
    }
    mem.age = age_secs;
    mem.seen = now_ms;
    let target = src.or(mem.target).unwrap_or(intro_target);
    // Latest transition so far: the intro, the operator's, the source's.
    let mut latest = intro;
    for a in [st.anim, mem.anim].into_iter().flatten() {
        if a.t0 >= latest.t0 {
            latest = a;
        }
    }
    match mem.target {
        Some(t) if (t - target).abs() > 1e-12 => {
            let shown = at(&Anim { to: t, ..latest });
            let a = Anim {
                from: shown,
                to: target,
                t0: now_ms,
                dur: if p.c.transition == Transition::None {
                    0.0
                } else {
                    p.c.trans_ms
                },
            };
            mem.anim = Some(a);
            latest = a;
        }
        _ => {}
    }
    mem.target = Some(target);
    let a = Anim {
        to: target,
        ..latest
    };
    (at(&a), Some(a))
}

/// The colour of the number (`base`: its `fill`), as `#hex`.
pub fn color_for(c: &Config, v: f64, anim: Option<&Anim>, now_ms: i64) -> Option<String> {
    match c.color_mode {
        ColorMode::Fixed => None,
        ColorMode::Thresholds => {
            let mut t: Vec<(f64, &str)> = c
                .thresholds
                .iter()
                .filter_map(|t| parse_num(&t.from).map(|n| (n, t.color.as_str())))
                .collect();
            t.sort_by(|a, b| a.0.total_cmp(&b.0));
            t.iter()
                .rfind(|(n, _)| v >= *n)
                .map(|(_, col)| col.to_string())
        }
        ColorMode::Trend => {
            let a = anim?;
            let fresh =
                c.trend_hold <= 0.0 || ((now_ms - a.t0) as f64) < c.trend_hold * 1000.0 + a.dur;
            if !fresh || (a.to - a.from).abs() < 1e-12 {
                return None;
            }
            Some(if a.to > a.from {
                c.up_color.clone()
            } else {
                c.down_color.clone()
            })
        }
    }
}

/// Applies an operator action. Ops: `inc`, `dec` (± step), `set`
/// (`{value}`: number or text), `reset` (back to the source), `freeze`
/// (`{on}` optional: toggles).
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let p = parts(layer).ok_or("not a counter")?;
    let (raw, src) = source(&p, ctx.data);
    let (shown, _) = display(&p, st, ctx.data, ctx.now_ms, ctx.age_secs, None);
    let target = if manual(st, &raw) {
        st.value.unwrap_or(shown)
    } else {
        src.unwrap_or(shown)
    };
    let dur = if p.c.transition == Transition::None {
        0.0
    } else {
        p.c.trans_ms
    };
    let go = |st: &mut State, v: f64| {
        let (c, capped) = cap(p.c, v);
        let c = round_to(c, p.fmt.decimals);
        if capped {
            st.alert = Some(Alert {
                level: "error".into(),
                text: format!(
                    "{} is out of range ({} – {}): capped at {}.",
                    format(&p.fmt, v),
                    p.c.min.map_or("−∞".into(), |m| format(&p.fmt, m)),
                    p.c.max.map_or("∞".into(), |m| format(&p.fmt, m)),
                    format(&p.fmt, c)
                ),
                at: ctx.now_ms,
            });
        }
        st.anim = Some(Anim {
            from: shown,
            to: c,
            t0: ctx.now_ms,
            dur,
        });
        st.value = Some(c);
        st.src_seen = Some(raw.clone());
    };
    match op {
        "inc" | "dec" => {
            // `by`: an amount of its own (the extra steps), else `times` the step.
            let by = args
                .get("by")
                .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(parse_num)));
            let k = args.get("times").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let d = by.unwrap_or(p.c.step * k) * if op == "inc" { 1.0 } else { -1.0 };
            go(st, target + d);
        }
        "set" => {
            let v = match args.get("value") {
                Some(serde_json::Value::Number(n)) => n.as_f64(),
                Some(serde_json::Value::String(s)) => parse_num(s),
                _ => None,
            };
            match v {
                Some(v) => go(st, v),
                None => {
                    let s = match args.get("value") {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(v) => v.to_string(),
                        None => String::new(),
                    };
                    st.alert = Some(Alert {
                        level: "warn".into(),
                        text: format!(
                            "Value “{s}”: not a number, ignored — {} kept.",
                            format(&p.fmt, target)
                        ),
                        at: ctx.now_ms,
                    });
                }
            }
        }
        "reset" => {
            let to = src
                .unwrap_or_else(|| parse_num(&interpolate_text(p.from, ctx.data)).unwrap_or(0.0));
            *st = State {
                anim: Some(Anim {
                    from: shown,
                    to,
                    t0: ctx.now_ms,
                    dur,
                }),
                ..Default::default()
            };
        }
        "freeze" => {
            let on = args
                .get("on")
                .and_then(|v| v.as_bool())
                .unwrap_or(!st.frozen);
            if on {
                st.frozen = true;
                st.value = Some(target);
                st.anim = Some(Anim {
                    from: shown,
                    to: target,
                    t0: ctx.now_ms,
                    dur,
                });
            } else if st.frozen {
                st.frozen = false;
                // Back to the source (its last value), from the value shown.
                if let Some(s) = src {
                    st.value = None;
                    st.src_seen = None;
                    st.anim = Some(Anim {
                        from: shown,
                        to: s,
                        t0: ctx.now_ms,
                        dur,
                    });
                } else {
                    st.src_seen = Some(raw.clone());
                }
            }
        }
        _ => return Err(format!("unknown action `{op}`")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn layer(extra: serde_json::Value) -> Layer {
        let mut v = json!({"id":"a","name":"C","type":"counter","from":"0","to":"1000","duration":2.0,"props":{}});
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        serde_json::from_value(v).unwrap()
    }

    fn fmt(locale: &str) -> Fmt {
        Fmt {
            decimals: 1,
            grouping: true,
            locale: locale.into(),
            plus: false,
            compact: false,
        }
    }

    #[test]
    fn formats() {
        assert_eq!(
            format(&fmt("fr-FR"), 1234567.8),
            "1\u{202f}234\u{202f}567,8"
        );
        assert_eq!(format(&fmt("en-US"), -1234567.8), "-1,234,567.8");
        assert_eq!(format(&fmt("de-CH"), 1234567.8), "1’234’567.8");
        assert_eq!(format(&fmt("en-IN"), 1234567.8), "12,34,567.8");
        assert_eq!(format(&fmt("es-ES"), 1234.5), "1234,5");
        let mut f = fmt("fr-FR");
        f.compact = true;
        assert_eq!(format(&f, 1_234_567.0), "1,2\u{a0}M");
        f.plus = true;
        f.compact = false;
        assert_eq!(format(&f, 3.0), "+3,0");
        assert_eq!(format(&f, 0.0), "0,0");
        // As older counters: the same as widgets::format_number.
        let old = Fmt {
            decimals: 0,
            ..fmt("fr-FR")
        };
        assert_eq!(
            format(&old, 12345.0),
            crate::widgets::format_number(12345.0, 0, true)
        );
    }

    #[test]
    fn parses() {
        assert_eq!(parse_num("1 234,5"), Some(1234.5));
        assert_eq!(parse_num("1,234.5"), Some(1234.5));
        assert_eq!(parse_num("1’234"), Some(1234.0));
        assert_eq!(parse_num("N/A"), None);
        assert_eq!(parse_num("12 3a4"), None);
        assert_eq!(parse_num(""), None);
        assert_eq!(parse_num("-3"), Some(-3.0));
    }

    #[test]
    fn old_counter_counts_up_as_before() {
        let l = layer(json!({}));
        let p = parts(&l).unwrap();
        assert_eq!(*p.c, Config::default());
        let data = Data::new();
        let st = State::default();
        let mut mem = Mem::default();
        let (v, _) = display(&p, &st, &data, 10_000, 1.0, Some(&mut mem));
        let old = 1000.0 * (1.0 - 0.5f64.powi(3));
        assert!((v - old).abs() < 1e-6);
        let (v, _) = display(&p, &st, &data, 12_000, 3.0, Some(&mut mem));
        assert_eq!(v, 1000.0);
    }

    #[test]
    fn actions_cap_and_restart_from_shown() {
        let l =
            layer(json!({"min":0.0,"max":1500.0,"step":100.0,"transMs":1000.0,"easing":"linear"}));
        let data = Data::new();
        let ctx = |now: i64| ActCtx {
            now_ms: now,
            age_secs: 10.0,
            data: &data,
            fps: 50.0,
        };
        let mut st = State::default();
        act(&l, &mut st, "inc", &json!({}), &ctx(0)).unwrap();
        assert_eq!(st.value, Some(1100.0));
        // Halfway, a new value: from the value shown.
        act(&l, &mut st, "set", &json!({"value":"9000"}), &ctx(500)).unwrap();
        let a = st.anim.unwrap();
        assert!((a.from - 1050.0).abs() < 1e-9);
        assert_eq!(a.to, 1500.0);
        assert_eq!(st.alert.as_ref().unwrap().level, "error");
        // Not a number: ignored.
        act(&l, &mut st, "set", &json!({"value":"N/A"}), &ctx(600)).unwrap();
        assert_eq!(st.value, Some(1500.0));
        assert_eq!(st.alert.as_ref().unwrap().level, "warn");
        act(&l, &mut st, "freeze", &json!({}), &ctx(700)).unwrap();
        assert!(st.frozen);
        act(&l, &mut st, "reset", &json!({}), &ctx(800)).unwrap();
        assert_eq!(st.value, None);
        assert!(act(&l, &mut st, "x", &json!({}), &ctx(0)).is_err());
    }

    #[test]
    fn extra_steps_add_their_own_amount() {
        let l = layer(json!({"to":"10","transition":"none","extraSteps":"2, 3; x 0"}));
        let LayerKind::Counter { x: c, .. } = &l.kind else {
            panic!()
        };
        assert_eq!(extra_steps(c), vec![2.0, 3.0]);
        let data = Data::new();
        let ctx = ActCtx {
            now_ms: 0,
            age_secs: 10.0,
            data: &data,
            fps: 50.0,
        };
        let mut st = State::default();
        act(&l, &mut st, "inc", &json!({"by": 3}), &ctx).unwrap();
        assert_eq!(st.value, Some(13.0));
        act(&l, &mut st, "dec", &json!({"by": "2"}), &ctx).unwrap();
        assert_eq!(st.value, Some(11.0));
    }

    #[test]
    fn source_change_rolls_from_shown_and_bad_value_is_ignored() {
        let l = layer(json!({"to":"{{v}}","transMs":1000.0,"easing":"linear"}));
        let p = parts(&l).unwrap();
        let st = State::default();
        let mut mem = Mem::default();
        let mut data = Data::new();
        data.insert("v".into(), "100".into());
        assert_eq!(
            display(&p, &st, &data, 10_000, 10.0, Some(&mut mem)).0,
            100.0
        );
        data.insert("v".into(), "200".into());
        assert_eq!(
            display(&p, &st, &data, 11_000, 11.0, Some(&mut mem)).0,
            100.0
        );
        assert_eq!(
            display(&p, &st, &data, 11_500, 11.5, Some(&mut mem)).0,
            150.0
        );
        data.insert("v".into(), "N/A".into());
        assert_eq!(
            display(&p, &st, &data, 13_000, 13.0, Some(&mut mem)).0,
            200.0
        );
    }
}

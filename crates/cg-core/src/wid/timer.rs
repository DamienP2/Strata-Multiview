//! Clock layers — settings, operator state and actions of two widgets:
//! the Clock (« Horloge »: time of day and date, `timerMode` clock / date)
//! and the Time (« Temps »: stopwatch and countdown, `timerMode` stopwatch /
//! countdown). Both are `clock` layers with the same settings: the mode says
//! which widget a layer is (`web/src/widgets/clock.tsx`, `time.tsx`).
//! Drawn by `cg-render/src/wid/timer.rs` and `web/src/widgets/timer.tsx` (keep in sync;
//! the formatting is mirrored by `web/src/widgets/timer-fmt.ts`).
//!
//! Every value comes from timestamps (start + accumulated pauses +
//! adjustments) and the time of the frame being drawn: nothing is counted
//! per frame, so nothing drifts.
//!
//! Older clock layers (`mode` time / countdown / elapsed / timer, legacy
//! format tokens) have no `timerMode`: they are mapped to the new modes and
//! keep following their time on air (so the old start / pause controls of
//! the graphic still drive them).

use serde::{Deserialize, Serialize};

#[path = "timer_fmt.rs"]
pub mod fmt;

use self::fmt::{
    apply_case, comp_at, dur_segs, parse_dur, parse_hms, parse_target, plain_dur, seg_text,
    target_ms, wall_segs, CaseMode, DigitOpts, Rounding, Seg, SegKind, WallOpts, ZoneSel,
};
use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{ClockMode, Layer, LayerKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimerMode {
    #[default]
    Clock,
    Date,
    Stopwatch,
    Countdown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Zeros {
    #[default]
    Pad,
    Trim,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FineUnit {
    #[default]
    S,
    F,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateCase {
    #[default]
    None,
    First,
    Words,
    Upper,
    Lower,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LimitMode {
    #[default]
    Stop,
    Continue,
    Restart,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SwRound {
    #[default]
    Trunc,
    Near,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CdTarget {
    #[default]
    Duration,
    Time,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AtZero {
    #[default]
    Freeze,
    Overrun,
    Restart,
    /// The widget disappears.
    Out,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartMode {
    #[default]
    Manual,
    /// At the In (time on air).
    Auto,
    /// At a time of day (server zone).
    At,
    /// Older layers: « External » behaved as Manual (and is read as it).
    Ext,
}

/// Prefix / suffix style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Affix {
    /// % of the font size.
    pub size: f32,
    /// "" = the text colour.
    pub color: String,
    pub weight: u16,
    pub upper: bool,
}

impl Default for Affix {
    fn default() -> Self {
        Self {
            size: 30.0,
            color: "#a594ff".into(),
            weight: 800,
            upper: true,
        }
    }
}

/// The background plate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Plate {
    pub on: bool,
    pub color: String,
    pub radius: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub accent: bool,
    pub accent_color: String,
    pub shadow: bool,
}

impl Default for Plate {
    fn default() -> Self {
        Self {
            on: false,
            color: "#140c2e".into(),
            radius: 22.0,
            pad_x: 46.0,
            pad_y: 14.0,
            accent: true,
            accent_color: "#ff3b5c".into(),
            shadow: true,
        }
    }
}

/// Digits, or a face with hands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Face {
    #[default]
    Digital,
    Analog,
}

/// The marks around an analog face.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ticks {
    /// 12 marks, longer at 3, 6, 9 and 12.
    Hours,
    /// 60 marks, the hours longer.
    #[default]
    Minutes,
    None,
}

/// The look of an analog face. Colours: "" = none (face, rim) or the text
/// colour (hands).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Analog {
    pub seconds: bool,
    /// The second hand sweeps (else it jumps each second).
    pub smooth: bool,
    pub ticks: Ticks,
    /// 1 to 12 around the face.
    pub numbers: bool,
    pub face: String,
    pub rim: String,
    pub hands: String,
    pub accent: String,
}

impl Default for Analog {
    fn default() -> Self {
        Self {
            seconds: true,
            smooth: false,
            ticks: Ticks::Minutes,
            numbers: false,
            face: "#0f1222e6".into(),
            rim: String::new(),
            hands: String::new(),
            accent: "#ff3b5c".into(),
        }
    }
}

/// "From X seconds" (stopwatch) / "under X seconds" (countdown): this style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Step {
    pub on: bool,
    /// Seconds.
    #[serde(deserialize_with = "loose_num")]
    pub at: f64,
    pub color: String,
    pub bg: String,
    #[serde(deserialize_with = "loose_bool")]
    pub blink: bool,
}

/// A number, or a text holding one (NaN when not).
fn loose_num<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        serde_json::Value::String(s) => s.trim().replace(',', ".").parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    })
}

/// true / false, or a text ("true", "yes", "1"; "" = false) from a list editor.
fn loose_bool<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Bool(b) => b,
        serde_json::Value::String(s) => matches!(s.trim(), "true" | "yes" | "1" | "on"),
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        _ => false,
    })
}

impl Default for Step {
    fn default() -> Self {
        Self {
            on: true,
            at: 10.0,
            color: "#ff3b5c".into(),
            bg: String::new(),
            blink: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct City {
    pub label: String,
    /// IANA zone, "" = server zone.
    pub zone: String,
}

/// Settings of the widget, flattened into the layer's JSON. The legacy
/// fields of the layer stay: `format`, `timezone` (clock / date zone,
/// "" = server), `mode` (older layers), `target` (older countdowns). Text
/// colour = `fill`, size = `fontSize`, font = the layer's font style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// None = an older layer: mapped from `mode`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer_mode: Option<TimerMode>,
    /// Digits or an analog face (its look in `analog`).
    pub face: Face,
    pub analog: Analog,
    pub tabular: bool,
    pub zeros: Zeros,
    pub hours_auto: bool,
    pub sep: String,
    pub sep_blink: bool,
    pub prefix: String,
    pub suffix: String,
    pub affix: Affix,
    /// Size on the widest value of the format.
    pub reserve: bool,
    /// Dashed outline of the reserved width (Designer only).
    pub guide: bool,
    /// Shrink to fit the box width.
    pub fit: bool,
    pub plate: Plate,
    pub steps: Vec<Step>,
    /// Time runs off air.
    pub off_air: bool,
    // clock / date
    /// Manual UTC offset (hours) instead of `timezone`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tz_offset: Option<f64>,
    pub h12: bool,
    pub am: String,
    pub pm: String,
    /// Time source: only "system" (the server clock) exists.
    pub source: String,
    /// A countdown at zero fires this trigger (pauses waiting for it play
    /// on, in every graphic on air); "" = none.
    pub zero_trigger: String,
    pub fine: f64,
    pub fine_unit: FineUnit,
    pub cities: bool,
    pub city_list: Vec<City>,
    /// Language of the day and month names ("fr-FR", "de"…; "" = the
    /// project's). Older layers without it keep English.
    pub lang: String,
    pub case: DateCase,
    // stopwatch
    /// Start value, ms.
    pub sw_start: i64,
    pub sw_limit_on: bool,
    pub sw_limit: i64,
    pub sw_limit_mode: LimitMode,
    pub sw_round: SwRound,
    /// Seconds a split stays frozen.
    pub split_hold: f64,
    // countdown
    pub cd_target: CdTarget,
    /// ms.
    pub cd_duration: i64,
    /// "YYYY-MM-DD" ("" = today), "HH:mm:ss", zone ("" = server).
    pub t_date: String,
    pub t_time: String,
    pub t_zone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_up: Option<bool>,
    pub at_zero: AtZero,
    pub over_color: String,
    pub over_bg: String,
    pub banner: bool,
    pub banner_text: String,
    pub banner_color: String,
    // start
    pub start_mode: StartMode,
    pub start_at: String,
    /// Seconds the Operator's − / + squares add (1, 10 or 60).
    pub adj_step: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            timer_mode: None,
            face: Face::Digital,
            analog: Analog::default(),
            // Digits that do not move: fixed-width figures, widest value reserved.
            tabular: true,
            zeros: Zeros::Pad,
            hours_auto: false,
            sep: ":".into(),
            sep_blink: false,
            prefix: String::new(),
            suffix: String::new(),
            affix: Affix::default(),
            reserve: true,
            guide: true,
            fit: false,
            plate: Plate::default(),
            steps: Vec::new(),
            off_air: true,
            tz_offset: None,
            h12: false,
            am: "AM".into(),
            pm: "PM".into(),
            source: "system".into(),
            zero_trigger: String::new(),
            fine: 0.0,
            fine_unit: FineUnit::S,
            cities: false,
            city_list: Vec::new(),
            lang: "en-GB".into(),
            case: DateCase::None,
            sw_start: 0,
            sw_limit_on: false,
            sw_limit: 5_400_000,
            sw_limit_mode: LimitMode::Stop,
            sw_round: SwRound::Trunc,
            split_hold: 5.0,
            cd_target: CdTarget::Duration,
            cd_duration: 300_000,
            t_date: String::new(),
            t_time: "20:00:00".into(),
            t_zone: String::new(),
            round_up: None,
            at_zero: AtZero::Freeze,
            over_color: "#ff3b5c".into(),
            over_bg: String::new(),
            banner: false,
            banner_text: "C'est parti !".into(),
            banner_color: "#ff3b5c".into(),
            start_mode: StartMode::Manual,
            start_at: "20:00:00".into(),
            adj_step: 10.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Split {
    pub at: i64,
    pub v: i64,
}

/// What the operator changed: timestamps only (Unix ms).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// The operator took control (else the start mode decides).
    pub ctl: bool,
    pub started_at: Option<i64>,
    pub paused_at: Option<i64>,
    pub paused_total: i64,
    /// Adjustments (± seconds, set value), ms.
    pub adj: i64,
    pub split: Option<Split>,
    /// New target set by the operator (countdown to a time).
    pub target_ms: Option<i64>,
    /// The zero its trigger was fired for (each zero fires once).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fired: Option<i64>,
}

// ------------------------------------------------------------ resolved --

/// The settings that apply, older layers mapped.
#[derive(Debug, Clone)]
pub struct Eff<'a> {
    pub c: &'a Config,
    pub mode: TimerMode,
    pub legacy: bool,
    pub manual_start: bool,
    pub format: String,
    pub zone: ZoneSel,
    pub start: StartMode,
    pub round_up: bool,
    pub at_zero: AtZero,
    /// Countdown to a time (else a duration).
    pub cd_time: bool,
    pub duration: i64,
    /// Target string ("[date ]time"), its zone.
    pub target: String,
    pub target_zone: ZoneSel,
    /// Language of the day and month names (a key of [`fmt::NAME_LANGS`]).
    pub lang: &'static str,
}

/// The timer settings of a clock layer (None for another layer), the names
/// in French when the layer follows the project's language.
pub fn eff<'a>(layer: &'a Layer, data: &Data) -> Option<Eff<'a>> {
    eff_in(layer, data, "")
}

/// The timer settings of a clock layer in a project of this language.
pub fn eff_in<'a>(layer: &'a Layer, data: &Data, project_lang: &str) -> Option<Eff<'a>> {
    let LayerKind::Clock {
        mode,
        format,
        timezone,
        target,
        x: c,
        ..
    } = &layer.kind
    else {
        return None;
    };
    let zone = match c.tz_offset {
        Some(h) => ZoneSel::Offset(h),
        None => ZoneSel::named(timezone),
    };
    let fmt = if format.trim().is_empty() {
        "HH:mm:ss".to_string()
    } else {
        format.clone()
    };
    let lang = fmt::name_lang(&c.lang, project_lang);
    Some(match c.timer_mode {
        Some(m) => Eff {
            c,
            mode: m,
            legacy: false,
            manual_start: layer.manual_start,
            format: fmt,
            zone,
            // The layer's manual start (Continue / trigger) carries the wait: its clock
            // starts the timer. Older layers still waiting for Play keep « manual »;
            // « External » behaved as it.
            start: match c.start_mode {
                StartMode::At => StartMode::At,
                _ if layer.manual_start => StartMode::Auto,
                StartMode::Ext => StartMode::Manual,
                m => m,
            },
            round_up: c.round_up.unwrap_or(true),
            at_zero: c.at_zero,
            cd_time: c.cd_target == CdTarget::Time,
            duration: c.cd_duration.max(0),
            target: if c.t_date.trim().is_empty() {
                c.t_time.trim().to_string()
            } else {
                format!("{} {}", c.t_date.trim(), c.t_time.trim())
            },
            target_zone: ZoneSel::named(&c.t_zone),
            lang,
        },
        None => {
            let tgt = interpolate_text(target, data);
            let (m, cd_time) = match mode {
                ClockMode::Time => (TimerMode::Clock, false),
                ClockMode::Elapsed => (TimerMode::Stopwatch, false),
                ClockMode::Timer => (TimerMode::Countdown, false),
                ClockMode::Countdown => (TimerMode::Countdown, true),
            };
            Eff {
                c,
                mode: m,
                legacy: true,
                manual_start: layer.manual_start,
                format: fmt,
                zone: zone.clone(),
                start: StartMode::Auto,
                round_up: c.round_up.unwrap_or(cd_time),
                at_zero: AtZero::Freeze,
                cd_time,
                duration: crate::widgets::parse_duration(&tgt)
                    .map(|s| (s * 1000.0).round() as i64)
                    .unwrap_or(0)
                    .max(0),
                target: tgt.trim().to_string(),
                target_zone: zone,
                lang,
            }
        }
    })
}

impl Eff<'_> {
    pub fn timed(&self) -> bool {
        matches!(self.mode, TimerMode::Stopwatch | TimerMode::Countdown)
    }
    pub fn is_target(&self) -> bool {
        self.mode == TimerMode::Countdown && self.cd_time
    }
    fn digits(&self) -> DigitOpts {
        DigitOpts {
            trim: self.c.zeros == Zeros::Trim,
            hours_auto: self.c.hours_auto,
        }
    }
}

/// The running state at a time: the state, or what the start mode implies
/// while the operator has not acted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Run {
    pub started_at: Option<i64>,
    pub paused_at: Option<i64>,
    pub paused_total: i64,
    pub adj: i64,
    pub split: Option<Split>,
    pub target_ms: Option<i64>,
}

/// Start time of the "at a time" mode, today in the server zone.
pub fn start_at_ms(e: &Eff, now: i64, default_tz: &str) -> Option<i64> {
    let (h, m, s) = parse_hms(&e.c.start_at)?;
    let tz = ZoneSel::Machine.tz(default_tz);
    let c = comp_at(now, &tz);
    fmt::zoned_ms(c.y, c.mo, c.d, h, m, s, &tz)
}

/// `age_ms`: time on air of the layer.
pub fn run(e: &Eff, st: &State, now: i64, age_ms: i64, default_tz: &str) -> Run {
    if st.ctl {
        return Run {
            started_at: st.started_at,
            paused_at: st.paused_at,
            paused_total: st.paused_total,
            adj: st.adj,
            split: st.split,
            target_ms: st.target_ms,
        };
    }
    let into_air = now - age_ms.max(0);
    let started_at = if e.legacy {
        // Older layers follow their time on air (0 until a manual start).
        Some(into_air)
    } else {
        match e.start {
            StartMode::Auto => Some(into_air),
            StartMode::At => start_at_ms(e, now, default_tz)
                .filter(|&ts| now >= ts && now - ts < 6 * 3_600_000)
                .map(|ts| if e.c.off_air { ts } else { ts.max(into_air) }),
            StartMode::Manual | StartMode::Ext => None,
        }
    };
    Run {
        started_at,
        ..Default::default()
    }
}

impl Run {
    pub fn started(&self, e: &Eff) -> bool {
        e.is_target() || self.started_at.is_some()
    }
    pub fn elapsed(&self, now: i64) -> i64 {
        match self.started_at {
            Some(s) => self.paused_at.unwrap_or(now) - s - self.paused_total,
            None => 0,
        }
    }
}

/// The instant a countdown to a time aims at.
pub fn target_of(e: &Eff, r: &Run, now: i64, default_tz: &str) -> Option<i64> {
    if let Some(t) = r.target_ms {
        return Some(t);
    }
    let tz = e.target_zone.tz(default_tz);
    if e.legacy {
        // Older countdowns: "HH:mm[:ss]" today or "yyyy-MM-ddTHH:mm:ss".
        let t = e.target.replace('T', " ");
        let t = t.split('.').next().unwrap_or("");
        return parse_target(t).ok().and_then(|p| target_ms(&p, &tz, now));
    }
    parse_target(&e.target)
        .ok()
        .and_then(|p| target_ms(&p, &tz, now))
}

/// The value before the limit / zero behaviour, ms (None: no valid target).
pub fn raw_value(e: &Eff, r: &Run, now: i64, default_tz: &str) -> Option<i64> {
    Some(match e.mode {
        TimerMode::Stopwatch => e.c.sw_start + r.elapsed(now) + r.adj,
        TimerMode::Countdown if e.cd_time => {
            target_of(e, r, now, default_tz)? - (r.paused_at.unwrap_or(now) - r.paused_total)
                + r.adj
        }
        TimerMode::Countdown => e.duration - r.elapsed(now) + r.adj,
        _ => 0,
    })
}

/// What a timed value shows.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Value {
    pub v: i64,
    /// Countdown past zero (overrun).
    pub over: bool,
    /// Countdown at / past zero.
    pub zero: bool,
    /// Stopwatch at its limit (stop).
    pub limit: bool,
    /// Instant the countdown reached zero.
    pub zero_at: Option<i64>,
}

pub fn value(e: &Eff, r: &Run, now: i64, default_tz: &str) -> Option<Value> {
    let mut v = raw_value(e, r, now, default_tz)?;
    let c = e.c;
    if e.mode == TimerMode::Stopwatch {
        let mut out = Value::default();
        if c.sw_limit_on && c.sw_limit > c.sw_start && v >= c.sw_limit {
            match c.sw_limit_mode {
                LimitMode::Stop => {
                    v = c.sw_limit;
                    out.limit = true;
                }
                LimitMode::Restart => v = c.sw_start + (v - c.sw_start) % (c.sw_limit - c.sw_start),
                LimitMode::Continue => {}
            }
        }
        out.v = v.max(0);
        return Some(out);
    }
    if v > 0 {
        return Some(Value {
            v,
            ..Default::default()
        });
    }
    let zero_at = r.paused_at.unwrap_or(now) + v;
    let base = Value {
        v: 0,
        zero: true,
        zero_at: Some(zero_at),
        ..Default::default()
    };
    match e.at_zero {
        AtZero::Overrun => Some(Value {
            v,
            over: v < 0,
            ..base
        }),
        AtZero::Restart if !e.cd_time => {
            let p = e.duration + r.adj;
            if p > 0 {
                let m = v.rem_euclid(p);
                Some(Value {
                    v: if m == 0 { p } else { m },
                    zero: false,
                    ..base
                })
            } else {
                Some(base)
            }
        }
        _ => Some(base),
    }
}

// ------------------------------------------------------------- display --

/// What the widget shows at an instant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Disp {
    pub segs: Vec<Seg>,
    /// Text colour / background of a style step or the overrun (None = normal).
    pub color: Option<String>,
    pub bg: Option<String>,
    pub blink: bool,
    /// A split is frozen on screen.
    pub split: bool,
    /// Nothing is drawn (countdown "out" at zero).
    pub hidden: bool,
    /// Timed value (ms) when there is one.
    pub v: Option<i64>,
    pub over: bool,
    pub zero_at: Option<i64>,
}

/// Inputs of a display besides the settings.
#[derive(Debug, Clone, Copy)]
pub struct At<'a> {
    pub now: i64,
    pub age_ms: i64,
    pub fps: f64,
    pub default_tz: &'a str,
}

fn wall_opts<'a>(e: &'a Eff, fps: f64) -> WallOpts<'a> {
    WallOpts {
        lang: e.lang,
        h12: e.c.h12,
        am: &e.c.am,
        pm: &e.c.pm,
        trim: e.c.zeros == Zeros::Trim,
        fps,
    }
}

fn case_of(c: DateCase) -> CaseMode {
    match c {
        DateCase::None => CaseMode::None,
        DateCase::First => CaseMode::First,
        DateCase::Words => CaseMode::Words,
        DateCase::Upper => CaseMode::Upper,
        DateCase::Lower => CaseMode::Lower,
    }
}

/// The clock time after the fine offset.
pub fn clock_ms(e: &Eff, now: i64, fps: f64) -> i64 {
    let fine = match e.c.fine_unit {
        FineUnit::F => e.c.fine * 1000.0 / fps.max(1.0),
        FineUnit::S => e.c.fine * 1000.0,
    };
    now + fine.round() as i64
}

/// Segments of a wall clock at `ms`, in `zone`.
pub fn wall(e: &Eff, ms: i64, zone: &ZoneSel, fmt_s: &str, fps: f64, default_tz: &str) -> Vec<Seg> {
    let c = comp_at(ms, &zone.tz(default_tz));
    let segs = wall_segs(&c, fmt_s, &wall_opts(e, fps));
    if e.mode == TimerMode::Date {
        apply_case(segs, case_of(e.c.case))
    } else {
        segs
    }
}

fn step_for<'a>(e: &Eff<'a>, v: i64) -> Option<&'a Step> {
    let mut best: Option<&Step> = None;
    for s in e.c.steps.iter().filter(|s| s.on && s.at.is_finite()) {
        let at = (s.at * 1000.0) as i64;
        let ok = if e.mode == TimerMode::Countdown {
            v <= at && best.is_none_or(|b| s.at < b.at)
        } else {
            v >= at && best.is_none_or(|b| s.at > b.at)
        };
        if ok {
            best = Some(s);
        }
    }
    best
}

fn rounding(e: &Eff, over: bool) -> Rounding {
    if e.mode == TimerMode::Countdown {
        if over || !e.round_up {
            Rounding::Trunc
        } else {
            Rounding::Up
        }
    } else if e.c.sw_round == SwRound::Near {
        Rounding::Near
    } else {
        Rounding::Trunc
    }
}

/// Segments of a duration with this widget's format and digits.
pub fn dur(e: &Eff, ms: i64, fps: f64, round: Rounding) -> Vec<Seg> {
    dur_segs(ms as f64, &e.format, fps, round, e.digits())
}

/// What is shown at `at.now` (`zone`: a city clock's zone instead of the widget's).
pub fn display(e: &Eff, st: &State, at: At, zone: Option<&ZoneSel>) -> Disp {
    let mut out = Disp::default();
    if !e.timed() {
        let ms = clock_ms(e, at.now, at.fps);
        out.segs = wall(
            e,
            ms,
            zone.unwrap_or(&e.zone),
            &e.format,
            at.fps,
            at.default_tz,
        );
        return out;
    }
    let r = run(e, st, at.now, at.age_ms, at.default_tz);
    let Some(val) = value(e, &r, at.now, at.default_tz) else {
        out.segs = vec![
            Seg::lit("--"),
            Seg {
                k: SegKind::Sep,
                t: ":".into(),
            },
            Seg::lit("--"),
        ];
        return out;
    };
    let mut v = val.v;
    if e.mode == TimerMode::Stopwatch {
        if let Some(sp) = r.split {
            if at.now >= sp.at && ((at.now - sp.at) as f64) < e.c.split_hold * 1000.0 {
                v = sp.v;
                out.split = true;
            }
        }
    }
    out.v = Some(v);
    out.over = val.over;
    out.zero_at = val.zero_at;
    if e.mode == TimerMode::Countdown && e.at_zero == AtZero::Out && val.zero {
        out.hidden = true;
    }
    out.segs = dur(e, v, at.fps, rounding(e, val.over));
    if val.over {
        out.segs.insert(0, Seg::lit("+"));
        out.color = Some(e.c.over_color.clone()).filter(|s| !s.is_empty());
        out.bg = Some(e.c.over_bg.clone()).filter(|s| !s.is_empty());
        return out;
    }
    if let Some(s) = step_for(e, v) {
        out.color = Some(s.color.clone()).filter(|s| !s.is_empty());
        out.bg = Some(s.bg.clone()).filter(|s| !s.is_empty());
        out.blink = s.blink;
    }
    out
}

/// Angles of the hour, minute and second hands, degrees from 12 o'clock
/// (None: nothing to show). A clock reads its zone's time of day; a
/// stopwatch or countdown shows its value as hours, minutes, seconds.
pub fn hands(e: &Eff, st: &State, at: At, zone: Option<&ZoneSel>) -> Option<[f64; 3]> {
    let secs = if e.timed() {
        let d = display(e, st, at, zone);
        if d.hidden {
            return None;
        }
        d.v?.unsigned_abs() as f64 / 1000.0
    } else {
        let ms = clock_ms(e, at.now, at.fps);
        let c = comp_at(ms, &zone.unwrap_or(&e.zone).tz(at.default_tz));
        (c.h * 3600 + c.mi * 60 + c.s) as f64 + c.ms as f64 / 1000.0
    };
    let s = if e.c.analog.smooth {
        secs
    } else {
        secs.floor()
    };
    Some([
        (s / 120.0).rem_euclid(360.0),
        (s / 10.0).rem_euclid(360.0),
        (s * 6.0).rem_euclid(360.0),
    ])
}

/// The widest strings the format can produce (digits as 8): the box is
/// sized on them so 9:59 → 10:00 does not move anything.
pub fn widest(e: &Eff, d: &Disp, sep: &str, fps: f64) -> Vec<String> {
    let dig = |s: String| -> String {
        s.chars()
            .map(|c| if c.is_ascii_digit() { '8' } else { c })
            .collect()
    };
    let mut out = vec![dig(seg_text(&d.segs, sep))];
    if !e.timed() {
        for i in 0..12u32 {
            let c = fmt::Comp {
                y: 2026,
                mo: i + 1,
                d: 28,
                h: if i % 2 == 1 { 20 } else { 8 },
                mi: 28,
                s: 28,
                ms: 888,
                wd: i % 7,
            };
            let segs = wall_segs(&c, &e.format, &wall_opts(e, fps));
            let segs = if e.mode == TimerMode::Date {
                apply_case(segs, case_of(e.c.case))
            } else {
                segs
            };
            out.push(dig(seg_text(&segs, sep)));
        }
        return out;
    }
    let has_h = fmt::tokenize(&e.format)
        .iter()
        .any(|t| matches!(t, fmt::Tok::T("HH" | "H" | "hh" | "h")));
    let max_v = if e.mode == TimerMode::Stopwatch {
        if e.c.sw_limit_on {
            e.c.sw_limit.max(e.c.sw_start)
        } else {
            e.c.sw_start
                .max(if has_h { 36_000_000 - 1 } else { 3_600_000 - 1 })
        }
    } else if e.cd_time {
        d.v.unwrap_or(0).max(0)
    } else {
        e.duration.max(0)
    };
    let m = dig(seg_text(&dur(e, max_v, fps, Rounding::Trunc), sep));
    if e.mode == TimerMode::Countdown && e.at_zero == AtZero::Overrun {
        out.push(format!("+{m}"));
    }
    out.push(m);
    out
}

/// Start → zero banner: (scale, opacity) while it shows.
pub fn banner(e: &Eff, d: &Disp, now: i64) -> Option<(f32, f32)> {
    if e.mode != TimerMode::Countdown || !e.c.banner || e.c.banner_text.trim().is_empty() {
        return None;
    }
    let dt = now - d.zero_at?;
    if !(0..=4500).contains(&dt) {
        return None;
    }
    let k = (dt as f32 / 350.0).clamp(0.0, 1.0);
    let k = 1.0 - (1.0 - k).powi(3);
    let k = if dt > 4000 {
        k * (1.0 - (dt - 4000) as f32 / 500.0)
    } else {
        k
    };
    Some((0.6 + 0.4 * k, k))
}

/// The prefix and suffix with data.
pub fn affixes(c: &Config, data: &Data) -> (String, String) {
    let f = |s: &str| {
        let t = interpolate_text(s, data);
        if c.affix.upper {
            t.to_uppercase()
        } else {
            t
        }
    };
    (f(&c.prefix), f(&c.suffix))
}

// ------------------------------------------------------------- actions --

fn now_run(e: &Eff, st: &State, ctx: &ActCtx) -> State {
    let r = run(e, st, ctx.now_ms, (ctx.age_secs * 1000.0) as i64, "");
    let mut s = State {
        ctl: true,
        started_at: r.started_at,
        paused_at: r.paused_at,
        paused_total: r.paused_total,
        adj: r.adj,
        split: r.split,
        target_ms: r.target_ms,
        fired: st.fired,
    };
    let now = ctx.now_ms;
    // A stop at the limit / a frozen zero becomes a real pause, so the next
    // action starts from what is on screen.
    let rr = run(e, &s, now, 0, "");
    if s.paused_at.is_none() && rr.started(e) {
        if let Some(raw) = raw_value(e, &rr, now, "") {
            let c = e.c;
            if e.mode == TimerMode::Stopwatch
                && c.sw_limit_on
                && c.sw_limit_mode == LimitMode::Stop
                && c.sw_limit > c.sw_start
                && raw >= c.sw_limit
            {
                s.paused_at = Some(now - (raw - c.sw_limit));
            }
            if e.mode == TimerMode::Countdown
                && !e.cd_time
                && matches!(e.at_zero, AtZero::Freeze | AtZero::Out)
                && raw <= 0
            {
                s.paused_at = Some(now + raw);
            }
        }
    }
    s
}

/// The trigger a countdown fires now: its name and the zero it is for,
/// when it is at zero and has not fired for that zero yet.
pub fn due_trigger(
    layer: &Layer,
    st: &State,
    data: &Data,
    now: i64,
    age_ms: i64,
    default_tz: &str,
) -> Option<(String, i64)> {
    let e = eff(layer, data)?;
    let name = e.c.zero_trigger.trim();
    if name.is_empty() || e.mode != TimerMode::Countdown {
        return None;
    }
    let r = run(&e, st, now, age_ms, default_tz);
    if !r.started(&e) {
        return None;
    }
    let v = value(&e, &r, now, default_tz)?;
    // Without its instant, the start stands for this run (never fires again each frame).
    let at = v.zero_at.or(r.started_at).unwrap_or(0);
    (v.zero && st.fired != Some(at)).then(|| (name.to_string(), at))
}

/// What a timer shows now, once started: its time (ms), whether it counts
/// down and whether it is at zero.
pub fn reading(
    layer: &Layer,
    st: &State,
    data: &Data,
    now: i64,
    age_ms: i64,
    default_tz: &str,
) -> Option<(i64, bool, bool)> {
    let e = eff(layer, data)?;
    let r = run(&e, st, now, age_ms, default_tz);
    if !r.started(&e) {
        return None;
    }
    let v = value(&e, &r, now, default_tz)?;
    Some((v.v, e.mode == TimerMode::Countdown, v.zero))
}

fn resume(s: &mut State, now: i64) {
    if let Some(p) = s.paused_at.take() {
        s.paused_total += now - p;
    }
}

fn num_arg(args: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    if let Some(n) = args.as_f64() {
        return Some(n);
    }
    keys.iter().find_map(|k| {
        let v = args.get(*k)?;
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
    })
}

/// Actions: `start`, `pause`, `resume`, `toggle`, `reset`, `restart`,
/// `set {value: "05:00" | "20:00:00" | "2026-10-18 20:00"}`,
/// `adjust {seconds: ±n}` (or `{ms}`), `split`.
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let e = eff(layer, ctx.data).ok_or("not a timer")?;
    if !e.timed() {
        return Err(format!(
            "`{op}`: a {} has no controls",
            if e.mode == TimerMode::Date {
                "date"
            } else {
                "clock"
            }
        ));
    }
    let now = ctx.now_ms;
    let tgt = e.is_target();
    let mut s = now_run(&e, st, ctx);
    let raw_now = |s: &State| raw_value(&e, &run(&e, s, now, 0, ""), now, "");
    match op {
        "start" => {
            if s.paused_at.is_some() {
                resume(&mut s, now);
            } else if s.started_at.is_none() && !tgt {
                s.started_at = Some(now);
                s.paused_total = 0;
            }
        }
        // A countdown to a time keeps its top: no pause (it would move the end).
        "pause" | "toggle" if tgt && s.paused_at.is_none() => {
            return Err("A countdown to a time cannot pause: its end stays at the time set".into());
        }
        "pause" => {
            if run(&e, &s, now, 0, "").started(&e) && s.paused_at.is_none() {
                s.paused_at = Some(now);
            }
        }
        "resume" => resume(&mut s, now),
        "toggle" => {
            let running = run(&e, &s, now, 0, "").started(&e) && s.paused_at.is_none();
            return act(
                layer,
                st,
                if running { "pause" } else { "start" },
                args,
                ctx,
            );
        }
        "reset" => {
            s = State {
                ctl: true,
                ..Default::default()
            }
        }
        "restart" => {
            s = State {
                ctl: true,
                started_at: (!tgt).then_some(now),
                ..Default::default()
            }
        }
        "adjust" => {
            let delta = num_arg(args, &["seconds", "s"])
                .map(|x| x * 1000.0)
                .or_else(|| num_arg(args, &["ms"]))
                .filter(|x| x.is_finite())
                .ok_or("adjust needs {\"seconds\": n}")?
                .round() as i64;
            let v = raw_now(&s).ok_or("The target time is invalid")?;
            let delta = if v + delta < 0 { -v } else { delta };
            s.adj += delta;
            if e.mode == TimerMode::Countdown
                && v + delta > 0
                && !tgt
                && s.paused_at.is_some()
                && v <= 0
            {
                resume(&mut s, now); // frozen at zero: runs again
            }
            if e.mode == TimerMode::Stopwatch
                && e.c.sw_limit_on
                && e.c.sw_limit_mode == LimitMode::Stop
                && v >= e.c.sw_limit
                && v + delta < e.c.sw_limit
                && s.started_at.is_some()
            {
                resume(&mut s, now);
            }
        }
        "split" => {
            if e.mode != TimerMode::Stopwatch {
                return Err("split: only a stopwatch has splits".into());
            }
            let r = run(&e, &s, now, 0, "");
            if r.started(&e) {
                let v = value(&e, &r, now, "").map(|v| v.v).unwrap_or(0);
                s.split = Some(Split { at: now, v });
            }
        }
        "set" => {
            let input = args
                .get("value")
                .or_else(|| args.get("target"))
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .or_else(|| args.as_str().map(String::from))
                .unwrap_or_default();
            if tgt {
                let p = parse_target(&input)?;
                let tz = e.target_zone.tz("");
                let t = target_ms(&p, &tz, now).ok_or("This time does not exist in the zone")?;
                s.target_ms = Some(t);
                s.adj = 0;
                s.paused_total = 0;
                if s.paused_at.is_some() {
                    s.paused_at = Some(now);
                }
            } else {
                let ms = parse_dur(&input)?;
                if e.mode == TimerMode::Stopwatch && e.c.sw_limit_on && ms > e.c.sw_limit {
                    return Err(format!(
                        "{} is above the limit {}",
                        plain_dur(ms),
                        plain_dur(e.c.sw_limit)
                    ));
                }
                let v = raw_now(&s).unwrap_or(0);
                s.adj += ms - v;
                if e.mode == TimerMode::Countdown && ms > 0 && s.paused_at.is_some() && v <= 0 {
                    // Was frozen at zero: stays paused at the new value.
                    s.paused_at = Some(now);
                }
            }
        }
        _ => return Err(format!("unknown action `{op}`")),
    }
    *st = s;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn layer(json: &str) -> Layer {
        serde_json::from_str(json).unwrap()
    }
    fn data() -> Data {
        BTreeMap::new()
    }
    fn ctx(now: i64, d: &Data) -> ActCtx<'_> {
        ActCtx {
            now_ms: now,
            age_secs: 0.0,
            data: d,
            fps: 50.0,
        }
    }
    fn text(l: &Layer, st: &State, now: i64, age_ms: i64) -> String {
        let d = data();
        let e = eff(l, &d).unwrap();
        let at = At {
            now,
            age_ms,
            fps: 25.0,
            default_tz: "UTC",
        };
        seg_text(&display(&e, st, at, None).segs, &e.c.sep)
    }
    const T0: i64 = 1_792_000_000_000;

    #[test]
    fn old_layers_load_and_map() {
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"timer","format":"mm:ss","target":"5:00","props":{}}"#,
        );
        // Legacy timer: counts down from the take, truncated like before.
        assert_eq!(text(&l, &State::default(), T0, 0), "05:00");
        assert_eq!(text(&l, &State::default(), T0, 1500), "04:58");
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"time","format":"HH:mm:ss","timezone":"UTC","props":{}}"#,
        );
        let t = text(&l, &State::default(), T0, 0);
        assert_eq!(t.len(), 8);
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"time","format":"dd/MM/yyyy h:mm A","timezone":"UTC","props":{}}"#,
        );
        assert!(text(&l, &State::default(), 0, 0).starts_with("01/01/1970 12:00 AM"));
    }

    #[test]
    fn analog_hands_follow_the_time() {
        let d = data();
        let at = |now| At {
            now,
            age_ms: 0,
            fps: 25.0,
            default_tz: "UTC",
        };
        // 1970-01-01 03:15:30 UTC.
        let now = (3 * 3600 + 15 * 60 + 30) * 1000;
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","timerMode":"clock","face":"analog","timezone":"UTC","props":{}}"#,
        );
        let e = eff(&l, &d).unwrap();
        let [h, m, s] = hands(&e, &State::default(), at(now), None).unwrap();
        assert!((h - (3.0 * 30.0 + 15.5 * 0.5)).abs() < 1e-9);
        assert!((m - 15.5 * 6.0).abs() < 1e-9);
        assert_eq!(s, 180.0);
        // A countdown shows its remaining time: 5 minutes.
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","timerMode":"countdown","face":"analog","cdDuration":300000,"props":{}}"#,
        );
        let e = eff(&l, &d).unwrap();
        let [_, m, s] = hands(&e, &State::default(), at(T0), None).unwrap();
        assert_eq!((m, s), (30.0, 0.0));
    }

    fn sw() -> Layer {
        layer(
            r#"{"id":"a","name":"Chrono","type":"clock","mode":"elapsed","format":"mm:ss","timerMode":"stopwatch","props":{}}"#,
        )
    }

    #[test]
    fn stopwatch_actions_are_timestamps() {
        let l = sw();
        let d = data();
        let mut st = State::default();
        assert_eq!(text(&l, &st, T0, 0), "00:00");
        act(&l, &mut st, "start", &serde_json::Value::Null, &ctx(T0, &d)).unwrap();
        assert_eq!(text(&l, &st, T0 + 3 * 3_600_000, 0), "180:00");
        act(
            &l,
            &mut st,
            "pause",
            &serde_json::Value::Null,
            &ctx(T0 + 10_000, &d),
        )
        .unwrap();
        assert_eq!(text(&l, &st, T0 + 60_000, 0), "00:10");
        act(
            &l,
            &mut st,
            "resume",
            &serde_json::Value::Null,
            &ctx(T0 + 60_000, &d),
        )
        .unwrap();
        assert_eq!(text(&l, &st, T0 + 65_000, 0), "00:15");
        act(
            &l,
            &mut st,
            "adjust",
            &serde_json::json!({"seconds": -60}),
            &ctx(T0 + 65_000, &d),
        )
        .unwrap();
        assert_eq!(text(&l, &st, T0 + 65_000, 0), "00:00");
        act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": "45:00"}),
            &ctx(T0 + 65_000, &d),
        )
        .unwrap();
        assert_eq!(text(&l, &st, T0 + 66_000, 0), "45:01");
        let before = st.clone();
        assert!(act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": "-1:7x"}),
            &ctx(T0, &d)
        )
        .is_err());
        assert!(act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": "1:75"}),
            &ctx(T0, &d)
        )
        .is_err());
        assert!(act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": ""}),
            &ctx(T0, &d)
        )
        .is_err());
        assert_eq!(st, before);
        act(
            &l,
            &mut st,
            "split",
            &serde_json::Value::Null,
            &ctx(T0 + 66_000, &d),
        )
        .unwrap();
        assert_eq!(text(&l, &st, T0 + 69_000, 0), "45:01");
        assert_eq!(text(&l, &st, T0 + 72_000, 0), "45:07");
    }

    #[test]
    fn a_layer_starting_on_continue_follows_its_clock() {
        // The layer's start (Continue / trigger) wins over the older « manual » wait: the
        // timer follows the layer's clock, 0 until the server starts it.
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"timer","format":"mm:ss","timerMode":"countdown","cdDuration":60000,"startMode":"manual","manualStart":true,"props":{}}"#,
        );
        let d = data();
        let e = eff(&l, &d).unwrap();
        assert_eq!(e.start, StartMode::Auto);
        assert!(e.manual_start);
        assert_eq!(e.c.adj_step, 10.0);
        assert_eq!(text(&l, &State::default(), T0, 0), "01:00");
        assert_eq!(text(&l, &State::default(), T0, 12_000), "00:48");
        // Without the layer's start, « manual » still waits for Play.
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"timer","format":"mm:ss","timerMode":"countdown","cdDuration":60000,"startMode":"manual","adjStep":60,"props":{}}"#,
        );
        let e = eff(&l, &d).unwrap();
        assert_eq!(e.start, StartMode::Manual);
        assert_eq!(e.c.adj_step, 60.0);
        assert_eq!(text(&l, &State::default(), T0, 12_000), "01:00");
        // A fixed time of day stays.
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"timer","format":"mm:ss","timerMode":"countdown","startMode":"at","manualStart":true,"props":{}}"#,
        );
        assert_eq!(eff(&l, &d).unwrap().start, StartMode::At);
    }

    #[test]
    fn hundredths_at_25fps_and_frames() {
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"elapsed","format":"mm:ss.SS","timerMode":"stopwatch","startMode":"auto","props":{}}"#,
        );
        assert_eq!(text(&l, &State::default(), T0, 12_340), "00:12.34");
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"elapsed","format":"mm:ss:FF","timerMode":"stopwatch","startMode":"auto","props":{}}"#,
        );
        assert_eq!(text(&l, &State::default(), T0, 1_960), "00:01:24");
    }

    #[test]
    fn a_countdown_at_zero_fires_its_trigger_once() {
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"timer","format":"mm:ss","timerMode":"countdown","cdDuration":5000,"startMode":"auto","zeroTrigger":"next","props":{}}"#,
        );
        let d = data();
        let mut st = State::default();
        assert!(due_trigger(&l, &st, &d, T0, 1000, "").is_none(), "not yet");
        let (name, at) = due_trigger(&l, &st, &d, T0, 6000, "").expect("at zero");
        assert_eq!(name, "next");
        st.fired = Some(at);
        assert!(
            due_trigger(&l, &st, &d, T0 + 1000, 7000, "").is_none(),
            "once"
        );
        // No trigger set: nothing.
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"timer","format":"mm:ss","timerMode":"countdown","cdDuration":5000,"startMode":"auto","props":{}}"#,
        );
        assert!(due_trigger(&l, &State::default(), &d, T0, 6000, "").is_none());
    }

    #[test]
    fn countdown_round_up_zero_and_overrun() {
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"timer","format":"mm:ss","timerMode":"countdown","cdDuration":300000,"startMode":"auto","atZero":"overrun","props":{}}"#,
        );
        let s = State::default();
        assert_eq!(text(&l, &s, T0, 0), "05:00");
        assert_eq!(text(&l, &s, T0, 100), "05:00");
        assert_eq!(text(&l, &s, T0, 1000), "04:59");
        assert_eq!(text(&l, &s, T0, 300_000), "00:00");
        assert_eq!(text(&l, &s, T0, 312_000), "+00:12");
        // Freeze: stays at zero; adjust brings it back.
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"timer","format":"mm:ss","timerMode":"countdown","cdDuration":5000,"props":{}}"#,
        );
        let d = data();
        let mut st = State::default();
        act(&l, &mut st, "start", &serde_json::Value::Null, &ctx(T0, &d)).unwrap();
        assert_eq!(text(&l, &st, T0 + 9000, 0), "00:00");
        act(
            &l,
            &mut st,
            "adjust",
            &serde_json::json!({"seconds": 10}),
            &ctx(T0 + 9000, &d),
        )
        .unwrap();
        assert_eq!(text(&l, &st, T0 + 9000, 0), "00:10");
        assert_eq!(text(&l, &st, T0 + 10_000, 0), "00:09");
    }

    #[test]
    fn hours_only_when_needed_and_days() {
        let o = DigitOpts {
            trim: false,
            hours_auto: true,
        };
        assert_eq!(
            seg_text(
                &dur_segs(65_000.0, "HH:mm:ss", 25.0, Rounding::Trunc, o),
                ":"
            ),
            "01:05"
        );
        assert_eq!(
            seg_text(
                &dur_segs(3_605_000.0, "HH:mm:ss", 25.0, Rounding::Trunc, o),
                ":"
            ),
            "01:00:05"
        );
        let t = DigitOpts {
            trim: true,
            hours_auto: true,
        };
        assert_eq!(
            seg_text(
                &dur_segs(65_000.0, "HH:mm:ss", 25.0, Rounding::Trunc, t),
                ":"
            ),
            "1:05"
        );
        assert_eq!(
            seg_text(
                &dur_segs(90_061_000.0, "DD[d] HH:mm:ss", 25.0, Rounding::Trunc, o),
                ":"
            ),
            "01d 01:01:01"
        );
        assert_eq!(
            seg_text(
                &dur_segs(3.0 * 86_400_000.0, "[J-]D", 25.0, Rounding::Up, o),
                ":"
            ),
            "J-3"
        );
    }

    #[test]
    fn a_countdown_to_a_time_keeps_its_top_and_external_is_manual() {
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"countdown","format":"HH:mm:ss","timerMode":"countdown","cdTarget":"time","tDate":"2026-10-25","tTime":"20:00:00","tZone":"UTC","props":{}}"#,
        );
        let d = data();
        let mut st = State::default();
        // No pause: the end stays at 20:00 (± seconds still move it on purpose).
        assert!(act(&l, &mut st, "pause", &serde_json::Value::Null, &ctx(T0, &d)).is_err());
        assert!(act(
            &l,
            &mut st,
            "toggle",
            &serde_json::Value::Null,
            &ctx(T0, &d)
        )
        .is_err());
        assert_eq!(st, State::default());
        act(
            &l,
            &mut st,
            "adjust",
            &serde_json::json!({"seconds": 10}),
            &ctx(T0, &d),
        )
        .unwrap();
        assert_eq!(st.adj, 10_000);
        // « External » (older layers) waits like Manual.
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"elapsed","format":"mm:ss","timerMode":"stopwatch","startMode":"ext","props":{}}"#,
        );
        let e = eff(&l, &d).unwrap();
        assert_eq!(e.start, StartMode::Manual);
        // Digits that do not move, by default.
        assert!(e.c.tabular && e.c.reserve);
    }

    #[test]
    fn target_is_absolute_across_dst() {
        // 25 Oct 2026 02:59:50 CEST (00:59:50 UTC), target 03:30 Paris: the
        // clocks go back at 03:00, so 1:30:10 remain, not 0:30:10.
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"countdown","format":"HH:mm:ss","timerMode":"countdown","cdTarget":"time","tDate":"2026-10-25","tTime":"03:30:00","tZone":"Europe/Paris","atZero":"overrun","props":{}}"#,
        );
        let now = jiff::Timestamp::from_second(1_792_889_990).unwrap(); // 2026-10-25T00:59:50Z
        assert_eq!(now.to_string(), "2026-10-25T00:59:50Z");
        assert_eq!(
            text(&l, &State::default(), now.as_millisecond(), 0),
            "01:30:10"
        );
        // Already passed: overrun at once.
        assert_eq!(
            text(
                &l,
                &State::default(),
                now.as_millisecond() + 5_410_000 + 300_000,
                0
            ),
            "+00:05:00"
        );
        let d = data();
        let mut st = State::default();
        assert!(act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": ""}),
            &ctx(0, &d)
        )
        .is_err());
        assert!(act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": "25:00"}),
            &ctx(0, &d)
        )
        .is_err());
        act(
            &l,
            &mut st,
            "set",
            &serde_json::json!({"value": "2026-10-25 04:00"}),
            &ctx(now.as_millisecond(), &d),
        )
        .unwrap();
        assert_eq!(text(&l, &st, now.as_millisecond(), 0), "02:00:10");
    }

    #[test]
    fn date_names_and_case() {
        let l = layer(
            r#"{"id":"a","name":"D","type":"clock","mode":"time","format":"dddd D MMMM YYYY","timerMode":"date","lang":"fr-FR","case":"first","timezone":"UTC","props":{}}"#,
        );
        // 2026-10-18 is a Sunday.
        let ms = jiff::Timestamp::from_second(1_792_324_800)
            .unwrap()
            .as_millisecond();
        assert_eq!(
            text(&l, &State::default(), ms, 0),
            "Dimanche 18 octobre 2026"
        );
        let l = layer(
            r#"{"id":"a","name":"D","type":"clock","mode":"time","format":"ddd D MMM","timerMode":"date","lang":"de","case":"upper","timezone":"UTC","props":{}}"#,
        );
        assert_eq!(text(&l, &State::default(), ms, 0), "SO 18 OKT");
    }

    #[test]
    fn names_follow_the_layer_then_the_project_then_french() {
        // 2026-10-18 is a Sunday.
        let ms = jiff::Timestamp::from_second(1_792_324_800)
            .unwrap()
            .as_millisecond();
        let d = data();
        let at = At {
            now: ms,
            age_ms: 0,
            fps: 25.0,
            default_tz: "UTC",
        };
        let shown = |lang: &str, project: &str| {
            let l = layer(&format!(
                r#"{{"id":"a","name":"D","type":"clock","format":"dddd D MMMM","timerMode":"date","lang":"{lang}","timezone":"UTC","props":{{}}}}"#
            ));
            let e = eff_in(&l, &d, project).unwrap();
            seg_text(&display(&e, &State::default(), at, None).segs, ":")
        };
        assert_eq!(shown("", "de"), "Sonntag 18 Oktober");
        assert_eq!(shown("", "en"), "Sunday 18 October");
        assert_eq!(shown("", ""), "dimanche 18 octobre");
        assert_eq!(shown("", "sv"), "dimanche 18 octobre");
        assert_eq!(shown("es-ES", "de"), "domingo 18 octubre");
        assert_eq!(shown("it", "fr"), "domenica 18 ottobre");
        assert_eq!(shown("pt-BR", "fr"), "domingo 18 outubro");
        assert_eq!(shown("nl-NL", "fr"), "zondag 18 oktober");
        // Older layers without a language keep English.
        let l = layer(
            r#"{"id":"a","name":"D","type":"clock","format":"dddd","timerMode":"date","timezone":"UTC","props":{}}"#,
        );
        let e = eff_in(&l, &d, "de").unwrap();
        assert_eq!(
            seg_text(&display(&e, &State::default(), at, None).segs, ":"),
            "Sunday"
        );
    }

    #[test]
    fn stopwatch_limit_stops_and_widest() {
        let l = layer(
            r#"{"id":"a","name":"C","type":"clock","mode":"elapsed","format":"mm:ss","timerMode":"stopwatch","startMode":"auto","swLimitOn":true,"swLimit":10000,"props":{}}"#,
        );
        assert_eq!(text(&l, &State::default(), T0, 25_000), "00:10");
        let d = data();
        let e = eff(&l, &d).unwrap();
        let disp = display(
            &e,
            &State::default(),
            At {
                now: T0,
                age_ms: 0,
                fps: 25.0,
                default_tz: "UTC",
            },
            None,
        );
        assert!(widest(&e, &disp, ":", 25.0).contains(&"88:88".to_string()));
    }
}

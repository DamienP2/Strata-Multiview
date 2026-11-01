//! The broadcast widgets of a multiview, besides the Source tile: audio
//! meters (up to 16 channels, an input with or without picture), loudness
//! (EBU R128), timecode, the state of the plant (PTP, NTP, faults) and the
//! alarm journal.
//!
//! Each one is laid out here as a list of rectangles and texts ([`Item`]),
//! drawn as such by both renderers (`strata-mv-render/src/wid/broadcast.rs`
//! and `web/src/widgets/broadcast.tsx`, which mirrors these functions: keep
//! them in sync).

use serde::{Deserialize, Serialize};

use crate::model::TextAlign;
use crate::signal::{
    AlarmKind, Level, Severity, SourceView, SystemView, MAX_TRUE_PEAK, SILENCE, TARGET,
};

/// A box: x, y, width, height (layer pixels).
pub type Area = [f32; 4];

/// What a broadcast widget draws.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Rect {
        a: Area,
        radius: f32,
        color: String,
    },
    /// One line, fitted into its box (smaller, then narrower if too long).
    Text {
        a: Area,
        text: String,
        size: f32,
        weight: u16,
        /// JetBrains Mono (figures that do not move) instead of Inter.
        mono: bool,
        color: String,
        align: TextAlign,
    },
    /// A line through points (closed: back to the first), stroked `width`
    /// wide, filled when `fill` is set: graticules, curves.
    Path {
        points: Vec<[f32; 2]>,
        closed: bool,
        width: f32,
        color: String,
        fill: Option<String>,
    },
}

/// A line through `points`.
pub fn path(points: Vec<[f32; 2]>, closed: bool, width: f32, color: &str) -> Item {
    Item::Path {
        points,
        closed,
        width,
        color: color.into(),
        fill: None,
    }
}

/// A circle of `r` around `c`, as a closed line of 72 points.
pub fn circle(c: [f32; 2], r: f32, width: f32, color: &str) -> Item {
    let points = (0..72)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / 72.0;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect();
    path(points, true, width, color)
}

pub fn rect(a: Area, radius: f32, color: &str) -> Item {
    Item::Rect {
        a,
        radius,
        color: color.into(),
    }
}

pub fn text(
    a: Area,
    t: impl Into<String>,
    size: f32,
    weight: u16,
    mono: bool,
    color: &str,
    align: TextAlign,
) -> Item {
    Item::Text {
        a,
        text: t.into(),
        size,
        weight,
        mono,
        color: color.into(),
        align,
    }
}

/// The colours the widgets share.
pub const GREEN: &str = "#2bd96b";
pub const YELLOW: &str = "#f2c12e";
pub const RED: &str = "#ff3b3b";
pub const AMBER: &str = "#fbbf24";
pub const TRACK: &str = "#ffffff14";
pub const DIM: &str = "#9aa3b2";
pub const WHITE: &str = "#ffffff";
/// Warning and critical backgrounds, and the text on them.
pub const WARN_BG: &str = "#fbbf24";
pub const WARN_FG: &str = "#1c1300";
pub const CRIT_BG: &str = "#dc2626";
pub const CRIT_FG: &str = "#ffffff";

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

/// A number with one decimal, a real minus sign ("−23.0").
pub fn num1(v: f32) -> String {
    let v = round1(v);
    if v < 0.0 {
        format!("\u{2212}{:.1}", -v)
    } else {
        format!("{v:.1}")
    }
}

pub(crate) fn pad_of(w: f32, h: f32) -> f32 {
    (w.min(h) * 0.04).clamp(3.0, 14.0).round()
}

// ----------------------------------------------------------- meters --

/// Audio meters of an input: one bar per channel (16 at most).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MeterConfig {
    pub input: String,
    /// Channels shown (0: all those it has, 16 at most).
    pub channels: u32,
    /// The names under the bars, comma separated ("L, R, C, LFE…"; none: the numbers).
    pub labels: String,
    /// The dB scale on the left.
    pub scale: bool,
    /// The peak of each channel above its bar.
    pub values: bool,
    /// The highest peak of the last seconds.
    pub hold: bool,
    /// The loudness under the bars.
    pub loudness: bool,
    /// dBFS: bottom of the scale, yellow from, red from.
    pub min: f32,
    pub yellow: f32,
    pub red: f32,
    pub background: String,
}

impl Default for MeterConfig {
    fn default() -> Self {
        Self {
            input: String::new(),
            channels: 0,
            labels: String::new(),
            scale: true,
            values: false,
            hold: true,
            loudness: true,
            min: -60.0,
            yellow: -18.0,
            red: -9.0,
            background: "#0b0d12d9".into(),
        }
    }
}

/// A level on the meters' scale: 0 at `min`, 1 at full scale.
fn on_scale(min: f32, db: f32) -> f32 {
    let min = min.min(-1.0);
    ((db - min) / -min).clamp(0.0, 1.0)
}

/// The colour of a level: green, then yellow, then red.
fn level_color(c: &MeterConfig, db: f32) -> &'static str {
    if db >= c.red {
        RED
    } else if db >= c.yellow {
        YELLOW
    } else {
        GREEN
    }
}

pub fn meters(c: &MeterConfig, w: f32, h: f32, v: Option<&SourceView>) -> Vec<Item> {
    let mut out = vec![rect([0.0, 0.0, w, h], pad_of(w, h) * 0.6, &c.background)];
    let levels: Vec<Level> = v.map(|v| v.levels.clone()).unwrap_or_default();
    let n = match (c.channels, levels.len()) {
        (0, 0) => 2,
        (0, k) => k.min(16),
        (k, _) => (k as usize).min(16),
    };
    let pad = pad_of(w, h);
    let fs = (h * 0.035).clamp(8.0, 18.0).round();
    let sw = if c.scale { (fs * 2.2).round() } else { 0.0 };
    let top = pad + if c.values { fs * 1.5 } else { 0.0 };
    let lufs_h = if c.loudness { fs * 1.8 } else { 0.0 };
    let bottom = h - pad - fs * 1.5 - lufs_h;
    let x0 = pad + sw;
    let span = (w - x0 - pad).max(1.0);
    let gap = (span / n as f32 * 0.18).clamp(1.0, 10.0).round();
    let bw = ((span - gap * (n - 1) as f32) / n as f32).max(1.0);
    let bh = (bottom - top).max(1.0);
    let labels: Vec<String> = c
        .labels
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if c.scale {
        for d in [0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0, -60.0] {
            if d < c.min {
                continue;
            }
            let y = top + bh * (1.0 - on_scale(c.min, d));
            out.push(text(
                [pad, y - fs * 0.6, sw - fs * 0.3, fs * 1.2],
                format!("{}", d as i32),
                fs * 0.8,
                500,
                true,
                DIM,
                TextAlign::Right,
            ));
        }
    }
    for i in 0..n {
        let x = x0 + i as f32 * (bw + gap);
        let l = levels.get(i).copied().unwrap_or_default();
        out.push(rect([x, top, bw, bh], 0.0, TRACK));
        // Lit bottom up: green, yellow, red.
        let lit = on_scale(c.min, l.peak);
        let (y1, r1) = (
            on_scale(c.min, c.yellow),
            on_scale(c.min, c.red.max(c.yellow)),
        );
        for (from, to, col) in [(0.0, y1, GREEN), (y1, r1, YELLOW), (r1, 1.0, RED)] {
            let t = lit.min(to);
            if t > from {
                out.push(rect(
                    [x, top + bh * (1.0 - t), bw, bh * (t - from)],
                    0.0,
                    col,
                ));
            }
        }
        if c.hold && l.hold > c.min {
            let k = on_scale(c.min, l.hold);
            let th = (bh * 0.008).clamp(1.0, 3.0);
            let y = (top + bh * (1.0 - k)).min(top + bh - th);
            out.push(rect([x, y, bw, th], 0.0, level_color(c, l.hold)));
        }
        if c.values {
            let s = if l.peak <= SILENCE + 1.0 || i >= levels.len() {
                "\u{2014}".to_string()
            } else {
                format!("{}", l.peak.round() as i32)
            };
            out.push(text(
                [x - gap / 2.0, pad, bw + gap, fs * 1.3],
                s,
                fs * 0.8,
                600,
                true,
                DIM,
                TextAlign::Center,
            ));
        }
        let name = labels
            .get(i)
            .cloned()
            .unwrap_or_else(|| (i + 1).to_string());
        out.push(text(
            [x - gap / 2.0, bottom + fs * 0.25, bw + gap, fs * 1.2],
            name,
            fs * 0.85,
            600,
            false,
            WHITE,
            TextAlign::Center,
        ));
    }
    if c.loudness {
        let y = h - pad - lufs_h;
        let s = match v.and_then(|v| v.loudness) {
            Some(l) => format!(
                "I {}  S {}  TP {}",
                num1(l.integrated),
                num1(l.short_term),
                num1(l.true_peak)
            ),
            None => "LUFS \u{2014}".into(),
        };
        let col = v
            .and_then(|v| v.loudness)
            .map_or(DIM, |l| loud_color(l.integrated, l.true_peak, TARGET, 1.0));
        out.push(text(
            [pad, y + fs * 0.3, w - 2.0 * pad, fs * 1.3],
            s,
            fs,
            700,
            true,
            col,
            TextAlign::Center,
        ));
    }
    out
}

/// The colour of a loudness: on target (green), off it (amber), the true
/// peak over the limit (red).
pub fn loud_color(lufs: f32, true_peak: f32, target: f32, tolerance: f32) -> &'static str {
    if true_peak > MAX_TRUE_PEAK {
        RED
    } else if (lufs - target).abs() <= tolerance {
        GREEN
    } else {
        AMBER
    }
}

// --------------------------------------------------------- loudness --

/// The EBU R128 loudness of an input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LoudnessConfig {
    pub input: String,
    /// LUFS, and the tolerance either side, LU.
    pub target: f32,
    pub tolerance: f32,
    /// The short-term loudness of the last two minutes.
    pub history: bool,
    pub background: String,
}

impl Default for LoudnessConfig {
    fn default() -> Self {
        Self {
            input: String::new(),
            target: TARGET,
            tolerance: 1.0,
            history: true,
            background: "#0b0d12d9".into(),
        }
    }
}

pub fn loudness(
    c: &LoudnessConfig,
    w: f32,
    h: f32,
    v: Option<&SourceView>,
    lang: &str,
) -> Vec<Item> {
    let pad = pad_of(w, h);
    let mut out = vec![rect([0.0, 0.0, w, h], pad * 0.6, &c.background)];
    let l = v.and_then(|v| v.loudness);
    let fr = lang.to_lowercase().starts_with("fr");
    let inner = w - 2.0 * pad;
    // The integrated loudness, big, and the target.
    let big = (h * 0.16).clamp(12.0, 96.0).round();
    let col = l.map_or(DIM, |l| {
        loud_color(l.integrated, l.true_peak, c.target, c.tolerance)
    });
    out.push(text(
        [pad, pad, inner * 0.62, big * 1.15],
        l.map_or("\u{2014}".to_string(), |l| num1(l.integrated)),
        big,
        700,
        true,
        col,
        TextAlign::Left,
    ));
    let small = (big * 0.3).max(7.0).round();
    out.push(text(
        [
            pad + inner * 0.62,
            pad + big * 0.1,
            inner * 0.38,
            small * 1.3,
        ],
        if fr {
            "LUFS INTÉGRÉ"
        } else {
            "LUFS INTEGRATED"
        },
        small,
        700,
        false,
        DIM,
        TextAlign::Right,
    ));
    out.push(text(
        [
            pad + inner * 0.62,
            pad + big * 0.1 + small * 1.4,
            inner * 0.38,
            small * 1.3,
        ],
        format!(
            "{} {} ±{}",
            if fr { "CIBLE" } else { "TARGET" },
            num1(c.target),
            c.tolerance
        ),
        small,
        600,
        false,
        DIM,
        TextAlign::Right,
    ));
    // The other values: momentary, short-term, range, true peak.
    let y = pad + big * 1.3;
    let cells: [(&str, String, &str); 4] = [
        (
            "M",
            l.map_or("\u{2014}".into(), |l| num1(l.momentary)),
            WHITE,
        ),
        (
            "S",
            l.map_or("\u{2014}".into(), |l| num1(l.short_term)),
            WHITE,
        ),
        (
            "LRA",
            l.map_or("\u{2014}".into(), |l| format!("{} LU", num1(l.range))),
            WHITE,
        ),
        (
            "TP",
            l.map_or("\u{2014}".into(), |l| format!("{} dBTP", num1(l.true_peak))),
            l.map_or(WHITE, |l| {
                if l.true_peak > MAX_TRUE_PEAK {
                    RED
                } else {
                    WHITE
                }
            }),
        ),
    ];
    let cw = (inner - pad * 3.0) / 4.0;
    let ch = (small * 3.6).round();
    for (i, (k, val, colr)) in cells.iter().enumerate() {
        let x = pad + i as f32 * (cw + pad);
        out.push(rect([x, y, cw, ch], pad * 0.4, "#ffffff0d"));
        out.push(text(
            [x + pad * 0.6, y + small * 0.3, cw - pad * 1.2, small * 1.2],
            *k,
            small,
            700,
            false,
            DIM,
            TextAlign::Left,
        ));
        out.push(text(
            [x + pad * 0.6, y + small * 1.6, cw - pad * 1.2, small * 1.7],
            val.clone(),
            small * 1.35,
            700,
            true,
            colr,
            TextAlign::Left,
        ));
    }
    // The short-term loudness of the last two minutes, and the target.
    let hy = y + ch + pad;
    let hh = h - hy - pad;
    if c.history && hh > 12.0 {
        out.push(rect([pad, hy, inner, hh], pad * 0.4, "#ffffff0d"));
        let (lo, hi) = (c.target - 13.0, c.target + 11.0);
        let k = |v: f32| ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
        let hist: &[f32] = v.map_or(&[], |v| &v.history);
        let slots = 120usize;
        let sw = inner / slots as f32;
        let first = slots.saturating_sub(hist.len());
        for (i, s) in hist.iter().rev().take(slots).rev().enumerate() {
            let t = k(*s);
            if t <= 0.0 {
                continue;
            }
            let x = pad + (first + i) as f32 * sw;
            let colr = if (*s - c.target) > c.tolerance {
                AMBER
            } else {
                GREEN
            };
            out.push(rect(
                [x, hy + hh * (1.0 - t), (sw - 1.0).max(1.0), hh * t],
                0.0,
                colr,
            ));
        }
        let ty = hy + hh * (1.0 - k(c.target));
        out.push(rect([pad, ty, inner, 1.0], 0.0, "#93c5fd"));
    }
    out
}

// --------------------------------------------------------- timecode --

/// A timecode: the station's (the server's time of day) or an input's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimecodeConfig {
    /// "" = the station's.
    pub input: String,
    /// Above the figures ("" = none).
    pub label: String,
    pub color: String,
    pub background: String,
}

impl Default for TimecodeConfig {
    fn default() -> Self {
        Self {
            input: String::new(),
            label: "TC".into(),
            color: "#fbbf24".into(),
            background: "#0b0d12d9".into(),
        }
    }
}

/// The station's timecode: the time of day in `tz` at `fps` ("HH:MM:SS:FF").
pub fn station_timecode(now_ms: i64, tz: &str, fps: f64) -> String {
    let Ok(t) = jiff::Timestamp::from_millisecond(now_ms) else {
        return String::new();
    };
    let z = t.to_zoned(crate::widgets::zone(tz, ""));
    let fps = if fps > 0.0 { fps } else { 25.0 };
    let ms = now_ms.rem_euclid(1000) as f64;
    let frame = (ms * fps / 1000.0).floor() as i64;
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        z.hour(),
        z.minute(),
        z.second(),
        frame
    )
}

/// The time of day in `tz`, "HH:MM:SS" (the journal).
pub fn clock(ms: i64, tz: &str) -> String {
    let Ok(t) = jiff::Timestamp::from_millisecond(ms) else {
        return String::new();
    };
    let z = t.to_zoned(crate::widgets::zone(tz, ""));
    format!("{:02}:{:02}:{:02}", z.hour(), z.minute(), z.second())
}

pub fn timecode(c: &TimecodeConfig, w: f32, h: f32, tc: &str, tag: &str) -> Vec<Item> {
    let pad = pad_of(w, h);
    let mut out = vec![rect([0.0, 0.0, w, h], pad * 0.6, &c.background)];
    let has_label = !c.label.trim().is_empty();
    let small = (h * 0.16).clamp(7.0, 28.0).round();
    let top = if has_label { pad + small * 1.3 } else { pad };
    if has_label {
        out.push(text(
            [pad, pad, (w - 2.0 * pad) * 0.6, small * 1.2],
            c.label.trim(),
            small,
            700,
            false,
            DIM,
            TextAlign::Left,
        ));
    }
    out.push(text(
        [
            pad + (w - 2.0 * pad) * 0.6,
            pad,
            (w - 2.0 * pad) * 0.4,
            small * 1.2,
        ],
        tag,
        small * 0.9,
        700,
        false,
        DIM,
        TextAlign::Right,
    ));
    let size = (h - top - pad).max(6.0) * 0.82;
    let shown = if tc.is_empty() { "--:--:--:--" } else { tc };
    out.push(text(
        [pad, top, w - 2.0 * pad, h - top - pad],
        shown,
        size,
        700,
        true,
        &c.color,
        TextAlign::Center,
    ));
    out
}

// ----------------------------------------------------------- status --

/// The state of the plant: PTP, NTP and the faults on now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StatusConfig {
    pub ptp: bool,
    pub ntp: bool,
    pub alarms: bool,
    pub background: String,
}

impl Default for StatusConfig {
    fn default() -> Self {
        Self {
            ptp: true,
            ntp: true,
            alarms: true,
            background: "#0b0d12d9".into(),
        }
    }
}

/// One line of the state: its name, what it says, its colour.
pub fn status_lines(
    c: &StatusConfig,
    s: &SystemView,
    lang: &str,
) -> Vec<(String, String, &'static str)> {
    let fr = lang.to_lowercase().starts_with("fr");
    let mut out = Vec::new();
    if c.ptp {
        let p = &s.ptp;
        let (val, col) = if p.locked {
            let off = p
                .offset_ns
                .map(|o| format!(" \u{00b1}{} ns", o.abs().round() as i64))
                .unwrap_or_default();
            (
                format!("{}{off}", if fr { "VERROUILLÉ" } else { "LOCKED" }),
                GREEN,
            )
        } else if p.running {
            (
                if p.state.is_empty() {
                    "?".to_string()
                } else {
                    p.state.clone()
                },
                AMBER,
            )
        } else {
            ((if fr { "ABSENT" } else { "NOT RUNNING" }).to_string(), DIM)
        };
        out.push((
            format!(
                "PTP · {} {}",
                if fr { "domaine" } else { "domain" },
                p.domain
            ),
            val,
            col,
        ));
    }
    if c.ntp {
        let n = &s.ntp;
        let (val, col) = match n.synced {
            Some(true) => (
                format!(
                    "{}{}",
                    if fr { "SYNCHRO" } else { "SYNCED" },
                    n.offset_ms
                        .map(|o| format!(" {:+.1} ms", o))
                        .unwrap_or_default()
                ),
                GREEN,
            ),
            Some(false) => (
                (if fr { "NON SYNCHRO" } else { "NOT SYNCED" }).to_string(),
                AMBER,
            ),
            None => ((if fr { "INCONNU" } else { "UNKNOWN" }).to_string(), DIM),
        };
        let name = if n.source.is_empty() {
            "NTP".to_string()
        } else {
            format!("NTP · {}", n.source)
        };
        out.push((name, val, col));
    }
    if c.alarms {
        let on: Vec<_> = s.journal.iter().filter(|e| e.ended.is_none()).collect();
        let crit = on
            .iter()
            .filter(|e| e.severity == Severity::Critical)
            .count();
        let (val, col) = if crit > 0 {
            (
                format!("{crit} {}", if fr { "CRITIQUE(S)" } else { "CRITICAL" }),
                RED,
            )
        } else if !on.is_empty() {
            (
                format!(
                    "{} {}",
                    on.len(),
                    if fr { "ALERTE(S)" } else { "WARNING(S)" }
                ),
                AMBER,
            )
        } else {
            ((if fr { "AUCUNE" } else { "NONE" }).to_string(), GREEN)
        };
        out.push((
            (if fr { "ALARMES" } else { "ALARMS" }).to_string(),
            val,
            col,
        ));
    }
    out
}

pub fn status(c: &StatusConfig, w: f32, h: f32, s: &SystemView, lang: &str) -> Vec<Item> {
    let pad = pad_of(w, h);
    let mut out = vec![rect([0.0, 0.0, w, h], pad * 0.6, &c.background)];
    let lines = status_lines(c, s, lang);
    if lines.is_empty() {
        return out;
    }
    let rh = (h - 2.0 * pad) / lines.len() as f32;
    // Narrow: the value under its name.
    let stack = w < rh * 5.0;
    let fs = if stack {
        (rh * 0.3).min(w * 0.06)
    } else {
        rh * 0.4
    }
    .clamp(7.0, 30.0)
    .round();
    let lh = if stack { rh * 0.5 } else { rh };
    for (i, (name, val, col)) in lines.iter().enumerate() {
        let y = pad + i as f32 * rh;
        let dot = fs * 0.7;
        out.push(rect([pad, y + (lh - dot) / 2.0, dot, dot], dot / 2.0, col));
        let x = pad + dot + fs * 0.6;
        let tw = w - x - pad;
        let (name_a, val_a) = if stack {
            ([x, y, tw, lh], [x, y + lh, tw, lh])
        } else {
            ([x, y, tw * 0.5, rh], [x + tw * 0.5, y, tw * 0.5, rh])
        };
        out.push(text(
            name_a,
            name.clone(),
            fs,
            600,
            false,
            WHITE,
            TextAlign::Left,
        ));
        out.push(text(
            val_a,
            val.clone(),
            fs,
            700,
            false,
            col,
            if stack {
                TextAlign::Left
            } else {
                TextAlign::Right
            },
        ));
    }
    out
}

// ------------------------------------------------------------ alarms --

/// The alarm journal, the newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AlarmLogConfig {
    /// Lines shown.
    pub rows: u32,
    /// The faults over too (dimmed).
    pub ended: bool,
    pub background: String,
}

impl Default for AlarmLogConfig {
    fn default() -> Self {
        Self {
            rows: 8,
            ended: true,
            background: "#0b0d12d9".into(),
        }
    }
}

pub fn alarm_log(
    c: &AlarmLogConfig,
    w: f32,
    h: f32,
    s: &SystemView,
    tz: &str,
    lang: &str,
) -> Vec<Item> {
    let pad = pad_of(w, h);
    let mut out = vec![rect([0.0, 0.0, w, h], pad * 0.6, &c.background)];
    let fr = lang.to_lowercase().starts_with("fr");
    let rows = c.rows.clamp(1, 40) as usize;
    let rh = (h - 2.0 * pad) / rows as f32;
    // Narrow: the fault under its time and input. As wide as the line
    // allows too.
    let stack = w < rh * 11.0;
    let fs = if stack {
        (rh * 0.3).min(w * 0.06)
    } else {
        (rh * 0.42).min(w * 0.026)
    }
    .clamp(7.0, 28.0)
    .round();
    let lh = if stack { rh * 0.5 } else { rh };
    let lines: Vec<_> = s
        .journal
        .iter()
        .filter(|e| c.ended || e.ended.is_none())
        .take(rows)
        .collect();
    if lines.is_empty() {
        out.push(text(
            [pad, pad, w - 2.0 * pad, rh],
            if fr { "Aucune alarme" } else { "No alarm" },
            fs,
            600,
            false,
            DIM,
            TextAlign::Left,
        ));
        return out;
    }
    let tw = fs * 4.6;
    let cw = fs * 6.4;
    for (i, e) in lines.iter().enumerate() {
        let y = pad + i as f32 * rh;
        let over = e.ended.is_some();
        let fg = if over { DIM } else { WHITE };
        out.push(text(
            [pad, y, tw, lh],
            clock(e.at, tz),
            fs,
            500,
            true,
            DIM,
            TextAlign::Left,
        ));
        let (bg, chip_fg, word) = match (over, e.severity) {
            (true, _) => ("#ffffff1a", DIM, if fr { "FINIE" } else { "OVER" }),
            (false, Severity::Critical) => {
                (CRIT_BG, CRIT_FG, if fr { "CRITIQUE" } else { "CRITICAL" })
            }
            (false, Severity::Warning) => (WARN_BG, WARN_FG, if fr { "ALERTE" } else { "WARNING" }),
        };
        let chip = [pad + tw + fs * 0.4, y + lh * 0.18, cw, lh * 0.64];
        out.push(rect(chip, fs * 0.25, bg));
        out.push(text(
            chip,
            word,
            fs * 0.72,
            800,
            false,
            chip_fg,
            TextAlign::Center,
        ));
        let x = chip[0] + cw + fs * 0.6;
        let rest = (w - x - pad).max(1.0);
        let (input_a, fault_a) = if stack {
            ([x, y, rest, lh], [pad, y + lh, w - 2.0 * pad, lh])
        } else {
            ([x, y, rest * 0.4, rh], [x + rest * 0.4, y, rest * 0.6, rh])
        };
        out.push(text(
            input_a,
            e.input.clone(),
            fs,
            700,
            false,
            fg,
            TextAlign::Left,
        ));
        out.push(text(
            fault_a,
            e.kind.text(lang),
            fs,
            600,
            false,
            fg,
            TextAlign::Left,
        ));
    }
    out
}

/// The colour of a fault: critical red, warning amber.
pub fn alarm_colors(k: AlarmKind) -> (&'static str, &'static str) {
    match k.severity() {
        Severity::Critical => (CRIT_BG, CRIT_FG),
        Severity::Warning => (WARN_BG, WARN_FG),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{JournalEntry, Loudness};

    #[test]
    fn meters_show_every_channel_and_the_loudness() {
        let v = SourceView {
            levels: vec![
                Level {
                    peak: -6.0,
                    rms: -9.0,
                    hold: -3.0
                };
                6
            ],
            loudness: Some(Loudness {
                momentary: -20.0,
                short_term: -21.0,
                integrated: -23.0,
                range: 6.0,
                true_peak: -2.0,
            }),
            ..Default::default()
        };
        let c = MeterConfig {
            labels: "L, R, C, LFE".into(),
            ..Default::default()
        };
        let items = meters(&c, 400.0, 300.0, Some(&v));
        let texts: Vec<&str> = items
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        for l in ["L", "R", "C", "LFE", "5", "6"] {
            assert!(texts.contains(&l), "{l} in {texts:?}");
        }
        assert!(texts.iter().any(|t| t.starts_with("I \u{2212}23.0")));
    }

    #[test]
    fn the_station_timecode_counts_frames() {
        let tc = station_timecode(1_700_000_000_480, "UTC", 25.0);
        assert_eq!(tc, "22:13:20:12");
        assert_eq!(clock(1_700_000_000_480, "UTC"), "22:13:20");
    }

    #[test]
    fn the_journal_shows_its_newest_lines() {
        let s = SystemView {
            journal: vec![
                JournalEntry {
                    at: 1_700_000_000_000,
                    input: "CAM 1".into(),
                    kind: AlarmKind::Freeze,
                    severity: Severity::Critical,
                    ended: None,
                    input_id: "cam1".into(),
                    acked: None,
                    by: String::new(),
                },
                JournalEntry {
                    at: 1_699_999_000_000,
                    input: "CAM 2".into(),
                    kind: AlarmKind::Silence,
                    severity: Severity::Warning,
                    ended: Some(1_699_999_100_000),
                    input_id: "cam2".into(),
                    acked: None,
                    by: String::new(),
                },
            ],
            ..Default::default()
        };
        let items = alarm_log(&AlarmLogConfig::default(), 600.0, 300.0, &s, "UTC", "fr");
        let has = |w: &str| {
            items
                .iter()
                .any(|i| matches!(i, Item::Text { text, .. } if text == w))
        };
        assert!(has("IMAGE FIGÉE") && has("CRITIQUE") && has("FINIE") && has("CAM 2"));
        let lines = status_lines(&StatusConfig::default(), &s, "fr");
        assert_eq!(lines[2].1, "1 CRITIQUE(S)");
    }
}

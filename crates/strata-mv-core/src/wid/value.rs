//! The Value widget: one live value as text — of an input (its name, UMD,
//! format, timecode, loudness, peaks, codec, colour space, HDR, levels,
//! phase, worst fault, tally) or of the plant (the time, the date, the
//! station timecode, PTP, NTP, processors, memory, faults on). Each one a
//! tool of its own, placed and styled freely. Laid out as items like the
//! other broadcast widgets, drawn by both renderers
//! (`web/src/widgets/value.tsx` mirrors this module: keep them in sync).

use serde::{Deserialize, Serialize};

use super::broadcast::{num1, pad_of, rect, text, Item, AMBER, DIM, GREEN, RED, WHITE};
use super::chart::{value_text, ChartMetric};
use super::signal_info::{audio_line, dynamic_colors, levels_text, matrix_name, primaries_name};
use crate::model::TextAlign;
use crate::signal::{Severity, SourceView, SystemView};

/// What the value is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueKind {
    // An input's.
    #[default]
    Name,
    Umd,
    Format,
    Timecode,
    Momentary,
    ShortTerm,
    Integrated,
    LoudnessRange,
    TruePeak,
    PeakLeft,
    PeakRight,
    Codec,
    Picture,
    ColorSpace,
    Dynamic,
    Levels,
    Phase,
    Audio,
    Alarm,
    Tally,
    // The plant's.
    Time,
    Date,
    StationTimecode,
    Ptp,
    PtpOffset,
    NtpOffset,
    Cpu,
    Memory,
    AlarmsOn,
}

impl ValueKind {
    /// Of the plant (no input).
    pub fn plant(self) -> bool {
        matches!(
            self,
            ValueKind::Time
                | ValueKind::Date
                | ValueKind::StationTimecode
                | ValueKind::Ptp
                | ValueKind::PtpOffset
                | ValueKind::NtpOffset
                | ValueKind::Cpu
                | ValueKind::Memory
                | ValueKind::AlarmsOn
        )
    }

    /// Its short name (the label unless one is typed).
    pub fn label(self, fr: bool) -> &'static str {
        let (f, e) = match self {
            ValueKind::Name => ("Source", "Source"),
            ValueKind::Umd => ("UMD", "UMD"),
            ValueKind::Format => ("Format", "Format"),
            ValueKind::Timecode => ("TC", "TC"),
            ValueKind::Momentary => ("M", "M"),
            ValueKind::ShortTerm => ("S", "S"),
            ValueKind::Integrated => ("I", "I"),
            ValueKind::LoudnessRange => ("LRA", "LRA"),
            ValueKind::TruePeak => ("TP", "TP"),
            ValueKind::PeakLeft => ("G", "L"),
            ValueKind::PeakRight => ("D", "R"),
            ValueKind::Codec => ("Codec", "Codec"),
            ValueKind::Picture => ("Image", "Picture"),
            ValueKind::ColorSpace => ("Colorimétrie", "Colour space"),
            ValueKind::Dynamic => ("Dynamique", "Dynamic range"),
            ValueKind::Levels => ("Niveaux", "Levels"),
            ValueKind::Phase => ("Phase", "Phase"),
            ValueKind::Audio => ("Son", "Sound"),
            ValueKind::Alarm => ("Alarme", "Alarm"),
            ValueKind::Tally => ("Tally", "Tally"),
            ValueKind::Time => ("Heure", "Time"),
            ValueKind::Date => ("Date", "Date"),
            ValueKind::StationTimecode => ("TC station", "House TC"),
            ValueKind::Ptp => ("PTP", "PTP"),
            ValueKind::PtpOffset => ("Écart PTP", "PTP offset"),
            ValueKind::NtpOffset => ("Écart NTP", "NTP offset"),
            ValueKind::Cpu => ("CPU", "CPU"),
            ValueKind::Memory => ("Mémoire", "Memory"),
            ValueKind::AlarmsOn => ("Alarmes", "Alarms"),
        };
        if fr {
            f
        } else {
            e
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ValueConfig {
    pub input: String,
    pub kind: ValueKind,
    /// The words before the value ("" = its short name).
    pub label: String,
    pub show_label: bool,
    /// The label over the value (else beside it).
    pub stack: bool,
    pub align: TextAlign,
    /// Amber, then red from these (0: none; offsets and phase by their size).
    pub warn: f32,
    pub crit: f32,
    /// The value's colour (white: the typography's).
    pub color: String,
    pub background: String,
}

impl Default for ValueConfig {
    fn default() -> Self {
        Self {
            input: String::new(),
            kind: ValueKind::Timecode,
            label: String::new(),
            show_label: true,
            stack: false,
            align: TextAlign::Center,
            warn: 0.0,
            crit: 0.0,
            color: WHITE.into(),
            background: "#0b0d12d9".into(),
        }
    }
}

/// What is known when the value is drawn.
pub struct Facts<'a> {
    pub source: Option<&'a SourceView>,
    pub system: &'a SystemView,
    pub now_ms: i64,
    pub tz: &'a str,
    pub fps: f64,
    pub lang: &'a str,
}

const DASH: &str = "—";

fn db(v: f32, unit: &str) -> String {
    if v <= -119.0 {
        DASH.into()
    } else {
        format!("{} {unit}", num1(v))
    }
}

/// The value now, its number (for the thresholds) and its own colour.
pub fn value(c: &ValueConfig, f: &Facts) -> (String, Option<f32>, Option<&'static str>) {
    let fr = f.lang.to_lowercase().starts_with("fr");
    let s = f.source;
    let v = s.map(|s| &s.video);
    let loud = s.and_then(|s| s.loudness);
    let last = |key: &str| -> Option<f32> {
        f.system
            .curves
            .series
            .get(key)
            .and_then(|x| x.iter().rev().flatten().next().copied())
    };
    let none = (DASH.to_string(), None, None);
    match c.kind {
        ValueKind::Name => s.map_or(none, |s| (s.name.clone(), None, None)),
        ValueKind::Umd => s.map_or(none, |s| {
            let u = if s.tally.umd.trim().is_empty() {
                s.name.clone()
            } else {
                s.tally.umd.clone()
            };
            (u, None, None)
        }),
        ValueKind::Format => match s.map(|s| s.format.clone()).filter(|x| !x.is_empty()) {
            Some(x) => (x, None, None),
            None => none,
        },
        ValueKind::Timecode => match s.map(|s| s.timecode.clone()).filter(|x| !x.is_empty()) {
            Some(x) => (x, None, None),
            None => none,
        },
        ValueKind::Momentary => {
            loud.map_or(none, |l| (db(l.momentary, "LUFS"), Some(l.momentary), None))
        }
        ValueKind::ShortTerm => loud.map_or(none, |l| {
            (db(l.short_term, "LUFS"), Some(l.short_term), None)
        }),
        ValueKind::Integrated => loud.map_or(none, |l| {
            (db(l.integrated, "LUFS"), Some(l.integrated), None)
        }),
        ValueKind::LoudnessRange => loud.map_or(none, |l| {
            (format!("{} LU", num1(l.range)), Some(l.range), None)
        }),
        ValueKind::TruePeak => {
            loud.map_or(none, |l| (db(l.true_peak, "dBTP"), Some(l.true_peak), None))
        }
        ValueKind::PeakLeft | ValueKind::PeakRight => {
            let i = usize::from(c.kind == ValueKind::PeakRight);
            match s.and_then(|s| s.levels.get(i)) {
                Some(l) => (db(l.peak, "dBFS"), Some(l.peak), None),
                None => none,
            }
        }
        ValueKind::Codec => match v.filter(|v| !v.codec.is_empty()) {
            Some(v) if v.profile.is_empty() => (v.codec.clone(), None, None),
            Some(v) => (format!("{} · {}", v.codec, v.profile), None, None),
            None => none,
        },
        ValueKind::Picture => match v {
            Some(v) if !v.size.is_empty() => {
                let mut p = vec![v.size.replace('x', "×")];
                if v.bits > 0 {
                    p.push(format!("{} bits", v.bits));
                }
                if !v.chroma.is_empty() {
                    p.push(v.chroma.clone());
                }
                (p.join(" · "), None, None)
            }
            _ => none,
        },
        ValueKind::ColorSpace => match v {
            Some(v) if !v.matrix.is_empty() || !v.primaries.is_empty() => {
                let s = if v.primaries.is_empty() {
                    matrix_name(&v.matrix)
                } else if v.matrix.is_empty() {
                    primaries_name(&v.primaries)
                } else {
                    format!(
                        "{} · {}",
                        matrix_name(&v.matrix),
                        primaries_name(&v.primaries)
                    )
                };
                (s, None, None)
            }
            _ => none,
        },
        ValueKind::Dynamic => match (s, v) {
            (Some(s), Some(v)) if s.live => {
                let d = v.dynamic();
                (d.to_string(), None, Some(dynamic_colors(d).0))
            }
            _ => none,
        },
        ValueKind::Levels => match v {
            Some(v) if v.luma.is_some() => {
                let (t, col) = levels_text(v, fr);
                (t, v.illegal, (col != WHITE).then_some(AMBER))
            }
            _ => none,
        },
        ValueKind::Phase => match s.and_then(|s| s.correlation) {
            Some(x) => {
                let r = (x.clamp(-1.0, 1.0) * 100.0).round() / 100.0;
                let t = if r < 0.0 {
                    format!("\u{2212}{:.2}", -r)
                } else {
                    format!("+{r:.2}")
                };
                let col = super::scope::correlation_color(x);
                (t, Some(x), Some(col))
            }
            None => none,
        },
        ValueKind::Audio => match v.and_then(|v| v.audio.first()) {
            Some(a) => (audio_line(a), None, None),
            None => none,
        },
        ValueKind::Alarm => match s {
            Some(s) => match s.alarms.first() {
                Some(a) => (
                    a.kind.text(f.lang).to_string(),
                    None,
                    Some(if a.severity == Severity::Critical {
                        RED
                    } else {
                        AMBER
                    }),
                ),
                None => ("OK".into(), None, Some(GREEN)),
            },
            None => none,
        },
        ValueKind::Tally => match s {
            Some(s) if s.tally.program => (
                (if fr { "ANTENNE" } else { "ON AIR" }).into(),
                None,
                Some(RED),
            ),
            Some(s) if s.tally.preview => (
                (if fr { "PRÉPARATION" } else { "PREVIEW" }).into(),
                None,
                Some(GREEN),
            ),
            Some(s) if s.tally.aux => ("AUX".into(), None, Some("#f2c12e")),
            Some(_) => (DASH.into(), None, Some(DIM)),
            None => none,
        },
        ValueKind::Time => (super::broadcast::clock(f.now_ms, f.tz), None, None),
        ValueKind::Date => (date(f.now_ms, f.tz, fr), None, None),
        ValueKind::StationTimecode => (
            super::broadcast::station_timecode(f.now_ms, f.tz, f.fps),
            None,
            None,
        ),
        ValueKind::Ptp => {
            let p = &f.system.ptp;
            if !p.running {
                (
                    (if fr { "Absent" } else { "Not running" }).into(),
                    None,
                    Some(DIM),
                )
            } else if p.locked {
                (
                    format!(
                        "{} · {} {}",
                        if fr { "Verrouillé" } else { "Locked" },
                        if fr { "domaine" } else { "domain" },
                        p.domain
                    ),
                    None,
                    Some(GREEN),
                )
            } else {
                (
                    (if fr { "Non verrouillé" } else { "Unlocked" }).into(),
                    None,
                    Some(RED),
                )
            }
        }
        ValueKind::PtpOffset => match f.system.ptp.offset_ns.filter(|_| f.system.ptp.running) {
            Some(o) => (
                value_text(ChartMetric::PtpOffset, o as f32),
                Some(o as f32),
                None,
            ),
            None => none,
        },
        ValueKind::NtpOffset => match f.system.ntp.offset_ms {
            Some(o) => (
                value_text(ChartMetric::NtpOffset, o as f32),
                Some(o as f32),
                None,
            ),
            None => none,
        },
        ValueKind::Cpu => match last("cpu") {
            Some(x) => (value_text(ChartMetric::Cpu, x), Some(x), None),
            None => none,
        },
        ValueKind::Memory => match last("memory") {
            Some(x) => (value_text(ChartMetric::Memory, x), Some(x), None),
            None => none,
        },
        ValueKind::AlarmsOn => {
            let on: Vec<_> = f
                .system
                .journal
                .iter()
                .filter(|e| e.ended.is_none())
                .collect();
            let col = if on.iter().any(|e| e.severity == Severity::Critical) {
                RED
            } else if on.is_empty() {
                GREEN
            } else {
                AMBER
            };
            (on.len().to_string(), Some(on.len() as f32), Some(col))
        }
    }
}

/// "01/11/2026" (French), "2026-11-01" (other languages).
pub fn date(ms: i64, tz: &str, fr: bool) -> String {
    let Ok(t) = jiff::Timestamp::from_millisecond(ms) else {
        return String::new();
    };
    let z = t.to_zoned(crate::widgets::zone(tz, ""));
    if fr {
        format!("{:02}/{:02}/{}", z.day(), z.month(), z.year())
    } else {
        format!("{}-{:02}-{:02}", z.year(), z.month(), z.day())
    }
}

/// The colour of the value: the thresholds first, then its own, then the set one.
pub fn color_of<'a>(c: &'a ValueConfig, n: Option<f32>, own: Option<&'static str>) -> &'a str {
    if let Some(n) = n {
        let size = match c.kind {
            ValueKind::PtpOffset | ValueKind::NtpOffset => n.abs(),
            _ => n,
        };
        if c.crit != 0.0 && size >= c.crit {
            return RED;
        }
        if c.warn != 0.0 && size >= c.warn {
            return AMBER;
        }
    }
    own.unwrap_or(&c.color)
}

/// The Value widget.
pub fn value_items(c: &ValueConfig, w: f32, h: f32, f: &Facts) -> Vec<Item> {
    let fr = f.lang.to_lowercase().starts_with("fr");
    let pad = pad_of(w, h);
    let mut out = vec![rect([0.0, 0.0, w, h], pad * 0.6, &c.background)];
    let (v, n, own) = value(c, f);
    let col = color_of(c, n, own);
    let label = if c.label.trim().is_empty() {
        c.kind.label(fr).to_string()
    } else {
        c.label.clone()
    };
    let (iw, ih) = (w - 2.0 * pad, h - 2.0 * pad);
    if c.show_label && c.stack {
        let lh = ih * 0.32;
        out.push(text(
            [pad, pad, iw, lh],
            label,
            lh * 0.8,
            600,
            false,
            DIM,
            c.align,
        ));
        out.push(text(
            [pad, pad + lh, iw, ih - lh],
            v,
            (ih - lh) * 0.72,
            700,
            true,
            col,
            c.align,
        ));
    } else if c.show_label {
        let size = (ih * 0.62).min(iw * 0.2);
        let lw = (label.chars().count() as f32 * size * 0.62 + size * 0.6).min(iw * 0.45);
        out.push(text(
            [pad, pad, lw, ih],
            label,
            size * 0.8,
            600,
            false,
            DIM,
            TextAlign::Left,
        ));
        out.push(text(
            [pad + lw, pad, iw - lw, ih],
            v,
            size,
            700,
            true,
            col,
            TextAlign::Right,
        ));
    } else {
        out.push(text(
            [pad, pad, iw, ih],
            v,
            ih * 0.72,
            700,
            true,
            col,
            c.align,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{Loudness, Tally};

    fn facts<'a>(s: Option<&'a SourceView>, sys: &'a SystemView) -> Facts<'a> {
        Facts {
            source: s,
            system: sys,
            now_ms: 1_793_520_000_000,
            tz: "UTC",
            fps: 25.0,
            lang: "fr",
        }
    }

    #[test]
    fn each_value_reads() {
        let sys = SystemView::default();
        let s = SourceView {
            name: "CAM 1".into(),
            format: "1080i50".into(),
            timecode: "10:00:00:12".into(),
            tally: Tally {
                program: true,
                ..Default::default()
            },
            loudness: Some(Loudness {
                momentary: -20.0,
                short_term: -21.5,
                integrated: -23.0,
                range: 6.4,
                true_peak: -1.2,
            }),
            ..Default::default()
        };
        let f = facts(Some(&s), &sys);
        let get = |kind: ValueKind| {
            value(
                &ValueConfig {
                    kind,
                    ..Default::default()
                },
                &f,
            )
        };
        assert_eq!(get(ValueKind::Umd).0, "CAM 1");
        assert_eq!(get(ValueKind::Format).0, "1080i50");
        assert_eq!(get(ValueKind::Timecode).0, "10:00:00:12");
        assert_eq!(get(ValueKind::Integrated).0, "\u{2212}23.0 LUFS");
        assert_eq!(get(ValueKind::LoudnessRange).0, "6.4 LU");
        assert_eq!(
            get(ValueKind::Tally),
            ("ANTENNE".to_string(), None, Some(RED))
        );
        assert_eq!(get(ValueKind::Alarm).0, "OK");
        assert_eq!(get(ValueKind::Codec).0, DASH);
        assert_eq!(get(ValueKind::Date).0, "01/11/2026");
        assert_eq!(get(ValueKind::Ptp).0, "Absent");
        // Thresholds: −1.2 dBTP from −2 (amber), −1 (red).
        let c = ValueConfig {
            kind: ValueKind::TruePeak,
            warn: -2.0,
            crit: -1.0,
            ..Default::default()
        };
        let (_, n, own) = value(&c, &f);
        assert_eq!(color_of(&c, n, own), AMBER);
    }

    #[test]
    fn lays_out_with_and_without_its_label() {
        let sys = SystemView::default();
        let f = facts(None, &sys);
        for (show, stack) in [(true, false), (true, true), (false, false)] {
            let c = ValueConfig {
                kind: ValueKind::Time,
                show_label: show,
                stack,
                ..Default::default()
            };
            let items = value_items(&c, 400.0, 100.0, &f);
            let texts = items
                .iter()
                .filter(|i| matches!(i, Item::Text { .. }))
                .count();
            assert_eq!(texts, if show { 2 } else { 1 });
        }
    }
}

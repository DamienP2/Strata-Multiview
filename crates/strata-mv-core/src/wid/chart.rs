//! The Chart widget: a measure of the plant over the last minutes, as a
//! curve — PTP and NTP offsets, processors, memory, the time to draw a
//! picture, an input's pictures a second or its loudness. Several make a
//! dashboard in a multiview. Laid out as items like the other broadcast
//! widgets, drawn by both renderers (`web/src/widgets/chart.tsx` mirrors
//! this module: keep them in sync).

use serde::{Deserialize, Serialize};

use super::broadcast::{pad_of, path, rect, text, Item, AMBER, DIM, RED, WHITE};
use crate::model::TextAlign;
use crate::signal::{Curves, SourceView};

/// What a chart shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartMetric {
    /// The PTP offset from the grandmaster (ns).
    #[default]
    PtpOffset,
    /// The system clock's offset from NTP (ms).
    NtpOffset,
    /// The machine's processors (%).
    Cpu,
    /// The machine's memory (%).
    Memory,
    /// Strata Multiview's own processor use (%).
    ServerCpu,
    /// The decoders' and encoders' processor use (%).
    FfmpegCpu,
    /// The time to draw a picture of a channel (ms).
    RenderTime,
    /// The pictures a second of an input.
    InputFps,
    /// The short-term loudness of an input (LUFS).
    Loudness,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChartConfig {
    pub metric: ChartMetric,
    /// The input (pictures a second, loudness).
    pub input: String,
    /// The channel (render time; "" the first).
    pub channel: String,
    /// "" = the measure's name.
    pub title: String,
    /// Seconds shown (30 to 300).
    pub window: u32,
    /// The scale (both 0: automatic).
    pub min: f32,
    pub max: f32,
    /// Amber, then red above these (0: none; PTP and NTP: the offset's size).
    pub warn: f32,
    pub crit: f32,
    pub color: String,
    /// The area under the curve, tinted.
    pub fill: bool,
    /// Lowest, mean and highest of the window, under it.
    pub stats: bool,
    pub background: String,
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            metric: ChartMetric::PtpOffset,
            input: String::new(),
            channel: String::new(),
            title: String::new(),
            window: 120,
            min: 0.0,
            max: 0.0,
            warn: 0.0,
            crit: 0.0,
            color: "#4ed8ff".into(),
            fill: true,
            stats: true,
            background: "#0b0d12d9".into(),
        }
    }
}

/// The key of its measure in `Curves` (None: from the input, loudness).
pub fn key_of(c: &ChartConfig, curves: &Curves) -> Option<String> {
    Some(match c.metric {
        ChartMetric::PtpOffset => "ptp".into(),
        ChartMetric::NtpOffset => "ntp".into(),
        ChartMetric::Cpu => "cpu".into(),
        ChartMetric::Memory => "memory".into(),
        ChartMetric::ServerCpu => "server".into(),
        ChartMetric::FfmpegCpu => "ffmpeg".into(),
        ChartMetric::RenderTime => {
            let ch = c.channel.trim();
            if ch.is_empty() {
                curves
                    .series
                    .keys()
                    .find(|k| k.starts_with("render:"))
                    .cloned()
                    .unwrap_or_else(|| "render:main".into())
            } else {
                format!("render:{ch}")
            }
        }
        ChartMetric::InputFps => format!("fps:{}", c.input.trim()),
        ChartMetric::Loudness => return None,
    })
}

/// Its name, in the project's language.
pub fn metric_name(m: ChartMetric, fr: bool) -> &'static str {
    match (m, fr) {
        (ChartMetric::PtpOffset, true) => "Écart PTP",
        (ChartMetric::PtpOffset, false) => "PTP offset",
        (ChartMetric::NtpOffset, true) => "Écart NTP",
        (ChartMetric::NtpOffset, false) => "NTP offset",
        (ChartMetric::Cpu, true) => "Processeurs",
        (ChartMetric::Cpu, false) => "Processors",
        (ChartMetric::Memory, true) => "Mémoire",
        (ChartMetric::Memory, false) => "Memory",
        (ChartMetric::ServerCpu, _) => "Strata Multiview",
        (ChartMetric::FfmpegCpu, true) => "Décodeurs (ffmpeg)",
        (ChartMetric::FfmpegCpu, false) => "Decoders (ffmpeg)",
        (ChartMetric::RenderTime, true) => "Temps de rendu",
        (ChartMetric::RenderTime, false) => "Render time",
        (ChartMetric::InputFps, true) => "Images par seconde",
        (ChartMetric::InputFps, false) => "Pictures a second",
        (ChartMetric::Loudness, true) => "Loudness court terme",
        (ChartMetric::Loudness, false) => "Short-term loudness",
    }
}

/// A real minus sign.
fn minus(s: String) -> String {
    match s.strip_prefix('-') {
        Some(r) => format!("\u{2212}{r}"),
        None => s,
    }
}

/// A value with its unit ("−120 ns", "1.4 µs", "0.42 ms", "38 %").
pub fn value_text(m: ChartMetric, v: f32) -> String {
    minus(match m {
        ChartMetric::PtpOffset => {
            let a = v.abs();
            if a < 1000.0 {
                format!("{v:.0} ns")
            } else if a < 1_000_000.0 {
                format!("{:.1} µs", v / 1000.0)
            } else {
                format!("{:.2} ms", v / 1_000_000.0)
            }
        }
        ChartMetric::NtpOffset => format!("{v:.2} ms"),
        ChartMetric::Cpu
        | ChartMetric::Memory
        | ChartMetric::ServerCpu
        | ChartMetric::FfmpegCpu => {
            format!("{v:.0} %")
        }
        ChartMetric::RenderTime => format!("{v:.1} ms"),
        ChartMetric::InputFps => format!("{v:.1} fps"),
        ChartMetric::Loudness => format!("{v:.1} LUFS"),
    })
}

/// The measures of the window, the newest last.
pub fn values(c: &ChartConfig, curves: &Curves, source: Option<&SourceView>) -> Vec<Option<f32>> {
    let window = c.window.clamp(30, 300) as usize;
    let all: Vec<Option<f32>> = match key_of(c, curves) {
        Some(k) => curves.series.get(&k).cloned().unwrap_or_default(),
        None => source
            .map(|s| {
                s.history
                    .iter()
                    .map(|v| (*v > -70.0).then_some(*v))
                    .collect()
            })
            .unwrap_or_default(),
    };
    let from = all.len().saturating_sub(window);
    all[from..].to_vec()
}

/// The scale: as set, else by the measure (percents 0–100, offsets around
/// zero, the others around what they were).
pub fn scale(c: &ChartConfig, vals: &[Option<f32>]) -> (f32, f32) {
    if c.max > c.min {
        return (c.min, c.max);
    }
    let known: Vec<f32> = vals.iter().flatten().copied().collect();
    let (lo, hi) = known
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    match c.metric {
        ChartMetric::Cpu
        | ChartMetric::Memory
        | ChartMetric::ServerCpu
        | ChartMetric::FfmpegCpu => (0.0, 100.0),
        ChartMetric::PtpOffset | ChartMetric::NtpOffset => {
            let floor: f32 = if c.metric == ChartMetric::PtpOffset {
                100.0
            } else {
                1.0
            };
            let m = known.iter().fold(floor, |a, v| a.max(v.abs())) * 1.2;
            (-m, m)
        }
        ChartMetric::RenderTime => (
            0.0,
            if known.is_empty() {
                40.0
            } else {
                (hi * 1.25).max(10.0)
            },
        ),
        ChartMetric::InputFps => {
            if known.is_empty() {
                (0.0, 60.0)
            } else {
                ((lo - 2.0).max(0.0).floor(), (hi + 2.0).ceil())
            }
        }
        ChartMetric::Loudness => {
            if known.is_empty() {
                (-40.0, -10.0)
            } else {
                ((lo - 3.0).min(-30.0).floor(), (hi + 3.0).max(-15.0).ceil())
            }
        }
    }
}

/// The colour of a value against the thresholds.
pub fn level_color(c: &ChartConfig, v: f32) -> &str {
    let size = match c.metric {
        ChartMetric::PtpOffset | ChartMetric::NtpOffset => v.abs(),
        _ => v,
    };
    if c.crit > 0.0 && size >= c.crit {
        RED
    } else if c.warn > 0.0 && size >= c.warn {
        AMBER
    } else {
        &c.color
    }
}

/// The Chart widget.
pub fn chart(
    c: &ChartConfig,
    w: f32,
    h: f32,
    curves: &Curves,
    source: Option<&SourceView>,
    lang: &str,
) -> Vec<Item> {
    let fr = lang.to_lowercase().starts_with("fr");
    let pad = pad_of(w, h);
    let mut out = vec![rect([0.0, 0.0, w, h], pad * 0.6, &c.background)];
    let vals = values(c, curves, source);
    let last = vals.iter().rev().flatten().next().copied();
    let fs = (h * 0.07).min(w * 0.045).clamp(8.0, 24.0).round();
    let head = fs * 1.8;
    // Its name (and the input's), its value now.
    let mut title = c.title.trim().to_string();
    if title.is_empty() {
        title = metric_name(c.metric, fr).to_string();
        if matches!(c.metric, ChartMetric::InputFps | ChartMetric::Loudness) {
            if let Some(s) = source.filter(|s| !s.name.is_empty()) {
                title = format!("{title} · {}", s.name);
            }
        }
    }
    let vw = (w - 2.0 * pad) * 0.42;
    out.push(text(
        [pad, pad, w - 2.0 * pad - vw, head],
        title,
        fs,
        600,
        false,
        WHITE,
        TextAlign::Left,
    ));
    out.push(text(
        [w - pad - vw, pad, vw, head],
        last.map_or("—".to_string(), |v| value_text(c.metric, v)),
        fs * 1.25,
        700,
        true,
        last.map_or(DIM, |v| level_color(c, v)),
        TextAlign::Right,
    ));
    // The plot, its scale on the left, its figures under it.
    let foot = if c.stats { fs * 1.4 } else { 0.0 };
    let label_w = fs * 3.6;
    let plot = [
        pad + label_w,
        pad + head + fs * 0.4,
        (w - 2.0 * pad - label_w).max(1.0),
        (h - 2.0 * pad - head - fs * 0.4 - foot).max(1.0),
    ];
    let (lo, hi) = scale(c, &vals);
    let span = (hi - lo).max(1e-6);
    let y_of = |v: f32| plot[1] + plot[3] * (1.0 - ((v - lo) / span).clamp(0.0, 1.0));
    let lw = (h / 300.0).clamp(1.0, 2.5);
    for (i, v) in [hi, (hi + lo) / 2.0, lo].iter().enumerate() {
        let y = y_of(*v);
        out.push(path(
            vec![[plot[0], y], [plot[0] + plot[2], y]],
            false,
            1.0,
            if i == 1 { "#ffffff14" } else { "#ffffff24" },
        ));
        out.push(text(
            [pad, y - fs * 0.55, label_w - fs * 0.4, fs * 1.1],
            value_text(c.metric, *v),
            fs * 0.62,
            500,
            true,
            DIM,
            TextAlign::Right,
        ));
    }
    // The thresholds (both signs for an offset).
    for (t, col) in [(c.warn, AMBER), (c.crit, RED)] {
        if t <= 0.0 {
            continue;
        }
        let signs: &[f32] = if matches!(c.metric, ChartMetric::PtpOffset | ChartMetric::NtpOffset) {
            &[1.0, -1.0]
        } else {
            &[1.0]
        };
        for s in signs {
            let v = t * s;
            if v > lo && v < hi {
                let y = y_of(v);
                out.push(path(
                    vec![[plot[0], y], [plot[0] + plot[2], y]],
                    false,
                    1.0,
                    &format!("{col}99"),
                ));
            }
        }
    }
    // The curve: one point a second, the newest on the right; a gap where
    // nothing was measured.
    let window = c.window.clamp(30, 300) as f32;
    let step = plot[2] / (window - 1.0).max(1.0);
    let n = vals.len();
    let mut runs: Vec<Vec<[f32; 2]>> = vec![Vec::new()];
    for (i, v) in vals.iter().enumerate() {
        let x = plot[0] + plot[2] - (n - 1 - i) as f32 * step;
        match v {
            Some(v) => runs.last_mut().expect("one run").push([x, y_of(*v)]),
            None => {
                if !runs.last().expect("one run").is_empty() {
                    runs.push(Vec::new());
                }
            }
        }
    }
    // Filled to zero when the scale holds it, else to its bottom.
    let base = y_of(if lo < 0.0 && hi > 0.0 { 0.0 } else { lo });
    for run in runs.into_iter().filter(|r| !r.is_empty()) {
        if c.fill && run.len() > 1 {
            let mut area = run.clone();
            area.push([run[run.len() - 1][0], base]);
            area.push([run[0][0], base]);
            out.push(Item::Path {
                points: area,
                closed: true,
                width: 0.0,
                color: c.color.clone(),
                fill: Some(format!("{}33", &c.color[..c.color.len().min(7)])),
            });
        }
        let points = if run.len() == 1 {
            vec![run[0], [run[0][0] + 1.0, run[0][1]]]
        } else {
            run
        };
        out.push(path(points, false, lw, &c.color));
    }
    if c.stats {
        let known: Vec<f32> = vals.iter().flatten().copied().collect();
        let s = if known.is_empty() {
            "—".to_string()
        } else {
            let (a, b) = known
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
            let mean = known.iter().sum::<f32>() / known.len() as f32;
            let (lo_w, mean_w, hi_w) = if fr {
                ("min", "moy.", "max")
            } else {
                ("min", "mean", "max")
            };
            format!(
                "{lo_w} {} · {mean_w} {} · {hi_w} {}",
                value_text(c.metric, a),
                value_text(c.metric, mean),
                value_text(c.metric, b)
            )
        };
        out.push(text(
            [plot[0], h - pad - foot, plot[2], foot],
            s,
            fs * 0.68,
            500,
            true,
            DIM,
            TextAlign::Right,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curves(key: &str, v: &[Option<f32>]) -> Curves {
        let mut c = Curves {
            end_ms: 1,
            ..Default::default()
        };
        c.series.insert(key.into(), v.to_vec());
        c
    }

    #[test]
    fn values_with_their_units() {
        assert_eq!(value_text(ChartMetric::PtpOffset, -120.4), "\u{2212}120 ns");
        assert_eq!(value_text(ChartMetric::PtpOffset, 1460.0), "1.5 µs");
        assert_eq!(value_text(ChartMetric::NtpOffset, 0.4213), "0.42 ms");
        assert_eq!(value_text(ChartMetric::Cpu, 37.6), "38 %");
    }

    #[test]
    fn a_ptp_curve_with_a_gap() {
        let c = ChartConfig::default();
        let cv = curves(
            "ptp",
            &[Some(20.0), Some(-40.0), None, Some(15.0), Some(10.0)],
        );
        let items = chart(&c, 900.0, 600.0, &cv, None, "fr");
        let curves = items
            .iter()
            .filter(|i| matches!(i, Item::Path { width, .. } if *width > 1.0))
            .count();
        // Two runs, either side of the gap.
        assert_eq!(curves, 2);
        let (lo, hi) = scale(&c, &values(&c, &cv, None));
        assert!(lo < -40.0 && hi > 40.0 && (lo + hi).abs() < 1e-3);
        let texts: Vec<String> = items
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"10 ns".to_string()));
        assert!(texts.contains(&"Écart PTP".to_string()));
    }

    #[test]
    fn thresholds_colour_the_value() {
        let c = ChartConfig {
            warn: 1000.0,
            crit: 10_000.0,
            ..Default::default()
        };
        assert_eq!(level_color(&c, -2000.0), AMBER);
        assert_eq!(level_color(&c, 20_000.0), RED);
        assert_eq!(level_color(&c, 50.0), c.color);
    }

    #[test]
    fn the_window_keeps_the_newest() {
        let c = ChartConfig {
            window: 30,
            metric: ChartMetric::Cpu,
            ..Default::default()
        };
        let v: Vec<Option<f32>> = (0..100).map(|i| Some(i as f32)).collect();
        let w = values(&c, &curves("cpu", &v), None);
        assert_eq!(w.len(), 30);
        assert_eq!(w.last(), Some(&Some(99.0)));
    }
}

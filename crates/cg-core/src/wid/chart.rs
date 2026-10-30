//! Chart: columns (grouped / stacked / 100 %), areas (simple / stacked),
//! lines and pie. Settings, operator state and actions; the picture is laid
//! out here (`chart_layout.rs`) as a list of shapes and texts that
//! `cg-render/src/wid/chart.rs` and `web/src/widgets/chart.tsx` (mirror in
//! `chart-layout.tsx`) both draw.
//!
//! Update animation: the state keeps the values shown at the last Reload
//! (`prev`), the data of that Reload (`cur`) and its time (`at`); the
//! renderers tween from `prev` to `cur`. Entrance animations run from the
//! time on air; reveal steps and new categories from their own timestamps.

use serde::{Deserialize, Serialize};

pub use super::table::AutoOn;
use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind};

#[path = "chart_layout.rs"]
mod layout;
pub use layout::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartType {
    #[default]
    Columns,
    Areas,
    Lines,
    Pie,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColMode {
    #[default]
    Grouped,
    Stacked,
    Percent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AreaMode {
    #[default]
    Simple,
    Stacked,
}

/// Where the numbers come from: the grid (cells may hold `{{key}}`) or a
/// CSV text (which may be a `{{key}}` whose data value is CSV).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    #[default]
    Grid,
    Csv,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Missing {
    #[default]
    Gap,
    Interp,
    Zero,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sep {
    #[default]
    Space,
    Comma,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Axis {
    pub auto: bool,
    pub min: f32,
    pub max: f32,
    pub ticks: u32,
    pub decimals: u32,
    pub prefix: String,
    pub suffix: String,
    pub sep: Sep,
    pub grid: bool,
    pub vgrid: bool,
    /// Lines: the auto range includes zero.
    pub zero: bool,
    pub x_title: String,
    pub y_title: String,
}

impl Default for Axis {
    fn default() -> Self {
        Self {
            auto: true,
            min: 0.0,
            max: 50.0,
            ticks: 5,
            decimals: 0,
            prefix: String::new(),
            suffix: " %".into(),
            sep: Sep::Space,
            grid: true,
            vgrid: false,
            zero: true,
            x_title: String::new(),
            y_title: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Marker {
    None,
    #[default]
    Circle,
    Square,
    Diamond,
}

/// Style of one series (unset fields: palette colour, 6 px, circles).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SeriesStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<Marker>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PieSort {
    None,
    #[default]
    Desc,
    Asc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pie {
    /// Index of the series shown.
    pub series: i32,
    /// Degrees, 0 = top, clockwise.
    pub start: f32,
    pub sort: PieSort,
    /// Hole, % of the radius.
    pub donut: f32,
    /// Row (category) index of the exploded slice, -1 = none.
    pub explode: i32,
    pub explode_dist: f32,
    /// Slices under `others_pct` % grouped in "Others".
    pub others: bool,
    pub others_pct: f32,
    pub center_total: bool,
}

impl Default for Pie {
    fn default() -> Self {
        Self {
            series: 0,
            start: 0.0,
            sort: PieSort::Desc,
            donut: 0.0,
            explode: -1,
            explode_dist: 36.0,
            others: false,
            others_pct: 4.0,
            center_total: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LabelPos {
    Inside,
    #[default]
    Outside,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Overlap {
    #[default]
    Offset,
    Hide,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Labels {
    pub value: bool,
    pub pct: bool,
    pub name: bool,
    pub pos: LabelPos,
    pub overlap: Overlap,
    /// Pie: no label under this share (%), or outside with Offset.
    pub min_size: f32,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            value: true,
            pct: false,
            name: false,
            pos: LabelPos::Outside,
            overlap: Overlap::Offset,
            min_size: 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TitlePos {
    #[default]
    Left,
    Center,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Title {
    pub text: String,
    pub sub: String,
    pub pos: TitlePos,
}

impl Default for Title {
    fn default() -> Self {
        Self {
            text: "Voting intentions".into(),
            sub: "Poll · first round · % of voters".into(),
            pos: TitlePos::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LegendPos {
    #[default]
    Top,
    Bottom,
    Right,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Panel {
    pub on: bool,
    pub color: String,
    pub opacity: f32,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            on: true,
            color: "#120e24".into(),
            opacity: 0.88,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fx {
    /// Columns grow, lines and areas draw, the pie sweeps.
    #[default]
    Auto,
    Grow,
    Draw,
    Sweep,
    Fade,
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Curve {
    #[default]
    Ease,
    Out,
    Linear,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Anim {
    pub fx: Fx,
    /// Entrance, seconds.
    pub dur: f32,
    /// Between two elements, ms.
    pub stagger: f32,
    /// Update tween, seconds (0 = cut).
    pub upd: f32,
    pub curve: Curve,
}

impl Default for Anim {
    fn default() -> Self {
        Self {
            fx: Fx::Auto,
            dur: 0.9,
            stagger: 110.0,
            upd: 0.8,
            curve: Curve::Ease,
        }
    }
}

/// Columns: bars standing (vertical) or lying (horizontal, names on the left).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orient {
    #[default]
    V,
    H,
}

/// New data while on air: shown at once with the update tween (Auto), or
/// kept until the operator's Reload (the data at the take stays).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OnAir {
    Auto,
    #[default]
    Reload,
}

/// A line across the values (a goal, a majority…).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RefLine {
    pub on: bool,
    pub value: f32,
    pub label: String,
}

/// Texts the chart writes itself; empty = in the project's language.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Texts {
    pub no_data: String,
    pub others: String,
    pub total: String,
}

/// A text the chart writes: the Designer's, else in the project language.
pub fn word(own: &str, en: &str, fr: &str, lang: &str) -> String {
    if !own.trim().is_empty() {
        own.to_string()
    } else if lang.starts_with("fr") {
        fr.to_string()
    } else {
        en.to_string()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RevealMode {
    #[default]
    All,
    Series,
    Categories,
}

/// Settings of the widget, flattened into the layer's JSON (every field
/// has a default). The text size is the animated `fontSize` property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub chart_type: ChartType,
    pub col_mode: ColMode,
    pub area_mode: AreaMode,
    pub source: Source,
    /// First row: header (first cell = categories title, then series
    /// names); first column: categories.
    pub data: Vec<Vec<String>>,
    pub csv: String,
    pub missing: Missing,
    pub axis: Axis,
    pub series: Vec<SeriesStyle>,
    pub slice_colors: Vec<String>,
    pub smooth: bool,
    pub area_opacity: f32,
    pub pie: Pie,
    pub labels: Labels,
    pub title: Title,
    pub legend: LegendPos,
    pub panel: Panel,
    pub anim: Anim,
    pub reveal: RevealMode,
    pub orient: Orient,
    pub on_air: OnAir,
    /// Font family of the texts (weights by role).
    pub font: String,
    /// Colour of the texts (empty = white and lavender).
    pub text_color: String,
    pub ref_line: RefLine,
    pub texts: Texts,
    /// Sparkline: the first series alone filling the box (columns, line
    /// or area), without panel, titles, axes or texts.
    pub spark: bool,
    /// The reveal steps are chained from the take: every `auto_secs`
    /// seconds, on Continue, or on a trigger (`auto_on`).
    pub auto_steps: bool,
    pub auto_secs: f64,
    pub auto_on: AutoOn,
    pub auto_trigger: String,
    /// Put forward from the take: "" = nothing, `s2` = series 3, `c0` =
    /// the first category (column, point or slice).
    pub hl_default: String,
}

/// The poll sample of a new chart.
pub fn sample() -> Vec<Vec<String>> {
    [
        ["Month", "Martin", "Durand", "Petit"],
        ["Jan", "28", "24", "12"],
        ["Feb", "30", "23", "13"],
        ["Mar", "27", "26", "14"],
        ["Apr", "31", "25", "11"],
        ["May", "33", "24", "12"],
        ["Jun", "32", "27", "10"],
    ]
    .iter()
    .map(|r| r.iter().map(|s| s.to_string()).collect())
    .collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            chart_type: ChartType::Columns,
            col_mode: ColMode::Grouped,
            area_mode: AreaMode::Simple,
            source: Source::Grid,
            data: sample(),
            csv: String::new(),
            missing: Missing::Gap,
            axis: Axis::default(),
            series: Vec::new(),
            slice_colors: Vec::new(),
            smooth: false,
            area_opacity: 0.45,
            pie: Pie::default(),
            labels: Labels::default(),
            title: Title::default(),
            legend: LegendPos::Top,
            panel: Panel::default(),
            anim: Anim::default(),
            reveal: RevealMode::All,
            orient: Orient::V,
            on_air: OnAir::Reload,
            font: "Inter".into(),
            text_color: String::new(),
            ref_line: RefLine::default(),
            texts: Texts::default(),
            spark: false,
            auto_steps: false,
            auto_secs: 5.0,
            auto_on: AutoOn::Timed,
            auto_trigger: String::new(),
            hl_default: String::new(),
        }
    }
}

// ------------------------------------------------------------------ state --

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HlKind {
    #[default]
    None,
    Series,
    /// A column / point of the category, or a pie slice.
    Category,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hl {
    pub kind: HlKind,
    /// Series index, or the row index of the category.
    pub idx: i32,
}

impl Default for Hl {
    fn default() -> Self {
        Self {
            kind: HlKind::None,
            idx: -1,
        }
    }
}

/// What was on screen for one category when the data was reloaded.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PrevCat {
    pub name: String,
    /// Slot position (0 = first).
    pub slot: f32,
    /// Values by series (same order as `State::prev_series`).
    pub vals: Vec<Option<f32>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// The data (resolved grid) of the last Reload; none = the live data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<Vec<Vec<String>>>,
    /// Values shown when it was reloaded (the tween starts from them).
    pub prev: Vec<PrevCat>,
    pub prev_series: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_axis: Option<[f32; 2]>,
    pub prev_n: f32,
    /// Reload time (Unix ms), 0 = never.
    pub at: i64,
    /// Reveal mode chosen by the operator (none = the Designer's).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<RevealMode>,
    /// Steps shown (≥ 1).
    pub step: u32,
    /// Time each step was revealed (Unix ms; 0 = with the entrance).
    pub step_at: Vec<i64>,
    pub hl: Hl,
    pub prev_hl: Hl,
    pub hl_at: i64,
    /// The operator acted (else the Designer's chained steps and highlight apply from the take).
    pub ctl: bool,
    /// Steps chained: one more every `interval` s counted from `auto_at`
    /// (`on` timed), or on Continue / on the trigger `trigger` (`interval`
    /// then 0). The step shown is `step` plus the intervals elapsed.
    pub auto: bool,
    pub interval: f64,
    pub auto_at: i64,
    pub on: AutoOn,
    pub trigger: String,
    /// Paused at (Unix ms), 0 = running.
    pub paused_at: i64,
}

// ------------------------------------------------------------------- data --

/// Rows of a CSV text (`,` `;` or tab; quotes allowed) — mirror of the kit's parseCsv.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.replace('\r', "");
    let lines: Vec<&str> = text.split('\n').filter(|l| !l.trim().is_empty()).collect();
    let first = lines.first().copied().unwrap_or("");
    let sep = [';', '\t', ',']
        .into_iter()
        .find(|s| first.contains(*s))
        .unwrap_or(',');
    lines
        .iter()
        .map(|l| {
            let ch: Vec<char> = l.chars().collect();
            let mut out = Vec::new();
            let mut cur = String::new();
            let mut q = false;
            let mut i = 0;
            while i < ch.len() {
                let c = ch[i];
                if q {
                    if c == '"' && ch.get(i + 1) == Some(&'"') {
                        cur.push('"');
                        i += 1;
                    } else if c == '"' {
                        q = false;
                    } else {
                        cur.push(c);
                    }
                } else if c == '"' {
                    q = true;
                } else if c == sep {
                    out.push(std::mem::take(&mut cur));
                } else {
                    cur.push(c);
                }
                i += 1;
            }
            out.push(cur);
            out.into_iter().map(|s| s.trim().to_string()).collect()
        })
        .collect()
}

/// The grid with the data values in (CSV source parsed).
pub fn resolve_grid(c: &Config, data: &Data) -> Vec<Vec<String>> {
    match c.source {
        // CSV, or JSON rows (a data set's table).
        Source::Csv => {
            let t = interpolate_text(&c.csv, data);
            if let Some(g) = number_list(&t) {
                g
            } else if t.trim_start().starts_with('[') {
                super::table::parse_rows(&t)
            } else {
                parse_csv(&t)
            }
        }
        Source::Grid => c
            .data
            .iter()
            .map(|r| r.iter().map(|s| interpolate_text(s, data)).collect())
            .collect(),
    }
}

/// A JSON list of numbers (`[58, 61, …]`: a count per minute) as one
/// series, its rows numbered from 1.
fn number_list(t: &str) -> Option<Vec<Vec<String>>> {
    let v: Vec<serde_json::Value> = serde_json::from_str(t.trim()).ok()?;
    if v.is_empty() || !v.iter().all(|x| x.is_number()) {
        return None;
    }
    let mut g = vec![vec![String::new(), "Value".to_string()]];
    for (i, x) in v.iter().enumerate() {
        g.push(vec![(i + 1).to_string(), x.to_string()]);
    }
    Some(g)
}

/// The settings with the data values in the titles.
pub fn with_data(c: &Config, data: &Data) -> Config {
    let mut c = c.clone();
    c.title.text = interpolate_text(&c.title.text, data);
    c.title.sub = interpolate_text(&c.title.sub, data);
    c.axis.x_title = interpolate_text(&c.axis.x_title, data);
    c.axis.y_title = interpolate_text(&c.axis.y_title, data);
    c.ref_line.label = interpolate_text(&c.ref_line.label, data);
    c
}

/// A number of a cell ("1 234,5" too); blank, "-" or "n/a" = missing.
pub fn num(v: &str) -> Option<f32> {
    let t: String = v.trim().chars().filter(|c| !c.is_whitespace()).collect();
    let t = t.replacen(',', ".", 1);
    let low = t.to_ascii_lowercase();
    if t.is_empty() || t == "-" || low == "na" || low == "n/a" {
        return None;
    }
    t.parse::<f32>().ok().filter(|n| n.is_finite())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cat {
    pub name: String,
    /// Values as typed (None = missing).
    pub raw: Vec<Option<f32>>,
    /// Values after the missing rule.
    pub vals: Vec<Option<f32>>,
    pub interp: Vec<bool>,
    /// Row index (identity for highlight / explode).
    pub ri: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub ser: Vec<String>,
    pub cats: Vec<Cat>,
    pub missing: usize,
}

pub fn parse(c: &Config, rows: &[Vec<String>]) -> Parsed {
    let head: Vec<String> = rows
        .first()
        .cloned()
        .unwrap_or_else(|| vec!["Category".into()]);
    let ser: Vec<String> = head
        .iter()
        .skip(1)
        .enumerate()
        .map(|(j, n)| {
            let n = n.trim();
            if n.is_empty() {
                format!("Series {}", j + 1)
            } else {
                n.to_string()
            }
        })
        .collect();
    let mut cats: Vec<Cat> = Vec::new();
    for (k, row) in rows.iter().skip(1).enumerate() {
        let name = row
            .first()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let raw: Vec<Option<f32>> = (0..ser.len())
            .map(|j| row.get(j + 1).and_then(|s| num(s)))
            .collect();
        if name.is_empty() && raw.iter().all(|v| v.is_none()) {
            continue;
        }
        cats.push(Cat {
            name: if name.is_empty() { "—".into() } else { name },
            raw: raw.clone(),
            vals: raw,
            interp: vec![false; ser.len()],
            ri: k,
        });
    }
    let mut missing = 0;
    for j in 0..ser.len() {
        for i in 0..cats.len() {
            if cats[i].raw[j].is_some() {
                continue;
            }
            missing += 1;
            match c.missing {
                Missing::Zero => cats[i].vals[j] = Some(0.0),
                Missing::Interp => {
                    let mut a = i as i64 - 1;
                    let mut b = i + 1;
                    while a >= 0 && cats[a as usize].raw[j].is_none() {
                        a -= 1;
                    }
                    while b < cats.len() && cats[b].raw[j].is_none() {
                        b += 1;
                    }
                    if a >= 0 && b < cats.len() {
                        let (va, vb) = (cats[a as usize].raw[j].unwrap(), cats[b].raw[j].unwrap());
                        cats[i].vals[j] =
                            Some(va + (vb - va) * (i as f32 - a as f32) / (b as f32 - a as f32));
                        cats[i].interp[j] = true;
                    }
                }
                Missing::Gap => {}
            }
        }
    }
    Parsed { ser, cats, missing }
}

// ---------------------------------------------------------------- helpers --

pub const PAL: [&str; 10] = [
    "#7c5cff", "#ff3b5c", "#22c55e", "#facc15", "#38bdf8", "#f97316", "#e879f9", "#94a3b8",
    "#14b8a6", "#fb7185",
];
pub const OTHERS: &str = "#64748b";

pub fn clamp(v: f32, a: f32, b: f32) -> f32 {
    v.max(a).min(b)
}
pub fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}
pub fn ease(k: f32) -> f32 {
    if k < 0.5 {
        4.0 * k * k * k
    } else {
        1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
    }
}
pub fn ease_out(k: f32) -> f32 {
    1.0 - (1.0 - k).powi(3)
}

pub fn curve(c: Curve, k: f32) -> f32 {
    match c {
        Curve::Linear => k,
        Curve::Out => ease_out(k),
        Curve::Ease => ease(k),
    }
}

/// The entrance effect actually used.
pub fn fx(c: &Config) -> Fx {
    match c.anim.fx {
        Fx::Auto => match c.chart_type {
            ChartType::Pie => Fx::Sweep,
            ChartType::Columns => Fx::Grow,
            _ => Fx::Draw,
        },
        f => f,
    }
}

/// Reveal mode in force (a pie shows one series: series → all).
pub fn reveal_mode(c: &Config, st: &State) -> RevealMode {
    let m = st.mode.unwrap_or(c.reveal);
    if c.chart_type == ChartType::Pie && m == RevealMode::Series {
        RevealMode::All
    } else {
        m
    }
}

/// Number of reveal steps (0 = all at once).
pub fn steps(c: &Config, st: &State, p: &Parsed) -> usize {
    match reveal_mode(c, st) {
        RevealMode::Series => p.ser.len(),
        RevealMode::Categories => p.cats.len(),
        RevealMode::All => 0,
    }
}

/// Style of series `j` with the defaults.
pub fn series_style(c: &Config, j: usize) -> (String, f32, Marker) {
    let s = c.series.get(j).cloned().unwrap_or_default();
    (
        s.color.unwrap_or_else(|| PAL[j % PAL.len()].to_string()),
        s.width.unwrap_or(6.0),
        s.marker.unwrap_or(Marker::Circle),
    )
}

pub fn slice_color(c: &Config, i: usize) -> String {
    c.slice_colors
        .get(i)
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| PAL[i % PAL.len()].to_string())
}

/// Update tween factor (1 = done / never reloaded).
pub fn upd_k(c: &Config, st: &State, now: i64) -> f32 {
    let dur = c.anim.upd * 1000.0;
    if st.at == 0 || dur <= 0.0 {
        return 1.0;
    }
    curve(c.anim.curve, clamp((now - st.at) as f32 / dur, 0.0, 1.0))
}

/// Highlight tween factor.
pub fn hl_k(c: &Config, st: &State, now: i64) -> f32 {
    let dur = c.anim.upd * 1000.0;
    if st.hl_at == 0 || dur <= 0.0 {
        return 1.0;
    }
    curve(c.anim.curve, clamp((now - st.hl_at) as f32 / dur, 0.0, 1.0))
}

/// The value shown for (category, series), tweened from the last Reload.
pub fn tween_val(st: &State, k: f32, cat: &str, ser: &str, target: Option<f32>) -> Option<f32> {
    let t = target?;
    if k >= 1.0 {
        return Some(t);
    }
    let j = st.prev_series.iter().position(|s| s == ser);
    let from = st
        .prev
        .iter()
        .find(|p| p.name == cat)
        .and_then(|p| j.and_then(|j| p.vals.get(j).copied().flatten()));
    Some(match from {
        Some(f) => lerp(f, t, k),
        None => t,
    })
}

/// True when the category arrived with the last Reload (it animates in).
pub fn is_new_cat(st: &State, name: &str) -> bool {
    st.at > 0 && !st.prev.is_empty() && !st.prev.iter().any(|p| p.name == name)
}

/// Value axis: (min, max, step).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

pub fn nice(lo: f32, hi: f32, n: u32) -> Scale {
    let (lo, hi) = if hi > lo { (lo, hi) } else { (lo, lo + 1.0) };
    let raw = (hi - lo) / (n.max(1) as f32);
    let mut step = 10f32.powf(raw.log10().floor());
    let e = raw / step;
    step *= if e >= 7.5 {
        10.0
    } else if e >= 3.5 {
        5.0
    } else if e >= 1.5 {
        2.0
    } else {
        1.0
    };
    Scale {
        min: (lo / step + 1e-6).floor() * step,
        max: (hi / step - 1e-6).ceil() * step,
        step,
    }
}

pub fn is_stacked(c: &Config) -> bool {
    (c.chart_type == ChartType::Columns && c.col_mode != ColMode::Grouped)
        || (c.chart_type == ChartType::Areas && c.area_mode == AreaMode::Stacked)
}
pub fn is_pct(c: &Config) -> bool {
    c.chart_type == ChartType::Columns && c.col_mode == ColMode::Percent
}

/// Values per category in the space drawn (100 %: shares).
pub fn targets(c: &Config, vals: &[Vec<Option<f32>>]) -> Vec<Vec<Option<f32>>> {
    let pct = is_pct(c);
    vals.iter()
        .map(|vs| {
            let tot: f32 = vs.iter().flatten().filter(|v| **v > 0.0).sum();
            vs.iter()
                .map(|v| {
                    let v = (*v)?;
                    if !pct {
                        return Some(v);
                    }
                    Some(if tot > 0.0 {
                        v.max(0.0) / tot * 100.0
                    } else {
                        0.0
                    })
                })
                .collect()
        })
        .collect()
}

/// Axis of the target values (x/y charts).
pub fn scale(c: &Config, tv: &[Vec<Option<f32>>]) -> Scale {
    let stacked = is_stacked(c);
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for vs in tv {
        if stacked {
            let (mut p, mut n) = (0.0f32, 0.0f32);
            for v in vs.iter().flatten() {
                if *v > 0.0 {
                    p += v;
                } else if *v < 0.0 {
                    n += v;
                }
            }
            hi = hi.max(p);
            lo = lo.min(n);
        } else {
            for v in vs.iter().flatten() {
                hi = hi.max(*v);
                lo = lo.min(*v);
            }
        }
    }
    if !lo.is_finite() {
        lo = 0.0;
        hi = 1.0;
    }
    if c.chart_type != ChartType::Lines || c.axis.zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    if is_pct(c) {
        Scale {
            min: 0.0,
            max: 100.0,
            step: nice(0.0, 100.0, c.axis.ticks).step,
        }
    } else if c.axis.auto {
        nice(lo, hi + (hi - lo) * 0.04, c.axis.ticks)
    } else {
        let mn = c.axis.min.min(c.axis.max - 1e-6);
        let mx = c.axis.max.max(mn + 1e-6);
        Scale {
            min: mn,
            max: mx,
            step: nice(mn, mx, c.axis.ticks).step,
        }
    }
}

/// Number like JavaScript prints it (no trailing zeros).
pub fn js_num(v: f32) -> String {
    let r = (v as f64 * 1e6).round() / 1e6;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        let s = format!("{r:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// `v` with `d` decimals, halves away from zero (same as the web).
pub fn fixed(v: f32, d: u32) -> String {
    let p = 10f64.powi(d as i32);
    let r = ((v as f64).abs() * p).round() / p;
    let s = format!("{:.*}", d as usize, r);
    if v < 0.0 && r != 0.0 {
        format!("-{s}")
    } else {
        s
    }
}

/// Formatted value of the axis (prefix, thousands, decimals, suffix).
pub fn fmt(c: &Config, v: f32) -> String {
    let a = &c.axis;
    let mut t = fixed(v.abs(), a.decimals.min(4));
    if a.sep != Sep::None {
        let (i, d) = match t.split_once('.') {
            Some((i, d)) => (i.to_string(), Some(d.to_string())),
            None => (t.clone(), None),
        };
        let sep = if a.sep == Sep::Space { ' ' } else { ',' };
        let n = i.len();
        let mut g = String::new();
        for (k, ch) in i.chars().enumerate() {
            if k > 0 && (n - k) % 3 == 0 {
                g.push(sep);
            }
            g.push(ch);
        }
        t = match d {
            Some(d) => format!("{g}.{d}"),
            None => g,
        };
    }
    let neg = v < 0.0
        && fixed(v.abs(), a.decimals.min(4))
            .chars()
            .any(|c| c != '0' && c != '.');
    format!(
        "{}{}{}{}",
        if neg { "−" } else { "" },
        a.prefix,
        t,
        a.suffix
    )
}

/// Label of a value: name · value · %.
pub fn label_text(
    c: &Config,
    name_on: bool,
    v: Option<f32>,
    pct: Option<f32>,
    name: &str,
) -> String {
    let l = &c.labels;
    let mut parts = Vec::new();
    if name_on && !name.is_empty() {
        parts.push(name.to_string());
    }
    if l.value {
        if let Some(v) = v {
            parts.push(fmt(c, v));
        }
    }
    if l.pct {
        if let Some(p) = pct {
            let d = if p < 10.0 && c.axis.decimals > 0 {
                1
            } else {
                0
            };
            parts.push(format!("{} %", fixed(p, d)));
        }
    }
    parts.join(" · ")
}

/// Approximate width of a text (layout decisions, same in both renderers).
pub fn tw2(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * 0.58
}

/// Text colour readable on `hex`.
pub fn ink(hex: &str) -> &'static str {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return "#ffffff";
    }
    let Ok(n) = u32::from_str_radix(&h[..6], 16) else {
        return "#ffffff";
    };
    let l = 0.299 * ((n >> 16) & 255) as f32
        + 0.587 * ((n >> 8) & 255) as f32
        + 0.114 * (n & 255) as f32;
    if l > 165.0 {
        "#14102a"
    } else {
        "#ffffff"
    }
}

// ---------------------------------------------------------------- actions --

fn config(layer: &Layer) -> Result<&Config, String> {
    match &layer.kind {
        LayerKind::Chart { x } => Ok(x),
        _ => Err("not a chart".into()),
    }
}

/// The grid shown: the last Reload's, else the live data.
pub fn shown_grid(c: &Config, st: &State, data: &Data) -> Vec<Vec<String>> {
    st.cur.clone().unwrap_or_else(|| resolve_grid(c, data))
}

/// Snapshot of what is on screen at `now` (start of the next tween).
fn snapshot(
    c: &Config,
    st: &State,
    data: &Data,
    now: i64,
) -> (Vec<PrevCat>, Vec<String>, Option<[f32; 2]>, f32) {
    let p = parse(c, &shown_grid(c, st, data));
    let k = upd_k(c, st, now);
    let n = p.cats.len() as f32;
    let prev: Vec<PrevCat> = p
        .cats
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let from_slot = st
                .prev
                .iter()
                .find(|q| q.name == cat.name)
                .map(|q| q.slot)
                .unwrap_or(i as f32);
            PrevCat {
                name: cat.name.clone(),
                slot: lerp(from_slot, i as f32, k),
                vals: p
                    .ser
                    .iter()
                    .enumerate()
                    .map(|(j, s)| tween_val(st, k, &cat.name, s, cat.vals[j]))
                    .collect(),
            }
        })
        .collect();
    let axis = if c.chart_type == ChartType::Pie || p.cats.is_empty() {
        None
    } else {
        let vals: Vec<Vec<Option<f32>>> = p.cats.iter().map(|c| c.vals.clone()).collect();
        let sc = scale(c, &targets(c, &vals));
        let from = st.prev_axis.unwrap_or([sc.min, sc.max]);
        Some([lerp(from[0], sc.min, k), lerp(from[1], sc.max, k)])
    };
    let from_n = if st.prev.is_empty() { n } else { st.prev_n };
    (prev, p.ser.clone(), axis, lerp(from_n, n, k))
}

fn now_hl(c: &Config, st: &mut State, hl: Hl, now: i64) {
    let _ = c;
    st.prev_hl = st.hl;
    st.hl = hl;
    st.hl_at = now;
}

/// Takes the data `data` (values tween from what is shown, or cut).
fn reload(c: &Config, st: &mut State, data: &Data, now: i64, cut: bool) {
    let (prev, ser, axis, n) = snapshot(c, st, data, now);
    st.cur = Some(resolve_grid(c, data));
    if cut {
        st.prev.clear();
        st.prev_series.clear();
        st.prev_axis = None;
        st.prev_n = 0.0;
        st.at = 0;
    } else {
        st.prev = prev;
        st.prev_series = ser;
        st.prev_axis = axis;
        st.prev_n = n;
        st.at = now;
    }
}

/// The graphic's data changed on air (`old` → `ctx.data`), by an Update, a
/// connector or an automation: the chart keeps what it showed (« Sur
/// Recharger »: until the operator's Reload) or tweens to the new values
/// (« Auto »). False when its own numbers did not change.
pub fn on_data(layer: &Layer, st: &mut State, old: &Data, ctx: &ActCtx) -> bool {
    let Ok(c) = config(layer) else {
        return false;
    };
    let new = resolve_grid(c, ctx.data);
    if st.cur.is_none() {
        let before = resolve_grid(c, old);
        if before == new {
            return false;
        }
        // Frozen as taken (what was on screen).
        reload(c, st, old, ctx.now_ms, true);
    } else if st.cur.as_ref() == Some(&new) {
        return false;
    }
    if c.on_air == OnAir::Auto {
        // The tween starts from what is shown (`cur`).
        reload(c, st, ctx.data, ctx.now_ms, false);
    }
    true
}

// ---------------------------------------------------------- chained steps --

/// The highlight written in the template (`hl_default`): `s2` = series 3,
/// `c0` = the first category; anything else = none.
pub fn hl_of(s: &str) -> Hl {
    let s = s.trim();
    let (kind, idx) = if let Some(n) = s.strip_prefix('s') {
        (HlKind::Series, n.parse::<i32>().ok())
    } else if let Some(n) = s.strip_prefix('c') {
        (HlKind::Category, n.parse::<i32>().ok())
    } else {
        (HlKind::None, None)
    };
    match idx {
        Some(idx) if idx >= 0 => Hl { kind, idx },
        _ => Hl::default(),
    }
}

/// The state on air: the operator's once he acted, else the Designer's
/// chained steps (counted from the take) and highlight.
pub fn live(c: &Config, st: &State, now_ms: i64, age_secs: f64) -> State {
    if st.ctl {
        return st.clone();
    }
    let mut s = st.clone();
    if s.hl.kind == HlKind::None && s.hl_at == 0 {
        s.hl = hl_of(&c.hl_default);
        s.prev_hl = s.hl;
    }
    if c.auto_steps && !s.auto {
        s.on = c.auto_on;
        s.trigger = c.auto_trigger.trim().to_string();
        if c.auto_on == AutoOn::Timed {
            if c.auto_secs > 0.0 {
                s.auto = true;
                s.interval = c.auto_secs.clamp(0.5, 3600.0);
                s.auto_at = (now_ms - (age_secs.max(0.0) * 1000.0) as i64).max(1);
            }
        } else {
            s.auto = true;
            s.interval = 0.0;
        }
    }
    s
}

/// The step shown at `now` (1 to `n`): the step set, plus the intervals
/// elapsed when the steps are timed (they stop at the last one).
pub fn shown_step(st: &State, n: usize, now_ms: i64) -> usize {
    let base = (st.step.max(1) as usize).min(n.max(1));
    if st.auto && st.interval > 0.0 && st.auto_at > 0 && base < n {
        let t = if st.paused_at > 0 {
            st.paused_at
        } else {
            now_ms
        };
        let step = ((st.interval * 1000.0) as i64).max(1);
        let k = ((t - st.auto_at).max(0) / step) as usize;
        return (base + k).min(n);
    }
    base
}

/// When the timed step `t` (0-based, after the step set) was revealed.
fn auto_step_at(st: &State, t: usize) -> i64 {
    let base = st.step.max(1) as usize;
    st.auto_at + (t + 1 - base) as i64 * ((st.interval * 1000.0) as i64).max(1)
}

/// The state as drawn at `now`: the take's chain and highlight applied, the
/// timed steps revealed so far (with their times). What the renderers lay out.
pub fn shown(c: &Config, st: &State, now_ms: i64, age_secs: f64, data: &Data) -> State {
    let mut s = live(c, st, now_ms, age_secs);
    let n = steps(c, &s, &parse(c, &shown_grid(c, &s, data)));
    if n > 0 {
        settle(&mut s, n, now_ms);
    }
    s
}

/// Makes the step shown at `now` the step set (its timed reveals kept).
fn settle(st: &mut State, n: usize, now_ms: i64) {
    let base = st.step.max(1) as usize;
    let sh = shown_step(st, n, now_ms);
    if sh > base {
        if st.step_at.len() < n {
            st.step_at.resize(n, 0);
        }
        for t in base..sh {
            st.step_at[t] = auto_step_at(st, t);
        }
        // The count goes on from the last reveal (its phase kept).
        st.auto_at = auto_step_at(st, sh - 1);
    }
    st.step = sh as u32;
}

/// Seconds left before the next timed step (None = not running, or at the last step).
pub fn next_in(st: &State, n: usize, now_ms: i64) -> Option<f64> {
    if !st.auto || st.interval <= 0.0 || st.auto_at <= 0 || shown_step(st, n, now_ms) >= n {
        return None;
    }
    let t = if st.paused_at > 0 {
        st.paused_at
    } else {
        now_ms
    };
    let step = st.interval * 1000.0;
    let e = (t - st.auto_at).max(0) as f64 % step;
    Some((step - e) / 1000.0)
}

/// The steps wait for Continue (F2): chained on it, not at the last step.
pub fn waits_continue(st: &State, n: usize, now_ms: i64) -> bool {
    st.auto && st.on == AutoOn::Continue && n > 0 && shown_step(st, n, now_ms) < n
}

/// Continue (F2): the next step when the steps wait for it.
pub fn on_continue(layer: &Layer, st: &mut State, ctx: &ActCtx) -> bool {
    let Ok(c) = config(layer) else {
        return false;
    };
    let live_st = live(c, st, ctx.now_ms, ctx.age_secs);
    let n = steps(c, &live_st, &parse(c, &shown_grid(c, &live_st, ctx.data)));
    if !waits_continue(&live_st, n, ctx.now_ms) {
        return false;
    }
    act(layer, st, "next", &serde_json::Value::Null, ctx).is_ok()
}

/// The trigger `name`: the next step when the steps wait for it (the last
/// step goes back to the first).
pub fn on_trigger(layer: &Layer, st: &mut State, name: &str, ctx: &ActCtx) -> bool {
    let Ok(c) = config(layer) else {
        return false;
    };
    let live_st = live(c, st, ctx.now_ms, ctx.age_secs);
    let n = steps(c, &live_st, &parse(c, &shown_grid(c, &live_st, ctx.data)));
    if !(n > 0
        && live_st.auto
        && live_st.on == AutoOn::Trigger
        && super::same_trigger(&live_st.trigger, name))
    {
        return false;
    }
    let op = if shown_step(&live_st, n, ctx.now_ms) >= n {
        "first"
    } else {
        "next"
    };
    act(layer, st, op, &serde_json::Value::Null, ctx).is_ok()
}

/// Operator actions:
/// - `reload` {cut?}: takes the current data; values tween from what is shown.
/// - `next` / `prev`: reveals the next step / hides the last one; `first`: back to the first.
/// - `showAll`: reveals every remaining step (staggered).
/// - `auto` {seconds}: the steps go on by themselves (chained, timed);
///   `advance` {on: timed | continue | trigger, seconds?, trigger?} ·
///   `pause` · `resume` · `stop` (not chained, back to the first step).
/// - `revealMode` {mode: all|series|categories}: starts over at step 1.
/// - `highlight` {kind: none|series|category|slice, idx}: same again = off.
/// - `reset`: back to the live data, nothing highlighted.
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let c = config(layer)?;
    let now = ctx.now_ms;
    // From now on the operator's state: what was on air so far, kept.
    *st = live(c, st, now, ctx.age_secs);
    st.ctl = true;
    let n = steps(c, st, &parse(c, &shown_grid(c, st, ctx.data)));
    if n > 0 {
        settle(st, n, now);
    }
    // A timed chain counts again from now when the operator moves the step.
    let restart = |st: &mut State| {
        if st.auto && st.interval > 0.0 {
            st.auto_at = now;
            if st.paused_at > 0 {
                st.paused_at = now;
            }
        }
    };
    match op {
        "reload" => {
            let cut = args.get("cut").and_then(|v| v.as_bool()).unwrap_or(false);
            reload(c, st, ctx.data, now, cut);
            Ok(())
        }
        "next" | "prev" | "showAll" | "first" => {
            if n == 0 {
                return Err("reveal is set to \"All at once\"".into());
            }
            let cur = (st.step.max(1) as usize).min(n);
            let next = match op {
                "next" => (cur + 1).min(n),
                "prev" => cur.saturating_sub(1).max(1),
                "first" => 1,
                _ => n,
            };
            if st.step_at.len() < n {
                st.step_at.resize(n, 0);
            }
            for (k, t) in (cur..next).enumerate() {
                st.step_at[t] = now + (k as f32 * c.anim.stagger) as i64;
            }
            st.step = next as u32;
            restart(st);
            Ok(())
        }
        "auto" => {
            let secs = args
                .get("seconds")
                .and_then(|v| v.as_f64())
                .unwrap_or(if st.interval > 0.0 { st.interval } else { 5.0 });
            if secs <= 0.0 {
                st.auto = false;
                st.paused_at = 0;
                return Ok(());
            }
            if !(0.5..=3600.0).contains(&secs) {
                return Err("interval: 0.5 to 3600 s".into());
            }
            if st.auto && st.auto_at > 0 {
                // New interval: keep the step, count from now.
                st.auto_at = if st.paused_at > 0 { st.paused_at } else { now };
            } else {
                st.auto_at = now;
                st.paused_at = 0;
            }
            st.auto = true;
            st.on = AutoOn::Timed;
            st.interval = secs;
            Ok(())
        }
        "advance" => {
            let on = match args.get("on").and_then(|v| v.as_str()).unwrap_or("timed") {
                "continue" => AutoOn::Continue,
                "trigger" => AutoOn::Trigger,
                "timed" => AutoOn::Timed,
                other => return Err(format!("unknown advance mode “{other}”")),
            };
            if let Some(name) = args.get("trigger").and_then(|v| v.as_str()) {
                st.trigger = name.trim().to_string();
            }
            if on == AutoOn::Timed {
                let secs =
                    args.get("seconds")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(if st.interval > 0.0 {
                            st.interval
                        } else {
                            c.auto_secs
                        });
                return act(
                    layer,
                    st,
                    "auto",
                    &serde_json::json!({ "seconds": secs }),
                    ctx,
                );
            }
            st.auto = true;
            st.on = on;
            st.interval = 0.0;
            st.auto_at = 0;
            st.paused_at = 0;
            Ok(())
        }
        "stop" => {
            st.auto = false;
            st.paused_at = 0;
            st.step = 1;
            st.step_at.clear();
            Ok(())
        }
        "pause" => {
            if st.auto && st.paused_at == 0 {
                st.paused_at = now;
            }
            Ok(())
        }
        "resume" => {
            if st.auto && st.paused_at > 0 {
                st.auto_at += now - st.paused_at;
                st.paused_at = 0;
            }
            Ok(())
        }
        "revealMode" => {
            let m: RevealMode =
                serde_json::from_value(args.get("mode").cloned().unwrap_or_default())
                    .map_err(|_| "mode: all, series or categories".to_string())?;
            st.mode = Some(m);
            st.step = 1;
            st.step_at.clear();
            restart(st);
            Ok(())
        }
        "highlight" => {
            let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("none");
            let idx = args.get("idx").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let kind = match kind {
                "series" => HlKind::Series,
                "category" | "column" | "point" | "slice" => HlKind::Category,
                "none" => HlKind::None,
                k => return Err(format!("unknown highlight `{k}`")),
            };
            let mut hl = Hl { kind, idx };
            if kind == HlKind::None || (st.hl.kind == kind && st.hl.idx == idx) {
                hl = Hl::default();
            }
            now_hl(c, st, hl, now);
            Ok(())
        }
        "reset" => {
            *st = State::default();
            Ok(())
        }
        _ => Err(format!("unknown action `{op}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    pub(super) fn layer(c: Config) -> Layer {
        Layer {
            id: "c".into(),
            name: "Chart".into(),
            parent: None,
            kind: LayerKind::Chart { x: c },
            visible: true,
            locked: false,
            manual_start: false,
            start_on: None,
            start_trigger: String::new(),
            scale_lock: false,
            lock_ratio: false,
            drop_shadow: false,
            timeline: None,
            props: BTreeMap::new(),
            tracks: BTreeMap::new(),
            bindings: BTreeMap::new(),
            setting_bindings: BTreeMap::new(),
            overrides: BTreeMap::new(),
            live_changes: BTreeMap::new(),
            span: None,
            motion: None,
        }
    }

    #[test]
    fn loads_with_defaults() {
        let l: Layer =
            serde_json::from_str(r#"{"id":"a","name":"C","type":"chart","props":{}}"#).unwrap();
        let LayerKind::Chart { x } = l.kind else {
            panic!()
        };
        assert_eq!(x, Config::default());
    }

    #[test]
    fn numbers_and_missing() {
        assert_eq!(num(" 1 234,5 "), Some(1234.5));
        assert_eq!(num("n/a"), None);
        assert_eq!(num("-"), None);
        let mut c = Config::default();
        let mut g = sample();
        g[3][1] = String::new();
        let p = parse(&c, &g);
        assert_eq!(p.missing, 1);
        assert_eq!(p.cats[2].vals[0], None);
        c.missing = Missing::Interp;
        let p = parse(&c, &g);
        assert_eq!(p.cats[2].vals[0], Some(30.5));
        assert!(p.cats[2].interp[0]);
    }

    #[test]
    fn csv_from_data() {
        let c = Config {
            source: Source::Csv,
            csv: "{{poll}}".into(),
            ..Default::default()
        };
        let mut d = Data::new();
        d.insert("poll".into(), "Party;Votes\nYes;12\nNo;\"8\"".into());
        let g = resolve_grid(&c, &d);
        assert_eq!(
            g,
            vec![vec!["Party", "Votes"], vec!["Yes", "12"], vec!["No", "8"]]
        );
    }

    #[test]
    fn a_list_of_numbers_draws_a_sparkline() {
        let c = Config {
            source: Source::Csv,
            csv: "{{series}}".into(),
            spark: true,
            ..Default::default()
        };
        let mut d = Data::new();
        d.insert("series".into(), "[3, 0, 5, 8]".into());
        let g = resolve_grid(&c, &d);
        assert_eq!(g.len(), 5);
        assert_eq!(g[4], vec!["4", "8"]);
        let st = State::default();
        let inp = |c: &Config| {
            layout(&Input {
                cfg: c,
                st: &st,
                grid: &g,
                w: 80.0,
                h: 40.0,
                fs: 26.0,
                age_ms: 5000.0,
                now: 0,
                lang: "fr",
            })
            .prims
        };
        // A column per value above zero, the highest one the box's height; no text.
        let p = inp(&c);
        assert_eq!(p.len(), 3);
        assert!(p.iter().all(|x| matches!(x, Prim::Shape(_))));
        // A line: one stroke.
        let line = Config {
            chart_type: ChartType::Lines,
            ..c.clone()
        };
        assert_eq!(inp(&line).len(), 1);
    }

    #[test]
    fn format() {
        let mut c = Config::default();
        c.axis.suffix = " M€".into();
        assert_eq!(fmt(&c, -12345.0), "−12 345 M€");
        c.axis.decimals = 1;
        assert_eq!(fmt(&c, 0.25), "0.3 M€");
        let s = nice(-53.0, 56.0, 5);
        assert_eq!((s.min, s.max, s.step), (-60.0, 60.0, 20.0));
    }

    #[test]
    fn reload_tweens_from_what_is_shown() {
        let l = layer(Config::default());
        let mut st = State::default();
        let mut data = Data::new();
        let ctx = ActCtx {
            now_ms: 1_000_000,
            age_secs: 5.0,
            data: &data,
            fps: 50.0,
        };
        act(
            &l,
            &mut st,
            "reload",
            &serde_json::json!({"cut": true}),
            &ctx,
        )
        .unwrap();
        assert_eq!(st.at, 0);
        // New values arrive in the grid; reload: tween from 28.
        let LayerKind::Chart { x } = &l.kind else {
            panic!()
        };
        let mut c2 = x.clone();
        c2.data[1][1] = "{{v}}".into();
        let mut l2 = l.clone();
        l2.kind = LayerKind::Chart { x: c2.clone() };
        data.insert("v".into(), "40".into());
        let ctx = ActCtx {
            now_ms: 1_001_000,
            age_secs: 6.0,
            data: &data,
            fps: 50.0,
        };
        act(&l2, &mut st, "reload", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.prev[0].vals[0], Some(28.0));
        let k = upd_k(&c2, &st, 1_001_400);
        let v = tween_val(&st, k, "Jan", "Martin", Some(40.0)).unwrap();
        assert!(v > 28.0 && v < 40.0);
        assert_eq!(upd_k(&c2, &st, 1_002_000), 1.0);
    }

    #[test]
    fn reveal_steps_and_highlight() {
        let c = Config {
            reveal: RevealMode::Series,
            ..Default::default()
        };
        let l = layer(c);
        let mut st = State::default();
        let data = Data::new();
        let ctx = ActCtx {
            now_ms: 5000,
            age_secs: 1.0,
            data: &data,
            fps: 50.0,
        };
        act(&l, &mut st, "next", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.step, 2);
        act(&l, &mut st, "showAll", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.step, 3);
        act(&l, &mut st, "next", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.step, 3);
        act(&l, &mut st, "prev", &serde_json::Value::Null, &ctx).unwrap();
        assert_eq!(st.step, 2);
        act(
            &l,
            &mut st,
            "highlight",
            &serde_json::json!({"kind":"slice","idx":2}),
            &ctx,
        )
        .unwrap();
        assert_eq!(
            st.hl,
            Hl {
                kind: HlKind::Category,
                idx: 2
            }
        );
        act(
            &l,
            &mut st,
            "highlight",
            &serde_json::json!({"kind":"category","idx":2}),
            &ctx,
        )
        .unwrap();
        assert_eq!(st.hl.kind, HlKind::None);
        let l = layer(Config::default());
        assert!(
            act(&l, &mut st, "next", &serde_json::Value::Null, &ctx).is_err() || st.mode.is_some()
        );
    }
}

#[cfg(test)]
mod chained_steps {
    use super::*;

    #[test]
    fn steps_chained_from_the_take_on_continue_and_on_a_trigger() {
        let mut c = Config {
            reveal: RevealMode::Series,
            auto_steps: true,
            auto_secs: 5.0,
            hl_default: "s1".into(),
            ..Default::default()
        };
        let d = Data::new();
        let cx = ActCtx {
            now_ms: 100_000,
            age_secs: 12.0,
            data: &d,
            fps: 50.0,
        };
        // Timed from the take (on air 12 s ago, 5 s per step): the third of the three series
        // shows, the highlight is the template's.
        let st = live(&c, &State::default(), 100_000, 12.0);
        assert!(st.auto && st.on == AutoOn::Timed && st.interval == 5.0 && st.auto_at == 88_000);
        assert_eq!(shown_step(&st, 3, 100_000), 3);
        assert_eq!(shown_step(&st, 3, 92_000), 1);
        assert_eq!(shown_step(&st, 3, 93_500), 2);
        assert_eq!(next_in(&st, 3, 93_500), Some(4.5));
        assert_eq!(next_in(&st, 3, 100_000), None);
        assert_eq!(
            st.hl,
            Hl {
                kind: HlKind::Series,
                idx: 1
            }
        );
        let sh = shown(&c, &State::default(), 93_500, 5.5, &d);
        assert_eq!((sh.step, sh.step_at[1]), (2, 93_000));
        // Chained on Continue: F2 reveals the steps, until the last one.
        c.auto_on = AutoOn::Continue;
        let l = super::tests::layer(c.clone());
        let mut st = State::default();
        assert!(waits_continue(&live(&c, &st, 100_000, 12.0), 3, 100_000));
        assert!(on_continue(&l, &mut st, &cx));
        assert_eq!(st.step, 2);
        assert!(st.ctl && st.auto && st.on == AutoOn::Continue);
        assert!(on_continue(&l, &mut st, &cx));
        assert_eq!(st.step, 3);
        assert!(!on_continue(&l, &mut st, &cx));
        // A trigger: only its name, and it wraps to the first step.
        act(
            &l,
            &mut st,
            "advance",
            &serde_json::json!({"on": "trigger", "trigger": "scores"}),
            &cx,
        )
        .unwrap();
        assert!(!on_trigger(&l, &mut st, "other", &cx));
        assert!(on_trigger(&l, &mut st, " SCORES ", &cx));
        assert_eq!(st.step, 1);
        assert!(on_trigger(&l, &mut st, "scores", &cx));
        assert_eq!(st.step, 2);
        // Stop: not chained, back to the first step; timed again from Advance, paused.
        act(&l, &mut st, "stop", &serde_json::Value::Null, &cx).unwrap();
        assert!(!st.auto && st.step == 1);
        act(
            &l,
            &mut st,
            "advance",
            &serde_json::json!({"on": "timed", "seconds": 4}),
            &cx,
        )
        .unwrap();
        assert!(st.auto && st.on == AutoOn::Timed && st.interval == 4.0 && st.auto_at == 100_000);
        assert_eq!(shown_step(&st, 3, 108_500), 3);
        let cx2 = ActCtx {
            now_ms: 104_500,
            age_secs: 16.5,
            data: &d,
            fps: 50.0,
        };
        act(&l, &mut st, "pause", &serde_json::Value::Null, &cx2).unwrap();
        assert_eq!(shown_step(&st, 3, 130_000), 2);
        act(&l, &mut st, "resume", &serde_json::Value::Null, &cx2).unwrap();
        assert_eq!(shown_step(&st, 3, 104_600), 2);
        // Next by hand settles the shown step and counts again from now.
        act(&l, &mut st, "prev", &serde_json::Value::Null, &cx2).unwrap();
        assert_eq!((st.step, st.auto_at), (1, 104_500));
    }
}

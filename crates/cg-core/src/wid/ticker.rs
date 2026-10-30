//! Ticker: crawl (horizontal, continuous), roll (vertical, line by line) or
//! flip (one item at a time), separators text / symbol / picture,
//! categories with their own chip, a fixed title block, edge fades and the
//! end of the list (loop / N passes / stop). The operator's queue lives in
//! the state (seeded from the layer's items); its edits apply at the next
//! pass, without a jump.
//!
//! Motion comes from the clock: position = base + speed × (now − anchor),
//! never incremented. The "tape" is a list of runs: a run is a queue
//! snapshot that starts at the first pass (or item) boundary of the run
//! before it that is not yet on screen. Resolving it needs the widths of the
//! items (fonts), so the renderers resolve it with [`resolve`]; flip and roll
//! use slots (width 1) and the same code.
//! Drawn by `cg-render/src/wid/ticker.rs` and `web/src/widgets/ticker.tsx` (keep in sync).

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind, Prop};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Crawl,
    Roll,
    Flip,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dir {
    #[default]
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trans {
    #[default]
    Slide,
    Push,
    Fade,
    Flip,
    /// The next item is typed in, character by character, over the
    /// transition time (the previous one goes at once).
    Type,
}

/// What a typed-in item shows `wk` seconds into its slot: its first
/// characters and the cursor while it types, all of it once typed.
pub fn typed(text: &str, wk: f64, dur: f64) -> (String, bool) {
    let n = text.chars().count();
    if wk >= dur || n == 0 {
        return (text.to_string(), false);
    }
    let k = ((n as f64) * (wk / dur.max(0.01)).max(0.0)).ceil() as usize;
    (text.chars().take(k.min(n)).collect(), true)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SepType {
    /// The layer's `separator` text (old tickers).
    #[default]
    Text,
    Symbol,
    Image,
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum End {
    #[default]
    Loop,
    Passes,
    Stop,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmptyMode {
    #[default]
    Message,
    Hide,
}

/// An item of the list (its text may hold `{{key}}`; a linked value of
/// several lines or a JSON array gives several items).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Item {
    pub cat: String,
    pub text: String,
    pub on: bool,
    /// The layer's `item_format` (a list's elements as text).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub format: String,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            cat: String::new(),
            text: String::new(),
            on: true,
            format: String::new(),
        }
    }
}

/// A category: its chip (background, text) and the item's text colour ("" = the bar's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cat {
    pub name: String,
    pub bg: String,
    pub color: String,
    pub text: String,
}

impl Default for Cat {
    fn default() -> Self {
        Self {
            name: "NEW".into(),
            bg: "#7c5cff".into(),
            color: "#ffffff".into(),
            text: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TitleBlock {
    pub on: bool,
    pub text: String,
    pub bg: String,
    pub color: String,
    pub w: f32,
}

impl Default for TitleBlock {
    fn default() -> Self {
        Self {
            on: false,
            text: "INFO".into(),
            bg: "#ff3b5c".into(),
            color: "#ffffff".into(),
            w: 190.0,
        }
    }
}

/// Settings of the widget, flattened into the layer next to the old fields
/// (`text` lines, `separator`, `speed`, font): an old ticker (no `items`)
/// keeps its look — no bar, its separator, entering from the right. The
/// text colour and size are the animated `fill` and `fontSize`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub mode: Mode,
    /// Crawl: left (right to left) or right; roll: up or down.
    pub dir: Dir,
    pub roll_dir: Dir,
    /// Flip: seconds per item; roll: hold per line.
    pub item_dur: f64,
    pub trans_dur: f64,
    pub trans: Trans,
    /// The list (empty = the lines of the old `text`).
    pub items: Vec<Item>,
    pub sep_type: SepType,
    pub sep_symbol: String,
    /// Picture separator (asset path).
    pub sep_image: String,
    /// "" = the text colour.
    pub sep_color: String,
    pub cats: Vec<Cat>,
    pub title: TitleBlock,
    /// Bar background ("" = none).
    pub bar_bg: String,
    /// Between an item and its separator, px.
    pub spacing: f32,
    pub fade: bool,
    pub fade_w: f32,
    pub end: End,
    pub passes: u32,
    pub empty_mode: EmptyMode,
    pub empty_text: String,
    /// Crawl: the zone is full at the In (else the text enters from the edge).
    pub start_full: bool,
    /// The elements of a linked list (JSON objects, a data set's rows) as
    /// text: `{name} · {text}` takes their keys (empty = their text).
    pub item_format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Crawl,
            dir: Dir::Left,
            roll_dir: Dir::Up,
            item_dur: 5.0,
            trans_dur: 0.6,
            trans: Trans::Slide,
            items: Vec::new(),
            sep_type: SepType::Text,
            sep_symbol: "●".into(),
            sep_image: String::new(),
            sep_color: String::new(),
            cats: Vec::new(),
            title: TitleBlock::default(),
            bar_bg: String::new(),
            spacing: 0.0,
            fade: false,
            fade_w: 90.0,
            end: End::Loop,
            passes: 2,
            empty_mode: EmptyMode::Message,
            empty_text: String::new(),
            start_full: false,
            item_format: String::new(),
        }
    }
}

/// The layer's settings and old fields.
pub struct Ticker<'a> {
    pub c: &'a Config,
    pub text: &'a str,
    pub separator: &'a str,
    pub speed: f64,
}

pub fn of(layer: &Layer) -> Option<Ticker<'_>> {
    match &layer.kind {
        LayerKind::Ticker {
            x,
            text,
            separator,
            speed,
            ..
        } => Some(Ticker {
            c: x,
            text,
            separator,
            speed: *speed,
        }),
        _ => None,
    }
}

/// The list of the layer: its items, or the lines of the old text.
pub fn layer_items(t: &Ticker) -> Vec<Item> {
    let fmt = |mut i: Item| {
        i.format = t.c.item_format.clone();
        i
    };
    if !t.c.items.is_empty() {
        return t.c.items.iter().cloned().map(fmt).collect();
    }
    t.text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            fmt(Item {
                text: l.to_string(),
                ..Item::default()
            })
        })
        .collect()
}

/// `{key}` of a format replaced by an element's values.
pub fn formatted(format: &str, get: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = format;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        match rest[i + 1..].find('}') {
            Some(j) => {
                out.push_str(&get(rest[i + 1..i + 1 + j].trim()).unwrap_or_default());
                rest = &rest[i + 2 + j..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Items to show: enabled, interpolated, a linked value expanded into its
/// rows (lines, or a JSON array of strings / {text, cat}).
pub fn expand(items: &[Item], data: &Data) -> Vec<Item> {
    let mut out = Vec::new();
    for it in items.iter().filter(|i| i.on) {
        let t = interpolate_text(&it.text, data);
        let tt = t.trim();
        if tt.starts_with('[') {
            if let Ok(serde_json::Value::Array(a)) = serde_json::from_str::<serde_json::Value>(tt) {
                // A data set's table: its text (or first) column, its category column.
                if let Some((head, rows)) = super::table::header_rows(&a) {
                    use super::table::find_col;
                    let text = find_col(&head, &["text", "title", "headline"]).unwrap_or(0);
                    let cat = find_col(&head, &["cat", "category"]);
                    for r in &rows {
                        let t = if it.format.is_empty() {
                            r.get(text).map(|s| s.trim()).unwrap_or("").to_string()
                        } else {
                            formatted(&it.format, &|k| {
                                find_col(&head, &[k]).and_then(|i| r.get(i)).cloned()
                            })
                        };
                        if !t.is_empty() {
                            out.push(Item {
                                cat: cat
                                    .and_then(|i| r.get(i))
                                    .cloned()
                                    .unwrap_or_else(|| it.cat.clone()),
                                text: t,
                                ..Item::default()
                            });
                        }
                    }
                    continue;
                }
                for v in a {
                    let (text, cat) = match &v {
                        serde_json::Value::String(s) => (s.clone(), it.cat.clone()),
                        serde_json::Value::Object(o) if !it.format.is_empty() => (
                            formatted(&it.format, &|k| o.get(k).map(crate::datapath::text)),
                            o.get("cat")
                                .or_else(|| o.get("category"))
                                .and_then(|x| x.as_str())
                                .map(String::from)
                                .unwrap_or_else(|| it.cat.clone()),
                        ),
                        serde_json::Value::Object(o) => (
                            o.get("text")
                                .or_else(|| o.get("title"))
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string(),
                            o.get("cat")
                                .or_else(|| o.get("category"))
                                .and_then(|x| x.as_str())
                                .map(String::from)
                                .unwrap_or_else(|| it.cat.clone()),
                        ),
                        other => (other.to_string(), it.cat.clone()),
                    };
                    if !text.trim().is_empty() {
                        out.push(Item {
                            cat,
                            text: text.trim().to_string(),
                            ..Item::default()
                        });
                    }
                }
                continue;
            }
        }
        for line in t.lines().map(str::trim).filter(|l| !l.is_empty()) {
            out.push(Item {
                cat: it.cat.clone(),
                text: line.to_string(),
                ..Item::default()
            });
        }
    }
    out
}

// ----------------------------------------------------------------- state --

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunKind {
    /// Starts at the next pass boundary not yet on screen.
    #[default]
    Pass,
    /// Starts at the next item boundary: the priority item, then the rest of the pass.
    Item,
    /// Starts at the next item boundary, without the rest of the pass (clear).
    Cut,
}

/// A queue snapshot on the tape.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Run {
    /// Clock position when it was made (px for a crawl, slot for flip / roll).
    pub at: f64,
    pub items: Vec<Item>,
    pub prio: Option<Item>,
    pub kind: RunKind,
}

/// What the operator changes on air.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// false = runs from the time on air (as at the In).
    pub started: bool,
    pub running: bool,
    /// Clock at `anchor` (px for a crawl, seconds for flip / roll).
    pub base: f64,
    pub anchor: i64,
    /// Speed set live, px/s.
    pub speed: Option<f64>,
    /// The queue (None = the layer's items).
    pub queue: Option<Vec<Item>>,
    /// The tape (empty = one run of the queue from the start).
    pub runs: Vec<Run>,
}

pub const MAX_RUNS: usize = 24;
/// Padding of the text in a flip / roll slot, px.
pub const PAD: f32 = 30.0;

/// Readable speeds, px/s: the same range in the Designer and on air.
pub const SPEED_MIN: f64 = 10.0;
pub const SPEED_MAX: f64 = 800.0;

pub fn speed(t: &Ticker, st: &State) -> f64 {
    st.speed.unwrap_or(t.speed).clamp(SPEED_MIN, SPEED_MAX)
}

/// Period of a slot (flip / roll), seconds.
pub fn period(t: &Ticker, st: &State, h: f32) -> f64 {
    match t.c.mode {
        Mode::Flip => t.c.item_dur.max(0.5),
        _ => t.c.item_dur.max(0.0) + h as f64 / speed(t, st).max(5.0),
    }
}

/// The clock: px travelled (crawl) or seconds (flip / roll). A crawl moves a
/// whole number of pixels per frame at `fps` ([`crate::widgets::frame_speed`]).
pub fn clock(t: &Ticker, st: &State, now_ms: i64, age_secs: f64, fps: f64) -> f64 {
    let rate = if t.c.mode == Mode::Crawl {
        crate::widgets::frame_speed(speed(t, st), fps)
    } else {
        1.0
    };
    if !st.started {
        return rate * age_secs.max(0.0);
    }
    st.base
        + if st.running {
            rate * (now_ms - st.anchor).max(0) as f64 / 1000.0
        } else {
            0.0
        }
}

/// The queue: the operator's, or the layer's items.
pub fn queue(t: &Ticker, st: &State) -> Vec<Item> {
    st.queue.clone().unwrap_or_else(|| layer_items(t))
}

/// The runs of the tape (one of the queue when none was made).
pub fn runs(t: &Ticker, st: &State) -> Vec<Run> {
    if st.runs.is_empty() {
        vec![Run {
            at: 0.0,
            items: queue(t, st),
            prio: None,
            kind: RunKind::Pass,
        }]
    } else {
        st.runs.clone()
    }
}

pub fn limit(c: &Config) -> Option<u32> {
    match c.end {
        End::Loop => None,
        End::Stop => Some(1),
        End::Passes => Some(c.passes.max(1)),
    }
}

// ------------------------------------------------------------------ tape --

/// A run placed on the tape.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub start: f64,
    /// Played once first: (item, width).
    pub lead: Vec<(Item, f64)>,
    /// Then looped.
    pub items: Vec<(Item, f64)>,
    pub lead_w: f64,
    pub cycle: f64,
    /// Passes completed before this run.
    pub passes_before: u32,
    /// The lead ends a pass.
    pub lead_pass: bool,
    /// Where it stops (next run, or the end of the passes).
    pub end: f64,
}

/// A piece of the tape: an item at `x` (its separator follows it in `w`).
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub item: Item,
    pub x: f64,
    pub w: f64,
}

fn place(items: &[Item], data: &Data, wf: &mut dyn FnMut(&Item) -> f64) -> Vec<(Item, f64)> {
    expand(items, data)
        .into_iter()
        .map(|i| {
            let w = wf(&i).max(1e-3);
            (i, w)
        })
        .collect()
}

/// Places the runs on the tape. `z`: length of the zone (a run starts out
/// of it); `first`: where the first run starts.
pub fn resolve(
    runs: &[Run],
    data: &Data,
    wf: &mut dyn FnMut(&Item) -> f64,
    z: f64,
    first: f64,
    limit: Option<u32>,
) -> Vec<Placed> {
    let mut out: Vec<Placed> = Vec::new();
    for run in runs {
        let items = place(&run.items, data, wf);
        let cycle: f64 = items.iter().map(|x| x.1).sum();
        let (start, mut lead, passes_before, lead_pass) = match out.last() {
            None => {
                let lead = match &run.prio {
                    Some(p) => place(std::slice::from_ref(p), data, wf),
                    None => Vec::new(),
                };
                (first, lead, 0, false)
            }
            Some(r) => {
                let t = run.at + z;
                let lp = u32::from(r.lead_pass);
                let (b, rest, done): (f64, Vec<(Item, f64)>, u32) = if t <= r.start {
                    let rest = if run.kind == RunKind::Item {
                        r.lead.clone()
                    } else {
                        Vec::new()
                    };
                    (r.start, rest, 0)
                } else if t <= r.start + r.lead_w || r.cycle <= 0.0 {
                    if run.kind == RunKind::Pass || r.cycle <= 0.0 && t > r.start + r.lead_w {
                        let b = if r.cycle <= 0.0 {
                            t.max(r.start + r.lead_w)
                        } else {
                            r.start + r.lead_w
                        };
                        (b, Vec::new(), lp)
                    } else {
                        let mut x = r.start;
                        let mut idx = r.lead.len();
                        for (i, (_, w)) in r.lead.iter().enumerate() {
                            if x >= t - 1e-9 {
                                idx = i;
                                break;
                            }
                            x += w;
                        }
                        let rest = if run.kind == RunKind::Item {
                            r.lead[idx..].to_vec()
                        } else {
                            Vec::new()
                        };
                        (x, rest, if idx == r.lead.len() { lp } else { 0 })
                    }
                } else {
                    let u = t - r.start - r.lead_w;
                    let n = (u / r.cycle).floor();
                    let base = r.start + r.lead_w + n * r.cycle;
                    if run.kind == RunKind::Pass {
                        let n2 = if base >= t - 1e-9 { n } else { n + 1.0 };
                        (
                            r.start + r.lead_w + n2 * r.cycle,
                            Vec::new(),
                            lp + n2 as u32,
                        )
                    } else {
                        let mut x = base;
                        let mut idx = r.items.len();
                        for (i, (_, w)) in r.items.iter().enumerate() {
                            if x >= t - 1e-9 {
                                idx = i;
                                break;
                            }
                            x += w;
                        }
                        if idx == r.items.len() {
                            (x, Vec::new(), lp + n as u32 + 1)
                        } else {
                            let rest = if run.kind == RunKind::Item {
                                r.items[idx..].to_vec()
                            } else {
                                Vec::new()
                            };
                            (x, rest, lp + n as u32)
                        }
                    }
                };
                let mut lead = match &run.prio {
                    Some(p) => place(std::slice::from_ref(p), data, wf),
                    None => Vec::new(),
                };
                let lead_pass = !rest.is_empty();
                lead.extend(rest);
                if b > r.end {
                    // The tape had ended: nothing more until Start.
                    (f64::INFINITY, lead, r.passes_before + lp, lead_pass)
                } else {
                    (b, lead, r.passes_before + done, lead_pass)
                }
            }
        };
        if let Some(prev) = out.last_mut() {
            prev.end = prev.end.min(start);
        }
        if start.is_infinite() {
            break;
        }
        lead.retain(|x| x.1 > 0.0);
        let lead_w: f64 = lead.iter().map(|x| x.1).sum();
        let mut end = f64::INFINITY;
        if let Some(l) = limit {
            let done = passes_before + u32::from(lead_pass);
            let left = l.saturating_sub(done) as f64;
            if done >= l || cycle <= 0.0 {
                end = start + lead_w;
            } else {
                end = start + lead_w + left * cycle;
            }
        } else if cycle <= 0.0 {
            end = start + lead_w;
        }
        out.push(Placed {
            start,
            lead,
            items,
            lead_w,
            cycle,
            passes_before,
            lead_pass,
            end,
        });
    }
    out
}

/// The pieces of the tape overlapping [lo, hi).
pub fn segs(placed: &[Placed], lo: f64, hi: f64) -> Vec<Seg> {
    let mut out = Vec::new();
    for p in placed {
        if p.start >= hi || p.end <= lo {
            continue;
        }
        let stop = p.end.min(hi);
        let mut x = p.start;
        for (it, w) in &p.lead {
            if x >= stop {
                break;
            }
            if x + w > lo {
                out.push(Seg {
                    item: it.clone(),
                    x,
                    w: *w,
                });
            }
            x += w;
        }
        if p.cycle <= 0.0 || x >= stop {
            continue;
        }
        let skip = ((lo - x) / p.cycle).floor().max(0.0);
        x += skip * p.cycle;
        let mut guard = 0;
        'outer: while x < stop && guard < 10_000 {
            for (it, w) in &p.items {
                if x >= stop {
                    break 'outer;
                }
                if x + w > lo {
                    out.push(Seg {
                        item: it.clone(),
                        x,
                        w: *w,
                    });
                }
                x += w;
                guard += 1;
            }
        }
    }
    out
}

/// The end of the tape (None = it loops for ever).
pub fn tape_end(placed: &[Placed]) -> Option<f64> {
    placed.last().map(|p| p.end).filter(|e| e.is_finite())
}

/// Pass number (1-based) at position `x`, for the readout.
pub fn pass_at(placed: &[Placed], x: f64) -> u32 {
    for p in placed.iter().rev() {
        if x >= p.start {
            let after = x - p.start - p.lead_w;
            let n = if p.cycle > 0.0 && after > 0.0 {
                (after / p.cycle).floor() as u32
            } else {
                0
            };
            return p.passes_before + u32::from(p.lead_pass && after > 0.0) + n + 1;
        }
    }
    1
}

/// Where the tape is on air, as drawn: its pass (1-based) and whether it has
/// ended. A crawl measures its items with `m` (the renderer's fonts).
pub fn progress(
    layer: &Layer,
    st: &State,
    data: &Data,
    now_ms: i64,
    age_secs: f64,
    fps: f64,
    m: &mut dyn crate::playout::TextWidth,
) -> Option<(u32, bool)> {
    let LayerKind::Ticker {
        x: c,
        text,
        separator,
        speed,
        font,
    } = &layer.kind
    else {
        return None;
    };
    let t = Ticker {
        c,
        text,
        separator,
        speed: *speed,
    };
    let num = |p: Prop, d: f64| layer.props.get(&p).and_then(|v| v.as_number()).unwrap_or(d) as f32;
    let e = clock(&t, st, now_ms, age_secs, fps);
    if c.mode != Mode::Crawl {
        let p = period(&t, st, num(Prop::Height, 80.0));
        let placed = resolve(&runs(&t, st), data, &mut |_| 1.0, 0.0, 0.0, limit(c));
        let k = (e / p).floor();
        return Some((
            pass_at(&placed, k),
            tape_end(&placed).is_some_and(|end| k >= end),
        ));
    }
    let (w, size, ls) = (
        num(Prop::Width, 600.0),
        num(Prop::FontSize, 32.0).max(1.0),
        num(Prop::LetterSpacing, 0.0),
    );
    let zx = if c.title.on {
        c.title.w.min(w - 40.0).max(0.0)
    } else {
        0.0
    };
    let zw = (w - zx).max(40.0);
    let up = |s: &str| {
        if font.uppercase {
            s.to_uppercase()
        } else {
            s.to_string()
        }
    };
    let sep_w = match c.sep_type {
        SepType::None => 0.0,
        SepType::Image => (size * 1.05).round() * 2.4,
        SepType::Symbol => m.width("Inter", 900, &c.sep_symbol, (size * 0.7).round(), 0.0),
        SepType::Text => m.width(
            &font.font_family,
            font.font_weight,
            &up(separator),
            size,
            ls,
        ),
    };
    let gap = c.spacing + sep_w;
    let mut wf = |it: &Item| {
        let chip = c
            .cats
            .iter()
            .find(|k| !it.cat.is_empty() && k.name == it.cat)
            .map(|k| {
                let (cs, cp, cg) = (
                    (size * 0.6).round(),
                    (size * 0.36).round(),
                    (size * 0.45).round(),
                );
                m.width("Inter", 900, &k.name.to_uppercase(), cs, 0.0) + 2.0 * cp + cg
            })
            .unwrap_or(0.0);
        (m.width(&font.font_family, font.font_weight, &up(&it.text), size, ls) + chip + gap) as f64
    };
    let first = if c.start_full { 0.0 } else { zw as f64 };
    let placed = resolve(&runs(&t, st), data, &mut wf, zw as f64, first, limit(c));
    Some((
        pass_at(&placed, e),
        tape_end(&placed).is_some_and(|end| e >= end),
    ))
}

// --------------------------------------------------------------- actions --

fn height(layer: &Layer) -> f32 {
    layer
        .props
        .get(&Prop::Height)
        .and_then(|v| v.as_number())
        .unwrap_or(80.0) as f32
}

fn item_arg(args: &serde_json::Value, base: Option<&Item>) -> Item {
    let mut it = base.cloned().unwrap_or_default();
    if let Some(s) = args.get("text").and_then(|v| v.as_str()) {
        it.text = s.to_string();
    }
    if let Some(s) = args.get("cat").and_then(|v| v.as_str()) {
        it.cat = s.to_string();
    }
    if let Some(b) = args.get("on").and_then(|v| v.as_bool()) {
        it.on = b;
    }
    it
}

fn index(args: &serde_json::Value, key: &str, len: usize) -> Result<usize, String> {
    let i = args
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or(format!("`{key}` needed"))? as usize;
    if i >= len {
        return Err(format!("no item {i}"));
    }
    Ok(i)
}

/// Makes the state explicit (started from the layer's time on air: 0 while
/// a manual start waits for its Continue / trigger, then its clock).
fn materialize(t: &Ticker, layer: &Layer, st: &mut State, ctx: &ActCtx) {
    if st.started {
        return;
    }
    st.base = clock(t, st, ctx.now_ms, ctx.age_secs, ctx.fps);
    st.anchor = ctx.now_ms;
    st.started = true;
    // A manual start not started yet (no time on air) waits for its Play.
    st.running = !layer.manual_start || ctx.age_secs > 0.0;
    // The runs stay empty: the tape keeps following the layer's items (and
    // their edits in the Designer) until the operator edits the queue.
}

/// Position where an edit made now applies (crawl: px; flip / roll: next slot).
fn edit_at(t: &Ticker, st: &State, now: i64, h: f32, fps: f64) -> f64 {
    let c = clock(t, st, now, 0.0, fps);
    if t.c.mode == Mode::Crawl {
        c
    } else {
        (c / period(t, st, h)).floor() + 1.0
    }
}

fn push_run(st: &mut State, run: Run) {
    st.runs.push(run);
    if st.runs.len() > MAX_RUNS {
        let n = st.runs.len() - MAX_RUNS;
        st.runs.drain(0..n);
    }
}

/// Applies an operator action (`op`, with its `args`) to the state.
/// start · pause · resume · speed {px | delta} · next · reload · clear ·
/// add {text, cat, on, index?} · edit {index, text?, cat?, on?} · delete {index} ·
/// move {from, to} · toggle {index, on?} · set {items} ·
/// insert {text, cat, when: "now" | "after", keep?}.
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let t = of(layer).ok_or("not a ticker")?;
    let h = height(layer);
    let now = ctx.now_ms;
    match op {
        // The graphic's sources are read again (the server does it): the
        // items follow them, nothing to change here.
        "reload" => return Ok(()),
        "start" => {
            st.started = true;
            st.running = true;
            st.base = 0.0;
            st.anchor = now;
            // The operator's queue, else the layer's items, followed live.
            st.runs = match &st.queue {
                Some(q) => vec![Run {
                    at: 0.0,
                    items: q.clone(),
                    prio: None,
                    kind: RunKind::Pass,
                }],
                None => Vec::new(),
            };
            return Ok(());
        }
        "pause" => {
            materialize(&t, layer, st, ctx);
            if st.running {
                st.base = clock(&t, st, now, 0.0, ctx.fps);
                st.anchor = now;
                st.running = false;
            }
            return Ok(());
        }
        "resume" => {
            materialize(&t, layer, st, ctx);
            if !st.running {
                st.anchor = now;
                st.running = true;
            }
            return Ok(());
        }
        "speed" => {
            materialize(&t, layer, st, ctx);
            let cur = speed(&t, st);
            let v = if let Some(px) = args.get("px").and_then(|v| v.as_f64()) {
                px
            } else if let Some(d) = args.get("delta").and_then(|v| v.as_f64()) {
                cur + d
            } else {
                return Err("`px` or `delta` needed".into());
            };
            let v = v.clamp(SPEED_MIN, SPEED_MAX);
            let p0 = period(&t, st, h);
            let c = clock(&t, st, now, 0.0, ctx.fps);
            st.base = c;
            st.anchor = now;
            st.speed = Some(v);
            if t.c.mode == Mode::Roll {
                // Same slot, same fraction of it: no jump.
                let p1 = period(&t, st, h);
                let k = (c / p0).floor();
                st.base = (k + (c - k * p0) / p0) * p1;
            }
            return Ok(());
        }
        "next" => {
            if t.c.mode == Mode::Crawl {
                return Err("crawl: no next item".into());
            }
            materialize(&t, layer, st, ctx);
            let p = period(&t, st, h);
            let c = clock(&t, st, now, 0.0, ctx.fps);
            let k = (c / p).floor();
            let w = c - k * p;
            st.base = if t.c.mode == Mode::Roll && w < t.c.item_dur {
                k * p + t.c.item_dur
            } else {
                (k + 1.0) * p
            };
            st.anchor = now;
            return Ok(());
        }
        _ => {}
    }
    // Queue edits: they apply at the next pass.
    materialize(&t, layer, st, ctx);
    if st.runs.is_empty() {
        // The tape so far: what was on screen until now.
        st.runs = runs(&t, st);
    }
    let mut q = queue(&t, st);
    let at = edit_at(&t, st, now, h, ctx.fps);
    match op {
        "add" => {
            let it = item_arg(args, None);
            if it.text.trim().is_empty() {
                return Err("empty item".into());
            }
            match args.get("index").and_then(|v| v.as_u64()) {
                Some(i) => q.insert((i as usize).min(q.len()), it),
                None => q.push(it),
            }
        }
        "edit" => {
            let i = index(args, "index", q.len())?;
            q[i] = item_arg(args, Some(&q[i]));
        }
        "delete" => {
            let i = index(args, "index", q.len())?;
            q.remove(i);
        }
        "move" => {
            let i = index(args, "from", q.len())?;
            let j = args
                .get("to")
                .and_then(|v| v.as_u64())
                .ok_or("`to` needed")? as usize;
            let it = q.remove(i);
            q.insert(j.min(q.len()), it);
        }
        "toggle" => {
            let i = index(args, "index", q.len())?;
            q[i].on = args.get("on").and_then(|v| v.as_bool()).unwrap_or(!q[i].on);
        }
        "set" => {
            q = serde_json::from_value(args.get("items").cloned().unwrap_or_default())
                .map_err(|e| format!("items: {e}"))?;
        }
        "clear" => {
            q.clear();
            st.queue = Some(q);
            push_run(
                st,
                Run {
                    at,
                    items: Vec::new(),
                    prio: None,
                    kind: RunKind::Cut,
                },
            );
            return Ok(());
        }
        "insert" => {
            let it = item_arg(args, None);
            if it.text.trim().is_empty() {
                return Err("type the priority item first".into());
            }
            if args.get("keep").and_then(|v| v.as_bool()).unwrap_or(false) {
                q.insert(0, it.clone());
            }
            st.queue = Some(q.clone());
            push_run(
                st,
                Run {
                    at,
                    items: q,
                    prio: Some(it),
                    kind: RunKind::Item,
                },
            );
            let when_now = args.get("when").and_then(|v| v.as_str()) == Some("now");
            if when_now && t.c.mode != Mode::Crawl {
                // On air now: the next slot starts now.
                st.base = at * period(&t, st, h);
                st.anchor = now;
            }
            return Ok(());
        }
        _ => return Err(format!("unknown action `{op}`")),
    }
    st.queue = Some(q.clone());
    push_run(
        st,
        Run {
            at,
            items: q,
            prio: None,
            kind: RunKind::Pass,
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_is_typed_in_over_the_transition() {
        assert_eq!(typed("Élection", 0.0, 2.0), (String::new(), true));
        assert_eq!(typed("Élection", 0.5, 2.0), ("Él".into(), true));
        assert_eq!(typed("Élection", 1.99, 2.0), ("Élection".into(), true));
        assert_eq!(typed("Élection", 2.0, 2.0), ("Élection".into(), false));
        assert_eq!(typed("", 0.5, 2.0), (String::new(), false));
    }

    fn layer(json: &str) -> Layer {
        serde_json::from_str(json).unwrap()
    }
    fn ctx(d: &Data, now: i64) -> ActCtx<'_> {
        ActCtx {
            now_ms: now,
            age_secs: 0.0,
            fps: 50.0,
            data: d,
        }
    }
    fn it(s: &str) -> Item {
        Item {
            text: s.into(),
            ..Item::default()
        }
    }

    #[test]
    fn a_manual_start_keeps_its_place_once_started_on_air() {
        // Started by Continue (its clock): 3 s on air at 100 px/s, a pause keeps the 300 px.
        let l = layer(
            r#"{"id":"t","name":"T","type":"ticker","text":"A\nB","separator":"-","speed":100,"manualStart":true,"props":{}}"#,
        );
        let d = Data::new();
        let t = of(&l).unwrap();
        let mut st = State::default();
        let cx = ActCtx {
            now_ms: 10_000,
            age_secs: 3.0,
            fps: 50.0,
            data: &d,
        };
        act(&l, &mut st, "pause", &serde_json::Value::Null, &cx).unwrap();
        assert!(st.started && !st.running);
        assert!((clock(&t, &st, 20_000, 0.0, 50.0) - 300.0).abs() < 1e-9);
        // Not started yet (no time on air): a pause leaves it at its start, waiting.
        let mut st = State::default();
        act(
            &l,
            &mut st,
            "pause",
            &serde_json::Value::Null,
            &ctx(&d, 10_000),
        )
        .unwrap();
        assert!(st.started && !st.running && st.base == 0.0);
        // Reload changes nothing of the state.
        let before = st.clone();
        act(
            &l,
            &mut st,
            "reload",
            &serde_json::Value::Null,
            &ctx(&d, 11_000),
        )
        .unwrap();
        assert_eq!(st, before);
    }

    #[test]
    fn old_tickers_load() {
        let l = layer(
            r#"{"id":"a","name":"T","type":"ticker","text":"one\ntwo","separator":" - ","speed":100,"fontFamily":"Inter","fontWeight":700,"props":{}}"#,
        );
        let t = of(&l).unwrap();
        assert_eq!(*t.c, Config::default());
        assert_eq!(layer_items(&t).len(), 2);
    }

    #[test]
    fn short_content_repeats_without_gap() {
        let runs = vec![Run {
            items: vec![it("a")],
            ..Run::default()
        }];
        let p = resolve(&runs, &Data::new(), &mut |_| 100.0, 1000.0, 0.0, None);
        let s = segs(&p, 0.0, 1000.0);
        assert_eq!(s.len(), 10);
        assert!(s.windows(2).all(|w| w[1].x == w[0].x + 100.0));
    }

    #[test]
    fn edits_apply_at_the_next_pass_out_of_screen() {
        // Pass of 300 px, zone 500 px; edit when 120 px travelled.
        let runs = vec![
            Run {
                items: vec![it("a"), it("b"), it("c")],
                ..Run::default()
            },
            Run {
                at: 120.0,
                items: vec![it("X")],
                ..Run::default()
            },
        ];
        let p = resolve(&runs, &Data::new(), &mut |_| 100.0, 500.0, 0.0, None);
        // First pass boundary ≥ 620: 900.
        assert_eq!(p[1].start, 900.0);
        let s = segs(&p, 120.0, 620.0);
        assert!(s.iter().all(|g| g.item.text != "X"));
    }

    #[test]
    fn priority_takes_the_next_free_slot() {
        let runs = vec![
            Run {
                items: vec![it("a"), it("b"), it("c")],
                ..Run::default()
            },
            Run {
                at: 50.0,
                items: vec![it("a"), it("b"), it("c")],
                prio: Some(it("P")),
                kind: RunKind::Item,
            },
        ];
        let p = resolve(&runs, &Data::new(), &mut |_| 100.0, 100.0, 0.0, None);
        assert_eq!(p[1].start, 200.0);
        let s = segs(&p, 200.0, 600.0);
        let t: Vec<&str> = s.iter().map(|g| g.item.text.as_str()).collect();
        assert_eq!(t, vec!["P", "c", "a", "b"]);
    }

    #[test]
    fn passes_end_the_tape() {
        let runs = vec![Run {
            items: vec![it("a"), it("b")],
            ..Run::default()
        }];
        let p = resolve(&runs, &Data::new(), &mut |_| 100.0, 0.0, 0.0, Some(2));
        assert_eq!(tape_end(&p), Some(400.0));
    }

    #[test]
    fn queue_lives_in_the_state_and_pause_keeps_the_position() {
        let l = layer(
            r#"{"id":"a","name":"T","type":"ticker","text":"","separator":" ","speed":100,"fontFamily":"Inter","fontWeight":700,"items":[{"text":"one"}],"props":{"height":80}}"#,
        );
        let d = Data::new();
        let mut st = State::default();
        act(&l, &mut st, "start", &serde_json::Value::Null, &ctx(&d, 0)).unwrap();
        act(
            &l,
            &mut st,
            "add",
            &serde_json::json!({"text":"two"}),
            &ctx(&d, 1000),
        )
        .unwrap();
        assert_eq!(st.queue.as_ref().unwrap().len(), 2);
        assert_eq!(st.runs.len(), 2);
        assert_eq!(st.runs[1].at, 100.0);
        act(
            &l,
            &mut st,
            "pause",
            &serde_json::Value::Null,
            &ctx(&d, 2000),
        )
        .unwrap();
        let t = of(&l).unwrap();
        assert_eq!(clock(&t, &st, 9000, 0.0, 50.0), 200.0);
        act(
            &l,
            &mut st,
            "speed",
            &serde_json::json!({"px": 200}),
            &ctx(&d, 9000),
        )
        .unwrap();
        act(
            &l,
            &mut st,
            "resume",
            &serde_json::Value::Null,
            &ctx(&d, 9000),
        )
        .unwrap();
        assert_eq!(clock(&t, &st, 10_000, 0.0, 50.0), 400.0);
    }

    #[test]
    fn a_pause_keeps_following_the_items_and_speeds_stay_readable() {
        let json = |items: &str, speed: u32| {
            format!(
                r#"{{"id":"a","name":"T","type":"ticker","text":"","separator":" ","speed":{speed},"fontFamily":"Inter","fontWeight":700,"items":[{items}],"props":{{"height":80}}}}"#
            )
        };
        let l = layer(&json(r#"{"text":"one"}"#, 100));
        let d = Data::new();
        let mut st = State::default();
        act(&l, &mut st, "pause", &serde_json::Value::Null, &ctx(&d, 0)).unwrap();
        // Edited in the Designer after the pause: the tape shows the new items.
        let l2 = layer(&json(r#"{"text":"one"},{"text":"two"}"#, 100));
        let t2 = of(&l2).unwrap();
        assert_eq!(runs(&t2, &st)[0].items.len(), 2);
        // 10 to 800 px/s in the Designer and on air.
        act(
            &l,
            &mut st,
            "speed",
            &serde_json::json!({"px": 5000}),
            &ctx(&d, 10),
        )
        .unwrap();
        assert_eq!(st.speed, Some(SPEED_MAX));
        let fast = layer(&json(r#"{"text":"one"}"#, 3000));
        assert_eq!(speed(&of(&fast).unwrap(), &State::default()), SPEED_MAX);
    }

    #[test]
    fn linked_value_expands() {
        let mut d = Data::new();
        d.insert("news".into(), r#"[{"text":"A","cat":"SPORT"},"B"]"#.into());
        let e = expand(&[it("{{news}}")], &d);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].cat, "SPORT");
    }

    #[test]
    fn a_list_of_messages_reads_with_the_item_format() {
        let l = layer(
            r#"{"id":"a","name":"T","type":"ticker","text":"{{msgs}}","separator":" ","speed":100,"fontFamily":"Inter","fontWeight":700,"itemFormat":"{name} · {text}","props":{}}"#,
        );
        let items = layer_items(&of(&l).unwrap());
        let mut d = Data::new();
        d.insert(
            "msgs".into(),
            r#"[{"name":"Sophie L.","text":"Pourquoi ?","n":3},{"name":"","text":"Seul"}]"#.into(),
        );
        let e = expand(&items, &d);
        let texts: Vec<&str> = e.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["Sophie L. · Pourquoi ?", "· Seul"]);
        // A data set's table: by its column names.
        d.insert(
            "msgs".into(),
            r#"[["name","text"],["Kevin","Brest !"]]"#.into(),
        );
        assert_eq!(expand(&items, &d)[0].text, "Kevin · Brest !");
        assert_eq!(formatted("{n} min", &|_| Some("58".into())), "58 min");
    }
}

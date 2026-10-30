//! Credits: roll (vertical), crawl (horizontal) or pages (cards). Structured
//! entries (section titles, role | name lines, logos, spaces) with an import
//! from text, 1 / 2 / 3 columns, styles per level, speed from a total
//! duration or px/s (optimised: whole px per frame), start off or on
//! screen, end with a full exit or on the last element, fades.
//!
//! Progress is a fraction of the travel computed from the clock
//! (f = base + rate × (now − anchor)); changing the remaining time re-anchors
//! at the current position with a new rate: nothing jumps.
//! Drawn by `cg-render/src/wid/credits.rs` and `web/src/widgets/credits.tsx` (keep in sync).

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Roll,
    Crawl,
    Pages,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Section,
    #[default]
    Line,
    Logo,
    Space,
}

/// An entry: section (`a` = title), line (`a` = role, `b` = name), logo
/// (`a` = picture asset path, or a name for a plate), space.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub kind: Kind,
    pub a: String,
    pub b: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    One,
    #[default]
    Two,
    Three,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Style {
    pub size: f32,
    pub color: String,
    pub weight: u16,
    pub upper: bool,
}

impl Style {
    fn new(size: f32, color: &str, weight: u16, upper: bool) -> Self {
        Self {
            size,
            color: color.into(),
            weight,
            upper,
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::new(38.0, "#ffffff", 800, false)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Styles {
    pub section: Style,
    pub role: Style,
    pub name: Style,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            section: Style::new(30.0, "#a594ff", 900, true),
            role: Style::new(26.0, "#a3a9bd", 600, true),
            name: Style::new(38.0, "#ffffff", 800, false),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeedMode {
    /// A total duration (the speed follows).
    #[default]
    Duration,
    /// px/s (the duration follows).
    Pxs,
    /// Pages: seconds per card.
    Card,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Start {
    Off,
    #[default]
    On,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum End {
    Exit,
    #[default]
    Stop,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Short {
    /// Content shorter than the screen stops centred…
    #[default]
    Center,
    /// …or passes and exits.
    Exit,
}

/// Settings of the widget, flattened into the layer's JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub mode: Mode,
    pub entries: Vec<Entry>,
    /// Linked source: `{{key}}` whose value is credits text (or CSV / JSON rows) — replaces the entries.
    pub link: String,
    /// Text of the import box (Designer only).
    pub import_text: String,
    pub layout: Layout,
    pub align: Align,
    /// One font for the whole credits (the channel's), Inter by default.
    pub font: String,
    pub gutter: f32,
    pub section_gap: f32,
    pub line_gap: f32,
    pub space_h: f32,
    pub logo_h: f32,
    pub st: Styles,
    pub speed_mode: SpeedMode,
    /// Seconds.
    pub duration: f64,
    pub pxs: f64,
    /// Whole px per frame, at `fps` frames per second (the project's).
    pub optimised: bool,
    pub fps: f64,
    /// Readable maximum, px/s (Designer warning).
    pub max_pxs: f64,
    pub card_dur: f64,
    pub start: Start,
    pub end: End,
    pub short_mode: Short,
    pub fade: bool,
    pub fade_size: f32,
    pub panel: bool,
    pub panel_color: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Roll,
            entries: Vec::new(),
            link: String::new(),
            import_text: String::new(),
            layout: Layout::Two,
            align: Align::Center,
            font: "Inter".into(),
            gutter: 40.0,
            section_gap: 70.0,
            line_gap: 10.0,
            space_h: 40.0,
            logo_h: 130.0,
            st: Styles::default(),
            speed_mode: SpeedMode::Duration,
            duration: 40.0,
            pxs: 120.0,
            optimised: false,
            fps: 25.0,
            max_pxs: 260.0,
            card_dur: 4.0,
            start: Start::On,
            end: End::Stop,
            short_mode: Short::Center,
            fade: true,
            fade_size: 140.0,
            panel: true,
            panel_color: "#060812c7".into(),
        }
    }
}

// --------------------------------------------------------------- content --

/// Text import: "# Section", "Role | Name" (or `;` / tab), "[logo /assets/x.png]",
/// a line alone = a name, empty line = a space.
pub fn parse(text: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            if out.last().is_some_and(|e| e.kind != Kind::Space) {
                out.push(Entry {
                    kind: Kind::Space,
                    ..Entry::default()
                });
            }
            continue;
        }
        if let Some(s) = l.strip_prefix('#') {
            out.push(Entry {
                kind: Kind::Section,
                a: s.trim_start_matches('#').trim().to_string(),
                b: String::new(),
            });
            continue;
        }
        let low = l.to_lowercase();
        if low.starts_with("[logo") && l.ends_with(']') {
            let v = l[5..l.len() - 1].trim();
            out.push(Entry {
                kind: Kind::Logo,
                a: if v.is_empty() {
                    "PULSE TV".into()
                } else {
                    v.to_string()
                },
                b: String::new(),
            });
            continue;
        }
        let parts: Vec<&str> = line.split(['|', ';', '\t']).collect();
        if parts.len() > 1 {
            out.push(Entry {
                kind: Kind::Line,
                a: parts[0].trim().to_string(),
                b: parts[1..]
                    .iter()
                    .map(|s| s.trim())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string(),
            });
        } else {
            out.push(Entry {
                kind: Kind::Line,
                a: String::new(),
                b: l.to_string(),
            });
        }
    }
    out
}

/// Back to the import text.
pub fn to_text(es: &[Entry]) -> String {
    es.iter()
        .map(|e| match e.kind {
            Kind::Section => format!("# {}", e.a),
            Kind::Logo => format!("[logo {}]", e.a),
            Kind::Space => String::new(),
            Kind::Line if e.a.is_empty() => e.b.clone(),
            Kind::Line => format!("{} | {}", e.a, e.b),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Entries from a table (a data set): its section / logo / role / name
/// columns, else the first two columns as role and name.
fn table_entries(head: &[String], rows: &[Vec<String>]) -> Vec<Entry> {
    use super::table::find_col;
    let section = find_col(head, &["section"]);
    let logo = find_col(head, &["logo"]);
    let mut role = find_col(head, &["role", "job", "function"]);
    let mut name = find_col(head, &["name", "person"]);
    if section.is_none() && logo.is_none() && role.is_none() && name.is_none() {
        (role, name) = if head.len() >= 2 {
            (Some(0), Some(1))
        } else {
            (None, Some(0))
        };
    }
    let cell = |r: &Vec<String>, i: Option<usize>| {
        i.and_then(|i| r.get(i))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for r in rows {
        let (s, l, a, b) = (
            cell(r, section),
            cell(r, logo),
            cell(r, role),
            cell(r, name),
        );
        if !s.is_empty() {
            out.push(Entry {
                kind: Kind::Section,
                a: s,
                b: String::new(),
            });
        }
        if !l.is_empty() {
            out.push(Entry {
                kind: Kind::Logo,
                a: l,
                b: String::new(),
            });
        }
        if !b.is_empty() || !a.is_empty() {
            out.push(Entry {
                kind: Kind::Line,
                a,
                b,
            });
        }
    }
    out
}

/// The entries shown: the linked value (credits text, or JSON rows
/// {section?, role, name}), else the entries with their `{{key}}` filled.
pub fn entries(c: &Config, data: &Data) -> Vec<Entry> {
    if !c.link.trim().is_empty() {
        let v = interpolate_text(&c.link, data);
        let t = v.trim();
        if t.starts_with('[') {
            if let Ok(serde_json::Value::Array(a)) = serde_json::from_str::<serde_json::Value>(t) {
                if let Some((head, rows)) = super::table::header_rows(&a) {
                    return table_entries(&head, &rows);
                }
                let mut out = Vec::new();
                let s = |o: &serde_json::Map<String, serde_json::Value>, k: &str| {
                    o.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
                };
                for r in a {
                    if let Some(o) = r.as_object() {
                        if !s(o, "section").is_empty() {
                            out.push(Entry {
                                kind: Kind::Section,
                                a: s(o, "section"),
                                b: String::new(),
                            });
                        }
                        if !s(o, "logo").is_empty() {
                            out.push(Entry {
                                kind: Kind::Logo,
                                a: s(o, "logo"),
                                b: String::new(),
                            });
                        }
                        if !s(o, "name").is_empty() {
                            out.push(Entry {
                                kind: Kind::Line,
                                a: s(o, "role"),
                                b: s(o, "name"),
                            });
                        }
                    } else if let Some(x) = r.as_str() {
                        out.extend(parse(x));
                    }
                }
                return out;
            }
        }
        return parse(&v);
    }
    c.entries
        .iter()
        .map(|e| Entry {
            kind: e.kind,
            a: interpolate_text(&e.a, data),
            b: interpolate_text(&e.b, data),
        })
        .collect()
}

// ---------------------------------------------------------------- layout --

/// What a block shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Section(String),
    Logo(String),
    Space,
    /// Role, name per column (one, or up to three).
    Lines(Vec<(String, String)>),
}

/// A block of the roll / pages, `y` from the top of the content.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub body: Body,
    pub gap: f32,
    pub y: f32,
    pub h: f32,
    /// Lines: height of the role row (0 = none).
    pub role_h: f32,
}

fn r(v: f32) -> f32 {
    v.round()
}

/// Vertical blocks (roll / pages).
pub fn blocks(c: &Config, es: &[Entry]) -> (Vec<Block>, f32) {
    let (s, rs, ns) = (&c.st.section, &c.st.role, &c.st.name);
    let mut out: Vec<Block> = Vec::new();
    let mut i = 0;
    while i < es.len() {
        let e = &es[i];
        let first = out.is_empty();
        let b = match e.kind {
            Kind::Section => {
                i += 1;
                Block {
                    body: Body::Section(e.a.clone()),
                    gap: if first { 0.0 } else { c.section_gap },
                    y: 0.0,
                    h: r(s.size * 1.3) + r(c.line_gap * 1.5),
                    role_h: 0.0,
                }
            }
            Kind::Logo => {
                i += 1;
                Block {
                    body: Body::Logo(e.a.clone()),
                    gap: if first { 0.0 } else { c.section_gap },
                    y: 0.0,
                    h: c.logo_h,
                    role_h: 0.0,
                }
            }
            Kind::Space => {
                i += 1;
                Block {
                    body: Body::Space,
                    gap: 0.0,
                    y: 0.0,
                    h: c.space_h,
                    role_h: 0.0,
                }
            }
            Kind::Line => match c.layout {
                Layout::Three => {
                    let mut row = Vec::new();
                    while i < es.len() && es[i].kind == Kind::Line && row.len() < 3 {
                        row.push((es[i].a.clone(), es[i].b.clone()));
                        i += 1;
                    }
                    let has_role = row.iter().any(|x| !x.0.is_empty());
                    let role_h = if has_role { r(rs.size * 1.3) } else { 0.0 };
                    Block {
                        body: Body::Lines(row),
                        gap: 0.0,
                        y: 0.0,
                        h: role_h + r(ns.size * 1.25) + c.line_gap,
                        role_h,
                    }
                }
                Layout::One => {
                    i += 1;
                    let role_h = if e.a.is_empty() {
                        0.0
                    } else {
                        r(rs.size * 1.3)
                    };
                    Block {
                        body: Body::Lines(vec![(e.a.clone(), e.b.clone())]),
                        gap: 0.0,
                        y: 0.0,
                        h: role_h + r(ns.size * 1.25) + c.line_gap,
                        role_h,
                    }
                }
                Layout::Two => {
                    i += 1;
                    let lh = r(rs.size.max(ns.size) * 1.32);
                    Block {
                        body: Body::Lines(vec![(e.a.clone(), e.b.clone())]),
                        gap: 0.0,
                        y: 0.0,
                        h: lh + c.line_gap,
                        role_h: lh,
                    }
                }
            },
        };
        out.push(b);
    }
    while out.last().is_some_and(|b| b.body == Body::Space) {
        out.pop();
    }
    let mut y = 0.0;
    for b in &mut out {
        y += b.gap;
        b.y = y;
        y += b.h;
    }
    (out, y)
}

/// A card of the pages: its blocks (`y` from the card top) and height.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub blocks: Vec<Block>,
    pub h: f32,
}

/// One card per section / logo, split when taller than the box.
pub fn cards(bl: &[Block], box_h: f32) -> Vec<Card> {
    let max = box_h - 80.0;
    let mut cs: Vec<Card> = Vec::new();
    for b in bl {
        let cur_empty = cs.last().is_none_or(|c| c.blocks.is_empty());
        if b.body == Body::Space && cur_empty {
            continue;
        }
        let brk = cs.is_empty()
            || matches!(b.body, Body::Section(_) | Body::Logo(_))
            || cs.last().unwrap().h + b.h > max;
        if brk {
            cs.push(Card {
                blocks: Vec::new(),
                h: 0.0,
            });
        }
        let cur = cs.last_mut().unwrap();
        let yy = if cur.blocks.is_empty() {
            0.0
        } else {
            cur.h + b.gap
        };
        cur.blocks.push(Block { y: yy, ..b.clone() });
        cur.h = yy + b.h;
    }
    for c in &mut cs {
        while c.blocks.last().is_some_and(|b| b.body == Body::Space) {
            let b = c.blocks.pop().unwrap();
            c.h -= b.h;
        }
    }
    cs.retain(|c| !c.blocks.is_empty());
    cs
}

/// An item of the crawl, `x` from the start of the line.
#[derive(Debug, Clone, PartialEq)]
pub struct CrawlItem {
    pub body: Body,
    pub x: f32,
    pub w: f32,
    /// Lines: width of the role (incl. its 16 px gap).
    pub role_w: f32,
}

/// Width of a text in a style: (text, size, weight, upper) → px.
pub type Measure<'a> = dyn FnMut(&str, &Style) -> f32 + 'a;

pub fn styled(s: &Style, t: &str) -> String {
    if s.upper {
        t.to_uppercase()
    } else {
        t.to_string()
    }
}

/// Letter spacing of a style, px.
pub fn ls(s: &Style) -> f32 {
    if s.upper {
        s.size * 0.06
    } else {
        0.0
    }
}

/// Logo height in the crawl.
pub fn crawl_logo_h(c: &Config, box_h: f32) -> f32 {
    c.logo_h.min(box_h * 0.7)
}

/// The line of the crawl. `logo_w`: width of a logo at a height.
pub fn crawl_items(
    c: &Config,
    es: &[Entry],
    box_h: f32,
    mw: &mut Measure,
    logo_w: &mut dyn FnMut(&str, f32) -> f32,
) -> (Vec<CrawlItem>, f32) {
    let mut out: Vec<CrawlItem> = Vec::new();
    let mut x = 0.0;
    let lh = crawl_logo_h(c, box_h);
    for e in es {
        let mut gap = if out.is_empty() { 0.0 } else { c.gutter * 2.0 };
        let (body, w, role_w) = match e.kind {
            Kind::Section => {
                if !out.is_empty() {
                    gap = c.section_gap * 2.0;
                }
                (Body::Section(e.a.clone()), mw(&e.a, &c.st.section), 0.0)
            }
            Kind::Logo => {
                if !out.is_empty() {
                    gap = c.section_gap * 2.0;
                }
                (Body::Logo(e.a.clone()), logo_w(&e.a, lh), 0.0)
            }
            Kind::Space => {
                x += c.space_h;
                continue;
            }
            Kind::Line => {
                let rw = if e.a.is_empty() {
                    0.0
                } else {
                    mw(&e.a, &c.st.role) + 16.0
                };
                (
                    Body::Lines(vec![(e.a.clone(), e.b.clone())]),
                    rw + mw(&e.b, &c.st.name),
                    rw,
                )
            }
        };
        x += gap;
        out.push(CrawlItem { body, x, w, role_w });
        x += w;
    }
    (out, x)
}

/// The travel: start position `a`, end `b` (the content's top / left is at
/// `a − p` after `p` px), `x` = a − b; cards for pages.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Travel {
    pub a: f32,
    pub b: f32,
    pub x: f32,
    pub len: f32,
    pub short: bool,
}

/// Roll / crawl travel for a content of length `len` with its last element
/// centred at `last_mid`, in a box of length `boxl`.
pub fn travel(c: &Config, len: f32, last_mid: Option<f32>, boxl: f32) -> Travel {
    let mut a = match c.start {
        Start::Off => boxl,
        Start::On if c.fade => c.fade_size * 0.6,
        Start::On => 0.0,
    };
    let short = len < boxl;
    let b = if short && c.short_mode == Short::Center {
        (boxl - len) / 2.0
    } else if let (End::Stop, Some(m)) = (c.end, last_mid) {
        boxl / 2.0 - m
    } else {
        -len
    };
    a = a.max(b);
    Travel {
        a,
        b,
        x: a - b,
        len,
        short,
    }
}

/// Rough text width when no font is at hand (actions of a crawl).
pub fn estimate(t: &str, s: &Style) -> f32 {
    t.chars().count() as f32 * s.size * 0.56 + t.chars().count() as f32 * ls(s)
}

/// Travel in px (roll / crawl) or cards (pages), measured with `mw`.
pub fn travel_of(
    c: &Config,
    data: &Data,
    w: f32,
    h: f32,
    mw: &mut Measure,
    logo_w: &mut dyn FnMut(&str, f32) -> f32,
) -> f32 {
    let es = entries(c, data);
    match c.mode {
        Mode::Pages => cards(&blocks(c, &es).0, h).len() as f32,
        Mode::Roll => {
            let (bl, len) = blocks(c, &es);
            travel(c, len, bl.last().map(|b| b.y + b.h / 2.0), h).x
        }
        Mode::Crawl => {
            let (ci, len) = crawl_items(c, &es, h, mw, logo_w);
            travel(c, len, ci.last().map(|i| i.x + i.w / 2.0), w).x
        }
    }
}

/// Nominal rate from the settings, fraction of the travel per second.
pub fn nominal(c: &Config, x: f32) -> f64 {
    let x = (x as f64).max(1e-6);
    match (c.mode, c.speed_mode) {
        (_, SpeedMode::Duration) => 1.0 / c.duration.max(0.5),
        (Mode::Pages, _) => 1.0 / (c.card_dur.max(0.3) * x),
        (_, _) => c.pxs.max(1.0) / x,
    }
}

/// Frames per second of the optimised speed.
fn fps(c: &Config) -> f64 {
    if c.fps.is_finite() && c.fps >= 1.0 {
        c.fps
    } else {
        25.0
    }
}

/// Optimised speed: a whole number of px per frame.
pub fn optim(c: &Config, px_s: f64) -> f64 {
    if c.optimised && c.mode != Mode::Pages {
        (px_s / fps(c)).round().max(1.0) * fps(c)
    } else {
        px_s
    }
}

// ----------------------------------------------------------------- state --

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// false = runs from the time on air (as at the In).
    pub started: bool,
    pub running: bool,
    /// Progress at `anchor`, fraction of the travel (0 → 1).
    pub base: f64,
    pub anchor: i64,
    /// Fraction per second set live (None = from the settings).
    pub rate: Option<f64>,
}

/// Progress (0 → 1) at `now`. `x` = travel (px or cards).
pub fn progress(c: &Config, st: &State, x: f32, now_ms: i64, age_secs: f64) -> f64 {
    let rate = st.rate.unwrap_or_else(|| nominal(c, x));
    let (base, dt) = if !st.started {
        (0.0, age_secs.max(0.0))
    } else if st.running {
        (st.base, (now_ms - st.anchor).max(0) as f64 / 1000.0)
    } else {
        (st.base, 0.0)
    };
    let adv = if c.optimised && c.mode != Mode::Pages && x > 0.0 {
        let v = optim(c, rate * x as f64);
        (v / fps(c)) * (dt * fps(c)).floor() / x as f64
    } else {
        rate * dt
    };
    (base + adv).clamp(0.0, 1.0)
}

/// Seconds left.
pub fn remaining(c: &Config, st: &State, x: f32, now_ms: i64, age_secs: f64) -> f64 {
    let f = progress(c, st, x, now_ms, age_secs);
    let rate = st.rate.unwrap_or_else(|| nominal(c, x));
    let rate = if c.optimised && c.mode != Mode::Pages && x > 0.0 {
        optim(c, rate * x as f64) / x as f64
    } else {
        rate
    };
    (1.0 - f) / rate.max(1e-9)
}

/// The roll or crawl has gone all the way, as drawn on air (a crawl
/// measures its text with `m`, the renderer's fonts; its logos are guessed).
pub fn ended(
    layer: &Layer,
    st: &State,
    data: &Data,
    now_ms: i64,
    age_secs: f64,
    m: &mut dyn crate::playout::TextWidth,
) -> Option<bool> {
    let c = config(layer).ok()?;
    let num = |p: crate::model::Prop, d: f64| {
        layer.props.get(&p).and_then(|v| v.as_number()).unwrap_or(d) as f32
    };
    let (w, h) = (
        num(crate::model::Prop::Width, 600.0),
        num(crate::model::Prop::Height, 400.0),
    );
    let es = entries(c, data);
    let t = match c.mode {
        Mode::Crawl => {
            let mut mw =
                |t: &str, s: &Style| m.width(&c.font, s.weight, &styled(s, t), s.size, ls(s));
            let mut lw = |_: &str, lh: f32| lh * 3.0;
            let (items, len) = crawl_items(c, &es, h, &mut mw, &mut lw);
            travel(c, len, items.last().map(|i| i.x + i.w / 2.0), w)
        }
        _ => {
            let (bl, len) = blocks(c, &es);
            travel(c, len, bl.last().map(|b| b.y + b.h / 2.0), h)
        }
    };
    Some(progress(c, st, t.x, now_ms, age_secs) >= 1.0 - 1e-6)
}

fn config(layer: &Layer) -> Result<&Config, String> {
    match &layer.kind {
        LayerKind::Credits { x } => Ok(x),
        _ => Err("not credits".into()),
    }
}

fn prop(layer: &Layer, p: crate::model::Prop, d: f64) -> f32 {
    layer.props.get(&p).and_then(|v| v.as_number()).unwrap_or(d) as f32
}

/// Applies an operator action (`op`, with its `args`) to the state.
/// start · pause · resume · reset · next (card) · remaining {seconds, travel?} ·
/// duration {seconds} (the whole roll) · adjust {seconds: ±} ·
/// endAt {time: "hh:mm[:ss]", tz?} · finish {seconds?: 3}
/// (to the end in a few seconds, from where it is).
/// `travel`: px measured by the caller (a crawl's width needs fonts; else estimated).
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let c = config(layer)?;
    let now = ctx.now_ms;
    let x = match args.get("travel").and_then(|v| v.as_f64()) {
        Some(t) if t > 0.0 => t as f32,
        _ => {
            let (w, h) = (
                prop(layer, crate::model::Prop::Width, 1100.0),
                prop(layer, crate::model::Prop::Height, 1000.0),
            );
            travel_of(
                c,
                ctx.data,
                w,
                h,
                &mut |t, s| estimate(t, s),
                &mut |_, lh| lh * 3.0,
            )
        }
    };
    // The layer's time on air (0 while a manual start waits for its Play).
    let age = ctx.age_secs;
    let f = progress(c, st, x, now, age);
    let done = f >= 1.0 - 1e-9;
    let rebase = |st: &mut State| {
        st.base = f;
        st.anchor = now;
        if !st.started {
            st.started = true;
            st.running = !layer.manual_start;
        }
    };
    let set_remaining = |st: &mut State, r: f64| -> Result<(), String> {
        if r.is_nan() {
            return Err("seconds needed".into());
        }
        let r = r.max(0.5);
        if (st.started && !st.running && st.base <= 0.0) || done {
            // Not running: the next start lasts that long.
            st.rate = Some(1.0 / r);
            return Ok(());
        }
        rebase(st);
        st.rate = Some(((1.0 - f) / r).max(1e-6));
        Ok(())
    };
    match op {
        "start" => {
            *st = State {
                started: true,
                running: true,
                base: 0.0,
                anchor: now,
                rate: st
                    .rate
                    .filter(|_| !st.running && st.base <= 0.0 && st.started),
            };
        }
        "pause" => {
            if !st.started || st.running {
                rebase(st);
                st.running = false;
            }
        }
        "resume" => {
            if done && st.started {
                return Err("finished: Start or Reset".into());
            }
            if !st.started || !st.running {
                rebase(st);
                st.running = true;
            }
        }
        "reset" => {
            *st = State {
                started: true,
                running: false,
                base: 0.0,
                anchor: now,
                rate: None,
            };
        }
        "next" => {
            if c.mode != Mode::Pages {
                return Err("pages only".into());
            }
            let n = x.max(1.0) as f64;
            rebase(st);
            st.base = (((f * n) + 1e-6).floor() + 1.0).min(n) / n;
        }
        "remaining" => {
            let r = args
                .get("seconds")
                .and_then(|v| v.as_f64())
                .unwrap_or(f64::NAN);
            set_remaining(st, r)?;
        }
        "duration" => {
            let d = args
                .get("seconds")
                .and_then(|v| v.as_f64())
                .filter(|d| *d > 0.0)
                .ok_or("type the duration as mm:ss")?;
            // The whole roll lasts that long: from where it is, what is left takes its share.
            set_remaining(st, if done { d } else { (1.0 - f) * d })?;
        }
        "adjust" => {
            let d = args
                .get("seconds")
                .and_then(|v| v.as_f64())
                .ok_or("seconds needed")?;
            let rem = remaining(c, st, x, now, age);
            set_remaining(st, rem + d)?;
        }
        "finish" => {
            if done {
                return Ok(());
            }
            let r = args
                .get("seconds")
                .and_then(|v| v.as_f64())
                .unwrap_or(3.0)
                .max(0.5);
            let rem = remaining(c, st, x, now, age);
            rebase(st);
            st.running = true;
            st.rate = Some(((1.0 - f) / r.min(rem.max(0.5))).max(1e-6));
        }
        "endAt" => {
            let t = args
                .get("time")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let parts: Vec<&str> = t.split(':').collect();
            let nums: Option<Vec<i8>> = parts.iter().map(|p| p.trim().parse::<i8>().ok()).collect();
            let (h, m, s) = match nums.as_deref() {
                Some([h, m]) => (*h, *m, 0),
                Some([h, m, s]) => (*h, *m, *s),
                _ => return Err("type the end time as hh:mm:ss".into()),
            };
            let tz_name = args.get("tz").and_then(|v| v.as_str()).unwrap_or("");
            let tz = if tz_name.is_empty() {
                jiff::tz::TimeZone::system()
            } else {
                jiff::tz::TimeZone::get(tz_name).map_err(|e| e.to_string())?
            };
            let z = jiff::Timestamp::from_millisecond(now)
                .map_err(|e| e.to_string())?
                .to_zoned(tz);
            let target = z
                .with()
                .hour(h)
                .minute(m)
                .second(s)
                .subsec_nanosecond(0)
                .build()
                .map_err(|e| e.to_string())?;
            let r = (target.timestamp().as_millisecond() - now) as f64 / 1000.0;
            if r <= 0.5 {
                return Err("that time has already passed".into());
            }
            set_remaining(st, r)?;
        }
        _ => return Err(format!("unknown action `{op}`")),
    }
    Ok(())
}

/// Continue (F2): a manual start (on Continue) not started yet starts.
pub fn on_continue(layer: &Layer, st: &mut State, ctx: &ActCtx) -> bool {
    if !layer.manual_start || layer.start_on.as_deref() == Some("trigger") || st.started {
        return false;
    }
    act(layer, st, "start", &serde_json::Value::Null, ctx).is_ok()
}

/// The trigger `name`: a manual start on this trigger, not started yet, starts.
pub fn on_trigger(layer: &Layer, st: &mut State, name: &str, ctx: &ActCtx) -> bool {
    if !layer.manual_start
        || layer.start_on.as_deref() != Some("trigger")
        || !super::same_trigger(&layer.start_trigger, name)
        || st.started
    {
        return false;
    }
    act(layer, st, "start", &serde_json::Value::Null, ctx).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# Presented by\nPresenter | Claire Martin\nCo-host | Julien Moreau\n\n# Production\nProducer | Antoine Girard\n | Yanis Mercier\n\n[logo /assets/logo.png]";

    fn layer(c: Config) -> Layer {
        let mut l: Layer = serde_json::from_str(
            r#"{"id":"c","name":"C","type":"credits","props":{"width":1100,"height":1000}}"#,
        )
        .unwrap();
        l.kind = LayerKind::Credits { x: c };
        l
    }
    fn ctx(d: &Data, now: i64) -> ActCtx<'_> {
        ActCtx {
            now_ms: now,
            age_secs: 0.0,
            fps: 50.0,
            data: d,
        }
    }

    #[test]
    fn a_manual_start_starts_on_continue_or_on_its_trigger() {
        let d = Data::new();
        let mut l = layer(Config {
            entries: parse(TEXT),
            ..Default::default()
        });
        // At the In: Continue is not for it.
        let mut st = State::default();
        assert!(!on_continue(&l, &mut st, &ctx(&d, 1000)));
        // On Continue: it starts once, then Continue is for the others.
        l.manual_start = true;
        assert!(on_continue(&l, &mut st, &ctx(&d, 1000)));
        assert!(st.started && st.running && st.anchor == 1000);
        assert!(!on_continue(&l, &mut st, &ctx(&d, 2000)));
        assert!(!on_trigger(&l, &mut st, "go", &ctx(&d, 2000)));
        // On a trigger: its name only.
        l.start_on = Some("trigger".into());
        l.start_trigger = "Go".into();
        let mut st = State::default();
        assert!(!on_continue(&l, &mut st, &ctx(&d, 3000)));
        assert!(!on_trigger(&l, &mut st, "stop", &ctx(&d, 3000)));
        assert!(on_trigger(&l, &mut st, " go ", &ctx(&d, 3000)));
        assert!(st.started && st.running);
    }

    #[test]
    fn finish_lands_in_a_few_seconds_from_where_it_is() {
        let l = layer(Config {
            entries: parse(TEXT),
            duration: 60.0,
            ..Config::default()
        });
        let d = Data::new();
        let mut st = State::default();
        let travel = serde_json::json!({"travel": 2000.0});
        act(&l, &mut st, "start", &travel, &ctx(&d, 0)).unwrap();
        act(&l, &mut st, "pause", &travel, &ctx(&d, 10_000)).unwrap();
        act(&l, &mut st, "finish", &travel, &ctx(&d, 20_000)).unwrap();
        let LayerKind::Credits { x: c } = &l.kind else {
            panic!()
        };
        assert!(st.running);
        assert!((remaining(c, &st, 2000.0, 20_000, 0.0) - 3.0).abs() < 1e-6);
        assert!(progress(c, &st, 2000.0, 23_000, 0.0) >= 1.0 - 1e-9);
        // The font of the whole credits: Inter unless set.
        assert_eq!(Config::default().font, "Inter");
    }

    #[test]
    fn old_credits_load_and_import() {
        let l: Layer =
            serde_json::from_str(r#"{"id":"a","name":"C","type":"credits","props":{}}"#).unwrap();
        let LayerKind::Credits { x } = l.kind else {
            panic!()
        };
        assert_eq!(x, Config::default());
        let es = parse(TEXT);
        assert_eq!(es[0].kind, Kind::Section);
        assert_eq!(es[1].b, "Claire Martin");
        assert_eq!(es[3].kind, Kind::Space);
        assert_eq!(es.last().unwrap().a, "/assets/logo.png");
        assert_eq!(parse(&to_text(&es)), es);
    }

    #[test]
    fn short_content_stops_centred() {
        let c = Config {
            entries: parse("# Thanks\nPresenter | Claire"),
            ..Config::default()
        };
        let (bl, len) = blocks(&c, &c.entries);
        let t = travel(&c, len, bl.last().map(|b| b.y + b.h / 2.0), 1000.0);
        assert!(t.short);
        assert_eq!(t.b, (1000.0 - len) / 2.0);
    }

    #[test]
    fn remaining_change_keeps_the_position() {
        let c = Config {
            entries: parse(TEXT),
            duration: 60.0,
            ..Config::default()
        };
        let l = layer(c.clone());
        let d = Data::new();
        let mut st = State::default();
        act(&l, &mut st, "start", &serde_json::Value::Null, &ctx(&d, 0)).unwrap();
        let x = 1000.0;
        let f0 = progress(&c, &st, x, 30_000, 0.0);
        assert!((f0 - 0.5).abs() < 1e-9);
        act(
            &l,
            &mut st,
            "remaining",
            &serde_json::json!({"seconds": 15, "travel": x}),
            &ctx(&d, 30_000),
        )
        .unwrap();
        // Same place, then twice as fast.
        assert!((progress(&c, &st, x, 30_000, 0.0) - 0.5).abs() < 1e-9);
        assert!((remaining(&c, &st, x, 30_000, 0.0) - 15.0).abs() < 1e-6);
        act(
            &l,
            &mut st,
            "pause",
            &serde_json::Value::Null,
            &ctx(&d, 37_500),
        )
        .unwrap();
        assert!((progress(&c, &st, x, 99_000, 0.0) - 0.75).abs() < 1e-9);
        act(
            &l,
            &mut st,
            "adjust",
            &serde_json::json!({"seconds": 10, "travel": x}),
            &ctx(&d, 99_000),
        )
        .unwrap();
        act(
            &l,
            &mut st,
            "resume",
            &serde_json::Value::Null,
            &ctx(&d, 99_000),
        )
        .unwrap();
        assert!((remaining(&c, &st, x, 99_000, 0.0) - 17.5).abs() < 1e-6);
    }

    #[test]
    fn duration_sets_the_whole_roll_from_where_it_is() {
        let c = Config {
            entries: parse(TEXT),
            duration: 60.0,
            ..Config::default()
        };
        let l = layer(c.clone());
        let d = Data::new();
        let x = 1000.0;
        let travel = |s: f64| serde_json::json!({"seconds": s, "travel": x});
        // Not started yet: the next start lasts that long.
        let mut st = State::default();
        act(&l, &mut st, "reset", &serde_json::Value::Null, &ctx(&d, 0)).unwrap();
        act(&l, &mut st, "duration", &travel(120.0), &ctx(&d, 0)).unwrap();
        act(
            &l,
            &mut st,
            "start",
            &serde_json::Value::Null,
            &ctx(&d, 1_000),
        )
        .unwrap();
        assert!((remaining(&c, &st, x, 1_000, 0.0) - 120.0).abs() < 1e-6);
        // Half way: same place, the other half takes half of the new duration.
        act(&l, &mut st, "duration", &travel(60.0), &ctx(&d, 61_000)).unwrap();
        assert!((progress(&c, &st, x, 61_000, 0.0) - 0.5).abs() < 1e-9);
        assert!((remaining(&c, &st, x, 61_000, 0.0) - 30.0).abs() < 1e-6);
        assert!(st.running);
        assert!(act(
            &l,
            &mut st,
            "duration",
            &serde_json::json!({"seconds": null}),
            &ctx(&d, 61_000)
        )
        .is_err());
    }

    #[test]
    fn next_card() {
        let c = Config {
            mode: Mode::Pages,
            entries: parse(TEXT),
            ..Config::default()
        };
        let l = layer(c.clone());
        let d = Data::new();
        let n = cards(&blocks(&c, &c.entries).0, 1000.0).len();
        assert_eq!(n, 3);
        let mut st = State::default();
        act(&l, &mut st, "start", &serde_json::Value::Null, &ctx(&d, 0)).unwrap();
        act(&l, &mut st, "next", &serde_json::Value::Null, &ctx(&d, 100)).unwrap();
        assert!((st.base * n as f64 - 1.0).abs() < 1e-9);
    }
}

//! Table: data grid (manual grid with CSV paste, or a linked source: a
//! `{{key}}` whose data value is CSV / JSON rows), mapped columns, header row
//! and column, merges, widths fixed / auto / %, styles per level, borders,
//! conditional formatting, sort, pagination (pages or continuous scroll) and
//! a cascade of the rows. Operator: pages (next / prev / go / auto / pause)
//! and a highlighted row.
//! Drawn by `cg-render/src/wid/table.rs` and `web/src/widgets/table.tsx` (keep in sync).

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColKind {
    #[default]
    Text,
    /// Picture: an asset path (`/assets/fr.png`), else a chip with the code.
    Image,
    Number,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// How a row's own colour (from the data) shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RowPaint {
    /// A bar on its left.
    #[default]
    Bar,
    /// Its background.
    Bg,
    /// Its text.
    Text,
}

/// A colour read from a data cell: `#rgb`, `#rrggbb` or `#rrggbbaa` only.
pub fn cell_color(v: &str) -> Option<&str> {
    let v = v.trim();
    let hex = v.strip_prefix('#')?;
    (matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(v)
}

/// A mapped column: source column → label, type, alignment, width, format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Col {
    pub src: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: ColKind,
    pub align: Align,
    /// Fixed width, px.
    pub w: f32,
    /// Share of the width, %.
    pub pct: f32,
    /// Number format: `0`, `0.0`, `+0`, `0 %`.
    pub fmt: String,
    pub on: bool,
    /// A colour column of the data drawn as a small swatch before the text ("" = none).
    pub mark: String,
    /// Its own text (None = the text of all the rows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txt: Option<Typo>,
    /// Around the value ("pts", "%").
    pub prefix: String,
    pub suffix: String,
    /// Pictures of a picture column.
    pub img: Pic,
    /// Data columns whose value (a colour) paints this column's text / background, row by row ("" = none).
    pub color_from: String,
    pub bg_from: String,
}

impl Default for Col {
    fn default() -> Self {
        Self {
            src: String::new(),
            label: String::new(),
            kind: ColKind::Text,
            align: Align::Left,
            w: 100.0,
            pct: 10.0,
            fmt: "0".into(),
            on: true,
            mark: String::new(),
            txt: None,
            prefix: String::new(),
            suffix: String::new(),
            img: Pic::default(),
            color_from: String::new(),
            bg_from: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Case {
    #[default]
    None,
    Upper,
    Lower,
    Title,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Deco {
    #[default]
    None,
    Underline,
    Strike,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lines {
    #[default]
    One,
    Multi,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Break {
    #[default]
    Words,
    Any,
}

/// What a too-long text does: narrower then smaller (then `then`), "…", or cut.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitMode {
    #[default]
    Fit,
    Ellipsis,
    Clip,
}

/// After Fit, what still does not fit: "…" or a plain cut.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Then {
    #[default]
    Ellipsis,
    Clip,
}

/// Typography of a level of the table (title, header, rows, a column). Every
/// field is optional: unset = the level's default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Typo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    /// Letter spacing, px.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case: Option<Case>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deco: Option<Deco>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Lines>,
    /// Several lines: at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    /// Line height, × the size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lh: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brk: Option<Break>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<FitMode>,
    /// Fit: narrower down to, % (one line).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrow: Option<f32>,
    /// Fit: smaller down to, %.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub then: Option<Then>,
}

/// The texts of the table's levels (unset = defaults from the older settings).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Texts {
    pub title: Typo,
    pub header: Typo,
    pub rows: Typo,
    /// The rank column (None = the rows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<Typo>,
}

/// Weight of a kind of row (0 = the rows' weight).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KindWeights {
    pub odd: u16,
    pub even: u16,
    pub hl: u16,
    pub top3: u16,
}

/// A level's text with every setting known.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub family: String,
    pub weight: u16,
    pub size: f32,
    pub spacing: f32,
    pub case: Case,
    pub italic: bool,
    pub deco: Deco,
    pub multi: bool,
    pub max: u32,
    pub lh: f32,
    pub any_break: bool,
    pub fit: FitMode,
    pub narrow: f32,
    pub small: f32,
    pub then: Then,
}

impl Text {
    /// `t` over `base`.
    pub fn over(base: Text, t: &Typo) -> Text {
        Text {
            family: t
                .family
                .clone()
                .filter(|f| !f.is_empty())
                .unwrap_or(base.family),
            weight: t.weight.unwrap_or(base.weight),
            size: t.size.filter(|v| *v > 0.0).unwrap_or(base.size),
            spacing: t.spacing.unwrap_or(base.spacing),
            case: t.case.unwrap_or(base.case),
            italic: t.italic.unwrap_or(base.italic),
            deco: t.deco.unwrap_or(base.deco),
            multi: t.lines.map(|l| l == Lines::Multi).unwrap_or(base.multi),
            max: t.max.unwrap_or(base.max).max(1),
            lh: t.lh.filter(|v| *v > 0.0).unwrap_or(base.lh),
            any_break: t.brk.map(|b| b == Break::Any).unwrap_or(base.any_break),
            fit: t.fit.unwrap_or(base.fit),
            narrow: t.narrow.unwrap_or(base.narrow).clamp(10.0, 100.0),
            small: t.small.unwrap_or(base.small).clamp(10.0, 100.0),
            then: t.then.unwrap_or(base.then),
        }
    }
    /// The text as it shows (case applied).
    pub fn cased(&self, s: &str) -> String {
        match self.case {
            Case::None => s.to_string(),
            Case::Upper => s.to_uppercase(),
            Case::Lower => s.to_lowercase(),
            Case::Title => {
                let mut out = String::with_capacity(s.len());
                let mut start = true;
                for ch in s.chars() {
                    if start && ch.is_alphanumeric() {
                        out.extend(ch.to_uppercase());
                        start = false;
                    } else {
                        if ch.is_whitespace() || ch == '-' {
                            start = true;
                        }
                        out.push(ch);
                    }
                }
                out
            }
        }
    }
}

/// The older single setting of the too-long cells, as a level's default.
fn fit_of(c: &Config, size: f32) -> (FitMode, f32, f32) {
    match c.overflow {
        Overflow::Ellipsis => (FitMode::Ellipsis, 100.0, 100.0),
        Overflow::Squeeze => (FitMode::Fit, c.min_squeeze, 100.0),
        Overflow::Shrink => (
            FitMode::Fit,
            c.min_squeeze,
            (c.min_size / size.max(1.0) * 100.0).min(100.0),
        ),
    }
}

fn base(c: &Config, size: f32, weight: u16, spacing: f32, case: Case) -> Text {
    let (fit, narrow, small) = fit_of(c, size);
    Text {
        family: "Inter".into(),
        weight,
        size,
        spacing,
        case,
        italic: false,
        deco: Deco::None,
        multi: false,
        max: 2,
        lh: 1.1,
        any_break: false,
        fit,
        narrow,
        small,
        then: Then::Ellipsis,
    }
}

/// The title bar's text.
pub fn title_text(c: &Config) -> Text {
    let b = Text {
        narrow: 70.0,
        small: 60.0,
        fit: FitMode::Fit,
        ..base(c, 30.0, 900, 1.2, Case::Upper)
    };
    Text::over(b, &c.txt.title)
}

/// The header row's text.
pub fn header_text(c: &Config) -> Text {
    let s = c.head_size;
    Text::over(base(c, s, 800, s * 0.05, Case::Upper), &c.txt.header)
}

/// The text of every row (`size`: the animated fontSize).
pub fn rows_text(c: &Config, size: f32) -> Text {
    Text::over(base(c, size, 700, 0.0, Case::None), &c.txt.rows)
}

/// The text of a column's cells: its own, else the rows' (numbers a bit bolder by default).
pub fn col_text(c: &Config, mc: &MCol, size: f32) -> Text {
    let mut rows = rows_text(c, size);
    if mc.col.kind == ColKind::Number && c.txt.rows.weight.is_none() {
        rows.weight = 800;
    }
    let own = if mc.rank() {
        c.txt.rank.as_ref()
    } else {
        mc.col.txt.as_ref()
    };
    match own {
        Some(t) => Text::over(rows, t),
        None => rows,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Merge {
    pub on: bool,
    /// Key of the row (its team…): the merge follows it when the order
    /// changes ("" = older merges: by `row`).
    pub key: String,
    /// Rank of the row (on the whole table, 1 = first), without a key.
    pub row: u32,
    /// First column (1 = left).
    pub col: u32,
    /// Number of cells.
    pub span: u32,
    pub text: String,
}

impl Default for Merge {
    fn default() -> Self {
        Self {
            on: true,
            key: String::new(),
            row: 1,
            col: 1,
            span: 2,
            text: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Title {
    pub on: bool,
    /// "" = « Classement » / "Standings", in the project's language.
    pub text: String,
}

/// Background and text colour of a level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pair {
    pub bg: String,
    pub color: String,
}

impl Default for Pair {
    fn default() -> Self {
        Self::new("#141a33", "#ffffff")
    }
}

impl Pair {
    fn new(bg: &str, color: &str) -> Self {
        Self {
            bg: bg.into(),
            color: color.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Styles {
    pub header: Pair,
    /// Header column.
    pub hcol: Pair,
    pub odd: Pair,
    pub even: Pair,
    /// Highlighted row.
    pub hl: Pair,
    pub top3: Pair,
    pub table_bg: String,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            header: Pair::new("#7c5cff", "#ffffff"),
            hcol: Pair::new("#232a4d", "#ffffff"),
            odd: Pair::new("#141a33", "#ffffff"),
            even: Pair::new("#1c2344", "#ffffff"),
            hl: Pair::new("#ff3b5c", "#ffffff"),
            top3: Pair::new("#2c2717", "#fff4cc"),
            table_bg: "#060812d9".into(),
        }
    }
}

/// Style of one cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CellStyle {
    pub on: bool,
    /// Key of the row (follows it when the order changes; "" = by `row`, the rank).
    pub key: String,
    pub row: u32,
    pub col: u32,
    pub bg: String,
    pub color: String,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            on: false,
            key: String::new(),
            row: 1,
            col: 1,
            bg: "#facc15".into(),
            color: "#111111".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Line {
    pub on: bool,
    pub width: f32,
    pub color: String,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            on: false,
            width: 1.0,
            color: "#ffffff1f".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Borders {
    pub outer: Line,
    pub inner_h: Line,
    pub inner_v: Line,
}

impl Default for Borders {
    fn default() -> Self {
        Self {
            outer: Line {
                on: false,
                width: 4.0,
                color: "#ffffff".into(),
            },
            inner_h: Line {
                on: true,
                width: 1.0,
                color: "#ffffff14".into(),
            },
            inner_v: Line::default(),
        }
    }
}

/// Conditional formatting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cond {
    pub neg: bool,
    pub neg_color: String,
    /// The first rows marked ("top"), how many, and their accent bar.
    pub top3: bool,
    pub top3_color: String,
    pub top_n: u32,
    pub top_bar: bool,
    /// The bar on the left of the highlighted row.
    pub hl_bar: bool,
    pub hl_bar_color: String,
}

impl Default for Cond {
    fn default() -> Self {
        Self {
            neg: true,
            neg_color: "#ff6b81".into(),
            top3: false,
            top3_color: "#facc15".into(),
            top_n: 3,
            top_bar: true,
            hl_bar: true,
            hl_bar_color: "#ffffff".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PicFit {
    #[default]
    Contain,
    Cover,
    Stretch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PicMask {
    #[default]
    None,
    Rounded,
    Circle,
}

/// How a picture column draws its pictures (like the Image widget).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pic {
    pub fit: PicFit,
    /// Height, % of the row.
    pub size: f32,
    /// Width, % of the cell (Cover / Stretch).
    pub width: f32,
    pub mask: PicMask,
    pub radius: f32,
    pub opacity: f32,
    /// Shown when the cell is empty.
    pub fallback: String,
}

impl Default for Pic {
    fn default() -> Self {
        Self {
            fit: PicFit::Contain,
            size: 50.0,
            width: 80.0,
            mask: PicMask::None,
            radius: 8.0,
            opacity: 100.0,
            fallback: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CascadeStyle {
    #[default]
    Slide,
    Fade,
    Flip,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cascade {
    pub on: bool,
    pub style: CascadeStyle,
    /// Between two rows, ms.
    pub offset: f64,
    /// Of one row, ms.
    pub dur: f64,
}

impl Default for Cascade {
    fn default() -> Self {
        Self {
            on: true,
            style: CascadeStyle::Slide,
            offset: 70.0,
            dur: 450.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WidthMode {
    #[default]
    Auto,
    Fixed,
    Percent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Overflow {
    #[default]
    Shrink,
    Squeeze,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortDir {
    #[default]
    Desc,
    Asc,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageMode {
    #[default]
    Pages,
    Scroll,
}

/// How the pages go on when they are chained: by themselves every
/// `auto_secs`, on Continue (F2), or on the trigger `auto_trigger`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoOn {
    #[default]
    Timed,
    Continue,
    Trigger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmptyMode {
    #[default]
    Message,
    Hide,
}

/// Settings of the widget, flattened into the layer's JSON (every field
/// has a default so older projects still load). The text size of the cells
/// is the animated `fontSize` property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Manual data, first row = column names (cells may hold `{{key}}`).
    pub grid: Vec<Vec<String>>,
    /// Linked source: `{{key}}` whose value is CSV or JSON rows (replaces the grid when set).
    pub link: String,
    /// Mapped columns (empty = every column of the data, as text).
    pub cols: Vec<Col>,
    pub title: Title,
    pub header_row: bool,
    pub header_col_on: bool,
    /// Source column styled as a header (and the key of the rows).
    pub header_col: String,
    /// A `#` column (display order).
    pub show_rank: bool,
    pub merges: Vec<Merge>,
    pub width_mode: WidthMode,
    pub row_h: f32,
    pub pad: f32,
    pub gap: f32,
    pub head_size: f32,
    pub overflow: Overflow,
    /// Shrink down to this size, px…
    pub min_size: f32,
    /// …then squeeze down to this width, %.
    pub min_squeeze: f32,
    pub st: Styles,
    pub cell: CellStyle,
    pub borders: Borders,
    pub cf: Cond,
    /// Source column to sort by ("" = data order).
    pub sort_col: String,
    pub sort_dir: SortDir,
    pub per_page: u32,
    pub repeat_header: bool,
    pub page_mode: PageMode,
    /// Continuous scroll, px/s.
    pub scroll_speed: f64,
    /// "2 / 4" under the table.
    pub indicator: bool,
    /// Pages are chained from the take: every `auto_secs` seconds, on
    /// Continue, or on a trigger (`auto_on`).
    pub auto_pages: bool,
    pub auto_secs: f64,
    pub auto_on: AutoOn,
    pub auto_trigger: String,
    /// Key of the row highlighted from the take ("" = none).
    pub hl_key: String,
    pub cascade: Cascade,
    pub empty_mode: EmptyMode,
    pub empty_text: String,
    /// Typography of the title, header and rows.
    pub txt: Box<Texts>,
    /// Weights of the kinds of rows.
    pub kw: KindWeights,
    /// Title bar: height, px; background ("" = the default gradient); text colour.
    pub title_h: f32,
    pub title_bg: String,
    pub title_color: String,
    /// Header row height, px (0 = 85 % of a row).
    pub head_h: f32,
    /// The title bar's gradient, alignment and side margin.
    pub title_from: String,
    pub title_to: String,
    pub title_align: Align,
    pub title_pad: f32,
    /// Header alignment (None = each column's).
    pub head_align: Option<Align>,
    /// Corners of the cells, px.
    pub radius: f32,
    /// Colour of the page dots.
    pub indicator_color: String,
    /// A data column whose value (a colour) marks each row ("" = none), and how.
    pub row_from: String,
    pub row_paint: RowPaint,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            grid: Vec::new(),
            link: String::new(),
            cols: Vec::new(),
            title: Title::default(),
            header_row: true,
            header_col_on: false,
            header_col: String::new(),
            show_rank: false,
            merges: Vec::new(),
            width_mode: WidthMode::Auto,
            row_h: 66.0,
            pad: 18.0,
            gap: 4.0,
            head_size: 20.0,
            overflow: Overflow::Shrink,
            min_size: 20.0,
            min_squeeze: 70.0,
            st: Styles::default(),
            cell: CellStyle::default(),
            borders: Borders::default(),
            cf: Cond::default(),
            sort_col: String::new(),
            sort_dir: SortDir::Desc,
            per_page: 7,
            repeat_header: true,
            page_mode: PageMode::Pages,
            scroll_speed: 50.0,
            indicator: true,
            auto_pages: false,
            auto_secs: 6.0,
            auto_on: AutoOn::Timed,
            auto_trigger: String::new(),
            hl_key: String::new(),
            cascade: Cascade::default(),
            empty_mode: EmptyMode::Message,
            // "" = the project's language.
            empty_text: String::new(),
            txt: Box::default(),
            kw: KindWeights::default(),
            title_h: 64.0,
            title_bg: String::new(),
            title_color: "#ffffff".into(),
            head_h: 0.0,
            title_from: "#ff3b5c".into(),
            title_to: "#7c5cff".into(),
            title_align: Align::Left,
            title_pad: 24.0,
            head_align: None,
            radius: 0.0,
            indicator_color: "#ff3b5c".into(),
            row_from: String::new(),
            row_paint: RowPaint::Bar,
        }
    }
}

// ------------------------------------------------------------------ data --

/// Rows of a CSV text (`,` `;` or tab, the first one found in the first
/// line; quotes allowed), like the kit's `parseCsv`.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .collect();
    let first = lines.first().copied().unwrap_or("");
    let sep = [';', '\t', ',']
        .into_iter()
        .find(|s| first.contains(*s))
        .unwrap_or(',');
    lines
        .iter()
        .map(|l| {
            let mut out = Vec::new();
            let mut cur = String::new();
            let mut q = false;
            let cs: Vec<char> = l.chars().collect();
            let mut i = 0;
            while i < cs.len() {
                let ch = cs[i];
                if q {
                    if ch == '"' && cs.get(i + 1) == Some(&'"') {
                        cur.push('"');
                        i += 1;
                    } else if ch == '"' {
                        q = false;
                    } else {
                        cur.push(ch);
                    }
                } else if ch == '"' {
                    q = true;
                } else if ch == sep {
                    out.push(std::mem::take(&mut cur));
                } else {
                    cur.push(ch);
                }
                i += 1;
            }
            out.push(cur);
            out.into_iter().map(|s| s.trim().to_string()).collect()
        })
        .collect()
}

fn json_cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        // 1.0 shows as 1, like the web (JavaScript numbers).
        serde_json::Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9.0e15 && !n.is_i64() && !n.is_u64() => {
                format!("{}", f as i64)
            }
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// A data set's whole table (`[["col", …], [cell, …], …]`: the header,
/// then the rows), as header and rows.
pub fn header_rows(a: &[serde_json::Value]) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let (head, rows) = a.split_first()?;
    if !a.iter().all(|r| r.is_array()) {
        return None;
    }
    let cells = |r: &serde_json::Value| {
        r.as_array()
            .unwrap()
            .iter()
            .map(json_cell)
            .collect::<Vec<_>>()
    };
    Some((cells(head), rows.iter().map(cells).collect()))
}

/// The column named one of `names` (any case).
pub fn find_col(head: &[String], names: &[&str]) -> Option<usize> {
    head.iter()
        .position(|h| names.iter().any(|n| h.trim().eq_ignore_ascii_case(n)))
}

/// Rows of a linked value: a JSON array (of arrays, or of objects: the keys
/// make the header) or CSV text.
pub fn parse_rows(text: &str) -> Vec<Vec<String>> {
    let t = text.trim();
    if t.starts_with('[') || t.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
            let arr = match &v {
                serde_json::Value::Array(a) => a.clone(),
                serde_json::Value::Object(o) => o
                    .get("rows")
                    .or_else(|| o.get("data"))
                    .and_then(|x| x.as_array().cloned())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            if arr.iter().all(|r| r.is_array()) {
                return arr
                    .iter()
                    .map(|r| r.as_array().unwrap().iter().map(json_cell).collect())
                    .collect();
            }
            let mut head: Vec<String> = Vec::new();
            for r in &arr {
                if let Some(o) = r.as_object() {
                    for k in o.keys() {
                        if !head.contains(k) {
                            head.push(k.clone());
                        }
                    }
                }
            }
            let mut out = vec![head.clone()];
            for r in &arr {
                if let Some(o) = r.as_object() {
                    out.push(
                        head.iter()
                            .map(|k| o.get(k).map(json_cell).unwrap_or_default())
                            .collect(),
                    );
                }
            }
            return out;
        }
    }
    parse_csv(text)
}

/// The data grid: the linked source when set, else the manual grid (cells interpolated).
pub fn grid(c: &Config, data: &Data) -> Vec<Vec<String>> {
    if !c.link.trim().is_empty() {
        return parse_rows(&interpolate_text(&c.link, data));
    }
    c.grid
        .iter()
        .map(|r| r.iter().map(|s| interpolate_text(s, data)).collect())
        .collect()
}

/// A number of a cell ("1 234,5", "−3" allowed).
pub fn num(s: &str) -> Option<f64> {
    let t: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}' && *c != '\u{202f}')
        .map(|c| match c {
            '−' => '-',
            ',' => '.',
            c => c,
        })
        .collect();
    let t = t.trim_end_matches('%');
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A number written with a format (`0`, `0.0`, `+0`, `0 %`); text stays as is.
pub fn fmt_num(s: &str, f: &str) -> String {
    let Some(n) = num(s) else {
        return s.to_string();
    };
    let minus = if n < 0.0 { "−" } else { "" };
    match f {
        "0.0" => format!("{minus}{:.1}", n.abs()),
        "+0" => {
            let r = n.round();
            if r > 0.0 {
                format!("+{}", r.abs())
            } else {
                format!("{}{}", if r < 0.0 { "−" } else { "" }, r.abs())
            }
        }
        "0 %" => format!("{minus}{} %", n.abs().round()),
        _ => {
            let r = n.abs().round() as u64;
            let mut digits = r.to_string();
            if r >= 1000 {
                let b = digits.as_bytes();
                let mut o = String::new();
                for (i, ch) in b.iter().enumerate() {
                    if i > 0 && (b.len() - i).is_multiple_of(3) {
                        o.push(' ');
                    }
                    o.push(*ch as char);
                }
                digits = o;
            }
            format!("{minus}{digits}")
        }
    }
}

/// A shown column: a mapped one (with its index in the data) or the rank.
#[derive(Debug, Clone, PartialEq)]
pub struct MCol {
    pub col: Col,
    /// Index in the data row (None = the rank column).
    pub i: Option<usize>,
}

impl MCol {
    pub fn rank(&self) -> bool {
        self.i.is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MRow {
    pub r: Vec<String>,
    /// Order in the data.
    pub k: usize,
    /// Key of the row (header column, or the first text column).
    pub key: String,
    /// Display order, 1 = first.
    pub rank: usize,
}

/// The table: the shown columns, the sorted rows, and what is not mapped / missing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    pub head: Vec<String>,
    pub cols: Vec<MCol>,
    pub rows: Vec<MRow>,
    /// Columns of the data not mapped (ignored until mapped).
    pub unmapped: Vec<String>,
    /// Mapped columns not in the data.
    pub missing: Vec<String>,
}

pub fn rank_col() -> Col {
    Col {
        src: String::new(),
        label: "#".into(),
        kind: ColKind::Number,
        align: Align::Center,
        w: 80.0,
        pct: 7.0,
        fmt: "0".into(),
        on: true,
        ..Col::default()
    }
}

pub fn model(c: &Config, data: &Data) -> Model {
    let g = grid(c, data);
    let head: Vec<String> = g
        .first()
        .map(|r| r.iter().map(|h| h.trim().to_string()).collect())
        .unwrap_or_default();
    let body: Vec<Vec<String>> = g
        .iter()
        .skip(1)
        .filter(|r| r.iter().any(|x| !x.trim().is_empty()))
        .cloned()
        .collect();
    let mut cols = Vec::new();
    let mut missing = Vec::new();
    let auto = c.cols.is_empty();
    let mapped: Vec<Col> = if auto {
        head.iter()
            .filter(|h| !h.is_empty())
            .map(|h| Col {
                src: h.clone(),
                label: h.clone(),
                ..Col::default()
            })
            .collect()
    } else {
        c.cols.clone()
    };
    for col in mapped.iter().filter(|x| x.on) {
        match head.iter().position(|h| *h == col.src) {
            Some(i) => cols.push(MCol {
                col: col.clone(),
                i: Some(i),
            }),
            None => missing.push(col.src.clone()),
        }
    }
    if c.show_rank {
        cols.insert(
            0,
            MCol {
                col: rank_col(),
                i: None,
            },
        );
    }
    let unmapped = if auto {
        Vec::new()
    } else {
        head.iter()
            .filter(|h| {
                !h.is_empty()
                    && !c.cols.iter().any(|x| {
                        x.src == **h
                            || (x.on && [&x.mark, &x.color_from, &x.bg_from].contains(&&**h))
                    })
                    && **h != c.row_from
            })
            .cloned()
            .collect()
    };
    let key_i = cols
        .iter()
        .find(|m| !m.rank() && c.header_col_on && m.col.src == c.header_col)
        .or_else(|| {
            cols.iter()
                .find(|m| m.col.kind == ColKind::Text && !m.rank())
        })
        .and_then(|m| m.i);
    let mut rows: Vec<MRow> = body
        .into_iter()
        .enumerate()
        .map(|(k, r)| MRow {
            key: key_i
                .map(|i| r.get(i).cloned().unwrap_or_default())
                .unwrap_or_else(|| (k + 1).to_string()),
            r,
            k,
            rank: 0,
        })
        .collect();
    if let Some(si) = cols
        .iter()
        .find(|m| !m.rank() && !c.sort_col.is_empty() && m.col.src == c.sort_col)
        .and_then(|m| m.i)
    {
        let dir = if c.sort_dir == SortDir::Asc {
            1.0
        } else {
            -1.0
        };
        rows.sort_by(|a, b| {
            let x = a.r.get(si).map(String::as_str).unwrap_or("");
            let y = b.r.get(si).map(String::as_str).unwrap_or("");
            let d = match (num(x), num(y)) {
                (Some(nx), Some(ny)) => nx.partial_cmp(&ny).unwrap_or(std::cmp::Ordering::Equal),
                _ => x.to_lowercase().cmp(&y.to_lowercase()),
            };
            let d = if dir < 0.0 { d.reverse() } else { d };
            d.then(a.k.cmp(&b.k))
        });
    }
    for (i, r) in rows.iter_mut().enumerate() {
        r.rank = i + 1;
    }
    Model {
        head,
        cols,
        rows,
        unmapped,
        missing,
    }
}

pub fn page_count(c: &Config, m: &Model) -> usize {
    let per = c.per_page.max(1) as usize;
    m.rows.len().div_ceil(per).max(1)
}

/// The text of a cell.
pub fn cell_text(col: &MCol, row: &MRow) -> String {
    match col.i {
        None => row.rank.to_string(),
        Some(i) => {
            let v = row.r.get(i).cloned().unwrap_or_default();
            let v = if col.col.kind == ColKind::Number {
                fmt_num(&v, &col.col.fmt)
            } else {
                v
            };
            if col.col.kind == ColKind::Image || v.trim().is_empty() {
                v
            } else {
                format!("{}{v}{}", col.col.prefix, col.col.suffix)
            }
        }
    }
}

/// The merge of a row, if any: (first column index, span, text).
/// The texts the table writes itself, in the project's language (`lang`),
/// unless the Designer typed its own: the title, the message without rows.
pub fn title_or(c: &Config, lang: &str) -> String {
    if c.title.text.trim().is_empty() {
        if lang.starts_with("fr") {
            "Classement"
        } else {
            "Standings"
        }
        .into()
    } else {
        c.title.text.clone()
    }
}
pub fn empty_or(c: &Config, lang: &str) -> String {
    if c.empty_text.trim().is_empty() {
        if lang.starts_with("fr") {
            "Pas encore de données"
        } else {
            "No data yet"
        }
        .into()
    } else {
        c.empty_text.clone()
    }
}

/// A setting aimed at a row: by its key (it follows the team), else by rank.
pub fn aims(key: &str, rank: u32, row: &MRow) -> bool {
    let k = key.trim();
    if k.is_empty() {
        rank as usize == row.rank
    } else {
        row.key == k
    }
}

/// The single cell styled, when it is in this row: its column (0-based).
pub fn cell_in(c: &Config, row: &MRow) -> Option<usize> {
    (c.cell.on && aims(&c.cell.key, c.cell.row, row)).then(|| c.cell.col.max(1) as usize - 1)
}

pub fn merge_of<'a>(c: &'a Config, m: &Model, row: &MRow) -> Option<(usize, usize, &'a str)> {
    let g = c.merges.iter().find(|g| g.on && aims(&g.key, g.row, row))?;
    let j = (g.col.max(1) - 1) as usize;
    if j >= m.cols.len() {
        return None;
    }
    let span = (g.span.max(1) as usize).min(m.cols.len() - j);
    Some((j, span, g.text.as_str()))
}

/// Heights: title bar, header row.
pub const TITLE_H: f32 = 64.0;
pub fn header_h(c: &Config) -> f32 {
    if c.head_h > 0.0 {
        c.head_h
    } else {
        (c.row_h * 0.85).round()
    }
}

/// The title bar's height.
pub fn title_h(c: &Config) -> f32 {
    if c.title_h > 0.0 {
        c.title_h
    } else {
        TITLE_H
    }
}

// ----------------------------------------------------------------- state --

/// What the operator changes on air: the page, auto pages, the highlighted row.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// The operator acted (else the Designer's auto pages and highlighted row apply from the take).
    pub ctl: bool,
    /// Page shown when auto pages started (or the page set), 0 = first.
    pub page: usize,
    /// When the page last changed (cascade), Unix ms; 0 = on air.
    pub page_at: i64,
    /// Pages chained: every `interval` s counted from `auto_at` (`on` timed),
    /// or on Continue / on the trigger `trigger` (`interval` then 0).
    pub auto: bool,
    pub interval: f64,
    pub auto_at: i64,
    pub on: AutoOn,
    pub trigger: String,
    /// Paused at (Unix ms), 0 = running.
    pub paused_at: i64,
    /// Key of the highlighted row ("" = none).
    pub hl: String,
}

/// The state on air: the operator's once he acted, else the Designer's auto
/// pages (counted from the take) and highlighted row.
pub fn live(c: &Config, st: &State, now_ms: i64, age_secs: f64) -> State {
    if st.ctl {
        return st.clone();
    }
    let mut s = st.clone();
    if s.hl.is_empty() {
        s.hl = c.hl_key.trim().to_string();
    }
    if c.auto_pages && !s.auto && c.page_mode == PageMode::Pages {
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

/// The pages wait for Continue (F2): chained on it, not on the last page.
pub fn waits_continue(st: &State, pages: usize, now_ms: i64) -> bool {
    st.auto && st.on == AutoOn::Continue && current_page(st, pages, now_ms).0 + 1 < pages
}

/// Continue (F2): the next page when the pages wait for it.
pub fn on_continue(layer: &Layer, st: &mut State, ctx: &ActCtx) -> bool {
    let Ok(c) = config(layer) else {
        return false;
    };
    let pages = page_count(c, &model(c, ctx.data));
    let live_st = live(c, st, ctx.now_ms, ctx.age_secs);
    if !waits_continue(&live_st, pages, ctx.now_ms) {
        return false;
    }
    act(layer, st, "next", &serde_json::Value::Null, ctx).is_ok()
}

/// The trigger `name`: the next page when the pages wait for it (the last
/// page goes back to the first).
pub fn on_trigger(layer: &Layer, st: &mut State, name: &str, ctx: &ActCtx) -> bool {
    let Ok(c) = config(layer) else {
        return false;
    };
    let live_st = live(c, st, ctx.now_ms, ctx.age_secs);
    if !(live_st.auto
        && live_st.on == AutoOn::Trigger
        && super::same_trigger(&live_st.trigger, name))
    {
        return false;
    }
    act(layer, st, "next", &serde_json::Value::Null, ctx).is_ok()
}

/// Page shown at `now` and when it appeared (Unix ms, 0 = since on air).
pub fn current_page(st: &State, pages: usize, now_ms: i64) -> (usize, i64) {
    let pages = pages.max(1);
    if st.auto && st.interval > 0.0 && st.auto_at > 0 && pages > 1 {
        let t = if st.paused_at > 0 {
            st.paused_at
        } else {
            now_ms
        };
        let step = (st.interval * 1000.0) as i64;
        let n = ((t - st.auto_at).max(0) / step.max(1)) as usize;
        let at = if n > 0 {
            st.auto_at + n as i64 * step
        } else {
            st.page_at
        };
        return ((st.page + n) % pages, at);
    }
    (st.page.min(pages - 1), st.page_at)
}

/// Seconds left before the next automatic page (None = not running).
pub fn next_in(st: &State, now_ms: i64) -> Option<f64> {
    if !st.auto || st.interval <= 0.0 || st.auto_at <= 0 {
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

fn config(layer: &Layer) -> Result<&Config, String> {
    match &layer.kind {
        LayerKind::Table { x } => Ok(x),
        _ => Err("not a table".into()),
    }
}

fn go(st: &mut State, pages: usize, p: i64, now: i64) {
    let pages = pages.max(1) as i64;
    st.page = p.rem_euclid(pages) as usize;
    st.page_at = now;
    if st.auto {
        st.auto_at = now;
        if st.paused_at > 0 {
            st.paused_at = now;
        }
    }
}

/// Applies an operator action (`op`, with its `args`) to the state.
/// next · prev · go {page: 1…} · auto {seconds} (chained, timed) ·
/// advance {on: timed | continue | trigger, seconds?, trigger?} · pause ·
/// resume · stop (not chained, back to the first page) ·
/// highlight {row: key or rank, "" = none} · reload (the rows come in
/// again; the server reads the sources again).
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let c = config(layer)?;
    let m = model(c, ctx.data);
    let pages = page_count(c, &m);
    let now = ctx.now_ms;
    // From now on the operator's state: what was on air so far, kept.
    *st = live(c, st, now, ctx.age_secs);
    st.ctl = true;
    let (cur, _) = current_page(st, pages, now);
    let scroll = c.page_mode == PageMode::Scroll;
    match op {
        "reload" => {
            st.page_at = now;
            return Ok(());
        }
        "next" | "prev" | "go" if scroll => {
            return Err("continuous scroll: no pages".into());
        }
        "next" => go(st, pages, cur as i64 + 1, now),
        "prev" => go(st, pages, cur as i64 - 1, now),
        "go" => {
            let p = args
                .get("page")
                .and_then(|v| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok()))
                .ok_or("page number needed")?;
            if p < 1 || p as usize > pages {
                return Err(format!("page {p} does not exist (1 to {pages})"));
            }
            go(st, pages, p - 1, now);
        }
        "auto" => {
            let secs = args
                .get("seconds")
                .and_then(|v| v.as_f64())
                .unwrap_or(if st.interval > 0.0 { st.interval } else { 6.0 });
            if secs <= 0.0 {
                st.page = cur;
                st.auto = false;
                st.paused_at = 0;
                return Ok(());
            }
            if !(0.5..=3600.0).contains(&secs) {
                return Err("interval: 0.5 to 3600 s".into());
            }
            if st.auto && st.auto_at > 0 {
                // New interval: keep the page, count from now.
                st.page = cur;
                st.auto_at = if st.paused_at > 0 { st.paused_at } else { now };
            } else {
                st.page = cur;
                st.auto_at = now;
                st.paused_at = 0;
            }
            st.auto = true;
            st.on = AutoOn::Timed;
            st.interval = secs;
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
            st.page = cur;
            st.auto = true;
            st.on = on;
            st.interval = 0.0;
            st.auto_at = 0;
            st.paused_at = 0;
        }
        "stop" => {
            st.page = 0;
            st.page_at = now;
            st.auto = false;
            st.paused_at = 0;
        }
        "pause" => {
            if st.auto && st.paused_at == 0 {
                st.paused_at = now;
            }
        }
        "resume" => {
            if st.auto && st.paused_at > 0 {
                st.auto_at += now - st.paused_at;
                st.paused_at = 0;
            }
        }
        "highlight" => {
            let v = args.get("row").cloned().unwrap_or(serde_json::Value::Null);
            let key = match &v {
                serde_json::Value::Number(n) => {
                    let rank = n.as_u64().unwrap_or(0) as usize;
                    m.rows
                        .iter()
                        .find(|r| r.rank == rank)
                        .map(|r| r.key.clone())
                        .ok_or(format!("no row {rank}"))?
                }
                serde_json::Value::String(s) => s.trim().to_string(),
                _ => String::new(),
            };
            if key.is_empty() {
                st.hl.clear();
                return Ok(());
            }
            let row = m
                .rows
                .iter()
                .find(|r| r.key == key)
                .ok_or(format!("no row `{key}`"))?;
            st.hl = key;
            if !scroll {
                let p = (row.rank - 1) / c.per_page.max(1) as usize;
                if p != cur {
                    go(st, pages, p as i64, now);
                } else {
                    st.page = cur;
                    if st.auto {
                        st.auto_at = now;
                    }
                }
            }
        }
        _ => return Err(format!("unknown action `{op}`")),
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn layer(c: Config) -> Layer {
        let mut l: Layer =
            serde_json::from_str(r#"{"id":"t","name":"T","type":"table","props":{}}"#).unwrap();
        l.kind = LayerKind::Table { x: Box::new(c) };
        l
    }
    pub(crate) fn sample(n: usize) -> Config {
        let mut grid = vec![vec!["Team".to_string(), "Pts".to_string()]];
        for i in 0..n {
            grid.push(vec![format!("T{i}"), format!("{}", i * 3)]);
        }
        Config {
            grid,
            per_page: 4,
            cols: vec![
                Col {
                    src: "Team".into(),
                    label: "Team".into(),
                    ..Col::default()
                },
                Col {
                    src: "Pts".into(),
                    label: "Pts".into(),
                    kind: ColKind::Number,
                    ..Col::default()
                },
            ],
            sort_col: "Pts".into(),
            ..Config::default()
        }
    }
    #[test]
    fn a_colour_mark_counts_as_mapped() {
        let mut c = sample(2);
        for r in c.grid.iter_mut() {
            r.push(if r[0] == "Team" {
                "Color".into()
            } else {
                "#ff0000".into()
            });
        }
        assert_eq!(model(&c, &Data::new()).unmapped, vec!["Color".to_string()]);
        c.cols[0].mark = "Color".into();
        assert!(model(&c, &Data::new()).unmapped.is_empty());
        // Old tables (no mark) still load.
        let old: Col = serde_json::from_str(r#"{"src":"Team"}"#).unwrap();
        assert_eq!(old.mark, "");
        // A row colour or a column painted from the data is mapped too.
        c.cols[0].mark.clear();
        c.row_from = "Color".into();
        assert!(model(&c, &Data::new()).unmapped.is_empty());
        c.row_from.clear();
        c.cols[0].bg_from = "Color".into();
        assert!(model(&c, &Data::new()).unmapped.is_empty());
    }

    #[test]
    fn colours_from_cells() {
        assert_eq!(cell_color(" #ff0000 "), Some("#ff0000"));
        assert_eq!(cell_color("#abc"), Some("#abc"));
        assert_eq!(cell_color("#ff000080"), Some("#ff000080"));
        assert_eq!(cell_color("red"), None);
        assert_eq!(cell_color("#ff00"), None);
        assert_eq!(cell_color("#gg0000"), None);
    }
    fn ctx(data: &Data, now: i64) -> ActCtx<'_> {
        ActCtx {
            now_ms: now,
            age_secs: 0.0,
            data,
            fps: 50.0,
        }
    }

    #[test]
    fn old_tables_load() {
        let l: Layer =
            serde_json::from_str(r#"{"id":"a","name":"T","type":"table","props":{}}"#).unwrap();
        let LayerKind::Table { x } = l.kind else {
            panic!()
        };
        assert_eq!(*x, Config::default());
    }

    #[test]
    fn sorted_and_paged() {
        let c = sample(10);
        let m = model(&c, &Data::new());
        assert_eq!(m.rows[0].key, "T9");
        assert_eq!(page_count(&c, &m), 3);
        assert_eq!(fmt_num("-3", "+0"), "−3");
        assert_eq!(fmt_num("5", "+0"), "+5");
        assert_eq!(fmt_num("12345", "0"), "12 345");
    }

    #[test]
    fn pages_and_auto() {
        let l = layer(sample(10));
        let d = Data::new();
        let mut st = State::default();
        act(
            &l,
            &mut st,
            "next",
            &serde_json::Value::Null,
            &ctx(&d, 1000),
        )
        .unwrap();
        assert_eq!(st.page, 1);
        act(
            &l,
            &mut st,
            "prev",
            &serde_json::Value::Null,
            &ctx(&d, 1000),
        )
        .unwrap();
        act(
            &l,
            &mut st,
            "prev",
            &serde_json::Value::Null,
            &ctx(&d, 1000),
        )
        .unwrap();
        assert_eq!(st.page, 2);
        assert!(act(
            &l,
            &mut st,
            "go",
            &serde_json::json!({"page": 9}),
            &ctx(&d, 1)
        )
        .is_err());
        act(
            &l,
            &mut st,
            "auto",
            &serde_json::json!({"seconds": 5}),
            &ctx(&d, 10_000),
        )
        .unwrap();
        assert_eq!(current_page(&st, 3, 16_000).0, 0);
        act(
            &l,
            &mut st,
            "pause",
            &serde_json::Value::Null,
            &ctx(&d, 16_000),
        )
        .unwrap();
        assert_eq!(current_page(&st, 3, 60_000).0, 0);
        act(
            &l,
            &mut st,
            "resume",
            &serde_json::Value::Null,
            &ctx(&d, 60_000),
        )
        .unwrap();
        assert_eq!(current_page(&st, 3, 64_100).0, 1);
    }

    #[test]
    fn designer_auto_pages_and_highlight_from_the_take() {
        let mut c = sample(10);
        c.auto_pages = true;
        c.auto_secs = 6.0;
        c.hl_key = "T3".into();
        let l = layer(c.clone());
        let d = Data::new();
        // On air for 13 s: two pages turned, the Designer's row highlighted.
        let st = live(&c, &State::default(), 100_000, 13.0);
        assert_eq!(current_page(&st, 3, 100_000).0, 2);
        assert_eq!(st.hl, "T3");
        // The operator acts: what was on air is kept, then it is his.
        let mut st = State::default();
        let cx = ActCtx {
            now_ms: 100_000,
            age_secs: 13.0,
            data: &d,
            fps: 50.0,
        };
        act(&l, &mut st, "next", &serde_json::Value::Null, &cx).unwrap();
        assert!(st.ctl && st.auto);
        assert_eq!(current_page(&st, 3, 100_000).0, 0);
        act(
            &l,
            &mut st,
            "highlight",
            &serde_json::json!({"row": ""}),
            &cx,
        )
        .unwrap();
        assert_eq!(live(&c, &st, 100_000, 13.0).hl, "");
    }

    #[test]
    fn merges_and_the_cell_follow_their_row() {
        let mut c = sample(4);
        c.merges = vec![Merge {
            key: "T1".into(),
            row: 1,
            ..Merge::default()
        }];
        c.cell = CellStyle {
            on: true,
            key: "T1".into(),
            row: 1,
            col: 2,
            ..CellStyle::default()
        };
        let m = model(&c, &Data::new());
        // Sorted by points: T3, T2, T1, T0. Rank 1 is T3; T1 is third.
        let t1 = m.rows.iter().find(|r| r.key == "T1").unwrap();
        assert_eq!(t1.rank, 3);
        assert!(merge_of(&c, &m, t1).is_some());
        assert_eq!(cell_in(&c, t1), Some(1));
        assert!(m
            .rows
            .iter()
            .filter(|r| r.key != "T1")
            .all(|r| merge_of(&c, &m, r).is_none() && cell_in(&c, r).is_none()));
        // Older settings without a key: the rank.
        c.merges[0].key.clear();
        let first = &m.rows[0];
        assert!(merge_of(&c, &m, first).is_some());
    }

    #[test]
    fn generated_texts_follow_the_language() {
        let c = Config::default();
        assert_eq!(title_or(&c, "fr"), "Classement");
        assert_eq!(empty_or(&c, "en"), "No data yet");
        let c = Config {
            empty_text: "Bientôt".into(),
            ..Config::default()
        };
        assert_eq!(empty_or(&c, "fr"), "Bientôt");
    }

    #[test]
    fn data_update_keeps_the_page() {
        let st = State {
            page: 2,
            ..State::default()
        };
        assert_eq!(current_page(&st, 3, 0).0, 2);
        // The data shrinks to two pages: the last one.
        assert_eq!(current_page(&st, 2, 0).0, 1);
    }

    #[test]
    fn highlight_goes_to_its_page() {
        let l = layer(sample(10));
        let d = Data::new();
        let mut st = State::default();
        act(
            &l,
            &mut st,
            "highlight",
            &serde_json::json!({"row": "T0"}),
            &ctx(&d, 5),
        )
        .unwrap();
        assert_eq!(st.page, 2);
        act(
            &l,
            &mut st,
            "highlight",
            &serde_json::json!({"row": 1}),
            &ctx(&d, 5),
        )
        .unwrap();
        assert_eq!((st.hl.as_str(), st.page), ("T9", 0));
    }

    #[test]
    fn linked_source_and_unmapped_columns() {
        let mut c = sample(0);
        c.link = "{{rows}}".into();
        let mut d = Data::new();
        d.insert(
            "rows".into(),
            r#"[{"Team":"A","Pts":"3","xG":"1.2"},{"Team":"B","Pts":"7","xG":"0.4"}]"#.into(),
        );
        let m = model(&c, &d);
        assert_eq!(m.rows.len(), 2);
        assert_eq!(m.unmapped, vec!["xG".to_string()]);
        d.insert("rows".into(), "Team;Pts\nC;1\nD;2\nE;3".into());
        assert_eq!(model(&c, &d).rows.len(), 3);
    }
}

#[cfg(test)]
mod dataset_table_tests {
    use super::*;

    #[test]
    fn a_data_set_table_feeds_table_credits_ticker_and_chart() {
        let all = r#"[["Role","Name"],["Director","Ada"],["Writer","Linus"]]"#;
        assert_eq!(parse_rows(all)[2], vec!["Writer", "Linus"]);
        let data: Data = [("ds__table".to_string(), all.to_string())].into();
        let c: super::super::credits::Config =
            serde_json::from_value(serde_json::json!({"link": "{{ds__table}}"})).unwrap();
        let es = super::super::credits::entries(&c, &data);
        assert_eq!((es[0].a.as_str(), es[0].b.as_str()), ("Director", "Ada"));
        assert_eq!(es.len(), 2);
        let items = super::super::ticker::expand(
            &[super::super::ticker::Item {
                cat: String::new(),
                text: "{{ds__table}}".into(),
                on: true,
                format: String::new(),
            }],
            &data,
        );
        assert_eq!(
            items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
            ["Director", "Writer"]
        );
        let ch: super::super::chart::Config =
            serde_json::from_value(serde_json::json!({"source": "csv", "csv": "{{ds__table}}"}))
                .unwrap();
        assert_eq!(
            super::super::chart::resolve_grid(&ch, &data)[1],
            vec!["Director", "Ada"]
        );
    }
}

#[cfg(test)]
mod chained_pages {
    use super::*;
    use crate::anim::Data;

    #[test]
    fn pages_chained_on_continue_and_on_a_trigger() {
        let mut c = super::tests::sample(10);
        c.auto_pages = true;
        c.auto_on = AutoOn::Continue;
        let l = super::tests::layer(c.clone());
        let d = Data::new();
        let cx = ActCtx {
            now_ms: 100_000,
            age_secs: 13.0,
            data: &d,
            fps: 50.0,
        };
        // From the take: chained on Continue, no timing.
        let st = live(&c, &State::default(), 100_000, 13.0);
        assert!(st.auto && st.on == AutoOn::Continue && st.interval == 0.0);
        assert_eq!(current_page(&st, 3, 100_000).0, 0);
        assert!(waits_continue(&st, 3, 100_000));
        // Continue turns the pages, until the last one: then it is the timeline's.
        let mut st = State::default();
        assert!(on_continue(&l, &mut st, &cx));
        assert_eq!(current_page(&st, 3, 100_000).0, 1);
        assert!(on_continue(&l, &mut st, &cx));
        assert_eq!(current_page(&st, 3, 100_000).0, 2);
        assert!(!on_continue(&l, &mut st, &cx));
        // A trigger: only its name, and it wraps.
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
        assert_eq!(current_page(&st, 3, 100_000).0, 0);
        // Stop: not chained, back to the first page; timed again from Advance.
        act(&l, &mut st, "next", &serde_json::Value::Null, &cx).unwrap();
        act(&l, &mut st, "stop", &serde_json::Value::Null, &cx).unwrap();
        assert!(!st.auto);
        assert_eq!(current_page(&st, 3, 100_000).0, 0);
        act(
            &l,
            &mut st,
            "advance",
            &serde_json::json!({"on": "timed", "seconds": 5}),
            &cx,
        )
        .unwrap();
        assert!(st.auto && st.on == AutoOn::Timed && st.interval == 5.0);
        assert_eq!(current_page(&st, 3, 106_000).0, 1);
    }
}

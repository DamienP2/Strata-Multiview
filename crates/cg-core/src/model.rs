//! Data model shared by the render engine, the server and the web app.
//!
//! Everything is serialised as camelCase JSON; the TypeScript mirror lives in
//! `web/src/engine/types.ts` and must be kept in sync.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::easing::Easing;

/// A frame rate expressed as a rational (e.g. 50/1, 60000/1001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }
    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den.max(1) as f64
    }
    pub fn frame_duration(self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(self.den as f64 / self.num.max(1) as f64)
    }
}

impl Default for FrameRate {
    fn default() -> Self {
        Self::new(50, 1)
    }
}

/// RGBA colour, components in 0..=1 (straight alpha).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color(pub [f32; 4]);

impl Color {
    pub const TRANSPARENT: Color = Color([0.0, 0.0, 0.0, 0.0]);
    pub const WHITE: Color = Color([1.0, 1.0, 1.0, 1.0]);
    pub const BLACK: Color = Color([0.0, 0.0, 0.0, 1.0]);

    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Color([r, g, b, a])
    }

    /// Parses `#rgb`, `#rrggbb` or `#rrggbbaa`.
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        let hex = |i: usize, len: usize| u8::from_str_radix(s.get(i..i + len)?, 16).ok();
        let (r, g, b, a) = match s.len() {
            3 => {
                let c = |i| hex(i, 1).map(|v| v * 17);
                (c(0)?, c(1)?, c(2)?, 255)
            }
            6 => (hex(0, 2)?, hex(2, 2)?, hex(4, 2)?, 255),
            8 => (hex(0, 2)?, hex(2, 2)?, hex(4, 2)?, hex(6, 2)?),
            _ => return None,
        };
        Some(Color([r, g, b, a].map(|v| v as f32 / 255.0)))
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.0.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }
}

/// A value a property can take. Untagged so JSON stays natural:
/// numbers are numbers, colours are `[r,g,b,a]` arrays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Number(f64),
    Color(Color),
}

impl Value {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Color(_) => None,
        }
    }
    pub fn as_color(&self) -> Option<Color> {
        match self {
            Value::Color(c) => Some(*c),
            Value::Number(_) => None,
        }
    }
}

/// A keyframe on a property track. The easing describes the curve of the
/// segment that *leaves* this keyframe (After Effects semantics).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyframe {
    pub frame: f64,
    pub value: Value,
    #[serde(default)]
    pub easing: Easing,
}

/// Every animatable property. Keep in sync with `AnimProp` in the web app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Prop {
    X,
    Y,
    Width,
    Height,
    AnchorX,
    AnchorY,
    Rotation,
    ScaleX,
    ScaleY,
    Opacity,
    Fill,
    Stroke,
    StrokeWidth,
    CornerRadius,
    FontSize,
    LetterSpacing,
    /// Reveal masks, 0..1 of the layer box, handy for wipes.
    CropLeft,
    CropRight,
    CropTop,
    CropBottom,
    /// Ellipse: start and end angles (degrees) and inner radius (%) of the
    /// portion; unset, the ellipse's own settings (`start`, `end`, `inner`).
    ArcStart,
    ArcEnd,
    InnerRadius,
    /// Drop shadow of the layer (`Layer::drop_shadow`): colour, offset and
    /// blur radius in layer pixels.
    ShadowColor,
    ShadowX,
    ShadowY,
    ShadowBlur,
    /// Pen: visible portion of the path, % of its length; unset, the pen's
    /// own settings (`vStart`, `vEnd`).
    DrawStart,
    DrawEnd,
    /// Rectangle « Per corner »: radius of each corner (top left, top right,
    /// bottom right, bottom left); unset, its settings (`corners`).
    CornerTl,
    CornerTr,
    CornerBr,
    CornerBl,
    /// Second colour of a gradient (shapes, Text); unset, their settings
    /// (`fillStyle.color2`, `color2`).
    Fill2,
}

impl Prop {
    pub const ALL: &[Prop] = &[
        Prop::X,
        Prop::Y,
        Prop::Width,
        Prop::Height,
        Prop::AnchorX,
        Prop::AnchorY,
        Prop::Rotation,
        Prop::ScaleX,
        Prop::ScaleY,
        Prop::Opacity,
        Prop::Fill,
        Prop::Stroke,
        Prop::StrokeWidth,
        Prop::CornerRadius,
        Prop::FontSize,
        Prop::LetterSpacing,
        Prop::CropLeft,
        Prop::CropRight,
        Prop::CropTop,
        Prop::CropBottom,
        Prop::ArcStart,
        Prop::ArcEnd,
        Prop::InnerRadius,
        Prop::ShadowColor,
        Prop::ShadowX,
        Prop::ShadowY,
        Prop::ShadowBlur,
        Prop::DrawStart,
        Prop::DrawEnd,
        Prop::CornerTl,
        Prop::CornerTr,
        Prop::CornerBr,
        Prop::CornerBl,
        Prop::Fill2,
    ];

    pub fn is_color(self) -> bool {
        matches!(
            self,
            Prop::Fill | Prop::Stroke | Prop::ShadowColor | Prop::Fill2
        )
    }
}

fn default_scroll_speed() -> f64 {
    120.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextScroll {
    #[default]
    None,
    /// The lines one after the other, from right to left (ticker).
    Horizontal,
    /// The text from bottom to top (credit roll).
    Vertical,
}

impl TextScroll {
    pub fn is_none(&self) -> bool {
        *self == TextScroll::None
    }
}

impl LayerKind {
    /// A plain rectangle / ellipse (default settings).
    pub fn rect() -> Self {
        LayerKind::Rect {
            x: Default::default(),
        }
    }
    pub fn ellipse() -> Self {
        LayerKind::Ellipse {
            x: Default::default(),
        }
    }

    /// A scrolling text drawn as the ticker or the roll it behaves like.
    pub fn scrolling_as(&self) -> Option<LayerKind> {
        let LayerKind::Text {
            text,
            font_family,
            font_weight,
            align,
            vertical_align,
            uppercase,
            transform,
            scroll,
            speed,
            separator,
            looping,
            ..
        } = self
        else {
            return None;
        };
        let font = FontStyle {
            font_family: font_family.clone(),
            font_weight: *font_weight,
            align: *align,
            vertical_align: *vertical_align,
            uppercase: *uppercase || *transform == TextTransform::Upper,
        };
        match scroll {
            TextScroll::None => None,
            TextScroll::Horizontal => Some(LayerKind::Ticker {
                x: Default::default(),
                text: text.clone(),
                separator: separator.clone(),
                speed: *speed,
                font,
            }),
            TextScroll::Vertical => Some(LayerKind::Roll {
                text: text.clone(),
                speed: *speed,
                looping: *looping,
                font,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextTransform {
    #[default]
    None,
    Upper,
    Lower,
    /// First letter of each word.
    Capitalize,
}

impl TextTransform {
    pub fn is_none(&self) -> bool {
        *self == TextTransform::None
    }

    pub fn apply(&self, t: &str) -> String {
        match self {
            TextTransform::None => t.to_string(),
            TextTransform::Upper => t.to_uppercase(),
            TextTransform::Lower => t.to_lowercase(),
            TextTransform::Capitalize => crate::datapath::filter(t, &DataFilter::Capitalize),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    LineThrough,
}

impl TextDecoration {
    pub fn is_none(&self) -> bool {
        *self == TextDecoration::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum VerticalAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LayerKind {
    Group {
        /// Clip children to the group box (used for wipes / reveals).
        #[serde(default)]
        clip: bool,
        /// A Lottie animation placed by the Lottie widget: its file (""
        /// while none is chosen); its layers are the group's children.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lottie: Option<String>,
    },
    Rect {
        /// Settings of the spec widget (`wid::quad`).
        #[serde(flatten)]
        x: crate::wid::quad::Config,
    },
    Ellipse {
        /// Settings of the spec widget (`wid::ellipse`).
        #[serde(flatten)]
        x: crate::wid::ellipse::Config,
    },
    #[serde(rename_all = "camelCase")]
    Text {
        /// Text content; `{{key}}` placeholders are replaced by field values.
        text: String,
        font_family: String,
        #[serde(default = "default_weight")]
        font_weight: u16,
        #[serde(default)]
        align: TextAlign,
        #[serde(default)]
        vertical_align: VerticalAlign,
        #[serde(default)]
        uppercase: bool,
        /// Paragraph: lines wrap at the box width.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        wrap: bool,
        /// Case change (`uppercase` = true still means upper case).
        #[serde(default, skip_serializing_if = "TextTransform::is_none")]
        transform: TextTransform,
        #[serde(default, skip_serializing_if = "TextDecoration::is_none")]
        decoration: TextDecoration,
        /// Line height, in font sizes (default 1.2).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_height: Option<f32>,
        /// Fixed, or scrolling like a ticker (horizontal) or a credit roll (vertical).
        #[serde(default, skip_serializing_if = "TextScroll::is_none")]
        scroll: TextScroll,
        /// Scrolling speed, px/s.
        #[serde(default = "default_scroll_speed")]
        speed: f64,
        /// Between the lines of a horizontal scroll.
        #[serde(default = "default_separator")]
        separator: String,
        /// A vertical scroll comes round again.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        looping: bool,
        /// What happens when the text does not fit its box.
        #[serde(default)]
        overflow: TextOverflow,
        /// Fit: narrowest the text gets (fraction of its width), then…
        #[serde(default = "default_squeeze")]
        squeeze: f32,
        /// …smallest font size (fraction of the size set).
        #[serde(default = "default_min_size")]
        min_size: f32,
        /// The Operator warns when the text gets smaller than this.
        #[serde(default = "default_warn_below")]
        warn_below: f32,
        /// Settings of the spec widget (`wid::text`).
        #[serde(flatten)]
        x: crate::wid::text::Config,
    },
    #[serde(rename_all = "camelCase")]
    Image {
        /// Asset path (`/assets/foo.png`) or `{{key}}` binding.
        src: String,
        #[serde(default)]
        fit: ImageFit,
        /// Settings of the spec widget (`wid::image`).
        #[serde(flatten)]
        x: crate::wid::image::Config,
    },
    /// Live clock / countdown / timer.
    #[serde(rename_all = "camelCase")]
    Clock {
        #[serde(default)]
        mode: ClockMode,
        /// Tokens: yyyy yy MM M dd d HH H hh h mm m ss s f A (see widgets.rs).
        #[serde(default = "default_clock_format")]
        format: String,
        /// IANA time zone ("Europe/Paris"); empty = the server's zone.
        #[serde(default)]
        timezone: String,
        /// Countdown: "HH:mm[:ss]" (today) or "yyyy-MM-ddTHH:mm:ss";
        /// timer: duration "[HH:]mm:ss". `{{field}}` allowed.
        #[serde(default)]
        target: String,
        #[serde(flatten)]
        font: FontStyle,
        /// Settings of the spec widget (`wid::timer`).
        #[serde(flatten)]
        x: Box<crate::wid::timer::Config>,
    },
    /// Horizontal crawl: one item per line of `text`, joined by `separator`.
    #[serde(rename_all = "camelCase")]
    Ticker {
        text: String,
        #[serde(default = "default_separator")]
        separator: String,
        /// Pixels per second.
        #[serde(default = "default_ticker_speed")]
        speed: f64,
        #[serde(flatten)]
        font: FontStyle,
        /// Settings of the spec widget (`wid::ticker`).
        #[serde(flatten)]
        x: crate::wid::ticker::Config,
    },
    /// Vertical roll (credits, lists): lines scroll upwards.
    #[serde(rename_all = "camelCase")]
    Roll {
        text: String,
        #[serde(default = "default_roll_speed")]
        speed: f64,
        /// Start again from the bottom once everything has scrolled past.
        #[serde(default = "yes", rename = "loop")]
        looping: bool,
        #[serde(flatten)]
        font: FontStyle,
    },
    /// Clock face with hands.
    #[serde(rename_all = "camelCase")]
    AnalogClock {
        /// IANA time zone; empty = the server's zone.
        #[serde(default)]
        timezone: String,
        #[serde(default = "yes")]
        seconds: bool,
        /// Second hand colour (face = fill, hands and ticks = stroke).
        #[serde(default = "default_accent")]
        accent: String,
    },
    /// Number counting from `from` to `to` once on air (scores, results…).
    #[serde(rename_all = "camelCase")]
    Counter {
        #[serde(default)]
        from: String,
        to: String,
        /// Seconds.
        #[serde(default = "default_two")]
        duration: f64,
        #[serde(default)]
        decimals: u8,
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        suffix: String,
        /// Thousands separator.
        #[serde(default = "yes")]
        grouping: bool,
        #[serde(flatten)]
        font: FontStyle,
        /// Settings of the spec widget (`wid::counter`).
        #[serde(flatten)]
        x: crate::wid::counter::Config,
    },
    /// Horizontal bar filled to `value` / `max`, animated from empty.
    #[serde(rename_all = "camelCase")]
    Progress {
        value: String,
        #[serde(default = "default_hundred")]
        max: String,
        #[serde(default = "default_one")]
        duration: f64,
        /// Colour of the empty part (the bar uses the fill colour).
        #[serde(default = "default_track")]
        track: String,
        /// Settings of the spec widget (`wid::progress`).
        #[serde(flatten)]
        x: crate::wid::progress::Config,
    },
    /// Shows the lines of `text` one after another with a cross-fade.
    #[serde(rename_all = "camelCase")]
    Rotator {
        text: String,
        /// Seconds per line.
        #[serde(default = "default_four")]
        interval: f64,
        #[serde(default = "default_half")]
        fade: f64,
        #[serde(flatten)]
        font: FontStyle,
    },
    /// Reveals `text` letter by letter.
    #[serde(rename_all = "camelCase")]
    Typewriter {
        text: String,
        /// Characters per second.
        #[serde(default = "default_twenty")]
        speed: f64,
        #[serde(default = "yes")]
        cursor: bool,
        #[serde(flatten)]
        font: FontStyle,
        /// Settings of the spec widget (`wid::typewriter`).
        #[serde(flatten)]
        x: crate::wid::typewriter::Config,
    },
    /// Cycles through images (one asset path per line) with a cross-fade.
    #[serde(rename_all = "camelCase")]
    Slideshow {
        images: String,
        #[serde(default = "default_five")]
        interval: f64,
        #[serde(default = "default_fade")]
        fade: f64,
        #[serde(default)]
        fit: ImageFit,
    },
    /// QR code of `data` (URL, text…); modules use the fill colour.
    #[serde(rename_all = "camelCase")]
    QrCode {
        data: String,
        #[serde(default = "default_white")]
        background: String,
        /// Settings of the spec widget (`wid::qrcode`).
        #[serde(flatten)]
        x: crate::wid::qrcode::Config,
    },
    /// A web page or HTML (shown by browsers only: HTML5 outputs, previews).
    #[serde(rename_all = "camelCase")]
    Web {
        #[serde(flatten)]
        x: crate::wid::web::Config,
    },
    /// Social wall: approved messages of a social source as cards.
    #[serde(rename_all = "camelCase")]
    SocialWall {
        #[serde(flatten)]
        x: crate::wid::socialwall::Config,
    },
    /// A social platform's badge, or a person's avatar.
    #[serde(rename_all = "camelCase")]
    SocialBadge {
        #[serde(flatten)]
        x: crate::wid::socialbadge::Config,
    },
    /// Horizontal bar chart / poll: one "Label: value" per line.
    #[serde(rename_all = "camelCase")]
    BarChart {
        text: String,
        /// Show values as a percentage of the total.
        #[serde(default = "yes")]
        percent: bool,
        #[serde(default = "default_one")]
        duration: f64,
        #[serde(default = "default_track")]
        track: String,
        /// Labels and values (the bars use the fill colour).
        #[serde(default = "default_white")]
        text_color: String,
        #[serde(flatten)]
        font: FontStyle,
    },
    /// Regular, triangle or free polygon.
    #[serde(rename_all = "camelCase")]
    Polygon {
        /// Settings of the spec widget (`wid::polygon`).
        #[serde(flatten)]
        x: crate::wid::polygon::Config,
    },
    /// Bézier path drawn with the pen tool.
    #[serde(rename_all = "camelCase")]
    Pen {
        /// Settings of the spec widget (`wid::pen`).
        #[serde(flatten)]
        x: crate::wid::pen::Config,
    },
    /// Table of data with pages.
    #[serde(rename_all = "camelCase")]
    Table {
        /// Settings of the spec widget (`wid::table`).
        #[serde(flatten)]
        x: Box<crate::wid::table::Config>,
    },
    /// Structured credits: roll, crawl or pages.
    #[serde(rename_all = "camelCase")]
    Credits {
        /// Settings of the spec widget (`wid::credits`).
        #[serde(flatten)]
        x: crate::wid::credits::Config,
    },
    /// Columns, areas, lines or pie.
    #[serde(rename_all = "camelCase")]
    Chart {
        /// Settings of the spec widget (`wid::chart`).
        #[serde(flatten)]
        x: crate::wid::chart::Config,
    },
    /// Vector map with views, markers, routes and zones.
    #[serde(rename_all = "camelCase")]
    Map {
        /// Settings of the spec widget (`wid::map`).
        #[serde(flatten)]
        x: crate::wid::map::Config,
    },
    /// Sport scoreboard with its counting rules.
    #[serde(rename_all = "camelCase")]
    Scoreboard {
        /// Settings of the spec widget (`wid::scoreboard`).
        #[serde(flatten)]
        x: crate::wid::scoreboard::Config,
    },
}

fn default_accent() -> String {
    "#ff3b5c".into()
}
fn default_track() -> String {
    "#ffffff33".into()
}
fn default_white() -> String {
    "#ffffff".into()
}
fn default_hundred() -> String {
    "100".into()
}
fn default_half() -> f64 {
    0.5
}
fn default_one() -> f64 {
    1.0
}
fn default_two() -> f64 {
    2.0
}
fn default_four() -> f64 {
    4.0
}
fn default_five() -> f64 {
    5.0
}
fn default_fade() -> f64 {
    0.8
}
fn default_twenty() -> f64 {
    20.0
}

/// Font settings shared by the text-based widgets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontStyle {
    #[serde(default = "default_font")]
    pub font_family: String,
    #[serde(default = "default_weight")]
    pub font_weight: u16,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default)]
    pub vertical_align: VerticalAlign,
    #[serde(default)]
    pub uppercase: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ClockMode {
    /// Time of day.
    #[default]
    Time,
    /// Remaining time until `target` (time of day or date-time).
    Countdown,
    /// Time since the graphic was taken on air.
    Elapsed,
    /// Counts down from the `target` duration once on air.
    Timer,
}

fn default_language() -> String {
    "fr".into()
}

fn default_font() -> String {
    "Inter".into()
}
fn default_clock_format() -> String {
    "HH:mm:ss".into()
}
fn default_separator() -> String {
    "   •   ".into()
}
fn default_ticker_speed() -> f64 {
    180.0
}
fn default_roll_speed() -> f64 {
    60.0
}

fn default_weight() -> u16 {
    400
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Stretch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(flatten)]
    pub kind: LayerKind,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    /// Timed widgets (timer, ticker…) wait for the operator to start them:
    /// on Continue, or on the trigger `start_trigger` when `start_on` is
    /// "trigger".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub manual_start: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_on: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start_trigger: String,
    /// Scale X and Y change together (designer setting).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scale_lock: bool,
    /// Width and height keep their ratio when resized (designer setting).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lock_ratio: bool,
    /// Drop shadow under the layer (and its children), set by the
    /// `Shadow*` properties; the Text widget has its own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drop_shadow: bool,
    /// Properties shown on the Designer's timeline (animatable); none =
    /// position, size and opacity. Properties with keyframes always are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<Vec<Prop>>,
    /// Static (base) values; a property with keyframes ignores its base value.
    #[serde(default)]
    pub props: BTreeMap<Prop, Value>,
    #[serde(default)]
    pub tracks: BTreeMap<Prop, Vec<Keyframe>>,
    /// Bind a property (colour or number) to a data field key.
    #[serde(default)]
    pub bindings: BTreeMap<Prop, String>,
    /// Bind a setting of the layer (a dotted path in its JSON: `mask`,
    /// `crop.l`, `teams.0.logo`…) to a data field key: what the Designer
    /// exposes to the Operator beyond the animatable properties.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub setting_bindings: BTreeMap<String, String>,
    /// Changes in the project's other formats (9:16, 1:1…), by format id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, LayerOverride>,
    /// How a new value of the data it shows (a source, a data set) shows on
    /// air, by data key (set at its port in the Data canvas); none = at once.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub live_changes: BTreeMap<String, LiveChange>,
    /// When the layer is shown, in frames of the template's timeline (its
    /// bar in the Designer's timeline); none = all along.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// A movement repeated while the layer is on air (Designer › Animations ›
    /// Loop): it runs on the layer's age, also once the template holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
}

/// A movement a layer repeats (see [`crate::anim::apply_motion`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Motion {
    pub kind: MotionKind,
    /// One cycle, in seconds.
    pub period: f64,
    /// The frame of the template from which it moves (the end of the in
    /// animation); before, the layer is as set.
    #[serde(default)]
    pub from: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MotionKind {
    /// Grows a little and back.
    Pulse,
    /// Fades a little and back.
    Blink,
    /// Turns a few degrees each way.
    Swing,
    /// Goes up and down.
    Float,
    /// Shakes sideways.
    Shake,
    /// A slow zoom in and out (pictures).
    KenBurns,
}

/// A stretch of a template's timeline: from (shown) to (gone), in frames.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub from: f64,
    pub to: f64,
}

impl Layer {
    /// Shown at `frame` of its template: visible, and within its span.
    pub fn shown_at(&self, frame: f64) -> bool {
        self.visible && self.span.is_none_or(|s| frame >= s.from && frame < s.to)
    }
}

/// How a layer shows a new value of its data on air.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveChange {
    pub kind: ValueChange,
    /// Its length (0.6 s when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secs: Option<f64>,
}

/// How a layer differs in one format: its own static values, animations
/// and visibility; anything not listed is inherited from the main format.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerOverride {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<Prop, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tracks: BTreeMap<Prop, Vec<Keyframe>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
}

/// An alternative layout of a project, e.g. 9:16 for social media. The
/// Designer aids of a project. Positions are fractions of the picture
/// (0..1), so they hold in every format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guides {
    /// Vertical guides (x).
    #[serde(default)]
    pub vertical: Vec<f64>,
    /// Horizontal guides (y).
    #[serde(default)]
    pub horizontal: Vec<f64>,
    /// Action safe and title safe margins (fractions, 0.05 = 5 %).
    #[serde(default = "default_action_safe")]
    pub action_safe: f64,
    #[serde(default = "default_title_safe")]
    pub title_safe: f64,
    /// Safe areas placed by hand (instead of the margins).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_area: Option<Area>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_area: Option<Area>,
    /// Areas graphics must leave clear (channel bug, sign language box…).
    #[serde(default)]
    pub exclusions: Vec<Exclusion>,
}

fn default_action_safe() -> f64 {
    0.05
}
fn default_title_safe() -> f64 {
    0.1
}

impl Default for Guides {
    fn default() -> Self {
        Guides {
            vertical: Vec::new(),
            horizontal: Vec::new(),
            action_safe: default_action_safe(),
            title_safe: default_title_safe(),
            action_area: None,
            title_area: None,
            exclusions: Vec::new(),
        }
    }
}

impl Guides {
    pub fn is_default(&self) -> bool {
        *self == Guides::default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exclusion {
    pub id: String,
    pub name: String,
    /// Fractions of the picture.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// main format is the project's own width × height (id "main").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Format {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

pub const MAIN_FORMAT: &str = "main";

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerKind {
    /// Playback stops here and resumes as set by [`Marker::resume`]
    /// (operator "Continue", after a delay, or on a trigger). Before the
    /// on-air point they build the graphic in steps; after it, they stop
    /// the out animation.
    Pause,
    /// The on-air point, which is also where the out animation starts: the
    /// take holds here, "Take out" plays on from here. One per template.
    Out,
    /// Simple label, no playback effect.
    Label,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub id: String,
    pub frame: f64,
    pub name: String,
    pub kind: MarkerKind,
    /// How a pause resumes.
    #[serde(default, skip_serializing_if = "Resume::is_manual")]
    pub resume: Resume,
}

/// How playback resumes from a pause marker.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Resume {
    /// The operator presses Continue.
    #[default]
    Manual,
    /// By itself after a delay.
    After { seconds: f64 },
    /// When this trigger is fired (by the operator, the API, Companion…).
    Trigger { name: String },
    /// When the template's Triggers say so (their « Sortir » or « Reprendre
    /// la pause » node, wired to events the server watches on air).
    Triggers,
}

/// What an event node of the Triggers watches on air (the server).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TriggerEvent {
    /// A ticker or a credit roll is done: after `passes` passes (0 = at its end).
    ScrollEnd {
        layer: String,
        #[serde(default)]
        passes: u32,
    },
    /// A countdown reaches zero.
    TimerZero { layer: String },
    /// A timer shows this time (seconds).
    TimerAt { layer: String, seconds: f64 },
    /// A video (or an animated sequence) ends.
    VideoEnd { layer: String },
    /// A data set shows its last row.
    LastRow { dataset: String },
    /// A value changes.
    DataChange { key: String },
    /// A value meets a condition: `op` is `=`, `!=`, `>`, `<`, `>=`, `<=` or `contains`.
    DataIs {
        key: String,
        op: String,
        value: String,
    },
    /// Another graphic goes on air (`out`: goes out).
    Graphic {
        template: String,
        #[serde(default)]
        out: bool,
    },
}

impl TriggerEvent {
    /// A value meeting the condition (numbers compared as numbers).
    pub fn holds(op: &str, have: &str, want: &str) -> bool {
        let (a, b) = (have.trim(), want.trim());
        let nums = a
            .replace(',', ".")
            .parse::<f64>()
            .ok()
            .zip(b.replace(',', ".").parse::<f64>().ok());
        let ord = match nums {
            Some((x, y)) => x.partial_cmp(&y),
            None => Some(a.to_lowercase().cmp(&b.to_lowercase())),
        };
        match op.trim() {
            "!=" | "≠" => ord != Some(std::cmp::Ordering::Equal),
            ">" => ord == Some(std::cmp::Ordering::Greater),
            "<" => ord == Some(std::cmp::Ordering::Less),
            ">=" | "≥" => matches!(
                ord,
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            ),
            "<=" | "≤" => matches!(
                ord,
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ),
            "contains" => a.to_lowercase().contains(&b.to_lowercase()),
            _ => ord == Some(std::cmp::Ordering::Equal),
        }
    }
}

impl Resume {
    pub fn is_manual(&self) -> bool {
        matches!(self, Resume::Manual)
    }
}

/// Current marker model: [`MarkerKind::Out`] is the on-air point.
pub const MARKER_MODEL: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Text,
    Multiline,
    Number,
    Color,
    Image,
    /// On / off ("true" / "false"; 1 / 0 for a number property).
    Boolean,
    /// Rows of a table typed or imported by the operator (JSON `[[header…], [cells…], …]`).
    Table,
}

/// A data input exposed to the operator (a field of the template).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub default: String,
    /// Live value from a data source (see [`crate::datapath`]); it
    /// replaces what the operator types while the source has a value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<FieldBinding>,
    /// Range of a number (slider in the editors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Choices of an exposed setting (value, label): the Operator picks one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,
}

/// Where a field takes its value: a data source and a path in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldBinding {
    pub source: String,
    /// `items[0].title`, `rows[{row}].name`, `players[number={num}].name`,
    /// `items[*].title` (all, one per line), `items.length`.
    pub path: String,
    /// Applied in order to the value read (see [`crate::datapath::filter`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<DataFilter>,
    /// Sorts a list of the source before the path reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<DataSort>,
    /// Table pipeline (the Data canvas): the list read as rows (dotted
    /// keys, "" = the root), then `steps`. When set, `path`, `filters`
    /// and `sort` are not used (see [`crate::datapath::pipeline`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<DataStep>,
    /// How a new value of the source shows on air (at once when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<ValueChange>,
    /// Length of that change, in seconds (0.6 when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_secs: Option<f64>,
}

/// How a value its data source changed shows on air.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueChange {
    /// At once.
    #[default]
    Cut,
    /// The graphic with the old value fades into the one with the new.
    Fade,
    /// In the layers showing it: the old value goes up, the new comes from below.
    Slide,
    /// In the layers showing it: turned over, the new value on the back.
    Flip,
    /// Numbers count from the old value to the new (else a cross-fade).
    Count,
    /// The graphic goes out and comes in again with the new value.
    OutIn,
    /// Not on air by itself: the value shown stays until the operator
    /// takes or updates the graphic.
    Hold,
}

/// Rows of a source (after its steps) shown one at a time: the value of
/// each column of the current row is in the data under
/// [`DataSet::column_key`], the row number (1 = first) under
/// [`DataSet::row_key`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSet {
    /// Data keys start with it (`standings` → `standings_team`).
    pub key: String,
    #[serde(default)]
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub list: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<DataStep>,
    /// How the graphic on air goes from one row to the next.
    #[serde(default)]
    pub change: RowChange,
}

impl DataSet {
    pub fn row_key(&self) -> String {
        format!("{}__row", self.key)
    }
    pub fn count_key(&self) -> String {
        format!("{}__count", self.key)
    }
    /// All its rows, for widgets showing a table: JSON `[[header…], [cells…], …]`.
    pub fn table_key(&self) -> String {
        format!("{}__table", self.key)
    }
    /// Its rows as text, a line per row, for a Text (only the current row when `one_key` is "1").
    pub fn lines_key(&self) -> String {
        format!("{}__lines", self.key)
    }
    /// "1": the Text shows one entry (the current row), chosen by the operator.
    pub fn one_key(&self) -> String {
        format!("{}__one", self.key)
    }
    /// Seconds between rows when the rows go on by themselves (empty: off).
    pub fn auto_key(&self) -> String {
        format!("{}__auto", self.key)
    }
    /// "1": after the last row, the first again.
    pub fn loop_key(&self) -> String {
        format!("{}__loop", self.key)
    }
    /// Age on air (frames) when the row last changed without going out.
    pub fn shown_key(&self) -> String {
        format!("{}__shown", self.key)
    }
    /// How the row changes, chosen by the operator ("outIn", "fade", "cut");
    /// unset = the data set's own setting.
    pub fn change_key(&self) -> String {
        format!("{}__change", self.key)
    }
    /// The way rows change for this graphic.
    pub fn change_in(&self, data: &crate::anim::Data) -> RowChange {
        match data.get(&self.change_key()).map(|s| s.trim()) {
            Some("outIn") => RowChange::OutIn,
            Some("fade") => RowChange::Fade,
            Some("cut") => RowChange::Cut,
            _ => self.change,
        }
    }
    /// Rows the operator added: JSON `[{"at": 3, "cells": {"team": "Nice"}}]`
    /// (`at`: the row number it takes, none = at the end).
    pub fn added_key(&self) -> String {
        format!("{}__added", self.key)
    }
    /// Data key of a column: letters, digits and `_` only.
    pub fn column_key(&self, column: &str) -> String {
        let c: String = column
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        format!("{}_{}", self.key, c)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RowChange {
    /// The graphic plays its out animation, then comes in with the new row.
    #[default]
    OutIn,
    /// The new row fades over the old one.
    Fade,
    /// The values change at once.
    Cut,
}

/// What a text does when it is too long for its box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextOverflow {
    /// Narrower, then smaller (down to its minimums), then cut at the box.
    #[default]
    Fit,
    /// Cut with "…".
    Ellipsis,
    /// Cut at the box.
    Clip,
    /// Drawn outside its box.
    Visible,
}

fn default_squeeze() -> f32 {
    0.8
}
fn default_min_size() -> f32 {
    0.6
}
fn default_warn_below() -> f32 {
    0.8
}

/// One node of the table pipeline between a source and a field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DataStep {
    /// Keeps the rows matching the conditions (all of them, or any with
    /// `any`), then rows `from` (1 = first, negative = from the end),
    /// `count` of them (0 = all), then the `columns` (empty = all).
    Filter {
        #[serde(default, rename = "where")]
        conditions: Vec<Condition>,
        #[serde(default)]
        any: bool,
        #[serde(default)]
        from: i64,
        #[serde(default)]
        count: usize,
        #[serde(default)]
        columns: Vec<String>,
    },
    /// Orders the rows by the keys, the first key first.
    Sort {
        #[serde(default)]
        keys: Vec<SortKey>,
    },
    /// Text operations on the cells of `columns` (empty = all). `join`
    /// makes the whole table one text, `default` fills an empty table.
    Transform {
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        ops: Vec<DataFilter>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub column: String,
    pub op: CondOp,
    /// Compared value; `{key}` is replaced by the item's data.
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CondOp {
    Eq,
    Ne,
    Contains,
    NotContains,
    StartsWith,
    Gt,
    Ge,
    Lt,
    Le,
    Empty,
    NotEmpty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortKey {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
    #[serde(default)]
    pub numeric: bool,
}

/// Sorts the list at `list` (dotted keys from the root, "" = the root) by
/// the value of `by` in each element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSort {
    #[serde(default)]
    pub list: String,
    pub by: String,
    #[serde(default)]
    pub descending: bool,
    /// Compare as numbers (else as text).
    #[serde(default)]
    pub numeric: bool,
}

/// A step between a data source and a field: changes the text read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DataFilter {
    Upper,
    Lower,
    /// First letter of each word in capitals.
    Capitalize,
    Trim,
    Prefix {
        text: String,
    },
    Suffix {
        text: String,
    },
    Replace {
        find: String,
        with: String,
    },
    /// At most `max` characters, then "…".
    Truncate {
        max: usize,
    },
    /// Lines `from` (1 = first, negative = from the end), `count` of them (0 = all).
    Lines {
        from: i64,
        count: usize,
    },
    /// The lines joined with `separator` (a list on one line).
    Join {
        separator: String,
    },
    /// A number with `decimals` and thousands grouping.
    Number {
        decimals: u8,
        grouping: bool,
    },
    /// A number: value `op` operand.
    Math {
        op: FilterOp,
        value: f64,
    },
    /// `text` when the value is empty (or missing).
    Default {
        text: String,
    },
    /// `then` when the value equals `when`, else unchanged.
    Map {
        when: String,
        then: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    pub name: String,
    /// Total timeline length in frames.
    pub duration: f64,
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default)]
    pub fields: Vec<Field>,
    /// Layers in paint order (first = bottom).
    #[serde(default)]
    pub layers: Vec<Layer>,
    /// Zone the template plays in by default (see [`Zone`]).
    #[serde(default)]
    pub zone: String,
    /// Relations between values (node graph), see [`crate::logic`].
    #[serde(default, skip_serializing_if = "crate::logic::Logic::is_empty")]
    pub logic: crate::logic::Logic,
    /// What happens on air (events → out, a pause playing on, a trigger…),
    /// the same kind of graph, evaluated by the server, see [`crate::logic`].
    #[serde(default, skip_serializing_if = "crate::logic::Logic::is_empty")]
    pub triggers: crate::logic::Logic,
    /// The Data canvas graph (sources, filters, sorts, transforms and their
    /// wires), kept as the editor wrote it; the fields' bindings are
    /// compiled from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataflow: Option<serde_json::Value>,
    /// Tables whose rows the operator steps through (compiled from the
    /// Data canvas); see [`crate::datapath::bind_datasets`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datasets: Vec<DataSet>,
    /// Published revision (1, 2…); 0 for a template never published.
    #[serde(default)]
    pub revision: u32,
    /// Marker model the template was made with ([`MARKER_MODEL`]); older
    /// templates are converted by [`Template::normalize_markers`].
    #[serde(default)]
    pub marker_model: u8,
}

/// The widget a scrolling Text of older projects becomes: horizontal, the
/// Ticker it was drawn as (its lines are the items); vertical, the Credits
/// (its lines are the names, at its speed in px/s, in its font).
fn legacy_scroll(kind: &LayerKind) -> Option<LayerKind> {
    let LayerKind::Text {
        text,
        font_family,
        scroll,
        speed,
        ..
    } = kind
    else {
        return None;
    };
    match scroll {
        TextScroll::None => None,
        TextScroll::Horizontal => kind.scrolling_as(),
        TextScroll::Vertical => {
            use crate::wid::credits::{parse, Config, SpeedMode};
            let mut x = Config::default();
            // Wired to one value (`{{key}}`): the credits read it as their text.
            let t = text.trim();
            if t.starts_with("{{") && t.ends_with("}}") && t.matches("{{").count() == 1 {
                x.link = t.to_string();
                x.entries.clear();
            } else {
                x.entries = parse(text);
            }
            x.speed_mode = SpeedMode::Pxs;
            x.pxs = *speed;
            x.font = font_family.clone();
            Some(LayerKind::Credits { x })
        }
    }
}

impl Template {
    /// An empty template: 1 s long (at `fps`), on air at half of it.
    pub fn blank(name: &str, zone: &str, fps: f64) -> Self {
        Self::blank_secs(name, zone, fps, 1.0)
    }

    /// An empty template `secs` long (its On air point half way).
    pub fn blank_secs(name: &str, zone: &str, fps: f64, secs: f64) -> Self {
        let len = (fps * secs.clamp(0.1, 600.0)).round().max(2.0);
        Template {
            id: new_id(),
            name: name.into(),
            duration: len,
            markers: vec![Marker {
                id: new_id(),
                frame: (len / 2.0).round(),
                name: "On air".into(),
                kind: MarkerKind::Out,
                resume: Resume::Manual,
            }],
            fields: Vec::new(),
            layers: Vec::new(),
            zone: zone.into(),
            logic: Default::default(),
            triggers: Default::default(),
            dataflow: None,
            datasets: vec![],
            revision: 0,
            marker_model: MARKER_MODEL,
        }
    }

    /// The template as laid out in `format` (overrides applied); the main
    /// format, or a format without overrides, borrows it unchanged.
    pub fn in_format(&self, format: &str) -> std::borrow::Cow<'_, Template> {
        if format == MAIN_FORMAT || !self.layers.iter().any(|l| l.overrides.contains_key(format)) {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut t = self.clone();
        for l in &mut t.layers {
            if let Some(o) = l.overrides.get(format) {
                for (p, v) in &o.props {
                    l.props.insert(*p, *v);
                    l.tracks.remove(p);
                }
                for (p, k) in &o.tracks {
                    l.tracks.insert(*p, k.clone());
                }
                if let Some(v) = o.visible {
                    l.visible = v;
                }
            }
        }
        std::borrow::Cow::Owned(t)
    }

    /// The template with its exposed settings set from `data` (see
    /// [`Layer::setting_bindings`]); without any, it is borrowed unchanged.
    pub fn with_settings(&self, data: &BTreeMap<String, String>) -> std::borrow::Cow<'_, Template> {
        let bound = |l: &Layer| l.setting_bindings.values().any(|k| data.contains_key(k));
        if !self.layers.iter().any(bound) {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut t = self.clone();
        for l in t.layers.iter_mut().filter(|l| bound(l)) {
            let Ok(mut json) = serde_json::to_value(&*l) else {
                continue;
            };
            for (path, key) in &l.setting_bindings {
                if let Some(raw) = data.get(key) {
                    set_setting(&mut json, path, raw);
                }
            }
            if let Ok(patched) = serde_json::from_value::<Layer>(json) {
                *l = patched;
            }
        }
        std::borrow::Cow::Owned(t)
    }

    /// The on-air frame (what the graphic looks like on air): the on-air /
    /// out marker, else the end.
    pub fn hold_frame(&self) -> f64 {
        self.out_frame().unwrap_or(self.duration)
    }

    /// Where playback stops next after `frame`: the next pause or the
    /// on-air point, else the end.
    pub fn next_stop_after(&self, frame: f64) -> f64 {
        let on_air = self.out_frame().filter(|&o| o > frame + 1e-6);
        self.markers
            .iter()
            .filter(|m| m.kind == MarkerKind::Pause && m.frame > frame + 1e-6)
            .map(|m| m.frame)
            .chain(on_air)
            .fold(self.duration, f64::min)
    }

    /// The pause marker at `frame`, if any.
    pub fn pause_at(&self, frame: f64) -> Option<&Marker> {
        self.markers
            .iter()
            .find(|m| m.kind == MarkerKind::Pause && (m.frame - frame).abs() < 1e-6)
    }

    /// Texts written with style tags (made before the variables had their
    /// own looks): each styled part becomes a variable of the template
    /// keeping its looks. Returns true if anything changed.
    fn unstyle_texts(&mut self) -> bool {
        let mut changed = false;
        for i in 0..self.layers.len() {
            let LayerKind::Text { text, .. } = &self.layers[i].kind else {
                continue;
            };
            if !text.contains('<') {
                continue;
            }
            let mut made: Vec<Field> = Vec::new();
            let fields = &self.fields;
            let Some((plain, looks)) = crate::rich::unstyle(text, |typed| {
                let mut n = fields.len() + made.len() + 1;
                let taken = |k: &str| fields.iter().chain(&made).any(|f| f.key == k);
                while taken(&format!("var_{n}")) {
                    n += 1;
                }
                let key = format!("var_{n}");
                made.push(Field {
                    key: key.clone(),
                    label: format!("Variable {n}"),
                    kind: FieldKind::Text,
                    default: typed.to_string(),
                    binding: None,
                    min: None,
                    max: None,
                    options: Vec::new(),
                });
                key
            }) else {
                continue;
            };
            if let LayerKind::Text { text, x, .. } = &mut self.layers[i].kind {
                *text = plain;
                for (key, v) in looks {
                    let e = x.var_styles.entry(key).or_default();
                    for (to, from) in [
                        (&mut e.bold, v.bold),
                        (&mut e.italic, v.italic),
                        (&mut e.underline, v.underline),
                        (&mut e.strike, v.strike),
                    ] {
                        if from.is_some() {
                            *to = from;
                        }
                    }
                    if v.color.is_some() {
                        e.color = v.color;
                    }
                }
            }
            self.fields.extend(made);
            changed = true;
        }
        changed
    }

    /// Layers of widgets that are gone become their replacement. Returns
    /// true if anything changed.
    pub fn upgrade_layers(&mut self) -> bool {
        let mut changed = self.unstyle_texts();
        for l in &mut self.layers {
            // The Ellipse's « Circle » is now the width / height lock of every layer.
            if let LayerKind::Ellipse { x } = &mut l.kind {
                if x.circle {
                    x.circle = false;
                    l.lock_ratio = true;
                    changed = true;
                }
            }
            // A scrolling Text is now a Ticker (horizontal) or Credits (vertical).
            if let Some(kind) = legacy_scroll(&l.kind) {
                l.kind = kind;
                changed = true;
            }
            // A timer started by hand (« Manual » / « External ») now waits for Continue (F2)
            // as every layer does (manual_start): its own start mode is At the In from then on.
            if let LayerKind::Clock { x, .. } = &mut l.kind {
                use crate::wid::timer::{StartMode, TimerMode};
                if matches!(
                    x.timer_mode,
                    Some(TimerMode::Stopwatch | TimerMode::Countdown)
                ) && matches!(x.start_mode, StartMode::Manual | StartMode::Ext)
                    && !l.manual_start
                {
                    l.manual_start = true;
                    x.start_mode = StartMode::Auto;
                    changed = true;
                }
            }
            // A typewriter wired to a data set's table takes its rows as lines, like a Text.
            if let LayerKind::Typewriter { text, .. } = &mut l.kind {
                if text.contains("__table}}") {
                    *text = text.replace("__table}}", "__lines}}");
                    changed = true;
                }
            }
            // The analog clock is now a Timer with an analog face.
            if let LayerKind::AnalogClock {
                timezone,
                seconds,
                accent,
            } = &l.kind
            {
                let color = |p: Prop, or: &str| {
                    l.props
                        .get(&p)
                        .and_then(Value::as_color)
                        .map(Color::to_hex)
                        .unwrap_or_else(|| or.into())
                };
                use crate::wid::timer::{Analog, Config, Face, Ticks, TimerMode};
                let x = Config {
                    timer_mode: Some(TimerMode::Clock),
                    face: Face::Analog,
                    analog: Analog {
                        seconds: *seconds,
                        ticks: Ticks::Hours,
                        face: color(Prop::Fill, "#0f1222e6"),
                        hands: color(Prop::Stroke, "#ffffff"),
                        accent: accent.clone(),
                        ..Analog::default()
                    },
                    ..Config::default()
                };
                l.kind = LayerKind::Clock {
                    mode: ClockMode::Time,
                    format: "HH:mm:ss".into(),
                    timezone: timezone.clone(),
                    target: String::new(),
                    font: FontStyle {
                        font_family: default_font(),
                        font_weight: default_weight(),
                        align: TextAlign::Center,
                        vertical_align: VerticalAlign::Middle,
                        uppercase: false,
                    },
                    x: Box::new(x),
                };
                changed = true;
            }
        }
        changed
    }

    /// Brings the markers to the current model: one on-air / out marker,
    /// frames inside the timeline. Templates made when the on-air point was
    /// the first pause (and the out marker could come later) get their last
    /// stop as the on-air point, and the never-played frames between it and
    /// the old out marker are cut. Returns true if anything changed.
    pub fn normalize_markers(&mut self) -> bool {
        let before = (self.markers.clone(), self.duration, self.marker_model);
        // A template lasts at least a frame (a negative or broken duration from an import or the API).
        if !self.duration.is_finite() || self.duration < 1.0 {
            self.duration = if self.duration.is_finite() {
                1.0
            } else {
                100.0
            };
        }
        if self.marker_model < MARKER_MODEL {
            self.marker_model = MARKER_MODEL;
            if let Some(out) = self.out_frame() {
                let last_stop = self
                    .markers
                    .iter()
                    .filter(|m| m.kind == MarkerKind::Pause && m.frame <= out + 1e-6)
                    .map(|m| m.frame)
                    .fold(None, |a: Option<f64>, f| Some(a.map_or(f, |a| a.max(f))));
                if let Some(stop) = last_stop {
                    self.markers.retain(|m| {
                        !(m.kind == MarkerKind::Pause && (m.frame - stop).abs() < 1e-6)
                    });
                    if out > stop + 1e-6 {
                        self.cut(stop, out);
                    }
                }
            }
            for m in &mut self.markers {
                if m.kind == MarkerKind::Pause && m.name.eq_ignore_ascii_case("on air") {
                    m.name = "Pause".into();
                }
            }
        }
        // Exactly one on-air / out marker.
        let mut seen = false;
        self.markers.retain(|m| {
            let dup = m.kind == MarkerKind::Out && seen;
            seen |= m.kind == MarkerKind::Out;
            !dup
        });
        if !seen {
            self.markers.push(Marker {
                id: crate::model::new_id(),
                frame: self.duration,
                name: "On air".into(),
                kind: MarkerKind::Out,
                resume: Resume::Manual,
            });
        }
        for m in &mut self.markers {
            m.frame = m.frame.clamp(0.0, self.duration.max(0.0));
            if m.kind == MarkerKind::Out {
                // It holds until Take out, goes out by itself after a delay, or on a trigger.
                m.name = "On air".into();
            }
        }
        self.markers.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        before != (self.markers.clone(), self.duration, self.marker_model)
    }

    /// Removes the frames between `from` and `to`: later keyframes and
    /// markers move back, keyframes inside are dropped.
    fn cut(&mut self, from: f64, to: f64) {
        let d = to - from;
        let shift = |keys: &mut Vec<Keyframe>| {
            keys.retain(|k| k.frame <= from + 1e-6 || k.frame >= to - 1e-6);
            for k in keys.iter_mut() {
                if k.frame >= to - 1e-6 {
                    k.frame -= d;
                }
            }
            keys.dedup_by(|b, a| (a.frame - b.frame).abs() < 1e-6);
        };
        for l in &mut self.layers {
            l.tracks.values_mut().for_each(shift);
            for o in l.overrides.values_mut() {
                o.tracks.values_mut().for_each(shift);
            }
        }
        for m in &mut self.markers {
            if m.frame >= to - 1e-6 {
                m.frame -= d;
            } else if m.frame > from {
                m.frame = from;
            }
        }
        self.duration = (self.duration - d).max(1.0);
    }

    pub fn layer(&self, id: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// Has something to put on air (a visible layer).
    pub fn playable(&self) -> bool {
        self.layers.iter().any(|l| l.visible)
    }

    pub fn out_frame(&self) -> Option<f64> {
        self.markers
            .iter()
            .find(|m| m.kind == MarkerKind::Out)
            .map(|m| m.frame.min(self.duration))
    }

    /// Default data built from field defaults.
    pub fn default_data(&self) -> BTreeMap<String, String> {
        self.fields
            .iter()
            .map(|f| (f.key.clone(), f.default.clone()))
            .collect()
    }
}

/// Applies what one client changed (`base` → `next`) to the rundown as it
/// is now (`current`), so two operators editing at the same time keep each
/// other's changes: items removed, edited, added or moved by this client
/// are applied; the others stay as they are.
pub fn merge_rundown(
    current: &[RundownItem],
    base: &[RundownItem],
    next: &[RundownItem],
) -> Vec<RundownItem> {
    use std::collections::HashSet;
    let ids = |v: &[RundownItem]| v.iter().map(|r| r.id.clone()).collect::<HashSet<_>>();
    let (in_base, in_next) = (ids(base), ids(next));
    let mut out: Vec<RundownItem> = current
        .iter()
        .filter(|r| !(in_base.contains(&r.id) && !in_next.contains(&r.id)))
        .cloned()
        .collect();
    // Edited by this client.
    for n in next {
        if let (Some(b), Some(o)) = (
            base.iter().find(|b| b.id == n.id),
            out.iter_mut().find(|o| o.id == n.id),
        ) {
            if b != n {
                *o = n.clone();
            }
        }
    }
    // Moved by this client: its order, others' new items after.
    let common = |v: &[RundownItem]| {
        v.iter()
            .filter(|r| in_base.contains(&r.id) && in_next.contains(&r.id))
            .map(|r| r.id.clone())
            .collect::<Vec<_>>()
    };
    if common(base) != common(next) {
        let mut ordered: Vec<RundownItem> = next
            .iter()
            .filter_map(|n| {
                if in_base.contains(&n.id) {
                    out.iter().find(|o| o.id == n.id).cloned()
                } else {
                    Some(n.clone())
                }
            })
            .collect();
        let placed = ids(&ordered);
        ordered.extend(out.into_iter().filter(|o| !placed.contains(&o.id)));
        return ordered;
    }
    // Added by this client: after the item before it.
    for (i, n) in next.iter().enumerate() {
        if in_base.contains(&n.id) || out.iter().any(|o| o.id == n.id) {
            continue;
        }
        let at = next[..i]
            .iter()
            .rev()
            .find_map(|p| out.iter().position(|o| o.id == p.id))
            .map_or(0, |p| p + 1);
        out.insert(at, n.clone());
    }
    out
}

/// An entry of the operator rundown (playlist).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RundownItem {
    pub id: String,
    pub name: String,
    pub template_id: String,
    /// Zone the item plays in ("" = the template's zone).
    #[serde(default)]
    pub zone: String,
    /// Playlist (rundown section) the item belongs to.
    #[serde(default)]
    pub playlist: String,
    /// Playout layer of pre-zone projects; replaced by `zone` on load.
    #[serde(default)]
    pub layer: u32,
    #[serde(default)]
    pub data: BTreeMap<String, String>,
    /// When the fields bound to data sources are read.
    #[serde(default, skip_serializing_if = "ReadMode::is_live")]
    pub read: ReadMode,
    /// Auto next (its playlist set to auto next): seconds on air, from the
    /// take, before the next graphic of the playlist is taken; none = the
    /// operator takes the next one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_secs: Option<f64>,
    /// Stays on air when the next graphic is taken (in another zone).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hold: bool,
}

/// When a rundown item reads its data sources.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadMode {
    /// On take, then on every change of the source, also on air.
    #[default]
    Live,
    /// Once, when the item is taken.
    Take,
    /// Only when the operator asks (the values are saved in the item).
    Manual,
}

impl ReadMode {
    pub fn is_live(&self) -> bool {
        *self == ReadMode::Live
    }
}

/// A named list of rundown items (show segment, sport block, breaking news…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    /// The graphics go on to the next one by themselves (see
    /// [`RundownItem::auto_secs`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_next: bool,
    /// After the last graphic, the first again.
    #[serde(default, rename = "loop", skip_serializing_if = "std::ops::Not::not")]
    pub looping: bool,
}

/// What happens when a graphic is taken in a zone that is already on air.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoneTransition {
    /// The graphic on air plays its out animation, then the new one comes in.
    #[default]
    Sequential,
    /// The new graphic replaces the old one immediately.
    Cut,
    /// The new graphic comes in while the one on air fades out.
    Fade,
}

/// An area of the screen that shows one template at a time (lower third,
/// bug, ticker, full screen…). Zones are on air independently of each other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    pub id: String,
    pub name: String,
    /// Playout layer, also the stacking order: higher is in front.
    pub layer: u32,
    #[serde(default = "default_zone_color")]
    pub color: String,
    #[serde(default)]
    pub transition: ZoneTransition,
    /// Where the zone sits, by format id (fractions of the frame), shown as
    /// a guide in the designer. A format without its own area uses the main
    /// one, else a default derived from the zone id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub areas: BTreeMap<String, Area>,
}

/// A rectangle in fractions of the frame (0..1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Area {
    pub const FULL: Area = Area {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };

    /// Covers the whole frame (nothing to cut).
    pub fn is_full(&self) -> bool {
        self.x <= 1e-6
            && self.y <= 1e-6
            && self.x + self.width >= 1.0 - 1e-6
            && self.y + self.height >= 1.0 - 1e-6
    }
}

impl Zone {
    /// Where the zone sits in a format: its own area, the main one, or a
    /// default (mirrors `web/src/engine/zones.ts`).
    pub fn area(&self, format: &str, portrait: bool) -> Area {
        if let Some(a) = self
            .areas
            .get(format)
            .or_else(|| self.areas.get(MAIN_FORMAT))
        {
            return *a;
        }
        let a = |x, y, width, height| Area {
            x,
            y,
            width,
            height,
        };
        let (land, port) = match self.id.as_str() {
            "lower-third" => (a(0.06, 0.7, 0.6, 0.19), a(0.05, 0.66, 0.9, 0.16)),
            "top-left" => (a(0.04, 0.05, 0.24, 0.14), a(0.05, 0.04, 0.42, 0.08)),
            "top-right" => (a(0.72, 0.05, 0.24, 0.14), a(0.53, 0.04, 0.42, 0.08)),
            "ticker" => (a(0.0, 0.89, 1.0, 0.07), a(0.0, 0.9, 1.0, 0.05)),
            "full-screen" => (Area::FULL, Area::FULL),
            _ => (a(0.25, 0.25, 0.5, 0.5), a(0.1, 0.3, 0.8, 0.3)),
        };
        if portrait {
            port
        } else {
            land
        }
    }

    /// Starting layout of a new project: one full-screen zone.
    pub fn defaults() -> Vec<Zone> {
        [("full-screen", "Full screen", 10)]
            .iter()
            .enumerate()
            .map(|(i, (id, name, layer))| Zone {
                id: (*id).into(),
                name: (*name).into(),
                layer: *layer,
                color: ZONE_COLORS[i].into(),
                transition: ZoneTransition::Sequential,
                areas: Default::default(),
            })
            .collect()
    }
}

fn default_zone_color() -> String {
    "#8fa3ff".into()
}

const ZONE_COLORS: [&str; 8] = [
    "#8fa3ff", "#ff3b5c", "#22c55e", "#f5a524", "#38bdf8", "#e879f9", "#14b8a6", "#f97316",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// A show project: the templates and the rundown of one programme,
/// designed for one video format (channels scale it to their own format).
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    #[serde(default)]
    pub templates: Vec<Template>,
    #[serde(default)]
    pub rundown: Vec<RundownItem>,
    /// Screen zones, one graphic on air at a time in each.
    #[serde(default)]
    pub zones: Vec<Zone>,
    /// Sections of the rundown, in display order.
    #[serde(default)]
    pub playlists: Vec<Playlist>,
    /// Alternative layouts (the main one is width × height).
    #[serde(default)]
    pub formats: Vec<Format>,
    /// Designer working copies not yet published. `templates` is what the
    /// operator, the outputs and automation use; "Publish" moves a draft
    /// there. A draft without a published template is a new template.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drafts: Vec<Template>,
    /// The empty template a new project starts with: removed at a publish
    /// while it still has no layer ([`Project::drop_empty_starter`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter: Option<String>,
    /// Designer aids: guides, safe areas, areas to keep clear.
    #[serde(default, skip_serializing_if = "Guides::is_default")]
    pub guides: Guides,
    /// The Designer's order of the templates (ids); the operator keeps its own.
    /// Templates missing from it follow, in their order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub design_order: Vec<String>,
    /// Language of the texts (hyphenation rules, French spaces): "fr", "en"…
    #[serde(default = "default_language")]
    pub language: String,
    /// Length of a new template (its IN and OUT), seconds; each one is then
    /// set on its own in the timeline. None: one second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_secs: Option<f64>,
    /// The project's own palette (the shared ones are in the Library).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<NamedColor>,
}

/// A colour of a palette: the layers using its value follow it when it
/// changes (see [`crate::palette`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedColor {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// `#rrggbb` or `#rrggbbaa`.
    pub value: String,
}

/// A palette shared by every project (Library › Colours).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub colors: Vec<NamedColor>,
}

impl Project {
    /// The project format closest in aspect ratio to a `width` × `height`
    /// output: (format id, design width, design height).
    pub fn format_for(&self, width: u32, height: u32) -> (&str, u32, u32) {
        let target = (width.max(1) as f64 / height.max(1) as f64).ln();
        let main = (MAIN_FORMAT, self.width, self.height);
        std::iter::once(main)
            .chain(
                self.formats
                    .iter()
                    .map(|f| (f.id.as_str(), f.width, f.height)),
            )
            .min_by(|a, b| {
                let d = |f: &(&str, u32, u32)| {
                    ((f.1.max(1) as f64 / f.2.max(1) as f64).ln() - target).abs()
                };
                d(a).total_cmp(&d(b))
            })
            .unwrap_or(main)
    }

    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    pub fn zone_at(&self, layer: u32) -> Option<&Zone> {
        self.zones.iter().find(|z| z.layer == layer)
    }

    /// Zone of a rundown item: its own, else its template's.
    pub fn item_zone(&self, item: &RundownItem) -> Option<&Zone> {
        self.zone(&item.zone).or_else(|| {
            self.template(&item.template_id)
                .and_then(|t| self.zone(&t.zone))
        })
    }

    /// Gives every rundown item and template a zone. Pre-zone projects get
    /// one zone per playout layer they used. Returns true if anything changed.
    pub fn ensure_zones(&mut self) -> bool {
        // Also brings the templates to the current marker model.
        let mut changed = false;
        for t in self.templates.iter_mut().chain(self.drafts.iter_mut()) {
            changed |= t.normalize_markers();
            changed |= t.upgrade_layers();
        }
        // "Hold anyway" is gone: those graphics now wait for the operator.
        for r in self.rundown.iter_mut().filter(|r| r.hold) {
            r.hold = false;
            r.auto_secs = None;
            changed = true;
        }
        for i in 0..self.rundown.len() {
            if self.zone(&self.rundown[i].zone).is_some() {
                continue;
            }
            // No zone given ("the template's zone"): the template's, not a new one.
            if self.rundown[i].zone.is_empty() {
                let tz = self
                    .template(&self.rundown[i].template_id)
                    .map(|t| t.zone.clone())
                    .filter(|z| self.zone(z).is_some());
                if let Some(tz) = tz {
                    self.rundown[i].zone = tz;
                    changed = true;
                    continue;
                }
            }
            let layer = self.rundown[i].layer;
            let id = match self.zone_at(layer) {
                Some(z) => z.id.clone(),
                None => {
                    let z = Zone {
                        id: format!("layer-{layer}"),
                        name: format!("Calque {}", i64::from(layer) + 1),
                        layer,
                        color: ZONE_COLORS[self.zones.len() % ZONE_COLORS.len()].into(),
                        transition: ZoneTransition::Sequential,
                        areas: Default::default(),
                    };
                    let id = z.id.clone();
                    self.zones.push(z);
                    id
                }
            };
            self.rundown[i].zone = id;
            changed = true;
        }
        if self.zones.is_empty() {
            self.zones.push(Zone {
                id: "main".into(),
                name: "Principal".into(),
                layer: 10,
                color: ZONE_COLORS[0].into(),
                transition: ZoneTransition::Sequential,
                areas: Default::default(),
            });
            changed = true;
        }
        for t in 0..self.templates.len() {
            if self.zone(&self.templates[t].zone).is_some() {
                continue;
            }
            let tid = self.templates[t].id.clone();
            let zone = self
                .rundown
                .iter()
                .find(|r| r.template_id == tid)
                .map(|r| r.zone.clone())
                .unwrap_or_else(|| self.zones[0].id.clone());
            self.templates[t].zone = zone;
            changed = true;
        }
        // Keep the legacy layer number in step for older clients.
        for i in 0..self.rundown.len() {
            if let Some(l) = self.zone(&self.rundown[i].zone).map(|z| z.layer) {
                if self.rundown[i].layer != l {
                    self.rundown[i].layer = l;
                    changed = true;
                }
            }
        }
        self.zones.sort_by_key(|z| z.layer);
        changed | self.ensure_playlists()
    }

    /// At least one playlist, and every rundown item in an existing one.
    pub fn ensure_playlists(&mut self) -> bool {
        let mut changed = false;
        if self.playlists.is_empty() {
            self.playlists.push(Playlist {
                id: "main".into(),
                name: "Principal".into(),
                auto_next: Default::default(),
                looping: Default::default(),
            });
            changed = true;
        }
        let first = self.playlists[0].id.clone();
        for r in &mut self.rundown {
            if !self.playlists.iter().any(|p| p.id == r.playlist) {
                r.playlist = first.clone();
                changed = true;
            }
        }
        changed
    }

    pub fn validate_zones(zones: &[Zone]) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        let mut layers = std::collections::HashSet::new();
        for z in zones {
            let valid = !z.id.is_empty()
                && z.id.len() <= 64
                && z.id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !valid || !ids.insert(z.id.as_str()) {
                return Err(format!(
                    "zone ids must be unique and use letters, digits, - or _ (`{}`)",
                    z.id
                ));
            }
            if z.name.trim().is_empty() {
                return Err("every zone needs a name".into());
            }
            if !layers.insert(z.layer) {
                return Err(format!("two zones share the stacking position {}", z.layer));
            }
        }
        if zones.is_empty() {
            return Err("a project needs at least one zone".into());
        }
        Ok(())
    }

    /// The published template (what goes on air).
    pub fn template(&self, id: &str) -> Option<&Template> {
        self.templates.iter().find(|t| t.id == id)
    }

    /// The template as the designer sees it: its draft, else the published one.
    pub fn draft(&self, id: &str) -> Option<&Template> {
        self.drafts
            .iter()
            .find(|t| t.id == id)
            .or_else(|| self.template(id))
    }

    /// Saves a designer edit as a draft (nothing changes on air).
    pub fn save_draft(&mut self, t: Template) {
        // Saved as it is published: nothing to publish.
        if self.template(&t.id).is_some_and(|p| {
            Template {
                revision: p.revision,
                ..t.clone()
            } == *p
        }) {
            self.drafts.retain(|x| x.id != t.id);
            return;
        }
        match self.drafts.iter_mut().find(|x| x.id == t.id) {
            Some(d) => *d = t,
            None => self.drafts.push(t),
        }
    }

    /// Publishes a template's draft: the operator and the outputs use it
    /// from now on. Returns the new revision, or None without a draft.
    pub fn publish(&mut self, id: &str) -> Option<u32> {
        let i = self.drafts.iter().position(|t| t.id == id)?;
        let mut t = self.drafts.remove(i);
        let rev = self.template(id).map_or(0, |p| p.revision) + 1;
        t.revision = rev;
        match self.templates.iter_mut().find(|x| x.id == id) {
            Some(p) => *p = t,
            None => self.templates.push(t),
        }
        Some(rev)
    }

    /// The empty template the project started with goes once another one is
    /// published, if it still has no layer (its draft neither); once it has
    /// layers it is an ordinary template. Returns the id removed.
    pub fn drop_empty_starter(&mut self) -> Option<String> {
        let id = self.starter.clone()?;
        let versions: Vec<&Template> = self
            .templates
            .iter()
            .chain(self.drafts.iter())
            .filter(|t| t.id == id)
            .collect();
        if versions.is_empty() {
            self.starter = None;
            return None;
        }
        if versions.iter().any(|t| !t.layers.is_empty()) {
            self.starter = None;
            return None;
        }
        // Never the last template of the project.
        if !self.design_templates().any(|t| t.id != id) {
            return None;
        }
        self.templates.retain(|t| t.id != id);
        self.drafts.retain(|t| t.id != id);
        self.rundown.retain(|r| r.template_id != id);
        self.starter = None;
        Some(id)
    }

    /// Every template the designer lists: published ones (or their draft),
    /// then new drafts.
    pub fn design_templates(&self) -> impl Iterator<Item = &Template> {
        self.templates
            .iter()
            .map(|t| self.draft(&t.id).unwrap_or(t))
            .chain(
                self.drafts
                    .iter()
                    .filter(|d| self.template(&d.id).is_none()),
            )
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_analog_clock_becomes_a_timer_with_an_analog_face() {
        let mut t = crate::sample::analog_clock();
        let LayerKind::Clock { x, timezone, .. } = &t.layers[0].kind else {
            panic!("not a timer: {:?}", t.layers[0].kind);
        };
        assert_eq!(x.face, crate::wid::timer::Face::Analog);
        assert_eq!(x.analog.face, "#0f1222e6");
        assert_eq!(x.analog.hands, "#ffffff");
        assert!(x.analog.seconds && timezone.is_empty());
        assert!(!t.upgrade_layers());
    }

    #[test]
    fn legacy_markers_become_one_on_air_marker() {
        // Before: on air at 40 (first pause), out animation from 100.
        let mut t = crate::sample::lower_third();
        assert_eq!(t.marker_model, 0);
        let duration = t.duration;
        let last_key = t
            .layers
            .iter()
            .flat_map(|l| l.tracks.values().flatten())
            .map(|k| k.frame)
            .fold(0.0, f64::max);
        assert!(t.normalize_markers());
        let kinds: Vec<_> = t.markers.iter().map(|m| (m.kind, m.frame)).collect();
        assert_eq!(kinds, vec![(MarkerKind::Out, 40.0)]);
        // The 60 frames never played (40 → 100) are cut.
        assert_eq!(t.duration, duration - 60.0);
        let new_last = t
            .layers
            .iter()
            .flat_map(|l| l.tracks.values().flatten())
            .map(|k| k.frame)
            .fold(0.0, f64::max);
        assert_eq!(new_last, last_key - 60.0);
        assert_eq!(t.hold_frame(), 40.0);
        // Idempotent.
        assert!(!t.normalize_markers());
    }

    #[test]
    fn pauses_before_on_air_keep_their_role() {
        let mut t = crate::sample::lower_third();
        t.marker_model = MARKER_MODEL;
        t.markers.retain(|m| m.kind == MarkerKind::Out);
        t.markers.push(Marker {
            id: "p".into(),
            frame: 10.0,
            name: "Pause".into(),
            kind: MarkerKind::Pause,
            resume: Resume::Manual,
        });
        t.normalize_markers();
        assert_eq!(t.next_stop_after(0.0), 10.0);
        assert_eq!(t.next_stop_after(10.0), 100.0);
        assert_eq!(t.hold_frame(), 100.0);
    }
}

/// Sets `path` (dotted, numbers index lists) in `root` to `raw`, read as
/// the type of the value already there (number, true/false, text, JSON).
fn set_setting(root: &mut serde_json::Value, path: &str, raw: &str) {
    use serde_json::Value as J;
    let mut cur = root;
    let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    let Some((last, head)) = parts.split_last() else {
        return;
    };
    for part in head {
        cur = match cur {
            J::Array(a) => match part.parse::<usize>().ok().and_then(|i| a.get_mut(i)) {
                Some(v) => v,
                None => return,
            },
            J::Object(o) => o
                .entry(part.to_string())
                .or_insert_with(|| J::Object(Default::default())),
            _ => return,
        };
    }
    let slot = match cur {
        J::Array(a) => match last.parse::<usize>().ok().and_then(|i| a.get_mut(i)) {
            Some(v) => v,
            None => return,
        },
        J::Object(o) => o.entry(last.to_string()).or_insert(J::Null),
        _ => return,
    };
    let t = raw.trim();
    let new = match slot {
        // A whole number stays an integer (counts, rows, weights are integers in the model).
        J::Number(_) => t.parse::<f64>().ok().and_then(|f| {
            if f.fract() == 0.0 && f.abs() < 9.0e15 {
                Some(J::Number(serde_json::Number::from(f as i64)))
            } else {
                serde_json::Number::from_f64(f).map(J::Number)
            }
        }),
        J::Bool(_) => match t {
            "true" | "1" | "yes" | "on" => Some(J::Bool(true)),
            "false" | "0" | "no" | "off" | "" => Some(J::Bool(false)),
            _ => None,
        },
        J::String(_) => Some(J::String(raw.to_string())),
        J::Array(_) | J::Object(_) => serde_json::from_str(t).ok(),
        J::Null => Some(
            serde_json::from_str::<J>(t)
                .ok()
                .filter(|v| v.is_number() || v.is_boolean())
                .unwrap_or_else(|| J::String(raw.to_string())),
        ),
    };
    if let Some(v) = new {
        *slot = v;
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn item(id: &str, name: &str) -> RundownItem {
        serde_json::from_value(serde_json::json!({"id": id, "name": name, "templateId": "t"}))
            .unwrap()
    }
    fn names(v: &[RundownItem]) -> Vec<String> {
        v.iter().map(|r| format!("{}:{}", r.id, r.name)).collect()
    }

    #[test]
    fn two_operators_keep_each_others_changes() {
        let base = vec![item("a", "A"), item("b", "B"), item("c", "C")];
        // Someone else renamed b and added d meanwhile.
        let current = vec![
            item("a", "A"),
            item("b", "B2"),
            item("c", "C"),
            item("d", "D"),
        ];
        // This client renamed a.
        let next = vec![item("a", "A1"), item("b", "B"), item("c", "C")];
        assert_eq!(
            names(&merge_rundown(&current, &base, &next)),
            ["a:A1", "b:B2", "c:C", "d:D"]
        );
        // This client deleted c and added e after a.
        let next = vec![item("a", "A"), item("e", "E"), item("b", "B")];
        assert_eq!(
            names(&merge_rundown(&current, &base, &next)),
            ["a:A", "e:E", "b:B2", "d:D"]
        );
        // This client moved c first.
        let next = vec![item("c", "C"), item("a", "A"), item("b", "B")];
        assert_eq!(
            names(&merge_rundown(&current, &base, &next)),
            ["c:C", "a:A", "b:B2", "d:D"]
        );
        // An item deleted by someone else is not brought back by an edit.
        let current = vec![item("a", "A")];
        let next = vec![item("a", "A"), item("b", "B9"), item("c", "C")];
        assert_eq!(names(&merge_rundown(&current, &base, &next)), ["a:A"]);
    }
}

#[cfg(test)]
mod setting_tests {
    use super::*;

    #[test]
    fn exposed_settings_follow_the_data() {
        let mut j = serde_json::json!({"mask": "rounded", "radius": 28, "crop": {"l": 0.1}, "on": false, "teams": [{"logo": "a.png"}]});
        set_setting(&mut j, "mask", "circle");
        set_setting(&mut j, "radius", "40");
        set_setting(&mut j, "crop.l", "0.25");
        set_setting(&mut j, "on", "true");
        set_setting(&mut j, "teams.0.logo", "b.png");
        set_setting(&mut j, "radius", "oops");
        set_setting(&mut j, "missing.deep", "3");
        assert_eq!(
            j,
            serde_json::json!({"mask": "circle", "radius": 40, "crop": {"l": 0.25}, "on": true, "teams": [{"logo": "b.png"}], "missing": {"deep": 3}})
        );
    }

    #[test]
    fn a_template_takes_the_operator_settings() {
        let p = crate::sample::demo_project();
        let is_text = |l: &Layer| matches!(l.kind, LayerKind::Text { .. });
        let mut t = p
            .templates
            .iter()
            .find(|t| t.layers.iter().any(is_text))
            .unwrap()
            .clone();
        let i = t.layers.iter().position(is_text).unwrap();
        t.layers[i]
            .setting_bindings
            .insert("verticalAlign".into(), "va".into());
        let data = BTreeMap::from([("va".to_string(), "bottom".to_string())]);
        let out = t.with_settings(&data);
        let LayerKind::Text { vertical_align, .. } = &out.layers[i].kind else {
            panic!()
        };
        assert_eq!(serde_json::to_value(vertical_align).unwrap(), "bottom");
        // Without the field in the data, the template is untouched.
        assert!(matches!(
            t.with_settings(&BTreeMap::new()),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn a_lottie_group_keeps_its_file() {
        for file in ["", "/assets/p/anim.json"] {
            let l: Layer = serde_json::from_value(serde_json::json!({
                "id": "g", "name": "Lottie", "type": "group", "clip": true, "lottie": file
            }))
            .unwrap();
            let back = serde_json::to_value(&l).unwrap();
            assert_eq!(
                back["lottie"], file,
                "the Fichier row needs it after a save"
            );
            assert_eq!(back["clip"], true);
        }
        // A plain group has none.
        let g: Layer = serde_json::from_str(r#"{"id":"g","name":"G","type":"group"}"#).unwrap();
        assert!(serde_json::to_value(&g).unwrap().get("lottie").is_none());
    }

    #[test]
    fn ratio_lock_and_drop_shadow_are_kept() {
        let l: Layer = serde_json::from_str(
            r#"{"id":"a","name":"A","type":"rect","lockRatio":true,"dropShadow":true,"props":{"shadowX":3.0,"shadowColor":[1.0,0.0,0.0,0.5]}}"#,
        )
        .unwrap();
        assert!(l.lock_ratio && l.drop_shadow);
        let v = serde_json::to_value(&l).unwrap();
        assert_eq!(v["lockRatio"], true);
        assert_eq!(v["dropShadow"], true);
        assert_eq!(v["props"]["shadowX"], 3.0);
        let r = crate::anim::resolve_layer(&l, 0.0, &Default::default(), 0.0);
        assert_eq!(r.num(Prop::ShadowX), 3.0);
        assert_eq!(r.num(Prop::ShadowY), 4.0);
        assert_eq!(r.num(Prop::ShadowBlur), 12.0);
        assert_eq!(r.color(Prop::ShadowColor), Color([1.0, 0.0, 0.0, 0.5]));
        // Off: not written.
        let plain: Layer = serde_json::from_str(r#"{"id":"a","name":"A","type":"rect"}"#).unwrap();
        let v = serde_json::to_value(&plain).unwrap();
        assert!(v.get("lockRatio").is_none() && v.get("dropShadow").is_none());
    }

    #[test]
    fn an_ellipse_circle_becomes_the_ratio_lock() {
        let mut t = Template::blank("T", "", 50.0);
        t.layers.push(
            serde_json::from_str(r#"{"id":"e","name":"E","type":"ellipse","circle":true}"#)
                .unwrap(),
        );
        assert!(t.upgrade_layers());
        assert!(t.layers[0].lock_ratio);
        assert!(!t.upgrade_layers());
    }

    #[test]
    fn the_on_air_marker_keeps_its_automatic_out() {
        let mut t = Template::blank("T", "", 50.0);
        t.markers.retain(|m| m.kind != MarkerKind::Out);
        t.markers.push(Marker {
            id: "o".into(),
            frame: 30.0,
            name: "On air".into(),
            kind: MarkerKind::Out,
            resume: Resume::After { seconds: 3.0 },
        });
        t.marker_model = MARKER_MODEL;
        t.normalize_markers();
        let out = t
            .markers
            .iter()
            .find(|m| m.kind == MarkerKind::Out)
            .unwrap();
        assert_eq!(out.resume, Resume::After { seconds: 3.0 });
    }

    #[test]
    fn a_typewriter_on_a_table_reads_its_rows_as_lines() {
        let mut t = Template::blank("T", "", 50.0);
        t.layers.push(
            serde_json::from_str(
                r#"{"id":"w","name":"W","type":"typewriter","text":"{{news__table}}"}"#,
            )
            .unwrap(),
        );
        assert!(t.upgrade_layers());
        let LayerKind::Typewriter { text, .. } = &t.layers[0].kind else {
            panic!("not a typewriter");
        };
        assert_eq!(text, "{{news__lines}}");
        assert!(!t.upgrade_layers());
    }

    #[test]
    fn the_empty_starter_template_goes_at_a_publish() {
        let mut p = crate::sample::demo_project();
        p.templates.clear();
        p.drafts.clear();
        p.rundown.clear();
        let starter = Template::blank("Template 1", "", 50.0);
        let sid = starter.id.clone();
        p.templates.push(starter);
        p.starter = Some(sid.clone());
        // Alone: it stays.
        assert_eq!(p.drop_empty_starter(), None);
        assert_eq!(p.starter.as_deref(), Some(sid.as_str()));
        // Another template published while it is still empty: it goes.
        let mut other = Template::blank("Lower third", "", 50.0);
        other
            .layers
            .push(serde_json::from_str(r#"{"id":"a","name":"A","type":"rect"}"#).unwrap());
        let oid = other.id.clone();
        p.save_draft(other);
        p.publish(&oid);
        assert_eq!(p.drop_empty_starter(), Some(sid.clone()));
        assert!(p.template(&sid).is_none() && p.starter.is_none());
        assert!(p.template(&oid).is_some());
        // Given layers (even only in its draft), it is kept for good.
        let mut q = crate::sample::demo_project();
        let mut s = Template::blank("Template 1", "", 50.0);
        q.starter = Some(s.id.clone());
        q.templates.push(s.clone());
        s.layers
            .push(serde_json::from_str(r#"{"id":"b","name":"B","type":"rect"}"#).unwrap());
        q.save_draft(s.clone());
        assert_eq!(q.drop_empty_starter(), None);
        assert!(q.starter.is_none() && q.template(&s.id).is_some());
    }

    #[test]
    fn scrolling_texts_become_a_ticker_or_credits() {
        let mut t = Template::blank("T", "", 50.0);
        let text = |id: &str, scroll: &str, body: &str| -> Layer {
            serde_json::from_value(serde_json::json!({
                "id": id, "name": id, "type": "text", "text": body, "fontFamily": "Oswald",
                "scroll": scroll, "speed": 90.0, "separator": " | "
            }))
            .unwrap()
        };
        t.layers.push(text("h", "horizontal", "One\nTwo"));
        t.layers
            .push(text("v", "vertical", "Directed by | Jane\nThanks"));
        t.layers.push(text("w", "vertical", "{{roll}}"));
        t.layers.push(text("f", "none", "Fixed"));
        assert!(t.upgrade_layers());
        let LayerKind::Ticker {
            text,
            separator,
            speed,
            font,
            ..
        } = &t.layers[0].kind
        else {
            panic!("{:?}", t.layers[0].kind)
        };
        assert_eq!(
            (text.as_str(), separator.as_str(), *speed),
            ("One\nTwo", " | ", 90.0)
        );
        assert_eq!(font.font_family, "Oswald");
        let LayerKind::Credits { x } = &t.layers[1].kind else {
            panic!("{:?}", t.layers[1].kind)
        };
        assert_eq!(x.entries.len(), 2);
        assert_eq!((x.pxs, x.font.as_str()), (90.0, "Oswald"));
        assert_eq!(x.speed_mode, crate::wid::credits::SpeedMode::Pxs);
        let LayerKind::Credits { x } = &t.layers[2].kind else {
            panic!()
        };
        assert_eq!(x.link, "{{roll}}");
        assert!(matches!(t.layers[3].kind, LayerKind::Text { .. }));
        assert!(!t.upgrade_layers());
    }
}

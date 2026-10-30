//! Import of SVG files: see [`super`].
//!
//! One template, every element of the document a layer inside a root group
//! the size of the SVG (the import modal places it on the frame):
//! rectangles, ellipses, paths (pens), texts (each a variable of the
//! template), pictures and groups (`g`, `use` / `symbol`, nested `svg`).
//! Styles come from the presentation attributes, `style=""` and a simple
//! `<style>` sheet; transforms become position, rotation and scale. What a
//! layer cannot draw (skews, masks, clipping paths, patterns, compound
//! paths…) is rasterised with resvg into a picture of just that element.
//! SMIL animations (`animate`, `set`, `animateTransform`, straight
//! `animateMotion`) become keyframes; the end of the last one is the on-air
//! point.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use base64::Engine as _;
use cg_core::anim::default_value;
use cg_core::wid::pen::{Arrow, HandleMode, PenPoint, PointType};
use cg_core::wid::shape::{
    Cap, Dash, FillKind, FillRule, FillStyle, GradientStop, Join, StrokePosition, StrokeStyle,
};
use cg_core::{
    new_id, Color, Easing, Field, FieldKind, ImageFit, Keyframe, Layer, LayerKind, MarkerKind,
    Prop, Template, TextAlign, TextDecoration, Value, VerticalAlign,
};
use resvg::{tiny_skia, usvg};
use roxmltree::{Document, Node, NodeId};

use super::{Comp, Converted, Ctx, Format, Ignored, LayerReport, Note, Status};

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const FPS: f64 = 25.0;
/// Limits against hostile files.
const MAX_BYTES: usize = 128 << 20;
const MAX_NODES: u32 = 500_000;
const MAX_LAYERS: usize = 3000;
const MAX_DEPTH: u32 = 48;
const MAX_USES: usize = 8;
const MAX_RASTERS: usize = 300;
const MAX_RASTER_PX: f64 = 4096.0 * 4096.0;
const MAX_SIZE: f64 = 16384.0;
const MAX_ANIMS: usize = 5000;
/// Animations end within an hour.
const MAX_TIME: f64 = 3600.0;

/// Analyses an SVG (or SVGZ) file.
pub fn analyze(bytes: &[u8], ctx: &Ctx) -> Result<Converted, String> {
    if bytes.len() > MAX_BYTES {
        return Err("this SVG file is too large".into());
    }
    let raw = if bytes.starts_with(&[0x1f, 0x8b]) {
        use std::io::Read;
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|_| "not a readable SVG file (bad compression)")?;
        if out.len() > MAX_BYTES {
            return Err("this SVG file is too large".into());
        }
        out
    } else {
        bytes.to_vec()
    };
    let text = std::str::from_utf8(&raw).map_err(|_| "not a readable SVG file (not UTF-8 text)")?;
    let text = text.trim_start_matches('\u{feff}');
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: MAX_NODES,
    };
    let doc = Document::parse_with_options(text, opts)
        .map_err(|e| format!("not a readable SVG file: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err("not an SVG file (no <svg> element)".into());
    }
    let mut cv = Cv::new(&doc, ctx);
    Ok(cv.run(root, text.as_bytes().to_vec()))
}

// ---------------------------------------------------------------- matrices --

/// Affine matrix `[a, b, c, d, e, f]`: x' = a x + c y + e, y' = b x + d y + f.
type Mat = [f64; 6];
const ID: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `m · n` (n applied first).
fn mul(m: &Mat, n: &Mat) -> Mat {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

fn pt(m: &Mat, (x, y): (f64, f64)) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn translate(x: f64, y: f64) -> Mat {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

fn scale(x: f64, y: f64) -> Mat {
    [x, 0.0, 0.0, y, 0.0, 0.0]
}

fn rotate(deg: f64) -> Mat {
    let (s, c) = deg.to_radians().sin_cos();
    [c, s, -s, c, 0.0, 0.0]
}

fn det(m: &Mat) -> f64 {
    m[0] * m[3] - m[1] * m[2]
}

/// A matrix as rotation × scale (× shear).
struct Dec {
    rot: f64,
    sx: f64,
    sy: f64,
    skew: bool,
}

fn decompose(m: &Mat) -> Dec {
    let (a, b, c, d) = (m[0], m[1], m[2], m[3]);
    let sx = a.hypot(b);
    if sx < 1e-9 {
        return Dec {
            rot: 0.0,
            sx: 0.0,
            sy: c.hypot(d),
            skew: false,
        };
    }
    let sy = (a * d - b * c) / sx;
    let shear = (a * c + b * d) / sx;
    Dec {
        rot: b.atan2(a).to_degrees(),
        sx,
        sy,
        skew: shear.abs() > 1e-3 * sx.max(sy.abs()),
    }
}

/// `r` plus whole turns, nearest to `hint`.
fn unwrap_deg(r: f64, hint: f64) -> f64 {
    r + 360.0 * ((hint - r) / 360.0).round()
}

/// Rounded to 4 decimals; never infinite or NaN (they would not survive JSON).
fn round4(v: f64) -> f64 {
    if v.is_finite() {
        (v.clamp(-1e9, 1e9) * 10000.0).round() / 10000.0
    } else {
        0.0
    }
}

/// As f32, never infinite or NaN.
fn fin32(v: f64) -> f32 {
    if v.is_finite() {
        v.clamp(-1e9, 1e9) as f32
    } else {
        0.0
    }
}

// ------------------------------------------------------------------ values --

/// A number at the start of `s`, and its length.
fn scan_number(s: &str) -> Option<(f64, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i > start;
    if i < b.len() && b[i] == b'.' {
        let f = i + 1;
        let mut j = f;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > f || digits {
            digits |= j > f;
            i = j;
        }
    }
    if !digits {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let e = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > e {
            i = j;
        }
    }
    s[..i]
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| (v, i))
}

/// The numbers of a list ("10,20 -5e2.5"), up to the first error.
fn numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut rest = s;
    while out.len() < 1_000_000 {
        let t = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        match scan_number(t) {
            Some((v, n)) => {
                out.push(v);
                rest = &t[n..];
            }
            None => break,
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
    Other,
}

fn axis_of(attr: &str) -> Axis {
    match attr {
        "x" | "cx" | "width" | "rx" | "x1" | "x2" | "dx" | "fx" => Axis::X,
        "y" | "cy" | "height" | "ry" | "y1" | "y2" | "dy" | "fy" => Axis::Y,
        _ => Axis::Other,
    }
}

/// A length in user units (`vp`: the viewport, for percentages).
fn length(s: &str, axis: Axis, vp: (f64, f64)) -> Option<f64> {
    let s = s.trim();
    let (v, n) = scan_number(s)?;
    let k = match s[n..].trim().to_ascii_lowercase().as_str() {
        "" | "px" => 1.0,
        "pt" => 4.0 / 3.0,
        "pc" => 16.0,
        "mm" => 96.0 / 25.4,
        "cm" => 96.0 / 2.54,
        "q" => 96.0 / 101.6,
        "in" => 96.0,
        "em" | "rem" => 16.0,
        "ex" => 8.0,
        "%" => {
            (match axis {
                Axis::X => vp.0,
                Axis::Y => vp.1,
                Axis::Other => ((vp.0 * vp.0 + vp.1 * vp.1) / 2.0).sqrt(),
            }) / 100.0
        }
        _ => return None,
    };
    Some(v * k)
}

/// 0..1 from a number or a percentage.
fn ratio(s: &str) -> Option<f64> {
    let s = s.trim();
    let (v, n) = scan_number(s)?;
    let v = if s[n..].trim() == "%" { v / 100.0 } else { v };
    Some(v.clamp(0.0, 1.0))
}

/// A clock value in seconds ("2s", "500ms", "0:01.5", "1min", "3").
fn clock(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() > 3 {
            return None;
        }
        let mut t = 0.0;
        for p in parts {
            t = t * 60.0 + p.trim().parse::<f64>().ok()?;
        }
        return t.is_finite().then_some(t);
    }
    let (v, n) = scan_number(s)?;
    let k = match s[n..].trim() {
        "" | "s" => 1.0,
        "ms" => 0.001,
        "min" => 60.0,
        "h" => 3600.0,
        _ => return None,
    };
    Some(v * k)
}

/// A `transform` list; None when it does not parse (then ignored).
fn transform(s: &str) -> Option<Mat> {
    let mut m = ID;
    let mut rest = s.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let close = rest[open..].find(')')? + open;
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let v = numbers(&rest[open + 1..close]);
        let g = |i: usize| v.get(i).copied();
        let t = match (name, v.len()) {
            ("matrix", 6) => [v[0], v[1], v[2], v[3], v[4], v[5]],
            ("translate", 1 | 2) => translate(v[0], g(1).unwrap_or(0.0)),
            ("scale", 1 | 2) => scale(v[0], g(1).unwrap_or(v[0])),
            ("rotate", 1) => rotate(v[0]),
            ("rotate", 3) => mul(
                &mul(&translate(v[1], v[2]), &rotate(v[0])),
                &translate(-v[1], -v[2]),
            ),
            ("skewX", 1) => [1.0, 0.0, v[0].to_radians().tan(), 1.0, 0.0, 0.0],
            ("skewY", 1) => [1.0, v[0].to_radians().tan(), 0.0, 1.0, 0.0, 0.0],
            _ => return None,
        };
        m = mul(&m, &t);
        rest = rest[close + 1..].trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    }
    Some(m)
}

/// The id of `url(#id)` / `#id`.
fn url_id(s: &str) -> Option<&str> {
    let s = s.trim();
    let inner = match s.strip_prefix("url(") {
        Some(r) => r[..r.find(')')?].trim().trim_matches(['"', '\'']),
        None => s,
    };
    inner.strip_prefix('#').filter(|i| !i.is_empty())
}

fn href<'a>(n: Node<'a, '_>) -> Option<&'a str> {
    n.attribute((XLINK_NS, "href")).or(n.attribute("href"))
}

fn xml_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// Lower case letters, digits and `sep`, at most 40 characters.
fn slug(s: &str, sep: char) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with(sep) {
            out.push(sep);
        }
        if out.len() >= 40 {
            break;
        }
    }
    out.trim_end_matches(sep).to_string()
}

fn with_alpha(c: Color, a: f64) -> Color {
    let [r, g, b, o] = c.0;
    Color([r, g, b, (o as f64 * a).clamp(0.0, 1.0) as f32])
}

// --------------------------------------------------------------------- CSS --

/// A simple selector: `type`, `.class`, `#id` and their combinations.
#[derive(Debug, Clone, Default)]
struct Sel {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

impl Sel {
    fn parse(s: &str) -> Option<Sel> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut sel = Sel::default();
        let name_end = s.find(['.', '#']).unwrap_or(s.len());
        let tag = &s[..name_end];
        let ident = |t: &str| {
            !t.is_empty()
                && t.chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        };
        if tag == "*" {
        } else if !tag.is_empty() {
            if !ident(tag) {
                return None;
            }
            sel.tag = Some(tag.to_string());
        }
        let mut rest = &s[name_end..];
        while !rest.is_empty() {
            let kind = rest.as_bytes()[0];
            let end = rest[1..].find(['.', '#']).map_or(rest.len(), |i| i + 1);
            let name = &rest[1..end];
            if !ident(name) {
                return None;
            }
            if kind == b'#' {
                sel.id = Some(name.to_string());
            } else {
                sel.classes.push(name.to_string());
            }
            rest = &rest[end..];
        }
        Some(sel)
    }

    fn specificity(&self) -> (u32, u32, u32) {
        (
            self.id.is_some() as u32,
            self.classes.len() as u32,
            self.tag.is_some() as u32,
        )
    }

    fn matches(&self, n: Node) -> bool {
        if self
            .tag
            .as_deref()
            .is_some_and(|t| t != n.tag_name().name())
        {
            return false;
        }
        if self
            .id
            .as_deref()
            .is_some_and(|i| n.attribute("id") != Some(i))
        {
            return false;
        }
        let classes: Vec<&str> = n
            .attribute("class")
            .unwrap_or("")
            .split_whitespace()
            .collect();
        self.classes.iter().all(|c| classes.contains(&c.as_str()))
    }
}

struct Rule {
    sel: Sel,
    spec: (u32, u32, u32),
    order: usize,
    decls: Vec<(String, String, bool)>,
}

/// `name: value; …` with `!important` flags.
fn parse_decls(s: &str) -> Vec<(String, String, bool)> {
    s.split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            let k = k.trim().to_ascii_lowercase();
            let mut v = v.trim();
            let important = v.to_ascii_lowercase().ends_with("!important");
            if important {
                v = v[..v.len() - "!important".len()].trim();
            }
            (!k.is_empty() && !v.is_empty()).then(|| (k, v.to_string(), important))
        })
        .collect()
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = match rest[i + 2..].find("*/") {
            Some(j) => &rest[i + 2 + j + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// Rules of a style sheet; `@keyframes` names; selectors not supported.
fn parse_sheet(css: &str, rules: &mut Vec<Rule>, keyframes: &mut Vec<String>, skipped: &mut usize) {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let head = rest[..open].trim();
        let mut depth = 0;
        let mut close = None;
        for (i, c) in rest[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { break };
        let body = &rest[open + 1..close];
        if let Some(at) = head.strip_prefix('@') {
            let word = at.split_whitespace().next().unwrap_or("");
            if word.ends_with("keyframes") {
                let name = at[word.len()..].trim();
                keyframes.push(if name.is_empty() { "keyframes" } else { name }.to_string());
            } else if word == "media" || word == "supports" {
                parse_sheet(body, rules, keyframes, skipped);
            }
        } else {
            let decls = parse_decls(body);
            for s in head.split(',') {
                match Sel::parse(s) {
                    Some(sel) => rules.push(Rule {
                        spec: sel.specificity(),
                        sel,
                        order: rules.len(),
                        decls: decls.clone(),
                    }),
                    None => *skipped += 1,
                }
            }
        }
        rest = &rest[close + 1..];
    }
}

/// CSS properties read, and whether children inherit them.
const PROPS: &[(&str, bool)] = &[
    ("fill", true),
    ("fill-opacity", true),
    ("fill-rule", true),
    ("stroke", true),
    ("stroke-width", true),
    ("stroke-opacity", true),
    ("stroke-linecap", true),
    ("stroke-linejoin", true),
    ("stroke-dasharray", true),
    ("visibility", true),
    ("color", true),
    ("font-family", true),
    ("font-size", true),
    ("font-weight", true),
    ("font-style", true),
    ("text-anchor", true),
    ("letter-spacing", true),
    ("dominant-baseline", true),
    ("text-decoration", true),
    ("marker-start", true),
    ("marker-mid", true),
    ("marker-end", true),
    ("opacity", false),
    ("display", false),
    ("filter", false),
    ("clip-path", false),
    ("mask", false),
    ("mix-blend-mode", false),
    ("alignment-baseline", false),
    ("stop-color", false),
    ("stop-opacity", false),
    ("animation", false),
    ("animation-name", false),
    ("overflow", false),
];

fn prop_key(name: &str) -> Option<(&'static str, bool)> {
    PROPS.iter().find(|(n, _)| *n == name).copied()
}

/// The `font` shorthand as its parts.
fn expand_font(v: &str, out: &mut Vec<(String, String)>) {
    let tokens: Vec<&str> = v.split_whitespace().collect();
    let Some(i) = tokens.iter().position(|t| {
        let size = t.split('/').next().unwrap_or("");
        scan_number(size).is_some_and(|(_, n)| n < size.len()) || size.ends_with("px")
    }) else {
        return;
    };
    for t in &tokens[..i] {
        match *t {
            "italic" | "oblique" => out.push(("font-style".into(), "italic".into())),
            "bold" | "bolder" | "lighter" | "normal" => {
                out.push(("font-weight".into(), t.to_string()))
            }
            w if w.parse::<u16>().is_ok() => out.push(("font-weight".into(), w.into())),
            _ => {}
        }
    }
    let size = tokens[i].split('/').next().unwrap_or("");
    out.push(("font-size".into(), size.into()));
    if i + 1 < tokens.len() {
        out.push(("font-family".into(), tokens[i + 1..].join(" ")));
    }
}

/// Computed style of an element (inherited properties and its own).
#[derive(Clone, Default, Debug)]
struct Style(HashMap<&'static str, String>);

impl Style {
    fn get(&self, k: &str) -> Option<&str> {
        self.0.get(k).map(String::as_str)
    }

    /// With the animated values of properties.
    fn over(&self, ov: &Ov) -> Style {
        let mut s = self.clone();
        for (k, v) in &ov.attrs {
            if let Some((n, _)) = prop_key(k) {
                s.0.insert(n, v.clone());
            }
        }
        s
    }

    /// The inherited properties as a `style` attribute.
    fn inherited_css(&self) -> String {
        let mut out = String::new();
        for (k, inherited) in PROPS {
            if let (true, Some(v)) = (inherited, self.get(k)) {
                out.push_str(&format!("{k}:{v};"));
            }
        }
        out
    }

    fn hidden(&self) -> bool {
        self.get("display") == Some("none")
            || matches!(self.get("visibility"), Some("hidden" | "collapse"))
    }
}

// -------------------------------------------------------------------- SMIL --

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cat {
    Opacity,
    Colour,
    Geometry,
}

fn prop_cat(p: Prop) -> Cat {
    match p {
        Prop::Opacity => Cat::Opacity,
        Prop::Fill | Prop::Stroke | Prop::Fill2 => Cat::Colour,
        _ => Cat::Geometry,
    }
}

fn prop_label(p: Prop) -> &'static str {
    match p {
        Prop::X | Prop::Y => "Position",
        Prop::Width | Prop::Height => "Size",
        Prop::Rotation => "Rotation",
        Prop::ScaleX | Prop::ScaleY => "Scale",
        Prop::AnchorX | Prop::AnchorY => "Anchor point",
        Prop::Opacity => "Opacity",
        Prop::Fill | Prop::Stroke | Prop::Fill2 => "Colour",
        Prop::StrokeWidth => "Stroke width",
        Prop::CornerRadius => "Corner radius",
        Prop::FontSize => "Font size",
        Prop::LetterSpacing => "Letter spacing",
        _ => "Animation",
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TfType {
    Translate,
    Scale,
    Rotate,
    SkewX,
    SkewY,
}

impl TfType {
    /// Matrix and rotation (degrees) of a value.
    fn matrix(self, v: &[f64]) -> (Mat, f64) {
        let g = |i: usize, d: f64| v.get(i).copied().unwrap_or(d);
        match self {
            TfType::Translate => (translate(g(0, 0.0), g(1, 0.0)), 0.0),
            TfType::Scale => (scale(g(0, 1.0), g(1, g(0, 1.0))), 0.0),
            TfType::Rotate => {
                let (a, cx, cy) = (g(0, 0.0), g(1, 0.0), g(2, 0.0));
                (
                    mul(&mul(&translate(cx, cy), &rotate(a)), &translate(-cx, -cy)),
                    a,
                )
            }
            TfType::SkewX => ([1.0, 0.0, g(0, 0.0).to_radians().tan(), 1.0, 0.0, 0.0], 0.0),
            TfType::SkewY => ([1.0, g(0, 0.0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0], 0.0),
        }
    }

    /// The value with all its numbers (defaults filled in).
    fn normalize(self, v: Vec<f64>) -> Vec<f64> {
        let g = |i: usize, d: f64| v.get(i).copied().unwrap_or(d);
        match self {
            TfType::Translate => vec![g(0, 0.0), g(1, 0.0)],
            TfType::Scale => vec![g(0, 1.0), g(1, g(0, 1.0))],
            TfType::Rotate => vec![g(0, 0.0), g(1, 0.0), g(2, 0.0)],
            TfType::SkewX | TfType::SkewY => vec![g(0, 0.0)],
        }
    }

    fn name(self) -> &'static str {
        match self {
            TfType::Translate => "translate",
            TfType::Scale => "scale",
            TfType::Rotate => "rotate",
            TfType::SkewX => "skewX",
            TfType::SkewY => "skewY",
        }
    }
}

#[derive(Debug, Clone)]
enum SKind {
    Attr(String),
    Transform(TfType),
    Motion,
}

#[derive(Debug, Clone, PartialEq)]
enum SVal {
    Num(Vec<f64>),
    Color(Color),
    Str(String),
}

impl SVal {
    fn lerp(&self, b: &SVal, t: f64) -> SVal {
        match (self, b) {
            (SVal::Num(a), SVal::Num(b)) if a.len() == b.len() => {
                SVal::Num(a.iter().zip(b).map(|(a, b)| a + (b - a) * t).collect())
            }
            (SVal::Color(Color(a)), SVal::Color(Color(b))) => {
                SVal::Color(Color(std::array::from_fn(|i| {
                    a[i] + (b[i] - a[i]) * t as f32
                })))
            }
            _ => {
                if t < 1.0 {
                    self.clone()
                } else {
                    b.clone()
                }
            }
        }
    }

    fn to_attr(&self) -> String {
        match self {
            SVal::Num(v) => format!("{}", v.first().copied().unwrap_or(0.0)),
            SVal::Color(c) => c.to_hex(),
            SVal::Str(s) => s.clone(),
        }
    }
}

/// One SMIL animation of an element.
#[derive(Debug, Clone)]
struct Smil {
    target: NodeId,
    kind: SKind,
    cat: Cat,
    begin: f64,
    dur: f64,
    /// End of the active duration (may be infinite for a `set`).
    end: f64,
    freeze: bool,
    values: Vec<SVal>,
    times: Vec<f64>,
    eases: Vec<Easing>,
    discrete: bool,
    sum: bool,
    /// Simplified (repeat played once, paced timing…).
    partial: bool,
    notes: Vec<Note>,
    /// Not converted: the note saying why.
    ignored: Option<Note>,
}

impl Smil {
    fn name(&self) -> String {
        match &self.kind {
            SKind::Attr(a) => a.clone(),
            SKind::Transform(t) => t.name().into(),
            SKind::Motion => "motion".into(),
        }
    }

    /// Simple time 0..1 at `t`.
    fn progress(&self, t: f64) -> f64 {
        if self.dur.is_finite() && self.dur > 0.0 {
            ((t - self.begin) / self.dur).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Its value at `t` (just before `t` when `left`), None when it does
    /// not apply.
    fn value(&self, t: f64, left: bool) -> Option<SVal> {
        const E: f64 = 1e-9;
        let (b, e) = (self.begin, self.end);
        let active = if left {
            t > b + E && t <= e + E
        } else {
            t >= b - E && t < e - E
        };
        if active {
            return Some(self.at(self.progress(t), left));
        }
        let after = if left { t > e + E } else { t >= e - E };
        (after && self.freeze && e.is_finite()).then(|| self.at(self.progress(e), true))
    }

    fn at(&self, p: f64, left: bool) -> SVal {
        let n = self.values.len();
        if n == 1 {
            return self.values[0].clone();
        }
        let kt = &self.times;
        if self.discrete {
            let i = kt
                .iter()
                .rposition(|&k| if left { k < p - 1e-9 } else { k <= p + 1e-9 })
                .unwrap_or(0);
            return self.values[i.min(n - 1)].clone();
        }
        if p >= 1.0 {
            return self.values[n - 1].clone();
        }
        let i = (0..n - 1)
            .find(|&i| if left { p <= kt[i + 1] } else { p < kt[i + 1] })
            .unwrap_or(n - 2);
        let span = kt[i + 1] - kt[i];
        let f = if span > 1e-12 {
            (p - kt[i]) / span
        } else {
            1.0
        };
        let e = self.eases.get(i).copied().unwrap_or(Easing::Linear);
        self.values[i].lerp(&self.values[i + 1], e.apply(f))
    }

    /// The easing of its curve from `t` on (Hold when inactive).
    fn ease_at(&self, t: f64) -> Option<Easing> {
        if t < self.begin - 1e-9 || t >= self.end - 1e-9 {
            return None;
        }
        if self.discrete || self.values.len() < 2 {
            return Some(Easing::Hold);
        }
        let p = self.progress(t);
        let i = (0..self.values.len() - 1)
            .find(|&i| p < self.times[i + 1] - 1e-9)
            .unwrap_or(self.values.len() - 2);
        Some(self.eases.get(i).copied().unwrap_or(Easing::Linear))
    }

    /// The times its value changes curve.
    fn key_times(&self) -> Vec<f64> {
        let mut out = vec![self.begin];
        if self.dur.is_finite() {
            out.extend(
                self.times
                    .iter()
                    .map(|k| self.begin + k * self.dur)
                    .filter(|t| *t <= self.end),
            );
        }
        if self.end.is_finite() {
            out.push(self.end);
        }
        out
    }
}

/// Animated values at a time, over the element's attributes.
#[derive(Debug, Clone, Default)]
struct Ov {
    attrs: HashMap<String, String>,
    transform: Option<Mat>,
    /// Rotation (degrees) of the animated transform, to count whole turns.
    rot: Option<f64>,
}

// --------------------------------------------------------------- converter --

#[derive(Clone)]
struct Scope {
    parent: String,
    rep: Option<usize>,
    /// From the parent element's user space to the parent layer's.
    p: Mat,
    vp: (f64, f64),
    depth: u32,
    style: Style,
    uses: Vec<NodeId>,
}

/// How an element is placed in its parent layer.
#[derive(Clone)]
struct Fit {
    p: Mat,
    vp: (f64, f64),
    /// Scale baked into the content (from the still state).
    u: Option<(f64, f64)>,
    /// Point (user space) the rotation / scale animations turn around.
    pivot: Option<(f64, f64)>,
    /// Pen points in the parent layer's space (no animated transform).
    bake: bool,
    /// Visibility / display are animated: they make the opacity.
    vis_anim: bool,
}

struct Shape {
    kind: LayerKind,
    props: BTreeMap<Prop, Value>,
    notes: Vec<Note>,
    u: (f64, f64),
    source: &'static str,
    /// Text: its lines.
    text: Option<Vec<String>>,
}

enum Paint {
    None,
    Color(Color),
    Grad(Grad),
    Pattern,
}

#[derive(Debug, Clone)]
struct Grad {
    radial: bool,
    bbox: bool,
    /// x1 y1 x2 y2, or cx cy r fx fy.
    c: [f64; 5],
    tf: Mat,
    stops: Vec<(f64, Color)>,
    pad: bool,
}

struct Rep {
    r: LayerReport,
    children: Vec<usize>,
}

/// A group whose box is fitted to its content (or to its clip) at the end.
struct GroupBox {
    id: String,
    clip: Option<[f64; 4]>,
}

struct Cv<'a, 'i> {
    doc: &'a Document<'i>,
    ctx: &'a Ctx,
    ids: HashMap<&'a str, Node<'a, 'i>>,
    rules: Vec<Rule>,
    smils: Vec<Smil>,
    anims: HashMap<NodeId, Vec<usize>>,
    colors: HashMap<String, Option<Color>>,
    layers: Vec<Layer>,
    reps: Vec<Rep>,
    top: Vec<usize>,
    groups: Vec<GroupBox>,
    ignored: Vec<Ignored>,
    warnings: Vec<Note>,
    fields: Vec<Field>,
    files: Vec<(String, Vec<u8>)>,
    slug: String,
    counts: HashMap<String, usize>,
    intrinsic: HashMap<NodeId, (f64, f64)>,
    /// Segments of the `path` elements (read once, sampled often).
    paths: HashMap<NodeId, Vec<tiny_skia::PathSegment>>,
    image_files: HashMap<NodeId, String>,
    rasters: usize,
    css: Option<String>,
    root_px: (f64, f64),
    /// End of the last animation, s.
    end: f64,
    full: bool,
}

impl<'a, 'i> Cv<'a, 'i> {
    fn new(doc: &'a Document<'i>, ctx: &'a Ctx) -> Self {
        let mut ids = HashMap::new();
        for n in doc.descendants().filter(|n| n.is_element()) {
            if let Some(id) = n.attribute("id") {
                ids.entry(id).or_insert(n);
            }
        }
        Cv {
            doc,
            ctx,
            ids,
            rules: Vec::new(),
            smils: Vec::new(),
            anims: HashMap::new(),
            colors: HashMap::new(),
            layers: Vec::new(),
            reps: Vec::new(),
            top: Vec::new(),
            groups: Vec::new(),
            ignored: Vec::new(),
            warnings: Vec::new(),
            fields: Vec::new(),
            files: Vec::new(),
            slug: String::new(),
            counts: HashMap::new(),
            intrinsic: HashMap::new(),
            paths: HashMap::new(),
            image_files: HashMap::new(),
            rasters: 0,
            css: None,
            root_px: (0.0, 0.0),
            end: 0.0,
            full: false,
        }
    }

    fn run(&mut self, root: Node<'a, 'i>, original: Vec<u8>) -> Converted {
        // Style sheets.
        let mut keyframes = Vec::new();
        let mut skipped = 0;
        for s in self.doc.descendants().filter(|n| is_svg(*n, "style")) {
            let css: String = s.children().filter_map(|c| c.text()).collect();
            parse_sheet(
                &strip_comments(&css),
                &mut self.rules,
                &mut keyframes,
                &mut skipped,
            );
        }
        if skipped > 0 {
            self.warnings
                .push(Note::new("Style rules not supported", Status::Partial));
        }
        for k in keyframes {
            self.ignored.push(Ignored::new(&k, "css", "CSS animation"));
        }
        // SMIL.
        for n in self.doc.descendants().filter(|n| {
            n.is_element()
                && matches!(
                    n.tag_name().name(),
                    "animate" | "set" | "animateTransform" | "animateMotion" | "animateColor"
                )
        }) {
            if self.smils.len() >= MAX_ANIMS {
                break;
            }
            if let Some(s) = self.parse_smil(n) {
                self.anims
                    .entry(s.target)
                    .or_default()
                    .push(self.smils.len());
                self.smils.push(s);
            }
        }

        // Size and view box.
        let vb = root
            .attribute("viewBox")
            .map(numbers)
            .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
        let attr_len = |n: &str| {
            root.attribute(n)
                .filter(|v| !v.trim().ends_with('%'))
                .and_then(|v| length(v, Axis::Other, (0.0, 0.0)))
                .filter(|v| *v > 0.0)
        };
        let (w, h) = match (attr_len("width"), attr_len("height"), &vb) {
            (Some(w), Some(h), _) => (w, h),
            (Some(w), None, Some(v)) => (w, w * v[3] / v[2]),
            (None, Some(h), Some(v)) => (h * v[2] / v[3], h),
            (_, _, Some(v)) => (v[2], v[3]),
            (w, h, None) => (w.unwrap_or(300.0), h.unwrap_or(150.0)),
        };
        let (w, h) = (
            w.round().clamp(1.0, MAX_SIZE),
            h.round().clamp(1.0, MAX_SIZE),
        );
        self.root_px = (w, h);
        let (p, vp) = match &vb {
            Some(v) => (
                view_box(
                    [v[0], v[1], v[2], v[3]],
                    (w, h),
                    root.attribute("preserveAspectRatio").unwrap_or(""),
                ),
                (v[2], v[3]),
            ),
            None => (ID, (w, h)),
        };

        let title = root
            .children()
            .find(|n| is_svg(*n, "title"))
            .map(|n| n.children().filter_map(|c| c.text()).collect::<String>())
            .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|t| !t.is_empty());
        let name = title
            .clone()
            .or_else(|| root.attribute("id").map(str::to_string))
            .unwrap_or_else(|| "SVG".into());
        self.slug = Some(slug(&name, '-'))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "svg".into());

        // Root group, the size of the SVG.
        let root_id = new_id();
        let mut g = layer(&root_id, &name, None, group_kind(false));
        for (k, v) in [
            (Prop::X, 0.0),
            (Prop::Y, 0.0),
            (Prop::Width, w),
            (Prop::Height, h),
        ] {
            g.props.insert(k, Value::Number(v));
        }
        self.layers.push(g);
        let style = self.style(root, &Style::default());
        let sc = Scope {
            parent: root_id,
            rep: None,
            p,
            vp,
            depth: 0,
            style,
            uses: Vec::new(),
        };
        for c in root.children() {
            self.walk(c, &sc);
        }
        self.fit_groups();

        // Template.
        let animated = self.end > 1e-6;
        let mut t = Template::blank(&name, &self.ctx.zone, FPS);
        let duration = if animated {
            (self.end * FPS).ceil().max(1.0)
        } else {
            FPS
        };
        t.duration = duration;
        for m in &mut t.markers {
            if m.kind == MarkerKind::Out {
                m.frame = if animated { duration } else { 0.0 };
            }
        }
        t.fields = std::mem::take(&mut self.fields);
        t.layers = std::mem::take(&mut self.layers);

        let mut reports = Vec::new();
        let top = std::mem::take(&mut self.top);
        self.flatten(&top, &mut reports);
        let file = format!("{}.svg", self.slug);
        self.files.insert(0, (file.clone(), original));
        Converted {
            format: Format::Svg,
            name: String::new(),
            width: w as u32,
            height: h as u32,
            fps: FPS,
            comps: vec![Comp {
                id: new_id(),
                name,
                duration: if animated { duration / FPS } else { 0.0 },
                selected: true,
                template: t,
                layers: reports,
                ignored: std::mem::take(&mut self.ignored),
                variables: Vec::new(),
                animated,
                flat: Some(format!("{}{file}", self.ctx.prefix)),
            }],
            warnings: std::mem::take(&mut self.warnings),
            files: std::mem::take(&mut self.files),
        }
    }

    fn flatten(&self, list: &[usize], out: &mut Vec<LayerReport>) {
        for &i in list.iter().rev() {
            out.push(self.reps[i].r.clone());
            self.flatten(&self.reps[i].children, out);
        }
    }

    // ------------------------------------------------------------ styles --

    /// Declarations of an element, lowest priority first.
    fn decls(&self, n: Node) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = n
            .attributes()
            .filter(|a| a.namespace().is_none() && prop_key(a.name()).is_some())
            .map(|a| (a.name().to_string(), a.value().trim().to_string()))
            .collect();
        let mut matched: Vec<&Rule> = self.rules.iter().filter(|r| r.sel.matches(n)).collect();
        matched.sort_by_key(|r| (r.spec, r.order));
        let inline = n.attribute("style").map(parse_decls).unwrap_or_default();
        let sheet = matched.iter().flat_map(|r| r.decls.iter());
        let mut ordered: Vec<&(String, String, bool)> = sheet.clone().filter(|d| !d.2).collect();
        ordered.extend(inline.iter().filter(|d| !d.2));
        ordered.extend(sheet.filter(|d| d.2));
        ordered.extend(inline.iter().filter(|d| d.2));
        for (k, v, _) in ordered {
            if k == "font" {
                expand_font(v, &mut out);
            } else {
                out.push((k.clone(), v.clone()));
            }
        }
        out
    }

    fn style(&self, n: Node, parent: &Style) -> Style {
        let mut s = Style(
            parent
                .0
                .iter()
                .filter(|(k, _)| prop_key(k).is_some_and(|p| p.1))
                .map(|(k, v)| (*k, v.clone()))
                .collect(),
        );
        for (k, v) in self.decls(n) {
            let Some((name, _)) = prop_key(&k) else {
                continue;
            };
            match v.as_str() {
                "inherit" => match parent.0.get(name) {
                    Some(p) => {
                        s.0.insert(name, p.clone());
                    }
                    None => {
                        s.0.remove(name);
                    }
                },
                "initial" | "unset" => {
                    s.0.remove(name);
                }
                _ => {
                    s.0.insert(name, v);
                }
            }
        }
        s
    }

    fn color(&mut self, s: &str) -> Option<Color> {
        let l = s.trim().to_ascii_lowercase();
        if l.starts_with('#') || l == "transparent" {
            return parse_color(&l);
        }
        if let Some(c) = self.colors.get(&l) {
            return *c;
        }
        let c = usvg_color(&l);
        if self.colors.len() < 4096 {
            self.colors.insert(l, c);
        }
        c
    }

    fn paint(&mut self, v: &str, st: &Style) -> Paint {
        let v = v.trim();
        if v == "none" || v.is_empty() {
            return Paint::None;
        }
        if v.starts_with("url(") {
            let fallback = v.find(')').map_or("", |i| v[i + 1..].trim());
            if let Some(n) = url_id(v).and_then(|id| self.ids.get(id).copied()) {
                match n.tag_name().name() {
                    "linearGradient" | "radialGradient" => {
                        if let Some(g) = self.gradient(n) {
                            return match g.stops.len() {
                                0 => Paint::None,
                                1 => Paint::Color(g.stops[0].1),
                                _ => Paint::Grad(g),
                            };
                        }
                    }
                    "pattern" => return Paint::Pattern,
                    _ => {}
                }
            }
            return if fallback.is_empty() {
                Paint::None
            } else {
                self.paint(fallback, st)
            };
        }
        if v.eq_ignore_ascii_case("currentcolor") {
            let c = st.get("color").unwrap_or("black").to_string();
            return self.color(&c).map_or(Paint::None, Paint::Color);
        }
        self.color(v).map_or(Paint::None, Paint::Color)
    }

    /// A gradient with what it inherits through `href`.
    fn gradient(&mut self, n: Node<'a, 'i>) -> Option<Grad> {
        let mut chain = vec![n];
        while chain.len() < 8 {
            let last = *chain.last()?;
            match href(last)
                .and_then(url_id)
                .and_then(|id| self.ids.get(id).copied())
            {
                Some(next)
                    if matches!(next.tag_name().name(), "linearGradient" | "radialGradient")
                        && !chain.contains(&next) =>
                {
                    chain.push(next)
                }
                _ => break,
            }
        }
        let attr = |name: &str| chain.iter().find_map(|g| g.attribute(name));
        let radial = n.tag_name().name() == "radialGradient";
        let bbox = attr("gradientUnits") != Some("userSpaceOnUse");
        let vp = (self.root_px.0, self.root_px.1);
        let num = |name: &str, def: &str| {
            let s = attr(name).unwrap_or(def);
            if bbox {
                let s = s.trim();
                scan_number(s).map(|(v, k)| if s[k..].trim() == "%" { v / 100.0 } else { v })
            } else {
                length(s, axis_of(name), vp)
            }
            .unwrap_or(0.0)
        };
        let c = if radial {
            let (cx, cy) = (num("cx", "50%"), num("cy", "50%"));
            [
                cx,
                cy,
                num("r", "50%"),
                attr("fx").map_or(cx, |_| num("fx", "50%")),
                attr("fy").map_or(cy, |_| num("fy", "50%")),
            ]
        } else {
            [
                num("x1", "0%"),
                num("y1", "0%"),
                num("x2", "100%"),
                num("y2", "0%"),
                0.0,
            ]
        };
        let tf = attr("gradientTransform").and_then(transform).unwrap_or(ID);
        let pad = matches!(attr("spreadMethod"), None | Some("pad"));
        let holder = chain
            .iter()
            .find(|g| g.children().any(|c| is_svg(c, "stop")))
            .copied();
        let mut stops: Vec<(f64, Color)> = Vec::new();
        for s in holder
            .into_iter()
            .flat_map(|h| h.children())
            .filter(|c| is_svg(*c, "stop"))
        {
            let st = self.style(s, &Style::default());
            let at = s.attribute("offset").and_then(ratio).unwrap_or(0.0);
            let at = stops.last().map_or(at, |l| at.max(l.0));
            let c = st.get("stop-color").unwrap_or("black").to_string();
            let col = if c.eq_ignore_ascii_case("currentcolor") {
                Some(Color::BLACK)
            } else {
                self.color(&c)
            }
            .unwrap_or(Color::BLACK);
            let o = st.get("stop-opacity").and_then(ratio).unwrap_or(1.0);
            stops.push((at, with_alpha(col, o)));
            if stops.len() >= 64 {
                break;
            }
        }
        Some(Grad {
            radial,
            bbox,
            c,
            tf,
            stops,
            pad,
        })
    }

    // ---------------------------------------------------------- SMIL -----

    fn parse_smil(&self, n: Node<'a, 'i>) -> Option<Smil> {
        let tag = n.tag_name().name();
        let target = match href(n) {
            Some(r) => *self.ids.get(r.trim().trim_start_matches('#'))?,
            None => n.parent_element()?,
        };
        let kind = match tag {
            "animateTransform" => {
                SKind::Transform(match n.attribute("type").unwrap_or("translate") {
                    "translate" => TfType::Translate,
                    "scale" => TfType::Scale,
                    "rotate" => TfType::Rotate,
                    "skewX" => TfType::SkewX,
                    "skewY" => TfType::SkewY,
                    _ => return None,
                })
            }
            "animateMotion" => SKind::Motion,
            _ => SKind::Attr(n.attribute("attributeName")?.trim().to_string()),
        };
        let cat = match &kind {
            SKind::Attr(a) => match a.as_str() {
                "opacity" | "visibility" | "display" => Cat::Opacity,
                "fill" | "stroke" | "color" | "fill-opacity" | "stroke-opacity" | "stop-color"
                | "stop-opacity" => Cat::Colour,
                _ => Cat::Geometry,
            },
            _ => Cat::Geometry,
        };
        let mut s = Smil {
            target: target.id(),
            kind,
            cat,
            begin: 0.0,
            dur: f64::INFINITY,
            end: f64::INFINITY,
            freeze: n.attribute("fill") == Some("freeze"),
            values: Vec::new(),
            times: Vec::new(),
            eases: Vec::new(),
            discrete: false,
            sum: n.attribute("additive") == Some("sum"),
            partial: false,
            notes: Vec::new(),
            ignored: None,
        };
        let label = s.name();
        match self.begin_of(n, 0) {
            Some(b) => s.begin = b,
            None => {
                s.ignored = Some(Note::new("Interactive animation", Status::Ignored));
                return Some(s);
            }
        }
        s.dur = n
            .attribute("dur")
            .and_then(clock)
            .filter(|d| *d > 0.0)
            .unwrap_or(f64::INFINITY);
        if tag != "set" && !s.dur.is_finite() {
            s.ignored = Some(Note::new("Animated {name}", Status::Ignored).with("name", label));
            return Some(s);
        }
        s.end = self.active_end(n, s.begin, 0).unwrap_or(f64::INFINITY);
        // A set without `dur` holds its value (with one, it goes back after).
        if tag == "set" && !s.dur.is_finite() {
            s.freeze = true;
        }
        let repeat = n.attribute("repeatCount").is_some_and(|r| {
            r.trim() == "indefinite" || r.trim().parse::<f64>().is_ok_and(|v| v > 1.0)
        }) || n.attribute("repeatDur").is_some();
        if repeat {
            s.partial = true;
            s.notes.push(Note::new("Repeat → once", Status::Partial));
        }

        // Values.
        let kind = s.kind.clone();
        let parse = |v: &str| -> Option<SVal> {
            match &kind {
                SKind::Transform(t) => {
                    let v = numbers(v);
                    (!v.is_empty()).then(|| SVal::Num(t.normalize(v)))
                }
                SKind::Motion => {
                    let v = numbers(v);
                    (v.len() >= 2).then(|| SVal::Num(vec![v[0], v[1]]))
                }
                SKind::Attr(a) => {
                    if is_color_attr(a) {
                        let v = v.trim();
                        if v.eq_ignore_ascii_case("none") {
                            Some(SVal::Color(Color::TRANSPARENT))
                        } else {
                            parse_color(v).map(SVal::Color)
                        }
                    } else if matches!(a.as_str(), "visibility" | "display") {
                        Some(SVal::Str(v.trim().to_string()))
                    } else if matches!(a.as_str(), "opacity" | "fill-opacity" | "stroke-opacity") {
                        let t = v.trim();
                        scan_number(t).map(|(x, k)| {
                            SVal::Num(vec![if t[k..].trim() == "%" { x / 100.0 } else { x }])
                        })
                    } else {
                        length(v, axis_of(a), self.root_px).map(|x| SVal::Num(vec![x]))
                    }
                }
            }
        };
        let base = || -> Option<SVal> {
            match &kind {
                SKind::Transform(t) => Some(SVal::Num(t.normalize(Vec::new()))),
                SKind::Motion => Some(SVal::Num(vec![0.0, 0.0])),
                SKind::Attr(a) => {
                    let v = self
                        .decls(target)
                        .into_iter()
                        .rev()
                        .find(|(k, _)| k == a)
                        .map(|(_, v)| v)
                        .or_else(|| target.attribute(a.as_str()).map(str::to_string))
                        .unwrap_or_else(|| default_attr(a).to_string());
                    parse(&v)
                }
            }
        };
        let add = |a: &SVal, b: &SVal| match (a, b) {
            (SVal::Num(a), SVal::Num(b)) => {
                SVal::Num(a.iter().zip(b).map(|(a, b)| a + b).collect())
            }
            _ => b.clone(),
        };
        let mut motion_path = None;
        if let SKind::Motion = s.kind {
            let d = n.attribute("path").map(str::to_string).or_else(|| {
                n.children()
                    .find(|c| is_svg(*c, "mpath"))
                    .and_then(href)
                    .and_then(url_id)
                    .and_then(|id| self.ids.get(id))
                    .and_then(|p| p.attribute("d"))
                    .map(str::to_string)
            });
            if let Some(d) = d {
                motion_path = Some(d);
            }
        }
        if let Some(d) = motion_path {
            let segs = path_segments(&d).unwrap_or_default();
            let mut pts = Vec::new();
            for seg in &segs {
                match seg {
                    tiny_skia::PathSegment::MoveTo(p) if pts.is_empty() => {
                        pts.push((p.x as f64, p.y as f64))
                    }
                    tiny_skia::PathSegment::LineTo(p) => pts.push((p.x as f64, p.y as f64)),
                    _ => {
                        s.ignored = Some(Note::new("Motion path", Status::Ignored));
                        return Some(s);
                    }
                }
            }
            if pts.len() < 2 {
                return None;
            }
            s.values = pts.iter().map(|p| SVal::Num(vec![p.0, p.1])).collect();
            s.partial = true;
        } else if tag == "set" {
            s.values = vec![parse(n.attribute("to")?)?];
            s.discrete = true;
        } else if let Some(v) = n.attribute("values") {
            s.values = v
                .split(';')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(parse)
                .collect::<Option<Vec<_>>>()?;
        } else {
            let from = n.attribute("from").and_then(parse);
            let to = n.attribute("to").and_then(parse);
            let by = n.attribute("by").and_then(parse);
            let from = from.or_else(base)?;
            let to = to.or_else(|| by.map(|b| add(&from, &b)))?;
            s.values = vec![from, to];
        }
        if s.values.is_empty() {
            return None;
        }
        if let SKind::Motion = s.kind {
            // Along a straight line: position keyframes (no orientation).
            s.partial = true;
        }
        if s.sum && !matches!(s.kind, SKind::Transform(_) | SKind::Motion) {
            s.partial = true;
        }
        let mode = n.attribute("calcMode").unwrap_or(match s.kind {
            SKind::Motion => "paced",
            _ => "linear",
        });
        let nv = s.values.len();
        if tag != "set" {
            s.discrete = mode == "discrete"
                || matches!(&s.kind, SKind::Attr(a) if matches!(a.as_str(), "visibility" | "display"));
        }
        let kt: Vec<f64> = n
            .attribute("keyTimes")
            .map(|k| {
                k.split(';')
                    .filter_map(|x| x.trim().parse::<f64>().ok())
                    .collect()
            })
            .unwrap_or_default();
        let kt_ok = kt.len() == nv
            && kt.windows(2).all(|w| w[0] <= w[1])
            && kt.first() == Some(&0.0)
            && (s.discrete || kt.last() == Some(&1.0))
            && mode != "paced";
        s.times = if kt_ok {
            kt
        } else if s.discrete {
            (0..nv).map(|i| i as f64 / nv as f64).collect()
        } else if mode == "paced" && nv > 2 {
            // Spaced by distance (numbers only), else evenly.
            let dist: Vec<f64> = s
                .values
                .windows(2)
                .map(|w| match (&w[0], &w[1]) {
                    (SVal::Num(a), SVal::Num(b)) => a
                        .iter()
                        .zip(b)
                        .map(|(a, b)| (b - a) * (b - a))
                        .sum::<f64>()
                        .sqrt(),
                    _ => 1.0,
                })
                .collect();
            let total: f64 = dist.iter().sum();
            if total > 1e-9 {
                let mut acc = 0.0;
                let mut t = vec![0.0];
                for d in dist {
                    acc += d;
                    t.push(acc / total);
                }
                t
            } else {
                (0..nv).map(|i| i as f64 / (nv - 1) as f64).collect()
            }
        } else {
            (0..nv).map(|i| i as f64 / (nv - 1).max(1) as f64).collect()
        };
        s.eases = vec![Easing::Linear; nv.saturating_sub(1)];
        if mode == "spline" {
            let splines: Vec<Vec<f64>> = n
                .attribute("keySplines")
                .unwrap_or("")
                .split(';')
                .map(numbers)
                .filter(|v| !v.is_empty())
                .collect();
            if splines.len() == nv - 1 && splines.iter().all(|v| v.len() == 4) {
                s.eases = splines
                    .iter()
                    .map(|v| Easing::Bezier {
                        x1: v[0].clamp(0.0, 1.0),
                        y1: v[1],
                        x2: v[2].clamp(0.0, 1.0),
                        y2: v[3],
                    })
                    .collect();
            }
        }
        Some(s)
    }

    /// Start of an animation, s (None: on an event or never).
    fn begin_of(&self, n: Node, depth: u32) -> Option<f64> {
        let mut best: Option<f64> = None;
        for part in n.attribute("begin").unwrap_or("0").split(';') {
            let p = part.trim();
            let v = if p.is_empty() {
                None
            } else {
                clock(p).or_else(|| self.syncbase(p, depth))
            };
            if let Some(v) = v {
                best = Some(best.map_or(v, |b: f64| b.min(v)));
            }
        }
        best
    }

    /// End of its active duration: `dur` (played once), `end`.
    fn active_end(&self, n: Node, begin: f64, depth: u32) -> Option<f64> {
        let dur = n
            .attribute("dur")
            .and_then(clock)
            .filter(|d| *d > 0.0)
            .unwrap_or(f64::INFINITY);
        let mut e = begin + dur;
        if let Some(end) = n.attribute("end") {
            let v = end
                .split(';')
                .filter_map(|p| clock(p.trim()).or_else(|| self.syncbase(p.trim(), depth)))
                .filter(|v| *v >= begin)
                .fold(f64::INFINITY, f64::min);
            e = e.min(v);
        }
        Some(e)
    }

    /// `id.begin+1s`, `id.end`.
    fn syncbase(&self, p: &str, depth: u32) -> Option<f64> {
        if depth > 8 {
            return None;
        }
        let i = p.find(".begin").or_else(|| p.find(".end"))?;
        let (id, rest) = (&p[..i], &p[i + 1..]);
        let (ev, off) = match rest.find(['+', '-']) {
            Some(i) => {
                let o = clock(&rest[i + 1..])?;
                (&rest[..i], if &rest[i..i + 1] == "-" { -o } else { o })
            }
            None => (rest, 0.0),
        };
        let t = *self.ids.get(id.trim())?;
        let b = self.begin_of(t, depth + 1)?;
        match ev.trim() {
            "begin" => Some(b + off),
            "end" => self
                .active_end(t, b, depth + 1)
                .filter(|e| e.is_finite())
                .map(|e| e + off),
            _ => None,
        }
    }

    /// The animated values of an element at `t`.
    fn overrides(&self, anims: &[usize], t: f64, left: bool, base_tf: Mat, base_rot: f64) -> Ov {
        let mut ov = Ov::default();
        let mut tf: Option<(Mat, f64)> = None;
        let mut motion: Option<(f64, f64)> = None;
        for &i in anims {
            let a = &self.smils[i];
            let Some(v) = a.value(t, left) else { continue };
            match (&a.kind, v) {
                (SKind::Attr(name), v) => {
                    ov.attrs.insert(name.clone(), v.to_attr());
                }
                (SKind::Transform(ty), SVal::Num(v)) => {
                    let (m, r) = ty.matrix(&v);
                    let (cur, cr) = tf.unwrap_or((base_tf, base_rot));
                    tf = Some(if a.sum {
                        (mul(&cur, &m), cr + r)
                    } else {
                        (m, r)
                    });
                }
                (SKind::Motion, SVal::Num(v)) => {
                    motion = Some((v[0], v.get(1).copied().unwrap_or(0.0)));
                }
                _ => {}
            }
        }
        if tf.is_some() || motion.is_some() {
            let (m, r) = tf.unwrap_or((base_tf, base_rot));
            let (mx, my) = motion.unwrap_or((0.0, 0.0));
            ov.transform = Some(mul(&translate(mx, my), &m));
            ov.rot = Some(r);
        }
        ov
    }

    /// Keyframes of what `f` gives (the layer's properties) over the
    /// animations: the properties at 0 and the tracks of those that change.
    #[allow(clippy::type_complexity)]
    fn animate(
        &mut self,
        anims: &[usize],
        base_tf: Mat,
        mut f: impl FnMut(&mut Self, &Ov) -> BTreeMap<Prop, Value>,
    ) -> (BTreeMap<Prop, Value>, BTreeMap<Prop, Vec<Keyframe>>) {
        let base_rot = decompose(&base_tf).rot;
        let mut times: Vec<(f64, Vec<usize>)> = vec![(0.0, Vec::new())];
        for &i in anims {
            for t in self.smils[i].key_times() {
                if !t.is_finite() || !(0.0..=MAX_TIME).contains(&t) {
                    continue;
                }
                match times.iter_mut().find(|(x, _)| (x - t).abs() < 1e-6) {
                    Some((_, v)) => v.push(i),
                    None => times.push((t, vec![i])),
                }
            }
        }
        times.sort_by(|a, b| a.0.total_cmp(&b.0));
        times.truncate(2000);
        let mut left = Vec::with_capacity(times.len());
        let mut right = Vec::with_capacity(times.len());
        for (t, _) in &times {
            left.push((*t > 0.0).then(|| {
                let ov = self.overrides(anims, *t, true, base_tf, base_rot);
                f(self, &ov)
            }));
            let ov = self.overrides(anims, *t, false, base_tf, base_rot);
            right.push(f(self, &ov));
        }
        for (t, _) in &times {
            self.end = self.end.max(*t);
        }
        let props: BTreeSet<Prop> = right
            .iter()
            .chain(left.iter().flatten())
            .flat_map(|m| m.keys().copied())
            .collect();
        let mut stat = BTreeMap::new();
        let mut tracks = BTreeMap::new();
        for prop in props {
            let val =
                |m: &BTreeMap<Prop, Value>| m.get(&prop).copied().unwrap_or(default_value(prop));
            let first = val(&right[0]);
            let varies = right
                .iter()
                .chain(left.iter().flatten())
                .any(|m| !same(val(m), first));
            if !varies {
                if right[0].contains_key(&prop) && !same(first, default_value(prop)) {
                    stat.insert(prop, first);
                }
                continue;
            }
            let cat = prop_cat(prop);
            let mut track: Vec<Keyframe> = Vec::new();
            for (k, (t, prods)) in times.iter().enumerate() {
                if k > 0 && !prods.iter().any(|&i| self.smils[i].cat == cat) {
                    continue;
                }
                let frame = (t * FPS * 1000.0).round() / 1000.0;
                let r = match (val(&right[k]), prop) {
                    (Value::Number(v), _) => Value::Number(round4(v)),
                    (v, _) => v,
                };
                if let Some(l) = &left[k] {
                    let l = val(l);
                    // A jump: the value just before (unless already held).
                    let held = track
                        .last()
                        .is_some_and(|p| p.easing == Easing::Hold && same(p.value, l));
                    if !same(l, r) && !held && track.last().is_some_and(|p| p.frame < frame - 0.02)
                    {
                        track.push(Keyframe {
                            frame: frame - 0.01,
                            value: l,
                            easing: Easing::Hold,
                        });
                    }
                }
                let easing = anims
                    .iter()
                    .rev()
                    .filter(|&&i| self.smils[i].cat == cat)
                    .find_map(|&i| self.smils[i].ease_at(*t))
                    .unwrap_or(Easing::Hold);
                track.push(Keyframe {
                    frame,
                    value: r,
                    easing,
                });
            }
            // Keys between two equal ones say nothing.
            let mut i = 1;
            while i + 1 < track.len() {
                if same(track[i - 1].value, track[i].value)
                    && same(track[i].value, track[i + 1].value)
                {
                    track.remove(i);
                } else {
                    i += 1;
                }
            }
            tracks.insert(prop, track);
        }
        (stat, tracks)
    }

    // ------------------------------------------------------------- walk --

    fn element_name(&mut self, n: Node, tag: &str) -> String {
        if let Some(id) = n.attribute("id").filter(|i| !i.trim().is_empty()) {
            return id.to_string();
        }
        let c = self.counts.entry(tag.to_string()).or_insert(0);
        *c += 1;
        format!("{tag} {c}")
    }

    fn walk(&mut self, n: Node<'a, 'i>, sc: &Scope) {
        if !n.is_element() || !svg_ns(n) {
            return;
        }
        if sc.depth > MAX_DEPTH {
            return;
        }
        if self.layers.len() >= MAX_LAYERS {
            if !self.full {
                self.full = true;
                self.warnings
                    .push(Note::new("Too many elements", Status::Partial));
            }
            return;
        }
        let tag = n.tag_name().name();
        match tag {
            "g" | "a" | "switch" | "svg" | "use" => self.group(n, tag, sc),
            "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path" | "text"
            | "image" => self.leaf(n, tag, sc),
            "script" => {
                let name = self.element_name(n, tag);
                self.ignored.push(Ignored::new(&name, "script", "Script"));
            }
            "foreignObject" => {
                let name = self.element_name(n, tag);
                self.ignored
                    .push(Ignored::new(&name, "foreignObject", "HTML content"));
            }
            _ => {}
        }
    }

    /// Notes every layer may have: filters, blend modes, CSS animations,
    /// animations not converted.
    fn common_notes(&self, st: &Style, raster: bool) -> Vec<Note> {
        let mut notes = Vec::new();
        if !raster {
            if let Some(f) = st.get("filter").filter(|f| *f != "none") {
                let name = url_id(f).unwrap_or(f).to_string();
                notes.push(Note::new("Filter {name}", Status::Ignored).with("name", name));
            }
        }
        if let Some(b) = st.get("mix-blend-mode").filter(|b| *b != "normal") {
            notes.push(Note::new("Blend mode {name}", Status::Ignored).with("name", b));
        }
        if st.get("animation").is_some_and(|a| a != "none")
            || st.get("animation-name").is_some_and(|a| a != "none")
        {
            notes.push(Note::new("CSS animation", Status::Ignored));
        }
        notes
    }

    /// Animations of an element: those converted, and notes for the others.
    fn anims_of(&self, n: Node, tag: &str, raster: bool, notes: &mut Vec<Note>) -> Vec<usize> {
        let mut out = Vec::new();
        for &i in self.anims.get(&n.id()).map(Vec::as_slice).unwrap_or(&[]) {
            let s = &self.smils[i];
            if let Some(note) = &s.ignored {
                push_note(notes, note.clone());
                continue;
            }
            let ok = match &s.kind {
                SKind::Attr(a) => {
                    matches!(a.as_str(), "opacity" | "visibility" | "display")
                        || (!raster && supported_attr(tag, a))
                }
                SKind::Transform(t) => !raster && !matches!(t, TfType::SkewX | TfType::SkewY),
                SKind::Motion => !raster,
            };
            if ok {
                out.push(i);
                for note in &s.notes {
                    push_note(notes, note.clone());
                }
            } else {
                push_note(
                    notes,
                    Note::new("Animated {name}", Status::Ignored).with("name", s.name()),
                );
            }
        }
        out
    }

    /// Notes of the animated properties of a layer.
    fn anim_notes(
        &self,
        anims: &[usize],
        tracks: &BTreeMap<Prop, Vec<Keyframe>>,
        notes: &mut Vec<Note>,
    ) {
        for p in tracks.keys() {
            let cat = prop_cat(*p);
            let partial = anims
                .iter()
                .any(|&i| self.smils[i].cat == cat && self.smils[i].partial);
            push_note(
                notes,
                Note::anim(
                    prop_label(*p),
                    if partial { Status::Partial } else { Status::Ok },
                ),
            );
        }
    }

    fn pivot(&self, anims: &[usize]) -> Option<(f64, f64)> {
        let tf = |want: TfType| {
            anims.iter().find_map(
                |&i| match (&self.smils[i].kind, self.smils[i].values.first()) {
                    (SKind::Transform(t), Some(SVal::Num(v))) if *t == want => Some(v.clone()),
                    _ => None,
                },
            )
        };
        if let Some(v) = tf(TfType::Rotate) {
            return Some((v[1], v[2]));
        }
        tf(TfType::Scale).map(|_| (0.0, 0.0))
    }

    fn report(
        &mut self,
        sc: &Scope,
        layer: &str,
        name: &str,
        source: &str,
        notes: Vec<Note>,
        hidden: bool,
    ) -> usize {
        let status = if notes.iter().any(|n| n.status != Status::Ok) {
            Status::Partial
        } else {
            Status::Ok
        };
        let i = self.reps.len();
        self.reps.push(Rep {
            r: LayerReport {
                layer: layer.into(),
                name: name.into(),
                source: source.into(),
                depth: sc.depth,
                status,
                notes,
                hidden,
            },
            children: Vec::new(),
        });
        match sc.rep {
            Some(p) => self.reps[p].children.push(i),
            None => self.top.push(i),
        }
        i
    }

    fn el_transform(&self, n: Node, ov: &Ov) -> Mat {
        ov.transform
            .unwrap_or_else(|| n.attribute("transform").and_then(transform).unwrap_or(ID))
    }

    fn attr<'s>(&self, n: Node<'s, '_>, ov: &'s Ov, name: &str) -> Option<&'s str> {
        ov.attrs
            .get(name)
            .map(String::as_str)
            .or_else(|| n.attribute(name))
    }

    fn len_attr(&self, n: Node, ov: &Ov, name: &str, vp: (f64, f64)) -> Option<f64> {
        self.attr(n, ov, name).and_then(|v| {
            length(
                v.split([' ', ',']).find(|s| !s.is_empty()).unwrap_or(""),
                axis_of(name),
                vp,
            )
        })
    }

    /// Opacity of the layer.
    fn opacity(&self, st: &Style, vis_anim: bool) -> f64 {
        let o = st.get("opacity").and_then(ratio).unwrap_or(1.0);
        if vis_anim && st.hidden() {
            0.0
        } else {
            o
        }
    }

    /// The raster cause of an element's style, if any.
    fn raster_cause(&self, st: &Style, tag: &str) -> Option<&'static str> {
        if st.get("mask").is_some_and(|m| m != "none") {
            return Some("Mask");
        }
        if let Some(c) = st.get("clip-path").filter(|c| *c != "none") {
            if !matches!(tag, "g" | "a" | "switch" | "use" | "svg") || self.clip_rect(c).is_none() {
                return Some("Clipping path");
            }
        }
        if matches!(tag, "path" | "line" | "polyline" | "polygon")
            && ["marker-start", "marker-mid", "marker-end"]
                .iter()
                .any(|m| st.get(m).is_some_and(|v| v != "none"))
        {
            return Some("Markers");
        }
        for k in ["fill", "stroke"] {
            if let Some(n) = st
                .get(k)
                .filter(|v| v.trim_start().starts_with("url("))
                .and_then(url_id)
                .and_then(|id| self.ids.get(id))
            {
                if n.tag_name().name() == "pattern" {
                    return Some("Pattern fill");
                }
            }
        }
        None
    }

    /// A clip path that is one plain rectangle: its box (user space).
    fn clip_rect(&self, v: &str) -> Option<[f64; 4]> {
        let c = self.ids.get(url_id(v)?)?;
        if c.tag_name().name() != "clipPath"
            || c.attribute("clipPathUnits") == Some("objectBoundingBox")
        {
            return None;
        }
        let kids: Vec<Node> = c.children().filter(|k| k.is_element()).collect();
        let [r] = kids.as_slice() else { return None };
        if r.tag_name().name() != "rect" {
            return None;
        }
        let tf = mul(
            &c.attribute("transform").and_then(transform).unwrap_or(ID),
            &r.attribute("transform").and_then(transform).unwrap_or(ID),
        );
        if tf[0] != 1.0 || tf[1] != 0.0 || tf[2] != 0.0 || tf[3] != 1.0 {
            return None;
        }
        let vp = self.root_px;
        let g = |k: &str| r.attribute(k).and_then(|v| length(v, axis_of(k), vp));
        Some([
            g("x").unwrap_or(0.0) + tf[4],
            g("y").unwrap_or(0.0) + tf[5],
            g("width")?,
            g("height")?,
        ])
    }

    // ------------------------------------------------------------ groups --

    fn group(&mut self, n: Node<'a, 'i>, tag: &str, sc: &Scope) {
        let st = self.style(n, &sc.style);
        // `use`: what it shows.
        let used = if tag == "use" {
            let Some(t) = href(n)
                .and_then(url_id)
                .and_then(|id| self.ids.get(id).copied())
            else {
                return;
            };
            if sc.uses.contains(&t.id()) || sc.uses.len() >= MAX_USES || t == n {
                return;
            }
            Some(t)
        } else {
            None
        };
        let name = self.element_name(n, tag);
        let cause = self.raster_cause(&st, tag);
        if let Some(cause) = cause {
            self.rasterised(n, tag, sc, &st, name, cause);
            return;
        }
        let mut notes = self.common_notes(&st, false);
        let anims = self.anims_of(n, tag, false, &mut notes);
        let vis_anim = anims
            .iter()
            .any(|&i| matches!(&self.smils[i].kind, SKind::Attr(a) if a == "visibility" || a == "display"));
        let hidden = st.hidden() && !vis_anim;

        // Extra placement of the content: x / y of a use or a nested svg,
        // and the view box of a nested svg / symbol.
        let vp = sc.vp;
        let lenv = |k: &str, d: f64| {
            n.attribute(k)
                .and_then(|v| length(v, axis_of(k), vp))
                .unwrap_or(d)
        };
        let mut extra = ID;
        let mut inner = ID;
        let mut inner_vp = sc.vp;
        let mut clip = None;
        let viewport = |vbn: Node, w: f64, h: f64, inner: &mut Mat, inner_vp: &mut (f64, f64)| {
            if let Some(v) = vbn
                .attribute("viewBox")
                .map(numbers)
                .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0)
            {
                *inner = view_box(
                    [v[0], v[1], v[2], v[3]],
                    (w, h),
                    vbn.attribute("preserveAspectRatio").unwrap_or(""),
                );
                *inner_vp = (v[2], v[3]);
            } else {
                *inner_vp = (w, h);
            }
        };
        match tag {
            "svg" => {
                let (x, y) = (lenv("x", 0.0), lenv("y", 0.0));
                let (w, h) = (lenv("width", vp.0), lenv("height", vp.1));
                extra = translate(x, y);
                viewport(n, w, h, &mut inner, &mut inner_vp);
                if !matches!(st.get("overflow"), Some("visible" | "auto")) {
                    clip = Some([0.0, 0.0, w, h]);
                }
            }
            "use" => {
                extra = translate(lenv("x", 0.0), lenv("y", 0.0));
                if let Some(t) = used.filter(|t| t.tag_name().name() == "symbol") {
                    let w = n.attribute("width").or(t.attribute("width"));
                    let h = n.attribute("height").or(t.attribute("height"));
                    let w = w.and_then(|v| length(v, Axis::X, vp)).unwrap_or(vp.0);
                    let h = h.and_then(|v| length(v, Axis::Y, vp)).unwrap_or(vp.1);
                    viewport(t, w, h, &mut inner, &mut inner_vp);
                }
            }
            _ => {
                if let Some(r) = st.get("clip-path").and_then(|c| self.clip_rect(c)) {
                    clip = Some(r);
                }
            }
        }

        let pivot = self.pivot(&anims).or(Some((0.0, 0.0)));
        let fit = Fit {
            p: sc.p,
            vp: sc.vp,
            u: None,
            pivot,
            bake: false,
            vis_anim,
        };
        let size = self.root_px;
        let base = self.group_props(n, &st, &Ov::default(), &fit, extra, size);
        let id = new_id();
        let mut l = layer(&id, &name, Some(&sc.parent), group_kind(false));
        l.props = base.0.clone();
        let (u, child_p) = (base.1, base.2);
        if base.3 {
            push_note(&mut notes, Note::new("Skew", Status::Partial));
        }
        if !anims.is_empty() {
            let fit_t = Fit { u: Some(u), ..fit };
            let base_tf = self.el_transform(n, &Ov::default());
            let (stat, tracks) = self.animate(&anims, base_tf, |cv, ov| {
                let st_t = st.over(ov);
                cv.group_props(n, &st_t, ov, &fit_t, extra, size).0
            });
            for p in tracks.keys() {
                l.props.remove(p);
            }
            l.props.extend(stat);
            self.anim_notes(&anims, &tracks, &mut notes);
            l.tracks = tracks;
        }
        // The clip in the group's pixels.
        let clip_local = clip.map(|[x, y, w, h]| {
            let a = pt(&child_p, (x, y));
            let b = pt(&child_p, (x + w, y + h));
            [
                a.0.min(b.0),
                a.1.min(b.1),
                (b.0 - a.0).abs(),
                (b.1 - a.1).abs(),
            ]
        });
        if clip_local.is_some() {
            l.kind = group_kind(true);
        }
        self.layers.push(l);
        self.groups.push(GroupBox {
            id: id.clone(),
            clip: clip_local,
        });
        let rep = self.report(
            sc,
            &id,
            &name,
            if tag == "use" { "use" } else { "group" },
            notes,
            hidden,
        );
        let mut inner_sc = Scope {
            parent: id,
            rep: Some(rep),
            p: mul(&child_p, &inner),
            vp: inner_vp,
            depth: sc.depth + 1,
            style: st,
            uses: sc.uses.clone(),
        };
        match used {
            Some(t) => {
                inner_sc.uses.push(t.id());
                if t.tag_name().name() == "symbol" {
                    for c in t.children() {
                        self.walk(c, &inner_sc);
                    }
                } else {
                    self.walk(t, &inner_sc);
                }
            }
            None if tag == "switch" => {
                if let Some(c) = n.children().find(|c| c.is_element() && svg_ns(*c)) {
                    self.walk(c, &inner_sc);
                }
            }
            None => {
                for c in n.children() {
                    self.walk(c, &inner_sc);
                }
            }
        }
    }

    /// Group placement: properties, scale baked into the children, the
    /// matrix of the children (from the group's user space), skewed.
    fn group_props(
        &self,
        n: Node,
        st: &Style,
        ov: &Ov,
        fit: &Fit,
        extra: Mat,
        size: (f64, f64),
    ) -> (BTreeMap<Prop, Value>, (f64, f64), Mat, bool) {
        let m = mul(&mul(&fit.p, &self.el_transform(n, ov)), &extra);
        let mut props = BTreeMap::new();
        let d = decompose(&m);
        let hint = ov.rot.map(|r| decompose(&fit.p).rot + r);
        let (u, child, skew) = if d.skew && fit.u.is_none() {
            // A still skewed group: its children take the whole matrix.
            props.insert(Prop::X, Value::Number(round4(m[4])));
            props.insert(Prop::Y, Value::Number(round4(m[5])));
            props.insert(Prop::Width, Value::Number(size.0));
            props.insert(Prop::Height, Value::Number(size.1));
            props.insert(Prop::AnchorX, Value::Number(0.0));
            props.insert(Prop::AnchorY, Value::Number(0.0));
            ((1.0, 1.0), [m[0], m[1], m[2], m[3], 0.0, 0.0], false)
        } else {
            let u = fit.u.unwrap_or((d.sx.max(1e-9), d.sy.abs().max(1e-9)));
            let b = [0.0, 0.0, size.0 / u.0, size.1 / u.1];
            let _ = place(&m, b, Some(u), fit.pivot, hint, true, &mut props);
            (u, scale(u.0, u.1), d.skew)
        };
        let o = self.opacity(st, fit.vis_anim);
        if o < 1.0 || fit.vis_anim {
            props.insert(Prop::Opacity, Value::Number(round4(o)));
        }
        (props, u, child, skew)
    }

    /// Fits the groups' boxes to their content (or clip), deepest first.
    fn fit_groups(&mut self) {
        let index: HashMap<String, usize> = self
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.clone(), i))
            .collect();
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, l) in self.layers.iter().enumerate() {
            if let Some(p) = &l.parent {
                children.entry(p.clone()).or_default().push(i);
            }
        }
        let groups = std::mem::take(&mut self.groups);
        for g in groups.iter().rev() {
            let Some(&gi) = index.get(&g.id) else {
                continue;
            };
            let kids = children.get(&g.id).cloned().unwrap_or_default();
            let b = match g.clip {
                Some(c) => Some(c),
                None => {
                    let mut bb: Option<[f64; 4]> = None;
                    for &k in &kids {
                        for (x, y) in corners(&self.layers[k]) {
                            bb = Some(match bb {
                                None => [x, y, x, y],
                                Some(b) => [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
                            });
                        }
                    }
                    bb.map(|b| [b[0], b[1], b[2] - b[0], b[3] - b[1]])
                }
            };
            let Some([bx, by, bw, bh]) = b else { continue };
            let (bw, bh) = (bw.max(1.0), bh.max(1.0));
            let g = &self.layers[gi];
            if g.tracks.contains_key(&Prop::AnchorX) || g.tracks.contains_key(&Prop::AnchorY) {
                continue;
            }
            let num = |l: &Layer, p: Prop| {
                l.props
                    .get(&p)
                    .and_then(Value::as_number)
                    .unwrap_or_else(|| default_value(p).as_number().unwrap_or(0.0))
            };
            let (w0, h0) = (num(g, Prop::Width), num(g, Prop::Height));
            let a = (num(g, Prop::AnchorX) * w0, num(g, Prop::AnchorY) * h0);
            let still = ![Prop::Rotation, Prop::ScaleX, Prop::ScaleY]
                .iter()
                .any(|p| g.tracks.contains_key(p));
            let (rot, sx, sy) = (
                num(g, Prop::Rotation),
                num(g, Prop::ScaleX),
                num(g, Prop::ScaleY),
            );
            let g = &mut self.layers[gi];
            shift(g, bx, by);
            g.props.insert(Prop::Width, Value::Number(round4(bw)));
            g.props.insert(Prop::Height, Value::Number(round4(bh)));
            let a = (a.0 - bx, a.1 - by);
            if still {
                // Turning around its centre, like any layer (nothing turns it).
                let d = (a.0 - bw / 2.0, a.1 - bh / 2.0);
                let rs = pt(&mul(&rotate(rot), &scale(sx, sy)), d);
                shift(g, d.0 - rs.0, d.1 - rs.1);
                g.props.remove(&Prop::AnchorX);
                g.props.remove(&Prop::AnchorY);
            } else {
                g.props
                    .insert(Prop::AnchorX, Value::Number(round4(a.0 / bw)));
                g.props
                    .insert(Prop::AnchorY, Value::Number(round4(a.1 / bh)));
            }
            for &k in &kids {
                shift(&mut self.layers[k], -bx, -by);
            }
        }
    }

    // ------------------------------------------------------------ leaves --

    fn leaf(&mut self, n: Node<'a, 'i>, tag: &str, sc: &Scope) {
        let st = self.style(n, &sc.style);
        let name = self.element_name(n, tag);
        if tag == "image" && !self.load_image(n, &name) {
            return;
        }
        let mut cause = self.raster_cause(&st, tag);
        let mut notes = Vec::new();
        if tag == "image" {
            // resvg draws no bitmaps here: the picture stays, without it.
            if let Some(c) = cause.take() {
                notes.push(Note::new(c, Status::Ignored));
            }
        }
        let mut probe = Vec::new();
        let anims = self.anims_of(n, tag, false, &mut probe);
        let tf_anim = anims
            .iter()
            .any(|&i| matches!(self.smils[i].kind, SKind::Transform(_) | SKind::Motion));
        let vis_anim = anims
            .iter()
            .any(|&i| matches!(&self.smils[i].kind, SKind::Attr(a) if a == "visibility" || a == "display"));
        let fit = Fit {
            p: sc.p,
            vp: sc.vp,
            u: None,
            pivot: self.pivot(&anims),
            bake: !tf_anim,
            vis_anim,
        };
        let mut base = None;
        if cause.is_none() {
            match self.shape(n, tag, &st, &Ov::default(), &fit) {
                Ok(s) => base = Some(s),
                Err("") => return,
                Err(c) => cause = Some(c),
            }
        }
        let Some(mut shape) = base else {
            if tag == "image" {
                self.ignored
                    .push(Ignored::new(&name, "image", "Skewed picture"));
            } else {
                self.rasterised(n, tag, sc, &st, name, cause.unwrap_or("Skew"));
            }
            return;
        };
        notes.extend(self.common_notes(&st, false));
        notes.append(&mut probe);
        notes.append(&mut shape.notes);
        if let Some(f) = st
            .get("filter")
            .and_then(url_id)
            .and_then(|id| self.ids.get(id).copied())
        {
            self.drop_shadow(f, &mut shape, &mut notes);
        }
        let hidden = st.hidden() && !vis_anim;
        let id = new_id();
        let mut l = layer(&id, &name, Some(&sc.parent), shape.kind.clone());
        l.drop_shadow = shape.props.contains_key(&Prop::ShadowColor);
        l.props = shape.props.clone();
        if !anims.is_empty() {
            let fit_t = Fit {
                u: Some(shape.u),
                ..fit.clone()
            };
            let base_tf = self.el_transform(n, &Ov::default());
            let tag_s = tag.to_string();
            let keep = shape.props.clone();
            let (stat, tracks) = self.animate(&anims, base_tf, |cv, ov| {
                let mut p = cv
                    .shape(n, &tag_s, &st, ov, &fit_t)
                    .map(|s| s.props)
                    .unwrap_or_else(|_| keep.clone());
                for k in [
                    Prop::ShadowColor,
                    Prop::ShadowX,
                    Prop::ShadowY,
                    Prop::ShadowBlur,
                ] {
                    if let Some(v) = keep.get(&k) {
                        p.insert(k, *v);
                    }
                }
                p
            });
            for p in tracks.keys() {
                l.props.remove(p);
            }
            l.props.extend(stat);
            self.anim_notes(&anims, &tracks, &mut notes);
            l.tracks = tracks;
        }
        if let (Some(lines), LayerKind::Text { text, .. }) = (&shape.text, &mut l.kind) {
            let key = self.text_field(n, lines);
            *text = format!("{{{{{key}}}}}");
            notes.insert(0, Note::new("Text → variable", Status::Ok));
        }
        if let (LayerKind::Image { src, .. }, Some(file)) = (&mut l.kind, self.image_file(n)) {
            *src = format!("{}{file}", self.ctx.prefix);
        }
        self.layers.push(l);
        self.report(sc, &id, &name, shape.source, notes, hidden);
    }

    /// A field for a text: key from its id or its words.
    fn text_field(&mut self, n: Node, lines: &[String]) -> String {
        let text = lines.join("\n");
        let id = n.attribute("id").filter(|i| !i.trim().is_empty());
        let label = id.map(str::to_string).unwrap_or_else(|| {
            let first = lines.first().map(String::as_str).unwrap_or("");
            let mut l: String = first.chars().take(32).collect();
            if first.chars().count() > 32 {
                l.push('…');
            }
            if l.is_empty() {
                "Text".into()
            } else {
                l
            }
        });
        let mut base = slug(id.unwrap_or(&text), '_');
        if base.is_empty() || base.starts_with(|c: char| c.is_ascii_digit()) {
            base = format!(
                "text{}",
                if base.is_empty() {
                    String::new()
                } else {
                    format!("_{base}")
                }
            );
        }
        let mut key = base.clone();
        let mut i = 2;
        while self.fields.iter().any(|f| f.key == key) {
            key = format!("{base}_{i}");
            i += 1;
        }
        self.fields.push(Field {
            key: key.clone(),
            label,
            kind: if lines.len() > 1 {
                FieldKind::Multiline
            } else {
                FieldKind::Text
            },
            default: text,
            binding: None,
            min: None,
            max: None,
            options: Vec::new(),
        });
        key
    }

    /// A `feDropShadow` filter as the layer's drop shadow.
    fn drop_shadow(&mut self, f: Node, shape: &mut Shape, notes: &mut [Note]) {
        let prims: Vec<Node> = f.children().filter(|c| c.is_element()).collect();
        let [d] = prims.as_slice() else { return };
        if d.tag_name().name() != "feDropShadow" {
            return;
        }
        let g = |k: &str, def: f64| {
            d.attribute(k)
                .and_then(|v| numbers(v).first().copied())
                .unwrap_or(def)
        };
        let k = (shape.u.0 * shape.u.1).sqrt();
        let flood = d.attribute("flood-color").unwrap_or("black").to_string();
        let c = self.color(&flood).unwrap_or(Color::BLACK);
        let o = d.attribute("flood-opacity").and_then(ratio).unwrap_or(1.0);
        shape
            .props
            .insert(Prop::ShadowColor, Value::Color(with_alpha(c, o)));
        shape
            .props
            .insert(Prop::ShadowX, Value::Number(round4(g("dx", 2.0) * k)));
        shape
            .props
            .insert(Prop::ShadowY, Value::Number(round4(g("dy", 2.0) * k)));
        shape.props.insert(
            Prop::ShadowBlur,
            Value::Number(round4(g("stdDeviation", 2.0) * 2.0 * k)),
        );
        for n in notes.iter_mut() {
            if n.label == "Filter {name}" {
                n.status = Status::Partial;
            }
        }
    }

    /// The layer of an element: kind and properties, or why it is drawn
    /// as a picture ("" for nothing to draw).
    fn shape(
        &mut self,
        n: Node,
        tag: &str,
        st: &Style,
        ov: &Ov,
        fit: &Fit,
    ) -> Result<Shape, &'static str> {
        let st = st.over(ov);
        let m = mul(&fit.p, &self.el_transform(n, ov));
        let hint = ov.rot.map(|r| decompose(&fit.p).rot + r);
        let vp = fit.vp;
        let mut props = BTreeMap::new();
        let mut notes = Vec::new();
        let o = self.opacity(&st, fit.vis_anim);
        if o < 1.0 || fit.vis_anim {
            props.insert(Prop::Opacity, Value::Number(round4(o)));
        }
        let g = |cv: &Self, k: &str| cv.len_attr(n, ov, k, vp);
        let mut text = None;
        let (kind, u, source) = match tag {
            "rect" | "circle" | "ellipse" => {
                let (b, radius) = if tag == "rect" {
                    let (x, y) = (g(self, "x").unwrap_or(0.0), g(self, "y").unwrap_or(0.0));
                    let w = g(self, "width").unwrap_or(0.0).max(0.0);
                    let h = g(self, "height").unwrap_or(0.0).max(0.0);
                    let (rx, ry) = match (g(self, "rx"), g(self, "ry")) {
                        (Some(a), None) => (a, a),
                        (None, Some(b)) => (b, b),
                        (Some(a), Some(b)) => (a, b),
                        _ => (0.0, 0.0),
                    };
                    let (rx, ry) = (rx.clamp(0.0, w / 2.0), ry.clamp(0.0, h / 2.0));
                    if (rx - ry).abs() > 0.5 {
                        notes.push(Note::new("Rounded corners", Status::Partial));
                    }
                    ([x, y, w, h], rx.min(ry))
                } else {
                    let (cx, cy) = (g(self, "cx").unwrap_or(0.0), g(self, "cy").unwrap_or(0.0));
                    let (rx, ry) = if tag == "circle" {
                        let r = g(self, "r").unwrap_or(0.0);
                        (r, r)
                    } else {
                        match (g(self, "rx"), g(self, "ry")) {
                            (Some(a), None) => (a, a),
                            (None, Some(b)) => (b, b),
                            (Some(a), Some(b)) => (a, b),
                            _ => (0.0, 0.0),
                        }
                    };
                    let (rx, ry) = (rx.max(0.0), ry.max(0.0));
                    ([cx - rx, cy - ry, 2.0 * rx, 2.0 * ry], 0.0)
                };
                let u = place(&m, b, fit.u, fit.pivot, hint, false, &mut props)?;
                let (w, h) = (b[2] * u.0, b[3] * u.1);
                let map = |q: (f64, f64)| ((q.0 - b[0]) * u.0, (q.1 - b[1]) * u.1);
                let (fs, ss, _, _) = self.paints(
                    &st,
                    b,
                    &map,
                    (w, h),
                    (u.0 * u.1).sqrt(),
                    &mut props,
                    &mut notes,
                )?;
                let kind = if tag == "rect" {
                    if radius > 0.0 {
                        props.insert(
                            Prop::CornerRadius,
                            Value::Number(round4(radius * (u.0 * u.1).sqrt())),
                        );
                    }
                    let mut x = cg_core::wid::quad::Config {
                        fill_style: fs,
                        ..Default::default()
                    };
                    x.stroke_style = StrokeStyle {
                        position: StrokePosition::Center,
                        ..ss
                    };
                    LayerKind::Rect { x }
                } else {
                    let mut x = cg_core::wid::ellipse::Config {
                        fill_style: fs,
                        ..Default::default()
                    };
                    x.stroke_style = StrokeStyle {
                        position: StrokePosition::Center,
                        ..ss
                    };
                    LayerKind::Ellipse { x }
                };
                (kind, u, if tag == "rect" { "rect" } else { "ellipse" })
            }
            "line" | "polyline" | "polygon" | "path" => {
                let (kind, u) = self.pen(n, tag, &st, ov, fit, &m, hint, &mut props, &mut notes)?;
                (kind, u, "path")
            }
            "text" => {
                let (kind, u, lines) =
                    self.text(n, &st, ov, fit, &m, hint, &mut props, &mut notes)?;
                text = Some(lines);
                (kind, u, "text")
            }
            "image" => {
                let (x, y) = (g(self, "x").unwrap_or(0.0), g(self, "y").unwrap_or(0.0));
                let nat = self
                    .intrinsic
                    .get(&n.id())
                    .copied()
                    .unwrap_or((100.0, 100.0));
                let (w, h) = match (g(self, "width"), g(self, "height")) {
                    (Some(w), Some(h)) => (w, h),
                    (Some(w), None) => (w, w * nat.1 / nat.0.max(1e-9)),
                    (None, Some(h)) => (h * nat.0 / nat.1.max(1e-9), h),
                    (None, None) => nat,
                };
                let u = place(
                    &m,
                    [x, y, w.max(0.0), h.max(0.0)],
                    fit.u,
                    fit.pivot,
                    hint,
                    false,
                    &mut props,
                )?;
                let par = n.attribute("preserveAspectRatio").unwrap_or("");
                let fit_mode = if par.trim().starts_with("none") {
                    ImageFit::Stretch
                } else if par.contains("slice") {
                    ImageFit::Cover
                } else {
                    ImageFit::Contain
                };
                (
                    LayerKind::Image {
                        src: String::new(),
                        fit: fit_mode,
                        x: Default::default(),
                    },
                    u,
                    "image",
                )
            }
            _ => return Err(""),
        };
        Ok(Shape {
            kind,
            props,
            notes,
            u,
            source,
            text,
        })
    }

    /// Fill and stroke: the styles of the shape, its `fill` / `stroke` /
    /// `strokeWidth`, and whether each shows.
    #[allow(clippy::too_many_arguments)]
    fn paints(
        &mut self,
        st: &Style,
        ub: [f64; 4],
        map: &dyn Fn((f64, f64)) -> (f64, f64),
        size: (f64, f64),
        sw_scale: f64,
        props: &mut BTreeMap<Prop, Value>,
        notes: &mut Vec<Note>,
    ) -> Result<(FillStyle, StrokeStyle, bool, bool), &'static str> {
        let fill = st.get("fill").unwrap_or("black").to_string();
        let fo = st.get("fill-opacity").and_then(ratio).unwrap_or(1.0);
        let mut fs = FillStyle::default();
        let fill_on = match self.paint(&fill, st) {
            Paint::None => {
                props.insert(Prop::Fill, Value::Color(Color::TRANSPARENT));
                false
            }
            Paint::Color(c) => {
                props.insert(Prop::Fill, Value::Color(with_alpha(c, fo)));
                c.0[3] > 0.0 && fo > 0.0
            }
            Paint::Grad(g) => {
                let (style, first) = grad_fill(&g, fo, ub, map, size, notes);
                props.insert(Prop::Fill, Value::Color(first));
                fs = style;
                true
            }
            Paint::Pattern => return Err("Pattern fill"),
        };
        let stroke = st.get("stroke").unwrap_or("none").to_string();
        let so = st.get("stroke-opacity").and_then(ratio).unwrap_or(1.0);
        let sw = st
            .get("stroke-width")
            .and_then(|v| length(v, Axis::Other, self.root_px))
            .unwrap_or(1.0)
            .max(0.0);
        let stroke_color = match self.paint(&stroke, st) {
            Paint::None => None,
            Paint::Color(c) => Some(with_alpha(c, so)),
            Paint::Grad(g) => {
                push_note(notes, Note::new("Gradient → colour", Status::Partial));
                Some(with_alpha(g.stops[0].1, so))
            }
            Paint::Pattern => return Err("Pattern fill"),
        };
        let stroke_on = stroke_color.is_some_and(|c| c.0[3] > 0.0) && sw > 0.0;
        if let Some(c) = stroke_color {
            props.insert(Prop::Stroke, Value::Color(c));
            props.insert(Prop::StrokeWidth, Value::Number(round4(sw * sw_scale)));
        }
        let mut ss = StrokeStyle {
            cap: match st.get("stroke-linecap") {
                Some("round") => Cap::Round,
                Some("square") => Cap::Square,
                _ => Cap::Butt,
            },
            join: match st.get("stroke-linejoin") {
                Some("round") => Join::Round,
                Some("bevel") => Join::Bevel,
                _ => Join::Miter,
            },
            ..Default::default()
        };
        if stroke_on {
            if let Some(d) = st.get("stroke-dasharray").filter(|d| *d != "none") {
                let v = numbers(d);
                if v.iter().any(|x| *x > 0.0) {
                    ss.dash = if v[0] <= sw * 1.01 {
                        Dash::Dotted
                    } else {
                        Dash::Dashed
                    };
                    push_note(notes, Note::new("Dashed stroke", Status::Partial));
                }
            }
        }
        Ok((fs, ss, fill_on, stroke_on))
    }

    /// Lines, polylines, polygons and paths as a pen.
    #[allow(clippy::too_many_arguments)]
    fn pen(
        &mut self,
        n: Node,
        tag: &str,
        st: &Style,
        ov: &Ov,
        fit: &Fit,
        m: &Mat,
        hint: Option<f64>,
        props: &mut BTreeMap<Prop, Value>,
        notes: &mut Vec<Note>,
    ) -> Result<(LayerKind, (f64, f64)), &'static str> {
        let vp = fit.vp;
        let d = match tag {
            "path" => n.attribute("d").unwrap_or("").to_string(),
            "line" => {
                let g = |k: &str| self.len_attr(n, ov, k, vp).unwrap_or(0.0);
                format!("M{} {} L{} {}", g("x1"), g("y1"), g("x2"), g("y2"))
            }
            _ => {
                let v = numbers(n.attribute("points").unwrap_or(""));
                let mut d = String::new();
                for (i, p) in v.as_chunks::<2>().0.iter().enumerate() {
                    d.push_str(&format!(
                        "{}{} {} ",
                        if i == 0 { "M" } else { "L" },
                        p[0],
                        p[1]
                    ));
                }
                if tag == "polygon" && !d.is_empty() {
                    d.push('Z');
                }
                d
            }
        };
        let segs = match (tag, self.paths.get(&n.id())) {
            ("path", Some(s)) => s.clone(),
            _ => {
                let s = path_segments(&d).unwrap_or_default();
                if tag == "path" {
                    self.paths.insert(n.id(), s.clone());
                }
                s
            }
        };
        // Subpaths: points with their handles (absolute), closed.
        #[derive(Clone, Copy)]
        struct P3 {
            p: (f64, f64),
            i: (f64, f64),
            o: (f64, f64),
        }
        let at = |p: tiny_skia::Point| (p.x as f64, p.y as f64);
        let mut subs: Vec<(Vec<P3>, bool)> = Vec::new();
        let mut cur: Vec<P3> = Vec::new();
        let mut closed = false;
        for s in segs {
            use tiny_skia::PathSegment as S;
            match s {
                S::MoveTo(p) => {
                    if cur.len() > 1 {
                        subs.push((std::mem::take(&mut cur), closed));
                    }
                    let p = at(p);
                    cur = vec![P3 { p, i: p, o: p }];
                    closed = false;
                }
                S::LineTo(p) => {
                    let p = at(p);
                    cur.push(P3 { p, i: p, o: p });
                }
                S::QuadTo(c, p) => {
                    let (c, p) = (at(c), at(p));
                    if let Some(last) = cur.last_mut() {
                        let a = last.p;
                        last.o = (a.0 + 2.0 / 3.0 * (c.0 - a.0), a.1 + 2.0 / 3.0 * (c.1 - a.1));
                    }
                    cur.push(P3 {
                        p,
                        i: (p.0 + 2.0 / 3.0 * (c.0 - p.0), p.1 + 2.0 / 3.0 * (c.1 - p.1)),
                        o: p,
                    });
                }
                S::CubicTo(c1, c2, p) => {
                    if let Some(last) = cur.last_mut() {
                        last.o = at(c1);
                    }
                    let p = at(p);
                    cur.push(P3 { p, i: at(c2), o: p });
                }
                S::Close => closed = true,
            }
        }
        if cur.len() > 1 {
            subs.push((cur, closed));
        }
        let (mut pts, mut closed) = match subs.len() {
            0 => return Err(""),
            1 => subs.remove(0),
            _ => return Err("Compound path"),
        };
        let fill_on = !matches!(
            self.paint(st.get("fill").unwrap_or("black"), st),
            Paint::None
        );
        let stroke_on = !matches!(
            self.paint(st.get("stroke").unwrap_or("none"), st),
            Paint::None
        );
        let near =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
        let ends_at_start = pts.len() > 2 && near(pts[0].p, pts[pts.len() - 1].p);
        if closed || (fill_on && ends_at_start) {
            if ends_at_start {
                let last = pts.pop().expect("more than 2 points");
                pts[0].i = last.i;
            }
            closed = true;
        } else if fill_on && pts.len() > 2 {
            if stroke_on {
                return Err("Open filled path");
            }
            closed = true;
        }
        let tight = |pts: &[P3]| {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(pts[0].p.0 as f32, pts[0].p.1 as f32);
            let n_seg = if closed { pts.len() } else { pts.len() - 1 };
            for k in 0..n_seg {
                let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                pb.cubic_to(
                    a.o.0 as f32,
                    a.o.1 as f32,
                    b.i.0 as f32,
                    b.i.1 as f32,
                    b.p.0 as f32,
                    b.p.1 as f32,
                );
            }
            pb.finish()
                .and_then(|p| p.compute_tight_bounds())
                .map(|r| {
                    [
                        r.x() as f64,
                        r.y() as f64,
                        r.width() as f64,
                        r.height() as f64,
                    ]
                })
                .unwrap_or([pts[0].p.0, pts[0].p.1, 0.0, 0.0])
        };
        // The user space box (for gradients), then the points in the parent
        // layer's space (baked) or in the user space.
        let ub = tight(&pts);
        let ub = [ub[0], ub[1], ub[2].max(1e-9), ub[3].max(1e-9)];
        if fit.bake {
            for p in &mut pts {
                p.p = pt(m, p.p);
                p.i = pt(m, p.i);
                p.o = pt(m, p.o);
            }
        }
        let bounds = tight(&pts);
        let (u, map_scale, b): ((f64, f64), f64, [f64; 4]) = if fit.bake {
            let b = [bounds[0], bounds[1], bounds[2].max(1.0), bounds[3].max(1.0)];
            for (k, v) in [
                (Prop::X, b[0]),
                (Prop::Y, b[1]),
                (Prop::Width, b[2]),
                (Prop::Height, b[3]),
            ] {
                props.insert(k, Value::Number(round4(v)));
            }
            ((1.0, 1.0), det(m).abs().sqrt(), b)
        } else {
            let d0 = decompose(m);
            let u0 = fit.u.unwrap_or((d0.sx.max(1e-9), d0.sy.abs().max(1e-9)));
            let b = [
                bounds[0],
                bounds[1],
                bounds[2].max(1.0 / u0.0),
                bounds[3].max(1.0 / u0.1),
            ];
            let u = place(m, b, Some(u0), fit.pivot, hint, false, props)?;
            (u, (u.0 * u.1).sqrt(), b)
        };
        let frac = |q: (f64, f64)| ((q.0 - b[0]) / b[2], (q.1 - b[1]) / b[3]);
        let points = pts
            .iter()
            .map(|p| {
                let (x, y) = frac(p.p);
                let (ix, iy) = ((p.i.0 - p.p.0) / b[2], (p.i.1 - p.p.1) / b[3]);
                let (ox, oy) = ((p.o.0 - p.p.0) / b[2], (p.o.1 - p.p.1) / b[3]);
                let curve = [ix, iy, ox, oy].iter().any(|v| v.abs() > 1e-9);
                PenPoint {
                    x: fin32(x),
                    y: fin32(y),
                    ix: fin32(ix),
                    iy: fin32(iy),
                    ox: fin32(ox),
                    oy: fin32(oy),
                    kind: if curve {
                        PointType::Curve
                    } else {
                        PointType::Corner
                    },
                    mode: HandleMode::Broken,
                }
            })
            .collect();
        let (w, h) = (b[2] * u.0, b[3] * u.1);
        // Gradients: user space → layer pixels.
        let off = (b[0], b[1]);
        let mm = *m;
        let bake = fit.bake;
        let map = move |q: (f64, f64)| {
            if bake {
                let r = pt(&mm, q);
                (r.0 - off.0, r.1 - off.1)
            } else {
                ((q.0 - off.0) * u.0, (q.1 - off.1) * u.1)
            }
        };
        let (fs, ss, fill_vis, _) = self.paints(st, ub, &map, (w, h), map_scale, props, notes)?;
        let rule = if st.get("fill-rule") == Some("evenodd") {
            FillRule::Evenodd
        } else {
            FillRule::Nonzero
        };
        let x = cg_core::wid::pen::Config {
            points,
            closed,
            stroke_style: ss,
            fixed_width: true,
            ref_w: fin32(w),
            ref_h: fin32(h),
            arrow_start: Arrow::None,
            arrow_end: Arrow::None,
            fill_on: closed && (fill_vis || fill_on),
            fill_style: fs,
            fill_rule: rule,
            draw_in: false,
            ..Default::default()
        };
        Ok((LayerKind::Pen { x }, u))
    }

    /// A text (its tspans as lines when they move to a new line).
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        n: Node,
        st: &Style,
        ov: &Ov,
        fit: &Fit,
        m: &Mat,
        hint: Option<f64>,
        props: &mut BTreeMap<Prop, Value>,
        notes: &mut Vec<Note>,
    ) -> Result<TextOut, &'static str> {
        let vp = fit.vp;
        let mut tl = TextLines::default();
        let preserve =
            n.ancestors().find_map(|a| a.attribute((XML_NS, "space"))) == Some("preserve");
        self.text_lines(n, &mut tl, 0);
        let lines: Vec<String> = tl
            .lines
            .iter()
            .map(|l| {
                if preserve {
                    l.clone()
                } else {
                    l.split_whitespace().collect::<Vec<_>>().join(" ")
                }
            })
            .collect();
        let mut lines: Vec<String> = lines.into_iter().filter(|l| !l.is_empty()).collect();
        if lines.is_empty() {
            return Err("");
        }
        lines.truncate(200);
        if tl.mixed {
            push_note(notes, Note::new("Mixed text styles", Status::Partial));
        }
        if tl.on_path {
            push_note(notes, Note::new("Text on a path", Status::Partial));
        }
        let first = |k: &str| {
            self.attr(n, ov, k)
                .and_then(|v| numbers_len(v, axis_of(k), vp))
                .or_else(|| tl.first.get(k).copied())
        };
        let x = first("x").unwrap_or(0.0) + first("dx").unwrap_or(0.0);
        let y = first("y").unwrap_or(0.0) + first("dy").unwrap_or(0.0);
        let size = st
            .get("font-size")
            .and_then(font_size)
            .unwrap_or(16.0)
            .max(0.1);
        let lh = match tl.ys.as_slice() {
            [a, b, ..] if b > a => ((b - a) / size).clamp(0.5, 4.0),
            _ => 1.2,
        };
        let family_raw = st
            .get("font-family")
            .unwrap_or("")
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(['"', '\''])
            .to_string();
        let generic = matches!(
            family_raw.to_ascii_lowercase().as_str(),
            "serif" | "sans-serif" | "monospace" | "cursive" | "fantasy" | "system-ui"
        );
        let (family, found) = if generic {
            ("Inter".to_string(), true)
        } else {
            self.ctx.font(&family_raw)
        };
        if !found {
            push_note(
                notes,
                Note::new("Font {font} → {replacement}", Status::Partial)
                    .with("font", family_raw.clone())
                    .with("replacement", family.clone()),
            );
        }
        let weight = match st.get("font-weight").unwrap_or("normal") {
            "bold" | "bolder" => 700,
            "lighter" => 300,
            w => w.parse::<u16>().unwrap_or(400).clamp(100, 900),
        };
        let align = match st.get("text-anchor") {
            Some("middle") => TextAlign::Center,
            Some("end") => TextAlign::Right,
            _ => TextAlign::Left,
        };
        let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
        let w = (longest * size * 0.62).max(size);
        let x0 = match align {
            TextAlign::Left => x,
            TextAlign::Center => x - w / 2.0,
            TextAlign::Right => x - w,
        };
        let db = st
            .get("dominant-baseline")
            .or(st.get("alignment-baseline"))
            .unwrap_or("auto");
        let baseline = y + size
            * match db {
                "middle" | "central" => 0.35,
                "hanging" | "text-before-edge" | "text-top" => 0.8,
                "text-after-edge" | "text-bottom" | "ideographic" => -0.24,
                _ => 0.0,
            };
        // The first baseline in a box of `lh` lines (the renderer's metrics).
        let top = baseline - size * (lh / 2.0 + 0.364);
        let b = [x0, top, w, size * lh * lines.len() as f64];
        let u = match fit.u {
            Some(u) => u,
            None => {
                let d = decompose(m);
                let s = d.sy.abs().max(1e-9);
                (s, s)
            }
        };
        let u = place(m, b, Some(u), fit.pivot, hint, false, props)?;
        props.insert(Prop::FontSize, Value::Number(round4(size * u.1)));
        if let Some(ls) = st
            .get("letter-spacing")
            .filter(|v| *v != "normal")
            .and_then(|v| length(v, Axis::Other, (size, size)))
        {
            if ls.abs() > 1e-6 {
                props.insert(Prop::LetterSpacing, Value::Number(round4(ls * u.0)));
            }
        }
        // Colours: a gradient gives its first colour.
        let fill = st.get("fill").unwrap_or("black").to_string();
        let fo = st.get("fill-opacity").and_then(ratio).unwrap_or(1.0);
        let fill_c = match self.paint(&fill, st) {
            Paint::None => Color::TRANSPARENT,
            Paint::Color(c) => with_alpha(c, fo),
            Paint::Grad(g) => {
                push_note(notes, Note::new("Gradient → colour", Status::Partial));
                with_alpha(g.stops[0].1, fo)
            }
            Paint::Pattern => return Err("Pattern fill"),
        };
        props.insert(Prop::Fill, Value::Color(fill_c));
        let stroke = st.get("stroke").unwrap_or("none").to_string();
        if let Paint::Color(c) = self.paint(&stroke, st) {
            let so = st.get("stroke-opacity").and_then(ratio).unwrap_or(1.0);
            let sw = st
                .get("stroke-width")
                .and_then(|v| length(v, Axis::Other, self.root_px))
                .unwrap_or(1.0);
            props.insert(Prop::Stroke, Value::Color(with_alpha(c, so)));
            props.insert(Prop::StrokeWidth, Value::Number(round4(sw * u.1)));
        }
        let decoration = match st.get("text-decoration") {
            Some(d) if d.contains("underline") => TextDecoration::Underline,
            Some(d) if d.contains("line-through") => TextDecoration::LineThrough,
            _ => TextDecoration::None,
        };
        let x = cg_core::wid::text::Config {
            italic: matches!(st.get("font-style"), Some("italic" | "oblique")),
            ..Default::default()
        };
        let kind = LayerKind::Text {
            x,
            wrap: false,
            transform: Default::default(),
            decoration,
            line_height: (lines.len() > 1 && (lh - 1.2).abs() > 0.01).then_some(fin32(lh)),
            scroll: Default::default(),
            speed: 120.0,
            separator: "   •   ".into(),
            looping: false,
            overflow: Default::default(),
            squeeze: 0.8,
            min_size: 0.6,
            warn_below: 0.8,
            text: String::new(),
            font_family: family,
            font_weight: weight,
            align,
            vertical_align: VerticalAlign::Top,
            uppercase: false,
        };
        Ok((kind, u, lines))
    }

    fn text_lines(&self, n: Node, tl: &mut TextLines, depth: u32) {
        if depth > 16 {
            return;
        }
        for c in n.children() {
            if let Some(t) = c.text().filter(|_| c.is_text()) {
                let t = t.replace(['\n', '\r', '\t'], " ");
                match tl.lines.last_mut() {
                    Some(l) => l.push_str(&t),
                    None => tl.lines.push(t),
                }
                continue;
            }
            if !c.is_element() || !svg_ns(c) {
                continue;
            }
            match c.tag_name().name() {
                "tspan" | "textPath" | "a" | "tref" => {
                    if c.tag_name().name() == "textPath" {
                        tl.on_path = true;
                    }
                    let vp = self.root_px;
                    let y = c.attribute("y").and_then(|v| numbers_len(v, Axis::Y, vp));
                    let dy = c.attribute("dy").and_then(|v| numbers_len(v, Axis::Y, vp));
                    let x = c.attribute("x").and_then(|v| numbers_len(v, Axis::X, vp));
                    let has_text = tl.lines.last().is_some_and(|l| !l.trim().is_empty());
                    if (y.is_some() || x.is_some() || dy.is_some_and(|d| d != 0.0)) && has_text {
                        let prev = tl
                            .ys
                            .last()
                            .copied()
                            .or(tl.first.get("y").copied())
                            .unwrap_or(0.0);
                        let ny = y.unwrap_or(prev) + dy.unwrap_or(0.0);
                        if tl.ys.is_empty() {
                            tl.ys.push(prev);
                        }
                        tl.ys.push(ny);
                        tl.lines.push(String::new());
                    } else if !has_text {
                        for (k, v) in [("x", x), ("y", y), ("dy", dy)] {
                            if let Some(v) = v {
                                tl.first.entry(k).or_insert(v);
                            }
                        }
                    }
                    if self.decls(c).iter().any(|(k, _)| {
                        matches!(
                            k.as_str(),
                            "fill" | "font-size" | "font-weight" | "font-family" | "font-style"
                        )
                    }) {
                        tl.mixed = true;
                    }
                    self.text_lines(c, tl, depth + 1);
                }
                _ => {}
            }
        }
    }

    // ----------------------------------------------------------- images --

    /// Reads the picture of an `image`; false when it is not imported.
    fn load_image(&mut self, n: Node, name: &str) -> bool {
        match image_data(href(n).unwrap_or("")) {
            Ok((data, ext)) => {
                let size = if ext == "svg" {
                    usvg::Tree::from_data(&data, &usvg::Options::default())
                        .ok()
                        .map(|t| (t.size().width() as f64, t.size().height() as f64))
                } else {
                    image::ImageReader::new(std::io::Cursor::new(&data))
                        .with_guessed_format()
                        .ok()
                        .and_then(|r| r.into_dimensions().ok())
                        .map(|(w, h)| (w as f64, h as f64))
                };
                if let Some(s) = size {
                    self.intrinsic.insert(n.id(), s);
                }
                let file = format!("{}-img{}.{ext}", self.slug, self.files.len() + 1);
                self.files.push((file.clone(), data));
                self.image_files.insert(n.id(), file);
                true
            }
            Err(reason) => {
                self.ignored.push(Ignored::new(name, "image", reason));
                false
            }
        }
    }

    fn image_file(&self, n: Node) -> Option<String> {
        self.image_files.get(&n.id()).cloned()
    }

    // ------------------------------------------------------- rasterising --

    /// The element drawn as a picture (with resvg), as an image layer.
    fn rasterised(
        &mut self,
        n: Node<'a, 'i>,
        tag: &str,
        sc: &Scope,
        st: &Style,
        name: String,
        cause: &str,
    ) {
        let mut notes = vec![
            Note::new("Rasterised", Status::Partial),
            Note::new(cause, Status::Partial),
        ];
        notes.extend(self.common_notes(st, true));
        let anims = self.anims_of(n, tag, true, &mut notes);
        let vis_anim = anims
            .iter()
            .any(|&i| matches!(&self.smils[i].kind, SKind::Attr(a) if a == "visibility" || a == "display"));
        if n.descendants().any(|d| is_svg(d, "text"))
            || (tag == "use"
                && href(n)
                    .and_then(url_id)
                    .and_then(|id| self.ids.get(id))
                    .is_some_and(|t| t.descendants().any(|d| is_svg(d, "text"))))
        {
            notes.push(Note::new("Text left out", Status::Partial));
        }
        let Some((file, [x, y, w, h])) = self.raster(n, sc, st) else {
            self.ignored
                .push(Ignored::new(&name, tag, "Nothing to draw"));
            return;
        };
        let id = new_id();
        let mut l = layer(
            &id,
            &name,
            Some(&sc.parent),
            LayerKind::Image {
                src: format!("{}{file}", self.ctx.prefix),
                fit: ImageFit::Stretch,
                x: Default::default(),
            },
        );
        for (k, v) in [
            (Prop::X, x),
            (Prop::Y, y),
            (Prop::Width, w),
            (Prop::Height, h),
        ] {
            l.props.insert(k, Value::Number(round4(v)));
        }
        let o = self.opacity(st, vis_anim);
        if o < 1.0 || vis_anim {
            l.props.insert(Prop::Opacity, Value::Number(round4(o)));
        }
        if !anims.is_empty() {
            let base_tf = self.el_transform(n, &Ov::default());
            let st2 = st.clone();
            let (stat, tracks) = self.animate(&anims, base_tf, |cv, ov| {
                let mut p = BTreeMap::new();
                p.insert(
                    Prop::Opacity,
                    Value::Number(round4(cv.opacity(&st2.over(ov), vis_anim))),
                );
                p
            });
            for p in tracks.keys() {
                l.props.remove(p);
            }
            l.props.extend(stat);
            self.anim_notes(&anims, &tracks, &mut notes);
            l.tracks = tracks;
        }
        let hidden = st.hidden() && !vis_anim;
        self.layers.push(l);
        self.report(sc, &id, &name, "image", notes, hidden);
    }

    /// Renders an element (in its parent layer's space) into a PNG: the
    /// file and its box.
    fn raster(&mut self, n: Node, sc: &Scope, st: &Style) -> Option<(String, [f64; 4])> {
        if self.rasters >= MAX_RASTERS {
            return None;
        }
        let css = self.css();
        let mut doc = format!(
            r#"<svg xmlns="{SVG_NS}" xmlns:xlink="{XLINK_NS}" width="{}" height="{}">"#,
            self.root_px.0, self.root_px.1
        );
        doc.push_str(&css);
        // The gradients, clip paths, symbols… it refers to.
        let mut refs = Vec::new();
        self.refs(n, &mut refs, 0);
        for id in ids_in(&css) {
            if let Some(t) = self.ids.get(id) {
                if !refs.contains(&t.id()) {
                    refs.push(t.id());
                    self.refs(*t, &mut refs, 1);
                }
            }
        }
        if !refs.is_empty() {
            doc.push_str("<defs>");
            for r in refs {
                if let Some(t) = self.doc.get_node(r) {
                    ser(t, &mut doc, "", 0);
                }
            }
            doc.push_str("</defs>");
        }
        let p = sc.p;
        doc.push_str(&format!(
            r#"<g transform="matrix({} {} {} {} {} {})" style=""#,
            p[0], p[1], p[2], p[3], p[4], p[5]
        ));
        let mut inherited = String::new();
        xml_escape(&sc.style.inherited_css(), &mut inherited);
        doc.push_str(&inherited);
        doc.push_str("\">");
        let extra = if st.hidden() {
            "opacity:1;display:inline;visibility:visible"
        } else {
            "opacity:1"
        };
        ser(n, &mut doc, extra, 0);
        doc.push_str("</g></svg>");
        let tree = usvg::Tree::from_str(&doc, &usvg::Options::default()).ok()?;
        if !tree.root().has_children() {
            return None;
        }
        let bb = tree.root().abs_layer_bounding_box();
        // What lies far outside the picture is not drawn.
        let lim = 4.0 * self.root_px.0.max(self.root_px.1);
        let (x0, y0) = (
            (bb.left().floor() as f64).max(-lim),
            (bb.top().floor() as f64).max(-lim),
        );
        let (x1, y1) = (
            (bb.right().ceil() as f64).min(lim),
            (bb.bottom().ceil() as f64).min(lim),
        );
        let (w, h) = (x1 - x0, y1 - y0);
        if !(w >= 1.0 && h >= 1.0) || !w.is_finite() || !h.is_finite() {
            return None;
        }
        let k = (MAX_RASTER_PX / (w * h))
            .sqrt()
            .min(1.0)
            .min(8192.0 / w.max(h));
        let (pw, ph) = (
            (w * k).ceil().max(1.0) as u32,
            (h * k).ceil().max(1.0) as u32,
        );
        let mut pm = tiny_skia::Pixmap::new(pw, ph)?;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(k as f32, k as f32)
                .pre_translate(-x0 as f32, -y0 as f32),
            &mut pm.as_mut(),
        );
        let png = pm.encode_png().ok()?;
        self.rasters += 1;
        let file = format!("{}-{}.png", self.slug, self.files.len() + 1);
        self.files.push((file.clone(), png));
        Some((file, [x0, y0, w, h]))
    }

    /// The style sheets of the document, for the pictures.
    fn css(&mut self) -> String {
        if let Some(r) = &self.css {
            return r.clone();
        }
        let css: String = self
            .doc
            .descendants()
            .filter(|n| is_svg(*n, "style"))
            .flat_map(|s| s.children().filter_map(|c| c.text()))
            .collect();
        let mut out = String::new();
        if !css.trim().is_empty() {
            out.push_str("<style>");
            xml_escape(&css, &mut out);
            out.push_str("</style>");
        }
        self.css = Some(out.clone());
        out
    }

    /// Elements a subtree refers to (`url(#…)`, `href`), and those they
    /// refer to, outside the subtree.
    fn refs(&self, n: Node, out: &mut Vec<NodeId>, depth: u32) {
        if depth > 16 || out.len() > 256 {
            return;
        }
        for d in n.descendants().filter(|d| d.is_element()) {
            for a in d.attributes() {
                let v = a.value();
                let found: Vec<&str> = if a.name() == "href" {
                    v.trim().strip_prefix('#').into_iter().collect()
                } else if v.contains("url(") {
                    ids_in(v)
                } else {
                    Vec::new()
                };
                for id in found {
                    let Some(t) = self.ids.get(id) else { continue };
                    if out.contains(&t.id()) || n.descendants().any(|x| x == *t) {
                        continue;
                    }
                    out.push(t.id());
                    self.refs(*t, out, depth + 1);
                }
            }
        }
    }
}

/// The ids of the `url(#id)` in a text.
fn ids_in(s: &str) -> Vec<&str> {
    s.match_indices("url(")
        .filter_map(|(i, _)| url_id(&s[i..]))
        .collect()
}

/// A text layer: kind, scale baked in, lines.
type TextOut = (LayerKind, (f64, f64), Vec<String>);

#[derive(Default)]
struct TextLines {
    lines: Vec<String>,
    /// Baselines of the lines after the first (with the first's).
    ys: Vec<f64>,
    /// Position given by the first tspan.
    first: HashMap<&'static str, f64>,
    mixed: bool,
    on_path: bool,
}

/// A font size in user units (relative sizes from 16).
fn font_size(v: &str) -> Option<f64> {
    let v = v.trim();
    match v {
        "xx-small" => Some(9.0),
        "x-small" => Some(10.0),
        "small" => Some(13.0),
        "medium" => Some(16.0),
        "large" => Some(18.0),
        "x-large" => Some(24.0),
        "xx-large" => Some(32.0),
        _ if v.ends_with('%') => scan_number(v).map(|x| 16.0 * x.0 / 100.0),
        _ => length(v, Axis::Other, (16.0, 16.0)),
    }
}

/// The first length of a list attribute.
fn numbers_len(v: &str, axis: Axis, vp: (f64, f64)) -> Option<f64> {
    length(v.split([' ', ',']).find(|s| !s.is_empty())?, axis, vp)
}

fn is_svg(n: Node, tag: &str) -> bool {
    n.is_element() && n.tag_name().name() == tag && svg_ns(n)
}

/// In the SVG namespace (or in none, for files without `xmlns`).
fn svg_ns(n: Node) -> bool {
    matches!(n.tag_name().namespace(), Some(SVG_NS) | None)
}

fn is_color_attr(a: &str) -> bool {
    matches!(
        a,
        "fill" | "stroke" | "color" | "stop-color" | "flood-color" | "lighting-color"
    )
}

/// The value of an attribute an animation starts from when not given.
fn default_attr(a: &str) -> &'static str {
    match a {
        "opacity" | "fill-opacity" | "stroke-opacity" => "1",
        "fill" => "black",
        "stroke" => "none",
        "visibility" => "visible",
        "display" => "inline",
        "stroke-width" => "1",
        "font-size" => "16",
        _ => "0",
    }
}

/// Attributes an element's layer follows when animated.
fn supported_attr(tag: &str, a: &str) -> bool {
    let paint = matches!(
        a,
        "fill" | "stroke" | "fill-opacity" | "stroke-opacity" | "stroke-width" | "color"
    );
    match tag {
        "rect" => paint || matches!(a, "x" | "y" | "width" | "height" | "rx" | "ry"),
        "circle" => paint || matches!(a, "cx" | "cy" | "r"),
        "ellipse" => paint || matches!(a, "cx" | "cy" | "rx" | "ry"),
        "line" | "polyline" | "polygon" | "path" => paint,
        "text" => paint || matches!(a, "x" | "y" | "font-size" | "letter-spacing"),
        "image" => matches!(a, "x" | "y" | "width" | "height"),
        _ => false,
    }
}

fn push_note(notes: &mut Vec<Note>, n: Note) {
    if !notes
        .iter()
        .any(|x| x.label == n.label && x.params == n.params)
    {
        notes.push(n);
    }
}

fn same(a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => (a - b).abs() < 1e-4,
        (Value::Color(a), Value::Color(b)) => {
            a.0.iter().zip(b.0).all(|(x, y)| (x - y).abs() < 1e-3)
        }
        _ => false,
    }
}

fn layer(id: &str, name: &str, parent: Option<&str>, kind: LayerKind) -> Layer {
    Layer {
        id: id.to_string(),
        name: name.to_string(),
        parent: parent.map(str::to_string),
        kind,
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

fn group_kind(clip: bool) -> LayerKind {
    LayerKind::Group { clip, lottie: None }
}

/// Places the box `b` (user space) by `m` in the parent layer: position,
/// size (scaled by `u`, else by the matrix), rotation, the rest of the
/// scale, and the anchor at `pivot` (default: the centre; `corner`: the
/// box's corner, for groups, whose skew is then left out). Fails on a skew.
fn place(
    m: &Mat,
    b: [f64; 4],
    u: Option<(f64, f64)>,
    pivot: Option<(f64, f64)>,
    hint: Option<f64>,
    corner: bool,
    props: &mut BTreeMap<Prop, Value>,
) -> Result<(f64, f64), &'static str> {
    let d = decompose(m);
    if d.skew && !corner {
        return Err("Skew");
    }
    let rot = match hint {
        Some(h) => unwrap_deg(d.rot, h),
        None => d.rot,
    };
    let u = u.unwrap_or((d.sx.max(1e-9), d.sy.abs().max(1e-9)));
    let (w, h) = (b[2] * u.0, b[3] * u.1);
    let q0 = pivot.unwrap_or(if corner {
        (b[0], b[1])
    } else {
        (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0)
    });
    let a = ((q0.0 - b[0]) * u.0, (q0.1 - b[1]) * u.1);
    let c = pt(m, q0);
    props.insert(Prop::X, Value::Number(round4(c.0 - a.0)));
    props.insert(Prop::Y, Value::Number(round4(c.1 - a.1)));
    props.insert(Prop::Width, Value::Number(round4(w)));
    props.insert(Prop::Height, Value::Number(round4(h)));
    if rot.abs() > 1e-6 || hint.is_some() {
        props.insert(Prop::Rotation, Value::Number(round4(rot)));
    }
    let (rx, ry) = (d.sx / u.0, d.sy / u.1);
    if (rx - 1.0).abs() > 1e-6 || hint.is_some() {
        props.insert(Prop::ScaleX, Value::Number(round4(rx)));
    }
    if (ry - 1.0).abs() > 1e-6 || hint.is_some() {
        props.insert(Prop::ScaleY, Value::Number(round4(ry)));
    }
    if pivot.is_some() || corner {
        let fx = if w > 1e-9 { a.0 / w } else { 0.5 };
        let fy = if h > 1e-9 { a.1 / h } else { 0.5 };
        props.insert(Prop::AnchorX, Value::Number(round4(fx)));
        props.insert(Prop::AnchorY, Value::Number(round4(fy)));
    }
    Ok(u)
}

/// The view box `vb` fitted in `size` as `preserveAspectRatio` says.
fn view_box(vb: [f64; 4], size: (f64, f64), par: &str) -> Mat {
    let mut words = par.split_whitespace().filter(|w| *w != "defer");
    let align = words.next().unwrap_or("xMidYMid");
    let slice = words.next() == Some("slice");
    let (sx, sy) = (size.0 / vb[2], size.1 / vb[3]);
    if align == "none" {
        return mul(&scale(sx, sy), &translate(-vb[0], -vb[1]));
    }
    let s = if slice { sx.max(sy) } else { sx.min(sy) };
    let f = |key: &str, mid: &str| {
        if align.contains(key) {
            0.0
        } else if align.contains(mid) {
            0.5
        } else if align.len() >= 8 {
            1.0
        } else {
            0.5
        }
    };
    let tx = (size.0 - vb[2] * s) * f("xMin", "xMid");
    let ty = (size.1 - vb[3] * s) * f("YMin", "YMid");
    mul(
        &mul(&translate(tx, ty), &scale(s, s)),
        &translate(-vb[0], -vb[1]),
    )
}

/// Path data as segments (absolute, arcs as curves), read by usvg.
fn path_segments(d: &str) -> Option<Vec<tiny_skia::PathSegment>> {
    if d.trim().is_empty() || d.len() > 8 << 20 {
        return None;
    }
    let mut doc = format!(r##"<svg xmlns="{SVG_NS}" width="1" height="1"><path fill="#000" d=""##);
    xml_escape(d, &mut doc);
    doc.push_str("\"/></svg>");
    let tree = usvg::Tree::from_str(&doc, &usvg::Options::default()).ok()?;
    first_path(tree.root()).map(|p| p.data().segments().collect())
}

/// The first path of a usvg tree.
fn first_path(g: &usvg::Group) -> Option<&usvg::Path> {
    g.children().iter().find_map(|c| match c {
        usvg::Node::Path(p) => Some(p.as_ref()),
        usvg::Node::Group(g) => first_path(g),
        _ => None,
    })
}

/// A colour usvg reads (names, `rgb()`, `hsl()`…).
fn usvg_color(s: &str) -> Option<Color> {
    if s.len() > 64
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " ,.%()#+-".contains(c))
    {
        return None;
    }
    let doc = format!(
        r##"<svg xmlns="{SVG_NS}" width="1" height="1"><g fill="#010203"><rect width="1" height="1" fill="{s}"/></g></svg>"##
    );
    let tree = usvg::Tree::from_str(&doc, &usvg::Options::default()).ok()?;
    let f = first_path(tree.root())?.fill()?;
    let usvg::Paint::Color(c) = f.paint() else {
        return None;
    };
    if (c.red, c.green, c.blue) == (1, 2, 3) {
        return None;
    }
    Some(Color([
        c.red as f32 / 255.0,
        c.green as f32 / 255.0,
        c.blue as f32 / 255.0,
        f.opacity().get(),
    ]))
}

/// A colour: `#hex`, else what usvg reads.
fn parse_color(s: &str) -> Option<Color> {
    let l = s.trim().to_ascii_lowercase();
    if l == "transparent" {
        return Some(Color::TRANSPARENT);
    }
    if let Some(h) = l.strip_prefix('#') {
        if h.len() == 4 {
            let x: String = h.chars().flat_map(|c| [c, c]).collect();
            return Color::from_hex(&x);
        }
        return Color::from_hex(h);
    }
    usvg_color(&l)
}

/// A gradient fill as the shape widgets draw it (CSS-like line across
/// the box, or a circle from the centre): its style and the `fill` colour.
fn grad_fill(
    g: &Grad,
    opacity: f64,
    ub: [f64; 4],
    map: &dyn Fn((f64, f64)) -> (f64, f64),
    (w, h): (f64, f64),
    notes: &mut Vec<Note>,
) -> (FillStyle, Color) {
    let col = |c: Color| with_alpha(c, opacity);
    let first = col(g.stops[0].1);
    let last = col(g.stops[g.stops.len() - 1].1);
    // A point of the gradient in the layer's pixels.
    let to_local = |p: (f64, f64)| {
        let q = pt(&g.tf, p);
        let user = if g.bbox {
            (ub[0] + q.0 * ub[2], ub[1] + q.1 * ub[3])
        } else {
            q
        };
        map(user)
    };
    let mut simplified = !g.pad;
    let dt = decompose(&g.tf);
    if dt.skew || (dt.sx - dt.sy.abs()).abs() > 1e-3 * dt.sx.max(1e-9) {
        simplified = true;
    }
    let mut fs = FillStyle {
        color2: last.to_hex(),
        ..Default::default()
    };
    let solid = |fs: &mut FillStyle| {
        fs.kind = FillKind::Solid;
    };
    if g.radial {
        let c = to_local((g.c[0], g.c[1]));
        let rx = to_local((g.c[0] + g.c[2], g.c[1]));
        let ry = to_local((g.c[0], g.c[1] + g.c[2]));
        let (dx, dy) = (
            (rx.0 - c.0).hypot(rx.1 - c.1),
            (ry.0 - c.0).hypot(ry.1 - c.1),
        );
        let r = (dx * dy).sqrt();
        let big = (w * w + h * h).sqrt() / 2.0;
        if (dx - dy).abs() > 0.05 * dx.max(dy)
            || (c.0 - w / 2.0).hypot(c.1 - h / 2.0) > 0.02 * big.max(1.0)
            || (g.c[3] - g.c[0]).abs() > 1e-6
            || (g.c[4] - g.c[1]).abs() > 1e-6
        {
            simplified = true;
        }
        if big < 1e-9 || r < 1e-9 {
            solid(&mut fs);
            return (fs, last);
        }
        fs.kind = FillKind::Radial;
        fs.stops = g
            .stops
            .iter()
            .map(|(o, c)| GradientStop {
                at: fin32((o * r / big).clamp(0.0, 1.0) * 100.0),
                color: col(*c).to_hex(),
            })
            .collect();
    } else {
        let p1 = to_local((g.c[0], g.c[1]));
        let p2 = to_local((g.c[2], g.c[3]));
        let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
        if dx.hypot(dy) < 1e-9 {
            solid(&mut fs);
            return (fs, last);
        }
        let a = dx.atan2(-dy);
        let len = (w * a.sin()).abs() + (h * a.cos()).abs();
        if len < 1e-9 {
            solid(&mut fs);
            return (fs, first);
        }
        let d = (a.sin(), -a.cos());
        let s = (w / 2.0 - d.0 * len / 2.0, h / 2.0 - d.1 * len / 2.0);
        let t = |p: (f64, f64)| ((p.0 - s.0) * d.0 + (p.1 - s.1) * d.1) / len;
        let (t1, t2) = (t(p1), t(p2));
        fs.kind = FillKind::Linear;
        fs.angle = fin32(a.to_degrees().rem_euclid(360.0));
        fs.stops = g
            .stops
            .iter()
            .map(|(o, c)| GradientStop {
                at: fin32((t1 + o * (t2 - t1)).clamp(0.0, 1.0) * 100.0),
                color: col(*c).to_hex(),
            })
            .collect();
    }
    if simplified {
        push_note(notes, Note::new("Gradient simplified", Status::Partial));
    }
    (fs, first)
}

/// The picture of an `image`: bytes and extension, or why not imported.
fn image_data(h: &str) -> Result<(Vec<u8>, &'static str), &'static str> {
    let h = h.trim();
    let Some(rest) = h.strip_prefix("data:") else {
        return Err("External picture");
    };
    let (meta, data) = rest.split_once(',').ok_or("Unreadable picture")?;
    let mime = meta
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let bytes = if meta.contains(";base64") {
        let clean: String = data.chars().filter(|c| !c.is_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(clean.trim_end_matches('='))
            .or_else(|_| {
                base64::engine::general_purpose::STANDARD_NO_PAD.decode(clean.trim_end_matches('='))
            })
            .map_err(|_| "Unreadable picture")?
    } else {
        percent_decode(data)
    };
    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => match bytes.get(..4) {
            Some([0x89, b'P', b'N', b'G']) => "png",
            Some([0xff, 0xd8, ..]) => "jpg",
            _ => return Err("Unreadable picture"),
        },
    };
    if bytes.is_empty() {
        return Err("Unreadable picture");
    }
    Ok((bytes, ext))
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Writes an element (and its content) as SVG; `extra` is added to its
/// `style`. Foreign elements and scripts are left out.
fn ser(n: Node, out: &mut String, extra: &str, depth: u32) {
    if depth > MAX_DEPTH * 2 || out.len() > 96 << 20 {
        return;
    }
    if n.is_text() {
        xml_escape(n.text().unwrap_or(""), out);
        return;
    }
    if !n.is_element() || !svg_ns(n) {
        return;
    }
    let name = n.tag_name().name();
    if matches!(name, "script" | "foreignObject") {
        return;
    }
    out.push('<');
    out.push_str(name);
    let mut styled = false;
    for a in n.attributes() {
        let prefix = match a.namespace() {
            None => "",
            Some(XLINK_NS) => "xlink:",
            Some(XML_NS) => "xml:",
            _ => continue,
        };
        out.push(' ');
        out.push_str(prefix);
        out.push_str(a.name());
        out.push_str("=\"");
        if prefix.is_empty() && a.name() == "style" && !extra.is_empty() {
            xml_escape(&format!("{};{extra}", a.value()), out);
            styled = true;
        } else {
            xml_escape(a.value(), out);
        }
        out.push('"');
    }
    if !styled && !extra.is_empty() {
        out.push_str(" style=\"");
        xml_escape(extra, out);
        out.push('"');
    }
    out.push('>');
    for c in n.children() {
        ser(c, out, "", depth + 1);
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// The four corners of a layer's box in its parent's space (still values).
fn corners(l: &Layer) -> Vec<(f64, f64)> {
    let num = |p: Prop| {
        l.props
            .get(&p)
            .copied()
            .or_else(|| l.tracks.get(&p).and_then(|t| t.first()).map(|k| k.value))
            .and_then(|v| v.as_number())
            .unwrap_or_else(|| default_value(p).as_number().unwrap_or(0.0))
    };
    let (w, h) = (num(Prop::Width), num(Prop::Height));
    let (ax, ay) = (num(Prop::AnchorX) * w, num(Prop::AnchorY) * h);
    let m = mul(
        &mul(
            &mul(
                &translate(num(Prop::X) + ax, num(Prop::Y) + ay),
                &rotate(num(Prop::Rotation)),
            ),
            &scale(num(Prop::ScaleX), num(Prop::ScaleY)),
        ),
        &translate(-ax, -ay),
    );
    [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
        .into_iter()
        .map(|p| pt(&m, p))
        .collect()
}

/// Moves a layer (its still position and keyframes).
fn shift(l: &mut Layer, dx: f64, dy: f64) {
    for (p, d) in [(Prop::X, dx), (Prop::Y, dy)] {
        if let Some(Value::Number(v)) = l.props.get_mut(&p) {
            *v = round4(*v + d);
        } else if !l.tracks.contains_key(&p) {
            l.props.insert(p, Value::Number(round4(d)));
        }
        if let Some(t) = l.tracks.get_mut(&p) {
            for k in t {
                if let Value::Number(v) = &mut k.value {
                    *v = round4(*v + d);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx {
            prefix: "/stage/".into(),
            fonts: vec!["Inter".into(), "Roboto".into()],
            zone: "lower".into(),
        }
    }

    fn fixture(name: &str) -> Converted {
        let path = format!(
            "{}/tests/fixtures/import/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        analyze(&std::fs::read(path).unwrap(), &ctx()).unwrap()
    }

    fn by_name<'t>(c: &'t Converted, name: &str) -> (&'t Layer, &'t LayerReport) {
        let t = &c.comps[0].template;
        let l = t.layers.iter().find(|l| l.name == name).unwrap();
        let r = c.comps[0].layers.iter().find(|r| r.layer == l.id).unwrap();
        (l, r)
    }

    /// Still value (or first keyframe) of a property.
    fn num(l: &Layer, p: Prop) -> f64 {
        l.props
            .get(&p)
            .copied()
            .or_else(|| l.tracks.get(&p).map(|t| t[0].value))
            .and_then(|v| v.as_number())
            .unwrap_or_else(|| default_value(p).as_number().unwrap())
    }

    /// Position in the root group (no rotation / scale on the way).
    fn abs(c: &Converted, l: &Layer) -> (f64, f64) {
        let t = &c.comps[0].template;
        let (mut x, mut y) = (num(l, Prop::X), num(l, Prop::Y));
        let mut p = l.parent.clone();
        while let Some(id) = p {
            let g = t.layer(&id).unwrap();
            if g.parent.is_none() {
                break;
            }
            x += num(g, Prop::X);
            y += num(g, Prop::Y);
            p = g.parent.clone();
        }
        (round4(x), round4(y))
    }

    fn labels(r: &LayerReport) -> Vec<(&str, Status)> {
        r.notes
            .iter()
            .map(|n| (n.label.as_str(), n.status))
            .collect()
    }

    fn track(l: &Layer, p: Prop) -> Vec<(f64, f64)> {
        l.tracks[&p]
            .iter()
            .map(|k| (k.frame, k.value.as_number().unwrap()))
            .collect()
    }

    #[test]
    fn converts_a_score_bug() {
        let c = fixture("score_bug.svg");
        assert_eq!((c.width, c.height, c.fps), (960, 540 / 3, 25.0));
        assert_eq!(c.format, Format::Svg);
        let comp = &c.comps[0];
        let t = &comp.template;
        assert_eq!(comp.name, "Score bug");
        assert_eq!(comp.flat.as_deref(), Some("/stage/score-bug.svg"));
        assert_eq!(c.files[0].0, "score-bug.svg");
        // Still: on air at once.
        assert!(!comp.animated);
        assert_eq!((t.duration, comp.duration), (25.0, 0.0));
        let outs: Vec<_> = t
            .markers
            .iter()
            .filter(|m| m.kind == MarkerKind::Out)
            .collect();
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].frame, 0.0);
        assert_eq!(t.zone, "lower");

        // One root group the size of the SVG.
        let roots: Vec<_> = t.layers.iter().filter(|l| l.parent.is_none()).collect();
        assert_eq!(roots.len(), 1);
        assert!(matches!(roots[0].kind, LayerKind::Group { .. }));
        assert_eq!(
            (num(roots[0], Prop::Width), num(roots[0], Prop::Height)),
            (960.0, 180.0)
        );

        // Rectangle with a gradient and a drop shadow (view box × 2).
        let (bg, r) = by_name(&c, "background");
        let LayerKind::Rect { x } = &bg.kind else {
            panic!()
        };
        assert_eq!(x.fill_style.kind, FillKind::Linear);
        assert_eq!(x.fill_style.angle, 90.0);
        assert_eq!(x.fill_style.color2, "#1e4fa8");
        assert_eq!(
            bg.props[&Prop::Fill],
            Value::Color(Color::from_hex("#0a1f44").unwrap())
        );
        assert_eq!(
            (num(bg, Prop::Width), num(bg, Prop::Height)),
            (960.0, 180.0)
        );
        assert_eq!(num(bg, Prop::CornerRadius), 16.0);
        assert!(bg.drop_shadow);
        assert_eq!(num(bg, Prop::ShadowY), 4.0);
        assert_eq!(labels(r), vec![("Filter {name}", Status::Partial)]);

        // Texts: variables, styles from the sheet (#id over .class).
        let keys: Vec<_> = t
            .fields
            .iter()
            .map(|f| (f.key.as_str(), f.default.as_str()))
            .collect();
        assert_eq!(
            keys,
            vec![("home", "PSG"), ("away", "OM"), ("home_score", "2")]
        );
        let (home, r) = by_name(&c, "home");
        let LayerKind::Text {
            text,
            font_family,
            font_weight,
            align,
            ..
        } = &home.kind
        else {
            panic!()
        };
        assert_eq!(
            (text.as_str(), font_family.as_str(), *font_weight),
            ("{{home}}", "Roboto", 700)
        );
        assert_eq!(*align, TextAlign::Left);
        assert_eq!(num(home, Prop::FontSize), 40.0);
        assert_eq!(labels(r), vec![("Text → variable", Status::Ok)]);
        // Baseline (30 + 10) × 2 = 80: the top is 0.964 font size above.
        assert_eq!(abs(&c, home), (40.0, 41.44));
        let (score, _) = by_name(&c, "home-score");
        assert_eq!(
            score.props[&Prop::Fill],
            Value::Color(Color::from_hex("#ffd400").unwrap())
        );
        let LayerKind::Text { align, .. } = &score.kind else {
            panic!()
        };
        assert_eq!(*align, TextAlign::Right);
        let right = abs(&c, score).0 + num(score, Prop::Width);
        assert!((right - 440.0).abs() < 1e-6, "{right}");

        // The group keeps its children, its box fitted to them.
        let (teams, r) = by_name(&c, "teams");
        assert!(matches!(teams.kind, LayerKind::Group { clip: false, .. }));
        assert_eq!(home.parent.as_deref(), Some(teams.id.as_str()));
        assert_eq!((r.source.as_str(), r.depth), ("group", 0));
        let (dot, r) = by_name(&c, "circle 1");
        assert!(matches!(dot.kind, LayerKind::Ellipse { .. }));
        assert_eq!(r.depth, 1);
        assert_eq!(abs(&c, dot), (490.0, 60.0));
        assert_eq!(num(dot, Prop::Width), 20.0);

        // A path: a closed pen.
        let (bar, _) = by_name(&c, "bar");
        let LayerKind::Pen { x } = &bar.kind else {
            panic!()
        };
        assert!(x.closed && x.fill_on && !x.draw_in);
        assert_eq!(x.points.len(), 4);
        assert_eq!((x.points[0].x, x.points[0].y), (20.0 / 340.0, 0.0));
        assert_eq!(abs(&c, bar), (580.0, 20.0));
        assert_eq!(
            (num(bar, Prop::Width), num(bar, Prop::Height)),
            (340.0, 140.0)
        );
        assert_eq!(num(bar, Prop::StrokeWidth), 4.0);
        let Value::Color(f) = bar.props[&Prop::Fill] else {
            panic!()
        };
        assert!((f.0[3] - 0.2).abs() < 1e-6);

        // A path with a hole and a skewed rectangle: pictures.
        for (name, cause, size) in [
            ("ring", "Compound path", (40.0, 40.0)),
            ("skewed", "Skew", (92.0, 40.0)),
        ] {
            let (l, r) = by_name(&c, name);
            let LayerKind::Image { src, .. } = &l.kind else {
                panic!()
            };
            assert_eq!(
                labels(r),
                vec![("Rasterised", Status::Partial), (cause, Status::Partial)]
            );
            assert_eq!(r.status, Status::Partial);
            assert_eq!(r.source, "image");
            assert_eq!((num(l, Prop::Width), num(l, Prop::Height)), size);
            let file = src.strip_prefix("/stage/").unwrap();
            let png = &c.files.iter().find(|f| f.0 == file).unwrap().1;
            let img = image::load_from_memory(png).unwrap().to_rgba8();
            assert_eq!((img.width() as f64, img.height() as f64), size);
            if name == "ring" {
                assert_eq!(img.get_pixel(2, 2).0[3], 255);
                assert_eq!(img.get_pixel(20, 20).0[3], 0, "the hole");
            }
        }
        assert_eq!(abs(&c, by_name(&c, "ring").0), (20.0, 20.0));

        // A clip path of one rectangle: a clipping group.
        let (clipped, _) = by_name(&c, "clipped");
        assert!(matches!(clipped.kind, LayerKind::Group { clip: true, .. }));
        assert_eq!(
            (num(clipped, Prop::Width), num(clipped, Prop::Height)),
            (400.0, 80.0)
        );
        assert_eq!(num(by_name(&c, "rect 1").0, Prop::X), -20.0);

        // Hidden, with a filter.
        let (_, r) = by_name(&c, "blurred");
        assert!(r.hidden);
        assert_eq!(r.notes[0].params["name"], "blur");
        assert_eq!(labels(r), vec![("Filter {name}", Status::Ignored)]);

        let (line, r) = by_name(&c, "line 1");
        let LayerKind::Pen { x } = &line.kind else {
            panic!()
        };
        assert_eq!(x.stroke_style.dash, Dash::Dashed);
        assert!(!x.closed && !x.fill_on);
        assert_eq!(labels(r), vec![("Dashed stroke", Status::Partial)]);

        // Embedded picture.
        let (logo, _) = by_name(&c, "logo");
        let LayerKind::Image { src, fit, .. } = &logo.kind else {
            panic!()
        };
        assert_eq!(*fit, ImageFit::Contain);
        assert!(c
            .files
            .iter()
            .any(|f| format!("/stage/{}", f.0) == *src && f.0.ends_with(".png")));
        assert_eq!((num(logo, Prop::X), num(logo, Prop::Width)), (880.0, 60.0));

        let ignored: Vec<_> = comp
            .ignored
            .iter()
            .map(|i| (i.name.as_str(), i.reason.as_str()))
            .collect();
        assert_eq!(
            ignored,
            vec![
                ("script 1", "Script"),
                ("foreignObject 1", "HTML content"),
                ("ext", "External picture")
            ]
        );
        assert_eq!(c.warnings[0].label, "Style rules not supported");

        // The list: top first, groups before their children.
        let names: Vec<_> = comp.layers.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names[0], "logo");
        assert_eq!(*names.last().unwrap(), "background");
        let pos = |n: &str| names.iter().position(|x| *x == n).unwrap();
        assert!(pos("teams") < pos("home") && pos("home-score") < pos("home"));
        // Every layer but the root once in the list.
        assert_eq!(comp.layers.len(), t.layers.len() - 1);

        let back: Template = serde_json::from_value(serde_json::to_value(t).unwrap()).unwrap();
        assert_eq!(&back, t);
    }

    #[test]
    fn converts_smil_animations() {
        let c = fixture("animated.svg");
        let comp = &c.comps[0];
        let t = &comp.template;
        assert!(comp.animated);
        assert_eq!((t.duration, comp.duration), (50.0, 2.0));
        let outs: Vec<_> = t
            .markers
            .iter()
            .filter(|m| m.kind == MarkerKind::Out)
            .collect();
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].frame, 50.0);

        // animate x with a spline, opacity.
        let (slide, r) = by_name(&c, "slide");
        assert_eq!(track(slide, Prop::X), vec![(0.0, -100.0), (25.0, 20.0)]);
        assert_eq!(
            slide.tracks[&Prop::X][0].easing,
            Easing::Bezier {
                x1: 0.25,
                y1: 0.1,
                x2: 0.25,
                y2: 1.0
            }
        );
        assert_eq!(track(slide, Prop::Opacity), vec![(0.0, 0.0), (12.5, 1.0)]);
        assert_eq!(slide.tracks[&Prop::Opacity][0].easing, Easing::Linear);
        assert!(!slide.props.contains_key(&Prop::X));
        assert_eq!(
            labels(r),
            vec![("Position", Status::Ok), ("Opacity", Status::Ok)]
        );
        assert!(r.notes.iter().all(|n| n.anim));

        // animateTransform rotate around the centre, played once.
        let (spin, r) = by_name(&c, "spinner");
        assert_eq!(
            track(spin, Prop::Rotation),
            vec![(0.0, 0.0), (12.5, 0.0), (37.49, 360.0), (37.5, 0.0)]
        );
        assert_eq!((num(spin, Prop::X), num(spin, Prop::AnchorX)), (200.0, 0.5));
        assert!(!spin.tracks.contains_key(&Prop::X));
        assert_eq!(
            labels(r),
            vec![
                ("Repeat → once", Status::Partial),
                ("Rotation", Status::Partial)
            ]
        );

        // set visibility → opacity; colour animation.
        let (pop, r) = by_name(&c, "pop");
        assert!(!r.hidden);
        assert_eq!(track(pop, Prop::Opacity), vec![(0.0, 0.0), (25.0, 1.0)]);
        assert_eq!(pop.tracks[&Prop::Opacity][0].easing, Easing::Hold);
        let fill: Vec<_> = pop.tracks[&Prop::Fill]
            .iter()
            .map(|k| (k.frame, k.value))
            .collect();
        assert_eq!(
            fill.first().unwrap().1,
            Value::Color(Color::from_hex("#0000ff").unwrap())
        );
        assert_eq!(
            fill.last().unwrap(),
            &(50.0, Value::Color(Color::from_hex("#ff0000").unwrap()))
        );
        assert_eq!(
            labels(r),
            vec![("Opacity", Status::Ok), ("Colour", Status::Ok)]
        );

        // animateMotion along a line: the group moves.
        let (mover, r) = by_name(&c, "mover");
        assert_eq!(track(mover, Prop::X), vec![(0.0, 10.0), (50.0, 60.0)]);
        assert_eq!(labels(r), vec![("Position", Status::Partial)]);
        assert_eq!(abs(&c, by_name(&c, "rect 1").0), (10.0, 150.0));

        // use → symbol: a group, the view box scaling its content.
        let (dot, r) = by_name(&c, "dot-use");
        assert_eq!(r.source, "use");
        let (circle, cr) = by_name(&c, "circle 1");
        assert_eq!(circle.parent.as_deref(), Some(dot.id.as_str()));
        assert_eq!(cr.depth, 1);
        assert_eq!(abs(&c, circle), (10.0, 10.0));
        assert_eq!(num(circle, Prop::Width), 20.0);

        let (masked, r) = by_name(&c, "masked");
        assert!(matches!(masked.kind, LayerKind::Image { .. }));
        assert_eq!(
            labels(r),
            vec![("Rasterised", Status::Partial), ("Mask", Status::Partial)]
        );

        let (title, r) = by_name(&c, "title");
        assert_eq!(t.fields[0].default, "Breaking news");
        let LayerKind::Text {
            font_family, align, ..
        } = &title.kind
        else {
            panic!()
        };
        assert_eq!((font_family.as_str(), *align), ("Inter", TextAlign::Center));
        assert!((num(title, Prop::X) + num(title, Prop::Width) / 2.0 - 200.0).abs() < 1e-6);
        let font = r
            .notes
            .iter()
            .find(|n| n.label == "Font {font} → {replacement}")
            .unwrap();
        assert_eq!(font.params["font"], "Comic Sans MS");
        assert_eq!(font.params["replacement"], "Inter");
        assert!(labels(r).contains(&("Mixed text styles", Status::Partial)));

        assert_eq!(
            labels(by_name(&c, "pulse").1),
            vec![("CSS animation", Status::Ignored)]
        );
        assert_eq!(comp.ignored[0].reason, "CSS animation");
        assert_eq!(comp.ignored[0].name, "pulse");
        let (clicky, r) = by_name(&c, "clicky");
        assert!(clicky.tracks.is_empty());
        assert_eq!(labels(r), vec![("Interactive animation", Status::Ignored)]);
        assert_eq!(
            labels(by_name(&c, "curve").1),
            vec![("Motion path", Status::Ignored)]
        );

        let back: Template = serde_json::from_value(serde_json::to_value(t).unwrap()).unwrap();
        assert_eq!(&back, t);
    }

    #[test]
    fn reads_odd_documents() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
            <svg x="100" y="0" width="100" height="100" viewBox="0 0 10 10">
              <rect width="10" height="5" fill="hsl(120, 100%, 50%)"/>
            </svg>
            <g transform="rotate(90 50 50)">
              <rect x="40" y="40" width="20" height="10" fill="rgb(255,0,0)"/>
            </g>
            <radialGradient id="r"><stop offset="0" stop-color="white"/><stop offset="1" stop-color="black" stop-opacity="0.5"/></radialGradient>
            <ellipse cx="20" cy="20" rx="10" ry="10" fill="url(#r)"/>
            <text x="10" y="90"><tspan x="10" y="70">One</tspan><tspan x="10" y="90">Two</tspan></text>
            <polyline points="0,0 10,10 20,0" fill="none" stroke="blue"/>
            <rect x="0" y="0" width="10" height="10">
              <animate attributeName="width" from="0" to="10" begin="a.end" dur="1s"/>
            </rect>
            <animate id="a" attributeName="opacity" from="0" to="1" dur="2s" href="#nothing"/>
          </svg>"##;
        let c = analyze(svg.as_bytes(), &ctx()).unwrap();
        assert_eq!((c.width, c.height), (200, 100));
        let t = &c.comps[0].template;
        // Nested svg: a clipping group, its view box scales the content.
        let nested = t.layers.iter().find(|l| l.name == "svg 1").unwrap();
        assert!(matches!(nested.kind, LayerKind::Group { clip: true, .. }));
        let r1 = t.layers.iter().find(|l| l.name == "rect 1").unwrap();
        assert_eq!((num(r1, Prop::Width), num(r1, Prop::Height)), (100.0, 50.0));
        assert_eq!(
            r1.props[&Prop::Fill],
            Value::Color(Color::from_hex("#00ff00").unwrap())
        );
        // A rotated group: its rectangle stays a rectangle.
        let g = t.layers.iter().find(|l| l.name == "g 1").unwrap();
        assert_eq!(num(g, Prop::Rotation), 90.0);
        let r2 = t.layers.iter().find(|l| l.name == "rect 2").unwrap();
        assert!(matches!(r2.kind, LayerKind::Rect { .. }));
        assert_eq!(r2.parent.as_deref(), Some(g.id.as_str()));
        // Radial gradient.
        let e = t.layers.iter().find(|l| l.name == "ellipse 1").unwrap();
        let LayerKind::Ellipse { x } = &e.kind else {
            panic!()
        };
        assert_eq!(x.fill_style.kind, FillKind::Radial);
        assert_eq!(x.fill_style.color2, "#00000080");
        // Lines of tspans: a multi-line variable.
        assert_eq!(t.fields[0].default, "One\nTwo");
        assert_eq!(t.fields[0].kind, FieldKind::Multiline);
        assert_eq!(t.fields[0].key, "one_two");
        let p = t.layers.iter().find(|l| l.name == "polyline 1").unwrap();
        let LayerKind::Pen { x } = &p.kind else {
            panic!()
        };
        assert_eq!((x.points.len(), x.closed), (3, false));
        // Begins when another animation ends (it targets nothing converted).
        let r3 = t.layers.iter().find(|l| l.name == "rect 3").unwrap();
        // Its own width before; no freeze: back to it after.
        assert_eq!(
            track(r3, Prop::Width),
            vec![(0.0, 10.0), (50.0, 0.0), (75.0, 10.0)]
        );
        assert_eq!(r3.tracks[&Prop::Width][0].easing, Easing::Hold);
        assert_eq!(r3.tracks[&Prop::Width][1].easing, Easing::Linear);
        assert_eq!(t.duration, 75.0);

        // Compressed, and unreadable files.
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, svg.as_bytes()).unwrap();
        assert!(analyze(&gz.finish().unwrap(), &ctx()).is_ok());
        assert!(analyze(b"<html></html>", &ctx()).is_err());
        assert!(analyze(b"\xff\xfe<svg", &ctx()).is_err());
        assert!(analyze(b"<svg><rect", &ctx()).is_err());
    }

    #[test]
    fn turns_a_group_around_its_origin() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <g id="wheel" transform="translate(100,100)">
              <animateTransform attributeName="transform" type="rotate" from="0" to="90"
                                dur="1s" additive="sum" fill="freeze"/>
              <rect x="-10" y="-10" width="20" height="20"/>
              <rect id="sk" x="0" y="0" width="10" height="10" transform="skewX(30)"/>
            </g>
          </svg>"##;
        let c = analyze(svg.as_bytes(), &ctx()).unwrap();
        let (wheel, r) = by_name(&c, "wheel");
        assert_eq!(track(wheel, Prop::Rotation), vec![(0.0, 0.0), (25.0, 90.0)]);
        assert!(!wheel.tracks.contains_key(&Prop::X));
        assert_eq!(labels(r), vec![("Rotation", Status::Ok)]);
        // The pivot (the group's origin) stays at (100, 100).
        let (x, w) = (num(wheel, Prop::X), num(wheel, Prop::Width));
        let ax = num(wheel, Prop::AnchorX) * w;
        assert!((x + ax - 100.0).abs() < 1e-3, "{x} {ax}");
        // A skewed child: a picture in the group's space.
        let (sk, r) = by_name(&c, "sk");
        assert!(matches!(sk.kind, LayerKind::Image { .. }));
        assert_eq!(sk.parent.as_deref(), Some(wheel.id.as_str()));
        assert_eq!(r.depth, 1);
        assert!((x + num(sk, Prop::X) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn never_panics_on_bad_values() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="-5" height="1e400">
            <rect width="abc" height="-3" rx="1e308" transform="matrix(1 2)"/>
            <path d="M 10 10 L"/><path d="garbage"/><polygon points="1"/>
            <circle r="5" fill="url(#missing) red"/><text>  </text>
            <use href="#u"/><g id="u"><use href="#u"/></g>
            <image href="data:image/png;base64,!!!"/>
            <animate attributeName="x" values="" dur="1s"/>
            <rect><animateTransform type="rotate" from="0" to="1e308" dur="0.0001s"/></rect>
            <text x="%" y="1em">é<tspan dy="nan">x</tspan></text>
            <rect width="1e308in" height="1e308" transform="scale(1e308)"/>
            <path d="M0 0 L 1e38 1e38 L 0 1e38" stroke-width="1e308"/>
            <path d="M0 0 L 1 1 L 0 1" stroke="red" transform="scale(1e30)"/>
            <rect width="5" height="5"><animate attributeName="x" from="0" to="1e308" dur="1e300s"/></rect>
            <text style="font: bold 1e308px x">big</text>
          </svg>"##;
        let c = analyze(svg.as_bytes(), &ctx()).unwrap();
        assert_eq!((c.width, c.height), (300, 150));
        let t = &c.comps[0].template;
        let back: Template = serde_json::from_value(serde_json::to_value(t).unwrap()).unwrap();
        assert_eq!(back.layers.len(), t.layers.len());
        assert!(t.duration <= MAX_TIME * FPS);
    }
}

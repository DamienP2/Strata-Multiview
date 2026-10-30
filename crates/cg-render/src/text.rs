//! Vector text: glyph outlines from `ab_glyph` turned into tiny-skia paths so
//! text stays sharp under any transform (scale, rotation).

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use ab_glyph::{Font, FontArc, OutlineCurve, ScaleFont};
use cg_core::{TextAlign, VerticalAlign};
use tiny_skia::{Path as SkPath, PathBuilder};

/// Fonts shipped inside the binary so rendering is identical on Linux,
/// macOS and Windows whatever is installed (Inter, SIL OFL 1.1). The server
/// also serves them to the web app under `/fonts/`.
pub const BUNDLED_FONTS: &[(&str, &[u8])] = &[
    (
        "Inter-Regular.otf",
        include_bytes!("../../../assets/fonts/Inter-Regular.otf"),
    ),
    (
        "Inter-Medium.otf",
        include_bytes!("../../../assets/fonts/Inter-Medium.otf"),
    ),
    (
        "Inter-SemiBold.otf",
        include_bytes!("../../../assets/fonts/Inter-SemiBold.otf"),
    ),
    (
        "Inter-Bold.otf",
        include_bytes!("../../../assets/fonts/Inter-Bold.otf"),
    ),
    (
        "Inter-ExtraBold.otf",
        include_bytes!("../../../assets/fonts/Inter-ExtraBold.otf"),
    ),
    // Broadcast families (SIL OFL 1.1, Google Fonts): condensed, display,
    // serif and monospaced faces used by the example projects.
    (
        "BarlowCondensed-Medium.ttf",
        include_bytes!("../../../assets/fonts/BarlowCondensed-Medium.ttf"),
    ),
    (
        "BarlowCondensed-SemiBold.ttf",
        include_bytes!("../../../assets/fonts/BarlowCondensed-SemiBold.ttf"),
    ),
    (
        "BarlowCondensed-Bold.ttf",
        include_bytes!("../../../assets/fonts/BarlowCondensed-Bold.ttf"),
    ),
    (
        "BarlowCondensed-ExtraBold.ttf",
        include_bytes!("../../../assets/fonts/BarlowCondensed-ExtraBold.ttf"),
    ),
    (
        "BebasNeue-Regular.ttf",
        include_bytes!("../../../assets/fonts/BebasNeue-Regular.ttf"),
    ),
    (
        "JetBrainsMono-Medium.ttf",
        include_bytes!("../../../assets/fonts/JetBrainsMono-Medium.ttf"),
    ),
    (
        "JetBrainsMono-Bold.ttf",
        include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf"),
    ),
    (
        "Montserrat-SemiBold.ttf",
        include_bytes!("../../../assets/fonts/Montserrat-SemiBold.ttf"),
    ),
    (
        "Montserrat-ExtraBold.ttf",
        include_bytes!("../../../assets/fonts/Montserrat-ExtraBold.ttf"),
    ),
    (
        "PlayfairDisplay-SemiBold.ttf",
        include_bytes!("../../../assets/fonts/PlayfairDisplay-SemiBold.ttf"),
    ),
    (
        "PlayfairDisplay-Bold.ttf",
        include_bytes!("../../../assets/fonts/PlayfairDisplay-Bold.ttf"),
    ),
    (
        "PlayfairDisplay-ExtraBold.ttf",
        include_bytes!("../../../assets/fonts/PlayfairDisplay-ExtraBold.ttf"),
    ),
    (
        "SpaceGrotesk-Medium.ttf",
        include_bytes!("../../../assets/fonts/SpaceGrotesk-Medium.ttf"),
    ),
    (
        "SpaceGrotesk-Bold.ttf",
        include_bytes!("../../../assets/fonts/SpaceGrotesk-Bold.ttf"),
    ),
    (
        "TitilliumWeb-SemiBold.ttf",
        include_bytes!("../../../assets/fonts/TitilliumWeb-SemiBold.ttf"),
    ),
    (
        "TitilliumWeb-Bold.ttf",
        include_bytes!("../../../assets/fonts/TitilliumWeb-Bold.ttf"),
    ),
    (
        "TitilliumWeb-Black.ttf",
        include_bytes!("../../../assets/fonts/TitilliumWeb-Black.ttf"),
    ),
];

pub struct FontStore {
    db: fontdb::Database,
    cache: HashMap<fontdb::ID, FontArc>,
    extra_dir: Option<std::path::PathBuf>,
    /// Font drawing a character the main font lacks (see [`FontStore::fallback`]).
    fallbacks: HashMap<char, Option<fontdb::ID>>,
    /// Fonts by (family, weight), as [`FontStore::font`] found them.
    queried: HashMap<(String, u16), Option<FontArc>>,
}

/// A font family available to templates.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFamily {
    pub family: String,
    pub weights: Vec<u16>,
    /// "bundled", "uploaded" or "system".
    pub source: &'static str,
    /// Uploaded fonts: their file names (to remove them).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

/// Bumped when fonts are added: renderers reload their font database.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn fonts_changed() {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn font_generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

impl FontStore {
    pub fn new(extra_dir: Option<&Path>) -> Self {
        let mut db = fontdb::Database::new();
        for (_, data) in BUNDLED_FONTS {
            db.load_font_data(data.to_vec());
        }
        db.load_system_fonts();
        if let Some(dir) = extra_dir {
            db.load_fonts_dir(dir);
        }
        db.set_sans_serif_family("Inter");
        tracing::info!(faces = db.len(), "font database loaded");
        Self {
            db,
            cache: HashMap::new(),
            extra_dir: extra_dir.map(Path::to_path_buf),
            fallbacks: HashMap::new(),
            queried: HashMap::new(),
        }
    }

    /// Every family with its weights (regular styles only).
    pub fn families(&self) -> Vec<FontFamily> {
        let mut map: std::collections::BTreeMap<String, FontFamily> =
            std::collections::BTreeMap::new();
        for face in self.db.faces() {
            if face.style != fontdb::Style::Normal {
                continue;
            }
            let Some((name, _)) = face.families.first() else {
                continue;
            };
            let source = match &face.source {
                fontdb::Source::Binary(_) => "bundled",
                fontdb::Source::File(p) | fontdb::Source::SharedFile(p, _)
                    if self.extra_dir.as_ref().is_some_and(|d| p.starts_with(d)) =>
                {
                    "uploaded"
                }
                _ => "system",
            };
            let e = map
                .entry(name.to_lowercase())
                .or_insert_with(|| FontFamily {
                    family: name.clone(),
                    weights: Vec::new(),
                    source,
                    files: Vec::new(),
                });
            if !e.weights.contains(&face.weight.0) {
                e.weights.push(face.weight.0);
            }
            if let fontdb::Source::File(p) | fontdb::Source::SharedFile(p, _) = &face.source {
                if source == "uploaded" {
                    if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                        if !e.files.iter().any(|f| f == n) {
                            e.files.push(n.to_string());
                        }
                    }
                }
            }
        }
        let mut out: Vec<FontFamily> = map.into_values().collect();
        for f in &mut out {
            f.weights.sort_unstable();
        }
        out
    }

    /// The font file used for `family` at `weight` (exact family only), to
    /// serve the same font to browsers.
    pub fn file(&self, family: &str, weight: u16) -> Option<Vec<u8>> {
        let families = [fontdb::Family::Name(family)];
        let id = self.db.query(&fontdb::Query {
            families: &families,
            weight: fontdb::Weight(weight),
            ..Default::default()
        })?;
        self.db
            .with_face_data(id, |data, index| (index == 0).then(|| data.to_vec()))
            .flatten()
    }

    pub fn font(&mut self, family: &str, weight: u16) -> Option<FontArc> {
        // The answer for a family and weight never changes for this database
        // (a new font makes a new store): asked once.
        if let Some(f) = self.queried.get(&(family.to_string(), weight)) {
            return f.clone();
        }
        let f = self.query(family, weight);
        self.queried.insert((family.to_string(), weight), f.clone());
        f
    }

    fn query(&mut self, family: &str, weight: u16) -> Option<FontArc> {
        let families = [fontdb::Family::Name(family), fontdb::Family::SansSerif];
        let query = fontdb::Query {
            families: &families,
            weight: fontdb::Weight(weight),
            ..Default::default()
        };
        let id = self.db.query(&query)?;
        if let Some(f) = self.cache.get(&id) {
            return Some(f.clone());
        }
        let font = self
            .db
            .with_face_data(id, |data, index| {
                ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).ok()
            })
            .flatten()
            .map(FontArc::new)?;
        self.cache.insert(id, font.clone());
        Some(font)
    }
}

pub struct TextStyle {
    pub size: f32,
    pub letter_spacing: f32,
    pub align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub box_w: f32,
    pub box_h: f32,
    /// In font sizes ([`LINE_HEIGHT`] by default).
    pub line_height: f32,
    pub decoration: cg_core::TextDecoration,
}

/// Line height multiplier — must match the web renderer (`line-height: 1.2`).
pub const LINE_HEIGHT: f32 = 1.2;

/// Width of the widest line, matching [`layout_text`]'s advances.
pub fn text_width(font: &FontArc, text: &str, size: f32, letter_spacing: f32) -> f32 {
    let upem = font.units_per_em().unwrap_or(1000.0);
    let scaled = font.as_scaled(ab_glyph::PxScale::from(
        size * font.height_unscaled() / upem,
    ));
    text.split('\n')
        .map(|line| {
            let glyphs: Vec<_> = line.chars().map(|c| font.glyph_id(c)).collect();
            let mut width = 0.0;
            for (j, g) in glyphs.iter().enumerate() {
                width += scaled.h_advance(*g);
                if j + 1 < glyphs.len() {
                    width += scaled.kern(*g, glyphs[j + 1]) + letter_spacing;
                }
            }
            width
        })
        .fold(0.0, f32::max)
}

/// Breaks the lines of `text` at spaces so that none is wider than
/// `max_w` (a word longer than the box stays on its own line), like the
/// web renderer's `white-space: pre-wrap`.
pub fn wrap_text(font: &FontArc, text: &str, size: f32, letter_spacing: f32, max_w: f32) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let mut cur = String::new();
        for word in line.split(' ') {
            let candidate = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if !cur.is_empty() && text_width(font, &candidate, size, letter_spacing) > max_w {
                out.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = candidate;
            }
        }
        out.push(cur);
    }
    out.join("\n")
}

/// A text made to fit its box (see [`fit_text`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Fitted {
    /// The lines to draw (wrapped, or shortened with "…").
    pub text: String,
    pub size: f32,
    /// Horizontal scale (1 = as designed).
    pub squeeze: f32,
    /// Cut at the box when drawn.
    pub clip: bool,
}

/// Height of a block of `lines` lines: the last one without its leading,
/// so that one line of a size fits a box of that height.
fn block_height(lines: usize, size: f32, line_height: f32) -> f32 {
    if lines == 0 {
        0.0
    } else {
        (lines - 1) as f32 * size * line_height + size
    }
}

/// Makes `content` fit the box of `style` the way `overflow` says. `wrap`:
/// a paragraph (lines wrap at the box width). Same rules as the web
/// renderer (`engine/textFit.ts`).
pub fn fit_text(
    font: &FontArc,
    content: &str,
    style: &TextStyle,
    wrap: bool,
    overflow: cg_core::TextOverflow,
    squeeze_min: f32,
    min_size: f32,
) -> Fitted {
    use cg_core::TextOverflow as O;
    let (bw, bh, lh, ls) = (
        style.box_w.max(1.0),
        style.box_h.max(1.0),
        style.line_height,
        style.letter_spacing,
    );
    let layout = |size: f32, width: f32| -> (String, f32, f32) {
        let t = if wrap {
            wrap_text(font, content, size, ls, width)
        } else {
            content.to_string()
        };
        let w = text_width(font, &t, size, ls);
        let h = block_height(t.split('\n').count(), size, lh);
        (t, w, h)
    };
    let size0 = style.size;
    let (text, w, h) = layout(size0, bw);
    let eps = 0.5;
    if w <= bw + eps && h <= bh + eps || overflow == O::Visible {
        return Fitted {
            text,
            size: size0,
            squeeze: 1.0,
            clip: false,
        };
    }
    match overflow {
        O::Visible => unreachable!(),
        O::Clip => Fitted {
            text,
            size: size0,
            squeeze: 1.0,
            clip: true,
        },
        O::Ellipsis => {
            let max_lines = ((bh - size0) / (size0 * lh)).floor().max(0.0) as usize + 1;
            let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
            let cut = lines.len() > max_lines;
            lines.truncate(max_lines);
            let n = lines.len();
            for (i, line) in lines.iter_mut().enumerate() {
                let last = i + 1 == n;
                if text_width(font, line, size0, ls) <= bw + eps && !(last && cut) {
                    continue;
                }
                let mut chars: Vec<char> = line.chars().collect();
                loop {
                    let t: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
                    if chars.is_empty() || text_width(font, &t, size0, ls) <= bw + eps {
                        *line = t;
                        break;
                    }
                    chars.pop();
                }
            }
            Fitted {
                text: lines.join("\n"),
                size: size0,
                squeeze: 1.0,
                clip: true,
            }
        }
        O::Fit => {
            // Narrower first (one line of text only: a paragraph wraps).
            let sx = if wrap || w <= bw {
                1.0
            } else {
                (bw / w).max(squeeze_min.clamp(0.1, 1.0))
            };
            let fits = |size: f32| {
                let (t, w, h) = layout(size, bw / sx);
                (w * sx <= bw + eps && h <= bh + eps, t)
            };
            if let (true, t) = fits(size0) {
                return Fitted {
                    text: t,
                    size: size0,
                    squeeze: sx,
                    clip: false,
                };
            }
            // Then smaller, down to the minimum: the largest size that fits.
            let lo_size = size0 * min_size.clamp(0.1, 1.0);
            let (ok, t) = fits(lo_size);
            if !ok {
                return Fitted {
                    text: t,
                    size: lo_size,
                    squeeze: sx,
                    clip: true,
                };
            }
            let (mut lo, mut hi, mut best) = (lo_size, size0, t);
            for _ in 0..14 {
                let mid = (lo + hi) / 2.0;
                match fits(mid) {
                    (true, t) => {
                        lo = mid;
                        best = t;
                    }
                    (false, _) => hi = mid,
                }
            }
            Fitted {
                text: best,
                size: lo,
                squeeze: sx,
                clip: false,
            }
        }
    }
}

/// Builds a single path containing all glyphs, in layer-local coordinates.
pub fn layout_text(font: &FontArc, text: &str, style: &TextStyle) -> Option<SkPath> {
    // CSS `font-size` is the em size; ab_glyph's PxScale is ascent-descent.
    let upem = font.units_per_em().unwrap_or(1000.0);
    let px = ab_glyph::PxScale::from(style.size * font.height_unscaled() / upem);
    let scaled = font.as_scaled(px);
    let line_h = style.size * style.line_height;
    let lines: Vec<&str> = text.split('\n').collect();
    let block_h = line_h * lines.len() as f32;
    let top = match style.vertical_align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (style.box_h - block_h) / 2.0,
        VerticalAlign::Bottom => style.box_h - block_h,
    };
    // Centre the glyph box inside the line box, as CSS does.
    let half_leading = (line_h - (scaled.ascent() - scaled.descent())) / 2.0;
    // Outline coordinates are in font units.
    let s = style.size / upem;

    let mut pb = PathBuilder::new();
    for (i, line) in lines.iter().enumerate() {
        let glyphs: Vec<_> = line.chars().map(|c| font.glyph_id(c)).collect();
        let mut width = 0.0;
        for (j, g) in glyphs.iter().enumerate() {
            width += scaled.h_advance(*g);
            if j + 1 < glyphs.len() {
                width += scaled.kern(*g, glyphs[j + 1]) + style.letter_spacing;
            }
        }
        let mut x = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (style.box_w - width) / 2.0,
            TextAlign::Right => style.box_w - width,
        };
        let baseline = top + i as f32 * line_h + half_leading + scaled.ascent();
        // Underline / strike through: a bar under the line or across it.
        let bar = match style.decoration {
            cg_core::TextDecoration::Underline => Some(baseline + style.size * 0.1),
            cg_core::TextDecoration::LineThrough => Some(baseline - style.size * 0.3),
            cg_core::TextDecoration::None => None,
        };
        if let (Some(y), true) = (bar, width > 0.0) {
            let t = (style.size * 0.06).max(1.0);
            if let Some(r) = tiny_skia::Rect::from_xywh(x, y - t / 2.0, width, t) {
                pb.push_rect(r);
            }
        }
        for (j, g) in glyphs.iter().enumerate() {
            if let Some(curves) = outline(font, *g) {
                append_outline(&mut pb, &curves, x, baseline, s);
            }
            x += scaled.h_advance(*g) + style.letter_spacing;
            if j + 1 < glyphs.len() {
                x += scaled.kern(*g, glyphs[j + 1]);
            }
        }
    }
    pb.finish()
}

/// Outlines of the glyphs met, by font and glyph (fonts kept alive here, so
/// that their address stays theirs). Read from the font file once, not at
/// every frame.
type Outlines = HashMap<
    usize,
    (
        FontArc,
        HashMap<ab_glyph::GlyphId, Option<Rc<[OutlineCurve]>>>,
    ),
>;

thread_local! {
    static OUTLINES: std::cell::RefCell<Outlines> = std::cell::RefCell::new(HashMap::new());
}

/// The outline of glyph `g` (None: nothing to draw), as `font.outline(g)` has it.
pub(crate) fn outline(font: &FontArc, g: ab_glyph::GlyphId) -> Option<Rc<[OutlineCurve]>> {
    OUTLINES.with(|o| {
        let mut o = o.borrow_mut();
        let (_, glyphs) = o
            .entry(font.font_data().as_ptr() as usize)
            .or_insert_with(|| (font.clone(), HashMap::new()));
        glyphs
            .entry(g)
            .or_insert_with(|| font.outline(g).map(|o| o.curves.into()))
            .clone()
    })
}

fn append_outline(pb: &mut PathBuilder, curves: &[OutlineCurve], ox: f32, baseline: f32, s: f32) {
    let tx = |p: ab_glyph::Point| (ox + p.x * s, baseline - p.y * s);
    let mut last: Option<(f32, f32)> = None;
    for c in curves {
        let start = match c {
            OutlineCurve::Line(a, _)
            | OutlineCurve::Quad(a, _, _)
            | OutlineCurve::Cubic(a, _, _, _) => tx(*a),
        };
        let continuous =
            last.is_some_and(|(lx, ly)| (lx - start.0).abs() < 1e-3 && (ly - start.1).abs() < 1e-3);
        if !continuous {
            if last.is_some() {
                pb.close();
            }
            pb.move_to(start.0, start.1);
        }
        let end = match c {
            OutlineCurve::Line(_, b) => {
                let b = tx(*b);
                pb.line_to(b.0, b.1);
                b
            }
            OutlineCurve::Quad(_, c1, b) => {
                let (c1, b) = (tx(*c1), tx(*b));
                pb.quad_to(c1.0, c1.1, b.0, b.1);
                b
            }
            OutlineCurve::Cubic(_, c1, c2, b) => {
                let (c1, c2, b) = (tx(*c1), tx(*c2), tx(*b));
                pb.cubic_to(c1.0, c1.1, c2.0, c2.1, b.0, b.1);
                b
            }
        };
        last = Some(end);
    }
    if last.is_some() {
        pb.close();
    }
}

// ------------------------------------------------------------- rich text --
// Fallback fonts (never an empty box), glyph runs, and the fit chain of the
// Text widget (`fit_lines`, mirrored by `fitLines` in web/src/engine/textFit.ts).

/// Families tried first for a character the font lacks.
const FALLBACK_EMOJI: &[&str] = &[
    "Noto Color Emoji",
    "Apple Color Emoji",
    "Segoe UI Emoji",
    "Noto Emoji",
    "Symbola",
];
const FALLBACK_TEXT: &[&str] = &[
    "Inter",
    "DejaVu Sans",
    "FreeSans",
    "FreeSerif",
    "Noto Sans",
    "Noto Sans Symbols",
    "Noto Sans Symbols2",
    "OpenSymbol",
    "Symbola",
    "Noto Emoji",
    "Noto Color Emoji",
    "WenQuanYi Zen Hei",
    "IPAGothic",
    "Unifont",
];

/// Characters drawn as emoji (colour font first), like browsers do.
pub fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x231A..=0x23FF | 0x2600..=0x27BF | 0x2B50 | 0x2B55)
}

/// Characters with nothing to draw (joiners, variation selectors, tags…).
pub fn invisible(c: char) -> bool {
    matches!(c as u32, 0x200B..=0x200D | 0x2060 | 0xFEFF | 0xFE00..=0xFE0F | 0xE0020..=0xE007F | 0xE0100..=0xE01EF)
}

/// Has `font` something to draw for `c` (an outline or a picture)?
pub fn covers(font: &FontArc, c: char) -> bool {
    let g = font.glyph_id(c);
    g.0 != 0 && (outline(font, g).is_some() || font.glyph_raster_image2(g, 64).is_some())
}

impl FontStore {
    fn load(&mut self, id: fontdb::ID) -> Option<FontArc> {
        if let Some(f) = self.cache.get(&id) {
            return Some(f.clone());
        }
        let font = self
            .db
            .with_face_data(id, |data, index| {
                ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).ok()
            })
            .flatten()
            .map(FontArc::new)?;
        self.cache.insert(id, font.clone());
        Some(font)
    }

    /// Is this family installed (bundled, uploaded or on the system)?
    pub fn has_family(&self, family: &str) -> bool {
        self.db.faces().any(|f| {
            f.families
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(family))
        })
    }

    /// A font that has `c`, when the text's font lacks it: emoji fonts for
    /// emoji, then common symbol fonts, then any installed font.
    pub fn fallback(&mut self, c: char) -> Option<FontArc> {
        if let Some(id) = self.fallbacks.get(&c) {
            return id.and_then(|id| self.load(id));
        }
        let mut order: Vec<fontdb::ID> = Vec::new();
        let named = |db: &fontdb::Database, names: &[&str], out: &mut Vec<fontdb::ID>| {
            for n in names {
                for f in db.faces() {
                    if f.style == fontdb::Style::Normal
                        && f.families.iter().any(|(x, _)| x.eq_ignore_ascii_case(n))
                        && !out.contains(&f.id)
                    {
                        out.push(f.id);
                    }
                }
            }
        };
        if is_emoji(c) {
            named(&self.db, FALLBACK_EMOJI, &mut order);
        }
        named(&self.db, FALLBACK_TEXT, &mut order);
        for f in self.db.faces() {
            if !order.contains(&f.id) {
                order.push(f.id);
            }
        }
        let mut found = None;
        for id in order {
            if let Some(f) = self.load(id) {
                if covers(&f, c) {
                    found = Some(id);
                    break;
                }
            }
        }
        self.fallbacks.insert(c, found);
        found.and_then(|id| self.load(id))
    }
}

/// The text's font and the fonts drawing what it lacks.
pub struct Chain {
    pub fonts: Vec<FontArc>,
    map: HashMap<char, usize>,
    /// The pictures of the text (`<img>`), by rank: drawn instead of the
    /// characters standing for them (`cg_core::rich::IMG_BASE` + rank).
    pub inline: Vec<InlinePic>,
}

/// A picture in a line of text.
#[derive(Clone)]
pub struct InlinePic {
    pub pixmap: Option<std::sync::Arc<tiny_skia::Pixmap>>,
    pub height: cg_core::rich::ImgHeight,
    pub line_height: f32,
}

impl InlinePic {
    /// Its height and width at `size` (square while missing).
    pub fn size_at(&self, size: f32) -> (f32, f32) {
        let h = self.height.px(size, self.line_height);
        let aspect = self
            .pixmap
            .as_ref()
            .map_or(1.0, |p| p.width() as f32 / p.height().max(1) as f32);
        (h, h * aspect)
    }
}

impl Chain {
    /// `main` and a fallback for every character of `text` it lacks.
    pub fn new(store: &mut FontStore, main: FontArc, text: &str) -> Chain {
        let mut ch = Chain {
            fonts: vec![main],
            map: HashMap::new(),
            inline: Vec::new(),
        };
        ch.add(store, text);
        ch
    }
    /// Adds the fallbacks of `text`.
    pub fn add(&mut self, store: &mut FontStore, text: &str) {
        for c in text.chars() {
            if c.is_whitespace()
                || c.is_control()
                || invisible(c)
                || self.map.contains_key(&c)
                || cg_core::rich::image_index(c).is_some()
            {
                continue;
            }
            let main = &self.fonts[0];
            if main.glyph_id(c).0 != 0 && outline(main, main.glyph_id(c)).is_some() {
                continue;
            }
            if let Some(f) = store.fallback(c) {
                let i = match self
                    .fonts
                    .iter()
                    .position(|x| std::ptr::eq(x.font_data().as_ptr(), f.font_data().as_ptr()))
                {
                    Some(i) => i,
                    None => {
                        self.fonts.push(f);
                        self.fonts.len() - 1
                    }
                };
                self.map.insert(c, i);
            }
        }
    }
    pub fn pick(&self, c: char) -> usize {
        self.map.get(&c).copied().unwrap_or(0)
    }
    /// The picture `c` stands for.
    pub fn inline_of(&self, c: char) -> Option<&InlinePic> {
        self.inline.get(cg_core::rich::image_index(c)?)
    }
}

/// One glyph of a line.
#[derive(Debug, Clone, Copy)]
pub struct PGlyph {
    pub font: usize,
    pub id: ab_glyph::GlyphId,
    /// Pen position from the line's start.
    pub x: f32,
    pub adv: f32,
    pub ch: char,
}

fn scale_of(font: &FontArc, size: f32) -> ab_glyph::PxScale {
    let upem = font.units_per_em().unwrap_or(1000.0);
    ab_glyph::PxScale::from(size * font.height_unscaled() / upem)
}

/// Glyphs of one line (`space_extra`: added after each space, to justify)
/// and its width, as [`text_width`] does for one font.
pub fn glyph_run(
    chain: &Chain,
    line: &str,
    size: f32,
    letter_spacing: f32,
    space_extra: f32,
) -> (Vec<PGlyph>, f32) {
    let mut out: Vec<PGlyph> = Vec::new();
    let mut x = 0.0;
    for c in line.chars() {
        if invisible(c) {
            continue;
        }
        if let Some(pic) = chain.inline_of(c) {
            if let Some(prev) = out.last() {
                x = prev.x + prev.adv + letter_spacing;
                if prev.ch == ' ' || prev.ch == '\u{a0}' {
                    x += space_extra;
                }
            }
            out.push(PGlyph {
                font: 0,
                id: ab_glyph::GlyphId(0),
                x,
                adv: pic.size_at(size).1,
                ch: c,
            });
            continue;
        }
        let fi = chain.pick(c);
        let font = &chain.fonts[fi];
        let sf = font.as_scaled(scale_of(font, size));
        let id = font.glyph_id(c);
        if let Some(prev) = out.last() {
            x = prev.x + prev.adv + letter_spacing;
            if prev.font == fi && chain.inline_of(prev.ch).is_none() {
                x += sf.kern(prev.id, id);
            }
            if prev.ch == ' ' || prev.ch == '\u{a0}' {
                x += space_extra;
            }
        }
        out.push(PGlyph {
            font: fi,
            id,
            x,
            adv: sf.h_advance(id),
            ch: c,
        });
    }
    let w = out.last().map_or(0.0, |g| g.x + g.adv);
    (out, w)
}

/// Width of one line drawn with the chain.
pub fn chain_width(chain: &Chain, line: &str, size: f32, letter_spacing: f32) -> f32 {
    glyph_run(chain, line, size, letter_spacing, 0.0).1
}

/// Outlines of a run at (x0, baseline); glyphs with only a picture (colour
/// emoji) are returned to be drawn as pictures.
pub fn run_outline(
    chain: &Chain,
    glyphs: &[PGlyph],
    size: f32,
    x0: f32,
    baseline: f32,
    pb: &mut PathBuilder,
) -> Vec<PGlyph> {
    let mut pictures = Vec::new();
    for g in glyphs {
        if chain.inline_of(g.ch).is_some() {
            pictures.push(*g);
            continue;
        }
        let font = &chain.fonts[g.font];
        match outline(font, g.id) {
            Some(curves) => {
                let upem = font.units_per_em().unwrap_or(1000.0);
                append_outline(pb, &curves, x0 + g.x, baseline, size / upem);
            }
            None if !g.ch.is_whitespace() => pictures.push(*g),
            None => {}
        }
    }
    pictures
}

/// Ascent and descent (negative) of the chain's main font at `size`.
pub fn metrics(font: &FontArc, size: f32) -> (f32, f32) {
    let sf = font.as_scaled(scale_of(font, size));
    (sf.ascent(), sf.descent())
}

/// Visual order of a line for right-to-left text (no shaping: Arabic
/// letters stay in their isolated forms): right-to-left runs reversed,
/// left-to-right runs (Latin, numbers) kept.
pub fn visual_rtl(line: &str) -> String {
    use cg_core::wid::text::{graphemes, is_rtl};
    let gs = graphemes(line);
    // 0 = right to left, 1 = left to right, 2 = neutral.
    let class: Vec<u8> = gs
        .iter()
        .map(|g| {
            let c = g.chars().next().unwrap_or(' ');
            if is_rtl(c) {
                0
            } else if c.is_alphanumeric() {
                1
            } else {
                2
            }
        })
        .collect();
    let n = gs.len();
    let mut res = class.clone();
    for i in 0..n {
        if class[i] == 2 {
            let prev = class[..i].iter().rev().find(|&&k| k != 2).copied();
            let next = class[i + 1..].iter().find(|&&k| k != 2).copied();
            res[i] = if prev == Some(1) && next == Some(1) {
                1
            } else {
                0
            };
        }
    }
    let mut runs: Vec<(u8, Vec<&str>)> = Vec::new();
    for i in 0..n {
        match runs.last_mut() {
            Some((k, v)) if *k == res[i] => v.push(gs[i]),
            _ => runs.push((res[i], vec![gs[i]])),
        }
    }
    let mirror = |g: &str| -> String {
        match g {
            "(" => ")".into(),
            ")" => "(".into(),
            "[" => "]".into(),
            "]" => "[".into(),
            "«" => "»".into(),
            "»" => "«".into(),
            "<" => ">".into(),
            ">" => "<".into(),
            _ => g.to_string(),
        }
    };
    let mut out = String::new();
    for (k, v) in runs.iter().rev() {
        if *k == 0 {
            for g in v.iter().rev() {
                out.push_str(&mirror(g));
            }
        } else {
            out.push_str(&v.concat());
        }
    }
    out
}

/// One line of a fitted text.
#[derive(Debug, Clone, PartialEq)]
pub struct FitLine {
    pub text: String,
    /// Last line of its paragraph (not justified).
    pub end: bool,
}

/// How the Text widget's text fits its box (see [`fit_lines`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FitLines {
    pub lines: Vec<FitLine>,
    pub size: f32,
    pub squeeze: f32,
    pub clip: bool,
    /// Size × width kept (1 = as designed).
    pub ratio: f32,
    pub overflows: bool,
    /// fit | squeeze | shrink | end (minimums reached) | clip | ellipsis | visible.
    pub stage: &'static str,
    /// A word longer than the line was hyphenated.
    pub forced: bool,
    pub hyphenated: bool,
}

pub struct FitOpts<'a> {
    pub size: f32,
    pub line_height: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub wrap: bool,
    pub overflow: cg_core::TextOverflow,
    pub squeeze: f32,
    pub min_size: f32,
    /// 0 = no limit.
    pub max_lines: usize,
    pub hyphen: bool,
    pub lang: &'a str,
    /// Fit ends with "…" at its minimums instead of cutting.
    pub ellipsis_end: bool,
}

type Measure<'m> = dyn FnMut(&str, f32) -> f32 + 'm;

struct Wrapped {
    lines: Vec<FitLine>,
    forced: bool,
    hyph: bool,
}

fn wrap_lines(
    content: &str,
    o: &FitOpts,
    size: f32,
    max_w: f32,
    forced: bool,
    m: &mut Measure,
) -> Wrapped {
    use cg_core::wid::text::{glue, graphemes, hyphen_points};
    let mut w = Wrapped {
        lines: Vec::new(),
        forced: false,
        hyph: false,
    };
    if !o.wrap {
        w.lines = content
            .split('\n')
            .map(|t| FitLine {
                text: t.to_string(),
                end: true,
            })
            .collect();
        return w;
    }
    let content = glue(content, o.lang);
    for para in content.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let cand = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if cur.is_empty() || m(&cand, size) <= max_w {
                cur = cand;
            } else {
                let mut rest = word.to_string();
                if o.hyphen {
                    if let Some((a, b)) = hyphen_points(word, o.lang)
                        .into_iter()
                        .rev()
                        .find(|(a, _)| m(&format!("{cur} {a}-"), size) <= max_w)
                    {
                        w.lines.push(FitLine {
                            text: format!("{cur} {a}-"),
                            end: false,
                        });
                        w.hyph = true;
                        rest = b;
                        cur.clear();
                    }
                }
                if !cur.is_empty() {
                    w.lines.push(FitLine {
                        text: cur,
                        end: false,
                    });
                }
                cur = rest;
            }
            let mut guard = 0;
            while forced && guard < 200 && m(&cur, size) > max_w {
                guard += 1;
                let p = hyphen_points(&cur, o.lang)
                    .into_iter()
                    .rev()
                    .find(|(a, _)| m(&format!("{a}-"), size) <= max_w)
                    .unwrap_or_else(|| {
                        let g = graphemes(&cur);
                        let mut k = g.len().saturating_sub(1);
                        while k > 1 && m(&format!("{}-", g[..k].concat()), size) > max_w {
                            k -= 1;
                        }
                        (g[..k].concat(), g[k..].concat())
                    });
                if p.0.is_empty() || p.1.is_empty() {
                    break;
                }
                w.lines.push(FitLine {
                    text: format!("{}-", p.0),
                    end: false,
                });
                w.forced = true;
                cur = p.1;
            }
        }
        w.lines.push(FitLine {
            text: cur,
            end: true,
        });
    }
    w
}

fn ellipsize(
    mut lines: Vec<FitLine>,
    o: &FitOpts,
    size: f32,
    sx: f32,
    m: &mut Measure,
) -> Vec<FitLine> {
    let bw = o.box_w.max(1.0) / sx;
    let eps = 0.5;
    let mut max_lines = ((o.box_h.max(1.0) - size) / (size * o.line_height))
        .floor()
        .max(0.0) as usize
        + 1;
    if o.max_lines > 0 {
        max_lines = max_lines.min(o.max_lines);
    }
    let cut = lines.len() > max_lines;
    lines.truncate(max_lines);
    let n = lines.len();
    for (i, line) in lines.iter_mut().enumerate() {
        let last = i + 1 == n;
        if m(&line.text, size) <= bw + eps && !(last && cut) {
            continue;
        }
        let mut g: Vec<String> = cg_core::wid::text::graphemes(&line.text)
            .into_iter()
            .map(String::from)
            .collect();
        loop {
            let t = g.concat().trim_end().to_string() + "…";
            if g.is_empty() || m(&t, size) <= bw + eps {
                line.text = t;
                line.end = true;
                break;
            }
            g.pop();
        }
    }
    lines
}

/// Makes `content` fit the box the way `o` says. The same chain as
/// [`fit_text`] (narrower, then smaller, then cut — or "…", cut, outside),
/// plus a line limit, hyphenation, words longer than the line hyphenated
/// when nothing else fits, and "…" at the minimums. `m(line, size)` is the
/// width of a line.
pub fn fit_lines(content: &str, o: &FitOpts, m: &mut Measure) -> FitLines {
    use cg_core::TextOverflow as O;
    let (bw, bh, lh) = (o.box_w.max(1.0), o.box_h.max(1.0), o.line_height);
    let eps = 0.5;
    let size0 = o.size;
    let widest = |w: &Wrapped, size: f32, m: &mut Measure| {
        w.lines.iter().map(|l| m(&l.text, size)).fold(0.0, f32::max)
    };
    let fits = |w: &Wrapped, size: f32, sx: f32, m: &mut Measure| {
        widest(w, size, m) * sx <= bw + eps
            && block_height(w.lines.len(), size, lh) <= bh + eps
            && (o.max_lines == 0 || w.lines.len() <= o.max_lines)
    };
    let done =
        |w: Wrapped, size: f32, sx: f32, clip: bool, overflows: bool, stage: &'static str| {
            FitLines {
                lines: w.lines,
                size,
                squeeze: sx,
                clip,
                ratio: size / size0.max(1e-6) * sx,
                overflows,
                stage,
                forced: w.forced,
                hyphenated: w.hyph,
            }
        };
    let first = wrap_lines(content, o, size0, bw, false, m);
    let w0 = widest(&first, size0, m);
    if fits(&first, size0, 1.0, m) {
        return done(first, size0, 1.0, false, false, "fit");
    }
    match o.overflow {
        O::Visible => done(first, size0, 1.0, false, true, "visible"),
        O::Clip | O::Ellipsis => {
            let f = if o.wrap {
                wrap_lines(content, o, size0, bw, true, m)
            } else {
                first
            };
            if fits(&f, size0, 1.0, m) {
                return done(f, size0, 1.0, false, false, "fit");
            }
            if o.overflow == O::Clip {
                return done(f, size0, 1.0, true, true, "clip");
            }
            let (forced, hyph) = (f.forced, f.hyph);
            let lines = ellipsize(f.lines, o, size0, 1.0, m);
            done(
                Wrapped {
                    lines,
                    forced,
                    hyph,
                },
                size0,
                1.0,
                true,
                true,
                "ellipsis",
            )
        }
        O::Fit => {
            // Narrower first (one line of text only: a paragraph wraps).
            let sx = if o.wrap || w0 <= bw {
                1.0
            } else {
                (bw / w0).max(o.squeeze.clamp(0.1, 1.0))
            };
            let test = |size: f32, forced: bool, m: &mut Measure| {
                let w = wrap_lines(content, o, size, bw / sx, forced, m);
                (fits(&w, size, sx, m), w)
            };
            if let (true, w) = test(size0, false, m) {
                return done(
                    w,
                    size0,
                    sx,
                    false,
                    false,
                    if sx < 1.0 { "squeeze" } else { "fit" },
                );
            }
            // Then smaller, down to the minimum: the largest size that fits.
            let lo_size = size0 * o.min_size.clamp(0.1, 1.0);
            let (ok, t) = test(lo_size, false, m);
            let (forced, t) = if ok {
                (false, t)
            } else if o.wrap {
                match test(lo_size, true, m) {
                    (true, w) => (true, w),
                    _ => (false, t),
                }
            } else {
                (false, t)
            };
            if !ok && !forced {
                // At the minimums: cut, or "…".
                if o.ellipsis_end {
                    let (fo, hy) = (t.forced, t.hyph);
                    let lines = ellipsize(t.lines, o, lo_size, sx, m);
                    return done(
                        Wrapped {
                            lines,
                            forced: fo,
                            hyph: hy,
                        },
                        lo_size,
                        sx,
                        true,
                        true,
                        "end",
                    );
                }
                return done(t, lo_size, sx, true, true, "end");
            }
            if forced {
                if let (true, w) = test(size0, true, m) {
                    return done(
                        w,
                        size0,
                        sx,
                        false,
                        false,
                        if sx < 1.0 { "squeeze" } else { "fit" },
                    );
                }
            }
            let (mut lo, mut hi, mut best) = (lo_size, size0, t);
            for _ in 0..14 {
                let mid = (lo + hi) / 2.0;
                match test(mid, forced, m) {
                    (true, w) => {
                        lo = mid;
                        best = w;
                    }
                    (false, _) => hi = mid,
                }
            }
            done(best, lo, sx, false, false, "shrink")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::TextOverflow;

    fn style(w: f32, h: f32) -> TextStyle {
        TextStyle {
            size: 50.0,
            letter_spacing: 0.0,
            align: TextAlign::Left,
            vertical_align: VerticalAlign::Middle,
            box_w: w,
            box_h: h,
            line_height: LINE_HEIGHT,
            decoration: cg_core::TextDecoration::None,
        }
    }

    #[test]
    fn fit_ellipsis_clip() {
        let mut store = FontStore::new(None);
        let name = store.families()[0].family.clone();
        let font = store.font(&name, 700).unwrap();
        let text = "Maria Antonietta Gonzalez";
        let w = text_width(&font, text, 50.0, 0.0);
        // Fits: as it is.
        let f = fit_text(
            &font,
            text,
            &style(w + 10.0, 60.0),
            false,
            TextOverflow::Fit,
            0.8,
            0.6,
        );
        assert_eq!((f.size, f.squeeze, f.clip), (50.0, 1.0, false));
        // A little too long: narrower only.
        let f = fit_text(
            &font,
            text,
            &style(w * 0.9, 60.0),
            false,
            TextOverflow::Fit,
            0.8,
            0.6,
        );
        assert_eq!(f.size, 50.0);
        assert!((f.squeeze - 0.9).abs() < 0.01);
        // Much too long: narrower, then smaller.
        let f = fit_text(
            &font,
            text,
            &style(w * 0.6, 60.0),
            false,
            TextOverflow::Fit,
            0.8,
            0.6,
        );
        assert!((f.squeeze - 0.8).abs() < 1e-3 && f.size < 50.0 && f.size >= 30.0 && !f.clip);
        assert!(text_width(&font, &f.text, f.size, 0.0) * f.squeeze <= w * 0.6 + 0.5);
        // Far too long: at the minimums, and cut.
        let f = fit_text(
            &font,
            text,
            &style(w * 0.3, 60.0),
            false,
            TextOverflow::Fit,
            0.8,
            0.6,
        );
        assert!(f.clip && (f.size - 30.0).abs() < 1e-3);
        // "…"
        let f = fit_text(
            &font,
            text,
            &style(w * 0.5, 60.0),
            false,
            TextOverflow::Ellipsis,
            0.8,
            0.6,
        );
        assert!(f.text.ends_with('…') && text_width(&font, &f.text, 50.0, 0.0) <= w * 0.5 + 0.5);
        // A paragraph: wraps, then smaller when the lines are too many.
        let f = fit_text(
            &font,
            "one two three four five six seven",
            &style(w * 0.5, 60.0),
            true,
            TextOverflow::Fit,
            0.8,
            0.3,
        );
        assert!(f.size < 50.0 && f.text.contains('\n') && !f.clip);
    }

    #[test]
    fn fit_lines_matches_fit_text_and_extends_it() {
        let mut store = FontStore::new(None);
        let font = store.font("Inter", 700).unwrap();
        let chain = Chain::new(&mut store, font.clone(), "x");
        let mut m = |s: &str, z: f32| chain_width(&chain, s, z, 0.0);
        let text = "Maria Antonietta Gonzalez";
        let w = text_width(&font, text, 50.0, 0.0);
        for (bw, wrap) in [
            (w + 10.0, false),
            (w * 0.9, false),
            (w * 0.6, false),
            (w * 0.3, false),
            (w * 0.5, true),
        ] {
            let old = fit_text(
                &font,
                text,
                &style(bw, 60.0),
                wrap,
                TextOverflow::Fit,
                0.8,
                0.6,
            );
            let o = FitOpts {
                size: 50.0,
                line_height: LINE_HEIGHT,
                box_w: bw,
                box_h: 60.0,
                wrap,
                overflow: TextOverflow::Fit,
                squeeze: 0.8,
                min_size: 0.6,
                max_lines: 0,
                hyphen: false,
                lang: "en",
                ellipsis_end: false,
            };
            let new = fit_lines(text, &o, &mut m);
            assert!(
                (old.size - new.size).abs() < 1e-3 && (old.squeeze - new.squeeze).abs() < 1e-3,
                "{bw}"
            );
            assert_eq!(
                old.text,
                new.lines
                    .iter()
                    .map(|l| l.text.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        // A word longer than the line: hyphenated, that word only.
        let o = FitOpts {
            size: 50.0,
            line_height: LINE_HEIGHT,
            box_w: 300.0,
            box_h: 400.0,
            wrap: true,
            overflow: TextOverflow::Fit,
            squeeze: 0.8,
            min_size: 0.9,
            max_lines: 0,
            hyphen: false,
            lang: "fr",
            ellipsis_end: false,
        };
        let f = fit_lines("la anticonstitutionnellement", &o, &mut m);
        assert!(f.forced && !f.clip && f.lines.len() >= 3);
        assert!(f.lines.iter().all(|l| m(&l.text, f.size) <= 300.5));
        // Line limit and "…" at the minimums.
        let o = FitOpts {
            box_h: 60.0,
            max_lines: 1,
            ellipsis_end: true,
            ..o
        };
        let f = fit_lines(
            "one two three four five six seven eight nine ten",
            &o,
            &mut m,
        );
        assert_eq!(f.lines.len(), 1);
        assert!(f.lines[0].text.ends_with('…') && f.stage == "end");
    }

    #[test]
    fn fallback_never_empty_box() {
        let mut store = FontStore::new(None);
        let font = store.font("Inter", 700).unwrap();
        let chain = Chain::new(&mut store, font, "⚽");
        // Inter has no ⚽: another installed font draws it (when the system has one).
        if store.has_family("DejaVu Sans") || store.has_family("Noto Color Emoji") {
            assert_ne!(chain.pick('⚽'), 0);
        }
        assert_eq!(visual_rtl("abc"), "abc");
        assert_eq!(visual_rtl("אבג (12)"), "(12) גבא");
    }
}

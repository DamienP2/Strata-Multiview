//! Text: settings, operator state and actions, and the language rules both
//! renderers share (grapheme clusters, hyphenation points, French spaces).
//! Drawn by `cg-render/src/wid/text.rs` and `web/src/widgets/text.tsx` (keep in sync);
//! the fit chain is `cg-render/src/text.rs` `fit_lines` / `web/src/engine/textFit.ts` `fitLines`.
//!
//! The layer keeps its older fields (`wrap`, `overflow`, `squeeze`,
//! `minSize`, `warnBelow`, `transform`, `decoration`, `lineHeight`,
//! `scroll`…): an old text looks exactly the same with the defaults below.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::model::Layer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BreakMode {
    /// Lines break between words only (a word longer than the line is still
    /// hyphenated, that word only, when nothing else fits).
    #[default]
    Word,
    /// Words are hyphenated at the end of the lines.
    Hyphen,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitEnd {
    /// At the minimums, cut at the box (as before).
    #[default]
    Clip,
    /// At the minimums, shortened with "…".
    Ellipsis,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillType {
    #[default]
    Solid,
    /// From the `fill` colour to `color2`.
    Gradient,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    #[default]
    Ltr,
    /// Arabic, Hebrew: lines read right to left.
    Rtl,
}

/// Settings of the widget, flattened into the layer's JSON (every field
/// has a default so older projects still load and look the same). The text
/// colour, stroke colour and width, size and letter spacing are the
/// animated `fill`, `stroke`, `strokeWidth`, `fontSize`, `letterSpacing`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Paragraph: most lines shown (0 = as many as the box holds).
    pub max_lines: u32,
    pub break_mode: BreakMode,
    /// Hyphenation and French spacing rules: fr, en, de, es, it.
    pub lang: String,
    /// What Fit does once at its minimums.
    pub fit_end: FitEnd,
    pub italic: bool,
    pub fill_type: FillType,
    pub color2: String,
    /// CSS angle: 0 = to the top, 90 = to the right.
    pub grad_angle: f32,
    /// Stroke drawn outside, behind the fill (twice the width, half shows).
    pub stroke_outside: bool,
    pub shadow: bool,
    pub shadow_color: String,
    pub shadow_x: f32,
    pub shadow_y: f32,
    pub shadow_blur: f32,
    /// One coloured band behind each line (synthé).
    pub band: bool,
    pub band_color: String,
    pub band_pad_x: f32,
    pub band_gap: f32,
    pub band_radius: f32,
    pub band_accent: bool,
    pub band_accent_color: String,
    /// Lines (but the last of a paragraph) fill the width.
    pub justify: bool,
    pub dir: Direction,
    /// The looks of the variables set in the text (`{{key}}`), over the
    /// text's own: what each sets differs from the rest of the text.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub var_styles: BTreeMap<String, VarStyle>,
}

/// The looks of a variable of the text; unset: as the rest of the text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VarStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    /// px, as the text's `fontSize` (scaled with it when the text fits its box).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    /// Smaller, above the line (exposant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sup: Option<bool>,
    /// Smaller, below the line (indice).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<bool>,
    /// px, as the text's `letterSpacing`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<crate::model::TextTransform>,
    /// `#rrggbb[aa]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl VarStyle {
    pub fn is_empty(&self) -> bool {
        *self == VarStyle::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_lines: 0,
            break_mode: BreakMode::Word,
            lang: String::new(),
            fit_end: FitEnd::Clip,
            italic: false,
            fill_type: FillType::Solid,
            color2: "#a594ff".into(),
            grad_angle: 90.0,
            stroke_outside: false,
            shadow: false,
            shadow_color: "#000000aa".into(),
            shadow_x: 0.0,
            shadow_y: 4.0,
            shadow_blur: 12.0,
            band: false,
            band_color: "#24164f".into(),
            band_pad_x: 26.0,
            band_gap: 10.0,
            band_radius: 0.0,
            band_accent: true,
            band_accent_color: "#ff3b5c".into(),
            justify: false,
            dir: Direction::Ltr,
            var_styles: BTreeMap::new(),
        }
    }
}

/// No operator state: a text is taken, continued and taken out like any graphic.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {}

pub fn act(
    _layer: &Layer,
    _st: &mut State,
    op: &str,
    _args: &serde_json::Value,
    _ctx: &ActCtx,
) -> Result<(), String> {
    Err(format!("unknown action `{op}`"))
}

// ------------------------------------------------------------ graphemes --

/// Continues the cluster of the character before (combining marks,
/// variation selectors, skin tones, tags, keycaps, joiners).
fn extends(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2
        | 0x05C4..=0x05C5 | 0x05C7 | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670
        | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7..=0x06E8 | 0x06EA..=0x06ED
        | 0x0900..=0x0903 | 0x093A..=0x094F | 0x0951..=0x0957 | 0x0962..=0x0963
        | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E
        | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x200C | 0x20D0..=0x20FF | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F | 0xE0100..=0xE01EF)
}

fn regional(c: char) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&(c as u32))
}

/// User-perceived characters (simplified Unicode grapheme rules): a base and
/// its combining marks, emoji ZWJ sequences, variation selectors, skin tones,
/// flags (pairs of regional indicators), CR LF.
pub fn graphemes(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev: Option<char> = None;
    let mut ri_run = 0; // regional indicators in the current cluster
    for (i, c) in s.char_indices() {
        let join = match prev {
            None => false,
            Some(p) => {
                (p == '\r' && c == '\n')
                    || extends(c)
                    || c == '\u{200D}'
                    || p == '\u{200D}'
                    || (regional(c) && regional(p) && ri_run % 2 == 1)
            }
        };
        if !join && prev.is_some() {
            out.push(&s[start..i]);
            start = i;
            ri_run = 0;
        }
        if regional(c) {
            ri_run += 1;
        }
        prev = Some(c);
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

// ----------------------------------------------------------- hyphenation --

struct LangRules {
    vowels: &'static str,
    keep: &'static [&'static str],
    left: usize,
    right: usize,
}

fn rules(lang: &str) -> LangRules {
    match lang {
        "en" => LangRules {
            vowels: "aeiouy",
            keep: &["ch", "sh", "th", "ph", "wh", "ck", "ng", "qu"],
            left: 2,
            right: 3,
        },
        "de" => LangRules {
            vowels: "aeiouyäöü",
            keep: &["ch", "ck", "ph", "th", "qu", "sch"],
            left: 2,
            right: 2,
        },
        "es" => LangRules {
            vowels: "aeiouáéíóúü",
            keep: &[
                "ch", "ll", "rr", "bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr", "pl", "pr",
                "tr", "qu",
            ],
            left: 2,
            right: 2,
        },
        "it" => LangRules {
            vowels: "aeiouàèéìòù",
            keep: &[
                "ch", "gh", "gn", "gl", "sc", "bl", "br", "cr", "dr", "fr", "gr", "pr", "tr", "qu",
            ],
            left: 2,
            right: 2,
        },
        _ => LangRules {
            vowels: "aeiouyàâäéèêëîïôöùûüœæ",
            keep: &[
                "ch", "ph", "th", "gn", "qu", "bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr",
                "pl", "pr", "tr", "vr",
            ],
            left: 2,
            right: 3,
        },
    }
}

/// Where `word` may be hyphenated (simple rules, no dictionary: V-CV and
/// VC-CV, consonant pairs of the language kept together), as
/// (before, after) pairs, shortest first. Same as `hyphenPoints` in text.tsx.
pub fn hyphen_points(word: &str, lang: &str) -> Vec<(String, String)> {
    let r = rules(lang);
    let c: Vec<char> = word.chars().collect();
    let low: Vec<char> = c
        .iter()
        .map(|x| x.to_lowercase().next().unwrap_or(*x))
        .collect();
    let is_l = |i: usize| low.get(i).is_some_and(|x| x.is_alphabetic());
    let is_v = |i: usize| low.get(i).is_some_and(|x| r.vowels.contains(*x));
    let mut out = Vec::new();
    if c.len() < r.left + r.right {
        return out;
    }
    for i in r.left..=(c.len() - r.right) {
        if !is_l(i - 1) || !is_l(i) || !is_l(i + 1) {
            continue;
        }
        let pair: String = [low[i - 1], low[i]].iter().collect();
        let ok = (is_v(i - 1) && !is_v(i) && is_v(i + 1))
            || (i >= 2
                && !is_v(i - 1)
                && !is_v(i)
                && is_v(i + 1)
                && is_v(i - 2)
                && !r.keep.contains(&pair.as_str()));
        if ok {
            out.push((c[..i].iter().collect(), c[i..].iter().collect()));
        }
    }
    out
}

/// French typography: the space before : ; ! ? » and after « never breaks.
pub fn glue(text: &str, lang: &str) -> String {
    if lang != "fr" {
        return text.to_string();
    }
    let mut out = text.to_string();
    for p in [":", ";", "!", "?", "»"] {
        out = out.replace(&format!(" {p}"), &format!("\u{a0}{p}"));
    }
    out.replace("« ", "«\u{a0}")
}

/// Strong right-to-left letter (Hebrew, Arabic and their presentation forms).
pub fn is_rtl(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grapheme_clusters() {
        assert_eq!(graphemes("été").len(), 3);
        assert_eq!(graphemes("e\u{301}t").len(), 2);
        assert_eq!(graphemes("👩‍💻").len(), 1);
        assert_eq!(graphemes("🇫🇷🇩🇪").len(), 2);
        assert_eq!(graphemes("👍🏽!").len(), 2);
        assert_eq!(graphemes("❤️a").len(), 2);
        assert_eq!(graphemes("").len(), 0);
    }

    #[test]
    fn hyphenation() {
        let p = hyphen_points("anticonstitutionnellement", "fr");
        assert!(!p.is_empty());
        assert!(p
            .iter()
            .all(|(a, b)| a.chars().count() >= 2 && b.chars().count() >= 3));
        // "ch" is kept together in French.
        assert!(!p.iter().any(|(a, _)| a == "mac"));
        assert!(hyphen_points("le", "fr").is_empty());
    }

    #[test]
    fn french_spaces() {
        assert_eq!(
            glue("Oui : non « a »", "fr"),
            "Oui\u{a0}: non «\u{a0}a\u{a0}»"
        );
        assert_eq!(glue("a : b", "en"), "a : b");
    }

    #[test]
    fn old_texts_load() {
        let l: Layer = serde_json::from_str(
            r#"{"id":"a","name":"T","type":"text","text":"Hi","fontFamily":"Inter","props":{}}"#,
        )
        .unwrap();
        let crate::LayerKind::Text { x, .. } = l.kind else {
            panic!()
        };
        assert_eq!(x, Config::default());
    }
}

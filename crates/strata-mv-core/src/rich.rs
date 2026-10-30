//! Rich text of the texts typed in the Designer: parts of the text in bold,
//! italic, underlined, struck through or in another colour.
//!
//! The style is written in the text itself with a few tags (mirror of
//! `web/src/engine/rich.ts`):
//!
//! `<b>bold</b> <i>italic</i> <u>underlined</u> <s>struck</s> <c=#ff3b5c>red</c>`
//!
//! A picture in the line: `<img=media/flag.png|cap>` (height of the capitals,
//! `size` the font size, `line` the line's; its width follows its proportions).
//! Its source may be a `{{key}}` the operator chooses. In the shown text it is
//! one character of [`IMG_BASE`] + its rank, as wide as the picture.
//!
//! Tags nest. Anything else that looks like a tag stays text. A `{{key}}`
//! takes the style around it. Only what people type is styled: the text of
//! the Designer, and the values the operator types (`rich` keys); the values
//! of the data sources never add styles (a `<b>` in them is text).

use std::collections::BTreeMap;

use crate::anim::{interpolate_text, Data};
use crate::model::{LayerKind, Template, TextTransform};
use crate::wid::text::VarStyle;

/// Keys whose values may have styles: the template's values typed by the
/// operator (not read from a data source).
pub fn typed_key(t: &Template, key: &str) -> bool {
    t.fields.iter().any(|f| f.key == key && f.binding.is_none())
}

/// First character standing for a picture in the shown text (the n-th
/// picture is `IMG_BASE + n`): the end of the last private use plane, which
/// no font draws.
pub const IMG_BASE: u32 = 0x10_F000;

/// The rank of the picture `c` stands for.
pub fn image_index(c: char) -> Option<usize> {
    let v = c as u32;
    (IMG_BASE..IMG_BASE + 0xFFD)
        .contains(&v)
        .then(|| (v - IMG_BASE) as usize)
}

/// How high a picture of the text is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImgHeight {
    /// The capitals' (0.7 of the font size).
    Cap,
    /// The font size.
    #[default]
    Size,
    /// The line's (font size × line height).
    Line,
}

impl ImgHeight {
    /// Height in px for a font `size` and a `line_height` factor.
    pub fn px(self, size: f32, line_height: f32) -> f32 {
        match self {
            ImgHeight::Cap => size * 0.7,
            ImgHeight::Size => size,
            ImgHeight::Line => size * line_height,
        }
    }
}

/// Middle of a picture of the text, above the baseline (the capitals' middle):
/// both renderers centre it there.
pub fn image_middle(size: f32) -> f32 {
    size * 0.35
}

/// A picture of the text: its source (data filled in) and its height.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImage {
    pub src: String,
    pub height: ImgHeight,
}

/// `src|cap` → its source and height.
fn img_value(v: &str) -> (String, ImgHeight) {
    match v.rsplit_once('|') {
        Some((src, "cap")) => (src.to_string(), ImgHeight::Cap),
        Some((src, "size")) => (src.to_string(), ImgHeight::Size),
        Some((src, "line")) => (src.to_string(), ImgHeight::Line),
        _ => (v.to_string(), ImgHeight::Size),
    }
}

/// The pictures of a Text layer of `t` as shown with `data`, in their order.
pub fn layer_images(t: &Template, layer: &crate::model::Layer, data: &Data) -> Vec<InlineImage> {
    let LayerKind::Text { text, .. } = &layer.kind else {
        return Vec::new();
    };
    if !text.contains("<img=") && !text.contains("{{") {
        return Vec::new();
    }
    inline_images(text, data, |k| typed_key(t, k))
}

/// The pictures of `markup` (`rich` keys expanded first, as [`runs`] does).
pub fn inline_images(markup: &str, data: &Data, rich: impl Fn(&str) -> bool) -> Vec<InlineImage> {
    let expanded = expand(markup, data, rich);
    let mut out = Vec::new();
    let mut rest = expanded.as_str();
    while let Some(i) = rest.find('<') {
        let at = &rest[i..];
        match open_at(at) {
            Some((Tag::Img(v), len)) => {
                let (src, height) = img_value(&v);
                out.push(InlineImage {
                    src: interpolate_text(&src, data).trim().to_string(),
                    height,
                });
                rest = &at[len..];
            }
            Some((_, len)) => rest = &at[len..],
            None => rest = &at[1..],
        }
    }
    out
}

/// The styled parts of a Text layer of `t` as shown with `data`; `None`
/// when it has no styles (one style for all).
pub fn layer_runs(t: &Template, layer: &crate::model::Layer, data: &Data) -> Option<Vec<Run>> {
    let LayerKind::Text {
        text,
        uppercase,
        transform,
        x,
        ..
    } = &layer.kind
    else {
        return None;
    };
    runs_with(
        text,
        data,
        |k| typed_key(t, k),
        |s| {
            if *uppercase {
                s.to_uppercase()
            } else {
                transform.apply(s)
            }
        },
        &x.var_styles,
    )
}

/// First private character standing for a character of a part measured in
/// its own style (plane 15, which no font draws): the n-th such character
/// of the text is `RUN_BASE + n`.
pub const RUN_BASE: u32 = 0xF_0000;

/// The shown text for the line breaking: the characters of the parts
/// measured in their own style (bold, another font, size…) stand as private
/// characters, so the measure knows their part; `table[n]` is the n-th:
/// (character, index of its run). Spaces and line ends stay as they are.
pub fn encode(runs: &[Run]) -> (String, Vec<(char, usize)>) {
    let mut text = String::new();
    let mut table = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        let own = r.style.metric();
        for c in r.text.chars() {
            if own && !matches!(c, ' ' | '\n') && image_index(c).is_none() {
                if let Some(p) = char::from_u32(RUN_BASE + table.len() as u32) {
                    table.push((c, i));
                    text.push(p);
                    continue;
                }
            }
            text.push(c);
        }
    }
    (text, table)
}

/// The character `c` of an [`encode`]d text stands for, and its run.
pub fn decode(c: char, table: &[(char, usize)]) -> Option<(char, usize)> {
    let v = c as u32;
    v.checked_sub(RUN_BASE)
        .and_then(|n| table.get(n as usize))
        .copied()
}

/// An [`encode`]d line back to the shown characters.
pub fn decoded(line: &str, table: &[(char, usize)]) -> String {
    line.chars()
        .map(|c| decode(c, table).map_or(c, |(c, _)| c))
        .collect()
}

/// The style of a part of the text, over the style of the whole text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// `#rrggbb[aa]`; none = the colour of the text.
    pub color: Option<String>,
    /// A variable's own looks (see [`VarStyle`]): font, weight, size and
    /// letter spacing (px, at the text's `fontSize`), smaller above or below
    /// the line, case.
    pub font_family: Option<String>,
    pub font_weight: Option<u16>,
    pub font_size: Option<f32>,
    pub letter_spacing: Option<f32>,
    pub sup: bool,
    pub sub: bool,
    pub transform: Option<TextTransform>,
}

impl Style {
    /// It is measured otherwise than the rest of the text (another font,
    /// weight, size or spacing).
    pub fn metric(&self) -> bool {
        self.bold
            || self.italic
            || self.sup
            || self.sub
            || self.font_family.is_some()
            || self.font_weight.is_some()
            || self.font_size.is_some()
            || self.letter_spacing.is_some()
    }

    /// The variable's looks over the tags around it.
    fn with_var(&mut self, v: &VarStyle) {
        let set = |on: &mut bool, o: Option<bool>| {
            if let Some(o) = o {
                *on = o;
            }
        };
        set(&mut self.bold, v.bold);
        set(&mut self.italic, v.italic);
        set(&mut self.underline, v.underline);
        set(&mut self.strike, v.strike);
        set(&mut self.sup, v.sup);
        set(&mut self.sub, v.sub);
        if v.color.is_some() {
            self.color = v.color.clone();
        }
        self.font_family = v.font_family.clone().or(self.font_family.take());
        self.font_weight = v.font_weight.or(self.font_weight);
        self.font_size = v.font_size.or(self.font_size);
        self.letter_spacing = v.letter_spacing.or(self.letter_spacing);
        self.transform = v.transform.or(self.transform);
    }
}

/// A part of the text and its style.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

enum Tag {
    Open(char, Option<String>),
    Close(char),
    /// A picture: `src|height`.
    Img(String),
}

/// No variables with looks of their own.
fn no_vars() -> &'static BTreeMap<String, VarStyle> {
    static EMPTY: BTreeMap<String, VarStyle> = BTreeMap::new();
    &EMPTY
}

/// The tag at the start of `s` and its length in bytes (`<v=key>`: the
/// looks of a variable of `vars`, put around it by [`with_vars`]).
fn tag_at(s: &str, vars: &BTreeMap<String, VarStyle>) -> Option<(Tag, usize)> {
    let end = s.find('>')?;
    let inner = &s[1..end];
    if let Some(v) = inner.strip_prefix("img=") {
        return (!v.trim().is_empty()).then(|| (Tag::Img(v.to_string()), end + 1));
    }
    if let Some(k) = inner.strip_prefix("v=") {
        return vars
            .contains_key(k)
            .then(|| (Tag::Open('v', Some(k.to_string())), end + 1));
    }
    let tag = match inner {
        "b" | "i" | "u" | "s" | "c" => return None,
        "/v" if !vars.is_empty() => Tag::Close('v'),
        "/b" | "/i" | "/u" | "/s" | "/c" => Tag::Close(inner.chars().nth(1)?),
        _ => {
            let hex = inner.strip_prefix("c=")?;
            let h = hex.strip_prefix('#')?;
            if !(matches!(h.len(), 3 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())) {
                return None;
            }
            Tag::Open('c', Some(hex.to_string()))
        }
    };
    Some((tag, end + 1))
}

/// `<b>`, `<i>`, `<u>`, `<s>` (bare letters are matched here, `c` needs a colour).
fn open_at(s: &str) -> Option<(Tag, usize)> {
    open_with(s, no_vars())
}

fn open_with(s: &str, vars: &BTreeMap<String, VarStyle>) -> Option<(Tag, usize)> {
    for t in ['b', 'i', 'u', 's'] {
        if s.starts_with(&format!("<{t}>")) {
            return Some((Tag::Open(t, None), 3));
        }
    }
    tag_at(s, vars)
}

/// `markup` with every variable of `vars` set in it wrapped in `<v=key>…</v>`.
fn with_vars(markup: &str, vars: &BTreeMap<String, VarStyle>) -> String {
    if vars.is_empty() {
        return markup.to_string();
    }
    let mut out = String::with_capacity(markup.len() + 16);
    let mut rest = markup;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after.find("}}") {
            Some(end) => {
                let key = after[..end].trim();
                let token = &rest[i..i + 2 + end + 2];
                if vars.contains_key(key) {
                    out.push_str(&format!("<v={key}>{token}</v>"));
                } else {
                    out.push_str(token);
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether `s` has style tags.
pub fn has_markup(s: &str) -> bool {
    s.match_indices('<')
        .any(|(i, _)| open_at(&s[i..]).is_some())
}

/// The parts of `markup` with their style (one part, unstyled, without tags).
pub fn parse(markup: &str) -> Vec<Run> {
    parse_with(markup, no_vars())
}

/// [`parse`], with the looks of the variables (`<v=key>` tags).
fn parse_with(markup: &str, vars: &BTreeMap<String, VarStyle>) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut stack: Vec<(char, Option<String>)> = Vec::new();
    let style = |stack: &[(char, Option<String>)]| {
        let mut s = Style::default();
        for (t, v) in stack {
            match t {
                'b' => s.bold = true,
                'i' => s.italic = true,
                'u' => s.underline = true,
                's' => s.strike = true,
                'v' => {
                    if let Some(vs) = v.as_ref().and_then(|k| vars.get(k)) {
                        s.with_var(vs);
                    }
                }
                _ => s.color = v.clone(),
            }
        }
        s
    };
    let mut cur = String::new();
    let mut rest = markup;
    let mut pictures = 0u32;
    let flush = |cur: &mut String, runs: &mut Vec<Run>, st: Style| {
        if cur.is_empty() {
            return;
        }
        match runs.last_mut() {
            Some(r) if r.style == st => r.text.push_str(cur),
            _ => runs.push(Run {
                text: cur.clone(),
                style: st,
            }),
        }
        cur.clear();
    };
    while let Some(i) = rest.find('<') {
        cur.push_str(&rest[..i]);
        let at = &rest[i..];
        match open_with(at, vars) {
            Some((Tag::Img(_), len)) => {
                // One character as wide as the picture, in the style around it.
                if let Some(c) = char::from_u32(IMG_BASE + pictures) {
                    cur.push(c);
                }
                pictures += 1;
                rest = &at[len..];
            }
            Some((tag, len)) => {
                flush(&mut cur, &mut runs, style(&stack));
                match tag {
                    Tag::Img(_) => {}
                    Tag::Open(t, v) => stack.push((t, v)),
                    Tag::Close(t) => {
                        if let Some(p) = stack.iter().rposition(|(x, _)| *x == t) {
                            stack.remove(p);
                        }
                    }
                }
                rest = &at[len..];
            }
            None => {
                cur.push('<');
                rest = &at[1..];
            }
        }
    }
    cur.push_str(rest);
    flush(&mut cur, &mut runs, style(&stack));
    runs
}

/// The text without its tags (nor its pictures).
pub fn plain(markup: &str) -> String {
    parse(markup)
        .into_iter()
        .flat_map(|r| r.text.chars().collect::<Vec<_>>())
        .filter(|c| image_index(*c).is_none())
        .collect()
}

/// `markup` with the values of the `rich` keys (typed by the operator, who
/// may style them) put in, as if typed in the text.
fn expand(markup: &str, data: &Data, rich: impl Fn(&str) -> bool) -> String {
    let mut expanded = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(i) = rest.find("{{") {
        expanded.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after.find("}}") {
            Some(end) if rich(after[..end].trim()) && data.contains_key(after[..end].trim()) => {
                expanded.push_str(&data[after[..end].trim()]);
                rest = &after[end + 2..];
            }
            _ => {
                expanded.push_str("{{");
                rest = after;
            }
        }
    }
    expanded.push_str(rest);
    expanded
}

/// The parts of a text as shown: its `{{key}}` replaced by the data (which
/// take the style around them; the values of `rich` keys, typed by the
/// operator, may have styles of their own) and its case changed by `case`
/// (on the whole text when that keeps its length, so "Capitalize" sees whole
/// words). `None` when the text has no style tags.
pub fn runs(
    markup: &str,
    data: &Data,
    rich: impl Fn(&str) -> bool,
    case: impl Fn(&str) -> String,
) -> Option<Vec<Run>> {
    runs_with(markup, data, rich, case, no_vars())
}

/// [`runs`], the variables of `vars` in their own looks (and case).
pub fn runs_with(
    markup: &str,
    data: &Data,
    rich: impl Fn(&str) -> bool,
    case: impl Fn(&str) -> String,
    vars: &BTreeMap<String, VarStyle>,
) -> Option<Vec<Run>> {
    // The variables in their own looks, then the operator's styled values,
    // as if typed in the text.
    let wrapped = with_vars(markup, vars);
    let expanded = expand(&wrapped, data, rich);
    let markup = expanded.as_str();
    if !has_markup(markup) && !(!vars.is_empty() && markup.contains("<v=")) {
        return None;
    }
    let mut runs: Vec<Run> = parse_with(markup, vars)
        .into_iter()
        .map(|r| Run {
            text: interpolate_text(&r.text, data),
            style: r.style,
        })
        .collect();
    let typed: Vec<String> = runs.iter().map(|r| r.text.clone()).collect();
    let whole: String = typed.concat();
    let cased = case(&whole);
    if cased.chars().count() == whole.chars().count() {
        let mut chars = cased.chars();
        for r in &mut runs {
            let n = r.text.chars().count();
            r.text = chars.by_ref().take(n).collect();
        }
    } else {
        for r in &mut runs {
            r.text = case(&r.text);
        }
    }
    // A variable's own case, on what was typed.
    for (r, t) in runs.iter_mut().zip(typed) {
        if let Some(tr) = r.style.transform {
            r.text = tr.apply(&t);
        }
    }
    runs.retain(|r| !r.text.is_empty());
    Some(runs)
}

/// A text written with style tags (`<b>`, `<c=…>`…), without them: each
/// styled part becomes a variable keeping its looks (the variables already
/// there keep theirs); `new_var(text)` makes a variable for a typed part
/// and gives its key. Pictures stay as they are. `None` without style tags.
pub fn unstyle(
    markup: &str,
    mut new_var: impl FnMut(&str) -> String,
) -> Option<(String, Vec<(String, VarStyle)>)> {
    let mut parts: Vec<(String, Style)> = Vec::new();
    let mut stack: Vec<(char, Option<String>)> = Vec::new();
    let mut styled = false;
    let mut cur = String::new();
    let style = |stack: &[(char, Option<String>)]| {
        let mut s = Style::default();
        for (t, v) in stack {
            match t {
                'b' => s.bold = true,
                'i' => s.italic = true,
                'u' => s.underline = true,
                's' => s.strike = true,
                _ => s.color = v.clone(),
            }
        }
        s
    };
    let mut rest = markup;
    while let Some(i) = rest.find('<') {
        cur.push_str(&rest[..i]);
        let at = &rest[i..];
        match open_at(at) {
            Some((Tag::Img(_), len)) => {
                cur.push_str(&at[..len]);
                rest = &at[len..];
            }
            Some((tag, len)) => {
                styled = true;
                if !cur.is_empty() {
                    parts.push((std::mem::take(&mut cur), style(&stack)));
                }
                match tag {
                    Tag::Open(t, v) => stack.push((t, v)),
                    Tag::Close(t) => {
                        if let Some(p) = stack.iter().rposition(|(x, _)| *x == t) {
                            stack.remove(p);
                        }
                    }
                    Tag::Img(_) => {}
                }
                rest = &at[len..];
            }
            None => {
                cur.push('<');
                rest = &at[1..];
            }
        }
    }
    cur.push_str(rest);
    if !cur.is_empty() {
        parts.push((cur, style(&stack)));
    }
    if !styled {
        return None;
    }
    let looks = |s: &Style| VarStyle {
        bold: s.bold.then_some(true),
        italic: s.italic.then_some(true),
        underline: s.underline.then_some(true),
        strike: s.strike.then_some(true),
        color: s.color.clone(),
        ..VarStyle::default()
    };
    let mut out = String::new();
    let mut vars: Vec<(String, VarStyle)> = Vec::new();
    for (text, st) in parts {
        if st == Style::default() {
            out.push_str(&text);
            continue;
        }
        // Its variables, pictures and typed pieces.
        let mut rest = text.as_str();
        let mut typed = |piece: &str, out: &mut String, vars: &mut Vec<(String, VarStyle)>| {
            let trimmed = piece.trim();
            if trimmed.is_empty() {
                out.push_str(piece);
                return;
            }
            let lead = &piece[..piece.len() - piece.trim_start().len()];
            let trail = &piece[piece.trim_end().len()..];
            let key = new_var(trimmed);
            out.push_str(&format!("{lead}{{{{{key}}}}}{trail}"));
            vars.push((key, looks(&st)));
        };
        loop {
            let tok = rest.find("{{");
            let img = rest.find("<img=");
            let next = match (tok, img) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let Some(at) = next else {
                typed(rest, &mut out, &mut vars);
                break;
            };
            typed(&rest[..at], &mut out, &mut vars);
            let after = &rest[at..];
            let end = if after.starts_with("{{") {
                after.find("}}").map(|e| e + 2)
            } else {
                after.find('>').map(|e| e + 1)
            };
            let Some(end) = end else {
                typed(after, &mut out, &mut vars);
                break;
            };
            let piece = &after[..end];
            out.push_str(piece);
            if let Some(key) = piece.strip_prefix("{{").and_then(|p| p.strip_suffix("}}")) {
                vars.push((key.trim().to_string(), looks(&st)));
            }
            rest = &after[end..];
        }
    }
    Some((out, vars))
}

/// Spaces the line breaking may change (French spaces before `: ; ! ?`).
fn space(c: char) -> bool {
    matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\n' | '\t')
}

/// Follows the shown lines through the text: the parts of each line, as
/// `(text, index of the run)`. Characters the breaking added (a hyphen, "…")
/// take the style before them; spaces it dropped are skipped.
pub fn align(runs: &[Run], lines: &[&str]) -> Vec<Vec<(String, usize)>> {
    let chars: Vec<(char, usize)> = runs
        .iter()
        .enumerate()
        .flat_map(|(i, r)| r.text.chars().map(move |c| (c, i)))
        .collect();
    let mut at = 0usize;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let mut parts: Vec<(String, usize)> = Vec::new();
        for ch in line.chars() {
            // Skip what the breaking dropped (spaces, line ends) when the
            // character comes right after them.
            if !space(ch) {
                let mut j = at;
                while j < chars.len() && chars[j].0 != ch && space(chars[j].0) {
                    j += 1;
                }
                if j < chars.len() && chars[j].0 == ch {
                    at = j;
                }
            }
            let run =
                if at < chars.len() && (chars[at].0 == ch || (space(ch) && space(chars[at].0))) {
                    at += 1;
                    chars[at - 1].1
                } else {
                    // Added by the breaking: the style before.
                    chars.get(at.saturating_sub(1)).map_or(0, |c| c.1)
                };
            match parts.last_mut() {
                Some((s, r)) if *r == run => s.push(ch),
                _ => parts.push((ch.to_string(), run)),
            }
        }
        out.push(parts);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(f: impl Fn(&mut Style)) -> Style {
        let mut s = Style::default();
        f(&mut s);
        s
    }

    #[test]
    fn tags_give_styles() {
        let r = parse("Hello <b>big <c=#ff0000>red</c></b> world");
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].text, "Hello ");
        assert_eq!(
            (r[1].text.as_str(), &r[1].style),
            ("big ", &st(|s| s.bold = true))
        );
        assert_eq!(
            (r[2].text.as_str(), &r[2].style),
            (
                "red",
                &st(|s| {
                    s.bold = true;
                    s.color = Some("#ff0000".into())
                })
            )
        );
        assert_eq!(r[3].text, " world");
        assert_eq!(plain("a <i>b</i> c"), "a b c");
        // Not tags: kept as text.
        assert_eq!(plain("1 < 2 <x> <c=red>a</c>"), "1 < 2 <x> <c=red>a");
        assert!(!has_markup("a < b"));
        assert!(has_markup("<u>a</u>"));
    }

    #[test]
    fn data_takes_the_style_around_it_and_never_adds_any() {
        let data: Data = [("name".to_string(), "<b>Zoé</b>".to_string())].into();
        let r = runs(
            "<c=#00ff00>{{name}}</c> !",
            &data,
            |_| false,
            |t| t.to_uppercase(),
        )
        .unwrap();
        assert_eq!(r[0].text, "<B>ZOÉ</B>");
        assert_eq!(r[0].style.color.as_deref(), Some("#00ff00"));
        assert_eq!(r[1].text, " !");
        assert!(runs("plain {{name}}", &data, |_| false, |t| t.to_string()).is_none());
        // Typed by the operator: its own styles count.
        let r = runs("{{name}} !", &data, |k| k == "name", |t| t.to_string()).unwrap();
        assert_eq!((r[0].text.as_str(), r[0].style.bold), ("Zoé", true));
    }

    #[test]
    fn pictures_are_one_character_each() {
        let data: Data = [("flag".to_string(), "media/fr.png".to_string())].into();
        let m = "Paris <img={{flag}}|cap> <b>24 <img=media/sun.svg></b>°";
        let r = runs(m, &data, |_| false, |t| t.to_uppercase()).unwrap();
        let shown: String = r.iter().map(|r| r.text.as_str()).collect();
        let a = char::from_u32(IMG_BASE).unwrap();
        let b = char::from_u32(IMG_BASE + 1).unwrap();
        assert_eq!(shown, format!("PARIS {a} 24 {b}°"));
        assert!(r[1].style.bold && r[1].text.ends_with(b));
        assert_eq!(image_index(b), Some(1));
        let imgs = inline_images(m, &data, |_| false);
        assert_eq!(
            imgs,
            vec![
                InlineImage {
                    src: "media/fr.png".into(),
                    height: ImgHeight::Cap
                },
                InlineImage {
                    src: "media/sun.svg".into(),
                    height: ImgHeight::Size
                },
            ]
        );
        assert_eq!(plain(m), "Paris  24 °");
        // An empty picture tag stays text.
        assert_eq!(plain("a <img=> b"), "a <img=> b");
    }

    #[test]
    fn styled_parts_become_variables() {
        let mut made = Vec::new();
        let (text, vars) = unstyle(
            "Hello <b>big <c=#ff0000>{{name}}</c></b> <img=a.png> <i>end</i>",
            |t| {
                made.push(t.to_string());
                format!("v{}", made.len())
            },
        )
        .unwrap();
        assert_eq!(text, "Hello {{v1}} {{name}} <img=a.png> {{v2}}");
        assert_eq!(made, ["big", "end"]);
        assert_eq!(vars[0].0, "v1");
        assert_eq!(vars[0].1.bold, Some(true));
        assert_eq!(vars[1].0, "name");
        assert_eq!(vars[1].1.color.as_deref(), Some("#ff0000"));
        assert_eq!(vars[2].1.italic, Some(true));
        assert!(unstyle("plain <img=a.png> {{x}}", |_| String::new()).is_none());
    }

    #[test]
    fn variables_have_their_own_looks() {
        let vars: BTreeMap<String, VarStyle> = [(
            "n".to_string(),
            VarStyle {
                font_size: Some(60.0),
                transform: Some(TextTransform::Upper),
                ..VarStyle::default()
            },
        )]
        .into();
        let data: Data = [("n".to_string(), "Zoé".to_string())].into();
        let r = runs_with(
            "Bonjour {{n}} !",
            &data,
            |_| false,
            |t| t.to_string(),
            &vars,
        )
        .unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(
            (r[1].text.as_str(), r[1].style.font_size),
            ("ZOÉ", Some(60.0))
        );
        assert!(r[1].style.metric() && !r[0].style.metric());
        let (enc, table) = encode(&r);
        assert_eq!(decoded(&enc, &table), "Bonjour ZOÉ !");
        assert_ne!(enc, "Bonjour ZOÉ !");
    }

    #[test]
    fn lines_keep_their_styles() {
        let r = parse("ab <b>cd ef</b> gh");
        // Broken as "ab cd" / "ef gh", or with a hyphen added.
        let l = align(&r, &["ab cd", "ef gh"]);
        assert_eq!(l[0], vec![("ab ".into(), 0), ("cd".into(), 1)]);
        assert_eq!(l[1], vec![("ef".into(), 1), (" gh".into(), 2)]);
        let l = align(&r, &["ab c-", "d ef…"]);
        assert_eq!(l[0], vec![("ab ".into(), 0), ("c-".into(), 1)]);
        assert_eq!(l[1], vec![("d ef…".into(), 1)]);
    }
}

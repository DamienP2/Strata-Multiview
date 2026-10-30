//! Social wall: the approved messages of a social source (`approved` of
//! `social-<project>`, a JSON list in a field, or a data set's table) as a
//! stack of cards: avatar (picture, or initials on a colour from the name),
//! name, platform badge, city, text wrapped to a few lines, photo. A card
//! coming in slides and fades in, the others move to their new place, a
//! card leaving fades out: the change is kept in the state (`on_data`) and
//! played from the wall clock, the same in both renderers.
//!
//! The layout is here ([`layout`], with the renderer's text measure); it is
//! painted by `cg-render/src/wid/socialwall.rs` and
//! `web/src/widgets/socialwall.tsx` (mirror, keep in sync). Platform colours
//! and the avatar colour are shared with the badge widget (`socialbadge`).

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind};

/// Where the stack sits in the box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    #[default]
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// The messages: `{{key}}` of a field holding a JSON list of messages
    /// (or a data set's table).
    pub items: String,
    /// The messages typed by hand, kept while a data set feeds the wall
    /// (back by hand, they return).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hand_items: String,
    /// Cards shown at most (the first ones of the list).
    pub max: u32,
    pub anchor: Anchor,
    /// Between two cards, px.
    pub gap: f32,
    /// Inside a card, px.
    pub pad: f32,
    pub radius: f32,
    pub bg: String,
    pub name_color: String,
    pub text_color: String,
    /// The city.
    pub muted: String,
    pub font: String,
    pub name_size: f32,
    pub text_size: f32,
    /// Lines of text at most ("…" after).
    pub lines: u32,
    pub avatar: bool,
    pub avatar_size: f32,
    pub badge: bool,
    pub city: bool,
    pub photo: bool,
    pub photo_h: f32,
    /// A left edge in the platform's colour.
    pub edge: bool,
    pub edge_w: f32,
    /// A card coming in or leaving, seconds (0 = cut).
    pub anim: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            items: String::new(),
            hand_items: String::new(),
            max: 3,
            anchor: Anchor::Top,
            gap: 12.0,
            pad: 16.0,
            radius: 14.0,
            bg: "#f2f3f5".into(),
            name_color: "#0b0d18".into(),
            text_color: "#1f2433".into(),
            muted: "#6b7280".into(),
            font: "Inter".into(),
            name_size: 20.0,
            text_size: 20.0,
            lines: 3,
            avatar: true,
            avatar_size: 40.0,
            badge: true,
            city: true,
            photo: true,
            photo_h: 130.0,
            edge: false,
            edge_w: 6.0,
            anim: 0.6,
        }
    }
}

// --------------------------------------------------------------- messages --

/// A message of the list (missing keys are empty).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Msg {
    pub id: String,
    pub platform: String,
    pub platform_name: String,
    pub name: String,
    pub handle: String,
    pub city: String,
    pub text: String,
    pub avatar: String,
    pub photo: String,
    pub amount: String,
    pub time: String,
}

fn cell(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.trim().to_string(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

impl Msg {
    /// From a JSON object of the social source (or a table row as one):
    /// its keys matched by name, whatever their case.
    pub fn of(o: &serde_json::Map<String, serde_json::Value>) -> Msg {
        let g = |k: &str| {
            cell(o.get(k).or_else(|| {
                o.iter()
                    .find(|(x, _)| x.eq_ignore_ascii_case(k))
                    .map(|(_, v)| v)
            }))
        };
        let mut m = Msg {
            id: g("id"),
            platform: g("platform").to_lowercase(),
            platform_name: g("platformName"),
            name: g("name"),
            handle: g("handle"),
            city: g("city"),
            text: g("text"),
            avatar: g("avatar"),
            photo: g("photo"),
            amount: g("amount"),
            time: g("time"),
        };
        if m.name.is_empty() {
            m.name = m.handle.clone();
        }
        if m.id.is_empty() {
            m.id = format!("{}|{}", m.name, m.text);
        }
        m
    }
}

/// The messages of a value: a JSON list of objects, a data set's table
/// (`[[header…], [cells…]…]`: its columns by name, a row's id its index
/// when there is no id column) or an object with one under `approved`.
pub fn parse(text: &str) -> Vec<Msg> {
    let t = text.trim();
    if !t.starts_with('[') && !t.starts_with('{') {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(t) else {
        return Vec::new();
    };
    let list = match &v {
        serde_json::Value::Array(a) => a.clone(),
        serde_json::Value::Object(o) => o
            .get("approved")
            .and_then(|a| a.as_array().cloned())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let has_text = |m: &Msg| !m.text.is_empty() || !m.name.is_empty();
    if let Some((head, rows)) = super::table::header_rows(&list) {
        let head: Vec<String> = head.iter().map(|h| h.trim().to_string()).collect();
        let ids = head.iter().any(|h| h.eq_ignore_ascii_case("id"));
        return rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let o = head
                    .iter()
                    .zip(r)
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                let mut m = Msg::of(&o);
                if !ids {
                    m.id = i.to_string();
                }
                m
            })
            .filter(has_text)
            .collect();
    }
    list.iter()
        .filter_map(|v| v.as_object())
        .map(Msg::of)
        .filter(has_text)
        .collect()
}

/// The messages of the layer with this data, the first `max` of them.
pub fn messages(c: &Config, data: &Data) -> Vec<Msg> {
    let mut m = parse(&interpolate_text(&c.items, data));
    m.truncate(c.max.max(1) as usize);
    m
}

// -------------------------------------------------------------- platforms --

/// The look of a platform's badge: its colour and its glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Play,
    Bubble,
    F,
    Camera,
    X,
    Note,
    Phone,
    Dots,
    /// The first letter of the platform's name.
    Letter,
}

/// Colour and glyph of a platform (an unknown one: slate and its letter).
pub fn platform(p: &str) -> (&'static str, Glyph) {
    match p.trim().to_lowercase().as_str() {
        "youtube" => ("#ff0033", Glyph::Play),
        "twitch" => ("#9146ff", Glyph::Bubble),
        "facebook" => ("#1877f2", Glyph::F),
        "instagram" => ("#e1306c", Glyph::Camera),
        "x" | "twitter" => ("#000000", Glyph::X),
        "tiktok" => ("#000000", Glyph::Note),
        "whatsapp" => ("#25d366", Glyph::Phone),
        "sms" => ("#64748b", Glyph::Dots),
        _ => ("#64748b", Glyph::Letter),
    }
}

/// TikTok's two edge colours (cyan, pink).
pub const TIKTOK_EDGES: [&str; 2] = ["#25f4ee", "#fe2c55"];

/// Avatar colours (light: initials in ink on them).
pub const AVATARS: [&str; 8] = [
    "#93c5fd", "#f9a8d4", "#86efac", "#fcd34d", "#c4b5fd", "#fdba74", "#67e8f9", "#fca5a5",
];
pub const AVATAR_INK: &str = "#0b0d18";

/// The avatar colour of a name (the same name, the same colour).
pub fn avatar_color(name: &str) -> &'static str {
    let h = name
        .trim()
        .chars()
        .fold(0u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
    AVATARS[(h % AVATARS.len() as u32) as usize]
}

/// Initials of a name: the first letter of its first two words (a handle's
/// `@` left out), in capitals.
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| {
            w.trim_start_matches('@')
                .chars()
                .find(|c| c.is_alphanumeric())
        })
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

// ----------------------------------------------------------------- layout --

/// Width of a line of text (text, size, weight), from the renderer's fonts.
pub type Measure<'a> = dyn FnMut(&str, f32, u16) -> f32 + 'a;

pub const NAME_WEIGHT: u16 = 700;
pub const TEXT_WEIGHT: u16 = 500;
pub const CITY_WEIGHT: u16 = 500;

/// `s` cut to `max_w` with "…".
pub fn ellipsize(s: &str, max_w: f32, size: f32, weight: u16, m: &mut Measure) -> String {
    if m(s, size, weight) <= max_w {
        return s.to_string();
    }
    let mut cs: Vec<char> = s.chars().collect();
    while !cs.is_empty() {
        cs.pop();
        let t = format!("{}…", cs.iter().collect::<String>().trim_end());
        if m(&t, size, weight) <= max_w {
            return t;
        }
    }
    String::new()
}

/// The text in lines of at most `max_w`, `k` lines at most (the last one
/// ends with "…" when some is left out).
pub fn wrap(
    text: &str,
    max_w: f32,
    k: usize,
    size: f32,
    weight: u16,
    m: &mut Measure,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for para in text.lines() {
        let mut cur = String::new();
        // Not at a no-break space (French « ? », « : »…).
        let breaks = |c: char| c.is_whitespace() && c != '\u{a0}' && c != '\u{202f}';
        for word in para.split(breaks).filter(|w| !w.is_empty()) {
            let cand = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if !cur.is_empty() && m(&cand, size, weight) > max_w {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = cand;
            }
        }
        lines.push(cur);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let k = k.max(1);
    let cut = lines.len() > k;
    lines.truncate(k);
    if let Some(last) = lines.last_mut() {
        if cut {
            *last = ellipsize(&format!("{last}…"), max_w, size, weight, m);
            if !last.ends_with('…') {
                last.push('…');
            }
        }
    }
    lines
        .into_iter()
        .map(|l| ellipsize(&l, max_w, size, weight, m))
        .collect()
}

/// A card laid out (layer px; its top at 0).
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub h: f32,
    /// Left of the avatar, of the texts.
    pub left: f32,
    pub x0: f32,
    pub name: String,
    pub name_w: f32,
    /// "· City" (empty: none, or no room).
    pub city: String,
    pub lines: Vec<String>,
    /// Photo: top, width, height.
    pub photo: Option<(f32, f32, f32)>,
}

pub fn header_h(c: &Config) -> f32 {
    c.name_size * 1.3
}
pub fn line_h(c: &Config) -> f32 {
    c.text_size * 1.3
}
/// The badge's side, px.
pub fn badge_size(c: &Config) -> f32 {
    (c.name_size * 0.9).round()
}

pub fn card(c: &Config, msg: &Msg, w: f32, m: &mut Measure) -> Card {
    let pad = c.pad.max(0.0);
    let a = if c.avatar {
        c.avatar_size.max(0.0)
    } else {
        0.0
    };
    let left = if c.edge { c.edge_w.max(0.0) } else { 0.0 } + pad;
    let x0 = left + if a > 0.0 { a + pad * 0.75 } else { 0.0 };
    let cw = (w - x0 - pad).max(10.0);
    let gap = c.name_size * 0.3;
    let bs = if c.badge { badge_size(c) + gap } else { 0.0 };
    let name = ellipsize(&msg.name, (cw - bs).max(0.0), c.name_size, NAME_WEIGHT, m);
    let name_w = m(&name, c.name_size, NAME_WEIGHT);
    let mut city = String::new();
    if c.city && !msg.city.is_empty() {
        let s = format!("· {}", msg.city);
        let room = cw - name_w - bs - gap;
        if m(&s, c.name_size * 0.85, CITY_WEIGHT) <= room {
            city = s;
        }
    }
    let lines = if msg.text.trim().is_empty() {
        Vec::new()
    } else {
        wrap(&msg.text, cw, c.lines as usize, c.text_size, TEXT_WEIGHT, m)
    };
    let mut top = header_h(c);
    if !lines.is_empty() {
        top += c.text_size * 0.15 + lines.len() as f32 * line_h(c);
    }
    let mut photo = None;
    if c.photo && !msg.photo.is_empty() && c.photo_h > 0.0 {
        let ph = c.photo_h;
        let pw = cw.min(ph * 16.0 / 9.0);
        let py = top + pad * 0.6;
        photo = Some((pad + py, pw, ph));
        top = py + ph;
    }
    Card {
        h: (pad * 2.0 + top.max(a)).round(),
        left,
        x0,
        name,
        name_w,
        city,
        lines,
        photo,
    }
}

/// A card placed: where, how visible (coming in, moving, leaving).
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub msg: Msg,
    pub card: Card,
    pub x: f32,
    pub y: f32,
    pub alpha: f32,
}

/// Tops of cards of these heights stacked in a box of height `h`.
fn stack(c: &Config, hs: &[f32], h: f32) -> Vec<f32> {
    let total: f32 = hs.iter().sum::<f32>() + c.gap * hs.len().saturating_sub(1) as f32;
    let mut y = match c.anchor {
        Anchor::Top => 0.0,
        Anchor::Bottom => h - total,
    };
    hs.iter()
        .map(|ch| {
            let top = y;
            y += ch + c.gap;
            top
        })
        .collect()
}

/// The cards that fit the box (the first one always).
fn fitting(c: &Config, cards: Vec<(Msg, Card)>, h: f32) -> Vec<(Msg, Card)> {
    let mut used = 0.0;
    let mut out = Vec::new();
    for (i, (msg, card)) in cards.into_iter().enumerate() {
        let next = used + card.h + if i > 0 { c.gap } else { 0.0 };
        if i > 0 && next > h + 0.5 {
            break;
        }
        used = next;
        out.push((msg, card));
    }
    out
}

/// Progress of the last change at `now` (1 = done).
pub fn progress(c: &Config, st: &State, now: i64) -> f32 {
    if st.at <= 0 || c.anim <= 0.0 {
        return 1.0;
    }
    ((now - st.at) as f32 / (c.anim * 1000.0)).clamp(0.0, 1.0)
}

fn ease_out(k: f32) -> f32 {
    1.0 - (1.0 - k).powi(3)
}

/// Share of the change after which a new card comes in (the others have
/// made room for it by then).
pub const IN_FROM: f32 = 0.4;

/// Slide of a card coming in or leaving, px.
pub const SLIDE: f32 = 40.0;

/// The cards of the wall at `now` (w × h box).
#[allow(clippy::too_many_arguments)]
pub fn layout(
    c: &Config,
    st: &State,
    data: &Data,
    w: f32,
    h: f32,
    now: i64,
    m: &mut Measure,
) -> Vec<Placed> {
    let msgs = messages(c, data);
    let cur: Vec<(Msg, Card)> = msgs
        .into_iter()
        .map(|msg| {
            let k = card(c, &msg, w, m);
            (msg, k)
        })
        .collect();
    let cur = fitting(c, cur, h);
    let ys = stack(c, &cur.iter().map(|x| x.1.h).collect::<Vec<_>>(), h);
    let t = progress(c, st, now);
    let (k, kin) = (
        ease_out(t),
        ease_out(((t - IN_FROM) / (1.0 - IN_FROM)).clamp(0.0, 1.0)),
    );
    if t >= 1.0 {
        return cur
            .into_iter()
            .zip(ys)
            .map(|((msg, card), y)| Placed {
                msg,
                card,
                x: 0.0,
                y,
                alpha: 1.0,
            })
            .collect();
    }
    // Before the change: the cards shown then, at their places then.
    let before: Vec<(Msg, Card)> = st
        .prev
        .iter()
        .filter_map(|id| {
            cur.iter().find(|x| &x.0.id == id).cloned().or_else(|| {
                st.gone.iter().find(|g| &g.id == id).map(|g| {
                    let k = card(c, g, w, m);
                    (g.clone(), k)
                })
            })
        })
        .collect();
    let ys0 = stack(c, &before.iter().map(|x| x.1.h).collect::<Vec<_>>(), h);
    let mut out = Vec::new();
    // Leaving: where they were, fading out.
    for ((msg, card), y) in before.iter().zip(&ys0) {
        if !cur.iter().any(|x| x.0.id == msg.id) {
            out.push(Placed {
                msg: msg.clone(),
                card: card.clone(),
                x: -SLIDE * k,
                y: *y,
                alpha: 1.0 - k,
            });
        }
    }
    for ((msg, card), y) in cur.into_iter().zip(ys) {
        let was = before.iter().position(|x| x.0.id == msg.id);
        out.push(match was {
            Some(i) => Placed {
                y: ys0[i] + (y - ys0[i]) * k,
                msg,
                card,
                x: 0.0,
                alpha: 1.0,
            },
            None => Placed {
                msg,
                card,
                x: SLIDE * (1.0 - kin),
                y,
                alpha: kin,
            },
        });
    }
    out
}

// ------------------------------------------------------------------ state --

/// The last change of the cards shown (none: shown as they are).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// Ids of the cards shown before it, in order.
    pub prev: Vec<String>,
    /// The cards that left then (fading out).
    pub gone: Vec<Msg>,
    /// When, Unix ms (0 = never).
    pub at: i64,
}

/// No operator action.
pub fn act(
    _layer: &Layer,
    _st: &mut State,
    op: &str,
    _args: &serde_json::Value,
    _ctx: &ActCtx,
) -> Result<(), String> {
    Err(format!("unknown action `{op}`"))
}

/// The data changed on air: when the cards shown change, the change is
/// kept to be played (cards coming in, moving, leaving).
pub fn on_data(layer: &Layer, st: &mut State, old: &Data, ctx: &ActCtx) -> bool {
    let LayerKind::SocialWall { x: c } = &layer.kind else {
        return false;
    };
    let before = messages(c, old);
    let after = messages(c, ctx.data);
    let ids = |v: &[Msg]| v.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
    if ids(&before) == ids(&after) {
        return false;
    }
    st.gone = before
        .iter()
        .filter(|m| !after.iter().any(|a| a.id == m.id))
        .cloned()
        .collect();
    st.prev = ids(&before);
    st.at = ctx.now_ms;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character as wide as half the size.
    fn half(s: &str, size: f32, _w: u16) -> f32 {
        s.chars().count() as f32 * size * 0.5
    }

    fn msg(id: &str, name: &str, text: &str) -> serde_json::Value {
        serde_json::json!({"id": id, "platform": "youtube", "platformName": "YouTube", "name": name,
            "city": "Lyon", "text": text, "avatar": "", "photo": ""})
    }

    fn data(list: &[serde_json::Value]) -> Data {
        let mut d = Data::new();
        d.insert(
            "wall".into(),
            serde_json::Value::Array(list.to_vec()).to_string(),
        );
        d
    }

    fn cfg() -> Config {
        Config {
            items: "{{wall}}".into(),
            ..Default::default()
        }
    }

    #[test]
    fn text_wraps_to_its_lines_with_an_ellipsis() {
        let mut m = half;
        // 10 px a character at 20 px: 10 characters in 100 px.
        let l = wrap("un deux trois quatre", 100.0, 3, 20.0, 500, &mut m);
        assert_eq!(l, vec!["un deux", "trois", "quatre"]);
        let l = wrap("un deux trois quatre cinq", 100.0, 2, 20.0, 500, &mut m);
        assert_eq!(l.len(), 2);
        assert!(
            l[1].ends_with('…') && half(&l[1], 20.0, 500) <= 100.0,
            "{l:?}"
        );
        // Not at a no-break space.
        let l = wrap("ab cd\u{a0}?", 60.0, 3, 20.0, 500, &mut m);
        assert_eq!(l, vec!["ab", "cd\u{a0}?"]);
        // A word longer than the line is cut.
        let l = wrap("anticonstitutionnellement", 100.0, 3, 20.0, 500, &mut m);
        assert_eq!(l.len(), 1);
        assert!(l[0].ends_with('…'));
    }

    #[test]
    fn card_height_follows_its_lines_and_photo() {
        let c = cfg();
        let mut m = half;
        let short = Msg::of(msg("1", "Ana", "Bravo").as_object().unwrap());
        let long = Msg::of(msg("2", "Ana", &"mot ".repeat(80)).as_object().unwrap());
        let a = card(&c, &short, 600.0, &mut m);
        let b = card(&c, &long, 600.0, &mut m);
        assert_eq!(a.lines.len(), 1);
        assert_eq!(b.lines.len(), 3, "at most `lines` lines");
        // pad 16 ×2 + header 26 + 3 + lines × 26.
        assert_eq!(a.h, (32.0f32 + 26.0 + 3.0 + 26.0).round());
        assert_eq!(b.h - a.h, 2.0 * line_h(&c));
        let mut with_photo = short.clone();
        with_photo.photo = "https://example.com/p.jpg".into();
        let p = card(&c, &with_photo, 600.0, &mut m);
        let (_, pw, ph) = p.photo.expect("a photo");
        assert_eq!(ph, c.photo_h);
        assert!(pw <= 600.0 - p.x0 - c.pad);
        assert!(p.h > a.h + c.photo_h);
        // No photo when it is off.
        let off = Config {
            photo: false,
            ..cfg()
        };
        assert_eq!(card(&off, &with_photo, 600.0, &mut m).h, a.h);
    }

    #[test]
    fn at_most_n_cards_and_those_that_fit() {
        let list: Vec<_> = (0..6)
            .map(|i| msg(&i.to_string(), "Ana", "Bravo"))
            .collect();
        let d = data(&list);
        let st = State::default();
        let mut m = half;
        let c = cfg();
        let p = layout(&c, &st, &d, 600.0, 1000.0, 0, &mut m);
        assert_eq!(p.len(), 3);
        assert_eq!(p[1].y, p[0].card.h + c.gap);
        // A short box: only what fits (the first card always).
        let h = p[0].card.h * 2.0 + c.gap;
        assert_eq!(layout(&c, &st, &d, 600.0, h, 0, &mut m).len(), 2);
        assert_eq!(layout(&c, &st, &d, 600.0, 10.0, 0, &mut m).len(), 1);
        // From the bottom.
        let b = Config {
            anchor: Anchor::Bottom,
            ..cfg()
        };
        let p = layout(&b, &st, &d, 600.0, 1000.0, 0, &mut m);
        let last = p.last().unwrap();
        assert_eq!(last.y + last.card.h, 1000.0);
        // Nothing to show.
        assert!(layout(&c, &st, &Data::new(), 600.0, 1000.0, 0, &mut m).is_empty());
    }

    #[test]
    fn a_new_card_comes_in_and_the_first_one_leaves() {
        let layer: Layer = serde_json::from_value(serde_json::json!({
            "id": "w", "name": "Mur", "type": "socialWall", "items": "{{wall}}",
            "props": {"width": 600.0, "height": 1000.0}
        }))
        .unwrap();
        let LayerKind::SocialWall { x: c } = &layer.kind else {
            panic!("a wall");
        };
        let old = data(&[msg("a", "Ana", "1"), msg("b", "Bob", "2")]);
        let new = data(&[msg("b", "Bob", "2"), msg("c", "Cléo", "3")]);
        let mut st = State::default();
        let ctx = ActCtx {
            now_ms: 10_000,
            age_secs: 5.0,
            fps: 50.0,
            data: &new,
        };
        assert!(on_data(&layer, &mut st, &old, &ctx));
        assert_eq!(st.prev, vec!["a", "b"]);
        assert_eq!(st.gone.len(), 1);
        // The same cards: nothing to play.
        let mut same = st.clone();
        let ctx2 = ActCtx { data: &old, ..ctx };
        assert!(!on_data(&layer, &mut same, &old, &ctx2));
        let mut m = half;
        // Halfway: "a" fading out at the top, "b" moving up, "c" coming in.
        let p = layout(c, &st, &new, 600.0, 1000.0, 10_150, &mut m);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].msg.id, "a");
        assert!(p[0].alpha < 1.0 && p[0].alpha > 0.0);
        let b = &p[1];
        assert!(b.y > 0.0 && b.y < p[0].card.h + c.gap);
        // Not yet in: it comes once the others have made room.
        assert!(p[2].alpha == 0.0 && p[2].x == SLIDE);
        let p = layout(c, &st, &new, 600.0, 1000.0, 10_450, &mut m);
        assert!(p[2].alpha > 0.0 && p[2].alpha < 1.0 && p[2].x > 0.0);
        // Done.
        let p = layout(c, &st, &new, 600.0, 1000.0, 11_000, &mut m);
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].y, p[0].alpha), (0.0, 1.0));
    }

    #[test]
    fn badges_have_their_platform_colours() {
        assert_eq!(platform("youtube"), ("#ff0033", Glyph::Play));
        assert_eq!(platform("Twitch").0, "#9146ff");
        assert_eq!(platform("facebook").0, "#1877f2");
        assert_eq!(platform("instagram").0, "#e1306c");
        assert_eq!(platform("x"), ("#000000", Glyph::X));
        assert_eq!(platform("tiktok"), ("#000000", Glyph::Note));
        assert_eq!(platform("whatsapp").0, "#25d366");
        assert_eq!(platform("sms").0, "#64748b");
        assert_eq!(platform("mastodon"), ("#64748b", Glyph::Letter));
        assert_eq!(initials("Jean-Marc Ollivier"), "JO");
        assert_eq!(initials("@claire.v"), "C");
        assert_eq!(avatar_color("Sophie L."), avatar_color(" Sophie L. "));
    }

    #[test]
    fn a_data_set_table_or_a_list_gives_the_messages() {
        let t = r#"[["name","text","platform"],["Ana","Bravo","x"]]"#;
        let m = parse(t);
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].name.as_str(), m[0].platform.as_str()), ("Ana", "x"));
        let o = r#"{"approved":[{"id":7,"handle":"@kev","text":"Salut"}]}"#;
        let m = parse(o);
        assert_eq!((m[0].id.as_str(), m[0].name.as_str()), ("7", "@kev"));
        assert!(parse("pas du JSON").is_empty());
    }

    #[test]
    fn a_table_maps_its_columns_by_name_and_numbers_its_rows() {
        // Columns in any case; no id column: the row's index is its id.
        let t = r#"[["Name","TEXT","platformName","Platform","city","time"],
            ["Ana","Bravo","YouTube","YouTube","Lyon","20:13"],
            ["","","","","",""],
            ["","Charlie","X","x","","20:14"]]"#;
        let m = parse(t);
        assert_eq!(m.len(), 2, "an empty row is no card");
        assert_eq!(m[0].id, "0");
        assert_eq!(m[0].name, "Ana");
        assert_eq!(m[0].text, "Bravo");
        assert_eq!(m[0].platform_name, "YouTube");
        assert_eq!(m[0].platform, "youtube");
        assert_eq!((m[0].city.as_str(), m[0].time.as_str()), ("Lyon", "20:13"));
        assert_eq!((m[1].id.as_str(), m[1].platform.as_str()), ("2", "x"));
        // An id column (any case) gives the ids; a handle stands in for a name.
        let t = r#"[["ID","handle","text"],["m7","@kev","Salut"]]"#;
        let m = parse(t);
        assert_eq!((m[0].id.as_str(), m[0].name.as_str()), ("m7", "@kev"));
        // The same through a layer's data.
        let mut d = Data::new();
        d.insert("wall__table".into(), t.to_string());
        let c = Config {
            items: "{{wall__table}}".into(),
            ..Default::default()
        };
        assert_eq!(messages(&c, &d).len(), 1);
    }
}

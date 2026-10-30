//! QR code: settings, payload, geometry (crisp integer modules) and the
//! logo budget. The matrix itself comes from ONE encoder, the `qrcode` crate
//! (`cg-render/src/wid/qrcode.rs`, served to the web by `GET /api/qr`), so
//! both renderers draw the same code; this module only knows the structure
//! of a QR symbol (function patterns, codeword placement, RS blocks) to say
//! which codewords a centre logo hides.
//! Drawn by `cg-render/src/wid/qrcode.rs` and `web/src/widgets/qrcode.tsx` (keep in sync).

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::Layer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// The layer's `data` as typed (old layers).
    #[default]
    Url,
    /// The layer's `data` too.
    Text,
    Vcard,
    Wifi,
    Email,
    Sms,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ecl {
    L,
    #[default]
    M,
    Q,
    H,
}

impl Ecl {
    pub fn ord(self) -> usize {
        match self {
            Ecl::L => 0,
            Ecl::M => 1,
            Ecl::Q => 2,
            Ecl::H => 3,
        }
    }
    /// Largest logo side, % of the code width.
    pub fn logo_max(self) -> u32 {
        [12, 18, 24, 30][self.ord()]
    }
    /// Format bits of the level.
    pub fn bits(self) -> u32 {
        [1, 0, 3, 2][self.ord()]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModStyle {
    #[default]
    Square,
    Rounded,
    Dots,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinderStyle {
    #[default]
    Square,
    Rounded,
    Circle,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Vcard {
    pub first: String,
    pub last: String,
    pub org: String,
    pub title: String,
    pub phone: String,
    pub email: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WifiSec {
    #[default]
    WPA,
    WEP,
    #[serde(rename = "nopass")]
    Nopass,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Wifi {
    pub ssid: String,
    pub password: String,
    pub security: WifiSec,
    pub hidden: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sms {
    pub number: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Logo {
    pub on: bool,
    /// Picture (asset path); empty = no logo (nothing drawn, no module hidden).
    pub src: String,
    /// Asked size, % of the code width (capped by the correction).
    pub size: u32,
}

impl Default for Logo {
    fn default() -> Self {
        Self {
            on: false,
            src: String::new(),
            size: 22,
        }
    }
}

/// Where the caption goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LabelPos {
    #[default]
    Below,
    Beside,
    Above,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Label {
    pub on: bool,
    pub text: String,
    pub size: f32,
    pub color: String,
    pub bg: String,
    pub pos: LabelPos,
    pub font: String,
    pub weight: u16,
    /// Capitals.
    pub upper: bool,
}

impl Default for Label {
    fn default() -> Self {
        Self {
            on: false,
            text: "Scan to vote".into(),
            size: 34.0,
            color: "#ffffff".into(),
            bg: "#7c5cff".into(),
            pos: LabelPos::Below,
            font: "Inter".into(),
            weight: 800,
            upper: false,
        }
    }
}

/// An address prepared in the Designer, chosen on air from a menu.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Address {
    pub name: String,
    /// `{{key}}` allowed.
    pub url: String,
    /// Its own caption (empty = the caption's text).
    pub caption: String,
}

/// Settings of the widget, flattened into the layer's JSON. The content of
/// URL / text is the layer's `data`, the modules use the `fill` colour and
/// the card the layer's `background`: an old layer (data + background +
/// fill) keeps its look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub kind: Kind,
    pub vcard: Vcard,
    pub wifi: Wifi,
    pub email: Email,
    pub sms: Sms,
    pub ecl: Ecl,
    pub transparent: bool,
    /// Transparent background: the colour assumed behind (readability check).
    pub backdrop: String,
    /// Card corners, px.
    pub radius: f32,
    pub mod_style: ModStyle,
    pub finder_style: FinderStyle,
    pub finder_same: bool,
    pub finder_color: String,
    pub logo: Logo,
    /// Quiet zone, modules.
    pub quiet: u32,
    /// Smallest module on screen before the check fails, px.
    pub min_px: u32,
    pub label: Label,
    /// URL type: addresses prepared for the Operator (none = the layer's `data`).
    pub addresses: Vec<Address>,
    /// The Operator gets the list of the prepared addresses (the control
    /// centre's row, when the Designer exposes it; the arrows stay).
    pub op_address: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            kind: Kind::Url,
            vcard: Vcard::default(),
            wifi: Wifi::default(),
            email: Email::default(),
            sms: Sms::default(),
            ecl: Ecl::M,
            transparent: false,
            backdrop: "#12151e".into(),
            radius: 0.0,
            mod_style: ModStyle::Square,
            finder_style: FinderStyle::Square,
            finder_same: true,
            finder_color: "#7c5cff".into(),
            logo: Logo::default(),
            quiet: 4,
            min_px: 6,
            label: Label::default(),
            addresses: Vec::new(),
            op_address: true,
        }
    }
}

/// What the operator chose on air.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// Prepared address on air (None = the first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addr: Option<usize>,
}

/// Actions: `address {i}` or `address {name}`: the prepared address on air.
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    _ctx: &ActCtx,
) -> Result<(), String> {
    let crate::LayerKind::QrCode { x: c, .. } = &layer.kind else {
        return Err("not a QR code".into());
    };
    match op {
        "address" => {
            if c.addresses.is_empty() {
                return Err("No address prepared".into());
            }
            let i = match (args.get("i").and_then(|v| v.as_u64()), args.get("name")) {
                (Some(i), _) => i as usize,
                (None, Some(serde_json::Value::String(n))) => c
                    .addresses
                    .iter()
                    .position(|a| a.name.trim().eq_ignore_ascii_case(n.trim()))
                    .ok_or_else(|| format!("No address named “{n}”"))?,
                _ => return Err("address needs {\"i\": n} or {\"name\": …}".into()),
            };
            if i >= c.addresses.len() {
                return Err(format!("No address {i}"));
            }
            st.addr = Some(i);
            Ok(())
        }
        _ => Err(format!("unknown action `{op}`")),
    }
}

/// The URL on air and its own caption: the prepared address chosen (URL
/// type with addresses), else the layer's `data` and the caption's text.
pub fn address<'a>(c: &'a Config, data_field: &'a str, st: &State) -> (&'a str, &'a str) {
    if c.kind == Kind::Url && !c.addresses.is_empty() {
        let a = &c.addresses[st.addr.unwrap_or(0).min(c.addresses.len() - 1)];
        let cap = if a.caption.trim().is_empty() {
            c.label.text.as_str()
        } else {
            a.caption.as_str()
        };
        return (&a.url, cap);
    }
    (data_field, &c.label.text)
}

// ---------------------------------------------------------------- payload --

/// JavaScript's encodeURIComponent.
pub fn uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out += &format!("%{b:02X}");
        }
    }
    out
}

fn wifi_esc(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

/// The text encoded: `data` (URL / text) or built from the form.
pub fn payload(c: &Config, data_field: &str, data: &Data) -> String {
    let i = |s: &str| interpolate_text(s, data);
    match c.kind {
        Kind::Url | Kind::Text => i(data_field),
        Kind::Vcard => {
            let v = &c.vcard;
            let (first, last) = (i(&v.first), i(&v.last));
            let full: Vec<&str> = [first.as_str(), last.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect();
            let mut l = vec![
                "BEGIN:VCARD".to_string(),
                "VERSION:3.0".into(),
                format!("N:{last};{first}"),
                format!("FN:{}", full.join(" ")),
            ];
            for (k, val) in [
                ("ORG", &v.org),
                ("TITLE", &v.title),
                ("TEL;TYPE=CELL", &v.phone),
                ("EMAIL", &v.email),
                ("URL", &v.url),
            ] {
                let val = i(val);
                if !val.is_empty() {
                    l.push(format!("{k}:{val}"));
                }
            }
            l.push("END:VCARD".into());
            l.join("\n")
        }
        Kind::Wifi => {
            let w = &c.wifi;
            let sec = match w.security {
                WifiSec::WPA => "WPA",
                WifiSec::WEP => "WEP",
                WifiSec::Nopass => "nopass",
            };
            let pass = if w.security == WifiSec::Nopass {
                String::new()
            } else {
                format!("P:{};", wifi_esc(&i(&w.password)))
            };
            format!(
                "WIFI:T:{sec};S:{};{pass}{};",
                wifi_esc(&i(&w.ssid)),
                if w.hidden { "H:true;" } else { "" }
            )
        }
        Kind::Email => {
            let m = &c.email;
            let (sub, body) = (i(&m.subject), i(&m.body));
            let mut q = Vec::new();
            if !sub.is_empty() {
                q.push(format!("subject={}", uri_component(&sub)));
            }
            if !body.is_empty() {
                q.push(format!("body={}", uri_component(&body)));
            }
            format!(
                "mailto:{}{}",
                i(&m.to),
                if q.is_empty() {
                    String::new()
                } else {
                    format!("?{}", q.join("&"))
                }
            )
        }
        Kind::Sms => format!("SMSTO:{}:{}", i(&c.sms.number), i(&c.sms.message)),
    }
}

// ----------------------------------------------------------------- layout --

/// Geometry in the box: integer module size `m`, code `px` × `px` at
/// (x0, y0), and the caption's pill (none when `lab` is 0) with its lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Geo {
    pub m: f32,
    pub px: f32,
    pub x0: f32,
    pub y0: f32,
    /// Caption pill: height (0 = none), left, top, width, corner radius.
    pub lab: f32,
    pub lab_x: f32,
    pub lab_y: f32,
    pub lab_w: f32,
    pub lab_r: f32,
    /// Its lines (one, or two beside the code).
    pub lines: Vec<String>,
}

pub fn round_js(v: f32) -> f32 {
    (v + 0.5).floor()
}

/// Approximate width of a caption (layout only, the same in both renderers).
pub fn cap_w(s: &str, size: f32, upper: bool) -> f32 {
    s.chars().count() as f32 * size * if upper { 0.68 } else { 0.6 }
}

/// The caption as drawn (capitals or as typed).
pub fn cap_text(c: &Config, s: &str) -> String {
    if c.label.upper {
        s.to_uppercase()
    } else {
        s.to_string()
    }
}

/// One line, or two (split between words so that the longer is shortest).
fn cap_lines(s: &str, max: f32, size: f32, upper: bool) -> Vec<String> {
    let words: Vec<&str> = s.split(' ').filter(|w| !w.is_empty()).collect();
    if cap_w(s, size, upper) <= max || words.len() < 2 {
        return vec![s.to_string()];
    }
    let mut best = (f32::INFINITY, 1);
    for i in 1..words.len() {
        let m = cap_w(&words[..i].join(" "), size, upper).max(cap_w(
            &words[i..].join(" "),
            size,
            upper,
        ));
        if m < best.0 {
            best = (m, i);
        }
    }
    vec![words[..best.1].join(" "), words[best.1..].join(" ")]
}

/// `label_text`: the caption as drawn (see [`cap_text`]).
pub fn geo(c: &Config, label_text: &str, n: usize, w: f32, h: f32) -> Geo {
    let on = c.label.on && !label_text.is_empty();
    let size = c.label.size;
    let up = c.label.upper;
    let total = (n as u32 + 2 * c.quiet) as f32;
    if on && c.label.pos == LabelPos::Beside {
        let max_w = round_js(w * 0.4);
        let gap = 18.0;
        let avail = (w - max_w - gap).min(h);
        let m = (avail / total).floor().max(1.0);
        let px = m * total;
        let lines = cap_lines(label_text, max_w - size * 1.6, size, up);
        let widest = lines.iter().map(|l| cap_w(l, size, up)).fold(0.0, f32::max);
        let lab_w = max_w.min(widest + size * 1.6).round();
        let lab = round_js(lines.len() as f32 * size * 1.25 + size * 0.75);
        let x0 = round_js((w - px - gap - lab_w) / 2.0).max(0.0);
        let y0 = round_js((h - px) / 2.0).max(0.0);
        return Geo {
            m,
            px,
            x0,
            y0,
            lab,
            lab_x: x0 + px + gap,
            lab_y: round_js(y0 + px / 2.0 - lab / 2.0),
            lab_w,
            lab_r: (lab / 2.0).min(size * 0.7),
            lines,
        };
    }
    let lab = if on { round_js(size * 1.9) } else { 0.0 };
    let gap = if lab > 0.0 { 18.0 } else { 0.0 };
    let avail = w.min(h - lab - gap);
    let m = (avail / total).floor().max(1.0);
    let px = m * total;
    let x0 = round_js((w - px) / 2.0);
    let top = round_js((h - lab - gap - px) / 2.0).max(0.0);
    let above = c.label.pos == LabelPos::Above;
    let (y0, lab_y) = if above {
        (top + lab + gap, top)
    } else {
        (top, top + px + gap)
    };
    let lab_w = w
        .min((px * 0.7).max(cap_w(label_text, size, up) + 60.0))
        .round();
    Geo {
        m,
        px,
        x0,
        y0,
        lab,
        lab_x: ((w - lab_w) / 2.0 + 0.5).floor(),
        lab_y,
        lab_w,
        lab_r: lab / 2.0,
        lines: if on {
            vec![label_text.to_string()]
        } else {
            Vec::new()
        },
    }
}

// -------------------------------------------------- structure of a symbol --

const ECC_PER_BLOCK: [[i32; 41]; 4] = [
    [
        -1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28,
        30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        -1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28,
        28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    ],
    [
        -1, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30,
        30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        -1, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24,
        30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
];
const NUM_BLOCKS: [[i32; 41]; 4] = [
    [
        -1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12,
        13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25,
    ],
    [
        -1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21,
        23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
    ],
    [
        -1, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27,
        29, 34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68,
    ],
    [
        -1, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32,
        35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81,
    ],
];

pub fn raw_modules(v: usize) -> usize {
    let mut r = (16 * v + 128) * v + 64;
    if v >= 2 {
        let na = v / 7 + 2;
        r -= (25 * na - 10) * na - 55;
        if v >= 7 {
            r -= 36;
        }
    }
    r
}

pub fn align_pos(v: usize) -> Vec<usize> {
    if v == 1 {
        return Vec::new();
    }
    let n = v / 7 + 2;
    let step = if v == 32 {
        26
    } else {
        (v * 4 + 4).div_ceil(n * 2 - 2) * 2
    };
    let mut r = vec![6];
    let mut p = v * 4 + 17 - 7;
    while r.len() < n {
        r.insert(1, p);
        p -= step;
    }
    r
}

/// Function modules, codeword of each data module and RS blocks of a version.
#[derive(Debug, Clone)]
pub struct Structure {
    pub size: usize,
    pub version: usize,
    /// True = finder, timing, alignment, format or version module.
    pub func: Vec<bool>,
    /// Codeword (interleaved order) of each module, -1 = none.
    pub cw: Vec<i32>,
    /// Block of each interleaved codeword.
    pub block_of: Vec<usize>,
    pub blocks: usize,
    pub ecc_len: usize,
}

pub fn structure(size: usize, ecl: Ecl) -> Option<Structure> {
    if size < 21 || !(size - 17).is_multiple_of(4) {
        return None;
    }
    let v = (size - 17) / 4;
    if v > 40 {
        return None;
    }
    let e = ecl.ord();
    let mut func = vec![false; size * size];
    let mut set = |x: usize, y: usize| func[y * size + x] = true;
    for i in 0..size {
        set(6, i);
        set(i, 6);
    }
    for (cx, cy) in [(3i32, 3i32), (size as i32 - 4, 3), (3, size as i32 - 4)] {
        for dy in -4..=4 {
            for dx in -4..=4 {
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0 && y >= 0 && (x as usize) < size && (y as usize) < size {
                    set(x as usize, y as usize);
                }
            }
        }
    }
    let al = align_pos(v);
    let k = al.len();
    for (i, &ay) in al.iter().enumerate() {
        for (j, &ax) in al.iter().enumerate() {
            if (i == 0 && j == 0) || (i == 0 && j == k - 1) || (i == k - 1 && j == 0) {
                continue;
            }
            for dy in 0..5 {
                for dx in 0..5 {
                    set(ax + dx - 2, ay + dy - 2);
                }
            }
        }
    }
    // Format areas.
    for i in 0..9 {
        set(8, i);
        set(i, 8);
    }
    for i in 0..8 {
        set(size - 1 - i, 8);
        set(8, size - 1 - i);
    }
    if v >= 7 {
        for i in 0..18 {
            let (a, b) = (size - 11 + i % 3, i / 3);
            set(a, b);
            set(b, a);
        }
    }
    let raw = raw_modules(v) / 8;
    let nb = NUM_BLOCKS[e][v] as usize;
    let ecc_len = ECC_PER_BLOCK[e][v] as usize;
    let n_short = nb - raw % nb;
    let short_len = raw / nb;
    // Interleaving: codeword k of the stream comes from block `block_of[k]`.
    let mut block_of = Vec::with_capacity(raw);
    for i in 0..=short_len {
        for j in 0..nb {
            // Short blocks have no codeword at index short_len - ecc_len.
            if i != short_len - ecc_len || j >= n_short {
                block_of.push(j);
            }
        }
    }
    let mut cw = vec![-1; size * size];
    let mut bit = 0usize;
    let mut right = size as i32 - 1;
    while right >= 1 {
        if right == 6 {
            right = 5;
        }
        for vert in 0..size {
            for j in 0..2 {
                let x = (right - j) as usize;
                let up = ((right + 1) & 2) == 0;
                let y = if up { size - 1 - vert } else { vert };
                if !func[y * size + x] && bit < raw * 8 {
                    cw[y * size + x] = (bit >> 3) as i32;
                    bit += 1;
                }
            }
        }
        right -= 2;
    }
    Some(Structure {
        size,
        version: v,
        func,
        cw,
        block_of,
        blocks: nb,
        ecc_len,
    })
}

/// Modules hidden by a centred logo of `pct` % (plus a one-module plate):
/// (x, y, n) in modules, and the logo side in modules.
pub fn logo_rect(size: usize, pct: u32) -> Option<(usize, usize, usize, usize)> {
    if pct == 0 {
        return None;
    }
    let side = (size as f32 * pct as f32 / 100.0).ceil() as usize;
    let plate = (side + 2).min(size);
    let a = (size - plate) / 2;
    Some((a, a, plate, side))
}

/// Damage of a logo: (a function module is hit, worst block, codewords a block repairs, modules, codewords).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Damage {
    pub fn_hit: bool,
    pub worst: usize,
    pub cap: usize,
    pub modules: usize,
    pub cws: usize,
}

pub fn damage(s: &Structure, pct: u32) -> Damage {
    let mut d = Damage {
        cap: s.ecc_len / 2,
        ..Default::default()
    };
    let Some((x0, y0, n, _)) = logo_rect(s.size, pct) else {
        return d;
    };
    let mut hit = std::collections::BTreeSet::new();
    for y in y0..(y0 + n).min(s.size) {
        for x in x0..(x0 + n).min(s.size) {
            d.modules += 1;
            let k = y * s.size + x;
            if s.func[k] {
                d.fn_hit = true;
            } else if s.cw[k] >= 0 {
                hit.insert(s.cw[k] as usize);
            }
        }
    }
    let mut per = vec![0usize; s.blocks];
    for &i in &hit {
        if let Some(b) = s.block_of.get(i) {
            per[*b] += 1;
        }
    }
    d.worst = per.into_iter().max().unwrap_or(0);
    d.cws = hit.len();
    d
}

/// The logo size drawn (%): asked, capped by the level and by what every
/// block can repair (85 % of it); and why it was capped ("level", "budget").
pub fn logo_size(c: &Config, s: Option<&Structure>) -> (u32, &'static str) {
    let Some(s) = s else {
        return (0, "");
    };
    // No picture: no logo (never a placeholder on air).
    if !c.logo.on || c.logo.src.trim().is_empty() {
        return (0, "");
    }
    let mut pct = c.logo.size.min(c.ecl.logo_max());
    let mut by = if c.logo.size > c.ecl.logo_max() {
        "level"
    } else {
        ""
    };
    while pct > 0 {
        let d = damage(s, pct);
        if !d.fn_hit && d.worst <= d.cap * 85 / 100 {
            break;
        }
        pct -= 1;
        by = "budget";
    }
    (pct, by)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_layers_load() {
        let l: Layer = serde_json::from_str(
            r##"{"id":"a","name":"QR","type":"qrCode","data":"https://x.y","background":"#ffffff","props":{}}"##,
        )
        .unwrap();
        let crate::LayerKind::QrCode { x, data, .. } = l.kind else {
            panic!()
        };
        assert_eq!(x, Config::default());
        assert_eq!(payload(&x, &data, &Data::new()), "https://x.y");
    }

    #[test]
    fn payloads() {
        let mut c = Config {
            kind: Kind::Wifi,
            ..Default::default()
        };
        c.wifi.ssid = "My;Net".into();
        c.wifi.password = "p:w".into();
        assert_eq!(
            payload(&c, "", &Data::new()),
            "WIFI:T:WPA;S:My\\;Net;P:p\\:w;;"
        );
        c.kind = Kind::Email;
        c.email.to = "a@b.c".into();
        c.email.subject = "Hi there".into();
        assert_eq!(
            payload(&c, "", &Data::new()),
            "mailto:a@b.c?subject=Hi%20there"
        );
        c.kind = Kind::Sms;
        c.sms.number = "+33".into();
        c.sms.message = "VOTE 7".into();
        assert_eq!(payload(&c, "", &Data::new()), "SMSTO:+33:VOTE 7");
    }

    #[test]
    fn crisp_geometry() {
        let c = Config {
            quiet: 4,
            ..Default::default()
        };
        let g = geo(&c, "", 25, 517.0, 633.0);
        assert_eq!(g.m, (517.0f32 / 33.0).floor());
        assert_eq!(g.x0.fract(), 0.0);
        assert_eq!(g.y0.fract(), 0.0);
    }

    #[test]
    fn caption_places_and_addresses() {
        let mut c = Config::default();
        c.label.on = true;
        let below = geo(&c, "Scannez pour voter", 25, 520.0, 640.0);
        assert!(below.lab_y > below.y0 + below.px);
        c.label.pos = LabelPos::Above;
        let above = geo(&c, "Scannez pour voter", 25, 520.0, 640.0);
        assert!(above.lab_y + above.lab <= above.y0);
        c.label.pos = LabelPos::Beside;
        let beside = geo(&c, "Scannez pour voter", 25, 520.0, 520.0);
        assert_eq!(beside.lines, vec!["Scannez", "pour voter"]);
        assert!(beside.lab_x >= beside.x0 + beside.px);
        assert!(beside.lab_x + beside.lab_w <= 520.0);
        c.addresses = vec![
            Address {
                name: "Vote".into(),
                url: "https://example.com/vote".into(),
                caption: String::new(),
            },
            Address {
                name: "Jeu".into(),
                url: "https://example.com/jeu".into(),
                caption: "Scannez pour jouer".into(),
            },
        ];
        let mut st = State::default();
        assert_eq!(address(&c, "x", &st).0, "https://example.com/vote");
        let mut l: Layer = serde_json::from_str(
            r##"{"id":"q","name":"QR","type":"qrCode","data":"x","background":"#ffffff","props":{}}"##,
        )
        .unwrap();
        if let crate::LayerKind::QrCode { x, .. } = &mut l.kind {
            *x = c.clone();
        }
        let d = Data::new();
        let ctx = ActCtx {
            now_ms: 0,
            age_secs: 0.0,
            data: &d,
            fps: 50.0,
        };
        act(
            &l,
            &mut st,
            "address",
            &serde_json::json!({"name": "jeu"}),
            &ctx,
        )
        .unwrap();
        assert_eq!(
            address(&c, "x", &st),
            ("https://example.com/jeu", "Scannez pour jouer")
        );
        assert!(act(&l, &mut st, "address", &serde_json::json!({"i": 5}), &ctx).is_err());
    }

    #[test]
    fn structure_counts() {
        for v in [1usize, 2, 7, 10, 40] {
            for e in [Ecl::L, Ecl::M, Ecl::Q, Ecl::H] {
                let s = structure(17 + 4 * v, e).unwrap();
                let data_modules = s.func.iter().filter(|f| !**f).count();
                assert_eq!(data_modules, raw_modules(v), "v{v}");
                assert_eq!(s.block_of.len(), raw_modules(v) / 8);
            }
        }
    }

    #[test]
    fn logo_is_capped() {
        let s = structure(29, Ecl::M).unwrap();
        let mut c = Config {
            logo: Logo {
                on: true,
                size: 40,
                src: "/assets/logo.png".into(),
            },
            ..Default::default()
        };
        let (pct, by) = logo_size(&c, Some(&s));
        assert!(pct <= 18 && !by.is_empty());
        // No picture: nothing hidden.
        c.logo.src.clear();
        assert_eq!(logo_size(&c, Some(&s)).0, 0);
        let s = structure(33, Ecl::H).unwrap();
        let c = Config {
            ecl: Ecl::H,
            logo: Logo {
                on: true,
                size: 22,
                src: "/assets/logo.png".into(),
            },
            ..Default::default()
        };
        let (pct, _) = logo_size(&c, Some(&s));
        assert!(pct > 10);
    }
}

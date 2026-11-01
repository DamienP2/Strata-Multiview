//! Source: one tile of a multiview. The picture of a live input, framed by
//! its tally (red on air, green next, yellow aux), with its UMD (the name
//! bar, its tally lamps at each end) and the meters of its audio channels.
//! The lamps, the UMD text and the levels come with the input
//! (`crate::signal`). Settings and geometry here; drawn by
//! `strata-mv-render/src/wid/source.rs` and `web/src/widgets/source.tsx`
//! (keep in sync).

use serde::{Deserialize, Serialize};

use crate::signal::{Level, SourceView};

/// How the picture fills its area.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    /// All of it, black bars around.
    #[default]
    Contain,
    /// All the area, the picture cropped.
    Cover,
    /// All the area, the picture distorted.
    Stretch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UmdPosition {
    #[default]
    Bottom,
    Top,
    Off,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UmdAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VuPosition {
    #[default]
    Right,
    Left,
    Off,
}

/// What the tile shows while its input has no picture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoSignalLook {
    /// SMPTE colour bars, the F AudioVisuel card bouncing over them.
    #[default]
    Bars,
    /// The background and the words only.
    Plain,
}

/// The timecode shown on the tile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TcShow {
    #[default]
    Off,
    /// The one the input carries.
    Input,
    /// The station's (the server's time of day).
    Station,
}

/// The colours of the tally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TallyLook {
    /// The frame takes the tally colour.
    pub border: bool,
    pub program: String,
    pub preview: String,
    pub aux: String,
    /// The frame with no tally ("#00000000": none).
    pub off: String,
}

impl Default for TallyLook {
    fn default() -> Self {
        Self {
            border: true,
            program: "#e5212b".into(),
            preview: "#17b14a".into(),
            aux: "#f2b705".into(),
            off: "#262a33".into(),
        }
    }
}

/// The name bar (UMD).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Umd {
    pub position: UmdPosition,
    /// Height, % of the tile's.
    pub size: f32,
    /// Beside the picture (the picture is smaller) instead of over it.
    pub outside: bool,
    /// The text ("" = the switcher's UMD, else the input's name); `{{key}}`
    /// are replaced.
    pub text: String,
    pub font: String,
    pub weight: u16,
    pub align: UmdAlign,
    pub color: String,
    pub background: String,
    /// Tally lamps at each end: on air on the left, next on the right.
    pub lamps: bool,
}

impl Default for Umd {
    fn default() -> Self {
        Self {
            position: UmdPosition::Bottom,
            size: 11.0,
            outside: false,
            text: String::new(),
            font: "Inter".into(),
            weight: 600,
            align: UmdAlign::Center,
            color: "#ffffff".into(),
            background: "#000000b3".into(),
            lamps: true,
        }
    }
}

/// The audio meters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Vu {
    pub position: VuPosition,
    /// Width of a channel's bar, px.
    pub width: f32,
    pub gap: f32,
    /// From the picture's edges, px.
    pub margin: f32,
    /// The bottom of the scale, dBFS.
    pub min: f32,
    /// Yellow from this level, red from that one, dBFS.
    pub yellow: f32,
    pub red: f32,
    /// The highest peak of the last seconds, as a mark.
    pub hold: bool,
    /// At most this many channels (0: all of them).
    pub max: u32,
    /// The first channel shown (1: from the first; 3: channels 3, 4…).
    pub first: u32,
}

impl Default for Vu {
    fn default() -> Self {
        Self {
            position: VuPosition::Right,
            width: 6.0,
            gap: 2.0,
            margin: 6.0,
            min: -60.0,
            yellow: -18.0,
            red: -9.0,
            hold: true,
            max: 0,
            first: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// The live input shown ("" = none).
    pub input: String,
    pub fit: Fit,
    /// Behind the picture (black bars, no signal).
    pub background: String,
    /// The frame's width, px.
    pub border: f32,
    pub tally: TallyLook,
    pub umd: Umd,
    pub vu: Vu,
    /// The format of the signal ("1080i50") in the top left corner.
    pub badge: bool,
    /// Shown without a picture ("" = « No signal » in the project's language).
    pub no_signal: String,
    pub no_signal_look: NoSignalLook,
    /// The timecode, top right.
    pub timecode: TcShow,
    /// The loudness (short-term), bottom right.
    pub loudness: bool,
    /// The faults the server sees: a banner, and the frame in their colour.
    pub alarms: bool,
    /// Lines over the picture: safe areas, other aspect ratios, centre, AFD.
    pub markers: Markers,
}

/// Lines drawn over the picture (EBU R95 safe areas, the 4:3 and 14:9
/// frames inside a 16:9 picture, the centre, the active area its AFD gives).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Markers {
    /// Action safe: 93 % of the picture.
    pub action: bool,
    /// Graphics (title) safe: 90 %.
    pub title: bool,
    pub four_three: bool,
    pub fourteen_nine: bool,
    pub centre: bool,
    /// The active area of the picture's Active Format Description, and its code.
    pub afd: bool,
    pub color: String,
}

impl Default for Markers {
    fn default() -> Self {
        Self {
            action: false,
            title: false,
            four_three: false,
            fourteen_nine: false,
            centre: false,
            afd: false,
            color: "#ffffffb3".into(),
        }
    }
}

impl Markers {
    pub fn any(&self) -> bool {
        self.action
            || self.title
            || self.four_three
            || self.fourteen_nine
            || self.centre
            || self.afd
    }
}

/// Where the picture is drawn in `v` (`size`: the picture's; none: 16:9).
pub fn picture_area(fit: Fit, v: Area, size: Option<(f32, f32)>) -> Area {
    let (iw, ih) = size
        .filter(|s| s.0 > 0.0 && s.1 > 0.0)
        .unwrap_or((16.0, 9.0));
    let (sx, sy) = match fit {
        Fit::Stretch => return v,
        Fit::Contain => {
            let k = (v[2] / iw).min(v[3] / ih);
            (k, k)
        }
        Fit::Cover => {
            let k = (v[2] / iw).max(v[3] / ih);
            (k, k)
        }
    };
    let (pw, ph) = ((iw * sx).min(v[2]), (ih * sy).min(v[3]));
    [v[0] + (v[2] - pw) / 2.0, v[1] + (v[3] - ph) / 2.0, pw, ph]
}

/// The active area an AFD code gives in picture `p` (none: all of it, or
/// an unknown code).
pub fn afd_area(code: u8, p: Area) -> Option<Area> {
    let wide = p[2] / p[3].max(1.0) > 1.5;
    let pillar = |aspect: f32| {
        let w = (p[3] * aspect).min(p[2]);
        [p[0] + (p[2] - w) / 2.0, p[1], w, p[3]]
    };
    let letter = |aspect: f32, top: bool| {
        let h = (p[2] / aspect).min(p[3]);
        [
            p[0],
            if top { p[1] } else { p[1] + (p[3] - h) / 2.0 },
            p[2],
            h,
        ]
    };
    match (wide, code) {
        // A 16:9 coded frame: 4:3 or 14:9 pillar boxes.
        (true, 9 | 13 | 15) => Some(pillar(4.0 / 3.0)),
        (true, 11 | 14) => Some(pillar(14.0 / 9.0)),
        // A 4:3 coded frame: letter boxes.
        (false, 2) => Some(letter(16.0 / 9.0, true)),
        (false, 3) => Some(letter(14.0 / 9.0, true)),
        (false, 10 | 15) => Some(letter(16.0 / 9.0, false)),
        (false, 11 | 14) => Some(letter(14.0 / 9.0, false)),
        _ => None,
    }
}

/// What an AFD code means, in short ("AFD 9 · 4:3").
pub fn afd_label(code: u8) -> String {
    let what = match code {
        2 => "16:9 haut",
        3 => "14:9 haut",
        4 => "> 16:9",
        8 => "plein",
        9 => "4:3",
        10 => "16:9",
        11 => "14:9",
        13 => "4:3 (14:9)",
        14 => "16:9 (14:9)",
        15 => "16:9 (4:3)",
        _ => "?",
    };
    format!("AFD {code} · {what}")
}

/// The lines of the markers over picture `p`, as thin boxes (the AFD's area
/// apart, drawn in its own colour), and where the AFD's label goes.
pub struct MarkerLines {
    pub lines: Vec<Area>,
    pub afd: Vec<Area>,
    pub label: Option<(Area, String)>,
}

fn outline(a: Area, t: f32, out: &mut Vec<Area>) {
    out.push([a[0], a[1], a[2], t]);
    out.push([a[0], a[1] + a[3] - t, a[2], t]);
    out.push([a[0], a[1], t, a[3]]);
    out.push([a[0] + a[2] - t, a[1], t, a[3]]);
}

pub fn marker_lines(m: &Markers, p: Area, afd: Option<u8>, size: f32) -> MarkerLines {
    let mut lines = Vec::new();
    let mut afd_lines = Vec::new();
    let mut label = None;
    if p[2] <= 4.0 || p[3] <= 4.0 {
        return MarkerLines {
            lines,
            afd: afd_lines,
            label,
        };
    }
    let t = (p[3] / 270.0).round().max(1.0);
    let inset = |k: f32| {
        [
            p[0] + p[2] * k,
            p[1] + p[3] * k,
            p[2] * (1.0 - 2.0 * k),
            p[3] * (1.0 - 2.0 * k),
        ]
    };
    if m.action {
        outline(inset(0.035), t, &mut lines);
    }
    if m.title {
        outline(inset(0.05), t, &mut lines);
    }
    for (on, aspect) in [(m.four_three, 4.0 / 3.0), (m.fourteen_nine, 14.0 / 9.0)] {
        let w = p[3] * aspect;
        if on && w < p[2] - 2.0 * t {
            let x = p[0] + (p[2] - w) / 2.0;
            lines.push([x, p[1], t, p[3]]);
            lines.push([x + w - t, p[1], t, p[3]]);
        }
    }
    if m.centre {
        let l = (p[3] * 0.05).max(4.0);
        let (cx, cy) = (p[0] + p[2] / 2.0, p[1] + p[3] / 2.0);
        lines.push([cx - l, cy - t / 2.0, 2.0 * l, t]);
        lines.push([cx - t / 2.0, cy - l, t, 2.0 * l]);
    }
    if let (true, Some(code)) = (m.afd, afd) {
        if let Some(a) = afd_area(code, p) {
            outline(a, t, &mut afd_lines);
        }
        let w = (size * 7.0).min(p[2] - 8.0).max(1.0);
        let pad = (size * 0.4).round();
        label = Some((
            [p[0] + (p[2] - w) / 2.0, p[1] + pad, w, size * 1.4],
            afd_label(code),
        ));
    }
    MarkerLines {
        lines,
        afd: afd_lines,
        label,
    }
}

/// The AFD's lines: cyan.
pub const AFD_COLOR: &str = "#22d3ee";

impl Default for Config {
    fn default() -> Self {
        Self {
            input: String::new(),
            fit: Fit::Contain,
            background: "#0b0d12".into(),
            border: 4.0,
            tally: TallyLook::default(),
            umd: Umd::default(),
            vu: Vu::default(),
            badge: false,
            no_signal: String::new(),
            no_signal_look: NoSignalLook::Bars,
            timecode: TcShow::Off,
            loudness: false,
            alarms: true,
            markers: Markers::default(),
        }
    }
}

/// A box: x, y, width, height (layer pixels).
pub type Area = [f32; 4];

/// Where everything of a tile goes, in its box.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Inside the frame.
    pub inner: Area,
    /// The picture's area (all of `inner`, or what the UMD leaves).
    pub video: Area,
    pub umd: Option<Area>,
    /// The lamps' squares in the UMD (on air, next).
    pub lamps: Option<(Area, Area)>,
    /// The UMD text's area (between the lamps).
    pub text: Option<Area>,
    /// One bar per channel.
    pub meters: Vec<Area>,
}

/// The layout of a `w` × `h` tile showing `channels` meters.
pub fn layout(c: &Config, w: f32, h: f32, channels: usize) -> Layout {
    let b = c.border.clamp(0.0, (w.min(h) / 4.0).max(0.0));
    let inner = [b, b, (w - 2.0 * b).max(0.0), (h - 2.0 * b).max(0.0)];
    let mut video = inner;
    let (mut umd, mut lamps, mut text) = (None, None, None);
    // What the UMD covers of the picture, top and bottom.
    let (mut top, mut bottom) = (0.0, 0.0);
    if c.umd.position != UmdPosition::Off && inner[2] > 0.0 && inner[3] > 0.0 {
        let bh = (c.umd.size.clamp(0.0, 50.0) / 100.0 * h)
            .max(8.0)
            .min(inner[3] / 2.0);
        let y = match c.umd.position {
            UmdPosition::Top => inner[1],
            _ => inner[1] + inner[3] - bh,
        };
        let bar = [inner[0], y, inner[2], bh];
        umd = Some(bar);
        if c.umd.outside {
            video[3] -= bh;
            if c.umd.position == UmdPosition::Top {
                video[1] += bh;
            }
        } else if c.umd.position == UmdPosition::Top {
            top = bh;
        } else {
            bottom = bh;
        }
        let pad = (bh * 0.2).round();
        let mut tx = [bar[0] + pad, bar[1], (bar[2] - 2.0 * pad).max(0.0), bh];
        if c.umd.lamps {
            let s = (bh * 0.56).round().max(4.0);
            let ly = bar[1] + (bh - s) / 2.0;
            lamps = Some((
                [bar[0] + pad, ly, s, s],
                [bar[0] + bar[2] - pad - s, ly, s, s],
            ));
            tx[0] += s + pad;
            tx[2] = (tx[2] - 2.0 * (s + pad)).max(0.0);
        }
        text = Some(tx);
    }
    let mut meters = Vec::new();
    if c.vu.position != VuPosition::Off && channels > 0 {
        let n = match c.vu.max {
            0 => channels,
            m => channels.min(m as usize),
        };
        let (bw, gap, m) = (c.vu.width.max(1.0), c.vu.gap.max(0.0), c.vu.margin.max(0.0));
        let total = n as f32 * bw + (n.saturating_sub(1)) as f32 * gap;
        let y0 = video[1] + top + m;
        let y1 = video[1] + video[3] - bottom - m;
        if y1 > y0 && total + 2.0 * m <= video[2] {
            let x0 = match c.vu.position {
                VuPosition::Left => video[0] + m,
                _ => video[0] + video[2] - m - total,
            };
            for i in 0..n {
                meters.push([x0 + i as f32 * (bw + gap), y0, bw, y1 - y0]);
            }
        }
    }
    Layout {
        inner,
        video,
        umd,
        lamps,
        text,
        meters,
    }
}

/// Where the overlays go: the timecode, the loudness, the fault's banner,
/// and the size of their text.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub tc: Area,
    pub lufs: Area,
    pub banner: Area,
    pub size: f32,
}

/// The overlays of a tile laid out as `l` (`h`: the tile's height).
pub fn overlay(c: &Config, l: &Layout, h: f32) -> Overlay {
    let size = (h * 0.05).clamp(9.0, 30.0).round();
    let m = c.vu.margin.max(4.0);
    let v = l.video;
    // Left of the meters when they are on the right.
    let right = match (c.vu.position, l.meters.first()) {
        (VuPosition::Right, Some(a)) => a[0] - m,
        _ => v[0] + v[2] - m,
    };
    let (mut top, mut bottom) = (v[1] + m, v[1] + v[3] - m);
    if let (Some(u), false) = (l.umd, c.umd.outside) {
        if c.umd.position == UmdPosition::Top {
            top = u[1] + u[3] + m;
        } else {
            bottom = u[1] - m;
        }
    }
    // JetBrains Mono: 0.6 em a character.
    let (tw, lw) = (
        (11.0 * 0.6 * size + size).round(),
        (5.0 * 0.6 * size + size).round(),
    );
    let bh = (size * 1.5).round();
    let banner_h = (size * 2.0).round();
    let banner_w = (v[2] * 0.7).round();
    Overlay {
        tc: [right - tw, top, tw, bh],
        lufs: [right - lw, bottom - bh, lw, bh],
        banner: [
            v[0] + (v[2] - banner_w) / 2.0,
            v[1] + (v[3] - banner_h) / 2.0,
            banner_w,
            banner_h,
        ],
        size,
    }
}

/// The frame's colour with the faults: a critical one blinks (half a
/// second on, half a second as the tally says), a warning stays.
pub fn alarm_border<'a>(
    c: &'a Config,
    t: &crate::signal::Tally,
    alarms: &[crate::signal::Alarm],
    now_ms: i64,
) -> &'a str {
    use crate::signal::Severity;
    if c.alarms {
        // Critical: flashing until acknowledged, then steady.
        let unseen = alarms.iter().any(|a| !a.acked);
        match alarms.first().map(|a| a.severity) {
            Some(Severity::Critical) if !unseen || (now_ms / 500).rem_euclid(2) == 0 => {
                return "#dc2626"
            }
            Some(Severity::Warning) => return "#fbbf24",
            _ => {}
        }
    }
    border_color(c, t)
}

/// The words of the banner: the worst fault, and how many others.
pub fn alarm_banner(alarms: &[crate::signal::Alarm], lang: &str) -> String {
    match alarms {
        [] => String::new(),
        [a] => a.kind.text(lang).to_string(),
        [a, rest @ ..] => format!("{} +{}", a.kind.text(lang), rest.len()),
    }
}

/// The frame's colour by the tally: on air first, then next, then aux.
pub fn border_color<'a>(c: &'a Config, t: &crate::signal::Tally) -> &'a str {
    if !c.tally.border {
        return &c.tally.off;
    }
    if t.program {
        &c.tally.program
    } else if t.preview {
        &c.tally.preview
    } else if t.aux {
        &c.tally.aux
    } else {
        &c.tally.off
    }
}

/// The UMD text: the one set, else the switcher's, else the input's name.
pub fn umd_text(c: &Config, view: Option<&SourceView>, data: &crate::anim::Data) -> String {
    let own = crate::anim::interpolate_text(&c.umd.text, data);
    if !own.trim().is_empty() {
        return own.trim().to_string();
    }
    let Some(v) = view else {
        return String::new();
    };
    if !v.tally.umd.trim().is_empty() {
        v.tally.umd.trim().to_string()
    } else {
        v.name.clone()
    }
}

/// « No signal » in the project's language, or the text set.
pub fn no_signal_text(c: &Config, lang: &str) -> String {
    if !c.no_signal.trim().is_empty() {
        c.no_signal.trim().to_string()
    } else if lang.to_lowercase().starts_with("fr") {
        "PAS DE SIGNAL".into()
    } else {
        "NO SIGNAL".into()
    }
}

/// SMPTE colour bars (EG 1-1990): (x, y, width, height) as fractions of the
/// picture's area, and the colour. 75 % bars, the reverse blue row, then
/// −I, white, +Q, black and the PLUGE.
pub const BARS: [(f32, f32, f32, f32, &str); 21] = {
    const S: f32 = 1.0 / 7.0;
    const T: f32 = 2.0 / 3.0;
    const M: f32 = 0.75;
    const B: f32 = 5.0 / 28.0;
    const P: f32 = 1.0 / 21.0;
    [
        (0.0, 0.0, S, T, "#c0c0c0"),
        (S, 0.0, S, T, "#c0c000"),
        (2.0 * S, 0.0, S, T, "#00c0c0"),
        (3.0 * S, 0.0, S, T, "#00c000"),
        (4.0 * S, 0.0, S, T, "#c000c0"),
        (5.0 * S, 0.0, S, T, "#c00000"),
        (6.0 * S, 0.0, S, T, "#0000c0"),
        (0.0, T, S, M - T, "#0000c0"),
        (S, T, S, M - T, "#131313"),
        (2.0 * S, T, S, M - T, "#c000c0"),
        (3.0 * S, T, S, M - T, "#131313"),
        (4.0 * S, T, S, M - T, "#00c0c0"),
        (5.0 * S, T, S, M - T, "#131313"),
        (6.0 * S, T, S, M - T, "#c0c0c0"),
        (0.0, M, B, 1.0 - M, "#00214c"),
        (B, M, B, 1.0 - M, "#ffffff"),
        (2.0 * B, M, B, 1.0 - M, "#32006a"),
        (3.0 * B, M, B + P, 1.0 - M, "#131313"),
        (4.0 * B + P, M, P, 1.0 - M, "#090909"),
        (4.0 * B + 2.0 * P, M, P, 1.0 - M, "#1d1d1d"),
        (6.0 * S, M, S, 1.0 - M, "#131313"),
    ]
};

/// Width over height of the F AudioVisuel signature, its quill included
/// (`strata-mv-render/src/slate_signature.svg`, `web/src/widgets/signature.svg`).
pub const LOGO_RATIO: f32 = 910.0 / 135.2;

/// The card bouncing over the bars: its plate, the logo, the words under it.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub plate: Area,
    pub logo: Area,
    pub text: Area,
    /// Corner radius of the plate.
    pub radius: f32,
}

/// Where along a back-and-forth run of `range` a point is after `travel`.
fn bounce(travel: f64, range: f64) -> f64 {
    if range <= 0.0 {
        return 0.0;
    }
    let p = (travel / range).rem_euclid(2.0);
    range * if p > 1.0 { 2.0 - p } else { p }
}

/// The card in the picture's area `video` at `now_ms` (the server's clock):
/// it crosses the picture at a tenth of its width a second, a little faster
/// down, and bounces off the edges; `seed` (the tile's place) starts each
/// tile's card elsewhere. None when the picture is too small.
pub fn no_signal_card(video: Area, now_ms: i64, seed: f32) -> Option<Card> {
    let [vx, vy, vw, vh] = video;
    let mut lh = (vh * 0.1).clamp(8.0, 110.0);
    let mut lw = lh * LOGO_RATIO;
    if lw > vw * 0.55 {
        let k = vw * 0.55 / lw;
        lw *= k;
        lh *= k;
    }
    let pad = (lh * 0.4).round();
    let (gap, th) = (lh * 0.25, lh * 0.55);
    let (pw, ph) = (lw + 2.0 * pad, pad + lh + gap + th + pad);
    if lh < 6.0 || pw > vw || ph > vh {
        return None;
    }
    let t = now_ms as f64 / 1000.0;
    let s = f64::from(seed);
    let x = vx + bounce(t * f64::from(vw) * 0.1 + s * 3.0, f64::from(vw - pw)) as f32;
    let y = vy + bounce(t * f64::from(vh) * 0.13 + s * 5.0, f64::from(vh - ph)) as f32;
    Some(Card {
        plate: [x, y, pw, ph],
        logo: [x + pad, y + pad, lw, lh],
        text: [x + pad, y + pad + lh + gap, lw, th],
        radius: pad * 0.6,
    })
}

/// A level on the scale: 0 at `min`, 1 at full scale.
/// The channels a tile's meters show, of the input's `levels`.
pub fn shown<'a, T>(c: &Vu, levels: &'a [T]) -> &'a [T] {
    let from = (c.first.max(1) as usize - 1).min(levels.len());
    &levels[from..]
}

pub fn scale(c: &Vu, db: f32) -> f32 {
    let min = c.min.min(-1.0);
    ((db - min) / -min).clamp(0.0, 1.0)
}

/// The parts of a meter lit at `level`, bottom up: (from, to, colour index
/// 0 green, 1 yellow, 2 red), fractions of the bar's height.
pub fn segments(c: &Vu, level: &Level) -> Vec<(f32, f32, u8)> {
    let v = scale(c, level.peak);
    let y = scale(c, c.yellow);
    let r = scale(c, c.red.max(c.yellow));
    let mut out = Vec::new();
    for (from, to, k) in [(0.0, y, 0u8), (y, r, 1), (r, 1.0, 2)] {
        let top = v.min(to);
        if top > from {
            out.push((from, top, k));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn markers_over_the_picture() {
        // A 4:3 picture in a 16:9 tile: drawn in the middle.
        let p = picture_area(
            Fit::Contain,
            [0.0, 0.0, 1600.0, 900.0],
            Some((640.0, 480.0)),
        );
        assert_eq!(p, [200.0, 0.0, 1200.0, 900.0]);
        assert_eq!(
            picture_area(Fit::Stretch, [1.0, 2.0, 3.0, 4.0], None),
            [1.0, 2.0, 3.0, 4.0]
        );
        let m = Markers {
            action: true,
            title: true,
            four_three: true,
            centre: true,
            afd: true,
            ..Default::default()
        };
        let p = [0.0, 0.0, 1920.0, 1080.0];
        let ml = marker_lines(&m, p, Some(9), 20.0);
        // 4 + 4 sides, the 4:3 frame's 2 lines, the cross's 2.
        assert_eq!(ml.lines.len(), 12);
        assert_eq!(ml.lines[0], [67.2, 37.8, 1785.6, 4.0]);
        assert_eq!(ml.afd.len(), 4);
        assert_eq!(ml.label.as_ref().unwrap().1, "AFD 9 · 4:3");
        // AFD 9 in a 16:9 frame: the 4:3 middle.
        assert_eq!(afd_area(9, p), Some([240.0, 0.0, 1440.0, 1080.0]));
        assert_eq!(afd_area(10, p), None);
        // AFD 10 in a 4:3 frame: a 16:9 letterbox.
        assert_eq!(
            afd_area(10, [0.0, 0.0, 1440.0, 1080.0]),
            Some([0.0, 135.0, 1440.0, 810.0])
        );
    }

    use super::*;
    use crate::signal::Tally;

    #[test]
    fn the_umd_and_the_meters_share_the_picture() {
        let c = Config::default();
        let l = layout(&c, 640.0, 360.0, 2);
        assert_eq!(l.inner, [4.0, 4.0, 632.0, 352.0]);
        // Over the picture, at the bottom: 11 % of the tile.
        let umd = l.umd.unwrap();
        assert!((umd[3] - 39.6).abs() < 1e-3);
        assert_eq!(l.video, l.inner);
        // Two bars on the right, above the UMD.
        assert_eq!(l.meters.len(), 2);
        let (a, b) = (l.meters[0], l.meters[1]);
        assert_eq!(b[0] + b[2], 4.0 + 632.0 - 6.0);
        assert_eq!(a[0] + a[2] + 2.0, b[0]);
        assert!((a[1] + a[3] - (umd[1] - 6.0)).abs() < 1e-3);
        // Beside it: the picture is smaller.
        let mut c = Config::default();
        c.umd.outside = true;
        c.umd.position = UmdPosition::Top;
        let l = layout(&c, 640.0, 360.0, 0);
        assert!(l.video[1] > l.inner[1] && l.meters.is_empty());
    }

    #[test]
    fn the_bars_fill_the_picture_and_the_card_stays_in_it() {
        // Each row of bars covers the whole width, the rows the whole height.
        for (y, h) in [
            (0.0, 2.0 / 3.0),
            (2.0 / 3.0, 0.75 - 2.0 / 3.0),
            (0.75, 0.25),
        ] {
            let w: f32 = BARS
                .iter()
                .filter(|b| (b.1 - y).abs() < 1e-6 && (b.3 - h).abs() < 1e-6)
                .map(|b| b.2)
                .sum();
            assert!((w - 1.0).abs() < 1e-5, "row at {y}: {w}");
        }
        let video = [4.0, 4.0, 632.0, 352.0];
        let mut seen = std::collections::HashSet::new();
        for i in 0..400 {
            let c = no_signal_card(video, 1_700_000_000_000 + i * 97, 0.0).unwrap();
            let [x, y, w, h] = c.plate;
            assert!(x >= 4.0 - 1e-3 && y >= 4.0 - 1e-3);
            assert!(x + w <= 636.0 + 1e-3 && y + h <= 356.0 + 1e-3);
            seen.insert((x as i32, y as i32));
        }
        // It moves.
        assert!(seen.len() > 100);
        assert!(no_signal_card([0.0, 0.0, 30.0, 20.0], 0, 0.0).is_none());
    }

    #[test]
    fn the_frame_follows_the_tally() {
        let c = Config::default();
        let mut t = Tally::default();
        assert_eq!(border_color(&c, &t), c.tally.off);
        t.preview = true;
        assert_eq!(border_color(&c, &t), c.tally.preview);
        t.program = true;
        assert_eq!(border_color(&c, &t), c.tally.program);
    }

    #[test]
    fn a_meter_lights_green_then_yellow_then_red() {
        let v = Vu::default();
        let lit = |db: f32| {
            segments(
                &v,
                &Level {
                    peak: db,
                    rms: db,
                    hold: db,
                },
            )
        };
        assert!(lit(-70.0).is_empty());
        assert_eq!(lit(-30.0).len(), 1);
        assert_eq!(lit(-12.0).len(), 2);
        let all = lit(0.0);
        assert_eq!(all.len(), 3);
        assert_eq!(all[2].1, 1.0);
    }

    #[test]
    fn the_umd_says_what_it_is_told_first() {
        let mut c = Config::default();
        let mut v = SourceView {
            name: "CAM 1".into(),
            ..Default::default()
        };
        let d = crate::anim::Data::new();
        assert_eq!(umd_text(&c, Some(&v), &d), "CAM 1");
        v.tally.umd = "Plateau".into();
        assert_eq!(umd_text(&c, Some(&v), &d), "Plateau");
        c.umd.text = "Invité".into();
        assert_eq!(umd_text(&c, Some(&v), &d), "Invité");
    }
}

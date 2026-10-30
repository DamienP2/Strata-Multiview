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
}

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

/// A level on the scale: 0 at `min`, 1 at full scale.
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

//! The measuring tools of a multiview: waveform (luma), RGB and Y'CbCr
//! parades, vectorscope, histogram, CIE 1931 chromaticity and false colour
//! of a live input's picture; goniometer and phase correlation of its sound.
//!
//! The trace (what the picture's pixels make) is computed by the server from
//! the input's pictures (`strata-mv-render/src/scopes.rs`) into the widget's
//! trace area; its graticule and labels are laid out here as items, drawn by
//! both renderers over the trace (`web/src/widgets/scope.tsx` mirrors this
//! module: keep them in sync). The graticule follows the picture's transfer:
//! % for SDR and HLG, nits for PQ (HDR10).

use serde::{Deserialize, Serialize};

use super::broadcast::{circle, path, rect, text, Area, Item};
use crate::model::TextAlign;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScopeKind {
    /// Luma of each column of the picture.
    #[default]
    Waveform,
    /// R, G and B side by side.
    Parade,
    /// Y', Cb and Cr side by side.
    YcbcrParade,
    /// Cb against Cr: hue and saturation.
    Vector,
    /// How many pixels at each level (luma and R, G, B).
    Histogram,
    /// Where the colours are on the CIE 1931 xy diagram, against the gamuts.
    Cie,
    /// The picture in false colours by exposure.
    FalseColor,
    /// The sound's left against its right (Lissajous), and the phase
    /// correlation of the two.
    Goniometer,
}

/// How the picture's code values read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Transfer {
    #[default]
    Sdr,
    /// SMPTE ST 2084 (HDR10, Dolby Vision).
    Pq,
    /// ARIB STD-B67 (HLG).
    Hlg,
}

impl Transfer {
    /// From ffmpeg's name of a transfer ("smpte2084", "arib-std-b67"…).
    pub fn of(name: &str) -> Transfer {
        let n = name.to_ascii_lowercase();
        if n.contains("2084") || n == "pq" {
            Transfer::Pq
        } else if n.contains("b67") || n.contains("hlg") {
            Transfer::Hlg
        } else {
            Transfer::Sdr
        }
    }
}

/// The settings of a scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScopeConfig {
    pub input: String,
    pub kind: ScopeKind,
    /// Lines and their values.
    pub graticule: bool,
    pub labels: bool,
    /// Brightness of the trace (0.25 to 4).
    pub intensity: f32,
    /// The trace's colour (waveform, histogram's luma; the others in their
    /// own colours).
    pub color: String,
    pub background: String,
}

impl Default for ScopeConfig {
    fn default() -> Self {
        Self {
            input: String::new(),
            kind: ScopeKind::Waveform,
            graticule: true,
            labels: true,
            intensity: 1.0,
            color: "#6dff8f".into(),
            background: "#05070b".into(),
        }
    }
}

/// Colours of the graticule and its labels.
pub const GRID: &str = "#ffffff38";
pub const GRID_STRONG: &str = "#ffffff70";
pub const LABEL: &str = "#ffffffb0";

/// PQ (ST 2084): the signal (0–1) of `nits` cd/m².
pub fn pq_signal(nits: f32) -> f32 {
    let (m1, m2, c1, c2, c3) = (0.159_301_76, 78.843_75, 0.835_937_5, 18.851_562, 18.6875);
    let y = (nits / 10_000.0).clamp(0.0, 1.0).powf(m1);
    ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
}

/// PQ (ST 2084): the cd/m² of signal `v` (0–1).
pub fn pq_nits(v: f32) -> f32 {
    let (m1, m2, c1, c2, c3) = (0.159_301_76, 78.843_75, 0.835_937_5, 18.851_562, 18.6875);
    let p = v.clamp(0.0, 1.0).powf(1.0 / m2);
    ((p - c1).max(0.0) / (c2 - c3 * p)).powf(1.0 / m1) * 10_000.0
}

/// The lines of a level scale (signal 0–1) and their labels, by transfer.
pub fn levels(t: Transfer) -> Vec<(f32, String, bool)> {
    match t {
        Transfer::Pq => [0.0, 1.0, 10.0, 100.0, 203.0, 1000.0, 4000.0, 10_000.0]
            .iter()
            .map(|&n| {
                let label = if n >= 1000.0 {
                    format!("{}k", n / 1000.0)
                } else {
                    format!("{n}")
                };
                // 203 nits: HDR reference white (BT.2408).
                (pq_signal(n), label, n == 203.0 || n == 100.0)
            })
            .collect(),
        // HLG: 75 % is reference white (BT.2408).
        Transfer::Hlg => [0.0, 25.0, 50.0, 75.0, 100.0]
            .iter()
            .map(|&p| (p / 100.0, format!("{p}"), p == 75.0))
            .collect(),
        Transfer::Sdr => [0.0, 25.0, 50.0, 75.0, 100.0]
            .iter()
            .map(|&p| (p / 100.0, format!("{p}"), p == 0.0 || p == 100.0))
            .collect(),
    }
}

/// The unit of the level scale.
pub fn unit(t: Transfer) -> &'static str {
    match t {
        Transfer::Pq => "nits",
        Transfer::Hlg => "% HLG",
        Transfer::Sdr => "%",
    }
}

/// Where the trace goes in a `w` × `h` widget: the space left of the labels,
/// a square for the vectorscope and the CIE diagram.
pub fn trace_area(c: &ScopeConfig, w: f32, h: f32) -> Area {
    let pad = (w.min(h) * 0.04).clamp(2.0, 16.0);
    let left = if c.labels
        && !matches!(
            c.kind,
            ScopeKind::Vector | ScopeKind::Cie | ScopeKind::FalseColor | ScopeKind::Goniometer
        ) {
        (h * 0.09).clamp(18.0, 48.0)
    } else {
        0.0
    };
    // Room for the parades' names and the histogram's scale, under or over.
    let top = if c.labels && matches!(c.kind, ScopeKind::Parade | ScopeKind::YcbcrParade) {
        (h * 0.07).clamp(10.0, 28.0)
    } else {
        0.0
    };
    let bottom = if c.labels && c.kind == ScopeKind::Histogram {
        (h * 0.07).clamp(10.0, 28.0)
    } else if c.kind == ScopeKind::Goniometer {
        // The correlation meter.
        correlation_height(h)
    } else {
        0.0
    };
    let right = if c.kind == ScopeKind::FalseColor && c.labels {
        (w * 0.1).clamp(28.0, 90.0)
    } else {
        0.0
    };
    let a = [
        pad + left,
        pad + top,
        (w - 2.0 * pad - left - right).max(1.0),
        (h - 2.0 * pad - top - bottom).max(1.0),
    ];
    if matches!(
        c.kind,
        ScopeKind::Vector | ScopeKind::Cie | ScopeKind::Goniometer
    ) {
        let s = a[2].min(a[3]);
        [a[0] + (a[2] - s) / 2.0, a[1] + (a[3] - s) / 2.0, s, s]
    } else {
        a
    }
}

/// The height of the goniometer's correlation meter.
pub fn correlation_height(h: f32) -> f32 {
    (h * 0.12).clamp(16.0, 44.0)
}

/// A sample (left, right, −1 to 1) on the goniometer in `a`: mono up the
/// middle (M), left alone on the upper left diagonal, right on the right.
pub fn gonio_point(l: f32, r: f32, a: Area) -> [f32; 2] {
    let half = a[2].min(a[3]) / 2.0 * 0.95;
    let (cx, cy) = (a[0] + a[2] / 2.0, a[1] + a[3] / 2.0);
    [cx + (r - l) / 2.0 * half, cy - (l + r) / 2.0 * half]
}

/// The colour of a phase correlation: out of phase red, wide amber, in
/// phase green.
pub fn correlation_color(v: f32) -> &'static str {
    if v < 0.0 {
        "#ff3b3b"
    } else if v < 0.3 {
        "#fbbf24"
    } else {
        "#2bd96b"
    }
}

/// The parades' three panels in the trace area (and the gap between them).
pub fn panels(a: Area) -> [Area; 3] {
    let gap = (a[2] * 0.02).max(2.0);
    let w = (a[2] - 2.0 * gap) / 3.0;
    [0, 1, 2].map(|i| [a[0] + i as f32 * (w + gap), a[1], w, a[3]])
}

/// The height of signal `v` (0–1) in `a`.
pub fn level_y(v: f32, a: Area) -> f32 {
    a[1] + a[3] * (1.0 - v)
}

/// Cb, Cr (−0.5 to 0.5) on the vectorscope in `a`.
pub fn vector_point(cb: f32, cr: f32, a: Area) -> [f32; 2] {
    let r = a[2].min(a[3]) / 2.0 * 0.92;
    let (cx, cy) = (a[0] + a[2] / 2.0, a[1] + a[3] / 2.0);
    // 100 % red and cyan (the farthest, 0.513) just inside the circle.
    [cx + cb * 1.85 * r, cy - cr * 1.85 * r]
}

/// Y', Cb, Cr (BT.709) of R', G', B' (0–1).
pub fn ycbcr(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    (y, (b - y) / 1.8556, (r - y) / 1.5748)
}

/// The CIE xy point on the diagram in `a` (x 0–0.8, y 0–0.9).
pub fn cie_point(x: f32, y: f32, a: Area) -> [f32; 2] {
    [a[0] + x / 0.8 * a[2], a[1] + a[3] - y / 0.9 * a[3]]
}

/// The spectral locus of the CIE 1931 2° observer, 380 to 700 nm.
pub const LOCUS: [[f32; 2]; 33] = [
    [0.1741, 0.0050],
    [0.1740, 0.0050],
    [0.1738, 0.0049],
    [0.1733, 0.0048],
    [0.1726, 0.0048],
    [0.1714, 0.0051],
    [0.1689, 0.0069],
    [0.1644, 0.0109],
    [0.1566, 0.0177],
    [0.1440, 0.0297],
    [0.1241, 0.0578],
    [0.0913, 0.1327],
    [0.0687, 0.2007],
    [0.0454, 0.2950],
    [0.0235, 0.4127],
    [0.0082, 0.5384],
    [0.0039, 0.6548],
    [0.0139, 0.7502],
    [0.0389, 0.8120],
    [0.0743, 0.8338],
    [0.1142, 0.8262],
    [0.1547, 0.8059],
    [0.1929, 0.7816],
    [0.2296, 0.7543],
    [0.2658, 0.7243],
    [0.3016, 0.6923],
    [0.3373, 0.6589],
    [0.3731, 0.6245],
    [0.4087, 0.5896],
    [0.4441, 0.5547],
    [0.4788, 0.5202],
    [0.5125, 0.4866],
    [0.7347, 0.2653],
];

/// The gamuts drawn on the CIE diagram: name, R, G, B primaries, colour.
pub const GAMUTS: [(&str, [[f32; 2]; 3], &str); 3] = [
    (
        "709",
        [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]],
        "#ffffffc0",
    ),
    (
        "P3",
        [[0.68, 0.32], [0.265, 0.69], [0.15, 0.06]],
        "#ffd34ec0",
    ),
    (
        "2020",
        [[0.708, 0.292], [0.17, 0.797], [0.131, 0.046]],
        "#4ed8ffc0",
    ),
];

/// D65.
pub const WHITE: [f32; 2] = [0.3127, 0.3290];

/// False colour bands: up to this signal (0–1), this colour, its name.
/// (SDR and HLG: % of signal; PQ: by nits.)
pub fn false_colors(t: Transfer) -> Vec<(f32, &'static str, String)> {
    match t {
        Transfer::Pq => vec![
            (pq_signal(0.5), "#5a2ca0", "<0.5".into()),
            (pq_signal(5.0), "#1f5fd6", "<5".into()),
            (pq_signal(50.0), "#3a3a3a", "<50".into()),
            (pq_signal(120.0), "#7a7a7a", "<120".into()),
            (pq_signal(250.0), "#2fb34a", "≈203".into()),
            (pq_signal(1000.0), "#f2d13a", "<1k".into()),
            (pq_signal(4000.0), "#ff8a1f", "<4k".into()),
            (1.01, "#ff2a2a", "4k+".into()),
        ],
        _ => vec![
            (0.025, "#5a2ca0", "<2.5".into()),
            (0.10, "#1f5fd6", "<10".into()),
            (0.40, "#3a3a3a", "<40".into()),
            (0.50, "#2fb34a", "40–50".into()),
            (0.70, "#7a7a7a", "<70".into()),
            (0.80, "#ff9fc2", "70–80".into()),
            (0.97, "#bdbdbd", "<97".into()),
            (0.995, "#f2d13a", "97–100".into()),
            (1.01, "#ff2a2a", "100".into()),
        ],
    }
}

/// The graticule and labels of a scope (`under`: its background, drawn
/// before the trace; `over`: drawn over it).
pub fn layout(
    c: &ScopeConfig,
    w: f32,
    h: f32,
    t: Transfer,
    correlation: Option<f32>,
) -> (Vec<Item>, Vec<Item>) {
    let mut under = vec![rect([0.0, 0.0, w, h], 0.0, &c.background)];
    let mut over = Vec::new();
    let a = trace_area(c, w, h);
    let size = (h * 0.045).clamp(8.0, 18.0);
    let lw = (h / 400.0).clamp(1.0, 2.0);
    let label = |over: &mut Vec<Item>, box_: Area, s: &str, align: TextAlign| {
        over.push(text(box_, s, size, 600, true, LABEL, align));
    };
    match c.kind {
        ScopeKind::Waveform | ScopeKind::Parade | ScopeKind::YcbcrParade => {
            let parts: Vec<Area> = if c.kind == ScopeKind::Waveform {
                vec![a]
            } else {
                panels(a).to_vec()
            };
            if c.graticule {
                for (v, name, strong) in levels(t) {
                    let y = level_y(v, a);
                    for p in &parts {
                        over.push(path(
                            vec![[p[0], y], [p[0] + p[2], y]],
                            false,
                            lw,
                            if strong { GRID_STRONG } else { GRID },
                        ));
                    }
                    if c.labels {
                        label(
                            &mut over,
                            [0.0, y - size * 0.6, a[0] - 4.0, size * 1.2],
                            &name,
                            TextAlign::Right,
                        );
                    }
                }
            }
            if c.labels && c.kind != ScopeKind::Waveform {
                let names = if c.kind == ScopeKind::Parade {
                    [("R", "#ff6b6b"), ("G", "#6bff8a"), ("B", "#6ba8ff")]
                } else {
                    [("Y'", "#ffffff"), ("Cb", "#6ba8ff"), ("Cr", "#ff6b6b")]
                };
                for (p, (n, col)) in parts.iter().zip(names) {
                    over.push(text(
                        [p[0], a[1] - size * 1.4, p[2], size * 1.3],
                        n,
                        size,
                        700,
                        false,
                        col,
                        TextAlign::Center,
                    ));
                }
            }
            if c.labels {
                label(
                    &mut over,
                    [a[0] + a[2] - 80.0, a[1], 78.0, size * 1.2],
                    unit(t),
                    TextAlign::Right,
                );
            }
        }
        ScopeKind::Histogram => {
            if c.graticule {
                for (v, name, strong) in levels(t) {
                    let x = a[0] + a[2] * v;
                    over.push(path(
                        vec![[x, a[1]], [x, a[1] + a[3]]],
                        false,
                        lw,
                        if strong { GRID_STRONG } else { GRID },
                    ));
                    if c.labels {
                        label(
                            &mut over,
                            [x - 30.0, a[1] + a[3] + 2.0, 60.0, size * 1.2],
                            &name,
                            TextAlign::Center,
                        );
                    }
                }
            }
        }
        ScopeKind::Vector => {
            let center = [a[0] + a[2] / 2.0, a[1] + a[3] / 2.0];
            let r = a[2].min(a[3]) / 2.0 * 0.92;
            if c.graticule {
                over.push(circle(center, r, lw, GRID_STRONG));
                over.push(circle(center, r * 0.5, lw, GRID));
                over.push(path(
                    vec![[center[0] - r, center[1]], [center[0] + r, center[1]]],
                    false,
                    lw,
                    GRID,
                ));
                over.push(path(
                    vec![[center[0], center[1] - r], [center[0], center[1] + r]],
                    false,
                    lw,
                    GRID,
                ));
                // The skin tone line (about 123°, from the centre through yellow-red).
                let ang = 123.0_f32.to_radians();
                over.push(path(
                    vec![
                        center,
                        [center[0] + r * ang.cos(), center[1] - r * ang.sin()],
                    ],
                    false,
                    lw,
                    "#ffb07080",
                ));
                // The 75 % and 100 % colour bar targets.
                for (name, rgb) in [
                    ("R", [1.0, 0.0, 0.0]),
                    ("MG", [1.0, 0.0, 1.0]),
                    ("B", [0.0, 0.0, 1.0]),
                    ("CY", [0.0, 1.0, 1.0]),
                    ("G", [0.0, 1.0, 0.0]),
                    ("YL", [1.0, 1.0, 0.0]),
                ] {
                    for (k, s) in [(0.75_f32, 0.06_f32), (1.0, 0.035)] {
                        let (_, cb, cr) = ycbcr(rgb[0] * k, rgb[1] * k, rgb[2] * k);
                        let p = vector_point(cb, cr, a);
                        let d = r * s;
                        over.push(path(
                            vec![
                                [p[0] - d, p[1] - d],
                                [p[0] + d, p[1] - d],
                                [p[0] + d, p[1] + d],
                                [p[0] - d, p[1] + d],
                            ],
                            true,
                            lw,
                            if k < 1.0 { GRID_STRONG } else { GRID },
                        ));
                        if k < 1.0 && c.labels {
                            let (_, cb, cr) = ycbcr(rgb[0] * 1.12, rgb[1] * 1.12, rgb[2] * 1.12);
                            let q = vector_point(cb, cr, a);
                            label(
                                &mut over,
                                [q[0] - 20.0, q[1] - size * 0.6, 40.0, size * 1.2],
                                name,
                                TextAlign::Center,
                            );
                        }
                    }
                }
            }
        }
        ScopeKind::Cie => {
            if c.graticule {
                let mut locus: Vec<[f32; 2]> =
                    LOCUS.iter().map(|p| cie_point(p[0], p[1], a)).collect();
                locus.dedup();
                over.push(path(locus, true, lw, GRID_STRONG));
                for (i, (name, prim, col)) in GAMUTS.iter().enumerate() {
                    over.push(path(
                        prim.iter().map(|p| cie_point(p[0], p[1], a)).collect(),
                        true,
                        lw,
                        col,
                    ));
                    if c.labels {
                        label(
                            &mut over,
                            [
                                a[0] + a[2] - 70.0,
                                a[1] + size * 1.3 * i as f32,
                                68.0,
                                size * 1.2,
                            ],
                            name,
                            TextAlign::Right,
                        );
                        if let Item::Text { color, .. } = over.last_mut().expect("pushed") {
                            *color = col.to_string();
                        }
                    }
                }
                let wp = cie_point(WHITE[0], WHITE[1], a);
                over.push(circle(wp, (a[2] * 0.008).max(2.0), lw, "#ffffff"));
            }
        }
        ScopeKind::Goniometer => {
            let center = [a[0] + a[2] / 2.0, a[1] + a[3] / 2.0];
            let r = a[2].min(a[3]) / 2.0 * 0.95;
            if c.graticule {
                // The square of full scale, its diagonals (L, R), M and S.
                let k = r / std::f32::consts::SQRT_2;
                let corner = |dx: f32, dy: f32| [center[0] + dx * r, center[1] + dy * r];
                over.push(path(
                    vec![
                        corner(0.0, -1.0),
                        corner(1.0, 0.0),
                        corner(0.0, 1.0),
                        corner(-1.0, 0.0),
                    ],
                    true,
                    lw,
                    GRID_STRONG,
                ));
                over.push(path(
                    vec![corner(0.0, -1.0), corner(0.0, 1.0)],
                    false,
                    lw,
                    GRID,
                ));
                over.push(path(
                    vec![corner(-1.0, 0.0), corner(1.0, 0.0)],
                    false,
                    lw,
                    GRID,
                ));
                over.push(path(
                    vec![
                        [center[0] - k, center[1] - k],
                        [center[0] + k, center[1] + k],
                    ],
                    false,
                    lw,
                    GRID,
                ));
                over.push(path(
                    vec![
                        [center[0] + k, center[1] - k],
                        [center[0] - k, center[1] + k],
                    ],
                    false,
                    lw,
                    GRID,
                ));
                if c.labels {
                    let d = k + size * 0.7;
                    for (name, x, y) in [
                        ("L", center[0] - d, center[1] - d),
                        ("R", center[0] + d, center[1] - d),
                        ("M", center[0], center[1] - r - size * 0.2),
                        ("S", center[0] + r + size * 0.6, center[1]),
                    ] {
                        label(
                            &mut over,
                            [x - 20.0, y - size * 0.6, 40.0, size * 1.2],
                            name,
                            TextAlign::Center,
                        );
                    }
                }
            }
            // The correlation meter, under it: −1 to +1.
            let mh = correlation_height(h);
            let pad = (w.min(h) * 0.04).clamp(2.0, 16.0);
            let lab = if c.labels { size * 2.2 } else { 0.0 };
            let bar = [
                pad + lab,
                h - pad - mh * 0.62,
                (w - 2.0 * pad - 2.0 * lab).max(1.0),
                mh * 0.36,
            ];
            under.push(rect(bar, bar[3] / 2.0, "#ffffff14"));
            let mid = bar[0] + bar[2] / 2.0;
            over.push(rect(
                [mid - 0.5, bar[1] - 2.0, 1.0, bar[3] + 4.0],
                0.0,
                GRID_STRONG,
            ));
            if let Some(v) = correlation {
                let v = v.clamp(-1.0, 1.0);
                let x = mid + v * bar[2] / 2.0;
                let col = correlation_color(v);
                let (x0, x1) = if v < 0.0 { (x, mid) } else { (mid, x) };
                over.push(rect(
                    [x0, bar[1], (x1 - x0).max(1.0), bar[3]],
                    bar[3] / 2.0,
                    col,
                ));
                over.push(rect(
                    [x - 1.5, bar[1] - 3.0, 3.0, bar[3] + 6.0],
                    1.0,
                    "#ffffff",
                ));
            }
            if c.labels {
                label(
                    &mut over,
                    [pad, bar[1] - size * 0.4, lab - 4.0, bar[3] + size * 0.8],
                    "−1",
                    TextAlign::Right,
                );
                label(
                    &mut over,
                    [
                        bar[0] + bar[2] + 4.0,
                        bar[1] - size * 0.4,
                        lab - 4.0,
                        bar[3] + size * 0.8,
                    ],
                    "+1",
                    TextAlign::Left,
                );
                let value = correlation.map_or("—".to_string(), |v| {
                    let v = (v.clamp(-1.0, 1.0) * 100.0).round() / 100.0;
                    if v < 0.0 {
                        format!("\u{2212}{:.2}", -v)
                    } else {
                        format!("{v:.2}")
                    }
                });
                over.push(text(
                    [w - pad - 90.0, pad, 88.0, size * 1.3],
                    value,
                    size * 1.1,
                    700,
                    true,
                    correlation.map_or(LABEL, correlation_color),
                    TextAlign::Right,
                ));
            }
        }
        ScopeKind::FalseColor => {
            if c.labels {
                let bands = false_colors(t);
                let x = a[0] + a[2] + 6.0;
                let bw = w - x - 4.0;
                let bh = (a[3] / bands.len() as f32).min(size * 1.6);
                for (i, (_, col, name)) in bands.iter().rev().enumerate() {
                    let y = a[1] + i as f32 * bh;
                    under.push(rect([x, y + 1.0, bw * 0.3, bh - 2.0], 2.0, col));
                    label(
                        &mut over,
                        [x + bw * 0.34, y, bw * 0.66, bh],
                        name,
                        TextAlign::Left,
                    );
                }
            }
        }
    }
    (under, over)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_both_ways() {
        for n in [0.1_f32, 1.0, 100.0, 203.0, 1000.0, 10_000.0] {
            let back = pq_nits(pq_signal(n));
            assert!((back - n).abs() / n < 0.01, "{n} → {back}");
        }
        // 100 nits ≈ 51 % of the PQ signal, 1000 nits ≈ 75 %.
        assert!((pq_signal(100.0) - 0.508).abs() < 0.01);
        assert!((pq_signal(1000.0) - 0.752).abs() < 0.01);
    }

    #[test]
    fn mono_goes_up_the_middle_of_the_goniometer() {
        let a = [0.0, 0.0, 200.0, 200.0];
        assert_eq!(gonio_point(0.0, 0.0, a), [100.0, 100.0]);
        let p = gonio_point(1.0, 1.0, a);
        assert!((p[0] - 100.0).abs() < 1e-3 && p[1] < 10.0, "{p:?}");
        // Left alone: up and left.
        let p = gonio_point(1.0, 0.0, a);
        assert!(p[0] < 100.0 && p[1] < 100.0, "{p:?}");
        // Out of phase: across.
        let p = gonio_point(1.0, -1.0, a);
        assert!((p[1] - 100.0).abs() < 1e-3 && p[0] < 10.0, "{p:?}");
    }

    #[test]
    fn transfers_by_name() {
        assert_eq!(Transfer::of("smpte2084"), Transfer::Pq);
        assert_eq!(Transfer::of("arib-std-b67"), Transfer::Hlg);
        assert_eq!(Transfer::of("bt709"), Transfer::Sdr);
        assert_eq!(Transfer::of(""), Transfer::Sdr);
    }

    #[test]
    fn colour_bar_targets_sit_on_the_vectorscope() {
        let a = [0.0, 0.0, 200.0, 200.0];
        // 100 % red: up, a little left (103°), inside the circle.
        let (_, cb, cr) = ycbcr(1.0, 0.0, 0.0);
        let p = vector_point(cb, cr, a);
        assert!(p[0] < 100.0 && p[0] > 70.0 && p[1] < 20.0, "{p:?}");
        let d = ((p[0] - 100.0).powi(2) + (p[1] - 100.0).powi(2)).sqrt();
        assert!(d < 92.0, "{d}");
        // Grey: the centre.
        let (_, cb, cr) = ycbcr(0.5, 0.5, 0.5);
        assert_eq!(vector_point(cb, cr, a), [100.0, 100.0]);
    }

    #[test]
    fn every_scope_lays_out() {
        for kind in [
            ScopeKind::Waveform,
            ScopeKind::Parade,
            ScopeKind::YcbcrParade,
            ScopeKind::Vector,
            ScopeKind::Histogram,
            ScopeKind::Cie,
            ScopeKind::FalseColor,
            ScopeKind::Goniometer,
        ] {
            for t in [Transfer::Sdr, Transfer::Pq, Transfer::Hlg] {
                let c = ScopeConfig {
                    kind,
                    ..Default::default()
                };
                let (under, over) = layout(&c, 640.0, 360.0, t, Some(0.5));
                assert!(!under.is_empty());
                let a = trace_area(&c, 640.0, 360.0);
                assert!(a[2] > 0.0 && a[3] > 0.0);
                assert!(
                    kind == ScopeKind::FalseColor || !over.is_empty(),
                    "{kind:?}"
                );
            }
        }
    }
}

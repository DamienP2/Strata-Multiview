//! The traces of the scopes (`strata_mv_core::wid::scope`): what a picture's
//! pixels make on a waveform, a parade, a vectorscope, a histogram, the CIE
//! diagram, or in false colours. One picture for both renderers: the server
//! makes it from the input's newest picture (`LiveInputs::scope`), the
//! outputs draw it into the trace area and the web pages receive it (screen
//! blended over the widget's background: black adds nothing).
//!
//! The positions come from the core's functions (`level_y`, `vector_point`,
//! `cie_point`, `panels`), so the trace sits on the graticule laid out there.

use strata_mv_core::wid::scope::{
    cie_point, false_colors, level_y, panels, pq_nits, vector_point, ycbcr, ScopeKind, Transfer,
};
use tiny_skia::Pixmap;

/// What trace is asked for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopeRequest {
    pub kind: ScopeKind,
    /// The trace area, pixels (at most 1024 each way).
    pub w: u32,
    pub h: u32,
    /// Brightness of the trace, in hundredths (100: normal).
    pub intensity: u16,
    /// The colour of a one-colour trace (waveform, histogram's luma).
    pub color: [u8; 3],
    pub transfer: Transfer,
    /// BT.2020 primaries (else BT.709): the CIE trace.
    pub wide: bool,
}

impl ScopeRequest {
    /// The size kept within reason, the intensity within 0.25–4.
    pub fn bounded(mut self) -> Self {
        self.w = self.w.clamp(16, 1024);
        self.h = self.h.clamp(16, 1024);
        self.intensity = self.intensity.clamp(25, 400);
        self
    }
}

/// `#rrggbb` → [r, g, b] (green when invalid).
pub fn rgb_of(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    let byte = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
    match (byte(0), byte(2), byte(4)) {
        (Some(r), Some(g), Some(b)) => [r, g, b],
        _ => [0x6d, 0xff, 0x8f],
    }
}

/// Counts of samples per trace pixel, in up to three colours.
struct Acc {
    w: usize,
    h: usize,
    n: Vec<[f32; 3]>,
}

impl Acc {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            n: vec![[0.0; 3]; w * h],
        }
    }
    #[inline]
    fn add(&mut self, x: f32, y: f32, c: usize, v: f32) {
        if x < 0.0 || y < 0.0 {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        if x < self.w && y < self.h {
            self.n[y * self.w + x][c] += v;
        }
    }
}

/// The samples of a picture: about `cols` × `rows` of its pixels, evenly
/// spread, as R', G', B' (0–1) with their column (0–1).
fn samples(pic: &Pixmap, cols: usize, rows: usize, mut f: impl FnMut(f32, f32, f32, f32)) {
    let (pw, ph) = (pic.width() as usize, pic.height() as usize);
    if pw == 0 || ph == 0 {
        return;
    }
    let (cols, rows) = (cols.clamp(1, pw), rows.clamp(1, ph));
    let data = pic.data();
    for j in 0..rows {
        let y = (j * ph + ph / 2) / rows;
        for i in 0..cols {
            let x = (i * pw + pw / 2) / cols;
            let o = (y * pw + x) * 4;
            let a = data[o + 3];
            if a == 0 {
                continue;
            }
            // Premultiplied: the inputs are opaque, this is for the others.
            let k = 1.0 / a as f32;
            f(
                (i as f32 + 0.5) / cols as f32,
                data[o] as f32 * k,
                data[o + 1] as f32 * k,
                data[o + 2] as f32 * k,
            );
        }
    }
}

/// R'G'B' (0–1) → linear light (relative, 0–1 for SDR; PQ in 10 000 nits).
fn linear(v: f32, t: Transfer) -> f32 {
    match t {
        Transfer::Sdr => v.max(0.0).powf(2.4),
        Transfer::Pq => pq_nits(v) / 10_000.0,
        Transfer::Hlg => {
            // BT.2100 inverse OETF.
            let (a, b, c) = (0.178_832_77, 0.284_668_92, 0.559_910_7);
            if v <= 0.5 {
                v * v / 3.0
            } else {
                (((v - c) / a).exp() + b) / 12.0
            }
        }
    }
}

/// Linear RGB → XYZ, BT.709 and BT.2020 primaries (D65).
const XYZ_709: [[f32; 3]; 3] = [
    [0.4124, 0.3576, 0.1805],
    [0.2126, 0.7152, 0.0722],
    [0.0193, 0.1192, 0.9505],
];
const XYZ_2020: [[f32; 3]; 3] = [
    [0.6370, 0.1446, 0.1689],
    [0.2627, 0.6780, 0.0593],
    [0.0000, 0.0281, 1.0610],
];

/// How bright a pixel with `n` samples shows, `k`: samples for half.
#[inline]
fn glow(n: f32, k: f32) -> f32 {
    1.0 - (-n * k).exp()
}

/// Into an opaque picture: each pixel `[r, g, b]` 0–1.
fn picture(w: usize, h: usize, mut px: impl FnMut(usize) -> [f32; 3]) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w as u32, h as u32)?;
    let d = pm.data_mut();
    for i in 0..w * h {
        let [r, g, b] = px(i);
        d[i * 4] = (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        d[i * 4 + 1] = (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        d[i * 4 + 2] = (b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        d[i * 4 + 3] = 255;
    }
    Some(pm)
}

/// How a vectorscope or CIE sample spreads around its pixel.
const SPLAT: [(isize, isize, f32); 5] = [
    (0, 0, 1.0),
    (1, 0, 0.5),
    (-1, 0, 0.5),
    (0, 1, 0.5),
    (0, -1, 0.5),
];

const RED: [f32; 3] = [1.0, 0.32, 0.32];
const GREEN: [f32; 3] = [0.32, 1.0, 0.45];
const BLUE: [f32; 3] = [0.35, 0.6, 1.0];
const WHITE: [f32; 3] = [0.92, 0.94, 1.0];

/// The trace of `pic`: an opaque picture `w` × `h` (false colour: the
/// picture's shape fitted in it), black where nothing is.
pub fn trace(pic: &Pixmap, r: &ScopeRequest) -> Option<Pixmap> {
    let r = r.clone().bounded();
    let (w, h) = (r.w as usize, r.h as usize);
    let a = [0.0, 0.0, w as f32, h as f32];
    let gain = r.intensity as f32 / 100.0;
    let one = r.color.map(|c| c as f32 / 255.0);
    match r.kind {
        ScopeKind::Waveform | ScopeKind::Parade | ScopeKind::YcbcrParade => {
            let parts = if r.kind == ScopeKind::Waveform {
                vec![a]
            } else {
                panels(a).to_vec()
            };
            let cols = (parts[0][2] as usize * 2)
                .clamp(64, 1920)
                .min(pic.width() as usize);
            let rows = (h * 3 / 4).clamp(64, 540).min(pic.height() as usize);
            let mut acc = Acc::new(w, h);
            samples(pic, cols, rows, |u, rr, g, b| {
                let vals = match r.kind {
                    ScopeKind::Waveform => [ycbcr(rr, g, b).0, -1.0, -1.0],
                    ScopeKind::Parade => [rr, g, b],
                    _ => {
                        let (y, cb, cr) = ycbcr(rr, g, b);
                        [y, cb + 0.5, cr + 0.5]
                    }
                };
                for (c, (v, p)) in vals.iter().zip(&parts).enumerate() {
                    if *v < 0.0 {
                        continue;
                    }
                    let x = p[0] + u * p[2];
                    acc.add(x, level_y(v.clamp(0.0, 1.0), a).min(h as f32 - 1.0), c, 1.0);
                }
            });
            // A column receives `rows` samples per output column of samples.
            let per = rows as f32 * cols as f32 / parts[0][2].max(1.0);
            let k = gain * 40.0 / per.max(1.0);
            let colors = match r.kind {
                ScopeKind::Waveform => [one, one, one],
                ScopeKind::Parade => [RED, GREEN, BLUE],
                _ => [WHITE, BLUE, RED],
            };
            picture(w, h, |i| {
                let n = acc.n[i];
                let mut o = [0.0; 3];
                for c in 0..3 {
                    let g = glow(n[c], k);
                    for (oc, cc) in o.iter_mut().zip(colors[c]) {
                        *oc += g * cc;
                    }
                }
                o
            })
        }
        ScopeKind::Vector | ScopeKind::Cie => {
            // Counts, and the sum of the samples' colours.
            let mut acc = Acc::new(w, h);
            let mut sum = vec![[0.0_f32; 3]; w * h];
            let (cols, rows) = (480, 270);
            let m = if r.wide { XYZ_2020 } else { XYZ_709 };
            samples(pic, cols, rows, |_, rr, g, b| {
                let p = if r.kind == ScopeKind::Vector {
                    let (_, cb, cr) = ycbcr(rr, g, b);
                    vector_point(cb, cr, a)
                } else {
                    let l = [
                        linear(rr, r.transfer),
                        linear(g, r.transfer),
                        linear(b, r.transfer),
                    ];
                    let xyz = m.map(|row| row[0] * l[0] + row[1] * l[1] + row[2] * l[2]);
                    let s = xyz[0] + xyz[1] + xyz[2];
                    if s < 1e-5 {
                        return;
                    }
                    cie_point(xyz[0] / s, xyz[1] / s, a)
                };
                if p[0] < 0.0 || p[1] < 0.0 || p[0] >= w as f32 || p[1] >= h as f32 {
                    return;
                }
                // A dot a little wider than a pixel (a flat colour shows).
                let (px, py) = (p[0] as usize, p[1] as usize);
                for (dx, dy, wt) in SPLAT {
                    let (x, y) = (px as isize + dx, py as isize + dy);
                    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                        continue;
                    }
                    let i = y as usize * w + x as usize;
                    acc.n[i][0] += wt;
                    sum[i][0] += rr * wt;
                    sum[i][1] += g * wt;
                    sum[i][2] += b * wt;
                }
            });
            let k = gain * 40.0 * (w * h) as f32 / (cols * rows) as f32 / 100.0;
            picture(w, h, |i| {
                let n = acc.n[i][0];
                if n == 0.0 {
                    return [0.0; 3];
                }
                let g = glow(n, k);
                // Its own colour, at full saturation, a little whitened.
                let c = sum[i].map(|s| s / n);
                let top = c[0].max(c[1]).max(c[2]).max(1e-3);
                c.map(|v| (0.35 + 0.65 * v / top) * g)
            })
        }
        ScopeKind::Histogram => {
            // The pictures' 256 levels at most (more bins: a comb).
            let bins = w.clamp(1, 256);
            let mut hist = vec![[0.0_f32; 4]; bins];
            samples(pic, 480, 270, |_, rr, g, b| {
                let y = ycbcr(rr, g, b).0;
                for (c, v) in [rr, g, b, y].iter().enumerate() {
                    let i = ((v.clamp(0.0, 1.0) * (bins - 1) as f32) + 0.5) as usize;
                    hist[i.min(bins - 1)][c] += 1.0;
                }
            });
            // Scaled to the busiest level but the extremes (a black or a
            // white picture would flatten the rest).
            let mut tops: Vec<f32> = hist.iter().flat_map(|b| b.iter().copied()).collect();
            tops.sort_by(|x, y| y.total_cmp(x));
            let top = tops.get(tops.len() / 50).copied().unwrap_or(1.0).max(1.0);
            let heights: Vec<[f32; 4]> = hist
                .iter()
                .map(|b| b.map(|n| (n / top * gain).min(1.0) * h as f32))
                .collect();
            picture(w, h, |i| {
                let (x, y) = (i % w * bins / w, i / w);
                let from_bottom = (h - y) as f32;
                let mut o = [0.0; 3];
                for (c, col) in [RED, GREEN, BLUE].iter().enumerate() {
                    if from_bottom <= heights[x][c] {
                        for (oc, cc) in o.iter_mut().zip(col) {
                            *oc += cc * 0.42;
                        }
                    }
                }
                if from_bottom <= heights[x][3] {
                    for (oc, cc) in o.iter_mut().zip(one) {
                        *oc = oc.max(cc * 0.55);
                    }
                }
                o
            })
        }
        ScopeKind::FalseColor => {
            let (pw, ph) = (pic.width() as f32, pic.height() as f32);
            if pw < 1.0 || ph < 1.0 {
                return None;
            }
            let k = (w as f32 / pw).min(h as f32 / ph);
            let (tw, th) = (((pw * k) as usize).max(1), ((ph * k) as usize).max(1));
            let bands: Vec<(f32, [f32; 3])> = false_colors(r.transfer)
                .into_iter()
                .map(|(v, c, _)| (v, rgb_of(c).map(|x| x as f32 / 255.0)))
                .collect();
            let data = pic.data();
            let (iw, ih) = (pic.width() as usize, pic.height() as usize);
            picture(tw, th, |i| {
                let (x, y) = (i % tw, i / tw);
                let sx = ((x as f32 + 0.5) / tw as f32 * iw as f32) as usize;
                let sy = ((y as f32 + 0.5) / th as f32 * ih as f32) as usize;
                let o = (sy.min(ih - 1) * iw + sx.min(iw - 1)) * 4;
                let l = ycbcr(
                    data[o] as f32 / 255.0,
                    data[o + 1] as f32 / 255.0,
                    data[o + 2] as f32 / 255.0,
                )
                .0;
                bands
                    .iter()
                    .find(|(v, _)| l < *v)
                    .map_or([1.0, 0.0, 0.0], |(_, c)| *c)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(r: u8, g: u8, b: u8) -> Pixmap {
        let mut pm = Pixmap::new(64, 36).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
        pm
    }

    fn req(kind: ScopeKind) -> ScopeRequest {
        ScopeRequest {
            kind,
            w: 200,
            h: 100,
            intensity: 100,
            color: [255, 255, 255],
            transfer: Transfer::Sdr,
            wide: false,
        }
    }

    /// Some pixel of row `y` between `x0` and `x1` is lit.
    fn row_lit(pm: &Pixmap, x0: u32, x1: u32, y: u32) -> bool {
        (x0..x1).any(|x| lit(pm, x, y))
    }

    fn lit(pm: &Pixmap, x: u32, y: u32) -> bool {
        let o = ((y * pm.width() + x) * 4) as usize;
        let d = pm.data();
        d[o] as u32 + d[o + 1] as u32 + d[o + 2] as u32 > 60
    }

    #[test]
    fn a_white_picture_is_a_line_at_the_top_of_the_waveform() {
        let t = trace(&flat(255, 255, 255), &req(ScopeKind::Waveform)).unwrap();
        assert!(row_lit(&t, 95, 105, 0));
        assert!(!row_lit(&t, 0, 200, 50));
        let t = trace(&flat(0, 0, 0), &req(ScopeKind::Waveform)).unwrap();
        assert!(row_lit(&t, 95, 105, 99));
        assert!(!row_lit(&t, 0, 200, 50));
    }

    #[test]
    fn red_is_high_in_the_first_panel_of_the_parade_only() {
        let t = trace(&flat(255, 0, 0), &req(ScopeKind::Parade)).unwrap();
        let p = panels([0.0, 0.0, 200.0, 100.0]);
        let mid = |i: usize| (p[i][0] + p[i][2] / 2.0) as u32;
        assert!(row_lit(&t, mid(0) - 4, mid(0) + 4, 0));
        assert!(!row_lit(&t, mid(0) - 4, mid(0) + 4, 99));
        assert!(row_lit(&t, mid(1) - 4, mid(1) + 4, 99));
        assert!(row_lit(&t, mid(2) - 4, mid(2) + 4, 99));
    }

    #[test]
    fn grey_sits_in_the_middle_of_the_vectorscope_and_red_on_its_target() {
        let mut r = req(ScopeKind::Vector);
        r.w = 100;
        r.h = 100;
        let t = trace(&flat(128, 128, 128), &r).unwrap();
        assert!(lit(&t, 50, 50));
        let t = trace(&flat(255, 0, 0), &r).unwrap();
        let (_, cb, cr) = ycbcr(1.0, 0.0, 0.0);
        let p = vector_point(cb, cr, [0.0, 0.0, 100.0, 100.0]);
        assert!(lit(&t, p[0] as u32, p[1] as u32));
        assert!(!lit(&t, 50, 50));
    }

    #[test]
    fn white_is_d65_on_the_cie_diagram() {
        let mut r = req(ScopeKind::Cie);
        r.w = 200;
        r.h = 200;
        let t = trace(&flat(255, 255, 255), &r).unwrap();
        let p = cie_point(0.3127, 0.3290, [0.0, 0.0, 200.0, 200.0]);
        assert!(lit(&t, p[0] as u32, p[1] as u32));
    }

    #[test]
    fn false_colour_keeps_the_picture_shape_and_flags_clipped_white() {
        let t = trace(&flat(255, 255, 255), &req(ScopeKind::FalseColor)).unwrap();
        // 64×36 fitted in 200×100: 177×100.
        assert_eq!((t.width(), t.height()), (177, 100));
        let d = t.data();
        assert_eq!(&d[0..3], &rgb_of("#ff2a2a")[..]);
    }

    #[test]
    fn every_kind_makes_a_trace() {
        let pic = flat(30, 140, 220);
        for kind in [ScopeKind::Histogram, ScopeKind::YcbcrParade] {
            assert!(trace(&pic, &req(kind)).is_some());
        }
    }
}

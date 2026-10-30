//! RGBA → Y'CbCr conversions for baseband outputs.

/// Narrow-range (video levels) BT.709 conversion of one premultiplied pixel.
/// Returns 10-bit (Y, Cb, Cr). Premultiplied RGB is what a keyer expects for a
/// "shaped" fill, which is the default for 2110 / SDI key+fill pairs.
#[inline]
pub fn rgb_to_ycbcr709_10(r: u8, g: u8, b: u8) -> (u16, u16, u16) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let cb = (b - y) / 1.8556;
    let cr = (r - y) / 1.5748;
    let y10 = (64.0 + y * 876.0).round().clamp(64.0, 940.0) as u16;
    let cb10 = (512.0 + cb * 896.0).round().clamp(64.0, 960.0) as u16;
    let cr10 = (512.0 + cr * 896.0).round().clamp(64.0, 960.0) as u16;
    (y10, cb10, cr10)
}

/// Key signal: alpha mapped to luma, neutral chroma.
#[inline]
pub fn alpha_to_key10(a: u8) -> (u16, u16, u16) {
    let y = (64.0 + (a as f32 / 255.0) * 876.0).round() as u16;
    (y, 512, 512)
}

// Fixed-point (Q16) BT.709 narrow-range coefficients on 8-bit input,
// pre-scaled to 10-bit output: Y in 64..940, Cb/Cr centred on 512.
const fn q(v: f64) -> i32 {
    (v * 65536.0 + if v >= 0.0 { 0.5 } else { -0.5 }) as i32
}
const YS: f64 = 876.0 / 255.0;
const CS: f64 = 896.0 / 255.0;
const KY: [i32; 3] = [q(0.2126 * YS), q(0.7152 * YS), q(0.0722 * YS)];
const KCB: [i32; 3] = [
    q(-0.2126 / 1.8556 * CS),
    q(-0.7152 / 1.8556 * CS),
    q(0.5 * CS),
];
const KCR: [i32; 3] = [
    q(0.5 * CS),
    q(-0.7152 / 1.5748 * CS),
    q(-0.0722 / 1.5748 * CS),
];

#[inline(always)]
fn dot(k: [i32; 3], r: i32, g: i32, b: i32) -> i32 {
    k[0] * r + k[1] * g + k[2] * b
}

/// Packs one line of RGBA into RFC 4175 YCbCr-4:2:2 10-bit pgroups
/// (5 bytes per 2 pixels: Cb Y0 Cr Y1, big-endian bit stream).
/// Integer-only hot path (see `examples/bench_packetize.rs`).
pub fn pack_line_422_10(rgba: &[u8], out: &mut Vec<u8>, key: bool) {
    out.clear();
    out.reserve(rgba.len() / 8 * 5 + 5);
    const HALF: i32 = 1 << 15;
    for pair in rgba.chunks(8) {
        let p1 = if pair.len() == 8 {
            &pair[4..8]
        } else {
            &pair[0..4]
        };
        let (y0, y1, cb, cr) = if key {
            let k = |a: u8| 64 + ((a as i32 * q(YS) + HALF) >> 16);
            (k(pair[3]), k(p1[3]), 512, 512)
        } else {
            let (r0, g0, b0) = (pair[0] as i32, pair[1] as i32, pair[2] as i32);
            let (r1, g1, b1) = (p1[0] as i32, p1[1] as i32, p1[2] as i32);
            let y0 = 64 + ((dot(KY, r0, g0, b0) + HALF) >> 16);
            let y1 = 64 + ((dot(KY, r1, g1, b1) + HALF) >> 16);
            // Chroma of the averaged pair (co-sited filtering kept simple).
            let (r, g, b) = (r0 + r1, g0 + g1, b0 + b1);
            let cb = 512 + ((dot(KCB, r, g, b) + (1 << 16)) >> 17);
            let cr = 512 + ((dot(KCR, r, g, b) + (1 << 16)) >> 17);
            (y0, y1, cb, cr)
        };
        let c = |v: i32, lo: i32, hi: i32| v.clamp(lo, hi) as u64;
        let bits: u64 = (c(cb, 64, 960) << 30)
            | (c(y0, 64, 940) << 20)
            | (c(cr, 64, 960) << 10)
            | c(y1, 64, 940);
        out.extend_from_slice(&bits.to_be_bytes()[3..8]);
    }
}

/// Packs one line of premultiplied RGBA into 8-bit RGBA pgroups (4 bytes
/// per pixel), for receivers that take the picture and its alpha in one
/// stream.
pub fn pack_line_rgba8(rgba: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.extend_from_slice(rgba);
}

/// Packs one line of RGBA into YCbCr + alpha 4:2:2:4 10-bit pgroups of 4
/// pixels (15 bytes: Cb0 Y0 A0 Cr0 Y1 A1 Cb1 Y2 A2 Cr1 Y3 A3, big-endian
/// bit stream). Fill is shaped (premultiplied), like the fill + key pair.
pub fn pack_line_4224_10(rgba: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(rgba.len() / 16 * 15 + 15);
    let px = |i: usize| -> [u8; 4] {
        let n = rgba.len() / 4;
        let i = i.min(n.saturating_sub(1)) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    let n = rgba.len() / 4;
    let mut i = 0;
    while i < n {
        let p: [[u8; 4]; 4] = [px(i), px(i + 1), px(i + 2), px(i + 3)];
        let ycc: Vec<(u16, u16, u16)> = p
            .iter()
            .map(|q| rgb_to_ycbcr709_10(q[0], q[1], q[2]))
            .collect();
        let a = |q: [u8; 4]| alpha_to_key10(q[3]).0 as u128;
        let avg = |x: u16, y: u16| (x as u32 + y as u32).div_ceil(2) as u128;
        let samples: [u128; 12] = [
            avg(ycc[0].1, ycc[1].1),
            ycc[0].0 as u128,
            a(p[0]),
            avg(ycc[0].2, ycc[1].2),
            ycc[1].0 as u128,
            a(p[1]),
            avg(ycc[2].1, ycc[3].1),
            ycc[2].0 as u128,
            a(p[2]),
            avg(ycc[2].2, ycc[3].2),
            ycc[3].0 as u128,
            a(p[3]),
        ];
        let bits = samples
            .iter()
            .fold(0u128, |acc, v| (acc << 10) | (v & 0x3ff));
        out.extend_from_slice(&bits.to_be_bytes()[1..16]);
        i += 4;
    }
}

// Q16 BT.709 narrow-range Y'CbCr (8-bit) → RGB coefficients.
const KR: f64 = 0.2126;
const KB: f64 = 0.0722;
const KG: f64 = 1.0 - KR - KB;
const Y8: i32 = q(255.0 / 219.0);
const CR_R: i32 = q(255.0 / 224.0 * 2.0 * (1.0 - KR));
const CB_G: i32 = q(255.0 / 224.0 * 2.0 * (1.0 - KB) * KB / KG);
const CR_G: i32 = q(255.0 / 224.0 * 2.0 * (1.0 - KR) * KR / KG);
const CB_B: i32 = q(255.0 / 224.0 * 2.0 * (1.0 - KB));

/// UYVY (8-bit 4:2:2, Cb Y0 Cr Y1, what SDI capture delivers) → opaque
/// RGBA, BT.709 narrow range. `uyvy` holds `height` rows of `width * 2`
/// bytes; each pixel pair shares its chroma.
pub fn uyvy_to_rgba709(uyvy: &[u8], width: usize, height: usize, out: &mut Vec<u8>) {
    out.clear();
    out.resize(width * height * 4, 255);
    let line = width * 2;
    if width == 0 || uyvy.len() < line * height {
        // Not a whole picture: opaque black.
        out.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[..3].fill(0));
        return;
    }
    rows_to_rgba709(uyvy, width, out);
}

/// Whole UYVY rows of `width` pixels into the matching RGBA rows of `out`
/// (alpha left as it is).
fn rows_to_rgba709(uyvy: &[u8], width: usize, out: &mut [u8]) {
    let line = width * 2;
    const HALF: i32 = 1 << 15;
    let px = |y: u8, cb: i32, cr: i32, dst: &mut [u8]| {
        let yy = (y as i32 - 16) * Y8 + HALF;
        dst[0] = ((yy + CR_R * cr) >> 16).clamp(0, 255) as u8;
        dst[1] = ((yy - CB_G * cb - CR_G * cr) >> 16).clamp(0, 255) as u8;
        dst[2] = ((yy + CB_B * cb) >> 16).clamp(0, 255) as u8;
    };
    for (src, dst) in uyvy.chunks_exact(line).zip(out.chunks_exact_mut(width * 4)) {
        for (i, s) in src.as_chunks::<4>().0.iter().enumerate() {
            let (cb, cr) = (s[0] as i32 - 128, s[2] as i32 - 128);
            px(s[1], cb, cr, &mut dst[i * 8..i * 8 + 4]);
            if i * 2 + 1 < width {
                px(s[3], cb, cr, &mut dst[i * 8 + 4..i * 8 + 8]);
            }
        }
    }
}

/// [`uyvy_to_rgba709`] split over a few threads (a 1080 picture in a couple
/// of milliseconds instead of a dozen): the live input is under the
/// graphics sooner.
pub fn uyvy_to_rgba709_par(uyvy: &[u8], width: usize, height: usize, out: &mut Vec<u8>) {
    let line = width * 2;
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 4);
    if threads == 1 || width == 0 || height < 64 || uyvy.len() < line * height {
        return uyvy_to_rgba709(uyvy, width, height, out);
    }
    out.clear();
    out.resize(width * height * 4, 255);
    let rows = height.div_ceil(threads);
    std::thread::scope(|s| {
        for (src, dst) in uyvy[..line * height]
            .chunks(rows * line)
            .zip(out.chunks_mut(rows * width * 4))
        {
            s.spawn(move || rows_to_rgba709(src, width, dst));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ycbcra_pgroup() {
        let mut out = Vec::new();
        pack_line_4224_10(&[255; 16], &mut out);
        assert_eq!(out.len(), 15);
        let mut b = [0u8; 16];
        b[1..].copy_from_slice(&out);
        let v = u128::from_be_bytes(b);
        let s = |i: u32| ((v >> (110 - i * 10)) & 0x3ff) as u16;
        assert_eq!((s(0), s(1), s(2)), (512, 940, 940));
        assert_eq!(s(11), 940);
    }

    #[test]
    fn levels() {
        assert_eq!(rgb_to_ycbcr709_10(0, 0, 0), (64, 512, 512));
        assert_eq!(rgb_to_ycbcr709_10(255, 255, 255), (940, 512, 512));
    }

    #[test]
    fn integer_path_matches_float_reference() {
        let mut out = Vec::new();
        for &(r, g, b) in &[
            (0u8, 0u8, 0u8),
            (255, 255, 255),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (12, 200, 99),
        ] {
            pack_line_422_10(&[r, g, b, 255, r, g, b, 255], &mut out, false);
            let v = u64::from_be_bytes([0, 0, 0, out[0], out[1], out[2], out[3], out[4]]);
            let (y, cb, cr) = rgb_to_ycbcr709_10(r, g, b);
            for (got, want) in [
                ((v >> 20) & 0x3ff, y),
                ((v >> 30) & 0x3ff, cb),
                ((v >> 10) & 0x3ff, cr),
            ] {
                assert!(
                    (got as i32 - want as i32).abs() <= 1,
                    "{r},{g},{b}: {got} vs {want}"
                );
            }
        }
    }

    #[test]
    fn pgroup_layout() {
        let mut out = Vec::new();
        pack_line_422_10(&[255; 8], &mut out, false);
        assert_eq!(out.len(), 5);
        // Cb=512 Y=940 Cr=512 Y=940
        let v = u64::from_be_bytes([0, 0, 0, out[0], out[1], out[2], out[3], out[4]]);
        assert_eq!((v >> 30) & 0x3ff, 512);
        assert_eq!((v >> 20) & 0x3ff, 940);
        assert_eq!((v >> 10) & 0x3ff, 512);
        assert_eq!(v & 0x3ff, 940);
    }

    #[test]
    fn uyvy_round_trip() {
        let mut out = Vec::new();
        // Black, white and grey at video levels.
        uyvy_to_rgba709(&[128, 16, 128, 235, 128, 126, 128, 126], 4, 1, &mut out);
        assert_eq!(&out[0..8], &[0, 0, 0, 255, 255, 255, 255, 255]);
        assert!(out[8..16].iter().all(|&v| v == 128 || v == 255), "{out:?}");
        // Colours through the 10-bit encoder (8-bit = 10-bit / 4).
        for &(r, g, b) in &[
            (255u8, 0u8, 0u8),
            (0, 255, 0),
            (0, 0, 255),
            (12, 200, 99),
            (240, 128, 30),
        ] {
            let (y, cb, cr) = rgb_to_ycbcr709_10(r, g, b);
            let e = |v: u16| ((v + 2) / 4) as u8;
            uyvy_to_rgba709(&[e(cb), e(y), e(cr), e(y)], 2, 1, &mut out);
            for (got, want) in out[0..3].iter().zip([r, g, b]) {
                assert!(
                    (*got as i32 - want as i32).abs() <= 2,
                    "{r},{g},{b}: {out:?}"
                );
            }
            assert_eq!(&out[0..4], &out[4..8]);
            assert_eq!(out[3], 255);
        }
        // Short input: opaque black of the right size, no panic.
        uyvy_to_rgba709(&[0; 3], 2, 2, &mut out);
        assert_eq!(out, [0, 0, 0, 255].repeat(4));
    }

    #[test]
    fn split_conversion_matches_the_plain_one() {
        let (w, h) = (96, 130);
        let uyvy: Vec<u8> = (0..w * h * 2).map(|i| (i * 37 % 251) as u8).collect();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        uyvy_to_rgba709(&uyvy, w, h, &mut a);
        uyvy_to_rgba709_par(&uyvy, w, h, &mut b);
        assert_eq!(a, b);
    }
}

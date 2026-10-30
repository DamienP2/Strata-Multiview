//! Drop shadow of a layer (`Layer::drop_shadow`), as the HTML5 renderer's
//! CSS `drop-shadow()` on the layer's box: the alpha of what the layer drew
//! (with its children), blurred (Gaussian, σ = blur / 2), in the shadow's
//! colour, offset, under the layer. Offset and blur are in the layer's space.

use cg_core::Color;
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

/// The box of the pixels of `pm` that are not transparent: (x0, y0, x1, y1), x1 / y1 excluded.
pub fn opaque_box(pm: &Pixmap) -> Option<(i32, i32, i32, i32)> {
    let w = pm.width() as usize;
    // The alpha bytes of two pixels in a word.
    const ALPHA: u64 = u64::from_ne_bytes([0, 0, 0, 0xff, 0, 0, 0, 0xff]);
    let clear = |row: &[u8]| {
        let (words, rest) = row.as_chunks::<64>();
        words.iter().all(|c| {
            c.as_chunks::<8>()
                .0
                .iter()
                .fold(0, |acc, b| acc | u64::from_ne_bytes(*b))
                & ALPHA
                == 0
        }) && rest.as_chunks::<4>().0.iter().all(|p| p[3] == 0)
    };
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (y, row) in pm.data().chunks_exact(w * 4).enumerate() {
        if clear(row) {
            continue;
        }
        let px = row.as_chunks::<4>().0;
        if let (Some(first), Some(last)) = (
            px.iter().position(|p| p[3] != 0),
            px.iter().rposition(|p| p[3] != 0),
        ) {
            x0 = x0.min(first);
            x1 = x1.max(last + 1);
            y0 = y0.min(y);
            y1 = y + 1;
        }
    }
    (x0 != usize::MAX).then_some((x0 as i32, y0 as i32, x1 as i32, y1 as i32))
}

/// Box sizes whose three passes approximate a Gaussian of `sigma`.
fn boxes(sigma: f32) -> [usize; 3] {
    let n = 3.0;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n)
        / (-4.0 * wl as f32 - 4.0))
        .round() as i32;
    std::array::from_fn(|i| {
        if (i as i32) < m {
            wl as usize
        } else {
            wu as usize
        }
    })
}

/// One box pass along the rows of a `w` × `h` buffer; outside is 0.
fn box_rows(src: &[f32], dst: &mut [f32], w: usize, radius: usize) {
    let div = (2 * radius + 1) as f32;
    for (s, d) in src.chunks_exact(w).zip(dst.chunks_exact_mut(w)) {
        let mut sum = 0.0;
        for v in &s[..radius.min(w)] {
            sum += v;
        }
        for i in 0..w {
            if i + radius < w {
                sum += s[i + radius];
            }
            d[i] = sum / div;
            if i >= radius {
                sum -= s[i - radius];
            }
        }
    }
}

/// The same along the columns, every column at once (the same sums, in the
/// same order, as one column after the other).
fn box_columns(src: &[f32], dst: &mut [f32], w: usize, h: usize, radius: usize) {
    let div = (2 * radius + 1) as f32;
    let row = |y: usize| &src[y * w..(y + 1) * w];
    let mut sums = vec![0.0f32; w];
    for y in 0..radius.min(h) {
        for (s, v) in sums.iter_mut().zip(row(y)) {
            *s += v;
        }
    }
    for y in 0..h {
        if y + radius < h {
            for (s, v) in sums.iter_mut().zip(row(y + radius)) {
                *s += v;
            }
        }
        for (d, s) in dst[y * w..(y + 1) * w].iter_mut().zip(&sums) {
            *d = s / div;
        }
        if y >= radius {
            for (s, v) in sums.iter_mut().zip(row(y - radius)) {
                *s -= v;
            }
        }
    }
}

/// Gaussian blur (three box passes each way) of a `w` × `h` alpha buffer.
fn blur(alpha: &mut [f32], w: usize, h: usize, sigma: f32) {
    if sigma < 0.3 {
        return;
    }
    let mut tmp = vec![0.0; alpha.len()];
    for size in boxes(sigma) {
        let r = (size - 1) / 2;
        box_rows(alpha, &mut tmp, w, r);
        box_columns(&tmp, alpha, w, h, r);
    }
}

/// `v.round() as u8` for `v` in 0..=255 (or NaN), without the library call:
/// the fraction left by truncating is exact, so is its comparison to ½.
#[inline]
fn round_u8(v: f32) -> u8 {
    let t = v as u32;
    (t + u32::from(v - t as f32 >= 0.5)).min(255) as u8
}

/// The layer drawn alone (`content`: frame-sized, premultiplied) over its shadow, offset by
/// (`dx`, `dy`) device pixels and blurred by `sigma` device pixels: the picture of the box both
/// cover (inside the frame) and where it goes. None when the layer drew nothing.
pub fn compose(
    content: &Pixmap,
    color: Color,
    dx: f32,
    dy: f32,
    sigma: f32,
) -> Option<(Pixmap, i32, i32)> {
    let (x0, y0, x1, y1) = opaque_box(content)?;
    let [cr, cg, cb, ca] = color.0.map(|c| c.clamp(0.0, 1.0));
    let pad = (sigma * 3.0).ceil() as i32 + 1;
    let (bx, by) = (x0 - pad, y0 - pad);
    let (bw, bh) = ((x1 - x0 + 2 * pad) as usize, (y1 - y0 + 2 * pad) as usize);
    let (cw, ch) = (content.width() as i32, content.height() as i32);
    // The box of the layer and of its shadow, inside the frame.
    let (ux0, uy0) = (
        x0.min((bx as f32 + dx).floor() as i32).max(0),
        y0.min((by as f32 + dy).floor() as i32).max(0),
    );
    let (ux1, uy1) = (
        x1.max((bx as f32 + dx).ceil() as i32 + bw as i32).min(cw),
        y1.max((by as f32 + dy).ceil() as i32 + bh as i32).min(ch),
    );
    let mut out = Pixmap::new((ux1 - ux0).max(1) as u32, (uy1 - uy0).max(1) as u32)?;
    if ca > 0.0 {
        let mut alpha = vec![0.0f32; bw * bh];
        let data = content.data();
        for (y, row) in alpha.chunks_exact_mut(bw).enumerate() {
            let sy = by + y as i32;
            if sy < 0 || sy >= ch {
                continue;
            }
            for (x, a) in row.iter_mut().enumerate() {
                let sx = bx + x as i32;
                if (0..cw).contains(&sx) {
                    *a = data[((sy * cw + sx) * 4 + 3) as usize] as f32 / 255.0;
                }
            }
        }
        blur(&mut alpha, bw, bh, sigma);
        let mut sh = Pixmap::new(bw as u32, bh as u32)?;
        for (px, a) in sh.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(&alpha) {
            let a = (a * ca).clamp(0.0, 1.0);
            px[0] = round_u8(cr * a * 255.0);
            px[1] = round_u8(cg * a * 255.0);
            px[2] = round_u8(cb * a * 255.0);
            px[3] = round_u8(a * 255.0);
        }
        let (fx, fy) = ((bx - ux0) as f32 + dx, (by - uy0) as f32 + dy);
        let paint = PixmapPaint {
            quality: if fx.fract() == 0.0 && fy.fract() == 0.0 {
                FilterQuality::Nearest
            } else {
                FilterQuality::Bilinear
            },
            ..PixmapPaint::default()
        };
        out.draw_pixmap(
            0,
            0,
            sh.as_ref(),
            &paint,
            Transform::from_translate(fx, fy),
            None,
        );
    }
    out.draw_pixmap(
        -ux0,
        -uy0,
        content.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Some((out, ux0, uy0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The box pass as it was written first (one line after the other).
    fn box_pass(
        src: &[f32],
        dst: &mut [f32],
        len: usize,
        lines: usize,
        step: usize,
        line_step: usize,
        radius: usize,
    ) {
        let div = (2 * radius + 1) as f32;
        for l in 0..lines {
            let at = |i: usize| l * line_step + i * step;
            let mut sum = 0.0;
            for i in 0..radius.min(len) {
                sum += src[at(i)];
            }
            for i in 0..len {
                if i + radius < len {
                    sum += src[at(i + radius)];
                }
                dst[at(i)] = sum / div;
                if i >= radius {
                    sum -= src[at(i - radius)];
                }
            }
        }
    }

    #[test]
    fn the_passes_sum_as_one_line_after_the_other() {
        let (w, h) = (37, 23);
        let src: Vec<f32> = (0..w * h)
            .map(|i| ((i * 7919) % 255) as f32 / 255.0)
            .collect();
        for r in [0, 1, 4, 30] {
            let (mut a, mut b) = (vec![0.0; w * h], vec![0.0; w * h]);
            box_pass(&src, &mut a, w, h, 1, w, r);
            box_rows(&src, &mut b, w, r);
            assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
            box_pass(&src, &mut a, h, w, w, 1, r);
            box_columns(&src, &mut b, w, h, r);
            assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
        }
    }

    #[test]
    fn rounding_is_the_library_one() {
        let mut v = 0.0f32;
        while v <= 255.0 {
            assert_eq!(round_u8(v), v.round() as u8, "{v}");
            v = f32::from_bits(v.to_bits() + 997);
        }
        for v in [0.5, 1.5, 254.5, 255.0, 0.49999997, 2.4999998, f32::NAN] {
            assert_eq!(round_u8(v), v.round() as u8, "{v}");
        }
    }

    #[test]
    fn the_opaque_box_is_found() {
        let mut pm = Pixmap::new(70, 9).unwrap();
        assert_eq!(opaque_box(&pm), None);
        for (x, y) in [(3, 2), (66, 5), (17, 7)] {
            let i = (y * 70 + x) * 4;
            pm.data_mut()[i + 3] = 1;
        }
        assert_eq!(opaque_box(&pm), Some((3, 2, 67, 8)));
    }

    #[test]
    fn three_boxes_make_the_gaussian_variance() {
        for sigma in [2.5f32, 6.0, 20.0] {
            // A box of size n has variance (n² − 1) / 12.
            let v: f32 = boxes(sigma)
                .iter()
                .map(|&n| ((n * n) as f32 - 1.0) / 12.0)
                .sum();
            assert!(
                (v - sigma * sigma).abs() / (sigma * sigma) < 0.2,
                "{sigma}: {v}"
            );
        }
    }

    #[test]
    fn a_blur_keeps_the_amount_of_alpha() {
        let (w, h) = (40, 40);
        let mut a = vec![0.0; w * h];
        for y in 15..25 {
            for x in 15..25 {
                a[y * w + x] = 1.0;
            }
        }
        blur(&mut a, w, h, 3.0);
        let total: f32 = a.iter().sum();
        assert!((total - 100.0).abs() < 1.0, "{total}");
        // Spread out, symmetric.
        assert!(a[20 * w + 20] < 1.0 && a[20 * w + 20] > 0.5);
        assert!((a[20 * w + 12] - a[20 * w + 27]).abs() < 1e-3);
    }
}

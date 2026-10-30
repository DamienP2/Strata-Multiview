//! Pictures kept scaled to the size they are drawn at, and the integer
//! compositing of kept pictures.
//!
//! A picture is drawn through its transform once, into a picture the size
//! of the part of the frame it can cover ([`Scaled::draw`]); that one is then
//! laid on the frame pixel for pixel ([`composite`]), at the layer's
//! opacity, through its clip. Next frames lay the same scaled picture again
//! while the picture, its size on the frame and its sub-pixel position do not
//! change: no filtering per frame (a bicubic upscale of a small logo to a
//! large box costs tens of ms per frame).
//!
//! The picture is drawn this way whether it is kept or not, so a frame is
//! the same, pixel for pixel, whatever was drawn before it (the frames kept
//! by `cache` are the frames drawn whole).

use std::collections::HashMap;

use tiny_skia::{FilterQuality, IntRect, Mask, Pixmap, PixmapPaint, Rect, Transform};

/// Frames a scaled picture is kept unused.
const KEEP_FRAMES: u64 = 250;
/// Bytes of scaled pictures kept at most (the oldest are dropped first).
const MAX_BYTES: usize = 192 << 20;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    /// The picture (source, colouring).
    src: String,
    quality: u8,
    /// Size of the scaled picture.
    size: (u32, u32),
    /// Picture → scaled picture (bits of the f32 terms).
    tf: [u32; 6],
}

struct Entry {
    pm: Pixmap,
    used: u64,
}

/// The scaled pictures of a renderer.
#[derive(Default)]
pub(crate) struct Scaled {
    map: HashMap<Key, Entry>,
    bytes: usize,
    tick: u64,
    /// Scaled pictures drawn and found (tests, measures).
    pub made: u64,
    pub found: u64,
}

fn quality_id(q: FilterQuality) -> u8 {
    match q {
        FilterQuality::Nearest => 0,
        FilterQuality::Bilinear => 1,
        FilterQuality::Bicubic => 2,
    }
}

/// Pixels of the `w` × `h` frame `rect` (through `tf`) can touch, rounded out.
pub(crate) fn device_bounds(rect: Rect, tf: Transform, w: u32, h: u32) -> Option<IntRect> {
    let b = tiny_skia::PathBuilder::from_rect(rect)
        .transform(tf)?
        .bounds();
    let x0 = b.left().floor().max(0.0);
    let y0 = b.top().floor().max(0.0);
    let x1 = b.right().ceil().min(w as f32);
    let y1 = b.bottom().ceil().min(h as f32);
    if !(x1 > x0 && y1 > y0) {
        return None;
    }
    IntRect::from_ltrb(x0 as i32, y0 as i32, x1 as i32, y1 as i32)
}

impl Scaled {
    /// A frame begins: pictures unused for a while are dropped.
    pub fn frame(&mut self) {
        self.tick += 1;
        let tick = self.tick;
        if self.map.is_empty() {
            return;
        }
        let mut bytes = self.bytes;
        self.map.retain(|_, e| {
            let keep = tick - e.used <= KEEP_FRAMES;
            if !keep {
                bytes -= e.pm.data().len();
            }
            keep
        });
        self.bytes = bytes;
    }

    /// Forgets everything (pictures read again).
    pub fn clear(&mut self) {
        self.map.clear();
        self.bytes = 0;
    }

    /// Forgets the scaled copies of a picture (reloaded).
    pub fn forget(&mut self, src: &str) {
        let mut bytes = self.bytes;
        self.map.retain(|k, e| {
            let keep = !(k.src == src || k.src.starts_with(&format!("{src}|")));
            if !keep {
                bytes -= e.pm.data().len();
            }
            keep
        });
        self.bytes = bytes;
    }

    /// The picture `src` drawn by `make` (on a transparent `size` picture,
    /// through `tf`), given to `with`: kept for the next frames when `keep`
    /// (drawn again when not kept, or for another `tf` or size).
    pub fn with_picture(
        &mut self,
        src: &str,
        tf: Transform,
        size: (u32, u32),
        keep: bool,
        make: impl FnOnce(&mut Pixmap),
        with: impl FnOnce(&Pixmap),
    ) {
        let key = Key {
            src: src.to_string(),
            quality: 255,
            size,
            tf: [tf.sx, tf.ky, tf.kx, tf.sy, tf.tx, tf.ty].map(f32::to_bits),
        };
        let tick = self.tick;
        if let Some(e) = self.map.get_mut(&key) {
            e.used = tick;
            self.found += 1;
            return with(&e.pm);
        }
        let Some(mut pm) = Pixmap::new(size.0, size.1) else {
            return;
        };
        make(&mut pm);
        self.made += 1;
        with(&pm);
        if keep {
            self.insert(key, pm);
        }
    }

    fn insert(&mut self, key: Key, pm: Pixmap) {
        self.bytes += pm.data().len();
        self.map.insert(
            key,
            Entry {
                pm,
                used: self.tick,
            },
        );
        // Over budget: the least recently used go.
        while self.bytes > MAX_BYTES && self.map.len() > 1 {
            let Some(old) = self
                .map
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(e) = self.map.remove(&old) {
                self.bytes -= e.pm.data().len();
            }
        }
    }

    /// Draws `img` (named `src`) through `img_tf` on `pm`, at `opacity`, cut
    /// by `mask`, in the region `clip` (frame pixels) only: scaled once with
    /// `quality` into a picture of that region, then laid on the frame.
    /// `keep`: the scaled picture is kept for the next frames (else it is
    /// only drawn: a picture moving or zooming changes every frame).
    /// False when the transform turns or skews the picture (not handled
    /// here: the caller draws it directly).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        pm: &mut Pixmap,
        src: &str,
        img: &Pixmap,
        img_tf: Transform,
        quality: FilterQuality,
        opacity: f32,
        mask: Option<&Mask>,
        clip: Option<IntRect>,
        keep: bool,
    ) -> bool {
        if img_tf.kx != 0.0 || img_tf.ky != 0.0 || img_tf.sx <= 0.0 || img_tf.sy <= 0.0 {
            return false;
        }
        if opacity <= 0.0 {
            return true;
        }
        let Some(img_rect) = Rect::from_xywh(0.0, 0.0, img.width() as f32, img.height() as f32)
        else {
            return true;
        };
        let Some(mut region) = device_bounds(img_rect, img_tf, pm.width(), pm.height()) else {
            return true;
        };
        if let Some(c) = clip {
            match region.intersect(&c) {
                Some(r) => region = r,
                None => return true,
            }
        }
        let (x0, y0) = (region.x(), region.y());
        let tf = Transform::from_translate(-(x0 as f32), -(y0 as f32)).pre_concat(img_tf);
        let key = Key {
            src: src.to_string(),
            quality: quality_id(quality),
            size: (region.width(), region.height()),
            tf: [tf.sx, tf.ky, tf.kx, tf.sy, tf.tx, tf.ty].map(f32::to_bits),
        };
        let tick = self.tick;
        if let Some(e) = self.map.get_mut(&key) {
            e.used = tick;
            self.found += 1;
            composite(pm, &e.pm, x0, y0, opacity, mask);
            return true;
        }
        let Some(mut scaled) = Pixmap::new(region.width(), region.height()) else {
            return true;
        };
        let paint = PixmapPaint {
            quality,
            ..Default::default()
        };
        scaled.draw_pixmap(0, 0, img.as_ref(), &paint, tf, None);
        self.made += 1;
        composite(pm, &scaled, x0, y0, opacity, mask);
        if keep {
            self.insert(key, scaled);
        }
        true
    }
}

/// Pieces of a picture: (x, y, picture) of its blocks that are not
/// transparent.
pub(crate) type Tiles = Vec<(i32, i32, Pixmap)>;

/// All zeroes (by blocks of 32 bytes: compared as wide words).
fn clear(b: &[u8]) -> bool {
    let (c, rest) = b.as_chunks::<32>();
    c.iter().all(|x| *x == [0u8; 32]) && rest.iter().all(|x| *x == 0)
}

/// Side of the blocks of [`tiles`].
const TILE: usize = 64;

/// The blocks of `pm` that are not all transparent (graphics scattered over
/// the frame: a bug in a corner, a clock in another), each cleared in `pm`
/// once copied. Laid back with [`composite`], they make what `pm` laid
/// whole would (transparent pixels leave the frame as it is).
pub(crate) fn tiles(pm: &mut Pixmap) -> Tiles {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let mut out = Vec::new();
    for ty in (0..h).step_by(TILE) {
        let th = TILE.min(h - ty);
        for tx in (0..w).step_by(TILE) {
            let tw = TILE.min(w - tx);
            let data = pm.data_mut();
            let used =
                (ty..ty + th).any(|y| !clear(&data[(y * w + tx) * 4..(y * w + tx + tw) * 4]));
            if !used {
                continue;
            }
            let Some(mut t) = Pixmap::new(tw as u32, th as u32) else {
                continue;
            };
            for (r, y) in (ty..ty + th).enumerate() {
                let src = &mut data[(y * w + tx) * 4..(y * w + tx + tw) * 4];
                t.data_mut()[r * tw * 4..(r + 1) * tw * 4].copy_from_slice(src);
                src.fill(0);
            }
            out.push((tx as i32, ty as i32, t));
        }
    }
    out
}

/// a × b / 255, rounded.
#[inline(always)]
pub(crate) fn mul255(a: u32, b: u32) -> u32 {
    let p = a * b + 128;
    (p + (p >> 8)) >> 8
}

/// The premultiplied pixel `src` × `cov` (0..255) laid over `dst` (source over).
#[inline(always)]
pub(crate) fn over(dst: &mut [u8], src: [u32; 4], cov: u32) {
    let sc = src.map(|v| mul255(v, cov));
    let k = 255 - sc[3].min(255);
    for c in 0..4 {
        dst[c] = (sc[c] + mul255(dst[c] as u32, k)).min(255) as u8;
    }
}

/// One premultiplied pixel over another, at full coverage.
#[inline(always)]
fn over_one(dp: &mut [u8; 4], sp: &[u8; 4]) {
    let sa = sp[3] as u32;
    if sa == 255 {
        *dp = *sp;
    } else if *sp != [0, 0, 0, 0] {
        let k = 255 - sa;
        for c in 0..4 {
            dp[c] = (sp[c] as u32 + mul255(dp[c] as u32, k)).min(255) as u8;
        }
    }
}

/// A run of premultiplied pixels × `cov` laid over another (source over),
/// the arithmetic of [`over`], four pixels at a time on x86-64.
#[inline(always)]
fn over_run(d: &mut [[u8; 4]], s: &[[u8; 4]], cov: u32) {
    #[cfg(target_arch = "x86_64")]
    {
        let (dq, dr) = d.as_chunks_mut::<4>();
        let (sq, sr) = s.as_chunks::<4>();
        for (dp, sp) in dq.iter_mut().zip(sq) {
            // SAFETY: SSE2 is part of every x86-64; 16 bytes in, 16 out.
            unsafe { sse::over4(dp, sp, [cov; 4]) }
        }
        over_run_scalar(dr, sr, cov);
    }
    #[cfg(not(target_arch = "x86_64"))]
    over_run_scalar(d, s, cov);
}

#[inline(always)]
fn over_run_scalar(d: &mut [[u8; 4]], s: &[[u8; 4]], cov: u32) {
    for (dp, sp) in d.iter_mut().zip(s) {
        let sa = mul255(sp[3] as u32, cov);
        let k = 255 - sa;
        for c in 0..4 {
            let v = mul255(sp[c] as u32, cov) + mul255(dp[c] as u32, k);
            dp[c] = v.min(255) as u8;
        }
    }
}

/// The same, with a coverage per pixel (`cov` × the mask's byte).
#[inline(always)]
fn over_run_masked(d: &mut [[u8; 4]], s: &[[u8; 4]], m: &[u8], cov: u32) {
    #[cfg(target_arch = "x86_64")]
    {
        let covs = |m: &[u8; 4]| -> [u32; 4] {
            m.map(|mv| {
                if cov == 255 {
                    mv as u32
                } else {
                    mul255(mv as u32, cov)
                }
            })
        };
        let (dq, dr) = d.as_chunks_mut::<4>();
        let (sq, sr) = s.as_chunks::<4>();
        let (mq, mr) = m.as_chunks::<4>();
        for ((dp, sp), mp) in dq.iter_mut().zip(sq).zip(mq) {
            // SAFETY: SSE2 is part of every x86-64; 16 bytes in, 16 out.
            unsafe { sse::over4(dp, sp, covs(mp)) }
        }
        over_run_masked_scalar(dr, sr, mr, cov);
    }
    #[cfg(not(target_arch = "x86_64"))]
    over_run_masked_scalar(d, s, m, cov);
}

#[inline(always)]
fn over_run_masked_scalar(d: &mut [[u8; 4]], s: &[[u8; 4]], m: &[u8], cov: u32) {
    for ((dp, sp), mv) in d.iter_mut().zip(s).zip(m) {
        let cov = if cov == 255 {
            *mv as u32
        } else {
            mul255(*mv as u32, cov)
        };
        let sa = mul255(sp[3] as u32, cov);
        let k = 255 - sa;
        for c in 0..4 {
            let v = mul255(sp[c] as u32, cov) + mul255(dp[c] as u32, k);
            dp[c] = v.min(255) as u8;
        }
    }
}

/// `(a × (256 − w) + b × w) >> 8` per channel: `a` and `b` mixed, `w` the
/// weight of `b` in 1/256.
#[inline(always)]
fn mix_rows(out: &mut [[u16; 4]], a: &[[u8; 4]], b: &[[u8; 4]], w: u32) {
    #[cfg(target_arch = "x86_64")]
    {
        let (oq, or) = out.as_chunks_mut::<4>();
        let (aq, ar) = a.as_chunks::<4>();
        let (bq, br) = b.as_chunks::<4>();
        for ((o, a), b) in oq.iter_mut().zip(aq).zip(bq) {
            // SAFETY: SSE2 is part of every x86-64; 16 bytes in, 32 out.
            unsafe { sse::mix4(o, a, b, w as u16) }
        }
        mix_rows_scalar(or, ar, br, w);
    }
    #[cfg(not(target_arch = "x86_64"))]
    mix_rows_scalar(out, a, b, w);
}

#[inline(always)]
fn mix_rows_scalar(out: &mut [[u16; 4]], a: &[[u8; 4]], b: &[[u8; 4]], w: u32) {
    for ((o, a), b) in out.iter_mut().zip(a).zip(b) {
        for c in 0..4 {
            o[c] = ((a[c] as u32 * (256 - w) + b[c] as u32 * w) >> 8) as u16;
        }
    }
}

/// `(l × (256 − w) + r × w) >> 8` per channel, `l` and `r` neighbouring
/// entries of `mixed` (`out[i]` from `mixed[i]` and `mixed[i + 1]`).
#[inline(always)]
fn mix_columns(out: &mut [[u8; 4]], mixed: &[[u16; 4]], w: u32) {
    #[cfg(target_arch = "x86_64")]
    {
        let (oq, or) = out.as_chunks_mut::<4>();
        let n = oq.len() * 4;
        for (i, o) in oq.iter_mut().enumerate() {
            // SAFETY: SSE2 is part of every x86-64; mixed has out.len() + 1 entries.
            unsafe { sse::columns4(o, &mixed[i * 4..i * 4 + 5], w as u16) }
        }
        mix_columns_scalar(or, &mixed[n..], w);
    }
    #[cfg(not(target_arch = "x86_64"))]
    mix_columns_scalar(out, mixed, w);
}

#[inline(always)]
fn mix_columns_scalar(out: &mut [[u8; 4]], mixed: &[[u16; 4]], w: u32) {
    for (px, pair) in out.iter_mut().zip(mixed.windows(2)) {
        let (l, r) = (pair[0], pair[1]);
        for c in 0..4 {
            px[c] = ((l[c] as u32 * (256 - w) + r[c] as u32 * w) >> 8) as u8;
        }
    }
}

/// The same arithmetic, on 16-bit lanes (SSE2, part of every x86-64).
#[cfg(target_arch = "x86_64")]
mod sse {
    use std::arch::x86_64::*;

    /// `a × b / 255` rounded, as [`super::mul255`], on u16 lanes (a, b ≤ 255).
    #[inline(always)]
    unsafe fn mul255(a: __m128i, b: __m128i) -> __m128i {
        let p = _mm_add_epi16(_mm_mullo_epi16(a, b), _mm_set1_epi16(128));
        _mm_srli_epi16(_mm_add_epi16(p, _mm_srli_epi16(p, 8)), 8)
    }

    /// Two pixels' coverages, each across its four lanes.
    #[inline(always)]
    unsafe fn covs(a: u32, b: u32) -> __m128i {
        let (a, b) = (a as i16, b as i16);
        _mm_set_epi16(b, b, b, b, a, a, a, a)
    }

    /// Four premultiplied pixels `s`, each × its `cov`, over `d`.
    #[inline(always)]
    pub unsafe fn over4(d: &mut [[u8; 4]; 4], s: &[[u8; 4]; 4], cov: [u32; 4]) {
        let zero = _mm_setzero_si128();
        let sv = _mm_loadu_si128(s.as_ptr() as *const __m128i);
        let dv = _mm_loadu_si128(d.as_ptr() as *const __m128i);
        let half = |s16: __m128i, d16: __m128i, c: __m128i| {
            let sc = mul255(s16, c);
            // Each pixel's alpha (lane 3 of its four) across its lanes.
            let sa = _mm_shufflehi_epi16(_mm_shufflelo_epi16(sc, 0xff), 0xff);
            let k = _mm_sub_epi16(_mm_set1_epi16(255), sa);
            let v = _mm_add_epi16(sc, mul255(d16, k));
            _mm_min_epi16(v, _mm_set1_epi16(255))
        };
        let lo = half(
            _mm_unpacklo_epi8(sv, zero),
            _mm_unpacklo_epi8(dv, zero),
            covs(cov[0], cov[1]),
        );
        let hi = half(
            _mm_unpackhi_epi8(sv, zero),
            _mm_unpackhi_epi8(dv, zero),
            covs(cov[2], cov[3]),
        );
        _mm_storeu_si128(d.as_mut_ptr() as *mut __m128i, _mm_packus_epi16(lo, hi));
    }

    /// `(a × (256 − w) + b × w) >> 8` on four pixels, out as u16 lanes.
    #[inline(always)]
    pub unsafe fn mix4(o: &mut [[u16; 4]; 4], a: &[[u8; 4]; 4], b: &[[u8; 4]; 4], w: u16) {
        let zero = _mm_setzero_si128();
        let av = _mm_loadu_si128(a.as_ptr() as *const __m128i);
        let bv = _mm_loadu_si128(b.as_ptr() as *const __m128i);
        let (wa, wb) = (_mm_set1_epi16(256 - w as i16), _mm_set1_epi16(w as i16));
        let mix = |a16: __m128i, b16: __m128i| {
            _mm_srli_epi16(
                _mm_add_epi16(_mm_mullo_epi16(a16, wa), _mm_mullo_epi16(b16, wb)),
                8,
            )
        };
        let lo = mix(_mm_unpacklo_epi8(av, zero), _mm_unpacklo_epi8(bv, zero));
        let hi = mix(_mm_unpackhi_epi8(av, zero), _mm_unpackhi_epi8(bv, zero));
        let p = o.as_mut_ptr() as *mut __m128i;
        _mm_storeu_si128(p, lo);
        _mm_storeu_si128(p.add(1), hi);
    }

    /// `(l × (256 − w) + r × w) >> 8` on four pixels from five mixed entries.
    #[inline(always)]
    pub unsafe fn columns4(o: &mut [[u8; 4]; 4], m: &[[u16; 4]], w: u16) {
        let p = m.as_ptr() as *const u16;
        let (wl, wr) = (_mm_set1_epi16(256 - w as i16), _mm_set1_epi16(w as i16));
        let mix = |l: __m128i, r: __m128i| {
            _mm_srli_epi16(
                _mm_add_epi16(_mm_mullo_epi16(l, wl), _mm_mullo_epi16(r, wr)),
                8,
            )
        };
        let lo = mix(
            _mm_loadu_si128(p as *const __m128i),
            _mm_loadu_si128(p.add(4) as *const __m128i),
        );
        let hi = mix(
            _mm_loadu_si128(p.add(8) as *const __m128i),
            _mm_loadu_si128(p.add(12) as *const __m128i),
        );
        _mm_storeu_si128(o.as_mut_ptr() as *mut __m128i, _mm_packus_epi16(lo, hi));
    }
}

/// Lays one row of source pixels over a row of the frame: transparent
/// stretches cost nothing, opaque ones at full coverage are copied.
#[inline(always)]
fn lay_row(d: &mut [u8], s: &[u8], m: Option<&[u8]>, op: u32) {
    const ALPHAS: u128 =
        u128::from_ne_bytes([0, 0, 0, 0xff, 0, 0, 0, 0xff, 0, 0, 0, 0xff, 0, 0, 0, 0xff]);
    let (dq, dr) = d.as_chunks_mut::<16>();
    let (sq, sr) = s.as_chunks::<16>();
    for (i, (dp, sp)) in dq.iter_mut().zip(sq).enumerate() {
        let bits = u128::from_ne_bytes(*sp);
        if bits == 0 {
            continue;
        }
        let d4 = dp.as_chunks_mut::<4>().0;
        let s4 = sp.as_chunks::<4>().0;
        match m {
            None if op == 255 => {
                if bits & ALPHAS == ALPHAS {
                    *dp = *sp;
                } else {
                    over_run(d4, s4, 255);
                }
            }
            None => over_run(d4, s4, op),
            Some(m) => over_run_masked(d4, s4, &m[i * 4..i * 4 + 4], op),
        }
    }
    let d4 = dr.as_chunks_mut::<4>().0;
    let s4 = sr.as_chunks::<4>().0;
    match m {
        None if op == 255 => {
            for (dp, sp) in d4.iter_mut().zip(s4) {
                over_one(dp, sp);
            }
        }
        None => over_run(d4, s4, op),
        Some(m) => over_run_masked(d4, s4, &m[m.len() - d4.len()..], op),
    }
}

/// `src` (premultiplied) laid over `pm` with its top-left at (`x0` + `fx`,
/// `y0` + `fy`), `fx` and `fy` in [0, 1): sampled bilinear (separable: each
/// row mixed with the one above, then each pixel with the one on its left),
/// × `opacity`, × `mask`. Rows of `src` that are transparent cost nothing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn composite_bilinear(
    pm: &mut Pixmap,
    src: &Pixmap,
    x0: i32,
    y0: i32,
    fx: f32,
    fy: f32,
    opacity: f32,
    mask: Option<&Mask>,
) {
    // Output pixel (x0 + i) covers src at i - fx: column i weighs 1 - fx,
    // column i - 1 weighs fx (same for the rows). Weights in 1/256.
    if fx <= 0.0 && fy <= 0.0 {
        return composite(pm, src, x0, y0, opacity, mask);
    }
    let wx = ((1.0 - fx.clamp(0.0, 1.0)) * 256.0).round() as u32;
    let wy = ((1.0 - fy.clamp(0.0, 1.0)) * 256.0).round() as u32;
    let (fw, fh) = (pm.width() as i32, pm.height() as i32);
    let (sw, sh) = (src.width() as i32, src.height() as i32);
    // Output pixel (x0 + i, y0 + j) mixes src columns i-1, i and rows j-1, j
    // (i in 0..=sw, j in 0..=sh); outside src is transparent.
    let (lx, ly) = (x0.max(0), y0.max(0));
    let (rx, ry) = ((x0 + sw + 1).min(fw), (y0 + sh + 1).min(fh));
    if rx <= lx || ry <= ly {
        return;
    }
    let op = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    if op == 0 {
        return;
    }
    let mask = mask.filter(|m| (m.width(), m.height()) == (pm.width(), pm.height()));
    let mdata = mask.map(|m| m.data());
    let swu = sw as usize;
    let rows: &[[u8; 4]] = src.data().as_chunks::<4>().0;
    let empty: Vec<bool> = rows
        .chunks_exact(swu)
        .map(|row| row.iter().all(|p| *p == [0, 0, 0, 0]))
        .collect();
    let zero = vec![[0u8; 4]; swu];
    let n = (rx - lx) as usize;
    // Columns -1 ..= sw of the source, mixed with the row above.
    let mut mixed = vec![[0u16; 4]; swu + 2];
    let mut line = vec![[0u8; 4]; n];
    let dst = pm.data_mut();
    for y in ly..ry {
        let j = y - y0;
        let row_of = |r: i32| -> Option<&[[u8; 4]]> {
            (r >= 0 && r < sh && !empty[r as usize])
                .then(|| &rows[r as usize * swu..(r as usize + 1) * swu])
        };
        let (above, here) = (row_of(j - 1), row_of(j));
        if above.is_none() && here.is_none() {
            continue;
        }
        let (above, here) = (above.unwrap_or(&zero), here.unwrap_or(&zero));
        mix_rows(&mut mixed[1..=swu], above, here, wy);
        // Output column lx + i is source column k = lx + i - x0: mixed[k]
        // holds column k - 1, mixed[k + 1] column k.
        let k0 = (lx - x0) as usize;
        mix_columns(&mut line, &mixed[k0..k0 + n + 1], wx);
        let drow = (y * fw + lx) as usize;
        let d = &mut dst[drow * 4..(drow + n) * 4];
        let m = mdata.map(|m| &m[drow..drow + n]);
        lay_row(d, line.as_flattened(), m, op);
    }
}

/// `src` (premultiplied) laid with its top-left at (`x0`, `y0`) over `pm`
/// (source over), × `opacity`, × `mask` (frame sized). Integer arithmetic,
/// rounded: an opaque pixel fully covered is copied as is, a transparent one
/// leaves the frame as it is.
pub(crate) fn composite(
    pm: &mut Pixmap,
    src: &Pixmap,
    x0: i32,
    y0: i32,
    opacity: f32,
    mask: Option<&Mask>,
) {
    composite_bytes(
        pm,
        src.data(),
        src.width(),
        src.height(),
        x0,
        y0,
        opacity,
        mask,
    );
}

/// [`composite`] of a `sw` × `sh` premultiplied RGBA buffer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn composite_bytes(
    pm: &mut Pixmap,
    sdata: &[u8],
    sw: u32,
    sh: u32,
    x0: i32,
    y0: i32,
    opacity: f32,
    mask: Option<&Mask>,
) {
    let (fw, fh) = (pm.width() as i32, pm.height() as i32);
    let (sw, sh) = (sw as i32, sh as i32);
    let (lx, ly) = (x0.max(0), y0.max(0));
    let (rx, ry) = ((x0 + sw).min(fw), (y0 + sh).min(fh));
    if rx <= lx || ry <= ly {
        return;
    }
    let op = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    if op == 0 {
        return;
    }
    let mask = mask.filter(|m| (m.width(), m.height()) == (pm.width(), pm.height()));
    let n = (rx - lx) as usize;
    let mdata = mask.map(|m| m.data());
    let dst = pm.data_mut();
    for y in ly..ry {
        let drow = (y * fw + lx) as usize;
        let srow = ((y - y0) * sw + (lx - x0)) as usize;
        let d = &mut dst[drow * 4..(drow + n) * 4];
        let s = &sdata[srow * 4..(srow + n) * 4];
        let m = mdata.map(|m| &m[drow..drow + n]);
        lay_row(d, s, m, op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{FillRule, PathBuilder};

    fn picture(w: u32, h: u32) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        for (i, px) in p.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let a = if i % 7 == 0 { 128 } else { 255 } as u32;
            let c = [
                (i * 37 % 256) as u32,
                (i * 11 % 256) as u32,
                (i * 3 % 256) as u32,
            ];
            px[0] = (c[0] * a / 255) as u8;
            px[1] = (c[1] * a / 255) as u8;
            px[2] = (c[2] * a / 255) as u8;
            px[3] = a as u8;
        }
        p
    }

    fn frame() -> Pixmap {
        let mut f = Pixmap::new(120, 90).unwrap();
        f.fill(tiny_skia::Color::from_rgba8(20, 60, 200, 255));
        f
    }

    fn mask() -> Mask {
        let mut m = Mask::new(120, 90).unwrap();
        let p = PathBuilder::from_circle(60.0, 45.0, 38.3).unwrap();
        m.fill_path(&p, FillRule::Winding, true, Transform::identity());
        m
    }

    /// The lanes compute what the scalar arithmetic does, bit for bit.
    #[test]
    fn lanes_and_scalars_agree() {
        let mut seed = 12345u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 24) as u8
        };
        for round in 0..200 {
            let n = 4 + round % 9;
            let mut px = || {
                let a = rnd();
                [rnd().min(a), rnd().min(a), rnd().min(a), a]
            };
            let s: Vec<[u8; 4]> = (0..n).map(|_| px()).collect();
            let d0: Vec<[u8; 4]> = (0..n).map(|_| px()).collect();
            let m: Vec<u8> = (0..n).map(|_| rnd()).collect();
            let cov = (rnd() as u32).max(1);
            let (mut a, mut b) = (d0.clone(), d0.clone());
            over_run(&mut a, &s, cov);
            over_run_scalar(&mut b, &s, cov);
            assert_eq!(a, b, "over, cov {cov}");
            let (mut a, mut b) = (d0.clone(), d0.clone());
            over_run_masked(&mut a, &s, &m, cov);
            over_run_masked_scalar(&mut b, &s, &m, cov);
            assert_eq!(a, b, "masked over, cov {cov}");
            let w = (rnd() as u32 + 1).min(256);
            let (mut a, mut b) = (vec![[0u16; 4]; n], vec![[0u16; 4]; n]);
            mix_rows(&mut a, &s, &d0, w);
            mix_rows_scalar(&mut b, &s, &d0, w);
            assert_eq!(a, b, "rows, w {w}");
            let mixed: Vec<[u16; 4]> = (0..n + 1)
                .map(|_| [rnd() as u16, rnd() as u16, rnd() as u16, rnd() as u16])
                .collect();
            let (mut a, mut b) = (vec![[0u8; 4]; n], vec![[0u8; 4]; n]);
            mix_columns(&mut a, &mixed, w);
            mix_columns_scalar(&mut b, &mixed, w);
            assert_eq!(a, b, "columns, w {w}");
        }
    }

    /// Laid at a fraction of a pixel, the separable bilinear is the four
    /// taps weighed by the fraction (to the rounding), transparent beyond
    /// the picture; whole pixels are laid as is.
    #[test]
    fn bilinear_laying_mixes_the_four_neighbours() {
        let src = picture(40, 30);
        let at = |x: i32, y: i32| -> [f32; 4] {
            if x < 0 || y < 0 || x >= 40 || y >= 30 {
                return [0.0; 4];
            }
            let p = &src.data()[((y * 40 + x) * 4) as usize..][..4];
            [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
        };
        for (fx, fy) in [(0.5f32, 0.25f32), (0.0, 0.75), (0.9, 0.0)] {
            let mut ours = frame();
            composite_bilinear(&mut ours, &src, 30, 20, fx, fy, 1.0, None);
            let mut worst = 0u8;
            for y in 0..90i32 {
                for x in 0..120i32 {
                    // Source coordinate of this pixel: columns i-1 and i, rows j-1 and j.
                    let (i, j) = (x - 30, y - 20);
                    let want = if (0..=40).contains(&i) && (0..=30).contains(&j) {
                        std::array::from_fn(|c| {
                            let top = at(i - 1, j - 1)[c] * fx + at(i, j - 1)[c] * (1.0 - fx);
                            let bottom = at(i - 1, j)[c] * fx + at(i, j)[c] * (1.0 - fx);
                            top * fy + bottom * (1.0 - fy)
                        })
                    } else {
                        [0.0; 4]
                    };
                    // Over the frame's blue.
                    let bg = [20.0, 60.0, 200.0, 255.0];
                    let got = ours.pixel(x as u32, y as u32).unwrap();
                    let got = [got.red(), got.green(), got.blue(), got.alpha()];
                    for c in 0..4 {
                        let expect = want[c] + bg[c] * (1.0 - want[3] / 255.0);
                        worst = worst.max((got[c] as f32 - expect).abs().round() as u8);
                    }
                }
            }
            assert!(worst <= 3, "({fx}, {fy}): off by {worst}");
        }
        let mut whole = frame();
        composite_bilinear(&mut whole, &src, 30, 20, 0.0, 0.0, 1.0, None);
        let mut plain = frame();
        composite(&mut plain, &src, 30, 20, 1.0, None);
        assert_eq!(whole.data(), plain.data());
    }

    /// A picture kept scaled draws the same pixels as when drawn fresh.
    #[test]
    fn a_kept_scaled_picture_draws_what_a_fresh_one_does() {
        let img = picture(16, 10);
        let tf = Transform::from_translate(10.3, 7.6).pre_scale(6.1, 6.4);
        let m = mask();
        let mut s = Scaled::default();
        let mut a = frame();
        assert!(s.draw(
            &mut a,
            "p",
            &img,
            tf,
            FilterQuality::Bicubic,
            0.8,
            Some(&m),
            None,
            true
        ));
        assert_eq!((s.made, s.found), (1, 0));
        let mut b = frame();
        s.frame();
        assert!(s.draw(
            &mut b,
            "p",
            &img,
            tf,
            FilterQuality::Bicubic,
            0.8,
            Some(&m),
            None,
            true
        ));
        assert_eq!((s.made, s.found), (1, 1));
        assert_eq!(a.data(), b.data());
        // Not kept: drawn again, the same pixels.
        let mut fresh = Scaled::default();
        let mut c = frame();
        fresh.draw(
            &mut c,
            "p",
            &img,
            tf,
            FilterQuality::Bicubic,
            0.8,
            Some(&m),
            None,
            false,
        );
        assert_eq!(a.data(), c.data());
        assert!(fresh.map.is_empty());
        // Close to tiny-skia's own drawing (±1 where partly covered).
        let mut want = frame();
        let paint = PixmapPaint {
            opacity: 0.8,
            quality: FilterQuality::Bicubic,
            ..Default::default()
        };
        want.draw_pixmap(0, 0, img.as_ref(), &paint, tf, Some(&m));
        let worst = a
            .data()
            .iter()
            .zip(want.data())
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap();
        assert!(worst <= 2, "{worst}");
    }

    /// Another size, position or picture: scaled again; reloaded: forgotten.
    #[test]
    fn scaled_pictures_follow_their_picture_and_transform() {
        let img = picture(16, 10);
        let mut s = Scaled::default();
        let tf = Transform::from_translate(10.0, 7.0).pre_scale(3.0, 3.0);
        let mut f = frame();
        s.draw(
            &mut f,
            "p",
            &img,
            tf,
            FilterQuality::Bicubic,
            1.0,
            None,
            None,
            true,
        );
        // Moved by whole pixels: the same scaled picture.
        let moved = Transform::from_translate(12.0, 9.0).pre_scale(3.0, 3.0);
        s.draw(
            &mut f,
            "p",
            &img,
            moved,
            FilterQuality::Bicubic,
            1.0,
            None,
            None,
            true,
        );
        assert_eq!((s.made, s.found), (1, 1));
        // Bigger, or by a fraction of a pixel: another one.
        let bigger = Transform::from_translate(10.0, 7.0).pre_scale(3.5, 3.0);
        s.draw(
            &mut f,
            "p",
            &img,
            bigger,
            FilterQuality::Bicubic,
            1.0,
            None,
            None,
            true,
        );
        let half = Transform::from_translate(10.5, 7.0).pre_scale(3.0, 3.0);
        s.draw(
            &mut f,
            "p",
            &img,
            half,
            FilterQuality::Bicubic,
            1.0,
            None,
            None,
            true,
        );
        s.draw(
            &mut f,
            "q",
            &img,
            tf,
            FilterQuality::Bicubic,
            1.0,
            None,
            None,
            true,
        );
        assert_eq!((s.made, s.found), (4, 1));
        s.forget("p");
        assert_eq!(s.map.len(), 1);
        s.draw(
            &mut f,
            "p",
            &img,
            tf,
            FilterQuality::Bicubic,
            1.0,
            None,
            None,
            true,
        );
        assert_eq!(s.made, 5);
        // Unused for long: dropped.
        for _ in 0..=KEEP_FRAMES + 1 {
            s.frame();
        }
        assert!(s.map.is_empty() && s.bytes == 0);
        // Turned: left to the caller.
        let turned = Transform::from_rotate(10.0);
        assert!(!s.draw(
            &mut f,
            "p",
            &img,
            turned,
            FilterQuality::Bicubic,
            1.0,
            None,
            None,
            true
        ));
    }

    /// Laid at full opacity without a clip: opaque pixels copied, others
    /// blended over.
    #[test]
    fn composite_is_source_over() {
        let src = picture(30, 20);
        let mut f = frame();
        let before = f.clone();
        composite(&mut f, &src, -5, 80, 1.0, None);
        // Only rows 80..90 and columns 0..25 touched.
        for (i, (a, b)) in f.data().chunks(4).zip(before.data().chunks(4)).enumerate() {
            let (x, y) = (i % 120, i / 120);
            if y < 80 || x >= 25 {
                assert_eq!(a, b);
            } else {
                let s = &src.data()[(((y - 80) * 30 + x + 5) * 4)..][..4];
                if s[3] == 255 {
                    assert_eq!(a, s);
                }
            }
        }
    }
}

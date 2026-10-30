//! Tall pictures of scrolling content (a credits roll), drawn once in
//! chunks and shown through a window each frame.
//!
//! A roll moves every frame but its content does not change: its lines are
//! drawn once, at their size on the frame, into a strip (in chunks of rows,
//! drawn when the window first reaches them and dropped once it has passed),
//! and each frame the rows under the window are laid on the frame. Between
//! two whole pixels the window mixes two strip rows (a linear filter), so the
//! roll still moves by fractions of a pixel. The result only depends on the
//! content and the position: the same frame whatever was drawn before.

use std::collections::HashMap;

use tiny_skia::Pixmap;

/// Frames a strip is kept unused.
const KEEP_FRAMES: u64 = 250;

/// The strip of one layer.
pub(crate) struct Strip {
    /// What it shows (content, size on the frame, fonts…).
    key: u64,
    width: u32,
    /// Rows per chunk.
    chunk: u32,
    /// Chunks drawn: their index, picture, and the columns each row covers
    /// (none: transparent).
    chunks: Vec<(i64, Pixmap, Vec<Option<(u32, u32)>>)>,
    used: u64,
    /// Chunks drawn so far (tests, measures).
    pub drawn: u64,
}

impl Strip {
    #[cfg(test)]
    fn row(&self, r: i64) -> Option<&[u8]> {
        self.row_span(r).map(|(row, _)| row)
    }

    /// Row `r` and the columns it covers (None: transparent or not drawn).
    fn row_span(&self, r: i64) -> Option<(&[u8], (u32, u32))> {
        let k = r.div_euclid(self.chunk as i64);
        let (_, pm, spans) = self.chunks.iter().find(|(i, _, _)| *i == k)?;
        let y = r.rem_euclid(self.chunk as i64) as usize;
        let span = spans[y]?;
        let len = self.width as usize * 4;
        Some((&pm.data()[y * len..(y + 1) * len], span))
    }

    /// Lays `height` rows of the strip from row `top` on `pm` at (`x0`,
    /// `y0`), each mixed with the row above it by `above` (0..1: the content
    /// `above` of a pixel lower), at `opacity`, cut by `mask` (frame sized):
    /// what laying [`Strip::window`] with `scaled::composite` does, only
    /// where the strip has something.
    #[allow(clippy::too_many_arguments)]
    pub fn lay(
        &self,
        pm: &mut Pixmap,
        x0: i32,
        y0: i32,
        top: i64,
        height: u32,
        above: f32,
        opacity: f32,
        mask: Option<&tiny_skia::Mask>,
    ) {
        let wa = (above.clamp(0.0, 1.0) * 256.0).round() as u32;
        let wb = 256 - wa;
        let op = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        if op == 0 {
            return;
        }
        let (fw, fh) = (pm.width() as i32, pm.height() as i32);
        let mask = mask.filter(|m| (m.width(), m.height()) == (pm.width(), pm.height()));
        let data = pm.data_mut();
        for j in 0..height as i64 {
            let y = y0 + j as i32;
            if y < 0 || y >= fh {
                continue;
            }
            let b = if wb > 0 { self.row_span(top + j) } else { None };
            let a = if wa > 0 {
                self.row_span(top + j - 1)
            } else {
                None
            };
            let (lo, hi) = match (a, b) {
                (None, None) => continue,
                (Some((_, s)), None) | (None, Some((_, s))) => s,
                (Some((_, s)), Some((_, t))) => (s.0.min(t.0), s.1.max(t.1)),
            };
            let (ra, rb) = (a.map(|r| r.0), b.map(|r| r.0));
            let row = (y * fw) as usize;
            for x in lo..hi {
                let fx = x0 + x as i32;
                if fx < 0 || fx >= fw {
                    continue;
                }
                let i = x as usize * 4;
                let px = |r: Option<&[u8]>, c: usize| r.map_or(0, |r| r[i + c] as u32);
                let src = [0, 1, 2, 3].map(|c| (px(ra, c) * wa + px(rb, c) * wb + 128) >> 8);
                if src == [0, 0, 0, 0] {
                    continue;
                }
                let at = row + fx as usize;
                let cov = match mask {
                    Some(m) => crate::scaled::mul255(m.data()[at] as u32, op),
                    None => op,
                };
                crate::scaled::over(&mut data[at * 4..at * 4 + 4], src, cov);
            }
        }
    }

    /// Makes sure strip rows `rows` (of `0..total`) are drawn: the missing
    /// chunks are drawn by `draw(k, picture)` (chunk `k` = rows `k × chunk`
    /// onwards); chunks out of all of `rows` are dropped.
    pub fn show(
        &mut self,
        rows: &[std::ops::Range<i64>],
        total: i64,
        mut draw: impl FnMut(i64, &mut Pixmap),
    ) {
        let c = self.chunk as i64;
        let needed: Vec<(i64, i64)> = rows
            .iter()
            .filter(|r| r.end.min(total) > r.start.max(0))
            .map(|r| {
                (
                    r.start.max(0).div_euclid(c),
                    (r.end.min(total) - 1).div_euclid(c),
                )
            })
            .collect();
        self.chunks
            .retain(|(k, _, _)| needed.iter().any(|(a, b)| k >= a && k <= b));
        for &(k0, k1) in &needed {
            for k in k0..=k1 {
                if self.chunks.iter().any(|(i, _, _)| *i == k) {
                    continue;
                }
                let Some(mut pm) = Pixmap::new(self.width, self.chunk) else {
                    continue;
                };
                draw(k, &mut pm);
                self.drawn += 1;
                let spans = spans(&pm);
                self.chunks.push((k, pm, spans));
            }
        }
    }

    /// `height` rows of the strip from row `top`, each mixed with the row
    /// above it by `above` (0..1: the content `above` of a pixel lower).
    #[cfg(test)]
    pub fn window(&self, top: i64, height: u32, above: f32) -> Option<Pixmap> {
        let mut out = Pixmap::new(self.width, height)?;
        let wa = (above.clamp(0.0, 1.0) * 256.0).round() as u32;
        let wb = 256 - wa;
        let len = self.width as usize * 4;
        let data = out.data_mut();
        for j in 0..height as i64 {
            let dst = &mut data[j as usize * len..(j as usize + 1) * len];
            let b = if wb > 0 { self.row(top + j) } else { None };
            let a = if wa > 0 { self.row(top + j - 1) } else { None };
            match (a, b) {
                (None, None) => {}
                (None, Some(b)) if wa == 0 => dst.copy_from_slice(b),
                (Some(a), None) if wb == 0 => dst.copy_from_slice(a),
                (a, b) => {
                    for (i, d) in dst.iter_mut().enumerate() {
                        let va = a.map_or(0, |r| r[i] as u32);
                        let vb = b.map_or(0, |r| r[i] as u32);
                        *d = ((va * wa + vb * wb + 128) >> 8) as u8;
                    }
                }
            }
        }
        Some(out)
    }
}

/// The columns each row of `pm` covers (None: a transparent row).
fn spans(pm: &Pixmap) -> Vec<Option<(u32, u32)>> {
    let w = pm.width() as usize;
    pm.data()
        .chunks_exact(w * 4)
        .map(|row| {
            let px = |x: &usize| row[x * 4..x * 4 + 4] != [0, 0, 0, 0];
            let lo = (0..w).find(px)?;
            let hi = (0..w).rev().find(px)? + 1;
            Some((lo as u32, hi as u32))
        })
        .collect()
}

/// Rows of a strip above its content (glyphs reaching above their line).
pub(crate) const MARGIN: i64 = 64;

/// Content `len` layer pixels long, scrolling in the `w` × `h` box of a
/// layer (through `tf`), its top at each of `tops` (layer pixels; a looping
/// roll shows two copies): laid on `pm` from the strip of layer `id`,
/// whose chunks `draw(renderer, chunk, transform)` draws (content
/// coordinates → chunk pixels), at `opacity`, cut by `mask`. `key` says
/// what the content is. False when `tf` turns or skews the layer (to be
/// drawn directly then).
#[allow(clippy::too_many_arguments)]
pub(crate) fn roll(
    rd: &mut crate::Renderer,
    pm: &mut Pixmap,
    id: &str,
    key: u64,
    tf: tiny_skia::Transform,
    (w, h): (f32, f32),
    len: f32,
    tops: &[f32],
    opacity: f32,
    mask: Option<&tiny_skia::Mask>,
    draw: &mut dyn FnMut(&mut crate::Renderer, &mut Pixmap, tiny_skia::Transform),
) -> bool {
    use std::hash::{Hash, Hasher};
    if tf.kx != 0.0 || tf.ky != 0.0 || tf.sx <= 0.0 || tf.sy <= 0.0 {
        return false;
    }
    if opacity <= 0.0 {
        return true;
    }
    let (fw, fh) = (pm.width(), pm.height());
    let Some(bx) = tiny_skia::Rect::from_xywh(0.0, 0.0, w.max(0.01), h.max(0.01))
        .and_then(|r| crate::scaled::device_bounds(r, tf, fw, fh))
    else {
        return true;
    };
    let (x0, sx, sy) = (bx.x(), tf.sx, tf.sy);
    let (width, rows) = (bx.width(), bx.height());
    let chunk = rows.div_ceil(2).max(128);
    let key = {
        let mut k = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut k);
        [w, sx, sy, tf.tx - x0 as f32]
            .map(f32::to_bits)
            .hash(&mut k);
        (rd.font_generation, rd.assets_generation).hash(&mut k);
        k.finish()
    };
    // Frame row y shows strip row y − top_y + MARGIN (top_y: the content's
    // top on the frame), between whole pixels mixed with the row above.
    let at: Vec<(i64, f32)> = tops
        .iter()
        .map(|t| {
            let top_y = sy * t + tf.ty;
            let iy = top_y.floor();
            (bx.y() as i64 - iy as i64 + MARGIN, top_y - iy)
        })
        .collect();
    let total = (sy * len).ceil() as i64 + 2 * MARGIN;
    let ranges: Vec<_> = at.iter().map(|(t, _)| t - 1..t + rows as i64).collect();
    let mut strip = rd.strips.take(id, key, width, chunk);
    strip.show(&ranges, total, |k, chunk_pm| {
        let ctop = k * chunk as i64;
        let t = tiny_skia::Transform::from_row(
            sx,
            0.0,
            0.0,
            sy,
            tf.tx - x0 as f32,
            (MARGIN - ctop) as f32,
        );
        draw(rd, chunk_pm, t);
    });
    for (top, above) in at {
        strip.lay(pm, x0, bx.y(), top, rows, above, opacity, mask);
    }
    rd.strips.put(id, strip);
    true
}

/// The strips of a renderer, by layer.
#[derive(Default)]
pub(crate) struct Strips {
    map: HashMap<String, Strip>,
    tick: u64,
}

impl Strips {
    /// A frame begins: strips unused for a while are dropped.
    pub fn frame(&mut self) {
        self.tick += 1;
        let tick = self.tick;
        self.map.retain(|_, s| tick - s.used <= KEEP_FRAMES);
    }

    /// The strip of layer `id` showing `key`, `width` pixels wide in chunks
    /// of `chunk` rows (a new one when it showed something else). Given
    /// back with [`Strips::put`].
    pub fn take(&mut self, id: &str, key: u64, width: u32, chunk: u32) -> Strip {
        match self.map.remove(id) {
            Some(s) if s.key == key && s.width == width && s.chunk == chunk => s,
            old => Strip {
                key,
                width,
                chunk,
                chunks: Vec::new(),
                used: self.tick,
                drawn: old.map_or(0, |o| o.drawn),
            },
        }
    }

    pub fn put(&mut self, id: &str, mut s: Strip) {
        s.used = self.tick;
        self.map.insert(id.to_string(), s);
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn strip(chunk: u32) -> Strip {
        Strip {
            key: 1,
            width: 3,
            chunk,
            chunks: Vec::new(),
            used: 0,
            drawn: 0,
        }
    }

    /// Row r of the content is all (r % 200) + 10.
    fn draw(chunk: u32) -> impl FnMut(i64, &mut Pixmap) {
        move |k, pm| {
            for (y, row) in pm.data_mut().as_chunks_mut::<12>().0.iter_mut().enumerate() {
                row.fill(((k * chunk as i64 + y as i64) % 200 + 10) as u8);
            }
        }
    }

    #[test]
    fn a_window_shows_the_rows_mixed_by_the_fraction() {
        let mut s = strip(8);
        s.show(&[5..20], 100, draw(8));
        assert_eq!(s.drawn, 3);
        let w = s.window(6, 10, 0.0).unwrap();
        for (j, row) in w.data().as_chunks::<12>().0.iter().enumerate() {
            assert!(row.iter().all(|v| *v as usize == 6 + j + 10));
        }
        // A quarter pixel lower: a quarter of the row above.
        let w = s.window(6, 10, 0.25).unwrap();
        for (j, row) in w.data().as_chunks::<12>().0.iter().enumerate() {
            let want = ((5 + j + 10) as f32 * 0.25 + (6 + j + 10) as f32 * 0.75).round();
            assert!(row.iter().all(|v| *v as f32 == want), "{j} {row:?} {want}");
        }
        // Moving on: only the new chunk is drawn, the passed ones dropped.
        s.show(&[9..28], 100, draw(8));
        assert_eq!(s.drawn, 4);
        assert_eq!(s.chunks.len(), 3);
        // Rows out of the content are transparent.
        s.show(&[-4..4], 100, draw(8));
        let w = s.window(-4, 8, 0.0).unwrap();
        assert!(w.data()[..4 * 12].iter().all(|v| *v == 0));
    }

    /// Laid directly, the window is what it is as a picture laid over.
    #[test]
    fn laying_is_compositing_the_window() {
        let mut s = strip(8);
        s.width = 6;
        let mut draw = |k: i64, pm: &mut Pixmap| {
            for (i, px) in pm.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (x, y) = (i % 6, i / 6 + k as usize * 8);
                if (x + y) % 3 != 0 && y % 7 != 2 {
                    let a = ((x * 40 + y * 13) % 256) as u8;
                    px.copy_from_slice(&[a / 2, a / 3, a, a]);
                }
            }
        };
        s.show(&[0..30], 40, &mut draw);
        let mut m = tiny_skia::Mask::new(10, 12).unwrap();
        for (i, v) in m.data_mut().iter_mut().enumerate() {
            *v = (i * 23 % 256) as u8;
        }
        for above in [0.0, 0.3, 1.0] {
            let mut want = Pixmap::new(10, 12).unwrap();
            want.fill(tiny_skia::Color::from_rgba8(10, 200, 30, 255));
            let mut got = want.clone();
            let w = s.window(3, 10, above).unwrap();
            crate::scaled::composite(&mut want, &w, 2, 1, 0.8, Some(&m));
            s.lay(&mut got, 2, 1, 3, 10, above, 0.8, Some(&m));
            assert_eq!(got.data(), want.data(), "{above}");
        }
    }

    #[test]
    fn a_strip_showing_something_else_is_drawn_again() {
        let mut all = Strips::default();
        let mut s = all.take("roll", 7, 3, 8);
        s.show(&[0..10], 100, draw(8));
        all.put("roll", s);
        let mut s = all.take("roll", 7, 3, 8);
        assert_eq!(s.chunks.len(), 2);
        s.show(&[0..10], 100, draw(8));
        assert_eq!(s.drawn, 2);
        all.put("roll", s);
        // New content: nothing kept.
        let s = all.take("roll", 8, 3, 8);
        assert!(s.chunks.is_empty());
        all.put("roll", s);
        for _ in 0..=KEEP_FRAMES + 1 {
            all.frame();
        }
        assert!(all.map.is_empty());
    }
}

//! Drawing helpers of the text widgets (Text, Typewriter, Counter): lines of
//! glyphs with fallback fonts and colour emoji, gradients, blurred shadows.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::Font;
use cg_core::Color;
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint,
    Point, Shader, SpreadMode, Stroke, Transform,
};

use super::Draw;
use crate::text::{self as tx, Chain, PGlyph};

pub(crate) fn sk(c: Color, opacity: f32) -> tiny_skia::Color {
    let [r, g, b, a] = c.0;
    tiny_skia::Color::from_rgba(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        (a * opacity).clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

/// A CSS `linear-gradient(angle, c1, c2)` over the box (0, 0, w, h).
pub(crate) fn gradient(
    c1: Color,
    c2: Color,
    angle: f32,
    w: f32,
    h: f32,
    opacity: f32,
) -> Option<Shader<'static>> {
    let a = (angle - 90.0).to_radians();
    let (dx, dy) = (a.cos() / 2.0, a.sin() / 2.0);
    let len = (w * dx.abs() + h * dy.abs()) * 2.0;
    let (cx, cy) = (w / 2.0, h / 2.0);
    LinearGradient::new(
        Point::from_xy(cx - dx * len, cy - dy * len),
        Point::from_xy(cx + dx * len, cy + dy * len),
        vec![
            GradientStop::new(0.0, sk(c1, opacity)),
            GradientStop::new(1.0, sk(c2, opacity)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
}

/// One line laid out: its outline (layer space) and its colour pictures.
pub(crate) struct LinePaint {
    pub path: Option<Path>,
    pub pictures: Vec<(PGlyph, f32, f32)>,
    pub width: f32,
}

/// Outline of `line` with its left at `x`, baseline `baseline`, squeezed
/// horizontally by `sx` from `x_origin` (1 = not squeezed), slanted by
/// `skew` (synthetic italic, like browsers: 0.25).
#[allow(clippy::too_many_arguments)]
pub(crate) fn line(
    chain: &Chain,
    line: &str,
    size: f32,
    ls: f32,
    space_extra: f32,
    x: f32,
    baseline: f32,
    sx: f32,
    x_origin: f32,
    skew: f32,
) -> LinePaint {
    let (glyphs, width) = tx::glyph_run(chain, line, size, ls, space_extra);
    let mut pb = PathBuilder::new();
    let pics = tx::run_outline(chain, &glyphs, size, x, baseline, &mut pb);
    let squeeze = Transform::from_row(
        sx,
        0.0,
        -skew,
        1.0,
        x_origin * (1.0 - sx) + skew * baseline,
        0.0,
    );
    let path = pb.finish().and_then(|p| {
        if sx == 1.0 && skew == 0.0 {
            Some(p)
        } else {
            p.transform(squeeze)
        }
    });
    let pictures = pics
        .into_iter()
        .map(|g| (g, x_origin + (x + g.x - x_origin) * sx, baseline))
        .collect();
    LinePaint {
        path,
        pictures,
        width,
    }
}

thread_local! {
    static PICS: RefCell<HashMap<(usize, u16, u16), Option<(Arc<Pixmap>, f32, f32, u16)>>> = RefCell::new(HashMap::new());
}

/// The colour picture of a glyph (emoji) and the transform placing it at pen
/// (x, baseline), `size` px, squeezed by `sx`, under `tf` (layer → picture).
#[allow(clippy::too_many_arguments)]
pub(crate) fn picture_at(
    chain: &Chain,
    g: &PGlyph,
    x: f32,
    baseline: f32,
    size: f32,
    sx: f32,
    tf: Transform,
) -> Option<(Arc<Pixmap>, Transform)> {
    // A picture of the text: centred on the capitals' middle.
    if let Some(pic) = chain.inline_of(g.ch) {
        let pm = pic.pixmap.clone()?;
        let (h, w) = pic.size_at(size);
        let top = baseline - cg_core::rich::image_middle(size) - h / 2.0;
        let at = tf.pre_translate(x, top).pre_scale(
            w * sx / pm.width().max(1) as f32,
            h / pm.height().max(1) as f32,
        );
        return Some((pm, at));
    }
    let font = &chain.fonts[g.font];
    let scale = (tf.sx * tf.sx + tf.ky * tf.ky).sqrt().max(0.01);
    let want = (size * scale).round().clamp(8.0, 512.0) as u16;
    let key = (font.font_data().as_ptr() as usize, g.id.0, want);
    let pic = PICS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 600 {
            m.clear();
        }
        m.entry(key)
            .or_insert_with(|| {
                let img = font.glyph_raster_image2(g.id, want)?;
                if !matches!(img.format, ab_glyph::GlyphImageFormat::Png) {
                    return None;
                }
                let rgba = image::load_from_memory_with_format(img.data, image::ImageFormat::Png)
                    .ok()?
                    .to_rgba8();
                let (w, h) = rgba.dimensions();
                let mut data = rgba.into_raw();
                for px in data.as_chunks_mut::<4>().0 {
                    let a = px[3] as u16;
                    for c in &mut px[..3] {
                        *c = ((*c as u16 * a + 127) / 255) as u8;
                    }
                }
                let pm = Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)?;
                Some((Arc::new(pm), img.origin.x, img.origin.y, img.pixels_per_em))
            })
            .clone()
    });
    let (pm, ox, oy, ppem) = pic?;
    let k = size / ppem.max(1) as f32;
    let h = pm.height() as f32;
    let at = tf
        .pre_translate(x + ox * k * sx, baseline - (oy + h) * k)
        .pre_scale(k * sx, k);
    Some((pm, at))
}

/// Draws a colour glyph (emoji) at pen (x, baseline), `size` px, squeezed by `sx`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn picture(
    d: &mut Draw,
    chain: &Chain,
    g: &PGlyph,
    x: f32,
    baseline: f32,
    size: f32,
    sx: f32,
    alpha: f32,
    mask: Option<&Mask>,
) {
    let Some((pm, tf)) = picture_at(chain, g, x, baseline, size, sx, d.tf) else {
        return;
    };
    let paint = PixmapPaint {
        opacity: alpha * d.opacity,
        quality: tiny_skia::FilterQuality::Bicubic,
        ..Default::default()
    };
    d.pm.draw_pixmap(0, 0, pm.as_ref().as_ref(), &paint, tf, mask.or(d.mask));
}

/// Draws `paths` (filled; `stroke`: also stroked that wide) in `color`,
/// blurred by `blur` px and offset by (dx, dy), like a CSS drop shadow.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shadow(
    d: &mut Draw,
    paths: &[&Path],
    stroke: f32,
    color: Color,
    dx: f32,
    dy: f32,
    blur: f32,
    mask: Option<&Mask>,
) {
    shadow_with(d, paths, &[], stroke, color, dx, dy, blur, mask);
}

/// The same with colour pictures (emoji: their own transform in layer
/// space) casting their shadow too, as the browser's drop shadow does.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shadow_with(
    d: &mut Draw,
    paths: &[&Path],
    pics: &[(Arc<Pixmap>, Transform)],
    stroke: f32,
    color: Color,
    dx: f32,
    dy: f32,
    blur: f32,
    mask: Option<&Mask>,
) {
    let tf = d.tf.pre_translate(dx, dy);
    let scale = (d.tf.sx * d.tf.sx + d.tf.ky * d.tf.ky).sqrt().max(0.01);
    let sigma = (blur * scale / 2.0).max(0.0);
    let margin = (sigma * 3.0 + stroke * scale + 2.0).ceil();
    let mut bounds: Option<tiny_skia::Rect> = None;
    let pic_paths: Vec<Path> = pics
        .iter()
        .filter_map(|(pm, t)| {
            let r = tiny_skia::Rect::from_xywh(0.0, 0.0, pm.width() as f32, pm.height() as f32)?;
            PathBuilder::from_rect(r).transform(*t)
        })
        .collect();
    for p in paths.iter().copied().chain(pic_paths.iter()) {
        if let Some(b) = p.clone().transform(tf).map(|p| p.bounds()) {
            bounds = Some(match bounds {
                None => b,
                Some(a) => tiny_skia::Rect::from_ltrb(
                    a.left().min(b.left()),
                    a.top().min(b.top()),
                    a.right().max(b.right()),
                    a.bottom().max(b.bottom()),
                )
                .unwrap_or(a),
            });
        }
    }
    let Some(b) = bounds else {
        return;
    };
    let x0 = (b.left() - margin).floor().max(0.0);
    let y0 = (b.top() - margin).floor().max(0.0);
    let x1 = (b.right() + margin).ceil().min(d.pm.width() as f32);
    let y1 = (b.bottom() + margin).ceil().min(d.pm.height() as f32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let Some(mut off) = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color(sk(color, 1.0));
    paint.anti_alias = true;
    let local = tf.post_translate(-x0, -y0);
    for p in paths {
        off.fill_path(p, &paint, FillRule::Winding, local, None);
        if stroke > 0.0 {
            let s = Stroke {
                width: stroke,
                line_join: tiny_skia::LineJoin::Round,
                ..Default::default()
            };
            off.stroke_path(p, &paint, &s, local, None);
        }
    }
    if !pics.is_empty() {
        // The pictures' coverage in the shadow's colour.
        let mut cover = Pixmap::new(off.width(), off.height()).unwrap_or_else(|| off.clone());
        let pp = PixmapPaint {
            quality: tiny_skia::FilterQuality::Bicubic,
            ..Default::default()
        };
        for (pm, t) in pics {
            cover.draw_pixmap(0, 0, pm.as_ref().as_ref(), &pp, local.pre_concat(*t), None);
        }
        let c = sk(color, 1.0).premultiply();
        let [cr, cg, cb, ca] = [c.red(), c.green(), c.blue(), c.alpha()];
        for (o, s) in off
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(cover.data().as_chunks::<4>().0)
        {
            let k = s[3] as f32 / 255.0;
            if k <= 0.0 {
                continue;
            }
            // Source over: the picture's shadow over what the paths left.
            let src = [cr * k, cg * k, cb * k, ca * k];
            let keep = 1.0 - src[3];
            for i in 0..4 {
                o[i] = (src[i] * 255.0 + o[i] as f32 * keep)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
    }
    if sigma > 0.3 {
        blur_pixmap(&mut off, sigma);
    }
    let pp = PixmapPaint {
        opacity: d.opacity,
        ..Default::default()
    };
    d.pm.draw_pixmap(
        x0 as i32,
        y0 as i32,
        off.as_ref(),
        &pp,
        Transform::identity(),
        mask.or(d.mask),
    );
}

/// Three box blurs ≈ a Gaussian of `sigma` (premultiplied RGBA).
fn blur_pixmap(pm: &mut Pixmap, sigma: f32) {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let size = ((12.0 * sigma * sigma / 3.0 + 1.0).sqrt()).max(1.0);
    let r = ((size - 1.0) / 2.0).round().max(1.0) as usize;
    let data = pm.data_mut();
    let mut tmp = vec![0u8; data.len()];
    for _ in 0..3 {
        box_pass(data, &mut tmp, w, h, r, true);
        box_pass(&tmp, data, w, h, r, false);
    }
}

/// `a / d` rounded down, for the sums of a box (`a` ≤ 255 × `d`, `d` <
/// 4096): by a multiplication, exact in that range.
#[derive(Clone, Copy)]
struct Div {
    m: u64,
}

impl Div {
    fn new(d: u32) -> Div {
        Div {
            m: (1u64 << 32).div_ceil(d as u64),
        }
    }
    #[inline(always)]
    fn of(self, a: u32) -> u8 {
        (((a as u64 * self.m) >> 32) as u32).min(255) as u8
    }
}

/// One box pass of radius `r` along the rows (`horizontal`) or the columns
/// of a premultiplied `w` × `h` picture; outside is transparent. Columns
/// are summed a whole row at a time (memory read in order).
fn box_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let d = (2 * r + 1) as u32;
    if d >= 4096 {
        return box_pass_slow(src, dst, w, h, r, horizontal);
    }
    let div = Div::new(d);
    if horizontal {
        for (s, o) in src.chunks_exact(w * 4).zip(dst.chunks_exact_mut(w * 4)) {
            let s = s.as_chunks::<4>().0;
            let o = o.as_chunks_mut::<4>().0;
            let mut acc = [0u32; 4];
            for p in &s[..=r.min(w - 1)] {
                for c in 0..4 {
                    acc[c] += p[c] as u32;
                }
            }
            for i in 0..w {
                o[i] = acc.map(|a| div.of(a));
                if i + r + 1 < w {
                    let q = s[i + r + 1];
                    for c in 0..4 {
                        acc[c] += q[c] as u32;
                    }
                }
                if i >= r {
                    let q = s[i - r];
                    for c in 0..4 {
                        acc[c] -= q[c] as u32;
                    }
                }
            }
        }
        return;
    }
    let row = w * 4;
    let mut acc = vec![0u32; row];
    let add = |acc: &mut [u32], y: usize| {
        for (a, v) in acc.iter_mut().zip(&src[y * row..(y + 1) * row]) {
            *a += *v as u32;
        }
    };
    for y in 0..=r.min(h - 1) {
        add(&mut acc, y);
    }
    for y in 0..h {
        for (o, a) in dst[y * row..(y + 1) * row].iter_mut().zip(&acc) {
            *o = div.of(*a);
        }
        if y + r + 1 < h {
            add(&mut acc, y + r + 1);
        }
        if y >= r {
            for (a, v) in acc.iter_mut().zip(&src[(y - r) * row..(y - r + 1) * row]) {
                *a -= *v as u32;
            }
        }
    }
}

/// The same, one pixel at a time (boxes too wide for `Div`; the reference).
fn box_pass_slow(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    let idx = |o: usize, i: usize| {
        if horizontal {
            (o * w + i) * 4
        } else {
            (i * w + o) * 4
        }
    };
    let div = (2 * r + 1) as u32;
    for o in 0..outer {
        let mut acc = [0u32; 4];
        for i in 0..=r.min(inner - 1) {
            let p = idx(o, i);
            for c in 0..4 {
                acc[c] += src[p + c] as u32;
            }
        }
        for i in 0..inner {
            let p = idx(o, i);
            for c in 0..4 {
                dst[p + c] = (acc[c] / div).min(255) as u8;
            }
            if i + r + 1 < inner {
                let q = idx(o, i + r + 1);
                for c in 0..4 {
                    acc[c] += src[q + c] as u32;
                }
            }
            if i >= r {
                let q = idx(o, i - r);
                for c in 0..4 {
                    acc[c] -= src[q + c] as u32;
                }
            }
        }
    }
}

/// The text's font, falling back to Inter (italic is synthesized, as the
/// browser does with the fonts the server serves).
pub(crate) fn main_font(d: &mut Draw, family: &str, weight: u16) -> Option<ab_glyph::FontArc> {
    d.rd.fonts
        .font(family, weight)
        .or_else(|| d.rd.fonts.font("Inter", weight))
}

/// A chain for `text` (and "…", "-", the cursor's characters).
pub(crate) fn chain(d: &mut Draw, main: ab_glyph::FontArc, text: &str) -> Chain {
    let mut c = Chain::new(&mut d.rd.fonts, main, &format!("{text}…-"));
    // The pictures of a Text (`<img>`).
    if !d.inline.is_empty() {
        let lh = match &d.layer.kind {
            cg_core::LayerKind::Text { line_height, .. } => line_height.unwrap_or(tx::LINE_HEIGHT),
            _ => tx::LINE_HEIGHT,
        };
        let pics: Vec<_> = d.inline.clone();
        c.inline = pics
            .iter()
            .map(|i| tx::InlinePic {
                pixmap: (!i.src.is_empty())
                    .then(|| d.rd.image(&i.src).map(|p| Arc::new(p.clone())))
                    .flatten(),
                height: i.height,
                line_height: lh,
            })
            .collect();
    }
    c
}

#[cfg(test)]
mod blur_tests {
    use super::*;

    /// Row by row, the passes give what one pixel at a time does, bit for bit.
    #[test]
    fn the_fast_box_passes_are_the_reference_ones() {
        let (w, h) = (37, 23);
        let mut seed = 7u32;
        let src: Vec<u8> = (0..w * h * 4)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 24) as u8
            })
            .collect();
        for r in [1, 2, 5, 11, 30] {
            for horizontal in [true, false] {
                let (mut a, mut b) = (vec![0u8; src.len()], vec![0u8; src.len()]);
                box_pass(&src, &mut a, w, h, r, horizontal);
                box_pass_slow(&src, &mut b, w, h, r, horizontal);
                assert_eq!(a, b, "r {r}, horizontal {horizontal}");
            }
        }
        for d in 1..4096u32 {
            let div = Div::new(d);
            for a in [0, 1, d - 1, d, d + 1, 255 * d - 1, 255 * d, 128 * d + d / 2] {
                assert_eq!(div.of(a) as u32, (a / d).min(255), "{a} / {d}");
            }
        }
    }
}

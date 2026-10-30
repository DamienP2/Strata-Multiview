//! Built-in slates: what a channel picture shows while no project is loaded.
//!
//! Six designs, each showing « Strata Graphics » (the app's logo), the
//! channel name, the picture size, its aspect ratio and frame rate, and the
//! F AudioVisuel signature; the seventh choice is the user's own picture
//! ([`SlateStyle::Custom`]). A slate is drawn once, at the picture size, and
//! sent as is by every output.
//!
//! The layouts follow the approved HTML references: sizes are in `cqmin` (a
//! hundredth of the picture's short side), portrait pictures get the
//! vertical layouts. Blurred shapes are drawn with soft edges (an analytic
//! blur), never by blurring the picture.

use std::time::SystemTime;

use ab_glyph::FontArc;
use serde::{Deserialize, Serialize};
use tiny_skia::{
    BlendMode, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, PixmapPaint,
    Point, Shader, SpreadMode, Stroke, Transform,
};

use crate::text::{self, Chain, FontStore, PGlyph};
use crate::{Frame, OutputFormat, Renderer};

/// The design a picture shows while no project is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlateStyle {
    /// Soft colour bands, everything centred (« Aurore »).
    #[default]
    Aurora,
    /// Tiles; the channel tile is a hole (transparent).
    Bento,
    /// A colour mesh (« Dégradé vif »).
    Gradient,
    /// Corner marks and a translucent card, over transparency (« Cadre »).
    Frame,
    /// Slanted colour lines (« Lignes »).
    Lines,
    /// A colour panel beside the format (« Partagé »).
    Split,
    /// The user's picture, or nothing.
    Custom,
    /// Nothing: fully transparent.
    Transparent,
}

/// The signature's width and its quill above the letters, in units of
/// `slate_signature.svg` (its letters are 104 high).
const SIG_W: f32 = 910.0;
const QUILL: f32 = 31.2;

/// What a channel picture shows while no project is loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Slate {
    pub style: SlateStyle,
    /// The picture of [`SlateStyle::Custom`] (`/assets/…`, "" = nothing).
    pub image: String,
    /// Written on the built-in designs.
    pub channel: String,
    pub width: u32,
    pub height: u32,
    /// Frame rate (num, den).
    pub rate: (u32, u32),
}

/// "16:9", "9:16", "4:3"; the usual name of a near shape ("21:9" for
/// 2560 × 1080, "17:9" for 4096 × 2160); else "2.39:1".
pub fn ratio_label(width: u32, height: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }
    let (w, h) = (width.max(1), height.max(1));
    let g = gcd(w, h);
    let (a, b) = (w / g, h / g);
    if a <= 32 && b <= 32 {
        return format!("{a}:{b}");
    }
    let r = w as f64 / h as f64;
    const USUAL: [(u32, u32); 6] = [(16, 9), (4, 3), (21, 9), (17, 9), (3, 2), (5, 4)];
    for (x, y) in USUAL {
        for (p, q) in [(x, y), (y, x)] {
            if (r * q as f64 / p as f64 - 1.0).abs() < 0.02 {
                return format!("{p}:{q}");
            }
        }
    }
    if r >= 1.0 {
        format!("{r:.2}:1")
    } else {
        format!("1:{:.2}", 1.0 / r)
    }
}

/// "50p", "59.94p" (channel formats are progressive).
pub fn rate_label(num: u32, den: u32) -> String {
    let fps = format!("{:.2}", num as f64 / den.max(1) as f64);
    format!("{}p", fps.trim_end_matches('0').trim_end_matches('.'))
}

impl Renderer {
    /// What a picture shows while no project is loaded, at the `out` size:
    /// a built-in design, the custom picture (filling the frame), or nothing.
    pub fn render_idle(&mut self, slate: &Slate, out: OutputFormat) -> Frame {
        let (w, h) = (out.width.max(1), out.height.max(1));
        if slate.style == SlateStyle::Transparent {
            return Frame::blank(w, h);
        }
        if slate.style == SlateStyle::Custom {
            return match slate.image.trim() {
                "" => Frame::blank(w, h),
                src => self.render_slate(src, out),
            };
        }
        self.refresh_fonts();
        let Some(mut p) = Painter::new(&mut self.fonts, w, h) else {
            return Frame::blank(w, h);
        };
        p.draw(slate);
        Frame {
            width: w,
            height: h,
            data: p.pm.take(),
            captured_at: SystemTime::now(),
        }
    }
}

// ---------------------------------------------------------------- drawing --

/// sRGB colour (0..=1) and alpha.
#[derive(Debug, Clone, Copy)]
struct C([f32; 4]);

const fn hex(v: u32) -> C {
    C([
        ((v >> 16) & 0xff) as f32 / 255.0,
        ((v >> 8) & 0xff) as f32 / 255.0,
        (v & 0xff) as f32 / 255.0,
        1.0,
    ])
}

const fn white(a: f32) -> C {
    C([1.0, 1.0, 1.0, a])
}

impl C {
    fn alpha(self, a: f32) -> C {
        let [r, g, b, _] = self.0;
        C([r, g, b, a])
    }
    fn rgb(self) -> [f32; 3] {
        [self.0[0], self.0[1], self.0[2]]
    }
    fn sk(self) -> tiny_skia::Color {
        let [r, g, b, a] = self.0.map(|v| v.clamp(0.0, 1.0));
        tiny_skia::Color::from_rgba(r, g, b, a).unwrap_or(tiny_skia::Color::TRANSPARENT)
    }
    /// `hsl(h, s, l)`, as CSS.
    fn hsl(h: f32, s: f32, l: f32) -> C {
        let a = s * l.min(1.0 - l);
        let f = |n: f32| {
            let k = (n + h / 30.0) % 12.0;
            l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
        };
        C([f(0.0), f(8.0), f(4.0), 1.0])
    }
}

fn solid(c: C) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.sk());
    p.anti_alias = true;
    p
}

fn shaded(shader: Shader<'static>) -> Paint<'static> {
    Paint {
        shader,
        anti_alias: true,
        ..Default::default()
    }
}

/// A CSS `linear-gradient(<deg>deg, …)` over a box.
fn css_linear(b: Bx, deg: f32, stops: &[(f32, C)]) -> Option<Shader<'static>> {
    let a = deg.to_radians();
    let (dx, dy) = (a.sin(), -a.cos());
    let len = (b.w * dx).abs() + (b.h * dy).abs();
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    LinearGradient::new(
        Point::from_xy(cx - dx * len / 2.0, cy - dy * len / 2.0),
        Point::from_xy(cx + dx * len / 2.0, cy + dy * len / 2.0),
        stops
            .iter()
            .map(|(p, c)| GradientStop::new(*p, c.sk()))
            .collect(),
        SpreadMode::Pad,
        Transform::identity(),
    )
}

/// A box (x, y, width, height).
#[derive(Debug, Clone, Copy)]
struct Bx {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Bx {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    /// The box inside a padding.
    fn inset(self, d: f32) -> Self {
        Self::new(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }
    fn bottom(self) -> f32 {
        self.y + self.h
    }
}

fn rrect(b: Bx, radius: f32) -> Option<tiny_skia::Path> {
    if b.w <= 0.0 || b.h <= 0.0 {
        return None;
    }
    let r = radius.clamp(0.0, b.w.min(b.h) / 2.0);
    let k = r * (1.0 - 0.552_284_8);
    let (x0, y0, x1, y1) = (b.x, b.y, b.x + b.w, b.y + b.h);
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - k, y0, x1, y0 + k, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - k, x1 - k, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + k, y1, x0, y1 - k, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + k, x0 + k, y0, x0 + r, y0);
    pb.close();
    pb.finish()
}

/// The top-left corner mark of the Frame design: a `size` square's top and
/// left borders (`bw` thick), its outer corner rounded.
fn corner_mark(size: f32, bw: f32, radius: f32) -> Option<tiny_skia::Path> {
    let (r, ri) = (radius, (radius - bw).max(0.0));
    let (k, ki) = (r * 0.552_284_8, ri * 0.552_284_8);
    let mut pb = PathBuilder::new();
    pb.move_to(size, 0.0);
    pb.line_to(r, 0.0);
    pb.cubic_to(r - k, 0.0, 0.0, r - k, 0.0, r);
    pb.line_to(0.0, size);
    pb.line_to(bw, size);
    pb.line_to(bw, bw + ri);
    pb.cubic_to(bw, bw + ri - ki, bw + ri - ki, bw, bw + ri, bw);
    pb.line_to(size, bw);
    pb.close();
    pb.finish()
}

/// Share of a Gaussian blur (σ = 1) of a half-plane at signed distance `s`
/// (positive outside): Φ(−s), from erfc (Abramowitz & Stegun 7.1.26).
fn soft_edge(s: f32) -> f32 {
    let x = s.abs() / std::f32::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_6
            + t * (-0.284_496_7 + t * (1.421_413_7 + t * (-1.453_152 + t * 1.061_405_4))));
    let tail = 0.5 * poly * (-x * x).exp();
    if s > 0.0 {
        tail
    } else {
        1.0 - tail
    }
}

/// A blurred ellipse with a left-to-right gradient (the Aurora bands).
struct Blob {
    cx: f32,
    cy: f32,
    /// Semi-axes.
    a: f32,
    b: f32,
    cos: f32,
    sin: f32,
    from: [f32; 3],
    to: [f32; 3],
    opacity: f32,
}

impl Blob {
    fn new(b: Bx, deg: f32, from: C, to: C, opacity: f32) -> Self {
        let r = deg.to_radians();
        Self {
            cx: b.x + b.w / 2.0,
            cy: b.y + b.h / 2.0,
            a: b.w / 2.0,
            b: b.h / 2.0,
            cos: r.cos(),
            sin: r.sin(),
            from: from.rgb(),
            to: to.rgb(),
            opacity,
        }
    }

    /// Coverage (blur σ) and colour at (x, y).
    fn at(&self, x: f32, y: f32, sigma: f32) -> (f32, [f32; 3]) {
        let (dx, dy) = (x - self.cx, y - self.cy);
        // Into the ellipse's frame (it is rotated by `deg`, clockwise).
        let lx = self.cos * dx + self.sin * dy;
        let ly = -self.sin * dx + self.cos * dy;
        let (nx, ny) = (lx / (self.a * self.a), ly / (self.b * self.b));
        let k = ((lx / self.a).powi(2) + (ly / self.b).powi(2)).sqrt();
        let g = (nx * nx + ny * ny).sqrt();
        // Distance to the outline (first order), negative inside.
        let s = if g > 1e-9 {
            (k - 1.0) * k / g
        } else {
            -self.a.min(self.b)
        };
        let t = ((lx + self.a) / (2.0 * self.a)).clamp(0.0, 1.0);
        let c = [0, 1, 2].map(|i| self.from[i] + (self.to[i] - self.from[i]) * t);
        (soft_edge(s / sigma) * self.opacity, c)
    }
}

/// A text style: size, weight, letter spacing (em), tabular figures.
#[derive(Debug, Clone, Copy)]
struct Type {
    size: f32,
    weight: u16,
    spacing: f32,
    tnum: bool,
}

const fn ty(size: f32, weight: u16) -> Type {
    Type {
        size,
        weight,
        spacing: 0.0,
        tnum: false,
    }
}

impl Type {
    /// With letter spacing, in em.
    const fn spaced(self, em: f32) -> Type {
        Type {
            spacing: em,
            ..self
        }
    }
    /// With tabular figures.
    const fn tabular(self) -> Type {
        Type { tnum: true, ..self }
    }
}

/// A line of text: its glyphs, width (CSS: letter spacing after every
/// letter) and font metrics.
struct Run {
    chain: Chain,
    glyphs: Vec<PGlyph>,
    size: f32,
    w: f32,
    ascent: f32,
    descent: f32,
}

impl Run {
    /// CSS `line-height: normal`.
    fn normal(&self) -> f32 {
        self.ascent - self.descent
    }
}

/// The app's logo, « Strata Graphics » (web/src/ui/Brand.tsx): « Strata »
/// white, the module in the accent colour, lighter, on the same line.
struct Wordmark {
    strata: Run,
    module: Run,
    gap: f32,
    size: f32,
}

impl Wordmark {
    fn w(&self) -> f32 {
        self.strata.w + self.gap + self.module.w
    }
}

/// The module of Strata this is, and the interface's accent colour.
const MODULE: &str = "Graphics";
const ACCENT: C = hex(0x8fa3ff);

/// The spec pills (size, ratio, rate).
struct Pills {
    runs: Vec<Run>,
    /// Width of each pill.
    widths: Vec<f32>,
    /// Height of a pill, space between two.
    h: f32,
    gap: f32,
}

impl Pills {
    fn one_line(&self) -> f32 {
        self.widths.iter().sum::<f32>() + self.gap * (self.widths.len().max(1) - 1) as f32
    }

    /// The pills in rows no wider than `max_w`: (first, end) of each row,
    /// and the size of the box.
    fn rows(&self, max_w: f32) -> (Vec<(usize, usize)>, f32, f32) {
        let mut rows = Vec::new();
        let (mut start, mut x, mut widest) = (0, 0.0_f32, 0.0_f32);
        for (i, w) in self.widths.iter().enumerate() {
            let next = if i == start { *w } else { x + self.gap + w };
            if i > start && next > max_w {
                rows.push((start, i));
                widest = widest.max(x);
                start = i;
                x = *w;
            } else {
                x = next;
            }
        }
        rows.push((start, self.widths.len()));
        widest = widest.max(x);
        let n = rows.len() as f32;
        (rows, widest, n * self.h + (n - 1.0) * self.gap)
    }
}

struct Painter<'a> {
    pm: Pixmap,
    fonts: &'a mut FontStore,
    /// Inter 400, 500, 600, 700, 800.
    inter: [FontArc; 5],
    w: f32,
    h: f32,
    /// 1 cqmin: a hundredth of the short side.
    u: f32,
    /// The references' 1 px borders (they were drawn 540 px high).
    px: f32,
}

impl<'a> Painter<'a> {
    fn new(fonts: &'a mut FontStore, w: u32, h: u32) -> Option<Self> {
        let [Some(a), Some(b), Some(c), Some(d), Some(e)] =
            [400, 500, 600, 700, 800].map(|wt| fonts.font("Inter", wt))
        else {
            return None;
        };
        let u = w.min(h) as f32 / 100.0;
        Some(Self {
            pm: Pixmap::new(w, h)?,
            fonts,
            inter: [a, b, c, d, e],
            w: w as f32,
            h: h as f32,
            u,
            px: (h as f32 / 540.0).max(1.0),
        })
    }

    fn draw(&mut self, s: &Slate) {
        let specs = [
            format!("{} × {}", s.width, s.height),
            ratio_label(s.width, s.height),
            rate_label(s.rate.0, s.rate.1),
        ];
        match s.style {
            SlateStyle::Aurora | SlateStyle::Custom => self.aurora(s, &specs),
            SlateStyle::Bento => self.bento(s, &specs),
            SlateStyle::Gradient => self.gradient(s, &specs),
            SlateStyle::Frame => self.frame(s, &specs),
            SlateStyle::Lines => self.lines(s, &specs),
            SlateStyle::Split => self.split(s, &specs),
            SlateStyle::Transparent => {}
        }
    }

    fn portrait(&self) -> bool {
        self.w <= self.h
    }

    // ------------------------------------------------------------ text --

    fn run(&mut self, s: &str, t: Type) -> Run {
        let main = self.inter[match t.weight {
            ..=400 => 0,
            401..=500 => 1,
            501..=600 => 2,
            601..=700 => 3,
            _ => 4,
        }]
        .clone();
        let chain = Chain::new(self.fonts, main.clone(), s);
        let ls = t.spacing * t.size;
        let (mut glyphs, mut w) = text::glyph_run(&chain, s, t.size, ls, 0.0);
        if t.tnum {
            // Every figure as wide (Inter's tabular figures: about the
            // median width), centred in it.
            let mut digits: Vec<f32> = text::glyph_run(&chain, "0123456789", t.size, 0.0, 0.0)
                .0
                .iter()
                .map(|g| g.adv)
                .collect();
            digits.sort_by(f32::total_cmp);
            let tab = (digits[4] + digits[5]) / 2.0;
            let mut shift = 0.0;
            for g in &mut glyphs {
                g.x += shift;
                if g.ch.is_ascii_digit() {
                    g.x += (tab - g.adv) / 2.0;
                    shift += tab - g.adv;
                }
            }
            w += shift;
        }
        let (ascent, descent) = text::metrics(&main, t.size);
        let w = if glyphs.is_empty() { 0.0 } else { w + ls };
        Run {
            chain,
            glyphs,
            size: t.size,
            w,
            ascent,
            descent,
        }
    }

    /// A run made smaller when wider than `max_w` (a long channel name).
    fn fit(&mut self, s: &str, t: Type, max_w: f32) -> Run {
        let r = self.run(s, t);
        if r.w <= max_w || max_w <= 0.0 {
            return r;
        }
        let size = t.size * max_w / r.w * 0.99;
        self.run(s, Type { size, ..t })
    }

    /// Lines of `s` broken at spaces to fit `max_w`.
    fn wrap(&mut self, s: &str, t: Type, max_w: f32) -> Vec<Run> {
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in s.split(' ') {
            let next = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if !cur.is_empty() && self.run(&next, t).w > max_w {
                lines.push(std::mem::replace(&mut cur, word.to_string()));
            } else {
                cur = next;
            }
        }
        lines.push(cur);
        lines.iter().map(|l| self.fit(l, t, max_w)).collect()
    }

    /// « Strata Graphics » `size` high, smaller when wider than `max_w`.
    fn wordmark(&mut self, size: f32, max_w: f32) -> Wordmark {
        let at = |p: &mut Self, size: f32| Wordmark {
            strata: p.run("Strata", ty(size, 800).spaced(-0.04)),
            module: p.run(MODULE, ty(size, 400).spaced(-0.02)),
            gap: 0.25 * size,
            size,
        };
        let m = at(self, size);
        if m.w() <= max_w || max_w <= 0.0 {
            return m;
        }
        at(self, size * max_w / m.w() * 0.99)
    }

    /// Draws a wordmark (line height 1) whose top left is (x, top).
    fn draw_wordmark(&mut self, m: &Wordmark, x: f32, top: f32) {
        self.text(&m.strata, x, top, m.size, white(1.0));
        self.text(&m.module, x + m.strata.w + m.gap, top, m.size, ACCENT);
    }

    /// Draws `r` centred in a line box `line_h` high whose top left is (x, top).
    fn text(&mut self, r: &Run, x: f32, top: f32, line_h: f32, color: C) {
        let base = top + (line_h - r.normal()) / 2.0 + r.ascent;
        let mut pb = PathBuilder::new();
        text::run_outline(&r.chain, &r.glyphs, r.size, x, base, &mut pb);
        if let Some(p) = pb.finish() {
            let paint = solid(color);
            self.pm
                .fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    // ---------------------------------------------------------- shapes --

    fn fill(&mut self, path: Option<tiny_skia::Path>, paint: &Paint) {
        if let Some(p) = path {
            self.pm
                .fill_path(&p, paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    /// A CSS border (1 px of the references) inside the box.
    fn border(&mut self, b: Bx, radius: f32, color: C) {
        let d = self.px / 2.0;
        if let Some(p) = rrect(
            Bx::new(b.x + d, b.y + d, b.w - self.px, b.h - self.px),
            radius - d,
        ) {
            let stroke = Stroke {
                width: self.px,
                ..Default::default()
            };
            self.pm
                .stroke_path(&p, &solid(color), &stroke, Transform::identity(), None);
        }
    }

    /// A tile: rounded, filled, bordered.
    fn tile(&mut self, b: Bx, radius: f32, fill: &Paint, border: C) {
        self.fill(rrect(b, radius), fill);
        self.border(b, radius, border);
    }

    /// Fills every pixel with `f(x, y)` (sRGB, opaque), dithered so soft
    /// gradients do not band.
    fn shade(&mut self, f: impl Fn(f32, f32) -> [f32; 3]) {
        let w = self.pm.width() as usize;
        for (i, px) in self
            .pm
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            let c = f(x + 0.5, y + 0.5);
            // Interleaved gradient noise, ±½ step.
            let n = (52.982_918 * (0.067_110_56 * x + 0.005_837_15 * y).fract()).fract() - 0.5;
            for (o, v) in px.iter_mut().zip(c) {
                *o = (v * 255.0 + n).round().clamp(0.0, 255.0) as u8;
            }
            px[3] = 255;
        }
    }

    // ------------------------------------------------------ components --

    /// The F AudioVisuel signature, its letters `h` high, their top left at
    /// (x, y); the F's quill rises above.
    fn signature(&mut self, x: f32, y: f32, h: f32, opacity: f32) {
        const SVG: &str = include_str!("slate_signature.svg");
        let Ok(tree) = resvg::usvg::Tree::from_str(SVG, &resvg::usvg::Options::default()) else {
            return;
        };
        let k = h / 104.0;
        let y = y - QUILL * k;
        let (x0, y0) = (x.floor(), y.floor());
        let Some(mut sig) = Pixmap::new(
            (SIG_W * k + 2.0).ceil() as u32,
            ((104.0 + QUILL) * k + 2.0).ceil() as u32,
        ) else {
            return;
        };
        resvg::render(
            &tree,
            Transform::from_translate(x - x0, y - y0).pre_scale(k, k),
            &mut sig.as_mut(),
        );
        let paint = PixmapPaint {
            opacity,
            ..Default::default()
        };
        self.pm.draw_pixmap(
            x0 as i32,
            y0 as i32,
            sig.as_ref(),
            &paint,
            Transform::identity(),
            None,
        );
    }

    /// Width of the signature `h` high.
    fn sig_w(h: f32) -> f32 {
        h * SIG_W / 104.0
    }

    /// The signature at the bottom of a box, centred (an SVG stretched
    /// across a flex column), smaller when the box is narrow.
    fn signature_in(&mut self, b: Bx, h: f32, opacity: f32) {
        let h = h.min(b.w * 104.0 / SIG_W);
        let x = b.x + (b.w - Self::sig_w(h)) / 2.0;
        self.signature(x, b.bottom() - h, h, opacity);
    }

    fn pills(&mut self, specs: &[String; 3]) -> Pills {
        let u = self.u;
        let runs: Vec<Run> = specs
            .iter()
            .map(|s| self.run(s, ty(2.4 * u, 600).tabular()))
            .collect();
        Pills {
            widths: runs.iter().map(|r| r.w + 3.6 * u + 2.0 * self.px).collect(),
            runs,
            h: 4.6 * u + 2.0 * self.px,
            gap: u,
        }
    }

    /// The pills wrapped at `max_w`, rows from (x, y), left aligned.
    fn draw_pills(&mut self, p: &Pills, x: f32, y: f32, max_w: f32, fill: C) {
        let u = self.u;
        let (rows, _, _) = p.rows(max_w);
        for (row, (start, end)) in rows.into_iter().enumerate() {
            let top = y + row as f32 * (p.h + p.gap);
            let mut left = x;
            for i in start..end {
                let b = Bx::new(left, top, p.widths[i], p.h);
                self.tile(b, p.h / 2.0, &solid(fill), white(0.18));
                // Text: line height 1.
                let (tx, ty) = (left + self.px + 1.8 * u, top + self.px + 1.1 * u);
                self.text(&p.runs[i], tx, ty, 2.4 * u, white(1.0));
                left += p.widths[i] + p.gap;
            }
        }
    }

    /// The pills as a flex item no wider than `max_w`, centred in a column
    /// `col_w` wide from `x` (rows start at the column's left once they wrap).
    fn pills_centred(&mut self, p: &Pills, x: f32, col_w: f32, y: f32, max_w: f32, fill: C) {
        let (_, w, _) = p.rows(max_w);
        let w = if p.one_line() <= max_w { w } else { max_w };
        self.draw_pills(p, x + (col_w - w) / 2.0, y, max_w, fill);
    }

    // --------------------------------------------------------- designs --

    /// 1 · Aurore: soft colour bands, everything centred.
    fn aurora(&mut self, s: &Slate, specs: &[String; 3]) {
        let (w, h, u) = (self.w, self.h, self.u);
        let (cw, ch) = (w / 100.0, h / 100.0);
        let blobs = [
            Blob::new(
                Bx::new(-20.0 * cw, 8.0 * ch, 120.0 * cw, 40.0 * ch),
                -8.0,
                hex(0x2b6cff),
                hex(0x7c4dff),
                0.55,
            ),
            Blob::new(
                Bx::new(10.0 * cw, 38.0 * ch, 100.0 * cw, 30.0 * ch),
                6.0,
                hex(0x00d4c8),
                hex(0x2b6cff),
                0.35,
            ),
            Blob::new(
                Bx::new(50.0 * cw, 80.0 * ch, 70.0 * cw, 25.0 * ch),
                0.0,
                hex(0xff3b7a),
                hex(0xff3b7a),
                0.3,
            ),
        ];
        let bg = hex(0x06070d).rgb();
        let sigma = 7.0 * u;
        self.shade(|x, y| {
            let mut c = bg;
            for b in &blobs {
                // mix-blend-mode: screen.
                let (a, col) = b.at(x, y, sigma);
                for (v, s) in c.iter_mut().zip(col) {
                    *v += a * s * (1.0 - *v);
                }
            }
            c
        });

        let word = self.wordmark(10.0 * u, w - 16.0 * u);
        let name = self.fit(&s.channel, ty(4.0 * u, 500), w - 10.0 * u);
        let pills = self.pills(specs);
        let (_, _, ph) = pills.rows(w - 10.0 * u);
        let total = word.size + 2.4 * u + name.normal() + 2.4 * u + ph;
        let mut y = (h - total) / 2.0;
        self.draw_wordmark(&word, (w - word.w()) / 2.0, y);
        y += word.size + 2.4 * u;
        self.text(&name, (w - name.w) / 2.0, y, name.normal(), white(0.85));
        y += name.normal() + 2.4 * u;
        self.pills_centred(&pills, 0.0, w, y, w - 10.0 * u, white(0.1));
        let sh = 3.0 * u;
        self.signature((w - Self::sig_w(sh)) / 2.0, h - 5.0 * u - sh, sh, 0.7);
    }

    /// A Bento tile's label and value, from the tile's bottom (`u`: the
    /// design's unit).
    fn bento_label(&mut self, b: Bx, u: f32, label: &str, value: (&str, f32), label_color: C) {
        let (value, value_size) = value;
        let inner = b.inset(self.px + 3.0 * u);
        let small = self.fit(
            &label.to_uppercase(),
            ty(1.9 * u, 600).spaced(0.14),
            inner.w,
        );
        let lines = if value_size > 5.0 * u {
            vec![self.fit(value, ty(value_size, 700).tabular(), inner.w)]
        } else {
            self.wrap(value, ty(value_size, 700).tabular(), inner.w)
        };
        let mut y = inner.bottom() - lines.iter().map(Run::normal).sum::<f32>();
        let label_top = y - u - small.normal();
        for l in &lines {
            self.text(l, inner.x, y, l.normal(), white(1.0));
            y += l.normal();
        }
        self.text(&small, inner.x, label_top, small.normal(), label_color);
    }

    /// 2 · Bento: tiles; the channel tile is a hole (transparent).
    fn bento(&mut self, s: &Slate, specs: &[String; 3]) {
        let (w, h) = (self.w, self.h);
        let bg = solid(hex(0x0b0c12));
        self.fill(rrect(Bx::new(0.0, 0.0, w, h), 0.0), &bg);
        // As the approved reference: a margin of a fifteenth of the height,
        // sizes in hundredths of the short side of what is inside it.
        let area = Bx::new(0.0, 0.0, w, h).inset(h / 15.0);
        let u = area.w.min(area.h) / 100.0;
        let gap = 1.6 * u;
        // (big, channel, size, ratio, rate)
        let (big, chan, cells) = if self.portrait() {
            let fx = (area.w - 2.0 * gap) / 3.0;
            let fy = (area.h - 2.0 * gap) / 4.4;
            let big = Bx::new(area.x, area.y, area.w, 2.4 * fy);
            let chan = Bx::new(area.x, big.bottom() + gap, area.w, fy);
            let y = chan.bottom() + gap;
            let cell = |i: f32| Bx::new(area.x + i * (fx + gap), y, fx, fy);
            (big, chan, [cell(0.0), cell(1.0), cell(2.0)])
        } else {
            let fx = (area.w - 3.0 * gap) / 5.0;
            let fy = (area.h - gap) / 2.2;
            let big = Bx::new(area.x, area.y, 2.0 * fx, area.h);
            let x = big.x + big.w + gap;
            let chan = Bx::new(x, area.y, 3.0 * fx + 2.0 * gap, 1.2 * fy);
            let y = chan.bottom() + gap;
            let cell = |i: f32| Bx::new(x + i * (fx + gap), y, fx, fy);
            (big, chan, [cell(0.0), cell(1.0), cell(2.0)])
        };
        let radius = 2.4 * u;
        let line = hex(0x232741);

        // « Strata » and the signature.
        if let Some(g) = css_linear(big, 150.0, &[(0.0, hex(0x3a3fd9)), (0.7, hex(0x14163a))]) {
            self.tile(big, radius, &shaded(g), line);
        }
        let inner = big.inset(self.px + 3.0 * u);
        let word = self.wordmark(8.5 * u, inner.w);
        self.draw_wordmark(&word, inner.x, inner.y);
        self.signature_in(inner, 3.0 * u, 0.7);

        // The channel: a hole in the slate, its border kept.
        let clear = Paint {
            blend_mode: BlendMode::Clear,
            anti_alias: true,
            ..Default::default()
        };
        self.fill(rrect(chan, radius), &clear);
        self.border(chan, radius, line);
        self.bento_label(chan, u, "Canal", (&s.channel, 6.0 * u), hex(0x7d84a6));

        let tile = solid(hex(0x151826));
        for (i, (label, value)) in ["Résolution", "Ratio", "Cadence"]
            .iter()
            .zip(specs)
            .enumerate()
        {
            let accent = i == 2;
            if accent {
                self.tile(cells[i], radius, &solid(hex(0xff3b5c)), hex(0xff3b5c));
            } else {
                self.tile(cells[i], radius, &tile, line);
            }
            let label_color = if accent { white(0.8) } else { hex(0x7d84a6) };
            self.bento_label(cells[i], u, label, (value, 4.2 * u), label_color);
        }
    }

    /// 3 · Dégradé vif: a colour mesh, « Strata » top left.
    fn gradient(&mut self, s: &Slate, specs: &[String; 3]) {
        let (w, h, u) = (self.w, self.h, self.u);
        // radial-gradient(rx ry at cx cy, colour, transparent 70%), first on
        // top, over a box 10 % larger than the picture on every side, then
        // blurred (4 cqmin): the end of each ramp is rounded off.
        let (bx, by, bw, bh) = (-0.1 * w, -0.1 * h, 1.2 * w, 1.2 * h);
        let blur = 4.0 * u;
        let layers = [
            (0.4, 0.4, 0.1, 0.9, hex(0xffb347)),
            (0.5, 0.6, 0.6, 0.9, hex(0x00c2ff)),
            (0.45, 0.55, 0.8, 0.2, hex(0x6a5cff)),
            (0.4, 0.5, 0.2, 0.3, hex(0xff4d6d)),
        ]
        .map(|(rx, ry, cx, cy, c)| {
            let (rx, ry) = (rx * bw, ry * bh);
            let soft = blur / (rx * ry).sqrt();
            (bx + cx * bw, by + cy * bh, rx, ry, soft, c.rgb())
        });
        let bg = hex(0x120a2a).rgb();
        self.shade(|x, y| {
            // The mesh over transparency (premultiplied)…
            let (mut p, mut a) = ([0.0_f32; 3], 0.0_f32);
            for (cx, cy, rx, ry, soft, c) in &layers {
                let t = (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt();
                // max(0, 0.7 - t) / 0.7 under a Gaussian blur.
                let d = (0.7 - t) / soft;
                let phi = (-0.5 * d * d).exp() * 0.398_942_3;
                let al = (soft * (phi + d * soft_edge(-d)) / 0.7).min(1.0);
                for (v, s) in p.iter_mut().zip(c) {
                    *v = s * al + *v * (1.0 - al);
                }
                a = al + a * (1.0 - al);
            }
            if a <= 0.0 {
                return bg;
            }
            // …saturate(1.2), then over the background.
            let [r, g, b] = p.map(|v| v / a);
            let k = 1.2;
            let sat = [
                (0.213 + 0.787 * k) * r + (0.715 - 0.715 * k) * g + (0.072 - 0.072 * k) * b,
                (0.213 - 0.213 * k) * r + (0.715 + 0.285 * k) * g + (0.072 - 0.072 * k) * b,
                (0.213 - 0.213 * k) * r + (0.715 - 0.715 * k) * g + (0.072 + 0.928 * k) * b,
            ];
            [0, 1, 2].map(|i| sat[i].clamp(0.0, 1.0) * a + bg[i] * (1.0 - a))
        });

        let word = self.wordmark(10.5 * u, w - 12.0 * u);
        self.draw_wordmark(&word, 6.0 * u, 6.0 * u);
        let sh = 3.0 * u;
        let sig_x = w - 5.0 * u - Self::sig_w(sh);
        self.signature(sig_x, h - 5.0 * u - sh, sh, 0.9);
        let max_w = w - 12.0 * u;
        let pills = self.pills(specs);
        let (_, _, ph) = pills.rows(max_w);
        let pills_top = h - 6.0 * u - ph;
        self.draw_pills(&pills, 6.0 * u, pills_top, max_w, C([0.0, 0.0, 0.0, 0.25]));
        let name = self.fit(&s.channel, ty(6.0 * u, 700), max_w);
        let name_top = pills_top - 2.0 * u - name.normal();
        self.text(&name, 6.0 * u, name_top, name.normal(), white(1.0));
    }

    /// 5 · Cadre: corner marks and a translucent card, over transparency.
    fn frame(&mut self, s: &Slate, specs: &[String; 3]) {
        let (w, h, u) = (self.w, self.h, self.u);
        if let Some(mark) = corner_mark(7.0 * u, 0.6 * u, 2.0 * u) {
            let m = 5.0 * u;
            let paint = solid(white(0.85));
            for (x, y, sx, sy) in [
                (m, m, 1.0, 1.0),
                (w - m, m, -1.0, 1.0),
                (m, h - m, 1.0, -1.0),
                (w - m, h - m, -1.0, -1.0),
            ] {
                let tf = Transform::from_row(sx, 0.0, 0.0, sy, x, y);
                self.pm
                    .fill_path(&mark, &paint, FillRule::Winding, tf, None);
            }
        }

        // The card is centred, at most half the picture wide.
        let max_w = (0.5 * w - 12.0 * u - 2.0 * self.px).max(u);
        let size = if self.portrait() { 5.0 * u } else { 6.5 * u };
        let word = self.wordmark(size, max_w);
        let name = self.fit(&s.channel, ty(3.8 * u, 500), max_w);
        let pills = self.pills(specs);
        let sh = 2.6 * u;
        let content_w = word
            .w()
            .max(name.w)
            .max(pills.one_line())
            .max(Self::sig_w(sh))
            .min(max_w);
        let (_, _, ph) = pills.rows(content_w);
        let content_h = word.size + 2.0 * u + name.normal() + 2.0 * u + ph + 3.0 * u + sh;
        let (card_w, card_h) = (
            content_w + 12.0 * u + 2.0 * self.px,
            content_h + 8.0 * u + 2.0 * self.px,
        );
        let card = Bx::new((w - card_w) / 2.0, (h - card_h) / 2.0, card_w, card_h);
        self.tile(card, 3.0 * u, &solid(hex(0x0a0b12).alpha(0.7)), white(0.14));
        let x = card.x + self.px + 6.0 * u;
        let mut y = card.y + self.px + 4.0 * u;
        self.draw_wordmark(&word, x + (content_w - word.w()) / 2.0, y);
        y += word.size + 2.0 * u;
        self.text(
            &name,
            x + (content_w - name.w) / 2.0,
            y,
            name.normal(),
            hex(0xd6d9ea),
        );
        y += name.normal() + 2.0 * u;
        self.pills_centred(&pills, x, content_w, y, content_w, white(0.1));
        y += ph + 3.0 * u;
        self.signature(x + (content_w - Self::sig_w(sh)) / 2.0, y, sh, 0.7);
    }

    /// 7 · Lignes: slanted colour lines, « Strata » bottom left.
    fn lines(&mut self, s: &Slate, specs: &[String; 3]) {
        let (w, h, u) = (self.w, self.h, self.u);
        self.fill(rrect(Bx::new(0.0, 0.0, w, h), 0.0), &solid(hex(0x08090f)));
        // 24 lines, centred, then skewY(-8deg) around the centre.
        let (thick, step) = (0.35 * u, 1.75 * u);
        let top = (h - (24.0 * thick + 23.0 * 1.4 * u)) / 2.0;
        let ky = (-8.0_f32).to_radians().tan();
        let tf = Transform::from_row(1.0, ky, 0.0, 1.0, 0.0, -ky * w / 2.0);
        for k in 0..24 {
            let kf = k as f32;
            let c = C::hsl(220.0 + kf * 4.0, 0.9, 0.65).alpha(0.15 + kf * 0.025);
            let stops = [
                (0.0, c.alpha(0.0)),
                (0.2 + kf * 0.02, c),
                (1.0, c.alpha(0.0)),
            ];
            let line = Bx::new(0.0, top + kf * step, w, thick);
            let (Some(g), Some(r)) = (
                css_linear(line, 90.0, &stops),
                tiny_skia::Rect::from_xywh(line.x, line.y, line.w, line.h),
            ) else {
                continue;
            };
            self.pm.fill_rect(r, &shaded(g), tf, None);
        }

        let sh = 3.0 * u;
        self.signature(w - 7.0 * u - Self::sig_w(sh), 6.0 * u, sh, 0.7);
        let avail = w - 14.0 * u;
        let name = self.fit(&s.channel, ty(4.4 * u, 500), avail);
        let pills = self.pills(specs);
        // The name and the pills on one row when they fit, else two.
        let one_row = name.w + 3.0 * u + pills.one_line() <= avail;
        let (_, _, ph) = pills.rows(avail);
        let bottom = h - 9.0 * u;
        let row_top = if one_row {
            let row_h = name.normal().max(ph);
            let top = bottom - row_h;
            self.text(&name, 7.0 * u, top, row_h, hex(0xcfd5ff));
            let px = 7.0 * u + name.w + 3.0 * u;
            self.draw_pills(&pills, px, top + (row_h - ph) / 2.0, avail, white(0.1));
            top
        } else {
            let top = bottom - ph - 3.0 * u - name.normal();
            self.text(&name, 7.0 * u, top, name.normal(), hex(0xcfd5ff));
            self.draw_pills(&pills, 7.0 * u, bottom - ph, avail, white(0.1));
            top
        };
        let word = self.wordmark(9.0 * u, avail);
        self.draw_wordmark(&word, 7.0 * u, row_top - 2.4 * u - word.size);
    }

    /// 10 · Partagé: a colour panel beside the format (above it when portrait).
    fn split(&mut self, s: &Slate, specs: &[String; 3]) {
        let (w, h, u) = (self.w, self.h, self.u);
        self.fill(rrect(Bx::new(0.0, 0.0, w, h), 0.0), &solid(hex(0x0b0c12)));
        let portrait = self.portrait();
        let (left, right) = if portrait {
            let lh = h * 1.2 / 2.2;
            (Bx::new(0.0, 0.0, w, lh), Bx::new(0.0, lh, w, h - lh))
        } else {
            let lw = w * 1.2 / 2.2;
            (Bx::new(0.0, 0.0, lw, h), Bx::new(lw, 0.0, w - lw, h))
        };
        if let Some(g) = css_linear(left, 160.0, &[(0.0, hex(0x3b3fe0)), (1.0, hex(0x1b1d6e))]) {
            self.fill(rrect(left, 0.0), &shaded(g));
        }
        let inner = left.inset(6.0 * u);
        let word = self.wordmark(8.0 * u, inner.w);
        self.draw_wordmark(&word, inner.x, inner.y);
        let sh = 3.0 * u;
        self.signature_in(inner, sh, 0.85);
        let name = self.fit(&s.channel, ty(6.0 * u, 700).spaced(-0.02), inner.w);
        let name_top = inner.bottom() - sh - 3.0 * u - name.normal();
        self.text(&name, inner.x, name_top, name.normal(), white(1.0));

        let inner = right.inset(6.0 * u);
        let kvs: Vec<(Run, Run)> = ["Résolution", "Ratio", "Cadence"]
            .iter()
            .zip(specs)
            .map(|(k, v)| {
                (
                    self.fit(&k.to_uppercase(), ty(2.0 * u, 600).spaced(0.16), inner.w),
                    self.fit(v, ty(6.0 * u, 700).spaced(-0.02).tabular(), inner.w),
                )
            })
            .collect();
        let kv_h = |(k, v): &(Run, Run)| k.normal() + 0.8 * u + v.normal();
        let total = kvs.iter().map(kv_h).sum::<f32>() + 8.0 * u;
        let mut y = if portrait {
            right.y + 5.0 * u
        } else {
            right.y + (right.h - total) / 2.0
        };
        for kv in &kvs {
            let (k, v) = kv;
            self.text(k, inner.x, y, k.normal(), hex(0x7d84a6));
            self.text(v, inner.x, y + k.normal() + 0.8 * u, v.normal(), white(1.0));
            y += kv_h(kv) + 4.0 * u;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: [SlateStyle; 6] = [
        SlateStyle::Aurora,
        SlateStyle::Bento,
        SlateStyle::Gradient,
        SlateStyle::Frame,
        SlateStyle::Lines,
        SlateStyle::Split,
    ];

    fn slate(style: SlateStyle, channel: &str, width: u32, height: u32) -> Slate {
        Slate {
            style,
            image: String::new(),
            channel: channel.into(),
            width,
            height,
            rate: (50, 1),
        }
    }

    fn renderer() -> Renderer {
        Renderer::new(std::env::temp_dir().join("cg-slate-tests"))
    }

    fn render(r: &mut Renderer, s: &Slate, w: u32, h: u32) -> Frame {
        r.render_idle(
            s,
            OutputFormat {
                width: w,
                height: h,
            },
        )
    }

    fn alpha(f: &Frame, x: f32, y: f32) -> u8 {
        let (x, y) = (x as u32, y as u32);
        f.data[((y * f.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn labels_name_the_format() {
        assert_eq!(ratio_label(1920, 1080), "16:9");
        assert_eq!(ratio_label(1080, 1920), "9:16");
        assert_eq!(ratio_label(1080, 1080), "1:1");
        assert_eq!(ratio_label(1440, 1080), "4:3");
        assert_eq!(ratio_label(1366, 768), "16:9");
        assert_eq!(ratio_label(2560, 1080), "21:9");
        assert_eq!(ratio_label(4096, 2160), "17:9");
        assert_eq!(ratio_label(2048, 858), "2.39:1");
        assert_eq!(rate_label(50, 1), "50p");
        assert_eq!(rate_label(60000, 1001), "59.94p");
        assert_eq!(rate_label(24000, 1001), "23.98p");
        assert_eq!(rate_label(25, 1), "25p");
    }

    #[test]
    fn styles_read_as_lowercase_words() {
        assert_eq!(SlateStyle::default(), SlateStyle::Aurora);
        for (s, name) in STYLES
            .iter()
            .zip(["aurora", "bento", "gradient", "frame", "lines", "split"])
        {
            assert_eq!(serde_json::to_value(s).unwrap(), serde_json::json!(name));
            assert_eq!(
                serde_json::from_value::<SlateStyle>(serde_json::json!(name)).unwrap(),
                *s
            );
        }
        let custom: SlateStyle = serde_json::from_str("\"custom\"").unwrap();
        assert_eq!(custom, SlateStyle::Custom);
    }

    #[test]
    fn transparent_shows_nothing() {
        let style: SlateStyle = serde_json::from_str("\"transparent\"").unwrap();
        let f = render(
            &mut renderer(),
            &slate(style, "Channel 1", 1920, 1080),
            192,
            108,
        );
        assert_eq!((f.width, f.height), (192, 108));
        assert!(f.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn every_design_fills_the_picture_size() {
        let r = &mut renderer();
        for style in STYLES {
            for (w, h) in [(480, 270), (270, 480)] {
                let f = render(r, &slate(style, "Channel 1", w * 4, h * 4), w, h);
                assert_eq!(
                    (f.width, f.height, f.data.len()),
                    (w, h, (w * h * 4) as usize)
                );
                // Something white is written (the text).
                let bright = f
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[0] > 230 && p[1] > 230 && p[2] > 230)
                    .count();
                assert!(bright > 50, "{style:?} {w}×{h}: no text");
                // « Graphics », in the accent colour.
                let accent = f
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| {
                        p[3] == 255
                            && p[0].abs_diff(0x8f) < 4
                            && p[1].abs_diff(0xa3) < 4
                            && p[2] > 250
                    })
                    .count();
                assert!(accent > 20, "{style:?} {w}×{h}: no « Graphics »");
                let opaque = f.data.as_chunks::<4>().0.iter().all(|p| p[3] == 255);
                let see_through = matches!(style, SlateStyle::Frame | SlateStyle::Bento);
                assert_eq!(opaque, !see_through, "{style:?} {w}×{h}");
            }
        }
    }

    #[test]
    fn the_frame_is_transparent_but_its_marks_and_card() {
        let f = render(
            &mut renderer(),
            &slate(SlateStyle::Frame, "Channel 1", 1920, 1080),
            640,
            360,
        );
        let u = 3.6;
        // Between the marks and the card: nothing.
        assert_eq!(alpha(&f, 60.0, 180.0), 0);
        assert_eq!(alpha(&f, 320.0, 30.0), 0);
        // A corner mark (white, 85 %), the card (70 %).
        assert!(alpha(&f, 5.0 * u + 0.3 * u, 5.0 * u + 4.0 * u) > 200);
        assert!(alpha(&f, 640.0 - 5.0 * u - 4.0 * u, 360.0 - 5.0 * u - 0.3 * u) > 200);
        let card = alpha(&f, 320.0, 180.0 - 3.0 * u);
        assert!((170..=255).contains(&card), "card {card}");
    }

    #[test]
    fn the_bento_channel_tile_is_a_hole() {
        let r = &mut renderer();
        let f = render(
            r,
            &slate(SlateStyle::Bento, "Channel 1", 1920, 1080),
            640,
            360,
        );
        // Margin: a fifteenth of the height (24 px).
        let (m, u) = (24.0, 3.6);
        // The tile's top right inside: transparent; the page around it is not.
        assert_eq!(alpha(&f, 640.0 - m - 6.0 * u, m + 6.0 * u), 0);
        assert_eq!(alpha(&f, 640.0 - m / 2.0, m / 2.0), 255);
        // The tile's border is kept.
        let border = (0..(3.0 * u) as u32)
            .map(|d| alpha(&f, 640.0 - m - 6.0 * u, m + d as f32))
            .max()
            .unwrap();
        assert!(border > 0);
        // Portrait: the channel tile spans the width, under « Strata ».
        let f = render(
            r,
            &slate(SlateStyle::Bento, "Channel 1", 1080, 1920),
            360,
            640,
        );
        let m = 640.0 / 15.0;
        assert_eq!(alpha(&f, 360.0 - m - 6.0 * u, 640.0 * 0.55), 0);
        assert_eq!(alpha(&f, 360.0 - m / 2.0, 640.0 * 0.55), 255);
    }

    #[test]
    fn the_text_follows_the_channel() {
        let r = &mut renderer();
        for style in STYLES {
            let a = render(r, &slate(style, "Channel 1", 1920, 1080), 320, 180);
            let b = render(r, &slate(style, "Studio B", 1920, 1080), 320, 180);
            let c = render(r, &slate(style, "Channel 1", 1280, 720), 320, 180);
            assert!(a.data != b.data, "{style:?}: the name");
            assert!(a.data != c.data, "{style:?}: the size");
        }
    }

    #[test]
    fn custom_shows_the_picture_or_nothing() {
        let r = &mut renderer();
        let mut s = slate(SlateStyle::Custom, "Channel 1", 1920, 1080);
        let f = render(r, &s, 64, 36);
        assert!(f.data.iter().all(|v| *v == 0));
        s.image = "/assets/missing.png".into();
        let f = render(r, &s, 64, 36);
        assert!(f.data.iter().all(|v| *v == 0));
    }
}
